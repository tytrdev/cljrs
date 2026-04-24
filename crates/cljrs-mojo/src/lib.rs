//! cljrs-mojo: transpile cljrs source to Mojo source.
//!
//! Entry point: [`emit`]. Parses a cljrs source string with
//! `cljrs::reader::read_all`, lowers to a small Mojo-ish AST, runs
//! tier-specific passes, and pretty-prints Mojo source.
//!
//! ## Coverage
//!
//! ### Supported
//!
//! - **Definitions**: `def`, `defn`, `defn-mojo`, `parameter-fn-mojo`,
//!   `always-inline-fn-mojo`, `raises-fn-mojo`, `parametric-fn-mojo`,
//!   `defstruct-mojo`, `deftrait-mojo`, `defn-method-mojo`, `alias-mojo`.
//!   Arbitrary decorator stacks via `^{:decorators [:a :b]}` meta on
//!   the fn/struct name (replaces the old sugars while keeping them
//!   backward-compat). Docstrings via `^{:doc "..."}`.
//! - **Primitive types**: `^i8 ^i16 ^i32 ^i64 ^u8 ^u16 ^u32 ^u64 ^f32
//!   ^f64 ^bf16 ^bool ^str`, plus user-defined named types that start
//!   with a capital letter.
//! - **Composite types**: `^SIMDf32x4` → `SIMD[DType.float32, 4]`,
//!   `^List-f32` → `List[Float32]`, `^Opt-f32` → `Optional[Float32]`,
//!   `^Tuple-i32-f32` → `Tuple[Int32, Float32]`,
//!   `^Dict-str-f32` → `Dict[String, Float32]`,
//!   `^fn/f32->f32` → `fn(Float32) -> Float32`,
//!   `^fn/f32|f32->f32` → `fn(Float32, Float32) -> Float32` (multi-arg
//!   fn-pointer types; `|` separates params because comma is reader
//!   whitespace).
//! - **Argument conventions**: `^owned`, `^borrowed`, `^inout`, `^ref`
//!   stack with a type tag: `[^inout ^i32 x]` → `inout x: Int32`.
//! - **Default args**: `^{:default EXPR}` on a param → `name: T = EXPR`.
//! - **Control flow**: `if`, `cond` (flat `if/elif/else`), `do`, `let`,
//!   `loop`/`recur` (with for-range fast path), `(for-mojo [i lo hi])`
//!   and iterator-protocol `(for-mojo-in [x xs] body)`, `(break)`,
//!   `(continue)`.
//! - **Exceptions**: `(raise (Error "msg"))`, bare `(raise)` re-raises,
//!   `(try BODY (catch T as n HANDLER)...)`, `raises-fn-mojo` for
//!   signatures that propagate.
//! - **Compile-time**: `alias-mojo`, `parametric-fn-mojo`
//!   (emits `fn foo[n: Int, T: AnyType]`), and `(parameter-if ...)`
//!   inside parametric bodies.
//! - **Collections**: `(list e1 e2 ...)` → `List[T](e1, ...)`,
//!   `(nth xs i)` → `xs[i]`, `(len xs)` → `len(xs)`,
//!   `(tuple a b c)` → `Tuple(a, b, c)`,
//!   `(dict-mojo ^Dict-K-V)` → `Dict[K, V]()`,
//!   `(get-mojo d k)` / `(assoc-mojo d k v)` for dict access.
//! - **Optional**: `(some x)` → `Optional(x)`, `(none)` → `None`,
//!   `(unwrap o)` → `o.value()`.
//! - **Traits & methods**: `(deftrait-mojo Shape (area ^f32 []))`,
//!   `(defstruct-mojo Square :Shape [^f32 side])` → `struct Square(Shape):`,
//!   `(defn-method-mojo Vec3 length ^f32 [] ...)` appends indented
//!   methods inside the matching struct.
//! - **Generic structs**: `(defstruct-mojo Vec3 [T] [^T x ^T y ^T z])` →
//!   `struct Vec3[T: AnyType]:`. Multi-param via flat pairs
//!   `[T AnyType N Int]`. Call sites with a type tag auto-specialize:
//!   `(Vec3 ^f32 1.0 2.0 3.0)` → `Vec3[Float32](1.0, 2.0, 3.0)`.
//! - **Assertions**: `(mojo-assert cond)` / `(mojo-assert cond msg)` →
//!   `debug_assert(...)`.
//! - **String helpers**: `str-len`, `str-slice`, `str-split`,
//!   `str-upper`, `str-lower`, `str-strip`, `str-starts-with?`,
//!   `str-ends-with?`, `str-contains?`, `str-replace`.
//! - **Introspection**: `(isinstance-mojo v T)` → `isinstance(v, T)`.
//! - **I/O**: `(print x)`, `(println x)` → `print(x)`;
//!   `(format "n={}" n)` → `"n=" + String(n)` left-folded.
//! - **Math**: trig (`sin cos tan asin acos atan atan2 sinh cosh tanh`),
//!   exponentials (`exp expm1 log log1p log2 log10`), roots & rounding
//!   (`sqrt cbrt floor ceil round trunc`), plus `pow`, `hypot`,
//!   `copysign`, `abs`, `min`, `max`.
//! - **Elementwise kernels**: `(elementwise-mojo NAME [^T a ^T b] ^T body)`
//!   emits a scalar loop at Readable/Optimized and a
//!   `vectorize[body, nelts](n)` kernel at Max. Mixed-precision allowed
//!   when the body contains at least one `(cast-mojo ^T x)` — each input
//!   loads at its own dtype, casts bridge to the accumulator/output.
//!   Multi-output: declare `^Tuple-T1-T2-...` as the return type and write
//!   the body as `(tuple e1 e2 ...)`; the emitter produces N output
//!   pointers and N per-element stores.
//! - **Parallel kernels**: `(parallel-mojo ...)` — same shape, emits
//!   `parallelize[kernel](n)` instead of vectorize (embarrassingly
//!   parallel across workers, same body across all tiers).
//! - **Reductions**: `(reduce-mojo sum [^f32 x] ^f32 (* x x) 0.0)` emits
//!   a scalar for-loop at Readable/Optimized and a SIMD-accumulator-plus-
//!   `reduce_add` kernel at Max. Multi-input reductions (`dot`,
//!   `sum-sq-diff`) take two pointer inputs and fuse the elementwise
//!   shape into one kernel pass. Combining op is `+` by default; a
//!   `^mul`/`^min`/`^max` tag on the body selects product / min / max.
//! - **Gather/scatter**: `(gather-mojo NAME [^T values ^I indices])` emits
//!   `out[i] = values[Int(indices[i])]`; `(scatter-mojo NAME [^I indices
//!   ^T values])` emits `out[Int(indices[i])] = values[i]`. Scalar loop
//!   at every tier — SIMD-gather/scatter spellings in Mojo are still
//!   API-uncertain so we leave that to future work.
//! - **GPU kernels**: `(elementwise-gpu-mojo NAME [^T a ^T b] ^T body)`
//!   emits a Mojo fn that computes its thread index from
//!   `block_idx.x * block_dim.x + thread_idx.x` and writes one output
//!   element per thread (same body across all tiers).
//! - **GPU host launcher**: `(launch-gpu-mojo [^T] KERNEL [args])`
//!   emits `fn launch_KERNEL(ctx: DeviceContext, args, n) raises:`
//!   that enqueues the kernel with `grid_dim=ceil(n/256), block_dim=256`.
//! - **Naming**: kebab-case identifiers (`vector-add`, `abs-max`) are
//!   auto-rewritten to snake_case in the emitted Mojo. The original
//!   source name is preserved in the `# cljrs:` tier-Readable comment.
//! - **Tiers**: Readable (keeps `# cljrs:` comments), Optimized
//!   (const-fold + CSE + inline 1-stmt return fns), Max (adds
//!   `@always_inline` to pure, non-recursive, ≤10-stmt fns with
//!   control depth ≤ 2; strips comments).
//!
//! ### Not supported (errors on sight)
//!
//! - Collection literals `[1 2 3]`, `{:a 1}`, `#{:a}` in expr position —
//!   use `(list ...)` / `(dict-mojo ...)` / `(set ...)` instead.
//! - Variadic params (`& rest`).
//! - Higher-order fn refs as arguments are supported via `^fn/...` typed
//!   parameters and calling the bound symbol. Anonymous `(fn [^T x] body)`
//!   hoists to a `__anon_fn<N>` top-level helper and yields a `Var`
//!   reference. Captures / closures-over-locals are NOT yet supported —
//!   the anonymous fn must be closed (only reference its own params).
//! - `loop`, `let`, `cond`, `recur`, `for-mojo`, `for-mojo-in`, `try`,
//!   `raise`, `parameter-if`, `mojo-assert` in non-tail / non-stmt
//!   positions.
//! - Mixed-precision kernels without an explicit `(cast-mojo ^T x)` in
//!   the body still error (we use the presence of cast-mojo as the
//!   opt-in signal).
//! - Source-level `;;` comment pass-through — the reader strips them
//!   before the transpiler sees the form.
//! - Multi-accumulator reductions (a single reduce producing 2+
//!   accumulators). Multi-output *elementwise* kernels are supported; only
//!   multi-output *reduce-mojo* is still TODO.
//!
//! Forms outside this set produce errors that quote the offending form.

