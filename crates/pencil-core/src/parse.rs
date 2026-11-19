//! A small hand-written parser. The syntax is expected to change, so it stays
//! cheap to rewrite; everything downstream only sees `Expr` and `Item`.

use crate::error::Error;
use crate::ir::Expr;

/// What a collection draws at each combination of its variables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ManyKind {
    Dots,
    Circles,
    Curves,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub name: Option<String>,
    pub items: Vec<Item>,
}

/// A beat with a length, as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub name: Option<String>,
    pub caption: Option<String>,
    pub length: Expr,
    pub show: Vec<String>,
    pub drop: Vec<String>,
    pub focus: Vec<Focus>,
    pub tags: Vec<String>,
    /// Whether it has a block, closed by an `EndBeat`.
    pub block: bool,
    pub line: usize,
}

/// What a beat is about: a mark or `.tag`, and maybe only while a choice is on one option, as in
/// `.middle if case = "q = 2p"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Focus {
    pub word: String,
    /// The choice, and the option, as written.
    pub when: Option<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Let {
        name: String,
        expr: Expr,
        /// The expression as written, for a label that shows it. Empty for a `let` nobody wrote.
        text: String,
        line: usize,
    },
    Fn {
        name: String,
        params: Vec<String>,
        body: Expr,
        line: usize,
    },
    Param {
        name: String,
        default: Expr,
        range: [Expr; 2],
        step: Option<Expr>,
        line: usize,
    },
    /// `param axis in {ω, 1, 1 + ω}`: a param that's one of these values. Each comes with its
    /// text as written, which is how a page names it, and so is the default if there is one.
    Choice {
        name: String,
        default: Option<String>,
        options: Vec<(Expr, String)>,
        line: usize,
    },
    Bind {
        name: String,
        expr: Expr,
        line: usize,
    },
    Duration(Expr),
    /// `poster { time: 3.5, roll: 0.6 }`: the moment a still of the figure shows.
    Poster {
        values: Vec<(String, Expr)>,
        line: usize,
    },
    Curve {
        name: String,
        form: CurveSyntax,
        line: usize,
    },
    Segment {
        name: String,
        points: Vec<Expr>,
        closed: bool,
        line: usize,
    },
    RightAngle {
        name: String,
        points: [Expr; 3],
        size: Expr,
        line: usize,
    },
    Region {
        name: String,
        parts: Vec<PartSyntax>,
        line: usize,
    },
    Dots {
        names: Vec<String>,
        line: usize,
    },
    Label {
        name: String,
        /// Without any, a panel gives it with `text`, or doesn't show it.
        text: Option<String>,
        at: Expr,
        offset: Option<Expr>,
        /// Which way from its point, with the distance in its style.
        away: Option<Expr>,
        line: usize,
    },
    /// What a label says in this panel: a thing's name, in this space.
    Text {
        name: String,
        text: String,
        line: usize,
    },
    /// A point's dot, after a map.
    DotAt {
        name: String,
        at: Expr,
        line: usize,
    },
    /// A straight segment, after a map, which can bend it. `var` runs from 0 to 1.
    ArcSegment {
        name: String,
        var: String,
        point: Expr,
        line: usize,
    },
    /// Unknowns defined by equations, found by Newton's method each frame.
    Solve {
        unknowns: Vec<String>,
        start: Vec<Expr>,
        equations: Vec<(Expr, Expr)>,
        line: usize,
    },
    /// One mark drawn many times, once for each combination of its variables.
    Many {
        name: String,
        kind: ManyKind,
        value: Expr,
        /// A curve's own parameter and its range, for `curves`.
        param: Option<(String, [Expr; 2])>,
        vars: Vec<(String, Over)>,
        lets: Vec<(String, Expr)>,
        line: usize,
    },
    /// Its own view of the figure, with its own marks. What's outside any panel is shared.
    Panel {
        name: String,
        title: Option<String>,
        formula: Option<String>,
        items: Vec<Item>,
    },
    /// Things to draw in more than one place, each declared once.
    Group {
        name: String,
        params: Vec<String>,
        items: Vec<Item>,
    },
    Use {
        name: String,
        args: Vec<Expr>,
        /// A function to draw what the group declares through.
        through: Option<String>,
        /// A prefix for what the group declares, so it can be drawn more than once.
        alias: Option<String>,
        line: usize,
    },
    /// `use "file"`: another file's statements, here. `files` reads them in, so `compile` never
    /// sees one.
    UseFile {
        /// As written, from the file that says it.
        path: String,
        line: usize,
        col: usize,
    },
    /// The rectangle another panel is looking at.
    Outline {
        name: String,
        panel: String,
        line: usize,
    },
    /// Where an expression can sit: a moving point, an offset from it, and which end it hangs from.
    Place {
        name: String,
        at: Expr,
        offset: Option<Expr>,
        hang: Option<String>,
        line: usize,
    },
    /// A list of states, each a list of written expressions and where they sit.
    Expression {
        name: String,
        by: Expr,
        states: Vec<Vec<(String, String)>>,
        line: usize,
    },
    Arrow {
        name: String,
        from: Expr,
        to: Expr,
        line: usize,
    },
    /// Opens a beat: a discrete step of a construction, with what it shows inside.
    Beat {
        caption: String,
        tags: Vec<String>,
        focus: Vec<Focus>,
    },
    EndBeat,
    /// A beat with a length: one move of a figure's story. Its marks are the block's, if it has
    /// one, and `show`, `drop` and `focus` name marks declared anywhere, by name or `.tag`.
    Move(Move),
    /// A hold between moves, in seconds.
    Pause(Expr),
    /// Tags the marks the statement before it made.
    Tag(Vec<String>),
    /// Dragging a mark sets one param, or two. Each expression is the explicit inverse from
    /// the pointer to its param, and each param's range is its constraint.
    Handle {
        mark: String,
        sets: Vec<HandleSet>,
        line: usize,
    },
    Viewport {
        x: Option<[Expr; 2]>,
        y: Option<[Expr; 2]>,
        fit: Vec<String>,
        pad: Option<Expr>,
        /// How far the view is turned about its vertical axis, for points in space.
        yaw: Option<Expr>,
    },
    Style(StyleRule),
}

/// What a collection's variable counts over: the whole numbers of a range, or a list of values in
/// the order written.
#[derive(Clone, Debug, PartialEq)]
pub enum Over {
    Range([Expr; 2]),
    List(Vec<Expr>),
}

impl Over {
    pub fn exprs(&self) -> &[Expr] {
        match self {
            Over::Range(range) => range,
            Over::List(values) => values,
        }
    }

    pub fn map(&self, f: impl Fn(&Expr) -> Expr) -> Over {
        match self {
            Over::Range([lo, hi]) => Over::Range([f(lo), f(hi)]),
            Over::List(values) => Over::List(values.iter().map(f).collect()),
        }
    }
}

/// One `param = expr` of a handle, with where the param's name is written.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleSet {
    pub param: String,
    pub inverse: Expr,
    pub line: usize,
    pub col: usize,
}

/// How a curve was written. Each form keeps its own representation downstream.
#[derive(Clone, Debug, PartialEq)]
pub enum CurveSyntax {
    Implicit {
        vars: [String; 2],
        lhs: Expr,
        rhs: Expr,
    },
    Sdf {
        vars: [String; 2],
        field: Expr,
    },
    Parametric {
        var: String,
        point: Expr,
        range: [Expr; 2],
    },
    /// A construction's value, such as `circle(O, A)` or `ray(O, B)`.
    Value(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PartSyntax {
    Point(Expr),
    Arc {
        var: String,
        point: Expr,
        range: [Expr; 2],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub decls: Vec<(String, Expr)>,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    /// A mark's name or type. Empty for a selector of tags alone.
    pub target: String,
    pub tags: Vec<String>,
    pub facet: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Px,
    Em,
    Punct(&'static str),
    /// A superscript digit such as `²`.
    Sup(f64),
    Str(String),
    Eof,
}

/// How a token reads in an error: as it was written.
impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tok::Ident(word) => write!(f, "`{word}`"),
            Tok::Num(n) => write!(f, "`{n}`"),
            Tok::Px => write!(f, "`px`"),
            Tok::Em => write!(f, "`em`"),
            Tok::Punct(p) => write!(f, "`{p}`"),
            Tok::Sup(n) => write!(f, "a superscript {n}"),
            Tok::Str(text) => write!(f, "the text {text:?}"),
            Tok::Eof => write!(f, "the end of the file"),
        }
    }
}

struct Lexed {
    tok: Tok,
    line: usize,
    col: usize,
    /// Whether a space or a comment comes right before it, which is what tells `1/2 π` from `1/2π`.
    spaced: bool,
}

const PUNCT: [&str; 16] = [
    "::", "..", "+", "-", "*", "/", "^", "(", ")", "{", "}", ",", ":", "=", ";", ".",
];

/// A name with its subscript written one way, as `x_1`: `x1`, `x₁` and `x_{1}` (which the lexer
/// has already unbraced) are the same name, as they are in Desmos and LaTeX. Digits in the
/// middle of a name, as in `p2q`, aren't a subscript, and a name has one subscript, so
/// `f_X175` stays as it is.
pub fn subscripted(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 1);
    for c in name.chars() {
        match c {
            '₀'..='₉' => {
                if !out.contains('_') && !out.ends_with(|d: char| d.is_ascii_digit()) {
                    out.push('_');
                }
                out.push(char::from_digit(c as u32 - '₀' as u32, 10).unwrap());
            }
            _ => out.push(c),
        }
    }
    let stem = out.trim_end_matches(|c: char| c.is_ascii_digit());
    if stem.len() < out.len() && !stem.is_empty() && !stem.contains('_') {
        out.insert(stem.len(), '_');
    }
    out
}

