//! Mojo-ish intermediate AST. We only model the subset of Mojo we actually
//! emit in v1: typed `fn`s, `let`, `if`, `while` (for loop/recur), binary
//! ops, calls, literals, and vars. The printer in lib.rs walks this.

use std::fmt;

/// A Mojo primitive type name. `None` means "no annotation" — we fall back
/// to Mojo's inference or (for top-level defs) default to Float64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MType {
    Int32,
    Int64,
    Float32,
    Float64,
    Bool,
    /// Unannotated. Printer will usually omit the `: T` suffix.
    Infer,
}

impl MType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MType::Int32 => "Int32",
            MType::Int64 => "Int64",
            MType::Float32 => "Float32",
            MType::Float64 => "Float64",
            MType::Bool => "Bool",
            MType::Infer => "",
        }
    }
    pub fn is_float(&self) -> bool {
        matches!(self, MType::Float32 | MType::Float64)
    }
}

impl fmt::Display for MType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    /// `break` — terminates the innermost While. Paired with setting a
    /// `__ret` var just above.
    Break,
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
