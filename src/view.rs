//! Renders a parsed song: sections, chords over lyrics, and tab staves.
//!
//! Consecutive tab blocks (with their chord lines) are reflowed: their measures are laid
//! end to end and wrapped to the available width, each row stretched to fill it.

use eframe::egui::{self, Align2, CornerRadius, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

use crate::diagram;
use crate::playback::{Playhead, Seek, measures};
use crate::song::{Block, ChordTok, Song, TabLine};

const MAX_STRETCH: f32 = 3.0;
/// Columns are never narrower than this many characters, so strummed bars stay readable.
const MIN_STRETCH: f32 = 1.35;
/// Width of a multi-measure rest, in characters.
const REST_W: f32 = 12.0;

struct Ctx<'a> {
    song: &'a Song,
    size: f32,
    cw: f32,
    lh: f32,
    mono: FontId,
    v: egui::Visuals,
    accent: egui::Color32,
    highlight: Option<usize>,
    play: Option<&'a Playhead>,
    follow: bool,
    width: f32,
}

/// One tab block, optionally with the chord line written above it.
struct StaffItem {
    chords: Option<usize>,
    tab: usize,
}

struct Measure<'a> {
    tab: usize,
    lines: &'a [TabLine],
    c0: f32,
    c1: f32,
    no: usize,
    /// (chord block, token index, column relative to the tab body)
    chords: Vec<(usize, usize, f32)>,
    /// Number of silent measures drawn as one (multi-measure rest), 0 for a normal measure.
    rest: usize,
}

impl Measure<'_> {
    fn silent(&self) -> bool {
        self.chords.is_empty() && self.lines.iter().all(|l| runs(&l.body, self.c0 as usize, self.c1 as usize).is_empty())
    }

    /// Drawn width of the columns, before padding.
    fn width(&self, col_w: f32, cw: f32) -> f32 {
        if self.rest > 0 { REST_W * cw } else { (self.c1 - self.c0) * col_w }
    }
}

/// Draws the song. Returns a seek request when the user clicks a measure or a chord.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    song: &Song,
    size: f32,
    width: f32,
    highlight: Option<usize>,
    hovered: &mut Option<usize>,
    play: Option<&Playhead>,
    follow: bool,
) -> Option<Seek> {
    let mono = FontId::monospace(size);
    let v = ui.visuals().clone();
    let cx = Ctx {
        song,
        size,
        cw: ui.fonts(|f| f.glyph_width(&mono, 'M')),
        lh: size * 1.45,
        accent: v.selection.bg_fill,
        mono,
        v,
        highlight,
        play,
        follow,
        width,
    };
    ui.spacing_mut().item_spacing.y = 0.0;

    let mut seek = None;
    let mut measure_no = 1;
    let mut i = 0;
    while i < song.blocks.len() {
        if let Some((items, next)) = staff_group(song, i) {
            seek = staff(ui, &cx, &items, &mut measure_no, hovered).or(seek);
            i = next;
            continue;
        }
        match &song.blocks[i] {
            Block::Blank => ui.add_space(cx.lh * 0.7),
            Block::Title(t) => {
                ui.label(RichText::new(t).size(size * 1.7).strong());
                ui.add_space(cx.lh * 0.4);
            }
            Block::Section(s) => section(ui, &cx, s),
            Block::Text(t) => {
                let w = t.chars().count() as f32 * cx.cw;
                let (rect, _) = ui.allocate_exact_size(vec2(w.max(1.0), cx.lh), Sense::hover());
                ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, t, cx.mono.clone(), cx.v.text_color());
            }
            Block::Chords(toks) => seek = chord_line(ui, &cx, i, toks, hovered).or(seek),
            Block::Tab(_) => unreachable!("tabs are drawn by staff groups"),
        }
        i += 1;
    }
    seek
}

fn section(ui: &mut egui::Ui, cx: &Ctx, s: &str) {
    ui.add_space(cx.lh * 0.35);
    let font = FontId::proportional(cx.size * 0.85);
    let galley = ui.painter().layout_no_wrap(s.to_uppercase(), font, cx.accent);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(20.0, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), cx.accent.gamma_multiply(0.15));
    ui.painter().galley(rect.min + vec2(10.0, 4.0), galley, cx.accent);
    ui.add_space(cx.lh * 0.35);
}

