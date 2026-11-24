//! A frame as SVG, for the styles that have a vector form.
//!
//! It's written from what a frame already prepares for the renderers: the stroke segments of
//! `stroke::build`, the regions' vertices, the circles' sides, and the labels where they settled.
//! So the picture is the renderers' own geometry, at the size it was prepared for, and not a
//! second reading of the figure.
//!
//! A style has a vector form when its value is one color or one number for a whole mark, for one
//! of a collection, or along a stroke. What's worked out per pixel has none: a fill or a stroke
//! that reads `self.dist`, `blend: add`, and the paper's `tone`. Those are left out, and
//! `Drawn::skipped` says which, so nothing is drawn as something it isn't.

use std::collections::HashSet;
use std::fmt;
use std::fmt::Write as _;

use crate::error::Error;
use crate::eval::{self, Env};
use crate::frame::{self, Inputs, Label, Prepared};
use crate::ir::{Expr, Form};
use crate::stroke::{self, FLOATS_PER_SEGMENT};
use crate::style::{Blend, Font, MarkStyle, SELF_CURVATURE, SELF_DIST, SELF_PIXEL, Tone};
use crate::terms::TextWidths;
use crate::{Composition, Figure, PanelFrame};

/// The fonts a page draws labels in, by role, as `<pencil-figure>` names them.
const SANS: &str = "system-ui, -apple-system, sans-serif";
const MATH: &str = "Charter, 'Bitstream Charter', 'Sitka Text', Cambria, serif";
/// How far below a label's center its baseline is, in ems: half of what a line of text stands
/// above its baseline, less what it hangs below. A viewer's font decides in the end.
const BASELINE: f64 = 0.355;
/// How far past the frame a stroke is kept, beyond its own half width, in pixels.
const KEEP: f64 = 2.0;
/// A joint's bevel is rounded when it would fall this far short of round, in pixels.
const ROUND_FROM: f64 = 0.02;
/// How many levels of ink a stroke whose opacity varies along it is cut into, as
/// `stroke::INK_STEP` cuts it.
const INK_LEVELS: f64 = 64.0;
/// Past this radius in pixels, a circle's side is drawn as the side of a line: across a frame
/// it's straight to a fraction of a pixel, and a viewer's arithmetic isn't that fine out there.
const STRAIGHT_PX: f64 = 1e6;

/// What of a mark isn't in the SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    /// All of it.
    Mark,
    Fill,
    Stroke,
    /// Nothing: it's drawn, without what the property does.
    Property,
}

/// Something a frame's SVG doesn't have, because its style has no vector form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The mark's name, with its panel's before it, or `paper`.
    pub mark: String,
    pub left: Left,
    /// The property that has no vector form here.
    pub property: &'static str,
    why: &'static str,
}

impl fmt::Display for Skipped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Skipped {
            mark,
            property,
            why,
            ..
        } = self;
        match self.left {
            Left::Mark => write!(f, "`{mark}` is left out: its `{property}` {why}"),
            Left::Fill => write!(f, "`{mark}`'s fill is left out: its `{property}` {why}"),
            Left::Stroke => write!(f, "`{mark}`'s stroke is left out: its `{property}` {why}"),
            Left::Property => write!(f, "`{mark}` is drawn without its `{property}`, which {why}"),
        }
    }
}

const PER_PIXEL: &str = "reads `self.dist`, which is worked out per pixel";

/// A frame's SVG, and what it had to leave out.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub svg: String,
    pub skipped: Vec<Skipped>,
}

/// A number as SVG writes it, to a hundredth.
fn num(v: f64) -> String {
    let mut text = format!("{v:.2}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text = "0".into();
    }
    text
}

/// An opacity, to a thousandth.
fn alpha(v: f64) -> String {
    let text = format!("{:.3}", v.clamp(0.0, 1.0));
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A linear color as `#rrggbb`.
fn hex(c: [f64; 3]) -> String {
    let [r, g, b] = c.map(|v| (eval::srgb_encode(v) * 255.0).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn reads(e: &Option<Expr>, name: &str) -> bool {
    let mut found = false;
    if let Some(e) = e {
        e.visit_syms(&mut |s| found |= s == name);
    }
    found
}

/// What's shared while a document is written: the ids taken, and what was left out.
#[derive(Default)]
struct Writer {
    ids: HashSet<String>,
    skipped: Vec<Skipped>,
}

impl Writer {
    /// A name as an id no other element has. A dot and its label can share a name, and a
    /// label's name can have a `/` in it.
    fn id(&mut self, name: &str) -> String {
        let mut id: String = name
            .chars()
            .map(|c| match c {
                c if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') => c,
                _ => '-',
            })
            .collect();
        if !id
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            id.insert(0, '_');
        }
        let mut unique = id.clone();
        let mut copy = 2;
        while !self.ids.insert(unique.clone()) {
            unique = format!("{id}-{copy}");
            copy += 1;
        }
        escape(&unique)
    }

    fn skip(&mut self, mark: &str, left: Left, property: &'static str, why: &'static str) {
        let skipped = Skipped {
            mark: mark.to_string(),
            left,
            property,
            why,
        };
        if !self.skipped.contains(&skipped) {
            self.skipped.push(skipped);
        }
    }
}

/// One stroke segment, as `stroke::build` lays it out, in device pixels.
#[derive(Clone, Copy, Debug)]
struct Seg {
    a: [f64; 2],
    b: [f64; 2],
    /// Half its width at each end.
    r: [f64; 2],
    /// How far along its line each end is.
    s: [f64; 2],
    on: f64,
    period: f64,
    /// Its line's length if the line ends square, and 0 if its ends are round.
    clip: f64,
    ink: f64,
    /// Whether each end is where the frame cut it, and not where its line goes on or ends.
    cut: [bool; 2],
    /// Its row of `Prepared::colors` plus one, for a line with its own color, and 0 otherwise.
    row: usize,
}

impl Seg {
    fn length(&self) -> f64 {
        (self.b[0] - self.a[0]).hypot(self.b[1] - self.a[1])
    }

    /// The part of it that can show on a frame this big, if any does. One that's all on the
    /// frame comes back as it was, so it still meets its neighbors exactly.
    fn within(self, width: f64, height: f64) -> Option<Seg> {
        let numbers = [self.a, self.b, self.r, self.s];
        if !numbers.iter().flatten().all(|v| v.is_finite()) {
            return None;
        }
        let pad = self.r[0].max(self.r[1]) + KEEP;
        let (dx, dy) = (self.b[0] - self.a[0], self.b[1] - self.a[1]);
        let (mut from, mut to) = (0.0f64, 1.0f64);
        for (p, q) in [
            (-dx, self.a[0] + pad),
            (dx, width + pad - self.a[0]),
            (-dy, self.a[1] + pad),
            (dy, height + pad - self.a[1]),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else if p < 0.0 {
                from = from.max(q / p);
            } else {
                to = to.min(q / p);
            }
        }
        if from > to {
            return None;
        }
        if from <= 0.0 && to >= 1.0 {
            return Some(self);
        }
        let at = |v: [f64; 2], t: f64| v[0] + (v[1] - v[0]) * t;
        let point = |t: f64| [self.a[0] + dx * t, self.a[1] + dy * t];
        Some(Seg {
            a: point(from),
            b: point(to),
            r: [at(self.r, from), at(self.r, to)],
            s: [at(self.s, from), at(self.s, to)],
            cut: [from > 0.0, to < 1.0],
            ..self
        })
    }
}

fn segments(raw: &[f32], rows: &[f32], width: f64, height: f64) -> Vec<Seg> {
    raw.chunks_exact(FLOATS_PER_SEGMENT)
        .enumerate()
        .filter_map(|(i, s)| {
            let f = |i: usize| f64::from(s[i]);
            Seg {
                a: [f(0), f(1)],
                b: [f(2), f(3)],
                r: [f(4), f(5)],
                s: [f(6), f(7)],
                on: f(8),
                period: f(9),
                clip: f(10),
                ink: 1.0 - f(11),
                cut: [false; 2],
                row: rows.get(i).map_or(0, |row| *row as usize),
            }
            .within(width, height)
        })
        .collect()
}

/// Segments that follow one another along one line.
fn chains(segments: &[Seg]) -> Vec<&[Seg]> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=segments.len() {
        let joined = i < segments.len() && {
            let (last, next) = (&segments[i - 1], &segments[i]);
            last.b == next.a
                && last.s[1] == next.s[0]
                && last.on == next.on
                && last.period == next.period
                && last.clip == next.clip
        };
        if !joined {
            out.push(&segments[start..i]);
            start = i;
        }
    }
    out
}

/// An element with its attributes still open, so a caller can add an id or an opacity.
type Element = String;

fn close(element: &Element) -> String {
    match element.strip_suffix('>') {
        // A group, which is already whole.
        Some(_) => element.clone(),
        None => format!("{element}/>"),
    }
}

fn circle(at: [f64; 2], r: f64, paint: &str) -> Element {
    format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{paint}\"",
        num(at[0]),
        num(at[1]),
        num(r)
    )
}

