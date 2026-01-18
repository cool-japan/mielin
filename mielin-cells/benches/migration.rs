use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_cells::{migration::*, Agent};
use std::hint::black_box;

/// Create a test agent with specified WASM binary size
fn create_test_agent(binary_size: usize) -> Agent {
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d]; // Valid WASM header
    let mut full_binary = wasm_binary.clone();
    full_binary.extend(vec![0u8; binary_size.saturating_sub(4)]);
    Agent::new(full_binary)
}

/// Create test state data
fn create_test_state(size: usize) -> Vec<u8> {
    vec![0x42u8; size]
}

/// Create test state with patterns for compression testing
fn create_patterned_state(size: usize) -> Vec<u8> {
    let mut state = Vec::with_capacity(size);
    for i in 0..(size / 4) {
        state.extend(&[
            (i % 256) as u8,
            ((i + 1) % 256) as u8,
            (i % 256) as u8,
            ((i + 1) % 256) as u8,
        ]);
    }
    state.resize(size, 0);
    state
}

fn bench_snapshot_capture(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_capture");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let agent = create_test_agent(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| MigrationSnapshot::capture(black_box(&agent), None));
        });
    }

    group.finish();
}

fn bench_snapshot_restore(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_restore");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let agent = create_test_agent(*size);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| black_box(&snapshot).restore());
        });
    }

    group.finish();
}

fn bench_snapshot_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_serialization");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let agent = create_test_agent(*size);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| black_box(&snapshot).serialize());
        });
    }

    group.finish();
}

fn bench_snapshot_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_deserialization");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let agent = create_test_agent(*size);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
        let serialized = snapshot.serialize().unwrap();

        group.throughput(Throughput::Bytes(serialized.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| MigrationSnapshot::deserialize(black_box(&serialized)));
        });
    }

    group.finish();
}

fn bench_compression_rle(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_rle");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_test_state(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| simple_compress(black_box(&data)));
        });
    }

    group.finish();
}

fn bench_decompression_rle(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression_rle");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_test_state(*size);
        let compressed = simple_compress(&data);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| simple_decompress(black_box(&compressed)));
        });
    }

    group.finish();
}

fn bench_compression_lz4(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_lz4");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_patterned_state(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| lz4_compress(black_box(&data)));
        });
    }

    group.finish();
}

fn bench_decompression_lz4(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression_lz4");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_patterned_state(*size);
        let compressed = lz4_compress(&data);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| lz4_decompress(black_box(&compressed)));
        });
    }

    group.finish();
}

fn bench_adaptive_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_adaptive");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_patterned_state(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| adaptive_compress(black_box(&data)));
        });
    }

    group.finish();
}

fn bench_compression_zstd_default(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_zstd_default");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_patterned_state(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| zstd_compress(black_box(&data), ZSTD_DEFAULT_LEVEL));
        });
    }

    group.finish();
}

fn bench_compression_zstd_high(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_zstd_high");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_patterned_state(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| zstd_compress(black_box(&data), ZSTD_HIGH_LEVEL));
        });
    }

    group.finish();
}

fn bench_decompression_zstd(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression_zstd");

    for size in [4096, 40_960, 409_600, 1_048_576].iter() {
        let data = create_patterned_state(*size);
        let compressed = zstd_compress(&data, ZSTD_DEFAULT_LEVEL).unwrap();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| zstd_decompress(black_box(&compressed)));
        });
    }

    group.finish();
}

fn bench_adaptive_compression_aggressive(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_adaptive_aggressive");

    for size in [4096, 40_960, 409_600].iter() {
        let data = create_patterned_state(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| adaptive_compress_aggressive(black_box(&data)));
        });
    }

    group.finish();
}

