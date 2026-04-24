//! cljrs → Mojo-AST lowering. Tier 1: faithful, no optimization.
//!
//! The shape of the input is `cljrs::value::Value` trees produced by
//! `cljrs::reader::read_all`. Type hints come through as
//! `(__tagged__ <tag> <form>)` sentinel lists (see reader.rs).

use std::cell::RefCell;

use cljrs::value::Value;

use crate::ast::{MCatch, MExpr, MFn, MItem, MModule, MStmt, MTraitMethod, MType, ParamConv, ReduceOp};
use crate::runtime;

/// Lowering context. Tracks imports that get lazily added as we encounter
/// `math.*` calls. Also holds a gensym counter for loop/cond fallbacks.
pub struct Ctx {
    imports: RefCell<Vec<String>>,
    gensym: RefCell<u32>,
    /// Pending struct methods, keyed by struct name. Drained after the
    /// initial pass and merged into the matching `MItem::Struct`.
    pub methods: RefCell<Vec<(String, MFn)>>,
    /// Whether we're currently lowering a parametric fn body (controls
    /// `(parameter-if ...)` legality).
    in_parametric: RefCell<bool>,
}

impl Ctx {
    pub fn new() -> Self {
        Ctx {
            imports: RefCell::new(Vec::new()),
            gensym: RefCell::new(0),
            methods: RefCell::new(Vec::new()),
            in_parametric: RefCell::new(false),
        }
    }
    fn need_import(&self, line: &str) {
        let mut v = self.imports.borrow_mut();
        if !v.iter().any(|s| s == line) {
            v.push(line.to_string());
        }
    }
    fn gensym(&self, prefix: &str) -> String {
        let mut n = self.gensym.borrow_mut();
        let id = *n;
        *n += 1;
        format!("__{prefix}{id}")
    }
    pub fn take_imports(self) -> Vec<String> {
        self.imports.into_inner()
    }
}

