//! Video tutorials: YouTube links saved by the user, watched in the app.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};

use crate::chords::{Chord, QUALITIES, slash_chords, voicing};
use crate::theme::ACCENT;
use crate::{clip, diagram, score, video};
use crate::widgets::{Icon, icon_button, nav_item};

/// Speeds offered by YouTube's player.
const SPEEDS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

#[derive(Clone, Debug, PartialEq)]
pub struct Tuto {
    pub id: String,
    pub title: String,
    /// Named timecodes, by time.
    pub marks: Vec<Mark>,
    /// Folder name, empty when the video is in no folder.
    pub folder: String,
    /// Chords played in the video, by name.
    pub chords: Vec<String>,
    /// Imported score, in the library's `Partitions` folder.
    pub pdf: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    /// Seconds from the start of the video.
    pub t: f32,
    pub name: String,
}

#[derive(Default)]
pub struct Tutos {
    pub list: Vec<Tuto>,
    current: Option<String>,
    url: String,
    error: Option<String>,
    player: Option<video::Player>,
    /// Where the player goes this frame.
    rect: Option<egui::Rect>,
    speed: f32,
    /// Titles fetched in the background: (video id, title).
    titles: Arc<Mutex<Vec<(String, String)>>>,
    /// Folder names, in display order (empty folders included).
    folders: Vec<String>,
    renaming: Option<Renaming>,
    /// The rename field must take the focus.
    rename_focus: bool,
    chord_input: String,
    /// Highlighted suggestion of the chord field.
    chord_sel: usize,
    /// Where the suggestions were last shown: they stay while the mouse is on them.
    suggest_rect: Option<egui::Rect>,
    /// Under the video: the score rather than the chords.
    score_tab: bool,
    /// Visible part of the scrolling page this frame: the web views are clipped to it.
    viewport: Option<egui::Rect>,
    clip: Option<clip::Clip>,
    score: Option<score::Score>,
    /// A mark was just added: its name field takes the focus.
    focus_mark: Option<usize>,
}

/// The YouTube video id of a link (watch, youtu.be, shorts, embed, live) or of a bare id.
pub fn parse_id(text: &str) -> Option<String> {
    let t = text.trim();
    let valid = |s: &str| s.len() == 11 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid(t) {
        return Some(t.to_string());
    }
    let rest = t.split_once("://").map_or(t, |(_, r)| r);
    let (host, path) = rest.split_once('/')?;
    let host = ["www.", "m.", "music."].iter().fold(host, |h, p| h.strip_prefix(p).unwrap_or(h));
    let end = |s: &str| s.split(['?', '&', '#', '/']).next().map(String::from);
    let id = match host {
        "youtu.be" => end(path),
        "youtube.com" | "youtube-nocookie.com" => match path.strip_prefix("watch") {
            Some(q) => q.split(['?', '&', '#']).find_map(|kv| kv.strip_prefix("v=")).and_then(end),
            None => ["shorts/", "embed/", "live/", "v/"].iter().find_map(|p| path.strip_prefix(p)).and_then(end),
        },
        _ => None,
    }?;
    valid(&id).then_some(id)
}

/// A name being edited in the list: (video id or folder name, new name).
enum Renaming {
    Tuto(String, String),
    Folder(String, String),
}

/// What the list asks for this frame.
enum Act {
    Pick(String),
    Rename(String),
    Remove(String),
    Move(String, String),
    /// New folder, with a video to put in it.
    NewFolder(Option<String>),
    RenameFolder(String),
    DeleteFolder(String),
}

/// An inline name editor: `Some(true)` to keep the new name, `Some(false)` to cancel.
fn rename_field(ui: &mut egui::Ui, buf: &mut String, focus: &mut bool) -> Option<bool> {
    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
    if std::mem::take(focus) {
        r.request_focus();
    }
    r.lost_focus().then(|| !ui.input(|i| i.key_pressed(egui::Key::Escape)) && !buf.trim().is_empty())
}

/// Chord names separated by commas or spaces, in English or French (`Am, Sol7, la mineur`).
pub fn parse_chords(text: &str) -> Result<Vec<Chord>, String> {
    let mut out = Vec::new();
    for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(c) = Chord::parse_loose(part) {
            out.push(c);
            continue;
        }
        for word in part.split_whitespace() {
            out.push(Chord::parse_loose(word).ok_or_else(|| format!("Accord inconnu : {word}"))?);
        }
    }
    Ok(out)
}

/// Lowercase, without accents: `Ré` and `re` compare equal.
fn fold(s: &str) -> String {
    s.to_lowercase().replace(['é', 'è', 'ê'], "e")
}

