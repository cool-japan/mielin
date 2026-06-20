//! 32-bit x86 (IA-32 / i686) Architecture Support
//!
//! This module provides x86 32-bit hardware abstraction and capability detection.
//! It targets the `i686-unknown-linux-gnu` and related IA-32 targets that remain
//! in use for legacy embedded controllers and 32-bit Linux deployments.
//!
//! ## Detected Capabilities (via CPUID)
//!
//! | Feature  | CPUID leaf / bit                  | Era              |
//! |----------|-----------------------------------|------------------|
//! | SSE      | 1.EDX[25]                         | Pentium III 1999 |
//! | SSE2     | 1.EDX[26]                         | Pentium 4   2001 |
//! | SSE3     | 1.ECX[0]                          | Prescott    2004 |
//! | SSSE3    | 1.ECX[9]                          | Core 2      2007 |
//! | SSE4.1   | 1.ECX[19]                         | Penryn      2007 |
//! | SSE4.2   | 1.ECX[20]                         | Nehalem     2008 |
//! | AVX      | 1.ECX[28]                         | Sandy Bridge 2011|
//! | AVX2     | 7.EBX[5]  (sub-leaf 0)            | Haswell      2013|
//! | FMA      | 1.ECX[12]                         | Haswell      2013|
//! | AES-NI   | 1.ECX[25]                         | Westmere     2010|
//!
//! ## Detection Strategy
//!
//! On a native `target_arch = "x86"` host the CPUID instruction is executed via
//! `core::arch::x86`.  On all other hosts safe conservative defaults are returned
//! (SSE + SSE2 only — the minimum guaranteed baseline for a i686 build in 2010+).
//!
//! ## HardwareCapabilities mapping
//!
//! `detect_x86_32_capabilities()` returns a `HardwareCapabilities` bitset:
//!
//! | x86 feature          | `HardwareCapabilities` bit |
//! |----------------------|---------------------------|
//! | SSE / SSE2 / higher  | `FPU`                    |
//! | SSE / AVX / AVX2     | `SIMD`                   |

use crate::capabilities::HardwareCapabilities;

// ─────────────────────────────────────────────────────────────────────────────
// Capabilities
// ─────────────────────────────────────────────────────────────────────────────

/// Hardware capabilities detected on a 32-bit x86 (IA-32 / i686) processor.
///
/// Fields are populated by executing the CPUID instruction on a native x86 host,
/// or set to conservative defaults on all other platforms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X86Capabilities {
    /// Streaming SIMD Extensions (SSE) — Pentium III, 1999.
    pub has_sse: bool,
    /// SSE2 — Pentium 4, 2001.
    pub has_sse2: bool,
    /// SSE3 (Prescott New Instructions) — Prescott, 2004.
    pub has_sse3: bool,
    /// Supplemental SSE3 (SSSE3) — Core 2, 2007.
    pub has_ssse3: bool,
    /// SSE4.1 — Penryn, 2007.
    pub has_sse4_1: bool,
    /// SSE4.2 — Nehalem, 2008.
    pub has_sse4_2: bool,
    /// Advanced Vector Extensions (AVX) — Sandy Bridge, 2011.
    pub has_avx: bool,
    /// AVX2 (256-bit integer SIMD) — Haswell, 2013.
    pub has_avx2: bool,
    /// Fused Multiply-Add (FMA3) — Haswell, 2013.
    pub has_fma: bool,
    /// AES hardware acceleration (AES-NI) — Westmere, 2010.
    pub has_aes: bool,
}

impl Default for X86Capabilities {
    /// Conservative defaults for a generic i686 target (SSE + SSE2 minimum).
    ///
    /// SSE and SSE2 are required by the i686 ABI on Linux since glibc 2.10, so
    /// returning `true` here is safe for any modern 32-bit x86 deployment.
    fn default() -> Self {
        X86Capabilities {
            has_sse: true,
            has_sse2: true,
            has_sse3: false,
            has_ssse3: false,
            has_sse4_1: false,
            has_sse4_2: false,
            has_avx: false,
            has_avx2: false,
            has_fma: false,
            has_aes: false,
        }
    }
}

impl X86Capabilities {
    /// Detect x86 32-bit capabilities.
    ///
    /// Executes CPUID on a native `target_arch = "x86"` host.  On all other
    /// hosts (AArch64, RISC-V, ARMv7, x86_64 …) the conservative `Default` is
    /// returned immediately.
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86")]
        return Self::detect_native();

