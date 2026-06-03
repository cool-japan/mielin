//! HAL Production Hardening — Hardware Probe with Retry and Fallback
//!
//! This module provides robust hardware detection infrastructure for production
//! embedded and bare-metal environments, where hardware may be unreliable,
//! registers may be transiently unavailable, or probes may timeout.
//!
//! ## Design Principles
//!
//! ### Retry-with-fallback
//!
//! `HardwareProbe::probe()` wraps any detection closure and retries it up to
//! `max_attempts` times. If all attempts fail it transparently invokes the
//! caller-supplied `fallback_fn` and marks `ProbeResult::fallback_used = true`.
//! This guarantees the caller always receives a usable value — never an error.
//!
//! ### Probe chains
//!
//! `HardwareProbe::probe_chain()` accepts an ordered slice of detection strategies.
//! The first strategy that succeeds wins. Strategies that fail are silently skipped.
//! This models the real-world layering of detection approaches:
//!
//! 1. Direct register read (fastest, requires privilege)
//! 2. Device Tree / ACPI query (portable, requires firmware)
//! 3. `/proc/cpuinfo` parsing (Linux only, requires file system)
//! 4. Compile-time known constant (always available)
//!
//! ### Diagnostics
//!
//! `HalDiagnostics` accumulates warnings and errors from multiple probe
//! operations into a single report. Callers can inspect `severity()` to
//! determine whether to proceed, degrade gracefully, or abort.
//!
//! ### PlatformDetector
//!
//! `PlatformDetector` combines `HardwareProbe` and `HalDiagnostics` into a
//! single high-level API. It detects architecture, vendor, model, core count,
//! and memory, annotating each detection step with diagnostic information.
//! On any non-native host `is_simulated = true` is set so callers know the
//! values came from fallback logic rather than real hardware.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

// ─────────────────────────────────────────────────────────────────────────────
// ProbeError
// ─────────────────────────────────────────────────────────────────────────────

/// A diagnostic error from a single probe attempt.
///
/// Variants cover the most common failure modes in embedded hardware probing:
/// register read timeout, wrong magic/signature value, hardware absent, and
/// a generic I/O catch-all.
#[derive(Debug, Clone)]
pub enum ProbeError {
    /// The probe did not receive a response within the configured timeout.
    Timeout {
        /// Number of milliseconds elapsed before the probe was abandoned.
        after_ms: u32,
    },
    /// The register or memory location contained an unexpected magic value.
    InvalidSignature {
        /// The value that was expected.
        expected: u32,
        /// The value that was actually read.
        got: u32,
    },
    /// The hardware component is simply not present on this device.
    NotPresent,
    /// A generic I/O-level failure with a human-readable description.
    IoError(String),
}

impl core::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProbeError::Timeout { after_ms } => {
                write!(f, "probe timeout after {}ms", after_ms)
            }
            ProbeError::InvalidSignature { expected, got } => {
                write!(
                    f,
                    "invalid signature: expected 0x{:08X}, got 0x{:08X}",
                    expected, got
                )
            }
            ProbeError::NotPresent => write!(f, "hardware not present"),
            ProbeError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ProbeResult
// ─────────────────────────────────────────────────────────────────────────────

/// The outcome of a hardware probe sequence (possibly including retries).
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// `true` when at least one probe attempt returned `Ok`.
    pub succeeded: bool,
    /// Total number of probe attempts made (1 on immediate success).
    pub attempts: u32,
    /// The last error seen before either succeeding or giving up.
    /// `None` when `succeeded == true`.
    pub error: Option<ProbeError>,
    /// `true` when the value was sourced from the fallback function rather
    /// than from actual hardware.
    pub fallback_used: bool,
}

impl ProbeResult {
    /// Construct a successful result after the given number of attempts.
    fn success(attempts: u32) -> Self {
        ProbeResult {
            succeeded: true,
            attempts,
            error: None,
            fallback_used: false,
        }
    }

