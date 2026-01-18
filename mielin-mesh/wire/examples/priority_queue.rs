//! Message Priority Queue Example
//!
//! This example demonstrates priority-based message handling with the message queue.
//!
//! Run with:
//! ```bash
//! cargo run --example priority_queue
//! ```

use mielin_mesh_wire::{Message, Priority, PriorityQueue};
use std::error::Error;
use tokio::time::{sleep, Duration};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting priority queue example");

    // Create priority queue with default configuration
    let mut queue = PriorityQueue::new();
    info!("Created priority queue");

    // Enqueue messages with different priorities
    info!("\n--- Enqueueing Messages ---");

    // Critical priority messages (highest)
    for i in 0..3 {
        let msg = Message::AgentMigration {
            agent_id: [i as u8; 16],
            snapshot: vec![1, 2, 3],
            priority: 10,
        };
        queue.enqueue_with_priority(msg, Priority::Critical)?;
        info!("Enqueued: CRITICAL-{}", i);
    }

    // High priority messages
    for i in 0..3 {
        let msg = Message::Discovery {
            node_id: [i as u8; 16],
            node_role: mielin_mesh_wire::NodeRole::Core,
            capabilities: vec!["migration".to_string()],
        };
        queue.enqueue_with_priority(msg, Priority::High)?;
        info!("Enqueued: HIGH-{}", i);
    }

    // Normal priority messages
    for i in 0..5 {
        let msg = Message::LoadInfo {
            cpu_usage: 0.5,
            memory_usage: 0.6,
            active_agents: i,
        };
        queue.enqueue_with_priority(msg, Priority::Normal)?;
        info!("Enqueued: NORMAL-{}", i);
    }

    // Low priority messages
    for i in 0..3 {
        let msg = Message::Ping {
            timestamp: i as u64,
        };
        queue.enqueue_with_priority(msg, Priority::Low)?;
        info!("Enqueued: LOW-{}", i);
    }

    // Display queue statistics
    let stats = queue.stats();
    info!("\n--- Queue Statistics ---");
    info!("Total enqueued: {}", stats.enqueued);
    info!("Total dequeued: {}", stats.dequeued);
    info!("Total dropped: {}", stats.dropped);

    // Dequeue messages (should come out in priority order)
    info!("\n--- Dequeueing Messages (Priority Order) ---");

    while let Some(msg) = queue.dequeue() {
        info!("Dequeued [Priority: {:?}]: {:?}", msg.priority, msg.message);

        // Simulate processing time
        sleep(Duration::from_millis(100)).await;
    }

    // Final statistics
    let final_stats = queue.stats();
    info!("\n--- Final Statistics ---");
    info!("Total processed: {}", final_stats.dequeued);
    info!("Messages dropped: {}", final_stats.dropped);

    info!("\nPriority queue example completed");
    Ok(())
}
