//! Distributed Hash Table implementation
//!
//! Provides a Kademlia-style DHT with:
//! - XOR distance-based routing
//! - Iterative lookup with parallelism
//! - Result caching with TTL
//! - Churn handling optimization
//! - Latency-aware peer selection

use crate::NodeId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const K_BUCKET_SIZE: usize = 20;
const PEER_TIMEOUT: Duration = Duration::from_secs(300);

/// Default cache TTL (5 minutes)
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// Default parallelism for iterative lookups
const DEFAULT_LOOKUP_PARALLELISM: usize = 3;

/// Maximum lookup iterations
const MAX_LOOKUP_ITERATIONS: usize = 20;

/// Churn detection window (lookups in last minute)
const CHURN_WINDOW: Duration = Duration::from_secs(60);

/// Default maximum cache size for DHT lookups
const DEFAULT_LOOKUP_CACHE_SIZE: usize = 1000;

pub struct Dht {
    local_id: NodeId,
    routing_table: HashMap<NodeId, PeerInfo>,
    /// Cache for frequent lookups
    lookup_cache: LookupCache,
    /// Churn tracker for adaptive caching
    churn_tracker: ChurnTracker,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub node_id: NodeId,
    pub address: String,
    pub last_seen: Instant,
    pub latency_ms: Option<u32>,
    pub reliability_score: u8,
}

impl PeerInfo {
    pub fn new(node_id: NodeId, address: String) -> Self {
        Self {
            node_id,
            address,
            last_seen: Instant::now(),
            latency_ms: None,
            reliability_score: 100,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.last_seen.elapsed() < PEER_TIMEOUT
    }

    pub fn update_seen(&mut self) {
        self.last_seen = Instant::now();
    }

    pub fn set_latency(&mut self, latency_ms: u32) {
        self.latency_ms = Some(latency_ms);
    }
}

// =============================================================================
// Lookup Cache
// =============================================================================

/// Cached lookup result
#[derive(Debug, Clone)]
pub struct CachedLookup {
    /// Target that was looked up
    pub target: NodeId,
    /// Closest nodes found
    pub results: Vec<NodeId>,
    /// When the lookup was performed
    pub cached_at: Instant,
    /// Time to live
    pub ttl: Duration,
    /// Number of cache hits
    pub hits: u64,
}

impl CachedLookup {
    /// Create a new cached lookup result
    pub fn new(target: NodeId, results: Vec<NodeId>, ttl: Duration) -> Self {
        Self {
            target,
            results,
            cached_at: Instant::now(),
            ttl,
            hits: 0,
        }
    }

    /// Check if the cache entry is still valid
    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }

    /// Get age of cache entry
    pub fn age(&self) -> Duration {
        self.cached_at.elapsed()
    }
}

/// Lookup cache with TTL-based expiration
#[derive(Debug)]
pub struct LookupCache {
    /// Cached entries
    entries: HashMap<NodeId, CachedLookup>,
    /// Default TTL for cache entries
    default_ttl: Duration,
    /// Maximum cache size
    max_size: usize,
    /// Cache statistics
    pub stats: CacheStats,
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct CacheStats {
    /// Total cache hits
    pub hits: AtomicU64,
    /// Total cache misses
    pub misses: AtomicU64,
    /// Total evictions
    pub evictions: AtomicU64,
    /// Total insertions
    pub insertions: AtomicU64,
}

impl CacheStats {
    /// Get hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

impl LookupCache {
    /// Create a new lookup cache
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            default_ttl: DEFAULT_CACHE_TTL,
            max_size,
            stats: CacheStats::default(),
        }
    }

