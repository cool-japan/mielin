//! Node-related data structures

use crate::output::MultiFormatDisplay;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NodeInfo {
    pub id: String,
    pub role: String,
    pub state: String,
    pub address: String,
    pub agents: usize,
    pub uptime: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct NodeList {
    pub nodes: Vec<NodeInfo>,
    pub total: usize,
}

impl MultiFormatDisplay for NodeList {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("ROLE").fg(Color::Cyan),
                Cell::new("STATE").fg(Color::Cyan),
                Cell::new("ADDRESS").fg(Color::Cyan),
                Cell::new("AGENTS").fg(Color::Cyan),
                Cell::new("UPTIME").fg(Color::Cyan),
            ]);

        for node in &self.nodes {
            let state_color = match node.state.as_str() {
                "Running" => Color::Green,
                "Stopped" => Color::Red,
                _ => Color::Yellow,
            };

            table.add_row(vec![
                Cell::new(&node.id[..8]),
                Cell::new(&node.role),
                Cell::new(&node.state).fg(state_color),
                Cell::new(&node.address),
                Cell::new(node.agents.to_string()),
                Cell::new(&node.uptime),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.nodes
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Serialize)]
pub struct MeshStatus {
    pub state: String,
    pub local_node: String,
    pub connected_peers: usize,
    pub total_agents: usize,
    pub gossip_round: u64,
    pub dht_entries: usize,
}

impl MultiFormatDisplay for MeshStatus {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        let state_color = match self.state.as_str() {
            "Healthy" => Color::Green,
            "Degraded" => Color::Yellow,
            "Critical" => Color::Red,
            _ => Color::White,
        };

        table.add_row(vec![
            Cell::new("Status").fg(Color::Cyan),
            Cell::new(&self.state).fg(state_color),
        ]);
        table.add_row(vec![
            Cell::new("Local Node").fg(Color::Cyan),
            Cell::new(&self.local_node),
        ]);
        table.add_row(vec![
            Cell::new("Connected Peers").fg(Color::Cyan),
            Cell::new(self.connected_peers.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Total Agents").fg(Color::Cyan),
            Cell::new(self.total_agents.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Gossip Round").fg(Color::Cyan),
            Cell::new(self.gossip_round.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("DHT Entries").fg(Color::Cyan),
            Cell::new(self.dht_entries.to_string()),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        self.state.clone()
    }
}
