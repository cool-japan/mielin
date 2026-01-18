//! Audit command implementations

use crate::audit::{AuditEventType, AuditLogger, AuditSeverity};
use crate::output::OutputFormat;
use anyhow::Result;
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use std::path::PathBuf;

/// Audit commands
#[derive(Debug, Args)]
pub struct AuditCommand {
    #[command(subcommand)]
    command: AuditSubcommand,
}

#[derive(Debug, Subcommand)]
enum AuditSubcommand {
    /// Show audit log entries
    #[command(visible_aliases = &["list", "ls"])]
    Show {
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,

        /// Filter by event type
        #[arg(short = 't', long)]
        event_type: Option<String>,

        /// Filter by severity (info, warning, error, critical)
        #[arg(short = 's', long)]
        severity: Option<String>,

        /// Filter by user
        #[arg(short = 'u', long)]
        user: Option<String>,
    },

    /// Query audit logs with advanced filters
    #[command(visible_aliases = &["search", "find"])]
    Query {
        /// Search pattern (searches in command and args)
        pattern: String,

        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "100")]
        limit: usize,

        /// Show only failed commands
        #[arg(long)]
        failed_only: bool,
    },

    /// Show audit statistics
    #[command(visible_aliases = &["statistics", "info"])]
    Stats,

    /// Clear audit logs
    #[command(visible_aliases = &["clean", "purge"])]
    Clear {
        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Export audit logs to file
    #[command(visible_aliases = &["save", "dump"])]
    Export {
        /// Output file path
        path: PathBuf,

        /// Export format (json, csv)
        #[arg(short = 'f', long, default_value = "json")]
        format: String,
    },
}

impl AuditCommand {
    pub async fn execute(&self, output_format: OutputFormat) -> Result<()> {
        match &self.command {
            AuditSubcommand::Show {
                limit,
                event_type,
                severity,
                user,
            } => {
                show_command(
                    *limit,
                    event_type.as_deref(),
                    severity.as_deref(),
                    user.as_deref(),
                    output_format,
                )
                .await
            }
            AuditSubcommand::Query {
                pattern,
                limit,
                failed_only,
            } => query_command(pattern, *limit, *failed_only, output_format).await,
            AuditSubcommand::Stats => stats_command(output_format).await,
            AuditSubcommand::Clear { yes } => clear_command(*yes).await,
            AuditSubcommand::Export { path, format } => export_command(path, format).await,
        }
    }
}

async fn show_command(
    limit: usize,
    event_type: Option<&str>,
    severity: Option<&str>,
    user: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let logger = AuditLogger::with_default_config()?;

    // Parse event type
    let event_type_filter = event_type.and_then(|et| match et.to_lowercase().as_str() {
        "command" | "cmd" => Some(AuditEventType::CommandExecution),
        "config" | "cfg" => Some(AuditEventType::ConfigChange),
        "auth" | "authentication" => Some(AuditEventType::Authentication),
        "authz" | "authorization" => Some(AuditEventType::Authorization),
        "security" | "sec" => Some(AuditEventType::Security),
        "system" | "sys" => Some(AuditEventType::System),
        _ => None,
    });

    // Parse severity
    let severity_filter = severity.and_then(|sev| match sev.to_lowercase().as_str() {
        "info" => Some(AuditSeverity::Info),
        "warning" | "warn" => Some(AuditSeverity::Warning),
        "error" | "err" => Some(AuditSeverity::Error),
        "critical" | "crit" => Some(AuditSeverity::Critical),
        _ => None,
    });

    let entries = logger.query_entries(
        event_type_filter,
        severity_filter,
        user,
        None,
        None,
        Some(limit),
    )?;

    if entries.is_empty() {
        println!("No audit entries found");
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&entries)?);
        }
        OutputFormat::Quiet => {
            for entry in &entries {
                println!("{}", entry.command);
            }
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec![
                    "Timestamp",
                    "User",
                    "Command",
                    "Type",
                    "Severity",
                    "Exit",
                ]);

            for entry in &entries {
                let severity_cell = match entry.severity {
                    AuditSeverity::Critical => {
                        Cell::new(format!("{:?}", entry.severity)).fg(Color::Red)
                    }
                    AuditSeverity::Error => {
                        Cell::new(format!("{:?}", entry.severity)).fg(Color::Red)
                    }
                    AuditSeverity::Warning => {
                        Cell::new(format!("{:?}", entry.severity)).fg(Color::Yellow)
                    }
                    AuditSeverity::Info => {
                        Cell::new(format!("{:?}", entry.severity)).fg(Color::Green)
                    }
                };

                let exit_cell = match entry.exit_code {
                    Some(0) => Cell::new("0").fg(Color::Green),
                    Some(code) => Cell::new(format!("{}", code)).fg(Color::Red),
                    None => Cell::new("-"),
                };

                let command_str = if entry.args.is_empty() {
                    entry.command.clone()
                } else {
                    format!("{} {}", entry.command, entry.args.join(" "))
                };

                table.add_row(vec![
                    Cell::new(entry.timestamp.format("%Y-%m-%d %H:%M:%S")),
                    Cell::new(&entry.user),
                    Cell::new(command_str),
                    Cell::new(format!("{:?}", entry.event_type)),
                    severity_cell,
                    exit_cell,
                ]);
            }

            println!("{}", table);
            println!("\nShowing {} of total audit entries", entries.len());
        }
    }

    Ok(())
}

