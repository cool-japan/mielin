//! End-to-End Multi-Node Mesh Cluster Example
//!
//! This example demonstrates a mesh cluster with:
//! - QUIC transport with TLS certificate management
//! - Node discovery using bootstrap nodes
//! - Gossip protocol for state synchronization
//! - Health monitoring
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example e2e-mesh-cluster -- --role core --port 9000
//! ```

use anyhow::{Context, Result};
use mielin_mesh_core::{
    discovery::BootstrapNode,
    gossip::GossipState,
    service::{MeshConfig, MeshService},
    Node, NodeRole,
};
use mielin_mesh_wire::{
    certs::CertManager,
    health::{HealthConfig, HealthMonitor},
    transport::QuicTransport,
    Message,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{info, warn};
use uuid::Uuid;

/// Node role (simplified without clap derive)
#[derive(Debug, Clone, Copy)]
enum Role {
    Core,
    Relay,
    Edge,
}

impl From<Role> for NodeRole {
    fn from(role: Role) -> Self {
        match role {
            Role::Core => NodeRole::Core,
            Role::Relay => NodeRole::Relay,
            Role::Edge => NodeRole::Edge,
        }
    }
}

/// Mesh cluster node with full integration
struct ClusterNode {
    /// Unique node identifier
    node_id: Uuid,
    /// Node role in the cluster
    role: NodeRole,
    /// Mesh node
    #[allow(dead_code)]
    node: Arc<Node>,
    /// Integrated mesh service (gossip, registry, migration)
    mesh_service: Arc<RwLock<MeshService>>,
    /// QUIC transport layer
    transport: Arc<QuicTransport>,
    /// TLS certificate manager (optional)
    cert_manager: Option<Arc<CertManager>>,
    /// Health monitor for peer nodes
    health_monitor: Arc<HealthMonitor>,
    /// Gossip state tracker
    gossip_state: Arc<GossipState>,
    /// Bootstrap peers for connecting
    bootstrap_peers: Vec<SocketAddr>,
}

impl ClusterNode {
    /// Create a new cluster node with full initialization
    async fn new(
        role: NodeRole,
        bind_port: u16,
        bootstrap_peers: Vec<String>,
        use_tls: bool,
        enable_mdns: bool,
        _gossip_interval_ms: u64,
        health_check_interval_secs: u64,
    ) -> Result<Self> {
        let bind_addr: SocketAddr = format!("127.0.0.1:{}", bind_port)
            .parse()
            .context("Failed to parse bind address")?;

        info!("🚀 Initializing {} node", role_name(role));

        // Create transport with optional TLS
        let (transport, cert_manager) = if use_tls {
            info!("🔒 Enabling TLS certificate management");
            let cert_manager = Arc::new(CertManager::new());
            let node_id_str = Uuid::new_v4().to_string();
            let transport =
                QuicTransport::new_with_certs(bind_addr, &node_id_str, cert_manager.clone())
                    .await
                    .context("Failed to create QUIC transport with TLS")?;
            (transport, Some(cert_manager))
        } else {
            info!("⚠️  TLS disabled - using self-signed certificates");
            let transport = QuicTransport::new(bind_addr)
                .await
                .context("Failed to create QUIC transport")?;
            (transport, None)
        };

        let actual_addr = transport
            .local_addr()
            .context("Failed to get local address")?;
        let node = Arc::new(Node::new(role));
        let node_id = *node.id();

        info!("✅ Transport initialized");
        info!("   Binding: {}", actual_addr);
        info!("   Node ID: {}", node_id);

        // Parse bootstrap nodes
        let parsed_peers: Vec<SocketAddr> = bootstrap_peers
            .iter()
            .filter_map(|addr| {
                addr.parse::<SocketAddr>().ok().inspect(|socket_addr| {
                    info!("   Bootstrap peer: {}", socket_addr);
                })
            })
            .collect();

        let bootstrap_nodes: Vec<BootstrapNode> = parsed_peers
            .iter()
            .map(|&addr| BootstrapNode {
                address: addr,
                public_key: None,
            })
            .collect();

        // Configure mesh service
        let config = MeshConfig {
            bind_address: actual_addr,
            bootstrap_nodes,
            enable_mdns,
            enable_gossip: true,
            enable_registry: true,
            enable_migration: true,
        };

        let mut mesh_service =
            MeshService::new(node.clone(), config).context("Failed to create mesh service")?;

        // Start mesh service
        mesh_service
            .start()
            .await
            .context("Failed to start mesh service")?;

        info!("✅ Mesh service started");
        info!(
            "   Services: gossip={}, registry={}, migration={}, mdns={}",
            true, true, true, enable_mdns
        );

        // Create gossip state tracker
        let gossip_state = Arc::new(GossipState::new(node.clone()));

        // Create health monitor
        let health_config = HealthConfig {
            heartbeat_interval: Duration::from_secs(health_check_interval_secs),
            unhealthy_timeout: Duration::from_secs(15),
            dead_timeout: Duration::from_secs(30),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(1),
        };
        let health_monitor = Arc::new(HealthMonitor::with_config(health_config));

        info!("✅ Cluster node initialized");

        Ok(Self {
            node_id,
            role,
            node,
            mesh_service: Arc::new(RwLock::new(mesh_service)),
            transport: Arc::new(transport),
            cert_manager,
            health_monitor,
            gossip_state,
            bootstrap_peers: parsed_peers,
        })
    }

    /// Connect to bootstrap peers
    async fn connect_to_peers(&self) -> Result<()> {
        if self.bootstrap_peers.is_empty() {
            info!("📡 No bootstrap peers configured - waiting for discovery");
            return Ok(());
        }

        info!(
            "🔗 Connecting to {} bootstrap peer(s)",
            self.bootstrap_peers.len()
        );

        for addr in &self.bootstrap_peers {
            info!("   Connecting to {}", addr);

            match self.connect_to_peer(*addr).await {
                Ok(()) => {
                    info!("   ✅ Connected to {}", addr);
                    self.health_monitor.register(*addr).await;
                }
                Err(e) => {
                    warn!("   ⚠️  Failed to connect to {}: {}", addr, e);
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// Connect to a single peer
    async fn connect_to_peer(&self, peer_addr: SocketAddr) -> Result<()> {
        let conn = self
            .transport
            .connect(peer_addr)
            .await
            .context("Failed to establish QUIC connection")?;

        // Send discovery message
        let discovery_msg = Message::Discovery {
            node_id: *self.node_id.as_bytes(),
            node_role: role_to_wire(self.role),
            capabilities: vec!["quic".to_string(), "tls".to_string(), "gossip".to_string()],
        };

        conn.send(&discovery_msg)
            .await
            .context("Failed to send discovery message")?;

        // Wait for discovery response
        match conn.receive().await {
            Ok(Message::DiscoveryResponse { node_id, peers }) => {
                let peer_id = Uuid::from_bytes(node_id);
                info!("   Discovered peer: {}", peer_id);
                info!("   Known peers: {}", peers.len());
                Ok(())
            }
            Ok(msg) => {
                warn!("   Unexpected response: {:?}", msg);
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failed to receive discovery response: {}",
                e
            )),
        }
    }

    /// Start gossip protocol
    async fn start_gossip_protocol(&self) {
        let gossip_state = self.gossip_state.clone();
        gossip_state.start().await;
        info!("🗣️  Gossip protocol started");
    }

    /// Display cluster status
    async fn display_status(&self) {
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📊 Cluster Status Report");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        info!("🏷️  Node Information:");
        info!("   ID: {}", self.node_id);
        info!("   Role: {}", role_name(self.role));
        info!(
            "   Address: {}",
            self.transport.local_addr().unwrap_or_else(|_| {
                "0.0.0.0:0"
                    .parse()
                    .expect("Failed to parse default address")
            })
        );

        // Mesh service stats
        let mesh = self.mesh_service.read().await;
        if let Ok((alive, suspect, dead)) = mesh.get_member_stats().await {
            info!("📡 Gossip Membership:");
            info!("   Alive: {}", alive);
            info!("   Suspect: {}", suspect);
            info!("   Dead: {}", dead);
        }

        if let Ok(count) = mesh.local_agent_count().await {
            info!("🤖 Agent Registry:");
            info!("   Local agents: {}", count);
        }

        if let Ok(stats) = mesh.get_migration_stats().await {
            info!("📦 Migration Statistics:");
            info!("   Total: {}", stats.total_migrations);
            info!("   Successful: {}", stats.successful_migrations);
            info!("   Failed: {}", stats.failed_migrations);
        }
        drop(mesh);

        // Health monitoring stats
        let healthy = self.health_monitor.get_healthy_connections().await;
        let all = self.health_monitor.get_all_connections().await;
        info!("💚 Health Monitoring:");
        info!("   Healthy peers: {}", healthy.len());
        info!("   Total peers: {}", all.len());

        // TLS certificate info
        if let Some(cert_mgr) = &self.cert_manager {
            if let Some(cert_info) = cert_mgr.get_cert_info().await {
                info!("🔒 TLS Certificate:");
                info!("   Subject: {}", cert_info.common_name);
                if let Some(days) = cert_info.time_until_expiry().map(|d| d.as_secs() / 86400) {
                    info!("   Expires in: {} days", days);
                }
            }
        }

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }

    /// Run the server loop
    async fn run_server(&self) -> Result<()> {
        info!("👂 Listening for incoming connections...");

        let node_id = self.node_id;

        loop {
            let conn = self
                .transport
                .accept()
                .await
                .context("Failed to accept connection")?;
            let remote = conn.remote_addr();

            info!("📨 Incoming connection from {}", remote);

            // Handle connection inline
            tokio::spawn(async move {
                if let Err(e) = handle_incoming_connection(conn, node_id).await {
                    warn!("Error handling connection: {}", e);
                }
            });
        }
    }
}

/// Handle an incoming connection
async fn handle_incoming_connection(
    conn: mielin_mesh_wire::transport::QuicConnection,
    node_id: Uuid,
) -> Result<()> {
    let msg = conn.receive().await.context("Failed to receive message")?;
    let msg = msg.unwrap_payload();

    match msg {
        Message::Discovery {
            node_id: peer_id,
            node_role,
            capabilities,
        } => {
            let peer_uuid = Uuid::from_bytes(peer_id);
            info!("🔍 Discovery from peer {}", peer_uuid);
            info!("   Role: {:?}", node_role);
            info!("   Capabilities: {:?}", capabilities);

            let response = Message::DiscoveryResponse {
                node_id: *node_id.as_bytes(),
                peers: vec![],
            };

            conn.send(&response)
                .await
                .context("Failed to send discovery response")?;
        }
        Message::Ping { timestamp } => {
            info!("🏓 Ping received (timestamp: {})", timestamp);

            let pong = Message::Pong {
                timestamp,
                latency_ms: 0,
            };

            conn.send(&pong).await.context("Failed to send pong")?;
        }
        other => {
            info!("📬 Received message: {:?}", other);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .init();

    // Simple argument parsing
    let args: Vec<String> = std::env::args().collect();
    let role = parse_role(&args).unwrap_or(Role::Core);
    let port = parse_port(&args).unwrap_or(0);
    let peers = parse_peers(&args);
    let use_tls = args.iter().any(|a| a == "--use-tls");
    let enable_mdns = args.iter().any(|a| a == "--enable-mdns");

    info!("═══════════════════════════════════════════════════════");
    info!("  MielinOS End-to-End Mesh Cluster");
    info!("  Version: v0.0.1 (Development Preview)");
    info!("  Role: {:?}", role);
    if use_tls {
        info!("  TLS: ENABLED");
    }
    if enable_mdns {
        info!("  mDNS Discovery: ENABLED");
    }
    info!("═══════════════════════════════════════════════════════");

    // Create cluster node
    let node = ClusterNode::new(role.into(), port, peers, use_tls, enable_mdns, 1000, 5)
        .await
        .context("Failed to create cluster node")?;

    // Connect to bootstrap peers
    node.connect_to_peers()
        .await
        .context("Failed to connect to peers")?;

    // Start background services
    node.start_gossip_protocol().await;

    // Display initial status
    sleep(Duration::from_secs(2)).await;
    node.display_status().await;

    // Start periodic status updates
    let node_for_status = Arc::new(node);
    let node_clone = node_for_status.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            node_clone.display_status().await;
        }
    });

    // Run server
    node_for_status
        .run_server()
        .await
        .context("Server loop failed")
}

// Helper functions

fn role_name(role: NodeRole) -> &'static str {
    match role {
        NodeRole::Core => "Core",
        NodeRole::Relay => "Relay",
        NodeRole::Edge => "Edge",
    }
}

fn role_to_wire(role: NodeRole) -> mielin_mesh_wire::NodeRole {
    match role {
        NodeRole::Core => mielin_mesh_wire::NodeRole::Core,
        NodeRole::Relay => mielin_mesh_wire::NodeRole::Relay,
        NodeRole::Edge => mielin_mesh_wire::NodeRole::Edge,
    }
}

fn parse_role(args: &[String]) -> Option<Role> {
    args.iter()
        .position(|a| a == "--role")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| match s.as_str() {
            "core" => Some(Role::Core),
            "relay" => Some(Role::Relay),
            "edge" => Some(Role::Edge),
            _ => None,
        })
}

fn parse_port(args: &[String]) -> Option<u16> {
    args.iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}

fn parse_peers(args: &[String]) -> Vec<String> {
    let mut peers = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--connect" || args[i] == "-c" {
            if let Some(peer) = args.get(i + 1) {
                peers.push(peer.clone());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    peers
}
