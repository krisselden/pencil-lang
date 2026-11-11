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
    /// Not a failure: a value that doesn't exist this frame, such as where two curves
    /// don't meet. It travels like an error so that anything built from it is absent too.
    pub absent: bool,
}

impl Error {
    pub fn at(line: usize, col: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            col,
            message: message.into(),
            sheet: None,
            absent: false,
        }
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
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.line, self.col, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for Error {}