fn polygon(points: &[[f64; 2]]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let command = match i {
            0 => "M",
            1 => "L",
            _ => " ",
        };
        let _ = write!(d, "{command}{} {}", num(p[0]), num(p[1]));
    }
    d
}

/// One run of a line's segments, as elements painted in `paint`. A stroke of one width is a
/// path with that stroke width, as SVG strokes it. A stroke whose width changes along it is the
/// outline of what the renderers cover: each segment's trapezoid, with a disc at a joint that
/// turns and at a round end.
fn run_elements(run: &[Seg], paint: &str, out: &mut Vec<Element>) {
    let run: Vec<Seg> = run.iter().copied().filter(|s| s.length() > 0.0).collect();
    let Some(first) = run.first() else {
        return;
    };
    let last = run[run.len() - 1];
    let round = first.clip <= 0.0;
    let mut points = vec![first.a];
    let mut radii = vec![first.r[0]];
    for seg in &run {
        points.push(seg.b);
        radii.push(seg.r[1]);
    }
    let widest = radii.iter().copied().fold(0.0, f64::max);
    if widest <= 0.0 {
        return;
    }
    let even = radii.iter().all(|r| (r - radii[0]).abs() < 1e-3);
    let dashed = first.period > 0.0 && first.on < first.period;
    if even {
        let ends = [points[0], points[points.len() - 1]];
        let closed = round
            && points.len() > 3
            && (ends[0][0] - ends[1][0]).hypot(ends[0][1] - ends[1][1]) < 1e-3;
        let mut d = polygon(if closed {
            &points[..points.len() - 1]
        } else {
            &points
        });
        if closed {
            d.push('Z');
        }
        let mut path = format!(
            "<path d=\"{d}\" fill=\"none\" stroke=\"{paint}\" stroke-width=\"{}\" stroke-linejoin=\"round\"",
            num(radii[0] * 2.0)
        );
        if dashed {
            let _ = write!(
                path,
                " stroke-dasharray=\"{} {}\"",
                num(first.on),
                num(first.period - first.on)
            );
            let offset = first.s[0].rem_euclid(first.period);
            if offset > 0.005 {
                let _ = write!(path, " stroke-dashoffset=\"{}\"", num(offset));
            }
        } else if round && !closed {
            path.push_str(" stroke-linecap=\"round\"");
        }
        out.push(path);
        // SVG caps every dash alike, and here only the line's own ends are round.
        if dashed && round && !closed {
            for (at, along, cut) in [
                (ends[0], first.s[0], first.cut[0]),
                (ends[1], last.s[1], last.cut[1]),
            ] {
                if !cut && along.rem_euclid(first.period) <= first.on {
                    out.push(circle(at, radii[0], paint));
                }
            }
        }
        return;
    }

    let normals: Vec<[f64; 2]> = run
        .iter()
        .map(|s| {
            let length = s.length();
            [-(s.b[1] - s.a[1]) / length, (s.b[0] - s.a[0]) / length]
        })
        .collect();
    let off = |i: usize, n: [f64; 2], side: f64| {
        [
            points[i][0] + n[0] * radii[i] * side,
            points[i][1] + n[1] * radii[i] * side,
        ]
    };
    // One outline is enough while every joint's inner corner lies inside both its segments,
    // which is while the stroke is narrower than its bends are tight. Otherwise the outline
    // would wind to nothing there, and each segment is its own shape.
    let mut simple = true;
    let mut turned = Vec::new();
    for j in 1..run.len() {
        let (m, n) = (normals[j - 1], normals[j]);
        let (cross, dot) = (m[0] * n[1] - m[1] * n[0], m[0] * n[0] + m[1] * n[1]);
        let reach = radii[j] * cross.abs();
        simple &= dot > 0.0 && reach <= run[j - 1].length().min(run[j].length());
        // How far a bevel's middle falls short of the round joint the renderers draw.
        if radii[j] * (1.0 - ((1.0 + dot.clamp(-1.0, 1.0)) / 2.0).sqrt()) > ROUND_FROM {
            turned.push(j);
        }
    }
    let mut d = String::new();
    if simple {
        let mut outline = Vec::with_capacity(run.len() * 4);
        for (i, n) in normals.iter().enumerate() {
            outline.push(off(i, *n, 1.0));
            outline.push(off(i + 1, *n, 1.0));
        }
        for (i, n) in normals.iter().enumerate().rev() {
            outline.push(off(i + 1, *n, -1.0));
            outline.push(off(i, *n, -1.0));
        }
        outline.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.005 && (a[1] - b[1]).abs() < 0.005);
        d = polygon(&outline);
        d.push('Z');
    } else {
        // Every piece winds the same way, so together they fill their union.
        let mut piece = |mut points: Vec<[f64; 2]>| {
            let area: f64 = (0..points.len())
                .map(|i| {
                    let (p, q) = (points[i], points[(i + 1) % points.len()]);
                    p[0] * q[1] - q[0] * p[1]
                })
                .sum();
            if area < 0.0 {
                points.reverse();
            }
            if area != 0.0 {
                d.push_str(&polygon(&points));
                d.push('Z');
            }
        };
        for (i, n) in normals.iter().enumerate() {
            piece(vec![
                off(i, *n, 1.0),
                off(i + 1, *n, 1.0),
                off(i + 1, *n, -1.0),
                off(i, *n, -1.0),
            ]);
            if i > 0 {
                for side in [1.0, -1.0] {
                    piece(vec![
                        points[i],
                        off(i, normals[i - 1], side),
                        off(i, *n, side),
                    ]);
                }
            }
        }
    }
    out.push(format!("<path d=\"{d}\" fill=\"{paint}\""));
    for j in turned {
        out.push(circle(points[j], radii[j], paint));
    }
    if round {
        for (i, cut) in [(0, first.cut[0]), (points.len() - 1, last.cut[1])] {
            if !cut && radii[i] > 0.01 {
                out.push(circle(points[i], radii[i], paint));
            }
        }
    }
}

