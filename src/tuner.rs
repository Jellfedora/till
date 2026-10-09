//! "Accordeur" page: guitar tuner listening to the microphone.
//!
//! The input stream fills a ring buffer; the UI thread runs YIN pitch detection on the
//! latest samples a few dozen times per second.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, pos2, vec2};

use crate::chords::{root_name, root_name_fr};
use crate::mic::Mic;
use crate::theme::ACCENT;

const IN_TUNE: f32 = 4.0;
const GREEN: Color32 = Color32::from_rgb(76, 190, 110);
/// Lowest / highest pitch we look for (a detuned low D to well above the high E).
const F_MIN: f32 = 60.0;
const F_MAX: f32 = 1000.0;
const WINDOW: usize = 2048;
const ANALYSE_EVERY: Duration = Duration::from_millis(40);

/// Tunings, low string first, as MIDI notes.
pub const TUNINGS: &[(&str, [u8; 6])] = &[
    ("Standard (E A D G B E)", [40, 45, 50, 55, 59, 64]),
    ("Drop D (D A D G B E)", [38, 45, 50, 55, 59, 64]),
    ("Demi-ton plus bas (Eb)", [39, 44, 49, 54, 58, 63]),
    ("DADGAD", [38, 45, 50, 55, 57, 62]),
    ("Open G (D G D G B D)", [38, 43, 50, 55, 59, 62]),
    ("Open D (D A D F# A D)", [38, 45, 50, 54, 57, 62]),
];

pub struct Tuner {
    input: Option<Mic>,
    pub error: Option<String>,
    pub tuning: usize,
    pub a4: f32,
    /// Locked string (0 = low), or `None` to follow whatever is played.
    locked: Option<usize>,
    recent: VecDeque<f32>,
    freq: Option<f32>,
    cents: f32,
    level: f32,
    last_heard: Option<Instant>,
    last_analysis: Option<Instant>,
}

impl Default for Tuner {
    fn default() -> Self {
        Tuner {
            input: None,
            error: None,
            tuning: 0,
            a4: 440.0,
            locked: None,
            recent: VecDeque::new(),
            freq: None,
            cents: 0.0,
            level: 0.0,
            last_heard: None,
            last_analysis: None,
        }
    }
}

/// YIN pitch estimate of `x` in Hz, or `None` when there is no clear pitch.
pub fn yin(x: &[f32], sr: f32) -> Option<f32> {
    let tau_min = (sr / F_MAX).floor().max(2.0) as usize;
    let tau_max = (sr / F_MIN).ceil() as usize;
    if x.len() < tau_max + 64 {
        return None;
    }
    let w = (x.len() - tau_max).min(WINDOW);
    let x = &x[x.len() - w - tau_max..];
    let mut d = vec![0.0f32; tau_max + 1];
    for (tau, dt) in d.iter_mut().enumerate().skip(1) {
        let (a, b) = (&x[..w], &x[tau..tau + w]);
        *dt = a.iter().zip(b).map(|(p, q)| (p - q) * (p - q)).sum();
    }
    // Cumulative mean normalized difference.
    let mut cmnd = vec![1.0f32; tau_max + 1];
    let mut running = 0.0;
    for tau in 1..=tau_max {
        running += d[tau];
        cmnd[tau] = if running > 0.0 { d[tau] * tau as f32 / running } else { 1.0 };
    }
    let mut tau = (tau_min..tau_max).find(|&t| cmnd[t] < 0.12).or_else(|| {
        let t = (tau_min..tau_max).min_by(|&a, &b| cmnd[a].total_cmp(&cmnd[b]))?;
        (cmnd[t] < 0.3).then_some(t)
    })?;
    while tau + 1 < tau_max && cmnd[tau + 1] < cmnd[tau] {
        tau += 1;
    }
    // Parabolic interpolation for sub-sample precision.
    let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
    let den = a - 2.0 * b + c;
    let shift = if den.abs() > 1e-9 { 0.5 * (a - c) / den } else { 0.0 };
    Some(sr / (tau as f32 + shift.clamp(-1.0, 1.0)))
}

impl Tuner {
    /// Opens the microphone (once) while the page is visible.
    pub fn start(&mut self) {
        if self.input.is_none() && self.error.is_none() {
            match Mic::open() {
                Ok(i) => self.input = Some(i),
                Err(e) => self.error = Some(e),
            }
        }
    }

    /// Releases the microphone.
    pub fn stop(&mut self) {
        self.input = None;
        self.error = None;
        self.recent.clear();
        self.freq = None;
    }

    fn midi_of(&self, f: f32) -> f32 {
        69.0 + 12.0 * (f / self.a4).log2()
    }

    fn strings(&self) -> [u8; 6] {
        TUNINGS[self.tuning.min(TUNINGS.len() - 1)].1
    }

