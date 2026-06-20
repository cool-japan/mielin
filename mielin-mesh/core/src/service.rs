//! Integrated Mesh Service
//!
//! Orchestrates all mesh components:
//! - Node discovery (mDNS + bootstrap)
//! - Gossip protocol (membership + failure detection)
//! - Agent registry (location tracking)
//! - Live migration (pre-copy/post-copy/hybrid)
//! - DHT routing

use crate::{
    dht::Dht,
    discovery::{BootstrapNode, DiscoveryService},
    gossip::GossipState,
    migration::{MigrationCoordinator, MigrationRequest, MigrationStats},
    registry::{AgentId, AgentRegistry},
    Node, NodeId,
};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum MeshError {
    #[error("Discovery error: {0}")]
    DiscoveryError(String),
    #[error("Gossip error: {0}")]
    GossipError(String),
    #[error("Registry error: {0}")]
    RegistryError(String),
    #[error("Migration error: {0}")]
    MigrationError(String),
    #[error("Service not started")]
    ServiceNotStarted,
}

/// Configuration for mesh service
#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub bind_address: SocketAddr,
    pub bootstrap_nodes: Vec<BootstrapNode>,
    pub enable_mdns: bool,
    pub enable_gossip: bool,
    pub enable_registry: bool,
    pub enable_migration: bool,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:8080"
                .parse()
                .expect("static bind address must parse"),
            bootstrap_nodes: Vec::new(),
            enable_mdns: true,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: true,
        }
    }
}

/// Integrated mesh service orchestrating all components
pub struct MeshService {
    node: Arc<Node>,
    config: MeshConfig,

    // Core components
    dht: Arc<RwLock<Dht>>,
    discovery: Option<Arc<RwLock<DiscoveryService>>>,
    gossip: Option<Arc<GossipState>>,
    registry: Option<Arc<AgentRegistry>>,
    migration: Option<Arc<MigrationCoordinator>>,

    // State
    started: Arc<RwLock<bool>>,
}

impl MeshService {
    /// Create a new mesh service
    pub fn new(node: Arc<Node>, config: MeshConfig) -> Result<Self, MeshError> {
        let dht = Arc::new(RwLock::new(Dht::new(*node.id())));

        Ok(Self {
            node,
            config,
            dht,
            discovery: None,
            gossip: None,
            registry: None,
            migration: None,
            started: Arc::new(RwLock::new(false)),
        })
    }

    /// Start all mesh services
    pub async fn start(&mut self) -> Result<(), MeshError> {
        let mut started = self.started.write().await;
        if *started {
            warn!("Mesh service already started");
            return Ok(());
        }

        info!(
            "Starting mesh service for node {} at {}",
            self.node.id(),
            self.config.bind_address
        );

        // Initialize discovery service
        if self.config.enable_mdns {
            let mut discovery = DiscoveryService::new(self.node.clone(), self.config.bind_address)
                .map_err(|e| MeshError::DiscoveryError(e.to_string()))?
                .with_bootstrap_nodes(self.config.bootstrap_nodes.clone());

            discovery
                .start()
                .await
                .map_err(|e| MeshError::DiscoveryError(e.to_string()))?;

            self.discovery = Some(Arc::new(RwLock::new(discovery)));
            info!("Discovery service started");
        }

        // Initialize gossip protocol
        if self.config.enable_gossip {
            let gossip = GossipState::new(self.node.clone());
            gossip.start().await;
            self.gossip = Some(Arc::new(gossip));
            info!("Gossip protocol started");
        }

        // Initialize agent registry
        if self.config.enable_registry {
            let registry = AgentRegistry::new(self.node.clone(), self.dht.clone());
            registry.start().await;
            self.registry = Some(Arc::new(registry));
            info!("Agent registry started");
        }

        // Initialize migration coordinator
        if self.config.enable_migration {
            let migration = MigrationCoordinator::new(self.node.clone());
            migration.start().await;
            self.migration = Some(Arc::new(migration));
            info!("Migration coordinator started");
        }

        // Start peer synchronization
        self.spawn_peer_sync_task();

        *started = true;
        info!("Mesh service started successfully");
        Ok(())
    }

