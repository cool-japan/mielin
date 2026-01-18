//! Agent pooling example
//!
//! This example demonstrates:
//! - Creating an agent pool
//! - Configuring pool parameters
//! - Acquiring and releasing agents
//! - Monitoring pool statistics

use mielin_cells::{AgentPool, PoolConfig};
use std::time::Duration;

fn main() {
    println!("=== Agent Pooling Example ===\n");

    // Create a simple WASM binary
    let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d];

    // Example 1: Basic pool usage
    println!("1. Basic Pool Usage:");
    let pool = AgentPool::new(wasm_binary.clone());

    // Acquire some agents
    let agent1 = pool.acquire();
    let agent2 = pool.acquire();
    let agent3 = pool.acquire();

    println!("   Acquired 3 agents");
    println!("   Pool size: {}", pool.size());

    // Release them back
    pool.release(agent1);
    pool.release(agent2);
    pool.release(agent3);

    println!("   Released 3 agents");
    println!("   Pool size: {}", pool.size());

    // Example 2: High-performance pool
    println!("\n2. High-Performance Pool:");
    let hp_config = PoolConfig::high_performance();
    let hp_pool = AgentPool::with_config(wasm_binary.clone(), hp_config);

    println!("   Config:");
    println!("   - Min size: {}", hp_pool.config().min_size);
    println!("   - Max size: {}", hp_pool.config().max_size);
    println!("   - Pre-warmed: {}", hp_pool.config().pre_warm);
    println!("   Initial pool size: {}", hp_pool.size());

    // Example 3: Custom pool configuration
    println!("\n3. Custom Pool Configuration:");
    let custom_config = PoolConfig {
        min_size: 2,
        max_size: 5,
        max_age: Duration::from_secs(3600),
        max_idle_time: Duration::from_secs(300),
        pre_warm: true,
    };

    let custom_pool = AgentPool::with_config(wasm_binary.clone(), custom_config);
    println!("   Custom pool created");
    println!("   Initial size: {}", custom_pool.size());

    // Example 4: Pool statistics
    println!("\n4. Pool Statistics:");
    let stats_pool = AgentPool::new(wasm_binary.clone());

    // Perform some operations
    let mut agents = Vec::new();
    for _ in 0..10 {
        agents.push(stats_pool.acquire());
    }

    for agent in agents {
        stats_pool.release(agent);
    }

    let stats = stats_pool.stats();
    println!("   Total acquired: {}", stats.total_acquired);
    println!("   Total returned: {}", stats.total_returned);
    println!("   Total created: {}", stats.total_created);
    println!("   Total destroyed: {}", stats.total_destroyed);
    println!("   Pool misses: {}", stats.pool_misses);
    println!("   Current size: {}", stats.current_size);
    println!("   Peak size: {}", stats.peak_size);

    // Example 5: Pool maintenance
    println!("\n5. Pool Maintenance:");
    let maintain_config = PoolConfig {
        min_size: 0,
        max_size: 10,
        max_age: Duration::from_secs(3600),
        max_idle_time: Duration::from_millis(100),
        pre_warm: false,
    };
    let maintain_pool = AgentPool::with_config(wasm_binary.clone(), maintain_config);

    // Add some agents
    let mut temp_agents = Vec::new();
    for _ in 0..5 {
        temp_agents.push(maintain_pool.acquire());
    }
    for agent in temp_agents {
        maintain_pool.release(agent);
    }

    println!("   Pool size before maintenance: {}", maintain_pool.size());

    // Wait for idle timeout
    std::thread::sleep(Duration::from_millis(150));

    // Run maintenance
    maintain_pool.maintain();
    println!("   Pool size after maintenance: {}", maintain_pool.size());

    // Example 6: Agent reuse
    println!("\n6. Agent Reuse Demonstration:");
    let reuse_pool = AgentPool::new(wasm_binary);

    let agent_a = reuse_pool.acquire();
    let id_a = agent_a.id();
    println!("   Acquired agent A: {}", id_a);

    reuse_pool.release(agent_a);
    println!("   Released agent A");

    let agent_b = reuse_pool.acquire();
    let id_b = agent_b.id();
    println!("   Acquired agent B: {}", id_b);

    if id_a == id_b {
        println!("   ✓ Agent was reused (same ID)");
    } else {
        println!("   ✗ Different agent acquired");
    }

    println!("\n=== Example Complete ===");
}
