//! Reader side of the synchronized video: the panel, the player's placement, and
//! the video acting as the playback clock.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, RichText};

use crate::{Page, TillApp, playback, songsterr, video};

/// Result of fetching the video of an older import: the updated tab text.
pub type Fetch = Arc<Mutex<Option<Result<Option<String>, String>>>>;

const SONGSTERR_LINE: &str = "Importé depuis Songsterr : ";

impl TillApp {
    /// The video to show for this song, if any.
    pub(crate) fn sync(&self) -> Option<&video::Sync> {
        if !self.video_on {
            return None;
        }
        let main = self.song.video.as_ref();
        if self.video_backing { self.song.backing.as_ref().or(main) } else { main }
    }

    fn songsterr_url(&self) -> Option<String> {
        self.source.lines().find_map(|l| l.strip_prefix(SONGSTERR_LINE)).map(|u| u.trim().to_string())
    }

    /// The side panel has something to show about video.
    pub(crate) fn has_video_panel(&self) -> bool {
        self.song.video.is_some() || self.songsterr_url().is_some()
    }

    /// Video as the playback clock. Returns false when no video drives playback.
    pub(crate) fn drive_video(&mut self, timeline: &playback::Timeline) -> bool {
        let Some(sync) = self.sync().cloned() else { return false };
        let Some(p) = self.video.as_ref().filter(|p| p.id == sync.id && p.ready() && p.error().is_none()) else { return false };

        // Started or paused from YouTube's own controls.
        let settled = self.video_cmd_at.is_none_or(|t| t.elapsed().as_secs_f32() > 0.8);
        if settled && p.playing() != self.video_cmd {
            self.video_cmd = p.playing();
            self.playing = p.playing();
            self.waiting = None;
            self.counting = None;
        }
        p.set_rate(self.speed);
        let want = self.playing && self.waiting.is_none();
        let at = |beat: f32| sync.time_at(timeline, beat) + self.song.video_offset;
        if self.video_resync || (want && !self.video_cmd) {
            p.seek(at(self.beat));
            self.video_resync = false;
        }
        if want != self.video_cmd {
            if want {
                p.play();
            } else {
                p.pause();
            }
            self.video_cmd = want;
            self.video_cmd_at = Some(Instant::now());
        }
        if !want {
            return true;
        }

        let from = self.beat;
        let mut to = sync.beat_at(timeline, p.time() - self.song.video_offset);
        // Player reports arrive late: ignore tiny steps back, follow real seeks.
        if to < from && from - to < 0.25 {
            to = from;
        }
        if self.wait && self.reader_mic.is_some() {
            if let Some(o) = timeline.onsets.iter().find(|o| o.beat >= from && o.beat < to && !o.notes.is_empty()) {
                to = o.beat;
                self.waiting = Some(o.beat);
                self.video_resync = true;
            }
        }
        // The video brings the music: only the metronome may sound.
        let sound = std::mem::replace(&mut self.sound, false);
        self.sound_between(timeline, from, to);
        self.sound = sound;
        self.beat = to;
        if let (Some(a), Some(b)) = (self.loop_a, self.loop_b) {
            if b > a && self.beat >= b {
                self.beat = a;
                self.video_resync = true;
            }
        }
        if self.beat >= timeline.total {
            self.playing = false;
            self.beat = 0.0;
            self.video_resync = true;
        }
        true
    }

    /// Shows, moves or hides the player over the rectangle reserved in the panel.
    pub(crate) fn place_video(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let target = self.video_rect.filter(|_| self.page == Page::Reader && !self.editing).zip(self.sync().map(|s| s.id.clone()));
        let zoom = ctx.zoom_factor();
        match (target, &mut self.video) {
            (Some((r, id)), Some(p)) => {
                if p.id != id {
                    p.load(&id);
                    self.video_cmd = false;
                }
                p.place(r, zoom);
            }
            (Some((r, id)), None) => match video::Player::new(frame, &id, r, zoom) {
                Ok(p) => self.video = Some(p),
                Err(e) => {
                    self.error = Some(e);
                    self.video_on = false;
                }
            },
            (None, Some(p)) => {
                p.hide();
                self.video_cmd = false;
            }
            (None, None) => {}
        }
    }

    /// Fetches the video of a tab imported before videos were supported.
    fn fetch_video(&mut self) {
        let Some(url) = self.songsterr_url() else { return };
        let src = self.source.clone();
        let slot: Fetch = Arc::new(Mutex::new(None));
        let out = slot.clone();
        std::thread::spawn(move || {
            let r = songsterr::fetch_meta(&url).map(|meta| {
                let track = songsterr::parse_url(&url).and_then(|(_, t)| t).unwrap_or(meta.default_track);
                let (main, backing) = songsterr::fetch_videos(&meta, track);
                main.is_some().then(|| songsterr::with_videos(&src, main.as_ref(), backing.as_ref()))
            });
            *out.lock().unwrap() = Some(r);
        });
        self.video_fetch = Some(slot);
    }

