# Changelog

All notable changes to MielinOS will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added (iteration 2)

#### Distributed Inference Engine (mielin-tensor/src/distributed.rs)
- `PartitionStrategy` — RowWise, ColumnWise, Block{rows,cols}, Pipeline{stages}
- `TensorShard` / `ShardedTensor::partition` + `reconstruct` — lossless roundtrip for all strategies; non-divisible dimensions distributed via `base+1` remainder
- `DistributedTransport` trait + `LocalTransport` (Arc<Mutex<HashMap>>) for in-process simulation
- `ReduceOp` (Sum, Product, Max, Min) with correct identity elements; used by `all_reduce`
- `distributed_matmul` — Cannon's algorithm on a p_r×p_c process grid; bit-near-identical to local matmul
- `ModelParallelLayer::forward` — RowWise concatenates shard outputs; ColumnWise all-reduces partial products
- `ModelParallelPipeline` + `DistributedInferenceEngine` with compile-time `LayerSpec` → `CompiledModel` plan; exposes FLOP and inter-node-byte metrics
- 16 integration tests covering all strategies, all reduce ops, distributed matmul, and engine end-to-end

#### Lock-Free Work-Stealing Scheduler (mielin-kernel)
- `WorkStealingDeque<T>` — Chase-Lev (2005) + Lê/Pop/Cohen/Nardelli (2013) correction; power-of-2 circular buffer with 2× growth; `push` (owner), `pop` (owner), `steal` (any thread); `Steal<T>` = Success/Empty/Retry
- `WorkStealingScheduler` — 8-worker array; per-worker 256-priority-level deques; LCG random victim selection; `spawn_task`, `schedule`, `yield_task`, `terminate_task`; `WorkStealingMetrics` + snapshot
- 21 new tests (4 Chase-Lev unit + 17 scheduler) covering concurrency, work stealing, priority ordering, load balance

#### Fault Injection and Chaos Testing (mielin-cells, mielin-mesh/core)
- `mielin-cells/src/fault.rs` — `FaultInjector` with `FaultKind` (Drop/Delay/Corrupt/Duplicate/Timeout); probabilistic LCG; per-label `max_occurrences` cap; builder helpers `always_drop`, `always_delay`, `occasionally`
- 15 fault injection tests: probability distribution, cross-version migration (v1→v2, v2→v1), HA failover under node failure, corruption detection, retry-on-transient, concurrent injection, composition
- 10 chaos tests in `mielin-mesh/core/tests/chaos_tests.rs`: network partition splits cluster, failure detection timing, gossip convergence after join/leave, split-brain prevention, node rejoin, concurrent joins, rapid churn, registry consistency, migration during partition

#### Energy-Aware Scheduling (mielin-rt)
- `EnergyPolicy` / `EnergyMode` (Performance/Balanced/PowerSave/BudgetEnforced) + `SleepRecommendation` (StayAwake/LightSleep/DeepSleep/Hibernate); `recommend_sleep` decision tree
- `PowerDomain` + `PowerDomainTracker` — 16-slot fixed array; per-peripheral power accounting with `E = P_mW × Δt_µs / 1000` µJ accrual on state transitions; `AtomicU32` total-power cache
- `EnergyAwareScheduler` in `src/energy_scheduler.rs` — pre-schedule hint (power-cap check, budget exhaustion, 20%-headroom warning), task start/stop bracketing, idle sleep recommendation
- `EnergyAdaptiveController` — proportional window controller; steps CPU frequency up/down based on energy utilisation vs target; no-heap, array-backed frequency table
- `PowerManager::suggest_next_state` — maps `EnergySchedulingHint.sleep_recommendation` to `PowerState`
- 21 new tests across energy.rs and energy_scheduler.rs

### Test Results
- **3704 tests run: 3704 passed** (up from 3617, net +87 new tests)
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0

### Added

#### SVE2 Dispatcher Wire-up (mielin-tensor)
- `sub_sve2` and `div_sve2` backend functions using `svsub_f32_m` / `svdiv_f32_m` intrinsics
- `dot_sve2`, `matmul_sve2`, `add_sve2`, `sub_sve2`, `mul_sve2`, `div_sve2` fully routed from `TensorOps` on `target_feature=+sve2` AArch64
- Removed obsolete "requires nightly Rust" comment — SVE2 intrinsics stabilised in Rust 1.86
- 2 new unit tests (`test_sve2_sub`, `test_sve2_div`) in the sve2 backend
- Benchmark harness extended with SVE2 in element-wise and matrix benchmarks

