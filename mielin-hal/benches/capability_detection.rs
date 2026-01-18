//! Benchmarks for hardware capability detection
//!
//! These benchmarks measure the performance of various capability detection
//! functions to ensure they meet performance requirements.
//!
//! Run with: cargo bench

use criterion::{criterion_group, criterion_main, Criterion};
use mielin_hal::*;
use std::hint::black_box;

fn bench_architecture_detection(c: &mut Criterion) {
    c.bench_function("architecture_detection", |b| {
        b.iter(|| {
            let arch = detect_architecture();
            black_box(arch);
        });
    });
}

fn bench_cache_topology_detection(c: &mut Criterion) {
    c.bench_function("cache_topology_detection", |b| {
        b.iter(|| {
            let cache = cache::CacheTopology::detect();
            black_box(cache);
        });
    });
}

fn bench_memory_info_detection(c: &mut Criterion) {
    c.bench_function("memory_info_detection", |b| {
        b.iter(|| {
            let mem = system::MemoryInfo::detect();
            black_box(mem);
        });
    });
}

fn bench_cpu_topology_detection(c: &mut Criterion) {
    c.bench_function("cpu_topology_detection", |b| {
        b.iter(|| {
            let topo = system::CpuTopology::detect();
            black_box(topo);
        });
    });
}

fn bench_platform_detection(c: &mut Criterion) {
    c.bench_function("platform_detection", |b| {
        b.iter(|| {
            let platform = platform::detect_platform();
            black_box(platform);
        });
    });
}

fn bench_platform_capabilities(c: &mut Criterion) {
    c.bench_function("platform_capabilities", |b| {
        b.iter(|| {
            let caps = platform::detect_capabilities();
            black_box(caps);
        });
    });
}

fn bench_gpu_detection(c: &mut Criterion) {
    c.bench_function("gpu_detection", |b| {
        b.iter(|| {
            let gpus = gpu::detect_gpus();
            black_box(gpus);
        });
    });
}

fn bench_accelerator_detection(c: &mut Criterion) {
    c.bench_function("accelerator_detection", |b| {
        b.iter(|| {
            let accel = accelerator::detect_accelerators();
            black_box(accel);
        });
    });
}

fn bench_power_info_detection(c: &mut Criterion) {
    c.bench_function("power_info_detection", |b| {
        b.iter(|| {
            let power = power::detect_power_info();
            black_box(power);
        });
    });
}

// Cached detection benchmarks (should be much faster on subsequent calls)

fn bench_cached_cache_topology(c: &mut Criterion) {
    // Prime the cache
    let _ = cache::CacheTopology::detect();

    c.bench_function("cached_cache_topology", |b| {
        b.iter(|| {
            let cache = cache::CacheTopology::detect();
            black_box(cache);
        });
    });
}

// Stress test: repeated detections

fn bench_repeated_architecture_detection(c: &mut Criterion) {
    c.bench_function("repeated_architecture_detection_100x", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let arch = detect_architecture();
                black_box(arch);
            }
        });
    });
}

fn bench_repeated_platform_detection(c: &mut Criterion) {
    c.bench_function("repeated_platform_detection_100x", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let platform = platform::detect_platform();
                black_box(platform);
            }
        });
    });
}

// Runtime capability switching benchmarks

fn bench_feature_requirement_creation(c: &mut Criterion) {
    c.bench_function("feature_requirement_creation", |b| {
        b.iter(|| {
            let req = runtime::FeatureRequirement::all_of(&[
                capabilities::HardwareCapabilities::AVX2,
                capabilities::HardwareCapabilities::FMA,
            ])
            .with_min_vector_width(256)
            .with_min_cores(4);
            black_box(req);
        });
    });
}

fn bench_feature_requirement_satisfaction(c: &mut Criterion) {
    let req = runtime::FeatureRequirement::all_of(&[capabilities::HardwareCapabilities::AVX2])
        .with_min_vector_width(256);
    let profile = capabilities::HardwareProfile::detect();

    c.bench_function("feature_requirement_satisfaction", |b| {
        b.iter(|| {
            let satisfied = req.is_satisfied_by(&profile);
            black_box(satisfied);
        });
    });
}

fn bench_feature_requirement_scoring(c: &mut Criterion) {
    let req = runtime::FeatureRequirement::all_of(&[capabilities::HardwareCapabilities::AVX2])
        .with_preferred(capabilities::HardwareCapabilities::FMA);
    let profile = capabilities::HardwareProfile::detect();

    c.bench_function("feature_requirement_scoring", |b| {
        b.iter(|| {
            let score = req.satisfaction_score(&profile);
            black_box(score);
        });
    });
}

fn bench_fallback_chain_creation(c: &mut Criterion) {
    c.bench_function("fallback_chain_creation", |b| {
        b.iter(|| {
            let mut chain = runtime::FallbackChain::new("simd_ops");
            chain.add_requirement(runtime::FeatureRequirement::all_of(&[
                capabilities::HardwareCapabilities::AVX512,
            ]));
            chain.add_requirement(runtime::FeatureRequirement::all_of(&[
                capabilities::HardwareCapabilities::AVX2,
            ]));
            chain.add_requirement(runtime::FeatureRequirement::all_of(&[
                capabilities::HardwareCapabilities::SSE4_2,
            ]));
            chain.add_requirement(runtime::FeatureRequirement::none());
            black_box(chain);
        });
    });
}

fn bench_fallback_chain_selection(c: &mut Criterion) {
    let mut chain = runtime::FallbackChain::new("simd_ops");
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX512,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX2,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::SSE4_2,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::none());
    let profile = capabilities::HardwareProfile::detect();

    c.bench_function("fallback_chain_selection", |b| {
        b.iter(|| {
            let selected = chain.select_best(&profile);
            black_box(selected);
        });
    });
}

