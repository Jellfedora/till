//! Tempo-driven playback: turns a song into a timeline of measures and chords.
//!
//! Text tabs carry no explicit rhythm, so we use the usual convention: each `|...|`
//! measure lasts one bar and its columns are spread evenly over it. In chords-over-lyrics
//! parts, each chord lasts a configurable number of beats.

use std::collections::BTreeSet;

use crate::song::{Block, Song};

/// Columns per bar when a tab has no bar lines.
const COLS_WITHOUT_BARS: usize = 16;

pub enum StepKind {
    /// Columns `c0..c1` of a tab block (cell units).
    Measure { c0: f32, c1: f32 },
    /// Token index within a chord line.
    Chord { tok: usize },
}

pub struct Step {
    pub block: usize,
    pub kind: StepKind,
    pub start: f32,
    pub beats: f32,
}

pub struct Note {
    /// 0 = lowest string.
    pub string: u8,
    pub midi: u8,
    pub gain: f32,
    pub delay_ms: f32,
    /// 0 = soft finger pluck, 1 = bright pick attack.
    pub bright: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strum {
    pub down: bool,
    /// Time for the pick to cross all the strings.
    pub sweep_ms: f32,
}

/// Notes that start together at `beat`.
pub struct Onset {
    pub beat: f32,
    pub notes: Vec<Note>,
    /// Set when the notes form a strummed chord.
    pub strum: Option<Strum>,
}

#[derive(Default)]
pub struct Timeline {
    pub steps: Vec<Step>,
    pub total: f32,
    pub onsets: Vec<Onset>,
}

/// Open-string MIDI pitches, lowest string first.
fn tuning(strings: usize) -> &'static [u8] {
    match strings {
        4 => &[28, 33, 38, 43],
        7 => &[35, 40, 45, 50, 55, 59, 64],
        _ => &[40, 45, 50, 55, 59, 64],
    }
}

/// Turns simultaneous notes into either a pluck (1–2 notes) or a strum (3+ notes).
///
/// Down strokes sweep low → high across every string; up strokes sweep high → low,
/// lighter, catching only the top four strings, as a real strumming hand does.
fn voice(mut notes: Vec<Note>, down: bool) -> (Vec<Note>, Option<Strum>) {
    notes.sort_by_key(|n| n.string);
    if notes.len() < 3 {
        return (notes, None);
    }
    if !down {
        let skip = notes.len().saturating_sub(4);
        notes.drain(..skip);
        notes.reverse();
    }
    let per_string = if down { 11.0 } else { 8.0 };
    let n = notes.len();
    for (i, note) in notes.iter_mut().enumerate() {
        note.delay_ms = i as f32 * per_string;
        note.bright = 1.0;
        // Accent the first strings hit; up strokes are lighter overall.
        let fade = 1.0 - 0.25 * i as f32 / n as f32;
        note.gain *= fade * if down { 0.75 } else { 0.5 };
    }
    let sweep_ms = per_string * n as f32;
    (notes, Some(Strum { down, sweep_ms }))
}

fn tab_onsets(lines: &[crate::song::TabLine], step: &Step, c0: f32, c1: f32) -> Vec<Onset> {
    let tune = tuning(lines.len());
    let mut by_col: std::collections::BTreeMap<usize, Vec<Note>> = Default::default();
    for (row, l) in lines.iter().enumerate() {
        let string = lines.len() - 1 - row;
        let Some(&open) = tune.get(string.min(tune.len() - 1)) else { continue };
        let mut j = c0 as usize;
        while j < (c1 as usize).min(l.body.len()) {
            if !l.body[j].is_ascii_digit() || (j > 0 && l.body[j - 1].is_ascii_digit()) {
                j += 1;
                continue;
            }
            let digits: String = l.body[j..].iter().take_while(|c| c.is_ascii_digit()).collect();
            let legato = j > 0 && matches!(l.body[j - 1], '/' | '\\' | 'h' | 'p' | 's');
            if let Ok(fret) = digits.parse::<u8>() {
                by_col.entry(j).or_default().push(Note {
                    string: string as u8,
                    midi: open.saturating_add(fret),
                    gain: if legato { 0.45 } else { 0.9 },
                    delay_ms: 0.0,
                    bright: 0.0,
                });
            }
            j += digits.len();
        }
    }
    by_col
        .into_iter()
        .map(|(col, notes)| {
            let local = (col as f32 - c0) / (c1 - c0) * step.beats;
            // Eighth notes on the beat are down strokes, those between beats are up strokes.
            let down = (local * 2.0 + 0.01).floor() as i32 % 2 == 0;
            let (notes, strum) = voice(notes, down);
            Onset { beat: step.start + local, notes, strum }
        })
        .collect()
}

