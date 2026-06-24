//! Mesh Performance Benchmarks
//!
//! Comprehensive benchmarks for MielinMesh critical paths:
//! - Message throughput (messages/sec)
//! - Gossip protocol convergence time
//! - DHT operations
//! - Priority queue efficiency
//! - Node creation overhead

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_mesh_core::{
    Dht, GossipMessage, HealthStatus, MemberInfo, Node, NodeId, NodeRole, PeerInfo,
};
use mielin_mesh_wire::{BatchConfig, Message, Priority, PriorityQueue, QueueConfig};
use std::collections::HashMap;
use std::hint::black_box;
use uuid::Uuid;

// ============================================================================
// Message Throughput
// ============================================================================

fn bench_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");

    for msg_size in [64, 256, 1024, 4096].iter() {
        group.throughput(Throughput::Bytes(*msg_size as u64));

        let message = Message::Ping { timestamp: 12345 };

        group.bench_with_input(
            BenchmarkId::new("serialize_ping", msg_size),
            &message,
            |b, msg| {
                b.iter(|| {
                    let serialized =
                        oxicode::encode_to_vec(&oxicode::serde::Compat(black_box(msg)))
                            .expect("Failed to serialize");
                    black_box(serialized);
                });
            },
        );
    }

    group.finish();
}

fn bench_message_batching(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_batching");

    for batch_size in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("batch_creation", batch_size),
            batch_size,
            |b, &size| {
                b.iter(|| {
                    let config = BatchConfig::default();
                    let _ = black_box(config.max_messages);

                    // Simulate message creation for batch
                    let mut messages = Vec::with_capacity(size);
                    for i in 0..size {
                        let msg = Message::Ping {
                            timestamp: i as u64,
                        };
                        messages.push(msg);
                    }
                    black_box(messages);
                });
            },
        );
    }

    group.finish();
}

fn bench_priority_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_queue");

    for queue_size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*queue_size as u64));

        group.bench_with_input(
            BenchmarkId::new("enqueue_dequeue", queue_size),
            queue_size,
            |b, &size| {
                b.iter(|| {
                    let mut queue = PriorityQueue::with_config(QueueConfig::default());

                    // Enqueue messages
                    for i in 0..size {
                        let priority = match i % 4 {
                            0 => Priority::Critical,
                            1 => Priority::High,
                            2 => Priority::Normal,
                            _ => Priority::Low,
                        };
                        let msg = Message::Ping {
                            timestamp: i as u64,
                        };
                        let _ = queue.enqueue_with_priority(msg, priority);
                    }

                    // Dequeue messages
                    for _ in 0..size {
                        let _ = queue.dequeue();
                    }

                    black_box(queue);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Gossip Protocol Performance
// ============================================================================

fn bench_gossip_message_creation(c: &mut Criterion) {
    c.bench_function("gossip_message_creation", |b| {
        let node_id = Uuid::new_v4();
        b.iter(|| {
            let msg = GossipMessage::Heartbeat {
                node_id: black_box(node_id),
                incarnation: 1,
            };
            black_box(msg);
        });
    });
}

fn bench_member_info_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("member_info");

    for member_count in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*member_count as u64));

        group.bench_with_input(
            BenchmarkId::new("create_members", member_count),
            member_count,
            |b, &count| {
                b.iter(|| {
                    let mut members: Vec<MemberInfo> = Vec::with_capacity(count);

                    for _ in 0..count {
                        let id = Uuid::new_v4();
                        let member = MemberInfo::new(id);
                        members.push(black_box(member));
                    }

                    black_box(members);
                });
            },
        );
    }

    group.finish();
}

fn bench_member_status_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("member_status");

    for member_count in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*member_count as u64));

        group.bench_with_input(
            BenchmarkId::new("status_transitions", member_count),
            member_count,
            |b, &count| {
                b.iter(|| {
                    let mut members: HashMap<NodeId, MemberInfo> = HashMap::new();

                    // Create members
                    for _ in 0..count {
                        let id = Uuid::new_v4();
                        let mut member = MemberInfo::new(id);
                        member.status = HealthStatus::Alive;
                        members.insert(id, member);
                    }

                    // Transition states
                    for member in members.values_mut() {
                        member.status = HealthStatus::Suspect;
                        black_box(&member.status);
                        member.status = HealthStatus::Dead;
                        black_box(&member.status);
                    }

                    black_box(members);
                });
            },
        );
    }

    group.finish();
}

