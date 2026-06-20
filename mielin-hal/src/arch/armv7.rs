//! ARMv7-A Architecture Support
//!
//! This module provides ARMv7-A hardware abstraction and capability detection
//! for application processors (Cortex-A series) running in Linux userspace or
//! bare-metal environments.
//!
//! ## Supported Extensions
//!
//! - **NEON**: Advanced SIMD (128-bit SIMD, 32 × 64-bit or 16 × 128-bit registers)
//! - **VFPv3**: Vector Floating Point v3 (32 single/double-precision registers)
//! - **VFPv4**: VFPv3 + fused multiply-add (VFMA) instructions
//! - **Thumb-2**: Mixed 16-bit / 32-bit instruction encoding (reduces code size ≈25%)
//! - **IDIV**: Hardware integer divide (`SDIV`/`UDIV`) — available on Cortex-A7/A15+
//! - **FPU**: Generic FPU marker (set when VFPv3 or VFPv4 is present)
//!
//! ## Detection Strategy
//!
//! On native ARMv7-A (`target_arch = "arm"`, `target_feature = "v7"`, not bare-metal)
//! Rust `target_feature` cfg values are used to probe compile-time knowledge of enabled
//! features.  On other hosts safe defaults matching a Cortex-A9 with NEON and VFPv3
//! are returned.
//!
//! ## HardwareCapabilities mapping
//!
//! `detect_armv7_capabilities()` returns a `HardwareCapabilities` bitset used by the
//! top-level `detect_capabilities()` dispatcher:
//!
//! | ARMv7 feature | `HardwareCapabilities` bit |
//! |---------------|---------------------------|
//! | VFPv3 / VFPv4 | `FPU`                    |
//! | NEON          | `SIMD`                   |

use crate::capabilities::HardwareCapabilities;

// ─────────────────────────────────────────────────────────────────────────────
// Capabilities
// ─────────────────────────────────────────────────────────────────────────────

/// Hardware capabilities detected on an ARMv7-A processor.
///
/// All fields are determined at compile time via `target_feature` cfg values when
/// targeting a real ARMv7 host, or set to safe defaults on other platforms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Armv7Capabilities {
    /// NEON Advanced SIMD is available (128-bit vector operations).
    pub has_neon: bool,
    /// VFPv3 floating-point unit is present.
    pub has_vfpv3: bool,
    /// VFPv4 floating-point unit (VFPv3 + fused multiply-add) is present.
    pub has_vfpv4: bool,
    /// Thumb-2 mixed 16/32-bit instruction encoding is available.
    pub has_thumb2: bool,
    /// Hardware integer divide (`SDIV`/`UDIV`) is available (Cortex-A7/A15+).
    pub has_idiv: bool,
}

impl Default for Armv7Capabilities {
    /// Safe defaults matching a Cortex-A9 with NEON and VFPv3 (most common ARMv7-A
    /// deployment in the 2010–2015 era).
    fn default() -> Self {
        Armv7Capabilities {
            has_neon: true,
            has_vfpv3: true,
            has_vfpv4: false,
            has_thumb2: true,
            has_idiv: false,
        }
    }
}

