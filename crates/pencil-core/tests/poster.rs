//! A figure's poster, and what a build writes down about a figure for the page around it.

use pencil_core::{Composition, compose};

fn moment(doc: &str) -> (f64, Vec<(String, f64)>) {
    let poster = compose(doc, &[]).unwrap().poster();
    (poster.time, poster.params)
}

#[test]
fn a_poster_is_the_moment_the_document_names() {
    let (time, params) = moment(
        "param roll = 1 in 0..1\nlet P = (roll, time)\ndot P\nposter { time: 3.5, roll: 0.6 }",
    );
    assert_eq!((time, &params[..]), (3.5, &[("roll".to_string(), 0.6)][..]));
}

#[test]
fn without_one_its_the_end_of_a_story_or_the_start_of_a_clock() {
    // Beats with lengths: where everything has arrived.
    let story = "let P = (0, 0)\nlet Q = (1, 0)\ndot P, Q\n\
                 beat one for 1 show P\npause 0.5\nbeat two for 2 show Q\npause 1";
    assert_eq!(moment(story).0, 4.5);
    // A poster that holds a param and names no time ends there too.
    let held = format!("param a = 0 in 0..1\n{story}\nposter {{ a: 1 }}");
    assert_eq!(moment(&held), (4.5, vec![("a".to_string(), 1.0)]));
    // A clock with no story, and a figure with no clock, start at the start.
    assert_eq!(moment("duration 13\nlet P = (cos(time), sin(time))\ndot P").0, 0.0);
    assert_eq!(moment("let P = (0, 0)\ndot P").0, 0.0);
}

#[test]
fn what_a_page_needs_to_know_survives_being_compiled() {
    let doc = "duration 13\nparam roll = 1 in 0.01..1\nparam p = 1 in 1..3 step 1\n\
               let P = (roll, cos(time))\ndot P\nposter { time: 3.5, roll: 0.6 }";
    let sheet = "let ink = rgb(18, 17, 15)\nlet paper_color = rgb(242, 240, 235)\n\
                 style paper { fill: paper_color }\nstyle figure { width: 640px; height: 400px }\n\
                 style P { fill: ink }";
    let composition = compose(doc, &[sheet]).unwrap();
    let info = composition.info();
    assert_eq!(info["size"].to_string(), "[640.0,400.0]");
    assert_eq!(info["duration"], 13.0);
    assert_eq!(info["poster"].to_string(), r#"{"params":{"roll":0.6},"time":3.5}"#);
    assert_eq!(
        info["colors"].to_string(),
        r#"{"ink":"rgb(18 17 15)","paper_color":"rgb(242 240 235)"}"#
    );
    let params: Vec<_> = info["params"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["min"].as_f64().unwrap(), p["max"].as_f64().unwrap()))
        .collect();
    assert_eq!(params, [("roll", 0.01, 1.0), ("p", 1.0, 3.0)]);

    // A page that loads the compiled figure has no sheets to ask, and is told the same.
    let read = Composition::from_bytes(&composition.to_bytes().unwrap()).unwrap();
    assert_eq!(read.info(), info);
    assert_eq!(read.colors, composition.colors);
}
