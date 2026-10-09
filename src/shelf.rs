//! "Tablatures" page: the list of tabs stored in the library folder.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, CornerRadius, FontId, RichText, Sense, vec2};

use crate::song::{self, Block};
use crate::theme::ACCENT;
use crate::widgets::{Icon, paint_icon, star_at};

const EXTENSIONS: &[&str] = &["txt", "tab", "crd", "chopro", "cho", "pro"];
const RESCAN_EVERY: Duration = Duration::from_secs(3);

pub struct Entry {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub chords: Vec<String>,
    pub tempo: Option<f32>,
    pub has_tab: bool,
    /// 1 (beginner) to 5 (advanced).
    pub level: u8,
}

pub const LEVELS: [&str; 5] = ["Débutant", "Facile", "Moyen", "Difficile", "Avancé"];

/// Beginner-oriented difficulty: hardest chords, number of chords, and how many notes per
/// second a tab asks for.
pub fn level(song: &song::Song) -> u8 {
    let mut hard: Vec<f32> = song.chords.iter().map(|c| c.voicing.as_ref().map_or(9.0, crate::chords::difficulty)).collect();
    hard.sort_by(|a, b| b.total_cmp(a));
    let top = hard.first().copied().unwrap_or(0.0);
    let mut chords = match top {
        t if t <= 2.5 => 1,
        t if t <= 4.0 => 2,
        t if t <= 6.0 => 3,
        t if t <= 8.0 => 4,
        _ => 5,
    };
    if hard.is_empty() {
        chords = 1;
    }
    if song.chords.len() > 8 {
        chords += 1;
    }
    let mut tab = 1;
    if song.blocks.iter().any(|b| matches!(b, Block::Tab(_))) {
        let bpb = song.beats_per_bar.unwrap_or(4.0);
        let t = crate::playback::Timeline::build(song, bpb, 4.0);
        let tempo = song.tempo.unwrap_or(90.0);
        let tab_beats: f32 = t.steps.iter().filter(|s| matches!(s.kind, crate::playback::StepKind::Measure { .. })).map(|s| s.beats).sum();
        let notes_per_sec = if tab_beats > 0.0 { t.onsets.len() as f32 / (tab_beats * 60.0 / tempo) } else { 0.0 };
        tab = match notes_per_sec {
            n if n <= 1.2 => 1,
            n if n <= 2.2 => 2,
            n if n <= 3.5 => 3,
            n if n <= 5.0 => 4,
            _ => 5,
        };
    }
    chords.max(tab).clamp(1, 5)
}

pub enum Action {
    Open(PathBuf),
    New,
    Import,
    Songsterr,
    /// A tab was moved to the Trash.
    Deleted(PathBuf),
}

pub struct Shelf {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    /// File names of favorite tabs.
    pub favorites: BTreeSet<String>,
    only_favorites: bool,
    by_level: bool,
    query: String,
    scanned: Option<Instant>,
    /// Tab awaiting delete confirmation.
    confirm_delete: Option<PathBuf>,
    error: Option<String>,
}

pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join("Documents").join("Till")
}

fn summarize(path: &Path) -> Option<Entry> {
    let text = std::fs::read(path).ok()?;
    let song = song::parse(&String::from_utf8_lossy(&text), 0);
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let (file_title, artist) = match stem.split_once(" - ") {
        Some((t, a)) => (t.trim().to_string(), a.trim().to_string()),
        None => (stem.clone(), String::new()),
    };
    let title = song
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Title(t) if !t.is_empty() => Some(t.clone()),
            _ => None,
        })
        .unwrap_or(file_title);
    Some(Entry {
        path: path.to_path_buf(),
        title,
        artist,
        chords: song.chords.iter().map(|c| c.name.clone()).collect(),
        tempo: song.tempo,
        has_tab: song.blocks.iter().any(|b| matches!(b, Block::Tab(_))),
        level: level(&song),
    })
}