    /// String the player is most likely tuning.
    fn target(&self, midi: f32) -> usize {
        self.locked.unwrap_or_else(|| {
            let s = self.strings();
            (0..6).min_by(|&a, &b| (s[a] as f32 - midi).abs().total_cmp(&(s[b] as f32 - midi).abs())).unwrap_or(0)
        })
    }

    fn analyse(&mut self) {
        let Some(input) = &self.input else { return };
        if self.last_analysis.is_some_and(|t| t.elapsed() < ANALYSE_EVERY) {
            return;
        }
        self.last_analysis = Some(Instant::now());
        let samples = input.latest(WINDOW + (input.sr / F_MIN) as usize + 1);
        let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt();
        self.level = self.level * 0.6 + rms * 0.4;
        let f = if rms > 0.004 { yin(&samples, input.sr) } else { None };
        match f {
            Some(f) => {
                self.recent.push_back(f);
                if self.recent.len() > 5 {
                    self.recent.pop_front();
                }
                // Median of the last estimates rejects stray octave jumps.
                let mut sorted: Vec<f32> = self.recent.iter().copied().collect();
                sorted.sort_by(f32::total_cmp);
                let med = sorted[sorted.len() / 2];
                let midi = self.midi_of(med);
                let target = self.strings()[self.target(midi)] as f32;
                let cents = (midi - target) * 100.0;
                let fresh = self.freq.is_none() || self.last_heard.is_none_or(|t| t.elapsed() > Duration::from_millis(400));
                self.cents = if fresh { cents } else { self.cents + 0.35 * (cents - self.cents) };
                self.freq = Some(med);
                self.last_heard = Some(Instant::now());
            }
            None => {
                if self.last_heard.is_none_or(|t| t.elapsed() > Duration::from_millis(300)) {
                    self.recent.clear();
                }
            }
        }
    }

