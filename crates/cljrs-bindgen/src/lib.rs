//! cljrs-bindgen — manifest-driven generator for cljrs interop crates.
//!
//! Phase-2 of Rust interop. Given a TOML manifest describing a foreign
//! crate (functions, opaque handle types, methods on those handles), it
//! emits a small standalone Rust shim crate whose `pub fn install(env)`
//! registers each entry as a cljrs `Builtin`.
//!
//! Type whitelist (v1): `i64`, `f64`, `bool`, `string`, `opaque:Tag`,
//! `vec<i64>`, `vec<f64>`. Anything else is rejected at manifest-load
//! time with a clear error.

pub mod codegen;
pub mod manifest;

pub use codegen::generate_install_rs;
pub use manifest::{Manifest, Method, Function, Type};
