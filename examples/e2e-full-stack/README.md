# MielinOS End-to-End Full-Stack Example

This comprehensive example demonstrates all major MielinOS components working together in a realistic distributed AI workload scenario.

## Overview

This example showcases the integration of:

- **mielin-hal**: Hardware Abstraction Layer with SIMD detection
- **mielin-cells**: Agent lifecycle and migration management
- **mielin-mesh-core**: Distributed mesh networking with gossip protocol
- **mielin-mesh-wire**: QUIC transport and message routing
- **mielin-tensor**: Tensor operations with hardware acceleration
- **mielin-kernel**: Scheduler and task management

## Architecture

```
                          +-------------------+
                          |  Orchestrator     |
                          |  (This Example)   |
                          +--------+----------+
                                   |
           +----------------------++-----------------------+
           |                       |                       |
  +--------v--------+    +--------v--------+    +---------v--------+
  |   Core Node     |    |   Relay Node    |    |    Edge Node     |
  | (QUIC Server)   |    | (QUIC Bridge)   |    | (QUIC Client)    |
  |                 |    |                 |    |                  |
  | - Agent Pool    |    | - Agent Pool    |    | - Agent Pool     |
  | - Tensor Compute|    | - Load Balance  |    | - ML Inference   |
  | - Migration Mgr |    | - Migration Mgr |    | - HAL Detection  |
  +-----------------+    +-----------------+    +------------------+
```

## Components Demonstrated

### 1. Mesh Networking

- 3-node QUIC mesh cluster (Core, Relay, Edge)
- Gossip protocol for membership management
- Health monitoring and status tracking
- Message routing with TTL and hop counting

### 2. Agent Deployment

- Create compute agents with WASM binaries
- Deploy agents across mesh nodes
- Agent registry for distributed tracking
- Scheduler integration for task management

### 3. Tensor Processing

- Hardware-accelerated tensor operations
- Matrix multiplication benchmarking
- Neural network inference (Dense, Conv2D layers)
- INT8 quantization and dequantization
- SIMD detection (NEON, SVE2, AVX2, AVX-512)

### 4. Live Migration

- Pre-copy migration strategy (high consistency)
- Post-copy migration strategy (low downtime)
- Delta migration strategy (efficient updates)
- Multi-hop migration between nodes
- Snapshot serialization and validation

### 5. HAL Integration

- Architecture detection (x86_64, AArch64, RISC-V)
- SIMD capability enumeration
- Hardware profile generation
- Optimization hints for tensor operations

### 6. Kernel Features

- Priority-based cooperative scheduler
- Task spawning and termination
- Work-stealing between schedulers
- Scheduler metrics and utilization tracking

## Usage

### Run Full Demonstration

```bash
cargo run --example e2e-full-stack
```

### Run with Verbose Logging

```bash
RUST_LOG=debug cargo run --example e2e-full-stack
```

### Run Specific Phases

```bash
# Hardware abstraction layer only
cargo run --example e2e-full-stack -- --phase hal

# Mesh networking only
cargo run --example e2e-full-stack -- --phase mesh

# Tensor computation only
cargo run --example e2e-full-stack -- --phase tensor

# Agent migration only
cargo run --example e2e-full-stack -- --phase migration

# Kernel features only
cargo run --example e2e-full-stack -- --phase kernel

# All phases (default)
cargo run --example e2e-full-stack -- --phase all
```

### Configure Parameters

```bash
# Deploy 10 agents, use 512x512 matrices
cargo run --example e2e-full-stack -- --agent-count 10 --matrix-size 512

# Enable profiling
cargo run --example e2e-full-stack -- --profile
```

## Command Line Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--phase` | `-p` | `all` | Phase to demonstrate (all, mesh, tensor, migration, hal, kernel) |
| `--verbose` | `-v` | `1` | Verbosity level (0-3) |
| `--agent-count` | `-a` | `5` | Number of agents to deploy |
| `--matrix-size` | `-m` | `256` | Matrix size for tensor benchmarks |
| `--profile` | | `false` | Enable performance profiling |

## Sample Output

```
################################################################
#                                                              #
#  MielinOS End-to-End Full-Stack Demonstration                #
#  Version: v0.1.0-rc.1 (Development Preview)                       #
#                                                              #
#  Demonstrating: Mesh + Tensor + Migration + HAL + Kernel     #
#                                                              #
################################################################

Configuration:
  Phase: All
  Agents: 5
  Matrix Size: 256x256
  Profiling: Disabled
  Timestamp: 2026-01-18 12:00:00 UTC

=== Phase 4: Hardware Abstraction Layer ===

Architecture Detection:
  Detected: aarch64
  Family: ARM 64-bit
  Potential SIMD: NEON, SVE, SVE2, SME

Hardware Profile:
  CPU Cores: 8
  Memory Size: 16384 MB

SIMD Capabilities:
  NEON:    YES
  SVE:     NO
  SVE2:    NO
  SME:     NO
  AVX2:    NO
  AVX-512: NO

...
```

## Key Concepts

### Saltatory Conduction

Agents "jump" between nodes via efficient migration, similar to how nerve impulses propagate along myelinated axons. This enables:

- Rapid workload redistribution
- Fault tolerance through mobility
- Load balancing across heterogeneous hardware

### Adaptive Scheduling

Work is distributed based on hardware capabilities:

- SIMD-optimized tensor operations on capable nodes
- Priority-based task scheduling
- Work-stealing for load balancing

### Zero-Copy Tensors

Efficient memory management for ML workloads:

- Memory-mapped tensor storage
- Reference-counted sharing
- Cache-aware blocking algorithms

### Fault Tolerance

Health monitoring and automatic failover:

- Gossip-based failure detection
- Automatic agent migration on node failure
- Checkpoint-based recovery

## Building

```bash
# From the mielin root directory
cargo build --example e2e-full-stack

# Release build for benchmarking
cargo build --example e2e-full-stack --release
```

## Testing

```bash
# Run the example's unit tests
cargo test --example e2e-full-stack
```

## Dependencies

This example depends on all major MielinOS crates:

- `mielin-hal` - Hardware abstraction
- `mielin-cells` - Agent management
- `mielin-mesh-core` - Mesh networking
- `mielin-mesh-wire` - Wire protocol
- `mielin-tensor` - Tensor operations
- `mielin-kernel` - Kernel primitives (with `std` feature)

## License

Apache-2.0
