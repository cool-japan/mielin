# Changelog

All notable changes to MielinOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-23 - "Oligodendrocyte" (Initial Release)

**First stable release** of MielinOS — a microkernel-based distributed agent mesh operating system built in 100% pure Rust.

### Highlights

- **~191 K lines of Rust** across the workspace (191,044 code lines per tokei)
- **4,570 tests passing** with zero clippy warnings
- Pure-Rust stack: oxicrypto-*, oxiquic-*, oxihttp-*, oxiarc-lz4/zstd replace all C/FFI equivalents
- Rust toolchain: nightly (rustc 1.98.0-nightly, 2026-06-19)

### Added

#### Core Crates

##### mielin (workspace root / lib)
- `MielinOS` top-level facade — microkernel-based OS for distributed AI agents with neural mesh networking

##### mielin-kernel
- Core unikernel implementation for agent execution across heterogeneous hardware (x86_64, AArch64, RISC-V, ARM Cortex-M)
- NUMA-aware memory subsystem with buddy allocator and 4-level page table management
- Hardware-assisted VMM: EPT/NPT with split-page guard mapping; IRQ injection via APIC/GIC
- IPC subsystem: synchronous channels, async message queues, shared memory regions
- **Lock-Free Work-Stealing Scheduler** (Chase-Lev 2005 + Lê/Pop/Cohen/Nardelli 2013 correction): `WorkStealingDeque<T>` with power-of-2 circular buffer and 2× growth; `WorkStealingScheduler` with 8 workers × 256 priority levels; LCG random victim selection; `WorkStealingMetrics` snapshot API
- 25 tests: NUMA, memory management, IPC, and Chase-Lev concurrency

##### mielin-hal
- Unified hardware abstraction layer across x86_64, AArch64, RISC-V, and ARM Cortex-M
- Architecture detection at runtime with compile-time feature gates
- MIPS / PowerPC HAL stubs (feature-gated); hardware database module with traits and types
- RISC-V and ARMv8-M MCU support; compiler-rt shim layer

##### mielin-rt
- Lightweight embedded runtime for Cortex-M and resource-constrained IoT devices
- Power management: WFI / WFE deep-sleep, SysTick timer, interrupt priority 0–255
- Memory pool bump allocator (`const`-generic size, alignment-aware, resettable)
- Battery-aware migration triggers (< 20% battery, not charging)
- **Energy-Aware Scheduling**: `EnergyPolicy` / `EnergyMode` (Performance/Balanced/PowerSave/BudgetEnforced); `SleepRecommendation` (StayAwake/LightSleep/DeepSleep/Hibernate); `recommend_sleep` decision tree
- `PowerDomain` + `PowerDomainTracker`: 16-slot fixed array, per-peripheral µJ accounting (`E = P_mW × Δt_µs / 1000`), `AtomicU32` total-power cache
- `EnergyAwareScheduler`: pre-schedule cap/budget/headroom hints; task start/stop bracketing; idle sleep recommendation
- `EnergyAdaptiveController`: proportional-window CPU frequency stepper, no-heap array-backed frequency table
- 29 tests (power modes, memory management, energy policy, energy scheduler)

##### mielin-cells
- Agent SDK: lifecycle management (`spawn`, `pause`, `resume`, `terminate`), policy execution, inter-agent messaging
- High-availability and disaster-recovery subsystems (HA/DR, multi-region, compliance)
- Agent versioning and cross-version live migration (v1→v2, v2→v1)
- **Fault Injection**: `FaultInjector` with `FaultKind` (Drop/Delay/Corrupt/Duplicate/Timeout), probabilistic LCG, per-label `max_occurrences` cap; builder helpers `always_drop`, `always_delay`, `occasionally`
- 15 fault injection tests (probability, cross-version migration, HA failover, corruption detection, retry-on-transient, concurrent injection)

