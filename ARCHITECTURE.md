# MielinOS Architecture

## Overview

MielinOS is designed as a 5-layer unikernel operating system inspired by the structure and function of neural systems, particularly the myelin sheath's role in enabling saltatory conduction.

## Design Philosophy

### Biological Inspiration

Just as myelin sheaths enable rapid signal transmission through saltatory conduction (signals "jumping" between Nodes of Ranvier), MielinOS enables ultra-fast "jumps" of AI agents across heterogeneous compute platforms.

**Key Metaphors:**
- **Myelin Sheath** → MielinOS Core (provides fast conduction path)
- **Nodes of Ranvier** → Compute Nodes (gaps where signals regenerate)
- **Saltatory Conduction** → Agent Migration (fast jumps between nodes)
- **Axon** → Hardware Layer (underlying substrate)
- **Neurotransmitters** → Agents (information carriers)

### Core Principles

1. **Ultra-Fast Migration**: Agent migration overhead <1ms (target)
2. **Heterogeneous Hardware**: Support diverse architectures (ARM, RISC-V, x86, Cortex-M)
3. **Capability Security**: Fine-grained permission model
4. **Lightweight**: Minimal kernel footprint (<10KB)
5. **No-Trust**: Agents are sandboxed with explicit capabilities

---

## 5-Layer Architecture

```
┌─────────────────────────────────────────────────┐
│ Layer 4: Tooling (Cortex)                      │  ← Control & Monitoring
│  - mielin-cli                                   │
│  - Studio (future)                              │
│  - Monitoring (future)                          │
├─────────────────────────────────────────────────┤
│ Layer 3: MielinMesh (Synapse)                  │  ← Networking
│  - mielin-mesh/core (DHT, Routing)             │
│  - mielin-mesh/wire (QUIC Protocol)            │
├─────────────────────────────────────────────────┤
│ Layer 2: Mielin Cells (Neurotransmitter)       │  ← Agent Management
│  - mielin-cells (Lifecycle, Migration, DNA)    │
│  - mielin-wasm (Sandbox Execution)             │
├─────────────────────────────────────────────────┤
│ Layer 1: MielinOS Core/RT (Myelin Sheath)      │  ← OS Kernel
│  - mielin-kernel (Memory, Scheduler, Tensor)   │
│  - mielin-hal (Hardware Abstraction)           │
│  - mielin-rt (Embedded Runtime)                │
│  - mielin-tensor (TensorLogic)                 │
├─────────────────────────────────────────────────┤
│ Layer 0: Hardware (Axon)                       │  ← Physical Layer
│  - AArch64, RISC-V, x86_64, Cortex-M          │
└─────────────────────────────────────────────────┘
```

---

## Layer 0: Hardware (Axon)

The physical compute substrate.

### Supported Architectures

- **AArch64**: AWS Graviton, Apple Silicon, Neoverse
- **RISC-V**: RISC-V cores (SiFive, etc.)
- **x86_64**: Intel, AMD processors
- **Cortex-M**: ARM microcontrollers (future)

### Hardware Capabilities

Detected via `mielin-hal`:
- **SIMD**: Vector instructions (NEON, AVX, AVX2, AVX512)
- **SVE/SVE2**: Scalable Vector Extensions (ARM)
- **SME**: Scalable Matrix Extensions (ARM)
- **NPU**: Neural Processing Units (future)

---

## Layer 1: MielinOS Core/RT (Myelin Sheath)

The kernel layer providing core OS functionality.

### mielin-kernel (Core Unikernel)

**Purpose**: Minimal no_std kernel for agent execution

**Components**:
- **Memory Manager**: Page-based allocator (4KB pages, 1024 max)
- **Scheduler**: Priority-based cooperative scheduler (64 concurrent tasks)
- **Tensor Context**: TensorLogic execution context
- **Boot**: Bootloader integration (x86_64-unknown-none)

**Key Design Decisions**:
- **no_std**: No standard library for minimal footprint
- **Cooperative Scheduling**: Simpler than preemptive, sufficient for agents
- **Page-Based Memory**: Fixed-size pages for predictable allocation
- **Static Allocation**: All resources allocated at compile/boot time

