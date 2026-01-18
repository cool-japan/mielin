//! End-to-End Tensor Computation Example
//!
//! This comprehensive example demonstrates tensor operations across multiple backends:
//! - CPU execution with SIMD acceleration (NEON, SVE2, AVX2, AVX-512)
//! - Performance benchmarking across backends
//! - Quantization (INT8, INT4) for efficient inference
//! - Neural network inference with various layers
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example e2e-tensor-compute
//! ```
//!
//! ## Backends Tested
//!
//! 1. **Scalar**: Pure Rust implementation, no SIMD
//! 2. **NEON**: Arm Advanced SIMD (128-bit vectors)
//! 3. **AVX2**: Intel Advanced Vector Extensions 2 (256-bit vectors)
//! 4. **AVX-512**: Intel AVX-512 (512-bit vectors)
//!
//! ## Operations Demonstrated
//!
//! - Matrix multiplication (GEMM)
//! - Convolution (2D)
//! - Element-wise operations (add, mul, relu)
//! - Reduction operations (sum, mean, max)
//! - Neural network layers (Dense, Conv2D, BatchNorm)
//! - Quantization and dequantization

use anyhow::Result;
use mielin_hal::capabilities::{HardwareCapabilities, HardwareProfile};
use mielin_tensor::{
    activation::Activation, conv::ConvOps, conv::PaddingMode, nn::Dense, quant::Quant4Tensor,
    quant::QuantGranularity, quant::QuantScheme, quant::QuantizedTensor, Matrix, Tensor,
    TensorRuntime,
};
use std::time::{Duration, Instant};
use tracing::info;

/// Benchmark result for a specific backend
#[derive(Debug, Clone)]
struct BenchmarkResult {
    backend: &'static str,
    operation: String,
    duration: Duration,
    throughput_gflops: f64,
}

impl BenchmarkResult {
    fn new(backend: &'static str, operation: &str, duration: Duration, flops: f64) -> Self {
        let throughput_gflops = if duration.as_secs_f64() > 0.0 {
            (flops / duration.as_secs_f64()) / 1e9
        } else {
            0.0
        };
        Self {
            backend,
            operation: operation.to_string(),
            duration,
            throughput_gflops,
        }
    }

    fn display(&self) {
        info!(
            "   {:<12} | {:<20} | {:>10.3}ms | {:>8.2} GFLOPS",
            self.backend,
            self.operation,
            self.duration.as_secs_f64() * 1000.0,
            self.throughput_gflops
        );
    }
}

/// Test matrix multiplication across backends
fn benchmark_matmul(
    runtime: &TensorRuntime,
    backend: &'static str,
    m: usize,
    n: usize,
    k: usize,
) -> Result<BenchmarkResult> {
    // Create random matrices
    let a = Tensor::zeros(vec![m, k]);
    let b = Tensor::zeros(vec![k, n]);

    // Warmup
    let _ = runtime.ops().matmul(&a, &b);

    // Benchmark
    let start = Instant::now();
    let iterations = 10;

    for _ in 0..iterations {
        let _ = runtime.ops().matmul(&a, &b);
    }

    let duration = start.elapsed() / iterations as u32;

    // FLOPs: 2 * m * n * k (multiply-add)
    let flops = (2 * m * n * k) as f64;

    Ok(BenchmarkResult::new(
        backend,
        &format!("MatMul {}x{}", m, n),
        duration,
        flops,
    ))
}

/// Test 2D convolution
fn benchmark_conv2d(_runtime: &TensorRuntime, backend: &'static str) -> Result<BenchmarkResult> {
    // Input: 32x32 (single channel 32x32 image)
    let input = Tensor::zeros(vec![32, 32]);
    let kernel = Tensor::zeros(vec![3, 3]);

    // Warmup
    let _ = ConvOps::conv2d(&input, &kernel, (1, 1), PaddingMode::Valid);

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let _ = ConvOps::conv2d(&input, &kernel, (1, 1), PaddingMode::Valid);
    }

    let duration = start.elapsed() / iterations as u32;

    // FLOPs: output_size * kernel_size * 2 (multiply-add)
    let output_size = 30 * 30; // (32 - 3 + 1) = 30
    let flops = (output_size * 9 * 2) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "Conv2D (32x32)",
        duration,
        flops,
    ))
}

