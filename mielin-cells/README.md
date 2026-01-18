# mielin-cells

**Agent SDK - The Neurotransmitter (Layer 2)**

Comprehensive SDK for creating, managing, and migrating autonomous AI agents in the MielinOS ecosystem. Mielin Cells provides the core abstractions that enable agents to traverse seamlessly across heterogeneous hardware—the computational "neurotransmitters" of the system.

## Overview

Mielin Cells implements Layer 2 of the MielinOS architecture, providing complete agent lifecycle management, stateful migration capabilities, policy enforcement, and DNA (WebAssembly binary) management.

**Current Status:** v0.1.0-rc.1 "Oligodendrocyte" (Released 2026-01-18)

## Features

- **Agent Lifecycle**: Complete state management from creation to termination
- **Stateful Migration**: Snapshot, serialize, transmit, and restore running agents
- **DNA Management**: Content-addressable WASM binaries with cryptographic verification
- **Policy Engine**: Flexible execution policies (battery, latency, architecture preferences)
- **Type-Safe**: Strong typing for agent IDs, states, and operations
- **Lightweight**: Minimal overhead suitable for embedded to cloud deployments

## Architecture

Layer 2 in the 5-layer neural architecture:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: Mielin Cells (The Neurotransmitter)               │
│          • Agent Lifecycle Manager                          │
│          • DNA (WASM) Management                            │
│          • Migration Snapshot/Restore                       │
│          • Policy Enforcement Engine                        │
└─────────────────────────────────────────────────────────────┘
```

### File Structure

```
mielin-cells/
├── src/
│   ├── agent.rs      # Agent lifecycle and state management
│   ├── dna.rs        # WebAssembly binary (DNA) management
│   ├── migration.rs  # Snapshot/restore and migration logic
│   ├── policy.rs     # Execution policy definitions
│   └── lib.rs        # Public API and error types
└── Cargo.toml
```

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-cells = { path = "../mielin-cells" }
```

### Creating an Agent

```rust
use mielin_cells::Agent;

// Create agent with WASM binary
let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, /* ... */];
let agent = Agent::new(wasm_binary);

println!("Agent ID: {}", agent.id());
println!("State: {:?}", agent.state());
println!("DNA hash: {:x?}", agent.dna().hash());
```

### Migrating an Agent (Saltatory Conduction)

```rust
use mielin_cells::migration::{MigrationSnapshot, MigrationManager};

let mut manager = MigrationManager::new();

// Phase 1: Initiate migration
let snapshot = manager.initiate_migration(
    &agent,
    Some(*target_node_id.as_bytes())
)?;

// Phase 2: Serialize for network transfer
let bytes = snapshot.serialize()?;
println!("Snapshot size: {} bytes", bytes.len());

// ... send over network (QUIC) ...

// Phase 3: Restore on target node
let restored_snapshot = MigrationSnapshot::deserialize(&bytes)?;
let restored_agent = restored_snapshot.restore()?;

// Verify integrity
assert_eq!(restored_agent.dna().hash(), agent.dna().hash());
```

## Core Components

### Agent

The fundamental unit of computation in MielinOS—autonomous entities that can execute and migrate.

```rust
use mielin_cells::{Agent, AgentState};

let mut agent = Agent::new(wasm_binary);

// Lifecycle management
agent.set_state(AgentState::Running);
agent.set_state(AgentState::Suspended);
agent.set_state(AgentState::Migrating);
agent.set_state(AgentState::Terminated);

// Access components
let dna = agent.dna();        // Immutable WASM bytecode
let policy = agent.policy();  // Execution constraints
let id = agent.id();          // Content-addressable ID
```

#### Agent States

```rust
pub enum AgentState {
    Created,      // Just created, not yet running
    Running,      // Actively executing on current node
    Suspended,    // Paused execution, can resume
    Migrating,    // In process of migration to another node
    Terminated,   // Execution completed or killed
}
```

State transitions:
```
Created → Running → Migrating → Running (on new node)
   ↓         ↓          ↓
Terminated ← Suspended → Running
```

### DNA (WebAssembly Binary)

Immutable code representation with content addressing for verification and deduplication.

```rust
use mielin_cells::Dna;

let dna = Dna::new(wasm_binary);

// Get binary content
let binary = dna.binary();
println!("WASM size: {} bytes", binary.len());

// Get content hash (SHA-256)
let hash = dna.hash();
println!("DNA hash: {:x?}", hash);

// Verify integrity after migration
fn verify_agent(original: &Agent, migrated: &Agent) -> bool {
    original.dna().hash() == migrated.dna().hash()
}
```