pub mod ast;
pub mod runtime;
pub mod tier1;
pub mod tier2;
pub mod tier3;

use crate::ast::{MExpr, MFn, MItem, MModule, MStmt, MType, ReduceOp};

/// Tier selector. `Readable` preserves cljrs source as comments; `Optimized`
/// runs const-fold / CSE / small-fn inlining; `Max` adds `@always_inline`
/// and parameter-specialization hooks and strips all comments.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    Readable,
    Optimized,
    Max,
}

/// Transpile a cljrs source string to Mojo source at the requested tier.
pub fn emit(src: &str, tier: Tier) -> Result<String, String> {
    let forms = cljrs::reader::read_all(src).map_err(|e| format!("read error: {e}"))?;
    let mut module = tier1::lower_module(&forms)?;
    match tier {
        Tier::Readable => {}
        Tier::Optimized => tier2::optimize(&mut module),
        Tier::Max => tier3::specialize(&mut module),
    }
    add_elementwise_imports(&mut module, tier);
    Ok(print_module(&module, tier))
}

/// Inject imports and `alias nelts_* = simd_width_of[...]()` lines needed by
/// elementwise kernels. Tier=Max pulls in `vectorize`; Readable/Optimized
/// emit scalar loops that need no extra imports.
fn add_elementwise_imports(m: &mut MModule, tier: Tier) {
    let has_elem = m.items.iter().any(|i| matches!(i, MItem::Elementwise { .. }));
    let has_reduce = m.items.iter().any(|i| matches!(i, MItem::Reduce { .. }));
    let has_gpu = m.items.iter().any(|i| matches!(i, MItem::GpuElementwise { .. }));
    let has_gather_scatter = m.items.iter().any(|i|
        matches!(i, MItem::Gather { .. } | MItem::Scatter { .. }));
    if has_gather_scatter {
        let imp = "from memory import UnsafePointer";
        if !m.imports.iter().any(|s| s == imp) {
            m.imports.push(imp.to_string());
        }
    }
    let has_launch = m.items.iter().any(|i| matches!(i, MItem::GpuLaunch { .. }));

    if has_launch {
        for imp in [
            "from gpu.host import DeviceContext",
            "from memory import UnsafePointer",
        ] {
            if !m.imports.iter().any(|s| s == imp) {
                m.imports.push(imp.to_string());
            }
        }
    }
    let has_parallel = m.items.iter().any(|i| matches!(i, MItem::Elementwise { parallel: true, .. }));

    if has_parallel {
        for imp in [
            "from algorithm import parallelize",
            "from memory import UnsafePointer",
        ] {
            if !m.imports.iter().any(|s| s == imp) {
                m.imports.push(imp.to_string());
            }
        }
    }

    // GPU imports are needed at every tier when a GPU kernel is present.
    if has_gpu {
        for imp in [
            "from gpu import thread_idx, block_idx, block_dim",
            "from memory import UnsafePointer",
        ] {
            if !m.imports.iter().any(|s| s == imp) {
                m.imports.push(imp.to_string());
            }
        }
    }

    if !(has_elem || has_reduce) {
        return;
    }
    if !matches!(tier, Tier::Max) {
        return;
    }
    // Dedup dtypes used by SIMD-lifted kernels.
    let mut dtypes: Vec<String> = Vec::new();
    for it in &m.items {
        let dt_opts: Vec<String> = match it {
            MItem::Elementwise { out_ty, ptr_inputs, .. } => {
                let mut ds = Vec::new();
                // Pick up ALL relevant dtypes: each ptr input's dtype and
                // every tuple leg's dtype (for multi-output kernels).
                match out_ty {
                    MType::Tuple(ts) => {
                        for t in ts {
                            ds.push(mtype_dtype_field(t));
                        }
                    }
                    _ => ds.push(mtype_dtype_field(out_ty)),
                }
                for (_, pty) in ptr_inputs {
                    ds.push(mtype_dtype_field(pty));
                }
                ds
            }
            MItem::Reduce { out_ty, ptr_inputs, .. } => {
                let mut ds = vec![mtype_dtype_field(out_ty)];
                for (_, pty) in ptr_inputs {
                    ds.push(mtype_dtype_field(pty));
                }
                ds
            }
            _ => Vec::new(),
        };
        for dt in dt_opts {
            if !dtypes.iter().any(|d| d == &dt) {
                dtypes.push(dt);
            }
        }
    }
    let needed_imports = [
        "from algorithm import vectorize",
        "from memory import UnsafePointer",
        "from sys import simd_width_of",
    ];
    for imp in needed_imports {
        if !m.imports.iter().any(|s| s == imp) {
            m.imports.push(imp.to_string());
        }
    }
    // Emit alias lines at the top of items: `alias nelts_f32 = simd_width_of[DType.float32]()`.
    // Inject as synthetic Alias items so the printer's spacing still works.
    let mut alias_items: Vec<MItem> = Vec::new();
    for dt in &dtypes {
        let aname = format!("nelts_{}", dtype_short(dt));
        alias_items.push(MItem::Alias {
            name: aname,
            ty: MType::Infer,
            value: MExpr::Call {
                callee: format!("simd_width_of[DType.{dt}]"),
                args: vec![],
            },
            comment: None,
        });
    }
    // Prepend aliases so they appear above the kernels.
    let mut new_items = alias_items;
    new_items.append(&mut m.items);
    m.items = new_items;
}

/// Map an MType primitive to the `DType.<field>` name used in Mojo.
fn mtype_dtype_field(t: &MType) -> String {
    match t {
        MType::Float32 => "float32".into(),
        MType::Float64 => "float64".into(),
        MType::BFloat16 => "bfloat16".into(),
        MType::Int8 => "int8".into(),
        MType::Int16 => "int16".into(),
        MType::Int32 => "int32".into(),
        MType::Int64 => "int64".into(),
        MType::UInt8 => "uint8".into(),
        MType::UInt16 => "uint16".into(),
        MType::UInt32 => "uint32".into(),
        MType::UInt64 => "uint64".into(),
        _ => "float32".into(),
    }
}