/// Chords whose English or French name starts like `typed`: the exact one first,
/// then favorites, then the common qualities.
pub fn suggest(typed: &str, favorites: &[String]) -> Vec<Chord> {
    let t = fold(typed.trim());
    if t.is_empty() {
        return Vec::new();
    }
    let mut all: Vec<(usize, Chord)> = QUALITIES
        .iter()
        .enumerate()
        .flat_map(|(qi, q)| (0..12).map(move |pc| (qi, Chord::from_parts(pc, q))))
        .chain(slash_chords().into_iter().map(|c| (QUALITIES.len(), c)))
        .filter(|(_, c)| [c.name(), c.name_fr(), c.short_fr()].iter().any(|n| fold(n).starts_with(&t)))
        .collect();
    all.sort_by_key(|(qi, c)| (!favorites.contains(&c.name()), *qi, c.name().len()));
    let mut out: Vec<Chord> = Chord::parse_loose(typed.trim()).into_iter().collect();
    for (_, c) in all {
        if !out.iter().any(|o| o.name() == c.name()) {
            out.push(c);
        }
    }
    out.truncate(8);
    out
}

/// The chord being typed at the end of the field: where it starts, and its suggestions.
fn last_token(input: &str, favorites: &[String]) -> (usize, Vec<Chord>) {
    let after_comma = input.rfind([',', ';']).map_or(0, |i| i + 1);
    let s = suggest(&input[after_comma..], favorites);
    if !s.is_empty() {
        return (after_comma, s);
    }
    let after_space = input.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    (after_space, suggest(&input[after_space..], favorites))
}

