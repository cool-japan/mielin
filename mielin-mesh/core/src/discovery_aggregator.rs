//! Multi-Source Discovery Aggregator
//!
//! Combines results from the three discovery backends — static peer list,
//! DNS SRV, and (in future) mDNS — into a unified, deduplicated stream of
//! `AggregatedPeer` values. Callers consume the aggregated view without
//! needing to know which backend provided each peer.
//!
//! Deduplication is `NodeId`-based: if the same `NodeId` appears in multiple
//! backends the first occurrence (by source priority: Static > DNS > mDNS) is
//! kept. Priority values are assigned per source: lower numeric value means
//! higher preference.

use crate::discovery_dns::{DnsDiscoveryError, DnsSrvDiscovery};
use crate::discovery_static::StaticPeerList;
use crate::NodeId;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// DiscoverySource
// ---------------------------------------------------------------------------

/// Identifies which backend discovered a particular peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscoverySource {
    Static,
    Dns,
    Mdns,
}

impl DiscoverySource {
    /// Numeric priority; lower = preferred when deduplicating.
    fn priority_value(self) -> u32 {
        match self {
            DiscoverySource::Static => 1,
            DiscoverySource::Dns => 2,
            DiscoverySource::Mdns => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// AggregatedPeer
// ---------------------------------------------------------------------------

/// A single discovered peer, annotated with provenance metadata.
#[derive(Debug, Clone)]
pub struct AggregatedPeer {
    pub node_id: NodeId,
    pub address: SocketAddr,
    /// Which backend first reported this peer.
    pub source: DiscoverySource,
    /// Monotonic timestamp of discovery within this aggregation run.
    pub discovered_at: Instant,
    /// Routing preference: lower value = preferred. Derived from source priority
    /// and (for DNS) SRV record priority.
    pub priority: u32,
}

// ---------------------------------------------------------------------------
// AggregatorStats
// ---------------------------------------------------------------------------

/// Cumulative statistics for a `DiscoveryAggregator` instance.
#[derive(Debug, Default, Clone)]
pub struct AggregatorStats {
    /// Total peers reported by the static backend across all `collect_peers` calls.
    pub static_discoveries: u64,
    /// Total peers reported by the DNS backend across all `collect_peers` calls.
    pub dns_discoveries: u64,
    /// Total peers reported by the mDNS backend across all `collect_peers` calls.
    pub mdns_discoveries: u64,
    /// Distinct peers present in the most recent `collect_peers` call.
    pub total_unique_peers: usize,
    /// Peers suppressed due to duplicate `NodeId` in the most recent call.
    pub duplicate_filtered: u64,
}

// ---------------------------------------------------------------------------
// DiscoveryAggregator
// ---------------------------------------------------------------------------

/// Aggregates multiple discovery backends into a unified peer view.
///
/// Build via the builder-style methods:
/// ```ignore
/// let agg = DiscoveryAggregator::new()
///     .with_static_list(Arc::clone(&static_list))
///     .with_dns(Arc::clone(&dns));
/// ```
pub struct DiscoveryAggregator {
    static_list: Option<Arc<StaticPeerList>>,
    dns_discovery: Option<Arc<DnsSrvDiscovery>>,
    /// Minimum interval between back-to-back re-aggregations (currently
    /// informational — callers decide when to call `collect_peers`).
    dedup_window: Duration,
    stats: Arc<RwLock<AggregatorStats>>,
}

impl DiscoveryAggregator {
    /// Create an aggregator with no backends attached yet.
    pub fn new() -> Self {
        Self {
            static_list: None,
            dns_discovery: None,
            dedup_window: Duration::from_secs(5),
            stats: Arc::new(RwLock::new(AggregatorStats::default())),
        }
    }

    /// Attach a static peer list backend.
    pub fn with_static_list(mut self, list: Arc<StaticPeerList>) -> Self {
        self.static_list = Some(list);
        self
    }

    /// Attach a DNS SRV discovery backend.
    pub fn with_dns(mut self, dns: Arc<DnsSrvDiscovery>) -> Self {
        self.dns_discovery = Some(dns);
        self
    }

    /// Override the dedup window (metadata only — no automatic throttling).
    pub fn with_dedup_window(mut self, window: Duration) -> Self {
        self.dedup_window = window;
        self
    }

    /// Returns `true` if at least one backend is attached.
    pub fn has_sources(&self) -> bool {
        self.static_list.is_some() || self.dns_discovery.is_some()
    }

    /// Collect peers from all enabled backends, deduplicate by `NodeId`, and
    /// return the merged list sorted by ascending priority.
    ///
    /// When the same `NodeId` is reported by multiple backends, the one with
    /// the lower source priority value is retained (Static wins over DNS, etc.).
    pub async fn collect_peers(&self) -> Vec<AggregatedPeer> {
        let mut seen: HashMap<NodeId, AggregatedPeer> = HashMap::new();
        let mut static_count: u64 = 0;
        let mut dns_count: u64 = 0;
        let mut duplicates: u64 = 0;

        // 1. Static backend (highest priority)
        if let Some(list) = &self.static_list {
            let healthy = list.healthy_peers().await;
            for peer in healthy {
                let ap = AggregatedPeer {
                    node_id: peer.node_id,
                    address: peer.address,
                    source: DiscoverySource::Static,
                    discovered_at: Instant::now(),
                    priority: DiscoverySource::Static.priority_value(),
                };
                static_count += 1;
                seen.insert(peer.node_id, ap);
            }
        }

        // 2. DNS SRV backend
        if let Some(dns) = &self.dns_discovery {
            let records = match dns.sorted_peers().await {
                Ok(r) => r,
                Err(DnsDiscoveryError::NoRecords { .. }) => vec![],
                Err(_) => vec![],
            };
            for record in &records {
                // Build a SocketAddr from the SRV hostname; since this is an
                // abstraction layer we store port only and use 0.0.0.0 as a
                // placeholder IPv4 address. A real implementation would resolve
                // the A/AAAA record for `record.target`.
                let addr: SocketAddr = format!("0.0.0.0:{}", record.port)
                    .parse()
                    .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

                // Synthesise a deterministic NodeId from the SRV target string
                // (sha-flavoured UUID v5 semantics via uuid's Uuid::new_v5).
                let node_id =
                    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, record.target.as_bytes());

                dns_count += 1;
                let ap = AggregatedPeer {
                    node_id,
                    address: addr,
                    source: DiscoverySource::Dns,
                    discovered_at: Instant::now(),
                    // Blend SRV priority into our ordering (scale by 100 to
                    // leave room above the static range of 1-99).
                    priority: DiscoverySource::Dns.priority_value() * 100 + record.priority as u32,
                };
                seen.entry(node_id)
                    .and_modify(|existing| {
                        duplicates += 1;
                        // Lower priority value wins.
                        if ap.priority < existing.priority {
                            *existing = ap.clone();
                        }
                    })
                    .or_insert(ap);
            }
        }

        // Compute deduplicated count vs. raw total
        let unique_count = seen.len();
        let raw_total = static_count + dns_count;
        let dup = raw_total.saturating_sub(unique_count as u64) + duplicates;

        // Persist stats
        {
            let mut stats = self.stats.write().await;
            stats.static_discoveries = stats.static_discoveries.saturating_add(static_count);
            stats.dns_discoveries = stats.dns_discoveries.saturating_add(dns_count);
            stats.total_unique_peers = unique_count;
            stats.duplicate_filtered = stats.duplicate_filtered.saturating_add(dup);
        }

        let mut peers: Vec<AggregatedPeer> = seen.into_values().collect();
        peers.sort_by_key(|p| p.priority);
        peers
    }

    /// Return the single best peer (lowest priority value) across all backends.
    pub async fn best_peer(&self) -> Option<AggregatedPeer> {
        let mut peers = self.collect_peers().await;
        if peers.is_empty() {
            return None;
        }
        // Peers are already sorted ascending by priority; take the first.
        Some(peers.remove(0))
    }

    /// Get a snapshot of cumulative statistics.
    pub async fn stats(&self) -> AggregatorStats {
        self.stats.read().await.clone()
    }
}

impl Default for DiscoveryAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery_dns::{DnsSrvConfig, DnsSrvDiscovery, DnsSrvRecord};
    use crate::discovery_static::{StaticPeer, StaticPeerList};
    use crate::{Node, NodeRole};
    use std::sync::Arc;

    fn make_node() -> Arc<Node> {
        Arc::new(Node::new(NodeRole::Relay))
    }

    fn make_addr(port: u16) -> std::net::SocketAddr {
        format!("127.0.0.1:{port}").parse().unwrap()
    }

    fn make_static_peer(port: u16) -> StaticPeer {
        StaticPeer::new(uuid::Uuid::new_v4(), make_addr(port))
    }

    fn make_dns_record(priority: u16, weight: u16, port: u16, target: &str) -> DnsSrvRecord {
        DnsSrvRecord::new(priority, weight, port, target)
    }

    fn basic_dns_config() -> DnsSrvConfig {
        DnsSrvConfig::new("_mielin._tcp", "test.local")
    }

    // 23. Empty aggregator has no sources
    #[test]
    fn test_aggregator_empty_has_no_sources() {
        let agg = DiscoveryAggregator::new();
        assert!(!agg.has_sources());
    }

    // 24. Adding a static list marks has_sources = true
    #[test]
    fn test_aggregator_with_static() {
        let list = Arc::new(StaticPeerList::new(make_node()));
        let agg = DiscoveryAggregator::new().with_static_list(list);
        assert!(agg.has_sources());
    }

    // 25. collect_peers returns peers from the static backend
    #[tokio::test]
    async fn test_aggregator_collect_from_static() {
        let list = Arc::new(StaticPeerList::new(make_node()));
        list.add_peer(make_static_peer(7001)).await;
        list.add_peer(make_static_peer(7002)).await;

        let agg = DiscoveryAggregator::new().with_static_list(Arc::clone(&list));
        let peers = agg.collect_peers().await;
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().all(|p| p.source == DiscoverySource::Static));
    }

