//! Architecture-specific implementations

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

// Always include riscv64 module for testing and cross-platform support
pub mod riscv64;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(all(target_arch = "arm", target_os = "none"))]
pub mod cortex_m;
