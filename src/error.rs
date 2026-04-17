use std::fmt;

use crate::value::Value;

#[derive(Debug, Clone)]
pub enum Error {
    Read(String),
    Eval(String),
    Arity { expected: String, got: usize },
    Type(String),
    Unbound(String),
    /// Control-flow signal for `recur`. Bubbles up until caught by the
    /// nearest enclosing `loop` or fn call frame. If it escapes, it means
    /// `recur` was used outside any valid target — surfaces as a user error.
    Recur(Vec<Value>),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Read(s) => write!(f, "read error: {s}"),
            Error::Eval(s) => write!(f, "eval error: {s}"),
            Error::Arity { expected, got } => {
                write!(f, "arity error: expected {expected}, got {got}")
            }
            Error::Type(s) => write!(f, "type error: {s}"),
            Error::Unbound(s) => write!(f, "unbound symbol: {s}"),
            Error::Recur(_) => write!(f, "recur used outside tail position of loop/fn"),
        }
    }
}

impl std::error::Error for Error {}
