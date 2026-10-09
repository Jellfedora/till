//! The beginner path: short lessons, each with something to do right away.

use std::collections::BTreeSet;

use eframe::egui::{self, CornerRadius, RichText, Sense, vec2};

use crate::chords::{Chord, voicing};
use crate::diagram;
use crate::synth::{Play, Synth};
use crate::theme::ACCENT;

#[derive(Clone, Copy)]
pub enum Action {
    Tuner,
    Shelf,
    Song(&'static str),
    Changes(&'static [&'static str]),
    Strum(usize),
    Check(&'static str),
    Listen(&'static str),
}

pub struct Lesson {
    pub title: &'static str,
    pub body: &'static [&'static str],
    pub chords: &'static [&'static str],
    pub actions: &'static [(Action, &'static str)],
}

const EX_TAB: &str = "{title: Exercice — Lire une tab}
{tempo: 60}
{time: 4/4}
Joue uniquement la corde la plus fine (Mi aigu). Un chiffre = une case ; 0 = corde à vide.
Utilise l'index pour la case 1 et l'annulaire pour la case 3.

e|0---0---1---1---|3---3---1---0---|0---1---3---1---|0-------0-------|
B|----------------|----------------|----------------|----------------|
G|----------------|----------------|----------------|----------------|
D|----------------|----------------|----------------|----------------|
A|----------------|----------------|----------------|----------------|
E|----------------|----------------|----------------|----------------|

e|0---1---3---1---|0---1---3-------|3---1---0---1---|0---------------|
B|----------------|----------------|----------------|----------------|
G|----------------|----------------|----------------|----------------|
D|----------------|----------------|----------------|----------------|
A|----------------|----------------|----------------|----------------|
E|----------------|----------------|----------------|----------------|
";

const EX_STRINGS: &str = "{title: Exercice — Changer de corde}
{tempo: 60}
{time: 4/4}
Les deux cordes les plus fines : Mi aigu (e) et Si (B). Regarde bien sur quelle ligne est chaque chiffre.

e|0-------1-------|--------0-------|0---1---3---1---|0---------------|
B|----0-------1---|1---3-------3---|----------------|----1---0-------|
G|----------------|----------------|----------------|----------------|
D|----------------|----------------|----------------|----------------|
A|----------------|----------------|----------------|----------------|
E|----------------|----------------|----------------|----------------|

e|----------------|0-------0-------|3---1---0-------|----------------|
B|0---1---3---1---|----3-------1---|------------3---|1-------0-------|
G|----------------|----------------|----------------|----------------|
D|----------------|----------------|----------------|----------------|
A|----------------|----------------|----------------|----------------|
E|----------------|----------------|----------------|----------------|
";

const EX_EM_AM: &str = "{title: Exercice — Em et Am}
{tempo: 60}
Chaque accord dure 4 temps : gratte une fois par temps, vers le bas.
Astuce : pour passer de Em à Am, garde la forme des deux doigts et descends-la d'une corde.

Em          Am          Em          Am
Em          Am          Em          Em
";

const EX_E_A_D: &str = "{title: Exercice — E, A et D}
{tempo: 60}
Chaque accord dure 4 temps. Pour D, ne gratte que les 4 cordes les plus fines.

E           A           E           A
A           D           A           D
E           A           D           E
";

const EX_FOUR: &str = "{title: Exercice — Les 4 accords magiques}
{tempo: 70}
G, D, Em et C : cet enchaînement fait tourner des centaines de chansons pop.
Commence avec 4 temps par accord, puis essaie le rythme pop/folk.

G           D           Em          C
G           D           Em          C
G           D           Em          C
G           D           C           G
";

