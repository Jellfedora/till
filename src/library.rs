//! "Accords" page: a searchable guitar chord dictionary.

use eframe::egui::{self, Align2, CornerRadius, FontId, RichText, Sense, vec2};

use crate::chords::{self, Chord, QUALITIES, root_name, root_name_fr, voicing};
use crate::diagram;
use crate::theme::ACCENT;
use crate::widgets::star_button;

/// What the chord page asks the app to do.
pub enum Action {
    Play(crate::chords::Voicing, crate::synth::Play),
    Check(String),
}

pub struct Library {
    pub root: u8,
    pub query: String,
    pub selected: Chord,
    /// Favorite chord names, in the order they were added.
    pub favorites: Vec<String>,
    action: Option<Action>,
}

impl Default for Library {
    fn default() -> Self {
        Library { root: 0, query: String::new(), selected: Chord::from_parts(0, ""), favorites: Vec::new(), action: None }
    }
}

impl Library {
    /// Selects a chord (e.g. clicked from the tab view) and shows its root family.
    pub fn select(&mut self, chord: &Chord) {
        self.selected = chord.clone();
        self.root = chord.root;
        self.query.clear();
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 20)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(RichText::new("Accords").strong());
                        ui.label(RichText::new("Dictionnaire guitare · accordage standard (E A D G B E)").weak());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Rechercher : Am7, Sol m, Ré#, C/B…")
                            .desired_width(260.0)
                            .margin(vec2(10.0, 6.0));
                        if ui.add(edit).changed() {
                            if let Some(c) = Chord::parse_loose(&self.query) {
                                self.selected = c;
                                self.root = self.selected.root;
                            }
                        }
                    });
                });
                if !self.query.trim().is_empty() && Chord::parse_loose(&self.query).is_none() {
                    ui.label(RichText::new("Accord inconnu").color(ui.visuals().warn_fg_color));
                }

                if !self.favorites.is_empty() {
                    ui.add_space(14.0);
                    ui.label(RichText::new("MES ACCORDS").size(10.5).weak().strong());
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 10.0);
                        let mut pick = None;
                        for name in &self.favorites {
                            let Some(chord) = Chord::parse(name) else { continue };
                            let v = voicing(&chord);
                            let selected = chord.name() == self.selected.name();
                            if diagram::card(ui, &chord.name(), &chord.name_fr(), v.as_ref(), 1.0, selected).clicked() {
                                pick = Some(chord);
                            }
                        }
                        if let Some(c) = pick {
                            self.select(&c);
                        }
                    });
                }

                ui.add_space(16.0);
                self.detail(ui);
                ui.add_space(22.0);
                self.roots(ui);
                ui.add_space(16.0);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 10.0);
                    for q in QUALITIES {
                        let chord = Chord::from_parts(self.root, q);
                        let selected = self.selected.root == chord.root
                            && self.selected.quality == chord.quality
                            && self.selected.bass.is_none();
                        let v = voicing(&chord);
                        if diagram::card(ui, &chord.name(), &chord.name_fr(), v.as_ref(), 1.0, selected).clicked() {
                            self.selected = chord;
                            self.query.clear();
                        }
                    }
                });
            });
        });
        self.action.take()
    }

    fn detail(&mut self, ui: &mut egui::Ui) {
        let c = self.selected.clone();
        let c = &c;
        let v = chords::voicing(c);
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    diagram::card(ui, "", "", v.as_ref(), 1.7, false);
                    ui.add_space(18.0);
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(c.name()).size(42.0).strong().color(ACCENT));
                            if star_button(ui, self.is_favorite(c)).clicked() {
                                self.toggle_favorite(c);
                            }
                        });
                        ui.label(RichText::new(c.name_fr()).size(20.0));
                        ui.add_space(14.0);
                        let notes = c.notes();
                        let en: Vec<_> = notes.iter().map(|n| n.0).collect();
                        let fr: Vec<_> = notes.iter().map(|n| n.1).collect();
                        fact(ui, "Notes", &format!("{}   ·   {}", en.join(" – "), fr.join(" – ")));
                        if let Some(v) = &v {
                            let frets: String = v
                                .frets
                                .iter()
                                .map(|f| match f {
                                    None => "x".to_string(),
                                    Some(f) if *f >= 10 => format!("({f})"),
                                    Some(f) => f.to_string(),
                                })
                                .collect();
                            fact(ui, "Doigté", &frets);
                            let fingers: String =
                                v.fingers.iter().map(|f| if *f == 0 { "–".into() } else { f.to_string() }).collect::<Vec<_>>().join(" ");
                            fact(ui, "Doigts", &fingers);
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                use crate::synth::Play;
                                if ui.button("Gratter").on_hover_text("Écouter l'accord gratté").clicked() {
                                    self.action = Some(Action::Play(v.clone(), Play::Down));
                                }
                                if ui.button("Note par note").on_hover_text("Écouter chaque corde l'une après l'autre").clicked() {
                                    self.action = Some(Action::Play(v.clone(), Play::Arpeggio));
                                }
                                if ui.button("Vérifier au micro").on_hover_text("Joue-le, Till te dit quelles cordes sonnent").clicked() {
                                    self.action = Some(Action::Check(c.name()));
                                }
                            });
                        }
                        if let Some((pc, _)) = c.bass {
                            fact(ui, "Basse", &format!("{} ({})", root_name(pc), root_name_fr(pc)));
                        }
                    });
                });
            });
    }

    pub fn is_favorite(&self, c: &Chord) -> bool {
        self.favorites.contains(&c.name())
    }

    pub fn toggle_favorite(&mut self, c: &Chord) {
        let name = c.name();
        if let Some(i) = self.favorites.iter().position(|n| *n == name) {
            self.favorites.remove(i);
        } else {
            self.favorites.push(name);
        }
    }

    fn roots(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for pc in 0..12u8 {
                let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 46.0), Sense::click());
                let selected = pc == self.root;
                let v = ui.visuals();
                let (fill, ink, sub) = if selected {
                    (ACCENT, v.selection.stroke.color, v.selection.stroke.color.gamma_multiply(0.75))
                } else if resp.hovered() {
                    (v.widgets.hovered.weak_bg_fill, v.strong_text_color(), v.weak_text_color())
                } else {
                    (v.widgets.inactive.bg_fill, v.text_color(), v.weak_text_color())
                };
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(10), fill);
                p.text(rect.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, root_name(pc), FontId::proportional(16.0), ink);
                p.text(rect.center() + vec2(0.0, 11.0), Align2::CENTER_CENTER, root_name_fr(pc), FontId::proportional(11.0), sub);
                if resp.clicked() {
                    self.root = pc;
                    self.selected = Chord::from_parts(pc, self.selected.quality);
                    self.query.clear();
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
    }
}

fn fact(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(64.0, 18.0), egui::Label::new(RichText::new(label).weak()));
        ui.label(RichText::new(value).monospace().size(14.0));
    });
}
