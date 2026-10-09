//! Import from a Songsterr URL: fetches the song metadata and one track, then writes it
//! as a plain text tab that the rest of Till already knows how to read and play.
//!
//! Songsterr gives exact durations; text tabs only have columns. Each measure gets a
//! column grid fine enough to put every note exactly at its time, since playback spreads
//! a measure's columns evenly over one bar.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use serde_json::Value;

use crate::theme::ACCENT;

use crate::video::Sync;

const API: &str = "https://www.songsterr.com/api/meta";
const CDN: &str = "https://dqsljvtekg760.cloudfront.net";
/// Safety cap once repeats are expanded.
const MAX_MEASURES: usize = 3000;

#[derive(Clone)]
pub struct Track {
    pub index: usize,
    pub label: String,
    pub drums: bool,
}

#[derive(Clone)]
pub struct Meta {
    pub url: String,
    pub song_id: u64,
    pub revision: u64,
    pub image: String,
    pub title: String,
    pub artist: String,
    pub tracks: Vec<Track>,
    pub default_track: usize,
}

/// Song id and optional track index from a Songsterr URL (`…-tab-s1823159` or `…-s1823159t2`).
pub fn parse_url(url: &str) -> Option<(u64, Option<usize>)> {
    let path = url.trim().split(['?', '#']).next()?.trim_end_matches('/');
    let last = path.rsplit('/').next()?;
    let tail = &last[last.rfind("-s").map(|i| i + 2).or_else(|| last.starts_with('s').then_some(1))?..];
    let id: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = &tail[id.len()..];
    let track = rest.strip_prefix('t').and_then(|t| t.parse().ok());
    Some((id.parse().ok()?, track))
}

fn get_json(url: &str) -> Result<Value, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Macintosh) Till")
        .call()
        .map_err(|e| format!("Requête échouée : {e}"))?;
    let text = resp.into_string().map_err(|e| format!("Lecture impossible : {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("Réponse illisible : {e}"))
}

pub fn fetch_meta(url: &str) -> Result<Meta, String> {
    let (id, track) = parse_url(url).ok_or("Ce lien ne ressemble pas à une tab Songsterr (…-s123456).")?;
    let m = get_json(&format!("{API}/{id}"))?;
    let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
    let tracks: Vec<Track> = m["tracks"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let instrument = t["instrument"].as_str().unwrap_or("Piste");
            let name = t["name"].as_str().unwrap_or("").trim();
            Track {
                index: i,
                label: if name.is_empty() { instrument.to_string() } else { format!("{instrument} · {name}") },
                drums: t["tuning"].is_null() || t["instrumentId"].as_u64() == Some(1024),
            }
        })
        .collect();
    if tracks.iter().all(|t| t.drums) {
        return Err("Ce morceau n'a pas de piste à cordes.".into());
    }
    let preferred = track.or(m["popularTrackGuitar"].as_u64().map(|t| t as usize)).unwrap_or(0);
    let default_track = tracks
        .iter()
        .find(|t| t.index == preferred && !t.drums)
        .or_else(|| tracks.iter().find(|t| !t.drums))
        .map_or(0, |t| t.index);
    Ok(Meta {
        url: url.trim().to_string(),
        song_id: id,
        revision: m["revisionId"].as_u64().ok_or("Révision introuvable")?,
        image: s("image"),
        title: s("title").trim_matches(['"', '“', '”', ' ']).to_string(),
        artist: s("artist"),
        tracks,
        default_track,
    })
}

pub fn fetch_track(meta: &Meta, index: usize) -> Result<Value, String> {
    get_json(&format!("{CDN}/{}/{}/{}/{index}.json", meta.song_id, meta.revision, meta.image))
}

/// The song's YouTube video and a backing video without `track`, both synchronized
/// measure by measure (Songsterr's video points).
pub fn fetch_videos(meta: &Meta, track: usize) -> (Option<Sync>, Option<Sync>) {
    let url = format!("https://www.songsterr.com/api/video-points/{}/{}/list", meta.song_id, meta.revision);
    let Ok(Value::Array(list)) = get_json(&url) else { return (None, None) };
    let sync = |v: &Value| {
        let points: Vec<f32> = v["points"].as_array()?.iter().filter_map(|p| p.as_f64().map(|p| p as f32)).collect();
        let id = v["videoId"].as_str()?.to_string();
        (points.len() >= 2 && v["status"].as_str() == Some("done")).then_some(Sync { id, points })
    };
    let everywhere = |v: &Value| match v["countries"].as_array() {
        None => true,
        Some(c) => c.iter().any(|c| matches!(c.as_str(), Some("All" | "FR"))),
    };
    let feature = |v: &Value| v["feature"].as_str().map(String::from);
    let main = list
        .iter()
        .filter(|v| feature(v).is_none() && everywhere(v))
        .chain(list.iter().filter(|v| feature(v).as_deref() == Some("alternative") && everywhere(v)))
        .find_map(sync);
    let backing = list
        .iter()
        .filter(|v| feature(v).as_deref() == Some("backing"))
        .filter(|v| v["tracks"].as_array().is_some_and(|t| t.iter().any(|t| t.as_u64() == Some(track as u64))))
        .find_map(sync);
    (main, backing)
}

