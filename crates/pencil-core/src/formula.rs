//! A label's mathematics, laid out: superscripts, subscripts, fractions and roots, with
//! variables in italics and digits, operators and function names upright.
//!
//! A label marks its mathematics with `$…$`, and what's inside is Pencil's own notation, the
//! one a `let` is written in, so `x^2/2` is what it would be there. Grouping is by parentheses,
//! so `{expr}` stays a value anywhere in the text. An expression's terms are mathematics
//! without being marked.
//!
//! What comes out is glyphs and rules at places, which the command burns in and a page sets as
//! positioned text, so both draw one layout. The places come from the metrics `build.rs` reads
//! out of the package's math font, the file both draw with: its advances, its ink, and its own
//! constants for where a script sits and how thick a fraction's rule is. Words outside a span are
//! the renderer's to measure, as every plain label is.
//!
//! It isn't TeX. There are no matrices, no aligned lines, no limits set above a sum, and no
//! accents. A tall bracket or root is the font's glyph stretched, not a larger one drawn for the
//! size.

use crate::parse::FUNCTIONS;
use crate::style::Font;
use crate::terms::TextWidths;

mod metrics {
    include!(concat!(env!("OUT_DIR"), "/math_metrics.rs"));
}

use metrics as m;

/// How far below a label's center its baseline is, in ems: the fonts' line box is centered on
/// the label's point, as a page centers a line of text.
pub const BASELINE: f64 = (m::ASCENDER + m::DESCENDER) / 2.0 / m::UNITS_PER_EM;

/// Which font a run is set in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    /// The label's own, by its style's `font`.
    Text,
    /// The math font.
    Math,
}

/// Glyphs on one baseline, in device pixels from the label's center, with y down.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    /// Its left edge.
    pub x: f64,
    /// Its baseline.
    pub y: f64,
    /// Its font size.
    pub size: f64,
    pub face: Face,
    /// How much taller than the font's own the glyphs are drawn, about the baseline: a root or a
    /// bracket around something tall.
    pub stretch: f64,
}

/// A fraction's bar or a root's, in device pixels from the label's center, with y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A label's text, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub width: f64,
    /// How far its ink reaches above and below the label's center.
    pub above: f64,
    pub below: f64,
    pub runs: Vec<Run>,
    pub rules: Vec<Rule>,
}

/// Whether a label's text has mathematics marked in it.
pub fn has_math(text: &str) -> bool {
    text.contains('$')
}

/// What a label's text must be for `lay` to read it: every `$` that opens a span closed.
pub fn check(text: &str) -> Result<(), String> {
    match text.replace("$$", "").matches('$').count() % 2 {
        0 => Ok(()),
        _ => Err("a `$` opens mathematics that no `$` closes; a dollar sign is `$$`".to_string()),
    }
}

/// A label's text with its `$…$` spans laid out, or None for text with no mathematics, which a
/// renderer sets as it always has.
pub fn lay(text: &str, size: f64, font: Font, widths: &dyn TextWidths) -> Option<Laid> {
    has_math(text).then(|| assemble(&segments(text), size, font, widths))
}

/// Text that is mathematics throughout, as an expression's term is.
pub fn lay_math(text: &str, size: f64) -> Laid {
    let parts = [Segment::Math(text.to_string())];
    assemble(&parts, size, Font::Math, &crate::terms::Estimate)
}

/// A label's width, whichever way it's set.
pub fn width(text: &str, size: f64, font: Font, widths: &dyn TextWidths) -> f64 {
    match lay(text, size, font, widths) {
        Some(laid) => laid.width,
        None => widths.width(text, size, font),
    }
}

/// A label's text as it's said: its words as they are, and its mathematics as words a screen
/// reader reads as mathematics, from the same reading `lay` sets it from. `z^2 = 1` is "z squared
/// = 1", `x_1` is "x sub 1", `(a + b)/2` is "(a + b) over 2" and `sqrt(x)` is "the square root of
/// x". Letters are the letters, not their italics.
pub fn spoken(text: &str) -> String {
    segments(text)
        .iter()
        .map(|segment| match segment {
            Segment::Text(words) => words.clone(),
            Segment::Math(source) => say_row(&parse(source)),
        })
        .collect()
}

/// A letter's italic, as `italic` makes it, back to the letter.
fn upright(c: char) -> char {
    let back = |first: u32, to: char| char::from_u32(to as u32 + (c as u32 - first)).unwrap_or(c);
    match c as u32 {
        0x210e => 'h',
        0x1d44e..=0x1d467 => back(0x1d44e, 'a'),
        0x1d434..=0x1d44d => back(0x1d434, 'A'),
        0x1d6fc..=0x1d714 => back(0x1d6fc, 'α'),
        _ => match c {
            '𝜖' => 'ϵ',
            '𝜗' => 'ϑ',
            '𝜘' => 'ϰ',
            '𝜙' => 'ϕ',
            '𝜚' => 'ϱ',
            '𝜛' => 'ϖ',
            _ => c,
        },
    }
}

/// A row of mathematics as it's said: what's spaced as an operator or a relation is said with
/// a space either side, and what's side by side stays together, as `2ab`.
fn say_row(nodes: &[Node]) -> String {
    let mut out = String::new();
    // A sign with nothing before it to work on, as the minus of `= −2`, is the number's own.
    let mut after: Option<Class> = None;
    for node in nodes {
        let said = say_node(node);
        let class = match node {
            Node::Atom { class, .. } => Some(*class),
            _ => None,
        };
        let unary = class == Some(Class::Bin)
            && matches!(
                after,
                None | Some(Class::Bin | Class::Rel | Class::Open | Class::Punct)
            );
        after = class.or(Some(Class::Ord));
        let spaced = !unary && matches!(class, Some(Class::Bin | Class::Rel));
        if spaced {
            out = out.trim_end().to_string();
            out.push(' ');
            out.push_str(&said);
            out.push(' ');
        } else if matches!(
            node,
            Node::Atom {
                class: Class::Punct,
                ..
            }
        ) {
            out.push_str(&said);
            out.push(' ');
        } else {
            out.push_str(&said);
        }
    }
    out.trim().to_string()
}