/// Lower a whole file of cljrs forms into an `MModule`.
pub fn lower_module(forms: &[Value]) -> Result<MModule, String> {
    let ctx = Ctx::new();
    let mut items = Vec::new();
    for form in forms {
        items.extend(lower_top(&ctx, form)?);
    }
    // Attach pending methods to their structs.
    let pending = std::mem::take(&mut *ctx.methods.borrow_mut());
    for (sname, m) in pending {
        let mut placed = false;
        for it in items.iter_mut() {
            if let MItem::Struct { name, methods, .. } = it {
                if name == &sname {
                    methods.push(m.clone());
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            return Err(format!(
                "defn-method-mojo: no struct named `{sname}` defined before this method"
            ));
        }
    }
    let imports = ctx.take_imports();
    Ok(MModule { imports, items })
}

fn lower_top(ctx: &Ctx, form: &Value) -> Result<Vec<MItem>, String> {
    let list = match as_list(form) {
        Some(l) => l,
        None => return Err(format!("top-level form must be a list: {}", pr(form))),
    };
    let head = list.first().and_then(sym_str).ok_or_else(|| {
        format!("top-level form must start with a symbol: {}", pr(form))
    })?;
    match head {
        "def" => lower_def(ctx, list, form).map(|i| vec![i]),
        "defn" | "defn-mojo" => lower_defn_any(ctx, list, form, &[]),
        "parameter-fn-mojo" => lower_defn_any(ctx, list, form, &["@parameter"]),
        "always-inline-fn-mojo" => lower_defn_any(ctx, list, form, &["@always_inline"]),
        "raises-fn-mojo" => lower_raises_fn(ctx, list, form).map(|i| vec![i]),
        "parametric-fn-mojo" => lower_parametric_fn(ctx, list, form).map(|i| vec![i]),
        "defstruct-mojo" => lower_defstruct(ctx, list, form).map(|i| vec![i]),
        "deftrait-mojo" => lower_deftrait(list, form).map(|i| vec![i]),
        "alias-mojo" => lower_alias(ctx, list, form).map(|i| vec![i]),
        "defn-method-mojo" => lower_defn_method(ctx, list, form).map(|_| Vec::new()),
        "elementwise-mojo" => lower_elementwise(ctx, list, form, false).map(|i| vec![i]),
        "parallel-mojo" => lower_elementwise(ctx, list, form, true).map(|i| vec![i]),
        "reduce-mojo" => lower_reduce(ctx, list, form).map(|i| vec![i]),
        "elementwise-gpu-mojo" => lower_gpu_elementwise(ctx, list, form).map(|i| vec![i]),
        other => Err(format!(
            "unsupported top-level form `{other}` in: {}",
            pr(form)
        )),
    }
}

/// Dispatcher: single-arity → `lower_defn`; multi-arity (body = list of
/// `([args] body...)` groups) → multiple Fn items with `_N` suffixes.
fn lower_defn_any(
    ctx: &Ctx,
    list: &[Value],
    form: &Value,
    extras: &[&str],
) -> Result<Vec<MItem>, String> {
    // Detect multi-arity shape: (defn NAME ([args] body) ([args2] body2) ...)
    // i.e. none of list[2..] is a Vector (they're all lists starting with a
    // Vector). We also allow a ^RET tag on the name in either case.
    let arity_slice_start = 2;
    // Peek list[arity_slice_start]: if it's a Vector, single arity; else multi.
    let first = match list.get(arity_slice_start) {
        Some(v) => v,
        None => return lower_defn(ctx, list, form, extras).map(|i| vec![i]),
    };
    let (_, first_peeled) = peel_tag(first);
    if matches!(first_peeled, Value::Vector(_)) {
        return lower_defn(ctx, list, form, extras).map(|i| vec![i]);
    }
    // Multi-arity path.
    let (name_tag, name_form) = peel_tag(&list[1]);
    let name = sym_str(name_form)
        .ok_or_else(|| format!("defn name must be a symbol: {}", pr(form)))?
        .to_string();
    let mut out = Vec::new();
    // Per-arity shared return tag, if any. It may sit on the name
    // (`(defn ^RET name ...)`) or on the first arity group
    // (`(defn name ^RET ([args] body) ...)`). We capture whichever fires.
    let mut shared_ret = name_tag.clone();
    for (idx, group) in list[arity_slice_start..].iter().enumerate() {
        let (gty, group_inner) = peel_tag(group);
        if idx == 0 && matches!(shared_ret, MType::Infer) {
            shared_ret = gty.clone();
        }
        let glist = match as_list(group_inner) {
            Some(l) => l,
            None => return Err(format!("multi-arity group must be a list: {}", pr(group))),
        };
        // Synthesize a single-arity defn call: (defn NAME ^RET [args] body...)
        let suffixed = if idx == 0 {
            name.clone()
        } else {
            format!("{name}_{}", idx + 1)
        };
        let name_val = if !matches!(shared_ret, MType::Infer) {
            let tag_sym = mtype_to_tag_symbol(&shared_ret);
            Value::List(std::sync::Arc::new(vec![
                Value::Symbol("__tagged__".into()),
                Value::Symbol(tag_sym.into()),
                Value::Symbol(suffixed.into()),
            ]))
        } else {
            Value::Symbol(suffixed.into())
        };
        let mut synth: Vec<Value> = Vec::with_capacity(glist.len() + 2);
        synth.push(Value::Symbol("defn".into()));
        synth.push(name_val);
        for g in glist.iter() {
            synth.push(g.clone());
        }
        let synth_form = Value::List(std::sync::Arc::new(synth));
        let synth_list = as_list(&synth_form).unwrap().to_vec();
        let item = lower_defn(ctx, &synth_list, &synth_form, extras)?;
        out.push(item);
    }
    Ok(out)
}

/// Best-effort reverse of runtime::type_hint. Used when re-synthesizing a
/// defn form for multi-arity lowering. Falls back to the type's pretty name.
fn mtype_to_tag_symbol(t: &MType) -> String {
    match t {
        MType::Int8 => "i8".into(),
        MType::Int16 => "i16".into(),
        MType::Int32 => "i32".into(),
        MType::Int64 => "i64".into(),
        MType::UInt8 => "u8".into(),
        MType::UInt16 => "u16".into(),
        MType::UInt32 => "u32".into(),
        MType::UInt64 => "u64".into(),
        MType::Float32 => "f32".into(),
        MType::Float64 => "f64".into(),
        MType::BFloat16 => "bf16".into(),
        MType::Bool => "bool".into(),
        MType::Str => "str".into(),
        MType::Named(s) => s.clone(),
        MType::Simd(_, _) | MType::Infer => "Infer".into(),
        MType::SimdParam(_, _) | MType::List(_) | MType::Optional(_) | MType::Tuple(_) => {
            "Infer".into()
        }
    }
}

fn lower_defstruct(_ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    // (defstruct-mojo NAME [fields])
    // (defstruct-mojo NAME :TraitName [fields])
    // Generic: (defstruct-mojo Name[T] [fields]) or (defstruct-mojo Name[T AnyType N Int] [fields])
    // The generic param vector is encoded at list[2] as Value::Vector when
    // the user writes it; we accept either `(defstruct-mojo Name[T] [fields])`
    // parsed as two vectors or the sugar form `(defstruct-mojo Name [T] [fields])`.
    if list.len() < 3 {
        return Err(format!("defstruct-mojo expects (defstruct-mojo NAME [fields]): {}", pr(form)));
    }
    // Struct name may carry `^{:decorators [:register-passable]}` meta.
    let (struct_decorators, _doc, name_form) = peel_name_meta(&list[1]);
    let name = sym_str(name_form)
        .ok_or_else(|| format!("defstruct-mojo name must be symbol: {}", pr(form)))?
        .to_string();
    let mut idx = 2;
    let mut trait_impl: Option<String> = None;
    if let Some(Value::Keyword(k)) = list.get(idx) {
        trait_impl = Some(k.to_string());
        idx += 1;
    }
    // Optional generic params vector: must be before the fields vector.
    // We distinguish fields from cparams by peeking at the first element —
    // fields start with either a type-tagged symbol (`^T field`) or a bare
    // field symbol; cparams start with a bare type-param symbol followed
    // by either another bare symbol or (in multi-param form) a type name.
    // Simpler heuristic: if list[idx] is a Vector AND list[idx+1] exists
    // and is also a Vector, then list[idx] is cparams and list[idx+1] is fields.
    let mut cparams: Vec<(String, String)> = Vec::new();
    let looks_generic = matches!(list.get(idx), Some(Value::Vector(_)))
        && matches!(list.get(idx + 1), Some(Value::Vector(_)));
    if looks_generic {
        if let Some(Value::Vector(cv)) = list.get(idx) {
            // Accept either a single `[T]` (shorthand for `[T: AnyType]`)
            // or flat pairs `[T AnyType N Int]`.
            if cv.len() == 1 {
                let n = sym_str(&cv[0]).ok_or_else(|| {
                    format!("defstruct-mojo generic param must be a symbol: {}", pr(form))
                })?;
                cparams.push((n.to_string(), "AnyType".to_string()));
            } else if cv.len() % 2 == 0 {
                let mut i = 0;
                while i < cv.len() {
                    let n = sym_str(&cv[i]).ok_or_else(|| {
                        format!("defstruct-mojo generic param name must be a symbol: {}", pr(form))
                    })?;
                    let t = sym_str(&cv[i + 1]).ok_or_else(|| {
                        format!("defstruct-mojo generic param bound must be a symbol: {}", pr(form))
                    })?;
                    cparams.push((n.to_string(), t.to_string()));
                    i += 2;
                }
            } else {
                return Err(format!(
                    "defstruct-mojo generic params must be [T] or pairs [T Bound ...]: {}",
                    pr(form)
                ));
            }
            idx += 1;
        }
    }
    let fields_form = list.get(idx).ok_or_else(|| {
        format!("defstruct-mojo missing fields vector: {}", pr(form))
    })?;
    let fields_vec = match fields_form {
        Value::Vector(v) => v,
        _ => return Err(format!("defstruct-mojo fields must be a vector: {}", pr(form))),
    };
    let mut fields: Vec<(String, MType)> = Vec::new();
    for f in fields_vec.iter() {
        let (ty, nf) = peel_tag(f);
        let fname = sym_str(nf)
            .ok_or_else(|| format!("defstruct-mojo field name must be symbol: {}", pr(form)))?
            .to_string();
        fields.push((fname, ty));
    }
    Ok(MItem::Struct {
        name,
        fields,
        methods: Vec::new(),
        trait_impl,
        cparams,
        decorators: struct_decorators,
        comment: Some(pr(form)),
    })
}

fn lower_def(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    // (def ^T name value) or (def name value)
    if list.len() != 3 {
        return Err(format!("def expects 2 args: {}", pr(form)));
    }
    let (ty, name_form) = peel_tag(&list[1]);
    let name = sym_str(name_form)
        .ok_or_else(|| format!("def name must be a symbol: {}", pr(form)))?
        .to_string();
    let value = lower_expr(ctx, &list[2])?;
    Ok(MItem::Var {
        name,
        ty,
        value,
        comment: Some(pr(form)),
    })
}

fn lower_defn(ctx: &Ctx, list: &[Value], form: &Value, extra_decorators: &[&str]) -> Result<MItem, String> {
    // (defn name ^RetT [^T arg ...] body...)
    //
    // Clojure's reader attaches `^Tag` metadata to whatever follows it,
    // so `(defn NAME ^RET [args] body)` reads as list[2] = (__tagged__
    // RET [args]). list[1] is the bare name (with an optional tag of
    // its own, which we also peel for permissiveness).
    if list.len() < 3 {
        return Err(format!("defn expects name, arg vec, body: {}", pr(form)));
    }
    // Return-type tag can sit on either the name (`(defn ^RET name …)`)
    // or on the arg vector (`(defn name ^RET […] …)`). Both are valid
    // Clojure reader input; accept either.
    let (name_tag, name_form) = peel_tag(&list[1]);
    // The name may carry map-meta `^{:doc "..."}`. Peel that too.
    let (docstring, name_form) = peel_doc_meta(name_form);
    let name = sym_str(name_form)
        .ok_or_else(|| format!("defn name must be a symbol: {}", pr(form)))?
        .to_string();
    let (args_tag, args_form) = peel_tag(&list[2]);
    let ret_ty = match (name_tag, args_tag) {
        (MType::Infer, t) => t,
        (t, MType::Infer) => t,
        (a, _) => a, // both set: prefer the one on the name
    };
    let params_vec = match args_form {
        Value::Vector(v) => v,
        _ => return Err(format!("defn arg vector expected: {}", pr(form))),
    };
    let mut params: Vec<(String, MType, ParamConv)> = Vec::new();
    let mut param_defaults: Vec<Option<MExpr>> = Vec::new();
    for p in params_vec.iter() {
        let (pty, pconv, pdefault, pname) = peel_param_tags_full(ctx, p)?;
        let s = sym_str(pname).ok_or_else(|| {
            format!("defn param must be a symbol: {}", pr(form))
        })?;
        if s.contains('&') {
            return Err(format!("variadic params not supported in cljrs-mojo v1: {}", pr(form)));
        }
        params.push((s.to_string(), pty, pconv));
        param_defaults.push(pdefault);
    }
    // Body: all forms after the arg vec. Exactly one expression is the
    // typical case; if there are several, wrap in an implicit `do`.
    let body_forms = &list[3..];
    let body_expr = if body_forms.len() == 1 {
        body_forms[0].clone()
    } else {
        // synthesize (do body...)
        let mut v = vec![Value::Symbol("do".into())];
        v.extend(body_forms.iter().cloned());
        Value::List(std::sync::Arc::new(v))
    };
    // Lower body as a tail-position expression and wrap in `return`.
    let mut stmts = Vec::new();
    lower_expr_tail(ctx, &body_expr, &mut stmts, TailMode::Return)?;
    Ok(MItem::Fn(MFn {
        name,
        params,
        param_defaults,
        ret: ret_ty,
        body: stmts,
        decorators: extra_decorators.iter().map(|s| s.to_string()).collect(),
        comment: Some(pr(form)),
        cparams: Vec::new(),
        raises: false,
        is_method: false,
        docstring,
    }))
}

// ---------------- Feature 1/3/4/9/10: extra top-level forms ----------------

/// `(raises-fn-mojo NAME ^RET [args] body...)` — emits `fn NAME(...) raises -> RET:`.
fn lower_raises_fn(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    // Identical shape to defn-mojo. Reuse lower_defn then flip raises.
    let mut synth = list.to_vec();
    synth[0] = Value::Symbol("defn".into());
    let synth_form = Value::List(std::sync::Arc::new(synth.clone()));
    let item = lower_defn(ctx, &synth, &synth_form, &[])?;
    if let MItem::Fn(mut f) = item {
        f.raises = true;
        f.comment = Some(pr(form));
        return Ok(MItem::Fn(f));
    }
    unreachable!("lower_defn returns MItem::Fn")
}

/// `(parametric-fn-mojo NAME [cparam-name CParamType ...] ^RET [args] body...)`
/// emits `fn NAME[n: Int, T: AnyType](args) -> RET:`. The cparam vector is
/// pairs flattened: `[n Int T AnyType]`.
fn lower_parametric_fn(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    if list.len() < 5 {
        return Err(format!(
            "parametric-fn-mojo expects (parametric-fn-mojo NAME [cparams] ^RET [args] body): {}",
            pr(form)
        ));
    }
    let name = sym_str(&list[1])
        .ok_or_else(|| format!("parametric-fn-mojo name must be symbol: {}", pr(form)))?
        .to_string();
    let cparam_vec = match &list[2] {
        Value::Vector(v) => v,
        _ => {
            return Err(format!(
                "parametric-fn-mojo cparams must be a vector: {}",
                pr(form)
            ));
        }
    };
    if cparam_vec.len() % 2 != 0 {
        return Err(format!(
            "parametric-fn-mojo cparams must have even length [name Type ...]: {}",
            pr(form)
        ));
    }
    let mut cparams: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < cparam_vec.len() {
        let n = sym_str(&cparam_vec[i]).ok_or_else(|| {
            format!("parametric cparam name must be a symbol: {}", pr(form))
        })?;
        let t = sym_str(&cparam_vec[i + 1]).ok_or_else(|| {
            format!("parametric cparam type must be a symbol: {}", pr(form))
        })?;
        cparams.push((n.to_string(), t.to_string()));
        i += 2;
    }
    // Build a synthetic defn for the args+body portion.
    // Shape after consuming list[2] (cparams): list[3] = ^RET / args, ...
    let mut synth: Vec<Value> = vec![
        Value::Symbol("defn".into()),
        Value::Symbol(name.clone().into()),
    ];
    synth.extend_from_slice(&list[3..]);
    let synth_form = Value::List(std::sync::Arc::new(synth.clone()));
    *ctx.in_parametric.borrow_mut() = true;
    let item = lower_defn(ctx, &synth, &synth_form, &[]);
    *ctx.in_parametric.borrow_mut() = false;
    let mut f = match item? {
        MItem::Fn(f) => f,
        _ => unreachable!(),
    };
    f.cparams = cparams;
    f.comment = Some(pr(form));
    Ok(MItem::Fn(f))
}

/// `(deftrait-mojo NAME (METHOD ^RET [args]) ...)` → `trait NAME:` block.
fn lower_deftrait(list: &[Value], form: &Value) -> Result<MItem, String> {
    if list.len() < 2 {
        return Err(format!("deftrait-mojo expects NAME and methods: {}", pr(form)));
    }
    let name = sym_str(&list[1])
        .ok_or_else(|| format!("deftrait-mojo name must be a symbol: {}", pr(form)))?
        .to_string();
    let mut methods: Vec<MTraitMethod> = Vec::new();
    for m in &list[2..] {
        let ml = match as_list(m) {
            Some(l) => l,
            None => return Err(format!("deftrait method must be a list: {}", pr(m))),
        };
        if ml.len() < 2 {
            return Err(format!("deftrait method needs name + args: {}", pr(m)));
        }
        let mname = sym_str(&ml[0])
            .ok_or_else(|| format!("trait method name must be symbol: {}", pr(m)))?
            .to_string();
        // Optional ^RET tag on the args vec.
        let (ret_ty, args_form) = peel_tag(&ml[1]);
        let pv = match args_form {
            Value::Vector(v) => v,
            _ => return Err(format!("trait method args must be vector: {}", pr(m))),
        };
        let mut params: Vec<(String, MType, ParamConv)> = Vec::new();
        for p in pv.iter() {
            let (pty, pconv, pn) = peel_param_tags(p);
            let pn = sym_str(pn)
                .ok_or_else(|| format!("trait method param must be a symbol: {}", pr(m)))?
                .to_string();
            params.push((pn, pty, pconv));
        }
        methods.push(MTraitMethod { name: mname, params, ret: ret_ty });
    }
    Ok(MItem::Trait { name, methods, comment: Some(pr(form)) })
}

/// `(alias-mojo NAME VALUE)` or `(alias-mojo ^T NAME VALUE)`.
fn lower_alias(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    if list.len() != 3 {
        return Err(format!("alias-mojo expects (alias-mojo [^T] NAME VALUE): {}", pr(form)));
    }
    let (ty, name_form) = peel_tag(&list[1]);
    let name = sym_str(name_form)
        .ok_or_else(|| format!("alias-mojo name must be a symbol: {}", pr(form)))?
        .to_string();
    let value = lower_expr(ctx, &list[2])?;
    Ok(MItem::Alias { name, ty, value, comment: Some(pr(form)) })
}

/// `(defn-method-mojo StructName method-name ^RET [args] body...)`. Stored
/// in `ctx.methods` and merged into the named struct after the module pass.
fn lower_defn_method(ctx: &Ctx, list: &[Value], form: &Value) -> Result<(), String> {
    if list.len() < 4 {
        return Err(format!(
            "defn-method-mojo expects (defn-method-mojo StructName method-name [args] body): {}",
            pr(form)
        ));
    }
    let sname = sym_str(&list[1])
        .ok_or_else(|| format!("defn-method-mojo struct name must be symbol: {}", pr(form)))?
        .to_string();
    // Optional generic marker after struct name: `(defn-method-mojo Vec3 [T] length [] ...)`
    // The `[T]` propagates the struct's compile-time params into the
    // method body. Inside the method, references like `^T` work because
    // the struct's cparams are in scope on the enclosing `struct Name[T]:`.
    let mut idx = 2;
    if matches!(list.get(idx), Some(Value::Vector(_))) {
        // Consume the marker vector; no need to parse it — the struct
        // already carries the authoritative cparams. This exists for
        // readability at the use site.
        idx += 1;
    }
    // Synthesize a defn with the rest.
    let mut synth: Vec<Value> = vec![Value::Symbol("defn".into())];
    synth.extend_from_slice(&list[idx..]);
    let synth_form = Value::List(std::sync::Arc::new(synth.clone()));
    let item = lower_defn(ctx, &synth, &synth_form, &[])?;
    if let MItem::Fn(mut f) = item {
        f.is_method = true;
        f.comment = Some(pr(form));
        ctx.methods.borrow_mut().push((sname, f));
    }
    Ok(())
}

/// `(elementwise-mojo NAME [^T a ^T b ^scalar ^T k ...] ^T body)` —
/// sugar for a pure elementwise kernel. Each non-`^scalar` param is a
/// per-element pointer input; `^scalar`-tagged params are broadcast args.
/// The body is a single pure expression over per-element names.
fn lower_elementwise(ctx: &Ctx, list: &[Value], form: &Value, parallel: bool) -> Result<MItem, String> {
    if list.len() < 4 {
        return Err(format!(
            "elementwise-mojo expects (elementwise-mojo NAME [params] ^RetT body): {}",
            pr(form)
        ));
    }
    let name = sym_str(&list[1])
        .ok_or_else(|| format!("elementwise-mojo name must be a symbol: {}", pr(form)))?
        .to_string();
    let params_vec = match &list[2] {
        Value::Vector(v) => v,
        _ => {
            return Err(format!(
                "elementwise-mojo params must be a vector: {}",
                pr(form)
            ));
        }
    };
    if params_vec.is_empty() {
        return Err(format!(
            "elementwise-mojo needs at least one input param: {}",
            pr(form)
        ));
    }
    let (ret_ty, body_form) = peel_tag(&list[3]);
    if matches!(ret_ty, MType::Infer) {
        return Err(format!(
            "elementwise-mojo return type must be annotated (e.g. ^f32): {}",
            pr(form)
        ));
    }
    // Trailing forms after the ^RET body must be empty — body is one expr.
    if list.len() != 4 {
        return Err(format!(
            "elementwise-mojo body must be a single expression: {}",
            pr(form)
        ));
    }
    let mut ptr_inputs: Vec<(String, MType)> = Vec::new();
    let mut scalar_inputs: Vec<(String, MType)> = Vec::new();
    for p in params_vec.iter() {
        let (ty, is_scalar, pname) = peel_elementwise_param(p);
        let n = sym_str(pname)
            .ok_or_else(|| format!("elementwise-mojo param name must be a symbol: {}", pr(form)))?
            .to_string();
        if matches!(ty, MType::Infer) {
            return Err(format!(
                "elementwise-mojo param `{n}` must have a type annotation: {}",
                pr(form)
            ));
        }
        if !is_simd_lanewise_ty(&ty) {
            return Err(format!(
                "elementwise-mojo param `{n}` type must be a primitive numeric (f32/f64/i32/..): {}",
                pr(form)
            ));
        }
        if is_scalar {
            scalar_inputs.push((n, ty));
        } else {
            ptr_inputs.push((n, ty));
        }
    }
    if ptr_inputs.is_empty() {
        return Err(format!(
            "elementwise-mojo needs at least one non-scalar (per-element) input: {}",
            pr(form)
        ));
    }
    // Validate that every ptr_input shares the element DType (phase 1/3).
    let first_ty = ptr_inputs[0].1.clone();
    for (n, t) in ptr_inputs.iter().skip(1) {
        if t != &first_ty {
            return Err(format!(
                "elementwise-mojo: all per-element inputs must share the same dtype; \
                 `{n}` is {} but first input is {} in: {}",
                t.as_str(), first_ty.as_str(), pr(form)
            ));
        }
    }
    if ret_ty != first_ty {
        return Err(format!(
            "elementwise-mojo: return type must match per-element input dtype ({}): {}",
            first_ty.as_str(), pr(form)
        ));
    }
    // Lower the body under a purity check: only arithmetic, comparisons,
    // math.*, builtin abs/min/max, `if`, and references to param names are
    // allowed. Calls to unknown user fns are rejected (can't be vectorized
    // safely).
    let body_names: Vec<String> = ptr_inputs.iter().chain(scalar_inputs.iter())
        .map(|(n, _)| n.clone()).collect();
    check_elementwise_pure(body_form, &body_names, form)?;
    let body = lower_expr(ctx, body_form)?;
    Ok(MItem::Elementwise {
        name,
        ptr_inputs,
        scalar_inputs,
        out_ty: ret_ty,
        body,
        parallel,
        comment: Some(pr(form)),
    })
}

/// Peel tags for an elementwise-mojo param: recognizes `^scalar` as a
/// marker and a type tag. Returns (type, is_scalar, name-form).
fn peel_elementwise_param(v: &Value) -> (MType, bool, &Value) {
    let mut ty = MType::Infer;
    let mut scalar = false;
    let mut cur = v;
    loop {
        let (tag, inner) = match cur {
            Value::List(xs) if xs.len() == 3 => match (&xs[0], &xs[1]) {
                (Value::Symbol(h), Value::Symbol(t)) if &**h == "__tagged__" => {
                    (Some(t.to_string()), &xs[2])
                }
                _ => (None, cur),
            },
            _ => (None, cur),
        };
        let Some(tag) = tag else { break };
        cur = inner;
        if tag == "scalar" {
            scalar = true;
        } else if matches!(ty, MType::Infer) {
            ty = parse_type_tag(&tag);
        }
    }
    (ty, scalar, cur)
}

/// Types we currently allow in elementwise kernels (primitive numeric).
fn is_simd_lanewise_ty(t: &MType) -> bool {
    matches!(
        t,
        MType::Float32 | MType::Float64 | MType::BFloat16
            | MType::Int8 | MType::Int16 | MType::Int32 | MType::Int64
            | MType::UInt8 | MType::UInt16 | MType::UInt32 | MType::UInt64
    )
}

/// Ensure body only references declared per-element/scalar names and uses
/// whitelisted ops (arithmetic, compare, math.*, builtins, if).
fn check_elementwise_pure(body: &Value, names: &[String], form: &Value) -> Result<(), String> {
    let (_, body) = peel_tag(body);
    match body {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => Ok(()),
        Value::Symbol(s) => {
            if names.iter().any(|n| n == s.as_ref()) {
                Ok(())
            } else {
                Err(format!(
                    "elementwise-mojo body references undeclared name `{s}` in: {}",
                    pr(form)
                ))
            }
        }
        Value::List(xs) => {
            let head = xs.first().and_then(sym_str).ok_or_else(|| {
                format!("elementwise-mojo body call must start with a symbol: {}", pr(form))
            })?;
            // `if` is allowed (lowers to IfExpr).
            if head == "if" {
                if xs.len() < 3 || xs.len() > 4 {
                    return Err(format!("elementwise-mojo: if needs 2 or 3 args: {}", pr(form)));
                }
                for a in &xs[1..] {
                    check_elementwise_pure(a, names, form)?;
                }
                return Ok(());
            }
            let allowed = crate::runtime::binop(head).is_some()
                || crate::runtime::unop(head).is_some()
                || crate::runtime::math_fn(head).is_some()
                || crate::runtime::builtin_fn(head).is_some()
                || head == "and" || head == "or" || head == "not";
            if !allowed {
                return Err(format!(
                    "elementwise-mojo body: unsupported op `{head}` (only arithmetic, \
                     comparisons, math.*, min/max/abs, and `if` are allowed) in: {}",
                    pr(form)
                ));
            }
            for a in &xs[1..] {
                check_elementwise_pure(a, names, form)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "elementwise-mojo body: unsupported literal in: {}",
            pr(form)
        )),
    }
}

/// `(reduce-mojo NAME [^T a ^T b ...] ^T body init)` — sugar for a pure
/// reduction over one or more per-element pointer inputs. `init` must be
/// a numeric literal; the combining op is inferred from `body`'s head
/// (see below).
fn lower_reduce(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    if list.len() != 5 {
        return Err(format!(
            "reduce-mojo expects (reduce-mojo NAME [params] ^RetT body init): {}",
            pr(form)
        ));
    }
    let name = sym_str(&list[1])
        .ok_or_else(|| format!("reduce-mojo name must be a symbol: {}", pr(form)))?
        .to_string();
    let params_vec = match &list[2] {
        Value::Vector(v) => v,
        _ => return Err(format!("reduce-mojo params must be a vector: {}", pr(form))),
    };
    if params_vec.is_empty() {
        return Err(format!("reduce-mojo needs at least one input param: {}", pr(form)));
    }
    let (ret_ty, body_form) = peel_tag(&list[3]);
    if matches!(ret_ty, MType::Infer) {
        return Err(format!(
            "reduce-mojo return type must be annotated (e.g. ^f32): {}",
            pr(form)
        ));
    }
    let init_form = &list[4];
    let init = match init_form {
        Value::Int(i) => MExpr::IntLit(*i),
        Value::Float(f) => MExpr::FloatLit(*f),
        Value::Bool(b) => MExpr::BoolLit(*b),
        _ => return Err(format!(
            "reduce-mojo init must be a numeric literal: {}", pr(form)
        )),
    };
    let mut ptr_inputs: Vec<(String, MType)> = Vec::new();
    for p in params_vec.iter() {
        let (ty, is_scalar, pname) = peel_elementwise_param(p);
        if is_scalar {
            return Err(format!(
                "reduce-mojo does not support ^scalar params (all inputs are per-element): {}",
                pr(form)
            ));
        }
        let n = sym_str(pname)
            .ok_or_else(|| format!("reduce-mojo param name must be a symbol: {}", pr(form)))?
            .to_string();
        if matches!(ty, MType::Infer) {
            return Err(format!(
                "reduce-mojo param `{n}` must have a type annotation: {}",
                pr(form)
            ));
        }
        if !is_simd_lanewise_ty(&ty) {
            return Err(format!(
                "reduce-mojo param `{n}` type must be a primitive numeric: {}",
                pr(form)
            ));
        }
        ptr_inputs.push((n, ty));
    }
    let first_ty = ptr_inputs[0].1.clone();
    for (n, t) in ptr_inputs.iter().skip(1) {
        if t != &first_ty {
            return Err(format!(
                "reduce-mojo: per-element inputs must share dtype; `{n}` is {} but first is {}",
                t.as_str(), first_ty.as_str()
            ));
        }
    }
    if ret_ty != first_ty {
        return Err(format!(
            "reduce-mojo: return type must match element dtype ({}): {}",
            first_ty.as_str(), pr(form)
        ));
    }
    // Infer combiner from the wrapping expression. Two idioms are supported:
    //   (reduce-mojo sum-sq [^f32 x] ^f32 (* x x) 0.0)        ← implicit +
    //   (reduce-mojo dot   [^f32 a ^f32 b] ^f32 (* a b) 0.0)  ← implicit +
    // Explicit combiner sugar: a body like (+ a b), (* a b), (min a b), (max a b)
    // pins the combiner AND becomes the per-element expression. But for the
    // dot-product / sum-of-squares idiom users write the per-element math and
    // the combiner is `+` by default. We keep a simple rule: default to `+`,
    // upgrade to * / min / max when the *top-level* body is exactly that op
    // applied to same-name inputs (a rare idiom). In practice users specify
    // what they want via `init`: for `*` init=1.0, for `min`/`max` init=first.
    // Here we read an optional `^reducer` tag on the body — e.g. `^mul` body.
    // Look at the raw tag string — ReduceOp tags like `mul`/`min`/`max`
    // are lowercase so `parse_type_tag` would resolve them to `MType::Infer`
    // rather than `MType::Named`. Do a manual peel here.
    let (combiner, body_form) = peel_reducer_tag(body_form);
    let body_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
    check_elementwise_pure(body_form, &body_names, form)?;
    let body = lower_expr(ctx, body_form)?;
    Ok(MItem::Reduce {
        name,
        ptr_inputs,
        out_ty: ret_ty,
        body,
        combiner,
        init,
        comment: Some(pr(form)),
    })
}

/// Peel an optional `^sum`/`^add`/`^mul`/`^prod`/`^min`/`^max` tag from
/// the reduction body, returning the resolved combiner and the inner form.
/// Unknown tags fall through without peeling so they error later.
fn peel_reducer_tag(v: &Value) -> (ReduceOp, &Value) {
    if let Value::List(xs) = v {
        if xs.len() == 3 {
            if let (Value::Symbol(h), Value::Symbol(tag)) = (&xs[0], &xs[1]) {
                if &**h == "__tagged__" {
                    let op = match tag.as_ref() {
                        "sum" | "add" | "plus" => Some(ReduceOp::Add),
                        "mul" | "prod" | "product" | "times" => Some(ReduceOp::Mul),
                        "min" => Some(ReduceOp::Min),
                        "max" => Some(ReduceOp::Max),
                        _ => None,
                    };
                    if let Some(op) = op {
                        return (op, &xs[2]);
                    }
                }
            }
        }
    }
    (ReduceOp::Add, v)
}


/// `(elementwise-gpu-mojo NAME [^T a ^T b ...] ^T body)` — Mojo GPU kernel.
/// Same purity contract as `elementwise-mojo`; emits a fn that reads its
/// index from `block_idx.x * block_dim.x + thread_idx.x` and writes one
/// output element per thread.
fn lower_gpu_elementwise(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
    if list.len() != 4 {
        return Err(format!(
            "elementwise-gpu-mojo expects (elementwise-gpu-mojo NAME [params] ^RetT body): {}",
            pr(form)
        ));
    }
    let name = sym_str(&list[1])
        .ok_or_else(|| format!("elementwise-gpu-mojo name must be symbol: {}", pr(form)))?
        .to_string();
    let params_vec = match &list[2] {
        Value::Vector(v) => v,
        _ => return Err(format!(
            "elementwise-gpu-mojo params must be a vector: {}", pr(form)
        )),
    };
    if params_vec.is_empty() {
        return Err(format!("elementwise-gpu-mojo needs at least one input: {}", pr(form)));
    }
    let (ret_ty, body_form) = peel_tag(&list[3]);
    if matches!(ret_ty, MType::Infer) {
        return Err(format!(
            "elementwise-gpu-mojo return type must be annotated: {}", pr(form)
        ));
    }
    let mut ptr_inputs: Vec<(String, MType)> = Vec::new();
    for p in params_vec.iter() {
        let (ty, is_scalar, pname) = peel_elementwise_param(p);
        if is_scalar {
            return Err(format!(
                "elementwise-gpu-mojo does not support ^scalar params in v1: {}", pr(form)
            ));
        }
        let n = sym_str(pname)
            .ok_or_else(|| format!("elementwise-gpu-mojo param name must be symbol: {}", pr(form)))?
            .to_string();
        if !is_simd_lanewise_ty(&ty) {
            return Err(format!(
                "elementwise-gpu-mojo param `{n}` type must be a primitive numeric: {}",
                pr(form)
            ));
        }
        ptr_inputs.push((n, ty));
    }
    let first_ty = ptr_inputs[0].1.clone();
    for (n, t) in ptr_inputs.iter().skip(1) {
        if t != &first_ty {
            return Err(format!(
                "elementwise-gpu-mojo: per-element inputs must share dtype; `{n}` is {} but first is {}",
                t.as_str(), first_ty.as_str()
            ));
        }
    }
    if ret_ty != first_ty {
        return Err(format!(
            "elementwise-gpu-mojo: return type must match element dtype ({}): {}",
            first_ty.as_str(), pr(form)
        ));
    }
    let body_names: Vec<String> = ptr_inputs.iter().map(|(n, _)| n.clone()).collect();
    check_elementwise_pure(body_form, &body_names, form)?;
    let body = lower_expr(ctx, body_form)?;
    Ok(MItem::GpuElementwise {
        name,
        ptr_inputs,
        out_ty: ret_ty,
        body,
        comment: Some(pr(form)),
    })
}

/// Where does the tail expression's value go?
#[derive(Clone)]
enum TailMode {
    /// `return EXPR`
    Return,
    /// `NAME = EXPR` (used inside a while-loop block for recur fallbacks).
    #[allow(dead_code)]
    Assign(String),
}

/// `(try BODY... (catch EX [as] NAME HANDLER...) (catch ...))`.
/// `as` is a separator we accept but ignore: `(catch ValueError as e ...)`.
/// Bare `(catch ExceptionType handler...)` is also allowed (no name binding).
fn lower_try(ctx: &Ctx, list: &[Value], out: &mut Vec<MStmt>) -> Result<(), String> {
    let mut body_forms: Vec<&Value> = Vec::new();
    let mut catches: Vec<MCatch> = Vec::new();
    for f in &list[1..] {
        if let Some(l) = as_list(f) {
            if sym_str(&l[0]) == Some("catch") {
                let c = lower_catch(ctx, l, f)?;
                catches.push(c);
                continue;
            }
        }
        body_forms.push(f);
    }
    let mut body: Vec<MStmt> = Vec::new();
    for f in &body_forms {
        lower_stmt(ctx, f, &mut body)?;
    }
    out.push(MStmt::Try { body, catches });
    Ok(())
}

fn lower_catch(ctx: &Ctx, l: &[Value], form: &Value) -> Result<MCatch, String> {
    if l.len() < 2 {
        return Err(format!("catch expects (catch EXCEPTION-TYPE [as] [name] handler...): {}", pr(form)));
    }
    let ty = sym_str(&l[1])
        .ok_or_else(|| format!("catch type must be symbol: {}", pr(form)))?
        .to_string();
    let mut idx = 2;
    // Optional `as` separator.
    if let Some(s) = l.get(idx).and_then(sym_str) {
        if s == "as" {
            idx += 1;
        }
    }
    // Optional binding name (a bare symbol that isn't a list).
    let mut name: Option<String> = None;
    if let Some(v) = l.get(idx) {
        if let Value::Symbol(s) = v {
            // Heuristic: if there's a body after this, treat as name.
            if l.len() > idx + 1 {
                name = Some(s.to_string());
                idx += 1;
            }
        }
    }
    let mut body: Vec<MStmt> = Vec::new();
    for f in &l[idx..] {
        lower_stmt(ctx, f, &mut body)?;
    }
    Ok(MCatch { ty, name, body })
}

/// Lower a tail-position expression into a stmt sequence. Control flow
/// like `if`, `cond`, `do`, `let`, and `loop` may emit multiple stmts;
/// simple expressions emit a single `return` / `assign`.
fn lower_expr_tail(
    ctx: &Ctx,
    form: &Value,
    out: &mut Vec<MStmt>,
    mode: TailMode,
) -> Result<(), String> {
    // Peel any (__tagged__ T form) — type hints on the return site are
    // informational only at this point.
    let (_, form) = peel_tag(form);
    if let Some(list) = as_list(form) {
        if let Some(head) = list.first().and_then(sym_str) {
            match head {
                "do" => {
                    // All but last as stmts; last in tail.
                    for f in &list[1..list.len().saturating_sub(1)] {
                        lower_stmt(ctx, f, out)?;
                    }
                    if let Some(last) = list.last().filter(|_| list.len() > 1) {
                        return lower_expr_tail(ctx, last, out, mode);
                    }
                    // empty `do` — return 0 / pass
                    out.push(finish(mode, MExpr::IntLit(0)));
                    return Ok(());
                }
                "let" => {
                    // lower_let_tail already emits bindings + tail body;
                    // don't pre-emit bindings or they show up twice.
                    return lower_let_tail(ctx, list, out, mode);
                }
                "if" => {
                    // if cond then else (else optional)
                    if list.len() < 3 || list.len() > 4 {
                        return Err(format!("if expects 2 or 3 args: {}", pr(form)));
                    }
                    let cond = lower_expr(ctx, &list[1])?;
                    let mut then_stmts = Vec::new();
                    lower_expr_tail(ctx, &list[2], &mut then_stmts, mode.clone())?;
                    let mut else_stmts = Vec::new();
                    if list.len() == 4 {
                        lower_expr_tail(ctx, &list[3], &mut else_stmts, mode.clone())?;
                    } else {
                        else_stmts.push(finish(mode.clone(), MExpr::IntLit(0)));
                    }
                    out.push(MStmt::If { cond, then: then_stmts, els: else_stmts });
                    return Ok(());
                }
                "cond" => {
                    // (cond c1 v1 c2 v2 ... :else vN)
                    let pairs = &list[1..];
                    if pairs.len() % 2 != 0 {
                        return Err(format!("cond expects even args: {}", pr(form)));
                    }
                    return lower_cond_tail(ctx, pairs, out, mode);
                }
                "loop" => {
                    return lower_loop_tail(ctx, list, out, mode);
                }
                "for-mojo-in" => {
                    return lower_for_in_mojo_tail(ctx, list, out, mode);
                }
                "for-mojo" => {
                    return lower_for_mojo_tail(ctx, list, out, mode);
                }
                "raise" | "try" | "parameter-if" | "mojo-assert" => {
                    // Statement-form-only at tail: emit, then no return value.
                    lower_stmt(ctx, form, out)?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    let e = lower_expr(ctx, form)?;
    out.push(finish(mode, e));
    Ok(())
}

fn finish(mode: TailMode, e: MExpr) -> MStmt {
    match mode {
        TailMode::Return => MStmt::Return(e),
        TailMode::Assign(n) => MStmt::Assign { name: n, value: e },
    }
}

fn lower_let(ctx: &Ctx, list: &[Value], out: &mut Vec<MStmt>) -> Result<(), String> {
    // list = (let [bindings...] body...)
    let bindings = match list.get(1) {
        Some(Value::Vector(v)) => v,
        _ => return Err("let expects binding vector".into()),
    };
    if bindings.len() % 2 != 0 {
        return Err("let bindings must be even".into());
    }
    let mut i = 0;
    while i < bindings.len() {
        let (ty, name_form) = peel_tag(&bindings[i]);
        let name = sym_str(name_form)
            .ok_or("let binding name must be symbol")?
            .to_string();
        let value = lower_expr(ctx, &bindings[i + 1])?;
        out.push(MStmt::Let { name, ty, value });
        i += 2;
    }
    Ok(())
}

fn lower_let_tail(
    ctx: &Ctx,
    list: &[Value],
    out: &mut Vec<MStmt>,
    mode: TailMode,
) -> Result<(), String> {
    lower_let(ctx, list, out)?;
    // Body forms after the binding vec.
    let body = &list[2..];
    if body.is_empty() {
        out.push(finish(mode, MExpr::IntLit(0)));
        return Ok(());
    }
    for f in &body[..body.len() - 1] {
        lower_stmt(ctx, f, out)?;
    }
    lower_expr_tail(ctx, body.last().unwrap(), out, mode)
}

/// Lower a form in statement position (side-effect only, value discarded).
/// Recognizes `for-mojo`, `loop`, nested `do`, `if` (without value), and
/// falls back to `Expr(lower_expr(...))` for plain calls.
fn lower_stmt(ctx: &Ctx, form: &Value, out: &mut Vec<MStmt>) -> Result<(), String> {
    let (_, form) = peel_tag(form);
    if let Some(list) = as_list(form) {
        if let Some(head) = list.first().and_then(sym_str) {
            match head {
                "break" => {
                    out.push(MStmt::Break);
                    return Ok(());
                }
                "continue" => {
                    out.push(MStmt::Continue);
                    return Ok(());
                }
                "raise" => {
                    if list.len() == 1 {
                        out.push(MStmt::ReRaise);
                        return Ok(());
                    }
                    if list.len() != 2 {
                        return Err(format!("raise expects 0 or 1 arg: {}", pr(form)));
                    }
                    let e = lower_expr(ctx, &list[1])?;
                    out.push(MStmt::Raise(e));
                    return Ok(());
                }
                "try" => {
                    return lower_try(ctx, list, out);
                }
                "parameter-if" => {
                    if !*ctx.in_parametric.borrow() {
                        return Err(format!(
                            "parameter-if only legal inside parametric-fn-mojo body: {}",
                            pr(form)
                        ));
                    }
                    if list.len() < 3 || list.len() > 4 {
                        return Err(format!("parameter-if expects (test then [else]): {}", pr(form)));
                    }
                    let cond = lower_expr(ctx, &list[1])?;
                    let mut then_s = Vec::new();
                    lower_stmt(ctx, &list[2], &mut then_s)?;
                    let mut els_s = Vec::new();
                    if list.len() == 4 {
                        lower_stmt(ctx, &list[3], &mut els_s)?;
                    }
                    out.push(MStmt::ParameterIf { cond, then: then_s, els: els_s });
                    return Ok(());
                }
                "mojo-assert" => {
                    if list.len() < 2 || list.len() > 3 {
                        return Err(format!("mojo-assert expects (test [msg]): {}", pr(form)));
                    }
                    let mut args = vec![lower_expr(ctx, &list[1])?];
                    if list.len() == 3 {
                        args.push(lower_expr(ctx, &list[2])?);
                    } else {
                        args.push(MExpr::StrLit("assertion failed".into()));
                    }
                    out.push(MStmt::Expr(MExpr::Call {
                        callee: "debug_assert".into(),
                        args,
                    }));
                    return Ok(());
                }
                "for-mojo" => return lower_for_mojo_tail(ctx, list, out, TailMode::Assign("__ignore".into()))
                    .and_then(|_| {
                        // Drop the trailing assign-to-__ignore.
                        if matches!(out.last(), Some(MStmt::Assign { name, .. }) if name == "__ignore") {
                            out.pop();
                        }
                        Ok(())
                    }),
                "for-mojo-in" => return lower_for_in_mojo_tail(ctx, list, out, TailMode::Assign("__ignore".into()))
                    .and_then(|_| {
                        if matches!(out.last(), Some(MStmt::Assign { name, .. }) if name == "__ignore") {
                            out.pop();
                        }
                        Ok(())
                    }),
                "do" => {
                    for f in &list[1..] {
                        lower_stmt(ctx, f, out)?;
                    }
                    return Ok(());
                }
                "if" => {
                    if list.len() >= 3 && list.len() <= 4 {
                        let cond = lower_expr(ctx, &list[1])?;
                        let mut t = Vec::new();
                        lower_stmt(ctx, &list[2], &mut t)?;
                        let mut e = Vec::new();
                        if list.len() == 4 {
                            lower_stmt(ctx, &list[3], &mut e)?;
                        }
                        out.push(MStmt::If { cond, then: t, els: e });
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
    let e = lower_expr(ctx, form)?;
    out.push(MStmt::Expr(e));
    Ok(())
}

fn lower_cond_tail(
    ctx: &Ctx,
    pairs: &[Value],
    out: &mut Vec<MStmt>,
    mode: TailMode,
) -> Result<(), String> {
    // Unwind right-to-left as nested if/else.
    if pairs.is_empty() {
        out.push(finish(mode, MExpr::IntLit(0)));
        return Ok(());
    }
    let test = &pairs[0];
    let branch = &pairs[1];
    // :else → unconditional
    let is_else = matches!(test, Value::Keyword(k) if &**k == "else")
        || matches!(test, Value::Bool(true));
    if is_else {
        return lower_expr_tail(ctx, branch, out, mode);
    }
    let cond = lower_expr(ctx, test)?;
    let mut then_stmts = Vec::new();
    lower_expr_tail(ctx, branch, &mut then_stmts, mode.clone())?;
    let mut else_stmts = Vec::new();
    lower_cond_tail(ctx, &pairs[2..], &mut else_stmts, mode)?;
    out.push(MStmt::If { cond, then: then_stmts, els: else_stmts });
    Ok(())
}

/// (loop [x init y init] body...) with (recur x' y') inside.
/// Lowered to:
///   var x = init; var y = init
///   var __done = False
///   var __ret: T = 0
///   while not __done: body'
/// where body' replaces recur with temp-swap + continue, and any non-recur
/// tail expr becomes `__ret = EXPR; __done = True; break`.
fn lower_loop_tail(
    ctx: &Ctx,
    list: &[Value],
    out: &mut Vec<MStmt>,
    mode: TailMode,
) -> Result<(), String> {
    let bindings = match list.get(1) {
        Some(Value::Vector(v)) => v,
        _ => return Err("loop expects binding vector".into()),
    };
    if bindings.len() % 2 != 0 {
        return Err("loop bindings must be even".into());
    }
    // Try the for-range fast path: a single counter that walks lo..hi by +1.
    let bindings_vec: Vec<Value> = bindings.iter().cloned().collect();
    if let Some(()) = try_lower_for_range(ctx, list, &bindings_vec, out, &mode)? {
        return Ok(());
    }
    let mut names: Vec<String> = Vec::new();
    let mut tys: Vec<MType> = Vec::new();
    let mut i = 0;
    while i < bindings.len() {
        let (ty, nf) = peel_tag(&bindings[i]);
        let name = sym_str(nf).ok_or("loop name must be symbol")?.to_string();
        let v = lower_expr(ctx, &bindings[i + 1])?;
        out.push(MStmt::Let { name: name.clone(), ty: ty.clone(), value: v });
        names.push(name);
        tys.push(ty);
        i += 2;
    }
    let done_name = ctx.gensym("done");
    let ret_name = ctx.gensym("ret");
    out.push(MStmt::Let {
        name: done_name.clone(),
        ty: MType::Bool,
        value: MExpr::BoolLit(false),
    });
    out.push(MStmt::Let {
        name: ret_name.clone(),
        ty: MType::Infer,
        value: MExpr::IntLit(0),
    });
    // Body — everything after the binding vec. Wrap in an implicit do.
    let body_forms = &list[2..];
    let body_expr = if body_forms.len() == 1 {
        body_forms[0].clone()
    } else {
        let mut v = vec![Value::Symbol("do".into())];
        v.extend(body_forms.iter().cloned());
        Value::List(std::sync::Arc::new(v))
    };
    let mut loop_body = Vec::new();
    lower_loop_body(ctx, &body_expr, &mut loop_body, &names, &done_name, &ret_name)?;
    out.push(MStmt::While {
        cond: MExpr::UnOp { op: "not".into(), rhs: Box::new(MExpr::Var(done_name)) },
        body: loop_body,
    });
    out.push(finish(mode, MExpr::Var(ret_name)));
    Ok(())
}

/// `(for-mojo [i lo hi] body...)` — sugar for the most common counting
/// loop. Lowers to `for i in range(lo, hi): body`. Body forms become Expr
/// stmts inside the loop; the for-form's own tail value is `0`.
fn lower_for_mojo_tail(
    ctx: &Ctx,
    list: &[Value],
    out: &mut Vec<MStmt>,
    mode: TailMode,
) -> Result<(), String> {
    let bindings = match list.get(1) {
        Some(Value::Vector(v)) => v,
        _ => return Err(format!("for-mojo expects [i lo hi] binding vec: {}", pr_list(list))),
    };
    if bindings.len() != 3 {
        return Err(format!("for-mojo binding vec must have 3 elements [i lo hi]: {}", pr_list(list)));
    }
    let (cty, name_form) = peel_tag(&bindings[0]);
    let cname = sym_str(name_form)
        .ok_or_else(|| format!("for-mojo counter must be symbol: {}", pr_list(list)))?
        .to_string();
    let lo = lower_expr(ctx, &bindings[1])?;
    let hi = lower_expr(ctx, &bindings[2])?;
    let mut body = Vec::new();
    for f in &list[2..] {
        let e = lower_expr(ctx, f)?;
        body.push(MStmt::Expr(e));
    }
    out.push(MStmt::ForRange { name: cname, ty: cty, lo, hi, body });
    out.push(finish(mode, MExpr::IntLit(0)));
    Ok(())
}

/// `(for-mojo-in [x xs] body...)` — iterator-protocol loop. Lowers to
/// `for x in xs: body`. Body forms become Expr stmts.
fn lower_for_in_mojo_tail(
    ctx: &Ctx,
    list: &[Value],
    out: &mut Vec<MStmt>,
    mode: TailMode,
) -> Result<(), String> {
    let bindings = match list.get(1) {
        Some(Value::Vector(v)) => v,
        _ => return Err(format!("for-mojo-in expects [x iter] binding vec: {}", pr_list(list))),
    };
    if bindings.len() != 2 {
        return Err(format!(
            "for-mojo-in binding vec must have 2 elements [x iter]: {}",
            pr_list(list)
        ));
    }
    let (cty, name_form) = peel_tag(&bindings[0]);
    let cname = sym_str(name_form)
        .ok_or_else(|| format!("for-mojo-in binding name must be a symbol: {}", pr_list(list)))?
        .to_string();
    let iter = lower_expr(ctx, &bindings[1])?;
    let mut body = Vec::new();
    for f in &list[2..] {
        let e = lower_expr(ctx, f)?;
        body.push(MStmt::Expr(e));
    }
    out.push(MStmt::ForIn { name: cname, ty: cty, iter, body });
    out.push(finish(mode, MExpr::IntLit(0)));
    Ok(())
}

/// Detect `(loop [^T i lo] (if (< i hi) (do BODY (recur (+ i 1))) TERM))`
/// shapes (TERM optional / nil) and emit `for i in range(lo, hi): BODY`.
/// Returns Some(()) if it took the fast path, None otherwise.
fn try_lower_for_range(
    ctx: &Ctx,
    list: &[Value],
    bindings: &[Value],
    out: &mut Vec<MStmt>,
    mode: &TailMode,
) -> Result<Option<()>, String> {
    if bindings.len() != 2 {
        return Ok(None);
    }
    let (cty, name_form) = peel_tag(&bindings[0]);
    let cname = match sym_str(name_form) {
        Some(s) => s.to_string(),
        None => return Ok(None),
    };
    // Body must be a single form for the fast path.
    if list.len() != 3 {
        return Ok(None);
    }
    let body_forms = &list[2..];
    let body = &body_forms[0];
    // Body shape: (if (< i HI) THEN ELSE?)
    let if_list = match as_list(&peel_tag(body).1) {
        Some(l) => l.to_vec(),
        None => return Ok(None),
    };
    if if_list.len() < 3 || if_list.len() > 4 {
        return Ok(None);
    }
    if sym_str(&if_list[0]) != Some("if") {
        return Ok(None);
    }
    // Cond must be (< i HI) or (<= i HI).
    let cond = match as_list(&peel_tag(&if_list[1]).1) {
        Some(l) => l.to_vec(),
        None => return Ok(None),
    };
    if cond.len() != 3 {
        return Ok(None);
    }
    let cmp = match sym_str(&cond[0]) {
        Some(s) => s,
        None => return Ok(None),
    };
    if cmp != "<" && cmp != "<=" {
        return Ok(None);
    }
    if sym_str(&peel_tag(&cond[1]).1) != Some(cname.as_str()) {
        return Ok(None);
    }
    let hi_form = cond[2].clone();
    // The loop-body branch must end with (recur (+ i 1)) (or (recur (inc i))).
    let then_form = if_list[2].clone();
    let (loop_body_forms, recur_args) = collect_loop_body_then_recur(&then_form)?;
    let recur_args = match recur_args {
        Some(a) => a,
        None => return Ok(None),
    };
    if recur_args.len() != 1 {
        return Ok(None);
    }
    if !is_increment_of(&recur_args[0], &cname) {
        return Ok(None);
    }
    // We have a counting loop. The else branch is the loop's terminal value
    // — supported when it's nil / 0 / a literal we can stash post-loop.
    let term_expr = if if_list.len() == 4 {
        Some(if_list[3].clone())
    } else {
        None
    };

    // Emit:
    //   for i in range(lo, hi): body
    //   <terminal stmt for tail mode>
    let lo = lower_expr(ctx, &bindings[1])?;
    // For `<=` we need range(lo, hi+1).
    let hi = if cmp == "<=" {
        MExpr::BinOp {
            op: "+".into(),
            lhs: Box::new(lower_expr(ctx, &hi_form)?),
            rhs: Box::new(MExpr::IntLit(1)),
        }
    } else {
        lower_expr(ctx, &hi_form)?
    };
    let mut for_body = Vec::new();
    for f in &loop_body_forms {
        let e = lower_expr(ctx, f)?;
        for_body.push(MStmt::Expr(e));
    }
    out.push(MStmt::ForRange {
        name: cname,
        ty: cty,
        lo,
        hi,
        body: for_body,
    });
    // Terminal value after the loop.
    let term = match term_expr {
        Some(t) => lower_expr(ctx, &t)?,
        None => MExpr::IntLit(0),
    };
    out.push(finish(mode.clone(), term));
    Ok(Some(()))
}

/// Walk a (do ...) form (or single form) and split off the trailing
/// (recur ...) call. Returns (preceding-body-forms, Some(recur-args)) if
/// found, else (forms, None).
fn collect_loop_body_then_recur(form: &Value) -> Result<(Vec<Value>, Option<Vec<Value>>), String> {
    let (_, form) = peel_tag(form);
    // bare (recur ...)
    if let Some(l) = as_list(form) {
        if sym_str(&l[0]) == Some("recur") {
            return Ok((Vec::new(), Some(l[1..].to_vec())));
        }
        if sym_str(&l[0]) == Some("do") {
            let parts = &l[1..];
            if parts.is_empty() {
                return Ok((Vec::new(), None));
            }
            let last = parts.last().unwrap();
            if let Some(ll) = as_list(&peel_tag(last).1) {
                if sym_str(&ll[0]) == Some("recur") {
                    return Ok((parts[..parts.len() - 1].to_vec(), Some(ll[1..].to_vec())));
                }
            }
            return Ok((parts.to_vec(), None));
        }
    }
    Ok((vec![form.clone()], None))
}

/// True if `form` is `(+ i 1)`, `(+ 1 i)`, or `(inc i)` for the named counter.
fn is_increment_of(form: &Value, counter: &str) -> bool {
    let (_, form) = peel_tag(form);
    let l = match as_list(form) {
        Some(l) => l,
        None => return false,
    };
    let head = match sym_str(&l[0]) {
        Some(s) => s,
        None => return false,
    };
    if head == "inc" && l.len() == 2 {
        return sym_str(&peel_tag(&l[1]).1) == Some(counter);
    }
    if head == "+" && l.len() == 3 {
        let a = &peel_tag(&l[1]).1;
        let b = &peel_tag(&l[2]).1;
        let one_a = matches!(a, Value::Int(1));
        let one_b = matches!(b, Value::Int(1));
        let i_a = sym_str(a) == Some(counter);
        let i_b = sym_str(b) == Some(counter);
        return (i_a && one_b) || (i_b && one_a);
    }
    false
}

fn lower_loop_body(
    ctx: &Ctx,
    form: &Value,
    out: &mut Vec<MStmt>,
    loop_names: &[String],
    done: &str,
    ret: &str,
) -> Result<(), String> {
    let (_, form) = peel_tag(form);
    if let Some(list) = as_list(form) {
        if let Some(head) = list.first().and_then(sym_str) {
            match head {
                "recur" => {
                    let args = &list[1..];
                    if args.len() != loop_names.len() {
                        return Err(format!(
                            "recur arity {} != loop bindings {}",
                            args.len(),
                            loop_names.len()
                        ));
                    }
                    // Compute all new values into temps first (to avoid
                    // clobbering x before y reads it).
                    let mut temps = Vec::new();
                    for (idx, a) in args.iter().enumerate() {
                        let tmp = format!("__rec{idx}");
                        let v = lower_expr(ctx, a)?;
                        out.push(MStmt::Let {
                            name: tmp.clone(),
                            ty: MType::Infer,
                            value: v,
                        });
                        temps.push(tmp);
                    }
                    for (name, tmp) in loop_names.iter().zip(temps.iter()) {
                        out.push(MStmt::Assign {
                            name: name.clone(),
                            value: MExpr::Var(tmp.clone()),
                        });
                    }
                    return Ok(());
                }
                "do" => {
                    for f in &list[1..list.len().saturating_sub(1)] {
                        let e = lower_expr(ctx, f)?;
                        out.push(MStmt::Expr(e));
                    }
                    if let Some(last) = list.last().filter(|_| list.len() > 1) {
                        return lower_loop_body(ctx, last, out, loop_names, done, ret);
                    }
                    return Ok(());
                }
                "let" => {
                    lower_let(ctx, list, out)?;
                    let body = &list[2..];
                    if body.is_empty() {
                        return Ok(());
                    }
                    for f in &body[..body.len() - 1] {
                        let e = lower_expr(ctx, f)?;
                        out.push(MStmt::Expr(e));
                    }
                    return lower_loop_body(
                        ctx,
                        body.last().unwrap(),
                        out,
                        loop_names,
                        done,
                        ret,
                    );
                }
                "if" => {
                    if list.len() < 3 || list.len() > 4 {
                        return Err(format!("if expects 2 or 3 args: {}", pr(form)));
                    }
                    let cond = lower_expr(ctx, &list[1])?;
                    let mut ts = Vec::new();
                    lower_loop_body(ctx, &list[2], &mut ts, loop_names, done, ret)?;
                    let mut es = Vec::new();
                    if list.len() == 4 {
                        lower_loop_body(ctx, &list[3], &mut es, loop_names, done, ret)?;
                    }
                    out.push(MStmt::If { cond, then: ts, els: es });
                    return Ok(());
                }
                "cond" => {
                    let pairs = &list[1..];
                    if pairs.len() % 2 != 0 {
                        return Err("cond expects even args".into());
                    }
                    return lower_cond_loop(ctx, pairs, out, loop_names, done, ret);
                }
                _ => {}
            }
        }
    }
    // Plain tail value: set ret, mark done, break.
    let e = lower_expr(ctx, form)?;
    out.push(MStmt::Assign { name: ret.into(), value: e });
    out.push(MStmt::Assign {
        name: done.into(),
        value: MExpr::BoolLit(true),
    });
    out.push(MStmt::Break);
    Ok(())
}

fn lower_cond_loop(
    ctx: &Ctx,
    pairs: &[Value],
    out: &mut Vec<MStmt>,
    loop_names: &[String],
    done: &str,
    ret: &str,
) -> Result<(), String> {
    if pairs.is_empty() {
        return Ok(());
    }
    let test = &pairs[0];
    let branch = &pairs[1];
    let is_else = matches!(test, Value::Keyword(k) if &**k == "else")
        || matches!(test, Value::Bool(true));
    if is_else {
        return lower_loop_body(ctx, branch, out, loop_names, done, ret);
    }
    let cond = lower_expr(ctx, test)?;
    let mut ts = Vec::new();
    lower_loop_body(ctx, branch, &mut ts, loop_names, done, ret)?;
    let mut es = Vec::new();
    lower_cond_loop(ctx, &pairs[2..], &mut es, loop_names, done, ret)?;
    out.push(MStmt::If { cond, then: ts, els: es });
    Ok(())
}

/// Lower an expression (non-tail). No stmt emission; must be pure MExpr.
pub fn lower_expr(ctx: &Ctx, form: &Value) -> Result<MExpr, String> {
    let (_, form) = peel_tag(form);
    match form {
        Value::Nil => Ok(MExpr::IntLit(0)),
        Value::Bool(b) => Ok(MExpr::BoolLit(*b)),
        Value::Int(i) => Ok(MExpr::IntLit(*i)),
        Value::Float(f) => Ok(MExpr::FloatLit(*f)),
        Value::Symbol(s) => Ok(MExpr::Var(s.to_string())),
        Value::List(v) => lower_call(ctx, v),
        Value::Vector(_) | Value::Map(_) | Value::Set(_) => Err(format!(
            "collection literals not supported in cljrs-mojo v1: {}",
            pr(form)
        )),
        Value::Str(s) => Ok(MExpr::StrLit(s.to_string())),
        _ => Err(format!("unsupported expr: {}", pr(form))),
    }
}

fn lower_call(ctx: &Ctx, v: &[Value]) -> Result<MExpr, String> {
    if v.is_empty() {
        return Err("empty call".into());
    }
    let head = sym_str(&v[0]).ok_or_else(|| {
        format!("higher-order call head not supported: {}", pr(&v[0]))
    })?;
    let args = &v[1..];

    // if as expression
    if head == "if" {
        if args.len() < 2 || args.len() > 3 {
            return Err("if expects 2 or 3 args".into());
        }
        let c = lower_expr(ctx, &args[0])?;
        let t = lower_expr(ctx, &args[1])?;
        let e = if args.len() == 3 {
            lower_expr(ctx, &args[2])?
        } else {
            MExpr::IntLit(0)
        };
        return Ok(MExpr::IfExpr {
            cond: Box::new(c),
            then: Box::new(t),
            els: Box::new(e),
        });
    }
    if head == "do" {
        // value-position do: all but last are discarded (we don't have
        // a statement-expression in MExpr; in practice users won't put
        // side-effecting stuff here in numeric kernels).
        if args.is_empty() {
            return Ok(MExpr::IntLit(0));
        }
        return lower_expr(ctx, args.last().unwrap());
    }
    // Field access: (. obj field) → obj.field
    if head == "." {
        if args.len() != 2 {
            return Err(format!("`.` expects 2 args (object, field): {}", pr_list(v)));
        }
        let obj = lower_expr(ctx, &args[0])?;
        let field = sym_str(&args[1])
            .ok_or_else(|| format!("`.` field name must be a symbol: {}", pr_list(v)))?
            .to_string();
        return Ok(MExpr::Field { obj: Box::new(obj), field });
    }
    if matches!(head, "let" | "loop" | "cond" | "recur" | "for-mojo" | "for-mojo-in"
        | "try" | "raise" | "parameter-if" | "mojo-assert") {
        return Err(format!(
            "`{head}` only supported in tail position in cljrs-mojo v1: {}",
            pr_list(v)
        ));
    }

    // ---- collection + option helpers ----
    if head == "list" {
        // (list [1 2 3]) or (list e1 e2 ...) → `List[T](e1, e2, ...)`.
        let owned_elems: Vec<Value>;
        let elems: &[Value] = if args.len() == 1 {
            if let Value::Vector(vv) = &args[0] {
                owned_elems = vv.iter().cloned().collect();
                &owned_elems
            } else {
                args
            }
        } else {
            args
        };
        let lowered: Result<Vec<_>, _> = elems.iter().map(|a| lower_expr(ctx, a)).collect();
        let lowered = lowered?;
        // Infer element type from first lit (Int → Int, Float → Float64, ...).
        let ty_str = infer_list_ty(&lowered);
        return Ok(MExpr::Call { callee: format!("List[{ty_str}]"), args: lowered });
    }
    if head == "nth" {
        if args.len() != 2 {
            return Err("nth expects 2 args".into());
        }
        let lst = lower_expr(ctx, &args[0])?;
        let i = lower_expr(ctx, &args[1])?;
        // Emit as a fake Call whose callee is "<expr>[". Use a special marker:
        // just emit `lst[i]` via a Call convention. Use BinOp-like hack.
        return Ok(MExpr::Call {
            callee: "__index__".into(),
            args: vec![lst, i],
        });
    }
    if head == "len" {
        if args.len() != 1 {
            return Err("len expects 1 arg".into());
        }
        let a = lower_expr(ctx, &args[0])?;
        return Ok(MExpr::Call { callee: "len".into(), args: vec![a] });
    }
    if head == "some" {
        if args.len() != 1 {
            return Err("some expects 1 arg".into());
        }
        let a = lower_expr(ctx, &args[0])?;
        return Ok(MExpr::Call { callee: "Optional".into(), args: vec![a] });
    }
    if head == "none" {
        return Ok(MExpr::Var("None".into()));
    }
    if head == "unwrap" {
        if args.len() != 1 {
            return Err("unwrap expects 1 arg".into());
        }
        let a = lower_expr(ctx, &args[0])?;
        return Ok(MExpr::Call {
            callee: "__method__value".into(),
            args: vec![a],
        });
    }
    // ---- string helpers ----
    if head == "str-len" {
        if args.len() != 1 {
            return Err("str-len expects 1 arg".into());
        }
        let a = lower_expr(ctx, &args[0])?;
        return Ok(MExpr::Call { callee: "len".into(), args: vec![a] });
    }
    if head == "str-slice" {
        if args.len() != 3 {
            return Err("str-slice expects 3 args (s, a, b)".into());
        }
        let s = lower_expr(ctx, &args[0])?;
        let a = lower_expr(ctx, &args[1])?;
        let b = lower_expr(ctx, &args[2])?;
        return Ok(MExpr::Call {
            callee: "__slice__".into(),
            args: vec![s, a, b],
        });
    }
    if head == "str-split" {
        if args.len() != 2 {
            return Err("str-split expects 2 args (s, sep)".into());
        }
        let s = lower_expr(ctx, &args[0])?;
        let sep = lower_expr(ctx, &args[1])?;
        return Ok(MExpr::Call {
            callee: "__method__split".into(),
            args: vec![s, sep],
        });
    }
    if head == "isinstance-mojo" {
        if args.len() != 2 {
            return Err("isinstance-mojo expects 2 args (value, Type)".into());
        }
        let v = lower_expr(ctx, &args[0])?;
        let ty = sym_str(&args[1])
            .ok_or("isinstance-mojo: second arg must be a type symbol")?
            .to_string();
        return Ok(MExpr::Call {
            callee: "isinstance".into(),
            args: vec![v, MExpr::Var(ty)],
        });
    }
    // print / println — Mojo's `print` builtin.
    if head == "print" || head == "println" {
        let lowered: Result<Vec<_>, _> = args.iter().map(|a| lower_expr(ctx, a)).collect();
        return Ok(MExpr::Call { callee: "print".into(), args: lowered? });
    }
    // (format "x={} y={}" x y) → "x=" + String(x) + " y=" + String(y) using
    // Mojo's `String` constructor for non-string args. Returns a String.
    if head == "format" {
        if args.is_empty() {
            return Err("format expects a template string".into());
        }
        let template = match &args[0] {
            Value::Str(s) => s.to_string(),
            _ => return Err(format!("format template must be a literal string: {}", pr(&args[0]))),
        };
        let rest: Vec<_> = args[1..].iter().collect();
        return build_format(ctx, &template, &rest);
    }

    // Boolean and/or/not
    if head == "and" || head == "or" {
        return fold_binop(ctx, runtime::binop(head).unwrap(), args, default_for(head));
    }
    if head == "not" {
        if args.len() != 1 {
            return Err("not expects 1 arg".into());
        }
        let a = lower_expr(ctx, &args[0])?;
        return Ok(MExpr::UnOp { op: "not".into(), rhs: Box::new(a) });
    }

    // Arithmetic / comparison infix
    if let Some(op) = runtime::binop(head) {
        if head == "-" && args.len() == 1 {
            let a = lower_expr(ctx, &args[0])?;
            return Ok(MExpr::UnOp { op: "-".into(), rhs: Box::new(a) });
        }
        if head == "/" && args.len() == 1 {
            // (/ x) = 1/x
            let a = lower_expr(ctx, &args[0])?;
            return Ok(MExpr::BinOp {
                op: "/".into(),
                lhs: Box::new(MExpr::FloatLit(1.0)),
                rhs: Box::new(a),
            });
        }
        // For comparisons with >2 args we'd need chained ANDs. Restrict to 2.
        if matches!(head, "<" | ">" | "<=" | ">=" | "=" | "not=") {
            if args.len() != 2 {
                return Err(format!(
                    "comparison `{head}` expects 2 args in cljrs-mojo v1"
                ));
            }
            let l = lower_expr(ctx, &args[0])?;
            let r = lower_expr(ctx, &args[1])?;
            return Ok(MExpr::BinOp { op: op.into(), lhs: Box::new(l), rhs: Box::new(r) });
        }
        return fold_binop(ctx, op, args, default_for(head));
    }

    // math.*
    if let Some((mname, import)) = runtime::math_fn(head) {
        ctx.need_import(import);
        let lowered: Result<Vec<_>, _> = args.iter().map(|a| lower_expr(ctx, a)).collect();
        return Ok(MExpr::Call { callee: mname.into(), args: lowered? });
    }
    // abs/min/max
    if let Some(bname) = runtime::builtin_fn(head) {
        let lowered: Result<Vec<_>, _> = args.iter().map(|a| lower_expr(ctx, a)).collect();
        return Ok(MExpr::Call { callee: bname.into(), args: lowered? });
    }

    // Generic struct construction: `(Vec3 ^f32 1.0 2.0 3.0)` → `Vec3[Float32](1.0, 2.0, 3.0)`.
    // Trigger only for CamelCase callees with a type-tagged first arg.
    if head.starts_with(|c: char| c.is_ascii_uppercase()) && !args.is_empty() {
        let (first_ty, _) = peel_tag(&args[0]);
        if !matches!(first_ty, MType::Infer) {
            let callee = format!("{head}[{}]", first_ty.as_str());
            let lowered: Result<Vec<_>, _> = args.iter().map(|a| lower_expr(ctx, a)).collect();
            return Ok(MExpr::Call { callee, args: lowered? });
        }
    }

    // Fallback: assume the symbol names a defined fn.
    let lowered: Result<Vec<_>, _> = args.iter().map(|a| lower_expr(ctx, a)).collect();
    Ok(MExpr::Call { callee: head.to_string(), args: lowered? })
}

/// Best-effort element-type inference for a `List(...)` constructor. Looks
/// at the first element's literal shape; falls back to `Int` when empty
/// or unable to decide.
fn infer_list_ty(elems: &[MExpr]) -> &'static str {
    if elems.is_empty() {
        return "Int";
    }
    match &elems[0] {
        MExpr::FloatLit(_) => "Float64",
        MExpr::IntLit(_) => "Int",
        MExpr::BoolLit(_) => "Bool",
        MExpr::StrLit(_) => "String",
        _ => "Int",
    }
}

/// Build the concat expression for `(format "a={} b={}" x y)`. Splits on
/// `{}` and interleaves with `String(arg)` calls (or the raw expr when it's
/// already a string literal).
fn build_format(ctx: &Ctx, template: &str, args: &[&Value]) -> Result<MExpr, String> {
    let parts: Vec<&str> = template.split("{}").collect();
    let placeholders = parts.len().saturating_sub(1);
    if placeholders != args.len() {
        return Err(format!(
            "format placeholders ({}) ≠ args ({}) for template {:?}",
            placeholders, args.len(), template
        ));
    }
    // Build left-folded string concat: "p0" + String(a0) + "p1" + String(a1) + ...
    let mut acc: Option<MExpr> = None;
    for (i, lit) in parts.iter().enumerate() {
        if !lit.is_empty() {
            let piece = MExpr::StrLit((*lit).to_string());
            acc = Some(match acc {
                None => piece,
                Some(prev) => MExpr::BinOp {
                    op: "+".into(), lhs: Box::new(prev), rhs: Box::new(piece),
                },
            });
        }
        if i < args.len() {
            let arg_expr = lower_expr(ctx, args[i])?;
            // Wrap non-string args in String(arg).
            let coerced = match &arg_expr {
                MExpr::StrLit(_) => arg_expr,
                _ => MExpr::Call { callee: "String".into(), args: vec![arg_expr] },
            };
            acc = Some(match acc {
                None => coerced,
                Some(prev) => MExpr::BinOp {
                    op: "+".into(), lhs: Box::new(prev), rhs: Box::new(coerced),
                },
            });
        }
    }
    Ok(acc.unwrap_or_else(|| MExpr::StrLit(String::new())))
}

fn default_for(head: &str) -> MExpr {
    match head {
        "+" | "-" => MExpr::IntLit(0),
        "*" | "/" => MExpr::IntLit(1),
        "and" => MExpr::BoolLit(true),
        "or" => MExpr::BoolLit(false),
        _ => MExpr::IntLit(0),
    }
}

fn fold_binop(
    ctx: &Ctx,
    op: &str,
    args: &[Value],
    identity: MExpr,
) -> Result<MExpr, String> {
    if args.is_empty() {
        return Ok(identity);
    }
    let mut it = args.iter();
    let mut acc = lower_expr(ctx, it.next().unwrap())?;
    for a in it {
        let r = lower_expr(ctx, a)?;
        acc = MExpr::BinOp { op: op.into(), lhs: Box::new(acc), rhs: Box::new(r) };
    }
    Ok(acc)
}

// ---------------- helpers ----------------

pub fn as_list(v: &Value) -> Option<&[Value]> {
    match v {
        Value::List(xs) => Some(xs.as_slice()),
        _ => None,
    }
}

pub fn sym_str(v: &Value) -> Option<&str> {
    match v {
        Value::Symbol(s) => Some(s.as_ref()),
        _ => None,
    }
}

/// Peel parameter tags including both type and convention. Recognized
/// convention tags (`owned`, `borrowed`, `inout`, `ref`) bind to the
/// argument convention slot rather than the type. Multiple `^tag` layers
/// stack on the same form.
pub fn peel_param_tags(v: &Value) -> (MType, ParamConv, &Value) {
    let mut ty = MType::Infer;
    let mut conv = ParamConv::Default;
    let mut cur = v;
    loop {
        // peel one tag layer manually
        let (tag_str, inner) = match cur {
            Value::List(xs) if xs.len() == 3 => match (&xs[0], &xs[1]) {
                (Value::Symbol(h), Value::Symbol(tag)) if &**h == "__tagged__" => {
                    (Some(tag.to_string()), &xs[2])
                }
                _ => (None, cur),
            },
            _ => (None, cur),
        };
        let Some(tag) = tag_str else { break };
        cur = inner;
        match tag.as_str() {
            "owned" => conv = ParamConv::Owned,
            "borrowed" => conv = ParamConv::Borrowed,
            "inout" => conv = ParamConv::Inout,
            "ref" => conv = ParamConv::Ref,
            other => {
                // Treat as a type tag.
                if matches!(ty, MType::Infer) {
                    ty = parse_type_tag(other);
                }
            }
        }
    }
    (ty, conv, cur)
}

/// Variant of `peel_param_tags` that also extracts `{:default EXPR}` from
/// a map-meta tag into an MExpr. Accepts type/convention symbol tags or
/// one map-meta tag (or any stack of them). Returns
/// `(ty, conv, default_expr, inner)`.
pub fn peel_param_tags_full<'a>(
    ctx: &Ctx,
    v: &'a Value,
) -> Result<(MType, ParamConv, Option<MExpr>, &'a Value), String> {
    let mut ty = MType::Infer;
    let mut conv = ParamConv::Default;
    let mut default: Option<MExpr> = None;
    let mut cur = v;
    loop {
        // `(__tagged__ <tag> <form>)` where `<tag>` is a Symbol (type/conv)
        // or a Map (`{:default EXPR}` / `{:doc "..."}`).
        let Some((tag_v, inner)) = peel_one_tag(cur) else {
            break;
        };
        cur = inner;
        match tag_v {
            Value::Symbol(tag) => match tag.as_ref() {
                "owned" => conv = ParamConv::Owned,
                "borrowed" => conv = ParamConv::Borrowed,
                "inout" => conv = ParamConv::Inout,
                "ref" => conv = ParamConv::Ref,
                other => {
                    if matches!(ty, MType::Infer) {
                        ty = parse_type_tag(other);
                    }
                }
            },
            Value::Map(m) => {
                for (k, val) in m.iter() {
                    if let Value::Keyword(kw) = k {
                        if kw.as_ref() == "default" {
                            default = Some(lower_expr(ctx, val)?);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok((ty, conv, default, cur))
}

/// Peel exactly one `(__tagged__ tag form)` layer. Returns the tag value
/// (Symbol or Map) and the inner form. `None` means `v` isn't a tagged list.
fn peel_one_tag(v: &Value) -> Option<(&Value, &Value)> {
    match v {
        Value::List(xs) if xs.len() == 3 => match &xs[0] {
            Value::Symbol(h) if h.as_ref() == "__tagged__" => Some((&xs[1], &xs[2])),
            _ => None,
        },
        _ => None,
    }
}

/// Peel a `^{:doc "..."}` map-meta (and any other map meta like
/// `^{:decorators [...]}`) off a symbol name. Returns the extracted
/// docstring (if any) and the inner bare symbol. Non-map meta is passed
/// through unchanged.
pub fn peel_doc_meta(v: &Value) -> (Option<String>, &Value) {
    let mut cur = v;
    let mut doc: Option<String> = None;
    loop {
        let Some((tag, inner)) = peel_one_tag(cur) else { break };
        match tag {
            Value::Map(m) => {
                for (k, val) in m.iter() {
                    if let Value::Keyword(kw) = k {
                        if kw.as_ref() == "doc" {
                            if let Value::Str(s) = val {
                                doc = Some(s.to_string());
                            }
                        }
                    }
                }
                cur = inner;
            }
            _ => break,
        }
    }
    (doc, cur)
}

/// Extract `^{:decorators [:a :b]}` and `^{:doc "..."}` metadata off a
/// fn-name form. Returns (decorators, doc, inner_form). Decorators are
/// returned as Mojo-style `@keyword` strings.
pub fn peel_name_meta(v: &Value) -> (Vec<String>, Option<String>, &Value) {
    let mut cur = v;
    let mut decorators: Vec<String> = Vec::new();
    let mut doc: Option<String> = None;
    loop {
        let Some((tag, inner)) = peel_one_tag(cur) else { break };
        match tag {
            Value::Map(m) => {
                for (k, val) in m.iter() {
                    if let Value::Keyword(kw) = k {
                        match kw.as_ref() {
                            "doc" => {
                                if let Value::Str(s) = val {
                                    doc = Some(s.to_string());
                                }
                            }
                            "decorators" => {
                                if let Value::Vector(vs) = val {
                                    for d in vs.iter() {
                                        if let Value::Keyword(k) = d {
                                            decorators.push(decorator_to_mojo(k.as_ref()));
                                        } else if let Value::Symbol(s) = d {
                                            decorators.push(decorator_to_mojo(s.as_ref()));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                cur = inner;
            }
            _ => break,
        }
    }
    (decorators, doc, cur)
}

fn decorator_to_mojo(s: &str) -> String {
    // Map kebab-case keywords to the idiomatic Mojo decorator strings.
    match s {
        "always-inline" | "always_inline" => "@always_inline".into(),
        "parameter" => "@parameter".into(),
        "register-passable" | "register_passable" => "@register_passable".into(),
        "value" => "@value".into(),
        "no-inline" | "no_inline" => "@no_inline".into(),
        // Fallback: snake_case and prepend @.
        other => format!("@{}", other.replace('-', "_")),
    }
}

/// Parse any user-facing type tag string into an MType. Recognises the
/// runtime primitives, `List-T`, `Opt-T`, `Tuple-T1-T2`, and named types.
pub fn parse_type_tag(tag: &str) -> MType {
    if let Some(t) = runtime::type_hint(tag) {
        return t;
    }
    if let Some(rest) = tag.strip_prefix("List-") {
        return MType::List(Box::new(parse_type_tag(rest)));
    }
    if let Some(rest) = tag.strip_prefix("Opt-") {
        return MType::Optional(Box::new(parse_type_tag(rest)));
    }
    if let Some(rest) = tag.strip_prefix("Tuple-") {
        let parts: Vec<MType> = rest.split('-').map(parse_type_tag).collect();
        return MType::Tuple(parts);
    }
    if tag.starts_with(|c: char| c.is_ascii_uppercase()) {
        MType::Named(tag.to_string())
    } else {
        MType::Infer
    }
}

/// Peel `(__tagged__ T form)` → (MType, form). Non-tagged returns (Infer, v).
pub fn peel_tag(v: &Value) -> (MType, &Value) {
    if let Value::List(xs) = v {
        if xs.len() == 3 {
            if let Value::Symbol(h) = &xs[0] {
                if &**h == "__tagged__" {
                    let tag = sym_str(&xs[1]).unwrap_or("");
                    return (parse_type_tag(tag), &xs[2]);
                }
            }
        }
    }
    (MType::Infer, v)
}

pub fn pr(v: &Value) -> String {
    v.to_pr_string()
}

fn pr_list(v: &[Value]) -> String {
    let mut s = String::from("(");
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&pr(x));
    }
    s.push(')');
    s
}
