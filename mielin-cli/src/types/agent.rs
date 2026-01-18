//! Agent-related data structures

use crate::output::MultiFormatDisplay;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub state: String,
    pub node: String,
    pub dna_hash: String,
    pub memory_mb: f64,
    pub uptime: String,
}

#[derive(Debug, Serialize)]
pub struct AgentList {
    pub agents: Vec<AgentInfo>,
    pub total: usize,
}

impl MultiFormatDisplay for AgentList {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("STATE").fg(Color::Cyan),
                Cell::new("NODE").fg(Color::Cyan),
                Cell::new("DNA").fg(Color::Cyan),
                Cell::new("MEMORY").fg(Color::Cyan),
                Cell::new("UPTIME").fg(Color::Cyan),
            ]);

        for agent in &self.agents {
            let state_color = match agent.state.as_str() {
                "Running" => Color::Green,
                "Paused" => Color::Yellow,
                "Error" => Color::Red,
                "Migrating" => Color::Blue,
                _ => Color::White,
            };

            table.add_row(vec![
                Cell::new(&agent.id[..8]),
                Cell::new(&agent.state).fg(state_color),
                Cell::new(&agent.node[..8]),
                Cell::new(&agent.dna_hash[..8]),
                Cell::new(format!("{:.1} MB", agent.memory_mb)),
                Cell::new(&agent.uptime),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.agents
            .iter()
            .map(|a| a.id.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