fn dtype_short(dt: &str) -> &str {
    match dt {
        "float32" => "f32",
        "float64" => "f64",
        "bfloat16" => "bf16",
        "int8" => "i8",
        "int16" => "i16",
        "int32" => "i32",
        "int64" => "i64",
        "uint8" => "u8",
        "uint16" => "u16",
        "uint32" => "u32",
        "uint64" => "u64",
        _ => dt,
    }
}

// ---------------- printer ----------------

/// Rewrite a single identifier: kebab-case → snake_case. Applied at
/// emission sites only; the cljrs AST keeps the original names so the
/// `# cljrs:` trace comment is faithful to the source.
pub(crate) fn snake(s: &str) -> String {
    if !s.contains('-') {
        return s.to_string();
    }
    // Leave literal strings / quoted content alone — this helper is only
    // called on identifier positions.
    s.replace('-', "_")
}

fn print_module(m: &MModule, tier: Tier) -> String {
    let mut out = String::new();
    for imp in &m.imports {
        out.push_str(imp);
        out.push('\n');
    }
    if !m.imports.is_empty() {
        out.push('\n');
    }
    for (i, item) in m.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_item(&mut out, item, tier);
    }
    out
}

fn print_item(out: &mut String, item: &MItem, tier: Tier) {
    match item {
        MItem::Fn(f) => print_fn(out, f, tier),
        MItem::Struct { name, fields, methods, trait_impl, cparams, decorators, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            for d in decorators {
                out.push_str(d);
                out.push('\n');
            }
            if !decorators.iter().any(|d| d == "@value") {
                out.push_str("@value\n");
            }
            out.push_str("struct ");
            out.push_str(&snake(name));
            if !cparams.is_empty() {
                out.push('[');
                for (i, (n, t)) in cparams.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(n);
                    out.push_str(": ");
                    out.push_str(t);
                }
                out.push(']');
            }
            if let Some(t) = trait_impl {
                out.push('(');
                out.push_str(&snake(t));
                out.push(')');
            }
            out.push_str(":\n");
            for (fname, fty) in fields {
                out.push_str("    var ");
                out.push_str(&snake(fname));
                if !matches!(fty, MType::Infer) {
                    out.push_str(": ");
                    out.push_str(&fty.as_str());
                }
                out.push('\n');
            }
            // Explicit __init__ for clarity.
            out.push_str("    fn __init__(out self");
            for (fname, fty) in fields {
                out.push_str(", ");
                out.push_str(&snake(fname));
                if !matches!(fty, MType::Infer) {
                    out.push_str(": ");
                    out.push_str(&fty.as_str());
                }
            }
            out.push_str("):\n");
            if fields.is_empty() && methods.is_empty() {
                out.push_str("        pass\n");
            } else if fields.is_empty() {
                out.push_str("        pass\n");
            } else {
                for (fname, _) in fields {
                    let n = snake(fname);
                    out.push_str("        self.");
                    out.push_str(&n);
                    out.push_str(" = ");
                    out.push_str(&n);
                    out.push('\n');
                }
            }
            // Methods: each emitted as `fn name(self, ...):` indented.
            for m in methods {
                out.push('\n');
                print_fn_indented(out, m, tier, 1);
            }
        }
        MItem::Alias { name, ty, value, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("alias ");
            out.push_str(&snake(name));
            if !matches!(ty, MType::Infer) {
                out.push_str(": ");
                out.push_str(&ty.as_str());
            }
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
        MItem::Trait { name, methods, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("trait ");
            out.push_str(&snake(name));
            out.push_str(":\n");
            if methods.is_empty() {
                out.push_str("    pass\n");
            } else {
                for m in methods {
                    out.push_str("    fn ");
                    out.push_str(&snake(&m.name));
                    out.push_str("(self");
                    for (n, t, c) in &m.params {
                        out.push_str(", ");
                        out.push_str(c.as_prefix());
                        out.push_str(&snake(n));
                        if !matches!(t, MType::Infer) {
                            out.push_str(": ");
                            out.push_str(&t.as_str());
                        }
                    }
                    out.push(')');
                    if !matches!(m.ret, MType::Infer) {
                        out.push_str(" -> ");
                        out.push_str(&m.ret.as_str());
                    }
                    out.push_str(": ...\n");
                }
            }
        }
        MItem::Elementwise { name, ptr_inputs, scalar_inputs, out_ty, body, parallel, comment } => {
            print_elementwise(out, name, ptr_inputs, scalar_inputs, out_ty, body, *parallel, comment.as_deref(), tier);
        }
        MItem::Reduce { name, ptr_inputs, out_ty, body, combiner, init, comment } => {
            print_reduce(out, name, ptr_inputs, out_ty, body, *combiner, init, comment.as_deref(), tier);
        }
        MItem::GpuElementwise { name, ptr_inputs, out_ty, body, comment } => {
            print_gpu_elementwise(out, name, ptr_inputs, out_ty, body, comment.as_deref(), tier);
        }
        MItem::GpuLaunch { launcher_name, kernel_name, ptr_args, out_ty, block_dim, comment } => {
            print_gpu_launch(out, launcher_name, kernel_name, ptr_args, out_ty, *block_dim, comment.as_deref(), tier);
        }
        MItem::Gather { name, values_name, values_ty, indices_name, indices_ty, out_ty, comment } => {
            print_gather(out, name, values_name, values_ty, indices_name, indices_ty, out_ty, comment.as_deref(), tier);
        }
        MItem::Scatter { name, indices_name, indices_ty, values_name, values_ty, out_ty, comment } => {
            print_scatter(out, name, indices_name, indices_ty, values_name, values_ty, out_ty, comment.as_deref(), tier);
        }
        MItem::Var { name, ty, value, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("var ");
            out.push_str(&snake(name));
            if !matches!(ty, MType::Infer) {
                out.push_str(": ");
                out.push_str(&ty.as_str());
            }
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
    }
}

fn print_elementwise(
    out: &mut String,
    name: &str,
    ptr_inputs: &[(String, MType)],
    scalar_inputs: &[(String, MType)],
    out_ty: &MType,
    body: &MExpr,
    parallel: bool,
    comment: Option<&str>,
    tier: Tier,
) {
    if let Some(c) = comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    let ty_str = out_ty.as_str();
    let dt = mtype_dtype_field(out_ty);
    // Multi-output detection: if out_ty is Tuple(...), body is a
    // Call("Tuple", [e1, e2, ...]) and we emit out0, out1, ... pointers
    // and N stores.
    let out_tys: Vec<MType> = match out_ty {
        MType::Tuple(ts) => ts.clone(),
        _ => Vec::new(),
    };
    let multi = !out_tys.is_empty();
    // Signature — each ptr input uses its OWN dtype (mixed-precision:
    // a Float16 input can feed a Float32 output).
    out.push_str("fn ");
    out.push_str(&snake(name));
    out.push('(');
    for (i, (n, pty)) in ptr_inputs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&snake(n));
        out.push_str(&format!(": UnsafePointer[{}]", pty.as_str()));
    }
    for (n, t) in scalar_inputs {
        out.push_str(", ");
        out.push_str(&snake(n));
        out.push_str(": ");
        out.push_str(&t.as_str());
    }
    if multi {
        for (j, t) in out_tys.iter().enumerate() {
            out.push_str(&format!(", out{j}: UnsafePointer[{}]", t.as_str()));
        }
    } else {
        out.push_str(", out: UnsafePointer[");
        out.push_str(&ty_str);
        out.push_str("]");
    }
    out.push_str(", n: Int):\n");
    // For multi-output, extract the tuple element exprs from the body.
    let multi_bodies: Vec<MExpr> = if multi {
        match body {
            MExpr::Call { callee, args } if callee == "Tuple" => args.clone(),
            _ => Vec::new(), // shouldn't reach — validated upstream
        }
    } else {
        Vec::new()
    };
    if parallel {
        // Emit: a nested @parameter fn(i) that does one element, and a
        // top-level `parallelize[__kernel](n)` dispatch. Workers default
        // to the runtime scheduler's choice (no explicit num_workers).
        out.push_str("    @parameter\n");
        out.push_str("    fn __kernel(i: Int):\n");
        let ptr_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
        if multi {
            for (j, e) in multi_bodies.iter().enumerate() {
                let subst = subst_ptr_loads(e, &ptr_names, false, &dt);
                out.push_str(&format!("        out{j}[i] = "));
                print_expr(out, &subst);
                out.push('\n');
            }
        } else {
            let subst_body = subst_ptr_loads(body, &ptr_names, false, &dt);
            out.push_str("        out[i] = ");
            print_expr(out, &subst_body);
            out.push('\n');
        }
        out.push_str("    parallelize[__kernel](n)\n");
        return;
    }
    match tier {
        Tier::Readable | Tier::Optimized => {
            // Scalar loop.
            out.push_str("    for i in range(n):\n");
            let ptr_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
            if multi {
                for (j, e) in multi_bodies.iter().enumerate() {
                    let subst = subst_ptr_loads(e, &ptr_names, false, &dt);
                    out.push_str(&format!("        out{j}[i] = "));
                    print_expr(out, &subst);
                    out.push('\n');
                }
            } else {
                let subst_body = subst_ptr_loads(body, &ptr_names, false, &dt);
                out.push_str("        out[i] = ");
                print_expr(out, &subst_body);
                out.push('\n');
            }
        }
        Tier::Max => {
            // Vectorized (single-output) / vectorized-multi-store (multi-output).
            out.push_str("    @parameter\n");
            out.push_str("    fn __kernel[w: Int](i: Int):\n");
            for (n, pty) in ptr_inputs {
                let sn = snake(n);
                let pdt = mtype_dtype_field(pty);
                out.push_str(&format!(
                    "        var {sn}v = SIMD[DType.{pdt}, w].load({sn}, i)\n"
                ));
            }
            let ptr_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
            if multi {
                for (j, e) in multi_bodies.iter().enumerate() {
                    let subst = subst_ptr_loads(e, &ptr_names, true, &dt);
                    out.push_str("        ");
                    print_expr_grouped(out, &subst);
                    out.push_str(&format!(".store(out{j}, i)\n"));
                }
                // Use the FIRST output dtype's nelts for the vectorize width.
                let first_dt = mtype_dtype_field(&out_tys[0]);
                let short = dtype_short(&first_dt);
                out.push_str(&format!("    vectorize[__kernel, nelts_{short}](n)\n"));
            } else {
                let subst_body = subst_ptr_loads(body, &ptr_names, true, &dt);
                out.push_str("        ");
                print_expr_grouped(out, &subst_body);
                out.push_str(".store(out, i)\n");
                let short = dtype_short(&dt);
                out.push_str(&format!("    vectorize[__kernel, nelts_{short}](n)\n"));
            }
        }
    }
}

