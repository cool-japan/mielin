//! CLI definition and command dispatch for MielinCTL

use crate::commands::*;
use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use std::io;
use std::net::SocketAddr;

/// MielinOS Control Interface
#[derive(Parser)]
#[command(name = "mielinctl")]
#[command(about = "MielinOS Control Interface", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Output format
    #[arg(short, long, value_enum, default_value = "table", global = true)]
    pub output: OutputFormat,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Node management commands
    Node {
        #[command(subcommand)]
        action: node::NodeCommands,
    },
    /// Agent management commands
    Agent {
        #[command(subcommand)]
        action: agent::AgentCommands,
    },
    /// Mesh network commands
    Mesh {
        #[command(subcommand)]
        action: mesh::MeshCommands,
    },
    /// Cluster management commands
    Cluster {
        #[command(subcommand)]
        action: cluster::ClusterCommands,
    },
    /// Migration commands
    Migrate {
        #[command(subcommand)]
        action: migrate::MigrateCommands,
    },
    /// Registry operations
    Registry {
        #[command(subcommand)]
        action: registry::RegistryCommands,
    },
    /// Gossip protocol operations
    Gossip {
        #[command(subcommand)]
        action: gossip::GossipCommands,
    },
    /// WASM development tools
    Wasm {
        #[command(subcommand)]
        action: wasm::WasmCommands,
    },
    /// Debugging tools
    Debug {
        #[command(subcommand)]
        action: debug::DebugCommands,
    },
    /// Audit log management
    #[command(visible_aliases = &["log", "logs"])]
    Audit(audit::AuditCommand),
    /// Configuration management
    #[command(visible_aliases = &["cfg"])]
    Config(config::ConfigCommand),
    /// Command history management
    #[command(visible_aliases = &["hist"])]
    History(history::HistoryCommand),
    /// Monitoring and observability commands
    #[command(visible_aliases = &["mon", "observe"])]
    Monitor(monitor::MonitorCommand),
    /// Plugin management
    #[command(visible_aliases = &["plugins", "ext"])]
    Plugin {
        #[command(subcommand)]
        action: plugin::PluginCommands,
    },
    /// Script management
    #[command(visible_aliases = &["scripts"])]
    Script {
        #[command(subcommand)]
        action: script::ScriptCommands,
    },
    /// Remote node management
    #[command(visible_aliases = &["nodes"])]
    Remote {
        #[command(subcommand)]
        action: remote::RemoteCommands,
    },
    /// Start the MielinOS daemon
    Daemon {
        /// Listen address for the mesh service
        #[arg(short, long, default_value = "0.0.0.0:8080")]
        listen: String,
        /// Node role (core, relay, edge)
        #[arg(short, long, default_value = "edge")]
        role: String,
        /// Bootstrap node address to connect to
        #[arg(short, long)]
        bootstrap: Option<String>,
        /// Listen address for the HTTP control-plane API
        #[arg(long, default_value = "127.0.0.1:8081")]
        control_listen: SocketAddr,
    },
    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completion for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Show version and build information
    Version,
    /// Start interactive mode (REPL)
    #[command(visible_aliases = &["repl", "shell"])]
    Interactive,
}

fn handle_version_command(format: OutputFormat) {
    use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
    use serde::Serialize;

    #[derive(Debug, Serialize)]
    struct VersionInfo {
        name: String,
        version: String,
        git_commit: String,
        build_date: String,
        rust_version: String,
        target: String,
    }

    impl MultiFormatDisplay for VersionInfo {
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
                Cell::new("Rust").fg(Color::Cyan),
                Cell::new(&self.rust_version),
            ]);
            table.add_row(vec![
                Cell::new("Target").fg(Color::Cyan),
                Cell::new(&self.target),
            ]);
            table
        }

        fn to_quiet(&self) -> String {
            self.version.clone()
        }
    }

    let info = VersionInfo {
        name: "MielinOS CLI".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit: option_env!("GIT_COMMIT").unwrap_or("unknown").to_string(),
        build_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        rust_version: {
            let rv = env!("CARGO_PKG_RUST_VERSION");
            if rv.is_empty() {
                "unknown".to_string()
            } else {
                rv.to_string()
            }
        },
        target: std::env::consts::ARCH.to_string(),
    };

    println!("{}", render_output(&info, format).unwrap_or_default());
}

fn handle_completion_command(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());
}

/// Dispatch a parsed command to the appropriate handler
pub fn dispatch(
    command: Commands,
    format: OutputFormat,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>> {
    Box::pin(async move {
        match command {
            Commands::Node { action } => handle_node_command(action, format).await,
            Commands::Agent { action } => handle_agent_command(action, format).await,
            Commands::Mesh { action } => handle_mesh_command(action, format).await,
            Commands::Cluster { action } => handle_cluster_command(action, format).await,
            Commands::Migrate { action } => handle_migrate_command(action, format).await,
            Commands::Registry { action } => handle_registry_command(action, format).await,
            Commands::Gossip { action } => handle_gossip_command(action, format).await,
            Commands::Wasm { action } => handle_wasm_command(action, format).await,
            Commands::Debug { action } => handle_debug_command(action, format).await,
            Commands::Audit(cmd) => handle_audit_command(cmd, format).await,
            Commands::Config(cmd) => handle_config_command(cmd, format).await,
            Commands::History(cmd) => cmd.execute(format).await,
            Commands::Monitor(cmd) => handle_monitor_command(cmd, format).await,
            Commands::Plugin { action } => handle_plugin_command(action, format).await,
            Commands::Script { action } => handle_script_command(action, format).await,
            Commands::Remote { action } => handle_remote_command(action, format).await,
            Commands::Daemon {
                listen,
                role,
                bootstrap,
                control_listen,
            } => handle_daemon_command(listen, role, bootstrap, control_listen, format).await,
            Commands::Completion { shell } => {
                handle_completion_command(shell);
                Ok(())
            }
            Commands::Version => {
                handle_version_command(format);
                Ok(())
            }
            Commands::Interactive => {
                let mut repl = crate::Repl::new()?;
                repl.run().await
            }
        }
    })
}
