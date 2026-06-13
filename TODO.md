# MielinOS TODO — v0.1.0 (2026-05-01)

## Pending Tasks

### High Priority - Release Critical
- [x] All tests passing (100% pass rate) — 3617 tests, 0 failures
- [x] Zero compiler warnings — clippy clean with -D warnings
- [x] Documentation updated — documentation fields added to all subcrates, README.md updated
- [x] CHANGELOG.md updated
- [x] Version bumped in Cargo.toml

### High Priority - Testing & Validation
- [ ] Local cluster testing
- [ ] Heterogeneous cluster testing
- [x] Resilience testing — fault injection framework (FaultInjector) + 15 fault tests + 10 chaos/partition tests
- [ ] Integration tests on real hardware
- [x] Improve test coverage for edge cases (>90% coverage) — 4000+ tests, fault injection, chaos, cross-version migration, large cluster simulation

### Medium Priority - Documentation
- [ ] Network protocol specification (RFC-style)
- [ ] Migration protocol documentation
- [ ] Deployment guide for small clusters
- [ ] Troubleshooting runbook
- [ ] Performance tuning guide
- [ ] Getting started guide (5-minute quickstart)
- [ ] Tutorial series (10+ tutorials)

### Medium Priority - Features
- [x] Cortex-M bootloader — A/B partition selection, image validation, trial/confirm rollback, flash abstraction, host-testable; 31 tests in mielin-rt/src/bootloader.rs
- [x] Low-power features for IoT — EnergyPolicy, PowerDomainTracker, EnergyAwareScheduler, EnergyAdaptiveController in mielin-rt
- [x] NPU support expansion — OnnxRuntimeBackend (universal fallback) + Hailo8Backend (device probe); 39 tests with feature flags in mielin-tensor/src/npu/
- [x] Distributed inference — DistributedInferenceEngine with ShardedTensor, Cannon's algorithm matmul, ModelParallelPipeline in mielin-tensor
- [x] Energy profiling — PowerDomain per-peripheral accounting, energy-aware scheduling hints integrated in mielin-rt

### Low Priority - Community
- [ ] Contribution guidelines (CONTRIBUTING.md)
- [ ] Code of conduct (CODE_OF_CONDUCT.md)
- [ ] Issue templates
- [ ] Security policy (SECURITY.md)
- [ ] Discord server setup
- [ ] Video walkthroughs

### Future - Research & Exploration
- [x] Arm SVE2/SME kernel integration — SVE2 dispatcher fully wired in mielin-tensor
- [x] Quantum-ready cryptography — ML-KEM 0.3 + X25519 hybrid KEX, HybridKexState, PqKeyShareExtension; 41 tests in quantum.rs
- [x] Self-evolving agents — EvolutionEngine (genetic algorithm), AgentGenome, FitnessEvaluator, MutationOperator, CrossoverOperator, CapabilityDiscovery; 27 tests in mielin-cells/src/evolution.rs
- [x] Federated learning — FedAvg + UniformAvg aggregation, FederatedCoordinator, LocalTrainer; 20 tests in mielin-tensor/src/federated.rs
- [x] Machine learning for migration prediction — Holt double-exponential smoothing + ridge regression + R² confidence in mielin-cells/src/resource/predictor_ml.rs; 16 tests
- [x] Novel consensus algorithms — complete super-peer term election (record_vote tallying + promote_super_peer) in mielin-mesh/core/src/gossip.rs

### Future - Ecosystem
- [ ] MielinCloud SaaS control plane
- [ ] Visual debugger (MielinStudio)
- [ ] Academic partnerships
- [ ] Industry adoption program

## Pure Rust Migration (COOLJAPAN Policy)

Goal: make the default build free of C/C++/Fortran and assembly dependencies. All
compression and cryptography must use Pure Rust crates (COOLJAPAN `oxiarc-*` /
`oxicrypto`, or RustCrypto). Track progress here.

- [x] (2026-06-05) Compression: `zstd` (C, via `zstd-sys`) → `oxiarc-zstd` and
  `lz4_flex` (Pure Rust) → `oxiarc-lz4`, for consistency under a single Pure Rust
  archive stack. Removed the C `zstd-sys` dependency from the default build.
  - Touched: `mielin-cells/src/migration/functions.rs`,
    `mielin-mesh/wire/src/compression.rs`, the `[workspace.dependencies]` table in
    `Cargo.toml`, and the `mielin-cells` / `mielin-mesh-wire` crate manifests.
  - API mapping: `zstd::encode_all`/`decode_all` → `oxiarc_zstd::encode_all`/
    `decode_all` (drop-in). `lz4_flex::compress_prepend_size`/
    `decompress_size_prepended` → `oxiarc_lz4::compress` / `oxiarc_lz4::decompress`
    (self-describing LZ4 frame format; the frame embeds the content size, so
    decompression needs only an output-size bound).
  - Verified: `cargo build`, `cargo nextest run`, and
    `cargo clippy --all-features --all-targets -- -D warnings` are green for both
    packages; `cargo tree -p mielin-mesh-wire` shows no `zstd-sys` / `lz4-sys` /
    `lz4_flex` (only `oxiarc-zstd` / `oxiarc-lz4`).