**Memory Layout** (simplified):
```
0x0000_0000_0000_0000 - Null guard page
0x0000_0000_0001_0000 - Kernel code/data
0x0000_0000_0010_0000 - Page pool (4MB = 1024 * 4KB)
0x0000_0000_0050_0000 - Task stacks
0x0000_0000_0100_0000 - Agent memory
...
```

### mielin-hal (Hardware Abstraction Layer)

**Purpose**: Abstract hardware differences across architectures

**Architecture Detection**:
```rust
pub enum Architecture {
    AArch64,   // ARM 64-bit
    RiscV64,   // RISC-V 64-bit
    X86_64,    // x86 64-bit
    ArmCortexM, // ARM Cortex-M
}
```

**Capability Detection**:
```rust
bitflags! {
    pub struct HardwareCapabilities: u64 {
        const FPU    = 1 << 0;  // Floating point
        const SIMD   = 1 << 1;  // SIMD instructions
        const SVE2   = 1 << 2;  // Scalable vectors (ARM)
        const AVX512 = 1 << 11; // Advanced vectors (x86)
        // ...
    }
}
```

**Vector Width Detection**: 128/256/512 bits

### mielin-rt (Embedded Runtime)

**Purpose**: Lightweight runtime for IoT/edge devices

**Power Management**:
- Normal: Full performance
- LowPower: Reduced clock
- UltraLowPower: Minimal power
- Sleep: Deep sleep

**Battery-Aware Migration**:
- Monitor battery level
- Trigger migration when low (<20%)
- Preserve agent state during migration

### mielin-tensor (TensorLogic)

**Purpose**: Hardware-aware tensor operation scheduling

**Capabilities**:
- Detect SVE2/SME/NPU support
- Schedule tensor ops to appropriate hardware
- Resource management for tensor operations

**Future**: Direct integration with hardware accelerators

---

## Layer 2: Mielin Cells (Neurotransmitter)

The agent management layer.

### mielin-cells (Agent SDK)

**Purpose**: Complete agent lifecycle and migration management

#### Agent Structure

```rust
pub struct Agent {
    id: AgentId,        // UUID
    dna: Dna,           // WASM binary (code)
    state: AgentState,  // Runtime state
    policy: Policy,     // Execution constraints
}
```

#### Agent Lifecycle

```
Created → Running → Suspended → Migrating → Running
   ↓         ↓          ↓           ↓
Terminated ←←←←←←←←←←←←←←←←←←←←←←←←←←
```

#### Agent DNA (Code)

```rust
pub struct Dna {
    binary: Vec<u8>,    // WASM binary
    hash: [u8; 32],     // SHA-256 hash
}
```

DNA is **content-addressable**: hash(binary) = ID

#### Migration Snapshot

```rust
pub struct MigrationSnapshot {
    agent_id: [u8; 16],      // Agent UUID
    wasm_binary: Vec<u8>,    // Code
    wasm_state: Vec<u8>,     // Runtime state
    policy: Policy,          // Constraints
    timestamp: u64,          // Creation time
    source_node: Option<[u8; 16]>, // Source
}
```

**Serialization**: Binary encoding with bincode (~78 bytes typical)

#### Policy System

```rust
pub struct Policy {
    battery_threshold: Option<u8>,     // Min battery %
    max_latency_ms: Option<u32>,       // Max network latency
    preferred_arch: Option<Architecture>, // Preferred CPU
}
```

Policies control **where** and **when** agents can execute.

### mielin-wasm (WASM Runtime)

**Purpose**: Sandboxed agent execution

**Runtime**: Wasmtime 27.0

**Capabilities** (Capability-based security):
```rust
pub enum Capability {
    FileSystem,  // File access
    Network,     // Network access
    Camera,      // Camera access
    Gpio,        // GPIO pins
}
```

**Sandbox**: Agents start with **zero** capabilities, must be explicitly granted.

**Execution**:
1. Validate WASM magic number (`\0asm`)
2. Compile module with Wasmtime
3. Check granted capabilities
4. Execute with sandbox enabled

---

## Layer 3: MielinMesh (Synapse)

The networking layer for inter-node communication.

### mielin-mesh/core (DHT & Routing)

**Purpose**: Distributed hash table for peer discovery

