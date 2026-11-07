//! Resolves stylesheets against a scene. Style values are expressions, so a fill
//! can depend on `time`, a param or `self.dist`. Nothing here changes the scene's IR.
//!
//! Facets: `::negative` and `::positive` are the field's raw sign. On a circle, `::inside` and
//! `::outside` are its true disk and the rest, whichever way round it runs. Elsewhere, they're
//! aliases for the sign under the gradient convention, where the negative side is inside. Later declarations win (SPEC/15's layers, without
//! specificity).

use std::collections::HashMap;

use crate::compile::{BEAT_PARAM, CONSTANTS, SIGNALS};
use crate::error::Error;
use crate::frame::VIEW_WIDTH;
use crate::ir::{Expr, Form, Scene, Shape};
use crate::parse::{Document, Item};

/// The distance to a mark, per pixel. Only legal in spatial properties.
pub const SELF_DIST: &str = "self.dist";
/// Arc-length position along a stroke, 0 to 1. Only legal in `width`.
pub const SELF_T: &str = "self.t";
/// How big a device pixel is, in world units, wherever `self.dist` can be read.
pub const SELF_PIXEL: &str = "self.pixel";
/// A circle's signed curvature, which is below 0 when it runs clockwise and 0 on a line.
pub const SELF_CURVATURE: &str = "self.curvature";
/// Style pixels per world unit, in a collection's sizes.
pub const SELF_SCALE: &str = "self.scale";
/// How many beats ago this mark's beat was: negative before it, 0 during it.
pub const SELF_AGE: &str = "self.age";
/// Every property a style can set. `docs/language.md` must describe each one, and a test
/// holds it to that.
pub const PROPERTIES: [&str; 19] = [
    "fill",
    "fill_opacity",
    "stroke",
    "width",
    "dash",
    "gap",
    "cap",
    "draw",
    "blend",
    "opacity",
    "radius",
    "size",
    "halo",
    "font",
    "place",
    "show",
    "head",
    "head_span",
    "distance",
];
pub const COLORS: [&str; 2] = ["white", "black"];
/// The tags that say what kind of mark something is.
const MARK_TYPES: [&str; 14] = [
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
    "outline",
];

/// The space a mark's edges blend in. The default is gamma-encoded sRGB, because
/// that's what SVG and CSS do and SVG's look is the quality bar. Linear light makes
/// dark lines on a light ground visibly thinner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Blend {
    #[default]
    Gamma,
    Linear,
    /// Adds to what's underneath, in display space, with no limit, as a shader's `col +=` does.
    /// What's drawn this way usually wants a `tone` on the paper.
    Add,
}

/// What a figure does with its finished color, for a figure that adds light.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Tone {
    /// Past 1 is white.
    #[default]
    Clip,
    /// tanh of each channel, in display space, which rolls off smoothly.
    Tanh,
}

/// A rendering hint. It chooses how a stroke is drawn, never what it means, so every
/// choice has to agree with the others within tolerance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Draw {
    /// The form's fast path: an exact SDF per pixel, anything else as a line.
    #[default]
    Auto,
    /// From the field's distance per pixel. Only for curves with a field.
    Field,
    /// As a line of segments: sampled if parametric, traced if implicit.
    Line,
}

/// How a line ends. Square is SVG's default, and round is what the blog uses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Cap {
    #[default]
    Butt,
    Round,
}

/// A label's typeface, as a role. What each role maps to belongs to the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Font {
    #[default]
    Sans,
    /// Italic serif, for math.
    Math,
}

/// How a label finds its spot. See `place.rs`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Place {
    /// At its offset from its point.
    #[default]
    Fixed,
    /// Tied to that spot by a spring, and pushed off what's drawn and other labels.
    Settle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkStyle {
    pub negative: Option<Expr>,
    pub positive: Option<Expr>,
    /// A circle's true disk, whichever sign it has, and the rest.
    pub inside: Option<Expr>,
    pub outside: Option<Expr>,
    pub stroke: Option<Expr>,
    /// In style pixels. Doesn't vary per pixel.
    pub width: Option<Expr>,
    pub draw: Draw,
    pub dash: Option<Expr>,
    pub gap: Option<Expr>,
    pub blend: Blend,
    pub cap: Cap,
    pub font: Font,
    pub opacity: Option<Expr>,
    /// Scales only the fill, on top of `opacity`.
    pub fill_opacity: Option<Expr>,
    /// A dot's radius in style pixels.
    pub radius: Option<Expr>,
    /// A label's text size in style pixels.
    pub size: Option<Expr>,
    /// The width of a label's halo in style pixels.
    pub halo: Option<Expr>,
    /// How far a label sits from its point, when the document says which way.
    pub distance: Option<Expr>,
    pub place: Place,
    /// Draws the mark only while this is above 0, decided once per frame.
    pub show: Option<Expr>,
    /// An arrow's head length in style pixels. A curve with one ends in an arrowhead.
    pub head: Option<Expr>,
    /// How much of a curve's parameter its head may cover, at most: 60% of that length.
    pub head_span: Option<Expr>,
}