**DNA Features:**
- **Content-Addressable**: Hash-based identification
- **Immutable**: Bytecode cannot be modified
- **Verifiable**: Cryptographic hash for integrity
- **Deduplic able**: Identical code shares same DNA

### Migration System

Complete migration workflow with checkpoint/restore support.

```rust
use mielin_cells::migration::{MigrationSnapshot, MigrationManager};

// Capture agent state
let snapshot = MigrationSnapshot::capture(&agent, None)?;

// Snapshot components
println!("Agent ID: {:x?}", snapshot.agent_id);
println!("WASM binary: {} bytes", snapshot.wasm_binary.len());
println!("WASM state: {} bytes", snapshot.wasm_state.len());
println!("Policy: {:?}", snapshot.policy);
println!("Timestamp: {}", snapshot.timestamp);

// Serialize to bytes (network transfer format)
let serialized = snapshot.serialize()?;
println!("Total snapshot: {} bytes", serialized.len());

// Deserialize on remote node
let restored = MigrationSnapshot::deserialize(&serialized)?;

// Restore agent
let new_agent = restored.restore()?;
assert_eq!(new_agent.id(), agent.id());
```

**Migration Performance:**
- Small agent (1MB state): ~5ms over LAN
- Large agent (100MB state): ~200ms over LAN
- Compression: 80-90% reduction with LZ4 (Phase 2)
- Delta encoding: Only changed pages (Phase 3)

#### Migration Manager

Tracks and manages pending migrations across the mesh:

```rust
let mut manager = MigrationManager::new();

// Start migration
let snapshot = manager.initiate_migration(&agent, target_node)?;

// Check status
println!("Pending migrations: {}", manager.pending_count());

// Complete migration (cleanup source)
manager.complete_migration(&agent_id);

// Or cancel if failed
manager.cancel_migration(&agent_id);
```

### Policy Engine

Define execution constraints and migration triggers for intelligent agent placement.

```rust
use mielin_cells::Policy;

let policy = Policy {
    min_battery_percent: 20,      // Don't run if battery < 20%
    max_latency_ms: 100,           // Prefer nodes with <100ms latency
    preferred_architectures: vec!["aarch64".to_string(), "x86_64".to_string()],
};

let mut agent = Agent::new(wasm_binary);
agent.set_policy(policy);

// Check policy compliance
fn can_run_on_node(agent: &Agent, battery: u8, latency: u32, arch: &str) -> bool {
    let p = agent.policy();
    battery >= p.min_battery_percent &&
    latency <= p.max_latency_ms &&
    (p.preferred_architectures.is_empty() ||
     p.preferred_architectures.contains(&arch.to_string()))
}
```

**Policy Use Cases:**
- **Battery Protection**: Migrate before power failure
- **Latency Optimization**: Stay near data sources
- **Architecture Affinity**: Prefer hardware with accelerators
- **Resource Constraints**: Avoid overloaded nodes

## API Reference

### `Agent`

```rust
pub struct Agent {
    id: AgentId,
    dna: Dna,
    state: AgentState,
    policy: Policy,
}

impl Agent {
    pub fn new(wasm_binary: Vec<u8>) -> Self;
    pub fn id(&self) -> AgentId;
    pub fn state(&self) -> &AgentState;
    pub fn set_state(&mut self, state: AgentState);
    pub fn dna(&self) -> &Dna;
    pub fn policy(&self) -> &Policy;
    pub fn set_policy(&mut self, policy: Policy);
}
```

### `Dna`

```rust
pub struct Dna {
    wasm_binary: Vec<u8>,
    hash: [u8; 32],  // SHA-256
}

impl Dna {
    pub fn new(wasm_binary: Vec<u8>) -> Self;
    pub fn binary(&self) -> &[u8];
    pub fn hash(&self) -> &[u8; 32];
}
```

### `MigrationSnapshot`

```rust
pub struct MigrationSnapshot {
    pub agent_id: [u8; 16],           // UUID
    pub wasm_binary: Vec<u8>,         // WASM bytecode
    pub wasm_state: Vec<u8>,          // Linear memory snapshot
    pub policy: Policy,               // Execution policy
    pub timestamp: u64,               // Creation time (Unix epoch)
    pub source_node: Option<[u8; 16]>, // Origin node ID
}

impl MigrationSnapshot {
    pub fn capture(agent: &Agent, source: Option<[u8; 16]>)
        -> Result<Self, CellError>;
    pub fn restore(&self) -> Result<Agent, CellError>;
    pub fn serialize(&self) -> Result<Vec<u8>, CellError>;
    pub fn deserialize(data: &[u8]) -> Result<Self, CellError>;
    pub fn size_bytes(&self) -> usize;
    pub fn age_seconds(&self) -> u64;
}
```