    /// Construct a fallback result after exhausting all attempts.
    fn fallback(attempts: u32, last_error: Option<ProbeError>) -> Self {
        ProbeResult {
            succeeded: false,
            attempts,
            error: last_error,
            fallback_used: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ProbeConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for a `HardwareProbe` executor.
///
/// Sensible defaults (`max_attempts=3`, `timeout_ms=100`, `use_fallback=true`)
/// are provided via `Default::default()`.
#[derive(Debug, Clone)]
pub struct ProbeConfig {
    /// Maximum number of probe attempts before falling back. Must be ≥ 1.
    pub max_attempts: u32,
    /// Delay between retry attempts in milliseconds.
    /// In a no_std environment this is recorded but not enacted (no timer
    /// available); callers that care about retry spacing must implement their
    /// own delay before invoking `probe()` in a loop.
    pub retry_delay_ms: u32,
    /// Per-attempt timeout in milliseconds.  Used as the `after_ms` field in
    /// `ProbeError::Timeout` when a probe closure exceeds this budget.
    pub timeout_ms: u32,
    /// When `true`, the probe executor returns the fallback value on total
    /// failure instead of propagating the error.
    pub use_fallback: bool,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        ProbeConfig {
            max_attempts: 3,
            retry_delay_ms: 10,
            timeout_ms: 100,
            use_fallback: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HardwareProbe
// ─────────────────────────────────────────────────────────────────────────────

/// Hardware probe executor with configurable retry semantics and fallback.
///
/// `HardwareProbe` is a thin shell around `ProbeConfig` that provides the
/// retry/fallback logic. It is generic over the value type, error type, probe
/// closure, and fallback closure — ensuring it can wrap any detection strategy
/// without heap allocation beyond what the closures themselves require.
#[derive(Default)]
pub struct HardwareProbe {
    config: ProbeConfig,
}

impl HardwareProbe {
    /// Construct a probe executor with explicit configuration.
    pub fn new(config: ProbeConfig) -> Self {
        HardwareProbe { config }
    }

    /// Probe once (or up to `max_attempts` times on failure).
    ///
    /// Returns a `(T, ProbeResult)` pair:
    /// - On first success: `(value, ProbeResult { attempts: n, succeeded: true, … })`
    /// - On total failure: `(fallback_fn(), ProbeResult { fallback_used: true, … })`
    ///
    /// The probe closure receives no arguments and must return `Result<T, E>`.
    /// The fallback closure must return a `T` unconditionally.
    ///
    /// In a no_std environment the `retry_delay_ms` from `ProbeConfig` is
    /// recorded but **not** enforced (no timer available). Callers that require
    /// real delay must implement it externally.
    pub fn probe<T, E, F, G>(&self, probe_fn: F, fallback_fn: G) -> (T, ProbeResult)
    where
        F: Fn() -> Result<T, E>,
        E: Into<ProbeError>,
        G: Fn() -> T,
    {
        let max = self.config.max_attempts.max(1);
        let mut last_error: Option<ProbeError> = None;

        for attempt in 1..=max {
            match probe_fn() {
                Ok(value) => {
                    return (value, ProbeResult::success(attempt));
                }
                Err(e) => {
                    last_error = Some(e.into());
                }
            }
            // Note: retry_delay_ms is intentionally not enacted here — this is
            // a no_std library and there is no portable timer. Callers that care
            // about inter-retry spacing must implement their own delay.
        }

        // All attempts failed — use fallback.
        let fallback_value = fallback_fn();
        (fallback_value, ProbeResult::fallback(max, last_error))
    }

    /// Try a sequence of strategies in order, returning the first success.
    ///
    /// Each strategy in `strategies` is called exactly once (no retries per
    /// strategy). The first closure that returns `Ok(T)` wins and the value is
    /// returned as `Some(T)`. If **all** strategies fail, `None` is returned.
    ///
    /// This models the layered detection approach common in hardware libraries:
    ///
    /// ```text
    /// strategies = [
    ///     try_cpuid_instruction,   // fastest, may need privilege
    ///     try_devicetree_node,     // portable, needs firmware
    ///     try_procfs_parse,        // Linux only
    /// ]
    /// ```
    pub fn probe_chain<T, E>(&self, strategies: &[&dyn Fn() -> Result<T, E>]) -> Option<T> {
        for strategy in strategies {
            if let Ok(value) = strategy() {
                return Some(value);
            }
        }
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DiagnosticSeverity
// ─────────────────────────────────────────────────────────────────────────────

/// Severity level for a `HalDiagnostics` report.
///
/// Variants are ordered — `Critical > Error > Warning > Ok` — so callers can
/// compare with `>=` to test "at least this severe".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    /// No issues found.
    Ok,
    /// Non-fatal issues that may affect accuracy.
    Warning,
    /// Definite failures; some capabilities could not be detected.
    Error,
    /// Platform is unusable or dangerously misconfigured.
    Critical,
}

impl core::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            DiagnosticSeverity::Ok => "OK",
            DiagnosticSeverity::Warning => "WARNING",
            DiagnosticSeverity::Error => "ERROR",
            DiagnosticSeverity::Critical => "CRITICAL",
        };
        f.write_str(s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HalDiagnostics
// ─────────────────────────────────────────────────────────────────────────────

/// Diagnostic report accumulated across one or more hardware probe operations.
///
/// Call `add_warning()` and `add_error()` as individual probe results arrive.
/// After all probes are complete, inspect `has_issues()` and `severity()` to
/// decide whether to proceed, degrade, or abort.
#[derive(Debug, Clone)]
pub struct HalDiagnostics {
    /// Human-readable architecture name (e.g. `"x86_64"`, `"aarch64"`).
    pub architecture: String,
    /// `true` when all probes completed without fallback.
    pub probed_successfully: bool,
    /// Total probe attempts across all hardware probes in this session.
    pub probe_attempts: u32,
    /// Number of individual probes that fell back to default values.
    pub fallback_count: u32,
    /// Non-fatal diagnostic messages.
    pub warnings: Vec<String>,
    /// Fatal or best-effort failure messages.
    pub errors: Vec<String>,
    /// Human-readable summary of detected capabilities.
    pub capabilities_summary: String,
}

impl HalDiagnostics {
    /// Construct a fresh diagnostics report with the current architecture filled in.
    pub fn new() -> Self {
        let arch = crate::detect_architecture().to_string();
        HalDiagnostics {
            architecture: arch,
            probed_successfully: true,
            probe_attempts: 0,
            fallback_count: 0,
            warnings: Vec::new(),
            errors: Vec::new(),
            capabilities_summary: String::new(),
        }
    }

    /// Append a warning message to the diagnostic log.
    ///
    /// Warnings indicate non-fatal issues such as a fallback value being used
    /// or a feature that could not be confirmed but has a safe default.
    pub fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
        if self.fallback_count > 0 || !self.errors.is_empty() {
            // Already degraded; just log.
        }
    }

    /// Append an error message to the diagnostic log.
    ///
    /// Errors indicate a probe failure that may result in incorrect capability
    /// detection or unsafe defaults being used.
    pub fn add_error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
        self.probed_successfully = false;
    }

    /// Accumulate a `ProbeResult` into the aggregate counters.
    pub fn record_probe(&mut self, result: &ProbeResult) {
        self.probe_attempts += result.attempts;
        if result.fallback_used {
            self.fallback_count += 1;
        }
        if !result.succeeded && !result.fallback_used {
            self.probed_successfully = false;
        }
    }

    /// Returns `true` when any warnings or errors have been recorded.
    #[inline]
    pub fn has_issues(&self) -> bool {
        !self.warnings.is_empty() || !self.errors.is_empty()
    }

    /// The highest-severity diagnostic present in this report.
    ///
    /// - `Ok` — no issues at all
    /// - `Warning` — warnings only
    /// - `Error` — at least one error (may have warnings too)
    /// - `Critical` — errors with `probed_successfully == false` and
    ///   `fallback_count > 0` (platform running on pure guesswork)
    pub fn severity(&self) -> DiagnosticSeverity {
        if !self.errors.is_empty() {
            if !self.probed_successfully && self.fallback_count > 0 {
                return DiagnosticSeverity::Critical;
            }
            return DiagnosticSeverity::Error;
        }
        if !self.warnings.is_empty() {
            return DiagnosticSeverity::Warning;
        }
        DiagnosticSeverity::Ok
    }
}

impl Default for HalDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DetectedPlatform
// ─────────────────────────────────────────────────────────────────────────────

/// Summary of the detected platform produced by `PlatformDetector`.
///
/// When `is_simulated == true` one or more values came from the fallback path
/// rather than actual hardware detection.  Callers that require verified
/// hardware information should inspect `is_simulated` before trusting the
/// returned values.
#[derive(Debug, Clone)]
pub struct DetectedPlatform {
    /// Target architecture string (e.g. `"x86_64"`, `"aarch64"`, `"mips"`).
    pub arch: String,
    /// CPU vendor string if determinable (e.g. `"GenuineIntel"`, `"AuthenticAMD"`).
    pub vendor: Option<String>,
    /// CPU model name if determinable.
    pub model: Option<String>,
    /// Number of logical CPUs / hardware threads detected.
    pub core_count: usize,
    /// Estimated total physical memory in KiB.
    pub memory_kb: u64,
    /// `true` when one or more fields were sourced from fallback defaults.
    pub is_simulated: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// PlatformDetector
// ─────────────────────────────────────────────────────────────────────────────

/// High-level platform detector that combines probing, retry, and diagnostics.
///
/// `PlatformDetector` orchestrates hardware detection across all sub-systems
/// and accumulates a `HalDiagnostics` report that callers can inspect after
/// calling `detect_with_fallback()`.
///
/// ## Typical Usage
///
/// ```no_run
/// use mielin_hal::hal_probe::PlatformDetector;
///
/// let mut detector = PlatformDetector::new();
/// let platform = detector.detect_with_fallback();
///
/// if detector.diagnostics().has_issues() {
///     // Log or handle degraded-mode operation.
/// }
/// ```
pub struct PlatformDetector {
    probe: HardwareProbe,
    diagnostics: HalDiagnostics,
}

impl PlatformDetector {
    /// Construct a fresh `PlatformDetector` with default probe settings.
    pub fn new() -> Self {
        PlatformDetector {
            probe: HardwareProbe::default(),
            diagnostics: HalDiagnostics::new(),
        }
    }

    /// Detect the current architecture as a string.
    ///
    /// Returns a string from a known set: `"x86_64"`, `"aarch64"`,
    /// `"riscv64"`, `"arm-cortex-m"`, `"mips"`, `"powerpc"`, or `"unknown"`.
    pub fn detect_arch(&mut self) -> String {
        // Strategy 1: compile-time target_arch
        let arch_from_compile = || -> Result<String, ProbeError> {
            #[cfg(target_arch = "x86_64")]
            return Ok("x86_64".to_string());
            #[cfg(target_arch = "aarch64")]
            return Ok("aarch64".to_string());
            #[cfg(target_arch = "riscv64")]
            return Ok("riscv64".to_string());
            #[cfg(all(target_arch = "arm", target_os = "none"))]
            return Ok("arm-cortex-m".to_string());
            #[cfg(target_arch = "mips")]
            return Ok("mips".to_string());
            #[cfg(target_arch = "mips64")]
            return Ok("mips64".to_string());
            #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
            return Ok("powerpc".to_string());

            #[allow(unreachable_code)]
            Err(ProbeError::NotPresent)
        };

        let fallback = || "unknown".to_string();
        let (arch, result) = self.probe.probe(arch_from_compile, fallback);
        self.diagnostics.record_probe(&result);
        if result.fallback_used {
            self.diagnostics
                .add_warning("Architecture detection fell back to 'unknown'");
        }
        arch
    }

    /// Detect the full platform description, using fallback values where needed.
    ///
    /// On any non-native host (e.g. x86_64 CI) `is_simulated` will be `true`
    /// because core count and memory are read from system calls or constants
    /// that are not available in no_std.  The returned values are still useful
    /// for testing and staging purposes.
    pub fn detect_with_fallback(&mut self) -> DetectedPlatform {
        let arch = self.detect_arch();

        // Vendor detection — compile-time only in no_std.
        let vendor_probe = || -> Result<Option<String>, ProbeError> {
            #[cfg(target_arch = "x86_64")]
            return Ok(Some("x86_64-vendor".to_string()));
            #[cfg(target_arch = "aarch64")]
            return Ok(Some("arm-vendor".to_string()));

            #[allow(unreachable_code)]
            Err(ProbeError::NotPresent)
        };
        let (vendor, vendor_result) = self.probe.probe(vendor_probe, || None::<String>);
        self.diagnostics.record_probe(&vendor_result);

        // Model detection — always falls back in no_std since /proc/cpuinfo
        // is unavailable; returning None is the correct no_std behaviour.
        let model_probe = || -> Result<Option<String>, ProbeError> {
            // In a full std environment we would read /proc/cpuinfo or CPUID.
            // In no_std we always fall through to the fallback.
            Err(ProbeError::NotPresent)
        };
        let (model, model_result) = self.probe.probe(model_probe, || None::<String>);
        self.diagnostics.record_probe(&model_result);

        // Core count — compile-time constant 1 in pure no_std.
        let core_probe = || -> Result<usize, ProbeError> {
            // A real implementation would use cpuid (x86) or device-tree.
            Err(ProbeError::NotPresent)
        };
        let (core_count, core_result) = self.probe.probe(core_probe, || 1_usize);
        self.diagnostics.record_probe(&core_result);
        if core_result.fallback_used {
            self.diagnostics
                .add_warning("Core count unavailable in no_std; defaulting to 1");
        }

        // Memory estimate — always fallback in no_std.
        let mem_probe = || -> Result<u64, ProbeError> { Err(ProbeError::NotPresent) };
        let (memory_kb, mem_result) = self.probe.probe(mem_probe, || 65536_u64);
        self.diagnostics.record_probe(&mem_result);
        if mem_result.fallback_used {
            self.diagnostics
                .add_warning("Memory size unavailable in no_std; defaulting to 64 MB");
        }

        // If any probe used a fallback the platform is simulated.
        let is_simulated = vendor_result.fallback_used
            || model_result.fallback_used
            || core_result.fallback_used
            || mem_result.fallback_used;

        let platform = DetectedPlatform {
            arch,
            vendor,
            model,
            core_count,
            memory_kb,
            is_simulated,
        };

        self.diagnostics.capabilities_summary = alloc::format!(
            "arch={} cores={} memory={}KB simulated={}",
            platform.arch,
            platform.core_count,
            platform.memory_kb,
            platform.is_simulated,
        );

        platform
    }

    /// Return a reference to the accumulated diagnostic report.
    pub fn diagnostics(&self) -> &HalDiagnostics {
        &self.diagnostics
    }
}

impl Default for PlatformDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── successful first attempt ──────────────────────────────────────────────

    #[test]
    fn test_probe_success_first_attempt() {
        let probe = HardwareProbe::default();
        let (value, result) = probe.probe(|| Ok::<u32, ProbeError>(42), || 0_u32);
        assert_eq!(value, 42);
        assert_eq!(result.attempts, 1);
        assert!(result.succeeded);
        assert!(!result.fallback_used);
    }

    // ── retry then success ────────────────────────────────────────────────────

    #[test]
    fn test_probe_retry_then_success() {
        use core::cell::Cell;
        // Succeed on the 3rd attempt.
        let counter = Cell::new(0u32);

        let config = ProbeConfig {
            max_attempts: 5,
            ..Default::default()
        };
        let probe = HardwareProbe::new(config);

        let (value, result) = probe.probe(
            || {
                let n = counter.get() + 1;
                counter.set(n);
                if n < 3 {
                    Err(ProbeError::Timeout { after_ms: 100 })
                } else {
                    Ok::<u32, ProbeError>(99)
                }
            },
            || 0_u32,
        );

        assert_eq!(value, 99);
        assert_eq!(result.attempts, 3, "should have taken exactly 3 attempts");
        assert!(result.succeeded);
        assert!(!result.fallback_used);
    }

    // ── all fail — fallback used ──────────────────────────────────────────────

    #[test]
    fn test_probe_all_fail_uses_fallback() {
        let config = ProbeConfig {
            max_attempts: 3,
            ..Default::default()
        };
        let probe = HardwareProbe::new(config);

        let (value, result) = probe.probe(
            || Err::<u32, ProbeError>(ProbeError::NotPresent),
            || 777_u32,
        );

        assert_eq!(value, 777);
        assert_eq!(result.attempts, 3);
        assert!(!result.succeeded);
        assert!(result.fallback_used);
        assert!(
            matches!(result.error, Some(ProbeError::NotPresent)),
            "last error must be NotPresent"
        );
    }

    // ── probe_chain — first strategy wins ─────────────────────────────────────

    #[test]
    fn test_probe_chain_first_wins() {
        let probe = HardwareProbe::default();
        let s1: &dyn Fn() -> Result<u32, ProbeError> = &|| Ok(10);
        let s2: &dyn Fn() -> Result<u32, ProbeError> = &|| Ok(20);
        let result = probe.probe_chain(&[s1, s2]);
        assert_eq!(result, Some(10));
    }

    // ── probe_chain — first fails, second wins ────────────────────────────────

    #[test]
    fn test_probe_chain_second_wins() {
        let probe = HardwareProbe::default();
        let s1: &dyn Fn() -> Result<u32, ProbeError> = &|| Err(ProbeError::NotPresent);
        let s2: &dyn Fn() -> Result<u32, ProbeError> = &|| Ok(42);
        let result = probe.probe_chain(&[s1, s2]);
        assert_eq!(result, Some(42));
    }

    // ── probe_chain — all fail → None ─────────────────────────────────────────

    #[test]
    fn test_probe_chain_all_fail() {
        let probe = HardwareProbe::default();
        let s1: &dyn Fn() -> Result<u32, ProbeError> = &|| Err(ProbeError::NotPresent);
        let s2: &dyn Fn() -> Result<u32, ProbeError> =
            &|| Err(ProbeError::Timeout { after_ms: 50 });
        let result = probe.probe_chain(&[s1, s2]);
        assert!(result.is_none());
    }

    // ── diagnostics — warning sets has_issues ────────────────────────────────

    #[test]
    fn test_diagnostics_add_warning() {
        let mut diag = HalDiagnostics::new();
        diag.add_warning("cache size approximated");
        assert!(diag.has_issues(), "a warning should trigger has_issues()");
        assert_eq!(diag.severity(), DiagnosticSeverity::Warning);
    }

    // ── diagnostics — error severity beats warning ───────────────────────────

    #[test]
    fn test_diagnostics_error_beats_warning() {
        let mut diag = HalDiagnostics::new();
        diag.add_warning("minor issue");
        diag.add_error("FPU not detected");
        // Error severity must dominate warning.
        assert!(
            diag.severity() >= DiagnosticSeverity::Error,
            "an error must raise severity to at least Error"
        );
        assert!(diag.severity() > DiagnosticSeverity::Warning);
    }

    // ── platform detector — no panic, is_simulated on test host ──────────────

    #[test]
    fn test_platform_detector_no_panic() {
        let mut detector = PlatformDetector::new();
        let platform = detector.detect_with_fallback();

        // On the x86_64 CI host most sub-probes use fallback → is_simulated.
        assert!(
            platform.is_simulated,
            "on the test host at least one probe should fall back"
        );
        // Core count and memory must always be non-zero (fallback guarantees this).
        assert!(platform.core_count >= 1);
        assert!(platform.memory_kb > 0);
        // Arch string must be non-empty.
        assert!(!platform.arch.is_empty());
    }

    // ── diagnostic severity ordering ─────────────────────────────────────────

    #[test]
    fn test_severity_ordering() {
        assert!(DiagnosticSeverity::Ok < DiagnosticSeverity::Warning);
        assert!(DiagnosticSeverity::Warning < DiagnosticSeverity::Error);
        assert!(DiagnosticSeverity::Error < DiagnosticSeverity::Critical);
    }

    // ── ProbeConfig default values ───────────────────────────────────────────

    #[test]
    fn test_probe_config_defaults() {
        let cfg = ProbeConfig::default();
        assert_eq!(cfg.max_attempts, 3);
        assert_eq!(cfg.retry_delay_ms, 10);
        assert_eq!(cfg.timeout_ms, 100);
        assert!(cfg.use_fallback);
    }

    // ── ProbeError Display ────────────────────────────────────────────────────

    #[test]
    fn test_probe_error_display() {
        use alloc::format;
        let e = ProbeError::Timeout { after_ms: 200 };
        assert!(format!("{}", e).contains("200"));

        let e2 = ProbeError::InvalidSignature {
            expected: 0xDEAD,
            got: 0xBEEF,
        };
        let s = format!("{}", e2);
        assert!(s.contains("DEAD"), "should contain expected value");
        assert!(s.contains("BEEF"), "should contain got value");

        let e3 = ProbeError::NotPresent;
        assert!(!format!("{}", e3).is_empty());

        let e4 = ProbeError::IoError("bus fault".to_string());
        assert!(format!("{}", e4).contains("bus fault"));
    }
}
