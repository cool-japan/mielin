//! Agent Versioning Example
//!
//! Demonstrates:
//! - Version registry and metadata management
//! - Rolling updates with different strategies
//! - Canary deployments with health monitoring
//! - A/B testing with traffic splitting

use mielin_cells::{
    ABTestConfig, AgentId, CanaryConfig, Dna, RollingUpdateConfig, RollingUpdateStrategy, Version,
    VersionDeployer, VersionMetadata, VersionRegistry,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Agent Versioning Example ===\n");

    // Create version registry
    let registry = Arc::new(VersionRegistry::new());

    // Register multiple versions of an agent
    register_versions(&registry)?;

    // Demonstrate rolling updates
    println!("\n--- Rolling Updates ---");
    rolling_update_example(&registry).await?;

    // Demonstrate canary deployment
    println!("\n--- Canary Deployment ---");
    canary_deployment_example(&registry).await?;

    // Demonstrate A/B testing
    println!("\n--- A/B Testing ---");
    ab_testing_example(&registry)?;

    println!("\n=== Example Complete ===");
    Ok(())
}

fn register_versions(registry: &Arc<VersionRegistry>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Registering agent versions...");

    // Register v1.0.0
    let v1_0_0 = Version::new(1, 0, 0);
    let v1_0_0_dna = Dna::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let v1_0_0_metadata = VersionMetadata::new(
        v1_0_0,
        v1_0_0_dna,
        "Initial release with basic functionality".to_string(),
    )
    .with_changelog(vec!["Initial release".to_string()]);

    registry.register_version(v1_0_0_metadata)?;
    println!("  ✓ Registered v1.0.0");

    // Register v1.1.0 (minor update)
    let v1_1_0 = Version::new(1, 1, 0);
    let v1_1_0_dna = Dna::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x01, 0x00, 0x00]);
    let v1_1_0_metadata = VersionMetadata::new(
        v1_1_0,
        v1_1_0_dna,
        "Performance improvements and new features".to_string(),
    )
    .with_changelog(vec![
        "Improved processing speed by 30%".to_string(),
        "Added caching layer".to_string(),
        "Bug fixes".to_string(),
    ]);

    registry.register_version(v1_1_0_metadata)?;
    println!("  ✓ Registered v1.1.0");

    // Register v2.0.0 (major update)
    let v2_0_0 = Version::new(2, 0, 0);
    let v2_0_0_dna = Dna::new(vec![0x00, 0x61, 0x73, 0x6d, 0x02, 0x00, 0x00, 0x00]);
    let v2_0_0_metadata = VersionMetadata::new(
        v2_0_0,
        v2_0_0_dna,
        "Major rewrite with breaking changes".to_string(),
    )
    .with_changelog(vec![
        "Complete architecture overhaul".to_string(),
        "Breaking API changes".to_string(),
        "5x performance improvement".to_string(),
    ])
    .with_migration_path(vec![v1_1_0]); // Must be on v1.1.0 to migrate to v2.0.0

    registry.register_version(v2_0_0_metadata)?;
    println!("  ✓ Registered v2.0.0");

    Ok(())
}

async fn rolling_update_example(
    registry: &Arc<VersionRegistry>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\nPerforming rolling update from v1.0.0 to v1.1.0...");

    // Create agents on v1.0.0
    let mut agents = Vec::new();
    let v1_0_0 = Version::new(1, 0, 0);

    for i in 0..10 {
        let agent_id = AgentId::new_v4();
        registry.register_agent(agent_id, v1_0_0)?;
        agents.push(agent_id);
        println!("  Agent {}: {:?} (v1.0.0)", i + 1, agent_id);
    }

    // Strategy 1: Immediate update
    println!("\n1. Immediate Update (all at once):");
    let config = RollingUpdateConfig::new(v1_0_0, Version::new(1, 1, 0))
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_rollback(true)
        .with_max_failures(2);

    let deployer = VersionDeployer::new(registry.clone());
    let updated = deployer.rolling_update(config, agents.clone()).await?;
    println!("  ✓ Updated {} agents immediately", updated.len());

    // Reset to v1.0.0
    for &agent_id in &agents {
        registry.update_agent_version(agent_id, v1_0_0)?;
    }

    // Strategy 2: Batched update
    println!("\n2. Batched Update (5 agents per batch):");
    let config = RollingUpdateConfig::new(v1_0_0, Version::new(1, 1, 0)).with_strategy(
        RollingUpdateStrategy::Batched {
            batch_size: 5,
            delay_between_batches_ms: 100,
        },
    );

    let updated = deployer.rolling_update(config, agents.clone()).await?;
    println!("  ✓ Updated {} agents in batches", updated.len());

    // Reset to v1.0.0
    for &agent_id in &agents {
        registry.update_agent_version(agent_id, v1_0_0)?;
    }

    // Strategy 3: Sequential update
    println!("\n3. Sequential Update (one at a time):");
    let config = RollingUpdateConfig::new(v1_0_0, Version::new(1, 1, 0)).with_strategy(
        RollingUpdateStrategy::Sequential {
            delay_between_agents_ms: 50,
        },
    );

    let updated = deployer.rolling_update(config, agents.clone()).await?;
    println!("  ✓ Updated {} agents sequentially", updated.len());

    // Verify all agents are on v1.1.0
    for (i, &agent_id) in agents.iter().enumerate() {
        let version = registry.get_agent_version(&agent_id)?;
        println!("  Agent {}: v{}", i + 1, version);
    }

    Ok(())
}

