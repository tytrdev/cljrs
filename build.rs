//! Emits an rpath to the brew-installed LLVM 22 lib directory when the
//! `mlir` feature is enabled, so `libMLIR.dylib` loads at runtime without
//! requiring DYLD_LIBRARY_PATH (which SIP strips from child processes).

fn main() {
    if std::env::var_os("CARGO_FEATURE_MLIR").is_none() {
        return;
    }

    let prefix = std::env::var("MLIR_SYS_220_PREFIX")
        .unwrap_or_else(|_| "/opt/homebrew/opt/llvm".to_string());

    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{prefix}/lib");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{prefix}/lib");
    }

    println!("cargo:rerun-if-env-changed=MLIR_SYS_220_PREFIX");
}