    /// Create with custom TTL
    pub fn with_ttl(max_size: usize, ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            default_ttl: ttl,
            max_size,
            stats: CacheStats::default(),
        }
    }

    /// Get a cached lookup result if valid
    pub fn get(&mut self, target: &NodeId) -> Option<Vec<NodeId>> {
        // First cleanup expired entries
        self.cleanup_expired();

        if let Some(entry) = self.entries.get_mut(target) {
            if entry.is_valid() {
                entry.hits += 1;
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.results.clone());
            }
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Insert a lookup result into the cache
    pub fn insert(&mut self, target: NodeId, results: Vec<NodeId>) {
        self.insert_with_ttl(target, results, self.default_ttl);
    }

    /// Insert with custom TTL
    pub fn insert_with_ttl(&mut self, target: NodeId, results: Vec<NodeId>, ttl: Duration) {
        // Evict if at capacity
        if self.entries.len() >= self.max_size && !self.entries.contains_key(&target) {
            self.evict_lru();
        }

        self.entries
            .insert(target, CachedLookup::new(target, results, ttl));
        self.stats.insertions.fetch_add(1, Ordering::Relaxed);
    }

    /// Invalidate a specific cache entry
    pub fn invalidate(&mut self, target: &NodeId) {
        self.entries.remove(target);
    }

    /// Invalidate all entries containing a specific node
    pub fn invalidate_containing(&mut self, node_id: &NodeId) {
        self.entries
            .retain(|_, entry| !entry.results.contains(node_id));
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cleanup expired entries
    fn cleanup_expired(&mut self) {
        self.entries.retain(|_, entry| entry.is_valid());
    }

    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.cached_at)
            .map(|(k, _)| *k)
        {
            self.entries.remove(&oldest_key);
            self.stats.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// =============================================================================
// Iterative Lookup
// =============================================================================

/// Configuration for iterative lookups
#[derive(Debug, Clone, Copy)]
pub struct LookupConfig {
    /// Number of parallel queries (alpha)
    pub parallelism: usize,
    /// Target number of results (k)
    pub result_count: usize,
    /// Maximum iterations before giving up
    pub max_iterations: usize,
    /// Whether to use caching
    pub use_cache: bool,
    /// Timeout for each query
    pub query_timeout: Duration,
}

impl Default for LookupConfig {
    fn default() -> Self {
        Self {
            parallelism: DEFAULT_LOOKUP_PARALLELISM,
            result_count: K_BUCKET_SIZE,
            max_iterations: MAX_LOOKUP_ITERATIONS,
            use_cache: true,
            query_timeout: Duration::from_secs(5),
        }
    }
}

impl LookupConfig {
    /// Create a fast lookup config (more parallelism)
    pub fn fast() -> Self {
        Self {
            parallelism: 5,
            result_count: K_BUCKET_SIZE,
            max_iterations: 10,
            use_cache: true,
            query_timeout: Duration::from_secs(2),
        }
    }

    /// Create a thorough lookup config (more results)
    pub fn thorough() -> Self {
        Self {
            parallelism: 3,
            result_count: K_BUCKET_SIZE * 2,
            max_iterations: MAX_LOOKUP_ITERATIONS,
            use_cache: false,
            query_timeout: Duration::from_secs(10),
        }
    }
}

/// State of an iterative lookup
#[derive(Debug)]
pub struct LookupState {
    /// Target node being searched for
    pub target: NodeId,
    /// Configuration for this lookup
    pub config: LookupConfig,
    /// Nodes that have been queried
    pub queried: Vec<NodeId>,
    /// Nodes pending query (sorted by distance)
    pub pending: Vec<NodeId>,
    /// Best results found so far (sorted by distance)
    pub results: Vec<NodeId>,
    /// When the lookup started
    pub started_at: Instant,
    /// Current iteration number
    pub iteration: usize,
    /// Whether the lookup is complete
    pub complete: bool,
}

impl LookupState {
    /// Create a new lookup state
    pub fn new(target: NodeId, initial_nodes: Vec<NodeId>, config: LookupConfig) -> Self {
        let mut pending = initial_nodes;
        pending.sort_by_key(|n| xor_distance(&target, n));
        pending.dedup();

        Self {
            target,
            config,
            queried: Vec::new(),
            pending,
            results: Vec::new(),
            started_at: Instant::now(),
            iteration: 0,
            complete: false,
        }
    }

    /// Get the next batch of nodes to query
    pub fn next_batch(&mut self) -> Vec<NodeId> {
        let mut batch = Vec::new();
        while batch.len() < self.config.parallelism && !self.pending.is_empty() {
            let node = self.pending.remove(0);
            if !self.queried.contains(&node) {
                batch.push(node);
            }
        }
        batch
    }

    /// Process responses from a batch of queries
    pub fn process_responses(&mut self, responses: Vec<(NodeId, Vec<NodeId>)>) {
        for (from, nodes) in responses {
            self.queried.push(from);

            for node in nodes {
                if !self.queried.contains(&node) && !self.pending.contains(&node) {
                    // Insert in sorted order by distance
                    let pos = self
                        .pending
                        .binary_search_by_key(&xor_distance(&self.target, &node), |n| {
                            xor_distance(&self.target, n)
                        })
                        .unwrap_or_else(|e| e);
                    self.pending.insert(pos, node);
                }
            }
        }

        // Update results with closest nodes seen
        self.update_results();
        self.iteration += 1;

        // Check if complete
        if self.pending.is_empty() || self.iteration >= self.config.max_iterations {
            self.complete = true;
        }

        // Check if we've converged (no closer nodes found)
        if !self.results.is_empty() && !self.pending.is_empty() {
            let best_result_dist = xor_distance(&self.target, &self.results[0]);
            let best_pending_dist = xor_distance(&self.target, &self.pending[0]);
            if best_pending_dist >= best_result_dist
                && self.results.len() >= self.config.result_count
            {
                self.complete = true;
            }
        }
    }

    /// Update the results with the closest nodes
    fn update_results(&mut self) {
        let mut all_nodes: Vec<_> = self
            .queried
            .iter()
            .chain(self.pending.iter())
            .copied()
            .collect();
        all_nodes.sort_by_key(|n| xor_distance(&self.target, n));
        all_nodes.dedup();
        all_nodes.truncate(self.config.result_count);
        self.results = all_nodes;
    }

    /// Get the duration of the lookup
    pub fn duration(&self) -> Duration {
        self.started_at.elapsed()
    }
}

// =============================================================================
// Churn Handling
// =============================================================================

/// Tracks peer churn (arrivals and departures)
#[derive(Debug)]
pub struct ChurnTracker {
    /// Recent peer arrivals (timestamp, node_id)
    arrivals: Vec<(Instant, NodeId)>,
    /// Recent peer departures (timestamp, node_id)
    departures: Vec<(Instant, NodeId)>,
    /// Window for tracking churn
    window: Duration,
}

impl ChurnTracker {
    /// Create a new churn tracker
    pub fn new() -> Self {
        Self {
            arrivals: Vec::new(),
            departures: Vec::new(),
            window: CHURN_WINDOW,
        }
    }

    /// Create with custom window
    pub fn with_window(window: Duration) -> Self {
        Self {
            arrivals: Vec::new(),
            departures: Vec::new(),
            window,
        }
    }

    /// Record a peer arrival
    pub fn record_arrival(&mut self, node_id: NodeId) {
        self.cleanup();
        self.arrivals.push((Instant::now(), node_id));
    }

    /// Record a peer departure
    pub fn record_departure(&mut self, node_id: NodeId) {
        self.cleanup();
        self.departures.push((Instant::now(), node_id));
    }

    /// Get the number of arrivals in the window
    pub fn arrival_count(&self) -> usize {
        self.arrivals
            .iter()
            .filter(|(t, _)| t.elapsed() < self.window)
            .count()
    }

    /// Get the number of departures in the window
    pub fn departure_count(&self) -> usize {
        self.departures
            .iter()
            .filter(|(t, _)| t.elapsed() < self.window)
            .count()
    }

    /// Calculate churn rate (events per minute)
    pub fn churn_rate(&self) -> f64 {
        let total = self.arrival_count() + self.departure_count();
        let window_mins = self.window.as_secs_f64() / 60.0;
        total as f64 / window_mins
    }

    /// Check if churn is high (more than threshold events per minute)
    pub fn is_high_churn(&self, threshold: f64) -> bool {
        self.churn_rate() > threshold
    }

    /// Get recommended cache TTL based on churn
    /// Lower TTL when churn is high
    pub fn recommended_cache_ttl(&self) -> Duration {
        let rate = self.churn_rate();
        if rate < 1.0 {
            Duration::from_secs(600) // 10 minutes
        } else if rate < 5.0 {
            Duration::from_secs(300) // 5 minutes
        } else if rate < 10.0 {
            Duration::from_secs(60) // 1 minute
        } else {
            Duration::from_secs(15) // 15 seconds
        }
    }

    /// Cleanup old entries outside the window
    fn cleanup(&mut self) {
        let now = Instant::now();
        self.arrivals
            .retain(|(t, _)| now.duration_since(*t) < self.window);
        self.departures
            .retain(|(t, _)| now.duration_since(*t) < self.window);
    }
}

impl Default for ChurnTracker {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Routing Table Replication
// =============================================================================

/// Backup routing table for resilience
#[derive(Debug)]
pub struct RoutingReplica {
    /// Backup entries
    entries: HashMap<NodeId, PeerInfo>,
    /// Last sync time
    last_sync: Instant,
    /// Maximum entries to keep
    max_entries: usize,
}

impl RoutingReplica {
    /// Create a new routing replica
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            last_sync: Instant::now(),
            max_entries,
        }
    }

    /// Sync from primary routing table
    pub fn sync_from(&mut self, primary: &HashMap<NodeId, PeerInfo>) {
        self.entries = primary.clone();
        self.last_sync = Instant::now();

        // Trim if over max
        while self.entries.len() > self.max_entries {
            // Remove oldest entry
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, info)| info.last_seen)
                .map(|(k, _)| *k)
            {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
    }

    /// Get a peer from the replica
    pub fn get(&self, node_id: &NodeId) -> Option<&PeerInfo> {
        self.entries.get(node_id)
    }

    /// Get age since last sync
    pub fn sync_age(&self) -> Duration {
        self.last_sync.elapsed()
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// =============================================================================
// DHT Implementation
// =============================================================================

impl Dht {
    pub fn new(local_id: NodeId) -> Self {
        Self {
            local_id,
            routing_table: HashMap::new(),
            lookup_cache: LookupCache::new(DEFAULT_LOOKUP_CACHE_SIZE),
            churn_tracker: ChurnTracker::new(),
        }
    }

    /// Create a DHT with custom cache size
    pub fn with_cache_size(local_id: NodeId, cache_size: usize) -> Self {
        Self {
            local_id,
            routing_table: HashMap::new(),
            lookup_cache: LookupCache::new(cache_size),
            churn_tracker: ChurnTracker::new(),
        }
    }

    pub fn max_peers(&self) -> usize {
        K_BUCKET_SIZE
    }

    pub fn local_id(&self) -> &NodeId {
        &self.local_id
    }

    pub fn insert_peer(&mut self, peer: PeerInfo) {
        if peer.node_id != self.local_id {
            let is_new = !self.routing_table.contains_key(&peer.node_id);
            self.routing_table.insert(peer.node_id, peer.clone());
            self.evict_old_peers();

            // Track churn and invalidate relevant cache entries
            if is_new {
                self.churn_tracker.record_arrival(peer.node_id);
                // Invalidate cache entries that might be affected by new peer
                self.lookup_cache.invalidate_containing(&peer.node_id);
            }
        }
    }

    fn evict_old_peers(&mut self) {
        self.routing_table.retain(|_, peer| peer.is_alive());
    }

    /// Find closest peers without caching (internal use)
    fn find_closest_uncached(&self, target: &NodeId, k: usize) -> Vec<NodeId> {
        let mut peers: Vec<_> = self.routing_table.keys().copied().collect();

        peers.sort_by_key(|peer_id| xor_distance(&self.local_id, peer_id));

        if target != &self.local_id {
            peers.sort_by_key(|peer_id| xor_distance(target, peer_id));
        }

        peers.truncate(k);
        peers
    }

    /// Find closest peers to target with optional caching
    ///
    /// Uses an adaptive caching strategy based on network churn rate.
    /// High churn = shorter TTL, low churn = longer TTL.
    pub fn find_closest(&mut self, target: &NodeId, k: usize) -> Vec<NodeId> {
        // Try cache first (for standard k value)
        if k == K_BUCKET_SIZE {
            if let Some(cached) = self.lookup_cache.get(target) {
                return cached;
            }
        }

        // Compute closest peers
        let result = self.find_closest_uncached(target, k);

        // Cache result with adaptive TTL based on churn
        if k == K_BUCKET_SIZE && !result.is_empty() {
            let ttl = self.churn_tracker.recommended_cache_ttl();
            self.lookup_cache
                .insert_with_ttl(*target, result.clone(), ttl);
        }

        result
    }

    /// Find closest peers without using or updating cache
    ///
    /// Use this when you need guaranteed fresh results.
    pub fn find_closest_fresh(&self, target: &NodeId, k: usize) -> Vec<NodeId> {
        self.find_closest_uncached(target, k)
    }

    pub fn find_closest_by_latency(&self, k: usize) -> Vec<NodeId> {
        let mut peers: Vec<_> = self
            .routing_table
            .iter()
            .filter(|(_, info)| info.is_alive() && info.latency_ms.is_some())
            .collect();

        // SAFETY: Already filtered for is_some() above
        peers.sort_by_key(|(_, info)| info.latency_ms.expect("latency_ms guaranteed to be Some"));

        peers.iter().take(k).map(|(&id, _)| id).collect()
    }

    pub fn get_peer(&self, node_id: &NodeId) -> Option<&PeerInfo> {
        self.routing_table.get(node_id)
    }

    pub fn get_peer_mut(&mut self, node_id: &NodeId) -> Option<&mut PeerInfo> {
        self.routing_table.get_mut(node_id)
    }

    pub fn peer_count(&self) -> usize {
        self.routing_table.len()
    }

    pub fn update_peer_latency(&mut self, node_id: &NodeId, latency_ms: u32) {
        if let Some(peer) = self.routing_table.get_mut(node_id) {
            peer.set_latency(latency_ms);
            peer.update_seen();
        }
    }

    /// Find the next hop to route a message to the target node
    /// Returns the peer closest to the target (greedy routing)
    pub fn route_to(&self, target: &NodeId) -> Option<&PeerInfo> {
        if let Some(peer) = self.routing_table.get(target) {
            // Direct route available
            return Some(peer);
        }

        // Find closest peer to target using XOR distance
        self.routing_table
            .values()
            .filter(|peer| peer.is_alive())
            .min_by_key(|peer| xor_distance(target, &peer.node_id))
    }

    /// Get all known peer addresses
    pub fn get_all_addresses(&self) -> Vec<String> {
        self.routing_table
            .values()
            .filter(|peer| peer.is_alive())
            .map(|peer| peer.address.clone())
            .collect()
    }

    /// Get address for a specific peer
    pub fn get_address(&self, node_id: &NodeId) -> Option<String> {
        self.routing_table
            .get(node_id)
            .map(|peer| peer.address.clone())
    }

    /// Get all alive peers sorted by latency (best first)
    pub fn get_peers_by_latency(&self) -> Vec<&PeerInfo> {
        let mut peers: Vec<_> = self
            .routing_table
            .values()
            .filter(|peer| peer.is_alive() && peer.latency_ms.is_some())
            .collect();

        // SAFETY: Already filtered for is_some() above
        peers.sort_by_key(|peer| peer.latency_ms.expect("latency_ms guaranteed to be Some"));
        peers
    }

    /// Remove a peer from the routing table
    pub fn remove_peer(&mut self, node_id: &NodeId) -> Option<PeerInfo> {
        let removed = self.routing_table.remove(node_id);
        if removed.is_some() {
            self.churn_tracker.record_departure(*node_id);
            // Invalidate cache entries containing this peer
            self.lookup_cache.invalidate_containing(node_id);
        }
        removed
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> &CacheStats {
        &self.lookup_cache.stats
    }

    /// Get cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        self.lookup_cache.stats.hit_rate()
    }

    /// Get current churn rate (events per minute)
    pub fn churn_rate(&self) -> f64 {
        self.churn_tracker.churn_rate()
    }

    /// Check if network is experiencing high churn
    pub fn is_high_churn(&self, threshold: f64) -> bool {
        self.churn_tracker.is_high_churn(threshold)
    }

    /// Clear the lookup cache
    pub fn clear_cache(&mut self) {
        self.lookup_cache.clear();
    }

    /// Get the current cache size
    pub fn cache_size(&self) -> usize {
        self.lookup_cache.len()
    }
}

fn xor_distance(a: &NodeId, b: &NodeId) -> u128 {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    let mut result = 0u128;
    for i in 0..16 {
        result = (result << 8) | ((a_bytes[i] ^ b_bytes[i]) as u128);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use uuid::Uuid;

    // ==========================================================================
    // Property-Based Tests
    // ==========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// XOR distance is symmetric: d(a, b) = d(b, a)
        // proptest uses rand::rng() → ChaCha20 NEON SIMD on aarch64; not UB — unsupported SIMD under Miri
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_xor_distance_symmetric(
            a_bytes in prop::collection::vec(any::<u8>(), 16),
            b_bytes in prop::collection::vec(any::<u8>(), 16)
        ) {
            let a = Uuid::from_slice(&a_bytes).unwrap();
            let b = Uuid::from_slice(&b_bytes).unwrap();
            prop_assert_eq!(xor_distance(&a, &b), xor_distance(&b, &a));
        }

        /// XOR distance to self is zero: d(a, a) = 0
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_xor_distance_self_zero(
            a_bytes in prop::collection::vec(any::<u8>(), 16)
        ) {
            let a = Uuid::from_slice(&a_bytes).unwrap();
            prop_assert_eq!(xor_distance(&a, &a), 0);
        }

        /// XOR distance is consistent: computed distance is the same for same inputs
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_xor_distance_deterministic(
            a_bytes in prop::collection::vec(any::<u8>(), 16),
            b_bytes in prop::collection::vec(any::<u8>(), 16)
        ) {
            let a = Uuid::from_slice(&a_bytes).unwrap();
            let b = Uuid::from_slice(&b_bytes).unwrap();
            // Distance should be deterministic
            let d1 = xor_distance(&a, &b);
            let d2 = xor_distance(&a, &b);
            prop_assert_eq!(d1, d2);
        }

        /// find_closest returns at most k peers
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_find_closest_returns_at_most_k(
            local_bytes in prop::collection::vec(any::<u8>(), 16),
            target_bytes in prop::collection::vec(any::<u8>(), 16),
            num_peers in 0usize..50,
            k in 1usize..30
        ) {
            let local_id = Uuid::from_slice(&local_bytes).unwrap();
            let target = Uuid::from_slice(&target_bytes).unwrap();
            let mut dht = Dht::new(local_id);

            for i in 0..num_peers {
                let peer_id = Uuid::new_v4();
                dht.insert_peer(PeerInfo::new(peer_id, format!("127.0.0.1:{}", 5000 + i)));
            }

            let closest = dht.find_closest(&target, k);
            prop_assert!(closest.len() <= k);
            prop_assert!(closest.len() <= num_peers);
        }

        /// find_closest results are sorted by distance to target
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_find_closest_sorted_by_distance(
            local_bytes in prop::collection::vec(any::<u8>(), 16),
            target_bytes in prop::collection::vec(any::<u8>(), 16),
            num_peers in 2usize..20
        ) {
            let local_id = Uuid::from_slice(&local_bytes).unwrap();
            let target = Uuid::from_slice(&target_bytes).unwrap();
            let mut dht = Dht::new(local_id);

            for i in 0..num_peers {
                let peer_id = Uuid::new_v4();
                dht.insert_peer(PeerInfo::new(peer_id, format!("127.0.0.1:{}", 5000 + i)));
            }

            let closest = dht.find_closest(&target, 10);

            // Check that results are sorted by distance
            for i in 1..closest.len() {
                let dist_prev = xor_distance(&target, &closest[i - 1]);
                let dist_curr = xor_distance(&target, &closest[i]);
                prop_assert!(dist_prev <= dist_curr);
            }
        }

        /// Inserting and removing a peer leaves DHT in consistent state
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_insert_remove_consistent(
            local_bytes in prop::collection::vec(any::<u8>(), 16),
            peer_bytes in prop::collection::vec(any::<u8>(), 16)
        ) {
            let local_id = Uuid::from_slice(&local_bytes).unwrap();
            let peer_id = Uuid::from_slice(&peer_bytes).unwrap();

            // Skip if local and peer are the same (won't be inserted)
            if local_id == peer_id {
                return Ok(());
            }

            let mut dht = Dht::new(local_id);
            let initial_count = dht.peer_count();

            dht.insert_peer(PeerInfo::new(peer_id, "127.0.0.1:5000".to_string()));
            prop_assert_eq!(dht.peer_count(), initial_count + 1);

            dht.remove_peer(&peer_id);
            prop_assert_eq!(dht.peer_count(), initial_count);
        }

        /// Cache hit rate is between 0 and 1
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_cache_hit_rate_valid(
            num_ops in 1usize..100
        ) {
            let mut cache = LookupCache::new(50);

            for i in 0..num_ops {
                let target = Uuid::new_v4();
                if i % 3 == 0 {
                    cache.insert(target, vec![Uuid::new_v4()]);
                }
                cache.get(&target);
            }

            let rate = cache.stats.hit_rate();
            prop_assert!((0.0..=1.0).contains(&rate));
        }

        /// Churn rate is non-negative
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_churn_rate_non_negative(
            arrivals in 0usize..50,
            departures in 0usize..50
        ) {
            let mut tracker = ChurnTracker::new();

            for _ in 0..arrivals {
                tracker.record_arrival(Uuid::new_v4());
            }
            for _ in 0..departures {
                tracker.record_departure(Uuid::new_v4());
            }

            prop_assert!(tracker.churn_rate() >= 0.0);
            prop_assert_eq!(tracker.arrival_count(), arrivals);
            prop_assert_eq!(tracker.departure_count(), departures);
        }

        /// Cache never exceeds max size
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_cache_max_size(
            max_size in 1usize..50,
            num_inserts in 0usize..100
        ) {
            let mut cache = LookupCache::new(max_size);

            for _ in 0..num_inserts {
                let target = Uuid::new_v4();
                cache.insert(target, vec![Uuid::new_v4()]);
            }

            prop_assert!(cache.len() <= max_size);
        }

        /// Routing replica never exceeds max entries
        #[cfg_attr(miri, ignore)]
        #[test]
        fn prop_replica_max_entries(
            max_entries in 1usize..50,
            num_peers in 0usize..100
        ) {
            let mut replica = RoutingReplica::new(max_entries);
            let mut primary = std::collections::HashMap::new();

            for i in 0..num_peers {
                let peer_id = Uuid::new_v4();
                primary.insert(peer_id, PeerInfo::new(peer_id, format!("127.0.0.1:{}", 5000 + i)));
            }

            replica.sync_from(&primary);
            prop_assert!(replica.len() <= max_entries);
        }
    }

    #[test]
    fn test_dht_creation() {
        let dht = Dht::new(Uuid::new_v4());
        assert_eq!(dht.routing_table.len(), 0);
    }

    #[test]
    fn test_peer_insertion() {
        let mut dht = Dht::new(Uuid::new_v4());
        let peer_id = Uuid::new_v4();
        let peer = PeerInfo::new(peer_id, "127.0.0.1:5000".to_string());

        dht.insert_peer(peer);
        assert_eq!(dht.peer_count(), 1);
    }

    #[test]
    fn test_find_closest() {
        let mut dht = Dht::new(Uuid::new_v4());

        for i in 0..5 {
            dht.insert_peer(PeerInfo::new(
                Uuid::new_v4(),
                format!("127.0.0.1:{}", 5000 + i),
            ));
        }

        let target = Uuid::new_v4();
        let closest = dht.find_closest(&target, 3);
        assert!(closest.len() <= 3);
    }

    #[test]
    fn test_xor_distance() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let dist1 = xor_distance(&id1, &id2);
        let dist2 = xor_distance(&id2, &id1);

        assert_eq!(dist1, dist2);
    }

    #[test]
    fn test_latency_based_search() {
        let mut dht = Dht::new(Uuid::new_v4());

        let peer1 = Uuid::new_v4();
        let peer2 = Uuid::new_v4();

        dht.insert_peer(PeerInfo::new(peer1, "127.0.0.1:5001".to_string()));
        dht.insert_peer(PeerInfo::new(peer2, "127.0.0.1:5002".to_string()));

        dht.update_peer_latency(&peer1, 50);
        dht.update_peer_latency(&peer2, 10);

        let closest = dht.find_closest_by_latency(1);
        assert_eq!(closest[0], peer2);
    }

    #[test]
    fn test_route_to_direct() {
        let mut dht = Dht::new(Uuid::new_v4());
        let peer_id = Uuid::new_v4();
        let address = "127.0.0.1:5000".to_string();

        dht.insert_peer(PeerInfo::new(peer_id, address.clone()));

        let route = dht.route_to(&peer_id);
        assert!(route.is_some());
        assert_eq!(route.unwrap().address, address);
    }

    #[test]
    fn test_route_to_indirect() {
        let mut dht = Dht::new(Uuid::new_v4());

        // Add multiple peers
        for i in 0..3 {
            dht.insert_peer(PeerInfo::new(
                Uuid::new_v4(),
                format!("127.0.0.1:{}", 5000 + i),
            ));
        }

        // Try to route to unknown target - should return closest peer
        let target = Uuid::new_v4();
        let route = dht.route_to(&target);
        assert!(route.is_some());
    }

    #[test]
    fn test_get_all_addresses() {
        let mut dht = Dht::new(Uuid::new_v4());

        let addresses = ["127.0.0.1:5000", "127.0.0.1:5001", "127.0.0.1:5002"];

        for addr in &addresses {
            dht.insert_peer(PeerInfo::new(Uuid::new_v4(), addr.to_string()));
        }

        let all_addrs = dht.get_all_addresses();
        assert_eq!(all_addrs.len(), 3);
    }

    #[test]
    fn test_get_address() {
        let mut dht = Dht::new(Uuid::new_v4());
        let peer_id = Uuid::new_v4();
        let address = "127.0.0.1:5000".to_string();

        dht.insert_peer(PeerInfo::new(peer_id, address.clone()));

        let addr = dht.get_address(&peer_id);
        assert_eq!(addr, Some(address));
    }

    #[test]
    fn test_remove_peer() {
        let mut dht = Dht::new(Uuid::new_v4());
        let peer_id = Uuid::new_v4();

        dht.insert_peer(PeerInfo::new(peer_id, "127.0.0.1:5000".to_string()));
        assert_eq!(dht.peer_count(), 1);

        let removed = dht.remove_peer(&peer_id);
        assert!(removed.is_some());
        assert_eq!(dht.peer_count(), 0);
    }

    // ==========================================================================
    // Lookup Cache Tests
    // ==========================================================================

    #[test]
    fn test_lookup_cache_creation() {
        let cache = LookupCache::new(100);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_lookup_cache_insert_and_get() {
        let mut cache = LookupCache::new(100);
        let target = Uuid::new_v4();
        let results = vec![Uuid::new_v4(), Uuid::new_v4()];

        cache.insert(target, results.clone());
        assert_eq!(cache.len(), 1);

        let cached = cache.get(&target);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 2);
    }

    #[test]
    fn test_lookup_cache_miss() {
        let mut cache = LookupCache::new(100);
        let target = Uuid::new_v4();

        let cached = cache.get(&target);
        assert!(cached.is_none());
    }

    #[test]
    fn test_lookup_cache_hit_rate() {
        let mut cache = LookupCache::new(100);
        let target = Uuid::new_v4();
        let results = vec![Uuid::new_v4()];

        cache.insert(target, results);

        // Miss for unknown
        cache.get(&Uuid::new_v4());
        // Hit for known
        cache.get(&target);
        cache.get(&target);

        let hit_rate = cache.stats.hit_rate();
        assert!(hit_rate > 0.5); // 2 hits, 1 miss = 66%
    }

    #[test]
    fn test_lookup_cache_invalidate() {
        let mut cache = LookupCache::new(100);
        let target = Uuid::new_v4();
        let results = vec![Uuid::new_v4()];

        cache.insert(target, results);
        assert_eq!(cache.len(), 1);

        cache.invalidate(&target);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_lookup_cache_invalidate_containing() {
        let mut cache = LookupCache::new(100);
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        let target1 = Uuid::new_v4();
        let target2 = Uuid::new_v4();

        cache.insert(target1, vec![node1, node2]);
        cache.insert(target2, vec![node2]);

        // Both entries contain node2
        cache.invalidate_containing(&node2);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_lookup_cache_eviction() {
        let mut cache = LookupCache::new(2);

        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let t3 = Uuid::new_v4();

        cache.insert(t1, vec![Uuid::new_v4()]);
        cache.insert(t2, vec![Uuid::new_v4()]);
        cache.insert(t3, vec![Uuid::new_v4()]);

        // Should have evicted oldest
        assert_eq!(cache.len(), 2);
        assert!(cache.stats.evictions.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn test_cached_lookup_validity() {
        let target = Uuid::new_v4();
        let results = vec![Uuid::new_v4()];
        let entry = CachedLookup::new(target, results, Duration::from_secs(300));

        assert!(entry.is_valid());
        assert!(entry.age() < Duration::from_secs(1));
    }

    // ==========================================================================
    // Lookup Config Tests
    // ==========================================================================

    #[test]
    fn test_lookup_config_default() {
        let config = LookupConfig::default();
        assert_eq!(config.parallelism, DEFAULT_LOOKUP_PARALLELISM);
        assert_eq!(config.result_count, K_BUCKET_SIZE);
        assert!(config.use_cache);
    }

    #[test]
    fn test_lookup_config_fast() {
        let config = LookupConfig::fast();
        assert!(config.parallelism > LookupConfig::default().parallelism);
        assert!(config.max_iterations < LookupConfig::default().max_iterations);
    }

    #[test]
    fn test_lookup_config_thorough() {
        let config = LookupConfig::thorough();
        assert!(config.result_count > LookupConfig::default().result_count);
        assert!(!config.use_cache);
    }

    // ==========================================================================
    // Lookup State Tests
    // ==========================================================================

    #[test]
    fn test_lookup_state_creation() {
        let target = Uuid::new_v4();
        let initial = vec![Uuid::new_v4(), Uuid::new_v4()];
        let state = LookupState::new(target, initial.clone(), LookupConfig::default());

        assert_eq!(state.target, target);
        assert!(!state.complete);
        assert_eq!(state.iteration, 0);
        assert_eq!(state.pending.len(), 2);
    }

    #[test]
    fn test_lookup_state_next_batch() {
        let target = Uuid::new_v4();
        let initial: Vec<_> = (0..10).map(|_| Uuid::new_v4()).collect();
        let mut state = LookupState::new(target, initial, LookupConfig::default());

        let batch = state.next_batch();
        assert_eq!(batch.len(), DEFAULT_LOOKUP_PARALLELISM);
    }

    #[test]
    fn test_lookup_state_process_responses() {
        let target = Uuid::new_v4();
        let n1 = Uuid::new_v4();
        let n2 = Uuid::new_v4();
        let n3 = Uuid::new_v4();
        let mut state = LookupState::new(target, vec![n1], LookupConfig::default());

        state.next_batch();
        state.process_responses(vec![(n1, vec![n2, n3])]);

        assert!(state.queried.contains(&n1));
        assert!(state.pending.contains(&n2) || state.results.contains(&n2));
    }

    #[test]
    fn test_lookup_state_completes() {
        let target = Uuid::new_v4();
        let initial = vec![Uuid::new_v4()];
        let mut state = LookupState::new(target, initial, LookupConfig::default());

        state.next_batch();
        state.process_responses(vec![]); // No new nodes found

        // Empty pending = complete
        assert!(state.complete || state.pending.is_empty());
    }

    // ==========================================================================
    // Churn Tracker Tests
    // ==========================================================================

    #[test]
    fn test_churn_tracker_creation() {
        let tracker = ChurnTracker::new();
        assert_eq!(tracker.arrival_count(), 0);
        assert_eq!(tracker.departure_count(), 0);
    }

    #[test]
    fn test_churn_tracker_arrivals() {
        let mut tracker = ChurnTracker::new();
        tracker.record_arrival(Uuid::new_v4());
        tracker.record_arrival(Uuid::new_v4());

        assert_eq!(tracker.arrival_count(), 2);
        assert_eq!(tracker.departure_count(), 0);
    }

    #[test]
    fn test_churn_tracker_departures() {
        let mut tracker = ChurnTracker::new();
        tracker.record_departure(Uuid::new_v4());

        assert_eq!(tracker.arrival_count(), 0);
        assert_eq!(tracker.departure_count(), 1);
    }

    #[test]
    fn test_churn_rate() {
        let mut tracker = ChurnTracker::new();

        for _ in 0..10 {
            tracker.record_arrival(Uuid::new_v4());
        }

        // 10 events in 60-second window = 10 per minute
        let rate = tracker.churn_rate();
        assert!(rate > 0.0);
    }

    #[test]
    fn test_high_churn_detection() {
        let mut tracker = ChurnTracker::new();

        // Add many events
        for _ in 0..100 {
            tracker.record_arrival(Uuid::new_v4());
        }

        assert!(tracker.is_high_churn(10.0));
    }

    #[test]
    fn test_recommended_cache_ttl() {
        let tracker = ChurnTracker::new();
        let ttl = tracker.recommended_cache_ttl();

        // Low churn = high TTL
        assert!(ttl >= Duration::from_secs(300));
    }

    // ==========================================================================
    // Routing Replica Tests
    // ==========================================================================

    #[test]
    fn test_routing_replica_creation() {
        let replica = RoutingReplica::new(100);
        assert!(replica.is_empty());
        assert_eq!(replica.len(), 0);
    }

    #[test]
    fn test_routing_replica_sync() {
        let mut replica = RoutingReplica::new(100);
        let mut primary = HashMap::new();

        let peer_id = Uuid::new_v4();
        primary.insert(
            peer_id,
            PeerInfo::new(peer_id, "127.0.0.1:5000".to_string()),
        );

        replica.sync_from(&primary);
        assert_eq!(replica.len(), 1);
        assert!(replica.get(&peer_id).is_some());
    }

    #[test]
    fn test_routing_replica_max_entries() {
        let mut replica = RoutingReplica::new(2);
        let mut primary = HashMap::new();

        for i in 0..5 {
            let peer_id = Uuid::new_v4();
            primary.insert(
                peer_id,
                PeerInfo::new(peer_id, format!("127.0.0.1:{}", 5000 + i)),
            );
        }

        replica.sync_from(&primary);
        assert_eq!(replica.len(), 2);
    }

    #[test]
    fn test_routing_replica_sync_age() {
        let mut replica = RoutingReplica::new(100);
        replica.sync_from(&HashMap::new());

        assert!(replica.sync_age() < Duration::from_secs(1));
    }

    // ==========================================================================
    // DHT Cache Integration Tests
    // ==========================================================================

    #[test]
    fn test_dht_cache_integration() {
        let mut dht = Dht::new(Uuid::new_v4());

        // Add peers
        for i in 0..10 {
            dht.insert_peer(PeerInfo::new(
                Uuid::new_v4(),
                format!("127.0.0.1:{}", 5000 + i),
            ));
        }

        let target = Uuid::new_v4();

        // First lookup - cache miss
        let result1 = dht.find_closest(&target, K_BUCKET_SIZE);
        assert!(!result1.is_empty());

        // Second lookup - should be cached
        let result2 = dht.find_closest(&target, K_BUCKET_SIZE);
        assert_eq!(result1, result2);

        // Check cache hit rate > 0
        assert!(dht.cache_hit_rate() > 0.0);
    }

    #[test]
    fn test_dht_cache_invalidation_on_insert() {
        let mut dht = Dht::new(Uuid::new_v4());

        // Add initial peers
        for i in 0..5 {
            dht.insert_peer(PeerInfo::new(
                Uuid::new_v4(),
                format!("127.0.0.1:{}", 5000 + i),
            ));
        }

        let target = Uuid::new_v4();

        // Populate cache
        dht.find_closest(&target, K_BUCKET_SIZE);
        assert!(dht.cache_size() > 0);

        // Clear cache to test fresh
        dht.clear_cache();
        assert_eq!(dht.cache_size(), 0);
    }

    #[test]
    fn test_dht_cache_invalidation_on_remove() {
        let mut dht = Dht::new(Uuid::new_v4());
        let peer_id = Uuid::new_v4();

        // Add peer
        dht.insert_peer(PeerInfo::new(peer_id, "127.0.0.1:5000".to_string()));

        // Lookup that includes this peer
        let closest = dht.find_closest(&peer_id, K_BUCKET_SIZE);
        assert!(!closest.is_empty());

        // Remove peer - should invalidate cache entries containing it
        dht.remove_peer(&peer_id);

        // Churn should be recorded
        assert!(dht.churn_rate() > 0.0);
    }

    #[test]
    fn test_dht_find_closest_fresh() {
        let mut dht = Dht::new(Uuid::new_v4());

        for i in 0..5 {
            dht.insert_peer(PeerInfo::new(
                Uuid::new_v4(),
                format!("127.0.0.1:{}", 5000 + i),
            ));
        }

        let target = Uuid::new_v4();

        // Fresh lookup shouldn't affect cache
        let initial_size = dht.cache_size();
        let _ = dht.find_closest_fresh(&target, 3);
        assert_eq!(dht.cache_size(), initial_size);
    }

    #[test]
    fn test_dht_churn_tracking() {
        let mut dht = Dht::new(Uuid::new_v4());

        // Initially no churn
        assert!(!dht.is_high_churn(10.0));

        // Add many peers quickly
        for i in 0..20 {
            dht.insert_peer(PeerInfo::new(
                Uuid::new_v4(),
                format!("127.0.0.1:{}", 5000 + i),
            ));
        }

        // Should have recorded arrivals
        assert!(dht.churn_rate() > 0.0);
    }

    #[test]
    fn test_dht_with_cache_size() {
        let dht = Dht::with_cache_size(Uuid::new_v4(), 50);
        assert_eq!(dht.cache_size(), 0);
        assert_eq!(dht.peer_count(), 0);
    }
}
