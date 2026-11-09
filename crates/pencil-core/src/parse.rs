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
    pub focus: Vec<String>,
    pub tags: Vec<String>,
    /// Whether it has a block, closed by an `EndBeat`.
    pub block: bool,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Let {
        name: String,
        expr: Expr,
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
        line: usize,
    },
    Bind {
        name: String,
        expr: Expr,
        line: usize,
    },
    Duration(Expr),
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
        vars: Vec<(String, [Expr; 2])>,
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
    },
    EndBeat,
    /// A beat with a length: one move of a figure's story. Its marks are the block's, if it has
    /// one, and `show`, `drop` and `focus` name marks declared anywhere, by name or `.tag`.
    Move(Move),
    /// A hold between moves, in seconds.
    Pause(Expr),
    /// Tags the marks the statement before it made.
    Tag(Vec<String>),
    /// Dragging a mark sets a param. The expression is the explicit inverse from the
    /// pointer to the param, and the param's range is the constraint.
    Handle {
        mark: String,
        param: String,
        inverse: Expr,
        line: usize,
    },
    Viewport {
        x: Option<[Expr; 2]>,
        y: Option<[Expr; 2]>,
        fit: Vec<String>,
        pad: Option<Expr>,
    },
    Style(StyleRule),
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

struct Lexed {
    tok: Tok,
    line: usize,
    col: usize,
}

const PUNCT: [&str; 16] = [
    "::", "..", "+", "-", "*", "/", "^", "(", ")", "{", "}", ",", ":", "=", ";", ".",
];

fn lex(src: &str) -> Result<Vec<Lexed>, Error> {
    let chars: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut col) = (0, 1, 1);
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        let (start_line, start_col) = (line, col);
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
                out.push(Lexed { tok, line, col });
                advance(2, &mut i, &mut col);
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                // Superscripts count as alphanumeric, but they are exponents here.
                if matches!(chars[i], '²' | '³') {
                    break;
                }
                advance(1, &mut i, &mut col);
            }
            let name: String = chars[start..i].iter().collect();
            out.push(Lexed {
                tok: Tok::Ident(name),
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
                tok: Tok::Str(text),
                line: start_line,
                col: start_col,
            });
        } else if c == '²' || c == '³' {
            let n = if c == '²' { 2.0 } else { 3.0 };
            advance(1, &mut i, &mut col);
            out.push(Lexed {
                tok: Tok::Sup(n),
                line: start_line,
                col: start_col,
            });
        } else if c == '·' {
            advance(1, &mut i, &mut col);
            out.push(Lexed {
                tok: Tok::Punct("*"),
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
                tok: Tok::Punct(p),
                line: start_line,
                col: start_col,
            });
        }
    }
    out.push(Lexed {
        tok: Tok::Eof,
        line,
        col,
    });
    Ok(out)
}

struct Parser {
    toks: Vec<Lexed>,
    pos: usize,
}

