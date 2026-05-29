//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// CPU time quota configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CpuQuota {
    /// Maximum CPU time per execution in microseconds
    pub max_execution_time_us: u64,
    /// Maximum CPU time per second (rate limiting)
    pub max_time_per_second_us: u64,
    /// Maximum instructions per execution (if supported)
    pub max_instructions: u64,
    /// Priority level (0-100, higher = more CPU time)
    pub priority: u8,
}
impl CpuQuota {
    /// Create an unlimited CPU quota
    pub fn unlimited() -> Self {
        Self {
            max_execution_time_us: u64::MAX,
            max_time_per_second_us: 1_000_000,
            max_instructions: u64::MAX,
            priority: 50,
        }
    }
    /// Create a CPU quota with specified limits
    pub fn with_limits(execution_time_us: u64, time_per_second_us: u64) -> Self {
        Self {
            max_execution_time_us: execution_time_us,
            max_time_per_second_us: time_per_second_us,
            max_instructions: u64::MAX,
            priority: 50,
        }
    }
    /// Set the priority level
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(100);
        self
    }
    /// Get max execution time as Duration
    pub fn max_execution_duration(&self) -> Duration {
        Duration::from_micros(self.max_execution_time_us)
    }
    /// Get CPU utilization limit as percentage (0-100)
    pub fn utilization_percent(&self) -> u8 {
        ((self.max_time_per_second_us as f64 / 1_000_000.0) * 100.0).min(100.0) as u8
    }
}
/// Complete resource monitor for an agent
#[derive(Debug)]
pub struct ResourceMonitor {
    /// Agent ID
    agent_id: [u8; 16],
    /// Resource history
    history: ResourceHistory,
    /// Anomaly detector
    detector: AnomalyDetector,
    /// Resource predictor
    predictor: ResourcePredictor,
}
impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new(agent_id: [u8; 16]) -> Self {
        Self {
            agent_id,
            history: ResourceHistory::new(agent_id, HistoryConfig::default()),
            detector: AnomalyDetector::new(AnomalyConfig::default()),
            predictor: ResourcePredictor::new(),
        }
    }
    /// Create with custom configurations
    pub fn with_config(
        agent_id: [u8; 16],
        history_config: HistoryConfig,
        anomaly_config: AnomalyConfig,
    ) -> Self {
        Self {
            agent_id,
            history: ResourceHistory::new(agent_id, history_config),
            detector: AnomalyDetector::new(anomaly_config),
            predictor: ResourcePredictor::new(),
        }
    }
    /// Get the agent ID
    pub fn agent_id(&self) -> &[u8; 16] {
        &self.agent_id
    }
    /// Get the history tracker
    pub fn history(&self) -> &ResourceHistory {
        &self.history
    }
    /// Get the anomaly detector
    pub fn detector(&self) -> &AnomalyDetector {
        &self.detector
    }
    /// Get the predictor
    pub fn predictor(&self) -> &ResourcePredictor {
        &self.predictor
    }
    /// Record current usage and check for anomalies
    pub fn record_and_analyze(
        &mut self,
        usage: &ResourceUsage,
        current_time_us: u64,
    ) -> Vec<Anomaly> {
        self.history.record(usage, current_time_us);
        self.detector.analyze(&self.history, current_time_us)
    }
    /// Record if enough time has passed and check for anomalies
    pub fn maybe_record_and_analyze(
        &mut self,
        usage: &ResourceUsage,
        current_time_us: u64,
    ) -> Vec<Anomaly> {
        if self.history.maybe_record(usage, current_time_us) {
            self.detector.analyze(&self.history, current_time_us)
        } else {
            Vec::new()
        }
    }
    /// Get a prediction for future resource usage
    pub fn predict(&self, horizon_us: u64) -> Option<ResourcePrediction> {
        self.predictor.predict(&self.history, horizon_us)
    }
    /// Predict when quota will be breached
    pub fn predict_quota_breach(&self, quota: &ResourceQuota) -> Option<u64> {
        self.predictor.predict_quota_breach(&self.history, quota)
    }
    /// Get summary statistics
    pub fn summary(&self) -> MonitorSummary {
        let peak_memory = self.history.peak_memory();
        let peak_cpu = self.history.peak_cpu();
        let avg = self.history.average();
        let latest = self.history.latest().cloned();
        MonitorSummary {
            agent_id: self.agent_id,
            snapshot_count: self.history.len(),
            peak_memory_bytes: peak_memory,
            peak_cpu_time_us: peak_cpu,
            average_memory_bytes: avg.as_ref().map(|a| a.memory_bytes).unwrap_or(0),
            average_cpu_time_us: avg.as_ref().map(|a| a.cpu_time_us).unwrap_or(0),
            latest_snapshot: latest,
            anomaly_count: self.detector.anomalies().len(),
            high_severity_anomalies: self.detector.high_severity_anomalies(0.7).len(),
        }
    }
    /// Clear all history and anomalies
    pub fn clear(&mut self) {
        self.history.clear();
        self.detector.clear();
    }
}
/// Enforces resource quotas for agents
#[derive(Debug)]
pub struct QuotaEnforcer {
    /// Resource quota
    quota: ResourceQuota,
    /// Current resource usage
    usage: ResourceUsage,
    /// Last second reset time
    last_second_reset: Instant,
    /// Violation callback count
    violation_count: u64,
}
impl QuotaEnforcer {
    /// Create a new quota enforcer
    pub fn new(quota: ResourceQuota) -> Self {
        Self {
            quota,
            usage: ResourceUsage::default(),
            last_second_reset: Instant::now(),
            violation_count: 0,
        }
    }
    /// Get the current quota
    pub fn quota(&self) -> &ResourceQuota {
        &self.quota
    }
    /// Update the quota
    pub fn set_quota(&mut self, quota: ResourceQuota) {
        self.quota = quota;
    }
    /// Get current resource usage
    pub fn usage(&self) -> &ResourceUsage {
        &self.usage
    }
    /// Reset per-second counters if a second has elapsed
    fn maybe_reset_second_counters(&mut self) {
        if self.last_second_reset.elapsed() >= Duration::from_secs(1) {
            self.usage.cpu_time_this_second_us = 0;
            self.usage.bytes_sent_this_second = 0;
            self.usage.bytes_received_this_second = 0;
            self.usage.messages_this_second = 0;
            self.last_second_reset = Instant::now();
        }
    }
    /// Check if current usage is within quota
    pub fn check(&mut self) -> QuotaCheckResult {
        self.maybe_reset_second_counters();
        let mut result = QuotaCheckResult::ok();
        if !self.quota.enforced {
            return result;
        }
        if self.usage.heap_bytes > self.quota.memory.max_heap_bytes {
            result = result.with_violation(QuotaViolation::HeapMemoryExceeded);
        }
        if self.usage.stack_bytes > self.quota.memory.max_stack_bytes {
            result = result.with_violation(QuotaViolation::StackMemoryExceeded);
        }
        if self.usage.total_memory_bytes > self.quota.memory.max_total_bytes {
            result = result.with_violation(QuotaViolation::TotalMemoryExceeded);
        }
        if self.usage.total_memory_bytes > self.quota.memory.total_soft_limit() {
            result = result.with_violation(QuotaViolation::MemorySoftLimitReached);
        }
        if self.usage.cpu_time_this_second_us > self.quota.cpu.max_time_per_second_us {
            result = result.with_violation(QuotaViolation::CpuRateLimitExceeded);
        }
        if self.usage.instructions_executed > self.quota.cpu.max_instructions {
            result = result.with_violation(QuotaViolation::InstructionLimitExceeded);
        }
        if self.usage.bytes_sent_this_second > self.quota.network.max_send_bytes_per_sec {
            result = result.with_violation(QuotaViolation::SendBandwidthExceeded);
        }
        if self.usage.bytes_received_this_second > self.quota.network.max_recv_bytes_per_sec {
            result = result.with_violation(QuotaViolation::RecvBandwidthExceeded);
        }
        if self.usage.network_bytes_this_second() > self.quota.network.max_total_bytes_per_sec {
            result = result.with_violation(QuotaViolation::TotalBandwidthExceeded);
        }
        if self.usage.active_connections > self.quota.network.max_connections {
            result = result.with_violation(QuotaViolation::ConnectionLimitExceeded);
        }
        if self.usage.messages_this_second > self.quota.network.max_messages_per_sec {
            result = result.with_violation(QuotaViolation::MessageRateExceeded);
        }
        if self.usage.persistent_storage_bytes > self.quota.storage.max_persistent_bytes {
            result = result.with_violation(QuotaViolation::PersistentStorageExceeded);
        }
        if self.usage.temp_storage_bytes > self.quota.storage.max_temp_bytes {
            result = result.with_violation(QuotaViolation::TempStorageExceeded);
        }
        if self.usage.file_count > self.quota.storage.max_files {
            result = result.with_violation(QuotaViolation::FileCountExceeded);
        }
        if !result.within_limits {
            self.violation_count += 1;
        }
        result
    }
    /// Check if a memory allocation can be made
    pub fn can_allocate_memory(&self, bytes: u64) -> bool {
        if !self.quota.enforced {
            return true;
        }
        self.usage.heap_bytes + bytes <= self.quota.memory.max_heap_bytes
            && self.usage.total_memory_bytes + bytes <= self.quota.memory.max_total_bytes
    }
    /// Record a memory allocation
    pub fn record_allocation(&mut self, bytes: u64) {
        self.usage.heap_bytes += bytes;
        self.usage.total_memory_bytes += bytes;
    }
    /// Record a memory deallocation
    pub fn record_deallocation(&mut self, bytes: u64) {
        self.usage.heap_bytes = self.usage.heap_bytes.saturating_sub(bytes);
        self.usage.total_memory_bytes = self.usage.total_memory_bytes.saturating_sub(bytes);
    }
    /// Check if CPU time can be used
    pub fn can_use_cpu(&mut self, time_us: u64) -> bool {
        self.maybe_reset_second_counters();
        if !self.quota.enforced {
            return true;
        }
        self.usage.cpu_time_this_second_us + time_us <= self.quota.cpu.max_time_per_second_us
    }
    /// Record CPU time usage
    pub fn record_cpu_time(&mut self, time_us: u64) {
        self.maybe_reset_second_counters();
        self.usage.cpu_time_this_second_us += time_us;
        self.usage.total_cpu_time_us += time_us;
    }
    /// Check if bytes can be sent
    pub fn can_send(&mut self, bytes: u64) -> bool {
        self.maybe_reset_second_counters();
        if !self.quota.enforced {
            return true;
        }
        self.usage.bytes_sent_this_second + bytes <= self.quota.network.max_send_bytes_per_sec
    }
    /// Record bytes sent
    pub fn record_send(&mut self, bytes: u64) {
        self.maybe_reset_second_counters();
        self.usage.bytes_sent_this_second += bytes;
        self.usage.total_bytes_sent += bytes;
    }
    /// Check if bytes can be received
    pub fn can_receive(&mut self, bytes: u64) -> bool {
        self.maybe_reset_second_counters();
        if !self.quota.enforced {
            return true;
        }
        self.usage.bytes_received_this_second + bytes <= self.quota.network.max_recv_bytes_per_sec
    }
    /// Record bytes received
    pub fn record_receive(&mut self, bytes: u64) {
        self.maybe_reset_second_counters();
        self.usage.bytes_received_this_second += bytes;
        self.usage.total_bytes_received += bytes;
    }
    /// Check if a message can be sent
    pub fn can_send_message(&mut self) -> bool {
        self.maybe_reset_second_counters();
        if !self.quota.enforced {
            return true;
        }
        self.usage.messages_this_second < self.quota.network.max_messages_per_sec
    }
    /// Record a message sent
    pub fn record_message(&mut self) {
        self.maybe_reset_second_counters();
        self.usage.messages_this_second += 1;
    }
    /// Check if a connection can be opened
    pub fn can_open_connection(&self) -> bool {
        if !self.quota.enforced {
            return true;
        }
        self.usage.active_connections < self.quota.network.max_connections
    }
    /// Record a connection opened
    pub fn record_connection_open(&mut self) {
        self.usage.active_connections += 1;
    }
    /// Record a connection closed
    pub fn record_connection_close(&mut self) {
        self.usage.active_connections = self.usage.active_connections.saturating_sub(1);
    }
    /// Check if storage can be used
    pub fn can_use_storage(&self, bytes: u64, persistent: bool) -> bool {
        if !self.quota.enforced {
            return true;
        }
        if persistent {
            self.usage.persistent_storage_bytes + bytes <= self.quota.storage.max_persistent_bytes
        } else {
            self.usage.temp_storage_bytes + bytes <= self.quota.storage.max_temp_bytes
        }
    }
    /// Record storage usage
    pub fn record_storage(&mut self, bytes: u64, persistent: bool) {
        if persistent {
            self.usage.persistent_storage_bytes += bytes;
        } else {
            self.usage.temp_storage_bytes += bytes;
        }
    }
    /// Record storage freed
    pub fn record_storage_freed(&mut self, bytes: u64, persistent: bool) {
        if persistent {
            self.usage.persistent_storage_bytes =
                self.usage.persistent_storage_bytes.saturating_sub(bytes);
        } else {
            self.usage.temp_storage_bytes = self.usage.temp_storage_bytes.saturating_sub(bytes);
        }
    }
    /// Check if a file can be created
    pub fn can_create_file(&self, size: u64) -> bool {
        if !self.quota.enforced {
            return true;
        }
        self.usage.file_count < self.quota.storage.max_files
            && size <= self.quota.storage.max_file_size
    }
    /// Record a file created
    pub fn record_file_created(&mut self) {
        self.usage.file_count += 1;
    }
    /// Record a file deleted
    pub fn record_file_deleted(&mut self) {
        self.usage.file_count = self.usage.file_count.saturating_sub(1);
    }
    /// Get the total violation count
    pub fn violation_count(&self) -> u64 {
        self.violation_count
    }
    /// Reset all usage counters
    pub fn reset_usage(&mut self) {
        self.usage = ResourceUsage::default();
        self.last_second_reset = Instant::now();
    }
}
/// A snapshot of resource usage at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    /// Timestamp in microseconds since UNIX epoch
    pub timestamp_us: u64,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU time used in microseconds
    pub cpu_time_us: u64,
    /// Network bytes sent
    pub network_sent_bytes: u64,
    /// Network bytes received
    pub network_recv_bytes: u64,
    /// Storage bytes used
    pub storage_bytes: u64,
}
impl ResourceSnapshot {
    /// Create a new snapshot from current usage
    pub fn from_usage(usage: &ResourceUsage, timestamp_us: u64) -> Self {
        Self {
            timestamp_us,
            memory_bytes: usage.total_memory_bytes,
            cpu_time_us: usage.total_cpu_time_us,
            network_sent_bytes: usage.total_bytes_sent,
            network_recv_bytes: usage.total_bytes_received,
            storage_bytes: usage.total_storage_bytes(),
        }
    }
    /// Calculate the rate of change compared to another snapshot
    pub fn rate_from(&self, other: &ResourceSnapshot) -> Option<ResourceRate> {
        let time_delta_us = self.timestamp_us.checked_sub(other.timestamp_us)?;
        if time_delta_us == 0 {
            return None;
        }
        let time_delta_secs = time_delta_us as f64 / 1_000_000.0;
        Some(ResourceRate {
            memory_bytes_per_sec: (self.memory_bytes as f64 - other.memory_bytes as f64)
                / time_delta_secs,
            cpu_percent: ((self.cpu_time_us - other.cpu_time_us) as f64 / time_delta_us as f64)
                * 100.0,
            network_sent_bytes_per_sec: (self.network_sent_bytes - other.network_sent_bytes) as f64
                / time_delta_secs,
            network_recv_bytes_per_sec: (self.network_recv_bytes - other.network_recv_bytes) as f64
                / time_delta_secs,
            storage_bytes_per_sec: (self.storage_bytes as f64 - other.storage_bytes as f64)
                / time_delta_secs,
        })
    }
}
/// Configuration for resource history tracking
#[derive(Debug, Clone, Copy)]
pub struct HistoryConfig {
    /// Maximum number of snapshots to keep
    pub max_snapshots: usize,
    /// Minimum interval between snapshots in microseconds
    pub min_interval_us: u64,
}
impl HistoryConfig {
    /// Create a high-resolution config (more frequent snapshots)
    pub fn high_resolution() -> Self {
        Self {
            max_snapshots: 10000,
            min_interval_us: 100_000,
        }
    }
    /// Create a low-resolution config (less frequent snapshots)
    pub fn low_resolution() -> Self {
        Self {
            max_snapshots: 100,
            min_interval_us: 60_000_000,
        }
    }
}
/// Memory quota configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MemoryQuota {
    /// Maximum heap memory in bytes
    pub max_heap_bytes: u64,
    /// Maximum stack size in bytes
    pub max_stack_bytes: u64,
    /// Maximum total memory in bytes
    pub max_total_bytes: u64,
    /// Soft limit (warning threshold) as percentage of max
    pub soft_limit_percent: u8,
}
impl MemoryQuota {
    /// Create an unlimited memory quota
    pub fn unlimited() -> Self {
        Self {
            max_heap_bytes: u64::MAX,
            max_stack_bytes: u64::MAX,
            max_total_bytes: u64::MAX,
            soft_limit_percent: 100,
        }
    }
    /// Create a memory quota with specified limits
    pub fn with_limits(heap: u64, stack: u64, total: u64) -> Self {
        Self {
            max_heap_bytes: heap,
            max_stack_bytes: stack,
            max_total_bytes: total,
            soft_limit_percent: 80,
        }
    }
    /// Get the soft limit for heap memory
    pub fn heap_soft_limit(&self) -> u64 {
        self.max_heap_bytes * u64::from(self.soft_limit_percent) / 100
    }
    /// Get the soft limit for total memory
    pub fn total_soft_limit(&self) -> u64 {
        self.max_total_bytes * u64::from(self.soft_limit_percent) / 100
    }
}
/// Network bandwidth quota configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NetworkQuota {
    /// Maximum bytes sent per second
    pub max_send_bytes_per_sec: u64,
    /// Maximum bytes received per second
    pub max_recv_bytes_per_sec: u64,
    /// Maximum total bytes per second
    pub max_total_bytes_per_sec: u64,
    /// Maximum concurrent connections
    pub max_connections: u32,
    /// Maximum messages per second
    pub max_messages_per_sec: u32,
}
impl NetworkQuota {
    /// Create an unlimited network quota
    pub fn unlimited() -> Self {
        Self {
            max_send_bytes_per_sec: u64::MAX,
            max_recv_bytes_per_sec: u64::MAX,
            max_total_bytes_per_sec: u64::MAX,
            max_connections: u32::MAX,
            max_messages_per_sec: u32::MAX,
        }
    }
    /// Create a network quota with specified bandwidth limits
    pub fn with_bandwidth(send: u64, recv: u64) -> Self {
        Self {
            max_send_bytes_per_sec: send,
            max_recv_bytes_per_sec: recv,
            max_total_bytes_per_sec: send + recv,
            max_connections: 100,
            max_messages_per_sec: 1000,
        }
    }
    /// Set connection limits
    pub fn with_connection_limit(mut self, max_connections: u32) -> Self {
        self.max_connections = max_connections;
        self
    }
}
/// Thread-safe atomic counter for resource tracking
#[derive(Debug, Default)]
pub struct AtomicResourceCounter {
    value: AtomicU64,
}
impl AtomicResourceCounter {
    /// Create a new counter with initial value
    pub fn new(initial: u64) -> Self {
        Self {
            value: AtomicU64::new(initial),
        }
    }
    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
    /// Add to the counter
    pub fn add(&self, amount: u64) {
        self.value.fetch_add(amount, Ordering::Relaxed);
    }
    /// Subtract from the counter (saturating)
    pub fn sub(&self, amount: u64) {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let new = current.saturating_sub(amount);
            match self.value.compare_exchange_weak(
                current,
                new,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current = x,
            }
        }
    }
    /// Reset to zero
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}
/// Result of checking resource usage against quotas
#[derive(Debug, Clone)]
pub struct QuotaCheckResult {
    /// Whether all hard limits are satisfied
    pub within_limits: bool,
    /// List of violations (if any)
    pub violations: Vec<QuotaViolation>,
    /// List of warnings (soft limit violations)
    pub warnings: Vec<QuotaViolation>,
}
impl QuotaCheckResult {
    /// Create a result indicating all limits are satisfied
    pub fn ok() -> Self {
        Self {
            within_limits: true,
            violations: Vec::new(),
            warnings: Vec::new(),
        }
    }
    /// Add a violation
    pub fn with_violation(mut self, violation: QuotaViolation) -> Self {
        if violation.is_warning() {
            self.warnings.push(violation);
        } else {
            self.within_limits = false;
            self.violations.push(violation);
        }
        self
    }
    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}
