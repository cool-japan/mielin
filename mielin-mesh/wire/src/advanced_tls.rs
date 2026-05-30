//! Advanced TLS Security Features
//!
//! Provides certificate pinning (HPKP-style), mutual TLS chain verification with
//! policy enforcement, and connection health monitoring with heartbeat-based RTT
//! tracking. All types integrate cleanly with the existing `security.rs` and
//! `cert_rotation.rs` infrastructure.

use ring::digest::{digest, SHA256, SHA384, SHA512};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Pin errors
// ---------------------------------------------------------------------------

/// Errors produced by certificate pinning
#[derive(Debug, Error)]
pub enum PinError {
    #[error("No pins configured for host {hostname} and unpinned connections are not allowed")]
    NoPinsForHost { hostname: String },
    #[error("Certificate fingerprint mismatch for host {hostname}")]
    FingerprintMismatch { hostname: String },
    #[error("All pins expired for host {hostname}")]
    AllPinsExpired { hostname: String },
}

// ---------------------------------------------------------------------------
// Chain errors
// ---------------------------------------------------------------------------

/// Errors produced by chain verification
#[derive(Debug, Error)]
pub enum ChainError {
    #[error("Chain too deep: depth {depth} exceeds max {max}")]
    TooDeep { depth: usize, max: usize },
    #[error("Pin verification failed: {0}")]
    PinFailed(#[from] PinError),
    #[error("Certificate chain is empty")]
    EmptyChain,
}

// ---------------------------------------------------------------------------
// Health monitor errors
// ---------------------------------------------------------------------------

/// Errors produced by the connection health monitor
#[derive(Debug, Error)]
pub enum HealthMonitorError {
    #[error("Connection not found: {node_id}")]
    ConnectionNotFound { node_id: String },
    #[error("Lock poisoned")]
    LockPoisoned,
}

// ---------------------------------------------------------------------------
// PinAlgorithm
// ---------------------------------------------------------------------------

/// Hashing algorithm used to compute a certificate pin fingerprint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl PinAlgorithm {
    /// Digest raw bytes with the selected algorithm
    fn digest_bytes(self, data: &[u8]) -> Vec<u8> {
        match self {
            PinAlgorithm::Sha256 => digest(&SHA256, data).as_ref().to_vec(),
            PinAlgorithm::Sha384 => digest(&SHA384, data).as_ref().to_vec(),
            PinAlgorithm::Sha512 => digest(&SHA512, data).as_ref().to_vec(),
        }
    }
}

// ---------------------------------------------------------------------------
// CertPin
// ---------------------------------------------------------------------------

/// A certificate pin (HPKP-style) applied to raw DER-encoded certificate bytes.
///
/// The fingerprint is compared against the hash of the full DER bytes of the
/// leaf certificate. Callers can optionally supply an expiry so that rotating
/// pins is possible without downtime.
#[derive(Debug, Clone)]
pub struct CertPin {
    /// Raw hash bytes (length determined by `algorithm`)
    pub fingerprint: Vec<u8>,
    /// Algorithm used to compute `fingerprint`
    pub algorithm: PinAlgorithm,
    /// Wall-clock time at which this pin was created
    pub created_at: Instant,
    /// Optional expiry deadline; `None` means the pin never expires
    pub expires_at: Option<Instant>,
    /// Human-readable label for diagnostics
    pub label: String,
}

impl CertPin {
    /// Create a new SHA-256 pin from a pre-computed 32-byte fingerprint.
    pub fn new_sha256(fingerprint: [u8; 32], label: impl Into<String>) -> Self {
        Self {
            fingerprint: fingerprint.to_vec(),
            algorithm: PinAlgorithm::Sha256,
            created_at: Instant::now(),
            expires_at: None,
            label: label.into(),
        }
    }

    /// Create a SHA-384 pin from a pre-computed 48-byte fingerprint.
    pub fn new_sha384(fingerprint: [u8; 48], label: impl Into<String>) -> Self {
        Self {
            fingerprint: fingerprint.to_vec(),
            algorithm: PinAlgorithm::Sha384,
            created_at: Instant::now(),
            expires_at: None,
            label: label.into(),
        }
    }

    /// Create a SHA-512 pin from a pre-computed 64-byte fingerprint.
    pub fn new_sha512(fingerprint: [u8; 64], label: impl Into<String>) -> Self {
        Self {
            fingerprint: fingerprint.to_vec(),
            algorithm: PinAlgorithm::Sha512,
            created_at: Instant::now(),
            expires_at: None,
            label: label.into(),
        }
    }

    /// Builder-style expiry attachment.
    pub fn with_expiry(mut self, duration: Duration) -> Self {
        self.expires_at = Some(self.created_at + duration);
        self
    }

    /// Returns `true` if the pin has passed its expiry deadline.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            None => false,
            Some(deadline) => Instant::now() >= deadline,
        }
    }

    /// Verify a DER-encoded certificate against this pin.
    ///
    /// Hashes the raw DER bytes with `self.algorithm` and compares the result
    /// to `self.fingerprint` in constant time via element-wise XOR accumulation.
    pub fn verify_der(&self, cert_der: &[u8]) -> bool {
        let computed = self.algorithm.digest_bytes(cert_der);
        if computed.len() != self.fingerprint.len() {
            return false;
        }
        // Constant-time equality: accumulate XOR differences and check all-zero
        let diff: u8 = computed
            .iter()
            .zip(self.fingerprint.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b));
        diff == 0
    }
}