/// Substitute references to ptr-input names in the body:
///   - Max (SIMD) tier: `a` → `av` (the loaded SIMD var)
///   - Scalar tier:    `a` → `a[i]` (UnsafePointer[T] indexing)
fn print_reduce(
    out: &mut String,
    name: &str,
    ptr_inputs: &[(String, MType)],
    out_ty: &MType,
    body: &MExpr,
    combiner: ReduceOp,
    init: &MExpr,
    comment: Option<&str>,
    tier: Tier,
) {
    if let Some(c) = comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    let ty_str = out_ty.as_str();
    let dt = mtype_dtype_field(out_ty);
    // Signature.
    out.push_str("fn ");
    out.push_str(&snake(name));
    out.push('(');
    for (i, (n, pty)) in ptr_inputs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&snake(n));
        out.push_str(&format!(": UnsafePointer[{}]", pty.as_str()));
    }
    out.push_str(&format!(", n: Int) -> {ty_str}:\n"));

    match tier {
        Tier::Readable | Tier::Optimized => {
            // Scalar loop.
            out.push_str(&format!("    var acc: {ty_str} = "));
            print_expr(out, init);
            out.push('\n');
            out.push_str("    for i in range(n):\n");
            let ptr_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
            let subst_body = subst_ptr_loads(body, &ptr_names, /*max=*/ false, &dt);
            match combiner {
                ReduceOp::Add => {
                    out.push_str("        acc += ");
                    print_expr(out, &subst_body);
                    out.push('\n');
                }
                ReduceOp::Mul => {
                    out.push_str("        acc *= ");
                    print_expr(out, &subst_body);
                    out.push('\n');
                }
                ReduceOp::Min => {
                    out.push_str("        acc = min(acc, ");
                    print_expr(out, &subst_body);
                    out.push_str(")\n");
                }
                ReduceOp::Max => {
                    out.push_str("        acc = max(acc, ");
                    print_expr(out, &subst_body);
                    out.push_str(")\n");
                }
            }
            out.push_str("    return acc\n");
        }
        Tier::Max => {
            // SIMD-lifted accumulator using vectorize.
            let short = dtype_short(&dt);
            out.push_str(&format!(
                "    var acc = SIMD[DType.{dt}, nelts_{short}](",
            ));
            print_expr(out, init);
            out.push_str(")\n");
            out.push_str("    @parameter\n");
            out.push_str("    fn __kernel[w: Int](i: Int):\n");
            for (n, pty) in ptr_inputs {
                let sn = snake(n);
                let pdt = mtype_dtype_field(pty);
                out.push_str(&format!(
                    "        var {sn}v = SIMD[DType.{pdt}, w].load({sn}, i)\n"
                ));
            }
            let ptr_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
            let subst_body = subst_ptr_loads(body, &ptr_names, /*max=*/ true, &dt);
            // Per-iter SIMD combine into acc. The vectorize parameter `w` may
            // differ from nelts_<short> on tail iterations, so we fold the
            // iteration's SIMD value through its own reduce_* before combining
            // into the outer nelts-wide accumulator's lane 0. This is the
            // safest shape across tail widths.
            let reduce_method = combiner.simd_reduce_method();
            match combiner {
                ReduceOp::Add => {
                    out.push_str("        acc[0] += ");
                    print_expr_grouped(out, &subst_body);
                    out.push_str(&format!(".{reduce_method}()\n"));
                }
                ReduceOp::Mul => {
                    out.push_str("        acc[0] *= ");
                    print_expr_grouped(out, &subst_body);
                    out.push_str(&format!(".{reduce_method}()\n"));
                }
                ReduceOp::Min => {
                    out.push_str("        acc[0] = min(acc[0], ");
                    print_expr_grouped(out, &subst_body);
                    out.push_str(&format!(".{reduce_method}())\n"));
                }
                ReduceOp::Max => {
                    out.push_str("        acc[0] = max(acc[0], ");
                    print_expr_grouped(out, &subst_body);
                    out.push_str(&format!(".{reduce_method}())\n"));
                }
            }
            out.push_str(&format!("    vectorize[__kernel, nelts_{short}](n)\n"));
            out.push_str(&format!("    return acc.{reduce_method}()\n"));
        }
    }
}

