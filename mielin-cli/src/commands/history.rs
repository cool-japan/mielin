//! History command implementations

use crate::history::History;
use crate::output::OutputFormat;
use anyhow::Result;
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use std::path::PathBuf;

/// History management commands
#[derive(Debug, Args)]
pub struct HistoryCommand {
    #[command(subcommand)]
    command: HistorySubcommand,
}

#[derive(Debug, Subcommand)]
enum HistorySubcommand {
    /// List command history
    #[command(visible_aliases = &["ls", "list"])]
    Show {
        /// Number of entries to show (default: 20)
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,

        /// Output format
        #[arg(short = 'o', long, value_enum)]
        output: Option<OutputFormat>,
    },

    /// Search command history
    #[command(visible_aliases = &["find", "grep"])]
    Search {
        /// Search pattern
        pattern: String,

        /// Output format
        #[arg(short = 'o', long, value_enum)]
        output: Option<OutputFormat>,
    },

    /// Show history statistics
    #[command(visible_aliases = &["stat", "info"])]
    Stats {
        /// Output format
        #[arg(short = 'o', long, value_enum)]
        output: Option<OutputFormat>,
    },

    /// Clear all history
    #[command(visible_aliases = &["delete", "remove"])]
    Clear {
        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Export history to file
    #[command(visible_aliases = &["save", "dump"])]
    Export {
        /// Output file path
        path: PathBuf,
    },
}

impl HistoryCommand {
    pub async fn execute(&self, output_format: OutputFormat) -> Result<()> {
        match &self.command {
            HistorySubcommand::Show { limit, output } => {
                let format = output.unwrap_or(output_format);
                show_history(*limit, format).await
            }
            HistorySubcommand::Search { pattern, output } => {
                let format = output.unwrap_or(output_format);
                search_history(pattern, format).await
            }
            HistorySubcommand::Stats { output } => {
                let format = output.unwrap_or(output_format);
                show_stats(format).await
            }
            HistorySubcommand::Clear { yes } => clear_history(*yes).await,
            HistorySubcommand::Export { path } => export_history(path).await,
        }
    }
}

async fn show_history(limit: usize, format: OutputFormat) -> Result<()> {
    let history = History::load()?;
    let entries = history.last(limit);

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(entries)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(entries)?);
        }
        OutputFormat::Quiet => {
            // In quiet mode, just output the commands
            for entry in entries {
                println!("{}", entry.command);
            }
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL).set_header(vec![
                "#",
                "Timestamp",
                "Command",
                "Exit Code",
                "Duration (ms)",
            ]);

            for (idx, entry) in entries.iter().enumerate() {
                let exit_code_cell = if entry.exit_code == 0 {
                    Cell::new(entry.exit_code).fg(Color::Green)
                } else {
                    Cell::new(entry.exit_code).fg(Color::Red)
                };

                table.add_row(vec![
                    Cell::new(idx + 1),
                    Cell::new(entry.timestamp.format("%Y-%m-%d %H:%M:%S")),
                    Cell::new(&entry.command),
                    exit_code_cell,
                    Cell::new(entry.duration_ms),
                ]);
            }

            println!("{}", table);
        }
    }

    Ok(())
}

async fn search_history(pattern: &str, format: OutputFormat) -> Result<()> {
    let history = History::load()?;
    let results = history.search(pattern);

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&results)?);
        }
        OutputFormat::Quiet => {
            // In quiet mode, just output the matching commands
            for entry in results {
                println!("{}", entry.command);
            }
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL).set_header(vec![
                "#",
                "Timestamp",
                "Command",
                "Exit Code",
                "Duration (ms)",
            ]);

            for (idx, entry) in results.iter().enumerate() {
                let exit_code_cell = if entry.exit_code == 0 {
                    Cell::new(entry.exit_code).fg(Color::Green)
                } else {
                    Cell::new(entry.exit_code).fg(Color::Red)
                };

                table.add_row(vec![
                    Cell::new(idx + 1),
                    Cell::new(entry.timestamp.format("%Y-%m-%d %H:%M:%S")),
                    Cell::new(&entry.command),
                    exit_code_cell,
                    Cell::new(entry.duration_ms),
                ]);
            }

            println!("{}", table);
            println!("\nFound {} matching command(s)", results.len());
        }
    }

    Ok(())
}

async fn show_stats(format: OutputFormat) -> Result<()> {
    let history = History::load()?;
    let stats = history.stats();

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&stats)?);
        }
        OutputFormat::Quiet => {
            // In quiet mode, just output the total command count
            println!("{}", stats.total_commands);
        }
        OutputFormat::Table => {
            println!("Command History Statistics");
            println!("==========================");
            println!("Total Commands:       {}", stats.total_commands);
            println!(
                "Successful Commands:  {} ({:.1}%)",
                stats.successful_commands,
                if stats.total_commands > 0 {
                    (stats.successful_commands as f64 / stats.total_commands as f64) * 100.0
                } else {
                    0.0
                }
            );
            println!(
                "Failed Commands:      {} ({:.1}%)",
                stats.failed_commands,
                if stats.total_commands > 0 {
                    (stats.failed_commands as f64 / stats.total_commands as f64) * 100.0
                } else {
                    0.0
                }
            );
            println!("Avg Duration:         {} ms", stats.avg_duration_ms);

            if !stats.most_used_commands.is_empty() {
                println!("\nMost Used Commands:");
                for (idx, (cmd, count)) in stats.most_used_commands.iter().enumerate() {
                    println!("  {}. {} ({} times)", idx + 1, cmd, count);
                }
            }
        }
    }

    Ok(())
}

async fn clear_history(yes: bool) -> Result<()> {
    if !yes {
        println!("Are you sure you want to clear all command history? (y/N)");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let mut history = History::load()?;
    history.clear();
    history.save()?;

    println!("Command history cleared successfully");
    Ok(())
}

async fn export_history(path: &PathBuf) -> Result<()> {
    let history = History::load()?;
    history.export(path)?;

    println!("History exported to: {}", path.display());
    Ok(())
}
