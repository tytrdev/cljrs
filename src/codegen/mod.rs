//! Native codegen backends for cljrs. Currently houses the MLIR path
//! (feature-gated); future backends or shared IR can live here too.

#[cfg(feature = "mlir")]
pub mod mlir;