fn print_gather(
    out: &mut String,
    name: &str,
    values_name: &str,
    values_ty: &MType,
    indices_name: &str,
    indices_ty: &MType,
    out_ty: &MType,
    comment: Option<&str>,
    tier: Tier,
) {
    if let Some(c) = comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    let v = snake(values_name);
    let idx = snake(indices_name);
    out.push_str("fn ");
    out.push_str(&snake(name));
    out.push_str(&format!(
        "({v}: UnsafePointer[{}], {idx}: UnsafePointer[{}], out: UnsafePointer[{}], n: Int):\n",
        values_ty.as_str(), indices_ty.as_str(), out_ty.as_str()
    ));
    out.push_str("    for i in range(n):\n");
    out.push_str(&format!("        out[i] = {v}[Int({idx}[i])]\n"));
}

fn print_scatter(
    out: &mut String,
    name: &str,
    indices_name: &str,
    indices_ty: &MType,
    values_name: &str,
    values_ty: &MType,
    out_ty: &MType,
    comment: Option<&str>,
    tier: Tier,
) {
    if let Some(c) = comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    let v = snake(values_name);
    let idx = snake(indices_name);
    out.push_str("fn ");
    out.push_str(&snake(name));
    out.push_str(&format!(
        "({idx}: UnsafePointer[{}], {v}: UnsafePointer[{}], out: UnsafePointer[{}], n: Int):\n",
        indices_ty.as_str(), values_ty.as_str(), out_ty.as_str()
    ));
    out.push_str("    for i in range(n):\n");
    out.push_str(&format!("        out[Int({idx}[i])] = {v}[i]\n"));
}

fn print_gpu_elementwise(
    out: &mut String,
    name: &str,
    ptr_inputs: &[(String, MType)],
    out_ty: &MType,
    body: &MExpr,
    comment: Option<&str>,
    tier: Tier,
) {
    if let Some(c) = comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    let ty_str = out_ty.as_str();
    // All three tiers share the kernel body — the function IS the per-thread
    // op, so vectorization and @always_inline wouldn't change semantics.
    out.push_str("fn ");
    out.push_str(&snake(name));
    out.push('(');
    for (n, _) in ptr_inputs {
        out.push_str(&snake(n));
        out.push_str(&format!(": UnsafePointer[{ty_str}], "));
    }
    out.push_str(&format!("out: UnsafePointer[{ty_str}], n: Int):\n"));
    out.push_str("    var i = block_idx.x * block_dim.x + thread_idx.x\n");
    out.push_str("    if i < n:\n");
    let ptr_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
    let subst_body = subst_ptr_loads(body, &ptr_names, /*max=*/ false, &mtype_dtype_field(out_ty));
    out.push_str("        out[i] = ");
    print_expr(out, &subst_body);
    out.push('\n');
}

/// Emit a host-side `raises` launcher that calls `ctx.enqueue_function[KERNEL](...)`.
fn print_gpu_launch(
    out: &mut String,
    launcher_name: &str,
    kernel_name: &str,
    ptr_args: &[String],
    out_ty: &MType,
    block_dim: usize,
    comment: Option<&str>,
    tier: Tier,
) {
    if let Some(c) = comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    let ty_str = out_ty.as_str();
    out.push_str("fn ");
    out.push_str(&snake(launcher_name));
    out.push_str("(ctx: DeviceContext");
    for a in ptr_args {
        out.push_str(", ");
        out.push_str(&snake(a));
        out.push_str(&format!(": UnsafePointer[{ty_str}]"));
    }
    out.push_str(", n: Int) raises:\n");
    out.push_str(&format!("    ctx.enqueue_function[{}](", snake(kernel_name)));
    for (i, a) in ptr_args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&snake(a));
    }
    if !ptr_args.is_empty() {
        out.push_str(", ");
    }
    out.push_str("n, ");
    out.push_str(&format!(
        "grid_dim=(n + {bd} - 1) // {bd}, block_dim={bd})\n",
        bd = block_dim
    ));
}

fn subst_ptr_loads(body: &MExpr, ptr_names: &[String], max: bool, _dt: &str) -> MExpr {
    match body {
        MExpr::IntLit(_) | MExpr::FloatLit(_) | MExpr::BoolLit(_) | MExpr::StrLit(_) => body.clone(),
        MExpr::Var(n) => {
            if ptr_names.iter().any(|p| p == n) {
                if max {
                    MExpr::Var(format!("{n}v"))
                } else {
                    MExpr::Call {
                        callee: "__index__".into(),
                        args: vec![MExpr::Var(n.clone()), MExpr::Var("i".into())],
                    }
                }
            } else {
                MExpr::Var(n.clone())
            }
        }
        MExpr::BinOp { op, lhs, rhs } => MExpr::BinOp {
            op: op.clone(),
            lhs: Box::new(subst_ptr_loads(lhs, ptr_names, max, _dt)),
            rhs: Box::new(subst_ptr_loads(rhs, ptr_names, max, _dt)),
        },
        MExpr::UnOp { op, rhs } => MExpr::UnOp {
            op: op.clone(),
            rhs: Box::new(subst_ptr_loads(rhs, ptr_names, max, _dt)),
        },
        MExpr::Call { callee, args } => MExpr::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| subst_ptr_loads(a, ptr_names, max, _dt)).collect(),
        },
        MExpr::IfExpr { cond, then, els } => MExpr::IfExpr {
            cond: Box::new(subst_ptr_loads(cond, ptr_names, max, _dt)),
            then: Box::new(subst_ptr_loads(then, ptr_names, max, _dt)),
            els: Box::new(subst_ptr_loads(els, ptr_names, max, _dt)),
        },
        MExpr::Field { obj, field } => MExpr::Field {
            obj: Box::new(subst_ptr_loads(obj, ptr_names, max, _dt)),
            field: field.clone(),
        },
    }
}

fn print_fn(out: &mut String, f: &MFn, tier: Tier) {
    print_fn_indented(out, f, tier, 0);
}

fn print_fn_indented(out: &mut String, f: &MFn, tier: Tier, base: usize) {
    if let Some(c) = &f.comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            indent(out, base);
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    for d in &f.decorators {
        indent(out, base);
        out.push_str(d);
        out.push('\n');
    }
    indent(out, base);
    out.push_str("fn ");
    out.push_str(&snake(&f.name));
    if !f.cparams.is_empty() {
        out.push('[');
        for (i, (n, t)) in f.cparams.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(n);
            out.push_str(": ");
            out.push_str(t);
        }
        out.push(']');
    }
    out.push('(');
    let mut idx = 0;
    if f.is_method {
        out.push_str("self");
        idx = 1;
    }
    for (i, (n, t, c)) in f.params.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        idx += 1;
        out.push_str(c.as_prefix());
        out.push_str(&snake(n));
        if !matches!(t, MType::Infer) {
            out.push_str(": ");
            out.push_str(&t.as_str());
        }
        if let Some(Some(def)) = f.param_defaults.get(i) {
            out.push_str(" = ");
            print_expr(out, def);
        }
    }
    out.push(')');
    if f.raises {
        out.push_str(" raises");
    }
    if !matches!(f.ret, MType::Infer) {
        out.push_str(" -> ");
        out.push_str(&f.ret.as_str());
    }
    out.push_str(":\n");
    if let Some(doc) = &f.docstring {
        indent(out, base + 1);
        out.push_str("\"\"\"");
        // Triple-quoted docstrings: escape backslashes and triple-quotes.
        let escaped = doc.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
        out.push_str(&escaped);
        out.push_str("\"\"\"\n");
    }
    if f.body.is_empty() {
        if f.docstring.is_none() {
            indent(out, base + 1);
            out.push_str("pass\n");
        }
    } else {
        for s in &f.body {
            print_stmt(out, s, base + 1);
        }
    }
}

fn indent(out: &mut String, lvl: usize) {
    for _ in 0..lvl {
        out.push_str("    ");
    }
}

