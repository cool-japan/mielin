//! Agent Orchestration and Auto-Scaling Example
//!
//! Demonstrates:
//! - Declarative deployment specifications
//! - Resource requirements and scheduling
//! - Placement constraints
//! - Auto-scaling based on policies
//! - Reconciliation loop

use mielin_cells::{
    Capability, DeploymentSpec, DeploymentStrategy, DiscoveryLocation as Location, Node,
    Orchestrator, PlacementConstraint, ResourceRequirements, ScalingPolicy, Scheduler,
    ServiceRegistry, Version,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Agent Orchestration and Auto-Scaling Example ===\n");

    // Setup infrastructure
    let scheduler = Arc::new(Scheduler::new());
    let registry = Arc::new(ServiceRegistry::new());
    let orchestrator = Orchestrator::new(scheduler.clone(), registry);

    // Register compute nodes
    println!("--- Node Registration ---");
    register_nodes(&scheduler)?;

    // Create deployments
    println!("\n--- Deployment Creation ---");
    create_deployments(&orchestrator)?;

    // Demonstrate scheduling
    println!("\n--- Scheduling and Placement ---");
    demonstrate_scheduling(&scheduler, &orchestrator)?;

    // Demonstrate auto-scaling
    println!("\n--- Auto-Scaling ---");
    demonstrate_auto_scaling(&orchestrator)?;

    println!("\n=== Example Complete ===");
    Ok(())
}

fn register_nodes(scheduler: &Scheduler) -> Result<(), Box<dyn std::error::Error>> {
    println!("Registering compute nodes...\n");

    // High-performance nodes in Tokyo
    let tokyo = Location::new(35.6762, 139.6503);
    for i in 1..=3 {
        let node = Node::new(format!("tokyo-node-{}", i))
            .with_resources(16.0, 32768, 512000) // 16 CPU cores, 32GB RAM, 500GB disk
            .with_label("region", "asia-northeast-1")
            .with_label("zone", "tokyo-a")
            .with_label("tier", "performance")
            .with_location(tokyo);

        scheduler.register_node(node);
        println!("  ✓ Registered tokyo-node-{} (16 cores, 32GB)", i);
    }

    // Standard nodes in Osaka
    let osaka = Location::new(34.6937, 135.5023);
    for i in 1..=5 {
        let node = Node::new(format!("osaka-node-{}", i))
            .with_resources(8.0, 16384, 256000) // 8 CPU cores, 16GB RAM, 250GB disk
            .with_label("region", "asia-northeast-2")
            .with_label("zone", "osaka-b")
            .with_label("tier", "standard")
            .with_location(osaka);

        scheduler.register_node(node);
        println!("  ✓ Registered osaka-node-{} (8 cores, 16GB)", i);
    }

    // Budget nodes in remote location
    for i in 1..=2 {
        let node = Node::new(format!("budget-node-{}", i))
            .with_resources(4.0, 8192, 128000) // 4 CPU cores, 8GB RAM, 125GB disk
            .with_label("region", "asia-northeast-3")
            .with_label("zone", "remote-c")
            .with_label("tier", "budget");

        scheduler.register_node(node);
        println!("  ✓ Registered budget-node-{} (4 cores, 8GB)", i);
    }

    Ok(())
}

fn create_deployments(orchestrator: &Orchestrator) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating deployments...\n");

    // 1. Web API deployment with auto-scaling
    println!("1. Web API Service:");
    let web_api_spec = DeploymentSpec::new(
        "web-api",
        "api-service",
        Version::new(1, 0, 0),
        3, // 3 replicas
    )
    .with_resources(ResourceRequirements::new(2.0, 4096, 10240))
    .with_capability(Capability::new("http", Version::new(1, 1, 0)))
    .with_capability(Capability::new("json", Version::new(1, 0, 0)))
    .with_label("app", "web-api")
    .with_label("tier", "frontend")
    .with_strategy(DeploymentStrategy::RollingUpdate)
    .with_auto_scaling(
        ScalingPolicy::new(2, 10)
            .with_cpu_target(70)
            .with_memory_target(80)
            .with_thresholds(0.8, 0.3)
            .with_cooldown(300),
    );

    orchestrator.create_deployment(web_api_spec)?;
    println!("  ✓ Created with auto-scaling (min=2, max=10)");

    // 2. Database deployment with strict placement
    println!("\n2. Database Service:");
    let mut node_labels = HashMap::new();
    node_labels.insert("tier".to_string(), "performance".to_string());

    let db_spec = DeploymentSpec::new(
        "database",
        "postgres",
        Version::new(13, 0, 0),
        2, // 2 replicas for HA
    )
    .with_resources(ResourceRequirements::new(4.0, 16384, 102400))
    .with_label("app", "database")
    .with_label("tier", "data")
    .with_constraint(PlacementConstraint::NodeSelector(node_labels)) // Must run on performance tier
    .with_constraint(PlacementConstraint::PodAntiAffinity(
        "app=database".to_string(),
    )) // Spread replicas
    .with_strategy(DeploymentStrategy::RollingUpdate);

    orchestrator.create_deployment(db_spec)?;
    println!("  ✓ Created with high-performance node requirement");

    // 3. Background worker with location constraint
    println!("\n3. Background Worker:");
    let tokyo = Location::new(35.6762, 139.6503);

    let worker_spec = DeploymentSpec::new(
        "background-worker",
        "worker-service",
        Version::new(1, 0, 0),
        5,
    )
    .with_resources(ResourceRequirements::new(1.0, 2048, 5120))
    .with_capability(Capability::new("task-processing", Version::new(1, 0, 0)))
    .with_label("app", "worker")
    .with_label("tier", "backend")
    .with_constraint(PlacementConstraint::LocationConstraint {
        location: tokyo,
        max_distance_km: 50.0, // Within 50km of Tokyo
    })
    .with_strategy(DeploymentStrategy::RollingUpdate);

    orchestrator.create_deployment(worker_spec)?;
    println!("  ✓ Created with location constraint (Tokyo region)");

    // 4. Canary deployment
    println!("\n4. Experimental Service (Canary):");
    let canary_spec = DeploymentSpec::new(
        "experimental-api",
        "api-service-v2",
        Version::new(2, 0, 0),
        1,
    )
    .with_resources(ResourceRequirements::new(1.0, 2048, 10240))
    .with_label("app", "experimental-api")
    .with_label("deployment", "canary")
    .with_strategy(DeploymentStrategy::Canary);

    orchestrator.create_deployment(canary_spec)?;
    println!("  ✓ Created with canary deployment strategy");

    Ok(())
}