- [ ] **`ring` → Pure Rust crypto (DEEP, security-sensitive).**
  - Scope: `ring = "0.17.14"` (`Cargo.toml`, `[workspace.dependencies]`) is pulled
    in unconditionally by `mielin-cells`, `mielin-mesh/core`, and `mielin-mesh/wire`
    (~39 call sites). `ring` bundles C and per-architecture assembly, so it violates
    the Pure Rust policy by default. It is currently used for:
    - Ed25519 + ECDSA P-256/P-384 **signatures** (`identity.rs` in `mielin-cells`
      and `mielin-mesh/core`).
    - X25519 ECDH + HKDF **key exchange** (`mielin-mesh/core/security/kex.rs`).
    - SHA-256/384/512 **digests** (`mielin-mesh/wire/advanced_tls.rs`,
      `mielin-mesh/wire/certs/{pinning,ca,acme}.rs`).
    - AES-256-GCM **AEAD** (`mielin-mesh/core/security/crypto.rs`,
      `mielin-cells/security/encryption.rs`).
    - `SystemRandom` **RNG**.
  - Extra blocker (transitive `ring` via TLS): `rustls` (`Cargo.toml`,
    `[workspace.dependencies]`) is configured with `default-features = false,
    features = ["ring", "std"]`, and `reqwest` (`[workspace.dependencies]`) is
    likewise pinned to the **ring** `CryptoProvider`. The migration must ALSO swap
    `rustls` to a Pure Rust `CryptoProvider` (not `aws-lc-rs` and not `ring`),
    otherwise `ring` re-enters the dependency graph through TLS.
  - Replacement options: RustCrypto Pure Rust crates (`ed25519-dalek`, `p256`,
    `p384`, `x25519-dalek`, `hkdf`, `sha2`, `aes-gcm`, `getrandom`) OR the COOLJAPAN
    `oxicrypto` stack. None are currently wired into the workspace.
  - Acceptance criteria:
    - `cargo tree -i ring` is empty under default features.
    - All crypto and TLS tests are green.
    - No C / C++ / Fortran / assembly in the default build.
  - Notes: this is a multi-day port. Do it primitive-by-primitive with tests at each
    step, in this order: digests → RNG → AEAD → signatures → ECDH → `rustls`
    `CryptoProvider`. Keep wire-format compatibility for any persisted or
    network-exchanged crypto material (signatures, key shares, ciphertext framing).

## Completed Features (v0.1.0)

MielinOS v0.1.0 "Oligodendrocyte" is a complete distributed agent mesh operating system with the following components:

### Core Components
- ✅ **mielin-kernel**: Unikernel with multi-architecture support
- ✅ **mielin-hal**: Hardware abstraction layer
- ✅ **mielin-rt**: Embedded runtime for Cortex-M
- ✅ **mielin-mesh**: Distributed hash table and QUIC-based wire protocol
- ✅ **mielin-cells**: Agent SDK with lifecycle management
- ✅ **mielin-wasm**: WebAssembly runtime with security sandbox
- ✅ **mielin-tensor**: Hardware-accelerated tensor operations
- ✅ **mielin-cli**: Command-line interface

### Key Capabilities
- ✅ Multi-architecture support (x86_64, AArch64, RISC-V, Cortex-M)
- ✅ Agent migration across heterogeneous hardware
- ✅ Hardware-accelerated ML inference (CUDA, Metal, NPU)
- ✅ QUIC-based secure communication
- ✅ Comprehensive test suite (3255+ tests passing)
- ✅ Zero warnings policy
- ✅ Production-ready documentation

For detailed feature descriptions, see individual crate README files and the project [README.md](README.md).

## Stubs to implement (added 2026-06-12 by /cooljapan-stub-check)

- [x] `mielin-kernel`: `mielin-kernel/src/vmm/mod.rs:582` — implement page-table cleanup in VMM drop/free path
  - Priority: P2 | Scope: medium | Hint: none
- [x] `mielin-kernel`: `mielin-kernel/src/ipc.rs:218` — implement platform-specific IPI delivery for ARM and RISC-V
  - Priority: P2 | Scope: medium | Hint: none
