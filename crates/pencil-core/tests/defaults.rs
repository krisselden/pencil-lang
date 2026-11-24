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
    // Text gets the ink, and a label settles. Nothing else: no halo, and still sans.
    let label = of(&s, "name");
    assert_eq!(label.negative, black());
    assert_eq!(label.halo, None);
    assert_eq!(label.place, pencil_core::style::Place::Settle);
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
    // A property asks for a paint only on a mark it applies to: `radius` doesn't size a region,
    // and a dot's stroke is a ring, which has no dashes and no head.
    let s = styles(
        MARKS,
        &["style z { radius: 4px }\nstyle P { dash: 4px; gap: 2px; head: 9px; head_span: 1 }"],
    );
    assert_eq!(
        (&of(&s, "z").stroke, &of(&s, "z").negative),
        (&black(), &None)
    );
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

/// With nothing said, a value's label settles off its own dot rather than sitting on it.
#[test]
fn a_label_with_nothing_said_moves_off_its_dot() {
    let doc = "viewport { x: -2..2, y: -2..2 }\nlet z = (0.5, 0.5)\ndot z\nlabel z at z";
    let f = build(doc, &[]).unwrap();
    assert_eq!(f.styles.marks[1].place, pencil_core::style::Place::Settle);
    let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
    let label = &p.labels[0];
    let [hw, hh] = label.half(&pencil_core::terms::Estimate);
    // The dot is at (250, 150), 3 px across: the nearest point of the label's box is past it.
    let (cx, cy) = (250.0, 150.0);
    let dx = (cx - label.x).abs() - hw;
    let dy = (cy - label.y).abs() - hh;
    let gap = dx.max(0.0).hypot(dy.max(0.0));
    assert!(
        gap > pencil_core::frame::DEFAULT_DOT_RADIUS,
        "{gap} from {label:?}"
    );
    // `place: fixed` keeps it where it's written, on the dot.
    let f = build(doc, &["style label { place: fixed }"]).unwrap();
    let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
    assert_eq!((p.labels[0].x, p.labels[0].y), (cx, cy));
}

/// The sheets' `ink` is read as a rule's value is: `duration` in it is the figure's number.
#[test]
fn ink_can_read_the_duration() {
    let doc = format!("duration 4\n{DOT}");
    let f = build(&doc, &["let ink = mix(black, white, time/duration)"]).unwrap();
    let stroke = f.styles.marks[0].negative.as_ref().unwrap();
    let mut read = Vec::new();
    stroke.visit_syms(&mut |s| read.push(s.to_string()));
    assert!(!read.iter().any(|s| s == "duration"), "{read:?}");
    let err = build(DOT, &["let ink = mix(black, white, time/duration)"]).unwrap_err();
    assert!(err.message.contains("no `duration`"), "{err}");
    assert_eq!(err.sheet, Some(0));
}

#[test]
fn ink_has_to_be_a_color() {
    let err = build(DOT, &["style paper { fill: black }", "let ink = 3"]).unwrap_err();
    assert!(
        err.message.contains("`ink`") && err.message.contains("has to be a color"),
        "{err}"
    );
    assert_eq!((err.sheet, err.line), (Some(1), 1));
}

