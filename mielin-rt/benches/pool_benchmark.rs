//! Memory Pool Allocator Benchmarks
//!
//! Benchmarks for measuring memory pool performance characteristics.

#![no_std]
#![no_main]

extern crate alloc;

use mielin_rt::pool::{PoolAllocator, PoolConfig};

/// Benchmark: Allocation performance
pub fn bench_allocation_latency() -> BenchmarkResult {
    let allocator = PoolAllocator::default();
    let iterations = 1000;

    let start = read_cycle_counter();

    for _ in 0..iterations {
        let alloc = allocator.allocate(64).unwrap();
        allocator.deallocate(&alloc).unwrap();
    }

    let end = read_cycle_counter();
    let total_cycles = end - start;
    let cycles_per_alloc = total_cycles / iterations;

    BenchmarkResult {
        name: "Allocation Latency",
        total_cycles,
        iterations,
        cycles_per_op: cycles_per_alloc,
    }
}

/// Benchmark: Different pool sizes
pub fn bench_allocation_sizes() -> [BenchmarkResult; 7] {
    let allocator = PoolAllocator::default();
    let sizes = [16, 32, 64, 128, 256, 512, 1024];
    let iterations = 1000;

    let mut results = [BenchmarkResult::default(); 7];

    for (i, &size) in sizes.iter().enumerate() {
        let start = read_cycle_counter();

        for _ in 0..iterations {
            let alloc = allocator.allocate(size).unwrap();
            allocator.deallocate(&alloc).unwrap();
        }

        let end = read_cycle_counter();
        let total_cycles = end - start;

        results[i] = BenchmarkResult {
            name: "Pool Size",
            total_cycles,
            iterations,
            cycles_per_op: total_cycles / iterations,
        };
    }

    results
}

/// Benchmark: Fragmentation impact
pub fn bench_fragmentation_impact() -> BenchmarkResult {
    let allocator = PoolAllocator::default();
    let iterations = 100;

    // Create fragmented state
    let mut allocations = alloc::vec::Vec::new();
    for i in 0..50 {
        let size = if i % 2 == 0 { 64 } else { 256 };
        if let Ok(alloc) = allocator.allocate(size) {
            allocations.push(alloc);
        }
    }

    // Deallocate every other allocation
    for i in (0..allocations.len()).step_by(2) {
        allocator.deallocate(&allocations[i]).ok();
    }

    // Benchmark allocation in fragmented state
    let start = read_cycle_counter();

    for _ in 0..iterations {
        if let Ok(alloc) = allocator.allocate(128) {
            allocator.deallocate(&alloc).ok();
        }
    }

    let end = read_cycle_counter();

    // Cleanup
    for i in (1..allocations.len()).step_by(2) {
        allocator.deallocate(&allocations[i]).ok();
    }

    BenchmarkResult {
        name: "Fragmented Allocation",
        total_cycles: end - start,
        iterations,
        cycles_per_op: (end - start) / iterations,
    }
}

/// Benchmark: Concurrent access overhead
pub fn bench_concurrent_access() -> BenchmarkResult {
    let allocator = PoolAllocator::default();
    let iterations = 1000;

    let start = read_cycle_counter();

    // Simulate concurrent access patterns
    for _ in 0..iterations {
        let alloc1 = allocator.allocate(64).unwrap();
        let alloc2 = allocator.allocate(128).unwrap();
        let alloc3 = allocator.allocate(32).unwrap();

        allocator.deallocate(&alloc2).unwrap();
        allocator.deallocate(&alloc1).unwrap();
        allocator.deallocate(&alloc3).unwrap();
    }

    let end = read_cycle_counter();

    BenchmarkResult {
        name: "Concurrent Access",
        total_cycles: end - start,
        iterations,
        cycles_per_op: (end - start) / iterations,
    }
}

/// Benchmark: Pool exhaustion handling
pub fn bench_pool_exhaustion() -> BenchmarkResult {
    let config = PoolConfig {
        blocks_per_pool: [8, 8, 8, 8, 8, 8, 8],
        track_statistics: true,
        track_fragmentation: false,
        fragmentation_threshold: 50,
    };
    let mut allocator = PoolAllocator::new(config);
    allocator.init();

    let iterations = 1000;
    let start = read_cycle_counter();

    for _ in 0..iterations {
        // Try to allocate (will fail after pool is exhausted)
        let _ = allocator.allocate(64);
    }

    let end = read_cycle_counter();

    BenchmarkResult {
        name: "Pool Exhaustion",
        total_cycles: end - start,
        iterations,
        cycles_per_op: (end - start) / iterations,
    }
}

