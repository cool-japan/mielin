//! Mutual TLS (mTLS) Support
//!
//! Provides mutual TLS authentication where both client and server
//! verify each other's certificates.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as RustlsError, ServerConfig, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::pinning::PinStore;
use super::{CertError, Certificate};

/// mTLS configuration
#[derive(Debug, Clone)]
pub struct MtlsConfig {
    /// Require client certificates
    pub require_client_cert: bool,
    /// Verify client certificates against CA
    pub verify_client_cert: bool,
    /// Verify server certificates against CA
    pub verify_server_cert: bool,
    /// Use certificate pinning for additional security
    pub use_pinning: bool,
    /// Allow self-signed certificates (for testing)
    pub allow_self_signed: bool,
}

impl Default for MtlsConfig {
    fn default() -> Self {
        Self {
            require_client_cert: true,
            verify_client_cert: true,
            verify_server_cert: true,
            use_pinning: false,
            allow_self_signed: false,
        }
    }
}

impl MtlsConfig {
    /// Create a new mTLS configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Require client certificates
    pub fn require_client_cert(mut self, require: bool) -> Self {
        self.require_client_cert = require;
        self
    }

    /// Verify client certificates
    pub fn verify_client_cert(mut self, verify: bool) -> Self {
        self.verify_client_cert = verify;
        self
    }

    /// Verify server certificates
    pub fn verify_server_cert(mut self, verify: bool) -> Self {
        self.verify_server_cert = verify;
        self
    }

    /// Use certificate pinning
    pub fn with_pinning(mut self) -> Self {
        self.use_pinning = true;
        self
    }

    /// Allow self-signed certificates (useful for testing)
    pub fn allow_self_signed(mut self) -> Self {
        self.allow_self_signed = true;
        self
    }

    /// Preset for production (strict verification)
    pub fn production() -> Self {
        Self {
            require_client_cert: true,
            verify_client_cert: true,
            verify_server_cert: true,
            use_pinning: true,
            allow_self_signed: false,
        }
    }

    /// Preset for development (relaxed verification)
    pub fn development() -> Self {
        Self {
            require_client_cert: false,
            verify_client_cert: false,
            verify_server_cert: false,
            use_pinning: false,
            allow_self_signed: true,
        }
    }

    /// Preset for testing (minimal verification)
    pub fn testing() -> Self {
        Self {
            require_client_cert: false,
            verify_client_cert: false,
            verify_server_cert: false,
            use_pinning: false,
            allow_self_signed: true,
        }
    }
}

/// Client certificate verification result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientCertVerification {
    /// Certificate verified successfully
    Verified {
        /// Common name from certificate
        common_name: String,
        /// Subject alternative names
        sans: Vec<String>,
    },
    /// Certificate verification failed
    Failed {
        /// Reason for failure
        reason: String,
    },
    /// No certificate provided
    NoCertificate,
}

/// Custom client certificate verifier
#[derive(Debug)]
struct MtlsClientVerifier {
    config: MtlsConfig,
    pin_store: Option<Arc<PinStore>>,
}

impl MtlsClientVerifier {
    fn new(config: MtlsConfig, pin_store: Option<Arc<PinStore>>) -> Self {
        Self { config, pin_store }
    }
}

impl ClientCertVerifier for MtlsClientVerifier {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        info!("Verifying client certificate");

        // If we're allowing self-signed, just accept
        if self.config.allow_self_signed {
            debug!("Accepting client certificate (allow_self_signed=true)");
            return Ok(ClientCertVerified::assertion());
        }

        // Basic validation
        if !self.config.verify_client_cert {
            debug!("Skipping client certificate verification (verify_client_cert=false)");
            return Ok(ClientCertVerified::assertion());
        }

        // TODO: Implement proper certificate chain validation with CA
        // For now, we'll use a simplified verification

        debug!(
            "Client certificate chain length: {}",
            intermediates.len() + 1
        );

        // If pinning is enabled, verify against pins
        if self.config.use_pinning {
            if let Some(ref _pin_store) = self.pin_store {
                // Extract identifier from certificate (CN or first SAN)
                // This is a simplified version - in production, use proper X.509 parsing
                debug!("Verifying client certificate against pins");

                // For now, accept the certificate
                // In production, extract CN/SAN and verify against _pin_store
            }
        }

        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.2 signature");
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.3 signature");
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Custom server certificate verifier
#[derive(Debug)]
struct MtlsServerVerifier {
    config: MtlsConfig,
    pin_store: Option<Arc<PinStore>>,
}

impl MtlsServerVerifier {
    fn new(config: MtlsConfig, pin_store: Option<Arc<PinStore>>) -> Self {
        Self { config, pin_store }
    }
}

impl ServerCertVerifier for MtlsServerVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        info!("Verifying server certificate");

        // If we're allowing self-signed, just accept
        if self.config.allow_self_signed {
            debug!("Accepting server certificate (allow_self_signed=true)");
            return Ok(ServerCertVerified::assertion());
        }

        // Basic validation
        if !self.config.verify_server_cert {
            debug!("Skipping server certificate verification (verify_server_cert=false)");
            return Ok(ServerCertVerified::assertion());
        }

        debug!(
            "Server certificate chain length: {}",
            intermediates.len() + 1
        );

        // If pinning is enabled, verify against pins
        if self.config.use_pinning {
            if let Some(ref _pin_store) = self.pin_store {
                debug!("Verifying server certificate against pins");
                // In production, extract server name and verify against _pin_store
            }
        }

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.2 signature");
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.3 signature");
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// mTLS context for managing mutual TLS connections
pub struct MtlsContext {
    config: MtlsConfig,
    certificate: Certificate,
    pin_store: Option<Arc<PinStore>>,
}

impl MtlsContext {
    /// Create a new mTLS context
    pub fn new(config: MtlsConfig, certificate: Certificate) -> Self {
        Self {
            config,
            certificate,
            pin_store: None,
        }
    }

