//! "Apprendre" page: beginner path, chord-change trainer, strumming patterns, chord
//! check with the microphone, and the practice journal.

pub mod check;
pub mod journal;
pub mod lessons;
pub mod rhythm;
pub mod trainer;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use eframe::egui::{self, RichText};

use crate::chords::{Chord, voicing};
use crate::mic::Mic;
use crate::synth::{Event, Play, Synth};

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Path,
    Changes,
    Strum,
    Check,
    Progress,
}

/// What the page asks the app to do.
pub enum Request {
    Tuner,
    Shelf,
    OpenSong { text: &'static str },
}

/// Beat clock driven by the frame time.
#[derive(Default)]
pub struct Clock {
    pub running: bool,
    pub beat: f32,
    last: Option<Instant>,
}

impl Clock {
    pub fn start(&mut self) {
        self.running = true;
        self.beat = -0.001;
        self.last = None;
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.beat = 0.0;
        self.last = None;
    }

    /// Advances by the time since the previous call; returns the beat interval covered.
    pub fn tick(&mut self, bpm: f32) -> Option<(f32, f32)> {
        if !self.running {
            return None;
        }
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
        self.last = Some(now);
        let from = self.beat;
        self.beat += dt * bpm / 60.0;
        Some((from, self.beat))
    }
}

/// Integer positions `k / per_beat` crossed in `from..to`, as k.
pub fn crossed(from: f32, to: f32, per_beat: f32) -> impl Iterator<Item = i64> {
    let a = (from * per_beat).floor() as i64 + 1;
    let b = (to * per_beat).floor() as i64;
    a..=b
}

pub fn click(synth: Option<&Synth>, accent: bool) {
    if let Some(s) = synth {
        s.send(Event::Click { accent });
    }
}

pub fn strum(synth: Option<&Synth>, chord: &str, how: Play, gain: f32) {
    if let (Some(s), Some(v)) = (synth, Chord::parse(chord).as_ref().and_then(voicing)) {
        s.chord(&v.frets, how, gain);
    }
}

pub struct Learn {
    pub tab: Tab,
    pub lesson: usize,
    pub done: BTreeSet<usize>,
    pub trainer: trainer::Trainer,
    pub rhythm: rhythm::Rhythm,
    pub check: check::Check,
    pub journal: journal::Journal,
    mic: Option<Mic>,
    mic_error: Option<String>,
}

impl Default for Learn {
    fn default() -> Self {
        Learn {
            tab: Tab::Path,
            lesson: 0,
            done: BTreeSet::new(),
            trainer: Default::default(),
            rhythm: Default::default(),
            check: Default::default(),
            journal: Default::default(),
            mic: None,
            mic_error: None,
        }
    }
}

impl Learn {
    pub fn load(get: &dyn Fn(&str) -> Option<String>) -> Learn {
        let mut l = Learn::default();
        if let Some(d) = get("lessons_done") {
            l.done = d.split(',').filter_map(|x| x.parse().ok()).collect();
        }
        l.lesson = get("lesson").and_then(|x| x.parse().ok()).unwrap_or(0).min(lessons::LESSONS.len() - 1);
        l.journal = journal::Journal::load(&get("journal").unwrap_or_default());
        l.trainer.records = parse_counts(&get("change_records").unwrap_or_default());
        l.check.clean = parse_counts(&get("chord_checks").unwrap_or_default());
        l
    }

    pub fn save(&self) -> Vec<(&'static str, String)> {
        vec![
            ("lessons_done", self.done.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",")),
            ("lesson", self.lesson.to_string()),
            ("journal", self.journal.save()),
            ("change_records", save_counts(&self.trainer.records)),
            ("chord_checks", save_counts(&self.check.clean)),
        ]
    }

    /// Something is being practiced right now (for the journal).
    pub fn practicing(&self) -> bool {
        self.trainer.running() || self.rhythm.clock.running || self.check.listening
    }

    /// Chords the player has shown they can play: clean checks or quick changes.
    pub fn mastered(&self) -> BTreeSet<String> {
        let mut m: BTreeSet<String> = self.check.clean.iter().filter(|(_, n)| **n >= 3).map(|(c, _)| c.clone()).collect();
        for (pair, &n) in &self.trainer.records {
            if n >= 30 {
                m.extend(pair.split(trainer::SEP).map(String::from));
            }
        }
        m
    }

    /// Stops sounds and the microphone when leaving the page.
    pub fn leave(&mut self) {
        self.trainer.stop();
        self.rhythm.clock.stop();
        self.check.listening = false;
        self.mic = None;
        self.mic_error = None;
    }

    pub fn open_check(&mut self, chord: &str) {
        self.tab = Tab::Check;
        self.check.chord = chord.to_string();
        self.check.reset();
    }

