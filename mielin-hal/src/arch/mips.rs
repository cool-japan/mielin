//! MIPS Architecture Support
//!
//! This module provides MIPS hardware abstraction and capability detection for both
//! 32-bit and 64-bit MIPS variants, covering classic MIPS32/64 as well as the
//! Release 6 ISA reforms and the microMIPS compressed encoding.
//!
//! ## Supported Variants
//!
//! - **MIPS32 Release 2** (Mips32R2): Most common embedded variant (MIPS24K, MIPS34K, etc.)
//! - **MIPS32 Release 6** (Mips32R6): Compact branches, removed delay slots, mandatory FPU
//! - **MIPS64 Release 2** (Mips64R2): 64-bit with older MIPS ISA rules
//! - **MIPS64 Release 6** (Mips64R6): 64-bit with Release 6 ISA reforms
//! - **microMIPS** (MicroMips): Variable-length compressed ISA (similar to ARM Thumb2)
//!
//! ## Hardware Extensions
//!
//! - **FPU**: IEEE 754 floating-point unit (32/64-bit)
//! - **DSP**: MIPS DSP Application Specific Extension (rev1/rev2)
//! - **MSA**: MIPS SIMD Architecture — 128-bit SIMD with 32 × 128-bit registers
//! - **MIPS16e**: 16-bit compressed instruction encoding (pre-microMIPS)
//! - **VZ**: Virtualization Extension (hardware-assisted guest/host isolation)
//!
//! ## Cache Architecture
//!
//! MIPS uses a Harvard cache architecture with separate instruction and data caches.
//! Cache sizes are encoded in the Config1 CP0 register:
//!
//! ```text
//! Config1[IS, IL, IA] → icache: sets/ways/line-size
//! Config1[DS, DL, DA] → dcache: sets/ways/line-size
//! ```
//!
//! ## Endianness
//!
//! MIPS supports both big-endian (classic MIPS) and little-endian (MIPSEL) variants.
//! Linux on MIPS typically uses little-endian. The hardware endianness is baked in
//! at core configuration time and cannot be changed at runtime.
//!
//! ## Detection Strategy
//!
//! On real MIPS hardware the Config/Config1 CP0 registers (register 16, select 0/1)
//! expose variant and cache information. On non-MIPS hosts (cross-compilation) safe
//! defaults matching a MIPS32R2 with FPU and 32 KB caches are returned.

extern crate alloc;

use alloc::string::String;

// ─────────────────────────────────────────────────────────────────────────────
// Variant
// ─────────────────────────────────────────────────────────────────────────────

/// MIPS processor variant.
///
/// Covers the main MIPS ISA generations used in production silicon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MipsVariant {
    /// MIPS32 Release 2 — most common embedded variant (e.g. MIPS 24K, 34K, interAptiv).
    Mips32R2,
    /// MIPS32 Release 6 — compact branches, no architectural delay slots, mandatory FPU.
    Mips32R6,
    /// MIPS64 Release 2 — 64-bit with the older MIPS ISA rules.
    Mips64R2,
    /// MIPS64 Release 6 — 64-bit with Release 6 ISA reforms.
    Mips64R6,
    /// microMIPS — variable-length compressed ISA reducing code size ~25%.
    MicroMips,
}

impl MipsVariant {
    /// Returns `true` for 64-bit variants (Mips64R2, Mips64R6).
    #[inline]
    pub fn is_64bit(self) -> bool {
        matches!(self, MipsVariant::Mips64R2 | MipsVariant::Mips64R6)
    }

    /// Human-readable short identifier for the variant.
    pub fn as_str(self) -> &'static str {
        match self {
            MipsVariant::Mips32R2 => "mips32r2",
            MipsVariant::Mips32R6 => "mips32r6",
            MipsVariant::Mips64R2 => "mips64r2",
            MipsVariant::Mips64R6 => "mips64r6",
            MipsVariant::MicroMips => "micromips",
        }
    }
}

impl core::fmt::Display for MipsVariant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Endianness
// ─────────────────────────────────────────────────────────────────────────────

