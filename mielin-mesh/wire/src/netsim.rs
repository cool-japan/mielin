//! Network condition simulator for MielinMesh Wire Protocol.
//!
//! Provides deterministic, reproducible simulation of adverse network conditions:
//! latency, jitter, packet loss, duplication, corruption, and bandwidth throttling.
//!
//! The simulator is self-contained: no `rand` dependency, no external I/O seam
//! required. It operates on raw byte payloads and can be injected at the
//! `QuicConnection::send`/`receive` boundary for integration tests, or used
//! directly in unit tests via the synchronous `decide`/`batch_decide` API.

use std::time::{Duration, Instant};

// ─── Deterministic PRNG ──────────────────────────────────────────────────────

/// Deterministic XorShift64 PRNG — reproducible in tests.
///
/// The period is 2^64 - 1. Seed must be non-zero; a zero seed would freeze
/// the generator at zero forever.
#[derive(Debug, Clone, Copy)]
pub struct Xorshift64(pub u64);

impl Xorshift64 {
    /// Create a new generator. Panics if `seed == 0`.
    pub fn new(seed: u64) -> Self {
        assert!(seed != 0, "xorshift64 seed must be non-zero");
        Self(seed)
    }

    /// Advance the state and return the next pseudo-random `u64`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Returns a float uniformly distributed in `[0.0, 1.0)`.
    ///
    /// Uses the upper 53 bits (IEEE-754 mantissa width) for accuracy.
    pub fn next_f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Returns a [`Duration`] uniformly distributed in `[0, max_ms)` milliseconds.
    ///
    /// If `max_ms == 0` returns `Duration::ZERO`.
    pub fn next_duration_ms(&mut self, max_ms: u64) -> Duration {
        if max_ms == 0 {
            return Duration::ZERO;
        }
        Duration::from_millis(self.next() % max_ms)
    }
}

// ─── NetworkConditions ───────────────────────────────────────────────────────

/// Parameters that describe a simulated network link.
#[derive(Debug, Clone)]
pub struct NetworkConditions {
    /// Fixed base latency added to every delivered packet.
    pub latency: Duration,
    /// Upper bound on random jitter: actual jitter drawn uniformly from `[0, jitter)`.
    pub jitter: Duration,
    /// Probability `[0.0, 1.0]` that a packet is silently dropped.
    pub packet_loss_rate: f64,
    /// Probability `[0.0, 1.0]` that a packet is delivered twice.
    pub duplicate_rate: f64,
    /// Probability `[0.0, 1.0]` that the first payload byte is XOR-flipped (`^ 0xFF`).
    pub corruption_rate: f64,
    /// Maximum bandwidth in bytes/second. `0` means unlimited.
    pub bandwidth_bps: u64,
}

impl NetworkConditions {
    /// Ideal channel: zero latency, no impairments, unlimited bandwidth.
    pub fn perfect() -> Self {
        Self {
            latency: Duration::ZERO,
            jitter: Duration::ZERO,
            packet_loss_rate: 0.0,
            duplicate_rate: 0.0,
            corruption_rate: 0.0,
            bandwidth_bps: 0,
        }
    }

    /// Typical LAN: 1 ms latency, 0.1 ms jitter, no loss.
    pub fn lan() -> Self {
        Self {
            latency: Duration::from_millis(1),
            jitter: Duration::from_micros(100),
            packet_loss_rate: 0.0,
            duplicate_rate: 0.0,
            corruption_rate: 0.0,
            bandwidth_bps: 0,
        }
    }

    /// Typical WAN / internet link: 50 ms latency, 20 ms jitter, 1 % loss.
    pub fn wan() -> Self {
        Self {
            latency: Duration::from_millis(50),
            jitter: Duration::from_millis(20),
            packet_loss_rate: 0.01,
            duplicate_rate: 0.0,
            corruption_rate: 0.0,
            bandwidth_bps: 0,
        }
    }

