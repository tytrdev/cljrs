use std::sync::Arc;

use crate::error::{Error, Result};
use crate::value::Value;

pub fn read_all(src: &str) -> Result<Vec<Value>> {
    let mut p = Parser::new(src);
    let mut forms = Vec::new();
    p.skip_ws();
    while !p.eof() {
        forms.push(p.read_form()?);
        p.skip_ws();
    }
    Ok(forms)
}

fn sym(s: &str) -> Value {
    Value::Symbol(Arc::from(s))
}

fn list_of(items: Vec<Value>) -> Value {
    Value::List(Arc::new(items))
}

/// Reader-time transformation of a `syntax-quoted` form into code that,
/// when evaluated, reconstructs the form — with `~x` evaluating `x` and
/// `~@xs` splicing `xs` into the enclosing sequence.
///
/// Limitation: nested syntax-quote (``form) does NOT increase the unquote
/// level the way Clojure's does — we expand eagerly. Works for the common
/// single-level macro pattern; deeply nested quoting is a known gap.
/// Auto-gensym (`foo#`) is not yet implemented.
pub fn syntax_quote(form: &Value) -> Value {
    match form {
        Value::Symbol(_) => list_of(vec![sym("quote"), form.clone()]),
        Value::List(xs) => wrap_seq(xs, /* vector */ false),
        Value::Vector(xs) => {
            let tmp: Vec<Value> = xs.iter().cloned().collect();
            wrap_seq(&tmp, /* vector */ true)
        }
        // maps, sets, and self-evaluating literals pass through quoted whole.
        Value::Map(_) => list_of(vec![sym("quote"), form.clone()]),
        _ => form.clone(),
    }
}

fn wrap_seq(xs: &[Value], as_vector: bool) -> Value {
    let mut items: Vec<Value> = Vec::with_capacity(xs.len() + 1);
    items.push(sym("concat"));
    for e in xs {
        items.push(quoted_item(e));
    }
    let concatted = list_of(items);
    if as_vector {
        list_of(vec![sym("vec"), concatted])
    } else {
        concatted
    }
}

/// Walk an anon-fn body to find the highest `%N` reference and whether
/// `%&` is used. Used by `#(...)` to decide the implicit param vector.
fn scan_anon_params(body: &[Value]) -> (usize, bool) {
    fn walk(v: &Value, max: &mut usize, rest: &mut bool) {
        match v {
            Value::Symbol(s) => {
                let n = s.as_ref();
                if n == "%" || n == "%1" {
                    *max = (*max).max(1);
                } else if n == "%&" {
                    *rest = true;
                } else if let Some(rest_num) = n.strip_prefix('%')
                    && let Ok(idx) = rest_num.parse::<usize>()
                    && idx >= 1
                {
                    *max = (*max).max(idx);
                }
            }
            Value::List(xs) => {
                for x in xs.iter() {
                    walk(x, max, rest);
                }
            }
            Value::Vector(xs) => {
                for x in xs.iter() {
                    walk(x, max, rest);
                }
            }
            Value::Map(m) => {
                for (k, val) in m.iter() {
                    walk(k, max, rest);
                    walk(val, max, rest);
                }
            }
            Value::Set(s) => {
                for x in s.iter() {
                    walk(x, max, rest);
                }
            }
            _ => {}
        }
    }
    let mut max = 0usize;
    let mut rest = false;
    for f in body {
        walk(f, &mut max, &mut rest);
    }
    (max, rest)
}

fn quoted_item(e: &Value) -> Value {
    if let Value::List(inner) = e
        && inner.len() == 2
        && let Value::Symbol(s) = &inner[0]
    {
        match s.as_ref() {
            "unquote" => return list_of(vec![sym("list"), inner[1].clone()]),
            "unquote-splicing" => return inner[1].clone(),
            _ => {}
        }
    }
    list_of(vec![sym("list"), syntax_quote(e)])
}

