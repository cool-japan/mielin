//! Migration Performance Benchmarks
//!
//! Comprehensive benchmarks for migration-related critical paths:
//! - Compression performance (LZ4, Zstd)
//! - Migration state machine transitions
//! - Memory copy baseline

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_cells::Agent;
use mielin_mesh_wire::{CompressionAlgorithm, Compressor, MigrationPhase, MigrationState};
use std::hint::black_box;

// ============================================================================
// Helper Functions
// ============================================================================

fn create_agent_with_size(size_bytes: usize) -> Agent {
    // Create minimal WASM header
    let mut wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    // Pad to desired size
    wasm_binary.resize(size_bytes, 0x42);
    Agent::new(wasm_binary)
}

fn create_test_data(size_bytes: usize) -> Vec<u8> {
    // Create compressible test data (repeated patterns)
    let pattern = b"MielinMesh Agent Migration Test Pattern Data ";
    let mut data = Vec::with_capacity(size_bytes);
    while data.len() < size_bytes {
        let remaining = size_bytes - data.len();
        let chunk_size = remaining.min(pattern.len());
        data.extend_from_slice(&pattern[..chunk_size]);
    }
    data
}

// ============================================================================
// Compression Performance
// ============================================================================

fn bench_lz4_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4_compression");

    for size_kb in [10, 100, 1024].iter() {
        let size_bytes = size_kb * 1024;
        group.throughput(Throughput::Bytes(size_bytes as u64));

        let data = create_test_data(size_bytes);
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4).min_size(0);

        group.bench_with_input(
            BenchmarkId::new("compress", format!("{}KB", size_kb)),
            &data,
            |b, data| {
                b.iter(|| {
                    let compressed = compressor
                        .compress(black_box(data))
                        .expect("Failed to compress");
                    black_box(compressed);
                });
            },
        );
    }

    group.finish();
}

fn bench_zstd_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("zstd_compression");

    for size_kb in [10, 100, 1024].iter() {
        let size_bytes = size_kb * 1024;
        group.throughput(Throughput::Bytes(size_bytes as u64));

        let data = create_test_data(size_bytes);
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Zstd).min_size(0);

        group.bench_with_input(
            BenchmarkId::new("compress", format!("{}KB", size_kb)),
            &data,
            |b, data| {
                b.iter(|| {
                    let compressed = compressor
                        .compress(black_box(data))
                        .expect("Failed to compress");
                    black_box(compressed);
                });
            },
        );
    }

    group.finish();
}

fn bench_decompression(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompression");

    for size_kb in [10, 100, 1024].iter() {
        let size_bytes = size_kb * 1024;
        group.throughput(Throughput::Bytes(size_bytes as u64));

        let data = create_test_data(size_bytes);

        // Pre-compress with LZ4
        let lz4_compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4).min_size(0);
        let lz4_compressed = lz4_compressor.compress(&data).expect("Failed to compress");

        // Pre-compress with Zstd
        let zstd_compressor = Compressor::with_algorithm(CompressionAlgorithm::Zstd).min_size(0);
        let zstd_compressed = zstd_compressor.compress(&data).expect("Failed to compress");

        // LZ4 decompression
        group.bench_with_input(
            BenchmarkId::new("lz4_decompress", format!("{}KB", size_kb)),
            &lz4_compressed,
            |b, compressed| {
                b.iter(|| {
                    let decompressed = lz4_compressor
                        .decompress(black_box(compressed))
                        .expect("Failed to decompress");
                    black_box(decompressed);
                });
            },
        );

        // Zstd decompression
        group.bench_with_input(
            BenchmarkId::new("zstd_decompress", format!("{}KB", size_kb)),
            &zstd_compressed,
            |b, compressed| {
                b.iter(|| {
                    let decompressed = zstd_compressor
                        .decompress(black_box(compressed))
                        .expect("Failed to decompress");
                    black_box(decompressed);
                });
            },
        );
    }

    group.finish();
}