#### HTTP Control Plane (mielin-cli)
- New `mielin-cli/src/control/` module: `ControlServer` (axum 0.8), `ControlClient` (reqwest), and serde DTOs
- Endpoints: `GET /api/v1/health`, `GET /api/v1/mesh/status`, `GET /api/v1/mesh/peers`, `GET /api/v1/mesh/nodes`, `GET /api/v1/agents`, `POST /api/v1/agents`, `DELETE /api/v1/agents/:id`, `GET /api/v1/agents/:id`, `POST /api/v1/migrate`, `GET /api/v1/migrate/status`
- `daemon` subcommand gains `--control-listen <addr>` (default `127.0.0.1:8081`); starts `ControlServer` alongside `MeshService`
- `mielinctl mesh status --daemon <addr>` reads live `MeshService` data; graceful mock fallback when daemon is unreachable
- Workspace dependencies: `axum 0.8`, `tower 0.5`, `tower-http 0.6`

#### WASI Preview 2 / Component Model Foundation (mielin-wasm)
- New `preview2` Cargo feature (default off) enabling `wasmtime-wasi` Component Model path
- `ComponentExecutor` in `src/preview2.rs` with `P2HostState` implementing `WasiView` + `IoView`; uses `wasmtime_wasi::p2::add_to_linker_async`
- Minimal WIT world at `wit/mielin.wit` importing `wasi:clocks/wall-clock`, `wasi:random/random`, `wasi:cli/environment`
- 3 integration tests gated on `--features preview2`: compile, clock, and random
- `wasmtime-wasi = "43.0.1"` added as optional workspace dependency

### Changed

#### Toolchain
- Bumped `rust-toolchain.toml` channel from `1.90.0` to `1.91.0` (required by `wasmtime 43.0.1` MSRV)

#### rand 0.10 API Migration
- All `use rand::Rng` imports updated to `use rand::RngExt` following rand 0.10 trait reorganisation
- `fill_bytes` calls updated to `fill` across mesh, cells, and cli crates

#### wasmtime 43 Compatibility
- Replaced `.context("msg")` with `.map_err(|e| anyhow::anyhow!("…: {e:#}"))` where `wasmtime::Error` is the error type (no longer implements `std::error::Error` in v43)
- Removed deprecated `Config::async_support(false)` call (no-op in wasmtime 43)

#### Clippy / Zero-Warnings
- Replaced manual `impl Default` blocks with `#[derive(Default)]` + `#[default]` on variants across `mielin-mesh-wire` and `mielin-rt`
- Removed redundant `let i = i;` rebind in integration_tests.rs
- Gated kernel `cli` privileged instruction behind `#[cfg(not(test))]` to fix SIGSEGV in userspace test runs

### Test Results
- **3617 tests run: 3617 passed** (up from 3255 at rc.1, +362 net new tests)
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0

### Planned
- Docker Compose setup for local cluster testing
- Real hardware testing (RasPi 4 + Graviton3)
- Network partition simulation testing
- NAT traversal (STUN/TURN)

---

## [0.1.0] - 2026-03-30 - "Ranvier" (Initial Release)

**First stable release** of MielinOS - Core mesh networking and agent migration.

### Changes from rc.1
- Version promoted from release candidate to stable
- All known issues from 0.1.0-rc.1 resolved

### Summary
- **155,178 lines of Rust** across 445 files
- **3,255 tests passing** with zero clippy warnings
- Complete QUIC transport with TLS 1.3 encryption
- P2P mesh networking with mDNS discovery and gossip protocol
- Live agent migration with delta compression
- Production features: HA, DR, multi-region, compliance

See [0.1.0-rc.1] release notes below for full feature list.

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

- **v0.1 "Ranvier"**: Nodes of Ranvier (gaps in myelin sheath where saltatory conduction occurs)
- **v0.2 "Oligodendrocyte"**: Cells that produce myelin in the central nervous system
- **v0.3 "Schwann"**: Cells that produce myelin in the peripheral nervous system
- **v1.0 "Saltatory"**: Saltatory conduction (the fast jumping of signals)

## Links

- [Repository](https://github.com/cool-japan/mielin)
- [Issues](https://github.com/cool-japan/mielin/issues)
- [Discussions](https://github.com/cool-japan/mielin/discussions)
- [Documentation](https://github.com/cool-japan/mielin/blob/main/README.md)

[Unreleased]: https://github.com/cool-japan/mielin/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/cool-japan/mielin/compare/v0.1.0-rc.1...v0.1.0
[0.1.0-rc.1]: https://github.com/cool-japan/mielin/releases/tag/v0.1.0-rc.1