    /// Deliberately lossy link: 100 ms latency, 50 ms jitter, 20 % loss, 2 % duplication.
    pub fn lossy() -> Self {
        Self {
            latency: Duration::from_millis(100),
            jitter: Duration::from_millis(50),
            packet_loss_rate: 0.20,
            duplicate_rate: 0.02,
            corruption_rate: 0.0,
            bandwidth_bps: 0,
        }
    }

    /// Geostationary satellite: 600 ms latency, 100 ms jitter, 2 % loss.
    pub fn satellite() -> Self {
        Self {
            latency: Duration::from_millis(600),
            jitter: Duration::from_millis(100),
            packet_loss_rate: 0.02,
            duplicate_rate: 0.0,
            corruption_rate: 0.0,
            bandwidth_bps: 0,
        }
    }

    /// Build a custom condition from three primary impairment parameters.
    pub fn custom(latency_ms: u64, jitter_ms: u64, loss_pct: f64) -> Self {
        Self {
            latency: Duration::from_millis(latency_ms),
            jitter: Duration::from_millis(jitter_ms),
            packet_loss_rate: loss_pct.clamp(0.0, 1.0),
            duplicate_rate: 0.0,
            corruption_rate: 0.0,
            bandwidth_bps: 0,
        }
    }
}

// ─── SimulationAction ────────────────────────────────────────────────────────

/// The fate of a single packet after the simulator has evaluated it.
#[derive(Debug, Clone, PartialEq)]
pub enum SimulationAction {
    /// Deliver the payload after the given delay.
    Deliver { payload: Vec<u8>, delay: Duration },
    /// Silently discard the packet.
    Drop,
    /// Deliver a duplicate copy after the given delay.
    Duplicate { payload: Vec<u8>, delay: Duration },
    /// Deliver a corrupted copy (first byte XOR'd with `0xFF`) after the given delay.
    Corrupt { payload: Vec<u8>, delay: Duration },
}

impl SimulationAction {
    /// Returns `true` if this action results in any delivery (including duplicate/corrupt).
    pub fn is_delivered(&self) -> bool {
        !matches!(self, SimulationAction::Drop)
    }

    /// Extract the payload reference regardless of variant (returns `None` for `Drop`).
    pub fn payload(&self) -> Option<&[u8]> {
        match self {
            SimulationAction::Deliver { payload, .. }
            | SimulationAction::Duplicate { payload, .. }
            | SimulationAction::Corrupt { payload, .. } => Some(payload),
            SimulationAction::Drop => None,
        }
    }

    /// Extract the delay regardless of variant (returns `Duration::ZERO` for `Drop`).
    pub fn delay(&self) -> Duration {
        match self {
            SimulationAction::Deliver { delay, .. }
            | SimulationAction::Duplicate { delay, .. }
            | SimulationAction::Corrupt { delay, .. } => *delay,
            SimulationAction::Drop => Duration::ZERO,
        }
    }
}

// ─── NetworkSimStats ─────────────────────────────────────────────────────────

/// Accumulated statistics over the lifetime of a [`NetworkSimulator`].
#[derive(Debug, Clone, Default)]
pub struct NetworkSimStats {
    /// Number of packets submitted to `decide` / `apply`.
    pub total_processed: u64,
    /// Packets delivered without impairment (pure `Deliver` actions).
    pub delivered: u64,
    /// Packets dropped entirely.
    pub dropped: u64,
    /// Packets delivered with a non-zero delay.
    pub delayed: u64,
    /// Packets duplicated.
    pub duplicated: u64,
    /// Packets delivered with payload corruption.
    pub corrupted: u64,
    /// Total input bytes across all packets (including later-dropped ones).
    pub total_bytes: u64,
    /// Bytes belonging to dropped packets.
    pub dropped_bytes: u64,
}

impl NetworkSimStats {
    /// Fraction of packets that were dropped: `dropped / total_processed`.
    ///
    /// Returns `0.0` when `total_processed == 0`.
    pub fn loss_rate(&self) -> f64 {
        if self.total_processed == 0 {
            return 0.0;
        }
        self.dropped as f64 / self.total_processed as f64
    }

