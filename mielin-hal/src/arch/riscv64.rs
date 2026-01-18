//! RISC-V 64-bit Architecture Support
//!
//! This module provides RISC-V 64-bit (RV64) hardware abstraction and capability detection.
//!
//! ## Supported Processors
//!
//! - **SiFive**: U74, U84 (FU740 SoC)
//! - **StarFive**: JH7110 (VisionFive 2)
//! - **Allwinner**: D1 (XuanTie C906)
//! - **T-Head**: C910, C920 (Alibaba)
//! - **SiFive Performance**: P270, P670 (future)
//!
//! ## Hardware Capabilities
//!
//! ### Base ISA Extensions
//!
//! - **RV64I**: Base integer instruction set
//! - **M**: Integer multiply/divide
//! - **A**: Atomic instructions
//! - **F**: Single-precision floating-point
//! - **D**: Double-precision floating-point
//! - **C**: Compressed instructions (16-bit)
//!
//! ### Standard Extensions
//!
//! - **V** (Vector): Variable-length SIMD (RVV 1.0)
//!   - VLEN: 128, 256, 512, or 1024 bits
//!   - ELEN: Maximum element width
//! - **B** (Bit Manipulation): Bitwise operations
//! - **K** (Crypto): Scalar cryptography
//! - **J** (JIT): Dynamic translation support
//! - **P** (Packed-SIMD): Fixed-width SIMD
//!
//! ### Detection Method
//!
//! Capabilities are detected via:
//! ```text
//! misa CSR: Machine ISA register (M-mode)
//! marchid CSR: Architecture ID
//! mimpid CSR: Implementation ID
//! Device Tree: /proc/device-tree/cpus/cpu@*/riscv,isa
//! ```
//!
//! ## Usage Examples
//!
//! ### Vector Extension Detection
//!
//! ```no_run
//! use mielin_hal::capabilities;
//!
//! let caps = capabilities::HardwareProfile::detect();
//!
//! if caps.capabilities.contains(capabilities::HardwareCapabilities::RVV) {
//!     println!("RISC-V Vector extension available");
//!     println!("Vector width: {} bits", caps.max_vector_width());
//! }
//! ```
//!
//! ### Platform Detection
//!
//! ```no_run
//! use mielin_hal::platform;
//!
//! let platform = platform::detect_platform();
//!
//! match platform {
//!     platform::Platform::GenericRiscV => {
//!         println!("Running on RISC-V platform");
//!     }
//!     _ => {}
//! }
//! ```
//!
//! ### Cache Configuration
//!
//! ```no_run
//! use mielin_hal::cache::CacheTopology;
//!
//! let cache = CacheTopology::detect();
//!
//! // RISC-V cache sizes vary widely by implementation:
//! // L1: 16-64 KB per core
//! // L2: 256 KB - 2 MB per core/cluster
//! // L3: Optional, 1-16 MB shared
//!
//! println!("Cache line: {} bytes", cache.default_line_size);
//! ```
//!
//! ## Performance Considerations
//!
//! ### Vector Extension (RVV)
//!
//! The RISC-V Vector extension is highly configurable:
//! - **VLEN**: Vector register length (implementation-defined)
//! - **LMUL**: Length multiplier for grouping registers
//! - **SEW**: Selected element width (8, 16, 32, 64 bits)
//! - **VTA/VMA**: Tail/mask agnostic policies
//!
//! Best practices:
//! - Write vector code that adapts to any VLEN
//! - Use vsetvli to configure vector length
//! - Prefer vector-length agnostic algorithms
//! - Check VLEN at runtime for optimization
//!
//! ### Memory Consistency
//!
//! RISC-V has a weak memory model (RVWMO):
//! - Use fence instructions for synchronization
//! - FENCE.I for instruction cache coherency
//! - Atomic operations (AMO) for lock-free algorithms
//! - Consider TSO extension if available
//!
//! ### Compressed Instructions
//!
//! RV64C reduces code size by ~25-30%:
//! - Automatically used by compiler
//! - Improves icache efficiency
//! - No performance penalty on modern cores
//!
//! ## Compiler Flags
//!
//! Enable RISC-V optimizations:
//!
//! ```bash
//! # Generic RV64GC (RV64IMAFD + C)
//! RUSTFLAGS="-C target-cpu=generic-rv64"
//!
//! # With vector extension
//! RUSTFLAGS="-C target-feature=+v"
//!
//! # With bit manipulation
//! RUSTFLAGS="-C target-feature=+b"
//!
//! # With crypto extensions
//! RUSTFLAGS="-C target-feature=+k"
//!
//! # Specific microarchitecture
//! RUSTFLAGS="-C target-cpu=sifive-u74"
//! ```
//!
//! ## Platform-Specific Notes
//!
//! ### StarFive VisionFive 2
//! - JH7110 SoC with SiFive U74 cores (4x 1.5 GHz)
//! - RV64GC + Vector extension
//! - 2-8 GB DDR4
//! - PCIe, USB 3.0, Gigabit Ethernet
//!
//! ### SiFive HiFive Unmatched
//! - FU740 SoC with SiFive U74 cores (4x 1.4 GHz)
//! - RV64GC (no vector yet)
//! - 16 GB DDR4
//! - PCIe, NVMe, Gigabit Ethernet
//!
//! ### Allwinner D1
//! - XuanTie C906 core (single-core 1 GHz)
//! - RV64GCV (with vector)
//! - Designed for embedded/IoT
//! - Low power consumption
//!
//! ## ISA String Format
//!
//! RISC-V ISA strings describe capabilities:
//! ```text
//! rv64imafdcv
//! └┬┘└┬─┬─┬─┬─┬─┬┘
//!  │  │ │ │ │ │ └─ Vector extension
//!  │  │ │ │ │ └─── Compressed
//!  │  │ │ │ └───── Double float
//!  │  │ │ └─────── Single float
//!  │  │ └───────── Atomic
//!  │  └─────────── Multiply/Divide
//!  └────────────── Base (RV64I)
//! ```
//!
//! ## Safety Notes
//!
//! - CSR access requires M-mode (machine mode) privileges
//! - Vector instructions require V extension to be enabled
//! - Misaligned access behavior is implementation-defined
//! - Always check feature availability before use
//!
//! ## Known Limitations
//!
//! - Limited RISC-V hardware available compared to x86/ARM
//! - Vector extension (RVV) is still being adopted
//! - Toolchain support varies by platform
//! - Performance counters require SBI (Supervisor Binary Interface)
//! - Some implementations lack L2/L3 caches
//!
//! ## Future Extensions
//!
//! Upcoming RISC-V extensions:
//! - **H** (Hypervisor): Virtualization support
//! - **B** (Bit Manipulation): Ratified, awaiting hardware
//! - **J** (JIT): Dynamic translation
//! - **Zfinx**: Float in integer registers
//! - **Zicond**: Integer conditional operations