/// Adds the video directives after the `{time: …}` line.
pub fn with_videos(text: &str, main: Option<&Sync>, backing: Option<&Sync>) -> String {
    let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("{video")).map(String::from).collect();
    let at = lines.iter().position(|l| l.starts_with("{time:")).map_or(0, |i| i + 1);
    let extra: Vec<String> =
        [main.map(|m| m.to_directive("video")), backing.map(|b| b.to_directive("video_backing"))].into_iter().flatten().collect();
    lines.splice(at..at, extra);
    lines.join("\n") + "\n"
}

/// File name (without folder) and contents of the imported tab.
pub fn import(meta: &Meta, index: usize) -> Result<(String, String), String> {
    let track = fetch_track(meta, index)?;
    // The other pitched tracks (rhythm guitars, bass) help guess the harmony.
    let others: Vec<Value> = meta
        .tracks
        .iter()
        .filter(|t| !t.drums && t.index != index)
        .filter_map(|t| fetch_track(meta, t.index).ok())
        .collect();
    let label = meta.tracks.iter().find(|t| t.index == index).map_or("", |t| t.label.as_str());
    let text = to_text(meta, label, &track, &others);
    let (main, backing) = fetch_videos(meta, index);
    let text = with_videos(&text, main.as_ref(), backing.as_ref());
    let clean = |s: &str| s.chars().filter(|c| !"/\\:*?\"<>|".contains(*c)).collect::<String>().trim().to_string();
    let guitars = meta.tracks.iter().filter(|t| !t.drums).count();
    let mut name = clean(&meta.title);
    if guitars > 1 && index != meta.default_track {
        name += &format!(" (piste {})", index + 1);
    }
    if !meta.artist.is_empty() {
        name += &format!(" - {}", clean(&meta.artist));
    }
    Ok((format!("{name}.txt"), text))
}

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// One printed note: `string` 0 = top line (highest string).
struct Mark {
    pos: f64,
    string: usize,
    text: String,
    /// Legato symbol to print just before the note (h, p, /, \).
    link: Option<char>,
}

/// A sounding note, tied ones included, for chord detection.
struct Sound {
    pos: f64,
    len: f64,
    string: usize,
    midi: u8,
}

/// What one source measure contains.
#[derive(Default)]
struct Parsed {
    marks: Vec<Mark>,
    sounds: Vec<Sound>,
    /// Chord names written in the Songsterr tab, with their position.
    chords: Vec<(f64, String)>,
}

