# MielinOS TODO — v0.1.0 (2026-06-23)

## Pending Tasks

### High Priority - Release Critical
- [x] All tests passing (100% pass rate) — 4,578 tests, 0 failures (re-verified 2026-07-11)
- [x] Zero compiler warnings — clippy clean with -D warnings. NOTE (2026-07-11): this claim had regressed. The default build emitted 17 compiler warnings (mielin-hal ×16 `unused_unsafe`, mielin-kernel ×1 `unreachable_code`) and `cargo clippy --workspace --all-targets -D warnings` failed with 6 lints (mielin-tensor, mielin-cells, mielin-rt ×2, mielin-wasm ×2, + 2 pre-existing in mielin-kernel/interrupt.rs). ALL fixed at root cause this run (no `#[allow]` suppression); the kernel `unreachable_code` fix was also a real correctness fix (guards the privileged `cli`/`sti` path out of std/test builds). Build + full-workspace clippy are now genuinely clean.
- [x] Documentation updated — documentation fields added to all subcrates, README.md updated
- [x] CHANGELOG.md updated
- [x] Version bumped in Cargo.toml

### High Priority - Testing & Validation
- [ ] Local cluster testing — DEFERRED (requires multi-node hardware/network; not runnable in CI sandbox). NOTE: the shipped docker-compose.yml does not actually mesh containers — see "Advertised-vs-actual gaps" below.
- [ ] Heterogeneous cluster testing — DEFERRED (requires heterogeneous real hardware)
- [x] Resilience testing — fault injection framework (FaultInjector) + 15 fault tests + 10 chaos/partition tests
- [ ] Integration tests on real hardware — DEFERRED (requires physical devices)
- [x] Improve test coverage for edge cases (>90% coverage) — 4,565 tests, fault injection, chaos, cross-version migration, large cluster simulation

### Medium Priority - Documentation
- [x] Network protocol specification (RFC-style) — docs/PROTOCOL.md (RFC-style wire-protocol spec, grounded in mielin-mesh/wire/src) (2026-07-11)
- [x] Migration protocol documentation — docs/MIGRATION.md (agent-migration pipeline, grounded in mielin-cells/src/migration + wire) (2026-07-11)
- [x] Deployment guide for small clusters — docs/DEPLOYMENT.md (grounded in mielin-cli commands, docker-compose, discovery) (2026-07-11)
- [x] Troubleshooting runbook — docs/TROUBLESHOOTING.md (symptom→cause→fix, grounded in real error enums) (2026-07-11)
- [x] Performance tuning guide — docs/PERFORMANCE_TUNING.md (real config knobs; companion to docs/PERFORMANCE.md) (2026-07-11)
- [x] Getting started guide (5-minute quickstart) — QUICKSTART.md (refreshed 2026-07-11: real API + honest live-vs-mock CLI status)
- [x] Tutorial series (10+ tutorials) — docs/TUTORIALS.md (12 tutorials; every snippet cargo-check verified) (2026-07-11)

### Medium Priority - Features
- [x] Cortex-M bootloader — A/B partition selection, image validation, trial/confirm rollback, flash abstraction, host-testable; 31 tests in mielin-rt/src/bootloader.rs
- [x] Low-power features for IoT — EnergyPolicy, PowerDomainTracker, EnergyAwareScheduler, EnergyAdaptiveController in mielin-rt
- [x] NPU support expansion — OnnxRuntimeBackend (universal fallback) + Hailo8Backend (device probe); 39 tests with feature flags in mielin-tensor/src/npu/
- [x] Distributed inference — DistributedInferenceEngine with ShardedTensor, Cannon's algorithm matmul, ModelParallelPipeline in mielin-tensor
- [x] Energy profiling — PowerDomain per-peripheral accounting, energy-aware scheduling hints integrated in mielin-rt

### Low Priority - Community
- [x] Contribution guidelines (CONTRIBUTING.md) — present at repo root
- [x] Code of conduct (CODE_OF_CONDUCT.md) — added 2026-07-11 (Contributor Covenant v2.1, GitHub-based enforcement contact)
- [x] Issue templates — present at .github/ISSUE_TEMPLATE/{bug_report,feature_request}.md + pull_request_template.md
- [x] Security policy (SECURITY.md) — added 2026-07-11 (GitHub private advisories; accurate, non-overstated security-model summary)
- [ ] Discord server setup — DEFERRED (external service; not actionable in-repo)
- [ ] Video walkthroughs — DEFERRED (media production; not actionable in-repo)

