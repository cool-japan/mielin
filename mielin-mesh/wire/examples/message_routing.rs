//! Message Routing Example
//!
//! Demonstrates multi-hop message routing with TTL.
//!
//! Run with:
//! ```bash
//! cargo run --example message_routing
//! ```

use mielin_mesh_wire::Message;
use std::error::Error;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting message routing example");

    // Create nodes
    let source = [1u8; 16];
    let _relay1 = [2u8; 16];
    let _relay2 = [3u8; 16];
    let destination = [4u8; 16];

    // Create a message
    let ping = Message::Ping { timestamp: 12345 };
    info!("Original message: {:?}", ping);

    // Wrap in routing envelope
    let mut routed = ping.route(source, destination);
    info!("\nRouted message created:");
    if let Message::RoutedMessage {
        source: s,
        destination: d,
        ttl,
        hop_count,
        ..
    } = &routed
    {
        info!("  Source: {:?}", s);
        info!("  Destination: {:?}", d);
        info!("  TTL: {}", ttl);
        info!("  Hop count: {}", hop_count);
    }

    // Simulate routing through multiple hops
    info!("\nSimulating routing:");

    // Hop 1: Source -> Relay1
    info!("  Hop 1: Source -> Relay1");
    routed.forward()?;
    if let Message::RoutedMessage { ttl, hop_count, .. } = &routed {
        info!("    TTL: {}, Hop count: {}", ttl, hop_count);
    }

    // Hop 2: Relay1 -> Relay2
    info!("  Hop 2: Relay1 -> Relay2");
    routed.forward()?;
    if let Message::RoutedMessage { ttl, hop_count, .. } = &routed {
        info!("    TTL: {}, Hop count: {}", ttl, hop_count);
    }

    // Hop 3: Relay2 -> Destination
    info!("  Hop 3: Relay2 -> Destination");
    routed.forward()?;
    if let Message::RoutedMessage { ttl, hop_count, .. } = &routed {
        info!("    TTL: {}, Hop count: {}", ttl, hop_count);
    }

    // Check if message is for destination
    assert!(routed.is_for(&destination));
    info!("\n✓ Message reached destination");

    // Unwrap payload
    let payload = routed.unwrap_payload();
    info!("Unwrapped payload: {:?}", payload);

    // Test TTL expiry
    info!("\n--- Testing TTL Expiry ---");
    let ping2 = Message::Ping { timestamp: 54321 };
    let mut routed2 = ping2.route(source, destination);

    // Forward until TTL expires
    for i in 0..16 {
        match routed2.forward() {
            Ok(_) => info!("  Forward {}: OK", i + 1),
            Err(e) => {
                info!("  Forward {}: Failed - {}", i + 1, e);
                break;
            }
        }
    }

    // Next forward should fail
    if routed2.forward().is_err() {
        info!("✓ TTL expiry working correctly");
    }

    info!("\nExample completed");
    Ok(())
}
