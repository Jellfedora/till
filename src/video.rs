//! YouTube video synchronized with the tab, from Songsterr's per-measure timestamps.
//!
//! The YouTube player runs in a native web view placed over a rectangle of the
//! window. Its page is served from a tiny local HTTP server: YouTube refuses to
//! play embeds that have no web origin.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use wry::dpi::{LogicalPosition, LogicalSize};

use crate::playback::{StepKind, Timeline};

/// A video and the time (s) at which each measure starts, in playing order.
#[derive(Clone, Debug, PartialEq)]
pub struct Sync {
    pub id: String,
    pub points: Vec<f32>,
}

impl Sync {
    /// `ID | t0 t1 t2 …` as written in a `{video: …}` directive.
    pub fn parse(text: &str) -> Option<Sync> {
        let (id, points) = text.split_once('|')?;
        let id = id.trim();
        let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let points: Vec<f32> = points.split_whitespace().filter_map(|p| p.parse().ok()).collect();
        (ok && points.len() >= 2).then(|| Sync { id: id.to_string(), points })
    }

    pub fn to_directive(&self, key: &str) -> String {
        let pts: Vec<String> = self.points.iter().map(|p| format!("{p}")).collect();
        format!("{{{key}: {} | {}}}", self.id, pts.join(" "))
    }

    /// Start time of measure `k` when the tab has `measures` of them.
    ///
    /// Songsterr's list sometimes has a few more or fewer entries than the tab has
    /// measures (intro count, pickup bars): the two are aligned on their ends, and
    /// times outside the list are extrapolated from the nearest measure length.
    fn start(&self, k: isize, measures: usize) -> f32 {
        let p = &self.points;
        let n = p.len() as isize;
        let i = k + n - measures as isize;
        if i < 0 {
            p[0] + i as f32 * (p[1] - p[0])
        } else if i >= n {
            p[n as usize - 1] + (i - n + 1) as f32 * (p[n as usize - 1] - p[n as usize - 2])
        } else {
            p[i as usize]
        }
    }

    /// Video time of a beat.
    pub fn time_at(&self, timeline: &Timeline, beat: f32) -> f32 {
        let m = measures(timeline);
        if m.is_empty() {
            return self.points[0];
        }
        let k = m.partition_point(|&(s, _)| s <= beat).saturating_sub(1);
        let (start, len) = m[k];
        let frac = ((beat - start) / len).clamp(0.0, 1.0);
        let (a, b) = (self.start(k as isize, m.len()), self.start(k as isize + 1, m.len()));
        a + frac * (b - a)
    }

    /// Beat at a video time.
    pub fn beat_at(&self, timeline: &Timeline, t: f32) -> f32 {
        let m = measures(timeline);
        if m.is_empty() {
            return 0.0;
        }
        let n = m.len();
        let k = (0..n).rev().find(|&k| self.start(k as isize, n) <= t).unwrap_or(0);
        let (a, b) = (self.start(k as isize, n), self.start(k as isize + 1, n));
        if k + 1 == n && t >= b {
            return timeline.total;
        }
        let (start, len) = m[k];
        start + ((t - a) / (b - a).max(1e-3)).clamp(0.0, 1.0) * len
    }
}

/// (start beat, length) of every tab measure, in order.
fn measures(timeline: &Timeline) -> Vec<(f32, f32)> {
    timeline.steps.iter().filter(|s| matches!(s.kind, StepKind::Measure { .. })).map(|s| (s.start, s.beats)).collect()
}

#[derive(Default)]
struct State {
    ready: bool,
    time: f32,
    at: Option<Instant>,
    playing: bool,
    rate: f32,
    error: Option<String>,
}

pub struct Player {
    view: wry::WebView,
    state: Arc<Mutex<State>>,
    pub id: String,
    shown: bool,
}

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8">
<style>html,body{margin:0;height:100%;background:#000;overflow:hidden}#p{width:100%;height:100%}</style></head>
<body><div id="p"></div><script>
var player = null, ready = false, pending = null;
function post(o) { window.ipc.postMessage(JSON.stringify(o)); }
function send() {
  if (!ready) return;
  post({ t: player.getCurrentTime(), s: player.getPlayerState(), r: player.getPlaybackRate() });
}
function onYouTubeIframeAPIReady() {
  player = new YT.Player('p', {
    width: '100%', height: '100%', videoId: '__ID__',
    playerVars: { playsinline: 1, rel: 0, controls: 1, disablekb: 1, origin: location.origin },
    events: {
      onReady: function () { ready = true; send(); },
      onStateChange: send,
      onError: function (e) { post({ error: e.data }); }
    }
  });
}
setInterval(send, 100);
window.till = {
  play: function () {
    if (!ready) return;
    if (pending !== null) { player.seekTo(pending, true); pending = null; }
    player.playVideo();
  },
  pause: function () { ready && player.pauseVideo(); },
  // Seeking a video that has not started yet would start it: keep it for play().
  seek: function (t) {
    if (!ready) return;
    t = Math.max(0, t);
    var st = player.getPlayerState();
    if (st === -1 || st === 5) pending = t; else player.seekTo(t, true);
  },
  rate: function (r) { ready && player.setPlaybackRate(r); },
  load: function (id) { pending = null; ready && player.cueVideoById(id); }
};
var s = document.createElement('script');
s.src = 'https://www.youtube.com/iframe_api';
document.head.appendChild(s);
</script></body></html>"#;

/// Serves the player page on localhost; returns the port.
fn serve(id: &str) -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let page = PAGE.replace("__ID__", id);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            // Drain the request headers.
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let mut out = &stream;
            let _ = write!(
                out,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
        }
    });
    Ok(port)
}

