//! Remote node management commands

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use crate::remote::{AuthMethod, ConnectionOptions, RemoteCommand, RemoteManager, RemoteNode};
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum RemoteCommands {
    /// List all remote nodes
    #[command(visible_aliases = &["ls"])]
    List {
        /// Filter by tag
        #[arg(short, long)]
        tag: Option<String>,
    },

    /// Add a new remote node
    #[command(visible_aliases = &["create", "new"])]
    Add {
        /// Node ID
        #[arg(short, long)]
        id: String,

        /// Node name
        #[arg(short, long)]
        name: String,

        /// Node address (host:port)
        #[arg(short, long)]
        address: String,

        /// Authentication method: none, `apikey:<KEY>`, `token:<TOKEN>`
        #[arg(long, default_value = "none")]
        auth: String,

        /// Tags (comma-separated)
        #[arg(short, long)]
        tags: Option<String>,

        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Remove a remote node
    #[command(visible_aliases = &["rm", "delete"])]
    Remove {
        /// Node ID
        id: String,

        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Show node information
    #[command(visible_aliases = &["show", "details"])]
    Info {
        /// Node ID
        id: String,
    },

    /// Update a remote node
    Update {
        /// Node ID
        id: String,

        /// New name
        #[arg(long)]
        name: Option<String>,

        /// New address
        #[arg(long)]
        address: Option<String>,

        /// New authentication
        #[arg(long)]
        auth: Option<String>,

        /// New tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// New description
        #[arg(long)]
        description: Option<String>,
    },

    /// Test connection to a remote node
    #[command(visible_aliases = &["ping"])]
    Test {
        /// Node ID
        id: String,
    },

    /// Execute a command on a remote node
    #[command(visible_aliases = &["exec", "run"])]
    Execute {
        /// Node ID or tag (prefix with @tag: for tags)
        target: String,

        /// Command to execute
        command: String,

        /// Command arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Import nodes from a configuration file
    Import {
        /// Path to configuration file
        path: PathBuf,
    },

    /// Export nodes to a configuration file
    Export {
        /// Path to output file
        path: PathBuf,
    },

    /// Show remote nodes configuration file path
    #[command(visible_aliases = &["path"])]
    Config,
}

pub async fn handle_remote_command(cmd: RemoteCommands, format: OutputFormat) -> Result<()> {
    match cmd {
        RemoteCommands::List { tag } => list_remote_nodes(tag, format).await,
        RemoteCommands::Add {
            id,
            name,
            address,
            auth,
            tags,
            description,
        } => add_remote_node(id, name, address, auth, tags, description, format).await,
        RemoteCommands::Remove { id, yes } => remove_remote_node(&id, yes, format).await,
        RemoteCommands::Info { id } => show_remote_node_info(&id, format).await,
        RemoteCommands::Update {
            id,
            name,
            address,
            auth,
            tags,
            description,
        } => update_remote_node(&id, name, address, auth, tags, description, format).await,
        RemoteCommands::Test { id } => test_remote_connection(&id, format).await,
        RemoteCommands::Execute {
            target,
            command,
            args,
        } => execute_remote_command(&target, &command, args, format).await,
        RemoteCommands::Import { path } => import_remote_nodes(&path, format).await,
        RemoteCommands::Export { path } => export_remote_nodes(&path, format).await,
        RemoteCommands::Config => show_config_path(format).await,
    }
}

#[derive(Debug, Serialize)]
struct RemoteNodeListEntry {
    id: String,
    name: String,
    address: String,
    auth_type: String,
    tags: String,
}

impl MultiFormatDisplay for Vec<RemoteNodeListEntry> {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Address").fg(Color::Cyan),
            Cell::new("Auth").fg(Color::Cyan),
            Cell::new("Tags").fg(Color::Cyan),
        ]);

        for entry in self {
            table.add_row(vec![
                Cell::new(&entry.id),
                Cell::new(&entry.name),
                Cell::new(&entry.address),
                Cell::new(&entry.auth_type),
                Cell::new(&entry.tags),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

async fn list_remote_nodes(tag: Option<String>, format: OutputFormat) -> Result<()> {
    let manager = RemoteManager::new()?;

    let nodes = if let Some(tag_filter) = tag {
        manager.list_nodes_by_tag(&tag_filter)
    } else {
        manager.list_nodes()
    };

    let entries: Vec<RemoteNodeListEntry> = nodes
        .iter()
        .map(|n| RemoteNodeListEntry {
            id: n.id.clone(),
            name: n.name.clone(),
            address: n.address.clone(),
            auth_type: match &n.auth {
                AuthMethod::None => "None".to_string(),
                AuthMethod::ApiKey { .. } => "API Key".to_string(),
                AuthMethod::Certificate { .. } => "Certificate".to_string(),
                AuthMethod::Token { .. } => "Token".to_string(),
            },
            tags: n.tags.join(", "),
        })
        .collect();

    println!("{}", render_output(&entries, format)?);
    Ok(())
}

async fn add_remote_node(
    id: String,
    name: String,
    address: String,
    auth_str: String,
    tags: Option<String>,
    description: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut manager = RemoteManager::new()?;

    // Parse authentication method
    let auth = parse_auth_method(&auth_str)?;

    // Parse tags
    let tags_vec = tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let node = RemoteNode {
        id: id.clone(),
        name,
        address,
        auth,
        options: ConnectionOptions::default(),
        tags: tags_vec,
        description: description.unwrap_or_default(),
    };

    manager.add_node(node)?;

    if format != OutputFormat::Quiet {
        println!("✓ Remote node added: {}", id);
    }

    Ok(())
}

async fn remove_remote_node(id: &str, yes: bool, format: OutputFormat) -> Result<()> {
    let mut manager = RemoteManager::new()?;

    // Check if node exists
    if manager.get_node(id).is_none() {
        anyhow::bail!("Remote node not found: {}", id);
    }

    // Confirm removal
    if !yes && format != OutputFormat::Quiet {
        println!(
            "Are you sure you want to remove remote node '{}'? [y/N]",
            id
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Removal cancelled");
            return Ok(());
        }
    }

    manager.remove_node(id)?;

    if format != OutputFormat::Quiet {
        println!("✓ Remote node removed: {}", id);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct RemoteNodeInfo {
    id: String,
    name: String,
    address: String,
    auth_type: String,
    tls_enabled: bool,
    verify_ssl: bool,
    timeout_secs: u64,
    max_retries: u32,
    tags: Vec<String>,
    description: String,
}

impl MultiFormatDisplay for RemoteNodeInfo {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![Cell::new("ID").fg(Color::Cyan), Cell::new(&self.id)]);
        table.add_row(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new(&self.name),
        ]);
        table.add_row(vec![
            Cell::new("Address").fg(Color::Cyan),
            Cell::new(&self.address),
        ]);
        table.add_row(vec![
            Cell::new("Auth Type").fg(Color::Cyan),
            Cell::new(&self.auth_type),
        ]);
        table.add_row(vec![
            Cell::new("TLS Enabled").fg(Color::Cyan),
            Cell::new(self.tls_enabled.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Verify SSL").fg(Color::Cyan),
            Cell::new(self.verify_ssl.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Timeout (secs)").fg(Color::Cyan),
            Cell::new(self.timeout_secs.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Max Retries").fg(Color::Cyan),
            Cell::new(self.max_retries.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Tags").fg(Color::Cyan),
            Cell::new(self.tags.join(", ")),
        ]);
        table.add_row(vec![
            Cell::new("Description").fg(Color::Cyan),
            Cell::new(&self.description),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        format!("{} - {}", self.id, self.name)
    }
}

async fn show_remote_node_info(id: &str, format: OutputFormat) -> Result<()> {
    let manager = RemoteManager::new()?;

    let node = manager
        .get_node(id)
        .ok_or_else(|| anyhow::anyhow!("Remote node not found: {}", id))?;

    let info = RemoteNodeInfo {
        id: node.id.clone(),
        name: node.name.clone(),
        address: node.address.clone(),
        auth_type: match &node.auth {
            AuthMethod::None => "None".to_string(),
            AuthMethod::ApiKey { .. } => "API Key".to_string(),
            AuthMethod::Certificate { .. } => "Certificate".to_string(),
            AuthMethod::Token { .. } => "Token".to_string(),
        },
        tls_enabled: node.options.tls,
        verify_ssl: node.options.verify_ssl,
        timeout_secs: node.options.timeout_secs,
        max_retries: node.options.max_retries,
        tags: node.tags.clone(),
        description: node.description.clone(),
    };

    println!("{}", render_output(&info, format)?);
    Ok(())
}

async fn update_remote_node(
    id: &str,
    name: Option<String>,
    address: Option<String>,
    auth: Option<String>,
    tags: Option<String>,
    description: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut manager = RemoteManager::new()?;

    let mut node = manager
        .get_node(id)
        .ok_or_else(|| anyhow::anyhow!("Remote node not found: {}", id))?
        .clone();

    if let Some(new_name) = name {
        node.name = new_name;
    }

    if let Some(new_address) = address {
        node.address = new_address;
    }

    if let Some(auth_str) = auth {
        node.auth = parse_auth_method(&auth_str)?;
    }

    if let Some(tags_str) = tags {
        node.tags = tags_str.split(',').map(|s| s.trim().to_string()).collect();
    }

    if let Some(new_desc) = description {
        node.description = new_desc;
    }

    manager.update_node(id, node)?;

    if format != OutputFormat::Quiet {
        println!("✓ Remote node updated: {}", id);
    }

    Ok(())
}

async fn test_remote_connection(id: &str, format: OutputFormat) -> Result<()> {
    let manager = RemoteManager::new()?;

    let connected = manager.test_connection(id).await?;

    if connected {
        if format != OutputFormat::Quiet {
            println!("✓ Connection successful to node: {}", id);
        }
    } else {
        anyhow::bail!("Connection failed to node: {}", id);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct RemoteExecutionResult {
    node_id: String,
    command: String,
    exit_code: i32,
    duration_ms: u64,
    output: String,
}

impl MultiFormatDisplay for Vec<RemoteExecutionResult> {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("Node").fg(Color::Cyan),
            Cell::new("Command").fg(Color::Cyan),
            Cell::new("Exit Code").fg(Color::Cyan),
            Cell::new("Duration (ms)").fg(Color::Cyan),
            Cell::new("Output").fg(Color::Cyan),
        ]);

        for result in self {
            let exit_code_cell = if result.exit_code == 0 {
                Cell::new(result.exit_code.to_string()).fg(Color::Green)
            } else {
                Cell::new(result.exit_code.to_string()).fg(Color::Red)
            };

            table.add_row(vec![
                Cell::new(&result.node_id),
                Cell::new(&result.command),
                exit_code_cell,
                Cell::new(result.duration_ms.to_string()),
                Cell::new(&result.output),
            ]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        self.iter()
            .map(|r| r.output.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

async fn execute_remote_command(
    target: &str,
    command: &str,
    args: Vec<String>,
    format: OutputFormat,
) -> Result<()> {
    let manager = RemoteManager::new()?;

    let remote_cmd = RemoteCommand {
        command: command.to_string(),
        args,
        env: HashMap::new(),
    };

    let results = if target.starts_with("@tag:") {
        // Execute on all nodes with the specified tag
        let tag = target.trim_start_matches("@tag:");
        let nodes = manager.list_nodes_by_tag(tag);
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        manager.execute_on_multiple(&node_ids, remote_cmd).await?
    } else {
        // Execute on a single node
        let result = manager.execute_command(target, remote_cmd).await?;
        vec![result]
    };

    let exec_results: Vec<RemoteExecutionResult> = results
        .iter()
        .map(|r| RemoteExecutionResult {
            node_id: r.node_id.clone(),
            command: command.to_string(),
            exit_code: r.exit_code,
            duration_ms: r.duration_ms,
            output: r.stdout.clone(),
        })
        .collect();

    println!("{}", render_output(&exec_results, format)?);
    Ok(())
}

async fn import_remote_nodes(path: &Path, format: OutputFormat) -> Result<()> {
    let mut manager = RemoteManager::new()?;
    let count = manager.import_nodes(path)?;

    if format != OutputFormat::Quiet {
        println!("✓ Imported {} remote node(s)", count);
    }

    Ok(())
}

async fn export_remote_nodes(path: &Path, format: OutputFormat) -> Result<()> {
    let manager = RemoteManager::new()?;
    manager.export_nodes(path)?;

    if format != OutputFormat::Quiet {
        println!("✓ Exported remote nodes to {:?}", path);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct ConfigPathInfo {
    path: String,
    exists: bool,
}

impl MultiFormatDisplay for ConfigPathInfo {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Config Path").fg(Color::Cyan),
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

async fn show_config_path(format: OutputFormat) -> Result<()> {
    let config_path = RemoteManager::get_config_path()?;

    let info = ConfigPathInfo {
        path: config_path.to_string_lossy().to_string(),
        exists: config_path.exists(),
    };

    println!("{}", render_output(&info, format)?);
    Ok(())
}

fn parse_auth_method(auth_str: &str) -> Result<AuthMethod> {
    let parts: Vec<&str> = auth_str.splitn(2, ':').collect();

    match parts[0].to_lowercase().as_str() {
        "none" => Ok(AuthMethod::None),
        "apikey" => {
            if parts.len() != 2 {
                anyhow::bail!("API key auth requires format: apikey:<KEY>");
            }
            Ok(AuthMethod::ApiKey {
                key: parts[1].to_string(),
            })
        }
        "token" => {
            if parts.len() != 2 {
                anyhow::bail!("Token auth requires format: token:<TOKEN>");
            }
            Ok(AuthMethod::Token {
                token: parts[1].to_string(),
            })
        }
        _ => anyhow::bail!(
            "Unsupported auth method: {}. Use: none, apikey:<KEY>, or token:<TOKEN>",
            parts[0]
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_auth_method_none() {
        let auth = parse_auth_method("none").unwrap();
        matches!(auth, AuthMethod::None);
    }

    #[test]
    fn test_parse_auth_method_apikey() {
        let auth = parse_auth_method("apikey:test-key").unwrap();
        matches!(auth, AuthMethod::ApiKey { .. });
    }

    #[test]
    fn test_parse_auth_method_token() {
        let auth = parse_auth_method("token:test-token").unwrap();
        matches!(auth, AuthMethod::Token { .. });
    }

    #[test]
    fn test_parse_auth_method_invalid() {
        let result = parse_auth_method("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_remote_node_list_entry() {
        let entry = RemoteNodeListEntry {
            id: "node1".to_string(),
            name: "Test Node".to_string(),
            address: "localhost:8080".to_string(),
            auth_type: "API Key".to_string(),
            tags: "prod, web".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("node1"));
        assert!(json.contains("Test Node"));
    }
}