impl Armv7Capabilities {
    /// Detect ARMv7-A capabilities.
    ///
    /// Uses compile-time `target_feature` probes on a native ARMv7 host; falls
    /// back to `Default` on all other platforms (MIPS, x86, AArch64, …).
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "arm", target_feature = "v7"))]
        {
            return Self::detect_native();
        }

        Self::default()
    }

    /// Native ARMv7-A detection via `target_feature` cfg probes.
    ///
    /// Rust exposes compile-time knowledge of enabled instruction-set extensions
    /// through `cfg(target_feature = "…")`.  The linker/loader may enable features
    /// at runtime (e.g. Linux kernel auxiliary vector `AT_HWCAP`), but for HAL
    /// purposes compile-time guarantees are sufficient.
    #[cfg(all(target_arch = "arm", target_feature = "v7"))]
    fn detect_native() -> Self {
        let has_neon = cfg!(target_feature = "neon");
        let has_vfpv4 = cfg!(target_feature = "vfp4");
        let has_vfpv3 = has_vfpv4 || cfg!(target_feature = "vfp3");
        let has_thumb2 = cfg!(target_feature = "thumb2") || cfg!(target_feature = "thumb-2");
        let has_idiv = cfg!(target_feature = "idiv");

        Armv7Capabilities {
            has_neon,
            has_vfpv3,
            has_vfpv4,
            has_thumb2,
            has_idiv,
        }
    }

    /// Returns `true` when any floating-point hardware is available.
    #[inline]
    pub fn has_fpu(&self) -> bool {
        self.has_vfpv3 || self.has_vfpv4
    }

    /// Returns `true` when SIMD (NEON) is available.
    #[inline]
    pub fn has_simd(&self) -> bool {
        self.has_neon
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HardwareCapabilities bridge
// ─────────────────────────────────────────────────────────────────────────────

/// Detect ARMv7-A capabilities and return the corresponding `HardwareCapabilities`
/// bitset for use by the top-level `detect_capabilities()` dispatcher.
///
/// Mapping:
/// - VFPv3 or VFPv4 present → `HardwareCapabilities::FPU`
/// - NEON present           → `HardwareCapabilities::SIMD`
pub fn detect_armv7_capabilities() -> HardwareCapabilities {
    let armv7 = Armv7Capabilities::detect();
    let mut caps = HardwareCapabilities::empty();

    if armv7.has_fpu() {
        caps |= HardwareCapabilities::FPU;
    }
    if armv7.has_simd() {
        caps |= HardwareCapabilities::SIMD;
    }

    caps
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_armv7_default_capabilities() {
        let caps = Armv7Capabilities::default();
        // Default Cortex-A9 profile
        assert!(caps.has_neon, "default must have NEON");
        assert!(caps.has_vfpv3, "default must have VFPv3");
        assert!(!caps.has_vfpv4, "default Cortex-A9 does not have VFPv4");
        assert!(caps.has_thumb2, "default must have Thumb-2");
        assert!(!caps.has_idiv, "default Cortex-A9 does not have IDIV");
    }

    #[test]
    fn test_armv7_has_fpu_vfpv3() {
        let caps = Armv7Capabilities {
            has_vfpv3: true,
            has_vfpv4: false,
            ..Armv7Capabilities::default()
        };
        assert!(caps.has_fpu(), "VFPv3 alone should set has_fpu()");
    }

    #[test]
    fn test_armv7_has_fpu_vfpv4() {
        let caps = Armv7Capabilities {
            has_vfpv3: true,
            has_vfpv4: true,
            ..Armv7Capabilities::default()
        };
        assert!(caps.has_fpu(), "VFPv4 should set has_fpu()");
    }

    #[test]
    fn test_armv7_no_fpu_when_neither_vfp() {
        let caps = Armv7Capabilities {
            has_vfpv3: false,
            has_vfpv4: false,
            ..Armv7Capabilities::default()
        };
        assert!(!caps.has_fpu(), "no VFP → has_fpu() must be false");
    }

    #[test]
    fn test_armv7_has_simd_when_neon() {
        let caps = Armv7Capabilities {
            has_neon: true,
            ..Armv7Capabilities::default()
        };
        assert!(caps.has_simd());
    }

    #[test]
    fn test_armv7_no_simd_without_neon() {
        let caps = Armv7Capabilities {
            has_neon: false,
            ..Armv7Capabilities::default()
        };
        assert!(!caps.has_simd());
    }

    #[test]
    fn test_detect_armv7_capabilities_does_not_panic() {
        // Must run on any host (x86_64 CI included) without panicking.
        let _caps = detect_armv7_capabilities();
    }

    #[test]
    fn test_armv7_detect_returns_capabilities() {
        let caps = Armv7Capabilities::detect();
        // On any host detect() must return a coherent struct:
        // if vfpv4 is set then vfpv3 must also be set.
        if caps.has_vfpv4 {
            assert!(
                caps.has_vfpv3,
                "VFPv4 implies VFPv3 — inconsistent detect() result"
            );
        }
    }
}
