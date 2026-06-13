//! Certificate Authority (CA) Integration
//!
//! Provides certificate authority integration for verifying certificate chains
//! and managing trust anchors. Includes support for certificate revocation
//! checking via CRL (Certificate Revocation Lists) and OCSP (Online Certificate
//! Status Protocol).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rustls::pki_types::{CertificateDer, TrustAnchor};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::CertError;

/// CA certificate information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaCertInfo {
    /// CA common name
    pub common_name: String,
    /// CA certificate fingerprint (SHA-256 hex)
    pub fingerprint: String,
    /// When the CA cert was added
    pub added_at: SystemTime,
    /// Whether this CA is trusted
    pub trusted: bool,
}

/// Certificate revocation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevocationStatus {
    /// Certificate is valid and not revoked
    Valid,
    /// Certificate is revoked
    Revoked {
        /// When the certificate was revoked
        revoked_at: SystemTime,
    },
    /// Revocation status unknown (OCSP/CRL unavailable)
    Unknown,
}

/// Revocation check method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevocationCheckMethod {
    /// Use CRL (Certificate Revocation List)
    Crl,
    /// Use OCSP (Online Certificate Status Protocol)
    Ocsp,
    /// Try OCSP first, fall back to CRL
    OcspThenCrl,
    /// No revocation checking
    None,
}

/// CA configuration
#[derive(Debug, Clone)]
pub struct CaConfig {
    /// Revocation check method
    pub revocation_check: RevocationCheckMethod,
    /// OCSP timeout
    pub ocsp_timeout: Duration,
    /// CRL cache duration
    pub crl_cache_duration: Duration,
    /// Allow expired CRLs (not recommended for production)
    pub allow_expired_crls: bool,
}

impl Default for CaConfig {
    fn default() -> Self {
        Self {
            revocation_check: RevocationCheckMethod::OcspThenCrl,
            ocsp_timeout: Duration::from_secs(10),
            crl_cache_duration: Duration::from_secs(3600),
            allow_expired_crls: false,
        }
    }
}

impl CaConfig {
    /// Create a new CA configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set revocation check method
    pub fn with_revocation_check(mut self, method: RevocationCheckMethod) -> Self {
        self.revocation_check = method;
        self
    }

    /// Set OCSP timeout
    pub fn with_ocsp_timeout(mut self, timeout: Duration) -> Self {
        self.ocsp_timeout = timeout;
        self
    }

    /// Set CRL cache duration
    pub fn with_crl_cache_duration(mut self, duration: Duration) -> Self {
        self.crl_cache_duration = duration;
        self
    }

    /// Preset for production (strict revocation checking)
    pub fn production() -> Self {
        Self {
            revocation_check: RevocationCheckMethod::OcspThenCrl,
            ocsp_timeout: Duration::from_secs(10),
            crl_cache_duration: Duration::from_secs(3600),
            allow_expired_crls: false,
        }
    }

    /// Preset for development (no revocation checking)
    pub fn development() -> Self {
        Self {
            revocation_check: RevocationCheckMethod::None,
            ocsp_timeout: Duration::from_secs(10),
            crl_cache_duration: Duration::from_secs(3600),
            allow_expired_crls: true,
        }
    }
}

/// CRL (Certificate Revocation List) entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrlEntry {
    /// Certificate serial number
    pub serial_number: Vec<u8>,
    /// Revocation date
    pub revoked_at: SystemTime,
    /// Revocation reason (optional)
    pub reason: Option<String>,
}

/// CRL cache entry
#[derive(Debug, Clone)]
struct CachedCrl {
    /// CRL entries
    entries: Vec<CrlEntry>,
    /// When the CRL was fetched (reserved for future use)
    _fetched_at: SystemTime,
    /// CRL expiry time
    expires_at: SystemTime,
}

/// Certificate Authority manager
#[derive(Debug)]
pub struct CertificateAuthority {
    /// CA configuration
    config: CaConfig,
    /// Trust anchors (root CA certificates)
    trust_anchors: Arc<RwLock<Vec<TrustAnchor<'static>>>>,
    /// CA certificate info indexed by fingerprint
    ca_info: Arc<RwLock<HashMap<String, CaCertInfo>>>,
    /// CRL cache indexed by issuer fingerprint
    crl_cache: Arc<RwLock<HashMap<String, CachedCrl>>>,
}

