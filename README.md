# MielinOS

**A Microkernel-Based Operating System for Distributed AI Agents**

MielinOS is a next-generation operating system designed from the ground up for distributed AI agents with neural mesh networking capabilities. Named after the myelin sheath that enables rapid signal transmission in biological neural networks, MielinOS provides the infrastructure for agents to migrate, communicate, and execute across heterogeneous hardware platforms.

**Current Status:** v0.1.0 "Oligodendrocyte" (Released 2026-06-23)

## Overview

MielinOS provides a complete platform for building, deploying, and managing distributed AI agent systems:

- **Microkernel Architecture**: Minimal trusted computing base with capability-based security
- **Neural Mesh Networking**: Kademlia-based DHT with QUIC transport for agent communication
- **Live Agent Migration**: Seamless state transfer between nodes with zero downtime
- **Hardware Abstraction**: Unified interface across Arm, RISC-V, x86, and embedded platforms
- **WebAssembly Sandboxing**: Portable agent execution with strong isolation guarantees
- **Tensor Acceleration**: Hardware-aware ML operations with SVE2, AVX512, NPU support

## Quick Start

Add MielinOS to your project:

```toml
[dependencies]
mielin = "0.1.0"
```

Basic usage:

```rust
use mielin::prelude::*;

fn main() {
    // Detect hardware capabilities
    let arch = detect_architecture();
    println!("Running on: {:?}", arch);

    // Create an agent from its WASM binary ("DNA")
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let agent = Agent::new(wasm_binary);
    println!("Agent ID: {}", agent.id());
    println!("Agent state: {:?}", agent.state());

    // Access tensor operations (Tensor::zeros takes a Vec<usize> shape)
    let tensor = Tensor::zeros(vec![2, 3]);
    println!("Tensor shape: {:?}", tensor.shape());
}
```

(Compile-verified against `mielin` 0.1.0; see [`docs/TUTORIALS.md`](./docs/TUTORIALS.md) for a
full, runnable walkthrough — `cargo run -p hello-agent` runs the equivalent agent-creation example
directly.)

## Crate Organization

MielinOS is organized as a workspace with specialized crates:

