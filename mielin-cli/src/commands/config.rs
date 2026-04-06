//! Configuration management command implementations

use crate::config::Config;
use crate::config_validator::{ConfigMigrator, ConfigValidator, ErrorSeverity};
use crate::output::OutputFormat;
use anyhow::Result;
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};

/// Configuration management commands
#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    /// Validate configuration file
    #[command(visible_aliases = &["check", "verify"])]
    Validate {
        /// Path to configuration file (uses default if not specified)
        #[arg(short = 'f', long)]
        file: Option<std::path::PathBuf>,

        /// Show suggestions even if config is valid
        #[arg(short = 's', long)]
        show_suggestions: bool,
    },

    /// Auto-fix configuration issues
    #[command(visible_aliases = &["repair", "fix"])]
    AutoFix {
        /// Path to configuration file (uses default if not specified)
        #[arg(short = 'f', long)]
        file: Option<std::path::PathBuf>,

        /// Dry run (show fixes without applying them)
        #[arg(short = 'd', long)]
        dry_run: bool,
    },

    /// Migrate configuration to newer version
    #[command(visible_aliases = &["upgrade", "update"])]
    Migrate {
        /// Source configuration file
        #[arg(short = 'f', long)]
        from: std::path::PathBuf,

        /// Destination file (overwrites source if not specified)
        #[arg(short = 't', long)]
        to: Option<std::path::PathBuf>,

        /// Source version
        #[arg(short = 'v', long)]
        from_version: String,
    },

    /// Show configuration file path
    #[command(visible_aliases = &["location", "path"])]
    Show,

    /// Initialize a new configuration file
    #[command(visible_aliases = &["create", "new"])]
    Init {
        /// Output path (uses default if not specified)
        #[arg(short = 'p', long = "config-path")]
        config_path: Option<std::path::PathBuf>,

        /// Force overwrite if file exists
        #[arg(short = 'f', long)]
        force: bool,
    },
}

impl ConfigCommand {
    pub async fn execute(&self, output_format: OutputFormat) -> Result<()> {
        match &self.command {
            ConfigSubcommand::Validate {
                file,
                show_suggestions,
            } => validate_command(file.as_deref(), *show_suggestions, output_format).await,
            ConfigSubcommand::AutoFix { file, dry_run } => {
                auto_fix_command(file.as_deref(), *dry_run).await
            }
            ConfigSubcommand::Migrate {
                from,
                to,
                from_version,
            } => migrate_command(from, to.as_deref(), from_version).await,
            ConfigSubcommand::Show => show_command().await,
            ConfigSubcommand::Init { config_path, force } => {
                init_command(config_path.as_deref(), *force).await
            }
        }
    }
}

async fn validate_command(
    file: Option<&std::path::Path>,
    show_suggestions: bool,
    format: OutputFormat,
) -> Result<()> {
    let validator = if let Some(path) = file {
        ConfigValidator::from_file(path)?
    } else {
        let config = Config::load()?;
        ConfigValidator::new(config)
    };

    let result = validator.validate();

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&result)?);
        }
        OutputFormat::Quiet => {
            if result.is_valid {
                println!("valid");
            } else {
                println!("invalid");
                std::process::exit(1);
            }
        }
        OutputFormat::Table => {
            if result.is_valid {
                println!("✓ Configuration is valid");
            } else {
                println!("✗ Configuration has errors");
            }
            println!();

            // Show errors
            if !result.errors.is_empty() {
                println!("Errors:");
                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .set_content_arrangement(ContentArrangement::Dynamic)
                    .set_header(vec!["Field", "Severity", "Message"]);

                for error in &result.errors {
                    let severity_cell = match error.severity {
                        ErrorSeverity::Critical => Cell::new("Critical").fg(Color::Red),
                        ErrorSeverity::Error => Cell::new("Error").fg(Color::Yellow),
                    };

                    table.add_row(vec![
                        Cell::new(&error.field),
                        severity_cell,
                        Cell::new(&error.message),
                    ]);
                }

                println!("{}", table);
                println!();
            }

            // Show warnings
            if !result.warnings.is_empty() {
                println!("Warnings:");
                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .set_content_arrangement(ContentArrangement::Dynamic)
                    .set_header(vec!["Field", "Message"]);

                for warning in &result.warnings {
                    table.add_row(vec![Cell::new(&warning.field), Cell::new(&warning.message)]);
                }

                println!("{}", table);
                println!();
            }

            // Show suggestions
            if (show_suggestions || !result.is_valid) && !result.suggestions.is_empty() {
                println!("Suggestions:");
                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .set_content_arrangement(ContentArrangement::Dynamic)
                    .set_header(vec!["Field", "Suggested Value", "Reason"]);

                for suggestion in &result.suggestions {
                    table.add_row(vec![
                        Cell::new(&suggestion.field),
                        Cell::new(&suggestion.suggested_value),
                        Cell::new(&suggestion.reason),
                    ]);
                }

                println!("{}", table);
            }

            if !result.is_valid {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

async fn auto_fix_command(file: Option<&std::path::Path>, dry_run: bool) -> Result<()> {
    let path = if let Some(p) = file {
        p.to_path_buf()
    } else {
        Config::default_path()
    };

    let mut validator = ConfigValidator::from_file(&path)?;
    let fixes = validator.auto_fix();

    if fixes.is_empty() {
        println!("✓ No issues to fix");
        return Ok(());
    }

    println!("Applied {} fixes:", fixes.len());
    for fix in &fixes {
        println!("  • {}", fix);
    }

    if dry_run {
        println!("\nDry run mode - no changes were saved");
    } else {
        validator.save(&path)?;
        println!("\n✓ Configuration saved to {}", path.display());
    }

    Ok(())
}

async fn migrate_command(
    from: &std::path::Path,
    to: Option<&std::path::Path>,
    from_version: &str,
) -> Result<()> {
    let mut config = Config::load_from_path(from)?;
    let changes = ConfigMigrator::migrate(from_version, &mut config)?;

    println!("Migration completed:");
    for change in &changes {
        println!("  • {}", change);
    }

    let target_path = to.unwrap_or(from);
    config.save_to_path(target_path)?;

    println!(
        "\n✓ Migrated configuration saved to {}",
        target_path.display()
    );

    Ok(())
}

async fn show_command() -> Result<()> {
    let path = Config::default_path();
    println!("Configuration file: {}", path.display());
    println!("Exists: {}", path.exists());

    if path.exists() {
        let metadata = std::fs::metadata(&path)?;
        println!("Size: {} bytes", metadata.len());
        println!("Modified: {:?}", metadata.modified()?);
    }

    Ok(())
}

async fn init_command(output: Option<&std::path::Path>, force: bool) -> Result<()> {
    let path = if let Some(p) = output {
        p.to_path_buf()
    } else {
        Config::default_path()
    };

    if path.exists() && !force {
        anyhow::bail!(
            "Configuration file already exists at {}. Use --force to overwrite",
            path.display()
        );
    }

    let config = Config::default();
    config.save_to_path(&path)?;

    println!("✓ Created configuration file at {}", path.display());

    Ok(())
}

/// Handle config command
pub async fn handle_config_command(command: ConfigCommand, format: OutputFormat) -> Result<()> {
    command.execute(format).await
}
