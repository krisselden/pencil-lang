use std::fmt;

/// A compile error. `line` and `col` are 1-based, and 0 means no source position.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub line: usize,
    pub col: usize,
    pub message: String,
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
