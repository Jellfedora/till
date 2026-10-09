//! Practice journal: minutes played per day, streak, chords mastered.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Local, NaiveDate};
use eframe::egui::{self, Align2, CornerRadius, FontId, RichText, Sense, pos2, vec2};

use crate::theme::ACCENT;

/// A day counts towards the streak from this many minutes of practice.
const STREAK_MINUTES: f32 = 5.0;

#[derive(Default)]
pub struct Journal {
    /// Seconds practiced per day.
    days: BTreeMap<NaiveDate, f32>,
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

impl Journal {
    pub fn add(&mut self, seconds: f32) {
        *self.days.entry(today()).or_default() += seconds;
    }

    pub fn minutes(&self, day: NaiveDate) -> f32 {
        self.days.get(&day).copied().unwrap_or(0.0) / 60.0
    }

    /// Consecutive days with enough practice, ending today (or yesterday if today is not done yet).
    pub fn streak(&self) -> u32 {
        let mut day = today();
        if self.minutes(day) < STREAK_MINUTES {
            day -= Duration::days(1);
        }
        let mut n = 0;
        while self.minutes(day) >= STREAK_MINUTES {
            n += 1;
            day -= Duration::days(1);
        }
        n
    }

    pub fn save(&self) -> String {
        self.days.iter().map(|(d, s)| format!("{d} {s:.0}")).collect::<Vec<_>>().join("\n")
    }

    pub fn load(text: &str) -> Journal {
        let days = text
            .lines()
            .filter_map(|l| {
                let (d, s) = l.split_once(' ')?;
                Some((d.parse().ok()?, s.parse().ok()?))
            })
            .collect();
        Journal { days }
    }

    /// Streak, today and this week, then a bar chart of the last 14 days.
    pub fn show(&self, ui: &mut egui::Ui) {
        let t = today();
        let week: f32 = (0..t.weekday().num_days_from_monday() as i64 + 1).map(|k| self.minutes(t - Duration::days(k))).sum();
        let total: f32 = self.days.values().sum::<f32>() / 60.0;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let streak = self.streak();
            stat(ui, &format!("{streak} jour{}", if streak > 1 { "s" } else { "" }), "série en cours");
            stat(ui, &fmt_minutes(self.minutes(t)), "aujourd'hui");
            stat(ui, &fmt_minutes(week), "cette semaine");
            stat(ui, &fmt_minutes(total), "au total");
        });
        ui.add_space(10.0);
        ui.label(RichText::new(format!("Une journée compte dans la série à partir de {STREAK_MINUTES:.0} minutes de pratique.")).weak());
        ui.add_space(14.0);

        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(720.0), 150.0), Sense::hover());
        let v = ui.visuals();
        let p = ui.painter();
        let days: Vec<NaiveDate> = (0..14).rev().map(|k| t - Duration::days(k)).collect();
        let max = days.iter().map(|&d| self.minutes(d)).fold(15.0, f32::max);
        let w = rect.width() / days.len() as f32;
        const JOURS: [&str; 7] = ["L", "M", "M", "J", "V", "S", "D"];
        for (i, &d) in days.iter().enumerate() {
            let m = self.minutes(d);
            let h = (rect.height() - 34.0) * (m / max);
            let x = rect.left() + i as f32 * w;
            let bar = egui::Rect::from_min_max(pos2(x + w * 0.2, rect.bottom() - 20.0 - h.max(2.0)), pos2(x + w * 0.8, rect.bottom() - 20.0));
            let col = if m >= STREAK_MINUTES { ACCENT } else { v.widgets.inactive.bg_fill };
            p.rect_filled(bar, CornerRadius::same(4), col);
            if m >= 1.0 {
                p.text(bar.center_top() - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("{m:.0}"), FontId::proportional(10.0), v.weak_text_color());
            }
            let label = if d == t { "auj." } else { JOURS[d.weekday().num_days_from_monday() as usize] };
            p.text(pos2(x + w / 2.0, rect.bottom() - 8.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), v.weak_text_color());
        }
    }
}

pub fn fmt_minutes(m: f32) -> String {
    if m >= 60.0 { format!("{}h{:02}", (m / 60.0) as u32, (m % 60.0) as u32) } else { format!("{m:.0} min") }
}

pub fn stat(ui: &mut egui::Ui, value: &str, label: &str) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_min_width(120.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(value).size(22.0).strong().color(ACCENT));
                ui.label(RichText::new(label).weak());
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streak_and_round_trip() {
        let t = today();
        let mut j = Journal::default();
        for k in 1..=3 {
            j.days.insert(t - Duration::days(k), 600.0);
        }
        j.days.insert(t - Duration::days(5), 600.0);
        assert_eq!(j.streak(), 3);
        j.add(400.0);
        assert_eq!(j.streak(), 4);
        let back = Journal::load(&j.save());
        assert_eq!(back.streak(), 4);
    }
}
