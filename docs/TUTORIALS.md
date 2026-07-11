# MielinOS Tutorials

A hands-on, progressive tutorial series for MielinOS. Every snippet here is grounded in the
**real, current API** — either copied directly from a compiling example under [`examples/`](../examples/)
or written against the exact public signatures in the crate source (`mielin-cells`, `mielin-tensor`,
`mielin-wasm`, `mielin-hal`, `mielin-mesh-core`, `mielin-mesh-wire`, `mielin-rt`).

Where an example prints output, this guide shows **real output captured by actually running the
example** (`cargo run -p <name>`) on x86_64/Linux with 16 cores — your UUIDs, timings, and
core count will differ, but the shape of the output is exact. Where running the example live was
impractical for this document (long-running servers, multi-minute benchmarks), the output is
clearly marked **"illustrative"** and derived from the real `info!`/`println!` call sites in the
source, not invented.

> If a snippet in [`README.md`](../README.md) or [`QUICKSTART.md`](../QUICKSTART.md) doesn't match
> what's here, trust this document and the source — see the **"Docs vs. reality"** note at the end.

## Prerequisites

```bash
git clone https://github.com/cool-japan/mielin
cd mielin
rustup show                      # picks up rust-toolchain.toml (nightly, rustfmt+clippy+llvm-tools)
cargo build --workspace          # first build compiles wasmtime, oxi* crates, etc. — takes a few minutes
```

The workspace `Cargo.toml` declares `examples/*` as members, so every tutorial below can be run
with `cargo run -p <package-name>` from the repository root. Package names (left) and binary names
(right, when they differ) are called out explicitly in each tutorial.

## Table of Contents

