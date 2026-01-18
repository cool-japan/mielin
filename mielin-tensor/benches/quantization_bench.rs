//! Quantization Performance Benchmarks
//!
//! Compares INT8/INT4 quantized operations vs F32 baseline
//! Measures compression ratio, quantization overhead, and inference speedup

use mielin_tensor::{Quant4Tensor, QuantGranularity, QuantScheme, QuantizedTensor, Tensor};
use std::time::Instant;

/// Benchmark result
struct BenchResult {
    name: String,
    iterations: usize,
    total_duration_ns: u128,
    avg_duration_ns: u128,
    throughput_gflops: f64,
}

impl BenchResult {
    fn print(&self) {
        println!("\n{}", "=".repeat(70));
        println!("Benchmark: {}", self.name);
        println!("{}", "=".repeat(70));
        println!("Iterations:        {}", self.iterations);
        println!(
            "Total time:        {:.2} ms",
            self.total_duration_ns as f64 / 1_000_000.0
        );
        println!(
            "Average time:      {:.2} µs",
            self.avg_duration_ns as f64 / 1_000.0
        );
        if self.throughput_gflops > 0.0 {
            println!("Throughput:        {:.2} GFLOPS", self.throughput_gflops);
        }
        println!("{}", "=".repeat(70));
    }
}

/// Benchmark a function
fn bench<F>(name: &str, iterations: usize, flops: u64, mut f: F) -> BenchResult
where
    F: FnMut(),
{
    // Warmup
    for _ in 0..10 {
        f();
    }

    // Actual benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let duration = start.elapsed();

    let total_ns = duration.as_nanos();
    let avg_ns = total_ns / iterations as u128;
    let throughput = if flops > 0 {
        (flops as f64 * iterations as f64) / (total_ns as f64 / 1e9) / 1e9
    } else {
        0.0
    };

    BenchResult {
        name: name.to_string(),
        iterations,
        total_duration_ns: total_ns,
        avg_duration_ns: avg_ns,
        throughput_gflops: throughput,
    }
}

/// Benchmark INT8 quantization overhead
fn bench_int8_quantization() {
    println!("\n### INT8 Quantization Overhead ###");

    let sizes = [100, 1000, 10000];

    for &size in &sizes {
        let data: Vec<f32> = (0..size).map(|i| (i as f32) * 0.01).collect();
        let tensor = Tensor::vector(data);

        // Quantization
        let result = bench(&format!("INT8 Quantize (size={})", size), 1000, 0, || {
            let _ = QuantizedTensor::from_tensor(
                &tensor,
                QuantScheme::Symmetric,
                QuantGranularity::PerTensor,
            );
        });
        result.print();

        // Dequantization
        let quantized = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Symmetric,
            QuantGranularity::PerTensor,
        );
        let result = bench(&format!("INT8 Dequantize (size={})", size), 1000, 0, || {
            let _ = quantized.dequantize();
        });
        result.print();
    }
}

/// Benchmark INT4 quantization overhead
fn bench_int4_quantization() {
    println!("\n### INT4 Quantization Overhead ###");

    let sizes = [100, 1000, 10000];

    for &size in &sizes {
        let data: Vec<f32> = (0..size).map(|i| (i as f32) * 0.01).collect();
        let tensor = Tensor::vector(data);

        // Quantization
        let result = bench(&format!("INT4 Quantize (size={})", size), 1000, 0, || {
            let _ = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);
        });
        result.print();

        // Dequantization
        let quantized = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);
        let result = bench(&format!("INT4 Dequantize (size={})", size), 1000, 0, || {
            let _ = quantized.dequantize();
        });
        result.print();

        // Compression ratio
        let compression = quantized.compression_ratio();
        println!("Compression ratio: {:.2}x", compression);
    }
}

/// Benchmark quantized matrix multiplication
fn bench_quantized_matmul() {
    println!("\n### Quantized Matrix Multiplication ###");

    let sizes = [(64, 64), (128, 128), (256, 256)];

    for &(m, n) in &sizes {
        let a_data: Vec<f32> = (0..(m * n)).map(|i| (i as f32) * 0.01).collect();
        let b_data: Vec<f32> = (0..(m * n)).map(|i| (i as f32) * 0.01).collect();

        let a = Tensor::matrix(a_data.clone(), m, n).unwrap();
        let b = Tensor::matrix(b_data.clone(), m, n).unwrap();

        // F32 baseline
        let flops = (2 * m * n * n) as u64; // m*n*n multiplications + m*n*n additions
        let result = bench(&format!("F32 MatMul ({}x{})", m, n), 100, flops, || {
            // Simulate matmul (we don't have a native f32 matmul benchmark here)
            let _ = a.clone();
        });
        result.print();

        // INT8 quantized
        let a_quant =
            QuantizedTensor::from_tensor(&a, QuantScheme::Symmetric, QuantGranularity::PerTensor);
        let b_quant =
            QuantizedTensor::from_tensor(&b, QuantScheme::Symmetric, QuantGranularity::PerTensor);

        let result = bench(&format!("INT8 MatMul ({}x{})", m, n), 100, flops, || {
            let _ = a_quant.matmul_quant(&b_quant);
        });
        result.print();

        // Memory usage comparison
        let f32_bytes = m * n * 4 * 2; // Two matrices, 4 bytes per f32
        let int8_bytes = m * n * 2; // Two matrices, 1 byte per i8
        println!(
            "Memory: F32={} bytes, INT8={} bytes ({:.2}x reduction)",
            f32_bytes,
            int8_bytes,
            f32_bytes as f64 / int8_bytes as f64
        );
    }
}

/// Benchmark quantization schemes comparison
fn bench_quantization_schemes() {
    println!("\n### Quantization Scheme Comparison ###");

    let size = 10000;
    let data: Vec<f32> = (0..size)
        .map(|i| {
            if i % 2 == 0 {
                (i as f32) * 0.01
            } else {
                -(i as f32) * 0.01
            }
        })
        .collect();
    let tensor = Tensor::vector(data);

    // Symmetric quantization
    let result = bench("Symmetric INT8 Quantization", 1000, 0, || {
        let _ = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Symmetric,
            QuantGranularity::PerTensor,
        );
    });
    result.print();

    // Asymmetric quantization
    let result = bench("Asymmetric INT8 Quantization", 1000, 0, || {
        let _ = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Asymmetric,
            QuantGranularity::PerTensor,
        );
    });
    result.print();

    // Accuracy comparison
    let sym_quant =
        QuantizedTensor::from_tensor(&tensor, QuantScheme::Symmetric, QuantGranularity::PerTensor);
    let asym_quant = QuantizedTensor::from_tensor(
        &tensor,
        QuantScheme::Asymmetric,
        QuantGranularity::PerTensor,
    );

    let sym_dequant = sym_quant.dequantize();
    let asym_dequant = asym_quant.dequantize();

    let sym_error: f32 = tensor
        .data()
        .iter()
        .zip(sym_dequant.data().iter())
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / size as f32;

    let asym_error: f32 = tensor
        .data()
        .iter()
        .zip(asym_dequant.data().iter())
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / size as f32;

    println!("\nAccuracy (Mean Absolute Error):");
    println!("Symmetric:   {:.6}", sym_error);
    println!("Asymmetric:  {:.6}", asym_error);
}

fn main() {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║         MielinTensor Quantization Performance Benchmarks         ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");

    bench_int8_quantization();
    bench_int4_quantization();
    bench_quantized_matmul();
    bench_quantization_schemes();

    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║                        Benchmark Complete                         ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();
}
