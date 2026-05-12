//! Node Discovery Protocol
//!
//! Provides multiple discovery mechanisms for finding peers in the mesh:
//! - mDNS for local network discovery
//! - Bootstrap node registry for initial connections
//! - Peer exchange protocol for mesh growth

use crate::{NodeRole, PeerDescriptor, WireError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Default mDNS service name
const MDNS_SERVICE_NAME: &str = "_mielin._udp.local";

/// Default mDNS query interval (30 seconds)
const MDNS_QUERY_INTERVAL: Duration = Duration::from_secs(30);

/// Default peer entry TTL (5 minutes)
const PEER_ENTRY_TTL: Duration = Duration::from_secs(300);

/// Default maximum peers to track
const MAX_PEERS: usize = 1000;

/// Node capabilities that can be advertised
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Can run WASM agents
    WasmRuntime,
    /// Can accept agent migrations
    AgentMigration,
    /// Provides relay services
    Relay,
    /// Has GPU acceleration
    GpuAcceleration,
    /// Supports specific architecture
    Architecture(String),
    /// Custom capability
    Custom(String),
}

/// Discovery method used to find a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// Found via mDNS on local network
    Mdns,
    /// Provided as bootstrap node
    Bootstrap,
    /// Received via peer exchange
    PeerExchange,
    /// Manually configured
    Manual,
}

fn now() -> Instant {
    Instant::now()
}

/// Information about a discovered peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    /// Node ID
    pub node_id: [u8; 16],
    /// Network address
    pub address: SocketAddr,
    /// Node role
    pub role: NodeRole,
    /// Advertised capabilities
    pub capabilities: Vec<Capability>,
    /// How this peer was discovered
    pub discovery_method: DiscoveryMethod,
    /// When this peer was first discovered
    #[serde(skip, default = "now")]
    pub discovered_at: Instant,
    /// Last time we received info about this peer
    #[serde(skip, default = "now")]
    pub last_seen: Instant,
    /// Measured latency (if available)
    pub latency_ms: Option<u32>,
    /// Number of successful connections
    pub connection_count: u32,
    /// Number of failed connection attempts
    pub failure_count: u32,
}

impl DiscoveredPeer {
    /// Create a new discovered peer entry
    pub fn new(
        node_id: [u8; 16],
        address: SocketAddr,
        role: NodeRole,
        capabilities: Vec<Capability>,
        method: DiscoveryMethod,
    ) -> Self {
        let now = Instant::now();
        Self {
            node_id,
            address,
            role,
            capabilities,
            discovery_method: method,
            discovered_at: now,
            last_seen: now,
            latency_ms: None,
            connection_count: 0,
            failure_count: 0,
        }
    }

    /// Check if this peer entry has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.last_seen.elapsed() > ttl
    }

    /// Update last seen timestamp
    pub fn touch(&mut self) {
        self.last_seen = Instant::now();
    }

    /// Record a successful connection
    pub fn record_success(&mut self, latency_ms: Option<u32>) {
        self.connection_count += 1;
        self.latency_ms = latency_ms;
        self.touch();
    }

    /// Record a failed connection attempt
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.touch();
    }

    /// Get connection success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.connection_count + self.failure_count;
        if total == 0 {
            1.0 // No data yet, assume good
        } else {
            self.connection_count as f64 / total as f64
        }
    }

    /// Check if peer has a specific capability
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }
}

/// Discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Enable mDNS discovery
    pub enable_mdns: bool,
    /// mDNS service name
    pub mdns_service_name: String,
    /// mDNS query interval
    pub mdns_query_interval: Duration,
    /// Peer entry TTL
    pub peer_ttl: Duration,
    /// Maximum number of peers to track
    pub max_peers: usize,
    /// Bootstrap nodes (initial peers)
    pub bootstrap_nodes: Vec<SocketAddr>,
    /// Enable peer exchange protocol
    pub enable_peer_exchange: bool,
    /// Maximum peers to include in exchange
    pub max_exchange_peers: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_mdns: true,
            mdns_service_name: MDNS_SERVICE_NAME.to_string(),
            mdns_query_interval: MDNS_QUERY_INTERVAL,
            peer_ttl: PEER_ENTRY_TTL,
            max_peers: MAX_PEERS,
            bootstrap_nodes: Vec::new(),
            enable_peer_exchange: true,
            max_exchange_peers: 20,
        }
    }
}

impl DiscoveryConfig {
    /// Create a new discovery configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a bootstrap node
    pub fn add_bootstrap(mut self, addr: SocketAddr) -> Self {
        self.bootstrap_nodes.push(addr);
        self
    }

