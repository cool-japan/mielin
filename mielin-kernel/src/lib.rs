#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![cfg_attr(not(any(test, feature = "std")), no_main)]

//! MielinOS Kernel - The Myelin Sheath
//!
//! Core unikernel implementation providing the foundation for agent execution
//! across heterogeneous hardware platforms (Arm, RISC-V, x86).

extern crate alloc;

pub mod boot;
pub mod config;
pub mod embedded;
pub mod executor;
pub mod hardening;
pub mod heap;
pub mod interrupt;
pub mod ipc;
pub mod lockfree;
pub mod memory;
pub mod numa;
pub mod observability;
pub mod percpu;
pub mod pool;
pub mod power;
pub mod rt;
pub mod scheduler;
pub mod tensor;
pub mod timer;
pub mod vmm;
pub mod work_stealing;

#[cfg(all(not(test), feature = "bootable"))]
use bootloader::{entry_point, BootInfo};
#[cfg(not(any(test, feature = "std")))]
use core::panic::PanicInfo;

#[cfg(all(not(test), feature = "bootable"))]
entry_point!(kernel_main);

/// Kernel entry point called by the bootloader
#[cfg(all(not(test), feature = "bootable"))]
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Initialize the kernel
    match kernel_init(boot_info) {
        Ok(_) => {
            // Kernel successfully initialized
            // In the future, this will start the agent runtime
        }
        Err(e) => {
            // Kernel initialization failed
            let _ = e;
        }
    }

    // Halt the CPU - in a real OS, this would enter the main loop
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("hlt");
        }
        #[cfg(not(target_arch = "x86_64"))]
        core::hint::spin_loop();
    }
}

#[cfg(all(not(test), not(feature = "std"), not(doc)))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        // In production, this would go to a serial port or logging facility
        let _ = (info.message(), location.file(), location.line());
    }

    loop {
        core::hint::spin_loop();
    }
}

/// Kernel initialization entry point (with bootloader)
#[cfg(all(not(test), feature = "bootable"))]
pub fn kernel_init(boot_info: &'static bootloader::BootInfo) -> Result<(), KernelError> {
    // Boot sequence initialization
    boot::init(boot_info)?;

    // Initialize memory management
    memory::init()?;

    // Initialize scheduler
    scheduler::init()?;

    // Initialize interrupt subsystem
    interrupt::init().map_err(|_| KernelError::HardwareNotSupported)?;

    // Initialize timer subsystem
    timer::init(timer::TimerConfig::default()).map_err(|_| KernelError::HardwareNotSupported)?;

    Ok(())
}

/// Kernel initialization entry point (without bootloader, non-test builds)
#[cfg(all(not(test), not(feature = "bootable")))]
pub fn kernel_init() -> Result<(), KernelError> {
    // Boot sequence initialization (no bootloader)
    boot::init()?;

    // Initialize memory management
    memory::init()?;

    // Initialize scheduler
    scheduler::init()?;

    // Initialize interrupt subsystem
    interrupt::init().map_err(|_| KernelError::HardwareNotSupported)?;

    // Initialize timer subsystem
    timer::init(timer::TimerConfig::default()).map_err(|_| KernelError::HardwareNotSupported)?;

    Ok(())
}

/// Kernel initialization for testing (without bootloader)
#[cfg(test)]
pub fn kernel_init() -> Result<(), KernelError> {
    // Initialize memory management
    memory::init()?;

    // Initialize pool allocator
    pool::init();

    // Initialize scheduler
    scheduler::init()?;

    // Initialize interrupt subsystem
    interrupt::init().map_err(|_| KernelError::HardwareNotSupported)?;

    // Initialize timer subsystem
    timer::init(timer::TimerConfig::default()).map_err(|_| KernelError::HardwareNotSupported)?;

    Ok(())
}

/// Kernel errors with detailed variants for specific failure modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    /// Memory subsystem initialization failed
    MemoryInitFailed,
    /// Scheduler subsystem initialization failed
    SchedulerInitFailed,
    /// Hardware is not supported on this platform
    HardwareNotSupported,
    /// Attempted to allocate zero pages
    ZeroAllocation,
    /// Requested allocation exceeds maximum pages
    AllocationTooLarge {
        /// Requested page count
        requested: usize,
        /// Maximum allowed pages
        max_pages: usize,
    },
    /// Address is out of valid memory bounds
    AddressOutOfBounds {
        /// The invalid address
        address: usize,
        /// Maximum valid address
        max_address: usize,
    },
    /// Page index is out of bounds
    PageIndexOutOfBounds {
        /// The invalid page index
        index: usize,
        /// Maximum valid page index
        max_index: usize,
    },
    /// Attempted to free a page that is not allocated
    DoubleFree {
        /// The address that was already freed
        address: usize,
    },
    /// Memory allocation failed (out of memory)
    OutOfMemory {
        /// Requested page count
        requested: usize,
        /// Available free pages
        available: usize,
    },
    /// Invalid page count for operation
    InvalidPageCount,
    /// Task spawn failed
    TaskSpawnFailed,
    /// Task not found
    TaskNotFound {
        /// The task ID that was not found
        task_id: usize,
    },
    /// Scheduler is not initialized
    SchedulerNotInitialized,
    /// Memory manager is not initialized
    MemoryNotInitialized,
}

#[cfg(feature = "std")]
impl std::error::Error for KernelError {}

impl core::fmt::Display for KernelError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MemoryInitFailed => write!(f, "memory initialization failed"),
            Self::SchedulerInitFailed => write!(f, "scheduler initialization failed"),
            Self::HardwareNotSupported => write!(f, "hardware not supported"),
            Self::ZeroAllocation => write!(f, "cannot allocate zero pages"),
            Self::AllocationTooLarge {
                requested,
                max_pages,
            } => {
                write!(
                    f,
                    "allocation too large: requested {} pages, max {}",
                    requested, max_pages
                )
            }
            Self::AddressOutOfBounds {
                address,
                max_address,
            } => {
                write!(
                    f,
                    "address out of bounds: {:#x} exceeds max {:#x}",
                    address, max_address
                )
            }
            Self::PageIndexOutOfBounds { index, max_index } => {
                write!(
                    f,
                    "page index out of bounds: {} exceeds max {}",
                    index, max_index
                )
            }
            Self::DoubleFree { address } => {
                write!(f, "double free: address {:#x} is not allocated", address)
            }
            Self::OutOfMemory {
                requested,
                available,
            } => {
                write!(
                    f,
                    "out of memory: requested {} pages, only {} available",
                    requested, available
                )
            }
            Self::InvalidPageCount => write!(f, "invalid page count"),
            Self::TaskSpawnFailed => write!(f, "task spawn failed"),
            Self::TaskNotFound { task_id } => write!(f, "task not found: {}", task_id),
            Self::SchedulerNotInitialized => write!(f, "scheduler not initialized"),
            Self::MemoryNotInitialized => write!(f, "memory manager not initialized"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_error_types() {
        let _ = KernelError::MemoryInitFailed;
    }
}
