//! A control's words and kind, and readouts: what a page is told so it can say them beside the
//! figure.

use pencil_core::frame::Inputs;
use pencil_core::terms::Estimate;
use pencil_core::{compose, parse};
use serde_json::{Value, json};

fn info(doc: &str, sheets: &[&str]) -> Value {
    compose(doc, sheets).unwrap().info()
}

fn param<'a>(info: &'a Value, id: &str) -> &'a Value {
    info["params"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("no param {id}"))
}

fn error(doc: &str, sheets: &[&str]) -> String {
    compose(doc, sheets).unwrap_err().to_string()
}

const VIEW: &str = "viewport { x: -2..2, y: -2..2 }\n";

#[test]
fn a_param_can_say_what_its_control_says() {
    let doc = format!(
        "{VIEW}param q \"arms\" = 8 in 3..16 step 1\nparam along \"direction\" = \"i\" in {{1, i, 1 + i}}\nparam r = 1 in 0..2"
    );
    let info = info(&doc, &[]);
    assert_eq!(param(&info, "q")["caption"], "arms");
    assert_eq!(param(&info, "along.index")["caption"], "direction");
    // Without one, the control says the param's name, as it did.
    assert_eq!(param(&info, "r")["caption"], Value::Null);
    // The caption is before the default, so the choice's default is still its option's name.
    let figure = pencil_core::build(&doc, &[]).unwrap();
    let env = pencil_core::eval::frame_env(&figure, &Inputs::at(0.0)).unwrap();
    assert_eq!(env.get("along.index"), Some(1.0));
    // It's in the IR, as written.
    let ir = figure.scene.to_json();
    let q = ir["params"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "q")
        .unwrap();
    assert_eq!(q["caption"], "arms");
}

#[test]
fn a_caption_is_words() {
    let at = |doc: &str| parse::parse(doc).unwrap_err().to_string();
    assert!(at("param q \"{q} arms\" = 8 in 3..16").contains("a value is a readout's"));
    assert!(at("param q \"$q$\" = 8 in 3..16").contains("mathematics"));
    assert!(at("param q \"\" = 8 in 3..16").contains("can't be empty"));
    // Before the name is still a mistake.
    assert!(parse::parse("param \"arms\" q = 8 in 3..16").is_err());
}

#[test]
fn a_choice_of_a_few_is_radios_more_is_a_select_and_off_and_on_a_checkbox() {
    let doc = format!(
        "{VIEW}param n = 1 in 0..2\nparam few in {{1, 2, 3, 4}}\nparam many in {{1, 2, 3, 4, 5}}\n\
         param glow in {{\"off\": 0, \"on\": 1}}\nparam bit in {{0, 1}}\nparam two in {{\"plain\": 0, \"lit\": 1}}"
    );
    let info = info(&doc, &[]);
    let kind = |id: &str| param(&info, id)["control"]["kind"].clone();
    assert_eq!(kind("n"), "slider");
    assert_eq!(kind("few.index"), "radios");
    assert_eq!(kind("many.index"), "select");
    assert_eq!(kind("glow.index"), "checkbox");
    assert_eq!(kind("bit.index"), "checkbox");
    // Two options that aren't off and on say what each is.
    assert_eq!(kind("two.index"), "radios");
    // A control's name is its param's, or for a choice, its value's.
    assert_eq!(param(&info, "glow.index")["control"]["name"], "glow");
    assert_eq!(
        param(&info, "n")["control"]["text"],
        json!(["caption", "value"])
    );
}

