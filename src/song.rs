//! Turns raw tab text (Ultimate Guitar style, ChordPro, plain chords-over-lyrics) into blocks.

use crate::chords::{Chord, Voicing, voicing};

pub struct ChordTok {
    pub col: usize,
    pub text: String,
    /// Index into `Song::chords`, `None` for fillers like `|` or `x2`.
    pub idx: Option<usize>,
}

pub struct TabLine {
    /// Characters before the body in the source line (label, spaces), for chord alignment.
    pub offset: usize,
    pub label: String,
    pub body: Vec<char>,
    pub note: String,
}

pub enum Block {
    Title(String),
    Section(String),
    Chords(Vec<ChordTok>),
    Text(String),
    Blank,
    Tab(Vec<TabLine>),
}

pub struct ChordInfo {
    pub chord: Chord,
    pub name: String,
    pub voicing: Option<Voicing>,
}

#[derive(Default)]
pub struct Song {
    pub blocks: Vec<Block>,
    pub chords: Vec<ChordInfo>,
    /// From `{tempo: 110}` / `{time: 3/4}` directives.
    pub tempo: Option<f32>,
    pub beats_per_bar: Option<f32>,
    /// Capo fret: chord shapes sound this many semitones higher.
    pub capo: u8,
    /// Synchronized YouTube video (`{video: …}`), and one without the guitar (`{video_backing: …}`).
    pub video: Option<crate::video::Sync>,
    pub backing: Option<crate::video::Sync>,
    /// Seconds added to the video time, set when the player recalibrates the sync.
    pub video_offset: f32,
}

fn split_tab_line(line: &str) -> TabLine {
    let chars: Vec<char> = line.chars().collect();
    let start = chars.iter().position(|&c| c == '|' || c == '-').unwrap_or(chars.len());
    let end = chars[start..].iter().position(|c| c.is_whitespace()).map_or(chars.len(), |p| p + start);
    TabLine {
        offset: start,
        label: chars[..start].iter().collect::<String>().trim().to_string(),
        body: chars[start..end].to_vec(),
        note: chars[end..].iter().collect::<String>().trim().to_string(),
    }
}

fn is_tab_line(line: &str) -> bool {
    let t = split_tab_line(line);
    let label_ok = t.label.chars().count() <= 3 && t.label.chars().all(|c| c.is_alphabetic() || c == '#');
    let dashes = t.body.iter().filter(|&&c| c == '-').count();
    label_ok && t.body.len() >= 4 && dashes >= 3 && dashes * 4 >= t.body.len()
}

