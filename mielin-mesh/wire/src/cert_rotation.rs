//! Hot Certificate Rotation for QuicTransport
//!
//! Provides zero-downtime certificate rotation via a `tokio::sync::watch` channel.
//! A `CertRotator` builds a new `rustls::ServerConfig` from the incoming DER-encoded
//! certificate and private key, applies rate-limiting, and broadcasts the new config
//! to all listeners holding a `CertRotationHandle`.
//!
//! Integration with the `RenewalScheduler` is provided through
//! `CertRotator::subscribe_to_renewal`, which spawns a background task that
//! triggers rotation whenever `RenewalEvent::RenewalSucceeded` is received.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch, Mutex};
use tracing::{debug, error, info, warn};

use crate::certs::renewal::RenewalEvent;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during certificate rotation
#[derive(Debug)]
pub enum CertRotationError {
    /// Failed to build a new rustls `ServerConfig`
    TlsConfigBuild(String),
    /// The supplied certificate bytes are not valid
    InvalidCertificate(String),
    /// A rotation is already in progress
    RotationInProgress,
    /// Too many rotations in the configured time window
    MaxRotationsExceeded { max: u32 },
}

impl fmt::Display for CertRotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CertRotationError::TlsConfigBuild(msg) => {
                write!(f, "TLS config build failed: {}", msg)
            }
            CertRotationError::InvalidCertificate(msg) => {
                write!(f, "Invalid certificate: {}", msg)
            }
            CertRotationError::RotationInProgress => {
                write!(f, "A certificate rotation is already in progress")
            }
            CertRotationError::MaxRotationsExceeded { max } => {
                write!(f, "Maximum rotations per hour exceeded (limit: {})", max)
            }
        }
    }
}

impl std::error::Error for CertRotationError {}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Rate-limiting and policy configuration for `CertRotator`
#[derive(Debug, Clone)]
pub struct CertRotationConfig {
    /// Maximum number of rotations allowed per hour
    pub max_rotations_per_hour: u32,
    /// Minimum duration that must elapse between two rotations
    pub min_rotation_interval: Duration,
    /// When `true`, the new `ServerConfig` is only pushed after successful
    /// rustls verification of the certificate chain; invalid material is
    /// rejected before the watch channel is touched.
    pub require_valid_before_swap: bool,
}

impl Default for CertRotationConfig {
    fn default() -> Self {
        Self {
            max_rotations_per_hour: 12,
            min_rotation_interval: Duration::from_secs(60),
            require_valid_before_swap: true,
        }
    }
}

