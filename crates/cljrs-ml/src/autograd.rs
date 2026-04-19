//! Tiny reverse-mode autograd over dense f32 tensors.
//!
//! Design: every `Tensor` is an `Arc<TensorInner>`. `TensorInner` holds
//! shape, value buffer, gradient buffer (interior-mutable), an `Op` tag
//! describing how it was produced, and the parent tensors it depends
//! on. `backward()` walks the graph from a scalar loss in reverse
//! topological order and accumulates `.grad` buffers.
//!
//! Scope: enough for a single hidden-layer MLP regression demo. Shapes
//! are 1-D vectors or 2-D matrices stored row-major.

use std::cell::RefCell;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Leaf,        // weights / biases / inputs / targets
    MatMul,      // a (m×k) · b (k×n) = (m×n)
    AddBias,     // a (m×n) + b (1×n)  (broadcast on rows)
    Add,         // a + b, same shape
    Sub,         // a - b, same shape
    Relu,
    MseMean,     // scalar = mean((pred - target)^2)
    MaeMean,     // scalar = mean(|pred - target|)
    Tanh,
    Sigmoid,
    Gelu,
    SoftmaxRow,  // row-wise softmax (m×n) -> (m×n)
    Conv1DValid, // 1-D valid conv: input (1, L) * kernel (1, K) = (1, L-K+1)
    /// Cross-entropy from softmax-prob inputs against one-hot targets.
    /// scalar = -mean(sum(target * log(pred + eps)))
    CrossEntropy,
}

/// Shape is `(rows, cols)`. A 1-D vector is `(1, n)` for matrix
/// purposes, but we keep a flag so display etc. can be sensible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub rows: usize,
    pub cols: usize,
}

impl Shape {
    pub fn new(rows: usize, cols: usize) -> Self {
        Shape { rows, cols }
    }
    pub fn numel(&self) -> usize {
        self.rows * self.cols
    }
}

pub struct TensorInner {
    pub shape: Shape,
    pub data: RefCell<Vec<f32>>,
    pub grad: RefCell<Vec<f32>>,
    pub op: Op,
    pub parents: Vec<Tensor>,
    /// True if this tensor is a learnable parameter (gradients should
    /// be flushed and updated by `sgd_step`). Leaf inputs/targets set
    /// this to false.
    pub is_param: bool,
}

#[derive(Clone)]
pub struct Tensor(pub Arc<TensorInner>);

// SAFETY: RefCell is !Sync, but we never share Tensors across threads
// in this demo path — all autograd work happens on the host thread
// driving the cljrs eval loop. Marker impls satisfy `Value::Opaque`'s
// `Send + Sync` bound. Misuse from another thread would be a logic
// error caught by RefCell at runtime via a borrow-fail panic.
unsafe impl Send for TensorInner {}
unsafe impl Sync for TensorInner {}

impl Tensor {
    pub fn leaf(shape: Shape, data: Vec<f32>, is_param: bool) -> Tensor {
        assert_eq!(data.len(), shape.numel(), "leaf data size mismatch");
        Tensor(Arc::new(TensorInner {
            shape,
            data: RefCell::new(data),
            grad: RefCell::new(vec![0.0; shape.numel()]),
            op: Op::Leaf,
            parents: Vec::new(),
            is_param,
        }))
    }

    fn from_op(shape: Shape, data: Vec<f32>, op: Op, parents: Vec<Tensor>) -> Tensor {
        debug_assert_eq!(data.len(), shape.numel());
        Tensor(Arc::new(TensorInner {
            shape,
            data: RefCell::new(data),
            grad: RefCell::new(vec![0.0; shape.numel()]),
            op,
            parents,
            is_param: false,
        }))
    }

    pub fn shape(&self) -> Shape {
        self.0.shape
    }

    pub fn data(&self) -> std::cell::Ref<'_, Vec<f32>> {
        self.0.data.borrow()
    }

    pub fn grad(&self) -> std::cell::Ref<'_, Vec<f32>> {
        self.0.grad.borrow()
    }

    pub fn ptr_eq(&self, other: &Tensor) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

// ---------------- forward ops --------------------------------------------

