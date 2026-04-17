use std::fmt;
use std::sync::Arc;

use crate::env::Env;
use crate::error::Result;
use crate::native::NativeFn;

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Arc<str>),
    Symbol(Arc<str>),
    Keyword(Arc<str>),
    List(Arc<Vec<Value>>),
    Vector(Arc<Vec<Value>>),
    Map(Arc<Vec<(Value, Value)>>),
    Fn(Arc<Lambda>),
    Macro(Arc<Lambda>),
    Builtin(Builtin),
    /// JIT-compiled native function. Created by `defn-native` when the
    /// `mlir` feature is on. Dispatched in `eval::apply` with a direct
    /// transmuted `extern "C"` call — no interpreter frame at invocation.
    Native(Arc<NativeFn>),
}

#[derive(Clone)]
pub struct Builtin {
    pub name: &'static str,
    pub f: fn(&[Value]) -> Result<Value>,
}

pub struct Lambda {
    pub params: Vec<Arc<str>>,
    pub variadic: Option<Arc<str>>,
    pub body: Arc<Vec<Value>>,
    pub env: Env,
    pub name: Option<Arc<str>>,
}

impl Value {
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::Keyword(_) => "keyword",
            Value::List(_) => "list",
            Value::Vector(_) => "vector",
            Value::Map(_) => "map",
            Value::Fn(_) => "fn",
            Value::Macro(_) => "macro",
            Value::Builtin(_) => "builtin",
            Value::Native(_) => "native",
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            _ => self.to_pr_string(),
        }
    }

    pub fn to_pr_string(&self) -> String {
        match self {
            Value::Nil => "nil".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_finite() && f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Value::Str(s) => format!("\"{}\"", escape_string(s)),
            Value::Symbol(s) => s.to_string(),
            Value::Keyword(s) => format!(":{s}"),
            Value::List(v) => format!(
                "({})",
                v.iter()
                    .map(Value::to_pr_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Value::Vector(v) => format!(
                "[{}]",
                v.iter()
                    .map(Value::to_pr_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Value::Map(m) => {
                let parts: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{} {}", k.to_pr_string(), v.to_pr_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Fn(lam) => match &lam.name {
                Some(n) => format!("#<fn {n}>"),
                None => "#<fn>".into(),
            },
            Value::Macro(lam) => match &lam.name {
                Some(n) => format!("#<macro {n}>"),
                None => "#<macro>".into(),
            },
            Value::Builtin(b) => format!("#<builtin {}>", b.name),
            Value::Native(nf) => format!("#<native {}>", nf.name),
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_pr_string())
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_pr_string())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Int(a), Float(b)) => (*a as f64) == *b,
            (Float(a), Int(b)) => *a == (*b as f64),
            (Str(a), Str(b)) => a == b,
            (Symbol(a), Symbol(b)) => a == b,
            (Keyword(a), Keyword(b)) => a == b,
            (List(a), List(b)) | (Vector(a), Vector(b)) => a == b,
            // Clojure: lists and vectors compare equal if same length + same elements in order
            (List(a), Vector(b)) | (Vector(a), List(b)) => **a == **b,
            (Map(a), Map(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter()
                    .all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            _ => false,
        }
    }
}
