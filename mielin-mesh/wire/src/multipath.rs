//! Multi-Path Network Support
//!
//! Provides comprehensive multi-path networking capabilities:
//! - Multiple network paths per peer connection
//! - Automatic path failover on failure
//! - Load balancing across healthy paths
//! - Path MTU discovery and tracking
//!
//! # Example
//!
//! ```rust,no_run
//! use mielin_mesh_wire::multipath::{PathPool, PathPoolConfig, NetworkPath, PathSelector, MultiPathPolicy};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a path pool for a peer
//! let config = PathPoolConfig::default();
//! let mut pool = PathPool::new([1u8; 16], config);
//!
//! // Add paths
//! let path = NetworkPath::new("192.168.1.1:8080".to_string(), false);
//! pool.add_path(path);
//!
//! // Select best path using policy
//! let selector = PathSelector::new(MultiPathPolicy::LeastLatency);
//! if let Some(path) = selector.select(&pool).await {
//!     println!("Selected path: {}", path.remote_address);
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

// =============================================================================
// Error Types
// =============================================================================

/// Errors related to multi-path operations
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MultiPathError {
    /// No paths available for the peer
    #[error("No paths available for peer")]
    NoPathsAvailable,

    /// Path not found
    #[error("Path not found: {path_id}")]
    PathNotFound { path_id: PathId },

    /// Path is unhealthy and cannot be used
    #[error("Path is unhealthy: {path_id}")]
    PathUnhealthy { path_id: PathId },

    /// Maximum number of paths reached
    #[error("Maximum paths limit reached: {max}")]
    MaxPathsReached { max: usize },

    /// Failover failed - no backup paths available
    #[error("Failover failed: no healthy backup paths")]
    FailoverFailed,

    /// MTU discovery failed
    #[error("MTU discovery failed: {reason}")]
    MtuDiscoveryFailed { reason: String },

    /// Path already exists
    #[error("Path already exists: {remote_address}")]
    PathAlreadyExists { remote_address: String },

    /// Invalid MTU value
    #[error("Invalid MTU value: {mtu} (must be between {min} and {max})")]
    InvalidMtu { mtu: u32, min: u32, max: u32 },

    /// Path selection failed
    #[error("Path selection failed: {reason}")]
    SelectionFailed { reason: String },

    /// Timeout during operation
    #[error("Operation timed out after {elapsed_ms}ms")]
    Timeout { elapsed_ms: u64 },
}

// =============================================================================
// Path Identifier
// =============================================================================

/// Unique identifier for a network path
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathId(pub u64);

impl PathId {
    /// Generate a new unique path ID
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Create from raw value
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Get raw value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for PathId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "path-{}", self.0)
    }
}

// =============================================================================
// Path Information
// =============================================================================

/// Metrics and status information for a network path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInfo {
    /// Smoothed round-trip time in microseconds
    pub latency_us: u64,

    /// RTT variance in microseconds
    pub latency_var_us: u64,

    /// Estimated bandwidth in bytes per second
    pub bandwidth_bps: u64,

    /// Packet loss rate (0.0 - 1.0)
    pub loss_rate: f64,

    /// Number of successful transmissions
    pub successful_sends: u64,

    /// Number of failed transmissions
    pub failed_sends: u64,

    /// Total bytes sent on this path
    pub bytes_sent: u64,

    /// Total bytes received on this path
    pub bytes_received: u64,

    /// Last time the path was successfully used (epoch millis)
    pub last_success_ms: u64,

    /// Last time the path failed (epoch millis)
    pub last_failure_ms: u64,

    /// Current path MTU in bytes
    pub mtu: u32,

    /// Whether MTU discovery is complete
    pub mtu_discovered: bool,

    /// Congestion window size
    pub cwnd: u64,

    /// Is the path currently probing
    pub probing: bool,
}

impl Default for PathInfo {
    fn default() -> Self {
        Self {
            latency_us: 0,
            latency_var_us: 0,
            bandwidth_bps: 0,
            loss_rate: 0.0,
            successful_sends: 0,
            failed_sends: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_success_ms: 0,
            last_failure_ms: 0,
            mtu: DEFAULT_MTU,
            mtu_discovered: false,
            cwnd: 65535,
            probing: false,
        }
    }
}

impl PathInfo {
    /// Calculate reliability score (0.0 - 1.0)
    pub fn reliability(&self) -> f64 {
        let total = self.successful_sends + self.failed_sends;
        if total == 0 {
            return 0.5; // Unknown reliability
        }
        self.successful_sends as f64 / total as f64
    }

    /// Calculate composite score for path selection (higher is better)
    pub fn score(&self) -> f64 {
        // Weight factors
        const LATENCY_WEIGHT: f64 = 0.4;
        const BANDWIDTH_WEIGHT: f64 = 0.3;
        const RELIABILITY_WEIGHT: f64 = 0.3;

        // Normalize latency (lower is better, cap at 1000ms)
        let latency_score = 1.0 - (self.latency_us as f64 / 1_000_000.0).min(1.0);

        // Normalize bandwidth (higher is better, cap at 1Gbps)
        let bandwidth_score = (self.bandwidth_bps as f64 / 1_000_000_000.0).min(1.0);

        // Reliability is already 0-1
        let reliability_score = self.reliability();

        (LATENCY_WEIGHT * latency_score)
            + (BANDWIDTH_WEIGHT * bandwidth_score)
            + (RELIABILITY_WEIGHT * reliability_score)
    }

    /// Check if path is considered healthy
    pub fn is_healthy(&self, config: &PathHealthConfig) -> bool {
        // Check loss rate
        if self.loss_rate > config.max_loss_rate {
            return false;
        }

        // Check latency
        if self.latency_us > config.max_latency_us {
            return false;
        }

        // Check recent failures
        let now_ms = current_time_ms();
        if self.last_failure_ms > 0 {
            let since_failure = now_ms.saturating_sub(self.last_failure_ms);
            if since_failure < config.recovery_period_ms {
                // Check if we've had a success since the failure
                if self.last_success_ms <= self.last_failure_ms {
                    return false;
                }
            }
        }

        true
    }
}

/// Configuration for path health checks
#[derive(Debug, Clone)]
pub struct PathHealthConfig {
    /// Maximum acceptable packet loss rate
    pub max_loss_rate: f64,

