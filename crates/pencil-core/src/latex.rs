//! LaTeX, as Desmos and MathLive copy it, read as Pencil text, so a formula pasted from either
//! lands on the names and products Pencil reads when it's typed: `\frac{1}{2}x^{2}` is
//! `1/2·x^2`, `\theta_{1}` is `θ_1`, and `\left|z\right|` is `abs(z)`.
//!
//! LaTeX runs single letters together as a product, `xy` being x·y, where Pencil reads one name,
//! so they come out apart. A letter before parentheses multiplies them too, unless the letter
//! is a function: a built-in, one the figure defines, or the one a line defines, as in
//! `f\left(x\right)=x^{2}`. What has no Pencil form yet, such as Desmos's piecewise braces or a
//! comparison, is an error that says so, rather than something close.

use crate::Error;
use crate::parse::{FUNCTIONS, subscripted};

/// Whether pasted text is LaTeX rather than Pencil: Pencil never writes a backslash, `^{`, or a
/// line that starts with `$`.
pub fn is_latex(text: &str) -> bool {
    text.contains('\\') || text.contains("^{") || text.trim_start().starts_with('$')
}

/// Each line of `latex` as a line of Pencil text. `functions` are the names a figure defines
/// with `fn`, which a letter before parentheses calls rather than multiplies.
pub fn read(latex: &str, functions: &[&str]) -> Result<String, Error> {
    let lines: Result<Vec<String>, Error> = latex
        .lines()
        .enumerate()
        .map(|(n, line)| {
            if line.trim().is_empty() {
                return Ok(String::new());
            }
            Reader::new(line, n + 1, functions)?.line()
        })
        .collect();
    // Lines copied whole end with a line break, and so does what they paste as.
    let end = if latex.ends_with('\n') { "\n" } else { "" };
    Ok(lines?.join("\n") + end)
}

// ---- Tokens ----

#[derive(Clone, Debug, PartialEq)]
enum T {
    /// `\name`, or a backslash and one other character, as `\{`.
    Cmd(String),
    /// Digits, with a decimal point if it has one.
    Num(String),
    /// An opening delimiter: `(`, `[`, or one sized with `\left`, as `|` for `\left|`.
    Open(char),
    Close(char),
    Ch(char),
    End,
}

/// Commands that only space things out.
const SPACING: [&str; 10] =
    [",", ":", ";", "!", " ", "quad", "qquad", "displaystyle", "textstyle", "limits"];

fn lex(line: &str) -> Result<Vec<(T, usize)>, Error> {
    let mut chars: Vec<char> = line.chars().collect();
    // Math delimiters around the whole line, as MathLive copies it: `$$…$$`, `$…$`, `\[…\]`
    // and `\(…\)` become spaces, so columns still count from the line as pasted.
    let text: String = chars.iter().collect();
    let (start, end) = (text.len() - text.trim_start().len(), text.trim_end().chars().count());
    let start = text[..start].chars().count();
    for (open, close) in [("$$", "$$"), ("$", "$"), ("\\[", "\\]"), ("\\(", "\\)")] {
        let body: String = chars[start..end].iter().collect();
        if body.len() >= open.len() + close.len() && body.starts_with(open) && body.ends_with(close) {
            for i in 0..open.chars().count() {
                chars[start + i] = ' ';
            }
            for i in 0..close.chars().count() {
                chars[end - 1 - i] = ' ';
            }
            break;
        }
    }

    let word = |i: usize| -> (String, usize) {
        let mut j = i;
        while j < chars.len() && chars[j].is_ascii_alphabetic() {
            j += 1;
        }
        (chars[i..j].iter().collect(), j)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (c, col) = (chars[i], i + 1);
        if c.is_whitespace() || c == '~' {
            i += 1;
        } else if c == '\\' {
            let (name, next) = match chars.get(i + 1) {
                Some(c) if c.is_ascii_alphabetic() => word(i + 1),
                Some(c) => (c.to_string(), i + 2),
                None => return Err(Error::at(0, col, "a backslash with nothing after it")),
            };
            i = next;
            if SPACING.contains(&name.as_str()) {
                continue;
            }
            // `\left(` and `\right|` are their delimiter, marked as opening or closing.
            if name == "left" || name == "right" {
                while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                    i += 1;
                }
                let (delimiter, next) = match chars.get(i) {
                    Some('\\') if chars.get(i + 1).is_some_and(char::is_ascii_alphabetic) => {
                        word(i + 1)
                    }
                    Some('\\') => (chars.get(i + 1).map_or(String::new(), char::to_string), i + 2),
                    Some(c) => (c.to_string(), i + 1),
                    None => (String::new(), i),
                };
                i = next;
                let Some(d) = delimiter_char(&delimiter) else {
                    return Err(Error::at(0, col, format!("unexpected `\\{name}{delimiter}`")));
                };
                out.push((if name == "left" { T::Open(d) } else { T::Close(d) }, col));
                continue;
            }
            out.push((
                match name.as_str() {
                    "lvert" => T::Open('|'),
                    "rvert" => T::Close('|'),
                    "lfloor" => T::Open('⌊'),
                    "rfloor" => T::Close('⌋'),
                    "lceil" => T::Open('⌈'),
                    "rceil" => T::Close('⌉'),
                    "{" | "lbrace" => T::Open('{'),
                    "}" | "rbrace" => T::Close('}'),
                    "vert" | "mid" | "|" => T::Ch('|'),
                    _ => T::Cmd(name),
                },
                col,
            ));
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if chars.get(j) == Some(&'.') && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            out.push((T::Num(chars[i..j].iter().collect()), col));
            i = j;
        } else {
            out.push((
                match c {
                    '(' | '[' => T::Open(c),
                    ')' | ']' => T::Close(c),
                    '·' | '×' => T::Cmd("cdot".into()),
                    '−' => T::Ch('-'),
                    _ => T::Ch(c),
                },
                col,
            ));
            i += 1;
        }
    }
    out.push((T::End, chars.len() + 1));
    Ok(out)
}