    /// Disable mDNS discovery
    pub fn disable_mdns(mut self) -> Self {
        self.enable_mdns = false;
        self
    }

    /// Preset for local development
    pub fn local() -> Self {
        Self {
            enable_mdns: true,
            mdns_query_interval: Duration::from_secs(10),
            peer_ttl: Duration::from_secs(60),
            max_peers: 100,
            bootstrap_nodes: Vec::new(),
            enable_peer_exchange: true,
            max_exchange_peers: 10,
            ..Default::default()
        }
    }

    /// Preset for production deployment
    pub fn production() -> Self {
        Self {
            enable_mdns: false, // Production uses bootstrap + peer exchange
            mdns_query_interval: MDNS_QUERY_INTERVAL,
            peer_ttl: PEER_ENTRY_TTL,
            max_peers: MAX_PEERS,
            bootstrap_nodes: Vec::new(),
            enable_peer_exchange: true,
            max_exchange_peers: 50,
            ..Default::default()
        }
    }
}

/// Peer discovery service
pub struct DiscoveryService {
    config: DiscoveryConfig,
    peers: Arc<RwLock<HashMap<[u8; 16], DiscoveredPeer>>>,
    local_node_id: [u8; 16],
    local_capabilities: Vec<Capability>,
}

impl DiscoveryService {
    /// Create a new discovery service
    pub fn new(
        config: DiscoveryConfig,
        local_node_id: [u8; 16],
        capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            local_node_id,
            local_capabilities: capabilities,
        }
    }

    /// Start the discovery service
    pub async fn start(&self) -> Result<(), WireError> {
        info!("Starting discovery service");

        // Add bootstrap nodes
        for addr in &self.config.bootstrap_nodes {
            self.add_bootstrap_peer(*addr).await;
        }

        // Start mDNS if enabled
        if self.config.enable_mdns {
            info!("mDNS discovery enabled");
            // In a real implementation, would start mDNS listener here
        }

        Ok(())
    }

    /// Add a bootstrap peer
    async fn add_bootstrap_peer(&self, addr: SocketAddr) {
        // Generate a temporary node ID for bootstrap nodes
        // In real implementation, would query the node for its actual ID
        let temp_id = Self::generate_temp_id(&addr);

        let peer = DiscoveredPeer::new(
            temp_id,
            addr,
            NodeRole::Core, // Assume bootstrap nodes are core
            vec![],
            DiscoveryMethod::Bootstrap,
        );

        let mut peers = self.peers.write().await;
        peers.insert(temp_id, peer);
        info!("Added bootstrap peer: {}", addr);
    }

    /// Generate a temporary node ID from address (for bootstrap)
    fn generate_temp_id(addr: &SocketAddr) -> [u8; 16] {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        addr.hash(&mut hasher);
        let hash = hasher.finish();

        let mut id = [0u8; 16];
        id[0..8].copy_from_slice(&hash.to_le_bytes());
        id[8..16].copy_from_slice(&hash.to_be_bytes());
        id
    }

    /// Announce this node on the network
    pub async fn announce(&self) -> Result<(), WireError> {
        if self.config.enable_mdns {
            debug!("Announcing node via mDNS");
            // In real implementation, would broadcast mDNS announcement
        }
        Ok(())
    }

    /// Discover peers on the local network
    pub async fn discover_local(&self) -> Result<Vec<DiscoveredPeer>, WireError> {
        if !self.config.enable_mdns {
            return Ok(Vec::new());
        }

        debug!("Querying for local peers via mDNS");
        // In real implementation, would perform mDNS query
        // For now, return cached local peers

        let peers = self.peers.read().await;
        Ok(peers
            .values()
            .filter(|p| p.discovery_method == DiscoveryMethod::Mdns)
            .cloned()
            .collect())
    }

    /// Add a discovered peer
    pub async fn add_peer(&self, peer: DiscoveredPeer) -> Result<(), WireError> {
        let mut peers = self.peers.write().await;

        // Check max peers limit
        if peers.len() >= self.config.max_peers && !peers.contains_key(&peer.node_id) {
            warn!("Peer limit reached, not adding new peer");
            return Err(WireError::TransportError("Peer limit reached".to_string()));
        }

        peers.insert(peer.node_id, peer);
        Ok(())
    }

    /// Get a peer by node ID
    pub async fn get_peer(&self, node_id: &[u8; 16]) -> Option<DiscoveredPeer> {
        self.peers.read().await.get(node_id).cloned()
    }

    /// Get all discovered peers
    pub async fn get_all_peers(&self) -> Vec<DiscoveredPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Get peers with specific capability
    pub async fn get_peers_with_capability(&self, capability: &Capability) -> Vec<DiscoveredPeer> {
        self.peers
            .read()
            .await
            .values()
            .filter(|p| p.has_capability(capability))
            .cloned()
            .collect()
    }

    /// Get peers by role
    pub async fn get_peers_by_role(&self, role: NodeRole) -> Vec<DiscoveredPeer> {
        self.peers
            .read()
            .await
            .values()
            .filter(|p| p.role == role)
            .cloned()
            .collect()
    }

    /// Get best peers (sorted by success rate and latency)
    pub async fn get_best_peers(&self, limit: usize) -> Vec<DiscoveredPeer> {
        let peers = self.peers.read().await;
        let mut peer_list: Vec<_> = peers.values().cloned().collect();

        // Sort by success rate (descending) then latency (ascending)
        peer_list.sort_by(|a, b| {
            let rate_cmp = b.success_rate().partial_cmp(&a.success_rate()).unwrap_or(std::cmp::Ordering::Equal);
            if rate_cmp == std::cmp::Ordering::Equal {
                match (a.latency_ms, b.latency_ms) {
                    (Some(a_lat), Some(b_lat)) => a_lat.cmp(&b_lat),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            } else {
                rate_cmp
            }
        });

        peer_list.into_iter().take(limit).collect()
    }

    /// Update peer with connection result
    pub async fn update_peer_connection(
        &self,
        node_id: &[u8; 16],
        success: bool,
        latency_ms: Option<u32>,
    ) {
        if let Some(peer) = self.peers.write().await.get_mut(node_id) {
            if success {
                peer.record_success(latency_ms);
            } else {
                peer.record_failure();
            }
        }
    }

    /// Clean up expired peers
    pub async fn cleanup_expired(&self) -> usize {
        let mut peers = self.peers.write().await;
        let initial_count = peers.len();

        peers.retain(|_, peer| !peer.is_expired(self.config.peer_ttl));

        let removed = initial_count - peers.len();
        if removed > 0 {
            info!("Cleaned up {} expired peers", removed);
        }
        removed
    }

    /// Get discovery statistics
    pub async fn stats(&self) -> DiscoveryStats {
        let peers = self.peers.read().await;

        let mut mdns_count = 0;
        let mut bootstrap_count = 0;
        let mut exchange_count = 0;
        let mut manual_count = 0;

        for peer in peers.values() {
            match peer.discovery_method {
                DiscoveryMethod::Mdns => mdns_count += 1,
                DiscoveryMethod::Bootstrap => bootstrap_count += 1,
                DiscoveryMethod::PeerExchange => exchange_count += 1,
                DiscoveryMethod::Manual => manual_count += 1,
            }
        }

        DiscoveryStats {
            total_peers: peers.len(),
            mdns_peers: mdns_count,
            bootstrap_peers: bootstrap_count,
            exchange_peers: exchange_count,
            manual_peers: manual_count,
        }
    }

    /// Exchange peers with another node
    pub async fn create_peer_exchange(&self) -> PeerExchange {
        let peers = self.get_best_peers(self.config.max_exchange_peers).await;

        let peer_descriptors: Vec<PeerDescriptor> = peers
            .into_iter()
            .map(|p| PeerDescriptor {
                node_id: p.node_id,
                address: p.address.to_string(),
                latency_ms: p.latency_ms,
            })
            .collect();

        PeerExchange {
            node_id: self.local_node_id,
            peers: peer_descriptors,
            capabilities: self.local_capabilities.clone(),
        }
    }

    /// Process received peer exchange
    pub async fn process_peer_exchange(&self, exchange: PeerExchange) -> usize {
        let mut added = 0;

        for peer_desc in exchange.peers {
            // Skip if this is our own node
            if peer_desc.node_id == self.local_node_id {
                continue;
            }

            // Parse address
            let addr = match peer_desc.address.parse() {
                Ok(a) => a,
                Err(_) => continue,
            };

            // Create peer entry
            let mut peer = DiscoveredPeer::new(
                peer_desc.node_id,
                addr,
                NodeRole::Edge, // Default role, will be updated
                exchange.capabilities.clone(),
                DiscoveryMethod::PeerExchange,
            );

            if let Some(latency) = peer_desc.latency_ms {
                peer.latency_ms = Some(latency);
            }

            if self.add_peer(peer).await.is_ok() {
                added += 1;
            }
        }

        if added > 0 {
            info!("Added {} peers from exchange", added);
        }

        added
    }
}

