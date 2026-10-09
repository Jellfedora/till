//! Strumming patterns: arrows light up in time, with optional strummed sound.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, pos2, vec2};

use super::{Clock, click, crossed, picker_chords, strum};
use crate::synth::{Play, Synth};
use crate::theme::ACCENT;

pub struct Pattern {
    pub name: &'static str,
    pub hint: &'static str,
    /// One slot per eighth note: 'D' down, 'U' up, '.' hand moves without touching.
    pub slots: &'static str,
    pub beats: usize,
}

pub const PATTERNS: &[Pattern] = &[
    Pattern { name: "4 temps", hint: "Un coup vers le bas sur chaque temps. La base de tout.", slots: "D.D.D.D.", beats: 4 },
    Pattern { name: "Croches", hint: "Bas sur le temps, haut entre les temps : la main ne s'arrête jamais.", slots: "DUDUDUDU", beats: 4 },
    Pattern { name: "Ballade", hint: "Trois temps posés, puis un aller-retour pour relancer.", slots: "D.D.D.DU", beats: 4 },
    Pattern { name: "Pop / folk", hint: "↓ ↓↑ ↑↓↑ — le temps 3 est « dans le vide » : la main descend sans toucher.", slots: "D.DU.UDU", beats: 4 },
    Pattern { name: "Rock", hint: "Bas-bas, haut, bas-bas, haut : énergique, avec des accents.", slots: "DD.UDD.U", beats: 4 },
    Pattern { name: "Contretemps (reggae)", hint: "Seulement entre les temps : la main gratte sur le « et ».", slots: ".D.D.D.D", beats: 4 },
    Pattern { name: "Valse (3 temps)", hint: "Un fort, deux légers : 1 2 3, 1 2 3.", slots: "D.DUDU", beats: 3 },
];

pub struct Rhythm {
    pub pattern: usize,
    pub bpm: f32,
    pub chord: String,
    pub sound: bool,
    pub metronome: bool,
    pub clock: Clock,
}

impl Default for Rhythm {
    fn default() -> Self {
        Rhythm { pattern: 0, bpm: 70.0, chord: "G".into(), sound: true, metronome: true, clock: Clock::default() }
    }
}

impl Rhythm {
    pub fn show(&mut self, ui: &mut egui::Ui, synth: Option<&Synth>, favorites: &[String]) {
        let p = &PATTERNS[self.pattern.min(PATTERNS.len() - 1)];
        if let Some((from, to)) = self.clock.tick(self.bpm) {
            for k in crossed(from, to, 2.0) {
                let slot = (k as usize) % p.slots.len();
                if k % 2 == 0 && self.metronome {
                    click(synth, slot == 0);
                }
                if self.sound {
                    match p.slots.as_bytes()[slot] {
                        b'D' => strum(synth, &self.chord, Play::Down, if slot == 0 { 1.0 } else { 0.8 }),
                        b'U' => strum(synth, &self.chord, Play::Up, 0.8),
                        _ => {}
                    }
                }
            }
        }

        ui.horizontal_wrapped(|ui| {
            for (i, pat) in PATTERNS.iter().enumerate() {
                if ui.selectable_label(self.pattern == i, RichText::new(pat.name).size(14.0)).clicked() {
                    self.pattern = i;
                    self.clock.stop();
                }
            }
        });
        ui.add_space(6.0);
        let p = &PATTERNS[self.pattern];
        ui.label(RichText::new(p.hint).size(14.5));
        ui.label(RichText::new(format!("Mesure à {} temps · compte à voix haute en jouant", p.beats)).weak());
        ui.add_space(14.0);

        // The pattern: one arrow per eighth, counts underneath.
        let n = p.slots.len();
        let slot_w = 74.0;
        let (rect, _) = ui.allocate_exact_size(vec2(slot_w * n as f32 + 24.0, 170.0), Sense::hover());
        let v = ui.visuals();
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(16), v.faint_bg_color);
        let now = self.clock.running.then(|| ((self.clock.beat.max(0.0) * 2.0) as usize) % n);
        for (i, ch) in p.slots.chars().enumerate() {
            let c = pos2(rect.left() + 12.0 + slot_w * (i as f32 + 0.5), rect.top() + 70.0);
            let lit = now == Some(i);
            if lit {
                painter.rect_filled(
                    egui::Rect::from_center_size(pos2(c.x, rect.center().y), vec2(slot_w - 8.0, rect.height() - 16.0)),
                    CornerRadius::same(12),
                    ACCENT.gamma_multiply(0.18),
                );
            }
            let col = if lit { ACCENT } else { v.strong_text_color() };
            match ch {
                'D' => arrow(painter, c, true, col),
                'U' => arrow(painter, c, false, col),
                _ => {
                    painter.circle_filled(c, 3.0, v.weak_text_color());
                }
            }
            let count = if i % 2 == 0 { (i / 2 + 1).to_string() } else { "et".to_string() };
            let size = if i % 2 == 0 { 17.0 } else { 12.5 };
            painter.text(pos2(c.x, rect.bottom() - 26.0), Align2::CENTER_CENTER, count, FontId::proportional(size), if lit { ACCENT } else { v.weak_text_color() });
        }

        ui.add_space(14.0);
        ui.horizontal_wrapped(|ui| {
            let label = if self.clock.running { "Stop" } else { "Démarrer" };
            let b = egui::Button::new(RichText::new(label).size(15.0).color(v_sel(ui))).fill(ACCENT).min_size(vec2(110.0, 34.0));
            if ui.add(b).clicked() {
                if self.clock.running {
                    self.clock.stop();
                } else {
                    self.clock.start();
                }
            }
            ui.add_space(10.0);
            ui.label("Tempo");
            ui.add(egui::Slider::new(&mut self.bpm, 40.0..=140.0).step_by(1.0).suffix(" BPM"));
            ui.toggle_value(&mut self.sound, "Son");
            ui.toggle_value(&mut self.metronome, "Métronome");
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Accord joué").weak());
        ui.horizontal_wrapped(|ui| {
            for c in picker_chords(favorites) {
                if crate::widgets::chip(ui, &c, c == self.chord).clicked() {
                    self.chord = c;
                }
            }
        });
        ui.add_space(10.0);
        ui.label(
            RichText::new("Conseil : coupe le son et gratte avec le métronome. Ta main fait des allers-retours réguliers, même sur les « . » où elle ne touche pas les cordes.")
                .weak(),
        );
    }
}

fn v_sel(ui: &egui::Ui) -> Color32 {
    ui.visuals().selection.stroke.color
}

/// Strum arrow: down (↓) or up (↑).
fn arrow(p: &egui::Painter, c: egui::Pos2, down: bool, col: Color32) {
    let dir = if down { 1.0 } else { -1.0 };
    let (tail, head) = (pos2(c.x, c.y - 26.0 * dir), pos2(c.x, c.y + 26.0 * dir));
    p.line_segment([tail, pos2(c.x, head.y - 10.0 * dir)], Stroke::new(5.0_f32, col));
    let tri = vec![head, pos2(c.x - 11.0, head.y - 16.0 * dir), pos2(c.x + 11.0, head.y - 16.0 * dir)];
    p.add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
}