/// The head a style's `text: expr = value` is kept under: its parts, in order.
pub const TEXT_PARTS: &str = "TextParts";

/// Glyphs that only ever name a constant, so they never join the letters around them.
const GLYPHS: [char; 4] = ['π', 'ℯ', 'ⅈ', 'ⅇ'];

fn lex(src: &str) -> Result<Vec<Lexed>, Error> {
    let chars: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut col) = (0, 1, 1);
    let mut out: Vec<Lexed> = Vec::new();
    // Whether anything but a token has been passed since the last one.
    let mut gap = false;
    while i < chars.len() {
        let c = chars[i];
        let (start_line, start_col) = (line, col);
        let first = out.len();
        let advance = |n: usize, i: &mut usize, col: &mut usize| {
            *i += n;
            *col += n;
        };
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
        } else if c.is_whitespace() {
            advance(1, &mut i, &mut col);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                advance(1, &mut i, &mut col);
            }
        } else if c.is_ascii_digit() {
            let start = i;
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            // `2..3` is a range, so a dot only belongs to the number before a digit.
            if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if matches!(chars.get(j), Some('e' | 'E')) {
                let mut k = j + 1;
                if matches!(chars.get(k), Some('+' | '-')) {
                    k += 1;
                }
                if chars.get(k).is_some_and(char::is_ascii_digit) {
                    j = k;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                }
            }
            let text: String = chars[start..j].iter().collect();
            let value = text
                .parse()
                .map_err(|_| Error::at(line, col, format!("bad number `{text}`")))?;
            advance(j - i, &mut i, &mut col);
            out.push(Lexed {
                spaced: false,
                tok: Tok::Num(value),
                line: start_line,
                col: start_col,
            });
            // A unit right after a number: `px` or `em`, and not the start of a longer name.
            let unit = match (chars.get(i), chars.get(i + 1)) {
                (Some('p'), Some('x')) => Some(Tok::Px),
                (Some('e'), Some('m')) => Some(Tok::Em),
                _ => None,
            }
            .filter(|_| {
                !chars
                    .get(i + 2)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            });
            if let Some(tok) = unit {
                out.push(Lexed {
                    tok,
                    line,
                    col,
                    spaced: false,
                });
                advance(2, &mut i, &mut col);
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            // A constant's own glyph is a name by itself, so `iπ` and `2πr` are products. It can
            // still have a subscript, as `π_1`.
            let glyph = GLYPHS.contains(&c);
            if glyph {
                advance(1, &mut i, &mut col);
            }
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                // Superscripts count as alphanumeric, but they are exponents here.
                if matches!(chars[i], '²' | '³') || GLYPHS.contains(&chars[i]) {
                    break;
                }
                let subscript =
                    chars[i] == '_' || chars[i].is_ascii_digit() || ('₀'..='₉').contains(&chars[i]);
                if glyph && !subscript {
                    break;
                }
                advance(1, &mut i, &mut col);
            }
            let mut name: String = chars[start..i].iter().collect();
            // `x_{1}`, as LaTeX writes a subscript.
            if name.ends_with('_') && chars.get(i) == Some(&'{') {
                let inner: String = chars[i + 1..]
                    .iter()
                    .take_while(|c| c.is_alphanumeric())
                    .collect();
                let close = i + 1 + inner.chars().count();
                if inner.is_empty() || chars.get(close) != Some(&'}') {
                    return Err(Error::at(
                        line,
                        col,
                        "a subscript in braces holds letters and digits, as in `x_{1}`",
                    ));
                }
                name.push_str(&inner);
                advance(close + 1 - i, &mut i, &mut col);
            }
            out.push(Lexed {
                spaced: false,
                tok: Tok::Ident(subscripted(&name)),
                line: start_line,
                col: start_col,
            });
        } else if c == '"' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '"' && chars[j] != '\n' {
                j += 1;
            }
            if chars.get(j) != Some(&'"') {
                return Err(Error::at(line, col, "unterminated string"));
            }
            let text: String = chars[start..j].iter().collect();
            advance(j + 1 - i, &mut i, &mut col);
            out.push(Lexed {
                spaced: false,
                tok: Tok::Str(text),
                line: start_line,
                col: start_col,
            });
        } else if c == '²' || c == '³' {
            let n = if c == '²' { 2.0 } else { 3.0 };
            advance(1, &mut i, &mut col);
            out.push(Lexed {
                spaced: false,
                tok: Tok::Sup(n),
                line: start_line,
                col: start_col,
            });
        } else if c == '·' {
            advance(1, &mut i, &mut col);
            out.push(Lexed {
                spaced: false,
                tok: Tok::Punct("*"),
                line: start_line,
                col: start_col,
            });
        } else if c == '±' || c == '∓' {
            advance(1, &mut i, &mut col);
            out.push(Lexed {
                spaced: false,
                tok: Tok::Punct(if c == '±' { "±" } else { "∓" }),
                line: start_line,
                col: start_col,
            });
        } else {
            let rest: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let Some(p) = PUNCT.iter().find(|p| rest.starts_with(**p)) else {
                return Err(Error::at(line, col, format!("unexpected character `{c}`")));
            };
            advance(p.chars().count(), &mut i, &mut col);
            out.push(Lexed {
                spaced: false,
                tok: Tok::Punct(p),
                line: start_line,
                col: start_col,
            });
        }
        match out.get_mut(first) {
            Some(token) => {
                token.spaced = gap;
                gap = false;
            }
            None => gap = true,
        }
    }
    out.push(Lexed {
        spaced: false,
        tok: Tok::Eof,
        line,
        col,
    });
    Ok(out)
}

struct Parser {
    toks: Vec<Lexed>,
    pos: usize,
    /// The source, and where each line starts in it, so a piece can be quoted as written.
    src: Vec<char>,
    line_starts: Vec<usize>,
    /// How far this text's lines are numbered on from 1, when it's one file of several.
    base: usize,
    /// Where the statement being read first says `±` or `∓`, which only a collection may.
    branches: Option<(usize, usize)>,
}