pub const LESSONS: &[Lesson] = &[
    Lesson {
        title: "Bien démarrer",
        body: &[
            "Assieds-toi droit, la guitare posée sur ta jambe droite (si tu es droitier), le manche légèrement relevé. Ta main gauche presse les cordes, ta main droite gratte.",
            "Les cordes sont numérotées de la plus fine (corde 1, Mi aigu) à la plus grosse (corde 6, Mi grave). De la plus grave à la plus aiguë : Mi La Ré Sol Si Mi (E A D G B E).",
            "Les doigts de la main gauche sont numérotés : 1 = index, 2 = majeur, 3 = annulaire, 4 = auriculaire. Les diagrammes d'accords utilisent ces numéros.",
            "Première chose avant chaque séance : accorder la guitare. Une guitare désaccordée sonne faux même si tu joues parfaitement.",
        ],
        chords: &[],
        actions: &[(Action::Tuner, "Accorder ma guitare")],
    },
    Lesson {
        title: "Lire une tablature",
        body: &[
            "Une tab a 6 lignes : une par corde. La ligne du haut est la corde la plus fine (Mi aigu), celle du bas la plus grosse (Mi grave) — comme si tu regardais ta guitare posée à plat.",
            "Le chiffre indique la case à presser : 0 = corde à vide, 3 = troisième case. On lit de gauche à droite. Des chiffres alignés verticalement se jouent ensemble.",
            "Appuie juste derrière la barrette métallique (la frette), avec le bout du doigt : c'est là que la note sonne le mieux avec le moins d'effort.",
            "Lance l'exercice, active « Son » pour l'entendre, puis baisse la vitesse et joue avec.",
        ],
        chords: &[],
        actions: &[(Action::Song(EX_TAB), "Exercice : une corde"), (Action::Song(EX_STRINGS), "Exercice : deux cordes")],
    },
    Lesson {
        title: "Premier accord : Mi mineur (Em)",
        body: &[
            "Em ne demande que deux doigts : le majeur (2) case 2 de la corde de La, l'annulaire (3) case 2 de la corde de Ré. Toutes les cordes sonnent.",
            "Arrondis bien les doigts pour ne pas toucher les cordes voisines. Gratte lentement corde par corde : chacune doit sonner clairement.",
            "Ça fait mal au bout des doigts les premiers jours : c'est normal, la corne se forme en 2 à 3 semaines. Joue un peu chaque jour plutôt que longtemps.",
        ],
        chords: &["Em"],
        actions: &[(Action::Listen("Em"), "Écouter Em"), (Action::Check("Em"), "Vérifier mon Em au micro")],
    },
    Lesson {
        title: "La mineur (Am)",
        body: &[
            "Am a la même forme que E (que tu verras plus tard) mais décalée d'une corde vers le bas : index case 1 de la corde de Si, majeur case 2 de Ré, annulaire case 2 de Sol.",
            "Ne gratte pas la corde de Mi grave (marquée ×) : commence à la corde de La.",
            "Vérifie au micro : Till te dit quelle corde ne sonne pas, souvent parce qu'un doigt la touche par erreur.",
        ],
        chords: &["Am"],
        actions: &[(Action::Listen("Am"), "Écouter Am"), (Action::Check("Am"), "Vérifier mon Am au micro")],
    },
    Lesson {
        title: "Enchaîner deux accords",
        body: &[
            "Le vrai défi du débutant, ce n'est pas l'accord, c'est le changement. La méthode qui marche : le défi d'une minute. Change entre deux accords le plus de fois possible en 60 secondes, et note ton score.",
            "Fais-le chaque jour : ton score monte vite. À 30 changements par minute, l'enchaînement est acquis.",
            "Astuce : place tous les doigts en même temps, comme une seule forme, au lieu de doigt par doigt.",
        ],
        chords: &["Em", "Am"],
        actions: &[(Action::Changes(&["Em", "Am"]), "Défi Em ↔ Am"), (Action::Song(EX_EM_AM), "Exercice en rythme")],
    },
    Lesson {
        title: "Gratter en rythme",
        body: &[
            "Le rythme compte plus que les notes : un accord un peu sale joué en rythme sonne mieux qu'un accord parfait qui arrive en retard.",
            "Commence par 4 coups vers le bas par mesure, un sur chaque temps, en comptant « 1, 2, 3, 4 ». Le poignet reste souple, le mouvement vient de l'avant-bras.",
            "Ensuite les croches : bas sur le temps, haut entre les temps (« 1 et 2 et… »). Ta main fait des allers-retours réguliers, même quand elle ne touche pas les cordes.",
        ],
        chords: &[],
        actions: &[(Action::Strum(0), "Rythme : 4 temps"), (Action::Strum(1), "Rythme : croches")],
    },
    Lesson {
        title: "Mi majeur (E) et La majeur (A)",
        body: &[
            "E : la forme de Am, une corde plus haut. Index case 1 de Sol, majeur case 2 de La, annulaire case 2 de Ré.",
            "A : trois doigts côte à côte sur la case 2, cordes Ré, Sol et Si. C'est serré : tu peux aussi utiliser 1-2-3 en les décalant légèrement. Ne gratte pas le Mi grave.",
        ],
        chords: &["E", "A"],
        actions: &[
            (Action::Listen("E"), "Écouter E"),
            (Action::Listen("A"), "Écouter A"),
            (Action::Check("A"), "Vérifier A au micro"),
            (Action::Changes(&["E", "A"]), "Défi E ↔ A"),
        ],
    },
    Lesson {
        title: "Ré majeur (D)",
        body: &[
            "D forme un petit triangle : index case 2 de Sol, majeur case 2 de Mi aigu, annulaire case 3 de Si.",
            "Ne gratte que les 4 cordes les plus fines : vise la corde de Ré avec le médiator.",
            "L'exercice E, A, D te fait travailler les trois accords ensemble.",
        ],
        chords: &["D"],
        actions: &[
            (Action::Listen("D"), "Écouter D"),
            (Action::Check("D"), "Vérifier D au micro"),
            (Action::Changes(&["D", "A"]), "Défi D ↔ A"),
            (Action::Song(EX_E_A_D), "Exercice E, A, D"),
        ],
    },
    Lesson {
        title: "Do (C) et Sol (G)",
        body: &[
            "C : un escalier. Annulaire case 3 de La, majeur case 2 de Ré, index case 1 de Si. Le Mi aigu à vide doit sonner : c'est le piège classique, l'index le touche souvent.",
            "G : majeur case 3 de Mi grave, index case 2 de La, annulaire case 3 de Mi aigu (ou auriculaire, ce qui facilite le passage vers C).",
        ],
        chords: &["C", "G"],
        actions: &[
            (Action::Listen("C"), "Écouter C"),
            (Action::Check("C"), "Vérifier C au micro"),
            (Action::Check("G"), "Vérifier G au micro"),
            (Action::Changes(&["C", "G"]), "Défi C ↔ G"),
        ],
    },
    Lesson {
        title: "Le rythme pop/folk",
        body: &[
            "Le rythme le plus utilisé : bas, bas-haut, haut-bas-haut (↓ ↓↑ ↑↓↑). Le « trou » sur le temps 3 donne tout le groove : la main descend dans le vide sans toucher les cordes.",
            "Commence très lentement (60 BPM) avec un seul accord, puis ajoute les changements.",
        ],
        chords: &[],
        actions: &[(Action::Strum(3), "Rythme pop/folk")],
    },
    Lesson {
        title: "Premier morceau : 4 accords",
        body: &[
            "G, D, Em, C : avec ces quatre accords, tu peux accompagner un nombre énorme de chansons.",
            "Joue l'exercice avec le son, puis coupe le son et joue toi-même. Baisse la vitesse si ça accroche, et travaille le changement le plus dur avec un défi d'une minute.",
        ],
        chords: &["G", "D", "Em", "C"],
        actions: &[(Action::Song(EX_FOUR), "Jouer les 4 accords"), (Action::Changes(&["G", "D", "Em", "C"]), "Défi G → D → Em → C")],
    },
    Lesson {
        title: "Vers le barré : Fa (F)",
        body: &[
            "F est le premier mur de tout guitariste : l'index barre plusieurs cordes. Commence par Fmaj7, sa version facile sans barré, qui sonne très bien à la place de F.",
            "Ensuite le vrai F : index à plat sur la case 1, côté osseux, juste derrière la frette, le coude près du corps. Vérifie corde par corde au micro.",
            "Ne force pas des heures : quelques minutes par jour pendant des semaines, c'est comme ça que le barré vient.",
        ],
        chords: &["Fmaj7", "F"],
        actions: &[
            (Action::Listen("Fmaj7"), "Écouter Fmaj7"),
            (Action::Check("Fmaj7"), "Vérifier Fmaj7"),
            (Action::Check("F"), "Vérifier F (barré)"),
            (Action::Changes(&["C", "Fmaj7"]), "Défi C ↔ Fmaj7"),
        ],
    },
    Lesson {
        title: "Jouer tes morceaux",
        body: &[
            "Dans le lecteur de tab, plusieurs aides t'attendent dans la barre du bas :",
            "• Vitesse : joue à 50 % ou 75 % et monte petit à petit.\n• Boucle A–B : répète un passage difficile.\n• Attendre la note : Till attend que tu joues la bonne note au micro (désactivable, avec un bouton Passer).\n• Accords faciles : remplace les accords difficiles par des versions simples.\n• Capo : Till propose une position de capo qui rend les accords plus faciles.",
            "Dans la liste des tablatures, chaque morceau a une note de difficulté : commence par les plus faciles.",
        ],
        chords: &[],
        actions: &[(Action::Shelf, "Voir mes tablatures")],
    },
];