/// A mark's line stroke: what to draw, and how.
enum Stroke {
    /// Elements in the stroke's color, all with this much ink.
    Even(Vec<Element>, f64),
    /// A mask's contents, for a stroke whose ink varies along it.
    Masked(Vec<Element>),
}

/// Whether a stroke is dashed where its width also varies, which no path here carries.
fn dashes_a_taper(segments: &[Seg]) -> bool {
    segments
        .iter()
        .any(|s| s.period > 0.0 && s.on < s.period && (s.r[0] - s.r[1]).abs() > 1e-3)
}

fn stroke_elements(segments: &[Seg], color: &str) -> Stroke {
    if segments.is_empty() {
        return Stroke::Even(Vec::new(), 1.0);
    }
    let inks = segments.iter().map(|s| s.ink);
    let (least, most) = inks.fold((1.0f64, 0.0f64), |(lo, hi), ink| (lo.min(ink), hi.max(ink)));
    let level = |seg: &Seg| (seg.ink * INK_LEVELS).round();
    if most - least < 0.5 / INK_LEVELS {
        let mut out = Vec::new();
        for chain in chains(segments) {
            // Dots, which are segments of no length.
            for seg in chain.iter().filter(|s| s.length() <= 0.0 && s.clip <= 0.0) {
                if seg.r[0] > 0.0 {
                    out.push(circle(seg.a, seg.r[0], color));
                }
            }
            run_elements(chain, color, &mut out);
        }
        return Stroke::Even(out, most.max(0.0));
    }
    // A mask's luminance is how much of the stroke shows, so each run is painted in a gray as
    // light as it's inked, the lightest last: where pieces overlap, the most ink wins, as it
    // does in the renderers' coverage.
    let mut runs: Vec<(f64, Vec<Element>)> = Vec::new();
    for chain in chains(segments) {
        let mut start = 0;
        for i in 1..=chain.len() {
            if i < chain.len() && level(&chain[i]) == level(&chain[start]) {
                continue;
            }
            let ink = level(&chain[start]);
            // A line with round ends has them on every piece, which is how its pieces overlap.
            // One with square ends overlaps a segment into each neighbor instead.
            let reach = usize::from(chain[start].clip > 0.0);
            let run = &chain[start.saturating_sub(reach)..(i + reach).min(chain.len())];
            let gray = hex([eval::srgb_decode(ink / INK_LEVELS); 3]);
            let mut elements = Vec::new();
            if ink > 0.0 {
                for seg in run.iter().filter(|s| s.length() <= 0.0 && s.clip <= 0.0) {
                    elements.push(circle(seg.a, seg.r[0], &gray));
                }
                run_elements(run, &gray, &mut elements);
            }
            runs.push((ink, elements));
            start = i;
        }
    }
    runs.sort_by(|a, b| a.0.total_cmp(&b.0));
    Stroke::Masked(
        runs.into_iter()
            .flat_map(|(_, elements)| elements)
            .collect(),
    )
}

/// A closed shape one side of a mark lies inside.
enum Outline {
    Circle([f64; 2], f64),
    /// Path data, and whether it's filled by the even-odd rule.
    Path(String, bool),
}

impl Outline {
    fn path(&self) -> String {
        match self {
            Outline::Circle(at, r) => format!(
                "M{} {}a{r} {r} 0 1 0 {d} 0a{r} {r} 0 1 0 -{d} 0Z",
                num(at[0] - r),
                num(at[1]),
                r = num(*r),
                d = num(r * 2.0),
            ),
            Outline::Path(d, _) => d.clone(),
        }
    }
}

/// A mark's sides: a closed shape, and which side of the mark is inside it.
struct Sides {
    outline: Outline,
    negative_inside: bool,
}

/// Which side a facet's fill covers.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Negative,
    Positive,
}

/// The fills of a mark's sides, as the oracle's `Frame::fill` gives each its facet.
fn fill_elements(
    style: &MarkStyle,
    flat: &MarkStyle,
    env: &Env,
    sides: &Sides,
    frame: [f64; 2],
    opacity: f64,
    out: &mut Vec<Element>,
) -> Result<(), Error> {
    let fill_opacity = match &flat.fill_opacity {
        Some(o) => eval::real(o, env)?.clamp(0.0, 1.0),
        None => 1.0,
    };
    let opacity = opacity * fill_opacity;
    if opacity <= 0.0 {
        return Ok(());
    }
    // `::inside` and `::outside` only exist on a circle, and win over the sign they share.
    let round = style.inside.is_some() || style.outside.is_some();
    let forward = !round || env.get(SELF_CURVATURE).unwrap_or(1.0) >= 0.0;
    let [negative_gates, positive_gates] = style.sign_gates();
    let which = usize::from(!forward);
    let (negative_gate, positive_gate) = match round {
        true => (negative_gates[which], positive_gates[which]),
        false => (1.0, 1.0),
    };
    let (disk, rest) = match forward {
        true => (Side::Negative, Side::Positive),
        false => (Side::Positive, Side::Negative),
    };
    let mut fills = Vec::new();
    for (fill, side, gate) in [
        (&flat.negative, Side::Negative, negative_gate),
        (&flat.positive, Side::Positive, positive_gate),
        (&flat.inside, disk, 1.0),
        (&flat.outside, rest, 1.0),
    ] {
        if let (Some(fill), true) = (fill, gate > 0.0) {
            fills.push((side, hex(eval::color(fill, env)?)));
        }
    }
    let within = match sides.negative_inside {
        true => Side::Negative,
        false => Side::Positive,
    };
    // Two opaque sides share an edge, and filled as two shapes they'd let the paper through
    // along it. So the outer one goes under the whole frame, and the inner one over it.
    let both = fills.iter().any(|(side, _)| *side == within)
        && fills.iter().any(|(side, _)| *side != within)
        && opacity >= 1.0;
    let whole = format!("M0 0H{}V{}H0Z", num(frame[0]), num(frame[1]));
    // The outer side first, for that reason.
    fills.sort_by_key(|(side, _)| *side == within);
    for (side, color) in fills {
        let mut element = match (side == within, &sides.outline) {
            (true, Outline::Circle(at, r)) => circle(*at, *r, &color),
            // None of the frame is on this side.
            (true, Outline::Path(d, _)) if d.is_empty() => continue,
            (true, Outline::Path(d, even_odd)) => {
                let rule = if *even_odd {
                    " fill-rule=\"evenodd\""
                } else {
                    ""
                };
                format!("<path d=\"{d}\" fill=\"{color}\"{rule}")
            }
            (false, _) if both => format!("<path d=\"{whole}\" fill=\"{color}\""),
            (false, outline) => format!(
                "<path d=\"{whole}{}\" fill=\"{color}\" fill-rule=\"evenodd\"",
                outline.path()
            ),
        };
        if opacity < 1.0 {
            let _ = write!(element, " fill-opacity=\"{}\"", alpha(opacity));
        }
        out.push(element);
    }
    Ok(())
}