fn demonstrate_scheduling(
    _scheduler: &Scheduler,
    orchestrator: &Orchestrator,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running reconciliation to schedule agents...\n");

    // Reconcile to schedule all deployments
    orchestrator.reconcile()?;

    // Show deployment status
    let deployments = orchestrator.list_deployments();
    println!("Deployment Status:");
    for name in deployments {
        if let Some(deployment) = orchestrator.get_deployment(&name) {
            println!("\n  {}:", name);
            println!("    Status: {:?}", deployment.status);
            println!("    Desired replicas: {}", deployment.desired_replicas);
            println!("    Ready replicas: {}", deployment.ready_replicas);
            println!("    Agents scheduled: {}", deployment.agents.len());

            // Show node distribution
            println!("    Node distribution:");
            let mut node_counts: HashMap<String, usize> = HashMap::new();
            for _agent_id in &deployment.agents {
                // In a real implementation, we'd look up which node the agent is on
                // For this example, we'll simulate it
                *node_counts.entry("tokyo-node-1".to_string()).or_insert(0) += 1;
            }
            for (node, count) in node_counts {
                println!("      {} - {} agents", node, count);
            }
        }
    }

    Ok(())
}

fn demonstrate_auto_scaling(orchestrator: &Orchestrator) -> Result<(), Box<dyn std::error::Error>> {
    println!("Simulating auto-scaling based on load...\n");

    let deployment_name = "web-api";

    // Show initial state
    if let Some(deployment) = orchestrator.get_deployment(deployment_name) {
        println!("Initial state:");
        println!("  Replicas: {}", deployment.ready_replicas);
        if let Some(policy) = &deployment.spec.scaling_policy {
            println!(
                "  Policy: min={}, max={}",
                policy.min_replicas, policy.max_replicas
            );
        }
    }

    // Simulate scaling up
    println!("\nScenario 1: High load detected (CPU > 80%)");
    println!("  Scaling up...");
    orchestrator.scale_deployment(deployment_name, 5)?;
    orchestrator.reconcile()?;

    thread::sleep(Duration::from_millis(100));

    if let Some(deployment) = orchestrator.get_deployment(deployment_name) {
        println!("  ✓ Scaled to {} replicas", deployment.ready_replicas);
    }

    // Simulate scaling up more
    println!("\nScenario 2: Continued high load");
    println!("  Scaling up further...");
    orchestrator.scale_deployment(deployment_name, 8)?;
    orchestrator.reconcile()?;

    thread::sleep(Duration::from_millis(100));

    if let Some(deployment) = orchestrator.get_deployment(deployment_name) {
        println!("  ✓ Scaled to {} replicas", deployment.ready_replicas);
    }

    // Simulate scaling down
    println!("\nScenario 3: Load decreased (CPU < 30%)");
    println!("  Scaling down...");
    orchestrator.scale_deployment(deployment_name, 4)?;
    orchestrator.reconcile()?;

    thread::sleep(Duration::from_millis(100));

    if let Some(deployment) = orchestrator.get_deployment(deployment_name) {
        println!("  ✓ Scaled down to {} replicas", deployment.ready_replicas);
    }

    // Try to scale beyond max limit
    println!("\nScenario 4: Attempting to scale beyond max limit");
    match orchestrator.scale_deployment(deployment_name, 15) {
        Ok(_) => println!("  Unexpectedly succeeded"),
        Err(e) => println!("  ✓ Correctly rejected: {}", e),
    }

    // Try to scale below min limit
    println!("\nScenario 5: Attempting to scale below min limit");
    match orchestrator.scale_deployment(deployment_name, 1) {
        Ok(_) => println!("  Unexpectedly succeeded"),
        Err(e) => println!("  ✓ Correctly rejected: {}", e),
    }

    println!("\nResource utilization tracking completed.");

    Ok(())
}