async fn query_command(
    pattern: &str,
    limit: usize,
    failed_only: bool,
    format: OutputFormat,
) -> Result<()> {
    let logger = AuditLogger::with_default_config()?;
    let mut entries = logger.read_entries()?;

    // Filter by pattern
    entries.retain(|entry| {
        let command_str = format!("{} {}", entry.command, entry.args.join(" "));
        command_str.to_lowercase().contains(&pattern.to_lowercase())
    });

    // Filter by failed only
    if failed_only {
        entries.retain(|entry| entry.exit_code.is_some() && entry.exit_code != Some(0));
    }

    // Sort by timestamp (newest first)
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Limit
    entries.truncate(limit);

    if entries.is_empty() {
        println!("No matching audit entries found for pattern: {}", pattern);
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&entries)?);
        }
        OutputFormat::Quiet => {
            for entry in &entries {
                println!("{}", entry.command);
            }
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(vec!["Timestamp", "Command", "Exit", "Duration"]);

            for entry in &entries {
                let exit_cell = match entry.exit_code {
                    Some(0) => Cell::new("0").fg(Color::Green),
                    Some(code) => Cell::new(format!("{}", code)).fg(Color::Red),
                    None => Cell::new("-"),
                };

                let duration_str = entry
                    .duration_ms
                    .map(|ms| format!("{}ms", ms))
                    .unwrap_or_else(|| "-".to_string());

                let command_str = if entry.args.is_empty() {
                    entry.command.clone()
                } else {
                    format!("{} {}", entry.command, entry.args.join(" "))
                };

                table.add_row(vec![
                    Cell::new(entry.timestamp.format("%Y-%m-%d %H:%M:%S")),
                    Cell::new(command_str),
                    exit_cell,
                    Cell::new(duration_str),
                ]);
            }

            println!("{}", table);
            println!("\nFound {} matching entries", entries.len());
        }
    }

    Ok(())
}

async fn stats_command(format: OutputFormat) -> Result<()> {
    let logger = AuditLogger::with_default_config()?;
    let stats = logger.get_stats()?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&stats)?);
        }
        OutputFormat::Quiet => {
            println!("{}", stats.total_entries);
        }
        OutputFormat::Table => {
            println!("Audit Log Statistics");
            println!("====================\n");

            println!("Total Entries: {}", stats.total_entries);

            if let (Some(oldest), Some(newest)) = (stats.oldest_entry, stats.newest_entry) {
                println!(
                    "Date Range: {} to {}",
                    oldest.format("%Y-%m-%d %H:%M:%S"),
                    newest.format("%Y-%m-%d %H:%M:%S")
                );
            }

            println!("\nBy Event Type:");
            for (event_type, count) in &stats.by_event_type {
                println!("  {}: {}", event_type, count);
            }

            println!("\nBy Severity:");
            for (severity, count) in &stats.by_severity {
                println!("  {}: {}", severity, count);
            }

            println!("\nBy User:");
            for (user, count) in &stats.by_user {
                println!("  {}: {}", user, count);
            }
        }
    }

    Ok(())
}

async fn clear_command(yes: bool) -> Result<()> {
    if !yes {
        print!("Are you sure you want to clear all audit logs? [y/N] ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted");
            return Ok(());
        }
    }

    let logger = AuditLogger::with_default_config()?;
    logger.clear()?;

    println!("✓ Audit logs cleared successfully");

    Ok(())
}

async fn export_command(path: &PathBuf, format: &str) -> Result<()> {
    let logger = AuditLogger::with_default_config()?;
    let entries = logger.read_entries()?;

    let content = match format.to_lowercase().as_str() {
        "json" => serde_json::to_string_pretty(&entries)?,
        "csv" => {
            let mut csv = String::from(
                "timestamp,user,command,args,exit_code,duration_ms,event_type,severity\n",
            );
            for entry in &entries {
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{:?},{:?}\n",
                    entry.timestamp.to_rfc3339(),
                    entry.user,
                    entry.command,
                    entry.args.join(" "),
                    entry.exit_code.map(|c| c.to_string()).unwrap_or_default(),
                    entry.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
                    entry.event_type,
                    entry.severity,
                ));
            }
            csv
        }
        _ => anyhow::bail!("Unsupported export format: {}. Use 'json' or 'csv'", format),
    };

    std::fs::write(path, content)?;

    println!(
        "✓ Exported {} audit entries to {}",
        entries.len(),
        path.display()
    );

    Ok(())
}

/// Handle audit command
pub async fn handle_audit_command(command: AuditCommand, format: OutputFormat) -> Result<()> {
    command.execute(format).await
}
