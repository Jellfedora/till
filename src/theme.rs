//! Visual identity: warm amber accent, soft surfaces, rounded widgets.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Theme, vec2};

pub const ACCENT: Color32 = Color32::from_rgb(234, 142, 52);
const ON_ACCENT: Color32 = Color32::from_rgb(38, 22, 6);

/// macOS fonts used as fallbacks for symbols the built-in fonts lack (arrows, check marks…).
const FALLBACK_FONTS: &[&str] = &["/System/Library/Fonts/Apple Symbols.ttf"];

fn install_fallback_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (i, path) in FALLBACK_FONTS.iter().enumerate() {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let name = format!("fallback{i}");
        fonts.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    install_fallback_fonts(ctx);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = vec2(8.0, 6.0);
            s.spacing.button_padding = vec2(10.0, 5.0);
            s.spacing.interact_size.y = 26.0;
            s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));

            let dark = theme == Theme::Dark;
            let v = &mut s.visuals;
            v.selection.bg_fill = ACCENT;
            v.selection.stroke = Stroke::new(1.0_f32, ON_ACCENT);
            v.hyperlink_color = ACCENT;
            v.window_corner_radius = CornerRadius::same(10);
            v.menu_corner_radius = CornerRadius::same(8);
            for w in [
                &mut v.widgets.noninteractive,
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
                &mut v.widgets.open,
            ] {
                w.corner_radius = CornerRadius::same(6);
            }
            v.widgets.inactive.bg_stroke = Stroke::NONE;
            v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.6));
            if dark {
                v.panel_fill = Color32::from_rgb(24, 24, 27);
                v.window_fill = Color32::from_rgb(30, 30, 34);
                v.extreme_bg_color = Color32::from_rgb(16, 16, 18);
                v.faint_bg_color = Color32::from_rgb(34, 34, 39);
                v.widgets.inactive.weak_bg_fill = Color32::from_rgb(40, 40, 46);
                v.widgets.inactive.bg_fill = Color32::from_rgb(40, 40, 46);
                v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(44, 44, 50));
            } else {
                v.panel_fill = Color32::from_rgb(252, 251, 249);
                v.window_fill = Color32::WHITE;
                v.extreme_bg_color = Color32::WHITE;
                v.faint_bg_color = Color32::from_rgb(243, 241, 237);
                v.widgets.inactive.weak_bg_fill = Color32::from_rgb(236, 233, 228);
                v.widgets.inactive.bg_fill = Color32::from_rgb(236, 233, 228);
                v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(228, 224, 218));
            }
        });
    }
}

/// Slightly recessed surface used for the navigation sidebar.
pub fn sidebar_fill(v: &egui::Visuals) -> Color32 {
    if v.dark_mode { Color32::from_rgb(19, 19, 22) } else { Color32::from_rgb(244, 242, 238) }
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    /// Every non-ASCII character written in the sources must have a glyph.
    #[test]
    fn all_symbols_have_glyphs() {
        let ctx = egui::Context::default();
        super::install_fallback_fonts(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        let mut chars = std::collections::BTreeSet::new();
        let mut dirs = vec![std::path::PathBuf::from("src")];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    // Only string literals matter; comments use the same characters anyway.
                    chars.extend(text.chars().filter(|c| !c.is_ascii() && !c.is_alphabetic()));
                }
            }
        }
        let missing: String = ctx.fonts(|f| {
            chars.iter().filter(|&&c| !f.has_glyph(&egui::FontId::proportional(14.0), c)).collect()
        });
        assert!(missing.is_empty(), "glyphes manquants : {missing:?}");
    }
}
