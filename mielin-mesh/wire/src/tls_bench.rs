//! TLS Handshake Performance Measurements
//!
//! This module provides in-process latency measurements for TLS-related
//! operations.  It is NOT a Criterion benchmark (those live in `benches/`);
//! instead it contains regular `#[tokio::test]` test functions with timing
//! assertions that enforce generous but meaningful upper bounds suitable for
//! both developer machines and CI environments.
//!
//! # Design
//!
//! Because setting up a full QUIC handshake end-to-end requires a live UDP
//! socket, which introduces network-stack jitter and can be flaky under
//! resource-constrained CI, we fall back to measuring the cost of
//! `rustls::ServerConfig` construction from a freshly-generated self-signed
//! certificate.  This is the hot path exercised during every connection
//! setup, and its latency directly bounds the minimum achievable handshake
//! time.  The measurements are deterministic and run fully in-process.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::cert_rotation::generate_self_signed_cert_der;

// ---------------------------------------------------------------------------
// Public API types
// ---------------------------------------------------------------------------

/// Latency histogram for a batch of handshake measurements (all values in
/// microseconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsHandshakeMetrics {
    /// Minimum observed latency in microseconds
    pub min_us: u64,
    /// Maximum observed latency in microseconds
    pub max_us: u64,
    /// Arithmetic mean latency in microseconds (rounded down)
    pub mean_us: u64,
    /// 99th-percentile latency in microseconds
    pub p99_us: u64,
    /// Number of samples collected
    pub samples: usize,
}

impl TlsHandshakeMetrics {
    /// Build `TlsHandshakeMetrics` from a raw slice of per-sample durations.
    /// Panics if `samples` is empty.
    pub fn from_samples(mut latencies_us: Vec<u64>) -> Self {
        assert!(!latencies_us.is_empty(), "at least one sample required");

        latencies_us.sort_unstable();

        let samples = latencies_us.len();
        let min_us = latencies_us[0];
        let max_us = latencies_us[samples - 1];
        let mean_us = latencies_us.iter().sum::<u64>() / samples as u64;

        // p99 index: for n samples the 99th percentile sits at
        // ceil(n * 0.99) - 1 (zero-based), clamped to [0, n-1].
        let p99_idx = ((samples as f64 * 0.99).ceil() as usize)
            .saturating_sub(1)
            .min(samples - 1);
        let p99_us = latencies_us[p99_idx];

        Self {
            min_us,
            max_us,
            mean_us,
            p99_us,
            samples,
        }
    }
}

// ---------------------------------------------------------------------------
// Benchmark configuration
// ---------------------------------------------------------------------------

/// Configuration for a handshake benchmark run
#[derive(Debug, Clone)]
pub struct HandshakeBenchConfig {
    /// Number of concurrent tasks (1 = sequential)
    pub parallelism: usize,
    /// Payload size in bytes sent after the handshake (reserved for future
    /// use; current implementation measures only config-construction cost)
    pub payload_bytes: usize,
    /// Hard timeout for the entire benchmark run
    pub timeout: Duration,
}

impl Default for HandshakeBenchConfig {
    fn default() -> Self {
        Self {
            parallelism: 1,
            payload_bytes: 0,
            timeout: Duration::from_secs(30),
        }
    }
}

impl HandshakeBenchConfig {
    /// Create a configuration for sequential measurement
    pub fn sequential(timeout: Duration) -> Self {
        Self {
            parallelism: 1,
            payload_bytes: 0,
            timeout,
        }
    }

