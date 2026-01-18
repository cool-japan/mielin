//! Kernel Performance Benchmarks
//!
//! Benchmarks for kernel-level operations (std-compatible components):
//! - Memory allocation latency
//! - Task switching overhead (simulated)
//! - Lock-free data structure performance
//! - Per-CPU structure scalability
//! - Atomic operations performance
//!
//! Note: Full kernel benchmarking requires no_std environment.
//! These benchmarks test algorithms and patterns used in the kernel.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::VecDeque;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

// ============================================================================
// Memory Allocation Performance
// ============================================================================

fn bench_allocation_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    // Test different allocation sizes
    for size in [64, 256, 1024, 4096, 16384, 65536].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("alloc_dealloc", format!("{}B", size)),
            size,
            |b, &s| {
                b.iter(|| {
                    let allocation = vec![0u8; s];
                    black_box(allocation);
                    // Drops here
                });
            },
        );
    }

    group.finish();
}

fn bench_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_patterns");

    // Sequential allocation
    group.bench_function("sequential_alloc_1kb", |b| {
        b.iter(|| {
            let allocations: Vec<Vec<u8>> = (0..100).map(|_| vec![0u8; 1024]).collect();
            black_box(allocations);
        });
    });

    // Interleaved allocation/deallocation
    group.bench_function("interleaved_alloc_dealloc", |b| {
        b.iter(|| {
            let mut allocations = Vec::new();
            for i in 0..100 {
                allocations.push(vec![0u8; 1024]);
                if i % 10 == 0 && !allocations.is_empty() {
                    allocations.remove(0);
                }
            }
            black_box(allocations);
        });
    });

    // Pool-based allocation simulation
    group.bench_function("pool_based_alloc", |b| {
        let mut pool: Vec<Vec<u8>> = (0..100).map(|_| vec![0u8; 1024]).collect();

        b.iter(|| {
            // Simulate taking from pool
            let mut borrowed = Vec::new();
            for _ in 0..10 {
                if let Some(buf) = pool.pop() {
                    borrowed.push(buf);
                }
            }
            // Return to pool
            pool.extend(borrowed);
            black_box(&pool);
        });
    });

    group.finish();
}

fn bench_allocation_fragmentation(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_fragmentation");
    group.sample_size(20);

    group.bench_function("fragmented_pattern", |b| {
        b.iter(|| {
            let mut allocations = Vec::new();

            // Create fragmentation with varying sizes
            for i in 0..100 {
                let size = if i % 2 == 0 { 64 } else { 4096 };
                allocations.push(vec![0u8; size]);
            }

            // Free every other allocation
            allocations = allocations
                .into_iter()
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, v)| v)
                .collect();

            // Allocate in the gaps
            for _ in 0..50 {
                allocations.push(vec![0u8; 1024]);
            }

            black_box(allocations);
        });
    });

    group.finish();
}

// ============================================================================
// Task Switching Overhead (Simulated)
// ============================================================================

fn bench_context_switch_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_switch_simulation");

    // Simulate task state save/restore
    #[derive(Clone)]
    struct TaskContext {
        registers: [u64; 32],
        stack_pointer: u64,
        program_counter: u64,
    }

    impl TaskContext {
        fn new() -> Self {
            Self {
                registers: [0; 32],
                stack_pointer: 0x1000,
                program_counter: 0x2000,
            }
        }

        fn save(&mut self) {
            // Simulate register save
            for i in 0..32 {
                self.registers[i] = i as u64;
            }
        }

        fn restore(&self) {
            // Simulate register restore
            let _ = black_box(&self.registers);
            let _ = black_box(self.stack_pointer);
            let _ = black_box(self.program_counter);
        }
    }

    group.bench_function("task_context_switch", |b| {
        let mut ctx1 = TaskContext::new();
        let ctx2 = TaskContext::new();

        b.iter(|| {
            // Save current context
            ctx1.save();
            black_box(&ctx1);

            // Restore other context
            ctx2.restore();
        });
    });

    // Benchmark task queue operations
    group.bench_function("task_queue_operations", |b| {
        let mut ready_queue = VecDeque::new();
        for i in 0..10 {
            ready_queue.push_back(i);
        }

        b.iter(|| {
            // Dequeue current task
            if let Some(task) = ready_queue.pop_front() {
                // Enqueue at back (round-robin)
                ready_queue.push_back(black_box(task));
            }
        });
    });

    group.finish();
}

