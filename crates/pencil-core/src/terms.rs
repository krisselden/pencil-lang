//! Expressions whose parts move. An expression is a list of states, and identity is
//! declared rather than found: a term is what's written, so the same term in the next
//! state moves to its new spot, and nothing is diffed or searched for.
//!
//! - A new term fades in, and a term that's gone fades out, riding along with its
//!   neighbors so it isn't left behind.
//! - A copy is written `z.1`. It prints as `z`, and when it's new it starts on `z`.
//! - When a copy goes, it moves into a term of the same name at the same place: factoring.
//!
//! Which term is where is decided here, and how wide text is comes from the renderer, since
//! only it knows the font. What comes out is ordinary labels. `lab/expressions` is where
//! this was chosen, next to the blog's hand-timed version.

use std::collections::HashMap;

use crate::Figure;
use crate::error::Error;
use crate::eval::{self, Env, snap};
use crate::frame::{self, DEFAULT_LABEL_SIZE, Label};
use crate::ir::{Expression, Form, Hang, Piece, Viewport};
use crate::style::{Font, Place};

/// A term's clear space on each side, in ems, if it's an operator.
const OPERATOR_PAD: f64 = 0.23;
const OPERATORS: [&str; 3] = ["+", "=", "−"];

/// How wide text is, in device pixels at a size in device pixels. A renderer answers with
/// its own font.
pub trait TextWidths {
    fn width(&self, text: &str, size: f64, font: Font) -> f64;
}

/// A width that doesn't depend on any font. Good enough for where labels settle, since the
/// bumps are soft, and for anything that isn't drawing text.
pub struct Estimate;

pub const EM_PER_CHAR: f64 = 0.55;

impl TextWidths for Estimate {
    fn width(&self, text: &str, size: f64, _font: Font) -> f64 {
        text.chars().count() as f64 * EM_PER_CHAR * size
    }
}

/// What a term prints as: `z.1` is a copy of `z`.
pub fn base(key: &str) -> &str {
    match key.rsplit_once('.') {
        Some((name, copy))
            if !copy.is_empty()
                && copy.chars().all(|c| c.is_ascii_digit())
                && !name.chars().all(|c| c.is_ascii_digit()) =>
        {
            name
        }
        _ => key,
    }
}

fn is_copy(key: &str) -> bool {
    base(key) != key
}

/// A term's spot in one state, in device pixels: the center of its text.
#[derive(Clone)]
struct Laid {
    at: [f64; 2],
    spot: usize,
    /// The expression it's written in, and its place there, to find its neighbors.
    keys: Vec<String>,
    index: usize,
}

struct Scope<'a> {
    figure: &'a Figure,
    env: &'a Env,
    view: &'a Viewport,
    width: u32,
    height: u32,
    dpr: f64,
    widths: &'a dyn TextWidths,
    /// The mark for each term of this expression.
    marks: HashMap<&'a str, usize>,
}

impl Scope<'_> {
    fn size_and_font(&self, key: &str) -> Result<(f64, Font), Error> {
        let Some(mark) = self.marks.get(key) else {
            return Ok((DEFAULT_LABEL_SIZE * self.dpr, Font::default()));
        };
        let style = &self.figure.styles.marks[*mark];
        let size = match &style.size {
            Some(s) => eval::real(s, self.env)?,
            None => DEFAULT_LABEL_SIZE,
        };
        Ok((size * self.dpr, style.font))
    }

    fn layout(&self, pieces: &[Piece]) -> Result<HashMap<String, Laid>, Error> {
        let mut laid = HashMap::new();
        for piece in pieces {
            let spot = &self.figure.scene.spots[piece.spot];
            // What sits at a point that doesn't exist isn't anywhere.
            let at = |e: &crate::ir::Expr| match eval::real(e, self.env) {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.absent => Ok(None),
                Err(e) => Err(e),
            };
            let (Some(x), Some(y), Some(ox), Some(oy)) = (
                at(&spot.at[0])?,
                at(&spot.at[1])?,
                at(&spot.offset[0])?,
                at(&spot.offset[1])?,
            ) else {
                continue;
            };
            let (dx, dy) = frame::device(x, y, self.view, self.width, self.height);
            let mut advances = Vec::with_capacity(piece.keys.len());
            for key in &piece.keys {
                let (size, font) = self.size_and_font(key)?;
                let text = base(key);
                let pad = if OPERATORS.contains(&text) {
                    OPERATOR_PAD * size
                } else {
                    0.0
                };
                advances.push((self.widths.width(text, size, font), pad));
            }
            let total: f64 = advances.iter().map(|(w, pad)| w + 2.0 * pad).sum();
            let mut left = dx + ox * self.dpr
                - match spot.hang {
                    Hang::Start => 0.0,
                    Hang::Middle => total / 2.0,
                    Hang::End => total,
                };
            for (index, (key, (w, pad))) in piece.keys.iter().zip(advances).enumerate() {
                laid.insert(
                    key.clone(),
                    Laid {
                        at: [left + pad + w / 2.0, dy + oy * self.dpr],
                        spot: piece.spot,
                        keys: piece.keys.clone(),
                        index,
                    },
                );
                left += w + 2.0 * pad;
            }
        }
        Ok(laid)
    }
}

