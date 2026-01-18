//! Plugin management commands

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use crate::plugin::PluginManager;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum PluginCommands {
    /// List all installed plugins
    #[command(visible_aliases = &["ls"])]
    List,

    /// Show plugin information
    #[command(visible_aliases = &["show", "details"])]
    Info {
        /// Plugin name
        name: String,
    },

    /// Install a plugin from a directory
    #[command(visible_aliases = &["add"])]
    Install {
        /// Path to plugin directory
        path: PathBuf,
    },

    /// Uninstall a plugin
    #[command(visible_aliases = &["remove", "rm"])]
    Uninstall {
        /// Plugin name
        name: String,

        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Enable a plugin
    Enable {
        /// Plugin name
        name: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin name
        name: String,
    },

    /// Execute a plugin command
    #[command(visible_aliases = &["run", "exec"])]
    Execute {
        /// Plugin name
        plugin: String,

        /// Command name
        command: String,

        /// Arguments as KEY=VALUE pairs
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Reload all plugins
    Reload,

    /// Show plugin directory path
    #[command(visible_aliases = &["dir"])]
    Directory,
}

pub async fn handle_plugin_command(cmd: PluginCommands, format: OutputFormat) -> Result<()> {
    match cmd {
        PluginCommands::List => list_plugins(format).await,
        PluginCommands::Info { name } => show_plugin_info(&name, format).await,
        PluginCommands::Install { path } => install_plugin(&path, format).await,
        PluginCommands::Uninstall { name, yes } => uninstall_plugin(&name, yes, format).await,
        PluginCommands::Enable { name } => enable_plugin(&name, format).await,
        PluginCommands::Disable { name } => disable_plugin(&name, format).await,
        PluginCommands::Execute {
            plugin,
            command,
            args,
        } => execute_plugin_command(&plugin, &command, args, format).await,
        PluginCommands::Reload => reload_plugins(format).await,
        PluginCommands::Directory => show_plugin_directory(format).await,
    }
}

#[derive(Debug, Serialize)]
struct PluginListEntry {
    name: String,
    version: String,
    description: String,
    enabled: String,
    commands: usize,
}

impl MultiFormatDisplay for Vec<PluginListEntry> {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Version").fg(Color::Cyan),
            Cell::new("Description").fg(Color::Cyan),
            Cell::new("Enabled").fg(Color::Cyan),
            Cell::new("Commands").fg(Color::Cyan),
        ]);

        for entry in self {
            let enabled_cell = if entry.enabled == "Yes" {
                Cell::new(&entry.enabled).fg(Color::Green)
            } else {
                Cell::new(&entry.enabled).fg(Color::Red)
            };

            table.add_row(vec![
                Cell::new(&entry.name),
                Cell::new(&entry.version),
                Cell::new(&entry.description),
                enabled_cell,
                Cell::new(entry.commands.to_string()),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

async fn list_plugins(format: OutputFormat) -> Result<()> {
    let mut manager = PluginManager::new()?;
    manager.discover_plugins()?;

    let plugins = manager.list_plugins();
    let entries: Vec<PluginListEntry> = plugins
        .iter()
        .map(|p| PluginListEntry {
            name: p.metadata.name.clone(),
            version: p.metadata.version.clone(),
            description: p.metadata.description.clone(),
            enabled: if p.enabled { "Yes" } else { "No" }.to_string(),
            commands: p.metadata.commands.len(),
        })
        .collect();

    println!("{}", render_output(&entries, format)?);
    Ok(())
}

#[derive(Debug, Serialize)]
struct PluginInfoDisplay {
    name: String,
    version: String,
    description: String,
    author: String,
    license: String,
    enabled: String,
    commands: Vec<CommandInfo>,
    dependencies: Vec<String>,
    min_version: String,
}

#[derive(Debug, Serialize)]
struct CommandInfo {
    name: String,
    description: String,
    aliases: String,
    arguments: usize,
}

impl MultiFormatDisplay for PluginInfoDisplay {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new(&self.name),
        ]);
        table.add_row(vec![
            Cell::new("Version").fg(Color::Cyan),
            Cell::new(&self.version),
        ]);
        table.add_row(vec![
            Cell::new("Description").fg(Color::Cyan),
            Cell::new(&self.description),
        ]);
        table.add_row(vec![
            Cell::new("Author").fg(Color::Cyan),
            Cell::new(&self.author),
        ]);
        table.add_row(vec![
            Cell::new("License").fg(Color::Cyan),
            Cell::new(&self.license),
        ]);
        table.add_row(vec![
            Cell::new("Enabled").fg(Color::Cyan),
            Cell::new(&self.enabled),
        ]);
        table.add_row(vec![
            Cell::new("Commands").fg(Color::Cyan),
            Cell::new(self.commands.len().to_string()),
        ]);

        if !self.commands.is_empty() {
            table.add_row(vec![Cell::new("").fg(Color::Cyan), Cell::new("")]);
            table.add_row(vec![
                Cell::new("Available Commands")
                    .fg(Color::Yellow)
                    .add_attribute(comfy_table::Attribute::Bold),
                Cell::new(""),
            ]);
            for cmd in &self.commands {
                table.add_row(vec![
                    Cell::new(format!("  {}", cmd.name)),
                    Cell::new(&cmd.description),
                ]);
            }
        }

        table
    }

    fn to_quiet(&self) -> String {
        format!("{} v{}", self.name, self.version)
    }
}

async fn show_plugin_info(name: &str, format: OutputFormat) -> Result<()> {
    let mut manager = PluginManager::new()?;
    manager.discover_plugins()?;

    let plugin = manager
        .get_plugin(name)
        .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", name))?;

    let info = PluginInfoDisplay {
        name: plugin.metadata.name.clone(),
        version: plugin.metadata.version.clone(),
        description: plugin.metadata.description.clone(),
        author: plugin.metadata.author.clone(),
        license: plugin.metadata.license.clone(),
        enabled: if plugin.enabled { "Yes" } else { "No" }.to_string(),
        commands: plugin
            .metadata
            .commands
            .iter()
            .map(|c| CommandInfo {
                name: c.name.clone(),
                description: c.description.clone(),
                aliases: c.aliases.join(", "),
                arguments: c.arguments.len(),
            })
            .collect(),
        dependencies: plugin.metadata.dependencies.clone(),
        min_version: plugin
            .metadata
            .min_version
            .clone()
            .unwrap_or_else(|| "None".to_string()),
    };

    println!("{}", render_output(&info, format)?);
    Ok(())
}

async fn install_plugin(path: &Path, format: OutputFormat) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Plugin directory not found: {:?}", path);
    }

    if !path.is_dir() {
        anyhow::bail!("Path is not a directory: {:?}", path);
    }

    let mut manager = PluginManager::new()?;
    manager.install_plugin(path)?;

    if format != OutputFormat::Quiet {
        println!("✓ Plugin installed successfully");
    }

    Ok(())
}

