//! Connection Pool Stress Test Framework
//!
//! Provides a fully in-process load-test harness that models the per-transport
//! 100-connection cap from `ConnectionPool::DEFAULT_MAX_CONNECTIONS`.
//!
//! # Architecture
//!
//! `LoadTestRunner::run_simulated` spawns `config.target_connections` Tokio
//! tasks.  Each task:
//!
//! 1. Acquires a permit from a `tokio::sync::Semaphore` (capacity = 100,
//!    matching the transport cap).
//! 2. Simulates connection setup via a short random sleep (derived from a
//!    deterministic xorshift64 PRNG — **no `rand`/`ndarray`**).
//! 3. Simulates work during the "hold" phase.
//! 4. Releases the permit (drops it), simulating disconnection.
//!
//! This correctly models the constraint: at most 100 "logical" connections are
//! active concurrently, while up to `target_connections` (e.g. 10 000) tasks
//! cycle through that pool.
//!
//! All counters (`attempted`, `succeeded`, `failed`, `peak_concurrent`) use
//! `Arc<AtomicUsize>` / `Arc<AtomicU64>` for lock-free updates from every
//! spawned task.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Xorshift64 PRNG (no rand/ndarray)
// ---------------------------------------------------------------------------

/// Minimal xorshift64 pseudo-random number generator.
/// Used to generate per-task jitter without pulling in `rand`.
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// Seed must be non-zero.
    fn new(seed: u64) -> Self {
        // Ensure non-zero; use a known non-zero fallback.
        let state = if seed == 0 { 0x853c49e6748fea9b } else { seed };
        Self { state }
    }

    /// Return the next pseudo-random u64.
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Return a pseudo-random u64 in `[0, limit)`.
    fn next_bounded(&mut self, limit: u64) -> u64 {
        if limit == 0 {
            return 0;
        }
        self.next() % limit
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Parameters controlling a single load-test run
#[derive(Debug, Clone)]
pub struct LoadTestConfig {
    /// Total number of connection attempts to simulate
    pub target_connections: usize,
    /// Duration over which connections are ramped up (milliseconds)
    pub ramp_up_ms: u64,
    /// Duration each simulated connection is held open (milliseconds)
    pub hold_ms: u64,
    /// Duration over which connections ramp down (milliseconds; informational)
    pub ramp_down_ms: u64,
    /// Maximum simulated message payload (bytes; currently unused but
    /// reserved for future protocol-layer simulation)
    pub max_message_size: usize,
    /// Semaphore capacity — defaults to 100 to match `ConnectionPool`
    pub semaphore_capacity: usize,
    /// When `true`, tasks that cannot acquire a permit within a timeout
    /// are counted as failures instead of blocking indefinitely
    pub fail_on_timeout: bool,
    /// Per-task permit acquisition timeout (only active when `fail_on_timeout`)
    pub acquire_timeout: Duration,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            target_connections: 100,
            ramp_up_ms: 0,
            hold_ms: 1,
            ramp_down_ms: 0,
            max_message_size: 1024,
            semaphore_capacity: 100,
            fail_on_timeout: false,
            acquire_timeout: Duration::from_secs(30),
        }
    }
}

impl LoadTestConfig {
    /// Create a configuration for a 10 000-connection load test exercising the
    /// 100-cap constraint, with deterministic zero-duration sleeps for fast CI.
    pub fn ten_thousand_cap_100() -> Self {
        Self {
            target_connections: 10_000,
            ramp_up_ms: 0,
            hold_ms: 0,
            ramp_down_ms: 0,
            max_message_size: 256,
            semaphore_capacity: 100,
            fail_on_timeout: false,
            acquire_timeout: Duration::from_secs(60),
        }
    }

