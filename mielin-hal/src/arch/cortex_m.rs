//! ARM Cortex-M Architecture Support
//!
//! This module provides hardware detection and capability querying for
//! ARM Cortex-M microcontrollers, from M0 to M85.
//!
//! ## Supported Variants
//!
//! - **Cortex-M0/M0+**: Ultra-low power (ARMv6-M)
//! - **Cortex-M3**: Baseline performance (ARMv7-M)
//! - **Cortex-M4**: DSP + optional FPU (ARMv7E-M)
//! - **Cortex-M7**: High performance + caches (ARMv7E-M)
//! - **Cortex-M23**: Low power with TrustZone (ARMv8-M Baseline)
//! - **Cortex-M33**: Mainstream with TrustZone (ARMv8-M Mainline)
//! - **Cortex-M55**: AI/ML optimized (ARMv8.1-M + Helium)
//! - **Cortex-M85**: Highest performance (ARMv8.1-M + Helium)
//!
//! ## Hardware Features
//!
//! ### Memory Protection Unit (MPU)
//! - Configurable memory regions (typically 8 or 16)
//! - Access permissions and memory attributes
//! - Optional separate instruction/data regions (ARMv8-M)
//!
//! ### Floating Point Unit (FPU)
//! - **FPv4-SP**: Single-precision (M4, M7)
//! - **FPv5**: Single and double-precision (M7, M33, M55, M85)
//! - Lazy context preservation
//!
//! ### DSP Extensions
//! - SIMD instructions for 8/16-bit data
//! - Saturating arithmetic
//! - Multiply-accumulate (MAC) operations
//! - Available on M4, M7, M33, M55, M85
//!
//! ### NVIC (Nested Vectored Interrupt Controller)
//! - 1-496 external interrupts
//! - Configurable priority levels (2-256)
//! - Priority grouping for preemption
//!
//! ## Usage Examples
//!
//! ### Basic Capability Detection
//!
//! ```no_run
//! # #[cfg(all(target_arch = "arm", target_os = "none"))]
//! # {
//! use mielin_hal::arch::cortex_m::CortexMCapabilities;
//!
//! let caps = CortexMCapabilities::detect();
//!
//! println!("Processor: {:?}", caps.processor);
//! println!("MPU regions: {}", caps.mpu.num_regions);
//! println!("FPU: {:?}", caps.fpu.variant);
//! println!("DSP: {}", caps.dsp.present);
//! # }
//! ```
//!
//! ### MPU Configuration
//!
//! ```no_run
//! # #[cfg(all(target_arch = "arm", target_os = "none"))]
//! # {
//! use mielin_hal::arch::cortex_m::MpuInfo;
//!
//! let mpu = MpuInfo::detect();
//!
//! if mpu.present {
//!     println!("MPU available with {} regions", mpu.num_regions);
//!     // Configure MPU regions for memory protection
//! }
//! # }
//! ```
//!
//! ### Interrupt Priority Configuration
//!
//! ```no_run
//! # #[cfg(all(target_arch = "arm", target_os = "none"))]
//! # {
//! use mielin_hal::arch::cortex_m::NvicInfo;
//!
//! let nvic = NvicInfo::detect();
//!
//! println!("Priority bits: {}", nvic.priority_bits);
//! println!("External interrupts: {}", nvic.num_interrupts);
//! println!("Priority levels: {}", nvic.num_priority_levels);
//! # }
//! ```
//!
//! ## Safety Requirements
//!
//! Many functions in this module use raw pointer accesses to read hardware
//! registers. These accesses are safe when:
//! - Running on actual Cortex-M hardware
//! - The target architecture is configured correctly
//! - Memory-mapped peripheral addresses are valid for the specific MCU
//! - Processor is in a valid state (not in fault handler)
//!
//! ## Performance Notes
//!
//! ### M0/M0+ vs M3/M4/M7
//! - M0/M0+: 3-stage pipeline, single-cycle multiplier, no DSP
//! - M3: 3-stage pipeline, hardware divider, no FPU
//! - M4: 3-stage pipeline, DSP, optional FPU (single-precision)
//! - M7: 6-stage superscalar, DSP, FPU (single/double), caches
//!
//! ### Cache Considerations (M7, M55, M85)
//! - I-Cache: 4-64 KB (instruction)
//! - D-Cache: 4-64 KB (data)
//! - TCM: Tightly Coupled Memory for deterministic access
//!
//! ## Common STM32 Families
//!
//! Platform detection supports:
//! - **F0**: Cortex-M0 (entry-level)
//! - **F1**: Cortex-M3 (mainstream)
//! - **F4**: Cortex-M4 with FPU (high performance)
//! - **F7**: Cortex-M7 with caches (very high performance)
//! - **H7**: Cortex-M7 dual-core (ultra-high performance)
//! - **L4**: Cortex-M4 (ultra-low power)
//! - **G4**: Cortex-M4 (mixed-signal)
//!
//! See the `platform` module for STM32-specific detection.