/// Test element-wise operations
fn benchmark_elementwise(
    runtime: &TensorRuntime,
    backend: &'static str,
    size: usize,
) -> Result<BenchmarkResult> {
    let a = Tensor::zeros(vec![size]);
    let b = Tensor::zeros(vec![size]);

    // Warmup
    let _ = runtime.ops().add(&a, &b);

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let _ = runtime.ops().add(&a, &b);
        let _ = runtime.ops().mul(&a, &b);
    }

    let duration = start.elapsed() / iterations as u32;

    // FLOPs: 2 * size (add + mul)
    let flops = (2 * size) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "Element-wise (1M)",
        duration,
        flops,
    ))
}

/// Test reduction operations
fn benchmark_reduction(
    _runtime: &TensorRuntime,
    backend: &'static str,
    size: usize,
) -> Result<BenchmarkResult> {
    let a = Tensor::zeros(vec![size]);

    // Warmup
    let _ = a.sum();

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let _ = a.sum();
    }

    let duration = start.elapsed() / iterations as u32;

    // FLOPs: sum ≈ size operations
    let flops = size as f64;

    Ok(BenchmarkResult::new(backend, "Reduction", duration, flops))
}

/// Test neural network inference
fn benchmark_nn_inference(backend: &'static str) -> Result<BenchmarkResult> {
    // Create a simple 2-layer neural network
    // Input: 784 (28x28 image) -> Hidden: 256 -> Output: 10 (classes)

    let input = Tensor::zeros(vec![784]);

    let dense1 = Dense::new(784, 256);
    let dense2 = Dense::new(256, 10);

    // Warmup
    let hidden = dense1.forward(input.data());
    let _ = dense2.forward(&hidden);

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let hidden = dense1.forward(input.data());
        let _ = dense2.forward(&hidden);
    }

    let duration = start.elapsed() / iterations as u32;

    // FLOPs: (784*256 + 256*10) * 2 (multiply-add)
    let flops = ((784 * 256 + 256 * 10) * 2) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "NN Inference",
        duration,
        flops,
    ))
}

/// Test quantization performance
fn benchmark_quantization(backend: &'static str) -> Result<BenchmarkResult> {
    // Create a tensor to quantize
    let tensor = Tensor::zeros(vec![1000, 1000]);

    // Benchmark INT8 quantization
    let start = Instant::now();
    let iterations = 10;

    for _ in 0..iterations {
        let quantized = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Asymmetric,
            QuantGranularity::PerTensor,
        );
        let _ = quantized.dequantize();
    }

    let duration = start.elapsed() / iterations as u32;

    // Operations: quantize + dequantize ≈ 2 ops per element
    let flops = (2 * 1000 * 1000) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "Quantization (INT8)",
        duration,
        flops,
    ))
}

/// Test INT4 quantization
fn benchmark_int4_quantization(backend: &'static str) -> Result<BenchmarkResult> {
    // Create a tensor to quantize
    let tensor = Tensor::zeros(vec![1000, 1000]);

    // Benchmark INT4 quantization
    let start = Instant::now();
    let iterations = 10;

    for _ in 0..iterations {
        let quantized = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);
        let _ = quantized.dequantize();
    }

    let duration = start.elapsed() / iterations as u32;

    // Operations: quantize + dequantize ≈ 2 ops per element
    let flops = (2 * 1000 * 1000) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "Quantization (INT4)",
        duration,
        flops,
    ))
}

/// Test activation functions
fn benchmark_activations(backend: &'static str) -> Result<BenchmarkResult> {
    let tensor = Tensor::zeros(vec![100_000]);
    let activation = Activation::new(HardwareCapabilities::NONE);

    // Warmup
    let _ = activation.relu(&tensor);

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let _ = activation.relu(&tensor);
        let _ = activation.sigmoid(&tensor);
        let _ = activation.tanh(&tensor);
    }

    let duration = start.elapsed() / iterations as u32;

    // Operations: 3 activations * 100k elements
    let flops = (3 * 100_000) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "Activations",
        duration,
        flops,
    ))
}

