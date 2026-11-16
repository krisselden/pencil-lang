//! A param's range: what keeps it there, and a range that follows other params.

use std::path::Path;

use pencil_core::eval::{self, Env};
use pencil_core::frame::Inputs;
use pencil_core::{Source, Sources, build, compose};

fn value(doc: &str, name: &str, held: &[(&str, f64)]) -> f64 {
    let figure = build(doc, &[]).unwrap();
    let inputs = Inputs {
        time: 0.0,
        params: held.iter().map(|(id, v)| (id.to_string(), *v)).collect(),
    };
    eval::frame_env(&figure, &inputs)
        .unwrap()
        .get(name)
        .unwrap()
}

const SIDES: &str = "param q = 7 in 3..12 step 1\nparam p = 2 in 0..q step 1\n\
                     let P = (p, q)\ndot P";

#[test]
fn a_param_stays_in_its_range_however_its_driven() {
    let doc = "param a = 5 in 0..2\nparam b = 0 in 0..2\nlet P = (a, b)\ndot P";
    // Its default, and a reader's value.
    assert_eq!(value(doc, "a", &[]), 2.0);
    assert_eq!(value(doc, "b", &[("b", -3.0)]), 0.0);
    assert_eq!(value(doc, "b", &[("b", 1.5)]), 1.5);
    // A step is counted from the start, and an end that's off a step isn't reached.
    let stepped = "param n = 0 in 1..4.5 step 1\nlet P = (n, 0)\ndot P";
    assert_eq!(value(stepped, "n", &[("n", 9.0)]), 4.0);
    assert_eq!(value(stepped, "n", &[("n", 2.4)]), 2.0);
}

#[test]
fn a_range_can_end_at_another_param() {
    assert_eq!(value(SIDES, "p", &[]), 2.0);
    assert_eq!(value(SIDES, "p", &[("p", 9.0)]), 7.0);
    // The reader's value is kept, and met again when the range has room for it.
    assert_eq!(value(SIDES, "p", &[("p", 9.0), ("q", 4.0)]), 4.0);
    assert_eq!(value(SIDES, "p", &[("p", 9.0), ("q", 11.0)]), 9.0);
    // The range's start is where steps count from.
    let from = "param a = 0.5 in 0..1\nparam b = 0 in a..a + 3 step 1\nlet P = (a, b)\ndot P";
    assert_eq!(value(from, "b", &[("b", 2.2)]), 2.5);
    // A range that ends before it starts leaves its start.
    let empty = "param a = 1 in 0..1\nparam b = 0 in 0.5..a\nlet P = (a, b)\ndot P";
    assert_eq!(value(empty, "b", &[("a", 0.0), ("b", 0.75)]), 0.5);
}

#[test]
fn a_page_can_read_the_widest_range_and_what_it_follows() {
    let composition = compose(SIDES, &[]).unwrap();
    let info = composition.info();
    let p = &info["params"][1];
    assert_eq!(
        (p["id"].as_str(), p["min"].as_f64(), p["max"].as_f64()),
        (Some("p"), Some(0.0), Some(12.0))
    );
    assert_eq!(p["follows"], serde_json::json!(["q"]));
    assert_eq!(info["params"][0]["follows"], serde_json::json!([]));
    // And the range as it stands in a frame.
    let figure = &composition.panels[0].figure;
    let held = Inputs {
        time: 0.0,
        params: vec![("q".to_string(), 5.0)],
    };
    let env = eval::frame_env(figure, &held).unwrap();
    assert_eq!(
        eval::param_range(&figure.scene.params[1], &env).unwrap(),
        [0.0, 5.0]
    );
    // The widest is found at the ends of what it reads, whichever way it runs.
    let falls = "param a = 0 in 1..4\nparam b = 0 in -a..10 - 2·a\nlet P = (a, b)\ndot P";
    let scene = build(falls, &[]).unwrap().scene;
    assert_eq!(scene.params[1].range, [-4.0, 8.0]);
}

#[test]
fn a_range_reads_numbers_and_params() {
    let message = |doc: &str| build(doc, &[]).unwrap_err().to_string();
    let named = message("let n = 3\nparam p = 0 in 0..n\nlet P = (p, 0)\ndot P");
    assert!(
        named.contains("numbers and the params before it, and `n` isn't either"),
        "{named}"
    );
    let timed = message("param p = 0 in 0..time\nlet P = (p, 0)\ndot P");
    assert!(
        timed.contains("must be constant, and it uses `time`"),
        "{timed}"
    );
    // A param can be written after the one whose range reads it, as any name can.
    assert_eq!(
        value(
            "param p = 2 in 0..q\nparam q = 1 in 0..4\nlet P = (p, q)\ndot P",
            "p",
            &[]
        ),
        1.0
    );
    let circle = message("param p = 0 in 0..q\nparam q = 0 in 0..p\nlet P = (p, q)\ndot P");
    assert!(circle.contains("is defined in terms of itself"), "{circle}");
}

#[test]
fn a_param_in_a_range_that_time_moves_follows_time() {
    let doc = "duration 4\nparam q = 1 + time in 0..5\nparam p = 9 in 0..q\nlet P = (p, q)\ndot P";
    let scene = build(doc, &[]).unwrap().scene;
    assert_eq!(pencil_core::clock::time_driven(&scene)[..2], ["q", "p"]);
}

/// Every figure in the corpus and the explainer, with the files it uses.
fn figures() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = vec![
        root.join("lab/explainer/limacon.pen"),
        root.join("lab/explainer/proof.pen"),
    ];
    for entry in std::fs::read_dir(root.join("corpus")).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(dir).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_some_and(|e| e == "pen") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn no_figures_own_motion_runs_into_the_end_of_a_range() {
    // Keeping a param in its range is for a reader's hand. A story that leaned on it would stop
    // short with nothing to say why, so the figures here don't.
    for path in figures() {
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.to_str().unwrap();
        let read = |file: &str| std::fs::read_to_string(file).ok();
        let sources = Sources::read(Source { name, text: &text }, &[], &read).unwrap();
        for panel in &sources.compose().unwrap().panels {
            let figure = &panel.figure;
            let scene = &figure.scene;
            let duration = scene.duration.unwrap_or(0.0);
            for k in 0..=240 {
                let time = duration * k as f64 / 240.0;
                let env: Env = eval::frame_env(figure, &Inputs::at(time)).unwrap();
                for param in &scene.params {
                    let binding = scene.bindings.iter().find(|b| b.id == param.id).unwrap();
                    let Ok(asked) = eval::real(&binding.expr, &env) else {
                        continue;
                    };
                    let [lo, hi] = eval::param_range(param, &env).unwrap();
                    assert!(
                        asked >= lo - 1e-9 && asked <= hi + 1e-9,
                        "{name}: `{}` is driven to {asked} at {time:.2} s, outside {lo}..{hi}",
                        param.id
                    );
                }
            }
        }
    }
}