// ---------------------------------------------------------------------------
// PinVerification
// ---------------------------------------------------------------------------

/// Outcome of a pin store lookup
#[derive(Debug)]
pub enum PinVerification {
    /// The certificate matched at least one active pin for the hostname
    Pinned,
    /// No pins are registered for this hostname (permitted when `allow_unpinned`)
    Unpinned,
    /// Pins exist for the hostname but they have all expired
    NoPinsLeft,
}

// ---------------------------------------------------------------------------
// CertPinStore
// ---------------------------------------------------------------------------

/// Thread-safe store mapping hostname patterns to lists of expected pins.
///
/// Pins are matched by exact hostname key. If `allow_unpinned` is `false` any
/// connection whose hostname has no entry in the store will be rejected.
pub struct CertPinStore {
    pins: RwLock<HashMap<String, Vec<CertPin>>>,
    /// When `false`, reject connections for hosts that have no pins registered
    allow_unpinned: bool,
}

impl CertPinStore {
    /// Create a new pin store.
    ///
    /// `allow_unpinned` controls whether hosts with no registered pins are
    /// allowed (`true`) or rejected (`false`).
    pub fn new(allow_unpinned: bool) -> Self {
        Self {
            pins: RwLock::new(HashMap::new()),
            allow_unpinned,
        }
    }

    /// Register a pin for the given hostname.
    pub fn add_pin(&self, hostname: &str, pin: CertPin) {
        let mut guard = match self.pins.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.entry(hostname.to_string()).or_default().push(pin);
    }

    /// Discard all pins that have passed their expiry deadline.
    pub fn remove_expired(&self) {
        let mut guard = match self.pins.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        for pins in guard.values_mut() {
            pins.retain(|p| !p.is_expired());
        }
        // Also remove hostname entries that became completely empty
        guard.retain(|_, v| !v.is_empty());
    }

    /// Verify a DER-encoded certificate for `hostname`.
    ///
    /// Returns:
    /// - `Ok(PinVerification::Pinned)` — at least one active pin matched
    /// - `Ok(PinVerification::Unpinned)` — no pins for hostname and `allow_unpinned` is `true`
    /// - `Ok(PinVerification::NoPinsLeft)` — all pins for hostname have expired
    /// - `Err(PinError::NoPinsForHost)` — no pins and `allow_unpinned` is `false`
    /// - `Err(PinError::FingerprintMismatch)` — pins exist, none matched
    pub fn verify(&self, hostname: &str, cert_der: &[u8]) -> Result<PinVerification, PinError> {
        let guard = match self.pins.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let Some(pins) = guard.get(hostname) else {
            if self.allow_unpinned {
                return Ok(PinVerification::Unpinned);
            }
            return Err(PinError::NoPinsForHost {
                hostname: hostname.to_string(),
            });
        };

        let active: Vec<&CertPin> = pins.iter().filter(|p| !p.is_expired()).collect();
        if active.is_empty() {
            return Ok(PinVerification::NoPinsLeft);
        }

        for pin in &active {
            if pin.verify_der(cert_der) {
                return Ok(PinVerification::Pinned);
            }
        }

        Err(PinError::FingerprintMismatch {
            hostname: hostname.to_string(),
        })
    }

    /// Total number of pins across all hostnames (including expired ones)
    pub fn pin_count(&self) -> usize {
        let guard = match self.pins.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.values().map(|v| v.len()).sum()
    }

    /// Number of distinct hostnames that have at least one pin registered
    pub fn hostname_count(&self) -> usize {
        let guard = match self.pins.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.len()
    }
}

// ---------------------------------------------------------------------------
// AllowedKeyAlgorithm
// ---------------------------------------------------------------------------

/// Key algorithm allow-list for certificate chain policy enforcement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedKeyAlgorithm {
    Ed25519,
    EcdsaP256,
    EcdsaP384,
    Rsa2048Plus,
}

// ---------------------------------------------------------------------------
// ChainVerification
// ---------------------------------------------------------------------------

/// Result of a successful certificate chain verification
#[derive(Debug)]
pub struct ChainVerification {
    /// Number of certificates in the chain
    pub depth: usize,
    /// Whether the leaf certificate matched a registered pin
    pub pinned: bool,
    /// String representation of the leaf subject (DER hex prefix for dummy certs)
    pub leaf_subject: String,
    /// SHA-256 fingerprint of the leaf certificate DER bytes
    pub leaf_fingerprint: Vec<u8>,
}

// ---------------------------------------------------------------------------
// CertChainVerifier
// ---------------------------------------------------------------------------

/// Certificate chain verifier with configurable policy enforcement.
///
/// The `verify_chain` method performs structural checks (chain depth, minimum
/// entries) and, if a `CertPinStore` is configured, delegates pin verification
/// for the hostname to it.
pub struct CertChainVerifier {
    /// Maximum allowed chain depth (default 5)
    max_depth: usize,
    /// Require Subject Alternative Name extension (policy field, future integration)
    require_san: bool,
    /// Permitted key algorithms (policy field, future integration)
    allowed_key_algs: Vec<AllowedKeyAlgorithm>,
    /// Minimum RSA key size in bits (policy field, future integration)
    min_key_bits: usize,
    /// Optional pin store for HPKP-style verification of the leaf cert
    pin_store: Option<Arc<CertPinStore>>,
}

