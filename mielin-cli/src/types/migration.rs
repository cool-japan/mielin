//! Migration-related data structures

use crate::output::MultiFormatDisplay;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct MigrationEntry {
    pub id: String,
    pub agent_id: String,
    pub source: String,
    pub target: String,
    pub status: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct MigrationHistory {
    pub migrations: Vec<MigrationEntry>,
    pub total: usize,
}

impl MultiFormatDisplay for MigrationHistory {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("AGENT").fg(Color::Cyan),
                Cell::new("SOURCE").fg(Color::Cyan),
                Cell::new("TARGET").fg(Color::Cyan),
                Cell::new("STATUS").fg(Color::Cyan),
                Cell::new("DURATION").fg(Color::Cyan),
            ]);

        for m in &self.migrations {
            let status_color = match m.status.as_str() {
                "Completed" => Color::Green,
                "Failed" => Color::Red,
                "InProgress" => Color::Blue,
                _ => Color::White,
            };

            let duration = m
                .duration_ms
                .map(|d| format!("{}ms", d))
                .unwrap_or_else(|| "-".to_string());

            table.add_row(vec![
                Cell::new(&m.id[..8]),
                Cell::new(&m.agent_id[..8]),
                Cell::new(&m.source[..8]),
                Cell::new(&m.target[..8]),
                Cell::new(&m.status).fg(status_color),
                Cell::new(duration),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.migrations
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