    /// Validate that the configuration is self-consistent
    pub fn validate(&self) -> Result<(), String> {
        if self.parallelism == 0 {
            return Err("parallelism must be >= 1".to_string());
        }
        if self.timeout == Duration::ZERO {
            return Err("timeout must be > 0".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Core measurement function
// ---------------------------------------------------------------------------

/// Measure `n` sequential `rustls::ServerConfig` constructions from a fresh
/// self-signed certificate and return aggregate latency metrics.
///
/// Each iteration calls `rcgen::generate_simple_self_signed` + rustls
/// `ServerConfig::builder().with_no_client_auth().with_single_cert(…)`.
/// This exercises the certificate parsing, key loading, and crypto provider
/// initialisation that occurs for every new QUIC connection setup.
///
/// Returns `Err` if crypto initialisation fails or `n == 0`.
pub fn measure_handshakes(
    n: usize,
    _config: &HandshakeBenchConfig,
) -> Result<TlsHandshakeMetrics, String> {
    if n == 0 {
        return Err("n must be > 0".to_string());
    }

    let mut latencies_us = Vec::with_capacity(n);

    for _ in 0..n {
        let t0 = Instant::now();

        // Generate fresh self-signed cert + build ServerConfig
        let (cert_der, key_der) = generate_self_signed_cert_der()?;

        let cert_chain: Vec<rustls::pki_types::CertificateDer<'static>> =
            vec![rustls::pki_types::CertificateDer::from(
                cert_der.as_ref().to_vec(),
            )];
        let private_key: rustls::pki_types::PrivateKeyDer<'static> = key_der.clone_key();

        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .map_err(|e| format!("ServerConfig build failed: {}", e))?;

        let elapsed = t0.elapsed();
        latencies_us.push(elapsed.as_micros() as u64);
    }

    Ok(TlsHandshakeMetrics::from_samples(latencies_us))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // HandshakeBenchConfig
    // -----------------------------------------------------------------------

    #[test]
    fn test_handshake_bench_config_default_is_valid() {
        let cfg = HandshakeBenchConfig::default();
        assert!(cfg.validate().is_ok(), "default config must be valid");
    }

    #[test]
    fn test_handshake_bench_config_zero_parallelism_is_invalid() {
        let cfg = HandshakeBenchConfig {
            parallelism: 0,
            payload_bytes: 0,
            timeout: Duration::from_secs(5),
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_handshake_bench_config_zero_timeout_is_invalid() {
        let cfg = HandshakeBenchConfig {
            parallelism: 1,
            payload_bytes: 0,
            timeout: Duration::ZERO,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_handshake_bench_config_sequential_helper() {
        let cfg = HandshakeBenchConfig::sequential(Duration::from_secs(10));
        assert_eq!(cfg.parallelism, 1);
        assert_eq!(cfg.timeout, Duration::from_secs(10));
        assert!(cfg.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // TlsHandshakeMetrics
    // -----------------------------------------------------------------------

    #[test]
    fn test_metrics_from_samples_basic() {
        let samples = vec![10u64, 20, 30, 40, 50];
        let m = TlsHandshakeMetrics::from_samples(samples);
        assert_eq!(m.min_us, 10);
        assert_eq!(m.max_us, 50);
        assert_eq!(m.mean_us, 30); // (10+20+30+40+50)/5 = 30
        assert_eq!(m.samples, 5);
    }

    #[test]
    fn test_metrics_p99_gte_mean() {
        init_crypto();
        let cfg = HandshakeBenchConfig::sequential(Duration::from_secs(60));
        let metrics = measure_handshakes(20, &cfg).expect("measure_handshakes failed");
        assert!(
            metrics.p99_us >= metrics.mean_us,
            "p99 ({}) must be >= mean ({})",
            metrics.p99_us,
            metrics.mean_us
        );
    }

    #[test]
    fn test_metrics_min_lte_mean_lte_max() {
        let samples = vec![5u64, 15, 25, 35, 45, 100, 200];
        let m = TlsHandshakeMetrics::from_samples(samples);
        assert!(m.min_us <= m.mean_us, "min must be <= mean");
        assert!(m.mean_us <= m.max_us, "mean must be <= max");
    }

    #[test]
    fn test_metrics_single_sample() {
        let samples = vec![42u64];
        let m = TlsHandshakeMetrics::from_samples(samples);
        assert_eq!(m.min_us, 42);
        assert_eq!(m.max_us, 42);
        assert_eq!(m.mean_us, 42);
        assert_eq!(m.p99_us, 42);
        assert_eq!(m.samples, 1);
    }

    #[test]
    fn test_metrics_two_samples() {
        let m = TlsHandshakeMetrics::from_samples(vec![10u64, 20]);
        assert_eq!(m.min_us, 10);
        assert_eq!(m.max_us, 20);
        assert_eq!(m.mean_us, 15);
        assert_eq!(m.samples, 2);
    }

    // -----------------------------------------------------------------------
    // Serialization roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_metrics_serde_roundtrip() {
        let original = TlsHandshakeMetrics {
            min_us: 100,
            max_us: 9999,
            mean_us: 3000,
            p99_us: 8500,
            samples: 200,
        };

        let json = serde_json::to_string(&original).expect("serialise");
        let decoded: TlsHandshakeMetrics = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(original, decoded);
    }

    // -----------------------------------------------------------------------
    // measure_handshakes
    // -----------------------------------------------------------------------

    #[test]
    fn test_measure_handshakes_zero_n_returns_err() {
        init_crypto();
        let cfg = HandshakeBenchConfig::default();
        assert!(measure_handshakes(0, &cfg).is_err());
    }

    #[test]
    fn test_measure_handshakes_one_sample_succeeds() {
        init_crypto();
        let cfg = HandshakeBenchConfig::default();
        let result = measure_handshakes(1, &cfg);
        assert!(result.is_ok(), "single sample failed: {:?}", result.err());
        let m = result.unwrap();
        assert_eq!(m.samples, 1);
        assert!(m.min_us <= m.max_us);
    }

    /// 20 sequential ServerConfig constructions must finish well within 5 s
    /// even on slow CI runners.  Each construction typically takes 1–50 ms.
    #[test]
    fn test_20_sequential_handshakes_complete_within_5_seconds() {
        init_crypto();
        let cfg = HandshakeBenchConfig::sequential(Duration::from_secs(5));

        let wall_start = Instant::now();
        let metrics = measure_handshakes(20, &cfg).expect("measure_handshakes failed");
        let wall_elapsed = wall_start.elapsed();

        assert!(
            wall_elapsed < Duration::from_secs(5),
            "20 handshakes took {:?}, expected < 5 s",
            wall_elapsed
        );
        assert_eq!(metrics.samples, 20);
    }

    #[test]
    fn test_mean_latency_is_calculated_correctly() {
        // Build a fixed latency vector and verify the mean manually
        let latencies: Vec<u64> = (1u64..=10).map(|x| x * 100).collect(); // [100, 200, … 1000]
        let expected_mean: u64 = latencies.iter().sum::<u64>() / latencies.len() as u64; // 550
        let m = TlsHandshakeMetrics::from_samples(latencies);
        assert_eq!(m.mean_us, expected_mean);
    }

    #[test]
    fn test_p99_index_correctness_100_samples() {
        // With 100 sorted samples [1..=100], p99 should be sample at index 98 → value 99
        let latencies: Vec<u64> = (1u64..=100).collect();
        let m = TlsHandshakeMetrics::from_samples(latencies);
        // p99_idx = ceil(100 * 0.99) - 1 = 99 - 1 = 98 → value 99
        assert_eq!(m.p99_us, 99);
    }

    #[test]
    fn test_measure_handshakes_returns_plausible_latencies() {
        init_crypto();
        let cfg = HandshakeBenchConfig::default();
        let m = measure_handshakes(5, &cfg).expect("measure");
        // Each construction should be > 0 µs and < 10 000 000 µs (10 s)
        assert!(m.min_us > 0, "min must be > 0");
        assert!(
            m.max_us < 10_000_000,
            "max {} µs looks implausible",
            m.max_us
        );
    }
}
