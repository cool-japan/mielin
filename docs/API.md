# MielinOS API Reference

## Table of Contents

1. [Core APIs](#core-apis)
2. [Agent Management API](#agent-management-api)
3. [Migration API](#migration-api)
4. [Mesh Networking API](#mesh-networking-api)
5. [Hardware Abstraction API](#hardware-abstraction-api)
6. [Tensor Operations API](#tensor-operations-api)
7. [Policy and Security API](#policy-and-security-api)
8. [Best Practices](#best-practices)
9. [Error Handling Guide](#error-handling-guide)
10. [Migration from Other Systems](#migration-from-other-systems)

---

## Core APIs

### mielin-kernel

The kernel provides core memory management and scheduling primitives.

#### Memory Management

```rust
use mielin_kernel::memory::{allocate_pages, deallocate_pages};

// Allocate memory pages
let result = allocate_pages(10); // Request 10 pages (40 KB)
match result {
    Ok(address) => {
        println!("Allocated at: 0x{:x}", address);
        // Use the memory...

        // Deallocate when done
        deallocate_pages(address, 10).expect("Failed to deallocate");
    }
    Err(e) => eprintln!("Allocation failed: {}", e),
}
```

**Available Functions**:

| Function | Signature | Description |
|----------|-----------|-------------|
| `allocate_pages` | `(count: usize) -> Result<usize, KernelError>` | Allocate contiguous pages |
| `deallocate_pages` | `(addr: usize, count: usize) -> Result<(), KernelError>` | Free allocated pages |
| `available_pages` | `() -> usize` | Get free page count |
| `used_pages` | `() -> usize` | Get allocated page count |

**Error Types**:

```rust
pub enum KernelError {
    OutOfMemory { requested: usize, available: usize },
    InvalidPageCount,
    DoubleFree { address: usize },
    AddressOutOfBounds { address: usize, max_address: usize },
}
```

#### Task Scheduling

```rust
use mielin_kernel::scheduler::{spawn_task, set_priority, Priority};

// Spawn a new task
let task_id = spawn_task(Priority::Normal, || {
    println!("Task executing");
    // Task logic here
})?;

// Change task priority
set_priority(task_id, Priority::High)?;
```

**Priority Levels**:

```rust
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}
```

**Available Functions**:

| Function | Signature | Description |
|----------|-----------|-------------|
| `spawn_task` | `(priority: Priority, func: F) -> Result<TaskId>` | Spawn new task |
| `set_priority` | `(id: TaskId, priority: Priority) -> Result<()>` | Change task priority |
| `yield_now` | `() -> ()` | Yield to scheduler |
| `task_count` | `() -> usize` | Get active task count |

---

## Agent Management API

### mielin-cells

The agent SDK provides comprehensive lifecycle management.

#### Creating Agents

```rust
use mielin_cells::{Agent, Dna, Policy};

// Method 1: Create from WASM binary
let wasm_binary = std::fs::read("agent.wasm")?;
let agent = Agent::new(wasm_binary);

println!("Agent ID: {}", agent.id());
println!("DNA Hash: {:?}", agent.dna().hash());

// Method 2: Create with policy
let policy = Policy {
    battery_threshold: Some(20),      // Don't run below 20% battery
    max_latency_ms: Some(100),         // Max 100ms network latency
    preferred_arch: Some(Architecture::AArch64),
    required_capabilities: vec![],
};

let agent_with_policy = Agent::with_policy(wasm_binary, policy);
```

**Agent Methods**:

```rust
impl Agent {
    // Creation
    pub fn new(wasm_binary: Vec<u8>) -> Self;
    pub fn with_policy(wasm_binary: Vec<u8>, policy: Policy) -> Self;

    // Accessors
    pub fn id(&self) -> &AgentId;
    pub fn dna(&self) -> &Dna;
    pub fn state(&self) -> AgentState;
    pub fn policy(&self) -> &Policy;

    // Lifecycle
    pub fn start(&mut self) -> Result<(), AgentError>;
    pub fn pause(&mut self) -> Result<(), AgentError>;
    pub fn resume(&mut self) -> Result<(), AgentError>;
    pub fn terminate(&mut self) -> Result<(), AgentError>;

    // State queries
    pub fn is_running(&self) -> bool;
    pub fn is_suspended(&self) -> bool;
    pub fn is_terminated(&self) -> bool;
}
```

#### Agent Lifecycle Management

```rust
use mielin_cells::{Agent, AgentState};

let mut agent = Agent::new(wasm_binary);

// Start the agent
agent.start()?;
assert_eq!(agent.state(), AgentState::Running);

// Pause execution
agent.pause()?;
assert_eq!(agent.state(), AgentState::Suspended);

// Resume execution
agent.resume()?;
assert_eq!(agent.state(), AgentState::Running);

// Terminate
agent.terminate()?;
assert_eq!(agent.state(), AgentState::Terminated);
```

#### Agent Pools

For managing multiple agents efficiently:

```rust
use mielin_cells::{AgentPool, PoolConfig};

// Create a pool with configuration
let config = PoolConfig {
    min_size: 5,
    max_size: 100,
    idle_timeout_secs: 300,
};

let mut pool = AgentPool::new(config);

// Acquire an agent from the pool
let agent = pool.acquire()?;

// Use the agent
agent.start()?;

// Return to pool when done
pool.release(agent)?;

// Pool statistics
let stats = pool.stats();
println!("Active: {}, Idle: {}", stats.active, stats.idle);
```

#### Agent Templates

Define reusable agent templates:

```rust
use mielin_cells::{AgentTemplate, TemplateBuilder};

// Create a template
let template = TemplateBuilder::new()
    .name("data-processor")
    .wasm_binary(wasm_binary)
    .default_policy(policy)
    .resource_limits(ResourceRequirements {
        min_memory_mb: 64,
        max_memory_mb: 256,
        min_cpu_cores: 1,
    })
    .build()?;

// Create agents from template
let agent1 = template.instantiate()?;
let agent2 = template.instantiate()?;
```

---

## Migration API

### Agent Migration

The migration API enables seamless agent transfer between nodes.

#### Basic Migration

```rust
use mielin_cells::migration::{MigrationSnapshot, MigrationError};

// On source node: Capture agent state
let snapshot = MigrationSnapshot::capture(&agent, None)?;

// Serialize for network transfer
let bytes = snapshot.serialize()?;

// ... send bytes over network ...

// On target node: Deserialize
let restored_snapshot = MigrationSnapshot::deserialize(&bytes)?;

// Restore agent
let new_agent = restored_snapshot.restore()?;
```

#### Advanced Migration with Metadata

```rust
use mielin_cells::migration::{MigrationSnapshot, MigrationMetadata};

// Capture with metadata
let metadata = MigrationMetadata {
    reason: "battery_low".to_string(),
    source_node: Some(source_node_id),
    target_node: Some(target_node_id),
    priority: 5,
};

let snapshot = MigrationSnapshot::capture(&agent, Some(metadata))?;

// Check snapshot size
println!("Snapshot size: {} bytes", snapshot.size());

// Verify integrity
snapshot.verify()?;
```

#### Migration Policies

```rust
use mielin_cells::policy::{Policy, MigrationTrigger};

let policy = Policy {
    battery_threshold: Some(20),
    max_latency_ms: Some(100),
    migration_triggers: vec![
        MigrationTrigger::BatteryLow,
        MigrationTrigger::NodeOverloaded,
        MigrationTrigger::ScheduledMaintenance,
    ],
    ..Default::default()
};
```

#### Versioned Migrations

```rust
use mielin_cells::versioning::{Version, VersionDeployer};

// Deploy new version with canary strategy
let deployer = VersionDeployer::new();

deployer.canary_deploy(
    template,
    CanaryConfig {
        canary_percentage: 10,  // 10% of traffic
        duration_mins: 30,       // Monitor for 30 minutes
        success_threshold: 0.99, // 99% success rate
    }
)?;
```

---

## Mesh Networking API

### mielin-mesh

#### Node Management

```rust
use mielin_mesh::{Node, NodeRole, MeshService, MeshConfig};

// Create a node
let node = Node::new(NodeRole::Edge);
println!("Node ID: {}", node.id());

// Start mesh service
let config = MeshConfig {
    listen_addr: "0.0.0.0:5000".parse()?,
    bootstrap_nodes: vec![
        "192.168.1.100:5000".parse()?,
    ],
    node_role: NodeRole::Edge,
};

let mesh = MeshService::new(config)?;
mesh.start().await?;
```

#### Peer Discovery

```rust
use mielin_mesh::{Dht, NodeId};

// Create DHT instance
let mut dht = Dht::new(node_id);

// Add bootstrap peers
dht.add_peer(bootstrap_peer)?;

// Find closest peers to a target
let target = NodeId::random();
let peers = dht.find_closest_peers(&target, 20)?;

for peer in peers {
    println!("Peer: {} (distance: {})", peer.id, peer.distance);
}
```

#### Service Discovery

```rust
use mielin_mesh::service_discovery::{
    ServiceDiscovery, ServiceRegistration, ServiceQuery,
};

// Register a service
let registration = ServiceRegistration {
    service_name: "ml-inference".to_string(),
    service_type: "gpu".to_string(),
    endpoint: "192.168.1.100:8080".parse()?,
    metadata: vec![
        ("gpu_type".to_string(), "nvidia-a100".to_string()),
        ("memory_gb".to_string(), "40".to_string()),
    ],
};

let mut discovery = ServiceDiscovery::new();
discovery.register(registration).await?;

// Query for services
let query = ServiceQuery {
    service_type: Some("gpu".to_string()),
    required_metadata: vec![
        ("memory_gb".to_string(), "40".to_string()),
    ],
    max_latency_ms: Some(50),
};

let services = discovery.query(query).await?;
for service in services {
    println!("Found: {} at {}", service.service_name, service.endpoint);
}
```

#### Load Balancing

```rust
use mielin_mesh::loadbalancer::{LoadBalancer, LoadBalancingAlgorithm};

// Create load balancer
let mut lb = LoadBalancer::new(LoadBalancingAlgorithm::LeastLoaded);

// Add endpoints
lb.add_endpoint("192.168.1.100:5000".parse()?)?;
lb.add_endpoint("192.168.1.101:5000".parse()?)?;

// Get next endpoint
let endpoint = lb.next_endpoint()?;
println!("Route to: {}", endpoint);
```

#### Multi-Region Support

```rust
use mielin_mesh::multiregion::{
    RegionManager, Region, GeoLocation, ReplicationPolicy,
};

// Define regions
let us_east = Region {
    id: RegionId::new("us-east-1"),
    location: GeoLocation {
        latitude: 38.13,
        longitude: -78.45,
    },
    nodes: vec![],
};

let eu_west = Region {
    id: RegionId::new("eu-west-1"),
    location: GeoLocation {
        latitude: 53.35,
        longitude: -6.26,
    },
    nodes: vec![],
};

// Create region manager
let mut manager = RegionManager::new();
manager.add_region(us_east)?;
manager.add_region(eu_west)?;

// Set replication policy
let policy = ReplicationPolicy {
    min_replicas: 2,
    preferred_regions: vec!["us-east-1", "eu-west-1"],
    consistency_level: ConsistencyLevel::Eventual,
};

manager.set_policy(policy)?;
```

---

## Hardware Abstraction API

### mielin-hal

#### Architecture Detection

```rust
use mielin_hal::{detect_architecture, Architecture};

let arch = detect_architecture();
match arch {
    Architecture::AArch64 => println!("Running on ARM 64-bit"),
    Architecture::X86_64 => println!("Running on x86-64"),
    Architecture::RiscV64 => println!("Running on RISC-V 64-bit"),
    Architecture::ArmCortexM => println!("Running on Cortex-M"),
    _ => println!("Unknown architecture"),
}
```

#### Hardware Capabilities

```rust
use mielin_hal::capabilities::{HardwareProfile, HardwareCapabilities};

let profile = HardwareProfile::detect();

// Check SIMD capabilities
if profile.capabilities.contains(HardwareCapabilities::SVE2) {
    println!("SVE2 available with {} -bit vectors",
             profile.max_vector_width());
}

if profile.capabilities.contains(HardwareCapabilities::AVX512) {
    println!("AVX-512 available");
}

// Get CPU information
println!("Cores: {}", profile.core_count);
println!("Memory: {} MB", profile.memory_size / 1024 / 1024);
```

#### Cache Topology

```rust
use mielin_hal::cache::CacheTopology;

let cache = CacheTopology::detect();

println!("L1 Data: {} KB", cache.l1_data_size / 1024);
println!("L1 Instruction: {} KB", cache.l1_instruction_size / 1024);
println!("L2: {} KB", cache.l2_size / 1024);
println!("L3: {} KB", cache.l3_size / 1024);
println!("Cache line size: {} bytes", cache.line_size);
```

#### Platform Detection

```rust
use mielin_hal::platform::{detect_platform, Platform};

match detect_platform() {
    Platform::RaspberryPi(model) => {
        println!("Running on Raspberry Pi {}", model);
    }
    Platform::Stm32(family) => {
        println!("Running on STM32{}", family);
    }
    Platform::Esp32(variant) => {
        println!("Running on ESP32 {}", variant);
    }
    Platform::Generic => {
        println!("Generic platform");
    }
}
```

#### Power Management

```rust
use mielin_hal::power::{detect_power_info, PowerState};

let power = detect_power_info();
println!("CPU Frequency: {} - {} MHz",
         power.frequency.min_mhz,
         power.frequency.max_mhz);

if power.turbo_supported {
    println!("Turbo boost available");
}

// Get current power state
let state = power.current_state();
match state {
    PowerState::HighPerformance => println!("High performance mode"),
    PowerState::Balanced => println!("Balanced mode"),
    PowerState::PowerSaver => println!("Power saver mode"),
}
```

---

## Tensor Operations API

### mielin-tensor

#### Basic Tensor Operations

```rust
use mielin_tensor::{Tensor, TensorRuntime};
use mielin_hal::capabilities::HardwareProfile;

// Create tensor runtime with hardware detection
let profile = HardwareProfile::detect();
let runtime = TensorRuntime::new(profile.capabilities);

println!("Using: {}", runtime.acceleration_info());

// Create tensors
let a = Tensor::from_vec(vec![1.0, 2.0, 3.0], vec![3]);
let b = Tensor::from_vec(vec![4.0, 5.0, 6.0], vec![3]);

// Perform operations (automatically uses optimal backend)
let sum = &a + &b;  // Element-wise addition
let product = &a * &b;  // Element-wise multiplication
let dot = runtime.ops().dot(&a, &b)?;  // Dot product

println!("Sum: {:?}", sum.data());
println!("Product: {:?}", product.data());
println!("Dot: {}", dot);
```

#### Matrix Operations

```rust
use mielin_tensor::Matrix;

// Create matrices
let m1 = Matrix::from_vec(vec![
    1.0, 2.0,
    3.0, 4.0,
], 2, 2)?;

let m2 = Matrix::from_vec(vec![
    5.0, 6.0,
    7.0, 8.0,
], 2, 2)?;

// Matrix multiplication
let result = m1.matmul(&m2)?;

// Transpose
let transposed = m1.transpose();

// Inverse
let inverse = m1.inverse()?;

// Eigenvalues and eigenvectors
let eigen = m1.eigen()?;
println!("Eigenvalues: {:?}", eigen.values);
```

#### Neural Network Layers

```rust
use mielin_tensor::{Dense, Sequential, Activation};

// Build a neural network
let mut model = Sequential::new();

// Add layers
model.add(Dense::new(784, 128, Some(Activation::ReLU))?);
model.add(Dense::new(128, 64, Some(Activation::ReLU))?);
model.add(Dense::new(64, 10, Some(Activation::Softmax))?);

// Forward pass
let input = Tensor::random(vec![1, 784])?;
let output = model.forward(&input)?;

println!("Output shape: {:?}", output.shape());
```

#### Training with Autograd

```rust
use mielin_tensor::{Variable, ComputeGraph, Adam};

// Create computation graph
let mut graph = ComputeGraph::new();

// Define variables
let x = Variable::new(Tensor::random(vec![100, 10])?);
let y = Variable::new(Tensor::random(vec![100, 1])?);
let w = Variable::new(Tensor::random(vec![10, 1])?);

// Forward pass
let pred = graph.matmul(&x, &w)?;
let loss = graph.mse(&pred, &y)?;

// Backward pass
graph.backward(&loss)?;

// Update weights with optimizer
let mut optimizer = Adam::new(0.001);
optimizer.step(&mut graph)?;
```

#### Quantization

```rust
use mielin_tensor::{QuantizedTensor, QuantScheme, QuantGranularity};

// Create a floating-point tensor
let tensor = Tensor::random(vec![1000, 1000])?;

// Quantize to 8-bit
let quantized = QuantizedTensor::quantize(
    &tensor,
    QuantScheme::PerTensor,
    QuantGranularity::Int8,
)?;

// Memory savings
let original_size = tensor.size_bytes();
let quantized_size = quantized.size_bytes();
println!("Compression: {:.1}x",
         original_size as f32 / quantized_size as f32);

// Dequantize for inference
let dequantized = quantized.dequantize()?;
```

---

## Policy and Security API

### Capability-Based Security

```rust
use mielin_cells::security::{
    Capability, SandboxConfig, SandboxExecutor,
};

// Define sandbox configuration
let config = SandboxConfig {
    granted_capabilities: vec![
        Capability::Network,     // Allow network access
        // FileSystem NOT granted → blocked
    ],
    max_memory_bytes: 64 * 1024 * 1024,  // 64 MB
    max_cpu_time_ms: 5000,                // 5 seconds
    allow_network_egress: true,
};

// Create sandbox
let sandbox = SandboxExecutor::new(config)?;

// Execute agent in sandbox
let result = sandbox.execute(&agent)?;
```

### Authentication and Identity

```rust
use mielin_cells::security::{
    AgentIdentity, IdentityProvider, AuthToken,
};

// Create identity provider
let mut provider = IdentityProvider::new();

// Register agent identity
let identity = AgentIdentity {
    agent_id: agent.id().clone(),
    public_key: agent.public_key(),
    dna_hash: agent.dna().hash(),
};

provider.register(identity)?;

// Authenticate agent
let token = provider.authenticate(&agent)?;

// Verify token
if provider.verify(&token)? {
    println!("Agent authenticated successfully");
}
```

### Encryption

```rust
use mielin_cells::security::{StateEncryptor, EncryptionKey};

// Generate encryption key
let key = EncryptionKey::generate()?;

// Create encryptor
let encryptor = StateEncryptor::new(key);

// Encrypt agent state
let encrypted = encryptor.encrypt(&agent)?;

// Decrypt
let decrypted = encryptor.decrypt(&encrypted)?;
```

### Compliance and Auditing

```rust
use mielin_cells::compliance::{
    AuditLogger, AuditEntry, EventType, CompliancePolicy,
};

// Create audit logger
let mut logger = AuditLogger::new();

// Log events
logger.log(AuditEntry {
    event_type: EventType::AgentCreated,
    agent_id: Some(agent.id().clone()),
    timestamp: chrono::Utc::now(),
    details: "Agent created from template".to_string(),
})?;

// Set compliance policy
let policy = CompliancePolicy {
    retention_days: 90,        // Keep logs for 90 days
    encryption_required: true,  // Encrypt audit logs
    anonymize_pii: true,       // Anonymize personal data
};

logger.set_policy(policy)?;

// Query audit trail
let entries = logger.query()
    .agent_id(agent.id())
    .event_type(EventType::AgentMigrated)
    .since(chrono::Utc::now() - chrono::Duration::days(7))
    .execute()?;
```

---

## Best Practices

### 1. Resource Management

**Always release resources explicitly**:

```rust
// Good: Explicit resource management
let agent = pool.acquire()?;
// Use agent...
pool.release(agent)?;

// Better: Use RAII pattern
{
    let _agent = pool.acquire()?;
    // Agent automatically returned to pool on drop
}
```

### 2. Error Handling

**Handle all possible errors**:

```rust
// Good: Explicit error handling
match agent.start() {
    Ok(()) => println!("Agent started"),
    Err(AgentError::InvalidState(msg)) => {
        eprintln!("Invalid state: {}", msg);
        // Attempt recovery
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
        return Err(e.into());
    }
}

// Avoid: Unwrapping
agent.start().unwrap();  // Don't do this in production!
```

### 3. Migration Best Practices

**Verify state before and after migration**:

```rust
// Capture state hash before migration
let state_hash = agent.state_hash();

// Perform migration
let snapshot = MigrationSnapshot::capture(&agent, None)?;
let bytes = snapshot.serialize()?;

// ... network transfer ...

let restored = MigrationSnapshot::deserialize(&bytes)?;
let new_agent = restored.restore()?;

// Verify state integrity
assert_eq!(state_hash, new_agent.state_hash());
```

### 4. Performance Optimization

**Use hardware-accelerated operations**:

```rust
// Detect capabilities once at startup
let profile = HardwareProfile::detect();
let runtime = TensorRuntime::new(profile.capabilities);

// Reuse runtime for all operations
fn process_data(runtime: &TensorRuntime, data: &Tensor) -> Result<Tensor> {
    runtime.ops().matmul(data, &weights)
}
```

### 5. Security Guidelines

**Principle of least privilege**:

```rust
// Grant only required capabilities
let config = SandboxConfig {
    granted_capabilities: vec![
        Capability::Network,  // Only what's needed
    ],
    max_memory_bytes: 32 * 1024 * 1024,  // Reasonable limit
    max_cpu_time_ms: 1000,               // Timeout
    allow_network_egress: false,         // Restrict by default
};
```

---

## Error Handling Guide

### Error Types

#### Kernel Errors

```rust
use mielin_kernel::KernelError;

fn handle_kernel_error(err: KernelError) {
    match err {
        KernelError::OutOfMemory { requested, available } => {
            eprintln!("OOM: requested {}, available {}", requested, available);
            // Trigger memory reclamation
        }
        KernelError::SchedulerNotInitialized => {
            eprintln!("Scheduler not initialized");
            // Initialize scheduler
        }
        _ => eprintln!("Kernel error: {}", err),
    }
}
```

#### Agent Errors

```rust
use mielin_cells::AgentError;

fn handle_agent_error(err: AgentError) {
    match err {
        AgentError::InvalidWasmBinary(msg) => {
            eprintln!("Invalid WASM: {}", msg);
            // Reject agent
        }
        AgentError::PolicyViolation(policy) => {
            eprintln!("Policy violated: {:?}", policy);
            // Enforce policy
        }
        _ => eprintln!("Agent error: {}", err),
    }
}
```

#### Network Errors

```rust
use mielin_mesh::MeshError;

fn handle_network_error(err: MeshError) {
    match err {
        MeshError::ConnectionFailed(addr) => {
            eprintln!("Connection failed to {}", addr);
            // Retry with backoff
        }
        MeshError::PeerTimeout(peer_id) => {
            eprintln!("Peer timeout: {}", peer_id);
            // Remove from routing table
        }
        _ => eprintln!("Network error: {}", err),
    }
}
```

### Error Propagation

```rust
use anyhow::{Context, Result};

fn complex_operation() -> Result<()> {
    let agent = Agent::new(wasm_binary)
        .context("Failed to create agent")?;

    agent.start()
        .context("Failed to start agent")?;

    let snapshot = MigrationSnapshot::capture(&agent, None)
        .context("Failed to capture migration snapshot")?;

    Ok(())
}
```

---

## Migration from Other Systems

### From Kubernetes

**Kubernetes Pod → MielinOS Agent**:

```rust
// Kubernetes YAML:
// apiVersion: v1
// kind: Pod
// spec:
//   containers:
//   - name: app
//     image: myapp:latest
//     resources:
//       limits:
//         memory: "128Mi"
//         cpu: "500m"

// MielinOS equivalent:
let policy = Policy {
    resource_limits: Some(ResourceRequirements {
        max_memory_mb: 128,
        max_cpu_cores: 0.5,
    }),
    ..Default::default()
};

let agent = Agent::with_policy(wasm_binary, policy);
```

### From Docker

**Docker Container → WASM Agent**:

```bash
# Build WASM from existing application
cargo build --target wasm32-wasi --release

# Optimize WASM
wasm-opt -Oz target/wasm32-wasi/release/app.wasm -o agent.wasm
```

```rust
// Deploy as MielinOS agent
let wasm_binary = std::fs::read("agent.wasm")?;
let agent = Agent::new(wasm_binary);
mesh.deploy_agent(agent).await?;
```

### From AWS Lambda

**Lambda Function → Stateful Agent**:

```rust
// AWS Lambda is stateless, MielinOS agents are stateful
// Lambda state must be externalized (DynamoDB, S3)
// MielinOS maintains state in agent memory

// Lambda handler equivalent:
pub fn handler(event: Event) -> Result<Response> {
    // Load state from agent memory
    let state = agent.load_state()?;

    // Process event
    let result = process(event, state)?;

    // Save state back to agent
    agent.save_state(state)?;

    Ok(result)
}
```

### From ROS (Robot Operating System)

**ROS Node → MielinOS Agent**:

```rust
// ROS publishes to topics, MielinOS uses message bus
use mielin_cells::messaging::{MessageBus, Topic};

let mut bus = MessageBus::new();

// Subscribe to topic
bus.subscribe(Topic::new("sensor_data"), |msg| {
    println!("Received: {:?}", msg);
})?;

// Publish message
bus.publish(Topic::new("actuator_cmd"), command)?;
```

---

## Code Examples

### Complete Agent Deployment

```rust
use mielin_cells::{Agent, Policy};
use mielin_mesh::{MeshService, MeshConfig, NodeRole};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize mesh network
    let config = MeshConfig {
        listen_addr: "0.0.0.0:5000".parse()?,
        bootstrap_nodes: vec![],
        node_role: NodeRole::Core,
    };

    let mut mesh = MeshService::new(config)?;
    mesh.start().await?;

    // Load and deploy agent
    let wasm_binary = std::fs::read("agent.wasm")?;
    let policy = Policy {
        battery_threshold: Some(20),
        max_latency_ms: Some(100),
        ..Default::default()
    };

    let mut agent = Agent::with_policy(wasm_binary, policy);
    agent.start()?;

    // Register with mesh
    mesh.register_agent(&agent).await?;

    println!("Agent {} deployed", agent.id());

    Ok(())
}
```

### Agent Migration Example

```rust
use mielin_cells::migration::MigrationSnapshot;
use mielin_mesh::MeshService;
use anyhow::Result;

async fn migrate_agent(
    agent: &Agent,
    source_mesh: &MeshService,
    target_node_id: &NodeId,
) -> Result<()> {
    // Capture snapshot
    let snapshot = MigrationSnapshot::capture(agent, None)?;

    // Serialize
    let bytes = snapshot.serialize()?;

    // Send to target node
    source_mesh.send_migration(target_node_id, bytes).await?;

    // Wait for acknowledgment
    let ack = source_mesh.await_migration_ack(agent.id()).await?;

    if ack.success {
        println!("Migration successful");
        // Terminate agent on source
        agent.terminate()?;
    } else {
        println!("Migration failed: {:?}", ack.error_msg);
    }

    Ok(())
}
```

---

## API Versioning

All APIs follow semantic versioning (SemVer):

- **Major version** (0.x.y → 1.0.0): Breaking changes
- **Minor version** (0.0.x → 0.1.0): New features, backward compatible
- **Patch version** (0.0.0 → 0.0.1): Bug fixes

Current version: **0.1.0-rc.1**

---

## Further Reading

- [Architecture Guide](ARCHITECTURE.md) - System architecture
- [Performance Guide](PERFORMANCE.md) - Optimization techniques
- [Integration Guide](INTEGRATION.md) - Building distributed apps
- [Developer Guide](DEVELOPER.md) - Contributing to MielinOS

---

**MielinOS API Reference** - Version 1.0 - 2026-01-17