/// Copies a PDF into `dir`, under a name not taken yet.
fn copy_pdf(src: &Path, dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stem = src.file_stem().map_or("Partition".into(), |s| s.to_string_lossy().into_owned());
    let dest = (1..)
        .map(|k| dir.join(if k == 1 { format!("{stem}.pdf") } else { format!("{stem} ({k}).pdf") }))
        .find(|p| !p.exists())
        .unwrap();
    std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// `1:05` or `1:02:03` for a time in seconds.
pub fn format_time(t: f32) -> String {
    let s = t.max(0.0).round() as u32;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// Reads `1:05`, `1:02:03` or a number of seconds.
pub fn parse_time(text: &str) -> Option<f32> {
    text.trim().split(':').try_fold(0.0, |acc, p| p.trim().parse::<f32>().ok().filter(|v| *v >= 0.0).map(|v| acc * 60.0 + v))
}

/// One line per video: id, title, then time and name of each mark, tab-separated.
fn encode(t: &Tuto) -> String {
    let clean = |s: &str| s.replace(['\t', '\n'], " ");
    let mut f = vec![t.id.clone(), clean(&t.title)];
    for m in &t.marks {
        f.push(format!("{:.1}", m.t));
        f.push(clean(&m.name));
    }
    f.join("\t")
}

fn decode(line: &str) -> Option<Tuto> {
    let mut f = line.split('\t');
    let (id, title) = (f.next()?, f.next()?);
    let rest: Vec<&str> = f.collect();
    let marks = rest.chunks(2).filter_map(|c| Some(Mark { t: c.first()?.parse().ok()?, name: c.get(1).unwrap_or(&"").to_string() })).collect();
    Some(Tuto { id: id.to_string(), title: title.to_string(), marks, folder: String::new(), chords: Vec::new(), pdf: None })
}

/// The video title, from YouTube's oEmbed endpoint.
fn fetch_title(id: &str) -> Option<String> {
    let url = format!("https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v={id}&format=json");
    let body = ureq::get(&url).call().ok()?.into_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    v["title"].as_str().map(String::from)
}

impl Tutos {
    pub fn load(get: &dyn Fn(&str) -> Option<String>) -> Tutos {
        let mut list: Vec<Tuto> = get("tutos")
            .unwrap_or_default()
            .lines()
            .filter_map(decode)
            .collect();
        // One line per folder: its name, then the ids of its videos.
        let mut folders = Vec::new();
        for line in get("tuto_folders").unwrap_or_default().lines() {
            let mut f = line.split('\t');
            let Some(name) = f.next().filter(|n| !n.is_empty()) else { continue };
            for id in f {
                if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                    t.folder = name.to_string();
                }
            }
            folders.push(name.to_string());
        }
        // One line per video: its id, then its chords.
        for line in get("tuto_chords").unwrap_or_default().lines() {
            let mut f = line.split('\t');
            if let Some(t) = f.next().and_then(|id| list.iter_mut().find(|t| t.id == id)) {
                t.chords = f.map(String::from).collect();
            }
        }
        for (id, path) in get("tuto_pdfs").unwrap_or_default().lines().filter_map(|l| l.split_once('\t')) {
            if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                t.pdf = Some(PathBuf::from(path));
            }
        }
        let current = get("tuto").filter(|c| list.iter().any(|t| t.id == *c)).or_else(|| list.first().map(|t| t.id.clone()));
        let score_tab = get("tuto_score_tab").is_some_and(|v| v == "true");
        Tutos { list, current, folders, speed: 1.0, score_tab, ..Default::default() }
    }

    pub fn save(&self) -> Vec<(&'static str, String)> {
        let list: Vec<String> = self.list.iter().map(encode).collect();
        let folders: Vec<String> = self
            .folders
            .iter()
            .map(|f| std::iter::once(f.as_str()).chain(self.list.iter().filter(|t| t.folder == *f).map(|t| t.id.as_str())).collect::<Vec<_>>().join("\t"))
            .collect();
        let chords: Vec<String> = self.list.iter().filter(|t| !t.chords.is_empty()).map(|t| format!("{}\t{}", t.id, t.chords.join("\t"))).collect();
        let pdfs: Vec<String> = self.list.iter().filter_map(|t| Some(format!("{}\t{}", t.id, t.pdf.as_ref()?.display()))).collect();
        vec![
            ("tutos", list.join("\n")),
            ("tuto_folders", folders.join("\n")),
            ("tuto_chords", chords.join("\n")),
            ("tuto_pdfs", pdfs.join("\n")),
            ("tuto", self.current.clone().unwrap_or_default()),
            ("tuto_score_tab", self.score_tab.to_string()),
        ]
    }

    /// Adds a video from a link and selects it. False when the link is not a YouTube video.
    pub fn add(&mut self, url: &str) -> bool {
        let Some(id) = parse_id(url) else {
            self.error = Some("Ce n'est pas un lien de vidéo YouTube.".into());
            return false;
        };
        self.error = None;
        if !self.list.iter().any(|t| t.id == id) {
            self.list.insert(0, Tuto { id: id.clone(), title: format!("Vidéo {id}"), marks: Vec::new(), folder: String::new(), chords: Vec::new(), pdf: None });
            let out = self.titles.clone();
            let fid = id.clone();
            std::thread::spawn(move || {
                if let Some(title) = fetch_title(&fid) {
                    out.lock().unwrap().push((fid, title));
                }
            });
        }
        self.current = Some(id);
        true
    }

    fn remove(&mut self, id: &str) {
        let at = self.list.iter().position(|t| t.id == id);
        self.list.retain(|t| t.id != id);
        if self.current.as_deref() == Some(id) {
            self.current = at.and_then(|i| self.list.get(i.min(self.list.len().saturating_sub(1)))).map(|t| t.id.clone());
        }
    }

    /// Notes the current video time as a new mark, whose name is then typed in.
    fn add_mark(&mut self) {
        let (Some(p), Some(id)) = (&self.player, &self.current) else { return };
        let t = (p.time() * 10.0).round() / 10.0;
        let Some(tuto) = self.list.iter_mut().find(|x| x.id == *id) else { return };
        tuto.marks.push(Mark { t, name: String::new() });
        tuto.marks.sort_by(|a, b| a.t.total_cmp(&b.t));
        self.focus_mark = tuto.marks.iter().rposition(|m| m.t == t);
    }

    /// The marks of the current video: go to one, retime it, rename it, delete it.
    fn marks_panel(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.current.as_ref().and_then(|c| self.list.iter().position(|t| t.id == *c)) else { return };
        ui.label(RichText::new("Repères").size(16.0).strong());
        ui.add_space(6.0);
        let ready = self.player.as_ref().is_some_and(|p| p.ready());
        let marks = &mut self.list[i].marks;
        if marks.is_empty() {
            ui.label(RichText::new("Pendant la vidéo, « + Repère » note le moment en cours : nomme-le pour y revenir d'un clic.").weak().size(11.5));
        }
        let (mut go, mut del, mut sort) = (None, None, false);
        for (k, m) in marks.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(ready, |ui| {
                    if icon_button(ui, Icon::Play, false).on_hover_text("Aller à ce moment").clicked() {
                        go = Some(m.t);
                    }
                });
                let time = ui
                    .add(
                        egui::DragValue::new(&mut m.t)
                            .range(0.0..=36000.0)
                            .speed(0.2)
                            .custom_formatter(|v, _| format_time(v as f32))
                            .custom_parser(|s| parse_time(s).map(f64::from)),
                    )
                    .on_hover_text("Glisser pour ajuster, double-clic pour saisir (1:23)");
                sort |= time.drag_stopped() || time.lost_focus();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Supprimer ce repère").clicked() {
                        del = Some(k);
                    }
                    let name = ui.add(egui::TextEdit::singleline(&mut m.name).hint_text("Nomme ce passage…").desired_width(ui.available_width()));
                    if self.focus_mark == Some(k) {
                        name.request_focus();
                        self.focus_mark = None;
                    }
                });
            });
        }
        if let Some(k) = del {
            marks.remove(k);
        }
        if sort {
            marks.sort_by(|a, b| a.t.total_cmp(&b.t));
        }
        if let (Some(t), Some(p)) = (go, &self.player) {
            p.seek(t);
            p.play();
        }
    }

    /// Folders, then the videos in no folder. Right click for the actions.
    fn list_panel(&mut self, ui: &mut egui::Ui) {
        let mut act = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mes tutos").size(16.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Dossier").on_hover_text("Ranger tes vidéos par thème").clicked() {
                    act = Some(Act::NewFolder(None));
                }
            });
        });
        ui.label(RichText::new("Double-clic = renommer · clic droit = plus d'actions").weak().size(11.0));
        ui.add_space(6.0);
        let list = self.list.clone();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for f in self.folders.clone() {
                if let Some(Renaming::Folder(old, buf)) = &mut self.renaming {
                    if *old == f {
                        match rename_field(ui, buf, &mut self.rename_focus) {
                            Some(true) => act = Some(Act::RenameFolder(f.clone())),
                            Some(false) => self.renaming = None,
                            None => {}
                        }
                        continue;
                    }
                }
                let n = list.iter().filter(|t| t.folder == f).count();
                let head = egui::CollapsingHeader::new(RichText::new(format!("{f}  ({n})")).strong())
                    .id_salt(("tuto_folder", &f))
                    .default_open(true)
                    .show(ui, |ui| {
                        if n == 0 {
                            ui.label(RichText::new("Vide : clic droit sur une vidéo → Ranger dans…").weak().size(11.0));
                        }
                        for t in list.iter().filter(|t| t.folder == f) {
                            self.row(ui, t, &mut act);
                        }
                    })
                    .header_response;
                if head.double_clicked() {
                    self.renaming = Some(Renaming::Folder(f.clone(), f.clone()));
                    self.rename_focus = true;
                }
                head.context_menu(|ui| {
                    if ui.button("Renommer le dossier").clicked() {
                        self.renaming = Some(Renaming::Folder(f.clone(), f.clone()));
                        self.rename_focus = true;
                        ui.close_menu();
                    }
                    if ui.button("Supprimer le dossier").on_hover_text("Les vidéos qu'il contient sont gardées, hors dossier").clicked() {
                        act = Some(Act::DeleteFolder(f.clone()));
                        ui.close_menu();
                    }
                });
            }
            for t in list.iter().filter(|t| t.folder.is_empty()) {
                self.row(ui, t, &mut act);
            }
        });
        match act {
            Some(Act::Pick(id)) => self.current = Some(id),
            Some(Act::Rename(id)) => {
                let title = self.list.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_default();
                self.renaming = Some(Renaming::Tuto(id, title));
                self.rename_focus = true;
            }
            Some(Act::Remove(id)) => self.remove(&id),
            Some(Act::Move(id, folder)) => self.move_to(&id, &folder),
            Some(Act::NewFolder(tuto)) => {
                let name = (1..).map(|k| if k == 1 { "Nouveau dossier".to_string() } else { format!("Nouveau dossier {k}") }).find(|n| !self.folders.contains(n)).unwrap();
                self.folders.push(name.clone());
                if let Some(id) = tuto {
                    self.move_to(&id, &name);
                }
                self.renaming = Some(Renaming::Folder(name.clone(), name));
                self.rename_focus = true;
            }
            Some(Act::RenameFolder(old)) => {
                if let Some(Renaming::Folder(_, new)) = self.renaming.take() {
                    let new = new.trim().replace(['\t', '\n'], " ");
                    if new != old && !self.folders.contains(&new) {
                        for f in self.folders.iter_mut().filter(|f| **f == old) {
                            *f = new.clone();
                        }
                        for t in self.list.iter_mut().filter(|t| t.folder == old) {
                            t.folder = new.clone();
                        }
                    }
                }
            }
            Some(Act::DeleteFolder(f)) => {
                self.folders.retain(|x| *x != f);
                for t in self.list.iter_mut().filter(|t| t.folder == f) {
                    t.folder.clear();
                }
            }
            None => {}
        }
    }

    /// One video of the list, or its rename field.
    fn row(&mut self, ui: &mut egui::Ui, t: &Tuto, act: &mut Option<Act>) {
        if let Some(Renaming::Tuto(id, buf)) = &mut self.renaming {
            if *id == t.id {
                match rename_field(ui, buf, &mut self.rename_focus) {
                    Some(true) => {
                        let title = buf.trim().to_string();
                        if let Some(x) = self.list.iter_mut().find(|x| x.id == t.id) {
                            x.title = title;
                        }
                        self.renaming = None;
                    }
                    Some(false) => self.renaming = None,
                    None => {}
                }
                return;
            }
        }
        let on = self.current.as_ref() == Some(&t.id);
        let r = nav_item(ui, Icon::Video, &t.title, on, None, 32.0).on_hover_text(&t.title);
        if r.double_clicked() {
            *act = Some(Act::Rename(t.id.clone()));
        } else if r.clicked() {
            *act = Some(Act::Pick(t.id.clone()));
        }
        r.context_menu(|ui| {
            if ui.button("Renommer").clicked() {
                *act = Some(Act::Rename(t.id.clone()));
                ui.close_menu();
            }
            ui.menu_button("Ranger dans…", |ui| {
                for f in &self.folders {
                    if ui.add_enabled(t.folder != *f, egui::Button::new(f)).clicked() {
                        *act = Some(Act::Move(t.id.clone(), f.clone()));
                        ui.close_menu();
                    }
                }
                if !t.folder.is_empty() && ui.button("Aucun dossier").clicked() {
                    *act = Some(Act::Move(t.id.clone(), String::new()));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Nouveau dossier…").clicked() {
                    *act = Some(Act::NewFolder(Some(t.id.clone())));
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button(RichText::new("Retirer de la liste").color(ui.visuals().error_fg_color)).clicked() {
                *act = Some(Act::Remove(t.id.clone()));
                ui.close_menu();
            }
        });
    }

    fn move_to(&mut self, id: &str, folder: &str) {
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            t.folder = folder.to_string();
        }
    }

    /// Where the player is this frame.
    pub fn web_rects(&self) -> impl Iterator<Item = egui::Rect> {
        self.rect.into_iter()
    }

    /// Imports a score for the current video, into `dir`.
    pub fn attach_pdf(&mut self, src: &Path, dir: &Path) {
        let Some(id) = self.current.clone() else { return };
        match copy_pdf(src, dir) {
            Ok(dest) => {
                if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
                    t.pdf = Some(dest);
                }
                self.score_tab = true;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Import de la partition impossible : {e}")),
        }
    }

    /// Chords and score of the current video, in two tabs.
    fn under_video(&mut self, ui: &mut egui::Ui, id: &str, pdf_dir: &Path, favorites: &[String]) {
        let Some(i) = self.list.iter().position(|t| t.id == id) else { return };
        // Suggestions of the chord field: where the typed chord starts, and the chords.
        let mut chips: Option<(usize, Vec<Chord>)> = None;
        ui.horizontal(|ui| {
            let n = self.list[i].chords.len();
            ui.selectable_value(&mut self.score_tab, false, if n > 0 { format!("Accords ({n})") } else { "Accords".into() });
            ui.selectable_value(&mut self.score_tab, true, "Partition");
            ui.separator();
            if self.score_tab {
                let has = self.list[i].pdf.is_some();
                if ui.button(if has { "Remplacer…" } else { "Importer un PDF…" }).on_hover_text("Tu peux aussi glisser un PDF dans la fenêtre").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                        self.attach_pdf(&p, pdf_dir);
                    }
                }
                if let Some(p) = self.list[i].pdf.clone() {
                    if ui.button("Ouvrir dans Aperçu").clicked() {
                        let _ = std::process::Command::new("open").arg(&p).spawn();
                    }
                    if ui.button("Retirer").on_hover_text("Détacher la partition de cette vidéo (le fichier reste dans le dossier Partitions)").clicked() {
                        self.list[i].pdf = None;
                    }
                }
                return;
            }
            // Suggestions for the chord being typed: arrows to choose, Tab to complete,
            // Enter or a click to add.
            let field_id = egui::Id::new("tuto_chord_field");
            let focused = ui.memory(|m| m.has_focus(field_id));
            let (start, sugg) = last_token(&self.chord_input, favorites);
            let n = sugg.len();
            if focused && n > 0 {
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                    self.chord_sel = (self.chord_sel + 1) % n;
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                    self.chord_sel = (self.chord_sel + n - 1) % n;
                }
            }
            self.chord_sel = self.chord_sel.min(n.saturating_sub(1));
            let tab = focused && n > 0 && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
            let field = ui.add(egui::TextEdit::singleline(&mut self.chord_input).id(field_id).hint_text("Ajouter : Am, Sol7, F#m…").desired_width(200.0));
            if field.changed() {
                self.chord_sel = 0;
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            // Shown under the field, in the page: a popup would go under the video.
            let pointer_on_list = self.suggest_rect.is_some_and(|r| ui.rect_contains_pointer(r));
            if (field.has_focus() || pointer_on_list) && n > 0 {
                chips = Some((start, sugg.clone()));
            }
            if let Some(k) = tab.then_some(self.chord_sel) {
                self.chord_input = format!("{}{}, ", &self.chord_input[..start], sugg[k].name());
                self.chord_input = self.chord_input.replacen(", , ", ", ", 1);
                self.chord_sel = 0;
                field.request_focus();
            }
            if let Some(k) = (enter && n > 0).then_some(self.chord_sel) {
                let head = self.chord_input[..start].to_string();
                self.chord_input = if head.trim().is_empty() { sugg[k].name() } else { format!("{head} {}", sugg[k].name()) };
            }
            if (enter || ui.button("Ajouter").clicked()) && !self.chord_input.trim().is_empty() {
                match parse_chords(&self.chord_input) {
                    Ok(cs) => {
                        for c in cs {
                            let name = c.name();
                            if !self.list[i].chords.contains(&name) {
                                self.list[i].chords.push(name);
                            }
                        }
                        self.chord_input.clear();
                        self.chord_sel = 0;
                        self.error = None;
                        field.request_focus();
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            if !favorites.is_empty() {
                ui.menu_button("Favoris ▾", |ui| {
                    for f in favorites {
                        let has = self.list[i].chords.contains(f);
                        if ui.add_enabled(!has, egui::Button::new(f)).clicked() {
                            self.list[i].chords.push(f.clone());
                        }
                    }
                })
                .response
                .on_hover_text("Tes accords favoris du dictionnaire");
            }
        });
        if let Some((start, sugg)) = chips {
            let mut click = None;
            let row = ui.horizontal_wrapped(|ui| {
                for (k, c) in sugg.iter().enumerate() {
                    let text = format!("{}  {}", c.name(), c.short_fr());
                    if ui.selectable_label(k == self.chord_sel, text).on_hover_text(c.name_fr()).clicked() {
                        click = Some(k);
                    }
                }
                ui.label(RichText::new("↑↓ choisir · Tab compléter · Entrée ajouter").weak().size(11.0));
            });
            self.suggest_rect = Some(row.response.rect);
            if let Some(k) = click {
                let name = sugg[k].name();
                if !self.list[i].chords.contains(&name) {
                    self.list[i].chords.push(name);
                }
                // The other chords typed stay in the field.
                self.chord_input.truncate(start);
                self.chord_input = self.chord_input.trim_end_matches([',', ';', ' ']).to_string();
                self.chord_sel = 0;
                ui.memory_mut(|m| m.request_focus(egui::Id::new("tuto_chord_field")));
            }
        } else {
            self.suggest_rect = None;
        }
        ui.add_space(6.0);

        if self.score_tab {
            if let Some(path) = self.list[i].pdf.clone() {
                if self.score.as_ref().is_none_or(|s| s.path != path) {
                    self.score = Some(score::Score::open(&path));
                }
                if let Some(s) = &mut self.score {
                    s.show(ui, ui.available_width());
                }
            } else {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Pas de partition pour cette vidéo").strong());
                    ui.label(RichText::new("Importe un PDF (ou glisse-le dans la fenêtre) pour l'avoir sous les yeux pendant le tuto.").weak());
                });
            }
            return;
        }
        if self.list[i].chords.is_empty() {
            ui.label(RichText::new("Ajoute les accords joués dans la vidéo pour avoir leurs diagrammes sous les yeux.").weak());
            return;
        }
        let (mut del, mut moved) = (None, None);
        ui.label(RichText::new("Glisse un accord pour changer l'ordre · × pour le retirer").weak().size(11.0));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 8.0);
            for (k, name) in self.list[i].chords.iter().enumerate() {
                let Some(c) = Chord::parse(name) else { continue };
                let card = ui
                    .dnd_drag_source(egui::Id::new(("tuto_chord", id, k)), k, |ui| {
                        diagram::card(ui, name, &c.name_fr(), voicing(&c).as_ref(), 1.0, false);
                    })
                    .response;
                // Dropped on this card: the dragged one takes its place.
                if let Some(from) = card.dnd_release_payload::<usize>() {
                    moved = Some((*from, k));
                }
                if card.dnd_hover_payload::<usize>().is_some_and(|from| *from != k) {
                    let x = card.rect.left() - 2.0;
                    ui.painter().line_segment([egui::pos2(x, card.rect.top()), egui::pos2(x, card.rect.bottom())], egui::Stroke::new(3.0_f32, ACCENT));
                }
                if ui.rect_contains_pointer(card.rect) && !card.dragged() {
                    let r = egui::Rect::from_center_size(card.rect.right_top() + egui::vec2(-11.0, 11.0), egui::vec2(18.0, 18.0));
                    let x = ui.interact(r, egui::Id::new(("tuto_chord_del", id, k)), egui::Sense::click()).on_hover_text("Retirer cet accord");
                    let v = ui.visuals();
                    let fill = if x.hovered() { v.error_fg_color } else { v.widgets.inactive.bg_fill };
                    ui.painter().circle_filled(r.center(), 9.0, fill);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(14.0), v.strong_text_color());
                    if x.clicked() {
                        del = Some(k);
                    }
                }
            }
        });
        ui.add_space(20.0);
        let chords = &mut self.list[i].chords;
        if let Some(k) = del {
            chords.remove(k);
        }
        if let Some((from, to)) = moved.filter(|(f, t)| f != t && *f < chords.len()) {
            let c = chords.remove(from);
            chords.insert(to.min(chords.len()), c);
        }
    }

    fn poll_titles(&mut self) {
        for (id, title) in self.titles.lock().unwrap().drain(..) {
            if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
                t.title = title;
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, pdf_dir: &Path, favorites: &[String]) {
        self.poll_titles();
        let v = ctx.style().visuals.clone();
        let mut remove = None;
        egui::SidePanel::right("tutos_list")
            .default_width(300.0)
            .min_width(220.0)
            .frame(egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin::symmetric(12, 14)).stroke(v.widgets.noninteractive.bg_stroke))
            .show(ctx, |ui| {
                if self.current.is_some() {
                    self.marks_panel(ui);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                self.list_panel(ui);
            });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin::symmetric(28, 20))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading(RichText::new("Tutos vidéos").strong());
                    let n = self.list.len();
                    ui.label(RichText::new(format!("{n} vidéo{}", if n > 1 { "s" } else { "" })).weak());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let add = egui::Button::new(RichText::new("Ajouter").color(ui.visuals().selection.stroke.color)).fill(ACCENT);
                    let clicked = ui.add(add).clicked();
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.url)
                            .hint_text("Colle un lien YouTube…")
                            .desired_width(320.0)
                            .margin(egui::vec2(10.0, 6.0)),
                    );
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (clicked || enter) && !self.url.trim().is_empty() {
                        let url = std::mem::take(&mut self.url);
                        if !self.add(&url) {
                            self.url = url;
                        }
                    }
                });
            });
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(14.0);

            let Some(id) = self.current.clone() else {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Aucun tuto pour l'instant").size(18.0).strong());
                    ui.label(RichText::new("Colle le lien d'une vidéo YouTube ci-dessus pour la regarder ici.").weak());
                });
                return;
            };

            // The page scrolls: the video goes out of sight, the score comes into view.
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                self.viewport = Some(ui.clip_rect());
                let title = self.list.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_default();
                ui.horizontal(|ui| {
                    let r = ui.add(egui::Label::new(RichText::new(&title).size(18.0).strong()).truncate().sense(egui::Sense::click()));
                    if r.on_hover_text("Double-clic pour renommer").double_clicked() || ui.small_button("✏").on_hover_text("Renommer").clicked() {
                        self.renaming = Some(Renaming::Tuto(id.clone(), title.clone()));
                        self.rename_focus = true;
                    }
                });
                ui.add_space(6.0);

                // 16:9, as wide as the page, but leaving a bit of what is under it in view.
                let vh = self.viewport.map_or(600.0, |v| v.height());
                let w = ui.available_width().min((vh - 120.0) * 16.0 / 9.0).max(160.0);
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, w * 9.0 / 16.0), egui::Sense::hover());
                // Seen while the player loads, under the web view.
                ui.painter().rect_filled(r, egui::CornerRadius::same(8), egui::Color32::BLACK);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Chargement…", egui::FontId::proportional(13.0), egui::Color32::GRAY);
                self.rect = Some(r);
                if let Some(e) = self.player.as_ref().and_then(|p| p.error()) {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let ready = self.player.as_ref().is_some_and(|p| p.ready());
                    ui.add_enabled_ui(ready, |ui| {
                        if ui.button("+ Repère").on_hover_text("Noter ce moment de la vidéo pour y revenir d'un clic").clicked() {
                            self.add_mark();
                        }
                        if ui.button("⟲ 5 s").on_hover_text("Revenir 5 secondes en arrière").clicked() {
                            if let Some(p) = &self.player {
                                p.seek((p.time() - 5.0).max(0.0));
                            }
                        }
                        let before = self.speed;
                        egui::ComboBox::from_id_salt("tuto_speed")
                            .selected_text(format!("Vitesse {:.0} %", self.speed * 100.0))
                            .width(115.0)
                            .show_ui(ui, |ui| {
                                for s in SPEEDS {
                                    ui.selectable_value(&mut self.speed, s, format!("{:.0} %", s * 100.0));
                                }
                            })
                            .response
                            .on_hover_text("Ralentir pour bien voir les doigts");
                        if self.speed != before {
                            if let Some(p) = &self.player {
                                p.set_rate(self.speed);
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Retirer").color(ui.visuals().error_fg_color)).on_hover_text("Retirer cette vidéo de la liste").clicked() {
                            remove = Some(id.clone());
                        }
                        let folder = self.list.iter().find(|t| t.id == id).map(|t| t.folder.clone()).unwrap_or_default();
                        if !self.folders.is_empty() {
                            let mut pick = folder.clone();
                            egui::ComboBox::from_id_salt("tuto_folder")
                                .selected_text(if folder.is_empty() { "Aucun dossier".to_string() } else { format!("Dossier : {folder}") })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut pick, String::new(), "Aucun dossier");
                                    for f in &self.folders {
                                        ui.selectable_value(&mut pick, f.clone(), f);
                                    }
                                });
                            if pick != folder {
                                self.move_to(&id, &pick);
                            }
                        }
                        if ui.button("Ouvrir sur YouTube").clicked() {
                            let _ = std::process::Command::new("open").arg(format!("https://www.youtube.com/watch?v={id}")).spawn();
                        }
                    });
                });

                ui.add_space(14.0);
                self.under_video(ui, &id, pdf_dir, favorites);
            });
        });
        if let Some(id) = remove {
            self.remove(&id);
        }
    }

    /// Shows, moves or hides the player over the rectangle reserved this frame.
    pub fn place(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let zoom = ctx.zoom_factor();
        let rect = self.rect.take();
        let Some(view) = self.viewport.take() else {
            if let Some(c) = &mut self.clip {
                c.hide();
            }
            if let Some(p) = &mut self.player {
                p.hide();
            }
            return;
        };
        if self.clip.is_none() {
            self.clip = clip::Clip::new(frame);
        }
        let Some(clip) = &mut self.clip else {
            self.error = Some("Affichage de la vidéo impossible.".into());
            return;
        };
        if clip.place(view, zoom) {
            // Placed relative to the container, which moved.
            if let Some(p) = &mut self.player {
                p.invalidate();
            }
        }
        let o = clip.origin().to_vec2();
        let clip = &*clip;
        let target = rect.map(|r| r.translate(-o)).zip(self.current.clone());
        match (target, &mut self.player) {
            (Some((r, id)), Some(p)) => {
                if p.id != id {
                    p.load(&id);
                    self.speed = 1.0;
                }
                p.place(r, zoom);
            }
            (Some((r, id)), None) => match video::Player::new(clip, &id, r, zoom) {
                Ok(p) => self.player = Some(p),
                Err(e) => {
                    self.error = Some(e);
                    self.current = None;
                }
            },
            (None, Some(p)) => p.hide(),
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_youtube_links() {
        let id = Some("dQw4w9WgXcQ".to_string());
        assert_eq!(parse_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"), id);
        assert_eq!(parse_id("https://www.youtube.com/watch?list=PL1&v=dQw4w9WgXcQ&t=42s"), id);
        assert_eq!(parse_id("https://youtu.be/dQw4w9WgXcQ?si=abc"), id);
        assert_eq!(parse_id("youtube.com/shorts/dQw4w9WgXcQ"), id);
        assert_eq!(parse_id("https://m.youtube.com/embed/dQw4w9WgXcQ"), id);
        assert_eq!(parse_id(" dQw4w9WgXcQ "), id);
        assert_eq!(parse_id("https://vimeo.com/123"), None);
        assert_eq!(parse_id("https://www.youtube.com/watch?v=short"), None);
    }

    #[test]
    fn times_and_storage() {
        assert_eq!(format_time(65.4), "1:05");
        assert_eq!(format_time(3723.0), "1:02:03");
        assert_eq!(parse_time("1:05"), Some(65.0));
        assert_eq!(parse_time("1:02:03"), Some(3723.0));
        assert_eq!(parse_time("42.5"), Some(42.5));
        assert_eq!(parse_time("1:xx"), None);
        let t = Tuto {
            id: "dQw4w9WgXcQ".into(),
            title: "Solo\tde fin".into(),
            marks: vec![Mark { t: 12.5, name: "Intro".into() }, Mark { t: 80.0, name: String::new() }],
            folder: String::new(),
            chords: Vec::new(),
            pdf: None,
        };
        let back = decode(&encode(&t)).unwrap();
        assert_eq!(back.title, "Solo de fin");
        assert_eq!(back.marks, t.marks);
    }
}

#[cfg(test)]
mod folder_tests {
    use super::*;

    #[test]
    fn folders_are_saved() {
        let mut a = Tutos::load(&|_| None);
        a.add("dQw4w9WgXcQ");
        a.add("abcdefghijk");
        a.folders = vec!["Solos".into(), "Vide".into()];
        a.move_to("abcdefghijk", "Solos");
        let saved: std::collections::HashMap<_, _> = a.save().into_iter().collect();
        let b = Tutos::load(&|k| saved.get(k).cloned());
        assert_eq!(b.folders, a.folders);
        assert_eq!(b.list.iter().map(|t| t.folder.as_str()).collect::<Vec<_>>(), ["Solos", ""]);
    }
}

#[cfg(test)]
mod chord_tests {
    use super::*;

    #[test]
    fn parses_chord_lists() {
        let names = |t: &str| parse_chords(t).map(|v| v.iter().map(|c| c.name()).collect::<Vec<_>>());
        assert_eq!(names("Am, G C"), Ok(vec!["Am".to_string(), "G".into(), "C".into()]));
        assert_eq!(names("la mineur, Sol7"), Ok(vec!["Am".to_string(), "G7".into()]));
        assert!(names("Am, xyz").is_err());
    }
}

#[cfg(test)]
mod suggest_tests {
    use super::*;

    #[test]
    fn suggests_chords() {
        let names = |t: &str| suggest(t, &[]).iter().map(|c| c.name()).collect::<Vec<_>>();
        assert_eq!(names("G")[..3], ["G", "Gm", "G7"]);
        assert_eq!(names("sol")[0], "G");
        assert_eq!(names("re m")[0], "Dm");
        assert_eq!(names("Am7")[0], "Am7");
        assert_eq!(suggest("G", &["G7".into()])[1].name(), "G7");
        assert!(names("xyz").is_empty());
        let (start, s) = last_token("Am, G C", &[]);
        assert_eq!((start, s[0].name()), (6, "C".to_string()));
    }
}

#[cfg(test)]
mod slash_tests {
    use super::*;

    #[test]
    fn slash_chords_in_both_languages() {
        assert_eq!(Chord::parse_loose("Do/Si").map(|c| c.name()), Some("C/B".to_string()));
        assert_eq!(Chord::parse_loose("Lam/Sol").map(|c| c.name()), Some("Am/G".to_string()));
        assert_eq!(suggest("C/", &[])[0].name(), "C/B");
        assert!(suggest("do/", &[]).iter().any(|c| c.name() == "C/B"));
        let cb = Chord::parse("C/B").unwrap();
        assert_eq!(voicing(&cb).map(|v| v.frets), Some([None, Some(2), Some(2), Some(0), Some(1), Some(0)]));
    }
}