    /// Maximum acceptable latency in microseconds
    pub max_latency_us: u64,

    /// Time to wait before considering a failed path healthy again
    pub recovery_period_ms: u64,

    /// Minimum successful sends to consider reliable
    pub min_samples: u64,
}

impl Default for PathHealthConfig {
    fn default() -> Self {
        Self {
            max_loss_rate: 0.1,         // 10% loss threshold
            max_latency_us: 500_000,    // 500ms latency threshold
            recovery_period_ms: 10_000, // 10 second recovery
            min_samples: 10,
        }
    }
}

// =============================================================================
// Network Path
// =============================================================================

/// Default MTU (conservative for internet)
pub const DEFAULT_MTU: u32 = 1280;

/// Minimum MTU (IPv6 minimum)
pub const MIN_MTU: u32 = 1280;

/// Maximum MTU (jumbo frames)
pub const MAX_MTU: u32 = 9000;

/// Path state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathState {
    /// Path is active and being used
    Active,

    /// Path is available but not primary
    Standby,

    /// Path is being probed for health
    Probing,

    /// Path has failed and is in recovery
    Failed,

    /// Path is disabled (manually or due to repeated failures)
    Disabled,
}

impl Default for PathState {
    fn default() -> Self {
        Self::Standby
    }
}

/// A network path to a remote peer
#[derive(Debug)]
pub struct NetworkPath {
    /// Unique path identifier
    pub id: PathId,

    /// Remote address (IP:port or domain:port)
    pub remote_address: String,

    /// Whether this is the primary path
    is_primary: AtomicBool,

    /// Current path state
    state: Mutex<PathState>,

    /// Path metrics and info
    info: RwLock<PathInfo>,

    /// Weight for load balancing (higher = more traffic)
    weight: AtomicU32,

    /// Priority for failover (lower = higher priority)
    priority: AtomicU32,

    /// Creation timestamp
    created_at: Instant,

    /// Health check config
    health_config: PathHealthConfig,
}

impl NetworkPath {
    /// Create a new network path
    pub fn new(remote_address: String, is_primary: bool) -> Self {
        Self {
            id: PathId::generate(),
            remote_address,
            is_primary: AtomicBool::new(is_primary),
            state: Mutex::new(if is_primary {
                PathState::Active
            } else {
                PathState::Standby
            }),
            info: RwLock::new(PathInfo::default()),
            weight: AtomicU32::new(100),
            priority: AtomicU32::new(if is_primary { 0 } else { 100 }),
            created_at: Instant::now(),
            health_config: PathHealthConfig::default(),
        }
    }

    /// Create a new path with custom ID
    pub fn with_id(id: PathId, remote_address: String, is_primary: bool) -> Self {
        Self {
            id,
            remote_address,
            is_primary: AtomicBool::new(is_primary),
            state: Mutex::new(if is_primary {
                PathState::Active
            } else {
                PathState::Standby
            }),
            info: RwLock::new(PathInfo::default()),
            weight: AtomicU32::new(100),
            priority: AtomicU32::new(if is_primary { 0 } else { 100 }),
            created_at: Instant::now(),
            health_config: PathHealthConfig::default(),
        }
    }

    /// Check if this is the primary path
    pub fn is_primary(&self) -> bool {
        self.is_primary.load(Ordering::Relaxed)
    }

    /// Set whether this is the primary path
    pub fn set_primary(&self, primary: bool) {
        self.is_primary.store(primary, Ordering::Relaxed);
    }

    /// Get current state
    pub async fn state(&self) -> PathState {
        *self.state.lock().await
    }

    /// Set path state
    pub async fn set_state(&self, state: PathState) {
        *self.state.lock().await = state;
    }

    /// Get path info
    pub async fn info(&self) -> PathInfo {
        self.info.read().await.clone()
    }

    /// Get path weight
    pub fn weight(&self) -> u32 {
        self.weight.load(Ordering::Relaxed)
    }

    /// Set path weight
    pub fn set_weight(&self, weight: u32) {
        self.weight.store(weight, Ordering::Relaxed);
    }

    /// Get path priority
    pub fn priority(&self) -> u32 {
        self.priority.load(Ordering::Relaxed)
    }

    /// Set path priority
    pub fn set_priority(&self, priority: u32) {
        self.priority.store(priority, Ordering::Relaxed);
    }

    /// Get age of the path
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Update path with RTT sample
    pub async fn update_rtt(&self, rtt_us: u64) {
        let mut info = self.info.write().await;

        if info.latency_us == 0 {
            // First sample
            info.latency_us = rtt_us;
            info.latency_var_us = rtt_us / 2;
        } else {
            // Exponential moving average (alpha = 0.125)
            let diff = (rtt_us as i64 - info.latency_us as i64).unsigned_abs();
            info.latency_var_us =
                ((info.latency_var_us as f64 * 0.75) + (diff as f64 * 0.25)) as u64;
            info.latency_us = ((info.latency_us as f64 * 0.875) + (rtt_us as f64 * 0.125)) as u64;
        }
    }

    /// Record successful transmission
    pub async fn record_success(&self, bytes: u64) {
        let mut info = self.info.write().await;
        info.successful_sends += 1;
        info.bytes_sent += bytes;
        info.last_success_ms = current_time_ms();

        // Update loss rate
        let total = info.successful_sends + info.failed_sends;
        info.loss_rate = if total > 0 {
            info.failed_sends as f64 / total as f64
        } else {
            0.0
        };
    }

    /// Record failed transmission
    pub async fn record_failure(&self) {
        let mut info = self.info.write().await;
        info.failed_sends += 1;
        info.last_failure_ms = current_time_ms();

        // Update loss rate
        let total = info.successful_sends + info.failed_sends;
        info.loss_rate = if total > 0 {
            info.failed_sends as f64 / total as f64
        } else {
            1.0
        };
    }

    /// Record bytes received
    pub async fn record_receive(&self, bytes: u64) {
        let mut info = self.info.write().await;
        info.bytes_received += bytes;
    }

    /// Update bandwidth estimate
    pub async fn update_bandwidth(&self, bytes: u64, duration_us: u64) {
        if duration_us > 0 {
            let bps = (bytes as f64 * 1_000_000.0 / duration_us as f64) as u64;
            let mut info = self.info.write().await;
            // Exponential moving average
            if info.bandwidth_bps == 0 {
                info.bandwidth_bps = bps;
            } else {
                info.bandwidth_bps =
                    ((info.bandwidth_bps as f64 * 0.8) + (bps as f64 * 0.2)) as u64;
            }
        }
    }

