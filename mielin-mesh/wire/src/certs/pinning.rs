//! Certificate Pinning
//!
//! Provides certificate and public key pinning for enhanced security.
//! Supports HPKP-style (HTTP Public Key Pinning) pin verification and rotation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use oxicrypto_hash::{Sha256, Sha384, Sha512};
use rustls::pki_types::CertificateDer;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::CertError;

/// Pin type (what to pin)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinType {
    /// Pin the entire certificate (more strict, breaks on renewal)
    Certificate,
    /// Pin the public key (survives certificate renewal with same key)
    PublicKey,
    /// Pin the Subject Public Key Info (SPKI) - recommended for HPKP
    SubjectPublicKeyInfo,
}

/// Hash algorithm for pins
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinHashAlgorithm {
    /// SHA-256 (recommended)
    Sha256,
    /// SHA-384
    Sha384,
    /// SHA-512
    Sha512,
}

impl PinHashAlgorithm {
    /// Compute the digest of `data` for this algorithm, returning the raw hash bytes.
    fn digest_bytes(&self, data: &[u8]) -> Vec<u8> {
        match self {
            PinHashAlgorithm::Sha256 => Sha256.hash_fixed(data).to_vec(),
            PinHashAlgorithm::Sha384 => Sha384.hash_fixed(data).to_vec(),
            PinHashAlgorithm::Sha512 => Sha512.hash_fixed(data).to_vec(),
        }
    }

    /// Get the expected hash length in bytes
    pub fn hash_length(&self) -> usize {
        match self {
            PinHashAlgorithm::Sha256 => 32,
            PinHashAlgorithm::Sha384 => 48,
            PinHashAlgorithm::Sha512 => 64,
        }
    }
}

/// Certificate pin
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    /// Pin identifier (peer ID or domain)
    pub identifier: String,
    /// Pin type
    pub pin_type: PinType,
    /// Hash algorithm
    pub hash_algorithm: PinHashAlgorithm,
    /// Pin hash (hex-encoded)
    pub hash: String,
    /// Pin creation time
    pub created_at: SystemTime,
    /// Pin expiration time (optional)
    pub expires_at: Option<SystemTime>,
    /// Whether this is a backup pin (for rotation)
    pub is_backup: bool,
}

impl Pin {
    /// Create a new pin
    pub fn new(
        identifier: String,
        pin_type: PinType,
        hash_algorithm: PinHashAlgorithm,
        hash: String,
    ) -> Self {
        Self {
            identifier,
            pin_type,
            hash_algorithm,
            hash,
            created_at: SystemTime::now(),
            expires_at: None,
            is_backup: false,
        }
    }

    /// Set expiration time
    pub fn with_expiration(mut self, expires_at: SystemTime) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set expiration duration from now
    pub fn with_expiration_duration(mut self, duration: Duration) -> Self {
        self.expires_at = Some(SystemTime::now() + duration);
        self
    }

    /// Mark as backup pin
    pub fn as_backup(mut self) -> Self {
        self.is_backup = true;
        self
    }

    /// Check if pin is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| SystemTime::now() > exp)
            .unwrap_or(false)
    }

    /// Validate pin format
    pub fn validate(&self) -> Result<(), CertError> {
        // Check hash length
        let expected_len = self.hash_algorithm.hash_length() * 2; // Hex encoding doubles length
        if self.hash.len() != expected_len {
            return Err(CertError::ValidationFailed {
                reason: format!(
                    "Invalid pin hash length: expected {} hex chars, got {}",
                    expected_len,
                    self.hash.len()
                ),
            });
        }

        // Check if hash is valid hex
        if !self.hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CertError::ValidationFailed {
                reason: "Pin hash contains invalid hex characters".to_string(),
            });
        }

        // Check identifier is not empty
        if self.identifier.is_empty() {
            return Err(CertError::ValidationFailed {
                reason: "Pin identifier cannot be empty".to_string(),
            });
        }

        Ok(())
    }
}

/// Pin verification result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinVerificationResult {
    /// Pin matched
    Matched {
        /// Which pin matched
        pin_hash: String,
        /// Whether it was a backup pin
        is_backup: bool,
    },
    /// No pins found for this identifier
    NoPins { identifier: String },
    /// Pins found but none matched
    NoMatch {
        identifier: String,
        expected_pins: Vec<String>,
        actual_hash: String,
    },
    /// Pin expired
    Expired {
        identifier: String,
        pin_hash: String,
    },
}

