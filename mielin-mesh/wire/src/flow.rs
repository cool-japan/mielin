//! Flow Control for Wire Protocol
//!
//! Provides comprehensive flow control with:
//! - Per-connection rate limiting (token bucket)
//! - Backpressure signaling
//! - Congestion control with adaptive rate adjustment
//! - Bandwidth allocation and fairness

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

// =============================================================================
// Token Bucket Rate Limiter
// =============================================================================

/// Token bucket configuration
#[derive(Debug, Clone)]
pub struct TokenBucketConfig {
    /// Maximum tokens (burst capacity)
    pub capacity: u64,
    /// Token refill rate (tokens per second)
    pub refill_rate: u64,
    /// Initial tokens
    pub initial_tokens: u64,
}

impl Default for TokenBucketConfig {
    fn default() -> Self {
        Self {
            capacity: 1_000_000,     // 1MB burst
            refill_rate: 10_000_000, // 10MB/s
            initial_tokens: 1_000_000,
        }
    }
}

impl TokenBucketConfig {
    /// High throughput preset (100MB/s, 10MB burst)
    pub fn high_throughput() -> Self {
        Self {
            capacity: 10_000_000,
            refill_rate: 100_000_000,
            initial_tokens: 10_000_000,
        }
    }

    /// Low latency preset (1MB/s, 100KB burst)
    pub fn low_latency() -> Self {
        Self {
            capacity: 100_000,
            refill_rate: 1_000_000,
            initial_tokens: 100_000,
        }
    }

    /// Embedded preset (100KB/s, 10KB burst)
    pub fn embedded() -> Self {
        Self {
            capacity: 10_000,
            refill_rate: 100_000,
            initial_tokens: 10_000,
        }
    }
}

/// Token bucket rate limiter
#[derive(Debug)]
pub struct TokenBucket {
    /// Configuration
    config: TokenBucketConfig,
    /// Current tokens
    tokens: AtomicU64,
    /// Last refill time
    last_refill: Mutex<Instant>,
}

impl TokenBucket {
    /// Create new token bucket
    pub fn new(config: TokenBucketConfig) -> Self {
        Self {
            tokens: AtomicU64::new(config.initial_tokens),
            last_refill: Mutex::new(Instant::now()),
            config,
        }
    }

    /// Create with default config
    pub fn default_bucket() -> Self {
        Self::new(TokenBucketConfig::default())
    }

