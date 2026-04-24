# cljrs: (elementwise-mojo vector_add [^f64 a ^f64 b] ^f64 (+ a b))
fn vector_add(a: UnsafePointer[Float64, MutAnyOrigin], b: UnsafePointer[Float64, MutAnyOrigin], dst: UnsafePointer[Float64, MutAnyOrigin], n: Int):
    for i in range(n):
        dst[i] = (a[i] + b[i])

# cljrs: (elementwise-mojo saxpy [^f64 a ^f64 x ^f64 y] ^f64 (+ (* a ...
fn saxpy(a: UnsafePointer[Float64, MutAnyOrigin], x: UnsafePointer[Float64, MutAnyOrigin], y: UnsafePointer[Float64, MutAnyOrigin], dst: UnsafePointer[Float64, MutAnyOrigin], n: Int):
    for i in range(n):
        dst[i] = (a[i] * x[i] + y[i])

# cljrs: (reduce-mojo dot [^f64 x ^f64 y] ^f64 (* x y) 0.0)
fn dot(x: UnsafePointer[Float64, MutAnyOrigin], y: UnsafePointer[Float64, MutAnyOrigin], n: Int) -> Float64:
    var acc: Float64 = 0.0
    for i in range(n):
        acc += (x[i] * y[i])
    return acc

# cljrs: (reduce-mojo sum_sq [^f64 x] ^f64 (* x x) 0.0)
fn sum_sq(x: UnsafePointer[Float64, MutAnyOrigin], n: Int) -> Float64:
    var acc: Float64 = 0.0
    for i in range(n):
        acc += (x[i] * x[i])
    return acc
