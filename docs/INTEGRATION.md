# MielinOS Integration Guide

## Table of Contents

1. [Integration Overview](#integration-overview)
2. [Component Integration](#component-integration)
3. [Building Distributed Applications](#building-distributed-applications)
4. [Deployment Strategies](#deployment-strategies)
5. [Monitoring and Observability](#monitoring-and-observability)
6. [Troubleshooting](#troubleshooting)
7. [Production Best Practices](#production-best-practices)
8. [Example Architectures](#example-architectures)

---

## Integration Overview

MielinOS is designed to integrate seamlessly into heterogeneous computing environments, from IoT edge devices to cloud clusters.

### Integration Patterns

```
┌────────────────────────────────────────────────┐
│                 Cloud Layer                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ Core Node│  │ Core Node│  │ Core Node│    │
│  │ (AWS)    │  │ (GCP)    │  │ (Azure)  │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘    │
└───────┼─────────────┼─────────────┼───────────┘
        │             │             │
┌───────┼─────────────┼─────────────┼───────────┐
│       │      Edge Computing       │           │
│  ┌────▼─────┐  ┌───▼──────┐  ┌───▼──────┐   │
│  │ Relay    │  │ Relay    │  │ Relay    │   │
│  │ (RasPi)  │  │ (RasPi)  │  │ (RasPi)  │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
└───────┼─────────────┼─────────────┼───────────┘
        │             │             │
┌───────┼─────────────┼─────────────┼───────────┐
│       │       IoT Devices         │           │
│  ┌────▼─────┐  ┌───▼──────┐  ┌───▼──────┐   │
│  │ Edge Node│  │ Edge Node│  │ Edge Node│   │
│  │ (ESP32)  │  │ (STM32)  │  │ (Cortex-M)│  │
│  └──────────┘  └──────────┘  └──────────┘   │
└────────────────────────────────────────────────┘
```

### Key Integration Points

1. **Hardware Layer**: CPU, GPU, NPU detection via `mielin-hal`
2. **Network Layer**: QUIC, DHT, service discovery via `mielin-mesh`
3. **Agent Layer**: Lifecycle, migration, policy via `mielin-cells`
4. **Application Layer**: Custom agents, APIs, integrations

---

## Component Integration

### Integrating mielin-kernel

The kernel provides the foundation for all other components.

#### Standalone Kernel Usage

```rust
use mielin_kernel::{kernel_init, memory, scheduler};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize kernel subsystems
    kernel_init()?;

    // Use kernel services
    let pages = memory::allocate_pages(10)?;

    // Spawn tasks
    let task_id = scheduler::spawn_task(
        scheduler::Priority::Normal,
        || {
            println!("Task running");
        }
    )?;

    Ok(())
}
```

#### Kernel with Custom Allocator

```rust
use mielin_kernel::memory::PageAllocator;

// Create custom allocator
struct MyAllocator {
    page_allocator: PageAllocator,
}

impl MyAllocator {
    fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        let pages = (size + 4095) / 4096;
        match self.page_allocator.allocate(pages) {
            Ok(addr) => Some(addr as *mut u8),
            Err(_) => None,
        }
    }
}
```

### Integrating mielin-hal

Hardware abstraction enables portable code across architectures.

#### Cross-Platform Application

```rust
use mielin_hal::{detect_architecture, capabilities::HardwareProfile};

fn main() {
    let arch = detect_architecture();
    let profile = HardwareProfile::detect();

    println!("Architecture: {}", arch);
    println!("Cores: {}", profile.core_count);

    // Use architecture-specific optimizations
    match arch {
        Architecture::AArch64 => {
            if profile.capabilities.contains(HardwareCapabilities::SVE2) {
                use_sve2_optimizations();
            }
        }
        Architecture::X86_64 => {
            if profile.capabilities.contains(HardwareCapabilities::AVX512) {
                use_avx512_optimizations();
            }
        }
        _ => use_generic_implementation(),
    }
}
```

#### Platform-Specific Features

```rust
use mielin_hal::platform::{detect_platform, Platform};

match detect_platform() {
    Platform::RaspberryPi(model) => {
        // Enable GPIO, I2C, SPI
        enable_raspberry_pi_peripherals(model)?;
    }
    Platform::Esp32(variant) => {
        // Enable WiFi, Bluetooth
        enable_esp32_wireless(variant)?;
    }
    Platform::Stm32(family) => {
        // Enable embedded peripherals
        enable_stm32_peripherals(family)?;
    }
    _ => {
        // Generic platform
    }
}
```

### Integrating mielin-cells

Agent management for building distributed applications.

#### Basic Agent Deployment

```rust
use mielin_cells::{Agent, AgentPool, PoolConfig};

// Create agent pool
let config = PoolConfig {
    min_size: 10,
    max_size: 100,
    idle_timeout_secs: 300,
};
let mut pool = AgentPool::new(config);

// Deploy agents
let wasm_binary = std::fs::read("agent.wasm")?;
for i in 0..10 {
    let agent = Agent::new(wasm_binary.clone());
    agent.start()?;
    pool.add(agent)?;
}
```

#### Agent with Custom Policy

```rust
use mielin_cells::{Agent, Policy, ResourceRequirements};

let policy = Policy {
    battery_threshold: Some(20),
    max_latency_ms: Some(100),
    resource_requirements: Some(ResourceRequirements {
        min_memory_mb: 64,
        max_memory_mb: 256,
        min_cpu_cores: 1,
    }),
    migration_enabled: true,
    ..Default::default()
};

let agent = Agent::with_policy(wasm_binary, policy);
```

### Integrating mielin-mesh

Networking layer for distributed communication.

#### Setting up Mesh Network

```rust
use mielin_mesh::{MeshService, MeshConfig, NodeRole};

#[tokio::main]
async fn main() -> Result<()> {
    // Configure mesh
    let config = MeshConfig {
        listen_addr: "0.0.0.0:5000".parse()?,
        bootstrap_nodes: vec![
            "bootstrap1.example.com:5000".parse()?,
            "bootstrap2.example.com:5000".parse()?,
        ],
        node_role: NodeRole::Edge,
    };

    // Start mesh service
    let mut mesh = MeshService::new(config)?;
    mesh.start().await?;

    // Register agents
    for agent in agents {
        mesh.register_agent(&agent).await?;
    }

    Ok(())
}
```

#### Multi-Node Setup

**Node 1 (Core)**:
```rust
let config = MeshConfig {
    listen_addr: "0.0.0.0:5000".parse()?,
    bootstrap_nodes: vec![],  // Core node is bootstrap
    node_role: NodeRole::Core,
};
```

**Node 2 (Relay)**:
```rust
let config = MeshConfig {
    listen_addr: "0.0.0.0:5001".parse()?,
    bootstrap_nodes: vec!["node1:5000".parse()?],
    node_role: NodeRole::Relay,
};
```

**Node 3 (Edge)**:
```rust
let config = MeshConfig {
    listen_addr: "0.0.0.0:5002".parse()?,
    bootstrap_nodes: vec!["node1:5000".parse()?, "node2:5001".parse()?],
    node_role: NodeRole::Edge,
};
```

---

## Building Distributed Applications

### Example: Distributed Data Processing Pipeline

```rust
use mielin_cells::{Agent, MessageBus, Topic};
use mielin_mesh::MeshService;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup mesh network
    let mesh = setup_mesh().await?;

    // 2. Create message bus
    let mut bus = MessageBus::new();

    // 3. Define pipeline stages
    let ingestion_agent = create_ingestion_agent()?;
    let processing_agent = create_processing_agent()?;
    let storage_agent = create_storage_agent()?;

    // 4. Connect pipeline with message passing
    bus.subscribe(Topic::new("raw_data"), move |msg| {
        processing_agent.send(msg)?;
        Ok(())
    })?;

    bus.subscribe(Topic::new("processed_data"), move |msg| {
        storage_agent.send(msg)?;
        Ok(())
    })?;

    // 5. Start agents
    mesh.deploy_agent(ingestion_agent).await?;
    mesh.deploy_agent(processing_agent).await?;
    mesh.deploy_agent(storage_agent).await?;

    // 6. Run pipeline
    tokio::signal::ctrl_c().await?;

    Ok(())
}

fn create_ingestion_agent() -> Result<Agent> {
    // WASM agent that reads data from source
    let wasm = std::fs::read("ingestion.wasm")?;
    Ok(Agent::new(wasm))
}

fn create_processing_agent() -> Result<Agent> {
    // WASM agent that processes data
    let wasm = std::fs::read("processing.wasm")?;
    Ok(Agent::new(wasm))
}

fn create_storage_agent() -> Result<Agent> {
    // WASM agent that stores results
    let wasm = std::fs::read("storage.wasm")?;
    Ok(Agent::new(wasm))
}
```

### Example: IoT Sensor Network

```rust
use mielin_cells::Agent;
use mielin_mesh::MeshService;
use mielin_rt::power::{PowerManager, PowerMode};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize power management
    let mut power = PowerManager::new();
    power.set_mode(PowerMode::LowPower)?;

    // Setup mesh
    let mesh = setup_mesh().await?;

    // Deploy sensor agents
    let temp_sensor = create_sensor_agent("temperature")?;
    let humidity_sensor = create_sensor_agent("humidity")?;
    let motion_sensor = create_sensor_agent("motion")?;

    mesh.deploy_agent(temp_sensor).await?;
    mesh.deploy_agent(humidity_sensor).await?;
    mesh.deploy_agent(motion_sensor).await?;

    // Monitor battery
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        let battery_level = power.battery_level()?;
        if battery_level < 20 {
            // Trigger migration to powered node
            mesh.migrate_all_agents_to("powered_node").await?;
            break;
        }
    }

    Ok(())
}
```

### Example: Edge AI Inference

```rust
use mielin_tensor::{TensorRuntime, Tensor};
use mielin_hal::capabilities::HardwareProfile;
use mielin_cells::Agent;

#[tokio::main]
async fn main() -> Result<()> {
    // Detect hardware
    let profile = HardwareProfile::detect();
    let runtime = TensorRuntime::new(profile.capabilities);

    println!("Using: {}", runtime.acceleration_info());

    // Load ML model
    let model = load_model("model.onnx")?;

    // Create inference agent
    let agent = create_inference_agent(model)?;
    agent.start()?;

    // Process stream of inputs
    loop {
        let input = receive_input().await?;

        // Perform inference
        let tensor = Tensor::from_vec(input, vec![1, 224, 224, 3]);
        let output = runtime.ops().matmul(&tensor, &model.weights)?;

        // Send result
        send_output(output).await?;
    }
}
```

---

## Deployment Strategies

### Single-Node Deployment

**Use Case**: Development, testing, small-scale applications

```rust
use mielin_cells::{Agent, AgentPool};

fn main() -> Result<()> {
    // Create agent pool
    let mut pool = AgentPool::new(PoolConfig::default());

    // Load agents
    for wasm_file in ["agent1.wasm", "agent2.wasm", "agent3.wasm"] {
        let wasm = std::fs::read(wasm_file)?;
        let agent = Agent::new(wasm);
        agent.start()?;
        pool.add(agent)?;
    }

    // Run
    loop {
        // Process events
    }
}
```

### Multi-Node Cluster

**Use Case**: Production, high availability, distributed workloads

```rust
use mielin_mesh::{MeshService, NodeRole};
use mielin_cells::orchestration::Orchestrator;

#[tokio::main]
async fn main() -> Result<()> {
    // Determine node role from environment
    let role = std::env::var("NODE_ROLE")
        .map(|r| match r.as_str() {
            "core" => NodeRole::Core,
            "relay" => NodeRole::Relay,
            _ => NodeRole::Edge,
        })
        .unwrap_or(NodeRole::Edge);

    // Start mesh
    let mesh = MeshService::new(MeshConfig {
        listen_addr: "0.0.0.0:5000".parse()?,
        bootstrap_nodes: get_bootstrap_nodes()?,
        node_role: role,
    })?;
    mesh.start().await?;

    // Create orchestrator
    let mut orchestrator = Orchestrator::new(mesh);

    // Deploy agents with placement constraints
    orchestrator.deploy_with_constraints(
        agents,
        PlacementConstraint {
            node_role: Some(NodeRole::Core),
            min_memory_mb: Some(512),
            required_capabilities: vec!["gpu"],
        }
    ).await?;

    Ok(())
}

fn get_bootstrap_nodes() -> Result<Vec<SocketAddr>> {
    let nodes = std::env::var("BOOTSTRAP_NODES")?;
    nodes.split(',')
        .map(|s| s.parse())
        .collect()
}
```

### Kubernetes Integration

**Deployment YAML**:

```yaml
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: mielin-edge
spec:
  selector:
    matchLabels:
      app: mielin-edge
  template:
    metadata:
      labels:
        app: mielin-edge
    spec:
      hostNetwork: true
      containers:
      - name: mielin
        image: mielin/edge:latest
        env:
        - name: NODE_ROLE
          value: "edge"
        - name: BOOTSTRAP_NODES
          value: "mielin-core-0.mielin-core:5000,mielin-core-1.mielin-core:5000"
        ports:
        - containerPort: 5000
          protocol: UDP
        resources:
          requests:
            memory: "256Mi"
            cpu: "500m"
          limits:
            memory: "512Mi"
            cpu: "1000m"
```

**Service YAML**:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: mielin-core
spec:
  clusterIP: None
  selector:
    app: mielin-core
  ports:
  - port: 5000
    protocol: UDP
```

### Docker Compose

```yaml
version: '3.8'

services:
  core-1:
    image: mielin/core:latest
    environment:
      NODE_ROLE: core
      BOOTSTRAP_NODES: ""
    ports:
      - "5000:5000/udp"
    networks:
      - mielin-net

  core-2:
    image: mielin/core:latest
    environment:
      NODE_ROLE: core
      BOOTSTRAP_NODES: "core-1:5000"
    ports:
      - "5001:5000/udp"
    networks:
      - mielin-net

  relay-1:
    image: mielin/relay:latest
    environment:
      NODE_ROLE: relay
      BOOTSTRAP_NODES: "core-1:5000,core-2:5000"
    ports:
      - "5002:5000/udp"
    networks:
      - mielin-net

  edge-1:
    image: mielin/edge:latest
    environment:
      NODE_ROLE: edge
      BOOTSTRAP_NODES: "core-1:5000,relay-1:5000"
    ports:
      - "5003:5000/udp"
    networks:
      - mielin-net

networks:
  mielin-net:
    driver: bridge
```

---

## Monitoring and Observability

### Metrics Collection

```rust
use mielin_mesh::metrics::MetricsRegistry;
use mielin_mesh::export::PrometheusExporter;

#[tokio::main]
async fn main() -> Result<()> {
    // Create metrics registry
    let registry = MetricsRegistry::new();

    // Export to Prometheus
    let exporter = PrometheusExporter::new(registry.clone());
    exporter.start("0.0.0.0:9090").await?;

    // Use registry
    let timer = registry.start_timer("agent_migration");
    // ... perform migration ...
    timer.stop();

    Ok(())
}
```

### Distributed Tracing

```rust
use mielin_mesh::tracing::{TraceCollector, TraceContext};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize trace collector
    let collector = TraceCollector::new(TraceCollectorConfig::default());

    // Start tracing
    let ctx = TraceContext::new();
    let span = ctx.start_span("migration");

    // Perform operation
    migrate_agent(&agent, &target).await?;

    // End span
    span.end();

    // Export traces
    let traces = collector.collect();
    export_trace_otel(&traces, "http://jaeger:14268/api/traces").await?;

    Ok(())
}
```

### Logging

```rust
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

fn main() {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    // Use logging
    info!("Starting MielinOS node");
    warn!("Low battery: {}%", battery_level);
    error!("Migration failed: {}", error);
}
```

### Health Checks

```rust
use mielin_mesh::service_discovery::ServiceHealth;

#[tokio::main]
async fn main() -> Result<()> {
    // Implement health check
    let health = ServiceHealth::new(|| async {
        // Check if node is healthy
        if is_healthy() {
            Ok(())
        } else {
            Err("Node unhealthy".into())
        }
    });

    // Expose health endpoint
    health.serve("0.0.0.0:8080").await?;

    Ok(())
}
```

---

## Troubleshooting

### Common Issues

#### 1. Agent Migration Failures

**Symptom**: Migration timeouts or errors

**Diagnosis**:
```rust
use tracing::debug;

debug!("Snapshot size: {} bytes", snapshot.size());
debug!("Network latency: {} ms", latency);
debug!("Target node load: {}", target_load);
```

**Solutions**:
- Reduce agent state size
- Increase network timeout
- Check target node capacity
- Verify network connectivity

#### 2. Memory Exhaustion

**Symptom**: OutOfMemory errors

**Diagnosis**:
```rust
use mielin_kernel::memory;

let available = memory::available_pages();
let used = memory::used_pages();
println!("Memory: {} / {} pages used", used, used + available);
```

**Solutions**:
- Increase kernel page pool size
- Reduce concurrent agents
- Enable agent eviction
- Use smaller agents

#### 3. Network Partition

**Symptom**: Agents unreachable

**Diagnosis**:
```rust
use mielin_mesh::partition::PartitionDetector;

let detector = PartitionDetector::new();
if let Some(partition) = detector.detect().await? {
    println!("Partition detected: {:?}", partition);
}
```

**Solutions**:
- Enable partition healing
- Use multi-region replication
- Implement split-brain detection
- Configure quorum policies

#### 4. Performance Degradation

**Symptom**: High latency, low throughput

**Diagnosis**:
```bash
# Check CPU usage
top -bn1 | grep mielin

# Check network usage
iftop -i eth0

# Profile application
perf record -g ./mielin-node
perf report
```

**Solutions**:
- Enable SIMD optimizations
- Increase thread pool size
- Optimize hot paths
- Use connection pooling

### Debugging Tools

#### Enable Debug Logging

```rust
RUST_LOG=debug cargo run
```

#### Remote Debugging

```rust
use mielin_cells::debug::{RemoteDebugger, DebuggerConfig};

let debugger = RemoteDebugger::new(DebuggerConfig {
    listen_addr: "0.0.0.0:9000".parse()?,
    enable_breakpoints: true,
    enable_profiling: true,
});

debugger.start().await?;
```

#### Heap Profiling

```bash
cargo install dhat
cargo run --features dhat
```

---

## Production Best Practices

### 1. Security Hardening

```rust
use mielin_cells::security::{SandboxConfig, Capability};

let config = SandboxConfig {
    // Principle of least privilege
    granted_capabilities: vec![Capability::Network],

    // Resource limits
    max_memory_bytes: 64 * 1024 * 1024,
    max_cpu_time_ms: 5000,

    // Network restrictions
    allow_network_egress: false,
    allowed_hosts: vec!["api.example.com".to_string()],
};
```

### 2. High Availability

```rust
use mielin_cells::ha::{FailoverConfig, FailoverPolicy};

let config = FailoverConfig {
    replication_factor: 3,
    health_check_interval_secs: 10,
    failover_timeout_secs: 30,
    policy: FailoverPolicy::Automatic,
};
```

### 3. Data Retention and Compliance

```rust
use mielin_cells::compliance::{CompliancePolicy, RetentionPolicy};

let policy = CompliancePolicy {
    retention: RetentionPolicy::Days(90),
    encryption_required: true,
    anonymize_pii: true,
    audit_all_access: true,
};
```

### 4. Disaster Recovery

```rust
use mielin_cells::dr::{BackupConfig, BackupSchedule};

let config = BackupConfig {
    schedule: BackupSchedule::Hourly,
    retention_count: 24,
    verify_backups: true,
    compression: true,
};
```

### 5. Capacity Planning

```rust
use mielin_cells::orchestration::ScalingPolicy;

let policy = ScalingPolicy {
    min_replicas: 3,
    max_replicas: 100,
    target_cpu_utilization: 70,
    scale_up_threshold: 80,
    scale_down_threshold: 30,
    cooldown_period_secs: 300,
};
```

---

## Example Architectures

### Architecture 1: Smart City IoT Network

```
┌─────────────────────────────────────────────┐
│           Cloud Control Plane               │
│  ┌────────┐  ┌────────┐  ┌────────┐        │
│  │ Core   │  │ Core   │  │ Core   │        │
│  │ (AWS)  │  │ (GCP)  │  │ (Azure)│        │
│  └───┬────┘  └───┬────┘  └───┬────┘        │
└──────┼───────────┼───────────┼─────────────┘
       │           │           │
┌──────▼───────────▼───────────▼─────────────┐
│          City-Wide Mesh Network             │
│  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐   │
│  │ Relay│  │ Relay│  │ Relay│  │ Relay│   │
│  │ (RPI)│  │ (RPI)│  │ (RPI)│  │ (RPI)│   │
│  └──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘   │
└─────┼─────────┼─────────┼─────────┼────────┘
      │         │         │         │
┌─────▼─────────▼─────────▼─────────▼────────┐
│        IoT Sensors (Edge Nodes)             │
│  Traffic │  Air    │  Noise  │  Weather    │
│  Lights  │  Quality│  Monitor│  Station    │
└─────────────────────────────────────────────┘
```

**Features**:
- Real-time traffic optimization
- Air quality monitoring
- Automated street lighting
- Weather prediction

### Architecture 2: Distributed ML Training

```
┌─────────────────────────────────────────────┐
│          Parameter Server (Core)            │
│     ┌────────────────────────────┐          │
│     │  Global Model Aggregation  │          │
│     └────────────────────────────┘          │
└──────────┬────────┬────────┬────────────────┘
           │        │        │
    ┌──────▼──┐ ┌──▼─────┐ ┌▼────────┐
    │ Worker  │ │ Worker │ │ Worker  │
    │ (GPU)   │ │ (GPU)  │ │ (GPU)   │
    └─────────┘ └────────┘ └─────────┘
```

**Features**:
- Federated learning
- Gradient aggregation
- Model versioning
- Privacy-preserving training

### Architecture 3: Edge AI Inference Pipeline

```
┌─────────────────────────────────────────────┐
│              Camera Sources                 │
│  📷 Camera 1  📷 Camera 2  📷 Camera 3      │
└──────┬──────────┬──────────┬────────────────┘
       │          │          │
┌──────▼──────────▼──────────▼────────────────┐
│        Edge Inference Nodes                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
│  │ YOLO     │  │ ResNet   │  │ MobileNet│  │
│  │ Detector │  │ Classify │  │ Segment  │  │
│  └─────┬────┘  └─────┬────┘  └─────┬────┘  │
└────────┼─────────────┼─────────────┼────────┘
         │             │             │
    ┌────▼─────────────▼─────────────▼────┐
    │       Central Analytics Node        │
    │     (Aggregation & Alerting)        │
    └─────────────────────────────────────┘
```

**Features**:
- Real-time object detection
- Distributed inference
- Alert generation
- Analytics aggregation

---

## Conclusion

MielinOS provides flexible integration options for building distributed applications across heterogeneous hardware. Follow this guide to:

1. Integrate MielinOS components into your application
2. Build distributed pipelines and networks
3. Deploy to production environments
4. Monitor and troubleshoot issues
5. Follow best practices for security and reliability

For more information, see:
- [Architecture Guide](ARCHITECTURE.md)
- [API Reference](API.md)
- [Performance Guide](PERFORMANCE.md)
- [Developer Guide](DEVELOPER.md)

---

**MielinOS Integration Guide** - Version 1.0 - 2026-01-17
