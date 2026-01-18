//! Comprehensive performance benchmarks for mielin-wasm
//!
//! Run with: cargo bench --bench wasm_benchmarks

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::cache::{CacheConfig, CacheKey, ModuleCache};
use mielin_wasm::executor::WasmExecutor;
use mielin_wasm::jit::{HotFunctionThreshold, JitConfig, JitOptimizer};
use mielin_wasm::memory::{MemoryLimits, MemoryManager, MemorySnapshot};
use mielin_wasm::resource::{CodeSizeOptimizer, LazyCompiler, PageDeduplicator};
use std::hint::black_box;
use std::time::Duration;

/// Simple WASM module for testing
fn simple_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
        )
        "#,
    )
    .unwrap()
}

/// Complex WASM module with loops and branches
fn complex_module() -> Vec<u8> {
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

/// Memory-intensive WASM module
#[allow(dead_code)]
fn memory_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 16)
            (func (export "fill_memory") (param i32)
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

/// Benchmark module compilation
fn bench_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation");

    let executor = WasmExecutor::new().unwrap();
    let simple = simple_module();
    let complex = complex_module();

    group.bench_function("simple_module", |b| {
        b.iter(|| {
            let _ = executor.compile_module(black_box(&simple));
        })
    });

    group.bench_function("complex_module", |b| {
        b.iter(|| {
            let _ = executor.compile_module(black_box(&complex));
        })
    });

    group.finish();
}

/// Benchmark function execution
fn bench_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution");

    let executor = WasmExecutor::new().unwrap();
    let simple = simple_module();
    let module = executor.compile_module(&simple).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    let add_func = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .unwrap();

    group.bench_function("simple_add", |b| {
        b.iter(|| {
            let _ = add_func.call(&mut store, black_box((5, 7)));
        })
    });

    group.finish();
}

/// Benchmark module caching
fn bench_caching(c: &mut Criterion) {
    let mut group = c.benchmark_group("caching");

    let _config = CacheConfig::default();
    let cache = ModuleCache::new();
    let wasm = simple_module();
    let key = CacheKey::from_bytecode(&wasm);

    group.bench_function("cache_insert", |b| {
        b.iter(|| {
            let _ = cache.insert(
                black_box(key.clone()),
                black_box(wasm.clone()),
                black_box(wasm.len()),
            );
        })
    });

    cache.insert(key.clone(), wasm.clone(), wasm.len());

    group.bench_function("cache_get_hit", |b| {
        b.iter(|| {
            let _ = cache.get(black_box(&key));
        })
    });

    let miss_key = CacheKey::from_bytecode(b"nonexistent");
    group.bench_function("cache_get_miss", |b| {
        b.iter(|| {
            let _ = cache.get(black_box(&miss_key));
        })
    });

    group.finish();
}

/// Benchmark memory management
fn bench_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");

    let _limits = MemoryLimits::standard();
    let _manager = MemoryManager::new(_limits);

    // Create test data for snapshot benchmarks
    let test_data = vec![0u8; 65536]; // 64KB of data
    let snapshot = MemorySnapshot::new(test_data.clone(), 1);

    group.bench_function("memory_snapshot_create", |b| {
        b.iter(|| {
            let _ = MemorySnapshot::new(black_box(test_data.clone()), black_box(1));
        })
    });

    group.bench_function("memory_snapshot_compress", |b| {
        b.iter(|| {
            let _ = snapshot.compress();
        })
    });

    let compressed = snapshot.compress();

    group.bench_function("memory_snapshot_decompress", |b| {
        b.iter(|| {
            let _ = MemorySnapshot::decompress(&compressed);
        })
    });

    group.finish();
}

/// Benchmark JIT optimization
fn bench_jit(c: &mut Criterion) {
    let mut group = c.benchmark_group("jit");

    let _threshold = HotFunctionThreshold::default();
    let optimizer = JitOptimizer::new(JitConfig::default());

    group.bench_function("record_execution", |b| {
        b.iter(|| {
            optimizer.record_execution(black_box(0), black_box(Duration::from_micros(10)));
        })
    });

    // Warm up the optimizer
    for _ in 0..1000 {
        optimizer.record_execution(0, Duration::from_micros(10));
    }

    group.bench_function("get_recompilation_candidates", |b| {
        b.iter(|| {
            let _ = optimizer.get_recompilation_candidates();
        })
    });

    let inline_cache = optimizer.pgo().inline_cache();

    group.bench_function("inline_cache_lookup", |b| {
        b.iter(|| {
            let _ = inline_cache
                .lock()
                .unwrap()
                .lookup(black_box(0), black_box(123));
        })
    });

    group.bench_function("inline_cache_insert", |b| {
        b.iter(|| {
            inline_cache
                .lock()
                .unwrap()
                .insert(black_box(0), black_box(123), black_box(456));
        })
    });

    group.finish();
}

/// Benchmark resource efficiency features
fn bench_resource(c: &mut Criterion) {
    let mut group = c.benchmark_group("resource");

    let mut dedup = PageDeduplicator::new();
    let data = vec![0u8; 4096 * 10]; // 10 pages

    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("page_deduplication", |b| {
        b.iter(|| {
            let _ = dedup.deduplicate(black_box(&data));
        })
    });

    let compiler = LazyCompiler::new();

    group.bench_function("lazy_compilation_first", |b| {
        b.iter(|| {
            let _ = compiler.request_compilation(black_box(999));
        })
    });

    group.bench_function("lazy_compilation_cached", |b| {
        b.iter(|| {
            let _ = compiler.request_compilation(black_box(0));
        })
    });

    let optimizer = CodeSizeOptimizer::new();

    group.bench_function("code_size_estimation", |b| {
        b.iter(|| {
            let _ = optimizer.estimate_reduction(black_box(10000));
        })
    });

    group.finish();
}

