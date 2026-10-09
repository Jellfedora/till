mod chords;
mod diagram;
mod icon;
mod learn;
mod library;
mod mic;
mod playback;
mod reader_video;
mod shelf;
mod song;
mod songsterr;
mod synth;
mod theme;
mod tuner;
mod video;
mod view;
mod widgets;

use std::path::PathBuf;

use eframe::egui::{self, Key, Modifiers, RichText};

use widgets::{Icon, group, nav_item, section_label};

const SAMPLE: &str = include_str!("../examples/demo.txt");
const MAX_RECENT: usize = 8;
/// Count-in length before playback, in seconds.
const COUNT_IN: f32 = 3.0;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    /// List of tabs in the library folder.
    Shelf,
    /// Reading one tab.
    Reader,
    Chords,
    Tuner,
    Learn,
}

struct TillApp {
    page: Page,
    source: String,
    path: Option<PathBuf>,
    recent: Vec<PathBuf>,
    transpose: i32,
    font_size: f32,
    song: song::Song,
    editing: bool,
    playing: bool,
    beat: f32,
    bpm: f32,
    beats_per_bar: f32,
    beats_per_chord: f32,
    follow: bool,
    highlight: Option<usize>,
    show_chords: bool,
    error: Option<String>,
    library: library::Library,
    shelf: shelf::Shelf,
    songsterr: songsterr::Importer,
    tuner: tuner::Tuner,
    learn: learn::Learn,
    /// Playback speed factor (1 = written tempo).
    speed: f32,
    easy_chords: bool,
    capo: u8,
    loop_a: Option<f32>,
    loop_b: Option<f32>,
    /// "Wait for the note": playback pauses on each note until the mic hears it.
    wait: bool,
    waiting: Option<f32>,
    reader_mic: Option<mic::Mic>,
    attack: mic::Attack,
    heard_at: Option<std::time::Instant>,
    /// Count-in before playback starts, to pick up the guitar.
    count_in: bool,
    counting: Option<std::time::Instant>,
    count_ticks: u64,
    /// Synchronized YouTube video (Songsterr imports).
    video: Option<video::Player>,
    video_on: bool,
    /// Plays the version without the guitar.
    video_backing: bool,
    /// Where the player goes this frame.
    video_rect: Option<egui::Rect>,
    /// Whether the player was last told to play, and when.
    video_cmd: bool,
    video_cmd_at: Option<std::time::Instant>,
    /// The beat moved: the video must seek to it.
    video_resync: bool,
    video_fetch: Option<reader_video::Fetch>,
    last_frame: Option<std::time::Instant>,
    synth: Option<synth::Synth>,
    sound: bool,
    metronome: bool,
    volume: f32,
    logo: egui::TextureHandle,
}

impl TillApp {
    fn new(cc: &eframe::CreationContext) -> Self {
        let mut app = Self::with_context(&cc.egui_ctx, cc.storage);
        if let Some(arg) = std::env::args().nth(1) {
            app.load(PathBuf::from(arg));
        }
        app
    }