/// Collects consecutive `[chord line] + tab` pairs starting at `start`, blank lines between them allowed.
fn staff_group(song: &Song, start: usize) -> Option<(Vec<StaffItem>, usize)> {
    let b = &song.blocks;
    let mut items = Vec::new();
    let mut i = start;
    loop {
        let mut j = i;
        while !items.is_empty() && matches!(b.get(j), Some(Block::Blank)) {
            j += 1;
        }
        match (b.get(j), b.get(j + 1)) {
            (Some(Block::Chords(_)), Some(Block::Tab(_))) => {
                items.push(StaffItem { chords: Some(j), tab: j + 1 });
                i = j + 2;
            }
            (Some(Block::Tab(_)), _) => {
                items.push(StaffItem { chords: None, tab: j });
                i = j + 1;
            }
            _ => break,
        }
    }
    (!items.is_empty()).then_some((items, i))
}

/// A note or symbol run inside a measure, e.g. "12", "/3", "1/2".
fn runs(body: &[char], c0: usize, c1: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut j = c0;
    let end = c1.min(body.len());
    while j < end {
        if matches!(body[j], '-' | '|') {
            j += 1;
            continue;
        }
        let len = body[j..end].iter().take_while(|c| !matches!(c, '-' | '|')).count();
        out.push((j, body[j..j + len].iter().collect()));
        j += len;
    }
    out
}

fn staff(ui: &mut egui::Ui, cx: &Ctx, items: &[StaffItem], measure_no: &mut usize, hovered: &mut Option<usize>) -> Option<Seek> {
    let song = cx.song;
    let mut all: Vec<Measure> = Vec::new();
    let mut notes: Vec<&str> = Vec::new();
    for it in items {
        let Block::Tab(lines) = &song.blocks[it.tab] else { continue };
        let offset = lines[0].offset as f32;
        let toks: &[ChordTok] = match it.chords.map(|c| &song.blocks[c]) {
            Some(Block::Chords(t)) => t,
            _ => &[],
        };
        notes.extend(lines.iter().filter(|l| !l.note.is_empty()).map(|l| l.note.as_str()));
        for (c0, c1) in measures(lines) {
            let chords = toks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.idx.is_some())
                .map(|(k, t)| (it.chords.unwrap(), k, t.col as f32 - offset))
                .filter(|(_, _, rel)| *rel >= c0 - 1.0 && *rel < c1 - 1.0)
                .collect();
            let m = Measure { tab: it.tab, lines, c0, c1, no: *measure_no, chords, rest: 0 };
            *measure_no += 1;
            // Silent measures in a row (an intro to wait through) merge into one.
            if m.silent() {
                if let Some(prev) = all.last_mut().filter(|p| p.tab == m.tab && p.c1 == m.c0 && (p.rest > 0 || p.silent())) {
                    prev.c1 = m.c1;
                    prev.rest = prev.rest.max(1) + 1;
                    continue;
                }
            }
            all.push(m);
        }
    }

    // Pack measures into rows that fit the width.
    let label_w = 2.0 * cx.cw;
    let avail = (cx.width - label_w).max(cx.cw * 16.0);
    let pad = cx.cw * 0.9;
    let mut rows: Vec<Vec<&Measure>> = Vec::new();
    let mut row_w = 0.0;
    for m in &all {
        let w = m.width(cx.cw * MIN_STRETCH, cx.cw) + 2.0 * pad;
        let same_strings = rows.last().and_then(|r| r.first()).is_none_or(|f| f.lines.len() == m.lines.len());
        match rows.last_mut() {
            Some(r) if row_w + w <= avail && same_strings => {
                r.push(m);
                row_w += w;
            }
            _ => {
                rows.push(vec![m]);
                row_w = w;
            }
        }
    }

    let mut seek = None;
    let mut prev_scale: Option<f32> = None;
    let n_rows = rows.len();
    for (ri, row) in rows.iter().enumerate() {
        let fixed: f32 = row.iter().filter(|m| m.rest > 0).map(|m| m.width(0.0, cx.cw)).sum();
        let natural: f32 = row.iter().filter(|m| m.rest == 0).map(|m| (m.c1 - m.c0) * cx.cw).sum();
        let fill = ((avail - fixed - row.len() as f32 * 2.0 * pad) / natural.max(1.0)).clamp(MIN_STRETCH, MAX_STRETCH);
        // The last row keeps the previous rows' spacing instead of stretching to the edge.
        let scale = if ri + 1 == n_rows { prev_scale.map_or(fill.min(1.6), |p| p.min(fill)) } else { fill };
        prev_scale = Some(scale);
        seek = staff_row(ui, cx, row, label_w, pad, scale, hovered).or(seek);
    }
    for n in notes {
        ui.label(RichText::new(n).monospace().size(cx.size * 0.85).weak());
    }
    ui.add_space(cx.lh * 0.3);
    seek
}

