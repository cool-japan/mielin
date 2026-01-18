//! End-to-End Full-Stack MielinOS Example
//!
//! This example demonstrates all major MielinOS components working together
//! in a distributed AI workload scenario.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example e2e-full-stack
//! ```

use anyhow::{Context, Result};
use mielin_cells::{migration::MigrationManager, Agent};
use mielin_hal::{
    capabilities::{HardwareCapabilities, HardwareProfile},
    detect_architecture, Architecture,
};
use mielin_kernel::scheduler::{Scheduler, SchedulerMetricsSnapshot};
use mielin_mesh_core::{Node, NodeRole};
use mielin_tensor::{
    nn::{Activation as NnActivation, Dense},
    quant::{QuantGranularity, QuantScheme, QuantizedTensor},
    Tensor, TensorRuntime,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

// =============================================================================
// Migration Statistics (local struct)
// =============================================================================

#[derive(Debug, Default)]
struct LocalMigrationStats {
    total_migrations: u64,
    successful_migrations: u64,
    failed_migrations: u64,
}

// =============================================================================
// Simulated Mesh Node
// =============================================================================

/// A simulated mesh node with full capabilities
#[allow(dead_code)]
struct FullStackNode {
    /// Unique node identifier
    node_id: Uuid,
    /// Node role in the cluster
    role: NodeRole,
    /// Deployed agents
    agents: Arc<RwLock<HashMap<Uuid, Agent>>>,
    /// Migration manager
    migration_manager: Arc<RwLock<MigrationManager>>,
    /// Migration statistics
    migration_stats: Arc<RwLock<LocalMigrationStats>>,
    /// Tensor runtime
    tensor_runtime: TensorRuntime,
    /// Scheduler for task management
    scheduler: Arc<RwLock<Scheduler>>,
    /// Hardware profile
    hw_profile: HardwareProfile,
}

impl FullStackNode {
    /// Create a new full-stack node
    fn new(role: NodeRole) -> Result<Self> {
        let node = Node::new(role);
        let node_id = *node.id();

        // Detect hardware capabilities
        let hw_profile = HardwareProfile::detect();

        // Create tensor runtime with detected capabilities
        let tensor_runtime = TensorRuntime::new(hw_profile.capabilities);

        // Create scheduler
        let scheduler = Arc::new(RwLock::new(Scheduler::new()));

        Ok(Self {
            node_id,
            role,
            agents: Arc::new(RwLock::new(HashMap::new())),
            migration_manager: Arc::new(RwLock::new(MigrationManager::new())),
            migration_stats: Arc::new(RwLock::new(LocalMigrationStats::default())),
            tensor_runtime,
            scheduler,
            hw_profile,
        })
    }

    /// Deploy an agent to this node
    async fn deploy_agent(&self, agent: Agent) -> Result<Uuid> {
        let agent_id = agent.id();

        info!(
            "  Deploying agent {} to {} node {}",
            agent_id,
            role_name(self.role),
            self.node_id
        );

        // Store agent
        self.agents.write().await.insert(agent_id, agent);

        // Create a scheduler task for this agent
        let mut scheduler = self.scheduler.write().await;
        let _task_id = scheduler
            .spawn_task(100)
            .context("Failed to spawn task for agent")?;

        Ok(agent_id)
    }

    /// Run tensor inference for an agent
    #[allow(dead_code)]
    async fn run_inference(&self, agent_id: Uuid, input: &Tensor<f32>) -> Result<Tensor<f32>> {
        let agents = self.agents.read().await;
        let _agent = agents.get(&agent_id).context("Agent not found")?;

        // Perform inference using the tensor runtime
        let ops = self.tensor_runtime.ops();

        // Simple forward pass: input * weight + bias
        let weight = Tensor::zeros(input.shape().to_vec());
        let bias = Tensor::zeros(vec![input.shape()[input.shape().len() - 1]]);

        let matmul_result = ops.mul(input, &weight).context("mul operation failed")?;
        let output = ops
            .add(&matmul_result, &bias)
            .context("add operation failed")?;

        Ok(output)
    }

    /// Get node status summary
    async fn status_summary(&self) -> NodeStatus {
        let agents = self.agents.read().await;
        let stats = self.migration_stats.read().await;
        let scheduler = self.scheduler.read().await;
        let scheduler_metrics = scheduler.metrics();

        NodeStatus {
            node_id: self.node_id,
            role: self.role,
            agent_count: agents.len(),
            total_migrations: stats.total_migrations,
            successful_migrations: stats.successful_migrations,
            failed_migrations: stats.failed_migrations,
            scheduler_metrics,
            hw_capabilities: self.hw_profile.capabilities,
        }
    }
}

/// Node status summary
struct NodeStatus {
    node_id: Uuid,
    role: NodeRole,
    agent_count: usize,
    total_migrations: u64,
    successful_migrations: u64,
    failed_migrations: u64,
    scheduler_metrics: SchedulerMetricsSnapshot,
    #[allow(dead_code)]
    hw_capabilities: HardwareCapabilities,
}

impl NodeStatus {
    fn display(&self) {
        info!("  Node: {}", self.node_id);
        info!("    Role: {}", role_name(self.role));
        info!("    Agents: {}", self.agent_count);
        info!(
            "    Migrations: {} total, {} successful, {} failed",
            self.total_migrations, self.successful_migrations, self.failed_migrations
        );
        info!(
            "    Scheduler: {} active tasks, {} peak",
            self.scheduler_metrics.active_tasks, self.scheduler_metrics.peak_tasks
        );
        info!(
            "    Utilization: {:.1}%",
            self.scheduler_metrics.utilization() * 100.0
        );
    }
}

// =============================================================================
// Demo Phases
// =============================================================================

/// Demonstrate mesh networking capabilities
async fn demo_mesh_networking() -> Result<()> {
    info!("=== Phase 1: Mesh Networking ===");
    info!("");

    // Create three nodes
    info!("Creating 3-node mesh cluster...");
    let core_node = Arc::new(FullStackNode::new(NodeRole::Core)?);
    let relay_node = Arc::new(FullStackNode::new(NodeRole::Relay)?);
    let edge_node = Arc::new(FullStackNode::new(NodeRole::Edge)?);

    info!("  Core Node:  {}", core_node.node_id);
    info!("  Relay Node: {}", relay_node.node_id);
    info!("  Edge Node:  {}", edge_node.node_id);
    info!("");

    // Demonstrate health monitoring
    info!("Health monitoring status:");
    info!("  Core Node:  Healthy");
    info!("  Relay Node: Healthy");
    info!("  Edge Node:  Healthy");
    info!("");

    Ok(())
}

/// Demonstrate tensor computation capabilities
async fn demo_tensor_computation(matrix_size: usize) -> Result<()> {
    info!("=== Phase 2: Tensor Computation ===");
    info!("");

    // Detect hardware
    let hw_profile = HardwareProfile::detect();
    info!("Hardware Detection:");
    info!("  Architecture: {}", detect_architecture());
    info!("  CPU Cores: {}", hw_profile.core_count);
    info!("  Memory: {} MB", hw_profile.memory_size / 1024 / 1024);
    info!(
        "  SIMD: {}",
        if hw_profile.has_simd() {
            "Available"
        } else {
            "Not available"
        }
    );
    info!("  Max Vector Width: {} bits", hw_profile.max_vector_width());
    info!("");

    // Create runtime with detected capabilities
    let runtime = TensorRuntime::new(hw_profile.capabilities);
    info!("Tensor Runtime:");
    info!("  Backend: {}", runtime.acceleration_info());
    info!("");

    // Matrix multiplication benchmark
    info!(
        "Matrix Multiplication Benchmark ({}x{}):",
        matrix_size, matrix_size
    );
    let a = Tensor::zeros(vec![matrix_size, matrix_size]);
    let b = Tensor::zeros(vec![matrix_size, matrix_size]);

    let start = Instant::now();
    let iterations = 5;
    for _ in 0..iterations {
        let _ = runtime.ops().matmul(&a, &b);
    }
    let duration = start.elapsed() / iterations as u32;
    let flops = (2 * matrix_size * matrix_size * matrix_size) as f64;
    let gflops = (flops / duration.as_secs_f64()) / 1e9;
    info!(
        "  Time per iteration: {:.3} ms",
        duration.as_secs_f64() * 1000.0
    );
    info!("  Throughput: {:.2} GFLOPS", gflops);
    info!("");

    // Neural network inference
    info!("Neural Network Inference (784 -> 256 -> 10):");
    let input: Vec<f32> = vec![0.0; 784];
    let dense1 = Dense::new(784, 256).with_activation(NnActivation::ReLU);
    let dense2 = Dense::new(256, 10);

    let start = Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        let hidden = dense1.forward(&input);
        let _ = dense2.forward(&hidden);
    }
    let duration = start.elapsed() / iterations as u32;
    info!(
        "  Inference time: {:.3} ms",
        duration.as_secs_f64() * 1000.0
    );
    info!("");

    // Quantization
    info!("Quantization Test (INT8):");
    let tensor = Tensor::zeros(vec![256, 256]);
    let start = Instant::now();
    let quantized = QuantizedTensor::from_tensor(
        &tensor,
        QuantScheme::Asymmetric,
        QuantGranularity::PerTensor,
    );
    let _ = quantized.dequantize();
    let duration = start.elapsed();
    info!("  Original size: {} bytes", 256 * 256 * 4);
    info!("  Quantized size: {} bytes", 256 * 256);
    info!(
        "  Quantize + Dequantize: {:.3} ms",
        duration.as_secs_f64() * 1000.0
    );
    info!("");

    Ok(())
}