/// How many times a ray from a point crosses loops: odd inside them, by the even-odd rule.
fn crossings(p: [f64; 2], loops: &[Vec<[f64; 2]>]) -> usize {
    let mut count = 0;
    for points in loops {
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
            {
                count += 1;
            }
        }
    }
    count
}

/// The part of a frame where `value` is below zero, for a `value` that's linear across it.
fn half_frame(frame: [f64; 2], value: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let corners = [[0.0, 0.0], [frame[0], 0.0], frame, [0.0, frame[1]]];
    let mut out = Vec::new();
    for i in 0..4 {
        let (p, q) = (corners[i], corners[(i + 1) % 4]);
        let (f, g) = (value(p), value(q));
        if f < 0.0 {
            out.push(p);
        }
        if (f < 0.0) != (g < 0.0) {
            let t = f / (f - g);
            out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
        }
    }
    out
}

/// A line's points in device pixels, and whether it closes.
type Line = (Vec<[f64; 2]>, bool);

/// A curve's lines. A circle that has become a line has no path of its own to follow, so it's
/// traced, as any implicit curve is.
fn curve_lines(
    form: &Form,
    env: &Env,
    prepared: &Prepared,
    width: u32,
    height: u32,
    reach: f64,
) -> Result<Vec<Line>, Error> {
    let view = &prepared.view;
    let mut lines = stroke::lines(form, env, view, width, height, false, reach)?;
    if let (
        true,
        Form::Field {
            vars,
            field,
            grad,
            exact,
            curvature,
            line: Some(_),
        },
    ) = (lines.is_empty(), form)
    {
        let traced = Form::Field {
            vars: vars.clone(),
            field: field.clone(),
            grad: grad.clone(),
            exact: *exact,
            curvature: curvature.clone(),
            line: None,
        };
        lines = stroke::lines(&traced, env, view, width, height, false, reach)?;
    }
    Ok(lines
        .into_iter()
        .map(|line| {
            let points = line
                .points
                .iter()
                .map(|p| {
                    let (x, y) = frame::device(p[0], p[1], view, width, height);
                    [x, y]
                })
                .collect();
            (points, line.closed)
        })
        .collect())
}

/// Wraps a mark's elements in its name: on the element itself if there's one, and on a group
/// of them otherwise.
fn named(id: &str, elements: Vec<String>, out: &mut String) {
    match elements.as_slice() {
        [] => {}
        [only] if !only.starts_with("<g") && !only.starts_with("<mask") => {
            let (tag, rest) = only.split_once(' ').unwrap_or((only, ""));
            let _ = writeln!(out, "{tag} id=\"{id}\" {rest}");
        }
        many => {
            let _ = writeln!(out, "<g id=\"{id}\">");
            for element in many {
                let _ = writeln!(out, "{element}");
            }
            out.push_str("</g>\n");
        }
    }
}

