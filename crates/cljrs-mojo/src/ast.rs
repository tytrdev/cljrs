//! Mojo-ish intermediate AST. We only model the subset of Mojo we actually
//! emit in v1: typed `fn`s, `let`, `if`, `while` (for loop/recur), binary
//! ops, calls, literals, and vars. The printer in lib.rs walks this.

use std::fmt;

/// A Mojo primitive type name. `None` means "no annotation" — we fall back
/// to Mojo's inference or (for top-level defs) default to Float64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MType {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    BFloat16,
    Bool,
    /// `String` (Mojo's owned utf-8 string).
    Str,
    /// User-defined struct or otherwise opaque type passed through verbatim.
    Named(&'static str),
    /// SIMD[DType.Tdtype, N] — emitted whenever the user wrote `^SIMD[t n]`.
    Simd(&'static str, usize),
    /// Unannotated. Printer will usually omit the `: T` suffix.
    Infer,
}

impl MType {
    pub fn as_str(&self) -> String {
        match self {
            MType::Int8 => "Int8".into(),
            MType::Int16 => "Int16".into(),
            MType::Int32 => "Int32".into(),
            MType::Int64 => "Int64".into(),
            MType::UInt8 => "UInt8".into(),
            MType::UInt16 => "UInt16".into(),
            MType::UInt32 => "UInt32".into(),
            MType::UInt64 => "UInt64".into(),
            MType::Float32 => "Float32".into(),
            MType::Float64 => "Float64".into(),
            MType::BFloat16 => "BFloat16".into(),
            MType::Bool => "Bool".into(),
            MType::Str => "String".into(),
            MType::Named(s) => (*s).to_string(),
            MType::Simd(dt, n) => format!("SIMD[DType.{dt}, {n}]"),
            MType::Infer => String::new(),
        }
    }
    pub fn is_float(&self) -> bool {
        matches!(self, MType::Float32 | MType::Float64 | MType::BFloat16)
    }
}

impl fmt::Display for MType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// A whole Mojo file.
#[derive(Debug, Clone)]
pub struct MModule {
    /// Lines of `from foo import bar`. Collected lazily as we encounter
    /// calls that need them.
    pub imports: Vec<String>,
    pub items: Vec<MItem>,
}

#[derive(Debug, Clone)]
pub enum MItem {
    /// A top-level `fn` or `def`. `def` (runtime-dynamic) is only used as a
    /// fallback; the numeric-kernel path always produces `fn`.
    Fn(MFn),
    /// Top-level `var NAME: T = EXPR` (cljrs `def`).
    Var {
        name: String,
        ty: MType,
        value: MExpr,
        /// Leading `# cljrs: ...` comment (tier 1 only).
        comment: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct MFn {
    pub name: String,
    pub params: Vec<(String, MType)>,
    pub ret: MType,
    pub body: Vec<MStmt>,
    /// Decorators like `@always_inline` or `@parameter`. One per line,
    /// printed verbatim.
    pub decorators: Vec<String>,
    /// Optional `# cljrs: ...` source comment (tier 1/2).
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MStmt {
    /// `var NAME[: T] = EXPR`
    Let { name: String, ty: MType, value: MExpr },
    /// `NAME = EXPR` (rebind, used for loop/recur lowering)
    Assign { name: String, value: MExpr },
    /// `return EXPR`
    Return(MExpr),
    /// Bare expression (e.g. a `print(...)` call — not currently emitted
    /// but useful).
    Expr(MExpr),
    /// `if cond: ... else: ...`. Each arm is a stmt block. Used only for
    /// control-flow `cond`/`if` when the value is being bound or returned
    /// via a rewritten shape. Value-producing `if` goes through
    /// `MExpr::IfExpr` in a `Return` or `Let`.
    If {
        cond: MExpr,
        then: Vec<MStmt>,
        els: Vec<MStmt>,
    },
    /// `while cond: body`. Used for loop/recur.
    While {
        cond: MExpr,
        body: Vec<MStmt>,
    },
    /// `for NAME in range(LO, HI): body` — emitted when a `loop`/`recur`
    /// reduces to a simple counter sweep.
    ForRange {
        name: String,
        ty: MType,
        lo: MExpr,
        hi: MExpr,
        body: Vec<MStmt>,
    },
    /// `break` — terminates the innermost While. Paired with setting a
    /// `__ret` var just above.
    Break,
    /// `continue` — skip to next iteration of the innermost loop.
    Continue,
}

#[derive(Debug, Clone)]
pub enum MExpr {
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    Var(String),
    /// `LHS OP RHS`
    BinOp { op: String, lhs: Box<MExpr>, rhs: Box<MExpr> },
    /// Unary prefix: `-x`, `not x`.
    UnOp { op: String, rhs: Box<MExpr> },
    /// `callee(args...)` — callee is a bare name.
    Call { callee: String, args: Vec<MExpr> },
    /// Ternary-style: `(A) if (C) else (B)`. Used for value-position `if`.
    IfExpr {
        cond: Box<MExpr>,
        then: Box<MExpr>,
        els: Box<MExpr>,
    },
}
