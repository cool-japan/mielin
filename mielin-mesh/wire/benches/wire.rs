//! Wire Protocol Performance Benchmarks
//!
//! Measures performance of compression, serialization, and protocol operations.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

use mielin_mesh_wire::compression::{CompressionAlgorithm, CompressionLevel, Compressor};
use mielin_mesh_wire::priority::{Priority, PriorityQueue, QueueConfig};
use mielin_mesh_wire::Message;

/// Generate test data with specified entropy level
fn generate_test_data(size: usize, compressible: bool) -> Vec<u8> {
    if compressible {
        // Low entropy data (compresses well)
        (0..size).map(|i| (i % 64) as u8).collect()
    } else {
        // High entropy data (doesn't compress well)
        (0..size).map(|i| ((i * 17 + 37) % 256) as u8).collect()
    }
}

/// Benchmark LZ4 compression at various sizes
fn bench_lz4_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4_compression");

    for size in [1024, 4096, 16384, 65536, 262144] {
        let data = generate_test_data(size, true);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("compress", size), &data, |b, data| {
            let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4)
                .level(CompressionLevel::Fast)
                .min_size(0); // Always compress for benchmark
            b.iter(|| {
                let result = compressor.compress(black_box(data));
                black_box(result)
            })
        });
    }

    group.finish();
}