fn delimiter_char(written: &str) -> Option<char> {
    Some(match written {
        "(" | ")" | "[" | "]" | "|" | "." => written.chars().next()?,
        "{" | "lbrace" => '{',
        "}" | "rbrace" => '}',
        "vert" | "lvert" | "rvert" => '|',
        "lfloor" => '⌊',
        "rfloor" => '⌋',
        "lceil" => '⌈',
        "rceil" => '⌉',
        "langle" => '⟨',
        "rangle" => '⟩',
        _ => return None,
    })
}

// ---- Reading ----

/// What a formula is, as far as printing it back needs.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    Num(String),
    Name(String),
    /// `+`, `-`, `·`, `/`, `^`, or `neg` with one operand.
    Op(&'static str, Vec<Node>),
    Call(String, Vec<Node>),
    Tuple(Vec<Node>),
}

fn op(name: &'static str, args: Vec<Node>) -> Node {
    Node::Op(name, args)
}

const GREEK: [(&str, &str); 40] = [
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    // `\epsilon` is drawn ϵ and `\varepsilon` ε, but a keyboard types ε, so both are that name.
    ("epsilon", "ε"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "θ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("varrho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    // Likewise `\phi` is drawn ϕ, and both are the φ a keyboard types.
    ("phi", "φ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("ell", "ℓ"),
    // MathLive writes Euler's number and the imaginary unit as commands of their own.
    ("exponentialE", "e"),
];

/// LaTeX's named operators, and the few whose Pencil name differs. The rest keep their names,
/// so one Pencil lacks, as `arcsin`, is an unknown function with a suggestion, not a guess.
const OPERATORS: [&str; 31] = [
    "sin", "cos", "tan", "sec", "csc", "cot", "sinh", "cosh", "tanh", "coth", "sech", "csch",
    "arcsin", "arccos", "arctan", "arcsinh", "arccosh", "arctanh", "ln", "log", "lg", "exp",
    "arg", "min", "max", "gcd", "lcm", "det", "sgn", "Re", "Im",
];

fn pencil_function(latex: &str) -> &str {
    match latex {
        "sgn" => "sign",
        "Re" => "real",
        "Im" => "imag",
        other => other,
    }
}

struct Reader<'a> {
    toks: Vec<(T, usize)>,
    pos: usize,
    line: usize,
    functions: Vec<String>,
    known: &'a [&'a str],
    /// How many `|…|` are open, so a bare `|` closes one rather than opening another.
    bars: usize,
    /// Whether the line's first name, if parentheses follow it, is a function it defines.
    defining: bool,
}