fn staff_row(
    ui: &mut egui::Ui,
    cx: &Ctx,
    row: &[&Measure],
    label_w: f32,
    pad: f32,
    scale: f32,
    hovered: &mut Option<usize>,
) -> Option<Seek> {
    let (cw, size, v, accent) = (cx.cw, cx.size, &cx.v, cx.accent);
    let th = size * 1.15;
    let n = row[0].lines.len();
    let chord_h = if row.iter().any(|m| !m.chords.is_empty()) { cx.lh } else { 0.0 };
    let num_h = size * 0.75;
    let staff_h = th * n as f32;
    let col_w = cw * scale;
    let row_w: f32 = row.iter().map(|m| m.width(col_w, cw) + 2.0 * pad).sum();
    let (rect, resp) = ui.allocate_exact_size(vec2(label_w + row_w, chord_h + num_h + staff_h + cx.lh * 0.6), Sense::click());
    let top = rect.top() + chord_h + num_h;
    let left = rect.left() + label_w;
    let y = |r: usize| top + (r as f32 + 0.5) * th;
    let (y_first, y_last) = (y(0), y(n - 1));
    let p = ui.painter().clone();

    let line = Stroke::new(1.0_f32, v.weak_text_color().gamma_multiply(0.6));
    let bar = Stroke::new(1.2_f32, v.text_color().gamma_multiply(0.8));
    for (r, l) in row[0].lines.iter().enumerate() {
        p.text(pos2(left - 4.0, y(r)), Align2::RIGHT_CENTER, &l.label, FontId::monospace(size * 0.75), v.weak_text_color());
        p.line_segment([pos2(left, y(r)), pos2(left + row_w, y(r))], line);
    }
    p.line_segment([pos2(left + row_w, y_first), pos2(left + row_w, y_last)], bar);

    let head = cx.play.and_then(|h| Some((h.block, h.col?)));
    let mut seek = None;
    let mut x0 = left;
    for (mi, m) in row.iter().enumerate() {
        let mw = m.width(col_w, cw) + 2.0 * pad;
        // A merged rest squeezes its columns into its fixed width.
        let col_w = if m.rest > 0 { m.width(col_w, cw) / (m.c1 - m.c0) } else { col_w };
        let col_x = |col: f32| x0 + pad + (col - m.c0 + 0.5) * col_w;
        p.line_segment([pos2(x0, y_first), pos2(x0, y_last)], bar);
        p.text(pos2(x0 + 2.0, top - 2.0), Align2::LEFT_BOTTOM, m.no.to_string(), FontId::proportional(size * 0.6), v.weak_text_color());

        let h = head.filter(|(b, c)| *b == m.tab && *c >= m.c0 && *c < m.c1).map(|(_, c)| c);
        if m.rest > 0 {
            // Multi-measure rest: a thick bar with the number of measures to wait.
            let (a, b) = (x0 + pad + cw, x0 + mw - pad - cw);
            let mid = (y_first + y_last) / 2.0;
            let ink = v.text_color().gamma_multiply(0.85);
            p.rect_filled(Rect::from_x_y_ranges(a..=b, mid - th * 0.18..=mid + th * 0.18), CornerRadius::same(1), ink);
            for x in [a, b] {
                p.line_segment([pos2(x, mid - th * 0.6), pos2(x, mid + th * 0.6)], Stroke::new(1.5_f32, ink));
            }
            p.text(pos2((a + b) / 2.0, y_first - th * 0.2), Align2::CENTER_BOTTOM, m.rest.to_string(), FontId::proportional(size * 1.05), v.strong_text_color());
        }
        for (r, l) in m.lines.iter().enumerate() {
            for (j, text) in runs(&l.body, m.c0 as usize, m.c1 as usize) {
                let len = text.chars().count() as f32;
                let lit = h.is_some_and(|h| h >= j as f32 && h < j as f32 + len + 1.0);
                let r0 = Rect::from_min_size(pos2(col_x(j as f32) - cw / 2.0, y(r) - size / 2.0), vec2(cw * len, size));
                let (bg, ink) = if lit { (accent, v.selection.stroke.color) } else { (v.panel_fill, v.strong_text_color()) };
                p.rect_filled(r0.expand2(vec2(1.0, 1.0)), CornerRadius::same(if lit { 3 } else { 0 }), bg);
                p.text(r0.left_center(), Align2::LEFT_CENTER, &text, cx.mono.clone(), ink);
            }
        }
        if let Some(h) = h {
            let hx = x0 + pad + (h - m.c0) * col_w;
            p.line_segment([pos2(hx, y_first - th / 2.0), pos2(hx, y_last + th / 2.0)], Stroke::new(2.0_f32, accent));
            if cx.follow {
                ui.scroll_to_rect(rect.expand(cx.lh * 2.0), Some(egui::Align::Center));
            }
        }

        for &(cb, k, rel) in &m.chords {
            let Block::Chords(toks) = &cx.song.blocks[cb] else { continue };
            let tok = &toks[k];
            let Some(idx) = tok.idx else { continue };
            let x = col_x(rel.max(m.c0)) - cw / 2.0;
            let tw = tok.text.chars().count() as f32 * cw;
            let chip = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(tw, cx.lh - 4.0)).expand2(vec2(3.0, 0.0));
            let resp = ui.interact(chip, ui.id().with(("staff-chord", cb, k, mi)), Sense::click());
            if resp.clicked() {
                seek = Some(Seek::Col { block: m.tab, col: rel.max(m.c0) });
            }
            if resp.hovered() {
                *hovered = Some(idx);
            }
            let playing = cx.play.and_then(|p| p.tok) == Some((cb, k));
            chip_paint(&p, cx, chip, &tok.text, playing || cx.highlight == Some(idx), playing);
            resp.on_hover_ui(|ui| {
                diagram::card_info(ui, &cx.song.chords[idx], false);
            });
        }

        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked()) {
            if pos.x >= x0 && pos.x < x0 + mw {
                let col = m.c0 + ((pos.x - x0 - pad) / col_w).max(0.0);
                seek = Some(Seek::Col { block: m.tab, col: col.min(m.c1 - 0.01) });
            }
        }
        x0 += mw;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    seek
}

