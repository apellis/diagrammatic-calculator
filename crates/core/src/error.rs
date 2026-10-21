//! Errors with plain messages, for people and agents alike.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Text that does not parse.
    Syntax,
    /// A well-formed input that is not valid here (an unknown name, a generator that does not fit).
    Invalid,
    /// A precondition of an operation fails (for example a finite-dimensional quotient is needed).
    Precondition,
    /// A computation stopped at a stated limit.
    Limit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    /// Byte offset in the input text, for syntax errors.
    pub offset: Option<usize>,
}

impl Error {
    pub fn syntax(message: impl Into<String>, offset: usize) -> Self {
        Error {
            kind: ErrorKind::Syntax,
            message: message.into(),
            offset: Some(offset),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Invalid,
            message: message.into(),
            offset: None,
        }
    }
    pub fn precondition(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Precondition,
            message: message.into(),
            offset: None,
        }
    }
    pub fn limit(message: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Limit,
            message: message.into(),
            offset: None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(o) => write!(f, "{} (at character {})", self.message, o + 1),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
