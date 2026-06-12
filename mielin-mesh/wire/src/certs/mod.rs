//! Certificate Management for Mesh Network
//!
//! Handles TLS certificates for secure QUIC connections between mesh nodes.
//! Supports self-signed certificates and Let's Encrypt (ACME) integration.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;

pub mod acme;
pub mod ca;
pub mod mtls;
pub mod pinning;
pub mod renewal;
pub mod storage;

/// Certificate errors
#[derive(Debug, Error)]
pub enum CertError {
    #[error("Failed to generate certificate: {0}")]
    GenerationFailed(String),

    #[error("Certificate expired on {expired_at:?} (now: {now:?})")]
    Expired {
        expired_at: SystemTime,
        now: SystemTime,
    },

    #[error("Certificate not found for {identifier}")]
    NotFound { identifier: String },

    #[error("Invalid certificate: {reason}")]
    Invalid { reason: String },

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Certificate validation failed: {reason}")]
    ValidationFailed { reason: String },

    #[error("Key serialization/deserialization failed: {details}")]
    KeyError { details: String },

    #[error("Certificate chain verification failed: {reason}")]
    ChainVerificationFailed { reason: String },

    #[error("Subject alternative name (SAN) mismatch: expected {expected}, got {actual}")]
    SanMismatch { expected: String, actual: String },

    #[error("Certificate rotation failed after {attempts} attempts: {last_error}")]
    RotationFailed { attempts: usize, last_error: String },

    #[error("TLS configuration error: {details}")]
    TlsConfigError { details: String },

    #[error("Certificate encoding error: {details}")]
    EncodingError { details: String },
}

/// Certificate information
#[derive(Debug, Clone)]
pub struct CertInfo {
    /// Common name (node ID or hostname)
    pub common_name: String,

    /// Subject alternative names (SANs)
    pub subject_alt_names: Vec<String>,

    /// Certificate validity period in days
    pub validity_days: u32,

    /// Creation timestamp
    pub created_at: SystemTime,

    /// Expiration timestamp
    pub expires_at: SystemTime,
}

impl CertInfo {
    /// Create a new certificate info
    pub fn new(common_name: String, validity_days: u32) -> Self {
        let created_at = SystemTime::now();
        let expires_at = created_at + Duration::from_secs(validity_days as u64 * 86400);

        Self {
            common_name,
            subject_alt_names: Vec::new(),
            validity_days,
            created_at,
            expires_at,
        }
    }

    /// Add a subject alternative name
    pub fn with_san(mut self, san: String) -> Self {
        self.subject_alt_names.push(san);
        self
    }

    /// Check if certificate is expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// Validate certificate and return error if expired
    pub fn validate_not_expired(&self) -> Result<(), CertError> {
        if self.is_expired() {
            return Err(CertError::Expired {
                expired_at: self.expires_at,
                now: SystemTime::now(),
            });
        }
        Ok(())
    }

    /// Get time until expiration
    pub fn time_until_expiry(&self) -> Option<Duration> {
        self.expires_at.duration_since(SystemTime::now()).ok()
    }

    /// Check if certificate should be rotated (expires in < 30 days)
    pub fn should_rotate(&self) -> bool {
        self.should_rotate_with_threshold(30)
    }

    /// Check if certificate should be rotated with custom threshold
    pub fn should_rotate_with_threshold(&self, threshold_days: u32) -> bool {
        self.time_until_expiry()
            .map(|d| d < Duration::from_secs(threshold_days as u64 * 86400))
            .unwrap_or(true)
    }

    /// Validate certificate info
    pub fn validate(&self) -> Result<(), CertError> {
        // Check not expired
        self.validate_not_expired()?;

        // Check common name is not empty
        if self.common_name.is_empty() {
            return Err(CertError::Invalid {
                reason: "Common name cannot be empty".to_string(),
            });
        }

        // Check validity period is reasonable
        if self.validity_days == 0 {
            return Err(CertError::Invalid {
                reason: "Validity period must be > 0 days".to_string(),
            });
        }

        if self.validity_days > 825 {
            // Max 825 days per CA/Browser Forum
            return Err(CertError::ValidationFailed {
                reason: format!(
                    "Validity period {} days exceeds maximum 825 days",
                    self.validity_days
                ),
            });
        }

        Ok(())
    }
}

/// Certificate with private key
pub struct Certificate {
    /// Certificate chain (typically just one cert for self-signed)
    pub cert_chain: Vec<CertificateDer<'static>>,

    /// Private key
    pub private_key: PrivateKeyDer<'static>,

