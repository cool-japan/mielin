//! PowerPC Architecture Support
//!
//! This module provides PowerPC hardware abstraction and capability detection
//! for both classic 32-bit embedded cores and modern 64-bit POWER server
//! processors. It covers the entire range from legacy MPC8XX embedded SoCs
//! through IBM POWER10 server chips.
//!
//! ## Supported Variants
//!
//! - **PowerPc32** — Classic 32-bit PowerPC (e.g. MPC8XX, MPC5XX, G3/G4 embedded)
//! - **PowerPc64** — 64-bit big-endian (IBM POWER4/5/6/7, RS6000)
//! - **PowerPc64Le** — 64-bit little-endian (IBM POWER8/9/10, modern Linux ppc64le)
//! - **Ppc440** — IBM 440 embedded core series (book-E architecture)
//! - **E500** — Freescale/NXP e500/e500mc (Power ISA embedded profile)
//!
//! ## SIMD/Vector Extensions
//!
//! PowerPC's vector extension history spans three generations:
//!
//! 1. **AltiVec** (a.k.a. VMX) — Classic 128-bit SIMD introduced on G4.
//!    Provides 32 × 128-bit vector registers, integer and float operations.
//! 2. **VSX** — Vector-Scalar Extension (POWER7+). 64 × 128-bit registers
//!    shared between FPU and vector units, IEEE 754-2008 compliance.
//! 3. **VSX version 3** (POWER9+) — Additional instructions, improved
//!    performance for machine-learning workloads.
//!
//! ## Cache Architecture
//!
//! Server-class POWER processors have large, multi-level caches:
//!
//! - L1: 32–64 KB per core (VIPT, 128-byte lines on POWER)
//! - L2: 256 KB – 2 MB per core (POWER7+)
//! - L3: 8–120 MB shared (POWER9/10)
//!
//! The 128-byte cache line size is characteristic of POWER and affects
//! prefetching, false sharing, and lock implementation.
//!
//! ## Endianness
//!
//! Classic PowerPC and POWER servers are big-endian. POWER8 and later
//! support hardware little-endian mode (PowerPC64LE), which is now the
//! default for modern Linux/POWER distributions.
//!
//! ## Detection Strategy
//!
//! On native POWER hardware the `mfspr PVR` instruction reads the Processor
//! Version Register. On non-PPC hosts safe defaults are returned. Linux
//! `/proc/cpuinfo` provides a rich source of information but is not available
//! in a no_std environment.

extern crate alloc;

use alloc::string::String;

// ─────────────────────────────────────────────────────────────────────────────
// Variant
// ─────────────────────────────────────────────────────────────────────────────

/// PowerPC / POWER processor variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPcVariant {
    /// Classic 32-bit PowerPC (e.g. MPC8XX, MPC5XX, PowerPC 604/750).
    PowerPc32,
    /// 64-bit big-endian (IBM POWER4/5/6/7, G5 desktop).
    PowerPc64,
    /// 64-bit little-endian — modern POWER8/9/10, the dominant Linux target.
    PowerPc64Le,
    /// IBM 440 embedded core series (Book-E architecture, 32-bit).
    Ppc440,
    /// Freescale/NXP e500 / e500mc (Power ISA embedded profile, 32-bit).
    E500,
}

impl PowerPcVariant {
    /// Returns `true` for 64-bit variants.
    #[inline]
    pub fn is_64bit(self) -> bool {
        matches!(
            self,
            PowerPcVariant::PowerPc64 | PowerPcVariant::PowerPc64Le
        )
    }

    /// Returns `true` for server-class variants (POWER8/9/10 era).
    ///
    /// PowerPc64Le targets are modern POWER8+ running little-endian Linux.
    /// Plain PowerPc64 may be older big-endian servers (POWER4-POWER7).
    /// Embedded variants (PowerPc32, Ppc440, E500) are never server-class.
    #[inline]
    pub fn is_server_class(self) -> bool {
        matches!(self, PowerPcVariant::PowerPc64Le)
    }