/// Builtin names in source, and their MathJSON heads. Anything else is left under
/// its own name for `compile` to resolve as a user function. `docs/language.md` must
/// describe every one, and a test holds it to that.
pub const FUNCTIONS: [(&str, &str); 47] = [
    ("sqrt", "Sqrt"),
    ("sin", "Sin"),
    ("cos", "Cos"),
    ("tan", "Tan"),
    ("acos", "Arccos"),
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
    ("length", "Length"),
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
    ("imag", "Imag"),
    ("conj", "Conj"),
    ("arg", "Arg"),
    ("sinh", "Sinh"),
    ("cosh", "Cosh"),
    ("tanh", "Tanh"),
    ("coth", "Coth"),
    ("sech", "Sech"),
    ("csch", "Csch"),
    ("arctanh", "Arctanh"),
    ("flip", "Flip"),
];

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
            Err(self.error(format!("expected `{p}`, found {:?}", self.peek())))
        }
    }

    fn ident(&mut self, what: &str) -> Result<String, Error> {
        match self.peek() {
            Tok::Ident(_) => match self.bump() {
                Tok::Ident(name) => Ok(name),
                _ => unreachable!(),
            },
            other => Err(self.error(format!("expected {what}, found {other:?}"))),
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

    fn statement(&mut self, name: &mut Option<String>, items: &mut Vec<Item>) -> Result<(), Error> {
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
                let expr = self.expr()?;
                items.push(Item::Let { name, expr, line });
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
                        return Err(Error::at(
                            line,
                            1,
                            format!("expected text, found {other:?}"),
                        ));
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
            "use" => {
                let group = self.ident("a group")?;
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
                                    "expected a written expression in quotes, found {other:?}"
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
                let param = self.ident("a param")?;
                self.expect("=")?;
                let inverse = self.expr()?;
                items.push(Item::Handle {
                    mark,
                    param,
                    inverse,
                    line,
                });
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
                    let list = match self.peek() {
                        Tok::Ident(w) if w == "show" => &mut show,
                        Tok::Ident(w) if w == "drop" => &mut drop,
                        Tok::Ident(w) if w == "focus" => &mut focus,
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
                            format!("expected a caption, found {other:?}"),
                        ));
                    }
                };
                let tags = self.tags()?;
                self.expect("{")?;
                items.push(Item::Beat { caption, tags });
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
        self.keyword("for")?;
        let mut vars = Vec::new();
        loop {
            let var = self.ident("a variable")?;
            self.keyword("in")?;
            vars.push((var, self.range()?));
            if !self.eat(",") {
                break;
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

    fn range(&mut self) -> Result<[Expr; 2], Error> {
        let lo = self.expr()?;
        self.expect("..")?;
        Ok([lo, self.expr()?])
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

    fn param(&mut self, line: usize) -> Result<Item, Error> {
        let name = self.ident("a param")?;
        let default = if self.eat("=") {
            self.expr()?
        } else {
            Expr::Num(0.0)
        };
        self.keyword("in")?;
        let range = self.range()?;
        Ok(Item::Param {
            name,
            default,
            range,
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
                    return Err(Error::at(
                        line,
                        1,
                        format!("expected text, found {other:?}"),
                    ));
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
        let (mut x, mut y, mut pad) = (None, None, None);
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
        Ok(Item::Viewport { x, y, fit, pad })
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
            let head = if self.eat("+") {
                "Add"
            } else if self.eat("-") {
                "Subtract"
            } else {
                return Ok(left);
            };
            left = Expr::app(head, vec![left, self.term()?]);
        }
    }

    fn term(&mut self) -> Result<Expr, Error> {
        let mut left = self.unary()?;
        loop {
            let head = if self.eat("*") {
                "Multiply"
            } else if self.eat("/") {
                "Divide"
            } else {
                return Ok(left);
            };
            left = Expr::app(head, vec![left, self.unary()?]);
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
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
                    .find(|(f, _)| *f == name)
                    .map_or(name.as_str(), |(_, head)| head);
                Ok(Expr::app(head, args))
            }
            _ if self.eat("(") => {
                let inner = self.expr()?;
                if self.eat(",") {
                    let second = self.expr()?;
                    self.expect(")")?;
                    return Ok(Expr::app("Tuple", vec![inner, second]));
                }
                self.expect(")")?;
                Ok(inner)
            }
            other => Err(self.error(format!("expected an expression, found {other:?}"))),
        }
    }
}

/// One expression, as written inside a label's text.
pub fn parse_expression(src: &str) -> Result<Expr, Error> {
    Parser {
        toks: lex(src)?,
        pos: 0,
    }
    .expr()
}

pub fn parse(src: &str) -> Result<Document, Error> {
    Parser {
        toks: lex(src)?,
        pos: 0,
    }
    .document()
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
            "param a = 1 in 0..tau
             bind a = time
             fn f(t) = (cos(t), sin(t))
             curve p = parametric (t): f(t) for t in 0..a
             curve s = sdf (x, y): x
             segment g = (0, 0), f(a)
             rightangle m = (0, 0), f(a), (1, 1) size 0.2
             region r = (0, 0), f(t) for t in a..tau
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
