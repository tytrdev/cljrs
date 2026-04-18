use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

use imbl::{HashMap as PMap, HashSet as PSet, Vector as PVec};

use crate::env::Env;
use crate::error::Result;
use crate::native::NativeFn;

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Arc<str>),
    Symbol(Arc<str>),
    Keyword(Arc<str>),
    // Lists stay as Arc<Vec<Value>> — they're used as SSA-style AST
    // containers more often than as data, and never grow after read time.
    // Phase 4 may unify if needed.
    List(Arc<Vec<Value>>),
    // Persistent collections via imbl (HAMT). O(log32) core ops,
    // structural sharing on clone.
    Vector(PVec<Value>),
    Map(PMap<Value, Value>),
    Set(PSet<Value>),
    Fn(Arc<Lambda>),
    Macro(Arc<Lambda>),
    Builtin(Builtin),
    /// JIT-compiled native function. Created by `defn-native` when the
    /// `mlir` feature is on. Dispatched in `eval::apply` with a direct
    /// transmuted `extern "C"` call — no interpreter frame at invocation.
    Native(Arc<NativeFn>),
    /// Mutable single-cell reference. Clojure `atom`/`deref`/`swap!`/
    /// `reset!`/`compare-and-set!`. The inner RwLock is the only mutable
    /// state cljrs currently exposes to user code — everything else is
    /// persistent.
    Atom(Arc<RwLock<Value>>),
    /// Compiled regex pattern from `#"..."`. Wraps the regex crate's
    /// Regex; lookups go through `re-find`, `re-matches`, `re-seq`.
    Regex(Arc<regex::Regex>),
    /// Multimethod from `defmulti`. Stores the dispatch fn + a
    /// dispatch-value to method-fn map. Callable via `apply` like
    /// any other fn.
    Multi(Arc<MultiFn>),
    /// Deferred sequence with a zero-arg thunk. `first`/`rest`/`seq`
    /// force it on demand; repeated access uses the cached result.
    /// Supports infinite sequences when consumers (`take`, etc.)
    /// walk via first/rest instead of eager collection.
    LazySeq(Arc<LazySeq>),
    /// Cons cell: a head element prepended to a tail that may be any
    /// seq-like value (including a LazySeq). Created by `cons` when
    /// the tail isn't an eager collection. Walking via first/rest is
    /// the only correct way to consume; `seq_items` chases the tail.
    Cons(Arc<Value>, Arc<Value>),
    /// Wraps a value to signal "stop reducing now". `reduce` and
    /// `transduce` unwrap and return the inner value when they see one.
    Reduced(Arc<Value>),
    /// Exact rational (num / den) in reduced form, denominator positive.
    /// Produced by `/` when the result isn't an integer; preserved
    /// through `+ - *` against ints and other ratios. Mixing with
    /// floats demotes to float.
    Ratio(i64, i64),
    /// Compiled GPU kernel from `defn-gpu`. Called as a normal fn: takes
    /// one arg (an f32 buffer = vector/list of numbers) and returns a
    /// vector of f32s. Internally dispatches via wgpu.
    #[cfg(feature = "gpu")]
    GpuKernel(Arc<crate::gpu::GpuKernel>),
    /// Compiled 2D pixel-shader kernel from `defn-gpu-pixel`. Not
    /// callable via normal `apply` — the host's render loop calls
    /// `render_frame` with width/height/t/sliders.
    #[cfg(feature = "gpu")]
    GpuPixelKernel(Arc<crate::gpu::GpuPixelKernel>),
}

#[derive(Clone)]
pub struct Builtin {
    pub name: &'static str,
    pub f: Arc<dyn Fn(&[Value]) -> Result<Value> + Send + Sync>,
}

impl Builtin {
    pub fn new_static(
        name: &'static str,
        f: fn(&[Value]) -> Result<Value>,
    ) -> Self {
        Builtin {
            name,
            f: Arc::new(f),
        }
    }