use core::ptr;

/// System Control Block (SCB) base address
/// This is standard for all Cortex-M processors
const SCB_BASE: usize = 0xE000_ED00;

/// MPU Type Register offset from SCB base
const MPU_TYPE_OFFSET: usize = 0x90;

/// CPUID Register offset from SCB base
const CPUID_OFFSET: usize = 0x00;

/// Coprocessor Access Control Register offset
const CPACR_OFFSET: usize = 0x88;

/// Floating Point Context Control Register
const FPCCR_OFFSET: usize = 0xF34;

/// NVIC base address
const NVIC_BASE: usize = 0xE000_E100;

/// MPU (Memory Protection Unit) information
#[derive(Debug, Clone, Copy, Default)]
pub struct MpuInfo {
    /// Whether MPU is present
    pub present: bool,
    /// Number of MPU regions (0 if not present)
    pub num_regions: u8,
    /// Whether MPU supports separate instruction/data regions
    pub separate_regions: bool,
}

impl MpuInfo {
    /// Detect MPU capabilities
    ///
    /// # Safety
    ///
    /// This function reads from memory-mapped hardware registers.
    /// It is safe to call on Cortex-M hardware with proper memory layout.
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "arm", target_os = "none"))]
        {
            unsafe { Self::detect_unsafe() }
        }

        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            // Return default (no MPU) on non-Cortex-M platforms
            Self::default()
        }
    }

    #[cfg(all(target_arch = "arm", target_os = "none"))]
    unsafe fn detect_unsafe() -> Self {
        let mpu_type_addr = (SCB_BASE + MPU_TYPE_OFFSET) as *const u32;
        let mpu_type = ptr::read_volatile(mpu_type_addr);

        let present = (mpu_type & 0xFF) > 0;
        let num_regions = ((mpu_type >> 8) & 0xFF) as u8;
        let separate_regions = ((mpu_type >> 16) & 0x1) != 0;

        Self {
            present,
            num_regions,
            separate_regions,
        }
    }
}

/// FPU (Floating Point Unit) variant information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpuVariant {
    /// No FPU present
    None,
    /// Single-precision FPU only (FPv4-SP)
    SinglePrecision,
    /// Single and double-precision FPU (FPv5)
    DoublePrecision,
}

impl Default for FpuVariant {
    fn default() -> Self {
        Self::None
    }
}

/// FPU information
#[derive(Debug, Clone, Copy, Default)]
pub struct FpuInfo {
    /// FPU variant
    pub variant: FpuVariant,
    /// Whether FPU context preservation is enabled
    pub lazy_context: bool,
    /// Number of FPU registers (typically 32 for S0-S31)
    pub num_registers: u8,
}