/// How far a term's nearest neighbor that's travelling has come. `travel` says so for a key.
fn ride(term: &Laid, travel: impl Fn(&str) -> Option<[f64; 2]>) -> [f64; 2] {
    let keys = &term.keys;
    for distance in 1..keys.len() {
        let before = term.index.checked_sub(distance).map(|i| &keys[i]);
        let after = keys.get(term.index + distance);
        for key in [before, after].into_iter().flatten() {
            if let Some(moved) = travel(key) {
                return moved;
            }
        }
    }
    [0.0, 0.0]
}

/// Where each term is between two states, `s` of the way: `(key, position, strength)`.
fn between(
    was: &HashMap<String, Laid>,
    now: &HashMap<String, Laid>,
    s: f64,
) -> HashMap<String, ([f64; 2], f64)> {
    let lerp = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s];
    let source = |key: &str| {
        was.get(key)
            .or_else(|| is_copy(key).then(|| was.get(base(key))).flatten())
    };
    let target = |key: &str, from: &Laid| {
        now.get(key).or_else(|| {
            is_copy(key)
                .then(|| {
                    // The same order every frame, whichever way the map iterates.
                    let mut same: Vec<_> = now
                        .iter()
                        .filter(|(k, t)| base(k) == base(key) && t.spot == from.spot)
                        .collect();
                    same.sort_by_key(|(k, _)| k.as_str());
                    same.first().map(|(_, t)| *t)
                })
                .flatten()
        })
    };
    let mut out = HashMap::new();
    for (key, to) in now {
        if let Some(from) = source(key) {
            out.insert(key.clone(), (lerp(from.at, to.at), 1.0));
        } else {
            let moved = ride(to, |other| {
                let (o, t) = (source(other)?, now.get(other)?);
                Some([
                    (o.at[0] - t.at[0]) * (1.0 - s),
                    (o.at[1] - t.at[1]) * (1.0 - s),
                ])
            });
            out.insert(key.clone(), ([to.at[0] + moved[0], to.at[1] + moved[1]], s));
        }
    }
    for (key, from) in was {
        if now.contains_key(key) {
            continue;
        }
        if let Some(into) = target(key, from) {
            out.insert(key.clone(), (lerp(from.at, into.at), 1.0));
        } else {
            let moved = ride(from, |other| {
                let f = was.get(other)?;
                let t = target(other, f)?;
                Some([(t.at[0] - f.at[0]) * s, (t.at[1] - f.at[1]) * s])
            });
            out.insert(
                key.clone(),
                ([from.at[0] + moved[0], from.at[1] + moved[1]], 1.0 - s),
            );
        }
    }
    out
}

