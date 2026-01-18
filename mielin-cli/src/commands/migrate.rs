//! Migration command handlers

use crate::output::{render_output, OutputFormat};
use crate::types::{MigrationEntry, MigrationHistory, OperationResult};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum MigrateCommands {
    /// Show migration status
    #[command(aliases = ["st", "stat"])]
    Status,
    /// Cancel a pending migration
    #[command(aliases = ["abort", "stop"])]
    Cancel {
        /// Migration ID
        migration_id: String,
    },
    /// Show migration history
    #[command(aliases = ["hist", "log"])]
    History {
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}

pub async fn handle_migrate_command(action: MigrateCommands, format: OutputFormat) -> Result<()> {
    match action {
        MigrateCommands::Status => {
            let result = OperationResult {
                success: true,
                message: "No migrations in progress".to_string(),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        MigrateCommands::Cancel { migration_id } => {
            let result = OperationResult {
                success: true,
                message: format!("Cancelled migration {}", migration_id),
                id: Some(migration_id),
            };
            println!("{}", render_output(&result, format)?);
        }
        MigrateCommands::History { limit: _ } => {
            let data = mock_migration_history();
            println!("{}", render_output(&data, format)?);
        }
    }
    Ok(())
}

fn mock_migration_history() -> MigrationHistory {
    MigrationHistory {
        migrations: vec![
            MigrationEntry {
                id: "mig-001-uuid-here-1234567890ab".to_string(),
                agent_id: "agent-001-uuid-here-1234567890ab".to_string(),
                source: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                target: "b2c3d4e5-f6a7-8901-bcde-f12345678901".to_string(),
                status: "Completed".to_string(),
                started_at: "2026-01-18 10:30:00".to_string(),
                duration_ms: Some(45),
            },
            MigrationEntry {
                id: "mig-002-uuid-here-abcdef123456".to_string(),
                agent_id: "agent-002-uuid-here-abcdef123456".to_string(),
                source: "b2c3d4e5-f6a7-8901-bcde-f12345678901".to_string(),
                target: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                status: "Completed".to_string(),
                started_at: "2026-01-18 10:25:00".to_string(),
                duration_ms: Some(38),
            },
        ],
        total: 2,
    }
}
