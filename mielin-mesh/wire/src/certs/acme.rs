//! ACME (Let's Encrypt) Integration
//!
//! Provides automatic certificate provisioning and renewal using the ACME protocol.
//! Supports both HTTP-01 and DNS-01 challenge types for domain validation.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base64::prelude::*;
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    RetryPolicy,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::{CertError, CertInfo, Certificate};

/// ACME challenge type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcmeChallengeType {
    /// HTTP-01 challenge (requires web server on port 80)
    Http01,
    /// DNS-01 challenge (requires DNS record creation)
    Dns01,
}

/// ACME account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeAccount {
    /// Account credentials (PEM format)
    pub credentials: String,
    /// Account contact email
    pub contact: Vec<String>,
    /// Account creation timestamp
    pub created_at: SystemTime,
}

/// ACME client configuration
#[derive(Debug, Clone)]
pub struct AcmeConfig {
    /// Contact emails for account registration
    pub contact_emails: Vec<String>,
    /// Challenge type to use
    pub challenge_type: AcmeChallengeType,
    /// Use Let's Encrypt staging environment (for testing)
    pub use_staging: bool,
    /// Automatic renewal threshold (days before expiry)
    pub renewal_threshold_days: u32,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            contact_emails: Vec::new(),
            challenge_type: AcmeChallengeType::Http01,
            use_staging: false,
            renewal_threshold_days: 30,
        }
    }
}

impl AcmeConfig {
    /// Create a new ACME configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a contact email
    pub fn with_email(mut self, email: String) -> Self {
        self.contact_emails.push(email);
        self
    }

    /// Set challenge type
    pub fn with_challenge_type(mut self, challenge_type: AcmeChallengeType) -> Self {
        self.challenge_type = challenge_type;
        self
    }

    /// Use staging environment
    pub fn with_staging(mut self) -> Self {
        self.use_staging = true;
        self
    }

    /// Set renewal threshold
    pub fn with_renewal_threshold(mut self, days: u32) -> Self {
        self.renewal_threshold_days = days;
        self
    }
}

/// Challenge validation callback
pub trait ChallengeValidator: Send + Sync {
    /// Validate HTTP-01 challenge
    ///
    /// The implementation should make the key authorization available at:
    /// `http://<domain>/.well-known/acme-challenge/<token>`
    fn validate_http01(
        &self,
        domain: &str,
        token: &str,
        key_authorization: &str,
    ) -> Result<(), CertError>;

    /// Validate DNS-01 challenge
    ///
    /// The implementation should create a TXT record at:
    /// `_acme-challenge.<domain>` with value: `<key_authorization_digest>`
    fn validate_dns01(
        &self,
        domain: &str,
        txt_record: &str,
        key_authorization_digest: &str,
    ) -> Result<(), CertError>;

    /// Cleanup HTTP-01 challenge
    fn cleanup_http01(&self, domain: &str, token: &str) -> Result<(), CertError>;

    /// Cleanup DNS-01 challenge
    fn cleanup_dns01(&self, domain: &str, txt_record: &str) -> Result<(), CertError>;
}

/// ACME client for automatic certificate management
pub struct AcmeClient {
    /// ACME configuration
    config: AcmeConfig,
    /// ACME account
    account: Arc<RwLock<Option<Account>>>,
    /// Challenge validator
    validator: Option<Arc<dyn ChallengeValidator>>,
}

impl AcmeClient {
    /// Create a new ACME client
    pub fn new(config: AcmeConfig) -> Self {
        Self {
            config,
            account: Arc::new(RwLock::new(None)),
            validator: None,
        }
    }

