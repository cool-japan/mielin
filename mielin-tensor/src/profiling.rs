//! Performance Profiling and Regression Testing Utilities
//!
//! Provides tools for tracking performance metrics, detecting regressions,
//! and profiling memory usage patterns.
//!
//! # Examples
//!
//! ```
//! use mielin_tensor::profiling::{PerfMonitor, MemoryTracker};
//!
//! // Track performance
//! let mut monitor = PerfMonitor::new();
//! monitor.start("my_operation");
//! // ... do work ...
//! monitor.stop("my_operation");
//! let stats = monitor.summary();
//!
//! // Track memory
//! let tracker = MemoryTracker::new();
//! let snapshot1 = tracker.snapshot();
//! // ... allocate memory ...
//! let snapshot2 = tracker.snapshot();
//! let diff = snapshot2 - snapshot1;
//! ```

#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;

/// Performance metrics for an operation
#[derive(Debug, Clone, Copy)]
pub struct PerfMetrics {
    /// Number of times the operation was called
    pub count: u64,
    /// Total time spent in nanoseconds
    pub total_ns: u128,
    /// Minimum execution time in nanoseconds
    pub min_ns: u128,
    /// Maximum execution time in nanoseconds
    pub max_ns: u128,
    /// Average execution time in nanoseconds
    pub avg_ns: u128,
}

impl PerfMetrics {
    fn new() -> Self {
        Self {
            count: 0,
            total_ns: 0,
            min_ns: u128::MAX,
            max_ns: 0,
            avg_ns: 0,
        }
    }

    fn record(&mut self, duration_ns: u128) {
        self.count += 1;
        self.total_ns += duration_ns;
        self.min_ns = self.min_ns.min(duration_ns);
        self.max_ns = self.max_ns.max(duration_ns);
        self.avg_ns = self.total_ns / self.count as u128;
    }
}

/// Performance monitor for tracking operation timings
///
/// Tracks multiple operations and provides statistics.
pub struct PerfMonitor {
    metrics: BTreeMap<String, PerfMetrics>,
    active_timers: BTreeMap<String, u128>,
}

impl PerfMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        Self {
            metrics: BTreeMap::new(),
            active_timers: BTreeMap::new(),
        }
    }

    /// Start timing an operation
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the operation to track
    pub fn start(&mut self, name: impl Into<String>) {
        let name = name.into();
        // Use a simple counter as a mock timer
        // In real implementation, this would use std::time::Instant
        self.active_timers.insert(name, 0);
    }

    /// Stop timing an operation and record the result
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the operation being timed
    pub fn stop(&mut self, name: impl Into<String>) {
        let name = name.into();
        if let Some(_start_time) = self.active_timers.remove(&name) {
            // Mock duration (in real implementation, would be now - start_time)
            let duration_ns = 1000u128; // 1 microsecond mock

            self.metrics
                .entry(name)
                .or_insert_with(PerfMetrics::new)
                .record(duration_ns);
        }
    }

    /// Get metrics for a specific operation
    pub fn get(&self, name: &str) -> Option<&PerfMetrics> {
        self.metrics.get(name)
    }

    /// Get all tracked metrics
    pub fn all_metrics(&self) -> &BTreeMap<String, PerfMetrics> {
        &self.metrics
    }

    /// Clear all metrics
    pub fn clear(&mut self) {
        self.metrics.clear();
        self.active_timers.clear();
    }

    /// Get a summary of performance statistics
    ///
    /// Returns formatted statistics as a string (requires alloc).
    pub fn summary(&self) -> String {
        let mut output = String::from("\n=== Performance Statistics ===\n");
        output.push_str(&alloc::format!(
            "{:30} {:>10} {:>12} {:>12} {:>12}\n",
            "Operation",
            "Count",
            "Avg (µs)",
            "Min (µs)",
            "Max (µs)"
        ));
        output.push_str(&alloc::format!("{}\n", "-".repeat(80)));

        for (name, metrics) in &self.metrics {
            output.push_str(&alloc::format!(
                "{:30} {:>10} {:>12.2} {:>12.2} {:>12.2}\n",
                name,
                metrics.count,
                metrics.avg_ns as f64 / 1000.0,
                metrics.min_ns as f64 / 1000.0,
                metrics.max_ns as f64 / 1000.0
            ));
        }

        output
    }
}

