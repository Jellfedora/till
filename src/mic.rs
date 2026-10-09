//! Microphone input and listening helpers shared by the tuner, the chord checker and
//! the "wait for the note" reading mode.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct Mic {
    _stream: cpal::Stream,
    buf: Arc<Mutex<VecDeque<f32>>>,
    pub sr: f32,
    pub device: String,
}

impl Mic {
    /// Opens the default input; keeps the last ~1.5 s of mono samples.
    pub fn open() -> Result<Mic, String> {
        let device = cpal::default_host().default_input_device().ok_or("Aucun micro trouvé.")?;
        let config = device.default_input_config().map_err(|e| format!("Micro inutilisable : {e}"))?;
        let sr = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let cap = (sr * 1.5) as usize;
        let buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(cap)));
        fn push(buf: &Mutex<VecDeque<f32>>, cap: usize, channels: usize, data: impl Iterator<Item = f32>) {
            let mut b = buf.lock().unwrap();
            let mut acc = 0.0;
            for (i, x) in data.enumerate() {
                acc += x;
                if (i + 1) % channels == 0 {
                    b.push_back(acc / channels as f32);
                    acc = 0.0;
                }
            }
            let extra = b.len().saturating_sub(cap);
            b.drain(..extra);
        }
        let err = |e| eprintln!("micro : {e}");
        let b = buf.clone();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |d: &[f32], _| push(&b, cap, channels, d.iter().copied()),
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |d: &[i16], _| push(&b, cap, channels, d.iter().map(|&x| x as f32 / 32768.0)),
                err,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |d: &[u16], _| push(&b, cap, channels, d.iter().map(|&x| (x as f32 - 32768.0) / 32768.0)),
                err,
                None,
            ),
            f => return Err(format!("Format micro non géré : {f:?}")),
        }
        .map_err(|e| format!("Impossible d'ouvrir le micro : {e}"))?;
        stream.play().map_err(|e| format!("Impossible de démarrer le micro : {e}"))?;
        Ok(Mic { _stream: stream, buf, sr, device: device.name().unwrap_or_else(|_| "Micro".into()) })
    }

    /// The most recent `n` samples (fewer at start-up).
    pub fn latest(&self, n: usize) -> Vec<f32> {
        let b = self.buf.lock().unwrap();
        let n = n.min(b.len());
        b.iter().skip(b.len() - n).copied().collect()
    }
}

pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// Detects a fresh pluck or strum: the level jumps well above what it was just before.
#[derive(Default)]
pub struct Attack {
    floor: f32,
    armed: bool,
}

impl Attack {
    /// Feed the level of the last ~50 ms; true once per attack.
    pub fn update(&mut self, level: f32) -> bool {
        let hit = self.armed && level > 0.01 && level > self.floor * 2.2;
        // Slow-moving floor: follows decays quickly, rises slowly.
        self.floor = if level < self.floor { level } else { self.floor + 0.08 * (level - self.floor) };
        self.armed = true;
        if hit {
            self.floor = level;
        }
        hit
    }
}

/// In-place radix-2 FFT (`re.len()` must be a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// Magnitude spectrum of a Hann-windowed frame, zero-padded to a power of two.
pub struct Spectrum {
    mag: Vec<f32>,
    bin_hz: f32,
}

impl Spectrum {
    pub fn new(x: &[f32], sr: f32) -> Spectrum {
        let n = x.len().next_power_of_two().max(1024);
        let mut re = vec![0.0f32; n];
        let mut im = vec![0.0f32; n];
        let len = x.len();
        for (i, v) in x.iter().enumerate() {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / len as f32).cos();
            re[i] = v * w;
        }
        fft(&mut re, &mut im);
        let mag = (0..n / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        Spectrum { mag, bin_hz: sr / n as f32 }
    }

    /// Peak magnitude within a quarter tone of `hz`.
    pub fn around(&self, hz: f32) -> f32 {
        let q = 2f32.powf(1.0 / 24.0);
        let lo = ((hz / q) / self.bin_hz).floor() as usize;
        let hi = (((hz * q) / self.bin_hz).ceil() as usize).min(self.mag.len().saturating_sub(1));
        self.mag.get(lo..=hi).map_or(0.0, |s| s.iter().copied().fold(0.0, f32::max))
    }