#[test]
fn text_with_fill_none_isnt_drawn() {
    let labels = |doc: &str, sheet: &str, time: f64| -> Vec<String> {
        let composition = pencil_core::compose(doc, &[sheet]).unwrap();
        let inputs = Inputs::at(time);
        let frames = composition
            .prepare(&inputs, 400, 200, 1.0, None, &pencil_core::terms::Estimate)
            .unwrap();
        composition
            .labels(&frames, &inputs, &pencil_core::terms::Estimate)
            .unwrap()
            .into_iter()
            .map(|l| l.id)
            .collect()
    };
    let panels = "panel one \"One\" \"x^2\" { viewport { x: 0..1, y: 0..1 } }";
    assert_eq!(labels(panels, "", 0.0), ["one/title", "one/formula"]);
    let none = "style title { fill: none }\nstyle formula { fill: none }";
    assert!(labels(panels, none, 0.0).is_empty());
    // A caption, once its beat has faded it in.
    let story = "beat grow \"A circle.\" for 1 show c\ncurve c = circle((0, 0), (grow, 0))";
    let caption = labels(story, "style caption { size: 12px }", 0.5);
    assert!(caption[0].starts_with("caption/"), "{caption:?}");
    assert!(labels(story, "style caption { size: 12px; fill: none }", 0.5).is_empty());
    // The paper, and what's filled by its sides, can't be `fill: none`, and say why.
    let err = build(DOT, &["style paper { fill: none }"]).unwrap_err();
    assert!(err.message.contains("everything is drawn on"), "{err}");
    let err = build(MARKS, &["style c { fill: none }"]).unwrap_err();
    assert!(
        err.message.contains("needs a facet, such as `c::inside`"),
        "{err}"
    );
    let err = build(MARKS, &["style o { fill: none }"]).unwrap_err();
    assert!(err.message.contains("filled by its sides"), "{err}");
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

// The corpus: every look of every figure, light and dark, the explainer's, the highlights' and the
// sheets the element's formulas page loads. None of their marks should get a paint their sheets
// didn't write, and text that gets the ink has to get the black it had.
//
// Left out: the element's fixture pages with no sheet or only the theme (`plane`, `keyed`, `panels`,
// and `leader`'s dot), whose marks are meant to draw by default now; `web/strokes.html`, whose
// inline sheets all give a stroke; and the playground, the keyboard lab and the prompter, which
// load the corpus looks above or a take generated from the explainer.

const THEME: &str = "corpus/theme.style";
const DARK: &str = "corpus/dark.style";

fn looks() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut looks = vec![
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
            vec![THEME, "corpus/001-circle/circle.style"],
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
            vec![THEME, "corpus/005-tangents/plain.style"],
        ),
        (
            "corpus/006-square/square.pen",
            vec![THEME, "corpus/006-square/plain.style"],
        ),
        (
            "corpus/007-lattice/lattice.pen",
            vec![THEME, "corpus/007-lattice/plain.style"],
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
            vec![THEME, "corpus/008-hearts/plain.style"],
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
            vec![THEME, "corpus/009-squaring/plain.style"],
        ),
        (
            "corpus/010-mobius/mobius.pen",
            vec![THEME, "corpus/010-mobius/plain.style"],
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
            vec![THEME, "corpus/013-tube/plain.style"],
        ),
        (
            "corpus/014-scaled-copy/scaled-copy.pen",
            vec![THEME, "corpus/014-scaled-copy/plain.style"],
        ),
        (
            "corpus/015-log-nearest/log-nearest.pen",
            vec![THEME, "corpus/015-log-nearest/plain.style"],
        ),
        (
            "corpus/016-unwrap/unwrap.pen",
            vec![THEME, "corpus/016-unwrap/plain.style"],
        ),
        (
            "corpus/017-arm-colors/arm-colors.pen",
            vec![THEME, "corpus/017-arm-colors/plain.style"],
        ),
        (
            "corpus/017-arm-colors/arm-colors.pen",
            vec![
                THEME,
                "corpus/017-arm-colors/plain.style",
                "corpus/017-arm-colors/period.style",
            ],
        ),
        (
            "corpus/017-arm-colors/arm-colors.pen",
            vec![
                THEME,
                "corpus/017-arm-colors/plain.style",
                "corpus/017-arm-colors/narrow.style",
            ],
        ),
        (
            "corpus/018-camera-log/camera-log.pen",
            vec![THEME, "corpus/018-camera-log/plain.style"],
        ),
        (
            "corpus/018-camera-log/camera-log.pen",
            vec![
                THEME,
                "corpus/018-camera-log/plain.style",
                "corpus/018-camera-log/narrow.style",
            ],
        ),
        (
            "lab/explainer/limacon.pen",
            vec![
                THEME,
                "lab/explainer/limacon.style",
                "lab/explainer/caption.style",
            ],
        ),
        (
            "lab/explainer/proof.pen",
            vec![
                THEME,
                "lab/explainer/proof.style",
                "lab/explainer/caption.style",
            ],
        ),
        (
            "lab/highlights/near.pen",
            vec![THEME, "lab/highlights/styles/near.style"],
        ),
        (
            "lab/highlights/central.pen",
            vec![THEME, "lab/highlights/styles/central.style"],
        ),
        (
            "packages/figure/tests/pages/formulas.pen",
            vec!["packages/figure/tests/pages/formulas.style"],
        ),
    ];
    for part in [
        "1_part", "2_ring", "3_spot", "4_arms", "5_taper", "6_circle",
    ] {
        let sheet: &'static str = format!("lab/highlights/styles/{part}.style").leak();
        looks.push((
            "lab/highlights/spike.pen",
            vec![THEME, "lab/highlights/styles/base.style", sheet],
        ));
    }
    // Every look that loads the theme has a dark variant, which only renames its colors.
    let dark: Vec<_> = looks
        .iter()
        .filter(|(_, sheets)| sheets.contains(&THEME))
        .map(|(pen, sheets)| (*pen, [sheets.clone(), vec![DARK]].concat()))
        .collect();
    looks.extend(dark);
    looks
}