/// MIPS endianness configuration.
///
/// The endianness is fixed at core synthesis time. This value describes how
/// the running binary was compiled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MipsEndianness {
    /// Classic MIPS byte order (network byte order).
    BigEndian,
    /// MIPSEL (little-endian) byte order — typical for Linux/OpenWrt on MIPS.
    LittleEndian,
}

impl MipsEndianness {
    /// Detect the native endianness of the current binary.
    ///
    /// Determined at compile time by `target_endian`. On non-MIPS hosts defaults
    /// to `LittleEndian` (most common MIPSEL deployment on Linux).
    #[inline]
    pub fn detect() -> Self {
        #[cfg(target_endian = "big")]
        return MipsEndianness::BigEndian;

        #[cfg(not(target_endian = "big"))]
        MipsEndianness::LittleEndian
    }

    /// Returns `true` when this configuration is big-endian.
    #[inline]
    pub fn is_big_endian(self) -> bool {
        self == MipsEndianness::BigEndian
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Capabilities
// ─────────────────────────────────────────────────────────────────────────────

/// Hardware capabilities for a MIPS processor.
///
/// Populated by reading CP0 Config/Config1/Config3 registers on a real MIPS
/// core, or by returning safe compiled-in defaults on any other host.
#[derive(Debug, Clone)]
pub struct MipsCapabilities {
    /// IEEE 754 floating-point unit present.
    pub has_fpu: bool,
    /// DSP Application Specific Extension (rev1 or rev2).
    pub has_dsp: bool,
    /// MIPS SIMD Architecture — 128-bit SIMD (32 × 128-bit registers).
    pub has_msa: bool,
    /// MIPS16e compressed 16-bit instruction encoding.
    pub has_mips16e: bool,
    /// VZ (Virtualization) Extension.
    pub has_virt: bool,
    /// Identified variant, if determinable.
    pub variant: Option<MipsVariant>,
}

impl Default for MipsCapabilities {
    fn default() -> Self {
        MipsCapabilities {
            has_fpu: true,
            has_dsp: false,
            has_msa: false,
            has_mips16e: false,
            has_virt: false,
            variant: Some(MipsVariant::Mips32R2),
        }
    }
}

impl MipsCapabilities {
    /// Detect capabilities from hardware registers.
    ///
    /// On a native MIPS host the CP0 Config/Config1/Config3 registers are read
    /// via inline assembly. On all other hosts safe default values are returned
    /// immediately without executing any MIPS-specific instructions.
    pub fn detect() -> Self {
        #[cfg(target_arch = "mips")]
        {
            return Self::detect_native_mips32();
        }

        #[cfg(target_arch = "mips64")]
        {
            return Self::detect_native_mips64();
        }

        // Non-MIPS host — return safe defaults matching a MIPS32R2 with FPU.
        #[allow(unreachable_code)]
        Self::default()
    }

    /// Returns `true` for 64-bit variants (Mips64R2 or Mips64R6).
    #[inline]
    pub fn is_64bit(&self) -> bool {
        self.variant.map(|v| v.is_64bit()).unwrap_or(false)
    }

    /// Returns `true` when MSA (MIPS SIMD Architecture) is available.
    #[inline]
    pub fn has_simd(&self) -> bool {
        self.has_msa
    }

    /// Native MIPS32 capability detection via CP0 register reads.
    ///
    /// Reads Config0 (AT field for arch type), Config1 (FPU present flag),
    /// and Config3 (DSP, MSA, VZ flags).
    #[cfg(target_arch = "mips")]
    fn detect_native_mips32() -> Self {
        let mut caps = Self::default();

        // CP0 Config register (register 16, select 0).
        // AT field [14:13] encodes architecture type: 0=MIPS32, 1/2=MIPS64.
        let config0: u32;
        unsafe {
            core::arch::asm!(
                "mfc0 {0}, $16, 0",
                out(reg) config0,
                options(nomem, nostack, preserves_flags),
            );
        }

        // CP0 Config1 register (register 16, select 1) — FPU and cache fields.
        let config1: u32;
        unsafe {
            core::arch::asm!(
                "mfc0 {0}, $16, 1",
                out(reg) config1,
                options(nomem, nostack, preserves_flags),
            );
        }

        // CP0 Config3 register (register 16, select 3) — extended feature flags.
        let config3: u32;
        unsafe {
            core::arch::asm!(
                "mfc0 {0}, $16, 3",
                out(reg) config3,
                options(nomem, nostack, preserves_flags),
            );
        }

        // Config1[0] = FPU present.
        caps.has_fpu = (config1 & 0x1) != 0;
        // Config3[10] = DSP ASE present.
        caps.has_dsp = (config3 & (1 << 10)) != 0;
        // Config3[28] = MSA present.
        caps.has_msa = (config3 & (1 << 28)) != 0;
        // Config3[14] = VZ (virtualization) present.
        caps.has_virt = (config3 & (1 << 14)) != 0;

        // Derive variant from Config0 AT field [14:13].
        let at = (config0 >> 13) & 0x3;
        caps.variant = Some(match at {
            0 => MipsVariant::Mips32R2,
            1 | 2 => MipsVariant::Mips64R2,
            _ => MipsVariant::Mips32R2,
        });

        caps
    }

    /// Native MIPS64 capability detection via CP0 register reads.
    #[cfg(target_arch = "mips64")]
    fn detect_native_mips64() -> Self {
        let mut caps = Self {
            variant: Some(MipsVariant::Mips64R2),
            ..Self::default()
        };

        let config1: u64;
        let config3: u64;
        unsafe {
            core::arch::asm!(
                "dmfc0 {0}, $16, 1",
                out(reg) config1,
                options(nomem, nostack, preserves_flags),
            );
            core::arch::asm!(
                "dmfc0 {0}, $16, 3",
                out(reg) config3,
                options(nomem, nostack, preserves_flags),
            );
        }

        caps.has_fpu = (config1 & 0x1) != 0;
        caps.has_dsp = (config3 & (1 << 10)) != 0;
        caps.has_msa = (config3 & (1 << 28)) != 0;
        caps.has_virt = (config3 & (1 << 14)) != 0;

        caps
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cache Info
// ─────────────────────────────────────────────────────────────────────────────

/// MIPS Harvard L1 cache topology.
///
/// MIPS caches are physically indexed, physically tagged (PIPT) or virtually
/// indexed, physically tagged (VIPT). Cache parameters are encoded in the
/// CP0 Config1 register and can also be read from the Device Tree.
#[derive(Debug, Clone)]
pub struct MipsCacheInfo {
    /// Instruction cache size in KiB.
    pub icache_size_kb: u32,
    /// Data cache size in KiB.
    pub dcache_size_kb: u32,
    /// Instruction cache associativity (number of ways).
    pub icache_ways: u32,
    /// Data cache associativity (number of ways).
    pub dcache_ways: u32,
    /// Cache line size in bytes (typically 32 on MIPS32/64).
    pub line_size_bytes: u32,
}

impl Default for MipsCacheInfo {
    /// Defaults match a mid-range MIPS32R2 embedded core (MIPS 24Kc class).
    fn default() -> Self {
        MipsCacheInfo {
            icache_size_kb: 32,
            dcache_size_kb: 32,
            icache_ways: 4,
            dcache_ways: 4,
            line_size_bytes: 32,
        }
    }
}

impl MipsCacheInfo {
    /// Detect cache parameters from CP0 Config1 or return safe defaults.
    ///
    /// # CP0 Config1 Cache Encoding
    ///
    /// ```text
    /// IS[24:22]  icache sets per way = 64 << IS
    /// IL[21:19]  icache line size    = 2 << IL  bytes  (IL=0 → no icache)
    /// IA[18:16]  icache associativity = IA + 1
    /// DS[13:11]  dcache sets per way = 64 << DS
    /// DL[10: 8]  dcache line size    = 2 << DL  bytes  (DL=0 → no dcache)
    /// DA[ 7: 5]  dcache associativity = DA + 1
    /// ```
    pub fn detect() -> Self {
        #[cfg(any(target_arch = "mips", target_arch = "mips64"))]
        {
            return Self::detect_native();
        }

        #[allow(unreachable_code)]
        Self::default()
    }

    #[cfg(any(target_arch = "mips", target_arch = "mips64"))]
    fn detect_native() -> Self {
        let config1: u32;
        unsafe {
            core::arch::asm!(
                "mfc0 {0}, $16, 1",
                out(reg) config1,
                options(nomem, nostack, preserves_flags),
            );
        }

        let i_s = ((config1 >> 22) & 0x7) as u32; // sets-per-way exponent
        let i_l = ((config1 >> 19) & 0x7) as u32; // line-size exponent
        let i_a = ((config1 >> 16) & 0x7) as u32; // ways − 1

        let d_s = ((config1 >> 11) & 0x7) as u32;
        let d_l = ((config1 >> 8) & 0x7) as u32;
        let d_a = ((config1 >> 5) & 0x7) as u32;

        // Line size: IL=0 means no cache; otherwise 2 << IL bytes.
        let i_line = if i_l == 0 { 32 } else { 2u32 << i_l };
        let d_line = if d_l == 0 { 32 } else { 2u32 << d_l };

        let i_sets = if i_l == 0 { 0 } else { 64u32 << i_s };
        let i_ways = i_a + 1;
        let icache_bytes = i_sets.saturating_mul(i_ways).saturating_mul(i_line);
        let icache_kb = (icache_bytes / 1024).max(1);

        let d_sets = if d_l == 0 { 0 } else { 64u32 << d_s };
        let d_ways = d_a + 1;
        let dcache_bytes = d_sets.saturating_mul(d_ways).saturating_mul(d_line);
        let dcache_kb = (dcache_bytes / 1024).max(1);

        MipsCacheInfo {
            icache_size_kb: icache_kb,
            dcache_size_kb: dcache_kb,
            icache_ways: i_ways,
            dcache_ways: d_ways,
            line_size_bytes: i_line,
        }
    }

    /// Number of icache lines: `icache_size_kb × 1024 / line_size_bytes`.
    ///
    /// Returns `0` if `line_size_bytes` is zero (no division by zero).
    #[inline]
    pub fn icache_lines(&self) -> u32 {
        if self.line_size_bytes == 0 {
            return 0;
        }
        self.icache_size_kb
            .saturating_mul(1024)
            .checked_div(self.line_size_bytes)
            .unwrap_or(0)
    }

    /// Number of dcache lines: `dcache_size_kb × 1024 / line_size_bytes`.
    ///
    /// Returns `0` if `line_size_bytes` is zero (no division by zero).
    #[inline]
    pub fn dcache_lines(&self) -> u32 {
        if self.line_size_bytes == 0 {
            return 0;
        }
        self.dcache_size_kb
            .saturating_mul(1024)
            .checked_div(self.line_size_bytes)
            .unwrap_or(0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform
// ─────────────────────────────────────────────────────────────────────────────

/// Complete MIPS platform descriptor.
///
/// Aggregates variant identification, hardware capabilities, cache topology, and
/// endianness into a single top-level struct that callers can query without
/// needing to know which CP0 registers to read.
pub struct MipsPlatform {
    /// The ISA variant this processor implements.
    pub variant: MipsVariant,
    /// Full capability set (FPU, DSP, MSA, etc.).
    pub capabilities: MipsCapabilities,
    /// L1 cache topology (Harvard architecture).
    pub cache: MipsCacheInfo,
    /// Byte order for data accesses.
    pub endianness: MipsEndianness,
}

impl MipsPlatform {
    /// Detect the full MIPS platform description.
    ///
    /// Aggregates the results of `MipsCapabilities::detect()`,
    /// `MipsCacheInfo::detect()`, and `MipsEndianness::detect()`.
    ///
    /// Never panics — on non-MIPS hosts all fields return safe defaults.
    pub fn detect() -> Self {
        let capabilities = MipsCapabilities::detect();
        let cache = MipsCacheInfo::detect();
        let endianness = MipsEndianness::detect();
        let variant = capabilities.variant.unwrap_or(MipsVariant::Mips32R2);

        MipsPlatform {
            variant,
            capabilities,
            cache,
            endianness,
        }
    }

    /// Returns `true` when the platform is configured big-endian.
    #[inline]
    pub fn is_big_endian(&self) -> bool {
        self.endianness.is_big_endian()
    }

    /// Compact human-readable description of this platform.
    pub fn description(&self) -> String {
        use alloc::format;
        format!(
            "{} {} {}  icache={}KB dcache={}KB line={}B",
            if self.endianness.is_big_endian() {
                "mips"
            } else {
                "mipsel"
            },
            self.variant,
            if self.capabilities.is_64bit() {
                "64-bit"
            } else {
                "32-bit"
            },
            self.cache.icache_size_kb,
            self.cache.dcache_size_kb,
            self.cache.line_size_bytes,
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
    fn test_mips_variant_is_32bit() {
        assert!(
            !MipsVariant::Mips32R2.is_64bit(),
            "Mips32R2 must not be 64-bit"
        );
        assert!(
            !MipsVariant::Mips32R6.is_64bit(),
            "Mips32R6 must not be 64-bit"
        );
        assert!(
            !MipsVariant::MicroMips.is_64bit(),
            "MicroMips must not be 64-bit"
        );
    }

    #[test]
    fn test_mips_variant_is_64bit() {
        assert!(MipsVariant::Mips64R2.is_64bit(), "Mips64R2 must be 64-bit");
        assert!(MipsVariant::Mips64R6.is_64bit(), "Mips64R6 must be 64-bit");
    }

    // ── platform detection must not panic ─────────────────────────────────────

    #[test]
    fn test_mips_detect_returns_platform() {
        // Must complete without panic on any host (including x86_64 CI).
        let platform = MipsPlatform::detect();
        // variant is always populated (fallback = Mips32R2)
        let _ = platform.variant.as_str();
        // cache sizes must be non-zero
        assert!(platform.cache.icache_size_kb > 0);
        assert!(platform.cache.dcache_size_kb > 0);
        assert!(platform.cache.line_size_bytes > 0);
    }

    // ── endianness ────────────────────────────────────────────────────────────

    #[test]
    fn test_mips_big_endian() {
        assert!(MipsEndianness::BigEndian.is_big_endian());
    }

    #[test]
    fn test_mips_le() {
        assert!(!MipsEndianness::LittleEndian.is_big_endian());
    }

    // ── SIMD via MSA ─────────────────────────────────────────────────────────

    #[test]
    fn test_mips_has_simd_with_msa() {
        let caps = MipsCapabilities {
            has_msa: true,
            ..Default::default()
        };
        assert!(
            caps.has_simd(),
            "has_simd() must be true when has_msa is set"
        );
    }

    // ── cache line counting ───────────────────────────────────────────────────

    #[test]
    fn test_mips_cache_lines() {
        let info = MipsCacheInfo {
            icache_size_kb: 32,
            dcache_size_kb: 32,
            icache_ways: 4,
            dcache_ways: 4,
            line_size_bytes: 32,
        };
        // 32 KiB / 32 B = 1024 lines
        assert_eq!(info.icache_lines(), 1024);
        assert_eq!(info.dcache_lines(), 1024);
    }

    // ── detect() returns reasonable defaults ─────────────────────────────────

    #[test]
    fn test_mips_detect_cache_info() {
        let info = MipsCacheInfo::detect();
        // Minimum sane values for any reasonable MIPS core.
        assert!(info.icache_size_kb >= 1, "icache must be at least 1 KB");
        assert!(info.dcache_size_kb >= 1, "dcache must be at least 1 KB");
        assert!(
            info.line_size_bytes >= 16,
            "cache line must be at least 16 bytes"
        );
        assert!(info.icache_ways >= 1, "icache must have at least 1 way");
    }
}
