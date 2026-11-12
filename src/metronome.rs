//! "Métronome" page: a standalone metronome. The clicks are scheduled by the audio
//! engine (sample-accurate, keeps going when the window is hidden); this page only
//! sets it up and shows the beat.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, RichText, Sense, Stroke, pos2, vec2};

use crate::chords::{Chord, voicing};
use crate::diagram;
use crate::synth::{Event, Metro, Synth};
use crate::theme::ACCENT;

const BPM_MIN: f32 = 30.0;
const BPM_MAX: f32 = 250.0;
const SUBDIVS: &[(u32, &str)] = &[(1, "Noires"), (2, "Croches"), (3, "Triolets"), (4, "Doubles croches")];

pub struct Metronome {
    pub bpm: f32,
    pub beats: u32,
    pub subdiv: u32,
    pub accent: bool,
    /// Chord sequence to play along, one chord per bar, in the player's order.
    pub chords: Vec<String>,
    running: bool,
    /// Settings last sent to the engine.
    sent: Option<Metro>,
    /// Tap tempo: times of the latest taps.
    taps: Vec<Instant>,
    /// Last tick count seen, and when it changed (for the flash).
    seen: u64,
    flash_at: Option<Instant>,
}

impl Default for Metronome {
    fn default() -> Self {
        Metronome {
            bpm: 80.0,
            beats: 4,
            subdiv: 1,
            accent: true,
            chords: Vec::new(),
            running: false,
            sent: None,
            taps: Vec::new(),
            seen: 0,
            flash_at: None,
        }
    }
}

impl Metronome {
    pub fn load(get: &dyn Fn(&str) -> Option<String>) -> Metronome {
        let mut m = Metronome::default();
        m.bpm = get("metro_bpm").and_then(|x| x.parse().ok()).unwrap_or(80.0f32).clamp(BPM_MIN, BPM_MAX);
        m.beats = get("metro_beats").and_then(|x| x.parse().ok()).unwrap_or(4u32).clamp(1, 12);
        m.subdiv = get("metro_subdiv").and_then(|x| x.parse().ok()).filter(|s| (1..=4).contains(s)).unwrap_or(1);
        m.accent = get("metro_accent").is_none_or(|v| v == "true");
        m.chords = get("metro_chords").unwrap_or_default().split_whitespace().filter(|c| Chord::parse(c).is_some()).map(String::from).collect();
        m
    }

    pub fn save(&self) -> Vec<(&'static str, String)> {
        vec![
            ("metro_bpm", self.bpm.to_string()),
            ("metro_beats", self.beats.to_string()),
            ("metro_subdiv", self.subdiv.to_string()),
            ("metro_accent", self.accent.to_string()),
            ("metro_chords", self.chords.join(" ")),
        ]
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn stop(&mut self, synth: Option<&Synth>) {
        if self.running || self.sent.is_some() {
            if let Some(s) = synth {
                s.send(Event::Metronome(None));
            }
        }
        self.running = false;
        self.sent = None;
        self.seen = 0;
        self.flash_at = None;
    }

    fn toggle(&mut self, synth: Option<&Synth>) {
        if self.running {
            self.stop(synth);
        } else {
            self.running = true;
        }
    }

    fn settings(&self) -> Metro {
        Metro { bpm: self.bpm.round(), beats: self.beats, subdiv: self.subdiv, accent: self.accent }
    }

    /// Keeps the engine in step with the page.
    fn sync(&mut self, synth: Option<&Synth>) {
        let Some(s) = synth else { return };
        if self.running && self.sent != Some(self.settings()) {
            s.send(Event::Metronome(Some(self.settings())));
            self.sent = Some(self.settings());
        }
        let ticks = if self.running { s.metro_ticks() } else { 0 };
        if ticks != self.seen {
            self.seen = ticks;
            self.flash_at = Some(Instant::now());
        }
    }

    fn tap(&mut self) {
        let now = Instant::now();
        if self.taps.last().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2)) {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > 6 {
            self.taps.remove(0);
        }
        if self.taps.len() >= 2 {
            let span = now.duration_since(self.taps[0]).as_secs_f32();
            let bpm = 60.0 * (self.taps.len() - 1) as f32 / span;
            self.bpm = bpm.round().clamp(BPM_MIN, BPM_MAX);
        }
    }

    fn nudge(&mut self, d: f32) {
        self.bpm = (self.bpm.round() + d).clamp(BPM_MIN, BPM_MAX);
    }

