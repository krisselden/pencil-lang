//! What's drawn with nothing said: a mark that would paint nothing gets its type's default paint
//! in the default ink, and `stroke: none` and `fill: none` keep a paint off.

use std::path::Path;

use pencil_core::eval::{self, Env, Frame};
use pencil_core::frame::{Inputs, prepare};
use pencil_core::ir::{Expr, Form};
use pencil_core::style::MarkStyle;
use pencil_core::{Source, Sources, build};

const MARKS: &str = "
    viewport { x: -2..2, y: -2..2 }
    let P = (0.5, 0.5)
    let Q = (-1, 0.5)
    curve c = circle((0, 0), (1, 0))
    curve l = line((-2, -1), (2, -1))
    curve r = ray((0, 0), (1, -1))
    curve k = implicit (x, y): x·y = 1
    curve s = parametric (t): (t, t²) for t in 0..1
    segment g = (-1, 1.5), (1, 1.5)
    region z = (-1.8, -1.8), (-1.2, -1.8), (-1.5, -1.2)
    dot P
    arrow a = (0, 0), (1, 0)
    dots d = (n/4, -1.5) for n in 0..3
    circles o = circle((n, 0), (n + 0.1, 0)) for n in 0..1
    label name = \"P\" at P";

fn styles(doc: &str, sheets: &[&str]) -> Vec<(String, MarkStyle)> {
    let f = build(doc, sheets).unwrap();
    f.scene
        .marks
        .iter()
        .map(|m| m.id.clone())
        .zip(f.styles.marks.iter().cloned())
        .collect()
}

fn of<'a>(styles: &'a [(String, MarkStyle)], id: &str) -> &'a MarkStyle {
    &styles.iter().find(|(m, _)| m == id).unwrap().1
}

fn black() -> Option<Expr> {
    Some(Expr::sym("black"))
}

#[test]
fn a_mark_with_nothing_said_gets_its_types_paint() {
    let s = styles(MARKS, &[]);
    // A stroke for what's a line, and an outline for a region.
    for id in ["c", "l", "r", "k", "s", "g", "z", "a", "o"] {
        let style = of(&s, id);
        assert_eq!((&style.stroke, &style.negative), (&black(), &None), "{id}");
    }
    // A fill for a dot. A collection of dots is drawn by its stroke, in that fill.
    assert_eq!(
        (&of(&s, "P").negative, &of(&s, "P").stroke),
        (&black(), &None)
    );
    assert_eq!(of(&s, "d").stroke, black());
    // Text gets the ink and nothing else: no halo, no settling, and still sans.
    let label = of(&s, "name");
    assert_eq!(label.negative, black());
    assert_eq!(label.halo, None);
    assert_eq!(label.place, pencil_core::style::Place::Fixed);
    assert_eq!(label.font, pencil_core::style::Font::Sans);
}

#[test]
fn a_mark_that_paints_anything_is_left_as_written() {
    let s = styles(
        MARKS,
        &["style c::inside { fill: white }\nstyle P { stroke: white }\nstyle z { fill: white }"],
    );
    let c = of(&s, "c");
    assert_eq!((&c.stroke, &c.inside), (&None, &Some(Expr::sym("white"))));
    assert_eq!(of(&s, "P").negative, None);
    assert_eq!(of(&s, "z").stroke, None);
}

#[test]
fn a_written_property_picks_its_paint() {
    // A width alone is a stroke in the ink, on a dot too, and keeps its width.
    let s = styles(
        MARKS,
        &["style c, P { width: 3px }\nstyle z { fill_opacity: 0.5 }"],
    );
    let c = of(&s, "c");
    assert_eq!(c.stroke, black());
    assert!(c.width.is_some());
    let p = of(&s, "P");
    assert_eq!((&p.stroke, &p.negative), (&black(), &None));
    // `fill_opacity` asks a region for a fill, and `radius` a dot.
    let z = of(&s, "z");
    assert_eq!((&z.negative, &z.stroke), (&black(), &None));
    let s = styles(MARKS, &["style P { radius: 6px }"]);
    assert_eq!(
        (&of(&s, "P").negative, &of(&s, "P").stroke),
        (&black(), &None)
    );
    // A fill its type never draws is no paint, so a segment with one still gets its stroke.
    let s = styles(MARKS, &["style g { fill: white }"]);
    assert_eq!(of(&s, "g").stroke, black());
}

