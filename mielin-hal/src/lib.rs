//! MielinOS Hardware Abstraction Layer
//!
//! A `no_std` hardware abstraction layer providing unified interfaces across
//! x86_64, AArch64, RISC-V, and ARM Cortex-M architectures.
//!
//! ## Quick Start
//!
//! ```no_run
//! use mielin_hal::{detect_architecture, capabilities, cache, system};
//!
//! // Detect architecture
//! let arch = detect_architecture();
//! println!("Running on: {}", arch);
//!
//! // Get hardware capabilities
//! let profile = capabilities::HardwareProfile::detect();
//! println!("Cores: {}, Memory: {} MB", profile.core_count, profile.memory_size / 1024 / 1024);
//!
//! // Cache information
//! let cache = cache::CacheTopology::detect();
//! println!("L1: {} KB", cache.l1_total_size() / 1024);
//! ```
//!
//! ## Modules
//!
//! - [`arch`]: Architecture-specific implementations (x86_64, AArch64, RISC-V, Cortex-M)
//! - [`capabilities`]: SIMD and hardware feature detection
//! - [`cache`]: Cache topology and size information
//! - [`system`]: Memory and CPU topology
//! - [`platform`]: Platform-specific detection (Raspberry Pi, STM32, ESP32, BeagleBone)
//! - [`gpu`]: GPU detection and capabilities
//! - [`accelerator`]: NPU/TPU accelerator detection
//! - [`power`]: Power management and frequency information
//! - [`pmu`]: Performance Monitoring Unit (PMU) and hardware counters
//! - [`virtualization`]: Hypervisor and container detection
//! - [`devicetree`]: Device Tree parsing and querying
//! - [`acpi`]: ACPI table parsing and hardware enumeration
//! - [`runtime`]: Runtime capability switching and optimal path selection
//! - [`error`]: Error types and handling
//! - [`traits`]: Common trait abstractions
//!
//! ## Common Use Cases
//!
//! ### SIMD Feature Detection
//!
//! ```no_run
//! use mielin_hal::capabilities::{HardwareProfile, HardwareCapabilities};
//!
//! let caps = HardwareProfile::detect();
//!
//! // x86_64
//! if caps.capabilities.contains(HardwareCapabilities::AVX2) {
//!     println!("AVX2 available for 256-bit SIMD");
//! }
//!
//! // AArch64
//! if caps.capabilities.contains(HardwareCapabilities::SVE) {
//!     println!("SVE available with {} -bit vectors", caps.max_vector_width());
//! }
//!
//! // RISC-V (note: RVV feature flag not yet added, use NEON as placeholder)
//! if caps.capabilities.contains(HardwareCapabilities::NEON) {
//!     println!("Vector extension available");
//! }
//! ```
//!
//! ### Platform-Specific Code
//!
//! ```no_run
//! use mielin_hal::platform::{detect_platform, Platform, RaspberryPiCapabilities};
//!
//! match detect_platform() {
//!     Platform::RaspberryPi(model) => {
//!         let caps = RaspberryPiCapabilities::detect();
//!         println!("Raspberry Pi {} with {} GPIO pins", model, caps.gpio_pins);
//!     }
//!     Platform::Stm32(family) => {
//!         println!("STM32{} microcontroller", family);
//!     }
//!     Platform::Esp32(variant) => {
//!         println!("ESP32 {} with WiFi/Bluetooth", variant);
//!     }
//!     _ => println!("Generic platform"),
//! }
//! ```
//!
//! ### Cache-Aware Optimization
//!
//! ```no_run
//! use mielin_hal::cache::CacheTopology;
//!
//! let cache = CacheTopology::detect();
//! let l1_size = cache.l1_total_size();
//!
//! // Use cache size for optimization
//! println!("L1 cache: {} KB", l1_size / 1024);
//! ```
//!
//! ### Power Management
//!
//! ```no_run
//! use mielin_hal::power::detect_power_info;
//!
//! let power = detect_power_info();
//! println!("CPU: {} - {} MHz", power.frequency.min_mhz, power.frequency.max_mhz);
//!
//! if power.turbo_supported {
//!     println!("Turbo boost available");
//! }
//! ```
//!
//! ## Architecture Coverage
//!
//! | Architecture | Detection | SIMD | Caches | Platform | Power |
//! |--------------|-----------|------|--------|----------|-------|
//! | x86_64       | ✓         | ✓    | ✓      | Generic  | ✓     |
//! | AArch64      | ✓         | ✓    | ✓      | RPi, BB  | Partial |
//! | RISC-V       | ✓         | ✓    | ✓      | Generic  | Partial |
//! | Cortex-M     | ✓         | Partial | ✓   | STM32, ESP32 | Limited |
//!
//! ## Common Traits
//!
//! The `traits` module provides common abstractions:
//! - `Named` - For types with human-readable names
//! - `DeviceInfo` - Basic device information
//! - `ComputeDevice` - Compute-capable device interface
//! - `MemoryDevice` - Memory information interface
//! - `PowerDevice` - Power-aware device interface
//!
//! ## Safety Requirements for Architecture-Specific Code
//!
//! This crate contains architecture-specific code that directly accesses hardware
//! registers and uses platform-specific instructions. When using this crate, the
//! following safety requirements must be met:
//!
//! ### General Safety Requirements
//!
//! 1. **Correct Target Configuration**: The crate must be compiled with the correct
//!    target triple for your hardware. Mismatched configurations may result in
//!    undefined behavior or incorrect hardware access.
//!
//! 2. **Hardware Availability**: Detection functions assume the presence of standard
//!    hardware components (e.g., CPUID on x86_64, system registers on AArch64).
//!    Attempting to access non-existent hardware may cause faults.
//!
//! 3. **Privilege Level**: Some hardware detection requires specific privilege levels:
//!    - x86_64: CPUID is generally unprivileged, but some MSRs require ring 0
//!    - AArch64: System registers may require EL1 or higher
//!    - Cortex-M: Some registers require privileged mode
//!
//! ### Architecture-Specific Safety Requirements
//!
//! #### x86_64
//!
//! - **CPUID Instructions**: The crate uses inline assembly to execute CPUID instructions.
//!   These are safe on all x86_64 processors but may return different results based on
//!   the CPU vendor and model.
//!
//! - **Register Clobbering**: Inline assembly carefully preserves register state, but
//!   calling code should be aware of potential side effects.
//!
//! - **Feature Detection**: Capability detection via CPUID is inherently safe but may
//!   not reflect runtime state (e.g., features disabled by hypervisor).
//!
//! #### AArch64
//!
//! - **System Registers**: Reading system registers (e.g., CTR_EL0) requires appropriate
//!   exception level. The crate uses registers accessible from EL0 when possible.
//!
//! - **SVE/SME Detection**: Vector length detection assumes SVE/SME is enabled in the
//!   system. Accessing disabled vector units may cause undefined instruction exceptions.
//!
//! #### RISC-V
//!
//! - **CSR Access**: Control and Status Register access may require M-mode or S-mode.
//!   User-mode code may trap when accessing privileged CSRs.
//!
//! - **Vector Extension**: RVV detection assumes the vector extension is present and
//!   enabled. Accessing disabled vector units causes illegal instruction exceptions.
//!
//! #### Cortex-M
//!
//! - **Memory-Mapped Registers**: The Cortex-M module directly accesses memory-mapped
//!   peripheral registers. These accesses are safe ONLY when:
//!   - Running on actual Cortex-M hardware
//!   - The memory map matches standard Cortex-M layout (SCB at 0xE000_ED00)
//!   - The processor is in a valid state (not halted, not in fault handler)
//!
//! - **Privileged Operations**: Some register reads require privileged (Handler/Thread)
//!   mode. Unprivileged access may result in faults.
//!
//! - **MPU Considerations**: If the Memory Protection Unit (MPU) is enabled, it must
//!   allow read access to SCB/NVIC registers.
//!
//! ### Error Handling
//!
//! The crate provides error types via the `error` module to handle:
//! - Unsupported architecture features
//! - Detection failures
//! - Invalid parameters
//! - Hardware access errors
//!
//! Always check return values and handle errors appropriately in production code.
//!
//! ### Testing Safety
//!
//! When writing tests:
//! - Use `#[cfg(target_arch = "...")]` to ensure tests only run on supported architectures
//! - Stub out hardware access for unit tests when possible
//! - Use integration tests on real hardware for validation
//! - Be aware that tests in CI may run on different hardware than production
//!
//! ### Examples
//!
//! ```no_run
//! use mielin_hal::{Architecture, detect_architecture, error::check_architecture_support};
//!
//! // Check if a feature is supported before using it
//! let result = check_architecture_support(
//!     "SVE2",
//!     &[Architecture::AArch64]
//! );
//!
//! match result {
//!     Ok(()) => {
//!         // Safe to use SVE2 features
//!     }
//!     Err(e) => {
//!         // Handle unsupported architecture
//!         eprintln!("Error: {}", e);
//!     }
//! }
//! ```