fn chip_paint(p: &egui::Painter, cx: &Ctx, chip: Rect, text: &str, active: bool, playing: bool) {
    let (fill, ink) = if active {
        (cx.accent, cx.v.selection.stroke.color)
    } else {
        (cx.accent.gamma_multiply(0.16), cx.v.strong_text_color())
    };
    if playing {
        p.rect_filled(chip.expand(3.0), CornerRadius::same(6), cx.accent.gamma_multiply(0.3));
    }
    p.rect_filled(chip, CornerRadius::same(4), fill);
    p.text(pos2(chip.left() + 3.0, chip.center().y), Align2::LEFT_CENTER, text, cx.mono.clone(), ink);
}

fn chord_line(ui: &mut egui::Ui, cx: &Ctx, bi: usize, toks: &[ChordTok], hovered: &mut Option<usize>) -> Option<Seek> {
    let cw = cx.cw;
    let mut seek = None;
    let playing_tok = cx.play.and_then(|p| p.tok).filter(|t| t.0 == bi).map(|t| t.1);
    let w = toks.last().map_or(0, |t| t.col + t.text.chars().count()) as f32 * cw;
    let (rect, _) = ui.allocate_exact_size(vec2(w + 4.0, cx.lh), Sense::hover());
    for (i, tok) in toks.iter().enumerate() {
        let x = rect.left() + tok.col as f32 * cw;
        let tw = tok.text.chars().count() as f32 * cw;
        let chip = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(tw, cx.lh - 4.0)).expand2(vec2(3.0, 0.0));
        let Some(idx) = tok.idx else {
            ui.painter().text(pos2(x, rect.center().y), Align2::LEFT_CENTER, &tok.text, cx.mono.clone(), cx.v.weak_text_color());
            continue;
        };
        let resp = ui.interact(chip, ui.id().with(("chord", bi, i)), Sense::click());
        if resp.clicked() {
            seek = Some(Seek::Tok { block: bi, tok: i });
        }
        let playing = playing_tok == Some(i);
        if playing && cx.follow {
            ui.scroll_to_rect(rect.expand(cx.lh * 3.0), Some(egui::Align::Center));
        }
        if resp.hovered() {
            *hovered = Some(idx);
        }
        chip_paint(ui.painter(), cx, chip, &tok.text, playing || cx.highlight == Some(idx), playing);
        resp.on_hover_ui(|ui| {
            diagram::card_info(ui, &cx.song.chords[idx], false);
        });
    }
    seek
}