##### mielin-mesh-core
- Kademlia DHT with XOR-distance routing, geographic-aware peer scoring, and latency-based sorting
- SWIM-inspired gossip protocol: node membership (Alive/Suspect/Dead), heartbeat failure detection (15 s suspect, 30 s dead), anti-entropy via sync requests, incarnation-number refutation
- mDNS discovery (mdns-sd 0.17) with 5-minute TTL caching; bootstrap node registry; peer exchange (PEX)
- Distributed agent registry: DHT-backed location tracking, content-addressable agent IDs, 10-minute TTL, replication factor 3
- Live migration coordinator: Pre-copy / Post-copy / Hybrid strategies; 9-phase telemetry (Planning→Complete); 30 s timeout; migration history & statistics API
- Integrated `MeshService` orchestrator: single entry point with optional enable/disable of mDNS, gossip, registry, and migration
- **Chaos testing** (10 tests): network partition cluster split, failure detection timing, gossip convergence after join/leave, split-brain prevention, node rejoin, concurrent joins, rapid churn, registry consistency, migration during partition

##### mielin-mesh-wire
- QUIC-based wire protocol using oxiquic-transport 0.1.4 + oxiquic-crypto 0.1.4 (pure-Rust QUIC, replaces quinn + ring)
- TLS 1.3 via rustls 0.23 + oxitls-rcgen 0.1.3; self-signed certificate generation via rcgen 0.14
- Certificate manager: `get_or_generate_cert`, `rotate_cert` (30-day threshold), expiry monitoring, thread-safe `Arc<RwLock>` cache
- Certificate storage backend (in-memory + file-based interface), PKCS#8 + DER serialization
- Multi-hop `RoutedMessage` envelope: TTL-based loop prevention (max 16 hops), transparent forwarding
- Connection pooling, stream multiplexing, 16 MB message size, 10 s connection timeout
- oxiarc-lz4 / oxiarc-zstd compression (replaces flate2/zstd C backends)
- 12 tests: QUIC transport, multi-path, health monitoring, certificate lifecycle

##### mielin-wasm
- WebAssembly sandboxing and execution using Wasmtime 43.0.1
- Capability-based resource isolation; WASI host-function bindings
- Feature-gated alternative engines: Wasmer, WASM3
- **WASI Preview 2 / Component Model** (`preview2` feature): `ComponentExecutor` with `P2HostState` implementing `WasiView + IoView`; minimal WIT world (`wasi:clocks/wall-clock`, `wasi:random/random`, `wasi:cli/environment`)
- TFLite model export (feature-gated); WASM tensor host functions (hardware capability queries, tensor creation, dot/add/matmul)
- 20+ tests: module loading, sandboxing, capability enforcement; 3 Component Model integration tests (compile, clock, random)

##### mielin-tensor
- Kernel-level tensor operations with runtime backend dispatch
- **ARM NEON** (AArch64): 128-bit SIMD (4×f32), `vld1q_f32`, `vmlaq_f32`, `vaddq_f32`, `vaddvq_f32`
- **x86_64 AVX2**: 256-bit SIMD (8×f32), `_mm256_loadu_ps`, `_mm256_mul_ps`, `_mm256_add_ps`, `_mm_hadd_ps`
- **ARM SVE2** (AArch64, stable since Rust 1.86): `svsub_f32_m`, `svdiv_f32_m`; full `dot_sve2`, `matmul_sve2`, `add_sve2`, `sub_sve2`, `mul_sve2`, `div_sve2` routing from `TensorOps`
- Scalar fallback; `no_std` compatible
- **Distributed Inference Engine** (`distributed.rs`): `PartitionStrategy` (RowWise/ColumnWise/Block/Pipeline); `TensorShard` / `ShardedTensor::partition` + `reconstruct`; `DistributedTransport` trait + `LocalTransport`; `ReduceOp` (Sum/Product/Max/Min); `distributed_matmul` via Cannon's algorithm; `ModelParallelLayer` + `ModelParallelPipeline` + `DistributedInferenceEngine` with FLOP/byte metrics
- Neural network layers: quantization, comprehensive benchmark harness (element-wise, matrix, SVE2)
- 16 distributed inference integration tests + SIMD unit tests

##### mielin-cli (`mielinctl` binary)
- Subcommands: `daemon`, `mesh status`, `agent list/spawn/migrate`, `config`, shell completion
- **HTTP Control Plane** (`control/` module): `ControlServer` (axum 0.8) + `ControlClient` (oxihttp-client 0.1.4, TLS); endpoints: health, mesh status/peers/nodes, agent CRUD, migrate/status
- `daemon --control-listen <addr>` (default `127.0.0.1:8081`); graceful mock fallback when daemon unreachable
- Rhai scripting engine for automation; comfy-table / tabled output formatting; rustyline REPL
- oxihttp-client replaces reqwest (pure-Rust HTTP)

