use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_mesh_core::dht::{Dht, PeerInfo};
use mielin_mesh_core::{Node, NodeRole};
use std::hint::black_box;
use uuid::Uuid;

fn node_creation_benchmark(c: &mut Criterion) {
    c.bench_function("node_creation", |b| {
        b.iter(|| {
            let node = Node::new(black_box(NodeRole::Edge));
            black_box(node);
        })
    });
}

fn node_role_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_roles");

    group.bench_function("edge", |b| b.iter(|| black_box(Node::new(NodeRole::Edge))));

    group.bench_function("relay", |b| {
        b.iter(|| black_box(Node::new(NodeRole::Relay)))
    });

    group.bench_function("core", |b| b.iter(|| black_box(Node::new(NodeRole::Core))));

    group.finish();
}

fn dht_operations_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("dht_operations");

    // Benchmark DHT creation
    group.bench_function("dht_new", |b| {
        b.iter(|| {
            let local_id = Uuid::new_v4();
            black_box(Dht::new(local_id))
        })
    });

    // Benchmark peer insertion
    let local_id = Uuid::new_v4();
    let mut dht = Dht::new(local_id);

    group.bench_function("peer_insert", |b| {
        b.iter(|| {
            let peer_id = Uuid::new_v4();
            let peer = PeerInfo::new(peer_id, "127.0.0.1:8080".to_string());
            dht.insert_peer(black_box(peer));
        })
    });

    // Benchmark peer lookup in populated DHT
    let local_id = Uuid::new_v4();
    let mut dht = Dht::new(local_id);
    let target_ids: Vec<Uuid> = (0..100)
        .map(|_| {
            let id = Uuid::new_v4();
            let peer = PeerInfo::new(id, "127.0.0.1:8080".to_string());
            dht.insert_peer(peer);
            id
        })
        .collect();

    group.bench_function("peer_lookup_100", |b| {
        let mut i = 0;
        b.iter(|| {
            let target = &target_ids[i % target_ids.len()];
            let result = dht.find_closest(black_box(target), 5);
            black_box(result);
            i += 1;
        })
    });

    group.finish();
}

fn dht_scaling_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("dht_scaling");
    group.sample_size(50);

    for size in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::new("find_closest", size), size, |b, &size| {
            let local_id = Uuid::new_v4();
            let mut dht = Dht::new(local_id);

            // Populate DHT
            for _ in 0..size {
                let id = Uuid::new_v4();
                let peer = PeerInfo::new(id, "127.0.0.1:8080".to_string());
                dht.insert_peer(peer);
            }

            let target = Uuid::new_v4();
            b.iter(|| {
                let result = dht.find_closest(black_box(&target), 5);
                black_box(result);
            })
        });
    }

    group.finish();
}

fn uuid_generation_benchmark(c: &mut Criterion) {
    c.bench_function("uuid_v4_generation", |b| {
        b.iter(|| {
            let id = Uuid::new_v4();
            black_box(id);
        })
    });
}

criterion_group!(
    mesh_benches,
    node_creation_benchmark,
    node_role_benchmark,
    dht_operations_benchmark,
    dht_scaling_benchmark,
    uuid_generation_benchmark
);
criterion_main!(mesh_benches);