### Future - Research & Exploration
- [x] Arm SVE2/SME kernel integration — SVE2 dispatcher fully wired in mielin-tensor
- [x] Quantum-ready cryptography — ML-KEM 0.3 + X25519 hybrid KEX, HybridKexState, PqKeyShareExtension; 41 tests in quantum.rs
- [x] Self-evolving agents — EvolutionEngine (genetic algorithm), AgentGenome, FitnessEvaluator, MutationOperator, CrossoverOperator, CapabilityDiscovery; 27 tests in mielin-cells/src/evolution.rs
- [x] Federated learning — FedAvg + UniformAvg aggregation, FederatedCoordinator, LocalTrainer; 20 tests in mielin-tensor/src/federated.rs
- [x] Machine learning for migration prediction — Holt double-exponential smoothing + ridge regression + R² confidence in mielin-cells/src/resource/predictor_ml.rs; 16 tests
- [x] Novel consensus algorithms — complete super-peer term election (record_vote tallying + promote_super_peer) in mielin-mesh/core/src/gossip.rs

### Future - Ecosystem
- [ ] MielinCloud SaaS control plane — DEFERRED (future product; out of repo scope)
- [ ] Visual debugger (MielinStudio) — DEFERRED (future product; out of repo scope)
- [ ] Academic partnerships — DEFERRED (non-engineering/business)
- [ ] Industry adoption program — DEFERRED (non-engineering/business)

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

- [x] **`ring` → Pure Rust crypto (DEEP, security-sensitive).**
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
  - Migration complete (2026-06-20): all ~39 direct call sites ported to oxicrypto-*/oxiquic-crypto. A non-compiled ring lock entry remains via rustls-webpki (feature-gated optional dep); removal awaits upstream rustls/webpki ring-free path (see line 158 above).

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
- ✅ Comprehensive test suite (4,565 tests passing)
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

## Last updated: 2026-07-11 (previously 2026-06-23)

## Stubs to implement (round 2, added 2026-06-14 by /ultra)

- [x] `mielin-tensor`: `mielin-tensor/src/autograd.rs:425` — true forward-mode JVP; currently returns all-zeros (perturbed input built then discarded, so numerator is f−f=0)
  - Priority: P2 | Scope: hard | Hint: tangent propagation per-op alongside existing closure GradFn
- [x] `mielin-tensor`: `mielin-tensor/src/autograd.rs:455` — correct second_derivative; currently returns first derivative as placeholder
  - Priority: P2 | Scope: hard | Hint: hyper-dual forward-over-forward pass
- [x] `mielin-tensor`: `mielin-tensor/src/sparse.rs:193` — CSR to_dense row-pointer decode; currently copies the COO arm and produces wrong output
  - Priority: P2 | Scope: small | Hint: rows is a row-pointer array (len nrows+1), not raw row indices
- [x] `mielin-tensor`: `mielin-tensor/src/formats/mod.rs:302` — native Mielin model format import/export; both arms return Err("not yet implemented")
  - Priority: P2 | Scope: medium | Hint: oxicode or extend serialize.rs "MIEL" hand-rolled format
- [x] `mielin-kernel`: `mielin-kernel/src/work_stealing.rs:363` — yield_task re-enqueues with hardcoded priority 0; loses real task priority
  - Priority: P2 | Scope: small | Hint: add current_priority: AtomicU32 to WorkerState
- [x] `mielin-kernel`: `mielin-kernel/src/vmm/mod.rs:799` — flush_tlb_range is single-core only; no IPI shootdown to remote CPUs; riscv64 flush_tlb_page is a no-op
  - Priority: P2 | Scope: hard | Hint: use IpiType::TlbFlush + CpuBarrier from ipc.rs built in run 1
- [x] `mielin-cells`: `mielin-cells/src/group.rs:685` — transition_all runs closure on throwaway Agent::new(vec![]); all_in_state returns true unconditionally
  - Priority: P2 | Scope: medium | Hint: thread &mut HashMap<AgentId,Agent> from caller; no registry exists
- [x] `mielin-wasm`: `mielin-wasm/src/runtime.rs:438` — Module::hash() returns 0 and size_bytes() returns 0 for all three Module impls
  - Priority: P2 | Scope: trivial | Hint: serialize bytes + FNV-1a (already in cache.rs:37)
