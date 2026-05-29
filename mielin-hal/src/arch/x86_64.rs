//! x86_64 Architecture Support
//!
//! This module provides x86_64-specific hardware abstraction and capability detection.
//!
//! ## Supported Processors
//!
//! - **Intel**: Core (6th gen+), Xeon Scalable, Atom
//! - **AMD**: Ryzen, EPYC, Threadripper
//!
//! ## Hardware Capabilities
//!
//! ### SIMD Extensions
//!
//! - **SSE/SSE2**: 128-bit SIMD (universal on x86_64)
//! - **AVX**: 256-bit SIMD (Sandy Bridge+, Bulldozer+)
//! - **AVX2**: Enhanced 256-bit SIMD with FMA (Haswell+, Excavator+)
//! - **AVX-512**: 512-bit SIMD (Skylake-X+, Zen 4+)
//!   - AVX-512F (Foundation)
//!   - AVX-512CD (Conflict Detection)
//!   - AVX-512BW (Byte/Word)
//!   - AVX-512DQ (Doubleword/Quadword)
//!   - AVX-512VL (Vector Length Extensions)
//!
//! ### CPU Features
//!
//! - **AES-NI**: Hardware AES encryption
//! - **RDRAND**: Hardware random number generator
//! - **TSC**: Time Stamp Counter for high-precision timing
//! - **FMA**: Fused Multiply-Add instructions
//! - **BMI1/BMI2**: Bit Manipulation Instructions
//!
//! ### Detection Method
//!
//! Capabilities are detected via the CPUID instruction:
//! ```text
//! CPUID EAX=0: Get maximum supported function
//! CPUID EAX=1: Get processor info and feature bits
//! CPUID EAX=7: Extended features
//! CPUID EAX=0x80000001: Extended processor info
//! ```
//!
//! ## Usage Examples
//!
//! ### Basic Capability Detection
//!
//! ```no_run
//! use mielin_hal::capabilities;
//!
//! let caps = capabilities::HardwareProfile::detect();
//!
//! if caps.capabilities.contains(capabilities::HardwareCapabilities::AVX2) {
//!     println!("AVX2 is available");
//! }
//!
//! if caps.capabilities.contains(capabilities::HardwareCapabilities::AVX512) {
//!     println!("AVX-512 Foundation available");
//!     println!("Vector width: {} bits", caps.max_vector_width());
//! }
//! ```
//!
//! ### Cache Optimization
//!
//! ```no_run
//! use mielin_hal::cache::CacheTopology;
//!
//! let cache = CacheTopology::detect();
//!
//! // Optimize loop tiling based on cache size
//! let l1_size = cache.l1_data.size;
//! let l2_size = cache.l2.size;
//! let block_size = cache.optimal_block_size::<f32>();
//!
//! println!("L1: {} KB, L2: {} KB", l1_size / 1024, l2_size / 1024);
//! println!("Optimal block size: {}", block_size);
//! ```
//!
//! ### Power Management
//!
//! ```no_run
//! use mielin_hal::power;
//!
//! let power = power::detect_power_info();
//!
//! println!("CPU Frequency: {} - {} MHz",
//!     power.frequency.min_mhz, power.frequency.max_mhz);
//!
//! if power.hwp_supported {
//!     println!("Hardware P-States (HWP) supported");
//! }
//!
//! if power.turbo_supported {
//!     println!("Turbo Boost available");
//! }
//! ```
//!
//! ## Performance Considerations
//!
//! ### SIMD Selection
//!
//! Choose the appropriate SIMD level for your workload:
//! - **SSE2**: Universal compatibility, 128-bit vectors
//! - **AVX2**: Best for most modern CPUs, 256-bit vectors, FMA
//! - **AVX-512**: Maximum performance on supported CPUs, 512-bit vectors
//!   - Note: May reduce CPU frequency on some Intel CPUs
//!
//! ### Cache-Aware Programming
//!
//! Optimize data access patterns based on cache hierarchy:
//! ```text
//! L1: 32-48 KB per core (~4 cycles)
//! L2: 256-512 KB per core (~12 cycles)
//! L3: 2-64 MB shared (~40-80 cycles)
//! ```
//!
//! ### Thermal Throttling
//!
//! Monitor thermal state to avoid performance degradation:
//! - Check `ThermalCapabilities` for throttling status
//! - Consider workload distribution across cores
//! - Allow thermal headroom for sustained workloads
//!
//! ## Compiler Flags
//!
//! To enable architecture-specific optimizations:
//!
//! ```bash
//! # Native CPU detection
//! RUSTFLAGS="-C target-cpu=native"
//!
//! # Specific features
//! RUSTFLAGS="-C target-feature=+avx2,+fma"
//!
//! # AVX-512 (if supported)
//! RUSTFLAGS="-C target-feature=+avx512f,+avx512cd,+avx512bw,+avx512dq,+avx512vl"
//! ```
//!
//! ## Safety Notes
//!
//! - CPUID execution is safe and unprivileged
//! - Feature detection does not enable features (use compiler flags)
//! - Hypervisors may hide or emulate some features
//! - Always check capabilities before using SIMD intrinsics
//!
//! ## Known Limitations
//!
//! - MSR (Model-Specific Register) access requires kernel privileges
//! - Some power management features require OS support
//! - Frequency scaling accuracy depends on CPU generation
//! - AVX-512 availability varies by Intel CPU SKU

