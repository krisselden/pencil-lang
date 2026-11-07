//! Which marks exist on a frame. Geometry is partial: two curves can fail to meet, so a
//! value can be absent, and anything built from an absent value is absent too. `Where`
//! is the only source of absence, and `Exists` the only thing that can see it.
//!
//! Absence is decided on the CPU, with the bindings. A mark that depends on something
//! absent draws nothing, and the shader learns that from a flag.

use std::collections::HashSet;

use crate::error::Error;
use crate::eval::{self, Env};
use crate::ir::{Expr, Form, Mark, Part, Scene, Shape};
use crate::style::{MarkStyle, Styled};

/// Whether an expression could be absent, given the names that could be. It errs toward
/// yes: a mark flagged here only costs a test, and `present` makes the real decision.
fn may_vanish(e: &Expr, vanishing: &HashSet<String>) -> bool {
    match e {
        Expr::Num(_) => false,
        Expr::Sym(s) => vanishing.contains(s),
        // A system might not settle on a solution.
        Expr::App(head, _) if head == "Where" || head == "Solved" => true,
        Expr::App(head, _) if head == "Exists" => false,
        Expr::App(_, args) => args.iter().any(|a| may_vanish(a, vanishing)),
    }
}

fn form_exprs(form: &Form) -> Vec<&Expr> {
    let mut out = Vec::new();
    match form {
        Form::Field {
            field,
            grad,
            curvature,
            ..
        } => {
            out.push(field);
            out.extend(grad);
            out.extend(curvature);
        }
        Form::Parametric(arc) => {
            out.extend(&arc.point);
            out.extend(&arc.range);
        }
        Form::Polyline { points, .. } => out.extend(points.iter().flatten()),
        Form::Region(parts) => {
            for part in parts {
                match part {
                    Part::Point(p) => out.extend(p),
                    Part::Arc(arc) => {
                        out.extend(&arc.point);
                        out.extend(&arc.range);
                    }
                }
            }
        }
        Form::Dot(at) => out.extend(at),
        Form::Arrow { from, to } => {
            out.extend(from);
            out.extend(to);
        }
        Form::Many { vars, lets, shape } => {
            out.extend(vars.iter().flat_map(|(_, range)| range));
            out.extend(lets.iter().map(|(_, e)| e));
            match shape {
                Shape::Dots(at) => out.extend(at),
                Shape::Circles { center, r } => {
                    out.extend(center);
                    out.push(r);
                }
                Shape::Curves(arc) => {
                    out.extend(&arc.point);
                    out.extend(&arc.range);
                }
                Shape::Cycles { a, b, c } => {
                    out.push(a);
                    out.extend(b);
                    out.push(c);
                }
            }
        }
        // Where it sits is the expression's business, per state.
        Form::Term { .. } => {}
        Form::Outline { .. } => {}
        Form::Label {
            at,
            offset,
            away,
            values,
            ..
        } => {
            out.extend(at);
            out.extend(offset);
            out.extend(away.iter().flatten());
            out.extend(values);
        }
    }
    out
}

/// One entry per mark: whether it can ever be absent. The paper can't be, since it's what
/// everything else is drawn on, so a paper that depends on a partial value is an error.
pub fn conditional_marks(scene: &Scene, styles: &Styled) -> Result<Vec<bool>, Error> {
    let mut vanishing = HashSet::new();
    for binding in &scene.bindings {
        if may_vanish(&binding.expr, &vanishing) {
            vanishing.insert(binding.id.clone());
        }
    }
    if let Some(paper) = &styles.paper
        && may_vanish(paper, &vanishing)
    {
        return Err(Error::new(
            "`paper` can't depend on something that may not exist",
        ));
    }
    Ok(scene
        .marks
        .iter()
        .zip(&styles.marks)
        .map(|(mark, style)| {
            style.show.is_some()
                || form_exprs(&mark.form)
                    .into_iter()
                    .chain(style.exprs())
                    .any(|e| may_vanish(e, &vanishing))
        })
        .collect())
}

fn names_exist(e: &Expr, env: &Env) -> bool {
    let mut exist = true;
    e.visit_syms(&mut |s| exist &= !env.is_absent(s));
    exist
}

/// Whether an expression has a value. Its names don't settle this for one that picks
/// between alternatives, so these are evaluated.
fn evaluates(e: &Expr, env: &Env) -> Result<bool, Error> {
    match eval::real(e, env) {
        Ok(_) => Ok(true),
        Err(e) if e.absent => Ok(false),
        Err(e) => Err(e),
    }
}

