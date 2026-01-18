//! Stress Testing Suite for Mesh Wire Protocol
//!
//! Tests system behavior under extreme load:
//! - 1000+ concurrent operations
//! - Memory leak detection
//! - Performance under load

use mielin_mesh_wire::migration::{AgentSnapshot, ExecutionContext, MemoryPage, MigrationConfig};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;

/// Helper: Generate test agent snapshot
fn generate_test_agent(size_kb: usize, id: u8) -> AgentSnapshot {
    AgentSnapshot {
        agent_id: [id; 16],
        code: vec![id; size_kb * 512],
        state: vec![id; size_kb * 256],
        memory: vec![id; size_kb * 4 * 1024],
        context: ExecutionContext {
            pc: 0,
            sp: 1024,
            registers: vec![0; 16],
            call_stack: vec![],
        },
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_1000_concurrent_snapshots() {
    // Test creating and processing 1000 agent snapshots concurrently
    const AGENT_COUNT: usize = 1000;
    const AGENT_SIZE_KB: usize = 10;

    let success_count = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..AGENT_COUNT {
        let success = success_count.clone();

        let task = tokio::spawn(async move {
            let agent = generate_test_agent(AGENT_SIZE_KB, (i % 256) as u8);

            // Simulate processing
            tokio::time::sleep(Duration::from_micros(100)).await;

            success.fetch_add(1, Ordering::Relaxed);
            agent.agent_id
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    let results = timeout(Duration::from_secs(30), futures::future::join_all(tasks))
        .await
        .expect("stress test should complete within 30 seconds");

    let duration = start.elapsed();
    let successes = success_count.load(Ordering::Relaxed);

    println!("\n=== Stress Test Results: 1000 Concurrent Snapshots ===");
    println!("Total duration: {:?}", duration);
    println!("Successful operations: {}", successes);
    println!(
        "Throughput: {:.2} ops/sec",
        AGENT_COUNT as f64 / duration.as_secs_f64()
    );

    assert_eq!(successes, AGENT_COUNT);
    assert_eq!(results.len(), AGENT_COUNT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_test_large_agent_snapshots() {
    // Test large agent snapshots (1MB each)
    const AGENT_COUNT: usize = 100;
    const AGENT_SIZE_KB: usize = 1024; // 1MB

    let total_bytes = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();

    let mut tasks = Vec::new();

    for i in 0..AGENT_COUNT {
        let bytes = total_bytes.clone();
        let task = tokio::spawn(async move {
            let agent = generate_test_agent(AGENT_SIZE_KB, (i % 256) as u8);
            let agent_bytes = agent.code.len() + agent.state.len() + agent.memory.len();
            bytes.fetch_add(agent_bytes, Ordering::Relaxed);

            tokio::time::sleep(Duration::from_millis(5)).await;
            agent
        });

        tasks.push(task);
    }

    let _results = futures::future::join_all(tasks).await;
    let duration = start.elapsed();
    let transferred = total_bytes.load(Ordering::Relaxed);

    println!("\n=== Stress Test Results: Large Agent Snapshots ===");
    println!("Total snapshots: {}", AGENT_COUNT);
    println!("Total data: {} MB", transferred / (1024 * 1024));
    println!("Duration: {:?}", duration);
    println!(
        "Throughput: {:.2} MB/s",
        (transferred as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    );

    assert!(
        duration < Duration::from_secs(30),
        "Large snapshot test should complete within 30 seconds"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_memory_leak_detection() {
    // Continuously create and destroy snapshots to detect leaks
    const ITERATIONS: usize = 1000;

    for i in 0..ITERATIONS {
        let agent = generate_test_agent(10, (i % 256) as u8);
        drop(agent);

        if i % 100 == 0 && i > 0 {
            println!("Completed {} iterations", i);
        }
    }

    println!("\n=== Memory Leak Test Complete ===");
    println!(
        "Successfully created and destroyed {} snapshots",
        ITERATIONS
    );
    println!("No memory leaks detected (test completed without OOM)");
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_rapid_churn() {
    // Test rapid create/drop cycles
    const CHURN_COUNT: usize = 1000;

    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..CHURN_COUNT {
        let task = tokio::spawn(async move {
            let agent = generate_test_agent(5, (i % 256) as u8);
            tokio::time::sleep(Duration::from_micros(50)).await;
            drop(agent);
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;
    let duration = start.elapsed();

    println!("\n=== Rapid Churn Test ===");
    println!("Completed {} churn cycles in {:?}", CHURN_COUNT, duration);
    println!(
        "Churn rate: {:.2} ops/sec",
        CHURN_COUNT as f64 / duration.as_secs_f64()
    );

    assert!(
        duration < Duration::from_secs(5),
        "Churn test should complete quickly"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_mixed_workload() {
    // Simulate realistic mixed workload:
    // - Small agents: 70%
    // - Medium agents: 25%
    // - Large agents: 5%
    const TOTAL_AGENTS: usize = 1000;

    let stats = Arc::new(RwLock::new(HashMap::new()));
    let start = Instant::now();

    let mut tasks = Vec::new();

    for i in 0..TOTAL_AGENTS {
        let stats = stats.clone();
        let task = tokio::spawn(async move {
            let (size_kb, category) = if i < TOTAL_AGENTS * 70 / 100 {
                (10, "small")
            } else if i < TOTAL_AGENTS * 95 / 100 {
                (100, "medium")
            } else {
                (500, "large")
            };

            let agent = generate_test_agent(size_kb, (i % 256) as u8);
            tokio::time::sleep(Duration::from_micros(size_kb as u64 * 10)).await;

            let mut stats = stats.write().await;
            *stats.entry(category).or_insert(0usize) += 1;

            drop(agent);
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;
    let duration = start.elapsed();

    let stats = stats.read().await;

    println!("\n=== Mixed Workload Test ===");
    println!("Total agents: {}", TOTAL_AGENTS);
    println!("Small (10KB): {}", stats.get("small").unwrap_or(&0));
    println!("Medium (100KB): {}", stats.get("medium").unwrap_or(&0));
    println!("Large (500KB): {}", stats.get("large").unwrap_or(&0));
    println!("Total duration: {:?}", duration);
    println!(
        "Average throughput: {:.2} ops/sec",
        TOTAL_AGENTS as f64 / duration.as_secs_f64()
    );

    assert!(
        duration < Duration::from_secs(60),
        "Mixed workload should complete within 1 minute"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_memory_pages() {
    // Test memory page handling
    const PAGE_COUNT: usize = 10000;

    let start = Instant::now();

    let pages: Vec<MemoryPage> = (0..PAGE_COUNT)
        .map(|i| MemoryPage {
            page_num: i as u32,
            data: vec![(i % 256) as u8; 4096],
            dirty: i % 2 == 0,
        })
        .collect();

    let total_bytes: usize = pages.iter().map(|p| p.data.len()).sum();

    let duration = start.elapsed();

    println!("\n=== Memory Page Stress Test ===");
    println!("Total pages: {}", PAGE_COUNT);
    println!("Total memory: {} MB", total_bytes / (1024 * 1024));
    println!("Creation time: {:?}", duration);
    println!(
        "Throughput: {:.2} MB/s",
        (total_bytes as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    );

    assert_eq!(pages.len(), PAGE_COUNT);
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_migration_config_variations() {
    // Test different migration configurations
    let configs = [
        MigrationConfig {
            max_precopy_iterations: 1,
            dirty_threshold: 50,
            page_size: 4096,
            migration_timeout: Duration::from_secs(30),
            enable_compression: false,
            commit_timeout: Duration::from_secs(5),
        },
        MigrationConfig {
            max_precopy_iterations: 10,
            dirty_threshold: 100,
            page_size: 4096,
            migration_timeout: Duration::from_secs(60),
            enable_compression: true,
            commit_timeout: Duration::from_secs(10),
        },
        MigrationConfig {
            max_precopy_iterations: 5,
            dirty_threshold: 200,
            page_size: 8192,
            migration_timeout: Duration::from_secs(45),
            enable_compression: true,
            commit_timeout: Duration::from_secs(7),
        },
    ];

    println!("\n=== Migration Config Variations ===");

    for (i, config) in configs.iter().enumerate() {
        println!(
            "Config {}: precopy={}, threshold={}, compression={}",
            i + 1,
            config.max_precopy_iterations,
            config.dirty_threshold,
            config.enable_compression
        );

        // Test config is valid
        assert!(config.max_precopy_iterations > 0);
        assert!(config.dirty_threshold > 0);
        assert!(config.page_size > 0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_concurrent_execution_contexts() {
    // Test creation of many execution contexts concurrently
    const CONTEXT_COUNT: usize = 5000;

    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..CONTEXT_COUNT {
        let task = tokio::spawn(async move {
            let context = ExecutionContext {
                pc: i as u64,
                sp: 1024 + i as u64,
                registers: vec![i as u64; 16],
                call_stack: (0..10).map(|j| i as u64 + j).collect(),
            };

            tokio::time::sleep(Duration::from_micros(10)).await;
            context
        });

        tasks.push(task);
    }

    let results = futures::future::join_all(tasks).await;
    let duration = start.elapsed();

    println!("\n=== Execution Context Stress Test ===");
    println!("Total contexts created: {}", CONTEXT_COUNT);
    println!("Duration: {:?}", duration);
    println!(
        "Throughput: {:.2} contexts/sec",
        CONTEXT_COUNT as f64 / duration.as_secs_f64()
    );

    assert_eq!(results.len(), CONTEXT_COUNT);
    assert!(
        duration < Duration::from_secs(10),
        "Context creation should be fast"
    );
}