pub fn matmul(a: &Tensor, b: &Tensor) -> Tensor {
    let (m, k) = (a.0.shape.rows, a.0.shape.cols);
    let (k2, n) = (b.0.shape.rows, b.0.shape.cols);
    assert_eq!(k, k2, "matmul: inner dims must match ({m}x{k}) · ({k2}x{n})");
    let av = a.0.data.borrow();
    let bv = b.0.data.borrow();
    let mut out = vec![0.0f32; m * n];
    for i in 0..m {
        for kk in 0..k {
            let aik = av[i * k + kk];
            if aik == 0.0 { continue; }
            for j in 0..n {
                out[i * n + j] += aik * bv[kk * n + j];
            }
        }
    }
    Tensor::from_op(Shape::new(m, n), out, Op::MatMul, vec![a.clone(), b.clone()])
}

pub fn add_bias(a: &Tensor, bias: &Tensor) -> Tensor {
    let (m, n) = (a.0.shape.rows, a.0.shape.cols);
    assert_eq!(bias.0.shape.rows, 1);
    assert_eq!(bias.0.shape.cols, n);
    let av = a.0.data.borrow();
    let bv = bias.0.data.borrow();
    let mut out = vec![0.0; m * n];
    for i in 0..m {
        for j in 0..n {
            out[i * n + j] = av[i * n + j] + bv[j];
        }
    }
    Tensor::from_op(Shape::new(m, n), out, Op::AddBias, vec![a.clone(), bias.clone()])
}

pub fn add(a: &Tensor, b: &Tensor) -> Tensor {
    assert_eq!(a.0.shape, b.0.shape);
    let av = a.0.data.borrow();
    let bv = b.0.data.borrow();
    let out = av.iter().zip(bv.iter()).map(|(x, y)| x + y).collect();
    Tensor::from_op(a.0.shape, out, Op::Add, vec![a.clone(), b.clone()])
}

pub fn sub(a: &Tensor, b: &Tensor) -> Tensor {
    assert_eq!(a.0.shape, b.0.shape);
    let av = a.0.data.borrow();
    let bv = b.0.data.borrow();
    let out = av.iter().zip(bv.iter()).map(|(x, y)| x - y).collect();
    Tensor::from_op(a.0.shape, out, Op::Sub, vec![a.clone(), b.clone()])
}

pub fn relu(a: &Tensor) -> Tensor {
    let av = a.0.data.borrow();
    let out = av.iter().map(|x| if *x > 0.0 { *x } else { 0.0 }).collect();
    Tensor::from_op(a.0.shape, out, Op::Relu, vec![a.clone()])
}

pub fn tanh(a: &Tensor) -> Tensor {
    let av = a.0.data.borrow();
    let out = av.iter().map(|x| x.tanh()).collect();
    Tensor::from_op(a.0.shape, out, Op::Tanh, vec![a.clone()])
}

pub fn sigmoid(a: &Tensor) -> Tensor {
    let av = a.0.data.borrow();
    let out: Vec<f32> = av.iter().map(|x| 1.0 / (1.0 + (-x).exp())).collect();
    Tensor::from_op(a.0.shape, out, Op::Sigmoid, vec![a.clone()])
}

/// GELU approximation (tanh-based, matches PyTorch's `approximate='tanh'`).
/// Backward uses a numerical surrogate: dGELU/dx ≈ analytic for tanh form.
pub fn gelu(a: &Tensor) -> Tensor {
    let av = a.0.data.borrow();
    let c = (2.0_f32 / std::f32::consts::PI).sqrt();
    let out: Vec<f32> = av.iter().map(|x| {
        let inner = c * (x + 0.044715 * x * x * x);
        0.5 * x * (1.0 + inner.tanh())
    }).collect();
    Tensor::from_op(a.0.shape, out, Op::Gelu, vec![a.clone()])
}