/// What the view needs to draw the playhead.
pub struct Playhead {
    pub block: usize,
    pub col: Option<f32>,
    /// Playing chord token as (chord-line block, token index).
    pub tok: Option<(usize, usize)>,
    pub chord: Option<usize>,
}

/// A chord line directly above a tab block names the chords of its measures.
fn attached_chords(song: &Song, tab_block: usize) -> Option<&Vec<crate::song::ChordTok>> {
    match song.blocks.get(tab_block.checked_sub(1)?)? {
        Block::Chords(toks) => Some(toks),
        _ => None,
    }
}

pub enum Seek {
    Col { block: usize, col: f32 },
    Tok { block: usize, tok: usize },
}

pub fn measures(lines: &[crate::song::TabLine]) -> Vec<(f32, f32)> {
    let len = lines.iter().map(|l| l.body.len()).max().unwrap_or(0);
    let bars: BTreeSet<usize> = lines
        .iter()
        .flat_map(|l| l.body.iter().enumerate().filter(|(_, c)| **c == '|').map(|(j, _)| j))
        .collect();
    if bars.is_empty() {
        return (0..len)
            .step_by(COLS_WITHOUT_BARS)
            .map(|s| (s as f32, (s + COLS_WITHOUT_BARS).min(len) as f32))
            .collect();
    }
    let mut out = Vec::new();
    let mut prev = 0usize;
    let mut prev_is_bar = false;
    for edge in bars.iter().copied().chain(std::iter::once(len)) {
        let start = if prev_is_bar { prev + 1 } else { prev };
        if edge >= start + 2 {
            out.push((start as f32, edge as f32));
        }
        prev = edge;
        prev_is_bar = true;
    }
    out
}

impl Timeline {
    pub fn build(song: &Song, beats_per_bar: f32, beats_per_chord: f32) -> Timeline {
        let mut t = Timeline::default();
        fn push(t: &mut Timeline, block: usize, kind: StepKind, beats: f32) {
            t.steps.push(Step { block, kind, start: t.total, beats });
            t.total += beats;
        }
        for (bi, block) in song.blocks.iter().enumerate() {
            match block {
                Block::Tab(lines) => {
                    for (c0, c1) in measures(lines) {
                        push(&mut t, bi, StepKind::Measure { c0, c1 }, beats_per_bar);
                        let step = t.steps.last().unwrap();
                        let onsets = tab_onsets(lines, step, c0, c1);
                        t.onsets.extend(onsets);
                    }
                }
                Block::Chords(_) if matches!(song.blocks.get(bi + 1), Some(Block::Tab(_))) => {}
                Block::Chords(toks) => {
                    for (i, tok) in toks.iter().enumerate() {
                        let Some(idx) = tok.idx else { continue };
                        push(&mut t, bi, StepKind::Chord { tok: i }, beats_per_chord);
                        if let Some(v) = &song.chords[idx].voicing {
                            let notes = v
                                .frets
                                .iter()
                                .enumerate()
                                .filter_map(|(s, f)| {
                                    f.map(|f| Note { string: s as u8, midi: tuning(6)[s] + f + song.capo, gain: 0.9, delay_ms: 0.0, bright: 0.0 })
                                })
                                .collect();
                            let beat = t.steps.last().unwrap().start;
                            let (notes, strum) = voice(notes, true);
                            t.onsets.push(Onset { beat, notes, strum });
                        }
                    }
                }
                _ => {}
            }
        }
        t.onsets.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        t
    }

    /// Start and end beat of the step playing at `beat`.
    pub fn step_bounds(&self, beat: f32) -> Option<(f32, f32)> {
        let i = self.steps.partition_point(|s| s.start + s.beats <= beat).min(self.steps.len().checked_sub(1)?);
        let s = &self.steps[i];
        Some((s.start, s.start + s.beats))
    }

    /// Onsets with `from <= beat < to`.
    pub fn onsets_between(&self, from: f32, to: f32) -> &[Onset] {
        let a = self.onsets.partition_point(|o| o.beat < from);
        let b = self.onsets.partition_point(|o| o.beat < to);
        &self.onsets[a..b.max(a)]
    }