**Algorithm**: Kademlia DHT with XOR distance metric

**XOR Distance**:
```
distance(A, B) = A XOR B
```

Nodes are organized in k-buckets by XOR distance.

**K-Bucket Size**: 20 peers per bucket

**Peer Selection**:
- By distance: closest K peers
- By latency: lowest latency peers (latency-aware routing)

**Peer Timeout**: 5 minutes of inactivity

**Node Roles**:
- **Edge**: IoT devices, sensors
- **Relay**: Intermediate routing nodes
- **Core**: Cloud servers, high availability

### mielin-mesh/wire (Protocol)

**Transport**: QUIC (quinn) - currently stub

**Message Types**:
```rust
pub enum Message {
    Ping { timestamp: u64 },
    Pong { timestamp: u64, latency_ms: u32 },
    AgentMigration {
        agent_id: [u8; 16],
        snapshot: Vec<u8>,
        priority: u8,
    },
    MigrationAck {
        agent_id: [u8; 16],
        success: bool,
        error_msg: Option<String>,
    },
    Discovery {
        node_id: [u8; 16],
        node_role: NodeRole,
        capabilities: Vec<String>,
    },
    LoadInfo {
        cpu_usage: f32,
        memory_usage: f32,
        active_agents: usize,
    },
}
```

**Priority**: Critical messages (migration) get priority

**Reliability**: ACK for migration messages

---

## Layer 4: Tooling (Cortex)

User-facing tools and interfaces.

### mielin-cli (Command-Line Interface)

**Tool**: `mielinctl`

**Commands** (future):
```bash
# Node management
mielinctl node start --role edge
mielinctl node list
mielinctl node info <node-id>

# Agent deployment
mielinctl agent deploy agent.wasm
mielinctl agent list
mielinctl agent migrate <agent-id> <target-node>

# Mesh inspection
mielinctl mesh status
mielinctl mesh peers
```

**Current Status**: Stub implementation

---

## Key Data Flows

### Agent Creation Flow

```
1. User provides WASM binary
2. Agent::new() creates agent with UUID
3. DNA hash computed (SHA-256)
4. Agent state set to Created
5. Policy applied (constraints)
6. Ready for execution
```

### Agent Migration Flow (10 Phases)

```
Phase 1: Create source and target nodes
  ↓
Phase 2: Detect hardware capabilities
  ↓
Phase 3: Create agent with WASM binary
  ↓
Phase 4: Initiate migration (capture snapshot)
  ↓
Phase 5: Serialize snapshot to bytes
  ↓
Phase 6: Create migration message
  ↓
Phase 7: Send message over network (simulated)
  ↓
Phase 8: Deserialize on target node
  ↓
Phase 9: Restore agent from snapshot
  ↓
Phase 10: Send acknowledgment
```

### DHT Peer Discovery Flow

```
1. Node joins mesh with UUID
2. Send Discovery message to bootstrap nodes
3. Receive peer information
4. Insert peers into routing table (k-buckets)
5. Periodic Ping/Pong for latency measurement
6. Remove stale peers (timeout)
7. Continuously maintain routing table
```

---

## Performance Characteristics

### Memory Operations

- **Page Allocation**: O(n) worst case, ~50 ns average
- **Page Deallocation**: O(1), ~50 ns

### Scheduler Operations

- **Task Spawn**: O(1), ~100 ns
- **Schedule Next**: O(n) scan, ~50 ns with few tasks

### Agent Operations

- **Agent Creation**: ~1 μs (WASM validation)
- **Snapshot Capture**: ~10 μs (serialization)
- **Snapshot Restore**: ~2 μs (deserialization)

### Network Operations

- **DHT Lookup**: O(log n), ~100 μs for 1000 peers
- **Peer Insertion**: O(1), ~1 μs
- **Message Serialize**: ~5 μs (bincode)

---

## Security Model

### Capability-Based Security

**Zero-Trust Model**: Agents start with no capabilities

**Explicit Grants**: Capabilities must be explicitly granted

**Example**:
```rust
let mut sandbox = Sandbox::new();
sandbox.grant(Capability::Network);  // Allow network
// FileSystem NOT granted → blocked
```