impl Default for CertChainVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl CertChainVerifier {
    /// Create a verifier with sane defaults:
    /// - `max_depth` = 5
    /// - `require_san` = true
    /// - `min_key_bits` = 2048
    /// - All key algorithms permitted
    pub fn new() -> Self {
        Self {
            max_depth: 5,
            require_san: true,
            allowed_key_algs: vec![
                AllowedKeyAlgorithm::Ed25519,
                AllowedKeyAlgorithm::EcdsaP256,
                AllowedKeyAlgorithm::EcdsaP384,
                AllowedKeyAlgorithm::Rsa2048Plus,
            ],
            min_key_bits: 2048,
            pin_store: None,
        }
    }

    /// Override the maximum chain depth.
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Attach a pin store for HPKP-style leaf-cert verification.
    pub fn with_pin_store(mut self, store: Arc<CertPinStore>) -> Self {
        self.pin_store = Some(store);
        self
    }

    /// Append a permitted key algorithm to the allow-list.
    pub fn with_key_algorithm(mut self, alg: AllowedKeyAlgorithm) -> Self {
        if !self.allowed_key_algs.contains(&alg) {
            self.allowed_key_algs.push(alg);
        }
        self
    }

    /// Set whether to require SANs.
    pub fn require_san(mut self, required: bool) -> Self {
        self.require_san = required;
        self
    }

    /// Set the minimum RSA key size.
    pub fn with_min_key_bits(mut self, bits: usize) -> Self {
        self.min_key_bits = bits;
        self
    }

    /// Verify a certificate chain given as a slice of DER-encoded certificates.
    ///
    /// `chain[0]` is the leaf; `chain[last]` is the root/CA.
    /// Performs:
    /// 1. Empty-chain guard
    /// 2. Depth guard (chain length vs `max_depth`)
    /// 3. Optional pin verification of the leaf cert
    ///
    /// Returns a `ChainVerification` on success.
    pub fn verify_chain(
        &self,
        chain: &[Vec<u8>],
        hostname: &str,
    ) -> Result<ChainVerification, ChainError> {
        if chain.is_empty() {
            return Err(ChainError::EmptyChain);
        }
        let depth = chain.len();
        if depth > self.max_depth {
            return Err(ChainError::TooDeep {
                depth,
                max: self.max_depth,
            });
        }

        let leaf = &chain[0];

        // Pin verification (if a store is configured)
        let pinned = if let Some(store) = &self.pin_store {
            match store.verify(hostname, leaf) {
                Ok(PinVerification::Pinned) => true,
                Ok(PinVerification::Unpinned) | Ok(PinVerification::NoPinsLeft) => false,
                Err(e) => return Err(ChainError::PinFailed(e)),
            }
        } else {
            false
        };

        // Compute leaf fingerprint (SHA-256 of DER bytes)
        let leaf_fingerprint = digest(&SHA256, leaf).as_ref().to_vec();

        // Derive a best-effort subject string from the DER prefix (hex of first 16 bytes)
        let preview_len = leaf.len().min(16);
        let leaf_subject = format!(
            "DER[{}..]:0x{}",
            leaf.len(),
            hex::encode(&leaf[..preview_len])
        );

        Ok(ChainVerification {
            depth,
            pinned,
            leaf_subject,
            leaf_fingerprint,
        })
    }
}

// ---------------------------------------------------------------------------
// HeartbeatConfig
// ---------------------------------------------------------------------------

/// Configuration governing heartbeat behaviour for a single connection
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// How often a ping should be sent (default 30 s)
    pub interval: Duration,
    /// Maximum time to wait for a corresponding pong (default 10 s)
    pub timeout: Duration,
    /// Consecutive missed pongs before the connection is marked `Unhealthy`
    pub max_missed: u32,
    /// Window size for the RTT moving-average (number of samples to retain)
    pub rtt_window: usize,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(10),
            max_missed: 3,
            rtt_window: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// ConnectionHealthStatus
// ---------------------------------------------------------------------------

/// Health status of a single peer connection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionHealthStatus {
    /// All heartbeats returning within acceptable latency
    Healthy,
    /// Some missed pings or unusually high latency
    Degraded,
    /// `max_missed` consecutive pongs not received
    Unhealthy,
    /// Not enough data to determine health (initial state)
    Unknown,
}

// ---------------------------------------------------------------------------
// ConnectionHealth
// ---------------------------------------------------------------------------

/// Per-connection health tracking state
#[derive(Debug)]
pub struct ConnectionHealth {
    /// Identifier of the remote peer
    pub node_id: String,
    /// Current assessed health status
    pub status: ConnectionHealthStatus,
    /// Sliding window of recent RTT samples
    pub rtt_samples: VecDeque<Duration>,
    /// Number of consecutive pings that received no pong
    pub missed_pings: u32,
    /// Monotonic instant of the most recent ping sent
    pub last_ping_sent: Option<Instant>,
    /// Monotonic instant of the most recent pong received
    pub last_pong_received: Option<Instant>,
    /// Cumulative pings sent since this connection was registered
    pub total_pings_sent: u64,
    /// Cumulative pongs received since this connection was registered
    pub total_pongs_received: u64,
}