    fn with_context(ctx: &egui::Context, storage: Option<&dyn eframe::Storage>) -> Self {
        theme::apply(ctx);
        let get = |k: &str| storage.and_then(|s| s.get_string(k));
        let mut app = TillApp {
            page: Page::Shelf,
            source: get("source").unwrap_or_else(|| SAMPLE.to_string()),
            path: get("path").filter(|p| !p.is_empty()).map(PathBuf::from),
            recent: get("recent")
                .map(|r| clean_recent(r.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()))
                .unwrap_or_default(),
            transpose: get("transpose").and_then(|t| t.parse().ok()).unwrap_or(0),
            font_size: get("font_size").and_then(|t| t.parse().ok()).unwrap_or(15.0),
            song: Default::default(),
            editing: false,
            playing: false,
            beat: 0.0,
            bpm: get("bpm").and_then(|t| t.parse().ok()).unwrap_or(80.0),
            beats_per_bar: get("beats_per_bar").and_then(|t| t.parse().ok()).unwrap_or(4.0),
            beats_per_chord: get("beats_per_chord").and_then(|t| t.parse().ok()).unwrap_or(4.0),
            follow: true,
            highlight: None,
            show_chords: true,
            error: None,
            library: Default::default(),
            songsterr: Default::default(),
            tuner: Default::default(),
            learn: learn::Learn::load(&get),
            speed: get("speed").and_then(|t| t.parse().ok()).unwrap_or(1.0),
            easy_chords: get("easy_chords").is_some_and(|v| v == "true"),
            capo: 0,
            loop_a: None,
            loop_b: None,
            wait: false,
            waiting: None,
            reader_mic: None,
            attack: Default::default(),
            heard_at: None,
            count_in: get("count_in").is_some_and(|v| v == "true"),
            counting: None,
            count_ticks: 0,
            video: None,
            video_on: get("video_on").is_none_or(|v| v == "true"),
            video_backing: false,
            video_rect: None,
            video_cmd: false,
            video_cmd_at: None,
            video_resync: false,
            video_fetch: None,
            last_frame: None,
            shelf: shelf::Shelf::new(get("library_dir").map_or_else(shelf::default_dir, PathBuf::from)),
            synth: synth::Synth::new(),
            sound: get("sound").is_none_or(|v| v == "true"),
            metronome: get("metronome").is_some_and(|v| v == "true"),
            volume: get("volume").and_then(|t| t.parse().ok()).unwrap_or(0.8),
            logo: ctx.load_texture(
                "logo",
                egui::ColorImage::from_rgba_unmultiplied([96, 96], &icon::rgba(96)),
                egui::TextureOptions::LINEAR,
            ),
        };
        if let Some(s) = &app.synth {
            s.send(synth::Event::Volume(app.volume));
        }
        app.shelf.favorites = get("fav_tabs").map(|f| f.lines().filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default();
        app.tuner.tuning = get("tuning").and_then(|t| t.parse().ok()).filter(|&t| t < tuner::TUNINGS.len()).unwrap_or(0);
        app.tuner.a4 = get("a4").and_then(|t| t.parse().ok()).unwrap_or(440.0);
        app.library.favorites = get("fav_chords").map(|f| f.lines().filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default();
        // The file may have changed since last time (re-import, sync fix): it wins.
        if let Some(text) = app.path.as_ref().and_then(|p| std::fs::read(p).ok()) {
            app.source = String::from_utf8_lossy(&text).into_owned();
        }
        app.rebuild();
        app
    }

    fn rebuild(&mut self) {
        self.song = song::parse_with(&self.source, self.transpose - self.capo as i32, self.easy_chords);
        self.song.capo = self.capo;
    }

    /// Clears per-song reading aids.
    fn reset_aids(&mut self) {
        self.capo = 0;
        self.loop_a = None;
        self.loop_b = None;
        self.waiting = None;
        self.video_backing = false;
        self.video_resync = true;
    }

    fn load(&mut self, path: PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => {
                self.source = String::from_utf8_lossy(&bytes).into_owned();
                self.recent.insert(0, path.clone());
                self.recent = clean_recent(std::mem::take(&mut self.recent));
                self.path = Some(path);
                self.transpose = 0;
                self.reset_aids();
                self.beat = 0.0;
                self.playing = false;
                self.counting = None;
                self.editing = false;
                self.error = None;
                self.page = Page::Reader;
                self.rebuild();
                if let Some(t) = self.song.tempo {
                    self.bpm = t;
                }
                if let Some(b) = self.song.beats_per_bar {
                    self.beats_per_bar = b;
                }
            }
            Err(e) => self.error = Some(format!("Impossible d'ouvrir {} : {e}", path.display())),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Tablatures", &["txt", "tab", "crd", "chopro", "cho", "pro"])
            .add_filter("Tous", &["*"])
            .pick_file()
        {
            self.import(p);
        }
    }

    /// Copies a file into the library folder, then opens it.
    fn import(&mut self, p: PathBuf) {
        match self.shelf.import(&p) {
            Ok(dest) => self.load(dest),
            Err(e) => {
                self.error = Some(format!("Import impossible : {e}"));
                self.load(p);
            }
        }
    }

    fn save_file(&mut self) {
        let path = self.path.clone().or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Tablature", &["txt"])
                .set_directory(&self.shelf.dir)
                .set_file_name("Titre - Artiste.txt")
                .save_file()
        });
        if let Some(p) = path {
            match std::fs::write(&p, &self.source) {
                Ok(()) => {
                    self.path = Some(p);
                    self.shelf.scan();
                }
                Err(e) => self.error = Some(format!("Échec de l'enregistrement : {e}")),
            }
        }
    }

    fn new_tab(&mut self) {
        self.source.clear();
        self.path = None;
        self.transpose = 0;
        self.editing = true;
        self.page = Page::Reader;
        self.rebuild();
    }

    /// Opens a built-in exercise in the reader, without a file.
    fn open_text(&mut self, text: &str) {
        self.source = text.to_string();
        self.path = None;
        self.transpose = 0;
        self.reset_aids();
        self.beat = 0.0;
        self.playing = false;
        self.counting = None;
        self.editing = false;
        self.page = Page::Reader;
        self.rebuild();
        if let Some(t) = self.song.tempo {
            self.bpm = t;
        }
        self.beats_per_bar = self.song.beats_per_bar.unwrap_or(4.0);
    }

    fn set_transpose(&mut self, t: i32) {
        self.transpose = (t + 11).rem_euclid(23) - 11;
        self.rebuild();
    }