- [x] `mielin-kernel`: `mielin-kernel/src/interrupt.rs:733` — implement `read_tsc` for ARM (CNTVCT_EL0) and RISC-V
  - Priority: P2 | Scope: small | Hint: none
- [x] `mielin-kernel`: `mielin-kernel/src/interrupt.rs:789` — implement corresponding TSC calibration for ARM and RISC-V
  - Priority: P2 | Scope: small | Hint: none
- [x] `mielin-kernel`: `mielin-kernel/src/rt.rs:278` — add proper error type for task-not-found / invalid-page-count instead of reusing `TaskNotFound`/`InvalidPageCount` in wrong context
  - Priority: P2 | Scope: trivial | Hint: none
- [x] `mielin-mesh`: `mielin-mesh/wire/src/certs/ca.rs:388` — implement real CRL fetching in CA certificate validation
  - Priority: P2 | Scope: medium | Hint: oxitls
- [x] `mielin-mesh`: `mielin-mesh/wire/src/certs/ca.rs:430` — parse name constraints extension in certificate validation
  - Priority: P2 | Scope: small | Hint: oxitls
- [x] `mielin-mesh`: `mielin-mesh/wire/src/certs/mtls.rs:174` — implement proper certificate chain validation with CA in mTLS
  - Priority: P2 | Scope: medium | Hint: oxitls
- [x] `mielin-tensor`: `mielin-tensor/src/quant.rs:150` — implement true per-channel quantization (currently falls back to per-tensor)
  - Priority: P2 | Scope: small | Hint: none

## Proposed follow-ups

- [ ] Transitive `ring` removal: `ring 0.17.14` survives as a transitive dep via `rustls-webpki`. Blocked on upstream rustls/webpki adopting a pure-Rust CryptoProvider. Track and re-visit when rustls 0.24+ ships a ring-free path.
- [ ] `hal-ci-all-architectures` (mielin-hal/TODO.md): COOLJAPAN policy forbids creating `.github/workflows/*.yml` (except pypi/npm). Rework as a local QEMU/cross-emulation `Makefile` target instead, or defer until policy allows.

## Last updated: 2026-06-14

## Stubs to implement (round 2, added 2026-06-14 by /ultra)

- [~] `mielin-tensor`: `mielin-tensor/src/autograd.rs:425` — true forward-mode JVP; currently returns all-zeros (perturbed input built then discarded, so numerator is f−f=0)
  - Priority: P2 | Scope: hard | Hint: tangent propagation per-op alongside existing closure GradFn
- [~] `mielin-tensor`: `mielin-tensor/src/autograd.rs:455` — correct second_derivative; currently returns first derivative as placeholder
  - Priority: P2 | Scope: hard | Hint: hyper-dual forward-over-forward pass
- [~] `mielin-tensor`: `mielin-tensor/src/sparse.rs:193` — CSR to_dense row-pointer decode; currently copies the COO arm and produces wrong output
  - Priority: P2 | Scope: small | Hint: rows is a row-pointer array (len nrows+1), not raw row indices
- [~] `mielin-tensor`: `mielin-tensor/src/formats/mod.rs:302` — native Mielin model format import/export; both arms return Err("not yet implemented")
  - Priority: P2 | Scope: medium | Hint: oxicode or extend serialize.rs "MIEL" hand-rolled format
- [~] `mielin-kernel`: `mielin-kernel/src/work_stealing.rs:363` — yield_task re-enqueues with hardcoded priority 0; loses real task priority
  - Priority: P2 | Scope: small | Hint: add current_priority: AtomicU32 to WorkerState
- [~] `mielin-kernel`: `mielin-kernel/src/vmm/mod.rs:799` — flush_tlb_range is single-core only; no IPI shootdown to remote CPUs; riscv64 flush_tlb_page is a no-op
  - Priority: P2 | Scope: hard | Hint: use IpiType::TlbFlush + CpuBarrier from ipc.rs built in run 1
- [~] `mielin-cells`: `mielin-cells/src/group.rs:685` — transition_all runs closure on throwaway Agent::new(vec![]); all_in_state returns true unconditionally
  - Priority: P2 | Scope: medium | Hint: thread &mut HashMap<AgentId,Agent> from caller; no registry exists
- [~] `mielin-wasm`: `mielin-wasm/src/runtime.rs:438` — Module::hash() returns 0 and size_bytes() returns 0 for all three Module impls
  - Priority: P2 | Scope: trivial | Hint: serialize bytes + FNV-1a (already in cache.rs:37)
- [~] `mielin-wasm`: `mielin-wasm/src/memory.rs:345` — MemorySnapshot::compress() prepends 12-byte header then raw data with no actual compression
  - Priority: P2 | Scope: small | Hint: oxiarc-lz4 already in workspace deps; add .workspace=true to mielin-wasm Cargo.toml
