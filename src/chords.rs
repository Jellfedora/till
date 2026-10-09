//! Chord parsing, naming (English / French) and guitar voicings.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    Natural,
    Sharp,
    Flat,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub root: u8,
    pub spell: Spell,
    /// Canonical quality used for voicing lookup ("", "m", "7", "maj7", ...).
    pub quality: &'static str,
    /// Suffix as written in the source ("min7", "M7", ...).
    pub suffix: String,
    pub bass: Option<(u8, Spell)>,
}

const SHARP: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLAT: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];
const PREF: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const FR_SHARP: [&str; 12] = [
    "Do", "Do#", "Ré", "Ré#", "Mi", "Fa", "Fa#", "Sol", "Sol#", "La", "La#", "Si",
];
const FR_FLAT: [&str; 12] = [
    "Do", "Réb", "Ré", "Mib", "Mi", "Fa", "Solb", "Sol", "Lab", "La", "Sib", "Si",
];

fn note_name(pc: u8, spell: Spell) -> &'static str {
    match spell {
        Spell::Sharp => SHARP[pc as usize],
        Spell::Flat => FLAT[pc as usize],
        Spell::Natural => PREF[pc as usize],
    }
}

fn note_name_fr(pc: u8, spell: Spell) -> &'static str {
    let en = note_name(pc, spell);
    if en.ends_with('b') {
        FR_FLAT[pc as usize]
    } else {
        FR_SHARP[pc as usize]
    }
}

fn parse_note(s: &str) -> Option<(u8, Spell, &str)> {
    let base = match s.chars().next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = &s[1..];
    if let Some(r) = rest.strip_prefix('#').or_else(|| rest.strip_prefix('♯')) {
        return Some(((base + 1) % 12, Spell::Sharp, r));
    }
    if let Some(r) = rest.strip_prefix('b').or_else(|| rest.strip_prefix('♭')) {
        return Some(((base + 11) % 12, Spell::Flat, r));
    }
    Some((base, Spell::Natural, rest))
}

fn canon_quality(q: &str) -> Option<&'static str> {
    Some(match q {
        "" | "maj" | "M" | "major" => "",
        "m" | "min" | "-" | "mi" => "m",
        "7" => "7",
        "m7" | "min7" | "-7" | "mi7" => "m7",
        "maj7" | "M7" | "Maj7" | "7M" | "Δ" | "Δ7" | "ma7" => "maj7",
        "6" => "6",
        "m6" | "min6" => "m6",
        "9" => "9",
        "m9" | "min9" => "m9",
        "maj9" | "M9" | "7M9" | "maj7(9)" => "maj9",
        "add9" | "add2" | "2" => "add9",
        "madd9" | "m(add9)" | "madd2" => "madd9",
        "sus2" => "sus2",
        "sus4" | "sus" => "sus4",
        "7sus4" | "7sus" => "7sus4",
        "dim" | "°" | "o" => "dim",
        "dim7" | "°7" | "o7" => "dim7",
        "m7b5" | "ø" | "ø7" | "m7-5" => "m7b5",
        "aug" | "+" | "#5" => "aug",
        "5" => "5",
        "11" => "11",
        "13" => "13",
        _ => return None,
    })
}

fn quality_fr(q: &str) -> &'static str {
    match q {
        "" => "majeur",
        "m" => "mineur",
        "7" => "7 (septième)",
        "m7" => "mineur 7",
        "maj7" => "7e majeure",
        "6" => "6 (sixte)",
        "m6" => "mineur 6",
        "9" => "9 (neuvième)",
        "m9" => "mineur 9",
        "maj9" => "9e majeure",
        "add9" => "add9",
        "madd9" => "mineur add9",
        "sus2" => "sus2",
        "sus4" => "sus4",
        "7sus4" => "7 sus4",
        "dim" => "diminué",
        "dim7" => "7e diminuée",
        "m7b5" => "demi-diminué",
        "aug" => "augmenté",
        "5" => "5 (quinte)",
        "11" => "11",
        "13" => "13",
        _ => "",
    }
}

/// Qualities shown in the chord library, in display order.
pub const QUALITIES: &[&str] = &[
    "", "m", "7", "m7", "maj7", "6", "m6", "sus2", "sus4", "7sus4", "add9", "madd9", "9", "m9",
    "maj9", "11", "13", "dim", "dim7", "m7b5", "aug", "5",
];

