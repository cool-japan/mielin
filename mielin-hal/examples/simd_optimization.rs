//! SIMD Optimization Example
//!
//! This example demonstrates how to use mielin-hal to detect hardware
//! capabilities and select the optimal implementation for performance-critical
//! operations like matrix multiplication.
//!
//! Run with:
//! ```bash
//! cargo run --example simd_optimization --release
//! ```

use mielin_hal::capabilities::{HardwareCapabilities, HardwareProfile};
use mielin_hal::runtime::{FallbackChain, FeatureRequirement, RuntimeSelector};
use std::time::Instant;

/// Matrix multiplication - scalar implementation (fallback)
fn matmul_scalar(a: &[f32], b: &[f32], c: &mut [f32], n: usize) {
    for i in 0..n {
        for j in 0..n {
            let mut sum = 0.0;
            for k in 0..n {
                sum += a[i * n + k] * b[k * n + j];
            }
            c[i * n + j] = sum;
        }
    }
}

/// Matrix multiplication - cache-optimized implementation
fn matmul_cache_optimized(a: &[f32], b: &[f32], c: &mut [f32], n: usize) {
    const BLOCK_SIZE: usize = 64;

    for i in (0..n).step_by(BLOCK_SIZE) {
        for j in (0..n).step_by(BLOCK_SIZE) {
            for k in (0..n).step_by(BLOCK_SIZE) {
                let i_max = (i + BLOCK_SIZE).min(n);
                let j_max = (j + BLOCK_SIZE).min(n);
                let k_max = (k + BLOCK_SIZE).min(n);

                for ii in i..i_max {
                    for jj in j..j_max {
                        let mut sum = c[ii * n + jj];
                        for kk in k..k_max {
                            sum += a[ii * n + kk] * b[kk * n + jj];
                        }
                        c[ii * n + jj] = sum;
                    }
                }
            }
        }
    }
}

/// Matrix multiplication - SIMD-friendly implementation
#[cfg(target_arch = "x86_64")]
fn matmul_simd(a: &[f32], b: &[f32], c: &mut [f32], n: usize) {
    // This would use actual SIMD intrinsics in production
    // For this example, we'll use the cache-optimized version
    matmul_cache_optimized(a, b, c, n);
}

#[cfg(not(target_arch = "x86_64"))]
fn matmul_simd(a: &[f32], b: &[f32], c: &mut [f32], n: usize) {
    matmul_cache_optimized(a, b, c, n);
}

