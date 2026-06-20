//! QUIC Echo Example
//!
//! Demonstrates a simple QUIC echo server/client exchange over a loopback connection.
//!
//! Run with:
//! ```bash
//! cargo run --example quic_echo
//! ```

use mielin_mesh_wire::{transport::QuicTransport, Message};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    info!("Starting QUIC echo example");

    // Bind a server transport on a random port
    let bind_addr: SocketAddr = "127.0.0.1:0".parse()?;
    let server = QuicTransport::new(bind_addr).await?;
    let server_addr = server.local_addr()?;
    info!("Server listening on {}", server_addr);

    // Spawn server task: accept one connection and receive one message
    let server_task = tokio::spawn(async move {
        info!("Server: waiting for connection");
        let conn = server.accept().await.map_err(|e| e.to_string())?;
        info!("Server: connection accepted from {:?}", conn.remote_addr());
        let msg = conn.receive().await.map_err(|e| e.to_string())?;
        info!("Server: received message: {:?}", msg);
        Ok::<Message, String>(msg)
    });

    // Give the server a moment to bind and start accepting
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create a client transport and connect to the server
    let client = QuicTransport::new_client().await?;
    info!("Client: connecting to {}", server_addr);
    let conn = client.connect(server_addr).await?;
    info!("Client: connected");

    // Send a Ping message
    let ping = Message::Ping { timestamp: 12345 };
    conn.send(&ping).await?;
    info!("Client: sent {:?}", ping);

    // Wait for the server task to finish (with a 5-second timeout)
    let join_result = timeout(Duration::from_secs(5), server_task).await??;
    let received = join_result.map_err(|e| e.to_string())?;
    info!("Echo exchange succeeded — server received: {:?}", received);

    info!("QUIC echo example completed successfully");
    Ok(())
}
