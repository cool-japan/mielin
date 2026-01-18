//! Memory Leak Detection Tests for MielinWasm
//!
//! Tests to detect memory leaks in module lifecycle, host functions, and resource management.
//!
//! Run with: cargo test --test memory_leak_tests --release

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::cache::{CacheKey, ModuleCache};
use mielin_wasm::executor::WasmExecutor;
use mielin_wasm::memory::MemorySnapshot;
use mielin_wasm::resource::PageDeduplicator;
use std::sync::Arc;

/// Simple WASM module for testing
fn test_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "allocate") (param i32)
                (local i32)
                i32.const 0
                local.set 1

                (block
                    (loop
                        local.get 1
                        local.get 0
                        i32.ge_u
                        br_if 1

                        local.get 1
                        local.get 1
                        i32.store

                        local.get 1
                        i32.const 4
                        i32.add
                        local.set 1

                        br 0
                    )
                )
            )
        )
        "#,
    )
    .unwrap()
}

/// Get current memory usage in bytes
fn get_memory_usage() -> usize {
    // This is a simplified approach - in production you'd use more accurate methods
    // like jemalloc's stats or OS-specific APIs
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<usize>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }

    // Fallback: estimate from allocator (not accurate but better than nothing)
    std::alloc::System.used_memory_estimate()
}

/// Rough estimate of used memory (fallback)
trait MemoryEstimate {
    fn used_memory_estimate(&self) -> usize;
}

impl MemoryEstimate for std::alloc::System {
    fn used_memory_estimate(&self) -> usize {
        // This is a very rough estimate and not accurate
        // In production, use jemalloc or platform-specific APIs
        0
    }
}

#[test]
fn test_module_compilation_no_leak() {
    const ITERATIONS: usize = 100;

    let executor = WasmExecutor::new().unwrap();
    let wasm = test_module();

    // Warm up
    for _ in 0..10 {
        let _ = executor.compile_module(&wasm);
    }

    let initial_memory = get_memory_usage();

    // Compile modules repeatedly
    for _ in 0..ITERATIONS {
        let module = executor.compile_module(&wasm).unwrap();
        drop(module);
    }

    let final_memory = get_memory_usage();

    // Allow some growth for internal structures, but not unbounded growth
    let memory_growth = final_memory.saturating_sub(initial_memory);
    let growth_per_iteration = memory_growth / ITERATIONS;

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Growth per iteration: {} bytes", growth_per_iteration);

    // Should not grow significantly
    // Allow up to 16KB per iteration for JIT compiler overhead and internal caching
    // Wasmtime's Engine maintains internal code caches that can grow with compilations
    assert!(
        growth_per_iteration < 16384,
        "Potential memory leak: {} bytes per iteration",
        growth_per_iteration
    );
}

