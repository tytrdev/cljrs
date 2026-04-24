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
    Named(String),
    /// SIMD[DType.Tdtype, N] — emitted whenever the user wrote `^SIMD[t n]`.
    Simd(String, usize),
    /// `List[T]`.
    List(Box<MType>),
    /// `Optional[T]`.
    Optional(Box<MType>),
    /// `Tuple[T1, T2, ...]`.
    Tuple(Vec<MType>),
    /// `Dict[K, V]`.
    Dict(Box<MType>, Box<MType>),
    /// `SIMD[DType.Tdtype, N]` where N is a compile-time parameter name.
    SimdParam(String, String),
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
            MType::Named(s) => s.clone(),
            MType::Simd(dt, n) => format!("SIMD[DType.{dt}, {n}]"),
            MType::SimdParam(dt, n) => format!("SIMD[DType.{dt}, {n}]"),
            MType::List(t) => format!("List[{}]", t.as_str()),
            MType::Optional(t) => format!("Optional[{}]", t.as_str()),
            MType::Tuple(ts) => {
                let parts: Vec<String> = ts.iter().map(|t| t.as_str()).collect();
                format!("Tuple[{}]", parts.join(", "))
            }
            MType::Dict(k, v) => format!("Dict[{}, {}]", k.as_str(), v.as_str()),
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
    /// `@value` struct with explicit init.
    Struct {
        name: String,
        fields: Vec<(String, MType)>,
        methods: Vec<MFn>,
        /// Optional trait the struct implements: `struct Name(Trait):`.
        trait_impl: Option<String>,
        /// Optional compile-time generic parameters, e.g. `[T: AnyType, N: Int]`.
        /// Stored as (name, bound) pairs. Empty for non-generic structs.
        cparams: Vec<(String, String)>,
        /// Extra struct decorators in addition to the default `@value`.
        /// e.g. `@register_passable`. Printed one-per-line above `struct`.
        decorators: Vec<String>,
        comment: Option<String>,
    },
    /// `alias NAME[: T] = VALUE` at top level.
    Alias {
        name: String,
        ty: MType,
        value: MExpr,
        comment: Option<String>,
    },
    /// `trait NAME:` with required fn signatures.
    Trait {
        name: String,
        methods: Vec<MTraitMethod>,
        comment: Option<String>,
    },
    /// Elementwise kernel (from `elementwise-mojo`). At tier=Readable /
    /// Optimized we print a scalar `for i in range(n): out[i] = body(...)`
    /// loop; at tier=Max we rewrite into Mojo's `vectorize[body, nelts](n)`
    /// idiom with `SIMD[DType, w].load/store`.
    Elementwise {
        name: String,
        /// Per-element pointer inputs (each becomes `UnsafePointer[T]`).
        /// All must share the same DType for now (phase 1/3).
        ptr_inputs: Vec<(String, MType)>,
        /// Scalar (broadcast) inputs — passed as-is, not loaded.
        scalar_inputs: Vec<(String, MType)>,
        /// Per-element output type (emits `out: UnsafePointer[T]`).
        out_ty: MType,
        /// Elementwise body expression (references the per-element names
        /// directly). At Max tier we rewrite these to SIMD loads.
        body: MExpr,
        /// If true, emit `parallelize[kernel](n, num_workers)` instead of
        /// the scalar / SIMD vectorize path. The per-thread body is the
        /// same scalar one; Mojo spreads indices across workers.
        parallel: bool,
        comment: Option<String>,
    },
    /// Reduction kernel (from `reduce-mojo`). Tier Readable/Optimized emit
    /// a scalar `for i in range(n): acc op= body` loop; Tier Max lifts the
    /// accumulator to SIMD and uses `.reduce_<op>()` to fold the final vector.
    Reduce {
        name: String,
        ptr_inputs: Vec<(String, MType)>,
        out_ty: MType,
        /// Per-element body expression (same vocabulary as Elementwise).
        body: MExpr,
        /// Combining op: `+`, `*`, `min`, or `max`.
        combiner: ReduceOp,
        /// Literal init value. Must be an `MExpr::IntLit`/`FloatLit`/`BoolLit`.
        init: MExpr,
        comment: Option<String>,
    },
    /// GPU kernel (from `elementwise-gpu-mojo`). Emits a Mojo fn that
    /// reads `thread_idx.x + block_idx.x * block_dim.x` and writes one
    /// output element per thread. All three tiers emit the same body —
    /// the kernel is already the per-thread op.
    GpuElementwise {
        name: String,
        ptr_inputs: Vec<(String, MType)>,
        out_ty: MType,
        body: MExpr,
        comment: Option<String>,
    },
    /// Host-side launcher for a GPU kernel. `kernel_name` is the symbol
    /// of an `elementwise-gpu-mojo` fn declared elsewhere; `ptr_args` are
    /// its pointer argument names (a, b, out). Emits a `raises` fn that
    /// takes a `DeviceContext`, those pointers, and `n: Int`, and calls
    /// `ctx.enqueue_function[KERNEL](..., grid_dim=..., block_dim=256)`.
    GpuLaunch {
        launcher_name: String,
        kernel_name: String,
        /// Ordered pointer arg names. Each is typed as
        /// `UnsafePointer[<out_ty>]`. Last element conventionally is `out`.
        ptr_args: Vec<String>,
        out_ty: MType,
        /// Threads per block. Default 256.
        block_dim: usize,
        comment: Option<String>,
    },
}