impl<'a> Reader<'a> {
    fn new(line: &str, number: usize, known: &'a [&'a str]) -> Result<Self, Error> {
        let toks = lex(line).map_err(|e| Error::at(number, e.col, e.message))?;
        Ok(Reader { toks, pos: 0, line: number, functions: Vec::new(), known, bars: 0, defining: false })
    }

    fn peek(&self) -> &T {
        &self.toks[self.pos].0
    }

    fn bump(&mut self) -> T {
        let t = self.toks[self.pos].0.clone();
        if t != T::End {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, t: &T) -> bool {
        if self.peek() == t {
            self.bump();
            return true;
        }
        false
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::at(self.line, self.toks[self.pos].1, message)
    }

    fn expect(&mut self, t: T, what: &str) -> Result<(), Error> {
        if self.eat(&t) {
            return Ok(());
        }
        Err(self.error(format!("expected {what}, found {}", shown(self.peek()))))
    }

    /// A line: an expression, or two either side of `=`, as Desmos copies a definition. A name
    /// with parentheses that starts a line with `=` in it is the function the line defines, as
    /// `f\left(x\right)=x^{2}`, so it's called on the right too.
    fn line(mut self) -> Result<String, Error> {
        self.defining = self.toks.iter().any(|(t, _)| *t == T::Ch('='));
        let left = self.expr()?;
        self.defining = false;
        let text = if self.eat(&T::Ch('=')) {
            let right = self.expr()?;
            format!("{} = {}", print(&left).0, print(&right).0)
        } else {
            print(&left).0
        };
        match self.peek() {
            T::End => Ok(text),
            t if comparison(t) => Err(self.error(
                "a comparison has no Pencil form yet: Pencil's expressions are numbers and points",
            )),
            T::Ch(',') => Err(self.error("a list has no Pencil form yet; paste one formula per line")),
            other => Err(self.error(format!("expected the end, found {}", shown(other)))),
        }
    }

    fn is_function(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f == name)
            || self.known.contains(&name)
            || FUNCTIONS.iter().any(|(f, _)| subscripted(f) == name)
    }

    fn expr(&mut self) -> Result<Node, Error> {
        let mut left = self.term()?;
        loop {
            let name = match self.peek() {
                T::Ch('+') => "+",
                T::Ch('-') => "-",
                T::Cmd(c) if c == "pm" || c == "mp" => {
                    return Err(self.error("`±` has no Pencil form: write each sign as its own formula"));
                }
                _ => return Ok(left),
            };
            self.bump();
            left = op(name, vec![left, self.term()?]);
        }
    }

    fn term(&mut self) -> Result<Node, Error> {
        let mut left = self.signed()?;
        loop {
            let name = match self.peek() {
                T::Ch('*') => "·",
                T::Cmd(c) if c == "cdot" || c == "times" || c == "ast" => "·",
                T::Ch('/') => "/",
                T::Cmd(c) if c == "div" => "/",
                _ => return Ok(left),
            };
            self.bump();
            left = op(name, vec![left, self.signed()?]);
        }
    }

    fn signed(&mut self) -> Result<Node, Error> {
        if self.eat(&T::Ch('-')) {
            return Ok(op("neg", vec![self.signed()?]));
        }
        if self.eat(&T::Ch('+')) {
            return self.signed();
        }
        self.run(false)
    }

    /// Factors side by side, which multiply. As a function's argument written without
    /// parentheses, as `\sin 2x`, the run stops at the next function, so `\sin x\cos x` is
    /// sin(x)·cos(x).
    fn run(&mut self, argument: bool) -> Result<Node, Error> {
        let mut left = self.power()?;
        while self.starts_factor(argument) {
            left = op("·", vec![left, self.power()?]);
        }
        Ok(left)
    }

    fn starts_factor(&self, argument: bool) -> bool {
        match self.peek() {
            T::Num(_) | T::Open(_) => true,
            T::Ch('{') => true,
            T::Ch('|') => self.bars == 0,
            T::Ch(c) => c.is_alphabetic(),
            T::Cmd(c) => {
                let function = OPERATORS.contains(&c.as_str()) || c == "operatorname";
                !(argument && function)
                    && !matches!(
                        c.as_str(),
                        "cdot" | "times" | "ast" | "div" | "pm" | "mp" | "circ" | "degree"
                    )
                    && !comparison(self.peek())
            }
            _ => false,
        }
    }

    fn power(&mut self) -> Result<Node, Error> {
        let mut base = self.primary()?;
        loop {
            if self.eat(&T::Cmd("degree".into())) {
                base = op("·", vec![base, Node::Name("Degrees".into())]);
            } else if *self.peek() == T::Ch('^') {
                self.bump();
                if self.degrees_follow() {
                    base = op("·", vec![base, Node::Name("Degrees".into())]);
                    continue;
                }
                base = op("^", vec![base, self.argument()?]);
            } else if *self.peek() == T::Ch('_') {
                return Err(self.error("a subscript can only follow a name, as in `x_{1}`"));
            } else if matches!(self.peek(), T::Ch('!' | '\'')) {
                return Err(self.error(format!("{} has no Pencil form yet", shown(self.peek()))));
            } else {
                return Ok(base);
            }
        }
    }

    /// `^{\circ}` or `^\circ`, a degree sign, which is MathJSON's `Degrees`.
    fn degrees_follow(&mut self) -> bool {
        let circ = T::Cmd("circ".into());
        if self.eat(&circ) {
            return true;
        }
        if *self.peek() == T::Ch('{') && self.toks[self.pos + 1].0 == circ
            && self.toks.get(self.pos + 2).is_some_and(|t| t.0 == T::Ch('}'))
        {
            self.pos += 3;
            return true;
        }
        false
    }

    /// A command's argument: a group in braces, or one token, so `x^23` is x²·3 as in LaTeX.
    fn argument(&mut self) -> Result<Node, Error> {
        match self.peek().clone() {
            T::Ch('{') => {
                self.bump();
                let inner = self.expr()?;
                self.expect(T::Ch('}'), "`}`")?;
                Ok(inner)
            }
            T::Num(digits) => {
                let mut rest = digits.chars();
                let first = rest.next().unwrap();
                let rest: String = rest.collect();
                if rest.is_empty() {
                    self.bump();
                } else {
                    let col = self.toks[self.pos].1;
                    self.toks[self.pos] = (T::Num(rest), col + 1);
                }
                Ok(Node::Num(normal_number(&first.to_string())))
            }
            T::Ch(c) if c.is_alphabetic() => {
                self.bump();
                Ok(Node::Name(c.to_string()))
            }
            T::Cmd(_) => self.primary(),
            other => Err(self.error(format!("expected an argument, found {}", shown(&other)))),
        }
    }

    fn primary(&mut self) -> Result<Node, Error> {
        let token = self.peek().clone();
        match token {
            T::Num(digits) => {
                self.bump();
                Ok(Node::Num(normal_number(&digits)))
            }
            T::Ch(c) if c.is_alphabetic() => {
                let first = self.pos == 0;
                self.bump();
                let name = self.subscript(c.to_string())?;
                self.name_or_call(name, first)
            }
            T::Cmd(command) => self.command(&command),
            T::Open('(') => {
                self.bump();
                let first = self.expr()?;
                let node = if self.eat(&T::Ch(',')) {
                    let second = self.expr()?;
                    if *self.peek() == T::Ch(',') {
                        return Err(self.error("a point in Pencil has two coordinates"));
                    }
                    Node::Tuple(vec![first, second])
                } else {
                    first
                };
                self.expect(T::Close(')'), "`)`")?;
                Ok(node)
            }
            T::Open('|') | T::Ch('|') => {
                self.bump();
                self.bars += 1;
                let inner = self.expr();
                self.bars -= 1;
                let inner = inner?;
                if !self.eat(&T::Close('|')) {
                    self.expect(T::Ch('|'), "a closing `|`")?;
                }
                Ok(Node::Call("abs".into(), vec![inner]))
            }
            T::Open('⌊') => {
                self.bump();
                let inner = self.expr()?;
                self.expect(T::Close('⌋'), "`\\rfloor`")?;
                Ok(Node::Call("floor".into(), vec![inner]))
            }
            T::Open('⌈') => {
                self.bump();
                let inner = self.expr()?;
                self.expect(T::Close('⌉'), "`\\rceil`")?;
                Ok(Node::Call("ceil".into(), vec![inner]))
            }
            T::Ch('{') => {
                self.bump();
                let inner = self.expr()?;
                self.expect(T::Ch('}'), "`}`")?;
                Ok(inner)
            }
            T::Open('{') => Err(self.error(
                "Desmos's piecewise `{condition: value}` has no Pencil form yet",
            )),
            T::Open('[') => Err(self.error("a list has no Pencil form yet")),
            other if comparison(&other) => Err(self.error(
                "a comparison has no Pencil form yet: Pencil's expressions are numbers and points",
            )),
            other => Err(self.error(format!("expected an expression, found {}", shown(&other)))),
        }
    }

    fn command(&mut self, command: &str) -> Result<Node, Error> {
        let first = self.pos == 0;
        if let Some((_, letter)) = GREEK.iter().find(|(c, _)| *c == command) {
            self.bump();
            let name = self.subscript(letter.to_string())?;
            return self.name_or_call(name, first);
        }
        if OPERATORS.contains(&command) {
            self.bump();
            return self.function(pencil_function(command).to_string());
        }
        match command {
            "imaginaryI" => {
                self.bump();
                Ok(Node::Name("i".into()))
            }
            "operatorname" => {
                self.bump();
                let word = self.word()?;
                self.function(pencil_function(&word).to_string())
            }
            "mathrm" | "mathit" | "text" | "textrm" | "textit" | "mathbf" => {
                self.bump();
                let word = self.word()?;
                let name = self.subscript(word)?;
                self.name_or_call(name, first)
            }
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                self.bump();
                let top = self.argument()?;
                let bottom = self.argument()?;
                Ok(op("/", vec![top, bottom]))
            }
            "sqrt" => {
                self.bump();
                if matches!(self.peek(), T::Open('[')) {
                    return Err(self.error(
                        "an n-th root has no Pencil form yet: `x^(1/n)` differs from it for negative x",
                    ));
                }
                Ok(Node::Call("sqrt".into(), vec![self.argument()?]))
            }
            "infty" => Err(self.error("infinity has no Pencil form")),
            _ => Err(self.error(format!("`\\{command}` has no Pencil form yet"))),
        }
    }

    /// `_1` or `_{12}` after a name, which joins it: `\theta_{1}` is the name `θ_1`.
    fn subscript(&mut self, name: String) -> Result<String, Error> {
        if !self.eat(&T::Ch('_')) {
            return Ok(name);
        }
        let text = match self.peek().clone() {
            T::Ch('{') => self.word()?,
            T::Num(digits) => {
                // One token, as with a power: `x_12` is x₁·2.
                let first = digits.chars().next().unwrap().to_string();
                let rest: String = digits.chars().skip(1).collect();
                if rest.is_empty() {
                    self.bump();
                } else {
                    let col = self.toks[self.pos].1;
                    self.toks[self.pos] = (T::Num(rest), col + 1);
                }
                first
            }
            T::Ch(c) if c.is_alphanumeric() => {
                self.bump();
                c.to_string()
            }
            other => {
                return Err(self.error(format!("expected a subscript, found {}", shown(&other))));
            }
        };
        Ok(subscripted(&format!("{name}_{text}")))
    }

    /// Letters and digits in braces, as in `\operatorname{mod}` or `x_{12}`.
    fn word(&mut self) -> Result<String, Error> {
        self.expect(T::Ch('{'), "`{`")?;
        let mut word = String::new();
        loop {
            match self.peek().clone() {
                T::Ch('}') if !word.is_empty() => {
                    self.bump();
                    return Ok(word);
                }
                T::Ch(c) if c.is_alphanumeric() || c == '_' => word.push(c),
                T::Num(digits) => word.push_str(&digits),
                _ => return Err(self.error("expected letters and digits in braces")),
            }
            self.bump();
        }
    }

    /// A name, called if it's a function and parentheses follow it. `first` is whether it
    /// starts the line.
    fn name_or_call(&mut self, name: String, first: bool) -> Result<Node, Error> {
        if !matches!(self.peek(), T::Open('(')) {
            return Ok(Node::Name(name));
        }
        if first && self.defining {
            self.functions.push(name.clone());
        }
        if self.is_function(&name) {
            let args = self.arguments()?;
            return Ok(Node::Call(name, args));
        }
        Ok(Node::Name(name))
    }

    /// A function written as an operator, `\sin`, with its argument in parentheses or not:
    /// `\sin\left(x\right)`, `\sin x`, `\sin^{2}x`.
    fn function(&mut self, name: String) -> Result<Node, Error> {
        let mut power = None;
        if self.eat(&T::Ch('^')) {
            let p = self.argument()?;
            if p == op("neg", vec![Node::Num("1".into())]) {
                return Err(self.error(format!(
                    "`\\{name}^{{-1}}` is an inverse, not a power: write `\\arc{name}` or the inverse's name"
                )));
            }
            power = Some(p);
        }
        let call = if matches!(self.peek(), T::Open('(')) {
            Node::Call(name, self.arguments()?)
        } else if self.starts_factor(true) {
            Node::Call(name, vec![self.run(true)?])
        } else {
            return Err(self.error(format!("`{name}` needs an argument")));
        };
        Ok(match power {
            Some(p) => op("^", vec![call, p]),
            None => call,
        })
    }

    fn arguments(&mut self) -> Result<Vec<Node>, Error> {
        self.bump();
        let mut args = Vec::new();
        if self.eat(&T::Close(')')) {
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            if self.eat(&T::Close(')')) {
                return Ok(args);
            }
            self.expect(T::Ch(','), "`,` or `)`")?;
        }
    }
}

