//! An expression whose states each say `on` which beat they're reached: what it lays out, next
//! to the same states written one per beat, and what it says when a state names the wrong beat.

use pencil_core::frame::{Inputs, Label, prepare};
use pencil_core::{Figure, build};

const HEAD: &str = "viewport { x: 0..400, y: 0..400 } style figure { width: 400px; height: 400px }
pause 0.5
beat write for 1
beat wait for 1
pause 0.5
beat move for 1
beat gone for 0.5
place left = (100, 200)
place right = (300, 200)
";

const NAMED: &str = "expression e {
  on write: \"a + b\" at left
  on move: \"+ b\" at right
  on gone: empty
}";

/// The same, whole, a state for each beat: the hold through `wait` written again, and what
/// `move` leaves at `left` written out.
const ROWS: &str = "expression e by beat {
  \"a + b\" at left
  \"a + b\" at left
  \"a\" at left, \"+ b\" at right
  empty
}";

fn labels(figure: &Figure, time: f64) -> Vec<Label> {
    prepare(figure, &Inputs::at(time), 400, 400, 1.0)
        .unwrap()
        .labels
}

/// Every label of both at every sampled time: which are drawn, where, and how strongly.
fn assert_same(a: &Figure, b: &Figure, duration: f64) {
    let frames = (duration * 120.0).round() as usize;
    for k in 0..=frames {
        let time = k as f64 / 120.0;
        let (la, lb) = (labels(a, time), labels(b, time));
        let seen = |l: &[Label]| -> Vec<(String, f64, f64, f64)> {
            l.iter()
                .map(|l| (l.id.clone(), l.x, l.y, l.opacity))
                .collect()
        };
        assert_eq!(seen(&la), seen(&lb), "at {time} s");
    }
}

#[test]
fn a_state_on_a_beat_is_reached_as_that_beat_ends_and_held() {
    let f = build(&format!("{HEAD}{NAMED}"), &[]).unwrap();
    let x = |time: f64, id: &str| {
        labels(&f, time)
            .iter()
            .find(|l| l.id == id)
            .map(|l| (l.x, l.opacity))
    };
    // Empty until `write` starts, at 0.5 s, and whole when it ends, at 1.5 s.
    assert_eq!(x(0.5, "e.a"), None);
    let (written, strength) = x(1.5, "e.b").unwrap();
    assert_eq!(strength, 1.0);
    assert!(x(1.0, "e.b").unwrap().1 < 1.0);
    // `wait` isn't named, so nothing moves through it or the pause after it.
    for time in [1.75, 2.0, 2.5, 2.9, 3.0] {
        assert_eq!(x(time, "e.b"), Some((written, 1.0)), "at {time} s");
    }
    // `move` takes it from where it was to the other place, solid all the way.
    let (moving, strength) = x(3.5, "e.b").unwrap();
    assert!(written < moving && moving < x(4.0, "e.b").unwrap().0);
    assert_eq!(strength, 1.0);
    // And `gone` leaves nothing.
    assert!(labels(&f, 4.5).is_empty());
}

#[test]
fn it_lays_out_what_a_state_for_each_beat_does() {
    let named = build(&format!("{HEAD}{NAMED}"), &[]).unwrap();
    let rows = build(&format!("{HEAD}{ROWS}"), &[]).unwrap();
    assert_same(&named, &rows, 4.5);
}

#[test]
fn its_beats_can_be_written_after_it() {
    let beats = "pause 0.5\nbeat write for 1\nbeat wait for 1\npause 0.5\nbeat move for 1\nbeat gone for 0.5\n";
    let rest = HEAD.replace(beats, "");
    assert_ne!(rest, HEAD);
    let after = build(&format!("{rest}{NAMED}\n{beats}"), &[]).unwrap();
    let before = build(&format!("{HEAD}{NAMED}"), &[]).unwrap();
    assert_same(&after, &before, 4.5);
}

