use std::sync::Arc;

use crate::env::Env;
use crate::error::{Error, Result};
use crate::eval;
use crate::value::{Builtin, Value};

pub fn install(env: &Env) {
    for (name, f) in core_fns() {
        env.define_global(name, Value::Builtin(Builtin { name, f }));
    }
}

fn core_fns() -> Vec<(&'static str, fn(&[Value]) -> Result<Value>)> {
    vec![
        ("+", add),
        ("-", sub),
        ("*", mul),
        ("/", div),
        ("=", eq),
        ("<", lt),
        (">", gt),
        ("<=", le),
        (">=", ge),
        ("not", not_fn),
        ("str", to_str),
        ("println", println_fn),
        ("pr-str", pr_str_fn),
        ("count", count_fn),
        ("first", first_fn),
        ("rest", rest_fn),
        ("cons", cons_fn),
        ("concat", concat_fn),
        ("list", list_fn),
        ("vector", vector_fn),
        ("conj", conj_fn),
        ("nth", nth_fn),
        ("vec", vec_fn),
        ("nil?", nil_q),
        ("zero?", zero_q),
        ("empty?", empty_q),
        ("inc", inc_fn),
        ("dec", dec_fn),
        ("map", map_fn),
        ("filter", filter_fn),
        ("reduce", reduce_fn),
        ("range", range_fn),
        ("take", take_fn),
        ("drop", drop_fn),
        ("even?", even_q),
        ("odd?", odd_q),
        ("pos?", pos_q),
        ("neg?", neg_q),
        ("identity", identity_fn),
    ]
}

#[derive(Clone, Copy)]
enum Num {
    I(i64),
    F(f64),
}

fn to_num(v: &Value) -> Result<Num> {
    match v {
        Value::Int(i) => Ok(Num::I(*i)),
        Value::Float(f) => Ok(Num::F(*f)),
        _ => Err(Error::Type(format!(
            "expected number, got {}",
            v.type_name()
        ))),
    }
}

fn num_to_value(n: Num) -> Value {
    match n {
        Num::I(i) => Value::Int(i),
        Num::F(f) => Value::Float(f),
    }
}

fn fold_num<F, G>(args: &[Value], init: Num, fi: F, ff: G) -> Result<Value>
where
    F: Fn(i64, i64) -> Option<i64>,
    G: Fn(f64, f64) -> f64,
{
    let mut acc = init;
    for a in args {
        let n = to_num(a)?;
        acc = match (acc, n) {
            (Num::I(x), Num::I(y)) => match fi(x, y) {
                Some(r) => Num::I(r),
                None => Num::F(ff(x as f64, y as f64)),
            },
            (Num::I(x), Num::F(y)) => Num::F(ff(x as f64, y)),
            (Num::F(x), Num::I(y)) => Num::F(ff(x, y as f64)),
            (Num::F(x), Num::F(y)) => Num::F(ff(x, y)),
        };
    }
    Ok(num_to_value(acc))
}

fn add(args: &[Value]) -> Result<Value> {
    fold_num(args, Num::I(0), |a, b| a.checked_add(b), |a, b| a + b)
}

fn sub(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity {
            expected: ">= 1".into(),
            got: 0,
        });
    }
    if args.len() == 1 {
        return match to_num(&args[0])? {
            Num::I(i) => Ok(Value::Int(-i)),
            Num::F(f) => Ok(Value::Float(-f)),
        };
    }
    let first = to_num(&args[0])?;
    fold_num(&args[1..], first, |a, b| a.checked_sub(b), |a, b| a - b)
}

fn mul(args: &[Value]) -> Result<Value> {
    fold_num(args, Num::I(1), |a, b| a.checked_mul(b), |a, b| a * b)
}

fn div(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity {
            expected: ">= 1".into(),
            got: 0,
        });
    }
    if args.len() == 1 {
        return match to_num(&args[0])? {
            Num::I(i) if i != 0 && 1 % i == 0 => Ok(Value::Int(1 / i)),
            Num::I(i) => Ok(Value::Float(1.0 / i as f64)),
            Num::F(f) => Ok(Value::Float(1.0 / f)),
        };
    }
    let first = to_num(&args[0])?;
    fold_num(
        &args[1..],
        first,
        |a, b| {
            if b != 0 && a % b == 0 {
                Some(a / b)
            } else {
                None
            }
        },
        |a, b| a / b,
    )
}