    /// Human-readable identifier for the variant.
    pub fn as_str(self) -> &'static str {
        match self {
            PowerPcVariant::PowerPc32 => "powerpc32",
            PowerPcVariant::PowerPc64 => "powerpc64",
            PowerPcVariant::PowerPc64Le => "powerpc64le",
            PowerPcVariant::Ppc440 => "ppc440",
            PowerPcVariant::E500 => "e500",
        }
    }
}

impl core::fmt::Display for PowerPcVariant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AltiVec / VSX support level
// ─────────────────────────────────────────────────────────────────────────────

/// AltiVec / VMX / VSX SIMD capability tier.
///
/// The tiers are strictly ordered — each successive tier is a superset of the
/// previous one. Callers should test against the minimum tier they require:
///
/// ```rust
/// # use mielin_hal::arch::powerpc::AltiVecSupport;
/// let support = AltiVecSupport::Vsx;
/// if support >= AltiVecSupport::AltiVec {
///     // Classic AltiVec operations are available
/// }
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AltiVecSupport {
    /// No vector unit present (embedded cores, e500 without AltiVec).
    #[default]
    None,
    /// Classic AltiVec / VMX — 32 × 128-bit registers, introduced on G4.
    AltiVec,
    /// VSX — Vector-Scalar Extension (POWER7+), 64 × 128-bit scalar/vector registers.
    Vsx,
    /// VSX version 3 (POWER9+) — additional instructions, bfloat16 support.
    Vsx3,
}

impl AltiVecSupport {
    /// Returns `true` when any form of vector unit is available.
    #[inline]
    pub fn has_simd(self) -> bool {
        self != AltiVecSupport::None
    }

    /// Human-readable name.
    pub fn as_str(self) -> &'static str {
        match self {
            AltiVecSupport::None => "none",
            AltiVecSupport::AltiVec => "altivec",
            AltiVecSupport::Vsx => "vsx",
            AltiVecSupport::Vsx3 => "vsx3",
        }
    }
}

impl core::fmt::Display for AltiVecSupport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Endianness
// ─────────────────────────────────────────────────────────────────────────────

/// PowerPC endianness configuration.
///
/// Classic PowerPC and IBM POWER (up to POWER7) are big-endian. POWER8 added
/// hardware little-endian support; modern distributions (Fedora, Ubuntu) use
/// little-endian exclusively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPcEndianness {
    /// Classic PowerPC / POWER big-endian.
    BigEndian,
    /// PowerPC64LE — little-endian (POWER8/9/10 under Linux).
    LittleEndian,
}

impl PowerPcEndianness {
    /// Detect native endianness from compile-time `target_endian`.
    #[inline]
    pub fn detect() -> Self {
        #[cfg(target_endian = "big")]
        return PowerPcEndianness::BigEndian;

        #[cfg(not(target_endian = "big"))]
        PowerPcEndianness::LittleEndian
    }