#[test]
fn a_sheet_says_what_a_control_says_and_which_kind_it_is() {
    let doc = format!(
        "{VIEW}param q \"arms\" = 8 in 3..16 step 1\nparam p = 1 in 0..2\nparam few in {{1, 2, 3}}\nparam glow in {{0, 1}}"
    );
    let info = info(
        &doc,
        &[
            "style control { text: name }",
            "style q.control { text: caption = value }\nstyle few.control { kind: select }\nstyle glow.control { kind: radios }",
        ],
    );
    assert_eq!(
        param(&info, "q")["control"]["text"],
        json!(["caption", "value"])
    );
    assert_eq!(param(&info, "p")["control"]["text"], json!(["name"]));
    assert_eq!(param(&info, "few.index")["control"]["kind"], "select");
    assert_eq!(param(&info, "glow.index")["control"]["kind"], "radios");
    // A rule for every control sets a kind only where it fits, and `none` takes one back.
    let info = self::info(
        &doc,
        &[
            "style control { kind: checkbox }",
            "style few.control { kind: select }\nstyle few.control { kind: none }",
        ],
    );
    assert_eq!(param(&info, "q")["control"]["kind"], "slider");
    assert_eq!(param(&info, "glow.index")["control"]["kind"], "checkbox");
    assert_eq!(param(&info, "few.index")["control"]["kind"], "radios");
}

#[test]
fn a_rule_for_one_control_has_to_fit_it() {
    let doc = format!("{VIEW}param q = 8 in 3..16 step 1\nparam few in {{1, 2, 3}}");
    assert!(error(&doc, &["style q.control { kind: radios }"]).contains("`q` can't be a radios"));
    assert!(error(&doc, &["style few.control { kind: checkbox }"]).contains("off and on"));
    assert!(error(&doc, &["style nope.control { text: name }"]).contains("unknown param `nope`"));
    assert!(error(&doc, &["style q.control { width: 2px }"]).contains("takes `text` and `kind`"));
    assert!(error(&doc, &["style q.control { kind: dial }"]).contains("not `dial`"));
    assert!(error(&doc, &["style q.control.blue { text: name }"]).contains("reached as `control`"));
    // A param's name alone still names a mark, so a control isn't reached by accident.
    assert!(error(&doc, &["style q { text: name }"]).contains("no mark named `q`"));
}