/// What an expression holds after each state, worked out here on its own from the rule and
/// not by the compiler: a place that's written is set, where it already is in the list, a term
/// written at a new place leaves its old one, a place left with nothing goes, and `empty` clears
/// them all. A state is `"terms" at place, …`, as written.
fn boards(states: &[&str]) -> Vec<String> {
    let mut board: Vec<(String, Vec<String>)> = Vec::new();
    let mut out = Vec::new();
    for state in states {
        if *state == "empty" {
            board.clear();
        } else {
            for piece in state.split(", ") {
                let (text, place) = piece.rsplit_once(" at ").unwrap();
                let terms: Vec<String> = (text.trim().trim_matches('"').split_whitespace())
                    .map(str::to_string)
                    .collect();
                for (other, held) in &mut board {
                    if other != place {
                        held.retain(|term| !terms.contains(term));
                    }
                }
                match board.iter_mut().find(|(other, _)| other == place) {
                    Some((_, held)) => *held = terms,
                    None => board.push((place.to_string(), terms)),
                }
            }
            board.retain(|(_, held)| !held.is_empty());
        }
        out.push(if board.is_empty() {
            "empty".to_string()
        } else {
            let pieces: Vec<String> = board
                .iter()
                .map(|(place, held)| format!("\"{}\" at {place}", held.join(" ")))
                .collect();
            pieces.join(", ")
        });
    }
    out
}

/// The square's `work`, with what it holds on each of its beats written out whole for that beat.
#[test]
fn the_square_lays_out_what_a_state_for_each_of_its_beats_does() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/");
    let read = |path: &str| std::fs::read_to_string(format!("{root}{path}")).unwrap();
    let pen = read("006-square/square.pen");
    let sheets = [read("theme.style"), read("006-square/plain.style")];
    let sheets: Vec<&str> = sheets.iter().map(String::as_str).collect();

    let open = "expression work {\n";
    let start = pen.find(open).expect("the square's `work`");
    let end = start + pen[start..].find("\n}").unwrap();
    let states: Vec<(&str, &str)> = pen[start + open.len()..end]
        .lines()
        .map(|line| {
            let (on, state) = line.trim().split_once(": ").unwrap();
            (on.strip_prefix("on ").unwrap(), state)
        })
        .collect();
    let beats: Vec<&str> = pen
        .lines()
        .filter_map(|line| line.strip_prefix("beat ")?.split(' ').next())
        .collect();
    assert!(states.iter().all(|(on, _)| beats.contains(on)));
    let written: Vec<&str> = states.iter().map(|(_, state)| *state).collect();
    let whole = boards(&written);
    // `z` is written once and held at its place through eight later states.
    assert_eq!(written.iter().filter(|s| s.contains("at_z")).count(), 3);
    assert_eq!(
        whole.iter().filter(|s| s.contains("\"z\" at at_z")).count(),
        8
    );
    let mut holds = "empty";
    let mut rows = String::from("expression work by beat {\n");
    for beat in &beats {
        if let Some(at) = states.iter().position(|(on, _)| on == beat) {
            holds = &whole[at];
        }
        rows.push_str(&format!("  {holds}\n"));
    }
    // More beats than states, or this compares a figure with itself.
    assert!(beats.len() > states.len(), "{} beats", beats.len());
    let by_beat = format!("{}{rows}{}", &pen[..start], &pen[end + 1..]);

    let named = build(&pen, &sheets).unwrap();
    let rows = build(&by_beat, &sheets).unwrap();
    let duration = named.scene.duration.unwrap();
    assert_eq!(rows.scene.duration, Some(duration));
    let frames = (duration * 60.0).round() as usize;
    for k in 0..=frames {
        let time = k as f64 / 60.0;
        let seen = |f: &Figure| -> Vec<(String, f64, f64, f64)> {
            prepare(f, &Inputs::at(time), 680, 470, 2.0)
                .unwrap()
                .labels
                .iter()
                .map(|l| (l.id.clone(), l.x, l.y, l.opacity))
                .collect()
        };
        assert_eq!(seen(&named), seen(&rows), "at {time} s");
    }
}

fn error(expression: &str) -> pencil_core::Error {
    build(&format!("{HEAD}{expression}"), &[]).unwrap_err()
}

