# MielinOS Agent Migration Protocol

This document describes how MielinOS moves a live agent from one node to another —
"Saltatory Conduction" in the project's neural metaphor. It is grounded entirely in
the current implementation; where the code is a scaffold rather than a finished
data-plane (e.g. the mesh-level pre-copy coordinators), that is called out
explicitly rather than described as if it were complete.

**Primary sources for this document:**

- `mielin-cells/src/migration/mod.rs`, `functions.rs`, `types/*.rs` — the migration
  data pipeline (snapshot, serialize, compress, checksum, verify, recover)
- `mielin-cells/src/agent.rs` — `AgentState` lifecycle and transition validation
- `mielin-cells/src/versioning.rs` — `Version`, `VersionRegistry`, rolling
  update/canary/A-B deployment
- `mielin-mesh/wire/src/lib.rs` — the `Message::AgentMigration` / `MigrationAck`
  wire frames actually used by the runnable example
- `mielin-mesh/wire/src/migration.rs` — the pre-copy `MigrationCoordinator` protocol
  design (`MigrationMessage`, `MigrationPhase`, `MigrationState`)
- `mielin-mesh/core/src/migration.rs` — the mesh-level `MigrationCoordinator` with
  `PreCopy`/`PostCopy`/`Hybrid` strategies and `MigrationMetrics` telemetry
- `mielin-mesh/core/src/metrics/types.rs` — `MigrationSuccessMetrics` /
  `MigrationSuccessSummary`
- `examples/agent-migration/src/main.rs` — the 10-phase demo referenced by
  `QUICKSTART.md` and `docs/ARCHITECTURE.md`
- `examples/e2e-agent-migration/src/main.rs`, `mielin-cells/examples/migration.rs`,
  `mielin-cells/examples/policy_migration.rs`
- `mielin-cells/tests/cross_version_migration_tests.rs` — multi-hop and
  deprecation-rejection test coverage

---

## Table of Contents