#[test]
fn a_label_with_no_place_is_a_readout() {
    let doc = format!(
        "{VIEW}param q = 8 in 3..16 step 1\nlet r = q / 3\nlabel r\nlabel ratio = \"ratio {{q/3}}\"\nlabel w = \"$w^2$ = {{(q, 1)}}\""
    );
    let composition = compose(
        &doc,
        &["style w { format: complex; digits: 1 }\nstyle ratio { digits: 3 }"],
    )
    .unwrap();
    let info = composition.info();
    assert_eq!(
        info["readouts"],
        json!([{ "id": "r" }, { "id": "ratio" }, { "id": "w" }])
    );
    let said = composition.readouts(&Inputs::at(0.0), &Estimate).unwrap();
    let texts: Vec<&str> = said.iter().map(|r| r.text.as_str()).collect();
    // A value's readout says its name and value, since nothing else beside the figure says which
    // it is. One with words says them, and each to its own digits and format.
    assert_eq!(texts, ["$r = 2.67$", "ratio 2.667", "$w^2$ = 8 + i"]);
    // A screen reader hears its mathematics as mathematics.
    assert_eq!(said[0].words(), "r = 2.67");
    assert_eq!(said[2].words(), "w squared = 8 + i");
    assert!(said[0].laid.is_some() && said[1].laid.is_none());
    // It's tagged, so a sheet can reach them all, and it's still a label.
    let figure = &composition.panels[0].figure;
    let tags = &figure
        .scene
        .marks
        .iter()
        .find(|m| m.id == "ratio")
        .unwrap()
        .tags;
    assert_eq!(tags[..2], ["label".to_string(), "readout".to_string()]);
    let said = compose(&doc, &["style readout { text: value }"])
        .unwrap()
        .readouts(&Inputs::at(0.0), &Estimate)
        .unwrap();
    assert_eq!(said[0].text, "$2.67$");
    // A reader's values are its values.
    let held = Inputs {
        time: 0.0,
        params: vec![("q".into(), 12.0)],
    };
    assert_eq!(
        composition.readouts(&held, &Estimate).unwrap()[0].text,
        "$r = 4$"
    );
    // It isn't drawn in the figure.
    let prepared = pencil_core::frame::prepare(figure, &Inputs::at(0.0), 200, 200, 1.0).unwrap();
    assert!(prepared.labels.is_empty());
    // In the IR it has no place.
    let ir = figure.scene.to_json();
    let mark = ir["marks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "ratio")
        .unwrap();
    assert_eq!(mark["form"]["readout"], true);
    assert!(mark["form"].get("at").is_none());
}

#[test]
fn a_readout_of_one_value_says_which_value_it_is() {
    let doc = format!(
        "{VIEW}param q = 3 in 1..9 step 1\nlet r = q / 3\nlabel n = \"{{q - 1}}\"\nlabel r = \"ratio\"\nlabel k = \"$k_1$ is {{q^2}}\""
    );
    let said = compose(&doc, &[])
        .unwrap()
        .readouts(&Inputs::at(0.0), &Estimate)
        .unwrap();
    let texts: Vec<&str> = said.iter().map(|r| r.text.as_str()).collect();
    // A value with no words is said by what's between its braces, as `text: name = value` says
    // it, and a value's label with words says them with its value.
    assert_eq!(texts, ["$q − 1 = 2$", "ratio = $1$", "$k_1$ is 9"]);
    let words: Vec<String> = said.iter().map(|r| r.words()).collect();
    assert_eq!(words, ["q − 1 = 2", "ratio = 1", "k sub 1 is 9"]);
}

#[test]
fn a_readouts_words_read_as_mathematics() {
    let said = |text: &str| pencil_core::formula::spoken(text);
    assert_eq!(said("$z^2 = 1$"), "z squared = 1");
    assert_eq!(said("$x^3 + y^n$"), "x cubed + y to the power n");
    assert_eq!(said("$(a + b)/2$"), "(a + b) over 2");
    assert_eq!(said("$sqrt(x + 1)$"), "the square root of (x + 1)");
    assert_eq!(said("area $x_1$"), "area x sub 1");
    // A sign that starts a number is the number's.
    assert_eq!(said("$A = -2 + 5ω$"), "A = −2 + 5ω");
    assert_eq!(said("$-x^2$"), "−x squared");
}

#[test]
fn control_is_a_controls_and_nothing_elses() {
    for doc in [
        format!("{VIEW}let control = (0, 0)\ndot control"),
        format!("{VIEW}let P = (0, 0)\ndot P\ntag control"),
        format!("{VIEW}param control = 1 in 0..2"),
    ] {
        let message = error(&doc, &[]);
        assert!(
            message.contains("`control` is the type of a param's control"),
            "{message}"
        );
    }
    // So `style control` is the controls', and says what it takes.
    let doc = format!("{VIEW}param q = 1 in 0..2");
    assert!(error(&doc, &["style control { fill: red }"]).contains("takes `text` and `kind`"));
}

#[test]
fn a_readout_that_doesnt_exist_says_a_dash() {
    let doc = format!(
        "{VIEW}param a = 0 in -2..2\nlet P = intersect(line((-5, a), (5, a)), circle((0, 0), (1, 0)), 1)\nlabel P"
    );
    let composition = compose(&doc, &["style P { digits: 1 }"]).unwrap();
    let held = |a: f64| Inputs {
        time: 0.0,
        params: vec![("a".into(), a)],
    };
    assert_eq!(
        composition.readouts(&held(0.0), &Estimate).unwrap()[0].text,
        "$P = (1, 0)$"
    );
    assert_eq!(
        composition.readouts(&held(1.5), &Estimate).unwrap()[0].text,
        "—"
    );
}

#[test]
fn a_readout_is_the_whole_figures() {
    let group = format!("{VIEW}group g() {{\n  label n = \"hi\"\n}}\nuse g()");
    assert!(error(&group, &[]).contains("outside any group"));
    let panel = "let one = 1\npanel a {\n  viewport { x: -2..2, y: -2..2 }\n  label n = \"hi\"\n}";
    assert!(error(panel, &[]).contains("outside any panel"));
    assert!(
        parse::parse("label n = \"hi\" away (1, 0)")
            .unwrap_err()
            .to_string()
            .contains("readout")
    );
    // With no words and no value, it says so, without asking for a place.
    let bare = error(&format!("{VIEW}label n"), &[]);
    assert!(bare.contains("write `label n = \"…\"`"), "{bare}");
    // A figure with panels says its readouts beside all of them.
    let panels = "param q = 3 in 1..5 step 1\nlabel count = \"{q} arms\"\npanel a {\n  viewport { x: -2..2, y: -2..2 }\n}\npanel b {\n  viewport { x: -2..2, y: -2..2 }\n}";
    let composition = compose(panels, &[]).unwrap();
    assert_eq!(composition.info()["readouts"], json!([{ "id": "count" }]));
    assert_eq!(
        composition.readouts(&Inputs::at(0.0), &Estimate).unwrap()[0].text,
        "3 arms"
    );
}

#[test]
fn a_readouts_room_holds_the_most_it_says() {
    let doc = format!("{VIEW}param q = 3 in 1..1000 step 1\nlet r = q / 7\nlabel r");
    let composition = compose(&doc, &[]).unwrap();
    let room = composition.readout_room(&Estimate, None).unwrap();
    let at = |q: f64| {
        let held = Inputs {
            time: 0.0,
            params: vec![("q".into(), q)],
        };
        composition.readouts(&held, &Estimate).unwrap()[0].width
    };
    assert_eq!(room.len(), 1);
    for q in [1.0, 3.0, 500.0, 999.0, 1000.0] {
        assert!(room[0][0] >= at(q), "{} < {} at {q}", room[0][0], at(q));
    }
    assert!(room[0][1] >= pencil_core::frame::READOUT_LINE);
    // One that follows time is tried through its clock.
    let timed = format!("{VIEW}duration 10\nlet t = time·100\nlabel t");
    let composition = compose(&timed, &[]).unwrap();
    let room = composition.readout_room(&Estimate, None).unwrap();
    let late = composition.readouts(&Inputs::at(9.99), &Estimate).unwrap()[0].width;
    assert!(room[0][0] >= late);
}

/// Every value an unstepped param can take, between the ends that are tried, and the moment a page
/// starts at, fit the room.
#[test]
fn a_readouts_room_holds_values_between_the_ends_and_where_a_page_starts() {
    let doc = format!(
        "{VIEW}param t = 0 in 0..1\nparam theta \"angle\" = 0 in 0..2·π\nlabel sq = \"t² = {{t·t}}\"\nlabel c = \"cos θ = {{cos(theta)}}\""
    );
    let composition = compose(&doc, &[]).unwrap();
    let room = composition.readout_room(&Estimate, None).unwrap();
    let held = |t: f64, theta: f64| Inputs {
        time: 0.0,
        params: vec![("t".into(), t), ("theta".into(), theta)],
    };
    for k in 0..=200 {
        let (t, theta) = (
            k as f64 / 200.0,
            2.0 * std::f64::consts::PI * k as f64 / 200.0,
        );
        let said = composition.readouts(&held(t, theta), &Estimate).unwrap();
        for (room, readout) in room.iter().zip(&said) {
            assert!(
                room[0] + 1e-9 >= readout.width,
                "{} doesn't fit {}",
                readout.text,
                room[0]
            );
        }
    }
    // What a build or a page holds is tried too: here a value near a pole that none of the
    // places tried comes near.
    let doc = format!("{VIEW}param a = 0 in 0..1\nlabel n = \"{{1/(a - 0.3001)}}\"");
    let composition = compose(&doc, &[]).unwrap();
    let asked = Inputs {
        time: 0.0,
        params: vec![("a".into(), 0.3)],
    };
    let width = composition.readouts(&asked, &Estimate).unwrap()[0].width;
    assert!(composition.readout_room(&Estimate, None).unwrap()[0][0] < width);
    assert!(composition.readout_room(&Estimate, Some(&asked)).unwrap()[0][0] >= width);
}