- [x] `mielin-wasm`: `mielin-wasm/src/memory.rs:345` — MemorySnapshot::compress() prepends 12-byte header then raw data with no actual compression
  - Priority: P2 | Scope: small | Hint: oxiarc-lz4 already in workspace deps; add .workspace=true to mielin-wasm Cargo.toml

## Stubs to implement (added 2026-06-22 by /cooljapan-stub-check)

- [x] **mielin** `mielin-cli`: `mielin-cli/src/script.rs` — replaced the `// TODO: Add your script logic here` placeholder in the emitted Rhai scaffold with a small runnable worked example (2026-07-11). Bonus: the original scaffold used bare `{ }` object literals, which are **invalid Rhai** (`#{ }` is required) — so generated scripts had never actually executed; fixed to `#{ }` and verified by running the rendered template through a real `rhai::Engine`. 159/159 mielin-cli tests pass; `rg TODO|FIXME` over src is now empty.

## Advertised-vs-actual gaps discovered (2026-07-11, by /ucont doc-grounding pass)

Writing the documentation suite required grounding every claim in the real source. That pass
uncovered **silent stubs / simulations** — code that compiles and returns plausible values but
does not do what the README / ARCHITECTURE / per-crate TODOs advertise. These carry **no loud
`todo!()` markers**, so they never tripped stub-check, yet they are real remaining work. The new
docs document the REAL behavior (with "Implementation note" call-outs) rather than the advertised
behavior. The original catalog is grouped by subsystem below.

### Status after the 2026-07-11 strict-check implementation pass

A follow-up strict-check pass (10 gaps, each: implement → adversarial verify) then **RESOLVED**
the tractable gaps. Full workspace after the pass: **build 0 warnings, `clippy --workspace
--all-targets -D warnings` clean, 4605 tests pass (0 fail)** — +27 new tests. Per-gap outcome:

**FIXED (real logic implemented):**
- `Dna::hash()` — now real SHA-256 via `oxicrypto-hash` (was `hash[0]=len`). ARCHITECTURE.md corrected back to SHA-256.
- `loadbalancer.rs` — `mark_healthy/unhealthy` now truly mutate `EndpointStats.health`; `random_select` now uses `oxicrypto-rand` (no direct `rand`).
- `gossip.rs` — `should_suspect()/should_declare_dead()` now take the runtime `GossipConfig` durations; config actually governs failure detection.
- `partition.rs` — `QuorumRegained` now emitted on quorum restore (with a regression test).
- `websocket.rs` — `connect()` now parses the real target URL (was a hardcoded `TcpStream::connect("localhost:8080")` bug).
- `protocol.rs` — `HelloMessage` nonce now CSPRNG (`oxicrypto-rand`); `build_hello` returns `Result` (honest RNG-failure).
- `certs/ca.rs` — `remove_ca_cert` now rebuilds `trust_anchors` (a removed CA can no longer validate).
- `flow.rs` — **real Cubic and BBR** congestion control implemented (were silent AIMD aliases).

**MADE HONEST (feature still not fully built, but the code no longer fabricates success):**
- `certs/ca.rs` OCSP — returns `RevocationStatus::Unknown` with an explicit "not implemented" reason; no silent "valid" pass. (Real OCSP client still TODO.)
- `migration/types/core.rs` `wasm_state` — `restore()` now errors if asked to drop non-empty runtime memory; loudly documents that mielin-cells embeds no WASM runtime. (Real cross-node WASM-memory transfer still TODO.)
- `migration/types/validation.rs` `agent_responsive` — now computed from a real `AgentState::is_active()` check; defaults to `false`/unknown, never fabricated `true`.
- `mielin-cli` `agent deploy/migrate/stop/logs/exec` — now return an honest "requires a live daemon; not yet supported over the control plane" error instead of a fake success; `list/inspect` labeled `[sample/mock]`. (Real control-plane POST endpoints still TODO.)