async fn canary_deployment_example(
    registry: &Arc<VersionRegistry>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\nDeploying v2.0.0 as canary release...");

    // Create canary config
    let canary_config = CanaryConfig::new(Version::new(1, 1, 0), Version::new(2, 0, 0))
        .with_initial_percentage(0.05) // Start with 5%
        .with_increment(0.10, 5) // Increase by 10% every 5 seconds
        .with_success_threshold(0.95) // Require 95% success rate
        .with_max_failures(3);

    let deployer = VersionDeployer::new(registry.clone());
    let mut canary = deployer.start_canary(canary_config)?;

    println!(
        "  Initial canary percentage: {}%",
        canary.current_percentage * 100.0
    );

    // Simulate assigning 100 agents
    println!("\nAssigning 100 agents to stable/canary versions:");
    for i in 0..100 {
        let agent_id = AgentId::new_v4();
        let assigned_version = canary.assign_agent(agent_id);
        registry.register_agent(agent_id, assigned_version)?;

        if i == 0 || i == 49 || i == 99 {
            let stats = canary.stats();
            let total = stats.stable_count + stats.canary_count;
            let actual_split = if total > 0 {
                stats.canary_count as f64 / total as f64
            } else {
                0.0
            };
            println!(
                "  After {} agents: Stable={}, Canary={}, Split={:.1}%",
                i + 1,
                stats.stable_count,
                stats.canary_count,
                actual_split * 100.0
            );
        }
    }

    // Simulate health monitoring
    println!("\nMonitoring canary health:");
    for round in 1..=5 {
        // Simulate successful operations
        for _ in 0..95 {
            canary.record_success();
        }
        // Simulate some failures
        for _ in 0..5 {
            canary.record_failure();
        }

        let stats = canary.stats();
        println!(
            "  Round {}: Success rate={:.1}%, Failures={}, Should promote={}",
            round,
            stats.success_rate * 100.0,
            stats.failure_count,
            canary.should_promote()
        );

        if canary.should_abort() {
            println!("  ✗ Canary aborted due to too many failures!");
            break;
        }

        if canary.should_promote() {
            println!("  ✓ Canary meets success threshold - ready for promotion!");
        }

        // Increment canary percentage
        canary.increment();
        println!(
            "  Canary percentage increased to {:.1}%",
            canary.current_percentage * 100.0
        );
    }

    Ok(())
}

fn ab_testing_example(registry: &Arc<VersionRegistry>) -> Result<(), Box<dyn std::error::Error>> {
    println!("\nRunning A/B test: v1.1.0 vs v2.0.0...");

    // Create A/B test config (70/30 split)
    let ab_config = ABTestConfig::new(Version::new(1, 1, 0), Version::new(2, 0, 0), 0.3)
        .with_duration(300) // 5 minutes
        .with_metrics(vec![
            "response_time_ms".to_string(),
            "error_rate".to_string(),
            "cpu_usage".to_string(),
        ]);

    let deployer = VersionDeployer::new(registry.clone());
    let mut ab_test = deployer.start_ab_test(ab_config)?;

    println!("  Traffic split: 70% v1.1.0 (A), 30% v2.0.0 (B)");

    // Simulate assigning 1000 agents
    println!("\nAssigning 1000 agents for A/B testing:");
    for _ in 0..1000 {
        let agent_id = AgentId::new_v4();
        let assigned_version = ab_test.assign_agent(agent_id);
        registry.register_agent(agent_id, assigned_version)?;
    }

    let stats = ab_test.stats();
    println!("\nA/B Test Results:");
    println!("  Version A (v1.1.0): {} agents", stats.version_a_count);
    println!("  Version B (v2.0.0): {} agents", stats.version_b_count);

    // Calculate actual split
    let total = stats.version_a_count + stats.version_b_count;
    let actual_split = if total > 0 {
        stats.version_b_count as f64 / total as f64
    } else {
        0.0
    };

    println!("  Actual split: {:.1}% to version B", actual_split * 100.0);
    println!("  Target split: 30.0% to version B");

    let deviation = (actual_split - 0.3).abs();
    println!("  Deviation: {:.2}%", deviation * 100.0);

    if deviation < 0.05 {
        println!("  ✓ Traffic split within acceptable range!");
    } else {
        println!(
            "  ⚠ Traffic split deviation higher than expected (may stabilize with more agents)"
        );
    }

    // Simulate collecting metrics
    println!("\nSimulated Metrics Comparison:");
    println!("  Metric              | Version A | Version B | Winner");
    println!("  -------------------|-----------|-----------|--------");
    println!("  Response Time (ms) |    45.2   |    18.5   |   B ✓");
    println!("  Error Rate (%)     |    0.5    |    0.3    |   B ✓");
    println!("  CPU Usage (%)      |    62.0   |    58.0   |   B ✓");
    println!("\n  Conclusion: Version B (v2.0.0) performs better across all metrics!");

    Ok(())
}
