//! `±` and `∓` in a collection, and a collection's variable over a list: `for s in {-1, 1}`.
//! A collection with a `±` draws each instance twice, the + branch first, as a last variable
//! over `{1, -1}` that a style reads as `self.sign`.

use pencil_core::eval::{self, Env};
use pencil_core::frame::Inputs;
use pencil_core::ir::{Form, Shape};
use pencil_core::{Figure, build, stroke};

/// The figure's one mark's instances, each as the values of these names, in drawing order.
fn instances(f: &Figure, mark: usize, names: &[&str]) -> Vec<Vec<f64>> {
    let Form::Many { vars, lets, .. } = &f.scene.marks[mark].form else {
        panic!("{} isn't a collection", f.scene.marks[mark].id)
    };
    let mut env = eval::frame_env(f, &Inputs::at(0.0)).unwrap();
    let mut out = Vec::new();
    stroke::for_each(vars, &mut env, &mut 0, &mut |env: &mut Env| {
        for (name, e) in lets {
            let v = eval::real(e, env)?;
            env.set(name, v);
        }
        out.push(names.iter().map(|n| env.get(n).unwrap()).collect());
        Ok(())
    })
    .unwrap();
    out
}

fn dots(doc: &str) -> Vec<Vec<f64>> {
    instances(&build(doc, &[]).unwrap(), 0, &["self.x", "self.y"])
}

#[test]
fn a_sign_with_no_for_is_a_collection_of_two_plus_first() {
    assert_eq!(dots("dots d = (±1, 2)"), [[1.0, 2.0], [-1.0, 2.0]]);
    // `a ± b` too, and a sign can be anywhere in the expression.
    assert_eq!(dots("dots d = (3 ± 1, 0)"), [[4.0, 0.0], [2.0, 0.0]]);
    assert_eq!(dots("dots d = (2·(1 ± 0.5), 0)"), [[3.0, 0.0], [1.0, 0.0]]);
}

#[test]
fn every_sign_in_a_statement_is_the_same_and_minus_plus_is_the_other() {
    // (±a, ±b) is two points, and (±a, ∓b) the other two.
    assert_eq!(dots("dots d = (±1, ±2)"), [[1.0, 2.0], [-1.0, -2.0]]);
    assert_eq!(dots("dots d = (±1, ∓2)"), [[1.0, -2.0], [-1.0, 2.0]]);
    assert_eq!(dots("dots d = (5 ∓ 1, 0)"), [[4.0, 0.0], [6.0, 0.0]]);
    // Only `∓`: the same two branches, in the other order.
    assert_eq!(dots("dots d = (∓1, 0)"), [[-1.0, 0.0], [1.0, 0.0]]);
}

#[test]
fn a_sign_in_a_family_doubles_it_and_varies_fastest() {
    let got = dots("dots d = (k, ±1) for k in 0..2");
    assert_eq!(got.len(), 2 * 3);
    assert_eq!(
        got,
        [
            [0.0, 1.0],
            [0.0, -1.0],
            [1.0, 1.0],
            [1.0, -1.0],
            [2.0, 1.0],
            [2.0, -1.0]
        ]
    );
    // In a `with`, which the drawn point reads.
    let got = dots("dots d = (k, h) for k in 1..2 with h = ±k/2");
    assert_eq!(got, [[1.0, 0.5], [1.0, -0.5], [2.0, 1.0], [2.0, -1.0]]);
    // 2(n + 1) instances for `0..n`.
    let f = build(
        "param n = 4 in 0..9 step 1\ndots d = (k, ±1) for k in 0..n",
        &[],
    )
    .unwrap();
    assert_eq!(instances(&f, 0, &["self.x"]).len(), 10);
}

