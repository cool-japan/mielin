//! Mesh network command handlers

use crate::output::{render_output, OutputFormat};
use crate::types::{MeshStatus, OperationResult, PeerInfo, PeerList};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum MeshCommands {
    /// Show mesh network status
    #[command(aliases = ["st", "stat"])]
    Status,
    /// List connected peers
    #[command(aliases = ["ls", "list"])]
    Peers,
    /// Show gossip protocol status
    Gossip,
    /// Show DHT status
    Dht,
}

pub async fn handle_mesh_command(action: MeshCommands, format: OutputFormat) -> Result<()> {
    match action {
        MeshCommands::Status => {
            let data = mock_mesh_status();
            println!("{}", render_output(&data, format)?);
        }
        MeshCommands::Peers => {
            let data = mock_peer_list();
            println!("{}", render_output(&data, format)?);
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
