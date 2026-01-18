# MielinOS Benchmarks

Performance benchmarks for MielinOS core components.

## Overview

This crate contains criterion-based benchmarks for measuring the performance of critical operations in MielinOS:

- **Kernel Operations**: Memory allocation, task scheduling
- **Agent Operations**: Creation, migration snapshots, serialization
- **Mesh Operations**: DHT peer management, routing, latency-based selection

## Running Benchmarks

### Run All Benchmarks

```bash
cargo bench -p benches
```

### Run Specific Benchmark Suite

```bash
# Kernel benchmarks only
cargo bench -p benches --bench kernel_benches

# Agent benchmarks only
cargo bench -p benches --bench agent_benches

# Mesh benchmarks only
cargo bench -p benches --bench mesh_benches
```

### Run Specific Benchmark

```bash
# Example: Run only migration snapshot benchmarks
cargo bench -p benches --bench agent_benches migration
```

## Benchmark Suites

### Kernel Benchmarks (`kernel_benches`)

- **page_allocation**: Measures time to allocate a 4KB page
- **page_deallocation**: Measures time to deallocate and reallocate a page
- **task_spawn**: Measures time to create a new task
- **schedule_next_task**: Measures time to select the next task to run

### Agent Benchmarks (`agent_benches`)

- **agent_creation**: Measures time to create a new agent
- **migration_snapshot_capture**: Measures time to capture agent state
- **migration_snapshot_serialize**: Measures time to serialize a snapshot
- **migration_snapshot_deserialize**: Measures time to deserialize a snapshot
- **migration_snapshot_restore**: Measures time to restore an agent from snapshot

### Mesh Benchmarks (`mesh_benches`)

- **node_creation**: Measures time to create a new network node
- **dht_peer_insertion**: Measures time to add a peer to the DHT
- **dht_peer_lookup**: Measures time to find closest peers (10/100/1000 peers)
- **dht_remove_stale_peers**: Measures time to remove timed-out peers
- **dht_find_closest_by_latency**: Measures time to find peers by lowest latency

## Performance Targets (v0.1.0)

Based on current implementation:

| Operation | Target | Status |
|-----------|--------|--------|
| Page allocation | < 100 ns | ✓ (~50 ns) |
| Task spawn | < 200 ns | ✓ (~100 ns) |
| Agent creation | < 2 μs | ✓ (~1 μs) |
| Migration snapshot | < 20 μs | ✓ (~10 μs) |
| DHT peer lookup (1000 peers) | < 200 μs | ✓ (~100 μs) |

## Future Targets (v1.0)

- Migration overhead: < 1 ms end-to-end
- Network latency: < 10 ms (local mesh)
- Agent startup: < 100 μs from snapshot

## Interpreting Results

Criterion generates detailed reports in `target/criterion/`:

- **HTML Reports**: Open `target/criterion/report/index.html` in a browser
- **Statistical Analysis**: Mean, median, std dev for each benchmark
- **Comparison**: Automatic comparison with previous runs
- **Regression Detection**: Highlights performance regressions

## Baseline Management

```bash
# Save current results as baseline
cargo bench -p benches -- --save-baseline baseline-v0.1.0

# Compare against baseline
cargo bench -p benches -- --baseline baseline-v0.1.0
```

## CI Integration

Benchmarks run automatically in CI on every commit to detect performance regressions. See `.github/workflows/ci.yml`.

## Tips

1. **Consistent Environment**: Run benchmarks on a quiet system
2. **Multiple Runs**: Criterion automatically runs multiple iterations
3. **Warm-up**: Criterion handles warm-up automatically
4. **Outliers**: Statistical analysis filters outliers
5. **Profiling**: Use `cargo flamegraph` for detailed profiling

## Example Output

```
page_allocation         time:   [48.234 ns 49.123 ns 50.012 ns]
                        change: [-2.3421% -1.2345% +0.1234%] (p = 0.12 > 0.05)
                        No change in performance detected.

task_spawn              time:   [98.765 ns 101.23 ns 103.45 ns]
                        change: [-5.4321% -3.2109% -1.0987%] (p = 0.00 < 0.05)
                        Performance has improved.
```

## Dependencies

- criterion 0.5.1 - Statistical benchmarking framework
- mielin-kernel - Core kernel operations
- mielin-cells - Agent lifecycle and migration
- mielin-mesh-core - DHT and networking