/// Row-wise softmax. Numerically stabilized by subtracting per-row max.
pub fn softmax(a: &Tensor) -> Tensor {
    let (m, n) = (a.0.shape.rows, a.0.shape.cols);
    let av = a.0.data.borrow();
    let mut out = vec![0.0f32; m * n];
    for i in 0..m {
        let row = &av[i * n..(i + 1) * n];
        let mx = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mut s = 0.0;
        for j in 0..n {
            let e = (row[j] - mx).exp();
            out[i * n + j] = e;
            s += e;
        }
        let inv = 1.0 / s.max(1e-30);
        for j in 0..n { out[i * n + j] *= inv; }
    }
    Tensor::from_op(a.0.shape, out, Op::SoftmaxRow, vec![a.clone()])
}

/// 1-D valid convolution. Input must be a (1, L) tensor, kernel a
/// (1, K) tensor. Output is (1, L-K+1). Single-channel only — meant
/// for the demo, not for general workloads.
pub fn conv1d_valid(input: &Tensor, kernel: &Tensor) -> Tensor {
    assert_eq!(input.0.shape.rows, 1);
    assert_eq!(kernel.0.shape.rows, 1);
    let l = input.0.shape.cols;
    let k = kernel.0.shape.cols;
    assert!(l >= k, "conv1d-valid: input length {l} < kernel {k}");
    let out_len = l - k + 1;
    let iv = input.0.data.borrow();
    let kv = kernel.0.data.borrow();
    let mut out = vec![0.0f32; out_len];
    for i in 0..out_len {
        let mut s = 0.0;
        for j in 0..k { s += iv[i + j] * kv[j]; }
        out[i] = s;
    }
    Tensor::from_op(Shape::new(1, out_len), out, Op::Conv1DValid,
        vec![input.clone(), kernel.clone()])
}

pub fn mae(pred: &Tensor, target: &Tensor) -> Tensor {
    assert_eq!(pred.0.shape, target.0.shape);
    let pv = pred.0.data.borrow();
    let tv = target.0.data.borrow();
    let n = pv.len() as f32;
    let s: f32 = pv.iter().zip(tv.iter()).map(|(p, t)| (p - t).abs()).sum();
    let out = vec![s / n.max(1.0)];
    Tensor::from_op(Shape::new(1, 1), out, Op::MaeMean,
        vec![pred.clone(), target.clone()])
}

/// Cross-entropy where `pred` is already a row-wise probability
/// distribution (softmax output) and `target` is a one-hot row-wise
/// distribution. Returns a scalar mean over rows.
pub fn cross_entropy(pred: &Tensor, target: &Tensor) -> Tensor {
    assert_eq!(pred.0.shape, target.0.shape);
    let pv = pred.0.data.borrow();
    let tv = target.0.data.borrow();
    let m = pred.0.shape.rows as f32;
    let mut s = 0.0;
    for (p, t) in pv.iter().zip(tv.iter()) {
        if *t > 0.0 {
            s -= t * (p.max(1e-12)).ln();
        }
    }
    let out = vec![s / m.max(1.0)];
    Tensor::from_op(Shape::new(1, 1), out, Op::CrossEntropy,
        vec![pred.clone(), target.clone()])
}

pub fn mse(pred: &Tensor, target: &Tensor) -> Tensor {
    assert_eq!(pred.0.shape, target.0.shape);
    let pv = pred.0.data.borrow();
    let tv = target.0.data.borrow();
    let n = pv.len() as f32;
    let s: f32 = pv.iter().zip(tv.iter()).map(|(p, t)| {
        let d = p - t;
        d * d
    }).sum();
    let out = vec![s / n.max(1.0)];
    Tensor::from_op(Shape::new(1, 1), out, Op::MseMean, vec![pred.clone(), target.clone()])
}

// ---------------- backward -----------------------------------------------

/// Zero out the `.grad` buffer of every reachable tensor in the graph
/// rooted at `root`. Useful between training steps, but `backward` also
/// does it for you, so callers can skip an explicit zero_grad.
pub fn zero_grad(root: &Tensor) {
    let order = topo(root);
    for t in &order {
        let mut g = t.0.grad.borrow_mut();
        for x in g.iter_mut() { *x = 0.0; }
    }
}

