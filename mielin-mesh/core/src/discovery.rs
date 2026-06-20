//! Node Discovery Protocol
//!
//! Implements multi-strategy node discovery:
//! - mDNS for local network discovery
//! - Bootstrap nodes for WAN connectivity
//! - Peer exchange for mesh growth

use crate::{Node, NodeId, NodeRole};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// MielinMesh mDNS service type
const SERVICE_TYPE: &str = "_mielin._udp.local.";

/// Peer cache expiration time
const PEER_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mDNS error: {0}")]
    MdnsError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Invalid peer data: {0}")]
    InvalidPeerData(String),
}

/// Discovered peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub node_id: NodeId,
    pub role: NodeRole,
    pub address: SocketAddr,
    pub capabilities: Vec<String>,
    pub discovered_at: std::time::SystemTime,
}

impl DiscoveredPeer {
    pub fn is_expired(&self) -> bool {
        self.discovered_at.elapsed().unwrap_or_default() > PEER_CACHE_TTL
    }
}

/// Bootstrap node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapNode {
    pub address: SocketAddr,
    pub public_key: Option<Vec<u8>>,
}

/// Discovery service for finding peers
pub struct DiscoveryService {
    node: Arc<Node>,
    bind_addr: SocketAddr,
    mdns_daemon: Option<ServiceDaemon>,
    peers: Arc<RwLock<HashMap<NodeId, DiscoveredPeer>>>,
    bootstrap_nodes: Vec<BootstrapNode>,
}

impl DiscoveryService {
    /// Create a new discovery service
    pub fn new(node: Arc<Node>, bind_addr: SocketAddr) -> Result<Self, DiscoveryError> {
        Ok(Self {
            node,
            bind_addr,
            mdns_daemon: None,
            peers: Arc::new(RwLock::new(HashMap::new())),
            bootstrap_nodes: Vec::new(),
        })
    }

    /// Add bootstrap nodes for initial connectivity
    pub fn with_bootstrap_nodes(mut self, nodes: Vec<BootstrapNode>) -> Self {
        self.bootstrap_nodes = nodes;
        self
    }

    /// Start the discovery service
    pub async fn start(&mut self) -> Result<(), DiscoveryError> {
        // Start mDNS discovery
        self.start_mdns()?;

        // Start peer cache cleanup task
        let peers = self.peers.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                Self::cleanup_expired_peers(&peers).await;
            }
        });

        info!("Discovery service started on {}", self.bind_addr);
        Ok(())
    }

    /// Start mDNS discovery
    fn start_mdns(&mut self) -> Result<(), DiscoveryError> {
        let mdns = ServiceDaemon::new().map_err(|e| {
            DiscoveryError::MdnsError(format!("Failed to create mDNS daemon: {}", e))
        })?;

        // Register our service
        let instance_name = format!("mielin-{}", self.node.id());
        let properties = self.create_service_properties();

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &format!("{}.local.", instance_name),
            self.bind_addr.ip(),
            self.bind_addr.port(),
            properties,
        )
        .map_err(|e| DiscoveryError::MdnsError(format!("Failed to create service info: {}", e)))?;

        mdns.register(service_info)
            .map_err(|e| DiscoveryError::MdnsError(format!("Failed to register service: {}", e)))?;

        // Browse for other MielinMesh nodes
        let receiver = mdns
            .browse(SERVICE_TYPE)
            .map_err(|e| DiscoveryError::MdnsError(format!("Failed to browse: {}", e)))?;

        // Spawn task to handle mDNS events
        let node_id = *self.node.id();
        let peers = self.peers.clone();
        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                if let Err(e) = Self::handle_mdns_event(event, &peers, &node_id).await {
                    warn!("Failed to handle mDNS event: {}", e);
                }
            }
        });

        self.mdns_daemon = Some(mdns);
        debug!("mDNS service registered and browsing");
        Ok(())
    }

    /// Create service properties for mDNS announcement
    fn create_service_properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();
        props.insert("node_id".to_string(), self.node.id().to_string());
        props.insert("role".to_string(), format!("{:?}", self.node.role()));
        props.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        props
    }

    /// Handle mDNS service events
    async fn handle_mdns_event(
        event: ServiceEvent,
        peers: &Arc<RwLock<HashMap<NodeId, DiscoveredPeer>>>,
        own_node_id: &NodeId,
    ) -> Result<(), DiscoveryError> {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                debug!("Discovered service: {}", info.get_fullname());

                // Extract node information from service properties
                let properties = info.get_properties();
                let node_id_str = properties.get("node_id").ok_or_else(|| {
                    DiscoveryError::InvalidPeerData("Missing node_id".to_string())
                })?;

                let node_id_val = node_id_str.val_str();
                let node_id = node_id_val.parse::<NodeId>().map_err(|e| {
                    DiscoveryError::InvalidPeerData(format!("Invalid node_id: {}", e))
                })?;

                // Don't add ourselves
                if node_id == *own_node_id {
                    return Ok(());
                }

                let role_str = properties
                    .get("role")
                    .ok_or_else(|| DiscoveryError::InvalidPeerData("Missing role".to_string()))?;

                let role_val = role_str.val_str();
                let role = match role_val {
                    "Edge" => NodeRole::Edge,
                    "Relay" => NodeRole::Relay,
                    "Core" => NodeRole::Core,
                    _ => {
                        return Err(DiscoveryError::InvalidPeerData(format!(
                            "Invalid role: {}",
                            role_val
                        )))
                    }
                };

                // Get address
                let addresses = info.get_addresses();
                let scoped_ip = addresses
                    .iter()
                    .next()
                    .ok_or_else(|| DiscoveryError::InvalidPeerData("No addresses".to_string()))?;
                let address = SocketAddr::new(scoped_ip.to_ip_addr(), info.get_port());

                let peer = DiscoveredPeer {
                    node_id,
                    role,
                    address,
                    capabilities: Vec::new(),
                    discovered_at: std::time::SystemTime::now(),
                };

                let mut peers = peers.write().await;
                peers.insert(node_id, peer.clone());
                info!(
                    "Discovered peer: {} at {} (role: {:?})",
                    node_id, address, role
                );
            }
            ServiceEvent::ServiceRemoved(_, fullname) => {
                debug!("Service removed: {}", fullname);
            }
            _ => {}
        }

        Ok(())
    }

    /// Clean up expired peers from cache
    async fn cleanup_expired_peers(peers: &Arc<RwLock<HashMap<NodeId, DiscoveredPeer>>>) {
        let mut peers = peers.write().await;
        let expired: Vec<NodeId> = peers
            .iter()
            .filter(|(_, peer)| peer.is_expired())
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            peers.remove(&id);
            debug!("Removed expired peer: {}", id);
        }
    }

    /// Get all discovered peers
    pub async fn get_peers(&self) -> Vec<DiscoveredPeer> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }

    /// Get peer by node ID
    pub async fn get_peer(&self, node_id: &NodeId) -> Option<DiscoveredPeer> {
        let peers = self.peers.read().await;
        peers.get(node_id).cloned()
    }

    /// Connect to bootstrap nodes and request peer list
    ///
    /// This initiates connections to configured bootstrap nodes and requests
    /// their known peers, which are then added to the local peer cache.
    ///
    /// Returns the list of newly discovered peers.
    pub async fn connect_bootstrap(&self) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        let discovered = Vec::new();

        for bootstrap in &self.bootstrap_nodes {
            debug!("Connecting to bootstrap node: {}", bootstrap.address);

            // Note: Actual QUIC connection would be done here
            // For Phase 2, this is a placeholder for the integration with
            // mielin-mesh-wire transport layer.
            //
            // Full implementation would:
            // 1. Use QuicTransport::connect() to establish connection
            // 2. Send Message::Discovery with our node info
            // 3. Receive Message::DiscoveryResponse with peer list
            // 4. Add received peers to our cache
            //
            // This will be implemented when we create the full mesh service
            // that coordinates discovery + transport + DHT.
        }

        Ok(discovered)
    }

    /// Exchange peers with a discovered node
    ///
    /// Implements peer exchange protocol (PEX) to grow the mesh through
    /// gossip-style peer sharing. Each node shares its known peers with
    /// new connections.
    pub async fn exchange_peers(
        &self,
        peer: &DiscoveredPeer,
    ) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        debug!("Initiating peer exchange with {}", peer.node_id);

        // Note: This would use the wire protocol to exchange peer lists
        // Implementation deferred to mesh service integration

        Ok(Vec::new())
    }

    /// Stop the discovery service
    pub async fn stop(&mut self) {
        if let Some(mdns) = self.mdns_daemon.take() {
            if let Err(e) = mdns.shutdown() {
                error!("Failed to shutdown mDNS daemon: {}", e);
            }
        }
        info!("Discovery service stopped");
    }
}