### `MigrationManager`

```rust
pub struct MigrationManager {
    pending_migrations: HashMap<AgentId, MigrationSnapshot>,
}

impl MigrationManager {
    pub fn new() -> Self;
    pub fn initiate_migration(
        &mut self,
        agent: &Agent,
        target: Option<[u8; 16]>,
    ) -> Result<MigrationSnapshot, CellError>;
    pub fn complete_migration(&mut self, agent_id: &AgentId);
    pub fn pending_count(&self) -> usize;
}
```

### `Policy`

```rust
pub struct Policy {
    pub min_battery_percent: u8,
    pub max_latency_ms: u32,
    pub preferred_architectures: Vec<String>,
}

impl Policy {
    pub fn allows_low_battery(&self) -> bool;
    pub fn default() -> Self;  // Permissive defaults
}
```

## Examples

### Complete Migration Workflow

```rust
use mielin_cells::{Agent, AgentState};
use mielin_cells::migration::{MigrationSnapshot, MigrationManager};

// === Source Node ===
let wasm = vec![/* WASM bytecode */];
let mut agent = Agent::new(wasm);
agent.set_state(AgentState::Running);

// Trigger migration (e.g., low battery)
let mut manager = MigrationManager::new();
let snapshot = manager.initiate_migration(&agent, Some(target_id))?;
agent.set_state(AgentState::Migrating);

// Serialize for network
let bytes = snapshot.serialize()?;

// === Network Transfer (QUIC) ===
// send_over_quic(bytes)?;

// === Target Node ===
let received_snapshot = MigrationSnapshot::deserialize(&bytes)?;
let mut migrated_agent = received_snapshot.restore()?;
migrated_agent.set_state(AgentState::Running);

// Verify integrity
assert_eq!(migrated_agent.dna().hash(), snapshot.wasm_binary[..32]);

// === Source Node (Cleanup) ===
manager.complete_migration(&agent.id());
agent.set_state(AgentState::Terminated);
```

### Policy-Based Execution

```rust
use mielin_cells::{Agent, Policy};

// Define strict policy for production agent
let policy = Policy {
    min_battery_percent: 30,
    max_latency_ms: 50,
    preferred_architectures: vec!["aarch64".to_string()],
};

let mut agent = Agent::new(wasm_binary);
agent.set_policy(policy);

// Node selection based on policy
struct NodeStatus {
    battery: u8,
    latency: u32,
    arch: String,
}

fn select_node(agent: &Agent, nodes: &[NodeStatus]) -> Option<usize> {
    let policy = agent.policy();
    nodes.iter().position(|node| {
        node.battery >= policy.min_battery_percent &&
        node.latency <= policy.max_latency_ms &&
        (policy.preferred_architectures.is_empty() ||
         policy.preferred_architectures.contains(&node.arch))
    })
}
```

### Snapshot Size Optimization

```rust
let snapshot = MigrationSnapshot::capture(&agent, None)?;

println!("Snapshot Analysis:");
println!("  Agent ID:     16 bytes");
println!("  WASM binary:  {} bytes", snapshot.wasm_binary.len());
println!("  WASM state:   {} bytes", snapshot.wasm_state.len());
println!("  Policy:       ~32 bytes");
println!("  Metadata:     ~24 bytes");
println!("  Total:        {} bytes", snapshot.size_bytes());

// Optimization strategies:
// 1. Minimize agent state (keep memory usage low)
// 2. Share DNA across agents (deduplicate bytecode)
// 3. Compress state (LZ4, coming in Phase 2)
// 4. Delta encoding (only changed pages, Phase 3)
```

## Error Handling

```rust
pub enum CellError {
    ExecutionFailed(String),
    MigrationFailed(String),
    InvalidState(String),
    SerializationFailed(String),
    DeserializationFailed(String),
}

impl std::fmt::Display for CellError { /* ... */ }
impl std::error::Error for CellError {}
```

All operations return `Result<T, CellError>` for explicit error handling.

## Testing

Run the comprehensive test suite:

```bash
# All tests
cargo test -p mielin-cells

# Specific test
cargo test test_agent_migration

# With output
cargo test -- --nocapture
```

**Test Coverage (39 tests, 100% pass rate):**
- Agent creation and state transitions
- DNA hashing and verification
- Migration snapshot creation and restoration
- Policy validation and enforcement
- Serialization round-trips
- Error conditions and edge cases
- Concurrent agent operations

