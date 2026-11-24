//! Resolves stylesheets against a scene. Style values are expressions, so a fill
//! can depend on `time`, a param or `self.dist`. Nothing here changes the scene's IR.
//!
//! Facets: `::negative` and `::positive` are the field's raw sign. On a circle, `::inside` and
//! `::outside` are its true disk and the rest, whichever way round it runs. Elsewhere, they're
//! aliases for the sign under the gradient convention, where the negative side is inside. Later declarations win (SPEC/15's layers, without
//! specificity).

use std::collections::HashMap;

use crate::compile::{
    BEAT_PARAM, CONSTANTS, DURATION, NO_DURATION, SIGNALS, constant_value, written_constant,
};
use crate::error::Error;
use crate::frame::VIEW_WIDTH;
use crate::ir::{Expr, Form, Scene, Shape};
use crate::parse::{Document, Item};

/// The distance to a mark, per pixel. Only legal in spatial properties.
pub const SELF_DIST: &str = "self.dist";
/// Arc-length position along a stroke, 0 to 1. Only legal in `width`.
pub const SELF_T: &str = "self.t";
/// A point's third coordinate as written, along a curve of points in space and for each dot of
/// a collection of them.
pub const SELF_Z: &str = "self.z";
/// How big a device pixel is, in world units, wherever `self.dist` can be read.
pub const SELF_PIXEL: &str = "self.pixel";
/// A circle's signed curvature, which is below 0 when it runs clockwise and 0 on a line.
pub const SELF_CURVATURE: &str = "self.curvature";
/// Style pixels per world unit, in a collection's sizes.
pub const SELF_SCALE: &str = "self.scale";
/// How many beats ago this mark's beat was: negative before it, 0 during it.
pub const SELF_AGE: &str = "self.age";
/// How present a mark is, as beats with lengths show and drop it: 0 to 1.
pub const SELF_LIFE: &str = "self.life";
/// How much the current beat is about this mark: 0 to 1.
pub const SELF_FOCUS: &str = "self.focus";
/// Every property a style can set. `docs/language.md` must describe each one, and a test
/// holds it to that.
pub const PROPERTIES: [&str; 27] = [
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
    "trim",
    "ticks",
    "tick",
    "leader",
    "leader_gap",
    "format",
    "digits",
    "text",
];
pub const COLORS: [&str; 2] = ["white", "black"];
/// The tags that say what kind of mark something is.
const MARK_TYPES: [&str; 15] = [
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

/// The space a mark's edges blend in. The default is gamma-encoded sRGB, because
/// that's what SVG and CSS do and SVG's look is the quality bar. Linear light makes
/// dark lines on a light ground visibly thinner.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Blend {
    #[default]
    Gamma,
    Linear,
    /// Adds to what's underneath, in display space, with no limit, as a shader's `col +=` does.
    /// What's drawn this way usually wants a `tone` on the paper.
    Add,
}

/// What a figure does with its finished color, for a figure that adds light.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Tone {
    /// Past 1 is white.
    #[default]
    Clip,
    /// tanh of each channel, in display space, which rolls off smoothly.
    Tanh,
    /// Between the two, in display space: 0 clips and 1 is `tanh`, so a figure can change
    /// over as what it draws starts adding light.
    Mix(Expr),
}

/// A rendering hint. It chooses how a stroke is drawn, never what it means, so every
/// choice has to agree with the others within tolerance.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Cap {
    #[default]
    Butt,
    Round,
}

/// A label's typeface, as a role. What each role maps to belongs to the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Font {
    #[default]
    Sans,
    /// Italic serif, for math.
    Math,
}

/// One part of what a label says about its value. A style's `text` lists them, joined by ` = `.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TextPart {
    /// The label's own words, or its value's name if it has none.
    Caption,
    /// The value's name.
    Name,
    /// The value's expression, as written.
    Expr,
    /// What it comes to.
    Value,
}

/// How a label's text writes a point it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Format {
    /// `(−0.5, −0.87)`.
    #[default]
    Point,
    /// `−0.5 − 0.87i`.
    Complex,
    /// `e^(−2πi/3)`, with π's fractions written as fractions.
    Polar,
}

/// How a label finds its spot. See `place.rs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Place {
    /// At its offset from its point, exactly.
    Fixed,
    /// Tied to that spot by a spring, and pushed off what's drawn and other labels, so a label
    /// with nothing said doesn't sit on its own dot.
    #[default]
    Settle,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// How much of a stroke is drawn, from its start: 0 to 1 of its parameter, or of its length
    /// for a straight line.
    pub trim: Option<Expr>,
    /// How many marks across a segment's middle say it's as long as another with as many.
    pub ticks: Option<Expr>,
    /// How long each of those marks is, in style pixels.
    pub tick: Option<Expr>,
    /// A label's leader: how wide its line is, in style pixels.
    pub leader: Option<Expr>,
    /// How far from its point a label's leader stops, in style pixels.
    pub leader_gap: Option<Expr>,
    /// How a label writes a point it shows.
    pub format: Format,
    /// How many decimal places a label writes a value to. A whole number is written whole.
    pub digits: Option<Expr>,
    /// What a label says about its value, as `expr = value`. `None` is its caption, or its
    /// value if it has no words of its own.
    pub text: Option<Vec<TextPart>>,
    /// A sheet wrote `stroke: none`: no stroke, and none by default either.
    pub no_stroke: bool,
    /// A sheet wrote `fill: none`, on the mark or a side of it: no fill, and none by default.
    pub no_fill: bool,
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
            // Per pixel, by its distance to the curve as sampled, when a sheet asks for that.
            Form::Parametric(_) => self.draw != Draw::Field,
            Form::Polyline { .. }
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
    /// The opacity of a line stroke that varies along it, with `self.t` or `self.z`. It's worked
    /// out where the stroke is built and drawn into its coverage, so shading leaves it alone.
    pub fn ink(&self) -> Option<&Expr> {
        self.opacity
            .as_ref()
            .filter(|o| uses(o, SELF_T) || uses(o, SELF_Z))
    }

    /// Whether a stroke's width or opacity reads its points' third coordinate.
    pub fn reads_depth(&self) -> bool {
        [&self.width, &self.opacity]
            .into_iter()
            .flatten()
            .any(|e| uses(e, SELF_Z))
    }

    pub fn needs_arc_length(&self) -> bool {
        self.trim.is_some()
            || self.dash.is_some()
            || self.gap.is_some()
            || self.width.as_ref().is_some_and(|w| uses(w, SELF_T))
            || self.opacity.as_ref().is_some_and(|o| uses(o, SELF_T))
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Styled {
    /// One entry per `scene.marks`, in the same order.
    pub marks: Vec<MarkStyle>,
    pub paper: Option<Expr>,
    pub tone: Tone,
    /// A panel title's color and size, and the color of the formula beside it.
    pub title_fill: Option<Expr>,
    pub title_size: Option<Expr>,
    /// Beat captions, drawn in the figure only when a sheet asks: a page can show them as text.
    pub caption: Option<CaptionStyle>,
    pub formula_fill: Option<Expr>,
    /// A sheet wrote `fill: none` for the titles or the formulas: they aren't drawn.
    pub no_title_fill: bool,
    pub no_formula_fill: bool,
    pub panels: PanelStyle,
}

/// How panels sit on the canvas, in design pixels: `style panels` for the whole, and
/// `style panel` for each. A figure without panels takes its `width` and `height` from
/// `style figure`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PanelStyle {
    pub columns: Option<Expr>,
    pub gap: Option<Expr>,
    pub title: Option<Expr>,
    pub width: Option<Expr>,
    pub height: Option<Expr>,
    pub pad: Option<Expr>,
    /// The reader's text size where there's no page to say, as in a video, in design pixels.
    pub em: Option<Expr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    Real,
    Color,
}