/// Tracks resource usage history for an agent
#[derive(Debug)]
pub struct ResourceHistory {
    /// Agent ID
    agent_id: [u8; 16],
    /// Configuration
    config: HistoryConfig,
    /// Historical snapshots (oldest first)
    snapshots: Vec<ResourceSnapshot>,
    /// Timestamp of last snapshot
    last_snapshot_us: u64,
}
impl ResourceHistory {
    /// Create a new resource history tracker
    pub fn new(agent_id: [u8; 16], config: HistoryConfig) -> Self {
        Self {
            agent_id,
            config,
            snapshots: Vec::new(),
            last_snapshot_us: 0,
        }
    }
    /// Get the agent ID
    pub fn agent_id(&self) -> &[u8; 16] {
        &self.agent_id
    }
    /// Get the number of snapshots
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }
    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
    /// Record a snapshot if enough time has passed
    pub fn maybe_record(&mut self, usage: &ResourceUsage, current_time_us: u64) -> bool {
        if current_time_us.saturating_sub(self.last_snapshot_us) < self.config.min_interval_us {
            return false;
        }
        let snapshot = ResourceSnapshot::from_usage(usage, current_time_us);
        self.snapshots.push(snapshot);
        self.last_snapshot_us = current_time_us;
        while self.snapshots.len() > self.config.max_snapshots {
            self.snapshots.remove(0);
        }
        true
    }
    /// Force record a snapshot
    pub fn record(&mut self, usage: &ResourceUsage, current_time_us: u64) {
        let snapshot = ResourceSnapshot::from_usage(usage, current_time_us);
        self.snapshots.push(snapshot);
        self.last_snapshot_us = current_time_us;
        while self.snapshots.len() > self.config.max_snapshots {
            self.snapshots.remove(0);
        }
    }
    /// Get the latest snapshot
    pub fn latest(&self) -> Option<&ResourceSnapshot> {
        self.snapshots.last()
    }
    /// Get all snapshots
    pub fn snapshots(&self) -> &[ResourceSnapshot] {
        &self.snapshots
    }
    /// Get snapshots in a time range
    pub fn snapshots_in_range(&self, start_us: u64, end_us: u64) -> Vec<&ResourceSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.timestamp_us >= start_us && s.timestamp_us <= end_us)
            .collect()
    }
    /// Calculate average resource usage over the history
    pub fn average(&self) -> Option<ResourceSnapshot> {
        if self.snapshots.is_empty() {
            return None;
        }
        let count = self.snapshots.len() as u64;
        let sum_memory: u64 = self.snapshots.iter().map(|s| s.memory_bytes).sum();
        let sum_cpu: u64 = self.snapshots.iter().map(|s| s.cpu_time_us).sum();
        let sum_net_sent: u64 = self.snapshots.iter().map(|s| s.network_sent_bytes).sum();
        let sum_net_recv: u64 = self.snapshots.iter().map(|s| s.network_recv_bytes).sum();
        let sum_storage: u64 = self.snapshots.iter().map(|s| s.storage_bytes).sum();
        Some(ResourceSnapshot {
            timestamp_us: self.last_snapshot_us,
            memory_bytes: sum_memory / count,
            cpu_time_us: sum_cpu / count,
            network_sent_bytes: sum_net_sent / count,
            network_recv_bytes: sum_net_recv / count,
            storage_bytes: sum_storage / count,
        })
    }
    /// Get the peak memory usage
    pub fn peak_memory(&self) -> u64 {
        self.snapshots
            .iter()
            .map(|s| s.memory_bytes)
            .max()
            .unwrap_or(0)
    }
    /// Get the peak CPU time
    pub fn peak_cpu(&self) -> u64 {
        self.snapshots
            .iter()
            .map(|s| s.cpu_time_us)
            .max()
            .unwrap_or(0)
    }
    /// Clear all history
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.last_snapshot_us = 0;
    }
}
/// A detected anomaly
#[derive(Debug, Clone)]
pub struct Anomaly {
    /// Type of anomaly
    pub anomaly_type: AnomalyType,
    /// Timestamp when detected (microseconds)
    pub detected_at_us: u64,
    /// Severity (0.0-1.0)
    pub severity: f64,
    /// Current value
    pub current_value: f64,
    /// Expected value (baseline)
    pub expected_value: f64,
    /// Description
    pub description: String,
}
/// Configuration for anomaly detection
#[derive(Debug, Clone, Copy)]
pub struct AnomalyConfig {
    /// Threshold for memory spike (multiplier of average)
    pub memory_spike_threshold: f64,
    /// Threshold for CPU spike (multiplier of average)
    pub cpu_spike_threshold: f64,
    /// Threshold for network spike (multiplier of average)
    pub network_spike_threshold: f64,
    /// Threshold for storage spike (multiplier of average)
    pub storage_spike_threshold: f64,
    /// Minimum samples needed for leak detection
    pub leak_detection_samples: usize,
    /// Threshold for leak detection (consecutive growth percentage)
    pub leak_growth_threshold: f64,
}
impl AnomalyConfig {
    /// Create a strict config (more sensitive)
    pub fn strict() -> Self {
        Self {
            memory_spike_threshold: 1.5,
            cpu_spike_threshold: 2.0,
            network_spike_threshold: 3.0,
            storage_spike_threshold: 1.5,
            leak_detection_samples: 5,
            leak_growth_threshold: 0.8,
        }
    }
    /// Create a lenient config (less sensitive)
    pub fn lenient() -> Self {
        Self {
            memory_spike_threshold: 3.0,
            cpu_spike_threshold: 5.0,
            network_spike_threshold: 10.0,
            storage_spike_threshold: 3.0,
            leak_detection_samples: 20,
            leak_growth_threshold: 0.98,
        }
    }
}
/// Manages resource quotas for multiple agents
#[derive(Debug)]
pub struct ResourceManager {
    /// Enforcers per agent
    enforcers: HashMap<[u8; 16], QuotaEnforcer>,
    /// Default quota for new agents
    default_quota: ResourceQuota,
    /// Global resource limits
    global_memory_limit: u64,
    /// Current total memory usage
    total_memory_used: AtomicResourceCounter,
}
impl ResourceManager {
    /// Create a new resource manager
    pub fn new() -> Self {
        Self {
            enforcers: HashMap::new(),
            default_quota: ResourceQuota::default(),
            global_memory_limit: 64 * 1024 * 1024 * 1024,
            total_memory_used: AtomicResourceCounter::new(0),
        }
    }
    /// Set the default quota for new agents
    pub fn set_default_quota(&mut self, quota: ResourceQuota) {
        self.default_quota = quota;
    }
    /// Set the global memory limit
    pub fn set_global_memory_limit(&mut self, limit: u64) {
        self.global_memory_limit = limit;
    }
    /// Register an agent with default quota
    pub fn register_agent(&mut self, agent_id: [u8; 16]) {
        self.register_agent_with_quota(agent_id, self.default_quota.clone());
    }
    /// Register an agent with custom quota
    pub fn register_agent_with_quota(&mut self, agent_id: [u8; 16], quota: ResourceQuota) {
        self.enforcers.insert(agent_id, QuotaEnforcer::new(quota));
    }
    /// Unregister an agent
    pub fn unregister_agent(&mut self, agent_id: &[u8; 16]) {
        if let Some(enforcer) = self.enforcers.remove(agent_id) {
            self.total_memory_used
                .sub(enforcer.usage().total_memory_bytes);
        }
    }
    /// Get an agent's enforcer
    pub fn get_enforcer(&self, agent_id: &[u8; 16]) -> Option<&QuotaEnforcer> {
        self.enforcers.get(agent_id)
    }
    /// Get an agent's enforcer mutably
    pub fn get_enforcer_mut(&mut self, agent_id: &[u8; 16]) -> Option<&mut QuotaEnforcer> {
        self.enforcers.get_mut(agent_id)
    }
    /// Update an agent's quota
    pub fn update_quota(&mut self, agent_id: &[u8; 16], quota: ResourceQuota) {
        if let Some(enforcer) = self.enforcers.get_mut(agent_id) {
            enforcer.set_quota(quota);
        }
    }
    /// Check if global memory limit allows allocation
    pub fn can_allocate_global(&self, bytes: u64) -> bool {
        self.total_memory_used.get() + bytes <= self.global_memory_limit
    }
    /// Record global memory allocation
    pub fn record_global_allocation(&self, bytes: u64) {
        self.total_memory_used.add(bytes);
    }
    /// Record global memory deallocation
    pub fn record_global_deallocation(&self, bytes: u64) {
        self.total_memory_used.sub(bytes);
    }
    /// Get total memory used across all agents
    pub fn total_memory_used(&self) -> u64 {
        self.total_memory_used.get()
    }
    /// Get the number of registered agents
    pub fn agent_count(&self) -> usize {
        self.enforcers.len()
    }
    /// Get a summary of all agents' resource usage
    pub fn usage_summary(&self) -> Vec<([u8; 16], ResourceUsage)> {
        self.enforcers
            .iter()
            .map(|(id, e)| (*id, e.usage().clone()))
            .collect()
    }
}
/// Summary of monitoring data
#[derive(Debug, Clone)]
pub struct MonitorSummary {
    /// Agent ID
    pub agent_id: [u8; 16],
    /// Number of snapshots
    pub snapshot_count: usize,
    /// Peak memory usage
    pub peak_memory_bytes: u64,
    /// Peak CPU time
    pub peak_cpu_time_us: u64,
    /// Average memory usage
    pub average_memory_bytes: u64,
    /// Average CPU time
    pub average_cpu_time_us: u64,
    /// Latest snapshot
    pub latest_snapshot: Option<ResourceSnapshot>,
    /// Total anomaly count
    pub anomaly_count: usize,
    /// High severity anomaly count
    pub high_severity_anomalies: usize,
}
/// Detects anomalies in resource usage
#[derive(Debug)]
pub struct AnomalyDetector {
    /// Configuration
    config: AnomalyConfig,
    /// Detected anomalies
    anomalies: Vec<Anomaly>,
    /// Maximum anomalies to keep
    max_anomalies: usize,
}
impl AnomalyDetector {
    /// Create a new anomaly detector
    pub fn new(config: AnomalyConfig) -> Self {
        Self {
            config,
            anomalies: Vec::new(),
            max_anomalies: 1000,
        }
    }
    /// Get the configuration
    pub fn config(&self) -> &AnomalyConfig {
        &self.config
    }
    /// Update the configuration
    pub fn set_config(&mut self, config: AnomalyConfig) {
        self.config = config;
    }
    /// Analyze history and detect anomalies
    pub fn analyze(&mut self, history: &ResourceHistory, current_time_us: u64) -> Vec<Anomaly> {
        let mut new_anomalies = Vec::new();
        if history.len() < 2 {
            return new_anomalies;
        }
        let avg = match history.average() {
            Some(a) => a,
            None => return new_anomalies,
        };
        let latest = match history.latest() {
            Some(l) => l,
            None => return new_anomalies,
        };
        if avg.memory_bytes > 0 {
            let ratio = latest.memory_bytes as f64 / avg.memory_bytes as f64;
            if ratio > self.config.memory_spike_threshold {
                let severity = ((ratio - 1.0) / self.config.memory_spike_threshold).min(1.0);
                new_anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::MemorySpike,
                    detected_at_us: current_time_us,
                    severity,
                    current_value: latest.memory_bytes as f64,
                    expected_value: avg.memory_bytes as f64,
                    description: format!("Memory usage is {:.1}x the average", ratio),
                });
            }
        }
        if history.len() >= self.config.leak_detection_samples {
            let samples: Vec<_> = history
                .snapshots()
                .iter()
                .rev()
                .take(self.config.leak_detection_samples)
                .collect();
            let growing_count = samples
                .windows(2)
                .filter(|w| w[0].memory_bytes > w[1].memory_bytes)
                .count();
            let growth_ratio = growing_count as f64 / (samples.len() - 1) as f64;
            if growth_ratio >= self.config.leak_growth_threshold {
                let first_mem = samples.last().map(|s| s.memory_bytes).unwrap_or(0);
                let last_mem = samples.first().map(|s| s.memory_bytes).unwrap_or(0);
                let total_growth = if first_mem > 0 {
                    (last_mem as f64 / first_mem as f64) - 1.0
                } else {
                    0.0
                };
                new_anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::MemoryLeak,
                    detected_at_us: current_time_us,
                    severity: growth_ratio,
                    current_value: last_mem as f64,
                    expected_value: first_mem as f64,
                    description: format!(
                        "Memory grew {:.1}% over {} samples ({:.1}% of samples showed growth)",
                        total_growth * 100.0,
                        samples.len(),
                        growth_ratio * 100.0
                    ),
                });
            }
        }
        let avg_network = avg.network_sent_bytes + avg.network_recv_bytes;
        let latest_network = latest.network_sent_bytes + latest.network_recv_bytes;
        if avg_network > 0 {
            let ratio = latest_network as f64 / avg_network as f64;
            if ratio > self.config.network_spike_threshold {
                let severity = ((ratio - 1.0) / self.config.network_spike_threshold).min(1.0);
                new_anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::NetworkSpike,
                    detected_at_us: current_time_us,
                    severity,
                    current_value: latest_network as f64,
                    expected_value: avg_network as f64,
                    description: format!("Network traffic is {:.1}x the average", ratio),
                });
            }
        }
        if avg.storage_bytes > 0 {
            let ratio = latest.storage_bytes as f64 / avg.storage_bytes as f64;
            if ratio > self.config.storage_spike_threshold {
                let severity = ((ratio - 1.0) / self.config.storage_spike_threshold).min(1.0);
                new_anomalies.push(Anomaly {
                    anomaly_type: AnomalyType::StorageSpike,
                    detected_at_us: current_time_us,
                    severity,
                    current_value: latest.storage_bytes as f64,
                    expected_value: avg.storage_bytes as f64,
                    description: format!("Storage usage is {:.1}x the average", ratio),
                });
            }
        }
        for anomaly in &new_anomalies {
            self.anomalies.push(anomaly.clone());
        }
        while self.anomalies.len() > self.max_anomalies {
            self.anomalies.remove(0);
        }
        new_anomalies
    }
    /// Get all detected anomalies
    pub fn anomalies(&self) -> &[Anomaly] {
        &self.anomalies
    }
    /// Get anomalies of a specific type
    pub fn anomalies_of_type(&self, anomaly_type: AnomalyType) -> Vec<&Anomaly> {
        self.anomalies
            .iter()
            .filter(|a| a.anomaly_type == anomaly_type)
            .collect()
    }
    /// Get anomalies in a time range
    pub fn anomalies_in_range(&self, start_us: u64, end_us: u64) -> Vec<&Anomaly> {
        self.anomalies
            .iter()
            .filter(|a| a.detected_at_us >= start_us && a.detected_at_us <= end_us)
            .collect()
    }
    /// Get high severity anomalies (severity >= threshold)
    pub fn high_severity_anomalies(&self, threshold: f64) -> Vec<&Anomaly> {
        self.anomalies
            .iter()
            .filter(|a| a.severity >= threshold)
            .collect()
    }
    /// Clear all anomalies
    pub fn clear(&mut self) {
        self.anomalies.clear();
    }
}
/// Predicts future resource usage based on history.
///
/// For histories with at least `min_samples` data points this delegates to
/// `EnhancedPredictor` (Holt double exponential smoothing + ridge-regularised
/// linear regression with R²-based confidence).  Shorter histories fall
/// back to the original simple linear extrapolation so callers always
/// receive `Some` whenever `history.len() >= min_samples`.
#[derive(Debug)]
pub struct ResourcePredictor {
    /// Minimum samples needed for prediction
    pub(super) min_samples: usize,
    /// Minimum samples required before the ML path is preferred over linear
    pub(super) ml_min_samples: usize,
    /// Cached ML predictor (re-created on each call — kept for future
    /// stateful extension via `ResourceMonitor`)
    pub(super) enhanced: super::predictor_ml::EnhancedPredictor,
}
impl ResourcePredictor {
    /// Create a new resource predictor
    pub fn new() -> Self {
        Self {
            min_samples: 5,
            // Require a richer history before switching to the ML path so that
            // the Holt smoothing has enough observations to converge.
            ml_min_samples: 15,
            enhanced: super::predictor_ml::EnhancedPredictor::new(),
        }
    }
    /// Set the minimum samples needed
    pub fn with_min_samples(mut self, min_samples: usize) -> Self {
        self.min_samples = min_samples.max(2);
        self.ml_min_samples = self.min_samples;
        self
    }
    /// Predict resource usage at a future time.
    ///
    /// Uses the ML-enhanced (Holt + ridge) path when sufficient data is
    /// available, otherwise falls back to plain linear extrapolation.
    /// The function signature is unchanged from the original implementation.
    pub fn predict(
        &self,
        history: &ResourceHistory,
        horizon_us: u64,
    ) -> Option<ResourcePrediction> {
        if history.len() < self.min_samples {
            return None;
        }
        let snapshots = history.snapshots();
        let n = snapshots.len();

        // ── ML-enhanced path ───────────────────────────────────────────────
        if n >= self.ml_min_samples {
            return Some(
                self.enhanced
                    .predict_from_history(snapshots, horizon_us),
            );
        }

        // ── Fallback: original linear extrapolation ────────────────────────
        let memory_pred = self.linear_predict(
            snapshots
                .iter()
                .map(|s| (s.timestamp_us as f64, s.memory_bytes as f64)),
            history.latest()?.timestamp_us as f64 + horizon_us as f64,
        )?;
        let storage_pred = self.linear_predict(
            snapshots
                .iter()
                .map(|s| (s.timestamp_us as f64, s.storage_bytes as f64)),
            history.latest()?.timestamp_us as f64 + horizon_us as f64,
        )?;
        let network_rate = if n >= 2 {
            let first = &snapshots[0];
            let last = &snapshots[n - 1];
            let time_delta = (last.timestamp_us - first.timestamp_us) as f64 / 1_000_000.0;
            if time_delta > 0.0 {
                let total_net = (last.network_sent_bytes + last.network_recv_bytes) as f64
                    - (first.network_sent_bytes + first.network_recv_bytes) as f64;
                (total_net / time_delta) as u64
            } else {
                0
            }
        } else {
            0
        };
        let cpu_avg = if n >= 2 {
            let rates: Vec<f64> = snapshots
                .windows(2)
                .filter_map(|w| {
                    let time_delta = w[1].timestamp_us.saturating_sub(w[0].timestamp_us);
                    if time_delta > 0 {
                        let cpu_delta = w[1].cpu_time_us.saturating_sub(w[0].cpu_time_us);
                        Some((cpu_delta as f64 / time_delta as f64) * 100.0)
                    } else {
                        None
                    }
                })
                .collect();
            if rates.is_empty() {
                0.0
            } else {
                rates.iter().sum::<f64>() / rates.len() as f64
            }
        } else {
            0.0
        };
        let confidence = (n as f64 / 100.0).min(0.9);
        Some(ResourcePrediction {
            memory_bytes: memory_pred.max(0.0) as u64,
            cpu_percent: cpu_avg.clamp(0.0, 100.0),
            network_bytes_per_sec: network_rate,
            storage_bytes: storage_pred.max(0.0) as u64,
            confidence,
            horizon_us,
        })
    }
    /// Simple linear regression prediction (fallback path)
    fn linear_predict<I>(&self, points: I, target_x: f64) -> Option<f64>
    where
        I: Iterator<Item = (f64, f64)>,
    {
        let points: Vec<(f64, f64)> = points.collect();
        let n = points.len() as f64;
        if n < 2.0 {
            return None;
        }
        let sum_x: f64 = points.iter().map(|(x, _)| x).sum();
        let sum_y: f64 = points.iter().map(|(_, y)| y).sum();
        let sum_xy: f64 = points.iter().map(|(x, y)| x * y).sum();
        let sum_x2: f64 = points.iter().map(|(x, _)| x * x).sum();
        let denominator = n * sum_x2 - sum_x * sum_x;
        if denominator.abs() < 1e-10 {
            return Some(sum_y / n);
        }
        let slope = (n * sum_xy - sum_x * sum_y) / denominator;
        let intercept = (sum_y - slope * sum_x) / n;
        Some(slope * target_x + intercept)
    }
    /// Predict when a quota will be exceeded.
    ///
    /// Signature is unchanged; uses ridge-based memory slope when
    /// sufficient history is available, otherwise uses simple linear rate.
    pub fn predict_quota_breach(
        &self,
        history: &ResourceHistory,
        quota: &ResourceQuota,
    ) -> Option<u64> {
        if history.len() < self.min_samples {
            return None;
        }
        let latest = history.latest()?;
        let snapshots = history.snapshots();
        let n = snapshots.len();

        // Use Holt-extrapolated memory slope when we have enough data
        if n >= self.ml_min_samples {
            let horizon_us = 1_000_000u64; // 1 second probe
            let probe = self.enhanced.predict_from_history(snapshots, horizon_us);
            let current_mem = latest.memory_bytes;
            if probe.memory_bytes > current_mem {
                let memory_rate_per_us =
                    (probe.memory_bytes - current_mem) as f64 / horizon_us as f64;
                let remaining = quota
                    .memory
                    .max_total_bytes
                    .saturating_sub(current_mem);
                if memory_rate_per_us > 0.0 {
                    let time_to_breach = (remaining as f64 / memory_rate_per_us) as u64;
                    return Some(latest.timestamp_us + time_to_breach);
                }
            }
            return None;
        }

        // Fallback: simple linear memory rate
        if n >= 2 {
            let first = &snapshots[0];
            let last = &snapshots[n - 1];
            let time_delta = (last.timestamp_us - first.timestamp_us) as f64;
            if time_delta > 0.0 && last.memory_bytes > first.memory_bytes {
                let memory_rate = (last.memory_bytes - first.memory_bytes) as f64 / time_delta;
                let remaining = quota
                    .memory
                    .max_total_bytes
                    .saturating_sub(latest.memory_bytes);
                if memory_rate > 0.0 {
                    let time_to_breach = (remaining as f64 / memory_rate) as u64;
                    return Some(latest.timestamp_us + time_to_breach);
                }
            }
        }
        None
    }
}
/// Type of anomaly detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyType {
    /// Memory usage spike
    MemorySpike,
    /// Memory leak (continuous growth)
    MemoryLeak,
    /// CPU spike
    CpuSpike,
    /// Network traffic spike
    NetworkSpike,
    /// Storage growth spike
    StorageSpike,
    /// Sudden drop in activity
    ActivityDrop,
}
impl AnomalyType {
    /// Get a description of the anomaly type
    pub fn description(&self) -> &'static str {
        match self {
            Self::MemorySpike => "Sudden increase in memory usage",
            Self::MemoryLeak => "Continuous memory growth detected",
            Self::CpuSpike => "Sudden increase in CPU usage",
            Self::NetworkSpike => "Sudden increase in network traffic",
            Self::StorageSpike => "Sudden increase in storage usage",
            Self::ActivityDrop => "Sudden drop in activity",
        }
    }
}
/// Resource usage prediction
#[derive(Debug, Clone)]
pub struct ResourcePrediction {
    /// Predicted memory usage in bytes
    pub memory_bytes: u64,
    /// Predicted CPU utilization (0-100)
    pub cpu_percent: f64,
    /// Predicted network bytes per second
    pub network_bytes_per_sec: u64,
    /// Predicted storage bytes
    pub storage_bytes: u64,
    /// Confidence level (0.0-1.0)
    pub confidence: f64,
    /// Prediction horizon in microseconds
    pub horizon_us: u64,
}
/// Storage quota configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StorageQuota {
    /// Maximum persistent storage in bytes
    pub max_persistent_bytes: u64,
    /// Maximum temporary storage in bytes
    pub max_temp_bytes: u64,
    /// Maximum number of files
    pub max_files: u32,
    /// Maximum single file size in bytes
    pub max_file_size: u64,
}
impl StorageQuota {
    /// Create an unlimited storage quota
    pub fn unlimited() -> Self {
        Self {
            max_persistent_bytes: u64::MAX,
            max_temp_bytes: u64::MAX,
            max_files: u32::MAX,
            max_file_size: u64::MAX,
        }
    }
    /// Create a storage quota with specified limits
    pub fn with_limits(persistent: u64, temp: u64) -> Self {
        Self {
            max_persistent_bytes: persistent,
            max_temp_bytes: temp,
            max_files: 10000,
            max_file_size: persistent / 10,
        }
    }
}
/// Violation type when quota is exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaViolation {
    /// Heap memory exceeded
    HeapMemoryExceeded,
    /// Stack memory exceeded
    StackMemoryExceeded,
    /// Total memory exceeded
    TotalMemoryExceeded,
    /// Memory soft limit reached (warning)
    MemorySoftLimitReached,
    /// CPU execution time exceeded
    CpuExecutionTimeExceeded,
    /// CPU rate limit exceeded
    CpuRateLimitExceeded,
    /// Instruction limit exceeded
    InstructionLimitExceeded,
    /// Send bandwidth exceeded
    SendBandwidthExceeded,
    /// Receive bandwidth exceeded
    RecvBandwidthExceeded,
    /// Total bandwidth exceeded
    TotalBandwidthExceeded,
    /// Connection limit exceeded
    ConnectionLimitExceeded,
    /// Message rate exceeded
    MessageRateExceeded,
    /// Persistent storage exceeded
    PersistentStorageExceeded,
    /// Temporary storage exceeded
    TempStorageExceeded,
    /// File count exceeded
    FileCountExceeded,
    /// File size exceeded
    FileSizeExceeded,
}
impl QuotaViolation {
    /// Check if this is a warning (soft limit) rather than a hard violation
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::MemorySoftLimitReached)
    }
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::HeapMemoryExceeded => "Heap memory limit exceeded",
            Self::StackMemoryExceeded => "Stack memory limit exceeded",
            Self::TotalMemoryExceeded => "Total memory limit exceeded",
            Self::MemorySoftLimitReached => "Memory soft limit reached (warning)",
            Self::CpuExecutionTimeExceeded => "CPU execution time limit exceeded",
            Self::CpuRateLimitExceeded => "CPU rate limit exceeded",
            Self::InstructionLimitExceeded => "Instruction limit exceeded",
            Self::SendBandwidthExceeded => "Send bandwidth limit exceeded",
            Self::RecvBandwidthExceeded => "Receive bandwidth limit exceeded",
            Self::TotalBandwidthExceeded => "Total bandwidth limit exceeded",
            Self::ConnectionLimitExceeded => "Connection limit exceeded",
            Self::MessageRateExceeded => "Message rate limit exceeded",
            Self::PersistentStorageExceeded => "Persistent storage limit exceeded",
            Self::TempStorageExceeded => "Temporary storage limit exceeded",
            Self::FileCountExceeded => "File count limit exceeded",
            Self::FileSizeExceeded => "Maximum file size exceeded",
        }
    }
}
/// Current resource usage for an agent
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// Current heap memory usage
    pub heap_bytes: u64,
    /// Current stack memory usage
    pub stack_bytes: u64,
    /// Total memory usage
    pub total_memory_bytes: u64,
    /// CPU time used in current second (microseconds)
    pub cpu_time_this_second_us: u64,
    /// Total CPU time used (microseconds)
    pub total_cpu_time_us: u64,
    /// Instructions executed
    pub instructions_executed: u64,
    /// Bytes sent this second
    pub bytes_sent_this_second: u64,
    /// Bytes received this second
    pub bytes_received_this_second: u64,
    /// Total bytes sent
    pub total_bytes_sent: u64,
    /// Total bytes received
    pub total_bytes_received: u64,
    /// Current connection count
    pub active_connections: u32,
    /// Messages sent this second
    pub messages_this_second: u32,
    /// Persistent storage used
    pub persistent_storage_bytes: u64,
    /// Temporary storage used
    pub temp_storage_bytes: u64,
    /// File count
    pub file_count: u32,
}
impl ResourceUsage {
    /// Calculate total network usage this second
    pub fn network_bytes_this_second(&self) -> u64 {
        self.bytes_sent_this_second + self.bytes_received_this_second
    }
    /// Calculate total storage usage
    pub fn total_storage_bytes(&self) -> u64 {
        self.persistent_storage_bytes + self.temp_storage_bytes
    }
}
/// Combined resource quota for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    /// Memory limits
    pub memory: MemoryQuota,
    /// CPU time limits
    pub cpu: CpuQuota,
    /// Network bandwidth limits
    pub network: NetworkQuota,
    /// Storage limits
    pub storage: StorageQuota,
    /// Whether quotas are enforced
    pub enforced: bool,
}
impl ResourceQuota {
    /// Create an unlimited quota (no enforcement)
    pub fn unlimited() -> Self {
        Self {
            memory: MemoryQuota::unlimited(),
            cpu: CpuQuota::unlimited(),
            network: NetworkQuota::unlimited(),
            storage: StorageQuota::unlimited(),
            enforced: false,
        }
    }
    /// Create a minimal quota for testing
    pub fn minimal() -> Self {
        Self {
            memory: MemoryQuota::with_limits(16 * 1024 * 1024, 1024 * 1024, 32 * 1024 * 1024),
            cpu: CpuQuota::with_limits(1_000_000, 100_000),
            network: NetworkQuota::with_bandwidth(1024 * 1024, 1024 * 1024),
            storage: StorageQuota::with_limits(10 * 1024 * 1024, 1024 * 1024),
            enforced: true,
        }
    }
    /// Create a high-performance quota
    pub fn high_performance() -> Self {
        Self {
            memory: MemoryQuota::with_limits(
                4 * 1024 * 1024 * 1024,
                64 * 1024 * 1024,
                8 * 1024 * 1024 * 1024,
            ),
            cpu: CpuQuota::with_limits(60_000_000, 900_000).with_priority(80),
            network: NetworkQuota::with_bandwidth(100 * 1024 * 1024, 100 * 1024 * 1024)
                .with_connection_limit(1000),
            storage: StorageQuota::with_limits(100 * 1024 * 1024 * 1024, 10 * 1024 * 1024 * 1024),
            enforced: true,
        }
    }
}
/// Rate of resource usage change
#[derive(Debug, Clone, Copy)]
pub struct ResourceRate {
    /// Memory change in bytes per second (can be negative)
    pub memory_bytes_per_sec: f64,
    /// CPU utilization percentage (0-100)
    pub cpu_percent: f64,
    /// Network bytes sent per second
    pub network_sent_bytes_per_sec: f64,
    /// Network bytes received per second
    pub network_recv_bytes_per_sec: f64,
    /// Storage change in bytes per second (can be negative)
    pub storage_bytes_per_sec: f64,
}
impl ResourceRate {
    /// Check if memory is growing
    pub fn is_memory_growing(&self) -> bool {
        self.memory_bytes_per_sec > 0.0
    }
    /// Check if storage is growing
    pub fn is_storage_growing(&self) -> bool {
        self.storage_bytes_per_sec > 0.0
    }
    /// Get total network throughput
    pub fn total_network_bytes_per_sec(&self) -> f64 {
        self.network_sent_bytes_per_sec + self.network_recv_bytes_per_sec
    }
}