impl CertificateAuthority {
    /// Create a new certificate authority manager
    pub fn new(config: CaConfig) -> Self {
        Self {
            config,
            trust_anchors: Arc::new(RwLock::new(Vec::new())),
            ca_info: Arc::new(RwLock::new(HashMap::new())),
            crl_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a trusted CA certificate
    pub async fn add_ca_cert(&self, cert: &CertificateDer<'_>) -> Result<String, CertError> {
        use x509_parser::prelude::*;

        // Parse certificate to extract info
        let (_, parsed_cert) =
            X509Certificate::from_der(cert.as_ref()).map_err(|e| CertError::ValidationFailed {
                reason: format!("Failed to parse CA certificate: {}", e),
            })?;

        // Get common name
        let common_name = parsed_cert
            .subject()
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap_or("Unknown CA")
            .to_string();

        // Calculate fingerprint (SHA-256 of DER)
        let fingerprint = {
            let hash = oxicrypto_hash::Sha256.hash_fixed(cert.as_ref());
            hex::encode(hash)
        };

        info!("Adding CA certificate: {} ({})", common_name, fingerprint);

        // Create CA info
        let ca_info = CaCertInfo {
            common_name: common_name.clone(),
            fingerprint: fingerprint.clone(),
            added_at: SystemTime::now(),
            trusted: true,
        };

        // Add to CA info map
        let mut ca_info_lock = self.ca_info.write().await;
        ca_info_lock.insert(fingerprint.clone(), ca_info);

        // Parse as trust anchor
        let trust_anchor = Self::cert_to_trust_anchor(cert)?;

        // Add to trust anchors
        let mut trust_anchors = self.trust_anchors.write().await;
        trust_anchors.push(trust_anchor);

        info!("CA certificate added successfully: {}", common_name);

        Ok(fingerprint)
    }

    /// Remove a CA certificate by fingerprint
    pub async fn remove_ca_cert(&self, fingerprint: &str) -> Result<bool, CertError> {
        let mut ca_info_lock = self.ca_info.write().await;

        if ca_info_lock.remove(fingerprint).is_some() {
            info!("Removed CA certificate: {}", fingerprint);
            // Note: We don't remove from trust_anchors as it's hard to identify
            // In production, consider rebuilding the trust_anchors list
            Ok(true)
        } else {
            warn!("CA certificate not found: {}", fingerprint);
            Ok(false)
        }
    }

    /// Get all CA certificates
    pub async fn get_ca_certs(&self) -> Vec<CaCertInfo> {
        let ca_info = self.ca_info.read().await;
        ca_info.values().cloned().collect()
    }

    /// Check certificate revocation status
    pub async fn check_revocation(
        &self,
        cert: &CertificateDer<'_>,
        _issuer: Option<&CertificateDer<'_>>,
    ) -> Result<RevocationStatus, CertError> {
        match self.config.revocation_check {
            RevocationCheckMethod::None => {
                debug!("Revocation checking disabled");
                Ok(RevocationStatus::Valid)
            }
            RevocationCheckMethod::Ocsp => self.check_ocsp(cert).await,
            RevocationCheckMethod::Crl => self.check_crl(cert).await,
            RevocationCheckMethod::OcspThenCrl => {
                // Try OCSP first
                match self.check_ocsp(cert).await {
                    Ok(status) => Ok(status),
                    Err(_) => {
                        debug!("OCSP failed, falling back to CRL");
                        self.check_crl(cert).await
                    }
                }
            }
        }
    }

    /// Check revocation via OCSP
    ///
    /// Extracts the OCSP responder URL from the certificate's Authority
    /// Information Access extension (OID 1.3.6.1.5.5.7.1.1) and returns
    /// `RevocationStatus::Unknown` with a log of the URL.
    ///
    /// Full OCSP request/response requires ASN.1 DER encoding that has no
    /// suitable pure-Rust workspace dep yet; the URL-extraction scaffold is
    /// the deliverable and the building block for a future implementation.
    async fn check_ocsp(&self, cert: &CertificateDer<'_>) -> Result<RevocationStatus, CertError> {
        use x509_parser::prelude::*;

        // AIA extension OID
        const OID_AIA: &str = "1.3.6.1.5.5.7.1.1";
        // OCSP access method OID
        const OID_OCSP: &str = "1.3.6.1.5.5.7.48.1";

        let (_, parsed_cert) =
            X509Certificate::from_der(cert.as_ref()).map_err(|e| CertError::ValidationFailed {
                reason: format!("Failed to parse certificate for OCSP check: {}", e),
            })?;

        // Walk extensions looking for Authority Information Access
        let ocsp_url: Option<String> = parsed_cert
            .extensions()
            .iter()
            .find(|ext| ext.oid.to_id_string() == OID_AIA)
            .and_then(|ext| {
                // The AIA extension value is a SEQUENCE OF AccessDescription.
                // Parse with x509_parser's AuthorityInfoAccess helper.
                if let ParsedExtension::AuthorityInfoAccess(aia) = ext.parsed_extension() {
                    aia.accessdescs
                        .iter()
                        .find(|desc| desc.access_method.to_id_string() == OID_OCSP)
                        .and_then(|desc| {
                            if let GeneralName::URI(uri) = &desc.access_location {
                                Some(uri.to_string())
                            } else {
                                None
                            }
                        })
                } else {
                    None
                }
            });

        match ocsp_url {
            None => {
                debug!("No OCSP URL found in certificate AIA extension; status unknown");
                Ok(RevocationStatus::Unknown)
            }
            Some(url) => {
                // URL extracted — full OCSP request/response encoding requires a
                // dedicated crate (e.g. ocsp-stapling) not yet in the workspace.
                // Log the URL and return Unknown rather than skipping revocation
                // silently or panicking on an unimplemented path.
                debug!(
                    "OCSP responder URL found: {}; request encoding not yet implemented",
                    url
                );
                Ok(RevocationStatus::Unknown)
            }
        }
    }

    /// Check revocation via CRL
    async fn check_crl(&self, cert: &CertificateDer<'_>) -> Result<RevocationStatus, CertError> {
        use x509_parser::prelude::*;

        // Parse certificate to get serial number
        let (_, parsed_cert) =
            X509Certificate::from_der(cert.as_ref()).map_err(|e| CertError::ValidationFailed {
                reason: format!("Failed to parse certificate: {}", e),
            })?;

        let serial_number = parsed_cert.serial.to_bytes_be();

        // Compute issuer fingerprint from the raw issuer DER bytes
        let issuer_bytes = parsed_cert.issuer().as_raw();
        let issuer_fingerprint = {
            let hash = oxicrypto_hash::Sha256.hash_fixed(issuer_bytes);
            hex::encode(hash)
        };

        let crl_cache = self.crl_cache.read().await;

        if let Some(cached_crl) = crl_cache.get(&issuer_fingerprint) {
            // Check if CRL is expired
            if !self.config.allow_expired_crls && SystemTime::now() > cached_crl.expires_at {
                debug!("CRL expired, need to fetch new one");
                drop(crl_cache);
                return self.fetch_and_check_crl(cert, &serial_number).await;
            }

            // Check if certificate is in CRL
            for entry in &cached_crl.entries {
                if entry.serial_number == serial_number {
                    warn!("Certificate is revoked");
                    return Ok(RevocationStatus::Revoked {
                        revoked_at: entry.revoked_at,
                    });
                }
            }

            debug!("Certificate not in CRL (valid)");
            Ok(RevocationStatus::Valid)
        } else {
            debug!("No cached CRL, fetching");
            drop(crl_cache);
            self.fetch_and_check_crl(cert, &serial_number).await
        }
    }

    /// Fetch CRL and check certificate
    async fn fetch_and_check_crl(
        &self,
        cert: &CertificateDer<'_>,
        serial_number: &[u8],
    ) -> Result<RevocationStatus, CertError> {
        use x509_parser::prelude::*;

        // Step 1: Parse certificate and extract CRL distribution point URIs
        let (_, parsed_cert) =
            X509Certificate::from_der(cert.as_ref()).map_err(|e| CertError::ValidationFailed {
                reason: format!("CRL: cert parse: {e}"),
            })?;

        let mut crl_uris: Vec<String> = Vec::new();
        for ext in parsed_cert.extensions() {
            if let ParsedExtension::CRLDistributionPoints(cdps) = ext.parsed_extension() {
                for dp in cdps.iter() {
                    if let Some(DistributionPointName::FullName(names)) = &dp.distribution_point {
                        for name in names.iter() {
                            if let GeneralName::URI(uri) = name {
                                crl_uris.push((*uri).to_string());
                            }
                        }
                    }
                }
            }
        }

        if crl_uris.is_empty() {
            debug!("No CRL distribution points found in certificate; status unknown");
            return Ok(RevocationStatus::Unknown);
        }

        // Step 2: Compute issuer fingerprint from raw issuer bytes
        let issuer_bytes = parsed_cert.issuer().as_raw();
        let issuer_fingerprint = {
            let hash = oxicrypto_hash::Sha256.hash_fixed(issuer_bytes);
            hex::encode(hash)
        };

        // Step 3: Fetch CRL bytes from the first working distribution point
        let raw_crl_bytes = self.fetch_crl_bytes(&crl_uris).await?;

        // Step 4: Parse CRL with x509_parser
        let crl = CertificateRevocationList::from_der(&raw_crl_bytes)
            .map_err(|e| CertError::ValidationFailed {
                reason: format!("CRL parse failed: {e}"),
            })
            .map(|(_, crl)| crl)?;

        // Step 5: Build revocation entry list from the parsed CRL
        let mut entries: Vec<CrlEntry> = Vec::new();
        for revoked in crl.iter_revoked_certificates() {
            let revoked_at = {
                let secs = revoked.revocation_date.timestamp();
                if secs >= 0 {
                    SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
                } else {
                    SystemTime::UNIX_EPOCH
                }
            };
            let reason = revoked.reason_code().map(|(_, code)| format!("{:?}", code));
            entries.push(CrlEntry {
                serial_number: revoked.raw_serial().to_vec(),
                revoked_at,
                reason,
            });
        }

        // Determine expiry from nextUpdate field
        let expires_at = crl
            .next_update()
            .map(|t| {
                let secs = t.timestamp();
                if secs >= 0 {
                    SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
                } else {
                    SystemTime::now() + self.config.crl_cache_duration
                }
            })
            .unwrap_or_else(|| SystemTime::now() + self.config.crl_cache_duration);

        let cached = CachedCrl {
            entries,
            _fetched_at: SystemTime::now(),
            expires_at,
        };

        // Step 6: Check if the serial number is in the revoked list before caching
        let revoked_entry = cached
            .entries
            .iter()
            .find(|e| e.serial_number == serial_number)
            .map(|e| e.revoked_at);

        // Cache the CRL under the issuer fingerprint key
        {
            let mut crl_cache = self.crl_cache.write().await;
            crl_cache.insert(issuer_fingerprint, cached);
        }

        match revoked_entry {
            Some(revoked_at) => {
                warn!("Certificate is revoked (found in fetched CRL)");
                Ok(RevocationStatus::Revoked { revoked_at })
            }
            None => {
                debug!("Certificate not found in CRL (valid)");
                Ok(RevocationStatus::Valid)
            }
        }
    }

    /// Fetch raw CRL bytes from a list of distribution point URLs.
    ///
    /// Tries each URL in order and returns the first successful response.
    async fn fetch_crl_bytes(&self, crl_uris: &[String]) -> Result<Vec<u8>, CertError> {
        use oxihttp_client::Client;

        let http = Client::builder()
            .with_webpki_roots()
            .build_https()
            .map_err(|e| CertError::TlsConfigError {
                details: format!("CRL HTTP client build failed: {e}"),
            })?;

        let mut last_err: Option<CertError> = None;
        for uri in crl_uris {
            debug!("Fetching CRL from: {}", uri);
            match http
                .get(uri.as_str())
                .map_err(|e| CertError::ValidationFailed {
                    reason: format!("CRL GET build error: {e}"),
                }) {
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
                Ok(req) => match req.send().await {
                    Err(e) => {
                        debug!("CRL fetch failed for {}: {}", uri, e);
                        last_err = Some(CertError::ValidationFailed {
                            reason: format!("CRL GET send error for {uri}: {e}"),
                        });
                        continue;
                    }
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            debug!("CRL fetch returned non-success status from {}", uri);
                            last_err = Some(CertError::ValidationFailed {
                                reason: format!("CRL GET returned {} from {uri}", resp.status()),
                            });
                            continue;
                        }
                        let bytes =
                            resp.body_bytes()
                                .await
                                .map_err(|e| CertError::ValidationFailed {
                                    reason: format!("CRL body read error: {e}"),
                                })?;
                        return Ok(bytes.to_vec());
                    }
                },
            }
        }

        Err(last_err.unwrap_or_else(|| CertError::ValidationFailed {
            reason: "No CRL distribution point URIs available".to_string(),
        }))
    }