fn bench_compression_ratios(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_ratios");

    for size_kb in [10, 100, 1024].iter() {
        let size_bytes = size_kb * 1024;

        let data = create_test_data(size_bytes);

        group.bench_with_input(
            BenchmarkId::new("measure_ratio", format!("{}KB", size_kb)),
            &data,
            |b, data| {
                b.iter(|| {
                    let original_size = data.len();

                    let lz4_compressor =
                        Compressor::with_algorithm(CompressionAlgorithm::Lz4).min_size(0);
                    let lz4_compressed = lz4_compressor
                        .compress(black_box(data))
                        .expect("Failed to compress");
                    let lz4_ratio = lz4_compressed.compression_ratio();

                    let zstd_compressor =
                        Compressor::with_algorithm(CompressionAlgorithm::Zstd).min_size(0);
                    let zstd_compressed = zstd_compressor
                        .compress(black_box(data))
                        .expect("Failed to compress");
                    let zstd_ratio = zstd_compressed.compression_ratio();

                    black_box((original_size, lz4_ratio, zstd_ratio));
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Agent Operations
// ============================================================================

fn bench_agent_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_creation");

    for size_kb in [1, 10, 100, 1024].iter() {
        let size_bytes = size_kb * 1024;
        group.throughput(Throughput::Bytes(size_bytes as u64));

        group.bench_with_input(
            BenchmarkId::new("create", format!("{}KB", size_kb)),
            &size_bytes,
            |b, &size| {
                b.iter(|| {
                    let agent = create_agent_with_size(black_box(size));
                    black_box(agent);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Migration State Machine
// ============================================================================

fn bench_migration_state_transitions(c: &mut Criterion) {
    c.bench_function("migration_state_transitions", |b| {
        b.iter(|| {
            let state = MigrationState::Idle;
            black_box(&state);

            let state = MigrationState::Preparing;
            black_box(&state);

            let state = MigrationState::PreCopying;
            black_box(&state);

            let state = MigrationState::Finalizing;
            black_box(&state);

            let state = MigrationState::Committing;
            black_box(&state);

            let state = MigrationState::Completed;
            black_box(&state);

            black_box(state);
        });
    });
}

fn bench_migration_phase_transitions(c: &mut Criterion) {
    c.bench_function("migration_phase_transitions", |b| {
        b.iter(|| {
            let phases = [
                MigrationPhase::Prepare,
                MigrationPhase::PreCopy,
                MigrationPhase::StopAndCopy,
                MigrationPhase::Commit,
            ];

            for phase in phases.iter() {
                black_box(phase);
            }

            black_box(phases);
        });
    });
}

// ============================================================================
// Sequential Operations
// ============================================================================

fn bench_sequential_agent_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_agent_creation");
    group.sample_size(10);

    for agent_count in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*agent_count as u64));

        group.bench_with_input(
            BenchmarkId::new("create_agents", agent_count),
            agent_count,
            |b, &count| {
                b.iter(|| {
                    let agents: Vec<Agent> = (0..count)
                        .map(|_| create_agent_with_size(10 * 1024))
                        .collect();

                    black_box(agents);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Baseline Comparisons
// ============================================================================

fn bench_baseline_memory_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_memory_copy");

    for size_kb in [1, 10, 100, 1024].iter() {
        let size_bytes = size_kb * 1024;
        group.throughput(Throughput::Bytes(size_bytes as u64));

        let data = vec![0u8; size_bytes];

        group.bench_with_input(
            BenchmarkId::new("memcpy", format!("{}KB", size_kb)),
            &data,
            |b, data| {
                b.iter(|| {
                    let copied = data.clone();
                    black_box(copied);
                });
            },
        );
    }

    group.finish();
}

fn bench_baseline_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_serialization");

    for size in [1024, 10240, 102400].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        let data = vec![42u8; *size];
        group.bench_with_input(
            BenchmarkId::new("oxicode_encode", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let encoded = oxicode::encode_to_vec(&oxicode::serde::Compat(black_box(data)))
                        .expect("Failed to encode");
                    black_box(encoded);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    compression_benches,
    bench_lz4_compression,
    bench_zstd_compression,
    bench_decompression,
    bench_compression_ratios,
);

criterion_group!(agent_benches, bench_agent_creation,);

criterion_group!(
    state_machine_benches,
    bench_migration_state_transitions,
    bench_migration_phase_transitions,
);

criterion_group!(throughput_benches, bench_sequential_agent_creation,);

criterion_group!(
    baseline_benches,
    bench_baseline_memory_copy,
    bench_baseline_serialization,
);

criterion_main!(
    compression_benches,
    agent_benches,
    state_machine_benches,
    throughput_benches,
    baseline_benches,
);