    fn zoom(&mut self, d: f32) {
        self.font_size = (self.font_size + d).clamp(8.0, 40.0);
    }

    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or("Sans titre".to_string(), |s| s.to_string_lossy().into_owned())
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = |k| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, k));
        if cmd(Key::O) {
            self.open_dialog();
        }
        if cmd(Key::S) {
            self.save_file();
        }
        if cmd(Key::N) {
            self.new_tab();
        }
        if cmd(Key::E) {
            self.page = Page::Reader;
            self.editing = !self.editing;
        }
        if cmd(Key::Num1) {
            self.page = Page::Reader;
        }
        if cmd(Key::Num2) {
            self.page = Page::Chords;
        }
        if cmd(Key::Num4) {
            self.stop();
            self.page = Page::Learn;
        }
        if cmd(Key::Num3) {
            self.stop();
            self.page = Page::Tuner;
        }
        if cmd(Key::Plus) || cmd(Key::Equals) {
            self.zoom(1.0);
        }
        if cmd(Key::Minus) {
            self.zoom(-1.0);
        }
        if self.page != Page::Reader || self.editing || ctx.wants_keyboard_input() {
            return;
        }
        let key = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if key(Key::Space) {
            self.toggle_play();
        }
        if key(Key::ArrowUp) {
            self.set_transpose(self.transpose + 1);
        }
        if key(Key::ArrowDown) {
            self.set_transpose(self.transpose - 1);
        }
        if key(Key::ArrowRight) {
            self.bpm = (self.bpm + 5.0).min(300.0);
        }
        if key(Key::Escape) {
            self.stop();
        }
        if key(Key::ArrowLeft) {
            self.bpm = (self.bpm - 5.0).max(20.0);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        widgets::logo(ui, &self.logo);
        ui.add_space(14.0);

        let n = self.shelf.entries.len();
        let on_tabs = matches!(self.page, Page::Shelf | Page::Reader);
        let badge = (n > 0).then(|| n.to_string());
        if nav_item(ui, Icon::Tab, "Tablatures", on_tabs, badge, 38.0).clicked() {
            self.stop();
            self.page = Page::Shelf;
        }
        if nav_item(ui, Icon::Chords, "Accords", self.page == Page::Chords, None, 38.0).clicked() {
            self.page = Page::Chords;
        }
        if nav_item(ui, Icon::Learn, "Apprendre", self.page == Page::Learn, None, 38.0).on_hover_text("Cmd+4").clicked() {
            self.stop();
            self.page = Page::Learn;
        }
        if nav_item(ui, Icon::Tuner, "Accordeur", self.page == Page::Tuner, None, 38.0).on_hover_text("Cmd+3").clicked() {
            self.stop();
            self.page = Page::Tuner;
        }

        let favs: Vec<(PathBuf, String)> = self.shelf.favorite_entries().map(|e| (e.path.clone(), e.title.clone())).collect();
        if !favs.is_empty() {
            section_label(ui, "Favoris");
            let mut open = None;
            for (p, title) in favs {
                let current = self.path.as_ref() == Some(&p) && self.page == Page::Reader;
                if nav_item(ui, Icon::Star, &title, current, None, 30.0).clicked() {
                    open = Some(p);
                }
            }
            if let Some(p) = open {
                self.load(p);
            }
        }

        section_label(ui, "Fichiers");
        if nav_item(ui, Icon::Plus, "Importer…", false, None, 30.0).on_hover_text("Cmd+O").clicked() {
            self.open_dialog();
        }
        if nav_item(ui, Icon::Plus, "Nouvelle tab", false, None, 30.0).on_hover_text("Cmd+N").clicked() {
            self.new_tab();
        }

        if !self.recent.is_empty() {
            section_label(ui, "Récents");
            let mut open = None;
            for p in &self.recent {
                let name = p.file_stem().map_or_else(|| p.display().to_string(), |s| s.to_string_lossy().into_owned());
                let current = self.path.as_ref() == Some(p) && self.page == Page::Reader;
                if nav_item(ui, Icon::File, &name, current, None, 30.0).on_hover_text(p.display().to_string()).clicked() {
                    open = Some(p.clone());
                }
            }
            if let Some(p) = open {
                self.load(p);
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(8.0);
            widgets::theme_switch(ui);
            ui.add_space(4.0);
            ui.label(RichText::new("Thème").size(10.5).weak().strong());
        });
    }

    fn toggle_play(&mut self) {
        if self.playing || self.counting.is_some() {
            self.playing = false;
            self.counting = None;
            self.damp();
        } else if self.count_in {
            self.counting = Some(std::time::Instant::now());
            self.count_ticks = 0;
        } else {
            self.playing = true;
        }
    }

    /// Count-in: one click per second, then playback starts.
    fn count_down(&mut self, ctx: &egui::Context) {
        let Some(t0) = self.counting else { return };
        let elapsed = t0.elapsed().as_secs_f32();
        if elapsed >= COUNT_IN {
            self.counting = None;
            self.playing = true;
            return;
        }
        let tick = elapsed as u64;
        if tick >= self.count_ticks {
            self.count_ticks = tick + 1;
            if let Some(s) = &self.synth {
                s.send(synth::Event::Click { accent: tick == 0 });
            }
        }
        let left = (COUNT_IN - elapsed).ceil() as u32;
        egui::Area::new(egui::Id::new("count_in"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill.gamma_multiply(0.92))
                    .corner_radius(egui::CornerRadius::same(24))
                    .inner_margin(egui::Margin::symmetric(48, 20))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(left.to_string()).size(96.0).strong().color(theme::ACCENT));
                            ui.label(RichText::new("Prends ta guitare…").size(16.0).weak());
                        });
                    });
            });
        ctx.request_repaint();
    }

    fn stop(&mut self) {
        self.counting = None;
        self.playing = false;
        self.beat = 0.0;
        self.video_resync = true;
        self.damp();
    }

    fn damp(&self) {
        if let Some(s) = &self.synth {
            s.send(synth::Event::Damp);
        }
    }

    /// Plays the notes and metronome ticks between two beat positions.
    fn sound_between(&self, timeline: &playback::Timeline, from: f32, to: f32) {
        let Some(synth) = &self.synth else { return };
        if self.sound {
            for o in timeline.onsets_between(from, to) {
                if let Some(st) = o.strum {
                    let gain = if st.down { 0.09 } else { 0.06 };
                    synth.send(synth::Event::Scratch { sweep_ms: st.sweep_ms, gain });
                }
                for n in &o.notes {
                    synth.send(synth::Event::Pluck {
                        string: n.string,
                        midi: n.midi,
                        gain: n.gain,
                        delay_ms: n.delay_ms,
                        bright: n.bright,
                    });
                }
            }
        }
        if self.metronome {
            let mut b = from.ceil();
            while b < to {
                let accent = (b % self.beats_per_bar.max(1.0)).abs() < 0.01;
                synth.send(synth::Event::Click { accent });
                b += 1.0;
            }
        }
    }

    /// A–B loop around the current measures.
    fn loop_controls(&mut self, ui: &mut egui::Ui, timeline: &playback::Timeline) {
        group(ui, |ui| {
            ui.label(RichText::new("Boucle").weak());
            let bounds = timeline.step_bounds(self.beat);
            if ui.selectable_label(self.loop_a.is_some(), "A").on_hover_text("Début de la boucle : mesure en cours").clicked() {
                self.loop_a = bounds.map(|b| b.0);
                if self.loop_b.is_some_and(|b| Some(b) <= self.loop_a) {
                    self.loop_b = None;
                }
            }
            if ui.selectable_label(self.loop_b.is_some(), "B").on_hover_text("Fin de la boucle : fin de la mesure en cours").clicked() {
                self.loop_b = bounds.map(|b| b.1);
                if self.loop_a.is_none() {
                    self.loop_a = Some(0.0);
                }
            }
            if (self.loop_a.is_some() || self.loop_b.is_some()) && ui.small_button("×").on_hover_text("Supprimer la boucle").clicked() {
                self.loop_a = None;
                self.loop_b = None;
            }
        });
    }

    /// Practice aids and less used settings, in a menu.
    fn options_menu(&mut self, ui: &mut egui::Ui) {
        let on = [self.count_in, self.wait, self.easy_chords].iter().filter(|b| **b).count();
        let label = if on > 0 { format!("Options · {on}") } else { "Options".to_string() };
        group(ui, |ui| {
            ui.menu_button(label, |ui| {
                ui.set_min_width(230.0);
                ui.label(RichText::new("AIDES").size(10.5).weak().strong());
                ui.checkbox(&mut self.count_in, "Décompte de 3 s avant de jouer").on_hover_text("Le temps de prendre ta guitare");
                if ui
                    .checkbox(&mut self.wait, "Attendre la note (micro)")
                    .on_hover_text("La lecture s'arrête sur chaque note jusqu'à ce que le micro l'entende. Bouton Passer (Tab) si la détection se trompe.")
                    .changed()
                    && !self.wait
                {
                    self.waiting = None;
                }
                if ui.checkbox(&mut self.easy_chords, "Accords faciles").on_hover_text("Remplace les accords difficiles (barrés, enrichis) par des versions simples").changed() {
                    self.rebuild();
                }
                ui.checkbox(&mut self.follow, "Suivre la lecture").on_hover_text("Faire défiler la page avec la lecture");
                ui.separator();
                ui.label(RichText::new("RYTHME").size(10.5).weak().strong());
                egui::Grid::new("rhythm_opts").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Temps par mesure");
                    ui.add(egui::DragValue::new(&mut self.beats_per_bar).range(1.0..=12.0).speed(0.05).fixed_decimals(0).suffix("/4"));
                    ui.end_row();
                    ui.label("Durée d'un accord").on_hover_text("Dans les parties chant (grilles d'accords)");
                    ui.add(egui::DragValue::new(&mut self.beats_per_chord).range(1.0..=16.0).speed(0.05).fixed_decimals(0).suffix(" temps"));
                    ui.end_row();
                });
            })
            .response
            .on_hover_text("Décompte, attendre la note, accords faciles, mesure…");
        });
    }

    /// Wait-for-the-note: keeps the microphone open and listens for the awaited notes.
    fn listen_for_note(&mut self, timeline: &playback::Timeline) {
        if !(self.wait && self.page == Page::Reader) {
            self.reader_mic = None;
            return;
        }
        if self.reader_mic.is_none() {
            match mic::Mic::open() {
                Ok(m) => self.reader_mic = Some(m),
                Err(e) => {
                    self.error = Some(format!("Attendre la note : {e}"));
                    self.wait = false;
                    return;
                }
            }
        }
        let (Some(at), Some(m)) = (self.waiting, &self.reader_mic) else { return };
        let level = mic::rms(&m.latest((m.sr * 0.03) as usize));
        if self.attack.update(level) {
            self.heard_at = Some(std::time::Instant::now());
        }
        if self.heard_at.is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(150)) {
            self.heard_at = None;
            let midis: Vec<u8> = timeline
                .onsets
                .iter()
                .filter(|o| (o.beat - at).abs() < 1e-4)
                .flat_map(|o| o.notes.iter().map(|n| n.midi))
                .collect();
            let n = if midis.len() > 1 { 8192 } else { 4096 };
            if mic::matches(&m.latest(n), m.sr, &midis) {
                self.release_wait(at);
            }
        }
    }

    fn release_wait(&mut self, at: f32) {
        self.waiting = None;
        self.beat = at + 1e-3;
        self.video_resync = true;
    }

    /// "Your turn" banner shown while waiting for a note.
    fn wait_banner(&mut self, ui: &mut egui::Ui, timeline: &playback::Timeline) {
        let Some(at) = self.waiting else { return };
        let notes: Vec<&playback::Note> = timeline.onsets.iter().filter(|o| (o.beat - at).abs() < 1e-4).flat_map(|o| &o.notes).collect();
        const NAMES: [&str; 6] = ["Mi grave", "La", "Ré", "Sol", "Si", "Mi aigu"];
        const OPEN: [u8; 6] = [40, 45, 50, 55, 59, 64];
        let what = match notes.as_slice() {
            [n] => {
                let s = (n.string as usize).min(5);
                let fret = n.midi.saturating_sub(OPEN[s]);
                format!("corde {} ({}), {}", 6 - s, NAMES[s], if fret == 0 { "à vide".to_string() } else { format!("case {fret}") })
            }
            many => format!("ces {} notes ensemble", many.len()),
        };
        let mut skip = false;
        egui::Frame::new()
            .fill(theme::ACCENT.gamma_multiply(0.16))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("À toi !").strong().color(theme::ACCENT).size(16.0));
                    ui.label(RichText::new(format!("Joue {what}")).size(15.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        skip |= ui.button("Passer (Tab)").clicked();
                        if ui.button("Désactiver").on_hover_text("Ne plus attendre les notes").clicked() {
                            self.wait = false;
                            skip = true;
                        }
                    });
                });
            });
        skip |= ui.input(|i| i.key_pressed(Key::Tab));
        if skip {
            self.release_wait(at);
        }
    }

    fn sound_controls(&mut self, ui: &mut egui::Ui) {
        group(ui, |ui| {
            let available = self.synth.is_some();
            ui.add_enabled_ui(available, |ui| {
                if ui
                    .toggle_value(&mut self.sound, "Son")
                    .on_hover_text("Jouer les notes avec la synthèse guitare")
                    .changed()
                    && !self.sound
                {
                    self.damp();
                }
                ui.toggle_value(&mut self.metronome, "Métronome");
                let r = ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.5).show_value(false));
                if r.changed() {
                    if let Some(s) = &self.synth {
                        s.send(synth::Event::Volume(self.volume));
                    }
                }
                r.on_hover_text("Volume");
            })
            .response
            .on_disabled_hover_text("Aucune sortie audio disponible");
        });
    }

    /// Play / stop, beat indicator, tempo, time signature.
    fn transport(&mut self, ui: &mut egui::Ui) {
        group(ui, |ui| {
            let active = self.playing || self.counting.is_some();
            let icon = if active { Icon::Pause } else { Icon::Play };
            if widgets::icon_button(ui, icon, active).on_hover_text("Lecture / pause (Espace)").clicked() {
                self.toggle_play();
            }
            if widgets::icon_button(ui, Icon::Stop, false).on_hover_text("Revenir au début (Échap)").clicked() {
                self.stop();
            }

            // Beat indicator: one dot per beat of the bar, the current one lit.
            let beat_in_bar = (self.beat % self.beats_per_bar.max(1.0)) as usize;
            let n = self.beats_per_bar.round().max(1.0) as usize;
            let (r, _) = ui.allocate_exact_size(egui::vec2(n as f32 * 10.0 + 6.0, 16.0), egui::Sense::hover());
            for i in 0..n {
                let c = egui::pos2(r.left() + 8.0 + i as f32 * 10.0, r.center().y);
                let on = (self.playing || self.beat > 0.0) && i == beat_in_bar;
                let color = if on { theme::ACCENT } else { ui.visuals().widgets.inactive.bg_fill.gamma_multiply(1.6) };
                ui.painter().circle_filled(c, if on && i == 0 { 4.5 } else { 3.5 }, color);
            }
            ui.separator();
            ui.label(RichText::new("Tempo").weak());
            ui.add(egui::DragValue::new(&mut self.bpm).range(20.0..=300.0).speed(0.5).fixed_decimals(0).suffix(" BPM"))
                .on_hover_text("Glisser ou double-cliquer pour saisir (flèches gauche / droite : ±5)");
            ui.separator();
            egui::ComboBox::from_id_salt("speed")
                .selected_text(format!("Vitesse {:.0} %", self.speed * 100.0))
                .width(105.0)
                .show_ui(ui, |ui| {
                    for p in [25, 50, 60, 75, 90, 100] {
                        ui.selectable_value(&mut self.speed, p as f32 / 100.0, format!("{p} %"));
                    }
                })
                .response
                .on_hover_text("Joue plus lentement sans changer le tempo écrit");
        });
    }

    fn tab_header(&mut self, ui: &mut egui::Ui) {
        let back = ui
            .allocate_ui(egui::vec2(150.0, 28.0), |ui| nav_item(ui, Icon::Back, "Tablatures", false, None, 28.0))
            .inner;
        if back.on_hover_text("Retour à la liste").clicked() {
            self.stop();
            self.page = Page::Shelf;
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(self.title()).size(22.0).strong());
                    if let Some(p) = self.path.clone().filter(|p| p.parent() == Some(self.shelf.dir.as_path())) {
                        if widgets::star_button(ui, self.shelf.is_favorite(&p)).clicked() {
                            self.shelf.toggle_favorite(&p);
                        }
                    }
                });
                let mut sub = format!("{} accord{}", self.song.chords.len(), if self.song.chords.len() > 1 { "s" } else { "" });
                if self.transpose != 0 {
                    sub += &format!(" · transposé {:+}", self.transpose);
                }
                if self.easy_chords {
                    sub += " · accords faciles";
                }
                ui.label(RichText::new(sub).weak());
                self.capo_hint(ui);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| self.view_controls(ui));
        });
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    }

    /// Capo: shows the current one, or suggests one that makes a chord sheet easier.
    fn capo_hint(&mut self, ui: &mut egui::Ui) {
        if self.capo > 0 {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Capo case {}", self.capo)).strong().color(theme::ACCENT));
                ui.label(RichText::new("· les accords affichés sont les formes à jouer").weak());
                if ui.small_button("Retirer le capo").clicked() {
                    self.capo = 0;
                    self.rebuild();
                }
            });
            return;
        }
        // Tabs are written fret by fret: a capo would not change them.
        if self.song.blocks.iter().any(|b| matches!(b, song::Block::Tab(_))) || self.song.chords.len() < 2 {
            return;
        }
        let chords: Vec<chords::Chord> = self.song.chords.iter().map(|c| c.chord.clone()).collect();
        let now: f32 = chords.iter().map(|c| c.difficulty()).sum();
        let (capo, best) = chords::best_capo(&chords);
        if capo == 0 || best > now * 0.75 {
            return;
        }
        let shapes: Vec<String> = chords.iter().map(|c| c.transposed(-(capo as i32)).name()).collect();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Astuce : capo case {capo} → tu joues {}", shapes.join(" "))).color(theme::ACCENT));
            if ui.small_button("Mettre le capo").on_hover_text("Même son, accords plus faciles").clicked() {
                self.capo = capo;
                self.rebuild();
            }
        });
    }

    /// Bottom options bar: progress, transport, sound, view options.
    fn bottom_bar(&mut self, ui: &mut egui::Ui, timeline: &playback::Timeline) {
        // Clickable progress bar.
        let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::click_and_drag());
        let v = ui.visuals();
        let frac = if timeline.total > 0.0 { (self.beat / timeline.total).clamp(0.0, 1.0) } else { 0.0 };
        ui.painter().rect_filled(r, egui::CornerRadius::same(3), v.widgets.inactive.bg_fill);
        let done = egui::Rect::from_min_size(r.min, egui::vec2(r.width() * frac, r.height()));
        ui.painter().rect_filled(done, egui::CornerRadius::same(3), theme::ACCENT);
        if let (Some(a), Some(b), true) = (self.loop_a, self.loop_b, timeline.total > 0.0) {
            let x = |t: f32| r.left() + r.width() * (t / timeline.total).clamp(0.0, 1.0);
            let zone = egui::Rect::from_x_y_ranges(x(a)..=x(b), r.top() - 3.0..=r.bottom() + 3.0);
            ui.painter().rect_stroke(zone, egui::CornerRadius::same(3), egui::Stroke::new(2.0_f32, v.strong_text_color()), egui::StrokeKind::Outside);
        }
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - r.left()) / r.width()).clamp(0.0, 1.0);
            self.damp();
            self.beat = t * timeline.total;
            self.video_resync = true;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            self.transport(ui);
            self.loop_controls(ui, timeline);
            self.options_menu(ui);
            self.sound_controls(ui);
        });
    }

    /// Header buttons, laid out right to left: chord panel, editor, text size, transposition.
    fn view_controls(&mut self, ui: &mut egui::Ui) {
        group(ui, |ui| {
            if ui.selectable_label(self.show_chords, "Diagrammes").on_hover_text("Afficher le panneau des accords").clicked() {
                self.show_chords = !self.show_chords;
            }
            if ui.selectable_label(self.editing, "✏ Éditer").on_hover_text("Coller / modifier le texte (Cmd+E)").clicked() {
                self.editing = !self.editing;
            }
        });
        group(ui, |ui| {
            if ui.small_button("A+").on_hover_text("Texte plus grand (Cmd +)").clicked() {
                self.zoom(1.0);
            }
            if ui.small_button("A−").on_hover_text("Texte plus petit (Cmd -)").clicked() {
                self.zoom(-1.0);
            }
        });
        group(ui, |ui| {
            if ui.small_button("+").on_hover_text("Flèche haut").clicked() {
                self.set_transpose(self.transpose + 1);
            }
            ui.label(RichText::new(format!("{:+}", self.transpose)).monospace().strong());
            if ui.small_button("−").on_hover_text("Flèche bas").clicked() {
                self.set_transpose(self.transpose - 1);
            }
            ui.label(RichText::new("Transpo").weak());
        });
    }

    fn chords_panel(&mut self, ui: &mut egui::Ui, hovered: &mut Option<usize>) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Accords du morceau").size(16.0).strong());
        });
        ui.label(RichText::new("Clic = ouvrir dans le dictionnaire").weak().size(11.0));
        ui.add_space(8.0);
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 8.0);
                for (i, info) in self.song.chords.iter().enumerate() {
                    let r = diagram::card_info(ui, info, self.highlight == Some(i));
                    if r.hovered() {
                        *hovered = Some(i);
                    }
                    if r.clicked() {
                        open = Some(info.chord.clone());
                    }
                }
            });
        });
        if let Some(c) = open {
            self.library.select(&c);
            self.page = Page::Chords;
        }
    }

    fn tab_page(&mut self, ctx: &egui::Context, hovered: &mut Option<usize>) {
        let v = ctx.style().visuals.clone();
        let timeline = playback::Timeline::build(&self.song, self.beats_per_bar, self.beats_per_chord);
        egui::TopBottomPanel::bottom("options")
            .frame(egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 10 }))
            .show_separator_line(true)
            .show(ctx, |ui| self.bottom_bar(ui, &timeline));
        let chords = self.show_chords && !self.song.chords.is_empty();
        if chords || self.has_video_panel() {
            egui::SidePanel::right("chords")
                .default_width(4.0 * (diagram::CARD_W + 4.0) + 24.0)
                .min_width(diagram::CARD_W + 24.0)
                .frame(egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin::symmetric(12, 0)).stroke(v.widgets.noninteractive.bg_stroke))
                .show(ctx, |ui| {
                    if self.has_video_panel() {
                        self.video_panel(ui, &timeline);
                    }
                    if chords {
                        self.chords_panel(ui, hovered);
                    }
                });
        }

        egui::TopBottomPanel::top("tab_header")
            .frame(egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin::symmetric(24, 14)))
            .show_separator_line(true)
            .show(ctx, |ui| self.tab_header(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(v.panel_fill)).show(ctx, |ui| {
            if self.editing {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 12)).show(ui, |ui| {
                    ui.label(RichText::new("Colle ta tab ici (texte brut, Ultimate Guitar ou ChordPro). Cmd+E pour revenir à la lecture.").weak());
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        let edit = egui::TextEdit::multiline(&mut self.source)
                            .font(egui::FontId::monospace(self.font_size))
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(40)
                            .margin(egui::vec2(12.0, 10.0));
                        if ui.add(edit).changed() {
                            self.rebuild();
                        }
                    });
                });
                return;
            }
            if self.song.blocks.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Glisse un fichier ici, Cmd+O pour ouvrir, ou Cmd+E pour coller une tab").weak().size(16.0));
                });
                return;
            }

            self.listen_for_note(&timeline);
            self.count_down(ctx);
            if self.drive_video(&timeline) {
                if self.playing {
                    ctx.request_repaint();
                }
            } else if self.playing {
                let dt = ctx.input(|i| i.stable_dt).min(0.1);
                let from = self.beat;
                let mut to = from + dt * self.bpm * self.speed / 60.0;
                if self.wait && self.reader_mic.is_some() {
                    if self.waiting.is_some() {
                        to = from;
                    } else if let Some(o) = timeline.onsets.iter().find(|o| o.beat >= from && !o.notes.is_empty()) {
                        // Stop right before the note: the player plays it, not Till.
                        if o.beat < to {
                            to = o.beat;
                            self.waiting = Some(o.beat);
                        }
                    }
                }
                self.sound_between(&timeline, from, to);
                self.beat = to;
                if let (Some(a), Some(b)) = (self.loop_a, self.loop_b) {
                    if b > a && self.beat >= b {
                        self.damp();
                        self.waiting = None;
                        self.beat = a;
                    }
                }
                if self.beat >= timeline.total {
                    self.playing = false;
                    self.beat = 0.0;
                    self.waiting = None;
                }
                ctx.request_repaint();
            }
            if self.waiting.is_some() {
                egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 10, bottom: 0 }).show(ui, |ui| {
                    self.wait_banner(ui, &timeline);
                });
            }
            let head = (self.playing || self.beat > 0.0).then(|| timeline.playhead(&self.song, self.beat)).flatten();
            if let Some(h) = head.as_ref().and_then(|h| h.chord) {
                hovered.get_or_insert(h);
            }
            let mut seek = None;
            // Content width inside the 32px side margins, used to reflow tab measures.
            let width = ui.available_width() - 64.0;
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(32, 20)).show(ui, |ui| {
                    let follow = self.playing && self.follow;
                    seek = view::show(ui, &self.song, self.font_size, width, self.highlight, hovered, head.as_ref(), follow);
                    ui.add_space(ui.available_height().max(200.0));
                });
            });
            if let Some(b) = seek.and_then(|s| timeline.seek(&s)) {
                self.damp();
                self.beat = b;
                self.video_resync = true;
                // Let the clicked note sound right away.
                self.sound_between(&timeline, b - 0.01, b + 0.15);
            }
        });
    }
}

