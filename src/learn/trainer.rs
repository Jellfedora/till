//! Chord-change trainer: guided at a tempo, or the one-minute challenge.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, RichText, vec2};

use super::{Clock, click, crossed, picker_chords, strum};
use crate::chords::{Chord, voicing};
use crate::diagram;
use crate::mic::{Attack, Mic, matches, rms};
use crate::synth::{Play, Synth};
use crate::theme::ACCENT;

/// Separator between chord names in a record key.
pub const SEP: &str = "~";
const MINUTE: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Guided,
    Minute,
}

pub struct Trainer {
    pub chords: Vec<String>,
    pub mode: Mode,
    pub bpm: f32,
    pub beats_per_chord: u32,
    pub sound: bool,
    /// Minute challenge: count changes from the microphone instead of the space bar.
    pub use_mic: bool,
    pub records: BTreeMap<String, u32>,
    clock: Clock,
    started: Option<Instant>,
    current: usize,
    changes: u32,
    result: Option<(u32, bool)>,
    attack: Attack,
    heard_at: Option<Instant>,
}

impl Default for Trainer {
    fn default() -> Self {
        Trainer {
            chords: vec!["Em".into(), "Am".into()],
            mode: Mode::Minute,
            bpm: 60.0,
            beats_per_chord: 4,
            sound: true,
            use_mic: false,
            records: BTreeMap::new(),
            clock: Clock::default(),
            started: None,
            current: 0,
            changes: 0,
            result: None,
            attack: Attack::default(),
            heard_at: None,
        }
    }
}

impl Trainer {
    pub fn running(&self) -> bool {
        self.clock.running || self.started.is_some()
    }

    pub fn stop(&mut self) {
        self.clock.stop();
        self.started = None;
        self.heard_at = None;
    }

    fn key(&self) -> String {
        self.chords.join(SEP)
    }