fn bench_compression_ratio_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_ratio_comparison");

    // Test with sparse data (all zeros)
    let sparse_data = vec![0u8; 102_400];
    group.bench_function("sparse/rle", |b| {
        b.iter(|| simple_compress(black_box(&sparse_data)));
    });
    group.bench_function("sparse/lz4", |b| {
        b.iter(|| lz4_compress(black_box(&sparse_data)));
    });
    group.bench_function("sparse/zstd", |b| {
        b.iter(|| zstd_compress(black_box(&sparse_data), ZSTD_DEFAULT_LEVEL));
    });

    // Test with patterned data
    let patterned_data = create_patterned_state(102_400);
    group.bench_function("patterned/rle", |b| {
        b.iter(|| simple_compress(black_box(&patterned_data)));
    });
    group.bench_function("patterned/lz4", |b| {
        b.iter(|| lz4_compress(black_box(&patterned_data)));
    });
    group.bench_function("patterned/zstd", |b| {
        b.iter(|| zstd_compress(black_box(&patterned_data), ZSTD_DEFAULT_LEVEL));
    });

    // Test with random-like data
    let random_data: Vec<u8> = (0..102_400).map(|i| ((i * 13 + 7) % 256) as u8).collect();
    group.bench_function("random/rle", |b| {
        b.iter(|| simple_compress(black_box(&random_data)));
    });
    group.bench_function("random/lz4", |b| {
        b.iter(|| lz4_compress(black_box(&random_data)));
    });
    group.bench_function("random/zstd", |b| {
        b.iter(|| zstd_compress(black_box(&random_data), ZSTD_DEFAULT_LEVEL));
    });

    group.finish();
}

fn bench_delta_snapshot_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_snapshot_creation");

    for size in [4096, 40_960, 409_600].iter() {
        let agent_id = [1u8; 16];
        let old_state = create_test_state(*size);
        let mut new_state = old_state.clone();
        // Modify 10% of pages
        let page_size = 4096;
        let num_pages = size / page_size;
        for i in 0..(num_pages / 10) {
            let offset = i * page_size * 10;
            if offset < new_state.len() {
                new_state[offset] = 0xFF;
            }
        }

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| {
                DeltaSnapshot::create(
                    black_box(agent_id),
                    black_box(&old_state),
                    black_box(&new_state),
                    0,
                    1,
                )
            });
        });
    }

    group.finish();
}

fn bench_delta_snapshot_apply(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_snapshot_apply");

    for size in [4096, 40_960, 409_600].iter() {
        let agent_id = [1u8; 16];
        let old_state = create_test_state(*size);
        let mut new_state = old_state.clone();
        // Modify 10% of pages
        let page_size = 4096;
        let num_pages = size / page_size;
        for i in 0..(num_pages / 10) {
            let offset = i * page_size * 10;
            if offset < new_state.len() {
                new_state[offset] = 0xFF;
            }
        }
        let delta = DeltaSnapshot::create(agent_id, &old_state, &new_state, 0, 1).unwrap();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| {
                let mut state = old_state.clone();
                black_box(&delta).apply(black_box(&mut state))
            });
        });
    }

    group.finish();
}

fn bench_dirty_page_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("dirty_page_tracking");

    for size in [4096, 40_960, 409_600].iter() {
        let agent_id = [1u8; 16];
        let state = create_test_state(*size);
        let mut tracker = DirtyPageTracker::new(agent_id, *size);
        tracker.initialize(&state);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| {
                // Mark 10% of pages dirty
                let page_size = 4096;
                let num_pages = size / page_size;
                for i in 0..(num_pages / 10) {
                    tracker.mark_dirty(black_box(i * 10));
                }
                tracker.clear_all();
            });
        });
    }

    group.finish();
}

fn bench_migration_manager_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("migration_manager");

    group.bench_function("initiate_migration", |b| {
        b.iter(|| {
            let mut manager = MigrationManager::new();
            let agent = create_test_agent(1024);
            manager.initiate_migration(black_box(&agent), None)
        });
    });

    group.bench_function("complete_migration", |b| {
        b.iter(|| {
            let mut manager = MigrationManager::new();
            let agent = create_test_agent(1024);
            let snapshot = manager.initiate_migration(&agent, None).unwrap();
            manager.complete_migration(black_box(&snapshot.agent_id));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_snapshot_capture,
    bench_snapshot_restore,
    bench_snapshot_serialization,
    bench_snapshot_deserialization,
    bench_compression_rle,
    bench_decompression_rle,
    bench_compression_lz4,
    bench_decompression_lz4,
    bench_compression_zstd_default,
    bench_compression_zstd_high,
    bench_decompression_zstd,
    bench_adaptive_compression,
    bench_adaptive_compression_aggressive,
    bench_compression_ratio_comparison,
    bench_delta_snapshot_creation,
    bench_delta_snapshot_apply,
    bench_dirty_page_tracking,
    bench_migration_manager_operations,
);

criterion_main!(benches);