pub fn read_one(src: &str) -> Result<Value> {
    let mut p = Parser::new(src);
    p.skip_ws();
    if p.eof() {
        return Err(Error::Read("empty input".into()));
    }
    p.read_form()
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Parser {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || c == b',' {
                self.pos += 1;
            } else if c == b';' {
                while let Some(c) = self.peek() {
                    self.pos += 1;
                    if c == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn read_form(&mut self) -> Result<Value> {
        self.skip_ws();
        let c = self
            .peek()
            .ok_or_else(|| Error::Read("unexpected eof".into()))?;
        match c {
            b'(' => {
                self.pos += 1;
                self.read_seq(b')').map(|v| Value::List(Arc::new(v)))
            }
            b'[' => {
                self.pos += 1;
                self.read_seq(b']')
                    .map(|v| Value::Vector(v.into_iter().collect()))
            }
            b'{' => {
                self.pos += 1;
                self.read_map()
            }
            b')' | b']' | b'}' => Err(Error::Read(format!("unexpected '{}'", c as char))),
            b'"' => self.read_string(),
            b'\'' => {
                self.pos += 1;
                let form = self.read_form()?;
                Ok(list_of(vec![sym("quote"), form]))
            }
            b'`' => {
                self.pos += 1;
                let form = self.read_form()?;
                Ok(syntax_quote(&form))
            }
            b'~' => {
                self.pos += 1;
                if self.peek() == Some(b'@') {
                    self.pos += 1;
                    let form = self.read_form()?;
                    Ok(list_of(vec![sym("unquote-splicing"), form]))
                } else {
                    let form = self.read_form()?;
                    Ok(list_of(vec![sym("unquote"), form]))
                }
            }
            b'^' => {
                // Type / metadata hint: `^Tag form` attaches Tag to form.
                // We surface it as a 3-element sentinel list that the
                // evaluator treats transparently and that `defn-native`
                // walks for codegen.
                self.pos += 1;
                let tag = self.read_form()?;
                let form = self.read_form()?;
                Ok(list_of(vec![sym("__tagged__"), tag, form]))
            }
            b'#' => self.read_dispatch(),
            b'@' => {
                self.pos += 1;
                let form = self.read_form()?;
                Ok(list_of(vec![sym("deref"), form]))
            }
            b':' => self.read_keyword(),
            c if c.is_ascii_digit() => self.read_number(),
            b'-' | b'+' => {
                if let Some(n) = self.src.get(self.pos + 1)
                    && n.is_ascii_digit()
                {
                    return self.read_number();
                }
                self.read_symbol()
            }
            _ => self.read_symbol(),
        }
    }

    /// `#` reader dispatch. Handles:
    ///   `#{a b c}`  → set literal
    ///   `#_form`    → discard next form (returns the one after)
    ///   `#(body)`   → anonymous fn, with `%`, `%1`, `%2`, `%&` params
    fn read_dispatch(&mut self) -> Result<Value> {
        self.pos += 1; // consume '#'
        let next = self
            .peek()
            .ok_or_else(|| Error::Read("unexpected eof after '#'".into()))?;
        match next {
            b'{' => {
                self.pos += 1;
                let items = self.read_seq(b'}')?;
                Ok(Value::Set(items.into_iter().collect()))
            }
            b'_' => {
                self.pos += 1;
                // Read and discard the next form.
                let _ = self.read_form()?;
                // Then read and return the form after it. If we're inside
                // a sequence, the caller will see this as the next real item.
                self.read_form()
            }
            b'(' => {
                self.pos += 1;
                let body = self.read_seq(b')')?;
                // Walk body collecting %, %1, %2, ... %& references.
                let (max_idx, has_rest) = scan_anon_params(&body);
                let mut params: Vec<Value> = Vec::new();
                for i in 1..=max_idx {
                    params.push(sym(&format!("%{i}")));
                }
                if has_rest {
                    params.push(sym("&"));
                    params.push(sym("%&"));
                }
                // Also allow bare `%` → same as `%1`. We leave `%` as an
                // alias by binding it below.
                let mut bindings: Vec<Value> = Vec::new();
                if max_idx >= 1 {
                    bindings.push(sym("%"));
                    bindings.push(sym("%1"));
                }
                let call = Value::List(Arc::new(body));
                let body_form = if bindings.is_empty() {
                    call
                } else {
                    list_of(vec![
                        sym("let"),
                        Value::Vector(bindings.into_iter().collect()),
                        call,
                    ])
                };
                let fn_form = list_of(vec![
                    sym("fn"),
                    Value::Vector(params.into_iter().collect()),
                    body_form,
                ]);
                Ok(fn_form)
            }
            b'\'' => {
                // #'sym — var reference. We don't have vars yet; treat as quote for now.
                self.pos += 1;
                let form = self.read_form()?;
                Ok(list_of(vec![sym("quote"), form]))
            }
            other => Err(Error::Read(format!("unknown reader dispatch #{}", other as char))),
        }
    }

    fn read_seq(&mut self, close: u8) -> Result<Vec<Value>> {
        let mut v = Vec::new();
        loop {
            self.skip_ws();
            let c = self.peek().ok_or_else(|| {
                Error::Read(format!("unclosed sequence, expected '{}'", close as char))
            })?;
            if c == close {
                self.pos += 1;
                return Ok(v);
            }
            v.push(self.read_form()?);
        }
    }

    fn read_map(&mut self) -> Result<Value> {
        let items = self.read_seq(b'}')?;
        if items.len() % 2 != 0 {
            return Err(Error::Read(
                "map literal must have an even number of forms".into(),
            ));
        }
        let mut pairs = Vec::with_capacity(items.len() / 2);
        let mut it = items.into_iter();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            pairs.push((k, v));
        }
        Ok(Value::Map(pairs.into_iter().collect()))
    }

    fn read_string(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(&c) = self.src.get(self.pos) {
            match c {
                b'"' => {
                    self.pos += 1;
                    let s = String::from_utf8(buf)
                        .map_err(|e| Error::Read(format!("invalid utf8 in string: {e}")))?;
                    return Ok(Value::Str(Arc::from(s.as_str())));
                }
                b'\\' => {
                    self.pos += 1;
                    let esc = *self
                        .src
                        .get(self.pos)
                        .ok_or_else(|| Error::Read("bad escape at eof".into()))?;
                    self.pos += 1;
                    match esc {
                        b'n' => buf.push(b'\n'),
                        b't' => buf.push(b'\t'),
                        b'r' => buf.push(b'\r'),
                        b'\\' => buf.push(b'\\'),
                        b'"' => buf.push(b'"'),
                        other => {
                            return Err(Error::Read(format!("bad escape \\{}", other as char)));
                        }
                    }
                }
                _ => {
                    buf.push(c);
                    self.pos += 1;
                }
            }
        }
        Err(Error::Read("unterminated string".into()))
    }

    fn read_keyword(&mut self) -> Result<Value> {
        self.pos += 1;
        let tok = self.read_atom_token();
        if tok.is_empty() {
            return Err(Error::Read("empty keyword".into()));
        }
        Ok(Value::Keyword(Arc::from(tok.as_str())))
    }

    fn read_number(&mut self) -> Result<Value> {
        let tok = self.read_atom_token();
        if let Ok(i) = tok.parse::<i64>() {
            return Ok(Value::Int(i));
        }
        if let Ok(f) = tok.parse::<f64>() {
            return Ok(Value::Float(f));
        }
        Err(Error::Read(format!("bad number: {tok}")))
    }

    fn read_symbol(&mut self) -> Result<Value> {
        let tok = self.read_atom_token();
        match tok.as_str() {
            "" => Err(Error::Read("empty symbol".into())),
            "nil" => Ok(Value::Nil),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Ok(Value::Symbol(Arc::from(tok.as_str()))),
        }
    }

    fn read_atom_token(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace()
                || c == b','
                || c == b'('
                || c == b')'
                || c == b'['
                || c == b']'
                || c == b'{'
                || c == b'}'
                || c == b'"'
                || c == b';'
                || c == b'^'
            {
                break;
            }
            self.pos += 1;
        }
        // SAFETY substitute: src came from a &str, so the byte range is valid UTF-8
        // because we only stopped on ASCII delimiters.
        String::from_utf8_lossy(&self.src[start..self.pos]).into_owned()
    }
}