impl FpuInfo {
    /// Detect FPU capabilities
    ///
    /// # Safety
    ///
    /// This function reads from memory-mapped hardware registers.
    /// It is safe to call on Cortex-M hardware with proper memory layout.
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "arm", target_os = "none"))]
        {
            unsafe { Self::detect_unsafe() }
        }

        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            // Return default (no FPU) on non-Cortex-M platforms
            Self::default()
        }
    }

    #[cfg(all(target_arch = "arm", target_os = "none"))]
    unsafe fn detect_unsafe() -> Self {
        // Read CPACR (Coprocessor Access Control Register)
        let cpacr_addr = (SCB_BASE + CPACR_OFFSET) as *const u32;
        let cpacr = ptr::read_volatile(cpacr_addr);

        // CP10 and CP11 access control (bits 20-23)
        let cp10_access = (cpacr >> 20) & 0x3;
        let cp11_access = (cpacr >> 22) & 0x3;

        // FPU is enabled if both CP10 and CP11 are accessible
        let fpu_enabled = cp10_access == 0x3 && cp11_access == 0x3;

        if !fpu_enabled {
            return Self::default();
        }

        // Read CPUID to determine processor variant
        let cpuid_addr = (SCB_BASE + CPUID_OFFSET) as *const u32;
        let cpuid = ptr::read_volatile(cpuid_addr);

        // Part number is in bits 4-15
        let part_num = (cpuid >> 4) & 0xFFF;

        // Determine FPU variant based on Cortex-M variant
        let variant = match part_num {
            0xC24 => FpuVariant::SinglePrecision, // Cortex-M4
            0xC27 => FpuVariant::DoublePrecision, // Cortex-M7
            0xD21 => FpuVariant::SinglePrecision, // Cortex-M33
            0xD20 => FpuVariant::SinglePrecision, // Cortex-M23 (optional FPU)
            _ => {
                // Unknown variant, check if double-precision is supported
                // by attempting to detect FPv5 features
                FpuVariant::SinglePrecision // Conservative default
            }
        };

        // Read FPCCR (FP Context Control Register) if available
        let fpccr_addr = (SCB_BASE + FPCCR_OFFSET) as *const u32;
        let fpccr = ptr::read_volatile(fpccr_addr);

        // LSPEN bit (bit 30) indicates lazy context save
        let lazy_context = (fpccr & (1 << 30)) != 0;

        Self {
            variant,
            lazy_context,
            num_registers: 32, // S0-S31 (or D0-D15 for double precision)
        }
    }
}

/// DSP extension information
#[derive(Debug, Clone, Copy, Default)]
pub struct DspInfo {
    /// Whether DSP extensions are present
    pub present: bool,
    /// Whether SIMD instructions are available
    pub simd: bool,
    /// Whether saturating arithmetic is available
    pub saturation: bool,
}

impl DspInfo {
    /// Detect DSP extension capabilities
    ///
    /// # Safety
    ///
    /// This function reads from memory-mapped hardware registers.
    /// It is safe to call on Cortex-M hardware with proper memory layout.
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "arm", target_os = "none"))]
        {
            unsafe { Self::detect_unsafe() }
        }

        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            // Return default (no DSP) on non-Cortex-M platforms
            Self::default()
        }
    }

    #[cfg(all(target_arch = "arm", target_os = "none"))]
    unsafe fn detect_unsafe() -> Self {
        // Read CPUID to determine processor variant
        let cpuid_addr = (SCB_BASE + CPUID_OFFSET) as *const u32;
        let cpuid = ptr::read_volatile(cpuid_addr);

        // Part number is in bits 4-15
        let part_num = (cpuid >> 4) & 0xFFF;

        // DSP extensions are present in Cortex-M4, M7, M33, M55, M85
        let present = matches!(
            part_num,
            0xC24 | // Cortex-M4
            0xC27 | // Cortex-M7
            0xD21 | // Cortex-M33
            0xD22 // Cortex-M55/M85 (same part number range)
        );

        Self {
            present,
            simd: present,       // SIMD is part of DSP extensions
            saturation: present, // Saturating arithmetic is part of DSP extensions
        }
    }
}