async fn uninstall_plugin(name: &str, yes: bool, format: OutputFormat) -> Result<()> {
    let mut manager = PluginManager::new()?;
    manager.discover_plugins()?;

    // Check if plugin exists
    if manager.get_plugin(name).is_none() {
        anyhow::bail!("Plugin not found: {}", name);
    }

    // Confirm uninstall
    if !yes && format != OutputFormat::Quiet {
        println!(
            "Are you sure you want to uninstall plugin '{}'? [y/N]",
            name
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Uninstall cancelled");
            return Ok(());
        }
    }

    manager.uninstall_plugin(name)?;

    if format != OutputFormat::Quiet {
        println!("✓ Plugin uninstalled successfully");
    }

    Ok(())
}

async fn enable_plugin(name: &str, format: OutputFormat) -> Result<()> {
    let mut manager = PluginManager::new()?;
    manager.discover_plugins()?;

    manager.enable_plugin(name)?;

    if format != OutputFormat::Quiet {
        println!("✓ Plugin enabled: {}", name);
    }

    Ok(())
}

async fn disable_plugin(name: &str, format: OutputFormat) -> Result<()> {
    let mut manager = PluginManager::new()?;
    manager.discover_plugins()?;

    manager.disable_plugin(name)?;

    if format != OutputFormat::Quiet {
        println!("✓ Plugin disabled: {}", name);
    }

    Ok(())
}

async fn execute_plugin_command(
    plugin: &str,
    command: &str,
    args: Vec<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut manager = PluginManager::new()?;
    manager.discover_plugins()?;

    // Parse arguments (KEY=VALUE format)
    let mut arguments = HashMap::new();
    for arg in args {
        let parts: Vec<&str> = arg.splitn(2, '=').collect();
        if parts.len() == 2 {
            arguments.insert(parts[0].to_string(), parts[1].to_string());
        } else {
            anyhow::bail!("Invalid argument format: {}. Expected KEY=VALUE", arg);
        }
    }

    let result = manager.execute_command(plugin, command, arguments).await?;

    if result.exit_code != 0 {
        if !result.stderr.is_empty() && format != OutputFormat::Quiet {
            eprintln!("{}", result.stderr);
        }
        anyhow::bail!("Plugin command failed with exit code: {}", result.exit_code);
    }

    if !result.stdout.is_empty() && format != OutputFormat::Quiet {
        println!("{}", result.stdout);
    }

    if let Some(data) = result.data {
        if format == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(&data)?);
        } else if format == OutputFormat::Yaml {
            println!("{}", serde_yaml::to_string(&data)?);
        }
    }

    Ok(())
}

async fn reload_plugins(format: OutputFormat) -> Result<()> {
    let mut manager = PluginManager::new()?;
    let count = manager.discover_plugins()?;

    if format != OutputFormat::Quiet {
        println!("✓ Reloaded {} plugin(s)", count);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct PluginDirectoryInfo {
    path: String,
    exists: bool,
}

impl MultiFormatDisplay for PluginDirectoryInfo {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Plugin Directory").fg(Color::Cyan),
            Cell::new(&self.path),
        ]);

        table.add_row(vec![
            Cell::new("Exists").fg(Color::Cyan),
            if self.exists {
                Cell::new("Yes").fg(Color::Green)
            } else {
                Cell::new("No").fg(Color::Red)
            },
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        self.path.clone()
    }
}

async fn show_plugin_directory(format: OutputFormat) -> Result<()> {
    let plugin_dir = PluginManager::get_plugin_dir()?;

    let info = PluginDirectoryInfo {
        path: plugin_dir.to_string_lossy().to_string(),
        exists: plugin_dir.exists(),
    };

    println!("{}", render_output(&info, format)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_list_entry_serialization() {
        let entry = PluginListEntry {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            enabled: "Yes".to_string(),
            commands: 2,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test-plugin"));
        assert!(json.contains("1.0.0"));
    }

    #[test]
    fn test_plugin_info_display() {
        let info = PluginInfoDisplay {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            enabled: "Yes".to_string(),
            commands: vec![],
            dependencies: vec![],
            min_version: "0.1.0".to_string(),
        };

        let quiet = info.to_quiet();
        assert_eq!(quiet, "test-plugin v1.0.0");
    }

    #[test]
    fn test_plugin_directory_info() {
        let info = PluginDirectoryInfo {
            path: "/tmp/plugins".to_string(),
            exists: false,
        };

        let quiet = info.to_quiet();
        assert_eq!(quiet, "/tmp/plugins");
    }
}