#[test]
fn none_keeps_a_paint_off() {
    let s = styles(
        MARKS,
        &["style c, g, z { stroke: none }\nstyle d { fill: none }\nstyle name { fill: none }"],
    );
    for id in ["c", "g", "z"] {
        let style = of(&s, id);
        assert_eq!((&style.stroke, &style.negative), (&None, &None), "{id}");
    }
    assert_eq!(of(&s, "d").stroke, None);
    // `fill: none` hides a dot. `stroke: none` on one with nothing else still fills it, as SVG's
    // would.
    let s = styles(MARKS, &["style P { fill: none }"]);
    assert_eq!((&of(&s, "P").negative, &of(&s, "P").stroke), (&None, &None));
    let s = styles(MARKS, &["style P { stroke: none }"]);
    assert_eq!(of(&s, "P").negative, black());
    // A width doesn't bring back a stroke that's `none`: the dot gets its fill instead.
    let s = styles(MARKS, &["style P { stroke: none; width: 2px }"]);
    assert_eq!(
        (&of(&s, "P").negative, &of(&s, "P").stroke),
        (&black(), &None)
    );
    // A later rule can paint it again.
    let s = styles(
        MARKS,
        &["style c { stroke: none }\nstyle circle { stroke: white }"],
    );
    assert_eq!(of(&s, "c").stroke, Some(Expr::sym("white")));
    // A label with `fill: none` isn't drawn.
    let f = build(MARKS, &["style name { fill: none }"]).unwrap();
    let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
    assert!(p.labels.is_empty());
    // Every other property's `none` still takes back what an earlier rule set.
    let s = styles(MARKS, &["style g { dash: 4px }\nstyle g { dash: none }"]);
    assert_eq!((&of(&s, "g").dash, &of(&s, "g").stroke), (&None, &black()));
}

/// The pixel at the middle of a 100² frame of `doc` with `sheet`, as the CPU draws it.
fn middle(doc: &str, sheet: &str) -> [f64; 3] {
    let f = build(doc, &[sheet]).unwrap();
    Frame::new(&f, &Inputs::at(0.0), 100, 100)
        .unwrap()
        .shade(50.5, 50.5)
        .unwrap()
}

const DOT: &str = "viewport { x: -1..1, y: -1..1 }\nlet P = (0, 0)\ndot P";

#[test]
fn the_ink_is_the_sheets_ink_or_stands_out_from_the_paper() {
    // No paper is white, and the ink black.
    assert_eq!(middle(DOT, ""), [0.0; 3]);
    // A light paper and a dark one.
    assert_eq!(
        middle(DOT, "style paper { fill: rgb(240, 230, 200) }"),
        [0.0; 3]
    );
    assert_eq!(
        middle(DOT, "style paper { fill: rgb(30, 20, 60) }"),
        [1.0; 3]
    );
    // A sheet's `ink` wins, wherever it's defined.
    let red = middle(DOT, "style paper { fill: black }\nlet ink = rgb(255, 0, 0)");
    assert_eq!(red, [1.0, 0.0, 0.0]);
    // A paper that changes is judged frame by frame.
    let doc = format!("{DOT}\nparam p = 0 in 0..1");
    let sheet = "style paper { fill: mix(black, white, p) }";
    let f = build(&doc, &[sheet]).unwrap();
    let at = |p: f64| {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("p".to_string(), p)],
        };
        Frame::new(&f, &inputs, 100, 100)
            .unwrap()
            .shade(50.5, 50.5)
            .unwrap()
    };
    assert_eq!(at(0.0), [1.0; 3]);
    assert_eq!(at(1.0), [0.0; 3]);
    // Text and a panel's title take it too.
    let f = build(
        &format!("{DOT}\nlabel name = \"P\" at P"),
        &["style paper { fill: rgb(30, 20, 60) }"],
    )
    .unwrap();
    let p = prepare(&f, &Inputs::at(0.0), 100, 100, 1.0).unwrap();
    assert_eq!(p.labels[0].color, [1.0; 3]);
    assert_eq!(f.styles.title_fill, Some(Expr::sym("white")));
}

#[test]
fn contrast_turns_at_equal_contrast() {
    let at = |l: f64| {
        let c = Expr::app(
            "Contrast",
            vec![Expr::app(
                "Mix",
                vec![Expr::sym("black"), Expr::sym("white"), Expr::Num(l)],
            )],
        );
        eval::color(&c, &Env::new(0.0)).unwrap()
    };
    let turn = eval::CONTRAST_LUMINANCE;
    assert_eq!(at(turn + 1e-6), [0.0; 3]);
    assert_eq!(at(turn - 1e-6), [1.0; 3]);
    // At that luminance black and white contrast equally, by WCAG's ratio.
    assert!(((turn + 0.05) / 0.05 - 1.05 / (turn + 0.05)).abs() < 1e-9);
}