        #[cfg(not(target_arch = "x86"))]
        Self::default()
    }

    /// Native x86 32-bit capability detection via CPUID.
    #[cfg(target_arch = "x86")]
    fn detect_native() -> Self {
        use core::arch::x86::{__cpuid, __cpuid_count};

        let leaf1 = unsafe { __cpuid(1) };
        let edx1 = leaf1.edx;
        let ecx1 = leaf1.ecx;

        let has_sse = (edx1 & (1 << 25)) != 0;
        let has_sse2 = (edx1 & (1 << 26)) != 0;
        let has_sse3 = (ecx1 & (1 << 0)) != 0;
        let has_ssse3 = (ecx1 & (1 << 9)) != 0;
        let has_sse4_1 = (ecx1 & (1 << 19)) != 0;
        let has_sse4_2 = (ecx1 & (1 << 20)) != 0;
        let has_avx = (ecx1 & (1 << 28)) != 0;
        let has_fma = (ecx1 & (1 << 12)) != 0;
        let has_aes = (ecx1 & (1 << 25)) != 0;

        // AVX2 is in extended leaf 7, sub-leaf 0, EBX[5].
        let has_avx2 = {
            let max_basic = unsafe { __cpuid(0) }.eax;
            if max_basic >= 7 {
                let leaf7 = unsafe { __cpuid_count(7, 0) };
                (leaf7.ebx & (1 << 5)) != 0
            } else {
                false
            }
        };

        X86Capabilities {
            has_sse,
            has_sse2,
            has_sse3,
            has_ssse3,
            has_sse4_1,
            has_sse4_2,
            has_avx,
            has_avx2,
            has_fma,
            has_aes,
        }
    }

    /// Returns `true` when any SSE-family or higher floating-point SIMD is available.
    #[inline]
    pub fn has_fpu(&self) -> bool {
        self.has_sse || self.has_sse2
    }

    /// Returns `true` when any vector SIMD extension is available.
    #[inline]
    pub fn has_simd(&self) -> bool {
        self.has_sse || self.has_avx || self.has_avx2
    }

    /// Returns the highest SSE generation available as a human-readable label.
    pub fn sse_level(&self) -> &'static str {
        if self.has_avx2 {
            "AVX2"
        } else if self.has_avx {
            "AVX"
        } else if self.has_sse4_2 {
            "SSE4.2"
        } else if self.has_sse4_1 {
            "SSE4.1"
        } else if self.has_ssse3 {
            "SSSE3"
        } else if self.has_sse3 {
            "SSE3"
        } else if self.has_sse2 {
            "SSE2"
        } else if self.has_sse {
            "SSE"
        } else {
            "none"
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HardwareCapabilities bridge
// ─────────────────────────────────────────────────────────────────────────────

/// Detect 32-bit x86 capabilities and return the corresponding
/// `HardwareCapabilities` bitset for the top-level `detect_capabilities()`.
///
/// Mapping:
/// - SSE or SSE2 present → `HardwareCapabilities::FPU`
/// - SSE, AVX, or AVX2  → `HardwareCapabilities::SIMD`
pub fn detect_x86_32_capabilities() -> HardwareCapabilities {
    let x86 = X86Capabilities::detect();
    let mut caps = HardwareCapabilities::empty();

    if x86.has_fpu() {
        caps |= HardwareCapabilities::FPU;
    }
    if x86.has_simd() {
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
    fn test_x86_default_capabilities() {
        let caps = X86Capabilities::default();
        assert!(caps.has_sse, "default i686 must have SSE");
        assert!(caps.has_sse2, "default i686 must have SSE2");
        // Conservative defaults: higher extensions off.
        assert!(!caps.has_avx, "default i686 should not claim AVX");
        assert!(!caps.has_avx2, "default i686 should not claim AVX2");
    }

    #[test]
    fn test_x86_has_fpu_with_sse() {
        let caps = X86Capabilities {
            has_sse: true,
            has_sse2: false,
            ..X86Capabilities::default()
        };
        assert!(caps.has_fpu(), "SSE alone should set has_fpu()");
    }

    #[test]
    fn test_x86_no_fpu_without_sse() {
        let caps = X86Capabilities {
            has_sse: false,
            has_sse2: false,
            ..X86Capabilities::default()
        };
        assert!(!caps.has_fpu(), "no SSE → has_fpu() must be false");
    }

    #[test]
    fn test_x86_has_simd_with_avx2() {
        let caps = X86Capabilities {
            has_sse: false,
            has_sse2: false,
            has_avx: false,
            has_avx2: true,
            ..X86Capabilities::default()
        };
        assert!(caps.has_simd(), "AVX2 alone should set has_simd()");
    }

    #[test]
    fn test_x86_sse_level_avx2() {
        let caps = X86Capabilities {
            has_sse: true,
            has_sse2: true,
            has_sse3: true,
            has_ssse3: true,
            has_sse4_1: true,
            has_sse4_2: true,
            has_avx: true,
            has_avx2: true,
            has_fma: true,
            has_aes: true,
        };
        assert_eq!(caps.sse_level(), "AVX2");
    }

    #[test]
    fn test_x86_sse_level_sse2_only() {
        let caps = X86Capabilities {
            has_sse: false,
            has_sse2: true,
            ..X86Capabilities::default()
        };
        assert_eq!(caps.sse_level(), "SSE2");
    }

    #[test]
    fn test_x86_sse_level_none() {
        let caps = X86Capabilities {
            has_sse: false,
            has_sse2: false,
            ..X86Capabilities::default()
        };
        assert_eq!(caps.sse_level(), "none");
    }

    #[test]
    fn test_detect_x86_32_capabilities_does_not_panic() {
        // Must complete without panic on any host (AArch64, x86_64 CI, …).
        let _caps = detect_x86_32_capabilities();
    }

    #[test]
    fn test_x86_detect_returns_capabilities() {
        let caps = X86Capabilities::detect();
        // AVX2 implies SSE, SSE2, SSE3, SSSE3, SSE4.1, SSE4.2, AVX on real HW.
        // We only check the internal consistency rule that detect() is coherent
        // (i.e. it does not panic and returns a struct where AVX2 implies AVX).
        if caps.has_avx2 {
            assert!(
                caps.has_avx,
                "AVX2 implies AVX — inconsistent detect() result"
            );
        }
    }
}