1. [Overview and Goals](#1-overview-and-goals)
2. [Agent State Model and Migration Lifecycle](#2-agent-state-model-and-migration-lifecycle)
3. [The Migration Pipeline End-to-End](#3-the-migration-pipeline-end-to-end)
4. [Wire Framing for Migration](#4-wire-framing-for-migration)
5. [Compression and Integrity](#5-compression-and-integrity)
6. [Version Compatibility and Cross-Version Migration](#6-version-compatibility-and-cross-version-migration)
7. [Failure Handling and Recovery](#7-failure-handling-and-recovery)
8. [Telemetry and Observability](#8-telemetry-and-observability)
9. [Worked Example: `examples/agent-migration`](#9-worked-example-examplesagent-migration)
10. [Known Gaps and Honest Caveats](#10-known-gaps-and-honest-caveats)

---

## 1. Overview and Goals

MielinOS agents (`mielin_cells::Agent`) are meant to move between heterogeneous
nodes — Edge, Relay, Core (`mielin_mesh_wire::NodeRole`) — with the state transfer
itself contributing negligible downtime relative to normal execution. The
migration subsystem is split across three crates, each responsible for a
different concern:

| Crate | Responsibility |
|---|---|
| `mielin-cells` (`src/migration/`) | Capturing, (de)serializing, compressing, checksumming and restoring agent state; version/compatibility gating; rollback bookkeeping |
| `mielin-mesh-wire` | The byte-level `Message` wire frame that carries a migration snapshot between nodes, plus a design for an iterative pre-copy protocol |
| `mielin-mesh-core` | Mesh-wide migration orchestration (`MigrationCoordinator`), strategy selection (`PreCopy`/`PostCopy`/`Hybrid`), and Prometheus-style success metrics |

Migration is *not* automatic background magic — it is driven by policy triggers
defined in `mielin_cells::policy` (`MigrationPolicy`, with `BatteryTriggerConfig`,
`LatencyTriggerConfig`, `CostTriggerConfig`, `LoadTriggerConfig`, evaluated by
`PolicyEvaluator`) and by version-rollout orchestration in
`mielin_cells::versioning` (`VersionDeployer::rolling_update`, canary, A/B). This
document focuses on the *mechanics* of moving a snapshot once a migration has
been decided, not on the trigger evaluation itself.

The stated goals, as reflected in the code:

- **Zero-downtime intent**: `mielin-mesh/wire/src/migration.rs` and
  `mielin-mesh/core/src/migration.rs` both model a pre-copy phase (iteratively
  transferring memory while the agent keeps running) followed by a short
  stop-and-copy window, exactly like classic VM live migration. See
  [§10](#10-known-gaps-and-honest-caveats) for how much of that data plane is
  actually wired up today versus still a phase/telemetry scaffold.
- **State fidelity**: every migrated snapshot carries a non-cryptographic
  checksum (`simple_checksum`) that is verified on the receiving side before the
  migration is considered successful (`StateVerifier::verify_state`).
- **Recoverability**: a failed migration can be retried, redirected to a
  different target, or rolled back to the pre-migration snapshot via
  `MigrationRecoveryManager` and `RollbackInfo`.
- **Version safety**: an agent cannot be migrated to a deprecated software
  version, and cross-major-version migration requires an explicit registered
  `migration_path` (`VersionRegistry::can_migrate`).

---

## 2. Agent State Model and Migration Lifecycle

`AgentState` (`mielin-cells/src/agent.rs`) has **seven** variants — note that this
is *not* the `Created/Running/Paused/Migrating/Terminated/Failed` set one might
guess from convention; the real terminal-error state is called `Error`, and there
is a separate involuntary `Suspended` state:

```rust
pub enum AgentState {
    Created,
    Running,
    Paused,
    Suspended,
    Migrating,
    Error,
    Terminated,
}
```

Transitions are validated by `AgentState::can_transition_to(&self, target: &AgentState) -> bool`,
enforced inside `Agent::transition_to`/`transition_to_with_reason` (which return a
`TransitionResult::{Success, InvalidTransition, Blocked}` rather than panicking on
an invalid move). The migration-relevant edges are:

| From | To | Trigger method |
|---|---|---|
| `Running` | `Migrating` | `Agent::begin_migration()` |
| `Paused` | `Migrating` | `Agent::begin_migration()` |
| `Suspended` | `Migrating` | `Agent::begin_migration()` |
| `Migrating` | `Running` | `Agent::complete_migration()` (success path) |
| `Migrating` | `Error` | direct `transition_to(AgentState::Error)` / `set_error()` (failure path) |
| `Migrating` | `Terminated` | `Agent::terminate()` |

`Created` cannot transition directly to `Migrating` — an agent must be started
first (`can_transition_to` only allows `Created → Running` or `Created →
Terminated`). `Terminated` is terminal (`AgentState::is_terminal()`).

```rust
let mut agent = Agent::new(wasm_binary);
agent.start();                // Created -> Running
agent.begin_migration();      // Running -> Migrating
// ... capture / transfer / restore ...
agent.complete_migration();   // Migrating -> Running (on the destination agent)
```

**Important:** `MigrationSnapshot::capture`/`restore` (§3) do **not** themselves
call `begin_migration()`/`complete_migration()` — they only read/construct
`Agent` values. Driving the state machine alongside the data pipeline is the
caller's responsibility; the 10-phase example in `examples/agent-migration`
(§9) does not call `begin_migration()` at all, so the demo agent stays in
`AgentState::Created` throughout.

`AgentError` (populated via `Agent::set_error`) carries `message`, an optional
`code`, `recovery_attempts`, and a `fatal` flag; `Agent::attempt_recovery()`
transitions `Error → Running` unless `fatal` is set, in which case it returns
`TransitionResult::Blocked`.

---

## 3. The Migration Pipeline End-to-End

The data pipeline lives in `mielin_cells::migration` and maps onto the
Snapshot → Serialize → Compress → Checksum → Transfer → Decompress → Verify →
Deserialize → Resume stages as follows.

### 3.1 Snapshot

```rust
pub struct MigrationSnapshot {
    pub agent_id: [u8; 16],
    pub wasm_binary: Vec<u8>,
    pub wasm_state: Vec<u8>,
    pub policy: Policy,
    pub timestamp: u64,
    pub source_node: Option<[u8; 16]>,
}

impl MigrationSnapshot {
    pub fn capture(agent: &Agent, source_node: Option<[u8; 16]>) -> Result<Self, CellError>;
}
```

`capture()` copies `agent.dna().binary()` into `wasm_binary` and clones
`agent.policy()`. **`wasm_state` is currently always initialized to an empty
`Vec`** — the snapshot type has a field for runtime WASM linear memory, but
`capture()` does not populate it from a live Wasmtime instance. `size_bytes()`
returns `wasm_binary.len() + wasm_state.len() + 64` (a fixed 64-byte overhead
estimate for the rest of the struct).

For *incremental* migration, `IncrementalMigrator`/`DirtyPageTracker`
(`migration/types/delta.rs`) capture only changed 4 KiB pages
(`PAGE_SIZE = 4096`) instead of a full snapshot:

```rust
let mut tracker = DirtyPageTracker::new(agent_id, state_size);
tracker.initialize(&initial_state);
tracker.mark_dirty(page_index);          // or mark_range_dirty(offset, len)
let delta: DeltaSnapshot = tracker.create_delta(&current_state)?;
```

`DeltaSnapshot::create` diffs the old and new state page-by-page and stores only
pages that differ, each wrapped in a `DirtyPage { index, data, compressed }`.

### 3.2 Serialize

Both `MigrationSnapshot` and `DeltaSnapshot` serialize the same way, via the
`oxicode` crate (bincode-style binary encoding) wrapped in `oxicode::serde::Compat`:

```rust
pub fn serialize(&self) -> Result<Vec<u8>, CellError> {
    oxicode::encode_to_vec(&oxicode::serde::Compat(self))
        .map_err(|e| CellError::InvalidState(format!("Serialization failed: {}", e)))
}

pub fn deserialize(data: &[u8]) -> Result<Self, CellError> {
    let (compat, _): (oxicode::serde::Compat<Self>, _) = oxicode::decode_from_slice(data)?;
    Ok(compat.0)
}
```

`RollbackInfo::capture` uses the same `oxicode::encode_to_vec(&oxicode::serde::Compat(...))`
pattern to serialize `agent.policy()` into `original_policy_data`.

### 3.3 Compress

Compression is **not** automatically applied inside `MigrationSnapshot::serialize`
— it is a separate, opt-in step the caller performs on the serialized bytes (see
§5 for the available algorithms). At the `DeltaSnapshot` level, each dirty page
compresses itself automatically and unconditionally with RLE only:

```rust
impl DirtyPage {
    pub fn compress(&mut self) {
        if self.compressed || self.data.is_empty() { return; }
        let compressed = simple_compress(&self.data);
        if compressed.len() < self.data.len() {
            self.data = compressed;
            self.compressed = true;
        }
    }
}
```

`DeltaSnapshot::create` calls `page.compress()` on every dirty page it produces.
LZ4/Zstd/adaptive compression (`migration::functions`) are standalone utilities a
caller applies explicitly to whole-snapshot byte buffers — they are not wired
into `DeltaSnapshot::create` or `MigrationSnapshot::serialize`.

### 3.4 Checksum

```rust
pub fn simple_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for (i, &byte) in data.iter().enumerate() {
        sum = sum.wrapping_add((byte as u32).wrapping_mul((i as u32).wrapping_add(1)));
    }
    sum
}
```

This is a **non-cryptographic**, position-weighted rolling sum (each byte is
multiplied by its 1-based index before accumulation) — it is designed to catch
accidental corruption/reordering during transfer, not to resist tampering. It is
stored in `DeltaSnapshot.checksum` and `RollbackInfo.original_checksum`, and is
what `StateVerifier` and `MigrationValidator` compare against after transfer.

### 3.5 Transfer

Transfer is where `mielin-cells` hands off to `mielin-mesh-wire`. The snapshot's
serialized bytes are embedded in a `Message::AgentMigration` frame (§4) and sent
over whatever transport is active (`QuicTransport` / `TcpTransport` in
`mielin-mesh-wire`). See [§10](#10-known-gaps-and-honest-caveats) for the current
state of the higher-level pre-copy `MigrationCoordinator`s.

### 3.6 Decompress

`adaptive_decompress(data, method_byte)` dispatches on the `CompressionMethod`
tag (§5); `DirtyPage::get_data()`/`decompress()` reverse the automatic per-page
RLE step during `DeltaSnapshot::apply`.

### 3.7 Verify

```rust
pub struct VerificationResult {
    pub passed: bool,
    pub checksum_valid: bool,
    pub size_valid: bool,
    pub agent_responsive: bool,
    pub details: Vec<String>,
}

impl StateVerifier {
    pub fn verify_state(original: &[u8], migrated: &[u8], original_checksum: u32) -> VerificationResult;
    pub fn verify_delta(base_state: &[u8], delta: &DeltaSnapshot, expected_checksum: u32) -> VerificationResult;
}
```

`verify_state` checks length equality and `simple_checksum(migrated) ==
original_checksum`; `agent_responsive` is present in the struct but is set to
`true` unconditionally by both constructors (`success()`/`failure()` and the
`verify_state`/`verify_delta` bodies) — there is no live liveness probe of the
restored agent in this code path today. `MigrationValidator::verify_post_migration`
wraps `verify_state` and logs a `VerificationCompleted` audit entry (§7).

Before transfer, `CompatibilityValidator::check_compatibility(requirements:
&AgentRequirements, capabilities: &NodeCapabilities) -> CompatibilityStatus`
gates whether the target node should even be attempted — see §6.

### 3.8 Deserialize

`MigrationSnapshot::deserialize` / `DeltaSnapshot::deserialize` / `Message::deserialize`
all mirror their serialize counterparts (§3.2/§4).

### 3.9 Resume

```rust
impl MigrationSnapshot {
    pub fn restore(&self) -> Result<Agent, CellError> {
        let mut agent = Agent::new(self.wasm_binary.clone());
        agent.set_policy(self.policy.clone());
        Ok(agent)
    }
}
```

`restore()` creates a brand-new `Agent` (fresh UUID, `AgentState::Created`) from
the WASM binary and re-applies the policy. **It does not restore `wasm_state`
into the new agent** (there is no linear-memory load step here — consistent
with `wasm_state` always being empty at capture time today). The caller is
responsible for calling `agent.start()`/`agent.begin_migration()`/
`agent.complete_migration()` as appropriate to move the restored agent through
the state machine described in §2.

`MigrationManager` (`migration/types/core.rs`) tracks in-flight migrations by
agent ID:

```rust
let mut manager = MigrationManager::new();
let snapshot = manager.initiate_migration(&agent, target_node)?; // captures + tracks
manager.pending_count();                                          // in-flight count
manager.get_pending(&agent_id);                                   // lookup
manager.complete_migration(&agent_id);                             // untrack
```

`initiate_migration` internally calls `MigrationSnapshot::capture` and pushes a
clone into an internal `Vec<MigrationSnapshot>`; there is no `cancel_migration`
method on this type (despite being referenced in `mielin-cells/README.md`'s
example) — only `initiate_migration`/`complete_migration`/`pending_count`/`get_pending`
exist on `MigrationManager` as of this codebase.

---

## 4. Wire Framing for Migration

The frame that the runnable 10-phase example (§9) actually sends is
`mielin_mesh_wire::Message::AgentMigration`, defined in `mielin-mesh/wire/src/lib.rs`:

```rust
pub enum Message {
    // ...
    AgentMigration {
        agent_id: [u8; 16],
        snapshot: Vec<u8>,   // MigrationSnapshot::serialize() output
        priority: u8,
    },
    MigrationAck {
        agent_id: [u8; 16],
        success: bool,
        error_msg: Option<String>,
    },
    // ...
}
```

`Message::serialize`/`deserialize` use the same `oxicode::encode_to_vec(&oxicode::serde::Compat(self))`
/ `oxicode::decode_from_slice` pattern as `MigrationSnapshot`. Two behavioral
hooks key off the migration variants:

```rust
impl Message {
    pub fn is_critical(&self) -> bool {
        match self {
            Message::AgentMigration { .. } => true,
            Message::RoutedMessage { payload, .. } => payload.is_critical(),
            _ => false,
        }
    }
    pub fn requires_ack(&self) -> bool {
        match self {
            Message::AgentMigration { .. } | Message::Discovery { .. } => true,
            Message::RoutedMessage { payload, .. } => payload.requires_ack(),
            _ => false,
        }
    }
}
```

Two other wire-layer modules consume this framing:

- **`priority.rs`** (`detect_priority`): an `AgentMigration` message's `priority`
  field maps to `Priority::Critical` when `>= 8`, `Priority::High` when `>= 5`,
  else `Priority::Normal`; a `MigrationAck { success: false, .. }` (a failed
  migration) is always bumped to `Priority::Critical` so error acknowledgements
  jump the queue.
- **`batch.rs`** (`estimate_message_size`): estimates `AgentMigration` size as
  `32 + snapshot.len()` bytes and a fixed `64` bytes for `MigrationAck`, used for
  batching heuristics.

Separately, `mielin-mesh/wire/src/migration.rs` defines a more elaborate
**pre-copy live-migration protocol** intended to run underneath this framing:

```rust
pub enum MigrationPhase { Prepare, PreCopy, StopAndCopy, Commit, Rollback }
pub enum MigrationState {
    Idle, Preparing, PreCopying, Finalizing, Committing, Completed, Failed, RollingBack,
}
pub enum MigrationMessage {
    PrepareRequest  { migration_id: [u8; 16], agent_id: [u8; 16], agent_size: usize, memory_size: usize },
    PrepareResponse { migration_id: [u8; 16], accepted: bool, reason: Option<String> },
    PreCopyData     { migration_id: [u8; 16], iteration: u32, pages: Vec<MemoryPage>, dirty_page_count: usize },
    PreCopyAck      { migration_id: [u8; 16], iteration: u32, received_pages: usize },
    StopAndCopy     { migration_id: [u8; 16], final_state: AgentSnapshot, dirty_pages: Vec<MemoryPage> },
    CommitRequest   { migration_id: [u8; 16] },
    CommitAck       { migration_id: [u8; 16], success: bool },
    RollbackRequest { migration_id: [u8; 16], reason: String },
    RollbackAck     { migration_id: [u8; 16] },
}
```

`MigrationCoordinator::execute_migration` drives these phases in order (Prepare →
PreCopy → StopAndCopy → Commit, with `rollback_migration` for the failure path),
tracked per-migration in an `ActiveMigration` struct and aggregated into a
`MigrationStats` (total_attempts/successful/failed/rolled_back/avg_duration_ms/
avg_downtime_ms/total_bytes_transferred/avg_precopy_iterations). See
[§10](#10-known-gaps-and-honest-caveats) — as written today, the phase functions
use `tokio::time::sleep` placeholders in place of a real `MigrationMessage` send
over a transport.

---

## 5. Compression and Integrity

`mielin_cells::migration::functions` exposes four interchangeable compressors,
tagged by `CompressionMethod`:

```rust
pub enum CompressionMethod {
    Rle = 0,         // simple_compress / simple_decompress (custom RLE)
    Lz4 = 1,         // lz4_compress / lz4_decompress (oxiarc_lz4)
    ZstdDefault = 2, // zstd_compress(.., ZSTD_DEFAULT_LEVEL) (oxiarc_zstd)
    ZstdHigh = 3,    // zstd_compress(.., ZSTD_HIGH_LEVEL)
}
```

- **RLE** (`simple_compress`): a hand-rolled run-length encoder — runs of 4+
  identical bytes (or any run of the escape byte `0xFF`) are encoded as
  `0xFF, count, byte`; shorter runs are copied through (with `0xFF` bytes
  individually escaped as `0xFF, 1, 0xFF`).
- **LZ4** (`lz4_compress`/`lz4_decompress`): thin wrappers over
  `oxiarc_lz4::compress`/`decompress`, using LZ4's self-describing frame format
  (content size embedded in the frame header, so decompression does not need the
  original length — `lz4_decompress` passes `usize::MAX` as the output cap).
- **Zstd** (`zstd_compress`/`zstd_decompress`): wrappers over
  `oxiarc_zstd::encode_all`/`decode_all`, with two levels defined as constants:
  `ZSTD_DEFAULT_LEVEL = 3` (balanced) and `ZSTD_HIGH_LEVEL = 10` (maximum ratio,
  slower).

`adaptive_compress(data)` picks a method heuristically without exhaustively
trying every option:

- `< 256` bytes → RLE only (`CompressionMethod::Rle`).
- `< 4096` bytes → compares RLE vs. LZ4, keeps the smaller.
- `>= 4096` bytes → also computes Zstd at `ZSTD_DEFAULT_LEVEL` and keeps the
  overall smallest of the three.

`adaptive_compress_aggressive(data)` always computes all four candidates
(RLE, LZ4, Zstd default, Zstd high) and keeps the smallest, trading CPU time for
compression ratio. Both return `(Vec<u8>, u8)` where the `u8` is the
`CompressionMethod` discriminant to pass to `adaptive_decompress(data, method)`.

**Integrity** is provided exclusively by `simple_checksum` (§3.4) — there is no
HMAC, digital signature, or cryptographic hash in the migration data path.
`DeltaSnapshot::apply` recomputes `simple_checksum` over the reconstructed state
and returns `CellError::InvalidState("Delta checksum mismatch")` if it disagrees
with the delta's stored checksum, so corrupted deltas are rejected at apply time,
not silently accepted.

At the per-page level inside `DeltaSnapshot`, compression is always RLE
(`DirtyPage::compress`) and is skipped entirely if the compressed form is not
actually smaller than the original page — there is currently no LZ4/Zstd option
for individual dirty pages, only for whole-snapshot byte buffers via the
functions above.

---

## 6. Version Compatibility and Cross-Version Migration

`mielin_cells::versioning::Version` is a plain semantic version:

```rust
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }

impl Version {
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major && self.minor >= other.minor
    }
    pub fn is_breaking_change(&self, other: &Version) -> bool { self.major != other.major }
    pub fn is_minor_update(&self, other: &Version) -> bool {
        self.major == other.major && self.minor != other.minor
    }
    pub fn is_patch_update(&self, other: &Version) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch != other.patch
    }
}
```

`VersionMetadata` attaches a `Dna`, `description`, `changelog`, a `deprecated`
flag, and an optional `migration_path: Option<Vec<Version>>` (explicit
intermediate versions required to reach this one) to each registered `Version`.
`VersionRegistry::can_migrate(from, to)` is the compatibility gate:

```rust
pub fn can_migrate(&self, from: &Version, to: &Version) -> Result<bool, VersionError> {
    let to_meta = self.get_version(to)?;
    if to_meta.deprecated { return Ok(false); }               // deprecated targets always rejected
    if to.is_compatible_with(from) { return Ok(true); }        // same major, minor >= from.minor
    match &to_meta.migration_path {
        Some(path) => Ok(path.contains(from)),                  // explicit hop required otherwise
        None => Ok(false),
    }
}
```

This is exercised by `mielin-cells/tests/cross_version_migration_tests.rs`:

- **Multi-hop chains** (`migration_v1_to_v1_5_to_v2_chain`, `migration_chain_three_hops`,
  `migration_chain_finds_intermediate`): a version registered with
  `.with_migration_path(vec![v1_5])` can only be reached from `v1_0_0` by first
  migrating to `v1_5_0` and updating the registry, then migrating from `v1_5_0`
  onward — `can_migrate(v1_0_0, v2_0_0)` returns `false` directly but `true` via
  the intermediate hop.
- **`migration_telemetry_records_hops`** ties this together with the data
  pipeline: for each hop it calls `VersionRegistry::can_migrate`, then
  `MigrationManager::initiate_migration`/`complete_migration` around a real
  `MigrationSnapshot::serialize`/`deserialize` round trip, then
  `VersionRegistry::update_agent_version` to advance the agent's recorded
  version before attempting the next hop.
- **Deprecation rejection** (`deprecated_target_rejected`,
  `multiple_deprecated_versions_in_chain`, `canary_to_deprecated_rejects`): a
  version marked `.deprecate()` is never a valid migration target, even if it
  would otherwise satisfy `is_compatible_with` or appear in a `migration_path`.
  `deprecated_source_migration_allowed` confirms migrating *away from* a
  deprecated version is still permitted.

Fleet-wide rollout uses `VersionDeployer::rolling_update(config, agents)` with
`RollingUpdateStrategy::{Immediate, Batched { batch_size, delay_between_batches_ms },
Sequential { delay_between_agents_ms }, Percentage { percentage, delay_between_batches_ms }}`;
it first calls `registry.can_migrate(from, to)` once for the whole rollout, then
updates each agent's tracked version via `VersionRegistry::update_agent_version`,
rolling back already-updated agents (`rollback_updates`) if failures exceed
`RollingUpdateConfig::max_failures` and `rollback_on_failure` is set.
`ABTestConfig`/`ABTestDeployment` and `CanaryConfig`/`CanaryDeployment` layer
traffic-split and staged-percentage rollout on top of the same version registry
(`CanaryDeployment::should_promote`/`should_abort` gate on a configurable
`success_threshold`/`max_failures`).

This registry-level gate is distinct from, and complementary to, the
per-migration hardware/capacity gate described next.

### Per-migration compatibility (`AgentRequirements` / `NodeCapabilities`)

Before a specific agent snapshot is sent to a specific target node,
`CompatibilityValidator::check_compatibility(requirements: &AgentRequirements,
capabilities: &NodeCapabilities) -> CompatibilityStatus` checks, in order:
target architecture support, available memory, available storage, maximum
agent state size, required WASM features, and a coarse same-major /
minor-or-patch-at-least semver check (`is_version_compatible`, distinct from
`Version::is_compatible_with` above — it parses plain strings). It can return:

```rust
pub enum CompatibilityStatus {
    Compatible,
    CompatibleWithWarnings(Vec<String>),   // e.g. <20% memory headroom after migration
    Incompatible(String),                  // includes a human-readable reason
}
```

`NodeCapabilities::new()` (not the derived `Default`, which yields all-zero /
empty fields) supplies realistic baseline values: `["x86_64", "aarch64"]`
architectures, 8 GiB memory, 100 GiB storage, `["bulk-memory", "simd"]` WASM
features, 1 GiB max state size, version `"0.1.0"`.

`MigrationValidator::validate_pre_migration` wraps this check and logs a
`ValidationCompleted` audit entry (§7).

---

## 7. Failure Handling and Recovery

Errors that occur during migration are classified by `MigrationErrorType`:

```rust
pub enum MigrationErrorType {
    NetworkTransient, NetworkPermanent, InsufficientResources,
    IncompatibleTarget, StateCorruption, Timeout, TargetUnreachable, Unknown,
}
```

`is_retryable()` is `true` for `NetworkTransient | InsufficientResources |
Timeout | TargetUnreachable`. `should_change_target()` is `true` for
`InsufficientResources | IncompatibleTarget | TargetUnreachable`. Each
`MigrationError` carries `agent_id`, an optional `target_node`, a `timestamp`,
and a `retry_count` incremented by `increment_retry()`.

`MigrationRecoveryManager::determine_strategy(&error) -> RecoveryStrategy`
implements the actual retry/rollback decision:

```rust
pub enum RecoveryStrategy { RetryOnSameTarget, RetryOnDifferentTarget, Rollback, GiveUp }

// if error.retry_count >= config.max_retries:
//     Rollback (if config.auto_rollback) else GiveUp
// else match error.error_type:
//     NetworkTransient | Timeout                              -> RetryOnSameTarget
//     InsufficientResources | IncompatibleTarget | TargetUnreachable -> RetryOnDifferentTarget
//     StateCorruption | NetworkPermanent | Unknown             -> Rollback (if auto_rollback) else GiveUp
```

`RecoveryConfig` (the effective `Default` — see caveat below) is:

```rust
RecoveryConfig {
    max_retries: 3,
    initial_retry_delay_secs: 10,
    max_retry_delay_secs: 300,
    use_exponential_backoff: true,
    recovery_timeout_secs: 600,
    auto_rollback: true,
}
```

`calculate_retry_delay(attempt)` is `initial_retry_delay_secs * 2^(attempt - 1)`,
capped at `max_retry_delay_secs`, when `use_exponential_backoff` is set (flat
`initial_retry_delay_secs` otherwise). `RecoveryConfig::validate()` rejects a
zero `initial_retry_delay_secs`, zero `recovery_timeout_secs`, or a
`max_retry_delay_secs` smaller than `initial_retry_delay_secs`.

The recovery loop is: `record_failure(error)` → `execute_recovery(&agent_id)`
(returns the chosen `RecoveryStrategy`, appends a `RecoveryAttempt` to
`recovery_history`, and bumps `retry_count`) → the caller acts on the strategy
and reports back via `mark_recovery_success(&agent_id)` or
`mark_recovery_failure(&agent_id, msg)`. `is_ready_for_retry`/`next_retry_time`/
`get_ready_for_retry` implement backoff-aware scheduling on top of
`calculate_retry_delay`.

**Rollback** itself is handled by `MigrationValidator` (which
`MigrationRecoveryManager` embeds):

```rust
validator.capture_rollback_info(migration_id, &agent, &state)?;  // before migration
// ... migration attempted, fails ...
let rollback: RollbackInfo = validator.execute_rollback(migration_id)?;
```

`RollbackInfo` stores `agent_id`, `original_state`, `original_checksum`
(`simple_checksum`), `original_policy_data` (oxicode-serialized `Policy`),
`created_at`, and `source_node`. `execute_rollback` removes the stored info (a
rollback can only be executed once) and fails with `"Rollback info has
expired"` if `RollbackInfo::is_valid(max_age_secs)` is false — the default max
age is `3600` seconds, configurable via `MigrationValidator::set_rollback_max_age`.

All of the above is recorded to an append-only `MigrationAuditLog`
(`migration/types/audit.rs`), capacity-bounded (default 10,000 entries,
oldest evicted first) and queryable by migration ID, agent ID, event type, time
range, or failures-only:

```rust
pub enum AuditEventType {
    MigrationStarted, ValidationCompleted, SnapshotCaptured,
    TransferStarted, TransferCompleted, VerificationCompleted,
    MigrationCompleted, MigrationFailed, RollbackStarted, RollbackCompleted,
}
```

`TransferStarted`/`TransferCompleted` are declared in the enum but, as of this
codebase, no call site constructs them (there is no `log_transfer_*` helper
alongside `log_start`/`log_completion`/`log_validation`/`log_verification`/
`log_rollback`) — they are reserved for a future transfer-phase logging hook.

---

## 8. Telemetry and Observability

Telemetry is **not consolidated into one type** — there are four distinct
migration-stats structures at different layers, which is worth knowing before
searching the codebase for "the" migration metrics type:

1. **`mielin_cells::migration::MigrationStats`** (`migration/types/delta.rs`) —
   `bytes_transferred`, `bytes_saved_compression`, `bytes_saved_delta`,
   `full_migrations`, `incremental_migrations`, `avg_delta_ratio`,
   `total_time_us`, with `record_full`, `record_incremental`,
   `record_compression`, `overall_compression_ratio()`, `throughput_mbps()`.
   `IncrementalMigrator::create_incremental` calls `record_incremental` and
   `record_compression` automatically; `record_full` exists but is not invoked
   anywhere outside tests — `MigrationManager` does not track `MigrationStats`
   at all today.

2. **`mielin_cells::migration::{MigrationProgressEvent, MigrationProgressTracker}`**
   (`migration/types/progress.rs`) — a `MigrationPhase` (`Capturing`,
   `Transferring`, `Validating`, `Restoring`, `Completed`, `Failed`) driven
   through a pluggable `MigrationProgressCallback` trait:
   - `CollectingCallback` — buffers events in a `Vec` for later inspection.
   - `LoggingCallback::new(verbose: bool)` — prints to stdout/stderr; throttles
     `TransferProgress` output to every 10% unless `verbose`.
   - `MultiCallback` — fans a single event out to multiple registered callbacks.

   `MigrationProgressEvent` variants: `Started { agent_id, total_bytes }`,
   `CapturingState { agent_id, percent }`, `TransferProgress { agent_id,
   transferred_bytes, total_bytes, percent }`, `Validating { agent_id, step }`,
   `Restoring { agent_id, percent }`, `Completed { agent_id, duration_ms,
   bytes_transferred }`, `Failed { agent_id, error }`; `.is_terminal()` is true
   for `Completed`/`Failed`.

3. **`mielin_mesh_wire::migration::MigrationStats`** — `total_attempts`,
   `successful`, `failed`, `rolled_back`, `avg_duration_ms`, `avg_downtime_ms`,
   `total_bytes_transferred`, `avg_precopy_iterations`, updated by the wire-layer
   `MigrationCoordinator` as it moves through Prepare/PreCopy/StopAndCopy/Commit.

4. **`mielin_mesh_core::migration::MigrationMetrics`** (per-migration) and
   **`mielin_mesh_core::metrics::MigrationSuccessMetrics`** (fleet-wide,
   Prometheus-style):

   ```rust
   pub struct MigrationMetrics {
       pub agent_id: AgentId, pub source_node: NodeId, pub target_node: NodeId,
       pub strategy: MigrationStrategy,   // PreCopy | PostCopy | Hybrid
       pub phase: MigrationPhase,         // Planning..Complete/Failed, PreCopy{iteration}
       pub started_at: SystemTime, pub completed_at: Option<SystemTime>,
       pub total_bytes: usize, pub transferred_bytes: usize,
       pub downtime_ms: u64, pub total_duration_ms: u64,
       pub precopy_iterations: usize, pub success: bool, pub error_message: Option<String>,
   }

   pub struct MigrationSuccessMetrics {
       pub total_migrations: Counter, pub successful_migrations: Counter,
       pub failed_migrations: Counter, pub cancelled_migrations: Counter,
       precopy_count: Counter, postcopy_count: Counter, hybrid_count: Counter,
       pub migration_duration: Histogram,   // ms buckets: 100,500,1000,2000,5000,10000,30000,60000
       pub migration_downtime: Histogram,   // ms buckets: 10,50,100,250,500,1000,2000,5000
       pub data_transferred: Histogram,     // byte-size buckets
       pub active_migrations: Gauge,
       recent_results: RwLock<Vec<MigrationResult>>,  // ring buffer, default cap 100
   }
   ```

   `MigrationSuccessMetrics::record_start(strategy: &str)` /
   `record_complete(result: MigrationResult).await` / `record_cancelled()` feed
   the counters/histograms; `.summary() -> MigrationSuccessSummary` exposes
   `success_rate`, `avg_duration_ms`/`p99_duration_ms`, and
   `avg_downtime_ms`/`p99_downtime_ms` (via `Histogram::stats().percentile(99.0)`).
   This is re-exported from `mielin_mesh_core::metrics`.

`mielin_mesh_core::migration::MigrationCoordinator` additionally exposes
`active_migration_count()`, `get_migration_stats() -> MigrationStats` (a fifth,
simpler summary type: `total_migrations`, `active_migrations`,
`successful_migrations`, `failed_migrations`, `average_downtime_ms`,
`average_duration_ms`), `get_migration_metrics(agent_id)`,
`get_migration_history(limit)`, and cooperative cancellation via
`cancel_migration`/`cancel_all_migrations`/`subscribe_cancellations`
(`tokio_util::sync::CancellationToken` + a `broadcast` channel).

---

## 9. Worked Example: `examples/agent-migration`

`examples/agent-migration/src/main.rs` is the runnable "Saltatory Conduction"
demo referenced by `QUICKSTART.md` (`cargo run -p agent-migration`) and by the
"Agent Migration Flow (10 Phases)" diagram in `docs/ARCHITECTURE.md`. Its ten
phases, with the real API calls behind each:

| Phase | What it does | Key call |
|---|---|---|
| 1 | Create source (Edge) and target (Core) nodes | `Node::new(NodeRole::Edge)`, `Node::new(NodeRole::Core)` |
| 2 | Detect hardware capabilities | `HardwareProfile::detect()` |
| 3 | Create an agent from a WASM binary | `Agent::new(wasm_binary)` |
| 4 | Initiate migration, capture snapshot | `MigrationManager::new()` + `initiate_migration(&agent, Some(target_id))` |
| 5 | Serialize the snapshot for network transfer | `snapshot.serialize()` |
| 6 | Build the wire message | `Message::AgentMigration { agent_id, snapshot, priority: 10 }` |
| 7 | "Simulate network transfer" (see caveat below) | `migration_msg.serialize()` |
| 8 | Deserialize on the target node | `Message::deserialize(&msg_bytes)` |
| 9 | Restore the agent on the target node | `MigrationSnapshot::deserialize(...)`, then `.restore()` |
| 10 | Send a migration acknowledgment | `Message::MigrationAck { agent_id, success: true, error_msg: None }`, then `migration_manager.complete_migration(&agent_id)` |

Phase 7 is titled "Simulating network transfer" in the source, and indeed the
example never opens a socket — it calls `Message::serialize()`/`deserialize()`
directly in the same process to demonstrate the wire format round trip. Real
network I/O for `Message` would go through `QuicTransport` or `TcpTransport`
(`mielin-mesh-wire`), which this particular example does not exercise.

Run it with:

```bash
cargo run -p agent-migration
RUST_LOG=debug cargo run -p agent-migration   # verbose logging, per QUICKSTART.md
```

Three further runnable references build on the same pipeline:

- **`mielin-cells/examples/migration.rs`** — a lower-level tour of
  `MigrationSnapshot::capture`/`serialize`/`restore`, all four compression
  functions, `DeltaSnapshot::create`/`apply`, `DirtyPageTracker`,
  `MigrationManager`, and `MigrationProgressTracker`, without any networking.
- **`examples/e2e-agent-migration/src/main.rs`** — an in-process multi-node
  simulation (`MigrationNode` wrapping a `Node`, an `Arc<RwLock<MigrationManager>>`
  and local stats) that walks through `IncrementalMigrator`-based delta capture,
  `mielin_mesh_core::migration::MigrationStrategy` selection, and rollback/
  telemetry tracking across pre-copy and post-copy strategies.
- **`mielin-cells/examples/policy_migration.rs`** — demonstrates the *trigger*
  side (`MigrationPolicy`, `BatteryTriggerConfig`/`LatencyTriggerConfig`/
  `CostTriggerConfig`/`LoadTriggerConfig`, `PolicyEvaluator`) that decides
  *when* to invoke the pipeline documented here.

---

## 10. Known Gaps and Honest Caveats

For anyone extending this subsystem, the following are real, code-verified gaps
rather than design choices to preserve:

- **`wasm_state` is never populated.** `MigrationSnapshot::capture` always sets
  `wasm_state: vec![]`, and `restore()` never attempts to load linear memory
  back into the new `Agent`. Only the WASM binary (code) and `Policy` actually
  survive a `MigrationSnapshot` round trip today; runtime memory state does not.
- **The mesh-level pre-copy coordinators are phase/telemetry scaffolds, not a
  finished data plane.** Both `mielin_mesh_wire::migration::MigrationCoordinator`
  and `mielin_mesh_core::migration::MigrationCoordinator` implement realistic
  state machines (phase tracking, timeout monitoring, cancellation, stats
  aggregation), but their phase-execution functions contain `tokio::time::sleep`
  placeholders and comments such as `"Send prepare request (in real
  implementation, would use transport)"` and `"In real implementation: 1.
  Snapshot dirty memory pages 2. Transfer to target node..."` — the actual byte
  transfer of `MigrationMessage`/dirty pages over a live transport is not yet
  wired into these coordinators. The genuinely working transfer path is the
  simpler one in §3.5/§4: an `mielin_cells::migration::MigrationSnapshot`
  serialized into a `mielin_mesh_wire::Message::AgentMigration` frame.
- **`RecoveryConfig` has two conflicting `Default` implementations in the source
  tree, only one of which is compiled.** `mielin-cells/src/migration/types/recovery.rs`
  defines `impl Default for RecoveryConfig` with `initial_retry_delay_secs: 10,
  max_retry_delay_secs: 300, recovery_timeout_secs: 600` (this is the one
  actually used — confirmed by both `RecoveryConfig::default()` call sites and
  the `test_recovery_config_default` unit test). A second, orphaned file,
  `mielin-cells/src/migration/recoveryconfig_traits.rs`, defines a different
  `Default` (`initial_retry_delay_secs: 5, max_retry_delay_secs: 60,
  recovery_timeout_secs: 300`) but is **not** declared as a module in
  `migration/mod.rs`, so it is dead code that never compiles or takes effect.
  This document uses the real, active values.
- **`agent_responsive` in `VerificationResult` is never independently
  measured.** It is hard-coded to `true` by every constructor/call site; there
  is no liveness probe of the restored agent distinct from the checksum/size
  checks.
- **`AuditEventType::TransferStarted`/`TransferCompleted`** are declared but
  unused — no code path currently logs them.
- **`MigrationManager` has no `cancel_migration` method**, despite
  `mielin-cells/README.md`'s example code calling
  `manager.cancel_migration(&agent_id)`. Only `initiate_migration`,
  `complete_migration`, `pending_count`, and `get_pending` exist.
- **`Dna::hash()` is not a cryptographic hash** despite being labeled "SHA-256
  hash" in `docs/ARCHITECTURE.md`'s code excerpt — `Dna::compute_hash` sets only
  `hash[0] = data.len() as u8` (truncated modulo 256) and leaves the remaining
  31 bytes zero. It is unrelated to the `simple_checksum` integrity mechanism
  described in §3.4/§5, which is what migration actually relies on.

---

## See Also

- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — overall system architecture,
  including the same 10-phase migration flow from the mesh/networking angle
- [`PROTOCOL.md`](./PROTOCOL.md) — full mesh wire protocol reference
- [`NETWORKING.md`](./NETWORKING.md) — transport, discovery, and connection
  handling that migration messages travel over
- [`mielin-cells/README.md`](../mielin-cells/README.md) — Agent SDK guide,
  including the Policy engine that decides *when* to migrate
