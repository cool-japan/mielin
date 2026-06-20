//! Architecture-specific implementations

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

// Always include riscv64 module for testing and cross-platform support
pub mod riscv64;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(all(target_arch = "arm", target_os = "none"))]
pub mod cortex_m;

// Exotic architecture stubs — included unconditionally for compile-time testing
// and cross-platform development.
pub mod mips;
pub mod powerpc;

pub use mips::{MipsCacheInfo, MipsCapabilities, MipsEndianness, MipsPlatform, MipsVariant};
pub use powerpc::{
    AltiVecSupport, PowerPcCacheInfo, PowerPcCapabilities, PowerPcEndianness, PowerPcPlatform,
    PowerPcVariant,
};

// Historical 32-bit architectures — included unconditionally for compile-time
// testing and cross-platform development.
pub mod armv7;
pub mod x86;

pub use armv7::{Armv7Capabilities, detect_armv7_capabilities};
pub use x86::{X86Capabilities, detect_x86_32_capabilities};