    pub fn playhead(&self, song: &Song, beat: f32) -> Option<Playhead> {
        let i = self.steps.partition_point(|s| s.start + s.beats <= beat);
        let s = self.steps.get(i)?;
        let frac = ((beat - s.start) / s.beats).clamp(0.0, 1.0);
        Some(match s.kind {
            StepKind::Measure { c0, c1 } => {
                let col = c0 + frac * (c1 - c0);
                let offset = match &song.blocks[s.block] {
                    Block::Tab(lines) => lines[0].offset as f32,
                    _ => 0.0,
                };
                let tok = attached_chords(song, s.block).and_then(|toks| {
                    toks.iter().rposition(|t| t.idx.is_some() && t.col as f32 - offset <= col + 0.5)
                });
                let chord = tok.and_then(|t| attached_chords(song, s.block)?[t].idx);
                Playhead { block: s.block, col: Some(col), tok: tok.map(|t| (s.block - 1, t)), chord }
            }
            StepKind::Chord { tok } => {
                let chord = match &song.blocks[s.block] {
                    Block::Chords(toks) => toks[tok].idx,
                    _ => None,
                };
                Playhead { block: s.block, col: None, tok: Some((s.block, tok)), chord }
            }
        })
    }

    /// Beat position for a click in the view.
    pub fn seek(&self, seek: &Seek) -> Option<f32> {
        self.steps.iter().find_map(|s| match (seek, &s.kind) {
            (Seek::Col { block, col }, StepKind::Measure { c0, c1 })
                if *block == s.block && *col >= *c0 && *col < *c1 =>
            {
                Some(s.start + (col - c0) / (c1 - c0) * s.beats)
            }
            (Seek::Tok { block, tok }, StepKind::Chord { tok: t }) if *block == s.block && tok == t => Some(s.start),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::parse;

    #[test]
    fn chords_above_tab_follow_the_playhead() {
        let src = "  G       C\ne|-3-------0------|\nB|-0-------1------|\n";
        let song = parse(src, 0);
        let t = Timeline::build(&song, 4.0, 4.0);
        assert_eq!(t.steps.len(), 1, "attached chord line must not be its own step");
        assert_eq!(song.chords[t.playhead(&song, 0.5).unwrap().chord.unwrap()].name, "G");
        assert_eq!(song.chords[t.playhead(&song, 2.5).unwrap().chord.unwrap()].name, "C");
    }

    #[test]
    fn measures_and_chords() {
        let src = "e|----|--------|\nB|-0--|---1----|\n\n G   C\nla la\n";
        let song = parse(src, 0);
        let t = Timeline::build(&song, 4.0, 2.0);
        assert_eq!(t.steps.len(), 4);
        assert_eq!(t.total, 4.0 * 2.0 + 2.0 * 2.0);
        let p = t.playhead(&song, 6.0).unwrap();
        assert_eq!(p.col, Some(10.0));
        let p = t.playhead(&song, 10.5).unwrap();
        assert_eq!(p.tok, Some((2, 1)));
        assert!(t.playhead(&song, 12.0).is_none());
        assert_eq!(t.seek(&Seek::Col { block: 0, col: 10.0 }), Some(6.0));
    }
}

#[cfg(test)]
mod redemption {
    use super::*;

    #[test]
    fn redemption_song_file() {
        let song = crate::song::parse(include_str!("../tabs/Redemption Song - Bob Marley.txt"), 0);
        assert_eq!(song.tempo, Some(110.0));
        let names: Vec<_> = song.chords.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["G", "Em", "C", "C7M9/B", "Am", "D"]);
        assert!(song.chords.iter().all(|c| c.voicing.is_some()));
        let t = Timeline::build(&song, 4.0, 4.0);
        assert_eq!(t.steps.len(), 104);
        // First intro note: low E string, 3rd fret = G2 (MIDI 43), on beat 0.
        let first = &t.onsets[0];
        assert!(first.beat < 0.5);
        assert_eq!((first.notes[0].string, first.notes[0].midi), (0, 43));
        // A G chord strum in the verse plays 6 strings.
        assert!(t.onsets.iter().any(|o| o.notes.len() == 6));
        // Verse strum pattern on a G bar: D D U U D U.
        let bar = t.steps.iter().find(|s| s.start >= 32.0).unwrap();
        let dirs: Vec<bool> = t
            .onsets_between(bar.start, bar.start + bar.beats)
            .iter()
            .map(|o| o.strum.expect("chord should be strummed").down)
            .collect();
        assert_eq!(dirs, [true, true, false, false, true, false]);
    }
}
