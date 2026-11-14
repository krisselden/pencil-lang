//! `use "file"`: a figure written in more than one file.
//!
//! A used file's statements join the figure where the line is written, in the one namespace a
//! figure has, so a long timeline can be split by section and helpers can be shared. A file joins
//! once: a second `use` of it adds nothing, so two files can each use the helpers they need.
//!
//! The core never opens a file. A host reads them, by the names this module gives it: the CLI
//! from disk, and a page by fetching. A name is the path as written, found from the file that
//! says it, so `use "shared/arms.pen"` in `figures/limacon.pen` is `figures/shared/arms.pen`.

use std::cell::RefCell;

use crate::error::Error;
use crate::parse::{self, Document, Item};

/// Reads a file by name, or says there is none.
pub type Read<'a> = &'a dyn Fn(&str) -> Option<String>;

/// A host with no files: every `use "file"` is a mistake.
pub fn none(_: &str) -> Option<String> {
    None
}

/// Which file each line of a loaded document came from. While a figure compiles, each file's
/// lines are numbered on from the last file's, so a line number alone says which file it's in.
#[derive(Clone, Debug, Default)]
pub struct Origins {
    /// Each file's name and the line before its first, in the order read. The first is the
    /// document itself.
    files: Vec<(usize, String)>,
}

impl Origins {
    /// Gives an error its line in its own file, and names the file unless it's the document.
    pub fn place(&self, mut error: Error) -> Error {
        if error.line == 0 || error.file.is_some() {
            return error;
        }
        if let Some(index) = self.files.iter().rposition(|(base, _)| *base < error.line) {
            let (base, name) = &self.files[index];
            error.line -= base;
            if index > 0 {
                error.file = Some(name.clone());
            }
        }
        error
    }

    /// Every file read, in order, after the document itself.
    pub fn used(&self) -> impl Iterator<Item = &str> {
        self.files.iter().skip(1).map(|(_, name)| name.as_str())
    }
}

struct Loader<'a> {
    read: Read<'a>,
    origins: Origins,
    /// The lines numbered so far.
    lines: usize,
    /// The files being read, outermost first, to catch one that uses itself.
    open: Vec<String>,
}

impl Loader<'_> {
    fn file(&mut self, name: &str, text: &str) -> Result<Document, Error> {
        let base = self.lines;
        self.lines += text.matches('\n').count() + 1;
        self.origins.files.push((base, name.to_string()));
        let mut document = parse::parse_numbered_from(text, base)?;
        self.open.push(name.to_string());
        document.items = self.items(document.items, name)?;
        self.open.pop();
        Ok(document)
    }

    /// `items` with each `use "file"` replaced by that file's statements, at any depth.
    fn items(&mut self, items: Vec<Item>, from: &str) -> Result<Vec<Item>, Error> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Item::UseFile { path, line, col } => {
                    let at = |message: String| Error::at(line, col, message);
                    let name = joined(from, &path).map_err(at)?;
                    if self.open.contains(&name) {
                        return Err(at(format!("`{name}` uses itself")));
                    }
                    if self.origins.files.iter().any(|(_, read)| *read == name) {
                        continue;
                    }
                    let text = (self.read)(&name).ok_or_else(|| at(format!("no file `{name}`")))?;
                    // Its `doc` name, if it has one, is its own: it's a figure by itself too.
                    out.extend(self.file(&name, &text)?.items);
                }
                Item::Panel {
                    name,
                    title,
                    formula,
                    items,
                } => out.push(Item::Panel {
                    name,
                    title,
                    formula,
                    items: self.items(items, from)?,
                }),
                Item::Group {
                    name,
                    params,
                    items,
                } => out.push(Item::Group {
                    name,
                    params,
                    items: self.items(items, from)?,
                }),
                other => out.push(other),
            }
        }
        Ok(out)
    }
}

/// Parses `text`, which is found by `name`, with every file it uses read in. An error says which
/// file it's in.
pub fn load(name: &str, text: &str, read: Read) -> Result<(Document, Origins), Error> {
    let mut loader = Loader {
        read,
        origins: Origins::default(),
        lines: 0,
        open: Vec::new(),
    };
    match loader.file(name, text) {
        Ok(document) => Ok((document, loader.origins)),
        Err(error) => Err(loader.origins.place(error)),
    }
}