fn print_stmt(out: &mut String, s: &MStmt, lvl: usize) {
    match s {
        MStmt::Let { name, ty, value } => {
            indent(out, lvl);
            out.push_str("var ");
            out.push_str(&snake(name));
            if !matches!(ty, MType::Infer) {
                out.push_str(": ");
                out.push_str(&ty.as_str());
            }
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
        MStmt::Assign { name, value } => {
            indent(out, lvl);
            out.push_str(&snake(name));
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
        MStmt::Return(e) => {
            indent(out, lvl);
            out.push_str("return ");
            print_expr_arg(out, e);
            out.push('\n');
        }
        MStmt::Expr(e) => {
            indent(out, lvl);
            print_expr(out, e);
            out.push('\n');
        }
        MStmt::If { cond, then, els } => {
            indent(out, lvl);
            out.push_str("if ");
            print_expr(out, cond);
            out.push_str(":\n");
            if then.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in then {
                    print_stmt(out, s, lvl + 1);
                }
            }
            // Flatten cond chains: `else: if X: ... else: ...` → `elif X: ... else: ...`
            let mut tail = els;
            loop {
                if tail.is_empty() {
                    break;
                }
                if tail.len() == 1 {
                    if let MStmt::If { cond: ec, then: et, els: ee } = &tail[0] {
                        indent(out, lvl);
                        out.push_str("elif ");
                        print_expr(out, ec);
                        out.push_str(":\n");
                        if et.is_empty() {
                            indent(out, lvl + 1);
                            out.push_str("pass\n");
                        } else {
                            for s in et {
                                print_stmt(out, s, lvl + 1);
                            }
                        }
                        tail = ee;
                        continue;
                    }
                }
                indent(out, lvl);
                out.push_str("else:\n");
                for s in tail {
                    print_stmt(out, s, lvl + 1);
                }
                break;
            }
        }
        MStmt::While { cond, body } => {
            indent(out, lvl);
            out.push_str("while ");
            print_expr(out, cond);
            out.push_str(":\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
        MStmt::Break => {
            indent(out, lvl);
            out.push_str("break\n");
        }
        MStmt::Continue => {
            indent(out, lvl);
            out.push_str("continue\n");
        }
        MStmt::Raise(e) => {
            indent(out, lvl);
            out.push_str("raise ");
            print_expr(out, e);
            out.push('\n');
        }
        MStmt::ReRaise => {
            indent(out, lvl);
            out.push_str("raise\n");
        }
        MStmt::Try { body, catches } => {
            indent(out, lvl);
            out.push_str("try:\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
            for c in catches {
                indent(out, lvl);
                out.push_str("except");
                if !c.ty.is_empty() {
                    out.push(' ');
                    out.push_str(&c.ty);
                }
                if let Some(n) = &c.name {
                    out.push_str(" as ");
                    out.push_str(n);
                }
                out.push_str(":\n");
                if c.body.is_empty() {
                    indent(out, lvl + 1);
                    out.push_str("pass\n");
                } else {
                    for s in &c.body {
                        print_stmt(out, s, lvl + 1);
                    }
                }
            }
        }
        MStmt::ParameterIf { cond, then, els } => {
            indent(out, lvl);
            out.push_str("@parameter\n");
            indent(out, lvl);
            out.push_str("if ");
            print_expr(out, cond);
            out.push_str(":\n");
            if then.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in then {
                    print_stmt(out, s, lvl + 1);
                }
            }
            if !els.is_empty() {
                indent(out, lvl);
                out.push_str("else:\n");
                for s in els {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
        MStmt::Raw(s) => {
            indent(out, lvl);
            out.push_str(s);
            out.push('\n');
        }
        MStmt::ForIn { name, ty: _, iter, body } => {
            indent(out, lvl);
            out.push_str("for ");
            out.push_str(&snake(name));
            out.push_str(" in ");
            print_expr(out, iter);
            out.push_str(":\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
        MStmt::ForRange { name, ty: _, lo, hi, body } => {
            indent(out, lvl);
            out.push_str("for ");
            out.push_str(&snake(name));
            out.push_str(" in range(");
            print_expr(out, lo);
            out.push_str(", ");
            print_expr(out, hi);
            out.push_str("):\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
    }
}

/// Public shim so tier1 can format expressions into its own buffers.
pub(crate) fn print_expr_public(out: &mut String, e: &MExpr) {
    print_expr(out, e);
}

/// Like `print_expr`, but if the outermost expression is a BinOp / UnOp /
/// IfExpr (which self-wrap with parentheses), emit it without adding a
/// caller-side extra set. Used at sites like `(body).store(out, i)` where
/// a redundant outer pair would produce `((a + b)).store(...)`.
fn print_expr_grouped(out: &mut String, e: &MExpr) {
    match e {
        MExpr::BinOp { .. } | MExpr::UnOp { .. } | MExpr::IfExpr { .. } => {
            // Emit exactly one wrapping pair: `(lhs op rhs).method()` rather
            // than `((lhs op rhs)).method()` (self-wrap + caller wrap).
            out.push('(');
            print_expr_arg(out, e);
            out.push(')');
        }
        _ => {
            // Everything else (literal, var, call, field) has no outer paren
            // — add one to keep `.store(...)` / `.reduce_add()` legal.
            out.push('(');
            print_expr(out, e);
            out.push(')');
        }
    }
}

/// Like `print_expr`, but for positions where surrounding punctuation
/// already delimits the expression (call arg, collection element). In
/// these contexts the outer paren BinOp/UnOp/IfExpr emits is redundant
/// and produces `foo((a + b))` — skip it.
fn print_expr_arg(out: &mut String, e: &MExpr) {
    match e {
        MExpr::BinOp { op, lhs, rhs } => {
            let prec = binop_prec(op);
            print_binop_operand(out, lhs, prec);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            print_binop_operand(out, rhs, prec);
        }
        MExpr::UnOp { op, rhs } => {
            out.push_str(op);
            if op.chars().next().map_or(false, |c| c.is_alphabetic()) {
                out.push(' ');
            }
            print_expr(out, rhs);
        }
        MExpr::IfExpr { cond, then, els } => {
            print_expr(out, then);
            out.push_str(" if ");
            print_expr(out, cond);
            out.push_str(" else ");
            print_expr(out, els);
        }
        _ => print_expr(out, e),
    }
}

/// Python/Mojo binary-operator precedence. Higher = binds tighter.
/// Used to suppress redundant parens when an inner BinOp is at least
/// as tight as the outer one on the correct side.
fn binop_prec(op: &str) -> u8 {
    match op {
        "or" => 1,
        "and" => 2,
        "<" | ">" | "<=" | ">=" | "==" | "!=" => 4,
        "|" => 5,
        "^" => 6,
        "&" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "//" | "%" => 10,
        "**" => 12,
        _ => 0,
    }
}

/// Print a BinOp/UnOp operand, adding parens only when precedence demands.
/// `outer_prec` is the parent BinOp's precedence; `is_rhs` matters for
/// left-associative ops where equal precedence on the rhs still needs parens
/// (but we conservatively allow equal precedence on either side — this is
/// fine for +,-,*,/ because they're associative up to floating-point rounding,
/// and the emitter already chose a tree that evaluates correctly).
fn print_binop_operand(out: &mut String, e: &MExpr, outer_prec: u8) {
    if let MExpr::BinOp { op, lhs, rhs } = e {
        let inner_prec = binop_prec(op);
        if inner_prec >= outer_prec && inner_prec > 0 && outer_prec > 0 {
            // No parens — inner binds at least as tight.
            print_binop_operand(out, lhs, inner_prec);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            print_binop_operand(out, rhs, inner_prec);
            return;
        }
    }
    print_expr(out, e);
}

fn print_expr(out: &mut String, e: &MExpr) {
    match e {
        MExpr::IntLit(i) => {
            out.push_str(&i.to_string());
        }
        MExpr::FloatLit(f) => {
            let s = format_float(*f);
            out.push_str(&s);
        }
        MExpr::BoolLit(b) => out.push_str(if *b { "True" } else { "False" }),
        MExpr::Var(n) => out.push_str(&snake(n)),
        MExpr::BinOp { op, lhs, rhs } => {
            let prec = binop_prec(op);
            out.push('(');
            print_binop_operand(out, lhs, prec);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            print_binop_operand(out, rhs, prec);
            out.push(')');
        }
        MExpr::UnOp { op, rhs } => {
            out.push('(');
            out.push_str(op);
            if op.chars().next().map_or(false, |c| c.is_alphabetic()) {
                out.push(' ');
            }
            print_expr(out, rhs);
            out.push(')');
        }
        MExpr::Call { callee, args } => {
            // Special virtual callees for indexing, slicing, and method calls.
            if callee == "__index__" && args.len() == 2 {
                print_expr(out, &args[0]);
                out.push('[');
                print_expr_arg(out, &args[1]);
                out.push(']');
                return;
            }
            if callee == "__slice__" && args.len() == 3 {
                print_expr(out, &args[0]);
                out.push('[');
                print_expr_arg(out, &args[1]);
                out.push(':');
                print_expr_arg(out, &args[2]);
                out.push(']');
                return;
            }
            if let Some(method) = callee.strip_prefix("__method__") {
                // First arg is receiver, rest are method args.
                if !args.is_empty() {
                    print_expr(out, &args[0]);
                    out.push('.');
                    out.push_str(&snake(method));
                    out.push('(');
                    for (i, a) in args[1..].iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        print_expr_arg(out, a);
                    }
                    out.push(')');
                    return;
                }
            }
            // Don't mangle callees that carry Mojo-level syntax (brackets,
            // dots, double-underscore sentinels) — those are already valid.
            if callee.contains('[') || callee.contains('.') || callee.starts_with("__") {
                out.push_str(callee);
            } else {
                out.push_str(&snake(callee));
            }
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr_arg(out, a);
            }
            out.push(')');
        }
        MExpr::IfExpr { cond, then, els } => {
            out.push('(');
            print_expr(out, then);
            out.push_str(" if ");
            print_expr(out, cond);
            out.push_str(" else ");
            print_expr(out, els);
            out.push(')');
        }
        MExpr::Field { obj, field } => {
            print_expr(out, obj);
            out.push('.');
            out.push_str(&snake(field));
        }
        MExpr::StrLit(s) => {
            out.push('"');
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
            out.push('"');
        }
    }
}

#[cfg(test)]
mod closure_tests {
    use super::{emit, Tier};

    #[test]
    fn fn_type_tag_parses_simple() {
        // A defn-mojo that accepts a `^fn/f32->f32` parameter.
        let src = "(defn-mojo apply-twice ^f32 [^fn/f32->f32 f ^f32 x] (f (f x)))";
        let out = emit(src, Tier::Readable).unwrap();
        assert!(
            out.contains("f: fn(Float32) -> Float32"),
            "expected fn-type param, got:\n{out}"
        );
        assert!(out.contains("f(f(x))"), "expected nested call, got:\n{out}");
    }

    #[test]
    fn fn_type_tag_two_params() {
        let src = "(defn-mojo zip-with ^f32 [^fn/f32|f32->f32 op ^f32 a ^f32 b] (op a b))";
        let out = emit(src, Tier::Readable).unwrap();
        assert!(
            out.contains("op: fn(Float32, Float32) -> Float32"),
            "got:\n{out}"
        );
        assert!(out.contains("op(a, b)"), "got:\n{out}");
    }

    #[test]
    fn anon_fn_hoists_to_helper() {
        // A defn-mojo that passes an anonymous fn to apply-twice.
        let src = "\
(defn-mojo apply-twice ^f32 [^fn/f32->f32 f ^f32 x] (f (f x)))
(defn-mojo use-it ^f32 [^f32 x] (apply-twice (fn ^f32 [^f32 y] (+ y 1.0)) x))";
        let out = emit(src, Tier::Readable).unwrap();
        // The anon fn should appear as a hoisted helper.
        assert!(
            out.contains("fn __anon_fn0("),
            "expected hoisted anon fn, got:\n{out}"
        );
        assert!(
            out.contains("apply_twice(__anon_fn0, x)"),
            "call site should reference hoisted name, got:\n{out}"
        );
    }

    #[test]
    fn anon_fn_bad_signature_errors() {
        // Missing params vector.
        let src = "(defn-mojo bad ^f32 [^f32 x] ((fn 1) x))";
        assert!(emit(src, Tier::Readable).is_err());
    }
}

#[cfg(test)]
mod mixed_precision_tests {
    use super::{emit, Tier};

    #[test]
    fn reduce_fp16_to_fp32_emits_mixed_load_and_cast() {
        // Sum-of-squares: BFloat16 input, Float32 accumulator.
        let src = "(reduce-mojo sum-sq-f16 [^bf16 x] ^f32 (* (cast-mojo ^f32 x) (cast-mojo ^f32 x)) 0.0)";
        let out = emit(src, Tier::Max).unwrap();
        // Input pointer uses source dtype.
        assert!(out.contains("x: UnsafePointer[BFloat16]"), "got:\n{out}");
        // Return type is the accumulator dtype.
        assert!(out.contains("-> Float32"), "got:\n{out}");
        // SIMD load uses the input dtype.
        assert!(out.contains("SIMD[DType.bfloat16"), "got:\n{out}");
        // Cast to f32 appears.
        assert!(out.contains("cast[DType.float32]"), "got:\n{out}");
    }

    #[test]
    fn elementwise_fp32_to_fp16_emits_cast_store() {
        // Takes f32, writes bf16 via cast.
        let src = "(elementwise-mojo downcast [^f32 x] ^bf16 (cast-mojo ^bf16 x))";
        let out = emit(src, Tier::Max).unwrap();
        assert!(out.contains("x: UnsafePointer[Float32]"), "got:\n{out}");
        assert!(out.contains("out: UnsafePointer[BFloat16]"), "got:\n{out}");
        assert!(out.contains("cast[DType.bfloat16]"), "got:\n{out}");
    }

    #[test]
    fn scalar_cast_in_plain_defn_works() {
        // Outside a kernel: `(cast-mojo ^i32 x)` in a defn-mojo body.
        let src = "(defn-mojo truncf ^i32 [^f32 x] (cast-mojo ^i32 x))";
        let out = emit(src, Tier::Readable).unwrap();
        // Scalar Mojo types also accept `.cast[DType.int32]()`.
        assert!(out.contains("x.cast[DType.int32]()"), "got:\n{out}");
    }

    #[test]
    fn mixed_dtypes_without_cast_still_errors() {
        // The old error path is preserved when no cast appears.
        let src = "(elementwise-mojo bad [^f32 a ^f64 b] ^f32 (+ a b))";
        let err = emit(src, Tier::Max).unwrap_err();
        assert!(err.contains("share the same dtype"), "got: {err}");
    }
}

#[cfg(test)]
mod multi_output_tests {
    use super::{emit, Tier};

    #[test]
    fn two_output_kernel_emits_two_pointers_and_stores() {
        // Split each input into (2x, 2x+1).
        let src = "(elementwise-mojo split-even-odd [^i32 x] ^Tuple-i32-i32 (tuple (* x 2) (+ (* x 2) 1)))";
        for tier in [Tier::Readable, Tier::Optimized, Tier::Max] {
            let out = emit(src, tier).unwrap();
            assert!(out.contains("out0: UnsafePointer[Int32]"), "tier {:?}:\n{out}", tier);
            assert!(out.contains("out1: UnsafePointer[Int32]"), "tier {:?}:\n{out}", tier);
            assert!(out.contains("out0") && out.contains("out1"),
                "expected two stores, tier {:?}:\n{out}", tier);
        }
    }

    #[test]
    fn three_output_kernel_works() {
        // (x, x*x, x+1)
        let src = "(elementwise-mojo triple [^f32 x] ^Tuple-f32-f32-f32 (tuple x (* x x) (+ x 1.0)))";
        let out = emit(src, Tier::Readable).unwrap();
        assert!(out.contains("out0: UnsafePointer[Float32]"), "got:\n{out}");
        assert!(out.contains("out1: UnsafePointer[Float32]"), "got:\n{out}");
        assert!(out.contains("out2: UnsafePointer[Float32]"), "got:\n{out}");
    }

    #[test]
    fn multi_output_without_tuple_body_errors() {
        let src = "(elementwise-mojo bad [^i32 x] ^Tuple-i32-i32 (+ x 1))";
        let err = emit(src, Tier::Readable).unwrap_err();
        assert!(err.contains("requires body `(tuple ...)`"), "got: {err}");
    }

    #[test]
    fn multi_output_tuple_arity_mismatch_errors() {
        let src = "(elementwise-mojo bad [^i32 x] ^Tuple-i32-i32 (tuple x x x))";
        let err = emit(src, Tier::Readable).unwrap_err();
        assert!(err.contains("body tuple has 3 elements"), "got: {err}");
    }
}

#[cfg(test)]
mod docstring_tests {
    use super::{emit, Tier};

    #[test]
    fn docstring_emits_as_triple_quoted_string() {
        let src = r#"(defn-mojo ^{:doc "Adds two floats."} ^f32 add [^f32 x ^f32 y] (+ x y))"#;
        for tier in [Tier::Readable, Tier::Optimized, Tier::Max] {
            let out = emit(src, tier).unwrap();
            assert!(
                out.contains(r#""""Adds two floats.""""#),
                "tier {:?} missing docstring:\n{out}",
                tier
            );
        }
    }

    #[test]
    fn docstring_with_multiline_content() {
        let src = r#"(defn-mojo ^{:doc "Line one.\nLine two."} ^f32 f [^f32 x] x)"#;
        let out = emit(src, Tier::Readable).unwrap();
        // The reader unescapes \n in strings.
        assert!(out.contains(r#""""Line one."#), "got:\n{out}");
    }

    #[test]
    fn struct_docstring_absent_does_not_emit_quoted_string() {
        let src = "(defn-mojo ^f32 f [^f32 x] x)";
        let out = emit(src, Tier::Readable).unwrap();
        assert!(!out.contains(r#"""""#), "unexpected docstring marker:\n{out}");
    }
}

#[cfg(test)]
mod decorator_tests {
    use super::{emit, Tier};

    #[test]
    fn register_passable_decorator() {
        let src = r#"(defstruct-mojo ^{:decorators [:register-passable]} Point [^f32 x ^f32 y])"#;
        let out = emit(src, Tier::Readable).unwrap();
        assert!(out.contains("@register_passable"), "got:\n{out}");
        assert!(out.contains("struct Point"), "got:\n{out}");
    }

    #[test]
    fn staticmethod_decorator_on_fn() {
        let src = r#"(defn-mojo ^{:decorators [:staticmethod]} ^i32 clamp01 [^i32 x] x)"#;
        let out = emit(src, Tier::Readable).unwrap();
        assert!(out.contains("@staticmethod"), "got:\n{out}");
    }

    #[test]
    fn multiple_decorators_stack_in_order() {
        let src = r#"(defn-mojo ^{:decorators [:always-inline :parameter]} ^f32 f [^f32 x] x)"#;
        let out = emit(src, Tier::Readable).unwrap();
        assert!(out.contains("@always_inline"), "got:\n{out}");
        assert!(out.contains("@parameter"), "got:\n{out}");
        // Both should appear before the `fn` line.
        let ai_pos = out.find("@always_inline").unwrap();
        let pm_pos = out.find("@parameter").unwrap();
        let fn_pos = out.find("fn f(").unwrap();
        assert!(ai_pos < fn_pos && pm_pos < fn_pos, "decorators should precede fn:\n{out}");
    }
}

#[cfg(test)]
mod gather_scatter_tests {
    use super::{emit, Tier};

    #[test]
    fn gather_emits_indexed_load_loop() {
        let src = "(gather-mojo gf32 [^f32 values ^i32 indices])";
        for tier in [Tier::Readable, Tier::Optimized, Tier::Max] {
            let out = emit(src, tier).unwrap();
            assert!(out.contains("fn gf32("), "tier {:?}:\n{out}", tier);
            assert!(out.contains("values: UnsafePointer[Float32]"), "tier {:?}:\n{out}", tier);
            assert!(out.contains("indices: UnsafePointer[Int32]"), "tier {:?}:\n{out}", tier);
            assert!(out.contains("out[i] = values[Int(indices[i])]"),
                "tier {:?}:\n{out}", tier);
        }
    }

    #[test]
    fn gather_missing_annotation_errors() {
        let src = "(gather-mojo bad [values ^i32 indices])";
        assert!(emit(src, Tier::Readable).is_err());
    }

    #[test]
    fn scatter_emits_indexed_store_loop() {
        let src = "(scatter-mojo sf32 [^i32 indices ^f32 values])";
        let out = emit(src, Tier::Readable).unwrap();
        assert!(out.contains("fn sf32("), "got:\n{out}");
        assert!(out.contains("out[Int(indices[i])] = values[i]"), "got:\n{out}");
    }

    #[test]
    fn scatter_integer_dtype_works() {
        let src = "(scatter-mojo si32 [^i32 indices ^i32 values])";
        let out = emit(src, Tier::Max).unwrap();
        assert!(out.contains("indices: UnsafePointer[Int32]"), "got:\n{out}");
        assert!(out.contains("values: UnsafePointer[Int32]"), "got:\n{out}");
        assert!(out.contains("out: UnsafePointer[Int32]"), "got:\n{out}");
    }
}

#[cfg(test)]
mod paren_tests {
    use super::{emit, Tier};

    fn assert_no_double_parens(src: &str) {
        for tier in [Tier::Readable, Tier::Optimized, Tier::Max] {
            let out = emit(src, tier).expect("emit");
            assert!(!out.contains("(("), "doubled parens in tier {:?}:\n{out}", tier);
        }
    }

    #[test]
    fn elementwise_vector_add_has_no_double_parens() {
        assert_no_double_parens(
            "(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))",
        );
    }

    #[test]
    fn reduce_sum_sq_has_no_double_parens() {
        assert_no_double_parens(
            "(reduce-mojo sum-sq [^f32 x] ^f32 (* x x) 0.0)",
        );
    }

    #[test]
    fn reduce_dot_has_no_double_parens() {
        assert_no_double_parens(
            "(reduce-mojo dot [^f32 a ^f32 b] ^f32 (* a b) 0.0)",
        );
    }

    #[test]
    fn defn_expr_body_has_no_double_parens() {
        assert_no_double_parens(
            "(defn ^f32 f [^f32 a ^f32 b ^f32 c] (+ a (* b c)))",
        );
    }
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "Float64.nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Float64.inf".into() } else { "-Float64.inf".into() };
    }
    if f == f.trunc() && f.abs() < 1e16 {
        format!("{:.1}", f)
    } else {
        // Use Rust's default, which is shortest round-trippable.
        let s = format!("{f}");
        if s.contains('.') || s.contains('e') || s.contains('E') {
            s
        } else {
            format!("{s}.0")
        }
    }
}
