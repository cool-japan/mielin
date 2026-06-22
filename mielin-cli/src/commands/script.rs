//! Script management commands

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use crate::script::{ScriptContext, ScriptEngine};
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum ScriptCommands {
    /// List all installed scripts
    #[command(visible_aliases = &["ls"])]
    List {
        /// Filter by tag
        #[arg(short, long)]
        tag: Option<String>,
    },

    /// Show script information
    #[command(visible_aliases = &["show", "details"])]
    Info {
        /// Script name
        name: String,
    },

    /// Execute a script
    #[command(visible_aliases = &["exec", "run"])]
    Execute {
        /// Script name
        name: String,

        /// Arguments as KEY=VALUE pairs
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Install a script from a file
    #[command(visible_aliases = &["add"])]
    Install {
        /// Path to script file (.rhai)
        path: PathBuf,
    },

    /// Uninstall a script
    #[command(visible_aliases = &["remove", "rm"])]
    Uninstall {
        /// Script name
        name: String,

        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Create a new script template
    #[command(visible_aliases = &["new"])]
    Create {
        /// Script name
        name: String,

        /// Output file path
        #[arg(short = 'f', long = "file")]
        file: PathBuf,
    },

    /// Reload all scripts
    Reload,

    /// Show scripts directory path
    #[command(visible_aliases = &["dir"])]
    Directory,

    /// Edit a script (opens in $EDITOR)
    Edit {
        /// Script name
        name: String,
    },
}

pub async fn handle_script_command(cmd: ScriptCommands, format: OutputFormat) -> Result<()> {
    match cmd {
        ScriptCommands::List { tag } => list_scripts(tag, format).await,
        ScriptCommands::Info { name } => show_script_info(&name, format).await,
        ScriptCommands::Execute { name, args } => execute_script(&name, args, format).await,
        ScriptCommands::Install { path } => install_script(&path, format).await,
        ScriptCommands::Uninstall { name, yes } => uninstall_script(&name, yes, format).await,
        ScriptCommands::Create { name, file } => create_script_template(&name, &file, format).await,
        ScriptCommands::Reload => reload_scripts(format).await,
        ScriptCommands::Directory => show_scripts_directory(format).await,
        ScriptCommands::Edit { name } => edit_script(&name, format).await,
    }
}

#[derive(Debug, Serialize)]
struct ScriptListEntry {
    name: String,
    version: String,
    description: String,
    author: String,
    tags: String,
}

impl MultiFormatDisplay for Vec<ScriptListEntry> {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Version").fg(Color::Cyan),
            Cell::new("Description").fg(Color::Cyan),
            Cell::new("Author").fg(Color::Cyan),
            Cell::new("Tags").fg(Color::Cyan),
        ]);

        for entry in self {
            table.add_row(vec![
                Cell::new(&entry.name),
                Cell::new(&entry.version),
                Cell::new(&entry.description),
                Cell::new(&entry.author),
                Cell::new(&entry.tags),
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

async fn list_scripts(tag: Option<String>, format: OutputFormat) -> Result<()> {
    let mut engine = ScriptEngine::new()?;
    engine.discover_scripts()?;

    let scripts = engine.list_scripts();
    let mut entries: Vec<ScriptListEntry> = scripts
        .iter()
        .map(|s| ScriptListEntry {
            name: s.metadata.name.clone(),
            version: s.metadata.version.clone(),
            description: s.metadata.description.clone(),
            author: s
                .metadata
                .author
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            tags: s.metadata.tags.join(", "),
        })
        .collect();

    // Filter by tag if specified
    if let Some(tag_filter) = tag {
        entries.retain(|e| {
            e.tags
                .split(", ")
                .any(|t| t.eq_ignore_ascii_case(&tag_filter))
        });
    }

    println!("{}", render_output(&entries, format)?);
    Ok(())
}

#[derive(Debug, Serialize)]
struct ScriptInfoDisplay {
    name: String,
    version: String,
    description: String,
    author: String,
    required_version: String,
    tags: Vec<String>,
    path: String,
    lines: usize,
}

impl MultiFormatDisplay for ScriptInfoDisplay {
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
            Cell::new("Required Version").fg(Color::Cyan),
            Cell::new(&self.required_version),
        ]);
        table.add_row(vec![
            Cell::new("Tags").fg(Color::Cyan),
            Cell::new(self.tags.join(", ")),
        ]);
        table.add_row(vec![
            Cell::new("Path").fg(Color::Cyan),
            Cell::new(&self.path),
        ]);
        table.add_row(vec![
            Cell::new("Lines").fg(Color::Cyan),
            Cell::new(self.lines.to_string()),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        format!("{} v{}", self.name, self.version)
    }
}

async fn show_script_info(name: &str, format: OutputFormat) -> Result<()> {
    let mut engine = ScriptEngine::new()?;
    engine.discover_scripts()?;

    let script = engine
        .get_script(name)
        .ok_or_else(|| anyhow::anyhow!("Script not found: {}", name))?;

    let info = ScriptInfoDisplay {
        name: script.metadata.name.clone(),
        version: script.metadata.version.clone(),
        description: script.metadata.description.clone(),
        author: script
            .metadata
            .author
            .clone()
            .unwrap_or_else(|| "Unknown".to_string()),
        required_version: script
            .metadata
            .required_version
            .clone()
            .unwrap_or_else(|| "None".to_string()),
        tags: script.metadata.tags.clone(),
        path: script.path.to_string_lossy().to_string(),
        lines: script.content.lines().count(),
    };

    println!("{}", render_output(&info, format)?);
    Ok(())
}

#[derive(Debug, Serialize)]
struct ScriptExecutionResult {
    script: String,
    exit_code: i32,
    duration_ms: u64,
    output: String,
}

impl MultiFormatDisplay for ScriptExecutionResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Script").fg(Color::Cyan),
            Cell::new(&self.script),
        ]);
        table.add_row(vec![
            Cell::new("Exit Code").fg(Color::Cyan),
            Cell::new(self.exit_code.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Duration (ms)").fg(Color::Cyan),
            Cell::new(self.duration_ms.to_string()),
        ]);

        if !self.output.is_empty() {
            table.add_row(vec![
                Cell::new("Output").fg(Color::Cyan),
                Cell::new(&self.output),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.output.clone()
    }
}

async fn execute_script(name: &str, args: Vec<String>, format: OutputFormat) -> Result<()> {
    let mut engine = ScriptEngine::new()?;
    engine.discover_scripts()?;

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

    let context = ScriptContext {
        args: arguments,
        env: std::env::vars().collect(),
        working_dir: std::env::current_dir()?.to_string_lossy().to_string(),
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let result = engine.execute_script(name, context)?;

    if result.exit_code != 0 && format != OutputFormat::Quiet {
        if !result.stderr.is_empty() {
            eprintln!("{}", result.stderr);
        }
        anyhow::bail!("Script failed with exit code: {}", result.exit_code);
    }

    let exec_result = ScriptExecutionResult {
        script: name.to_string(),
        exit_code: result.exit_code,
        duration_ms: result.duration_ms,
        output: result.stdout,
    };

    println!("{}", render_output(&exec_result, format)?);
    Ok(())
}

async fn install_script(path: &Path, format: OutputFormat) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Script file not found: {:?}", path);
    }

    if !path.is_file() {
        anyhow::bail!("Path is not a file: {:?}", path);
    }

    if path.extension().and_then(|s| s.to_str()) != Some("rhai") {
        anyhow::bail!("Invalid script file extension. Expected .rhai");
    }

    let mut engine = ScriptEngine::new()?;
    engine.install_script(path)?;

    if format != OutputFormat::Quiet {
        println!("✓ Script installed successfully");
    }

    Ok(())
}

async fn uninstall_script(name: &str, yes: bool, format: OutputFormat) -> Result<()> {
    let mut engine = ScriptEngine::new()?;
    engine.discover_scripts()?;

    // Check if script exists
    if engine.get_script(name).is_none() {
        anyhow::bail!("Script not found: {}", name);
    }

    // Confirm uninstall
    if !yes && format != OutputFormat::Quiet {
        println!(
            "Are you sure you want to uninstall script '{}'? [y/N]",
            name
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Uninstall cancelled");
            return Ok(());
        }
    }

    engine.uninstall_script(name)?;

    if format != OutputFormat::Quiet {
        println!("✓ Script uninstalled successfully");
    }

    Ok(())
}

async fn create_script_template(name: &str, file: &Path, format: OutputFormat) -> Result<()> {
    let engine = ScriptEngine::new()?;
    engine.create_template(name, file)?;

    if format != OutputFormat::Quiet {
        println!("✓ Script template created: {:?}", file);
        println!("  Edit the template and install it with:");
        println!("  mielinctl script install {:?}", file);
    }

    Ok(())
}

async fn reload_scripts(format: OutputFormat) -> Result<()> {
    let mut engine = ScriptEngine::new()?;
    let count = engine.discover_scripts()?;

    if format != OutputFormat::Quiet {
        println!("✓ Reloaded {} script(s)", count);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct ScriptsDirectoryInfo {
    path: String,
    exists: bool,
    script_count: usize,
}

impl MultiFormatDisplay for ScriptsDirectoryInfo {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Scripts Directory").fg(Color::Cyan),
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

        table.add_row(vec![
            Cell::new("Script Count").fg(Color::Cyan),
            Cell::new(self.script_count.to_string()),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        self.path.clone()
    }
}

async fn show_scripts_directory(format: OutputFormat) -> Result<()> {
    let scripts_dir = ScriptEngine::get_scripts_dir()?;

    let script_count = if scripts_dir.exists() {
        std::fs::read_dir(&scripts_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rhai"))
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let info = ScriptsDirectoryInfo {
        path: scripts_dir.to_string_lossy().to_string(),
        exists: scripts_dir.exists(),
        script_count,
    };

    println!("{}", render_output(&info, format)?);
    Ok(())
}

async fn edit_script(name: &str, format: OutputFormat) -> Result<()> {
    let mut engine = ScriptEngine::new()?;
    engine.discover_scripts()?;

    let script = engine
        .get_script(name)
        .ok_or_else(|| anyhow::anyhow!("Script not found: {}", name))?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    if format != OutputFormat::Quiet {
        println!("Opening {} in {}...", script.path.display(), editor);
    }

    let status = std::process::Command::new(&editor)
        .arg(&script.path)
        .status()?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    if format != OutputFormat::Quiet {
        println!("✓ Script edited successfully");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_list_entry() {
        let entry = ScriptListEntry {
            name: "test-script".to_string(),
            version: "1.0.0".to_string(),
            description: "A test script".to_string(),
            author: "Test Author".to_string(),
            tags: "test, demo".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test-script"));
        assert!(json.contains("1.0.0"));
    }

    #[test]
    fn test_script_info_display() {
        let info = ScriptInfoDisplay {
            name: "test-script".to_string(),
            version: "1.0.0".to_string(),
            description: "A test script".to_string(),
            author: "Test Author".to_string(),
            required_version: "0.1.0".to_string(),
            tags: vec!["test".to_string(), "demo".to_string()],
            path: std::env::temp_dir()
                .join("test.rhai")
                .to_string_lossy()
                .into_owned(),
            lines: 42,
        };

        let quiet = info.to_quiet();
        assert_eq!(quiet, "test-script v1.0.0");
    }

    #[test]
    fn test_script_execution_result() {
        let result = ScriptExecutionResult {
            script: "test-script".to_string(),
            exit_code: 0,
            duration_ms: 100,
            output: "Success".to_string(),
        };

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.output, "Success");
    }
}
