from algorithm import vectorize
from memory import UnsafePointer
from sys import simd_width_of

alias nelts_f64 = simd_width_of[DType.float64]()

fn vector_add(a: UnsafePointer[Float64], b: UnsafePointer[Float64], out: UnsafePointer[Float64], n: Int):
    @parameter
    fn __kernel[w: Int](i: Int):
        var av = SIMD[DType.float64, w].load(a, i)
        var bv = SIMD[DType.float64, w].load(b, i)
        (av + bv).store(out, i)
    vectorize[__kernel, nelts_f64](n)

fn saxpy(a: UnsafePointer[Float64], x: UnsafePointer[Float64], y: UnsafePointer[Float64], out: UnsafePointer[Float64], n: Int):
    @parameter
    fn __kernel[w: Int](i: Int):
        var av = SIMD[DType.float64, w].load(a, i)
        var xv = SIMD[DType.float64, w].load(x, i)
        var yv = SIMD[DType.float64, w].load(y, i)
        (av * xv + yv).store(out, i)
    vectorize[__kernel, nelts_f64](n)

fn dot(x: UnsafePointer[Float64], y: UnsafePointer[Float64], n: Int) -> Float64:
    var acc = SIMD[DType.float64, nelts_f64](0.0)
    @parameter
    fn __kernel[w: Int](i: Int):
        var xv = SIMD[DType.float64, w].load(x, i)
        var yv = SIMD[DType.float64, w].load(y, i)
        acc[0] += (xv * yv).reduce_add()
    vectorize[__kernel, nelts_f64](n)
    return acc.reduce_add()

fn sum_sq(x: UnsafePointer[Float64], n: Int) -> Float64:
    var acc = SIMD[DType.float64, nelts_f64](0.0)
    @parameter
    fn __kernel[w: Int](i: Int):
        var xv = SIMD[DType.float64, w].load(x, i)
        acc[0] += (xv * xv).reduce_add()
    vectorize[__kernel, nelts_f64](n)
    return acc.reduce_add()
