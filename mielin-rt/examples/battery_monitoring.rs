//! Battery Monitoring Example
//!
//! Demonstrates how to use MielinRT's battery management features
//! including fuel gauge integration, health tracking, and calibration.

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

use mielin_rt::battery::{
    BatteryCalibration, BatteryChemistry, BatteryConfig, BatteryManager, FuelGaugeReading,
};
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    // Example 1: Basic battery monitoring
    example_basic_monitoring();

    // Example 2: Battery calibration
    example_battery_calibration();

    // Example 3: Health tracking
    example_health_tracking();

    // Example 4: Power consumption analysis
    example_power_analysis();

    loop {
        cortex_m::asm::wfi();
    }
}

/// Example 1: Basic battery monitoring
fn example_basic_monitoring() {
    // Configure for a single-cell LiPo battery (2000 mAh)
    let config = BatteryConfig::single_cell_lipo(2000);
    let mut manager = BatteryManager::new(config);

    // Simulated fuel gauge reading (in real code, read from actual hardware)
    let reading = FuelGaugeReading {
        soc_percent: 75,
        remaining_mah: 1500,
        full_capacity_mah: 2000,
        design_capacity_mah: 2000,
        voltage_mv: 3850,
        current_ma: -500,        // Discharging at 500mA
        temperature_deci_c: 250, // 25.0°C
        time_to_empty_min: 180,  // 3 hours
        time_to_full_min: 0,
    };

    // Update manager with reading
    let timestamp_us = get_timestamp_us();
    manager.update(reading, timestamp_us);

    // Check battery status
    if manager.is_low() {
        // Battery is low, take action
        enter_power_saving_mode();
    }

    if manager.is_critical() {
        // Battery critically low, save state and prepare for shutdown
        save_critical_state();
    }

    // Get battery summary
    let summary = manager.summary();
    print_battery_info(&summary);
}

/// Example 2: Battery calibration for accurate SoC
fn example_battery_calibration() {
    // Create calibration for LiPo chemistry
    let mut calibration = BatteryCalibration::new(BatteryChemistry::LithiumPolymer, 1);

    // Add custom calibration points if needed
    calibration.add_point(3750, 45); // 3750mV = 45% SoC

    // Enable temperature compensation (-3mV/°C per cell)
    calibration.set_temperature_compensation(-3.0, 250);

    // Use calibration to get SoC from voltage
    let voltage = 3700; // mV
    let temperature = 300; // 30°C in deci-Celsius
    let _soc = calibration.voltage_to_soc(voltage, temperature);

    // Or without temperature compensation
    let _soc_simple = calibration.voltage_to_soc_simple(voltage);

    // Validate calibration
    if let Err(e) = calibration.validate() {
        panic!("Invalid calibration: {}", e);
    }
}

/// Example 3: Battery health tracking
fn example_health_tracking() {
    let config = BatteryConfig::single_cell_lipo(2000);
    let mut manager = BatteryManager::new(config);

    // Simulate multiple readings over time
    for cycle in 0..100 {
        // Discharge cycle
        let discharge_reading = FuelGaugeReading {
            soc_percent: 10,
            voltage_mv: 3400,
            current_ma: -1000,
            design_capacity_mah: 2000,
            full_capacity_mah: 1950 - cycle, // Capacity degrades over time
            temperature_deci_c: 250,
            ..Default::default()
        };
        manager.update(discharge_reading, get_timestamp_us());

        // Charge cycle
        let charge_reading = FuelGaugeReading {
            soc_percent: 100,
            voltage_mv: 4200,
            current_ma: 500,
            design_capacity_mah: 2000,
            full_capacity_mah: 1950 - cycle,
            temperature_deci_c: 300, // Gets warmer during charging
            ..Default::default()
        };
        manager.update(charge_reading, get_timestamp_us());
    }

    // Check battery health
    let health = manager.health();
    match health.status {
        mielin_rt::battery::HealthStatus::Good => {
            // Battery in good condition
        }
        mielin_rt::battery::HealthStatus::Fair => {
            // Some degradation, monitor more closely
        }
        mielin_rt::battery::HealthStatus::Poor => {
            // Significant degradation, recommend replacement
        }
        mielin_rt::battery::HealthStatus::Critical => {
            // Battery needs immediate replacement
        }
        _ => {}
    }

    // Get remaining useful life estimate
    let remaining_life = health.remaining_life_percent();
    if remaining_life < 20 {
        // Battery near end of life
        notify_user_replace_battery();
    }
}

/// Example 4: Power consumption analysis
fn example_power_analysis() {
    let config = BatteryConfig::single_cell_lipo(2000);
    let mut manager = BatteryManager::new(config);

    // Record power consumption over time
    for i in 0u64..1000 {
        let reading = FuelGaugeReading {
            soc_percent: 100 - ((i / 10) as u8),
            voltage_mv: 3700,
            current_ma: -500 - ((i % 100) as i32), // Variable load
            ..Default::default()
        };

        manager.update(reading, get_timestamp_us() + i * 1000);
    }

    // Analyze power consumption
    let consumption = manager.consumption();
    let _average_power_mw = consumption.average_mw();
    let _peak_power_mw = consumption.peak_mw();
    let _total_energy_mj = consumption.total_energy_mj();

    // Estimate runtime at average power
    if let Some(_runtime_min) = manager.estimated_runtime_min() {
        // Display estimated runtime
    }

    // Reset statistics if needed
    manager.reset_consumption();
}

// Helper functions
fn get_timestamp_us() -> u64 {
    // In real code, read from hardware timer
    0
}

fn enter_power_saving_mode() {
    // Reduce system power consumption
}

fn save_critical_state() {
    // Save application state to non-volatile memory
}

fn notify_user_replace_battery() {
    // Alert user that battery needs replacement
}

fn print_battery_info(_summary: &mielin_rt::battery::BatterySummary) {
    // Print battery information (implementation depends on platform)
}