    /// Clear CRL cache
    pub async fn clear_crl_cache(&self) {
        let mut crl_cache = self.crl_cache.write().await;
        crl_cache.clear();
        info!("CRL cache cleared");
    }

    /// Get CRL cache size
    pub async fn crl_cache_size(&self) -> usize {
        let crl_cache = self.crl_cache.read().await;
        crl_cache.len()
    }

    /// Convert certificate to trust anchor
    fn cert_to_trust_anchor(cert: &CertificateDer<'_>) -> Result<TrustAnchor<'static>, CertError> {
        use x509_parser::prelude::*;

        let (_, parsed_cert) =
            X509Certificate::from_der(cert.as_ref()).map_err(|e| CertError::ValidationFailed {
                reason: format!("Failed to parse certificate: {}", e),
            })?;

        // Extract subject
        let subject = parsed_cert.subject().as_raw().to_vec();

        // Extract SPKI
        let spki = parsed_cert.public_key().raw.to_vec();

        // Extract name constraints (if any) — OID 2.5.29.30
        let name_constraints: Option<rustls::pki_types::Der<'static>> = parsed_cert
            .extensions()
            .iter()
            .find(|ext| ext.oid.to_id_string() == "2.5.29.30")
            .map(|ext| rustls::pki_types::Der::from(ext.value.to_vec()));

        Ok(TrustAnchor {
            subject: subject.into(),
            subject_public_key_info: spki.into(),
            name_constraints,
        })
    }

    /// Get trust anchors
    pub async fn trust_anchors(&self) -> Vec<TrustAnchor<'static>> {
        let anchors = self.trust_anchors.read().await;
        anchors.clone()
    }