/// One thing in a row as it's said. What's more than one thing is bracketed, so it's heard as one.
fn say_group(nodes: &[Node]) -> String {
    let said = say_row(nodes);
    match nodes {
        [Node::Atom { .. }] | [Node::Fence { .. }] => said,
        _ => format!("({said})"),
    }
}

fn say_node(node: &Node) -> String {
    match node {
        Node::Atom { text, class } => {
            let text: String = text.chars().map(upright).collect();
            match class {
                // A function's name is said apart from what it's of.
                Class::Op => format!("{text} "),
                _ => text,
            }
        }
        Node::Scripts { base, sup, sub } => {
            let mut said = say_node(base).trim_end().to_string();
            if let Some(sub) = sub {
                said.push_str(&format!(" sub {}", say_group(sub)));
            }
            if let Some(sup) = sup {
                let power = say_row(sup);
                said.push_str(&match power.as_str() {
                    "2" => " squared".to_string(),
                    "3" => " cubed".to_string(),
                    _ => format!(" to the power {}", say_group(sup)),
                });
            }
            said
        }
        Node::Frac { num, den, .. } => format!("{} over {}", say_group(num), say_group(den)),
        Node::Root(body) => format!("the square root of {}", say_group(body)),
        Node::Fence { open, body, close } => {
            let inner = say_row(body);
            match (open, close) {
                ('|', _) => format!("the absolute value of {}", say_group(body)),
                _ => format!(
                    "{open}{inner}{}",
                    close.map(String::from).unwrap_or_default()
                ),
            }
        }
    }
}

/// What an editor writes for a name typed after a backslash, as `\to` is `→`: the names LaTeX
/// gives the symbols a keyboard lacks. Pencil's text is the symbol itself, and never a backslash,
/// so this is for typing, and nothing reads it back.
const SYMBOLS: [(&str, &str); 73] = [
    ("to", "→"),
    ("rightarrow", "→"),
    ("leftarrow", "←"),
    ("gets", "←"),
    ("leftrightarrow", "↔"),
    ("Rightarrow", "⇒"),
    ("implies", "⇒"),
    ("Leftarrow", "⇐"),
    ("Leftrightarrow", "⇔"),
    ("iff", "⇔"),
    ("mapsto", "↦"),
    ("le", "≤"),
    ("leq", "≤"),
    ("ge", "≥"),
    ("geq", "≥"),
    ("ne", "≠"),
    ("neq", "≠"),
    ("approx", "≈"),
    ("equiv", "≡"),
    ("sim", "∼"),
    ("propto", "∝"),
    ("ll", "≪"),
    ("gg", "≫"),
    ("in", "∈"),
    ("notin", "∉"),
    ("subset", "⊂"),
    ("subseteq", "⊆"),
    ("supset", "⊃"),
    ("supseteq", "⊇"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("setminus", "∖"),
    ("emptyset", "∅"),
    ("times", "×"),
    ("cdot", "·"),
    ("div", "÷"),
    ("pm", "±"),
    ("mp", "∓"),
    ("circ", "∘"),
    ("nabla", "∇"),
    ("partial", "∂"),
    ("infty", "∞"),
    ("sqrt", "√"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("int", "∫"),
    ("R", "ℝ"),
    ("C", "ℂ"),
    ("Z", "ℤ"),
    ("N", "ℕ"),
    ("Q", "ℚ"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("neg", "¬"),
    ("land", "∧"),
    ("lor", "∨"),
    ("angle", "∠"),
    ("perp", "⟂"),
    ("parallel", "∥"),
    ("degree", "°"),
    ("prime", "′"),
    ("cdots", "⋯"),
    ("ldots", "…"),
    ("langle", "⟨"),
    ("rangle", "⟩"),
    ("lfloor", "⌊"),
    ("rfloor", "⌋"),
    ("lceil", "⌈"),
    ("rceil", "⌉"),
    ("hbar", "ℏ"),
    ("minus", "−"),
    ("cdotp", "·"),
    ("colon", ":"),
];

/// What an editor writes for these typed inside a label's text, where they can't be Pencil's own
/// operators. Longest first, since `|->` ends in `->`.
const DIGRAPHS: [(&str, &str); 7] = [
    ("|->", "↦"),
    ("->", "→"),
    ("<=", "≤"),
    (">=", "≥"),
    ("!=", "≠"),
    ("~=", "≈"),
    ("+-", "±"),
];

/// Every name an editor completes after a backslash, with what it writes: the symbols above and
/// the Greek letters LaTeX names.
pub fn symbols() -> Vec<(&'static str, &'static str)> {
    let greek = crate::latex::GREEK
        .iter()
        .filter(|(_, letter)| !letter.is_ascii());
    SYMBOLS.iter().chain(greek).copied().collect()
}

/// What an editor writes for two or three characters typed in a label's text.
pub fn digraphs() -> &'static [(&'static str, &'static str)] {
    &DIGRAPHS
}

#[derive(Debug, PartialEq)]
enum Segment {
    Text(String),
    Math(String),
}

/// Splits on `$`, with `$$` a dollar sign in the words around it.
fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut math = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            current.push(c);
        } else if !math && chars.peek() == Some(&'$') {
            chars.next();
            current.push('$');
        } else {
            let done = std::mem::take(&mut current);
            if !done.is_empty() {
                out.push(match math {
                    true => Segment::Math(done),
                    false => Segment::Text(done),
                });
            }
            math = !math;
        }
    }
    if !current.is_empty() {
        // `check` refuses an open span when a figure compiles, so this is words.
        out.push(Segment::Text(current));
    }
    out
}

/// What a plain run of words is taken to reach, in ems, for a label's extent.
const TEXT_HEIGHT: f64 = 0.72;
const TEXT_DEPTH: f64 = 0.22;