/// Benchmark: Integrity checking overhead
pub fn bench_integrity_check() -> BenchmarkResult {
    let allocator = PoolAllocator::default();
    let iterations = 1000;

    // Create some allocations
    let mut allocs = alloc::vec::Vec::new();
    for _ in 0..10 {
        allocs.push(allocator.allocate(128).unwrap());
    }

    let start = read_cycle_counter();

    for _ in 0..iterations {
        allocator.check_integrity().unwrap();
    }

    let end = read_cycle_counter();

    // Cleanup
    for alloc in allocs {
        allocator.deallocate(&alloc).unwrap();
    }

    BenchmarkResult {
        name: "Integrity Check",
        total_cycles: end - start,
        iterations,
        cycles_per_op: (end - start) / iterations,
    }
}

/// Benchmark: Safe vs unsafe deallocation
pub fn bench_safe_vs_unsafe_dealloc() -> (BenchmarkResult, BenchmarkResult) {
    let allocator = PoolAllocator::default();
    let iterations = 1000;

    // Benchmark normal deallocation
    let start = read_cycle_counter();
    for _ in 0..iterations {
        let alloc = allocator.allocate(64).unwrap();
        allocator.deallocate(&alloc).unwrap();
    }
    let end = read_cycle_counter();

    let normal_result = BenchmarkResult {
        name: "Normal Dealloc",
        total_cycles: end - start,
        iterations,
        cycles_per_op: (end - start) / iterations,
    };

    // Benchmark safe deallocation
    let start = read_cycle_counter();
    for _ in 0..iterations {
        let alloc = allocator.allocate(64).unwrap();
        allocator.safe_deallocate(&alloc).unwrap();
    }
    let end = read_cycle_counter();

    let safe_result = BenchmarkResult {
        name: "Safe Dealloc",
        total_cycles: end - start,
        iterations,
        cycles_per_op: (end - start) / iterations,
    };

    (normal_result, safe_result)
}

// Benchmark result structure
#[derive(Debug, Clone, Copy, Default)]
pub struct BenchmarkResult {
    pub name: &'static str,
    pub total_cycles: u32,
    pub iterations: u32,
    pub cycles_per_op: u32,
}

impl BenchmarkResult {
    pub fn print(&self) {
        // Print results (implementation depends on platform)
    }

    pub fn cycles_to_ns(&self, cpu_freq_mhz: u32) -> u32 {
        // Convert cycles to nanoseconds
        (self.cycles_per_op * 1000) / cpu_freq_mhz
    }
}

// Helper function to read CPU cycle counter
#[inline(always)]
fn read_cycle_counter() -> u32 {
    #[cfg(target_arch = "arm")]
    {
        use cortex_m::peripheral::DWT;
        DWT::cycle_count()
    }

    #[cfg(not(target_arch = "arm"))]
    {
        0 // Fallback for non-ARM platforms
    }
}

/// Run all benchmarks
pub fn run_all_benchmarks() -> BenchmarkSuite {
    BenchmarkSuite {
        allocation_latency: bench_allocation_latency(),
        allocation_sizes: bench_allocation_sizes(),
        fragmentation: bench_fragmentation_impact(),
        concurrent_access: bench_concurrent_access(),
        pool_exhaustion: bench_pool_exhaustion(),
        integrity_check: bench_integrity_check(),
        safe_dealloc: bench_safe_vs_unsafe_dealloc(),
    }
}

pub struct BenchmarkSuite {
    pub allocation_latency: BenchmarkResult,
    pub allocation_sizes: [BenchmarkResult; 7],
    pub fragmentation: BenchmarkResult,
    pub concurrent_access: BenchmarkResult,
    pub pool_exhaustion: BenchmarkResult,
    pub integrity_check: BenchmarkResult,
    pub safe_dealloc: (BenchmarkResult, BenchmarkResult),
}

impl BenchmarkSuite {
    pub fn print_summary(&self) {
        // Print comprehensive benchmark summary
    }
}