**Enforcement**: Wasmtime sandbox enforces at runtime

### Memory Safety

**Language**: Pure Rust (memory-safe)

**Unsafe Code**: Minimized, only in:
- Kernel memory allocator (global state)
- Scheduler (task switching)

**Validation**: All unsafe blocks audited

### Network Security

**Current (v0.1)**: No encryption (stub only)

**Future (v0.2+)**:
- TLS 1.3 for QUIC
- Agent DNA signing (verify authenticity)
- Node authentication
- Encrypted snapshots

---

## Scalability

### Current Limits (v0.1.0)

- **Max Pages**: 1024 (4MB total memory)
- **Max Tasks**: 64 concurrent
- **Max Agents**: Limited by memory
- **DHT Peers**: K-buckets of 20, ~260 total

### Scaling Strategies (Future)

- **Vertical**: Increase page pool, task limit
- **Horizontal**: Multi-node mesh
- **Hierarchical**: Core/Relay/Edge topology
- **Sharding**: Partition agent space by ID range

---

## Testing Strategy

### Unit Tests (31 tests)

- Memory allocation/deallocation
- Scheduler priority and state
- DHT routing and peer management
- Agent lifecycle and migration
- WASM compilation and validation

### Integration Tests

- Agent migration end-to-end (10 phases)
- Multi-step workflows

### Benchmarks

- Kernel operations (memory, scheduler)
- Agent operations (creation, migration)
- Mesh operations (DHT, routing)

**Tooling**: Criterion for statistical benchmarks

---

## Future Enhancements

### v0.2 "Oligodendrocyte"

- Real QUIC transport (quinn)
- 3-node local cluster
- Live agent migration over network
- Performance optimization (<1ms overhead)

### v0.3 "Schwann"

- Cortex-M support (embedded devices)
- Battery-aware migration (real hardware)
- IoT device integration
- Multi-architecture testing

### v1.0 "Saltatory"

- TensorLogic integration (SVE2/SME execution)
- Security audit (third-party)
- 1000+ node simulation
- Production deployment

---

## Design Decisions & Rationale

### Why no_std Kernel?

- **Minimal Dependencies**: No reliance on OS
- **Predictable**: No hidden allocations
- **Portable**: Works on bare metal
- **Small**: Fits in ~10KB

### Why Cooperative Scheduling?

- **Simpler**: No context switching overhead
- **Sufficient**: Agents are cooperative
- **Predictable**: No preemption surprises

### Why Kademlia DHT?

- **Proven**: Used in BitTorrent, IPFS
- **Efficient**: O(log n) lookups
- **Resilient**: Self-healing topology
- **Decentralized**: No single point of failure

### Why Wasmtime?

- **Mature**: Production-grade WASM runtime
- **Sandboxed**: Built-in security model
- **Fast**: JIT compilation
- **Standard**: W3C WebAssembly spec

### Why Content-Addressable DNA?

- **Verification**: Hash proves authenticity
- **Deduplication**: Same code = same hash
- **Integrity**: Detect tampering

---

## References

### Biological Inspiration

- Myelination in neural systems
- Saltatory conduction in axons
- Nodes of Ranvier (regeneration points)

### Technical References

- [Kademlia DHT Paper](https://pdos.csail.mit.edu/~petar/papers/maymounkov-kademlia-lncs.pdf)
- [QUIC Protocol (RFC 9000)](https://www.rfc-editor.org/rfc/rfc9000.html)
- [WebAssembly Specification](https://webassembly.github.io/spec/)
- [Capability-Based Security](https://en.wikipedia.org/wiki/Capability-based_security)

---

## Glossary

- **Agent**: Autonomous program (WASM binary + state)
- **DNA**: Agent code (WASM binary)
- **Migration**: Moving agent between nodes
- **Snapshot**: Serialized agent state
- **Capability**: Permission to access resource
- **DHT**: Distributed Hash Table
- **K-Bucket**: Routing table bucket in Kademlia
- **XOR Distance**: Metric for DHT routing
- **Saltatory Conduction**: Fast signal propagation
- **TensorLogic**: Kernel-level tensor awareness

---

**MielinOS Architecture** - Designed for the trillion-device ASI era 🧠⚡
