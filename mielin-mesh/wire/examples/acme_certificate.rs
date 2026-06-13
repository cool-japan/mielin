//! ACME (Let's Encrypt) Certificate Example
//!
//! This example demonstrates how to request a certificate from Let's Encrypt
//! using the ACME protocol. Note: This requires actual domain ownership and
//! the ability to complete HTTP-01 or DNS-01 challenges.
//!
//! Run with:
//! ```bash
//! cargo run --example acme_certificate
//! ```

use mielin_mesh_wire::certs::acme::{
    AcmeChallengeType, AcmeClient, AcmeConfig, ChallengeValidator,
};
use mielin_mesh_wire::certs::CertError;
use std::error::Error;
use std::sync::Arc;
use tracing::{info, warn, Level};

// Simple HTTP-01 challenge validator implementation
struct SimpleHttp01Validator;

impl ChallengeValidator for SimpleHttp01Validator {
    fn validate_http01(
        &self,
        domain: &str,
        token: &str,
        key_authorization: &str,
    ) -> Result<(), CertError> {
        info!("HTTP-01 Challenge for domain: {}", domain);
        info!("  Token: {}", token);
        info!("  Key Authorization: {}", key_authorization);
        info!("");
        info!("To complete this challenge, you need to:");
        info!(
            "  1. Create a file at: http://{}/.well-known/acme-challenge/{}",
            domain, token
        );
        info!("  2. File content should be: {}", key_authorization);
        info!("");
        warn!("⚠️  This is a demonstration validator - implement actual HTTP server!");

        // In a real implementation, you would:
        // - Start an HTTP server on port 80
        // - Serve the key_authorization at the specified path
        // - Keep the server running until the challenge is validated

        Ok(())
    }

    fn validate_dns01(
        &self,
        domain: &str,
        txt_record: &str,
        key_authorization_digest: &str,
    ) -> Result<(), CertError> {
        info!("DNS-01 Challenge for domain: {}", domain);
        info!("  TXT Record: {}", txt_record);
        info!("  Value: {}", key_authorization_digest);
        info!("");
        info!("To complete this challenge, you need to:");
        info!("  1. Create a TXT record: {}", txt_record);
        info!("  2. TXT record value: {}", key_authorization_digest);
        info!("");
        warn!("⚠️  This is a demonstration validator - implement actual DNS API!");

        // In a real implementation, you would:
        // - Use your DNS provider's API
        // - Create the TXT record
        // - Wait for DNS propagation (usually a few minutes)

        Ok(())
    }

    fn cleanup_http01(&self, domain: &str, token: &str) -> Result<(), CertError> {
        info!("Cleaning up HTTP-01 challenge for {}: {}", domain, token);
        // Remove the challenge file
        Ok(())
    }

    fn cleanup_dns01(&self, _domain: &str, txt_record: &str) -> Result<(), CertError> {
        info!("Cleaning up DNS-01 challenge: {}", txt_record);
        // Remove the TXT record
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting ACME certificate example");
    info!("⚠️  WARNING: This example requires:");
    info!("  - A valid domain name that you control");
    info!("  - Ability to complete ACME challenges (HTTP-01 or DNS-01)");
    info!("  - For production use, implement actual challenge validators");
    info!("");

    // Configure ACME client
    let config = AcmeConfig::new()
        .with_email("admin@example.com".to_string())
        .with_challenge_type(AcmeChallengeType::Http01)
        .with_staging() // Use staging environment for testing
        .with_renewal_threshold(30);

    info!("ACME Configuration:");
    info!("  Contact: {:?}", config.contact_emails);
    info!("  Challenge Type: {:?}", config.challenge_type);
    info!("  Staging: {}", config.use_staging);
    info!(
        "  Renewal Threshold: {} days",
        config.renewal_threshold_days
    );
    info!("");

    // Create ACME client with validator
    let validator = Arc::new(SimpleHttp01Validator);
    let client = AcmeClient::new(config)?.with_validator(validator);

    // Initialize ACME account
    info!("Initializing ACME account...");
    match client.initialize_account().await {
        Ok(_) => info!("✓ ACME account initialized successfully"),
        Err(e) => {
            warn!("Failed to initialize ACME account: {}", e);
            warn!("This is expected in a demonstration - actual ACME requires network access");
            return Ok(());
        }
    }

    // Request certificate for domain(s)
    let domains = vec!["example.com".to_string()];
    info!("\nRequesting certificate for domains: {:?}", domains);

    match client.request_certificate(domains.clone()).await {
        Ok(cert) => {
            info!("✓ Certificate obtained successfully!");
            info!("  Common Name: {}", cert.info.common_name);
            info!("  SANs: {:?}", cert.info.subject_alt_names);
            info!("  Valid for: {} days", cert.info.validity_days);
            info!("  Expires: {:?}", cert.info.expires_at);
            info!("  Certificate chain length: {}", cert.cert_chain.len());
        }
        Err(e) => {
            warn!("Failed to obtain certificate: {}", e);
            warn!("Expected - this demo requires actual domain ownership and HTTP server");
        }
    }

    info!("\n--- ACME Integration Notes ---");
    info!("For production use:");
    info!("  1. Implement ChallengeValidator trait properly");
    info!("  2. For HTTP-01: Set up HTTP server on port 80");
    info!("  3. For DNS-01: Integrate with your DNS provider's API");
    info!("  4. Use production ACME server (remove .with_staging())");
    info!("  5. Store certificates securely");
    info!("  6. Implement automatic renewal (run before expiry)");
    info!("");

    info!("ACME certificate example completed");
    Ok(())
}
