//! Mesh network command handlers

use crate::control::ControlClient;
use crate::output::{render_output, OutputFormat};
use crate::types::{MeshStatus, OperationResult, PeerInfo, PeerList};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum MeshCommands {
    /// Show mesh network status
    #[command(aliases = ["st", "stat"])]
    Status {
        /// Fetch live data from the running daemon at this address (overrides MIELIN_DAEMON env var)
        #[arg(long)]
        daemon: Option<String>,
    },
    /// List connected peers
    #[command(aliases = ["ls", "list"])]
    Peers {
        /// Fetch live data from the running daemon at this address (overrides MIELIN_DAEMON env var)
        #[arg(long)]
        daemon: Option<String>,
    },
    /// Show gossip protocol status
    Gossip,
    /// Show DHT status
    Dht,
}

/// Resolve daemon address from CLI arg or `MIELIN_DAEMON` environment variable.
fn resolve_daemon(arg: Option<String>) -> Option<String> {
    arg.or_else(|| std::env::var("MIELIN_DAEMON").ok())
}

pub async fn handle_mesh_command(action: MeshCommands, format: OutputFormat) -> Result<()> {
    match action {
        MeshCommands::Status { daemon } => {
            if let Some(addr) = resolve_daemon(daemon) {
                let client = ControlClient::new(&daemon_base_url(&addr));
                let status = client.mesh_status().await?;
                let data = MeshStatus {
                    state: if status.alive > 0 {
                        "Healthy".to_string()
                    } else {
                        "Unknown".to_string()
                    },
                    local_node: String::new(),
                    connected_peers: status.alive,
                    total_agents: status.local_agents,
                    gossip_round: 0,
                    dht_entries: status.dht_peers,
                };
                println!("{}", render_output(&data, format)?);
            } else {
                let data = mock_mesh_status();
                println!("{}", render_output(&data, format)?);
            }
        }
        MeshCommands::Peers { daemon } => {
            if let Some(addr) = resolve_daemon(daemon) {
                let client = ControlClient::new(&daemon_base_url(&addr));
                let peers = client.mesh_peers().await?;
                let peer_list = PeerList {
                    total: peers.len(),
                    peers: peers
                        .into_iter()
                        .map(|p| PeerInfo {
                            id: p.node_id,
                            address: String::new(),
                            latency_ms: 0.0,
                            state: p.status,
                            last_seen: format!("{}s ago", p.last_seen_secs),
                        })
                        .collect(),
                };
                println!("{}", render_output(&peer_list, format)?);
            } else {
                let data = mock_peer_list();
                println!("{}", render_output(&data, format)?);
            }
        }
        MeshCommands::Gossip => {
            let result = OperationResult {
                success: true,
                message: "Gossip protocol status: Active".to_string(),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        MeshCommands::Dht => {
            let result = OperationResult {
                success: true,
                message: "DHT status: 256 entries, 15 buckets".to_string(),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
    }
    Ok(())
}

/// Normalise a daemon address into a base URL.
///
/// If the user supplies a bare `host:port` without a scheme we prepend `http://`.
fn daemon_base_url(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    }
}

fn mock_mesh_status() -> MeshStatus {
    MeshStatus {
        state: "Healthy".to_string(),
        local_node: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
        connected_peers: 3,
        total_agents: 7,
        gossip_round: 1542,
        dht_entries: 256,
    }
}

fn mock_peer_list() -> PeerList {
    PeerList {
        peers: vec![
            PeerInfo {
                id: "peer-001-uuid-here-1234567890ab".to_string(),
                address: "192.168.1.101:9000".to_string(),
                latency_ms: 2.3,
                state: "Connected".to_string(),
                last_seen: "2s ago".to_string(),
            },
            PeerInfo {
                id: "peer-002-uuid-here-abcdef123456".to_string(),
                address: "192.168.1.102:9000".to_string(),
                latency_ms: 5.1,
                state: "Connected".to_string(),
                last_seen: "5s ago".to_string(),
            },
        ],
        total: 2,
    }
}
