//! A compiled figure written to bytes and read back is the same figure, and a file that isn't
//! this build's is an error and never a wrong figure.

use std::path::{Path, PathBuf};

use pencil_core::compiled::FINGERPRINT;
use pencil_core::eval::Frame;
use pencil_core::frame::Inputs;
use pencil_core::{Composition, Source, Sources};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles a document against sheets, all named from the repository root, reading the files
/// they use as the CLI does.
fn compose(pen: &str, sheets: &[&str]) -> Result<Composition, pencil_core::Error> {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).ok();
    let texts: Vec<String> = sheets.iter().map(|s| read(s).unwrap()).collect();
    let sheets: Vec<Source> = sheets
        .iter()
        .zip(&texts)
        .map(|(name, text)| Source { name, text })
        .collect();
    let doc = read(pen).unwrap();
    Sources::read(
        Source {
            name: pen,
            text: &doc,
        },
        &sheets,
        &read,
    )?
    .compose()
}

/// Every panel's CPU oracle pixels at a few times, in a small frame.
fn pixels(c: &Composition) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for panel in &c.panels {
        for time in [0.0, 0.7, 2.3] {
            let frame = Frame::new(&panel.figure, &Inputs::at(time), 24, 18);
            out.push(frame.and_then(|f| f.rgba()).unwrap_or_default());
        }
    }
    out
}

fn same(name: &str, a: &Composition, b: &Composition) {
    assert_eq!(a.panels.len(), b.panels.len(), "{name}");
    assert_eq!(a.size, b.size, "{name}");
    for (x, y) in a.panels.iter().zip(&b.panels) {
        assert_eq!(
            x.figure.scene.to_json(),
            y.figure.scene.to_json(),
            "{name}: IR"
        );
        // The JSON leaves out what the sheets add and some of the viewport, and the whole of
        // each is what a figure is, so the rest is compared as it is.
        assert_eq!(x.figure.scene, y.figure.scene, "{name}: scene");
        assert_eq!(x.figure.styles, y.figure.styles, "{name}: styles");
        assert_eq!(x.figure.conditional, y.figure.conditional, "{name}");
        assert_eq!(
            (&x.id, &x.title, &x.formula),
            (&y.id, &y.title, &y.formula),
            "{name}"
        );
        assert_eq!(x.rect, y.rect, "{name}");
    }
    assert_eq!(pixels(a), pixels(b), "{name}: pixels");
}

/// The sheet sets the corpus is drawn with: each sheet beside a figure alone and under the theme,
/// and the figure with no sheet. The explainer's four sheets, with the dark one, are its own
/// case. A set a figure's rules don't fit is a compile error, which has nothing to write.
fn corpus_cases() -> Vec<(String, Vec<String>)> {
    let theme = "corpus/theme.style";
    let mut cases = Vec::new();
    let mut dirs: Vec<_> = std::fs::read_dir(root().join("corpus"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_str().unwrap().to_string();
        let files = |extension: &str| {
            let mut found: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|e| e == extension))
                .map(|p| format!("corpus/{name}/{}", p.file_name().unwrap().to_str().unwrap()))
                .collect();
            found.sort();
            found
        };
        for pen in files("pen") {
            cases.push((pen.clone(), vec![]));
            for style in files("style") {
                let sets = [vec![style.clone()], vec![theme.into(), style.clone()]];
                for sheets in sets {
                    cases.push((pen.clone(), sheets));
                }
            }
        }
    }
    cases
}

fn explainer_case() -> (String, Vec<String>) {
    (
        "lab/explainer/limacon.pen".into(),
        [
            "corpus/theme.style",
            "corpus/dark.style",
            "lab/explainer/limacon.style",
            "lab/explainer/caption.style",
        ]
        .map(String::from)
        .into(),
    )
}