    /// Certificate metadata
    pub info: CertInfo,
}

impl Certificate {
    /// Generate a self-signed certificate
    ///
    /// # Arguments
    ///
    /// * `common_name` - Common name for the certificate (e.g., node ID)
    /// * `validity_days` - Number of days the certificate is valid
    pub fn generate_self_signed(
        common_name: String,
        validity_days: u32,
    ) -> Result<Self, CertError> {
        Self::generate_self_signed_with_sans(common_name, validity_days, vec![])
    }

    /// Generate a self-signed certificate with additional SANs
    pub fn generate_self_signed_with_sans(
        common_name: String,
        validity_days: u32,
        additional_sans: Vec<String>,
    ) -> Result<Self, CertError> {
        // Validate inputs
        if common_name.is_empty() {
            return Err(CertError::Invalid {
                reason: "Common name cannot be empty".to_string(),
            });
        }

        if validity_days == 0 || validity_days > 825 {
            return Err(CertError::Invalid {
                reason: format!("Validity days {} must be between 1 and 825", validity_days),
            });
        }

        // Create subject alternative names including the common name
        let mut subject_alt_names = vec![common_name.clone()];

        // Add additional SANs
        for san in additional_sans {
            if !subject_alt_names.contains(&san) {
                subject_alt_names.push(san);
            }
        }

        // Add localhost for testing
        if !subject_alt_names.contains(&"localhost".to_string()) {
            subject_alt_names.push("localhost".to_string());
        }
        if !subject_alt_names.contains(&"127.0.0.1".to_string()) {
            subject_alt_names.push("127.0.0.1".to_string());
        }

        // Generate the certificate using Pure-Rust oxitls-rcgen (ECDSA P-256).
        let san_strs: Vec<&str> = subject_alt_names.iter().map(String::as_str).collect();
        let ck = oxitls_rcgen::generate_self_signed_p256(&san_strs)
            .map_err(|e| CertError::GenerationFailed(e.to_string()))?;

        // Wire cert_der / pkcs8_der into rustls types.
        let cert_der = CertificateDer::from(ck.cert_der);
        let key_der = PrivateKeyDer::try_from(ck.pkcs8_der).map_err(|e| CertError::KeyError {
            details: format!("Key serialization failed: {:?}", e),
        })?;

        let info = CertInfo {
            common_name,
            subject_alt_names,
            validity_days,
            created_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(validity_days as u64 * 86400),
        };

        Ok(Self {
            cert_chain: vec![cert_der],
            private_key: key_der,
            info,
        })
    }

    /// Check if certificate is expired
    pub fn is_expired(&self) -> bool {
        self.info.is_expired()
    }

    /// Validate certificate is not expired
    pub fn validate_not_expired(&self) -> Result<(), CertError> {
        self.info.validate_not_expired()
    }

    /// Validate certificate
    pub fn validate(&self) -> Result<(), CertError> {
        self.info.validate()?;

        // Additional certificate-level validations
        if self.cert_chain.is_empty() {
            return Err(CertError::Invalid {
                reason: "Certificate chain is empty".to_string(),
            });
        }

        Ok(())
    }

    /// Check if certificate should be rotated
    pub fn should_rotate(&self) -> bool {
        self.info.should_rotate()
    }

    /// Check if a hostname/IP matches the certificate SANs
    pub fn matches_host(&self, host: &str) -> bool {
        self.info
            .subject_alt_names
            .iter()
            .any(|san| san == host || san == "*")
    }
}

/// Certificate manager for mesh nodes
pub struct CertManager {
    /// Current certificate
    current_cert: Arc<tokio::sync::RwLock<Option<Certificate>>>,

    /// Certificate rotation threshold (days before expiry)
    rotation_threshold_days: u32,
}

impl CertManager {
    /// Create a new certificate manager
    pub fn new() -> Self {
        Self {
            current_cert: Arc::new(tokio::sync::RwLock::new(None)),
            rotation_threshold_days: 30,
        }
    }

    /// Set rotation threshold in days
    pub fn with_rotation_threshold(mut self, days: u32) -> Self {
        self.rotation_threshold_days = days;
        self
    }

