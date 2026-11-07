//! A faster way to evaluate the same expressions, for the loops that run one thousands of times
//! a frame: a curve's samples and a collection's instances. Names are resolved to slots and
//! functions to a node up front, so nothing is looked up or compared per evaluation. A piece that
//! appears more than once is worked out once per sample, which matters because inlining functions
//! copies their arguments: `coth` of something is that something many times over.
//!
//! `eval` stays the definition. This has to agree with it to the bit, which a test holds it to
//! on every expression in the corpus, and anything it doesn't know how to do it leaves to `eval`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::eval::{self, Env};
use crate::ir::Expr;

#[derive(Clone, Debug)]
pub enum Node {
    /// One piece, however many places it's used, remembered until a value changes.
    Shared(u32, Rc<Node>),
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
    /// What's been compiled so far, so equal pieces become one.
    pieces: HashMap<PieceKey, (u32, Node)>,
    next_piece: u32,
    /// Each shared piece's value, and the generation it's from. Setting a value starts a new one.
    remembered: Vec<(Cell<u64>, Cell<f64>)>,
    generation: Cell<u64>,
}

/// A piece, by what it is: a head and the pieces it takes, or a number or a name.
#[derive(PartialEq, Eq, Hash)]
enum PieceKey {
    Number(u64),
    Name(String),
    App(String, Vec<u32>),
}

impl Slots {
    /// Takes names' values from `env` as they're first compiled.
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            values: Vec::new(),
            states: Vec::new(),
            pieces: HashMap::new(),
            next_piece: 0,
            remembered: Vec::new(),
            generation: Cell::new(1),
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
        self.generation.set(self.generation.get() + 1);
    }

    /// The piece this key names, or this node as a new one. A piece of leaves is cheaper to work
    /// out than to remember.
    fn piece(&mut self, key: PieceKey, node: Node) -> (Node, u32) {
        if let Some((id, known)) = self.pieces.get(&key) {
            return (known.clone(), *id);
        }
        let id = self.next_piece;
        self.next_piece += 1;
        let node = if node.is_small() {
            node
        } else {
            let slot = self.remembered.len() as u32;
            self.remembered.push((Cell::new(0), Cell::new(0.0)));
            Node::Shared(slot, Rc::new(node))
        };
        self.pieces.insert(key, (id, node.clone()));
        (node, id)
    }
}

impl Default for Slots {
    fn default() -> Self {
        Self::new()
    }
}

impl Node {
    fn is_leaf(&self) -> bool {
        matches!(self, Node::Num(_) | Node::Slot(_))
    }

    /// One operation on leaves.
    fn is_small(&self) -> bool {
        match self {
            Node::Num(_) | Node::Slot(_) => true,
            Node::Negate(a) | Node::Abs(a) | Node::Sign(a) | Node::Floor(a) => a.is_leaf(),
            Node::Add(p) | Node::Subtract(p) | Node::Multiply(p) | Node::Divide(p) => {
                p.iter().all(Node::is_leaf)
            }
            _ => false,
        }
    }
}

/// Resolves an expression, or says it's one for `eval`.
pub fn compile(e: &Expr, slots: &mut Slots, env: &Env) -> Option<Node> {
    compile_piece(e, slots, env).map(|(node, _)| node)
}

fn compile_piece(e: &Expr, slots: &mut Slots, env: &Env) -> Option<(Node, u32)> {
    Some(match e {
        Expr::Num(n) => slots.piece(PieceKey::Number(n.to_bits()), Node::Num(*n)),
        Expr::Sym(s) => match s.as_str() {
            "pi" | "π" => slots.piece(
                PieceKey::Number(std::f64::consts::PI.to_bits()),
                Node::Num(std::f64::consts::PI),
            ),
            "tau" => slots.piece(
                PieceKey::Number(std::f64::consts::TAU.to_bits()),
                Node::Num(std::f64::consts::TAU),
            ),
            "white" | "black" => return None,
            name => {
                let slot = Node::Slot(slots.slot(name, env));
                slots.piece(PieceKey::Name(name.to_string()), slot)
            }
        },
        Expr::App(head, args) => {
            let mut ids = Vec::with_capacity(args.len());
            let node = {
                let ids = &mut ids;
                let mut go = |x: &Expr| {
                    let (node, id) = compile_piece(x, slots, env)?;
                    ids.push(id);
                    Some(node)
                };
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
            };
            slots.piece(PieceKey::App(head.clone(), ids), node)
        }
    })
}

/// Runs a resolved expression. Keep in step with `eval::eval`.
pub fn run(node: &Node, slots: &Slots) -> Result<f64, Stop> {
    let go = |n: &Node| run(n, slots);
    Ok(match node {
        Node::Shared(slot, inner) => {
            let (from, value) = &slots.remembered[*slot as usize];
            if from.get() == slots.generation.get() {
                value.get()
            } else {
                let result = run(inner, slots)?;
                from.set(slots.generation.get());
                value.set(result);
                result
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_expression;

    /// A remembered piece can't outlive the values it came from, and it's one piece wherever
    /// it's used, across the expressions compiled together.
    #[test]
    fn a_shared_piece_follows_its_values() {
        let env = Env::new(0.0);
        let (a, b) = (
            parse_expression("(t + 1)·(t + 2)·exp(t + 1) + sin(t)·sin(t)").unwrap(),
            parse_expression("exp(t + 1) - (t + 1)·(t + 2)").unwrap(),
        );
        let mut slots = Slots::new();
        let var = slots.variable("t");
        let (x, y) = (
            compile(&a, &mut slots, &env).unwrap(),
            compile(&b, &mut slots, &env).unwrap(),
        );
        assert!(!slots.remembered.is_empty(), "nothing was shared");
        for t in [0.5f64, -1.25, 0.5, 3.0, 3.0, 0.0] {
            slots.set(var, t);
            let want_x = (t + 1.0) * (t + 2.0) * (t + 1.0).exp() + t.sin() * t.sin();
            let want_y = (t + 1.0).exp() - (t + 1.0) * (t + 2.0);
            // In either order, and more than once.
            assert_eq!(
                run(&y, &slots).unwrap().to_bits(),
                want_y.to_bits(),
                "y at {t}"
            );
            assert_eq!(
                run(&x, &slots).unwrap().to_bits(),
                want_x.to_bits(),
                "x at {t}"
            );
            assert_eq!(
                run(&x, &slots).unwrap().to_bits(),
                want_x.to_bits(),
                "x again at {t}"
            );
        }
    }

    /// Something that doesn't exist isn't remembered as a value.
    #[test]
    fn a_shared_piece_can_stop_existing() {
        let env = Env::new(0.0);
        let e = Expr::app(
            "Add",
            vec![
                Expr::app(
                    "Where",
                    vec![Expr::sym("t"), Expr::app("Sin", vec![Expr::sym("t")])],
                ),
                Expr::app(
                    "Where",
                    vec![Expr::sym("t"), Expr::app("Sin", vec![Expr::sym("t")])],
                ),
            ],
        );
        let mut slots = Slots::new();
        let var = slots.variable("t");
        let node = compile(&e, &mut slots, &env).unwrap();
        slots.set(var, 1.0);
        assert_eq!(run(&node, &slots), Ok(2.0 * 1f64.sin()));
        slots.set(var, -1.0);
        assert_eq!(run(&node, &slots), Err(Stop::Absent));
        slots.set(var, 1.0);
        assert_eq!(run(&node, &slots), Ok(2.0 * 1f64.sin()));
    }
}
