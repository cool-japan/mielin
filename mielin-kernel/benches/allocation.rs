//! Memory Allocation Benchmarks
//!
//! Measures performance of the kernel memory allocator under various conditions.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_kernel::memory::{AllocationStrategy, MemoryManager, MAX_PAGES};
use std::hint::black_box;

/// Benchmark single page allocation
fn bench_single_page_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_page");

    group.throughput(Throughput::Elements(1));

    group.bench_function("allocate", |b| {
        let mut mm = MemoryManager::default();
        b.iter(|| {
            let addr = mm.allocate_page().unwrap();
            mm.free_page(addr).unwrap();
            black_box(addr)
        })
    });

    group.bench_function("allocate_only", |b| {
        b.iter_custom(|iters| {
            let mut mm = MemoryManager::default();
            let start = std::time::Instant::now();
            for _ in 0..iters.min(MAX_PAGES as u64) {
                let _ = mm.allocate_page();
            }
            start.elapsed()
        })
    });

    group.finish();
}

/// Benchmark multi-page allocation with varying sizes
fn bench_multi_page_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_page");

    for size in [1, 4, 16, 64, 256].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("allocate", size), size, |b, &size| {
            let mut mm = MemoryManager::default();
            b.iter(|| {
                let addr = mm.allocate_pages(size).unwrap();
                mm.free_pages(addr, size).unwrap();
                black_box(addr)
            })
        });
    }

    group.finish();
}

/// Benchmark allocation strategies
fn bench_allocation_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("strategies");

    let strategies = [
        ("first_fit", AllocationStrategy::FirstFit),
        ("best_fit", AllocationStrategy::BestFit),
        ("worst_fit", AllocationStrategy::WorstFit),
    ];

    for (name, strategy) in strategies.iter() {
        group.bench_function(*name, |b| {
            let mut mm = MemoryManager::with_strategy(*strategy);
            b.iter(|| {
                // Allocate various sizes to test strategy effectiveness
                let a1 = mm.allocate_pages(10).unwrap();
                let a2 = mm.allocate_pages(5).unwrap();
                let a3 = mm.allocate_pages(20).unwrap();

                mm.free_pages(a2, 5).unwrap(); // Create a hole

                let a4 = mm.allocate_pages(5).unwrap(); // Fill the hole (strategy dependent)

                // Cleanup
                mm.free_pages(a1, 10).unwrap();
                mm.free_pages(a3, 20).unwrap();
                mm.free_pages(a4, 5).unwrap();

                black_box((a1, a2, a3, a4))
            })
        });
    }

    group.finish();
}

/// Benchmark fragmented allocation patterns
fn bench_fragmented_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("fragmented");

    group.bench_function("alternating_free", |b| {
        b.iter_custom(|iters| {
            let mut mm = MemoryManager::default();
            let mut addrs = Vec::new();

            // Create fragmentation by allocating many small blocks
            for _ in 0..100.min(MAX_PAGES / 2) {
                if let Ok(addr) = mm.allocate_pages(2) {
                    addrs.push(addr);
                }
            }

            // Free every other block to create holes
            for i in (0..addrs.len()).step_by(2) {
                mm.free_pages(addrs[i], 2).unwrap();
            }

            // Time allocations in fragmented state
            let start = std::time::Instant::now();
            for _ in 0..iters.min(50) {
                if let Ok(addr) = mm.allocate_pages(2) {
                    let _ = mm.free_pages(addr, 2);
                }
            }
            start.elapsed()
        })
    });

    group.finish();
}

/// Benchmark coalescing performance
fn bench_coalescing(c: &mut Criterion) {
    let mut group = c.benchmark_group("coalescing");

    group.bench_function("adjacent_free", |b| {
        let mut mm = MemoryManager::default();
        b.iter(|| {
            // Allocate 10 adjacent single pages
            let mut addrs = Vec::new();
            for _ in 0..10 {
                addrs.push(mm.allocate_page().unwrap());
            }

            // Free them all - should trigger coalescing
            for addr in addrs {
                mm.free_page(addr).unwrap();
            }

            black_box(())
        })
    });

    group.finish();
}

/// Benchmark scheduler operations
fn bench_scheduler(c: &mut Criterion) {
    use mielin_kernel::scheduler::Scheduler;

    let mut group = c.benchmark_group("scheduler");

    group.bench_function("spawn_terminate", |b| {
        let mut scheduler = Scheduler::new();
        b.iter(|| {
            let id = scheduler.spawn_task(5).unwrap();
            scheduler.terminate_task(id);
            black_box(id)
        })
    });

    group.bench_function("schedule_yield", |b| {
        let mut scheduler = Scheduler::new();
        scheduler.spawn_task(5).unwrap();
        b.iter(|| {
            scheduler.schedule();
            scheduler.yield_task();
        })
    });

    group.bench_function("priority_selection", |b| {
        let mut scheduler = Scheduler::new();
        // Spawn tasks with various priorities
        for p in 0..10 {
            scheduler.spawn_task(p * 10).unwrap();
        }
        b.iter(|| {
            scheduler.schedule();
            scheduler.yield_task();
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_page_alloc,
    bench_multi_page_alloc,
    bench_allocation_strategies,
    bench_fragmented_alloc,
    bench_coalescing,
    bench_scheduler,
);

criterion_main!(benches);