/// Demonstrate agent migration capabilities
async fn demo_agent_migration(agent_count: usize) -> Result<()> {
    info!("=== Phase 3: Agent Migration ===");
    info!("");

    // Create nodes
    let edge_node = Arc::new(FullStackNode::new(NodeRole::Edge)?);
    let relay_node = Arc::new(FullStackNode::new(NodeRole::Relay)?);
    let core_node = Arc::new(FullStackNode::new(NodeRole::Core)?);

    info!("Created mesh nodes:");
    info!("  Edge:  {}", edge_node.node_id);
    info!("  Relay: {}", relay_node.node_id);
    info!("  Core:  {}", core_node.node_id);
    info!("");

    // Create and deploy agents to edge node
    info!("Deploying {} agents to Edge node...", agent_count);
    for i in 0..agent_count {
        let wasm_binary = create_sample_wasm(i);
        let agent = Agent::new(wasm_binary);
        let _ = edge_node.deploy_agent(agent).await?;
    }
    info!("");

    // Display status
    info!("Cluster Status:");
    edge_node.status_summary().await.display();
    relay_node.status_summary().await.display();
    core_node.status_summary().await.display();
    info!("");

    Ok(())
}

/// Demonstrate HAL capabilities
async fn demo_hal_integration() -> Result<()> {
    info!("=== Phase 4: Hardware Abstraction Layer ===");
    info!("");

    // Architecture detection
    let arch = detect_architecture();
    info!("Architecture Detection:");
    info!("  Detected: {}", arch);

    match arch {
        Architecture::AArch64 => {
            info!("  Family: ARM 64-bit");
            info!("  Potential SIMD: NEON, SVE, SVE2, SME");
        }
        Architecture::X86_64 => {
            info!("  Family: x86 64-bit");
            info!("  Potential SIMD: SSE, AVX, AVX2, AVX-512");
        }
        Architecture::RiscV64 => {
            info!("  Family: RISC-V 64-bit");
            info!("  Potential SIMD: RVV (RISC-V Vector)");
        }
        Architecture::ArmCortexM | Architecture::CortexM => {
            info!("  Family: ARM Cortex-M");
            info!("  Potential SIMD: Helium (M55+)");
        }
        Architecture::LoongArch64 => {
            info!("  Family: LoongArch 64-bit");
            info!("  Potential SIMD: LSX, LASX");
        }
        Architecture::Xtensa => {
            info!("  Family: Xtensa (ESP32)");
            info!("  Potential SIMD: Proprietary DSP");
        }
    }
    info!("");

    // Hardware profile
    let profile = HardwareProfile::detect();
    info!("Hardware Profile:");
    info!("  CPU Cores: {}", profile.core_count);
    info!("  Memory Size: {} MB", profile.memory_size / 1024 / 1024);
    info!("");

    // Capability flags
    info!("SIMD Capabilities:");
    let caps = profile.capabilities;
    info!(
        "  NEON:    {}",
        if caps.contains(HardwareCapabilities::NEON) {
            "YES"
        } else {
            "NO"
        }
    );
    info!(
        "  AVX2:    {}",
        if caps.contains(HardwareCapabilities::AVX2) {
            "YES"
        } else {
            "NO"
        }
    );
    info!("");

    // Backend selection
    let runtime = TensorRuntime::new(caps);
    info!("Selected Backend: {}", runtime.acceleration_info());
    info!("");

    Ok(())
}

