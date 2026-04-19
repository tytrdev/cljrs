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