    /// Strength of a note. Low notes may also count their 2nd and 3rd harmonics, since
    /// small mics barely pick up a low E fundamental — unless another expected note sits
    /// there and would fake it.
    pub fn note(&self, midi: u8, others: &[u8]) -> f32 {
        let f = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
        let mut v = self.around(f);
        if midi < 50 {
            for (h, up, w) in [(2.0, 12u8, 0.8), (3.0, 19, 0.6)] {
                if !others.contains(&(midi + up)) {
                    v = v.max(self.around(h * f) * w);
                }
            }
        }
        v
    }

    /// Energy per pitch class between 70 Hz and 1.4 kHz, normalized to max 1.
    pub fn chroma(&self) -> [f32; 12] {
        let mut c = [0.0f32; 12];
        for midi in 38u8..=88 {
            let f = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
            c[(midi % 12) as usize] += self.around(f);
        }
        let max = c.iter().copied().fold(0.0, f32::max).max(1e-9);
        c.map(|v| v / max)
    }
}

/// How each string of a chord sounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StringState {
    Ok,
    /// Expected but too quiet: muted by a finger or not pressed hard enough.
    Missing,
    /// Should not ring (marked x) — not checked, just shown.
    Muted,
}

/// Checks a strummed chord: one state per string (index 0 = low E) for the given frets.
pub fn check_chord(x: &[f32], sr: f32, frets: &[Option<u8>; 6], tuning: &[u8; 6]) -> [StringState; 6] {
    let sp = Spectrum::new(x, sr);
    let notes: Vec<(usize, u8)> = frets.iter().enumerate().filter_map(|(s, f)| f.map(|f| (s, tuning[s] + f))).collect();
    let all: Vec<u8> = notes.iter().map(|n| n.1).collect();
    let strengths: Vec<f32> = notes.iter().map(|&(_, m)| sp.note(m, &all)).collect();
    let loudest = strengths.iter().copied().fold(0.0, f32::max).max(1e-9);
    let mut out = [StringState::Muted; 6];
    for (&(s, _), &v) in notes.iter().zip(&strengths) {
        out[s] = if v / loudest > 0.12 { StringState::Ok } else { StringState::Missing };
    }
    out
}

/// Does what was heard match these pitches? Single notes compare pitch (octave errors
/// forgiven); chords compare pitch-class content.
pub fn matches(x: &[f32], sr: f32, midis: &[u8]) -> bool {
    if midis.is_empty() || rms(x) < 0.005 {
        return false;
    }
    if midis.len() == 1 {
        let Some(f) = crate::tuner::yin(x, sr) else { return false };
        let heard = 69.0 + 12.0 * (f / 440.0).log2();
        let d = (heard - midis[0] as f32).abs();
        return d < 0.6 || (d - 12.0).abs() < 0.6 || (d - 24.0).abs() < 0.6;
    }
    let chroma = Spectrum::new(x, sr).chroma();
    let mut pcs: Vec<u8> = midis.iter().map(|m| m % 12).collect();
    pcs.sort_unstable();
    pcs.dedup();
    let present = pcs.iter().filter(|&&pc| chroma[pc as usize] > 0.25).count();
    present * 3 >= pcs.len() * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(midi: u8, sr: f32, n: usize) -> Vec<f32> {
        let f = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                (1..=5).map(|h| (std::f32::consts::TAU * f * h as f32 * t).sin() / h as f32).sum::<f32>() * (-t * 2.0).exp()
            })
            .collect()
    }

    #[test]
    fn checks_strummed_chords() {
        let sr = 48_000.0;
        let n = 16_384;
        let tuning = [40, 45, 50, 55, 59, 64];
        // G major 320003, with the A string (B2) muted by a stray finger.
        let frets = [Some(3), Some(2), Some(0), Some(0), Some(0), Some(3)];
        let mut mix = vec![0.0f32; n];
        for (s, f) in frets.iter().enumerate() {
            let Some(f) = f else { continue };
            if s == 1 {
                continue;
            }
            for (m, v) in mix.iter_mut().zip(string(tuning[s] + f, sr, n)) {
                *m += v * 0.2;
            }
        }
        let got = check_chord(&mix, sr, &frets, &tuning);
        use StringState::*;
        assert_eq!(got, [Ok, Missing, Ok, Ok, Ok, Ok]);
        let c = [None, Some(3), Some(2), Some(0), Some(1), Some(0)];
        assert_eq!(check_chord(&mix, sr, &c, &tuning)[0], Muted);
        assert!(matches(&mix, sr, &[43, 50, 55, 59, 67]));
        assert!(!matches(&mix, sr, &[49, 53, 56]));
        // A single note matches its pitch only.
        let e = string(52, sr, 4096);
        assert!(matches(&e, sr, &[52]));
        assert!(!matches(&e, sr, &[53]));
    }
}