fn measure_marks(m: &Value, tuning: &[u64], prev: &mut Vec<Option<(u64, Option<&'static str>)>>) -> Parsed {
    let mut out = Parsed::default();
    let marks = &mut out.marks;
    for voice in m["voices"].as_array().map(|a| a.as_slice()).unwrap_or_default() {
        let mut pos = 0.0f64;
        for beat in voice["beats"].as_array().map(|a| a.as_slice()).unwrap_or_default() {
            let d = &beat["duration"];
            let len = d[0].as_f64().unwrap_or(1.0) / d[1].as_f64().unwrap_or(4.0).max(1.0);
            if beat["graceNote"].is_string() {
                continue;
            }
            if let Some(name) = beat["chord"]["text"].as_str() {
                out.chords.push((pos, name.trim().to_string()));
            }
            for n in beat["notes"].as_array().map(|a| a.as_slice()).unwrap_or_default() {
                let (Some(string), Some(fret)) = (n["string"].as_u64(), n["fret"].as_u64()) else { continue };
                let string = string as usize;
                if string >= prev.len() {
                    prev.resize(string + 1, None);
                }
                if n["dead"].as_bool() != Some(true) {
                    if let Some(&open) = tuning.get(string) {
                        out.sounds.push(Sound { pos, len, string, midi: (open + fret).min(127) as u8 });
                    }
                }
                if n["tie"].as_bool() == Some(true) {
                    continue;
                }
                let link = prev[string].and_then(|(pf, kind)| match kind {
                    Some("hp") => Some(if fret >= pf { 'h' } else { 'p' }),
                    Some(_) => Some(if fret >= pf { '/' } else { '\\' }),
                    None => None,
                });
                let mut text = if n["dead"].as_bool() == Some(true) { "x".to_string() } else { fret.to_string() };
                if n["ghost"].as_bool() == Some(true) {
                    text = format!("({text})");
                }
                if n["bend"].is_object() {
                    text.push('b');
                }
                let kind = if n["hp"].as_bool() == Some(true) {
                    Some("hp")
                } else if n["slide"].is_string() {
                    Some("slide")
                } else {
                    None
                };
                prev[string] = Some((fret, kind));
                marks.push(Mark { pos, string, text, link });
            }
            pos += len;
        }
    }
    out
}

/// Chord shapes tried by the detector, with a small cost so plain triads win ties.
const DETECT: &[(&str, f64)] = &[
    ("", 0.0),
    ("m", 0.0),
    ("5", 0.05),
    ("7", 0.08),
    ("m7", 0.08),
    ("maj7", 0.1),
    ("sus4", 0.12),
    ("sus2", 0.12),
    ("add9", 0.14),
    ("6", 0.14),
    ("m6", 0.14),
    ("dim", 0.18),
];

/// A bare root + fifth takes the third the song's scale gives it: E5 in G major is Em.
fn fill_power_chord(root: u8, q: &'static str, scale: Option<&[bool; 12]>) -> &'static str {
    match scale {
        Some(sc) if q == "5" && sc[((root + 4) % 12) as usize] => "",
        Some(sc) if q == "5" && sc[((root + 3) % 12) as usize] => "m",
        _ => q,
    }
}

/// Best chord name for these notes, with its score (higher is surer).
fn guess_chord(sounds: &[&Sound], scale: Option<&[bool; 12]>) -> Option<(String, f64)> {
    let mut weight = [0.0f64; 12];
    let mut total = 0.0;
    for s in sounds {
        let w = s.len.min(0.5);
        weight[(s.midi % 12) as usize] += w;
        total += w;
    }
    let distinct = weight.iter().filter(|&&w| w > 0.0).count();
    if total <= 0.0 || distinct < 2 {
        return None;
    }
    weight.iter_mut().for_each(|w| *w /= total);
    let bass = sounds.iter().map(|s| s.midi).min()? % 12;
    let mut best: Option<(u8, &str, f64)> = None;
    for root in 0..12u8 {
        for &(q, cost) in DETECT {
            let tones: Vec<u8> = crate::chords::intervals(q).iter().map(|i| (root + i) % 12).collect();
            // A power chord only names exactly two notes.
            if q == "5" && distinct != 2 {
                continue;
            }
            let mut score = 0.0;
            for pc in 0..12u8 {
                let w = weight[pc as usize];
                score += if tones.contains(&pc) { w } else { -1.2 * w };
            }
            for (k, &t) in tones.iter().enumerate() {
                if weight[t as usize] == 0.0 {
                    // A missing fifth is common; any other missing tone is suspicious.
                    score -= if k == 2 && q != "sus2" && q != "sus4" && q != "dim" { 0.05 } else { 0.25 };
                }
            }
            if weight[root as usize] == 0.0 {
                score -= 0.3;
            }
            if bass == root {
                score += 0.15;
            }
            score -= cost;
            if best.is_none_or(|b| score > b.2) {
                best = Some((root, q, score));
            }
        }
    }
    let (root, q, score) = best?;
    let q = fill_power_chord(root, q, scale);
    let mut name = crate::chords::Chord::from_parts(root, q).name();
    let tones: Vec<u8> = crate::chords::intervals(q).iter().map(|i| (root + i) % 12).collect();
    if bass != root && tones.contains(&bass) && q != "5" {
        // A third or leading tone in the bass reads as a sharp under sharp-side roots: E/G#, D/F#.
        let interval = (bass + 12 - root) % 12;
        let sharp = matches!(interval, 4 | 11) && !crate::chords::root_name(root).ends_with('b');
        let bass_name = if sharp { NOTE_NAMES[bass as usize] } else { crate::chords::root_name(bass) };
        name = format!("{name}/{bass_name}");
    }
    Some((name, score))
}

/// The major scale (as pitch classes) that holds most of the song's notes.
fn song_scale(parsed: &[Parsed]) -> Option<[bool; 12]> {
    let mut weight = [0.0f64; 12];
    for s in parsed.iter().flat_map(|p| &p.sounds) {
        weight[(s.midi % 12) as usize] += s.len;
    }
    let total: f64 = weight.iter().sum();
    let scale_of = |tonic: usize| -> [bool; 12] { std::array::from_fn(|pc| [0, 2, 4, 5, 7, 9, 11].contains(&((pc + 12 - tonic) % 12))) };
    let (best, inside) = (0..12)
        .map(|t| {
            let sc = scale_of(t);
            (t, (0..12).filter(|&pc| sc[pc]).map(|pc| weight[pc]).sum::<f64>())
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    // Only trust it when the music clearly sits in one scale.
    (total > 0.0 && inside / total > 0.85).then(|| scale_of(best))
}

/// Chords heard in one measure: one, or two when its halves clearly differ. Measures
/// that are just a melody line get none.
fn detect_chords(sounds: &[Sound], bar: f64, scale: Option<&[bool; 12]>) -> Vec<(f64, String)> {
    const SURE: f64 = 0.45;
    let all: Vec<&Sound> = sounds.iter().collect();
    let mut strings: Vec<usize> = all.iter().map(|s| s.string).collect();
    strings.sort_unstable();
    strings.dedup();
    let together = all
        .iter()
        .map(|a| all.iter().filter(|b| (b.pos - a.pos).abs() < 1e-9).count())
        .max()
        .unwrap_or(0);
    if together < 3 && strings.len() < 3 {
        return Vec::new();
    }
    let half = |first: bool| -> Vec<&Sound> {
        all.iter().copied().filter(|s| (s.pos < bar / 2.0 - 1e-9) == first).collect()
    };
    let whole = guess_chord(&all, scale);
    let (a, b) = (guess_chord(&half(true), scale), guess_chord(&half(false), scale));
    if let (Some(a), Some(b)) = (&a, &b) {
        let split = (a.1 + b.1) / 2.0;
        if a.0 != b.0 && a.1 >= SURE && b.1 >= SURE && whole.as_ref().is_none_or(|w| split > w.1 + 0.15) {
            return vec![(0.0, a.0.clone()), (bar / 2.0, b.0.clone())];
        }
    }
    whole.filter(|w| w.1 >= SURE).map(|w| vec![(0.0, w.0)]).unwrap_or_default()
}

/// Places marks on a grid of `w` columns for a bar of `bar` whole notes. `None` when the
/// grid can't hold them exactly with a gap between consecutive notes on a string.
fn place(marks: &[Mark], bar: f64, w: usize, exact: bool) -> Option<Vec<usize>> {
    let mut cols = Vec::with_capacity(marks.len());
    let mut end = vec![0usize; 16];
    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by(|&a, &b| marks[a].pos.total_cmp(&marks[b].pos));
    cols.resize(marks.len(), 0);
    for i in order {
        let m = &marks[i];
        let x = m.pos / bar * w as f64;
        let mut col = x.round() as usize;
        if exact && (x - x.round()).abs() > 1e-6 {
            return None;
        }
        if m.string >= end.len() {
            end.resize(m.string + 1, 0);
        }
        let min = if end[m.string] == 0 { 0 } else { end[m.string] + 1 };
        if col < min {
            if exact {
                return None;
            }
            col = min;
        }
        if m.link.is_some() && col == 0 {
            // Room for the legato symbol before the note.
            if exact {
                return None;
            }
            col = 1;
        }
        let e = col + m.text.chars().count();
        if exact && e > w {
            return None;
        }
        end[m.string] = e;
        cols[i] = col;
    }
    Some(cols)
}

/// Text body (between bar lines) of one measure, per string.
fn render_measure(marks: &[Mark], strings: usize, num: u64, den: u64) -> (Vec<String>, usize) {
    let bar = num as f64 / den.max(1) as f64;
    let quarters = (bar * 4.0).max(0.25);
    // Columns per quarter note, coarse first: the first grid that fits exactly wins.
    let mut chosen = None;
    for q in [4usize, 6, 8, 12, 16, 24, 32, 48] {
        let w = (quarters * q as f64).round().max(4.0) as usize;
        if let Some(c) = place(marks, bar, w, true) {
            chosen = Some((w, c));
            break;
        }
    }
    let (mut w, cols) = chosen.unwrap_or_else(|| {
        let w = (quarters * 16.0).round().max(4.0) as usize;
        (w, place(marks, bar, w, false).unwrap_or_default())
    });
    for (m, &c) in marks.iter().zip(&cols) {
        w = w.max(c + m.text.chars().count());
    }
    let mut rows = vec![vec!['-'; w]; strings];
    for (m, &c) in marks.iter().zip(&cols) {
        let Some(row) = rows.get_mut(m.string) else { continue };
        for (k, ch) in m.text.chars().enumerate() {
            row[c + k] = ch;
        }
        if let (Some(l), true) = (m.link, c > 0) {
            if row[c - 1] == '-' {
                row[c - 1] = l;
            }
        }
    }
    (rows.into_iter().map(|r| r.into_iter().collect()).collect(), w)
}

/// Measure indices in playing order, with repeats expanded.
fn play_order(measures: &[Value]) -> Vec<usize> {
    let mut order = Vec::new();
    let mut start = 0;
    let mut passes = vec![0u64; measures.len()];
    // Current pass through the repeated section, for alternate endings ("1.", "2.").
    let mut pass = 1;
    let mut i = 0;
    while i < measures.len() && order.len() < MAX_MEASURES {
        let m = &measures[i];
        if m["repeatStart"].as_bool() == Some(true) {
            if start != i {
                pass = 1;
            }
            start = i;
        }
        let ending = m["alternateEnding"].as_array();
        if ending.is_none_or(|e| e.iter().any(|p| p.as_u64() == Some(pass))) {
            order.push(i);
        }
        if let Some(times) = m["repeat"].as_u64() {
            passes[i] += 1;
            if passes[i] < times {
                pass = passes[i] + 1;
                i = start;
                continue;
            }
            passes[i] = 0;
        }
        i += 1;
    }
    order
}

pub fn to_text(meta: &Meta, track_label: &str, track: &Value, others: &[Value]) -> String {
    let tuning: Vec<u64> = track["tuning"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .filter(|t: &Vec<u64>| !t.is_empty())
        .unwrap_or_else(|| vec![64, 59, 55, 50, 45, 40]);
    let strings = tuning.len();
    let mut labels: Vec<String> = tuning.iter().map(|&m| NOTE_NAMES[(m % 12) as usize].to_string()).collect();
    if strings > 1 && labels[0] == labels[strings - 1] {
        labels[0] = labels[0].to_lowercase();
    }
    let label_w = labels.iter().map(|l| l.chars().count()).max().unwrap_or(1);

    let measures = track["measures"].as_array().map(|a| a.as_slice()).unwrap_or_default();
    let bpm = track["automations"]["tempo"][0]["bpm"].as_f64();
    let first_sig = measures.iter().find_map(|m| m["signature"].as_array().cloned());
    let (num, den) = first_sig
        .as_ref()
        .map_or((4, 4), |s| (s[0].as_u64().unwrap_or(4), s.get(1).and_then(Value::as_u64).unwrap_or(4)));

    let mut out = String::new();
    out += &format!("{{title: {}}}\n", meta.title);
    if let Some(b) = bpm {
        out += &format!("{{tempo: {}}}\n", b.round());
    }
    out += &format!("{{time: {num}/{den}}}\n");
    out += &format!("{} — {track_label}\n", if meta.artist.is_empty() { "Artiste inconnu" } else { &meta.artist });
    let tuning_names: Vec<&str> = tuning.iter().rev().map(|&m| NOTE_NAMES[(m % 12) as usize]).collect();
    out += &format!("Accordage : {}\n", tuning_names.join(" "));
    out += &format!("Importé depuis Songsterr : {}\n", meta.url);

    // Signatures and the legato state follow the source order, not the expanded one.
    let mut sigs = Vec::with_capacity(measures.len());
    let (mut n, mut d) = (num, den);
    for m in measures {
        if let Some(s) = m["signature"].as_array() {
            n = s[0].as_u64().unwrap_or(n);
            d = s.get(1).and_then(Value::as_u64).unwrap_or(d);
        }
        sigs.push((n, d));
    }
    let mut prev = vec![None; strings];
    let mut parsed: Vec<Parsed> = measures.iter().map(|m| measure_marks(m, &tuning, &mut prev)).collect();
    for (k, other) in others.iter().enumerate() {
        let tuning: Vec<u64> = other["tuning"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
        let mut prev = Vec::new();
        let ms = other["measures"].as_array().map(|a| a.as_slice()).unwrap_or_default();
        for (p, m) in parsed.iter_mut().zip(ms) {
            let o = measure_marks(m, &tuning, &mut prev);
            // Distinct string numbers per track so the "several strings" test counts them apart.
            p.sounds.extend(o.sounds.into_iter().map(|s| Sound { string: s.string + 16 * (k + 1), ..s }));
            if p.chords.is_empty() {
                p.chords = o.chords;
            }
        }
    }
    let bodies: Vec<(Vec<String>, usize)> =
        parsed.iter().zip(&sigs).map(|(p, &(n, d))| render_measure(&p.marks, strings, n, d)).collect();
    // Chords written by the transcriber win; otherwise they are guessed from the notes.
    let given = parsed.iter().any(|p| p.chords.iter().any(|(_, c)| crate::chords::Chord::parse(c).is_some()));
    let scale = song_scale(&parsed);
    let chords: Vec<Vec<(usize, String)>> = parsed
        .iter()
        .zip(&sigs)
        .zip(&bodies)
        .map(|((p, &(n, d)), (_, w))| {
            let bar = n as f64 / d.max(1) as f64;
            let list = if given {
                p.chords
                    .iter()
                    .filter_map(|(pos, c)| {
                        let chord = crate::chords::Chord::parse(c)?;
                        let name = if chord.quality == "5" && chord.bass.is_none() {
                            let q = fill_power_chord(chord.root, "5", scale.as_ref());
                            crate::chords::Chord::from_parts(chord.root, q).name()
                        } else {
                            chord.name()
                        };
                        Some((*pos, name))
                    })
                    .collect()
            } else {
                detect_chords(&p.sounds, bar, scale.as_ref())
            };
            list.into_iter().map(|(pos, c)| (((pos / bar) * *w as f64).round() as usize, c)).collect()
        })
        .collect();

    // Measures of the current line, with the chord names to print above each.
    let mut line: Vec<(usize, Vec<(usize, String)>)> = Vec::new();
    let flush = |out: &mut String, line: &mut Vec<(usize, Vec<(usize, String)>)>| {
        if line.is_empty() {
            return;
        }
        out.push('\n');
        let mut above = String::new();
        let mut x = label_w + 1;
        for (i, names) in line.iter() {
            for (col, name) in names {
                let at = x + col;
                let len = above.chars().count();
                let at = if len == 0 { at } else { at.max(len + 1) };
                if at >= x + bodies[*i].1 {
                    continue;
                }
                above.extend(std::iter::repeat_n(' ', at - len));
                above.push_str(name);
            }
            x += bodies[*i].1 + 1;
        }
        if !above.is_empty() {
            out.push_str(&above);
            out.push('\n');
        }
        for s in 0..strings {
            out.push_str(&format!("{:<label_w$}|", labels[s]));
            for (i, _) in line.iter() {
                out.push_str(&bodies[*i].0[s]);
                out.push('|');
            }
            out.push('\n');
        }
        line.clear();
    };
    let mut last_chord: Option<String> = None;
    let mut changed_sig = false;
    for i in play_order(measures) {
        if let Some(text) = measures[i]["marker"]["text"].as_str() {
            flush(&mut out, &mut line);
            out += &format!("\n[{}]\n", text.trim());
            last_chord = None;
        }
        changed_sig |= sigs[i] != (num, den);
        // Like a lead sheet: a name only where the chord changes.
        let mut names = Vec::new();
        for (col, c) in &chords[i] {
            if last_chord.as_ref() != Some(c) {
                names.push((*col, c.clone()));
                last_chord = Some(c.clone());
            }
        }
        line.push((i, names));
        if line.len() == 4 {
            flush(&mut out, &mut line);
        }
    }
    flush(&mut out, &mut line);
    if changed_sig {
        out += "\nNote : ce morceau change de mesure en cours de route ; Till joue toutes les mesures à la même durée.\n";
    }
    out
}

enum State {
    Idle,
    Loading,
    Ready { meta: Meta, track: usize },
    Importing,
    Failed(String),
}

type Slot<T> = Arc<Mutex<Option<Result<T, String>>>>;

/// The "Importer depuis Songsterr" window.
pub struct Importer {
    pub open: bool,
    url: String,
    state: State,
    meta_slot: Slot<Meta>,
    file_slot: Slot<(String, String)>,
}

impl Default for Importer {
    fn default() -> Self {
        Importer {
            open: false,
            url: String::new(),
            state: State::Idle,
            meta_slot: Default::default(),
            file_slot: Default::default(),
        }
    }
}

fn spawn<T: Send + 'static>(ctx: &egui::Context, slot: &Slot<T>, job: impl FnOnce() -> Result<T, String> + Send + 'static) {
    let (slot, ctx) = (slot.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = job();
        *slot.lock().unwrap() = Some(r);
        ctx.request_repaint();
    });
}

impl Importer {
    pub fn show_with(&mut self, url: String) {
        self.open = true;
        self.url = url;
        self.state = State::Idle;
    }

    fn load(&mut self, ctx: &egui::Context) {
        let url = self.url.clone();
        self.state = State::Loading;
        spawn(ctx, &self.meta_slot, move || fetch_meta(&url));
    }

    /// Shows the window; returns the file (name, contents) once a track is imported.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<(String, String)> {
        if let Some(r) = self.meta_slot.lock().unwrap().take() {
            self.state = match r {
                Ok(meta) => State::Ready { track: meta.default_track, meta },
                Err(e) => State::Failed(e),
            };
        }
        let mut done = None;
        if let Some(r) = self.file_slot.lock().unwrap().take() {
            match r {
                Ok(f) => {
                    done = Some(f);
                    self.open = false;
                    self.url.clear();
                    self.state = State::Idle;
                }
                Err(e) => self.state = State::Failed(e),
            }
        }
        if !self.open {
            return done;
        }
        let mut open = self.open;
        egui::Window::new("Importer depuis Songsterr")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(RichText::new("Colle le lien d'une tab Songsterr.").weak());
                ui.add_space(4.0);
                let busy = matches!(self.state, State::Loading | State::Importing);
                let mut go = false;
                ui.horizontal(|ui| {
                    let edit = ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut self.url)
                            .hint_text("https://www.songsterr.com/a/wsa/…-tab-s123456")
                            .desired_width(360.0),
                    );
                    if edit.changed() && matches!(self.state, State::Ready { .. } | State::Failed(_)) {
                        self.state = State::Idle;
                    }
                    go |= edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    go |= ui.add_enabled(!busy && !self.url.trim().is_empty(), egui::Button::new("Charger")).clicked();
                });
                if go && !self.url.trim().is_empty() {
                    self.load(ctx);
                }
                ui.add_space(8.0);
                let mut import = None;
                match &mut self.state {
                    State::Idle => {}
                    State::Loading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Chargement du morceau…");
                        });
                    }
                    State::Importing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Téléchargement de la piste…");
                        });
                    }
                    State::Failed(e) => {
                        ui.colored_label(ui.visuals().error_fg_color, e.as_str());
                    }
                    State::Ready { meta, track } => {
                        ui.label(RichText::new(&meta.title).size(17.0).strong());
                        if !meta.artist.is_empty() {
                            ui.label(RichText::new(&meta.artist).weak());
                        }
                        ui.add_space(6.0);
                        ui.label("Piste :");
                        for t in meta.tracks.iter().filter(|t| !t.drums) {
                            ui.radio_value(track, t.index, format!("{} · {}", t.index + 1, t.label));
                        }
                        ui.add_space(8.0);
                        let btn = egui::Button::new(RichText::new("Importer").color(ui.visuals().selection.stroke.color)).fill(ACCENT);
                        if ui.add(btn).clicked() {
                            import = Some((meta.clone(), *track));
                        }
                    }
                }
                if let Some((meta, track)) = import {
                    self.state = State::Importing;
                    spawn(ctx, &self.file_slot, move || import_track(&meta, track));
                }
            });
        self.open = open;
        done
    }
}