#[test]
fn every_figure_reads_back_as_itself() {
    let (mut compiled, mut pens) = (0, std::collections::HashSet::new());
    let mut cases = corpus_cases();
    cases.push(explainer_case());
    for (pen, sheets) in cases {
        let sheets: Vec<&str> = sheets.iter().map(String::as_str).collect();
        let Ok(original) = compose(&pen, &sheets) else {
            continue;
        };
        let name = format!("{pen} with {sheets:?}");
        let bytes = original.to_bytes().unwrap();
        let back = Composition::from_bytes(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        same(&name, &original, &back);
        // Written again from what was read, and from a second compile (whose maps hash
        // differently), it's the same bytes.
        assert_eq!(bytes, back.to_bytes().unwrap(), "{name}: written again");
        assert_eq!(
            bytes,
            compose(&pen, &sheets).unwrap().to_bytes().unwrap(),
            "{name}: compiled again"
        );
        compiled += 1;
        pens.insert(pen);
    }
    assert!(compiled > 30, "only {compiled} figures compiled");
    assert!(pens.contains("lab/explainer/limacon.pen"));
    // Every figure in the corpus is covered by at least one set.
    let wanted: Vec<String> = corpus_cases().into_iter().map(|(pen, _)| pen).collect();
    for pen in wanted {
        assert!(pens.contains(&pen), "{pen} compiled with no set of sheets");
    }
}

/// A document that uses files, so the bytes hold what they said and nothing is read back from disk.
#[test]
fn a_compiled_figure_needs_no_files() {
    let (pen, sheets) = explainer_case();
    let sheets: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let bytes = compose(&pen, &sheets).unwrap().to_bytes().unwrap();
    let back = Composition::from_bytes(&bytes).unwrap();
    assert!(!back.panels[0].figure.scene.marks.is_empty());
}

/// Panels with the same equations share their answers after reading, as they do after compiling.
#[test]
fn panels_share_their_answers_after_reading() {
    let sheets = ["corpus/theme.style", "corpus/007-lattice/plain.style"];
    let made = compose("corpus/007-lattice/lattice.pen", &sheets).unwrap();
    let back = Composition::from_bytes(&made.to_bytes().unwrap()).unwrap();
    let shares = |c: &Composition| -> Vec<bool> {
        let first = &c.panels[0].figure.solved;
        c.panels
            .iter()
            .skip(1)
            .map(|p| std::sync::Arc::ptr_eq(&p.figure.solved, first))
            .collect()
    };
    assert!(
        shares(&made).contains(&true),
        "the figure has no panels that share"
    );
    assert_eq!(shares(&made), shares(&back));
}

fn sample() -> Vec<u8> {
    compose(
        "corpus/012-limacon/limacon.pen",
        &["corpus/012-limacon/plain.style"],
    )
    .unwrap()
    .to_bytes()
    .unwrap()
}

fn refused(bytes: &[u8]) -> String {
    match Composition::from_bytes(bytes) {
        Ok(_) => panic!("read bytes that aren't this build's figure"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn another_builds_figure_is_refused_with_a_reason() {
    let mut bytes = sample();
    let other = (FINGERPRINT ^ 1).to_le_bytes();
    bytes[8..16].copy_from_slice(&other);
    let said = refused(&bytes);
    assert!(
        said.contains("compiled by a different build of Pencil")
            && said.contains("compile it again"),
        "{said}"
    );
}

#[test]
fn cut_off_bytes_are_refused_wherever_they_end() {
    let bytes = sample();
    for end in (0..bytes.len()).step_by(7).chain([bytes.len() - 1]) {
        refused(&bytes[..end]);
    }
    let mut longer = bytes.clone();
    longer.push(0);
    refused(&longer);
}

#[test]
fn bytes_that_are_not_a_figure_are_refused() {
    refused(b"");
    refused(b"PENCILFG");
    refused(b"not a figure at all, but long enough to have a header in it");
    // Pseudo-random bytes, from a fixed seed so a failure can be run again.
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for length in [1, 23, 24, 25, 100, 5000] {
        let bytes: Vec<u8> = (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        refused(&bytes);
    }
}

/// With a good header on a bad body, which a bit flipped in a real file looks like.
#[test]
fn a_damaged_body_is_an_error_and_never_a_panic() {
    let bytes = sample();
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut flipped = 0;
    for _ in 0..300 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let mut damaged = bytes.clone();
        let at = 24 + (state as usize) % (bytes.len() - 24);
        damaged[at] ^= 1 << ((state >> 40) % 8);
        // Some changes are to a number and give another figure, which is all a checksum would
        // be needed for. What matters is that none of them panics.
        if Composition::from_bytes(&damaged).is_err() {
            flipped += 1;
        }
    }
    assert!(flipped > 0);
}

/// Sizes and times, for the table in the docs. Run it on a release build:
/// `PENCIL_BENCH_OUT=dir cargo test --release -p pencil-core --test compiled -- --ignored
/// --nocapture bench`. It prints a row a figure, and with the variable set it leaves each
/// figure's sources, all in one file, and its compiled bytes in that directory, for gzip and
/// brotli to measure.
#[test]
#[ignore = "a measurement, not a test"]
fn bench() {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::time::Instant;

    let theme = "corpus/theme.style";
    let mut cases: Vec<(String, Vec<String>)> = vec![
        (
            "corpus/001-circle/circle.pen".into(),
            vec!["corpus/001-circle/circle.style".into()],
        ),
        (
            "corpus/002-strokes/ring.pen".into(),
            vec!["corpus/002-strokes/filled.style".into()],
        ),
        (
            "corpus/002-strokes/wave.pen".into(),
            vec!["corpus/002-strokes/taper.style".into()],
        ),
        (
            "corpus/003-unroll/unroll.pen".into(),
            vec!["corpus/003-unroll/blog.style".into()],
        ),
        (
            "corpus/004-bounds/bounds.pen".into(),
            vec!["corpus/004-bounds/blog.style".into()],
        ),
    ];
    for (n, name) in [
        ("005-tangents", "tangents"),
        ("006-square", "square"),
        ("007-lattice", "lattice"),
        ("008-hearts", "hearts"),
        ("009-squaring", "squaring"),
        ("010-mobius", "mobius"),
        ("011-circumcircle", "circumcircle"),
        ("012-limacon", "limacon"),
    ] {
        cases.push((
            format!("corpus/{n}/{name}.pen"),
            vec![theme.into(), format!("corpus/{n}/plain.style")],
        ));
    }
    cases.push(explainer_case());

    let out = std::env::var_os("PENCIL_BENCH_OUT").map(PathBuf::from);
    println!(
        "{:<34} {:>9} {:>9} {:>11} {:>11}",
        "figure", "source B", "compiled B", "compile µs", "read µs"
    );
    for (pen, sheets) in cases {
        let disk = |p: &str| std::fs::read_to_string(root().join(p)).ok();
        // Everything read once, so the timings are of parsing and compiling and not of the disk.
        let used = RefCell::new(BTreeMap::<String, String>::new());
        let doc = disk(&pen).unwrap();
        let texts: Vec<String> = sheets.iter().map(|s| disk(s).unwrap()).collect();
        let read = |p: &str| {
            let text = disk(p)?;
            used.borrow_mut().insert(p.to_string(), text.clone());
            Some(text)
        };
        let source_list: Vec<Source> = sheets
            .iter()
            .zip(&texts)
            .map(|(name, text)| Source { name, text })
            .collect();
        let sources = Sources::read(
            Source {
                name: &pen,
                text: &doc,
            },
            &source_list,
            &read,
        )
        .unwrap();
        let made = sources.compose().unwrap();
        let files = used.into_inner();
        let source_bytes = doc.len()
            + texts.iter().map(String::len).sum::<usize>()
            + files.values().map(String::len).sum::<usize>();
        let bytes = made.to_bytes().unwrap();

        let from_memory = |p: &str| std::hint::black_box(files.get(p).cloned());
        let median = |mut run: Box<dyn FnMut()>| {
            let mut times = Vec::new();
            let start = Instant::now();
            while times.len() < 30 || (start.elapsed().as_secs_f64() < 1.0 && times.len() < 2000) {
                let t = Instant::now();
                run();
                times.push(t.elapsed().as_secs_f64() * 1e6);
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            times[times.len() / 2]
        };
        let compile = median(Box::new(|| {
            let s = Sources::read(
                Source {
                    name: &pen,
                    text: &doc,
                },
                &source_list,
                &from_memory,
            )
            .unwrap();
            std::hint::black_box(s.compose().unwrap());
        }));
        let read_back = median(Box::new(|| {
            std::hint::black_box(Composition::from_bytes(std::hint::black_box(&bytes)).unwrap());
        }));
        println!(
            "{:<34} {:>9} {:>9} {:>11.0} {:>11.0}",
            pen.trim_start_matches("corpus/"),
            source_bytes,
            bytes.len(),
            compile,
            read_back
        );

        if let Some(dir) = &out {
            std::fs::create_dir_all(dir).unwrap();
            let stem = pen.replace('/', "_");
            let mut all = doc.clone();
            for text in &texts {
                all.push_str(text);
            }
            for text in files.values() {
                all.push_str(text);
            }
            std::fs::write(dir.join(format!("{stem}.src")), all).unwrap();
            std::fs::write(dir.join(format!("{stem}.pfig")), &bytes).unwrap();
        }
    }
}