/// Benchmark LZ4 decompression at various sizes
fn bench_lz4_decompression(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4_decompression");

    for size in [1024, 4096, 16384, 65536, 262144] {
        let data = generate_test_data(size, true);
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4)
            .level(CompressionLevel::Fast)
            .min_size(0);
        let compressed = compressor.compress(&data).unwrap();

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("decompress", size),
            &compressed,
            |b, compressed| {
                b.iter(|| {
                    let result = compressor.decompress(black_box(compressed));
                    black_box(result)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark Zstd compression at various sizes
fn bench_zstd_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("zstd_compression");

    for size in [1024, 4096, 16384, 65536] {
        let data = generate_test_data(size, true);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("compress", size), &data, |b, data| {
            let compressor = Compressor::with_algorithm(CompressionAlgorithm::Zstd)
                .level(CompressionLevel::Default)
                .min_size(0);
            b.iter(|| {
                let result = compressor.compress(black_box(data));
                black_box(result)
            })
        });
    }

    group.finish();
}

/// Benchmark priority queue operations
fn bench_priority_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_queue");

    // Benchmark enqueue with critical priority
    group.bench_function("enqueue_critical", |b| {
        let config = QueueConfig::default();
        let mut queue = PriorityQueue::with_config(config);
        let msg = Message::Ping { timestamp: 12345 };

        b.iter(|| {
            // Dequeue to make room
            let _ = queue.dequeue();
            let result = queue.enqueue_with_priority(black_box(msg.clone()), Priority::Critical);
            black_box(result)
        })
    });

    // Benchmark enqueue with normal priority
    group.bench_function("enqueue_normal", |b| {
        let config = QueueConfig::default();
        let mut queue = PriorityQueue::with_config(config);
        let msg = Message::Ping { timestamp: 12345 };

        b.iter(|| {
            let _ = queue.dequeue();
            let result = queue.enqueue_with_priority(black_box(msg.clone()), Priority::Normal);
            black_box(result)
        })
    });

    // Benchmark dequeue
    group.bench_function("dequeue", |b| {
        let config = QueueConfig::default();
        let mut queue = PriorityQueue::with_config(config);

        // Fill queue
        for _ in 0..100 {
            let msg = Message::Ping { timestamp: 12345 };
            let _ = queue.enqueue(msg);
        }

        b.iter(|| {
            // Refill if empty
            if queue.is_empty() {
                for _ in 0..100 {
                    let msg = Message::Ping { timestamp: 12345 };
                    let _ = queue.enqueue(msg);
                }
            }
            let result = queue.dequeue();
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark message serialization
fn bench_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");

    // Small message (Ping)
    group.bench_function("ping_serialize", |b| {
        let msg = Message::Ping { timestamp: 12345 };
        b.iter(|| {
            let result = black_box(&msg).serialize();
            black_box(result)
        })
    });

    // Medium message (AgentMigration with 1KB payload)
    group.bench_function("migration_1kb_serialize", |b| {
        let payload = vec![0u8; 1024];
        let msg = Message::AgentMigration {
            agent_id: [1u8; 16],
            snapshot: payload,
            priority: 5,
        };
        b.iter(|| {
            let result = black_box(&msg).serialize();
            black_box(result)
        })
    });

    // Large message (AgentMigration with 64KB payload)
    group.bench_function("migration_64kb_serialize", |b| {
        let payload = vec![0u8; 65536];
        let msg = Message::AgentMigration {
            agent_id: [1u8; 16],
            snapshot: payload,
            priority: 5,
        };
        b.iter(|| {
            let result = black_box(&msg).serialize();
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark message deserialization
fn bench_message_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_deserialization");

    // Small message
    group.bench_function("ping_deserialize", |b| {
        let msg = Message::Ping { timestamp: 12345 };
        let serialized = msg.serialize().unwrap();
        b.iter(|| {
            let result = Message::deserialize(black_box(&serialized));
            black_box(result)
        })
    });

    // Medium message
    group.bench_function("migration_1kb_deserialize", |b| {
        let msg = Message::AgentMigration {
            agent_id: [1u8; 16],
            snapshot: vec![0u8; 1024],
            priority: 5,
        };
        let serialized = msg.serialize().unwrap();
        b.iter(|| {
            let result = Message::deserialize(black_box(&serialized));
            black_box(result)
        })
    });

    // Large message
    group.bench_function("migration_64kb_deserialize", |b| {
        let msg = Message::AgentMigration {
            agent_id: [1u8; 16],
            snapshot: vec![0u8; 65536],
            priority: 5,
        };
        let serialized = msg.serialize().unwrap();
        b.iter(|| {
            let result = Message::deserialize(black_box(&serialized));
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark compression ratio comparison
fn bench_compression_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_comparison");

    let data = generate_test_data(65536, true); // 64KB compressible data
    group.throughput(Throughput::Bytes(65536));

    group.bench_function("lz4_roundtrip", |b| {
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Lz4)
            .level(CompressionLevel::Fast)
            .min_size(0);
        b.iter(|| {
            let compressed = compressor.compress(black_box(&data)).unwrap();
            let decompressed = compressor.decompress(&compressed).unwrap();
            black_box(decompressed)
        })
    });

    group.bench_function("zstd_roundtrip", |b| {
        let compressor = Compressor::with_algorithm(CompressionAlgorithm::Zstd)
            .level(CompressionLevel::Default)
            .min_size(0);
        b.iter(|| {
            let compressed = compressor.compress(black_box(&data)).unwrap();
            let decompressed = compressor.decompress(&compressed).unwrap();
            black_box(decompressed)
        })
    });

    group.finish();
}

/// Benchmark message routing overhead
fn bench_message_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_routing");

    group.bench_function("route_message", |b| {
        let msg = Message::Ping { timestamp: 12345 };
        let source = [1u8; 16];
        let dest = [2u8; 16];

        b.iter(|| {
            let routed = black_box(msg.clone()).route(source, dest);
            black_box(routed)
        })
    });

    group.bench_function("forward_message", |b| {
        let msg = Message::Ping { timestamp: 12345 };
        let mut routed = msg.route([1u8; 16], [2u8; 16]);

        b.iter(|| {
            // Reset TTL/hop_count periodically
            if let Message::RoutedMessage { ttl, hop_count, .. } = &mut routed {
                if *ttl == 0 {
                    *ttl = 16;
                    *hop_count = 0;
                }
            }
            let result = routed.forward();
            black_box(result)
        })
    });

    group.bench_function("unwrap_payload", |b| {
        let msg = Message::Ping { timestamp: 12345 };

        b.iter(|| {
            let routed = msg.clone().route([1u8; 16], [2u8; 16]);
            let unwrapped = routed.unwrap_payload();
            black_box(unwrapped)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_lz4_compression,
    bench_lz4_decompression,
    bench_zstd_compression,
    bench_priority_queue,
    bench_message_serialization,
    bench_message_deserialization,
    bench_compression_comparison,
    bench_message_routing,
);

criterion_main!(benches);