/// Benchmark module validation
fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");

    let executor = WasmExecutor::new().unwrap();
    let simple = simple_module();
    let complex = complex_module();

    group.bench_function("validate_simple", |b| {
        b.iter(|| {
            let _ = executor.validate(black_box(&simple));
        })
    });

    group.bench_function("validate_complex", |b| {
        b.iter(|| {
            let _ = executor.validate(black_box(&complex));
        })
    });

    group.finish();
}

/// Benchmark scalability with module sizes
fn bench_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");

    let executor = WasmExecutor::new().unwrap();

    for size in [1, 10, 100].iter() {
        let module_bytes = simple_module();

        group.throughput(Throughput::Bytes(module_bytes.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _ = executor.compile_module(black_box(&module_bytes));
            })
        });
    }

    group.finish();
}

/// Benchmark concurrent module execution
fn bench_concurrency(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrency");
    group.sample_size(10); // Reduce sample size for concurrent tests

    let executor = WasmExecutor::new().unwrap();
    let wasm = simple_module();
    let module = executor.compile_module(&wasm).unwrap();

    for thread_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &count| {
                b.iter(|| {
                    let handles: Vec<_> = (0..count)
                        .map(|_| {
                            let (instance, mut store) = executor
                                .instantiate(&module, HardwareCapabilities::NONE)
                                .unwrap();
                            std::thread::spawn(move || {
                                let func = instance
                                    .get_typed_func::<(i32, i32), i32>(&mut store, "add")
                                    .unwrap();
                                let _ = func.call(&mut store, (5, 7));
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark custom extensions
fn bench_extensions(c: &mut Criterion) {
    use mielin_wasm::extensions::{
        Extension, ExtensionBuilder, ExtensionFunction, ExtensionInfo, ExtensionRegistry,
        FunctionSignature,
    };

    let mut group = c.benchmark_group("extensions");

    // Benchmark extension creation
    group.bench_function("extension_creation", |b| {
        b.iter(|| {
            let _ = ExtensionBuilder::new("test")
                .version("1.0.0")
                .description("Test extension")
                .function(ExtensionFunction::new("func1", FunctionSignature::I32))
                .function(ExtensionFunction::new("func2", FunctionSignature::I32ToI32))
                .build();
        })
    });

    // Benchmark registry operations
    group.bench_function("registry_register", |b| {
        b.iter(|| {
            let mut registry = ExtensionRegistry::new();
            for i in 0..10 {
                let ext = Extension::new(ExtensionInfo::new(format!("ext{}", i)));
                let _ = registry.register(ext);
            }
        })
    });

    // Benchmark extension state operations
    group.bench_function("state_operations", |b| {
        let ext = Extension::new(ExtensionInfo::new("test"));
        b.iter(|| {
            for i in 0..100 {
                let _ = ext.set_state(format!("key{}", i), Box::new(i));
            }
        })
    });

    group.finish();
}

/// Benchmark WASI debugging
fn bench_wasi_debug(c: &mut Criterion) {
    use mielin_wasm::wasi_debug::{WasiDebugger, WasiSyscall, WasiTraceEntry};
    use std::time::Duration;

    let mut group = c.benchmark_group("wasi_debug");

    // Benchmark trace recording
    group.bench_function("trace_recording", |b| {
        let debugger = WasiDebugger::new();
        b.iter(|| {
            for _ in 0..100 {
                let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
                    .with_fd(0)
                    .with_bytes(100)
                    .with_duration(Duration::from_micros(100));
                let _ = debugger.trace(entry);
            }
        })
    });

    // Benchmark statistics retrieval
    group.bench_function("stats_retrieval", |b| {
        let debugger = WasiDebugger::new();
        // Add some traces first
        for _ in 0..1000 {
            let _ = debugger.trace(WasiTraceEntry::new(WasiSyscall::FdRead));
        }
        b.iter(|| {
            let _ = debugger.get_stats(WasiSyscall::FdRead);
        })
    });

    // Benchmark FD monitor access
    group.bench_function("fd_monitor_access", |b| {
        let debugger = WasiDebugger::new();
        // Add some traces
        for fd in 0..10 {
            let _ = debugger.trace(
                WasiTraceEntry::new(WasiSyscall::FdRead)
                    .with_fd(fd)
                    .with_bytes(100),
            );
        }
        b.iter(|| {
            for fd in 0..10 {
                let _ = debugger.get_fd_monitor(fd);
            }
        })
    });

    // Benchmark trace formatting
    group.bench_function("trace_formatting", |b| {
        let debugger = WasiDebugger::new();
        // Add traces
        for _ in 0..100 {
            let _ = debugger.trace(
                WasiTraceEntry::new(WasiSyscall::FdRead)
                    .with_fd(0)
                    .with_bytes(100),
            );
        }
        b.iter(|| {
            let _ = debugger.format_traces();
        })
    });

    // Benchmark concurrent tracing
    group.bench_function("concurrent_tracing", |b| {
        use std::sync::Arc;
        let debugger = Arc::new(WasiDebugger::new());
        b.iter(|| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let debugger_clone = Arc::clone(&debugger);
                    std::thread::spawn(move || {
                        for _ in 0..25 {
                            let _ = debugger_clone.trace(WasiTraceEntry::new(WasiSyscall::FdRead));
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_compilation,
    bench_execution,
    bench_caching,
    bench_memory,
    bench_jit,
    bench_resource,
    bench_validation,
    bench_scalability,
    bench_concurrency,
    bench_extensions,
    bench_wasi_debug
);

criterion_main!(benches);
