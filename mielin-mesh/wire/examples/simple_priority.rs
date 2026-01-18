//! Simple Priority Queue Example
//!
//! Demonstrates priority-based message handling.
//!
//! Run with:
//! ```bash
//! cargo run --example simple_priority
//! ```

use mielin_mesh_wire::{Message, Priority, PriorityQueue};
use std::error::Error;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting simple priority queue example");

    // Create priority queue with default configuration
    let mut queue = PriorityQueue::new();

    info!("Created priority queue");

    // Create different priority messages
    let critical = Message::AgentMigration {
        agent_id: [1; 16],
        snapshot: vec![1, 2, 3],
        priority: 10,
    };

    let high = Message::Discovery {
        node_id: [2; 16],
        node_role: mielin_mesh_wire::NodeRole::Core,
        capabilities: vec!["migration".to_string()],
    };

    let normal = Message::LoadInfo {
        cpu_usage: 0.5,
        memory_usage: 0.7,
        active_agents: 10,
    };

    let low = Message::Ping { timestamp: 12345 };

    // Enqueue messages
    info!("Enqueueing messages...");
    queue.enqueue_with_priority(low.clone(), Priority::Low)?;
    queue.enqueue_with_priority(normal.clone(), Priority::Normal)?;
    queue.enqueue_with_priority(critical.clone(), Priority::Critical)?;
    queue.enqueue_with_priority(high.clone(), Priority::High)?;

    // Display stats
    let stats = queue.stats();
    info!("Queue stats: {:?}", stats);

    // Dequeue messages (will come out in priority order)
    info!("Dequeueing messages in priority order:");
    while let Some(msg) = queue.dequeue() {
        info!("  Priority {:?}: {:?}", msg.priority, msg.message);
    }

    info!("Example completed");
    Ok(())
}