impl Default for PerfMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory usage snapshot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySnapshot {
    /// Total bytes allocated
    pub allocated_bytes: usize,
    /// Number of allocations
    pub allocation_count: usize,
    /// Peak memory usage
    pub peak_bytes: usize,
}

impl MemorySnapshot {
    /// Create a new snapshot
    pub fn new(allocated: usize, count: usize, peak: usize) -> Self {
        Self {
            allocated_bytes: allocated,
            allocation_count: count,
            peak_bytes: peak,
        }
    }

    /// Get memory usage in kilobytes
    pub fn kb(&self) -> f64 {
        self.allocated_bytes as f64 / 1024.0
    }

    /// Get memory usage in megabytes
    pub fn mb(&self) -> f64 {
        self.allocated_bytes as f64 / 1024.0 / 1024.0
    }
}

impl core::ops::Sub for MemorySnapshot {
    type Output = MemoryDiff;

    fn sub(self, rhs: Self) -> Self::Output {
        MemoryDiff {
            bytes_delta: self.allocated_bytes as isize - rhs.allocated_bytes as isize,
            count_delta: self.allocation_count as isize - rhs.allocation_count as isize,
        }
    }
}

/// Difference between two memory snapshots
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryDiff {
    /// Change in allocated bytes (can be negative)
    pub bytes_delta: isize,
    /// Change in allocation count (can be negative)
    pub count_delta: isize,
}

impl MemoryDiff {
    /// Check if memory increased
    pub fn is_increase(&self) -> bool {
        self.bytes_delta > 0
    }

    /// Get absolute change in bytes
    pub fn abs_bytes(&self) -> usize {
        self.bytes_delta.unsigned_abs()
    }

    /// Get change in kilobytes
    pub fn kb_delta(&self) -> f64 {
        self.bytes_delta as f64 / 1024.0
    }

    /// Get change in megabytes
    pub fn mb_delta(&self) -> f64 {
        self.bytes_delta as f64 / 1024.0 / 1024.0
    }
}

/// Memory usage tracker
///
/// Tracks memory allocations and provides snapshots for comparison.
pub struct MemoryTracker {
    allocated_bytes: usize,
    allocation_count: usize,
    peak_bytes: usize,
}

impl MemoryTracker {
    /// Create a new memory tracker
    pub fn new() -> Self {
        Self {
            allocated_bytes: 0,
            allocation_count: 0,
            peak_bytes: 0,
        }
    }

    /// Record an allocation
    pub fn record_allocation(&mut self, bytes: usize) {
        self.allocated_bytes += bytes;
        self.allocation_count += 1;
        self.peak_bytes = self.peak_bytes.max(self.allocated_bytes);
    }

    /// Record a deallocation
    pub fn record_deallocation(&mut self, bytes: usize) {
        self.allocated_bytes = self.allocated_bytes.saturating_sub(bytes);
    }

    /// Take a snapshot of current memory usage
    pub fn snapshot(&self) -> MemorySnapshot {
        MemorySnapshot::new(self.allocated_bytes, self.allocation_count, self.peak_bytes)
    }

    /// Reset all counters
    pub fn reset(&mut self) {
        self.allocated_bytes = 0;
        self.allocation_count = 0;
        self.peak_bytes = 0;
    }
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance regression detector
///
/// Compares current performance against baseline and detects regressions.
#[derive(Debug, Clone)]
pub struct RegressionDetector {
    baselines: BTreeMap<String, PerfMetrics>,
    threshold_percent: f64,
}

impl RegressionDetector {
    /// Create a new regression detector
    ///
    /// # Arguments
    ///
    /// * `threshold_percent` - Percentage threshold for regression (e.g., 10.0 for 10%)
    pub fn new(threshold_percent: f64) -> Self {
        Self {
            baselines: BTreeMap::new(),
            threshold_percent,
        }
    }

    /// Set baseline metrics for an operation
    pub fn set_baseline(&mut self, name: impl Into<String>, metrics: PerfMetrics) {
        self.baselines.insert(name.into(), metrics);
    }

