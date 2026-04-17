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
    /// User-level exception carrying an arbitrary Value payload. Raised by
    /// `(throw x)`, caught by `(try ... (catch _ e ...))`. Chose to embed
    /// a Value rather than a new exception type so user code can throw
    /// anything — maps, strings, ex-info records — and pattern-match on
    /// the data in the catch clause.
    Thrown(Value),
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
            Error::Thrown(v) => write!(f, "thrown: {}", v.to_display_string()),
        }
    }
}

impl std::error::Error for Error {}