#![no_std]

extern crate alloc;

pub mod accelerator;
pub mod acpi;
pub mod arch;
pub mod cache;
pub mod capabilities;
pub mod devicetree;
pub mod error;
pub mod gpu;
pub mod hal_probe;
pub mod hardware_db;
pub mod platform;
pub mod pmu;
pub mod power;
pub mod runtime;
pub mod system;
pub mod traits;
pub mod virtualization;

// Re-export common traits and types at crate root
pub use traits::{ComputeDevice, DeviceInfo, MemoryDevice, Named, PowerDevice};

// Re-export error types
pub use error::{Error, Result};

// Re-export Cortex-M types when on Cortex-M architecture
#[cfg(all(target_arch = "arm", target_os = "none"))]
pub use arch::cortex_m::{CortexMCapabilities, DspInfo, FpuInfo, FpuVariant, MpuInfo, NvicInfo};

// Re-export HAL probe / hardening types
pub use hal_probe::{
    DetectedPlatform, DiagnosticSeverity, HalDiagnostics, HardwareProbe, PlatformDetector,
    ProbeConfig, ProbeError, ProbeResult,
};

// Re-export platform types
pub use platform::{
    BeagleBoneModel, Esp32Capabilities, Esp32Variant, JetsonModel, Platform, PlatformCapabilities,
    RaspberryPiCapabilities, RaspberryPiModel, Stm32Capabilities, Stm32Family,
};

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    AArch64,
    RiscV64,
    X86_64,
    ArmCortexM,
    CortexM,
    LoongArch64,
    Xtensa,
    /// ARMv7-A application processor (Cortex-A series, Linux userspace).
    Arm32,
    /// 32-bit x86 (IA-32 / i686) processor.
    X86,
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Architecture::AArch64 => write!(f, "aarch64"),
            Architecture::RiscV64 => write!(f, "riscv64"),
            Architecture::X86_64 => write!(f, "x86_64"),
            Architecture::ArmCortexM => write!(f, "arm-cortex-m"),
            Architecture::CortexM => write!(f, "cortex-m"),
            Architecture::LoongArch64 => write!(f, "loongarch64"),
            Architecture::Xtensa => write!(f, "xtensa"),
            Architecture::Arm32 => write!(f, "armv7"),
            Architecture::X86 => write!(f, "x86"),
        }
    }
}