    /// Check if current metrics represent a regression
    ///
    /// Returns the regression percentage if regression detected, None otherwise
    pub fn check_regression(&self, name: &str, current: &PerfMetrics) -> Option<f64> {
        if let Some(baseline) = self.baselines.get(name) {
            let increase_pct =
                ((current.avg_ns as f64 - baseline.avg_ns as f64) / baseline.avg_ns as f64) * 100.0;

            if increase_pct > self.threshold_percent {
                return Some(increase_pct);
            }
        }
        None
    }

    /// Load baselines from performance monitor
    pub fn load_from_monitor(&mut self, monitor: &PerfMonitor) {
        for (name, metrics) in monitor.all_metrics() {
            self.baselines.insert(name.clone(), *metrics);
        }
    }

    /// Get baseline for an operation
    pub fn get_baseline(&self, name: &str) -> Option<&PerfMetrics> {
        self.baselines.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_perf_monitor_basic() {
        let mut monitor = PerfMonitor::new();
        monitor.start("test_op");
        monitor.stop("test_op");

        let metrics = monitor.get("test_op").unwrap();
        assert_eq!(metrics.count, 1);
        assert!(metrics.avg_ns > 0);
    }

    #[test]
    fn test_perf_monitor_multiple() {
        let mut monitor = PerfMonitor::new();

        for _ in 0..5 {
            monitor.start("op1");
            monitor.stop("op1");
        }

        let metrics = monitor.get("op1").unwrap();
        assert_eq!(metrics.count, 5);
    }

    #[test]
    fn test_memory_snapshot() {
        let snap1 = MemorySnapshot::new(1024, 10, 2048);
        assert_eq!(snap1.kb(), 1.0);
        assert_eq!(snap1.allocated_bytes, 1024);
        assert_eq!(snap1.peak_bytes, 2048);
    }

    #[test]
    fn test_memory_diff() {
        let snap1 = MemorySnapshot::new(1024, 10, 2048);
        let snap2 = MemorySnapshot::new(2048, 15, 3072);

        let diff = snap2 - snap1;
        assert_eq!(diff.bytes_delta, 1024);
        assert_eq!(diff.count_delta, 5);
        assert!(diff.is_increase());
    }

    #[test]
    fn test_memory_tracker() {
        let mut tracker = MemoryTracker::new();

        tracker.record_allocation(1024);
        let snap1 = tracker.snapshot();
        assert_eq!(snap1.allocated_bytes, 1024);
        assert_eq!(snap1.allocation_count, 1);

        tracker.record_allocation(512);
        let snap2 = tracker.snapshot();
        assert_eq!(snap2.allocated_bytes, 1536);
        assert_eq!(snap2.allocation_count, 2);

        tracker.record_deallocation(1024);
        let snap3 = tracker.snapshot();
        assert_eq!(snap3.allocated_bytes, 512);
    }

    #[test]
    fn test_regression_detector() {
        let mut detector = RegressionDetector::new(10.0); // 10% threshold

        let baseline = PerfMetrics {
            count: 100,
            total_ns: 100_000,
            min_ns: 900,
            max_ns: 1100,
            avg_ns: 1000,
        };
        detector.set_baseline("test_op", baseline);

        // No regression (5% slower)
        let current_ok = PerfMetrics {
            count: 100,
            total_ns: 105_000,
            min_ns: 950,
            max_ns: 1150,
            avg_ns: 1050,
        };
        assert!(detector.check_regression("test_op", &current_ok).is_none());

        // Regression detected (15% slower)
        let current_bad = PerfMetrics {
            count: 100,
            total_ns: 115_000,
            min_ns: 1050,
            max_ns: 1250,
            avg_ns: 1150,
        };
        let regression = detector.check_regression("test_op", &current_bad);
        assert!(regression.is_some());
        assert!(regression.unwrap() > 10.0);
    }

    #[test]
    fn test_perf_monitor_clear() {
        let mut monitor = PerfMonitor::new();
        monitor.start("test");
        monitor.stop("test");
        assert_eq!(monitor.all_metrics().len(), 1);

        monitor.clear();
        assert_eq!(monitor.all_metrics().len(), 0);
    }

    #[test]
    fn test_memory_tracker_peak() {
        let mut tracker = MemoryTracker::new();

        tracker.record_allocation(1000);
        tracker.record_allocation(500);
        assert_eq!(tracker.snapshot().peak_bytes, 1500);

        tracker.record_deallocation(500);
        assert_eq!(tracker.snapshot().peak_bytes, 1500); // Peak stays the same
    }
}