pub fn root_name(pc: u8) -> &'static str {
    PREF[pc as usize]
}

pub fn root_name_fr(pc: u8) -> &'static str {
    note_name_fr(pc, Spell::Natural)
}

pub fn intervals(q: &str) -> &'static [u8] {
    match q {
        "m" => &[0, 3, 7],
        "7" => &[0, 4, 7, 10],
        "m7" => &[0, 3, 7, 10],
        "maj7" => &[0, 4, 7, 11],
        "6" => &[0, 4, 7, 9],
        "m6" => &[0, 3, 7, 9],
        "sus2" => &[0, 2, 7],
        "sus4" => &[0, 5, 7],
        "7sus4" => &[0, 5, 7, 10],
        "add9" => &[0, 4, 7, 2],
        "madd9" => &[0, 3, 7, 2],
        "9" => &[0, 4, 7, 10, 2],
        "m9" => &[0, 3, 7, 10, 2],
        "maj9" => &[0, 4, 7, 11, 2],
        "11" => &[0, 7, 10, 2, 5],
        "13" => &[0, 4, 7, 10, 9],
        "dim" => &[0, 3, 6],
        "dim7" => &[0, 3, 6, 9],
        "m7b5" => &[0, 3, 6, 10],
        "aug" => &[0, 4, 8],
        "5" => &[0, 7],
        _ => &[0, 4, 7],
    }
}

impl Chord {
    /// Chord tones as (English, French) names, root first.
    pub fn notes(&self) -> Vec<(&'static str, &'static str)> {
        let spell = if self.spell == Spell::Flat { Spell::Flat } else { Spell::Natural };
        intervals(self.quality)
            .iter()
            .map(|i| {
                let pc = (self.root + i) % 12;
                (note_name(pc, spell), note_name_fr(pc, spell))
            })
            .collect()
    }

    pub fn from_parts(root: u8, quality: &'static str) -> Chord {
        Chord { root, spell: Spell::Natural, quality, suffix: quality.to_string(), bass: None }
    }

    /// Lenient parsing for search: English ("am7", "F#") or French ("Sol m7", "Lam", "Ré#").
    pub fn parse_loose(s: &str) -> Option<Chord> {
        let s: String = s.split_whitespace().collect();
        let s = s.replace("mineur", "m").replace("majeur", "").replace("maj7e", "maj7");
        let lower = s.to_lowercase();
        let french = [("sol", 7), ("do", 0), ("ré", 2), ("re", 2), ("mi", 4), ("fa", 5), ("la", 9), ("si", 11)]
            .iter()
            .find(|(n, _)| lower.starts_with(n))
            .map(|(n, pc)| (*pc as u8, n.len()));
        let Some((pc, len)) = french else {
            return Self::parse_english(&s);
        };
        let rest = &s[len..];
        let (pc, spell, rest) = if let Some(r) = rest.strip_prefix('#') {
            ((pc + 1) % 12, Spell::Sharp, r)
        } else if let Some(r) = rest.strip_prefix('b') {
            ((pc + 11) % 12, Spell::Flat, r)
        } else {
            (pc, Spell::Natural, rest)
        };
        if let Some(q) = canon_quality(rest) {
            return Some(Chord { root: pc, spell, quality: q, suffix: rest.to_string(), bass: None });
        }
        Self::parse_english(&s)
    }

    fn parse_english(s: &str) -> Option<Chord> {
        if let Some(c) = Chord::parse(s) {
            return Some(c);
        }
        let mut cap = s.to_string();
        if let Some(f) = cap.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        if let Some(c) = Chord::parse(&cap) {
            return Some(c);
        }
        None
    }

    pub fn parse(s: &str) -> Option<Chord> {
        let (main, bass) = match s.split_once('/') {
            Some((m, b)) => (m, Some(b)),
            None => (s, None),
        };
        let (root, spell, rest) = parse_note(main)?;
        let quality = canon_quality(rest)?;
        let bass = match bass {
            Some(b) => {
                let (pc, sp, r) = parse_note(b)?;
                if !r.is_empty() {
                    return None;
                }
                Some((pc, sp))
            }
            None => None,
        };
        Some(Chord { root, spell, quality, suffix: rest.to_string(), bass })
    }