/// The line a state is on, counting from the expression's own first line.
fn line(of_expression: usize) -> usize {
    HEAD.lines().count() + of_expression
}

#[test]
fn a_state_on_a_beat_that_is_not_one_is_an_error_at_that_state() {
    let err = error("expression e {\n  on write: \"a\" at left\n  on mvoe: \"a\" at right\n}");
    assert_eq!(
        err.message,
        "`e` reaches a state on `mvoe`, and no beat is named that: did you mean `move`?"
    );
    assert_eq!(err.line, line(3));

    let err = error("expression e {\n  on nothing: \"a\" at left\n}");
    assert!(
        err.message
            .starts_with("`e` reaches a state on `nothing`, and no beat is named that."),
        "{err}"
    );
    assert_eq!(err.line, line(2));

    // A value moves as a beat's name does, but it has no place in the timeline.
    let err = error("let half = write / 2\nexpression e {\n  on half: \"a\" at left\n}");
    assert!(
        err.message
            .contains("`half`, which is a value and not a beat"),
        "{err}"
    );
    assert_eq!(err.line, line(3));
}

#[test]
fn a_beat_named_twice_is_an_error_at_the_second_state() {
    let err = error(
        "expression e {\n  on write: \"a\" at left\n  on move: \"a\" at right\n  on write: empty\n}",
    );
    assert_eq!(
        err.message,
        format!(
            "`write` brings `e` to a state twice, here and on line {}. A beat is one move, so it ends in one state",
            line(2)
        )
    );
    assert_eq!(err.line, line(4));
}

#[test]
fn states_out_of_the_timelines_order_are_an_error_at_the_one_that_runs_first() {
    let err = error(
        "expression e {\n  on write: \"a\" at left\n  on gone: empty\n  on move: \"a\" at right\n}",
    );
    assert_eq!(
        err.message,
        "`move` comes before `gone` in the timeline, but follows it in `e`. Its states are written in the order their beats run"
    );
    assert_eq!(err.line, line(4));
}

#[test]
fn a_figure_whose_beats_have_no_lengths_has_none_to_name() {
    let err = build(
        "place p = (0, 0)\nlet P = (0, 0)\nlet Q = (1, 0)\nbeat \"One.\" {\n  dot P\n}\nbeat \"Two.\" {\n  dot Q\n}\nexpression e {\n  on two: \"a\" at p\n}",
        &[],
    )
    .unwrap_err();
    assert!(
        err.message.contains("this figure's beats have no names")
            && err.message.contains("`expression e by beat`"),
        "{err}"
    );
    assert_eq!(err.line, 11);
}

#[test]
fn a_state_says_on_or_the_expression_says_by_and_not_both() {
    let err = error("expression e by beat {\n  on write: \"a\" at left\n}");
    assert!(err.message.contains("don't say `on`"), "{err}");
    assert_eq!(err.line, line(2));
    let err = error("expression e {\n  \"a\" at left\n}");
    assert!(err.message.contains("`e` has no `by`"), "{err}");
    assert_eq!(err.line, line(2));
    let err = error("expression e \"a\" at left");
    assert!(err.message.contains("expected `by`"), "{err}");
}

/// The reference's two examples, one for each way to drive an expression, as they're written.
#[test]
fn the_references_examples_compile() {
    let doc = include_str!("../../../docs/language.md");
    let start = doc.find("### Expressions that move").unwrap();
    let end = start + doc[start..].find("### Mathematics in labels").unwrap();
    let blocks: Vec<&str> = doc[start..end].split("```").skip(1).step_by(2).collect();
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    let view = "viewport { x: -2..2, y: -2..2 }\n";
    let named = build(&format!("{view}{}", blocks[0]), &[]).unwrap();
    assert!((named.scene.duration.unwrap() - 8.0).abs() < 1e-9);
    // `answer` writes at `result` alone, and the equation is still above it.
    assert_eq!(
        held(&named),
        [
            "z ( a + b ) at equation",
            "z a + z.1 b at equation",
            "z ( a + b ) at equation",
            "z ( a + b ) at equation, = c at result",
        ]
    );
    let by = build(&format!("{view}{}", blocks[1]), &[]).unwrap();
    assert_eq!(by.scene.expressions[0].states.len(), 4);
    // The series starts on its first term, and a reader's `n` takes it to the fourth.
    assert_eq!(labels(&by, 0.0).len(), 1);
}