    /// Draws the page. Returns a (string, midi) reference note to play when a string is clicked.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<(u8, u8)> {
        self.start();
        self.keys(ui);
        self.analyse();
        ui.ctx().request_repaint_after(Duration::from_millis(30));
        let mut play = None;

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 20)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(RichText::new("Accordeur").strong());
                        let sub = match (&self.input, &self.error) {
                            (Some(i), _) => format!("Micro : {}", i.device),
                            (None, Some(_)) => "Micro indisponible".into(),
                            _ => "Ouverture du micro…".into(),
                        };
                        ui.label(RichText::new(sub).weak());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::DragValue::new(&mut self.a4).range(415.0..=466.0).speed(0.2).suffix(" Hz"))
                            .on_hover_text("Fréquence de référence du La");
                        ui.label(RichText::new("La =").weak());
                        ui.add_space(12.0);
                        egui::ComboBox::from_id_salt("tuning")
                            .selected_text(TUNINGS[self.tuning].0)
                            .width(210.0)
                            .show_ui(ui, |ui| {
                                for (i, (name, _)) in TUNINGS.iter().enumerate() {
                                    ui.selectable_value(&mut self.tuning, i, *name);
                                }
                            });
                    });
                });
                if let Some(e) = &self.error {
                    ui.add_space(8.0);
                    ui.colored_label(ui.visuals().error_fg_color, e);
                    ui.label(
                        RichText::new("Vérifie l'accès au micro dans Réglages Système › Confidentialité et sécurité › Micro.").weak(),
                    );
                    if ui.button("Réessayer").clicked() {
                        self.error = None;
                    }
                }
                ui.add_space(18.0);
                self.gauge(ui);
                ui.add_space(22.0);
                play = self.string_buttons(ui);
                ui.add_space(12.0);
                ui.label(
                    RichText::new(
                        "Joue une corde à vide près du micro. En Auto, Till reconnaît la corde jouée ; choisis une corde (ou touches 1 à 6) pour viser celle-là, même si tu es très loin de la note.",
                    )
                    .weak(),
                );
            });
        });
        play
    }

    fn gauge(&self, ui: &mut egui::Ui) {
        let w = ui.available_width().min(640.0);
        let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), 330.0), Sense::hover());
        let rect = egui::Rect::from_center_size(outer.center(), vec2(w, outer.height()));
        let v = ui.visuals();
        let p = ui.painter();
        p.rect_filled(outer, CornerRadius::same(16), v.faint_bg_color);

        let heard = self.freq.is_some() && self.last_heard.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
        let live = heard && self.last_heard.is_some_and(|t| t.elapsed() < Duration::from_millis(350));
        let cents = self.cents;
        let tuned = heard && cents.abs() <= IN_TUNE;
        let accent = if tuned { GREEN } else { ACCENT };

        // Arc from -50 (left) to +50 cents (right).
        let center = pos2(rect.center().x, rect.top() + 190.0);
        let r = 150.0;
        let angle = |c: f32| std::f32::consts::PI * (1.5 + c.clamp(-50.0, 50.0) / 100.0);
        let at = |c: f32, rad: f32| {
            let a = angle(c);
            pos2(center.x + rad * a.cos(), center.y + rad * a.sin())
        };
        let zone: Vec<_> = (-5..=5).map(|c| at(c as f32, r)).collect();
        p.add(egui::Shape::line(zone, Stroke::new(10.0_f32, GREEN.gamma_multiply(0.35))));
        for c in (-50..=50).step_by(5) {
            let major = c % 25 == 0;
            let len = if major { 16.0 } else { 8.0 };
            let col = if c == 0 { GREEN } else { v.weak_text_color() };
            p.line_segment([at(c as f32, r - len), at(c as f32, r)], Stroke::new(if major { 2.0_f32 } else { 1.0 }, col));
        }
        for c in [-50, -25, 25, 50] {
            p.text(at(c as f32, r + 16.0), Align2::CENTER_CENTER, format!("{c:+}"), FontId::proportional(11.0), v.weak_text_color());
        }
        p.text(at(0.0, r + 16.0), Align2::CENTER_CENTER, "0", FontId::proportional(11.0), GREEN);

        if heard {
            let tip = at(cents, r - 6.0);
            let ink = if live { accent } else { accent.gamma_multiply(0.4) };
            p.line_segment([center, tip], Stroke::new(3.0_f32, ink));
            p.circle_filled(center, 7.0, ink);
        } else {
            p.circle_filled(center, 7.0, v.weak_text_color());
        }

        // Note name, octave and French name in the middle of the dial.
        let (name, fr, detail, status) = match self.freq.filter(|_| heard) {
            Some(f) => {
                let midi = self.midi_of(f);
                let s = self.strings()[self.target(midi)];
                let pc = s % 12;
                let status = if tuned {
                    "Accordée".to_string()
                } else if cents.abs() >= 100.0 {
                    let semis = (cents.abs() / 100.0).round();
                    let what = if cents < 0.0 { "Trop bas" } else { "Trop haut" };
                    let todo = if cents < 0.0 { "resserre" } else { "desserre" };
                    format!("{what} d'environ {semis:.0} demi-ton{} · {todo}", if semis > 1.0 { "s" } else { "" })
                } else if cents < 0.0 {
                    format!("Trop bas de {:.0} cents · resserre", -cents)
                } else {
                    format!("Trop haut de {cents:.0} cents · desserre")
                };
                let played = midi.round() as i32;
                let played_name = format!("{}{}", root_name(played.rem_euclid(12) as u8), played.div_euclid(12) - 1);
                (
                    format!("{}{}", root_name(pc), s / 12 - 1),
                    root_name_fr(pc).to_string(),
                    format!("{f:.1} Hz · note jouée {played_name} · {cents:+.0} cents"),
                    status,
                )
            }
            None => match self.locked {
                Some(i) => {
                    let m = self.strings()[i];
                    (
                        format!("{}{}", root_name(m % 12), m / 12 - 1),
                        root_name_fr(m % 12).to_string(),
                        format!("Joue la corde {}…", 6 - i),
                        String::new(),
                    )
                }
                None => ("–".into(), String::new(), "En attente d'une note…".into(), String::new()),
            },
        };
        p.text(pos2(center.x, center.y - 62.0), Align2::CENTER_CENTER, name, FontId::proportional(44.0), if heard { accent } else { v.weak_text_color() });
        p.text(pos2(center.x, center.y - 28.0), Align2::CENTER_CENTER, fr, FontId::proportional(16.0), v.text_color());
        p.text(pos2(center.x, center.y + 34.0), Align2::CENTER_CENTER, status, FontId::proportional(17.0), accent);
        p.text(pos2(center.x, center.y + 60.0), Align2::CENTER_CENTER, detail, FontId::proportional(12.5), v.weak_text_color());

        // Microphone level.
        let bar = egui::Rect::from_min_size(pos2(outer.left() + 18.0, outer.bottom() - 22.0), vec2(120.0, 5.0));
        p.rect_filled(bar, CornerRadius::same(3), v.widgets.inactive.bg_fill);
        let lvl = (self.level * 12.0).sqrt().clamp(0.0, 1.0);
        p.rect_filled(egui::Rect::from_min_size(bar.min, vec2(bar.width() * lvl, bar.height())), CornerRadius::same(3), v.weak_text_color());
        p.text(bar.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, "niveau micro", FontId::proportional(10.5), v.weak_text_color());
    }

    fn string_buttons(&mut self, ui: &mut egui::Ui) -> Option<(u8, u8)> {
        let strings = self.strings();
        let heard = self.freq.filter(|_| self.last_heard.is_some_and(|t| t.elapsed() < Duration::from_millis(1500)));
        let current = self.locked.or_else(|| heard.map(|f| self.target(self.midi_of(f))));
        let tuned = heard.is_some() && self.cents.abs() <= IN_TUNE;
        let mut play = None;
        ui.label(RichText::new("CORDE À ACCORDER").size(10.5).weak().strong());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let gap = 10.0;
            let auto_w = 84.0;
            let n = strings.len() as f32;
            let size = ((ui.available_width() - auto_w - gap * n) / n).min(96.0);
            ui.spacing_mut().item_spacing.x = gap;

            // "Auto" follows whatever string is played.
            let (rect, resp) = ui.allocate_exact_size(vec2(auto_w, 64.0), Sense::click());
            let v = ui.visuals();
            let on = self.locked.is_none();
            let fill = if on {
                v.strong_text_color()
            } else if resp.hovered() {
                v.widgets.hovered.weak_bg_fill
            } else {
                v.widgets.inactive.bg_fill
            };
            let ink = if on { v.panel_fill } else { v.strong_text_color() };
            ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
            ui.painter().text(rect.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, "Auto", FontId::proportional(16.0), ink);
            ui.painter().text(rect.center() + vec2(0.0, 13.0), Align2::CENTER_CENTER, "touche 0", FontId::proportional(10.5), ink.gamma_multiply(0.7));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.on_hover_text("Détecte automatiquement la corde jouée").clicked() {
                self.locked = None;
            }

            for (i, &m) in strings.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(size, 64.0), Sense::click());
                let v = ui.visuals();
                let on = current == Some(i);
                let fill = match (on, tuned && heard.is_some_and(|f| self.target(self.midi_of(f)) == i)) {
                    (true, true) => GREEN,
                    (true, false) => ACCENT,
                    _ if resp.hovered() => v.widgets.hovered.weak_bg_fill,
                    _ => v.widgets.inactive.bg_fill,
                };
                let ink = if on { v.selection.stroke.color } else { v.strong_text_color() };
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(12), fill);
                if self.locked == Some(i) {
                    p.rect_stroke(rect, CornerRadius::same(12), Stroke::new(2.0_f32, v.strong_text_color()), egui::StrokeKind::Inside);
                }
                let pc = m % 12;
                p.text(rect.center() - vec2(0.0, 9.0), Align2::CENTER_CENTER, format!("{}{}", root_name(pc), m / 12 - 1), FontId::proportional(18.0), ink);
                let sub = format!("{} · corde {}", root_name_fr(pc), 6 - i);
                p.text(rect.center() + vec2(0.0, 14.0), Align2::CENTER_CENTER, sub, FontId::proportional(10.5), ink.gamma_multiply(0.8));
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.on_hover_text(format!("Accorder la corde {} (touche {})", 6 - i, 6 - i)).clicked() {
                    self.locked = Some(i);
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let i = current.unwrap_or(0);
            let m = strings[i];
            let label = format!("Écouter la corde {} ({}{})", 6 - i, root_name(m % 12), m / 12 - 1);
            if ui.button(label).on_hover_text("Joue la note de référence").clicked() {
                play = Some((i as u8, m));
            }
        });
        play
    }

    /// Number keys pick the string to tune (1 = high E … 6 = low E), 0 goes back to auto.
    fn keys(&mut self, ui: &egui::Ui) {
        if ui.ctx().wants_keyboard_input() {
            return;
        }
        use egui::Key::*;
        ui.input(|i| {
            if i.modifiers.any() {
                return;
            }
            if i.key_pressed(Num0) {
                self.locked = None;
            }
            for (k, key) in [Num1, Num2, Num3, Num4, Num5, Num6].into_iter().enumerate() {
                if i.key_pressed(key) {
                    self.locked = Some(5 - k);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plucked-string-like tone: fundamental plus strong harmonics.
    fn tone(f: f32, sr: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                (1..=6).map(|h| (std::f32::consts::TAU * f * h as f32 * t).sin() / h as f32 * if h == 2 { 1.6f32 } else { 1.0 }).sum()
            })
            .collect()
    }

    #[test]
    fn finds_guitar_strings() {
        let sr = 48_000.0;
        for f in [73.42, 82.41, 110.0, 146.83, 196.0, 246.94, 329.63, 333.0] {
            let got = yin(&tone(f, sr, 4096), sr).unwrap();
            let cents = 1200.0 * (got / f).log2();
            assert!(cents.abs() < 1.0, "{f} Hz -> {got} Hz ({cents} cents)");
        }
        assert!(yin(&vec![0.0; 4096], sr).is_none());
    }
}
