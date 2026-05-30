//! TLS certificate types for MielinMesh security.

use super::*;

// =============================================================================
// Mutual TLS Configuration
// =============================================================================

/// Certificate data wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateData {
    /// DER-encoded certificate
    pub der: Vec<u8>,
    /// Subject common name
    pub subject_cn: String,
    /// Issuer common name
    pub issuer_cn: String,
    /// Not valid before
    pub not_before: SystemTime,
    /// Not valid after
    pub not_after: SystemTime,
    /// Serial number (hex encoded)
    pub serial: String,
}

impl CertificateData {
    /// Create new certificate data
    pub fn new(
        der: Vec<u8>,
        subject_cn: impl Into<String>,
        issuer_cn: impl Into<String>,
        not_before: SystemTime,
        not_after: SystemTime,
    ) -> Self {
        let serial = hex::encode(&der[..16.min(der.len())]);
        Self {
            der,
            subject_cn: subject_cn.into(),
            issuer_cn: issuer_cn.into(),
            not_before,
            not_after,
            serial,
        }
    }

    /// Check if certificate is currently valid
    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now();
        now >= self.not_before && now <= self.not_after
    }

    /// Check if certificate is expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.not_after
    }

    /// Get remaining validity duration
    pub fn remaining_validity(&self) -> Option<Duration> {
        self.not_after.duration_since(SystemTime::now()).ok()
    }
}

/// TLS version enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TlsVersion {
    /// TLS 1.2 (minimum supported)
    Tls12,
    /// TLS 1.3 (recommended)
    #[default]
    Tls13,
}

/// Mutual TLS configuration for mesh connections
#[derive(Debug, Clone)]
pub struct MtlsConfig {
    /// Local node's certificate
    pub node_certificate: CertificateData,
    /// Local node's private key (DER-encoded PKCS#8)
    pub node_private_key: Vec<u8>,
    /// Trusted CA certificates
    pub trusted_cas: Vec<CertificateData>,
    /// Certificate revocation list serials
    revoked_serials: HashSet<String>,
    /// Require client certificate
    pub require_client_cert: bool,
    /// Minimum TLS version (always TLS 1.3)
    pub min_tls_version: TlsVersion,
    /// ALPN protocols
    pub alpn_protocols: Vec<String>,
    /// Certificate verification depth
    pub verify_depth: usize,
}

impl MtlsConfig {
    /// Create new mTLS configuration
    pub fn new(
        node_certificate: CertificateData,
        node_private_key: Vec<u8>,
        trusted_cas: Vec<CertificateData>,
    ) -> Self {
        Self {
            node_certificate,
            node_private_key,
            trusted_cas,
            revoked_serials: HashSet::new(),
            require_client_cert: true,
            min_tls_version: TlsVersion::Tls13,
            alpn_protocols: vec!["mielin-mesh/1.0".to_string()],
            verify_depth: 3,
        }
    }

    /// Create with builder pattern
    pub fn builder() -> MtlsConfigBuilder {
        MtlsConfigBuilder::new()
    }

    /// Add CA certificate
    pub fn add_trusted_ca(&mut self, ca: CertificateData) {
        self.trusted_cas.push(ca);
    }

    /// Revoke a certificate by serial
    pub fn revoke_certificate(&mut self, serial: impl Into<String>) {
        self.revoked_serials.insert(serial.into());
    }

    /// Check if certificate is revoked
    pub fn is_revoked(&self, serial: &str) -> bool {
        self.revoked_serials.contains(serial)
    }

    /// Verify a peer certificate
    pub fn verify_peer_certificate(&self, peer_cert: &CertificateData) -> SecurityResult<()> {
        // Check revocation
        if self.is_revoked(&peer_cert.serial) {
            return Err(SecurityError::CertificateRevoked {
                serial: peer_cert.serial.clone(),
            });
        }

        // Check validity period
        let now = SystemTime::now();
        if now < peer_cert.not_before {
            return Err(SecurityError::CertificateNotYetValid {
                valid_from: peer_cert.not_before,
            });
        }
        if now > peer_cert.not_after {
            return Err(SecurityError::CertificateExpired {
                expiry: peer_cert.not_after,
            });
        }

        // Verify issuer is trusted
        let issuer_trusted = self
            .trusted_cas
            .iter()
            .any(|ca| ca.subject_cn == peer_cert.issuer_cn);
        if !issuer_trusted {
            return Err(SecurityError::CertificateChainInvalid {
                details: format!("Issuer '{}' not in trusted CAs", peer_cert.issuer_cn),
            });
        }

        Ok(())
    }

    /// Get private key bytes
    pub fn private_key_bytes(&self) -> &[u8] {
        &self.node_private_key
    }

    /// Check if local certificate needs renewal (within 30 days)
    pub fn needs_renewal(&self) -> bool {
        self.node_certificate
            .remaining_validity()
            .map(|d| d < Duration::from_secs(30 * 24 * 60 * 60))
            .unwrap_or(true)
    }
}

/// Builder for MtlsConfig
pub struct MtlsConfigBuilder {
    node_certificate: Option<CertificateData>,
    node_private_key: Option<Vec<u8>>,
    trusted_cas: Vec<CertificateData>,
    require_client_cert: bool,
    min_tls_version: TlsVersion,
    alpn_protocols: Vec<String>,
    verify_depth: usize,
}

impl MtlsConfigBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            node_certificate: None,
            node_private_key: None,
            trusted_cas: Vec::new(),
            require_client_cert: true,
            min_tls_version: TlsVersion::Tls13,
            alpn_protocols: vec!["mielin-mesh/1.0".to_string()],
            verify_depth: 3,
        }
    }

    /// Set node certificate
    pub fn certificate(mut self, cert: CertificateData) -> Self {
        self.node_certificate = Some(cert);
        self
    }

    /// Set private key
    pub fn private_key(mut self, key: Vec<u8>) -> Self {
        self.node_private_key = Some(key);
        self
    }

    /// Add trusted CA
    pub fn trusted_ca(mut self, ca: CertificateData) -> Self {
        self.trusted_cas.push(ca);
        self
    }

    /// Set client certificate requirement
    pub fn require_client_cert(mut self, require: bool) -> Self {
        self.require_client_cert = require;
        self
    }

    /// Set minimum TLS version
    pub fn min_tls_version(mut self, version: TlsVersion) -> Self {
        self.min_tls_version = version;
        self
    }

    /// Add ALPN protocol
    pub fn alpn_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.alpn_protocols.push(protocol.into());
        self
    }

    /// Set certificate verification depth
    pub fn verify_depth(mut self, depth: usize) -> Self {
        self.verify_depth = depth;
        self
    }

    /// Build the configuration
    pub fn build(self) -> SecurityResult<MtlsConfig> {
        let node_certificate =
            self.node_certificate
                .ok_or_else(|| SecurityError::ConfigurationError {
                    details: "Node certificate is required".to_string(),
                })?;

        let node_private_key =
            self.node_private_key
                .ok_or_else(|| SecurityError::ConfigurationError {
                    details: "Private key is required".to_string(),
                })?;

        Ok(MtlsConfig {
            node_certificate,
            node_private_key,
            trusted_cas: self.trusted_cas,
            revoked_serials: HashSet::new(),
            require_client_cert: self.require_client_cert,
            min_tls_version: self.min_tls_version,
            alpn_protocols: self.alpn_protocols,
            verify_depth: self.verify_depth,
        })
    }
}

impl Default for MtlsConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
