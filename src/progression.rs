//! Chord progressions built on the "Accords" page, turned into strumming tabs.

use crate::chords::{Chord, voicing};

/// Chords per line in the generated tab.
const PER_LINE: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Progression {
    pub name: String,
    /// Chord names, in playing order.
    pub chords: Vec<String>,
    /// Down–up strokes (↓↑) per chord: one per beat.
    pub pairs: u8,
    /// Times the whole progression is repeated in the tab.
    pub loops: u8,
    pub bpm: u16,
}

impl Progression {
    pub fn new(name: &str, chords: &[&str]) -> Progression {
        Progression { name: name.to_string(), chords: chords.iter().map(|c| c.to_string()).collect(), pairs: 1, loops: 4, bpm: 70 }
    }

    /// The built-in example, offered until the user saves their own.
    pub fn defaults() -> Vec<Progression> {
        vec![Progression::new("Sol Ré Mim Do", &["G", "D", "Em", "C"])]
    }

    /// One progression per line: `name \t pairs \t loops \t bpm \t G D Em C`.
    pub fn save_all(list: &[Progression]) -> String {
        list.iter()
            .map(|p| format!("{}\t{}\t{}\t{}\t{}", p.name.replace(['\t', '\n'], " "), p.pairs, p.loops, p.bpm, p.chords.join(" ")))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn load_all(text: &str) -> Vec<Progression> {
        text.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let [name, pairs, loops, bpm, chords] = f[..] else { return None };
                Some(Progression {
                    name: name.to_string(),
                    chords: chords.split_whitespace().filter(|c| Chord::parse(c).is_some()).map(String::from).collect(),
                    pairs: pairs.parse().unwrap_or(1).clamp(1, 8),
                    loops: loops.parse().unwrap_or(4).clamp(1, 32),
                    bpm: bpm.parse().unwrap_or(70).clamp(30, 240),
                })
            })
            .collect()
    }

    /// A playable tab: one measure per chord, each beat strummed down then up.
    pub fn to_tab(&self) -> String {
        let pattern = vec!["↓ ↑"; self.pairs as usize].join("  ");
        let mut out = format!(
            "{{title: {}}}\n{{tempo: {}}}\n{{time: {}/4}}\n\n[Rythme : {pattern} par accord]\n\n",
            self.name, self.bpm, self.pairs
        );
        let chords: Vec<Chord> = self.chords.iter().filter_map(|c| Chord::parse(c)).collect();
        if chords.is_empty() {
            return out;
        }
        let all: Vec<&Chord> = (0..self.loops).flat_map(|_| chords.iter()).collect();
        for line in all.chunks(PER_LINE) {
            let mut names = String::from("  ");
            let mut strings: [String; 6] = std::array::from_fn(|_| String::from("|"));
            for chord in line {
                let frets = voicing(chord).map_or([None; 6], |v| v.frets);
                let name = chord.name();
                let digits = frets.iter().flatten().map(|f| f.to_string().len()).max().unwrap_or(1);
                // Down stroke at the start of each beat, up stroke halfway through it.
                let half = (digits + 1).max((name.chars().count() + 1).div_ceil(2 * self.pairs as usize));
                for (row, s) in strings.iter_mut().enumerate() {
                    let cell = match frets[5 - row] {
                        Some(f) => format!("{f:-<half$}"),
                        None => "-".repeat(half),
                    };
                    for _ in 0..self.pairs * 2 {
                        s.push_str(&cell);
                    }
                    s.push('|');
                }
                let width = half * 2 * self.pairs as usize + 1;
                names.push_str(&format!("{name:<width$}"));
            }
            out.push_str(names.trim_end());
            out.push('\n');
            for (label, s) in ["e", "B", "G", "D", "A", "E"].iter().zip(&strings) {
                out.push_str(&format!("{label}{s}\n"));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Timeline;

    #[test]
    fn round_trips() {
        let mut list = Progression::defaults();
        list.push(Progression { name: "Blues".into(), chords: vec!["A7".into(), "D7".into()], pairs: 2, loops: 3, bpm: 90 });
        assert_eq!(Progression::load_all(&Progression::save_all(&list)), list);
    }

    #[test]
    fn tab_strums_down_then_up() {
        let p = Progression { loops: 1, ..Progression::new("Test", &["G", "D", "Em", "C"]) };
        let song = crate::song::parse(&p.to_tab(), 0);
        let names: Vec<_> = song.chords.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["G", "D", "Em", "C"]);
        assert_eq!(song.beats_per_bar, Some(1.0));
        assert_eq!(song.tempo, Some(70.0));
        let t = Timeline::build(&song, 1.0, 4.0);
        assert_eq!(t.steps.len(), 4);
        let dirs: Vec<(f32, bool)> = t.onsets.iter().map(|o| (o.beat, o.strum.expect("strum").down)).collect();
        assert_eq!(dirs, [(0.0, true), (0.5, false), (1.0, true), (1.5, false), (2.0, true), (2.5, false), (3.0, true), (3.5, false)]);
        // G down stroke: all six strings.
        assert_eq!(t.onsets[0].notes.len(), 6);
    }

    #[test]
    fn long_names_and_pairs() {
        let p = Progression { loops: 2, pairs: 2, ..Progression::new("X", &["Cmaj7", "D/F#", "Bm"]) };
        let song = crate::song::parse(&p.to_tab(), 0);
        let t = Timeline::build(&song, 2.0, 4.0);
        assert_eq!(t.steps.len(), 6);
        let downs = t.onsets.iter().filter(|o| o.strum.is_some_and(|s| s.down)).count();
        assert_eq!(downs, 12);
    }
}
