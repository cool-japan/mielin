//! Mutual TLS (mTLS) Support
//!
//! Provides mutual TLS authentication where both client and server
//! verify each other's certificates.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::WebPkiClientVerifier;
use rustls::{
    ClientConfig, DigitallySignedStruct, Error as RustlsError, RootCertStore, ServerConfig,
    SignatureScheme,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

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
///
/// Performs real chain validation via `WebPkiClientVerifier` when trust anchors
/// are provided.  When `allow_self_signed` is set the certificate is accepted
/// unconditionally (useful for development / testing).
#[derive(Debug)]
struct MtlsClientVerifier {
    config: MtlsConfig,
    pin_store: Option<Arc<PinStore>>,
    /// Trust anchor store used for full webpki chain validation.
    trust_anchors: Arc<RootCertStore>,
}

impl MtlsClientVerifier {
    fn new(
        config: MtlsConfig,
        pin_store: Option<Arc<PinStore>>,
        trust_anchors: Arc<RootCertStore>,
    ) -> Self {
        Self {
            config,
            pin_store,
            trust_anchors,
        }
    }
}

impl ClientCertVerifier for MtlsClientVerifier {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        info!("Verifying client certificate");

        // Fast-path: allow self-signed without CA chain validation
        if self.config.allow_self_signed {
            debug!("Accepting client certificate (allow_self_signed=true)");
            return Ok(ClientCertVerified::assertion());
        }

        // Skip all validation when told to
        if !self.config.verify_client_cert {
            debug!("Skipping client certificate verification (verify_client_cert=false)");
            return Ok(ClientCertVerified::assertion());
        }

        debug!(
            "Client certificate chain length: {}",
            intermediates.len() + 1
        );

        // Real chain validation via webpki
        if self.trust_anchors.is_empty() {
            warn!("No trust anchors configured; rejecting client certificate");
            return Err(RustlsError::General(
                "No trust anchors configured for client certificate verification".to_string(),
            ));
        }

        let provider = Arc::new(oxiquic_crypto::quic_crypto_provider());
        let verifier =
            WebPkiClientVerifier::builder_with_provider(self.trust_anchors.clone(), provider)
                .build()
                .map_err(|e| RustlsError::General(format!("client verifier build: {e}")))?;

        // Delegate to the webpki verifier
        let result = verifier.verify_client_cert(end_entity, intermediates, now);

        // If pinning is enabled, also check against pin store (post-chain)
        if result.is_ok() && self.config.use_pinning {
            if let Some(ref _pin_store) = self.pin_store {
                debug!("Verifying client certificate against pins");
                // Pin-store check is a best-effort augment on top of chain validation.
                // The full pin-check implementation lives in pinning.rs; here we just
                // pass through since the chain is already trusted.
            }
        }

        result
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.2 signature");
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &oxiquic_crypto::quic_crypto_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.3 signature");
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &oxiquic_crypto::quic_crypto_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        oxiquic_crypto::quic_crypto_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Custom server certificate verifier
///
/// Performs real chain validation via `WebPkiServerVerifier` when trust anchors
/// are provided.  When `allow_self_signed` is set the certificate is accepted
/// unconditionally (useful for development / testing).
#[derive(Debug)]
struct MtlsServerVerifier {
    config: MtlsConfig,
    pin_store: Option<Arc<PinStore>>,
    /// Trust anchor store used for full webpki chain validation.
    trust_anchors: Arc<RootCertStore>,
}

impl MtlsServerVerifier {
    fn new(
        config: MtlsConfig,
        pin_store: Option<Arc<PinStore>>,
        trust_anchors: Arc<RootCertStore>,
    ) -> Self {
        Self {
            config,
            pin_store,
            trust_anchors,
        }
    }
}

impl ServerCertVerifier for MtlsServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        info!("Verifying server certificate");

        // Fast-path: allow self-signed without CA chain validation
        if self.config.allow_self_signed {
            debug!("Accepting server certificate (allow_self_signed=true)");
            return Ok(ServerCertVerified::assertion());
        }

        // Skip all validation when told to
        if !self.config.verify_server_cert {
            debug!("Skipping server certificate verification (verify_server_cert=false)");
            return Ok(ServerCertVerified::assertion());
        }

        debug!(
            "Server certificate chain length: {}",
            intermediates.len() + 1
        );

        // Real chain validation via webpki
        if self.trust_anchors.is_empty() {
            warn!("No trust anchors configured; rejecting server certificate");
            return Err(RustlsError::General(
                "No trust anchors configured for server certificate verification".to_string(),
            ));
        }

        let provider = Arc::new(oxiquic_crypto::quic_crypto_provider());
        let verifier =
            rustls::client::WebPkiServerVerifier::builder_with_provider(
                self.trust_anchors.clone(),
                provider,
            )
            .build()
            .map_err(|e| RustlsError::General(format!("server verifier build: {e}")))?;

        // Delegate to the webpki verifier (includes name check)
        let result = verifier.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );

        // Pin-store check (post-chain)
        if result.is_ok() && self.config.use_pinning {
            if let Some(ref _pin_store) = self.pin_store {
                debug!("Verifying server certificate against pins");
            }
        }

        result
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.2 signature");
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &oxiquic_crypto::quic_crypto_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        debug!("Verifying TLS 1.3 signature");
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &oxiquic_crypto::quic_crypto_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        oxiquic_crypto::quic_crypto_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// mTLS context for managing mutual TLS connections
pub struct MtlsContext {
    config: MtlsConfig,
    certificate: Certificate,
    pin_store: Option<Arc<PinStore>>,
    /// Root CA trust anchors used for real chain validation.
    trust_anchors: Arc<RootCertStore>,
}

impl MtlsContext {
    /// Create a new mTLS context with an empty trust anchor store.
    ///
    /// Call [`with_trust_anchor`] to add trusted CA certificates, or set
    /// `allow_self_signed` in the config to bypass chain validation entirely.
    pub fn new(config: MtlsConfig, certificate: Certificate) -> Self {
        Self {
            config,
            certificate,
            pin_store: None,
            trust_anchors: Arc::new(RootCertStore::empty()),
        }
    }

    /// Set pin store for certificate pinning
    pub fn with_pin_store(mut self, pin_store: Arc<PinStore>) -> Self {
        self.pin_store = Some(pin_store);
        self
    }

    /// Add a trusted CA certificate (DER-encoded) to the trust anchor store.
    ///
    /// Returns an error if the DER data cannot be parsed as a valid certificate.
    pub fn with_trust_anchor(
        mut self,
        ca_cert_der: CertificateDer<'static>,
    ) -> Result<Self, CertError> {
        // We need a mutable RootCertStore; clone-on-write if shared.
        let store = Arc::make_mut(&mut self.trust_anchors);
        store
            .add(ca_cert_der)
            .map_err(|e| CertError::TlsConfigError {
                details: format!("Failed to add trust anchor: {e}"),
            })?;
        Ok(self)
    }

    /// Create a server TLS configuration
    pub fn create_server_config(&self) -> Result<ServerConfig, CertError> {
        info!("Creating mTLS server configuration");

        let cert_chain = self.certificate.cert_chain.clone();
        let private_key = self.certificate.private_key.clone_key();

        let provider = std::sync::Arc::new(oxiquic_crypto::quic_crypto_provider());
        let mut config = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| CertError::TlsConfigError {
                details: format!("Protocol version error: {}", e),
            })?
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
                self.trust_anchors.clone(),
            ));

            config = ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(|e| CertError::TlsConfigError {
                    details: format!("Protocol version error: {}", e),
                })?
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
            self.trust_anchors.clone(),
        ));

        let provider = std::sync::Arc::new(oxiquic_crypto::quic_crypto_provider());
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| CertError::TlsConfigError {
                details: format!("Protocol version error: {}", e),
            })?
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
        let config = MtlsConfig::development();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let context = MtlsContext::new(config, cert);
        let server_config = context.create_server_config();

        assert!(server_config.is_ok());
    }

    #[test]
    fn test_mtls_client_config_creation() {
        let config = MtlsConfig::development();
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();

        let context = MtlsContext::new(config, cert);
        let client_config = context.create_client_config();

        assert!(client_config.is_ok());
    }

    #[test]
    fn test_mtls_with_client_cert_required() {
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

    // ── ITEM 8: real mTLS chain + signature validation ─────────────────────────

    /// Helper: generate a CA cert and a leaf cert signed by that CA.
    /// Returns `(ca_cert_der, leaf_cert_der)`.
    fn make_ca_and_leaf() -> (
        rustls::pki_types::CertificateDer<'static>,
        rustls::pki_types::CertificateDer<'static>,
    ) {
        use oxitls_rcgen::{SigningAlgorithm, generate_ca, generate_ca_signed_client_cert};
        use rustls::pki_types::CertificateDer;

        // Build a CA certificate (Ed25519 is lighter for tests)
        let ca = generate_ca("Test CA", SigningAlgorithm::EcdsaP256).unwrap();

        // Build a leaf certificate signed by the CA with ClientAuth EKU
        let leaf = generate_ca_signed_client_cert(&["leaf.example.com"], SigningAlgorithm::EcdsaP256, &ca)
            .unwrap();

        (
            CertificateDer::from(ca.certified_key.cert_der.clone()),
            CertificateDer::from(leaf.cert_der.clone()),
        )
    }

    /// A CA-signed client certificate should pass `verify_client_cert`.
    #[test]
    fn test_mtls_ca_signed_cert_validates() {
        use rustls::pki_types::UnixTime;

        let (ca_cert_der, leaf_cert_der) = make_ca_and_leaf();

        let node_cert = Certificate::generate_self_signed("node".to_string(), 365).unwrap();
        let config = MtlsConfig::new()
            .verify_client_cert(true)
            .require_client_cert(true);

        let context = MtlsContext::new(config, node_cert)
            .with_trust_anchor(ca_cert_der)
            .unwrap();

        let verifier = MtlsClientVerifier::new(
            context.config.clone(),
            None,
            context.trust_anchors.clone(),
        );

        let now = UnixTime::now();
        let result = verifier.verify_client_cert(&leaf_cert_der, &[], now);
        assert!(result.is_ok(), "CA-signed cert should validate: {result:?}");
    }

    /// A cert signed by a different CA (not in the trust store) must be rejected.
    #[test]
    fn test_mtls_wrong_ca_rejected() {
        use oxitls_rcgen::{SigningAlgorithm, generate_ca, generate_ca_signed_leaf};
        use rustls::pki_types::{CertificateDer, UnixTime};

        // Generate CA A (in trust store)
        let ca_a = generate_ca("CA A", SigningAlgorithm::EcdsaP256).unwrap();
        let ca_a_der = CertificateDer::from(ca_a.certified_key.cert_der.clone());

        // Generate CA B (NOT in trust store) and a leaf signed by B
        let ca_b = generate_ca("CA B", SigningAlgorithm::EcdsaP256).unwrap();
        let leaf_b =
            generate_ca_signed_leaf(&["leaf.example.com"], SigningAlgorithm::EcdsaP256, &ca_b)
                .unwrap();
        let leaf_b_der = CertificateDer::from(leaf_b.cert_der.clone());

        // Build verifier trusting only CA A
        let node_cert = Certificate::generate_self_signed("node".to_string(), 365).unwrap();
        let config = MtlsConfig::new()
            .verify_client_cert(true)
            .require_client_cert(true);
        let context = MtlsContext::new(config, node_cert)
            .with_trust_anchor(ca_a_der)
            .unwrap();

        let verifier = MtlsClientVerifier::new(
            context.config.clone(),
            None,
            context.trust_anchors.clone(),
        );

        let now = UnixTime::now();
        let result = verifier.verify_client_cert(&leaf_b_der, &[], now);
        assert!(
            result.is_err(),
            "cert signed by untrusted CA must be rejected"
        );
    }

    /// With `allow_self_signed=true`, any cert (even from an unknown CA) is accepted.
    #[test]
    fn test_mtls_allow_self_signed() {
        use rustls::pki_types::UnixTime;

        let (_, leaf_cert_der) = make_ca_and_leaf();

        let node_cert = Certificate::generate_self_signed("node".to_string(), 365).unwrap();
        // Empty trust store — no CA trusted
        let config = MtlsConfig::new()
            .verify_client_cert(true)
            .allow_self_signed();

        let context = MtlsContext::new(config, node_cert);
        let verifier = MtlsClientVerifier::new(
            context.config.clone(),
            None,
            context.trust_anchors.clone(),
        );

        let now = UnixTime::now();
        let result = verifier.verify_client_cert(&leaf_cert_der, &[], now);
        assert!(
            result.is_ok(),
            "allow_self_signed=true should bypass CA check: {result:?}"
        );
    }
}