fn bench_gossip_convergence_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("gossip_convergence");
    group.sample_size(10); // Reduce samples for long-running test

    for network_size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("convergence_time", network_size),
            network_size,
            |b, &size| {
                b.iter(|| {
                    // Simulate gossip rounds until convergence
                    let mut members: Vec<HashMap<NodeId, MemberInfo>> =
                        (0..size).map(|_| HashMap::new()).collect();

                    // Initial state: each node knows only itself
                    for (i, member_map) in members.iter_mut().enumerate() {
                        let id = Uuid::new_v4();
                        let member = MemberInfo::new(id);
                        member_map.insert(id, member);
                        let _ = black_box(i);
                    }

                    // Simulate gossip rounds
                    for _round in 0..10 {
                        for i in 0..size {
                            // Each node gossips to 3 random peers
                            for _ in 0..3 {
                                let peer_idx = (i + 1) % size;
                                // Merge states (simplified)
                                let _ = black_box((&members[i], &members[peer_idx]));
                            }
                        }
                    }

                    black_box(members);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// DHT Operations Performance
// ============================================================================

fn bench_dht_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("dht_operations");

    // Peer insertion
    group.bench_function("dht_peer_insert", |b| {
        let local_id = Uuid::new_v4();
        let mut dht = Dht::new(local_id);

        b.iter(|| {
            let peer_id = Uuid::new_v4();
            let peer = PeerInfo::new(peer_id, "127.0.0.1:8080".to_string());
            dht.insert_peer(black_box(peer));
        });
    });

    // Peer lookup with different DHT sizes
    for dht_size in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*dht_size as u64));

        group.bench_with_input(
            BenchmarkId::new("dht_lookup", dht_size),
            dht_size,
            |b, &size| {
                let local_id = Uuid::new_v4();
                let mut dht = Dht::new(local_id);

                // Populate DHT
                let target_ids: Vec<NodeId> = (0..size)
                    .map(|_| {
                        let id = Uuid::new_v4();
                        let peer = PeerInfo::new(id, "127.0.0.1:8080".to_string());
                        dht.insert_peer(peer);
                        id
                    })
                    .collect();

                b.iter(|| {
                    let target = &target_ids[size / 2];
                    let result = dht.find_closest(black_box(target), 5);
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_dht_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("dht_scalability");
    group.sample_size(20);

    for dht_size in [1000, 5000, 10000].iter() {
        group.throughput(Throughput::Elements(*dht_size as u64));

        group.bench_with_input(
            BenchmarkId::new("bulk_operations", dht_size),
            dht_size,
            |b, &size| {
                b.iter(|| {
                    let local_id = Uuid::new_v4();
                    let mut dht = Dht::new(local_id);

                    // Bulk insert
                    for _ in 0..size {
                        let id = Uuid::new_v4();
                        let peer = PeerInfo::new(id, "127.0.0.1:8080".to_string());
                        dht.insert_peer(peer);
                    }

                    // Bulk lookup
                    for _ in 0..100 {
                        let target = Uuid::new_v4();
                        let result = dht.find_closest(&target, 5);
                        black_box(result);
                    }

                    black_box(dht);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Node Creation Performance
// ============================================================================

fn bench_node_creation_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_creation");

    for role in [NodeRole::Edge, NodeRole::Relay, NodeRole::Core].iter() {
        group.bench_with_input(
            BenchmarkId::new("create", format!("{:?}", role)),
            role,
            |b, role| {
                b.iter(|| {
                    let node = Node::new(black_box(*role));
                    black_box(node);
                });
            },
        );
    }

    group.finish();
}

fn bench_node_id_generation(c: &mut Criterion) {
    c.bench_function("node_id_generation", |b| {
        b.iter(|| {
            let id = Uuid::new_v4();
            black_box(id);
        });
    });
}

// ============================================================================
// Baseline Comparisons
// ============================================================================

fn bench_serialization_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_serialization");

    for size in [64, 256, 1024, 4096].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        let data = vec![42u8; *size];
        group.bench_with_input(BenchmarkId::new("oxicode", size), &data, |b, data| {
            b.iter(|| {
                // Use serde::Compat wrapper for Vec<u8>
                let encoded = oxicode::encode_to_vec(&oxicode::serde::Compat(black_box(data)))
                    .expect("Failed to encode");
                black_box(encoded);
            });
        });
    }

    group.finish();
}

fn bench_hashmap_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_hashmap");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("insert_lookup", size), size, |b, &size| {
            b.iter(|| {
                let mut map: HashMap<NodeId, u64> = HashMap::with_capacity(size);

                for i in 0..size {
                    let id = Uuid::new_v4();
                    map.insert(id, i as u64);
                }

                // Lookup all keys
                for key in map.keys() {
                    let _ = map.get(key);
                }

                black_box(map);
            });
        });
    }

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    message_benches,
    bench_message_serialization,
    bench_message_batching,
    bench_priority_queue,
);

criterion_group!(
    gossip_benches,
    bench_gossip_message_creation,
    bench_member_info_creation,
    bench_member_status_updates,
    bench_gossip_convergence_simulation,
);

criterion_group!(
    discovery_benches,
    bench_dht_operations,
    bench_dht_scalability,
);

criterion_group!(
    node_benches,
    bench_node_creation_overhead,
    bench_node_id_generation,
);

criterion_group!(
    baseline_benches,
    bench_serialization_baseline,
    bench_hashmap_baseline,
);

criterion_main!(
    message_benches,
    gossip_benches,
    discovery_benches,
    node_benches,
    baseline_benches,
);