/// NVIC (Nested Vectored Interrupt Controller) capabilities
#[derive(Debug, Clone, Copy, Default)]
pub struct NvicInfo {
    /// Number of interrupt priority levels (typically 4, 8, 16, 32, 64, 128, or 256)
    pub priority_levels: u8,
    /// Number of interrupt lines supported
    pub num_interrupts: u16,
    /// Whether interrupt grouping is supported
    pub grouping: bool,
}

impl NvicInfo {
    /// Detect NVIC capabilities
    ///
    /// # Safety
    ///
    /// This function reads from memory-mapped hardware registers.
    /// It is safe to call on Cortex-M hardware with proper memory layout.
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "arm", target_os = "none"))]
        {
            unsafe { Self::detect_unsafe() }
        }

        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            // Return default on non-Cortex-M platforms
            Self::default()
        }
    }

    #[cfg(all(target_arch = "arm", target_os = "none"))]
    unsafe fn detect_unsafe() -> Self {
        // Read CPUID to determine processor variant
        let cpuid_addr = (SCB_BASE + CPUID_OFFSET) as *const u32;
        let cpuid = ptr::read_volatile(cpuid_addr);

        // Part number is in bits 4-15
        let part_num = (cpuid >> 4) & 0xFFF;

        // Determine priority levels based on Cortex-M variant
        let priority_levels = match part_num {
            0xC20 | 0xC60 => 4, // Cortex-M0/M0+: 2 bits (4 levels)
            0xC23 => 8,         // Cortex-M3: 3-8 bits (implementation defined)
            0xC24 => 8,         // Cortex-M4: typically 3 bits (8 levels)
            0xC27 => 8,         // Cortex-M7: typically 3 bits (8 levels)
            0xD20 => 4,         // Cortex-M23: 2 bits (4 levels)
            0xD21 => 8,         // Cortex-M33: typically 3 bits (8 levels)
            0xD22 => 16,        // Cortex-M55/M85: typically 4 bits (16 levels)
            _ => 4,             // Default/unknown
        };

        // Read ICTR (Interrupt Controller Type Register) for interrupt count
        // ICTR is at offset 0x004 from SCB base
        let ictr_addr = (SCB_BASE + 0x004) as *const u32;
        let ictr = ptr::read_volatile(ictr_addr);

        // INTLINESNUM field (bits 0-4) indicates number of interrupt lines
        // Actual number = (INTLINESNUM + 1) * 32
        let intlinesnum = ictr & 0x1F;
        let num_interrupts = ((intlinesnum + 1) * 32) as u16;

        Self {
            priority_levels,
            num_interrupts,
            grouping: true, // All Cortex-M processors support interrupt grouping
        }
    }

    /// Get the number of priority bits
    pub fn priority_bits(&self) -> u8 {
        // Calculate log2 of priority levels
        let mut bits = 0;
        let mut levels = self.priority_levels;
        while levels > 1 {
            levels >>= 1;
            bits += 1;
        }
        bits
    }
}

/// Complete Cortex-M capabilities
#[derive(Debug, Clone, Copy, Default)]
pub struct CortexMCapabilities {
    /// MPU information
    pub mpu: MpuInfo,
    /// FPU information
    pub fpu: FpuInfo,
    /// DSP extension information
    pub dsp: DspInfo,
    /// NVIC information
    pub nvic: NvicInfo,
    /// Processor part number
    pub part_number: u16,
    /// Processor revision
    pub revision: u8,
}

impl CortexMCapabilities {
    /// Detect all Cortex-M capabilities
    ///
    /// # Safety
    ///
    /// This function reads from memory-mapped hardware registers.
    /// It is safe to call on Cortex-M hardware with proper memory layout.
    pub fn detect() -> Self {
        let mpu = MpuInfo::detect();
        let fpu = FpuInfo::detect();
        let dsp = DspInfo::detect();
        let nvic = NvicInfo::detect();

        #[cfg(all(target_arch = "arm", target_os = "none"))]
        let (part_number, revision) = unsafe {
            let cpuid_addr = (SCB_BASE + CPUID_OFFSET) as *const u32;
            let cpuid = ptr::read_volatile(cpuid_addr);
            let part = ((cpuid >> 4) & 0xFFF) as u16;
            let rev = (cpuid & 0xF) as u8;
            (part, rev)
        };

        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        let (part_number, revision) = (0, 0);

        Self {
            mpu,
            fpu,
            dsp,
            nvic,
            part_number,
            revision,
        }
    }