impl Shelf {
    pub fn new(dir: PathBuf) -> Shelf {
        let _ = std::fs::create_dir_all(&dir);
        let mut s = Shelf { dir, entries: Vec::new(), favorites: BTreeSet::new(), only_favorites: false, by_level: false, query: String::new(), scanned: None, confirm_delete: None, error: None };
        s.scan();
        s
    }

    pub fn scan(&mut self) {
        let mut entries: Vec<Entry> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
            })
            .filter_map(|p| summarize(&p))
            .collect();
        entries.sort_by_key(|e| e.title.to_lowercase());
        self.entries = entries;
        self.scanned = Some(Instant::now());
    }

    /// Copies a file into the library (unless it is already there) and returns its new path.
    pub fn import(&mut self, src: &Path) -> std::io::Result<PathBuf> {
        if src.parent() == Some(self.dir.as_path()) {
            return Ok(src.to_path_buf());
        }
        let name = src.file_name().ok_or_else(|| std::io::Error::other("nom de fichier invalide"))?;
        let mut dest = self.dir.join(name);
        let mut n = 2;
        while dest.exists() {
            let stem = src.file_stem().unwrap_or_default().to_string_lossy();
            let ext = src.extension().map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
            dest = self.dir.join(format!("{stem} ({n}){ext}"));
            n += 1;
        }
        std::fs::copy(src, &dest)?;
        self.scan();
        Ok(dest)
    }

    /// Writes a new file into the library under a free name and returns its path.
    pub fn add(&mut self, name: &str, text: &str) -> std::io::Result<PathBuf> {
        let base = Path::new(name);
        let stem = base.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let ext = base.extension().map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
        let mut dest = self.dir.join(name);
        let mut n = 2;
        while dest.exists() {
            dest = self.dir.join(format!("{stem} ({n}){ext}"));
            n += 1;
        }
        std::fs::write(&dest, text)?;
        self.scan();
        Ok(dest)
    }

    pub fn is_favorite(&self, path: &Path) -> bool {
        path.file_name().is_some_and(|n| self.favorites.contains(&*n.to_string_lossy()))
    }

    pub fn toggle_favorite(&mut self, path: &Path) {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
        if !self.favorites.remove(&name) {
            self.favorites.insert(name);
        }
    }

    /// Favorite tabs that still exist, in library order.
    pub fn favorite_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| self.is_favorite(&e.path))
    }

    /// Moves a tab to the Trash (it can be restored from the Finder).
    pub fn delete(&mut self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| format!("Impossible de supprimer : {e}"))?;
        if let Some(name) = path.file_name() {
            self.favorites.remove(&*name.to_string_lossy());
        }
        self.scan();
        Ok(())
    }

    fn confirm_dialog(&mut self, ui: &egui::Ui) -> Option<Action> {
        let path = self.confirm_delete.clone()?;
        let title = self.entries.iter().find(|e| e.path == path).map_or_else(|| path.display().to_string(), |e| e.title.clone());
        let mut action = None;
        let modal = egui::Modal::new(egui::Id::new("confirm_delete")).show(ui.ctx(), |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(format!("Supprimer « {title} » ?")).size(17.0).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("Le fichier part dans la Corbeille : tu pourras le récupérer depuis le Finder.").weak());
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let del = egui::Button::new(RichText::new("Supprimer").color(ui.visuals().selection.stroke.color)).fill(ACCENT);
                if ui.add(del).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    match self.delete(&path) {
                        Ok(()) => action = Some(Action::Deleted(path.clone())),
                        Err(e) => self.error = Some(e),
                    }
                    self.confirm_delete = None;
                }
                if ui.button("Annuler").clicked() {
                    self.confirm_delete = None;
                }
            });
        });
        if modal.should_close() {
            self.confirm_delete = None;
        }
        action
    }

    pub fn show(&mut self, ui: &mut egui::Ui, current: Option<&Path>) -> Option<Action> {
        if self.scanned.is_none_or(|t| t.elapsed() > RESCAN_EVERY) {
            self.scan();
        }
        let mut action = self.confirm_dialog(ui);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 20)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(RichText::new("Tablatures").strong());
                        let n = self.entries.len();
                        ui.label(RichText::new(format!("{n} morceau{} · {}", if n > 1 { "x" } else { "" }, self.dir.display())).weak());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dossier").on_hover_text("Ouvrir le dossier dans le Finder").clicked() {
                            let _ = std::process::Command::new("open").arg(&self.dir).spawn();
                        }
                        if ui.button("Songsterr…").on_hover_text("Importer une tab depuis un lien Songsterr").clicked() {
                            action = Some(Action::Songsterr);
                        }
                        if ui.button("Importer…").on_hover_text("Copier un fichier dans la bibliothèque (Cmd+O)").clicked() {
                            action = Some(Action::Import);
                        }
                        let new = egui::Button::new(RichText::new("Nouvelle tab").color(ui.visuals().selection.stroke.color)).fill(ACCENT);
                        if ui.add(new).on_hover_text("Cmd+N").clicked() {
                            action = Some(Action::New);
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text("Rechercher un titre, un artiste…")
                                .desired_width(240.0)
                                .margin(vec2(10.0, 6.0)),
                        );
                    });
                });
                if let Some(e) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                ui.add_space(12.0);
                let favs = self.favorite_entries().count();
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.only_favorites, false, format!("Toutes ({})", self.entries.len()));
                    ui.selectable_value(&mut self.only_favorites, true, format!("Favoris ({favs})"));
                    ui.separator();
                    ui.label(RichText::new("Trier par").weak());
                    ui.selectable_value(&mut self.by_level, false, "Titre");
                    ui.selectable_value(&mut self.by_level, true, "Difficulté");
                });
                ui.add_space(12.0);

                if self.entries.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Ta bibliothèque est vide").size(18.0).strong());
                        ui.label(RichText::new("Importe un fichier .txt, glisse-le dans la fenêtre, ou crée une nouvelle tab.").weak());
                    });
                    return;
                }

                let q = self.query.to_lowercase();
                let mut shown = 0;
                let mut toggle = None;
                let mut order: Vec<&Entry> = self.entries.iter().collect();
                if self.by_level {
                    order.sort_by_key(|e| e.level);
                }
                for e in order {
                    if !q.is_empty() && !format!("{} {}", e.title, e.artist).to_lowercase().contains(&q) {
                        continue;
                    }
                    let fav = self.is_favorite(&e.path);
                    if self.only_favorites && !fav {
                        continue;
                    }
                    shown += 1;
                    match row(ui, e, current == Some(e.path.as_path()), fav) {
                        RowClick::Open => action = Some(Action::Open(e.path.clone())),
                        RowClick::Star => toggle = Some(e.path.clone()),
                        RowClick::Delete => self.confirm_delete = Some(e.path.clone()),
                        RowClick::Finder => {
                            let _ = std::process::Command::new("open").arg("-R").arg(&e.path).spawn();
                        }
                        RowClick::None => {}
                    }
                    ui.add_space(6.0);
                }
                if let Some(p) = toggle {
                    self.toggle_favorite(&p);
                }
                if shown == 0 {
                    let msg = if self.only_favorites && q.is_empty() {
                        "Aucun favori pour l'instant : clique sur l'étoile d'une tab pour l'ajouter."
                    } else {
                        "Aucun résultat"
                    };
                    ui.label(RichText::new(msg).weak());
                }
            });
        });
        action
    }
}