fn main() {
    println!("=== SIMD Optimization Example ===\n");

    // Detect hardware capabilities
    let profile = HardwareProfile::detect();

    println!("Hardware Profile:");
    println!("  Architecture: {:?}", profile.architecture);
    println!("  Capabilities: {:?}", profile.capabilities);
    println!("  Core count: {}", profile.core_count);
    println!("  Cache line size: {} bytes", profile.cache_line_size);
    println!("  L1 cache: {} KB", profile.l1_cache_size / 1024);
    println!("  L2 cache: {} KB", profile.l2_cache_size / 1024);
    println!("  L3 cache: {} KB\n", profile.l3_cache_size / 1024);

    // Create a fallback chain for matrix multiplication
    let mut chain = FallbackChain::new("matmul");

    #[cfg(target_arch = "x86_64")]
    {
        // Prefer AVX512 if available
        chain.add_requirement(
            FeatureRequirement::all_of(&[HardwareCapabilities::AVX512])
                .with_min_cache_size(256 * 1024), // Require at least 256KB L2 cache
        );

        // Fall back to AVX2
        chain.add_requirement(
            FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])
                .with_min_cache_size(128 * 1024),
        );

        // Fall back to SSE4.2
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::SSE4_2]));
    }

    #[cfg(target_arch = "aarch64")]
    {
        // Prefer SVE2 if available
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::SVE2]));

        // Fall back to NEON
        chain.add_requirement(FeatureRequirement::all_of(&[HardwareCapabilities::NEON]));
    }

    // Always have a scalar fallback
    chain.add_requirement(FeatureRequirement::none());

    // Select the best implementation
    let selector = RuntimeSelector::new();
    let selected = selector
        .select(&chain)
        .expect("Failed to select implementation");

    println!("Runtime Selection:");
    println!("  Selected: {}", selected.name);
    println!("  Priority: {}\n", selected.priority);

    // Benchmark different implementations
    let n = 256;
    let mut a = vec![0.0f32; n * n];
    let mut b = vec![0.0f32; n * n];
    let mut c = vec![0.0f32; n * n];

    // Initialize matrices with random-ish data
    for i in 0..n * n {
        a[i] = (i % 100) as f32 / 100.0;
        b[i] = ((i * 7) % 100) as f32 / 100.0;
    }

    println!("Benchmarking {}x{} matrix multiplication:\n", n, n);

    // Benchmark scalar implementation
    c.fill(0.0);
    let start = Instant::now();
    matmul_scalar(&a, &b, &mut c, n);
    let scalar_time = start.elapsed();
    println!(
        "  Scalar:           {:>8.2} ms",
        scalar_time.as_secs_f64() * 1000.0
    );

    // Benchmark cache-optimized implementation
    c.fill(0.0);
    let start = Instant::now();
    matmul_cache_optimized(&a, &b, &mut c, n);
    let cache_time = start.elapsed();
    println!(
        "  Cache-optimized:  {:>8.2} ms  ({:.2}x faster)",
        cache_time.as_secs_f64() * 1000.0,
        scalar_time.as_secs_f64() / cache_time.as_secs_f64()
    );

    // Benchmark SIMD implementation
    c.fill(0.0);
    let start = Instant::now();
    matmul_simd(&a, &b, &mut c, n);
    let simd_time = start.elapsed();
    println!(
        "  SIMD-friendly:    {:>8.2} ms  ({:.2}x faster)",
        simd_time.as_secs_f64() * 1000.0,
        scalar_time.as_secs_f64() / simd_time.as_secs_f64()
    );

    println!("\n=== Performance Analysis ===\n");

    // Calculate theoretical cache efficiency
    let matrix_size_bytes = n * n * 4; // f32 = 4 bytes
    let working_set = matrix_size_bytes * 3; // 3 matrices

    println!("Working set: {} KB", working_set / 1024);

    if profile.l1_cache_size > 0 {
        let l1_utilization = (working_set as f64 / profile.l1_cache_size as f64) * 100.0;
        println!("L1 cache utilization: {:.1}%", l1_utilization);
    }

    if profile.l2_cache_size > 0 {
        let l2_utilization = (working_set as f64 / profile.l2_cache_size as f64) * 100.0;
        println!("L2 cache utilization: {:.1}%", l2_utilization);
    }

    if profile.l3_cache_size > 0 {
        let l3_utilization = (working_set as f64 / profile.l3_cache_size as f64) * 100.0;
        println!("L3 cache utilization: {:.1}%", l3_utilization);
    }

    // Performance recommendations
    println!("\n=== Recommendations ===\n");

    if profile.capabilities.contains(HardwareCapabilities::AVX512) {
        println!("✓ AVX-512 available - consider using SIMD intrinsics for maximum performance");
    } else if profile.capabilities.contains(HardwareCapabilities::AVX2) {
        println!("✓ AVX2 available - consider using 256-bit SIMD");
    } else if profile.capabilities.contains(HardwareCapabilities::NEON) {
        println!("✓ NEON available - consider using ARM SIMD intrinsics");
    }

    if profile.l3_cache_size > 0 && working_set < profile.l3_cache_size {
        println!("✓ Working set fits in L3 cache - good cache locality");
    } else if profile.l2_cache_size > 0 && working_set > profile.l2_cache_size {
        println!("⚠ Working set exceeds L2 cache - consider blocking or tiling");
    }

    if profile.core_count > 1 {
        println!(
            "✓ {} cores available - consider parallelization",
            profile.core_count
        );
    }

    println!(
        "\nOptimal block size for cache: {} x {}",
        (profile.cache_line_size / 4).max(32),
        (profile.cache_line_size / 4).max(32)
    );
}
