//! Resource management example
//!
//! This example demonstrates:
//! - Defining resource quotas
//! - Enforcing resource limits
//! - Monitoring resource usage
//! - Detecting anomalies
//! - Predicting future resource needs

use mielin_cells::{
    AnomalyConfig, AnomalyDetector, HistoryConfig, QuotaEnforcer, ResourceHistory, ResourceManager,
    ResourceMonitor, ResourcePredictor, ResourceQuota, ResourceUsage,
};

fn main() {
    println!("=== Resource Management Example ===\n");

    // Example 1: Resource quotas
    println!("1. Resource Quotas:");

    let default_quota = ResourceQuota::default();
    println!("   Default quota:");
    println!(
        "   - Max heap: {} MB",
        default_quota.memory.max_heap_bytes / (1024 * 1024)
    );
    println!(
        "   - Max CPU time: {} ms",
        default_quota.cpu.max_execution_time_us / 1000
    );
    println!(
        "   - Max network send: {} MB/s",
        default_quota.network.max_send_bytes_per_sec / (1024 * 1024)
    );

    let minimal_quota = ResourceQuota::minimal();
    println!("\n   Minimal quota:");
    println!(
        "   - Max heap: {} MB",
        minimal_quota.memory.max_heap_bytes / (1024 * 1024)
    );

    let hp_quota = ResourceQuota::high_performance();
    println!("\n   High-performance quota:");
    println!("   - CPU priority: {}", hp_quota.cpu.priority);

    // Example 2: Quota enforcement
    println!("\n2. Quota Enforcement:");

    let quota = ResourceQuota::default();
    let mut enforcer = QuotaEnforcer::new(quota);

    println!("   Checking memory allocation...");
    if enforcer.can_allocate_memory(1024 * 1024) {
        println!("   ✓ Can allocate 1MB");
        enforcer.record_allocation(1024 * 1024);
        println!(
            "   Current memory usage: {} bytes",
            enforcer.usage().heap_bytes
        );
    }

    println!("\n   Checking CPU time...");
    if enforcer.can_use_cpu(5_000_000) {
        println!("   ✓ Can use 5 seconds of CPU time");
        enforcer.record_cpu_time(5_000_000);
        println!(
            "   Total CPU time: {} μs",
            enforcer.usage().total_cpu_time_us
        );
    }

    println!("\n   Checking network send...");
    if enforcer.can_send(1024 * 1024) {
        println!("   ✓ Can send 1MB");
        enforcer.record_send(1024 * 1024);
        println!("   Bytes sent: {}", enforcer.usage().bytes_sent_this_second);
    }

    // Check quota violations
    let result = enforcer.check();
    if result.within_limits {
        println!("\n   ✓ All resources within limits");
    } else {
        println!("\n   ✗ Quota violations:");
        for violation in &result.violations {
            println!("   - {}", violation.description());
        }
    }

    // Example 3: Resource manager
    println!("\n3. Resource Manager:");

    let mut manager = ResourceManager::new();

    let agent_id1 = [1u8; 16];
    let agent_id2 = [2u8; 16];

    manager.register_agent(agent_id1);
    manager.register_agent_with_quota(agent_id2, ResourceQuota::minimal());

    println!("   Registered 2 agents");
    println!("   Agent count: {}", manager.agent_count());

    // Set global memory limit
    manager.set_global_memory_limit(1024 * 1024 * 1024); // 1GB
    println!("   Global memory limit: 1GB");

    // Record some usage
    if let Some(enforcer) = manager.get_enforcer_mut(&agent_id1) {
        enforcer.record_allocation(10 * 1024 * 1024); // 10MB
    }

    println!(
        "   Total memory used: {} MB",
        manager.total_memory_used() / (1024 * 1024)
    );

    // Get usage summary
    let summary = manager.usage_summary();
    println!("   Usage summary:");
    for (agent_id, usage) in summary {
        println!("   - Agent {:?}: {} bytes", agent_id, usage.heap_bytes);
    }

    // Example 4: Resource monitoring
    println!("\n4. Resource Monitoring:");

    let monitor_id = [3u8; 16];
    let mut monitor = ResourceMonitor::new(monitor_id);

    // Record some usage over time
    for i in 0..10 {
        let usage = ResourceUsage {
            heap_bytes: (i + 1) * 1024 * 1024,
            total_memory_bytes: (i + 1) * 1024 * 1024,
            total_cpu_time_us: i * 100_000,
            ..Default::default()
        };
        monitor.record_and_analyze(&usage, i * 1_000_000);
    }

    println!("   Recorded 10 usage snapshots");
    println!("   History length: {}", monitor.history().len());

    let summary = monitor.summary();
    println!("\n   Summary:");
    println!("   - Agent ID: {:?}", summary.agent_id);
    println!("   - Snapshot count: {}", summary.snapshot_count);
    println!(
        "   - Peak memory: {} MB",
        summary.peak_memory_bytes / (1024 * 1024)
    );
    println!("   - Peak CPU: {} μs", summary.peak_cpu_time_us);
    println!("   - Anomaly count: {}", summary.anomaly_count);

    // Example 5: Anomaly detection
    println!("\n5. Anomaly Detection:");

    let anomaly_config = AnomalyConfig::strict();
    let mut detector = AnomalyDetector::new(anomaly_config);

    let anomaly_id = [4u8; 16];
    let mut history = ResourceHistory::new(anomaly_id, HistoryConfig::default());

    // Record normal usage
    for i in 0..5 {
        let usage = ResourceUsage {
            total_memory_bytes: 1000,
            ..Default::default()
        };
        history.record(&usage, i * 1_000_000);
    }

    // Add a spike
    let spike_usage = ResourceUsage {
        total_memory_bytes: 5000,
        ..Default::default()
    };
    history.record(&spike_usage, 5_000_000);

    let anomalies = detector.analyze(&history, 5_000_000);

    if !anomalies.is_empty() {
        println!("   ✓ Detected {} anomalies", anomalies.len());
        for anomaly in &anomalies {
            println!("   - Type: {:?}", anomaly.anomaly_type);
            println!("     Description: {}", anomaly.anomaly_type.description());
            println!("     Severity: {:.2}", anomaly.severity);
        }
    }

    // Example 6: Resource prediction
    println!("\n6. Resource Prediction:");

    let predictor = ResourcePredictor::new().with_min_samples(3);

    let pred_id = [5u8; 16];
    let mut pred_history = ResourceHistory::new(pred_id, HistoryConfig::default());

    // Record growing usage
    for i in 0..10 {
        let usage = ResourceUsage {
            total_memory_bytes: (i + 1) * 1_000_000,
            total_cpu_time_us: i * 100_000,
            ..Default::default()
        };
        pred_history.record(&usage, i * 1_000_000);
    }

    if let Some(prediction) = predictor.predict(&pred_history, 1_000_000) {
        println!("   ✓ Resource prediction (1s ahead):");
        println!(
            "   - Predicted memory: {} MB",
            prediction.memory_bytes / 1_000_000
        );
        println!("   - Predicted CPU: {:.2}%", prediction.cpu_percent);
        println!("   - Confidence: {:.2}%", prediction.confidence * 100.0);
        println!("   - Horizon: {} μs", prediction.horizon_us);
    }

    // Check for quota breach prediction
    let mut test_quota = ResourceQuota::default();
    test_quota.memory.max_heap_bytes = 15 * 1024 * 1024;
    if let Some(breach_time) = predictor.predict_quota_breach(&pred_history, &test_quota) {
        println!("\n   ⚠ Quota breach predicted in {} μs", breach_time);
    } else {
        println!("\n   ✓ No quota breach predicted");
    }

    println!("\n=== Example Complete ===");
}
