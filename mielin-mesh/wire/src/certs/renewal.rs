//! Automatic Certificate Renewal
//!
//! Provides automatic certificate renewal scheduling for both self-signed
//! and ACME certificates. Monitors certificate expiry and triggers renewal
//! before expiration.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use super::acme::AcmeClient;
use super::{CertError, Certificate};

/// Renewal event types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenewalEvent {
    /// Renewal check started
    CheckStarted {
        /// Certificate identifier (domain or node ID)
        identifier: String,
        /// Days until expiry
        days_until_expiry: u32,
    },
    /// Renewal triggered
    RenewalTriggered {
        /// Certificate identifier
        identifier: String,
        /// Days until expiry
        days_until_expiry: u32,
        /// Renewal method (ACME or self-signed)
        method: RenewalMethod,
    },
    /// Renewal succeeded
    RenewalSucceeded {
        /// Certificate identifier
        identifier: String,
        /// New certificate validity days
        validity_days: u32,
    },
    /// Renewal failed
    RenewalFailed {
        /// Certificate identifier
        identifier: String,
        /// Error message
        error: String,
        /// Retry attempt number
        attempt: usize,
    },
    /// Renewal will retry
    RenewalRetrying {
        /// Certificate identifier
        identifier: String,
        /// Retry attempt number
        attempt: usize,
        /// Next retry delay in seconds
        delay_secs: u64,
    },
    /// Renewal gave up after max retries
    RenewalGaveUp {
        /// Certificate identifier
        identifier: String,
        /// Total attempts made
        total_attempts: usize,
        /// Last error
        last_error: String,
    },
}

/// Renewal method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenewalMethod {
    /// ACME (Let's Encrypt)
    Acme,
    /// Self-signed
    SelfSigned,
}

/// Renewal strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenewalStrategy {
    /// Use ACME for renewal
    Acme {
        /// Domains to renew
        domains: Vec<String>,
    },
    /// Use self-signed for renewal
    SelfSigned {
        /// Node ID or common name
        node_id: String,
        /// Validity days for new certificate
        validity_days: u32,
    },
}

/// Renewal configuration
#[derive(Debug, Clone)]
pub struct RenewalConfig {
    /// Check interval (how often to check certificate status)
    pub check_interval: Duration,
    /// Renewal threshold in days before expiry
    pub renewal_threshold_days: u32,
    /// Maximum retry attempts for failed renewals
    pub max_retry_attempts: usize,
    /// Initial retry delay
    pub initial_retry_delay: Duration,
    /// Maximum retry delay
    pub max_retry_delay: Duration,
    /// Use exponential backoff for retries
    pub exponential_backoff: bool,
}

impl Default for RenewalConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(3600), // Check every hour
            renewal_threshold_days: 30,
            max_retry_attempts: 5,
            initial_retry_delay: Duration::from_secs(60),
            max_retry_delay: Duration::from_secs(3600),
            exponential_backoff: true,
        }
    }
}

impl RenewalConfig {
    /// Create a new renewal configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set check interval
    pub fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Set renewal threshold
    pub fn with_renewal_threshold(mut self, days: u32) -> Self {
        self.renewal_threshold_days = days;
        self
    }

    /// Set max retry attempts
    pub fn with_max_retry_attempts(mut self, max_attempts: usize) -> Self {
        self.max_retry_attempts = max_attempts;
        self
    }

    /// Preset for aggressive renewal (check every 15 minutes, renew 60 days before expiry)
    pub fn aggressive() -> Self {
        Self {
            check_interval: Duration::from_secs(900),
            renewal_threshold_days: 60,
            max_retry_attempts: 10,
            initial_retry_delay: Duration::from_secs(30),
            max_retry_delay: Duration::from_secs(1800),
            exponential_backoff: true,
        }
    }

    /// Preset for conservative renewal (check every 6 hours, renew 7 days before expiry)
    pub fn conservative() -> Self {
        Self {
            check_interval: Duration::from_secs(21600),
            renewal_threshold_days: 7,
            max_retry_attempts: 3,
            initial_retry_delay: Duration::from_secs(300),
            max_retry_delay: Duration::from_secs(7200),
            exponential_backoff: true,
        }
    }

    /// Preset for production (check every hour, renew 30 days before expiry)
    pub fn production() -> Self {
        Self::default()
    }
}

/// Certificate renewal tracker
struct RenewalTracker {
    /// Current certificate
    certificate: Option<Certificate>,
    /// Renewal strategy
    strategy: RenewalStrategy,
    /// Current retry attempt
    current_retry: usize,
    /// Last renewal attempt time
    last_attempt: Option<std::time::SystemTime>,
}

