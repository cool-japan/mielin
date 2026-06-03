//! Adaptive Backoff Controller
//!
//! Production-grade retry delay controller with five distinct strategies:
//! Fixed, Linear, Exponential, Exponential-with-Jitter, and Fibonacci.
//!
//! The controller maintains fine-grained statistics (total wait, attempt
//! history) and exposes a deterministic `next_delay` that accepts a caller-
//! supplied `rng` seed so jittered strategies remain reproducibly testable.

use std::time::{Duration, Instant};

// ─── BackoffStrategy ─────────────────────────────────────────────────────────

/// Available retry-delay calculation strategies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackoffStrategy {
    /// Every attempt uses exactly `delay`, regardless of how many retries
    /// have elapsed.
    Fixed { delay: Duration },

    /// Delay grows linearly: `min(initial + step * attempt, max)`.
    Linear {
        initial: Duration,
        step: Duration,
        max: Duration,
    },

    /// Delay doubles each attempt: `min(initial * multiplier^attempt, max)`.
    Exponential {
        initial: Duration,
        multiplier: f64,
        max: Duration,
    },

    /// Exponential with additive uniform jitter, sampled from
    /// `[0, base * jitter_factor]` using a caller-supplied 64-bit seed.
    /// Prevents thundering-herd reconnection storms.
    ExponentialJitter {
        initial: Duration,
        multiplier: f64,
        max: Duration,
        /// Fraction of the base delay that can be added as jitter (0–1).
        jitter_factor: f64,
    },

    /// Delay follows the Fibonacci sequence scaled by `unit`:
    /// F(0)=0, F(1)=1, F(2)=1, F(3)=2, … → `min(F(attempt)*unit, max)`.
    Fibonacci { unit: Duration, max: Duration },
}

impl BackoffStrategy {
    /// Compute the delay for the given `attempt` number (0-indexed).
    ///
    /// `rng` is a 64-bit seed used **only** by `ExponentialJitter`; all other
    /// strategies ignore it.  Callers wanting reproducible tests can pass a
    /// fixed value; callers wanting randomness should pass e.g. the lower
    /// 64 bits of the current time.
    pub fn next_delay(&self, attempt: u32, rng: u64) -> Duration {
        match *self {
            BackoffStrategy::Fixed { delay } => delay,

            BackoffStrategy::Linear { initial, step, max } => {
                let total_nanos = initial.as_nanos() + step.as_nanos() * attempt as u128;
                let max_nanos = max.as_nanos();
                Duration::from_nanos(total_nanos.min(max_nanos) as u64)
            }

            BackoffStrategy::Exponential {
                initial,
                multiplier,
                max,
            } => {
                let scale = multiplier.powi(attempt as i32);
                let nanos = (initial.as_nanos() as f64 * scale).min(max.as_nanos() as f64);
                Duration::from_nanos(nanos as u64)
            }

            BackoffStrategy::ExponentialJitter {
                initial,
                multiplier,
                max,
                jitter_factor,
            } => {
                let scale = multiplier.powi(attempt as i32);
                let base_nanos = (initial.as_nanos() as f64 * scale).min(max.as_nanos() as f64);

                // Derive a deterministic fraction in [0, 1) from the rng seed
                // using splitmix64 (strong avalanche — avoids seed proximity issues).
                let rng_frac = splitmix64_unit(rng);
                let jitter_nanos = base_nanos * jitter_factor * rng_frac;
                let total = (base_nanos + jitter_nanos).min(max.as_nanos() as f64);
                Duration::from_nanos(total as u64)
            }

            BackoffStrategy::Fibonacci { unit, max } => {
                let fib = fibonacci_u128(attempt);
                let nanos = (unit.as_nanos() * fib).min(max.as_nanos());
                Duration::from_nanos(nanos as u64)
            }
        }
    }

    /// The configured upper bound on delay.  For `Fixed`, this equals the
    /// fixed delay itself.
    pub fn max_delay(&self) -> Duration {
        match *self {
            BackoffStrategy::Fixed { delay } => delay,
            BackoffStrategy::Linear { max, .. } => max,
            BackoffStrategy::Exponential { max, .. } => max,
            BackoffStrategy::ExponentialJitter { max, .. } => max,
            BackoffStrategy::Fibonacci { max, .. } => max,
        }
    }

    /// Returns `true` if this strategy introduces non-deterministic jitter.
    pub fn is_jittered(&self) -> bool {
        matches!(self, BackoffStrategy::ExponentialJitter { .. })
    }
}

// ─── Internal maths helpers ──────────────────────────────────────────────────

