//! Smoke-test the cljrs source the docs/ml.html page boots with. Catches
//! breakage from missing builtins / macros without rebuilding wasm.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

const PAGE_SRC: &str = r#"
(require '[cljrs.ml :as ml])

(def HIDDEN-SIZE   24)
(def LEARNING-RATE 0.05)
(def BATCH         32)
(def N-DATA        80)
(def NOISE-AMP     0.10)
(def X-MIN        -3.0)
(def X-MAX         3.0)
(def FREQ          2.0)

(defonce RNG (atom 1234567))
(defn rng-next! []
  (let [s (mod (+ (* @RNG 1103515245) 12345) 2147483648)]
    (reset! RNG s)
    s))
(defn rng-float! [] (/ (* 1.0 (rng-next!)) 2147483648.0))
(defn rng-int! [n] (mod (rng-next!) n))

(defn target-fn [x] (sin (* FREQ x)))

(defn make-data [n]
  (let [xs (vec (for [i (range n)]
                  (+ X-MIN (* (- X-MAX X-MIN)
                              (/ (* 1.0 i) (max 1 (dec n)))))))
        ys (mapv (fn [x]
                   (+ (target-fn x)
                      (* NOISE-AMP (- (rng-float!) 0.5))))
                 xs)]
    {:xs xs :ys ys}))

(defonce DATA (atom (make-data N-DATA)))
(when (not (= (count (:xs @DATA)) N-DATA))
  (reset! DATA (make-data N-DATA)))

(defonce W1 (atom (ml/randn 1 HIDDEN-SIZE 0.8)))
(defonce B1 (atom (ml/zeros 1 HIDDEN-SIZE)))
(defonce W2 (atom (ml/randn HIDDEN-SIZE 1 (/ 1.0 (sqrt HIDDEN-SIZE)))))
(defonce B2 (atom (ml/zeros 1 1)))

(defn fix-shapes! []
  (when (not (= (second (ml/shape @W1)) HIDDEN-SIZE))
    (reset! W1 (ml/randn 1 HIDDEN-SIZE 0.8))
    (reset! B1 (ml/zeros 1 HIDDEN-SIZE))
    (reset! W2 (ml/randn HIDDEN-SIZE 1 (/ 1.0 (sqrt HIDDEN-SIZE))))
    (reset! B2 (ml/zeros 1 1))))
(fix-shapes!)

(defonce X-BUF (atom (ml/zeros BATCH 1)))
(defonce Y-BUF (atom (ml/zeros BATCH 1)))
(when (not (= (first (ml/shape @X-BUF)) BATCH))
  (reset! X-BUF (ml/zeros BATCH 1))
  (reset! Y-BUF (ml/zeros BATCH 1)))

(defn forward [x-tensor]
  (let [h (ml/tanh (ml/add-bias (ml/matmul x-tensor @W1) @B1))]
    (ml/add-bias (ml/matmul h @W2) @B2)))

(defn sample-batch! []
  (let [d  @DATA
        xs (:xs d)
        ys (:ys d)
        n  (count xs)
        idxs (vec (for [_ (range BATCH)] (rng-int! n)))
        xb (mapv #(nth xs %) idxs)
        yb (mapv #(nth ys %) idxs)]
    (ml/set-data! @X-BUF xb)
    (ml/set-data! @Y-BUF yb)))

(defn train-step! []
  (sample-batch!)
  (let [pred (forward @X-BUF)
        loss (ml/mse pred @Y-BUF)]
    (ml/backward! loss)
    (ml/sgd-step! [@W1 @B1 @W2 @B2] LEARNING-RATE)
    [(ml/scalar loss)]))

(def GRID-N 80)
(defn predict-grid []
  (let [xs (vec (for [i (range GRID-N)]
                  (+ X-MIN (* (- X-MAX X-MIN)
                              (/ (* 1.0 i) (max 1 (dec GRID-N)))))))
        xt (ml/tensor GRID-N 1 xs)
        ys (ml/tolist (forward xt))]
    (vec (interleave xs ys))))

(defn data-points []
  (let [d @DATA]
    (vec (interleave (:xs d) (:ys d)))))
"#;

#[test]
fn page_source_loads_and_trains() {
    let env = Env::new();
    builtins::install(&env);
    cljrs_ml::install(&env);
    let forms = reader::read_all(PAGE_SRC).expect("page source must parse");
    for f in forms {
        eval::eval(&f, &env)
            .unwrap_or_else(|e| panic!("page source eval failed at form `{f}`: {e}"));
    }
    // Initial loss
    let initial = eval::eval(
        &reader::read_all("(do (sample-batch!) (ml/scalar (ml/mse (forward @X-BUF) @Y-BUF)))")
            .unwrap()[0], &env).unwrap();
    let initial_loss = match initial { Value::Float(f) => f, other => panic!("{other:?}") };

    // Run 200 steps.
    for _ in 0..200 {
        eval::eval(&reader::read_all("(train-step!)").unwrap()[0], &env).unwrap();
    }
    let final_v = eval::eval(
        &reader::read_all("(do (sample-batch!) (ml/scalar (ml/mse (forward @X-BUF) @Y-BUF)))")
            .unwrap()[0], &env).unwrap();
    let final_loss = match final_v { Value::Float(f) => f, other => panic!("{other:?}") };
    assert!(final_loss < initial_loss * 0.7,
        "training didn't reduce loss enough: {initial_loss} -> {final_loss}");

    // predict-grid + data-points should produce flat numeric vectors.
    for fn_call in &["(predict-grid)", "(data-points)"] {
        let v = eval::eval(&reader::read_all(fn_call).unwrap()[0], &env).unwrap();
        match v {
            Value::Vector(xs) => assert!(xs.len() >= 2, "{fn_call}: empty"),
            other => panic!("{fn_call}: expected vector, got {other:?}"),
        }
    }
}

// --- Smoke tests for the new showcases. They share interface
// `(train-step!) -> [loss]` and `(viz) -> flat float vector`. We
// don't reproduce the full HTML programs here — that would drag in
// hundreds of lines of stroke specs etc. Instead each test exercises
// a minimal stand-in that uses the *same builtins* the showcase
// relies on (softmax, cross-entropy, adam, conv1d-valid, sigmoid),
// catching any regression that breaks the per-tab program.

const MOONS_MIN: &str = r#"
(require '[cljrs.ml :as ml])
(def H 8)
(defonce W1 (atom (ml/kaiming 2 H)))
(defonce B1 (atom (ml/zeros 1 H)))
(defonce W2 (atom (ml/xavier H 2)))
(defonce B2 (atom (ml/zeros 1 2)))
(def X (ml/tensor [[0.0 0.0] [1.0 1.0] [0.5 -0.3] [-0.2 0.8]]))
(def Y (ml/one-hot [0 1 0 1] 2))
(defn forward [x]
  (let [h (ml/relu (ml/add-bias (ml/matmul x @W1) @B1))]
    (ml/softmax (ml/add-bias (ml/matmul h @W2) @B2))))
(defn train-step! []
  (let [p (forward X)
        l (ml/cross-entropy p Y)]
    (ml/backward! l)
    (ml/adam-step! [@W1 @B1 @W2 @B2] 0.05)
    [(ml/scalar l)]))
(defn viz [] (ml/tolist (forward X)))
"#;

const AE_MIN: &str = r#"
(require '[cljrs.ml :as ml])
(def L 4)
(defonce W1 (atom (ml/kaiming 8 L)))
(defonce B1 (atom (ml/zeros 1 L)))
(defonce W2 (atom (ml/xavier L 8)))
(defonce B2 (atom (ml/zeros 1 8)))
(def X (ml/tensor [[1 0 1 0 1 0 1 0]
                   [0 1 0 1 0 1 0 1]]))
(defn forward [x]
  (let [h (ml/sigmoid (ml/add-bias (ml/matmul x @W1) @B1))]
    (ml/sigmoid (ml/add-bias (ml/matmul h @W2) @B2))))
(defn train-step! []
  (let [p (forward X)
        l (ml/mse p X)]
    (ml/backward! l)
    (ml/adam-step! [@W1 @B1 @W2 @B2] 0.02)
    [(ml/scalar l)]))
(defn viz [] (ml/tolist (forward X)))
"#;

fn fresh_env() -> Env {
    let env = Env::new();
    builtins::install(&env);
    cljrs_ml::install(&env);
    env
}

fn eval_all(env: &Env, src: &str) {
    for f in reader::read_all(src).expect("read") {
        eval::eval(&f, env).unwrap_or_else(|e| panic!("eval `{f}`: {e}"));
    }
}

fn loss_after_n_steps(env: &Env, n: usize) -> (f64, f64) {
    let initial = match eval::eval(
        &reader::read_all("(first (train-step!))").unwrap()[0], env).unwrap() {
        Value::Float(f) => f, v => panic!("{v:?}"),
    };
    let mut last = initial;
    for _ in 0..n {
        let v = eval::eval(
            &reader::read_all("(first (train-step!))").unwrap()[0], env).unwrap();
        if let Value::Float(f) = v { last = f; }
    }
    (initial, last)
}

#[test]
fn moons_showcase_min_trains() {
    let env = fresh_env();
    eval_all(&env, MOONS_MIN);
    let (i, f) = loss_after_n_steps(&env, 200);
    assert!(f < i * 0.5, "moons-min loss didn't drop: {i} -> {f}");
    let v = eval::eval(&reader::read_all("(viz)").unwrap()[0], &env).unwrap();
    assert!(matches!(v, Value::Vector(_)));
}

#[test]
fn autoencoder_min_trains() {
    let env = fresh_env();
    eval_all(&env, AE_MIN);
    let (i, f) = loss_after_n_steps(&env, 300);
    assert!(f < i * 0.6, "AE-min loss didn't drop: {i} -> {f}");
    let v = eval::eval(&reader::read_all("(viz)").unwrap()[0], &env).unwrap();
    assert!(matches!(v, Value::Vector(_)));
}

#[test]
fn conv1d_in_repl() {
    // conv1d-valid + autograd via cljrs.ml.
    let env = fresh_env();
    eval_all(&env, r#"
        (require '[cljrs.ml :as ml])
        (def x (ml/tensor [1 2 3 4 5]))
        (def k (ml/param  [1 0 -1]))
        (def y (ml/conv1d-valid x k))
        (def t (ml/tensor [0 0 0]))
        (def l (ml/mse y t))
        (ml/backward! l)
        (ml/adam-step! [k] 0.1)
    "#);
    // sanity: viz returns a vector
    let v = eval::eval(&reader::read_all("(ml/tolist k)").unwrap()[0], &env).unwrap();
    assert!(matches!(v, Value::Vector(_)));
}
