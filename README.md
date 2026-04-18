# cljrs

**Clojure, reimplemented from scratch in Rust, with MLIR/LLVM native codegen and a GPU compute path — no JVM.**

On three microbenchmarks (`fib` / `loop-sum` / `cond-chain`), functions written with `defn-native` run **2.4× – 13× faster than Clojure-JVM**, **15× – 343× faster than Jank**, and **60× – 236× faster than Babashka**. Every implementation returns the same value.

```
| bench           | cljrs native | Clojure-JVM | Babashka | Jank      |
|-----------------|-------------:|------------:|---------:|----------:|
| fib(25)         |      198 µs  |    463 µs   |  11.2 ms |  10.3 ms  |
| loop-sum 10k    |     1.45 µs  |   9.39 µs   |   345 µs |   497 µs  |
| cond-chain ×10  |      121 ns  |    1651 ns  |  1525 ns |  1804 ns  |
```

1000 iters each, release build, Apple Silicon. JVM numbers are with a warmed JIT.

The docs site (live REPL in your browser, GPU kernels, demo videos, full coverage table) is at **<https://tytrdev.github.io/cljrs/>**.

## Why another Clojure

Clojure is a beautiful language chained to the JVM. cljrs is the experiment of asking: *what does Clojure feel like if you keep the syntax, keep macros, keep the data-oriented philosophy — and drop every single thing that requires Java?*

No reflection. No proxy/reify. No `java.time`. No interop tax. The payoff: an AOT-compiled-on-definition language that produces native machine code through MLIR and LLVM's modern optimizer, with a live REPL and hot-reload, and a GPU compute path that runs the same source on Metal, Vulkan, DX12, or WebGPU.

## What works today

One language, three compilers — same surface syntax across all of them.

| Path | What it does |
| --- | --- |
| **Tree walker** | Full Clojure semantics. Reader, macros, persistent collections (HAMT via [imbl](https://docs.rs/imbl)), destructuring, multimethods, protocols, records, real namespaces with `:as` / `:refer`, lazy seqs, transducers, atoms, regex, ratios, `try`/`catch`. ~125 builtins + prelude macros. See the [coverage page](https://tytrdev.github.io/cljrs/coverage.html). |
| **Native (MLIR + LLVM)** | `defn-native` JIT-compiles type-hinted (`^i64` / `^f64`) bodies to native code. Recursion, cross-fn calls, `loop`/`recur`, `if`, `let`, `do`, arithmetic, comparisons. Live-reloads on save. |
| **GPU (WGSL)** | `defn-gpu` lowers kernels to WGSL via the same compiler frontend; runs on Metal / Vulkan / DX12 natively, and on WebGPU in the browser through a WASM build of the runtime. |

## Quick start

**Prerequisites (macOS):**
```bash
brew install rust llvm
# .cargo/config.toml in this repo auto-points at brew's llvm@22.
```

**Run the test suite (200+ tests):**
```bash
cargo test                  # tree-walker tests (no LLVM required)
cargo test --features mlir  # adds the MLIR JIT tests
```

**Run a live-coded demo:**
```bash
cargo run --release --features demo --bin demo -- demo/fractal.clj    # Mandelbrot, 60+ fps
cargo run --release --features demo --bin demo -- demo/raymarch.clj   # 3D SDF raymarcher
cargo run --release --features demo --bin demo -- demo/plasma.clj     # demoscene plasma, 130+ fps
```

Each demo opens an `eframe` window with four sliders + a live FPS HUD. Each slider is threaded into the kernel as an `^i64` arg. Edit the `.clj` in your editor, save, and the window picks up the new JIT-compiled version within a frame — *while* you're sliding parameters. There are 21 CPU demos in `demo/` and 7 GPU demos in `demo_gpu/`.

**Run the REPL:**
```bash
cargo run --release
```

**Run the benchmark matrix** (requires `clojure` + `java` + optionally `bb` / `jank`):
```bash
./bench/run.sh 1000
```

**Build the docs site locally:**
```bash
./docs/build.sh                       # rebuild WASM + WGSL artifacts
cd docs && python3 -m http.server 8080
```

## Example: the perf story in 10 lines

```clojure
;; Idiomatic Clojure — runs natively at 1.45µs per call
;; (6.5× faster than Clojure-JVM)
(defn-native sum-to ^i64 [^i64 n]
  (loop [i 0 acc 0]
    (if (> i n) acc (recur (+ i 1) (+ acc i)))))

(sum-to 10000) ;=> 50005000

;; Cross-fn calls JIT'd + linked — ~12ns of call overhead each
(defn-native square ^i64 [^i64 n] (* n n))

(defn-native sum-of-squares ^i64 [^i64 n]
  (if (= n 0) 0 (+ (square n) (sum-of-squares (- n 1)))))

(sum-of-squares 10) ;=> 385
```

`defn-native` is opt-in: every param and the return type needs a `^` hint. Without hints, `defn` still works on the tree-walker path with full Clojure dynamism.

## Architecture decisions

- **Rust implementation, MLIR + LLVM 22 codegen** via [melior](https://docs.rs/melior). Pivoted from plain LLVM because GPU is in scope — MLIR is the heterogeneous CPU+GPU backbone (Mojo's secret, Triton's foundation).
- **Incremental AOT** (SBCL / Chez Scheme tradition): each `defn-native` compiles to native code on definition; the live REPL gets native speed without JIT warmup.
- **Macro-based DSLs as the GPU story.** Because Clojure is homoiconic, adding new compiler targets doesn't require compiler forks — kernel DSLs live in userland and reuse the same reader, macroexpander, and analyzer.
- **No Java interop, ever.** The scope decision that made the project tractable. Java-backed Clojure stdlib pieces (`java.time`, `BigInt`, reflection, `proxy`/`reify`) are out.
- **Tree-walker as semantic spec.** It exists to pin down semantics against `clojure.test`-style suites before native codegen lands. Hot paths get `defn-native`; everything else stays on the walker.

## Status

The current focus is coverage and ergonomics. Standing gaps tracked on the [coverage page](https://tytrdev.github.io/cljrs/coverage.html): `deftype`, bigint, tagged literals, agents/futures/promises/refs, dynamic vars, the reducers library, `clojure.test` port, EDN reader/writer.

## Honest caveats

- JVM Clojure benches don't use `^long` hints. With proper type hints, JVM's JIT would close some of the gap. cljrs still wins substantially, but the delta isn't all codegen quality — some is annotation density.
- Tree-walker overhead dominates anything not wrapped in `defn-native`. Existing benches that exercise the walker (plain `defn`, macros) are 30-300× slower than Clojure-JVM. That's the cost of dynamism — the walker is what `defn-native` opts you out of.
- The native demos are macOS-first today (via `eframe` + Apple Silicon perf numbers). Linux + Windows builds work but aren't the primary target yet.
- `defn-native` only handles `i64`/`f64` signatures; `bool` at the FFI boundary is deferred because LLVM's i1 ABI varies by platform.

## Contributing

Not yet open for external PRs — the foundation is still shifting. Issues and discussion are welcome.

## License

MIT. See [LICENSE](LICENSE).
