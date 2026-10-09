//! Small custom widgets: sidebar entries, painted icons, grouped controls.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, RichText, Sense, Stroke, pos2, vec2};

use crate::theme::ACCENT;

#[derive(Clone, Copy)]
pub enum Icon {
    Tab,
    Chords,
    File,
    Plus,
    Play,
    Pause,
    Stop,
    Back,
    Star,
    Tuner,
    Learn,
    Trash,
}

/// Five-pointed star, filled when `on`.
pub fn paint_star(p: &Painter, c: Pos2, r: f32, on: bool, color: Color32) {
    let pt = |k: usize, rad: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
        pos2(c.x + rad * a.cos(), c.y + rad * a.sin())
    };
    let outline: Vec<Pos2> = (0..10).map(|k| pt(k, if k % 2 == 0 { r } else { r * 0.45 })).collect();
    if on {
        // The star is concave: fill the inner pentagon and each point separately.
        let inner: Vec<Pos2> = (0..5).map(|k| outline[2 * k + 1]).collect();
        p.add(egui::Shape::convex_polygon(inner, color, Stroke::NONE));
        for k in 0..5 {
            let tri = vec![outline[(2 * k + 9) % 10], outline[2 * k], outline[2 * k + 1]];
            p.add(egui::Shape::convex_polygon(tri, color, Stroke::NONE));
        }
    }
    p.add(egui::Shape::closed_line(outline, Stroke::new(1.3_f32, color)));
}

/// Clickable favorite star in `rect`.
pub fn star_at(ui: &mut egui::Ui, rect: Rect, id: egui::Id, on: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let v = ui.visuals();
    if resp.hovered() {
        ui.painter().circle_filled(rect.center(), rect.width() / 2.0, v.widgets.hovered.weak_bg_fill);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if on { ACCENT } else if resp.hovered() { v.strong_text_color() } else { v.weak_text_color() };
    paint_star(ui.painter(), rect.center(), rect.width() * 0.3, on, color);
    resp.on_hover_text(if on { "Retirer des favoris" } else { "Ajouter aux favoris" })
}

/// Favorite star laid out inline.
pub fn star_button(ui: &mut egui::Ui, on: bool) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
    let id = ui.id().with("star");
    star_at(ui, rect, id, on)
}