fn import_track(meta: &Meta, track: usize) -> Result<(String, String), String> {
    import(meta, track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_with_endings() {
        // |: 0 1 [1.-2. 2] [3. 3] :| x3, then 4.
        let m: Vec<Value> = serde_json::from_str(
            r#"[{"repeatStart":true},{},{"alternateEnding":[1,2]},{"alternateEnding":[3],"repeat":3},{}]"#,
        )
        .unwrap();
        assert_eq!(super::play_order(&m), [0, 1, 2, 0, 1, 2, 0, 1, 3, 4]);
    }

    #[test]
    fn urls() {
        assert_eq!(parse_url("https://www.songsterr.com/a/wsa/kpop-demon-hunters-golden-tab-s1823159"), Some((1823159, None)));
        assert_eq!(parse_url("https://www.songsterr.com/a/wsa/nirvana-smells-like-teen-spirit-tab-s269t2?x=1"), Some((269, Some(2))));
        assert_eq!(parse_url("https://www.songsterr.com/"), None);
    }

    fn note(fret: u64, string: u64, d: u64) -> Value {
        serde_json::json!({"notes": [{"fret": fret, "string": string}], "duration": [1, d]})
    }

    fn strum(midis: &[u8], pos: f64) -> Vec<Sound> {
        midis.iter().enumerate().map(|(i, &midi)| Sound { pos, len: 0.5, string: 5 - i, midi }).collect()
    }

    #[test]
    fn detects_chords() {
        // Open C, then G with B in the bass: two chords in one measure.
        let mut notes = strum(&[48, 52, 55, 60, 64], 0.0);
        notes.extend(strum(&[47, 50, 55, 59, 67], 0.5));
        let got: Vec<String> = detect_chords(&notes, 1.0, None).into_iter().map(|c| c.1).collect();
        assert_eq!(got, ["C", "G/B"]);
        // A melody alone names nothing.
        let melody: Vec<Sound> = [64u8, 67, 69, 67].iter().enumerate().map(|(i, &midi)| Sound { pos: i as f64 / 4.0, len: 0.25, string: 0, midi }).collect();
        assert!(detect_chords(&melody, 1.0, None).is_empty());
        // E5 power chord in G major reads as Em.
        let g_major: [bool; 12] = std::array::from_fn(|pc| [0, 2, 4, 5, 7, 9, 11].contains(&((pc + 5) % 12)));
        let e5 = strum(&[40, 47, 52], 0.0);
        assert_eq!(detect_chords(&e5, 1.0, Some(&g_major))[0].1, "Em");
        assert_eq!(detect_chords(&e5, 1.0, None)[0].1, "E5");
    }

    #[test]
    fn measures_keep_exact_timing() {
        let meta = Meta {
            url: String::new(),
            song_id: 1,
            revision: 1,
            image: String::new(),
            title: "T".into(),
            artist: "A".into(),
            tracks: vec![],
            default_track: 0,
        };
        // Bar 1: eighths with two-digit frets; bar 2: a triplet; bar 3: repeated twice.
        let track = serde_json::json!({
            "tuning": [64, 59, 55, 50, 45, 40],
            "automations": {"tempo": [{"bpm": 123}]},
            "measures": [
                {"signature": [4, 4], "voices": [{"beats": (0..8).map(|i| note(10 + i % 3, 1, 8)).collect::<Vec<_>>()}]},
                {"voices": [{"beats": [note(0, 0, 4), note(1, 0, 12), note(2, 0, 12), note(3, 0, 12), note(5, 5, 2)]}]},
                {"repeatStart": true, "repeat": 2, "voices": [{"beats": [note(3, 5, 1)]}]}
            ]
        });
        let text = to_text(&meta, "Guitare", &track, &[]);
        assert!(text.contains("{tempo: 123}") && text.contains("{time: 4/4}"), "{text}");
        let song = crate::song::parse(&text, 0);
        let tl = crate::playback::Timeline::build(&song, 4.0, 4.0);
        let beats: Vec<f32> = tl.onsets.iter().map(|o| o.beat).collect();
        let want = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 5.0 + 1.0 / 3.0, 5.0 + 2.0 / 3.0, 6.0, 8.0, 12.0];
        assert_eq!(beats.len(), want.len(), "{text}\n{beats:?}");
        for (b, w) in beats.iter().zip(want) {
            assert!((b - w).abs() < 1e-4, "{beats:?}\n{text}");
        }
        assert_eq!(tl.onsets[0].notes[0].midi, 59 + 10);
    }
}