impl PinVerificationResult {
    /// Check if verification succeeded
    pub fn is_success(&self) -> bool {
        matches!(self, PinVerificationResult::Matched { .. })
    }
}

/// HPKP-style header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpkpHeader {
    /// Pin hashes (base64-encoded SHA-256 of SPKI)
    pub pins: Vec<String>,
    /// Max age in seconds
    pub max_age: u64,
    /// Include subdomains
    pub include_subdomains: bool,
    /// Report URI (optional)
    pub report_uri: Option<String>,
}

impl HpkpHeader {
    /// Create a new HPKP header
    pub fn new(pins: Vec<String>, max_age: u64) -> Self {
        Self {
            pins,
            max_age,
            include_subdomains: false,
            report_uri: None,
        }
    }

    /// Include subdomains
    pub fn with_subdomains(mut self) -> Self {
        self.include_subdomains = true;
        self
    }

    /// Set report URI
    pub fn with_report_uri(mut self, uri: String) -> Self {
        self.report_uri = Some(uri);
        self
    }

    /// Parse HPKP header from string
    pub fn parse(header: &str) -> Result<Self, CertError> {
        let mut pins = Vec::new();
        let mut max_age = 0;
        let mut include_subdomains = false;
        let mut report_uri = None;

        for directive in header.split(';') {
            let directive = directive.trim();

            if directive.starts_with("pin-sha256=") {
                let pin = directive
                    .trim_start_matches("pin-sha256=\"")
                    .trim_end_matches('"');
                pins.push(pin.to_string());
            } else if directive.starts_with("max-age=") {
                max_age = directive
                    .trim_start_matches("max-age=")
                    .parse()
                    .map_err(|_| CertError::ValidationFailed {
                        reason: "Invalid max-age value".to_string(),
                    })?;
            } else if directive == "includeSubDomains" {
                include_subdomains = true;
            } else if directive.starts_with("report-uri=") {
                let uri = directive
                    .trim_start_matches("report-uri=\"")
                    .trim_end_matches('"');
                report_uri = Some(uri.to_string());
            }
        }

        if pins.is_empty() {
            return Err(CertError::ValidationFailed {
                reason: "No pins found in HPKP header".to_string(),
            });
        }

        Ok(Self {
            pins,
            max_age,
            include_subdomains,
            report_uri,
        })
    }

    /// Format as HPKP header string
    pub fn format(&self) -> String {
        let mut parts = Vec::new();

        for pin in &self.pins {
            parts.push(format!("pin-sha256=\"{}\"", pin));
        }

        parts.push(format!("max-age={}", self.max_age));

        if self.include_subdomains {
            parts.push("includeSubDomains".to_string());
        }

        if let Some(ref uri) = self.report_uri {
            parts.push(format!("report-uri=\"{}\"", uri));
        }

        parts.join("; ")
    }
}

/// Pin store for managing certificate pins
#[derive(Debug)]
pub struct PinStore {
    /// Pins indexed by identifier
    pins: Arc<RwLock<HashMap<String, Vec<Pin>>>>,
}