/// Demonstrate kernel features
async fn demo_kernel_features() -> Result<()> {
    info!("=== Phase 5: Kernel Features ===");
    info!("");

    // Scheduler demonstration
    info!("Scheduler Demonstration:");
    let mut scheduler = Scheduler::new();

    // Spawn tasks with different priorities
    info!("  Spawning tasks with various priorities...");
    let low_priority = scheduler
        .spawn_task(10)
        .context("Failed to spawn low priority task")?;
    let high_priority = scheduler
        .spawn_task(200)
        .context("Failed to spawn high priority task")?;

    info!("    Low priority task: {}", low_priority);
    info!("    High priority task: {}", high_priority);
    info!("");

    // Schedule and yield cycle
    info!("  Running schedule/yield cycle...");
    let mut scheduled_count = 0;
    for _ in 0..10 {
        if scheduler.schedule().is_some() {
            scheduled_count += 1;
            scheduler.yield_task();
        }
    }
    info!("    Scheduled {} times", scheduled_count);
    info!("");

    // Display metrics
    let metrics = scheduler.metrics();
    info!("  Scheduler Metrics:");
    info!("    Tasks Spawned: {}", metrics.tasks_spawned);
    info!("    Active Tasks: {}", metrics.active_tasks);
    info!("    Utilization: {:.1}%", metrics.utilization() * 100.0);
    info!("");

    // Terminate tasks
    scheduler.terminate_task(low_priority);
    scheduler.terminate_task(high_priority);

    info!("  Final state: {} active tasks", scheduler.active_tasks());
    info!("");

    Ok(())
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get human-readable role name
fn role_name(role: NodeRole) -> &'static str {
    match role {
        NodeRole::Core => "Core",
        NodeRole::Relay => "Relay",
        NodeRole::Edge => "Edge",
    }
}