fn assemble(parts: &[Segment], size: f64, font: Font, widths: &dyn TextWidths) -> Laid {
    let mut row = Laying::default();
    for part in parts {
        match part {
            Segment::Text(words) => {
                let width = widths.width(words, size, font) / size;
                row.items.push(Item::Run {
                    text: words.clone(),
                    x: row.width,
                    y: 0.0,
                    scale: 1.0,
                    face: Face::Text,
                    stretch: 1.0,
                });
                row.width += width;
                row.height = row.height.max(TEXT_HEIGHT);
                row.depth = row.depth.max(TEXT_DEPTH);
            }
            Segment::Math(source) => {
                let laid = lay_row(&parse(source), 0);
                row.append(laid, 0.0);
            }
        }
    }
    let half = row.width / 2.0;
    let mut runs: Vec<Run> = Vec::new();
    let mut rules = Vec::new();
    for item in row.items {
        match item {
            Item::Run {
                text,
                x,
                y,
                scale,
                face,
                stretch,
            } => {
                let run = Run {
                    text,
                    x: (x - half) * size,
                    y: (BASELINE - y) * size,
                    size: scale * size,
                    face,
                    stretch,
                };
                // Glyphs that follow on the same line join, so a page has fewer pieces to place.
                match runs.last_mut() {
                    Some(last)
                        if last.face == run.face
                            && last.face == Face::Math
                            && last.size == run.size
                            && last.y == run.y
                            && last.stretch == 1.0
                            && run.stretch == 1.0
                            && (last.x + advance(&last.text) * last.size - run.x).abs() < 1e-6 =>
                    {
                        last.text.push_str(&run.text);
                    }
                    _ => runs.push(run),
                }
            }
            Item::Rule {
                x,
                y,
                width,
                height,
            } => rules.push(Rule {
                x: (x - half) * size,
                y: (BASELINE - y) * size,
                width: width * size,
                height: height * size,
            }),
        }
    }
    Laid {
        width: row.width * size,
        above: (row.height - BASELINE) * size,
        below: (row.depth + BASELINE) * size,
        runs,
        rules,
    }
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// How a thing spaces from its neighbors, as TeX classes them.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    Ord,
    /// A named function or a big operator.
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    /// A fraction.
    Inner,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    /// Glyphs as they're drawn: letters already in their italics.
    Atom {
        text: String,
        class: Class,
    },
    Scripts {
        base: Box<Node>,
        sup: Option<Vec<Node>>,
        sub: Option<Vec<Node>>,
    },
    /// `bracketed` says which side was written in parentheses that the bar made unnecessary,
    /// and which come back where the fraction is too small to stack.
    Frac {
        num: Vec<Node>,
        den: Vec<Node>,
        bracketed: [bool; 2],
    },
    Root(Vec<Node>),
    Fence {
        open: char,
        body: Vec<Node>,
        close: Option<char>,
    },
}

/// Names set upright that Pencil doesn't compute with.
const WORDS: [&str; 12] = [
    "lim", "sup", "inf", "det", "gcd", "lcm", "dim", "ker", "deg", "Re", "Im", "lg",
];

fn is_function(name: &str) -> bool {
    name.chars().count() > 1
        && (FUNCTIONS.iter().any(|(known, _)| *known == name) || WORDS.contains(&name))
}

/// A letter as a variable: the math font's italic of it.
fn italic(c: char) -> char {
    let at = |base: u32, first: char| char::from_u32(base + (c as u32 - first as u32)).unwrap_or(c);
    match c {
        'h' => 'ℎ',
        'a'..='z' => at(0x1d44e, 'a'),
        'A'..='Z' => at(0x1d434, 'A'),
        'α'..='ω' => at(0x1d6fc, 'α'),
        'ϵ' => '𝜖',
        'ϑ' => '𝜗',
        'ϰ' => '𝜘',
        'ϕ' => '𝜙',
        'ϱ' => '𝜚',
        'ϖ' => '𝜛',
        // The glyphs Pencil reads its constants from print as the letters they are.
        'ⅈ' => at(0x1d44e + 8, 'ⅈ'),
        'ⅇ' | 'ℯ' => '𝑒',
        _ => c,
    }
}

/// A superscript or subscript character, as what it raises or lowers.
fn script(c: char) -> Option<(bool, char)> {
    let digit = |first: char| char::from_digit(c as u32 - first as u32, 10).unwrap_or('0');
    Some(match c {
        '⁰' => (true, '0'),
        '¹' => (true, '1'),
        '²' => (true, '2'),
        '³' => (true, '3'),
        '⁴'..='⁹' => (true, digit('⁰')),
        '⁺' => (true, '+'),
        '⁻' => (true, '−'),
        'ⁿ' => (true, 'n'),
        'ⁱ' => (true, 'i'),
        'ᵗ' => (true, 't'),
        '₀'..='₉' => (false, digit('₀')),
        '₊' => (false, '+'),
        '₋' => (false, '−'),
        _ => return None,
    })
}

fn class_of(c: char) -> Class {
    match c {
        '+' | '−' | '±' | '∓' | '·' | '×' | '÷' | '∘' | '∗' | '∪' | '∩' | '∧' | '∨' | '⊕' | '⊗'
        | '∖' => Class::Bin,
        '=' | '≠' | '<' | '>' | '≤' | '≥' | '≈' | '≡' | '∼' | '≃' | '≅' | '∝' | '∈' | '∉' | '∋'
        | '⊂' | '⊃' | '⊆' | '⊇' | '∥' | '⟂' | '⊥' | '≪' | '≫' | '≔' => {
            Class::Rel
        }
        '←'..='⇿' | '⟵'..='⟿' => Class::Rel,
        '(' | '[' | '⟨' | '⌈' | '⌊' => Class::Open,
        ')' | ']' | '⟩' | '⌉' | '⌋' => Class::Close,
        ',' | ';' | ':' => Class::Punct,
        '∑' | '∏' | '∫' | '∮' | '⋃' | '⋂' => Class::Op,
        _ => Class::Ord,
    }
}

fn closes(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        '⟨' => '⟩',
        '⌈' => '⌉',
        _ => '⌋',
    }
}

fn atom(text: impl Into<String>, class: Class) -> Node {
    Node::Atom {
        text: text.into(),
        class,
    }
}

struct Reader {
    chars: Vec<char>,
    at: usize,
}

