//! Symbolic differentiation over the IR, which is what lets a shader get an exact
//! gradient for its anti-aliasing rather than a screen-space approximation.

use crate::error::Error;
use crate::ir::Expr;
use Expr::{App, Num, Sym};

fn app(head: &str, args: Vec<Expr>) -> Expr {
    fold(Expr::app(head, args))
}

fn add(a: Expr, b: Expr) -> Expr {
    app("Add", vec![a, b])
}

fn sub(a: Expr, b: Expr) -> Expr {
    app("Subtract", vec![a, b])
}

fn mul(a: Expr, b: Expr) -> Expr {
    app("Multiply", vec![a, b])
}

fn div(a: Expr, b: Expr) -> Expr {
    app("Divide", vec![a, b])
}

fn power(a: Expr, n: f64) -> Expr {
    app("Power", vec![a, Num(n)])
}

fn neg(a: Expr) -> Expr {
    app("Negate", vec![a])
}

/// The only simplification: folding identities, so gradients stay readable.
fn fold(e: Expr) -> Expr {
    let App(head, args) = &e else { return e };
    match (head.as_str(), args.as_slice()) {
        ("Add", [Num(a), Num(b)]) => Num(a + b),
        ("Add", [Num(z), x]) | ("Add", [x, Num(z)]) if *z == 0.0 => x.clone(),
        ("Subtract", [Num(a), Num(b)]) => Num(a - b),
        ("Subtract", [x, Num(z)]) if *z == 0.0 => x.clone(),
        ("Subtract", [Num(z), x]) if *z == 0.0 => neg(x.clone()),
        ("Multiply", [Num(a), Num(b)]) => Num(a * b),
        ("Multiply", [Num(z), _]) | ("Multiply", [_, Num(z)]) if *z == 0.0 => Num(0.0),
        ("Multiply", [Num(o), x]) | ("Multiply", [x, Num(o)]) if *o == 1.0 => x.clone(),
        ("Divide", [Num(z), _]) if *z == 0.0 => Num(0.0),
        ("Divide", [x, Num(o)]) if *o == 1.0 => x.clone(),
        ("Negate", [Num(a)]) => Num(-a),
        ("Power", [x, Num(o)]) if *o == 1.0 => x.clone(),
        ("Power", [_, Num(z)]) if *z == 0.0 => Num(1.0),
        _ => e,
    }
}

/// `∂e/∂var`, treating every other symbol as constant.
pub fn derivative(e: &Expr, var: &str) -> Result<Expr, Error> {
    let d = |x: &Expr| derivative(x, var);
    Ok(match e {
        Num(_) => Num(0.0),
        Sym(s) => Num(if s == var { 1.0 } else { 0.0 }),
        App(head, args) => match (head.as_str(), args.as_slice()) {
            ("Add", [a, b]) => add(d(a)?, d(b)?),
            ("Subtract", [a, b]) => sub(d(a)?, d(b)?),
            ("Negate", [a]) => neg(d(a)?),
            ("Multiply", [a, b]) => add(mul(d(a)?, b.clone()), mul(a.clone(), d(b)?)),
            ("Divide", [a, b]) => div(
                sub(mul(d(a)?, b.clone()), mul(a.clone(), d(b)?)),
                power(b.clone(), 2.0),
            ),
            ("Power", [a, Num(n)]) => mul(mul(Num(*n), power(a.clone(), n - 1.0)), d(a)?),
            ("Power", [a, b]) => mul(
                e.clone(),
                add(
                    mul(d(b)?, app("Ln", vec![a.clone()])),
                    div(mul(b.clone(), d(a)?), a.clone()),
                ),
            ),
            ("Sqrt", [a]) => div(d(a)?, mul(Num(2.0), e.clone())),
            ("Sin", [a]) => mul(app("Cos", vec![a.clone()]), d(a)?),
            ("Cos", [a]) => neg(mul(app("Sin", vec![a.clone()]), d(a)?)),
            ("Tan", [a]) => div(d(a)?, power(app("Cos", vec![a.clone()]), 2.0)),
            ("Exp", [a]) => mul(e.clone(), d(a)?),
            // Min and max follow whichever side is active, using the two-sided identity.
            ("Min", [a, b]) | ("Max", [a, b]) => {
                let sign = if head == "Max" { 1.0 } else { -1.0 };
                let swing = mul(
                    app("Sign", vec![sub(a.clone(), b.clone())]),
                    sub(d(a)?, d(b)?),
                );
                div(add(add(d(a)?, d(b)?), mul(Num(sign), swing)), Num(2.0))
            }
            ("Ln", [a]) => div(d(a)?, a.clone()),
            ("Abs", [a]) => mul(app("Sign", vec![a.clone()]), d(a)?),
            ("Sign", [_]) | ("Floor", [_]) => Num(0.0),
            ("Arccos", [a]) => neg(div(
                d(a)?,
                app("Sqrt", vec![sub(Num(1.0), power(a.clone(), 2.0))]),
            )),
            ("Arctan2", [y, x]) => div(
                sub(mul(x.clone(), d(y)?), mul(y.clone(), d(x)?)),
                add(power(x.clone(), 2.0), power(y.clone(), 2.0)),
            ),
            _ => return Err(Error::new(format!("can't differentiate `{head}` yet"))),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{self, Env};
    use crate::parse::{Item, parse};

    fn expr(src: &str) -> Expr {
        match &parse(&format!("let v = {src}")).unwrap().items[0] {
            Item::Let { expr, .. } => expr.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn folds_to_the_readable_form() {
        assert_eq!(derivative(&expr("x^2"), "x").unwrap(), expr("2 * x"));
        assert_eq!(derivative(&expr("y^2"), "x").unwrap(), Num(0.0));
    }

    /// The symbolic derivative must agree with a central difference.
    #[test]
    fn matches_finite_differences() {
        let cases = [
            "x^3 - 2*x + 1",
            "sin(x) * cos(2*x)",
            "tan(x / 3)",
            "exp(x) / (1 + x^2)",
            "ln(x + 3)",
            "sqrt(x^2 + 1)",
            "abs(x - 5)",
            "atan2(x, 2)",
            "atan2(1, x + 3)",
            "(x + 2) ^ (x / 2)",
            "2 ^ x",
            "-x^2",
            "acos(x / 3)",
            "max(x^2, 1 - x)",
            "min(x^2, 1 - x)",
        ];
        let at = |e: &Expr, x: f64| {
            let mut env = Env::new(0.0);
            env.set("x", x);
            eval::real(e, &env).unwrap()
        };
        for src in cases {
            let e = expr(src);
            let d = derivative(&e, "x").unwrap();
            for x in [-1.3, -0.4, 0.7, 1.9] {
                let h = 1e-5;
                let fd = (at(&e, x + h) - at(&e, x - h)) / (2.0 * h);
                let sym = at(&d, x);
                assert!((fd - sym).abs() < 1e-6, "{src} at {x}: {sym} vs {fd}");
            }
        }
    }

    #[test]
    fn unknown_heads_fail_loudly() {
        let err = derivative(&Expr::app("Frob", vec![Expr::sym("x")]), "x").unwrap_err();
        assert!(err.message.contains("Frob"));
    }
}