fn comparison(t: &T) -> bool {
    match t {
        T::Ch(c) => matches!(c, '<' | '>' | '≤' | '≥' | '≠'),
        T::Cmd(c) => matches!(
            c.as_str(),
            "le" | "leq" | "ge" | "geq" | "ne" | "neq" | "lt" | "gt" | "approx"
        ),
        _ => false,
    }
}

fn normal_number(digits: &str) -> String {
    if digits.starts_with('.') { format!("0{digits}") } else { digits.to_string() }
}

fn shown(t: &T) -> String {
    match t {
        T::Cmd(c) => format!("`\\{c}`"),
        T::Num(n) => format!("`{n}`"),
        T::Open(c) | T::Close(c) | T::Ch(c) => format!("`{c}`"),
        T::End => "the end".into(),
    }
}

// ---- Printing ----

/// Glyphs Pencil reads as a name by themselves, so a product can be written beside them.
const GLYPHS: [char; 4] = ['π', 'ℯ', 'ⅈ', 'ⅇ'];

// How tightly printed text holds together, as Pencil's grammar reads it.
const SUM: u8 = 1;
const TERM: u8 = 2;
const SIDE_BY_SIDE: u8 = 3;
const UNARY: u8 = 4;
const POWER: u8 = 5;
const ATOM: u8 = 6;

