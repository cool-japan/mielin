# MielinOS Architecture Guide

## Table of Contents

1. [System Overview](#system-overview)
2. [Component Architecture](#component-architecture)
3. [Data Flow](#data-flow)
4. [Threading and Concurrency Model](#threading-and-concurrency-model)
5. [Memory Management Strategy](#memory-management-strategy)
6. [Security Architecture](#security-architecture)
7. [Network Architecture](#network-architecture)
8. [Design Patterns](#design-patterns)

---

## System Overview

MielinOS is a Rust-based unikernel operating system designed for autonomous AI agents to seamlessly traverse across heterogeneous compute platforms—from IoT microcontrollers to cloud clusters—at nanosecond-scale latency.

### Biological Metaphor

The architecture is inspired by the myelin sheath in neural systems:

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

## Component Architecture

### 5-Layer Architecture

```
┌─────────────────────────────────────────────────┐
│ Layer 4: Tooling (Cortex)                      │  ← Control & Monitoring
│  - mielin-cli (mielinctl)                      │
│  - MielinStudio (Web UI - future)              │
│  - Monitoring & Observability                  │
├─────────────────────────────────────────────────┤
│ Layer 3: MielinMesh (Synapse)                  │  ← Networking & Discovery
│  - mielin-mesh/core (DHT, Routing)             │
│  - mielin-mesh/wire (QUIC Protocol)            │
│  - Service Discovery & Load Balancing          │
├─────────────────────────────────────────────────┤
│ Layer 2: Mielin Cells (Neurotransmitter)       │  ← Agent Management
│  - mielin-cells (Lifecycle, Migration)         │
│  - mielin-wasm (Sandbox Execution)             │
│  - Policy Engine & Resource Management         │
├─────────────────────────────────────────────────┤
│ Layer 1: MielinOS Core/RT (Myelin Sheath)      │  ← OS Kernel
│  - mielin-kernel (Memory, Scheduler)           │
│  - mielin-hal (Hardware Abstraction)           │
│  - mielin-rt (Embedded Runtime)                │
│  - mielin-tensor (TensorLogic)                 │
├─────────────────────────────────────────────────┤
│ Layer 0: Hardware (Axon)                       │  ← Physical Layer
│  - AArch64, RISC-V, x86_64, Cortex-M          │
│  - SIMD (NEON, SVE2, AVX2, AVX512)            │
│  - NPU/TPU Accelerators                        │
└─────────────────────────────────────────────────┘
```

### Layer 0: Hardware (Axon)

The physical compute substrate providing the foundation for all operations.

#### Supported Architectures

| Architecture | Status | Features | Use Cases |
|-------------|--------|----------|-----------|
| **AArch64** | ✅ Full | NEON, SVE2, SME | AWS Graviton, Raspberry Pi, Apple Silicon |
| **RISC-V** | ✅ Full | RVV (Vector) | SiFive cores, embedded systems |
| **x86_64** | ✅ Full | AVX2, AVX512 | Intel/AMD servers, desktops |
| **Cortex-M** | 🚧 Partial | DSP, FPU | STM32, ESP32, microcontrollers |

#### Hardware Detection

MielinOS automatically detects hardware capabilities at runtime:

```rust
use mielin_hal::{detect_architecture, capabilities::HardwareProfile};

let arch = detect_architecture();
let profile = HardwareProfile::detect();

// Check for SIMD capabilities
if profile.capabilities.contains(HardwareCapabilities::SVE2) {
    // Use SVE2-optimized code paths
}
```

### Layer 1: MielinOS Core/RT (Myelin Sheath)

The kernel layer providing core OS functionality with two variants:

#### 1.1 mielin-kernel (Core Unikernel)

**Purpose**: Minimal `no_std` kernel for agent execution on application processors

**Key Components**:

- **Memory Manager** (`memory` module)
  - Page-based allocator (4KB pages, 1024 max)
  - Bitmap tracking for allocation state
  - O(n) allocation, O(1) deallocation
  - Static memory pool (4MB default)

- **Scheduler** (`scheduler` module)
  - Priority-based cooperative scheduler
  - Support for 64 concurrent tasks
  - Round-robin within priority levels
  - Async/await support via custom executor

- **Tensor Context** (`tensor` module)
  - Hardware-aware tensor operation routing
  - Integration with TensorLogic
  - Automatic dispatch to optimal backend

- **Boot System** (`boot` module)
  - Bootloader integration (x86_64-unknown-none)
  - Early initialization sequence
  - Hardware enumeration

**Memory Layout** (x86_64):
```
0x0000_0000_0000_0000 - Null guard page (4KB)
0x0000_0000_0001_0000 - Kernel code/data
0x0000_0000_0010_0000 - Page pool (4MB = 1024 * 4KB)
0x0000_0000_0050_0000 - Task stacks
0x0000_0000_0100_0000 - Agent memory
```

#### 1.2 mielin-hal (Hardware Abstraction Layer)

**Purpose**: Abstract hardware differences across architectures

**Architecture Detection**:
```rust
pub enum Architecture {
    AArch64,    // ARM 64-bit
    RiscV64,    // RISC-V 64-bit
    X86_64,     // x86 64-bit
    ArmCortexM, // ARM Cortex-M
}
```

**Capability Detection**:
```rust
bitflags! {
    pub struct HardwareCapabilities: u64 {
        const FPU     = 1 << 0;  // Floating point
        const SIMD    = 1 << 1;  // SIMD instructions
        const NEON    = 1 << 2;  // ARM NEON
        const SVE2    = 1 << 3;  // ARM Scalable Vectors
        const SME     = 1 << 4;  // ARM Scalable Matrix
        const AVX2    = 1 << 10; // Intel AVX2
        const AVX512  = 1 << 11; // Intel AVX-512
        // ... more capabilities
    }
}
```

**Vector Width Detection**: Automatically detects optimal vector width (128/256/512 bits)

#### 1.3 mielin-rt (Embedded Runtime)

**Purpose**: Lightweight runtime for IoT/edge devices with strict resource constraints

**Key Features**:

- **Power Management**:
  - Normal: Full performance
  - LowPower: Reduced clock frequency
  - UltraLowPower: Minimal power consumption
  - Sleep: Deep sleep mode

- **Battery-Aware Migration**:
  - Monitor battery level via ADC
  - Trigger migration when low (<20% threshold)
  - Preserve complete agent state

- **Memory Pools**:
  - Fixed-size block allocation
  - No fragmentation
  - Predictable allocation time

#### 1.4 mielin-tensor (TensorLogic)

**Purpose**: Kernel-level awareness of tensor operations for AI workloads

**Key Capabilities**:

- Detect hardware acceleration (SVE2/SME/NPU)
- Route tensor operations to optimal backend
- Resource management for tensor workloads
- Memory estimation and planning

**Backend Selection**:
```rust
pub struct TensorRuntime {
    capabilities: HardwareCapabilities,
    ops: TensorOps,
}

impl TensorRuntime {
    pub fn acceleration_info(&self) -> &'static str {
        if self.supports_sve2() {
            "Arm SVE2 (Scalable Vector Extension)"
        } else if self.supports_avx512() {
            "Intel AVX-512"
        } else if self.supports_neon() {
            "Arm NEON (Advanced SIMD)"
        } else {
            "Scalar (no SIMD acceleration)"
        }
    }
}
```

### Layer 2: Mielin Cells (Neurotransmitter)

The agent management layer responsible for lifecycle, migration, and execution.

#### 2.1 mielin-cells (Agent SDK)

**Purpose**: Complete agent lifecycle and migration management

**Core Types**:

```rust
pub struct Agent {
    id: AgentId,        // UUID (16 bytes)
    dna: Dna,           // WASM binary (code)
    state: AgentState,  // Runtime state
    policy: Policy,     // Execution constraints
}

pub struct Dna {
    binary: Vec<u8>,    // WASM bytecode
    hash: [u8; 32],     // SHA-256 hash
}
```

**Agent Lifecycle**:
```
Created → Running → Suspended → Migrating → Running
   ↓         ↓          ↓           ↓
Terminated ←←←←←←←←←←←←←←←←←←←←←←←←←←
```

**State Transitions**:

| From | To | Trigger | Action |
|------|-----|---------|--------|
| Created | Running | `start()` | Initialize WASM runtime |
| Running | Suspended | `pause()` | Capture state snapshot |
| Suspended | Running | `resume()` | Restore state from snapshot |
| Running | Migrating | `migrate()` | Serialize agent + state |
| Migrating | Running | Network transfer | Deserialize on target |
| Any | Terminated | `terminate()` | Cleanup resources |

**Migration Snapshot**:
```rust
pub struct MigrationSnapshot {
    agent_id: [u8; 16],         // Agent UUID
    wasm_binary: Vec<u8>,       // Code (DNA)
    wasm_state: Vec<u8>,        // Runtime state
    policy: Policy,             // Constraints
    timestamp: u64,             // Creation time
    source_node: Option<[u8; 16]>, // Source node ID
}
```

**Policy System**:
```rust
pub struct Policy {
    battery_threshold: Option<u8>,        // Min battery %
    max_latency_ms: Option<u32>,          // Max network latency
    preferred_arch: Option<Architecture>, // Preferred CPU arch
    required_capabilities: Vec<Capability>, // Required features
}
```

**Specialized Modules**:

- **Compliance** (`compliance` module): Audit logging, GDPR/HIPAA compliance
- **High Availability** (`ha` module): Failover, replication, leader election
- **Disaster Recovery** (`dr` module): Backup, verification, recovery
- **Multi-Region** (`multiregion` module): Geographic routing, cross-region sync
- **Orchestration** (`orchestration` module): Deployment, scaling, scheduling
- **Security** (`security` module): Identity, encryption, sandboxing
- **Debug** (`debug` module): Remote debugging, profiling, tracing

#### 2.2 mielin-wasm (WASM Runtime)

**Purpose**: Sandboxed agent execution using WebAssembly

**Runtime**: Wasmtime 39.0 with custom capability extensions

**Capability-Based Security**:
```rust
pub enum Capability {
    FileSystem,  // File I/O access
    Network,     // Network socket access
    Camera,      // Camera device access
    Gpio,        // GPIO pin access (embedded)
    I2C,         // I2C bus access (embedded)
    SPI,         // SPI bus access (embedded)
}
```

**Sandbox Configuration**:
```rust
pub struct SandboxConfig {
    granted_capabilities: Vec<Capability>,
    max_memory_bytes: usize,
    max_cpu_time_ms: u64,
    allow_network_egress: bool,
}
```

**Execution Flow**:
1. Validate WASM magic number (`\0asm`)
2. Compile module with Wasmtime
3. Check granted capabilities
4. Execute with sandbox enabled
5. Monitor resource usage
6. Enforce timeouts and limits

### Layer 3: MielinMesh (Synapse)

The networking layer enabling agent migration and inter-node communication.

#### 3.1 mielin-mesh/core (DHT & Routing)

**Purpose**: Distributed hash table for peer discovery and routing

**Algorithm**: Extended Kademlia DHT with XOR distance metric

**XOR Distance**:
```
distance(A, B) = A XOR B
```

Nodes are organized in k-buckets by XOR distance from local node.

**DHT Parameters**:

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| K-Bucket Size | 20 peers | Balances redundancy vs overhead |
| Peer Timeout | 5 minutes | Detects failures quickly |
| Replication Factor | 3 | Ensures data availability |
| Lookup Parallelism | 3 | Speeds up lookups |

**Node Roles**:

- **Edge**: IoT devices, sensors (limited resources)
- **Relay**: Intermediate routing nodes (stable connectivity)
- **Core**: Cloud servers, high availability (maximum resources)

**Routing Features**:

- **Distance-based routing**: Closest K peers by XOR distance
- **Latency-aware routing**: Prefer low-latency peers
- **Geographic awareness**: Route based on physical location
- **Load balancing**: Distribute load across peers

**Key Operations**:

| Operation | Complexity | Typical Latency |
|-----------|-----------|-----------------|
| Node Join | O(log n) | ~100ms |
| Peer Lookup | O(log n) | ~100μs (local cache) |
| Peer Insertion | O(1) | ~1μs |
| Route Discovery | O(log n) | ~200ms |

#### 3.2 mielin-mesh/wire (Protocol)

**Purpose**: Binary protocol for inter-node communication

**Transport**: QUIC (UDP-based, encrypted, multiplexed)

**Message Types**:
```rust
pub enum Message {
    // Control messages
    Ping { timestamp: u64 },
    Pong { timestamp: u64, latency_ms: u32 },

    // Discovery messages
    Discovery {
        node_id: [u8; 16],
        node_role: NodeRole,
        capabilities: Vec<String>,
    },

    // Migration messages (high priority)
    AgentMigration {
        agent_id: [u8; 16],
        snapshot: Vec<u8>,      // Serialized MigrationSnapshot
        priority: u8,            // 0-255, higher = more urgent
    },
    MigrationAck {
        agent_id: [u8; 16],
        success: bool,
        error_msg: Option<String>,
    },

    // Monitoring messages
    LoadInfo {
        cpu_usage: f32,          // 0.0-1.0
        memory_usage: f32,       // 0.0-1.0
        active_agents: usize,
    },
}
```

**Priority Queuing**: Critical messages (migration) bypass normal queue

**Reliability**: ACK/NAK for migration messages, best-effort for discovery

**Serialization**: Binary encoding with oxicode (~40% smaller than bincode)

### Layer 4: Tooling (Cortex)

User-facing tools and interfaces for management and monitoring.

#### 4.1 mielin-cli (Command-Line Interface)

**Tool**: `mielinctl`

**Current Commands**:
```bash
# Node management
mielinctl node start --role edge --port 5001
mielinctl node list
mielinctl node info <node-id>

# Agent deployment
mielinctl agent deploy agent.wasm
mielinctl agent list
mielinctl agent migrate <agent-id> <target-node>

# Mesh inspection
mielinctl mesh status
mielinctl mesh peers
mielinctl mesh topology
```

**Future Features**:
- Interactive REPL mode
- Scripting support (Rhai)
- Remote management via HTTP API
- Batch operations

---

## Data Flow

### Agent Creation Flow

```
1. User provides WASM binary
2. Validate WASM magic number and structure
3. Compute DNA hash (SHA-256)
4. Create Agent with UUID
5. Set initial state to Created
6. Apply policy constraints
7. Ready for execution
```

### Agent Execution Flow

```
1. Scheduler selects agent for execution
2. Load WASM module into Wasmtime
3. Initialize runtime environment
4. Grant capabilities per policy
5. Execute agent code
6. Monitor resource usage
7. Yield on cooperation points
8. Update agent state
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
  ↓ Snapshot includes: agent_id, wasm_binary, wasm_state, policy
Phase 5: Serialize snapshot to bytes (oxicode)
  ↓ Typical size: ~1-100 KB
Phase 6: Create AgentMigration message
  ↓ Includes: agent_id, snapshot, priority
Phase 7: Send over QUIC connection
  ↓ Encrypted, reliable, multiplexed
Phase 8: Deserialize on target node
  ↓ Validate integrity, check compatibility
Phase 9: Restore agent from snapshot
  ↓ Load WASM, restore state, apply policy
Phase 10: Send MigrationAck
  ↓ Confirm success or report error
```

### DHT Peer Discovery Flow

```
1. Node joins mesh with UUID
2. Generate node ID (random or derived)
3. Connect to bootstrap nodes
4. Send Discovery message to bootstrap
5. Receive peer list from bootstrap
6. Insert peers into routing table (k-buckets)
7. Periodic Ping/Pong for latency measurement
8. Update routing table with latencies
9. Remove stale peers (timeout)
10. Continuously maintain routing table
```

### Network Message Flow

```
┌──────────┐                           ┌──────────┐
│  Node A  │                           │  Node B  │
└────┬─────┘                           └────┬─────┘
     │                                      │
     │  1. Open QUIC connection             │
     │─────────────────────────────────────>│
     │                                      │
     │  2. TLS handshake (encrypted)        │
     │<─────────────────────────────────────│
     │                                      │
     │  3. Send AgentMigration message      │
     │─────────────────────────────────────>│
     │                                      │
     │  4. Receive & deserialize            │
     │                                      │
     │  5. Restore agent                    │
     │                                      │
     │  6. Send MigrationAck                │
     │<─────────────────────────────────────│
     │                                      │
     │  7. Terminate source agent           │
     │                                      │
```

---

## Threading and Concurrency Model

### Cooperative Multitasking

MielinOS uses **cooperative multitasking** rather than preemptive scheduling:

**Advantages**:
- Simpler implementation
- No context switching overhead
- Predictable execution
- Lower memory usage

**Requirements**:
- Agents must yield voluntarily
- No infinite loops without yield points
- Async/await for I/O operations

### Async/Await Model

All I/O operations use async/await:

```rust
use tokio::runtime::Runtime;

// Create async runtime
let rt = Runtime::new()?;

// Execute async operation
rt.block_on(async {
    let result = async_operation().await;
});
```

### Task Scheduling

**Priority Levels**: 4 levels (0-3, 3 = highest)

**Scheduling Algorithm**:
1. Select highest priority level with runnable tasks
2. Round-robin within priority level
3. Execute task until yield point
4. Move to next task in round-robin order

**Fairness**: Equal time slices within priority level

### Concurrency Primitives

- **Locks**: `Mutex`, `RwLock` from `parking_lot`
- **Channels**: `tokio::sync::mpsc` for message passing
- **Atomic Operations**: `std::sync::atomic` for lock-free data structures
- **Lock-Free Structures**: Custom lock-free queue for scheduler

---

## Memory Management Strategy

### Page-Based Allocation

**Page Size**: 4 KB (standard Linux/x86 page size)

**Pool Size**: 1024 pages (4 MB total)

**Allocation Strategy**:
- Bitmap tracking (1 bit per page)
- First-fit allocation
- Coalescing on deallocation

**Performance**:
- Allocation: O(n) worst case, ~50 ns average
- Deallocation: O(1), ~50 ns
- Fragmentation: Minimal due to fixed page size

### Heap Management

For user-space (with `std`):
- Standard Rust allocator (jemalloc on Linux)
- Custom allocators for embedded (mielin-rt)

For kernel-space (no_std):
- Bump allocator for early boot
- Page allocator for runtime
- No dynamic allocation in critical paths

### Memory Safety

**Language-Level Safety**:
- Rust ownership and borrowing
- No null pointers
- No use-after-free
- No data races (enforced by compiler)

**Unsafe Code**:
- Minimized to kernel internals
- Audited and documented
- Encapsulated in safe abstractions

### Memory Pools

For frequent allocations (embedded):

```rust
pub struct MemoryPool {
    block_size: usize,
    blocks: Vec<*mut u8>,
}

// Pre-allocated blocks
// No fragmentation
// Constant-time allocation
```

---

## Security Architecture

### Capability-Based Security

**Zero-Trust Model**: Agents start with zero capabilities

**Explicit Grants**: Capabilities must be explicitly granted

```rust
let mut sandbox = Sandbox::new();
sandbox.grant(Capability::Network);  // Allow network
// FileSystem NOT granted → blocked
```

**Capability Attestation**:
```rust
pub struct CapabilityAttestation {
    agent_id: AgentId,
    capabilities: Vec<Capability>,
    signature: [u8; 64],  // Ed25519 signature
    expires_at: u64,       // Unix timestamp
}
```

### Sandboxing

**WASM Sandbox**: All agents run in WebAssembly sandbox

**Isolation Guarantees**:
- Memory isolation (linear memory)
- No direct hardware access
- Controlled syscalls via WASI
- Resource limits (memory, CPU time)

**Sandbox Violations**:
```rust
pub enum SandboxViolation {
    UnauthorizedCapability(Capability),
    MemoryLimitExceeded { limit: usize, used: usize },
    CpuTimeLimitExceeded { limit_ms: u64 },
    InvalidSyscall(String),
}
```

### Cryptography

**Algorithms**:
- Hashing: SHA-256 (ring crate)
- Signing: Ed25519 (future)
- Encryption: AES-256-GCM (future)
- Key Exchange: X25519 (future)

**TLS/QUIC**:
- TLS 1.3 for transport security
- Certificate-based authentication
- Perfect forward secrecy

### Identity and Authentication

**Agent Identity**:
```rust
pub struct AgentIdentity {
    agent_id: AgentId,
    public_key: [u8; 32],  // Ed25519 public key
    dna_hash: [u8; 32],    // SHA-256 of WASM binary
}
```

**Authentication Flow**:
1. Agent presents identity and signature
2. Verify signature with public key
3. Verify DNA hash matches binary
4. Grant capabilities based on identity

### Audit Logging

**Compliance Modules**:
- GDPR compliance (data retention, right to deletion)
- HIPAA compliance (audit trails, encryption)
- SOC2 compliance (access control, monitoring)

**Audit Events**:
```rust
pub enum EventType {
    AgentCreated,
    AgentMigrated,
    CapabilityGranted,
    PolicyViolation,
    StateAccessed,
}
```

---

## Network Architecture

### QUIC Transport

**Protocol**: QUIC (RFC 9000) over UDP

**Features**:
- Multiplexing (multiple streams per connection)
- 0-RTT connection establishment
- Built-in encryption (TLS 1.3)
- Connection migration support
- Congestion control

**Implementation**: quinn crate (Rust QUIC library)

### Service Discovery

**Mechanisms**:
1. **Bootstrap Nodes**: Well-known entry points
2. **DHT**: Distributed peer discovery
3. **mDNS**: Local network discovery (future)
4. **DNS-SD**: Service announcement (future)

### Load Balancing

**Strategies**:
- Round-robin: Equal distribution
- Least-loaded: Route to least busy node
- Latency-based: Route to lowest latency
- Geographic: Route to nearest location

**Health Checks**:
- Active: Periodic ping/pong
- Passive: Monitor message latency
- Application-level: Custom health endpoints

### Multi-Region Support

**Geographic Routing**:
```rust
pub struct Region {
    id: RegionId,
    location: GeoLocation,  // Latitude, longitude
    nodes: Vec<Node>,
}
```

**Cross-Region Sync**:
- Eventual consistency
- Conflict resolution (version vectors)
- Bandwidth optimization

---

## Design Patterns

### Actor Model

Agents follow the Actor model:
- Isolated state
- Message-passing communication
- Location transparency

### Event-Driven Architecture

Components communicate via events:
- Publish/subscribe pattern
- Event sourcing for state changes
- CQRS for read/write separation

### Microservices Pattern

Each layer is independently deployable:
- Clear API boundaries
- Service discovery
- Circuit breakers for resilience

### State Machine Pattern

Agent lifecycle as state machine:
- Explicit states
- Defined transitions
- Validation at each step

---

## Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        User Space                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │ mielinctl    │  │ Agent WASM   │  │ Applications │     │
│  │ (CLI)        │  │ Binaries     │  │              │     │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘     │
│         │                  │                  │              │
├─────────┼──────────────────┼──────────────────┼──────────────┤
│         │    mielin-cells  │                  │              │
│         │  ┌───────────────▼──────────────────▼───────────┐ │
│         │  │ Agent SDK (Lifecycle, Policy, Migration)    │ │
│         │  └───────────────┬──────────────────────────────┘ │
│         │                  │                                 │
│         │  ┌───────────────▼──────────────┐                 │
│         │  │ mielin-wasm (Sandbox)        │                 │
│         │  └───────────────┬──────────────┘                 │
│         │                  │                                 │
├─────────┼──────────────────┼─────────────────────────────────┤
│         │   mielin-mesh    │                                 │
│         │  ┌───────────────▼──────────────┐                 │
│         └─>│ Core (DHT, Routing)          │                 │
│            └───────────────┬──────────────┘                 │
│                            │                                 │
│            ┌───────────────▼──────────────┐                 │
│            │ Wire (QUIC Protocol)         │                 │
│            └───────────────┬──────────────┘                 │
│                            │                                 │
├────────────────────────────┼─────────────────────────────────┤
│     mielin-kernel          │       mielin-hal                │
│  ┌──────────────────┐      │   ┌──────────────────┐         │
│  │ Scheduler        │◄─────┼──>│ Architecture     │         │
│  │ Memory Manager   │      │   │ Detection        │         │
│  │ Tensor Context   │      │   │ Capability Check │         │
│  └──────┬───────────┘      │   └────────┬─────────┘         │
│         │                  │            │                    │
├─────────┼──────────────────┼────────────┼─────────────────── │
│         │   Hardware       │            │                    │
│         │  ┌───────────────▼────────────▼─────────────────┐ │
│         └─>│ CPU (AArch64/RISC-V/x86_64/Cortex-M)        │ │
│            │ SIMD (NEON/SVE2/AVX2/AVX512)                 │ │
│            │ NPU/TPU Accelerators                         │ │
│            └──────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-17 | Initial comprehensive architecture documentation |

---

**MielinOS Architecture** - Designed for the trillion-device ASI era