#[test]
fn test_instance_lifecycle_no_leak() {
    const ITERATIONS: usize = 100;

    let executor = WasmExecutor::new().unwrap();
    let wasm = test_module();
    let module = executor.compile_module(&wasm).unwrap();

    // Warm up
    for _ in 0..10 {
        let (_instance, _store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();
    }

    let initial_memory = get_memory_usage();

    // Create and destroy instances repeatedly
    for _ in 0..ITERATIONS {
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        // Use the instance
        let allocate = instance
            .get_typed_func::<i32, ()>(&mut store, "allocate")
            .unwrap();
        allocate.call(&mut store, 100).unwrap();

        // Instance and store are dropped here
    }

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    let growth_per_iteration = memory_growth / ITERATIONS;

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Growth per iteration: {} bytes", growth_per_iteration);

    // Should not grow significantly
    // Allow up to 32KB per iteration for JIT compiler caching, instance creation overhead,
    // and allocator fragmentation
    assert!(
        growth_per_iteration < 32768,
        "Potential memory leak: {} bytes per iteration",
        growth_per_iteration
    );
}

#[test]
fn test_memory_snapshot_no_leak() {
    const ITERATIONS: usize = 50;

    let executor = WasmExecutor::new().unwrap();
    let wasm = test_module();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Warm up
    for _ in 0..10 {
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let memory_data = memory.data(&store).to_vec();
        let memory_pages = memory.size(&store) as u32;
        let _ = MemorySnapshot::new(memory_data, memory_pages);
    }

    let initial_memory = get_memory_usage();

    // Create snapshots repeatedly
    for _ in 0..ITERATIONS {
        // Get memory from instance
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let memory_data = memory.data(&store).to_vec();
        let memory_pages = memory.size(&store) as u32;

        // Create snapshot
        let snapshot = MemorySnapshot::new(memory_data, memory_pages);

        // Compress and decompress
        let compressed = snapshot.compress();
        let _decompressed = MemorySnapshot::decompress(&compressed).unwrap();

        // All snapshots should be dropped here
    }

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    let growth_per_iteration = memory_growth / ITERATIONS;

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Growth per iteration: {} bytes", growth_per_iteration);

    // Snapshots are large, but should be freed
    // Allow up to 32KB per iteration for compression/decompression buffers,
    // zstd internal state, and allocator overhead
    assert!(
        growth_per_iteration < 32768,
        "Potential memory leak in snapshots: {} bytes per iteration",
        growth_per_iteration
    );
}

#[test]
fn test_cache_no_leak() {
    const ITERATIONS: usize = 100;

    let cache = ModuleCache::new();

    // Warm up
    for _i in 0..10 {
        let wasm = test_module();
        let key = CacheKey::from_bytecode(&wasm);
        cache.insert(key, wasm.clone(), wasm.len());
    }

    let initial_memory = get_memory_usage();

    // Insert and evict modules repeatedly
    for i in 0..ITERATIONS {
        let wasm = test_module();
        let key = CacheKey::from_bytecode(format!("module_{}", i).as_bytes());

        cache.insert(key, wasm.clone(), wasm.len());

        // Force eviction by checking size
        let stats = cache.stats();
        if stats.evictions > 0 {
            println!("Evictions: {}", stats.evictions);
        }
    }

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Cache stats: {:?}", cache.stats());

    // Cache should stabilize after evictions
    // Allow up to 20MB for allocator overhead, page table entries, and memory fragmentation
    // The cache itself holds entries but VmRSS can grow due to allocator behavior
    assert!(
        memory_growth < 20 * 1024 * 1024,
        "Potential memory leak in cache: {} bytes total growth",
        memory_growth
    );
}

#[test]
fn test_deduplicator_no_leak() {
    const ITERATIONS: usize = 100;

    let mut dedup = PageDeduplicator::new();

    // Create test data
    let data = vec![0u8; 4096 * 10]; // 10 pages

    // Warm up
    for _ in 0..10 {
        let _ = dedup.deduplicate(&data);
    }

    let initial_memory = get_memory_usage();

    // Deduplicate repeatedly
    for _ in 0..ITERATIONS {
        let pages = dedup.deduplicate(&data);

        // Pages should be Arc'd and shared
        assert!(!pages.is_empty());

        // Pages are dropped here
    }

    // Cleanup weak references
    dedup.cleanup();

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    let growth_per_iteration = memory_growth / ITERATIONS;

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Growth per iteration: {} bytes", growth_per_iteration);
    println!("Dedup stats: {:?}", dedup.stats());

    // Should not grow significantly due to Arc sharing
    // Allow up to 128KB per iteration for allocator overhead and internal hash table growth
    // The deduplicator maintains internal structures that can grow with unique pages
    assert!(
        growth_per_iteration < 131072,
        "Potential memory leak in deduplicator: {} bytes per iteration",
        growth_per_iteration
    );
}

#[test]
fn test_concurrent_instances_no_leak() {
    const THREAD_COUNT: usize = 10;
    const ITERATIONS_PER_THREAD: usize = 20;

    let executor = Arc::new(WasmExecutor::new().unwrap());
    let wasm = test_module();
    let module = Arc::new(executor.compile_module(&wasm).unwrap());

    // Warm up
    for _ in 0..10 {
        let (_instance, _store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();
    }

    let initial_memory = get_memory_usage();

    // Create instances concurrently
    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let executor = Arc::clone(&executor);
            let module = Arc::clone(&module);

            std::thread::spawn(move || {
                for _ in 0..ITERATIONS_PER_THREAD {
                    let (instance, mut store) = executor
                        .instantiate(&module, HardwareCapabilities::NONE)
                        .unwrap();

                    let allocate = instance
                        .get_typed_func::<i32, ()>(&mut store, "allocate")
                        .unwrap();
                    allocate.call(&mut store, 50).unwrap();

                    // Instance dropped
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    let total_iterations = THREAD_COUNT * ITERATIONS_PER_THREAD;
    let growth_per_iteration = memory_growth / total_iterations;

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Growth per iteration: {} bytes", growth_per_iteration);

    // Should not leak even with concurrent usage
    // Allow up to 32KB per iteration for JIT compiler caching, thread overhead,
    // and concurrent allocation fragmentation
    assert!(
        growth_per_iteration < 32768,
        "Potential memory leak in concurrent usage: {} bytes per iteration",
        growth_per_iteration
    );
}

#[test]
fn test_long_running_no_leak() {
    use std::time::{Duration, Instant};

    const DURATION_SECS: u64 = 10;

    let executor = WasmExecutor::new().unwrap();
    let wasm = test_module();
    let module = executor.compile_module(&wasm).unwrap();

    let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);
    let mut iteration = 0;

    // Initial memory baseline
    let initial_memory = get_memory_usage();
    let mut last_check = initial_memory;
    let mut max_growth = 0;

    while Instant::now() < deadline {
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let allocate = instance
            .get_typed_func::<i32, ()>(&mut store, "allocate")
            .unwrap();
        allocate.call(&mut store, 100).unwrap();

        iteration += 1;

        // Check memory every 100 iterations
        if iteration % 100 == 0 {
            let current_memory = get_memory_usage();
            let growth = current_memory.saturating_sub(last_check);
            max_growth = max_growth.max(growth);
            last_check = current_memory;
        }
    }

    let final_memory = get_memory_usage();
    let total_growth = final_memory.saturating_sub(initial_memory);

    println!("Iterations: {}", iteration);
    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Total growth: {} bytes", total_growth);
    println!("Max growth per 100 iterations: {} bytes", max_growth);

    // Memory should stabilize, not grow unbounded
    // Allow up to 4MB per 100 iterations for JIT compiler caching, allocator overhead,
    // and normal system memory fluctuations during long runs
    assert!(
        max_growth < 4 * 1024 * 1024,
        "Potential memory leak: {} bytes growth per 100 iterations",
        max_growth
    );
}

#[test]
fn test_store_data_no_leak() {
    const ITERATIONS: usize = 100;

    let executor = WasmExecutor::new().unwrap();
    let wasm = test_module();
    let module = executor.compile_module(&wasm).unwrap();

    let initial_memory = get_memory_usage();

    for _ in 0..ITERATIONS {
        let (_instance, store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        // Access host state
        {
            let state = store.data();
            let _ = state.tensor_runtime();
            let _ = state.system_state();
        }

        // Store dropped here
    }

    let final_memory = get_memory_usage();
    let memory_growth = final_memory.saturating_sub(initial_memory);
    let growth_per_iteration = memory_growth / ITERATIONS;

    println!("Initial memory: {} bytes", initial_memory);
    println!("Final memory: {} bytes", final_memory);
    println!("Growth: {} bytes", memory_growth);
    println!("Growth per iteration: {} bytes", growth_per_iteration);

    // Host state should be cleaned up with store
    // Allow up to 8KB per iteration for allocator overhead
    assert!(
        growth_per_iteration < 8192,
        "Potential memory leak in host state: {} bytes per iteration",
        growth_per_iteration
    );
}
