use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_cells::migration::MigrationSnapshot;
use mielin_cells::Agent;
use std::hint::black_box;

fn agent_creation_benchmark(c: &mut Criterion) {
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    c.bench_function("agent_creation", |b| {
        b.iter(|| {
            let agent = Agent::new(black_box(wasm_binary.clone()));
            black_box(agent);
        })
    });
}

fn agent_dna_sizes_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_dna_sizes");

    // Test with different WASM binary sizes
    for size in [64, 256, 1024, 4096].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        // Create a minimal valid WASM binary of the given size
        let mut wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        wasm_binary.resize(*size, 0x00);

        group.bench_with_input(BenchmarkId::new("create", size), &wasm_binary, |b, wasm| {
            b.iter(|| {
                let agent = Agent::new(black_box(wasm.clone()));
                black_box(agent);
            })
        });
    }

    group.finish();
}

fn migration_snapshot_capture_benchmark(c: &mut Criterion) {
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let agent = Agent::new(wasm_binary);

    c.bench_function("migration_snapshot_capture", |b| {
        b.iter(|| {
            let snapshot = MigrationSnapshot::capture(black_box(&agent), None)
                .expect("Failed to capture snapshot");
            black_box(snapshot);
        })
    });
}

fn migration_snapshot_serialize_benchmark(c: &mut Criterion) {
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let agent = Agent::new(wasm_binary);
    let snapshot = MigrationSnapshot::capture(&agent, None).expect("Failed to capture snapshot");

    c.bench_function("migration_snapshot_serialize", |b| {
        b.iter(|| {
            let serialized = snapshot.serialize().expect("Failed to serialize");
            black_box(serialized);
        })
    });
}

fn migration_snapshot_deserialize_benchmark(c: &mut Criterion) {
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let agent = Agent::new(wasm_binary);
    let snapshot = MigrationSnapshot::capture(&agent, None).expect("Failed to capture snapshot");
    let serialized = snapshot.serialize().expect("Failed to serialize");

    c.bench_function("migration_snapshot_deserialize", |b| {
        b.iter(|| {
            let deserialized = MigrationSnapshot::deserialize(black_box(&serialized))
                .expect("Failed to deserialize");
            black_box(deserialized);
        })
    });
}

fn migration_snapshot_restore_benchmark(c: &mut Criterion) {
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let agent = Agent::new(wasm_binary);
    let snapshot = MigrationSnapshot::capture(&agent, None).expect("Failed to capture snapshot");

    c.bench_function("migration_snapshot_restore", |b| {
        b.iter(|| {
            let restored = snapshot.restore().expect("Failed to restore");
            black_box(restored);
        })
    });
}

fn migration_roundtrip_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("migration_roundtrip");

    for size in [64, 256, 1024, 4096].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        let mut wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        wasm_binary.resize(*size, 0x00);
        let agent = Agent::new(wasm_binary);

        group.bench_with_input(BenchmarkId::new("full_cycle", size), &agent, |b, agent| {
            b.iter(|| {
                // Capture → Serialize → Deserialize → Restore
                let snapshot =
                    MigrationSnapshot::capture(black_box(agent), None).expect("Failed to capture");
                let serialized = snapshot.serialize().expect("Failed to serialize");
                let deserialized =
                    MigrationSnapshot::deserialize(&serialized).expect("Failed to deserialize");
                let restored = deserialized.restore().expect("Failed to restore");
                black_box(restored);
            })
        });
    }

    group.finish();
}

criterion_group!(
    agent_benches,
    agent_creation_benchmark,
    agent_dna_sizes_benchmark,
    migration_snapshot_capture_benchmark,
    migration_snapshot_serialize_benchmark,
    migration_snapshot_deserialize_benchmark,
    migration_snapshot_restore_benchmark,
    migration_roundtrip_benchmark
);
criterion_main!(agent_benches);