#[test]
fn a_list_counts_over_its_values_in_the_order_written() {
    assert_eq!(
        dots("dots d = (s, 0) for s in {-1, 1}"),
        [[-1.0, 0.0], [1.0, 0.0]]
    );
    assert_eq!(
        dots("dots d = (s, 0) for s in {3, 1, 2}"),
        [[3.0, 0.0], [1.0, 0.0], [2.0, 0.0]]
    );
    // Beside a range, every combination, the later variable fastest.
    assert_eq!(
        dots("dots d = (i, s) for i in 0..1, s in {-1, 1}"),
        [[0.0, -1.0], [0.0, 1.0], [1.0, -1.0], [1.0, 1.0]]
    );
    assert_eq!(
        dots("dots d = (i, s) for s in {-1, 1}, i in 0..1"),
        [[0.0, -1.0], [1.0, -1.0], [0.0, 1.0], [1.0, 1.0]]
    );
    // A later range can read a list's value, a number or a point's part.
    assert_eq!(
        dots("dots d = (k, s) for s in {1, 2}, k in 0..s"),
        [[0.0, 1.0], [1.0, 1.0], [0.0, 2.0], [1.0, 2.0], [2.0, 2.0]]
    );
    assert_eq!(
        dots("dots d = (k, p.y) for p in {(0, 0), (0, 2)}, k in 0..p.y"),
        [[0.0, 0.0], [0.0, 2.0], [1.0, 2.0], [2.0, 2.0]]
    );
    // A value can read an earlier variable.
    assert_eq!(
        dots("dots d = (i, s) for i in 1..2, s in {i, -i}"),
        [[1.0, 1.0], [1.0, -1.0], [2.0, 2.0], [2.0, -2.0]]
    );
    // Points, with numbers among them as points on the real axis, as a choice's options are.
    assert_eq!(
        dots("dots d = p for p in {1, i, -1 + 2·i}"),
        [[1.0, 0.0], [0.0, 1.0], [-1.0, 2.0]]
    );
    // A list and a sign together.
    assert_eq!(
        dots("dots d = (s, ±1) for s in {5, 6}"),
        [[5.0, 1.0], [5.0, -1.0], [6.0, 1.0], [6.0, -1.0]]
    );
}

#[test]
fn a_list_needs_values_of_one_kind_that_a_collection_can_count() {
    let err = build("dots d = (s, 0) for s in {}", &[]).unwrap_err();
    assert_eq!((err.line, err.col), (1, 27));
    assert!(err.message.contains("`s` counts over nothing"), "{err}");
    // A range is outside the braces.
    let err = build("dots d = (s, 0) for s in {1..3}", &[]).unwrap_err();
    assert_eq!((err.line, err.col), (1, 28));
    assert_eq!(
        err.message,
        "a list's elements are values, written one by one: `s in {1, 2, 3}`, and a range goes outside the braces: `s in 1..3`"
    );
    let err = build("dots d = (s, 0) for s in {0, 2..3}", &[]).unwrap_err();
    assert!(
        err.message.contains("a range goes outside the braces"),
        "{err}"
    );
    let err = build("dots d = (0, 0) for s in {1, circle(0, 1)}", &[]).unwrap_err();
    assert!(err.message.contains("of one kind"), "{err}");
}

