//! Comprehensive Certificate Management Example
//!
//! This example demonstrates the complete certificate management system including:
//! - Automatic certificate renewal
//! - Certificate pinning
//! - Mutual TLS (mTLS)
//! - Certificate Authority integration
//!
//! Run with:
//! ```bash
//! cargo run --example certificate_management
//! ```

use mielin_mesh_wire::certs::{
    ca::{CaConfig, CertificateAuthority, RevocationCheckMethod},
    mtls::{MtlsConfig, MtlsContext},
    pinning::{PinHashAlgorithm, PinStore, PinType},
    renewal::{RenewalConfig, RenewalScheduler, RenewalStrategy},
    Certificate,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== MielinOS Mesh Certificate Management Demo ===\n");

    // 1. Generate certificates
    info!("1. Generating certificates...");
    let server_cert = Certificate::generate_self_signed("mesh-server".to_string(), 365)?;
    let client_cert = Certificate::generate_self_signed("mesh-client".to_string(), 365)?;
    info!("   ✓ Server certificate: {}", server_cert.info.common_name);
    info!("   ✓ Client certificate: {}", client_cert.info.common_name);
    info!("");

    // 2. Certificate Pinning
    info!("2. Setting up certificate pinning...");
    let pin_store = Arc::new(PinStore::new());

    // Pin the server certificate
    let server_pin = pin_store
        .pin_certificate(
            "mesh-server".to_string(),
            &server_cert.cert_chain[0],
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            Some(Duration::from_secs(86400 * 365)), // 1 year
        )
        .await?;

    info!("   ✓ Server certificate pinned");
    info!("     Pin hash: {}", &server_pin.hash[..16]);
    info!("     Pin type: {:?}", server_pin.pin_type);
    info!("     Hash algorithm: {:?}", server_pin.hash_algorithm);

    // Verify the pin
    let verify_result = pin_store
        .verify_certificate("mesh-server", &server_cert.cert_chain)
        .await?;

    info!("   ✓ Pin verification: {:?}", verify_result.is_success());
    info!("");

    // 3. Certificate Authority Setup
    info!("3. Setting up Certificate Authority...");
    let ca_config = CaConfig::new()
        .with_revocation_check(RevocationCheckMethod::OcspThenCrl)
        .with_ocsp_timeout(Duration::from_secs(10));

    let ca = CertificateAuthority::new(ca_config);

    // Add a CA certificate (using server cert as CA for demo)
    let ca_fingerprint = ca.add_ca_cert(&server_cert.cert_chain[0]).await?;
    info!("   ✓ CA certificate added");
    info!("     Fingerprint: {}...", &ca_fingerprint[..16]);

    // Check certificate count
    let ca_count = ca.ca_count().await;
    info!("   ✓ Total CA certificates: {}", ca_count);
    info!("");

    // 4. Mutual TLS Configuration
    info!("4. Configuring Mutual TLS (mTLS)...");

    // Server-side mTLS (generate new cert for mTLS)
    let server_mtls_cert = Certificate::generate_self_signed("mtls-server".to_string(), 365)?;
    let server_mtls_config = MtlsConfig::production()
        .require_client_cert(true)
        .with_pinning();

    let server_mtls =
        MtlsContext::new(server_mtls_config, server_mtls_cert).with_pin_store(pin_store.clone());

    info!("   ✓ Server mTLS context created");
    info!(
        "     Requires client cert: {}",
        server_mtls.config().require_client_cert
    );
    info!("     Uses pinning: {}", server_mtls.config().use_pinning);

    // Client-side mTLS (generate new cert for client)
    let client_mtls_cert = Certificate::generate_self_signed("mtls-client".to_string(), 365)?;
    let client_mtls_config = MtlsConfig::production().with_pinning();

    let _client_mtls =
        MtlsContext::new(client_mtls_config, client_mtls_cert).with_pin_store(pin_store.clone());

    info!("   ✓ Client mTLS context created");
    info!("");

    // 5. Automatic Certificate Renewal
    info!("5. Setting up automatic certificate renewal...");

    // Create a short-lived certificate for demo
    let renewable_cert = Certificate::generate_self_signed("renewable-service".to_string(), 30)?;

    let renewal_config = RenewalConfig::new()
        .with_check_interval(Duration::from_secs(3600)) // Check every hour
        .with_renewal_threshold(7); // Renew 7 days before expiry

    let renewal_strategy = RenewalStrategy::SelfSigned {
        node_id: "renewable-service".to_string(),
        validity_days: 365,
    };

    let renewal_scheduler = Arc::new(RenewalScheduler::new(renewal_config, renewal_strategy));
    renewal_scheduler.set_certificate(renewable_cert).await;

    info!("   ✓ Renewal scheduler created");
    info!("     Check interval: 1 hour");
    info!("     Renewal threshold: 7 days");

    // Subscribe to renewal events
    let mut event_rx = renewal_scheduler.subscribe();

    // Spawn renewal scheduler in background
    let scheduler_handle = {
        let scheduler = renewal_scheduler.clone();
        tokio::spawn(async move {
            // Run for a short time for demo
            tokio::time::timeout(Duration::from_secs(2), scheduler.start())
                .await
                .ok();
        })
    };

    // Wait for some events (with timeout)
    tokio::select! {
        Ok(event) = event_rx.recv() => {
            info!("   ✓ Received renewal event: {:?}", event);
        }
        _ = tokio::time::sleep(Duration::from_secs(1)) => {
            info!("   ✓ Renewal scheduler running in background");
        }
    }

    scheduler_handle.abort();
    info!("");

    // 6. Pin Rotation Demo
    info!("6. Demonstrating pin rotation...");

    // Generate a new certificate for rotation
    let new_server_cert = Certificate::generate_self_signed("mesh-server".to_string(), 365)?;

    // Add new certificate as backup pin
    let backup_pin = pin_store
        .rotate_pin(
            "mesh-server",
            &new_server_cert.cert_chain[0],
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
        )
        .await?;

    info!("   ✓ Backup pin created");
    info!("     Is backup: {}", backup_pin.is_backup);

    // Verify both old and new certificates work
    let old_verify = pin_store
        .verify_certificate("mesh-server", &server_cert.cert_chain)
        .await?;
    let new_verify = pin_store
        .verify_certificate("mesh-server", &new_server_cert.cert_chain)
        .await?;

    info!(
        "   ✓ Old certificate verification: {}",
        old_verify.is_success()
    );
    info!(
        "   ✓ New certificate verification: {}",
        new_verify.is_success()
    );

    // Promote backup pin
    let promoted = pin_store.promote_backup_pin("mesh-server", false).await?;
    info!("   ✓ Promoted {} backup pins", promoted);
    info!("");

    // 7. Statistics and Summary
    info!("7. System statistics...");
    info!("   Total pins: {}", pin_store.pin_count().await);
    info!("   Total CAs: {}", ca.ca_count().await);
    info!("   CRL cache size: {}", ca.crl_cache_size().await);

    let current_cert = renewal_scheduler.get_certificate().await;
    if let Some(cert) = current_cert {
        info!("   Renewable cert common name: {}", cert.info.common_name);
        info!(
            "   Renewable cert validity: {} days",
            cert.info.validity_days
        );
        if let Some(time_left) = cert.info.time_until_expiry() {
            info!("   Time until expiry: {} days", time_left.as_secs() / 86400);
        }
    }
    info!("");

    // 8. Cleanup demonstration
    info!("8. Cleanup operations...");

    // Remove expired pins
    let removed_pins = pin_store.cleanup_expired().await;
    info!("   ✓ Removed {} expired pins", removed_pins);

    // Clear CRL cache
    ca.clear_crl_cache().await;
    info!("   ✓ CRL cache cleared");
    info!("");

    info!("=== Certificate Management Demo Complete ===");
    info!("");
    info!("Key Features Demonstrated:");
    info!("  ✓ Certificate generation (self-signed)");
    info!("  ✓ Certificate pinning (SPKI with SHA-256)");
    info!("  ✓ Pin verification and rotation");
    info!("  ✓ Certificate Authority management");
    info!("  ✓ Mutual TLS configuration");
    info!("  ✓ Automatic renewal scheduling");
    info!("  ✓ Event-driven lifecycle management");
    info!("");
    info!("Production Considerations:");
    info!("  • Use Let's Encrypt (ACME) for public certificates");
    info!("  • Implement proper challenge validators for ACME");
    info!("  • Configure revocation checking (OCSP/CRL)");
    info!("  • Set appropriate renewal thresholds");
    info!("  • Monitor renewal events for failures");
    info!("  • Implement secure certificate storage");
    info!("  • Use production mTLS configuration");
    info!("  • Regular pin rotation for critical services");

    Ok(())
}
