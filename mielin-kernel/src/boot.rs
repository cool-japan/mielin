//! Boot sequence and initialization
//!
//! This module handles the early boot process after the bootloader
//! transfers control to the kernel.

use crate::KernelError;

#[cfg(all(not(test), feature = "bootable"))]
use bootloader::BootInfo;

/// Initialize boot-time hardware and prepare kernel environment (with bootloader)
#[cfg(all(not(test), feature = "bootable"))]
pub fn init(boot_info: &'static BootInfo) -> Result<(), KernelError> {
    // Extract memory map from bootloader
    let _memory_map = &boot_info.memory_map;

    // Get physical memory offset for accessing physical memory
    let _phys_mem_offset = boot_info.physical_memory_offset;

    // Future:
    // - Parse memory map and set up page tables
    // - Initialize serial port for logging
    // - Detect CPU features and capabilities
    // - Set up interrupt handlers

    Ok(())
}

/// Initialize boot-time (no bootloader, non-test builds)
#[cfg(all(not(test), not(feature = "bootable")))]
pub fn init() -> Result<(), KernelError> {
    // Minimal initialization without bootloader
    Ok(())
}

/// Test-only initialization (no bootloader)
#[cfg(test)]
pub fn init() -> Result<(), KernelError> {
    Ok(())
}
