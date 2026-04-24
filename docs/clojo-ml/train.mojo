# Logistic regression training harness driving cljrs-mojo kernels.
#
# This file gets concatenated with kernels.mojo.max (the transpiled
# kernels from bench/clojo_logreg/kernels.clj) at build time, so
# dot/sigmoid/vsub/vsum/update_weights are already in scope.
#
# The training math is plain gradient descent on BCE loss:
#   forward:    z[i] = dot(X_row[i], w) + b
#               p[i] = sigmoid(z[i])
#   backward:   err  = p - y
#               g[j] = (1/N) * dot(X_col[j], err)
#               g_b  = mean(err)
#   update:     w -= lr * g
#               b -= lr * g_b
#
# Data is loaded from raw little-endian f64 binary files produced by
# prepare_data.py. X is stored row-major N×D; we materialize a
# column-major copy once at load time so each feature's gradient is
# a single `dot(X_col_j, err)` call.

from std.memory.unsafe_pointer import alloc
from std.time import perf_counter_ns
from pathlib import Path


def read_f64_file(path: String, n: Int, dst: UnsafePointer[Float64, MutAnyOrigin]) raises:
    """Read `n` little-endian f64 values from `path` into `dst`."""
    var f = open(path, "r")
    var bytes = f.read_bytes()
    f.close()
    var nbytes = len(bytes)
    debug_assert(nbytes == n * 8, "file size mismatch for " + path)
    # Memcpy-equivalent via byte-wise copy. bytes is a List[UInt8].
    var bp = dst.bitcast[UInt8]()
    for i in range(nbytes):
        bp[i] = bytes[i]


def transpose_to_col_major(
    src: UnsafePointer[Float64, MutAnyOrigin],
    dst: UnsafePointer[Float64, MutAnyOrigin],
    rows: Int, cols: Int,
):
    for i in range(rows):
        for j in range(cols):
            dst[j * rows + i] = src[i * cols + j]


def accuracy(p: UnsafePointer[Float64, MutAnyOrigin],
             y: UnsafePointer[Float64, MutAnyOrigin], n: Int) -> Float64:
    var correct = 0
    for i in range(n):
        var pred: Float64 = 1.0 if p[i] >= 0.5 else 0.0
        if pred == y[i]:
            correct += 1
    return Float64(correct) / Float64(n)


def bce_loss(p: UnsafePointer[Float64, MutAnyOrigin],
             y: UnsafePointer[Float64, MutAnyOrigin], n: Int) -> Float64:
    # -mean(y*log(p) + (1-y)*log(1-p)), with eps clamping.
    from math import log
    var s: Float64 = 0.0
    var eps: Float64 = 1.0e-15
    for i in range(n):
        var pc: Float64 = p[i]
        if pc < eps:
            pc = eps
        if pc > 1.0 - eps:
            pc = 1.0 - eps
        s += -(y[i] * log(pc) + (1.0 - y[i]) * log(1.0 - pc))
    return s / Float64(n)


def forward_pass(
    X_row: UnsafePointer[Float64, MutAnyOrigin],  # [N x D] row-major
    w: UnsafePointer[Float64, MutAnyOrigin],
    b: Float64,
    z: UnsafePointer[Float64, MutAnyOrigin],
    p: UnsafePointer[Float64, MutAnyOrigin],
    N: Int, D: Int,
):
    for i in range(N):
        # One dot product per sample — the transpiled `dot` kernel.
        var dot_row_w = dot(X_row+ i * D, w, D)
        z[i] = dot_row_w + b
    sigmoid(z, p, N)


