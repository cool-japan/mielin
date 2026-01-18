//! Security hardening features
//!
//! Enhanced TLS 1.3 configuration, cryptographic algorithm selection,
//! DoS protection, rate limiting, and anomaly detection.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

/// TLS 1.3 cipher suite selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsCipherSuite {
    /// TLS_AES_128_GCM_SHA256 (fast, standard security)
    Aes128GcmSha256,
    /// TLS_AES_256_GCM_SHA384 (slower, high security)
    Aes256GcmSha384,
    /// TLS_CHACHA20_POLY1305_SHA256 (fast on mobile/no AES-NI)
    ChaCha20Poly1305Sha256,
}

impl TlsCipherSuite {
    /// Get rustls cipher suite
    pub fn to_rustls_suite(&self) -> rustls::CipherSuite {
        match self {
            Self::Aes128GcmSha256 => rustls::CipherSuite::TLS13_AES_128_GCM_SHA256,
            Self::Aes256GcmSha384 => rustls::CipherSuite::TLS13_AES_256_GCM_SHA384,
            Self::ChaCha20Poly1305Sha256 => rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
        }
    }

    /// Get all supported cipher suites
    pub fn all() -> Vec<Self> {
        vec![
            Self::Aes256GcmSha384,
            Self::Aes128GcmSha256,
            Self::ChaCha20Poly1305Sha256,
        ]
    }

    /// Get high security cipher suites
    pub fn high_security() -> Vec<Self> {
        vec![Self::Aes256GcmSha384]
    }

    /// Get balanced cipher suites
    pub fn balanced() -> Vec<Self> {
        vec![Self::Aes256GcmSha384, Self::Aes128GcmSha256]
    }
}

/// Key exchange algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyExchangeAlgorithm {
    /// X25519 (Curve25519 ECDH)
    X25519,
    /// secp256r1 (NIST P-256)
    Secp256r1,
    /// secp384r1 (NIST P-384)
    Secp384r1,
}

/// Signature algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureAlgorithm {
    /// Ed25519
    Ed25519,
    /// ECDSA with P-256
    EcdsaP256Sha256,
    /// ECDSA with P-384
    EcdsaP384Sha384,
    /// RSA-PSS with SHA-256
    RsaPssSha256,
    /// RSA-PSS with SHA-384
    RsaPssSha384,
}

/// Enhanced TLS configuration
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Cipher suites in order of preference
    pub cipher_suites: Vec<TlsCipherSuite>,
    /// Signature algorithms in order of preference
    pub signature_algorithms: Vec<SignatureAlgorithm>,
    /// Minimum TLS version (must be 1.3)
    pub min_version: TlsVersion,
    /// Maximum TLS version
    pub max_version: TlsVersion,
    /// Enable session resumption
    pub enable_session_resumption: bool,
    /// Enable 0-RTT (requires careful consideration)
    pub enable_0rtt: bool,
    /// Maximum certificate chain length
    pub max_cert_chain_length: usize,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cipher_suites: TlsCipherSuite::balanced(),
            signature_algorithms: vec![
                SignatureAlgorithm::Ed25519,
                SignatureAlgorithm::EcdsaP256Sha256,
                SignatureAlgorithm::EcdsaP384Sha384,
            ],
            min_version: TlsVersion::V1_3,
            max_version: TlsVersion::V1_3,
            enable_session_resumption: true,
            enable_0rtt: false,
            max_cert_chain_length: 4,
        }
    }
}

impl TlsConfig {
    /// High security configuration
    pub fn high_security() -> Self {
        Self {
            cipher_suites: TlsCipherSuite::high_security(),
            signature_algorithms: vec![
                SignatureAlgorithm::Ed25519,
                SignatureAlgorithm::EcdsaP384Sha384,
            ],
            min_version: TlsVersion::V1_3,
            max_version: TlsVersion::V1_3,
            enable_session_resumption: false,
            enable_0rtt: false,
            max_cert_chain_length: 3,
        }
    }

    /// Production configuration
    pub fn production() -> Self {
        Self::default()
    }