#### Examples & Tooling

- `examples/mesh-cluster`: 3-node QUIC mesh cluster with Edge/Relay/Core roles, live agent migration, registry + gossip integration, optional TLS (`--use-certs`), migration telemetry display
- `examples/embedded-iot`: simulated temperature sensor node, battery lifecycle (100% → 15% → charging), automatic migration on low battery, power-mode transitions
- Benchmark crate (`benches`): criterion harnesses for SIMD, matrix ops, CLI, quantization, WASM

### Changed

#### Pure-Rust Migration (COOLJAPAN Policy)
- Replaced `ring` with `oxicrypto-*` sub-crates (hash, rand, aead, sig, kex, kdf, core)
- Replaced `quinn` + `ring` QUIC stack with `oxiquic-transport` + `oxiquic-crypto`
- Replaced `reqwest` HTTP client with `oxihttp-client 0.1.4`
- Replaced `flate2` / `zstd` (C backends) with `oxiarc-lz4 0.3.3` / `oxiarc-zstd 0.3.3`
- Replaced `bincode 1.x` with `oxicode 0.2.4` (with serde feature) for binary serialization
- Replaced `rcgen 0.13` with `oxitls-rcgen 0.1.3`

#### Toolchain & API Updates
- Rust toolchain bumped from 1.90.0 → 1.91.0 (wasmtime 43.0.1 MSRV)
- rand 0.10 API migration: `use rand::Rng` → `use rand::RngExt`; `fill_bytes` → `fill`
- wasmtime 43 compatibility: `.context()` replaced with `.map_err(|e| anyhow::anyhow!())` where `wasmtime::Error` is the type; removed deprecated `Config::async_support(false)`
- x86 architecture support added to HAL bootstrap

#### Clippy / Zero-Warnings
- `#[derive(Default)]` + `#[default]` variants replace manual `impl Default` in `mielin-mesh-wire` and `mielin-rt`
- `#[cfg(not(test))]` guard on kernel `cli` instruction to prevent SIGSEGV in userspace test runs
- Removed redundant `let i = i;` rebind in integration tests

### Fixed