/// Test matrix operations
fn benchmark_matrix_ops(backend: &'static str) -> Result<BenchmarkResult> {
    let mat =
        Tensor::from_vec(vec![1.0f32; 256 * 256], vec![256, 256]).expect("Failed to create tensor");

    // Warmup
    let _ = Matrix::transpose(&mat);

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let _ = Matrix::transpose(&mat);
    }

    let duration = start.elapsed() / iterations as u32;

    // Operations: N^2 reads and writes
    let flops = (256 * 256 * 2) as f64;

    Ok(BenchmarkResult::new(
        backend,
        "Transpose (256x256)",
        duration,
        flops,
    ))
}

/// Run comprehensive benchmarks for a backend
fn run_backend_benchmarks(
    capabilities: HardwareCapabilities,
    backend_name: &'static str,
) -> Result<Vec<BenchmarkResult>> {
    let runtime = TensorRuntime::new(capabilities);
    let mut results = Vec::new();

    info!("🔬 Testing {} backend", backend_name);
    info!("   Acceleration: {}", runtime.acceleration_info());

    // Matrix multiplication (different sizes)
    results.push(benchmark_matmul(&runtime, backend_name, 256, 256, 256)?);
    results.push(benchmark_matmul(&runtime, backend_name, 512, 512, 512)?);

    // Convolution
    results.push(benchmark_conv2d(&runtime, backend_name)?);

    // Element-wise operations
    results.push(benchmark_elementwise(&runtime, backend_name, 1_000_000)?);

    // Reduction operations
    results.push(benchmark_reduction(&runtime, backend_name, 1_000_000)?);

    // Neural network inference
    results.push(benchmark_nn_inference(backend_name)?);

    // Matrix operations
    results.push(benchmark_matrix_ops(backend_name)?);

    // Activations
    results.push(benchmark_activations(backend_name)?);

    // Quantization
    results.push(benchmark_quantization(backend_name)?);
    results.push(benchmark_int4_quantization(backend_name)?);

    Ok(results)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("╔═══════════════════════════════════════════════════════════════════════════╗");
    info!("║           MielinOS End-to-End Tensor Compute Benchmarks                  ║");
    info!("╚═══════════════════════════════════════════════════════════════════════════╝");

    // Detect hardware capabilities
    let profile = HardwareProfile::detect();
    info!("\n🖥️  Hardware Profile:");
    info!("   Capabilities: {:?}", profile.capabilities);

    let mut all_results = Vec::new();

    // Test Scalar backend
    all_results.extend(run_backend_benchmarks(
        HardwareCapabilities::NONE,
        "Scalar",
    )?);

    // Test NEON backend (ARM)
    if profile.capabilities.contains(HardwareCapabilities::NEON) {
        all_results.extend(run_backend_benchmarks(HardwareCapabilities::NEON, "NEON")?);
    }

    // Test AVX2 backend (x86_64)
    if profile.capabilities.contains(HardwareCapabilities::AVX2) {
        all_results.extend(run_backend_benchmarks(HardwareCapabilities::AVX2, "AVX2")?);
    }

    // Test AVX-512 backend (x86_64)
    if profile.capabilities.contains(HardwareCapabilities::AVX512) {
        all_results.extend(run_backend_benchmarks(
            HardwareCapabilities::AVX512,
            "AVX-512",
        )?);
    }

    // Print summary
    info!("\n📊 Benchmark Results Summary");
    info!(
        "   {:<12} | {:<20} | {:>10} | {:>12}",
        "Backend", "Operation", "Time (ms)", "GFLOPS"
    );
    info!("   {}", "-".repeat(60));

    for result in &all_results {
        result.display();
    }

    info!("\n✅ Benchmarks completed successfully!");

    Ok(())
}