fn has_head(e: &Expr, name: &str) -> bool {
    match e {
        Expr::App(head, args) => head == name || args.iter().any(|a| has_head(a, name)),
        _ => false,
    }
}

/// Whether a collection's stroke reads its instances' own names, so each of its lines has its own
/// color, worked out where its segments are. A dot of a collection is drawn by its stroke.
pub fn inked(mark: &crate::ir::Mark, style: &MarkStyle) -> bool {
    let (Form::Many { vars, lets, .. }, Some(stroke)) = (&mark.form, &style.stroke) else {
        return false;
    };
    let mut found = false;
    stroke.visit_syms(&mut |s| {
        found |= vars.iter().any(|(v, _)| v == s) || lets.iter().any(|(v, _)| v == s);
    });
    found
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
                || CONSTANTS.contains(&s.as_str())
                || written_constant(s).and_then(constant_value).is_some() =>
        {
            Ok(Ty::Real)
        }
        Expr::Sym(s) => {
            let known = names
                .iter()
                .copied()
                .chain(SIGNALS)
                .chain(CONSTANTS)
                .chain(COLORS);
            Err(crate::suggest::unknown(
                "name",
                s,
                known.chain(["pi", "π", "e"]),
            ))
        }
        Expr::App(head, args) => {
            let tys = args
                .iter()
                .map(|a| type_of(a, names, spatial))
                .collect::<Result<Vec<_>, _>>()?;
            use Ty::{Color, Real};
            match (head.as_str(), tys.as_slice()) {
                ("Oklch" | "Rgb", [Real, Real, Real]) => Ok(Color),
                ("Hue", [Real]) => Ok(Color),
                ("Contrast", [Color]) => Ok(Color),
                ("Mix", [Color, Color, Real]) => Ok(Color),
                (
                    "Mix" | "Clamp" | "Smoothstep" | "Reveal" | "IfNonnegative",
                    [Real, Real, Real],
                ) => Ok(Real),
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
                    | "Sign" | "Floor" | "Pixels" | "Ems" | "Sinh" | "Cosh" | "Tanh",
                    [Real],
                ) => Ok(Real),
                // Built-ins are capitalized once parsed, so a name still in lowercase is one no
                // sheet defines.
                _ if head.starts_with(|c: char| c.is_lowercase()) => {
                    let built_in = crate::parse::FUNCTIONS.iter().map(|(f, _)| *f);
                    Err(match crate::suggest::suggestion(head, built_in) {
                        Some(hint) => format!("no function `{head}`: did you mean {hint}?"),
                        None => format!(
                            "no function `{head}`: a sheet can define one with `fn {head}(…) = …`"
                        ),
                    })
                }
                _ => Err(format!("`{head}` doesn't take {tys:?}")),
            }
        }
    }
}

/// A sheet's own function: its parameters and body, inlined where it's called.
type Functions = HashMap<String, (Vec<String>, Expr)>;

/// How deep sheet functions can call each other, which only a function that calls itself would
/// reach.
const CALL_DEPTH: usize = 32;