- `MeshNetworkError::CircuitBreakerOpen` now uses `Option<SocketAddr>` instead of the placeholder address `"0.0.0.0:0"`, removing a hardcoded stub and a `.expect()` call from the retry executor
- REPL `execute_command` now dispatches to real command handlers via `Cli::try_parse_from`, replacing the stub "Command execution in REPL mode is a stub" message; parse errors print usage and return `Ok(())` so the REPL survives bad input
- `mielinctl monitor events` no longer panics with exit 134 (SIGABRT): renamed the local `output` field to `write_to` (`--write-to`/`-w`) to eliminate the clap flag collision with the global `--output`/`-o` flag
- `WasmerRuntime` and `Wasm3Runtime` doc comments and error strings no longer say "(stub)"; both are documented as wasmtime-backed compatibility aliases pending a native pure-Rust backend
- 36 wasmtime JIT tests annotated `#[cfg_attr(miri, ignore)]` so `cargo +nightly miri test` passes (Cranelift JIT is inherently incompatible with Miri's interpreter model)
- Four clippy errors fixed: `manual_checked_ops` in `mielin-kernel/src/bpf/interp.rs` (div/rem) and `mielin-mesh/core/src/metrics/types.rs` (avg); `unnecessary_sort_by` in `mielin-mesh/core/src/registry.rs` and `mielin-mesh/core/src/security/acl.rs`
- Hardcoded `"/tmp"` literal replaced with `std::env::temp_dir()` in `mielin-cli/src/script.rs`, `mielin-cli/src/plugin.rs`, and `mielin-wasm/src/filesystem.rs`
- `bootloader-api = "0.11.14"` added as a workspace dependency; `mielin-kernel` now imports `BootInfo` and `entry_point!` from `bootloader-api` (bootloader v0.11 moved the kernel-facing API to a separate crate)
- `llvm-tools` added to `rust-toolchain.toml` components — required by the bootloader build script to generate x86_64 ELF/binary images
- Bench test timing bounds in `mielin-mesh/core/tests/` increased to accommodate debug-build execution under parallel CI load: consistent-hash lookup 2 s → 15 s; 100-node gossip throughput 60 s → 180 s
- `mielin-kernel/src/bpf/verifier.rs`: `HelperId` import gated behind `#[cfg(not(feature = "bpf-maps"))]` to eliminate the unused-import warning when `--all-features` is enabled
- `mielin-kernel/src/lib.rs` and `mielin-kernel/src/boot.rs`: `kernel_main`, `entry_point!(kernel_main)`, and `boot_info.memory_regions` usage gated behind `target_arch = "x86_64"` to prevent dead-code warnings when building on AArch64 hosts

### Testing Summary

- **4,570 tests passing** (default) / **4,654 with --all-features** across all workspace crates — zero failures, zero clippy warnings
- Test breakdown by area:
  - `mielin-kernel`: NUMA, buddy allocator, VMM, scheduler, IPC, work-stealing
  - `mielin-hal`: architecture detection, capability queries
  - `mielin-rt`: power management, sensors, communication, energy scheduling
  - `mielin-cells`: agent lifecycle, migration, HA/DR, fault injection
  - `mielin-mesh-core`: DHT, gossip, registry, partition tolerance, chaos
  - `mielin-mesh-wire`: QUIC transport, TLS, certificate lifecycle, multi-path
  - `mielin-wasm`: module loading, sandboxing, capability enforcement, WASI Preview 2
  - `mielin-tensor`: SIMD backends (NEON/AVX2/SVE2), neural layers, distributed inference
  - `mielin-cli`: control plane, daemon, scripting
  - `mielin-tests`: cross-crate integration

---

## [0.1.0-rc.1] - 2026-01-17 - "Oligodendrocyte" (Release Candidate)

**First Release Candidate** - Core mesh networking and agent migration complete.

### Highlights

- **155,178 lines of Rust** across 445 files
- **3,255 tests passing** with zero clippy warnings
- **Complete QUIC transport** with TLS 1.3 encryption
- **P2P mesh networking** with mDNS discovery and gossip protocol
- **Live agent migration** with delta compression
- **Production features**: HA, DR, multi-region, compliance

### Added (Phase 2 - Complete)

#### QUIC Transport (mielin-mesh/wire)
- Real QUIC transport using quinn library
- TLS 1.3 encryption with self-signed certificates
- Connection pooling and reuse
- Stream multiplexing support
- 16MB message size support
- 10-second connection timeout
- Server and client endpoint support

#### DHT-based Routing (mielin-mesh/core)
- Peer address storage in DHT routing table
- Greedy routing with XOR distance metric
- Direct and indirect peer routing
- Peer address lookup and management
- Latency-based peer sorting
- Automatic peer table maintenance
- 5 new routing tests (route_to, get_address, remove_peer, etc.)

#### Multi-Hop Message Routing (mielin-mesh/wire)
- RoutedMessage envelope with source, destination, TTL, hop_count
- Automatic message forwarding through intermediate nodes
- TTL-based loop prevention (max 16 hops)
- Message routing methods: route(), forward(), is_for(), unwrap_payload()
- Transparent routing for all message types
- 6 new routing tests (routed_message, forward, ttl_expiry, etc.)

#### Node Discovery Protocol (mielin-mesh/core)
- mDNS-based local network discovery using mdns-sd 0.17.0
- Automatic service announcement and browsing
- Peer information caching with 5-minute TTL
- Bootstrap node registry for WAN connectivity
- Peer exchange protocol (PEX) for mesh growth
- Discovery service with start/stop lifecycle management
- Automatic cleanup of expired peers
- 5 comprehensive integration tests

#### Gossip Protocol (mielin-mesh/core)
- SWIM-inspired gossip protocol for state synchronization
- Node membership management with health tracking (Alive, Suspect, Dead)
- Heartbeat-based failure detection (15s suspect, 30s dead)
- Anti-entropy reconciliation via sync requests/responses
- Incarnation numbers for refuting false suspicions
- State dissemination with version tracking
- Background tasks for heartbeat, failure detection, and gossip propagation
- 9 comprehensive tests covering all protocol aspects

#### Distributed Agent Registry (mielin-mesh/core)
- DHT-based agent location tracking and discovery
- Content-addressable agent IDs (16-byte hash)
- Agent location caching with 10-minute TTL
- Replication factor of 3 for fault tolerance
- Support for agent registration, deregistration, and updates
- Query API for agent location lookups
- Metadata support for agent tagging
- Automatic cleanup of expired registry entries
- Integration with DHT for closest-node calculation
- 10 comprehensive tests covering all registry operations

#### Live Migration Service (mielin-mesh/core)
- Migration coordinator for orchestrating agent migrations
- Three migration strategies: Pre-copy, Post-copy, and Hybrid
- Pre-copy: iteratively copy memory while agent runs (max 3 iterations)
- Post-copy: pause, copy minimal state, resume on target, background transfer
- Hybrid: start with pre-copy, switch to post-copy if stalled
- Comprehensive migration telemetry tracking
  - Migration phases (Planning → PreCopy → Pausing → Copying → Transferring → Validating → Resuming → Cleanup → Complete)
  - Downtime measurement (pause to resume)
  - Success/failure rates
  - Average migration duration
- Migration timeout detection (30s max)
- Migration history and statistics API
- 8 comprehensive tests covering all strategies

#### Integrated Mesh Service (mielin-mesh/core)
- Unified MeshService orchestrating all mesh components
- Single entry point for all mesh operations
- Lifecycle management (start/stop) for all services
- Automatic peer synchronization between components
  - Discovery → Gossip membership
  - Discovery → DHT routing table
- Component integration with optional enable/disable
  - mDNS discovery (enable_mdns)
  - Gossip protocol (enable_gossip)
  - Agent registry (enable_registry)
  - Live migration (enable_migration)
- Comprehensive API surface covering all subsystems
- Configuration via MeshConfig struct
- 6 integration tests covering full stack

#### 3-Node Mesh Cluster Example
- Complete mesh-cluster example demonstrating QUIC networking
- **UPDATED**: Now uses integrated MeshService orchestrator
- **UPDATED**: Optional TLS certificate management with `--use-certs` flag
- Support for Edge, Relay, and Core node roles
- CLI interface with clap
- Live agent migration over network with migration coordinator tracking
- Agent registry integration for automatic location tracking
- Gossip protocol membership tracking
- Bootstrap node support for peer discovery
- Mesh status display showing gossip, registry, migration, and certificate stats
- Migration acknowledgment system
- Certificate lifecycle monitoring (expiry warnings, rotation status)
- Simplified codebase using high-level MeshService API

#### Serialization Updates
- Migrated to bincode 2.0.1 with serde feature
- Updated Message and MigrationSnapshot serialization
- Used bincode::serde::Compat wrapper for compatibility

#### Certificate Management (mielin-mesh/wire)
- **TLS Certificate Infrastructure** for secure mesh communication
  - Self-signed certificate generation using rcgen 0.13
  - TLS 1.3 support via rustls integration
  - Certificate rotation with 30-day threshold
  - Automatic expiry detection and management
- **Certificate Manager** with lifecycle management
  - `get_or_generate_cert`: Automatic certificate provisioning
  - `rotate_cert`: Manual certificate rotation
  - `needs_rotation`: Expiry monitoring
  - Thread-safe certificate caching with Arc<RwLock>
- **Certificate Storage Backend**
  - In-memory storage for development
  - File-based storage interface (future persistence)
  - Certificate CRUD operations (store, retrieve, delete, list)
  - Automatic cleanup of expired certificates
- **Certificate Metadata Tracking**
  - Common name (node ID) and subject alternative names
  - Creation and expiration timestamps
  - Validity period configuration (default 365 days)
  - Time-until-expiry calculations
- **Self-Signed Certificate Features**
  - Localhost and 127.0.0.1 SANs for testing
  - Customizable validity periods
  - PKCS#8 private key serialization
  - DER-encoded certificate chains
- 10 comprehensive tests covering generation, rotation, and storage
- 2 additional QUIC transport integration tests

#### TensorLogic (mielin-tensor)
- **Hardware-Accelerated Tensor Operations**: Dot product, element-wise add, matrix multiplication
- **ACTUAL SIMD Intrinsics Implementation** (Phase 2):
  - **ARM NEON**: 128-bit SIMD vectors (4 x f32) for AArch64
    - `vld1q_f32`: Load 4 floats
    - `vmlaq_f32`: Fused multiply-add
    - `vaddq_f32`: Vector addition
    - `vaddvq_f32`: Horizontal sum
    - Processes chunks of 4 with scalar remainder
  - **x86_64 AVX2**: 256-bit SIMD vectors (8 x f32)
    - `_mm256_loadu_ps`: Load 8 floats
    - `_mm256_mul_ps`: Vector multiply
    - `_mm256_add_ps`: Vector addition
    - Horizontal sum using `_mm_hadd_ps`
    - Processes chunks of 8 with scalar remainder
  - **Matrix-Vector Multiplication**: Optimized building block for matmul
  - **Safety Documentation**: All intrinsics functions with proper # Safety sections
- **Multi-Backend Support**: SVE2 (fallback to NEON), NEON, AVX2, and scalar fallback
- **WASM Integration**: Host functions exposing tensor operations to agents
  - Hardware capability queries (tensor_supports_sve2, tensor_supports_neon, tensor_supports_avx2)
  - Tensor creation (tensor_zeros, tensor_ones)
  - Tensor operations (tensor_dot, tensor_add, tensor_matmul)
  - Tensor introspection (tensor_get_shape, tensor_free)
- **no_std Compatible**: Works in kernel and userspace environments
- **Runtime Dispatch**: Automatic selection of optimal backend based on hardware capabilities
- **Comprehensive Testing**: 20 unit tests covering all operations and backends (6 new SIMD tests)

#### Build System
- Made bootloader dependency optional (requires nightly)
- Added `bootable` feature flag to mielin-kernel
- Enabled testing on stable Rust 1.90.0
- Updated rust-toolchain.toml to 1.90.0

#### Embedded Runtime Enhancements (mielin-rt) - Phase 2
- **Cortex-M Specific Runtime Module** (mielin-rt/src/cortex_m.rs):
  - Power management with WFI (Wait For Interrupt) and WFE (Wait For Event)
  - Deep sleep mode with SCB register configuration
  - SysTick timer support for millisecond timing
  - Interrupt priority levels (0-255 range)
  - Memory pool allocator for no-heap embedded systems
    - Bump allocator with alignment support
    - Const-generic size parameter
    - Reset capability for reuse
  - 5 comprehensive tests for power modes and memory management
- **Enhanced Embedded Runtime**:
  - Battery-aware migration triggers (<20% battery, not charging)
  - Power mode management (Normal, LowPower, UltraLowPower, Sleep)
  - Architecture detection integration
  - Low power wait states (WFI on ARM)
  - 3 new integration tests
- **Embedded IoT Example** (examples/embedded-iot):
  - Simulated temperature sensor node
  - Battery lifecycle simulation (100% → 15% → charging)
  - Automatic migration triggers on low battery
  - Power mode transitions based on battery level
  - Real-world IoT device behavior demonstration
  - 3 example tests

### Testing Summary
- **3,255 tests passing** across all workspace crates
- **Zero clippy warnings** with strict lints (-D warnings)
- **100% pass rate** for all features
- Comprehensive test coverage:
  - mielin-kernel: NUMA, scheduler, memory management
  - mielin-cells: Agent lifecycle, migration, HA/DR
  - mielin-mesh-core: DHT, gossip, registry, partition tolerance
  - mielin-mesh-wire: QUIC transport, health monitoring, multi-path
  - mielin-rt: Power management, sensors, communication protocols
  - mielin-wasm: Module loading, sandboxing, capability enforcement
  - mielin-tensor: SIMD operations, neural network layers

## Previous Versions

This is the initial release of MielinOS.

---

## Version Naming

MielinOS versions are named after key components of the nervous system:

- **v0.1 "Oligodendrocyte"**: Cells that produce myelin in the central nervous system
- **v0.2 "Ranvier"**: Nodes of Ranvier (gaps in myelin sheath where saltatory conduction occurs)
- **v0.3 "Schwann"**: Cells that produce myelin in the peripheral nervous system
- **v1.0 "Saltatory"**: Saltatory conduction (the fast jumping of signals)

## Links

- [Repository](https://github.com/cool-japan/mielin)
- [Issues](https://github.com/cool-japan/mielin/issues)
- [Discussions](https://github.com/cool-japan/mielin/discussions)
- [Documentation](https://github.com/cool-japan/mielin/blob/main/README.md)

[0.1.0]: https://github.com/cool-japan/mielin/releases/tag/v0.1.0
[0.1.0-rc.1]: https://github.com/cool-japan/mielin/releases/tag/v0.1.0-rc.1