    /// Set challenge validator
    pub fn with_validator(mut self, validator: Arc<dyn ChallengeValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Initialize ACME account
    pub async fn initialize_account(&self) -> Result<(), CertError> {
        let mut account_lock = self.account.write().await;

        if account_lock.is_some() {
            return Ok(());
        }

        info!("Initializing ACME account");

        let url = if self.config.use_staging {
            LetsEncrypt::Staging.url()
        } else {
            LetsEncrypt::Production.url()
        };

        let contact_strs: Vec<_> = self
            .config
            .contact_emails
            .iter()
            .map(|email| format!("mailto:{}", email))
            .collect();
        let contact_refs: Vec<&str> = contact_strs.iter().map(|s| s.as_str()).collect();

        let (account, _credentials) = Account::builder()
            .map_err(|e| {
                CertError::GenerationFailed(format!("ACME account builder failed: {}", e))
            })?
            .create(
                &NewAccount {
                    contact: &contact_refs,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                url.to_string(),
                None,
            )
            .await
            .map_err(|e| {
                CertError::GenerationFailed(format!("ACME account creation failed: {}", e))
            })?;

        info!("ACME account created successfully");
        *account_lock = Some(account);
        Ok(())
    }

    /// Request a certificate for the given domains
    pub async fn request_certificate(
        &self,
        domains: Vec<String>,
    ) -> Result<Certificate, CertError> {
        self.initialize_account().await?;

        let account_lock = self.account.read().await;
        let account = account_lock
            .as_ref()
            .ok_or_else(|| CertError::GenerationFailed("No ACME account".to_string()))?;

        info!("Requesting certificate for domains: {:?}", domains);

        // Create identifiers
        let identifiers: Vec<_> = domains.iter().map(|d| Identifier::Dns(d.clone())).collect();

        // Create new order
        let mut order = account
            .new_order(&NewOrder::new(&identifiers))
            .await
            .map_err(|e| CertError::GenerationFailed(format!("Order creation failed: {}", e)))?;

        info!("ACME order created: {:?}", order.state().status);

        // Process authorizations
        let mut authorizations = order.authorizations();

        while let Some(authz_result) = authorizations.next().await {
            let mut authz = authz_result.map_err(|e| {
                CertError::GenerationFailed(format!("Failed to get authorization: {}", e))
            })?;

            match authz.status {
                AuthorizationStatus::Valid => {
                    debug!("Authorization already valid");
                    continue;
                }
                AuthorizationStatus::Pending => {
                    // Continue processing
                }
                _ => {
                    return Err(CertError::GenerationFailed(format!(
                        "Unexpected authorization status: {:?}",
                        authz.status
                    )));
                }
            }

            let challenge_type = match self.config.challenge_type {
                AcmeChallengeType::Http01 => ChallengeType::Http01,
                AcmeChallengeType::Dns01 => ChallengeType::Dns01,
            };

            let challenge_error_msg = format!("{:?} challenge not available", challenge_type);

            let mut challenge = authz
                .challenge(challenge_type)
                .ok_or(CertError::GenerationFailed(challenge_error_msg))?;

            let domain_identifier = challenge.identifier();
            let domain = domain_identifier.to_string();
            info!("Processing authorization for domain: {}", domain);

            let key_auth = challenge.key_authorization();
            let key_authorization = key_auth.as_str();

            self.validate_challenge(&domain, &challenge, key_authorization)
                .await?;

            challenge.set_ready().await.map_err(|e| {
                CertError::GenerationFailed(format!("Failed to set challenge ready: {}", e))
            })?;

            info!("Challenge completed for domain: {}", domain);
        }

        // Wait for order to be ready using integrated polling
        let retry_policy = RetryPolicy::default();
        let _order_state = order.poll_ready(&retry_policy).await.map_err(|e| {
            CertError::GenerationFailed(format!("Failed to poll order ready: {}", e))
        })?;

        info!("Order ready for finalization");

        // Generate key pair and CSR for the ACME order
        let key_pair = rcgen::KeyPair::generate().map_err(|e| {
            CertError::GenerationFailed(format!("Key pair generation failed: {}", e))
        })?;
        let key_pair_der = key_pair.serialize_der();

        let mut params = rcgen::CertificateParams::new(domains.clone()).map_err(|e| {
            CertError::GenerationFailed(format!("CertificateParams creation failed: {}", e))
        })?;
        params.distinguished_name = rcgen::DistinguishedName::new();

        let csr = params
            .serialize_request(&key_pair)
            .map_err(|e| CertError::GenerationFailed(format!("CSR serialization failed: {}", e)))?
            .der()
            .to_vec();

        // Finalize order with custom CSR
        order.finalize_csr(&csr).await.map_err(|e| {
            CertError::GenerationFailed(format!("Order finalization failed: {}", e))
        })?;

        // Wait for certificate using integrated polling
        let retry_policy = RetryPolicy::default();
        let cert_chain_pem = order.poll_certificate(&retry_policy).await.map_err(|e| {
            CertError::GenerationFailed(format!("Failed to poll certificate: {}", e))
        })?;

        info!("Certificate issued");

        // Parse certificate chain
        let mut cert_reader = std::io::BufReader::new(cert_chain_pem.as_bytes());
        let cert_chain: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CertError::EncodingError {
                details: format!("Failed to parse certificate chain: {}", e),
            })?;

        // Get private key from the key pair we generated
        let private_key =
            PrivateKeyDer::try_from(key_pair_der).map_err(|e| CertError::KeyError {
                details: format!("Private key conversion failed: {:?}", e),
            })?;

        // Extract certificate info from first cert in chain
        let validity_days = 90; // Let's Encrypt certificates are valid for 90 days
        let info = CertInfo {
            common_name: domains[0].clone(),
            subject_alt_names: domains,
            validity_days,
            created_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(validity_days as u64 * 86400),
        };

        info!("Certificate successfully obtained from ACME");

        Ok(Certificate {
            cert_chain,
            private_key,
            info,
        })
    }

    /// Validate a challenge
    async fn validate_challenge(
        &self,
        domain: &str,
        challenge: &instant_acme::Challenge,
        key_authorization: &str,
    ) -> Result<(), CertError> {
        let validator = self.validator.as_ref().ok_or_else(|| {
            CertError::GenerationFailed("No challenge validator configured".to_string())
        })?;

        match challenge.r#type {
            ChallengeType::Http01 => {
                let token = &challenge.token;
                info!(
                    "Setting up HTTP-01 challenge for {} (token: {})",
                    domain, token
                );
                validator.validate_http01(domain, token, key_authorization)?;
            }
            ChallengeType::Dns01 => {
                // For DNS-01, we need to create a TXT record with the base64url-encoded SHA256 hash
                let digest =
                    ring::digest::digest(&ring::digest::SHA256, key_authorization.as_bytes());
                let digest_b64 = BASE64_URL_SAFE_NO_PAD.encode(digest.as_ref());
                let txt_record = format!("_acme-challenge.{}", domain);
                info!(
                    "Setting up DNS-01 challenge for {} (record: {}, value: {})",
                    domain, txt_record, digest_b64
                );
                validator.validate_dns01(domain, &txt_record, &digest_b64)?;
            }
            _ => {
                return Err(CertError::GenerationFailed(format!(
                    "Unsupported challenge type: {:?}",
                    challenge.r#type
                )));
            }
        }

        // Wait for DNS propagation or HTTP server to be ready
        tokio::time::sleep(Duration::from_secs(2)).await;

        Ok(())
    }