1. [Hello Agent](#tutorial-1-hello-agent)
2. [Agent Lifecycle & State Transitions](#tutorial-2-agent-lifecycle--state-transitions)
3. [Inter-Agent Messaging](#tutorial-3-inter-agent-messaging)
4. [Agent Groups & Roles](#tutorial-4-agent-groups--roles)
5. [Policies & Migration Decisions](#tutorial-5-policies--migration-decisions)
6. [Agent Migration End-to-End](#tutorial-6-agent-migration-end-to-end)
7. [Tensor Basics](#tutorial-7-tensor-basics)
8. [Quantization & Backend Selection](#tutorial-8-quantization--backend-selection)
9. [Running a WASM-Sandboxed Agent](#tutorial-9-running-a-wasm-sandboxed-agent)
10. [Hardware Capability Detection with mielin-hal](#tutorial-10-hardware-capability-detection-with-mielin-hal)
11. [Starting a Mesh Node / Small Cluster](#tutorial-11-starting-a-mesh-node--small-cluster)
12. [Embedded/IoT Runtime with mielin-rt](#tutorial-12-embeddediot-runtime-with-mielin-rt)

---

## Tutorial 1: Hello Agent

**Goal:** Create your first agent and inspect its identity, state, and DNA hash.

**Grounded in:** [`examples/hello-agent/src/main.rs`](../examples/hello-agent/src/main.rs),
`mielin_cells::Agent` (`mielin-cells/src/agent.rs`, `mielin-cells/src/dna.rs`).

**Prerequisites:** `cargo build -p hello-agent` (pulls in `mielin-cells` and `mielin-wasm`).

An agent is created from a raw WASM binary (its "DNA"). `Agent::new` assigns a random UUID, hashes
the binary into `Dna`, and starts the agent in `AgentState::Created`:

```rust
use mielin_cells::Agent;

#[tokio::main]
async fn main() {
    println!("MielinOS - Hello Agent Example");

    // Minimal valid WASM module header (magic number + version, no sections)
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let agent = Agent::new(wasm_binary);

    println!("Created agent with ID: {}", agent.id());
    println!("Agent state: {:?}", agent.state());
    println!("Agent DNA hash: {:?}", agent.dna().hash());
}
```

Run it:

```bash
cargo run -p hello-agent
```

**Real captured output** (UUID and hash will differ per run — the DNA hash here is `[8, 0, 0, ...]`
because `Dna::compute_hash` in `mielin-cells/src/dna.rs` is a placeholder that stores the binary
length in the first byte, not a cryptographic digest):

```
MielinOS - Hello Agent Example
Created agent with ID: 19aade9e-b693-468e-8f20-26465ab0ac53
Agent state: Created
Agent DNA hash: [8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
```

**What you learned:**
- `Agent::new(wasm_binary: Vec<u8>) -> Agent` is the entry point for creating agents.
- `agent.id() -> AgentId` (a `Uuid`), `agent.state() -> &AgentState`, `agent.dna() -> &Dna`.
- `Dna::hash()` returns a `&[u8; 32]` — currently a length-derived placeholder, not a real hash.
  Don't rely on it for content-addressing until this is upgraded.

**Next steps:** Tutorial 2 drives the agent through its full lifecycle state machine.

---

## Tutorial 2: Agent Lifecycle & State Transitions

**Goal:** Understand the `AgentState` state machine and drive an agent through valid transitions,
including error handling and recovery.

**Grounded in:** `mielin-cells/src/agent.rs` (`AgentState`, `Agent::transition_to`,
`Agent::set_error`, `Agent::attempt_recovery` — every call below matches a `#[test]` in that file).

**Prerequisites:** `mielin-cells` crate only (no async runtime needed for this tutorial).

`AgentState` has seven states: `Created`, `Running`, `Paused`, `Suspended`, `Migrating`, `Error`,
`Terminated`. Transitions are validated by `AgentState::can_transition_to`; invalid transitions are
rejected rather than panicking.

```rust
use mielin_cells::{Agent, AgentError, AgentState, TransitionResult};

fn main() {
    let mut agent = Agent::new(vec![]);
    assert_eq!(agent.state(), &AgentState::Created);

    // Created -> Paused is NOT a valid transition
    match agent.pause() {
        TransitionResult::InvalidTransition { from, to } => {
            println!("Rejected: {:?} -> {:?}", from, to);
        }
        _ => unreachable!(),
    }

    // Created -> Running is valid
    assert!(matches!(agent.start(), TransitionResult::Success));
    println!("State: {:?}", agent.state()); // Running

    // Running -> Paused -> Running
    agent.pause();
    println!("State: {:?}", agent.state()); // Paused
    agent.resume();
    println!("State: {:?}", agent.state()); // Running

    // Simulate an execution failure
    let error = AgentError::new("host function trapped").with_code(42);
    agent.set_error(error);
    println!("State: {:?}, error: {:?}", agent.state(), agent.error());

    // Recover back to Running (clears the error)
    let result = agent.attempt_recovery();
    assert!(matches!(result, TransitionResult::Success));
    assert!(agent.error().is_none());

    // Begin/complete a migration
    agent.begin_migration();
    println!("State: {:?}", agent.state()); // Migrating
    agent.complete_migration();
    println!("State: {:?}", agent.state()); // Running (back)

    // Terminated is a true terminal state
    agent.terminate();
    println!("State: {:?}", agent.state()); // Terminated
    assert!(agent.state().is_terminal());
}
```

This isn't tied to a single example binary — it's assembled directly from the doc comment and unit
tests in `mielin-cells/src/agent.rs`, so every call is exactly what ships. You can paste it into a
scratch `fn main()` in any crate that depends on `mielin-cells`, or run
`cargo test -p mielin-cells agent::tests` to see the equivalent assertions execute.

Key facts from the state machine (see the ASCII diagram and transition table in
`mielin-cells/src/agent.rs`):
- `Created` can only go to `Running` or `Terminated`.
- `Terminate` is reachable from every non-terminal state (Running, Paused, Suspended, Migrating,
  Error).
- `AgentError::fatal()` marks an error unrecoverable — `attempt_recovery()` then returns
  `TransitionResult::Blocked { reason }` instead of transitioning.
- Agent keeps a bounded transition history: `agent.state_history()` returns the last 10
  `(AgentState, Instant)` pairs.

**What you learned:**
- The full `AgentState` transition table and how to query it with `can_transition_to`.
- `TransitionResult` has three variants: `Success`, `InvalidTransition`, `Blocked`.
- Error recovery via `AgentError` / `Agent::set_error` / `Agent::attempt_recovery`.

**Next steps:** Tutorial 3 connects multiple agents together with the messaging API.

---

## Tutorial 3: Inter-Agent Messaging

**Goal:** Send point-to-point messages, do request/response, and broadcast to topic subscribers
using the built-in `MessageBus`.

**Grounded in:** `mielin-cells/src/messaging.rs` (`MessageBus`, `Mailbox`, `Message`, `Topic`,
`Priority`) — adapted directly from the `#[tokio::test]` functions in that file (`test_send_message`,
`test_pubsub`, `test_request_response`).

**Prerequisites:** `mielin-cells` with the `tokio` runtime (`#[tokio::main]`).

`MessageBus` supports three patterns: direct send (with offline queueing), pub/sub over `Topic`s,
and request/response with a timeout.

```rust
use mielin_cells::{Message, MessageBus, Priority, Topic};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let bus = Arc::new(MessageBus::new());

    // --- 1. Direct send ---
    let alice = Uuid::new_v4();
    let bob = Uuid::new_v4();
    let _alice_mailbox = bus.register(alice).await;
    let mut bob_mailbox = bus.register(bob).await;

    let msg = Message::new(alice, bob, b"hello bob".to_vec()).with_priority(Priority::High);
    bus.send(msg).await.expect("send failed");

    if let Some(received) = bob_mailbox.recv_timeout(Duration::from_millis(100)).await {
        println!("Bob received: {}", String::from_utf8_lossy(&received.payload));
    }

    // --- 2. Pub/sub ---
    let news_topic = Topic::new("news");
    let mut sub_mailbox = bus.register(Uuid::new_v4()).await;
    bus.subscribe(sub_mailbox.agent_id(), news_topic.clone()).await;

    let broadcast = Message::broadcast(alice, news_topic, b"breaking news".to_vec());
    let delivered = bus.publish(broadcast).await.expect("publish failed");
    println!("Delivered to {} subscriber(s)", delivered);
    let _ = sub_mailbox.recv_timeout(Duration::from_millis(100)).await;

    // --- 3. Request/response ---
    let responder = Uuid::new_v4();
    let mut responder_mailbox = bus.register(responder).await;
    let bus_clone = Arc::clone(&bus);
    tokio::spawn(async move {
        if let Some(req) = responder_mailbox.recv_timeout(Duration::from_millis(200)).await {
            let response = req.create_response(responder, b"pong".to_vec());
            bus_clone.send(response).await.expect("respond failed");
        }
    });

    let request = Message::new(alice, responder, b"ping".to_vec());
    let response = bus.request(request, Duration::from_millis(500)).await.expect("no response");
    println!("Got response: {}", String::from_utf8_lossy(&response.payload));
}
```

**What you learned:**
- `MessageBus::register(agent_id) -> Mailbox` returns the receiving end; keep the returned
  `Mailbox` alive or the agent is treated as offline.
- Messages sent to an unregistered/offline agent are queued (bounded, default 1000 per agent) and
  flushed on `register`.
- `MessageBus::request` internally uses a `oneshot` channel keyed by message ID and times out with
  `MessagingError::Timeout`.
- Duplicate `Message::id`s are rejected (`MessagingError::DuplicateMessage`) within a
  deduplication window (default 60s) — useful when messages can be retried at the transport layer.

**Next steps:** Tutorial 4 shows how to organize many agents into coordinated `AgentGroup`s.

---

## Tutorial 4: Agent Groups & Roles

**Goal:** Group agents under `Leader`/`Member`/`Observer` roles, apply a shared policy, and drive a
coordinated state transition across the whole group.

**Grounded in:** `mielin-cells/src/group.rs` (`AgentGroup`, `GroupConfig`, `GroupRole`,
`GroupCoordinator`, `GroupRegistry`) — mirrors `coordinator_tests::test_transition_all_transitions_real_agents`.

**Prerequisites:** `mielin-cells` (no async runtime required — group state uses `std::sync::RwLock`).

```rust
use mielin_cells::{Agent, AgentGroup, AgentState, GroupCoordinator, GroupRole};
use std::collections::HashMap;
use std::sync::Arc;

fn main() {
    let group = Arc::new(AgentGroup::new("inference-cluster"));

    // Create three real agents and add them to the group
    let mut agents = HashMap::new();
    for i in 0..3 {
        let agent = Agent::new(vec![]);
        let id = agent.id();
        let role = if i == 0 { GroupRole::Leader } else { GroupRole::Member };
        group.add_member(id, role).unwrap(); // GroupResult<()>::unwrap()
        agents.insert(id, agent);
    }

    println!("Group '{}' has {} members ({} leaders)",
        group.name(), group.member_count(), group.leader_count());

    // Tag the group for discovery, e.g. by a GroupRegistry
    group.add_tag("production");

    // Coordinate a transition across every member's REAL Agent
    let coordinator = GroupCoordinator::new(group.clone());
    let results = coordinator.transition_all(&mut agents, |agent| agent.start());
    for (agent_id, result) in &results {
        println!("{} -> {:?}", agent_id, result);
    }

    // Confirm every agent actually reached Running
    assert!(coordinator.all_in_state(&agents, |s| *s == AgentState::Running));
    println!("All agents are now Running");
}
```

`GroupResult<T>` is a custom `Success(T)`/`Error(GroupError)` enum (not `std::result::Result`), so
`.unwrap()` and `.is_success()` are inherent methods defined on it, not the standard trait.

**What you learned:**
- `AgentGroup::add_member` enforces `GroupConfig` constraints (`max_members`, `min_leaders`,
  `max_leaders`) — removing the last leader is rejected with `GroupError::MinLeadersViolation`.
- `GroupCoordinator::transition_all` takes a closure `FnMut(&mut Agent) -> TransitionResult` and
  applies it to every group member found in the agent map you provide; agents missing from the map
  come back as `TransitionResult::Blocked`.
- `GroupRegistry` (not shown above) indexes groups by agent membership and by tag —
  `groups_for_agent` / `groups_by_tag` — useful once you have many groups.

**Next steps:** Tutorial 5 shows how a `Policy` and `PolicyEvaluator` decide *when* an agent or
group should migrate.

---

## Tutorial 5: Policies & Migration Decisions

**Goal:** Configure per-agent policy constraints and evaluate node metrics against a
`MigrationPolicy` to get an automatic migrate/stay decision.

**Grounded in:** `mielin-cells/src/policy.rs` (`Policy`, `NodeMetrics`, `MigrationPolicy`,
`PolicyEvaluator`, `MigrationDecision`, `TargetRequirements`, `score_target_node`,
`select_best_target`) — mirrors `test_policy_evaluator_evaluate_thermal` and
`test_select_best_target`.

**Prerequisites:** `mielin-cells` only.

There are two related but distinct concepts here:
- `Policy` — a simple, per-agent execution policy (`min_battery_percent`, `max_latency_ms`,
  `preferred_architectures`), stored on `Agent::policy()`.
- `MigrationPolicy` + `PolicyEvaluator` — a richer, per-node policy engine with five independent
  triggers (thermal, battery, load, latency, cost), each individually configurable and rate-limited.

```rust
use mielin_cells::{Agent, Policy};
// NodeMetrics/MigrationPolicy/PolicyEvaluator/score_target_node/select_best_target live in the
// `policy` submodule and are NOT re-exported at the mielin_cells crate root (only `Policy` is —
// see the `pub use policy::Policy;` line in mielin-cells/src/lib.rs).
use mielin_cells::policy::{MigrationPolicy, NodeMetrics, PolicyEvaluator};

fn main() {
    // --- Per-agent Policy ---
    let mut agent = Agent::new(vec![]);
    let policy = Policy {
        min_battery_percent: 25,
        max_latency_ms: 50,
        preferred_architectures: vec!["aarch64".to_string()],
    };
    policy.validate().expect("policy should be valid");
    agent.set_policy(policy);

    // --- Node-level MigrationPolicy + evaluator ---
    let evaluator = PolicyEvaluator::new(MigrationPolicy::performance_optimized());

    let hot_node = NodeMetrics {
        cpu_load_percent: 40,
        memory_usage_percent: 50,
        temperature_celsius: 90.0, // above the 85.0 critical default threshold
        network_latency_ms: 20,
        cost_per_unit: 0.4,
        on_ac_power: true,
        battery_percent: Some(80),
        ..NodeMetrics::new()
    };

    let decision = evaluator.evaluate(&hot_node);
    println!(
        "should_migrate={} trigger={:?} priority={} reason={}",
        decision.should_migrate, decision.trigger, decision.priority, decision.reason
    );
    // Thermal is evaluated first (highest priority for safety), so even with only
    // moderate CPU load, an overheating node wins: trigger=ThermalBased, priority=100.

    // --- Picking the best target from candidates ---
    let candidates = vec![
        (0, NodeMetrics { cpu_load_percent: 70, ..NodeMetrics::new() }),
        (1, NodeMetrics { cpu_load_percent: 30, on_ac_power: true, ..NodeMetrics::new() }),
        (2, NodeMetrics { cpu_load_percent: 90, ..NodeMetrics::new() }), // too loaded
    ];
    let best = mielin_cells::policy::select_best_target(&candidates, &decision.target_requirements);
    println!("Best target index: {:?}", best);
}
```

**What you learned:**
- `PolicyEvaluator::evaluate` checks triggers in a fixed priority order — thermal > battery > load
  > latency > cost — and stops at the first one that fires.
- `PolicyEvaluator::migration_allowed()` enforces a cooldown (`migration_cooldown_secs`, default
  60s) and an hourly cap (`max_migrations_per_hour`, default 10) via `record_migration()`.
- `MigrationPolicy` ships three presets: `performance_optimized()`, `cost_optimized()`,
  `battery_optimized()`.
- `score_target_node` / `select_best_target` rank candidate nodes by a weighted score (lower CPU
  load, memory, temperature, latency, and cost all increase the score; AC power and low agent
  count add bonus points).
- Every config type here has a `.validate()` method returning `Result<(), String>` — call it after
  deserializing policy from an external config file.

**Next steps:** Tutorial 6 puts `Policy` decisions into action with a full agent migration.

---

## Tutorial 6: Agent Migration End-to-End

**Goal:** Walk through MielinOS's "Saltatory Conduction" (跳躍伝導) migration flow — snapshot,
serialize, transfer, restore — exactly as `examples/agent-migration` does it.

**Grounded in:** [`examples/agent-migration/src/main.rs`](../examples/agent-migration/src/main.rs)
(package `agent-migration`), `mielin_cells::migration::{MigrationManager, MigrationSnapshot}`
(`mielin-cells/src/migration/types/core.rs`), `mielin_mesh_wire::Message`,
`mielin_mesh_core::{Node, NodeRole}`, `mielin_hal::capabilities::HardwareProfile`.

**Prerequisites:** `mielin-cells`, `mielin-mesh-core`, `mielin-mesh-wire`, `mielin-hal`,
`mielin-wasm`, `tokio`, `uuid` (see [`examples/agent-migration/Cargo.toml`](../examples/agent-migration/Cargo.toml)).

Run the example directly:

```bash
cargo run -p agent-migration
```

**Real captured output** (10 phases, abridged — UUIDs/timestamps vary per run):

```
=== MielinOS: Agent Migration Demo ===

Phase 1: Creating source and target nodes
  Source Node ID: 42ab699c-fdcb-4dd6-b10a-57271ba6672e
  Target Node ID: 1cd281f6-6cfa-4ae6-9f3f-07611f03ed9b
  Source Role: Edge
  Target Role: Core

Phase 2: Detecting hardware capabilities
  Architecture: x86_64
  CPU Cores: 16
  SIMD Support: true
  Tensor Ops: false
  Max Vector Width: 256 bits

Phase 3: Creating agent with WASM binary
  Agent ID: 744d4f74-b851-43fa-ac13-69ab260c2796
  Agent State: Created
  DNA Hash: [8, 0, 0, 0, ...]

Phase 4: Initiating migration (Saltatory Conduction)
  Snapshot Size: 72 bytes
  Timestamp: 1783734532
  Pending Migrations: 1

Phase 5: Serializing snapshot for network transfer
  Serialized Size: 51 bytes

Phase 6: Creating migration message
  Message Type: AgentMigration
  Is Critical: true
  Requires ACK: true

Phase 7: Simulating network transfer
  Total Message Size: 70 bytes

Phase 8: Deserializing on target node
  Received Agent ID: [74, 4d, 4f, 74, b8, 51, ...]
  Priority: 10
  Snapshot Age: 0 seconds

Phase 9: Restoring agent on target node
  Restored Agent ID: ec26b823-98b9-4c9a-b57e-1cfecfd59560
  Restored State: Created
  DNA Match: true

Phase 10: Sending migration acknowledgment
  ACK Size: 19 bytes
  Migration Completed
  Pending Migrations: 0

=== Migration Complete: Saltatory Conduction Successful ===

Agent successfully migrated from Edge to Core node
This demonstrates MielinOS's ability to perform ultra-fast
agent migration across heterogeneous hardware platforms.
```

The core of the flow, distilled:

```rust
use mielin_cells::{migration::{MigrationManager, MigrationSnapshot}, Agent};
use mielin_mesh_core::{Node, NodeRole};
use mielin_mesh_wire::Message;

#[tokio::main]
async fn main() {
    let source_node = Node::new(NodeRole::Edge);
    let target_node = Node::new(NodeRole::Core);

    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    // 1. Snapshot + track as pending
    let mut migration_manager = MigrationManager::new();
    let snapshot = migration_manager
        .initiate_migration(&agent, Some(*target_node.id().as_bytes()))
        .expect("snapshot failed");

    // 2. Serialize (oxicode) for the wire
    let serialized = snapshot.serialize().expect("serialize failed");

    // 3. Wrap in a wire Message and (de)serialize as if sent over the network
    let migration_msg = Message::AgentMigration {
        agent_id: snapshot.agent_id,
        snapshot: serialized.clone(),
        priority: 10,
    };
    let msg_bytes = migration_msg.serialize().expect("wire serialize failed");
    let received_msg = Message::deserialize(&msg_bytes).expect("wire deserialize failed");

    if let Message::AgentMigration { agent_id, snapshot: received, .. } = received_msg {
        let restored_snapshot = MigrationSnapshot::deserialize(&received).unwrap();
        let restored_agent = restored_snapshot.restore().expect("restore failed");

        // 4. Acknowledge and clean up the pending-migration bookkeeping
        let _ack = Message::MigrationAck { agent_id, success: true, error_msg: None };
        migration_manager.complete_migration(&agent_id);

        assert_eq!(restored_agent.dna().binary(), agent.dna().binary());
        println!("Migrated {} -> new instance {}", agent.id(), restored_agent.id());
    }
}
```

For a multi-strategy version (Pre-Copy, Post-Copy, Hybrid, plus incremental delta migration with
`IncrementalMigrator`), see
[`examples/e2e-agent-migration`](../examples/e2e-agent-migration/src/main.rs)
(`cargo run -p e2e-agent-migration`) — it simulates edge/relay/core nodes in a single process and
prints throughput per migration strategy, e.g.:

```
🧪 Test 1: Pre-Copy Migration (Edge → Relay)
   Snapshot size: 72 bytes
   ...
   Total time: 101.67188ms
   Throughput: 0.49 KB/s
```
(captured from a real run; exact timings depend on your machine)

**What you learned:**
- `MigrationSnapshot::capture(&agent, source_node) -> Result<Self, CellError>` — copies the WASM
  binary, policy, and a timestamp. `wasm_state` is currently always empty (`vec![]`); live memory
  snapshotting is not yet implemented — see `MigrationSnapshot::capture` in
  `mielin-cells/src/migration/types/core.rs`.
- `MigrationSnapshot::restore()` creates a **brand-new** `Agent` (new random UUID) from the
  captured binary + policy — the restored agent is a different `Agent::id()` from the original by
  design; only the DNA and policy are preserved.
- `MigrationManager` just tracks pending snapshots in a `Vec` (`pending_count`,
  `complete_migration`) — it does not perform any network I/O itself.
- `mielin_mesh_wire::Message::AgentMigration/MigrationAck` are the wire-level envelope;
  `Message::is_critical()` / `requires_ack()` tell a transport layer how to prioritize delivery.
- `IncrementalMigrator` (in `e2e-agent-migration`) supports delta-only migration via
  `start_tracking` / `mark_dirty` / `create_incremental` for cheaper repeated migrations.

**Next steps:** Tutorial 11 shows migration driven over a real QUIC transport between two OS
processes instead of in-process simulation.

---

## Tutorial 7: Tensor Basics

**Goal:** Create tensors, run elementwise arithmetic, matrix multiplication, and reductions through
the hardware-aware `TensorRuntime`.

**Grounded in:** `mielin-tensor/src/tensor.rs` (`Tensor<f32>`), `mielin-tensor/src/ops.rs`
(`TensorOps`), `mielin-tensor/src/lib.rs` (`TensorRuntime`) — patterns match
[`examples/e2e-tensor-compute`](../examples/e2e-tensor-compute/src/main.rs) and
[`examples/e2e-full-stack`](../examples/e2e-full-stack/src/main.rs) (`demo_tensor_computation`).

**Prerequisites:** `mielin-tensor` and `mielin-hal` (for capability detection).

```rust
use mielin_hal::capabilities::HardwareProfile;
use mielin_tensor::{Tensor, TensorRuntime};

fn main() {
    // Detect capabilities once, build a runtime around them
    let hw = HardwareProfile::detect();
    let runtime = TensorRuntime::new(hw.capabilities);
    println!("Backend: {}", runtime.acceleration_info());

    // --- Construction ---
    let a = Tensor::vector(vec![1.0, 2.0, 3.0]);
    let b = Tensor::vector(vec![4.0, 5.0, 6.0]);
    let m = Tensor::zeros(vec![256, 256]); // Vec<usize> shape, not a slice

    // --- Arithmetic (inherent methods on Tensor<f32>) ---
    let sum = a.add(&b);      // [5.0, 7.0, 9.0]
    let diff = a.sub(&b);     // [-3.0, -3.0, -3.0]
    let prod = a.mul(&b);     // elementwise: [4.0, 10.0, 18.0]
    let scaled = a.scale(2.0);

    // --- Reductions ---
    println!("sum={} mean={} min={} max={}", a.sum(), a.mean(), a.min(), a.max());

    // --- Hardware-aware ops via TensorRuntime::ops() ---
    // These return Option<Tensor<f32>> / Option<f32> — None on shape mismatch.
    let dot = runtime.ops().dot(&a, &b).expect("shape mismatch");
    let added = runtime.ops().add(&a, &b).expect("shape mismatch");
    let matmul_result = runtime.ops().matmul(&m, &m).expect("shape mismatch");

    println!("dot={dot}, matmul shape={:?}", matmul_result.shape());
    println!("(unused: {:?} {:?} {:?} {:?} {:?})", sum, diff, prod, scaled, added);
}
```

`TensorRuntime::new` doesn't change the numeric algorithm you get by calling `Tensor<f32>`'s
inherent methods (`add`/`sub`/`mul`/`sum`/...) — those always run scalar. The *hardware-aware* path
is `runtime.ops()`, which returns a `TensorOps` built from the `HardwareCapabilities` you passed in
and internally dispatches to SIMD kernels when available (see `mielin-tensor/src/backends/`).

To see multi-backend benchmarking (Scalar / NEON / AVX2 / AVX-512, matmul / conv2d / elementwise /
reductions / NN inference / matrix transpose / quantization), run:

```bash
cargo run -p e2e-tensor-compute
```

This performs real GFLOPS benchmarking across every backend your CPU supports (detected via
`HardwareProfile::detect()`), and can take several minutes in a debug build — use
`cargo run --release -p e2e-tensor-compute` for realistic numbers. The printed table format
(from `BenchmarkResult::display` in the example) looks like:

```
   Backend      | Operation            |  Time (ms) |       GFLOPS
   ------------------------------------------------------------
   Scalar       | MatMul 256x256       |      X.XXXms |     X.XX GFLOPS
   Scalar       | MatMul 512x512       |      X.XXXms |     X.XX GFLOPS
   AVX2         | MatMul 256x256       |      X.XXXms |     X.XX GFLOPS
   ...
```
(illustrative — exact operation list and column widths from `BenchmarkResult::display` in
`examples/e2e-tensor-compute/src/main.rs`; actual numbers depend entirely on your CPU)

**What you learned:**
- `Tensor::zeros`/`ones`/`filled`/`vector`/`matrix` take `Vec<usize>` shapes, **not** `&[usize]`
  slices (the shape passed to `Tensor::zeros(&[2, 3])` shown in the top-level README does not
  compile against the real API — see the "Docs vs. reality" note below).
- Simple arithmetic (`add`/`sub`/`mul`/`scale`/`sum`/`mean`/`min`/`max`/`sqrt`/`exp`/`log`/`pow`/
  `clip`) is available directly on `Tensor<f32>` with no runtime needed.
- `TensorOps` (via `TensorRuntime::ops()`) is the hardware-dispatched path and returns `Option`,
  not `Result` — `None` means a shape mismatch.
- `TensorRuntime::acceleration_info()` gives a human string describing the selected backend
  (`"Arm SVE2..."`, `"Intel AVX2"`, `"Scalar (no SIMD acceleration)"`, ...).

**Next steps:** Tutorial 8 shows quantizing these same tensors to INT8/INT4 for efficient
inference, and how backend selection interacts with quantization.

---

## Tutorial 8: Quantization & Backend Selection

**Goal:** Quantize a tensor to INT8 and INT4, measure the compression ratio, and pick a
`TensorRuntime` backend based on detected hardware.

**Grounded in:** `mielin-tensor/src/quant.rs` (`QuantizedTensor`, `Quant4Tensor`, `QuantScheme`,
`QuantGranularity`) — exactly the API used in
[`examples/e2e-tensor-compute`](../examples/e2e-tensor-compute/src/main.rs)
(`benchmark_quantization`, `benchmark_int4_quantization`) and
[`examples/e2e-full-stack`](../examples/e2e-full-stack/src/main.rs) (`demo_tensor_computation`).

**Prerequisites:** `mielin-tensor`, `mielin-hal`.

```rust
use mielin_hal::capabilities::{HardwareCapabilities, HardwareProfile};
use mielin_tensor::{
    quant::{Quant4Tensor, QuantGranularity, QuantScheme, QuantizedTensor},
    Tensor, TensorRuntime,
};

fn main() {
    let tensor = Tensor::from_vec(vec![1.0f32; 256 * 256], vec![256, 256]).unwrap();

    // --- INT8 quantization (asymmetric, per-tensor) ---
    let q8 = QuantizedTensor::from_tensor(&tensor, QuantScheme::Asymmetric, QuantGranularity::PerTensor);
    let dequantized = q8.dequantize();
    println!(
        "INT8: original={} bytes, quantized={} bytes",
        256 * 256 * 4, // f32
        q8.data().len(), // i8
    );

    // --- INT4 quantization (symmetric) ---
    let q4 = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);
    println!(
        "INT4: compression_ratio={:.1}x, packed_bytes={}",
        q4.compression_ratio(),
        q4.data().len(),
    );
    let _restored = q4.dequantize();
    let _ = dequantized; // silence unused warning in this standalone snippet

    // --- Backend selection driven by detected hardware ---
    let hw = HardwareProfile::detect();
    for (name, caps) in [
        ("Scalar", HardwareCapabilities::NONE),
        ("NEON", HardwareCapabilities::NEON),
        ("AVX2", HardwareCapabilities::AVX2),
        ("AVX-512", HardwareCapabilities::AVX512),
    ] {
        if name == "Scalar" || hw.capabilities.contains(caps) {
            let runtime = TensorRuntime::new(caps);
            println!("{name} available -> {}", runtime.acceleration_info());
        }
    }
}
```

**What you learned:**
- `QuantizedTensor::from_tensor(&tensor, scheme, granularity)` supports
  `QuantScheme::{Symmetric, Asymmetric}` and `QuantGranularity::{PerTensor, PerChannel, ...}`, and
  packs values into `i8`.
- `Quant4Tensor::from_tensor(&tensor, scheme)` packs two INT4 values per byte —
  `compression_ratio()` reports the achieved ratio versus f32.
- `QuantizedTensor::matmul_quant(&other)` lets you multiply two quantized tensors directly without
  fully dequantizing first.
- Real backend availability is data-driven: only enable a `TensorRuntime` backend after checking
  `HardwareProfile::detect().capabilities.contains(...)` — hardcoding a backend can panic or
  silently fall back to scalar on unsupported hardware.

**Next steps:** Tutorial 9 runs actual WASM code (not just host-side tensors) inside a sandboxed
`WasmExecutor`, including the same tensor host functions used above.

---

## Tutorial 9: Running a WASM-Sandboxed Agent

**Goal:** Compile and execute a WASM module inside `WasmExecutor`, call MielinOS's tensor host
functions from WAT, and enforce capability/resource limits with the sandbox APIs.

**Grounded in:** [`examples/tensor-inference/src/main.rs`](../examples/tensor-inference/src/main.rs)
(package `tensor-inference`), `mielin-wasm/src/executor.rs` (`WasmExecutor`),
`mielin-wasm/src/sandbox.rs` (`Sandbox`, `Capability`), `mielin-cells/src/security/sandbox.rs`
(`SandboxConfig`, `SandboxExecutor`, `SandboxViolation`).

**Prerequisites:** `mielin-wasm`, `mielin-cells`, `mielin-hal`, `wat` (for authoring WASM by hand).

### 9a. Execute a WASM agent with hardware-aware tensor host functions

```bash
cargo run -p tensor-inference
```

**Real captured output** (tracing-formatted, backend loop runs Scalar/NEON/SVE2/AVX2 — the executor
doesn't refuse to "run" an unsupported backend, it just won't accelerate; hardware detection inside
the WAT module picks the fastest one it's told is available):

```
MielinOS - Tensor Inference Example
===================================

--- Testing with Scalar backend ---
Agent ID: b76b2bc7-707c-44d5-894e-63d5fb4e6cce
✓ Inference completed with Scalar backend
  Exit code: 0

--- Testing with NEON backend ---
Agent ID: b1af8721-5c0b-4d8a-b3db-1fb2962feb37
✓ Inference completed with NEON backend
  Exit code: 0

--- Testing with SVE2 backend ---
Agent ID: bf8590c5-627b-4a30-87c5-b0844e3045cd
✓ Inference completed with SVE2 backend
  Exit code: 0

--- Testing with AVX2 backend ---
Agent ID: 38032915-16e9-4250-ac27-6165fc19fca1
✓ Inference completed with AVX2 backend
  Exit code: 0

✓ All backends tested successfully!
```

The relevant Rust driving code (trimmed from the example):

```rust
use mielin_cells::Agent;
use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::executor::WasmExecutor;

fn main() -> anyhow::Result<()> {
    let executor = WasmExecutor::with_capabilities(HardwareCapabilities::NEON)?;

    // `agent.dna().binary()` must start with the WASM magic number \0asm
    let wasm_binary = wat::parse_str(r#"
        (module
            (import "mielin" "tensor_zeros" (func $zeros (param i32 i32) (result i32)))
            (import "mielin" "tensor_free" (func $free (param i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "_start")
                ;; shape [2, 2] stored at offset 0 (two i32 dims)
                i32.const 0
                i32.const 2
                i32.store
                i32.const 4
                i32.const 2
                i32.store

                ;; tensor_zeros(shape_ptr=0, shape_len=2) -> handle
                i32.const 0
                i32.const 2
                call $zeros
                call $free
                drop
            )
        )
    "#)?;
    let agent = Agent::new(wasm_binary);

    let result = executor.execute_with_capabilities(&agent, HardwareCapabilities::NEON)?;
    println!("exit_code={}", result.exit_code);
    Ok(())
}
```

`WasmExecutor` exposes a family of `mielin.*` host imports (see the tests at the bottom of
`mielin-wasm/src/lib.rs` for the full catalogue): `tensor_zeros`/`tensor_ones`/`tensor_add`/
`tensor_matmul`/`tensor_free`, `tensor_supports_neon`/`_sve2`/`_avx2`, `time_now_millis`/
`time_monotonic_nanos`, `random_u32`/`random_f32`/`random_bytes`, and `process_platform`/
`process_arch`/`process_cpu_count`/`process_pointer_bits`.

### 9b. Enforce capabilities and resource limits

Two complementary sandboxing layers exist. `mielin_wasm::sandbox::Sandbox` is a minimal
capability allow-list:

```rust
use mielin_wasm::sandbox::{Capability, Sandbox};

fn main() {
    let mut sandbox = Sandbox::new();
    assert!(!sandbox.has_capability(&Capability::Camera));

    sandbox.grant(Capability::Camera);
    assert!(sandbox.has_capability(&Capability::Camera));
    // Capability is one of: FileSystem, Network, Camera, Gpio
}
```

`mielin_cells::security::sandbox::{SandboxConfig, SandboxExecutor}` is a much richer resource-quota
sandbox (memory, CPU time, file descriptors, network connections, syscall blocklist, path
allow-lists) used for agent-level policy enforcement:

```rust
use mielin_cells::{SandboxConfig, SandboxExecutor};
use mielin_cells::security::attestation::Capability;

fn main() {
    // `SandboxConfig::minimal()` denies almost everything; `permissive()` is dev-friendly.
    let config = SandboxConfig::minimal();
    let mut executor = SandboxExecutor::new(config);

    // Capability checks
    match executor.check_capability(&Capability::Network) {
        Ok(()) => println!("network allowed"),
        Err(violation) => println!("denied: {violation}"),
    }

    // Resource quota checks (call these as the agent consumes resources)
    if let Err(v) = executor.check_memory(128 * 1024 * 1024) {
        println!("sandbox violation: {v}"); // minimal() caps memory at 64 MB
    }

    println!("violations so far: {}", executor.violation_count());
}
```

**What you learned:**
- `WasmExecutor::new()` / `WasmExecutor::with_capabilities(caps)` build a `wasmtime::Engine` +
  `Linker` pre-registered with MielinOS's host functions (`HostFunctions::register`).
  `execute_with_capabilities` validates the WASM magic number, compiles, instantiates, and calls
  `_start` (falling back to `main`) if present.
- The `HardwareCapabilities` you pass to `with_capabilities`/`execute_with_capabilities` do **not**
  change the actual instruction set your machine executes — they control what the
  `tensor_supports_*` host functions report to the guest WASM, letting you unit-test
  hardware-conditional guest logic without owning the hardware.
- There are two separate sandbox types with different scopes:  `mielin_wasm::sandbox::Sandbox`
  (simple 4-capability allow-list) and `mielin_cells::security::sandbox::SandboxExecutor` (full
  resource-quota + syscall + path enforcement with a `SandboxViolation` audit trail). Pick based on
  whether you need just capability gating or full resource accounting.

**Next steps:** Tutorial 10 covers the hardware detection layer (`mielin-hal`) that feeds
`HardwareCapabilities` into both the tensor runtime and the WASM executor.

---

## Tutorial 10: Hardware Capability Detection with mielin-hal

**Goal:** Query architecture, SIMD capability flags, core count, memory size, and cache topology at
runtime, with zero `unsafe` in your own code.

**Grounded in:** `mielin-hal/src/lib.rs` (`Architecture`, `detect_architecture`),
`mielin-hal/src/capabilities.rs` (`HardwareCapabilities`, `HardwareProfile`) — used throughout
[`examples/e2e-full-stack`](../examples/e2e-full-stack/src/main.rs) (`demo_hal_integration`) and
[`examples/agent-migration`](../examples/agent-migration/src/main.rs) (Phase 2).

**Prerequisites:** `mielin-hal` only — it's `#![no_std]`, so it works on embedded targets too.

```rust
use mielin_hal::{detect_architecture, Architecture};
use mielin_hal::capabilities::{HardwareCapabilities, HardwareProfile};

fn main() {
    let arch = detect_architecture();
    println!("Architecture: {}", arch); // Display impl: "x86_64", "aarch64", ...

    // HardwareProfile::detect() is cached after the first call (see doc comment on
    // HardwareProfile::detect in mielin-hal/src/capabilities.rs) — cheap to call repeatedly.
    let profile = HardwareProfile::detect();
    println!("Cores: {}", profile.core_count);
    println!("Memory: {} MB", profile.memory_size / 1024 / 1024);
    println!("L1/L2/L3: {}/{}/{} KB",
        profile.l1_cache_size / 1024, profile.l2_cache_size / 1024, profile.l3_cache_size / 1024);

    println!("Has SIMD: {}", profile.has_simd());
    println!("Has SVE2: {}", profile.has_sve2());
    println!("Has NPU: {}", profile.has_npu());
    println!("Supports tensor ops (SVE2/NPU/AVX512/SME): {}", profile.supports_tensor_ops());
    println!("Max vector width: {} bits", profile.max_vector_width());

    // Fine-grained flags via the HardwareCapabilities bitflags
    if profile.capabilities.contains(HardwareCapabilities::AVX2) {
        println!("AVX2 available for 256-bit SIMD");
    }

    match arch {
        Architecture::AArch64 => println!("ARM 64-bit — potential SIMD: NEON, SVE, SVE2, SME"),
        Architecture::X86_64 => println!("x86 64-bit — potential SIMD: SSE, AVX, AVX2, AVX-512"),
        Architecture::RiscV64 => println!("RISC-V 64-bit — potential SIMD: RVV"),
        Architecture::ArmCortexM | Architecture::CortexM => println!("Cortex-M — Helium on M55+"),
        _ => println!("Other architecture: {arch}"),
    }
}
```

Real output on the machine this document was written on (x86_64, 16 cores — captured from
`agent-migration`'s Phase 2, which calls the same API):

```
  Architecture: x86_64
  CPU Cores: 16
  SIMD Support: true
  Tensor Ops: false
  Max Vector Width: 256 bits
```

`Tensor Ops: false` here is expected and correct: `supports_tensor_ops()` only returns `true` for
SVE2, NPU, AVX-512, or SME — this particular CPU has AVX2 (256-bit) but not AVX-512.

**What you learned:**
- `Architecture` has 9 variants covering AArch64, RISC-V64, x86_64/x86 (32-bit), ARM Cortex-M,
  ARM32, LoongArch64, and Xtensa (ESP32) — `detect_architecture()` picks one via
  `#[cfg(target_arch = ...)]`.
- `HardwareCapabilities` is a `bitflags!` type — check membership with `.contains(...)` and
  overlap with `.intersects(...)`.
- `HardwareProfile::detect()` caches its result in atomics after the first call;
  `HardwareProfile::invalidate_cache()` forces re-detection (useful for hot-plug/hypervisor
  scenarios).
- `mielin-hal` also has `platform` (Raspberry Pi/STM32/ESP32/BeagleBone/Jetson detection), `power`,
  `pmu`, `gpu`, `accelerator`, `virtualization`, `devicetree`, and `acpi` modules for deeper
  platform introspection — see the crate-level doc comment in `mielin-hal/src/lib.rs`.

**Next steps:** Tutorial 11 shows this same `HardwareProfile` feeding into a real multi-node mesh
cluster over QUIC.

---

## Tutorial 11: Starting a Mesh Node / Small Cluster

**Goal:** Stand up MielinOS mesh nodes with QUIC transport, discovery, gossip, and agent migration,
both as a standalone binary and via the `mielinctl` control-plane CLI.

**Grounded in:** [`examples/mesh-cluster/src/main.rs`](../examples/mesh-cluster/src/main.rs)
(package `mesh-cluster`, binary `node`),
[`examples/e2e-mesh-cluster/src/main.rs`](../examples/e2e-mesh-cluster/src/main.rs),
`mielin_mesh_core::{Node, NodeRole, service::{MeshConfig, MeshService}}`,
`mielin_mesh_wire::transport::QuicTransport`, and `mielin-cli/src/cli.rs`
(`Commands::Daemon`, `mesh::MeshCommands`, `node::NodeCommands`).
Cross-reference: [`docs/CLUSTER_TESTING.md`](./CLUSTER_TESTING.md) for a full Docker 3-node setup.

**Prerequisites:** `mielin-mesh-core`, `mielin-mesh-wire`, `mielin-cells`, `mielin-hal`, `tokio`,
`clap`, `tracing`.

### 11a. Two-node cluster with the `mesh-cluster` example

Start a core node first (it will bind to an OS-assigned port because `--port 0` isn't set here —
pass an explicit `-p` to make it discoverable):

```bash
# terminal 1: core node, agent-bearing
cargo run -p mesh-cluster -- --role core --port 9000 --agent

# terminal 2: edge node, connects to the core node and migrates its agent there
cargo run -p mesh-cluster -- --role edge --port 9001 --connect 127.0.0.1:9000 --agent --migrate-to 127.0.0.1:9000
```

The `Args` struct (from the example) accepts:

| Flag | Meaning |
|------|---------|
| `-r, --role <edge\|relay\|core>` | Node role (required) |
| `-p, --port <PORT>` | Bind port (default `0` = OS-assigned) |
| `-c, --connect <ip:port>` | Peer(s) to send a discovery message to (repeatable) |
| `-a, --agent` | Create a sample agent on this node at startup |
| `-m, --migrate-to <ip:port>` | Migrate this node's first agent to the given peer |
| `--use-certs` | Enable `CertManager`-issued TLS certificates on the QUIC transport |

Each node runs an integrated `MeshService` (`mielin_mesh_core::service::MeshService`) providing
discovery, gossip, an agent registry, and migration coordination, over a `QuicTransport`
(`mielin_mesh_wire::transport::QuicTransport`). Internally, a node is built roughly like this
(trimmed from `MeshNode::new` in the example):

```rust
use mielin_mesh_core::{discovery::BootstrapNode, service::{MeshConfig, MeshService}, Node, NodeRole};
use mielin_mesh_wire::transport::QuicTransport;
use std::net::SocketAddr;

async fn start_node(role: NodeRole, bind_port: u16) -> anyhow::Result<()> {
    let bind_addr: SocketAddr = format!("127.0.0.1:{bind_port}").parse()?;
    let transport = QuicTransport::new(bind_addr).await?;
    let actual_addr = transport.local_addr()?;

    let node = std::sync::Arc::new(Node::new(role));
    let config = MeshConfig {
        bind_address: actual_addr,
        bootstrap_nodes: Vec::<BootstrapNode>::new(),
        enable_mdns: false,
        enable_gossip: true,
        enable_registry: true,
        enable_migration: true,
    };

    let mut mesh_service = MeshService::new(node, config)?;
    mesh_service.start().await?; // spins up gossip + registry + migration coordinator
    println!("Node ID: {}", mesh_service.node_id());
    Ok(())
}
```

For a hardened variant with TLS certificate rotation, SWIM-style health monitoring, and status
reporting, see [`examples/e2e-mesh-cluster`](../examples/e2e-mesh-cluster/src/main.rs):

```bash
cargo run -p e2e-mesh-cluster -- --role core --port 9000
# in another terminal:
cargo run -p e2e-mesh-cluster -- --role edge --port 9001 --connect 127.0.0.1:9000 --use-tls
```

Both `mesh-cluster` and `e2e-mesh-cluster` run a `run_server()` loop that never returns
(`Ctrl+C` to stop) — appropriate output includes lines like `🚀 Edge node started on 127.0.0.1:9001`,
`📋 Node ID: ...`, `✅ Mesh service started (gossip + registry + migration)`, and (after connecting)
a `📊 Mesh Service Status:` block with gossip alive/suspect/dead counts, local agent count, and
migration stats — see `MeshNode::show_mesh_status` in the example for the exact fields.

### 11b. Control-plane CLI (`mielinctl`)

`mielin-cli` builds a binary named `mielinctl` (see `[[bin]] name = "mielinctl"` in
`mielin-cli/Cargo.toml`). To start a daemon and inspect it:

```bash
cargo run -p mielin-cli -- daemon --listen 0.0.0.0:9000 --role edge --control-listen 127.0.0.1:8081
```

```bash
# in another terminal — talk to the daemon's HTTP control-plane API
cargo run -p mielin-cli -- mesh status --daemon 127.0.0.1:8081
cargo run -p mielin-cli -- mesh peers --daemon 127.0.0.1:8081
cargo run -p mielin-cli -- node list
cargo run -p mielin-cli -- node create --role edge
cargo run -p mielin-cli -- node join 192.168.1.100:9000
cargo run -p mielin-cli -- agent deploy ./my-agent.wasm --node <node-id>
cargo run -p mielin-cli -- agent migrate <agent-id> <target-node>
```

Be aware of the current CLI behavior when reading `mielin-cli/src/commands/*.rs`: `mesh status`
and `mesh peers` accept an optional `--daemon <addr>` and, when given, call the daemon's real HTTP
control-plane API (`mielin-cli/src/control/client.rs`); without `--daemon` they print illustrative
mock fixtures (`mock_mesh_status()`/`mock_peer_list()`). `node list` and `agent list` currently
**always** print mock fixtures (`mock_node_list()`/`mock_agent_list()` in
`mielin-cli/src/commands/node.rs` / `agent.rs`) regardless of flags — useful for exploring the
output shape, but not yet wired to a live daemon for those two subcommands.

**What you learned:**
- The binary produced by the `mielin-cli` package is called `mielinctl`, not `mielin-cli`.
- `MeshService::new(node, MeshConfig) -> Result<MeshService, MeshError>` plus `.start().await` is
  the one-stop-shop for discovery + gossip + registry + migration; `mesh_service.node_id()`,
  `get_member_stats()`, `local_agent_count()`, and `get_migration_stats()` give you introspection.
- `QuicTransport::new(addr)` / `QuicTransport::new_with_certs(addr, node_id, cert_manager)` differ
  only in whether a `CertManager`-issued certificate secures the QUIC/TLS handshake.
- The real top-level daemon command is `mielinctl daemon --listen <addr> --role <role> --bootstrap
  <addr>` — there is **no** `mielinctl mesh start` subcommand (`MeshCommands` only has `Status`,
  `Peers`, `Gossip`, `Dht`); if you've seen `mesh start`/`mesh join` referenced elsewhere in this
  repo's docs, treat `daemon` + `node join` as the current, real equivalent.

**Next steps:** [`docs/CLUSTER_TESTING.md`](./CLUSTER_TESTING.md) walks through the same 3-node
topology as a Docker Compose cluster with health checks. [`docs/NETWORKING.md`](./NETWORKING.md)
and [`docs/PROTOCOL.md`](./PROTOCOL.md) cover the wire protocol in depth.

---

## Tutorial 12: Embedded/IoT Runtime with mielin-rt

**Goal:** Run MielinOS's embedded runtime profile on a simulated battery-powered sensor node: power
mode transitions, battery-triggered migration, and (in the advanced version) energy profiling and
power-anomaly detection.

**Grounded in:** [`examples/embedded-iot/src/main.rs`](../examples/embedded-iot/src/main.rs)
(package `embedded-iot`, binary `sensor-node`) and
[`examples/e2e-embedded-iot/src/main.rs`](../examples/e2e-embedded-iot/src/main.rs),
`mielin-rt/src/lib.rs` (`EmbeddedRuntime`), `mielin-rt/src/power.rs` (`PowerMode`,
`BatteryStatus`), `mielin-rt/src/energy.rs` (`EnergyProfiler`), `mielin-rt/src/measurement.rs`
(`MeasurementConfig`, `PowerMeasurement`), `mielin-rt/src/config.rs` (`ConfigPreset`).

**Prerequisites:** `mielin-rt`, `mielin-cells`, `mielin-hal`.

### 12a. Basic sensor node

```bash
cargo run -p embedded-iot
```

**Real captured output** (abridged — full run has 7 battery-lifecycle scenarios):

```
═══════════════════════════════════════════
  MielinOS Embedded IoT Sensor Node
  Simulated Temperature Monitoring Device
═══════════════════════════════════════════

✅ Runtime initialized
   Architecture: X86_64

🤖 Temperature sensor agent created
   Agent ID: 191a291f-8158-4617-9a8d-17cd80305390

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📊 Battery Update:
   Level: 100%
   Charging: No
   Status: Full battery, normal operation
   ⚡ Power Mode: Normal
...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📊 Battery Update:
   Level: 15%
   Charging: No
   Status: Critical battery, sleep mode + migration trigger
   🚨 MIGRATION TRIGGER ACTIVATED!
   → Agent will migrate to preserve battery
   → Target: Nearest relay node with power
   → Agent ID: 191a291f-8158-4617-9a8d-17cd80305390
   ⚡ Power Mode: Sleep
   💤 Entering low power wait state...
...
📈 Battery Lifecycle Summary:
   Final battery level: 80%
   Migration was triggered: false
   Current power mode: Normal

═══════════════════════════════════════════
  Simulation Complete
═══════════════════════════════════════════
```

The core API, distilled:

```rust
use mielin_cells::Agent;
use mielin_rt::{power::*, EmbeddedRuntime, RuntimeError};

fn main() -> Result<(), RuntimeError> {
    let mut runtime = EmbeddedRuntime::new();
    runtime.init()?;
    println!("Architecture: {:?}", runtime.architecture());

    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    runtime.update_battery(BatteryStatus { level_percent: 15, is_charging: false });

    if runtime.should_migrate() {
        println!("Migration trigger for agent {} — battery critical", agent.id());
        runtime.set_power_mode(PowerMode::Sleep);
        runtime.low_power_wait(); // WFI on Cortex-M, no-op elsewhere
    }

    // Charging cancels the migration trigger even at the same battery level
    runtime.update_battery(BatteryStatus { level_percent: 15, is_charging: true });
    assert!(!runtime.should_migrate());

    Ok(())
}
```

### 12b. Advanced: energy profiling and power measurement

[`examples/e2e-embedded-iot`](../examples/e2e-embedded-iot/src/main.rs) builds on the same
`EmbeddedRuntime` but adds `EnergyProfiler`, `PowerMonitor`-backed measurement with anomaly
detection, and a `ConfigPreset`-based constructor:

```rust
use mielin_rt::{
    config::ConfigPreset,
    energy::EnergyProfiler,
    measurement::{MeasurementConfig, PowerMeasurement},
    power::{BatteryStatus, PowerMode},
    EmbeddedRuntime, RuntimeError,
};

fn main() -> Result<(), RuntimeError> {
    // ConfigPreset: Default, LowPower, HighPerformance, Minimal, MaximumMonitoring
    let mut runtime = EmbeddedRuntime::from_preset(ConfigPreset::LowPower)?;
    let energy_profiler = EnergyProfiler::new();

    runtime.init()?;
    runtime.enable_power_monitoring(MeasurementConfig::default().with_sample_rate_hz(100));

    // Feed in a measurement (voltage in mV, current in mA, timestamp in some monotonic unit)
    let measurement = PowerMeasurement::new(3700, 50, 1000);
    if let Some(anomaly) = runtime.record_power_measurement(measurement) {
        println!("Power anomaly detected: {anomaly:?}");
    }

    if let Some(stats) = runtime.power_statistics() {
        println!("samples={} avg_power_mw={:.2}", stats.sample_count, stats.avg_power_mw);
    }

    let summary = energy_profiler.task_summary();
    println!("tracked tasks: {}", summary.task_count);

    Ok(())
}
```

Run the full 30-second simulation (real elapsed time, since it uses `std::thread::sleep`):

```bash
cargo run -p e2e-embedded-iot
```

It prints an initialization banner, periodic `📊 Status:` blocks with battery/power-mode readings,
and a final `📊 IoT Sensor Node Statistics` block with battery, power (`avg_voltage_mv`,
`avg_current_ma`, `avg_power_mw`, `sample_count`), and energy-profile sections — see
`IoTSensorNode::display_statistics` in the example for exact field names (illustrative shape below,
since the 30-second wall-clock run wasn't executed for this document):

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📊 IoT Sensor Node Statistics
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🔋 Battery:
   Level: NN%
   Power mode: <PowerMode>
   Migration trigger: <bool>
⚡ Power Statistics:
   Samples: N
   Avg voltage: N.NNV
   Avg current: N.NNmA
   Avg power: N.NNmW
📈 Energy Profile:
   Tracked tasks: N
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**What you learned:**
- `EmbeddedRuntime` is `#![no_std]`-compatible and works identically whether the target is a full
  OS (for local testing, as above) or bare-metal Cortex-M/RISC-V (`low_power_wait()` compiles to a
  real `WFI` instruction only under `#[cfg(target_arch = "arm")]`).
- `BatteryStatus.is_charging` overrides the migration trigger even below the critical threshold —
  charging always means "don't migrate."
- `EmbeddedRuntime::from_preset(ConfigPreset::...)` is the fastest way to get a sane starting
  configuration (`LowPower`, `HighPerformance`, `Minimal`, `MaximumMonitoring`, `Default`); it
  internally validates the config and returns `RuntimeError::InvalidConfig` on inconsistent
  thresholds (e.g. `battery_threshold_critical >= battery_threshold_low`).
- `record_power_measurement` returns `Option<PowerAnomaly>` — check it on every sample if you want
  anomaly-driven alerts rather than polling `power_statistics()`.

---

## Docs vs. reality

While preparing this guide, two places where existing docs disagree with the actual compiling API
were found. This tutorial series always follows the **real code** (verified by building/running
the examples above); if you spot the same pattern elsewhere, prefer the crate source over prose:

- **`README.md`**'s "Basic usage" and "Creating an Agent" snippets are aspirational, not current
  API: `AgentId::new()` doesn't exist (`AgentId` is a `Uuid` type alias — get one via
  `Agent::new(wasm).id()`); `Tensor::zeros(&[2, 3])` doesn't compile (`Tensor::zeros` takes
  `Vec<usize>`, not `&[usize]`); `impl Agent for MyAgent { fn on_message(...) }` doesn't compile
  either — `Agent` is a concrete `struct`, not a trait, and has no `on_message`/`on_migrate`
  methods. `Tensor::randn(&[1024, 1024])` also doesn't exist on `Tensor<f32>` (no RNG-based
  constructor is exposed at all; `Tensor::zeros`/`ones`/`filled`/`from_vec` are the real
  constructors). The README's `cargo run -p mielin-cli -- mesh start --bind 0.0.0.0:9000` /
  `mesh join ...` commands also don't exist — see Tutorial 11 for the real `mielinctl daemon` /
  `node join` equivalents.
- **`QUICKSTART.md`**'s sample `hello-agent` output block (`"Creating a simple agent..."` /
  `Agent ID: 1b4e28ba-...`) doesn't match what the example actually prints — see Tutorial 1 for
  the real, captured output (`"MielinOS - Hello Agent Example"` / `"Created agent with ID: ..."` /
  `"Agent state: ..."` / `"Agent DNA hash: ..."`). The rest of QUICKSTART.md's "Your First Agent"
  snippet (`Agent::new(wasm)`, `.id()`, `.state()`) is accurate.
- For cluster deployment guidance, see [`docs/DEPLOYMENT.md`](./DEPLOYMENT.md); the Docker Compose
  3-node topology walkthrough also lives in [`docs/CLUSTER_TESTING.md`](./CLUSTER_TESTING.md) —
  both are linked below.

---

## Where to go next

- [`../QUICKSTART.md`](../QUICKSTART.md) — project setup, build/test/lint workflow, and the
  overall crate map.
- [`./MIGRATION.md`](./MIGRATION.md) — the full agent migration protocol reference (state machine,
  wire format, failure/rollback semantics) that Tutorials 6 and 11 only summarize.
- [`./CLUSTER_TESTING.md`](./CLUSTER_TESTING.md) — Docker-based 3-node cluster testing (the
  deployment-oriented companion to Tutorial 11).
- [`../examples/`](../examples/) — every example referenced above, plus
  `examples/e2e-full-stack` (mesh + tensor + migration + HAL + kernel scheduler in one binary,
  `cargo run -p e2e-full-stack`) which is a good "read the whole thing" capstone once you've been
  through this series.
- [`./ARCHITECTURE.md`](./ARCHITECTURE.md), [`./API.md`](./API.md), [`./PROTOCOL.md`](./PROTOCOL.md),
  [`./NETWORKING.md`](./NETWORKING.md), [`./PERFORMANCE.md`](./PERFORMANCE.md) — deeper reference
  material for each subsystem touched on above.