#[cfg(test)]
mod live {
    /// `TILL_VIDEO_FILE=tab.txt cargo test live` adds the Songsterr video to an earlier import.
    #[test]
    fn add_video() {
        let Ok(path) = std::env::var("TILL_VIDEO_FILE") else { return };
        let text = std::fs::read_to_string(&path).unwrap();
        let url = text.lines().find_map(|l| l.strip_prefix("Importé depuis Songsterr : ")).unwrap().trim().to_string();
        let meta = super::fetch_meta(&url).unwrap();
        let track = super::parse_url(&url).and_then(|(_, t)| t).unwrap_or(meta.default_track);
        let (main, backing) = super::fetch_videos(&meta, track);
        println!("{path}: video {:?}, backing {:?}", main.as_ref().map(|m| &m.id), backing.as_ref().map(|b| &b.id));
        std::fs::write(&path, super::with_videos(&text, main.as_ref(), backing.as_ref())).unwrap();
    }

    /// `TILL_SONGSTERR_URL=… TILL_SONGSTERR_OUT=file.txt cargo test live` imports a real song.
    #[test]
    fn import_url() {
        let (Ok(url), Ok(out)) = (std::env::var("TILL_SONGSTERR_URL"), std::env::var("TILL_SONGSTERR_OUT")) else { return };
        let meta = super::fetch_meta(&url).unwrap();
        // Optional: pick the track whose label matches TILL_SONGSTERR_TRACK.
        let track = std::env::var("TILL_SONGSTERR_TRACK")
            .ok()
            .and_then(|l| meta.tracks.iter().find(|t| t.label == l).map(|t| t.index))
            .unwrap_or(meta.default_track);
        let (name, text) = super::import(&meta, track).unwrap();
        let song = crate::song::parse(&text, 0);
        let tl = crate::playback::Timeline::build(&song, 4.0, 4.0);
        println!("{name}: {} steps, {} onsets, {} beats", tl.steps.len(), tl.onsets.len(), tl.total);
        println!("chords: {:?}", song.chords.iter().map(|c| c.name.as_str()).collect::<Vec<_>>());
        std::fs::write(out, text).unwrap();
    }
}