    pub fn transposed(&self, n: i32) -> Chord {
        if n == 0 {
            return self.clone();
        }
        let t = |pc: u8, sp: Spell| {
            let pc = (pc as i32 + n).rem_euclid(12) as u8;
            // Natural notes may become accidentals: use the usual spelling then.
            (pc, sp)
        };
        let (root, spell) = t(self.root, self.spell);
        Chord {
            root,
            spell,
            quality: self.quality,
            suffix: self.suffix.clone(),
            bass: self.bass.map(|(pc, sp)| t(pc, sp)),
        }
    }

    pub fn name(&self) -> String {
        let mut s = format!("{}{}", note_name(self.root, self.spell), self.suffix);
        if let Some((pc, sp)) = self.bass {
            s.push('/');
            s.push_str(note_name(pc, sp));
        }
        s
    }

    pub fn name_fr(&self) -> String {
        let mut s = format!("{} {}", note_name_fr(self.root, self.spell), quality_fr(self.quality));
        if let Some((pc, sp)) = self.bass {
            s.push_str(", basse ");
            s.push_str(note_name_fr(pc, sp));
        }
        s
    }
}

/// Frets per string, index 0 = low E. `None` = muted, `Some(0)` = open.
#[derive(Clone, Debug, PartialEq)]
pub struct Voicing {
    pub frets: [Option<u8>; 6],
    pub fingers: [u8; 6],
}

impl Voicing {
    fn new(frets: [Option<u8>; 6]) -> Voicing {
        Voicing { frets, fingers: auto_fingers(&frets) }
    }
}

/// Simple fingering heuristic: optional index barre, then remaining notes by fret / string.
fn auto_fingers(frets: &[Option<u8>; 6]) -> [u8; 6] {
    let mut fingers = [0u8; 6];
    let notes: Vec<(usize, u8)> = frets
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.filter(|&f| f > 0).map(|f| (i, f)))
        .collect();
    let Some(&first) = notes.first() else { return fingers };
    let base = notes.iter().map(|n| n.1).min().unwrap();
    let base_strings: Vec<usize> = notes.iter().filter(|n| n.1 == base).map(|n| n.0).collect();
    let (lo, hi) = (base_strings[0], *base_strings.last().unwrap());
    let full_barre = !frets.contains(&Some(0)) && first.1 == base && base_strings.len() >= 2;
    let top_barre = base_strings.len() >= 2 && hi == 5 && hi - lo + 1 == base_strings.len();
    let barre = full_barre || top_barre;

    let mut next = 1;
    if barre {
        for &s in &base_strings {
            fingers[s] = 1;
        }
        next = 2;
    }
    let mut rest: Vec<(usize, u8)> =
        notes.into_iter().filter(|n| !(barre && n.1 == base)).collect();
    rest.sort_by_key(|n| (n.1, n.0));
    for (s, _) in rest {
        fingers[s] = next.min(4);
        next += 1;
    }
    fingers
}

fn parse_frets(s: &str) -> [Option<u8>; 6] {
    let mut out = [None; 6];
    for (i, c) in s.chars().take(6).enumerate() {
        out[i] = c.to_digit(10).map(|d| d as u8);
    }
    out
}

/// Common open-position shapes, preferred over movable shapes.
const OPEN: &[(&str, &str)] = &[
    ("C", "x32010"), ("C7", "x32310"), ("Cmaj7", "x32000"), ("Cadd9", "x32030"),
    ("Csus2", "x30013"), ("Csus4", "x33011"), ("C6", "x32210"),
    ("D", "xx0232"), ("Dm", "xx0231"), ("D7", "xx0212"), ("Dm7", "xx0211"),
    ("Dmaj7", "xx0222"), ("Dsus2", "xx0230"), ("Dsus4", "xx0233"), ("D6", "xx0202"),
    ("Dadd9", "xx0230"), ("D5", "xx023x"),
    ("E", "022100"), ("Em", "022000"), ("E7", "020100"), ("Em7", "020000"),
    ("Emaj7", "021100"), ("Esus4", "022200"), ("E6", "022120"), ("Em6", "022020"),
    ("E5", "022xxx"), ("Emadd9", "024000"), ("E7sus4", "020200"),
    ("F", "133211"), ("Fmaj7", "xx3210"), ("F7", "131211"), ("Fm", "133111"),
    ("G", "320003"), ("G7", "320001"), ("Gmaj7", "320002"), ("G6", "320000"),
    ("Gadd9", "300003"), ("Gsus4", "330013"),
    ("A", "x02220"), ("Am", "x02210"), ("A7", "x02020"), ("Am7", "x02010"),
    ("Amaj7", "x02120"), ("Asus2", "x02200"), ("Asus4", "x02230"), ("A7sus4", "x02030"),
    ("A6", "x02222"), ("Am6", "x04210"), ("Aadd9", "x02420"), ("A5", "x022xx"),
    ("B7", "x21202"), ("Bm7", "x20202"),
    // Slash chords
    ("G/B", "x20003"), ("C/B", "x22010"), ("D/F#", "200232"), ("C/G", "332010"),
    ("G/F#", "200003"), ("Am/G", "3x2210"), ("C/E", "032010"), ("D/A", "x00232"),
    ("Am/E", "002210"), ("Cmaj9/B", "x20010"), ("F/C", "x33211"), ("Em/D", "xx0000"), ("A/E", "002220"),
];