fn read(name: &str) -> Option<String> {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    std::fs::read_to_string(root.join(name)).ok()
}

/// A paint no sheet writes stays as this, so a mark that still has it was given it by default.
const UNWRITTEN_STROKE: [f64; 3] = [1.0, 2.0, 3.0];
const UNWRITTEN_FILL: [f64; 3] = [4.0, 5.0, 6.0];

fn compose(pen: &str, doc: &str, sheets: &[String]) -> pencil_core::Composition {
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
            text: doc,
        },
        &sources,
        &read,
    )
    .unwrap()
    .compose()
    .unwrap_or_else(|e| panic!("{pen}: {e}"))
}

/// The type a mark was declared as.
fn type_of(mark: &pencil_core::ir::Mark) -> &str {
    const TYPES: [&str; 15] = [
        "circle",
        "line",
        "ray",
        "curve",
        "segment",
        "decoration",
        "region",
        "dot",
        "label",
        "arrow",
        "term",
        "dots",
        "circles",
        "curves",
        "outline",
    ];
    mark.tags
        .iter()
        .map(String::as_str)
        .find(|t| TYPES.contains(t))
        .unwrap_or_else(|| panic!("`{}` has no type", mark.id))
}

/// What in this document under these sheets was given a paint, or text a color other than black,
/// that no sheet wrote: `panel mark` for each, and `panel title`, `panel formula` and `caption`.
fn defaulted(pen: &str, doc: &str, sheets: &[String]) -> Vec<String> {
    let [s, f] = [UNWRITTEN_STROKE, UNWRITTEN_FILL].map(|[r, g, b]| format!("rgb({r}, {g}, {b})"));
    let probe = format!(
        "style curve, circle, line, ray, segment, decoration, region, dot, arrow, dots, circles, \
         curves, outline {{ stroke: {s} }}\n\
         style dot, region, dots, label, term, title, formula, caption {{ fill: {f} }}"
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
    let plain = compose(pen, doc, sheets);
    let probed = compose(pen, doc, &[vec![probe], sheets.to_vec()].concat());
    let mut found = Vec::new();
    for (panel, probe_panel) in plain.panels.iter().zip(&probed.panels) {
        let (f, p) = (&panel.figure.styles, &probe_panel.figure.styles);
        // Text that takes the ink may only take the black it had.
        let not_black = |e: &Option<Expr>| e.is_some() && *e != black();
        if panel.title.is_some()
            && unwritten(&p.title_fill, UNWRITTEN_FILL)
            && not_black(&f.title_fill)
        {
            found.push(format!("{} title", panel.id));
        }
        if panel.formula.is_some()
            && unwritten(&p.formula_fill, UNWRITTEN_FILL)
            && not_black(&f.formula_fill)
        {
            found.push(format!("{} formula", panel.id));
        }
        if let (Some(caption), Some(probe)) = (&f.caption, &p.caption)
            && unwritten(&probe.fill, UNWRITTEN_FILL)
            && not_black(&caption.fill)
        {
            found.push("caption".into());
        }
        let scene = &panel.figure.scene;
        for ((mark, style), probe) in scene.marks.iter().zip(&f.marks).zip(&p.marks) {
            // What `show: 0` keeps out of every frame shows no paint, given or not.
            let absent = style
                .show
                .as_ref()
                .and_then(|e| pencil_core::compile::constant(e, "show").ok())
                .is_some_and(|v| v <= 0.0);
            if absent {
                continue;
            }
            let defaulted = match type_of(mark) {
                "label" | "term" => {
                    unwritten(&probe.negative, UNWRITTEN_FILL) && not_black(&style.negative)
                }
                // A collection of dots is drawn by its stroke, in its fill.
                "dots" => unwritten(&probe.stroke, UNWRITTEN_FILL) && style.stroke.is_some(),
                _ => {
                    (unwritten(&probe.stroke, UNWRITTEN_STROKE) && style.stroke.is_some())
                        || (unwritten(&probe.negative, UNWRITTEN_FILL) && style.negative.is_some())
                }
            };
            if defaulted {
                found.push(format!("{} {}", panel.id, mark.id));
            }
        }
    }
    found
}

#[test]
fn no_corpus_look_gets_a_paint_it_didnt_write() {
    let mut found = Vec::new();
    for (pen, sheets) in looks() {
        let sheets: Vec<String> = sheets.iter().map(|s| s.to_string()).collect();
        for what in defaulted(pen, &read(pen).unwrap(), &sheets) {
            found.push(format!("{pen} {sheets:?}: {what}"));
        }
    }
    assert!(found.is_empty(), "given a default:\n{}", found.join("\n"));
}

/// The guard above can see every type of mark the corpus has: one of each, given nothing, is
/// flagged, and given its paint, isn't.
/// One mark of every type, and an outline, which only a figure with panels has.
const EVERY_TYPE: [&str; 2] = [
    "viewport { x: -2..2, y: -2..2 }
    param step = 1 in 0..1
    let P = (0.5, 0.5)
    curve a_curve = parametric (t): (t, t²) for t in 0..1
    curve a_circle = circle((0, 0), (1, 0))
    curve a_line = line((-2, -1), (2, -1))
    curve a_ray = ray((0, 0), (1, -1))
    segment a_segment = (-1, 1.5), (1, 1.5)
    rightangle a_decoration = (1, 0), (0, 0), (0, 1) size 0.2
    region a_region = (-1.8, -1.8), (-1.2, -1.8), (-1.5, -1.2)
    dot P
    arrow an_arrow = (0, 0), (1, 0)
    dots some_dots = (n/4, -1.5) for n in 0..3
    circles some_circles = circle((n, 0), (n + 0.1, 0)) for n in 0..1
    curves some_curves = parametric (t): (t, t + n) for t in 0..1 for n in 0..1
    label a_label = \"P\" at P
    place spot = (0, 0)
    expression e by step {
      \"x\" at spot
      \"x\" at spot
    }",
    "let P = (0.5, 0.5)
    panel whole \"Whole\" {
      viewport { x: -2..2, y: -2..2 }
      outline an_outline = detail
    }
    panel detail \"Detail\" {
      viewport { x: 0..1, y: 0..1 }
      dot P
    }",
];