    /// Set MTU
    pub async fn set_mtu(&self, mtu: u32) -> Result<(), MultiPathError> {
        if !(MIN_MTU..=MAX_MTU).contains(&mtu) {
            return Err(MultiPathError::InvalidMtu {
                mtu,
                min: MIN_MTU,
                max: MAX_MTU,
            });
        }
        let mut info = self.info.write().await;
        info.mtu = mtu;
        Ok(())
    }

    /// Mark MTU discovery as complete
    pub async fn set_mtu_discovered(&self, discovered: bool) {
        let mut info = self.info.write().await;
        info.mtu_discovered = discovered;
    }

    /// Check if path is healthy
    pub async fn is_healthy(&self) -> bool {
        let info = self.info.read().await;
        info.is_healthy(&self.health_config)
    }

    /// Get current MTU
    pub async fn mtu(&self) -> u32 {
        self.info.read().await.mtu
    }
}

// =============================================================================
// Path Pool Configuration
// =============================================================================

/// Configuration for path pool
#[derive(Debug, Clone)]
pub struct PathPoolConfig {
    /// Maximum number of paths per peer
    pub max_paths: usize,

    /// Enable automatic failover
    pub auto_failover: bool,

    /// Failover threshold (consecutive failures)
    pub failover_threshold: u32,

    /// Target failover time in milliseconds
    pub failover_target_ms: u64,

    /// Path probe interval in milliseconds
    pub probe_interval_ms: u64,

    /// Enable load balancing
    pub load_balancing: bool,

    /// Health check configuration
    pub health_config: PathHealthConfig,
}

impl Default for PathPoolConfig {
    fn default() -> Self {
        Self {
            max_paths: 8,
            auto_failover: true,
            failover_threshold: 3,
            failover_target_ms: 50, // Target <50ms failover
            probe_interval_ms: 5000,
            load_balancing: true,
            health_config: PathHealthConfig::default(),
        }
    }
}

impl PathPoolConfig {
    /// High availability configuration
    pub fn high_availability() -> Self {
        Self {
            max_paths: 16,
            auto_failover: true,
            failover_threshold: 2,
            failover_target_ms: 25,
            probe_interval_ms: 2000,
            load_balancing: true,
            health_config: PathHealthConfig {
                max_loss_rate: 0.05,
                max_latency_us: 200_000,
                recovery_period_ms: 5000,
                min_samples: 5,
            },
        }
    }

    /// Embedded/low-resource configuration
    pub fn embedded() -> Self {
        Self {
            max_paths: 2,
            auto_failover: true,
            failover_threshold: 5,
            failover_target_ms: 100,
            probe_interval_ms: 30000,
            load_balancing: false,
            health_config: PathHealthConfig {
                max_loss_rate: 0.2,
                max_latency_us: 1_000_000,
                recovery_period_ms: 30000,
                min_samples: 3,
            },
        }
    }
}

// =============================================================================
// Path Pool
// =============================================================================

/// Pool of network paths to a single peer
pub struct PathPool {
    /// Peer identifier
    pub peer_id: [u8; 16],

    /// Configuration
    config: PathPoolConfig,

    /// Active paths indexed by PathId
    paths: RwLock<HashMap<PathId, Arc<NetworkPath>>>,

    /// Statistics
    stats: RwLock<PathPoolStats>,

    /// Current primary path ID
    primary_path_id: Mutex<Option<PathId>>,

    /// Round-robin index for load balancing
    rr_index: AtomicU64,
}

/// Statistics for path pool
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathPoolStats {
    /// Total failovers performed
    pub failovers: u64,

    /// Successful failovers
    pub successful_failovers: u64,

    /// Failed failovers
    pub failed_failovers: u64,

    /// Total paths added
    pub paths_added: u64,

    /// Total paths removed
    pub paths_removed: u64,

    /// Total bytes sent across all paths
    pub total_bytes_sent: u64,

    /// Total bytes received across all paths
    pub total_bytes_received: u64,

    /// Average failover time in milliseconds
    pub avg_failover_time_ms: u64,
}

impl PathPool {
    /// Create a new path pool for a peer
    pub fn new(peer_id: [u8; 16], config: PathPoolConfig) -> Self {
        Self {
            peer_id,
            config,
            paths: RwLock::new(HashMap::new()),
            stats: RwLock::new(PathPoolStats::default()),
            primary_path_id: Mutex::new(None),
            rr_index: AtomicU64::new(0),
        }
    }

    /// Create with default configuration
    pub fn default_pool(peer_id: [u8; 16]) -> Self {
        Self::new(peer_id, PathPoolConfig::default())
    }

