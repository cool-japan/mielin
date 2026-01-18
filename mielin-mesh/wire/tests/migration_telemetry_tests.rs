//! Migration Telemetry and Metrics Tests
//!
//! Comprehensive telemetry system for tracking migration performance,
//! success rates, and system health during agent migrations.

use mielin_mesh_wire::migration::{AgentSnapshot, ExecutionContext};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Telemetry data for a single migration
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MigrationTelemetry {
    agent_id: [u8; 16],
    start_time: Instant,
    end_time: Option<Instant>,
    agent_size_bytes: usize,
    dirty_pages: usize,
    compression_ratio: f64,
    success: bool,
    downtime_ms: Option<u64>,
    bandwidth_mbps: Option<f64>,
    error: Option<String>,
}

impl MigrationTelemetry {
    fn new(agent_id: [u8; 16], agent_size_bytes: usize, dirty_pages: usize) -> Self {
        Self {
            agent_id,
            start_time: Instant::now(),
            end_time: None,
            agent_size_bytes,
            dirty_pages,
            compression_ratio: 1.0,
            success: false,
            downtime_ms: None,
            bandwidth_mbps: None,
            error: None,
        }
    }

    fn complete_success(&mut self, downtime_ms: u64) {
        self.end_time = Some(Instant::now());
        self.success = true;
        self.downtime_ms = Some(downtime_ms);

        // Calculate bandwidth
        if let Some(end) = self.end_time {
            let duration_sec = (end - self.start_time).as_secs_f64();
            let size_mb = self.agent_size_bytes as f64 / (1024.0 * 1024.0);
            self.bandwidth_mbps = Some(size_mb / duration_sec);
        }
    }

    fn complete_failure(&mut self, error: String) {
        self.end_time = Some(Instant::now());
        self.success = false;
        self.error = Some(error);
    }

    fn duration_ms(&self) -> Option<u64> {
        self.end_time
            .map(|end| (end - self.start_time).as_millis() as u64)
    }
}

/// System-wide telemetry aggregator
#[derive(Debug)]
struct TelemetryAggregator {
    migrations: Arc<RwLock<Vec<MigrationTelemetry>>>,
    total_migrations: AtomicUsize,
    successful_migrations: AtomicUsize,
    failed_migrations: AtomicUsize,
    total_bytes_transferred: AtomicU64,
    total_downtime_ms: AtomicU64,
    peak_concurrent_migrations: AtomicUsize,
    current_concurrent_migrations: AtomicUsize,
}

impl TelemetryAggregator {
    fn new() -> Self {
        Self {
            migrations: Arc::new(RwLock::new(Vec::new())),
            total_migrations: AtomicUsize::new(0),
            successful_migrations: AtomicUsize::new(0),
            failed_migrations: AtomicUsize::new(0),
            total_bytes_transferred: AtomicU64::new(0),
            total_downtime_ms: AtomicU64::new(0),
            peak_concurrent_migrations: AtomicUsize::new(0),
            current_concurrent_migrations: AtomicUsize::new(0),
        }
    }

    async fn start_migration(
        &self,
        agent_id: [u8; 16],
        size_bytes: usize,
        dirty_pages: usize,
    ) -> MigrationTelemetry {
        self.total_migrations.fetch_add(1, Ordering::Relaxed);

        let current = self
            .current_concurrent_migrations
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        let mut peak = self.peak_concurrent_migrations.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_concurrent_migrations.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }

