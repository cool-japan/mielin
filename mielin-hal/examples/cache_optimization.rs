//! Cache Optimization Example
//!
//! This example demonstrates how to use cache topology information from
//! mielin-hal to optimize data structure layout and access patterns for
//! better cache performance.
//!
//! Run with:
//! ```bash
//! cargo run --example cache_optimization --release
//! ```

use mielin_hal::cache::CacheTopology;
use mielin_hal::capabilities::HardwareProfile;
use std::time::Instant;

/// Array traversal - row-major order (cache-friendly)
fn traverse_row_major(data: &[i32], rows: usize, cols: usize) -> i64 {
    let mut sum = 0i64;
    for i in 0..rows {
        for j in 0..cols {
            sum += data[i * cols + j] as i64;
        }
    }
    sum
}

/// Array traversal - column-major order (cache-unfriendly)
fn traverse_column_major(data: &[i32], rows: usize, cols: usize) -> i64 {
    let mut sum = 0i64;
    for j in 0..cols {
        for i in 0..rows {
            sum += data[i * cols + j] as i64;
        }
    }
    sum
}

/// Struct-of-Arrays (SoA) layout - cache-friendly for column access
struct PointsSoA {
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
}

/// Array-of-Structs (AoS) layout - cache-friendly for full point access
#[derive(Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
    z: f32,
}

fn process_soa(points: &PointsSoA) -> f32 {
    let mut sum = 0.0;
    for i in 0..points.x.len() {
        sum += points.x[i] + points.y[i] + points.z[i];
    }
    sum
}

fn process_aos(points: &[Point]) -> f32 {
    let mut sum = 0.0;
    for point in points {
        sum += point.x + point.y + point.z;
    }
    sum
}