/// `lab/expressions/joined.pen` and `spaced.pen`: the same two states, written `2ab` and
/// `2 a b`. A term is what's between spaces, so only the second has a `b` to move.
#[test]
fn written_together_a_product_fades_and_spaced_its_last_factor_moves() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lab/expressions/");
    let figure = |name: &str| {
        build(
            &std::fs::read_to_string(format!("{root}{name}")).unwrap(),
            &[],
        )
        .unwrap()
    };
    let (joined, spaced) = (figure("joined.pen"), figure("spaced.pen"));
    let find = |f: &Figure, time: f64, id: &str| {
        labels(f, time)
            .iter()
            .find(|l| l.id == id)
            .map(|l| (l.x, l.y, l.opacity))
    };
    // Written at 1 s, and `part` runs from 2 s to 3.5 s.
    let (held, moving, done) = (1.9, 2.3, 4.0);

    // Spaced: `b` goes from the end of the product to its own place, solid, and nothing else moves.
    let (from, y, _) = find(&spaced, held, "e.b").unwrap();
    let (to, _, _) = find(&spaced, done, "e.b").unwrap();
    let (x, at_y, strength) = find(&spaced, moving, "e.b").unwrap();
    assert!(from < x && x < to, "{from} {x} {to}");
    assert_eq!((at_y, strength), (y, 1.0));
    for id in ["e.2", "e.a"] {
        let still = find(&spaced, held, id).unwrap();
        assert_eq!(find(&spaced, moving, id), Some(still), "{id}");
        assert_eq!(find(&spaced, done, id), Some(still), "{id}");
    }

    // Joined: the product has no part in the next state, so it fades where it is, and what
    // replaces it fades in where it ends up. Nothing travels.
    let (at, _, _) = find(&joined, held, "e.2ab").unwrap();
    let (x, _, going) = find(&joined, moving, "e.2ab").unwrap();
    assert_eq!(x, at);
    assert!(0.0 < going && going < 1.0, "{going}");
    assert_eq!(find(&joined, done, "e.2ab"), None);
    for id in ["e.2a", "e.b"] {
        let (ends, _, _) = find(&joined, done, id).unwrap();
        let (x, _, coming) = find(&joined, moving, id).unwrap();
        assert_eq!(x, ends, "{id}");
        assert!((going + coming - 1.0).abs() < 1e-12, "{id}: {coming}");
        assert_eq!(find(&joined, held, id), None, "{id}");
    }
    // Both end the same: `b` at its place, and what's left where the product was.
    assert_eq!(
        find(&joined, done, "e.b").map(|l| l.0),
        find(&spaced, done, "e.b").map(|l| l.0)
    );
}

/// A trial, from Kris's "maybe just make it 1 second by default": a named beat can leave out its
/// length. With no `duration` it takes a second, so it has a during, and a state `on` it plays.
const OPEN: &str = "viewport { x: 0..400, y: 0..400 } style figure { width: 400px; height: 400px }
beat write
pause 0.5
beat wait for 2
beat move
place left = (100, 200)
place right = (300, 200)
expression e {
  on write: \"a + b\" at left
  on move: \"a\" at left, \"+ b\" at right
}
";