    /// Create a configuration with an effectively infinite semaphore (for
    /// sanity-checking that all connections succeed when there is no cap).
    pub fn uncapped(target: usize) -> Self {
        Self {
            target_connections: target,
            semaphore_capacity: target + 1,
            hold_ms: 0,
            ramp_up_ms: 0,
            ramp_down_ms: 0,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Summary of a completed load-test run
#[derive(Debug, Clone)]
pub struct LoadTestResult {
    /// Total connection attempts initiated
    pub connections_attempted: usize,
    /// Number of connections that were successfully established and closed
    pub connections_succeeded: usize,
    /// Number of connections that failed (e.g. permit timeout)
    pub connections_failed: usize,
    /// Peak simultaneous concurrent connections observed
    pub peak_concurrent: usize,
    /// Total simulated messages sent across all connections
    pub messages_sent: usize,
    /// Error messages collected during the run
    pub errors: Vec<String>,
    /// Wall-clock duration of the run
    pub duration: Duration,
}

impl LoadTestResult {
    /// Fraction of attempted connections that succeeded (0.0 – 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.connections_attempted == 0 {
            return 1.0;
        }
        self.connections_succeeded as f64 / self.connections_attempted as f64
    }

    /// Returns `true` when every attempted connection succeeded
    pub fn all_succeeded(&self) -> bool {
        self.connections_failed == 0 && self.connections_succeeded == self.connections_attempted
    }

    /// Throughput: connections completed per second
    pub fn throughput(&self) -> f64 {
        if self.duration.as_secs_f64() == 0.0 {
            return 0.0;
        }
        self.connections_succeeded as f64 / self.duration.as_secs_f64()
    }
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Executes a simulated load test entirely in-process using Tokio tasks and
/// a `Semaphore` to model the per-transport connection cap.
pub struct LoadTestRunner {
    config: LoadTestConfig,
}

impl LoadTestRunner {
    /// Create a runner with the given configuration
    pub fn new(config: LoadTestConfig) -> Self {
        Self { config }
    }

    /// Run a fully simulated load test (no network I/O required).
    ///
    /// Spawns `config.target_connections` Tokio tasks.  Each task:
    /// - acquires a semaphore permit (capacity = `config.semaphore_capacity`)
    /// - sleeps for a small pseudo-random duration (up to `config.hold_ms`)
    /// - releases the permit
    ///
    /// Atomic counters accumulate results; peak concurrency is tracked with
    /// a dedicated `AtomicUsize` updated on every acquire/release cycle.
    pub async fn run_simulated(&self) -> LoadTestResult {
        let wall_start = Instant::now();
        let config = &self.config;

        let semaphore = Arc::new(Semaphore::new(config.semaphore_capacity));
        let attempted = Arc::new(AtomicUsize::new(0));
        let succeeded = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));
        let messages_sent = Arc::new(AtomicU64::new(0));
        let concurrent = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let errors: Arc<tokio::sync::Mutex<Vec<String>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));

        // Compute per-task ramp-up delay from total budget.
        let total_connections = config.target_connections;
        let ramp_up_per_task_us: u64 = if total_connections > 1 && config.ramp_up_ms > 0 {
            (config.ramp_up_ms * 1000) / (total_connections as u64)
        } else {
            0
        };

        let hold_ms = config.hold_ms;
        let fail_on_timeout = config.fail_on_timeout;
        let acquire_timeout = config.acquire_timeout;

        let mut task_handles = Vec::with_capacity(total_connections);

        for task_idx in 0..total_connections {
            let sem = semaphore.clone();
            let att = attempted.clone();
            let succ = succeeded.clone();
            let fail = failed.clone();
            let msgs = messages_sent.clone();
            let cur = concurrent.clone();
            let pk = peak.clone();
            let errs = errors.clone();

            let handle = tokio::spawn(async move {
                // Staggered ramp-up: each task waits proportionally longer
                if ramp_up_per_task_us > 0 {
                    let delay_us = ramp_up_per_task_us * task_idx as u64;
                    tokio::time::sleep(Duration::from_micros(delay_us)).await;
                }

                att.fetch_add(1, Ordering::Relaxed);

                // Attempt to acquire a semaphore permit
                let permit_result = if fail_on_timeout {
                    tokio::time::timeout(acquire_timeout, sem.acquire_owned())
                        .await
                        .map_err(|_| "permit acquire timeout".to_string())
                        .and_then(|r| r.map_err(|e| e.to_string()))
                } else {
                    sem.acquire_owned().await.map_err(|e| e.to_string())
                };

                match permit_result {
                    Ok(_permit) => {
                        // Track peak concurrency
                        let current = cur.fetch_add(1, Ordering::Relaxed) + 1;
                        // Update peak using CAS loop
                        let mut old_peak = pk.load(Ordering::Relaxed);
                        while current > old_peak {
                            match pk.compare_exchange_weak(
                                old_peak,
                                current,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => break,
                                Err(actual) => old_peak = actual,
                            }
                        }

                        // Simulate connection hold time with a small PRNG-seeded delay
                        if hold_ms > 0 {
                            // Seed each task's PRNG from task index to get
                            // deterministic but varied delays.
                            let mut rng =
                                Xorshift64::new(0xdeadbeef_u64.wrapping_add(task_idx as u64));
                            let jitter_us = rng.next_bounded(hold_ms * 1000 + 1);
                            if jitter_us > 0 {
                                tokio::time::sleep(Duration::from_micros(jitter_us)).await;
                            }
                        }

                        // Simulate sending one message per connection
                        msgs.fetch_add(1, Ordering::Relaxed);

                        // Release permit (drop it)
                        cur.fetch_sub(1, Ordering::Relaxed);
                        succ.fetch_add(1, Ordering::Relaxed);

                        debug!("task {} completed", task_idx);
                    }
                    Err(e) => {
                        fail.fetch_add(1, Ordering::Relaxed);
                        let mut guard = errs.lock().await;
                        guard.push(format!("task {}: {}", task_idx, e));
                    }
                }
            });

            task_handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in task_handles {
            if let Err(e) = handle.await {
                let mut guard = errors.lock().await;
                guard.push(format!("task panicked: {:?}", e));
            }
        }

        let duration = wall_start.elapsed();
        let error_snapshot = errors.lock().await.clone();

        let result = LoadTestResult {
            connections_attempted: attempted.load(Ordering::Relaxed),
            connections_succeeded: succeeded.load(Ordering::Relaxed),
            connections_failed: failed.load(Ordering::Relaxed),
            peak_concurrent: peak.load(Ordering::Relaxed),
            messages_sent: messages_sent.load(Ordering::Relaxed) as usize,
            errors: error_snapshot,
            duration,
        };

        info!(
            "Load test complete: {}/{} succeeded ({:.1}%), peak={}, duration={:?}",
            result.connections_succeeded,
            result.connections_attempted,
            result.success_rate() * 100.0,
            result.peak_concurrent,
            result.duration
        );

        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Xorshift64
    // -----------------------------------------------------------------------

    #[test]
    fn test_xorshift64_non_zero_output() {
        let mut rng = Xorshift64::new(1);
        // None of the first 100 outputs should be zero (astronomically rare)
        for _ in 0..100 {
            assert_ne!(rng.next(), 0);
        }
    }

    #[test]
    fn test_xorshift64_bounded_stays_in_range() {
        let mut rng = Xorshift64::new(42);
        let limit = 17u64;
        for _ in 0..1000 {
            assert!(rng.next_bounded(limit) < limit);
        }
    }

    #[test]
    fn test_xorshift64_zero_seed_handled() {
        let mut rng = Xorshift64::new(0);
        // Should not produce an infinite loop or panic
        let v = rng.next();
        assert_ne!(
            v, 0,
            "xorshift64 with 0 seed must use fallback non-zero state"
        );
    }

    // -----------------------------------------------------------------------
    // LoadTestConfig
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_test_config_default() {
        let cfg = LoadTestConfig::default();
        assert_eq!(cfg.target_connections, 100);
        assert_eq!(cfg.semaphore_capacity, 100);
    }

    #[test]
    fn test_load_test_config_ten_thousand() {
        let cfg = LoadTestConfig::ten_thousand_cap_100();
        assert_eq!(cfg.target_connections, 10_000);
        assert_eq!(cfg.semaphore_capacity, 100);
    }

    #[test]
    fn test_load_test_config_uncapped() {
        let cfg = LoadTestConfig::uncapped(500);
        assert_eq!(cfg.target_connections, 500);
        assert!(cfg.semaphore_capacity > 500);
    }

    // -----------------------------------------------------------------------
    // LoadTestResult methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_success_rate_all_succeed() {
        let r = LoadTestResult {
            connections_attempted: 100,
            connections_succeeded: 100,
            connections_failed: 0,
            peak_concurrent: 10,
            messages_sent: 100,
            errors: vec![],
            duration: Duration::from_secs(1),
        };
        assert!((r.success_rate() - 1.0).abs() < f64::EPSILON);
        assert!(r.all_succeeded());
    }

    #[test]
    fn test_success_rate_partial() {
        let r = LoadTestResult {
            connections_attempted: 200,
            connections_succeeded: 150,
            connections_failed: 50,
            peak_concurrent: 100,
            messages_sent: 150,
            errors: vec!["timeout".to_string(); 50],
            duration: Duration::from_secs(2),
        };
        assert!((r.success_rate() - 0.75).abs() < f64::EPSILON);
        assert!(!r.all_succeeded());
    }

    #[test]
    fn test_success_rate_zero_attempted() {
        let r = LoadTestResult {
            connections_attempted: 0,
            connections_succeeded: 0,
            connections_failed: 0,
            peak_concurrent: 0,
            messages_sent: 0,
            errors: vec![],
            duration: Duration::ZERO,
        };
        // Special case: 0 attempted → 1.0 (no failures)
        assert!((r.success_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_throughput_calculation() {
        let r = LoadTestResult {
            connections_attempted: 100,
            connections_succeeded: 100,
            connections_failed: 0,
            peak_concurrent: 10,
            messages_sent: 100,
            errors: vec![],
            duration: Duration::from_secs(2),
        };
        // 100 / 2.0 = 50.0
        assert!((r.throughput() - 50.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Simulated load tests
    // -----------------------------------------------------------------------

    /// 10 000 logical connections with a 100-cap semaphore must complete
    /// without deadlock.  The test has a generous 30-second wall-clock budget.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_10k_connections_100_cap_no_deadlock() {
        let cfg = LoadTestConfig::ten_thousand_cap_100();
        let runner = LoadTestRunner::new(cfg);
        let result = tokio::time::timeout(Duration::from_secs(30), runner.run_simulated())
            .await
            .expect("10k test must complete within 30 seconds");

        assert_eq!(result.connections_attempted, 10_000);
        assert_eq!(
            result.connections_succeeded + result.connections_failed,
            result.connections_attempted,
            "succeeded + failed must equal attempted"
        );
        assert!(result.all_succeeded(), "all 10k connections must succeed");
    }

    /// succeeded + failed == attempted (accounting invariant)
    #[tokio::test]
    async fn test_accounting_invariant() {
        let cfg = LoadTestConfig::default();
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        assert_eq!(
            result.connections_succeeded + result.connections_failed,
            result.connections_attempted
        );
    }

    /// When semaphore capacity equals target, all connections succeed.
    #[tokio::test]
    async fn test_uncapped_all_succeed() {
        let cfg = LoadTestConfig::uncapped(200);
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        assert_eq!(result.connections_failed, 0);
        assert_eq!(result.connections_succeeded, 200);
        assert!(result.all_succeeded());
    }

    /// Peak concurrent connections must never exceed semaphore capacity.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_peak_concurrent_bounded_by_semaphore() {
        let cap = 10usize;
        let cfg = LoadTestConfig {
            target_connections: 200,
            semaphore_capacity: cap,
            hold_ms: 1,
            ramp_up_ms: 0,
            ramp_down_ms: 0,
            ..Default::default()
        };
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        assert!(
            result.peak_concurrent <= cap,
            "peak {} exceeded semaphore cap {}",
            result.peak_concurrent,
            cap
        );
    }

    /// Zero-duration run is safe (no panics, all counters consistent).
    #[tokio::test]
    async fn test_zero_duration_is_safe() {
        let cfg = LoadTestConfig {
            target_connections: 50,
            hold_ms: 0,
            ramp_up_ms: 0,
            ramp_down_ms: 0,
            semaphore_capacity: 50,
            ..Default::default()
        };
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        assert_eq!(result.connections_attempted, 50);
        assert_eq!(result.connections_failed, 0);
    }

    /// One-connection smoke test.
    #[tokio::test]
    async fn test_single_connection_smoke() {
        let cfg = LoadTestConfig {
            target_connections: 1,
            semaphore_capacity: 1,
            hold_ms: 0,
            ..Default::default()
        };
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        assert_eq!(result.connections_attempted, 1);
        assert_eq!(result.connections_succeeded, 1);
        assert_eq!(result.connections_failed, 0);
    }

    /// Ramp-up delay causes total duration to be proportionally longer.
    #[tokio::test]
    async fn test_ramp_up_delay_reflected_in_timing() {
        // 100 connections, 100 ms ramp-up → each task delayed by ~1 ms
        let cfg = LoadTestConfig {
            target_connections: 100,
            ramp_up_ms: 100,
            hold_ms: 0,
            semaphore_capacity: 100,
            ..Default::default()
        };
        let runner = LoadTestRunner::new(cfg);
        let t0 = Instant::now();
        let result = runner.run_simulated().await;
        let elapsed = t0.elapsed();

        // The last task is delayed by ~100 ms total; run should take at least 50 ms
        assert!(
            elapsed >= Duration::from_millis(50),
            "ramp-up should cause measurable delay, got {:?}",
            elapsed
        );
        assert_eq!(result.connections_attempted, 100);
    }

    /// Messages sent equals connections succeeded (1 msg per connection model).
    #[tokio::test]
    async fn test_messages_sent_equals_connections_succeeded() {
        let cfg = LoadTestConfig::uncapped(300);
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        assert_eq!(result.messages_sent, result.connections_succeeded);
    }

    /// success_rate() clamps between 0.0 and 1.0.
    #[test]
    fn test_success_rate_never_exceeds_one() {
        for (att, succ) in [(0, 0), (1, 1), (100, 100), (100, 50), (1000, 999)] {
            let r = LoadTestResult {
                connections_attempted: att,
                connections_succeeded: succ,
                connections_failed: att.saturating_sub(succ),
                peak_concurrent: 0,
                messages_sent: succ,
                errors: vec![],
                duration: Duration::from_millis(1),
            };
            let rate = r.success_rate();
            assert!(
                (0.0..=1.0).contains(&rate),
                "rate={} for att={} succ={}",
                rate,
                att,
                succ
            );
        }
    }

    /// Throughput is zero when duration is zero.
    #[test]
    fn test_throughput_zero_when_duration_zero() {
        let r = LoadTestResult {
            connections_attempted: 0,
            connections_succeeded: 0,
            connections_failed: 0,
            peak_concurrent: 0,
            messages_sent: 0,
            errors: vec![],
            duration: Duration::ZERO,
        };
        assert_eq!(r.throughput(), 0.0);
    }

    /// Errors list grows when semaphore is poisoned (closed).
    /// We simulate this by using `fail_on_timeout = true` with 0 capacity.
    #[tokio::test]
    async fn test_errors_list_populated_on_failure() {
        let cfg = LoadTestConfig {
            target_connections: 5,
            semaphore_capacity: 1,
            hold_ms: 100, // hold long enough for others to time out
            fail_on_timeout: true,
            acquire_timeout: Duration::from_millis(1), // immediate timeout
            ..Default::default()
        };
        let runner = LoadTestRunner::new(cfg);
        let result = runner.run_simulated().await;
        // Some connections should have failed with timeout
        // (at least those blocked behind the single holder)
        assert!(
            result.connections_failed > 0 || !result.errors.is_empty(),
            "expected some failures with 1-cap sem, 1ms timeout, hold=100ms"
        );
    }
}