pub fn detect_architecture() -> Architecture {
    #[cfg(target_arch = "aarch64")]
    return Architecture::AArch64;

    #[cfg(target_arch = "riscv64")]
    return Architecture::RiscV64;

    #[cfg(target_arch = "x86_64")]
    return Architecture::X86_64;

    #[cfg(all(target_arch = "arm", target_os = "none"))]
    return Architecture::ArmCortexM;

    #[cfg(all(target_arch = "arm", not(target_os = "none")))]
    return Architecture::Arm32;

    #[cfg(target_arch = "x86")]
    return Architecture::X86;

    #[cfg(not(any(
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "x86",
    )))]
    return Architecture::X86_64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_architecture_detection() {
        let arch = detect_architecture();
        assert!(matches!(
            arch,
            Architecture::AArch64
                | Architecture::RiscV64
                | Architecture::X86_64
                | Architecture::ArmCortexM
                | Architecture::Arm32
                | Architecture::X86
        ));
    }

    #[test]
    fn test_architecture_display() {
        assert_eq!(Architecture::AArch64.to_string(), "aarch64");
        assert_eq!(Architecture::X86_64.to_string(), "x86_64");
        assert_eq!(Architecture::Arm32.to_string(), "armv7");
        assert_eq!(Architecture::X86.to_string(), "x86");
    }
}