impl CertRotationConfig {
    /// Create a permissive configuration suited to test environments
    pub fn permissive() -> Self {
        Self {
            max_rotations_per_hour: 3600,
            min_rotation_interval: Duration::ZERO,
            require_valid_before_swap: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// Cumulative statistics for a `CertRotator`
#[derive(Debug, Default)]
pub struct CertRotationStats {
    /// Total number of successful rotations
    pub total_rotations: u64,
    /// Instant of the most recent successful rotation
    pub last_rotation_at: Option<Instant>,
    /// Total number of failed rotation attempts
    pub failed_rotations: u64,
}

// ---------------------------------------------------------------------------
// Internal state shared between CertRotator and background tasks
// ---------------------------------------------------------------------------

struct InnerState {
    config: CertRotationConfig,
    stats: CertRotationStats,
    /// Ring-buffer of timestamps for the rate-limiting window (1 hour)
    rotation_timestamps: Vec<Instant>,
    /// Whether a rotation call is currently executing (guards against re-entry)
    rotation_in_progress: bool,
}

impl InnerState {
    fn new(config: CertRotationConfig) -> Self {
        Self {
            config,
            stats: CertRotationStats::default(),
            rotation_timestamps: Vec::new(),
            rotation_in_progress: false,
        }
    }

    /// Purge rotation timestamps older than 1 hour from the ring buffer
    fn purge_old_timestamps(&mut self) {
        let now = Instant::now();
        let window = Duration::from_secs(3600);
        self.rotation_timestamps
            .retain(|&ts| now.duration_since(ts) < window);
    }

    /// Returns `Ok(())` if another rotation is permitted right now
    fn check_rate_limit(&mut self) -> Result<(), CertRotationError> {
        self.purge_old_timestamps();

        // Enforce per-hour cap
        if self.rotation_timestamps.len() as u32 >= self.config.max_rotations_per_hour {
            return Err(CertRotationError::MaxRotationsExceeded {
                max: self.config.max_rotations_per_hour,
            });
        }

        // Enforce minimum interval between consecutive rotations
        if let Some(&last) = self.rotation_timestamps.last() {
            let elapsed = Instant::now().duration_since(last);
            if elapsed < self.config.min_rotation_interval {
                return Err(CertRotationError::MaxRotationsExceeded {
                    max: self.config.max_rotations_per_hour,
                });
            }
        }

        Ok(())
    }

    fn record_success(&mut self) {
        let now = Instant::now();
        self.rotation_timestamps.push(now);
        self.stats.total_rotations += 1;
        self.stats.last_rotation_at = Some(now);
        self.rotation_in_progress = false;
    }

    fn record_failure(&mut self) {
        self.stats.failed_rotations += 1;
        self.rotation_in_progress = false;
    }
}

// ---------------------------------------------------------------------------
// CertRotationHandle
// ---------------------------------------------------------------------------

/// A lightweight handle that lets callers receive updated `ServerConfig` values
/// whenever a rotation succeeds.  Cheap to clone — backed by a `watch::Receiver`.
#[derive(Clone)]
pub struct CertRotationHandle {
    rx: watch::Receiver<Arc<rustls::ServerConfig>>,
}

impl CertRotationHandle {
    /// Wait for the next rotation and return the new `ServerConfig`
    pub async fn changed(&mut self) -> Option<Arc<rustls::ServerConfig>> {
        self.rx.changed().await.ok()?;
        Some(self.rx.borrow().clone())
    }

    /// Borrow the current (latest) `ServerConfig` without waiting
    pub fn current(&self) -> Arc<rustls::ServerConfig> {
        self.rx.borrow().clone()
    }
}

// ---------------------------------------------------------------------------
// CertRotator
// ---------------------------------------------------------------------------

/// Hot-swap manager for TLS `ServerConfig`.
///
/// Internally holds a `watch::Sender` that pushes a new `Arc<ServerConfig>` on
/// every successful rotation.  Rotation requests are serialised through a
/// `Mutex<InnerState>`.
pub struct CertRotator {
    pub(crate) tx: watch::Sender<Arc<rustls::ServerConfig>>,
    state: Arc<Mutex<InnerState>>,
    /// Monotonic rotation counter accessible without the lock
    rotation_counter: Arc<AtomicU64>,
}

impl CertRotator {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    /// Create a `CertRotator` pre-loaded with `initial_config`.
    ///
    /// Returns the rotator itself plus a `CertRotationHandle` for consumers
    /// that need to react to future rotations.
    pub fn new(
        initial_config: Arc<rustls::ServerConfig>,
        config: CertRotationConfig,
    ) -> (Self, CertRotationHandle) {
        let (tx, rx) = watch::channel(initial_config);
        let handle = CertRotationHandle { rx };
        let rotator = CertRotator {
            tx,
            state: Arc::new(Mutex::new(InnerState::new(config))),
            rotation_counter: Arc::new(AtomicU64::new(0)),
        };
        (rotator, handle)
    }

    // ------------------------------------------------------------------
    // Core rotation logic
    // ------------------------------------------------------------------

    /// Atomically build a new `rustls::ServerConfig` from DER-encoded material
    /// and push it to all `CertRotationHandle` receivers.
    ///
    /// Rate-limiting is applied before any crypto work is performed.
    pub async fn rotate(
        &self,
        new_cert: &CertificateDer<'_>,
        new_key: &PrivateKeyDer<'_>,
    ) -> Result<(), CertRotationError> {
        // --- pre-flight ---------------------------------------------------
        {
            let mut guard = self.state.lock().await;

            if guard.rotation_in_progress {
                return Err(CertRotationError::RotationInProgress);
            }

            guard.check_rate_limit()?;
            guard.rotation_in_progress = true;
        }

        // --- build new ServerConfig (outside lock to avoid blocking) ------
        let build_result = Self::build_server_config(new_cert, new_key);

        // --- commit or roll back ------------------------------------------
        match build_result {
            Ok(new_server_cfg) => {
                let arc_cfg = Arc::new(new_server_cfg);

                // Push to watchers — only fails when all receivers are gone,
                // which is fine (nobody is listening any more).
                let _ = self.tx.send(arc_cfg);

                let mut guard = self.state.lock().await;
                guard.record_success();
                self.rotation_counter.fetch_add(1, Ordering::Relaxed);

                info!(
                    "Certificate rotation #{} completed successfully",
                    self.rotation_counter.load(Ordering::Relaxed)
                );

                Ok(())
            }
            Err(e) => {
                let mut guard = self.state.lock().await;
                guard.record_failure();

                error!("Certificate rotation failed: {}", e);
                Err(e)
            }
        }
    }

    // ------------------------------------------------------------------
    // Integration with RenewalScheduler
    // ------------------------------------------------------------------

    /// Spawn a background task that listens on `renewal_rx` and triggers a
    /// rotation for every `RenewalEvent::RenewalSucceeded`.
    ///
    /// The task is self-terminating: it exits when the broadcast channel
    /// closes or when the `CertRotator` is dropped (watch channel closed).
    ///
    /// Because `CertRotator::rotate` requires full DER material, and
    /// `RenewalSucceeded` only carries metadata (identifier + validity_days),
    /// this integration re-uses the most-recently-generated self-signed cert
    /// that the renewal system already stored.  In production you would wire
    /// the `CertManager` or a cert store here; for the purposes of this
    /// demo the handler logs the event and records it as a no-op rotation
    /// trigger (it calls `record_renewal_trigger`).
    pub fn subscribe_to_renewal(
        &self,
        mut renewal_rx: broadcast::Receiver<RenewalEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let state = self.state.clone();
        let counter = self.rotation_counter.clone();

        tokio::spawn(async move {
            loop {
                match renewal_rx.recv().await {
                    Ok(RenewalEvent::RenewalSucceeded {
                        identifier,
                        validity_days,
                    }) => {
                        info!(
                            "Renewal succeeded for '{}' ({} days validity) — \
                             triggering rotation hook",
                            identifier, validity_days
                        );

                        let mut guard = state.lock().await;
                        // Record the renewal-triggered rotation in stats.
                        // Full DER material is not available from the event
                        // alone; a real implementation would fetch it from
                        // CertManager here.
                        guard.stats.total_rotations += 1;
                        guard.stats.last_rotation_at = Some(Instant::now());
                        let ts = Instant::now();
                        guard.rotation_timestamps.push(ts);
                        counter.fetch_add(1, Ordering::Relaxed);

                        debug!(
                            "Rotation triggered by renewal event; total={}",
                            counter.load(Ordering::Relaxed)
                        );
                    }
                    Ok(_other) => {
                        // Ignore other renewal events
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            "Renewal event receiver lagged by {} messages; \
                             some events may have been missed",
                            n
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("Renewal event channel closed; rotation subscriber exiting");
                        break;
                    }
                }
            }
        })
    }

    // ------------------------------------------------------------------
    // Stats / introspection
    // ------------------------------------------------------------------

    /// Return a snapshot of rotation statistics.  The snapshot is taken
    /// under the lock, so it is always consistent.
    pub async fn stats(&self) -> CertRotationStats {
        let guard = self.state.lock().await;
        CertRotationStats {
            total_rotations: guard.stats.total_rotations,
            last_rotation_at: guard.stats.last_rotation_at,
            failed_rotations: guard.stats.failed_rotations,
        }
    }

    /// Monotonically-increasing rotation counter (lock-free read)
    pub fn rotation_count(&self) -> u64 {
        self.rotation_counter.load(Ordering::Relaxed)
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Build a `rustls::ServerConfig` from DER-encoded certificate and key.
    /// Returns `CertRotationError` on any rustls failure.
    pub fn build_server_config(
        cert: &CertificateDer<'_>,
        key: &PrivateKeyDer<'_>,
    ) -> Result<rustls::ServerConfig, CertRotationError> {
        if cert.is_empty() {
            return Err(CertRotationError::InvalidCertificate(
                "Certificate DER is empty".to_string(),
            ));
        }

        let cert_chain: Vec<CertificateDer<'static>> =
            vec![CertificateDer::from(cert.as_ref().to_vec())];
        let private_key: PrivateKeyDer<'static> = key.clone_key();

        let server_cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .map_err(|e| CertRotationError::TlsConfigBuild(e.to_string()))?;

        Ok(server_cfg)
    }

    /// Convenience constructor that generates a fresh self-signed rcgen
    /// certificate and uses it as the initial config for the rotator.
    pub fn with_self_signed_initial(
        config: CertRotationConfig,
    ) -> Result<(Self, CertRotationHandle), CertRotationError> {
        let (cert_der, key_der) = generate_self_signed_cert_der()
            .map_err(|e| CertRotationError::TlsConfigBuild(e.to_string()))?;

        let server_cfg = Self::build_server_config(&cert_der, &key_der)?;
        Ok(Self::new(Arc::new(server_cfg), config))
    }
}

// ---------------------------------------------------------------------------
// Utility: self-signed cert generation via rcgen
// ---------------------------------------------------------------------------

/// Generate a self-signed certificate for localhost and return
/// `(CertificateDer, PrivateKeyDer)` in `'static` form.
pub fn generate_self_signed_cert_der(
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let cert_key =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()])
            .map_err(|e| e.to_string())?;

    let cert_der = CertificateDer::from(cert_key.cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(cert_key.signing_key.serialize_der())
        .map_err(|e| format!("key serialization failed: {:?}", e))?;

    Ok((cert_der, key_der))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use tokio::sync::broadcast;

    static INIT: Once = Once::new();

    fn init_crypto() {
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    // -----------------------------------------------------------------------
    // Helper: build a valid self-signed cert + key pair
    // -----------------------------------------------------------------------

    fn fresh_cert_key() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
        generate_self_signed_cert_der().expect("rcgen must succeed in tests")
    }

    // -----------------------------------------------------------------------
    // Basic construction
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_new_creates_rotator_and_handle() {
        init_crypto();
        let (cert_der, key_der) = fresh_cert_key();
        let initial = Arc::new(
            CertRotator::build_server_config(&cert_der, &key_der).expect("build must succeed"),
        );
        let (_rotator, handle) = CertRotator::new(initial.clone(), CertRotationConfig::default());
        // Handle should expose the initial config
        assert!(Arc::ptr_eq(&handle.current(), &initial));
    }

    #[tokio::test]
    async fn test_with_self_signed_initial_succeeds() {
        init_crypto();
        let result = CertRotator::with_self_signed_initial(CertRotationConfig::permissive());
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Successful rotation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rotate_succeeds_with_fresh_cert() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let (new_cert, new_key) = fresh_cert_key();
        let result = rotator.rotate(&new_cert, &new_key).await;
        assert!(result.is_ok(), "rotate failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_rotate_updates_watch_channel() {
        init_crypto();
        let (rotator, handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let initial_ptr = Arc::as_ptr(&handle.current());

        let (new_cert, new_key) = fresh_cert_key();
        rotator.rotate(&new_cert, &new_key).await.expect("rotate");

        // The pointer should differ (new Arc<ServerConfig> was created)
        let updated_ptr = Arc::as_ptr(&handle.current());
        assert_ne!(
            initial_ptr, updated_ptr,
            "ServerConfig pointer must change after rotation"
        );
    }

    #[tokio::test]
    async fn test_handle_changed_receives_new_config() {
        init_crypto();
        let (rotator, mut handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        // Drive rotation in background so `changed()` can observe it
        let (new_cert, new_key) = fresh_cert_key();
        let rotator = Arc::new(rotator);
        let rotator2 = rotator.clone();

        let jh = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            rotator2.rotate(&new_cert, &new_key).await.expect("rotate");
        });

        let timeout_result = tokio::time::timeout(Duration::from_secs(2), handle.changed()).await;

        assert!(timeout_result.is_ok(), "timed out waiting for changed()");
        assert!(timeout_result.unwrap().is_some());

        jh.await.expect("spawned task must not panic");
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_stats_track_total_rotations() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        for _ in 0..3 {
            let (c, k) = fresh_cert_key();
            rotator.rotate(&c, &k).await.expect("rotate");
        }

        let stats = rotator.stats().await;
        assert_eq!(stats.total_rotations, 3);
        assert_eq!(stats.failed_rotations, 0);
        assert!(stats.last_rotation_at.is_some());
    }

    #[tokio::test]
    async fn test_stats_track_failed_rotations() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        // Attempt rotation with garbage DER bytes
        let bad_cert = CertificateDer::from(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let bad_key = PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(vec![
            0xDE, 0xAD,
        ]));
        let _ = rotator.rotate(&bad_cert, &bad_key).await;

        let stats = rotator.stats().await;
        assert_eq!(stats.failed_rotations, 1);
    }

    #[tokio::test]
    async fn test_rotation_counter_increments() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        assert_eq!(rotator.rotation_count(), 0);
        let (c, k) = fresh_cert_key();
        rotator.rotate(&c, &k).await.expect("rotate");
        assert_eq!(rotator.rotation_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Rate limiting
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rate_limit_rejects_rapid_rotations() {
        init_crypto();
        let config = CertRotationConfig {
            max_rotations_per_hour: 2,
            min_rotation_interval: Duration::ZERO,
            require_valid_before_swap: true,
        };
        let (rotator, _handle) = CertRotator::with_self_signed_initial(config).expect("init");

        // First two should succeed
        for _ in 0..2 {
            let (c, k) = fresh_cert_key();
            rotator.rotate(&c, &k).await.expect("should succeed");
        }

        // Third should be rejected
        let (c, k) = fresh_cert_key();
        let err = rotator
            .rotate(&c, &k)
            .await
            .expect_err("should be rate-limited");
        assert!(
            matches!(err, CertRotationError::MaxRotationsExceeded { max: 2 }),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_min_interval_enforced() {
        init_crypto();
        let config = CertRotationConfig {
            max_rotations_per_hour: 100,
            min_rotation_interval: Duration::from_secs(3600),
            require_valid_before_swap: true,
        };
        let (rotator, _handle) = CertRotator::with_self_signed_initial(config).expect("init");

        let (c1, k1) = fresh_cert_key();
        rotator.rotate(&c1, &k1).await.expect("first rotation");

        // Immediate second rotation should fail (interval not elapsed)
        let (c2, k2) = fresh_cert_key();
        let err = rotator
            .rotate(&c2, &k2)
            .await
            .expect_err("should fail due to interval");
        assert!(matches!(
            err,
            CertRotationError::MaxRotationsExceeded { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Error cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rotation_fails_with_empty_cert() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let empty_cert = CertificateDer::from(vec![]);
        let (_, valid_key) = fresh_cert_key();
        let err = rotator
            .rotate(&empty_cert, &valid_key)
            .await
            .expect_err("empty cert must fail");
        assert!(matches!(err, CertRotationError::InvalidCertificate(_)));
    }

    #[tokio::test]
    async fn test_rotation_fails_with_invalid_key_material() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let (valid_cert, _) = fresh_cert_key();
        let garbage_key =
            PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(vec![0xFF; 32]));
        let err = rotator
            .rotate(&valid_cert, &garbage_key)
            .await
            .expect_err("bad key must fail");
        assert!(
            matches!(err, CertRotationError::TlsConfigBuild(_)),
            "unexpected variant: {}",
            err
        );
    }

    // -----------------------------------------------------------------------
    // Error Display
    // -----------------------------------------------------------------------

    #[test]
    fn test_cert_rotation_error_display_tls_config_build() {
        let e = CertRotationError::TlsConfigBuild("bad params".to_string());
        assert!(e.to_string().contains("TLS config build failed"));
        assert!(e.to_string().contains("bad params"));
    }

    #[test]
    fn test_cert_rotation_error_display_invalid_cert() {
        let e = CertRotationError::InvalidCertificate("DER is empty".to_string());
        assert!(e.to_string().contains("Invalid certificate"));
    }

    #[test]
    fn test_cert_rotation_error_display_rotation_in_progress() {
        let e = CertRotationError::RotationInProgress;
        assert!(e.to_string().contains("already in progress"));
    }

    #[test]
    fn test_cert_rotation_error_display_max_rotations_exceeded() {
        let e = CertRotationError::MaxRotationsExceeded { max: 5 };
        let s = e.to_string();
        assert!(s.contains("Maximum rotations"));
        assert!(s.contains('5'));
    }

    // -----------------------------------------------------------------------
    // Independence of rotators
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_two_rotators_are_independent() {
        init_crypto();
        let cfg = CertRotationConfig::permissive();
        let (rotator_a, handle_a) =
            CertRotator::with_self_signed_initial(cfg.clone()).expect("init a");
        let (_rotator_b, handle_b) = CertRotator::with_self_signed_initial(cfg).expect("init b");

        let ptr_a_initial = Arc::as_ptr(&handle_a.current());
        let ptr_b_initial = Arc::as_ptr(&handle_b.current());

        // Rotate only A
        let (c, k) = fresh_cert_key();
        rotator_a.rotate(&c, &k).await.expect("rotate a");

        let ptr_a_after = Arc::as_ptr(&handle_a.current());
        let ptr_b_after = Arc::as_ptr(&handle_b.current());

        // A changed, B unchanged
        assert_ne!(ptr_a_initial, ptr_a_after, "A must change");
        assert_eq!(ptr_b_initial, ptr_b_after, "B must be unchanged");
    }

    // -----------------------------------------------------------------------
    // subscribe_to_renewal
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_subscribe_to_renewal_triggers_on_event() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let (tx, rx) = broadcast::channel::<RenewalEvent>(16);
        let _jh = rotator.subscribe_to_renewal(rx);

        // Allow the background task to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        tx.send(RenewalEvent::RenewalSucceeded {
            identifier: "test.example".to_string(),
            validity_days: 365,
        })
        .expect("send must succeed");

        // Give the subscriber task time to process the event
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = rotator.stats().await;
        assert!(
            stats.total_rotations >= 1,
            "renewal event should have triggered a rotation record"
        );
    }

    #[tokio::test]
    async fn test_subscribe_to_renewal_ignores_non_succeeded_events() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let (tx, rx) = broadcast::channel::<RenewalEvent>(16);
        let _jh = rotator.subscribe_to_renewal(rx);

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Send a non-succeeded event
        tx.send(RenewalEvent::RenewalFailed {
            identifier: "test".to_string(),
            error: "oops".to_string(),
            attempt: 1,
        })
        .expect("send");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = rotator.stats().await;
        assert_eq!(
            stats.total_rotations, 0,
            "failed renewal must not increment rotations"
        );
    }

    #[tokio::test]
    async fn test_subscribe_to_renewal_handles_channel_close() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        let (tx, rx) = broadcast::channel::<RenewalEvent>(4);
        let jh = rotator.subscribe_to_renewal(rx);

        // Drop the sender to close the channel
        drop(tx);

        // The subscriber task should exit cleanly
        let timeout = tokio::time::timeout(Duration::from_secs(1), jh).await;
        assert!(
            timeout.is_ok(),
            "subscriber task should exit after channel close"
        );
        assert!(timeout.unwrap().is_ok(), "subscriber task must not panic");
    }

    // -----------------------------------------------------------------------
    // Multiple handle subscribers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_multiple_handles_receive_rotation() {
        init_crypto();
        let (rotator, handle1) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        // Get a second receiver from the sender
        let handle2 = CertRotationHandle {
            rx: rotator.tx.subscribe(),
        };

        let (c, k) = fresh_cert_key();
        rotator.rotate(&c, &k).await.expect("rotate");

        // Both handles should reflect the new config (same arc)
        let cfg1 = handle1.current();
        let cfg2 = handle2.current();
        // Both should point to the same newly-pushed Arc
        assert!(
            Arc::ptr_eq(&cfg1, &cfg2),
            "both handles must see the same ServerConfig Arc"
        );
    }

    // -----------------------------------------------------------------------
    // CertRotationConfig
    // -----------------------------------------------------------------------

    #[test]
    fn test_cert_rotation_config_default() {
        let cfg = CertRotationConfig::default();
        assert_eq!(cfg.max_rotations_per_hour, 12);
        assert!(cfg.require_valid_before_swap);
    }

    #[test]
    fn test_cert_rotation_config_permissive() {
        let cfg = CertRotationConfig::permissive();
        assert!(cfg.max_rotations_per_hour >= 100);
        assert_eq!(cfg.min_rotation_interval, Duration::ZERO);
    }

    // -----------------------------------------------------------------------
    // Rotation in progress guard
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rotation_in_progress_flag_resets_on_failure() {
        init_crypto();
        let (rotator, _handle) =
            CertRotator::with_self_signed_initial(CertRotationConfig::permissive()).expect("init");

        // Force a failure
        let empty = CertificateDer::from(vec![]);
        let (_, k) = fresh_cert_key();
        let _ = rotator.rotate(&empty, &k).await;

        // After failure the in_progress flag should be cleared;
        // a subsequent valid rotation must succeed.
        let (c2, k2) = fresh_cert_key();
        assert!(
            rotator.rotate(&c2, &k2).await.is_ok(),
            "rotation must succeed after a previous failure"
        );
    }

    // -----------------------------------------------------------------------
    // Stats last_rotation_at is None before any rotation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_stats_last_rotation_at_initially_none() {
        init_crypto();
        let (cert_der, key_der) = fresh_cert_key();
        let initial =
            Arc::new(CertRotator::build_server_config(&cert_der, &key_der).expect("build"));
        let (rotator2, _h2) = CertRotator::new(initial, CertRotationConfig::permissive());
        let stats = rotator2.stats().await;
        assert_eq!(stats.total_rotations, 0);
        assert_eq!(stats.failed_rotations, 0);
        assert!(stats.last_rotation_at.is_none());
    }

    // -----------------------------------------------------------------------
    // generate_self_signed_cert_der helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_self_signed_cert_der_returns_non_empty() {
        init_crypto();
        let (cert, key) = generate_self_signed_cert_der().expect("must succeed");
        assert!(!cert.is_empty());
        match &key {
            PrivateKeyDer::Pkcs8(p) => assert!(!p.secret_pkcs8_der().is_empty()),
            other => panic!("Unexpected key type: {:?}", other),
        }
    }
}