def backward_and_update(
    X_col: UnsafePointer[Float64, MutAnyOrigin],  # [D x N] column-major
    y: UnsafePointer[Float64, MutAnyOrigin],
    p: UnsafePointer[Float64, MutAnyOrigin],
    err: UnsafePointer[Float64, MutAnyOrigin],
    g: UnsafePointer[Float64, MutAnyOrigin],
    w: UnsafePointer[Float64, MutAnyOrigin],
    b_ptr: UnsafePointer[Float64, MutAnyOrigin],  # single-f64 cell
    w_new: UnsafePointer[Float64, MutAnyOrigin],
    lr: Float64, N: Int, D: Int,
):
    vsub(p, y, err, N)
    var inv_n: Float64 = 1.0 / Float64(N)
    # gradient per feature via dot(X_col_j, err), scaled by 1/N.
    for j in range(D):
        g[j] = dot(X_col+ j * N, err, N) * inv_n
    # bias gradient = mean(err)
    var g_b = vsum(err, N) * inv_n
    # w -= lr * g  (via transpiled elementwise kernel)
    update_weights(w, g, lr, w_new, D)
    # swap w and w_new — we allocate w_new per-epoch in harness
    for k in range(D):
        w[k] = w_new[k]
    b_ptr[0] -= lr * g_b


def main() raises:
    var n_train = 455
    var n_test = 114
    var D = 30
    var EPOCHS = 1000
    var LR: Float64 = 0.05

    # Allocate.
    var X_train_row = alloc[Float64](n_train * D)
    var X_train_col = alloc[Float64](n_train * D)
    var y_train = alloc[Float64](n_train)
    var X_test_row = alloc[Float64](n_test * D)
    var y_test = alloc[Float64](n_test)

    var w = alloc[Float64](D)
    var w_new = alloc[Float64](D)
    var g = alloc[Float64](D)
    var b = alloc[Float64](1)
    var z_train = alloc[Float64](n_train)
    var p_train = alloc[Float64](n_train)
    var err_train = alloc[Float64](n_train)
    var z_test = alloc[Float64](n_test)
    var p_test = alloc[Float64](n_test)

    # Zero-init weights.
    for k in range(D):
        w[k] = 0.0
        g[k] = 0.0
    b[0] = 0.0

    # Load data — files are relative to the CWD the binary runs in.
    var data_dir = "bench/clojo_logreg/data/"
    read_f64_file(data_dir + "X_train.f64", n_train * D, X_train_row)
    read_f64_file(data_dir + "y_train.f64", n_train, y_train)
    read_f64_file(data_dir + "X_test.f64", n_test * D, X_test_row)
    read_f64_file(data_dir + "y_test.f64", n_test, y_test)
    transpose_to_col_major(X_train_row, X_train_col, n_train, D)

    # Training loop.
    var t0 = perf_counter_ns()
    for epoch in range(EPOCHS):
        forward_pass(X_train_row, w, b[0], z_train, p_train, n_train, D)
        backward_and_update(X_train_col, y_train, p_train, err_train, g,
                            w, b, w_new, LR, n_train, D)
    var train_ns = perf_counter_ns() - t0

    # Final metrics on both splits.
    forward_pass(X_train_row, w, b[0], z_train, p_train, n_train, D)
    var train_acc = accuracy(p_train, y_train, n_train)
    var train_loss = bce_loss(p_train, y_train, n_train)

    forward_pass(X_test_row, w, b[0], z_test, p_test, n_test, D)
    var test_acc = accuracy(p_test, y_test, n_test)
    var test_loss = bce_loss(p_test, y_test, n_test)

    # JSON output.
    print("{")
    print('  "epochs":', EPOCHS, ',')
    print('  "lr":', LR, ',')
    print('  "n_train":', n_train, ', "n_test":', n_test, ', "n_features":', D, ',')
    print('  "train_ns":', Int(train_ns), ',')
    print('  "train_acc":', train_acc, ',')
    print('  "train_loss":', train_loss, ',')
    print('  "test_acc":', test_acc, ',')
    print('  "test_loss":', test_loss, ',')
    print('  "bias":', b[0], ',')
    print('  "weights": [')
    for k in range(D):
        var comma = "," if k < D - 1 else ""
        print("   ", w[k], comma)
    print('  ]')
    print("}")

    X_train_row.free(); X_train_col.free(); y_train.free()
    X_test_row.free(); y_test.free()
    w.free(); w_new.free(); g.free(); b.free()
    z_train.free(); p_train.free(); err_train.free()
    z_test.free(); p_test.free()