fn bench_fallback_chain_all_satisfied(c: &mut Criterion) {
    let mut chain = runtime::FallbackChain::new("simd_ops");
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX512,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX2,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::SSE4_2,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::none());
    let profile = capabilities::HardwareProfile::detect();

    c.bench_function("fallback_chain_all_satisfied", |b| {
        b.iter(|| {
            let satisfied = chain.all_satisfied(&profile);
            black_box(satisfied);
        });
    });
}

fn bench_runtime_selector_creation(c: &mut Criterion) {
    c.bench_function("runtime_selector_creation", |b| {
        b.iter(|| {
            let selector = runtime::RuntimeSelector::new();
            black_box(selector);
        });
    });
}

fn bench_runtime_selector_selection(c: &mut Criterion) {
    let mut chain = runtime::FallbackChain::new("test");
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX512,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX2,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::none());
    let selector = runtime::RuntimeSelector::new();

    c.bench_function("runtime_selector_selection", |b| {
        b.iter(|| {
            let result = selector.select(&chain);
            let _ = black_box(result);
        });
    });
}

fn bench_runtime_selector_refresh(c: &mut Criterion) {
    c.bench_function("runtime_selector_refresh", |b| {
        let mut selector = runtime::RuntimeSelector::new();
        b.iter(|| {
            selector.refresh();
            black_box(&selector);
        });
    });
}

fn bench_dispatch_table_selection(c: &mut Criterion) {
    c.bench_function("dispatch_table_selection", |b| {
        b.iter(|| {
            fn scalar_impl() -> i32 {
                42
            }
            fn vector_impl() -> i32 {
                84
            }

            let mut table: runtime::DispatchTable<i32> = runtime::DispatchTable::new("compute");
            table.add_impl(runtime::FeatureRequirement::none(), scalar_impl);
            table.add_impl(
                runtime::FeatureRequirement::all_of(&[capabilities::HardwareCapabilities::AVX2]),
                vector_impl,
            );
            let selector = runtime::RuntimeSelector::new();
            let result = table.select(&selector);
            let _ = black_box(result);
        });
    });
}

fn bench_dispatch_table_call(c: &mut Criterion) {
    fn scalar_impl() -> i32 {
        42
    }

    let mut table: runtime::DispatchTable<i32> = runtime::DispatchTable::new("compute");
    table.add_impl(runtime::FeatureRequirement::none(), scalar_impl);
    let selector = runtime::RuntimeSelector::new();
    let _ = table.select(&selector);

    c.bench_function("dispatch_table_call", |b| {
        b.iter(|| {
            let result = table.call();
            let _ = black_box(result);
        });
    });
}

fn bench_runtime_switching_full_pipeline(c: &mut Criterion) {
    c.bench_function("runtime_switching_full_pipeline", |b| {
        b.iter(|| {
            // Full pipeline: create chain, selector, select, and execute
            let mut chain = runtime::FallbackChain::new("pipeline");
            chain.add_requirement(runtime::FeatureRequirement::all_of(&[
                capabilities::HardwareCapabilities::AVX512,
            ]));
            chain.add_requirement(runtime::FeatureRequirement::all_of(&[
                capabilities::HardwareCapabilities::AVX2,
            ]));
            chain.add_requirement(runtime::FeatureRequirement::none());

            let selector = runtime::RuntimeSelector::new();
            let selected = selector.select(&chain);
            let _ = black_box(selected);
        });
    });
}

fn bench_runtime_switching_cached(c: &mut Criterion) {
    // Pre-create chain and selector (simulating cached scenario)
    let mut chain = runtime::FallbackChain::new("cached");
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX512,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::all_of(&[
        capabilities::HardwareCapabilities::AVX2,
    ]));
    chain.add_requirement(runtime::FeatureRequirement::none());
    let selector = runtime::RuntimeSelector::new();

    c.bench_function("runtime_switching_cached", |b| {
        b.iter(|| {
            let selected = selector.select(&chain);
            let _ = black_box(selected);
        });
    });
}

fn bench_runtime_switching_overhead_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_switching_overhead");

    // Direct function call (baseline)
    fn direct_impl() -> i32 {
        42
    }
    group.bench_function("direct_call", |b| {
        b.iter(|| {
            let result = direct_impl();
            black_box(result);
        });
    });

    // Runtime dispatch
    let mut table: runtime::DispatchTable<i32> = runtime::DispatchTable::new("overhead_test");
    table.add_impl(runtime::FeatureRequirement::none(), direct_impl);
    let selector = runtime::RuntimeSelector::new();
    let _ = table.select(&selector);

    group.bench_function("runtime_dispatch", |b| {
        b.iter(|| {
            let result = table.call();
            let _ = black_box(result);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_architecture_detection,
    bench_cache_topology_detection,
    bench_memory_info_detection,
    bench_cpu_topology_detection,
    bench_platform_detection,
    bench_platform_capabilities,
    bench_gpu_detection,
    bench_accelerator_detection,
    bench_power_info_detection,
    bench_cached_cache_topology,
    bench_repeated_architecture_detection,
    bench_repeated_platform_detection,
    // Runtime switching benchmarks
    bench_feature_requirement_creation,
    bench_feature_requirement_satisfaction,
    bench_feature_requirement_scoring,
    bench_fallback_chain_creation,
    bench_fallback_chain_selection,
    bench_fallback_chain_all_satisfied,
    bench_runtime_selector_creation,
    bench_runtime_selector_selection,
    bench_runtime_selector_refresh,
    bench_dispatch_table_selection,
    bench_dispatch_table_call,
    bench_runtime_switching_full_pipeline,
    bench_runtime_switching_cached,
    bench_runtime_switching_overhead_comparison,
);

criterion_main!(benches);