/// Movable shapes as fret offsets from the root fret (E-shape: root on low E, A-shape: on A).
const E_SHAPES: &[(&str, &str)] = &[
    ("", "0 2 2 1 0 0"), ("m", "0 2 2 0 0 0"), ("7", "0 2 0 1 0 0"), ("m7", "0 2 0 0 0 0"),
    ("maj7", "0 x 1 1 0 x"), ("6", "0 2 2 1 2 0"), ("m6", "0 2 2 0 2 0"),
    ("sus4", "0 2 2 2 0 0"), ("7sus4", "0 2 0 2 0 0"), ("9", "0 2 0 1 0 2"),
    ("madd9", "0 2 4 0 0 0"), ("dim", "0 1 2 0 x x"), ("aug", "0 3 2 1 1 0"),
    ("5", "0 2 2 x x x"), ("13", "0 x 0 1 2 x"),
];
const A_SHAPES: &[(&str, &str)] = &[
    ("", "x 0 2 2 2 0"), ("m", "x 0 2 2 1 0"), ("7", "x 0 2 0 2 0"), ("m7", "x 0 2 0 1 0"),
    ("maj7", "x 0 2 1 2 0"), ("6", "x 0 2 2 2 2"), ("m6", "x 0 2 2 1 2"),
    ("sus2", "x 0 2 2 0 0"), ("sus4", "x 0 2 2 3 0"), ("7sus4", "x 0 2 0 3 0"),
    ("add9", "x 0 2 4 2 0"), ("madd9", "x 0 2 4 1 0"), ("9", "x 0 -1 0 0 0"),
    ("m9", "x 0 -2 0 0 0"), ("maj9", "x 0 -1 1 0 x"), ("11", "x 0 0 0 0 0"),
    ("dim", "x 0 1 2 1 x"), ("dim7", "x 0 1 -1 1 x"), ("m7b5", "x 0 1 0 1 x"),
    ("aug", "x 0 -1 -2 -2 x"), ("5", "x 0 2 2 x x"),
];

fn open_table() -> &'static [(Chord, [Option<u8>; 6])] {
    static T: OnceLock<Vec<(Chord, [Option<u8>; 6])>> = OnceLock::new();
    T.get_or_init(|| {
        OPEN.iter()
            .map(|(n, f)| (Chord::parse(n).expect("bad chord in table"), parse_frets(f)))
            .collect()
    })
}

fn same_chord(a: &Chord, b: &Chord) -> bool {
    a.root == b.root
        && a.quality == b.quality
        && a.bass.map(|b| b.0) == b.bass.map(|b| b.0)
}

fn movable(root: u8, quality: &str) -> Option<[Option<u8>; 6]> {
    let mut best: Option<([Option<u8>; 6], u8)> = None;
    for (string_pc, shapes) in [(4u8, E_SHAPES), (9u8, A_SHAPES)] {
        let Some((_, tpl)) = shapes.iter().find(|(q, _)| *q == quality) else { continue };
        let offs: Vec<Option<i32>> = tpl.split_whitespace().map(|t| t.parse().ok()).collect();
        let r0 = (root as i32 - string_pc as i32).rem_euclid(12);
        for r in [r0, r0 + 12] {
            let mut frets = [None; 6];
            let mut ok = true;
            for (i, o) in offs.iter().enumerate() {
                if let Some(o) = o {
                    let f = r + o;
                    if f < 0 {
                        ok = false;
                    }
                    frets[i] = Some(f.max(0) as u8);
                }
            }
            if !ok {
                continue;
            }
            let top = frets.iter().flatten().copied().max().unwrap_or(0);
            if best.as_ref().is_none_or(|b| top < b.1) {
                best = Some((frets, top));
            }
        }
    }
    best.map(|b| b.0)
}

