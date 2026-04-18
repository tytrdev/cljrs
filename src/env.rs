use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::{Error, Result};
use crate::native::{NativeRegistry, NativeSig};
use crate::value::Value;

/// Lexical scope frame. A chain of these (inner → outer) models the
/// local environment for a running closure or `let`. Each frame owns
/// a small, linearly-scanned list of bindings, which beats a HashMap
/// for the common case of few bindings per scope and avoids the
/// per-call allocation that dominated early benchmarks.
pub struct Frame {
    pub bindings: Vec<(Arc<str>, Value)>,
    pub parent: Option<Arc<Frame>>,
}

#[derive(Clone)]
pub struct Env {
    /// Global bindings stored as fully qualified "ns/name" keys. Each
    /// `def`/`defn`/builtin install puts its name under the current
    /// namespace. Lookups for unqualified names check the current ns
    /// first, then fall back to `cljrs.core` (where the builtins and
    /// prelude live).
    globals: Arc<RwLock<HashMap<Arc<str>, Value>>>,
    /// Per-namespace aliases for `(require :as alias)`. Keyed by
    /// (consumer-ns, alias) -> target-ns. When resolving `alias/name`,
    /// we rewrite to `target/name` before global lookup.
    aliases: Arc<RwLock<HashMap<(Arc<str>, Arc<str>), Arc<str>>>>,
    /// Name of the current namespace. Mutated by `ns` / `in-ns`.
    current_ns: Arc<RwLock<Arc<str>>>,
    locals: Option<Arc<Frame>>,
}

/// Namespace where the built-ins and prelude live, and the implicit
/// fallback for unqualified lookups in any namespace.
pub const CORE_NS: &str = "cljrs.core";
/// Default namespace for user code before any `ns` form runs.
pub const USER_NS: &str = "user";

impl Env {
    pub fn new() -> Self {
        Env {
            globals: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            current_ns: Arc::new(RwLock::new(Arc::from(CORE_NS))),
            locals: None,
        }
    }

    pub fn current_ns(&self) -> Arc<str> {
        Arc::clone(&self.current_ns.read().unwrap())
    }

    pub fn set_current_ns(&self, name: &str) {
        *self.current_ns.write().unwrap() = Arc::from(name);
    }

    pub fn add_alias(&self, consumer: &str, alias: &str, target: &str) {
        self.aliases
            .write()
            .unwrap()
            .insert((Arc::from(consumer), Arc::from(alias)), Arc::from(target));
    }

    /// Look up a symbol. Local scope first (lexical chain), then globals:
    /// - qualified `ns/name`: resolve any alias, then look up directly
    /// - unqualified: try `<current-ns>/name`, then `cljrs.core/name`
    pub fn lookup(&self, name: &str) -> Result<Value> {
        let mut cur = self.locals.as_ref();
        while let Some(frame) = cur {
            for (n, v) in frame.bindings.iter().rev() {
                if n.as_ref() == name {
                    return Ok(v.clone());
                }
            }
            cur = frame.parent.as_ref();
        }
        let globals = self.globals.read().unwrap();
        if let Some((prefix, suffix)) = split_qualified(name) {
            let current = self.current_ns();
            let aliases = self.aliases.read().unwrap();
            let target = aliases
                .get(&(Arc::clone(&current), Arc::from(prefix)))
                .map(|t| t.as_ref().to_string())
                .unwrap_or_else(|| prefix.to_string());
            let key = format!("{target}/{suffix}");
            if let Some(v) = globals.get(key.as_str()) {
                return Ok(v.clone());
            }
            return Err(Error::Unbound(name.to_string()));
        }
        // Unqualified: current ns, then cljrs.core.
        let current = self.current_ns();
        let key = format!("{current}/{name}");
        if let Some(v) = globals.get(key.as_str()) {
            return Ok(v.clone());
        }
        if current.as_ref() != CORE_NS {
            let core_key = format!("{CORE_NS}/{name}");
            if let Some(v) = globals.get(core_key.as_str()) {
                return Ok(v.clone());
            }
        }
        Err(Error::Unbound(name.to_string()))
    }

    /// Define a global. Bare names go into the current namespace.
    /// Already-qualified names (`ns/x`) keep their qualification.
    /// `/` by itself is the division operator, not a qualifier.
    pub fn define_global(&self, name: &str, val: Value) {
        let qualified = split_qualified(name).is_some();
        let key: Arc<str> = if qualified {
            Arc::from(name)
        } else {
            Arc::from(format!("{}/{name}", self.current_ns()).as_str())
        };
        self.globals.write().unwrap().insert(key, val);
    }

    /// Build a `NativeRegistry` containing every currently-bound native
    /// fn in globals. Consumed by the MLIR compiler so a new
    /// `defn-native` body can call previously-defined natives via an
    /// MLIR `func.call`. The registry is keyed by the unqualified name
    /// so generated symbol names don't leak the namespace prefix.
    pub fn snapshot_natives(&self) -> NativeRegistry {
        let mut by_name = HashMap::new();
        for (k, v) in self.globals.read().unwrap().iter() {
            if let Value::Native(nf) = v {
                let short = k.rsplit('/').next().unwrap_or(k).to_string();
                by_name.insert(
                    short,
                    NativeSig {
                        arg_types: nf.arg_types.clone(),
                        ret_type: nf.ret_type,
                        ptr: nf.ptr,
                    },
                );
            }
        }
        NativeRegistry { by_name }
    }

    pub fn push_scope(&self, bindings: Vec<(Arc<str>, Value)>) -> Env {
        Env {
            globals: Arc::clone(&self.globals),
            aliases: Arc::clone(&self.aliases),
            current_ns: Arc::clone(&self.current_ns),
            locals: Some(Arc::new(Frame {
                bindings,
                parent: self.locals.clone(),
            })),
        }
    }
}

fn split_qualified(name: &str) -> Option<(&str, &str)> {
    // Treat a single `/` as the namespace separator. The name `/`
    // itself (division) has no prefix; handle that case by requiring
    // both sides non-empty.
    let mut it = name.splitn(2, '/');
    let prefix = it.next()?;
    let suffix = it.next()?;
    if prefix.is_empty() || suffix.is_empty() {
        return None;
    }
    Some((prefix, suffix))
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
