//! `docs/language.md` is the reference for the language, so it can't fall behind the code.

use std::path::Path;

use pencil_core::parse::FUNCTIONS;
use pencil_core::style::PROPERTIES;

fn reference() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/language.md");
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn every_builtin_function_is_documented() {
    let doc = reference();
    for (name, _) in FUNCTIONS {
        assert!(
            doc.contains(&format!("`{name}(")),
            "`{name}` isn't in docs/language.md"
        );
    }
}

#[test]
fn every_style_property_is_documented() {
    let doc = reference();
    for name in PROPERTIES {
        assert!(
            doc.contains(&format!("`{name}`")),
            "`{name}` isn't in docs/language.md"
        );
    }
}

/// The statements `parse.rs` accepts.
#[test]
fn every_statement_is_documented() {
    let doc = reference();
    let source = include_str!("../src/parse.rs");
    let start = source.find("match word.as_str() {").unwrap();
    let end = source[start..].find("unknown statement").unwrap();
    let arms = &source[start..start + end];
    let mut found = 0;
    // Only this match's own arms: quoted words at its indent, not those of a match inside one.
    for line in arms.lines().filter(|l| l.starts_with("            \"")) {
        let patterns = line.split("=>").next().unwrap();
        for piece in patterns.split('"').skip(1).step_by(2) {
            found += 1;
            assert!(
                doc.contains(&format!("`{piece} ")) || doc.contains(&format!("`{piece}`")),
                "statement `{piece}` isn't in docs/language.md"
            );
        }
    }
    assert!(
        found >= 15,
        "only found {found} statements, so this test has gone stale"
    );
}