impl Drop for DiscoveryService {
    fn drop(&mut self) {
        if let Some(mdns) = self.mdns_daemon.take() {
            let _ = mdns.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_peer_expiration() {
        let peer = DiscoveredPeer {
            node_id: NodeId::new_v4(),
            role: NodeRole::Edge,
            address: "127.0.0.1:8080".parse().unwrap(),
            capabilities: vec![],
            discovered_at: std::time::SystemTime::now() - Duration::from_secs(400), // Older than TTL
        };

        assert!(peer.is_expired());
    }

    #[test]
    fn test_discovered_peer_not_expired() {
        let peer = DiscoveredPeer {
            node_id: NodeId::new_v4(),
            role: NodeRole::Relay,
            address: "127.0.0.1:8080".parse().unwrap(),
            capabilities: vec![],
            discovered_at: std::time::SystemTime::now(),
        };

        assert!(!peer.is_expired());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_discovery_service_creation() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let bind_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let discovery = DiscoveryService::new(node, bind_addr);
        assert!(discovery.is_ok());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_bootstrap_nodes() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let bind_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let bootstrap = vec![
            BootstrapNode {
                address: "192.168.1.100:8080".parse().unwrap(),
                public_key: None,
            },
            BootstrapNode {
                address: "192.168.1.101:8080".parse().unwrap(),
                public_key: None,
            },
        ];

        let discovery = DiscoveryService::new(node, bind_addr)
            .unwrap()
            .with_bootstrap_nodes(bootstrap.clone());

        assert_eq!(discovery.bootstrap_nodes.len(), 2);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_cache() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let bind_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        let discovery = DiscoveryService::new(node, bind_addr).unwrap();

        // Add a peer manually
        let peer = DiscoveredPeer {
            node_id: NodeId::new_v4(),
            role: NodeRole::Edge,
            address: "192.168.1.100:8080".parse().unwrap(),
            capabilities: vec!["tensor".to_string()],
            discovered_at: std::time::SystemTime::now(),
        };

        let peer_id = peer.node_id;
        discovery.peers.write().await.insert(peer_id, peer.clone());

        // Retrieve the peer
        let retrieved = discovery.get_peer(&peer_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().address, peer.address);

        // Get all peers
        let all_peers = discovery.get_peers().await;
        assert_eq!(all_peers.len(), 1);
    }
}