    /// Get or generate a certificate for the given node ID
    pub async fn get_or_generate_cert(&self, node_id: &str) -> Result<Arc<Certificate>, CertError> {
        let mut cert_lock = self.current_cert.write().await;

        // Check if we have a valid certificate
        if let Some(ref cert) = *cert_lock {
            if !cert.should_rotate() {
                return Ok(Arc::new(Certificate {
                    cert_chain: cert.cert_chain.clone(),
                    private_key: cert.private_key.clone_key(),
                    info: cert.info.clone(),
                }));
            }
        }

        // Generate new certificate
        let cert = Certificate::generate_self_signed(node_id.to_string(), 365)?;
        let cert_arc = Arc::new(Certificate {
            cert_chain: cert.cert_chain.clone(),
            private_key: cert.private_key.clone_key(),
            info: cert.info.clone(),
        });

        *cert_lock = Some(cert);
        Ok(cert_arc)
    }

    /// Force certificate rotation
    pub async fn rotate_cert(&self, node_id: &str) -> Result<Arc<Certificate>, CertError> {
        self.rotate_cert_with_retry(node_id, 3).await
    }

    /// Force certificate rotation with retry logic
    pub async fn rotate_cert_with_retry(
        &self,
        node_id: &str,
        max_attempts: usize,
    ) -> Result<Arc<Certificate>, CertError> {
        let mut last_error = None;

        for attempt in 0..max_attempts {
            let mut cert_lock = self.current_cert.write().await;

            match Certificate::generate_self_signed(node_id.to_string(), 365) {
                Ok(cert) => {
                    let cert_arc = Arc::new(Certificate {
                        cert_chain: cert.cert_chain.clone(),
                        private_key: cert.private_key.clone_key(),
                        info: cert.info.clone(),
                    });

                    *cert_lock = Some(cert);
                    return Ok(cert_arc);
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    if attempt < max_attempts - 1 {
                        drop(cert_lock);
                        // Exponential backoff
                        tokio::time::sleep(Duration::from_millis(
                            100 * (2_u64.pow(attempt as u32)),
                        ))
                        .await;
                    }
                }
            }
        }

        Err(CertError::RotationFailed {
            attempts: max_attempts,
            last_error: last_error.unwrap_or_else(|| "Unknown error".to_string()),
        })
    }

    /// Get certificate info without generating
    pub async fn get_cert_info(&self) -> Option<CertInfo> {
        let cert_lock = self.current_cert.read().await;
        cert_lock.as_ref().map(|c| c.info.clone())
    }

    /// Check if certificate needs rotation
    pub async fn needs_rotation(&self) -> bool {
        let cert_lock = self.current_cert.read().await;
        cert_lock
            .as_ref()
            .map(|c| c.should_rotate())
            .unwrap_or(true)
    }
}

impl Default for CertManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_cert_generation() {
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365);
        assert!(cert.is_ok());

        let cert = cert.unwrap();
        assert_eq!(cert.info.common_name, "test-node");
        assert_eq!(cert.info.validity_days, 365);
        assert!(!cert.is_expired());
        assert!(!cert.should_rotate());
    }

    #[test]
    fn test_cert_info() {
        let info = CertInfo::new("test-node".to_string(), 365);
        assert!(!info.is_expired());
        assert!(!info.should_rotate());

        let time_left = info.time_until_expiry();
        assert!(time_left.is_some());
        assert!(time_left.unwrap() > Duration::from_secs(300 * 86400)); // More than 300 days
    }

    #[test]
    fn test_short_validity_rotation() {
        let info = CertInfo::new("test-node".to_string(), 20); // 20 days
        assert!(info.should_rotate()); // Should rotate if < 30 days left
    }

    #[tokio::test]
    async fn test_cert_manager() {
        let manager = CertManager::new();

        // Generate certificate
        let cert = manager.get_or_generate_cert("test-node").await;
        assert!(cert.is_ok());

        let cert = cert.unwrap();
        assert_eq!(cert.info.common_name, "test-node");

        // Get same certificate again
        let cert2 = manager.get_or_generate_cert("test-node").await.unwrap();
        assert_eq!(cert2.info.common_name, cert.info.common_name);
    }

    #[tokio::test]
    async fn test_cert_rotation() {
        let manager = CertManager::new();

        // Generate initial certificate
        let cert1 = manager.get_or_generate_cert("test-node").await.unwrap();
        let created1 = cert1.info.created_at;

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Force rotation
        let cert2 = manager.rotate_cert("test-node").await.unwrap();
        let created2 = cert2.info.created_at;

        // Should be different timestamps
        assert!(created2 > created1);
    }

    #[tokio::test]
    async fn test_needs_rotation() {
        let manager = CertManager::new();

        // Initially needs rotation (no cert)
        assert!(manager.needs_rotation().await);

        // After generation, shouldn't need rotation
        manager.get_or_generate_cert("test-node").await.unwrap();
        assert!(!manager.needs_rotation().await);
    }
}