    fn keys(&mut self, ui: &egui::Ui, synth: Option<&Synth>) {
        if ui.ctx().wants_keyboard_input() {
            return;
        }
        let key = |k| ui.ctx().input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if key(Key::Space) {
            self.toggle(synth);
        }
        if key(Key::T) {
            self.tap();
        }
        if key(Key::ArrowUp) {
            self.nudge(1.0);
        }
        if key(Key::ArrowDown) {
            self.nudge(-1.0);
        }
        if key(Key::ArrowRight) {
            self.nudge(5.0);
        }
        if key(Key::ArrowLeft) {
            self.nudge(-5.0);
        }
        if key(Key::Escape) {
            self.stop(synth);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, synth: Option<&Synth>, favorites: &[String]) {
        self.keys(ui, synth);
        self.sync(synth);
        if self.running {
            ui.ctx().request_repaint_after(Duration::from_millis(15));
        }

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 20)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.heading(RichText::new("Métronome").strong());
                ui.label(RichText::new("Espace : démarrer / arrêter · T : taper le tempo · ↑↓ ±1 · ←→ ±5").weak());
                if synth.is_none() {
                    ui.add_space(8.0);
                    ui.colored_label(ui.visuals().error_fg_color, "Aucune sortie audio disponible : le métronome ne peut pas sonner.");
                }
                ui.add_space(18.0);

                self.beat_lights(ui);
                ui.add_space(18.0);

                // Tempo.
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("−").size(22.0)).min_size(vec2(44.0, 44.0))).clicked() {
                        self.nudge(-1.0);
                    }
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{:.0}", self.bpm)).size(64.0).strong().color(ACCENT));
                        ui.label(RichText::new(format!("BPM · {}", tempo_name(self.bpm))).weak());
                    });
                    ui.add_space(8.0);
                    if ui.add(egui::Button::new(RichText::new("+").size(22.0)).min_size(vec2(44.0, 44.0))).clicked() {
                        self.nudge(1.0);
                    }
                });
                ui.add_space(8.0);
                ui.spacing_mut().slider_width = 420.0;
                ui.add(egui::Slider::new(&mut self.bpm, BPM_MIN..=BPM_MAX).step_by(1.0).show_value(false));
                ui.add_space(14.0);

                ui.horizontal(|ui| {
                    let label = if self.running { "Stop" } else { "Démarrer" };
                    let fg = ui.visuals().selection.stroke.color;
                    let b = egui::Button::new(RichText::new(label).size(16.0).color(fg)).fill(ACCENT).min_size(vec2(130.0, 38.0));
                    if ui.add(b).clicked() {
                        self.toggle(synth);
                    }
                    ui.add_space(8.0);
                    let tap = egui::Button::new(RichText::new("Taper le tempo").size(15.0)).min_size(vec2(0.0, 38.0));
                    if ui.add(tap).on_hover_text("Clique en rythme (ou touche T) : le tempo suit").clicked() {
                        self.tap();
                    }
                });
                ui.add_space(22.0);

                ui.label(RichText::new("TEMPS PAR MESURE").size(10.5).weak().strong());
                ui.horizontal_wrapped(|ui| {
                    for n in 1..=12 {
                        if crate::widgets::chip(ui, &n.to_string(), self.beats == n).clicked() {
                            self.beats = n;
                        }
                    }
                });
                ui.add_space(12.0);
                ui.label(RichText::new("SUBDIVISION").size(10.5).weak().strong());
                ui.horizontal_wrapped(|ui| {
                    for &(n, name) in SUBDIVS {
                        if crate::widgets::chip(ui, name, self.subdiv == n).clicked() {
                            self.subdiv = n;
                        }
                    }
                });
                ui.add_space(12.0);
                ui.checkbox(&mut self.accent, "Accentuer le premier temps");
                ui.add_space(26.0);
                self.sequence(ui, favorites);
            });
        });
        // Settings changed this frame reach the engine right away.
        self.sync(synth);
    }

    /// Bar being played (0 = first), while running.
    fn bar(&self) -> Option<usize> {
        let t = self.seen.checked_sub(1)?;
        Some((t / self.subdiv.max(1) as u64 / self.beats.max(1) as u64) as usize)
    }

    /// The chord sequence: add chords, drag them to reorder, × to remove.
    fn sequence(&mut self, ui: &mut egui::Ui, favorites: &[String]) {
        ui.label(RichText::new("SUITE D'ACCORDS").size(10.5).weak().strong());
        ui.label(RichText::new("Un accord par mesure, dans l'ordre que tu veux · glisse une carte pour la déplacer · × pour la retirer").weak());
        ui.add_space(6.0);
        let current = self.bar().filter(|_| !self.chords.is_empty()).map(|b| b % self.chords.len());
        let (mut del, mut moved) = (None, None);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
            for (k, name) in self.chords.iter().enumerate() {
                let Some(c) = Chord::parse(name) else { continue };
                let card = ui
                    .dnd_drag_source(egui::Id::new(("metro_chord", k)), k, |ui| {
                        diagram::card(ui, name, &c.name_fr(), voicing(&c).as_ref(), 1.0, current == Some(k));
                    })
                    .response;
                // Dropped on this card: the dragged one takes its place.
                if let Some(from) = card.dnd_release_payload::<usize>() {
                    moved = Some((*from, k));
                }
                if card.dnd_hover_payload::<usize>().is_some_and(|from| *from != k) {
                    let x = card.rect.left() - 3.0;
                    ui.painter().line_segment([pos2(x, card.rect.top()), pos2(x, card.rect.bottom())], Stroke::new(3.0_f32, ACCENT));
                }
                if ui.rect_contains_pointer(card.rect) && !card.dragged() {
                    let r = egui::Rect::from_center_size(card.rect.right_top() + vec2(-11.0, 11.0), vec2(18.0, 18.0));
                    let x = ui.interact(r, egui::Id::new(("metro_chord_del", k)), Sense::click()).on_hover_text("Retirer cet accord");
                    let v = ui.visuals();
                    let fill = if x.hovered() { v.error_fg_color } else { v.widgets.inactive.bg_fill };
                    ui.painter().circle_filled(r.center(), 9.0, fill);
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "×", FontId::proportional(14.0), v.strong_text_color());
                    if x.clicked() {
                        del = Some(k);
                    }
                }
            }
        });
        if let Some((from, to)) = moved {
            let c = self.chords.remove(from);
            self.chords.insert(to, c);
        }
        if let Some(k) = del {
            self.chords.remove(k);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Ajouter :").weak());
            if !self.chords.is_empty() && ui.small_button("Tout effacer").clicked() {
                self.chords.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            for c in crate::learn::picker_chords(favorites) {
                if crate::widgets::chip(ui, &c, false).on_hover_text("Ajouter à la fin de la suite").clicked() {
                    self.chords.push(c);
                }
            }
        });
    }

    /// One light per beat; the current one flashes on each click.
    fn beat_lights(&self, ui: &mut egui::Ui) {
        let n = self.beats as usize;
        let r = if n > 8 { 16.0 } else { 22.0 };
        let gap = 14.0;
        let w = n as f32 * (2.0 * r + gap) - gap;
        let (rect, _) = ui.allocate_exact_size(vec2(w.max(1.0), 2.0 * r + 22.0), Sense::hover());
        let v = ui.visuals();
        let painter = ui.painter();
        let sub = self.subdiv.max(1) as u64;
        // `seen` counts ticks played; the latest one is `seen - 1`.
        let current = self.seen.checked_sub(1).map(|t| ((t / sub) % self.beats.max(1) as u64) as usize);
        let on_beat = self.seen.checked_sub(1).is_some_and(|t| t % sub == 0);
        let fade = self.flash_at.map_or(0.0, |t| 1.0 - (t.elapsed().as_secs_f32() / 0.25).min(1.0));
        for i in 0..n {
            let c = pos2(rect.left() + r + i as f32 * (2.0 * r + gap), rect.top() + r);
            let accent = i == 0 && self.accent;
            let base = if accent { ACCENT.gamma_multiply(0.35) } else { v.widgets.inactive.bg_fill };
            let fill = if current == Some(i) {
                let k = if on_beat { 0.35 + 0.65 * fade } else { 0.35 };
                lerp(base, if accent { ACCENT } else { v.strong_text_color() }, k)
            } else {
                base
            };
            painter.circle(c, r, fill, Stroke::new(1.0_f32, v.widgets.inactive.bg_stroke.color));
            painter.text(pos2(c.x, rect.bottom() - 8.0), Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(12.0), v.weak_text_color());
        }
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Italian tempo marking for a BPM.
fn tempo_name(bpm: f32) -> &'static str {
    match bpm as u32 {
        0..=44 => "Grave",
        45..=59 => "Largo",
        60..=72 => "Adagio",
        73..=99 => "Andante",
        100..=119 => "Moderato",
        120..=167 => "Allegro",
        168..=199 => "Presto",
        _ => "Prestissimo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_names() {
        assert_eq!(tempo_name(40.0), "Grave");
        assert_eq!(tempo_name(80.0), "Andante");
        assert_eq!(tempo_name(130.0), "Allegro");
        assert_eq!(tempo_name(240.0), "Prestissimo");
    }
}
