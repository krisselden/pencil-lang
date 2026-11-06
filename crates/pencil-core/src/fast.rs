//! A faster way to evaluate the same expressions, for the loops that run one thousands of times
//! a frame: a curve's samples and a collection's instances. Names are resolved to slots and
//! functions to a node up front, so nothing is looked up or compared per evaluation.
//!
//! `eval` stays the definition. This has to agree with it to the bit, which a test holds it to
//! on every expression in the corpus, and anything it doesn't know how to do it leaves to `eval`.

use std::collections::HashMap;

use crate::eval::{self, Env};
use crate::ir::Expr;

#[derive(Clone, Debug)]
pub enum Node {
    Num(f64),
    Slot(u32),
    Negate(Box<Node>),
    Add(Box<[Node; 2]>),
    Subtract(Box<[Node; 2]>),
    Multiply(Box<[Node; 2]>),
    Divide(Box<[Node; 2]>),
    Power(Box<[Node; 2]>),
    Min(Box<[Node; 2]>),
    Max(Box<[Node; 2]>),
    Arctan2(Box<[Node; 2]>),
    Mod(Box<[Node; 2]>),
    Sqrt(Box<Node>),
    Sin(Box<Node>),
    Cos(Box<Node>),
    Tan(Box<Node>),
    Arccos(Box<Node>),
    Floor(Box<Node>),
    Exp(Box<Node>),
    Ln(Box<Node>),
    Abs(Box<Node>),
    Sign(Box<Node>),
    IfNonnegative(Box<[Node; 3]>),
    Clamp(Box<[Node; 3]>),
    Mix(Box<[Node; 3]>),
    Where(Box<[Node; 2]>),
    Exists(Vec<Node>),
}

/// Why there's no number. Only `Unknown` needs `eval`, to say what's wrong.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    Absent,
    Unknown,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Known,
    Absent,
    Unknown,
}

/// The values a compiled expression reads.
pub struct Slots {
    index: HashMap<String, u32>,
    values: Vec<f64>,
    states: Vec<State>,
}

impl Slots {
    /// Takes names' values from `env` as they're first compiled.
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            values: Vec::new(),
            states: Vec::new(),
        }
    }

    fn slot(&mut self, name: &str, env: &Env) -> u32 {
        if let Some(slot) = self.index.get(name) {
            return *slot;
        }
        let slot = self.values.len() as u32;
        self.index.insert(name.to_string(), slot);
        match env.get(name) {
            Some(value) => {
                self.values.push(value);
                self.states.push(State::Known);
            }
            None => {
                self.values.push(0.0);
                self.states.push(if env.is_absent(name) {
                    State::Absent
                } else {
                    State::Unknown
                });
            }
        }
        slot
    }

    /// A variable the loop will set, such as a curve's parameter.
    pub fn variable(&mut self, name: &str) -> u32 {
        let slot = self.index.get(name).copied().unwrap_or_else(|| {
            let slot = self.values.len() as u32;
            self.index.insert(name.to_string(), slot);
            self.values.push(0.0);
            self.states.push(State::Known);
            slot
        });
        self.states[slot as usize] = State::Known;
        slot
    }

    pub fn set(&mut self, slot: u32, value: f64) {
        self.values[slot as usize] = value;
    }
}