    /// Add a path to the pool
    pub async fn add_path(&self, path: NetworkPath) -> Result<PathId, MultiPathError> {
        let mut paths = self.paths.write().await;

        if paths.len() >= self.config.max_paths {
            return Err(MultiPathError::MaxPathsReached {
                max: self.config.max_paths,
            });
        }

        // Check for duplicate
        for existing in paths.values() {
            if existing.remote_address == path.remote_address {
                return Err(MultiPathError::PathAlreadyExists {
                    remote_address: path.remote_address.clone(),
                });
            }
        }

        let path_id = path.id;
        let is_primary = path.is_primary();
        paths.insert(path_id, Arc::new(path));

        // Update primary if needed
        if is_primary {
            let mut primary = self.primary_path_id.lock().await;
            *primary = Some(path_id);
        } else if paths.len() == 1 {
            // First path becomes primary
            let mut primary = self.primary_path_id.lock().await;
            *primary = Some(path_id);
            if let Some(p) = paths.get(&path_id) {
                p.set_primary(true);
                p.set_state(PathState::Active).await;
            }
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.paths_added += 1;

        Ok(path_id)
    }

    /// Remove a path from the pool
    pub async fn remove_path(&self, path_id: PathId) -> Result<(), MultiPathError> {
        let mut paths = self.paths.write().await;

        if paths.remove(&path_id).is_none() {
            return Err(MultiPathError::PathNotFound { path_id });
        }

        // Update primary if removed
        let mut primary = self.primary_path_id.lock().await;
        if *primary == Some(path_id) {
            // Find a new primary
            *primary = paths.keys().next().copied();
            if let Some(new_primary_id) = *primary {
                if let Some(p) = paths.get(&new_primary_id) {
                    p.set_primary(true);
                    p.set_state(PathState::Active).await;
                }
            }
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.paths_removed += 1;

        Ok(())
    }

    /// Get a path by ID
    pub async fn get_path(&self, path_id: PathId) -> Option<Arc<NetworkPath>> {
        self.paths.read().await.get(&path_id).cloned()
    }

    /// Get the primary path
    pub async fn primary_path(&self) -> Option<Arc<NetworkPath>> {
        let primary_id = *self.primary_path_id.lock().await;
        if let Some(id) = primary_id {
            self.paths.read().await.get(&id).cloned()
        } else {
            None
        }
    }

    /// Get all paths
    pub async fn all_paths(&self) -> Vec<Arc<NetworkPath>> {
        self.paths.read().await.values().cloned().collect()
    }

    /// Get all healthy paths
    pub async fn healthy_paths(&self) -> Vec<Arc<NetworkPath>> {
        let paths = self.paths.read().await;
        let mut healthy = Vec::new();
        for path in paths.values() {
            if path.is_healthy().await {
                healthy.push(path.clone());
            }
        }
        healthy
    }

    /// Get number of paths
    pub async fn path_count(&self) -> usize {
        self.paths.read().await.len()
    }

    /// Get number of healthy paths
    pub async fn healthy_path_count(&self) -> usize {
        self.healthy_paths().await.len()
    }

    /// Get statistics
    pub async fn stats(&self) -> PathPoolStats {
        self.stats.read().await.clone()
    }

    /// Get next path for round-robin
    pub async fn next_rr_path(&self) -> Option<Arc<NetworkPath>> {
        let healthy = self.healthy_paths().await;
        if healthy.is_empty() {
            return None;
        }

        let index = self.rr_index.fetch_add(1, Ordering::Relaxed) as usize;
        Some(healthy[index % healthy.len()].clone())
    }

    /// Set primary path
    pub async fn set_primary(&self, path_id: PathId) -> Result<(), MultiPathError> {
        let paths = self.paths.read().await;

        // Verify path exists
        if !paths.contains_key(&path_id) {
            return Err(MultiPathError::PathNotFound { path_id });
        }

        // Update old primary
        let mut primary = self.primary_path_id.lock().await;
        if let Some(old_id) = *primary {
            if let Some(old_path) = paths.get(&old_id) {
                old_path.set_primary(false);
                old_path.set_state(PathState::Standby).await;
            }
        }

        // Set new primary
        if let Some(new_path) = paths.get(&path_id) {
            new_path.set_primary(true);
            new_path.set_state(PathState::Active).await;
        }

        *primary = Some(path_id);
        Ok(())
    }
}

// =============================================================================
// Path Failover
// =============================================================================

/// Failover controller for automatic path switching
pub struct PathFailover {
    /// Path pool reference
    pool: Arc<PathPool>,

    /// Consecutive failure count per path
    failure_counts: RwLock<HashMap<PathId, u32>>,

    /// Failover in progress
    failover_in_progress: AtomicBool,

    /// Last failover timestamp
    last_failover_ms: AtomicU64,
}

impl PathFailover {
    /// Create a new failover controller
    pub fn new(pool: Arc<PathPool>) -> Self {
        Self {
            pool,
            failure_counts: RwLock::new(HashMap::new()),
            failover_in_progress: AtomicBool::new(false),
            last_failover_ms: AtomicU64::new(0),
        }
    }

    /// Record a path failure and trigger failover if needed
    pub async fn record_failure(&self, path_id: PathId) -> Result<Option<PathId>, MultiPathError> {
        // Increment failure count
        let should_failover = {
            let mut counts = self.failure_counts.write().await;
            let count = counts.entry(path_id).or_insert(0);
            *count += 1;
            *count >= self.pool.config.failover_threshold
        };

        if should_failover {
            self.trigger_failover(path_id).await
        } else {
            Ok(None)
        }
    }

    /// Record a path success (resets failure count)
    pub async fn record_success(&self, path_id: PathId) {
        let mut counts = self.failure_counts.write().await;
        counts.insert(path_id, 0);
    }

    /// Manually trigger failover from a path
    pub async fn trigger_failover(
        &self,
        failed_path_id: PathId,
    ) -> Result<Option<PathId>, MultiPathError> {
        // Check if already failing over
        if self.failover_in_progress.swap(true, Ordering::SeqCst) {
            return Ok(None); // Already in progress
        }

        let start = Instant::now();
        let result = self.do_failover(failed_path_id).await;

        // Update stats
        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.last_failover_ms.store(elapsed_ms, Ordering::Relaxed);

        let mut stats = self.pool.stats.write().await;
        stats.failovers += 1;

        match &result {
            Ok(Some(_)) => {
                stats.successful_failovers += 1;
                // Update average failover time
                if stats.successful_failovers == 1 {
                    stats.avg_failover_time_ms = elapsed_ms;
                } else {
                    stats.avg_failover_time_ms = (stats.avg_failover_time_ms * 7 + elapsed_ms) / 8;
                }
            }
            Ok(None) => {}
            Err(_) => {
                stats.failed_failovers += 1;
            }
        }

        self.failover_in_progress.store(false, Ordering::SeqCst);
        result
    }

    /// Perform the actual failover
    async fn do_failover(&self, failed_path_id: PathId) -> Result<Option<PathId>, MultiPathError> {
        // Mark failed path
        if let Some(failed_path) = self.pool.get_path(failed_path_id).await {
            failed_path.set_state(PathState::Failed).await;
        }

        // Find best backup path
        let healthy = self.pool.healthy_paths().await;
        let backup = healthy
            .iter()
            .filter(|p| p.id != failed_path_id)
            .min_by_key(|p| p.priority());

        if let Some(new_primary) = backup {
            let new_id = new_primary.id;
            self.pool.set_primary(new_id).await?;
            Ok(Some(new_id))
        } else {
            Err(MultiPathError::FailoverFailed)
        }
    }

    /// Check if failover is in progress
    pub fn is_failover_in_progress(&self) -> bool {
        self.failover_in_progress.load(Ordering::Relaxed)
    }

    /// Get last failover time in milliseconds
    pub fn last_failover_time_ms(&self) -> u64 {
        self.last_failover_ms.load(Ordering::Relaxed)
    }
}

// =============================================================================
// Multi-Path Policy
// =============================================================================

/// Policy for selecting paths
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiPathPolicy {
    /// Use round-robin across all healthy paths
    RoundRobin,

    /// Weighted random selection based on path weights
    WeightedRandom,

    /// Always select the path with lowest latency
    LeastLatency,

    /// Select path with highest available bandwidth
    Bandwidth,

    /// Use primary path only (no load balancing)
    PrimaryOnly,

    /// Composite score based on multiple factors
    CompositeScore,
}

impl Default for MultiPathPolicy {
    fn default() -> Self {
        Self::LeastLatency
    }
}

// =============================================================================
// Path Selector
// =============================================================================

/// Selects paths based on configured policy
pub struct PathSelector {
    /// Selection policy
    policy: MultiPathPolicy,

    /// Random state for weighted selection
    random_state: AtomicU64,
}

impl PathSelector {
    /// Create a new path selector
    pub fn new(policy: MultiPathPolicy) -> Self {
        Self {
            policy,
            random_state: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64,
            ),
        }
    }

    /// Select a path from the pool
    pub async fn select(&self, pool: &PathPool) -> Option<Arc<NetworkPath>> {
        match self.policy {
            MultiPathPolicy::RoundRobin => pool.next_rr_path().await,
            MultiPathPolicy::WeightedRandom => self.weighted_random(pool).await,
            MultiPathPolicy::LeastLatency => self.least_latency(pool).await,
            MultiPathPolicy::Bandwidth => self.highest_bandwidth(pool).await,
            MultiPathPolicy::PrimaryOnly => pool.primary_path().await,
            MultiPathPolicy::CompositeScore => self.composite_score(pool).await,
        }
    }

    /// Weighted random selection
    async fn weighted_random(&self, pool: &PathPool) -> Option<Arc<NetworkPath>> {
        let healthy = pool.healthy_paths().await;
        if healthy.is_empty() {
            return None;
        }

        let total_weight: u64 = healthy.iter().map(|p| p.weight() as u64).sum();
        if total_weight == 0 {
            return healthy.first().cloned();
        }

        // Simple LCG random
        let old = self.random_state.load(Ordering::Relaxed);
        let new = old.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.random_state.store(new, Ordering::Relaxed);

        let random_value = new % total_weight;
        let mut cumulative = 0u64;

        for path in &healthy {
            cumulative += path.weight() as u64;
            if random_value < cumulative {
                return Some(path.clone());
            }
        }

        healthy.last().cloned()
    }

    /// Select path with lowest latency
    async fn least_latency(&self, pool: &PathPool) -> Option<Arc<NetworkPath>> {
        let healthy = pool.healthy_paths().await;
        let mut best: Option<(Arc<NetworkPath>, u64)> = None;

        for path in healthy {
            let info = path.info().await;
            let latency = if info.latency_us == 0 {
                u64::MAX / 2 // Unknown latency, treat as high but not maximum
            } else {
                info.latency_us
            };

            match &best {
                None => best = Some((path, latency)),
                Some((_, best_latency)) if latency < *best_latency => {
                    best = Some((path, latency));
                }
                _ => {}
            }
        }

        best.map(|(p, _)| p)
    }

    /// Select path with highest bandwidth
    async fn highest_bandwidth(&self, pool: &PathPool) -> Option<Arc<NetworkPath>> {
        let healthy = pool.healthy_paths().await;
        let mut best: Option<(Arc<NetworkPath>, u64)> = None;

        for path in healthy {
            let info = path.info().await;

            match &best {
                None => best = Some((path, info.bandwidth_bps)),
                Some((_, best_bw)) if info.bandwidth_bps > *best_bw => {
                    best = Some((path, info.bandwidth_bps));
                }
                _ => {}
            }
        }

        best.map(|(p, _)| p)
    }

    /// Select path with best composite score
    async fn composite_score(&self, pool: &PathPool) -> Option<Arc<NetworkPath>> {
        let healthy = pool.healthy_paths().await;
        let mut best: Option<(Arc<NetworkPath>, f64)> = None;

        for path in healthy {
            let info = path.info().await;
            let score = info.score();

            match &best {
                None => best = Some((path, score)),
                Some((_, best_score)) if score > *best_score => {
                    best = Some((path, score));
                }
                _ => {}
            }
        }

        best.map(|(p, _)| p)
    }

    /// Get current policy
    pub fn policy(&self) -> MultiPathPolicy {
        self.policy
    }

    /// Set policy
    pub fn set_policy(&mut self, policy: MultiPathPolicy) {
        self.policy = policy;
    }
}

// =============================================================================
// MTU Discovery
// =============================================================================

/// MTU discovery state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtuDiscoveryState {
    /// Not started
    Idle,

