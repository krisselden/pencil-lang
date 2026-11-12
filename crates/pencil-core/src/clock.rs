//! What time drives in a figure, so a host knows whether it needs a clock at all, and whether
//! stepping a param means seeking time or setting the param.

use std::collections::HashSet;

use crate::compile::SIGNALS;
use crate::ir::{Arc, Expr, Form, Scene, Shape};
use crate::style::{CaptionStyle, MarkStyle, PanelStyle, Styled};
use crate::{Composition, Figure};

const TIME: &str = SIGNALS[0];

/// The bindings whose values change with time, directly or through the bindings they read, in
/// the scene's order. A param among them follows time until a reader takes it over.
pub fn time_driven(scene: &Scene) -> Vec<String> {
    let mut driven: HashSet<&str> = HashSet::from([TIME]);
    let mut found = Vec::new();
    // Bindings come in the order they're evaluated, so each reads only those before it.
    for binding in &scene.bindings {
        let mut reads = false;
        binding.expr.visit_syms(&mut |s| reads |= driven.contains(s));
        if let Expr::App(head, args) = &binding.expr
            && head == "Solved"
            && let Some(Expr::Num(system)) = args.first()
        {
            let depends = &scene.systems[*system as usize].depends;
            reads |= depends.iter().any(|d| driven.contains(d.as_str()));
        }
        if reads {
            driven.insert(&binding.id);
            found.push(binding.id.clone());
        }
    }
    found
}

/// Whether anything in the figure changes with time: a binding time drives, or `time` read
/// directly by a mark, a style or the paper. A figure where nothing does needs no clock.
pub fn uses_time(figure: &Figure) -> bool {
    // A binding that time drives is there to be read, so it's taken to be.
    if !time_driven(&figure.scene).is_empty() {
        return true;
    }
    let mut found = false;
    let mut look = |e: &Expr| e.visit_syms(&mut |s| found |= s == TIME);
    scene_exprs(&figure.scene, &mut look);
    styled_exprs(&figure.styles, &mut look);
    found
}

/// Whether any panel of a composition changes with time.
pub fn composition_uses_time(composition: &Composition) -> bool {
    composition.panels.iter().any(|p| uses_time(&p.figure))
}

fn pair(p: &[Expr; 2], f: &mut impl FnMut(&Expr)) {
    p.iter().for_each(&mut *f);
}

fn arc(a: &Arc, f: &mut impl FnMut(&Expr)) {
    pair(&a.point, f);
    pair(&a.range, f);
}

/// Every expression a scene draws from, outside its bindings.
fn scene_exprs(scene: &Scene, f: &mut impl FnMut(&Expr)) {
    for mark in &scene.marks {
        mark.life.iter().chain(&mark.focus).for_each(&mut *f);
        match &mark.form {
            Form::Field {
                field,
                grad,
                curvature,
                line,
                ..
            } => {
                f(field);
                pair(grad, f);
                curvature.iter().for_each(&mut *f);
                line.iter().for_each(|a| arc(a, f));
            }
            Form::Parametric(a) => arc(a, f),
            Form::Polyline { points, .. } => points.iter().for_each(|p| pair(p, f)),
            Form::Region(parts) => {
                for part in parts {
                    match part {
                        crate::ir::Part::Point(p) => pair(p, f),
                        crate::ir::Part::Arc(a) => arc(a, f),
                    }
                }
            }
            Form::Dot(p) => pair(p, f),
            Form::Arrow { from, to } => {
                pair(from, f);
                pair(to, f);
            }
            Form::Many { vars, lets, shape } => {
                vars.iter().for_each(|(_, range)| pair(range, f));
                lets.iter().for_each(|(_, e)| f(e));
                match shape {
                    Shape::Dots(p) => pair(p, f),
                    Shape::Circles { center, r } => {
                        pair(center, f);
                        f(r);
                    }
                    Shape::Cycles { a, b, c } => {
                        f(a);
                        pair(b, f);
                        f(c);
                    }
                    Shape::Curves(a) => arc(a, f),
                }
            }
            Form::Term { .. } | Form::Outline { .. } => {}
            Form::Label {
                values,
                at,
                offset,
                away,
                ..
            } => {
                values.iter().for_each(&mut *f);
                pair(at, f);
                pair(offset, f);
                away.iter().for_each(|p| pair(p, f));
            }
        }
    }
    for expression in &scene.expressions {
        f(&expression.by);
    }
    for spot in &scene.spots {
        pair(&spot.at, f);
        pair(&spot.offset, f);
    }
    let view = &scene.viewport;
    view.fit.iter().flatten().for_each(&mut *f);
    view.fit_curves.iter().for_each(|a| arc(a, f));
    for family in &view.fit_families {
        family.vars.iter().for_each(|(_, range)| pair(range, f));
        family.lets.iter().for_each(|(_, e)| f(e));
        arc(&family.arc, f);
    }
}

