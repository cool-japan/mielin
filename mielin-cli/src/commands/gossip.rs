//! Gossip protocol command handlers

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use crate::types::OperationResult;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Subcommand)]
pub enum GossipCommands {
    /// Show gossip protocol status
    #[command(aliases = ["st", "stat"])]
    Status,
    /// List membership information
    #[command(aliases = ["ls", "list"])]
    Members {
        /// Show only alive members
        #[arg(short, long)]
        alive: bool,
    },
    /// Force gossip synchronization
    #[command(aliases = ["synchronize", "refresh"])]
    Sync {
        /// Target peer ID to sync with
        peer_id: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct GossipStatus {
    state: String,
    protocol_version: u32,
    round: u64,
    messages_sent: u64,
    messages_received: u64,
    active_peers: usize,
    convergence_rate: f64,
}

impl MultiFormatDisplay for GossipStatus {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        let state_color = match self.state.as_str() {
            "Active" => Color::Green,
            "Degraded" => Color::Yellow,
            "Stopped" => Color::Red,
            _ => Color::White,
        };

        table.add_row(vec![
            Cell::new("State").fg(Color::Cyan),
            Cell::new(&self.state).fg(state_color),
        ]);
        table.add_row(vec![
            Cell::new("Protocol Version").fg(Color::Cyan),
            Cell::new(self.protocol_version.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Current Round").fg(Color::Cyan),
            Cell::new(self.round.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Messages Sent").fg(Color::Cyan),
            Cell::new(self.messages_sent.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Messages Received").fg(Color::Cyan),
            Cell::new(self.messages_received.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Active Peers").fg(Color::Cyan),
            Cell::new(self.active_peers.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Convergence Rate").fg(Color::Cyan),
            Cell::new(format!("{:.2}%", self.convergence_rate * 100.0)),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        self.state.clone()
    }
}

#[derive(Debug, Serialize)]
struct GossipMember {
    id: String,
    address: String,
    state: String,
    incarnation: u64,
    last_seen: String,
    rtt_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
struct GossipMemberList {
    members: Vec<GossipMember>,
    total: usize,
    alive: usize,
    suspect: usize,
    dead: usize,
}

impl MultiFormatDisplay for GossipMemberList {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("MEMBER ID").fg(Color::Cyan),
                Cell::new("ADDRESS").fg(Color::Cyan),
                Cell::new("STATE").fg(Color::Cyan),
                Cell::new("INCARNATION").fg(Color::Cyan),
                Cell::new("RTT").fg(Color::Cyan),
                Cell::new("LAST SEEN").fg(Color::Cyan),
            ]);

        for member in &self.members {
            let state_color = match member.state.as_str() {
                "Alive" => Color::Green,
                "Suspect" => Color::Yellow,
                "Dead" => Color::Red,
                _ => Color::White,
            };

            let rtt = member
                .rtt_ms
                .map(|r| format!("{:.1}ms", r))
                .unwrap_or_else(|| "N/A".to_string());

            table.add_row(vec![
                Cell::new(&member.id[..8]),
                Cell::new(&member.address),
                Cell::new(&member.state).fg(state_color),
                Cell::new(member.incarnation.to_string()),
                Cell::new(rtt),
                Cell::new(&member.last_seen),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.members
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub async fn handle_gossip_command(action: GossipCommands, format: OutputFormat) -> Result<()> {
    match action {
        GossipCommands::Status => {
            let data = mock_gossip_status();
            println!("{}", render_output(&data, format)?);
        }
        GossipCommands::Members { alive } => {
            let data = mock_gossip_members(alive);
            println!("{}", render_output(&data, format)?);
        }
        GossipCommands::Sync { peer_id } => {
            let message = if let Some(id) = peer_id {
                format!("Forcing gossip sync with peer {}", id)
            } else {
                "Forcing gossip sync with all peers".to_string()
            };

            let result = OperationResult {
                success: true,
                message,
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
    }
    Ok(())
}

fn mock_gossip_status() -> GossipStatus {
    GossipStatus {
        state: "Active".to_string(),
        protocol_version: 1,
        round: 1542,
        messages_sent: 45678,
        messages_received: 45234,
        active_peers: 5,
        convergence_rate: 0.98,
    }
}

fn mock_gossip_members(alive_only: bool) -> GossipMemberList {
    let mut members = vec![
        GossipMember {
            id: "member-001-uuid-here-1234567890ab".to_string(),
            address: "192.168.1.101:9000".to_string(),
            state: "Alive".to_string(),
            incarnation: 15,
            last_seen: "2s ago".to_string(),
            rtt_ms: Some(2.3),
        },
        GossipMember {
            id: "member-002-uuid-here-abcdef123456".to_string(),
            address: "192.168.1.102:9000".to_string(),
            state: "Alive".to_string(),
            incarnation: 12,
            last_seen: "1s ago".to_string(),
            rtt_ms: Some(1.8),
        },
        GossipMember {
            id: "member-003-uuid-here-fedcba654321".to_string(),
            address: "192.168.1.103:9000".to_string(),
            state: "Suspect".to_string(),
            incarnation: 8,
            last_seen: "15s ago".to_string(),
            rtt_ms: Some(45.2),
        },
        GossipMember {
            id: "member-004-uuid-here-987654321abc".to_string(),
            address: "192.168.1.104:9000".to_string(),
            state: "Dead".to_string(),
            incarnation: 5,
            last_seen: "5m ago".to_string(),
            rtt_ms: None,
        },
    ];

    if alive_only {
        members.retain(|m| m.state == "Alive");
    }

    let alive = members.iter().filter(|m| m.state == "Alive").count();
    let suspect = members.iter().filter(|m| m.state == "Suspect").count();
    let dead = members.iter().filter(|m| m.state == "Dead").count();

    GossipMemberList {
        total: members.len(),
        members,
        alive,
        suspect,
        dead,
    }
}