impl PinStore {
    /// Create a new pin store
    pub fn new() -> Self {
        Self {
            pins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a pin
    pub async fn add_pin(&self, pin: Pin) -> Result<(), CertError> {
        pin.validate()?;

        let mut pins = self.pins.write().await;
        pins.entry(pin.identifier.clone())
            .or_insert_with(Vec::new)
            .push(pin);

        Ok(())
    }

    /// Remove a pin
    pub async fn remove_pin(&self, identifier: &str, hash: &str) -> Result<bool, CertError> {
        let mut pins = self.pins.write().await;

        if let Some(pin_list) = pins.get_mut(identifier) {
            let original_len = pin_list.len();
            pin_list.retain(|p| p.hash != hash);
            Ok(pin_list.len() < original_len)
        } else {
            Ok(false)
        }
    }

    /// Get pins for an identifier
    pub async fn get_pins(&self, identifier: &str) -> Vec<Pin> {
        let pins = self.pins.read().await;
        pins.get(identifier).cloned().unwrap_or_default()
    }

    /// Remove expired pins
    pub async fn cleanup_expired(&self) -> usize {
        let mut pins = self.pins.write().await;
        let mut removed_count = 0;

        for pin_list in pins.values_mut() {
            let original_len = pin_list.len();
            pin_list.retain(|p| !p.is_expired());
            removed_count += original_len - pin_list.len();
        }

        removed_count
    }

    /// Verify a certificate against stored pins
    pub async fn verify_certificate(
        &self,
        identifier: &str,
        cert_chain: &[CertificateDer<'_>],
    ) -> Result<PinVerificationResult, CertError> {
        let pins = self.get_pins(identifier).await;

        if pins.is_empty() {
            return Ok(PinVerificationResult::NoPins {
                identifier: identifier.to_string(),
            });
        }

        // Remove expired pins first
        let valid_pins: Vec<_> = pins.iter().filter(|p| !p.is_expired()).collect();

        if valid_pins.is_empty() {
            return Ok(PinVerificationResult::Expired {
                identifier: identifier.to_string(),
                pin_hash: pins.first().map(|p| p.hash.clone()).unwrap_or_default(),
            });
        }

        // Try to match any certificate in the chain
        for cert in cert_chain {
            for pin in &valid_pins {
                let cert_hash =
                    Self::compute_hash(cert.as_ref(), &pin.pin_type, &pin.hash_algorithm)?;

                if cert_hash == pin.hash {
                    info!(
                        "Certificate pin matched for '{}' (backup: {})",
                        identifier, pin.is_backup
                    );
                    return Ok(PinVerificationResult::Matched {
                        pin_hash: pin.hash.clone(),
                        is_backup: pin.is_backup,
                    });
                }
            }
        }

        // No match found
        let expected_pins: Vec<String> = valid_pins.iter().map(|p| p.hash.clone()).collect();
        let actual_hash = if let Some(first_cert) = cert_chain.first() {
            let pin_type = valid_pins
                .first()
                .map(|p| p.pin_type)
                .unwrap_or(PinType::SubjectPublicKeyInfo);
            let hash_algo = valid_pins
                .first()
                .map(|p| p.hash_algorithm)
                .unwrap_or(PinHashAlgorithm::Sha256);
            Self::compute_hash(first_cert.as_ref(), &pin_type, &hash_algo)?
        } else {
            String::new()
        };

        warn!(
            "Certificate pin verification failed for '{}': expected {:?}, got {}",
            identifier, expected_pins, actual_hash
        );

        Ok(PinVerificationResult::NoMatch {
            identifier: identifier.to_string(),
            expected_pins,
            actual_hash,
        })
    }

    /// Compute hash for a certificate
    fn compute_hash(
        cert_der: &[u8],
        pin_type: &PinType,
        hash_algorithm: &PinHashAlgorithm,
    ) -> Result<String, CertError> {
        let data_to_hash = match pin_type {
            PinType::Certificate => cert_der.to_vec(),
            PinType::PublicKey | PinType::SubjectPublicKeyInfo => {
                // Extract SPKI from certificate
                Self::extract_spki(cert_der)?
            }
        };

        let hash = hash_algorithm.digest_bytes(&data_to_hash);
        Ok(hex::encode(hash))
    }

    /// Extract Subject Public Key Info (SPKI) from certificate DER
    fn extract_spki(cert_der: &[u8]) -> Result<Vec<u8>, CertError> {
        use x509_parser::prelude::*;

        let (_, cert) =
            X509Certificate::from_der(cert_der).map_err(|e| CertError::ValidationFailed {
                reason: format!("Failed to parse certificate: {}", e),
            })?;

        // Get the SPKI (SubjectPublicKeyInfo)
        let spki = cert.public_key();
        Ok(spki.raw.to_vec())
    }

    /// Create a pin from a certificate
    pub async fn pin_certificate(
        &self,
        identifier: String,
        cert: &CertificateDer<'_>,
        pin_type: PinType,
        hash_algorithm: PinHashAlgorithm,
        expiration: Option<Duration>,
    ) -> Result<Pin, CertError> {
        let hash = Self::compute_hash(cert.as_ref(), &pin_type, &hash_algorithm)?;

        let mut pin = Pin::new(identifier, pin_type, hash_algorithm, hash);

        if let Some(exp) = expiration {
            pin = pin.with_expiration_duration(exp);
        }

        self.add_pin(pin.clone()).await?;

        info!("Created pin for '{}': {}", pin.identifier, pin.hash);

        Ok(pin)
    }

    /// Rotate pins (add new pin as backup, will promote later)
    pub async fn rotate_pin(
        &self,
        identifier: &str,
        new_cert: &CertificateDer<'_>,
        pin_type: PinType,
        hash_algorithm: PinHashAlgorithm,
    ) -> Result<Pin, CertError> {
        let hash = Self::compute_hash(new_cert.as_ref(), &pin_type, &hash_algorithm)?;

        let backup_pin = Pin::new(identifier.to_string(), pin_type, hash_algorithm, hash)
            .as_backup()
            .with_expiration_duration(Duration::from_secs(86400 * 60)); // 60 days

        self.add_pin(backup_pin.clone()).await?;

        info!(
            "Created backup pin for '{}': {}",
            identifier, backup_pin.hash
        );

        Ok(backup_pin)
    }

    /// Promote backup pin to primary (and optionally remove old primary)
    pub async fn promote_backup_pin(
        &self,
        identifier: &str,
        remove_old: bool,
    ) -> Result<usize, CertError> {
        let mut pins = self.pins.write().await;

        let pin_list = pins
            .get_mut(identifier)
            .ok_or_else(|| CertError::NotFound {
                identifier: identifier.to_string(),
            })?;

        // Mark all backup pins as non-backup
        let mut promoted = 0;
        for pin in pin_list.iter_mut() {
            if pin.is_backup {
                pin.is_backup = false;
                promoted += 1;
            }
        }

        // Optionally remove old (non-backup) pins
        if remove_old && promoted > 0 {
            pin_list.retain(|p| {
                let keep =
                    p.is_backup || p.created_at > SystemTime::now() - Duration::from_secs(3600);
                if !keep {
                    debug!("Removing old pin: {}", p.hash);
                }
                keep
            });
        }

        info!("Promoted {} backup pins for '{}'", promoted, identifier);

        Ok(promoted)
    }

    /// Get pin count
    pub async fn pin_count(&self) -> usize {
        let pins = self.pins.read().await;
        pins.values().map(|v| v.len()).sum()
    }

    /// Clear all pins
    pub async fn clear(&self) {
        let mut pins = self.pins.write().await;
        pins.clear();
    }
}

impl Default for PinStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certs::Certificate;


    #[test]
    fn test_pin_creation() {
        let pin = Pin::new(
            "test.example.com".to_string(),
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            "a".repeat(64),
        );

        assert_eq!(pin.identifier, "test.example.com");
        assert_eq!(pin.pin_type, PinType::SubjectPublicKeyInfo);
        assert_eq!(pin.hash_algorithm, PinHashAlgorithm::Sha256);
        assert!(!pin.is_backup);
        assert!(!pin.is_expired());
    }

    #[test]
    fn test_pin_validation() {
        let valid_pin = Pin::new(
            "test".to_string(),
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            "a".repeat(64),
        );
        assert!(valid_pin.validate().is_ok());

        let invalid_pin = Pin::new(
            "test".to_string(),
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            "xyz".to_string(),
        );
        assert!(invalid_pin.validate().is_err());
    }

    #[tokio::test]
    async fn test_pin_store() {
        let store = PinStore::new();

        let pin = Pin::new(
            "test".to_string(),
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            "a".repeat(64),
        );

        assert!(store.add_pin(pin.clone()).await.is_ok());
        assert_eq!(store.pin_count().await, 1);

        let pins = store.get_pins("test").await;
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].hash, pin.hash);
    }