/// Pencil text for a node, and how tightly it holds together.
fn print(node: &Node) -> (String, u8) {
    let at_least = |node: &Node, min: u8| {
        let (text, prec) = print(node);
        if prec < min { format!("({text})") } else { text }
    };
    match node {
        Node::Num(n) => (n.clone(), ATOM),
        Node::Name(n) => (n.clone(), ATOM),
        Node::Call(f, args) => {
            let args: Vec<String> = args.iter().map(|a| print(a).0).collect();
            (format!("{f}({})", args.join(", ")), ATOM)
        }
        Node::Tuple(items) => {
            let items: Vec<String> = items.iter().map(|a| print(a).0).collect();
            (format!("({})", items.join(", ")), ATOM)
        }
        Node::Op(name, args) => match (*name, args.as_slice()) {
            ("+" | "-", [a, b]) => (format!("{} {name} {}", at_least(a, SUM), at_least(b, TERM)), SUM),
            ("·", [a, b]) => {
                if side_by_side(a, b) {
                    (format!("{}{}", print(a).0, print(b).0), SIDE_BY_SIDE)
                } else {
                    (format!("{}·{}", at_least(a, TERM), at_least(b, SIDE_BY_SIDE)), TERM)
                }
            }
            // A denominator that's a product is bracketed, as `1/(2π)`: Pencil would read
            // `1/2π` the same way, but a reader can't be sure.
            ("/", [a, b]) => (format!("{}/{}", at_least(a, TERM), at_least(b, UNARY)), TERM),
            ("^", [a, b]) => (format!("{}^{}", at_least(a, ATOM), at_least(b, UNARY)), POWER),
            ("neg", [a]) => {
                // -(a·b) and -(a/b) are (-a)·b and (-a)/b, so the sign can lead a term.
                let (text, prec) = print(a);
                match prec {
                    TERM | SIDE_BY_SIDE => (format!("-{text}"), TERM),
                    p if p >= UNARY => (format!("-{text}"), UNARY),
                    _ => (format!("-({text})"), UNARY),
                }
            }
            _ => unreachable!("{name} with {} operands", args.len()),
        },
    }
}

