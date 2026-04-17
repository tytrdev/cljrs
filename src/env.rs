use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::{Error, Result};
use crate::value::Value;

/// Lexical scope frame. A chain of these (inner → outer) models the
/// local environment for a running closure or `let`. Each frame owns
/// a small, linearly-scanned list of bindings — this beats a HashMap
/// for the common case of few bindings per scope, and avoids the
/// per-call HashMap allocation that dominated early benchmarks.
pub struct Frame {
    pub bindings: Vec<(Arc<str>, Value)>,
    pub parent: Option<Arc<Frame>>,
}

#[derive(Clone)]
pub struct Env {
    globals: Arc<RwLock<HashMap<Arc<str>, Value>>>,
    locals: Option<Arc<Frame>>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            globals: Arc::new(RwLock::new(HashMap::new())),
            locals: None,
        }
    }

    pub fn lookup(&self, name: &str) -> Result<Value> {
        let mut cur = self.locals.as_ref();
        while let Some(frame) = cur {
            // Newest bindings (pushed later within a single frame) win, so scan in reverse.
            for (n, v) in frame.bindings.iter().rev() {
                if n.as_ref() == name {
                    return Ok(v.clone());
                }
            }
            cur = frame.parent.as_ref();
        }
        if let Some(v) = self.globals.read().unwrap().get(name) {
            return Ok(v.clone());
        }
        Err(Error::Unbound(name.to_string()))
    }

    pub fn define_global(&self, name: &str, val: Value) {
        self.globals
            .write()
            .unwrap()
            .insert(Arc::from(name), val);
    }

    pub fn push_scope(&self, bindings: Vec<(Arc<str>, Value)>) -> Env {
        Env {
            globals: Arc::clone(&self.globals),
            locals: Some(Arc::new(Frame {
                bindings,
                parent: self.locals.clone(),
            })),
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
