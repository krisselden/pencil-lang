use std::fmt;

/// A compile error. `line` and `col` are 1-based, and 0 means no source position.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub line: usize,
    pub col: usize,
    pub message: String,
    /// Which style sheet it's in, by its place in the list a figure was built with. `None` is
    /// the document itself, or no source at all.
    pub sheet: Option<usize>,
    /// The file it's in, when that's one the document or a sheet uses, not the document or sheet
    /// itself. `line` and `col` are in that file.
    pub file: Option<String>,
    /// Not a failure: a value that doesn't exist this frame, such as where two curves
    /// don't meet. It travels like an error so that anything built from it is absent too.
    pub absent: bool,
    /// Another line the message names, as `line N` with this number, which is where it is while
    /// a figure compiles. Placing the error says it in that line's own file.
    pub also: Option<usize>,
}

impl Error {
    pub fn at(line: usize, col: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            col,
            message: message.into(),
            sheet: None,
            file: None,
            absent: false,
            also: None,
        }
    }

    /// How a message names another line, `line`, for the error to place along with its own.
    pub fn line_text(line: usize) -> String {
        format!("line {line}")
    }

    /// Says the message names another line, which it wrote with `line_text`.
    pub fn also(mut self, line: usize) -> Self {
        self.also = Some(line);
        self
    }

    pub fn absent() -> Self {
        Self {
            absent: true,
            ..Self::new("this value doesn't exist here")
        }
    }

    pub fn new(message: impl Into<String>) -> Self {
        Self::at(0, 0, message)
    }

    /// Says which sheet this came from, unless it already says.
    pub fn in_sheet(mut self, sheet: usize) -> Self {
        self.sheet.get_or_insert(sheet);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}:")?;
        }
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.line, self.col, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for Error {}
