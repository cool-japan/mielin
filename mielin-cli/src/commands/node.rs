//! Node command handlers

use crate::config::Config;
use crate::output::{render_output, OutputFormat};
use crate::types::{NodeInfo, NodeList, OperationResult};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum NodeCommands {
    /// List all nodes in the cluster
    #[command(alias = "ls")]
    List,
    /// Show detailed node information
    #[command(aliases = ["show", "details"])]
    Info {
        /// Node ID to inspect
        node_id: String,
    },
    /// Start the local node
    Start,
    /// Stop the local node
    #[command(aliases = ["kill", "terminate"])]
    Stop,
    /// Create a new node
    #[command(aliases = ["new", "add"])]
    Create {
        /// Node role (core, relay, edge)
        #[arg(short, long, default_value = "edge")]
        role: String,
    },
    /// Join an existing cluster
    #[command(alias = "connect")]
    Join {
        /// Bootstrap node address
        address: String,
    },
    /// Leave the cluster gracefully
    #[command(aliases = ["disconnect", "depart"])]
    Leave,
    /// View or edit node configuration
    #[command(aliases = ["cfg", "configure"])]
    Config {
        /// Configuration key to get/set
        key: Option<String>,
        /// Value to set (if not provided, displays current value)
        value: Option<String>,
        /// Show all configuration
        #[arg(short, long)]
        all: bool,
    },
}

pub async fn handle_node_command(action: NodeCommands, format: OutputFormat) -> Result<()> {
    match action {
        NodeCommands::List => {
            let data = mock_node_list();
            println!("{}", render_output(&data, format)?);
        }
        NodeCommands::Info { node_id } => {
            let result = OperationResult {
                success: true,
                message: format!("Node info for {}", node_id),
                id: Some(node_id),
            };
            println!("{}", render_output(&result, format)?);
        }
        NodeCommands::Start => {
            let result = OperationResult {
                success: true,
                message: "Node started successfully".to_string(),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        NodeCommands::Stop => {
            let result = OperationResult {
                success: true,
                message: "Node stopped successfully".to_string(),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        NodeCommands::Create { role } => {
            let result = OperationResult {
                success: true,
                message: format!("Created {} node", role),
                id: Some("new-node-uuid".to_string()),
            };
            println!("{}", render_output(&result, format)?);
        }
        NodeCommands::Join { address } => {
            let result = OperationResult {
                success: true,
                message: format!("Joined cluster at {}", address),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        NodeCommands::Leave => {
            let result = OperationResult {
                success: true,
                message: "Left cluster gracefully".to_string(),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        NodeCommands::Config { key, value, all } => {
            let mut config = Config::load()?;

            if all {
                let config_str = toml::to_string_pretty(&config)?;
                println!("{}", config_str);
            } else if let Some(k) = key {
                if let Some(v) = value {
                    config.set(&k, &v)?;
                    config.save()?;
                    let result = OperationResult {
                        success: true,
                        message: format!("Set {} = {}", k, v),
                        id: None,
                    };
                    println!("{}", render_output(&result, format)?);
                } else if let Some(current_value) = config.get(&k) {
                    println!("{} = {}", k, current_value);
                } else {
                    let result = OperationResult {
                        success: false,
                        message: format!("Unknown configuration key: {}", k),
                        id: None,
                    };
                    println!("{}", render_output(&result, format)?);
                }
            } else {
                let result = OperationResult {
                    success: false,
                    message: "Please specify a configuration key or use --all".to_string(),
                    id: None,
                };
                println!("{}", render_output(&result, format)?);
            }
        }
    }
    Ok(())
}

fn mock_node_list() -> NodeList {
    NodeList {
        nodes: vec![
            NodeInfo {
                id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string(),
                role: "Core".to_string(),
                state: "Running".to_string(),
                address: "192.168.1.100:9000".to_string(),
                agents: 5,
                uptime: "3d 2h".to_string(),
                version: "0.0.1".to_string(),
            },
            NodeInfo {
                id: "b2c3d4e5-f6a7-8901-bcde-f12345678901".to_string(),
                role: "Edge".to_string(),
                state: "Running".to_string(),
                address: "192.168.1.101:9000".to_string(),
                agents: 2,
                uptime: "1d 5h".to_string(),
                version: "0.0.1".to_string(),
            },
        ],
        total: 2,
    }
}