    pub fn new_closure<F>(name: &'static str, f: F) -> Self
    where
        F: Fn(&[Value]) -> Result<Value> + Send + Sync + 'static,
    {
        Builtin {
            name,
            f: Arc::new(f),
        }
    }
}

pub struct Lambda {
    pub params: Vec<Arc<str>>,
    pub variadic: Option<Arc<str>>,
    pub body: Arc<Vec<Value>>,
    pub env: Env,
    pub name: Option<Arc<str>>,
}

/// A multimethod. The dispatch fn is applied to the call args and its
/// return value is looked up in `methods`. Falls back to the `:default`
/// method if no exact match. Methods are mutated via `defmethod`.
pub struct MultiFn {
    pub name: Arc<str>,
    pub dispatch: Value,
    pub methods: RwLock<imbl::HashMap<Value, Value>>,
}

/// Memoized deferred seq. Created by `(lazy-seq body)` which stores the
/// body as a 0-arg fn thunk. `force` invokes the thunk exactly once;
/// subsequent forces return the cached value.
pub struct LazySeq {
    state: std::sync::Mutex<LazyState>,
}

enum LazyState {
    Pending(Value), // 0-arg callable
    Forced(Value),  // result of the callable (a seq or nil)
}

impl LazySeq {
    pub fn new_thunk(thunk: Value) -> Self {
        LazySeq {
            state: std::sync::Mutex::new(LazyState::Pending(thunk)),
        }
    }
    /// Realize one step of the lazy seq, returning the inner seq (list,
    /// vector, nil, or another LazySeq). Caches on first force.
    pub fn force(&self) -> Result<Value> {
        // Grab the thunk out (if pending), release the lock, run it,
        // re-acquire and store. Lets the thunk recursively evaluate
        // without reentering the same mutex.
        let thunk = {
            let guard = self.state.lock().unwrap();
            match &*guard {
                LazyState::Forced(v) => return Ok(v.clone()),
                LazyState::Pending(t) => t.clone(),
            }
        };
        let v = crate::eval::apply(&thunk, &[])?;
        let mut guard = self.state.lock().unwrap();
        *guard = LazyState::Forced(v.clone());
        Ok(v)
    }
}

impl Value {
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::Keyword(_) => "keyword",
            Value::List(_) => "list",
            Value::Vector(_) => "vector",
            Value::Map(_) => "map",
            Value::Set(_) => "set",
            Value::Fn(_) => "fn",
            Value::Macro(_) => "macro",
            Value::Builtin(_) => "builtin",
            Value::Native(_) => "native",
            Value::Atom(_) => "atom",
            Value::Regex(_) => "regex",
            Value::Multi(_) => "multi",
            Value::LazySeq(_) => "lazy-seq",
            Value::Cons(_, _) => "cons",
            Value::Reduced(_) => "reduced",
            Value::Ratio(_, _) => "ratio",
            #[cfg(feature = "gpu")]
            Value::GpuKernel(_) => "gpu-kernel",
            #[cfg(feature = "gpu")]
            Value::GpuPixelKernel(_) => "gpu-pixel-kernel",
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            _ => self.to_pr_string(),
        }
    }

    pub fn to_pr_string(&self) -> String {
        match self {
            Value::Nil => "nil".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_finite() && f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Value::Str(s) => format!("\"{}\"", escape_string(s)),
            Value::Symbol(s) => s.to_string(),
            Value::Keyword(s) => format!(":{s}"),
            Value::List(v) => format!(
                "({})",
                v.iter()
                    .map(Value::to_pr_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Value::Vector(v) => format!(
                "[{}]",
                v.iter()
                    .map(Value::to_pr_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Value::Map(m) => {
                let parts: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{} {}", k.to_pr_string(), v.to_pr_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Set(s) => {
                let parts: Vec<String> = s.iter().map(Value::to_pr_string).collect();
                format!("#{{{}}}", parts.join(" "))
            }
            Value::Fn(lam) => match &lam.name {
                Some(n) => format!("#<fn {n}>"),
                None => "#<fn>".into(),
            },
            Value::Macro(lam) => match &lam.name {
                Some(n) => format!("#<macro {n}>"),
                None => "#<macro>".into(),
            },
            Value::Builtin(b) => format!("#<builtin {}>", b.name),
            Value::Native(nf) => format!("#<native {}>", nf.name),
            Value::Atom(a) => format!("#<atom {}>", a.read().unwrap().to_pr_string()),
            Value::Regex(r) => format!("#\"{}\"", r.as_str()),
            Value::Multi(m) => format!("#<multi {}>", m.name),
            Value::LazySeq(_) => "#<lazy-seq>".to_string(),
            Value::Cons(h, t) => format!("(cons {} {})", h.to_pr_string(), t.to_pr_string()),
            Value::Reduced(v) => format!("#<reduced {}>", v.to_pr_string()),
            Value::Ratio(n, d) => format!("{n}/{d}"),
            #[cfg(feature = "gpu")]
            Value::GpuKernel(k) => format!("#<gpu-kernel {}>", k.name),
            #[cfg(feature = "gpu")]
            Value::GpuPixelKernel(k) => format!("#<gpu-pixel-kernel {}>", k.name),
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
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
    out
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_pr_string())
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_pr_string())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Int(a), Float(b)) => (*a as f64) == *b,
            (Float(a), Int(b)) => *a == (*b as f64),
            (Str(a), Str(b)) => a == b,
            (Symbol(a), Symbol(b)) => a == b,
            (Keyword(a), Keyword(b)) => a == b,
            (List(a), List(b)) => a == b,
            (Vector(a), Vector(b)) => a == b,
            // Clojure: lists and vectors compare equal if same length + elements.
            (List(a), Vector(b)) => {
                a.len() == b.len()
                    && a.iter().zip(b.iter()).all(|(x, y)| x == y)
            }
            (Vector(a), List(b)) => {
                a.len() == b.len()
                    && a.iter().zip(b.iter()).all(|(x, y)| x == y)
            }
            (Map(a), Map(b)) => a == b,
            (Set(a), Set(b)) => a == b,
            (Atom(a), Atom(b)) => Arc::ptr_eq(a, b),
            (Regex(a), Regex(b)) => a.as_str() == b.as_str(),
            (Multi(a), Multi(b)) => Arc::ptr_eq(a, b),
            (LazySeq(a), LazySeq(b)) => Arc::ptr_eq(a, b),
            (Cons(ah, at), Cons(bh, bt)) => ah == bh && at == bt,
            (Reduced(a), Reduced(b)) => a == b,
            (Ratio(an, ad), Ratio(bn, bd)) => an == bn && ad == bd,
            (Ratio(n, d), Int(i)) | (Int(i), Ratio(n, d)) => *n == *i * *d,
            (Ratio(n, d), Float(f)) | (Float(f), Ratio(n, d)) => {
                (*n as f64 / *d as f64) == *f
            }
            #[cfg(feature = "gpu")]
            (GpuKernel(a), GpuKernel(b)) => Arc::ptr_eq(a, b),
            #[cfg(feature = "gpu")]
            (GpuPixelKernel(a), GpuPixelKernel(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Eq is a semantic lie in the presence of NaN-valued Floats, but imbl's
/// HashMap/HashSet require it. We accept the lie; it only surfaces if a
/// NaN is used as a map key (rare and already undefined behavior in
/// most Clojure-family implementations).
impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Discriminator bytes keep e.g. `(hash :foo)` != `(hash "foo")`
        // even though their str payloads would otherwise collide.
        match self {
            Value::Nil => 0u8.hash(state),
            Value::Bool(b) => {
                1u8.hash(state);
                b.hash(state);
            }
            Value::Int(i) => {
                2u8.hash(state);
                i.hash(state);
            }
            Value::Float(f) => {
                3u8.hash(state);
                f.to_bits().hash(state);
            }
            Value::Str(s) => {
                4u8.hash(state);
                s.as_ref().hash(state);
            }
            Value::Symbol(s) => {
                5u8.hash(state);
                s.as_ref().hash(state);
            }
            Value::Keyword(s) => {
                6u8.hash(state);
                s.as_ref().hash(state);
            }
            Value::List(v) => {
                7u8.hash(state);
                v.len().hash(state);
                for item in v.iter() {
                    item.hash(state);
                }
            }
            Value::Vector(v) => {
                8u8.hash(state);
                v.len().hash(state);
                for item in v.iter() {
                    item.hash(state);
                }
            }
            Value::Map(m) => {
                // Order-independent: XOR each (k,v) sub-hash.
                use std::collections::hash_map::DefaultHasher;
                let mut xor: u64 = 0;
                for (k, v) in m.iter() {
                    let mut sub = DefaultHasher::new();
                    k.hash(&mut sub);
                    v.hash(&mut sub);
                    xor ^= sub.finish();
                }
                9u8.hash(state);
                m.len().hash(state);
                xor.hash(state);
            }
            Value::Set(s) => {
                use std::collections::hash_map::DefaultHasher;
                let mut xor: u64 = 0;
                for item in s.iter() {
                    let mut sub = DefaultHasher::new();
                    item.hash(&mut sub);
                    xor ^= sub.finish();
                }
                10u8.hash(state);
                s.len().hash(state);
                xor.hash(state);
            }
            Value::Fn(lam) => {
                11u8.hash(state);
                (Arc::as_ptr(lam) as usize).hash(state);
            }
            Value::Macro(lam) => {
                12u8.hash(state);
                (Arc::as_ptr(lam) as usize).hash(state);
            }
            Value::Builtin(b) => {
                13u8.hash(state);
                b.name.hash(state);
            }
            Value::Native(n) => {
                14u8.hash(state);
                (Arc::as_ptr(n) as usize).hash(state);
            }
            Value::Atom(a) => {
                15u8.hash(state);
                (Arc::as_ptr(a) as usize).hash(state);
            }
            Value::Regex(r) => {
                18u8.hash(state);
                r.as_str().hash(state);
            }
            Value::Multi(m) => {
                19u8.hash(state);
                (Arc::as_ptr(m) as usize).hash(state);
            }
            Value::LazySeq(l) => {
                20u8.hash(state);
                (Arc::as_ptr(l) as usize).hash(state);
            }
            Value::Cons(h, t) => {
                21u8.hash(state);
                h.hash(state);
                t.hash(state);
            }
            Value::Reduced(v) => {
                22u8.hash(state);
                v.hash(state);
            }
            Value::Ratio(n, d) => {
                23u8.hash(state);
                n.hash(state);
                d.hash(state);
            }
            #[cfg(feature = "gpu")]
            Value::GpuKernel(k) => {
                16u8.hash(state);
                (Arc::as_ptr(k) as usize).hash(state);
            }
            #[cfg(feature = "gpu")]
            Value::GpuPixelKernel(k) => {
                17u8.hash(state);
                (Arc::as_ptr(k) as usize).hash(state);
            }
        }
    }
}