fn parse(source: &str) -> Vec<Node> {
    let mut reader = Reader {
        chars: source.chars().collect(),
        at: 0,
    };
    let mut row = reader.row(None);
    // A bracket that closes nothing is drawn where it's written.
    while reader.at < reader.chars.len() {
        row.push(atom(reader.chars[reader.at], Class::Close));
        reader.at += 1;
        row.extend(reader.row(None));
    }
    row
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn spaces(&mut self) -> bool {
        let from = self.at;
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
        self.at > from
    }

    /// What's written up to `until` or the end. It stops before a closing bracket either way.
    fn row(&mut self, until: Option<char>) -> Vec<Node> {
        let mut row: Vec<Node> = Vec::new();
        loop {
            self.spaces();
            let Some(c) = self.peek() else { break };
            if Some(c) == until || class_of(c) == Class::Close {
                break;
            }
            if c == '/' {
                self.at += 1;
                self.fraction(&mut row);
                continue;
            }
            let node = self.factor();
            row.push(node);
        }
        row
    }

    /// The bar takes the product before it and the factors that follow it with no space, as
    /// `1/2π` is 1 over 2π where Pencil computes.
    fn fraction(&mut self, row: &mut Vec<Node>) {
        let mut from = row.len();
        while from > 0 && is_factor(&row[from - 1]) {
            from -= 1;
        }
        let num: Vec<Node> = row.drain(from..).collect();
        let mut den = Vec::new();
        while self.peek().is_some_and(starts_factor) {
            den.push(self.factor());
        }
        if num.is_empty() || den.is_empty() {
            // Nothing to stack: a slash, as written.
            row.extend(num);
            row.push(atom('/', Class::Ord));
            row.extend(den);
            return;
        }
        let (num, num_bracketed) = unbracketed(num);
        let (den, den_bracketed) = unbracketed(den);
        row.push(Node::Frac {
            num,
            den,
            bracketed: [num_bracketed, den_bracketed],
        });
    }

    /// One thing and the scripts written on it.
    fn factor(&mut self) -> Node {
        let mut node = self.primary();
        loop {
            match self.peek() {
                Some('^') => {
                    self.at += 1;
                    let raised = self.argument();
                    node = scripted(node, Some(raised), None);
                }
                Some('_') => {
                    self.at += 1;
                    let lowered = self.argument();
                    node = scripted(node, None, Some(lowered));
                }
                Some(c) if script(c).is_some() => {
                    let (up, _) = script(c).unwrap();
                    let mut text = String::new();
                    while let Some((same, plain)) = self.peek().and_then(script) {
                        if same != up {
                            break;
                        }
                        text.push(plain);
                        self.at += 1;
                    }
                    let nodes = parse(&text);
                    node = match up {
                        true => scripted(node, Some(nodes), None),
                        false => scripted(node, None, Some(nodes)),
                    };
                }
                _ => return node,
            }
        }
    }

    /// What a `^` or `_` takes: a group, whose parentheses aren't drawn, or one thing, which
    /// can have a sign.
    fn argument(&mut self) -> Vec<Node> {
        match self.peek() {
            Some('(') => {
                self.at += 1;
                let row = self.row(Some(')'));
                if self.peek() == Some(')') {
                    self.at += 1;
                }
                row
            }
            Some(sign @ ('-' | '−' | '+')) => {
                self.at += 1;
                let sign = if sign == '+' { '+' } else { '−' };
                let mut row = vec![atom(sign, Class::Ord)];
                if self.peek().is_some_and(starts_factor) {
                    row.push(self.primary());
                }
                row
            }
            Some(c) if starts_factor(c) => vec![self.primary()],
            _ => Vec::new(),
        }
    }

    fn primary(&mut self) -> Node {
        let c = self.chars[self.at];
        if c.is_ascii_digit() {
            return atom(self.number(), Class::Ord);
        }
        if c.is_ascii_alphabetic() {
            return self.name();
        }
        self.at += 1;
        match c {
            '√' => {
                let body = match self.peek() {
                    Some('(') => self.argument(),
                    Some(c) if starts_factor(c) => vec![self.primary()],
                    _ => Vec::new(),
                };
                Node::Root(body)
            }
            '(' | '[' | '⟨' | '⌈' | '⌊' => {
                let body = self.row(Some(closes(c)));
                let close = (self.peek() == Some(closes(c))).then(|| {
                    self.at += 1;
                    closes(c)
                });
                Node::Fence {
                    open: c,
                    body,
                    close,
                }
            }
            '-' => atom('−', Class::Bin),
            '*' => atom('·', Class::Bin),
            '\'' => atom('′', Class::Ord),
            _ => atom(italic(c), class_of(c)),
        }
    }

    fn number(&mut self) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            let point = c == '.'
                && self
                    .chars
                    .get(self.at + 1)
                    .is_some_and(char::is_ascii_digit);
            if !c.is_ascii_digit() && !point {
                break;
            }
            text.push(c);
            self.at += 1;
        }
        text
    }

    /// Letters together are one name, as Pencil reads them: a function's is upright, `sqrt`
    /// and `abs` draw what they mean, and digits that end a name are its subscript.
    fn name(&mut self) -> Node {
        let mut name = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            name.push(c);
            self.at += 1;
        }
        if self.peek() == Some('(') && (name == "sqrt" || name == "abs") {
            let body = self.argument();
            return match name.as_str() {
                "sqrt" => Node::Root(body),
                _ => Node::Fence {
                    open: '|',
                    body,
                    close: Some('|'),
                },
            };
        }
        if is_function(&name) {
            return atom(name, Class::Op);
        }
        let letters = atom(name.chars().map(italic).collect::<String>(), Class::Ord);
        let mut digits = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_digit) {
            digits.push(c);
            self.at += 1;
        }
        match digits.is_empty() {
            true => letters,
            false => scripted(letters, None, Some(vec![atom(digits, Class::Ord)])),
        }
    }
}

/// Whether a character begins something a product or a denominator can take.
fn starts_factor(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(c, '(' | '[' | '√' | '∞' | '∂' | '∇' | '⟨' | '⌈' | '⌊')
        || (!c.is_whitespace()
            && class_of(c) == Class::Ord
            && !matches!(c, '/' | '^' | '_' | '|' | '\'' | '.' | '!' | '$'))
}

/// Whether a node is part of the product a `/` divides.
fn is_factor(node: &Node) -> bool {
    match node {
        Node::Atom { class, .. } => *class == Class::Ord,
        Node::Scripts { base, .. } => is_factor(base),
        Node::Frac { .. } => false,
        Node::Root(_) | Node::Fence { .. } => true,
    }
}