    fn need_mic(&mut self) {
        if self.mic.is_none() && self.mic_error.is_none() {
            match Mic::open() {
                Ok(m) => self.mic = Some(m),
                Err(e) => self.mic_error = Some(e),
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, synth: Option<&Synth>, favorites: &[String]) -> Option<Request> {
        let mut req = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 20)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.heading(RichText::new("Apprendre").strong());
                ui.label(RichText::new("Guitare débutant : leçons, exercices et suivi de tes progrès").weak());
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    for (t, label) in [
                        (Tab::Path, "Parcours"),
                        (Tab::Changes, "Changements d'accords"),
                        (Tab::Strum, "Grattage"),
                        (Tab::Check, "Vérifier un accord"),
                        (Tab::Progress, "Progrès"),
                    ] {
                        if ui.selectable_label(self.tab == t, RichText::new(label).size(14.0)).clicked() && self.tab != t {
                            self.trainer.stop();
                            self.rhythm.clock.stop();
                            self.check.listening = false;
                            self.tab = t;
                        }
                    }
                });
                ui.separator();
                ui.add_space(10.0);
                let uses_mic = (self.tab == Tab::Check) || (self.tab == Tab::Changes && self.trainer.use_mic);
                if uses_mic {
                    self.need_mic();
                } else {
                    self.mic = None;
                    self.mic_error = None;
                }
                if let (true, Some(e)) = (uses_mic, &self.mic_error) {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                    ui.label(RichText::new("Vérifie l'accès au micro dans Réglages Système › Confidentialité et sécurité › Micro.").weak());
                    if ui.button("Réessayer").clicked() {
                        self.mic_error = None;
                    }
                    ui.add_space(8.0);
                }
                let mic = self.mic.as_ref();
                match self.tab {
                    Tab::Path => {
                        if let Some(a) = lessons::show(ui, &mut self.lesson, &mut self.done, synth) {
                            req = self.lesson_action(a);
                        }
                    }
                    Tab::Changes => self.trainer.show(ui, synth, mic, favorites),
                    Tab::Strum => self.rhythm.show(ui, synth, favorites),
                    Tab::Check => self.check.show(ui, synth, mic, favorites),
                    Tab::Progress => self.progress(ui),
                }
            });
        });
        if self.practicing() || matches!(self.tab, Tab::Check | Tab::Changes | Tab::Strum) {
            ui.ctx().request_repaint();
        }
        req
    }

    fn lesson_action(&mut self, a: lessons::Action) -> Option<Request> {
        use lessons::Action::*;
        match a {
            Tuner => return Some(Request::Tuner),
            Shelf => return Some(Request::Shelf),
            Song(text) => return Some(Request::OpenSong { text }),
            Changes(chords) => {
                self.trainer.chords = chords.iter().map(|c| c.to_string()).collect();
                self.trainer.stop();
                self.tab = Tab::Changes;
            }
            Strum(p) => {
                self.rhythm.pattern = p;
                self.rhythm.clock.stop();
                self.tab = Tab::Strum;
            }
            Check(c) => self.open_check(c),
            Listen(_) => {}
        }
        None
    }

    fn progress(&mut self, ui: &mut egui::Ui) {
        self.journal.show(ui);
        ui.add_space(22.0);
        ui.label(RichText::new("PARCOURS").size(10.5).weak().strong());
        let n = lessons::LESSONS.len();
        ui.add(egui::ProgressBar::new(self.done.len() as f32 / n as f32).desired_width(360.0).text(format!("{} / {n} leçons", self.done.len())));
        ui.add_space(18.0);
        ui.label(RichText::new("ACCORDS MAÎTRISÉS").size(10.5).weak().strong());
        ui.label(
            RichText::new("Un accord est maîtrisé après 3 vérifications propres au micro, ou 30 changements en une minute.").weak(),
        );
        ui.add_space(4.0);
        let mastered = self.mastered();
        if mastered.is_empty() {
            ui.label(RichText::new("Pas encore : direction « Vérifier un accord » !").italics());
        } else {
            ui.horizontal_wrapped(|ui| {
                for c in &mastered {
                    crate::widgets::chip(ui, c, false);
                }
            });
        }
        ui.add_space(18.0);
        ui.label(RichText::new("RECORDS DE CHANGEMENTS (PAR MINUTE)").size(10.5).weak().strong());
        if self.trainer.records.is_empty() {
            ui.label(RichText::new("Aucun record : lance un défi d'une minute dans « Changements d'accords ».").italics());
        }
        let mut records: Vec<_> = self.trainer.records.iter().collect();
        records.sort_by(|a, b| b.1.cmp(a.1));
        for (pair, n) in records {
            ui.horizontal(|ui| {
                ui.label(RichText::new(pair.replace(trainer::SEP, " ↔ ")).monospace().strong());
                ui.label(RichText::new(format!("{n} changements")).weak());
            });
        }
    }
}

fn parse_counts(text: &str) -> BTreeMap<String, u32> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.rsplit_once(' ')?;
            Some((k.to_string(), v.parse().ok()?))
        })
        .collect()
}

fn save_counts(m: &BTreeMap<String, u32>) -> String {
    m.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join("\n")
}

/// Common beginner chords offered in pickers, before the player's favorites.
pub const COMMON: &[&str] = &[
    "Em", "Am", "E", "A", "D", "C", "G", "Dm", "E7", "A7", "D7", "G7", "C7", "B7", "Asus2", "Dsus2", "Cadd9", "Fmaj7", "F", "Bm7", "Bm",
];

/// Chord chips to pick from: common chords plus favorites.
pub fn picker_chords(favorites: &[String]) -> Vec<String> {
    let mut all: Vec<String> = COMMON.iter().map(|c| c.to_string()).collect();
    for f in favorites {
        if !all.contains(f) {
            all.push(f.clone());
        }
    }
    all
}
