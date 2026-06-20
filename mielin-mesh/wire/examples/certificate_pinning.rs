//! Certificate Pinning Example
//!
//! Demonstrates using PinStore for HPKP-style certificate pinning,
//! including adding, retrieving, and verifying pins.
//!
//! Run with:
//! ```bash
//! cargo run --example certificate_pinning
//! ```

use mielin_mesh_wire::certs::pinning::{
    Pin, PinHashAlgorithm, PinStore, PinType, PinVerificationResult,
};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    info!("Starting certificate pinning example");

    // Create a new pin store
    let pin_store = PinStore::new();

    // Build a fake 64-character lowercase hex SHA-256 hash
    let fake_hash = "a".repeat(64);
    info!("Fake pin hash: {}", fake_hash);

    // Create a pin for "example-node"
    let pin = Pin::new(
        "example-node".to_string(),
        PinType::Certificate,
        PinHashAlgorithm::Sha256,
        fake_hash.clone(),
    );
    info!(
        "Created pin: identifier={}, type={:?}, algo={:?}",
        pin.identifier, pin.pin_type, pin.hash_algorithm
    );

    // Add the pin to the store
    pin_store.add_pin(pin).await?;
    info!("Pin added to store");

    // Retrieve pins for "example-node"
    let pins = pin_store.get_pins("example-node").await;
    info!(
        "Pins for 'example-node': count={}, first_identifier={}",
        pins.len(),
        pins.first().map(|p| p.identifier.as_str()).unwrap_or("(none)")
    );

    // Verify against an empty cert slice — pins exist but no cert bytes to match
    let result = pin_store.verify_certificate("example-node", &[]).await?;
    info!(
        "Verification result for 'example-node' (empty certs): {:?}, is_success={}",
        result,
        result.is_success()
    );

    match &result {
        PinVerificationResult::NoMatch {
            identifier,
            expected_pins,
            actual_hash,
        } => {
            info!(
                "NoMatch details: identifier={}, expected_pins_count={}, actual_hash='{}'",
                identifier,
                expected_pins.len(),
                actual_hash
            );
        }
        other => {
            info!("Unexpected result variant: {:?}", other);
        }
    }

    // Also verify for a nonexistent identifier — should yield NoPins
    let no_pins_result = pin_store.verify_certificate("nonexistent", &[]).await?;
    info!(
        "Verification result for 'nonexistent': {:?}, is_success={}",
        no_pins_result,
        no_pins_result.is_success()
    );

    match &no_pins_result {
        PinVerificationResult::NoPins { identifier } => {
            info!("NoPins: no pins registered for '{}'", identifier);
        }
        other => {
            info!("Unexpected result variant: {:?}", other);
        }
    }

    info!("Certificate pinning example completed successfully");
    Ok(())
}