fn eq(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    let first = &args[0];
    for a in &args[1..] {
        if a != first {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

fn cmp<F>(args: &[Value], f: F) -> Result<Value>
where
    F: Fn(f64, f64) -> bool,
{
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    let mut prev = as_f64(&args[0])?;
    for a in &args[1..] {
        let cur = as_f64(a)?;
        if !f(prev, cur) {
            return Ok(Value::Bool(false));
        }
        prev = cur;
    }
    Ok(Value::Bool(true))
}

fn as_f64(v: &Value) -> Result<f64> {
    match to_num(v)? {
        Num::I(i) => Ok(i as f64),
        Num::F(x) => Ok(x),
    }
}

fn lt(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a < b)
}
fn gt(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a > b)
}
fn le(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a <= b)
}
fn ge(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a >= b)
}

fn not_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(!args[0].truthy()))
}

fn to_str(args: &[Value]) -> Result<Value> {
    let mut s = String::new();
    for a in args {
        if !matches!(a, Value::Nil) {
            s.push_str(&a.to_display_string());
        }
    }
    Ok(Value::Str(Arc::from(s.as_str())))
}

fn println_fn(args: &[Value]) -> Result<Value> {
    let mut first = true;
    for a in args {
        if !first {
            print!(" ");
        }
        first = false;
        print!("{}", a.to_display_string());
    }
    println!();
    Ok(Value::Nil)
}

fn pr_str_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_pr_string).collect();
    Ok(Value::Str(Arc::from(parts.join(" ").as_str())))
}

fn count_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let n = match &args[0] {
        Value::Nil => 0,
        Value::List(v) | Value::Vector(v) => v.len(),
        Value::Map(m) => m.len(),
        Value::Str(s) => s.chars().count(),
        _ => {
            return Err(Error::Type(format!(
                "count on non-sequence: {}",
                args[0].type_name()
            )));
        }
    };
    Ok(Value::Int(n as i64))
}

fn first_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::List(v) | Value::Vector(v) => Ok(v.first().cloned().unwrap_or(Value::Nil)),
        _ => Err(Error::Type(format!(
            "first on non-sequence: {}",
            args[0].type_name()
        ))),
    }
}

fn rest_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match &args[0] {
        Value::Nil => Ok(Value::List(Arc::new(Vec::new()))),
        Value::List(v) | Value::Vector(v) => {
            if v.is_empty() {
                Ok(Value::List(Arc::new(Vec::new())))
            } else {
                Ok(Value::List(Arc::new(v[1..].to_vec())))
            }
        }
        _ => Err(Error::Type(format!(
            "rest on non-sequence: {}",
            args[0].type_name()
        ))),
    }
}

fn cons_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let mut out = Vec::new();
    out.push(args[0].clone());
    match &args[1] {
        Value::Nil => {}
        Value::List(v) | Value::Vector(v) => out.extend(v.iter().cloned()),
        _ => {
            return Err(Error::Type(format!(
                "cons onto non-sequence: {}",
                args[1].type_name()
            )));
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn list_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::List(Arc::new(args.to_vec())))
}

fn concat_fn(args: &[Value]) -> Result<Value> {
    let mut out: Vec<Value> = Vec::new();
    for a in args {
        match a {
            Value::Nil => {}
            Value::List(v) | Value::Vector(v) => out.extend(v.iter().cloned()),
            _ => {
                return Err(Error::Type(format!(
                    "concat: not a sequence: {}",
                    a.type_name()
                )));
            }
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn vector_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Vector(Arc::new(args.to_vec())))
}

fn conj_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity {
            expected: ">= 1".into(),
            got: 0,
        });
    }
    match &args[0] {
        Value::Nil => {
            let mut out: Vec<Value> = Vec::new();
            for a in &args[1..] {
                out.insert(0, a.clone());
            }
            Ok(Value::List(Arc::new(out)))
        }
        Value::Vector(v) => {
            let mut out = (**v).clone();
            for a in &args[1..] {
                out.push(a.clone());
            }
            Ok(Value::Vector(Arc::new(out)))
        }
        Value::List(v) => {
            let mut out = (**v).clone();
            for a in &args[1..] {
                out.insert(0, a.clone());
            }
            Ok(Value::List(Arc::new(out)))
        }
        _ => Err(Error::Type(format!("conj onto {}", args[0].type_name()))),
    }
}

fn nth_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let idx = match &args[1] {
        Value::Int(i) => *i,
        _ => return Err(Error::Type("nth index must be int".into())),
    };
    if idx < 0 {
        return Err(Error::Eval("nth: negative index".into()));
    }
    match &args[0] {
        Value::List(v) | Value::Vector(v) => v
            .get(idx as usize)
            .cloned()
            .ok_or_else(|| Error::Eval(format!("nth: index {idx} out of range"))),
        _ => Err(Error::Type("nth on non-sequence".into())),
    }
}