/// Reverse-mode backprop from a scalar (1×1) loss.
pub fn backward(loss: &Tensor) {
    assert_eq!(loss.0.shape.numel(), 1, "backward: loss must be scalar");
    let order = topo(loss);
    // Reset grads on every node we touched.
    for t in &order {
        let mut g = t.0.grad.borrow_mut();
        for x in g.iter_mut() { *x = 0.0; }
    }
    // Seed dL/dL = 1.
    loss.0.grad.borrow_mut()[0] = 1.0;
    // Walk in reverse topo order.
    for t in order.iter().rev() {
        backprop_op(t);
    }
}

fn backprop_op(t: &Tensor) {
    let inner = &t.0;
    let g_out = inner.grad.borrow().clone();
    match inner.op {
        Op::Leaf => {}
        Op::MatMul => {
            let a = &inner.parents[0];
            let b = &inner.parents[1];
            let (m, k) = (a.0.shape.rows, a.0.shape.cols);
            let n = b.0.shape.cols;
            let av = a.0.data.borrow();
            let bv = b.0.data.borrow();
            // dA = dC · Bᵀ
            {
                let mut ga = a.0.grad.borrow_mut();
                for i in 0..m {
                    for kk in 0..k {
                        let mut s = 0.0;
                        for j in 0..n {
                            s += g_out[i * n + j] * bv[kk * n + j];
                        }
                        ga[i * k + kk] += s;
                    }
                }
            }
            // dB = Aᵀ · dC
            {
                let mut gb = b.0.grad.borrow_mut();
                for kk in 0..k {
                    for j in 0..n {
                        let mut s = 0.0;
                        for i in 0..m {
                            s += av[i * k + kk] * g_out[i * n + j];
                        }
                        gb[kk * n + j] += s;
                    }
                }
            }
        }
        Op::AddBias => {
            let a = &inner.parents[0];
            let bias = &inner.parents[1];
            let (m, n) = (a.0.shape.rows, a.0.shape.cols);
            {
                let mut ga = a.0.grad.borrow_mut();
                for i in 0..(m * n) { ga[i] += g_out[i]; }
            }
            {
                let mut gb = bias.0.grad.borrow_mut();
                for j in 0..n {
                    let mut s = 0.0;
                    for i in 0..m { s += g_out[i * n + j]; }
                    gb[j] += s;
                }
            }
        }
        Op::Add => {
            let a = &inner.parents[0];
            let b = &inner.parents[1];
            let mut ga = a.0.grad.borrow_mut();
            let mut gb = b.0.grad.borrow_mut();
            for i in 0..g_out.len() {
                ga[i] += g_out[i];
                gb[i] += g_out[i];
            }
        }
        Op::Sub => {
            let a = &inner.parents[0];
            let b = &inner.parents[1];
            let mut ga = a.0.grad.borrow_mut();
            let mut gb = b.0.grad.borrow_mut();
            for i in 0..g_out.len() {
                ga[i] += g_out[i];
                gb[i] -= g_out[i];
            }
        }
        Op::Relu => {
            let a = &inner.parents[0];
            let av = a.0.data.borrow();
            let mut ga = a.0.grad.borrow_mut();
            for i in 0..g_out.len() {
                ga[i] += if av[i] > 0.0 { g_out[i] } else { 0.0 };
            }
        }
        Op::Tanh => {
            let a = &inner.parents[0];
            let out = inner.data.borrow();
            let mut ga = a.0.grad.borrow_mut();
            for i in 0..g_out.len() {
                let y = out[i];
                ga[i] += g_out[i] * (1.0 - y * y);
            }
        }
        Op::Sigmoid => {
            let a = &inner.parents[0];
            let out = inner.data.borrow();
            let mut ga = a.0.grad.borrow_mut();
            for i in 0..g_out.len() {
                let y = out[i];
                ga[i] += g_out[i] * y * (1.0 - y);
            }
        }
        Op::Gelu => {
            // Derivative of the tanh-approx GELU. Computed analytically
            // from the forward expression for stability.
            let a = &inner.parents[0];
            let av = a.0.data.borrow();
            let mut ga = a.0.grad.borrow_mut();
            let c = (2.0_f32 / std::f32::consts::PI).sqrt();
            for i in 0..g_out.len() {
                let x = av[i];
                let u = c * (x + 0.044715 * x * x * x);
                let t = u.tanh();
                let du_dx = c * (1.0 + 3.0 * 0.044715 * x * x);
                let dgelu = 0.5 * (1.0 + t) + 0.5 * x * (1.0 - t * t) * du_dx;
                ga[i] += g_out[i] * dgelu;
            }
        }
        Op::SoftmaxRow => {
            // dL/dx_i = sum_j dL/dy_j * (y_i * (delta_ij - y_j))
            //        = y_i * (dL/dy_i - sum_j(dL/dy_j * y_j))
            let a = &inner.parents[0];
            let out = inner.data.borrow();
            let (m, n) = (inner.shape.rows, inner.shape.cols);
            let mut ga = a.0.grad.borrow_mut();
            for i in 0..m {
                let mut dot = 0.0;
                for j in 0..n {
                    dot += g_out[i * n + j] * out[i * n + j];
                }
                for j in 0..n {
                    let y = out[i * n + j];
                    ga[i * n + j] += y * (g_out[i * n + j] - dot);
                }
            }
        }
        Op::MaeMean => {
            let pred = &inner.parents[0];
            let target = &inner.parents[1];
            let pv = pred.0.data.borrow();
            let tv = target.0.data.borrow();
            let n = pv.len() as f32;
            let scale = g_out[0] / n.max(1.0);
            let mut gp = pred.0.grad.borrow_mut();
            let mut gt = target.0.grad.borrow_mut();
            for i in 0..pv.len() {
                let s = (pv[i] - tv[i]).signum();
                gp[i] += scale * s;
                gt[i] -= scale * s;
            }
        }
        Op::Conv1DValid => {
            let input = &inner.parents[0];
            let kernel = &inner.parents[1];
            let l = input.0.shape.cols;
            let k = kernel.0.shape.cols;
            let out_len = l - k + 1;
            let iv = input.0.data.borrow();
            let kv = kernel.0.data.borrow();
            let mut gi = input.0.grad.borrow_mut();
            let mut gk = kernel.0.grad.borrow_mut();
            for i in 0..out_len {
                let go = g_out[i];
                for j in 0..k {
                    gi[i + j] += go * kv[j];
                    gk[j]    += go * iv[i + j];
                }
            }
        }
        Op::CrossEntropy => {
            // d/dp_i = -t_i / p_i (only "pred" gets a real gradient).
            let pred = &inner.parents[0];
            let target = &inner.parents[1];
            let pv = pred.0.data.borrow();
            let tv = target.0.data.borrow();
            let m = pred.0.shape.rows as f32;
            let scale = g_out[0] / m.max(1.0);
            let mut gp = pred.0.grad.borrow_mut();
            for i in 0..pv.len() {
                if tv[i] > 0.0 {
                    gp[i] += scale * (-tv[i] / pv[i].max(1e-12));
                }
            }
        }
        Op::MseMean => {
            let pred = &inner.parents[0];
            let target = &inner.parents[1];
            let pv = pred.0.data.borrow();
            let tv = target.0.data.borrow();
            let n = pv.len() as f32;
            let scale = (2.0 / n.max(1.0)) * g_out[0];
            let mut gp = pred.0.grad.borrow_mut();
            let mut gt = target.0.grad.borrow_mut();
            for i in 0..pv.len() {
                let d = pv[i] - tv[i];
                gp[i] += scale * d;
                gt[i] -= scale * d;
            }
        }
    }
}