fn words(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<(usize, usize)> = None; // (char col, byte idx)
    for (col, (bi, c)) in line.char_indices().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some((col, bi)),
            (true, Some((sc, sb))) => {
                out.push((sc, &line[sb..bi]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some((sc, sb)) = start {
        out.push((sc, &line[sb..]));
    }
    out
}

/// Splits "(G)" / "G*" into ("(", chord, ")").
fn chord_word(w: &str) -> Option<(&str, Chord, &str)> {
    let core_start = w.find(|c: char| c != '(').unwrap_or(w.len());
    let core_end = w
        .char_indices()
        .rfind(|&(_, c)| !matches!(c, ')' | '*' | ',' | '.'))
        .map_or(core_start, |(i, c)| i + c.len_utf8());
    if core_start >= core_end {
        return None;
    }
    let chord = Chord::parse(&w[core_start..core_end])?;
    Some((&w[..core_start], chord, &w[core_end..]))
}

fn is_filler(w: &str) -> bool {
    let t = w.trim_matches(|c| c == '(' || c == ')');
    matches!(t, "|" | "||" | "/" | "//" | "-" | "%" | "N.C." | "NC" | "N.C" | "..." | ":" | "|:" | ":|")
        || (t.len() >= 2
            && (t.starts_with(['x', 'X']) || t.ends_with(['x', 'X']))
            && t.trim_matches(['x', 'X']).chars().all(|c| c.is_ascii_digit()))
}

struct Builder {
    song: Song,
    transpose: i32,
    easy: bool,
}

impl Builder {
    fn chord_index(&mut self, chord: &Chord) -> (usize, String) {
        let c = chord.transposed(self.transpose);
        let c = if self.easy { c.easy() } else { c };
        let name = c.name();
        if let Some(i) = self.song.chords.iter().position(|ci| ci.name == name) {
            return (i, name);
        }
        let voicing = voicing(&c);
        self.song.chords.push(ChordInfo { chord: c, name: name.clone(), voicing });
        (self.song.chords.len() - 1, name)
    }

    /// Builds a chord line from (column, word) pairs, reflowing columns after transposition.
    fn chord_line(&mut self, items: Vec<(usize, String)>) -> Block {
        let mut toks = Vec::new();
        let mut min_col = 0;
        for (col, w) in items {
            let (text, idx) = match chord_word(&w) {
                Some((pre, chord, post)) => {
                    let (i, name) = self.chord_index(&chord);
                    (format!("{pre}{name}{post}"), Some(i))
                }
                None => (w, None),
            };
            let col = col.max(min_col);
            min_col = col + text.chars().count() + 1;
            toks.push(ChordTok { col, text, idx });
        }
        Block::Chords(toks)
    }

    /// ChordPro inline chords: "[G]Hello [C]world" -> chord line + lyric line.
    fn inline_chords(&mut self, line: &str) -> Option<(Block, Block)> {
        let mut lyric = String::new();
        let mut chords = Vec::new();
        let mut rest = line;
        while let Some(open) = rest.find('[') {
            lyric.push_str(&rest[..open]);
            let close = rest[open..].find(']')? + open;
            let inner = &rest[open + 1..close];
            Chord::parse(inner)?;
            chords.push((lyric.chars().count(), inner.to_string()));
            rest = &rest[close + 1..];
        }
        lyric.push_str(rest);
        if chords.is_empty() {
            return None;
        }
        Some((self.chord_line(chords), Block::Text(lyric)))
    }

    fn directive(&mut self, inner: &str) {
        let (key, val) = inner.split_once(':').unwrap_or((inner, ""));
        let val = val.trim().to_string();
        let block = match key.trim().to_lowercase().as_str() {
            "title" | "t" => Block::Title(val),
            "subtitle" | "st" | "artist" => Block::Text(val),
            "comment" | "c" | "ci" | "comment_italic" => Block::Section(val),
            "tempo" | "bpm" => {
                self.song.tempo = val.parse().ok();
                return;
            }
            "video" => {
                self.song.video = crate::video::Sync::parse(&val);
                return;
            }
            "video_offset" => {
                self.song.video_offset = val.parse().unwrap_or(0.0);
                return;
            }
            "video_backing" => {
                self.song.backing = crate::video::Sync::parse(&val);
                return;
            }
            "time" => {
                self.song.beats_per_bar = val.split('/').next().and_then(|n| n.trim().parse().ok());
                return;
            }
            "start_of_chorus" | "soc" => Block::Section("Chorus".into()),
            "start_of_verse" | "sov" => Block::Section("Verse".into()),
            "start_of_bridge" | "sob" => Block::Section("Bridge".into()),
            _ => return,
        };
        self.song.blocks.push(block);
    }

    fn flush_tab(&mut self, tab: &mut Vec<TabLine>) {
        if tab.is_empty() {
            return;
        }
        if tab.len() == 1 && tab[0].label.is_empty() {
            let t = tab.pop().unwrap();
            self.song.blocks.push(Block::Text(t.body.iter().collect::<String>() + " " + &t.note));
        } else {
            self.song.blocks.push(Block::Tab(std::mem::take(tab)));
        }
    }

    fn line(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            self.song.blocks.push(Block::Blank);
            return;
        }
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            self.directive(&trimmed[1..trimmed.len() - 1]);
            return;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.matches('[').count() == 1 {
            let inner = &trimmed[1..trimmed.len() - 1];
            if Chord::parse(inner).is_none() {
                self.song.blocks.push(Block::Section(inner.to_string()));
                return;
            }
        }
        if line.contains('[') {
            if let Some((c, l)) = self.inline_chords(line) {
                self.song.blocks.push(c);
                if !l_is_blank(&l) {
                    self.song.blocks.push(l);
                }
                return;
            }
        }
        let ws = words(line);
        let all_chords = ws.iter().all(|(_, w)| chord_word(w).is_some() || is_filler(w));
        if all_chords && ws.iter().any(|(_, w)| chord_word(w).is_some()) {
            let items = ws.into_iter().map(|(c, w)| (c, w.to_string())).collect();
            let block = self.chord_line(items);
            self.song.blocks.push(block);
            return;
        }
        self.song.blocks.push(Block::Text(line.trim_end().to_string()));
    }
}

fn l_is_blank(b: &Block) -> bool {
    matches!(b, Block::Text(t) if t.trim().is_empty())
}

pub fn parse(src: &str, transpose: i32) -> Song {
    parse_with(src, transpose, false)
}

/// Like [`parse`]; `easy` swaps hard chords for beginner-friendly ones.
pub fn parse_with(src: &str, transpose: i32, easy: bool) -> Song {
    let clean = src
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
        .replace("[ch]", "")
        .replace("[/ch]", "")
        .replace("[tab]", "")
        .replace("[/tab]", "");
    let mut b = Builder { song: Song::default(), transpose, easy };
    let mut tab = Vec::new();
    for line in clean.lines() {
        if is_tab_line(line) {
            tab.push(split_tab_line(line));
            continue;
        }
        b.flush_tab(&mut tab);
        b.line(line);
    }
    b.flush_tab(&mut tab);
    // Trim leading/trailing blanks and collapse runs of blanks.
    let mut blocks: Vec<Block> = Vec::new();
    for block in b.song.blocks {
        if matches!(block, Block::Blank) && matches!(blocks.last(), None | Some(Block::Blank)) {
            continue;
        }
        blocks.push(block);
    }
    while matches!(blocks.last(), Some(Block::Blank)) {
        blocks.pop();
    }
    b.song.blocks = blocks;
    b.song
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[Intro]\n\ne|-----------|\nB|-----------|\nG|-----------|\nD|------1/2-0|\nA|--0-2------|\nE|-3---------|  / = Slide up\n\n[Verse 1]\n    G          Em   Em7\nOld pirates, yes, they rob I\n";

    #[test]
    fn parses_sample() {
        let s = parse(SAMPLE, 0);
        assert!(matches!(s.blocks[0], Block::Section(ref n) if n == "Intro"));
        let Block::Tab(t) = &s.blocks[2] else { panic!("expected tab") };
        assert_eq!(t.len(), 6);
        assert_eq!(t[5].note, "/ = Slide up");
        let names: Vec<_> = s.chords.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["G", "Em", "Em7"]);
        assert!(matches!(s.blocks.last(), Some(Block::Text(_))));
    }

    #[test]
    fn lyrics_are_not_chords() {
        let s = parse("A man and a plan\nAm I right", 0);
        assert!(s.chords.is_empty());
    }

    #[test]
    fn chordpro_inline() {
        let s = parse("{title: Test}\n[G]Hello [C/B]world", 2);
        let names: Vec<_> = s.chords.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["A", "D/C#"]);
        let Block::Chords(t) = &s.blocks[1] else { panic!() };
        assert_eq!(t[1].col, 6);
    }

    #[test]
    fn reflows_after_transpose() {
        let s = parse("G C", 1);
        let Block::Chords(t) = &s.blocks[0] else { panic!() };
        assert_eq!(t[0].text, "Ab");
        assert_eq!(t[1].col, 3);
    }
}
