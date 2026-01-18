//! Live Agent Migration Demo
//!
//! Demonstrates the live migration system with pre-copy strategy,
//! two-phase commit, and rollback capabilities.

use mielin_mesh_wire::{
    discovery::{DiscoveryConfig, DiscoveryService},
    health::{HealthConfig, HealthMonitor},
    migration::{AgentSnapshot, ExecutionContext, MigrationConfig, MigrationCoordinator},
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("=== Live Agent Migration Demo ===\n");

    // Setup source node
    info!("Setting up source node (127.0.0.1:8000)");
    let source_addr: SocketAddr = "127.0.0.1:8000".parse()?;
    let source_coordinator = create_coordinator(source_addr).await?;
    source_coordinator.start().await?;

    // Setup destination nodes
    info!("Setting up destination node 1 (127.0.0.1:8001)");
    let dest1_addr: SocketAddr = "127.0.0.1:8001".parse()?;
    let dest1_coordinator = create_coordinator(dest1_addr).await?;
    dest1_coordinator.start().await?;

    info!("Setting up destination node 2 (127.0.0.1:8002)");
    let dest2_addr: SocketAddr = "127.0.0.1:8002".parse()?;
    let dest2_coordinator = create_coordinator(dest2_addr).await?;
    dest2_coordinator.start().await?;

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Example 1: Simple migration
    info!("\n=== Example 1: Simple Agent Migration ===");
    let agent1_id = [1u8; 16];
    let snapshot1 = create_agent_snapshot(agent1_id, 1024, 4096);

    info!("Agent size: {} bytes", snapshot1.code.len());
    info!("Memory size: {} bytes", snapshot1.memory.len());
    info!("Migrating from {} to {}", source_addr, dest1_addr);

    let migration_id1 = source_coordinator
        .initiate_migration(agent1_id, dest1_addr, snapshot1)
        .await?;

    info!("Migration ID: {}", hex::encode(migration_id1));

    // Wait for completion
    tokio::time::sleep(Duration::from_millis(300)).await;

    let state1 = source_coordinator.get_migration_state(&migration_id1).await;
    info!("Migration state: {:?}", state1);

    // Example 2: Large agent migration with pre-copy
    info!("\n=== Example 2: Large Agent Migration (Pre-copy) ===");
    let agent2_id = [2u8; 16];
    let snapshot2 = create_agent_snapshot(agent2_id, 10240, 409600); // 10KB code, 400KB memory

    info!("Agent size: {} KB", snapshot2.code.len() / 1024);
    info!("Memory size: {} KB", snapshot2.memory.len() / 1024);
    info!("Migrating from {} to {}", source_addr, dest2_addr);
    info!("Using pre-copy strategy for large agent...");

    let migration_id2 = source_coordinator
        .initiate_migration(agent2_id, dest2_addr, snapshot2)
        .await?;

    info!("Migration ID: {}", hex::encode(migration_id2));

    tokio::time::sleep(Duration::from_millis(500)).await;

    let state2 = source_coordinator.get_migration_state(&migration_id2).await;
    info!("Migration state: {:?}", state2);

    // Example 3: Multiple concurrent migrations
    info!("\n=== Example 3: Concurrent Migrations ===");
    info!("Starting 3 concurrent migrations...");

    let mut migration_ids = Vec::new();
    for i in 0..3 {
        let agent_id = [10 + i; 16];
        let destination = if i % 2 == 0 { dest1_addr } else { dest2_addr };
        let snapshot = create_agent_snapshot(agent_id, 2048, 8192);

        info!("  Agent {}: {} -> {}", i + 1, source_addr, destination);

        let migration_id = source_coordinator
            .initiate_migration(agent_id, destination, snapshot)
            .await?;

        migration_ids.push(migration_id);
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("\nChecking migration states:");
    for (i, migration_id) in migration_ids.iter().enumerate() {
        let state = source_coordinator.get_migration_state(migration_id).await;
        info!("  Migration {}: {:?}", i + 1, state);
    }

    // Example 4: Migration with rollback
    info!("\n=== Example 4: Migration Rollback ===");
    let agent4_id = [4u8; 16];
    let snapshot4 = create_agent_snapshot(agent4_id, 1024, 4096);

    info!("Initiating migration...");
    let migration_id4 = source_coordinator
        .initiate_migration(agent4_id, dest1_addr, snapshot4)
        .await?;

    // Wait a bit then rollback
    tokio::time::sleep(Duration::from_millis(50)).await;

    info!("Simulating failure, rolling back migration...");
    source_coordinator
        .rollback_migration(migration_id4, "Simulated network failure".to_string())
        .await?;

    let state4 = source_coordinator.get_migration_state(&migration_id4).await;
    info!("Final state: {:?}", state4);

    // Display statistics
    info!("\n=== Migration Statistics ===");

    let stats = source_coordinator.stats().await;
    info!("\nSource Node:");
    info!("  Total attempts: {}", stats.total_attempts);
    info!("  Successful: {}", stats.successful);
    info!("  Failed: {}", stats.failed);
    info!("  Rolled back: {}", stats.rolled_back);
    info!("  Average duration: {} ms", stats.avg_duration_ms);
    info!("  Average downtime: {} ms", stats.avg_downtime_ms);
    info!(
        "  Total bytes transferred: {} KB",
        stats.total_bytes_transferred / 1024
    );
    info!(
        "  Average pre-copy iterations: {:.2}",
        stats.avg_precopy_iterations
    );

    let success_rate = if stats.total_attempts > 0 {
        (stats.successful as f64 / stats.total_attempts as f64) * 100.0
    } else {
        0.0
    };
    info!("  Success rate: {:.1}%", success_rate);

    // Example 5: Migration phases demonstration
    info!("\n=== Example 5: Migration Phases ===");
    info!("Migration workflow consists of:");
    info!("  1. Prepare - Resource check and pre-flight validation");
    info!("  2. Pre-copy - Iterative dirty page transfer (optional)");
    info!("  3. Stop-and-copy - Final state transfer with minimal downtime");
    info!("  4. Commit - Activate agent on destination");

    info!("\nPre-copy strategy:");
    info!("  • Reduces downtime for large agents");
    info!("  • Iteratively transfers memory pages");
    info!("  • Converges when dirty pages < threshold");
    info!("  • Default: max 10 iterations, stop at <100 dirty pages");

    info!("\nTwo-phase commit:");
    info!("  • Ensures atomic migration");
    info!("  • Rollback on any failure");
    info!("  • Prevents agent loss");

    // Example 6: Configuration options
    info!("\n=== Example 6: Migration Configuration ===");
    let config = MigrationConfig::default();
    info!("Default configuration:");
    info!(
        "  Max pre-copy iterations: {}",
        config.max_precopy_iterations
    );
    info!("  Dirty page threshold: {}", config.dirty_threshold);
    info!("  Page size: {} bytes", config.page_size);
    info!("  Migration timeout: {:?}", config.migration_timeout);
    info!("  Compression enabled: {}", config.enable_compression);
    info!("  Commit timeout: {:?}", config.commit_timeout);

    info!("\n=== Demo Complete ===");
    info!("\nKey Features Demonstrated:");
    info!("✓ Pre-copy migration strategy");
    info!("✓ Stop-and-copy with minimal downtime");
    info!("✓ Two-phase commit protocol");
    info!("✓ Rollback on failure");
    info!("✓ Concurrent migrations");
    info!("✓ Migration statistics tracking");
    info!("✓ State machine transitions");

    Ok(())
}

/// Create a migration coordinator for a node
async fn create_coordinator(
    addr: SocketAddr,
) -> Result<Arc<MigrationCoordinator>, Box<dyn std::error::Error>> {
    let node_id = {
        let mut id = [0u8; 16];
        // Use last octet of IP as first byte of node ID
        if let std::net::IpAddr::V4(ipv4) = addr.ip() {
            id[0] = ipv4.octets()[3];
        }
        id
    };

    let discovery_config = DiscoveryConfig::default();
    let discovery = Arc::new(DiscoveryService::new(discovery_config, node_id, vec![]));

    let health_config = HealthConfig::default();
    let health = Arc::new(HealthMonitor::with_config(health_config));

    let migration_config = MigrationConfig {
        max_precopy_iterations: 10,
        dirty_threshold: 100,
        page_size: 4096,
        migration_timeout: Duration::from_secs(60),
        enable_compression: true,
        commit_timeout: Duration::from_secs(5),
    };

    let coordinator = Arc::new(MigrationCoordinator::new(
        migration_config,
        addr,
        discovery,
        health,
    ));

    Ok(coordinator)
}

/// Create an agent snapshot for testing
fn create_agent_snapshot(
    agent_id: [u8; 16],
    code_size: usize,
    memory_size: usize,
) -> AgentSnapshot {
    AgentSnapshot {
        agent_id,
        code: vec![0u8; code_size],
        state: vec![0u8; code_size / 2],
        memory: vec![0u8; memory_size],
        context: ExecutionContext {
            pc: 0,
            sp: memory_size as u64,
            registers: vec![0; 16],
            call_stack: vec![],
        },
    }
}
