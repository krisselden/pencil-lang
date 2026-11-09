//! One parametric curve drawn many times, as `dots` and `circles` are.

use pencil_core::build;
use pencil_core::eval::Frame;
use pencil_core::frame::{self, Inputs};

const VIEW: &str = "viewport { x: -8..8, y: -8..8 }\n";
const INK: &str = "style paper { fill: white } style curves { stroke: black; width: 2px }";

/// Whether each row of a 160 px frame, at this column, has ink in it. 10 px is one world unit.
fn ink_at(doc: &str, sheet: &str, column: f64, rows: &[f64]) -> Vec<bool> {
    let figure = build(&format!("{VIEW}{doc}"), &[sheet]).unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    rows.iter()
        .map(|row| frame.shade(column, *row).unwrap()[0] < 0.5)
        .collect()
}

/// y = 1, 2 and 3 are at rows 70, 60 and 50, and these are just below each.
const LINES: [f64; 6] = [70.0, 75.5, 60.0, 65.5, 50.0, 55.5];

#[test]
fn it_draws_one_curve_for_each_count() {
    let doc = "curves c = parametric (t): (t, s) for t in -9..9 for s in 1..3";
    assert_eq!(
        ink_at(doc, INK, 80.5, &LINES),
        [true, false, true, false, true, false]
    );
    // It's one mark, with one layer to draw into, however many it holds.
    let figure = build(&format!("{VIEW}{doc}"), &[INK]).unwrap();
    assert_eq!(figure.scene.marks.len(), 1);
    let prepared = frame::prepare(&figure, &Inputs::at(0.0), 160, 160, 1.0).unwrap();
    assert_eq!(prepared.strokes.len(), 1);
}

#[test]
fn a_count_can_follow_a_param() {
    let doc = "param n = 2 in 1..3
               curves c = parametric (t): (t, s) for t in -9..9 for s in 1..n";
    assert_eq!(
        ink_at(doc, INK, 80.5, &LINES),
        [true, false, true, false, false, false]
    );
}

#[test]
fn its_range_and_point_can_read_what_counts_it() {
    // Each line starts at x = 0 and is as long as its count, so columns 80..80 + 10·s have ink.
    let doc = "curves c = parametric (t): (t, s) for t in 0..s for s in 1..3";
    assert_eq!(
        ink_at(doc, INK, 85.5, &[70.0, 60.0, 50.0]),
        [true, true, true]
    );
    assert_eq!(
        ink_at(doc, INK, 95.5, &[70.0, 60.0, 50.0]),
        [false, true, true]
    );
    assert_eq!(
        ink_at(doc, INK, 105.5, &[70.0, 60.0, 50.0]),
        [false, false, true]
    );
    assert_eq!(
        ink_at(doc, INK, 75.5, &[70.0, 60.0, 50.0]),
        [false, false, false]
    );
}

#[test]
fn with_names_a_value_per_curve() {
    let doc = "curves c = parametric (t): (t, y) for t in -9..9 for s in 1..2 with y = s + 1";
    assert_eq!(
        ink_at(doc, INK, 80.5, &LINES),
        [false, false, true, false, true, false]
    );
    // A point too.
    let doc =
        "curves c = parametric (t): at + (t, 0) for t in -9..9 for s in 1..2 with at = (0, s + 1)";
    assert_eq!(
        ink_at(doc, INK, 80.5, &LINES),
        [false, false, true, false, true, false]
    );
}

#[test]
fn a_curve_that_does_not_exist_is_skipped() {
    // The two circles meet only for s = 2 and 3, and otherwise `at` doesn't exist, so neither
    // does that curve. Only those two are drawn, and nothing is an error.
    let doc = "curves c = parametric (t): (t, at.y) for t in -9..9 for s in 1..4 with at = intersect(line((0, s), (1, s)), circle((0, 0), (3.5, 0)), 0)";
    let figure = build(&format!("{VIEW}{doc}"), &[INK]);
    let figure = figure.unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    let ink = |row: f64| frame.shade(80.5, row).unwrap()[0] < 0.5;
    assert!(
        [ink(70.0), ink(60.0), ink(50.0)].iter().all(|i| *i),
        "the lines that meet it"
    );
    assert!(!ink(40.0), "y = 4 is past a circle of radius 3.5");
}

#[test]
fn its_parameter_is_its_own() {
    // `t` in the document is something else, and so is `s`, and neither gets in the way.
    let doc = "let t = 5
               let s = 6
               curves c = parametric (t): (t, s) for t in -t..t for s in 1..1";
    assert_eq!(ink_at(doc, INK, 80.5, &[70.0, 20.0]), [true, false]);
    // The range is -5..5, so it stops before x = 6.
    assert_eq!(ink_at(doc, INK, 125.5, &[70.0]), [true]);
    assert_eq!(ink_at(doc, INK, 135.5, &[70.0]), [false]);
    // In a group, renamed and drawn twice, as well.
    let doc = "let t = 5
               group g(k) {
                 curves c = parametric (u): (u, k + s) for u in -t..t for s in 0..1
               }
               use g(1) as a
               use g(3) as b";
    assert_eq!(
        ink_at(doc, INK, 80.5, &[70.0, 60.0, 50.0, 40.0, 30.0]),
        [true, true, true, true, false]
    );
}