    /// Effective throughput in bytes/second based on delivered bytes and elapsed time.
    ///
    /// `elapsed_secs` must be positive; returns `0.0` otherwise.
    pub fn effective_throughput_bps(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs <= 0.0 {
            return 0.0;
        }
        let delivered_bytes = self.total_bytes.saturating_sub(self.dropped_bytes) as f64;
        delivered_bytes / elapsed_secs
    }
}

// ─── NetworkSimulator ────────────────────────────────────────────────────────

/// Self-contained network condition simulator.
///
/// Operates on raw `Vec<u8>` payloads. The simulator is single-owner and not
/// `Send`; wrap in `Arc<Mutex<_>>` for shared use across tasks.
pub struct NetworkSimulator {
    conditions: NetworkConditions,
    rng: Xorshift64,
    stats: NetworkSimStats,
    /// Bytes passed to `decide` within the current one-second bandwidth window.
    bytes_in_window: u64,
    /// Start of the current bandwidth throttling window.
    window_start: Instant,
}

impl NetworkSimulator {
    /// Create a simulator from explicit conditions and a non-zero seed.
    pub fn new(conditions: NetworkConditions, seed: u64) -> Self {
        Self {
            conditions,
            rng: Xorshift64::new(seed.max(1)),
            stats: NetworkSimStats::default(),
            bytes_in_window: 0,
            window_start: Instant::now(),
        }
    }

    /// Convenience constructor: perfect conditions, seed `1`.
    pub fn with_perfect() -> Self {
        Self::new(NetworkConditions::perfect(), 1)
    }

    // ── Core decision logic ──────────────────────────────────────────────────

    /// Decide what to do with `payload` — pure, synchronous, no I/O.
    ///
    /// Steps (in order):
    /// 1. Bandwidth throttle: if limit exceeded in current 1-second window → `Drop`.
    /// 2. Loss roll.
    /// 3. Compute `delay = latency + uniform_jitter`.
    /// 4. Duplicate roll.
    /// 5. Corruption roll.
    /// 6. Deliver.
    pub fn decide(&mut self, payload: &[u8]) -> SimulationAction {
        let payload_len = payload.len() as u64;

        // ── Step 1: bandwidth throttle ───────────────────────────────────────
        if self.conditions.bandwidth_bps > 0 {
            let elapsed = self.window_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                // Roll over to a new window.
                self.window_start = Instant::now();
                self.bytes_in_window = 0;
            }

            if self.bytes_in_window + payload_len > self.conditions.bandwidth_bps {
                // Would exceed the limit — throttle-drop.
                self.stats.total_processed += 1;
                self.stats.total_bytes += payload_len;
                self.stats.dropped += 1;
                self.stats.dropped_bytes += payload_len;
                return SimulationAction::Drop;
            }

            self.bytes_in_window += payload_len;
        }

        // ── Step 2: packet loss ──────────────────────────────────────────────
        if self.conditions.packet_loss_rate > 0.0
            && self.rng.next_f64() < self.conditions.packet_loss_rate
        {
            self.stats.total_processed += 1;
            self.stats.total_bytes += payload_len;
            self.stats.dropped += 1;
            self.stats.dropped_bytes += payload_len;
            return SimulationAction::Drop;
        }

        // ── Step 3: compute delay ────────────────────────────────────────────
        let jitter_ms = self.conditions.jitter.as_millis() as u64;
        let delay = self.conditions.latency + self.rng.next_duration_ms(jitter_ms);

        // ── Step 4: duplication roll ─────────────────────────────────────────
        if self.conditions.duplicate_rate > 0.0
            && self.rng.next_f64() < self.conditions.duplicate_rate
        {
            self.stats.total_processed += 1;
            self.stats.total_bytes += payload_len;
            self.stats.duplicated += 1;
            if delay > Duration::ZERO {
                self.stats.delayed += 1;
            }
            return SimulationAction::Duplicate {
                payload: payload.to_vec(),
                delay,
            };
        }