    /// Currently probing
    Probing,

    /// Discovery complete
    Complete,

    /// Discovery failed
    Failed,
}

/// MTU discovery configuration
#[derive(Debug, Clone)]
pub struct MtuDiscoveryConfig {
    /// Minimum MTU to probe
    pub min_mtu: u32,

    /// Maximum MTU to probe
    pub max_mtu: u32,

    /// Probe timeout in milliseconds
    pub probe_timeout_ms: u64,

    /// Maximum probe retries per size
    pub max_retries: u32,

    /// Step size for linear probing phase
    pub step_size: u32,
}

impl Default for MtuDiscoveryConfig {
    fn default() -> Self {
        Self {
            min_mtu: MIN_MTU,
            max_mtu: MAX_MTU,
            probe_timeout_ms: 1000,
            max_retries: 3,
            step_size: 64,
        }
    }
}

/// Path MTU Discovery using binary search
pub struct MtuDiscovery {
    /// Configuration
    config: MtuDiscoveryConfig,

    /// Current state
    state: Mutex<MtuDiscoveryState>,

    /// Currently probing MTU
    current_probe_mtu: AtomicU32,

    /// Known working MTU (lower bound)
    working_mtu: AtomicU32,

    /// Known failing MTU (upper bound)
    failing_mtu: AtomicU32,

    /// Probe retry count
    retry_count: AtomicU32,

    /// Discovery start time
    start_time: Mutex<Option<Instant>>,