    fn poll_fetch(&mut self) {
        let Some(r) = self.video_fetch.as_ref().and_then(|s| s.lock().unwrap().take()) else { return };
        self.video_fetch = None;
        match r {
            Ok(Some(text)) => {
                self.source = text;
                self.rebuild();
                if self.path.is_some() {
                    self.save_file();
                }
            }
            Ok(None) => self.error = Some("Songsterr n'a pas de vidéo synchronisée pour ce morceau.".into()),
            Err(e) => self.error = Some(e),
        }
    }

    /// Stores the sync correction in the tab file, as a `{video_offset: …}` line.
    fn set_video_offset(&mut self, offset: f32) {
        let line = format!("{{video_offset: {offset:.2}}}");
        let mut lines: Vec<String> = self.source.lines().filter(|l| !l.starts_with("{video_offset:")).map(String::from).collect();
        let at = lines.iter().rposition(|l| l.starts_with("{video")).map_or(0, |i| i + 1);
        lines.insert(at, line);
        self.source = lines.join("\n") + "\n";
        self.rebuild();
        self.video_resync = true;
        if self.path.is_some() {
            self.save_file();
        }
    }

    pub(crate) fn video_panel(&mut self, ui: &mut egui::Ui, timeline: &playback::Timeline) {
        self.poll_fetch();
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Vidéo").size(16.0).strong());
            if self.song.video.is_some() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.video_on { "Masquer" } else { "Afficher" };
                    if ui.small_button(label).clicked() {
                        self.video_on = !self.video_on;
                        self.video_resync = true;
                    }
                });
            }
        });
        ui.add_space(6.0);

        if self.song.video.is_none() {
            if self.video_fetch.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Recherche de la vidéo…").weak());
                });
            } else if ui.button("Charger la vidéo Songsterr").on_hover_text("La vidéo YouTube du morceau, synchronisée avec la tab").clicked() {
                self.fetch_video();
            }
            ui.add_space(10.0);
            return;
        }
        if !self.video_on {
            ui.label(RichText::new("Vidéo masquée : Till joue la tab avec son propre son.").weak());
            ui.add_space(10.0);
            return;
        }

        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(egui::vec2(w, w * 9.0 / 16.0), egui::Sense::hover());
        // Seen while the player loads, under the web view.
        ui.painter().rect_filled(r, egui::CornerRadius::same(8), egui::Color32::BLACK);
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Chargement…", egui::FontId::proportional(13.0), egui::Color32::GRAY);
        self.video_rect = Some(r);
        if let Some(e) = self.video.as_ref().and_then(|p| p.error()) {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if self.song.backing.is_some()
                && ui
                    .toggle_value(&mut self.video_backing, "Sans la guitare")
                    .on_hover_text("Version où la guitare est retirée : c'est toi qui joues sa partie")
                    .changed()
            {
                self.video_resync = true;
            }
            ui.label(RichText::new("Décalage").weak()).on_hover_text("Si la tab est en avance ou en retard sur la vidéo");
            let offset = self.song.video_offset;
            if ui.small_button("−").clicked() {
                self.set_video_offset(offset - 0.1);
            }
            ui.label(RichText::new(format!("{offset:+.1} s")).monospace());
            if ui.small_button("+").clicked() {
                self.set_video_offset(offset + 0.1);
            }
        });
        // Recalibration: the player says when the first note is heard in the video.
        let first = timeline.onsets.iter().find(|o| !o.notes.is_empty()).map(|o| o.beat);
        let playing = self.video.as_ref().is_some_and(|p| p.playing());
        if let (Some(first), Some(sync), true) = (first, self.sync().cloned(), playing) {
            let b = egui::Button::new(RichText::new("La guitare commence maintenant !").color(ui.visuals().selection.stroke.color)).fill(crate::theme::ACCENT);
            if ui.add(b).on_hover_text("Clique au moment où la première note de la tab est jouée dans la vidéo : Till recale toute la tab").clicked() {
                let now = self.video.as_ref().map_or(0.0, |p| p.time());
                self.set_video_offset(now - sync.time_at(timeline, first));
            }
        } else {
            ui.label(RichText::new("Décalée ? Lance la lecture : un bouton te laissera indiquer quand la guitare commence.").weak().size(11.0));
        }
        ui.add_space(2.0);
        ui.label(RichText::new("La tab suit la vidéo ; Lecture, la boucle et la vitesse la pilotent aussi.").weak().size(11.0));
        ui.add_space(4.0);
    }
}
