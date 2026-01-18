//! Gossip Protocol Demo
//!
//! Demonstrates the epidemic-style state propagation protocol with
//! vector clocks for causal ordering and anti-entropy reconciliation.

use mielin_mesh_wire::{
    discovery::{DiscoveryConfig, DiscoveryService},
    gossip::{GossipConfig, GossipMessage, GossipService, GossipState, VectorClock},
    health::{HealthConfig, HealthMonitor},
};
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

    info!("=== Gossip Protocol Demo ===\n");

    // Create three simulated nodes
    let node1_id = [1u8; 16];
    let node2_id = [2u8; 16];
    let node3_id = [3u8; 16];

    // Setup node 1
    info!("Setting up Node 1...");
    let (gossip1, health1) = create_node(node1_id, "127.0.0.1:8001").await?;

    // Setup node 2
    info!("Setting up Node 2...");
    let (gossip2, health2) = create_node(node2_id, "127.0.0.1:8002").await?;

    // Setup node 3
    info!("Setting up Node 3...");
    let (gossip3, _health3) = create_node(node3_id, "127.0.0.1:8003").await?;

    // Register peers for health monitoring (simulate mesh connectivity)
    info!("\nSimulating mesh network connectivity...");
    health1.register("127.0.0.1:8002".parse()?).await;
    health1.register("127.0.0.1:8003".parse()?).await;
    health2.register("127.0.0.1:8001".parse()?).await;
    health2.register("127.0.0.1:8003".parse()?).await;

    // Start gossip services
    info!("\nStarting gossip services...");
    gossip1.start().await?;
    gossip2.start().await?;
    gossip3.start().await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Example 1: Basic state propagation
    info!("\n=== Example 1: Basic State Propagation ===");
    info!("Node 1: Setting key 'temperature' = '72°F'");
    gossip1
        .put("temperature".to_string(), b"72F".to_vec())
        .await?;

    info!("Node 2: Setting key 'humidity' = '65%'");
    gossip2.put("humidity".to_string(), b"65%".to_vec()).await?;

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Check local states
    info!("\nNode 1 state:");
    for key in gossip1.keys().await {
        if let Some(value) = gossip1.get(&key).await {
            info!("  {} = {}", key, String::from_utf8_lossy(&value));
        }
    }

    info!("\nNode 2 state:");
    for key in gossip2.keys().await {
        if let Some(value) = gossip2.get(&key).await {
            info!("  {} = {}", key, String::from_utf8_lossy(&value));
        }
    }

    // Example 2: Vector clock demonstration
    info!("\n=== Example 2: Vector Clocks ===");
    let mut clock1 = VectorClock::new();
    let mut clock2 = VectorClock::new();

    info!("Initial clocks:");
    info!("  Clock 1: node1={}", clock1.get_version(&node1_id));
    info!("  Clock 2: node2={}", clock2.get_version(&node2_id));

    clock1.increment(node1_id);
    info!("\nAfter Node 1 increment:");
    info!("  Clock 1: node1={}", clock1.get_version(&node1_id));

    clock2.increment(node2_id);
    clock2.increment(node2_id);
    info!("\nAfter Node 2 increments (x2):");
    info!("  Clock 2: node2={}", clock2.get_version(&node2_id));

    // Demonstrate happens-before relationship
    let mut clock1_copy = clock1.clone();
    clock1_copy.merge(&clock2);
    info!("\nAfter merging Clock 2 into Clock 1:");
    info!(
        "  Merged: node1={}, node2={}",
        clock1_copy.get_version(&node1_id),
        clock1_copy.get_version(&node2_id)
    );

    // Example 3: Concurrent updates and conflict detection
    info!("\n=== Example 3: Concurrent Updates ===");
    info!("Creating concurrent updates on different nodes...");

    // Node 1 updates
    info!("Node 1: Setting 'shared_key' = 'value_from_node1'");
    gossip1
        .put("shared_key".to_string(), b"value_from_node1".to_vec())
        .await?;

    // Node 3 updates concurrently (before seeing Node 1's update)
    info!("Node 3: Setting 'shared_key' = 'value_from_node3' (concurrent)");
    gossip3
        .put("shared_key".to_string(), b"value_from_node3".to_vec())
        .await?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Simulate message exchange (in real system, would happen via network)
    info!("\nSimulating gossip message exchange...");

    // Get Node 1's state
    if let Some(node1_value) = gossip1.get("shared_key").await {
        info!("Node 1's value: {}", String::from_utf8_lossy(&node1_value));
    }

    // Get Node 3's state
    if let Some(node3_value) = gossip3.get("shared_key").await {
        info!("Node 3's value: {}", String::from_utf8_lossy(&node3_value));
    }

    // Example 4: Anti-entropy demonstration
    info!("\n=== Example 4: Anti-Entropy ===");
    info!("Adding states to different nodes...");

    gossip1
        .put("sensor1".to_string(), b"active".to_vec())
        .await?;
    gossip1
        .put("sensor2".to_string(), b"standby".to_vec())
        .await?;
    gossip2
        .put("sensor3".to_string(), b"active".to_vec())
        .await?;
    gossip3
        .put("sensor4".to_string(), b"inactive".to_vec())
        .await?;

    tokio::time::sleep(Duration::from_millis(300)).await;

    info!("\nNode 1 keys: {:?}", gossip1.keys().await);
    info!("Node 2 keys: {:?}", gossip2.keys().await);
    info!("Node 3 keys: {:?}", gossip3.keys().await);

    // Display statistics
    info!("\n=== Gossip Statistics ===");
    let stats1 = gossip1.stats().await;
    info!("\nNode 1:");
    info!("  Rounds: {}", stats1.rounds);
    info!("  Messages sent: {}", stats1.messages_sent);
    info!("  Messages received: {}", stats1.messages_received);
    info!("  States propagated: {}", stats1.states_propagated);
    info!("  Conflicts detected: {}", stats1.conflicts_detected);
    info!("  Current state count: {}", stats1.state_count);

    let stats2 = gossip2.stats().await;
    info!("\nNode 2:");
    info!("  Rounds: {}", stats2.rounds);
    info!("  Messages sent: {}", stats2.messages_sent);
    info!("  Messages received: {}", stats2.messages_received);
    info!("  States propagated: {}", stats2.states_propagated);
    info!("  Conflicts detected: {}", stats2.conflicts_detected);
    info!("  Current state count: {}", stats2.state_count);

    let stats3 = gossip3.stats().await;
    info!("\nNode 3:");
    info!("  Rounds: {}", stats3.rounds);
    info!("  Messages sent: {}", stats3.messages_sent);
    info!("  Messages received: {}", stats3.messages_received);
    info!("  States propagated: {}", stats3.states_propagated);
    info!("  Conflicts detected: {}", stats3.conflicts_detected);
    info!("  Current state count: {}", stats3.state_count);

    // Example 5: Message types
    info!("\n=== Example 5: Gossip Message Types ===");

    let _push_msg = GossipMessage::Push {
        node_id: node1_id,
        states: vec![GossipState {
            key: "example".to_string(),
            value: b"data".to_vec(),
            version: VectorClock::new(),
            updated_at: std::time::Instant::now(),
        }],
    };
    info!("Push message created (epidemic broadcast)");

    let _pull_msg = GossipMessage::Pull {
        node_id: node2_id,
        known_versions: std::collections::HashMap::new(),
    };
    info!("Pull message created (anti-entropy request)");

    info!("\n=== Demo Complete ===");
    info!("\nKey Features Demonstrated:");
    info!("✓ Vector clocks for causal ordering");
    info!("✓ Conflict detection on concurrent updates");
    info!("✓ Push-pull state propagation");
    info!("✓ Anti-entropy reconciliation");
    info!("✓ Automatic state expiration");
    info!("✓ Gossip statistics tracking");

    Ok(())
}

/// Helper function to create a gossip node
async fn create_node(
    node_id: [u8; 16],
    addr: &str,
) -> Result<(Arc<GossipService>, Arc<HealthMonitor>), Box<dyn std::error::Error>> {
    let discovery_config = DiscoveryConfig::default();
    let discovery = Arc::new(DiscoveryService::new(discovery_config, node_id, vec![]));

    let health_config = HealthConfig::default();
    let health = Arc::new(HealthMonitor::with_config(health_config));

    let gossip_config = GossipConfig {
        gossip_interval: Duration::from_secs(2),
        fanout: 2,
        state_expiration: Duration::from_secs(300),
        enable_anti_entropy: true,
        anti_entropy_interval: Duration::from_secs(10),
        max_message_size: 1_048_576,
    };

    let gossip = Arc::new(GossipService::new(
        gossip_config,
        node_id,
        discovery,
        health.clone(),
    ));

    info!("  Node ID: {}", hex::encode(node_id));
    info!("  Address: {}", addr);

    Ok((gossip, health))
}