    /// Final discovered MTU
    discovered_mtu: AtomicU32,
}

impl MtuDiscovery {
    /// Create new MTU discovery
    pub fn new(config: MtuDiscoveryConfig) -> Self {
        Self {
            state: Mutex::new(MtuDiscoveryState::Idle),
            current_probe_mtu: AtomicU32::new(config.max_mtu),
            working_mtu: AtomicU32::new(config.min_mtu),
            failing_mtu: AtomicU32::new(config.max_mtu + 1),
            retry_count: AtomicU32::new(0),
            start_time: Mutex::new(None),
            discovered_mtu: AtomicU32::new(config.min_mtu),
            config,
        }
    }

    /// Create with default config
    pub fn default_discovery() -> Self {
        Self::new(MtuDiscoveryConfig::default())
    }

    /// Start MTU discovery
    pub async fn start(&self) -> Result<(), MultiPathError> {
        let mut state = self.state.lock().await;
        if *state == MtuDiscoveryState::Probing {
            return Ok(()); // Already probing
        }

        *state = MtuDiscoveryState::Probing;
        self.working_mtu
            .store(self.config.min_mtu, Ordering::Relaxed);
        self.failing_mtu
            .store(self.config.max_mtu + 1, Ordering::Relaxed);
        self.retry_count.store(0, Ordering::Relaxed);

        // Start with max MTU
        self.current_probe_mtu
            .store(self.config.max_mtu, Ordering::Relaxed);

        *self.start_time.lock().await = Some(Instant::now());

        Ok(())
    }

    /// Get the next MTU to probe
    pub fn next_probe_mtu(&self) -> Option<u32> {
        let working = self.working_mtu.load(Ordering::Relaxed);
        let failing = self.failing_mtu.load(Ordering::Relaxed);

        if failing <= working + 1 {
            return None; // Discovery complete
        }

        Some(working + (failing - working) / 2)
    }

    /// Record probe success
    pub async fn record_probe_success(&self, mtu: u32) {
        self.working_mtu.store(mtu, Ordering::Relaxed);
        self.retry_count.store(0, Ordering::Relaxed);

        // Check if complete
        let failing = self.failing_mtu.load(Ordering::Relaxed);
        if failing <= mtu + 1 {
            self.complete_discovery(mtu).await;
        } else {
            // Set next probe
            let next = self.next_probe_mtu();
            if let Some(next_mtu) = next {
                self.current_probe_mtu.store(next_mtu, Ordering::Relaxed);
            }
        }
    }

    /// Record probe failure
    pub async fn record_probe_failure(&self, mtu: u32) {
        let retries = self.retry_count.fetch_add(1, Ordering::Relaxed);

        if retries + 1 >= self.config.max_retries {
            // Mark this MTU as failing
            let current_failing = self.failing_mtu.load(Ordering::Relaxed);
            if mtu < current_failing {
                self.failing_mtu.store(mtu, Ordering::Relaxed);
            }
            self.retry_count.store(0, Ordering::Relaxed);

            // Check if complete
            let working = self.working_mtu.load(Ordering::Relaxed);
            if mtu <= working + 1 {
                self.complete_discovery(working).await;
            } else {
                // Set next probe (binary search lower)
                let next = self.next_probe_mtu();
                if let Some(next_mtu) = next {
                    self.current_probe_mtu.store(next_mtu, Ordering::Relaxed);
                }
            }
        }
    }

    /// Complete the discovery
    async fn complete_discovery(&self, mtu: u32) {
        *self.state.lock().await = MtuDiscoveryState::Complete;
        self.discovered_mtu.store(mtu, Ordering::Relaxed);
    }

    /// Get current state
    pub async fn state(&self) -> MtuDiscoveryState {
        *self.state.lock().await
    }

    /// Get current probe MTU
    pub fn current_probe_mtu(&self) -> u32 {
        self.current_probe_mtu.load(Ordering::Relaxed)
    }

    /// Get discovered MTU (valid only when complete)
    pub fn discovered_mtu(&self) -> u32 {
        self.discovered_mtu.load(Ordering::Relaxed)
    }

    /// Get working (known good) MTU
    pub fn working_mtu(&self) -> u32 {
        self.working_mtu.load(Ordering::Relaxed)
    }

    /// Get elapsed time since start
    pub async fn elapsed(&self) -> Option<Duration> {
        self.start_time.lock().await.map(|t| t.elapsed())
    }

    /// Reset discovery state
    pub async fn reset(&self) {
        *self.state.lock().await = MtuDiscoveryState::Idle;
        self.working_mtu
            .store(self.config.min_mtu, Ordering::Relaxed);
        self.failing_mtu
            .store(self.config.max_mtu + 1, Ordering::Relaxed);
        self.retry_count.store(0, Ordering::Relaxed);
        self.discovered_mtu
            .store(self.config.min_mtu, Ordering::Relaxed);
        *self.start_time.lock().await = None;
    }
}

// =============================================================================
// Shared Types
// =============================================================================

/// Thread-safe path pool
pub type SharedPathPool = Arc<PathPool>;