**STILL DEFERRED — genuine dedicated feature work (NOT attempted; faking it would be a new fabrication):**
- `partition.rs` `SplitBrainDetected` — honestly left unwired (single-view detector can't determine multiple partitions without a new mechanism).
- mesh-core `routing.rs` empty `RoutingTable` / flat-HashMap DHT (real Kademlia k-buckets); gossip **network** dissemination (currently logging-only; anti-entropy via SyncRequest/Response does work); `discovery.rs`/DNS real peer exchange.
- Real cross-node agent migration byte transfer (`MigrationCoordinator` is still `sleep()`-scaffolded) + real CLI control-plane POST endpoints + docker-compose container meshing (127.0.0.1 hardcode).
- Cosmetic/smaller: duplicate type names (`ProtocolVersion`×2, `Capability`×2), unused `wire_formats.rs WireSerializer`, `ProtocolHandler::handle_message` placeholder, missing `MigrationManager::cancel_migration`, `cert_rotation` hot-swap, `mtls` pin no-op / `CertChainVerifier` unenforced fields, transitive `ring` (upstream).

### Original gap catalog (as discovered by the doc-grounding pass), grouped by subsystem:

### CLI (mielin-cli) — headline deliverable is largely MOCK
- `mielinctl agent deploy/create/migrate/stop/list/inspect` make **no network call** — they mint a
  UUID and print a mock `OperationResult` (`commands/agent.rs` never imports `ControlClient`; the
  axum control server registers only GET routes — no POST deploy/migrate endpoint exists).
- Most `node/cluster/registry/gossip/migrate *` subcommands render hard-coded or `rand`-generated
  data, not live daemon state. Only `mesh status/peers --daemon <addr>` and `node config` truly
  talk to the daemon — yet mielin-cli/TODO.md marks "actual node connection" / "actual mesh status
  fetching" as DONE `[x]`.
- `mielinctl daemon --bootstrap` is a no-op (`DiscoveryService::connect_bootstrap()` is a placeholder
  never called from startup). Node identity is a fresh random UUID each restart (no persistence).
- Shipped `docker-compose.yml` cannot actually mesh containers: `connect_to_peer` uses
  `str::parse::<SocketAddr>()` (no DNS for Docker service names) and `MeshNode::new` hard-binds QUIC
  to `127.0.0.1` regardless of `--port`.

### Agent migration (mielin-cells) — headline feature is a simulation
- `MigrationSnapshot::capture` never populates `wasm_state`; `restore()` never loads it → only
  code + policy survive a migration, **not runtime agent memory**.
- Both mesh `MigrationCoordinator`s (wire + core) are phase/telemetry scaffolds using
  `tokio::time::sleep`; no live byte transfer is wired in. The agent-migration example's "network
  transfer" phase is in-process serialize/deserialize (no socket).
- `Dna::hash()` is not SHA-256 (ARCHITECTURE.md claimed it was) — it sets `hash[0]=len`, rest zero.
- `VerificationResult.agent_responsive` is hard-coded `true`; `RecoveryConfig` has two conflicting
  `impl Default` (one orphaned/dead); `MigrationManager` has no `cancel_migration` (README calls one).

### Mesh core (mielin-mesh-core)
- `routing.rs` is an empty stub `struct RoutingTable {}`; `Dht.routing_table` is a flat HashMap
  (not a k-bucket tree). README claimed 160-bit node IDs — actual `NodeId = Uuid` (128-bit).
- Gossip fanout dissemination is logging-only (real anti-entropy only via SyncRequest/Response);
  `MemberInfo::should_suspect()/should_declare_dead()` read hard-coded constants, ignoring the
  runtime `GossipConfig`. `discovery.rs`/DNS refresh return canned/empty results.
- `PartitionEvent::SplitBrainDetected`/`QuorumRegained` are never emitted; `loadbalancer.rs`
  `mark_healthy/unhealthy` never mutate health, and its `random_select` uses `rand` directly
  (SciRS2-Core policy divergence).

### Mesh wire (mielin-mesh-wire)
- `CongestionAlgorithm::Cubic`/`Bbr` silently fall back to AIMD; `WebSocketTransport::connect()`
  ignores its `url` host/port; `ProtocolHandler::handle_message()` is a placeholder.
- Two distinct types each named `ProtocolVersion` and two named `Capability`; `HelloMessage.nonce`
  is a counter despite a "random" doc; the `wire_formats.rs` `WireSerializer` (1-byte format tag)
  is defined but never invoked by any transport.
- Certs: OCSP only extracts+logs the responder URL (returns Unknown, no real request);
  `remove_ca_cert` doesn't rebuild trust anchors; `cert_rotation.rs::subscribe_to_renewal` doesn't
  hot-swap `ServerConfig`; `mtls.rs` post-chain pin check is a no-op; `CertChainVerifier`
  `require_san`/`allowed_key_algs`/`min_key_bits` are unenforced.

### Minor / hygiene
- Transitive `ring` still enters via `rustls-webpki` (tracked above; upstream-blocked).
- External `proc-macro-error2 v2.0.1` emits a future-incompat note (transitive; upstream-owned).