/// Builtin names in source, and their MathJSON heads. Anything else is left under
/// its own name for `compile` to resolve as a user function. `docs/language.md` must
/// describe every one, and a test holds it to that.
pub const FUNCTIONS: [(&str, &str); 68] = [
    ("sqrt", "Sqrt"),
    ("sin", "Sin"),
    ("cos", "Cos"),
    ("tan", "Tan"),
    ("acos", "Arccos"),
    ("arccos", "Arccos"),
    ("exp", "Exp"),
    ("ln", "Ln"),
    ("abs", "Abs"),
    ("sign", "Sign"),
    ("floor", "Floor"),
    ("atan2", "Arctan2"),
    ("oklch", "Oklch"),
    ("rgb", "Rgb"),
    ("mix", "Mix"),
    ("hue", "Hue"),
    ("min", "Min"),
    ("max", "Max"),
    ("clamp", "Clamp"),
    ("mod", "Mod"),
    ("smoothstep", "Smoothstep"),
    ("reveal", "Reveal"),
    ("length", "Abs"),
    ("dist", "Distance"),
    ("dot", "Dot"),
    ("midpoint", "Midpoint"),
    ("circle", "Circle"),
    ("line", "Line"),
    ("ray", "Ray"),
    ("perpendicular", "Perpendicular"),
    ("intersect", "Intersect"),
    ("mirror", "Mirror"),
    ("exists", "Exists"),
    ("otherwise", "Otherwise"),
    ("snap", "Snap"),
    ("timeline", "Timeline"),
    ("real", "Real"),
    ("imag", "Imaginary"),
    ("conj", "Conjugate"),
    ("arg", "Arg"),
    ("sinh", "Sinh"),
    ("cosh", "Cosh"),
    ("tanh", "Tanh"),
    ("coth", "Coth"),
    ("sech", "Sech"),
    ("csch", "Csch"),
    ("arctanh", "Artanh"),
    ("artanh", "Artanh"),
    ("sec", "Sec"),
    ("csc", "Csc"),
    ("cot", "Cot"),
    ("arcsin", "Arcsin"),
    ("asin", "Arcsin"),
    ("arctan", "Arctan"),
    ("atan", "Arctan"),
    ("arcsec", "Arcsec"),
    ("arccsc", "Arccsc"),
    ("arccot", "Arccot"),
    ("arcsinh", "Arsinh"),
    ("arsinh", "Arsinh"),
    ("arccosh", "Arcosh"),
    ("arcosh", "Arcosh"),
    ("arccoth", "Arcoth"),
    ("arcoth", "Arcoth"),
    ("log", "Log"),
    ("ceil", "Ceil"),
    ("round", "Round"),
    ("flip", "Flip"),
];

/// Words that can follow an expression on its line, so they end it rather than multiply it.
const AFTER_EXPRESSIONS: [&str; 19] = [
    "as", "at", "away", "by", "drop", "focus", "for", "from", "hang", "if", "in", "offset", "sets",
    "show", "size", "step", "tag", "through", "with",
];

/// `a ± b`, and `±a` with one argument: both signs, one for each of a collection's two branches.
pub const PLUS_MINUS: &str = "PlusMinus";
/// `a ∓ b` and `∓a`: the opposite sign to `±` in each branch.
pub const MINUS_PLUS: &str = "MinusPlus";

/// Words that can follow `beat` in a beat with a length, so none of them can be its name.
const MOVE_WORDS: [&str; 5] = ["for", "show", "drop", "focus", "tag"];

impl Parser {
    /// Whether the `beat` just read has a length: `beat name "caption" for 2`, with the name
    /// and caption optional. The older `beat "caption" { … }` is a step with no length.
    fn is_move(&self) -> bool {
        let mut at = self.pos;
        let tok = |i: usize| self.toks.get(i).map(|t| &t.tok);
        if matches!(tok(at), Some(Tok::Ident(w)) if !MOVE_WORDS.contains(&w.as_str())) {
            at += 1;
        }
        if matches!(tok(at), Some(Tok::Str(_))) {
            at += 1;
        }
        matches!(tok(at), Some(Tok::Ident(w)) if w == "for")
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn line(&self) -> usize {
        self.toks[self.pos].line
    }

    fn error(&self, message: impl Into<String>) -> Error {
        let t = &self.toks[self.pos];
        Error::at(t.line, t.col, message)
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Ident(w) if w == word)
    }

