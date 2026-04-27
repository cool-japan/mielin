//! Daemon command handler

use crate::control::ControlServer;
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
    control_listen: SocketAddr,
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

    // Wrap in Arc so the ControlServer can share it without needing ownership.
    let mesh = Arc::new(service);

    let result = OperationResult {
        success: true,
        message: format!(
            "Mesh service started. Control plane on {}. Press Ctrl+C to stop.",
            control_listen
        ),
        id: None,
    };
    println!("{}", render_output(&result, format)?);

    let control_server = ControlServer::new(mesh.clone());

    tokio::select! {
        res = control_server.serve(control_listen) => {
            if let Err(e) = res {
                tracing::error!("Control plane exited with error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, shutting down");
        }
    }

    let result = OperationResult {
        success: true,
        message: "Shutting down...".to_string(),
        id: None,
    };
    println!("{}", render_output(&result, format)?);

    // Recover the MeshService from the Arc to call stop().
    // The ControlServer has been dropped at this point, so try_unwrap should succeed.
    match Arc::try_unwrap(mesh) {
        Ok(mut svc) => {
            svc.stop().await?;
        }
        Err(_arc) => {
            tracing::warn!(
                "Could not obtain exclusive ownership of MeshService for clean shutdown"
            );
        }
    }

    let result = OperationResult {
        success: true,
        message: "Daemon stopped gracefully".to_string(),
        id: None,
    };
    println!("{}", render_output(&result, format)?);

    Ok(())
}