impl eframe::App for TillApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.video_rect = None;
        self.ui(ctx);
        self.place_video(ctx, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.persist(storage);
    }
}

impl TillApp {
    fn ui(&mut self, ctx: &egui::Context) {
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            if let Some(p) = f.path {
                self.import(p);
            }
        }
        self.shortcuts(ctx);

        let title = match self.page {
            Page::Shelf => "Tablatures — Till".to_string(),
            Page::Reader => format!("{} — Till", self.title()),
            Page::Chords => "Accords — Till".to_string(),
            Page::Tuner => "Accordeur — Till".to_string(),
            Page::Learn => "Apprendre — Till".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        let v = ctx.style().visuals.clone();
        egui::SidePanel::left("nav")
            .exact_width(220.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::sidebar_fill(&v))
                    .inner_margin(egui::Margin::symmetric(12, 14))
                    .stroke(v.widgets.noninteractive.bg_stroke),
            )
            .show(ctx, |ui| self.sidebar(ui));

        let mut hovered = None;
        match self.page {
            Page::Reader => self.tab_page(ctx, &mut hovered),
            Page::Shelf => {
                let current = self.path.clone();
                let action = egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(v.panel_fill))
                    .show(ctx, |ui| self.shelf.show(ui, current.as_deref()))
                    .inner;
                match action {
                    Some(shelf::Action::Open(p)) => self.load(p),
                    Some(shelf::Action::New) => self.new_tab(),
                    Some(shelf::Action::Import) => self.open_dialog(),
                    Some(shelf::Action::Songsterr) => self.songsterr.show_with(String::new()),
                Some(shelf::Action::Deleted(p)) => {
                    self.recent.retain(|r| *r != p);
                    if self.path.as_ref() == Some(&p) {
                        self.path = None;
                    }
                }
                    None => {}
                }
            }
            Page::Chords => {
                let act = egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(v.panel_fill))
                    .show(ctx, |ui| self.library.show(ui))
                    .inner;
                match act {
                    Some(library::Action::Play(v, how)) => {
                        if let Some(s) = &self.synth {
                            s.send(synth::Event::Damp);
                            s.chord(&v.frets, how, 1.0);
                        }
                    }
                    Some(library::Action::Check(name)) => {
                        self.learn.open_check(&name);
                        self.page = Page::Learn;
                    }
                    None => {}
                }
            }
            Page::Tuner => {
                let play = egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(v.panel_fill))
                    .show(ctx, |ui| self.tuner.show(ui))
                    .inner;
                if let (Some((string, midi)), Some(s)) = (play, &self.synth) {
                    s.send(synth::Event::Damp);
                    s.send(synth::Event::Pluck { string, midi, gain: 0.9, delay_ms: 0.0, bright: 0.3 });
                }
            }
            Page::Learn => {
                let favorites = self.library.favorites.clone();
                let req = egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(v.panel_fill))
                    .show(ctx, |ui| self.learn.show(ui, self.synth.as_ref(), &favorites))
                    .inner;
                match req {
                    Some(learn::Request::Tuner) => self.page = Page::Tuner,
                    Some(learn::Request::Shelf) => self.page = Page::Shelf,
                    Some(learn::Request::OpenSong { text }) => self.open_text(text),
                    None => {}
                }
            }
        }
        // The microphone stays open only while a page that listens is on screen.
        if self.page != Page::Tuner {
            self.tuner.stop();
        }
        if self.page != Page::Learn {
            self.learn.leave();
        }
        if self.page != Page::Reader {
            self.reader_mic = None;
            self.waiting = None;
        }
        // Practice journal: time spent playing along or doing exercises.
        let now = std::time::Instant::now();
        if let Some(t) = self.last_frame {
            let dt = now.duration_since(t).as_secs_f32();
            if dt < 1.0 && (self.playing || self.learn.practicing()) {
                self.learn.journal.add(dt);
            }
        }
        self.last_frame = Some(now);
        self.highlight = hovered;

        // Pasting a Songsterr link anywhere (outside a text field) opens the importer.
        let pasted = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(t) if songsterr::parse_url(t).is_some() && t.contains("songsterr.com") => Some(t.clone()),
                _ => None,
            })
        });
        if let Some(url) = pasted.filter(|_| !ctx.wants_keyboard_input()) {
            self.songsterr.show_with(url);
        }
        if let Some((name, text)) = self.songsterr.show(ctx) {
            match self.shelf.add(&name, &text) {
                Ok(path) => self.load(path),
                Err(e) => self.error = Some(format!("Enregistrement impossible : {e}")),
            }
        }
    }

    fn persist(&mut self, storage: &mut dyn eframe::Storage) {
        let recent: Vec<String> = self.recent.iter().map(|p| p.display().to_string()).collect();
        storage.set_string("recent", recent.join("\n"));
        storage.set_string("source", self.source.clone());
        storage.set_string("path", self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
        storage.set_string("transpose", self.transpose.to_string());
        storage.set_string("font_size", self.font_size.to_string());
        storage.set_string("bpm", self.bpm.to_string());
        storage.set_string("library_dir", self.shelf.dir.display().to_string());
        storage.set_string("sound", self.sound.to_string());
        storage.set_string("metronome", self.metronome.to_string());
        storage.set_string("volume", self.volume.to_string());
        storage.set_string("beats_per_bar", self.beats_per_bar.to_string());
        storage.set_string("beats_per_chord", self.beats_per_chord.to_string());
        storage.set_string("fav_tabs", self.shelf.favorites.iter().cloned().collect::<Vec<_>>().join("\n"));
        storage.set_string("fav_chords", self.library.favorites.join("\n"));
        storage.set_string("tuning", self.tuner.tuning.to_string());
        storage.set_string("speed", self.speed.to_string());
        storage.set_string("easy_chords", self.easy_chords.to_string());
        storage.set_string("count_in", self.count_in.to_string());
        storage.set_string("video_on", self.video_on.to_string());
        for (k, v) in self.learn.save() {
            storage.set_string(k, v);
        }
        storage.set_string("a4", self.tuner.a4.to_string());
    }
}