    fn eat(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Tok::Punct(q) if *q == p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> Result<(), Error> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.error(format!("expected `{p}`, found {}", self.peek())))
        }
    }

    fn ident(&mut self, what: &str) -> Result<String, Error> {
        match self.peek() {
            Tok::Ident(_) => match self.bump() {
                Tok::Ident(name) => Ok(name),
                _ => unreachable!(),
            },
            other => Err(self.error(format!("expected {what}, found {other}"))),
        }
    }

    fn keyword(&mut self, word: &str) -> Result<(), Error> {
        let found = self.ident(&format!("`{word}`"))?;
        if found == word {
            Ok(())
        } else {
            Err(self.error(format!("expected `{word}`, found `{found}`")))
        }
    }

    fn document(&mut self) -> Result<Document, Error> {
        let mut name = None;
        let mut items = Vec::new();
        while *self.peek() != Tok::Eof {
            self.statement(&mut name, &mut items)?;
        }
        Ok(Document { name, items })
    }

    /// One statement. `±` and `∓` draw two of a mark, so only a collection can say them, and
    /// one that's anywhere else is an error at the sign, naming the collection to use.
    fn statement(&mut self, name: &mut Option<String>, items: &mut Vec<Item>) -> Result<(), Error> {
        let outer = self.branches.take();
        let word = match self.peek() {
            Tok::Ident(word) => word.clone(),
            _ => String::new(),
        };
        self.statement_itself(name, items)?;
        if let Some((line, col)) = self.branches
            && !matches!(word.as_str(), "dots" | "circles" | "curves")
        {
            let plural = match word.as_str() {
                "curve" => "a `curves` collection, or `circles` for a circle",
                _ => "a collection: `dots` for points, `circles` for circles, `curves` for curves",
            };
            return Err(Error::at(
                line,
                col,
                format!("`±` draws two marks, one for each sign, so it belongs in {plural}"),
            ));
        }
        self.branches = outer;
        Ok(())
    }

    fn statement_itself(
        &mut self,
        name: &mut Option<String>,
        items: &mut Vec<Item>,
    ) -> Result<(), Error> {
        let line = self.line();
        let word = self.ident("a statement")?;
        match word.as_str() {
            "doc" => {
                let mut full = self.ident("a document name")?;
                while self.eat("-") {
                    full.push('-');
                    full.push_str(&self.ident("a document name")?);
                }
                *name = Some(full);
            }
            "let" => {
                let name = self.ident("a name")?;
                self.expect("=")?;
                let from = self.pos;
                let expr = self.expr()?;
                // As written, less any comment after it.
                let written = self.text(from, self.pos);
                let text: Vec<&str> = written
                    .lines()
                    .map(|l| l.split("//").next().unwrap_or(l).trim())
                    .collect();
                let text = text.join(" ").trim().to_string();
                items.push(Item::Let {
                    name,
                    expr,
                    text,
                    line,
                });
            }
            "fn" => items.push(self.function(line)?),
            "param" => items.push(self.param(line)?),
            "bind" => {
                let name = self.ident("a param")?;
                self.expect("=")?;
                let expr = self.expr()?;
                items.push(Item::Bind { name, expr, line });
            }
            "duration" => items.push(Item::Duration(self.expr()?)),
            "poster" => {
                self.expect("{")?;
                let mut values = Vec::new();
                while !self.eat("}") {
                    let name = self.ident("`time` or a param")?;
                    self.expect(":")?;
                    values.push((name, self.expr()?));
                    if !self.eat(",") {
                        self.eat(";");
                    }
                }
                items.push(Item::Poster { values, line });
            }
            "curve" => items.push(self.curve(line)?),
            "segment" | "path" | "polygon" => {
                let name = self.ident("a name")?;
                self.expect("=")?;
                let points = self.expr_list()?;
                let closed = word == "polygon";
                items.push(Item::Segment {
                    name,
                    points,
                    closed,
                    line,
                });
            }
            "rightangle" => {
                let name = self.ident("a name")?;
                self.expect("=")?;
                let points = self.expr_list()?;
                self.keyword("size")?;
                let size = self.expr()?;
                let [a, b, c] = <[Expr; 3]>::try_from(points)
                    .map_err(|_| Error::at(line, 1, "`rightangle` takes three points"))?;
                items.push(Item::RightAngle {
                    name,
                    points: [a, b, c],
                    size,
                    line,
                });
            }
            "region" => items.push(self.region(line)?),
            "dot" => {
                let mut names = vec![self.ident("a point")?];
                while self.eat(",") {
                    names.push(self.ident("a point")?);
                }
                items.push(Item::Dots { names, line });
            }
            "label" => items.push(self.label(line)?),
            "text" => {
                let name = self.ident("a label's name")?;
                self.expect("=")?;
                let text = match self.bump() {
                    Tok::Str(text) => text,
                    other => {
                        return Err(Error::at(line, 1, format!("expected text, found {other}")));
                    }
                };
                items.push(Item::Text { name, text, line });
            }
            "solve" => {
                let mut unknowns = Vec::new();
                if self.eat("(") {
                    while !self.eat(")") {
                        unknowns.push(self.ident("an unknown")?);
                        self.eat(",");
                    }
                } else {
                    unknowns.push(self.ident("an unknown")?);
                }
                self.keyword("from")?;
                let mut start = vec![self.expr()?];
                while self.eat(",") {
                    start.push(self.expr()?);
                }
                self.expect(":")?;
                let mut equations = Vec::new();
                loop {
                    let lhs = self.expr()?;
                    self.expect("=")?;
                    equations.push((lhs, self.expr()?));
                    if !self.eat(",") {
                        break;
                    }
                }
                items.push(Item::Solve {
                    unknowns,
                    start,
                    equations,
                    line,
                });
            }
            "outline" => {
                let outline = self.ident("an outline's name")?;
                self.expect("=")?;
                let panel = self.ident("a panel")?;
                items.push(Item::Outline {
                    name: outline,
                    panel,
                    line,
                });
            }
            "dots" => items.push(self.many(ManyKind::Dots, line)?),
            "circles" => items.push(self.many(ManyKind::Circles, line)?),
            "curves" => items.push(self.many(ManyKind::Curves, line)?),
            "panel" => {
                let panel = self.ident("a panel's name")?;
                let title = self.optional_string();
                let formula = self.optional_string();
                let inner = self.block(name, "panel")?;
                items.push(Item::Panel {
                    name: panel,
                    title,
                    formula,
                    items: inner,
                });
            }
            "group" => {
                let group = self.ident("a group's name")?;
                self.expect("(")?;
                let mut params = Vec::new();
                while !self.eat(")") {
                    params.push(self.ident("a parameter")?);
                    self.eat(",");
                }
                let inner = self.block(name, "group")?;
                items.push(Item::Group {
                    name: group,
                    params,
                    items: inner,
                });
            }
            "use" if matches!(self.peek(), Tok::Str(_)) => {
                let col = self.toks[self.pos].col;
                if let Tok::Str(path) = self.bump() {
                    items.push(Item::UseFile { path, line, col });
                }
                return Ok(());
            }
            "use" => {
                let group = self.ident("a group or a file's name in quotes")?;
                self.expect("(")?;
                let mut args = Vec::new();
                while !self.eat(")") {
                    args.push(self.expr()?);
                    self.eat(",");
                }
                let through = if self.at_word("through") {
                    self.bump();
                    Some(self.ident("a function")?)
                } else {
                    None
                };
                let alias = if self.at_word("as") {
                    self.bump();
                    Some(self.ident("a prefix")?)
                } else {
                    None
                };
                items.push(Item::Use {
                    name: group,
                    args,
                    through,
                    alias,
                    line,
                });
            }
            "place" => {
                let name = self.ident("a place")?;
                self.expect("=")?;
                let at = self.expr()?;
                let mut offset = None;
                let mut hang = None;
                loop {
                    if self.at_word("offset") {
                        self.bump();
                        offset = Some(self.expr()?);
                    } else if self.at_word("hang") {
                        self.bump();
                        hang = Some(self.ident("`start`, `middle` or `end`")?);
                    } else {
                        break;
                    }
                }
                items.push(Item::Place {
                    name,
                    at,
                    offset,
                    hang,
                    line,
                });
            }
            "expression" => {
                let name = self.ident("an expression's name")?;
                self.keyword("by")?;
                let by = self.expr()?;
                self.expect("{")?;
                let mut states = Vec::new();
                while !self.eat("}") {
                    if self.at_word("empty") {
                        self.bump();
                        states.push(Vec::new());
                        continue;
                    }
                    let mut pieces = Vec::new();
                    loop {
                        let text = match self.bump() {
                            Tok::Str(text) => text,
                            other => {
                                return Err(self.error(format!(
                                    "expected a written expression in quotes, found {other}"
                                )));
                            }
                        };
                        self.keyword("at")?;
                        pieces.push((text, self.ident("a place")?));
                        if !self.eat(",") {
                            break;
                        }
                    }
                    states.push(pieces);
                }
                items.push(Item::Expression {
                    name,
                    by,
                    states,
                    line,
                });
            }
            "arrow" => {
                let name = self.ident("an arrow's name")?;
                self.expect("=")?;
                let from = self.expr()?;
                self.expect(",")?;
                let to = self.expr()?;
                items.push(Item::Arrow {
                    name,
                    from,
                    to,
                    line,
                });
            }
            "handle" => {
                let mark = self.ident("a mark")?;
                self.keyword("sets")?;
                // One param, or two for a point dragged in two dimensions, each from the pointer.
                let mut sets: Vec<HandleSet> = Vec::new();
                loop {
                    let (at, col) = (self.line(), self.toks[self.pos].col);
                    let param = self.ident("a param")?;
                    if sets.iter().any(|s| s.param == param) {
                        return Err(Error::at(
                            at,
                            col,
                            format!("`handle {mark}` sets `{param}` twice"),
                        ));
                    }
                    self.expect("=")?;
                    let inverse = self.expr()?;
                    sets.push(HandleSet {
                        param,
                        inverse,
                        line: at,
                        col,
                    });
                    if !matches!(self.peek(), Tok::Punct(p) if *p == ",") {
                        break;
                    }
                    if sets.len() == 2 {
                        return Err(self.error(format!(
                            "`handle {mark}` sets one param or two, not more: a point has two coordinates"
                        )));
                    }
                    self.bump();
                }
                items.push(Item::Handle { mark, sets, line });
            }
            "beat" if self.is_move() => {
                let beat_name = match self.peek() {
                    Tok::Ident(w) if !MOVE_WORDS.contains(&w.as_str()) => {
                        Some(self.ident("a name")?)
                    }
                    _ => None,
                };
                let caption = self.optional_string();
                self.keyword("for")?;
                let length = self.expr()?;
                let (mut show, mut drop, mut focus, mut tags) = (vec![], vec![], vec![], vec![]);
                loop {
                    if self.at_word("focus") {
                        self.bump();
                        focus.extend(self.focus_list()?);
                        continue;
                    }
                    let list = match self.peek() {
                        Tok::Ident(w) if w == "show" => &mut show,
                        Tok::Ident(w) if w == "drop" => &mut drop,
                        Tok::Ident(w) if w == "tag" => &mut tags,
                        _ => break,
                    };
                    self.bump();
                    loop {
                        // A tag is written `.tag`, as in a selector.
                        let dotted = self.eat(".");
                        let word = self.ident("a mark or `.tag`")?;
                        list.push(if dotted { format!(".{word}") } else { word });
                        if !self.eat(",") {
                            break;
                        }
                    }
                }
                if tags.iter().any(|t| t.starts_with('.')) {
                    return Err(Error::at(line, 1, "a beat's `tag` takes plain words"));
                }
                let block = self.eat("{");
                items.push(Item::Move(Move {
                    name: beat_name,
                    caption,
                    length,
                    show,
                    drop,
                    focus,
                    tags,
                    block,
                    line,
                }));
                if block {
                    while !self.eat("}") {
                        if *self.peek() == Tok::Eof {
                            return Err(self.error("this `beat` isn't closed"));
                        }
                        self.statement(name, items)?;
                    }
                    items.push(Item::EndBeat);
                }
                return Ok(());
            }
            "pause" => items.push(Item::Pause(self.expr()?)),
            "beat" => {
                let caption = match self.bump() {
                    Tok::Str(text) => text,
                    other => {
                        return Err(Error::at(
                            line,
                            1,
                            format!("expected a caption, found {other}"),
                        ));
                    }
                };
                let tags = self.tags()?;
                let focus = if self.at_word("focus") {
                    self.bump();
                    self.focus_list()?
                } else {
                    Vec::new()
                };
                self.expect("{")?;
                items.push(Item::Beat {
                    caption,
                    tags,
                    focus,
                });
                while !self.eat("}") {
                    if *self.peek() == Tok::Eof {
                        return Err(self.error("this `beat` isn't closed"));
                    }
                    self.statement(name, items)?;
                }
                items.push(Item::EndBeat);
            }
            "viewport" => items.push(self.viewport()?),
            "style" => items.push(Item::Style(self.style(line)?)),
            other => {
                return Err(Error::at(line, 1, format!("unknown statement `{other}`")));
            }
        }
        let tags = self.tags()?;
        if !tags.is_empty() {
            items.push(Item::Tag(tags));
        }
        Ok(())
    }

    /// An optional `tag a, b`.
    fn tags(&mut self) -> Result<Vec<String>, Error> {
        if !matches!(self.peek(), Tok::Ident(w) if w == "tag") {
            return Ok(Vec::new());
        }
        self.bump();
        let mut tags = vec![self.ident("a tag")?];
        while self.eat(",") {
            tags.push(self.ident("a tag")?);
        }
        Ok(tags)
    }

    fn expr_list(&mut self) -> Result<Vec<Expr>, Error> {
        let mut out = vec![self.expr()?];
        while self.eat(",") {
            out.push(self.expr()?);
        }
        Ok(out)
    }

    fn optional_string(&mut self) -> Option<String> {
        match self.peek() {
            Tok::Str(_) => match self.bump() {
                Tok::Str(text) => Some(text),
                _ => None,
            },
            _ => None,
        }
    }

    /// `{ statements }`, as their own list.
    fn block(&mut self, name: &mut Option<String>, what: &str) -> Result<Vec<Item>, Error> {
        self.expect("{")?;
        let mut inner = Vec::new();
        while !self.eat("}") {
            if *self.peek() == Tok::Eof {
                return Err(self.error(format!("this `{what}` isn't closed")));
            }
            self.statement(name, &mut inner)?;
        }
        Ok(inner)
    }

    /// `name = value for a in lo..hi, b in lo..hi`.
    fn many(&mut self, kind: ManyKind, line: usize) -> Result<Item, Error> {
        let name = self.ident("a collection's name")?;
        self.expect("=")?;
        // A curve has its own parameter, which runs smoothly, before what counts the curves.
        let mut param = None;
        let value = if kind == ManyKind::Curves {
            self.keyword("parametric")?;
            self.expect("(")?;
            let var = self.ident("a parameter")?;
            self.expect(")")?;
            self.expect(":")?;
            let value = self.expr()?;
            self.keyword("for")?;
            self.keyword(&var)?;
            self.keyword("in")?;
            param = Some((var, self.range()?));
            value
        } else {
            self.expr()?
        };
        // A collection counts over its variables, and one that says `±` draws both signs, so
        // with one its `for` can be left out: it's a collection of two.
        let mut vars = Vec::new();
        let missing_for = self.error(format!("expected `for`, found {}", self.peek()));
        let counted = self.at_word("for");
        if counted {
            self.bump();
            loop {
                let var = self.ident("a variable")?;
                self.keyword("in")?;
                let from = self.pos;
                let over = if self.eat("{") {
                    if *self.peek() == Tok::Punct("}") {
                        return Err(self
                            .error(format!("`{var}` counts over nothing: a list needs a value")));
                    }
                    let mut values = Vec::new();
                    loop {
                        values.push(self.expr()?);
                        if *self.peek() == Tok::Punct("..") {
                            return Err(self.error(format!(
                                "a list's elements are values, written one by one: `{var} in {{1, 2, 3}}`, and a range goes outside the braces: `{var} in 1..3`"
                            )));
                        }
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect("}")?;
                    Over::List(values)
                } else {
                    Over::Range(self.range()?)
                };
                // The signs are the innermost of a collection's variables, so a list another
                // counts over can't depend on them. A range checks its own.
                if let (Over::List(_), Some(sign)) = (
                    &over,
                    self.toks[from..self.pos]
                        .iter()
                        .find(|t| matches!(t.tok, Tok::Punct("±" | "∓"))),
                ) {
                    return Err(Error::at(
                        sign.line,
                        sign.col,
                        format!(
                            "`{var}` can't count over `±`: the two signs are a collection's last variable, so write them as values, `{var} in {{1, -1}}`, or `±` in what it draws"
                        ),
                    ));
                }
                vars.push((var, over));
                if !self.eat(",") {
                    break;
                }
            }
        }
        let mut lets = Vec::new();
        if self.at_word("with") {
            self.bump();
            loop {
                let local = self.ident("a name")?;
                self.expect("=")?;
                lets.push((local, self.expr()?));
                if !self.eat(",") {
                    break;
                }
            }
        }
        if !counted && self.branches.is_none() {
            return Err(missing_for);
        }
        Ok(Item::Many {
            name,
            kind,
            value,
            param,
            vars,
            lets,
            line,
        })
    }

    /// `a..b`, or `med ± dev` as the whole range, `(med - abs(dev))..(med + abs(dev))`: the
    /// tolerance reading of `±`, as in x = 5 ± 2. It's sugar, so each place allows what it
    /// allows of a range's ends. A `±` anywhere else in a range would make two ranges, which a
    /// list says instead.
    fn range(&mut self) -> Result<[Expr; 2], Error> {
        let outer = self.branches.take();
        let from = self.pos;
        let lo = self.expr()?;
        let signs = |p: &Parser, from: usize| {
            p.toks[from..p.pos]
                .iter()
                .filter(|t| matches!(t.tok, Tok::Punct("±" | "∓")))
                .map(|t| (t.line, t.col, t.tok == Tok::Punct("±")))
                .collect::<Vec<_>>()
        };
        if *self.peek() != Tok::Punct("..") {
            let found = signs(self, from);
            match (&lo, found.as_slice()) {
                (Expr::App(head, parts), [_]) if head == PLUS_MINUS && parts.len() == 2 => {
                    self.branches = outer;
                    // The interval between the two values, whichever is larger, so a
                    // negative `dev` doesn't reverse it.
                    let med = parts[0].clone();
                    let dev = match &parts[1] {
                        Expr::Num(n) => Expr::Num(n.abs()),
                        dev => Expr::app("Abs", vec![dev.clone()]),
                    };
                    return Ok([
                        Expr::app("Subtract", vec![med.clone(), dev.clone()]),
                        Expr::app("Add", vec![med, dev]),
                    ]);
                }
                (Expr::App(head, parts), [(line, col, true)])
                    if head == PLUS_MINUS && parts.len() == 1 =>
                {
                    return Err(Error::at(
                        *line,
                        *col,
                        "`±x` alone is two values, +x and -x, not a range: for the two values write `{x, -x}`, and for the interval `-x..x` or `0 ± x`",
                    ));
                }
                (_, [(line, col, false), ..]) => {
                    return Err(Error::at(
                        *line,
                        *col,
                        "`∓` isn't a range: an interval about a middle is `med ± dev`, as `2 ± 1`, or `a..b`",
                    ));
                }
                (_, [(line, col, _), ..]) => {
                    return Err(Error::at(
                        *line,
                        *col,
                        "a range is `a..b`, or one `med ± dev` as the whole of it, as `2 ± 1`",
                    ));
                }
                _ => {}
            }
        }
        self.expect("..")?;
        let hi = self.expr()?;
        if let Some((line, col, _)) = signs(self, from).first() {
            return Err(Error::at(
                *line,
                *col,
                "`±` in an end of a range would make two ranges. For an interval about a middle, the whole range is `med ± dev`, as `2 ± 1`; for both signs, count over them first: `for s in {1, -1}, k in 0..(2 + s)`",
            ));
        }
        self.branches = outer;
        Ok([lo, hi])
    }

    fn function(&mut self, line: usize) -> Result<Item, Error> {
        let name = self.ident("a function name")?;
        self.expect("(")?;
        let mut params = vec![self.ident("a parameter")?];
        while self.eat(",") {
            params.push(self.ident("a parameter")?);
        }
        self.expect(")")?;
        self.expect("=")?;
        let body = self.expr()?;
        Ok(Item::Fn {
            name,
            params,
            body,
            line,
        })
    }

    /// A beat's `focus`: marks and `.tags`, each maybe only while a choice is on one option.
    fn focus_list(&mut self) -> Result<Vec<Focus>, Error> {
        let mut list = Vec::new();
        loop {
            // A tag is written `.tag`, as in a selector.
            let dotted = self.eat(".");
            let word = self.ident("a mark or `.tag`")?;
            let word = if dotted { format!(".{word}") } else { word };
            let when = if self.at_word("if") {
                self.bump();
                let choice = self.ident("a choice")?;
                self.expect("=")?;
                match self.bump() {
                    Tok::Str(option) => Some((choice, option)),
                    other => {
                        return Err(self.error(format!(
                            "a focus follows a choice by an option's name in quotes, found {other}"
                        )));
                    }
                }
            } else {
                None
            };
            list.push(Focus { word, when });
            if !self.eat(",") {
                return Ok(list);
            }
        }
    }

    /// Where token `i` starts in the source.
    fn offset(&self, i: usize) -> usize {
        let t = &self.toks[i];
        self.line_starts
            .get(t.line - self.base - 1)
            .map_or(self.src.len(), |s| s + t.col - 1)
    }

    /// The source from token `from` up to token `to`, as written.
    fn text(&self, from: usize, to: usize) -> String {
        let (a, b) = (self.offset(from), self.offset(to).min(self.src.len()));
        self.src[a..b.max(a)]
            .iter()
            .collect::<String>()
            .trim()
            .to_string()
    }

    fn param(&mut self, line: usize) -> Result<Item, Error> {
        let name = self.ident("a param")?;
        let start = self.pos + 1;
        // A choice's default can be one of its options' names.
        let (default, default_text) = if self.eat("=") {
            if let Tok::Str(named) = self.peek().clone() {
                self.bump();
                (None, Some(named))
            } else {
                let default = self.expr()?;
                (Some(default), Some(self.text(start, self.pos)))
            }
        } else {
            (None, None)
        };
        self.keyword("in")?;
        if self.eat("{") {
            let mut options = Vec::new();
            loop {
                // An option can have a name, `"q = 2p": 2·acos(…)`, which a page shows instead of
                // its value as written.
                let named = match (
                    self.peek().clone(),
                    self.toks.get(self.pos + 1).map(|t| &t.tok),
                ) {
                    (Tok::Str(name), Some(Tok::Punct(":"))) => {
                        self.bump();
                        self.bump();
                        Some(name)
                    }
                    _ => None,
                };
                let from = self.pos;
                let option = self.expr()?;
                if *self.peek() == Tok::Punct("..") {
                    return Err(self.error(format!(
                        "a choice's options are values, written one by one: `{name} in {{1, 2, 3}}`, and a range goes outside the braces: `{name} in 1..3`"
                    )));
                }
                let text = named.unwrap_or_else(|| self.text(from, self.pos));
                options.push((option, text));
                if self.eat("}") {
                    break;
                }
                self.expect(",")?;
            }
            return Ok(Item::Choice {
                name,
                default: default_text,
                options,
                line,
            });
        }
        let default = match (default, default_text) {
            (Some(default), _) => default,
            (None, Some(named)) => {
                return Err(self.error(format!(
                    "a param's default is a number; \"{named}\" names an option, which only a choice has"
                )));
            }
            (None, None) => Expr::Num(0.0),
        };
        let range = self.range()?;
        let step = if self.at_word("step") {
            self.bump();
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Item::Param {
            name,
            default,
            range,
            step,
            line,
        })
    }

    fn coordinates(&mut self) -> Result<[String; 2], Error> {
        self.expect("(")?;
        let a = self.ident("a coordinate")?;
        self.expect(",")?;
        let b = self.ident("a coordinate")?;
        self.expect(")")?;
        self.expect(":")?;
        Ok([a, b])
    }

    fn curve(&mut self, line: usize) -> Result<Item, Error> {
        let name = self.ident("a curve name")?;
        self.expect("=")?;
        let before = self.pos;
        let kind = self.ident("a curve form")?;
        let form = match kind.as_str() {
            "implicit" => {
                let vars = self.coordinates()?;
                let lhs = self.expr()?;
                self.expect("=")?;
                let rhs = self.expr()?;
                CurveSyntax::Implicit { vars, lhs, rhs }
            }
            "sdf" => {
                let vars = self.coordinates()?;
                CurveSyntax::Sdf {
                    vars,
                    field: self.expr()?,
                }
            }
            "parametric" => {
                self.expect("(")?;
                let var = self.ident("a parameter")?;
                self.expect(")")?;
                self.expect(":")?;
                let point = self.expr()?;
                self.keyword("for")?;
                self.keyword(&var)?;
                self.keyword("in")?;
                CurveSyntax::Parametric {
                    var,
                    point,
                    range: self.range()?,
                }
            }
            _ => {
                // Anything else is an expression: a construction's value.
                self.pos = before;
                CurveSyntax::Value(self.expr()?)
            }
        };
        Ok(Item::Curve { name, form, line })
    }

    fn region(&mut self, line: usize) -> Result<Item, Error> {
        let name = self.ident("a region name")?;
        self.expect("=")?;
        let mut parts = Vec::new();
        loop {
            let point = self.expr()?;
            if matches!(self.peek(), Tok::Ident(w) if w == "for") {
                self.bump();
                let var = self.ident("a parameter")?;
                self.keyword("in")?;
                parts.push(PartSyntax::Arc {
                    var,
                    point,
                    range: self.range()?,
                });
            } else {
                parts.push(PartSyntax::Point(point));
            }
            if !self.eat(",") {
                break;
            }
        }
        Ok(Item::Region { name, parts, line })
    }

    fn label(&mut self, line: usize) -> Result<Item, Error> {
        let name = self.ident("a label name")?;
        let text = if self.eat("=") {
            match self.bump() {
                Tok::Str(text) => Some(text),
                other => {
                    return Err(Error::at(line, 1, format!("expected text, found {other}")));
                }
            }
        } else {
            None
        };
        self.keyword("at")?;
        let at = self.expr()?;
        let (mut offset, mut away) = (None, None);
        if matches!(self.peek(), Tok::Ident(w) if w == "offset") {
            self.bump();
            offset = Some(self.expr()?);
        } else if matches!(self.peek(), Tok::Ident(w) if w == "away") {
            self.bump();
            away = Some(self.expr()?);
        }
        if matches!(self.peek(), Tok::Ident(w) if w == "offset" || w == "away") {
            return Err(self.error("a label takes `offset` or `away`, not both"));
        }
        Ok(Item::Label {
            name,
            text,
            at,
            offset,
            away,
            line,
        })
    }

    fn viewport(&mut self) -> Result<Item, Error> {
        if matches!(self.peek(), Tok::Ident(_)) {
            self.bump();
        }
        self.expect("{")?;
        let (mut x, mut y, mut pad, mut yaw) = (None, None, None, None);
        let mut fit = Vec::new();
        while !self.eat("}") {
            let prop = self.ident("a viewport property")?;
            self.expect(":")?;
            match prop.as_str() {
                "x" => x = Some(self.range()?),
                "y" => y = Some(self.range()?),
                // How big the figure is drawn depends on where, so it's a sheet's.
                "width" | "height" => {
                    return Err(self.error(format!(
                        "a figure's size is a sheet's, as `style figure {{ {prop}: 640px }}`"
                    )));
                }
                "pad" => pad = Some(self.expr()?),
                "yaw" => yaw = Some(self.expr()?),
                "fit" => {
                    while matches!(self.peek(), Tok::Ident(_)) {
                        fit.push(self.ident("a mark")?);
                    }
                }
                other => return Err(self.error(format!("unknown viewport property `{other}`"))),
            }
            if !self.eat(",") {
                self.eat(";");
            }
        }
        if fit.is_empty() && (x.is_none() || y.is_none()) {
            return Err(self.error("a viewport needs both `x` and `y`, or `fit`"));
        }
        Ok(Item::Viewport {
            x,
            y,
            fit,
            pad,
            yaw,
        })
    }

    fn style(&mut self, line: usize) -> Result<StyleRule, Error> {
        let mut selectors = Vec::new();
        loop {
            let target = if matches!(self.peek(), Tok::Ident(_)) {
                self.ident("a selector")?
            } else {
                String::new()
            };
            let mut tags = Vec::new();
            while self.eat(".") {
                tags.push(self.ident("a tag")?);
            }
            if target.is_empty() && tags.is_empty() {
                return Err(self.error("expected a selector"));
            }
            let facet = if self.eat("::") {
                Some(self.ident("a facet")?)
            } else {
                None
            };
            selectors.push(Selector {
                target,
                tags,
                facet,
            });
            if !self.eat(",") {
                break;
            }
        }
        self.expect("{")?;
        let mut decls = Vec::new();
        while !self.eat("}") {
            let prop = self.ident("a property")?;
            self.expect(":")?;
            // What a label's text is made of, written as it reads: `text: expr = value`.
            if prop == "text" {
                let mut parts = vec![Expr::Sym(self.ident("a part of a label's text")?)];
                while self.eat("=") {
                    parts.push(Expr::Sym(self.ident("a part of a label's text")?));
                }
                decls.push((prop, Expr::app(TEXT_PARTS, parts)));
                self.eat(";");
                continue;
            }
            decls.push((prop, self.expr()?));
            self.eat(";");
        }
        Ok(StyleRule {
            selectors,
            decls,
            line,
        })
    }

    fn expr(&mut self) -> Result<Expr, Error> {
        let mut left = self.term()?;
        loop {
            self.note_branches();
            let head = if self.eat("+") {
                "Add"
            } else if self.eat("-") {
                "Subtract"
            } else if self.eat("±") {
                PLUS_MINUS
            } else if self.eat("∓") {
                MINUS_PLUS
            } else {
                return Ok(left);
            };
            left = Expr::app(head, vec![left, self.term()?]);
        }
    }

    /// Products and quotients, left to right. Factors with a space between them multiply here,
    /// as `a b` and `1/2 π` do, so the space ends a denominator: `1/2 π` is π/2.
    fn term(&mut self) -> Result<Expr, Error> {
        let mut left = self.product()?;
        loop {
            let head = if self.eat("*") {
                "Multiply"
            } else if self.eat("/") {
                "Divide"
            } else if self.side_by_side() {
                "Multiply"
            } else {
                return Ok(left);
            };
            left = Expr::app(head, vec![left, self.product()?]);
        }
    }

    /// Factors written right against each other multiply, as `2π`, `2(x + 1)` and
    /// `(a + b)(a - b)`. They bind tighter than `/`, so `1/2π` is 1/(2π), as a reader takes it.
    fn product(&mut self) -> Result<Expr, Error> {
        let mut left = self.unary()?;
        while self.side_by_side() && !self.toks[self.pos].spaced {
            left = Expr::app("Multiply", vec![left, self.power()?]);
        }
        Ok(left)
    }

    /// Whether the next token starts a factor that multiplies the one before it: on the same
    /// line, and not one of the words that can follow an expression, such as `for` or `tag`.
    /// A name right before `(` was already read as a call.
    fn side_by_side(&self) -> bool {
        let (before, next) = (&self.toks[self.pos - 1], &self.toks[self.pos]);
        next.line == before.line
            && match &next.tok {
                Tok::Num(_) => true,
                Tok::Ident(w) => !AFTER_EXPRESSIONS.contains(&w.as_str()),
                Tok::Punct(p) => *p == "(",
                _ => false,
            }
    }

    /// Notes where a statement first says `±` or `∓`, if the next token is one.
    fn note_branches(&mut self) {
        if self.branches.is_none() && matches!(self.peek(), Tok::Punct("±" | "∓")) {
            let t = &self.toks[self.pos];
            self.branches = Some((t.line, t.col));
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        self.note_branches();
        for (sign, head) in [("±", PLUS_MINUS), ("∓", MINUS_PLUS)] {
            if self.eat(sign) {
                return Ok(Expr::app(head, vec![self.unary()?]));
            }
        }
        if self.eat("-") {
            // A negated literal stays one number, so `-2..2` reads as data.
            return Ok(match self.unary()? {
                Expr::Num(n) => Expr::Num(-n),
                other => Expr::app("Negate", vec![other]),
            });
        }
        self.power()
    }

    fn power(&mut self) -> Result<Expr, Error> {
        let base = self.primary()?;
        if let Tok::Sup(n) = *self.peek() {
            self.bump();
            return Ok(Expr::app("Power", vec![base, Expr::Num(n)]));
        }
        if self.eat("^") {
            return Ok(Expr::app("Power", vec![base, self.unary()?]));
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.bump();
                if *self.peek() == Tok::Px {
                    self.bump();
                    return Ok(Expr::app("Pixels", vec![Expr::Num(n)]));
                }
                if *self.peek() == Tok::Em {
                    self.bump();
                    return Ok(Expr::app("Ems", vec![Expr::Num(n)]));
                }
                Ok(Expr::Num(n))
            }
            Tok::Ident(_) => {
                let (line, col) = (self.toks[self.pos].line, self.toks[self.pos].col);
                let mut name = self.ident("a name")?;
                if self.eat(".") {
                    name = format!("{name}.{}", self.ident("a property")?);
                }
                if !self.eat("(") {
                    return Ok(Expr::Sym(name));
                }
                let mut args = Vec::new();
                if !self.eat(")") {
                    loop {
                        args.push(self.expr()?);
                        if self.eat(")") {
                            break;
                        }
                        self.expect(",")?;
                    }
                }
                let _ = (line, col);
                let head = FUNCTIONS
                    .iter()
                    .find(|(f, _)| subscripted(f) == name)
                    .map_or(name.as_str(), |(_, head)| head);
                Ok(Expr::app(head, args))
            }
            _ if self.eat("(") => {
                let inner = self.expr()?;
                if self.eat(",") {
                    let mut parts = vec![inner, self.expr()?];
                    // A third part is a point in space.
                    if self.eat(",") {
                        parts.push(self.expr()?);
                    }
                    self.expect(")")?;
                    return Ok(Expr::app("Tuple", parts));
                }
                self.expect(")")?;
                Ok(inner)
            }
            other => Err(self.error(format!("expected an expression, found {other}"))),
        }
    }
}