impl Default for Slots {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolves an expression, or says it's one for `eval`.
pub fn compile(e: &Expr, slots: &mut Slots, env: &Env) -> Option<Node> {
    Some(match e {
        Expr::Num(n) => Node::Num(*n),
        Expr::Sym(s) => match s.as_str() {
            "pi" | "π" => Node::Num(std::f64::consts::PI),
            "tau" => Node::Num(std::f64::consts::TAU),
            "white" | "black" => return None,
            name => Node::Slot(slots.slot(name, env)),
        },
        Expr::App(head, args) => {
            let mut go = |x: &Expr| compile(x, slots, env);
            match (head.as_str(), args.as_slice()) {
                ("Add", [a, b]) => Node::Add(Box::new([go(a)?, go(b)?])),
                ("Subtract", [a, b]) => Node::Subtract(Box::new([go(a)?, go(b)?])),
                ("Multiply", [a, b]) => Node::Multiply(Box::new([go(a)?, go(b)?])),
                ("Divide", [a, b]) => Node::Divide(Box::new([go(a)?, go(b)?])),
                ("Power", [a, b]) => Node::Power(Box::new([go(a)?, go(b)?])),
                ("Min", [a, b]) => Node::Min(Box::new([go(a)?, go(b)?])),
                ("Max", [a, b]) => Node::Max(Box::new([go(a)?, go(b)?])),
                ("Arctan2", [a, b]) => Node::Arctan2(Box::new([go(a)?, go(b)?])),
                ("Mod", [a, b]) => Node::Mod(Box::new([go(a)?, go(b)?])),
                ("Negate", [a]) => Node::Negate(Box::new(go(a)?)),
                ("Sqrt", [a]) => Node::Sqrt(Box::new(go(a)?)),
                ("Sin", [a]) => Node::Sin(Box::new(go(a)?)),
                ("Cos", [a]) => Node::Cos(Box::new(go(a)?)),
                ("Tan", [a]) => Node::Tan(Box::new(go(a)?)),
                ("Arccos", [a]) => Node::Arccos(Box::new(go(a)?)),
                ("Floor", [a]) => Node::Floor(Box::new(go(a)?)),
                ("Exp", [a]) => Node::Exp(Box::new(go(a)?)),
                ("Ln", [a]) => Node::Ln(Box::new(go(a)?)),
                ("Abs", [a]) => Node::Abs(Box::new(go(a)?)),
                ("Sign", [a]) => Node::Sign(Box::new(go(a)?)),
                ("IfNonnegative", [x, a, b]) => {
                    Node::IfNonnegative(Box::new([go(x)?, go(a)?, go(b)?]))
                }
                ("Clamp", [x, lo, hi]) => Node::Clamp(Box::new([go(x)?, go(lo)?, go(hi)?])),
                ("Where", [c, v]) => Node::Where(Box::new([go(c)?, go(v)?])),
                ("Exists", fields) => {
                    Node::Exists(fields.iter().map(&mut go).collect::<Option<Vec<_>>>()?)
                }
                ("Mix", [a, b, t]) => {
                    // Only for numbers: this mixing colors is `eval`'s.
                    let (a, b, t) = (go(a)?, go(b)?, go(t)?);
                    Node::Mix(Box::new([a, b, t]))
                }
                _ => return None,
            }
        }
    })
}

/// Runs a resolved expression. Keep in step with `eval::eval`.
pub fn run(node: &Node, slots: &Slots) -> Result<f64, Stop> {
    let go = |n: &Node| run(n, slots);
    Ok(match node {
        Node::Num(n) => *n,
        Node::Slot(i) => {
            let i = *i as usize;
            match slots.states[i] {
                State::Known => slots.values[i],
                State::Absent => return Err(Stop::Absent),
                State::Unknown => return Err(Stop::Unknown),
            }
        }
        Node::Negate(a) => -go(a)?,
        Node::Add(p) => go(&p[0])? + go(&p[1])?,
        Node::Subtract(p) => go(&p[0])? - go(&p[1])?,
        Node::Multiply(p) => go(&p[0])? * go(&p[1])?,
        Node::Divide(p) => go(&p[0])? / go(&p[1])?,
        Node::Power(p) => go(&p[0])?.powf(go(&p[1])?),
        Node::Min(p) => go(&p[0])?.min(go(&p[1])?),
        Node::Max(p) => go(&p[0])?.max(go(&p[1])?),
        Node::Arctan2(p) => go(&p[0])?.atan2(go(&p[1])?),
        Node::Mod(p) => {
            let (x, y) = (go(&p[0])?, go(&p[1])?);
            x - y * (x / y).floor()
        }
        Node::Sqrt(a) => go(a)?.sqrt(),
        Node::Sin(a) => go(a)?.sin(),
        Node::Cos(a) => go(a)?.cos(),
        Node::Tan(a) => go(a)?.tan(),
        Node::Arccos(a) => go(a)?.acos(),
        Node::Floor(a) => go(a)?.floor(),
        Node::Exp(a) => go(a)?.exp(),
        Node::Ln(a) => go(a)?.ln(),
        Node::Abs(a) => go(a)?.abs(),
        Node::Sign(a) => {
            let v = go(a)?;
            if v == 0.0 { 0.0 } else { v.signum() }
        }
        Node::IfNonnegative(p) => {
            if go(&p[0])? >= 0.0 {
                go(&p[1])?
            } else {
                go(&p[2])?
            }
        }
        Node::Clamp(p) => go(&p[0])?.max(go(&p[1])?).min(go(&p[2])?),
        Node::Mix(p) => {
            let t = go(&p[2])?;
            let (a, b) = (go(&p[0])?, go(&p[1])?);
            a + (b - a) * t
        }
        Node::Where(p) => {
            if go(&p[0])? >= 0.0 {
                go(&p[1])?
            } else {
                return Err(Stop::Absent);
            }
        }
        Node::Exists(fields) => {
            let mut all = 1.0;
            for field in fields {
                match go(field) {
                    Ok(_) => {}
                    Err(Stop::Absent) => all = 0.0,
                    Err(stop) => return Err(stop),
                }
            }
            all
        }
    })
}

/// Whether `eval` and this agree on an expression here, to the bit. For tests.
pub fn agrees(e: &Expr, env: &Env) -> Result<(), String> {
    let mut slots = Slots::new();
    let Some(node) = compile(e, &mut slots, env) else {
        return Ok(());
    };
    let slow = eval::real(e, env);
    let fast = run(&node, &slots);
    match (slow, fast) {
        (Ok(a), Ok(b)) if a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()) => Ok(()),
        (Err(a), Err(Stop::Absent)) if a.absent => Ok(()),
        (Err(a), Err(Stop::Unknown)) if !a.absent => Ok(()),
        // `Mix` on colors isn't a number to `eval`, which the type checker keeps from here.
        (Err(a), _) if a.message.contains("found a color") => Ok(()),
        (slow, fast) => Err(format!(
            "eval says {slow:?}, and this says {fast:?}, for {e:?}"
        )),
    }
}
