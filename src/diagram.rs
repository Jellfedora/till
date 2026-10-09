//! Chord diagram cards.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, pos2, vec2};

use crate::chords::Voicing;
use crate::song::ChordInfo;

pub const CARD_W: f32 = 92.0;
pub const CARD_H: f32 = 150.0;

const SX: f32 = 12.0; // string spacing
const FY: f32 = 15.0; // fret spacing
const ROWS: u8 = 5;

pub fn card_info(ui: &mut egui::Ui, info: &ChordInfo, highlight: bool) -> Response {
    card(ui, &info.name, &info.chord.name_fr(), info.voicing.as_ref(), 1.0, highlight)
}

pub fn card(
    ui: &mut egui::Ui,
    name: &str,
    name_fr: &str,
    voicing: Option<&Voicing>,
    k: f32,
    highlight: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H) * k, Sense::click());
    let v = ui.visuals();
    let p = ui.painter();
    let ink = v.strong_text_color();
    let accent = v.selection.bg_fill;

    let hovered = resp.hovered();
    if highlight || hovered {
        p.rect_filled(rect, CornerRadius::same(8), accent.gamma_multiply(if highlight { 0.28 } else { 0.14 }));
    }

    let cx = rect.center().x;
    p.text(pos2(cx, rect.top() + 4.0 * k), Align2::CENTER_TOP, name, FontId::proportional(15.0 * k), ink);

    let (sx, fy) = (SX * k, FY * k);
    let grid_w = sx * 5.0;
    let left = cx - grid_w / 2.0;
    let marks_y = rect.top() + 32.0 * k;
    let top = marks_y + 8.0 * k;
    let line = Stroke::new(1.0_f32, v.text_color().gamma_multiply(0.55));
    let fr_y = top + ROWS as f32 * fy + 6.0 * k;

    let Some(vo) = voicing else {
        p.text(pos2(cx, top + fy * 2.5), Align2::CENTER_CENTER, "?", FontId::proportional(24.0 * k), v.weak_text_color());
        french(ui, name_fr, cx, fr_y, k);
        return resp;
    };

    let fretted: Vec<u8> = vo.frets.iter().flatten().copied().filter(|&f| f > 0).collect();
    let max = fretted.iter().copied().max().unwrap_or(0);
    let min = fretted.iter().copied().min().unwrap_or(1);
    let start = if max <= ROWS { 1 } else { min };

    for r in 0..=ROWS {
        let y = top + r as f32 * fy;
        p.line_segment([pos2(left, y), pos2(left + grid_w, y)], line);
    }
    for s in 0..6 {
        let x = left + s as f32 * sx;
        p.line_segment([pos2(x, top), pos2(x, top + ROWS as f32 * fy)], line);
    }
    if start == 1 {
        p.line_segment([pos2(left - 0.5, top), pos2(left + grid_w + 0.5, top)], Stroke::new(3.5 * k, ink));
    } else {
        p.text(
            pos2(left - 5.0 * k, top + fy / 2.0),
            Align2::RIGHT_CENTER,
            format!("{start}fr"),
            FontId::proportional(9.5 * k),
            v.text_color(),
        );
    }

    for (s, f) in vo.frets.iter().enumerate() {
        let x = left + s as f32 * sx;
        match f {
            None => {
                p.text(pos2(x, marks_y), Align2::CENTER_CENTER, "×", FontId::proportional(12.0 * k), v.text_color());
            }
            Some(0) => {
                p.circle_stroke(pos2(x, marks_y), 3.5 * k, Stroke::new(1.2 * k, v.text_color()));
            }
            _ => {}
        }
    }

    let r = 5.5 * k;
    let barre: Vec<usize> = (0..6).filter(|&s| vo.fingers[s] == 1).collect();
    if barre.len() >= 2 {
        let f = vo.frets[barre[0]].unwrap_or(0);
        if barre.iter().all(|&s| vo.frets[s] == Some(f)) {
            let y = top + (f - start) as f32 * fy + fy / 2.0;
            let x0 = left + barre[0] as f32 * sx;
            let x1 = left + *barre.last().unwrap() as f32 * sx;
            p.rect_filled(Rect::from_min_max(pos2(x0, y - r), pos2(x1, y + r)), CornerRadius::same(255), ink);
        }
    }

    for (s, f) in vo.frets.iter().enumerate() {
        let Some(f) = f.filter(|&f| f > 0) else { continue };
        let c = pos2(left + s as f32 * sx, top + (f - start) as f32 * fy + fy / 2.0);
        p.circle_filled(c, r, ink);
        if vo.fingers[s] > 0 {
            p.text(c, Align2::CENTER_CENTER, vo.fingers[s].to_string(), FontId::proportional(9.0 * k), v.panel_fill);
        }
    }

    french(ui, name_fr, cx, fr_y, k);
    resp
}

fn french(ui: &egui::Ui, text: &str, cx: f32, y: f32, k: f32) {
    let mut job = egui::text::LayoutJob::simple(
        text.to_string(),
        FontId::proportional(11.0 * k),
        ui.visuals().weak_text_color(),
        CARD_W * k - 4.0,
    );
    job.halign = egui::Align::Center;
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(pos2(cx, y), galley, Color32::PLACEHOLDER);
}