    #[tokio::test]
    async fn test_pin_removal() {
        let store = PinStore::new();

        let pin = Pin::new(
            "test".to_string(),
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            "a".repeat(64),
        );

        store.add_pin(pin.clone()).await.unwrap();
        assert_eq!(store.pin_count().await, 1);

        let removed = store.remove_pin("test", &pin.hash).await.unwrap();
        assert!(removed);
        assert_eq!(store.pin_count().await, 0);
    }

    #[tokio::test]
    async fn test_expired_pin_cleanup() {
        let store = PinStore::new();

        let expired_pin = Pin::new(
            "test".to_string(),
            PinType::SubjectPublicKeyInfo,
            PinHashAlgorithm::Sha256,
            "a".repeat(64),
        )
        .with_expiration(SystemTime::now() - Duration::from_secs(3600));

        store.add_pin(expired_pin).await.unwrap();
        assert_eq!(store.pin_count().await, 1);

        let removed = store.cleanup_expired().await;
        assert_eq!(removed, 1);
        assert_eq!(store.pin_count().await, 0);
    }

    #[tokio::test]
    async fn test_certificate_pinning() {
        let store = PinStore::new();

        // Generate a test certificate
        let cert = Certificate::generate_self_signed("test.example.com".to_string(), 365).unwrap();

        // Pin the certificate
        let pin = store
            .pin_certificate(
                "test.example.com".to_string(),
                &cert.cert_chain[0],
                PinType::SubjectPublicKeyInfo,
                PinHashAlgorithm::Sha256,
                None,
            )
            .await
            .unwrap();

        assert!(!pin.hash.is_empty());

        // Verify the certificate
        let result = store
            .verify_certificate("test.example.com", &cert.cert_chain)
            .await
            .unwrap();

        assert!(result.is_success());
        if let PinVerificationResult::Matched { is_backup, .. } = result {
            assert!(!is_backup);
        } else {
            panic!("Expected Matched result");
        }
    }