/// Create a shared path pool
pub fn shared_path_pool(peer_id: [u8; 16], config: PathPoolConfig) -> SharedPathPool {
    Arc::new(PathPool::new(peer_id, config))
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // PathId tests

    #[test]
    fn test_path_id_generation() {
        let id1 = PathId::generate();
        let id2 = PathId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_path_id_from_raw() {
        let id = PathId::from_raw(12345);
        assert_eq!(id.raw(), 12345);
    }

    #[test]
    fn test_path_id_display() {
        let id = PathId::from_raw(42);
        assert_eq!(format!("{}", id), "path-42");
    }

    // PathInfo tests

    #[test]
    fn test_path_info_reliability() {
        let mut info = PathInfo::default();
        assert_eq!(info.reliability(), 0.5); // Unknown

        info.successful_sends = 90;
        info.failed_sends = 10;
        assert!((info.reliability() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_path_info_score() {
        let info = PathInfo {
            latency_us: 10_000,         // 10ms - good
            bandwidth_bps: 100_000_000, // 100Mbps - good
            successful_sends: 95,
            failed_sends: 5,
            ..Default::default()
        };

        let score = info.score();
        assert!(score > 0.5); // Should be high with good metrics
    }

    #[test]
    fn test_path_info_health() {
        let config = PathHealthConfig::default();

        let healthy_info = PathInfo {
            latency_us: 50_000,
            loss_rate: 0.01,
            ..Default::default()
        };
        assert!(healthy_info.is_healthy(&config));

        let unhealthy_info = PathInfo {
            latency_us: 1_000_000, // Too high
            loss_rate: 0.01,
            ..Default::default()
        };
        assert!(!unhealthy_info.is_healthy(&config));
    }

    // NetworkPath tests

    #[tokio::test]
    async fn test_network_path_creation() {
        let path = NetworkPath::new("192.168.1.1:8080".to_string(), true);

        assert!(path.is_primary());
        assert_eq!(path.state().await, PathState::Active);
        assert_eq!(path.weight(), 100);
    }

    #[tokio::test]
    async fn test_network_path_rtt_update() {
        let path = NetworkPath::new("10.0.0.1:1234".to_string(), false);

        // First sample
        path.update_rtt(10_000).await;
        let info = path.info().await;
        assert_eq!(info.latency_us, 10_000);

        // Second sample (should be smoothed)
        path.update_rtt(20_000).await;
        let info = path.info().await;
        assert!(info.latency_us > 10_000 && info.latency_us < 20_000);
    }

    #[tokio::test]
    async fn test_network_path_record_success() {
        let path = NetworkPath::new("10.0.0.1:1234".to_string(), false);

        path.record_success(1000).await;
        let info = path.info().await;

        assert_eq!(info.successful_sends, 1);
        assert_eq!(info.bytes_sent, 1000);
        assert!(info.last_success_ms > 0);
    }

    #[tokio::test]
    async fn test_network_path_record_failure() {
        let path = NetworkPath::new("10.0.0.1:1234".to_string(), false);

        path.record_failure().await;
        let info = path.info().await;

        assert_eq!(info.failed_sends, 1);
        assert_eq!(info.loss_rate, 1.0);
    }

    #[tokio::test]
    async fn test_network_path_mtu() {
        let path = NetworkPath::new("10.0.0.1:1234".to_string(), false);

        assert_eq!(path.mtu().await, DEFAULT_MTU);

        path.set_mtu(1500).await.expect("Valid MTU");
        assert_eq!(path.mtu().await, 1500);

        // Invalid MTU
        let result = path.set_mtu(100).await;
        assert!(result.is_err());
    }

    // PathPool tests

    #[tokio::test]
    async fn test_path_pool_creation() {
        let pool = PathPool::default_pool([1u8; 16]);
        assert_eq!(pool.path_count().await, 0);
    }

    #[tokio::test]
    async fn test_path_pool_add_path() {
        let pool = PathPool::default_pool([1u8; 16]);

        let path = NetworkPath::new("192.168.1.1:8080".to_string(), true);
        let path_id = pool.add_path(path).await.expect("Should add path");

        assert_eq!(pool.path_count().await, 1);
        assert!(pool.get_path(path_id).await.is_some());
    }

    #[tokio::test]
    async fn test_path_pool_remove_path() {
        let pool = PathPool::default_pool([1u8; 16]);

        let path = NetworkPath::new("192.168.1.1:8080".to_string(), true);
        let path_id = pool.add_path(path).await.expect("Should add path");

        pool.remove_path(path_id).await.expect("Should remove path");
        assert_eq!(pool.path_count().await, 0);
    }

    #[tokio::test]
    async fn test_path_pool_max_paths() {
        let config = PathPoolConfig {
            max_paths: 2,
            ..Default::default()
        };
        let pool = PathPool::new([1u8; 16], config);

        pool.add_path(NetworkPath::new("10.0.0.1:1234".to_string(), true))
            .await
            .expect("First path");
        pool.add_path(NetworkPath::new("10.0.0.2:1234".to_string(), false))
            .await
            .expect("Second path");

        let result = pool
            .add_path(NetworkPath::new("10.0.0.3:1234".to_string(), false))
            .await;
        assert!(matches!(
            result,
            Err(MultiPathError::MaxPathsReached { .. })
        ));
    }

    #[tokio::test]
    async fn test_path_pool_duplicate_path() {
        let pool = PathPool::default_pool([1u8; 16]);

        pool.add_path(NetworkPath::new("10.0.0.1:1234".to_string(), true))
            .await
            .expect("First path");

        let result = pool
            .add_path(NetworkPath::new("10.0.0.1:1234".to_string(), false))
            .await;
        assert!(matches!(
            result,
            Err(MultiPathError::PathAlreadyExists { .. })
        ));
    }

    #[tokio::test]
    async fn test_path_pool_primary_path() {
        let pool = PathPool::default_pool([1u8; 16]);

        let path1 = NetworkPath::new("10.0.0.1:1234".to_string(), false);
        let id1 = pool.add_path(path1).await.expect("First path");

        // First path (non-primary) becomes primary since it's the only one
        let primary = pool.primary_path().await;
        assert!(primary.is_some());
        assert_eq!(primary.as_ref().map(|p| p.id), Some(id1));

        // Add second path as explicit primary
        let path2 = NetworkPath::new("10.0.0.2:1234".to_string(), true);
        let id2 = pool.add_path(path2).await.expect("Second path");

        // Second path should now be primary since it was added with is_primary=true
        let primary = pool.primary_path().await;
        assert!(primary.is_some());
        assert_eq!(primary.as_ref().map(|p| p.id), Some(id2));
    }

    // PathFailover tests

    #[tokio::test]
    async fn test_path_failover_record_success() {
        let pool = Arc::new(PathPool::default_pool([1u8; 16]));
        let failover = PathFailover::new(pool.clone());

        let path = NetworkPath::new("10.0.0.1:1234".to_string(), true);
        let path_id = pool.add_path(path).await.expect("Add path");

        failover.record_success(path_id).await;
        // No error expected
    }

    #[tokio::test]
    async fn test_path_failover_trigger() {
        let config = PathPoolConfig {
            failover_threshold: 2,
            ..Default::default()
        };
        let pool = Arc::new(PathPool::new([1u8; 16], config));
        let failover = PathFailover::new(pool.clone());

        let path1 = NetworkPath::new("10.0.0.1:1234".to_string(), true);
        pool.add_path(path1).await.expect("Add path 1");

        let path2 = NetworkPath::new("10.0.0.2:1234".to_string(), false);
        let path2_id = pool.add_path(path2).await.expect("Add path 2");

        // Get primary path ID
        let primary = pool.primary_path().await.expect("Primary exists");
        let primary_id = primary.id;

        // Simulate failures
        failover.record_failure(primary_id).await.ok();
        let result = failover.record_failure(primary_id).await;

        // Should trigger failover to path2
        assert!(result.is_ok());
        if let Ok(Some(new_primary)) = result {
            assert_eq!(new_primary, path2_id);
        }
    }

    // PathSelector tests

    #[tokio::test]
    async fn test_path_selector_round_robin() {
        let pool = PathPool::default_pool([1u8; 16]);

        pool.add_path(NetworkPath::new("10.0.0.1:1234".to_string(), true))
            .await
            .expect("Path 1");
        pool.add_path(NetworkPath::new("10.0.0.2:1234".to_string(), false))
            .await
            .expect("Path 2");

        let selector = PathSelector::new(MultiPathPolicy::RoundRobin);

        let p1 = selector.select(&pool).await;
        let p2 = selector.select(&pool).await;

        assert!(p1.is_some());
        assert!(p2.is_some());
    }

    #[tokio::test]
    async fn test_path_selector_least_latency() {
        let pool = PathPool::default_pool([1u8; 16]);

        let path1 = NetworkPath::new("10.0.0.1:1234".to_string(), false);
        path1.update_rtt(100_000).await; // 100ms
        let id1 = pool.add_path(path1).await.expect("Path 1");

        let path2 = NetworkPath::new("10.0.0.2:1234".to_string(), false);
        path2.update_rtt(10_000).await; // 10ms
        let id2 = pool.add_path(path2).await.expect("Path 2");

        let selector = PathSelector::new(MultiPathPolicy::LeastLatency);
        let selected = selector.select(&pool).await;

        assert!(selected.is_some());
        // Path with lower latency should be selected
        assert_eq!(selected.map(|p| p.id), Some(id2));
        let _ = id1; // Silence warning
    }

    #[tokio::test]
    async fn test_path_selector_primary_only() {
        let pool = PathPool::default_pool([1u8; 16]);

        let path1 = NetworkPath::new("10.0.0.1:1234".to_string(), true);
        let id1 = pool.add_path(path1).await.expect("Path 1");

        pool.add_path(NetworkPath::new("10.0.0.2:1234".to_string(), false))
            .await
            .expect("Path 2");

        let selector = PathSelector::new(MultiPathPolicy::PrimaryOnly);
        let selected = selector.select(&pool).await;

        assert_eq!(selected.map(|p| p.id), Some(id1));
    }

    // MtuDiscovery tests

    #[tokio::test]
    async fn test_mtu_discovery_creation() {
        let discovery = MtuDiscovery::default_discovery();
        assert_eq!(discovery.state().await, MtuDiscoveryState::Idle);
    }

    #[tokio::test]
    async fn test_mtu_discovery_start() {
        let discovery = MtuDiscovery::default_discovery();

        discovery.start().await.expect("Start discovery");
        assert_eq!(discovery.state().await, MtuDiscoveryState::Probing);
    }

    #[tokio::test]
    async fn test_mtu_discovery_binary_search() {
        let config = MtuDiscoveryConfig {
            min_mtu: 1000,
            max_mtu: 2000,
            max_retries: 1,
            ..Default::default()
        };
        let discovery = MtuDiscovery::new(config);

        discovery.start().await.expect("Start");

        // First probe is at max (2000)
        assert_eq!(discovery.current_probe_mtu(), 2000);

        // Simulate failure at 2000
        discovery.record_probe_failure(2000).await;

        // Next probe should be binary search: (1000 + 2000) / 2 = 1500
        let next = discovery.next_probe_mtu();
        assert_eq!(next, Some(1500));
    }

    #[tokio::test]
    async fn test_mtu_discovery_success() {
        let config = MtuDiscoveryConfig {
            min_mtu: 1000,
            max_mtu: 1500,
            max_retries: 1,
            ..Default::default()
        };
        let discovery = MtuDiscovery::new(config);

        discovery.start().await.expect("Start");

        // Probe at 1500 succeeds
        discovery.record_probe_success(1500).await;

        // Should be complete with MTU 1500
        assert_eq!(discovery.state().await, MtuDiscoveryState::Complete);
        assert_eq!(discovery.discovered_mtu(), 1500);
    }

    #[tokio::test]
    async fn test_mtu_discovery_reset() {
        let discovery = MtuDiscovery::default_discovery();

        discovery.start().await.expect("Start");
        discovery.record_probe_success(1400).await;

        discovery.reset().await;

        assert_eq!(discovery.state().await, MtuDiscoveryState::Idle);
        assert_eq!(discovery.discovered_mtu(), MIN_MTU);
    }

    // Error tests

    #[test]
    fn test_multipath_error_display() {
        let err = MultiPathError::NoPathsAvailable;
        assert_eq!(err.to_string(), "No paths available for peer");

        let err = MultiPathError::PathNotFound {
            path_id: PathId::from_raw(42),
        };
        assert!(err.to_string().contains("path-42"));

        let err = MultiPathError::InvalidMtu {
            mtu: 100,
            min: 1280,
            max: 9000,
        };
        assert!(err.to_string().contains("100"));
    }

    // Config tests

    #[test]
    fn test_path_pool_config_presets() {
        let default = PathPoolConfig::default();
        let ha = PathPoolConfig::high_availability();
        let embedded = PathPoolConfig::embedded();

        assert!(ha.max_paths > default.max_paths);
        assert!(embedded.max_paths < default.max_paths);
        assert!(ha.failover_target_ms < default.failover_target_ms);
    }

    // Shared types tests

    #[test]
    fn test_shared_path_pool() {
        let pool = shared_path_pool([1u8; 16], PathPoolConfig::default());
        assert_eq!(pool.peer_id, [1u8; 16]);
    }

    // Stats tests

    #[tokio::test]
    async fn test_path_pool_stats() {
        let pool = PathPool::default_pool([1u8; 16]);

        pool.add_path(NetworkPath::new("10.0.0.1:1234".to_string(), true))
            .await
            .expect("Add path");

        let stats = pool.stats().await;
        assert_eq!(stats.paths_added, 1);
    }
}