/// One panel's marks, in order, in its own device pixels.
#[allow(clippy::too_many_arguments)]
fn marks(
    writer: &mut Writer,
    prefix: &str,
    figure: &Figure,
    inputs: &Inputs,
    prepared: &Prepared,
    width: u32,
    height: u32,
    out: &mut String,
) -> Result<(), Error> {
    let mut frame_env = eval::frame_env(figure, inputs)?;
    prepared.units(&mut frame_env);
    frame_env.set(SELF_PIXEL, prepared.pixel);
    let view = &prepared.view;
    let size = [width as f64, height as f64];
    let per_unit = size[0] / (view.x[1] - view.x[0]);
    let sided = figure.sided();
    let (mut layer, mut region) = (0, 0);
    for (i, (mark, style)) in figure
        .scene
        .marks
        .iter()
        .zip(&figure.styles.marks)
        .enumerate()
    {
        // An absent mark draws nothing, but keeps its places in the shared tables.
        let is_line = style.stroke_is_line(&mark.form);
        let (this_layer, this_region) = (layer, region);
        layer += usize::from(is_line);
        region += usize::from(matches!(mark.form, Form::Region(_)));
        if !prepared.shown[i] || matches!(mark.form, Form::Label { .. } | Form::Term { .. }) {
            continue;
        }
        let name = format!("{prefix}{}", mark.id);
        if style.blend == Blend::Add {
            writer.skip(
                &name,
                Left::Mark,
                "blend",
                "is `add`, which adds light, and SVG has no paint that does",
            );
            continue;
        }
        if style.blend == Blend::Linear {
            writer.skip(
                &name,
                Left::Property,
                "blend",
                "is `linear`, and SVG blends edges and opacity as `gamma` does",
            );
        }
        let mut env = frame_env.clone();
        if let Form::Field {
            curvature: Some(curvature),
            ..
        } = &mark.form
            && let Ok(curvature) = eval::real(curvature, &env)
        {
            env.set(SELF_CURVATURE, curvature);
        }
        // A stroke drawn per pixel can fade with its distance, as a glow does, and its fade is
        // worked out per pixel too.
        if reads(&style.opacity, SELF_DIST) {
            writer.skip(&name, Left::Mark, "opacity", PER_PIXEL);
            continue;
        }
        let opacity = match &style.opacity {
            // One that varies along a stroke is already in its segments' ink.
            Some(_) if style.ink().is_some() => 1.0,
            Some(o) => eval::real(o, &env)?.clamp(0.0, 1.0),
            None => 1.0,
        };
        if opacity <= 0.0 {
            continue;
        }

        // The sides, without the ones worked out per pixel.
        let mut flat = style.clone();
        if reads(&flat.fill_opacity, SELF_DIST) {
            if style.has_sides() {
                writer.skip(&name, Left::Fill, "fill_opacity", PER_PIXEL);
            }
            (flat.negative, flat.positive, flat.inside, flat.outside) = (None, None, None, None);
        }
        for side in [
            &mut flat.negative,
            &mut flat.positive,
            &mut flat.inside,
            &mut flat.outside,
        ] {
            if reads(side, SELF_DIST) {
                writer.skip(&name, Left::Fill, "fill", PER_PIXEL);
                *side = None;
            }
        }
        let stroke = match &style.stroke {
            Some(_) if reads(&style.stroke, SELF_DIST) => {
                writer.skip(&name, Left::Stroke, "stroke", PER_PIXEL);
                None
            }
            // Each line's own color is in its segments.
            Some(_) if crate::style::inked(mark, style) => Some(String::new()),
            Some(color) => Some(hex(eval::color(color, &env)?)),
            None => None,
        };

        let mut elements: Vec<String> = Vec::new();
        let mut filled: Vec<Element> = Vec::new();
        // What a stroke that isn't a line of segments follows: a field's curve, or a dot's edge.
        let mut edge: Vec<Seg> = Vec::new();
        let mut ring = None;
        match &mark.form {
            // Every circle of a collection, in order, from the numbers the shaders get.
            Form::Many { .. } => {
                if let (Some(group), true) = (sided.iter().find(|g| g.mark == i), flat.has_sides())
                {
                    let count = prepared.values[group.slot] as usize;
                    let data = &prepared.sides[group.base * 4..];
                    for row in data.chunks(group.stride * 4).take(count) {
                        let [a, bx, by, c] = [row[0], row[1], row[2], row[3]].map(f64::from);
                        let mut env = env.clone();
                        for (name, value) in group.names.iter().zip(&row[4..]) {
                            env.set(name, f64::from(*value));
                        }
                        env.set(SELF_CURVATURE, a);
                        let radius = per_unit / a.abs();
                        let sides = if radius < STRAIGHT_PX {
                            let (x, y) = frame::device(bx / a, by / a, view, width, height);
                            Sides {
                                outline: Outline::Circle([x, y], radius),
                                negative_inside: a > 0.0,
                            }
                        } else {
                            // The circle's own equation, which is a line's as `a` goes to 0,
                            // taken as straight across the frame.
                            let value = |p: [f64; 2]| {
                                let x = view.x[0] + p[0] / size[0] * (view.x[1] - view.x[0]);
                                let y = view.y[1] - p[1] / size[1] * (view.y[1] - view.y[0]);
                                a * (x * x + y * y) - 2.0 * (bx * x + by * y) + c
                            };
                            let middle = [size[0] / 2.0, size[1] / 2.0];
                            let here = value(middle);
                            let slope = [
                                value([middle[0] + 1.0, middle[1]]) - here,
                                value([middle[0], middle[1] + 1.0]) - here,
                            ];
                            let half = half_frame(size, |p| {
                                here + slope[0] * (p[0] - middle[0]) + slope[1] * (p[1] - middle[1])
                            });
                            // With none of the frame on the negative side, there's no shape.
                            let d = match half.len() < 3 {
                                true => String::new(),
                                false => polygon(&half) + "Z",
                            };
                            Sides {
                                outline: Outline::Path(d, false),
                                negative_inside: true,
                            }
                        };
                        fill_elements(style, &flat, &env, &sides, size, opacity, &mut filled)?;
                    }
                }
            }
            Form::Region(_) if flat.has_sides() => {
                let at = |i: usize, k: usize| f64::from(prepared.geometry[i * 4 + k]);
                let (start, count) = (at(this_region, 0) as usize, at(this_region, 1) as usize);
                if count >= 3 {
                    let vertices: Vec<[f64; 2]> = (start..start + count)
                        .map(|i| [at(i, 0), at(i, 1)])
                        .collect();
                    let sides = Sides {
                        outline: Outline::Path(polygon(&vertices) + "Z", false),
                        negative_inside: true,
                    };
                    fill_elements(style, &flat, &env, &sides, size, opacity, &mut filled)?;
                }
            }
            Form::Dot(at) => {
                let (x, y) = frame::device(
                    eval::real(&at[0], &env)?,
                    eval::real(&at[1], &env)?,
                    view,
                    width,
                    height,
                );
                let radius = frame::dot_radius(style, &env)?;
                let reach = radius + frame::stroke_width(style, &env)? / 2.0 + KEEP;
                let on_frame =
                    x > -reach && y > -reach && x < size[0] + reach && y < size[1] + reach;
                if on_frame && radius > 0.0 {
                    let sides = Sides {
                        outline: Outline::Circle([x, y], radius),
                        negative_inside: true,
                    };
                    fill_elements(style, &flat, &env, &sides, size, opacity, &mut filled)?;
                    ring = Some(([x, y], radius));
                }
            }
            Form::Field { vars, field, .. } => {
                let follows = stroke.is_some() && !is_line;
                if flat.has_sides() || follows {
                    let half = frame::stroke_width(style, &env)? / 2.0;
                    let lines = curve_lines(&mark.form, &env, prepared, width, height, half)?;
                    if follows {
                        for (points, _) in &lines {
                            edge.extend(points.windows(2).filter_map(|pair| {
                                Seg {
                                    a: pair[0],
                                    b: pair[1],
                                    r: [half, half],
                                    s: [0.0, 0.0],
                                    on: 0.0,
                                    period: 0.0,
                                    clip: 0.0,
                                    ink: 1.0,
                                    cut: [false; 2],
                                    row: 0,
                                }
                                .within(size[0], size[1])
                            }));
                        }
                    }
                    let loops: Vec<Vec<[f64; 2]>> = lines
                        .iter()
                        .filter(|(points, closed)| *closed && points.len() > 3)
                        .map(|(points, _)| points.clone())
                        .collect();
                    if flat.has_sides() && (loops.is_empty() || loops.len() < lines.len()) {
                        writer.skip(
                            &name,
                            Left::Fill,
                            "fill",
                            "is the side of a curve that doesn't close inside the frame, which \
                             isn't made into a shape yet",
                        );
                    } else if flat.has_sides() {
                        // Which side the loops hold: the field's sign just left of the first
                        // one's start, against whether that's inside them.
                        let (a, b) = (loops[0][0], loops[0][1]);
                        let length = (b[0] - a[0]).hypot(b[1] - a[1]).max(1e-9);
                        let probe = [
                            (a[0] + b[0]) / 2.0 + (b[1] - a[1]) / length * 0.5,
                            (a[1] + b[1]) / 2.0 - (b[0] - a[0]) / length * 0.5,
                        ];
                        let mut there = env.clone();
                        there.set(
                            &vars[0],
                            view.x[0] + probe[0] / size[0] * (view.x[1] - view.x[0]),
                        );
                        there.set(
                            &vars[1],
                            view.y[1] - probe[1] / size[1] * (view.y[1] - view.y[0]),
                        );
                        let negative = eval::real(field, &there)? < 0.0;
                        let inside = crossings(probe, &loops) % 2 == 1;
                        let mut d = String::new();
                        for points in &loops {
                            d.push_str(&polygon(&points[..points.len() - 1]));
                            d.push('Z');
                        }
                        let sides = Sides {
                            outline: Outline::Path(d, loops.len() > 1),
                            negative_inside: negative == inside,
                        };
                        fill_elements(style, &flat, &env, &sides, size, opacity, &mut filled)?;
                    }
                }
            }
            _ => {}
        }
        elements.extend(filled.iter().map(close));

        if let Some(color) = &stroke {
            let lined;
            let pieces: &[Seg] = if is_line {
                lined = segments(
                    &prepared.strokes[this_layer],
                    prepared.inks.get(this_layer).map_or(&[], |rows| rows),
                    size[0],
                    size[1],
                );
                &lined
            } else {
                &edge
            };
            let mut drawn = Vec::new();
            let mut ink = opacity;
            // A collection whose lines each have their own color is drawn a line at a time.
            let runs: Vec<(&[Seg], String)> = match crate::style::inked(mark, style) {
                true => pieces
                    .chunk_by(|a, b| a.row == b.row)
                    .filter_map(|run| {
                        let row = run[0].row.checked_sub(1)?;
                        let color =
                            std::array::from_fn(|k| f64::from(prepared.colors[row * 4 + k]));
                        Some((run, hex(color)))
                    })
                    .collect(),
                false => vec![(pieces, color.clone())],
            };
            if dashes_a_taper(pieces) {
                writer.skip(
                    &name,
                    Left::Stroke,
                    "dash",
                    "is on a stroke whose `width` varies along it, which isn't cut into dashes yet",
                );
            } else {
                for (pieces, color) in &runs {
                    match stroke_elements(pieces, color) {
                        Stroke::Even(elements, even) => {
                            drawn.extend(elements);
                            ink = opacity * even;
                        }
                        Stroke::Masked(contents) if !contents.is_empty() => {
                            let mask = writer.id(&format!("{name}.ink"));
                            let mut group = format!(
                                "<mask id=\"{mask}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\">\n",
                                w = num(size[0]),
                                h = num(size[1]),
                            );
                            for element in &contents {
                                let _ = writeln!(group, "{}", close(element));
                            }
                            group.push_str("</mask>");
                            elements.push(group);
                            drawn.push(format!(
                                "<rect width=\"{}\" height=\"{}\" fill=\"{color}\" mask=\"url(#{mask})\"",
                                num(size[0]),
                                num(size[1]),
                            ));
                        }
                        Stroke::Masked(_) => {}
                    }
                }
            }
            // A dot's stroke is a ring on its edge.
            if let (Some((at, radius)), false) = (ring, is_line) {
                let width = frame::stroke_width(style, &env)?;
                if width > 0.0 {
                    drawn.push(format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"",
                        num(at[0]),
                        num(at[1]),
                        num(radius),
                        num(width)
                    ));
                }
            }
            // Pieces of one stroke overlap, so they fade together, as one layer.
            if ink > 0.0 {
                match (ink < 1.0, drawn.len()) {
                    (false, _) => elements.extend(drawn.iter().map(close)),
                    (true, 1) => {
                        elements.push(format!("{} opacity=\"{}\"/>", drawn[0], alpha(ink)))
                    }
                    (true, _) => {
                        let mut group = format!("<g opacity=\"{}\">\n", alpha(ink));
                        for element in &drawn {
                            let _ = writeln!(group, "{}", close(element));
                        }
                        group.push_str("</g>");
                        if !drawn.is_empty() {
                            elements.push(group);
                        }
                    }
                }
            }
        }
        let id = writer.id(&name);
        named(&id, elements, out);
    }
    // A tone that's part way to `tanh` is at none of the way while its number is 0.
    let toned = match &figure.styles.tone {
        Tone::Clip => false,
        Tone::Tanh => true,
        Tone::Mix(k) => eval::real(k, &frame_env)? > 0.0,
    };
    if toned {
        writer.skip(
            "paper",
            Left::Property,
            "tone",
            "is worked out per pixel from everything drawn, so colors are as their styles give them",
        );
    }
    Ok(())
}