/// When each beat starts and ends, from the timeline `beat` follows.
fn windows(figure: &Figure) -> Vec<f64> {
    let beat = figure
        .scene
        .bindings
        .iter()
        .find(|b| b.id == "beat")
        .unwrap();
    let pencil_core::ir::Expr::App(head, args) = &beat.expr else {
        panic!("{:?}", beat.expr);
    };
    assert_eq!(head, "Timeline");
    args[1..]
        .iter()
        .map(|e| match e {
            pencil_core::ir::Expr::Num(n) => *n,
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_named_beat_with_no_length_takes_a_second_and_a_state_on_it_plays() {
    let f = build(OPEN, &[]).unwrap();
    assert_eq!(windows(&f), [0.0, 1.0, 1.5, 3.5, 3.5, 4.5]);
    assert_eq!(f.scene.duration, Some(4.5));
    assert_eq!(f.scene.beat_lengths, [1.0, 2.0, 1.0]);
    // Halfway through `move`, which has no length, `b` is on its way and solid: it has a during.
    let b = |time: f64| {
        let all = labels(&f, time);
        let b = all.iter().find(|l| l.id == "e.b").unwrap();
        (b.x, b.opacity)
    };
    let (from, to) = (b(3.5).0, b(4.5).0);
    let (x, strength) = b(4.0);
    assert!(from < x && x < to, "{from} {x} {to}");
    assert_eq!(strength, 1.0);
    // It's the same figure as one that says `for 1` on both.
    let said = OPEN
        .replace("beat write\n", "beat write for 1\n")
        .replace("beat move\n", "beat move for 1\n");
    assert_ne!(said, OPEN);
    assert_same(&f, &build(&said, &[]).unwrap(), 4.5);
}

#[test]
fn under_a_stated_duration_beats_with_no_length_share_what_is_left() {
    // 10 s, less 2 s and a pause of 0.5 s, leaves 3.75 s for each of the two.
    let f = build(&format!("duration 10\n{OPEN}"), &[]).unwrap();
    assert_eq!(windows(&f), [0.0, 3.75, 4.25, 6.25, 6.25, 10.0]);
    assert_eq!(f.scene.duration, Some(10.0));
    // With nothing left, there's no length to give them.
    let err = build(&format!("duration 2.5\n{OPEN}"), &[]).unwrap_err();
    assert_eq!(
        err.message,
        "the beats with lengths and the pauses take 2.5 seconds, which leaves nothing of the `duration` of 2.5 for the 2 beats with no length"
    );
    assert_eq!(err.line, 3);
    // And `duration` reads the sum when it isn't stated, a second for each.
    let f = build(&format!("{OPEN}let d = duration\n"), &[]).unwrap();
    let d = f.scene.bindings.iter().find(|b| b.id == "d").unwrap();
    assert_eq!(d.expr, pencil_core::ir::Expr::Num(4.5));
}

/// The square with every length taken out, and its `duration` and `poster` with them: what the
/// trial does to a real figure. Each of its 15 beats is a second, and its algebra still plays.
#[test]
fn the_square_with_no_lengths_plays_a_second_a_beat() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/");
    let read = |path: &str| std::fs::read_to_string(format!("{root}{path}")).unwrap();
    let sheets = [read("theme.style"), read("006-square/plain.style")];
    let sheets: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let mut beats = 0;
    let pen: String = read("006-square/square.pen")
        .lines()
        .filter(|line| !line.starts_with("duration ") && !line.starts_with("poster "))
        .map(|line| {
            let Some(rest) = line.strip_prefix("beat ") else {
                return format!("{line}\n");
            };
            // `beat name for 0.6 show hypotenuse` to `beat name show hypotenuse`.
            let mut words: Vec<&str> = rest.split(' ').collect();
            let at = words.iter().position(|w| *w == "for").unwrap();
            words.drain(at..at + 2);
            beats += 1;
            format!("beat {}\n", words.join(" "))
        })
        .collect();
    let f = build(&pen, &sheets).unwrap();
    assert_eq!((beats, f.scene.beat_lengths.len()), (15, 15));
    assert!(f.scene.beat_lengths.iter().all(|length| *length == 1.0));
    // 15 s of beats and the 8.4 s of pauses it wrote.
    let duration = f.scene.duration.unwrap();
    assert!((duration - 23.4).abs() < 1e-9, "{duration}");
    // Halfway through `scale_a`, the copy of z has left z and is on its way to the arrow's tip.
    let times = windows(&f);
    let (start, end) = (times[2 * 6], times[2 * 6 + 1]);
    assert_eq!(end - start, 1.0);
    let at = |time: f64, id: &str| {
        let all = prepare(&f, &Inputs::at(time), 680, 470, 2.0)
            .unwrap()
            .labels;
        all.iter()
            .find(|l| l.id == id)
            .map(|l| (l.x, l.y, l.opacity))
    };
    let z = at(start + 0.5, "work.z").unwrap();
    let copy = at(start + 0.5, "work.z.1").unwrap();
    let there = at(end, "work.z.1").unwrap();
    assert_eq!(copy.2, 1.0);
    let gone = ((copy.0 - z.0).powi(2) + (copy.1 - z.1).powi(2)).sqrt();
    let left = ((copy.0 - there.0).powi(2) + (copy.1 - there.1).powi(2)).sqrt();
    assert!(
        gone > 5.0 && left > 0.5,
        "{gone} px from z, {left} px to go"
    );
}

/// What an `on` expression holds after each of its states, as the compiler built it.
fn held(figure: &Figure) -> Vec<String> {
    let spots = &figure.scene.spots;
    (figure.scene.expressions[0].states.iter())
        .map(|state| {
            let pieces: Vec<String> = state
                .iter()
                .map(|p| format!("{} at {}", p.keys.join(" "), spots[p.spot].id))
                .collect();
            pieces.join(", ")
        })
        .collect()
}

const PLACES: &str = "beat one for 1\nbeat two for 1\nbeat three for 1\nbeat four for 1
place p = (0, 0)\nplace q = (1, 0)\nplace r = (2, 0)\n";

#[test]
fn a_state_on_a_beat_writes_only_the_places_it_changes() {
    let states = |expression: &str| {
        held(&build(&format!("{PLACES}expression e {{\n{expression}\n}}"), &[]).unwrap())
    };
    // A place that isn't named keeps what it has, and one that is named is set to what's written:
    // `c` goes from `p`, since the second state names `p` without it.
    assert_eq!(
        states("on one: \"a b c\" at p\non two: \"x\" at q\non three: \"a b\" at p"),
        ["a b c at p", "a b c at p, x at q", "a b at p, x at q"]
    );
    // A term is in one place: written at another, it leaves where it was, and a place left with
    // nothing is gone. So `p` empties without being named.
    assert_eq!(
        states("on one: \"a\" at p\non two: \"+ b\" at q\non three: \"a + b\" at r"),
        ["a at p", "a at p, + b at q", "a + b at r"]
    );
    // One term moving out leaves the rest where they are, in their order.
    assert_eq!(
        states("on one: \"a b c\" at p\non two: \"b\" at q"),
        ["a b c at p", "a c at p, b at q"]
    );
    // `empty` clears every place, and what's written after it starts again.
    assert_eq!(
        states("on one: \"a\" at p\non two: \"b\" at q\non three: empty\non four: \"c\" at r"),
        ["a at p", "a at p, b at q", "", "c at r"]
    );
    // A copy is its own term: `z.1` written elsewhere leaves `z` where it is.
    assert_eq!(
        states("on one: \"z\" at p\non two: \"z.1 a\" at q"),
        ["z at p", "z at p, z.1 a at q"]
    );
    // With `by`, a state is whole, as it always was: what isn't written isn't there.
    let by = build(
        &format!("{PLACES}expression e by beat {{\n\"a\" at p\n\"b\" at q\n}}"),
        &[],
    )
    .unwrap();
    assert_eq!(held(&by), ["a at p", "b at q"]);
}

#[test]
fn a_place_is_written_once_in_a_state_and_a_wrong_name_says_what_is_close() {
    let err = build(
        &format!("{PLACES}expression e {{\non one: \"a\" at p, \"b\" at p\n}}"),
        &[],
    )
    .unwrap_err();
    assert!(
        err.message
            .starts_with("`p` is written twice in one state of `e`"),
        "{err}"
    );
    assert_eq!(err.line, 9);
    let err = build(
        "place at_sum = (0, 0)\nbeat one for 1\nexpression e {\non one: \"a\" at at_sun\n}",
        &[],
    )
    .unwrap_err();
    assert_eq!(
        err.message,
        "no place named `at_sun`: did you mean `at_sum`?"
    );
    assert_eq!(err.line, 4);
}
