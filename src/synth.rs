//! Plucked-string synthesizer (Karplus-Strong) and metronome, played through cpal.

use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub enum Event {
    /// Pluck `string` (0 = lowest) at `midi` pitch after `delay_ms` (used for strums).
    Pluck { string: u8, midi: u8, gain: f32, delay_ms: f32, bright: f32 },
    /// Pick noise brushing across the strings during a strum.
    Scratch { sweep_ms: f32, gain: f32 },
    Click { accent: bool },
    /// Quickly mutes every ringing string.
    Damp,
    Volume(f32),
}

pub struct Synth {
    tx: Sender<Event>,
    _stream: cpal::Stream,
}

impl Synth {
    /// Opens the default output device. `None` when no usable audio output exists.
    pub fn new() -> Option<Synth> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        if config.sample_format() != cpal::SampleFormat::F32 {
            return None;
        }
        let sr = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let (tx, rx) = channel();
        let mut engine = Engine::new(sr, rx);
        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| engine.fill(data, channels),
                |e| eprintln!("audio: {e}"),
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(Synth { tx, _stream: stream })
    }

    pub fn send(&self, e: Event) {
        let _ = self.tx.send(e);
    }

    /// Plays a chord shape (frets from low E, standard tuning) as a strum or an arpeggio.
    pub fn chord(&self, frets: &[Option<u8>; 6], how: Play, gain: f32) {
        self.chord_at(frets, how, gain, 0.0);
    }

    /// Like [`Synth::chord`], starting `start_ms` from now.
    pub fn chord_at(&self, frets: &[Option<u8>; 6], how: Play, gain: f32, start_ms: f32) {
        const TUNING: [u8; 6] = [40, 45, 50, 55, 59, 64];
        let mut notes: Vec<(u8, u8)> =
            frets.iter().enumerate().filter_map(|(s, f)| f.map(|f| (s as u8, TUNING[s] + f))).collect();
        let (step, bright, scratch) = match how {
            Play::Down => (11.0, 1.0, Some(0.09)),
            Play::Up => {
                notes.reverse();
                notes.truncate(4);
                (9.0, 1.0, Some(0.06))
            }
            Play::Arpeggio => (260.0, 0.0, None),
        };
        if let Some(g) = scratch {
            self.send(Event::Scratch { sweep_ms: step * notes.len() as f32, gain: g * gain });
        }
        for (k, (string, midi)) in notes.into_iter().enumerate() {
            let fade = if matches!(how, Play::Up) { 0.6 } else { 1.0 };
            self.send(Event::Pluck { string, midi, gain: 0.8 * gain * fade, delay_ms: start_ms + k as f32 * step, bright });
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Play {
    Down,
    Up,
    Arpeggio,
}

fn midi_to_hz(m: u8) -> f32 {
    440.0 * 2f32.powf((m as f32 - 69.0) / 12.0)
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

struct Voice {
    string: u8,
    buf: Vec<f32>,
    pos: usize,
    gain: f32,
    delay: u32,
    feedback: f32,
    age: u32,
    released: Option<u32>,
    /// Set on the sample the note starts sounding (after its delay).
    just_started: bool,
}

impl Voice {
    fn new(sr: f32, string: u8, midi: u8, gain: f32, delay_ms: f32, bright: f32, rng: &mut Rng) -> Voice {
        let n = (sr / midi_to_hz(midi)).round().max(2.0) as usize;
        // Noise burst excitation: low-passed for a soft finger pluck, rawer for a pick strum.
        let mut buf: Vec<f32> = (0..n).map(|_| rng.next()).collect();
        let passes = if bright > 0.5 { 1 } else { 2 };
        for _ in 0..passes {
            let mut prev = buf[n - 1];
            for x in buf.iter_mut() {
                *x = 0.5 * (*x + prev);
                prev = *x;
            }
        }
        let mean = buf.iter().sum::<f32>() / n as f32;
        buf.iter_mut().for_each(|x| *x -= mean);
        // Same loudness whatever the excitation filtering: normalize to a fixed RMS.
        let rms = (buf.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt().max(1e-6);
        buf.iter_mut().for_each(|x| *x *= 0.3 / rms);
        Voice {
            string,
            buf,
            pos: 0,
            gain,
            delay: (delay_ms / 1000.0 * sr) as u32,
            feedback: 0.996,
            age: 0,
            released: None,
            just_started: false,
        }
    }

    fn release(&mut self) {
        if self.released.is_none() {
            self.released = Some(self.age);
            self.feedback = 0.90;
        }
    }

    fn next(&mut self) -> f32 {
        if self.delay > 0 {
            self.delay -= 1;
            return 0.0;
        }
        self.just_started = self.age == 0;
        self.age += 1;
        let n = self.buf.len();
        let j = (self.pos + 1) % n;
        let out = self.buf[self.pos];
        self.buf[self.pos] = 0.5 * (self.buf[self.pos] + self.buf[j]) * self.feedback;
        self.pos = j;
        out * self.gain
    }

    fn done(&self, sr: f32) -> bool {
        let max_age = (sr * 7.0) as u32;
        self.age > max_age || self.released.is_some_and(|r| self.age - r > (sr * 0.15) as u32)
    }
}

/// Band-passed noise with a quick swell and fade: the pick rubbing the strings.
struct Scratch {
    len: u32,
    t: u32,
    gain: f32,
    prev: f32,
    lp: f32,
}

struct Click {
    phase: f32,
    freq: f32,
    t: u32,
}

struct Engine {
    sr: f32,
    rx: Receiver<Event>,
    voices: Vec<Voice>,
    clicks: Vec<Click>,
    scratches: Vec<Scratch>,
    volume: f32,
    rng: Rng,
    lp: f32,
}

impl Engine {
    fn new(sr: f32, rx: Receiver<Event>) -> Engine {
        Engine { sr, rx, voices: Vec::new(), clicks: Vec::new(), scratches: Vec::new(), volume: 0.8, rng: Rng(0x9E37_79B9), lp: 0.0 }
    }

    fn handle(&mut self, e: Event) {
        match e {
            Event::Pluck { string, midi, gain, delay_ms, bright } => {
                self.voices.push(Voice::new(self.sr, string, midi, gain, delay_ms, bright, &mut self.rng));
                if self.voices.len() > 48 {
                    self.voices.remove(0);
                }
            }
            Event::Scratch { sweep_ms, gain } => self.scratches.push(Scratch {
                len: (sweep_ms / 1000.0 * self.sr).max(1.0) as u32 + (self.sr * 0.015) as u32,
                t: 0,
                gain,
                prev: 0.0,
                lp: 0.0,
            }),
            Event::Click { accent } => self.clicks.push(Click { phase: 0.0, freq: if accent { 1760.0 } else { 1320.0 }, t: 0 }),
            Event::Damp => self.voices.iter_mut().for_each(Voice::release),
            Event::Volume(v) => self.volume = v,
        }
    }

    fn fill(&mut self, data: &mut [f32], channels: usize) {
        while let Ok(e) = self.rx.try_recv() {
            self.handle(e);
        }
        let click_len = (self.sr * 0.03) as u32;
        for frame in data.chunks_mut(channels) {
            let mut s: f32 = self.voices.iter_mut().map(Voice::next).sum();
            // A string only rings once: a note starting stops the previous one on that string.
            for i in 0..self.voices.len() {
                if self.voices[i].just_started {
                    self.voices[i].just_started = false;
                    let string = self.voices[i].string;
                    for (j, v) in self.voices.iter_mut().enumerate() {
                        if j != i && v.string == string && v.age > 1 {
                            v.release();
                        }
                    }
                }
            }
            // Gentle low-pass for a warmer, body-like tone.
            self.lp += 0.45 * (s - self.lp);
            s = self.lp * 0.4;
            for sc in self.scratches.iter_mut().filter(|c| c.t < c.len) {
                let x = self.rng.next();
                let hp = x - sc.prev;
                sc.prev = x;
                sc.lp += 0.35 * (hp - sc.lp);
                let p = sc.t as f32 / sc.len as f32;
                let env = (p * 6.0).min(1.0) * (1.0 - p).powi(2);
                s += sc.lp * env * sc.gain;
                sc.t += 1;
            }
            for c in self.clicks.iter_mut().filter(|c| c.t < click_len) {
                let env = 1.0 - c.t as f32 / click_len as f32;
                s += (c.phase * std::f32::consts::TAU).sin() * env * env * 0.25;
                c.phase = (c.phase + c.freq / self.sr).fract();
                c.t += 1;
            }
            let out = (s * self.volume).tanh();
            frame.iter_mut().for_each(|x| *x = out);
        }
        let sr = self.sr;
        self.voices.retain(|v| !v.done(sr));
        self.clicks.retain(|c| c.t < click_len);
        self.scratches.retain(|c| c.t < c.len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strum_renders_clean_audio() {
        let (tx, rx) = channel();
        let mut e = Engine::new(44_100.0, rx);
        tx.send(Event::Scratch { sweep_ms: 66.0, gain: 0.09 }).unwrap();
        for (i, (s, m)) in [(0u8, 43u8), (1, 47), (2, 50), (3, 55), (4, 59), (5, 67)].into_iter().enumerate() {
            tx.send(Event::Pluck { string: s, midi: m, gain: 0.7, delay_ms: i as f32 * 11.0, bright: 1.0 }).unwrap();
        }
        let mut buf = vec![0.0f32; 44_100];
        e.fill(&mut buf, 1);
        let peak = buf.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(buf.iter().all(|x| x.is_finite()));
        assert!(peak > 0.1 && peak < 0.95, "peak {peak}");
        // Still ringing after one second, but quieter than the attack.
        let tail = buf[40_000..].iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(tail > 0.005 && tail < peak, "tail {tail}");
    }
}
