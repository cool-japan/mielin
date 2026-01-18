//! Policy-based migration example
//!
//! This example demonstrates:
//! - Evaluating migration policies
//! - Configuring different migration triggers
//! - Scoring and selecting target nodes
//! - Policy-based decision making
//! - Creating custom migration policies

use mielin_cells::policy::*;

fn main() {
    println!("=== Policy-Based Migration Example ===\n");

    // Example 1: Default migration policy
    println!("1. Default migration policy:");
    let default_policy = MigrationPolicy::default();
    println!(
        "   Load trigger: CPU {}%, Memory {}%",
        default_policy.load_trigger.cpu_threshold, default_policy.load_trigger.memory_threshold
    );
    println!(
        "   Thermal trigger: Warning {:.1}°C, Critical {:.1}°C",
        default_policy.thermal_trigger.warning_threshold,
        default_policy.thermal_trigger.critical_threshold
    );
    println!(
        "   Migration cooldown: {}s",
        default_policy.migration_cooldown_secs
    );

    // Example 2: Performance-optimized policy
    println!("\n2. Performance-optimized policy:");
    let perf_policy = MigrationPolicy::performance_optimized();
    println!("   Aggressive thresholds for lower latency");
    println!(
        "   CPU threshold: {}%",
        perf_policy.load_trigger.cpu_threshold
    );
    println!(
        "   Max latency: {}ms",
        perf_policy.latency_trigger.max_latency_ms
    );

    // Example 3: Cost-optimized policy
    println!("\n3. Cost-optimized policy:");
    let cost_policy = MigrationPolicy::cost_optimized();
    println!("   Prioritizes cost savings over performance");
    println!(
        "   Cost trigger enabled: {}",
        cost_policy.cost_trigger.enabled
    );
    println!(
        "   Max cost per unit: ${:.2}",
        cost_policy.cost_trigger.max_cost_per_unit
    );

    // Example 4: Battery-optimized policy
    println!("\n4. Battery-optimized policy:");
    let battery_policy = MigrationPolicy::battery_optimized();
    println!("   Optimized for mobile devices");
    println!(
        "   Min battery: {}%",
        battery_policy.battery_trigger.min_battery_percent
    );
    println!(
        "   Lower CPU threshold: {}%",
        battery_policy.load_trigger.cpu_threshold
    );

    // Example 5: Evaluating node conditions
    println!("\n5. Evaluating current node metrics:");

    // Create metrics for a healthy node
    let healthy_node = NodeMetrics {
        cpu_load_percent: 30,
        memory_usage_percent: 40,
        available_memory_bytes: 8_000_000_000,
        temperature_celsius: 50.0,
        network_latency_ms: 20,
        available_bandwidth_bps: 100_000_000,
        cost_per_unit: 0.5,
        agent_count: 5,
        queue_depth: 10,
        battery_percent: Some(80),
        on_ac_power: true,
    };

    let evaluator = PolicyEvaluator::new(default_policy.clone());
    let decision = evaluator.evaluate(&healthy_node);

    println!("   Healthy node decision:");
    println!("   Should migrate: {}", decision.should_migrate);
    println!("   Reason: {}", decision.reason);

    // Example 6: High load scenario
    println!("\n6. High load scenario:");

    let high_load_node = NodeMetrics {
        cpu_load_percent: 95,
        memory_usage_percent: 92,
        temperature_celsius: 75.0,
        queue_depth: 150,
        ..healthy_node
    };

    let decision = evaluator.evaluate(&high_load_node);
    println!("   Should migrate: {}", decision.should_migrate);
    println!("   Trigger: {:?}", decision.trigger);
    println!("   Priority: {}", decision.priority);
    println!("   Reason: {}", decision.reason);

    // Example 7: Thermal emergency
    println!("\n7. Thermal emergency scenario:");

    let overheating_node = NodeMetrics {
        temperature_celsius: 90.0, // Critical temperature
        cpu_load_percent: 85,
        ..healthy_node
    };

    let decision = evaluator.evaluate(&overheating_node);
    println!("   Should migrate: {}", decision.should_migrate);
    println!("   Trigger: {:?}", decision.trigger);
    println!("   Priority: {} (maximum)", decision.priority);
    println!("   Reason: {}", decision.reason);

    // Example 8: Target node selection
    println!("\n8. Selecting best target node:");

    let requirements = TargetRequirements {
        max_cpu_load: Some(60),
        max_memory_usage: Some(70),
        max_temperature: Some(65.0),
        max_latency: Some(50),
        prefer_ac_power: true,
        ..Default::default()
    };

    // Create candidate nodes
    let candidates = vec![
        (
            0,
            NodeMetrics {
                cpu_load_percent: 70, // Too high
                ..healthy_node
            },
        ),
        (
            1,
            NodeMetrics {
                cpu_load_percent: 40,
                memory_usage_percent: 50,
                temperature_celsius: 45.0,
                on_ac_power: true,
                agent_count: 3, // Less loaded
                ..healthy_node
            },
        ),
        (
            2,
            NodeMetrics {
                cpu_load_percent: 50,
                memory_usage_percent: 60,
                temperature_celsius: 55.0,
                on_ac_power: false, // Not preferred
                ..healthy_node
            },
        ),
    ];

    println!("   Candidate nodes:");
    for (idx, metrics) in &candidates {
        let score = score_target_node(metrics, &requirements);
        match score {
            Some(s) => println!("     Node {}: Score = {:.2}", idx, s),
            None => println!("     Node {}: Does not meet requirements", idx),
        }
    }

    if let Some(best_idx) = select_best_target(&candidates, &requirements) {
        println!("   ✓ Best target: Node {}", best_idx);
    }

    // Example 9: Custom migration policy
    println!("\n9. Creating custom migration policy:");

    let custom_policy = MigrationPolicy {
        load_trigger: LoadTriggerConfig {
            cpu_threshold: 80,
            memory_threshold: 85,
            queue_depth_threshold: 100,
            sustained_duration_secs: 30,
            enabled: true,
        },
        thermal_trigger: ThermalTriggerConfig {
            warning_threshold: 65.0,
            critical_threshold: 80.0,
            enabled: true,
        },
        latency_trigger: LatencyTriggerConfig {
            max_latency_ms: 75,
            min_bandwidth_bps: 10_000_000,
            enabled: true,
        },
        cost_trigger: CostTriggerConfig {
            max_cost_per_unit: 1.5,
            cost_reduction_threshold: 25.0,
            enabled: true,
        },
        battery_trigger: BatteryTriggerConfig {
            min_battery_percent: 20,
            prefer_ac_power: true,
            enabled: true,
        },
        migration_cooldown_secs: 120, // 2 minutes
        max_migrations_per_hour: 5,   // Conservative limit
    };

    // Validate custom policy
    match custom_policy.validate() {
        Ok(_) => println!("   ✓ Custom policy validated successfully"),
        Err(e) => println!("   ✗ Policy validation failed: {}", e),
    }

    println!("   Custom policy parameters:");
    println!(
        "     Migration cooldown: {}s",
        custom_policy.migration_cooldown_secs
    );
    println!(
        "     Max migrations/hour: {}",
        custom_policy.max_migrations_per_hour
    );

    // Example 10: Rate limiting
    println!("\n10. Migration rate limiting:");

    let mut rate_limited_evaluator = PolicyEvaluator::new(custom_policy.clone());

    // First migration is allowed
    let decision1 = rate_limited_evaluator.evaluate(&high_load_node);
    println!(
        "   First evaluation: should_migrate = {}",
        decision1.should_migrate
    );

    if decision1.should_migrate {
        rate_limited_evaluator.record_migration();
        println!("   ✓ Migration recorded");
    }

    // Immediate second migration should be blocked by cooldown
    let decision2 = rate_limited_evaluator.evaluate(&high_load_node);
    println!(
        "   Second evaluation (immediate): should_migrate = {}",
        decision2.should_migrate
    );
    println!(
        "   Migrations this hour: {}",
        rate_limited_evaluator.migrations_this_hour()
    );

    println!("\n=== Example Complete ===");
}
