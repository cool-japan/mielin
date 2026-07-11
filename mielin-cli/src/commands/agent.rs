//! Agent command handlers

use crate::output::{render_output, OutputFormat};
use crate::types::{AgentInfo, AgentList, OperationResult};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum AgentCommands {
    /// List all agents
    #[command(alias = "ls")]
    List {
        /// Filter by state
        #[arg(short, long)]
        state: Option<String>,
        /// Filter by node
        #[arg(short, long)]
        node: Option<String>,
    },
    /// Deploy a new WASM agent
    #[command(alias = "dep")]
    Deploy {
        /// Path to WASM file
        wasm_path: String,
        /// Target node (optional, auto-selects if not specified)
        #[arg(short, long)]
        node: Option<String>,
    },
    /// Migrate an agent to another node
    #[command(aliases = ["mv", "move"])]
    Migrate {
        /// Agent ID
        agent_id: String,
        /// Target node
        target_node: String,
    },
    /// Stop an agent
    #[command(aliases = ["kill", "terminate"])]
    Stop {
        /// Agent ID
        agent_id: String,
    },
    /// Show agent details
    #[command(aliases = ["show", "details"])]
    Inspect {
        /// Agent ID
        agent_id: String,
    },
    /// View agent logs
    #[command(alias = "log")]
    Logs {
        /// Agent ID
        agent_id: String,
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show
        #[arg(short = 'n', long, default_value = "100")]
        lines: usize,
    },
    /// Create a new agent from WASM module
    #[command(aliases = ["new", "add"])]
    Create {
        /// Path to WASM file
        wasm_path: String,
        /// Agent name
        #[arg(short, long)]
        name: String,
        /// Target node (optional, auto-selects if not specified)
        #[arg(short = 't', long)]
        node: Option<String>,
        /// Environment variables (KEY=VALUE)
        #[arg(short, long)]
        env: Vec<String>,
        /// Memory limit in MB
        #[arg(short, long, default_value = "256")]
        memory: u64,
        /// CPU shares (relative weight)
        #[arg(short, long, default_value = "1024")]
        cpu: u64,
    },
    /// Execute a command inside an agent
    #[command(aliases = ["run", "execute"])]
    Exec {
        /// Agent ID
        agent_id: String,
        /// Command to execute
        command: Vec<String>,
        /// Interactive mode
        #[arg(short, long)]
        interactive: bool,
        /// Allocate a TTY
        #[arg(short, long)]
        tty: bool,
    },
}

pub async fn handle_agent_command(action: AgentCommands, format: OutputFormat) -> Result<()> {
    match action {
        AgentCommands::List { state: _, node: _ } => {
            eprintln!(
                "note: sample/mock data — `agent list` is not yet wired to a live daemon over the control plane"
            );
            let data = mock_agent_list();
            println!("{}", render_output(&data, format)?);
        }
        AgentCommands::Deploy { wasm_path, node: _ } => {
            anyhow::bail!(
                "agent deploy (from {}) requires a live daemon and is not yet supported over the control plane",
                wasm_path
            );
        }
        AgentCommands::Migrate {
            agent_id,
            target_node,
        } => {
            anyhow::bail!(
                "agent migrate ({} -> {}) requires a live daemon and is not yet supported over the control plane",
                agent_id,
                target_node
            );
        }
        AgentCommands::Stop { agent_id } => {
            anyhow::bail!(
                "agent stop ({}) requires a live daemon and is not yet supported over the control plane",
                agent_id
            );
        }
        AgentCommands::Inspect { agent_id } => {
            eprintln!(
                "note: sample/mock data — `agent inspect` is not yet wired to a live daemon over the control plane"
            );
            let result = OperationResult {
                success: true,
                message: format!("[sample/mock] Agent {} details", agent_id),
                id: Some(agent_id),
            };
            println!("{}", render_output(&result, format)?);
        }
        AgentCommands::Logs {
            agent_id,
            follow: _,
            lines: _,
        } => {
            anyhow::bail!(
                "agent logs ({}) requires a live daemon and is not yet supported over the control plane",
                agent_id
            );
        }
        AgentCommands::Create {
            wasm_path,
            name,
            node: _,
            env,
            memory: _,
            cpu: _,
        } => {
            use std::path::Path;

            // Validate WASM file exists
            if !Path::new(&wasm_path).exists() {
                anyhow::bail!("WASM file not found: {}", wasm_path);
            }

            // Validate environment variables format
            for env_var in &env {
                if !env_var.contains('=') {
                    anyhow::bail!(
                        "Invalid environment variable format: {}. Expected KEY=VALUE",
                        env_var
                    );
                }
            }

            // Input validation above is real; the actual creation step needs a
            // live daemon connection over the control plane, which does not
            // exist yet (the control server exposes no POST endpoint for
            // agent creation). Report this honestly instead of fabricating a
            // created agent id.
            anyhow::bail!(
                "agent create ('{}' from {}) requires a live daemon and is not yet supported over the control plane",
                name,
                wasm_path
            );
        }
        AgentCommands::Exec {
            agent_id,
            command,
            interactive: _,
            tty: _,
        } => {
            if command.is_empty() {
                anyhow::bail!("No command specified");
            }

            let cmd_str = command.join(" ");
            anyhow::bail!(
                "agent exec ('{}' in {}) requires a live daemon and is not yet supported over the control plane",
                cmd_str,
                agent_id
            );
        }
    }
    Ok(())
}

fn mock_agent_list() -> AgentList {
    AgentList {
        agents: vec![
            AgentInfo {
                id: "agent-001-uuid-here-1234567890ab".to_string(),
                state: "Running".to_string(),
                node: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                dna_hash: "sha256:abc123def456789012345678901234567890".to_string(),
                memory_mb: 12.5,
                uptime: "2h 30m".to_string(),
            },
            AgentInfo {
                id: "agent-002-uuid-here-abcdef123456".to_string(),
                state: "Paused".to_string(),
                node: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                dna_hash: "sha256:def456abc789012345678901234567890123".to_string(),
                memory_mb: 8.2,
                uptime: "5h 15m".to_string(),
            },
        ],
        total: 2,
    }
}
