//! Stress Tests for MielinWasm
//!
//! Tests concurrent module execution, resource limits, and system stability.
//!
//! Run with: cargo test --test stress_tests --release -- --test-threads=1

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::cache::{CacheKey, ModuleCache};
use mielin_wasm::executor::WasmExecutor;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Simple WASM module for stress testing
fn test_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
            (func (export "multiply") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.mul
            )
        )
        "#,
    )
    .unwrap()
}

/// Memory-intensive module
fn memory_intensive_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "fill") (param i32)
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

/// CPU-intensive module
fn cpu_intensive_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "fibonacci") (param i32) (result i32)
                (local i32 i32 i32)
                i32.const 0
                local.set 1
                i32.const 1
                local.set 2

                (block
                    (loop
                        local.get 0
                        i32.eqz
                        br_if 1

                        local.get 1
                        local.get 2
                        i32.add
                        local.set 3

                        local.get 2
                        local.set 1
                        local.get 3
                        local.set 2

                        local.get 0
                        i32.const 1
                        i32.sub
                        local.set 0

                        br 0
                    )
                )

                local.get 2
            )
        )
        "#,
    )
    .unwrap()
}

#[test]
fn test_concurrent_module_compilation() {
    const THREAD_COUNT: usize = 100;
    const MODULES_PER_THREAD: usize = 10;

    let start = Instant::now();
    let success_count = Arc::new(Mutex::new(0));
    let error_count = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|thread_id| {
            let success = Arc::clone(&success_count);
            let errors = Arc::clone(&error_count);

            thread::spawn(move || {
                let executor = WasmExecutor::new().unwrap();
                let wasm = test_module();

                for _ in 0..MODULES_PER_THREAD {
                    match executor.compile_module(&wasm) {
                        Ok(_) => {
                            let mut count = success.lock().unwrap();
                            *count += 1;
                        }
                        Err(e) => {
                            eprintln!("Thread {} compilation error: {}", thread_id, e);
                            let mut count = errors.lock().unwrap();
                            *count += 1;
                        }
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_modules = THREAD_COUNT * MODULES_PER_THREAD;
    let success = *success_count.lock().unwrap();
    let errors = *error_count.lock().unwrap();

    println!("\n=== Concurrent Compilation Results ===");
    println!("Total modules: {}", total_modules);
    println!("Successful: {}", success);
    println!("Errors: {}", errors);
    println!("Duration: {:?}", duration);
    println!(
        "Throughput: {:.2} compilations/sec",
        total_modules as f64 / duration.as_secs_f64()
    );

    assert_eq!(success, total_modules);
    assert_eq!(errors, 0);
}

#[test]
fn test_concurrent_module_execution() {
    const THREAD_COUNT: usize = 100;
    const EXECUTIONS_PER_THREAD: usize = 10;

    let executor = Arc::new(WasmExecutor::new().unwrap());
    let wasm = test_module();
    let module = Arc::new(executor.compile_module(&wasm).unwrap());

    let start = Instant::now();
    let success_count = Arc::new(Mutex::new(0));
    let error_count = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let executor = Arc::clone(&executor);
            let module = Arc::clone(&module);
            let success = Arc::clone(&success_count);
            let errors = Arc::clone(&error_count);

            thread::spawn(move || {
                for _ in 0..EXECUTIONS_PER_THREAD {
                    match executor.instantiate(&module, HardwareCapabilities::NONE) {
                        Ok((instance, mut store)) => {
                            let add = instance
                                .get_typed_func::<(i32, i32), i32>(&mut store, "add")
                                .unwrap();

                            match add.call(&mut store, (5, 7)) {
                                Ok(result) => {
                                    assert_eq!(result, 12);
                                    let mut count = success.lock().unwrap();
                                    *count += 1;
                                }
                                Err(_) => {
                                    let mut count = errors.lock().unwrap();
                                    *count += 1;
                                }
                            }
                        }
                        Err(_) => {
                            let mut count = errors.lock().unwrap();
                            *count += 1;
                        }
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let total_executions = THREAD_COUNT * EXECUTIONS_PER_THREAD;
    let success = *success_count.lock().unwrap();
    let errors = *error_count.lock().unwrap();

    println!("\n=== Concurrent Execution Results ===");
    println!("Total executions: {}", total_executions);
    println!("Successful: {}", success);
    println!("Errors: {}", errors);
    println!("Duration: {:?}", duration);
    println!(
        "Throughput: {:.2} executions/sec",
        total_executions as f64 / duration.as_secs_f64()
    );

    assert_eq!(success, total_executions);
    assert_eq!(errors, 0);
}

#[test]
fn test_1000_concurrent_modules() {
    const MODULE_COUNT: usize = 1000;

    let executor = Arc::new(WasmExecutor::new().unwrap());
    let wasm = test_module();
    let module = Arc::new(executor.compile_module(&wasm).unwrap());

    let start = Instant::now();
    let success_count = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..MODULE_COUNT)
        .map(|id| {
            let executor = Arc::clone(&executor);
            let module = Arc::clone(&module);
            let success = Arc::clone(&success_count);

            thread::spawn(move || {
                match executor.instantiate(&module, HardwareCapabilities::NONE) {
                    Ok((instance, mut store)) => {
                        // Execute function
                        let add = instance
                            .get_typed_func::<(i32, i32), i32>(&mut store, "add")
                            .unwrap();

                        let result = add.call(&mut store, (id as i32, 1)).unwrap();
                        assert_eq!(result, id as i32 + 1);

                        let mut count = success.lock().unwrap();
                        *count += 1;
                    }
                    Err(e) => {
                        eprintln!("Module {} instantiation failed: {}", id, e);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let success = *success_count.lock().unwrap();

    println!("\n=== 1000 Concurrent Modules Test ===");
    println!("Target modules: {}", MODULE_COUNT);
    println!("Successful: {}", success);
    println!("Duration: {:?}", duration);
    println!("Average per module: {:?}", duration / MODULE_COUNT as u32);

    assert_eq!(success, MODULE_COUNT);
}

#[test]
fn test_memory_stress() {
    const THREAD_COUNT: usize = 50;

    let executor = Arc::new(WasmExecutor::new().unwrap());
    let wasm = memory_intensive_module();
    let module = Arc::new(executor.compile_module(&wasm).unwrap());

    let start = Instant::now();
    let success_count = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let executor = Arc::clone(&executor);
            let module = Arc::clone(&module);
            let success = Arc::clone(&success_count);

            thread::spawn(move || {
                match executor.instantiate(&module, HardwareCapabilities::NONE) {
                    Ok((instance, mut store)) => {
                        let fill = instance
                            .get_typed_func::<i32, ()>(&mut store, "fill")
                            .unwrap();

                        // Fill first 1KB of memory
                        fill.call(&mut store, 256).unwrap();

                        let mut count = success.lock().unwrap();
                        *count += 1;
                    }
                    Err(e) => {
                        eprintln!("Memory stress test failed: {}", e);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let success = *success_count.lock().unwrap();

    println!("\n=== Memory Stress Test ===");
    println!("Concurrent instances: {}", THREAD_COUNT);
    println!("Successful: {}", success);
    println!("Duration: {:?}", duration);

    assert_eq!(success, THREAD_COUNT);
}

#[test]
fn test_cpu_stress() {
    const THREAD_COUNT: usize = 20;
    const FIBONACCI_N: i32 = 30;

    let executor = Arc::new(WasmExecutor::new().unwrap());
    let wasm = cpu_intensive_module();
    let module = Arc::new(executor.compile_module(&wasm).unwrap());

    let start = Instant::now();
    let success_count = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let executor = Arc::clone(&executor);
            let module = Arc::clone(&module);
            let success = Arc::clone(&success_count);

            thread::spawn(
                move || match executor.instantiate(&module, HardwareCapabilities::NONE) {
                    Ok((instance, mut store)) => {
                        let fib = instance
                            .get_typed_func::<i32, i32>(&mut store, "fibonacci")
                            .unwrap();

                        let result = fib.call(&mut store, FIBONACCI_N).unwrap();
                        assert!(result > 0);

                        let mut count = success.lock().unwrap();
                        *count += 1;
                    }
                    Err(e) => {
                        eprintln!("CPU stress test failed: {}", e);
                    }
                },
            )
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let success = *success_count.lock().unwrap();

    println!("\n=== CPU Stress Test ===");
    println!("Concurrent computations: {}", THREAD_COUNT);
    println!("Fibonacci({})", FIBONACCI_N);
    println!("Successful: {}", success);
    println!("Duration: {:?}", duration);
    println!("Avg per computation: {:?}", duration / THREAD_COUNT as u32);

    assert_eq!(success, THREAD_COUNT);
}

#[test]
fn test_cache_stress() {
    const THREAD_COUNT: usize = 100;
    const OPERATIONS_PER_THREAD: usize = 100;

    let cache = Arc::new(Mutex::new(ModuleCache::new()));
    let wasm = test_module();
    let key = CacheKey::from_bytecode(&wasm);

    // Pre-populate cache
    cache
        .lock()
        .unwrap()
        .insert(key.clone(), wasm.clone(), wasm.len());

    let start = Instant::now();
    let hit_count = Arc::new(Mutex::new(0));
    let miss_count = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let hits = Arc::clone(&hit_count);
            let misses = Arc::clone(&miss_count);
            let key = key.clone();

            thread::spawn(move || {
                for _ in 0..OPERATIONS_PER_THREAD {
                    let cache_locked = cache.lock().unwrap();
                    match cache_locked.get(&key) {
                        Some(_) => {
                            let mut count = hits.lock().unwrap();
                            *count += 1;
                        }
                        None => {
                            let mut count = misses.lock().unwrap();
                            *count += 1;
                        }
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let hits = *hit_count.lock().unwrap();
    let misses = *miss_count.lock().unwrap();
    let total = THREAD_COUNT * OPERATIONS_PER_THREAD;

    println!("\n=== Cache Stress Test ===");
    println!("Total operations: {}", total);
    println!("Cache hits: {}", hits);
    println!("Cache misses: {}", misses);
    println!("Hit rate: {:.2}%", (hits as f64 / total as f64) * 100.0);
    println!("Duration: {:?}", duration);
    println!(
        "Throughput: {:.2} ops/sec",
        total as f64 / duration.as_secs_f64()
    );

    // Should have mostly hits since we pre-populated
    assert!(hits > total / 2);
}

#[test]
fn test_rapid_instantiation_cleanup() {
    const ITERATIONS: usize = 1000;

    let executor = WasmExecutor::new().unwrap();
    let wasm = test_module();
    let module = executor.compile_module(&wasm).unwrap();

    let start = Instant::now();

    for i in 0..ITERATIONS {
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let add = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "add")
            .unwrap();

        let result = add.call(&mut store, (i as i32, 1)).unwrap();
        assert_eq!(result, i as i32 + 1);

        // Instance and store are dropped here
    }

    let duration = start.elapsed();

    println!("\n=== Rapid Instantiation/Cleanup Test ===");
    println!("Iterations: {}", ITERATIONS);
    println!("Duration: {:?}", duration);
    println!("Avg per iteration: {:?}", duration / ITERATIONS as u32);
    println!(
        "Throughput: {:.2} cycles/sec",
        ITERATIONS as f64 / duration.as_secs_f64()
    );
}

#[test]
fn test_long_running_concurrent() {
    const THREAD_COUNT: usize = 10;
    const DURATION_SECS: u64 = 5;

    let executor = Arc::new(WasmExecutor::new().unwrap());
    let wasm = cpu_intensive_module();
    let module = Arc::new(executor.compile_module(&wasm).unwrap());

    let start = Instant::now();
    let operation_counts = Arc::new(Mutex::new(vec![0usize; THREAD_COUNT]));

    let handles: Vec<_> = (0..THREAD_COUNT)
        .map(|thread_id| {
            let executor = Arc::clone(&executor);
            let module = Arc::clone(&module);
            let counts = Arc::clone(&operation_counts);

            thread::spawn(move || {
                let (instance, mut store) = executor
                    .instantiate(&module, HardwareCapabilities::NONE)
                    .unwrap();

                let fib = instance
                    .get_typed_func::<i32, i32>(&mut store, "fibonacci")
                    .unwrap();

                let mut ops = 0;
                let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);

                while Instant::now() < deadline {
                    let _ = fib.call(&mut store, 20);
                    ops += 1;
                }

                let mut counts_locked = counts.lock().unwrap();
                counts_locked[thread_id] = ops;
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    let counts = operation_counts.lock().unwrap();
    let total_ops: usize = counts.iter().sum();

    println!("\n=== Long-Running Concurrent Test ===");
    println!("Duration: {:?}", duration);
    println!("Threads: {}", THREAD_COUNT);
    println!("Total operations: {}", total_ops);
    println!("Ops per thread: {:?}", *counts);
    println!(
        "Throughput: {:.2} ops/sec",
        total_ops as f64 / duration.as_secs_f64()
    );
}