/// Labels as text, in the order a frame burns them in: leaders, then every halo, then every
/// label, so one label's halo can't eat into its neighbor. A label is centered on its place,
/// whatever font a viewer has, except the ones named in `from_left`, which start at theirs.
fn text(
    writer: &mut Writer,
    labels: &[Label],
    from_left: &HashSet<String>,
    paper: [f64; 3],
    widths: &dyn TextWidths,
) -> String {
    let visible: Vec<&Label> = labels
        .iter()
        .filter(|l| l.opacity > 0.0 && !l.text.is_empty())
        .collect();
    let faded = |label: &Label| match label.opacity < 1.0 {
        true => format!(" opacity=\"{}\"", alpha(label.opacity)),
        false => String::new(),
    };
    let set = |label: &Label| {
        let (family, style) = match label.font {
            Font::Sans => (SANS, ""),
            Font::Math => (MATH, " font-style=\"italic\""),
        };
        let (x, anchor) = match from_left.contains(&label.id) {
            true => (label.x - label.half(widths)[0], " text-anchor=\"start\""),
            false => (label.x, ""),
        };
        format!(
            "x=\"{}\" y=\"{}\"{anchor} font-family=\"{family}\" font-size=\"{}\"{style}",
            num(x),
            num(label.y + label.size * BASELINE),
            num(label.size),
        )
    };
    let mut out = String::new();
    for label in &visible {
        let half = label.half(widths);
        if let (Some([a, b]), Some(leader)) = (frame::leader_ends(label, half), label.leader) {
            let _ = writeln!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
                num(a[0]),
                num(a[1]),
                num(b[0]),
                num(b[1]),
                hex(label.color),
                num(leader.width),
                faded(label),
            );
        }
    }
    let paper = hex(paper);
    for label in visible.iter().filter(|l| l.halo > 0.0) {
        // Like SVG's stroke-width, `halo` is the full width, centered on the outline.
        let _ = writeln!(
            out,
            "<text {} fill=\"{paper}\" stroke=\"{paper}\" stroke-width=\"{}\" stroke-linejoin=\"round\"{} aria-hidden=\"true\">{}</text>",
            set(label),
            num(label.halo),
            faded(label),
            escape(&label.words()),
        );
    }
    for label in &visible {
        let _ = writeln!(
            out,
            "<text id=\"{}\" {} fill=\"{}\"{}>{}</text>",
            writer.id(&label.id),
            set(label),
            hex(label.color),
            faded(label),
            escape(&label.words()),
        );
    }
    if out.is_empty() {
        return out;
    }
    format!("<g text-anchor=\"middle\">\n{out}</g>\n")
}

