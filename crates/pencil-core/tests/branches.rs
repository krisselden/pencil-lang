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
    assert!(err.message.contains("`k` can't count over `±`"), "{err}");
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