    // 26. Same NodeId from static + DNS appears only once
    #[tokio::test]
    async fn test_aggregator_dedup() {
        // Build a static peer with a deterministic UUID (same as what DNS
        // will synthesise for the target "shared.test.local").
        let shared_id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, b"shared.test.local");
        let static_list = Arc::new(StaticPeerList::new(make_node()));
        let shared_peer = StaticPeer::new(shared_id, make_addr(9000));
        static_list.add_peer(shared_peer).await;

        // DNS backend with a record whose synthesised UUID will equal shared_id.
        let dns = Arc::new(DnsSrvDiscovery::new(basic_dns_config()));
        dns.inject_records(vec![make_dns_record(1, 100, 9000, "shared.test.local")])
            .await;

        let agg = DiscoveryAggregator::new()
            .with_static_list(Arc::clone(&static_list))
            .with_dns(Arc::clone(&dns));

        let peers = agg.collect_peers().await;
        // Even though both backends report the same NodeId, we should see
        // exactly one entry.
        let count = peers.iter().filter(|p| p.node_id == shared_id).count();
        assert_eq!(count, 1, "Duplicate NodeId must be deduplicated");
    }

    // 27. best_peer returns the highest-priority peer from static list
    #[tokio::test]
    async fn test_aggregator_best_peer_static() {
        let list = Arc::new(StaticPeerList::new(make_node()));
        list.add_peer(make_static_peer(7010)).await;
        list.add_peer(make_static_peer(7011)).await;

        let agg = DiscoveryAggregator::new().with_static_list(Arc::clone(&list));
        let best = agg.best_peer().await;
        assert!(best.is_some());
        assert_eq!(best.unwrap().source, DiscoverySource::Static);
    }

    // 28. stats.static_discoveries increments across multiple collect calls
    #[tokio::test]
    async fn test_aggregator_stats_track_sources() {
        let list = Arc::new(StaticPeerList::new(make_node()));
        list.add_peer(make_static_peer(7020)).await;
        list.add_peer(make_static_peer(7021)).await;

        let agg = DiscoveryAggregator::new().with_static_list(Arc::clone(&list));
        let _ = agg.collect_peers().await;
        let _ = agg.collect_peers().await;

        let stats = agg.stats().await;
        // Each call reported 2 static peers → cumulative should be 4.
        assert_eq!(
            stats.static_discoveries, 4,
            "Expected 2 peers × 2 calls = 4 static_discoveries"
        );
    }
}