/// The files `text` uses itself, by name, for a host that has to fetch them before it compiles.
/// Text that doesn't parse uses none: compiling it says what's wrong.
pub fn uses(name: &str, text: &str) -> Vec<String> {
    let found = RefCell::new(Vec::new());
    let _ = load(name, text, &|file| {
        found.borrow_mut().push(file.to_string());
        Some(String::new())
    });
    found.into_inner()
}

/// `path`, as written in the file named `from`, as a name of its own: `from`'s folder, then the
/// path, with each `..` taken up.
fn joined(from: &str, path: &str) -> Result<String, String> {
    if path.is_empty() {
        return Err("`use` takes a file's name".to_string());
    }
    if path.starts_with('/') || path.contains(':') || path.contains('\\') {
        return Err(format!(
            "`{path}` isn't found from this file: write where it is from here, as in `shared/arms.pen` or `../shared/arms.pen`"
        ));
    }
    let folder = from.rfind('/').map_or("", |slash| &from[..slash + 1]);
    let mut parts: Vec<&str> = folder.split('/').filter(|part| *part != ".").collect();
    // The split's last piece is what follows the folder's slash: nothing.
    parts.pop();
    // What `..` can't step out of: the root of a path that starts at one, or a URL's host.
    let root = match parts[..] {
        [scheme, "", _, ..] if scheme.ends_with(':') => 3,
        ["", ..] => 1,
        _ => 0,
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last() {
                Some(last) if parts.len() > root && *last != ".." => {
                    parts.pop();
                }
                // Above a root there's nothing, as in a URL. Elsewhere it stays as written.
                _ if root > 0 => {}
                _ => parts.push(".."),
            },
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host whose files are these.
    fn among<'a>(files: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            files
                .iter()
                .find(|(file, _)| *file == name)
                .map(|(_, text)| text.to_string())
        }
    }

    fn names(document: &Document) -> Vec<String> {
        document
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Let { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_used_file_joins_where_its_line_is() {
        let files = [("parts/b.pen", "let b = 2\nlet c = 3")];
        let (document, origins) =
            load("a.pen", "let a = 1\nuse \"parts/b.pen\"\nlet d = 4", &among(&files)).unwrap();
        assert_eq!(names(&document), ["a", "b", "c", "d"]);
        assert_eq!(origins.used().collect::<Vec<_>>(), ["parts/b.pen"]);
    }

    #[test]
    fn a_file_is_found_from_the_file_that_uses_it() {
        let files = [
            ("figures/parts/b.pen", "use \"../shared/c.pen\"\nlet b = 2"),
            ("figures/shared/c.pen", "let c = 3"),
        ];
        let (document, _) =
            load("figures/a.pen", "use \"parts/b.pen\"", &among(&files)).unwrap();
        assert_eq!(names(&document), ["c", "b"]);

        assert_eq!(joined("a.pen", "b.pen").unwrap(), "b.pen");
        assert_eq!(joined("./a.pen", "./b.pen").unwrap(), "b.pen");
        assert_eq!(joined("x/y/a.pen", "../../b.pen").unwrap(), "b.pen");
        assert_eq!(joined("x/a.pen", "../../b.pen").unwrap(), "../b.pen");
        assert_eq!(joined("../x/a.pen", "b.pen").unwrap(), "../x/b.pen");
        assert_eq!(joined("/x/a.pen", "y/b.pen").unwrap(), "/x/y/b.pen");
        assert_eq!(
            joined("https://example.com/x/a.pen", "../../../b.pen").unwrap(),
            "https://example.com/b.pen"
        );
        assert_eq!(joined("/x/a.pen", "../../b.pen").unwrap(), "/b.pen");
        for path in ["/b.pen", "https://example.com/b.pen", ""] {
            assert!(joined("a.pen", path).is_err(), "{path}");
        }
    }

    #[test]
    fn a_file_joins_once() {
        // Both parts use the helpers, and the figure has them once, where they were first used.
        let files = [
            ("one.pen", "use \"helpers.pen\"\nlet one = 1"),
            ("two.pen", "use \"helpers.pen\"\nlet two = 2"),
            ("helpers.pen", "let helper = 0"),
        ];
        let (document, origins) =
            load("a.pen", "use \"one.pen\"\nuse \"two.pen\"", &among(&files)).unwrap();
        assert_eq!(names(&document), ["helper", "one", "two"]);
        assert_eq!(origins.used().count(), 3);
    }

    #[test]
    fn a_file_inside_a_panel_or_group_joins_there() {
        let files = [("b.pen", "let b = 2")];
        let (document, _) =
            load("a.pen", "panel left {\n  use \"b.pen\"\n}", &among(&files)).unwrap();
        let [Item::Panel { items, .. }] = &document.items[..] else {
            panic!("{:?}", document.items);
        };
        assert!(matches!(&items[..], [Item::Let { name, .. }] if name == "b"));
    }

    #[test]
    fn an_error_says_which_file_and_line() {
        let files = [
            ("b.pen", "let b = 2\nlet c = )"),
            ("loop.pen", "use \"a.pen\""),
        ];
        let read = among(&files);
        let error = |text: &str| load("a.pen", text, &read).unwrap_err();

        // In a used file: that file's own line, and its name.
        let inside = error("let a = 1\n\n\nuse \"b.pen\"");
        assert_eq!((inside.file.as_deref(), inside.line), (Some("b.pen"), 2));
        assert!(inside.to_string().starts_with("b.pen:2:"), "{inside}");

        // In the document, after a used file: the document's line, and no file.
        let files = [("b.pen", "let b = 2\nlet c = 3")];
        let after = load("a.pen", "use \"b.pen\"\nlet a = 1\nlet d = )", &among(&files)).unwrap_err();
        assert_eq!((after.file.as_deref(), after.line), (None, 3));

        // A file that isn't there, at the line that uses it.
        let missing = error("let a = 1\nuse \"parts/gone.pen\"");
        assert_eq!((missing.line, missing.col), (2, 5));
        assert_eq!(missing.message, "no file `parts/gone.pen`");

        // A file that uses the one using it.
        let looped = error("use \"loop.pen\"");
        assert_eq!(looped.file.as_deref(), Some("loop.pen"));
        assert_eq!(looped.message, "`a.pen` uses itself");
    }

    #[test]
    fn a_mistake_found_while_compiling_is_placed_too() {
        use crate::{Source, Sources};
        let doc = Source {
            name: "figures/a.pen",
            text: "viewport { x: -1..1, y: -1..1 }\nuse \"parts.pen\"\nlet O = (0, 0)\ndot O",
        };
        let sheet = |text| Source {
            name: "sheets/plain.style",
            text,
        };
        let error = |files: &[(&str, &str)], sheets: &[Source]| {
            let error = Sources::read(doc, sheets, &among(files)).unwrap().build().unwrap_err();
            (error.sheet, error.file.clone(), error.line)
        };

        // An unknown name in the used file, which only compiling finds.
        assert_eq!(
            error(&[("figures/parts.pen", "let a = 1\n\ndot P")], &[]),
            (None, Some("figures/parts.pen".to_string()), 3)
        );

        // A rule for a mark that isn't there, in a file a sheet uses.
        let parts = ("figures/parts.pen", "let a = 1");
        assert_eq!(
            error(
                &[parts, ("sheets/shared.style", "\nstyle Q { fill: black }")],
                &[sheet("style O { fill: black }\nuse \"shared.style\"")]
            ),
            (Some(0), Some("sheets/shared.style".to_string()), 2)
        );

        // And one after the used file, in the sheet itself.
        assert_eq!(
            error(
                &[parts, ("sheets/shared.style", "style O { fill: black }")],
                &[sheet("use \"shared.style\"\nstyle R { fill: black }")]
            ),
            (Some(0), None, 2)
        );
    }

    #[test]
    fn a_host_asks_which_files_a_text_uses() {
        assert_eq!(
            uses("figures/a.pen", "use \"b.pen\"\nlet a = 1\nuse \"../c.pen\"\nuse \"b.pen\""),
            ["figures/b.pen", "c.pen"]
        );
        // A label can be called `use`, and its text isn't a file.
        assert_eq!(uses("a.pen", "label use = \"b.pen\" at (0, 0)"), Vec::<String>::new());
        assert_eq!(uses("a.pen", "let a = )"), Vec::<String>::new());
    }
}