fn main() {
    println!("=== Cache Optimization Example ===\n");

    // Detect cache topology
    let cache = CacheTopology::detect();
    let profile = HardwareProfile::detect();

    println!("Cache Topology:");
    println!("  L1 Data:        {} KB", cache.l1_data.size / 1024);
    println!("  L1 Instruction: {} KB", cache.l1_instruction.size / 1024);
    println!("  L2:             {} KB", cache.l2.size / 1024);
    println!("  L3:             {} KB", cache.l3.size / 1024);
    println!("  Cache line:     {} bytes", cache.default_line_size);
    println!("  L1 associativity: {}-way", cache.l1_data.associativity);
    println!("  L2 associativity: {}-way\n", cache.l2.associativity);

    // Calculate optimal data structure sizes
    let cache_line_elements = cache.default_line_size / 4; // 4 bytes per i32
    println!("Elements per cache line (i32): {}", cache_line_elements);

    let l1_elements = cache.l1_data.size / 4;
    println!("Elements fitting in L1: {}", l1_elements);

    let optimal_array_size = (cache.l1_data.size / 2) / 4; // Use half of L1
    println!(
        "Optimal array size for L1: {} elements\n",
        optimal_array_size
    );

    // Benchmark 1: Row-major vs Column-major traversal
    println!("=== Benchmark 1: Array Traversal Patterns ===\n");

    let rows = 1024;
    let cols = 1024;
    let data: Vec<i32> = (0..(rows * cols)).map(|i| i as i32).collect();

    println!(
        "Array size: {}x{} = {} MB",
        rows,
        cols,
        (rows * cols * 4) / (1024 * 1024)
    );

    // Row-major traversal (cache-friendly)
    let start = Instant::now();
    let sum1 = traverse_row_major(&data, rows, cols);
    let row_time = start.elapsed();

    // Column-major traversal (cache-unfriendly)
    let start = Instant::now();
    let sum2 = traverse_column_major(&data, rows, cols);
    let col_time = start.elapsed();

    println!(
        "  Row-major:    {:>8.2} ms  (sum: {})",
        row_time.as_secs_f64() * 1000.0,
        sum1
    );
    println!(
        "  Column-major: {:>8.2} ms  (sum: {})  ({:.2}x slower)",
        col_time.as_secs_f64() * 1000.0,
        sum2,
        col_time.as_secs_f64() / row_time.as_secs_f64()
    );

    // Estimate cache misses
    let total_accesses = rows * cols;
    let cache_lines_needed = (rows * cols * 4).div_ceil(cache.default_line_size);

    println!("\n  Analysis:");
    println!("    Total accesses: {}", total_accesses);
    println!("    Cache lines needed: {}", cache_lines_needed);
    println!(
        "    Row-major cache misses (est): {} (one per cache line)",
        cache_lines_needed
    );
    println!(
        "    Column-major cache misses (est): {} (one per access if no cache)",
        total_accesses
    );

    // Benchmark 2: SoA vs AoS
    println!("\n=== Benchmark 2: Data Layout Patterns ===\n");

    let n_points = 1_000_000;

    // Create SoA layout
    let points_soa = PointsSoA {
        x: (0..n_points).map(|i| i as f32).collect(),
        y: (0..n_points).map(|i| (i * 2) as f32).collect(),
        z: (0..n_points).map(|i| (i * 3) as f32).collect(),
    };

    // Create AoS layout
    let points_aos: Vec<Point> = (0..n_points)
        .map(|i| Point {
            x: i as f32,
            y: (i * 2) as f32,
            z: (i * 3) as f32,
        })
        .collect();

    let point_size_bytes = n_points * 3 * 4; // 3 floats per point
    println!("Point cloud size: {} MB", point_size_bytes / (1024 * 1024));

    // Benchmark SoA
    let start = Instant::now();
    let sum_soa = process_soa(&points_soa);
    let soa_time = start.elapsed();

    // Benchmark AoS
    let start = Instant::now();
    let sum_aos = process_aos(&points_aos);
    let aos_time = start.elapsed();

    println!(
        "  SoA layout: {:>8.2} ms  (sum: {:.0})",
        soa_time.as_secs_f64() * 1000.0,
        sum_soa
    );
    println!(
        "  AoS layout: {:>8.2} ms  (sum: {:.0})  ({:.2}x {})",
        aos_time.as_secs_f64() * 1000.0,
        sum_aos,
        if soa_time < aos_time {
            aos_time.as_secs_f64() / soa_time.as_secs_f64()
        } else {
            soa_time.as_secs_f64() / aos_time.as_secs_f64()
        },
        if soa_time < aos_time {
            "faster"
        } else {
            "slower"
        }
    );

    println!("\n  Analysis:");
    println!("    SoA: Better for operations on single components (e.g., all X values)");
    println!("    AoS: Better for operations on complete points");
    println!("    Choice depends on access pattern!");

    // Cache-aware recommendations
    println!("\n=== Recommendations ===\n");

    if cache.l1_data.size > 0 {
        let optimal_tile_size = ((cache.l1_data.size / 2) as f64).sqrt() as usize;
        println!(
            "✓ Optimal tile size for L1 cache: {}x{}",
            optimal_tile_size, optimal_tile_size
        );
    }

    if cache.default_line_size > 0 {
        println!(
            "✓ Align critical data structures to {} byte boundaries",
            cache.default_line_size
        );
        println!(
            "✓ Group frequently accessed data within {} byte blocks",
            cache.default_line_size
        );
    }

    if cache.l1_data.associativity > 0 {
        println!(
            "✓ L1 is {}-way associative - avoid stride patterns of {}",
            cache.l1_data.associativity,
            cache.l1_data.size / (cache.l1_data.associativity as usize)
        );
    }

    println!("✓ Prefer sequential memory access patterns");
    println!(
        "✓ Keep working set under {} KB for L1 cache hits",
        cache.l1_data.size / 1024
    );

    use mielin_hal::capabilities::HardwareCapabilities;
    if profile.capabilities.contains(HardwareCapabilities::SIMD) {
        println!("✓ Use SIMD with properly aligned data for best performance");
    }
}
