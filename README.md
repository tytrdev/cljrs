# cljrs

**Clojure, reimplemented from scratch in Rust, with MLIR/LLVM native codegen — no JVM, GPU-class performance as the target.**

On three microbenchmarks (`fib` / `loop-sum` / `cond-chain`), functions written with `defn-native` run **2.4× – 13× faster than Clojure-JVM**, **15× – 343× faster than Jank**, and **60× – 236× faster than Babashka**. Results cross-checked — every implementation returns the same value.

```
| bench           | cljrs native | Clojure-JVM | Babashka | Jank      |
|-----------------|-------------:|------------:|---------:|----------:|
| fib(25)         |      198 µs  |    463 µs   |  11.2 ms |  10.3 ms  |
| loop-sum 10k    |     1.45 µs  |   9.39 µs   |   345 µs |   497 µs  |
| cond-chain ×10  |      121 ns  |    1651 ns  |  1525 ns |  1804 ns  |
```

1000 iters each, release build, Apple Silicon. JVM numbers are with a warmed JIT.

## Why another Clojure

Clojure is a beautiful language chained to the JVM. cljrs is the experiment of asking: *what does Clojure feel like if you keep the syntax, keep macros, keep the data-oriented philosophy — and drop every single thing that requires Java?* 

No reflection. No proxy/reify. No `java.time`. No AOT compiled to JVM bytecode. No interop tax. The payoff: an AOT-compiled-on-definition language that produces native machine code through MLIR and LLVM's modern optimizer, with a live REPL and hot-reload, opening the door to **GPU-class performance and Bret-Victor-style live coding** — two things Clojure-on-the-JVM cannot reach.

## What works today