/// A numerator or denominator written as one parenthesized group, without the parentheses.
fn unbracketed(mut nodes: Vec<Node>) -> (Vec<Node>, bool) {
    if nodes.len() == 1
        && matches!(
            &nodes[0],
            Node::Fence {
                open: '(',
                close: Some(')'),
                ..
            }
        )
        && let Some(Node::Fence { body, .. }) = nodes.pop()
    {
        return (body, true);
    }
    (nodes, false)
}

fn scripted(node: Node, sup: Option<Vec<Node>>, sub: Option<Vec<Node>>) -> Node {
    match node {
        // A second script joins the first, as `a_n^2` has both on `a`.
        Node::Scripts {
            base,
            sup: old_sup,
            sub: old_sub,
        } if (sup.is_none() || old_sup.is_none()) && (sub.is_none() || old_sub.is_none()) => {
            Node::Scripts {
                base,
                sup: sup.or(old_sup),
                sub: sub.or(old_sub),
            }
        }
        base => Node::Scripts {
            base: Box::new(base),
            sup,
            sub,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Laying out, in ems of the label's size, with y up from the baseline
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Item {
    Run {
        text: String,
        x: f64,
        y: f64,
        scale: f64,
        face: Face,
        stretch: f64,
    },
    Rule {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Clone, Debug, Default)]
struct Laying {
    width: f64,
    height: f64,
    depth: f64,
    /// The last glyph's lean, which a superscript clears and a subscript tucks under.
    italic: f64,
    items: Vec<Item>,
}

impl Laying {
    /// Sets `other` after what's here, raised by `raise`.
    fn append(&mut self, other: Laying, raise: f64) {
        self.place(other, self.width, raise);
    }

    /// Sets `other` with its left edge at `x`, raised by `raise`.
    fn place(&mut self, other: Laying, x: f64, raise: f64) {
        for mut item in other.items {
            match &mut item {
                Item::Run { x: ix, y: iy, .. } | Item::Rule { x: ix, y: iy, .. } => {
                    *ix += x;
                    *iy += raise;
                }
            }
            self.items.push(item);
        }
        self.width = self.width.max(x + other.width);
        self.height = self.height.max(other.height + raise);
        self.depth = self.depth.max(other.depth - raise);
        self.italic = other.italic;
    }
}

/// A glyph's `(advance, ink bottom, ink top, ink right, italic correction)`, in ems.
fn glyph(c: char) -> Option<[f64; 5]> {
    let at = m::GLYPHS
        .binary_search_by_key(&(c as u32), |entry| entry.0)
        .ok()?;
    let (_, advance, bottom, top, right, lean) = m::GLYPHS[at];
    Some([advance, bottom, top, right, lean].map(|v| f64::from(v) / m::UNITS_PER_EM))
}

/// What a glyph the math font lacks is taken to be, so the rest still sits about right around
/// whatever a renderer finds to draw it with.
const UNKNOWN: [f64; 5] = [0.55, 0.0, 0.66, 0.5, 0.0];

/// How wide the math font sets text, in ems.
fn advance(text: &str) -> f64 {
    text.chars().map(|c| glyph(c).unwrap_or(UNKNOWN)[0]).sum()
}

fn em(units: f64) -> f64 {
    units / m::UNITS_PER_EM
}

/// The size of a level: a label's own, a script's, and a script's script.
fn scale(level: usize) -> f64 {
    match level {
        0 => 1.0,
        1 => m::SCRIPT_PERCENT / 100.0,
        _ => m::SCRIPT_SCRIPT_PERCENT / 100.0,
    }
}

fn glyphs(text: &str, level: usize) -> Laying {
    let s = scale(level);
    let mut laid = Laying::default();
    for c in text.chars() {
        let [advance, bottom, top, _, lean] = glyph(c).unwrap_or(UNKNOWN);
        laid.width += advance * s;
        laid.height = laid.height.max(top * s);
        laid.depth = laid.depth.max(-bottom * s);
        laid.italic = lean * s;
    }
    laid.items.push(Item::Run {
        text: text.to_string(),
        x: 0.0,
        y: 0.0,
        scale: s,
        face: Face::Math,
        stretch: 1.0,
    });
    laid
}

/// The classes a node shows its neighbors on each side.
fn sides(node: &Node) -> (Class, Class) {
    match node {
        Node::Atom { class, .. } => (*class, *class),
        Node::Scripts { base, .. } => sides(base),
        Node::Frac { .. } => (Class::Inner, Class::Inner),
        Node::Root(_) => (Class::Ord, Class::Ord),
        Node::Fence { open, .. } => match open {
            '|' => (Class::Ord, Class::Ord),
            _ => (Class::Open, Class::Close),
        },
    }
}

/// TeX's space between two classes, in eighteenths of an em, and whether it's dropped in a
/// script.
fn space(left: Class, right: Class) -> (f64, bool) {
    use Class::*;
    const THIN: f64 = 3.0;
    const MEDIUM: f64 = 4.0;
    const THICK: f64 = 5.0;
    match (left, right) {
        (Ord | Close, Op) | (Op, Ord | Op) | (Inner, Op) => (THIN, false),
        (Ord | Close | Inner, Bin) | (Bin, Ord | Op | Open | Inner) => (MEDIUM, true),
        (Ord | Op | Close | Inner, Rel) | (Rel, Ord | Op | Open | Inner) => (THICK, true),
        (Ord | Op | Close, Inner) | (Inner, Ord | Open | Punct | Inner) => (THIN, true),
        (Punct, Bin) => (0.0, false),
        (Punct, _) => (THIN, true),
        _ => (0.0, false),
    }
}

/// Whether a node begins with a letter in italics, which sets against another as type does.
fn leans(node: &Node) -> bool {
    match node {
        Node::Atom { text, .. } => text
            .chars()
            .next()
            .is_some_and(|c| c == 'ℎ' || ('𝐴'..='𝜛').contains(&c)),
        Node::Scripts { base, .. } => leans(base),
        _ => false,
    }
}

fn lay_row(nodes: &[Node], level: usize) -> Laying {
    // A sign with nothing before it to add to is part of what follows it.
    let mut classes: Vec<(Class, Class)> = nodes.iter().map(sides).collect();
    for i in 0..classes.len() {
        if classes[i].0 != Class::Bin {
            continue;
        }
        let before = i.checked_sub(1).map(|j| classes[j].1);
        let after = classes.get(i + 1).map(|c| c.0);
        let lone = matches!(
            before,
            None | Some(Class::Bin | Class::Op | Class::Rel | Class::Open | Class::Punct)
        ) || matches!(after, None | Some(Class::Rel | Class::Close | Class::Punct));
        if lone {
            classes[i] = (Class::Ord, Class::Ord);
        }
    }
    let mut row = Laying::default();
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            let (mu, text_only) = space(classes[i - 1].1, classes[i].0);
            let gap = match text_only && level > 0 {
                true => 0.0,
                false => mu / 18.0 * scale(level),
            };
            // A leaning letter keeps clear of an upright thing set right against it.
            if gap == 0.0 && !leans(node) {
                row.width += row.italic;
            }
            row.width += gap;
        }
        let laid = lay_node(node, level);
        row.append(laid, 0.0);
    }
    row
}

fn lay_node(node: &Node, level: usize) -> Laying {
    match node {
        Node::Atom { text, .. } => glyphs(text, level),
        Node::Scripts { base, sup, sub } => {
            lay_scripts(base, sup.as_deref(), sub.as_deref(), level)
        }
        Node::Frac {
            num,
            den,
            bracketed,
        } => match level {
            0 => lay_fraction(num, den),
            // Too small to stack, so it's on the line with a slash, as it was written.
            _ => {
                let side = |nodes: &[Node], bracketed: bool| match bracketed {
                    true => lay_fence('(', nodes, Some(')'), level),
                    false => lay_row(nodes, level),
                };
                let mut row = side(num, bracketed[0]);
                row.width += row.italic;
                row.append(glyphs("/", level), 0.0);
                row.append(side(den, bracketed[1]), 0.0);
                row
            }
        },
        Node::Root(body) => lay_root(body, level),
        Node::Fence { open, body, close } => lay_fence(*open, body, *close, level),
    }
}

/// Scripts by the font's own constants: how far up a superscript goes, how far down a subscript,
/// and how much room they keep from the base and from each other.
fn lay_scripts(base: &Node, sup: Option<&[Node]>, sub: Option<&[Node]>, level: usize) -> Laying {
    let s = scale(level);
    let mut laid = lay_node(base, level);
    let lean = laid.italic;
    let sup = sup.map(|nodes| lay_row(nodes, level + 1));
    let sub = sub.map(|nodes| lay_row(nodes, level + 1));
    let mut up = sup.as_ref().map(|raised| {
        (em(m::SUPERSCRIPT_SHIFT_UP) * s)
            .max(laid.height - em(m::SUPERSCRIPT_BASELINE_DROP_MAX) * s)
            .max(raised.depth + em(m::SUPERSCRIPT_BOTTOM_MIN) * s)
    });
    let mut down = sub.as_ref().map(|lowered| {
        (em(m::SUBSCRIPT_SHIFT_DOWN) * s)
            .max(laid.depth + em(m::SUBSCRIPT_BASELINE_DROP_MIN) * s)
            .max(lowered.height - em(m::SUBSCRIPT_TOP_MAX) * s)
    });
    if let (Some(u), Some(d), Some(raised), Some(lowered)) = (&mut up, &mut down, &sup, &sub) {
        let gap = (*u - raised.depth) - (lowered.height - *d);
        let least = em(m::SUB_SUPERSCRIPT_GAP_MIN) * s;
        if gap < least {
            *d += least - gap;
        }
    }
    let at = laid.width;
    if let (Some(raised), Some(u)) = (sup, up) {
        laid.place(raised, at + lean, u);
    }
    if let (Some(lowered), Some(d)) = (sub, down) {
        laid.place(lowered, at, -d);
    }
    laid.width += em(m::SPACE_AFTER_SCRIPT) * s;
    laid.italic = 0.0;
    laid
}

/// A fraction's bar sits on the font's math axis, with its parts a script's size, each at least
/// the font's gap from the bar.
fn lay_fraction(num: &[Node], den: &[Node]) -> Laying {
    let top = lay_row(num, 1);
    let bottom = lay_row(den, 1);
    let axis = em(m::AXIS_HEIGHT);
    let thick = em(m::FRACTION_RULE_THICKNESS);
    let up = em(m::FRACTION_NUMERATOR_SHIFT_UP)
        .max(axis + thick / 2.0 + em(m::FRACTION_NUMERATOR_GAP_MIN) + top.depth);
    let down = em(m::FRACTION_DENOMINATOR_SHIFT_DOWN)
        .max(bottom.height + em(m::FRACTION_DENOMINATOR_GAP_MIN) - (axis - thick / 2.0));
    // The bar reaches a little past the wider part, and stands a little clear of its neighbors.
    const OVERHANG: f64 = 0.08;
    const CLEAR: f64 = 0.06;
    let bar = top.width.max(bottom.width) + 2.0 * OVERHANG;
    let mut laid = Laying::default();
    let (top_width, bottom_width) = (top.width, bottom.width);
    laid.place(top, CLEAR + (bar - top_width) / 2.0, up);
    laid.place(bottom, CLEAR + (bar - bottom_width) / 2.0, -down);
    laid.items.push(Item::Rule {
        x: CLEAR,
        y: axis + thick / 2.0,
        width: bar,
        height: thick,
    });
    laid.width = bar + 2.0 * CLEAR;
    laid.italic = 0.0;
    laid
}

/// A root is the font's `√`, made taller when what's under it is, with a bar from its top.
fn lay_root(body: &[Node], level: usize) -> Laying {
    let s = scale(level);
    let inner = lay_row(body, level);
    let [_, bottom, top, right, _] = glyph('√').unwrap_or(UNKNOWN);
    let thick = em(m::RADICAL_RULE_THICKNESS) * s;
    let gap = em(m::RADICAL_VERTICAL_GAP) * s;
    // A root over nothing tall is still a letter's height.
    let under = inner.height.max(em(m::X_HEIGHT) * s);
    let needed = under + inner.depth + gap + thick;
    let natural = (top - bottom) * s;
    let stretch = (needed / natural).max(1.0);
    // What the glyph has to spare goes half above and half below.
    let spare = natural * stretch - needed;
    let reach = under + gap + thick + spare / 2.0;
    let raise = reach - top * s * stretch;
    let mut laid = Laying::default();
    laid.items.push(Item::Run {
        text: '√'.to_string(),
        x: 0.0,
        y: raise,
        scale: s,
        face: Face::Math,
        stretch,
    });
    // The glyph's ink ends where its bar begins, which is past its advance.
    let start = right * s;
    const PAD: f64 = 0.04;
    laid.items.push(Item::Rule {
        x: start - 0.01 * s,
        y: reach,
        width: inner.width + inner.italic + (PAD + 0.01) * s,
        height: thick,
    });
    laid.height = reach + em(m::RADICAL_EXTRA_ASCENDER) * s;
    laid.depth = -(raise + bottom * s * stretch);
    laid.width = start;
    let lean = inner.italic;
    laid.place(inner, start, 0.0);
    laid.width += lean + PAD * s;
    laid.italic = 0.0;
    laid
}

/// Brackets around what they hold, made taller about the math axis when it's taller than they are.
fn lay_fence(open: char, body: &[Node], close: Option<char>, level: usize) -> Laying {
    let s = scale(level);
    let inner = lay_row(body, level);
    let axis = em(m::AXIS_HEIGHT) * s;
    let reach = (inner.height - axis).max(inner.depth + axis);
    let bracket = |c: char| {
        let [_, bottom, top, _, _] = glyph(c).unwrap_or(UNKNOWN);
        let natural = (top - bottom) * s;
        // As TeX asks of a delimiter: nine tenths of what it surrounds.
        let stretch = match 2.0 * reach * 0.9 / natural {
            needed if needed > 1.08 => needed,
            _ => 1.0,
        };
        let mut laid = glyphs(&c.to_string(), level);
        if stretch > 1.0 {
            let center = (top + bottom) / 2.0 * s * stretch;
            let raise = axis - center;
            if let Some(Item::Run {
                y, stretch: tall, ..
            }) = laid.items.first_mut()
            {
                *y = raise;
                *tall = stretch;
            }
            laid.height = top * s * stretch + raise;
            laid.depth = -(bottom * s * stretch + raise);
        }
        laid
    };
    let mut laid = bracket(open);
    let lean = inner.italic;
    laid.append(inner, 0.0);
    if let Some(close) = close {
        // A leaning letter keeps clear of the bracket that closes on it.
        laid.width += lean;
        laid.append(bracket(close), 0.0);
    }
    laid.italic = 0.0;
    laid
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terms::Estimate;

    const SIZE: f64 = 100.0;

    fn math(text: &str) -> Laid {
        lay_math(text, SIZE)
    }

    fn run<'a>(laid: &'a Laid, text: &str) -> &'a Run {
        laid.runs
            .iter()
            .find(|r| r.text == text)
            .unwrap_or_else(|| panic!("no run {text:?} in {:?}", laid.runs))
    }

    #[test]
    fn text_without_a_span_is_the_renderers() {
        assert!(lay("arg z", SIZE, Font::Math, &Estimate).is_none());
        assert_eq!(width("abc", 10.0, Font::Sans, &Estimate), 16.5);
    }

    #[test]
    fn a_span_has_to_close() {
        assert!(check("area = $x^2$").is_ok());
        assert!(check("costs $$5").is_ok());
        assert!(check("area = $x^2").is_err());
    }

    #[test]
    fn words_and_mathematics_share_a_line() {
        let laid = lay("area = $x^2$", SIZE, Font::Sans, &Estimate).unwrap();
        let words = &laid.runs[0];
        assert_eq!((words.text.as_str(), words.face), ("area = ", Face::Text));
        let x = run(&laid, "𝑥");
        // The words are seven characters of the estimate's width, and the x starts where they end.
        assert!((x.x - words.x - 7.0 * 0.55 * SIZE).abs() < 1e-9);
        assert_eq!(x.y, words.y);
        // A dollar sign is two of them.
        let price = lay("$$5 for $n$", SIZE, Font::Sans, &Estimate).unwrap();
        assert_eq!(price.runs[0].text, "$5 for ");
    }

    #[test]
    fn letters_are_italic_and_digits_and_functions_upright() {
        let laid = math("2 sin x");
        assert_eq!(run(&laid, "2").face, Face::Math);
        run(&laid, "sin");
        run(&laid, "𝑥");
        // A function's name stands a thin space from what it's of.
        let (sin, x) = (run(&laid, "sin"), run(&laid, "𝑥"));
        let gap = x.x - (sin.x + advance("sin") * SIZE);
        assert!((gap - 3.0 / 18.0 * SIZE).abs() < 1e-9, "{gap}");
        // Letters together are one name, as Pencil reads them, and stay letters.
        assert_eq!(math("2ab").runs[0].text, "2𝑎𝑏");
    }

    /// The font's own numbers place a script: a superscript's baseline is its shift up, at the
    /// font's script size, past the base and its lean.
    #[test]
    fn a_superscript_sits_where_the_font_says() {
        let laid = math("x^2");
        let (x, two) = (run(&laid, "𝑥"), run(&laid, "2"));
        assert_eq!(two.size, SIZE * m::SCRIPT_PERCENT / 100.0);
        assert!((x.y - two.y - em(m::SUPERSCRIPT_SHIFT_UP) * SIZE).abs() < 1e-9);
        let [advance, _, _, _, lean] = glyph('𝑥').unwrap();
        assert!((two.x - x.x - (advance + lean) * SIZE).abs() < 1e-9);
        // The same thing written with the character.
        assert_eq!(math("x²"), laid);
    }

    #[test]
    fn a_subscript_sits_below_and_both_keep_apart() {
        let laid = math("x_1");
        let (x, one) = (run(&laid, "𝑥"), run(&laid, "1"));
        assert!((one.y - x.y - em(m::SUBSCRIPT_SHIFT_DOWN) * SIZE).abs() < 1e-9);
        // The subscript tucks under the lean the superscript clears.
        assert!((one.x - x.x - glyph('𝑥').unwrap()[0] * SIZE).abs() < 1e-9);
        // A name that ends in digits has them as its subscript, however it's written.
        assert_eq!(math("x1"), laid);
        assert_eq!(math("x₁"), laid);

        let both = math("a_n^2");
        let (n, two) = (run(&both, "𝑛"), run(&both, "2"));
        let script = SIZE * m::SCRIPT_PERCENT / 100.0;
        let two_bottom = two.y - glyph('2').unwrap()[1] * script;
        let n_top = n.y - glyph('𝑛').unwrap()[2] * script;
        assert!(n_top - two_bottom >= em(m::SUB_SUPERSCRIPT_GAP_MIN) * SIZE - 1e-9);
    }

    #[test]
    fn a_fraction_stacks_about_its_bar() {
        let laid = math("x^2/2");
        assert_eq!(laid.rules.len(), 1);
        let bar = laid.rules[0];
        // The bar is centered on the math axis and as thick as the font says.
        let axis = (BASELINE - em(m::AXIS_HEIGHT)) * SIZE;
        assert!((bar.y + bar.height / 2.0 - axis).abs() < 1e-9);
        assert!((bar.height - em(m::FRACTION_RULE_THICKNESS) * SIZE).abs() < 1e-9);
        let (x, twos): (&Run, Vec<&Run>) = (
            run(&laid, "𝑥"),
            laid.runs.iter().filter(|r| r.text == "2").collect(),
        );
        let below = twos.iter().find(|r| r.y > bar.y).expect("a denominator");
        assert!(x.y < bar.y && below.y > bar.y + bar.height);
        // Both parts are centered on the bar.
        let middle = bar.x + bar.width / 2.0;
        let center = below.x + advance("2") * below.size / 2.0;
        assert!((center - middle).abs() < 1e-9);
        // It's taller than a line of text, and says so.
        assert!(laid.above > 0.5 * SIZE && laid.below > 0.5 * SIZE);
        assert!((laid.width - (bar.width + 0.12 * SIZE)).abs() < 1e-9);
    }

    #[test]
    fn a_bar_takes_what_pencil_would_divide() {
        // The product before it, and the factors after it with no space.
        let laid = math("1/2π x");
        let bar = laid.rules[0];
        let pi = run(&laid, "2𝜋");
        assert!(pi.y > bar.y);
        let x = run(&laid, "𝑥");
        assert!(x.x > bar.x + bar.width && (x.y - BASELINE * SIZE).abs() < 1e-9);
        // Parentheses that only group go, since the bar groups.
        let grouped = math("(a + b)/2");
        assert!(grouped.runs.iter().all(|r| !r.text.contains('(')));
        // A sum isn't taken whole.
        let sum = math("a + b/2");
        let a = run(&sum, "𝑎");
        assert!((a.y - BASELINE * SIZE).abs() < 1e-9);
    }

    #[test]
    fn a_fraction_in_a_script_stays_on_the_line() {
        let laid = math("e^(−2πi/3)");
        assert!(laid.rules.is_empty());
        assert!(laid.runs.iter().any(|r| r.text.contains('/')));
        // And its parentheses come back where it needs them.
        let grouped = math("e^((a + b)/2)");
        assert!(grouped.runs.iter().any(|r| r.text.contains('(')));
    }

    #[test]
    fn a_root_has_a_bar_over_what_it_holds() {
        let laid = math("sqrt(x + 1)");
        assert_eq!(laid, math("√(x + 1)"));
        let root = run(&laid, "√");
        assert_eq!(root.stretch, 1.0);
        let bar = laid.rules[0];
        let (x, one) = (run(&laid, "𝑥"), run(&laid, "1"));
        assert!(bar.x <= x.x && bar.x + bar.width >= one.x + advance("1") * SIZE);
        // The bar clears the tallest thing under it by the font's gap.
        let top_of_one = one.y - glyph('1').unwrap()[2] * SIZE;
        assert!(top_of_one - (bar.y + bar.height) >= em(m::RADICAL_VERTICAL_GAP) * SIZE - 1e-9);
        // Something tall under it makes the glyph taller, not the font's own height.
        let tall = math("√(1/2)");
        assert!(run(&tall, "√").stretch > 1.0);
    }

    #[test]
    fn brackets_grow_around_a_fraction() {
        let plain = math("f(x)");
        assert!(plain.runs.iter().all(|r| r.stretch == 1.0));
        let tall = math("(1/2)^2");
        let open = run(&tall, "(");
        assert!(open.stretch > 1.0);
        // The exponent goes on the bracket, above where it would on a letter.
        let two = tall
            .runs
            .iter()
            .rfind(|r| r.text == "2")
            .expect("an exponent");
        assert!(BASELINE * SIZE - two.y > em(m::SUPERSCRIPT_SHIFT_UP) * SIZE);
        let bars: String = math("abs(z)")
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(bars, "|𝑧|");
    }

    #[test]
    fn operators_space_as_tex_spaces_them() {
        let laid = math("a + b = c");
        let at = |text: &str| run(&laid, text).x;
        let right = |text: &str| at(text) + advance(text) * SIZE;
        assert!((at("+") - right("𝑎") - 4.0 / 18.0 * SIZE).abs() < 1e-9);
        assert!((at("=") - right("𝑏") - 5.0 / 18.0 * SIZE).abs() < 1e-9);
        // A sign with nothing before it is part of its number, and a hyphen is a minus.
        let negative = math("-7");
        assert_eq!(negative.runs[0].text, "−7");
        // Spaces in the source don't space the result.
        assert_eq!(math("a+b=c"), laid);
    }

    /// An editor only writes what the math font can draw, and a name means one thing.
    #[test]
    fn every_symbol_an_editor_writes_is_in_the_font() {
        let all = symbols();
        for (name, symbol) in &all {
            assert!(
                symbol.chars().all(|c| glyph(c).is_some()),
                "\\{name} writes {symbol}, which the font lacks"
            );
            assert_eq!(all.iter().filter(|(n, _)| n == name).count(), 1, "{name}");
        }
        assert!(all.contains(&("theta", "θ")) && all.contains(&("to", "→")));
        for (typed, symbol) in digraphs() {
            assert!(symbol.chars().all(|c| glyph(c).is_some()), "{typed}");
        }
    }

    #[test]
    fn what_the_font_lacks_still_takes_room() {
        let laid = math("x 😀 y");
        let known = (advance("𝑥") + advance("𝑦")) * SIZE;
        assert!(laid.width > known + 0.5 * SIZE);
    }
}