/// CPUID result structure
#[derive(Debug, Clone, Copy)]
pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

/// Execute CPUID instruction
///
/// # Safety
///
/// This function uses inline assembly to execute the CPUID instruction.
/// CPUID is a safe, unprivileged instruction on x86_64, but we mark it
/// unsafe due to the use of inline assembly.
#[cfg(target_arch = "x86_64")]
unsafe fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;

    // Note: We can't use ebx directly as LLVM reserves rbx internally.
    // We use xchg to swap rbx with a temporary register before and after cpuid.
    core::arch::asm!(
        "xchg {0:r}, rbx",  // Save rbx to temp, load temp into rbx
        "cpuid",             // Execute cpuid (modifies eax, ebx, ecx, edx)
        "xchg {0:r}, rbx",  // Swap back: temp gets ebx result, rbx restored
        out(reg) ebx,
        inout("eax") leaf => eax,
        inout("ecx") subleaf => ecx,
        inout("edx") 0u32 => edx,
        options(nomem, nostack, preserves_flags)
    );

    CpuidResult { eax, ebx, ecx, edx }
}

/// Detect SSE4.2 support
///
/// SSE4.2 is indicated by CPUID.01H:ECX.SSE4_2[bit 20]
pub fn has_sse4_2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = cpuid(1, 0);
            (result.ecx & (1 << 20)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Detect FMA (Fused Multiply-Add) support
///
/// FMA is indicated by CPUID.01H:ECX.FMA[bit 12]
pub fn has_fma() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = cpuid(1, 0);
            (result.ecx & (1 << 12)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Detect AES-NI support
///
/// AES-NI is indicated by CPUID.01H:ECX.AES[bit 25]
pub fn has_aes_ni() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = cpuid(1, 0);
            (result.ecx & (1 << 25)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Detect AVX support
///
/// AVX is indicated by CPUID.01H:ECX.AVX[bit 28]
pub fn has_avx() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = cpuid(1, 0);
            (result.ecx & (1 << 28)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Detect AVX2 support
///
/// AVX2 is indicated by CPUID.07H:EBX.AVX2[bit 5]
pub fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = cpuid(7, 0);
            (result.ebx & (1 << 5)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Detect AVX-512 Foundation support
///
/// AVX-512F is indicated by CPUID.07H:EBX.AVX512F[bit 16]
pub fn has_avx512f() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let result = cpuid(7, 0);
            (result.ebx & (1 << 16)) != 0
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Detect all x86_64 capabilities
pub fn detect_x86_64_capabilities() -> crate::capabilities::HardwareCapabilities {
    use crate::capabilities::HardwareCapabilities;

    let mut caps = HardwareCapabilities::FPU;

    if has_sse4_2() {
        caps |= HardwareCapabilities::SSE4_2;
    }

    if has_fma() {
        caps |= HardwareCapabilities::FMA;
    }

    if has_aes_ni() {
        caps |= HardwareCapabilities::AES_NI;
    }

    if has_avx() {
        caps |= HardwareCapabilities::AVX;
    }

    if has_avx2() {
        caps |= HardwareCapabilities::AVX2;
    }

    if has_avx512f() {
        caps |= HardwareCapabilities::AVX512;
    }

    caps
}

pub fn init() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_cpuid_basic() {
        unsafe {
            let result = cpuid(0, 0);
            // EAX should contain the maximum supported standard function
            assert!(result.eax > 0);
        }
    }

    #[test]
    fn test_sse4_2_detection() {
        // Just ensure it doesn't panic
        let _has_sse4_2 = has_sse4_2();
    }

    #[test]
    fn test_fma_detection() {
        let _has_fma = has_fma();
    }

    #[test]
    fn test_aes_ni_detection() {
        let _has_aes_ni = has_aes_ni();
    }

    #[test]
    fn test_avx_detection() {
        let _has_avx = has_avx();
    }

    #[test]
    fn test_avx2_detection() {
        let _has_avx2 = has_avx2();
    }

    #[test]
    fn test_avx512f_detection() {
        let _has_avx512 = has_avx512f();
    }

    #[test]
    fn test_detect_all_capabilities() {
        let caps = detect_x86_64_capabilities();
        // FPU should always be present on x86_64
        #[cfg(target_arch = "x86_64")]
        assert!(caps.contains(crate::capabilities::HardwareCapabilities::FPU));
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_capability_consistency() {
        // If AVX2 is present, AVX should also be present
        if has_avx2() {
            assert!(has_avx(), "AVX2 requires AVX");
        }

        // If AVX512F is present, AVX2 should also be present
        if has_avx512f() {
            assert!(has_avx2(), "AVX-512F requires AVX2");
            assert!(has_avx(), "AVX-512F requires AVX");
        }
    }
}