/// Every expression a figure's styles evaluate. Destructured whole, so a new property can't be
/// missed here.
fn styled_exprs(styled: &Styled, f: &mut impl FnMut(&Expr)) {
    let Styled {
        marks,
        paper,
        tone: _,
        title_fill,
        title_size,
        caption,
        formula_fill,
        panels,
    } = styled;
    for style in marks {
        let MarkStyle {
            negative,
            positive,
            inside,
            outside,
            stroke,
            width,
            draw: _,
            dash,
            gap,
            blend: _,
            cap: _,
            font: _,
            opacity,
            fill_opacity,
            radius,
            size,
            halo,
            distance,
            place: _,
            show,
            head,
            head_span,
            trim,
            ticks,
            tick,
            leader,
            leader_gap,
        } = style;
        [
            negative,
            positive,
            inside,
            outside,
            stroke,
            width,
            dash,
            gap,
            opacity,
            fill_opacity,
            radius,
            size,
            halo,
            distance,
            show,
            head,
            head_span,
            trim,
            ticks,
            tick,
            leader,
            leader_gap,
        ]
        .into_iter()
        .flatten()
        .for_each(&mut *f);
    }
    if let Some(CaptionStyle {
        fill,
        size,
        halo,
        distance,
        font: _,
    }) = caption
    {
        [fill, size, halo, distance].into_iter().flatten().for_each(&mut *f);
    }
    let PanelStyle {
        columns,
        gap,
        title,
        width,
        height,
        pad,
        em,
    } = panels;
    [
        paper,
        title_fill,
        title_size,
        formula_fill,
        columns,
        gap,
        title,
        width,
        height,
        pad,
        em,
    ]
    .into_iter()
    .flatten()
    .for_each(&mut *f);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose;

    fn compiled(doc: &str, sheets: &[&str]) -> Composition {
        compose(doc, sheets).unwrap()
    }

    #[test]
    fn a_binding_follows_time_through_the_bindings_it_reads() {
        let c = compiled(
            "param a in 0..1\nbind a = time / 10\nlet b = 2·a\nlet k = 3\nlet p = (b, k)\ndot p\n",
            &[],
        );
        let driven = time_driven(&c.panels[0].figure.scene);
        assert!(driven.contains(&"a".to_string()), "{driven:?}");
        assert!(driven.contains(&"b".to_string()), "{driven:?}");
        assert!(!driven.contains(&"k".to_string()), "{driven:?}");
        assert!(composition_uses_time(&c));
    }

    #[test]
    fn a_figure_without_time_needs_no_clock() {
        let c = compiled("param a in 0..1\nlet p = (a, 0)\ndot p\n", &[]);
        assert!(time_driven(&c.panels[0].figure.scene).is_empty());
        assert!(!composition_uses_time(&c));
    }

    #[test]
    fn time_in_a_style_alone_needs_a_clock() {
        let doc = "let p = (0, 0)\ndot p\n";
        assert!(!composition_uses_time(&compiled(doc, &[])));
        let sheet = "style p { opacity: 0.5 + 0.5·sin(time) }\n";
        assert!(composition_uses_time(&compiled(doc, &[sheet])));
    }

    fn figure(pen: &str, sheets: &[&str]) -> Composition {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let read = |p: &str| std::fs::read_to_string(format!("{root}{p}")).unwrap();
        let sheets: Vec<String> = sheets.iter().map(|s| read(s)).collect();
        let refs: Vec<&str> = sheets.iter().map(String::as_str).collect();
        compose(&read(pen), &refs).unwrap()
    }

    #[test]
    fn beats_bound_to_time_are_stepped_by_seeking_it() {
        // Untimed beats that a `bind` drives from time, and beats with lengths.
        for (pen, sheets) in [
            ("corpus/004-bounds/bounds.pen", &["corpus/004-bounds/blog.style"][..]),
            (
                "lab/explainer/limacon.pen",
                &["corpus/theme.style", "lab/explainer/limacon.style"][..],
            ),
        ] {
            let c = figure(pen, sheets);
            let driven = time_driven(&c.panels[0].figure.scene);
            assert!(driven.contains(&"beat".to_string()), "{pen}: {driven:?}");
        }
    }

    #[test]
    fn which_corpus_figures_need_a_clock() {
        let theme = "corpus/theme.style";
        for (pen, sheets, clock) in [
            ("corpus/001-circle/circle.pen", vec!["corpus/001-circle/circle.style"], true),
            ("corpus/002-strokes/ring.pen", vec!["corpus/002-strokes/filled.style"], false),
            ("corpus/002-strokes/wave.pen", vec!["corpus/002-strokes/taper.style"], false),
            ("corpus/003-unroll/unroll.pen", vec!["corpus/003-unroll/blog.style"], true),
            ("corpus/006-square/square.pen", vec![theme, "corpus/006-square/plain.style"], true),
            ("corpus/007-lattice/lattice.pen", vec![theme, "corpus/007-lattice/plain.style"], false),
            ("corpus/012-limacon/limacon.pen", vec!["corpus/012-limacon/plain.style"], true),
        ] {
            assert_eq!(composition_uses_time(&figure(pen, &sheets)), clock, "{pen}");
        }
    }
}