/// One frame's terms, as labels, in mark order.
#[allow(clippy::too_many_arguments)]
pub fn labels(
    figure: &Figure,
    env: &Env,
    view: &Viewport,
    width: u32,
    height: u32,
    dpr: f64,
    shown: &[bool],
    widths: &dyn TextWidths,
) -> Result<Vec<Label>, Error> {
    let mut out = Vec::new();
    for (index, expression) in figure.scene.expressions.iter().enumerate() {
        let marks: HashMap<&str, usize> = figure
            .scene
            .marks
            .iter()
            .enumerate()
            .filter_map(|(i, m)| match &m.form {
                Form::Term {
                    expression: e, key, ..
                } if *e == index => Some((key.as_str(), i)),
                _ => None,
            })
            .collect();
        let scope = Scope {
            figure,
            env,
            view,
            width,
            height,
            dpr,
            widths,
            marks,
        };
        let terms = frame_terms(&scope, expression)?;
        for (i, mark) in figure.scene.marks.iter().enumerate() {
            let Form::Term {
                expression: e,
                key,
                text,
            } = &mark.form
            else {
                continue;
            };
            let Some((at, strength)) = terms.get(key) else {
                continue;
            };
            if *e != index || !shown[i] || *strength <= 0.0 {
                continue;
            }
            let style = &figure.styles.marks[i];
            let opacity = match &style.opacity {
                Some(o) => eval::real(o, env)?.clamp(0.0, 1.0),
                None => 1.0,
            };
            out.push(Label {
                id: mark.id.clone(),
                text: text.clone(),
                x: at[0],
                y: at[1],
                size: scope.size_and_font(key)?.0,
                color: match &style.negative {
                    Some(c) => eval::color(c, env)?,
                    None => [0.0; 3],
                },
                halo: match &style.halo {
                    Some(h) => eval::real(h, env)?,
                    None => 0.0,
                } * dpr,
                opacity: opacity * strength,
                font: style.font,
                anchor: *at,
                place: Place::Fixed,
            });
        }
    }
    Ok(out)
}