    /// Development configuration (more permissive)
    pub fn development() -> Self {
        Self {
            cipher_suites: TlsCipherSuite::all(),
            ..Self::default()
        }
    }
}

/// TLS version
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TlsVersion {
    /// TLS 1.3 (required)
    V1_3,
}

/// Connection rate limiter
pub struct RateLimiter {
    /// Maximum connections per IP per window
    max_connections_per_ip: usize,
    /// Time window for rate limiting
    window: Duration,
    /// Connection attempts by IP
    attempts: Arc<RwLock<HashMap<IpAddr, VecDeque<Instant>>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(max_connections_per_ip: usize, window: Duration) -> Self {
        Self {
            max_connections_per_ip,
            window,
            attempts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a permissive rate limiter for development
    pub fn permissive() -> Self {
        Self::new(1000, Duration::from_secs(60))
    }

    /// Create a strict rate limiter for production
    pub fn strict() -> Self {
        Self::new(100, Duration::from_secs(60))
    }

    /// Check if a connection from the given IP is allowed
    pub async fn check_allowed(&self, ip: IpAddr) -> Result<(), SecurityError> {
        let mut attempts = self.attempts.write().await;
        let now = Instant::now();

        let entry = attempts.entry(ip).or_insert_with(VecDeque::new);

        // Remove old attempts outside the window
        while let Some(oldest) = entry.front() {
            if now.duration_since(*oldest) > self.window {
                entry.pop_front();
            } else {
                break;
            }
        }

        // Check if limit is reached
        if entry.len() >= self.max_connections_per_ip {
            return Err(SecurityError::RateLimitExceeded(ip));
        }

        // Record this attempt
        entry.push_back(now);

        Ok(())
    }

    /// Get current connection count for an IP
    pub async fn get_connection_count(&self, ip: IpAddr) -> usize {
        let attempts = self.attempts.read().await;
        attempts.get(&ip).map(|v| v.len()).unwrap_or(0)
    }

    /// Clear old entries (cleanup)
    pub async fn cleanup(&self) {
        let mut attempts = self.attempts.write().await;
        let now = Instant::now();

        attempts.retain(|_, entries| {
            // Remove old entries
            while let Some(oldest) = entries.front() {
                if now.duration_since(*oldest) > self.window {
                    entries.pop_front();
                } else {
                    break;
                }
            }
            !entries.is_empty()
        });
    }
}

/// IP allowlist/blocklist
pub struct IpFilter {
    /// Allowed IPs (if not empty, only these are allowed)
    allowed: Arc<RwLock<Vec<IpAddr>>>,
    /// Blocked IPs (always denied)
    blocked: Arc<RwLock<Vec<IpAddr>>>,
    /// Allowed CIDR ranges
    allowed_ranges: Arc<RwLock<Vec<(IpAddr, u8)>>>,
}

impl IpFilter {
    /// Create a new IP filter
    pub fn new() -> Self {
        Self {
            allowed: Arc::new(RwLock::new(Vec::new())),
            blocked: Arc::new(RwLock::new(Vec::new())),
            allowed_ranges: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add an IP to the allowlist
    pub async fn allow_ip(&self, ip: IpAddr) {
        let mut allowed = self.allowed.write().await;
        if !allowed.contains(&ip) {
            allowed.push(ip);
        }
    }

    /// Add an IP to the blocklist
    pub async fn block_ip(&self, ip: IpAddr) {
        let mut blocked = self.blocked.write().await;
        if !blocked.contains(&ip) {
            blocked.push(ip);
        }
    }

    /// Add a CIDR range to the allowlist
    pub async fn allow_range(&self, network: IpAddr, prefix_len: u8) {
        let mut ranges = self.allowed_ranges.write().await;
        ranges.push((network, prefix_len));
    }

    /// Check if an IP is allowed
    pub async fn is_allowed(&self, ip: IpAddr) -> Result<(), SecurityError> {
        // Check blocklist first
        let blocked = self.blocked.read().await;
        if blocked.contains(&ip) {
            return Err(SecurityError::IpBlocked(ip));
        }

        // If allowlist is empty, allow all (except blocked)
        let allowed = self.allowed.read().await;
        if allowed.is_empty() {
            // Check CIDR ranges
            let ranges = self.allowed_ranges.read().await;
            if ranges.is_empty() {
                return Ok(());
            }

            // Check if IP is in any allowed range
            for (network, prefix_len) in ranges.iter() {
                if ip_in_range(ip, *network, *prefix_len) {
                    return Ok(());
                }
            }

            return Err(SecurityError::IpNotAllowed(ip));
        }

        // Check if IP is in allowlist
        if allowed.contains(&ip) {
            return Ok(());
        }

        // Check CIDR ranges
        let ranges = self.allowed_ranges.read().await;
        for (network, prefix_len) in ranges.iter() {
            if ip_in_range(ip, *network, *prefix_len) {
                return Ok(());
            }
        }

        Err(SecurityError::IpNotAllowed(ip))
    }

    /// Remove an IP from the blocklist
    pub async fn unblock_ip(&self, ip: IpAddr) {
        let mut blocked = self.blocked.write().await;
        blocked.retain(|&x| x != ip);
    }

    /// Clear all filters
    pub async fn clear(&self) {
        self.allowed.write().await.clear();
        self.blocked.write().await.clear();
        self.allowed_ranges.write().await.clear();
    }
}

impl Default for IpFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if an IP is in a CIDR range
fn ip_in_range(ip: IpAddr, network: IpAddr, prefix_len: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) => {
            let ip_bits = u32::from(ip);
            let network_bits = u32::from(network);
            let mask = !((1u32 << (32 - prefix_len)) - 1);
            (ip_bits & mask) == (network_bits & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) => {
            let ip_bits = u128::from(ip);
            let network_bits = u128::from(network);
            let mask = !((1u128 << (128 - prefix_len)) - 1);
            (ip_bits & mask) == (network_bits & mask)
        }
        _ => false,
    }
}

/// Anomaly detection for suspicious connection patterns
pub struct AnomalyDetector {
    /// Connection rate threshold (connections per second)
    rate_threshold: f64,
    /// Message size threshold (bytes)
    message_size_threshold: usize,
    /// Failed authentication threshold
    auth_failure_threshold: usize,
    /// Statistics by IP
    stats: Arc<RwLock<HashMap<IpAddr, ConnectionStats>>>,
}

/// Connection statistics for anomaly detection
#[derive(Debug, Clone)]
struct ConnectionStats {
    /// Total connections
    total_connections: usize,
    /// Failed authentications
    failed_auths: usize,
    /// Average message size
    avg_message_size: usize,
    /// Last connection time
    last_connection: Instant,
    /// Connection rate (per second)
    connection_rate: f64,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self {
            total_connections: 0,
            failed_auths: 0,
            avg_message_size: 0,
            last_connection: Instant::now(),
            connection_rate: 0.0,
        }
    }
}

impl AnomalyDetector {
    /// Create a new anomaly detector
    pub fn new() -> Self {
        Self {
            rate_threshold: 10.0,                // 10 connections per second
            message_size_threshold: 1024 * 1024, // 1MB
            auth_failure_threshold: 5,
            stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a connection attempt
    pub async fn record_connection(&self, ip: IpAddr) {
        let mut stats = self.stats.write().await;
        let entry = stats.entry(ip).or_insert_with(ConnectionStats::default);

        let now = Instant::now();

        // Only calculate rate if we have previous connections
        if entry.total_connections > 0 {
            let elapsed = now.duration_since(entry.last_connection).as_secs_f64();
            entry.connection_rate = if elapsed > 0.0 { 1.0 / elapsed } else { 0.0 };
        }

        entry.total_connections += 1;
        entry.last_connection = now;
    }

    /// Record a failed authentication
    pub async fn record_auth_failure(&self, ip: IpAddr) {
        let mut stats = self.stats.write().await;
        let entry = stats.entry(ip).or_insert_with(ConnectionStats::default);
        entry.failed_auths += 1;
    }

    /// Record a message size
    pub async fn record_message_size(&self, ip: IpAddr, size: usize) {
        let mut stats = self.stats.write().await;
        let entry = stats.entry(ip).or_insert_with(ConnectionStats::default);

        // Update average
        let total = entry.total_connections;
        entry.avg_message_size =
            (entry.avg_message_size * (total.saturating_sub(1)) + size) / total.max(1);
    }

    /// Check for anomalies
    pub async fn check_anomalies(&self, ip: IpAddr) -> Result<(), SecurityError> {
        let stats = self.stats.read().await;
        let entry = stats.get(&ip);

        if let Some(entry) = entry {
            // Check connection rate
            if entry.connection_rate > self.rate_threshold {
                return Err(SecurityError::AnomalyDetected(
                    ip,
                    format!("High connection rate: {:.2}/s", entry.connection_rate),
                ));
            }

            // Check failed authentications
            if entry.failed_auths > self.auth_failure_threshold {
                return Err(SecurityError::AnomalyDetected(
                    ip,
                    format!("Too many failed authentications: {}", entry.failed_auths),
                ));
            }

            // Check message size
            if entry.avg_message_size > self.message_size_threshold {
                return Err(SecurityError::AnomalyDetected(
                    ip,
                    format!("Unusually large messages: {} bytes", entry.avg_message_size),
                ));
            }
        }

        Ok(())
    }

    /// Clear statistics for an IP
    pub async fn clear_stats(&self, ip: IpAddr) {
        let mut stats = self.stats.write().await;
        stats.remove(&ip);
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined security manager
pub struct SecurityManager {
    /// TLS configuration
    pub tls_config: TlsConfig,
    /// Rate limiter
    pub rate_limiter: RateLimiter,
    /// IP filter
    pub ip_filter: IpFilter,
    /// Anomaly detector
    pub anomaly_detector: AnomalyDetector,
}

impl SecurityManager {
    /// Create a new security manager with default settings
    pub fn new() -> Self {
        Self {
            tls_config: TlsConfig::default(),
            rate_limiter: RateLimiter::permissive(),
            ip_filter: IpFilter::new(),
            anomaly_detector: AnomalyDetector::new(),
        }
    }

    /// Create a security manager for production
    pub fn production() -> Self {
        Self {
            tls_config: TlsConfig::production(),
            rate_limiter: RateLimiter::strict(),
            ip_filter: IpFilter::new(),
            anomaly_detector: AnomalyDetector::new(),
        }
    }

    /// Create a security manager for development
    pub fn development() -> Self {
        Self {
            tls_config: TlsConfig::development(),
            rate_limiter: RateLimiter::permissive(),
            ip_filter: IpFilter::new(),
            anomaly_detector: AnomalyDetector::new(),
        }
    }

    /// Check if a connection from the given IP is allowed
    pub async fn check_connection(&self, ip: IpAddr) -> Result<(), SecurityError> {
        // Check IP filter
        self.ip_filter.is_allowed(ip).await?;

        // Check rate limiter
        self.rate_limiter.check_allowed(ip).await?;

        // Check anomaly detector
        self.anomaly_detector.check_anomalies(ip).await?;

        // Record connection
        self.anomaly_detector.record_connection(ip).await;

        Ok(())
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Security-related errors
#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Rate limit exceeded for IP: {0}")]
    RateLimitExceeded(IpAddr),

    #[error("IP blocked: {0}")]
    IpBlocked(IpAddr),

    #[error("IP not allowed: {0}")]
    IpNotAllowed(IpAddr),

    #[error("Anomaly detected from IP {0}: {1}")]
    AnomalyDetected(IpAddr, String),

    #[error("TLS configuration error: {0}")]
    TlsConfigError(String),

    #[error("Certificate validation failed: {0}")]
    CertificateValidationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_tls_cipher_suites() {
        let all = TlsCipherSuite::all();
        assert_eq!(all.len(), 3);

        let high_sec = TlsCipherSuite::high_security();
        assert_eq!(high_sec.len(), 1);
        assert_eq!(high_sec[0], TlsCipherSuite::Aes256GcmSha384);

        let balanced = TlsCipherSuite::balanced();
        assert_eq!(balanced.len(), 2);
    }

    #[test]
    fn test_tls_config() {
        let default = TlsConfig::default();
        assert_eq!(default.min_version, TlsVersion::V1_3);
        assert!(default.enable_session_resumption);
        assert!(!default.enable_0rtt);

        let high_sec = TlsConfig::high_security();
        assert!(!high_sec.enable_session_resumption);
        assert_eq!(high_sec.max_cert_chain_length, 3);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(2, Duration::from_secs(1));
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        // First two should succeed
        assert!(limiter.check_allowed(ip).await.is_ok());
        assert!(limiter.check_allowed(ip).await.is_ok());

        // Third should fail
        assert!(limiter.check_allowed(ip).await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup() {
        let limiter = RateLimiter::new(2, Duration::from_millis(100));
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        limiter.check_allowed(ip).await.unwrap();
        limiter.check_allowed(ip).await.unwrap();

        // Wait for window to expire
        tokio::time::sleep(Duration::from_millis(150)).await;
        limiter.cleanup().await;

        // Should succeed after cleanup
        assert!(limiter.check_allowed(ip).await.is_ok());
    }

    #[tokio::test]
    async fn test_ip_filter_allowlist() {
        let filter = IpFilter::new();
        let allowed_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let other_ip: IpAddr = "192.168.1.1".parse().unwrap();

        filter.allow_ip(allowed_ip).await;

        assert!(filter.is_allowed(allowed_ip).await.is_ok());
        assert!(filter.is_allowed(other_ip).await.is_err());
    }

    #[tokio::test]
    async fn test_ip_filter_blocklist() {
        let filter = IpFilter::new();
        let blocked_ip: IpAddr = "10.0.0.1".parse().unwrap();

        filter.block_ip(blocked_ip).await;

        assert!(filter.is_allowed(blocked_ip).await.is_err());
    }

    #[tokio::test]
    async fn test_ip_filter_cidr_range() {
        let filter = IpFilter::new();
        let network: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 0));
        let ip_in_range: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 100));
        let ip_out_range: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 169, 0, 1));

        filter.allow_range(network, 16).await;

        assert!(filter.is_allowed(ip_in_range).await.is_ok());
        assert!(filter.is_allowed(ip_out_range).await.is_err());
    }

    #[test]
    fn test_ip_in_range() {
        let network: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 0));
        let ip1: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1));
        let ip2: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip3: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 169, 0, 1));

        assert!(ip_in_range(ip1, network, 24));
        assert!(!ip_in_range(ip2, network, 24));
        assert!(ip_in_range(ip2, network, 16));
        assert!(!ip_in_range(ip3, network, 16));
    }

    #[tokio::test]
    async fn test_anomaly_detector() {
        let detector = AnomalyDetector::new();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        detector.record_connection(ip).await;
        assert!(detector.check_anomalies(ip).await.is_ok());

        // Record multiple failed authentications
        for _ in 0..6 {
            detector.record_auth_failure(ip).await;
        }
        assert!(detector.check_anomalies(ip).await.is_err());
    }

    #[tokio::test]
    async fn test_anomaly_detector_message_size() {
        let detector = AnomalyDetector::new();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        detector.record_connection(ip).await;
        detector.record_message_size(ip, 2 * 1024 * 1024).await; // 2MB

        assert!(detector.check_anomalies(ip).await.is_err());
    }

    #[tokio::test]
    async fn test_security_manager() {
        let manager = SecurityManager::development();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        assert!(manager.check_connection(ip).await.is_ok());
    }

    #[tokio::test]
    async fn test_security_manager_blocked_ip() {
        let manager = SecurityManager::new();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();

        manager.ip_filter.block_ip(ip).await;
        assert!(manager.check_connection(ip).await.is_err());
    }

    #[tokio::test]
    async fn test_security_manager_rate_limit() {
        let manager = SecurityManager {
            rate_limiter: RateLimiter::new(1, Duration::from_secs(60)),
            ..SecurityManager::new()
        };
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        assert!(manager.check_connection(ip).await.is_ok());
        assert!(manager.check_connection(ip).await.is_err());
    }
}