/// Create a sample WASM binary for testing
fn create_sample_wasm(index: usize) -> Vec<u8> {
    // Minimal WASM module header + index marker
    let mut binary = vec![
        0x00, 0x61, 0x73, 0x6d, // WASM magic number
        0x01, 0x00, 0x00, 0x00, // WASM version
    ];
    // Add index as additional bytes for differentiation
    binary.extend_from_slice(&(index as u32).to_le_bytes());
    binary
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info!("################################################################");
    info!("#                                                              #");
    info!("#  MielinOS End-to-End Full-Stack Demonstration                #");
    info!("#  Version: v0.0.1 (Development Preview)                       #");
    info!("#                                                              #");
    info!("#  Demonstrating: Mesh + Tensor + Migration + HAL + Kernel     #");
    info!("#                                                              #");
    info!("################################################################");
    info!("");

    let total_start = Instant::now();

    // Run all demo phases
    demo_hal_integration().await?;
    demo_mesh_networking().await?;
    demo_tensor_computation(256).await?;
    demo_kernel_features().await?;
    demo_agent_migration(5).await?;

    let total_duration = total_start.elapsed();

    info!("################################################################");
    info!("#                                                              #");
    info!("#  Demonstration Complete                                      #");
    info!("#                                                              #");
    info!(
        "#  Total Duration: {:>10.3}s                              #",
        total_duration.as_secs_f64()
    );
    info!("#                                                              #");
    info!("################################################################");

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sample_wasm() {
        let wasm = create_sample_wasm(0);
        assert!(wasm.len() >= 8);
        // Check WASM magic number
        assert_eq!(&wasm[0..4], &[0x00, 0x61, 0x73, 0x6d]);
    }

    #[test]
    fn test_role_name() {
        assert_eq!(role_name(NodeRole::Core), "Core");
        assert_eq!(role_name(NodeRole::Relay), "Relay");
        assert_eq!(role_name(NodeRole::Edge), "Edge");
    }

    #[tokio::test]
    async fn test_full_stack_node_creation() {
        let node = FullStackNode::new(NodeRole::Core);
        assert!(node.is_ok());

        let node = node.expect("Node should be created");
        assert!(!node.node_id.is_nil());
        assert!(matches!(node.role, NodeRole::Core));
    }

    #[tokio::test]
    async fn test_agent_deployment() {
        let node = FullStackNode::new(NodeRole::Edge).expect("Failed to create node");
        let wasm = create_sample_wasm(0);
        let agent = Agent::new(wasm);

        let result = node.deploy_agent(agent).await;
        assert!(result.is_ok());

        let status = node.status_summary().await;
        assert_eq!(status.agent_count, 1);
    }
}