    /// Try to consume tokens (non-blocking)
    pub async fn try_consume(&self, amount: u64) -> bool {
        self.refill().await;

        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            if current < amount {
                return false;
            }

            match self.tokens.compare_exchange_weak(
                current,
                current - amount,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Consume tokens (blocking until available)
    pub async fn consume(&self, amount: u64) {
        loop {
            if self.try_consume(amount).await {
                return;
            }

            // Calculate wait time
            let tokens_needed = amount.saturating_sub(self.tokens.load(Ordering::Relaxed));
            let wait_secs = tokens_needed as f64 / self.config.refill_rate as f64;
            let wait_duration = Duration::from_secs_f64(wait_secs.max(0.001));

            tokio::time::sleep(wait_duration).await;
        }
    }

    /// Refill tokens based on elapsed time
    async fn refill(&self) {
        let mut last = self.last_refill.lock().await;
        let elapsed = last.elapsed();
        *last = Instant::now();
        drop(last);

        let new_tokens = (elapsed.as_secs_f64() * self.config.refill_rate as f64) as u64;
        if new_tokens > 0 {
            let mut current = self.tokens.load(Ordering::Relaxed);
            loop {
                let new_value = (current + new_tokens).min(self.config.capacity);
                match self.tokens.compare_exchange_weak(
                    current,
                    new_value,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    /// Get current token count
    pub fn available(&self) -> u64 {
        self.tokens.load(Ordering::Relaxed)
    }

    /// Get fill percentage (0.0 - 1.0)
    pub fn fill_ratio(&self) -> f64 {
        self.tokens.load(Ordering::Relaxed) as f64 / self.config.capacity as f64
    }

    /// Get capacity
    pub fn capacity(&self) -> u64 {
        self.config.capacity
    }

    /// Get refill rate
    pub fn refill_rate(&self) -> u64 {
        self.config.refill_rate
    }
}

// =============================================================================
// Backpressure Signaling
// =============================================================================

/// Backpressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BackpressureLevel {
    /// No backpressure - full speed ahead
    None = 0,
    /// Light backpressure - reduce rate slightly
    Light = 1,
    /// Moderate backpressure - reduce rate significantly
    Moderate = 2,
    /// Heavy backpressure - critical slowdown
    Heavy = 3,
    /// Critical - stop sending new data
    Critical = 4,
}

impl BackpressureLevel {
    /// Get rate multiplier (0.0 - 1.0)
    pub fn rate_multiplier(&self) -> f64 {
        match self {
            Self::None => 1.0,
            Self::Light => 0.8,
            Self::Moderate => 0.5,
            Self::Heavy => 0.2,
            Self::Critical => 0.0,
        }
    }

    /// From fill ratio (queue fullness)
    pub fn from_fill_ratio(ratio: f64) -> Self {
        if ratio < 0.5 {
            Self::None
        } else if ratio < 0.7 {
            Self::Light
        } else if ratio < 0.85 {
            Self::Moderate
        } else if ratio < 0.95 {
            Self::Heavy
        } else {
            Self::Critical
        }
    }
}

/// Backpressure signal for cross-connection coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureSignal {
    /// Connection/peer identifier
    pub peer_id: [u8; 16],
    /// Current backpressure level
    pub level: BackpressureLevel,
    /// Suggested rate (bytes/sec), 0 = stop
    pub suggested_rate: u64,
    /// Signal timestamp
    pub timestamp: u64,
    /// Reason for backpressure
    pub reason: BackpressureReason,
}

/// Reason for backpressure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackpressureReason {
    /// Queue is filling up
    QueueFull,
    /// Memory pressure
    MemoryPressure,
    /// CPU overloaded
    CpuOverload,
    /// Network congestion
    NetworkCongestion,
    /// Downstream backpressure
    DownstreamPressure,
    /// Unknown/unspecified
    Unknown,
}

impl BackpressureSignal {
    /// Create new backpressure signal
    pub fn new(peer_id: [u8; 16], level: BackpressureLevel, reason: BackpressureReason) -> Self {
        let suggested_rate = match level {
            BackpressureLevel::None => u64::MAX,
            BackpressureLevel::Light => 8_000_000,    // 8MB/s
            BackpressureLevel::Moderate => 4_000_000, // 4MB/s
            BackpressureLevel::Heavy => 1_000_000,    // 1MB/s
            BackpressureLevel::Critical => 0,
        };

        Self {
            peer_id,
            level,
            suggested_rate,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            reason,
        }
    }

    /// Check if signal is recent (within threshold)
    pub fn is_recent(&self, threshold_ms: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        now.saturating_sub(self.timestamp) < threshold_ms
    }
}

/// Backpressure controller for a connection
#[derive(Debug)]
pub struct BackpressureController {
    /// Current outbound backpressure level
    outbound_level: AtomicU64,
    /// Current inbound backpressure level
    inbound_level: AtomicU64,
    /// Is paused (critical backpressure)
    paused: AtomicBool,
    /// Pending signals to send
    pending_signals: Mutex<Vec<BackpressureSignal>>,
    /// Received signals from peers
    received_signals: RwLock<HashMap<[u8; 16], BackpressureSignal>>,
}

impl Default for BackpressureController {
    fn default() -> Self {
        Self::new()
    }
}

impl BackpressureController {
    /// Create new backpressure controller
    pub fn new() -> Self {
        Self {
            outbound_level: AtomicU64::new(BackpressureLevel::None as u64),
            inbound_level: AtomicU64::new(BackpressureLevel::None as u64),
            paused: AtomicBool::new(false),
            pending_signals: Mutex::new(Vec::new()),
            received_signals: RwLock::new(HashMap::new()),
        }
    }

    /// Set outbound backpressure level
    pub fn set_outbound_level(&self, level: BackpressureLevel) {
        self.outbound_level.store(level as u64, Ordering::Relaxed);
        self.paused
            .store(level == BackpressureLevel::Critical, Ordering::Relaxed);
    }

    /// Get outbound backpressure level
    pub fn outbound_level(&self) -> BackpressureLevel {
        match self.outbound_level.load(Ordering::Relaxed) {
            0 => BackpressureLevel::None,
            1 => BackpressureLevel::Light,
            2 => BackpressureLevel::Moderate,
            3 => BackpressureLevel::Heavy,
            _ => BackpressureLevel::Critical,
        }
    }

    /// Set inbound backpressure level
    pub fn set_inbound_level(&self, level: BackpressureLevel) {
        self.inbound_level.store(level as u64, Ordering::Relaxed);
    }

    /// Get inbound backpressure level
    pub fn inbound_level(&self) -> BackpressureLevel {
        match self.inbound_level.load(Ordering::Relaxed) {
            0 => BackpressureLevel::None,
            1 => BackpressureLevel::Light,
            2 => BackpressureLevel::Moderate,
            3 => BackpressureLevel::Heavy,
            _ => BackpressureLevel::Critical,
        }
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Queue a signal to send
    pub async fn queue_signal(&self, signal: BackpressureSignal) {
        self.pending_signals.lock().await.push(signal);
    }

    /// Get pending signals
    pub async fn take_pending_signals(&self) -> Vec<BackpressureSignal> {
        std::mem::take(&mut *self.pending_signals.lock().await)
    }

    /// Record received signal
    pub async fn receive_signal(&self, signal: BackpressureSignal) {
        let peer_id = signal.peer_id;
        self.received_signals.write().await.insert(peer_id, signal);
    }

    /// Get signal from peer
    pub async fn peer_signal(&self, peer_id: &[u8; 16]) -> Option<BackpressureSignal> {
        self.received_signals.read().await.get(peer_id).cloned()
    }

    /// Get effective rate multiplier for a peer
    pub async fn effective_rate_multiplier(&self, peer_id: &[u8; 16]) -> f64 {
        // Start with outbound level
        let mut multiplier = self.outbound_level().rate_multiplier();

        // Factor in peer's backpressure signal
        if let Some(signal) = self.peer_signal(peer_id).await {
            if signal.is_recent(5000) {
                multiplier = multiplier.min(signal.level.rate_multiplier());
            }
        }

        multiplier
    }

    /// Clean up old signals
    pub async fn cleanup_old_signals(&self, max_age_ms: u64) {
        let mut signals = self.received_signals.write().await;
        signals.retain(|_, signal| signal.is_recent(max_age_ms));
    }
}

// =============================================================================
// Congestion Control
// =============================================================================

/// Congestion control algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CongestionAlgorithm {
    /// No congestion control
    None,
    /// Additive Increase Multiplicative Decrease (AIMD)
    #[default]
    Aimd,
    /// TCP CUBIC (RFC 8312): the congestion window follows a cubic function
    /// of the time elapsed since the last congestion event, with a
    /// TCP-friendly linear region so CUBIC flows remain fair when they
    /// share a bottleneck with Reno/AIMD flows.
    Cubic,
    /// BBR (Bottleneck Bandwidth and RTT), following Cardwell et al.,
    /// "BBR: Congestion-Based Congestion Control" (ACM Queue, 2016). Builds
    /// an explicit model of the path (max-filtered delivery rate `BtlBw`,
    /// min-filtered RTT `RTprop`) and drives the window to
    /// `BtlBw * RTprop * gain` through the Startup / Drain / ProbeBW /
    /// ProbeRTT state machine. Note: this controller only sizes the
    /// congestion window; it does not implement BBR's packet-level pacer,
    /// since pacing requires a send-side scheduler this crate does not own.
    Bbr,
}

/// Congestion control configuration
#[derive(Debug, Clone)]
pub struct CongestionConfig {
    /// Algorithm to use
    pub algorithm: CongestionAlgorithm,
    /// Initial window size (bytes)
    pub initial_window: u64,
    /// Minimum window size (bytes)
    pub min_window: u64,
    /// Maximum window size (bytes)
    pub max_window: u64,
    /// AIMD: additive increase (bytes per RTT)
    pub aimd_increase: u64,
    /// AIMD: multiplicative decrease factor
    pub aimd_decrease: f64,
    /// RTT smoothing factor (0-1)
    pub rtt_alpha: f64,
}

impl Default for CongestionConfig {
    fn default() -> Self {
        Self {
            algorithm: CongestionAlgorithm::Aimd,
            initial_window: 64_000, // 64KB initial
            min_window: 4_000,      // 4KB minimum
            max_window: 16_000_000, // 16MB maximum
            aimd_increase: 16_000,  // 16KB per RTT
            aimd_decrease: 0.5,     // halve on congestion
            rtt_alpha: 0.125,       // RTT smoothing
        }
    }
}

/// Congestion state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CongestionState {
    /// Slow start phase
    #[default]
    SlowStart,
    /// Congestion avoidance phase
    CongestionAvoidance,
    /// Recovery phase after loss
    Recovery,
}

/// Segment size (bytes) used as CUBIC's and BBR's internal unit of account.
/// RFC 8312's constants (`C`, `beta_cubic`) are defined in units of
/// (typically 1460-byte) segments, so byte-denominated windows are converted
/// to/from segments with this constant to keep the growth dynamics faithful
/// to the RFC regardless of the actual MSS negotiated on the wire.
const CONGESTION_MSS: f64 = 1460.0;

/// RFC 8312 `C`: scaling constant controlling how aggressively CUBIC probes
/// for additional bandwidth.
const CUBIC_C: f64 = 0.4;

/// RFC 8312 `beta_cubic`: multiplicative window decrease factor applied on
/// congestion (less aggressive than AIMD's classic 0.5, per the RFC).
const CUBIC_BETA: f64 = 0.7;

/// Internal epoch state for TCP CUBIC (RFC 8312) congestion avoidance.
#[derive(Debug, Default)]
struct CubicState {
    /// Window size (bytes) recorded at the last congestion event (`W_max`).
    w_max: u64,
    /// Wall-clock origin of the current cubic epoch; set the first time
    /// congestion avoidance runs after (re)entering it, and cleared again on
    /// the next congestion event so a fresh epoch begins.
    epoch_start: Option<Instant>,
    /// Precomputed `K`: the time (seconds) at which `W_cubic(t)` returns to
    /// `w_max`, derived from `w_max` and the constants above.
    k: f64,
}

/// BBR operating phase (Startup / Drain / ProbeBW / ProbeRTT), mirroring the
/// state machine described in Cardwell et al., 2016.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BbrPhase {
    /// Exponential search for the bottleneck bandwidth.
    Startup,
    /// Drain the queue built up during Startup's overshoot.
    Drain,
    /// Steady state: cycle the pacing gain to probe for extra bandwidth
    /// while otherwise sending at the estimated bottleneck rate.
    ProbeBw,
    /// Periodically shrink the window to get an uninflated RTT sample.
    ProbeRtt,
}

/// Gain cycle applied to the pacing rate during `ProbeBw` (BBR v1): probe up
/// once, drain the resulting queue once, then cruise at the estimated rate.
const BBR_GAIN_CYCLE: [f64; 8] = [1.25, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
/// `cwnd` gain used in `ProbeBw` (2x the bandwidth-delay product, per BBR v1).
const BBR_PROBE_BW_CWND_GAIN: f64 = 2.0;
/// `Startup`/`Drain` gain: `2/ln(2)`, doubles the delivery rate each round.
const BBR_STARTUP_GAIN: f64 = 2.885_390_08;
/// How long BBR holds a minimal window in `ProbeRTT` before resuming.
const BBR_PROBE_RTT_DURATION: Duration = Duration::from_millis(200);
/// How often BBR forces a `ProbeRTT` round to refresh the RTprop estimate.
const BBR_PROBE_RTT_INTERVAL: Duration = Duration::from_secs(10);
/// Number of delivery-rate samples retained for the `BtlBw` max-filter.
const BBR_BW_WINDOW: usize = 16;

/// Internal model state for BBR: the max-filtered bandwidth (`BtlBw`) and
/// min-filtered RTT (`RTprop`) estimates, plus the state-machine phase used
/// to decide which gain to apply to the bandwidth-delay product.
#[derive(Debug)]
struct BbrModel {
    /// Current state-machine phase.
    phase: BbrPhase,
    /// Delivery-rate samples (bytes/sec), each timestamped so stale samples
    /// can be evicted from the windowed max-filter.
    bw_samples: std::collections::VecDeque<(Instant, f64)>,
    /// Windowed-min RTT estimate (`RTprop`) and when it was last refreshed.
    min_rtt: Option<Duration>,
    min_rtt_stamp: Instant,
    /// Bandwidth observed at the start of the current Startup round, used to
    /// detect the growth plateau that signals the pipe is full.
    startup_last_bw: f64,
    /// Consecutive Startup rounds without >=25% bandwidth growth.
    startup_plateau_rounds: u32,
    /// Index into [`BBR_GAIN_CYCLE`] for the current `ProbeBw` phase.
    cycle_index: usize,
    /// When the current gain-cycle phase (or Startup round) began.
    cycle_start: Instant,
    /// Deadline for leaving `ProbeRTT`, set once the phase is entered.
    probe_rtt_deadline: Option<Instant>,
}

impl BbrModel {
    fn new(now: Instant) -> Self {
        Self {
            phase: BbrPhase::Startup,
            bw_samples: std::collections::VecDeque::with_capacity(BBR_BW_WINDOW),
            min_rtt: None,
            min_rtt_stamp: now,
            startup_last_bw: 0.0,
            startup_plateau_rounds: 0,
            cycle_index: 0,
            cycle_start: now,
            probe_rtt_deadline: None,
        }
    }
}

/// Congestion controller
#[derive(Debug)]
pub struct CongestionController {
    /// Configuration
    config: CongestionConfig,
    /// Current congestion window (bytes)
    cwnd: AtomicU64,
    /// Slow start threshold
    ssthresh: AtomicU64,
    /// Current state
    state: Mutex<CongestionState>,
    /// Smoothed RTT (microseconds)
    srtt: AtomicU64,
    /// RTT variance
    rttvar: AtomicU64,
    /// Bytes in flight
    bytes_in_flight: AtomicU64,
    /// Packets acknowledged
    acks_received: AtomicU64,
    /// Packets lost
    packets_lost: AtomicU64,
    /// Last window reduction time
    last_reduction: Mutex<Option<Instant>>,
    /// CUBIC epoch state (only touched when `config.algorithm` is `Cubic`)
    cubic: Mutex<CubicState>,
    /// BBR path model (only touched when `config.algorithm` is `Bbr`)
    bbr: Mutex<BbrModel>,
}

impl CongestionController {
    /// Create new congestion controller
    pub fn new(config: CongestionConfig) -> Self {
        let now = Instant::now();
        Self {
            cwnd: AtomicU64::new(config.initial_window),
            ssthresh: AtomicU64::new(config.max_window),
            state: Mutex::new(CongestionState::SlowStart),
            srtt: AtomicU64::new(0),
            rttvar: AtomicU64::new(0),
            bytes_in_flight: AtomicU64::new(0),
            acks_received: AtomicU64::new(0),
            packets_lost: AtomicU64::new(0),
            last_reduction: Mutex::new(None),
            cubic: Mutex::new(CubicState::default()),
            bbr: Mutex::new(BbrModel::new(now)),
            config,
        }
    }

    /// Create with default config
    pub fn default_controller() -> Self {
        Self::new(CongestionConfig::default())
    }

    /// Get current congestion window
    pub fn cwnd(&self) -> u64 {
        self.cwnd.load(Ordering::Relaxed)
    }

    /// Get available window (can send this many bytes)
    pub fn available_window(&self) -> u64 {
        let cwnd = self.cwnd.load(Ordering::Relaxed);
        let in_flight = self.bytes_in_flight.load(Ordering::Relaxed);
        cwnd.saturating_sub(in_flight)
    }

    /// Check if can send bytes
    pub fn can_send(&self, bytes: u64) -> bool {
        self.available_window() >= bytes
    }

    /// Record bytes sent
    pub fn record_sent(&self, bytes: u64) {
        self.bytes_in_flight.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record bytes acknowledged
    pub async fn record_ack(&self, bytes: u64, rtt_us: u64) {
        // Update bytes in flight
        self.bytes_in_flight.fetch_sub(
            bytes.min(self.bytes_in_flight.load(Ordering::Relaxed)),
            Ordering::Relaxed,
        );
        self.acks_received.fetch_add(1, Ordering::Relaxed);

        // Update RTT
        self.update_rtt(rtt_us);

        // Update congestion window
        match self.config.algorithm {
            CongestionAlgorithm::None => {}
            CongestionAlgorithm::Aimd => self.aimd_ack(bytes).await,
            CongestionAlgorithm::Cubic => self.cubic_ack(bytes).await,
            CongestionAlgorithm::Bbr => self.bbr_ack(bytes, rtt_us).await,
        }
    }

    /// Record packet loss
    pub async fn record_loss(&self, bytes: u64) {
        self.bytes_in_flight.fetch_sub(
            bytes.min(self.bytes_in_flight.load(Ordering::Relaxed)),
            Ordering::Relaxed,
        );
        self.packets_lost.fetch_add(1, Ordering::Relaxed);

        match self.config.algorithm {
            CongestionAlgorithm::None => {}
            CongestionAlgorithm::Aimd => self.aimd_loss().await,
            CongestionAlgorithm::Cubic => self.cubic_loss().await,
            CongestionAlgorithm::Bbr => self.bbr_loss().await,
        }
    }

    /// Update RTT estimate
    fn update_rtt(&self, rtt_us: u64) {
        let srtt = self.srtt.load(Ordering::Relaxed);

        if srtt == 0 {
            // First RTT sample
            self.srtt.store(rtt_us, Ordering::Relaxed);
            self.rttvar.store(rtt_us / 2, Ordering::Relaxed);
        } else {
            // Exponential moving average
            let alpha = self.config.rtt_alpha;
            let new_srtt = ((1.0 - alpha) * srtt as f64 + alpha * rtt_us as f64) as u64;
            self.srtt.store(new_srtt, Ordering::Relaxed);

            // Update variance
            let rttvar = self.rttvar.load(Ordering::Relaxed);
            let diff = (rtt_us as i64 - srtt as i64).unsigned_abs();
            let new_rttvar = ((1.0 - alpha) * rttvar as f64 + alpha * diff as f64) as u64;
            self.rttvar.store(new_rttvar, Ordering::Relaxed);
        }
    }

    /// AIMD: handle ACK
    async fn aimd_ack(&self, bytes: u64) {
        let cwnd = self.cwnd.load(Ordering::Relaxed);
        let ssthresh = self.ssthresh.load(Ordering::Relaxed);
        let mut state = self.state.lock().await;

        let new_cwnd = match *state {
            CongestionState::SlowStart => {
                // Exponential growth
                let new = cwnd + bytes;
                if new >= ssthresh {
                    *state = CongestionState::CongestionAvoidance;
                }
                new
            }
            CongestionState::CongestionAvoidance => {
                // Linear growth
                cwnd + (self.config.aimd_increase * bytes / cwnd).max(1)
            }
            CongestionState::Recovery => {
                // Stay in recovery, linear growth
                cwnd + (self.config.aimd_increase * bytes / cwnd).max(1)
            }
        };

        self.cwnd.store(
            new_cwnd.clamp(self.config.min_window, self.config.max_window),
            Ordering::Relaxed,
        );
    }

    /// AIMD: handle loss
    async fn aimd_loss(&self) {
        // Check if we recently reduced
        let mut last_reduction = self.last_reduction.lock().await;
        if let Some(last) = *last_reduction {
            if last.elapsed() < Duration::from_millis(100) {
                return; // Don't reduce too frequently
            }
        }
        *last_reduction = Some(Instant::now());
        drop(last_reduction);

        let cwnd = self.cwnd.load(Ordering::Relaxed);

        // Multiplicative decrease
        let new_cwnd = (cwnd as f64 * self.config.aimd_decrease) as u64;
        let new_cwnd = new_cwnd.clamp(self.config.min_window, self.config.max_window);

        self.cwnd.store(new_cwnd, Ordering::Relaxed);
        self.ssthresh.store(new_cwnd, Ordering::Relaxed);

        *self.state.lock().await = CongestionState::Recovery;
    }

    /// CUBIC: handle ACK.
    ///
    /// Implements RFC 8312's window-growth function directly: slow start is
    /// shared with AIMD (exponential growth until `ssthresh`), but once in
    /// congestion avoidance the window follows `W_cubic(t) = C*(t-K)^3 +
    /// W_max`, where `t` is the time elapsed since the last congestion event
    /// and `K` is chosen so the curve reaches `W_max` again at `t = K`. The
    /// RFC's TCP-friendly region (`W_est`) is also evaluated so CUBIC never
    /// grows slower than standard Reno/AIMD would, preserving fairness.
    async fn cubic_ack(&self, bytes: u64) {
        let cwnd = self.cwnd.load(Ordering::Relaxed);
        let ssthresh = self.ssthresh.load(Ordering::Relaxed);
        let mut state = self.state.lock().await;

        let new_cwnd = match *state {
            CongestionState::SlowStart => {
                // Exponential growth, identical in spirit to AIMD/Reno slow
                // start (RFC 8312 does not redefine slow start).
                let new = cwnd + bytes;
                if new >= ssthresh {
                    *state = CongestionState::CongestionAvoidance;
                }
                new
            }
            CongestionState::CongestionAvoidance | CongestionState::Recovery => {
                // A loss's Recovery ends as soon as a new ACK arrives; fold
                // back into ordinary congestion avoidance.
                *state = CongestionState::CongestionAvoidance;
                self.cubic_window(cwnd).await
            }
        };

        self.cwnd.store(
            new_cwnd.clamp(self.config.min_window, self.config.max_window),
            Ordering::Relaxed,
        );
    }

    /// CUBIC: compute the RFC 8312 target window for the current epoch.
    async fn cubic_window(&self, cwnd: u64) -> u64 {
        let now = Instant::now();
        let mut cubic = self.cubic.lock().await;

        if cubic.epoch_start.is_none() {
            // Starting a fresh epoch. If we haven't recorded a congestion
            // event yet (e.g. this is the first transition out of slow
            // start), anchor W_max at the current window so the curve
            // begins flat rather than jumping.
            if cubic.w_max == 0 {
                cubic.w_max = cwnd.max(1);
            }
            let w_max_segments = cubic.w_max as f64 / CONGESTION_MSS;
            cubic.k = (w_max_segments * (1.0 - CUBIC_BETA) / CUBIC_C).cbrt();
            cubic.epoch_start = Some(now);
        }

        let epoch_start = cubic
            .epoch_start
            .expect("epoch_start is set unconditionally above");
        let w_max = cubic.w_max;
        let k = cubic.k;
        drop(cubic);

        let t = epoch_start.elapsed().as_secs_f64();
        let w_max_segments = w_max as f64 / CONGESTION_MSS;
        let w_cubic_segments = CUBIC_C * (t - k).powi(3) + w_max_segments;
        let w_cubic = (w_cubic_segments * CONGESTION_MSS).max(0.0);

        // TCP-friendly region (RFC 8312 §4.2): the window a standard
        // AIMD/Reno flow would reach over the same interval. CUBIC uses
        // whichever of the two curves is larger so it never grows slower
        // than Reno when they compete for the same bottleneck.
        let srtt_s = (self.srtt() as f64 / 1_000_000.0).max(0.001);
        let w_est = w_max as f64 * CUBIC_BETA
            + (3.0 * (1.0 - CUBIC_BETA) / (1.0 + CUBIC_BETA)) * (t / srtt_s) * CONGESTION_MSS;

        // The window never shrinks on an ACK; a target below the current
        // cwnd just means neither curve has caught up yet.
        w_cubic.max(w_est).max(cwnd as f64) as u64
    }

    /// CUBIC: handle loss.
    ///
    /// Applies RFC 8312's multiplicative decrease (`beta_cubic = 0.7`,
    /// gentler than AIMD's classic 0.5) and records the pre-reduction window
    /// as the new `W_max`, then clears the epoch so the next ACK recomputes
    /// `K` from this fresh reference point.
    async fn cubic_loss(&self) {
        let mut last_reduction = self.last_reduction.lock().await;
        if let Some(last) = *last_reduction {
            if last.elapsed() < Duration::from_millis(100) {
                return; // Don't reduce too frequently
            }
        }
        *last_reduction = Some(Instant::now());
        drop(last_reduction);

        let cwnd = self.cwnd.load(Ordering::Relaxed);
        let new_cwnd = (cwnd as f64 * CUBIC_BETA) as u64;
        let new_cwnd = new_cwnd.clamp(self.config.min_window, self.config.max_window);

        self.cwnd.store(new_cwnd, Ordering::Relaxed);
        self.ssthresh.store(new_cwnd, Ordering::Relaxed);

        {
            let mut cubic = self.cubic.lock().await;
            cubic.w_max = cwnd;
            cubic.epoch_start = None;
        }

        *self.state.lock().await = CongestionState::Recovery;
    }

    /// BBR: fold one ACK sample into the bandwidth/RTT model and recompute
    /// `cwnd` as `BDP * gain`, following Cardwell et al., "BBR:
    /// Congestion-Based Congestion Control" (ACM Queue, 2016).
    ///
    /// This drives the window from the same `BtlBw` (max-filtered delivery
    /// rate) / `RTprop` (min-filtered RTT) model and Startup -> Drain ->
    /// ProbeBW -> ProbeRTT state machine as the paper. It intentionally does
    /// not implement packet-level pacing: this controller only exposes a
    /// congestion window, not a send-side pacer, so the pacing gain informs
    /// window sizing rather than an actual paced send rate.
    async fn bbr_ack(&self, bytes: u64, rtt_us: u64) {
        if rtt_us == 0 || bytes == 0 {
            return; // No usable delivery-rate sample.
        }

        let now = Instant::now();
        let rtt = Duration::from_micros(rtt_us);
        let delivery_rate = bytes as f64 / (rtt_us as f64 / 1_000_000.0);

        let mut model = self.bbr.lock().await;

        // --- RTprop: windowed-min RTT filter ---
        let is_new_min = match model.min_rtt {
            None => true,
            Some(m) => rtt < m,
        };
        let stale = now.duration_since(model.min_rtt_stamp) > BBR_PROBE_RTT_INTERVAL;
        if is_new_min {
            model.min_rtt = Some(rtt);
            model.min_rtt_stamp = now;
        } else if stale && model.phase != BbrPhase::ProbeRtt {
            // The RTprop estimate is stale; force a ProbeRTT round to get an
            // uninflated sample instead of trusting a possibly-queued RTT.
            model.phase = BbrPhase::ProbeRtt;
            model.probe_rtt_deadline = None;
        }

        // --- BtlBw: windowed-max delivery-rate filter ---
        model.bw_samples.push_back((now, delivery_rate));
        while model.bw_samples.len() > BBR_BW_WINDOW {
            model.bw_samples.pop_front();
        }
        let window_horizon = model.min_rtt.unwrap_or(Duration::from_millis(100)) * 10;
        while let Some(&(ts, _)) = model.bw_samples.front() {
            if now.duration_since(ts) > window_horizon && model.bw_samples.len() > 1 {
                model.bw_samples.pop_front();
            } else {
                break;
            }
        }
        let btlbw = model
            .bw_samples
            .iter()
            .map(|(_, bw)| *bw)
            .fold(0.0_f64, f64::max);
        let rtprop = model.min_rtt.unwrap_or(rtt).as_secs_f64().max(0.0001);
        let bdp = (btlbw * rtprop) as u64;

        // --- State machine: choose this round's cwnd gain ---
        let cwnd_gain = match model.phase {
            BbrPhase::Startup => {
                // Exit Startup once BtlBw plateaus (<25% growth) for 3
                // consecutive ~RTT-spaced rounds: the pipe is full.
                if now.duration_since(model.cycle_start) >= rtt {
                    if btlbw < model.startup_last_bw * 1.25 {
                        model.startup_plateau_rounds += 1;
                    } else {
                        model.startup_plateau_rounds = 0;
                    }
                    model.startup_last_bw = btlbw;
                    model.cycle_start = now;
                    if model.startup_plateau_rounds >= 3 {
                        model.phase = BbrPhase::Drain;
                    }
                }
                BBR_STARTUP_GAIN
            }
            BbrPhase::Drain => {
                let in_flight = self.bytes_in_flight.load(Ordering::Relaxed);
                if in_flight as f64 <= bdp as f64 {
                    model.phase = BbrPhase::ProbeBw;
                    model.cycle_index = 0;
                    model.cycle_start = now;
                }
                // Drain the Startup overshoot: shrink the window below BDP.
                1.0 / BBR_STARTUP_GAIN
            }
            BbrPhase::ProbeBw => {
                if now.duration_since(model.cycle_start) >= rtt.max(Duration::from_micros(1)) {
                    model.cycle_index = (model.cycle_index + 1) % BBR_GAIN_CYCLE.len();
                    model.cycle_start = now;
                }
                if now.duration_since(model.min_rtt_stamp) > BBR_PROBE_RTT_INTERVAL {
                    model.phase = BbrPhase::ProbeRtt;
                    model.probe_rtt_deadline = None;
                }
                // This controller has no separate pacing-rate knob, so the
                // gain-cycle (which BBR v1 applies to pacing rate) is folded
                // into the window gain directly: probing phases visibly
                // grow/shrink the window around the steady BDP*2 target.
                BBR_GAIN_CYCLE[model.cycle_index] * BBR_PROBE_BW_CWND_GAIN
            }
            BbrPhase::ProbeRtt => {
                let deadline = *model
                    .probe_rtt_deadline
                    .get_or_insert(now + BBR_PROBE_RTT_DURATION);
                if now >= deadline {
                    model.phase = BbrPhase::ProbeBw;
                    model.cycle_index = 0;
                    model.cycle_start = now;
                    model.probe_rtt_deadline = None;
                    model.min_rtt_stamp = now; // Fresh RTprop sample taken.
                }
                0.0 // Overridden below: ProbeRTT clamps to a minimal window.
            }
        };

        let final_phase = model.phase;
        drop(model);

        let target = if final_phase == BbrPhase::ProbeRtt {
            // Hold a minimal window (4 segments) so queued bytes drain and
            // the next RTT sample reflects true propagation delay.
            (4.0 * CONGESTION_MSS) as u64
        } else {
            (bdp as f64 * cwnd_gain) as u64
        };

        self.cwnd.store(
            target.clamp(self.config.min_window, self.config.max_window),
            Ordering::Relaxed,
        );

        let mapped_state = match final_phase {
            BbrPhase::Startup => CongestionState::SlowStart,
            BbrPhase::Drain | BbrPhase::ProbeBw => CongestionState::CongestionAvoidance,
            BbrPhase::ProbeRtt => CongestionState::Recovery,
        };
        *self.state.lock().await = mapped_state;
    }

    /// BBR: handle loss.
    ///
    /// Unlike AIMD/CUBIC, BBR deliberately does not multiplicatively cut
    /// `cwnd` in response to an isolated packet loss: its control loop is
    /// bandwidth-model-driven, not loss-driven (Cardwell et al. 2016, §3).
    /// `packets_lost` is still tracked by the caller for observability, and
    /// sustained loss will show up as reduced deliveries and shrink the
    /// `BtlBw` max-filter naturally through subsequent `bbr_ack` samples.
    async fn bbr_loss(&self) {
        tracing::debug!(
            "BBR congestion controller observed a packet loss; cwnd is model-driven \
             (BtlBw * RTprop * gain) and is intentionally not cut on isolated loss"
        );
    }

    /// Get smoothed RTT (microseconds)
    pub fn srtt(&self) -> u64 {
        self.srtt.load(Ordering::Relaxed)
    }

    /// Get RTT variance
    pub fn rttvar(&self) -> u64 {
        self.rttvar.load(Ordering::Relaxed)
    }

    /// Get current state
    pub async fn state(&self) -> CongestionState {
        *self.state.lock().await
    }

    /// Get statistics
    pub fn stats(&self) -> CongestionStats {
        CongestionStats {
            cwnd: self.cwnd.load(Ordering::Relaxed),
            ssthresh: self.ssthresh.load(Ordering::Relaxed),
            bytes_in_flight: self.bytes_in_flight.load(Ordering::Relaxed),
            srtt_us: self.srtt.load(Ordering::Relaxed),
            rttvar_us: self.rttvar.load(Ordering::Relaxed),
            acks_received: self.acks_received.load(Ordering::Relaxed),
            packets_lost: self.packets_lost.load(Ordering::Relaxed),
        }
    }

    /// Reset controller
    pub async fn reset(&self) {
        self.cwnd
            .store(self.config.initial_window, Ordering::Relaxed);
        self.ssthresh
            .store(self.config.max_window, Ordering::Relaxed);
        self.bytes_in_flight.store(0, Ordering::Relaxed);
        self.srtt.store(0, Ordering::Relaxed);
        self.rttvar.store(0, Ordering::Relaxed);
        *self.state.lock().await = CongestionState::SlowStart;
        *self.last_reduction.lock().await = None;
        *self.cubic.lock().await = CubicState::default();
        *self.bbr.lock().await = BbrModel::new(Instant::now());
    }
}

/// Congestion statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CongestionStats {
    pub cwnd: u64,
    pub ssthresh: u64,
    pub bytes_in_flight: u64,
    pub srtt_us: u64,
    pub rttvar_us: u64,
    pub acks_received: u64,
    pub packets_lost: u64,
}

// =============================================================================
// Flow Controller (Combined)
// =============================================================================

/// Combined flow controller for a connection
pub struct FlowController {
    /// Token bucket for rate limiting
    pub rate_limiter: TokenBucket,
    /// Backpressure controller
    pub backpressure: BackpressureController,
    /// Congestion controller
    pub congestion: CongestionController,
    /// Connection identifier
    pub peer_id: [u8; 16],
}

impl FlowController {
    /// Create new flow controller
    pub fn new(
        peer_id: [u8; 16],
        rate_config: TokenBucketConfig,
        congestion_config: CongestionConfig,
    ) -> Self {
        Self {
            rate_limiter: TokenBucket::new(rate_config),
            backpressure: BackpressureController::new(),
            congestion: CongestionController::new(congestion_config),
            peer_id,
        }
    }

    /// Create with default configs
    pub fn default_controller(peer_id: [u8; 16]) -> Self {
        Self::new(
            peer_id,
            TokenBucketConfig::default(),
            CongestionConfig::default(),
        )
    }

    /// Check if can send bytes (combines all flow control)
    pub async fn can_send(&self, bytes: u64) -> bool {
        // Check backpressure
        if self.backpressure.is_paused() {
            return false;
        }

        // Check congestion window
        if !self.congestion.can_send(bytes) {
            return false;
        }

        // Check rate limiter
        self.rate_limiter.try_consume(bytes).await
    }

    /// Wait until can send (blocking)
    pub async fn wait_to_send(&self, bytes: u64) {
        // Wait for backpressure to clear
        while self.backpressure.is_paused() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Wait for congestion window
        while !self.congestion.can_send(bytes) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // Wait for rate limiter
        self.rate_limiter.consume(bytes).await;
    }

    /// Record successful send
    pub fn record_sent(&self, bytes: u64) {
        self.congestion.record_sent(bytes);
    }

    /// Record acknowledgment
    pub async fn record_ack(&self, bytes: u64, rtt_us: u64) {
        self.congestion.record_ack(bytes, rtt_us).await;
    }

    /// Record loss
    pub async fn record_loss(&self, bytes: u64) {
        self.congestion.record_loss(bytes).await;
    }

    /// Get effective send rate (bytes/sec)
    pub async fn effective_rate(&self) -> u64 {
        let base_rate = self.rate_limiter.refill_rate();
        let multiplier = self
            .backpressure
            .effective_rate_multiplier(&self.peer_id)
            .await;
        (base_rate as f64 * multiplier) as u64
    }

    /// Get flow control summary
    pub fn summary(&self) -> FlowControlSummary {
        FlowControlSummary {
            rate_limiter_available: self.rate_limiter.available(),
            rate_limiter_capacity: self.rate_limiter.capacity(),
            backpressure_level: self.backpressure.outbound_level(),
            is_paused: self.backpressure.is_paused(),
            congestion_stats: self.congestion.stats(),
        }
    }
}

/// Flow control summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowControlSummary {
    pub rate_limiter_available: u64,
    pub rate_limiter_capacity: u64,
    pub backpressure_level: BackpressureLevel,
    pub is_paused: bool,
    pub congestion_stats: CongestionStats,
}

// =============================================================================
// Shared Flow Controller
// =============================================================================

/// Thread-safe flow controller
pub type SharedFlowController = Arc<FlowController>;

/// Create a shared flow controller
pub fn shared_flow_controller(peer_id: [u8; 16]) -> SharedFlowController {
    Arc::new(FlowController::default_controller(peer_id))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Token Bucket Tests

    #[tokio::test]
    async fn test_token_bucket_creation() {
        let bucket = TokenBucket::default_bucket();
        assert!(bucket.available() > 0);
        assert!(bucket.fill_ratio() > 0.0);
    }

    #[tokio::test]
    async fn test_token_bucket_consume() {
        let config = TokenBucketConfig {
            capacity: 1000,
            refill_rate: 100,
            initial_tokens: 1000,
        };
        let bucket = TokenBucket::new(config);

        // Should succeed
        assert!(bucket.try_consume(500).await);
        assert_eq!(bucket.available(), 500);

        // Should succeed
        assert!(bucket.try_consume(500).await);
        assert_eq!(bucket.available(), 0);

        // Should fail - no tokens
        assert!(!bucket.try_consume(1).await);
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let config = TokenBucketConfig {
            capacity: 1000,
            refill_rate: 10000, // 10000 tokens/sec
            initial_tokens: 0,
        };
        let bucket = TokenBucket::new(config);

        // Wait for refill (50ms at 10000/sec = ~500 tokens)
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to consume - this triggers refill and should succeed
        // After 100ms at 10000 tokens/sec, we should have ~1000 tokens
        let result = bucket.try_consume(100).await;
        assert!(result, "Should have tokens after refill period");
    }

    #[test]
    fn test_token_bucket_presets() {
        let high = TokenBucketConfig::high_throughput();
        assert!(high.refill_rate > TokenBucketConfig::default().refill_rate);

        let low = TokenBucketConfig::low_latency();
        assert!(low.capacity < TokenBucketConfig::default().capacity);

        let embedded = TokenBucketConfig::embedded();
        assert!(embedded.refill_rate < low.refill_rate);
    }

    // Backpressure Tests

    #[test]
    fn test_backpressure_level_multiplier() {
        assert_eq!(BackpressureLevel::None.rate_multiplier(), 1.0);
        assert!(BackpressureLevel::Light.rate_multiplier() < 1.0);
        assert!(BackpressureLevel::Critical.rate_multiplier() == 0.0);
    }

    #[test]
    fn test_backpressure_from_fill_ratio() {
        assert_eq!(
            BackpressureLevel::from_fill_ratio(0.3),
            BackpressureLevel::None
        );
        assert_eq!(
            BackpressureLevel::from_fill_ratio(0.6),
            BackpressureLevel::Light
        );
        assert_eq!(
            BackpressureLevel::from_fill_ratio(0.8),
            BackpressureLevel::Moderate
        );
        assert_eq!(
            BackpressureLevel::from_fill_ratio(0.9),
            BackpressureLevel::Heavy
        );
        assert_eq!(
            BackpressureLevel::from_fill_ratio(0.98),
            BackpressureLevel::Critical
        );
    }

    #[test]
    fn test_backpressure_signal() {
        let signal = BackpressureSignal::new(
            [1u8; 16],
            BackpressureLevel::Moderate,
            BackpressureReason::QueueFull,
        );

        assert_eq!(signal.level, BackpressureLevel::Moderate);
        assert!(signal.is_recent(1000));
    }

    #[tokio::test]
    async fn test_backpressure_controller() {
        let controller = BackpressureController::new();

        assert_eq!(controller.outbound_level(), BackpressureLevel::None);
        assert!(!controller.is_paused());

        controller.set_outbound_level(BackpressureLevel::Heavy);
        assert_eq!(controller.outbound_level(), BackpressureLevel::Heavy);
        assert!(!controller.is_paused());

        controller.set_outbound_level(BackpressureLevel::Critical);
        assert!(controller.is_paused());
    }

    #[tokio::test]
    async fn test_backpressure_signals() {
        let controller = BackpressureController::new();
        let peer_id = [1u8; 16];

        let signal = BackpressureSignal::new(
            peer_id,
            BackpressureLevel::Moderate,
            BackpressureReason::NetworkCongestion,
        );

        controller.receive_signal(signal).await;

        let received = controller.peer_signal(&peer_id).await;
        assert!(received.is_some());
        assert_eq!(received.unwrap().level, BackpressureLevel::Moderate);
    }

    // Congestion Control Tests

    #[tokio::test]
    async fn test_congestion_controller_creation() {
        let controller = CongestionController::default_controller();

        assert!(controller.cwnd() > 0);
        assert_eq!(controller.state().await, CongestionState::SlowStart);
    }

    #[tokio::test]
    async fn test_congestion_window() {
        let controller = CongestionController::default_controller();

        let initial = controller.cwnd();

        // Record sent
        controller.record_sent(1000);
        assert_eq!(controller.available_window(), initial - 1000);

        // Record ack
        controller.record_ack(1000, 10000).await;
        assert!(controller.cwnd() >= initial); // Window should grow
    }

    #[tokio::test]
    async fn test_congestion_loss() {
        let controller = CongestionController::default_controller();

        let initial = controller.cwnd();

        // Record loss
        controller.record_loss(1000).await;

        // Window should shrink
        assert!(controller.cwnd() < initial);
        assert_eq!(controller.state().await, CongestionState::Recovery);
    }

    #[tokio::test]
    async fn test_congestion_rtt() {
        let controller = CongestionController::default_controller();

        controller.record_ack(1000, 5000).await;
        assert!(controller.srtt() > 0);

        controller.record_ack(1000, 6000).await;
        // RTT should be smoothed
        let srtt = controller.srtt();
        assert!(srtt > 5000 && srtt < 6000);
    }

    #[test]
    fn test_congestion_stats() {
        let controller = CongestionController::default_controller();
        let stats = controller.stats();

        assert!(stats.cwnd > 0);
        assert_eq!(stats.bytes_in_flight, 0);
    }

    // CUBIC / BBR honesty tests: prove these algorithms run their own real
    // logic rather than silently falling back to the AIMD codepath.

    fn congestion_config_with_algorithm(algorithm: CongestionAlgorithm) -> CongestionConfig {
        CongestionConfig {
            algorithm,
            initial_window: 4_000,
            min_window: 200,
            max_window: 16_000_000,
            aimd_increase: 16_000,
            aimd_decrease: 0.5,
            rtt_alpha: 0.125,
        }
    }

    #[tokio::test]
    async fn test_cubic_loss_uses_rfc8312_beta_not_aimd_half() {
        let aimd =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Aimd));
        let cubic =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Cubic));

        aimd.record_loss(1000).await;
        cubic.record_loss(1000).await;

        // AIMD halves the window (factor 0.5); CUBIC applies RFC 8312's
        // gentler beta_cubic = 0.7. If CUBIC silently reused AIMD's loss
        // handler these two would be equal.
        assert_eq!(aimd.cwnd(), (4_000_f64 * 0.5) as u64);
        assert_eq!(cubic.cwnd(), (4_000_f64 * 0.7) as u64);
        assert_ne!(
            aimd.cwnd(),
            cubic.cwnd(),
            "CUBIC must not fabricate AIMD's multiplicative-decrease factor"
        );
    }

    #[tokio::test]
    async fn test_cubic_growth_diverges_from_aimd_growth() {
        let aimd =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Aimd));
        let cubic =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Cubic));

        for controller in [&aimd, &cubic] {
            controller.record_loss(1000).await;
        }

        // Let real wall-clock time pass so CUBIC's epoch timer (t in
        // W_cubic(t)) advances; AIMD's growth rule has no time dependence.
        tokio::time::sleep(Duration::from_millis(250)).await;

        for controller in [&aimd, &cubic] {
            controller.record_ack(1000, 20_000).await;
        }

        assert_ne!(
            aimd.cwnd(),
            cubic.cwnd(),
            "CUBIC's post-loss regrowth must follow its own cubic/TCP-friendly \
             curve, not AIMD's additive-increase rule"
        );
    }

    #[tokio::test]
    async fn test_bbr_ignores_isolated_loss() {
        let controller =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Bbr));
        let initial = controller.cwnd();

        controller.record_loss(1000).await;

        // Real BBR is bandwidth-model-driven, not loss-driven: an isolated
        // loss must not multiplicatively cut cwnd the way AIMD/CUBIC do.
        assert_eq!(
            controller.cwnd(),
            initial,
            "BBR must not fabricate AIMD's loss-triggered window cut"
        );
    }

    #[tokio::test]
    async fn test_bbr_cwnd_tracks_bandwidth_delay_product() {
        let controller =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Bbr));

        // A single high-bandwidth, low-RTT delivery sample: 100_000 bytes
        // acknowledged over a 10ms RTT implies ~10MB/s of delivered
        // bandwidth.
        controller.record_sent(100_000);
        controller.record_ack(100_000, 10_000).await;

        // AIMD's slow-start rule would give exactly initial_window + bytes
        // = 4_000 + 100_000 = 104_000. BBR must instead size the window
        // from BtlBw * RTprop * gain (Startup gain ~2.885), which is
        // substantially larger here and independent of the AIMD formula.
        let cwnd = controller.cwnd();
        assert!(
            cwnd > 200_000,
            "BBR cwnd ({cwnd}) should scale with bandwidth * RTT * gain, not \
             AIMD's additive '+= bytes' rule"
        );
    }

    #[tokio::test]
    async fn test_bbr_state_reflects_startup_phase() {
        let controller =
            CongestionController::new(congestion_config_with_algorithm(CongestionAlgorithm::Bbr));

        controller.record_ack(1_000, 10_000).await;

        // BBR starts in its Startup phase, which is projected onto the
        // shared CongestionState::SlowStart for API compatibility.
        assert_eq!(controller.state().await, CongestionState::SlowStart);
    }

    // Flow Controller Tests

    #[tokio::test]
    async fn test_flow_controller_creation() {
        let controller = FlowController::default_controller([1u8; 16]);

        assert!(!controller.backpressure.is_paused());
        assert!(controller.rate_limiter.available() > 0);
    }

    #[tokio::test]
    async fn test_flow_controller_can_send() {
        let controller = FlowController::default_controller([1u8; 16]);

        // Should be able to send small amount
        assert!(controller.can_send(1000).await);

        // Set critical backpressure
        controller
            .backpressure
            .set_outbound_level(BackpressureLevel::Critical);
        assert!(!controller.can_send(1000).await);
    }

    #[tokio::test]
    async fn test_flow_controller_effective_rate() {
        let controller = FlowController::default_controller([1u8; 16]);

        let rate1 = controller.effective_rate().await;

        controller
            .backpressure
            .set_outbound_level(BackpressureLevel::Moderate);
        let rate2 = controller.effective_rate().await;

        assert!(rate2 < rate1);
    }

    #[test]
    fn test_flow_control_summary() {
        let controller = FlowController::default_controller([1u8; 16]);
        let summary = controller.summary();

        assert!(summary.rate_limiter_capacity > 0);
        assert_eq!(summary.backpressure_level, BackpressureLevel::None);
        assert!(!summary.is_paused);
    }

    #[test]
    fn test_shared_flow_controller() {
        let controller = shared_flow_controller([1u8; 16]);
        assert!(!controller.backpressure.is_paused());
    }

    #[tokio::test]
    async fn test_flow_controller_record_operations() {
        let controller = FlowController::default_controller([1u8; 16]);

        controller.record_sent(1000);
        controller.record_ack(1000, 5000).await;

        let stats = controller.congestion.stats();
        assert_eq!(stats.acks_received, 1);
    }
}
