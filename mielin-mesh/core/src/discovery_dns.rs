//! DNS SRV-Based Discovery
//!
//! Models the DNS SRV record protocol semantics for service discovery.
//! This abstraction layer does not depend on any async DNS resolver — instead
//! it maintains an injected record cache that production code would populate via
//! a real resolver (e.g. `trust-dns-resolver`), while tests drive it through
//! `inject_records`.
//!
//! SRV record selection follows RFC 2782:
//! 1. Group records by priority (lower numeric value = higher priority).
//! 2. Within a priority group, perform weighted random selection
//!    proportional to the `weight` field.
//!
//! A Xorshift64 PRNG seeded by the caller provides deterministic,
//! reproducible selection without any external randomness dependency.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Xorshift64 — deterministic PRNG (no external dep)
// ---------------------------------------------------------------------------

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0xCAFE_BABE_DEAD_BEEF
        } else {
            seed
        })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

// ---------------------------------------------------------------------------
// DnsSrvRecord
// ---------------------------------------------------------------------------

/// A single DNS SRV record as defined by RFC 2782.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsSrvRecord {
    /// Priority: lower numeric value means higher preference.
    pub priority: u16,
    /// Weight for stochastic selection within the same priority tier.
    pub weight: u16,
    /// TCP/UDP port number.
    pub port: u16,
    /// Target hostname (FQDN without trailing dot).
    pub target: String,
}

impl DnsSrvRecord {
    /// Convenience constructor.
    pub fn new(priority: u16, weight: u16, port: u16, target: impl Into<String>) -> Self {
        Self {
            priority,
            weight,
            port,
            target: target.into(),
        }
    }

    /// Returns a best-effort `SocketAddr` string (hostname:port) for logging.
    pub fn socket_repr(&self) -> String {
        format!("{}:{}", self.target, self.port)
    }
}

// ---------------------------------------------------------------------------
// DnsSrvConfig
// ---------------------------------------------------------------------------

/// Configuration for a DNS SRV discovery instance.
#[derive(Debug, Clone)]
pub struct DnsSrvConfig {
    /// Service label, e.g. `_mielin._tcp`.
    pub service_name: String,
    /// DNS domain, e.g. `example.com`.
    pub domain: String,
    /// Optional custom DNS resolver endpoint.
    pub resolver_addr: Option<std::net::SocketAddr>,
    /// How long fetched records are considered fresh.
    pub ttl: Duration,
    /// Maximum number of SRV records to use after a lookup (0 = unlimited).
    pub max_records: usize,
}