    /// Get processor name from part number
    pub fn processor_name(&self) -> &'static str {
        match self.part_number {
            0xC20 => "Cortex-M0",
            0xC60 => "Cortex-M0+",
            0xC23 => "Cortex-M3",
            0xC24 => "Cortex-M4",
            0xC27 => "Cortex-M7",
            0xD20 => "Cortex-M23",
            0xD21 => "Cortex-M33",
            0xD22 => "Cortex-M55/M85",
            _ => "Unknown",
        }
    }
}

/// Initialize Cortex-M architecture support
pub fn init() {
    // Initialization code if needed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mpu_info_detect() {
        let mpu = MpuInfo::detect();
        // Should not panic
        // On non-Cortex-M platforms, should return default
        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            assert!(!mpu.present);
            assert_eq!(mpu.num_regions, 0);
        }
    }

    #[test]
    fn test_fpu_info_detect() {
        let fpu = FpuInfo::detect();
        // Should not panic
        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            assert_eq!(fpu.variant, FpuVariant::None);
        }
    }

    #[test]
    fn test_dsp_info_detect() {
        let dsp = DspInfo::detect();
        // Should not panic
        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            assert!(!dsp.present);
        }
    }

    #[test]
    fn test_nvic_info_detect() {
        let nvic = NvicInfo::detect();
        // Should not panic
        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            assert_eq!(nvic.num_interrupts, 0);
        }
    }

    #[test]
    fn test_nvic_priority_bits() {
        let nvic = NvicInfo {
            priority_levels: 8,
            num_interrupts: 64,
            grouping: true,
        };
        assert_eq!(nvic.priority_bits(), 3); // log2(8) = 3
    }

    #[test]
    fn test_cortex_m_capabilities_detect() {
        let caps = CortexMCapabilities::detect();
        // Should not panic
        let _name = caps.processor_name();
    }

    #[test]
    fn test_processor_names() {
        let test_cases = [
            (0xC20, "Cortex-M0"),
            (0xC60, "Cortex-M0+"),
            (0xC23, "Cortex-M3"),
            (0xC24, "Cortex-M4"),
            (0xC27, "Cortex-M7"),
            (0xD20, "Cortex-M23"),
            (0xD21, "Cortex-M33"),
            (0xD22, "Cortex-M55/M85"),
            (0xFFF, "Unknown"),
        ];

        for (part_num, expected_name) in test_cases {
            let caps = CortexMCapabilities {
                part_number: part_num,
                ..Default::default()
            };
            assert_eq!(caps.processor_name(), expected_name);
        }
    }

    #[test]
    fn test_fpu_variant_default() {
        let variant = FpuVariant::default();
        assert_eq!(variant, FpuVariant::None);
    }

    #[test]
    fn test_mpu_info_default() {
        let mpu = MpuInfo::default();
        assert!(!mpu.present);
        assert_eq!(mpu.num_regions, 0);
        assert!(!mpu.separate_regions);
    }

    #[test]
    fn test_fpu_info_default() {
        let fpu = FpuInfo::default();
        assert_eq!(fpu.variant, FpuVariant::None);
        assert!(!fpu.lazy_context);
        assert_eq!(fpu.num_registers, 0);
    }

    #[test]
    fn test_dsp_info_default() {
        let dsp = DspInfo::default();
        assert!(!dsp.present);
        assert!(!dsp.simd);
        assert!(!dsp.saturation);
    }

    #[test]
    fn test_nvic_info_default() {
        let nvic = NvicInfo::default();
        assert_eq!(nvic.priority_levels, 0);
        assert_eq!(nvic.num_interrupts, 0);
        assert!(!nvic.grouping);
    }
}
