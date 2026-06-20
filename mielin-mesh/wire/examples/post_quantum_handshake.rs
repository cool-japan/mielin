//! Post-Quantum Hybrid Key Exchange Example
//!
//! Demonstrates a complete ML-KEM + X25519 hybrid key exchange between
//! a simulated client and server, verifying the derived shared secrets match.
//!
//! Run with:
//! ```bash
//! cargo run --example post_quantum_handshake
//! ```

use mielin_mesh_wire::quantum::{HybridKexState, MlKemVariant};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    info!("=== Post-Quantum Hybrid Key Exchange Demo ===");

    let variant = MlKemVariant::MlKem768;

    // Print variant metadata
    info!(
        "Variant: {}, security_bits={}, hybrid_group_id=0x{:04X}",
        variant,
        variant.security_bits(),
        variant.hybrid_group_id()
    );
    info!(
        "Encapsulation key size: {} bytes, ciphertext size: {} bytes",
        variant.encapsulation_key_bytes(),
        variant.ciphertext_bytes()
    );

    // --- Client: generate ephemeral hybrid key pair ---
    let client = HybridKexState::new(variant)?;
    let client_share = client.public_key_share();
    info!(
        "Client share: group_id=0x{:04X}, classical_share.len={}, pq_share.len={}",
        client_share.group_id,
        client_share.classical_share.len(),
        client_share.pq_share.len()
    );

    // --- Server: respond to the client's key share ---
    let server = HybridKexState::new(variant)?;
    let (server_share, server_secret) = server.server_respond(&client_share)?;
    info!(
        "Server share: group_id=0x{:04X}, classical_share.len={}, pq_share.len={}",
        server_share.group_id,
        server_share.classical_share.len(),
        server_share.pq_share.len()
    );

    // --- Client: finish the key exchange using the server's share ---
    let server_ciphertext = server_share.pq_share.clone();
    let client_secret = client.client_finish(&server_share, &server_ciphertext)?;

    // --- Verify both sides derived the same combined session key ---
    assert_eq!(
        client_secret.combined(),
        server_secret.combined(),
        "secrets must match"
    );

    info!(
        "Key exchange successful! Combined session key (first 8 bytes): {:?}",
        &client_secret.combined()[..8]
    );

    info!("Post-quantum handshake example completed successfully");
    Ok(())
}