/// Peer exchange message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerExchange {
    /// Node ID of sender
    pub node_id: [u8; 16],
    /// Peers to share
    pub peers: Vec<PeerDescriptor>,
    /// Sender's capabilities
    pub capabilities: Vec<Capability>,
}

/// Discovery statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryStats {
    pub total_peers: usize,
    pub mdns_peers: usize,
    pub bootstrap_peers: usize,
    pub exchange_peers: usize,
    pub manual_peers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_config_creation() {
        let config = DiscoveryConfig::new();
        assert!(config.enable_mdns);
        assert_eq!(config.max_peers, MAX_PEERS);
    }

    #[test]
    fn test_discovery_config_presets() {
        let local = DiscoveryConfig::local();
        assert!(local.enable_mdns);
        assert_eq!(local.max_peers, 100);

        let prod = DiscoveryConfig::production();
        assert!(!prod.enable_mdns);
        assert_eq!(prod.max_peers, MAX_PEERS);
    }

    #[test]
    fn test_discovered_peer_creation() {
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [1u8; 16],
            addr,
            NodeRole::Edge,
            vec![Capability::WasmRuntime],
            DiscoveryMethod::Mdns,
        );

        assert_eq!(peer.node_id, [1u8; 16]);
        assert_eq!(peer.address, addr);
        assert_eq!(peer.connection_count, 0);
        assert_eq!(peer.failure_count, 0);
    }

    #[test]
    fn test_peer_success_rate() {
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let mut peer = DiscoveredPeer::new(
            [1u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        assert_eq!(peer.success_rate(), 1.0); // No data yet

        peer.record_success(Some(10));
        peer.record_success(Some(15));
        peer.record_failure();

        assert!((peer.success_rate() - 0.6667).abs() < 0.001);
    }

    #[test]
    fn test_peer_expiration() {
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let mut peer = DiscoveredPeer::new(
            [1u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        assert!(!peer.is_expired(Duration::from_secs(1)));

        // Simulate old peer
        peer.last_seen = Instant::now() - Duration::from_secs(10);
        assert!(peer.is_expired(Duration::from_secs(5)));
    }

    #[test]
    fn test_capability_checking() {
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [1u8; 16],
            addr,
            NodeRole::Edge,
            vec![Capability::WasmRuntime, Capability::AgentMigration],
            DiscoveryMethod::Mdns,
        );

        assert!(peer.has_capability(&Capability::WasmRuntime));
        assert!(peer.has_capability(&Capability::AgentMigration));
        assert!(!peer.has_capability(&Capability::GpuAcceleration));
    }

    #[tokio::test]
    async fn test_discovery_service_creation() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let stats = service.stats().await;
        assert_eq!(stats.total_peers, 0);
    }

    #[tokio::test]
    async fn test_add_peer() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [2u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        service.add_peer(peer).await.unwrap();

        let stats = service.stats().await;
        assert_eq!(stats.total_peers, 1);
        assert_eq!(stats.mdns_peers, 1);
    }

    #[tokio::test]
    async fn test_get_peer() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [2u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        service.add_peer(peer).await.unwrap();

        let retrieved = service.get_peer(&[2u8; 16]).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().node_id, [2u8; 16]);
    }

    #[tokio::test]
    async fn test_get_peers_by_role() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let addr1: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer1 = DiscoveredPeer::new(
            [2u8; 16],
            addr1,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        let addr2: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        let peer2 = DiscoveredPeer::new(
            [3u8; 16],
            addr2,
            NodeRole::Core,
            vec![],
            DiscoveryMethod::Bootstrap,
        );

        service.add_peer(peer1).await.unwrap();
        service.add_peer(peer2).await.unwrap();

        let edge_peers = service.get_peers_by_role(NodeRole::Edge).await;
        assert_eq!(edge_peers.len(), 1);

        let core_peers = service.get_peers_by_role(NodeRole::Core).await;
        assert_eq!(core_peers.len(), 1);
    }

    #[tokio::test]
    async fn test_get_peers_with_capability() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let addr1: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer1 = DiscoveredPeer::new(
            [2u8; 16],
            addr1,
            NodeRole::Edge,
            vec![Capability::WasmRuntime],
            DiscoveryMethod::Mdns,
        );

        let addr2: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        let peer2 = DiscoveredPeer::new(
            [3u8; 16],
            addr2,
            NodeRole::Edge,
            vec![Capability::GpuAcceleration],
            DiscoveryMethod::Mdns,
        );

        service.add_peer(peer1).await.unwrap();
        service.add_peer(peer2).await.unwrap();

        let wasm_peers = service
            .get_peers_with_capability(&Capability::WasmRuntime)
            .await;
        assert_eq!(wasm_peers.len(), 1);

        let gpu_peers = service
            .get_peers_with_capability(&Capability::GpuAcceleration)
            .await;
        assert_eq!(gpu_peers.len(), 1);
    }

    #[tokio::test]
    async fn test_update_peer_connection() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [2u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        service.add_peer(peer).await.unwrap();

        service
            .update_peer_connection(&[2u8; 16], true, Some(10))
            .await;

        let updated = service.get_peer(&[2u8; 16]).await.unwrap();
        assert_eq!(updated.connection_count, 1);
        assert_eq!(updated.latency_ms, Some(10));
    }

    #[tokio::test]
    async fn test_get_best_peers() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        // Add peers with different success rates and latencies
        let addr1: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let mut peer1 = DiscoveredPeer::new(
            [2u8; 16],
            addr1,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );
        peer1.record_success(Some(50));
        peer1.record_success(Some(50));

        let addr2: SocketAddr = "127.0.0.1:8001".parse().unwrap();
        let mut peer2 = DiscoveredPeer::new(
            [3u8; 16],
            addr2,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );
        peer2.record_success(Some(10));
        peer2.record_success(Some(10));
        peer2.record_success(Some(10));

        service.add_peer(peer1).await.unwrap();
        service.add_peer(peer2).await.unwrap();

        let best = service.get_best_peers(1).await;
        assert_eq!(best.len(), 1);
        // peer2 should be first (better success rate and lower latency)
        assert_eq!(best[0].node_id, [3u8; 16]);
    }

    #[tokio::test]
    async fn test_peer_exchange() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![Capability::WasmRuntime]);

        // Add some peers
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [2u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );
        service.add_peer(peer).await.unwrap();

        let exchange = service.create_peer_exchange().await;
        assert_eq!(exchange.node_id, [1u8; 16]);
        assert_eq!(exchange.peers.len(), 1);
        assert!(exchange.capabilities.contains(&Capability::WasmRuntime));
    }

    #[tokio::test]
    async fn test_process_peer_exchange() {
        let config = DiscoveryConfig::new();
        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let exchange = PeerExchange {
            node_id: [2u8; 16],
            peers: vec![PeerDescriptor {
                node_id: [3u8; 16],
                address: "127.0.0.1:8000".to_string(),
                latency_ms: Some(10),
            }],
            capabilities: vec![Capability::WasmRuntime],
        };

        let added = service.process_peer_exchange(exchange).await;
        assert_eq!(added, 1);

        let stats = service.stats().await;
        assert_eq!(stats.total_peers, 1);
        assert_eq!(stats.exchange_peers, 1);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let mut config = DiscoveryConfig::new();
        config.peer_ttl = Duration::from_millis(100);

        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let mut peer = DiscoveredPeer::new(
            [2u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );

        // Make peer old
        peer.last_seen = Instant::now() - Duration::from_secs(1);
        service.add_peer(peer).await.unwrap();

        let removed = service.cleanup_expired().await;
        assert_eq!(removed, 1);

        let stats = service.stats().await;
        assert_eq!(stats.total_peers, 0);
    }

    #[tokio::test]
    async fn test_max_peers_limit() {
        let mut config = DiscoveryConfig::new();
        config.max_peers = 2;

        let service = DiscoveryService::new(config, [1u8; 16], vec![]);

        // Add 2 peers (should succeed)
        for i in 0..2 {
            let addr: SocketAddr = format!("127.0.0.1:800{}", i).parse().unwrap();
            let peer = DiscoveredPeer::new(
                [i as u8; 16],
                addr,
                NodeRole::Edge,
                vec![],
                DiscoveryMethod::Mdns,
            );
            service.add_peer(peer).await.unwrap();
        }

        // Try to add 3rd peer (should fail)
        let addr: SocketAddr = "127.0.0.1:8002".parse().unwrap();
        let peer = DiscoveredPeer::new(
            [3u8; 16],
            addr,
            NodeRole::Edge,
            vec![],
            DiscoveryMethod::Mdns,
        );
        let result = service.add_peer(peer).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_capability_equality() {
        assert_eq!(Capability::WasmRuntime, Capability::WasmRuntime);
        assert_ne!(Capability::WasmRuntime, Capability::GpuAcceleration);

        let arch1 = Capability::Architecture("aarch64".to_string());
        let arch2 = Capability::Architecture("aarch64".to_string());
        assert_eq!(arch1, arch2);
    }

    #[test]
    fn test_discovery_method_equality() {
        assert_eq!(DiscoveryMethod::Mdns, DiscoveryMethod::Mdns);
        assert_ne!(DiscoveryMethod::Mdns, DiscoveryMethod::Bootstrap);
    }
}
