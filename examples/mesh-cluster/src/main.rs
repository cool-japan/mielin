//! MielinOS 3-Node Mesh Cluster Example
//!
//! Demonstrates integrated mesh service with discovery, gossip, agent registry,
//! and live agent migration across a cluster of edge, relay, and core nodes.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use mielin_cells::{migration::MigrationSnapshot, Agent};
use mielin_mesh_core::{
    discovery::BootstrapNode,
    migration::{MigrationRequest, MigrationStrategy},
    service::{MeshConfig, MeshService},
    Node, NodeRole,
};
use mielin_mesh_wire::{certs::CertManager, transport::QuicTransport, Message};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, ValueEnum)]
enum Role {
    Edge,
    Relay,
    Core,
}

impl From<Role> for NodeRole {
    fn from(role: Role) -> Self {
        match role {
            Role::Edge => NodeRole::Edge,
            Role::Relay => NodeRole::Relay,
            Role::Core => NodeRole::Core,
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Node role (edge, relay, or core)
    #[arg(short, long, value_enum)]
    role: Role,

    /// Port to bind to
    #[arg(short, long, default_value = "0")]
    port: u16,

    /// Peer addresses to connect to (format: ip:port)
    #[arg(short = 'c', long)]
    connect: Vec<String>,

    /// Create an agent on this node
    #[arg(short, long)]
    agent: bool,

    /// Migrate agent to this peer (format: ip:port)
    #[arg(short, long)]
    migrate_to: Option<String>,

    /// Enable TLS certificate management
    #[arg(long)]
    use_certs: bool,
}

struct MeshNode {
    mesh_service: Arc<RwLock<MeshService>>,
    transport: Arc<QuicTransport>,
    agents: Arc<RwLock<Vec<Agent>>>,
    cert_manager: Option<Arc<CertManager>>,
}

impl MeshNode {
    async fn new(
        role: NodeRole,
        bind_port: u16,
        bootstrap_addrs: Vec<String>,
        use_certs: bool,
    ) -> Result<Self> {
        let bind_addr: SocketAddr = format!("127.0.0.1:{}", bind_port).parse()?;

        // Create transport with or without certificate management
        let (transport, cert_manager) = if use_certs {
            let cert_manager = Arc::new(CertManager::new());
            let node_id = Uuid::new_v4().to_string();
            info!(
                "Creating transport with certificate management (node_id: {})",
                node_id
            );
            let transport =
                QuicTransport::new_with_certs(bind_addr, &node_id, cert_manager.clone()).await?;
            (transport, Some(cert_manager))
        } else {
            info!("Creating transport without certificate management");
            let transport = QuicTransport::new(bind_addr).await?;
            (transport, None)
        };

        let actual_addr = transport.local_addr()?;

        let node = Arc::new(Node::new(role));

        // Parse bootstrap nodes from addresses
        let bootstrap_nodes: Vec<BootstrapNode> = bootstrap_addrs
            .iter()
            .filter_map(|addr| {
                addr.parse::<SocketAddr>()
                    .ok()
                    .map(|socket_addr| BootstrapNode {
                        address: socket_addr,
                        public_key: None,
                    })
            })
            .collect();

        // Configure mesh service
        let config = MeshConfig {
            bind_address: actual_addr,
            bootstrap_nodes,
            enable_mdns: false, // Disable mDNS for now (requires multicast)
            enable_gossip: true,
            enable_registry: true,
            enable_migration: true,
        };

        let mut mesh_service = MeshService::new(node.clone(), config)?;

        info!(
            "🚀 {} node started on {}",
            match role {
                NodeRole::Edge => "Edge",
                NodeRole::Relay => "Relay",
                NodeRole::Core => "Core",
            },
            actual_addr
        );
        info!("📋 Node ID: {}", mesh_service.node_id());

        // Start mesh service
        mesh_service.start().await?;
        info!("✅ Mesh service started (gossip + registry + migration)");

        Ok(Self {
            mesh_service: Arc::new(RwLock::new(mesh_service)),
            transport: Arc::new(transport),
            agents: Arc::new(RwLock::new(Vec::new())),
            cert_manager,
        })
    }

    async fn connect_to_peer(&self, peer_addr: &str) -> Result<()> {
        let addr: SocketAddr = peer_addr.parse()?;

        info!("🔗 Connecting to peer at {}", addr);
        let conn = self.transport.connect(addr).await?;

        let mesh = self.mesh_service.read().await;
        let node_id = mesh.node_id();

        // Send discovery message
        let discovery_msg = Message::Discovery {
            node_id: *node_id.as_bytes(),
            node_role: mielin_mesh_wire::NodeRole::Edge, // Simplified for example
            capabilities: vec!["quic".to_string(), "migration".to_string()],
        };

        conn.send(&discovery_msg).await?;
        info!("✅ Discovery message sent to {}", addr);

        // Wait for discovery response
        if let Ok(Message::DiscoveryResponse { .. }) = conn.receive().await {
            info!("📋 Received discovery response from peer");
        }

        Ok(())
    }

    async fn show_mesh_status(&self) {
        let mesh = self.mesh_service.read().await;

        info!("📊 Mesh Service Status:");

        // Show gossip membership
        if let Ok((alive, suspect, dead)) = mesh.get_member_stats().await {
            info!(
                "   Gossip: {} alive, {} suspect, {} dead",
                alive, suspect, dead
            );
        }

        // Show registry
        if let Ok(count) = mesh.local_agent_count().await {
            info!("   Registry: {} local agents", count);
        }

        // Show migration stats
        if let Ok(stats) = mesh.get_migration_stats().await {
            info!(
                "   Migrations: {} total, {} successful",
                stats.total_migrations, stats.successful_migrations
            );
        }

        // Show certificate status
        if let Some(cert_mgr) = &self.cert_manager {
            if let Some(cert_info) = cert_mgr.get_cert_info().await {
                info!(
                    "   Certificate: {} (expires in {} days)",
                    cert_info.common_name,
                    cert_info
                        .time_until_expiry()
                        .map(|d| d.as_secs() / 86400)
                        .unwrap_or(0)
                );
                if cert_info.should_rotate() {
                    warn!("   ⚠️  Certificate should be rotated!");
                }
            }
        }
    }

    async fn create_agent(&self) -> Result<()> {
        // Create a simple WASM binary (minimal agent)
        let wasm_binary = vec![
            0x00, 0x61, 0x73, 0x6d, // WASM magic number
            0x01, 0x00, 0x00, 0x00, // WASM version
        ];

        let agent = Agent::new(wasm_binary);
        let agent_id = agent.id();

        self.agents.write().await.push(agent);

        // Register agent with mesh service
        let mesh = self.mesh_service.read().await;
        let bind_addr = mesh.bind_address();
        let agent_id_bytes = *agent_id.as_bytes();

        mesh.register_agent(agent_id_bytes, bind_addr).await?;

        info!("🤖 Created agent with ID: {}", agent_id);
        info!("   Registered in mesh registry");
        info!(
            "   Total agents on this node: {}",
            self.agents.read().await.len()
        );

        Ok(())
    }

    async fn migrate_agent_to(&self, target_addr: &str) -> Result<()> {
        let agents = self.agents.read().await;

        if agents.is_empty() {
            warn!("⚠️  No agents to migrate");
            return Ok(());
        }

        let agent = &agents[0];
        let agent_id = agent.id();
        let agent_id_bytes = *agent_id.as_bytes();

        info!("📦 Initiating migration for agent {}", agent_id);

        let target: SocketAddr = target_addr.parse()?;
        let mesh = self.mesh_service.read().await;
        let source_node = *mesh.node_id();

        // Create migration request using mesh service
        let request = MigrationRequest {
            agent_id: agent_id_bytes,
            source_node,
            target_node: Uuid::new_v4(), // In real scenario, query registry for target node ID
            target_address: target,
            strategy: MigrationStrategy::PreCopy,
            priority: 10,
        };

        // Initiate migration through mesh service
        mesh.migrate_agent(request).await?;

        info!("✅ Migration initiated via mesh service");

        // Still do the actual network transfer (mesh service tracks it)
        let snapshot = MigrationSnapshot::capture(agent, Some(*source_node.as_bytes()))?;
        let snapshot_data = snapshot.serialize()?;
        drop(agents);

        let conn = self.transport.connect(target).await?;
        let migration_msg = Message::AgentMigration {
            agent_id: agent_id_bytes,
            snapshot: snapshot_data,
            priority: 10,
        };

        conn.send(&migration_msg).await?;

        // Wait for ACK
        let response = conn.receive().await?;

        match response {
            Message::MigrationAck {
                success, error_msg, ..
            } => {
                if success {
                    info!("🎉 Migration successful!");
                    // Deregister agent from local registry
                    mesh.deregister_agent(agent_id_bytes).await?;
                    // Remove agent from local storage
                    self.agents.write().await.remove(0);
                } else {
                    warn!("❌ Migration failed: {:?}", error_msg);
                }
            }
            _ => {
                warn!("⚠️  Unexpected response type");
            }
        }

        Ok(())
    }

    async fn run_server(&self) -> Result<()> {
        info!("👂 Listening for incoming connections...");

        loop {
            let conn = self.transport.accept().await?;
            let remote = conn.remote_addr();

            info!("📨 Received connection from {}", remote);

            let agents = self.agents.clone();
            let mesh_service = self.mesh_service.clone();

            tokio::spawn(async move {
                match conn.receive().await {
                    Ok(msg) => {
                        let mesh = mesh_service.read().await;
                        let node_id = *mesh.node_id().as_bytes();
                        drop(mesh);

                        // Unwrap routed messages before processing
                        let msg = msg.unwrap_payload();

                        match msg {
                            Message::Discovery {
                                node_id: peer_id,
                                node_role,
                                capabilities,
                            } => {
                                info!("🔍 Discovery from peer:");
                                info!("   Node ID: {:?}", peer_id);
                                info!("   Role: {:?}", node_role);
                                info!("   Capabilities: {:?}", capabilities);

                                // Send discovery response
                                let response = Message::DiscoveryResponse {
                                    node_id,
                                    peers: vec![],
                                };

                                if let Err(e) = conn.send(&response).await {
                                    warn!("Failed to send discovery response: {}", e);
                                }
                            }
                            Message::AgentMigration {
                                agent_id,
                                snapshot,
                                priority,
                            } => {
                                info!("📥 Received agent migration:");
                                info!("   Agent ID: {:?}", agent_id);
                                info!("   Snapshot size: {} bytes", snapshot.len());
                                info!("   Priority: {}", priority);

                                // Deserialize and restore agent
                                match MigrationSnapshot::deserialize(&snapshot) {
                                    Ok(snapshot) => {
                                        match snapshot.restore() {
                                            Ok(agent) => {
                                                agents.write().await.push(agent);

                                                info!("✅ Agent restored successfully");
                                                info!(
                                                    "   Total agents on this node: {}",
                                                    agents.read().await.len()
                                                );

                                                // Register agent with mesh service
                                                let mesh = mesh_service.read().await;
                                                let bind_addr = mesh.bind_address();
                                                if let Err(e) =
                                                    mesh.register_agent(agent_id, bind_addr).await
                                                {
                                                    warn!(
                                                        "Failed to register agent in mesh: {}",
                                                        e
                                                    );
                                                } else {
                                                    info!("   Registered in mesh registry");
                                                }

                                                // Send success ACK
                                                let ack = Message::MigrationAck {
                                                    agent_id,
                                                    success: true,
                                                    error_msg: None,
                                                };

                                                if let Err(e) = conn.send(&ack).await {
                                                    warn!("Failed to send migration ACK: {}", e);
                                                }
                                            }
                                            Err(e) => {
                                                warn!("❌ Failed to restore agent: {}", e);

                                                let ack = Message::MigrationAck {
                                                    agent_id,
                                                    success: false,
                                                    error_msg: Some(e.to_string()),
                                                };

                                                if let Err(e) = conn.send(&ack).await {
                                                    warn!("Failed to send failure ACK: {}", e);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("❌ Failed to deserialize snapshot: {}", e);
                                    }
                                }
                            }
                            Message::Ping { timestamp } => {
                                info!("🏓 Ping received (timestamp: {})", timestamp);

                                let pong = Message::Pong {
                                    timestamp,
                                    latency_ms: 0,
                                };

                                if let Err(e) = conn.send(&pong).await {
                                    warn!("Failed to send pong: {}", e);
                                }
                            }
                            other => {
                                info!("📬 Received message: {:?}", other);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to receive message: {}", e);
                    }
                }
            });
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();

    info!("═══════════════════════════════════════════════");
    info!("  MielinOS Mesh Cluster - 3-Node Demo");
    info!("  Version: v0.0.1 (Development Preview)");
    info!("  Using Integrated MeshService");
    if args.use_certs {
        info!("  TLS Certificate Management: ENABLED");
    }
    info!("═══════════════════════════════════════════════");

    // Create mesh node with bootstrap addresses
    let node = MeshNode::new(
        args.role.clone().into(),
        args.port,
        args.connect.clone(),
        args.use_certs,
    )
    .await?;

    // Connect to peers
    for peer_addr in &args.connect {
        if let Err(e) = node.connect_to_peer(peer_addr).await {
            warn!("Failed to connect to peer {}: {}", peer_addr, e);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Show mesh status after connecting
    if !args.connect.is_empty() {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        node.show_mesh_status().await;
    }

    // Create agent if requested
    if args.agent {
        node.create_agent().await?;
    }

    // Migrate agent if requested
    if let Some(target) = &args.migrate_to {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        node.migrate_agent_to(target).await?;
    }

    // Run server
    node.run_server().await
}