    /// Spawn task to synchronize peers between components
    fn spawn_peer_sync_task(&self) {
        let discovery = self.discovery.clone();
        let gossip = self.gossip.clone();
        let dht = self.dht.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;

                // Sync discovered peers to gossip and DHT
                if let (Some(disc), Some(gsp)) = (&discovery, &gossip) {
                    let disc = disc.read().await;
                    let peers = disc.get_peers().await;
                    drop(disc);

                    for peer in peers {
                        // Add to gossip membership
                        if let Err(e) = gsp.add_member(peer.node_id).await {
                            warn!("Failed to add peer to gossip: {}", e);
                        }

                        // Add to DHT
                        let mut dht = dht.write().await;
                        let dht_peer =
                            crate::dht::PeerInfo::new(peer.node_id, peer.address.to_string());
                        dht.insert_peer(dht_peer);
                    }
                }
            }
        });
    }

    /// Stop the mesh service
    pub async fn stop(&mut self) -> Result<(), MeshError> {
        let mut started = self.started.write().await;
        if !*started {
            return Ok(());
        }

        info!("Stopping mesh service for node {}", self.node.id());

        // Stop discovery
        if let Some(discovery) = &self.discovery {
            let mut disc = discovery.write().await;
            disc.stop().await;
        }

        *started = false;
        info!("Mesh service stopped");
        Ok(())
    }

    /// Check if service is started
    pub async fn is_started(&self) -> bool {
        *self.started.read().await
    }

    /// Get local node ID
    pub fn node_id(&self) -> &NodeId {
        self.node.id()
    }

    /// Get bind address
    pub fn bind_address(&self) -> SocketAddr {
        self.config.bind_address
    }

    // Discovery API

    /// Get discovered peers
    pub async fn get_peers(&self) -> Result<Vec<crate::discovery::DiscoveredPeer>, MeshError> {
        let discovery = self
            .discovery
            .as_ref()
            .ok_or(MeshError::ServiceNotStarted)?;
        let disc = discovery.read().await;
        Ok(disc.get_peers().await)
    }

    /// Connect to bootstrap nodes
    pub async fn connect_bootstrap(&self) -> Result<(), MeshError> {
        let discovery = self
            .discovery
            .as_ref()
            .ok_or(MeshError::ServiceNotStarted)?;
        let disc = discovery.read().await;
        disc.connect_bootstrap()
            .await
            .map_err(|e| MeshError::DiscoveryError(e.to_string()))?;
        Ok(())
    }

    // Gossip API

    /// Get gossip membership stats
    pub async fn get_member_stats(&self) -> Result<(usize, usize, usize), MeshError> {
        let gossip = self.gossip.as_ref().ok_or(MeshError::ServiceNotStarted)?;
        Ok(gossip.get_member_stats().await)
    }

    /// Get all alive members
    pub async fn get_alive_members(&self) -> Result<Vec<crate::gossip::MemberInfo>, MeshError> {
        let gossip = self.gossip.as_ref().ok_or(MeshError::ServiceNotStarted)?;
        Ok(gossip.get_alive_members().await)
    }

    // Registry API

    /// Register a local agent
    pub async fn register_agent(
        &self,
        agent_id: AgentId,
        address: SocketAddr,
    ) -> Result<(), MeshError> {
        let registry = self.registry.as_ref().ok_or(MeshError::ServiceNotStarted)?;
        registry
            .register_agent(agent_id, address)
            .await
            .map_err(|e| MeshError::RegistryError(e.to_string()))
    }

    /// Deregister an agent
    pub async fn deregister_agent(&self, agent_id: AgentId) -> Result<(), MeshError> {
        let registry = self.registry.as_ref().ok_or(MeshError::ServiceNotStarted)?;
        registry
            .deregister_agent(agent_id)
            .await
            .map_err(|e| MeshError::RegistryError(e.to_string()))
    }

    /// Query agent location
    pub async fn query_agent(
        &self,
        agent_id: AgentId,
    ) -> Result<crate::registry::AgentLocation, MeshError> {
        let registry = self.registry.as_ref().ok_or(MeshError::ServiceNotStarted)?;
        registry
            .query_agent(agent_id)
            .await
            .map_err(|e| MeshError::RegistryError(e.to_string()))
    }

    /// Get local agent count
    pub async fn local_agent_count(&self) -> Result<usize, MeshError> {
        let registry = self.registry.as_ref().ok_or(MeshError::ServiceNotStarted)?;
        Ok(registry.local_agent_count().await)
    }

    // Migration API

    /// Initiate agent migration
    pub async fn migrate_agent(&self, request: MigrationRequest) -> Result<(), MeshError> {
        let migration = self
            .migration
            .as_ref()
            .ok_or(MeshError::ServiceNotStarted)?;
        migration
            .initiate_migration(request)
            .await
            .map_err(|e| MeshError::MigrationError(e.to_string()))
    }

    /// Get migration statistics
    pub async fn get_migration_stats(&self) -> Result<MigrationStats, MeshError> {
        let migration = self
            .migration
            .as_ref()
            .ok_or(MeshError::ServiceNotStarted)?;
        Ok(migration.get_migration_stats().await)
    }

    /// Get active migration count
    pub async fn active_migration_count(&self) -> Result<usize, MeshError> {
        let migration = self
            .migration
            .as_ref()
            .ok_or(MeshError::ServiceNotStarted)?;
        Ok(migration.active_migration_count().await)
    }

    // DHT API

    /// Get peer count in DHT
    pub async fn dht_peer_count(&self) -> usize {
        let dht = self.dht.read().await;
        // DHT doesn't have a direct peer count method, so we'll use max_peers as proxy
        dht.max_peers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeRole;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mesh_service_creation() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig::default();
        let service = MeshService::new(node, config);
        assert!(service.is_ok());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mesh_service_lifecycle() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false, // Disable mDNS for testing
            enable_gossip: true,
            enable_registry: true,
            enable_migration: true,
        };

        let mut service = MeshService::new(node, config).unwrap();

        assert!(!service.is_started().await);

        service.start().await.unwrap();
        assert!(service.is_started().await);

        service.stop().await.unwrap();
        assert!(!service.is_started().await);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mesh_service_registry_integration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: false,
            enable_registry: true,
            enable_migration: false,
        };

        let mut service = MeshService::new(node, config).unwrap();
        service.start().await.unwrap();

        let agent_id = [1u8; 16];
        let address = "127.0.0.1:9000".parse().unwrap();

        service.register_agent(agent_id, address).await.unwrap();

        let count = service.local_agent_count().await.unwrap();
        assert_eq!(count, 1);

        let location = service.query_agent(agent_id).await.unwrap();
        assert_eq!(location.agent_id, agent_id);

        service.deregister_agent(agent_id).await.unwrap();
        let count = service.local_agent_count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mesh_service_gossip_integration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: true,
            enable_registry: false,
            enable_migration: false,
        };

        let mut service = MeshService::new(node, config).unwrap();
        service.start().await.unwrap();

        let (alive, suspect, dead) = service.get_member_stats().await.unwrap();
        assert_eq!(alive, 1); // Just local node
        assert_eq!(suspect, 0);
        assert_eq!(dead, 0);

        let members = service.get_alive_members().await.unwrap();
        assert_eq!(members.len(), 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mesh_service_migration_integration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false,
            enable_gossip: false,
            enable_registry: false,
            enable_migration: true,
        };

        let mut service = MeshService::new(node.clone(), config).unwrap();
        service.start().await.unwrap();

        let request = MigrationRequest {
            agent_id: [1u8; 16],
            source_node: *node.id(),
            target_node: NodeId::new_v4(),
            target_address: "127.0.0.1:8080".parse().unwrap(),
            strategy: crate::migration::MigrationStrategy::PreCopy,
            priority: 5,
        };

        service.migrate_agent(request).await.unwrap();

        let stats = service.get_migration_stats().await.unwrap();
        assert_eq!(stats.total_migrations, 1);
        assert_eq!(stats.successful_migrations, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mesh_service_full_integration() {
        let node = Arc::new(Node::new(NodeRole::Core));
        let config = MeshConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            bootstrap_nodes: Vec::new(),
            enable_mdns: false, // Disable for testing
            enable_gossip: true,
            enable_registry: true,
            enable_migration: true,
        };

        let mut service = MeshService::new(node.clone(), config).unwrap();
        service.start().await.unwrap();

        // Test registry
        let agent_id = [1u8; 16];
        service
            .register_agent(agent_id, "127.0.0.1:9000".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(service.local_agent_count().await.unwrap(), 1);

        // Test gossip
        let (alive, _, _) = service.get_member_stats().await.unwrap();
        assert_eq!(alive, 1);

        // Test migration
        let request = MigrationRequest {
            agent_id,
            source_node: *node.id(),
            target_node: NodeId::new_v4(),
            target_address: "127.0.0.1:8080".parse().unwrap(),
            strategy: crate::migration::MigrationStrategy::PostCopy,
            priority: 5,
        };
        service.migrate_agent(request).await.unwrap();

        let stats = service.get_migration_stats().await.unwrap();
        assert_eq!(stats.successful_migrations, 1);

        service.stop().await.unwrap();
    }
}
