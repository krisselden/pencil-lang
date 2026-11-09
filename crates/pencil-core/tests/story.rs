//! Beats with lengths, as a sheet and a frame see them: what's there, what it's about, and
//! the caption.

use pencil_core::frame::{Inputs, prepare_with};
use pencil_core::terms::Estimate;
use pencil_core::{build, compose, eval};

const STORY: &str = "
    viewport { x: -2..2, y: -2..2 } style figure { width: 200px; height: 200px }
    pause 0.5
    beat grow \"A circle.\" for 1 show c
    beat spin \"A point goes round.\" for 2 show P focus P
    beat hold for 1
    beat gone \"\" for 0.5 drop c, P
    let P = (cos(tau·spin), sin(tau·spin))
    curve c = circle((0, 0), (grow, 0))
    dot P
    segment axis = (-1, 0), (1, 0)";

const SHEET: &str = "
    style c { stroke: black; width: 2px; opacity: self.life·mix(0.25, 1, self.focus) }
    style P { fill: black; radius: 3px; opacity: self.life }
    style axis { stroke: black; width: 1px; opacity: self.life·self.focus }";

fn at(time: f64) -> Inputs {
    Inputs {
        time,
        params: vec![],
    }
}

#[test]
fn what_a_beat_has_not_shown_is_not_drawn() {
    let f = build(STORY, &[SHEET]).unwrap();
    let id = |name: &str| f.scene.marks.iter().position(|m| m.id == name).unwrap();
    let shown = |time: f64| {
        prepare_with(
            &f,
            &at(time),
            200,
            200,
            1.0,
            pencil_core::ir::DEFAULT_EM,
            &Estimate,
        )
        .unwrap()
        .shown
    };
    let (c, p, axis) = (id("c"), id("P"), id("axis"));
    // Before it's shown, and after it's dropped, a mark isn't there at all.
    assert!(!shown(0.0)[c] && !shown(0.0)[p] && shown(0.0)[axis]);
    assert!(shown(1.0)[c] && !shown(1.0)[p]);
    assert!(shown(2.0)[c] && shown(2.0)[p]);
    assert!(!shown(5.0)[c] && !shown(5.0)[p] && shown(5.0)[axis]);
}

#[test]
fn a_sheet_reads_each_marks_life_and_focus() {
    let f = build(STORY, &[SHEET]).unwrap();
    let id = |name: &str| f.scene.marks.iter().position(|m| m.id == name).unwrap();
    let opacity = |name: &str, time: f64| {
        let env = eval::frame_env(&f, &at(time)).unwrap();
        eval::real(f.styles.marks[id(name)].opacity.as_ref().unwrap(), &env).unwrap()
    };
    // Arriving halfway through its beat, then in focus, then stepping back as the next beat
    // is about something else, and leaving.
    assert!((opacity("c", 1.0) - 0.5).abs() < 1e-12);
    assert_eq!(opacity("c", 1.5), 1.0);
    assert!((opacity("c", 2.5) - 0.625).abs() < 1e-12);
    assert_eq!(opacity("c", 3.5), 0.25);
    assert!((opacity("c", 4.75) - 0.125).abs() < 1e-12);
    // A beat with no `focus` keeps the one before.
    assert_eq!(opacity("axis", 4.5), 0.0);
    assert_eq!(opacity("P", 4.5), 1.0);
}

#[test]
fn captions_are_drawn_only_when_a_sheet_asks() {
    let captions = |sheets: &[&str], time: f64| {
        let c = compose(STORY, sheets).unwrap();
        let frames = c
            .prepare(&at(time), 200, 200, 1.0, None, &Estimate)
            .unwrap();
        c.labels(&frames, &at(time), &Estimate)
            .unwrap()
            .into_iter()
            .filter(|l| l.id.starts_with("caption/"))
            .map(|l| (l.text, (l.opacity * 1000.0).round() / 1000.0))
            .collect::<Vec<_>>()
    };
    assert!(captions(&[SHEET], 1.0).is_empty());
    let asked = [SHEET, "style caption { fill: black; size: 12px }"];
    // Nothing before the first beat moves, then it fades in over its first fifth of a second.
    assert!(captions(&asked, 0.4).is_empty());
    assert_eq!(captions(&asked, 0.6), [("A circle.".to_string(), 0.5)]);
    assert_eq!(captions(&asked, 1.2), [("A circle.".to_string(), 1.0)]);
    // A new caption waits for the old one to go, and a beat without one keeps it.
    assert_eq!(captions(&asked, 1.6), [("A circle.".to_string(), 0.5)]);
    assert!(captions(&asked, 1.7).is_empty());
    assert_eq!(
        captions(&asked, 1.8),
        [("A point goes round.".to_string(), 0.5)]
    );
    assert_eq!(
        captions(&asked, 4.4),
        [("A point goes round.".to_string(), 1.0)]
    );
    // An empty caption clears it.
    assert!(captions(&asked, 5.0).is_empty());
    let label = |time: f64| {
        let c = compose(STORY, &asked).unwrap();
        let frames = c
            .prepare(&at(time), 200, 200, 1.0, None, &Estimate)
            .unwrap();
        c.labels(&frames, &at(time), &Estimate).unwrap()
    };
    let caption = label(1.2)
        .into_iter()
        .find(|l| l.id.starts_with("caption/"))
        .unwrap();
    // Centered along the bottom, 28 px up by default.
    assert_eq!((caption.x, caption.y, caption.size), (100.0, 172.0, 12.0));
}
