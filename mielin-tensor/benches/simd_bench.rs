//! SIMD Backend Performance Benchmarks
//!
//! Compares performance across different SIMD backends:
//! - Scalar (baseline)
//! - NEON (ARM)
//! - AVX2 (x86_64)
//! - AVX-512 (x86_64)
//! - SVE2 (ARM)

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_tensor::{Tensor, TensorRuntime};
use std::time::Instant;

struct BenchResult {
    backend: &'static str,
    operation: &'static str,
    size: usize,
    _iterations: usize,
    avg_time_ns: u128,
    throughput_gflops: f64,
}

impl BenchResult {
    fn print(&self) {
        println!(
            "{:12} | {:20} | {:8} | {:8.2} µs | {:8.2} GFLOPS",
            self.backend,
            self.operation,
            self.size,
            self.avg_time_ns as f64 / 1000.0,
            self.throughput_gflops
        );
    }
}

fn bench_backend(
    backend_name: &'static str,
    caps: HardwareCapabilities,
    op_name: &'static str,
    size: usize,
    iterations: usize,
    flops: u64,
    mut f: impl FnMut(&TensorRuntime),
) -> BenchResult {
    let runtime = TensorRuntime::new(caps);

    // Warmup
    for _ in 0..10 {
        f(&runtime);
    }

    // Benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        f(&runtime);
    }
    let duration = start.elapsed();

    let total_ns = duration.as_nanos();
    let avg_ns = total_ns / iterations as u128;
    let throughput = (flops as f64 * iterations as f64) / (total_ns as f64 / 1e9) / 1e9;

    BenchResult {
        backend: backend_name,
        operation: op_name,
        size,
        _iterations: iterations,
        avg_time_ns: avg_ns,
        throughput_gflops: throughput,
    }
}

fn bench_dot_product() {
    println!("\n### Dot Product Benchmarks ###");
    println!(
        "{:12} | {:20} | {:8} | {:>12} | {:>12}",
        "Backend", "Operation", "Size", "Time", "GFLOPS"
    );
    println!("{}", "-".repeat(80));

    let sizes = [100, 1000, 10000, 100000];
    let backends = [
        ("Scalar", HardwareCapabilities::NONE),
        ("NEON", HardwareCapabilities::NEON),
        ("AVX2", HardwareCapabilities::AVX2),
        ("AVX-512", HardwareCapabilities::AVX512),
        ("SVE2", HardwareCapabilities::SVE2),
    ];

    for &size in &sizes {
        let a = Tensor::vector((0..size).map(|i| i as f32 * 0.01).collect());
        let b = Tensor::vector((0..size).map(|i| i as f32 * 0.02).collect());
        let flops = (2 * size) as u64; // multiply + add per element

        for (backend_name, caps) in &backends {
            let result = bench_backend(
                backend_name,
                *caps,
                "dot_product",
                size,
                1000,
                flops,
                |runtime| {
                    let _ = runtime.ops().dot(&a, &b);
                },
            );
            result.print();
        }
        println!("{}", "-".repeat(80));
    }
}

fn bench_element_wise_ops() {
    println!("\n### Element-wise Operation Benchmarks ###");
    println!(
        "{:12} | {:20} | {:8} | {:>12} | {:>12}",
        "Backend", "Operation", "Size", "Time", "GB/s"
    );
    println!("{}", "-".repeat(80));

    let size = 100000;

    let backends = [
        ("Scalar", HardwareCapabilities::NONE),
        ("NEON", HardwareCapabilities::NEON),
        ("AVX2", HardwareCapabilities::AVX2),
    ];

    // Benchmark add operation
    {
        let a = Tensor::vector((0..size).map(|i| i as f32 * 0.01).collect());
        let b = Tensor::vector((0..size).map(|i| i as f32 * 0.02).collect());

        for (backend_name, caps) in &backends {
            let bytes = (size * 12) as u64; // read a, read b, write result
            let a_clone = a.clone();
            let b_clone = b.clone();
            let result = bench_backend(backend_name, *caps, "add", size, 1000, 0, |runtime| {
                let _ = runtime.ops().add(&a_clone, &b_clone);
            });

            let bandwidth_gbs = (bytes as f64 * 1000.0) / (result.avg_time_ns as f64) / 1e9 * 1e9;
            println!(
                "{:12} | {:20} | {:8} | {:8.2} µs | {:8.2}",
                result.backend,
                result.operation,
                result.size,
                result.avg_time_ns as f64 / 1000.0,
                bandwidth_gbs
            );
        }
        println!("{}", "-".repeat(80));
    }

    // Benchmark scale operation (using Tensor method directly since TensorOps doesn't have it)
    {
        let a = Tensor::vector((0..size).map(|i| i as f32 * 0.01).collect());

        for (backend_name, caps) in &backends {
            let bytes = (size * 8) as u64; // read a, write result
            let a_clone = a.clone();
            let result = bench_backend(backend_name, *caps, "scale", size, 1000, 0, |_runtime| {
                let _ = a_clone.scale(2.5);
            });

            let bandwidth_gbs = (bytes as f64 * 1000.0) / (result.avg_time_ns as f64) / 1e9 * 1e9;
            println!(
                "{:12} | {:20} | {:8} | {:8.2} µs | {:8.2}",
                result.backend,
                result.operation,
                result.size,
                result.avg_time_ns as f64 / 1000.0,
                bandwidth_gbs
            );
        }
        println!("{}", "-".repeat(80));
    }
}

fn bench_matrix_ops() {
    println!("\n### Matrix Operation Benchmarks ###");
    println!(
        "{:12} | {:20} | {:8} | {:>12} | {:>12}",
        "Backend", "Operation", "Size", "Time", "GFLOPS"
    );
    println!("{}", "-".repeat(80));

    let sizes = [64, 128, 256];
    let backends = [
        ("Scalar", HardwareCapabilities::NONE),
        ("NEON", HardwareCapabilities::NEON),
        ("AVX2", HardwareCapabilities::AVX2),
    ];

    for &m in &sizes {
        let n = m;
        let a = Tensor::matrix((0..(m * n)).map(|i| i as f32 * 0.01).collect(), m, n).unwrap();
        let b = Tensor::matrix((0..(m * n)).map(|i| i as f32 * 0.02).collect(), m, n).unwrap();
        let flops = (2 * m * n * n) as u64;

        for (backend_name, caps) in &backends {
            let result = bench_backend(backend_name, *caps, "matmul", m, 100, flops, |runtime| {
                let _ = runtime.ops().matmul(&a, &b);
            });
            result.print();
        }
        println!("{}", "-".repeat(80));
    }
}

fn main() {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║            MielinTensor SIMD Backend Benchmarks                  ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");

    bench_dot_product();
    bench_element_wise_ops();
    bench_matrix_ops();

    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║                        Benchmark Complete                         ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();
}