/// Recent files: existing ones only, one entry per file name (most recent wins).
fn clean_recent(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .filter(|p| p.exists() && seen.insert(p.file_name().map(|n| n.to_os_string())))
        .take(MAX_RECENT)
        .collect()
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([760.0, 480.0])
            .with_title("Till")
            .with_icon(icon::icon_data(256))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Till", options, Box::new(|cc| Ok(Box::new(TillApp::new(cc)))))
}

#[cfg(test)]
mod screenshots {
    use super::*;
    use egui_kittest::Harness;

    /// Renders the app offscreen: `TILL_SHOT_DIR=/some/dir cargo test screenshots -- --nocapture`.
    #[test]
    fn render_pages() {
        let Ok(dir) = std::env::var("TILL_SHOT_DIR") else { return };
        let tab = std::env::var("TILL_SHOT_TAB").unwrap_or_else(|_| "tabs/Redemption Song - Bob Marley.txt".into());
        let dark = std::env::var("TILL_SHOT_LIGHT").is_err();
        let page = std::env::var("TILL_SHOT_PAGE").unwrap_or_default();
        let beat: f32 = std::env::var("TILL_SHOT_BEAT").ok().and_then(|b| b.parse().ok()).unwrap_or(0.0);
        let mut harness = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_state(
            |ctx, app: &mut TillApp| app.ui(ctx),
            {
                let ctx = egui::Context::default();
                let mut app = TillApp::with_context(&ctx, None);
                app.synth = None;
                app.load(PathBuf::from(&tab));
                app.beat = beat;
                app.page = match page.as_str() {
                    "shelf" => Page::Shelf,
                    "chords" => Page::Chords,
                    _ => Page::Reader,
                };
                app
            },
        );
        harness.ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
        theme::apply(&harness.ctx);
        let logo = harness.ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([96, 96], &icon::rgba(96)),
            egui::TextureOptions::LINEAR,
        );
        harness.state_mut().logo = logo;
        harness.run_steps(6);
        let img = harness.render().expect("render");
        let name = if page.is_empty() { "reader".to_string() } else { page };
        img.save(format!("{dir}/{name}.png")).unwrap();
    }
}
