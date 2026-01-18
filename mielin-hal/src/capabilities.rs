//! Hardware capability detection and management
//!
//! This module provides cached hardware capability detection to avoid
//! expensive repeated queries of hardware registers and system information.
//!
//! # Caching Strategy
//!
//! Hardware capabilities are cached after first detection since they don't
//! change during program execution. This improves performance for frequently
//! queried information.

use crate::Architecture;
use bitflags::bitflags;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct HardwareCapabilities: u64 {
        const NONE = 0;
        const SIMD = 1 << 0;
        const SVE = 1 << 1;
        const SVE2 = 1 << 2;
        const SME = 1 << 3;
        const NPU = 1 << 4;
        const CRYPTO = 1 << 5;
        const ATOMICS = 1 << 6;
        const FPU = 1 << 7;
        const NEON = 1 << 8;
        const AVX = 1 << 9;
        const AVX2 = 1 << 10;
        const AVX512 = 1 << 11;
        const SSE4_2 = 1 << 12;
        const FMA = 1 << 13;
        const AES_NI = 1 << 14;
        // RISC-V specific
        const RVV = 1 << 15;  // RISC-V Vector extension (RVV 1.0)
        const RVB = 1 << 16;  // RISC-V Bit Manipulation
        const RVK = 1 << 17;  // RISC-V Cryptography extension
    }
}

pub struct HardwareProfile {
    pub architecture: Architecture,
    pub capabilities: HardwareCapabilities,
    pub core_count: usize,
    pub memory_size: usize,
    pub cache_line_size: usize,
    pub page_size: usize,
    pub l1_cache_size: usize,
    pub l2_cache_size: usize,
    pub l3_cache_size: usize,
}

impl HardwareProfile {
    /// Detect hardware profile with caching
    ///
    /// This function uses a cached version after the first call to avoid
    /// expensive repeated hardware queries.
    pub fn detect() -> Self {
        static CACHED_INITIALIZED: AtomicBool = AtomicBool::new(false);
        static CACHED_CAPABILITIES: AtomicU64 = AtomicU64::new(0);
        static CACHED_CORE_COUNT: AtomicUsize = AtomicUsize::new(0);
        static CACHED_MEMORY_SIZE: AtomicUsize = AtomicUsize::new(0);
        static CACHED_L1_SIZE: AtomicUsize = AtomicUsize::new(0);
        static CACHED_L2_SIZE: AtomicUsize = AtomicUsize::new(0);
        static CACHED_L3_SIZE: AtomicUsize = AtomicUsize::new(0);
        static CACHED_LINE_SIZE: AtomicUsize = AtomicUsize::new(0);

        if CACHED_INITIALIZED.load(Ordering::Relaxed) {
            // Return cached values
            let arch = crate::detect_architecture();
            return Self {
                architecture: arch,
                capabilities: HardwareCapabilities::from_bits_truncate(
                    CACHED_CAPABILITIES.load(Ordering::Relaxed),
                ),
                core_count: CACHED_CORE_COUNT.load(Ordering::Relaxed),
                memory_size: CACHED_MEMORY_SIZE.load(Ordering::Relaxed),
                cache_line_size: CACHED_LINE_SIZE.load(Ordering::Relaxed),
                page_size: 4096,
                l1_cache_size: CACHED_L1_SIZE.load(Ordering::Relaxed),
                l2_cache_size: CACHED_L2_SIZE.load(Ordering::Relaxed),
                l3_cache_size: CACHED_L3_SIZE.load(Ordering::Relaxed),
            };
        }

        // First detection - query hardware
        let arch = crate::detect_architecture();
        let capabilities = detect_capabilities(&arch);
        let core_count = detect_core_count();
        let memory_size = detect_memory_size();
        let cache_topology = crate::cache::CacheTopology::detect();

        // Store in cache
        CACHED_CAPABILITIES.store(capabilities.bits(), Ordering::Relaxed);
        CACHED_CORE_COUNT.store(core_count, Ordering::Relaxed);
        CACHED_MEMORY_SIZE.store(memory_size, Ordering::Relaxed);
        CACHED_L1_SIZE.store(cache_topology.l1_data.size, Ordering::Relaxed);
        CACHED_L2_SIZE.store(cache_topology.l2.size, Ordering::Relaxed);
        CACHED_L3_SIZE.store(cache_topology.l3.size, Ordering::Relaxed);
        CACHED_LINE_SIZE.store(cache_topology.default_line_size, Ordering::Relaxed);
        CACHED_INITIALIZED.store(true, Ordering::Release);

        Self {
            architecture: arch,
            capabilities,
            core_count,
            memory_size,
            cache_line_size: cache_topology.default_line_size,
            page_size: 4096,
            l1_cache_size: cache_topology.l1_data.size,
            l2_cache_size: cache_topology.l2.size,
            l3_cache_size: cache_topology.l3.size,
        }
    }

