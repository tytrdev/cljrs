//! Mapping from cljrs core forms to Mojo. Keeps all the "which Mojo symbol
//! does `sqrt` become" knowledge in one place so tier lowering stays clean.

use crate::ast::MType;

/// Map a cljrs type-hint symbol (from a `^tag` meta) to an MType. Returns
/// None if the hint isn't one of the known primitives — callers should
/// either pass it through (`Infer`) or error, depending on context.
pub fn type_hint(sym: &str) -> Option<MType> {
    Some(match sym {
        "i32" | "Int32" => MType::Int32,
        "i64" | "long" | "Int64" => MType::Int64,
        "f32" | "Float32" => MType::Float32,
        "f64" | "double" | "Float64" => MType::Float64,
        "bool" | "Bool" => MType::Bool,
        _ => return None,
    })
}

/// If `sym` is an infix arithmetic/comparison operator in both cljrs and
/// Mojo, return the Mojo spelling. Otherwise None.
pub fn binop(sym: &str) -> Option<&'static str> {
    Some(match sym {
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "/",
        "mod" => "%",
        "rem" => "%",
        "quot" => "//",
        "<" => "<",
        ">" => ">",
        "<=" => "<=",
        ">=" => ">=",
        "=" => "==",
        "not=" => "!=",
        "and" => "and",
        "or" => "or",
        _ => return None,
    })
}

/// For unary cljrs forms we handle specially.
pub fn unop(sym: &str) -> Option<&'static str> {
    Some(match sym {
        "not" => "not",
        _ => return None,
    })
}

/// If this cljrs symbol maps to a function in Mojo's `math` module, return
/// the bare name plus the import line we need.
pub fn math_fn(sym: &str) -> Option<(&'static str, &'static str)> {
    Some(match sym {
        "sin" => ("sin", "from math import sin"),
        "cos" => ("cos", "from math import cos"),
        "tan" => ("tan", "from math import tan"),
        "sqrt" => ("sqrt", "from math import sqrt"),
        "exp" => ("exp", "from math import exp"),
        "log" => ("log", "from math import log"),
        "floor" => ("floor", "from math import floor"),
        "ceil" => ("ceil", "from math import ceil"),
        "pow" => ("pow", "from math import pow"),
        _ => return None,
    })
}

/// `abs`, `min`, `max` are builtins in Mojo, no import needed.
pub fn builtin_fn(sym: &str) -> Option<&'static str> {
    Some(match sym {
        "abs" => "abs",
        "min" => "min",
        "max" => "max",
        _ => return None,
    })
}