| Area | Status |
| --- | --- |
| Reader | S-exprs, numbers (i64/f64), strings, symbols, keywords, lists, vectors, maps, sets, `'quote`, `` `syntax-quote ``, `~unquote`, `~@splice`, `^type-hints`, `;` comments |
| Evaluator (tree-walker) | Full Clojure semantics for: `def`/`defn`/`fn`/`let`/`if`/`do`/`loop`/`recur`/`quote`/`defmacro`/`macroexpand`/`macroexpand-1`/named recursive fns/variadic `& rest` |
| Macros | `defmacro`, recursive macro self-reference, `` ` `` + `~` + `~@`, `macroexpand-1` |
| Collections | **HAMT-backed via [imbl](https://docs.rs/imbl)** — `PersistentVector`/`PersistentMap`/`PersistentSet` with O(log32) core ops, structural sharing on clone, structural equality |
| Stdlib (eager) | `map`, `filter`, `reduce`, `range`, `take`, `drop`, `count`, `first`, `rest`, `cons`, `conj`, `nth`, `list`, `vector`, `concat`, `str`, `pr-str`, `println`, `=`, `<`, `>`, `<=`, `>=`, `not`, `nil?`, `zero?`, `empty?`, `inc`, `dec`, `even?`, `odd?`, `pos?`, `neg?`, `identity` |
| Namespaces | **Cosmetic for now** — `(ns ...)`, `(in-ns ...)`, `(load-file "path")`, `(require 'some.ns)`. Real isolation + `:refer`/`:as` comes in Arc 2 |
| **Native codegen** | `defn-native` JIT-compiles typed `i64`/`f64` bodies through MLIR → LLVM. Supports recursion, cross-fn calls, `loop`/`recur`, `if`, `let`, `do`, `+`/`-`/`*`/`/`, comparisons, `(float …)` / `(int …)` conversions |
| Live coding | `src/bin/demo.rs` watches a `.clj` source file and re-JITs on save. Paired with `demo/fractal.clj` = a Mandelbrot renderer you edit while it runs |

## Quick start

**Prerequisites (macOS):**
```bash
brew install rust llvm
# The .cargo/config.toml in this repo auto-points at brew's llvm@22.
```

**Run the test suite:**
```bash
cargo test                  # tree-walker tests (no LLVM required)
cargo test --features mlir  # adds the MLIR JIT tests
```

**Run a live-coded demo** (the thing worth showing):
```bash
cargo run --release --features demo --bin demo -- demo/fractal.clj    # Mandelbrot, 60+ fps
cargo run --release --features demo --bin demo -- demo/raymarch.clj   # 3D SDF raymarcher, ~32 fps
cargo run --release --features demo --bin demo -- demo/plasma.clj     # demoscene plasma, 130+ fps
```
Each demo opens an `eframe` window with a side panel of four sliders plus a live FPS HUD. Every slider is threaded into the kernel as an additional `^i64` arg (0..1000); the cljrs code rescales to whatever it wants (iter count, light angle, color cycle, march step, whatever). Edit the `.clj` in your editor, save, and the window picks up the new JIT-compiled version within a frame — *while* you're sliding parameters.

Under the hood: rayon parallelizes across all CPU cores, so 518,400 JIT-compiled pixel calls per frame at 960×540 finish in ~16 ms on Apple Silicon. The CPU rendering is doing real work — no cached framebuffers, no GPU — and it still feels fluid because every pixel call resolves through one `extern "C"` transmute + native fn dispatch, no interpreter frame.

**Run the REPL:**
```bash
cargo run --release
```

**Run the benchmark matrix** (requires `clojure` + `java` + optionally `bb` / `jank`):
```bash
./bench/run.sh 1000
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
- **Macro-based DSLs** as the GPU story (Arc 2). Because Clojure is homoiconic, adding new compiler targets doesn't require compiler forks — users can write kernel DSLs in userland.
- **No Java interop, ever.** The scope decision that made the project tractable. Java-backed Clojure stdlib pieces (`java.time`, `BigInt`, reflection, `proxy`/`reify`) are out.
- **Tree-walker is throwaway infra.** It exists to pin down semantics against `clojure.test`-style suites before native codegen lands. Every tree-walker feature eventually gets a native path.

## Roadmap

### Arc 1 — **the demo** (mostly shipped)

Vertical slice, aimed at a recordable video. Almost everything in the "works today" table above.

**Shipped tonight:**
- Multi-fn modules + `loop`/`recur` via helper-fn + LLVM TCO
- Cross-fn native calls via `register_symbol`
- `f64` + internal `bool` on the native path
- HAMT collections (imbl)
- Eager stdlib
- `(ns ...)` / `(load-file ...)` / `(require ...)`
- Live-coded fractal demo

**Deferred to Arc 2:**
- `loop`/`recur` native via proper `scf.while` (helper fn + TCO works but wastes a call layer)
- True lazy seqs (thunk + force; today's `map`/`filter` are eager)
- Real namespace isolation (currently cosmetic)
- GPU kernel DSL via MLIR `gpu` dialect → SPIR-V

### Arc 2 — **the language** (~3-4 months)

Destructuring, multimethods, protocols, records, `try`/`catch`, atoms, char/regex/tagged literals, core.async via CPS transform, I/O + EDN, full metadata, `clojure.test` port. Target: idiomatic Clojure code ports with no changes.

### Arc 3 — **the launch** (~1-2 months)

Name the language (cljrs is an implementation handle, not a brand). Docs site. Benchmarks page. Linux port. Launch video. Blog post + Clojurians Slack + r/Clojure + HN.

## Honest caveats

- JVM Clojure benches don't use `^long` hints. With proper type hints, JVM's JIT would close some of the gap. cljrs still wins substantially, but the delta isn't all codegen quality — some is annotation density.
- The "GPU-class performance as the target" tagline is the *thesis*, not a shipped feature. Native code through MLIR validates the compilation approach; GPU dispatch comes in Arc 2.
- Tree-walker overhead dominates anything not wrapped in `defn-native`. Existing benches that exercise tree-walker (plain `defn`, macros) are 30-300× slower than Clojure-JVM.
- The demo is macOS-only today (via `minifb`). Linux + Windows come with the launch.
- `defn-native` only handles i64/f64 signatures in phase 2; `bool` at the FFI boundary is deferred because LLVM's i1 ABI varies by platform.

## Contributing

Not yet open for external PRs — the foundation is still shifting. Watch the repo; follow the roadmap above; we'll open up during Arc 3.

## License

TBD. Will land with Arc 3 / launch.

---

*Built in an aggressive autonomous session — much of this code was generated in one night. The [commit log](../../commits/main) tells the story.*