    #[tokio::test]
    async fn test_pin_rotation() {
        let store = PinStore::new();

        let cert1 = Certificate::generate_self_signed("test.example.com".to_string(), 365).unwrap();
        let cert2 = Certificate::generate_self_signed("test.example.com".to_string(), 365).unwrap();

        // Pin first certificate
        store
            .pin_certificate(
                "test.example.com".to_string(),
                &cert1.cert_chain[0],
                PinType::SubjectPublicKeyInfo,
                PinHashAlgorithm::Sha256,
                None,
            )
            .await
            .unwrap();

        // Rotate to second certificate (add as backup)
        let backup_pin = store
            .rotate_pin(
                "test.example.com",
                &cert2.cert_chain[0],
                PinType::SubjectPublicKeyInfo,
                PinHashAlgorithm::Sha256,
            )
            .await
            .unwrap();

        assert!(backup_pin.is_backup);
        assert_eq!(store.pin_count().await, 2);

        // Verify both certificates work
        let result1 = store
            .verify_certificate("test.example.com", &cert1.cert_chain)
            .await
            .unwrap();
        assert!(result1.is_success());

        let result2 = store
            .verify_certificate("test.example.com", &cert2.cert_chain)
            .await
            .unwrap();
        assert!(result2.is_success());

        // Promote backup
        let promoted = store
            .promote_backup_pin("test.example.com", false)
            .await
            .unwrap();
        assert_eq!(promoted, 1);
    }

    #[test]
    fn test_hpkp_header_parse() {
        let header =
            "pin-sha256=\"abc123\"; pin-sha256=\"def456\"; max-age=2592000; includeSubDomains";
        let hpkp = HpkpHeader::parse(header).unwrap();

        assert_eq!(hpkp.pins.len(), 2);
        assert_eq!(hpkp.max_age, 2592000);
        assert!(hpkp.include_subdomains);
    }

    #[test]
    fn test_hpkp_header_format() {
        let hpkp = HpkpHeader::new(vec!["abc123".to_string(), "def456".to_string()], 2592000)
            .with_subdomains();

        let formatted = hpkp.format();
        assert!(formatted.contains("pin-sha256=\"abc123\""));
        assert!(formatted.contains("pin-sha256=\"def456\""));
        assert!(formatted.contains("max-age=2592000"));
        assert!(formatted.contains("includeSubDomains"));
    }

    #[test]
    fn test_pin_hash_lengths() {
        assert_eq!(PinHashAlgorithm::Sha256.hash_length(), 32);
        assert_eq!(PinHashAlgorithm::Sha384.hash_length(), 48);
        assert_eq!(PinHashAlgorithm::Sha512.hash_length(), 64);
    }
}
