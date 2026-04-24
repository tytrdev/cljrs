from std.memory import UnsafePointer
from std.memory.unsafe_pointer import alloc
from std.sys import simd_width_of

alias nelts_f64 = simd_width_of[DType.float64]()

fn vector_add(a: UnsafePointer[Float64, MutAnyOrigin], b: UnsafePointer[Float64, MutAnyOrigin], dst: UnsafePointer[Float64, MutAnyOrigin], n: Int):
    var i = 0
    while i + nelts_f64 <= n:
        var av = a.load[width=nelts_f64](i)
        var bv = b.load[width=nelts_f64](i)
        dst.store[width=nelts_f64](i, (av + bv))
        i += nelts_f64
    while i < n:
        dst[i] = (a[i] + b[i])
        i += 1

fn saxpy(a: UnsafePointer[Float64, MutAnyOrigin], x: UnsafePointer[Float64, MutAnyOrigin], y: UnsafePointer[Float64, MutAnyOrigin], dst: UnsafePointer[Float64, MutAnyOrigin], n: Int):
    var i = 0
    while i + nelts_f64 <= n:
        var av = a.load[width=nelts_f64](i)
        var xv = x.load[width=nelts_f64](i)
        var yv = y.load[width=nelts_f64](i)
        dst.store[width=nelts_f64](i, (av * xv + yv))
        i += nelts_f64
    while i < n:
        dst[i] = (a[i] * x[i] + y[i])
        i += 1

fn dot(x: UnsafePointer[Float64, MutAnyOrigin], y: UnsafePointer[Float64, MutAnyOrigin], n: Int) -> Float64:
    var acc = SIMD[DType.float64, nelts_f64](0.0)
    var i = 0
    while i + nelts_f64 <= n:
        var xv = x.load[width=nelts_f64](i)
        var yv = y.load[width=nelts_f64](i)
        acc += (xv * yv)
        i += nelts_f64
    var tail: Float64 = acc.reduce_add()
    while i < n:
        tail += (x[i] * y[i])
        i += 1
    return tail

fn sum_sq(x: UnsafePointer[Float64, MutAnyOrigin], n: Int) -> Float64:
    var acc = SIMD[DType.float64, nelts_f64](0.0)
    var i = 0
    while i + nelts_f64 <= n:
        var xv = x.load[width=nelts_f64](i)
        acc += (xv * xv)
        i += nelts_f64
    var tail: Float64 = acc.reduce_add()
    while i < n:
        tail += (x[i] * x[i])
        i += 1
    return tail
