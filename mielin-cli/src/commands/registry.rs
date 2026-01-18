//! Registry operation command handlers

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Subcommand)]
pub enum RegistryCommands {
    /// List all agents in registry
    #[command(alias = "ls")]
    List {
        /// Filter by pattern
        #[arg(short, long)]
        pattern: Option<String>,
    },
    /// Query agents by pattern
    #[command(aliases = ["search", "find"])]
    Query {
        /// Search pattern
        pattern: String,
    },
    /// Show registry statistics
    #[command(aliases = ["statistics", "info"])]
    Stats,
}

#[derive(Debug, Serialize)]
struct RegistryEntry {
    dna_hash: String,
    name: String,
    version: String,
    instances: usize,
    total_memory_mb: f64,
}

#[derive(Debug, Serialize)]
struct RegistryList {
    entries: Vec<RegistryEntry>,
    total: usize,
}

impl MultiFormatDisplay for RegistryList {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("DNA HASH").fg(Color::Cyan),
                Cell::new("NAME").fg(Color::Cyan),
                Cell::new("VERSION").fg(Color::Cyan),
                Cell::new("INSTANCES").fg(Color::Cyan),
                Cell::new("MEMORY").fg(Color::Cyan),
            ]);

        for entry in &self.entries {
            table.add_row(vec![
                Cell::new(&entry.dna_hash[..16]),
                Cell::new(&entry.name),
                Cell::new(&entry.version),
                Cell::new(entry.instances.to_string()),
                Cell::new(format!("{:.1} MB", entry.total_memory_mb)),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.entries
            .iter()
            .map(|e| e.dna_hash.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Serialize)]
struct RegistryStats {
    total_entries: usize,
    total_instances: usize,
    total_memory_mb: f64,
    unique_agents: usize,
}

impl MultiFormatDisplay for RegistryStats {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Total Entries").fg(Color::Cyan),
            Cell::new(self.total_entries.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Total Instances").fg(Color::Cyan),
            Cell::new(self.total_instances.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Total Memory").fg(Color::Cyan),
            Cell::new(format!("{:.1} MB", self.total_memory_mb)),
        ]);
        table.add_row(vec![
            Cell::new("Unique Agents").fg(Color::Cyan),
            Cell::new(self.unique_agents.to_string()),
        ]);

        table
    }
}

pub async fn handle_registry_command(action: RegistryCommands, format: OutputFormat) -> Result<()> {
    match action {
        RegistryCommands::List { pattern } => {
            let data = mock_registry_list(pattern);
            println!("{}", render_output(&data, format)?);
        }
        RegistryCommands::Query { pattern } => {
            let data = mock_registry_list(Some(pattern));
            println!("{}", render_output(&data, format)?);
        }
        RegistryCommands::Stats => {
            let data = mock_registry_stats();
            println!("{}", render_output(&data, format)?);
        }
    }
    Ok(())
}

fn mock_registry_list(_pattern: Option<String>) -> RegistryList {
    RegistryList {
        entries: vec![
            RegistryEntry {
                dna_hash: "sha256:abc123def456789012345678901234567890".to_string(),
                name: "neural-processor".to_string(),
                version: "1.0.0".to_string(),
                instances: 5,
                total_memory_mb: 62.5,
            },
            RegistryEntry {
                dna_hash: "sha256:def456abc789012345678901234567890123".to_string(),
                name: "data-analyzer".to_string(),
                version: "0.9.2".to_string(),
                instances: 3,
                total_memory_mb: 24.6,
            },
        ],
        total: 2,
    }
}

fn mock_registry_stats() -> RegistryStats {
    RegistryStats {
        total_entries: 2,
        total_instances: 8,
        total_memory_mb: 87.1,
        unique_agents: 2,
    }
}
