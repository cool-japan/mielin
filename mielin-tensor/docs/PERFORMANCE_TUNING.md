# Performance Tuning Guide for MielinTensor

## Table of Contents

1. [Introduction](#introduction)
2. [Profiling and Measurement](#profiling-and-measurement)
3. [Memory Optimization](#memory-optimization)
4. [Compute Optimization](#compute-optimization)
5. [Hardware-Specific Tuning](#hardware-specific-tuning)
6. [Tensor Operations Optimization](#tensor-operations-optimization)
7. [Neural Network Inference](#neural-network-inference)
8. [Quantization and Mixed Precision](#quantization-and-mixed-precision)
9. [Common Performance Pitfalls](#common-performance-pitfalls)
10. [Performance Targets and Benchmarking](#performance-targets-and-benchmarking)

## Introduction

This guide provides practical strategies for optimizing tensor operations in mielin-tensor. The goal is to achieve maximum throughput while minimizing memory usage and latency.

### Performance Philosophy

1. **Measure First**: Always profile before optimizing
2. **Focus on Hot Paths**: Optimize the critical 10% that takes 90% of time
3. **Hardware Awareness**: Understand your target platform's capabilities
4. **Trade-offs**: Balance speed, accuracy, and memory usage

### Expected Performance Gains

With proper optimization:
- **2-4x** from cache optimization
- **3-16x** from SIMD (depending on platform)
- **2-8x** from quantization
- **10-100x** from NPU offload (future)

## Profiling and Measurement

### Using PerfMonitor

MielinTensor includes built-in performance monitoring:

```rust
use mielin_tensor::profiling::PerfMonitor;

let mut monitor = PerfMonitor::new();

// Start monitoring an operation
monitor.start("matrix_multiply");

// ... perform operation ...

// Record timing
monitor.record("matrix_multiply");

// Get statistics
let metrics = monitor.metrics("matrix_multiply").unwrap();
println!("Min: {:.3}ms, Max: {:.3}ms, Avg: {:.3}ms",
    metrics.min_time * 1000.0,
    metrics.max_time * 1000.0,
    metrics.avg_time * 1000.0
);
```

### Memory Tracking

```rust
use mielin_tensor::profiling::MemoryTracker;

let mut tracker = MemoryTracker::new();

let before = tracker.snapshot();

// ... allocate tensors ...

let after = tracker.snapshot();
let diff = after.diff(&before);

println!("Allocated: {} bytes, Deallocated: {} bytes, Net: {} bytes",
    diff.allocated, diff.deallocated, diff.net_change
);
```

### Regression Detection

```rust
use mielin_tensor::profiling::RegressionDetector;

let mut detector = RegressionDetector::new(0.10); // 10% threshold

// Record baseline
detector.record_baseline("operation", 100.0); // 100ms

// Check for regression
if let Some(regression) = detector.check_regression("operation", 115.0) {
    println!("REGRESSION: {}% slower!", regression.percent_change * 100.0);
}
```

### External Profilers

**Linux (perf)**:
```bash
cargo build --release
perf record -g ./target/release/your_binary
perf report
```

**macOS (Instruments)**:
```bash
cargo build --release
xcrun xctrace record --template 'Time Profiler' \
    --launch ./target/release/your_binary
```

**Flamegraphs**:
```bash
cargo install flamegraph
cargo flamegraph --bin your_binary
```

## Memory Optimization

### 1. Use the Tensor Pool

The tensor pool reuses memory allocations:

```rust
use mielin_tensor::pool::{TensorPool, TENSOR_POOL};

// Automatic pooling (recommended)
let buffer = TENSOR_POOL.allocate_aligned(1024, 64);

// ... use buffer ...

// Automatically returned to pool when dropped
drop(buffer);

// Check pool statistics
let stats = TENSOR_POOL.stats();
println!("Hit rate: {:.1}%", stats.hit_rate * 100.0);
println!("Bytes saved: {}", stats.bytes_saved);
```

**Configuration**:
```rust
// Adjust pool limits for your workload
TENSOR_POOL.set_max_cached_bytes(100 * 1024 * 1024); // 100 MB

// Clear cache to free memory
TENSOR_POOL.clear();

// Trim unused memory
TENSOR_POOL.trim();
```

### 2. In-Place Operations

Avoid unnecessary allocations with views:

```rust
use mielin_tensor::{Tensor, TensorViewMut};

// BAD: Creates new tensor
let result = tensor.scale(2.0);

// GOOD: In-place modification
let mut tensor = Tensor::ones(vec![1000, 1000]);
for val in tensor.data_mut() {
    *val *= 2.0;
}

// BETTER: Using mutable views
let mut view = TensorViewMut::new(&mut tensor, &[0..500, 0..500]);
// Modify view in-place
```

### 3. Avoid Small Allocations

Batch operations to amortize allocation overhead:

```rust
// BAD: Many small allocations
for i in 0..1000 {
    let temp = Tensor::zeros(vec![10]);
    // ... use temp ...
}

// GOOD: Reuse allocation
let mut temp = Tensor::zeros(vec![10]);
for i in 0..1000 {
    // Reuse temp
    for val in temp.data_mut() {
        *val = 0.0; // Reset
    }
    // ... use temp ...
}
```

### 4. Memory Layout

Choose appropriate shapes for cache efficiency:

```rust
// BAD: Row-major access of column-major data
// Causes cache misses
for col in 0..cols {
    for row in 0..rows {
        process(matrix[row][col]); // Non-contiguous access
    }
}

// GOOD: Row-major access of row-major data
// Sequential, cache-friendly
for row in 0..rows {
    for col in 0..cols {
        process(matrix[row][col]); // Contiguous access
    }
}
```

### 5. Cache-Blocked Algorithms

Use blocked algorithms for large matrices:

```rust
use mielin_tensor::cache::{blocked_matmul, CacheConfig};

let config = CacheConfig::default(); // Optimized for typical CPUs

let result = blocked_matmul(&a, &b, &config);
```

**Manual blocking**:
```rust
const BLOCK_SIZE: usize = 64; // Fits in L1 cache

for bi in (0..n).step_by(BLOCK_SIZE) {
    for bj in (0..m).step_by(BLOCK_SIZE) {
        for bk in (0..p).step_by(BLOCK_SIZE) {
            // Process BLOCK_SIZE x BLOCK_SIZE submatrix
            let block_end_i = (bi + BLOCK_SIZE).min(n);
            let block_end_j = (bj + BLOCK_SIZE).min(m);
            let block_end_k = (bk + BLOCK_SIZE).min(p);

            for i in bi..block_end_i {
                for j in bj..block_end_j {
                    for k in bk..block_end_k {
                        c[i][j] += a[i][k] * b[k][j];
                    }
                }
            }
        }
    }
}
```

## Compute Optimization

### 1. Choose the Right Algorithm

Different algorithms have different complexity:

| Operation | Naive | Optimized | When to Use |
|-----------|-------|-----------|-------------|
| Matrix Multiply | O(n³) | O(n²·⁸¹) Strassen | n > 1024 |
| Matrix Inverse | O(n³) Gauss | O(n³) LU | Always use LU |
| Eigenvalues | O(n³) Power | O(n³) QR | QR for all eigenvalues |
| Convolution | O(n²m²) | O(n²log m) FFT | Kernel size > 7×7 |

Example: Choosing matrix multiplication algorithm:

```rust
pub fn matmul_optimized(a: &Tensor, b: &Tensor) -> Tensor {
    let (m, n) = (a.shape()[0], a.shape()[1]);
    let p = b.shape()[1];

    if m < 64 && n < 64 && p < 64 {
        // Small matrices: simple algorithm
        matmul_simple(a, b)
    } else if m > 1024 && n > 1024 && p > 1024 {
        // Large matrices: Strassen or cache-blocked
        blocked_matmul(a, b, &CacheConfig::default())
    } else {
        // Medium matrices: SIMD-optimized
        matmul_simd(a, b)
    }
}
```

### 2. Vectorization

Always use SIMD when possible (see [SIMD_GUIDE.md](./SIMD_GUIDE.md)):

```rust
use mielin_hal::capabilities::HardwareCapabilities;

let caps = HardwareCapabilities::detect();

if caps.contains(HardwareCapabilities::AVX512) {
    // 16x f32 per instruction
    process_avx512(data)
} else if caps.contains(HardwareCapabilities::AVX2) {
    // 8x f32 per instruction
    process_avx2(data)
} else if caps.contains(HardwareCapabilities::NEON) {
    // 4x f32 per instruction
    process_neon(data)
} else {
    // 1x f32 per instruction
    process_scalar(data)
}
```

### 3. Parallelization

For large workloads, use multi-threading (requires `std`):

```rust
#[cfg(feature = "std")]
use rayon::prelude::*;

// Parallel matrix multiplication (row-wise parallelism)
#[cfg(feature = "std")]
pub fn matmul_parallel(a: &Tensor, b: &Tensor) -> Tensor {
    let (m, n) = (a.shape()[0], a.shape()[1]);
    let p = b.shape()[1];

    let mut result = Tensor::zeros(vec![m, p]);

    result.data_mut()
        .par_chunks_mut(p)
        .enumerate()
        .for_each(|(i, row)| {
            for j in 0..p {
                let mut sum = 0.0;
                for k in 0..n {
                    sum += a.data()[i * n + k] * b.data()[k * p + j];
                }
                row[j] = sum;
            }
        });

    result
}
```

### 4. Fusion of Operations

Combine multiple operations to reduce memory traffic:

```rust
// BAD: Three passes over data
let temp1 = a.add(&b);      // Pass 1
let temp2 = temp1.mul(&c);  // Pass 2
let result = temp2.scale(2.0); // Pass 3

// GOOD: Single fused pass
let mut result = Tensor::zeros(a.shape().to_vec());
for i in 0..a.size() {
    result.data_mut()[i] = (a.data()[i] + b.data()[i]) * c.data()[i] * 2.0;
}
```

### 5. Avoid Unnecessary Copies

Use views and borrows:

```rust
// BAD: Creates copies
fn process_submatrix(matrix: Tensor) -> Tensor {
    let sub = matrix.clone(); // Unnecessary copy
    // ... process ...
    sub
}

// GOOD: Uses views
fn process_submatrix_view(matrix: &Tensor) -> Tensor {
    let view = TensorView::new(matrix, &[0..10, 0..10]);
    // ... process view ...
    view.to_owned() // Only copy if needed
}
```

## Hardware-Specific Tuning

### ARM (NEON, SVE2)

**Optimization Tips**:
- Use FMA (fused multiply-add) extensively
- Align data to 16-byte boundaries for NEON
- Unroll loops 2x for better throughput
- Use `vld1q_f32_x4` for loading 4 vectors at once

```rust
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn optimized_neon(data: &[f32]) {
    use core::arch::aarch64::*;

    // Load 4 vectors at once (better instruction pipelining)
    let v0 = vld1q_f32(data.as_ptr().add(0));
    let v1 = vld1q_f32(data.as_ptr().add(4));
    let v2 = vld1q_f32(data.as_ptr().add(8));
    let v3 = vld1q_f32(data.as_ptr().add(12));

    // Process all 4 vectors (better parallelism)
    let r0 = vmulq_f32(v0, v0);
    let r1 = vmulq_f32(v1, v1);
    let r2 = vmulq_f32(v2, v2);
    let r3 = vmulq_f32(v3, v3);

    // ... store results ...
}
```

### Intel (AVX2, AVX-512)

**Optimization Tips**:
- Use AVX-512 reductions for horizontal sums
- Prefer FMA over separate mul+add
- Avoid unnecessary `_mm256_extract_*` operations
- Use masked operations in AVX-512 for conditionals

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn optimized_avx512(data: &[f32]) -> f32 {
    use core::arch::x86_64::*;

    let mut sum = _mm512_setzero_ps();

    // Process 64 elements (4 x 16) per iteration
    for i in (0..data.len()).step_by(64) {
        let v0 = _mm512_loadu_ps(data.as_ptr().add(i + 0));
        let v1 = _mm512_loadu_ps(data.as_ptr().add(i + 16));
        let v2 = _mm512_loadu_ps(data.as_ptr().add(i + 32));
        let v3 = _mm512_loadu_ps(data.as_ptr().add(i + 48));

        sum = _mm512_add_ps(sum, v0);
        sum = _mm512_add_ps(sum, v1);
        sum = _mm512_add_ps(sum, v2);
        sum = _mm512_add_ps(sum, v3);
    }

    // Efficient horizontal reduction in AVX-512
    _mm512_reduce_add_ps(sum)
}
```

### Cache Hierarchy

Understand your platform's cache sizes:

| Platform | L1 | L2 | L3 |
|----------|----|----|-----|
| ARM Cortex-A76 | 64 KB | 512 KB | 4 MB |
| Intel Core i7 | 32 KB | 256 KB | 8 MB |
| Apple M1 | 128 KB | 12 MB | - |
| AMD Zen 3 | 32 KB | 512 KB | 32 MB |

**Tuning for cache**:
```rust
use mielin_tensor::cache::{L1_CACHE_SIZE, L2_CACHE_SIZE, L3_CACHE_SIZE};

// Size working set to fit in L1
const TILE_SIZE: usize = L1_CACHE_SIZE / (2 * size_of::<f32>());

// Use cache-blocked algorithms
let config = CacheConfig {
    l1_block_size: 32,  // Fits in L1
    l2_block_size: 128, // Fits in L2
    l3_block_size: 512, // Fits in L3
};
```

## Tensor Operations Optimization

### Matrix Multiplication

**Best Practices**:
```rust
use mielin_tensor::cache::blocked_matmul;

// Use cache-blocked algorithm for large matrices
let result = blocked_matmul(&a, &b, &CacheConfig::default());

// For small matrices, simple algorithm is faster
if a.shape()[0] < 32 {
    let result = simple_matmul(&a, &b);
}
```

### Convolution

**Optimization strategies**:

```rust
use mielin_tensor::conv::ConvOps;

// For large kernels, consider FFT-based convolution
if kernel_size > 7 {
    result = fft_conv(&input, &kernel); // O(n log n)
} else {
    result = ConvOps::conv2d(&input, &kernel, 1, 1, PaddingMode::Valid);
}

// Use depthwise separable convolutions when possible
// Reduces complexity from O(h·w·c_in·c_out·k²) to O(h·w·c·k²)
let result = ConvOps::depthwise_conv2d(&input, &kernel, 1, 1);
```

### Activation Functions

**SIMD-optimized activations**:

```rust
use mielin_tensor::activation::Activation;

// ReLU and Leaky ReLU use SIMD
let output = Activation::relu(&input); // AVX2/NEON optimized

// Sigmoid and Tanh use approximations for speed
let output = Activation::sigmoid_fast(&input); // Faster approximation
```

### Pooling

**Efficient pooling**:

```rust
use mielin_tensor::pool::{PoolingMode, global_pool};

// Global pooling is faster than manual reduction
let output = global_pool(&input, PoolingMode::Average);

// For local pooling, batch when possible
let batched = pool_batched(&batch_input, 2, 2, PoolingMode::Max);
```

## Neural Network Inference

### Layer Fusion

Combine layers to reduce memory traffic:

```rust
// BAD: Separate layers
let x = conv2d(input);
let x = batch_norm(x);
let x = relu(x);

// GOOD: Fused convolution + batch norm + activation
let x = fused_conv_bn_relu(input, weights, bn_params);
```

### Batch Processing

Process multiple inputs together:

```rust
// BAD: Process one at a time
for input in inputs {
    let output = model.forward(&input);
    results.push(output);
}

// GOOD: Batch processing
let batched_input = stack_inputs(&inputs); // [batch, ...]
let batched_output = model.forward_batch(&batched_input);
```

### Weight Quantization

Use quantized weights for faster inference:

```rust
use mielin_tensor::quant::{QuantizedTensor, QuantScheme, QuantGranularity};

// Quantize weights to INT8
let quantized_weights = QuantizedTensor::quantize(
    &weights,
    QuantScheme::Symmetric,
    QuantGranularity::PerTensor,
);

// Inference with INT8 (faster)
let output = quantized_matmul(&input, &quantized_weights);
```

## Quantization and Mixed Precision

### INT8 Quantization

**Benefits**:
- 4x memory reduction
- 2-4x faster inference on CPUs
- Minimal accuracy loss (<1% for most models)

```rust
use mielin_tensor::quant::{QuantizedTensor, QuantCalibrator};

// Calibrate quantization parameters
let mut calibrator = QuantCalibrator::new();
for sample in calibration_data {
    calibrator.update(&sample);
}
let params = calibrator.finalize();

// Quantize tensor
let quantized = QuantizedTensor::quantize_with_params(&tensor, params);

// INT8 matrix multiplication
let output = quantized.matmul(&input_quantized);
```

### INT4 Quantization

**Benefits**:
- 8x memory reduction
- Suitable for memory-bound workloads
- Good for embeddings and weights

```rust
use mielin_tensor::quant::Quant4Tensor;

// Quantize to INT4 (4 bits per element)
let quant4 = Quant4Tensor::quantize(&tensor, QuantScheme::Symmetric);

// Uses 2 values per byte
assert_eq!(quant4.packed_data().len(), tensor.size() / 2);
```

### Mixed Precision (FP16/BF16)

**Use cases**:
- **FP16**: 2x memory reduction, good for small values
- **BF16**: Better dynamic range, preferred for training

```rust
use mielin_tensor::mixed_precision::{MixedPrecisionTensor, PrecisionType, F16, BF16};

// Convert to FP16 for inference
let fp16_tensor = MixedPrecisionTensor::new(&tensor, PrecisionType::FP16);

// BF16 for larger dynamic range
let bf16_tensor = MixedPrecisionTensor::new(&tensor, PrecisionType::BF16);

// Convert back to FP32 when needed
let fp32_result = fp16_tensor.to_f32();
```

## Common Performance Pitfalls

### ❌ Pitfall 1: Frequent Small Operations

```rust
// BAD: Many small tensor operations
for i in 0..1000 {
    let a = Tensor::scalar(values[i]);
    let b = Tensor::scalar(values[i + 1]);
    let sum = a.add(&b); // Overhead dominates
    results.push(sum.data()[0]);
}

// GOOD: Batch into single operation
let a = Tensor::from_vec(values[..1000].to_vec(), vec![1000]).unwrap();
let b = Tensor::from_vec(values[1..1001].to_vec(), vec![1000]).unwrap();
let results = a.add(&b);
```

### ❌ Pitfall 2: Ignoring Cache Locality

```rust
// BAD: Column-major access of row-major data
for j in 0..cols {
    for i in 0..rows {
        sum += matrix.get(&[i, j]).unwrap(); // Cache misses!
    }
}

// GOOD: Row-major access
for i in 0..rows {
    for j in 0..cols {
        sum += matrix.get(&[i, j]).unwrap(); // Cache friendly
    }
}
```

### ❌ Pitfall 3: Not Using SIMD

```rust
// BAD: Scalar operations
for i in 0..data.len() {
    result[i] = data[i] * scale;
}

// GOOD: Use hardware acceleration
let tensor = Tensor::from_vec(data, vec![data.len()]).unwrap();
let result_tensor = tensor.scale(scale);
```

### ❌ Pitfall 4: Memory Allocation in Hot Loops

```rust
// BAD: Allocates every iteration
for _ in 0..iterations {
    let temp = Tensor::zeros(vec![1024]); // Allocation!
    // ... use temp ...
}

// GOOD: Allocate once, reuse
let mut temp = Tensor::zeros(vec![1024]);
for _ in 0..iterations {
    // Reuse temp
    for val in temp.data_mut() {
        *val = 0.0; // Reset
    }
    // ... use temp ...
}
```

### ❌ Pitfall 5: Not Profiling

```rust
// ALWAYS profile to find bottlenecks

use mielin_tensor::profiling::PerfMonitor;

let mut monitor = PerfMonitor::new();

monitor.start("operation_a");
operation_a();
monitor.record("operation_a");

monitor.start("operation_b");
operation_b();
monitor.record("operation_b");

// Which is slower?
println!("A: {:.3}ms", monitor.metrics("operation_a").unwrap().avg_time * 1000.0);
println!("B: {:.3}ms", monitor.metrics("operation_b").unwrap().avg_time * 1000.0);
```

## Performance Targets and Benchmarking

### Target Performance (1024×1024 matrices)

| Operation | Scalar | NEON | AVX2 | AVX-512 |
|-----------|--------|------|------|---------|
| Dot Product | 0.5 GFLOPS | 8 GFLOPS | 12 GFLOPS | 20 GFLOPS |
| Matrix Mul | 0.2 GFLOPS | 4 GFLOPS | 8 GFLOPS | 15 GFLOPS |
| Element Add | 2 GB/s | 20 GB/s | 40 GB/s | 60 GB/s |
| ReLU | 2 GB/s | 15 GB/s | 30 GB/s | 50 GB/s |

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Specific benchmark
cargo bench --bench simd_bench

# Comprehensive benchmark
cargo bench --bench comprehensive_bench

# With CPU affinity (Linux)
taskset -c 0 cargo bench --bench simd_bench
```

### Interpreting Results

```
test bench_matmul_avx2 ... bench:   2,345,678 ns/iter (+/- 123,456)
```

Calculations:
- **Operations**: 1024×1024×1024 = ~1.07 billion FLOPs
- **Time**: 2.345 ms
- **GFLOPS**: 1,073,741,824 / 0.002345 ≈ 457 GFLOPS

### Regression Testing

```rust
use mielin_tensor::profiling::RegressionDetector;

let mut detector = RegressionDetector::new(0.05); // 5% threshold

// Baseline from previous version
detector.record_baseline("matmul_1024", 2.5); // ms

// Current version
let current_time = benchmark_matmul();

if let Some(regression) = detector.check_regression("matmul_1024", current_time) {
    panic!("Performance regression detected: {:.1}% slower",
        regression.percent_change * 100.0);
}
```

## Checklist for Optimized Code

- [ ] Profiled to identify bottlenecks
- [ ] Used SIMD for vectorizable operations
- [ ] Minimized memory allocations (use pool)
- [ ] Cache-blocked for large matrices
- [ ] In-place operations where possible
- [ ] Batched small operations
- [ ] Fused operations to reduce passes
- [ ] Used quantization for memory-bound workloads
- [ ] Tested on target hardware
- [ ] Set up regression detection

## Further Reading

- [SIMD Programming Guide](./SIMD_GUIDE.md) - Detailed SIMD optimization
- [Tensor Layout Conventions](./TENSOR_LAYOUT.md) - Memory layout optimization
- [What Every Programmer Should Know About Memory](https://people.freebsd.org/~lstewart/articles/cpumemory.pdf)
- [Agner Fog's Optimization Manuals](https://www.agner.org/optimize/)
- [Intel Optimization Manual](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)

---

**Last Updated**: 2026-01-18
**Maintainers**: COOLJAPAN OU (Team Kitasan)