/// One expression, as written inside a label's text.
impl Parser {
    fn new(src: &str) -> Result<Parser, Error> {
        Self::numbered_from(src, 0)
    }

    /// A parser whose lines count on from `base`, so each file of a figure has lines of its own.
    fn numbered_from(src: &str, base: usize) -> Result<Parser, Error> {
        let src: Vec<char> = src.chars().collect();
        let mut line_starts = vec![0];
        line_starts.extend(
            src.iter()
                .enumerate()
                .filter(|(_, c)| **c == '\n')
                .map(|(i, _)| i + 1),
        );
        let mut toks = lex(&src.iter().collect::<String>()).map_err(|mut e| {
            e.line += base;
            e
        })?;
        for t in &mut toks {
            t.line += base;
        }
        Ok(Parser {
            toks,
            pos: 0,
            src,
            line_starts,
            base,
            branches: None,
        })
    }
}

pub fn parse_expression(src: &str) -> Result<Expr, Error> {
    Parser::new(src)?.expr()
}

/// An expression that is the whole of `src`, with nothing after it.
pub fn parse_whole_expression(src: &str) -> Result<Expr, Error> {
    let mut parser = Parser::new(src)?;
    let e = parser.expr()?;
    if *parser.peek() != Tok::Eof {
        return Err(parser.error(format!("expected the end, found {}", parser.peek())));
    }
    Ok(e)
}