/// The guard above can see every type of mark the corpus has: one of each, given nothing, is
/// flagged, and given its paint, isn't.
#[test]
fn the_guard_sees_every_type_the_corpus_has() {
    let mut fixture = std::collections::BTreeMap::new();
    for doc in EVERY_TYPE {
        for panel in &compose("", doc, &[]).panels {
            for mark in &panel.figure.scene.marks {
                let at = format!("{} {}", panel.id, mark.id).trim().to_string();
                fixture
                    .entry(type_of(mark).to_string())
                    .or_insert((doc, at, mark.id.clone()));
            }
        }
    }
    let mut corpus = std::collections::BTreeSet::new();
    for (pen, _) in looks() {
        for panel in &compose(pen, &read(pen).unwrap(), &[]).panels {
            for mark in &panel.figure.scene.marks {
                corpus.insert(type_of(mark).to_string());
            }
        }
    }
    assert!(
        corpus.contains("curves") && corpus.contains("outline"),
        "{corpus:?}"
    );
    // On a dark paper, so the ink isn't the black that text may take.
    let paper = "style paper { fill: black }".to_string();
    for ty in &corpus {
        let (doc, at, id) = fixture
            .get(ty)
            .unwrap_or_else(|| panic!("the guard's fixture has no `{ty}`"));
        let flagged = |sheet: &str| {
            defaulted("", doc, &[sheet.to_string()])
                .iter()
                .any(|f| f.trim() == at)
        };
        assert!(flagged(&paper), "`{ty}` given nothing isn't flagged");
        let paint = match ty.as_str() {
            "dot" | "dots" | "label" | "term" => "fill",
            _ => "stroke",
        };
        let painted = format!("{paper}\nstyle {id} {{ {paint}: white }}");
        assert!(!flagged(&painted), "`{ty}` given its paint is flagged");
    }
}

#[test]
fn every_corpus_figure_draws_with_no_sheet() {
    let mut pens: Vec<&str> = looks().into_iter().map(|(pen, _)| pen).collect();
    pens.sort();
    pens.dedup();
    for pen in pens {
        let bare = compose(pen, &read(pen).unwrap(), &[]);
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