enum RowClick {
    None,
    Open,
    Star,
    Delete,
    Finder,
}

fn row(ui: &mut egui::Ui, e: &Entry, current: bool, fav: bool) -> RowClick {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    let v = ui.visuals().clone();
    let p = ui.painter();
    let fill = if resp.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        v.faint_bg_color
    };
    p.rect_filled(rect, CornerRadius::same(12), fill);
    if current {
        p.rect_stroke(rect, CornerRadius::same(12), egui::Stroke::new(1.5_f32, ACCENT), egui::StrokeKind::Inside);
    }

    // Icon tile
    let tile = egui::Rect::from_center_size(egui::pos2(rect.left() + 34.0, rect.center().y), vec2(40.0, 40.0));
    p.rect_filled(tile, CornerRadius::same(10), ACCENT.gamma_multiply(0.18));
    paint_icon(p, if e.has_tab { Icon::Tab } else { Icon::Chords }, tile.center(), ACCENT);

    let x = rect.left() + 68.0;
    p.text(egui::pos2(x, rect.top() + 14.0), egui::Align2::LEFT_TOP, &e.title, FontId::proportional(16.0), v.strong_text_color());
    let mut sub = if e.artist.is_empty() { "Artiste inconnu".to_string() } else { e.artist.clone() };
    if let Some(t) = e.tempo {
        sub += &format!("  ·  {t:.0} BPM");
    }
    sub += if e.has_tab { "  ·  Tab" } else { "  ·  Accords" };
    sub += &format!("  ·  {}", LEVELS[(e.level as usize).clamp(1, 5) - 1]);
    p.text(egui::pos2(x, rect.top() + 36.0), egui::Align2::LEFT_TOP, sub, FontId::proportional(12.5), v.weak_text_color());

    // Favorite star at the far right, then chord chips.
    let star_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - 28.0, rect.center().y), vec2(32.0, 32.0));
    let star_id = ui.id().with(("fav", &e.path));
    let star = star_at(ui, star_rect, star_id, fav);
    // Trash, shown while hovering the row.
    let trash_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - 62.0, rect.center().y), vec2(30.0, 30.0));
    let trash = ui.interact(trash_rect, ui.id().with(("trash", &e.path)), Sense::click()).on_hover_text("Supprimer (clic droit pour plus d'options)");
    let row_hovered = ui.rect_contains_pointer(rect);
    if row_hovered {
        let p = ui.painter();
        if trash.hovered() {
            p.rect_filled(trash_rect, CornerRadius::same(8), v.widgets.hovered.bg_fill);
        }
        let col = if trash.hovered() { v.error_fg_color } else { v.weak_text_color() };
        paint_icon(p, Icon::Trash, trash_rect.center(), col);
    }
    let mut menu = RowClick::None;
    resp.context_menu(|ui| {
        if ui.button("Ouvrir").clicked() {
            menu = RowClick::Open;
            ui.close_menu();
        }
        if ui.button(if fav { "Retirer des favoris" } else { "Ajouter aux favoris" }).clicked() {
            menu = RowClick::Star;
            ui.close_menu();
        }
        if ui.button("Afficher dans le Finder").clicked() {
            menu = RowClick::Finder;
            ui.close_menu();
        }
        ui.separator();
        if ui.button(RichText::new("Supprimer…").color(ui.visuals().error_fg_color)).clicked() {
            menu = RowClick::Delete;
            ui.close_menu();
        }
    });
    let p = ui.painter();
    let mut cx = rect.right() - 86.0;
    for name in e.chords.iter().take(8).rev() {
        let g = p.layout_no_wrap(name.clone(), FontId::monospace(12.0), v.strong_text_color());
        let w = g.size().x + 12.0;
        if cx - w < x + 260.0 {
            break;
        }
        let chip = egui::Rect::from_min_size(egui::pos2(cx - w, rect.center().y - 11.0), vec2(w, 22.0));
        p.rect_filled(chip, CornerRadius::same(6), ACCENT.gamma_multiply(0.14));
        p.galley(chip.center() - g.size() / 2.0, g, v.strong_text_color());
        cx -= w + 6.0;
    }
    if resp.hovered() && !star.hovered() && !trash.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !matches!(menu, RowClick::None) {
        menu
    } else if trash.clicked() {
        RowClick::Delete
    } else if star.clicked() {
        RowClick::Star
    } else if resp.clicked() {
        RowClick::Open
    } else {
        RowClick::None
    }
}