impl DnsSrvConfig {
    /// Create a minimal configuration with sensible defaults.
    pub fn new(service: impl Into<String>, domain: impl Into<String>) -> Self {
        Self {
            service_name: service.into(),
            domain: domain.into(),
            resolver_addr: None,
            ttl: Duration::from_secs(60),
            max_records: 10,
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn with_resolver(mut self, addr: std::net::SocketAddr) -> Self {
        self.resolver_addr = Some(addr);
        self
    }

    pub fn with_max_records(mut self, max: usize) -> Self {
        self.max_records = max;
        self
    }

    /// Fully-qualified DNS name for the SRV query, e.g. `_mielin._tcp.example.com`.
    pub fn service_fqdn(&self) -> String {
        format!("{}.{}", self.service_name, self.domain)
    }
}

// ---------------------------------------------------------------------------
// SrvCacheEntry
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of SRV records with TTL tracking.
#[derive(Debug, Clone)]
pub struct SrvCacheEntry {
    pub records: Vec<DnsSrvRecord>,
    pub fetched_at: Instant,
    pub ttl: Duration,
}

impl SrvCacheEntry {
    /// Returns `true` if the cache entry has lived longer than its TTL.
    pub fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() >= self.ttl
    }

    /// Remaining validity duration (saturates at `Duration::ZERO` if expired).
    pub fn time_remaining(&self) -> Duration {
        self.ttl.saturating_sub(self.fetched_at.elapsed())
    }
}

// ---------------------------------------------------------------------------
// DnsDiscoveryStats
// ---------------------------------------------------------------------------

/// Cumulative operational metrics for a `DnsSrvDiscovery` instance.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DnsDiscoveryStats {
    /// Total number of `lookup()` calls made.
    pub total_lookups: u64,
    /// How many lookups were served from cache.
    pub cache_hits: u64,
    /// How many lookups required a (simulated) network fetch.
    pub cache_misses: u64,
    /// Number of lookup errors.
    pub lookup_errors: u64,
    /// Cumulative count of individual SRV records seen.
    pub records_discovered: u64,
    /// Latency of the most recent lookup in milliseconds.
    pub last_lookup_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// DnsSrvDiscovery
// ---------------------------------------------------------------------------

/// DNS SRV discovery service with TTL-based caching and RFC 2782 selection.
///
/// In a real deployment, `refresh()` would call an async DNS resolver. Here it
/// signals "no records available" unless `inject_records()` has been called.
pub struct DnsSrvDiscovery {
    config: DnsSrvConfig,
    cache: Arc<RwLock<Option<SrvCacheEntry>>>,
    stats: Arc<RwLock<DnsDiscoveryStats>>,
}

impl DnsSrvDiscovery {
    /// Create a new discovery instance from a `DnsSrvConfig`.
    pub fn new(config: DnsSrvConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(None)),
            stats: Arc::new(RwLock::new(DnsDiscoveryStats::default())),
        }
    }

    /// Inject mock SRV records directly into the cache — primary test entry-point.
    ///
    /// The injected records are treated as a fresh fetch; the TTL clock starts now.
    pub async fn inject_records(&self, records: Vec<DnsSrvRecord>) {
        let entry = SrvCacheEntry {
            records: records.clone(),
            fetched_at: Instant::now(),
            ttl: self.config.ttl,
        };
        let count = records.len() as u64;
        let mut cache = self.cache.write().await;
        *cache = Some(entry);
        drop(cache);
        let mut stats = self.stats.write().await;
        stats.records_discovered = stats.records_discovered.saturating_add(count);
    }

    /// Return SRV records, using cache when valid; otherwise attempts a refresh.
    ///
    /// Cache validity is checked first. If expired or absent, `refresh()` is
    /// called which — without an actual DNS resolver — will return an error
    /// unless records were pre-injected via `inject_records()`.
    pub async fn lookup(&self) -> Result<Vec<DnsSrvRecord>, DnsDiscoveryError> {
        let start = Instant::now();
        let mut stats = self.stats.write().await;
        stats.total_lookups += 1;
        drop(stats);

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(entry) = &*cache {
                if !entry.is_expired() {
                    let records = self.apply_max_records(entry.records.clone());
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    let mut stats = self.stats.write().await;
                    stats.cache_hits += 1;
                    stats.last_lookup_ms = Some(elapsed_ms);
                    return Ok(records);
                }
            }
        }

        // Cache miss — attempt refresh
        let mut stats = self.stats.write().await;
        stats.cache_misses += 1;
        drop(stats);

        match self.do_refresh().await {
            Ok(count) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let mut stats = self.stats.write().await;
                stats.last_lookup_ms = Some(elapsed_ms);
                let _ = count;
                let cache = self.cache.read().await;
                if let Some(entry) = &*cache {
                    return Ok(self.apply_max_records(entry.records.clone()));
                }
                Err(DnsDiscoveryError::NoRecords {
                    service: self.config.service_fqdn(),
                })
            }
            Err(e) => {
                let mut stats = self.stats.write().await;
                stats.lookup_errors += 1;
                Err(e)
            }
        }
    }

    /// Force a cache refresh, returning the number of records fetched.
    ///
    /// In production this would drive a real DNS resolver. In this
    /// abstraction it returns `CacheExpired` when no records have been
    /// injected.
    pub async fn refresh(&self) -> Result<usize, DnsDiscoveryError> {
        self.do_refresh().await
    }

    /// Internal refresh — checks whether injected records are available.
    async fn do_refresh(&self) -> Result<usize, DnsDiscoveryError> {
        // In a real implementation this would perform an async DNS lookup.
        // Here we check whether the cache already has entries (possibly
        // expired). If so, we "re-validate" them (reset TTL) to simulate a
        // successful re-fetch. If empty, we signal no records.
        let mut cache = self.cache.write().await;
        match cache.as_mut() {
            Some(entry) => {
                // Re-stamp the TTL to simulate a successful refresh.
                entry.fetched_at = Instant::now();
                let count = entry.records.len();
                Ok(count)
            }
            None => Err(DnsDiscoveryError::NoRecords {
                service: self.config.service_fqdn(),
            }),
        }
    }

    /// Return SRV records sorted ascending by priority (RFC 2782 §3 step 1).
    pub async fn sorted_peers(&self) -> Result<Vec<DnsSrvRecord>, DnsDiscoveryError> {
        let mut records = self.lookup().await?;
        records.sort_by_key(|r| r.priority);
        Ok(records)
    }

    /// Select a single SRV record using RFC 2782 weighted selection.
    ///
    /// Algorithm:
    /// 1. Sort records by priority.
    /// 2. Take all records from the lowest priority tier.
    /// 3. Apply weighted random selection (Xorshift64) within that tier.
    pub async fn select_peer(&self, seed: u64) -> Result<DnsSrvRecord, DnsDiscoveryError> {
        let records = self.sorted_peers().await?;
        if records.is_empty() {
            return Err(DnsDiscoveryError::NoRecords {
                service: self.config.service_fqdn(),
            });
        }

        // Identify the highest-priority tier (smallest numeric priority value).
        let min_priority = records[0].priority;
        let tier: Vec<&DnsSrvRecord> = records
            .iter()
            .filter(|r| r.priority == min_priority)
            .collect();

        let total_weight: u32 = tier.iter().map(|r| r.weight as u32).sum();
        if total_weight == 0 {
            // All weights zero → uniform selection.
            let mut rng = Xorshift64::new(seed);
            let idx = (rng.next() as usize) % tier.len();
            return Ok(tier[idx].clone());
        }

        let mut rng = Xorshift64::new(seed);
        let draw = (rng.next() as u32) % total_weight;
        let mut cumulative: u32 = 0;
        for record in &tier {
            cumulative += record.weight as u32;
            if cumulative > draw {
                return Ok((*record).clone());
            }
        }
        // Fallback — logically unreachable with positive total weight.
        Ok(tier[0].clone())
    }

    /// Get a snapshot of current statistics.
    pub async fn stats(&self) -> DnsDiscoveryStats {
        self.stats.read().await.clone()
    }

    /// Returns `true` if a non-expired cache entry exists.
    pub async fn is_cache_valid(&self) -> bool {
        let cache = self.cache.read().await;
        cache.as_ref().map(|e| !e.is_expired()).unwrap_or(false)
    }

    /// Access the configuration.
    pub fn config(&self) -> &DnsSrvConfig {
        &self.config
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    fn apply_max_records(&self, mut records: Vec<DnsSrvRecord>) -> Vec<DnsSrvRecord> {
        if self.config.max_records > 0 && records.len() > self.config.max_records {
            records.truncate(self.config.max_records);
        }
        records
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum DnsDiscoveryError {
    #[error("DNS lookup failed: {0}")]
    LookupFailed(String),

    #[error("No SRV records found for {service}")]
    NoRecords { service: String },

    #[error("Cache expired and no fallback available")]
    CacheExpired,

    #[error("Invalid SRV record: {reason}")]
    InvalidRecord { reason: String },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_config() -> DnsSrvConfig {
        DnsSrvConfig::new("_mielin._tcp", "example.com")
    }

    fn make_record(priority: u16, weight: u16, port: u16, target: &str) -> DnsSrvRecord {
        DnsSrvRecord::new(priority, weight, port, target)
    }

    // 13. service_fqdn construction
    #[test]
    fn test_dns_config_service_fqdn() {
        let cfg = DnsSrvConfig::new("_mielin._tcp", "example.com");
        assert_eq!(cfg.service_fqdn(), "_mielin._tcp.example.com");
    }

    // 14. inject_records followed by lookup returns injected records
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_inject_records() {
        let svc = DnsSrvDiscovery::new(basic_config());
        let records = vec![make_record(10, 100, 8080, "node1.example.com")];
        svc.inject_records(records.clone()).await;
        let result = svc.lookup().await.expect("lookup must succeed");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, "node1.example.com");
    }

    // 15. Second lookup after inject is a cache hit
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_cache_hit() {
        let svc = DnsSrvDiscovery::new(basic_config());
        svc.inject_records(vec![make_record(10, 50, 8080, "n1.example.com")])
            .await;
        let _ = svc.lookup().await.unwrap(); // first — cache miss + inject loads it
        let _ = svc.lookup().await.unwrap(); // second — must be cache hit
        let stats = svc.stats().await;
        assert!(stats.cache_hits >= 1, "Expected at least one cache hit");
    }

    // 16. sorted_peers returns lower priority first
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_sorted_by_priority() {
        let svc = DnsSrvDiscovery::new(basic_config());
        let records = vec![
            make_record(10, 100, 8080, "low-prio.example.com"),
            make_record(1, 100, 8081, "high-prio.example.com"),
            make_record(5, 100, 8082, "mid-prio.example.com"),
        ];
        svc.inject_records(records).await;
        let sorted = svc.sorted_peers().await.expect("sorted ok");
        assert_eq!(sorted[0].priority, 1);
        assert_eq!(sorted[1].priority, 5);
        assert_eq!(sorted[2].priority, 10);
    }

    // 17. select_peer returns a valid record
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_select_peer_from_records() {
        let svc = DnsSrvDiscovery::new(basic_config());
        svc.inject_records(vec![
            make_record(1, 100, 8080, "a.example.com"),
            make_record(1, 100, 8081, "b.example.com"),
        ])
        .await;
        let selected = svc.select_peer(99).await.expect("must select");
        assert!(selected.port == 8080 || selected.port == 8081);
    }

    // 18. is_cache_valid returns true after inject
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_is_cache_valid() {
        let svc = DnsSrvDiscovery::new(basic_config());
        assert!(!svc.is_cache_valid().await);
        svc.inject_records(vec![make_record(1, 10, 80, "x.example.com")])
            .await;
        assert!(svc.is_cache_valid().await);
    }

    // 19. lookup increments total_lookups
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_stats_lookup_counts() {
        let svc = DnsSrvDiscovery::new(basic_config());
        svc.inject_records(vec![make_record(1, 10, 80, "x.example.com")])
            .await;
        let _ = svc.lookup().await;
        let _ = svc.lookup().await;
        let stats = svc.stats().await;
        assert_eq!(stats.total_lookups, 2);
    }

    // 20. No records available yields NoRecords error
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_no_records_error() {
        let svc = DnsSrvDiscovery::new(basic_config());
        let result = svc.lookup().await;
        assert!(
            matches!(result, Err(DnsDiscoveryError::NoRecords { .. })),
            "Expected NoRecords, got {:?}",
            result
        );
    }

    // 21. Weighted selection with equal-priority records follows weights
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_dns_weighted_selection() {
        let svc = DnsSrvDiscovery::new(basic_config());
        svc.inject_records(vec![
            make_record(1, 1, 8080, "light.example.com"), // weight 1
            make_record(1, 9, 8081, "heavy.example.com"), // weight 9
        ])
        .await;

        let trials = 200u64;
        let mut heavy_count = 0u64;
        for seed in 0..trials {
            match svc.select_peer(seed * 11 + 7).await {
                Ok(r) if r.port == 8081 => heavy_count += 1,
                _ => {}
            }
        }
        let ratio = heavy_count as f64 / trials as f64;
        assert!(
            ratio >= 0.60,
            "Expected heavy record selected >=60% but got {ratio:.2}"
        );
    }

    // 22. Custom resolver address is stored in config
    #[test]
    fn test_dns_config_with_resolver() {
        let resolver: std::net::SocketAddr = "8.8.8.8:53".parse().unwrap();
        let cfg = DnsSrvConfig::new("_svc._tcp", "internal.corp").with_resolver(resolver);
        assert_eq!(cfg.resolver_addr, Some(resolver));
    }
}