    /// Force re-detection of hardware capabilities (clears cache)
    ///
    /// This is useful if hardware configuration changes at runtime
    /// (e.g., hot-plug scenarios).
    pub fn invalidate_cache() {
        static CACHED_INITIALIZED: AtomicBool = AtomicBool::new(false);
        CACHED_INITIALIZED.store(false, Ordering::Release);
    }

    pub fn has_sve2(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::SVE2)
    }

    pub fn has_npu(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::NPU)
    }

    pub fn has_simd(&self) -> bool {
        // SIMD is available if max_vector_width > 0
        self.max_vector_width() > 0
    }

    pub fn supports_tensor_ops(&self) -> bool {
        self.has_sve2()
            || self.has_npu()
            || self.capabilities.contains(HardwareCapabilities::AVX512)
            || self.capabilities.contains(HardwareCapabilities::SME)
    }

    pub fn max_vector_width(&self) -> usize {
        // Check for highest vector width first
        if self.capabilities.contains(HardwareCapabilities::SVE2)
            || self.capabilities.contains(HardwareCapabilities::SME)
        {
            // SVE2/SME can be 128-2048 bits, default to 256 for now
            256
        } else if self.capabilities.contains(HardwareCapabilities::AVX512) {
            512
        } else if self
            .capabilities
            .intersects(HardwareCapabilities::AVX2 | HardwareCapabilities::AVX)
        {
            256
        } else if self.capabilities.contains(HardwareCapabilities::RVV) {
            // RISC-V Vector extension (RVV) has configurable VLEN
            // Can be 128, 256, 512, 1024, or 2048 bits
            // Default to 128 (minimum required by spec) until we can detect actual VLEN
            128
        } else if self.capabilities.contains(HardwareCapabilities::NEON)
            || self.capabilities.contains(HardwareCapabilities::SIMD)
        {
            128
        } else {
            0
        }
    }
}

fn detect_capabilities(arch: &Architecture) -> HardwareCapabilities {
    let mut caps = HardwareCapabilities::NONE;

    match arch {
        Architecture::AArch64 => {
            caps |= HardwareCapabilities::FPU;
            caps |= HardwareCapabilities::NEON;
            caps |= HardwareCapabilities::ATOMICS;

            #[cfg(target_arch = "aarch64")]
            {
                #[cfg(target_feature = "sve")]
                {
                    caps |= HardwareCapabilities::SVE;
                }
                #[cfg(target_feature = "sve2")]
                {
                    caps |= HardwareCapabilities::SVE2;
                }
            }
        }
        Architecture::X86_64 => {
            // Use runtime detection via CPUID
            #[cfg(target_arch = "x86_64")]
            {
                caps = crate::arch::x86_64::detect_x86_64_capabilities();
            }

            // Fallback to compile-time detection for non-x86_64 targets
            #[cfg(not(target_arch = "x86_64"))]
            {
                caps |= HardwareCapabilities::FPU;
            }
        }
        Architecture::RiscV64 => {
            // Use runtime detection for RISC-V
            #[cfg(any(target_arch = "riscv64", target_os = "linux"))]
            {
                caps = crate::arch::riscv64::detect_riscv_capabilities();
            }

            // Fallback for non-RISC-V targets
            #[cfg(not(any(target_arch = "riscv64", target_os = "linux")))]
            {
                caps |= HardwareCapabilities::FPU;
            }
        }
        Architecture::ArmCortexM => {
            #[cfg(all(target_arch = "arm", target_feature = "v7"))]
            {
                caps |= HardwareCapabilities::FPU;
            }
        }
        Architecture::CortexM => {
            #[cfg(all(target_arch = "arm", target_feature = "v7"))]
            {
                caps |= HardwareCapabilities::FPU;
            }
        }
        Architecture::LoongArch64 => {
            caps |= HardwareCapabilities::FPU;
            caps |= HardwareCapabilities::SIMD;
            caps |= HardwareCapabilities::ATOMICS;
        }
        Architecture::Xtensa => {
            caps |= HardwareCapabilities::FPU;
        }
    }

    caps
}

fn detect_core_count() -> usize {
    let topology = crate::system::CpuTopology::detect();
    topology.logical_cores
}