/// Put `bass` as the lowest note of `frets`, on the low E or A string, if it fits the hand.
fn with_bass(frets: [Option<u8>; 6], bass: u8) -> [Option<u8>; 6] {
    for (s, string_pc) in [(0usize, 4u8), (1, 9)] {
        let f0 = (bass as i32 - string_pc as i32).rem_euclid(12) as u8;
        for f in [f0, f0 + 12] {
            let mut out = frets;
            for o in out.iter_mut().take(s) {
                *o = None;
            }
            out[s] = Some(f);
            let fretted: Vec<u8> = out.iter().flatten().copied().filter(|&f| f > 0).collect();
            let span = fretted.iter().max().unwrap_or(&0) - fretted.iter().min().unwrap_or(&0);
            if span <= 3 && fretted.len() <= 4 {
                return out;
            }
        }
    }
    frets
}

pub fn voicing(chord: &Chord) -> Option<Voicing> {
    let table = open_table();
    if let Some((_, f)) = table.iter().find(|(c, _)| same_chord(c, chord)) {
        return Some(Voicing::new(*f));
    }
    let plain = Chord { bass: None, ..chord.clone() };
    let base = table
        .iter()
        .find(|(c, _)| same_chord(c, &plain))
        .map(|(_, f)| *f)
        .or_else(|| movable(chord.root, chord.quality))?;
    let frets = match chord.bass {
        Some((pc, _)) if pc != chord.root => with_bass(base, pc),
        _ => base,
    };
    Some(Voicing::new(frets))
}

/// How hard a voicing is for a beginner: 0 = open two-finger shape, 10+ = hard barre.
pub fn difficulty(v: &Voicing) -> f32 {
    let fretted: Vec<u8> = v.frets.iter().flatten().copied().filter(|&f| f > 0).collect();
    let mut d = fretted.len() as f32 * 0.6;
    let mut fingers: Vec<u8> = v.fingers.iter().copied().filter(|&f| f > 0).collect();
    let pressed = fingers.len();
    fingers.sort_unstable();
    fingers.dedup();
    if fingers.len() < pressed {
        // One finger on several strings: a barre.
        d += 4.0;
    }
    if let (Some(&lo), Some(&hi)) = (fretted.iter().min(), fretted.iter().max()) {
        d += (hi - lo).saturating_sub(2) as f32 * 1.2;
        if lo >= 5 {
            d += 1.0;
        }
    }
    // Muted strings in the middle of the shape are hard to avoid when strumming.
    let first = v.frets.iter().position(|f| f.is_some()).unwrap_or(0);
    d += v.frets[first..].iter().filter(|f| f.is_none()).count() as f32 * 0.8;
    d
}

impl Chord {
    /// Difficulty of the usual voicing (see [`difficulty`]).
    pub fn difficulty(&self) -> f32 {
        voicing(self).map_or(12.0, |v| difficulty(&v))
    }

    /// A beginner-friendly chord that keeps the same function: extensions and slash basses
    /// are dropped, and hard barre shapes swap for a close open shape (F -> Fmaj7, Bm -> Bm7).
    pub fn easy(&self) -> Chord {
        let base = match self.quality {
            "maj9" | "maj7" | "6" | "add9" | "" | "aug" => "",
            "m9" | "m7" | "m6" | "madd9" | "m" => "m",
            "9" | "11" | "13" | "7" => "7",
            "7sus4" | "sus4" => "sus4",
            q => q,
        };
        let simple = Chord { root: self.root, spell: self.spell, quality: base, suffix: base.to_string(), bass: None };
        let alternatives: &[&'static str] = match base {
            "" => &["", "maj7", "5", "sus2"],
            "m" => &["m", "m7", "5"],
            "7" => &["7", ""],
            "sus4" => &["sus4", "sus2", ""],
            "dim" | "dim7" | "m7b5" => &["dim", "m"],
            _ => &[],
        };
        let mut best = (simple.difficulty(), simple.clone());
        for (k, &q) in alternatives.iter().enumerate() {
            let c = Chord { root: self.root, spell: self.spell, quality: q, suffix: q.to_string(), bass: None };
            // Changing the chord colour must buy a real simplification.
            let d = c.difficulty() + k as f32 * 0.8;
            if d + 1.5 < best.0 {
                best = (d, c);
            }
        }
        best.1
    }
}