fn frame_terms(
    scope: &Scope,
    expression: &Expression,
) -> Result<HashMap<String, ([f64; 2], f64)>, Error> {
    let step = match eval::real(&expression.by, scope.env) {
        Ok(v) => v,
        Err(e) if e.absent => return Ok(HashMap::new()),
        Err(e) => return Err(e),
    };
    // State 0 is the empty one before the first that's written.
    let last = expression.states.len();
    let state = |k: usize| -> &[Piece] {
        if k == 0 {
            &[]
        } else {
            &expression.states[k - 1]
        }
    };
    let step = step.clamp(0.0, last as f64);
    let k = (step.floor() as usize).min(last);
    let was = scope.layout(state(k))?;
    if k == last {
        return Ok(between(&was, &was, 1.0));
    }
    let now = scope.layout(state(k + 1))?;
    Ok(between(&was, &now, snap(step - k as f64, None)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::frame::{Inputs, prepare};

    const DOC: &str = "
        viewport { x: 0..400, y: 0..400, width: 400 }
        param step = 0 in 0..9
        place left = (100, 200)
        place right = (300, 200)
        expression e by step {
          \"a\" at left
          \"a + b\" at left
          \"a + b\" at right
          \"z.1 a + z.2 b\" at right
          \"z.1 ( a + b )\" at right
          empty
        }
        let P = (10, 10)
        expression copies by step {
          \"z\" at left
          \"z, unused\" at left
          \"z\" at left
          \"z z.1\" at left
          \"z\" at left
          empty
        }";

    /// A fixed width, so positions are easy to check by hand.
    struct Ten;
    impl TextWidths for Ten {
        fn width(&self, text: &str, _size: f64, _font: Font) -> f64 {
            text.chars().count() as f64 * 10.0
        }
    }

    fn at(step: f64) -> Vec<Label> {
        let f = build(DOC, &["style term { size: 10px }"]).unwrap();
        let inputs = Inputs {
            time: 0.0,
            params: vec![("step".to_string(), step)],
        };
        frame::prepare_with(&f, &inputs, 400, 400, 1.0, &Ten)
            .unwrap()
            .labels
    }

    fn find<'a>(labels: &'a [Label], id: &str) -> &'a Label {
        labels.iter().find(|l| l.id == id).unwrap_or_else(|| {
            panic!(
                "no `{id}` in {:?}",
                labels.iter().map(|l| &l.id).collect::<Vec<_>>()
            )
        })
    }

    fn has(labels: &[Label], id: &str) -> bool {
        labels.iter().any(|l| l.id == id)
    }

    #[test]
    fn a_state_lays_out_left_to_right_and_holds_at_whole_steps() {
        let two = at(2.0);
        let (a, plus, b) = (find(&two, "e.a"), find(&two, "e.+"), find(&two, "e.b"));
        // a is 10 wide from 100. + has a 2.3 pad each side, then b.
        assert_eq!(a.x, 105.0);
        assert!((plus.x - 117.3).abs() < 1e-9, "{}", plus.x);
        assert!((b.x - 129.6).abs() < 1e-9, "{}", b.x);
        assert_eq!((a.opacity, plus.opacity, b.opacity), (1.0, 1.0, 1.0));
        assert_eq!(a.y, 200.0);
    }

    #[test]
    fn what_matches_moves_and_stays_solid() {
        let start = find(&at(2.0), "e.a").x;
        let end = find(&at(3.0), "e.a").x;
        assert_eq!(end - start, 200.0);
        let mid = at(2.5);
        let a = find(&mid, "e.a");
        assert_eq!(a.opacity, 1.0);
        let s = snap(0.5, None);
        assert!((a.x - (start + 200.0 * s)).abs() < 1e-9, "{}", a.x);
    }

    #[test]
    fn only_what_does_not_match_fades() {
        let mid = at(1.5);
        let s = snap(0.5, None);
        assert_eq!(find(&mid, "e.a").opacity, 1.0);
        assert!((find(&mid, "e.b").opacity - s).abs() < 1e-9);
        assert!((find(&mid, "e.+").opacity - s).abs() < 1e-9);
        // Nothing left over at either end.
        assert!(!has(&at(1.0), "e.b"));
        assert!(!has(&at(0.0), "e.a"));
    }

    /// A term fading in travels with the rest, not ahead of it at its final spot.
    #[test]
    fn a_new_term_rides_with_its_neighbors() {
        let doc = "viewport { x: 0..400, y: 0..400, width: 400 }
            param step = 0 in 0..3
            place one = (100, 100)
            place two = (300, 300)
            expression e by step {
              \"a\" at one
              \"a b\" at two
            }";
        let f = build(doc, &[]).unwrap();
        let inputs = Inputs {
            time: 0.0,
            params: vec![("step".to_string(), 1.3)],
        };
        let labels = frame::prepare_with(&f, &inputs, 400, 400, 1.0, &Ten)
            .unwrap()
            .labels;
        let (a, b) = (find(&labels, "e.a"), find(&labels, "e.b"));
        // Always one width apart, wherever they are on the way.
        assert!((b.x - a.x - 10.0).abs() < 1e-9, "{} and {}", a.x, b.x);
        assert_eq!(a.y, b.y);
    }

    #[test]
    fn a_new_copy_starts_on_what_it_copies() {
        let doc = "viewport { x: 0..400, y: 0..400, width: 400 }
            param step = 0 in 0..3
            place one = (100, 100)
            place two = (300, 300)
            expression e by step {
              \"z\" at one
              \"z\" at one, \"z.1\" at two
            }";
        let f = build(doc, &[]).unwrap();
        let labels = |step: f64| {
            let inputs = Inputs {
                time: 0.0,
                params: vec![("step".to_string(), step)],
            };
            frame::prepare_with(&f, &inputs, 400, 400, 1.0, &Ten)
                .unwrap()
                .labels
        };
        let start = labels(1.0001);
        let (z, copy) = (find(&start, "e.z"), find(&start, "e.z.1"));
        assert!((copy.x - z.x).abs() < 1.0 && (copy.y - z.y).abs() < 1.0);
        assert_eq!(copy.opacity, 1.0);
        assert_eq!(copy.text, "z");
        let end = labels(2.0);
        assert_eq!(find(&end, "e.z.1").x, 305.0);
        assert_eq!(find(&end, "e.z").x, 105.0);
    }

    /// Factoring: z.2 moves into z.1, and z at another place has nothing to do with it.
    #[test]
    fn a_copy_that_goes_moves_into_its_namesake_at_that_place() {
        let doc = "viewport { x: 0..400, y: 0..400, width: 400 }
            param step = 0 in 0..3
            place far = (50, 50)
            place sum = (200, 200)
            expression e by step {
              \"z\" at far, \"z.1 a + z.2 b\" at sum
              \"z\" at far, \"z.1 ( a + b )\" at sum
            }";
        let f = build(doc, &[]).unwrap();
        let labels = |step: f64| {
            let inputs = Inputs {
                time: 0.0,
                params: vec![("step".to_string(), step)],
            };
            frame::prepare_with(&f, &inputs, 400, 400, 1.0, &Ten)
                .unwrap()
                .labels
        };
        let near_end = labels(1.9999);
        let (two, one) = (find(&near_end, "e.z.2"), find(&near_end, "e.z.1"));
        assert!((two.x - one.x).abs() < 0.01, "{} into {}", two.x, one.x);
        assert_eq!(two.opacity, 1.0);
        assert!(!has(&labels(2.0), "e.z.2"));
        // Parens fade in, riding with what they enclose.
        assert_eq!(find(&near_end, "e.(").text, "(");
        assert!(find(&near_end, "e.(").opacity > 0.99);
    }

    #[test]
    fn past_the_last_state_it_holds() {
        let doc = "viewport { x: 0..400, y: 0..400 }
            param step = 5 in 0..9
            place p = (100, 100)
            expression e by step { \"a\" at p }";
        let f = build(doc, &[]).unwrap();
        let p = prepare(&f, &Inputs::at(0.0), 400, 400, 1.0).unwrap();
        assert_eq!(p.labels.len(), 1);
        assert_eq!(p.labels[0].opacity, 1.0);
    }

    #[test]
    fn terms_can_be_styled_by_name_and_as_a_whole() {
        let f = build(
            DOC,
            &["style term { size: 10px }
               style term.a { fill: white }
               style e { halo: 2px }"],
        )
        .unwrap();
        let inputs = Inputs {
            time: 0.0,
            params: vec![("step".to_string(), 2.0)],
        };
        let labels = frame::prepare_with(&f, &inputs, 400, 400, 1.0, &Ten)
            .unwrap()
            .labels;
        assert_eq!(find(&labels, "e.a").color, [1.0; 3]);
        assert_eq!(find(&labels, "e.b").color, [0.0; 3]);
        assert_eq!(find(&labels, "e.b").halo, 2.0);
    }

    #[test]
    fn a_term_at_a_point_that_does_not_exist_fades() {
        let doc = "viewport { x: -2..2, y: -2..2 }
            param step = 2 in 0..2
            param r = 1 in 0..3
            curve c = circle((0, 0), (1, 0))
            let P = intersect(c, line((-2, r), (2, r)))
            place near = P
            expression e by step {
              \"a\" at near
              \"a\" at near
            }";
        let f = build(doc, &[]).unwrap();
        let with = |r: f64| {
            let inputs = Inputs {
                time: 0.0,
                params: vec![("r".to_string(), r)],
            };
            prepare(&f, &inputs, 100, 100, 1.0).unwrap().labels.len()
        };
        assert_eq!(with(0.5), 1);
        assert_eq!(with(2.0), 0);
    }

    #[test]
    fn places_must_be_declared_first() {
        let err = build(
            "param step in 0..1\nexpression e by step { \"a\" at nowhere }",
            &[],
        )
        .unwrap_err();
        assert!(err.message.contains("no place named `nowhere`"), "{err}");
    }

    #[test]
    fn a_term_written_twice_in_a_state_is_an_error() {
        // The second `+` would take the first's place, and the sum would lose a sign.
        let doc = |state: &str| {
            build(
                &format!(
                    "param step in 0..1\nplace p = (0, 0)\nplace q = (1, 0)\nexpression e by step {{\n  {state}\n}}"
                ),
                &[],
            )
        };
        let err = doc("\"a + b + c\" at p").unwrap_err();
        assert!(err.message.contains("`+` is written twice"), "{err}");
        assert!(err.message.contains("Write the second as `+.1`"), "{err}");
        assert_eq!(err.line, 4);
        // Across the pieces of one state, too, and the suggestion skips copies already written.
        let err = doc("\"z +.1\" at p, \"+ z\" at q").unwrap_err();
        assert!(err.message.contains("`z` is written twice"), "{err}");
        let err = doc("\"a + b +.1 c + d\" at p").unwrap_err();
        assert!(err.message.contains("as `+.2`"), "{err}");
        // A number can't have a copy, since `2.1` is a number.
        let err = doc("\"2 + 2\" at p").unwrap_err();
        assert!(err.message.contains("a number can't have a copy"), "{err}");
        // With a copy, both signs are drawn, in order.
        let f = doc("\"a + b +.1 c\" at p\n  \"a + b\" at p").unwrap();
        let inputs = Inputs {
            time: 0.0,
            params: vec![("step".to_string(), 1.0)],
        };
        let labels = frame::prepare_with(&f, &inputs, 400, 400, 1.0, &Ten)
            .unwrap()
            .labels;
        let (first, second) = (find(&labels, "e.+"), find(&labels, "e.+.1"));
        assert_eq!((first.text.as_str(), second.text.as_str()), ("+", "+"));
        assert!(first.x < second.x);
        assert_eq!((first.opacity, second.opacity), (1.0, 1.0));
    }

    #[test]
    fn copies_print_as_what_they_copy() {
        assert_eq!(base("z.1"), "z");
        assert_eq!(base("z"), "z");
        assert_eq!(base("2.5"), "2.5");
        assert_eq!(base("a.b"), "a.b");
    }
}
