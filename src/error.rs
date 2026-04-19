use std::fmt;
use std::sync::Arc;

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
    /// Wrapper attaching source location (1-based line/col) and an
    /// optional Clojure call stack to an inner error. The reader/eval
    /// pipeline produces these at the form that triggered the failure;
    /// downstream callers should `peel`/`peel_ref` before pattern-matching
    /// on the original kind, since `Recur`, `Thrown`, etc. are still the
    /// payload that controls catch / loop semantics.
    Located {
        inner: Box<Error>,
        line: u32,
        col: u32,
        stack: Vec<Arc<str>>,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Strip any Located wrappers, returning the innermost error.
    pub fn peel(self) -> Error {
        match self {
            Error::Located { inner, .. } => inner.peel(),
            other => other,
        }
    }

    /// Borrow the innermost error, peeling Located wrappers.
    pub fn peel_ref(&self) -> &Error {
        match self {
            Error::Located { inner, .. } => inner.peel_ref(),
            other => other,
        }
    }

    /// True if this error is (or wraps) a Recur control-flow signal.
    pub fn is_recur(&self) -> bool {
        matches!(self.peel_ref(), Error::Recur(_))
    }

    /// Attach line/col + an optional call stack. If the error already has
    /// a Located wrapper we leave it alone — the innermost wrap is the
    /// most precise.
    pub fn at(self, line: u32, col: u32, stack: Vec<Arc<str>>) -> Error {
        match self {
            // Don't wrap control-flow signals; they're consumed by the
            // interpreter, never displayed to the user.
            Error::Recur(_) => self,
            Error::Located { .. } => self,
            other => Error::Located {
                inner: Box::new(other),
                line,
                col,
                stack,
            },
        }
    }
}

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
            Error::Located { inner, line, col, stack } => {
                write!(f, "{line}:{col}: {inner}")?;
                if !stack.is_empty() {
                    write!(f, " (in")?;
                    for (i, name) in stack.iter().enumerate() {
                        if i > 0 {
                            write!(f, " <-")?;
                        }
                        write!(f, " {name}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Error {}