/// Splitmix64 hash step, mapped to the unit interval `[0, 1)`.
///
/// Splitmix64 has excellent avalanche properties: every input bit affects
/// every output bit, so even two seeds that differ only in their LSB produce
/// maximally different output fractions.  This is far superior to xorshift for
/// use as a jitter source because nearby wall-clock timestamps (common rng
/// inputs) produce well-separated jitter values.
fn splitmix64_unit(seed: u64) -> f64 {
    let mut z = seed.wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z = z ^ (z >> 31);
    // Map to [0, 1) using the high 53 bits (full IEEE 754 double mantissa).
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Fibonacci number F(n) in u128 arithmetic.  F(0) = 0, F(1) = 1.
fn fibonacci_u128(n: u32) -> u128 {
    if n == 0 {
        return 0;
    }
    let (mut a, mut b) = (0u128, 1u128);
    for _ in 1..n {
        let next = a.saturating_add(b);
        a = b;
        b = next;
    }
    b
}

// ─── BackoffStats ────────────────────────────────────────────────────────────

/// Snapshot of an `AdaptiveBackoff` instance at a point in time.
#[derive(Debug, Clone)]
pub struct BackoffStats {
    /// Number of attempts made so far (failures recorded).
    pub attempts: u32,
    /// Maximum number of attempts before exhaustion.
    pub max_attempts: u32,
    /// Total time spent waiting across all `next_delay` calls, in ms.
    pub total_wait_ms: u64,
    /// Whether the last recorded event was a success.
    pub succeeded: bool,
    /// Delay returned by the most recent `next_delay` call, in ms.
    pub last_delay_ms: Option<u64>,
}

// ─── AdaptiveBackoff ─────────────────────────────────────────────────────────

/// Stateful retry-delay controller.
///
/// Call `next_delay()` before each retry attempt.  Record outcomes with
/// `record_success()` / `record_failure()`.  Inspect `is_exhausted()` to
/// decide whether to give up entirely.
pub struct AdaptiveBackoff {
    strategy: BackoffStrategy,
    current_attempt: u32,
    max_attempts: u32,
    last_attempt_at: Option<Instant>,
    total_wait: Duration,
    succeeded_after: Option<u32>,
    last_delay: Option<Duration>,
}

impl AdaptiveBackoff {
    /// Construct with a custom strategy and attempt ceiling.
    pub fn new(strategy: BackoffStrategy, max_attempts: u32) -> Self {
        Self {
            strategy,
            current_attempt: 0,
            max_attempts,
            last_attempt_at: None,
            total_wait: Duration::ZERO,
            succeeded_after: None,
            last_delay: None,
        }
    }

    /// Factory: exponential backoff with 2× multiplier.
    ///
    /// `initial_ms` — base delay in milliseconds.
    /// `max_ms`     — cap on delay in milliseconds.
    /// `max_attempts` — give up after this many failures.
    pub fn exponential(initial_ms: u64, max_ms: u64, max_attempts: u32) -> Self {
        Self::new(
            BackoffStrategy::Exponential {
                initial: Duration::from_millis(initial_ms),
                multiplier: 2.0,
                max: Duration::from_millis(max_ms),
            },
            max_attempts,
        )
    }

    /// Factory: linear backoff.
    ///
    /// `initial_ms` — delay for attempt 0.
    /// `step_ms`    — increment per attempt.
    /// `max_ms`     — cap on delay.
    /// `max_attempts` — give up after this many failures.
    pub fn linear(initial_ms: u64, step_ms: u64, max_ms: u64, max_attempts: u32) -> Self {
        Self::new(
            BackoffStrategy::Linear {
                initial: Duration::from_millis(initial_ms),
                step: Duration::from_millis(step_ms),
                max: Duration::from_millis(max_ms),
            },
            max_attempts,
        )
    }

    /// Return the delay to wait before the **current** attempt, then advance
    /// the internal counter.
    ///
    /// Returns `None` if `is_exhausted()` is true (caller should give up).
    pub fn next_delay(&mut self) -> Option<Duration> {
        if self.is_exhausted() {
            return None;
        }

        // Derive a cheap pseudo-random seed from elapsed time so jittered
        // strategies produce different values each call without requiring rand.
        let rng_seed: u64 = self
            .last_attempt_at
            .map(|t| {
                let micros = t.elapsed().as_micros() as u64;
                micros
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407)
            })
            .unwrap_or(0xDEAD_BEEF_CAFE_BABE);

        let delay = self.strategy.next_delay(self.current_attempt, rng_seed);
        self.last_attempt_at = Some(Instant::now());
        self.total_wait += delay;
        self.last_delay = Some(delay);
        Some(delay)
    }

    /// Record that the last operation succeeded.
    ///
    /// Stores the attempt count at which success occurred and resets the
    /// failure counter so the controller can be reused.
    pub fn record_success(&mut self) {
        self.succeeded_after = Some(self.current_attempt);
        self.current_attempt = 0;
    }

    /// Record that the last operation failed, incrementing the attempt counter.
    pub fn record_failure(&mut self) {
        self.current_attempt = self.current_attempt.saturating_add(1);
    }

    /// Returns `true` if the maximum number of attempts has been exceeded.
    pub fn is_exhausted(&self) -> bool {
        self.current_attempt >= self.max_attempts
    }

    /// Reset all state to initial conditions (as if just constructed).
    pub fn reset(&mut self) {
        self.current_attempt = 0;
        self.last_attempt_at = None;
        self.total_wait = Duration::ZERO;
        self.succeeded_after = None;
        self.last_delay = None;
    }

    /// Current 0-indexed attempt number (equals failure count since last reset).
    pub fn current_attempt(&self) -> u32 {
        self.current_attempt
    }

    /// Cumulative time spent waiting across all `next_delay` calls.
    pub fn total_wait(&self) -> Duration {
        self.total_wait
    }

    /// Snapshot of current statistics.
    pub fn stats(&self) -> BackoffStats {
        BackoffStats {
            attempts: self.current_attempt,
            max_attempts: self.max_attempts,
            total_wait_ms: self.total_wait.as_millis() as u64,
            succeeded: self.succeeded_after.is_some(),
            last_delay_ms: self.last_delay.map(|d| d.as_millis() as u64),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Strategy delay ordering ──────────────────────────────────────────────

    #[test]
    fn test_backoff_exponential_delays_grow() {
        let strategy = BackoffStrategy::Exponential {
            initial: Duration::from_millis(100),
            multiplier: 2.0,
            max: Duration::from_secs(60),
        };
        let d0 = strategy.next_delay(0, 0);
        let d1 = strategy.next_delay(1, 0);
        let d2 = strategy.next_delay(2, 0);
        assert!(d0 < d1, "attempt 0 ({d0:?}) should be < attempt 1 ({d1:?})");
        assert!(d1 < d2, "attempt 1 ({d1:?}) should be < attempt 2 ({d2:?})");
    }

    #[test]
    fn test_backoff_exponential_caps_at_max() {
        let max = Duration::from_millis(500);
        let strategy = BackoffStrategy::Exponential {
            initial: Duration::from_millis(100),
            multiplier: 2.0,
            max,
        };
        for attempt in 0..20 {
            let d = strategy.next_delay(attempt, 0);
            assert!(d <= max, "attempt {attempt}: {d:?} exceeds max {max:?}");
        }
    }

    #[test]
    fn test_backoff_linear_increases_by_step() {
        let initial = Duration::from_millis(100);
        let step = Duration::from_millis(50);
        let max = Duration::from_secs(10);
        let strategy = BackoffStrategy::Linear { initial, step, max };
        let d0 = strategy.next_delay(0, 0);
        let d1 = strategy.next_delay(1, 0);
        assert_eq!(d0, initial, "attempt 0 should equal initial");
        assert_eq!(d1, initial + step, "attempt 1 should equal initial + step");
    }

    #[test]
    fn test_backoff_fixed_always_same() {
        let fixed_delay = Duration::from_millis(250);
        let strategy = BackoffStrategy::Fixed { delay: fixed_delay };
        for attempt in 0..10 {
            let d = strategy.next_delay(attempt, attempt as u64);
            assert_eq!(d, fixed_delay, "attempt {attempt}: expected fixed delay");
        }
    }

    #[test]
    fn test_backoff_fibonacci_sequence() {
        let unit = Duration::from_millis(100);
        let max = Duration::from_secs(60);
        let strategy = BackoffStrategy::Fibonacci { unit, max };

        // F sequence: 0, 1, 1, 2, 3, 5, 8, 13, 21 ...
        // Attempt 1 → F(1)*100ms = 100ms
        // Attempt 2 → F(2)*100ms = 100ms
        // Attempt 3 → F(3)*100ms = 200ms
        // Attempt 4 → F(4)*100ms = 300ms
        let d1 = strategy.next_delay(1, 0);
        let d2 = strategy.next_delay(2, 0);
        let d3 = strategy.next_delay(3, 0);
        let d4 = strategy.next_delay(4, 0);

        assert_eq!(d1, Duration::from_millis(100)); // F(1)=1
        assert_eq!(d2, Duration::from_millis(100)); // F(2)=1
        assert_eq!(d3, Duration::from_millis(200)); // F(3)=2
        assert_eq!(d4, Duration::from_millis(300)); // F(4)=3
                                                    // Verify monotonic growth.
        assert!(d3 > d2, "Fibonacci should grow: d3({d3:?}) > d2({d2:?})");
        assert!(d4 > d3, "Fibonacci should grow: d4({d4:?}) > d3({d3:?})");
    }

    // ── Exhaustion ───────────────────────────────────────────────────────────

    #[test]
    fn test_backoff_exhausted_after_max() {
        let mut backoff = AdaptiveBackoff::exponential(100, 1000, 3);
        // 3 failures → attempt counter = 3 = max_attempts → exhausted.
        backoff.record_failure();
        backoff.record_failure();
        backoff.record_failure();
        assert!(backoff.is_exhausted());
        assert_eq!(backoff.next_delay(), None);
    }

    // ── Reset / success ──────────────────────────────────────────────────────

    #[test]
    fn test_backoff_reset_clears_state() {
        let mut backoff = AdaptiveBackoff::exponential(100, 1000, 5);
        backoff.record_failure();
        backoff.record_failure();
        let d = backoff
            .next_delay()
            .expect("should have delay before reset");
        assert!(d > Duration::ZERO);

        backoff.reset();
        assert_eq!(backoff.current_attempt(), 0);
        assert_eq!(backoff.total_wait(), Duration::ZERO);
        assert!(!backoff.is_exhausted());
    }

    #[test]
    fn test_backoff_record_success_resets() {
        let mut backoff = AdaptiveBackoff::exponential(100, 1000, 5);
        // Push attempt counter up.
        backoff.record_failure();
        backoff.record_failure();
        backoff.record_failure();
        assert_eq!(backoff.current_attempt(), 3);

        backoff.record_success();
        assert_eq!(
            backoff.current_attempt(),
            0,
            "success should reset attempt counter"
        );
        // Verify we get a fresh initial delay again.
        let d = backoff
            .next_delay()
            .expect("should not be exhausted after success");
        assert_eq!(
            d,
            Duration::from_millis(100),
            "delay should reset to initial after success"
        );
    }

    // ── Jitter variation ─────────────────────────────────────────────────────

    #[test]
    fn test_backoff_jitter_varies() {
        let strategy = BackoffStrategy::ExponentialJitter {
            initial: Duration::from_millis(100),
            multiplier: 2.0,
            max: Duration::from_secs(30),
            jitter_factor: 0.5,
        };
        // Different rng seeds should produce different delays.
        let d_a = strategy.next_delay(3, 0x0123_4567_89AB_CDEF);
        let d_b = strategy.next_delay(3, 0xFEDC_BA98_7654_3210);
        assert_ne!(
            d_a, d_b,
            "Different rng seeds should yield different jittered delays"
        );
    }

    // ── Statistics accumulation ──────────────────────────────────────────────

    #[test]
    fn test_backoff_stats_track_wait() {
        let mut backoff = AdaptiveBackoff::exponential(50, 5000, 10);
        // Call next_delay twice; total_wait should accumulate.
        let d0 = backoff.next_delay().expect("delay 0");
        let d1 = backoff.next_delay().expect("delay 1");
        let expected_ms = (d0 + d1).as_millis() as u64;
        assert_eq!(
            backoff.stats().total_wait_ms,
            expected_ms,
            "total_wait_ms should equal sum of returned delays"
        );
    }

    // ── Factory methods ──────────────────────────────────────────────────────

    #[test]
    fn test_backoff_factory_exponential() {
        let backoff = AdaptiveBackoff::exponential(100, 5000, 7);
        // Verify by inspecting the first delay (attempt 0 → initial delay).
        let delay = backoff.strategy.next_delay(0, 0);
        assert_eq!(delay, Duration::from_millis(100));
        assert_eq!(backoff.max_attempts, 7);
        let cap = backoff.strategy.max_delay();
        assert_eq!(cap, Duration::from_millis(5000));
    }

    #[test]
    fn test_backoff_factory_linear() {
        let backoff = AdaptiveBackoff::linear(200, 100, 2000, 4);
        let delay0 = backoff.strategy.next_delay(0, 0);
        let delay1 = backoff.strategy.next_delay(1, 0);
        assert_eq!(delay0, Duration::from_millis(200));
        assert_eq!(delay1, Duration::from_millis(300));
        assert_eq!(backoff.max_attempts, 4);
    }
}
