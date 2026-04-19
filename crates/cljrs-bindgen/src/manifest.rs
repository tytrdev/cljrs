//! Manifest schema. Parsed via serde+toml.
//!
//! A manifest binds one Rust crate. It carries:
//!   * `[crate]`   — identity + cljrs target namespace.
//!   * `[[fn]]`    — free functions: `crate_name::fn_name(args...) -> ret`.
//!   * `[[method]] — methods on opaque handles previously produced by
//!                   some `[[fn]]` returning `opaque:Tag`. The `on` field
//!                   names the tag; the call lowers to
//!                   `(*handle).method_name(args...)`.

use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(rename = "crate")]
    pub crate_: CrateInfo,
    #[serde(rename = "fn", default)]
    pub fns: Vec<Function>,
    #[serde(rename = "method", default)]
    pub methods: Vec<Method>,
}

#[derive(Debug, Deserialize)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub ns: String,
    /// Optional `use` lines emitted at the top of the generated install
    /// file. Lets the manifest bring opaque types into scope under the
    /// short name used by `opaque:Tag` (e.g.
    /// `"rand::rngs::ThreadRng as Rng"`).
    #[serde(default)]
    pub imports: Vec<String>,
    /// Optional path prefix prepended to every `[[fn]]` `name`. Defaults
    /// to the crate name; set to `""` to call free functions imported
    /// via `imports`.
    #[serde(default, rename = "fn-prefix")]
    pub fn_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Function {
    /// Foreign Rust path inside the crate. May be a single ident
    /// (`random`) or `module::name` (e.g. `seq::IteratorRandom`).
    pub name: String,
    /// Cljrs-side name; what the user calls.
    #[serde(rename = "clj-name")]
    pub clj_name: String,
    pub args: Vec<String>,
    pub returns: String,
    /// Optional call-site template overriding the default
    /// `name(arg0, arg1, ...)`. Substitutions: `{0}`, `{1}`, ...
    /// Useful for casts (`seed_from_u64({0} as u64)`) or wrapper
    /// shapes (Range, Option) the foreign API requires.
    #[serde(default)]
    pub call: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Method {
    /// Opaque tag the method dispatches on.
    pub on: String,
    /// Rust method name on the handle.
    pub name: String,
    /// Cljrs-side name.
    #[serde(rename = "clj-name")]
    pub clj_name: String,
    pub args: Vec<String>,
    pub returns: String,
    /// Optional call-site template overriding the default
    /// `name(arg0, arg1, ...)` for cases where the foreign API takes
    /// shapes like `Range` (e.g. `gen_range(0..10)`).
    /// Substitutions: `{0}`, `{1}`, ... reference the marshalled args.
    /// Example: `call = "gen_range({0}..{1})"`.
    #[serde(default)]
    pub call: Option<String>,
}

/// Whitelisted parameter / return types. Parsed from the manifest's
/// stringly-typed fields so error reporting is concentrated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I64,
    F64,
    Bool,
    Str,
    /// Opaque handle, tagged. Tag is just a short human name shared
    /// between functions returning this type and methods receiving it.
    Opaque(String),
    /// Vector of i64 / f64 — marshalled from a cljrs vector or list of
    /// numbers. Useful for batch numeric APIs (e.g. `shuffle`).
    VecI64,
    VecF64,
}

impl Type {
    pub fn parse(s: &str) -> Result<Type, BindgenError> {
        let s = s.trim();
        Ok(match s {
            "i64" => Type::I64,
            "f64" => Type::F64,
            "bool" => Type::Bool,
            "string" => Type::Str,
            "vec<i64>" | "Vec<i64>" => Type::VecI64,
            "vec<f64>" | "Vec<f64>" => Type::VecF64,
            other => {
                if let Some(rest) = other.strip_prefix("opaque:") {
                    let tag = rest.trim();
                    if tag.is_empty() {
                        return Err(BindgenError(format!(
                            "type `{other}` missing tag after `opaque:`"
                        )));
                    }
                    Type::Opaque(tag.to_string())
                } else {
                    return Err(BindgenError(format!(
                        "unsupported type `{other}` — whitelist: i64, f64, bool, string, vec<i64>, vec<f64>, opaque:Tag"
                    )));
                }
            }
        })
    }
}

#[derive(Debug)]
pub struct BindgenError(pub String);

impl fmt::Display for BindgenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BindgenError {}

impl Manifest {
    pub fn from_str(s: &str) -> Result<Manifest, BindgenError> {
        let m: Manifest = toml::from_str(s)
            .map_err(|e| BindgenError(format!("invalid TOML: {e}")))?;
        m.validate()?;
        Ok(m)
    }

    fn validate(&self) -> Result<(), BindgenError> {
        if self.crate_.name.is_empty() {
            return Err(BindgenError("[crate].name is empty".into()));
        }
        if self.crate_.ns.is_empty() {
            return Err(BindgenError("[crate].ns is empty".into()));
        }
        for f in &self.fns {
            for a in &f.args {
                Type::parse(a).map_err(|e| {
                    BindgenError(format!("fn `{}` arg type: {}", f.clj_name, e.0))
                })?;
            }
            Type::parse(&f.returns).map_err(|e| {
                BindgenError(format!("fn `{}` return type: {}", f.clj_name, e.0))
            })?;
        }
        for m in &self.methods {
            if m.on.is_empty() {
                return Err(BindgenError(format!(
                    "method `{}` missing `on` (opaque tag)",
                    m.clj_name
                )));
            }
            for a in &m.args {
                Type::parse(a).map_err(|e| {
                    BindgenError(format!(
                        "method `{}` arg type: {}",
                        m.clj_name, e.0
                    ))
                })?;
            }
            Type::parse(&m.returns).map_err(|e| {
                BindgenError(format!(
                    "method `{}` return type: {}",
                    m.clj_name, e.0
                ))
            })?;
        }
        Ok(())
    }
}