/// Lesson list and the selected lesson. Returns the action the player clicked.
pub fn show(ui: &mut egui::Ui, current: &mut usize, done: &mut BTreeSet<usize>, synth: Option<&Synth>) -> Option<Action> {
    let mut out = None;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(250.0);
            ui.add(
                egui::ProgressBar::new(done.len() as f32 / LESSONS.len() as f32)
                    .desired_width(240.0)
                    .text(format!("{} / {} leçons", done.len(), LESSONS.len())),
            );
            ui.add_space(8.0);
            for (i, l) in LESSONS.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(240.0, 34.0), Sense::click());
                let v = ui.visuals();
                let sel = *current == i;
                let fill = if sel {
                    ACCENT.gamma_multiply(0.16)
                } else if resp.hovered() {
                    v.widgets.hovered.weak_bg_fill
                } else {
                    egui::Color32::TRANSPARENT
                };
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(8), fill);
                let c = egui::pos2(rect.left() + 16.0, rect.center().y);
                if done.contains(&i) {
                    p.circle_filled(c, 9.0, ACCENT);
                    p.text(c, egui::Align2::CENTER_CENTER, "✔", egui::FontId::proportional(11.0), v.selection.stroke.color);
                } else {
                    p.circle_stroke(c, 9.0, egui::Stroke::new(1.2_f32, v.weak_text_color()));
                    p.text(c, egui::Align2::CENTER_CENTER, (i + 1).to_string(), egui::FontId::proportional(10.5), v.text_color());
                }
                let ink = if sel { ACCENT } else { v.text_color() };
                p.text(egui::pos2(rect.left() + 34.0, rect.center().y), egui::Align2::LEFT_CENTER, l.title, egui::FontId::proportional(13.0), ink);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    *current = i;
                }
            }
        });
        ui.add_space(18.0);
        ui.vertical(|ui| {
            let l = &LESSONS[*current];
            ui.label(RichText::new(format!("Leçon {}", *current + 1)).weak());
            ui.label(RichText::new(l.title).size(24.0).strong());
            ui.add_space(8.0);
            for para in l.body {
                ui.label(RichText::new(*para).size(14.5));
                ui.add_space(6.0);
            }
            if !l.chords.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for name in l.chords {
                        let Some(c) = Chord::parse(name) else { continue };
                        let v = voicing(&c);
                        let r = diagram::card(ui, &c.name(), &c.name_fr(), v.as_ref(), 1.3, false).on_hover_text("Clic : écouter");
                        if r.clicked() {
                            super::strum(synth, name, Play::Down, 1.0);
                        }
                    }
                });
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for &(a, label) in l.actions {
                    let accent = !matches!(a, Action::Listen(_));
                    let text = RichText::new(label).size(14.0);
                    let b = if accent {
                        egui::Button::new(text.color(ui.visuals().selection.stroke.color)).fill(ACCENT)
                    } else {
                        egui::Button::new(text)
                    };
                    if ui.add(b.min_size(vec2(0.0, 32.0))).clicked() {
                        if let Action::Listen(c) = a {
                            super::strum(synth, c, Play::Down, 1.0);
                            // Then note by note, so the shape can be checked by ear.
                            if let (Some(s), Some(v)) = (synth, Chord::parse(c).as_ref().and_then(voicing)) {
                                s.chord_at(&v.frets, Play::Arpeggio, 0.9, 1600.0);
                            }
                        }
                        out = Some(a);
                    }
                }
            });
            ui.add_space(16.0);
            let mut is_done = done.contains(current);
            if ui.checkbox(&mut is_done, "Leçon terminée").changed() {
                if is_done {
                    done.insert(*current);
                    if *current + 1 < LESSONS.len() {
                        *current += 1;
                    }
                } else {
                    done.remove(current);
                }
            }
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Timeline;
    use crate::song::{Block, parse};

    #[test]
    fn exercises_parse_and_play() {
        for (text, chords, tab) in [
            (EX_TAB, 0, true),
            (EX_STRINGS, 0, true),
            (EX_EM_AM, 2, false),
            (EX_E_A_D, 3, false),
            (EX_FOUR, 4, false),
        ] {
            let song = parse(text, 0);
            assert_eq!(song.chords.len(), chords, "{text}");
            assert_eq!(song.blocks.iter().any(|b| matches!(b, Block::Tab(_))), tab, "{text}");
            let t = Timeline::build(&song, 4.0, 4.0);
            assert!(t.onsets.len() >= 8, "{text}");
            assert!(crate::shelf::level(&song) <= 2, "{text}: level {}", crate::shelf::level(&song));
        }
        for l in LESSONS {
            for c in l.chords {
                assert!(Chord::parse(c).as_ref().and_then(voicing).is_some(), "{c}");
            }
        }
    }

    #[test]
    fn levels_rank_songs() {
        let redemption = std::fs::read_to_string("tabs/Redemption Song - Bob Marley.txt").unwrap();
        let lvl = crate::shelf::level(&parse(&redemption, 0));
        assert!((2..=4).contains(&lvl), "redemption {lvl}");
        let barre = parse("Bm F#m Bb Ebm C#m G#", 0);
        assert!(crate::shelf::level(&barre) >= 4);
    }
}
