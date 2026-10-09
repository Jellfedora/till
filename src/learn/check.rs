//! Chord check: strum a chord, Till tells which strings ring and which don't.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, vec2};

use super::{picker_chords, strum};
use crate::chords::{Chord, voicing};
use crate::diagram;
use crate::mic::{Attack, Mic, StringState, check_chord, rms};
use crate::synth::{Play, Synth};
use crate::theme::ACCENT;

const GREEN: Color32 = Color32::from_rgb(76, 190, 110);
const STRING_NAMES: [&str; 6] = ["Mi grave", "La", "Ré", "Sol", "Si", "Mi aigu"];
const TUNING: [u8; 6] = [40, 45, 50, 55, 59, 64];

pub struct Check {
    pub chord: String,
    pub listening: bool,
    /// Clean checks per chord.
    pub clean: BTreeMap<String, u32>,
    attack: Attack,
    heard_at: Option<Instant>,
    result: Option<[StringState; 6]>,
}

impl Default for Check {
    fn default() -> Self {
        Check { chord: "Em".into(), listening: false, clean: BTreeMap::new(), attack: Attack::default(), heard_at: None, result: None }
    }
}

impl Check {
    pub fn reset(&mut self) {
        self.result = None;
        self.heard_at = None;
        self.listening = true;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, synth: Option<&Synth>, mic: Option<&Mic>, favorites: &[String]) {
        let chord = Chord::parse(&self.chord);
        let shape = chord.as_ref().and_then(voicing);

        if let (true, Some(mic), Some(v)) = (self.listening, mic, &shape) {
            let level = rms(&mic.latest((mic.sr * 0.03) as usize));
            if self.attack.update(level) {
                self.heard_at = Some(Instant::now());
            }
            if self.heard_at.is_some_and(|t| t.elapsed() > Duration::from_millis(300)) {
                self.heard_at = None;
                let r = check_chord(&mic.latest(16_384), mic.sr, &v.frets, &TUNING);
                if !r.contains(&StringState::Missing) {
                    *self.clean.entry(self.chord.clone()).or_default() += 1;
                }
                self.result = Some(r);
            }
        }

        ui.label(RichText::new("Choisis un accord, place tes doigts, puis gratte-le une fois franchement. Till te dit quelles cordes sonnent.").size(14.5));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for c in picker_chords(favorites) {
                if crate::widgets::chip(ui, &c, c == self.chord).clicked() {
                    self.chord = c;
                    self.result = None;
                }
            }
        });
        ui.add_space(14.0);
        let Some(chord) = chord else { return };
        ui.horizontal_top(|ui| {
            diagram::card(ui, &chord.name(), &chord.name_fr(), shape.as_ref(), 1.9, false);
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if self.listening {
                        ui.spinner();
                        ui.label(RichText::new("J'écoute… gratte l'accord").size(17.0));
                        if ui.button("Pause").clicked() {
                            self.listening = false;
                        }
                    } else {
                        let b = egui::Button::new(RichText::new("Écouter mon accord").size(15.0).color(ui.visuals().selection.stroke.color))
                            .fill(ACCENT)
                            .min_size(vec2(0.0, 32.0));
                        if ui.add(b).clicked() {
                            self.reset();
                        }
                    }
                    if ui.button("Entendre le modèle").clicked() {
                        strum(synth, &self.chord, Play::Down, 1.0);
                        if let (Some(s), Some(v)) = (synth, &shape) {
                            s.chord_at(&v.frets, Play::Arpeggio, 0.9, 1600.0);
                        }
                    }
                });
                ui.add_space(12.0);
                let Some(v) = &shape else { return };
                match &self.result {
                    None => {
                        ui.label(RichText::new("Le résultat s'affichera ici, corde par corde.").weak());
                    }
                    Some(r) => {
                        let missing = r.iter().filter(|s| **s == StringState::Missing).count();
                        let (msg, col) = if missing == 0 {
                            ("Propre ! Toutes les cordes sonnent.", GREEN)
                        } else if missing == 1 {
                            ("Presque : une corde ne sonne pas.", ACCENT)
                        } else {
                            ("Plusieurs cordes ne sonnent pas.", ACCENT)
                        };
                        ui.label(RichText::new(msg).size(19.0).strong().color(col));
                        ui.add_space(6.0);
                        // From the thinnest string down, as on a tab.
                        for s in (0..6).rev() {
                            let (mark, text, col) = match (r[s], v.frets[s]) {
                                (StringState::Muted, _) => ("×", "ne pas jouer".to_string(), ui.visuals().weak_text_color()),
                                (StringState::Ok, Some(0)) => ("✔", "à vide, sonne bien".to_string(), GREEN),
                                (StringState::Ok, Some(f)) => ("✔", format!("case {f}, sonne bien"), GREEN),
                                (StringState::Missing, Some(0)) => ("✖", "à vide : un doigt la touche sans doute".to_string(), ACCENT),
                                (StringState::Missing, Some(f)) => ("✖", format!("case {f} : appuie plus fort, près de la frette"), ACCENT),
                                (_, None) => ("×", "ne pas jouer".to_string(), ui.visuals().weak_text_color()),
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(mark).size(16.0).strong().color(col));
                                ui.add_sized(vec2(130.0, 18.0), egui::Label::new(RichText::new(format!("Corde {} · {}", 6 - s, STRING_NAMES[s])).strong()));
                                ui.label(RichText::new(text).color(col));
                            });
                        }
                    }
                }
                ui.add_space(10.0);
                let n = self.clean.get(&self.chord).copied().unwrap_or(0);
                ui.label(RichText::new(format!("Vérifications propres pour {} : {n}{}", self.chord, if n >= 3 { " · maîtrisé ✔" } else { " (3 pour le maîtriser)" })).weak());
                ui.add_space(6.0);
                ui.label(
                    RichText::new("La détection peut se tromper (micro de l'ordi, bruit ambiant) : fie-toi aussi à ton oreille, corde par corde.")
                        .weak()
                        .italics(),
                );
            });
        });
    }
}