extern crate alloc;

use crate::capabilities::HardwareCapabilities;
use alloc::string::String;
use alloc::vec::Vec;

pub fn init() {}

/// Detect RISC-V capabilities from ISA string
///
/// Parses RISC-V ISA strings from device-tree or other sources to determine
/// available hardware capabilities.
///
/// # ISA String Format
///
/// RISC-V ISA strings follow the format: `rv64imafdcvbk_zbs_zba...`
/// - Base: `rv64` or `rv32`
/// - Single-letter extensions: `i`, `m`, `a`, `f`, `d`, `c`, `v`, `b`, `k`
/// - Multi-letter extensions: `_z*` (e.g., `_zbs`, `_zba`)
pub fn detect_riscv_capabilities() -> HardwareCapabilities {
    let mut caps = HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS;

    // Try to read ISA string from device-tree
    if let Some(isa_string) = read_isa_from_devicetree() {
        caps |= parse_isa_string(&isa_string);
    }

    // Check compile-time features as fallback
    #[cfg(target_arch = "riscv64")]
    {
        #[cfg(target_feature = "v")]
        {
            caps |= HardwareCapabilities::RVV;
        }
    }

    caps
}

/// Parse RISC-V ISA string and extract capabilities
fn parse_isa_string(isa: &str) -> HardwareCapabilities {
    let mut caps = HardwareCapabilities::NONE;
    let isa_lower = isa.to_lowercase();

    // 'g' is shorthand for IMAFD (base + multiply + atomic + float + double)
    if isa_lower.contains('g') {
        caps |= HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS;
    }

    // Single-letter extensions
    if isa_lower.contains('f') || isa_lower.contains('d') {
        caps |= HardwareCapabilities::FPU;
    }

    if isa_lower.contains('a') {
        caps |= HardwareCapabilities::ATOMICS;
    }

    if isa_lower.contains('v') {
        caps |= HardwareCapabilities::RVV | HardwareCapabilities::SIMD;
    }

    if isa_lower.contains('b') {
        caps |= HardwareCapabilities::RVB;
    }

    if isa_lower.contains('k') || isa_lower.contains("_zk") {
        caps |= HardwareCapabilities::RVK | HardwareCapabilities::CRYPTO;
    }

    // Multi-letter extensions
    if isa_lower.contains("_zbs") || isa_lower.contains("_zba") || isa_lower.contains("_zbb") {
        caps |= HardwareCapabilities::RVB;
    }

    caps
}