/// Associative reduction operator for `reduce-mojo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceOp {
    Add,
    Mul,
    Min,
    Max,
}

impl ReduceOp {
    pub fn scalar_op(self) -> &'static str {
        match self {
            ReduceOp::Add => "+",
            ReduceOp::Mul => "*",
            ReduceOp::Min => "",   // handled as min(acc, x)
            ReduceOp::Max => "",
        }
    }
    pub fn simd_reduce_method(self) -> &'static str {
        match self {
            ReduceOp::Add => "reduce_add",
            ReduceOp::Mul => "reduce_mul",
            ReduceOp::Min => "reduce_min",
            ReduceOp::Max => "reduce_max",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MTraitMethod {
    pub name: String,
    pub params: Vec<(String, MType, ParamConv)>,
    pub ret: MType,
}

#[derive(Debug, Clone)]
pub struct MFn {
    pub name: String,
    pub params: Vec<(String, MType, ParamConv)>,
    /// Per-param default values. Same length as `params`. `None` means
    /// the param is required; `Some(e)` emits `name: T = e` in Mojo and
    /// enables keyword-style call sites.
    pub param_defaults: Vec<Option<MExpr>>,
    pub ret: MType,
    pub body: Vec<MStmt>,
    /// Decorators like `@always_inline` or `@parameter`. One per line,
    /// printed verbatim.
    pub decorators: Vec<String>,
    /// Optional `# cljrs: ...` source comment (tier 1/2).
    pub comment: Option<String>,
    /// Compile-time parameters `fn foo[n: Int, T: AnyType](...)`.
    pub cparams: Vec<(String, String)>,
    /// `fn foo() raises -> T:` when true.
    pub raises: bool,
    /// `self` implicit first param for method defns inside a struct.
    pub is_method: bool,
    /// Optional docstring — emitted as the first statement of the body
    /// as a Mojo triple-quoted string literal. Drained from
    /// `^{:doc "..."}` metadata on the fn name.
    pub docstring: Option<String>,
}

/// Mojo argument convention. Emitted as a keyword before the param name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamConv {
    /// Default — no keyword.
    Default,
    Owned,
    Borrowed,
    Inout,
    Ref,
}

impl ParamConv {
    pub fn as_prefix(&self) -> &'static str {
        match self {
            ParamConv::Default => "",
            ParamConv::Owned => "owned ",
            ParamConv::Borrowed => "borrowed ",
            ParamConv::Inout => "inout ",
            ParamConv::Ref => "ref ",
        }
    }
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
    /// `for NAME in ITER: body` — iterator-protocol loop over a List/etc.
    ForIn {
        name: String,
        ty: MType,
        iter: MExpr,
        body: Vec<MStmt>,
    },
    /// `break` — terminates the innermost While. Paired with setting a
    /// `__ret` var just above.
    Break,
    /// `continue` — skip to next iteration of the innermost loop.
    Continue,
    /// `raise EXPR` — exception.
    Raise(MExpr),
    /// `raise` (bare) — re-raise in except handler.
    ReRaise,
    /// `try: body except T as n: handler ...`
    Try {
        body: Vec<MStmt>,
        catches: Vec<MCatch>,
    },
    /// `@parameter\nif TEST: then\nelse: els`
    ParameterIf {
        cond: MExpr,
        then: Vec<MStmt>,
        els: Vec<MStmt>,
    },
    /// Verbatim text line (with current indentation). Used for decorators
    /// nested inside fn bodies (e.g. `@parameter` on a nested fn).
    Raw(String),
}

#[derive(Debug, Clone)]
pub struct MCatch {
    /// e.g. `Error`, `ValueError`. Empty → bare `except:`.
    pub ty: String,
    /// Optional `as NAME`.
    pub name: Option<String>,
    pub body: Vec<MStmt>,
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
    /// `obj.field` — emitted from `(. obj field)`.
    Field { obj: Box<MExpr>, field: String },
    /// String literal (utf-8). Printed quoted with backslash-escapes.
    StrLit(String),
}