pub fn paint_icon(p: &Painter, icon: Icon, c: Pos2, color: Color32) {
    let s = Stroke::new(1.4_f32, color);
    match icon {
        Icon::Tab => {
            for i in 0..4 {
                let y = c.y - 6.0 + i as f32 * 4.0;
                p.line_segment([pos2(c.x - 8.0, y), pos2(c.x + 8.0, y)], s);
            }
            p.line_segment([pos2(c.x - 8.0, c.y - 6.0), pos2(c.x - 8.0, c.y + 6.0)], s);
            p.circle_filled(pos2(c.x + 1.0, c.y - 2.0), 2.2, color);
            p.circle_filled(pos2(c.x + 5.0, c.y + 2.0), 2.2, color);
        }
        Icon::Chords => {
            let r = Rect::from_center_size(c, vec2(14.0, 16.0));
            for i in 0..4 {
                let x = r.left() + i as f32 * r.width() / 3.0;
                p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], s);
            }
            for i in 0..4 {
                let y = r.top() + i as f32 * r.height() / 3.0;
                p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], s);
            }
            p.line_segment([r.left_top(), r.right_top()], Stroke::new(2.6_f32, color));
            p.circle_filled(pos2(r.left() + r.width() / 3.0, r.top() + r.height() / 6.0), 2.4, color);
            p.circle_filled(pos2(r.right(), r.top() + r.height() / 2.0), 2.4, color);
        }
        Icon::File => {
            let r = Rect::from_center_size(c, vec2(11.0, 14.0));
            p.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.2_f32, color), egui::StrokeKind::Inside);
            for i in 0..3 {
                let y = r.top() + 4.0 + i as f32 * 3.0;
                p.line_segment([pos2(r.left() + 3.0, y), pos2(r.right() - 3.0, y)], Stroke::new(1.0_f32, color));
            }
        }
        Icon::Play => {
            let pts = vec![pos2(c.x - 3.5, c.y - 6.0), pos2(c.x + 6.0, c.y), pos2(c.x - 3.5, c.y + 6.0)];
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Pause => {
            for dx in [-3.0, 3.0] {
                let r = Rect::from_center_size(pos2(c.x + dx, c.y), vec2(3.2, 11.0));
                p.rect_filled(r, CornerRadius::same(1), color);
            }
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(9.5, 9.5)), CornerRadius::same(2), color);
        }
        Icon::Back => {
            p.line_segment([pos2(c.x + 3.0, c.y - 6.0), pos2(c.x - 3.0, c.y)], s);
            p.line_segment([pos2(c.x - 3.0, c.y), pos2(c.x + 3.0, c.y + 6.0)], s);
        }
        Icon::Star => paint_star(p, c, 7.5, true, color),
        Icon::Trash => {
            // Bin with a lid.
            p.line_segment([pos2(c.x - 7.0, c.y - 5.0), pos2(c.x + 7.0, c.y - 5.0)], s);
            p.line_segment([pos2(c.x - 2.5, c.y - 7.0), pos2(c.x + 2.5, c.y - 7.0)], s);
            let body = [pos2(c.x - 5.5, c.y - 5.0), pos2(c.x - 4.5, c.y + 7.0), pos2(c.x + 4.5, c.y + 7.0), pos2(c.x + 5.5, c.y - 5.0)];
            p.add(egui::Shape::line(body.to_vec(), s));
            for dx in [-2.0, 2.0] {
                p.line_segment([pos2(c.x + dx, c.y - 2.0), pos2(c.x + dx, c.y + 4.5)], s);
            }
        }
        Icon::Learn => {
            // Open book.
            let l = [pos2(c.x, c.y - 4.0), pos2(c.x - 8.0, c.y - 6.0), pos2(c.x - 8.0, c.y + 5.0), pos2(c.x, c.y + 7.0)];
            let r = [pos2(c.x, c.y - 4.0), pos2(c.x + 8.0, c.y - 6.0), pos2(c.x + 8.0, c.y + 5.0), pos2(c.x, c.y + 7.0)];
            p.add(egui::Shape::closed_line(l.to_vec(), s));
            p.add(egui::Shape::closed_line(r.to_vec(), s));
        }
        Icon::Tuner => {
            // Small dial with a needle.
            let base = pos2(c.x, c.y + 5.0);
            let arc: Vec<Pos2> = (0..=12)
                .map(|k| {
                    let a = std::f32::consts::PI * (1.0 + k as f32 / 12.0);
                    pos2(base.x + 8.5 * a.cos(), base.y + 8.5 * a.sin())
                })
                .collect();
            p.add(egui::Shape::line(arc, s));
            p.line_segment([base, pos2(c.x + 3.5, c.y - 2.5)], Stroke::new(1.6_f32, color));
            p.circle_filled(base, 1.8, color);
        }
        Icon::Plus => {
            p.line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], s);
            p.line_segment([pos2(c.x, c.y - 6.0), pos2(c.x, c.y + 6.0)], s);
        }
    }
}