impl ConnectionHealth {
    /// Initialise a fresh connection health tracker in the `Unknown` state.
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            status: ConnectionHealthStatus::Unknown,
            rtt_samples: VecDeque::new(),
            missed_pings: 0,
            last_ping_sent: None,
            last_pong_received: None,
            total_pings_sent: 0,
            total_pongs_received: 0,
        }
    }

    /// Record a pong with the measured RTT.
    ///
    /// Resets the missed-ping counter, appends the sample to the sliding window,
    /// and refreshes `last_pong_received`.
    pub fn record_pong(&mut self, rtt: Duration) {
        self.missed_pings = 0;
        self.last_pong_received = Some(Instant::now());
        self.total_pongs_received += 1;
        self.rtt_samples.push_back(rtt);
        // Trim to a hard cap to avoid unbounded growth before update_status is called
        while self.rtt_samples.len() > 1024 {
            self.rtt_samples.pop_front();
        }
    }

    /// Trim the RTT window to `config.rtt_window`.
    fn trim_rtt_window(&mut self, config: &HeartbeatConfig) {
        while self.rtt_samples.len() > config.rtt_window {
            self.rtt_samples.pop_front();
        }
    }

    /// Record a missed ping (i.e. the heartbeat timer fired without a pong).
    ///
    /// Increments the consecutive miss counter and trims the RTT window.
    pub fn record_missed_ping(&mut self, config: &HeartbeatConfig) {
        self.missed_pings = self.missed_pings.saturating_add(1);
        self.trim_rtt_window(config);
    }

    /// Arithmetic mean of all RTT samples in the window.
    ///
    /// Returns `None` when no samples have been collected yet.
    pub fn mean_rtt(&self) -> Option<Duration> {
        if self.rtt_samples.is_empty() {
            return None;
        }
        let total_nanos: u128 = self.rtt_samples.iter().map(|d| d.as_nanos()).sum();
        Some(Duration::from_nanos(
            (total_nanos / self.rtt_samples.len() as u128) as u64,
        ))
    }

    /// 99th-percentile RTT from the current window.
    ///
    /// Returns `None` when fewer than 2 samples have been collected.
    pub fn p99_rtt(&self) -> Option<Duration> {
        let n = self.rtt_samples.len();
        if n < 2 {
            return None;
        }
        let mut sorted: Vec<Duration> = self.rtt_samples.iter().copied().collect();
        sorted.sort_unstable();
        // Nearest-rank method: ceil(p * n) − 1 (0-indexed)
        let index = (99 * n).div_ceil(100).saturating_sub(1).min(n - 1);
        Some(sorted[index])
    }

    /// Returns `true` when the connection should be considered healthy given `config`.
    pub fn is_healthy(&self, config: &HeartbeatConfig) -> bool {
        self.missed_pings < config.max_missed
            && self.status != ConnectionHealthStatus::Unhealthy
    }

    /// Recompute and store the `status` field based on the current state.
    ///
    /// Status rules (evaluated top-down):
    /// - `Unhealthy` — `missed_pings >= config.max_missed`
    /// - `Degraded`  — `missed_pings >= 1` (but still under max)
    /// - `Healthy`   — at least one pong received and no misses
    /// - `Unknown`   — no pongs received yet
    pub fn update_status(&mut self, config: &HeartbeatConfig) {
        self.trim_rtt_window(config);
        self.status = if self.missed_pings >= config.max_missed {
            ConnectionHealthStatus::Unhealthy
        } else if self.missed_pings >= 1 {
            ConnectionHealthStatus::Degraded
        } else if self.total_pongs_received > 0 {
            ConnectionHealthStatus::Healthy
        } else {
            ConnectionHealthStatus::Unknown
        };
    }
}

// ---------------------------------------------------------------------------
// ConnectionMonitorStats
// ---------------------------------------------------------------------------

/// Aggregate statistics across all monitored connections
#[derive(Debug, Default, Clone)]
pub struct ConnectionMonitorStats {
    /// Total connections ever registered (monotonically increasing)
    pub total_connections: u64,
    /// Current number of healthy connections
    pub healthy_connections: usize,
    /// Current number of degraded connections
    pub degraded_connections: usize,
    /// Current number of unhealthy connections
    pub unhealthy_connections: usize,
    /// Total pings sent (cumulative)
    pub total_pings_sent: u64,
    /// Total pongs received (cumulative)
    pub total_pongs_received: u64,
    /// Total missed pings (cumulative)
    pub total_missed_pings: u64,
}

// ---------------------------------------------------------------------------
// ConnectionHealthMonitor
// ---------------------------------------------------------------------------

/// Thread-safe monitor that tracks heartbeat health across multiple peers.
pub struct ConnectionHealthMonitor {
    config: HeartbeatConfig,
    connections: Arc<RwLock<HashMap<String, ConnectionHealth>>>,
    total_connections: std::sync::atomic::AtomicU64,
    total_pings_sent: std::sync::atomic::AtomicU64,
    total_pongs_received: std::sync::atomic::AtomicU64,
    total_missed_pings: std::sync::atomic::AtomicU64,
}