fn bench_scheduler_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduler_operations");

    for task_count in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*task_count as u64));

        group.bench_with_input(
            BenchmarkId::new("round_robin", task_count),
            task_count,
            |b, &count| {
                b.iter(|| {
                    let mut queue = VecDeque::new();
                    for i in 0..count {
                        queue.push_back(i);
                    }

                    // Simulate scheduling rounds
                    for _ in 0..100 {
                        if let Some(task) = queue.pop_front() {
                            queue.push_back(task);
                        }
                    }

                    black_box(queue);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Lock-Free Data Structure Performance
// ============================================================================

fn bench_atomic_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic_operations");

    let counter = AtomicU64::new(0);

    // Relaxed ordering
    group.bench_function("atomic_load_relaxed", |b| {
        b.iter(|| {
            let value = counter.load(Ordering::Relaxed);
            black_box(value);
        });
    });

    group.bench_function("atomic_store_relaxed", |b| {
        b.iter(|| {
            counter.store(black_box(42), Ordering::Relaxed);
        });
    });

    // Acquire/Release ordering
    group.bench_function("atomic_load_acquire", |b| {
        b.iter(|| {
            let value = counter.load(Ordering::Acquire);
            black_box(value);
        });
    });

    group.bench_function("atomic_store_release", |b| {
        b.iter(|| {
            counter.store(black_box(42), Ordering::Release);
        });
    });

    // SeqCst ordering
    group.bench_function("atomic_load_seqcst", |b| {
        b.iter(|| {
            let value = counter.load(Ordering::SeqCst);
            black_box(value);
        });
    });

    group.bench_function("atomic_store_seqcst", |b| {
        b.iter(|| {
            counter.store(black_box(42), Ordering::SeqCst);
        });
    });

    // Compare-and-swap
    group.bench_function("atomic_cas", |b| {
        b.iter(|| {
            let result = counter.compare_exchange(
                black_box(0),
                black_box(1),
                Ordering::SeqCst,
                Ordering::Relaxed,
            );
            let _ = black_box(result);
        });
    });

    // Fetch-and-add
    group.bench_function("atomic_fetch_add", |b| {
        b.iter(|| {
            let old = counter.fetch_add(black_box(1), Ordering::Relaxed);
            black_box(old);
        });
    });

    group.finish();
}

fn bench_lock_free_queue_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("lock_free_queue");

    // Simple atomic-based queue simulation
    struct SimpleAtomicQueue {
        head: AtomicUsize,
        tail: AtomicUsize,
        buffer: Vec<AtomicU64>,
        capacity: usize,
    }

    impl SimpleAtomicQueue {
        fn new(capacity: usize) -> Self {
            let mut buffer = Vec::with_capacity(capacity);
            for _ in 0..capacity {
                buffer.push(AtomicU64::new(0));
            }
            Self {
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                buffer,
                capacity,
            }
        }

        fn try_enqueue(&self, value: u64) -> bool {
            let tail = self.tail.load(Ordering::Acquire);
            let next_tail = (tail + 1) % self.capacity;
            let head = self.head.load(Ordering::Acquire);

            if next_tail != head {
                self.buffer[tail].store(value, Ordering::Release);
                self.tail.store(next_tail, Ordering::Release);
                true
            } else {
                false
            }
        }

        fn try_dequeue(&self) -> Option<u64> {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);

            if head != tail {
                let value = self.buffer[head].load(Ordering::Acquire);
                let next_head = (head + 1) % self.capacity;
                self.head.store(next_head, Ordering::Release);
                Some(value)
            } else {
                None
            }
        }
    }

    for queue_size in [64, 256, 1024].iter() {
        group.bench_with_input(
            BenchmarkId::new("enqueue_dequeue", queue_size),
            queue_size,
            |b, &size| {
                let queue = SimpleAtomicQueue::new(size);

                b.iter(|| {
                    // Enqueue
                    for i in 0..10 {
                        let _ = queue.try_enqueue(black_box(i));
                    }

                    // Dequeue
                    for _ in 0..10 {
                        let _ = queue.try_dequeue();
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_lock_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("lock_contention");

    let mutex_counter = Arc::new(Mutex::new(0u64));
    let rwlock_counter = Arc::new(RwLock::new(0u64));

    // Mutex contention
    group.bench_function("mutex_lock_unlock", |b| {
        b.iter(|| {
            let mut guard = mutex_counter.lock().expect("Lock failed");
            *guard += 1;
            black_box(&*guard);
        });
    });

    // RwLock read
    group.bench_function("rwlock_read", |b| {
        b.iter(|| {
            let guard = rwlock_counter.read().expect("Lock failed");
            black_box(&*guard);
        });
    });

    // RwLock write
    group.bench_function("rwlock_write", |b| {
        b.iter(|| {
            let mut guard = rwlock_counter.write().expect("Lock failed");
            *guard += 1;
            black_box(&*guard);
        });
    });

    group.finish();
}

// ============================================================================
// Per-CPU Structure Scalability
// ============================================================================

fn bench_percpu_access_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("percpu_access");

    // Simulate per-CPU counters
    struct PerCpuCounters {
        counters: Vec<AtomicU64>,
    }

    impl PerCpuCounters {
        fn new(num_cpus: usize) -> Self {
            let mut counters = Vec::with_capacity(num_cpus);
            for _ in 0..num_cpus {
                counters.push(AtomicU64::new(0));
            }
            Self { counters }
        }

        fn increment(&self, cpu_id: usize) {
            if cpu_id < self.counters.len() {
                self.counters[cpu_id].fetch_add(1, Ordering::Relaxed);
            }
        }

        fn read(&self, cpu_id: usize) -> u64 {
            if cpu_id < self.counters.len() {
                self.counters[cpu_id].load(Ordering::Relaxed)
            } else {
                0
            }
        }

        fn total(&self) -> u64 {
            self.counters
                .iter()
                .map(|c| c.load(Ordering::Relaxed))
                .sum()
        }
    }

    for num_cpus in [4, 8, 16, 32].iter() {
        group.bench_with_input(
            BenchmarkId::new("percpu_increment", format!("{}cpus", num_cpus)),
            num_cpus,
            |b, &cpus| {
                let counters = PerCpuCounters::new(cpus);

                b.iter(|| {
                    for cpu in 0..cpus {
                        counters.increment(black_box(cpu));
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("percpu_read", format!("{}cpus", num_cpus)),
            num_cpus,
            |b, &cpus| {
                let counters = PerCpuCounters::new(cpus);

                b.iter(|| {
                    for cpu in 0..cpus {
                        let value = counters.read(black_box(cpu));
                        black_box(value);
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("percpu_aggregate", format!("{}cpus", num_cpus)),
            num_cpus,
            |b, &cpus| {
                let counters = PerCpuCounters::new(cpus);

                b.iter(|| {
                    let total = counters.total();
                    black_box(total);
                });
            },
        );
    }

    group.finish();
}

fn bench_cache_line_effects(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_line_effects");

    // Unpadded structure (false sharing)
    struct UnpaddedCounters {
        counters: Vec<AtomicU64>,
    }

    // Padded structure (no false sharing)
    #[repr(align(64))]
    struct PaddedCounter {
        value: AtomicU64,
        _padding: [u8; 56], // 64 - 8 bytes
    }

    struct PaddedCounters {
        counters: Vec<PaddedCounter>,
    }

    let num_counters = 8;

    // Unpadded (potential false sharing)
    group.bench_function("unpadded_counters", |b| {
        let counters = UnpaddedCounters {
            counters: (0..num_counters).map(|_| AtomicU64::new(0)).collect(),
        };

        b.iter(|| {
            for i in 0..num_counters {
                counters.counters[i].fetch_add(1, Ordering::Relaxed);
            }
        });
    });

    // Padded (no false sharing)
    group.bench_function("padded_counters", |b| {
        let counters = PaddedCounters {
            counters: (0..num_counters)
                .map(|_| PaddedCounter {
                    value: AtomicU64::new(0),
                    _padding: [0; 56],
                })
                .collect(),
        };

        b.iter(|| {
            for i in 0..num_counters {
                counters.counters[i].value.fetch_add(1, Ordering::Relaxed);
            }
        });
    });

    group.finish();
}

// ============================================================================
// Interrupt Handling Simulation
// ============================================================================

fn bench_interrupt_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("interrupt_simulation");

    // Simulate interrupt handler overhead
    fn simulate_interrupt_handler() {
        // Simulate saving context
        let context = [0u64; 32];
        black_box(&context);

        // Simulate interrupt processing
        let interrupt_id = 42;
        black_box(interrupt_id);

        // Simulate restoring context
        black_box(&context);
    }

    group.bench_function("interrupt_handler_overhead", |b| {
        b.iter(|| {
            simulate_interrupt_handler();
        });
    });

    // Nested interrupt simulation
    group.bench_function("nested_interrupt", |b| {
        b.iter(|| {
            simulate_interrupt_handler();
            simulate_interrupt_handler(); // Nested
        });
    });

    group.finish();
}

// ============================================================================
// Baseline Comparisons
// ============================================================================

fn bench_baseline_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_kernel_ops");

    // Function call overhead
    #[inline(never)]
    fn noinline_function(x: u64) -> u64 {
        x + 1
    }

    group.bench_function("function_call_overhead", |b| {
        b.iter(|| {
            let result = noinline_function(black_box(42));
            black_box(result);
        });
    });

    // Pointer dereferencing
    group.bench_function("pointer_deref", |b| {
        let value = 42u64;
        let ptr = &value as *const u64;

        b.iter(|| unsafe {
            let deref = *black_box(ptr);
            black_box(deref);
        });
    });

    // Branch prediction
    group.bench_function("predictable_branch", |b| {
        b.iter(|| {
            let x = black_box(42);
            if x > 10 {
                black_box(x + 1);
            } else {
                black_box(x - 1);
            }
        });
    });

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    allocation_benches,
    bench_allocation_sizes,
    bench_allocation_patterns,
    bench_allocation_fragmentation,
);

criterion_group!(
    scheduling_benches,
    bench_context_switch_simulation,
    bench_scheduler_operations,
);

criterion_group!(
    lockfree_benches,
    bench_atomic_operations,
    bench_lock_free_queue_simulation,
    bench_lock_contention,
);

criterion_group!(
    percpu_benches,
    bench_percpu_access_patterns,
    bench_cache_line_effects,
);

criterion_group!(interrupt_benches, bench_interrupt_overhead,);

criterion_group!(baseline_benches, bench_baseline_operations,);

criterion_main!(
    allocation_benches,
    scheduling_benches,
    lockfree_benches,
    percpu_benches,
    interrupt_benches,
    baseline_benches,
);
