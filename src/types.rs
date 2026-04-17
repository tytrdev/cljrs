//! Shared primitive type signatures used by `defn-native` parsing and,
//! later, by the MLIR codegen pass. Kept outside the `codegen` module so
//! it's available even when the `mlir` feature is off (the tree-walker
//! still needs to parse and validate type hints so test suites are
//! consistent across builds).

use crate::error::{Error, Result};
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimType {
    I64,
    F64,
    Bool,
    /// Pointer to a flat `f64` buffer. At the FFI boundary this is
    /// passed as an `i64` (pointer bits); inside the kernel it behaves
    /// as an opaque handle indexed by `(buf-get buf i)` / `(buf-set ...)`.
    /// Prerequisite for BSP/BVH/N-body and eventually GPU kernels.
    F64Buf,
}

impl PrimType {
    pub fn as_str(self) -> &'static str {
        match self {
            PrimType::I64 => "i64",
            PrimType::F64 => "f64",
            PrimType::Bool => "bool",
            PrimType::F64Buf => "f64-buf",
        }
    }

    /// True if this type travels through the C ABI in an integer register
    /// (i64 or pointer). Used to decide how to pack invoke args.
    pub fn is_int_abi(self) -> bool {
        matches!(self, PrimType::I64 | PrimType::F64Buf | PrimType::Bool)
    }
}

/// Parse a type-hint symbol into a [`PrimType`]. Accepts both cljrs-native
/// names (`i64`, `f64`, `bool`) and the Clojure aliases (`long`, `double`)
/// so existing Clojure code using `^long` / `^double` reads without churn.
pub fn parse_type_name(v: &Value) -> Result<PrimType> {
    let s = match v {
        Value::Symbol(s) => s.as_ref(),
        _ => {
            return Err(Error::Eval(format!(
                "type hint must be a symbol, got {}",
                v.type_name()
            )));
        }
    };
    match s {
        "i64" | "long" => Ok(PrimType::I64),
        "f64" | "double" => Ok(PrimType::F64),
        "bool" => Ok(PrimType::Bool),
        "f64-buf" => Ok(PrimType::F64Buf),
        other => Err(Error::Eval(format!(
            "unknown type hint: ^{other} (allowed: i64/long, f64/double, bool, f64-buf)"
        ))),
    }
}

/// If `v` is a reader-emitted `(__tagged__ tag form)` triple, return the
/// `(tag, form)` pair. Otherwise `None`. Used by `defn-native` to walk
/// type annotations without tree-walker evaluation.
pub fn unwrap_tagged(v: &Value) -> Option<(&Value, &Value)> {
    let Value::List(xs) = v else {
        return None;
    };
    if xs.len() != 3 {
        return None;
    }
    let Value::Symbol(s) = &xs[0] else {
        return None;
    };
    if s.as_ref() != "__tagged__" {
        return None;
    }
    Some((&xs[1], &xs[2]))
}