/// A full-width sidebar entry with icon, label and optional trailing badge.
pub fn nav_item(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool, badge: Option<String>, height: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let v = ui.visuals();
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, CornerRadius::same(8), ACCENT.gamma_multiply(0.16));
        p.rect_filled(
            Rect::from_min_size(rect.left_top() + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0)),
            CornerRadius::same(2),
            ACCENT,
        );
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
    }
    let color = if selected { ACCENT } else if resp.hovered() { v.strong_text_color() } else { v.text_color() };
    paint_icon(p, icon, pos2(rect.left() + 20.0, rect.center().y), color);

    let font = FontId::proportional(if height > 32.0 { 14.5 } else { 13.0 });
    let badge_w = badge.as_ref().map_or(0.0, |_| 34.0);
    let max_w = rect.width() - 42.0 - badge_w;
    let text = ellipsize(ui, label, max_w, font, color);
    p.galley(pos2(rect.left() + 40.0, rect.center().y - text.size().y / 2.0), text, color);

    if let Some(b) = badge {
        let g = p.layout_no_wrap(b, FontId::proportional(11.0), v.text_color());
        let br = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), g.size() + vec2(12.0, 4.0));
        p.rect_filled(br, CornerRadius::same(255), v.widgets.inactive.bg_fill);
        p.galley(br.center() - g.size() / 2.0, g, v.text_color());
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn ellipsize(ui: &egui::Ui, text: &str, max_w: f32, font: FontId, color: Color32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w);
    ui.fonts(|f| f.layout_job(job))
}

pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(14.0);
    ui.label(RichText::new(text.to_uppercase()).size(10.5).weak().strong());
    ui.add_space(2.0);
}

/// Groups a few controls on a soft rounded surface.
pub fn group<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // In a wrapping row, egui only knows a frame's size once drawn: remember it, and
    // start a new row when the group would not fit.
    let id = ui.next_auto_id().with("group_width");
    let last = ui.ctx().data(|d| d.get_temp::<f32>(id));
    if ui.layout().main_wrap && last.is_some_and(|w| w > ui.available_size_before_wrap().x) {
        ui.end_row();
    }
    let r = egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal(add).inner
        });
    ui.ctx().data_mut(|d| d.insert_temp(id, r.response.rect.width()));
    r.inner
}

/// App logo (the guitar icon) and name.
pub fn logo(ui: &mut egui::Ui, texture: &egui::TextureHandle) {
    ui.horizontal(|ui| {
        ui.add(egui::Image::new(texture).fit_to_exact_size(vec2(36.0, 36.0)));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("Till").size(18.0).strong());
            ui.label(RichText::new("Tabs & accords guitare").size(11.0).weak());
        });
    });
}

/// Three-way theme selector that always fits the width it is given.
pub fn theme_switch(ui: &mut egui::Ui) {
    use egui::ThemePreference as T;
    let current = ui.ctx().options(|o| o.theme_preference);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let v = ui.visuals().clone();
    ui.painter().rect_filled(rect, CornerRadius::same(8), v.widgets.inactive.bg_fill);
    let options = [(T::Light, "Clair"), (T::Dark, "Sombre"), (T::System, "Auto")];
    let w = rect.width() / options.len() as f32;
    for (i, (pref, label)) in options.into_iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * w, 0.0), vec2(w, rect.height())).shrink(3.0);
        let resp = ui.interact(r, ui.id().with(("theme", i)), Sense::click());
        let selected = current == pref;
        if selected {
            ui.painter().rect_filled(r, CornerRadius::same(6), ACCENT);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(6), v.widgets.hovered.weak_bg_fill);
        }
        let ink = if selected { v.selection.stroke.color } else { v.text_color() };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), ink);
        if resp.clicked() {
            ui.ctx().set_theme(pref);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
}

/// Round button with a painted transport icon.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let v = ui.visuals();
    let fill = if active {
        ACCENT
    } else if resp.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        v.widgets.inactive.bg_fill
    };
    ui.painter().circle_filled(rect.center(), 13.0, fill);
    let ink = if active { v.selection.stroke.color } else { v.strong_text_color() };
    paint_icon(ui.painter(), icon, rect.center(), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Small rounded chord chip; filled with the accent color when selected.
pub fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> Response {
    let v = ui.visuals().clone();
    let ink = if selected { v.selection.stroke.color } else { v.strong_text_color() };
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::monospace(13.0), ink);
    let (rect, resp) = ui.allocate_exact_size(g.size() + vec2(18.0, 10.0), Sense::click());
    let fill = if selected {
        ACCENT
    } else if resp.hovered() {
        ACCENT.gamma_multiply(0.3)
    } else {
        ACCENT.gamma_multiply(0.14)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
