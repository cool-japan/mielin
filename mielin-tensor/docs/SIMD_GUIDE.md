# SIMD Programming Guide for MielinTensor

## Table of Contents

1. [Introduction](#introduction)
2. [SIMD Architecture Overview](#simd-architecture-overview)
3. [Getting Started with SIMD](#getting-started-with-simd)
4. [Platform-Specific Programming](#platform-specific-programming)
5. [Optimization Techniques](#optimization-techniques)
6. [Common Patterns](#common-patterns)
7. [Performance Tips](#performance-tips)
8. [Debugging SIMD Code](#debugging-simd-code)
9. [Examples](#examples)

## Introduction

Single Instruction, Multiple Data (SIMD) is a parallel computing architecture that allows a single instruction to process multiple data elements simultaneously. This guide provides practical information for writing and optimizing SIMD code in mielin-tensor.

### Why SIMD?

- **Performance**: Process 4-16 elements per instruction vs. 1 for scalar
- **Efficiency**: Better utilization of CPU execution units
- **Power**: More work per watt compared to scalar operations
- **Modern Hardware**: All modern CPUs include SIMD extensions

### Performance Gains

Typical speedups for well-optimized SIMD code:
- **NEON (4x f32)**: 3-4x faster than scalar
- **AVX2 (8x f32)**: 6-8x faster than scalar
- **AVX-512 (16x f32)**: 10-14x faster than scalar
- **SVE2 (scalable)**: 4-16x depending on vector length

## SIMD Architecture Overview

### Supported Platforms

| Platform | Width | Elements | Availability |
|----------|-------|----------|--------------|
| ARM NEON | 128-bit | 4 x f32 | All ARMv8+ |
| Intel AVX2 | 256-bit | 8 x f32 | 2013+ |
| Intel AVX-512 | 512-bit | 16 x f32 | 2017+ |
| ARM SVE2 | Scalable | Variable | 2021+ |
| RISC-V RVV | Scalable | Variable | RV64V |

### Register Organization

```
Scalar:  [a]
NEON:    [a, b, c, d]
AVX2:    [a, b, c, d, e, f, g, h]
AVX-512: [a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p]
```

## Getting Started with SIMD

### Basic Workflow

1. **Detect capabilities** at runtime
2. **Select backend** based on available features
3. **Write SIMD function** with proper safety attributes
4. **Provide fallback** for unsupported platforms
5. **Test thoroughly** on target hardware

### Example: Dot Product

```rust
use mielin_hal::capabilities::HardwareCapabilities;

pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());

    let caps = HardwareCapabilities::detect();

    if caps.contains(HardwareCapabilities::AVX2) {
        unsafe { dot_product_avx2(a, b) }
    } else if caps.contains(HardwareCapabilities::NEON) {
        unsafe { dot_product_neon(a, b) }
    } else {
        dot_product_scalar(a, b)
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    let len = a.len();
    let mut sum = _mm256_setzero_ps();

    // Process 8 elements at a time
    let chunks = len / 8;
    for i in 0..chunks {
        let idx = i * 8;
        let va = _mm256_loadu_ps(a.as_ptr().add(idx));
        let vb = _mm256_loadu_ps(b.as_ptr().add(idx));
        sum = _mm256_fmadd_ps(va, vb, sum);
    }

    // Horizontal sum
    let mut result = [0f32; 8];
    _mm256_storeu_ps(result.as_mut_ptr(), sum);
    let simd_sum: f32 = result.iter().sum();

    // Handle remainder
    let remainder: f32 = (chunks * 8..len)
        .map(|i| a[i] * b[i])
        .sum();

    simd_sum + remainder
}

fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
```

## Platform-Specific Programming

### ARM NEON

**Characteristics**:
- Fixed 128-bit width
- 32 SIMD registers (v0-v31)
- Efficient FMA (fused multiply-add)
- Good for mobile and embedded

**Key Intrinsics**:

```rust
use core::arch::aarch64::*;

// Load/Store
let v = vld1q_f32(ptr);           // Load 4 x f32
vst1q_f32(ptr, v);                // Store 4 x f32

// Arithmetic
let sum = vaddq_f32(a, b);        // a + b
let diff = vsubq_f32(a, b);       // a - b
let prod = vmulq_f32(a, b);       // a * b
let fma = vfmaq_f32(c, a, b);     // c + a * b

// Reduction
let sum_all = vaddvq_f32(v);      // Horizontal sum

// Comparison
let mask = vcgtq_f32(a, b);       // a > b
let max = vmaxq_f32(a, b);        // element-wise max
```

**Example**: Element-wise multiplication

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn mul_neon(a: &[f32], b: &[f32], result: &mut [f32]) {
    use core::arch::aarch64::*;

    let len = a.len();
    let chunks = len / 4;

    for i in 0..chunks {
        let idx = i * 4;
        let va = vld1q_f32(a.as_ptr().add(idx));
        let vb = vld1q_f32(b.as_ptr().add(idx));
        let vr = vmulq_f32(va, vb);
        vst1q_f32(result.as_mut_ptr().add(idx), vr);
    }

    // Handle remainder
    for i in (chunks * 4)..len {
        result[i] = a[i] * b[i];
    }
}
```

### Intel AVX2

**Characteristics**:
- 256-bit width (8 x f32)
- 16 YMM registers (ymm0-ymm15)
- Excellent FMA support
- Desktop/server workloads

**Key Intrinsics**:

```rust
use core::arch::x86_64::*;

// Load/Store
let v = _mm256_loadu_ps(ptr);           // Load 8 x f32 (unaligned)
let v = _mm256_load_ps(ptr);            // Load 8 x f32 (32-byte aligned)
_mm256_storeu_ps(ptr, v);               // Store (unaligned)

// Arithmetic
let sum = _mm256_add_ps(a, b);          // a + b
let diff = _mm256_sub_ps(a, b);         // a - b
let prod = _mm256_mul_ps(a, b);         // a * b
let fma = _mm256_fmadd_ps(a, b, c);     // a * b + c

// Reduction (requires horizontal operations)
let h1 = _mm256_hadd_ps(v, v);          // Horizontal add (slow!)
// Better: use scalar extraction and sum

// Comparison
let mask = _mm256_cmp_ps(a, b, _CMP_GT_OQ);  // a > b
let max = _mm256_max_ps(a, b);          // element-wise max
```

**Example**: Scaled addition (y = a * x + b)

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn saxpy_avx2(a: f32, x: &[f32], b: &[f32], y: &mut [f32]) {
    use core::arch::x86_64::*;

    let len = x.len();
    let chunks = len / 8;
    let va = _mm256_set1_ps(a);  // Broadcast scalar

    for i in 0..chunks {
        let idx = i * 8;
        let vx = _mm256_loadu_ps(x.as_ptr().add(idx));
        let vb = _mm256_loadu_ps(b.as_ptr().add(idx));
        let vy = _mm256_fmadd_ps(va, vx, vb);  // a * x + b
        _mm256_storeu_ps(y.as_mut_ptr().add(idx), vy);
    }

    // Remainder
    for i in (chunks * 8)..len {
        y[i] = a * x[i] + b[i];
    }
}
```

### Intel AVX-512

**Characteristics**:
- 512-bit width (16 x f32)
- 32 ZMM registers (zmm0-zmm31)
- Masked operations
- Server/HPC workloads

**Key Intrinsics**:

```rust
use core::arch::x86_64::*;

// Load/Store
let v = _mm512_loadu_ps(ptr);           // Load 16 x f32
_mm512_storeu_ps(ptr, v);               // Store 16 x f32

// Arithmetic
let sum = _mm512_add_ps(a, b);
let fma = _mm512_fmadd_ps(a, b, c);

// Reduction
let sum_all = _mm512_reduce_add_ps(v);  // Horizontal sum (efficient!)

// Masked operations
let mask = _mm512_cmp_ps_mask(a, b, _CMP_GT_OQ);
let v = _mm512_mask_add_ps(src, mask, a, b);  // Conditional add
```

**Example**: ReLU activation

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn relu_avx512(x: &[f32], y: &mut [f32]) {
    use core::arch::x86_64::*;

    let len = x.len();
    let chunks = len / 16;
    let zero = _mm512_setzero_ps();

    for i in 0..chunks {
        let idx = i * 16;
        let vx = _mm512_loadu_ps(x.as_ptr().add(idx));
        let vy = _mm512_max_ps(vx, zero);  // max(x, 0)
        _mm512_storeu_ps(y.as_mut_ptr().add(idx), vy);
    }

    for i in (chunks * 16)..len {
        y[i] = x[i].max(0.0);
    }
}
```

## Optimization Techniques

### 1. Loop Unrolling

Process multiple SIMD vectors per iteration to reduce loop overhead:

```rust
// Instead of processing 1 vector per iteration
for i in 0..chunks {
    let v = load_vector(i);
    let result = process(v);
    store_vector(result, i);
}

// Unroll to process 4 vectors per iteration
let chunks4 = len / (SIMD_WIDTH * 4);
for i in 0..chunks4 {
    let v0 = load_vector(i * 4 + 0);
    let v1 = load_vector(i * 4 + 1);
    let v2 = load_vector(i * 4 + 2);
    let v3 = load_vector(i * 4 + 3);

    let r0 = process(v0);
    let r1 = process(v1);
    let r2 = process(v2);
    let r3 = process(v3);

    store_vector(r0, i * 4 + 0);
    store_vector(r1, i * 4 + 1);
    store_vector(r2, i * 4 + 2);
    store_vector(r3, i * 4 + 3);
}
```

**Benefits**:
- Reduced branch mispredictions
- Better instruction-level parallelism
- Amortized loop overhead

### 2. Minimize Horizontal Operations

Horizontal operations (shuffles, reductions) are slow on most platforms:

```rust
// SLOW: Frequent horizontal sums
let mut sum = zero_vector();
for chunk in chunks {
    let v = load(chunk);
    let h = horizontal_sum(v);  // SLOW!
    sum = add_scalar(sum, h);
}

// FAST: Accumulate in vector, reduce once at end
let mut accum = zero_vector();
for chunk in chunks {
    let v = load(chunk);
    accum = add(accum, v);
}
let total = horizontal_sum(accum);  // Only once!
```

### 3. Cache-Aware Processing

Structure loops to maximize cache locality:

```rust
// Process data in cache-friendly blocks
const L1_SIZE: usize = 32 * 1024;  // 32KB L1 cache
const BLOCK_SIZE: usize = L1_SIZE / (4 * size_of::<f32>());

for block_start in (0..n).step_by(BLOCK_SIZE) {
    let block_end = (block_start + BLOCK_SIZE).min(n);

    // Process this block while it's hot in cache
    for i in (block_start..block_end).step_by(SIMD_WIDTH) {
        // SIMD operations on [i..i+SIMD_WIDTH]
    }
}
```

### 4. Alignment Optimization

Aligned loads/stores are faster:

```rust
use std::alloc::{alloc, dealloc, Layout};

// Allocate aligned memory
unsafe fn allocate_aligned(size: usize, align: usize) -> *mut f32 {
    let layout = Layout::from_size_align_unchecked(
        size * size_of::<f32>(),
        align
    );
    alloc(layout) as *mut f32
}

// Use aligned loads when possible
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn process_aligned(data: *const f32, len: usize) {
    use core::arch::x86_64::*;

    // Assume data is 32-byte aligned
    for i in (0..len).step_by(8) {
        let v = _mm256_load_ps(data.add(i));  // Aligned load (faster)
        // ... process v
    }
}
```

### 5. Prefetching

Prefetch data before it's needed:

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse")]
unsafe fn prefetch_example(data: &[f32]) {
    use core::arch::x86_64::*;

    const PREFETCH_DISTANCE: usize = 64; // 64 elements ahead

    for i in (0..data.len()).step_by(8) {
        // Prefetch future data
        if i + PREFETCH_DISTANCE < data.len() {
            _mm_prefetch(
                data.as_ptr().add(i + PREFETCH_DISTANCE) as *const i8,
                _MM_HINT_T0  // L1 cache
            );
        }

        // Process current data
        let v = _mm256_loadu_ps(data.as_ptr().add(i));
        // ... process v
    }
}
```

## Common Patterns

### Pattern 1: Map (Element-wise Unary Operation)

```rust
fn map_simd<F>(input: &[f32], output: &mut [f32], f: F)
where
    F: Fn(__m256) -> __m256,
{
    let chunks = input.len() / 8;

    for i in 0..chunks {
        unsafe {
            let idx = i * 8;
            let v_in = _mm256_loadu_ps(input.as_ptr().add(idx));
            let v_out = f(v_in);
            _mm256_storeu_ps(output.as_mut_ptr().add(idx), v_out);
        }
    }

    // Remainder (scalar)
    for i in (chunks * 8)..input.len() {
        output[i] = /* scalar version of f */
    }
}
```

### Pattern 2: Zip (Element-wise Binary Operation)

```rust
fn zip_simd<F>(a: &[f32], b: &[f32], output: &mut [f32], f: F)
where
    F: Fn(__m256, __m256) -> __m256,
{
    let chunks = a.len() / 8;

    for i in 0..chunks {
        unsafe {
            let idx = i * 8;
            let va = _mm256_loadu_ps(a.as_ptr().add(idx));
            let vb = _mm256_loadu_ps(b.as_ptr().add(idx));
            let vout = f(va, vb);
            _mm256_storeu_ps(output.as_mut_ptr().add(idx), vout);
        }
    }

    // Remainder
}
```

### Pattern 3: Reduce (Accumulation)

```rust
fn reduce_simd<F>(input: &[f32], init: __m256, f: F) -> f32
where
    F: Fn(__m256, __m256) -> __m256,
{
    let chunks = input.len() / 8;
    let mut accum = init;

    for i in 0..chunks {
        unsafe {
            let idx = i * 8;
            let v = _mm256_loadu_ps(input.as_ptr().add(idx));
            accum = f(accum, v);
        }
    }

    // Horizontal reduction
    let result = unsafe {
        let mut temp = [0f32; 8];
        _mm256_storeu_ps(temp.as_mut_ptr(), accum);
        temp.iter().sum::<f32>()
    };

    // Add remainder
    result + input[chunks * 8..].iter().sum::<f32>()
}
```

## Performance Tips

### Do's

✓ **Use FMA** when available (faster than separate mul + add)
✓ **Unroll loops** 2-4x for better ILP
✓ **Avoid branches** inside SIMD loops
✓ **Align data** to SIMD width for faster loads
✓ **Process remainders** correctly (don't ignore!)
✓ **Benchmark** on target hardware
✓ **Use builtin reductions** (AVX-512) when available

### Don'ts

✗ **Avoid horizontal operations** in hot loops
✗ **Don't mix scalar and SIMD** unnecessarily
✗ **Don't assume alignment** unless guaranteed
✗ **Avoid gather/scatter** unless necessary (slow)
✗ **Don't over-unroll** (code bloat, instruction cache misses)

### Measuring Performance

```rust
use std::time::Instant;

fn benchmark_simd_vs_scalar() {
    let data = vec![1.0f32; 1_000_000];
    let mut result = vec![0.0f32; 1_000_000];

    // Warm up
    for _ in 0..10 {
        process_simd(&data, &mut result);
    }

    // Benchmark SIMD
    let start = Instant::now();
    for _ in 0..1000 {
        process_simd(&data, &mut result);
    }
    let simd_time = start.elapsed();

    // Benchmark scalar
    let start = Instant::now();
    for _ in 0..1000 {
        process_scalar(&data, &mut result);
    }
    let scalar_time = start.elapsed();

    println!("SIMD: {:?}, Scalar: {:?}, Speedup: {:.2}x",
        simd_time, scalar_time,
        scalar_time.as_secs_f64() / simd_time.as_secs_f64()
    );
}
```

## Debugging SIMD Code

### Print SIMD Vectors

```rust
unsafe fn print_m256(v: __m256, name: &str) {
    let mut data = [0f32; 8];
    _mm256_storeu_ps(data.as_mut_ptr(), v);
    println!("{}: [{}, {}, {}, {}, {}, {}, {}, {}]",
        name, data[0], data[1], data[2], data[3],
        data[4], data[5], data[6], data[7]
    );
}
```

### Compare with Scalar

```rust
fn verify_simd_correctness() {
    let input = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
    let mut simd_result = vec![0.0; 9];
    let mut scalar_result = vec![0.0; 9];

    process_simd(&input, &mut simd_result);
    process_scalar(&input, &mut scalar_result);

    for (i, (&s, &v)) in scalar_result.iter().zip(&simd_result).enumerate() {
        assert!(
            (s - v).abs() < 1e-6,
            "Mismatch at index {}: scalar={}, simd={}",
            i, s, v
        );
    }
}
```

### Use Sanitizers

```bash
# Address sanitizer (detects out-of-bounds, use-after-free)
RUSTFLAGS="-Z sanitizer=address" cargo test

# Memory sanitizer (detects uninitialized reads)
RUSTFLAGS="-Z sanitizer=memory" cargo test
```

## Examples

### Example 1: Matrix-Vector Multiplication

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn matvec_avx2(
    matrix: &[f32],  // Row-major, rows x cols
    vector: &[f32],  // Length cols
    result: &mut [f32],  // Length rows
    rows: usize,
    cols: usize,
) {
    use core::arch::x86_64::*;

    for row in 0..rows {
        let mut accum = _mm256_setzero_ps();
        let row_start = row * cols;

        // Process 8 elements at a time
        let chunks = cols / 8;
        for i in 0..chunks {
            let idx = i * 8;
            let m = _mm256_loadu_ps(matrix.as_ptr().add(row_start + idx));
            let v = _mm256_loadu_ps(vector.as_ptr().add(idx));
            accum = _mm256_fmadd_ps(m, v, accum);
        }

        // Horizontal sum
        let mut temp = [0f32; 8];
        _mm256_storeu_ps(temp.as_mut_ptr(), accum);
        let mut sum: f32 = temp.iter().sum();

        // Remainder
        for i in (chunks * 8)..cols {
            sum += matrix[row_start + i] * vector[i];
        }

        result[row] = sum;
    }
}
```

### Example 2: Softmax Activation

```rust
pub fn softmax(input: &[f32], output: &mut [f32]) {
    // Find max for numerical stability
    let max_val = input.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Compute exp(x - max) and sum
    let mut sum = 0.0f32;
    for (&x, out) in input.iter().zip(output.iter_mut()) {
        let exp_val = (x - max_val).exp();
        *out = exp_val;
        sum += exp_val;
    }

    // Normalize
    let inv_sum = 1.0 / sum;
    for out in output.iter_mut() {
        *out *= inv_sum;
    }
}

// SIMD version
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn softmax_avx2(input: &[f32], output: &mut [f32]) {
    use core::arch::x86_64::*;

    // Find max (SIMD reduction)
    let len = input.len();
    let chunks = len / 8;
    let mut vmax = _mm256_set1_ps(f32::NEG_INFINITY);

    for i in 0..chunks {
        let v = _mm256_loadu_ps(input.as_ptr().add(i * 8));
        vmax = _mm256_max_ps(vmax, v);
    }

    let mut temp = [0f32; 8];
    _mm256_storeu_ps(temp.as_mut_ptr(), vmax);
    let max_val = temp.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Handle remainder for max
    let max_val = input[chunks * 8..].iter()
        .cloned()
        .fold(max_val, f32::max);

    let vmax_broadcast = _mm256_set1_ps(max_val);

    // Compute exp(x - max) and sum
    let mut vsum = _mm256_setzero_ps();

    for i in 0..chunks {
        let idx = i * 8;
        let v = _mm256_loadu_ps(input.as_ptr().add(idx));
        let v_sub = _mm256_sub_ps(v, vmax_broadcast);

        // Note: No native exp in AVX2, need scalar fallback for exp
        let mut exp_vals = [0f32; 8];
        _mm256_storeu_ps(exp_vals.as_mut_ptr(), v_sub);

        for j in 0..8 {
            exp_vals[j] = exp_vals[j].exp();
        }

        let v_exp = _mm256_loadu_ps(exp_vals.as_ptr());
        _mm256_storeu_ps(output.as_mut_ptr().add(idx), v_exp);
        vsum = _mm256_add_ps(vsum, v_exp);
    }

    // Sum reduction
    _mm256_storeu_ps(temp.as_mut_ptr(), vsum);
    let mut sum: f32 = temp.iter().sum();

    // Remainder
    for i in (chunks * 8)..len {
        let exp_val = (input[i] - max_val).exp();
        output[i] = exp_val;
        sum += exp_val;
    }

    // Normalize
    let inv_sum = 1.0 / sum;
    let vinv = _mm256_set1_ps(inv_sum);

    for i in 0..chunks {
        let idx = i * 8;
        let v = _mm256_loadu_ps(output.as_ptr().add(idx));
        let v_norm = _mm256_mul_ps(v, vinv);
        _mm256_storeu_ps(output.as_mut_ptr().add(idx), v_norm);
    }

    for i in (chunks * 8)..len {
        output[i] *= inv_sum;
    }
}
```

## Further Reading

- [Intel Intrinsics Guide](https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html)
- [ARM NEON Intrinsics Reference](https://developer.arm.com/architectures/instruction-sets/intrinsics/)
- [Agner Fog's Optimization Manuals](https://www.agner.org/optimize/)
- [SIMD for C++ Developers](https://www.youtube.com/watch?v=x9Scb5Mku1g)
- [Rust SIMD Performance Guide](https://rust-lang.github.io/packed_simd/perf-guide/)

## See Also

- [SIMD_SAFETY.md](./SIMD_SAFETY.md) - Safety requirements and best practices
- [Performance Tuning Guide](./PERFORMANCE_TUNING.md) - General optimization strategies
- [Tensor Layout Conventions](./TENSOR_LAYOUT.md) - Memory layout and indexing

---

**Last Updated**: 2026-01-18
**Maintainers**: COOLJAPAN OU (Team Kitasan)
