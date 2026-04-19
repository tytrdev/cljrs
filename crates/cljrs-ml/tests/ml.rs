//! End-to-end: drive the cljrs.ml builtins from Clojure source and
//! verify autograd / SGD do something sensible. Plus a few Rust-side
//! unit tests on the autograd module directly.

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use cljrs_ml::autograd::{self, Shape, Tensor};

fn fresh_env() -> Env {
    let env = Env::new();
    builtins::install(&env);
    cljrs_ml::install(&env);
    env
}

fn run(env: &Env, src: &str) -> Value {
    let forms = reader::read_all(src).expect("read");
    let mut last = Value::Nil;
    for f in forms {
        last = eval::eval(&f, env).expect(&format!("eval: {f}"));
    }
    last
}

fn as_floats(v: &Value) -> Vec<f64> {
    match v {
        Value::Vector(xs) => xs.iter().map(|x| match x {
            Value::Float(f) => *f,
            Value::Int(i) => *i as f64,
            _ => panic!("non-numeric: {x:?}"),
        }).collect(),
        _ => panic!("not a vector: {v:?}"),
    }
}

// ---------------- cljrs-side smoke tests ---------------------------------

#[test]
fn tensor_creation_and_shape() {
    let env = fresh_env();
    let v = run(&env, r#"
        (require '[cljrs.ml :as ml])
        (def t (ml/tensor [[1 2 3] [4 5 6]]))
        (ml/shape t)
    "#);
    let xs = as_floats(&v);
    assert_eq!(xs, vec![2.0, 3.0]);
}

#[test]
fn matmul_shape() {
    let env = fresh_env();
    let v = run(&env, r#"
        (require '[cljrs.ml :as ml])
        (def a (ml/tensor 2 3 [1 2 3 4 5 6]))
        (def b (ml/tensor 3 4 [1 0 0 0 0 1 0 0 0 0 1 0]))
        (ml/shape (ml/matmul a b))
    "#);
    assert_eq!(as_floats(&v), vec![2.0, 4.0]);
}

#[test]
fn relu_zeros_negatives() {
    let env = fresh_env();
    let v = run(&env, r#"
        (require '[cljrs.ml :as ml])
        (ml/tolist (ml/relu (ml/tensor [[-1 2 -3 4]])))
    "#);
    assert_eq!(as_floats(&v), vec![0.0, 2.0, 0.0, 4.0]);
}

#[test]
fn backward_gradient_sign_trivial() {
    // f(x) = (x - 2)^2.  At x=5, dL/dx = 2*(5-2) = 6 > 0.
    let env = fresh_env();
    let v = run(&env, r#"
        (require '[cljrs.ml :as ml])
        (def x      (ml/param  [[5.0]]))
        (def target (ml/tensor [[2.0]]))
        (def loss   (ml/mse x target))
        (ml/backward! loss)
        ;; pull gradient through one tiny SGD step then look at new x
        (ml/sgd-step! [x] 0.1)
        (ml/tolist x)
    "#);
    let xs = as_floats(&v);
    // x_new = 5 - 0.1 * 6 = 4.4
    assert!((xs[0] - 4.4).abs() < 1e-5, "x after step = {}", xs[0]);
}

#[test]
fn sgd_step_lowers_loss_on_linear_fit() {
    let env = fresh_env();
    let v = run(&env, r#"
        (require '[cljrs.ml :as ml])
        ;; fit y = 3x with one parameter.
        (def w (ml/param [[0.0]]))
        (def xs (ml/tensor [[1.0] [2.0] [3.0] [4.0]]))
        (def ys (ml/tensor [[3.0] [6.0] [9.0] [12.0]]))
        (defn pred [] (ml/matmul xs w))
        (defn loss-of [] (ml/mse (pred) ys))
        (def initial-loss (ml/scalar (loss-of)))
        (loop [i 0]
          (when (< i 200)
            (let [l (loss-of)]
              (ml/backward! l)
              (ml/sgd-step! [w] 0.02))
            (recur (inc i))))
        [initial-loss (ml/scalar (loss-of)) (first (ml/tolist w))]
    "#);
    let xs = as_floats(&v);
    let initial = xs[0];
    let final_loss = xs[1];
    let w_final = xs[2];
    assert!(final_loss < initial * 0.01,
        "loss didn't drop enough: {initial} -> {final_loss}");
    assert!((w_final - 3.0).abs() < 0.05,
        "w should approach 3.0, got {w_final}");
}

// ---------------- autograd unit tests ------------------------------------

#[test]
fn matmul_numerical_correctness() {
    let a = Tensor::leaf(Shape::new(2, 3), vec![1., 2., 3., 4., 5., 6.], false);
    let b = Tensor::leaf(Shape::new(3, 2), vec![7., 8., 9., 10., 11., 12.], false);
    let c = autograd::matmul(&a, &b);
    let d = c.data();
    // [[58 64] [139 154]]
    assert_eq!(*d, vec![58., 64., 139., 154.]);
}

#[test]
fn mse_gradient_matches_analytic() {
    let p = Tensor::leaf(Shape::new(1, 3), vec![1.0, 2.0, 3.0], true);
    let t = Tensor::leaf(Shape::new(1, 3), vec![0.0, 0.0, 0.0], false);
    let l = autograd::mse(&p, &t);
    autograd::backward(&l);
    // dL/dp_i = (2/N)*(p_i - t_i), N=3. So [2/3, 4/3, 6/3].
    let g = p.grad();
    assert!((g[0] - 2.0/3.0).abs() < 1e-6);
    assert!((g[1] - 4.0/3.0).abs() < 1e-6);
    assert!((g[2] - 6.0/3.0).abs() < 1e-6);
}

#[test]
fn mlp_one_step_lowers_loss() {
    // Trivially: 1 -> 4 -> 1 MLP, fit y=2x on 4 inputs.
    let xs = Tensor::leaf(Shape::new(4, 1), vec![-1., -0.5, 0.5, 1.], false);
    let ys = Tensor::leaf(Shape::new(4, 1), vec![-2., -1., 1., 2.], false);
    let w1 = Tensor::leaf(Shape::new(1, 4), vec![0.3, -0.2, 0.5, 0.1], true);
    let b1 = Tensor::leaf(Shape::new(1, 4), vec![0.0, 0.0, 0.0, 0.0], true);
    let w2 = Tensor::leaf(Shape::new(4, 1), vec![0.1, 0.2, -0.1, 0.3], true);
    let b2 = Tensor::leaf(Shape::new(1, 1), vec![0.0], true);

    let initial_loss = {
        let h = autograd::add_bias(&autograd::matmul(&xs, &w1), &b1);
        let h = autograd::tanh(&h);
        let p = autograd::add_bias(&autograd::matmul(&h, &w2), &b2);
        let l = autograd::mse(&p, &ys);
        let v = l.data()[0];
        v
    };

    for _ in 0..200 {
        let h = autograd::add_bias(&autograd::matmul(&xs, &w1), &b1);
        let h = autograd::tanh(&h);
        let p = autograd::add_bias(&autograd::matmul(&h, &w2), &b2);
        let l = autograd::mse(&p, &ys);
        autograd::backward(&l);
        autograd::sgd_step(&[w1.clone(), b1.clone(), w2.clone(), b2.clone()], 0.05);
    }

    let final_loss = {
        let h = autograd::add_bias(&autograd::matmul(&xs, &w1), &b1);
        let h = autograd::tanh(&h);
        let p = autograd::add_bias(&autograd::matmul(&h, &w2), &b2);
        let l = autograd::mse(&p, &ys);
        l.data()[0]
    };

    assert!(final_loss < initial_loss * 0.5,
        "MLP loss didn't drop: {initial_loss} -> {final_loss}");
}
