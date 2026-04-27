//! Fault Injection Framework
//!
//! Composable, deterministic fault injection for reliability testing.
//! Uses a lock-free LCG for probabilistic decision-making without rand dependency.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Probability in [0.0, 1.0]; values outside this range are clamped on construction.
#[derive(Debug, Clone, Copy)]
pub struct Probability(f64);

impl Probability {
    pub fn new(p: f64) -> Self {
        Self(p.clamp(0.0, 1.0))
    }

    pub fn always() -> Self {
        Self(1.0)
    }

    pub fn never() -> Self {
        Self(0.0)
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

/// The specific kind of fault to inject
#[derive(Debug, Clone)]
pub enum FaultKind {
    /// Abort the operation, return an error to the caller
    Drop,
    /// Insert a wall-clock delay before the operation proceeds
    Delay { micros: u64 },
    /// Flip a single bit in the payload at the given byte offset
    /// (or a deterministically chosen offset when `None`)
    Corrupt { byte_offset: Option<usize> },
    /// Process the operation a second time (idempotency stress)
    Duplicate,
    /// Simulate a hung operation by sleeping for the given duration
    Timeout { after_micros: u64 },
}

/// A single fault specification: what to inject, how often, and how many times
#[derive(Debug, Clone)]
pub struct FaultSpec {
    pub kind: FaultKind,
    pub probability: Probability,
    /// `None` means unlimited injections
    pub max_occurrences: Option<usize>,
}

impl FaultSpec {
    pub fn new(kind: FaultKind, probability: Probability) -> Self {
        Self {
            kind,
            probability,
            max_occurrences: None,
        }
    }

    pub fn with_max_occurrences(mut self, n: usize) -> Self {
        self.max_occurrences = Some(n);
        self
    }
}

/// Statistics snapshot for one complete injector
#[derive(Debug, Clone)]
pub struct FaultInjectorStats {
    pub total_injected: usize,
    pub per_label: HashMap<String, usize>,
}

/// Composable fault injector backed by a Knuth-multiplicative LCG.
///
/// The LCG provides deterministic (when seeded) pseudo-randomness with no
/// external dependency.  The initial seed is intentionally non-zero so the
/// sequence is never stuck at 0.
pub struct FaultInjector {
    faults: Vec<(String, FaultSpec)>,
    rng_state: AtomicU64,
    occurrence_counts: Mutex<HashMap<String, usize>>,
    total_injected: AtomicUsize,
}

impl std::fmt::Debug for FaultInjector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaultInjector")
            .field("fault_count", &self.faults.len())
            .field(
                "total_injected",
                &self.total_injected.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl FaultInjector {
    /// Knuth multiplicative constant for the 64-bit LCG
    const LCG_A: u64 = 6_364_136_223_846_793_005;
    const LCG_C: u64 = 1_442_695_040_888_963_407;

    pub fn new() -> Self {
        Self {
            faults: Vec::new(),
            rng_state: AtomicU64::new(0x12345678ABCDEF01),
            occurrence_counts: Mutex::new(HashMap::new()),
            total_injected: AtomicUsize::new(0),
        }
    }

    /// Advance the LCG state and return a value in `[0, u64::MAX]`.
    fn next_raw(&self) -> u64 {
        loop {
            let old = self.rng_state.load(Ordering::Relaxed);
            let new = old.wrapping_mul(Self::LCG_A).wrapping_add(Self::LCG_C);
            if self
                .rng_state
                .compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return new;
            }
        }
    }

    /// Return a pseudo-random `f64` in `[0.0, 1.0)` using the high 32 bits.
    fn next_f64(&self) -> f64 {
        let raw = self.next_raw();
        (raw >> 32) as f64 / (u32::MAX as f64 + 1.0)
    }

    /// Register a fault spec under `label`.  Multiple specs under the same
    /// label are evaluated in insertion order; the first one that fires wins.
    pub fn add_fault(&mut self, label: &str, spec: FaultSpec) -> &mut Self {
        self.faults.push((label.to_string(), spec));
        self
    }

    /// Return `true` if a fault for `label` would fire right now.
    pub fn should_inject(&self, label: &str) -> bool {
        self.inject_if(label).is_some()
    }

    /// If a fault should fire for `label`, consume one occurrence and return
    /// `Some(FaultKind)`.  Returns `None` when no fault is eligible.
    pub fn inject_if(&self, label: &str) -> Option<FaultKind> {
        let mut counts = self
            .occurrence_counts
            .lock()
            .expect("occurrence_counts poisoned");

        for (l, spec) in &self.faults {
            if l != label {
                continue;
            }

            let used = counts.get(l).copied().unwrap_or(0);

            if let Some(max) = spec.max_occurrences {
                if used >= max {
                    continue;
                }
            }

            if self.next_f64() < spec.probability.value() {
                *counts.entry(l.clone()).or_insert(0) += 1;
                self.total_injected.fetch_add(1, Ordering::Relaxed);
                return Some(spec.kind.clone());
            }
        }
        None
    }

    /// Return a stats snapshot; does not reset counters.
    pub fn stats(&self) -> FaultInjectorStats {
        let counts = self
            .occurrence_counts
            .lock()
            .expect("occurrence_counts poisoned");
        FaultInjectorStats {
            total_injected: self.total_injected.load(Ordering::Relaxed),
            per_label: counts.clone(),
        }
    }

    /// Clear occurrence counters and total; does not remove fault specs.
    pub fn reset(&self) {
        let mut counts = self
            .occurrence_counts
            .lock()
            .expect("occurrence_counts poisoned");
        counts.clear();
        self.total_injected.store(0, Ordering::Relaxed);
    }

    // ── Builder helpers ──────────────────────────────────────────────────────

    /// Create an injector with a single always-drop fault for `label`.
    pub fn always_drop(label: &str) -> Self {
        let mut fi = Self::new();
        fi.add_fault(
            label,
            FaultSpec::new(FaultKind::Drop, Probability::always()),
        );
        fi
    }

    /// Create an injector with a single always-delay fault for `label`.
    pub fn always_delay(label: &str, micros: u64) -> Self {
        let mut fi = Self::new();
        fi.add_fault(
            label,
            FaultSpec::new(FaultKind::Delay { micros }, Probability::always()),
        );
        fi
    }

    /// Create an injector with a probabilistic fault of the given kind.
    pub fn occasionally(label: &str, probability: Probability, kind: FaultKind) -> Self {
        let mut fi = Self::new();
        fi.add_fault(label, FaultSpec::new(kind, probability));
        fi
    }
}

impl Default for FaultInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply a non-async delay (busy-wait) for the given microseconds.
/// Used in synchronous contexts; for async contexts wrap with `tokio::time::sleep`.
pub fn apply_delay_sync(micros: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_micros(micros);
    while std::time::Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

/// Perform a single-bit corruption at `byte_offset` (modulo payload length).
/// If the payload is empty the function is a no-op.
pub fn corrupt_payload(payload: &mut [u8], byte_offset: Option<usize>) {
    if payload.is_empty() {
        return;
    }
    let idx = byte_offset.unwrap_or(0) % payload.len();
    payload[idx] ^= 0x01;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probability_clamp() {
        assert_eq!(Probability::new(-1.0).value(), 0.0);
        assert_eq!(Probability::new(2.0).value(), 1.0);
        assert_eq!(Probability::new(0.5).value(), 0.5);
    }

    #[test]
    fn test_always_drop_fires_every_time() {
        let fi = FaultInjector::always_drop("op");
        for _ in 0..20 {
            assert!(matches!(fi.inject_if("op"), Some(FaultKind::Drop)));
        }
    }

    #[test]
    fn test_never_fires_on_zero_probability() {
        let fi = FaultInjector::occasionally("op", Probability::never(), FaultKind::Drop);
        for _ in 0..100 {
            assert!(fi.inject_if("op").is_none());
        }
    }

    #[test]
    fn test_stats_accumulate() {
        let fi = FaultInjector::always_drop("op");
        fi.inject_if("op");
        fi.inject_if("op");
        let s = fi.stats();
        assert_eq!(s.total_injected, 2);
        assert_eq!(s.per_label["op"], 2);
    }

    #[test]
    fn test_reset_clears_counters() {
        let fi = FaultInjector::always_drop("op");
        fi.inject_if("op");
        fi.reset();
        let s = fi.stats();
        assert_eq!(s.total_injected, 0);
        assert!(s.per_label.is_empty());
    }

    #[test]
    fn test_max_occurrences_respected() {
        let mut fi = FaultInjector::new();
        fi.add_fault(
            "op",
            FaultSpec::new(FaultKind::Drop, Probability::always()).with_max_occurrences(3),
        );
        let injected: usize = (0..10).map(|_| fi.inject_if("op").is_some() as usize).sum();
        assert_eq!(injected, 3);
    }

    #[test]
    fn test_corrupt_payload_flips_byte() {
        let mut payload = vec![0x00u8, 0xFF, 0xAA];
        corrupt_payload(&mut payload, Some(0));
        assert_eq!(payload[0], 0x01);
    }
}