    /// Renew certificate if needed
    pub async fn renew_if_needed(
        &self,
        current_cert: &Certificate,
        domains: Vec<String>,
    ) -> Result<Option<Certificate>, CertError> {
        if !current_cert
            .info
            .should_rotate_with_threshold(self.config.renewal_threshold_days)
        {
            debug!("Certificate does not need renewal yet");
            return Ok(None);
        }

        info!("Certificate needs renewal, requesting new certificate");
        let new_cert = self.request_certificate(domains).await?;
        Ok(Some(new_cert))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acme_config_creation() {
        let config = AcmeConfig::new()
            .with_email("admin@example.com".to_string())
            .with_challenge_type(AcmeChallengeType::Http01)
            .with_staging();

        assert_eq!(config.contact_emails.len(), 1);
        assert_eq!(config.challenge_type, AcmeChallengeType::Http01);
        assert!(config.use_staging);
    }

    #[test]
    fn test_acme_challenge_types() {
        assert_eq!(AcmeChallengeType::Http01, AcmeChallengeType::Http01);
        assert_ne!(AcmeChallengeType::Http01, AcmeChallengeType::Dns01);
    }

    #[tokio::test]
    async fn test_acme_client_creation() {
        let config = AcmeConfig::new().with_staging();
        let client = AcmeClient::new(config);

        assert!(client.account.read().await.is_none());
    }
}