impl Composition {
    /// One frame as an SVG document, at the size its panels were prepared for, with `labels`
    /// as text, and what its styles left it unable to draw. `widths` are the font's that the
    /// labels were placed with, for where a leader stops short of its text.
    pub fn svg(
        &self,
        frames: &[PanelFrame],
        inputs: &Inputs,
        width: u32,
        height: u32,
        labels: &[Label],
        widths: &dyn TextWidths,
    ) -> Result<Drawn, Error> {
        let mut writer = Writer::default();
        let paper = frames.first().map_or([1.0; 3], |f| f.prepared.paper);
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n\
             <rect width=\"{width}\" height=\"{height}\" fill=\"{}\"/>\n",
            hex(paper)
        );
        let alone = self.panels.len() == 1;
        for (panel, frame) in self.panels.iter().zip(frames) {
            let prefix = match panel.id.is_empty() {
                true => String::new(),
                false => format!("{}/", panel.id),
            };
            let mut body = String::new();
            marks(
                &mut writer,
                &prefix,
                &panel.figure,
                inputs,
                &frame.prepared,
                frame.width,
                frame.height,
                &mut body,
            )?;
            if alone {
                svg.push_str(&body);
                continue;
            }
            // A panel is a window: what runs past its edge is cut there.
            let id = writer.id(&panel.id);
            let clip = writer.id(&format!("{}.frame", panel.id));
            let (w, h) = (frame.width, frame.height);
            let _ = writeln!(
                svg,
                "<clipPath id=\"{clip}\"><rect width=\"{w}\" height=\"{h}\"/></clipPath>\n\
                 <g id=\"{id}\" transform=\"translate({} {})\" clip-path=\"url(#{clip})\">",
                frame.origin[0], frame.origin[1],
            );
            if frame.prepared.paper != paper {
                let _ = writeln!(
                    svg,
                    "<rect width=\"{w}\" height=\"{h}\" fill=\"{}\"/>",
                    hex(frame.prepared.paper)
                );
            }
            svg.push_str(&body);
            svg.push_str("</g>\n");
        }
        // A panel's title and the formula beside it sit at its left edge, and a viewer's font
        // can be wider than the one that placed them.
        let from_left: HashSet<String> = self
            .panels
            .iter()
            .flat_map(|p| [format!("{}/title", p.id), format!("{}/formula", p.id)])
            .collect();
        svg.push_str(&text(&mut writer, labels, &from_left, paper, widths));
        svg.push_str("</svg>\n");
        Ok(Drawn {
            svg,
            skipped: writer.skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terms::Estimate;

    /// A document under a sheet, 200 pixels square at time 0. Its window is 4 units across, so a
    /// unit is 50 pixels.
    fn draw(doc: &str, sheet: &str) -> Drawn {
        let doc = format!("viewport {{ x: 0..4, y: 0..4 }}\n{doc}");
        let composition = crate::compose(&doc, &[sheet]).unwrap();
        let inputs = Inputs::at(0.0);
        let frames = composition
            .prepare(&inputs, 200, 200, 1.0, None, &Estimate)
            .unwrap();
        let labels = composition.labels(&frames, &inputs, &Estimate).unwrap();
        composition
            .svg(&frames, &inputs, 200, 200, &labels, &Estimate)
            .unwrap()
    }

    /// The element or group with an id, up to the end of its line.
    fn named<'a>(svg: &'a str, id: &str) -> &'a str {
        let at = svg
            .find(&format!(" id=\"{id}\""))
            .unwrap_or_else(|| panic!("no `{id}` in:\n{svg}"));
        let start = svg[..at].rfind('<').unwrap();
        svg[start..].lines().next().unwrap()
    }

    #[test]
    fn a_later_range_can_read_a_list() {
        // k counts up to s, which is 1 and then 2: three dots. And up to a point's y, 1 and
        // then 3: six.
        let drawn = draw(
            "dots d = (k, s) for s in {1, 2}, k in 0..s",
            "style d { stroke: black; radius: 2px }",
        );
        let found: Vec<(f64, f64)> = drawn
            .svg
            .split("<circle")
            .skip(1)
            .map(|c| {
                let at = |k: &str| -> f64 {
                    let v = c.split(&format!(" {k}=\"")).nth(1).unwrap();
                    v.split('"').next().unwrap().parse().unwrap()
                };
                (at("cx"), at("cy"))
            })
            .collect();
        // A unit is 50 pixels, and y is down from 4.
        let px = |x: f64, y: f64| (x * 50.0, (4.0 - y) * 50.0);
        assert_eq!(
            found,
            [
                px(0.0, 1.0),
                px(1.0, 1.0),
                px(0.0, 2.0),
                px(1.0, 2.0),
                px(2.0, 2.0)
            ]
        );
        let drawn = draw(
            "dots d = (k, p.y) for p in {(0, 1), (0, 3)}, k in 0..p.y",
            "style d { stroke: black; radius: 2px }",
        );
        assert_eq!(drawn.svg.matches("<circle").count(), 2 + 4, "{}", drawn.svg);
    }