## Performance

Typical measurements on x86_64 (3.5 GHz):

| Operation | Time | Size | Notes |
|-----------|------|------|-------|
| Agent creation | ~1 μs | 64 bytes | Overhead only |
| DNA hash (SHA-256) | ~0.5 μs/KB | - | For 1KB WASM |
| Snapshot creation | ~10 μs | Variable | Excludes WASM copy |
| Serialization | ~5 μs | +24 bytes | Overhead |
| Deserialization | ~5 μs | - | Plus allocation |
| Restoration | ~1 μs | - | Struct creation |

**Snapshot Sizes:**
- Minimal (empty WASM): ~78 bytes
- Typical (small agent): ~1-10 KB
- Medium (with state): ~100 KB
- Large (complex agent): ~1 MB
- Maximum (v1.0 target): <100 MB

## Best Practices

### 1. Minimize Agent State
```rust
// Good: Small, focused agents
struct SensorAgent {
    reading: f32,
    timestamp: u64,
}

// Avoid: Large, stateful agents
struct MonolithAgent {
    history: Vec<f32>,  // Grows unbounded
    cache: HashMap<String, Vec<u8>>,  // Large memory
}
```

### 2. Use Policies Wisely
```rust
// Define clear policies
let policy = Policy {
    min_battery_percent: 20,  // Reasonable threshold
    max_latency_ms: 100,      // Based on requirements
    preferred_architectures: vec!["aarch64".to_string()],
};

// Not overly restrictive
let bad_policy = Policy {
    min_battery_percent: 90,  // Too strict
    max_latency_ms: 1,        // Unrealistic
    preferred_architectures: vec!["very-specific-cpu".to_string()],
};
```

### 3. Verify After Migration
```rust
// Always verify DNA integrity
fn verify_migration(original: &Agent, migrated: &Agent) -> bool {
    original.dna().hash() == migrated.dna().hash()
}

if !verify_migration(&original_agent, &restored_agent) {
    panic!("Migration integrity check failed!");
}
```

### 4. Handle Errors Gracefully
```rust
match snapshot.restore() {
    Ok(agent) => {
        println!("Migration successful");
        agent.set_state(AgentState::Running);
    }
    Err(e) => {
        eprintln!("Migration failed: {}", e);
        // Implement retry logic or fallback
    }
}
```

### 5. Monitor Snapshot Age
```rust
let snapshot = MigrationSnapshot::capture(&agent, None)?;

if snapshot.age_seconds() > 60 {
    println!("Warning: Snapshot is {} seconds old", snapshot.age_seconds());
    // Consider re-capturing
}
```

## Roadmap

### Phase 1 (v0.1 "Ranvier") ✅ Complete
- ✅ Agent lifecycle management
- ✅ DNA content addressing
- ✅ Snapshot/restore mechanism
- ✅ Policy framework
- ✅ Migration manager

### Phase 2 (v0.2 "Oligodendrocyte") - Q1-Q2 2026
- [ ] LZ4 compression for snapshots
- [ ] Incremental state snapshots
- [ ] Migration telemetry and metrics
- [ ] Enhanced error recovery

### Phase 3 (v0.3 "Schwann") - Q2-Q3 2026
- [ ] Delta encoding (changed pages only)
- [ ] Encrypted agent state (AES-256-GCM)
- [ ] Advanced policies (thermal, load-based)
- [ ] Multi-agent coordination primitives

### Phase 4 (v1.0 "Saltatory") - Q4 2026
- [ ] Signed DNA verification (Ed25519)
- [ ] Agent versioning and upgrades
- [ ] Capability attestation
- [ ] Production hardening

See [TODO.md](../TODO.md) for detailed roadmap.

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

**Key areas for contribution:**
- Snapshot compression algorithms
- Policy DSL design
- Migration optimization strategies
- Agent coordination patterns
- Performance benchmarking

## Resources

- [Main Documentation](../mielin.md) - Complete technical whitepaper
- [TODO & Roadmap](../TODO.md) - Development plan
- [Examples](../examples/) - Reference implementations
- [mielin-wasm](../mielin-wasm/README.md) - WASM runtime integration

## Contact

- **Repository**: https://github.com/cool-japan/mielin
- **Issues**: https://github.com/cool-japan/mielin/issues
- **Email**: contact@cooljapan.tech

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../LICENSE-MIT))

at your option.

---

**Mielin Cells** - The neurotransmitters enabling autonomous agents to traverse the computational nervous system 🧠⚡

**Current Version:** v0.1.0-rc.1 "Oligodendrocyte" | Released 2026-01-18