impl MarkStyle {
    /// Whether this mark's stroke needs a line of segments, for this form.
    pub fn stroke_is_line(&self, form: &Form) -> bool {
        if self.stroke.is_none() {
            return false;
        }
        match form {
            Form::Field { exact, .. } => match self.draw {
                Draw::Line => true,
                Draw::Field => false,
                Draw::Auto => !exact || self.needs_arc_length(),
            },
            Form::Parametric(_)
            | Form::Polyline { .. }
            | Form::Region(_)
            | Form::Arrow { .. }
            | Form::Outline { .. }
            | Form::Many { .. } => true,
            Form::Dot(_) | Form::Label { .. } | Form::Term { .. } => false,
        }
    }

    /// Whether this colors the sides of what it styles.
    pub fn has_sides(&self) -> bool {
        self.negative.is_some()
            || self.positive.is_some()
            || self.inside.is_some()
            || self.outside.is_some()
    }

    /// What a side's color can read per instance.
    pub fn side_exprs(&self) -> impl Iterator<Item = &Expr> {
        [
            &self.negative,
            &self.positive,
            &self.inside,
            &self.outside,
            &self.fill_opacity,
        ]
        .into_iter()
        .flatten()
    }

    /// How much of each side's sign this style's `::negative` and `::positive` still get, for a
    /// circle that has `a >= 0` and one that doesn't. Where `::inside` or `::outside` name the
    /// same part of the plane, those win.
    pub fn sign_gates(&self) -> [[f64; 2]; 2] {
        let open = |taken: &Option<Expr>| if taken.is_some() { 0.0 } else { 1.0 };
        [
            [open(&self.inside), open(&self.outside)],
            [open(&self.outside), open(&self.inside)],
        ]
    }

    /// Every expression this style evaluates.
    pub fn exprs(&self) -> impl Iterator<Item = &Expr> {
        [
            &self.negative,
            &self.positive,
            &self.inside,
            &self.outside,
            &self.stroke,
            &self.width,
            &self.dash,
            &self.gap,
            &self.opacity,
            &self.fill_opacity,
            &self.radius,
            &self.size,
            &self.halo,
            &self.show,
            &self.head,
            &self.head_span,
        ]
        .into_iter()
        .flatten()
    }

