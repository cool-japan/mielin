//! mTLS Connection Configuration Example
//!
//! Demonstrates the MtlsConfig builder API and MtlsContext creation,
//! covering both preset and custom configurations.
//!
//! Run with:
//! ```bash
//! cargo run --example mtls_connection
//! ```

use mielin_mesh_wire::certs::{
    mtls::{MtlsConfig, MtlsContext},
    Certificate,
};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    info!("Starting mTLS connection configuration example");

    // --- Custom config via builder ---
    let custom_config = MtlsConfig::new()
        .require_client_cert(true)
        .verify_client_cert(true)
        .verify_server_cert(false)
        .with_pinning()
        .allow_self_signed();

    info!(
        "Custom config: require_client_cert={}, verify_client_cert={}, \
         verify_server_cert={}, use_pinning={}, allow_self_signed={}",
        custom_config.require_client_cert,
        custom_config.verify_client_cert,
        custom_config.verify_server_cert,
        custom_config.use_pinning,
        custom_config.allow_self_signed,
    );

    // --- Production preset ---
    let prod = MtlsConfig::production();
    info!(
        "Production preset: require_client_cert={}, verify_client_cert={}, \
         verify_server_cert={}, use_pinning={}, allow_self_signed={}",
        prod.require_client_cert,
        prod.verify_client_cert,
        prod.verify_server_cert,
        prod.use_pinning,
        prod.allow_self_signed,
    );

    // --- Development preset ---
    let dev = MtlsConfig::development();
    info!(
        "Development preset: require_client_cert={}, verify_client_cert={}, \
         verify_server_cert={}, use_pinning={}, allow_self_signed={}",
        dev.require_client_cert,
        dev.verify_client_cert,
        dev.verify_server_cert,
        dev.use_pinning,
        dev.allow_self_signed,
    );

    // --- Generate a self-signed certificate ---
    let cert = Certificate::generate_self_signed("demo-node".to_string(), 90)?;
    info!(
        "Generated self-signed cert: common_name={}, validity_days={}",
        cert.info.common_name, cert.info.validity_days
    );

    // --- Create an MtlsContext with the development config ---
    let context = MtlsContext::new(MtlsConfig::development(), cert);
    let cfg = context.config();
    info!(
        "MtlsContext config: require_client_cert={}, allow_self_signed={}",
        cfg.require_client_cert, cfg.allow_self_signed,
    );
    info!(
        "MtlsContext certificate common name: {}",
        context.certificate().info.common_name
    );

    info!("mTLS configuration example completed successfully");
    Ok(())
}
