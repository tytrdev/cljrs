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
    Tanh,
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
