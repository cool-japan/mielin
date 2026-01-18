//! Peer-related data structures

use crate::output::MultiFormatDisplay;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PeerInfo {
    pub id: String,
    pub address: String,
    pub latency_ms: f64,
    pub state: String,
    pub last_seen: String,
}

#[derive(Debug, Serialize)]
pub struct PeerList {
    pub peers: Vec<PeerInfo>,
    pub total: usize,
}

impl MultiFormatDisplay for PeerList {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("PEER ID").fg(Color::Cyan),
                Cell::new("ADDRESS").fg(Color::Cyan),
                Cell::new("LATENCY").fg(Color::Cyan),
                Cell::new("STATE").fg(Color::Cyan),
                Cell::new("LAST SEEN").fg(Color::Cyan),
            ]);

        for peer in &self.peers {
            let state_color = match peer.state.as_str() {
                "Connected" => Color::Green,
                "Suspicious" => Color::Yellow,
                "Failed" => Color::Red,
                _ => Color::White,
            };

            table.add_row(vec![
                Cell::new(&peer.id[..8]),
                Cell::new(&peer.address),
                Cell::new(format!("{:.1}ms", peer.latency_ms)),
                Cell::new(&peer.state).fg(state_color),
                Cell::new(&peer.last_seen),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.peers
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