#[cfg(test)]
mod compare {
    /// `TILL_SONGSTERR_COMPARE=<url>`: detected chords next to the transcriber's, per measure.
    #[test]
    fn detection_vs_given() {
        let Ok(url) = std::env::var("TILL_SONGSTERR_COMPARE") else { return };
        let meta = super::fetch_meta(&url).unwrap();
        let track = super::fetch_track(&meta, meta.default_track).unwrap();
        let tuning: Vec<u64> = track["tuning"].as_array().unwrap().iter().filter_map(|v| v.as_u64()).collect();
        let mut prev = vec![None; tuning.len()];
        let (mut same, mut total) = (0, 0);
        for (i, m) in track["measures"].as_array().unwrap().iter().enumerate() {
            let p = super::measure_marks(m, &tuning, &mut prev);
            let got = super::detect_chords(&p.sounds, 1.0, None);
            let given: Vec<&str> = p.chords.iter().map(|c| c.1.as_str()).collect();
            let got: Vec<&str> = got.iter().map(|c| c.1.as_str()).collect();
            if !given.is_empty() {
                total += 1;
                same += (given.first() == got.first()) as i32;
            }
            println!("{i:3} given {given:?}  detected {got:?}");
        }
        println!("first chord agrees on {same}/{total} measures");
    }
}