    /// Returns `true` for big-endian configurations.
    #[inline]
    pub fn is_big_endian(self) -> bool {
        self == PowerPcEndianness::BigEndian
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Capabilities
// ─────────────────────────────────────────────────────────────────────────────

/// Hardware capabilities for a PowerPC / POWER processor.
#[derive(Debug, Clone)]
pub struct PowerPcCapabilities {
    /// Identified variant, if determinable.
    pub variant: Option<PowerPcVariant>,
    /// Level of AltiVec / VMX / VSX vector support.
    pub altivec: AltiVecSupport,
    /// Scalar IEEE 754 floating-point unit.
    pub has_fpu: bool,
    /// SPE — Signal Processing Extension (e500 variant, mutually exclusive with AltiVec).
    pub has_spe: bool,
    /// Crypto extensions (AES, SHA) present in POWER8+.
    pub has_crypto: bool,
    /// Hardware Transactional Memory (POWER8+).
    pub has_htm: bool,
    /// HTM without the "Suspend" capability (POWER9 restricted-TSX mode).
    pub has_htm_no_suspend: bool,
}

impl Default for PowerPcCapabilities {
    fn default() -> Self {
        PowerPcCapabilities {
            variant: Some(PowerPcVariant::PowerPc64Le),
            altivec: AltiVecSupport::AltiVec,
            has_fpu: true,
            has_spe: false,
            has_crypto: false,
            has_htm: false,
            has_htm_no_suspend: false,
        }
    }
}

impl PowerPcCapabilities {
    /// Detect capabilities from hardware registers.
    ///
    /// On native PowerPC hosts the PVR (Processor Version Register) is read via
    /// `mfspr` to identify the chip generation, then AltiVec/VSX presence is
    /// inferred. On non-PPC hosts safe defaults for a generic PowerPc64Le with
    /// AltiVec are returned.
    pub fn detect() -> Self {
        #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
        {
            return Self::detect_native();
        }

        #[allow(unreachable_code)]
        Self::default()
    }

    /// Returns `true` when any form of SIMD (AltiVec, VSX) is available.
    #[inline]
    pub fn has_simd(&self) -> bool {
        self.altivec.has_simd()
    }

    /// Returns `true` for 64-bit variants (PowerPc64 or PowerPc64Le).
    #[inline]
    pub fn is_64bit(&self) -> bool {
        self.variant.map(|v| v.is_64bit()).unwrap_or(false)
    }

    /// Native detection via PVR register read.
    ///
    /// PVR encodes [31:16] = version, [15:0] = revision.
    /// Well-known version values allow us to identify the chip generation.
    #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
    fn detect_native() -> Self {
        let pvr: u32;
        unsafe {
            core::arch::asm!(
                "mfspr {0}, 287",   // SPR 287 = PVR
                out(reg) pvr,
                options(nomem, nostack, preserves_flags),
            );
        }

        let version = pvr >> 16;
        let mut caps = Self::default();

        // Map PVR version to variant and capability tier.
        // Reference: Linux arch/powerpc/include/asm/cputable.h
        match version {
            // Classic 32-bit PowerPC: 740 (G3), 7400/7410/7450/7455 (G4)
            0x0008 | 0x0020 | 0x0024 | 0x0070 | 0x0083 | 0x0084 => {
                caps.variant = Some(PowerPcVariant::PowerPc32);
                // G4 (7400+) has AltiVec; G3 does not.
                caps.altivec = if version >= 0x000C {
                    AltiVecSupport::AltiVec
                } else {
                    AltiVecSupport::None
                };
                caps.has_fpu = true;
            }
            // IBM 440 embedded series
            0x1291 | 0x7ff2 => {
                caps.variant = Some(PowerPcVariant::Ppc440);
                caps.altivec = AltiVecSupport::None;
                caps.has_fpu = true;
            }
            // e500 / e500mc (Freescale)
            0x8020 | 0x8021 | 0x8022 | 0x8023 | 0x8024 => {
                caps.variant = Some(PowerPcVariant::E500);
                caps.altivec = AltiVecSupport::None;
                caps.has_spe = true;
                caps.has_fpu = false; // e500 has SPE not FPU
            }
            // POWER4 / POWER4+
            0x0035 | 0x0038 => {
                caps.variant = Some(PowerPcVariant::PowerPc64);
                caps.altivec = AltiVecSupport::AltiVec;
                caps.has_fpu = true;
            }
            // POWER5 / POWER5+
            0x003A | 0x003B => {
                caps.variant = Some(PowerPcVariant::PowerPc64);
                caps.altivec = AltiVecSupport::AltiVec;
                caps.has_fpu = true;
            }
            // POWER6
            0x003E => {
                caps.variant = Some(PowerPcVariant::PowerPc64);
                caps.altivec = AltiVecSupport::AltiVec;
                caps.has_fpu = true;
            }
            // POWER7 — introduces VSX
            0x003F => {
                caps.variant = Some(PowerPcVariant::PowerPc64);
                caps.altivec = AltiVecSupport::Vsx;
                caps.has_fpu = true;
            }
            // POWER8 — adds Crypto, HTM, little-endian support
            0x004B | 0x004C | 0x004D => {
                caps.variant = Some(PowerPcVariant::PowerPc64Le);
                caps.altivec = AltiVecSupport::Vsx;
                caps.has_fpu = true;
                caps.has_crypto = true;
                caps.has_htm = true;
            }
            // POWER9 — VSX3, HTM-no-suspend, enhanced crypto
            0x004E => {
                caps.variant = Some(PowerPcVariant::PowerPc64Le);
                caps.altivec = AltiVecSupport::Vsx3;
                caps.has_fpu = true;
                caps.has_crypto = true;
                caps.has_htm = true;
                caps.has_htm_no_suspend = true;
            }
            // POWER10 (and future POWER11)
            0x0080 | 0x0082 => {
                caps.variant = Some(PowerPcVariant::PowerPc64Le);
                caps.altivec = AltiVecSupport::Vsx3;
                caps.has_fpu = true;
                caps.has_crypto = true;
                caps.has_htm = true;
                caps.has_htm_no_suspend = true;
            }
            // Unknown — assume safe defaults
            _ => {
                caps.variant = Some(PowerPcVariant::PowerPc64Le);
                caps.altivec = AltiVecSupport::AltiVec;
                caps.has_fpu = true;
            }
        }

        caps
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cache Info
// ─────────────────────────────────────────────────────────────────────────────

/// PowerPC / POWER cache topology.
///
/// Server-class POWER processors are notable for their 128-byte L1 cache line,
/// which is twice that of typical x86_64 processors.  This strongly influences
/// padding, alignment, and false-sharing avoidance strategies.
#[derive(Debug, Clone)]
pub struct PowerPcCacheInfo {
    /// L1 instruction cache size in KiB.
    pub l1_icache_kb: u32,
    /// L1 data cache size in KiB.
    pub l1_dcache_kb: u32,
    /// L2 unified cache size in KiB (None on cores without L2).
    pub l2_cache_kb: Option<u32>,
    /// L3 unified cache size in MiB (None on embedded cores without L3).
    pub l3_cache_mb: Option<u32>,
    /// Cache line size in bytes (128 B on POWER servers, 32 B on embedded).
    pub cache_line_bytes: u32,
}

impl Default for PowerPcCacheInfo {
    /// Defaults match a modern POWER8 server core with 128-byte cache lines.
    fn default() -> Self {
        PowerPcCacheInfo {
            l1_icache_kb: 32,
            l1_dcache_kb: 32,
            l2_cache_kb: Some(512),
            l3_cache_mb: Some(8),
            cache_line_bytes: 128,
        }
    }
}

impl PowerPcCacheInfo {
    /// Detect cache topology from hardware registers or return safe defaults.
    ///
    /// On native PPC hosts this attempts to read cache descriptor registers;
    /// on any other host the defaults for a generic POWER8 server are returned.
    pub fn detect() -> Self {
        #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
        {
            return Self::detect_native();
        }

        #[allow(unreachable_code)]
        Self::default()
    }

    #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
    fn detect_native() -> Self {
        // The PVR tells us the generation; we use well-known cache sizes.
        let pvr: u32;
        unsafe {
            core::arch::asm!(
                "mfspr {0}, 287",
                out(reg) pvr,
                options(nomem, nostack, preserves_flags),
            );
        }

        let version = pvr >> 16;

        match version {
            // Classic G3/G4 embedded: 32 KB caches, 32-byte lines.
            0x0008 | 0x0020 | 0x0024 | 0x0070 => PowerPcCacheInfo {
                l1_icache_kb: 32,
                l1_dcache_kb: 32,
                l2_cache_kb: Some(256),
                l3_cache_mb: None,
                cache_line_bytes: 32,
            },
            // Ppc440 / e500: small caches, 32-byte lines.
            0x1291 | 0x7ff2 | 0x8020..=0x8024 => PowerPcCacheInfo {
                l1_icache_kb: 16,
                l1_dcache_kb: 16,
                l2_cache_kb: None,
                l3_cache_mb: None,
                cache_line_bytes: 32,
            },
            // POWER7: 32KB L1, 256KB L2, 4MB L3, 128-byte lines.
            0x003F => PowerPcCacheInfo {
                l1_icache_kb: 32,
                l1_dcache_kb: 32,
                l2_cache_kb: Some(256),
                l3_cache_mb: Some(4),
                cache_line_bytes: 128,
            },
            // POWER8: 32KB L1, 512KB L2, 8MB L3.
            0x004B | 0x004C | 0x004D => PowerPcCacheInfo {
                l1_icache_kb: 32,
                l1_dcache_kb: 32,
                l2_cache_kb: Some(512),
                l3_cache_mb: Some(8),
                cache_line_bytes: 128,
            },
            // POWER9: 32KB L1, 512KB L2, 10MB L3.
            0x004E => PowerPcCacheInfo {
                l1_icache_kb: 32,
                l1_dcache_kb: 32,
                l2_cache_kb: Some(512),
                l3_cache_mb: Some(10),
                cache_line_bytes: 128,
            },
            // POWER10: 48KB L1, 2MB L2, 120MB L3.
            0x0080 | 0x0082 => PowerPcCacheInfo {
                l1_icache_kb: 48,
                l1_dcache_kb: 32,
                l2_cache_kb: Some(2048),
                l3_cache_mb: Some(120),
                cache_line_bytes: 128,
            },
            // Default fallback for unrecognised POWER chips.
            _ => Self::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform
// ─────────────────────────────────────────────────────────────────────────────

/// Complete PowerPC / POWER platform descriptor.
pub struct PowerPcPlatform {
    /// Identified processor variant.
    pub variant: PowerPcVariant,
    /// Full capability set.
    pub capabilities: PowerPcCapabilities,
    /// Cache topology.
    pub cache: PowerPcCacheInfo,
    /// Byte order.
    pub endianness: PowerPcEndianness,
}

impl PowerPcPlatform {
    /// Detect the full PowerPC / POWER platform description.
    ///
    /// Never panics — on non-PPC hosts all fields return safe defaults.
    pub fn detect() -> Self {
        let capabilities = PowerPcCapabilities::detect();
        let cache = PowerPcCacheInfo::detect();
        let endianness = PowerPcEndianness::detect();
        let variant = capabilities.variant.unwrap_or(PowerPcVariant::PowerPc64Le);

        PowerPcPlatform {
            variant,
            capabilities,
            cache,
            endianness,
        }
    }

    /// Returns `true` when this is a server-class POWER processor (POWER8/9/10).
    ///
    /// Server-class implies little-endian 64-bit mode, VSX, hardware crypto,
    /// and large multi-level caches — characteristics that distinguish these
    /// processors from embedded PowerPC variants.
    #[inline]
    pub fn is_server_class(&self) -> bool {
        self.variant.is_server_class()
    }

    /// Returns `true` when the platform is big-endian.
    #[inline]
    pub fn is_big_endian(&self) -> bool {
        self.endianness.is_big_endian()
    }

    /// Compact human-readable description of this platform.
    pub fn description(&self) -> String {
        use alloc::format;
        format!(
            "{} {}  altivec={}  crypto={}  l1={}KB+{}KB  line={}B",
            self.variant,
            if self.capabilities.is_64bit() {
                "64-bit"
            } else {
                "32-bit"
            },
            self.capabilities.altivec,
            if self.capabilities.has_crypto {
                "yes"
            } else {
                "no"
            },
            self.cache.l1_icache_kb,
            self.cache.l1_dcache_kb,
            self.cache.cache_line_bytes,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── variant bitwidth ──────────────────────────────────────────────────────

    #[test]
    fn test_ppc_variant_64bit() {
        assert!(
            PowerPcVariant::PowerPc64.is_64bit(),
            "PowerPc64 must be 64-bit"
        );
        assert!(
            PowerPcVariant::PowerPc64Le.is_64bit(),
            "PowerPc64Le must be 64-bit"
        );
    }

    #[test]
    fn test_ppc_variant_32bit() {
        assert!(
            !PowerPcVariant::PowerPc32.is_64bit(),
            "PowerPc32 must not be 64-bit"
        );
        assert!(
            !PowerPcVariant::Ppc440.is_64bit(),
            "Ppc440 must not be 64-bit"
        );
        assert!(!PowerPcVariant::E500.is_64bit(), "E500 must not be 64-bit");
    }

    // ── AltiVec / VSX SIMD ────────────────────────────────────────────────────

    #[test]
    fn test_ppc_altivec_has_simd() {
        assert!(
            AltiVecSupport::AltiVec.has_simd(),
            "AltiVec must report SIMD"
        );
        assert!(AltiVecSupport::Vsx.has_simd(), "VSX must report SIMD");
        assert!(AltiVecSupport::Vsx3.has_simd(), "VSX3 must report SIMD");
    }

    #[test]
    fn test_ppc_no_altivec() {
        assert!(
            !AltiVecSupport::None.has_simd(),
            "No AltiVec must not report SIMD"
        );

        let caps = PowerPcCapabilities {
            altivec: AltiVecSupport::None,
            ..Default::default()
        };
        assert!(!caps.has_simd(), "has_simd() must be false without AltiVec");
    }

    // ── server-class detection ────────────────────────────────────────────────

    #[test]
    fn test_ppc_server_class() {
        // PowerPc64Le represents POWER8/9/10 — modern server class.
        let plat = PowerPcPlatform {
            variant: PowerPcVariant::PowerPc64Le,
            capabilities: PowerPcCapabilities {
                variant: Some(PowerPcVariant::PowerPc64Le),
                altivec: AltiVecSupport::Vsx3,
                has_fpu: true,
                has_crypto: true,
                has_htm: true,
                has_htm_no_suspend: false,
                has_spe: false,
            },
            cache: PowerPcCacheInfo::default(),
            endianness: PowerPcEndianness::LittleEndian,
        };
        assert!(
            plat.is_server_class(),
            "PowerPc64Le platform must be server-class"
        );
    }

    #[test]
    fn test_ppc_embedded_not_server() {
        let plat = PowerPcPlatform {
            variant: PowerPcVariant::Ppc440,
            capabilities: PowerPcCapabilities {
                variant: Some(PowerPcVariant::Ppc440),
                altivec: AltiVecSupport::None,
                has_fpu: true,
                has_spe: false,
                has_crypto: false,
                has_htm: false,
                has_htm_no_suspend: false,
            },
            cache: PowerPcCacheInfo {
                l1_icache_kb: 16,
                l1_dcache_kb: 16,
                l2_cache_kb: None,
                l3_cache_mb: None,
                cache_line_bytes: 32,
            },
            endianness: PowerPcEndianness::BigEndian,
        };
        assert!(!plat.is_server_class(), "Ppc440 must not be server-class");

        // E500 likewise
        assert!(!PowerPcVariant::E500.is_server_class());
    }

    // ── endianness ────────────────────────────────────────────────────────────

    #[test]
    fn test_ppc_big_endian() {
        assert!(
            PowerPcEndianness::BigEndian.is_big_endian(),
            "BigEndian must report big-endian"
        );
        // Classic PowerPc64 runs big-endian.
        let plat = PowerPcPlatform {
            variant: PowerPcVariant::PowerPc64,
            capabilities: PowerPcCapabilities {
                variant: Some(PowerPcVariant::PowerPc64),
                ..Default::default()
            },
            cache: PowerPcCacheInfo::default(),
            endianness: PowerPcEndianness::BigEndian,
        };
        assert!(plat.is_big_endian());
    }

    #[test]
    fn test_ppc64le_endianness() {
        assert!(
            !PowerPcEndianness::LittleEndian.is_big_endian(),
            "LittleEndian must not report big-endian"
        );
        let plat = PowerPcPlatform {
            variant: PowerPcVariant::PowerPc64Le,
            capabilities: PowerPcCapabilities::default(),
            cache: PowerPcCacheInfo::default(),
            endianness: PowerPcEndianness::LittleEndian,
        };
        assert!(!plat.is_big_endian());
    }

    // ── detect() sanity checks ────────────────────────────────────────────────

    #[test]
    fn test_ppc_detect_no_panic() {
        let platform = PowerPcPlatform::detect();
        // On any host (including x86_64) detect() must return non-zero cache sizes.
        assert!(platform.cache.l1_icache_kb > 0);
        assert!(platform.cache.l1_dcache_kb > 0);
        assert!(platform.cache.cache_line_bytes > 0);
    }

    #[test]
    fn test_ppc_altivec_ordering() {
        // Ordering matters for >= checks.
        assert!(AltiVecSupport::None < AltiVecSupport::AltiVec);
        assert!(AltiVecSupport::AltiVec < AltiVecSupport::Vsx);
        assert!(AltiVecSupport::Vsx < AltiVecSupport::Vsx3);
    }

    #[test]
    fn test_ppc_description_nonempty() {
        let platform = PowerPcPlatform::detect();
        let desc = platform.description();
        assert!(!desc.is_empty());
    }
}