| Crate | Description | crates.io |
|-------|-------------|-----------|
| [`mielin`](./mielin) | Meta crate re-exporting all components | [![crates.io](https://img.shields.io/crates/v/mielin.svg)](https://crates.io/crates/mielin) |
| [`mielin-kernel`](./mielin-kernel) | Microkernel with capability-based IPC | [![crates.io](https://img.shields.io/crates/v/mielin-kernel.svg)](https://crates.io/crates/mielin-kernel) |
| [`mielin-hal`](./mielin-hal) | Hardware Abstraction Layer | [![crates.io](https://img.shields.io/crates/v/mielin-hal.svg)](https://crates.io/crates/mielin-hal) |
| [`mielin-rt`](./mielin-rt) | Embedded runtime for IoT devices | [![crates.io](https://img.shields.io/crates/v/mielin-rt.svg)](https://crates.io/crates/mielin-rt) |
| [`mielin-mesh-core`](./mielin-mesh/core) | Distributed hash table and routing | [![crates.io](https://img.shields.io/crates/v/mielin-mesh-core.svg)](https://crates.io/crates/mielin-mesh-core) |
| [`mielin-mesh-wire`](./mielin-mesh/wire) | QUIC-based wire protocol | [![crates.io](https://img.shields.io/crates/v/mielin-mesh-wire.svg)](https://crates.io/crates/mielin-mesh-wire) |
| [`mielin-cells`](./mielin-cells) | Agent SDK and lifecycle management | [![crates.io](https://img.shields.io/crates/v/mielin-cells.svg)](https://crates.io/crates/mielin-cells) |
| [`mielin-wasm`](./mielin-wasm) | WebAssembly runtime integration | [![crates.io](https://img.shields.io/crates/v/mielin-wasm.svg)](https://crates.io/crates/mielin-wasm) |
| [`mielin-tensor`](./mielin-tensor) | Tensor operations with hardware acceleration | [![crates.io](https://img.shields.io/crates/v/mielin-tensor.svg)](https://crates.io/crates/mielin-tensor) |
| [`mielin-cli`](./mielin-cli) | Command-line interface | [![crates.io](https://img.shields.io/crates/v/mielin-cli.svg)](https://crates.io/crates/mielin-cli) |

### Using Individual Crates

You can depend on specific crates instead of the meta crate:

```toml
[dependencies]
mielin-hal = "0.1.0"      # Hardware abstraction only
mielin-tensor = "0.1.0"   # Tensor operations only
mielin-cells = "0.1.0"    # Agent SDK only
```

## Architecture

MielinOS uses a layered architecture inspired by biological neural networks:

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 4: Applications & Agents                                  │
│          • AI Agents (WASM sandboxed)                          │
│          • User Applications                                    │
├─────────────────────────────────────────────────────────────────┤
│ Layer 3: Services & Runtime                                     │
│          • mielin-cells (Agent SDK)                            │
│          • mielin-wasm (WebAssembly Runtime)                   │
│          • mielin-tensor (Tensor Operations)                   │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: Mesh Networking                                        │
│          • mielin-mesh-core (DHT, Routing)                     │
│          • mielin-mesh-wire (QUIC Protocol)                    │
├─────────────────────────────────────────────────────────────────┤
│ Layer 1: Kernel & Runtime                                       │
│          • mielin-kernel (Microkernel)                         │
│          • mielin-rt (Embedded Runtime)                        │
├─────────────────────────────────────────────────────────────────┤
│ Layer 0: Hardware Abstraction                                   │
│          • mielin-hal (HAL)                                    │
│          ↕                                                      │
│ Physical: Arm | RISC-V | x86 | Cortex-M | NPU | GPU            │
└─────────────────────────────────────────────────────────────────┘
```

## Features

### Hardware Support

| Platform | Architecture | Status |
|----------|--------------|--------|
| AWS Graviton | AArch64 | ✅ Full support |
| Apple Silicon | AArch64 | ✅ Full support |
| Intel/AMD | x86_64 | ✅ Full support |
| Raspberry Pi | AArch64/Arm | ✅ Full support |
| STM32 | Cortex-M | ✅ Embedded support |
| ESP32 | RISC-V/Xtensa | ✅ Embedded support |
| SiFive | RISC-V 64 | ⚠️ Experimental |

### Vector Extensions

- **Arm**: SVE, SVE2, SME, NEON
- **x86**: AVX, AVX2, AVX512, AMX
- **RISC-V**: Vector Extension (experimental)

### Key Capabilities

- **Agent Migration**: Live state transfer between nodes
- **Mesh Networking**: Decentralized peer discovery and routing
- **Fault Tolerance**: Automatic failover and recovery
- **Multi-tenancy**: Namespace isolation and resource quotas
- **Observability**: Distributed tracing and metrics
- **Security**: mTLS, capability-based access control

## Examples

### Running a Mesh Cluster

`mielin-cli` builds a binary named `mielinctl`. There is no `mesh start`/`mesh join` subcommand —
bring up a node with `daemon`, and join an existing cluster by passing a `--bootstrap` address:

```bash
# Start the MielinOS daemon (mesh service + HTTP control-plane API)
cargo run -p mielin-cli -- daemon --listen 0.0.0.0:9000 --role edge --control-listen 127.0.0.1:8081

# In another terminal: join an existing cluster's bootstrap node
cargo run -p mielin-cli -- node join 192.168.1.100:9000

# Deploy an agent
cargo run -p mielin-cli -- agent deploy ./my-agent.wasm
```

See [`docs/TUTORIALS.md`](./docs/TUTORIALS.md) (Tutorial 11) for the full mesh-cluster walkthrough,
including the standalone `examples/mesh-cluster` binary and which `mielinctl` subcommands are wired
to a live daemon versus illustrative.

### Creating an Agent

`Agent` is a concrete struct with a lifecycle state machine — not a trait you implement. Create one
from a WASM binary, attach a `Policy`, and drive it through valid state transitions:

```rust
use mielin_cells::{Agent, AgentState, Policy, TransitionResult};

fn main() {
    // Agent::new assigns a random UUID and starts the agent in `Created` state
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut agent = Agent::new(wasm_binary);
    assert_eq!(agent.state(), &AgentState::Created);

    // Attach an execution policy (battery/latency/architecture constraints)
    let policy = Policy {
        min_battery_percent: 20,
        max_latency_ms: 100,
        preferred_architectures: vec!["x86_64".to_string()],
    };
    policy.validate().expect("policy should be valid");
    agent.set_policy(policy);

    // Created -> Running is a valid transition
    match agent.start() {
        TransitionResult::Success => {
            println!("Agent {} is now {:?}", agent.id(), agent.state());
        }
        other => println!("Unexpected transition result: {other:?}"),
    }
}
```

See [`docs/TUTORIALS.md`](./docs/TUTORIALS.md) (Tutorials 1–2) for the full lifecycle state
machine, error recovery, and migration, or run `cargo run -p hello-agent` /
`cargo run -p agent-migration` directly.

### Hardware-Aware Tensor Operations

`Tensor` has no `randn`/RNG-based constructor; use `zeros`/`ones`/`filled`/`from_vec`, and dispatch
hardware-accelerated ops through `TensorRuntime::ops()` (which returns `Option`, not a bare value):

```rust
use mielin_hal::capabilities::HardwareProfile;
use mielin_tensor::{Tensor, TensorRuntime};

fn main() {
    let hw = HardwareProfile::detect();
    println!("Vector width: {} bits", hw.max_vector_width());

    let runtime = TensorRuntime::new(hw.capabilities);
    println!("Backend: {}", runtime.acceleration_info());

    // Operations dispatch to the best available SIMD backend via TensorRuntime::ops()
    let a = Tensor::zeros(vec![1024, 1024]);
    let b = Tensor::zeros(vec![1024, 1024]);
    let c = runtime.ops().matmul(&a, &b).expect("shape mismatch");

    println!("Result shape: {:?}", c.shape());
}
```

See [`docs/TUTORIALS.md`](./docs/TUTORIALS.md) (Tutorials 7–8) for quantization and multi-backend
benchmarking, or run `cargo run -p tensor-inference` / `cargo run --release -p e2e-tensor-compute`.

## Building from Source

### Prerequisites

- Rust nightly (1.98.0-nightly, per `rust-toolchain.toml`)
- For kernel development: `rustup target add x86_64-unknown-none`
- For WASM agents: `rustup target add wasm32-wasip1`

### Build Commands

```bash
# Build all default crates
cargo build

# Build with all features
cargo build --all-features

# Build the kernel (requires bare-metal target)
cargo build -p mielin-kernel --target x86_64-unknown-none

# Run tests
cargo test

# Run benchmarks
cargo bench -p benches
```

## Documentation

- [API Documentation](https://docs.rs/mielin) - Rustdoc reference
- [Tutorials](./docs/TUTORIALS.md) - Hands-on, compile-verified tutorial series (start here)
- [Architecture Guide](./docs/ARCHITECTURE.md) - System design details
- [Migration Guide](./docs/MIGRATION.md) - Agent migration protocol
- [Protocol Reference](./docs/PROTOCOL.md) - Wire protocol and message format
- [Networking Guide](./docs/NETWORKING.md) - Mesh discovery, gossip, and QUIC transport
- [Certificates Guide](./docs/CERTIFICATES.md) - TLS certificate issuance and rotation
- [Deployment Guide](./docs/DEPLOYMENT.md) - Running MielinOS in production/cluster environments
- [Troubleshooting Guide](./docs/TROUBLESHOOTING.md) - Common issues and how to resolve them
- [Performance Tuning](./docs/PERFORMANCE_TUNING.md) - Benchmarks and optimization guidance
- [Security Policy](./SECURITY.md) - Vulnerability reporting and capability-based security model
- [Code of Conduct](./CODE_OF_CONDUCT.md) - Community guidelines

## Roadmap

### Phase 1: "Oligodendrocyte" (v0.1.x) - Foundation ✅
- ✅ Microkernel with capability-based IPC
- ✅ Hardware abstraction layer
- ✅ Basic mesh networking (DHT, QUIC)
- ✅ WebAssembly agent runtime
- ✅ Tensor operations with SIMD

### Phase 2: "Ranvier" (v0.2.x) - Q3-Q4 2026
- [ ] Live agent migration
- [ ] Multi-region deployment
- [ ] GPU/NPU acceleration
- [ ] Distributed tracing

### Phase 3: "Schwann" (v0.3.x) - Q3-Q4 2026
- [ ] Consensus protocols
- [ ] State replication
- [ ] Edge computing optimizations
- [ ] IoT gateway support

### Phase 4: "Saltatory" (v1.0) - 2027
- [ ] Production hardening
- [ ] Enterprise features
- [ ] Compliance certifications
- [ ] Long-term support

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

Areas where we need help:
- Hardware drivers for new platforms
- Performance optimizations
- Documentation and examples
- Security auditing

## Sponsorship

MielinOS is developed and maintained by **COOLJAPAN OU (Team Kitasan)**.

If you find MielinOS useful, please consider sponsoring the project to support continued development of the Pure Rust ecosystem.

[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%9D%A4-red?logo=github)](https://github.com/sponsors/cool-japan)

**[https://github.com/sponsors/cool-japan](https://github.com/sponsors/cool-japan)**

Your sponsorship helps us:
- Maintain and improve the COOLJAPAN ecosystem
- Keep the entire ecosystem (OxiBLAS, OxiFFT, SciRS2, etc.) 100% Pure Rust
- Provide long-term support and security updates

## License

Licensed under:

- Apache License, Version 2.0 ([LICENSE](./LICENSE))

## Acknowledgments

MielinOS is developed by **COOLJAPAN OU (Team Kitasan)**.

Special thanks to the Rust community and the authors of our dependencies.

---

**MielinOS** - Enabling AI agents to traverse seamlessly across the neural mesh

*Named after the myelin sheath - the biological structure enabling rapid signal transmission in neural networks*