fn bounds(rect: egui::Rect, zoom: f32) -> wry::Rect {
    wry::Rect {
        position: LogicalPosition::new(rect.left() * zoom, rect.top() * zoom).into(),
        size: LogicalSize::new(rect.width() * zoom, rect.height() * zoom).into(),
    }
}

impl Player {
    pub fn new(frame: &eframe::Frame, id: &str, rect: egui::Rect, zoom: f32) -> Result<Player, String> {
        let port = serve(id)?;
        let state = Arc::new(Mutex::new(State { rate: 1.0, ..Default::default() }));
        let st = state.clone();
        let view = wry::WebViewBuilder::new()
            .with_url(format!("http://localhost:{port}/"))
            .with_bounds(bounds(rect, zoom))
            .with_autoplay(true)
            .with_background_color((0, 0, 0, 255))
            .with_ipc_handler(move |req| {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(req.body()) else { return };
                let mut s = st.lock().unwrap();
                if let Some(code) = v["error"].as_i64() {
                    s.error = Some(match code {
                        101 | 150 | 153 => "Cette vidéo ne peut pas être lue hors de YouTube.".to_string(),
                        100 => "Vidéo introuvable (supprimée ou privée).".to_string(),
                        _ => format!("Erreur du lecteur YouTube ({code})."),
                    });
                    return;
                }
                s.ready = true;
                s.time = v["t"].as_f64().unwrap_or(0.0) as f32;
                s.at = Some(Instant::now());
                s.playing = v["s"].as_i64() == Some(1);
                s.rate = v["r"].as_f64().unwrap_or(1.0) as f32;
                s.error = None;
            })
            .build_as_child(frame)
            .map_err(|e| format!("Lecteur vidéo indisponible : {e}"))?;
        Ok(Player { view, state, id: id.to_string(), shown: true })
    }

    fn js(&self, code: &str) {
        let _ = self.view.evaluate_script(&format!("window.till && till.{code}"));
    }

    pub fn place(&mut self, rect: egui::Rect, zoom: f32) {
        let _ = self.view.set_bounds(bounds(rect, zoom));
        if !self.shown {
            let _ = self.view.set_visible(true);
            self.shown = true;
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            self.pause();
            let _ = self.view.set_visible(false);
            self.shown = false;
        }
    }

    pub fn load(&mut self, id: &str) {
        if self.id != id {
            self.id = id.to_string();
            self.js(&format!("load('{id}')"));
            let mut s = self.state.lock().unwrap();
            s.playing = false;
            s.error = None;
        }
    }

    pub fn play(&self) {
        self.js("play()");
    }

    pub fn pause(&self) {
        self.js("pause()");
        self.state.lock().unwrap().playing = false;
    }

    pub fn seek(&self, t: f32) {
        self.js(&format!("seek({t:.3})"));
        let mut s = self.state.lock().unwrap();
        s.time = t;
        s.at = Some(Instant::now());
    }

    pub fn set_rate(&self, r: f32) {
        let s = self.state.lock().unwrap();
        if (s.rate - r).abs() > 0.01 && s.ready {
            drop(s);
            self.js(&format!("rate({r})"));
        }
    }

    pub fn ready(&self) -> bool {
        self.state.lock().unwrap().ready
    }

    pub fn playing(&self) -> bool {
        self.state.lock().unwrap().playing
    }

    pub fn error(&self) -> Option<String> {
        self.state.lock().unwrap().error.clone()
    }

    /// Current video time, extrapolated between the player's reports.
    pub fn time(&self) -> f32 {
        let s = self.state.lock().unwrap();
        match (s.playing, s.at) {
            (true, Some(at)) => s.time + at.elapsed().as_secs_f32().min(0.5) * s.rate,
            _ => s.time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song;

    #[test]
    fn maps_beats_and_times() {
        let s = Sync::parse("abc_D-1 | 0 2 4").unwrap();
        assert_eq!(Sync::parse(&s.to_directive("video")[8..].trim_end_matches('}')), Some(s.clone()));
        let song = song::parse("e|----|----|----|\nB|----|----|----|\n", 0);
        let t = Timeline::build(&song, 4.0, 4.0);
        assert_eq!(t.total, 12.0);
        assert_eq!(s.time_at(&t, 0.0), 0.0);
        assert_eq!(s.time_at(&t, 6.0), 3.0);
        assert_eq!(s.time_at(&t, 10.0), 5.0);
        assert_eq!(s.beat_at(&t, 3.0), 6.0);
        assert_eq!(s.beat_at(&t, 5.0), 10.0);
        assert_eq!(s.beat_at(&t, 9.0), 12.0);
        // An extra entry at the start (intro): the measures follow the last ones.
        let extra = Sync::parse("x | 0 10 12 14").unwrap();
        assert_eq!(extra.time_at(&t, 0.0), 10.0);
        assert_eq!(extra.beat_at(&t, 13.0), 6.0);
        assert_eq!(extra.beat_at(&t, 5.0), 0.0);
    }
}