    /// Dashes and tapers depend on position along the curve, which a field can't give.
    pub fn needs_arc_length(&self) -> bool {
        self.dash.is_some()
            || self.gap.is_some()
            || self.width.as_ref().is_some_and(|w| uses(w, SELF_T))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Styled {
    /// One entry per `scene.marks`, in the same order.
    pub marks: Vec<MarkStyle>,
    pub paper: Option<Expr>,
    pub tone: Tone,
    /// A panel title's color and size, and the color of the formula beside it.
    pub title_fill: Option<Expr>,
    pub title_size: Option<Expr>,
    pub formula_fill: Option<Expr>,
    pub panels: PanelStyle,
}

/// How panels sit on the canvas, in design pixels: `style panels` for the whole, and
/// `style panel` for each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelStyle {
    pub columns: Option<Expr>,
    pub gap: Option<Expr>,
    pub title: Option<Expr>,
    pub width: Option<Expr>,
    pub height: Option<Expr>,
    pub pad: Option<Expr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    Real,
    Color,
}

fn has_pixels(e: &Expr) -> bool {
    match e {
        Expr::App(head, args) => head == "Pixels" || args.iter().any(has_pixels),
        _ => false,
    }
}

fn uses(e: &Expr, name: &str) -> bool {
    let mut found = false;
    e.visit_syms(&mut |s| found |= s == name);
    found
}

/// Checks an expression's type, and that it only uses names in scope.
pub fn type_of(e: &Expr, names: &[&str], spatial: bool) -> Result<Ty, String> {
    match e {
        Expr::Num(_) => Ok(Ty::Real),
        Expr::Sym(s) if s == SELF_DIST || s == SELF_PIXEL || s == SELF_CURVATURE => {
            if spatial {
                Ok(Ty::Real)
            } else {
                Err(format!(
                    "`{s}` is only legal in spatial properties (`fill`, `stroke`, `fill_opacity`)"
                ))
            }
        }
        Expr::Sym(s) if COLORS.contains(&s.as_str()) => Ok(Ty::Color),
        Expr::Sym(s)
            if names.contains(&s.as_str())
                || SIGNALS.contains(&s.as_str())
                || CONSTANTS.contains(&s.as_str()) =>
        {
            Ok(Ty::Real)
        }
        Expr::Sym(s) => Err(format!("unknown name `{s}`")),
        Expr::App(head, args) => {
            let tys = args
                .iter()
                .map(|a| type_of(a, names, spatial))
                .collect::<Result<Vec<_>, _>>()?;
            use Ty::{Color, Real};
            match (head.as_str(), tys.as_slice()) {
                ("Oklch" | "Rgb", [Real, Real, Real]) => Ok(Color),
                ("Hue", [Real]) => Ok(Color),
                ("Mix", [Color, Color, Real]) => Ok(Color),
                ("Mix" | "Clamp" | "Smoothstep" | "Reveal", [Real, Real, Real]) => Ok(Real),
                ("Multiply", [Color, Real] | [Real, Color]) | ("Divide", [Color, Real]) => {
                    Ok(Color)
                }
                ("Add" | "Subtract", [Color, Color]) => Ok(Color),
                ("Snap", [Real] | [Real, Real]) => Ok(Real),
                (
                    "Add" | "Subtract" | "Multiply" | "Divide" | "Power" | "Min" | "Max" | "Mod"
                    | "Arctan2",
                    [Real, Real],
                ) => Ok(Real),
                (
                    "Negate" | "Sqrt" | "Sin" | "Cos" | "Tan" | "Arccos" | "Exp" | "Ln" | "Abs"
                    | "Sign" | "Floor" | "Pixels" | "Sinh" | "Cosh" | "Tanh",
                    [Real],
                ) => Ok(Real),
                _ => Err(format!("`{head}` doesn't take {tys:?}")),
            }
        }
    }
}

/// Replaces a style sheet's own names with what they stand for, so a theme can name its
/// colors and sizes and nothing downstream needs to know.
fn with_names(e: &Expr, names: &HashMap<String, Expr>) -> Expr {
    match e {
        Expr::Sym(s) => names.get(s).cloned().unwrap_or_else(|| e.clone()),
        Expr::App(head, args) => Expr::App(
            head.clone(),
            args.iter().map(|a| with_names(a, names)).collect(),
        ),
        other => other.clone(),
    }
}

/// Replaces `self.age` with this mark's own expression: `beat` minus its beat's index.
/// A mark outside any beat is always current. Done symbolically, so the runtime needs
/// nothing new.
fn with_age(e: &Expr, beat: Option<usize>) -> Expr {
    match e {
        Expr::Sym(s) if s == SELF_AGE => match beat {
            Some(k) => Expr::app("Subtract", vec![Expr::sym(BEAT_PARAM), Expr::Num(k as f64)]),
            None => Expr::Num(0.0),
        },
        Expr::App(head, args) => Expr::App(
            head.clone(),
            args.iter().map(|a| with_age(a, beat)).collect(),
        ),
        other => other.clone(),
    }
}

/// The values sheets name. A later sheet's definition replaces an earlier one's everywhere,
/// including in rules written before it, so a sheet can change what `ink` means for a whole
/// figure. A definition can use any other name, in any order.
pub fn sheet_names(sheets: &[Document]) -> Result<HashMap<String, Expr>, Error> {
    let mut written: HashMap<&str, (&Expr, usize)> = HashMap::new();
    for sheet in sheets {
        for item in &sheet.items {
            if let Item::Let { name, expr, line } = item {
                written.insert(name, (expr, *line));
            }
        }
    }
    fn expand(
        name: &str,
        written: &HashMap<&str, (&Expr, usize)>,
        done: &mut HashMap<String, Expr>,
        visiting: &mut Vec<String>,
    ) -> Result<(), Error> {
        if done.contains_key(name) {
            return Ok(());
        }
        let (expr, line) = written[name];
        if visiting.iter().any(|v| v == name) {
            return Err(Error::at(
                line,
                1,
                format!("`{name}` is defined in terms of itself"),
            ));
        }
        visiting.push(name.to_string());
        let mut used = Vec::new();
        expr.visit_syms(&mut |s| used.push(s.to_string()));
        for other in used {
            if written.contains_key(other.as_str()) {
                expand(&other, written, done, visiting)?;
            }
        }
        visiting.pop();
        done.insert(name.to_string(), with_names(expr, done));
        Ok(())
    }
    let mut done = HashMap::new();
    for name in written.keys() {
        expand(name, &written, &mut done, &mut Vec::new())?;
    }
    Ok(done)
}

pub fn resolve(scene: &Scene, sheets: &[Document]) -> Result<Styled, Error> {
    let mut unmatched = Vec::new();
    let styled = resolve_keeping(scene, None, sheets, &mut unmatched)?;
    match unmatched.into_iter().next() {
        Some(error) => Err(error),
        None => Ok(styled),
    }
}

/// `resolve`, with the rules that name a mark this scene doesn't have set aside, since with
/// panels a name only has to be in one of them.
pub fn resolve_keeping(
    scene: &Scene,
    panel: Option<&str>,
    sheets: &[Document],
    unmatched: &mut Vec<Error>,
) -> Result<Styled, Error> {
    let mut styled = Styled {
        marks: vec![MarkStyle::default(); scene.marks.len()],
        ..Styled::default()
    };
    let names: Vec<&str> = scene.bindings.iter().map(|b| b.id.as_str()).collect();
    // The document comes last, and its own `let`s are bindings.
    let sheet_names = sheet_names(&sheets[..sheets.len().saturating_sub(1)])?;

    for sheet in sheets {
        for item in &sheet.items {
            let Item::Style(rule) = item else { continue };
            let at = |message: String| Error::at(rule.line, 1, message);
            let decls: Vec<(String, Expr)> = rule
                .decls
                .iter()
                .map(|(prop, value)| (prop.clone(), with_names(value, &sheet_names)))
                .collect();
            for selector in &rule.selectors {
                if selector.target == "panels" && selector.tags.is_empty() {
                    for (prop, value) in &decls {
                        let slot = match prop.as_str() {
                            "columns" => &mut styled.panels.columns,
                            "gap" => &mut styled.panels.gap,
                            "title" => &mut styled.panels.title,
                            other => {
                                return Err(at(format!(
                                    "`panels` takes `columns`, `gap` and `title`, not `{other}`"
                                )));
                            }
                        };
                        set_layout(prop, value, prop != "columns", slot).map_err(&at)?;
                    }
                    continue;
                }
                if selector.target == "panel" {
                    if !selector.tags.iter().all(|t| Some(t.as_str()) == panel) {
                        unmatched.push(at(format!("no panel named `{}`", selector.tags.join("."))));
                        continue;
                    }
                    for (prop, value) in &decls {
                        let slot = match prop.as_str() {
                            "width" => &mut styled.panels.width,
                            "height" => &mut styled.panels.height,
                            "pad" => &mut styled.panels.pad,
                            other => {
                                return Err(at(format!(
                                    "`panel` takes `width`, `height` and `pad`, not `{other}`"
                                )));
                            }
                        };
                        set_layout(prop, value, prop != "pad", slot).map_err(&at)?;
                    }
                    continue;
                }
                if selector.target == "formula" && selector.tags.is_empty() {
                    for (prop, value) in &decls {
                        if prop != "fill"
                            || type_of(value, &names, false).map_err(&at)? != Ty::Color
                        {
                            return Err(at(format!("`formula` only takes a `fill`, not `{prop}`")));
                        }
                        styled.formula_fill = Some(value.clone());
                    }
                    continue;
                }
                if selector.target == "title" && selector.tags.is_empty() {
                    for (prop, value) in &decls {
                        match prop.as_str() {
                            "fill" => {
                                if type_of(value, &names, false).map_err(&at)? != Ty::Color {
                                    return Err(at("`fill` needs a Color".into()));
                                }
                                styled.title_fill = Some(value.clone());
                            }
                            "size" => styled.title_size = Some(value.clone()),
                            other => {
                                return Err(at(format!(
                                    "`title` takes `fill` and `size`, not `{other}`"
                                )));
                            }
                        }
                    }
                    continue;
                }
                if selector.target == "paper" {
                    for (prop, value) in &decls {
                        if prop == "tone" && selector.facet.is_none() {
                            styled.tone = match value {
                                Expr::Sym(word) if word == "clip" => Tone::Clip,
                                Expr::Sym(word) if word == "tanh" => Tone::Tanh,
                                _ => return Err(at("`tone` is `clip` or `tanh`".to_string())),
                            };
                            continue;
                        }
                        if prop != "fill" || selector.facet.is_some() {
                            return Err(at(format!(
                                "`paper` only takes `fill` and `tone`, not `{prop}`"
                            )));
                        }
                        if type_of(value, &names, false).map_err(&at)? != Ty::Color {
                            return Err(at("`fill` needs a Color".into()));
                        }
                        styled.paper = Some(value.clone());
                    }
                    continue;
                }
                let matches: Vec<usize> = scene
                    .marks
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        (selector.target.is_empty()
                            || m.id == selector.target
                            || m.tags.contains(&selector.target))
                            && selector.tags.iter().all(|t| m.tags.contains(t))
                    })
                    .map(|(i, _)| i)
                    .collect();
                // A shared sheet can't know which types and roles a figure uses, but a rule
                // that names a mark has to find it.
                let is_general = selector.target.is_empty()
                    || !selector.tags.is_empty()
                    || MARK_TYPES.contains(&selector.target.as_str());
                if matches.is_empty() && !is_general {
                    let mut words = selector.target.clone();
                    for tag in &selector.tags {
                        words.push('.');
                        words.push_str(tag);
                    }
                    unmatched.push(at(format!("no mark named `{words}`")));
                }
                for index in matches {
                    let mark = &scene.marks[index];
                    apply(
                        &mut styled.marks[index],
                        &mark.form,
                        mark.beat,
                        &mark.id,
                        selector.facet.as_deref(),
                        &decls,
                        &names,
                    )
                    .map_err(&at)?;
                }
            }
        }
    }
    // A dot in a collection is drawn by its stroke, in the color a dot is filled with.
    for (mark, style) in scene.marks.iter().zip(&mut styled.marks) {
        if let Form::Many {
            shape: Shape::Dots(_),
            ..
        } = &mark.form
        {
            style.stroke = style.negative.take().or(style.stroke.take());
        }
    }
    for (mark, style) in scene.marks.iter().zip(&styled.marks) {
        if style.needs_arc_length() && style.draw == Draw::Field {
            return Err(Error::new(format!(
                "`{}`: dashes and tapers need position along the curve, so they can't use `draw: field`",
                mark.id
            )));
        }
    }
    Ok(styled)
}