fn vec_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match &args[0] {
        Value::Nil => Ok(Value::Vector(Arc::new(Vec::new()))),
        Value::List(v) | Value::Vector(v) => Ok(Value::Vector(Arc::clone(v))),
        _ => Err(Error::Type("vec on non-sequence".into())),
    }
}

fn nil_q(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(matches!(args[0], Value::Nil)))
}

fn zero_q(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(match &args[0] {
        Value::Int(0) => true,
        Value::Float(f) if *f == 0.0 => true,
        _ => false,
    }))
}

fn empty_q(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(match &args[0] {
        Value::Nil => true,
        Value::List(v) | Value::Vector(v) => v.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Str(s) => s.is_empty(),
        _ => false,
    }))
}

fn inc_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match to_num(&args[0])? {
        Num::I(i) => Ok(i.checked_add(1).map(Value::Int).unwrap_or_else(|| Value::Float(i as f64 + 1.0))),
        Num::F(f) => Ok(Value::Float(f + 1.0)),
    }
}

fn dec_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match to_num(&args[0])? {
        Num::I(i) => Ok(i.checked_sub(1).map(Value::Int).unwrap_or_else(|| Value::Float(i as f64 - 1.0))),
        Num::F(f) => Ok(Value::Float(f - 1.0)),
    }
}

fn as_seq<'a>(v: &'a Value) -> Result<&'a [Value]> {
    match v {
        Value::Nil => Ok(&[]),
        Value::List(v) | Value::Vector(v) => Ok(v.as_slice()),
        _ => Err(Error::Type(format!(
            "expected sequence, got {}",
            v.type_name()
        ))),
    }
}

fn map_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity {
            expected: ">= 2".into(),
            got: args.len(),
        });
    }
    let f = &args[0];
    let coll = as_seq(&args[1])?;
    let mut out = Vec::with_capacity(coll.len());
    for item in coll {
        out.push(eval::apply(f, std::slice::from_ref(item))?);
    }
    Ok(Value::List(Arc::new(out)))
}

fn filter_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let pred = &args[0];
    let coll = as_seq(&args[1])?;
    let mut out = Vec::new();
    for item in coll {
        let keep = eval::apply(pred, std::slice::from_ref(item))?;
        if keep.truthy() {
            out.push(item.clone());
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn reduce_fn(args: &[Value]) -> Result<Value> {
    match args.len() {
        2 => {
            let f = &args[0];
            let coll = as_seq(&args[1])?;
            if coll.is_empty() {
                // Clojure's reduce with no init on empty coll: call f with no args
                return eval::apply(f, &[]);
            }
            let mut acc = coll[0].clone();
            for item in &coll[1..] {
                acc = eval::apply(f, &[acc, item.clone()])?;
            }
            Ok(acc)
        }
        3 => {
            let f = &args[0];
            let mut acc = args[1].clone();
            let coll = as_seq(&args[2])?;
            for item in coll {
                acc = eval::apply(f, &[acc, item.clone()])?;
            }
            Ok(acc)
        }
        n => Err(Error::Arity {
            expected: "2 or 3".into(),
            got: n,
        }),
    }
}

fn range_fn(args: &[Value]) -> Result<Value> {
    let (start, end, step) = match args.len() {
        1 => (0i64, to_i64(&args[0])?, 1i64),
        2 => (to_i64(&args[0])?, to_i64(&args[1])?, 1i64),
        3 => (to_i64(&args[0])?, to_i64(&args[1])?, to_i64(&args[2])?),
        n => {
            return Err(Error::Arity {
                expected: "1, 2, or 3".into(),
                got: n,
            });
        }
    };
    if step == 0 {
        return Err(Error::Eval("range: step cannot be zero".into()));
    }
    let mut out = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < end {
            out.push(Value::Int(i));
            i += step;
        }
    } else {
        while i > end {
            out.push(Value::Int(i));
            i += step;
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn to_i64(v: &Value) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        _ => Err(Error::Type(format!(
            "expected integer, got {}",
            v.type_name()
        ))),
    }
}

fn take_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let n = to_i64(&args[0])?.max(0) as usize;
    let coll = as_seq(&args[1])?;
    let taken: Vec<Value> = coll.iter().take(n).cloned().collect();
    Ok(Value::List(Arc::new(taken)))
}

fn drop_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let n = to_i64(&args[0])?.max(0) as usize;
    let coll = as_seq(&args[1])?;
    let dropped: Vec<Value> = coll.iter().skip(n).cloned().collect();
    Ok(Value::List(Arc::new(dropped)))
}

fn even_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? % 2 == 0))
}
fn odd_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? % 2 != 0))
}
fn pos_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? > 0))
}
fn neg_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? < 0))
}

fn identity_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(args[0].clone())
}