/// A token as the lexer read it, for tools that show how text is read: what kind it is, and
/// its text after the lexer's own rewriting, so `·` reads `*` and `²` reads `^2`.
pub struct Token {
    pub kind: &'static str,
    pub text: String,
    pub line: usize,
    pub col: usize,
}

pub fn tokens(src: &str) -> Result<Vec<Token>, Error> {
    Ok(lex(src)?
        .into_iter()
        .filter(|t| t.tok != Tok::Eof)
        .map(|t| {
            let (kind, text) = match t.tok {
                Tok::Ident(name) => ("name", name),
                Tok::Num(n) => ("number", n.to_string()),
                Tok::Px => ("unit", "px".to_string()),
                Tok::Em => ("unit", "em".to_string()),
                Tok::Punct(p) => ("punct", p.to_string()),
                Tok::Sup(n) => ("power", format!("^{n}")),
                Tok::Str(text) => ("text", format!("{text:?}")),
                Tok::Eof => unreachable!(),
            };
            Token {
                kind,
                text,
                line: t.line,
                col: t.col,
            }
        })
        .collect())
}

pub fn parse(src: &str) -> Result<Document, Error> {
    Parser::new(src)?.document()
}

/// A document whose lines are numbered on from `base`, as one file of several. See `files`.
pub fn parse_numbered_from(src: &str, base: usize) -> Result<Document, Error> {
    Parser::numbered_from(src, base)?.document()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(src: &str) -> Expr {
        let doc = parse(&format!("let v = {src}")).unwrap();
        match &doc.items[0] {
            Item::Let { expr, .. } => expr.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(
            expr("1 + 2 * x").to_mathjson().to_string(),
            r#"["Add",1.0,["Multiply",2.0,"x"]]"#
        );
        assert_eq!(
            expr("-x^2").to_mathjson().to_string(),
            r#"["Negate",["Power","x",2.0]]"#
        );
        assert_eq!(
            expr("2^3^x").to_mathjson().to_string(),
            r#"["Power",2.0,["Power",3.0,"x"]]"#
        );
    }

    #[test]
    fn notation_matches_ascii() {
        assert_eq!(expr("x²"), expr("x^2"));
        assert_eq!(expr("2·x"), expr("2*x"));
    }

    #[test]
    fn numbers_stop_before_a_range() {
        let doc = parse("viewport { x: -2..2, y: -1.5..1.5 }").unwrap();
        assert_eq!(
            doc.items[0],
            Item::Viewport {
                x: Some([Expr::Num(-2.0), Expr::Num(2.0)]),
                y: Some([Expr::Num(-1.5), Expr::Num(1.5)]),
                fit: Vec::new(),
                pad: None,
                yaw: None,
            }
        );
    }

    #[test]
    fn a_viewport_says_what_it_shows_and_not_how_big() {
        for prop in ["width", "height"] {
            let err =
                parse(&format!("viewport {{ x: -2..2, y: -2..2, {prop}: 640 }}")).unwrap_err();
            assert!(err.message.contains("`style figure"), "{err}");
        }
    }

    #[test]
    fn factors_side_by_side_multiply() {
        let same = |a: &str, b: &str| assert_eq!(expr(a), expr(b), "{a} and {b}");
        same("2π", "2·π");
        same("2 π", "2·π");
        same("2(x + 1)", "2·(x + 1)");
        same("(a + b)(a - b)", "(a + b)·(a - b)");
        same("a b", "a·b");
        same("2x²", "2·x²");
        same("-2π", "-2·π");
        same("2πi", "2·π·i");
        same("2πr", "2·π·r");
        same("iπ", "i·π");
        assert_eq!(expr("π_1"), Expr::sym("π_1"));
        assert_eq!(expr("π₁"), Expr::sym("π_1"));
        // Tighter than `/`, as a reader takes `1/2π`.
        same("1/2π", "1/(2·π)");
        // A space ends the denominator, so what follows multiplies the quotient.
        same("1/2 π", "(1/2)·π");
        same("(k - 1)/N π - π", "((k - 1)/N)·π - π");
        same("x/2π r", "(x/(2·π))·r");
        same("6/2(1 + 2)", "6/(2·(1 + 2))");
        same("6/2 (1 + 2)", "(6/2)·(1 + 2)");
        same("a b c", "a·b·c");
        same("a b^2 c", "a·b^2·c");
        // A comment or the end of a line isn't a space between factors.
        same("1/2 // half\n", "1/2");
        // A name before `(` is a call, and two letters together are one name.
        assert_eq!(expr("f(x)"), Expr::app("f", vec![Expr::sym("x")]));
        assert_eq!(expr("ab"), Expr::sym("ab"));
        assert_eq!(expr("θπ"), expr("θ·π"));
        // The words that follow an expression end it, and so does the end of a line.
        let doc = parse("param a in 0..2π step 1\nlet b = 2a\ndot P").unwrap();
        assert_eq!(doc.items.len(), 3);
    }

    #[test]
    fn a_subscript_is_one_name_however_it_is_written() {
        for written in ["x1", "x_1", "x₁", "x_{1}"] {
            assert_eq!(expr(written), Expr::sym("x_1"), "{written}");
        }
        assert_eq!(expr("σ₁₂"), Expr::sym("σ_12"));
        assert_eq!(expr("x_{max}"), Expr::sym("x_max"));
        // A name has one subscript, so digits at the end of one aren't another.
        assert_eq!(expr("f_{X175}"), Expr::sym("f_X175"));
        assert_eq!(expr("f_X175"), Expr::sym("f_X175"));
        // Digits inside a name aren't a subscript, and a built-in keeps its own.
        assert_eq!(expr("p2q"), Expr::sym("p2q"));
        assert_eq!(
            expr("atan2(1, 2)"),
            Expr::app("Arctan2", vec![Expr::Num(1.0), Expr::Num(2.0)])
        );
        let err = parse("let v = x_{1 + 2}").unwrap_err();
        assert!(err.message.contains("subscript in braces"), "{err}");
    }

    #[test]
    fn a_handle_sets_one_param_or_two() {
        let doc = parse("handle z sets a = pointer.x,\n  b = pointer.y").unwrap();
        let Item::Handle { mark, sets, line } = &doc.items[0] else {
            panic!("{:?}", doc.items[0])
        };
        assert_eq!((mark.as_str(), *line), ("z", 1));
        let names: Vec<_> = sets
            .iter()
            .map(|s| (s.param.as_str(), s.line, s.col))
            .collect();
        assert_eq!(names, [("a", 1, 15), ("b", 2, 3)]);
        assert_eq!(sets[1].inverse, Expr::sym("pointer.y"));
        let doc = parse("handle P sets a = pointer.x").unwrap();
        assert!(matches!(&doc.items[0], Item::Handle { sets, .. } if sets.len() == 1));
    }

    #[test]
    fn a_handle_says_each_param_once_and_at_most_two() {
        let err = parse("handle z sets a = pointer.x, a = pointer.y").unwrap_err();
        assert_eq!((err.line, err.col), (1, 30));
        assert_eq!(err.message, "`handle z` sets `a` twice");
        let err = parse("handle z sets a = pointer.x, b = pointer.y, c = 0").unwrap_err();
        assert_eq!((err.line, err.col), (1, 43));
        assert!(err.message.contains("one param or two, not more"), "{err}");
        let err = parse("handle z sets a = pointer.x, = 1").unwrap_err();
        assert!(err.message.contains("expected a param"), "{err}");
    }

    #[test]
    fn signs_parse_as_one_and_two_argument_heads() {
        assert_eq!(
            parse_whole_expression("±cosh(t)")
                .unwrap()
                .to_mathjson()
                .to_string(),
            r#"["PlusMinus",["Cosh","t"]]"#
        );
        assert_eq!(
            parse_whole_expression("a ∓ b·c")
                .unwrap()
                .to_mathjson()
                .to_string(),
            r#"["MinusPlus","a",["Multiply","b","c"]]"#
        );
        // As `+` and `-`: left to right, below products.
        assert_eq!(
            parse_whole_expression("1 ± 2 - 3")
                .unwrap()
                .to_mathjson()
                .to_string(),
            r#"["Subtract",["PlusMinus",1.0,2.0],3.0]"#
        );
    }

    #[test]
    fn errors_carry_a_position() {
        let err = parse("let r = 1\nlet s = )").unwrap_err();
        assert_eq!((err.line, err.col), (2, 9));
        let err = parse("label a = 3 at b").unwrap_err();
        assert!(err.message.contains("expected text"), "{err}");
    }

    #[test]
    fn tuples_and_user_functions_parse() {
        assert_eq!(
            expr("f((1, x))").to_mathjson().to_string(),
            r#"["f",["Tuple",1.0,"x"]]"#
        );
    }

    #[test]
    fn marks_parse() {
        let doc = parse(
            "param a = 1 in 0..2·π
             bind a = time
             fn f(t) = (cos(t), sin(t))
             curve p = parametric (t): f(t) for t in 0..a
             curve s = sdf (x, y): x
             segment g = (0, 0), f(a)
             rightangle m = (0, 0), f(a), (1, 1) size 0.2
             region r = (0, 0), f(t) for t in a..2·π
             dot O, P
             label n = \"P\" at f(a) offset (1, 2)",
        )
        .unwrap();
        assert_eq!(doc.items.len(), 10);
    }

    #[test]
    fn beats_tags_and_handles_parse() {
        let doc = parse(
            "viewport { fit: a b, pad: 0.1 }
             beat \"Start\" tag blue {
               let O = (0, 0)
               curve c = circle(O, (1, 0)) tag aux, thin
             }
             handle P sets a = pointer.x
             style circle.blue::inside, .aux, c { fill: white }",
        )
        .unwrap();
        assert!(matches!(&doc.items[1], Item::Beat { tags, .. } if tags == &["blue"]));
        assert!(matches!(&doc.items[4], Item::Tag(t) if t.len() == 2));
        assert!(matches!(doc.items[5], Item::EndBeat));
        assert!(matches!(&doc.items[6], Item::Handle { mark, .. } if mark == "P"));
        let Item::Style(rule) = &doc.items[7] else {
            panic!()
        };
        assert_eq!(rule.selectors[0].target, "circle");
        assert_eq!(rule.selectors[0].tags, ["blue"]);
        assert_eq!(rule.selectors[0].facet.as_deref(), Some("inside"));
        assert_eq!(rule.selectors[1].target, "");
        assert_eq!(rule.selectors[1].tags, ["aux"]);
    }

    #[test]
    fn style_rules_keep_facets_and_properties() {
        let doc = parse("style a::inside, b { fill: white; width: 3px }").unwrap();
        let Item::Style(rule) = &doc.items[0] else {
            panic!()
        };
        assert_eq!(rule.selectors[0].facet.as_deref(), Some("inside"));
        assert_eq!(rule.selectors[1].facet, None);
        assert_eq!(rule.decls[0].0, "fill");
        assert_eq!(rule.decls[1].1, Expr::app("Pixels", vec![Expr::Num(3.0)]));
    }
}