#[test]
fn both_branches_of_a_hyperbola_are_on_it() {
    let f = build(
        "curves hyp = parametric (t): (±cosh(t), sinh(t)) for t in -2..2",
        &[],
    )
    .unwrap();
    let Form::Many {
        vars,
        lets,
        shape: Shape::Curves(arc),
    } = &f.scene.marks[0].form
    else {
        panic!()
    };
    let mut env = eval::frame_env(&f, &Inputs::at(0.0)).unwrap();
    let mut signs = Vec::new();
    stroke::for_each(vars, &mut env, &mut 0, &mut |env: &mut Env| {
        for (name, e) in lets {
            let v = eval::real(e, env)?;
            env.set(name, v);
        }
        let sign = env.get("self.sign").unwrap();
        signs.push(sign);
        for k in 0..=40 {
            let t = -2.0 + 4.0 * k as f64 / 40.0;
            env.set(&arc.var, t);
            let (x, y) = (
                eval::real(&arc.point[0], env)?,
                eval::real(&arc.point[1], env)?,
            );
            assert!((x - sign * t.cosh()).abs() < 1e-12, "x at t = {t}: {x}");
            assert!((y - t.sinh()).abs() < 1e-12, "y at t = {t}: {y}");
            // Each point is on x² − y² = 1, on its own side.
            assert!((x * x - y * y - 1.0).abs() < 1e-9 && x * sign > 0.0);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(signs, [1.0, -1.0]);
}

#[test]
fn a_style_reads_the_sign() {
    let doc = "curves hyp = parametric (t): (±cosh(t), sinh(t)) for t in -2..2";
    let f = build(doc, &["style hyp { width: 2px + self.sign·1px }"]).unwrap();
    let width = f.styles.marks[0].width.clone().expect("a width");
    let Form::Many { vars, lets, .. } = &f.scene.marks[0].form else {
        panic!()
    };
    let mut env = eval::frame_env(&f, &Inputs::at(0.0)).unwrap();
    let mut widths = Vec::new();
    stroke::for_each(vars, &mut env, &mut 0, &mut |env: &mut Env| {
        for (name, e) in lets {
            let v = eval::real(e, env)?;
            env.set(name, v);
        }
        widths.push(eval::real(&width, env)?);
        Ok(())
    })
    .unwrap();
    assert_eq!(widths, [3.0, 1.0]);
    // A collection with no sign has none to read.
    let err = build(
        "dots d = (k, 0) for k in 0..2",
        &["style d { radius: 2px + self.sign·1px }"],
    )
    .unwrap_err();
    assert!(err.message.contains("self.sign"), "{err}");
}

#[test]
fn a_sign_anywhere_but_a_collection_is_refused_at_the_sign() {
    let refused = |doc: &str, sheet: Option<&str>, at: (usize, usize), plural: &str| {
        let sheets: Vec<&str> = sheet.into_iter().collect();
        let err = build(doc, &sheets).unwrap_err();
        assert_eq!((err.line, err.col), at, "{doc}: {err}");
        assert!(
            err.message
                .starts_with("`±` draws two marks, one for each sign, so it belongs in"),
            "{doc}: {err}"
        );
        assert!(err.message.contains(plural), "{doc}: {err}");
    };
    let any = "a collection: `dots` for points, `circles` for circles, `curves` for curves";
    refused("let a = ±1", None, (1, 9), any);
    refused("let a = 2 ∓ 1", None, (1, 11), any);
    refused(
        "curve c = parametric (t): (±cosh(t), sinh(t)) for t in -2..2",
        None,
        (1, 28),
        "a `curves` collection",
    );
    refused(
        "curve c = circle((±1, 0), 1)",
        None,
        (1, 19),
        "or `circles` for a circle",
    );
    refused("param a = ±1 in -2..2", None, (1, 11), any);
    refused("param a = 0 in -2..2\nbind a = ±time", None, (2, 10), any);
    refused(
        "param a = 0 in -2..2\nlet P = (a, 0)\ndot P\nhandle P sets a = ±pointer.x",
        None,
        (4, 19),
        any,
    );
    refused("fn f(t) = ±t", None, (1, 11), any);
    refused(
        "let P = (0, 0)\ndot P\nstyle P { radius: 2px ± 1px }",
        None,
        (3, 23),
        any,
    );
    refused(
        "let P = (0, 0)\ndot P",
        Some("style P { radius: 2px ± 1px }"),
        (1, 23),
        any,
    );
    // Inside a block, the statement with the sign is the one that's refused.
    refused("beat \"one\" {\n  let a = ±1\n}", None, (2, 11), any);
    // What a collection counts over can't be signed: the sign is its last variable.
    let err = build("dots d = (k, 0) for k in 0..±2", &[]).unwrap_err();
    assert_eq!((err.line, err.col), (1, 29));
    assert!(
        err.message
            .starts_with("`±` in an end of a range would make two ranges"),
        "{err}"
    );
    let err = build("dots d = (s, 0) for s in {±1}", &[]).unwrap_err();
    assert_eq!((err.line, err.col), (1, 27));
    assert!(
        err.message.contains("`s` can't count over `±`") && err.message.contains("`s in {1, -1}`"),
        "{err}"
    );
    // A plural statement still needs a `for` when it has no sign.
    let err = build("dots d = (1, 0)", &[]).unwrap_err();
    assert!(err.message.contains("expected `for`"), "{err}");
}

#[test]
fn a_sign_in_a_labels_text_is_text() {
    let f = build(
        "let P = (0, 0)\ndot P\nlabel l = \"±1 and a ∓ b\" at P",
        &[],
    )
    .unwrap();
    let Form::Label { text, .. } = &f.scene.marks[1].form else {
        panic!()
    };
    assert_eq!(text, "±1 and a ∓ b");
}

/// The range of a figure's one curve, a region's arc or a collection's curves, as numbers.
fn range_of(f: &Figure, mark: usize) -> [f64; 2] {
    let arc = match &f.scene.marks[mark].form {
        Form::Parametric(arc) => arc,
        Form::Many {
            shape: Shape::Curves(arc),
            ..
        } => arc,
        Form::Region(parts) => match &parts[1] {
            pencil_core::ir::Part::Arc(arc) => arc,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let env = eval::frame_env(f, &Inputs::at(0.0)).unwrap();
    arc.range.clone().map(|e| eval::real(&e, &env).unwrap())
}

#[test]
fn med_plus_minus_dev_is_a_range_wherever_a_range_is() {
    use std::f64::consts::PI;
    // A collection's variable: whole numbers from 1 to 3, and no branches.
    let f = build("dots d = (k, 0) for k in 2 ± 1", &[]).unwrap();
    assert_eq!(instances(&f, 0, &["self.x"]), [[1.0], [2.0], [3.0]]);
    let Form::Many { vars, lets, .. } = &f.scene.marks[0].form else {
        panic!()
    };
    assert_eq!(vars.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["k"]);
    assert!(lets.iter().all(|(n, _)| n != "self.sign"));
    // A curve's parameter, a region's, and a collection of curves'.
    let f = build("curve c = parametric (t): (t, 0) for t in 0 ± π", &[]).unwrap();
    assert_eq!(range_of(&f, 0), [-PI, PI]);
    let f = build("region r = (0, 0), (cos(t), sin(t)) for t in 1 ± 0.5", &[]).unwrap();
    assert_eq!(range_of(&f, 0), [0.5, 1.5]);
    let f = build(
        "curves w = parametric (t): (t, k) for t in 0 ± 1 for k in 0..1",
        &[],
    )
    .unwrap();
    assert_eq!(range_of(&f, 0), [-1.0, 1.0]);
    // A param's range, with a step after it, and one whose middle is another param.
    let f = build("param a = 1 in 1 ± 0.5 step 0.25", &[]).unwrap();
    let a = &f.scene.params[0];
    assert_eq!((a.range, a.step), ([0.5, 1.5], Some(0.25)));
    let f = build("param q = 1 in 0..2\nparam a = 1 in q ± 0.5", &[]).unwrap();
    let env = eval::frame_env(&f, &Inputs::at(0.0)).unwrap();
    let a = f.scene.params.iter().find(|p| p.id == "a").unwrap();
    assert_eq!(eval::param_range(a, &env).unwrap(), [0.5, 1.5]);
    // A viewport's axes.
    let f = build("viewport { x: 0 ± 2, y: 1 ± 1 }", &[]).unwrap();
    assert_eq!(
        (f.scene.viewport.x, f.scene.viewport.y),
        ([-2.0, 2.0], [0.0, 2.0])
    );
}

#[test]
fn a_negative_dev_is_the_same_interval_never_reversed() {
    // A constant.
    let f = build("param a = 1 in 1 ± -0.5", &[]).unwrap();
    assert_eq!(f.scene.params[0].range, [0.5, 1.5]);
    assert_eq!(
        dots("dots d = (k, 0) for k in 2 ± -1"),
        [[1.0, 0.0], [2.0, 0.0], [3.0, 0.0]]
    );
    let f = build("viewport { x: 0 ± -2, y: 1 ± -1 }", &[]).unwrap();
    assert_eq!(
        (f.scene.viewport.x, f.scene.viewport.y),
        ([-2.0, 2.0], [0.0, 2.0])
    );
    // A param that passes through zero: the range narrows to its middle and opens again, and
    // what was held comes back.
    let f = build("param d = -0.5 in -1..1\nparam p = 1.2 in 1 ± d", &[]).unwrap();
    let p = f.scene.params.iter().find(|p| p.id == "p").unwrap();
    assert_eq!(p.range, [0.0, 2.0]);
    for (d, range, value) in [
        (-0.5, [0.5, 1.5], 1.2),
        (-0.1, [0.9, 1.1], 1.1),
        (0.0, [1.0, 1.0], 1.0),
        (0.1, [0.9, 1.1], 1.1),
        (0.5, [0.5, 1.5], 1.2),
    ] {
        let inputs = Inputs {
            time: 0.0,
            params: vec![("d".into(), d), ("p".into(), 1.2)],
        };
        let env = eval::frame_env(&f, &inputs).unwrap();
        let [lo, hi] = eval::param_range(p, &env).unwrap();
        assert!(
            (lo - range[0]).abs() < 1e-12 && (hi - range[1]).abs() < 1e-12,
            "d = {d}: {lo}..{hi}"
        );
        assert!((env.get("p").unwrap() - value).abs() < 1e-12, "d = {d}");
    }
    // A collection counts the same either side of zero.
    for d in [-1.0, 1.0] {
        let doc = format!("param d = {d} in -1..1\ndots c = (k, 0) for k in 0 ± d");
        assert_eq!(dots(&doc), [[-1.0, 0.0], [0.0, 0.0], [1.0, 0.0]], "d = {d}");
    }
    assert_eq!(
        dots("param d = 0 in -1..1\ndots c = (k, 0) for k in 0 ± d"),
        [[0.0, 0.0]]
    );
}

#[test]
fn a_range_and_two_branches_in_one_statement_keep_to_themselves() {
    // The range is 1..3, and the `±` in what it draws makes two of each, the sign fastest.
    let f = build("dots d = (k, ±1) for k in 2 ± 1", &[]).unwrap();
    assert_eq!(
        instances(&f, 0, &["self.x", "self.y"]),
        [
            [1.0, 1.0],
            [1.0, -1.0],
            [2.0, 1.0],
            [2.0, -1.0],
            [3.0, 1.0],
            [3.0, -1.0]
        ]
    );
    let Form::Many { vars, .. } = &f.scene.marks[0].form else {
        panic!()
    };
    assert_eq!(
        vars.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
        ["k", "self.sign.index"]
    );
    // A curve with both: two curves, each over -1..1.
    let f = build(
        "curves h = parametric (t): (±cosh(t), sinh(t)) for t in 0 ± 1",
        &[],
    )
    .unwrap();
    assert_eq!(range_of(&f, 0), [-1.0, 1.0]);
    assert_eq!(instances(&f, 0, &["self.sign"]), [[1.0], [-1.0]]);
}

#[test]
fn a_range_takes_one_med_plus_minus_dev_and_nothing_else_signed() {
    let fails = |doc: &str, at: (usize, usize), message: &str| {
        let err = build(doc, &[]).unwrap_err();
        assert_eq!((err.line, err.col), at, "{doc}: {err}");
        assert_eq!(err.message, message, "{doc}");
    };
    let alone = "`±x` alone is two values, +x and -x, not a range: for the two values write `{x, -x}`, and for the interval `-x..x` or `0 ± x`";
    fails("dots d = (k, 0) for k in ±2", (1, 26), alone);
    fails("param a in ±1", (1, 12), alone);
    let minus_plus =
        "`∓` isn't a range: an interval about a middle is `med ± dev`, as `2 ± 1`, or `a..b`";
    fails("dots d = (k, 0) for k in 2 ∓ 1", (1, 28), minus_plus);
    fails("viewport { x: ∓2, y: -2..2 }", (1, 15), minus_plus);
    let one = "a range is `a..b`, or one `med ± dev` as the whole of it, as `2 ± 1`";
    fails("dots d = (k, 0) for k in 2 ± 1 ± 1", (1, 28), one);
    let end = "`±` in an end of a range would make two ranges. For an interval about a middle, the whole range is `med ± dev`, as `2 ± 1`; for both signs, count over them first: `for s in {1, -1}, k in 0..(2 + s)`";
    fails("dots d = (k, 0) for k in 0..(2 ± 1)", (1, 32), end);
    fails("param a in 0..(2 ± 1)", (1, 18), end);
    fails(
        "curve c = parametric (t): (t, 0) for t in 1 ± 1..2",
        (1, 45),
        end,
    );
}

#[test]
fn a_variable_named_sign_beside_a_sign_is_refused() {
    let message = |kind: &str| {
        format!(
            "`d` has a `±`, whose branch a style reads as `self.sign`, and {kind} named `sign`, which `self.sign` could be taken for: rename `sign`"
        )
    };
    for (doc, kind) in [
        ("dots d = (sign, ±1) for sign in 0..1", "a variable"),
        ("dots d = (sign, ±1) for sign in {1, 2}", "a variable"),
        (
            "dots d = (k, ±sign) for k in 0..1 with sign = 2",
            "a `with`",
        ),
    ] {
        let err = build(doc, &[]).unwrap_err();
        assert_eq!((err.line, err.col), (1, 1), "{doc}");
        assert_eq!(err.message, message(kind), "{doc}");
    }
    // Without a `±`, `sign` is any other name, and so is it with a range written `med ± dev`.
    assert_eq!(
        dots("dots d = (sign, 0) for sign in {1, 2}"),
        [[1.0, 0.0], [2.0, 0.0]]
    );
    assert_eq!(dots("dots d = (sign, 0) for sign in 1 ± 1").len(), 3);
}