    /// Get CA count
    pub async fn ca_count(&self) -> usize {
        let ca_info = self.ca_info.read().await;
        ca_info.len()
    }
}

impl Default for CertificateAuthority {
    fn default() -> Self {
        Self::new(CaConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certs::Certificate;

    #[tokio::test]
    async fn test_ca_creation() {
        let config = CaConfig::new();
        let ca = CertificateAuthority::new(config);

        assert_eq!(ca.ca_count().await, 0);
        assert_eq!(ca.crl_cache_size().await, 0);
    }

    #[tokio::test]
    async fn test_add_ca_cert() {
        let ca = CertificateAuthority::new(CaConfig::new());

        // Generate a test certificate
        let cert = Certificate::generate_self_signed("Test CA".to_string(), 365).unwrap();

        let fingerprint = ca.add_ca_cert(&cert.cert_chain[0]).await.unwrap();

        assert!(!fingerprint.is_empty());
        assert_eq!(ca.ca_count().await, 1);
    }

    #[tokio::test]
    async fn test_remove_ca_cert() {
        let ca = CertificateAuthority::new(CaConfig::new());

        let cert = Certificate::generate_self_signed("Test CA".to_string(), 365).unwrap();
        let fingerprint = ca.add_ca_cert(&cert.cert_chain[0]).await.unwrap();

        assert_eq!(ca.ca_count().await, 1);

        let removed = ca.remove_ca_cert(&fingerprint).await.unwrap();
        assert!(removed);
        assert_eq!(ca.ca_count().await, 0);
    }

    #[tokio::test]
    async fn test_get_ca_certs() {
        let ca = CertificateAuthority::new(CaConfig::new());

        let cert = Certificate::generate_self_signed("Test CA".to_string(), 365).unwrap();
        ca.add_ca_cert(&cert.cert_chain[0]).await.unwrap();

        let ca_certs = ca.get_ca_certs().await;
        assert_eq!(ca_certs.len(), 1);
        assert!(ca_certs[0].trusted);
    }

    #[tokio::test]
    async fn test_revocation_check_disabled() {
        let config = CaConfig::new().with_revocation_check(RevocationCheckMethod::None);
        let ca = CertificateAuthority::new(config);

        let cert = Certificate::generate_self_signed("Test".to_string(), 365).unwrap();

        let status = ca
            .check_revocation(&cert.cert_chain[0], None)
            .await
            .unwrap();
        assert_eq!(status, RevocationStatus::Valid);
    }

    #[tokio::test]
    async fn test_config_presets() {
        let prod = CaConfig::production();
        assert_eq!(prod.revocation_check, RevocationCheckMethod::OcspThenCrl);
        assert!(!prod.allow_expired_crls);

        let dev = CaConfig::development();
        assert_eq!(dev.revocation_check, RevocationCheckMethod::None);
        assert!(dev.allow_expired_crls);
    }

    #[tokio::test]
    async fn test_clear_crl_cache() {
        let ca = CertificateAuthority::new(CaConfig::new());

        assert_eq!(ca.crl_cache_size().await, 0);

        ca.clear_crl_cache().await;

        assert_eq!(ca.crl_cache_size().await, 0);
    }

    #[test]
    fn test_revocation_status() {
        let valid = RevocationStatus::Valid;
        assert_eq!(valid, RevocationStatus::Valid);

        let revoked = RevocationStatus::Revoked {
            revoked_at: SystemTime::now(),
        };
        assert!(matches!(revoked, RevocationStatus::Revoked { .. }));

        let unknown = RevocationStatus::Unknown;
        assert_eq!(unknown, RevocationStatus::Unknown);
    }

    #[test]
    fn test_crl_entry() {
        let entry = CrlEntry {
            serial_number: vec![1, 2, 3, 4],
            revoked_at: SystemTime::now(),
            reason: Some("Key compromise".to_string()),
        };

        assert_eq!(entry.serial_number, vec![1, 2, 3, 4]);
        assert!(entry.reason.is_some());
    }

    // ── ITEM 6: CRL cache-based revocation checks ─────────────────────────────

    /// Pre-populate the CRL cache and verify that a certificate whose serial
    /// appears in the cache is returned as Revoked.
    #[tokio::test]
    async fn test_crl_cache_revoked_cert() {
        use x509_parser::prelude::*;

        let ca = CertificateAuthority::new(
            CaConfig::new().with_revocation_check(RevocationCheckMethod::Crl),
        );

        // Generate a test certificate to query
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();
        let cert_der = &cert.cert_chain[0];

        // Extract the serial number and issuer fingerprint the same way the
        // production code does.
        let (_, parsed) = X509Certificate::from_der(cert_der.as_ref()).unwrap();
        let serial_number = parsed.serial.to_bytes_be();
        let issuer_bytes = parsed.issuer().as_raw();
        let issuer_fingerprint = {
            let hash = oxicrypto_hash::Sha256.hash_fixed(issuer_bytes);
            hex::encode(hash)
        };

        // Seed the CRL cache with a synthetic entry that marks this serial as revoked.
        let revoked_at = SystemTime::now();
        let synthetic_crl = CachedCrl {
            entries: vec![CrlEntry {
                serial_number: serial_number.clone(),
                revoked_at,
                reason: Some("keyCompromise".to_string()),
            }],
            _fetched_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(3600),
        };
        {
            let mut cache = ca.crl_cache.write().await;
            cache.insert(issuer_fingerprint.clone(), synthetic_crl);
        }

        let status = ca.check_revocation(cert_der, None).await.unwrap();
        assert!(
            matches!(status, RevocationStatus::Revoked { .. }),
            "expected Revoked, got {status:?}"
        );
    }

    /// Same setup but the serial is NOT in the cache → should return Valid.
    #[tokio::test]
    async fn test_crl_cache_valid_cert() {
        use x509_parser::prelude::*;

        let ca = CertificateAuthority::new(
            CaConfig::new().with_revocation_check(RevocationCheckMethod::Crl),
        );

        let cert = Certificate::generate_self_signed("clean-node".to_string(), 365).unwrap();
        let cert_der = &cert.cert_chain[0];

        let (_, parsed) = X509Certificate::from_der(cert_der.as_ref()).unwrap();
        let issuer_bytes = parsed.issuer().as_raw();
        let issuer_fingerprint = {
            let hash = oxicrypto_hash::Sha256.hash_fixed(issuer_bytes);
            hex::encode(hash)
        };

        // Cache contains a *different* serial
        let synthetic_crl = CachedCrl {
            entries: vec![CrlEntry {
                serial_number: vec![0xDE, 0xAD, 0xBE, 0xEF],
                revoked_at: SystemTime::now(),
                reason: None,
            }],
            _fetched_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(3600),
        };
        {
            let mut cache = ca.crl_cache.write().await;
            cache.insert(issuer_fingerprint.clone(), synthetic_crl);
        }

        let status = ca.check_revocation(cert_der, None).await.unwrap();
        assert_eq!(status, RevocationStatus::Valid);
    }

    // ── ITEM 7: NameConstraints parsing ───────────────────────────────────────

    /// A plain self-signed certificate has no NameConstraints extension;
    /// `cert_to_trust_anchor` should produce `name_constraints: None`.
    #[test]
    fn test_cert_to_trust_anchor_no_name_constraints() {
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();
        let trust_anchor = CertificateAuthority::cert_to_trust_anchor(&cert.cert_chain[0]).unwrap();
        assert!(
            trust_anchor.name_constraints.is_none(),
            "expected no NameConstraints for plain self-signed cert"
        );
    }

    /// Generate a certificate that carries a raw NameConstraints DER blob and
    /// verify that `cert_to_trust_anchor` picks it up.
    #[test]
    fn test_cert_to_trust_anchor_with_name_constraints() {
        use oxitls_rcgen::keypair::OxiEcdsaP256Key;
        use rcgen::{CertificateParams, CustomExtension};
        use rustls::pki_types::CertificateDer;

        // OID 2.5.29.30 — NameConstraints
        // Minimal DER: SEQUENCE { permittedSubtrees [0] { SEQUENCE { SEQUENCE {
        //   [2] dNSName "example.com" } } } }
        let name_constraints_value: Vec<u8> = vec![
            0x30, 0x13, // SEQUENCE (19 bytes) — outer NameConstraints
            0xa0, 0x11, // [0] permittedSubtrees (17 bytes)
            0x30, 0x0f, // SEQUENCE (15 bytes) — GeneralSubtree
            0x30, 0x0d, // SEQUENCE (13 bytes)
            0x82, 0x0b, // [2] dNSName (11 bytes)
            b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm',
        ];

        let nc_oid = vec![2u64, 5, 29, 30];
        let custom_ext = CustomExtension::from_oid_content(&nc_oid, name_constraints_value.clone());

        let mut params = CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        params.custom_extensions.push(custom_ext);
        params.serial_number = Some(rcgen::SerialNumber::from_slice(&[1, 2, 3, 4]));
        let key = OxiEcdsaP256Key::generate().unwrap();
        let cert_obj = params.self_signed(&key).unwrap();
        let cert_der = CertificateDer::from(cert_obj.der().to_vec());

        let trust_anchor = CertificateAuthority::cert_to_trust_anchor(&cert_der).unwrap();
        assert!(
            trust_anchor.name_constraints.is_some(),
            "expected NameConstraints to be present"
        );
    }
}