    fn next(&mut self) {
        self.changes += 1;
        self.current = (self.current + 1) % self.chords.len().max(1);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, synth: Option<&Synth>, mic: Option<&Mic>, favorites: &[String]) {
        self.update(ui, synth, mic);

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mode, Mode::Minute, RichText::new("Défi d'une minute").size(14.0));
            ui.selectable_value(&mut self.mode, Mode::Guided, RichText::new("Guidé au tempo").size(14.0));
        });
        ui.label(
            RichText::new(match self.mode {
                Mode::Minute => "Change d'accord le plus de fois possible en 60 secondes. Appuie sur Espace à chaque changement réussi, ou laisse Till écouter au micro.",
                Mode::Guided => "Till affiche l'accord à jouer en rythme. Joue sur chaque temps ; monte le tempo quand c'est facile.",
            })
            .weak(),
        );
        ui.add_space(10.0);

        ui.label(RichText::new("ACCORDS (dans l'ordre de clic)").size(10.5).weak().strong());
        ui.horizontal_wrapped(|ui| {
            for c in picker_chords(favorites) {
                let pos = self.chords.iter().position(|x| *x == c);
                let label = match pos {
                    Some(i) => format!("{} {c}", i + 1),
                    None => c.clone(),
                };
                if crate::widgets::chip(ui, &label, pos.is_some()).clicked() && !self.running() {
                    match pos {
                        Some(i) => {
                            self.chords.remove(i);
                        }
                        None => self.chords.push(c),
                    }
                    self.result = None;
                }
            }
        });
        if self.chords.len() < 2 {
            ui.colored_label(ui.visuals().warn_fg_color, "Choisis au moins deux accords.");
            return;
        }
        ui.add_space(12.0);

        ui.horizontal_wrapped(|ui| {
            let label = if self.running() { "Arrêter" } else { "Démarrer" };
            let b = egui::Button::new(RichText::new(label).size(15.0).color(ui.visuals().selection.stroke.color))
                .fill(ACCENT)
                .min_size(vec2(110.0, 34.0));
            if ui.add(b).clicked() {
                if self.running() {
                    self.stop();
                } else {
                    self.start(synth);
                }
            }
            ui.add_space(10.0);
            match self.mode {
                Mode::Guided => {
                    ui.label("Tempo");
                    ui.add(egui::Slider::new(&mut self.bpm, 30.0..=140.0).step_by(1.0).suffix(" BPM"));
                    ui.label("Temps par accord");
                    for n in [4, 2, 1] {
                        ui.selectable_value(&mut self.beats_per_chord, n, n.to_string());
                    }
                    ui.toggle_value(&mut self.sound, "Son guide");
                }
                Mode::Minute => {
                    ui.toggle_value(&mut self.use_mic, "Compter au micro").on_hover_text(
                        "Till écoute et compte un changement quand il entend l'accord suivant. Si la détection se trompe, coupe-le et utilise Espace.",
                    );
                    if let Some(r) = self.records.get(&self.key()) {
                        ui.label(RichText::new(format!("Record : {r}")).strong());
                    }
                }
            }
        });
        ui.add_space(16.0);
        self.stage(ui);
    }

    fn start(&mut self, synth: Option<&Synth>) {
        self.current = 0;
        self.changes = 0;
        self.result = None;
        self.heard_at = None;
        match self.mode {
            Mode::Guided => self.clock.start(),
            Mode::Minute => {
                self.started = Some(Instant::now());
                click(synth, true);
            }
        }
    }

    fn update(&mut self, ui: &egui::Ui, synth: Option<&Synth>, mic: Option<&Mic>) {
        let n = self.chords.len().max(1);
        if let Some((from, to)) = self.clock.tick(self.bpm) {
            for b in crossed(from, to, 1.0) {
                let per = self.beats_per_chord as i64;
                let first = b % per == 0;
                click(synth, first);
                if first {
                    self.current = ((b / per) as usize) % n;
                    if self.sound {
                        strum(synth, &self.chords[self.current], Play::Down, 0.9);
                    }
                }
            }
        }
        let Some(t0) = self.started else { return };
        if t0.elapsed() >= MINUTE {
            let key = self.key();
            let best = self.records.get(&key).copied().unwrap_or(0);
            let record = self.changes > best;
            if record {
                self.records.insert(key, self.changes);
            }
            self.result = Some((self.changes, record));
            self.started = None;
            click(synth, true);
            return;
        }
        if !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Space)) {
            self.next();
        }
        if let (true, Some(mic)) = (self.use_mic, mic) {
            let level = rms(&mic.latest((mic.sr * 0.03) as usize));
            if self.attack.update(level) {
                self.heard_at = Some(Instant::now());
            }
            // Judge the strum once it has settled.
            if self.heard_at.is_some_and(|t| t.elapsed() > Duration::from_millis(250)) {
                self.heard_at = None;
                let target = &self.chords[(self.current + 1) % n];
                let midis = chord_midis(target);
                if matches(&mic.latest(8192), mic.sr, &midis) {
                    self.next();
                }
            }
        }
    }

    fn stage(&mut self, ui: &mut egui::Ui) {
        let n = self.chords.len();
        if let Some((count, record)) = self.result.filter(|_| !self.running()) {
            ui.label(RichText::new(format!("{count} changements en une minute")).size(26.0).strong().color(ACCENT));
            if record {
                ui.label(RichText::new("Nouveau record !").size(17.0).strong());
            }
            ui.label(
                RichText::new(if count >= 30 {
                    "Excellent : cet enchaînement est acquis. Essaie une autre paire !"
                } else if count >= 15 {
                    "Bien ! Refais le défi demain : ton score va encore monter."
                } else {
                    "C'est un début : fais ce défi chaque jour, les progrès viennent vite."
                })
                .weak(),
            );
            return;
        }
        let cur = self.chords[self.current.min(n - 1)].clone();
        let next = self.chords[(self.current + 1) % n].clone();
        ui.horizontal(|ui| {
            if let Some(c) = Chord::parse(&cur) {
                let v = voicing(&c);
                diagram::card(ui, &c.name(), &c.name_fr(), v.as_ref(), 2.0, true);
            }
            ui.add_space(24.0);
            ui.vertical(|ui| {
                if let Some(t0) = self.started {
                    let left = MINUTE.saturating_sub(t0.elapsed()).as_secs();
                    ui.label(RichText::new(format!("{left} s")).size(46.0).strong());
                    ui.label(RichText::new(format!("{} changements", self.changes)).size(20.0).color(ACCENT));
                    ui.add_space(8.0);
                    if ui.button(RichText::new("Changé ! (Espace)").size(15.0)).clicked() {
                        self.next();
                    }
                } else if self.clock.running {
                    let per = self.beats_per_chord as f32;
                    let beat = (self.clock.beat.max(0.0) % per).floor() as u32 + 1;
                    ui.label(RichText::new(format!("Temps {beat} / {}", self.beats_per_chord)).size(24.0).strong());
                } else {
                    ui.label(RichText::new("Prêt ?").size(24.0).strong());
                }
                ui.add_space(14.0);
                ui.label(RichText::new("Ensuite").weak());
                if let Some(c) = Chord::parse(&next) {
                    let v = voicing(&c);
                    diagram::card(ui, &c.name(), &c.name_fr(), v.as_ref(), 1.0, false);
                }
            });
        });
    }
}

/// Sounding MIDI notes of a chord's usual voicing.
pub fn chord_midis(name: &str) -> Vec<u8> {
    const TUNING: [u8; 6] = [40, 45, 50, 55, 59, 64];
    Chord::parse(name)
        .as_ref()
        .and_then(voicing)
        .map(|v| v.frets.iter().enumerate().filter_map(|(s, f)| f.map(|f| TUNING[s] + f)).collect())
        .unwrap_or_default()
}