/// Iterative DFS yielding tensors in topological order: parents before
/// children. `Arc::as_ptr` identity is used to dedupe.
fn topo(root: &Tensor) -> Vec<Tensor> {
    let mut out: Vec<Tensor> = Vec::new();
    let mut seen: Vec<*const TensorInner> = Vec::new();
    let mut stack: Vec<(Tensor, bool)> = vec![(root.clone(), false)];
    while let Some((node, expanded)) = stack.pop() {
        let p = Arc::as_ptr(&node.0);
        if expanded {
            if !seen.contains(&p) {
                seen.push(p);
                out.push(node);
            }
        } else {
            if seen.contains(&p) { continue; }
            stack.push((node.clone(), true));
            for parent in node.0.parents.iter() {
                stack.push((parent.clone(), false));
            }
        }
    }
    out
}

/// Apply `param.data -= lr * param.grad` in-place for any tensor in
/// `params` flagged as a parameter. Zeros the grad after.
pub fn sgd_step(params: &[Tensor], lr: f32) {
    for p in params {
        if !p.0.is_param { continue; }
        let mut data = p.0.data.borrow_mut();
        let mut grad = p.0.grad.borrow_mut();
        for i in 0..data.len() {
            data[i] -= lr * grad[i];
            grad[i] = 0.0;
        }
    }
}