/// A sheet's `pi` is the constant, and its `duration` the figure's, unless the figure has a
/// binding of that name. `duration` is a number from here on, so the CPU and the GPU never see
/// the name.
fn constants(e: &Expr, names: &[&str], duration: Option<f64>) -> Result<Expr, String> {
    Ok(match e {
        Expr::Sym(s) if s == DURATION && !names.contains(&DURATION) => match duration {
            Some(d) => Expr::Num(d),
            None => return Err(NO_DURATION.into()),
        },
        Expr::Sym(s) if !names.contains(&s.as_str()) => {
            match written_constant(s).filter(|c| constant_value(c).is_some()) {
                Some(constant) => Expr::sym(constant),
                None => e.clone(),
            }
        }
        Expr::App(head, args) => Expr::App(
            head.clone(),
            args.iter()
                .map(|a| constants(a, names, duration))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

/// Replaces a style sheet's own names with what they stand for, and its functions' calls with
/// their bodies, so a theme can name its colors, sizes and curves and nothing downstream needs to
/// know.
fn with_names(
    e: &Expr,
    names: &HashMap<String, Expr>,
    functions: &Functions,
) -> Result<Expr, String> {
    with_names_in(e, names, functions, &HashMap::new(), 0)
}

/// `with_names`, inside a function's body, where `args` are what its parameters stand for and
/// shadow the sheet's names.
fn with_names_in(
    e: &Expr,
    names: &HashMap<String, Expr>,
    functions: &Functions,
    args: &HashMap<String, Expr>,
    depth: usize,
) -> Result<Expr, String> {
    Ok(match e {
        Expr::Sym(s) => match args.get(s).or_else(|| names.get(s)) {
            Some(value) => value.clone(),
            None => e.clone(),
        },
        Expr::App(head, given) => {
            let given = given
                .iter()
                .map(|a| with_names_in(a, names, functions, args, depth))
                .collect::<Result<Vec<_>, _>>()?;
            match functions.get(head) {
                Some((params, body)) => {
                    if params.len() != given.len() {
                        return Err(format!(
                            "`{head}` takes {} argument{}, not {}",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" },
                            given.len()
                        ));
                    }
                    if depth >= CALL_DEPTH {
                        return Err(format!(
                            "`{head}` calls itself, which a sheet's function can't"
                        ));
                    }
                    let local = params.iter().cloned().zip(given).collect();
                    with_names_in(body, names, functions, &local, depth + 1)?
                }
                // A function that is written out in others, as `arctan` is, is here too.
                None => crate::compile::lowered(head, &given)
                    .unwrap_or_else(|| Expr::App(head.clone(), given)),
            }
        }
        other => other.clone(),
    })
}

/// The names an expression uses, including those in the bodies of the functions it calls, but
/// not their parameters.
fn names_used(
    e: &Expr,
    functions: &Functions,
    params: &[String],
    calling: &mut Vec<String>,
    out: &mut Vec<String>,
) {
    match e {
        Expr::Sym(s) if !params.contains(s) => out.push(s.clone()),
        Expr::App(head, args) => {
            for a in args {
                names_used(a, functions, params, calling, out);
            }
            if let Some((own, body)) = functions.get(head) {
                // A function that calls itself is reported where it's inlined.
                if !calling.contains(head) {
                    calling.push(head.clone());
                    names_used(body, functions, own, calling, out);
                    calling.pop();
                }
            }
        }
        _ => {}
    }
}

/// The functions sheets define. As with names, the last sheet to define one decides what it
/// means everywhere.
fn sheet_functions(sheets: &[Document]) -> Functions {
    let mut functions = Functions::new();
    for sheet in sheets {
        for item in &sheet.items {
            if let Item::Fn {
                name, params, body, ..
            } = item
            {
                functions.insert(name.clone(), (params.clone(), body.clone()));
            }
        }
    }
    functions
}

/// How a beat's caption looks, when a sheet asks for it with `style caption { … }`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaptionStyle {
    pub fill: Option<Expr>,
    pub size: Option<Expr>,
    pub halo: Option<Expr>,
    /// How far its center sits from the bottom of the canvas, in `px`.
    pub distance: Option<Expr>,
    pub font: Font,
    /// A sheet wrote `fill: none`: captions aren't drawn.
    pub no_fill: bool,
}

/// Replaces `self.age`, `self.life` and `self.focus` with this mark's own expressions in
/// `beat`. `self.age` is `beat` minus its beat's index, and a mark outside any beat is always
/// current. Done symbolically, so the runtime needs nothing new.
fn with_story(e: &Expr, mark: &crate::ir::Mark) -> Expr {
    match e {
        Expr::Sym(s) if s == SELF_AGE => match mark.beat {
            Some(k) => Expr::app("Subtract", vec![Expr::sym(BEAT_PARAM), Expr::Num(k as f64)]),
            None => Expr::Num(0.0),
        },
        Expr::Sym(s) if s == SELF_LIFE => mark.life.clone().unwrap_or(Expr::Num(1.0)),
        Expr::Sym(s) if s == SELF_FOCUS => mark.focus.clone().unwrap_or(Expr::Num(1.0)),
        Expr::App(head, args) => Expr::App(
            head.clone(),
            args.iter().map(|a| with_story(a, mark)).collect(),
        ),
        other => other.clone(),
    }
}

/// The values sheets name. A later sheet's definition replaces an earlier one's everywhere,
/// including in rules written before it, so a sheet can change what `ink` means for a whole
/// figure. A definition can use any other name, in any order.
pub fn sheet_names(sheets: &[Document]) -> Result<HashMap<String, Expr>, Error> {
    let mut written: HashMap<&str, (&Expr, usize, usize)> = HashMap::new();
    for (index, sheet) in sheets.iter().enumerate() {
        for item in &sheet.items {
            if let Item::Let {
                name, expr, line, ..
            } = item
            {
                written.insert(name, (expr, *line, index));
            }
        }
    }
    let functions = sheet_functions(sheets);
    fn expand(
        name: &str,
        written: &HashMap<&str, (&Expr, usize, usize)>,
        functions: &Functions,
        done: &mut HashMap<String, Expr>,
        visiting: &mut Vec<String>,
    ) -> Result<(), Error> {
        if done.contains_key(name) {
            return Ok(());
        }
        let (expr, line, sheet) = written[name];
        if visiting.iter().any(|v| v == name) {
            return Err(
                Error::at(line, 1, format!("`{name}` is defined in terms of itself"))
                    .in_sheet(sheet),
            );
        }
        visiting.push(name.to_string());
        let mut used = Vec::new();
        names_used(expr, functions, &[], &mut Vec::new(), &mut used);
        for other in used {
            if written.contains_key(other.as_str()) {
                expand(&other, written, functions, done, visiting)?;
            }
        }
        visiting.pop();
        let value = with_names(expr, done, functions)
            .map_err(|message| Error::at(line, 1, message).in_sheet(sheet))?;
        done.insert(name.to_string(), value);
        Ok(())
    }
    let mut done = HashMap::new();
    for name in written.keys() {
        expand(name, &written, &functions, &mut done, &mut Vec::new())?;
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
    // What `duration` reads: what the document says, or what its moves and pauses add up to.
    let duration = scene.duration;
    // The document comes last, and its own `let`s are bindings.
    let sheet_names = sheet_names(&sheets[..sheets.len().saturating_sub(1)])?;
    let functions = sheet_functions(&sheets[..sheets.len().saturating_sub(1)]);
    // The sheets' own names are gone by the time a value is checked, so they're only here for
    // an error to suggest.
    let checked: Vec<&str> = names
        .iter()
        .copied()
        .chain(sheet_names.keys().map(String::as_str))
        .collect();

    for (index, sheet) in sheets.iter().enumerate() {
        // The document comes last, and its errors name no sheet.
        let in_sheet = |e: Error| match index + 1 < sheets.len() {
            true => e.in_sheet(index),
            false => e,
        };
        for item in &sheet.items {
            let Item::Style(rule) = item else { continue };
            let at = |message: String| in_sheet(Error::at(rule.line, 1, message));
            let decls: Vec<(String, Expr)> = rule
                .decls
                .iter()
                .map(|(prop, value)| {
                    let value = with_names(value, &sheet_names, &functions).map_err(&at)?;
                    Ok((
                        prop.clone(),
                        constants(&value, &names, duration).map_err(&at)?,
                    ))
                })
                .collect::<Result<_, Error>>()?;
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
                if selector.target == "figure" && selector.tags.is_empty() {
                    for (prop, value) in &decls {
                        let slot = match prop.as_str() {
                            "width" | "height" if panel.is_some() => {
                                return Err(at(format!(
                                    "a figure with panels is sized by `style panel`, not `style figure {{ {prop} }}`"
                                )));
                            }
                            "width" => &mut styled.panels.width,
                            "height" => &mut styled.panels.height,
                            "em" => &mut styled.panels.em,
                            other => {
                                return Err(at(format!(
                                    "`figure` takes `width`, `height` and `em`, not `{other}`"
                                )));
                            }
                        };
                        set_layout(prop, value, true, slot).map_err(&at)?;
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
                        if prop == "fill" && is_none(value) {
                            (styled.formula_fill, styled.no_formula_fill) = (None, true);
                            continue;
                        }
                        if prop != "fill"
                            || type_of(value, &checked, false).map_err(&at)? != Ty::Color
                        {
                            return Err(at(format!("`formula` only takes a `fill`, not `{prop}`")));
                        }
                        (styled.formula_fill, styled.no_formula_fill) =
                            (Some(value.clone()), false);
                    }
                    continue;
                }
                if selector.target == "caption" && selector.tags.is_empty() {
                    let caption = styled.caption.get_or_insert_with(CaptionStyle::default);
                    for (prop, value) in &decls {
                        match (prop.as_str(), value) {
                            ("fill", _) if is_none(value) => {
                                (caption.fill, caption.no_fill) = (None, true)
                            }
                            ("fill", _) => {
                                if type_of(value, &checked, false).map_err(&at)? != Ty::Color {
                                    return Err(at("`fill` needs a Color".into()));
                                }
                                (caption.fill, caption.no_fill) = (Some(value.clone()), false);
                            }
                            ("size", _) => caption.size = Some(value.clone()),
                            ("halo", _) => caption.halo = Some(value.clone()),
                            ("distance", _) => caption.distance = Some(value.clone()),
                            ("font", Expr::Sym(w)) if w == "sans" => caption.font = Font::Sans,
                            ("font", Expr::Sym(w)) if w == "math" => caption.font = Font::Math,
                            (other, _) => {
                                return Err(at(format!(
                                    "`caption` takes `fill`, `size`, `halo`, `distance` and `font`, not `{other}`"
                                )));
                            }
                        }
                    }
                    continue;
                }
                if selector.target == "title" && selector.tags.is_empty() {
                    for (prop, value) in &decls {
                        match prop.as_str() {
                            "fill" if is_none(value) => {
                                (styled.title_fill, styled.no_title_fill) = (None, true);
                            }
                            "fill" => {
                                if type_of(value, &checked, false).map_err(&at)? != Ty::Color {
                                    return Err(at("`fill` needs a Color".into()));
                                }
                                (styled.title_fill, styled.no_title_fill) =
                                    (Some(value.clone()), false);
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
                                _ if type_of(value, &checked, false) == Ok(Ty::Real) => {
                                    Tone::Mix(value.clone())
                                }
                                _ => {
                                    return Err(at("`tone` is `clip`, `tanh`, or a number from \
                                        0 to 1 between them"
                                        .to_string()));
                                }
                            };
                            continue;
                        }
                        if prop != "fill" || selector.facet.is_some() {
                            return Err(at(format!(
                                "`paper` only takes `fill` and `tone`, not `{prop}`"
                            )));
                        }
                        if is_none(value) {
                            return Err(at("the paper is what everything is drawn on, so it \
                                can't be `fill: none`: leave its fill out for white"
                                .into()));
                        }
                        if type_of(value, &checked, false).map_err(&at)? != Ty::Color {
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
                for &index in &matches {
                    let mark = &scene.marks[index];
                    // A value's dot and its label share its name, so a rule that names the
                    // value reaches both, and what's only a label's skips the dot.
                    let beside_label = !matches!(mark.form, Form::Label { .. })
                        && matches.iter().any(|&other| {
                            let other = &scene.marks[other];
                            other.id == mark.id && matches!(other.form, Form::Label { .. })
                        });
                    apply(
                        &mut styled.marks[index],
                        mark,
                        &mark.id,
                        selector.facet.as_deref(),
                        &decls,
                        &checked,
                        beside_label,
                    )
                    .map_err(&at)?;
                }
            }
        }
    }
    for (mark, style) in scene.marks.iter().zip(&styled.marks) {
        if style.trim.is_some() && style.draw == Draw::Field {
            return Err(Error::new(format!(
                "`{}` is drawn per pixel, with `draw: field`, so it can't use `trim`",
                mark.id
            )));
        }
    }
    // A mark that a beat hasn't shown yet, or has dropped, isn't there at all: how it arrives
    // and leaves is the sheet's, but whether it's there is the document's.
    for (mark, style) in scene.marks.iter().zip(&mut styled.marks) {
        if let Some(life) = &mark.life {
            style.show = Some(match style.show.take() {
                Some(show) => Expr::app("Min", vec![show, life.clone()]),
                None => life.clone(),
            });
        }
    }
    // What's drawn with nothing said: a mark with nothing to paint gets its default paint, and
    // text and titles with no fill get the ink.
    // The sheets' `ink`, read as a rule's value is, so a constant in it is folded.
    let sheet_ink = match sheet_names.get("ink") {
        Some(ink) => {
            let (index, line) = last_let(&sheets[..sheets.len().saturating_sub(1)], "ink")
                .expect("a sheet's name is defined in a sheet");
            let at = |message: String| Error::at(line, 1, message).in_sheet(index);
            let ink = constants(ink, &names, duration).map_err(at)?;
            if type_of(&ink, &names, false) != Ok(Ty::Color) {
                return Err(at(
                    "`ink` is the color a mark is drawn in when no rule gives it \
                     one, so it has to be a color"
                        .into(),
                ));
            }
            Some(ink)
        }
        None => None,
    };
    let ink = default_ink(sheet_ink.as_ref(), styled.paper.as_ref(), &names);
    for (mark, style) in scene.marks.iter().zip(&mut styled.marks) {
        complete(mark, style, &ink);
    }
    for (fill, none) in [
        (&mut styled.title_fill, styled.no_title_fill),
        (&mut styled.formula_fill, styled.no_formula_fill),
    ] {
        if !none {
            fill.get_or_insert_with(|| ink.clone());
        }
    }
    if let Some(caption) = &mut styled.caption
        && !caption.no_fill
    {
        caption.fill.get_or_insert_with(|| ink.clone());
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
        let reads_distance = [&style.stroke, &style.opacity]
            .into_iter()
            .flatten()
            .any(|e| uses(e, SELF_DIST) || uses(e, SELF_PIXEL));
        if matches!(mark.form, Form::Parametric(_)) && reads_distance && style.draw != Draw::Field {
            return Err(Error::new(format!(
                "`{}` is drawn as a line, which has no distance: add `draw: field` for its stroke to use `self.dist`",
                mark.id
            )));
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

/// The color a mark is drawn in when its sheets don't say: the sheets' `ink`, if one names a
/// color by that name, or else black or white, whichever stands out more against the paper. A
/// paper that changes is judged frame by frame, by `Contrast`, which the CPU and the shader both
/// evaluate.
pub fn default_ink(ink: Option<&Expr>, paper: Option<&Expr>, names: &[&str]) -> Expr {
    if let Some(ink) = ink.filter(|ink| type_of(ink, names, false) == Ok(Ty::Color)) {
        return ink.clone();
    }
    let Some(paper) = paper else {
        return Expr::sym("black");
    };
    let mut fixed = true;
    paper.visit_syms(&mut |s| fixed &= COLORS.contains(&s));
    let contrast = Expr::app("Contrast", vec![paper.clone()]);
    if fixed && let Ok(c) = crate::eval::color(&contrast, &crate::eval::Env::new(0.0)) {
        return Expr::sym(if c[0] > 0.5 { "white" } else { "black" });
    }
    contrast
}

/// Gives a mark that would paint nothing the paint its sheets' properties belong to, or its
/// type's own: a stroke, an outline for a region, and a fill for a dot. A property that only a
/// stroke has, such as `width`, asks for a stroke, and `radius` or `fill_opacity` for a fill. A
/// paint written `none` stays off. A mark that paints anything is left as it is, and a label or
/// a term only gets the ink for its text.
fn complete(mark: &crate::ir::Mark, style: &mut MarkStyle, ink: &Expr) {
    let form = &mark.form;
    if matches!(form, Form::Label { .. } | Form::Term { .. }) {
        if style.negative.is_none() && !style.no_fill {
            style.negative = Some(ink.clone());
        }
        return;
    }
    let dots = matches!(
        form,
        Form::Many {
            shape: Shape::Dots(_),
            ..
        }
    );
    // What's filled with a plain `fill`, and what's drawn by its sides.
    let filled = dots || matches!(form, Form::Dot(_) | Form::Region(_));
    let sided = filled
        || matches!(form, Form::Field { .. })
        || matches!(
            form,
            Form::Many {
                shape: Shape::Circles { .. } | Shape::Cycles { .. },
                ..
            }
        );
    if style.stroke.is_some() || (sided && style.has_sides()) {
        return;
    }
    // A property asks for a paint only on a mark it applies to.
    let dot = matches!(form, Form::Dot(_));
    let curve = matches!(
        form,
        Form::Parametric(_)
            | Form::Many {
                shape: Shape::Curves(_),
                ..
            }
    );
    let arrow = matches!(form, Form::Arrow { .. });
    let set = |p: &Option<Expr>| p.is_some();
    // `trim`, `ticks` and `tick` are only accepted where there's a line to trim or mark.
    let stroked = set(&style.width)
        || (!(dot || dots) && (set(&style.dash) || set(&style.gap)))
        || ((curve || arrow) && set(&style.head))
        || (curve && set(&style.head_span))
        || set(&style.trim)
        || set(&style.ticks)
        || set(&style.tick);
    let asks_fill = ((dot || dots) && set(&style.radius))
        || ((dot || matches!(form, Form::Region(_))) && set(&style.fill_opacity));
    let mut stroke = stroked && !style.no_stroke;
    let mut fill = asks_fill && !style.no_fill;
    if !stroke && !fill {
        if dots || dot {
            fill = !style.no_fill;
        } else {
            stroke = !style.no_stroke;
        }
    }
    if stroke {
        style.stroke = Some(ink.clone());
    }
    if fill {
        style.negative = Some(ink.clone());
    }
}

fn is_none(value: &Expr) -> bool {
    matches!(value, Expr::Sym(word) if word == "none")
}

/// The sheet and line of the last `let` of this name, which is the one that holds.
fn last_let(sheets: &[Document], name: &str) -> Option<(usize, usize)> {
    let mut found = None;
    for (index, sheet) in sheets.iter().enumerate() {
        for item in &sheet.items {
            if let Item::Let { name: n, line, .. } = item
                && n == name
            {
                found = Some((index, *line));
            }
        }
    }
    found
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
    if has_head(value, "Ems") {
        return Err(format!(
            "`{prop}` is in `em`, the reader's text size, so it can't size a figure"
        ));
    }
    if needs_unit && !has_head(value, "Pixels") {
        return Err(format!("`{prop}` needs a unit, such as `2px`"));
    }
    *slot = Some(value.clone());
    Ok(())
}

/// Applies a rule's declarations to one mark.
fn apply(
    target: &mut MarkStyle,
    mark: &crate::ir::Mark,
    id: &str,
    facet: Option<&str>,
    decls: &[(String, Expr)],
    names: &[&str],
    beside_label: bool,
) -> Result<(), String> {
    let form = &mark.form;
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
        // What only a label takes passes over the dot that shares its value's name.
        let label_only = matches!(
            prop.as_str(),
            "text" | "format" | "digits" | "leader" | "leader_gap"
        ) || (prop == "place" && matches!(value, Expr::Sym(w) if w == "settle"));
        if label_only && beside_label {
            continue;
        }
        if prop == "text" {
            if !matches!(form, Form::Label { .. }) || facet.is_some() {
                return Err(format!("`{id}` isn't a label, so it can't use `text`"));
            }
            let parts = match value {
                Expr::App(head, parts) if head == crate::parse::TEXT_PARTS => parts,
                _ => return Err("`text` is parts joined by `=`, as `expr = value`".into()),
            };
            let parts: Result<Vec<TextPart>, String> = parts
                .iter()
                .map(|part| match part {
                    Expr::Sym(word) => match word.as_str() {
                        "caption" => Ok(TextPart::Caption),
                        "name" => Ok(TextPart::Name),
                        "expr" => Ok(TextPart::Expr),
                        "value" => Ok(TextPart::Value),
                        other => Err(format!(
                            "a label's text is made of `caption`, `name`, `expr` and `value`, not `{other}`"
                        )),
                    },
                    _ => Err("`text` is parts joined by `=`, as `expr = value`".into()),
                })
                .collect();
            target.text = Some(parts?);
            continue;
        }
        if let (None, "draw" | "blend" | "cap" | "font" | "place" | "format") =
            (facet, prop.as_str())
        {
            let word = match value {
                Expr::Sym(word) => word.as_str(),
                _ => return Err(format!("`{prop}` needs a keyword")),
            };
            match (prop.as_str(), word) {
                ("draw", "auto") => target.draw = Draw::Auto,
                ("draw", "line") => target.draw = Draw::Line,
                ("draw", "field") => {
                    if !matches!(form, Form::Field { .. } | Form::Parametric(_)) {
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
                ("format", "point" | "complex" | "polar") => {
                    if !matches!(form, Form::Label { .. }) {
                        return Err(format!("`{id}` isn't a label, so it can't use `format`"));
                    }
                    target.format = match word {
                        "point" => Format::Point,
                        "complex" => Format::Complex,
                        _ => Format::Polar,
                    };
                }
                _ => return Err(format!("`{prop}` can't be `{word}`")),
            }
            continue;
        }
        // `none` takes back what an earlier rule set, as in CSS: `dash: none` is a solid line.
        let clear = matches!(value, Expr::Sym(word) if word == "none");
        let value = &with_story(value, mark);
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
                | "trim"
                | "ticks"
                | "tick"
                | "leader"
                | "leader_gap"
                | "digits"
        );
        let mut scope = names.to_vec();
        if prop == "width" {
            scope.push(SELF_T);
        }
        // A curve's opacity can vary along it too, and for a curve of points in space both
        // can follow the points' third coordinate.
        let curve = match form {
            Form::Parametric(arc) => Some(arc),
            Form::Many {
                shape: Shape::Curves(arc),
                ..
            } => Some(arc),
            _ => None,
        };
        if let (Some(arc), "width" | "opacity") = (curve, prop.as_str()) {
            scope.push(SELF_T);
            if arc.depth.is_some() {
                scope.push(SELF_Z);
            }
        }
        if prop == "show" {
            scope.push(VIEW_WIDTH);
        }
        // A collection's sizes are worked out per instance, so they can use its variables,
        // and how big a world unit is, to size something in the figure's own terms.
        if let (Form::Many { vars, lets, .. }, "radius" | "width" | "trim") = (form, prop.as_str())
        {
            scope.extend(vars.iter().map(|(name, _)| name.as_str()));
            scope.extend(lets.iter().map(|(name, _)| name.as_str()));
            scope.push(SELF_SCALE);
        }
        // A side's color and fade are worked out per instance too, and so is the color of each
        // line: a stroke, and the fill of a dot, which is drawn as one.
        let per_side = (facet.is_some() && prop == "fill") || prop == "fill_opacity";
        let dots = matches!(
            form,
            Form::Many {
                shape: Shape::Dots(_),
                ..
            }
        );
        let per_line = facet.is_none() && (prop == "stroke" || (dots && prop == "fill"));
        if let (Form::Many { vars, lets, .. }, true) =
            (form, (many_circles && per_side) || per_line)
        {
            scope.extend(vars.iter().map(|(name, _)| name.as_str()));
            scope.extend(lets.iter().map(|(name, _)| name.as_str()));
        }
        // `fill_opacity` is a measure that can still fade with distance, as a glow does.
        // And so can the `opacity` of a parametric curve whose stroke is drawn per pixel.
        let traced = matches!(form, Form::Parametric(_));
        let spatial = !measure || prop == "fill_opacity" || (traced && prop == "opacity");
        if !clear {
            let ty = type_of(value, &scope, spatial)?;
            let want = if measure { Ty::Real } else { Ty::Color };
            if ty != want {
                return Err(format!("`{prop}` needs a {want:?}, found a {ty:?}"));
            }
        }
        // A parametric curve's stroke has a distance when it's drawn per pixel, which is checked
        // once the whole sheet is read.
        let traced_stroke = traced && (prop == "stroke" || prop == "opacity");
        if spatial
            && !spatial_ok
            && !traced_stroke
            && (uses(value, SELF_DIST) || uses(value, SELF_PIXEL))
        {
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
        // Drawing part of a stroke needs a start: a parameter, or the first of its points.
        if prop == "trim" && !clear {
            let has_start = match form {
                Form::Parametric(_) | Form::Polyline { .. } | Form::Arrow { .. } => true,
                Form::Field { line, .. } => line.is_some(),
                Form::Many { shape, .. } => matches!(shape, Shape::Curves(_)),
                _ => false,
            };
            if !has_start {
                return Err(format!(
                    "`{id}` has no start to draw from, so it can't use `trim`: it's traced, a \
                     region, a collection of circles or dots, or not a line"
                ));
            }
        }
        if matches!(prop.as_str(), "ticks" | "tick")
            && !clear
            && !matches!(form, Form::Polyline { points, closed: false } if points.len() == 2)
        {
            return Err(format!("`{id}` isn't a segment, so it can't use `{prop}`"));
        }
        if matches!(prop.as_str(), "leader" | "leader_gap" | "digits")
            && !clear
            && !matches!(form, Form::Label { .. })
        {
            return Err(format!("`{id}` isn't a label, so it can't use `{prop}`"));
        }
        let slot = match (facet, prop.as_str()) {
            (None, "trim") => &mut target.trim,
            (None, "ticks") => &mut target.ticks,
            (None, "digits") => &mut target.digits,
            (Some("inside"), "fill") if round => &mut target.inside,
            (Some("outside"), "fill") if round => &mut target.outside,
            (Some("inside" | "negative"), "fill") => &mut target.negative,
            (Some("outside" | "positive"), "fill") => &mut target.positive,
            (None, "stroke") => &mut target.stroke,
            (None, "opacity") => &mut target.opacity,
            (None, "fill_opacity") => &mut target.fill_opacity,
            (None, "show") => &mut target.show,
            (None, "head_span") => &mut target.head_span,
            (
                None,
                "width" | "dash" | "gap" | "radius" | "size" | "halo" | "head" | "distance"
                | "tick" | "leader" | "leader_gap",
            ) => {
                // A length: a bare number is in the figure's own units, and `px` and `em` convert.
                match prop.as_str() {
                    "width" => &mut target.width,
                    "dash" => &mut target.dash,
                    "gap" => &mut target.gap,
                    "radius" => &mut target.radius,
                    "size" => &mut target.size,
                    "head" => &mut target.head,
                    "distance" => &mut target.distance,
                    "tick" => &mut target.tick,
                    "leader" => &mut target.leader,
                    "leader_gap" => &mut target.leader_gap,
                    _ => &mut target.halo,
                }
            }
            (None, "fill") => {
                if (many_circles || matches!(form, Form::Field { .. })) && clear {
                    return Err(format!(
                        "`{id}` is filled by its sides, which have no fill of their own until a \
                         sheet gives them one, so `fill: none` needs a facet, such as \
                         `{id}::inside`"
                    ));
                }
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
        // A paint written `none` is no paint, which a default doesn't fill in. Every other
        // property's `none` only takes back what an earlier rule set.
        match prop.as_str() {
            "stroke" => target.no_stroke = clear,
            "fill" => target.no_fill = clear,
            _ => {}
        }
        *slot = if clear { None } else { Some(value.clone()) };
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

    /// A style reads the figure's `duration` as a number, so the CPU and the GPU see the same
    /// value and neither needs the name.
    #[test]
    fn a_style_reads_duration() {
        let doc = format!("duration 4\n{DOC}");
        let f = build(&doc, &["style c { stroke: black; width: duration·1px }"]).unwrap();
        let width = f.styles.marks[0].width.clone().unwrap();
        let four = Expr::app(
            "Multiply",
            vec![Expr::Num(4.0), Expr::app("Pixels", vec![Expr::Num(1.0)])],
        );
        assert_eq!(width, four);
        // Without a statement, a figure's own `duration` is its own, as with `pi`.
        let doc = format!("param duration = 1 in 0..2\n{DOC}");
        let f = build(&doc, &["style c { stroke: black; width: duration·1px }"]).unwrap();
        let width = f.styles.marks[0].width.clone().unwrap();
        let mut named = false;
        width.visit_syms(&mut |n| named |= n == "duration");
        assert!(named, "the param, read each frame: {width:?}");
        // A sheet's own `duration` is the sheet's, as any name a sheet defines is.
        let doc = format!("duration 4\n{DOC}");
        let f = build(
            &doc,
            &["let duration = 9\nstyle c { stroke: black; width: duration·1px }"],
        )
        .unwrap();
        let nine = Expr::app(
            "Multiply",
            vec![Expr::Num(9.0), Expr::app("Pixels", vec![Expr::Num(1.0)])],
        );
        assert_eq!(f.styles.marks[0].width.clone().unwrap(), nine);
        // With no statement it's what beats with lengths add up to, and nothing without them.
        let f = build(
            &format!("beat a for 2\npause 0.5\n{DOC}"),
            &["style c { stroke: black; width: duration·1px }"],
        )
        .unwrap();
        let sum = Expr::app(
            "Multiply",
            vec![Expr::Num(2.5), Expr::app("Pixels", vec![Expr::Num(1.0)])],
        );
        assert_eq!(f.styles.marks[0].width.clone().unwrap(), sum);
        let err = build(DOC, &["style c { width: duration·1px }"]).unwrap_err();
        assert!(err.message.contains(crate::compile::NO_DURATION), "{err}");
    }

    /// `none` takes back what an earlier rule set, so one ray can be solid among dashed ones.
    #[test]
    fn none_takes_a_property_back() {
        let doc =
            "let O = (0, 0)\nlet A = (1, 0)\ncurve base = ray(O, A)\ncurve other = ray(O, (0, 1))";
        let f = build(
            doc,
            &["style ray { stroke: black; dash: 10px; gap: 8px }\nstyle base { dash: none; stroke: none }"],
        )
        .unwrap();
        let [base, other] = [&f.styles.marks[0], &f.styles.marks[1]];
        assert!(base.dash.is_none() && base.stroke.is_none());
        assert!(base.gap.is_some(), "only what's named is taken back");
        assert!(other.dash.is_some() && other.stroke.is_some());
    }

    /// A sheet's function is inlined where it's called, from a rule or a name, with its
    /// parameters shadowing the sheet's names.
    #[test]
    fn a_sheet_can_define_a_function() {
        let doc = "let O = (0, 0)\nlet A = (1, 0)\ncurve base = ray(O, A)";
        let theme = "let soft = 4\nfn glow(d, w) = exp(-d / w) * soft\nlet faint = glow(2, 8)";
        let f = build(
            doc,
            &[
                theme,
                "let w = 100\nstyle base { stroke: black; width: glow(1, 2); opacity: faint }",
            ],
        )
        .unwrap();
        let env = crate::eval::Env::new(0.0);
        let value = |e: &Option<Expr>| crate::eval::real(e.as_ref().unwrap(), &env).unwrap();
        let base = &f.styles.marks[0];
        assert!(
            (value(&base.width) - (-0.5f64).exp() * 4.0).abs() < 1e-12,
            "`w` is the parameter, not the sheet's 100"
        );
        assert!((value(&base.opacity) - (-0.25f64).exp() * 4.0).abs() < 1e-12);
    }

    #[test]
    fn an_unknown_name_in_a_sheet_suggests_what_was_meant() {
        let doc = "let O = (0, 0)\nlet A = (1, 0)\ncurve base = ray(O, A)";
        for (sheet, wanted) in [
            (
                "let ink = black\nstyle base { stroke: inkk }",
                "did you mean `ink`",
            ),
            (
                "style base { stroke: black; width: sqtr(4) }",
                "did you mean `sqrt`",
            ),
        ] {
            let err = build(doc, &[sheet]).unwrap_err().to_string();
            assert!(err.contains(wanted), "{sheet}: {err}");
        }
    }

    #[test]
    fn a_sheets_function_says_what_went_wrong() {
        let doc = "let O = (0, 0)\nlet A = (1, 0)\ncurve base = ray(O, A)";
        for (sheet, wanted) in [
            (
                "style base { stroke: black; width: glow(1, 2) }",
                "no function `glow`",
            ),
            (
                "fn glow(d) = d\nstyle base { stroke: black; width: glow(1, 2) }",
                "takes 1 argument, not 2",
            ),
            (
                "fn glow(d) = glow(d)\nstyle base { stroke: black; width: glow(1) }",
                "calls itself",
            ),
        ] {
            let error = build(doc, &[sheet]).unwrap_err().to_string();
            assert!(error.contains(wanted), "{sheet}: {error}");
        }
    }

    /// A dash of no length is no dashing: the line is drawn solid, not as nothing.
    #[test]
    fn a_dash_of_zero_is_solid() {
        let doc = "let O = (0, 0)\nlet A = (1, 0)\ncurve base = ray(O, A)";
        let f = build(
            doc,
            &["style base { stroke: black; width: 2px; dash: 0; gap: 8px }"],
        )
        .unwrap();
        let p = crate::frame::prepare(&f, &crate::frame::Inputs::at(0.0), 64, 64, 1.0).unwrap();
        let segments = &p.strokes[0];
        assert!(!segments.is_empty());
        // Each segment carries its dash and period, and both are 0 for a solid line.
        for s in segments.chunks(crate::stroke::FLOATS_PER_SEGMENT) {
            assert_eq!((s[8], s[9]), (0.0, 0.0));
        }
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

    /// A parametric curve's stroke can be drawn per pixel, by its distance to the curve as sampled.
    #[test]
    fn a_parametric_stroke_can_be_a_field() {
        let doc = "curve p = parametric (t): (cos(t), sin(t)) for t in 0..6";
        let glow = "style p { draw: field; width: 40px; stroke: white·tanh(self.pixel / abs(self.dist)); opacity: tanh(self.pixel / abs(self.dist)) }";
        let f = build(doc, &[glow]).unwrap();
        assert!(!f.styles.marks[0].stroke_is_line(&f.scene.marks[0].form));
        assert!(crate::nearest::measures(
            &f.scene.marks[0],
            &f.styles.marks[0]
        ));
        // As a line it has no distance, and the error says what to add.
        let err = build(
            doc,
            &["style p { stroke: white·tanh(self.pixel / abs(self.dist)) }"],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("add `draw: field`"), "{err}");
        let faded = "style p { stroke: white; opacity: tanh(1 / abs(self.dist)) }";
        let err = build(doc, &[faded]).unwrap_err().to_string();
        assert!(err.contains("add `draw: field`"), "{err}");
        // It still has no sides.
        let err = build(doc, &["style p::inside { fill: white }"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("has no sides"), "{err}");
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
            ("style c { dash: self.t * 1px }", "unknown name `self.t`"),
            ("style c { draw: sketchy }", "can't be `sketchy`"),
            ("style c { cap: wobbly }", "can't be `wobbly`"),
            ("style c { blend: 2 }", "needs a keyword"),
            ("style nope { stroke: white }", "no mark named"),
            ("style c::sideways { fill: white }", "unknown facet"),
            ("style c { fill: white }", "needs a facet"),
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
        // What a rule doesn't reach is drawn in the default ink, black.
        let f = build(BEATS, &["style circle { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke == Some(Expr::sym("white"));
        assert!(has(0) && !has(1) && has(2), "by type");

        let f = build(BEATS, &["style .aux { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke == Some(Expr::sym("white"));
        assert!(!has(0) && has(1) && !has(2), "by tag");

        let f = build(BEATS, &["style circle.blue { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke == Some(Expr::sym("white"));
        assert!(has(0) && !has(1) && !has(2), "conjunction");

        let f = build(BEATS, &["style c, b { stroke: white }"]).unwrap();
        let has = |i: usize| f.styles.marks[i].stroke == Some(Expr::sym("white"));
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
        let doc = "curve p = parametric (t): (t, t) for t in 0..1\nsegment s = (0, 0), (1, 1)";
        let err = build(doc, &["style s { draw: field }"]).unwrap_err();
        assert!(err.message.contains("has no field"), "{err}");
        let err = build(doc, &["style s { stroke: mix(white, black, self.dist) }"]).unwrap_err();
        assert!(err.message.contains("has no distance"), "{err}");
        // A parametric curve has one only when it's drawn per pixel.
        let err = build(doc, &["style p { stroke: mix(white, black, self.dist) }"]).unwrap_err();
        assert!(err.message.contains("has no distance"), "{err}");
    }
}