/// A layout value is one number that holds for the whole figure, so it can't read a param.
fn set_layout(
    prop: &str,
    value: &Expr,
    needs_unit: bool,
    slot: &mut Option<Expr>,
) -> Result<(), String> {
    if type_of(value, &[], false)? != Ty::Real {
        return Err(format!("`{prop}` needs a number"));
    }
    if needs_unit && !has_pixels(value) {
        return Err(format!("`{prop}` needs a unit, such as `2px`"));
    }
    *slot = Some(value.clone());
    Ok(())
}

/// Applies a rule's declarations to one mark.
fn apply(
    target: &mut MarkStyle,
    form: &Form,
    beat: Option<usize>,
    id: &str,
    facet: Option<&str>,
    decls: &[(String, Expr)],
    names: &[&str],
) -> Result<(), String> {
    // A collection of circles has sides, one instance at a time.
    let many_circles = matches!(
        form,
        Form::Many {
            shape: Shape::Circles { .. } | Shape::Cycles { .. },
            ..
        }
    );
    // A collection of curves is lines, as a curve is.
    if matches!(
        form,
        Form::Many {
            shape: Shape::Curves(_),
            ..
        }
    ) && let Some((prop, _)) = decls
        .iter()
        .find(|(prop, _)| matches!(prop.as_str(), "fill" | "fill_opacity" | "radius"))
    {
        return Err(format!(
            "`{id}` is made of lines, so it has a `stroke` and no `{prop}`"
        ));
    }
    let spatial_ok =
        many_circles || matches!(form, Form::Field { .. } | Form::Region(_) | Form::Dot(_));
    for (prop, value) in decls {
        // A line of samples has no sides to fill, and silently drawing nothing hides a mistake.
        if facet.is_some() && !spatial_ok {
            return Err(format!(
                "`{id}` has no sides, so it can't be styled with `::{}`",
                facet.unwrap_or_default()
            ));
        }
        if !PROPERTIES.contains(&prop.as_str()) {
            return Err(format!("unknown property `{prop}`"));
        }
        if let (None, "draw" | "blend" | "cap" | "font" | "place") = (facet, prop.as_str()) {
            let word = match value {
                Expr::Sym(word) => word.as_str(),
                _ => return Err(format!("`{prop}` needs a keyword")),
            };
            match (prop.as_str(), word) {
                ("draw", "auto") => target.draw = Draw::Auto,
                ("draw", "line") => target.draw = Draw::Line,
                ("draw", "field") => {
                    if !matches!(form, Form::Field { .. }) {
                        return Err(format!(
                            "`{id}` has no field, so it can't use `draw: field`"
                        ));
                    }
                    target.draw = Draw::Field;
                }
                ("blend", "linear") => target.blend = Blend::Linear,
                ("blend", "gamma") => target.blend = Blend::Gamma,
                ("blend", "add") => target.blend = Blend::Add,
                ("cap", "butt") => target.cap = Cap::Butt,
                ("cap", "round") => target.cap = Cap::Round,
                ("font", "sans") => target.font = Font::Sans,
                ("font", "math") => target.font = Font::Math,
                ("place", "fixed") => target.place = Place::Fixed,
                ("place", "settle") => {
                    if !matches!(form, Form::Label { .. }) {
                        return Err(format!("`{id}` isn't a label, so it can't use `place`"));
                    }
                    target.place = Place::Settle;
                }
                _ => return Err(format!("`{prop}` can't be `{word}`")),
            }
            continue;
        }
        let value = &with_age(value, beat);
        let measure = matches!(
            prop.as_str(),
            "width"
                | "dash"
                | "gap"
                | "opacity"
                | "fill_opacity"
                | "radius"
                | "size"
                | "halo"
                | "show"
                | "head"
                | "head_span"
                | "distance"
        );
        let mut scope = names.to_vec();
        if prop == "width" {
            scope.push(SELF_T);
        }
        if prop == "show" {
            scope.push(VIEW_WIDTH);
        }
        // A collection's sizes are worked out per instance, so they can use its variables,
        // and how big a world unit is, to size something in the figure's own terms.
        if let (Form::Many { vars, lets, .. }, "radius" | "width") = (form, prop.as_str()) {
            scope.extend(vars.iter().map(|(name, _)| name.as_str()));
            scope.extend(lets.iter().map(|(name, _)| name.as_str()));
            scope.push(SELF_SCALE);
        }
        // A side's color and fade are worked out per instance too. Its rim isn't: that's one line.
        let per_side = (facet.is_some() && prop == "fill") || prop == "fill_opacity";
        if let (Form::Many { vars, lets, .. }, true) = (form, many_circles && per_side) {
            scope.extend(vars.iter().map(|(name, _)| name.as_str()));
            scope.extend(lets.iter().map(|(name, _)| name.as_str()));
        }
        // `fill_opacity` is a measure that can still fade with distance, as a glow does.
        let spatial = !measure || prop == "fill_opacity";
        let ty = type_of(value, &scope, spatial)?;
        let want = if measure { Ty::Real } else { Ty::Color };
        if ty != want {
            return Err(format!("`{prop}` needs a {want:?}, found a {ty:?}"));
        }
        if spatial && !spatial_ok && (uses(value, SELF_DIST) || uses(value, SELF_PIXEL)) {
            return Err(format!(
                "`{id}` has no distance, so it can't use `self.dist`"
            ));
        }
        let round = many_circles
            || matches!(
                form,
                Form::Field {
                    curvature: Some(_),
                    ..
                }
            );
        if uses(value, SELF_CURVATURE) && !round {
            return Err(format!(
                "`{id}` isn't a circle, so it can't use `self.curvature`"
            ));
        }
        if many_circles
            && !per_side
            && [SELF_DIST, SELF_PIXEL, SELF_CURVATURE]
                .iter()
                .any(|name| uses(value, name))
        {
            return Err(format!(
                "`{id}`'s `{prop}` is the same along all its circles, so it can't use `self`: only a side's `fill` and `fill_opacity` can"
            ));
        }
        let slot = match (facet, prop.as_str()) {
            (Some("inside"), "fill") if round => &mut target.inside,
            (Some("outside"), "fill") if round => &mut target.outside,
            (Some("inside" | "negative"), "fill") => &mut target.negative,
            (Some("outside" | "positive"), "fill") => &mut target.positive,
            (None, "stroke") => &mut target.stroke,
            (None, "opacity") => &mut target.opacity,
            (None, "fill_opacity") => &mut target.fill_opacity,
            (None, "show") => &mut target.show,
            (None, "head_span") => &mut target.head_span,
            (None, "width" | "dash" | "gap" | "radius" | "size" | "halo" | "head" | "distance") => {
                if !has_pixels(value) {
                    return Err(format!("`{prop}` needs a unit, such as `2px`"));
                }
                match prop.as_str() {
                    "width" => &mut target.width,
                    "dash" => &mut target.dash,
                    "gap" => &mut target.gap,
                    "radius" => &mut target.radius,
                    "size" => &mut target.size,
                    "head" => &mut target.head,
                    "distance" => &mut target.distance,
                    _ => &mut target.halo,
                }
            }
            (None, "fill") => {
                if many_circles || matches!(form, Form::Field { .. }) {
                    return Err("`fill` needs a facet, such as `::inside`, for now".into());
                }
                &mut target.negative
            }
            (Some(facet), _)
                if !matches!(facet, "inside" | "outside" | "negative" | "positive") =>
            {
                return Err(format!("unknown facet `::{facet}`"));
            }
            (Some(facet), prop) => {
                return Err(format!("`{prop}` doesn't apply to `::{facet}`"));
            }
            (None, prop) => {
                return Err(format!("unknown property `{prop}`"));
            }
        };
        *slot = Some(value.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Blend, Draw};
    use crate::Error;
    use crate::build;
    use crate::ir::Expr;

    const DOC: &str = "curve c = implicit (x, y): x^2 + y^2 = 1";

    fn styled(sheet: &str) -> Result<crate::Figure, Error> {
        build(DOC, &[sheet])
    }

    fn paper_of(doc: &str, sheets: &[&str]) -> Result<Expr, Error> {
        Ok(crate::build(doc, sheets)?.styles.paper.unwrap())
    }

    #[test]
    fn a_later_sheets_name_reaches_rules_before_it() {
        let earlier = "let ink = rgb(1, 2, 3)\nstyle paper { fill: ink }";
        let later = "let ink = rgb(4, 5, 6)";
        let doc = "curve c = circle((0, 0), (1, 0))";
        let plain = paper_of(doc, &[earlier]).unwrap();
        let both = paper_of(doc, &[earlier, later]).unwrap();
        assert_eq!(
            both,
            paper_of(doc, &["style paper { fill: rgb(4, 5, 6) }"]).unwrap()
        );
        assert_ne!(plain, both);
    }

    #[test]
    fn a_name_can_use_one_defined_after_it() {
        let sheet = "let ink = base\nlet base = rgb(4, 5, 6)\nstyle paper { fill: ink }";
        let doc = "curve c = circle((0, 0), (1, 0))";
        assert_eq!(
            paper_of(doc, &[sheet]).unwrap(),
            paper_of(doc, &["style paper { fill: rgb(4, 5, 6) }"]).unwrap()
        );
    }

    #[test]
    fn a_name_cannot_be_defined_in_terms_of_itself() {
        let doc = "curve c = circle((0, 0), (1, 0))";
        let err =
            crate::build(doc, &["let a = b\nlet b = a\nstyle paper { fill: a }"]).unwrap_err();
        assert!(err.message.contains("in terms of itself"), "{err}");
    }

    #[test]
    fn an_unknown_property_says_so() {
        let doc = "curve c = circle((0, 0), (1, 0))";
        let err = crate::build(doc, &["style c { distnce: 2px }"]).unwrap_err();
        assert!(err.message.contains("unknown property `distnce`"), "{err}");
    }

    #[test]
    fn a_sheet_can_name_its_values_for_later_sheets() {
        let f = build(
            DOC,
            &[
                "let ink = rgb(1, 2, 3)
                 let line = 2px",
                "style c { stroke: ink; width: line }",
            ],
        )
        .unwrap();
        let style = &f.styles.marks[0];
        assert_eq!(
            style.stroke,
            Some(Expr::app(
                "Rgb",
                vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]
            ))
        );
        assert!(style.width.is_some());
        // A document's own `let`s aren't style names, and a keyword isn't either.
        let err = build(DOC, &["style c { stroke: ink }"]).unwrap_err();
        assert!(err.message.contains("unknown name `ink`"), "{err}");
    }

    #[test]
    fn aliases_write_the_same_slot_and_later_rules_win() {
        let f = styled(
            "style c::inside { fill: white }
             style c::negative { fill: black }",
        )
        .unwrap();
        assert_eq!(f.styles.marks[0].negative, Some(Expr::sym("black")));
        assert_eq!(f.styles.marks[0].positive, None);
    }

    #[test]
    fn later_sheets_win_and_the_document_comes_last() {
        let doc = format!("{DOC}\nstyle c::inside {{ fill: black }}");
        let f = build(&doc, &["style c::inside { fill: white }"]).unwrap();
        assert_eq!(f.styles.marks[0].negative, Some(Expr::sym("black")));
        let f = build(
            DOC,
            &[
                "style c::inside { fill: black }",
                "style c::inside { fill: white }",
            ],
        )
        .unwrap();
        assert_eq!(f.styles.marks[0].negative, Some(Expr::sym("white")));
    }

    #[test]
    fn a_stylesheet_never_changes_the_scene() {
        let plain = build(DOC, &[]).unwrap();
        let styled = styled("style c::inside { fill: mix(white, black, 0.5) }").unwrap();
        assert_eq!(plain.scene, styled.scene);
    }

    #[test]
    fn style_values_can_use_signals() {
        styled("style c::inside { fill: oklch(0.5 + 0.2 * sin(time), 0.1, 40) }").unwrap();
    }

    #[test]
    fn self_dist_is_spatial_only() {
        styled("style c::inside { fill: mix(white, black, self.dist) }").unwrap();
        let err = styled("style c { width: 2px + self.dist }").unwrap_err();
        assert!(err.message.contains("spatial"), "{err}");
    }

    #[test]
    fn stroke_properties_resolve() {
        let f = styled(
            "style c { stroke: black; width: 1px + 9px * self.t; dash: 4px; gap: 2px;
                       draw: line; blend: linear }",
        )
        .unwrap();
        let s = &f.styles.marks[0];
        assert!(s.dash.is_some() && s.gap.is_some());
        assert_eq!(s.draw, Draw::Line);
        assert_eq!(s.blend, Blend::Linear);
    }

    #[test]
    fn draw_defaults_to_each_forms_fast_path() {
        let doc = "curve i = implicit (x, y): x = 0
                   curve s = sdf (x, y): x
                   curve p = parametric (t): (t, t) for t in 0..1";
        let sheet = "style i, s, p { stroke: black; width: 1px }";
        let f = build(doc, &[sheet]).unwrap();
        let line = |i: usize| f.styles.marks[i].stroke_is_line(&f.scene.marks[i].form);
        assert!(line(0), "implicit strokes trace");
        assert!(!line(1), "an exact SDF strokes per pixel");
        assert!(line(2), "parametric strokes sample");

        // Dashes need arc length, even on an SDF.
        let f = build(doc, &[&format!("{sheet}\nstyle s {{ dash: 4px }}")]).unwrap();
        assert!(f.styles.marks[1].stroke_is_line(&f.scene.marks[1].form));
    }

    #[test]
    fn hints_can_override_the_default() {
        let doc = "curve i = implicit (x, y): x = 0\ncurve s = sdf (x, y): x";
        let f = build(
            doc,
            &["style i { stroke: black; width: 1px; draw: field }
               style s { stroke: black; width: 1px; draw: line }"],
        )
        .unwrap();
        assert!(!f.styles.marks[0].stroke_is_line(&f.scene.marks[0].form));
        assert!(f.styles.marks[1].stroke_is_line(&f.scene.marks[1].form));
    }

    #[test]
    fn mistakes_fail_loudly() {
        for (sheet, expected) in [
            (
                "style c { dash: 4px; draw: field }",
                "can't use `draw: field`",
            ),
            (
                "style c { width: 1px + 2px * self.t; draw: field }",
                "can't use `draw: field`",
            ),
            ("style c { dash: 4 }", "needs a unit"),
            ("style c { dash: self.t * 1px }", "unknown name `self.t`"),
            ("style c { draw: sketchy }", "can't be `sketchy`"),
            ("style c { cap: wobbly }", "can't be `wobbly`"),
            ("style c { blend: 2 }", "needs a keyword"),
            ("style nope { stroke: white }", "no mark named"),
            ("style c::sideways { fill: white }", "unknown facet"),
            ("style c { fill: white }", "needs a facet"),
            ("style c { width: 2 }", "needs a unit"),
            ("style c::inside { fill: 2 }", "needs a Color"),
            ("style c::inside { fill: oklch(1, 2) }", "doesn't take"),
            ("style c { glow: white }", "unknown property"),
            ("style c::inside { fill: mystery }", "unknown name"),
            ("style paper { stroke: white }", "only takes `fill`"),
        ] {
            let err = styled(sheet).expect_err(sheet);
            assert!(err.message.contains(expected), "{sheet}: {err}");
        }
    }

    const BEATS: &str = "
        let P = (0, 0)
        beat \"One\" tag blue {
          curve a = circle(P, (1, 0))
          curve b = line(P, (1, 0)) tag aux
        }
        beat \"Two\" tag red {
          curve c = circle(P, (2, 0))
        }";

    #[test]
    fn selectors_match_names_types_and_tags() {
        let f = build(BEATS, &["style circle { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke.is_some();
        assert!(has(0) && !has(1) && has(2), "by type");

        let f = build(BEATS, &["style .aux { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke.is_some();
        assert!(!has(0) && has(1) && !has(2), "by tag");

        let f = build(BEATS, &["style circle.blue { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke.is_some();
        assert!(has(0) && !has(1) && !has(2), "conjunction");

        let f = build(BEATS, &["style c, b { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke.is_some();
        assert!(!has(0) && has(1) && has(2), "names");

        // A shared sheet can name roles a figure doesn't use, but a name has to be there.
        assert!(build(BEATS, &["style .purple { stroke: white }"]).is_ok());
        let err = build(BEATS, &["style purple { stroke: white }"]).unwrap_err();
        assert!(err.message.contains("no mark named `purple`"), "{err}");
    }

    /// One rule, a different expression per mark: each mark's own beat is substituted.
    #[test]
    fn self_age_becomes_each_marks_own_expression() {
        let f = build(
            BEATS,
            &["style circle { opacity: reveal(self.age, 1, 0.5) }"],
        )
        .unwrap();
        let of = |i: usize| {
            f.styles.marks[i]
                .opacity
                .clone()
                .unwrap()
                .to_mathjson()
                .to_string()
        };
        assert!(of(0).contains(r#"["Subtract","beat",0.0]"#), "{}", of(0));
        assert!(of(2).contains(r#"["Subtract","beat",1.0]"#), "{}", of(2));
    }

    #[test]
    fn self_age_is_zero_outside_any_beat() {
        let doc = "curve c = circle((0, 0), (1, 0))";
        let f = build(doc, &["style c { opacity: reveal(self.age, 1, 0.5) }"]).unwrap();
        let o = f.styles.marks[0]
            .opacity
            .clone()
            .unwrap()
            .to_mathjson()
            .to_string();
        assert!(o.starts_with(r#"["Reveal",0.0,"#), "{o}");
    }

    #[test]
    fn forms_without_a_field_or_distance_say_so() {
        let doc = "curve p = parametric (t): (t, t) for t in 0..1";
        let err = build(doc, &["style p { draw: field }"]).unwrap_err();
        assert!(err.message.contains("has no field"), "{err}");
        let err = build(doc, &["style p { stroke: mix(white, black, self.dist) }"]).unwrap_err();
        assert!(err.message.contains("has no distance"), "{err}");
    }
}