impl ConnectionHealthMonitor {
    /// Create a new monitor with the supplied heartbeat configuration.
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            total_connections: std::sync::atomic::AtomicU64::new(0),
            total_pings_sent: std::sync::atomic::AtomicU64::new(0),
            total_pongs_received: std::sync::atomic::AtomicU64::new(0),
            total_missed_pings: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Register a new connection, initialised to `Unknown` health.
    ///
    /// If a connection with the same `node_id` already exists it is replaced.
    pub fn register_connection(&self, node_id: &str) {
        let mut guard = match self.connections.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.insert(node_id.to_string(), ConnectionHealth::new(node_id));
        self.total_connections
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Remove a connection from the monitor.
    pub fn unregister_connection(&self, node_id: &str) {
        let mut guard = match self.connections.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.remove(node_id);
    }

    /// Record a pong received from `node_id` with the given RTT.
    pub fn record_pong(&self, node_id: &str, rtt: Duration) -> Result<(), HealthMonitorError> {
        let mut guard = match self.connections.write() {
            Ok(g) => g,
            Err(_) => return Err(HealthMonitorError::LockPoisoned),
        };
        let health = guard.get_mut(node_id).ok_or_else(|| {
            HealthMonitorError::ConnectionNotFound {
                node_id: node_id.to_string(),
            }
        })?;
        health.record_pong(rtt);
        health.update_status(&self.config);
        self.total_pongs_received
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Record a missed ping for `node_id` (no pong arrived within the timeout).
    pub fn record_missed_ping(&self, node_id: &str) -> Result<(), HealthMonitorError> {
        let mut guard = match self.connections.write() {
            Ok(g) => g,
            Err(_) => return Err(HealthMonitorError::LockPoisoned),
        };
        let health = guard.get_mut(node_id).ok_or_else(|| {
            HealthMonitorError::ConnectionNotFound {
                node_id: node_id.to_string(),
            }
        })?;
        health.record_missed_ping(&self.config);
        health.update_status(&self.config);
        self.total_missed_pings
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Return a snapshot of the health state for a single connection.
    ///
    /// Returns `None` if the connection is not registered.
    pub fn connection_health(&self, node_id: &str) -> Option<ConnectionHealth> {
        let guard = match self.connections.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let h = guard.get(node_id)?;
        Some(ConnectionHealth {
            node_id: h.node_id.clone(),
            status: h.status.clone(),
            rtt_samples: h.rtt_samples.clone(),
            missed_pings: h.missed_pings,
            last_ping_sent: h.last_ping_sent,
            last_pong_received: h.last_pong_received,
            total_pings_sent: h.total_pings_sent,
            total_pongs_received: h.total_pongs_received,
        })
    }

    /// Return the node IDs of all connections currently in `Unhealthy` state.
    pub fn unhealthy_connections(&self) -> Vec<String> {
        let guard = match self.connections.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .values()
            .filter(|h| h.status == ConnectionHealthStatus::Unhealthy)
            .map(|h| h.node_id.clone())
            .collect()
    }

    /// Return the node IDs of all connections currently in `Degraded` state.
    pub fn degraded_connections(&self) -> Vec<String> {
        let guard = match self.connections.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .values()
            .filter(|h| h.status == ConnectionHealthStatus::Degraded)
            .map(|h| h.node_id.clone())
            .collect()
    }

    /// Run a health sweep across all registered connections.
    ///
    /// Updates every connection's status then partitions them into three buckets:
    /// - `healthy`   — `Healthy` or `Unknown`
    /// - `degraded`  — `Degraded`
    /// - `to_evict`  — `Unhealthy` (callers should close these connections)
    pub fn health_sweep(&self) -> HealthSweepResult {
        let mut guard = match self.connections.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };

        let mut healthy = Vec::new();
        let mut degraded = Vec::new();
        let mut to_evict = Vec::new();

        for health in guard.values_mut() {
            health.update_status(&self.config);
            match health.status {
                ConnectionHealthStatus::Healthy | ConnectionHealthStatus::Unknown => {
                    healthy.push(health.node_id.clone());
                }
                ConnectionHealthStatus::Degraded => {
                    degraded.push(health.node_id.clone());
                }
                ConnectionHealthStatus::Unhealthy => {
                    to_evict.push(health.node_id.clone());
                }
            }
        }

        HealthSweepResult {
            healthy,
            degraded,
            to_evict,
        }
    }

    /// Return aggregate statistics for all connections.
    pub fn stats(&self) -> ConnectionMonitorStats {
        let guard = match self.connections.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };

        let mut healthy = 0usize;
        let mut degraded = 0usize;
        let mut unhealthy = 0usize;
        for h in guard.values() {
            match h.status {
                ConnectionHealthStatus::Healthy | ConnectionHealthStatus::Unknown => healthy += 1,
                ConnectionHealthStatus::Degraded => degraded += 1,
                ConnectionHealthStatus::Unhealthy => unhealthy += 1,
            }
        }

        ConnectionMonitorStats {
            total_connections: self
                .total_connections
                .load(std::sync::atomic::Ordering::Relaxed),
            healthy_connections: healthy,
            degraded_connections: degraded,
            unhealthy_connections: unhealthy,
            total_pings_sent: self
                .total_pings_sent
                .load(std::sync::atomic::Ordering::Relaxed),
            total_pongs_received: self
                .total_pongs_received
                .load(std::sync::atomic::Ordering::Relaxed),
            total_missed_pings: self
                .total_missed_pings
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// HealthSweepResult
// ---------------------------------------------------------------------------

/// Result of a single health sweep across all monitored connections
#[derive(Debug)]
pub struct HealthSweepResult {
    /// Node IDs of connections in `Healthy` or `Unknown` state
    pub healthy: Vec<String>,
    /// Node IDs of connections in `Degraded` state
    pub degraded: Vec<String>,
    /// Node IDs of connections in `Unhealthy` state — callers should evict these
    pub to_evict: Vec<String>,
}

// ---------------------------------------------------------------------------
// heartbeat_loop — async tokio task
// ---------------------------------------------------------------------------

/// Continuously fire heartbeat events for all registered connections.
///
/// On every `config.interval` tick:
/// 1. Records a missed ping for each connection that has NOT received a pong
///    since the previous tick (connections whose `last_pong_received` is older
///    than `config.timeout` are considered to have missed).
/// 2. Updates each connection's `last_ping_sent` timestamp.
///
/// The loop exits cleanly when `shutdown` emits `true`.
pub async fn heartbeat_loop(
    monitor: Arc<ConnectionHealthMonitor>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(monitor.config.interval);
    // The first tick fires immediately; skip it so we don't mark misses before
    // any pongs have had a chance to arrive.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let tick_origin = Instant::now();

                // Collect node IDs that need a missed-ping recorded
                let node_ids: Vec<String> = {
                    let guard = match monitor.connections.read() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    guard
                        .values()
                        .filter(|h| {
                            match h.last_pong_received {
                                None => h.total_pings_sent > 0,
                                Some(t) => {
                                    tick_origin.duration_since(t) > monitor.config.timeout
                                }
                            }
                        })
                        .map(|h| h.node_id.clone())
                        .collect()
                };

                for id in &node_ids {
                    let _ = monitor.record_missed_ping(id);
                }

                // Update ping timestamps
                {
                    let mut guard = match monitor.connections.write() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    for h in guard.values_mut() {
                        h.last_ping_sent = Some(Instant::now());
                        h.total_pings_sent += 1;
                    }
                }
                monitor
                    .total_pings_sent
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            Ok(()) = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Produce deterministic "DER" bytes for tests (not actually valid X.509)
    fn dummy_der(seed: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(64);
        for i in 0u8..64 {
            v.push(seed.wrapping_add(i));
        }
        v
    }

    /// Compute the SHA-256 fingerprint of `data`
    fn sha256_of(data: &[u8]) -> [u8; 32] {
        let d = ring::digest::digest(&ring::digest::SHA256, data);
        let mut out = [0u8; 32];
        out.copy_from_slice(d.as_ref());
        out
    }

    // -----------------------------------------------------------------------
    // 1. test_cert_pin_sha256_create
    // -----------------------------------------------------------------------
    #[test]
    fn test_cert_pin_sha256_create() {
        let fp = [0x42u8; 32];
        let pin = CertPin::new_sha256(fp, "test-pin");
        assert_eq!(pin.algorithm, PinAlgorithm::Sha256);
        assert_eq!(pin.fingerprint, fp.to_vec());
        assert_eq!(pin.label, "test-pin");
        assert!(!pin.is_expired());
    }

    // -----------------------------------------------------------------------
    // 2. test_cert_pin_verify_der_match
    // -----------------------------------------------------------------------
    #[test]
    fn test_cert_pin_verify_der_match() {
        let der = dummy_der(0xAB);
        let fp = sha256_of(&der);
        let pin = CertPin::new_sha256(fp, "match");
        assert!(pin.verify_der(&der));
    }

    // -----------------------------------------------------------------------
    // 3. test_cert_pin_verify_der_mismatch
    // -----------------------------------------------------------------------
    #[test]
    fn test_cert_pin_verify_der_mismatch() {
        let der = dummy_der(0xAB);
        let fp = sha256_of(&dummy_der(0xFF)); // fingerprint of a different cert
        let pin = CertPin::new_sha256(fp, "mismatch");
        assert!(!pin.verify_der(&der));
    }

    // -----------------------------------------------------------------------
    // 4. test_cert_pin_expiry — zero-duration expiry is immediately expired
    // -----------------------------------------------------------------------
    #[test]
    fn test_cert_pin_expiry() {
        let pin = CertPin::new_sha256([0u8; 32], "expired").with_expiry(Duration::ZERO);
        assert!(pin.is_expired());
    }

    // -----------------------------------------------------------------------
    // 5. test_cert_pin_not_expired
    // -----------------------------------------------------------------------
    #[test]
    fn test_cert_pin_not_expired() {
        let pin =
            CertPin::new_sha256([0u8; 32], "valid").with_expiry(Duration::from_secs(3600));
        assert!(!pin.is_expired());
    }

    // -----------------------------------------------------------------------
    // 6. test_pin_store_add_verify_pinned
    // -----------------------------------------------------------------------
    #[test]
    fn test_pin_store_add_verify_pinned() {
        let der = dummy_der(0x01);
        let fp = sha256_of(&der);
        let store = CertPinStore::new(false);
        store.add_pin("example.com", CertPin::new_sha256(fp, "label"));
        let result = store.verify("example.com", &der).unwrap();
        assert!(matches!(result, PinVerification::Pinned));
    }

    // -----------------------------------------------------------------------
    // 7. test_pin_store_no_pins_allow_unpinned
    // -----------------------------------------------------------------------
    #[test]
    fn test_pin_store_no_pins_allow_unpinned() {
        let store = CertPinStore::new(true);
        let result = store.verify("example.com", &dummy_der(0x02)).unwrap();
        assert!(matches!(result, PinVerification::Unpinned));
    }

    // -----------------------------------------------------------------------
    // 8. test_pin_store_no_pins_deny_unpinned
    // -----------------------------------------------------------------------
    #[test]
    fn test_pin_store_no_pins_deny_unpinned() {
        let store = CertPinStore::new(false);
        let err = store.verify("example.com", &dummy_der(0x03)).unwrap_err();
        assert!(matches!(err, PinError::NoPinsForHost { .. }));
    }

    // -----------------------------------------------------------------------
    // 9. test_pin_store_remove_expired
    // -----------------------------------------------------------------------
    #[test]
    fn test_pin_store_remove_expired() {
        let store = CertPinStore::new(true);
        let pin = CertPin::new_sha256([0u8; 32], "expired").with_expiry(Duration::ZERO);
        store.add_pin("example.com", pin);
        assert_eq!(store.pin_count(), 1);
        store.remove_expired();
        assert_eq!(store.pin_count(), 0);
    }

    // -----------------------------------------------------------------------
    // 10. test_pin_store_multiple_pins_any_match
    // -----------------------------------------------------------------------
    #[test]
    fn test_pin_store_multiple_pins_any_match() {
        let der = dummy_der(0x10);
        let wrong_fp = sha256_of(&dummy_der(0x99));
        let right_fp = sha256_of(&der);

        let store = CertPinStore::new(false);
        store.add_pin("example.com", CertPin::new_sha256(wrong_fp, "wrong"));
        store.add_pin("example.com", CertPin::new_sha256(right_fp, "right"));

        let result = store.verify("example.com", &der).unwrap();
        assert!(matches!(result, PinVerification::Pinned));
    }

    // -----------------------------------------------------------------------
    // 11. test_chain_verifier_empty_chain
    // -----------------------------------------------------------------------
    #[test]
    fn test_chain_verifier_empty_chain() {
        let verifier = CertChainVerifier::new();
        let err = verifier.verify_chain(&[], "example.com").unwrap_err();
        assert!(matches!(err, ChainError::EmptyChain));
    }

    // -----------------------------------------------------------------------
    // 12. test_chain_verifier_too_deep
    // -----------------------------------------------------------------------
    #[test]
    fn test_chain_verifier_too_deep() {
        let verifier = CertChainVerifier::new().with_max_depth(2);
        let chain: Vec<Vec<u8>> = (0..3u8).map(dummy_der).collect();
        let err = verifier.verify_chain(&chain, "example.com").unwrap_err();
        assert!(matches!(err, ChainError::TooDeep { depth: 3, max: 2 }));
    }

    // -----------------------------------------------------------------------
    // 13. test_chain_verifier_valid_chain
    // -----------------------------------------------------------------------
    #[test]
    fn test_chain_verifier_valid_chain() {
        let verifier = CertChainVerifier::new();
        let chain = vec![dummy_der(0x20)];
        let cv = verifier.verify_chain(&chain, "example.com").unwrap();
        assert_eq!(cv.depth, 1);
        assert!(!cv.pinned); // no pin store attached
        assert_eq!(cv.leaf_fingerprint.len(), 32);
    }

    // -----------------------------------------------------------------------
    // 14. test_connection_health_new
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_health_new() {
        let h = ConnectionHealth::new("node-1");
        assert_eq!(h.status, ConnectionHealthStatus::Unknown);
        assert!(h.rtt_samples.is_empty());
        assert_eq!(h.missed_pings, 0);
        assert_eq!(h.node_id, "node-1");
    }

    // -----------------------------------------------------------------------
    // 15. test_connection_health_record_pong
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_health_record_pong() {
        let config = HeartbeatConfig::default();
        let mut h = ConnectionHealth::new("node-2");
        for _ in 0..3 {
            h.record_pong(Duration::from_millis(20));
        }
        h.update_status(&config);
        assert_eq!(h.status, ConnectionHealthStatus::Healthy);
    }

    // -----------------------------------------------------------------------
    // 16. test_connection_health_mean_rtt
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_health_mean_rtt() {
        let mut h = ConnectionHealth::new("node-3");
        h.record_pong(Duration::from_millis(10));
        h.record_pong(Duration::from_millis(20));
        h.record_pong(Duration::from_millis(30));
        let mean = h.mean_rtt().unwrap();
        assert_eq!(mean.as_millis(), 20);
    }

    // -----------------------------------------------------------------------
    // 17. test_connection_health_p99_rtt
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_health_p99_rtt() {
        let mut h = ConnectionHealth::new("node-4");
        // 10 samples: 10ms..100ms in steps of 10ms
        for i in 1..=10u64 {
            h.record_pong(Duration::from_millis(i * 10));
        }
        let p99 = h.p99_rtt().unwrap();
        // nearest-rank p99 of 10 samples → index ceil(0.99*10)−1 = 9 → 100ms
        assert_eq!(p99.as_millis(), 100);
    }

    // -----------------------------------------------------------------------
    // 18. test_connection_health_missed_pings — 3 missed → Unhealthy
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_health_missed_pings() {
        let config = HeartbeatConfig::default(); // max_missed = 3
        let mut h = ConnectionHealth::new("node-5");
        for _ in 0..3 {
            h.record_missed_ping(&config);
        }
        h.update_status(&config);
        assert_eq!(h.status, ConnectionHealthStatus::Unhealthy);
    }

    // -----------------------------------------------------------------------
    // 19. test_connection_health_partial_miss — 1 missed → Degraded
    // -----------------------------------------------------------------------
    #[test]
    fn test_connection_health_partial_miss() {
        let config = HeartbeatConfig::default(); // max_missed = 3
        let mut h = ConnectionHealth::new("node-6");
        h.record_pong(Duration::from_millis(15)); // establish healthy baseline
        h.record_missed_ping(&config);
        h.update_status(&config);
        assert_eq!(h.status, ConnectionHealthStatus::Degraded);
    }

    // -----------------------------------------------------------------------
    // 20. test_monitor_register_unregister
    // -----------------------------------------------------------------------
    #[test]
    fn test_monitor_register_unregister() {
        let monitor = ConnectionHealthMonitor::new(HeartbeatConfig::default());
        monitor.register_connection("peer-a");
        let s = monitor.stats();
        assert_eq!(s.total_connections, 1);
        monitor.unregister_connection("peer-a");
        let s2 = monitor.stats();
        assert_eq!(s2.healthy_connections, 0);
    }

    // -----------------------------------------------------------------------
    // 21. test_monitor_record_pong
    // -----------------------------------------------------------------------
    #[test]
    fn test_monitor_record_pong() {
        let monitor = ConnectionHealthMonitor::new(HeartbeatConfig::default());
        monitor.register_connection("peer-b");
        for _ in 0..3 {
            monitor
                .record_pong("peer-b", Duration::from_millis(10))
                .unwrap();
        }
        let h = monitor.connection_health("peer-b").unwrap();
        assert_eq!(h.status, ConnectionHealthStatus::Healthy);
    }

    // -----------------------------------------------------------------------
    // 22. test_monitor_record_missed
    // -----------------------------------------------------------------------
    #[test]
    fn test_monitor_record_missed() {
        let config = HeartbeatConfig {
            max_missed: 3,
            ..Default::default()
        };
        let monitor = ConnectionHealthMonitor::new(config);
        monitor.register_connection("peer-c");
        for _ in 0..3 {
            monitor.record_missed_ping("peer-c").unwrap();
        }
        let unhealthy = monitor.unhealthy_connections();
        assert!(unhealthy.contains(&"peer-c".to_string()));
    }

    // -----------------------------------------------------------------------
    // 23. test_monitor_health_sweep
    // -----------------------------------------------------------------------
    #[test]
    fn test_monitor_health_sweep() {
        let config = HeartbeatConfig {
            max_missed: 3,
            ..Default::default()
        };
        let monitor = ConnectionHealthMonitor::new(config);

        // healthy peer
        monitor.register_connection("healthy");
        for _ in 0..3 {
            monitor
                .record_pong("healthy", Duration::from_millis(5))
                .unwrap();
        }

        // degraded peer
        monitor.register_connection("degraded");
        monitor
            .record_pong("degraded", Duration::from_millis(5))
            .unwrap();
        monitor.record_missed_ping("degraded").unwrap();

        // unhealthy peer
        monitor.register_connection("dead");
        for _ in 0..3 {
            monitor.record_missed_ping("dead").unwrap();
        }

        let sweep = monitor.health_sweep();
        assert!(sweep.healthy.contains(&"healthy".to_string()));
        assert!(sweep.degraded.contains(&"degraded".to_string()));
        assert!(sweep.to_evict.contains(&"dead".to_string()));
    }

    // -----------------------------------------------------------------------
    // 24. test_monitor_stats_accumulate
    // -----------------------------------------------------------------------
    #[test]
    fn test_monitor_stats_accumulate() {
        let monitor = ConnectionHealthMonitor::new(HeartbeatConfig::default());
        monitor.register_connection("p1");
        monitor.register_connection("p2");
        monitor
            .record_pong("p1", Duration::from_millis(10))
            .unwrap();
        monitor
            .record_pong("p2", Duration::from_millis(20))
            .unwrap();
        monitor.record_missed_ping("p1").unwrap();

        let s = monitor.stats();
        assert_eq!(s.total_connections, 2);
        assert_eq!(s.total_pongs_received, 2);
        assert_eq!(s.total_missed_pings, 1);
    }

    // -----------------------------------------------------------------------
    // 25. test_heartbeat_loop_shutdown
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_heartbeat_loop_shutdown() {
        let config = HeartbeatConfig {
            interval: Duration::from_millis(50),
            ..Default::default()
        };
        let monitor = Arc::new(ConnectionHealthMonitor::new(config));
        let (tx, rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn(heartbeat_loop(Arc::clone(&monitor), rx));

        // Give it one tick, then shut it down
        tokio::time::sleep(Duration::from_millis(80)).await;
        tx.send(true).unwrap();

        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("heartbeat_loop did not shut down in time")
            .expect("task panicked");
    }

    // -----------------------------------------------------------------------
    // 26. test_cert_pin_store_hostname_count
    // -----------------------------------------------------------------------
    #[test]
    fn test_cert_pin_store_hostname_count() {
        let store = CertPinStore::new(true);
        store.add_pin("host-a.example.com", CertPin::new_sha256([0u8; 32], "a1"));
        store.add_pin("host-a.example.com", CertPin::new_sha256([1u8; 32], "a2"));
        store.add_pin("host-b.example.com", CertPin::new_sha256([2u8; 32], "b1"));
        assert_eq!(store.hostname_count(), 2);
        assert_eq!(store.pin_count(), 3);
    }
}