/// Whether `a b` can be written side by side, as `2x`, `2πr`, `iπ` or `2sin(x)`: the right is a
/// one-letter name, a call, or a power of one, and Pencil's lexer will still see two factors
/// (a number before a letter, or a glyph on either side). `x y` would be one name, `xy`.
fn side_by_side(a: &Node, b: &Node) -> bool {
    let letter = |n: &str| n.split('_').next().is_some_and(|stem| stem.chars().count() == 1);
    let right = match b {
        Node::Name(n) => letter(n),
        Node::Call(..) => true,
        Node::Op("^", parts) => match &parts[0] {
            Node::Name(n) => letter(n),
            Node::Call(..) => true,
            _ => false,
        },
        _ => false,
    };
    let (left_text, left_prec) = print(a);
    let right_text = print(b).0;
    let left = match a {
        Node::Num(_) => !right_text.starts_with("px") && !right_text.starts_with("em"),
        Node::Name(_) => true,
        Node::Op("·", _) => left_prec == SIDE_BY_SIDE,
        _ => false,
    };
    let glyph = left_text.ends_with(GLYPHS) || right_text.starts_with(GLYPHS);
    right && left && (matches!(a, Node::Num(_)) || glyph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_whole_expression;

    fn pencil(latex: &str) -> String {
        read(latex, &[]).unwrap_or_else(|e| panic!("{latex}: {e}"))
    }

    fn error(latex: &str) -> String {
        read(latex, &[]).unwrap_err().message
    }

    #[test]
    fn what_desmos_copies_reads_as_pencil() {
        assert_eq!(pencil(r"y=\frac{1}{2}x^{2}"), "y = 1/2·x^2");
        assert_eq!(pencil(r"f\left(x\right)=x^{2}+1"), "f(x) = x^2 + 1");
        assert_eq!(pencil(r"\sin\left(2x\right)"), "sin(2x)");
        assert_eq!(pencil(r"\left|z\right|^{2}"), "abs(z)^2");
        assert_eq!(pencil(r"\sqrt{x^{2}+y^{2}}"), "sqrt(x^2 + y^2)");
        assert_eq!(pencil(r"\operatorname{mod}\left(x,2\right)"), "mod(x, 2)");
        assert_eq!(pencil(r"\left(1,2\right)"), "(1, 2)");
        assert_eq!(pencil(r"a=3"), "a = 3");
        assert_eq!(pencil("a=3\n\nb=\\frac{a}{2}\n"), "a = 3\n\nb = a/2\n");
        // Only a name with parentheses defines a function: `xy=3` is still a product.
        assert_eq!(pencil(r"xy=3"), "x·y = 3");
        assert_eq!(pencil(r"g\left(t\right)=2g\left(t-1\right)"), "g(t) = 2g(t - 1)");
    }

    #[test]
    fn what_mathlive_copies_reads_as_pencil() {
        assert_eq!(pencil(r"$$\exponentialE^{\imaginaryI\pi}+1=0$$"), "e^(iπ) + 1 = 0");
        assert_eq!(pencil(r"\mathrm{e}^{x}"), "e^x");
        assert_eq!(pencil(r"\frac12"), "1/2");
        assert_eq!(pencil(r"30^{\circ}"), "30·Degrees");
        assert_eq!(pencil(r"\lvert z\rvert"), "abs(z)");
    }

    #[test]
    fn letters_run_together_are_a_product_and_subscripts_join_their_name() {
        assert_eq!(pencil(r"xy"), "x·y");
        assert_eq!(pencil(r"2\pi r"), "2πr");
        assert_eq!(pencil(r"\theta_{1}"), "θ_1");
        assert_eq!(pencil(r"x_{1}y"), "x_1·y");
        assert_eq!(pencil(r"x_12"), "x_1·2");
        assert_eq!(pencil(r"x^23"), "x^2·3");
        assert_eq!(pencil(r"\varphi+\phi"), "φ + φ");
    }

    #[test]
    fn a_letter_before_parentheses_is_a_call_only_if_its_a_function() {
        assert_eq!(pencil(r"a\left(x+1\right)"), "a·(x + 1)");
        assert_eq!(read(r"a\left(x+1\right)", &["a"]).unwrap(), "a(x + 1)");
        assert_eq!(pencil(r"\sin x\cos x"), "sin(x)·cos(x)");
        assert_eq!(pencil(r"2\sin x"), "2sin(x)");
        assert_eq!(pencil(r"\sin^{2}x+1"), "sin(x)^2 + 1");
        assert_eq!(pencil(r"\sin 2x+1"), "sin(2x) + 1");
    }

    #[test]
    fn brackets_go_where_pencil_needs_them() {
        assert_eq!(pencil(r"\frac{1}{2\pi}"), "1/(2π)");
        assert_eq!(pencil(r"-\frac{b}{2a}"), "-b/(2a)");
        assert_eq!(
            pencil(r"\frac{-b+\sqrt{b^{2}-4ac}}{2a}"),
            "(-b + sqrt(b^2 - 4a·c))/(2a)"
        );
        assert_eq!(pencil(r"\left(x+1\right)^{-1}"), "(x + 1)^-1");
        assert_eq!(pencil(r"\left(-2\right)^{2}"), "(-2)^2");
        assert_eq!(pencil(r"a-\left(b-c\right)"), "a - (b - c)");
        assert_eq!(pencil(r"x^{2n}"), "x^(2n)");
        assert_eq!(pencil(r".5x"), "0.5x");
    }

    /// Whatever is printed reads back, in Pencil, as the formula that was pasted.
    #[test]
    fn the_text_reads_back_as_the_formula() {
        let cases = [
            (r"\frac{1}{2}x^{2}", r#"["Multiply",["Divide",1.0,2.0],["Power","x",2.0]]"#),
            (r"\frac{1}{2\pi}", r#"["Divide",1.0,["Multiply",2.0,"π"]]"#),
            (r"e^{i\pi}", r#"["Power","e",["Multiply","i","π"]]"#),
            (r"2\pi r", r#"["Multiply",["Multiply",2.0,"π"],"r"]"#),
            (r"x^{2}y", r#"["Multiply",["Power","x",2.0],"y"]"#),
        ];
        for (latex, expected) in cases {
            let text = pencil(latex);
            let read = parse_whole_expression(&text).unwrap().to_mathjson().to_string();
            assert_eq!(read, expected, "{latex} printed {text}");
        }
    }

    /// The tree Pencil should read from a node's text: MathJSON, with built-ins by their heads, and
    /// a sign folded in the ways Pencil's parser folds it.
    fn expected(node: &Node) -> serde_json::Value {
        use serde_json::json;
        let negate = |inner: serde_json::Value| -> serde_json::Value {
            match inner {
                serde_json::Value::Number(n) => json!(-n.as_f64().unwrap()),
                serde_json::Value::Array(ref parts)
                    if parts[0] == "Multiply" || parts[0] == "Divide" =>
                {
                    let mut parts = parts.clone();
                    let first = parts[1].take();
                    parts[1] = match first {
                        serde_json::Value::Number(n) => json!(-n.as_f64().unwrap()),
                        other => json!(["Negate", other]),
                    };
                    serde_json::Value::Array(parts)
                }
                other => json!(["Negate", other]),
            }
        };
        match node {
            Node::Num(n) => json!(n.parse::<f64>().unwrap()),
            Node::Name(n) => json!(n),
            Node::Call(f, args) => {
                let head = FUNCTIONS.iter().find(|(w, _)| w == f).map_or(f.as_str(), |(_, h)| h);
                let mut parts = vec![json!(head)];
                parts.extend(args.iter().map(expected));
                serde_json::Value::Array(parts)
            }
            Node::Tuple(items) => {
                let mut parts = vec![json!("Tuple")];
                parts.extend(items.iter().map(expected));
                serde_json::Value::Array(parts)
            }
            Node::Op("neg", args) => negate(expected(&args[0])),
            Node::Op(name, args) => {
                let head = match *name {
                    "+" => "Add",
                    "-" => "Subtract",
                    "·" => "Multiply",
                    "/" => "Divide",
                    _ => "Power",
                };
                json!([head, expected(&args[0]), expected(&args[1])])
            }
        }
    }

    /// Every formula in these tests, printed, reads back in Pencil as the tree it was read as.
    #[test]
    fn every_printed_formula_reads_back_as_its_tree() {
        let formulas = [
            r"\frac{1}{2}x^{2}", r"\sin\left(2x\right)", r"\left|z\right|^{2}", r"\sqrt{x^{2}+y^{2}}",
            r"\operatorname{mod}\left(x,2\right)", r"\left(1,2\right)", r"\exponentialE^{\imaginaryI\pi}+1",
            r"\frac12", r"30^{\circ}", r"xy", r"2\pi r", r"\theta_{1}", r"x_{1}y", r"x_12", r"x^23",
            r"a\left(x+1\right)", r"\sin x\cos x", r"2\sin x", r"\sin^{2}x+1", r"\sin 2x+1",
            r"\frac{1}{2\pi}", r"-\frac{b}{2a}", r"\frac{-b+\sqrt{b^{2}-4ac}}{2a}", r"\left(x+1\right)^{-1}",
            r"\left(-2\right)^{2}", r"a-\left(b-c\right)", r"x^{2n}", r".5x", r"-2x^{2}-x",
            r"\frac{a}{b}\cdot\frac{c}{d}", r"a\div b\div c", r"a-b-c", r"a-\left(-b\right)",
            r"2^{-x^{2}}", r"-\left(a+b\right)c", r"\pi_{1}r", r"\frac{x}{y}z", r"e^{-\frac{x^{2}}{2}}",
        ];
        for latex in formulas {
            let mut reader = Reader::new(latex, 1, &[]).unwrap();
            let node = reader.expr().unwrap();
            let text = print(&node).0;
            let read = parse_whole_expression(&text)
                .unwrap_or_else(|e| panic!("{latex} printed {text}, which doesn't parse: {e}"));
            assert_eq!(read.to_mathjson(), expected(&node), "{latex} printed {text}");
        }
    }

    #[test]
    fn what_has_no_pencil_form_says_so() {
        assert!(error(r"\left\{x<0:-x,x\right\}").contains("piecewise"));
        assert!(error(r"x\le1").contains("comparison"));
        assert!(error(r"\sqrt[3]{x}").contains("n-th root"));
        assert!(error(r"\sin^{-1}x").contains("inverse"));
        let e = read("x\n2\\foo", &[]).unwrap_err();
        assert_eq!((e.line, e.col), (2, 2), "{e}");
    }

    #[test]
    fn pencil_text_isnt_mistaken_for_latex() {
        assert!(is_latex(r"\frac{1}{2}"));
        assert!(is_latex("x^{2}"));
        assert!(is_latex("$x$"));
        assert!(!is_latex("let a = 2·π\nlet b = x_{1}"));
    }
}
