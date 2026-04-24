//! cljrs → Mojo-AST lowering. Tier 1: faithful, no optimization.
//!
//! The shape of the input is `cljrs::value::Value` trees produced by
//! `cljrs::reader::read_all`. Type hints come through as
//! `(__tagged__ <tag> <form>)` sentinel lists (see reader.rs).

use std::cell::RefCell;

use cljrs::value::Value;

use crate::ast::{MExpr, MFn, MItem, MModule, MStmt, MType};
use crate::runtime;

/// Lowering context. Tracks imports that get lazily added as we encounter
/// `math.*` calls. Also holds a gensym counter for loop/cond fallbacks.
pub struct Ctx {
    imports: RefCell<Vec<String>>,
    gensym: RefCell<u32>,
}

impl Ctx {
    pub fn new() -> Self {
        Ctx { imports: RefCell::new(Vec::new()), gensym: RefCell::new(0) }
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
        items.push(lower_top(&ctx, form)?);
    }
    let imports = ctx.take_imports();
    Ok(MModule { imports, items })
}

fn lower_top(ctx: &Ctx, form: &Value) -> Result<MItem, String> {
    let list = match as_list(form) {
        Some(l) => l,
        None => return Err(format!("top-level form must be a list: {}", pr(form))),
    };
    let head = list.first().and_then(sym_str).ok_or_else(|| {
        format!("top-level form must start with a symbol: {}", pr(form))
    })?;
    match head {
        "def" => lower_def(ctx, list, form),
        "defn" | "defn-mojo" => lower_defn(ctx, list, form, &[]),
        "parameter-fn-mojo" => lower_defn(ctx, list, form, &["@parameter"]),
        "always-inline-fn-mojo" => lower_defn(ctx, list, form, &["@always_inline"]),
        "defstruct-mojo" => lower_defstruct(list, form),
        other => Err(format!(
            "unsupported top-level form `{other}` in: {}",
            pr(form)
        )),
    }
}

fn lower_defstruct(list: &[Value], form: &Value) -> Result<MItem, String> {
    // (defstruct-mojo NAME [^T field ...])
    if list.len() != 3 {
        return Err(format!("defstruct-mojo expects (defstruct-mojo NAME [fields]): {}", pr(form)));
    }
    let name = sym_str(&list[1])
        .ok_or_else(|| format!("defstruct-mojo name must be symbol: {}", pr(form)))?
        .to_string();
    let fields_vec = match &list[2] {
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
    let mut params: Vec<(String, MType)> = Vec::new();
    for p in params_vec.iter() {
        let (pty, pname) = peel_tag(p);
        let s = sym_str(pname).ok_or_else(|| {
            format!("defn param must be a symbol: {}", pr(form))
        })?;
        if s.contains('&') {
            return Err(format!("variadic params not supported in cljrs-mojo v1: {}", pr(form)));
        }
        params.push((s.to_string(), pty));
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
        ret: ret_ty,
        body: stmts,
        decorators: extra_decorators.iter().map(|s| s.to_string()).collect(),
        comment: Some(pr(form)),
    }))
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
                "for-mojo" => {
                    return lower_for_mojo_tail(ctx, list, out, mode);
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
                "for-mojo" => return lower_for_mojo_tail(ctx, list, out, TailMode::Assign("__ignore".into()))
                    .and_then(|_| {
                        // Drop the trailing assign-to-__ignore.
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
    if head == "let" || head == "loop" || head == "cond" || head == "recur" || head == "for-mojo" {
        return Err(format!(
            "`{head}` only supported in tail position in cljrs-mojo v1: {}",
            pr_list(v)
        ));
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

    // Fallback: assume the symbol names a defined fn.
    let lowered: Result<Vec<_>, _> = args.iter().map(|a| lower_expr(ctx, a)).collect();
    Ok(MExpr::Call { callee: head.to_string(), args: lowered? })
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

/// Peel `(__tagged__ T form)` → (MType, form). Non-tagged returns (Infer, v).
pub fn peel_tag(v: &Value) -> (MType, &Value) {
    if let Value::List(xs) = v {
        if xs.len() == 3 {
            if let Value::Symbol(h) = &xs[0] {
                if &**h == "__tagged__" {
                    let tag = sym_str(&xs[1]).unwrap_or("");
                    let ty = runtime::type_hint(tag).unwrap_or_else(|| {
                        // User-defined type: pass through if it looks like a
                        // type name (starts with uppercase or contains '[').
                        if tag.starts_with(|c: char| c.is_ascii_uppercase()) {
                            MType::Named(tag.to_string())
                        } else {
                            MType::Infer
                        }
                    });
                    return (ty, &xs[2]);
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