        MigrationTelemetry::new(agent_id, size_bytes, dirty_pages)
    }

    async fn complete_migration(&self, telemetry: MigrationTelemetry) {
        self.current_concurrent_migrations
            .fetch_sub(1, Ordering::Relaxed);

        if telemetry.success {
            self.successful_migrations.fetch_add(1, Ordering::Relaxed);
            self.total_bytes_transferred
                .fetch_add(telemetry.agent_size_bytes as u64, Ordering::Relaxed);
            if let Some(downtime) = telemetry.downtime_ms {
                self.total_downtime_ms
                    .fetch_add(downtime, Ordering::Relaxed);
            }
        } else {
            self.failed_migrations.fetch_add(1, Ordering::Relaxed);
        }

        self.migrations.write().await.push(telemetry);
    }

    async fn generate_report(&self) -> TelemetryReport {
        let migrations = self.migrations.read().await;

        let total = self.total_migrations.load(Ordering::Relaxed);
        let successful = self.successful_migrations.load(Ordering::Relaxed);
        let failed = self.failed_migrations.load(Ordering::Relaxed);
        let success_rate = if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        // Calculate percentiles for successful migrations
        let mut durations: Vec<u64> = migrations
            .iter()
            .filter(|m| m.success)
            .filter_map(|m| m.duration_ms())
            .collect();
        durations.sort_unstable();

        let p50 = percentile(&durations, 50);
        let p90 = percentile(&durations, 90);
        let p99 = percentile(&durations, 99);

        // Calculate bandwidth statistics
        let bandwidths: Vec<f64> = migrations.iter().filter_map(|m| m.bandwidth_mbps).collect();
        let avg_bandwidth = if !bandwidths.is_empty() {
            bandwidths.iter().sum::<f64>() / bandwidths.len() as f64
        } else {
            0.0
        };

        // Calculate average downtime
        let avg_downtime = if successful > 0 {
            self.total_downtime_ms.load(Ordering::Relaxed) as f64 / successful as f64
        } else {
            0.0
        };

        TelemetryReport {
            total_migrations: total,
            successful_migrations: successful,
            failed_migrations: failed,
            success_rate_percent: success_rate,
            avg_migration_time_ms: if successful > 0 {
                durations.iter().sum::<u64>() as f64 / successful as f64
            } else {
                0.0
            },
            p50_migration_time_ms: p50,
            p90_migration_time_ms: p90,
            p99_migration_time_ms: p99,
            avg_downtime_ms: avg_downtime,
            total_bytes_transferred: self.total_bytes_transferred.load(Ordering::Relaxed),
            avg_bandwidth_mbps: avg_bandwidth,
            peak_concurrent_migrations: self.peak_concurrent_migrations.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
struct TelemetryReport {
    total_migrations: usize,
    successful_migrations: usize,
    failed_migrations: usize,
    success_rate_percent: f64,
    avg_migration_time_ms: f64,
    p50_migration_time_ms: u64,
    p90_migration_time_ms: u64,
    p99_migration_time_ms: u64,
    avg_downtime_ms: f64,
    total_bytes_transferred: u64,
    avg_bandwidth_mbps: f64,
    peak_concurrent_migrations: usize,
}

impl TelemetryReport {
    fn print(&self) {
        println!("\n{}", "═".repeat(80));
        println!("                      MIGRATION TELEMETRY REPORT");
        println!("{}", "═".repeat(80));

        println!("\n📊 Overall Statistics");
        println!("  Total migrations:         {}", self.total_migrations);
        println!("  Successful migrations:    {}", self.successful_migrations);
        println!("  Failed migrations:        {}", self.failed_migrations);
        println!(
            "  Success rate:             {:.2}%",
            self.success_rate_percent
        );
        println!(
            "  Peak concurrent:          {}",
            self.peak_concurrent_migrations
        );

        println!("\n⏱️  Timing Statistics");
        println!(
            "  Average migration time:   {:.2} ms",
            self.avg_migration_time_ms
        );
        println!(
            "  P50 migration time:       {} ms",
            self.p50_migration_time_ms
        );
        println!(
            "  P90 migration time:       {} ms",
            self.p90_migration_time_ms
        );
        println!(
            "  P99 migration time:       {} ms",
            self.p99_migration_time_ms
        );
        println!("  Average downtime:         {:.2} ms", self.avg_downtime_ms);

        println!("\n📡 Data Transfer Statistics");
        println!(
            "  Total bytes transferred:  {:.2} MB",
            self.total_bytes_transferred as f64 / (1024.0 * 1024.0)
        );
        println!(
            "  Average bandwidth:        {:.2} MB/s",
            self.avg_bandwidth_mbps
        );

        println!("\n{}", "═".repeat(80));
    }
}

fn percentile(sorted_values: &[u64], percentile: u8) -> u64 {
    if sorted_values.is_empty() {
        return 0;
    }
    let idx = (sorted_values.len() as f64 * percentile as f64 / 100.0) as usize;
    let idx = idx.min(sorted_values.len() - 1);
    sorted_values[idx]
}

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
async fn telemetry_1000_migrations_with_metrics() {
    println!("\n🚀 Starting migration telemetry test: 1000 concurrent migrations");

    const AGENT_COUNT: usize = 1000;
    const AGENT_SIZE_KB: usize = 10;

    let aggregator = Arc::new(TelemetryAggregator::new());
    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..AGENT_COUNT {
        let agg = aggregator.clone();

        let task = tokio::spawn(async move {
            let agent = generate_test_agent(AGENT_SIZE_KB, (i % 256) as u8);
            let size_bytes = agent.code.len() + agent.state.len() + agent.memory.len();

            let mut telemetry = agg.start_migration(agent.agent_id, size_bytes, 0).await;

            // Simulate migration process
            tokio::time::sleep(Duration::from_micros(100)).await;

            // Simulate downtime (time when agent is paused)
            let downtime_start = Instant::now();
            tokio::time::sleep(Duration::from_micros(50)).await;
            let downtime_ms = downtime_start.elapsed().as_millis() as u64;

            // Complete with success
            telemetry.complete_success(downtime_ms);
            agg.complete_migration(telemetry).await;
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;
    let total_duration = start.elapsed();

    println!(
        "\n✅ Test completed in {:.2}s",
        total_duration.as_secs_f64()
    );

    let report = aggregator.generate_report().await;
    report.print();

    // Assertions
    assert_eq!(report.total_migrations, AGENT_COUNT);
    assert_eq!(report.successful_migrations, AGENT_COUNT);
    assert_eq!(report.failed_migrations, 0);
    assert_eq!(report.success_rate_percent, 100.0);
    assert!(report.avg_downtime_ms < 10.0); // Sub-10ms downtime (excellent performance)
    assert!(report.p99_migration_time_ms < 500); // P99 < 500ms
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn telemetry_mixed_size_migrations() {
    println!("\n🚀 Starting mixed-size migration telemetry test");

    const TOTAL_AGENTS: usize = 500;

    let aggregator = Arc::new(TelemetryAggregator::new());
    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..TOTAL_AGENTS {
        let agg = aggregator.clone();

        let task = tokio::spawn(async move {
            // Mixed sizes: 70% small, 25% medium, 5% large
            let size_kb = if i < TOTAL_AGENTS * 70 / 100 {
                10 // Small
            } else if i < TOTAL_AGENTS * 95 / 100 {
                100 // Medium
            } else {
                500 // Large
            };

            let agent = generate_test_agent(size_kb, (i % 256) as u8);
            let size_bytes = agent.code.len() + agent.state.len() + agent.memory.len();

            let mut telemetry = agg.start_migration(agent.agent_id, size_bytes, 0).await;

            // Simulate migration with size-dependent delay
            tokio::time::sleep(Duration::from_micros(size_kb as u64 * 10)).await;

            let downtime_ms = (size_kb / 100) as u64; // Proportional downtime
            telemetry.complete_success(downtime_ms);
            agg.complete_migration(telemetry).await;
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;
    let total_duration = start.elapsed();

    println!(
        "\n✅ Mixed-size test completed in {:.2}s",
        total_duration.as_secs_f64()
    );

    let report = aggregator.generate_report().await;
    report.print();

    // Assertions
    assert_eq!(report.total_migrations, TOTAL_AGENTS);
    assert_eq!(report.success_rate_percent, 100.0);
    assert!(report.avg_bandwidth_mbps > 0.0);
}

#[tokio::test(flavor = "multi_thread")]
async fn telemetry_with_failures() {
    println!("\n🚀 Starting migration telemetry test with simulated failures");

    const AGENT_COUNT: usize = 100;
    const FAILURE_RATE: usize = 10; // 10% failure rate

    let aggregator = Arc::new(TelemetryAggregator::new());
    let mut tasks = Vec::new();

    for i in 0..AGENT_COUNT {
        let agg = aggregator.clone();

        let task = tokio::spawn(async move {
            let agent = generate_test_agent(10, (i % 256) as u8);
            let size_bytes = agent.code.len() + agent.state.len() + agent.memory.len();

            let mut telemetry = agg.start_migration(agent.agent_id, size_bytes, 0).await;

            tokio::time::sleep(Duration::from_micros(100)).await;

            // Simulate some failures
            if i % FAILURE_RATE == 0 {
                telemetry.complete_failure("Simulated network timeout".to_string());
            } else {
                telemetry.complete_success(10);
            }

            agg.complete_migration(telemetry).await;
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;

    let report = aggregator.generate_report().await;
    report.print();

    // Assertions
    assert_eq!(report.total_migrations, AGENT_COUNT);
    assert_eq!(report.failed_migrations, AGENT_COUNT / FAILURE_RATE);
    assert!(report.success_rate_percent >= 85.0 && report.success_rate_percent <= 95.0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn telemetry_high_concurrency_stress() {
    println!("\n🚀 High concurrency stress test with telemetry");

    const AGENT_COUNT: usize = 2000;
    const AGENT_SIZE_KB: usize = 5;

    let aggregator = Arc::new(TelemetryAggregator::new());
    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..AGENT_COUNT {
        let agg = aggregator.clone();

        let task = tokio::spawn(async move {
            let agent = generate_test_agent(AGENT_SIZE_KB, (i % 256) as u8);
            let size_bytes = agent.code.len() + agent.state.len() + agent.memory.len();

            let mut telemetry = agg.start_migration(agent.agent_id, size_bytes, 0).await;

            tokio::time::sleep(Duration::from_micros(50)).await;

            telemetry.complete_success(5);
            agg.complete_migration(telemetry).await;
        });

        tasks.push(task);
    }

    futures::future::join_all(tasks).await;
    let total_duration = start.elapsed();

    println!(
        "\n✅ High concurrency test completed in {:.2}s",
        total_duration.as_secs_f64()
    );

    let report = aggregator.generate_report().await;
    report.print();

    println!("\n🎯 Concurrency Statistics:");
    println!(
        "  Peak concurrent migrations: {}",
        report.peak_concurrent_migrations
    );
    println!(
        "  Average throughput:         {:.0} migrations/sec",
        AGENT_COUNT as f64 / total_duration.as_secs_f64()
    );

    // Assertions
    assert_eq!(report.total_migrations, AGENT_COUNT);
    assert_eq!(report.success_rate_percent, 100.0);
    assert!(report.peak_concurrent_migrations > 100); // Should have high concurrency
}