// --- Adam / RMSprop state ------------------------------------------------
//
// We need per-parameter buffers (momentum + variance for Adam, just
// variance for RMSprop) without forcing the cljrs side to plumb them
// through. We park them in a thread-local `HashMap<*const TensorInner,
// State>` — `Tensor` is non-Send but the autograd path is single-
// threaded anyway. Key by Arc identity; entries die when the param's
// last clone drops (we don't bother with weak refs because params
// usually live for the whole training session).

use std::collections::HashMap;

#[derive(Default)]
struct AdamState {
    m: Vec<f32>,
    v: Vec<f32>,
    t: u32,
}

thread_local! {
    static ADAM: RefCell<HashMap<usize, AdamState>> = RefCell::new(HashMap::new());
    static RMS:  RefCell<HashMap<usize, Vec<f32>>>  = RefCell::new(HashMap::new());
}

/// Adam update with bias correction. lr ~= 1e-3 is the usual default.
pub fn adam_step(params: &[Tensor], lr: f32, beta1: f32, beta2: f32, eps: f32) {
    ADAM.with(|cell| {
        let mut map = cell.borrow_mut();
        for p in params {
            if !p.0.is_param { continue; }
            let key = Arc::as_ptr(&p.0) as usize;
            let st = map.entry(key).or_insert_with(|| AdamState {
                m: vec![0.0; p.0.shape.numel()],
                v: vec![0.0; p.0.shape.numel()],
                t: 0,
            });
            if st.m.len() != p.0.shape.numel() {
                st.m = vec![0.0; p.0.shape.numel()];
                st.v = vec![0.0; p.0.shape.numel()];
                st.t = 0;
            }
            st.t += 1;
            let bc1 = 1.0 - beta1.powi(st.t as i32);
            let bc2 = 1.0 - beta2.powi(st.t as i32);
            let mut data = p.0.data.borrow_mut();
            let mut grad = p.0.grad.borrow_mut();
            for i in 0..data.len() {
                let g = grad[i];
                st.m[i] = beta1 * st.m[i] + (1.0 - beta1) * g;
                st.v[i] = beta2 * st.v[i] + (1.0 - beta2) * g * g;
                let mhat = st.m[i] / bc1;
                let vhat = st.v[i] / bc2;
                data[i] -= lr * mhat / (vhat.sqrt() + eps);
                grad[i] = 0.0;
            }
        }
    })
}

pub fn rmsprop_step(params: &[Tensor], lr: f32, decay: f32, eps: f32) {
    RMS.with(|cell| {
        let mut map = cell.borrow_mut();
        for p in params {
            if !p.0.is_param { continue; }
            let key = Arc::as_ptr(&p.0) as usize;
            let v = map.entry(key).or_insert_with(|| vec![0.0; p.0.shape.numel()]);
            if v.len() != p.0.shape.numel() {
                *v = vec![0.0; p.0.shape.numel()];
            }
            let mut data = p.0.data.borrow_mut();
            let mut grad = p.0.grad.borrow_mut();
            for i in 0..data.len() {
                let g = grad[i];
                v[i] = decay * v[i] + (1.0 - decay) * g * g;
                data[i] -= lr * g / (v[i].sqrt() + eps);
                grad[i] = 0.0;
            }
        }
    })
}

/// Drop accumulated optimizer state. Useful when reinitializing weights.
pub fn reset_optimizer_state() {
    ADAM.with(|c| c.borrow_mut().clear());
    RMS.with(|c| c.borrow_mut().clear());
}