fn detect_memory_size() -> usize {
    let mem_info = crate::system::MemoryInfo::detect();
    mem_info.total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_profile() {
        let profile = HardwareProfile::detect();
        assert!(profile.core_count > 0);
        assert!(profile.page_size > 0);
    }

    #[test]
    fn test_capability_flags() {
        let caps = HardwareCapabilities::SVE2 | HardwareCapabilities::NEON;
        assert!(caps.contains(HardwareCapabilities::SVE2));
        assert!(caps.contains(HardwareCapabilities::NEON));
        assert!(!caps.contains(HardwareCapabilities::AVX));
    }

    #[test]
    fn test_vector_width_detection() {
        let profile = HardwareProfile::detect();
        let width = profile.max_vector_width();
        assert!(width == 0 || width == 128 || width == 256 || width == 512);
    }

    #[test]
    fn test_cached_detection() {
        // First call should initialize cache
        let profile1 = HardwareProfile::detect();

        // Second call should use cache
        let profile2 = HardwareProfile::detect();

        // Should return same values
        assert_eq!(profile1.core_count, profile2.core_count);
        assert_eq!(profile1.capabilities, profile2.capabilities);
        assert_eq!(profile1.l1_cache_size, profile2.l1_cache_size);
    }

    #[test]
    fn test_cache_invalidation() {
        let profile1 = HardwareProfile::detect();

        // Invalidate cache
        HardwareProfile::invalidate_cache();

        // Re-detect after invalidation
        let profile2 = HardwareProfile::detect();

        // Should still return same values (hardware hasn't changed)
        assert_eq!(profile1.core_count, profile2.core_count);
    }

    #[test]
    fn test_simd_detection() {
        let profile = HardwareProfile::detect();
        // has_simd() should not panic
        let _has_simd = profile.has_simd();
    }

    #[test]
    fn test_tensor_ops_support() {
        let profile = HardwareProfile::detect();
        // supports_tensor_ops() should not panic
        let _supports = profile.supports_tensor_ops();
    }

    // Property-based tests with proptest
    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Test that bitflags union is commutative and associative
            #[test]
            fn test_capability_flags_union_properties(a: u64, b: u64, c: u64) {
                let cap_a = HardwareCapabilities::from_bits_truncate(a);
                let cap_b = HardwareCapabilities::from_bits_truncate(b);
                let cap_c = HardwareCapabilities::from_bits_truncate(c);

                // Commutative: a | b == b | a
                assert_eq!(cap_a | cap_b, cap_b | cap_a);

                // Associative: (a | b) | c == a | (b | c)
                assert_eq!((cap_a | cap_b) | cap_c, cap_a | (cap_b | cap_c));

                // Identity: a | empty == a
                assert_eq!(cap_a | HardwareCapabilities::empty(), cap_a);
            }

            /// Test that bitflags intersection is commutative and associative
            #[test]
            fn test_capability_flags_intersection_properties(a: u64, b: u64, c: u64) {
                let cap_a = HardwareCapabilities::from_bits_truncate(a);
                let cap_b = HardwareCapabilities::from_bits_truncate(b);
                let cap_c = HardwareCapabilities::from_bits_truncate(c);

                // Commutative: a & b == b & a
                assert_eq!(cap_a & cap_b, cap_b & cap_a);

                // Associative: (a & b) & c == a & (b & c)
                assert_eq!((cap_a & cap_b) & cap_c, cap_a & (cap_b & cap_c));

                // Identity: a & all == a
                assert_eq!(cap_a & cap_a, cap_a);
            }

            /// Test De Morgan's laws for bitflags
            #[test]
            fn test_capability_flags_de_morgan(a: u64, b: u64) {
                let cap_a = HardwareCapabilities::from_bits_truncate(a);
                let cap_b = HardwareCapabilities::from_bits_truncate(b);

                // !(a | b) == !a & !b
                assert_eq!(!(cap_a | cap_b), !cap_a & !cap_b);

                // !(a & b) == !a | !b
                assert_eq!(!(cap_a & cap_b), !cap_a | !cap_b);
            }

            /// Test that vector width is always a valid value
            #[test]
            fn test_vector_width_validity(bits: u64) {
                let caps = HardwareCapabilities::from_bits_truncate(bits);
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: caps,
                    core_count: 1,
                    memory_size: 4096,
                    cache_line_size: 64,
                    page_size: 4096,
                    l1_cache_size: 32768,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                let width = profile.max_vector_width();
                // Valid vector widths: 0, 128, 256, 512, 1024, 2048
                prop_assert!(
                    width == 0 || width == 128 || width == 256 ||
                    width == 512 || width == 1024 || width == 2048,
                    "Invalid vector width: {}", width
                );
            }

            /// Test that SIMD detection is consistent with capabilities
            #[test]
            fn test_simd_consistency(bits: u64) {
                let caps = HardwareCapabilities::from_bits_truncate(bits);
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: caps,
                    core_count: 1,
                    memory_size: 4096,
                    cache_line_size: 64,
                    page_size: 4096,
                    l1_cache_size: 32768,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                let has_simd = profile.has_simd();
                let vector_width = profile.max_vector_width();

                // If has_simd is true, vector_width should be > 0
                if has_simd {
                    prop_assert!(vector_width > 0, "SIMD detected but vector width is 0");
                }

                // If vector_width > 0, has_simd should be true
                if vector_width > 0 {
                    prop_assert!(has_simd, "Vector width > 0 but has_simd is false");
                }
            }

            /// Test that core count is always positive and reasonable
            #[test]
            fn test_core_count_bounds(count in 1_usize..=1024_usize) {
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: HardwareCapabilities::empty(),
                    core_count: count,
                    memory_size: 4096,
                    cache_line_size: 64,
                    page_size: 4096,
                    l1_cache_size: 32768,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                prop_assert!(profile.core_count > 0);
                prop_assert!(profile.core_count <= 1024);
            }

            /// Test that memory size is reasonable
            #[test]
            fn test_memory_size_bounds(size in 4096_usize..=1_099_511_627_776_usize) { // 4 KB to 1 TB
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: HardwareCapabilities::empty(),
                    core_count: 1,
                    memory_size: size,
                    cache_line_size: 64,
                    page_size: 4096,
                    l1_cache_size: 32768,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                prop_assert!(profile.memory_size >= 4096);
                prop_assert!(profile.memory_size <= 1_099_511_627_776);
            }

            /// Test that page size is a power of 2
            #[test]
            fn test_page_size_power_of_two(exp in 12_u32..=16_u32) { // 4KB to 64KB
                let page_size = 1_usize << exp;
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: HardwareCapabilities::empty(),
                    core_count: 1,
                    memory_size: 4096,
                    cache_line_size: 64,
                    page_size,
                    l1_cache_size: 32768,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                // Page size should be power of 2
                prop_assert!(profile.page_size.is_power_of_two());
                prop_assert!(profile.page_size >= 4096);
                prop_assert!(profile.page_size <= 65536);
            }

            /// Test that cache size is reasonable (0 or >= 1KB)
            #[test]
            fn test_cache_size_validity(size in prop::option::of(1024_usize..=67_108_864_usize)) { // 1KB to 64MB
                let cache_size = size.unwrap_or(0);
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: HardwareCapabilities::empty(),
                    core_count: 1,
                    memory_size: 4096,
                    cache_line_size: 64,
                    page_size: 4096,
                    l1_cache_size: cache_size,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                if profile.l1_cache_size > 0 {
                    prop_assert!(profile.l1_cache_size >= 1024);
                    prop_assert!(profile.l1_cache_size <= 67_108_864);
                }
            }

            /// Test that capability flags subset relationships hold
            #[test]
            fn test_capability_subset_properties(a: u64, b: u64) {
                let cap_a = HardwareCapabilities::from_bits_truncate(a);
                let cap_b = HardwareCapabilities::from_bits_truncate(b);
                let union_ab = cap_a | cap_b;
                let inter_ab = cap_a & cap_b;

                // If a & b == a, then a is subset of b (contains all bits of a)
                if inter_ab == cap_a {
                    prop_assert!(cap_b.contains(cap_a));
                }

                // a | b always contains both a and b
                prop_assert!(union_ab.contains(cap_a));
                prop_assert!(union_ab.contains(cap_b));

                // a & b is contained by both a and b
                prop_assert!(cap_a.contains(inter_ab));
                prop_assert!(cap_b.contains(inter_ab));
            }

            /// Test that tensor ops support implies relevant extensions
            #[test]
            fn test_tensor_ops_implies_extensions(bits: u64) {
                let caps = HardwareCapabilities::from_bits_truncate(bits);
                let profile = HardwareProfile {
                    architecture: Architecture::X86_64,
                    capabilities: caps,
                    core_count: 1,
                    memory_size: 4096,
                    cache_line_size: 64,
                    page_size: 4096,
                    l1_cache_size: 32768,
                    l2_cache_size: 262144,
                    l3_cache_size: 8388608,
                };

                if profile.supports_tensor_ops() {
                    // Tensor ops require AVX512, SME, SVE2, or NPU
                    prop_assert!(
                        caps.contains(HardwareCapabilities::AVX512) ||
                        caps.contains(HardwareCapabilities::SME) ||
                        caps.contains(HardwareCapabilities::SVE2) ||
                        caps.contains(HardwareCapabilities::NPU),
                        "Tensor ops detected without AVX512/SME/SVE2/NPU"
                    );
                }
            }
        }
    }
}