#[test]
fn it_goes_through_a_map() {
    // Doubled, y = 1 is at y = 2, and its length is doubled too.
    let doc = "fn twice(z) = 2·z
               group g() { curves c = parametric (t): (t, s) for t in -3..3 for s in 1..2 }
               use g() through twice";
    assert_eq!(
        ink_at(doc, INK, 80.5, &[60.0, 40.0, 70.0]),
        [true, true, false]
    );
    assert_eq!(ink_at(doc, INK, 135.5, &[60.0]), [true]);
    assert_eq!(ink_at(doc, INK, 145.5, &[60.0]), [false]);
}

#[test]
fn its_width_can_vary_by_count_and_along_each_curve() {
    let doc = "curves c = parametric (t): (t, 3·s) for t in -9..9 for s in 0..1";
    let sheet = "style paper { fill: white } style c { stroke: black; width: (1 + 7·s)·1px }";
    // y = 0 at row 80, and y = 3 at row 50.
    assert_eq!(ink_at(doc, sheet, 80.5, &[83.5, 53.5]), [false, true]);
    let sheet = "style paper { fill: white } style c { stroke: black; width: (1 + 9·self.t)·1px }";
    assert_eq!(ink_at(doc, sheet, 2.5, &[53.5]), [false]);
    assert_eq!(ink_at(doc, sheet, 157.5, &[53.5]), [true]);
}

#[test]
fn each_curve_gets_its_own_dashes_and_head() {
    let doc = "curves c = parametric (t): (t, s) for t in -2..2 for s in 1..2";
    let plain = "style paper { fill: white } style c { stroke: black; width: 2px }";
    let dashed = format!("{plain} style c {{ dash: 6px; gap: 6px }}");
    let figure = build(&format!("{VIEW}{doc}"), &[&dashed]).unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    for row in [70.0, 60.0] {
        let gaps = (0..160)
            .filter(|x| (61..99).contains(x) && frame.shade(*x as f64 + 0.5, row).unwrap()[0] > 0.5)
            .count();
        assert!(gaps > 5, "row {row} has {gaps} gaps");
    }
    let headed = format!("{plain} style c {{ head: 12px }}");
    let figure = build(&format!("{VIEW}{doc}"), &[&headed]).unwrap();
    let frame = Frame::new(&figure, &Inputs::at(0.0), 160, 160).unwrap();
    // A head is wider than the line, at the end of each.
    for row in [70.0, 60.0] {
        assert!(frame.shade(90.5, row - 3.0).unwrap()[0] < 0.5, "row {row}");
        assert!(frame.shade(80.5, row - 3.0).unwrap()[0] > 0.5, "row {row}");
    }
}

#[test]
fn a_view_can_fit_all_of_them() {
    let doc = "viewport { fit: c, pad: 0 } style figure { width: 100px; height: 100px }
               param n = 3 in 1..9
               curves c = parametric (t): (t, s) for t in 0..10 for s in 1..n";
    let figure = build(doc, &[]).unwrap();
    let view = |n: f64| {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("n".to_string(), n)],
        };
        frame::view_for(&figure, &inputs, 100, 100).unwrap()
    };
    let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
    let at_3 = view(3.0);
    assert!(close(at_3.x, [0.0, 10.0]), "{:?}", at_3.x);
    // Its bounds are 1 to 3 tall, and it's made square around their middle.
    assert!(close(at_3.y, [-3.0, 7.0]), "{:?}", at_3.y);
    let at_9 = view(9.0);
    assert!(close(at_9.y, [0.0, 10.0]), "{:?}", at_9.y);
}

#[test]
fn what_it_cant_do_is_an_error() {
    let doc = format!("{VIEW}curves c = parametric (t): (t, s) for t in 0..1 for s in 1..2");
    for (sheet, wanted) in [
        ("style c { fill: black }", "made of lines"),
        ("style c { fill_opacity: 0.5 }", "made of lines"),
        ("style c { radius: 2px }", "made of lines"),
        ("style c::inside { fill: black }", "made of lines"),
        ("style c { stroke: rgb(s, 0, 0) }", "unknown name"),
        (
            "style c { stroke: mix(white, black, self.dist) }",
            "self.dist",
        ),
    ] {
        let err = build(&doc, &[sheet]).unwrap_err();
        assert!(err.to_string().contains(wanted), "{sheet}: {err}");
    }
    for (doc, wanted) in [
        ("curves c = (t, 0) for t in 0..1", "parametric"),
        ("curves c = parametric (t): (t, 0) for t in 0..1", "for"),
        (
            "curves c = parametric (t): (t, s) for u in 0..1 for s in 1..2",
            "`t`",
        ),
    ] {
        let err = build(doc, &[]).unwrap_err();
        assert!(err.to_string().contains(wanted), "{doc}: {err}");
    }
}