// The corpus: every look, and the sheets the test pages and the explainer load. None of their
// marks should get a paint their sheets didn't write, and text that gets the ink gets black.

const THEME: &str = "corpus/theme.style";
const DARK: &str = "corpus/dark.style";

fn looks() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "corpus/001-circle/circle.pen",
            vec!["corpus/001-circle/circle.style"],
        ),
        (
            "corpus/001-circle/circle.pen",
            vec!["corpus/001-circle/bands.style"],
        ),
        (
            "corpus/001-circle/circle.pen",
            vec![THEME, "corpus/001-circle/circle.style", DARK],
        ),
        (
            "corpus/002-strokes/ring.pen",
            vec!["corpus/002-strokes/filled.style"],
        ),
        (
            "corpus/002-strokes/ring.pen",
            vec!["corpus/002-strokes/solid.style"],
        ),
        (
            "corpus/002-strokes/ring.pen",
            vec!["corpus/002-strokes/dashed.style"],
        ),
        (
            "corpus/002-strokes/wave.pen",
            vec!["corpus/002-strokes/taper.style"],
        ),
        (
            "corpus/003-unroll/unroll.pen",
            vec!["corpus/003-unroll/blog.style"],
        ),
        (
            "corpus/003-unroll/unroll.pen",
            vec!["corpus/003-unroll/night.style"],
        ),
        (
            "corpus/004-bounds/bounds.pen",
            vec!["corpus/004-bounds/blog.style"],
        ),
        (
            "corpus/004-bounds/bounds.pen",
            vec![THEME, "corpus/004-bounds/blog.style"],
        ),
        (
            "corpus/005-tangents/tangents.pen",
            vec![THEME, "corpus/005-tangents/plain.style", DARK],
        ),
        (
            "corpus/006-square/square.pen",
            vec![THEME, "corpus/006-square/plain.style", DARK],
        ),
        (
            "corpus/007-lattice/lattice.pen",
            vec![THEME, "corpus/007-lattice/plain.style", DARK],
        ),
        (
            "corpus/007-lattice/lattice.pen",
            vec![
                THEME,
                "corpus/007-lattice/plain.style",
                "corpus/007-lattice/narrow.style",
            ],
        ),
        (
            "corpus/008-hearts/hearts.pen",
            vec![THEME, "corpus/008-hearts/plain.style", DARK],
        ),
        (
            "corpus/008-hearts/hearts.pen",
            vec!["corpus/008-hearts/glow.style"],
        ),
        (
            "corpus/008-hearts/hearts.pen",
            vec![
                "corpus/008-hearts/glow.style",
                "corpus/008-hearts/card.style",
            ],
        ),
        (
            "corpus/009-squaring/squaring.pen",
            vec![THEME, "corpus/009-squaring/plain.style", DARK],
        ),
        (
            "corpus/010-mobius/mobius.pen",
            vec![THEME, "corpus/010-mobius/plain.style", DARK],
        ),
        (
            "corpus/010-mobius/mobius.pen",
            vec![
                THEME,
                "corpus/010-mobius/plain.style",
                "corpus/010-mobius/outside.style",
            ],
        ),
        (
            "corpus/011-circumcircle/circumcircle.pen",
            vec!["corpus/011-circumcircle/plain.style"],
        ),
        (
            "corpus/012-limacon/limacon.pen",
            vec!["corpus/012-limacon/plain.style"],
        ),
        (
            "corpus/013-tube/tube.pen",
            vec![THEME, "corpus/013-tube/plain.style", DARK],
        ),
        (
            "corpus/014-scaled-copy/scaled-copy.pen",
            vec![THEME, "corpus/014-scaled-copy/plain.style", DARK],
        ),
        (
            "corpus/015-log-nearest/log-nearest.pen",
            vec![THEME, "corpus/015-log-nearest/plain.style", DARK],
        ),
        (
            "corpus/016-unwrap/unwrap.pen",
            vec![THEME, "corpus/016-unwrap/plain.style", DARK],
        ),
        (
            "lab/explainer/limacon.pen",
            vec![
                THEME,
                DARK,
                "lab/explainer/limacon.style",
                "lab/explainer/caption.style",
            ],
        ),
        (
            "lab/explainer/proof.pen",
            vec![
                THEME,
                DARK,
                "lab/explainer/proof.style",
                "lab/explainer/caption.style",
            ],
        ),
        (
            "packages/figure/tests/pages/formulas.pen",
            vec!["packages/figure/tests/pages/formulas.style"],
        ),
    ]
}