/// Automatic certificate renewal scheduler
pub struct RenewalScheduler {
    /// Renewal configuration
    config: RenewalConfig,
    /// ACME client (optional, for ACME renewals)
    acme_client: Option<Arc<AcmeClient>>,
    /// Certificate tracker
    tracker: Arc<RwLock<RenewalTracker>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<RenewalEvent>,
    /// Shutdown signal receiver
    shutdown_rx: Arc<RwLock<Option<tokio::sync::oneshot::Receiver<()>>>>,
    /// Shutdown signal sender (kept to prevent premature drop)
    _shutdown_tx: Arc<tokio::sync::oneshot::Sender<()>>,
}

impl RenewalScheduler {
    /// Create a new renewal scheduler
    pub fn new(config: RenewalConfig, strategy: RenewalStrategy) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        Self {
            config,
            acme_client: None,
            tracker: Arc::new(RwLock::new(RenewalTracker {
                certificate: None,
                strategy,
                current_retry: 0,
                last_attempt: None,
            })),
            event_tx,
            shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
            _shutdown_tx: Arc::new(shutdown_tx),
        }
    }

    /// Set ACME client for ACME renewals
    pub fn with_acme_client(mut self, client: Arc<AcmeClient>) -> Self {
        self.acme_client = Some(client);
        self
    }

    /// Set initial certificate
    pub async fn set_certificate(&self, cert: Certificate) {
        let mut tracker = self.tracker.write().await;
        tracker.certificate = Some(cert);
    }

    /// Get current certificate
    pub async fn get_certificate(&self) -> Option<Certificate> {
        let tracker = self.tracker.read().await;
        tracker.certificate.as_ref().map(|c| Certificate {
            cert_chain: c.cert_chain.clone(),
            private_key: c.private_key.clone_key(),
            info: c.info.clone(),
        })
    }

    /// Subscribe to renewal events
    pub fn subscribe(&self) -> broadcast::Receiver<RenewalEvent> {
        self.event_tx.subscribe()
    }

    /// Start the renewal scheduler
    pub async fn start(self: Arc<Self>) {
        info!("Starting certificate renewal scheduler");
        info!("  Check interval: {:?}", self.config.check_interval);
        info!(
            "  Renewal threshold: {} days",
            self.config.renewal_threshold_days
        );

        let mut check_interval = interval(self.config.check_interval);

        loop {
            tokio::select! {
                _ = check_interval.tick() => {
                    self.check_and_renew().await;
                }
                _ = self.wait_for_shutdown() => {
                    info!("Renewal scheduler shutting down");
                    break;
                }
            }
        }
    }

    /// Wait for shutdown signal
    async fn wait_for_shutdown(&self) {
        let mut shutdown_lock = self.shutdown_rx.write().await;
        if let Some(rx) = shutdown_lock.take() {
            let _ = rx.await;
        }
    }

    /// Check certificate and renew if needed
    async fn check_and_renew(&self) {
        let tracker = self.tracker.read().await;

        let cert = match &tracker.certificate {
            Some(c) => c,
            None => {
                debug!("No certificate to check");
                return;
            }
        };

        let identifier = cert.info.common_name.clone();

        // Calculate days until expiry
        let days_until_expiry = cert
            .info
            .time_until_expiry()
            .map(|d| (d.as_secs() / 86400) as u32)
            .unwrap_or(0);

        // Emit check event
        let _ = self.event_tx.send(RenewalEvent::CheckStarted {
            identifier: identifier.clone(),
            days_until_expiry,
        });

        debug!(
            "Checking certificate '{}': {} days until expiry",
            identifier, days_until_expiry
        );

        // Check if renewal is needed
        if !cert
            .info
            .should_rotate_with_threshold(self.config.renewal_threshold_days)
        {
            debug!("Certificate '{}' does not need renewal yet", identifier);
            return;
        }

        info!(
            "Certificate '{}' needs renewal ({} days until expiry)",
            identifier, days_until_expiry
        );

        drop(tracker); // Release read lock before renewal

        // Trigger renewal
        self.trigger_renewal().await;
    }

    /// Trigger certificate renewal
    async fn trigger_renewal(&self) {
        let mut tracker = self.tracker.write().await;

        let strategy = tracker.strategy.clone();
        let identifier = match &strategy {
            RenewalStrategy::Acme { domains } => domains.first().unwrap_or(&String::new()).clone(),
            RenewalStrategy::SelfSigned { node_id, .. } => node_id.clone(),
        };

        let method = match &strategy {
            RenewalStrategy::Acme { .. } => RenewalMethod::Acme,
            RenewalStrategy::SelfSigned { .. } => RenewalMethod::SelfSigned,
        };

        let days_until_expiry = tracker
            .certificate
            .as_ref()
            .and_then(|c| c.info.time_until_expiry())
            .map(|d| (d.as_secs() / 86400) as u32)
            .unwrap_or(0);

        // Emit renewal triggered event
        let _ = self.event_tx.send(RenewalEvent::RenewalTriggered {
            identifier: identifier.clone(),
            days_until_expiry,
            method,
        });

        info!("Triggering renewal for '{}' using {:?}", identifier, method);

        // Attempt renewal with retries
        for attempt in 0..self.config.max_retry_attempts {
            tracker.current_retry = attempt;
            tracker.last_attempt = Some(std::time::SystemTime::now());

            drop(tracker); // Release lock during renewal attempt

            let result = match &strategy {
                RenewalStrategy::Acme { domains } => self.renew_with_acme(domains.clone()).await,
                RenewalStrategy::SelfSigned {
                    node_id,
                    validity_days,
                } => {
                    self.renew_self_signed(node_id.clone(), *validity_days)
                        .await
                }
            };

            tracker = self.tracker.write().await;

            match result {
                Ok(new_cert) => {
                    info!(
                        "Successfully renewed certificate for '{}'",
                        new_cert.info.common_name
                    );

                    let validity_days = new_cert.info.validity_days;

                    // Update certificate
                    tracker.certificate = Some(new_cert);
                    tracker.current_retry = 0;

                    // Emit success event
                    let _ = self.event_tx.send(RenewalEvent::RenewalSucceeded {
                        identifier: identifier.clone(),
                        validity_days,
                    });

                    return;
                }
                Err(e) => {
                    error!(
                        "Renewal attempt {} failed for '{}': {}",
                        attempt + 1,
                        identifier,
                        e
                    );

                    // Emit failure event
                    let _ = self.event_tx.send(RenewalEvent::RenewalFailed {
                        identifier: identifier.clone(),
                        error: e.to_string(),
                        attempt: attempt + 1,
                    });

                    // Check if we should retry
                    if attempt + 1 < self.config.max_retry_attempts {
                        let delay = self.calculate_retry_delay(attempt);

                        // Emit retry event
                        let _ = self.event_tx.send(RenewalEvent::RenewalRetrying {
                            identifier: identifier.clone(),
                            attempt: attempt + 1,
                            delay_secs: delay.as_secs(),
                        });

                        warn!(
                            "Will retry renewal for '{}' in {:?} (attempt {}/{})",
                            identifier,
                            delay,
                            attempt + 2,
                            self.config.max_retry_attempts
                        );

                        drop(tracker); // Release lock during sleep
                        tokio::time::sleep(delay).await;
                        tracker = self.tracker.write().await;
                    } else {
                        // Give up
                        error!(
                            "Gave up renewing certificate for '{}' after {} attempts",
                            identifier, self.config.max_retry_attempts
                        );

                        let _ = self.event_tx.send(RenewalEvent::RenewalGaveUp {
                            identifier: identifier.clone(),
                            total_attempts: self.config.max_retry_attempts,
                            last_error: e.to_string(),
                        });

                        return;
                    }
                }
            }
        }
    }

    /// Renew certificate using ACME
    async fn renew_with_acme(&self, domains: Vec<String>) -> Result<Certificate, CertError> {
        let client = self
            .acme_client
            .as_ref()
            .ok_or_else(|| CertError::GenerationFailed("No ACME client configured".to_string()))?;

        client.request_certificate(domains).await
    }

    /// Renew self-signed certificate
    async fn renew_self_signed(
        &self,
        node_id: String,
        validity_days: u32,
    ) -> Result<Certificate, CertError> {
        Certificate::generate_self_signed(node_id, validity_days)
    }

    /// Calculate retry delay with optional exponential backoff
    fn calculate_retry_delay(&self, attempt: usize) -> Duration {
        if self.config.exponential_backoff {
            let delay_secs = self.config.initial_retry_delay.as_secs() * 2_u64.pow(attempt as u32);
            let delay = Duration::from_secs(delay_secs);
            delay.min(self.config.max_retry_delay)
        } else {
            self.config.initial_retry_delay
        }
    }

    /// Shutdown the renewal scheduler
    pub async fn shutdown(&self) {
        info!("Requesting renewal scheduler shutdown");
        // The shutdown_tx will be dropped, signaling shutdown
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_renewal_config_creation() {
        let config = RenewalConfig::new()
            .with_check_interval(Duration::from_secs(600))
            .with_renewal_threshold(15)
            .with_max_retry_attempts(3);

        assert_eq!(config.check_interval, Duration::from_secs(600));
        assert_eq!(config.renewal_threshold_days, 15);
        assert_eq!(config.max_retry_attempts, 3);
    }

    #[test]
    fn test_renewal_config_presets() {
        let aggressive = RenewalConfig::aggressive();
        assert_eq!(aggressive.check_interval, Duration::from_secs(900));
        assert_eq!(aggressive.renewal_threshold_days, 60);

        let conservative = RenewalConfig::conservative();
        assert_eq!(conservative.check_interval, Duration::from_secs(21600));
        assert_eq!(conservative.renewal_threshold_days, 7);

        let production = RenewalConfig::production();
        assert_eq!(production.check_interval, Duration::from_secs(3600));
        assert_eq!(production.renewal_threshold_days, 30);
    }

    #[tokio::test]
    async fn test_renewal_scheduler_creation() {
        let config = RenewalConfig::new();
        let strategy = RenewalStrategy::SelfSigned {
            node_id: "test-node".to_string(),
            validity_days: 365,
        };

        let scheduler = RenewalScheduler::new(config, strategy);
        assert!(scheduler.get_certificate().await.is_none());
    }

    #[tokio::test]
    async fn test_set_and_get_certificate() {
        let config = RenewalConfig::new();
        let strategy = RenewalStrategy::SelfSigned {
            node_id: "test-node".to_string(),
            validity_days: 365,
        };

        let scheduler = RenewalScheduler::new(config, strategy);

        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();
        scheduler.set_certificate(cert).await;

        let retrieved = scheduler.get_certificate().await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().info.common_name, "test-node");
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config = RenewalConfig::new();
        let strategy = RenewalStrategy::SelfSigned {
            node_id: "test-node".to_string(),
            validity_days: 365,
        };

        let scheduler = RenewalScheduler::new(config, strategy);
        let mut rx = scheduler.subscribe();

        // Send a test event
        let event = RenewalEvent::CheckStarted {
            identifier: "test".to_string(),
            days_until_expiry: 30,
        };
        let _ = scheduler.event_tx.send(event.clone());

        // Receive event
        let received = rx.recv().await.unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn test_renewal_event_types() {
        let event = RenewalEvent::RenewalTriggered {
            identifier: "test".to_string(),
            days_until_expiry: 25,
            method: RenewalMethod::Acme,
        };

        match event {
            RenewalEvent::RenewalTriggered { method, .. } => {
                assert_eq!(method, RenewalMethod::Acme);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_renewal_with_short_lived_cert() {
        let config = RenewalConfig::new()
            .with_check_interval(Duration::from_millis(100))
            .with_renewal_threshold(25); // Renew if < 25 days

        let strategy = RenewalStrategy::SelfSigned {
            node_id: "test-node".to_string(),
            validity_days: 365,
        };

        let scheduler = Arc::new(RenewalScheduler::new(config, strategy));

        // Create a short-lived certificate that needs renewal
        let cert = Certificate::generate_self_signed("test-node".to_string(), 20).unwrap();
        scheduler.set_certificate(cert).await;

        let mut rx = scheduler.subscribe();

        // Start scheduler in background
        let scheduler_clone = scheduler.clone();
        let handle = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(2), scheduler_clone.start())
                .await
                .ok();
        });

        // Wait for renewal events
        let mut renewal_triggered = false;
        let timeout = tokio::time::sleep(Duration::from_secs(1));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Ok(event) = rx.recv() => {
                    if let RenewalEvent::RenewalTriggered { .. } = event {
                        renewal_triggered = true;
                        break;
                    }
                }
                _ = &mut timeout => {
                    break;
                }
            }
        }

        assert!(renewal_triggered, "Renewal should have been triggered");

        handle.abort();
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = RenewalConfig::new()
            .with_check_interval(Duration::from_secs(60))
            .with_renewal_threshold(30);

        let strategy = RenewalStrategy::SelfSigned {
            node_id: "test".to_string(),
            validity_days: 365,
        };

        let scheduler = RenewalScheduler::new(config, strategy);

        // Test exponential backoff
        assert_eq!(scheduler.calculate_retry_delay(0), Duration::from_secs(60));
        assert_eq!(scheduler.calculate_retry_delay(1), Duration::from_secs(120));
        assert_eq!(scheduler.calculate_retry_delay(2), Duration::from_secs(240));
    }
}
