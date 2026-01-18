//! Cortex-M specific runtime implementation
//!
//! Provides low-level runtime support for ARM Cortex-M microcontrollers.
//! Includes power management, interrupt handling, and memory management.

#![cfg(target_arch = "arm")]

use cortex_m::peripheral::{scb, syst};
use cortex_m::register::control;

/// Cortex-M runtime configuration
pub struct CortexMRuntime {
    power_mode: PowerMode,
    systick_hz: u32,
}

/// Power modes for Cortex-M
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// Normal operation, all peripherals active
    Run,
    /// Sleep mode - CPU clock stopped, peripherals running
    /// Wakes on any interrupt
    Sleep,
    /// Deep sleep mode - CPU and most peripherals stopped
    /// Wakes on specific interrupts only
    DeepSleep,
}

impl CortexMRuntime {
    /// Create a new Cortex-M runtime
    pub fn new() -> Self {
        Self {
            power_mode: PowerMode::Run,
            systick_hz: 1000, // 1ms tick by default
        }
    }

    /// Initialize the runtime
    pub fn init(&mut self) -> Result<(), RuntimeError> {
        // In a real implementation, this would:
        // 1. Configure system clocks
        // 2. Initialize SysTick timer
        // 3. Set up interrupt priorities
        // 4. Configure power management
        Ok(())
    }

    /// Enter sleep mode (WFI - Wait For Interrupt)
    ///
    /// # Safety
    ///
    /// Safe to call, but should only be used when the system is ready to sleep.
    /// Interrupts must be enabled for the CPU to wake up.
    #[inline]
    pub fn sleep(&self) {
        #[cfg(target_arch = "arm")]
        {
            cortex_m::asm::wfi();
        }
        #[cfg(not(target_arch = "arm"))]
        {
            // No-op on non-ARM platforms for testing
        }
    }

    /// Enter deep sleep mode
    ///
    /// # Safety
    ///
    /// Requires proper configuration of wake-up sources before calling.
    pub fn deep_sleep(&mut self) -> Result<(), RuntimeError> {
        #[cfg(target_arch = "arm")]
        {
            // Enable deep sleep mode in SCB
            unsafe {
                let scb = &*cortex_m::peripheral::SCB::PTR;
                scb.set_sleepdeep();
            }
            cortex_m::asm::wfi();

            // Clear deep sleep flag on wake
            unsafe {
                let scb = &*cortex_m::peripheral::SCB::PTR;
                scb.clear_sleepdeep();
            }
        }
        self.power_mode = PowerMode::Run;
        Ok(())
    }

    /// Wait for event (WFE)
    #[inline]
    pub fn wait_for_event(&self) {
        #[cfg(target_arch = "arm")]
        {
            cortex_m::asm::wfe();
        }
    }

    /// Send event (SEV)
    #[inline]
    pub fn send_event(&self) {
        #[cfg(target_arch = "arm")]
        {
            cortex_m::asm::sev();
        }
    }

    /// Set power mode
    pub fn set_power_mode(&mut self, mode: PowerMode) {
        self.power_mode = mode;
    }

    /// Get current power mode
    pub fn power_mode(&self) -> PowerMode {
        self.power_mode
    }

    /// Configure SysTick timer
    ///
    /// # Arguments
    ///
    /// * `reload` - Reload value for SysTick counter
    /// * `enable_interrupt` - Enable SysTick interrupt
    pub fn configure_systick(&mut self, reload: u32, enable_interrupt: bool) {
        self.systick_hz = reload;
        // In real implementation, would configure SysTick peripheral
    }

    /// Get milliseconds since boot (if SysTick is configured)
    pub fn millis(&self) -> u32 {
        // In real implementation, would read SysTick counter
        // For now, return placeholder
        0
    }
}

impl Default for CortexMRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// Initialization failed
    InitFailed,
    /// Invalid configuration
    InvalidConfig,
    /// Power management error
    PowerError,
}

/// Memory pool for embedded allocations
///
/// Provides a simple bump allocator for embedded systems without heap.
pub struct MemoryPool<const SIZE: usize> {
    buffer: [u8; SIZE],
    offset: usize,
}

impl<const SIZE: usize> MemoryPool<SIZE> {
    /// Create a new memory pool
    pub const fn new() -> Self {
        Self {
            buffer: [0; SIZE],
            offset: 0,
        }
    }

    /// Allocate bytes from the pool
    ///
    /// Returns None if insufficient space
    pub fn allocate(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        // Align offset
        let aligned_offset = (self.offset + align - 1) & !(align - 1);

        // Check if we have space
        if aligned_offset + size > SIZE {
            return None;
        }

        let ptr = unsafe { self.buffer.as_mut_ptr().add(aligned_offset) };
        self.offset = aligned_offset + size;
        Some(ptr)
    }

    /// Reset the pool (free all allocations)
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Get remaining space
    pub fn available(&self) -> usize {
        SIZE.saturating_sub(self.offset)
    }

    /// Get used space
    pub fn used(&self) -> usize {
        self.offset
    }
}

/// Interrupt priority levels (0 = highest, 255 = lowest)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(pub u8);

impl Priority {
    pub const HIGHEST: Priority = Priority(0);
    pub const HIGH: Priority = Priority(64);
    pub const MEDIUM: Priority = Priority(128);
    pub const LOW: Priority = Priority(192);
    pub const LOWEST: Priority = Priority(255);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let rt = CortexMRuntime::new();
        assert_eq!(rt.power_mode(), PowerMode::Run);
    }

    #[test]
    fn test_power_mode_transition() {
        let mut rt = CortexMRuntime::new();
        rt.set_power_mode(PowerMode::Sleep);
        assert_eq!(rt.power_mode(), PowerMode::Sleep);

        rt.set_power_mode(PowerMode::DeepSleep);
        assert_eq!(rt.power_mode(), PowerMode::DeepSleep);
    }

    #[test]
    fn test_memory_pool() {
        let mut pool = MemoryPool::<1024>::new();

        assert_eq!(pool.available(), 1024);
        assert_eq!(pool.used(), 0);

        // Allocate 64 bytes with 4-byte alignment
        let ptr1 = pool.allocate(64, 4);
        assert!(ptr1.is_some());
        assert_eq!(pool.used(), 64);
        assert_eq!(pool.available(), 1024 - 64);

        // Allocate another 128 bytes with 8-byte alignment
        let ptr2 = pool.allocate(128, 8);
        assert!(ptr2.is_some());

        // Check alignment
        let offset = pool.used();
        assert!(offset >= 64 + 128);

        // Reset pool
        pool.reset();
        assert_eq!(pool.used(), 0);
        assert_eq!(pool.available(), 1024);
    }

    #[test]
    fn test_memory_pool_exhaustion() {
        let mut pool = MemoryPool::<128>::new();

        // Allocate most of the space
        let ptr1 = pool.allocate(100, 1);
        assert!(ptr1.is_some());

        // This should fail
        let ptr2 = pool.allocate(100, 1);
        assert!(ptr2.is_none());

        // But smaller allocation should work
        let ptr3 = pool.allocate(20, 1);
        assert!(ptr3.is_some());
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::HIGHEST < Priority::HIGH);
        assert!(Priority::HIGH < Priority::MEDIUM);
        assert!(Priority::MEDIUM < Priority::LOW);
        assert!(Priority::LOW < Priority::LOWEST);
    }
}
