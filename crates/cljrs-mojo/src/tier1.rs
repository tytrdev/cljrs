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
        "defn" | "defn-mojo" => lower_defn(ctx, list, form),
        other => Err(format!(
            "unsupported top-level form `{other}` in: {}",
            pr(form)
        )),
    }
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

fn lower_defn(ctx: &Ctx, list: &[Value], form: &Value) -> Result<MItem, String> {
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
        decorators: Vec::new(),
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
                        let e = lower_expr(ctx, f)?;
                        out.push(MStmt::Expr(e));
                    }
                    if let Some(last) = list.last().filter(|_| list.len() > 1) {
                        return lower_expr_tail(ctx, last, out, mode);
                    }
                    // empty `do` — return 0 / pass
                    out.push(finish(mode, MExpr::IntLit(0)));
                    return Ok(());
                }
                "let" => {
                    lower_let(ctx, list, out)?;
                    // Fall through: the last form's value is the tail.
                    // But lower_let already emitted bindings and the body
                    // is emitted below. Actually, simpler: inline here.
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
        let e = lower_expr(ctx, f)?;
        out.push(MStmt::Expr(e));
    }
    lower_expr_tail(ctx, body.last().unwrap(), out, mode)
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
        Value::Str(_) => Err("string literals not supported in cljrs-mojo v1".into()),
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
    if head == "let" || head == "loop" || head == "cond" || head == "recur" {
        return Err(format!(
            "`{head}` only supported in tail position in cljrs-mojo v1: {}",
            pr_list(v)
        ));
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
                    let ty = runtime::type_hint(tag).unwrap_or(MType::Infer);
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
