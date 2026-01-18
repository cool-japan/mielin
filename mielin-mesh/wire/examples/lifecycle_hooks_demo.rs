//! Lifecycle Hooks Demo
//!
//! Demonstrates how to use connection lifecycle hooks to monitor
//! and react to peer connection state changes.

use mielin_mesh_wire::{
    discovery::{DiscoveryConfig, DiscoveryService},
    health::{HealthConfig, HealthMonitor},
    lifecycle_hooks::{HookBuilder, HookContext, HookEventType, LifecycleHookManager},
    peer_lifecycle::{PeerLifecycleConfig, PeerLifecycleManager},
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
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

    info!("=== Lifecycle Hooks Demo ===\n");

    // Create the core services
    let discovery_config = DiscoveryConfig::default();
    let local_id = [0u8; 16];
    let discovery = Arc::new(DiscoveryService::new(discovery_config, local_id, vec![]));

    let health_config = HealthConfig::default();
    let health = Arc::new(HealthMonitor::with_config(health_config));

    let lifecycle_config = PeerLifecycleConfig::new();
    let lifecycle_manager = Arc::new(PeerLifecycleManager::new(
        lifecycle_config,
        discovery,
        health.clone(),
    ));

    // Create hook manager
    let hook_manager = Arc::new(LifecycleHookManager::new(lifecycle_manager.clone()));

    // Example 1: Simple logging hook
    info!("Example 1: Registering a simple logging hook");
    let _hook1 = HookBuilder::new()
        .on_all_events()
        .build(&hook_manager, |ctx: HookContext| {
            info!(
                "[HOOK] Event: {} | Peer: {} | Address: {}",
                ctx.event_type.as_str(),
                hex::encode(ctx.node_id),
                ctx.address
            );
            if let Some(meta) = ctx.metadata {
                info!("       Reason: {}", meta);
            }
        })
        .await?;

    // Example 2: Connection counter hook
    info!("\nExample 2: Registering a connection counter hook");
    let connection_count = Arc::new(AtomicUsize::new(0));
    let disconnection_count = Arc::new(AtomicUsize::new(0));

    let conn_count = connection_count.clone();
    let disc_count = disconnection_count.clone();

    let _hook2 = HookBuilder::new()
        .on_connected()
        .on_removed()
        .build(&hook_manager, move |ctx: HookContext| {
            match ctx.event_type {
                HookEventType::Connected => {
                    let count = conn_count.fetch_add(1, Ordering::SeqCst) + 1;
                    info!("[COUNTER] Total connections: {}", count);
                }
                HookEventType::Removed => {
                    let count = disc_count.fetch_add(1, Ordering::SeqCst) + 1;
                    info!("[COUNTER] Total disconnections: {}", count);
                }
                _ => {}
            }
        })
        .await?;

    // Example 3: Alert hook for dead connections
    info!("\nExample 3: Registering an alert hook for dead connections");
    let _hook3 = HookBuilder::new()
        .on_dead()
        .on_suspected()
        .build(&hook_manager, |ctx: HookContext| match ctx.event_type {
            HookEventType::Suspected => {
                info!(
                    "[ALERT] ⚠️  Peer {} is suspected dead!",
                    hex::encode(ctx.node_id)
                );
            }
            HookEventType::Dead => {
                info!(
                    "[ALERT] ❌ Peer {} confirmed dead!",
                    hex::encode(ctx.node_id)
                );
            }
            _ => {}
        })
        .await?;

    // Start hook processing
    info!("\nStarting hook manager...");
    hook_manager.start().await?;
    lifecycle_manager.start().await?;

    // Simulate peer lifecycle events
    info!("\n=== Simulating Peer Lifecycle ===\n");

    // Add some peers
    info!("Adding 3 peers...");
    let peers = vec![
        ([1u8; 16], "127.0.0.1:8001".parse::<SocketAddr>()?),
        ([2u8; 16], "127.0.0.1:8002".parse::<SocketAddr>()?),
        ([3u8; 16], "127.0.0.1:8003".parse::<SocketAddr>()?),
    ];

    for (node_id, addr) in &peers {
        lifecycle_manager.register_peer(*node_id, *addr).await;
        health.register(*addr).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for events to be processed
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Simulate health degradation
    info!("\nSimulating health degradation for peer 1...");
    for _ in 0..3 {
        health.heartbeat_missed(&peers[0].1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Simulate recovery
    info!("\nSimulating recovery for peer 1...");
    health.pong_received(&peers[0].1, 50).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Simulate peer death
    info!("\nSimulating peer 2 death...");
    for _ in 0..10 {
        health.heartbeat_missed(&peers[1].1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Display final statistics
    info!("\n=== Final Statistics ===");
    info!(
        "Total connections: {}",
        connection_count.load(Ordering::SeqCst)
    );
    info!(
        "Total disconnections: {}",
        disconnection_count.load(Ordering::SeqCst)
    );
    info!("Registered hooks: {}", hook_manager.hook_count().await);

    let stats = lifecycle_manager.stats().await;
    info!("\nPeer Lifecycle Stats:");
    info!("  Total peers: {}", stats.total_peers);
    info!("  Active: {}", stats.active);
    info!("  Degraded: {}", stats.degraded);
    info!("  Suspected: {}", stats.suspected);
    info!("  Dead: {}", stats.dead);
    info!("  Removed: {}", stats.removed);

    info!("\n=== Demo Complete ===");

    Ok(())
}
