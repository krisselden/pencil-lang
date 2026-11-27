//! `pencil build` writes what a figure's readouts say at its poster, and the room each needs, from
//! the moment it was asked to start at.

use std::path::{Path, PathBuf};
use std::process::Command;

fn dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("readouts");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A readout near a pole at a = 0.3001, which none of the places tried along `a`'s range comes
/// near, so only the moment asked for, `--param a=0.3`, shows how wide it gets.
const POLE: &str = "viewport { x: -2..2, y: -2..2 }\nparam a = 0 in 0..1\nlet O = (0, 0)\ndot O\n\
                    label n = \"{1/(a - 0.3001)}\"\n";

#[test]
fn a_builds_room_holds_what_its_readouts_say_where_it_was_asked_to_start() {
    let dir = dir();
    let doc = dir.join("pole.pen");
    std::fs::write(&doc, POLE).unwrap();
    let out = dir.join("pole");
    let built = Command::new(env!("CARGO_BIN_EXE_pencil"))
        .args(["build", doc.to_str().unwrap(), "--param", "a=0.3"])
        .args(["--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("pole.json")).unwrap()).unwrap();
    let said = &manifest["poster"]["readouts"][0];
    let room = &manifest["info"]["readouts"][0]["room"];
    assert_eq!(said["words"], "1 over (a − 0.3001) = −10000.00");
    let (width, room) = (said["width"].as_f64().unwrap(), room[0].as_f64().unwrap());
    assert!(room >= width, "room {room} < {width}, what it says");
}