/// Capo position (0..=7) that makes these chords easiest, with the total difficulty.
pub fn best_capo(chords: &[Chord]) -> (u8, f32) {
    (0..=7u8)
        .map(|capo| (capo, chords.iter().map(|c| c.transposed(-(capo as i32)).difficulty()).sum::<f32>()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> (String, [u8; 6]) {
        let vo = voicing(&Chord::parse(name).unwrap()).unwrap();
        let s = vo
            .frets
            .iter()
            .map(|f| f.map_or("x".to_string(), |f| f.to_string()))
            .collect();
        (s, vo.fingers)
    }

    #[test]
    fn parses_and_names() {
        let c = Chord::parse("G/B").unwrap();
        assert_eq!(c.name(), "G/B");
        assert_eq!(c.name_fr(), "Sol majeur, basse Si");
        assert_eq!(Chord::parse("Em7").unwrap().name_fr(), "Mi mineur 7");
        assert_eq!(Chord::parse("Bbmaj7").unwrap().name_fr(), "Sib 7e majeure");
        assert!(Chord::parse("Hello").is_none());
        assert!(Chord::parse("Am").is_some());
        assert!(Chord::parse("A").is_some());
    }

    #[test]
    fn loose_search() {
        assert_eq!(Chord::parse_loose("am7").unwrap().name(), "Am7");
        assert_eq!(Chord::parse_loose("Sol m7").unwrap().name(), "Gm7");
        assert_eq!(Chord::parse_loose("Lam").unwrap().name(), "Am");
        assert_eq!(Chord::parse_loose("ré#").unwrap().name(), "D#");
        assert_eq!(Chord::parse_loose("Si mineur").unwrap().name(), "Bm");
        for q in QUALITIES {
            for r in 0..12 {
                assert!(voicing(&Chord::from_parts(r, q)).is_some(), "{r} {q}");
            }
        }
    }

    #[test]
    fn beginner_helpers() {
        let d = |n: &str| Chord::parse(n).unwrap().difficulty();
        assert!(d("Em") < d("C") && d("C") < d("F") && d("G") < d("Bm"), "{} {} {} {}", d("Em"), d("C"), d("F"), d("Bm"));
        let easy = |n: &str| Chord::parse(n).unwrap().easy().name();
        assert_eq!(easy("F"), "Fmaj7");
        assert_eq!(easy("Bm"), "Bm7");
        assert_eq!(easy("G"), "G");
        assert_eq!(easy("Cmaj9/B"), "C");
        // Bb Eb F Gm is much easier with capo 3 (G C D Em).
        let song: Vec<Chord> = ["Bb", "Eb", "F", "Gm"].iter().map(|n| Chord::parse(n).unwrap()).collect();
        assert_eq!(best_capo(&song).0, 3);
    }

    #[test]
    fn transposes() {
        let c = Chord::parse("Am/G").unwrap().transposed(2);
        assert_eq!(c.name(), "Bm/A");
        assert_eq!(Chord::parse("E").unwrap().transposed(-1).name(), "Eb");
    }

    #[test]
    fn voicings_and_fingers() {
        assert_eq!(v("G"), ("320003".into(), [2, 1, 0, 0, 0, 3]));
        assert_eq!(v("C"), ("x32010".into(), [0, 3, 2, 0, 1, 0]));
        assert_eq!(v("D"), ("xx0232".into(), [0, 0, 0, 1, 3, 2]));
        assert_eq!(v("F"), ("133211".into(), [1, 3, 4, 2, 1, 1]));
        assert_eq!(v("Bm").0, "x24432");
        assert_eq!(v("Cm").0, "x35543");
        assert_eq!(v("F#m7").0, "242222");
        assert_eq!(v("C/B").0, "x22010");
        assert_eq!(v("A/C#").0, "x42220");
    }
}