    #[test]
    fn marks_are_shapes_under_their_names_and_labels_are_text() {
        let drawn = draw(
            "let P = (1, 1)
             curve c = circle((2, 2), (3, 2))
             segment s = (0, 0), (4, 4)
             dot P
             label n = \"P < Q\" at P",
            "style paper { fill: rgb(242, 240, 235) }
             style c::inside { fill: rgb(0, 104, 181) }
             style c { stroke: black; width: 2px; fill_opacity: 0.25 }
             style s { stroke: rgb(171, 58, 51); width: 1.5px; dash: 6px; gap: 4px; opacity: 0.5 }
             style P { fill: black; radius: 4px }
             style n { fill: rgb(96, 66, 166); halo: 3px; place: fixed }",
        );
        assert_eq!(drawn.skipped, []);
        let svg = &drawn.svg;
        assert!(svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\" viewBox=\"0 0 200 200\">\n\
             <rect width=\"200\" height=\"200\" fill=\"#f2f0eb\"/>\n"
        ));
        // The circle's disk, and its edge as a closed path two pixels wide.
        assert_eq!(named(svg, "c"), "<g id=\"c\">");
        assert!(
            svg.contains("fill=\"#0068b5\" fill-opacity=\"0.25\"/>"),
            "{svg}"
        );
        assert!(
            svg.contains("Z\" fill=\"none\" stroke=\"#000000\" stroke-width=\"2\" stroke-linejoin=\"round\"/>"),
            "{svg}"
        );
        // A segment ends square, as SVG's does, and carries its dashes and its opacity.
        assert_eq!(
            named(svg, "s"),
            "<path id=\"s\" d=\"M0 200L200 0\" fill=\"none\" stroke=\"#ab3a33\" stroke-width=\"1.5\" \
             stroke-linejoin=\"round\" stroke-dasharray=\"6 4\" opacity=\"0.5\"/>"
        );
        assert_eq!(
            named(svg, "P"),
            "<circle id=\"P\" cx=\"50\" cy=\"150\" r=\"4\" fill=\"#000000\"/>"
        );
        // The label is text, over its halo in the paper's color, with its name on the text.
        let label = named(svg, "n");
        assert!(label.starts_with("<text id=\"n\" x=\"50\" "), "{label}");
        assert!(
            label.ends_with("fill=\"#6042a6\">P &lt; Q</text>"),
            "{label}"
        );
        assert!(
            svg.contains("fill=\"#f2f0eb\" stroke=\"#f2f0eb\" stroke-width=\"3\" stroke-linejoin=\"round\" aria-hidden=\"true\">P &lt; Q</text>"),
            "{svg}"
        );
    }

    #[test]
    fn what_is_worked_out_per_pixel_is_left_out_and_named() {
        let doc = "curve c = circle((2, 2), (3, 2))\ncurve glow = circle((1, 1), (1, 2))";
        let drawn = draw(
            doc,
            "style paper { fill: black; tone: tanh }
             style c::inside { fill: mix(white, black, 0.5 + 0.5·cos(9·self.dist)) }
             style c { stroke: white; width: 2px }
             style glow { blend: add; stroke: white }
             style glow::inside { fill: white }",
        );
        let said: Vec<String> = drawn.skipped.iter().map(ToString::to_string).collect();
        assert_eq!(
            said,
            [
                "`c`'s fill is left out: its `fill` reads `self.dist`, which is worked out per pixel",
                "`glow` is left out: its `blend` is `add`, which adds light, and SVG has no paint that does",
                "`paper` is drawn without its `tone`, which is worked out per pixel from everything \
                 drawn, so colors are as their styles give them",
            ]
        );
        assert_eq!(
            (drawn.skipped[0].left, drawn.skipped[0].property),
            (Left::Fill, "fill")
        );
        // The rest of the mark is still there: its plain stroke.
        let c = named(&drawn.svg, "c");
        assert!(c.starts_with("<path id=\"c\" d=\"M150 100L"), "{c}");
        assert!(c.contains("stroke=\"#ffffff\" stroke-width=\"2\""), "{c}");
        assert!(!drawn.svg.contains("glow"), "{}", drawn.svg);

        // A fade with distance takes every side with it, and so does a stroke that reads it.
        let drawn = draw(
            doc,
            "style c::inside { fill: white }
             style c { fill_opacity: tanh(self.pixel / abs(self.dist)) }
             style glow { stroke: mix(white, black, self.dist) }",
        );
        let left: Vec<_> = drawn
            .skipped
            .iter()
            .map(|s| (s.mark.as_str(), s.left, s.property))
            .collect();
        assert_eq!(
            left,
            [
                ("c", Left::Fill, "fill_opacity"),
                ("glow", Left::Stroke, "stroke")
            ]
        );
        assert!(
            !drawn.svg.contains("<path") && !drawn.svg.contains("<g"),
            "{}",
            drawn.svg
        );

        // A curve drawn per pixel whose opacity fades with its distance, as a glow does.
        let drawn = draw(
            "curve p = parametric (t): (2 + cos(t), 2 + sin(t)) for t in 0..2·pi",
            "style p { draw: field; stroke: white; opacity: tanh(self.pixel / abs(self.dist)) }",
        );
        let left: Vec<_> = drawn
            .skipped
            .iter()
            .map(|s| (s.mark.as_str(), s.left, s.property))
            .collect();
        assert_eq!(left, [("p", Left::Mark, "opacity")]);
    }

    #[test]
    fn a_width_that_varies_along_a_stroke_is_an_outline() {
        let drawn = draw(
            "curve w = parametric (t): (t, 2) for t in 0..4",
            "style w { stroke: black; width: 1px + 9px·self.t }",
        );
        assert_eq!(drawn.skipped, []);
        let w = named(&drawn.svg, "w");
        // A shape and not a stroke: half a pixel either side at its start, five at its end.
        assert!(w.starts_with("<path id=\"w\" d=\"M0 100.5L"), "{w}");
        assert!(w.contains(" 200 105 200 95 "), "{w}");
        assert!(w.ends_with(" 0 99.5Z\" fill=\"#000000\"/>"), "{w}");
        assert!(!w.contains("stroke"), "{w}");

        // Dashes along it would need the outline cut, which isn't built, so it says so.
        let drawn = draw(
            "curve w = parametric (t): (t, 2) for t in 0..4",
            "style w { stroke: black; width: 1px + 9px·self.t; dash: 5px }",
        );
        assert_eq!(drawn.skipped.len(), 1);
        assert_eq!(
            (drawn.skipped[0].left, drawn.skipped[0].property),
            (Left::Stroke, "dash")
        );
        assert!(!drawn.svg.contains("id=\"w\""), "{}", drawn.svg);
    }

    #[test]
    fn an_opacity_that_varies_along_a_stroke_is_a_mask() {
        let drawn = draw(
            "curve w = parametric (t): (t, 2) for t in 0..4",
            "style w { stroke: rgb(0, 104, 181); width: 4px; opacity: self.t }",
        );
        assert_eq!(drawn.skipped, []);
        let svg = &drawn.svg;
        assert!(
            svg.contains("<mask id=\"w.ink\" maskUnits=\"userSpaceOnUse\""),
            "{svg}"
        );
        assert!(
            svg.contains(
                "<rect width=\"200\" height=\"200\" fill=\"#0068b5\" mask=\"url(#w.ink)\"/>"
            ),
            "{svg}"
        );
        // Grays from nearly none of its ink to all of it, the lightest last.
        let grays: Vec<&str> = svg
            .lines()
            .filter(|l| l.starts_with("<path") && l.contains("stroke-width=\"4\""))
            .filter_map(|l| l.split("stroke=\"").nth(1)?.split('"').next())
            .collect();
        assert!(grays.len() > 32, "{}", grays.len());
        assert_eq!(grays.last(), Some(&"#ffffff"));
        assert!(grays.is_sorted(), "{grays:?}");
    }

    #[test]
    fn an_arrow_is_its_shaft_and_a_triangle() {
        let drawn = draw(
            "arrow a = (0, 2), (4, 2)",
            "style a { stroke: black; width: 2px; head: 20px }",
        );
        let lines: Vec<&str> = drawn.svg.lines().collect();
        let at = lines.iter().position(|l| *l == "<g id=\"a\">").unwrap();
        // The shaft stops inside the head, which is 20 long and 0.45 of that to each side.
        assert_eq!(
            lines[at + 1],
            "<path d=\"M0 100L184 100\" fill=\"none\" stroke=\"#000000\" stroke-width=\"2\" stroke-linejoin=\"round\"/>"
        );
        assert_eq!(
            lines[at + 2],
            "<path d=\"M180 109L200 100 180 91Z\" fill=\"#000000\"/>"
        );
    }

    #[test]
    fn each_circle_of_a_collection_fills_its_own_side() {
        let drawn = draw(
            "circles rims = circle((n, 2), (n + 0.5, 2)) for n in 1..3",
            "style rims::inside { fill: black }
             style rims { stroke: black; fill_opacity: 0.1·n }",
        );
        assert_eq!(drawn.skipped, []);
        for (x, opacity) in [(50, "0.1"), (100, "0.2"), (150, "0.3")] {
            let disk = format!(
                "<circle cx=\"{x}\" cy=\"100\" r=\"25\" fill=\"#000000\" fill-opacity=\"{opacity}\"/>"
            );
            assert!(drawn.svg.contains(&disk), "{disk}\n{}", drawn.svg);
        }
    }

    #[test]
    fn panels_are_windows_and_their_titles_start_at_their_left() {
        let composition = crate::compose(
            "panel a \"Left\" { viewport { x: 0..1, y: 0..1 } segment s = (0, 0), (1, 1) }
             panel b \"Right\" { viewport { x: 0..1, y: 0..1 } segment s = (0, 0), (1, 1) }",
            &["style panels { columns: 2; gap: 10px; title: 20px } style panel { width: 100px; height: 100px }
               style s { stroke: black }"],
        )
        .unwrap();
        let inputs = Inputs::at(0.0);
        let frames = composition
            .prepare(&inputs, 210, 120, 1.0, None, &Estimate)
            .unwrap();
        let labels = composition.labels(&frames, &inputs, &Estimate).unwrap();
        let svg = composition
            .svg(&frames, &inputs, 210, 120, &labels, &Estimate)
            .unwrap()
            .svg;
        assert!(
            svg.contains(
                "<g id=\"b\" transform=\"translate(110 20)\" clip-path=\"url(#b.frame)\">"
            ),
            "{svg}"
        );
        // The same mark in each panel, under each panel's name.
        assert!(named(&svg, "a-s").contains("d=\"M0 100L100 0\""), "{svg}");
        assert!(named(&svg, "b-s").contains("d=\"M0 100L100 0\""), "{svg}");
        let title = named(&svg, "b-title");
        assert!(
            title.starts_with("<text id=\"b-title\" x=\"110\" "),
            "{title}"
        );
        assert!(title.contains(" text-anchor=\"start\" "), "{title}");
    }
}
