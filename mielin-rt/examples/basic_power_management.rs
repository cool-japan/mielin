//! Basic Power Management Example
//!
//! Demonstrates how to use MielinRT's power management features
//! to control system power modes and optimize energy consumption.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::Layout;

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

struct DummyAllocator;

unsafe impl core::alloc::GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

use mielin_rt::power::{AdvancedPowerManager, PerformanceLevel, PowerMode, WakeConfig, WakeSource};
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    // Initialize power manager
    let mut pm = AdvancedPowerManager::new();

    // Configure wake sources before entering sleep
    pm.wake_controller_mut()
        .add_source(WakeConfig::new(WakeSource::RtcAlarm).with_debounce(100));

    pm.wake_controller_mut()
        .add_source(WakeConfig::new(WakeSource::Gpio { port: 0, pin: 2 }));

    // Example 1: Manual power mode management
    example_manual_power_modes(&mut pm);

    // Example 2: Battery-aware power management
    example_battery_aware_power(&mut pm);

    // Example 3: Performance scaling
    example_performance_scaling(&mut pm);

    loop {
        cortex_m::asm::wfi();
    }
}

/// Example 1: Manual power mode transitions
fn example_manual_power_modes(pm: &mut AdvancedPowerManager) {
    // Start in normal mode
    assert_eq!(pm.mode(), PowerMode::Normal);

    // Reduce power during idle periods
    pm.set_mode(PowerMode::LowPower).unwrap();

    // Perform some low-priority work...
    simulate_work(100);

    // Enter deep sleep when no work is pending
    pm.set_mode(PowerMode::Sleep).unwrap();

    // (Wake up from interrupt would happen here)

    // Return to normal mode
    pm.set_mode(PowerMode::Normal).unwrap();

    // Use safe transitions for complex state changes
    pm.safe_transition_to(PowerMode::Standby).unwrap();
}

/// Example 2: Battery-aware power management
fn example_battery_aware_power(pm: &mut AdvancedPowerManager) {
    // Get power summary to check battery status
    let summary = pm.summary();

    // In a real application, you would check battery level here
    // For this example, we'll just demonstrate power scaling
    if summary.mode == PowerMode::Normal {
        // Reduce power consumption
        pm.safe_transition_to(PowerMode::UltraLowPower).unwrap();

        // Reduce DVFS level
        pm.dvfs_mut().set_level(PerformanceLevel::Minimum).unwrap();

        // Gate non-essential clocks
        pm.clock_gating_mut().gate_non_essential();
    } else {
        // Battery healthy, use normal power management
        pm.set_mode(PowerMode::Normal).unwrap();
    }
}

/// Example 3: Dynamic performance scaling
fn example_performance_scaling(pm: &mut AdvancedPowerManager) {
    // Scale up for high-priority work
    pm.dvfs_mut().set_level(PerformanceLevel::Maximum).unwrap();
    simulate_work(50); // CPU-intensive task

    // Scale down during I/O wait
    pm.dvfs_mut().set_level(PerformanceLevel::Low).unwrap();
    simulate_io_wait(100);

    // Return to balanced mode
    pm.dvfs_mut().set_level(PerformanceLevel::Balanced).unwrap();

    // Get power summary
    let summary = pm.summary();
    if summary.active_peripherals > 0 {
        // Cannot enter deep sleep with active peripherals
        pm.set_mode(PowerMode::LowPower).unwrap();
    } else {
        pm.set_mode(PowerMode::Sleep).unwrap();
    }
}

// Helper functions
fn simulate_work(iterations: u32) {
    for _ in 0..iterations {
        cortex_m::asm::nop();
    }
}

fn simulate_io_wait(_duration_ms: u32) {
    // In real code, this would be an actual I/O wait
    cortex_m::asm::wfi();
}