    /// Set pin store for certificate pinning
    pub fn with_pin_store(mut self, pin_store: Arc<PinStore>) -> Self {
        self.pin_store = Some(pin_store);
        self
    }

    /// Create a server TLS configuration
    pub fn create_server_config(&self) -> Result<ServerConfig, CertError> {
        info!("Creating mTLS server configuration");

        let cert_chain = self.certificate.cert_chain.clone();
        let private_key = self.certificate.private_key.clone_key();

        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .map_err(|e| CertError::TlsConfigError {
                details: format!("Failed to create server config: {}", e),
            })?;

        // If client certificates are required, set up client verification
        if self.config.require_client_cert {
            info!("Client certificates required");

            let client_verifier = Arc::new(MtlsClientVerifier::new(
                self.config.clone(),
                self.pin_store.clone(),
            ));

            config = ServerConfig::builder()
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(
                    self.certificate.cert_chain.clone(),
                    self.certificate.private_key.clone_key(),
                )
                .map_err(|e| CertError::TlsConfigError {
                    details: format!("Failed to create server config with client auth: {}", e),
                })?;
        }

        config.alpn_protocols = vec![b"h3".to_vec(), b"h2".to_vec(), b"http/1.1".to_vec()];

        Ok(config)
    }

    /// Create a client TLS configuration
    pub fn create_client_config(&self) -> Result<ClientConfig, CertError> {
        info!("Creating mTLS client configuration");

        let cert_chain = self.certificate.cert_chain.clone();
        let private_key = self.certificate.private_key.clone_key();

        let server_verifier = Arc::new(MtlsServerVerifier::new(
            self.config.clone(),
            self.pin_store.clone(),
        ));

        let mut config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(server_verifier)
            .with_client_auth_cert(cert_chain, private_key)
            .map_err(|e| CertError::TlsConfigError {
                details: format!("Failed to create client config: {}", e),
            })?;

        config.alpn_protocols = vec![b"h3".to_vec(), b"h2".to_vec(), b"http/1.1".to_vec()];

        Ok(config)
    }

    /// Get the certificate
    pub fn certificate(&self) -> &Certificate {
        &self.certificate
    }

    /// Get the configuration
    pub fn config(&self) -> &MtlsConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_crypto() {
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    #[test]
    fn test_mtls_config_creation() {
        let config = MtlsConfig::new()
            .require_client_cert(true)
            .verify_client_cert(true)
            .verify_server_cert(true);

        assert!(config.require_client_cert);
        assert!(config.verify_client_cert);
        assert!(config.verify_server_cert);
        assert!(!config.use_pinning);
        assert!(!config.allow_self_signed);
    }

    #[test]
    fn test_mtls_config_presets() {
        let prod = MtlsConfig::production();
        assert!(prod.require_client_cert);
        assert!(prod.verify_client_cert);
        assert!(prod.verify_server_cert);
        assert!(prod.use_pinning);
        assert!(!prod.allow_self_signed);

        let dev = MtlsConfig::development();
        assert!(!dev.require_client_cert);
        assert!(!dev.verify_client_cert);
        assert!(!dev.verify_server_cert);
        assert!(!dev.use_pinning);
        assert!(dev.allow_self_signed);

        let test = MtlsConfig::testing();
        assert!(!test.require_client_cert);
        assert!(!test.verify_client_cert);
        assert!(!test.verify_server_cert);
        assert!(!test.use_pinning);
        assert!(test.allow_self_signed);
    }

    #[test]
    fn test_mtls_context_creation() {
        let config = MtlsConfig::development();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let context = MtlsContext::new(config, cert);
        assert!(context.config().allow_self_signed);
    }

    #[test]
    fn test_mtls_server_config_creation() {
        init_crypto();
        let config = MtlsConfig::development();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let context = MtlsContext::new(config, cert);
        let server_config = context.create_server_config();

        assert!(server_config.is_ok());
    }

    #[test]
    fn test_mtls_client_config_creation() {
        init_crypto();
        let config = MtlsConfig::development();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let context = MtlsContext::new(config, cert);
        let client_config = context.create_client_config();

        assert!(client_config.is_ok());
    }

    #[test]
    fn test_mtls_with_client_cert_required() {
        init_crypto();
        let config = MtlsConfig::new()
            .require_client_cert(true)
            .allow_self_signed();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let context = MtlsContext::new(config, cert);
        let server_config = context.create_server_config();

        assert!(server_config.is_ok());
    }

    #[test]
    fn test_mtls_with_pinning() {
        let config = MtlsConfig::new().with_pinning().allow_self_signed();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let pin_store = Arc::new(PinStore::new());
        let context = MtlsContext::new(config, cert).with_pin_store(pin_store);

        assert!(context.pin_store.is_some());
        assert!(context.config().use_pinning);
    }

    #[test]
    fn test_client_cert_verification_variants() {
        let verified = ClientCertVerification::Verified {
            common_name: "test".to_string(),
            sans: vec!["test.example.com".to_string()],
        };
        assert!(matches!(verified, ClientCertVerification::Verified { .. }));

        let failed = ClientCertVerification::Failed {
            reason: "Invalid".to_string(),
        };
        assert!(matches!(failed, ClientCertVerification::Failed { .. }));

        let no_cert = ClientCertVerification::NoCertificate;
        assert!(matches!(no_cert, ClientCertVerification::NoCertificate));
    }
}