        // ── Step 5: corruption roll ──────────────────────────────────────────
        if self.conditions.corruption_rate > 0.0
            && self.rng.next_f64() < self.conditions.corruption_rate
        {
            let mut corrupted_payload = payload.to_vec();
            if let Some(first_byte) = corrupted_payload.first_mut() {
                *first_byte ^= 0xFF;
            }
            self.stats.total_processed += 1;
            self.stats.total_bytes += payload_len;
            self.stats.corrupted += 1;
            if delay > Duration::ZERO {
                self.stats.delayed += 1;
            }
            return SimulationAction::Corrupt {
                payload: corrupted_payload,
                delay,
            };
        }

        // ── Step 6: deliver ──────────────────────────────────────────────────
        self.stats.total_processed += 1;
        self.stats.total_bytes += payload_len;
        self.stats.delivered += 1;
        if delay > Duration::ZERO {
            self.stats.delayed += 1;
        }
        SimulationAction::Deliver {
            payload: payload.to_vec(),
            delay,
        }
    }

    /// Apply network conditions to `payload` asynchronously, sleeping for the
    /// computed delay before returning.
    ///
    /// Returns `None` if the packet was dropped; `Some(payload)` otherwise
    /// (payload may be modified by corruption).
    pub async fn apply(&mut self, payload: Vec<u8>) -> Option<Vec<u8>> {
        let action = self.decide(&payload);
        match action {
            SimulationAction::Deliver { payload, delay } => {
                if delay > Duration::ZERO {
                    tokio::time::sleep(delay).await;
                }
                Some(payload)
            }
            SimulationAction::Drop => None,
            SimulationAction::Duplicate { payload, delay } => {
                // For testing purposes we return one copy after half the delay.
                let half_delay = delay / 2;
                if half_delay > Duration::ZERO {
                    tokio::time::sleep(half_delay).await;
                }
                Some(payload)
            }
            SimulationAction::Corrupt { payload, delay } => {
                if delay > Duration::ZERO {
                    tokio::time::sleep(delay).await;
                }
                Some(payload)
            }
        }
    }

    // ── Accessors ────────────────────────────────────────────────────────────

    /// Borrow the active conditions.
    pub fn conditions(&self) -> &NetworkConditions {
        &self.conditions
    }

    /// Borrow the accumulated statistics.
    pub fn stats(&self) -> &NetworkSimStats {
        &self.stats
    }

    /// Reset all statistics counters to zero (conditions and RNG state unchanged).
    pub fn reset_stats(&mut self) {
        self.stats = NetworkSimStats::default();
    }

    /// Run `decide` on every payload in the slice and return the full action vector.
    ///
    /// Useful for unit tests that need to inspect a batch of outcomes without async.
    pub fn batch_decide(&mut self, payloads: &[Vec<u8>]) -> Vec<SimulationAction> {
        payloads.iter().map(|p| self.decide(p)).collect()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // ── Helper ───────────────────────────────────────────────────────────────

    /// Create a 64-byte payload with every byte set to `n`.
    fn payload(n: u8) -> Vec<u8> {
        vec![n; 64]
    }

    // ── PRNG tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_xorshift_nonzero() {
        let mut rng = Xorshift64::new(42);
        for _ in 0..1_000 {
            assert_ne!(rng.next(), 0, "xorshift64 must never output zero");
        }
    }

    #[test]
    fn test_xorshift_deterministic() {
        let mut a = Xorshift64::new(12345);
        let mut b = Xorshift64::new(12345);
        let seq_a: Vec<u64> = (0..50).map(|_| a.next()).collect();
        let seq_b: Vec<u64> = (0..50).map(|_| b.next()).collect();
        assert_eq!(seq_a, seq_b, "same seed must produce identical sequence");
    }

    #[test]
    fn test_xorshift_different_seeds() {
        let mut a = Xorshift64::new(1);
        let mut b = Xorshift64::new(2);
        let seq_a: Vec<u64> = (0..20).map(|_| a.next()).collect();
        let seq_b: Vec<u64> = (0..20).map(|_| b.next()).collect();
        assert_ne!(
            seq_a, seq_b,
            "different seeds must produce different sequences"
        );
    }

    #[test]
    fn test_xorshift_f64_in_range() {
        let mut rng = Xorshift64::new(99991);
        for _ in 0..1_000 {
            let v = rng.next_f64();
            assert!(
                (0.0..1.0).contains(&v),
                "next_f64 must return a value in [0.0, 1.0), got {v}"
            );
        }
    }

    // ── NetworkConditions preset tests ───────────────────────────────────────

    #[test]
    fn test_perfect_preset_values() {
        let c = NetworkConditions::perfect();
        assert_eq!(c.latency, Duration::ZERO);
        assert_eq!(c.jitter, Duration::ZERO);
        assert_eq!(c.packet_loss_rate, 0.0);
        assert_eq!(c.duplicate_rate, 0.0);
        assert_eq!(c.corruption_rate, 0.0);
        assert_eq!(c.bandwidth_bps, 0);
    }

    #[test]
    fn test_wan_preset_values() {
        let c = NetworkConditions::wan();
        assert_eq!(c.latency, Duration::from_millis(50));
        assert_eq!(c.packet_loss_rate, 0.01);
    }

    #[test]
    fn test_custom_constructor() {
        let c = NetworkConditions::custom(100, 50, 0.05);
        assert_eq!(c.latency, Duration::from_millis(100));
        assert_eq!(c.jitter, Duration::from_millis(50));
        assert!((c.packet_loss_rate - 0.05).abs() < 1e-9);
        assert_eq!(c.duplicate_rate, 0.0);
        assert_eq!(c.corruption_rate, 0.0);
    }

    // ── Decision logic tests ─────────────────────────────────────────────────

    #[test]
    fn test_perfect_conditions_always_deliver() {
        let mut sim = NetworkSimulator::new(NetworkConditions::perfect(), 1);
        let mut drop_count = 0;
        for i in 0..1_000_u32 {
            if let SimulationAction::Drop = sim.decide(&payload((i % 256) as u8)) {
                drop_count += 1;
            }
        }
        assert_eq!(drop_count, 0, "perfect conditions must never drop packets");
        assert_eq!(sim.stats().total_processed, 1_000);
    }

    #[test]
    fn test_lossy_conditions_drop_rate() {
        let mut conditions = NetworkConditions::perfect();
        conditions.packet_loss_rate = 0.20;
        let mut sim = NetworkSimulator::new(conditions, 7);
        let drops: usize = (0..1_000u16)
            .filter(|i| {
                matches!(
                    sim.decide(&payload((*i % 256) as u8)),
                    SimulationAction::Drop
                )
            })
            .count();
        // Expected 200; allow ±60 for statistical variance (3σ ≈ 48 at p=0.2, n=1000).
        assert!(
            (140..=260).contains(&drops),
            "expected ~200 drops, got {drops}"
        );
    }

    #[test]
    fn test_duplicate_rate() {
        let mut conditions = NetworkConditions::perfect();
        conditions.duplicate_rate = 0.10;
        let mut sim = NetworkSimulator::new(conditions, 13);
        let dups: usize = (0..1_000u16)
            .filter(|i| {
                matches!(
                    sim.decide(&payload((*i % 256) as u8)),
                    SimulationAction::Duplicate { .. }
                )
            })
            .count();
        assert!(
            (50..=150).contains(&dups),
            "expected ~100 duplicates, got {dups}"
        );
    }

    #[test]
    fn test_corrupt_rate() {
        let mut conditions = NetworkConditions::perfect();
        conditions.corruption_rate = 0.50;
        let mut sim = NetworkSimulator::new(conditions, 17);
        let corrupted: usize = (0..1_000u16)
            .filter(|i| {
                matches!(
                    sim.decide(&payload((*i % 256) as u8)),
                    SimulationAction::Corrupt { .. }
                )
            })
            .count();
        assert!(
            (400..=600).contains(&corrupted),
            "expected ~500 corrupted, got {corrupted}"
        );
    }

    #[test]
    fn test_no_loss_all_delivered() {
        let mut conditions = NetworkConditions::perfect();
        conditions.packet_loss_rate = 0.0;
        let mut sim = NetworkSimulator::new(conditions, 3);
        let non_deliver = (0..500u16)
            .filter(|i| {
                !matches!(
                    sim.decide(&payload((*i % 256) as u8)),
                    SimulationAction::Deliver { .. }
                )
            })
            .count();
        assert_eq!(
            non_deliver, 0,
            "zero loss rate must result in all Deliver actions"
        );
    }

    // ── Latency tests (async) ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_latency_is_applied() {
        let mut sim = NetworkSimulator::new(NetworkConditions::lan(), 5);
        let start = Instant::now();
        let result = sim.apply(payload(0)).await;
        let elapsed = start.elapsed();
        assert!(
            result.is_some(),
            "LAN conditions should not drop the packet"
        );
        assert!(
            elapsed >= Duration::from_millis(1),
            "LAN latency should introduce >= 1 ms delay, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_latency_minimum_bound() {
        // Use a small latency (5 ms) and verify apply() takes at least that long.
        // We use real time here (no tokio::time::pause) to keep the test self-contained.
        let mut conditions = NetworkConditions::perfect();
        conditions.latency = Duration::from_millis(5);
        conditions.jitter = Duration::ZERO;
        let mut sim = NetworkSimulator::new(conditions, 11);

        let start = Instant::now();
        let _result = sim.apply(payload(0)).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(5),
            "minimum 5 ms latency not honoured: {elapsed:?}"
        );
    }

    // ── Determinism tests ────────────────────────────────────────────────────

    #[test]
    fn test_determinism_same_seed() {
        let conditions = NetworkConditions::lossy();
        let mut sim_a = NetworkSimulator::new(conditions.clone(), 42);
        let mut sim_b = NetworkSimulator::new(conditions, 42);

        let payloads: Vec<Vec<u8>> = (0..100u8).map(payload).collect();
        let actions_a = sim_a.batch_decide(&payloads);
        let actions_b = sim_b.batch_decide(&payloads);

        assert_eq!(
            actions_a, actions_b,
            "identical seeds must produce identical action sequences"
        );
    }

    #[test]
    fn test_zero_jitter_deterministic_delay() {
        let mut conditions = NetworkConditions::perfect();
        conditions.latency = Duration::from_millis(25);
        conditions.jitter = Duration::ZERO;
        let mut sim = NetworkSimulator::new(conditions, 7);

        for i in 0..100u8 {
            match sim.decide(&payload(i)) {
                SimulationAction::Deliver { delay, .. } => {
                    assert_eq!(
                        delay,
                        Duration::from_millis(25),
                        "delay should equal latency exactly when jitter == 0"
                    );
                }
                other => panic!("expected Deliver, got {other:?}"),
            }
        }
    }

    // ── Stats tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_stats_tracking() {
        let mut sim = NetworkSimulator::new(NetworkConditions::perfect(), 1);
        for i in 0..100u8 {
            sim.decide(&payload(i));
        }
        assert_eq!(
            sim.stats().total_processed,
            100,
            "stats.total_processed must equal the number of decide() calls"
        );
    }

    #[test]
    fn test_stats_drop_count() {
        let mut conditions = NetworkConditions::perfect();
        conditions.packet_loss_rate = 0.30;
        let mut sim = NetworkSimulator::new(conditions, 31);
        let mut manual_drops = 0u64;
        for i in 0..200u8 {
            if matches!(sim.decide(&payload(i)), SimulationAction::Drop) {
                manual_drops += 1;
            }
        }
        assert_eq!(
            sim.stats().dropped,
            manual_drops,
            "stats.dropped must match manually counted drops"
        );
    }

    #[test]
    fn test_stats_loss_rate() {
        let mut conditions = NetworkConditions::perfect();
        conditions.packet_loss_rate = 0.50;
        let mut sim = NetworkSimulator::new(conditions, 37);
        for i in 0..100u8 {
            sim.decide(&payload(i));
        }
        let lr = sim.stats().loss_rate();
        assert!(
            (0.0..=1.0).contains(&lr),
            "loss_rate() must be in [0, 1], got {lr}"
        );
        // Expected ~0.5 ± 0.15 (wide tolerance for small n).
        assert!(
            (0.25..=0.75).contains(&lr),
            "loss_rate() expected around 0.5, got {lr}"
        );
    }

    #[test]
    fn test_reset_stats() {
        let mut sim = NetworkSimulator::new(NetworkConditions::lossy(), 41);
        for i in 0..10u8 {
            sim.decide(&payload(i));
        }
        sim.reset_stats();
        let s = sim.stats();
        assert_eq!(s.total_processed, 0);
        assert_eq!(s.delivered, 0);
        assert_eq!(s.dropped, 0);
        assert_eq!(s.delayed, 0);
        assert_eq!(s.duplicated, 0);
        assert_eq!(s.corrupted, 0);
        assert_eq!(s.total_bytes, 0);
        assert_eq!(s.dropped_bytes, 0);
    }

    #[test]
    fn test_bytes_accounting() {
        let mut sim = NetworkSimulator::new(NetworkConditions::perfect(), 1);
        let p = payload(7); // 64 bytes each
        for _ in 0..10 {
            sim.decide(&p);
        }
        assert_eq!(
            sim.stats().total_bytes,
            640,
            "total_bytes must equal sum of all submitted payload lengths"
        );
    }

    // ── Bandwidth throttle test ──────────────────────────────────────────────

    #[test]
    fn test_bandwidth_throttle() {
        // Cap at 100 bytes/sec; submit 20 packets of 10 bytes each (200 bytes total).
        // The first 10 should pass (100 bytes), the rest should be throttled.
        let mut conditions = NetworkConditions::perfect();
        conditions.bandwidth_bps = 100;
        let mut sim = NetworkSimulator::new(conditions, 53);

        let small = vec![0u8; 10]; // 10 bytes
        let mut dropped = 0;
        for _ in 0..20 {
            if matches!(sim.decide(&small), SimulationAction::Drop) {
                dropped += 1;
            }
        }
        assert!(
            dropped >= 9,
            "at least 9 of 20 packets should be throttled (only 100 bytes/s cap), got {dropped} drops"
        );
    }

    // ── Batch decide test ────────────────────────────────────────────────────

    #[test]
    fn test_batch_decide() {
        let mut conditions = NetworkConditions::perfect();
        conditions.packet_loss_rate = 0.10;
        let mut sim = NetworkSimulator::new(conditions, 61);
        let payloads: Vec<Vec<u8>> = (0..10u8).map(payload).collect();
        let actions = sim.batch_decide(&payloads);
        assert_eq!(
            actions.len(),
            10,
            "batch_decide must return one action per payload"
        );
        assert_eq!(
            sim.stats().total_processed,
            10,
            "stats must reflect batch_decide calls"
        );
    }

    // ── Corruption content test ──────────────────────────────────────────────

    #[test]
    fn test_corrupt_modifies_first_byte() {
        // Use 100% corruption to guarantee we get a Corrupt action.
        let mut conditions = NetworkConditions::perfect();
        conditions.corruption_rate = 1.0;
        let mut sim = NetworkSimulator::new(conditions, 71);
        let original = payload(0xAB); // all bytes = 0xAB
        match sim.decide(&original) {
            SimulationAction::Corrupt { payload, .. } => {
                assert_ne!(
                    payload[0], original[0],
                    "corrupted first byte must differ from original"
                );
                assert_eq!(
                    payload[0],
                    original[0] ^ 0xFF,
                    "corrupted first byte must be original XOR 0xFF"
                );
                // Remaining bytes unchanged.
                assert!(
                    payload[1..].iter().all(|&b| b == 0xAB),
                    "only the first byte should be modified"
                );
            }
            other => panic!("expected Corrupt with 100% corruption_rate, got {other:?}"),
        }
    }

    // ── Async drop test ──────────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_apply_async_drop_returns_none() {
        let mut conditions = NetworkConditions::perfect();
        conditions.packet_loss_rate = 1.0; // 100% loss
        let mut sim = NetworkSimulator::new(conditions, 83);
        let result = sim.apply(payload(42)).await;
        assert!(
            result.is_none(),
            "100% packet loss must make apply() return None"
        );
        assert_eq!(sim.stats().dropped, 1);
        assert_eq!(sim.stats().total_processed, 1);
    }
}