/// Read RISC-V ISA string from device-tree
///
/// Reads from `/proc/device-tree/cpus/cpu@0/riscv,isa` on Linux systems
fn read_isa_from_devicetree() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        // Try reading from device-tree
        let paths = [
            "/proc/device-tree/cpus/cpu@0/riscv,isa",
            "/proc/device-tree/cpus/cpu0/riscv,isa",
            "/sys/firmware/devicetree/base/cpus/cpu@0/riscv,isa",
        ];

        for path in &paths {
            if let Ok(content) = core::str::from_utf8(&read_file_syscall(path)) {
                // Device-tree strings are null-terminated
                let trimmed = content.trim_end_matches('\0').trim();
                if !trimmed.is_empty() {
                    return Some(String::from(trimmed));
                }
            }
        }
    }

    None
}

/// Read file using syscalls (no_std compatible)
#[cfg(target_os = "linux")]
fn read_file_syscall(path: &str) -> Vec<u8> {
    use alloc::vec;

    let mut buffer = vec![0u8; 512];
    let path_bytes = path.as_bytes();

    #[cfg(target_arch = "riscv64")]
    {
        use core::arch::asm;

        unsafe {
            let fd: i64;
            // open syscall (56)
            asm!(
                "li a7, 56",
                "ecall",
                in("a0") path_bytes.as_ptr(),
                in("a1") 0, // O_RDONLY
                lateout("a0") fd,
            );

            if fd < 0 {
                return Vec::new();
            }

            let bytes_read: i64;
            // read syscall (63)
            asm!(
                "li a7, 63",
                "ecall",
                in("a0") fd,
                in("a1") buffer.as_mut_ptr(),
                in("a2") buffer.len(),
                lateout("a0") bytes_read,
            );

            // close syscall (57)
            asm!(
                "li a7, 57",
                "ecall",
                in("a0") fd,
            );

            if bytes_read > 0 {
                buffer.truncate(bytes_read as usize);
                return buffer;
            }
        }
    }

    #[cfg(not(target_arch = "riscv64"))]
    {
        // Fallback for non-RISC-V targets (testing)
        let _ = (path_bytes, &mut buffer);
    }

    Vec::new()
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
fn read_file_syscall(_path: &str) -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_isa_string_basic() {
        let isa = "rv64imafdcv";
        let caps = parse_isa_string(isa);

        assert!(caps.contains(HardwareCapabilities::FPU));
        assert!(caps.contains(HardwareCapabilities::ATOMICS));
        assert!(caps.contains(HardwareCapabilities::RVV));
        assert!(caps.contains(HardwareCapabilities::SIMD));
    }

    #[test]
    fn test_parse_isa_string_with_extensions() {
        let isa = "rv64gc_zba_zbs_zbc";
        let caps = parse_isa_string(isa);

        assert!(caps.contains(HardwareCapabilities::FPU));
        assert!(caps.contains(HardwareCapabilities::ATOMICS));
        assert!(caps.contains(HardwareCapabilities::RVB));
    }

    #[test]
    fn test_parse_isa_string_vector() {
        let isa = "rv64imafdc_v_zve32f";
        let caps = parse_isa_string(isa);

        assert!(caps.contains(HardwareCapabilities::RVV));
        assert!(caps.contains(HardwareCapabilities::SIMD));
    }

    #[test]
    fn test_parse_isa_string_crypto() {
        let isa = "rv64imafdc_zk_zkn_zks";
        let caps = parse_isa_string(isa);

        assert!(caps.contains(HardwareCapabilities::RVK));
        assert!(caps.contains(HardwareCapabilities::CRYPTO));
    }

    #[test]
    fn test_parse_isa_string_bit_manip() {
        let isa = "rv64imafdcb_zba_zbb_zbs";
        let caps = parse_isa_string(isa);

        assert!(caps.contains(HardwareCapabilities::RVB));
    }

    #[test]
    fn test_detect_riscv_capabilities() {
        let caps = detect_riscv_capabilities();

        // Should at least have FPU and ATOMICS
        assert!(caps.contains(HardwareCapabilities::FPU));
        assert!(caps.contains(HardwareCapabilities::ATOMICS));
    }
}
