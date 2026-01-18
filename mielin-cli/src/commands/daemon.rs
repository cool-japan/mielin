//! Daemon command handler

use crate::output::{render_output, OutputFormat};
use crate::types::OperationResult;
use anyhow::Result;
use mielin_mesh_core::service::{MeshConfig, MeshService};
use mielin_mesh_core::{Node, NodeRole};
use std::net::SocketAddr;
use std::sync::Arc;

pub async fn handle_daemon_command(
    listen: String,
    role: String,
    bootstrap: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let bind_address: SocketAddr = listen
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid listen address '{}': {}", listen, e))?;

    let node_role = match role.to_lowercase().as_str() {
        "core" => NodeRole::Core,
        "relay" => NodeRole::Relay,
        _ => NodeRole::Edge,
    };

    let node = Arc::new(Node::new(node_role));
    let node_id = *node.id();

    let mut config = MeshConfig {
        bind_address,
        bootstrap_nodes: Vec::new(),
        enable_mdns: true,
        enable_gossip: true,
        enable_registry: true,
        enable_migration: true,
    };

    if let Some(bootstrap_addr) = bootstrap {
        let addr: SocketAddr = bootstrap_addr.parse().map_err(|e| {
            anyhow::anyhow!("Invalid bootstrap address '{}': {}", bootstrap_addr, e)
        })?;
        config
            .bootstrap_nodes
            .push(mielin_mesh_core::discovery::BootstrapNode {
                address: addr,
                public_key: None,
            });
    }

    let mut service = MeshService::new(node.clone(), config)?;

    let result = OperationResult {
        success: true,
        message: format!(
            "Starting MielinOS daemon on {} as {} node (ID: {})",
            bind_address, role, node_id
        ),
        id: Some(node_id.to_string()),
    };
    println!("{}", render_output(&result, format)?);

    service.start().await?;

    let result = OperationResult {
        success: true,
        message: "Mesh service started. Press Ctrl+C to stop.".to_string(),
        id: None,
    };
    println!("{}", render_output(&result, format)?);

    tokio::signal::ctrl_c().await?;

    let result = OperationResult {
        success: true,
        message: "Shutting down...".to_string(),
        id: None,
    };
    println!("{}", render_output(&result, format)?);

    service.stop().await?;

    let result = OperationResult {
        success: true,
        message: "Daemon stopped gracefully".to_string(),
        id: None,
    };
    println!("{}", render_output(&result, format)?);

    Ok(())
}