fn read(name: &str) -> Option<String> {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    std::fs::read_to_string(root.join(name)).ok()
}

/// A paint no sheet writes stays as this, so a mark that still has it was given it by default.
const UNWRITTEN_STROKE: &str = "rgb(1, 2, 3)";
const UNWRITTEN_FILL: &str = "rgb(4, 5, 6)";

fn compose(pen: &str, sheets: &[String]) -> pencil_core::Composition {
    let doc = read(pen).unwrap();
    let texts: Vec<(&str, String)> = sheets
        .iter()
        .map(|s| match read(s) {
            Some(text) => (s.as_str(), text),
            None => ("", s.clone()),
        })
        .collect();
    let sources: Vec<Source> = texts
        .iter()
        .map(|(name, text)| Source { name, text })
        .collect();
    Sources::read(
        Source {
            name: pen,
            text: &doc,
        },
        &sources,
        &read,
    )
    .unwrap()
    .compose()
    .unwrap_or_else(|e| panic!("{pen}: {e}"))
}

#[test]
fn no_corpus_look_gets_a_paint_it_didnt_write() {
    let probe = format!(
        "style curve, circle, line, ray, segment, decoration, region, dot, arrow, dots, circles, \
         outline {{ stroke: {UNWRITTEN_STROKE} }}\n\
         style dot, region, dots, label, term, title, formula {{ fill: {UNWRITTEN_FILL} }}"
    );
    let unwritten = |e: &Option<Expr>, rgb: [f64; 3]| {
        e.as_ref()
            .and_then(|e| eval::color(e, &Env::new(0.0)).ok())
            .is_some_and(|c| {
                c.iter()
                    .zip(rgb)
                    .all(|(a, b)| (a - eval::srgb_decode(b / 255.0)).abs() < 1e-12)
            })
    };
    let mut found = Vec::new();
    for (pen, sheets) in looks() {
        let sheets: Vec<String> = sheets.iter().map(|s| s.to_string()).collect();
        let plain = compose(pen, &sheets);
        let probed = compose(pen, &[vec![probe.clone()], sheets.clone()].concat());
        for (panel, probe_panel) in plain.panels.iter().zip(&probed.panels) {
            let (f, p) = (&panel.figure, &probe_panel.figure);
            for title in [
                (&f.styles.title_fill, &p.styles.title_fill),
                (&f.styles.formula_fill, &p.styles.formula_fill),
            ] {
                if unwritten(title.1, [4.0, 5.0, 6.0]) && panel.title.is_some() {
                    found.push(format!("{pen} {sheets:?}: a title or formula's fill"));
                }
            }
            for ((mark, style), probe) in f
                .scene
                .marks
                .iter()
                .zip(&f.styles.marks)
                .zip(&p.styles.marks)
            {
                let text = matches!(mark.form, Form::Label { .. } | Form::Term { .. });
                let dots = mark.tags.iter().any(|t| t == "dots");
                let defaulted = if text {
                    // The ink replaces black, so it may only be black.
                    unwritten(&probe.negative, [4.0, 5.0, 6.0]) && style.negative != black()
                } else if dots {
                    unwritten(&probe.stroke, [4.0, 5.0, 6.0]) && style.stroke.is_some()
                } else {
                    (unwritten(&probe.stroke, [1.0, 2.0, 3.0]) && style.stroke.is_some())
                        || (unwritten(&probe.negative, [4.0, 5.0, 6.0]) && style.negative.is_some())
                };
                if defaulted {
                    found.push(format!("{pen} {sheets:?}: {} {}", panel.id, mark.id));
                }
            }
        }
    }
    assert!(found.is_empty(), "given a default:\n{}", found.join("\n"));
}

#[test]
fn every_corpus_figure_draws_with_no_sheet() {
    let mut pens: Vec<&str> = looks().into_iter().map(|(pen, _)| pen).collect();
    pens.dedup();
    for pen in pens {
        let bare = compose(pen, &[]);
        for panel in &bare.panels {
            for (mark, style) in panel
                .figure
                .scene
                .marks
                .iter()
                .zip(&panel.figure.styles.marks)
            {
                let paints = style.stroke.is_some()
                    || style.negative.is_some()
                    || matches!(mark.form, Form::Label { .. } | Form::Term { .. });
                assert!(paints, "{pen}: {} paints nothing", mark.id);
            }
        }
    }
}
