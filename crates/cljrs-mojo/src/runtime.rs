//! Mapping from cljrs core forms to Mojo. Keeps all the "which Mojo symbol
//! does `sqrt` become" knowledge in one place so tier lowering stays clean.

use crate::ast::MType;

/// Map a cljrs type-hint symbol (from a `^tag` meta) to an MType. Returns
/// None if the hint isn't one of the known primitives — callers should
/// either pass it through (`Infer`) or error, depending on context.
pub fn type_hint(sym: &str) -> Option<MType> {
    Some(match sym {
        "i8"  | "Int8"  => MType::Int8,
        "i16" | "Int16" => MType::Int16,
        "i32" | "Int32" => MType::Int32,
        "i64" | "long" | "Int64" => MType::Int64,
        "u8"  | "UInt8"  => MType::UInt8,
        "u16" | "UInt16" => MType::UInt16,
        "u32" | "UInt32" => MType::UInt32,
        "u64" | "UInt64" => MType::UInt64,
        "f32" | "Float32" => MType::Float32,
        "f64" | "double" | "Float64" => MType::Float64,
        "bf16" | "BFloat16" => MType::BFloat16,
        "bool" | "Bool" => MType::Bool,
        "str"  | "String" => MType::Str,
        // `^SIMDf32x4` style: dtype + 'x' + lane count.
        s if s.starts_with("SIMD") => return parse_simd_tag(&s[4..]),
        _ => return None,
    })
}

/// Parse the trailing portion of a `SIMD<dtype>x<n>` tag, e.g. `f32x4`,
/// `i64x8`, `bf16x16`. Returns Some(MType::Simd) or None.
fn parse_simd_tag(rest: &str) -> Option<MType> {
    let (dt_alias, n_str) = rest.split_once('x')?;
    let n: usize = n_str.parse().ok()?;
    // Map alias → Mojo DType field name.
    let dtype = match dt_alias {
        "i8" => "int8",
        "i16" => "int16",
        "i32" => "int32",
        "i64" => "int64",
        "u8" => "uint8",
        "u16" => "uint16",
        "u32" => "uint32",
        "u64" => "uint64",
        "f16" => "float16",
        "f32" => "float32",
        "f64" => "float64",
        "bf16" => "bfloat16",
        "bool" => "bool",
        _ => return None,
    };
    Some(MType::Simd(dtype.to_string(), n))
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
        "sin"      => ("sin",      "from math import sin"),
        "cos"      => ("cos",      "from math import cos"),
        "tan"      => ("tan",      "from math import tan"),
        "asin"     => ("asin",     "from math import asin"),
        "acos"     => ("acos",     "from math import acos"),
        "atan"     => ("atan",     "from math import atan"),
        "atan2"    => ("atan2",    "from math import atan2"),
        "sinh"     => ("sinh",     "from math import sinh"),
        "cosh"     => ("cosh",     "from math import cosh"),
        "tanh"     => ("tanh",     "from math import tanh"),
        "sqrt"     => ("sqrt",     "from math import sqrt"),
        "cbrt"     => ("cbrt",     "from math import cbrt"),
        "exp"      => ("exp",      "from math import exp"),
        "expm1"    => ("expm1",    "from math import expm1"),
        "log"      => ("log",      "from math import log"),
        "log1p"    => ("log1p",    "from math import log1p"),
        "log2"     => ("log2",     "from math import log2"),
        "log10"    => ("log10",    "from math import log10"),
        "floor"    => ("floor",    "from math import floor"),
        "ceil"     => ("ceil",     "from math import ceil"),
        "round"    => ("round",    "from math import round"),
        "trunc"    => ("trunc",    "from math import trunc"),
        "pow"      => ("pow",      "from math import pow"),
        "hypot"    => ("hypot",    "from math import hypot"),
        "copysign" => ("copysign", "from math import copysign"),
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