fn all_evaluate<'a>(mut exprs: impl Iterator<Item = &'a Expr>, env: &Env) -> Result<bool, Error> {
    exprs.try_fold(true, |all, e| Ok(all && evaluates(e, env)?))
}

/// Whether a mark exists on this frame. A parametric curve or region arc is also checked
/// sample by sample when it's drawn, since its point can be absent over part of its range.
pub fn present(mark: &Mark, style: &MarkStyle, env: &Env) -> Result<bool, Error> {
    // Styles and fields hold plain names and arithmetic, and can use coordinates and
    // `self`, which have no value here.
    if !style.exprs().all(|e| names_exist(e, env)) {
        return Ok(false);
    }
    if let Some(show) = &style.show
        && eval::real(show, env)? <= 0.0
    {
        return Ok(false);
    }
    match &mark.form {
        Form::Field { field, grad, .. } => {
            Ok(names_exist(field, env) && grad.iter().all(|g| names_exist(g, env)))
        }
        Form::Parametric(arc) => all_evaluate(arc.range.iter(), env),
        Form::Polyline { points, .. } => all_evaluate(points.iter().flatten(), env),
        Form::Region(parts) => {
            for part in parts {
                let exists = match part {
                    Part::Point(p) => all_evaluate(p.iter(), env)?,
                    Part::Arc(arc) => all_evaluate(arc.range.iter(), env)?,
                };
                if !exists {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Form::Dot(at) => all_evaluate(at.iter(), env),
        Form::Arrow { from, to } => all_evaluate(from.iter().chain(to), env),
        // These find out as they go, per instance and per state.
        Form::Many { .. } | Form::Term { .. } => Ok(true),
        Form::Outline { .. } => Ok(true),
        Form::Label {
            at,
            offset,
            away,
            values,
            ..
        } => all_evaluate(
            at.iter()
                .chain(offset)
                .chain(away.iter().flatten())
                .chain(values),
            env,
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::build;
    use crate::eval::Frame;
    use crate::frame::{Inputs, drag_value, handle_at, prepare, view_for};
    use crate::wgsl;

    /// `a` moves a line up past a unit circle: it meets it for `a` up to 1.
    const SWEEP: &str = "
        viewport { x: -2..2, y: -2..2 }
        param a = 0 in 0..2
        curve c = circle((0, 0), (1, 0))
        curve l = line((-2, a), (2, a))
        let P = intersect(c, l, 0)
        let Q = intersect(c, l, 1)
        dot P, Q
        segment chord = P, Q
        label name = \"P\" at P
        curve through = circle(P, (0, 0))
        // Always there, and drawn after everything that can vanish.
        segment base = (-2, -1.5), (2, -1.5)";
    const INK: &str = "
        style chord, base { stroke: black; width: 4px }
        style through { stroke: black; width: 2px }
        style P, Q { fill: black; radius: 6px }";

    fn with(a: f64) -> Inputs {
        Inputs {
            time: 0.0,
            params: vec![("a".to_string(), a)],
        }
    }

    fn shown(f: &crate::Figure, a: f64) -> Vec<(String, bool)> {
        let p = prepare(f, &with(a), 100, 100, 1.0).unwrap();
        f.scene
            .marks
            .iter()
            .map(|m| m.id.clone())
            .zip(p.shown)
            .collect()
    }

    #[test]
    fn what_depends_on_an_absent_value_is_not_drawn() {
        let f = build(SWEEP, &[INK]).unwrap();
        let present: Vec<bool> = shown(&f, 0.5).iter().map(|(_, s)| *s).collect();
        assert!(present.iter().all(|s| *s));
        for (id, is_shown) in shown(&f, 1.5) {
            let survives = matches!(id.as_str(), "c" | "l" | "base");
            assert_eq!(is_shown, survives, "{id}");
        }
    }

    #[test]
    fn an_absent_label_is_left_out() {
        let f = build(SWEEP, &[INK]).unwrap();
        assert_eq!(
            prepare(&f, &with(0.5), 100, 100, 1.0).unwrap().labels.len(),
            1
        );
        assert!(
            prepare(&f, &with(1.5), 100, 100, 1.0)
                .unwrap()
                .labels
                .is_empty()
        );
    }

    /// An absent stroke keeps its layer, or every later mark would read the wrong one.
    #[test]
    fn later_marks_keep_their_layers() {
        let f = build(SWEEP, &[INK]).unwrap();
        let p = prepare(&f, &with(1.5), 100, 100, 1.0).unwrap();
        assert_eq!(p.strokes.len(), 2);
        assert!(p.strokes[0].is_empty());
        assert!(!p.strokes[1].is_empty());
        let frame = Frame::new(&f, &with(1.5), 100, 100).unwrap();
        // World y = −1.5 is row 87.5, and the chord's row is empty.
        assert_eq!(frame.shade(50.5, 87.5).unwrap(), [0.0; 3]);
        assert_eq!(frame.shade(50.5, 12.5).unwrap(), [1.0; 3]);
    }

    /// No NaN reaches a shader or a pixel, however the frame turns out.
    #[test]
    fn absent_values_reach_the_gpu_as_zero_and_a_flag() {
        let f = build(SWEEP, &[INK]).unwrap();
        let p = prepare(&f, &with(1.5), 100, 100, 1.0).unwrap();
        assert!(p.values.iter().all(|v| v.is_finite()));
        for (mark, slot) in f.presence_slots() {
            assert_eq!(p.values[slot], f32::from(u8::from(p.shown[mark])));
        }
        let source = wgsl::emit(&f);
        assert!(source.contains("if u.values["), "{source}");
    }

    #[test]
    fn a_mark_that_cannot_vanish_gets_no_flag() {
        let f = build(
            "curve c = circle((0, 0), (1, 0))\nlet P = (1, 1)\ndot P",
            &[],
        )
        .unwrap();
        assert_eq!(f.presence_slots().count(), 0);
        assert!(!wgsl::emit(&f).contains("if u.values["));
    }

    #[test]
    fn a_style_that_reads_an_absent_value_hides_its_mark() {
        let doc = format!("{SWEEP}\nsegment other = (0, 0), (1, 1)");
        let sheet = format!("{INK}\nstyle other {{ stroke: black; opacity: P.x + 1 }}");
        let f = build(&doc, &[&sheet]).unwrap();
        let of = |a: f64| shown(&f, a).iter().find(|(id, _)| id == "other").unwrap().1;
        assert!(of(0.5));
        assert!(!of(1.5));
    }

    #[test]
    fn exists_lets_a_style_respond_without_vanishing() {
        let doc = format!("{SWEEP}\nlet meets = exists(P)\nsegment other = (0, 0), (1, 1)");
        let sheet = format!("{INK}\nstyle other {{ stroke: black; opacity: meets }}");
        let f = build(&doc, &[&sheet]).unwrap();
        let other = shown(&f, 1.5);
        assert!(other.iter().find(|(id, _)| id == "other").unwrap().1);
    }

    #[test]
    fn the_paper_cannot_depend_on_something_that_may_not_exist() {
        let err = build(SWEEP, &["style paper { fill: rgb(P.x, 0, 0) }"]).unwrap_err();
        assert!(err.message.contains("`paper`"), "{err}");
    }

    #[test]
    fn a_fit_view_skips_what_does_not_exist() {
        let doc = "viewport { x: -9..9, y: -9..9, fit: c P Q, pad: 0 }
            param a = 0 in 0..2
            curve c = circle((0, 0), (1, 0))
            curve far = circle((5, 0), (5, 1))
            curve l = line((-2, a), (2, a))
            let P = intersect(far, l, 0)
            let Q = intersect(c, l, 1)";
        let f = build(doc, &[]).unwrap();
        // With a = 1.5 the line meets only the far circle at 5 ± √(1 − 2.25): nothing.
        let v = view_for(&f, &with(1.5), 100, 100).unwrap();
        assert_eq!(v.x, [-1.0, 1.0]);
        let v = view_for(&f, &with(0.0), 100, 100).unwrap();
        assert_eq!(v.x, [-1.0, 4.0]);
    }

    #[test]
    fn a_fit_with_nothing_left_keeps_the_declared_window() {
        let doc = "viewport { x: -9..9, y: -9..9, fit: P }
            param a = 3 in 0..3
            let P = intersect(circle((0, 0), (1, 0)), line((-2, a), (2, a)))";
        let f = build(doc, &[]).unwrap();
        assert_eq!(
            view_for(&f, &Inputs::at(0.0), 100, 100).unwrap().x,
            [-9.0, 9.0]
        );
    }

    #[test]
    fn an_absent_handle_cannot_be_grabbed() {
        let doc = format!("{SWEEP}\nhandle P sets a = pointer.y");
        let f = build(&doc, &[]).unwrap();
        let at = |a: f64, x: f64, y: f64| handle_at(&f, &with(a), 100, 100, x, y, 500.0).unwrap();
        assert_eq!(at(0.5, 50.0, 50.0), Some(0));
        assert_eq!(at(1.5, 50.0, 50.0), None);
    }

    #[test]
    fn a_drag_with_no_answer_leaves_its_param() {
        let doc = "viewport { x: -2..2, y: -2..2 }
            param a = 0.25 in 0..1
            curve c = circle((0, 0), (1, 0))
            let P = (a, 0)
            dot P
            let reach = intersect(c, line((-2, 3), (2, 3)))
            handle P sets a = reach.x";
        let f = build(doc, &[]).unwrap();
        let (_, value) = drag_value(&f, &Inputs::at(0.0), 0, 100, 100, 75.0, 50.0).unwrap();
        assert_eq!(value, 0.25);
    }

    #[test]
    fn a_parametric_curve_has_gaps_where_its_point_does_not_exist() {
        // A vertical line at x = t meets the unit circle for t within ±1.
        let doc = "viewport { x: -2..2, y: -2..2 }
            curve c = circle((0, 0), (1, 0))
            curve u = parametric (t): intersect(c, line((t, 0), (t, 1)), 1) for t in -2..2";
        let f = build(doc, &["style u { stroke: black; width: 2px }"]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
        assert!(p.shown[1]);
        let segments = p.strokes[0].chunks(crate::stroke::FLOATS_PER_SEGMENT);
        let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
        for s in segments {
            left = left.min(s[0]).min(s[2]);
            right = right.max(s[0]).max(s[2]);
        }
        // World ±1 are pixels 100 and 300, and the bisection gets right up to them.
        assert!((left - 100.0).abs() < 0.01, "{left}");
        assert!((right - 300.0).abs() < 0.01, "{right}");
    }

    #[test]
    fn a_curve_can_come_in_pieces() {
        // x = 2·sin(t) is within ±1 near t = 0, π and 2π.
        let doc = "viewport { x: -2..2, y: -2..2 }
            curve c = circle((0, 0), (1, 0))
            curve u = parametric (t): intersect(c, line((2·sin(t), 0), (2·sin(t), 1)), 1) for t in 0..tau";
        let f = build(doc, &[]).unwrap();
        let crate::ir::Form::Parametric(arc) = &f.scene.marks[1].form else {
            panic!();
        };
        let env = crate::eval::frame_env(&f, &Inputs::at(0.0)).unwrap();
        let sampled = crate::stroke::sample_arc(arc, &env, &f.scene.viewport, 400, 400).unwrap();
        assert_eq!(sampled.pieces.len(), 3);
        assert!(!sampled.complete);
    }

    #[test]
    fn show_hides_a_mark_and_can_read_the_view() {
        let doc = "viewport { fit: c, pad: 0 }
            param r = 1 in 0..9
            curve c = circle((0, 0), (r, 0))
            segment big = (0, 0), (1, 1)
            label near = \"N\" at (0, 0)";
        // Only shown while zoomed in: while the view is under 4 world units wide.
        let sheet = "style near, big { show: 4 - view.width }
                     style big { stroke: black }";
        let f = build(doc, &[sheet]).unwrap();
        let at = |r: f64| {
            prepare(
                &f,
                &{
                    Inputs {
                        time: 0.0,
                        params: vec![("r".to_string(), r)],
                    }
                },
                100,
                100,
                1.0,
            )
            .unwrap()
        };
        let close = at(1.0);
        assert_eq!(close.labels.len(), 1);
        assert!(close.shown[1] && close.shown[2]);
        let wide = at(3.0);
        assert!(wide.labels.is_empty());
        assert!(!wide.shown[1]);
        // It goes through the same flag as anything else that isn't there.
        assert!(wgsl::emit(&f).contains("if u.values["));
        // And `view.width` isn't for anything that runs per pixel.
        let err = build(doc, &["style big { opacity: view.width }"]).unwrap_err();
        assert!(err.message.contains("unknown name"), "{err}");
    }

    #[test]
    fn a_region_with_a_gap_in_its_boundary_does_not_exist() {
        let doc = "viewport { x: -2..2, y: -2..2 }
            param a = 1 in 0..3
            curve c = circle((0, 0), (1, 0))
            region r = (0, -1), intersect(c, line((a·t, 0), (a·t, 1)), 1) for t in -1..1";
        let f = build(doc, &["style r { fill: black }"]).unwrap();
        let p = prepare(&f, &with(1.0), 100, 100, 1.0).unwrap();
        assert!(p.shown[1]);
        let p = prepare(&f, &with(2.0), 100, 100, 1.0).unwrap();
        assert!(!p.shown[1]);
        assert_eq!(p.geometry[1], 0.0);
        let frame = Frame::new(&f, &with(2.0), 100, 100).unwrap();
        assert_eq!(frame.shade(50.5, 40.5).unwrap(), [1.0; 3]);
    }
}
