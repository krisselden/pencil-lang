//! A small hand-written parser. The syntax is expected to change, so it stays
//! cheap to rewrite; everything downstream only sees `Expr` and `Item`.

use crate::error::Error;
use crate::ir::Expr;

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub name: Option<String>,
    pub items: Vec<Item>,
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
        text: String,
        at: Expr,
        offset: Option<Expr>,
        line: usize,
    },
    Viewport {
        x: [Expr; 2],
        y: [Expr; 2],
        width: Option<Expr>,
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
    pub target: String,
    pub facet: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Px,
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
            let unit = chars.get(i) == Some(&'p')
                && chars.get(i + 1) == Some(&'x')
                && !chars
                    .get(i + 2)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_');
            if unit {
                out.push(Lexed {
                    tok: Tok::Px,
                    line,
                    col,
                });
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
/// its own name for `compile` to resolve as a user function.
const FUNCTIONS: [(&str, &str); 21] = [
    ("sqrt", "Sqrt"),
    ("sin", "Sin"),
    ("cos", "Cos"),
    ("tan", "Tan"),
    ("exp", "Exp"),
    ("ln", "Ln"),
    ("abs", "Abs"),
    ("sign", "Sign"),
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
    ("length", "Length"),
    ("dist", "Distance"),
    ("dot", "Dot"),
];

impl Parser {
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
            let line = self.line();
            let word = self.ident("a statement")?;
            match word.as_str() {
                "doc" => {
                    let mut full = self.ident("a document name")?;
                    while self.eat("-") {
                        full.push('-');
                        full.push_str(&self.ident("a document name")?);
                    }
                    name = Some(full);
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
                "viewport" => items.push(self.viewport()?),
                "style" => items.push(Item::Style(self.style(line)?)),
                other => {
                    return Err(Error::at(line, 1, format!("unknown statement `{other}`")));
                }
            }
        }
        Ok(Document { name, items })
    }

    fn expr_list(&mut self) -> Result<Vec<Expr>, Error> {
        let mut out = vec![self.expr()?];
        while self.eat(",") {
            out.push(self.expr()?);
        }
        Ok(out)
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
            other => {
                return Err(Error::at(line, 1, format!("unknown curve form `{other}`")));
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
        self.keyword("at")?;
        let at = self.expr()?;
        let offset = if matches!(self.peek(), Tok::Ident(w) if w == "offset") {
            self.bump();
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Item::Label {
            name,
            text,
            at,
            offset,
            line,
        })
    }

    fn viewport(&mut self) -> Result<Item, Error> {
        if matches!(self.peek(), Tok::Ident(_)) {
            self.bump();
        }
        self.expect("{")?;
        let (mut x, mut y, mut width) = (None, None, None);
        while !self.eat("}") {
            let axis = self.ident("`x` or `y`")?;
            self.expect(":")?;
            if axis == "width" {
                width = Some(self.expr()?);
                if !self.eat(",") {
                    self.eat(";");
                }
                continue;
            }
            let lo = self.expr()?;
            self.expect("..")?;
            let hi = self.expr()?;
            match axis.as_str() {
                "x" => x = Some([lo, hi]),
                "y" => y = Some([lo, hi]),
                other => return Err(self.error(format!("unknown viewport axis `{other}`"))),
            }
            if !self.eat(",") {
                self.eat(";");
            }
        }
        match (x, y) {
            (Some(x), Some(y)) => Ok(Item::Viewport { x, y, width }),
            _ => Err(self.error("a viewport needs both `x` and `y`")),
        }
    }

    fn style(&mut self, line: usize) -> Result<StyleRule, Error> {
        let mut selectors = Vec::new();
        loop {
            let target = self.ident("a selector")?;
            let facet = if self.eat("::") {
                Some(self.ident("a facet")?)
            } else {
                None
            };
            selectors.push(Selector { target, facet });
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
                x: [Expr::Num(-2.0), Expr::Num(2.0)],
                y: [Expr::Num(-1.5), Expr::Num(1.5)],
                width: None,
            }
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
