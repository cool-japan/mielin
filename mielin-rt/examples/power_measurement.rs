//! Power Measurement Example
//!
//! Demonstrates comprehensive power consumption monitoring using the
//! measurement module. Shows how to:
//! - Configure power sensors
//! - Record real-time power measurements
//! - Detect power consumption anomalies
//! - Monitor multiple power rails
//! - Track energy consumption over time
//!
//! This example simulates an embedded system with multiple power rails
//! and demonstrates best practices for power monitoring.

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

use panic_halt as _;

use mielin_rt::measurement::{
    AnomalyDetector, MeasurementConfig, MultiRailMonitor, PowerMeasurement, PowerMonitor,
    SensorConfig, SensorType,
};

#[cortex_m_rt::entry]
fn main() -> ! {
    // Example 1: Basic power monitoring
    basic_power_monitoring();

    // Example 2: Anomaly detection
    anomaly_detection_example();

    // Example 3: Multi-rail monitoring
    multi_rail_monitoring();

    // Example 4: Energy tracking
    energy_tracking_example();

    loop {
        cortex_m::asm::wfi();
    }
}

/// Example 1: Basic power monitoring with a single sensor
fn basic_power_monitoring() {
    // Configure measurement system
    let config = MeasurementConfig::default()
        .with_sample_rate_hz(100) // Sample at 100 Hz
        .with_averaging(8) // Average 8 samples
        .with_history_size(1000); // Keep 1000 measurements

    let mut monitor = PowerMonitor::new(config);

    // Simulate power measurements over 10 seconds
    let mut timestamp_us = 0u64;
    for i in 0..1000 {
        // Simulate reading from a power sensor (e.g., INA219)
        // In real hardware, this would be actual sensor readings
        let voltage_mv = 3300 + ((i % 100) - 50) as u32; // 3.25V - 3.35V
        let current_ma = 150 + (i % 50); // 150-200 mA

        let measurement = PowerMeasurement::new(voltage_mv, current_ma, timestamp_us);

        // Record measurement (returns anomaly if detected)
        if let Some(anomaly) = monitor.record(measurement) {
            // Handle anomaly (in real system, might log or trigger alert)
            handle_anomaly(anomaly);
        }

        timestamp_us += 10_000; // 10ms between samples
    }

    // Get statistics
    let stats = monitor.statistics();

    // In a real system, you might:
    // - Display statistics on a screen
    // - Send to a logging system
    // - Use for adaptive power management
    process_statistics(stats);
}

/// Example 2: Power anomaly detection
fn anomaly_detection_example() {
    // Create detector with 30% threshold
    let mut detector = AnomalyDetector::new(30);

    // Configure expected ranges
    detector.set_voltage_range(3000, 3600); // 3.0V - 3.6V (Li-Ion range)
    detector.set_current_max(2000); // Maximum 2A

    // Establish baseline with normal operation
    for i in 0..50 {
        let normal_measurement = PowerMeasurement::new(3300, 150, i * 10_000);
        detector.update_baseline(normal_measurement.power_mw());
    }

    // Now monitor for anomalies
    let test_cases = [
        // Normal operation
        PowerMeasurement::new(3300, 150, 500_000),
        // Voltage drop (battery depleting)
        PowerMeasurement::new(2800, 150, 510_000),
        // Current spike (motor start)
        PowerMeasurement::new(3300, 800, 520_000),
        // Power spike (peripheral activation)
        PowerMeasurement::new(3300, 500, 530_000),
        // Back to normal
        PowerMeasurement::new(3300, 150, 540_000),
    ];

    for measurement in test_cases.iter() {
        if let Some(anomaly) = detector.check(measurement) {
            // Handle different anomaly types
            match anomaly {
                mielin_rt::measurement::PowerAnomaly::Spike { .. } => {
                    // Power spike detected - might indicate:
                    // - Motor startup
                    // - Radio transmission
                    // - Peripheral activation
                    handle_power_spike(measurement);
                }
                mielin_rt::measurement::PowerAnomaly::Drop { .. } => {
                    // Power drop detected - might indicate:
                    // - System entering low power mode
                    // - Peripheral shutdown
                    handle_power_drop(measurement);
                }
                mielin_rt::measurement::PowerAnomaly::VoltageOutOfRange { .. } => {
                    // Voltage out of range - might indicate:
                    // - Battery critically low
                    // - Power supply issue
                    // - Brownout condition
                    handle_voltage_anomaly(measurement);
                }
                mielin_rt::measurement::PowerAnomaly::CurrentOutOfRange { .. } => {
                    // Current out of range - might indicate:
                    // - Short circuit
                    // - Overload condition
                    handle_current_anomaly(measurement);
                }
            }
        }
    }
}

/// Example 3: Multi-rail power monitoring
fn multi_rail_monitoring() {
    let mut monitor = MultiRailMonitor::new();

    // Add power rails for different system components
    let core_rail = monitor
        .add_rail(
            "VDD_CORE",
            MeasurementConfig::default()
                .with_sample_rate_hz(100)
                .with_averaging(4),
        )
        .expect("Failed to add core rail");

    let io_rail = monitor
        .add_rail(
            "VDD_IO",
            MeasurementConfig::default()
                .with_sample_rate_hz(100)
                .with_averaging(4),
        )
        .expect("Failed to add I/O rail");

    let radio_rail = monitor
        .add_rail(
            "VDD_RADIO",
            MeasurementConfig::default()
                .with_sample_rate_hz(1000) // Higher rate for radio
                .with_averaging(1),
        )
        .expect("Failed to add radio rail");

    // Simulate measurements from different rails
    for i in 0..100 {
        let timestamp = i * 10_000;

        // Core voltage: 1.2V, steady current
        monitor.record(
            core_rail,
            PowerMeasurement::new(1200, 200 + (i % 20) as i32, timestamp),
        );

        // I/O voltage: 3.3V, moderate current
        monitor.record(
            io_rail,
            PowerMeasurement::new(3300, 100 + (i % 30) as i32, timestamp),
        );

        // Radio voltage: 3.3V, bursty current (simulating TX/RX)
        let radio_current = if i % 10 < 5 {
            50 // RX/idle
        } else {
            500 // TX burst
        };
        monitor.record(
            radio_rail,
            PowerMeasurement::new(3300, radio_current, timestamp),
        );
    }

    // Analyze per-rail statistics
    if let Some(core_stats) = monitor.rail_statistics(core_rail) {
        // Core rail analysis
        analyze_rail_statistics("VDD_CORE", core_stats);
    }

    if let Some(io_stats) = monitor.rail_statistics(io_rail) {
        // I/O rail analysis
        analyze_rail_statistics("VDD_IO", io_stats);
    }

    if let Some(radio_stats) = monitor.rail_statistics(radio_rail) {
        // Radio rail analysis - expect high variance
        analyze_rail_statistics("VDD_RADIO", radio_stats);
    }

    // Get total system power
    let total_power_mw = monitor.total_power_mw();
    let total_energy = monitor.total_energy();

    // Use for battery life estimation
    estimate_battery_life(total_power_mw, total_energy);
}

/// Example 4: Energy tracking over time
fn energy_tracking_example() {
    let config = MeasurementConfig::default()
        .with_sample_rate_hz(100)
        .with_averaging(1)
        .with_history_size(1000);

    let mut monitor = PowerMonitor::new(config);

    // Simulate different operational phases
    simulate_operational_phases(&mut monitor);

    // Analyze energy consumption by phase
    let stats = monitor.statistics();

    // Calculate runtime estimates
    let avg_power_mw = stats.average_power_mw();
    let battery_capacity_mah = 2000; // 2000 mAh battery
    let nominal_voltage_mv = 3700; // 3.7V nominal

    // Battery energy in milliwatt-hours
    let battery_energy_mwh = battery_capacity_mah * nominal_voltage_mv / 1000;

    // Estimated runtime in hours
    let estimated_runtime_h = if avg_power_mw > 0 {
        battery_energy_mwh / avg_power_mw
    } else {
        0
    };

    // In real system, display or log this information
    display_energy_report(stats, estimated_runtime_h);
}

/// Simulate different operational phases with varying power consumption
fn simulate_operational_phases(monitor: &mut PowerMonitor) {
    let mut timestamp = 0u64;

    // Phase 1: Idle (100ms)
    for _ in 0..10 {
        monitor.record(PowerMeasurement::new(3300, 50, timestamp));
        timestamp += 10_000;
    }

    // Phase 2: Active processing (200ms)
    for _ in 0..20 {
        monitor.record(PowerMeasurement::new(3300, 200, timestamp));
        timestamp += 10_000;
    }

    // Phase 3: Radio transmission (100ms)
    for _ in 0..10 {
        monitor.record(PowerMeasurement::new(3300, 500, timestamp));
        timestamp += 10_000;
    }

    // Phase 4: Sleep (300ms)
    for _ in 0..30 {
        monitor.record(PowerMeasurement::new(3300, 10, timestamp));
        timestamp += 10_000;
    }
}

// Helper functions (would contain actual implementation in real system)

fn handle_anomaly(_anomaly: mielin_rt::measurement::PowerAnomaly) {
    // In real system:
    // - Log to flash memory
    // - Trigger LED indicator
    // - Send alert via radio
    // - Adjust power management
}

fn process_statistics(_stats: &mielin_rt::measurement::PowerStatistics) {
    // In real system:
    // - Display on screen
    // - Send to cloud
    // - Store in flash
    // - Use for optimization
}

fn handle_power_spike(_measurement: &PowerMeasurement) {
    // Verify spike is expected (e.g., scheduled radio TX)
    // If unexpected, may indicate hardware issue
}

fn handle_power_drop(_measurement: &PowerMeasurement) {
    // Check if intentional (low power mode)
    // If unexpected, may indicate peripheral failure
}

fn handle_voltage_anomaly(_measurement: &PowerMeasurement) {
    // Critical: may need to save state and shutdown
    // Or switch to emergency power mode
}

fn handle_current_anomaly(_measurement: &PowerMeasurement) {
    // Very critical: possible short circuit
    // Should disable affected peripherals
}

fn analyze_rail_statistics(_rail_name: &str, _stats: &mielin_rt::measurement::PowerStatistics) {
    // Analyze per-rail consumption
    // Identify optimization opportunities
    // Detect abnormal behavior
}

fn estimate_battery_life(_total_power_mw: u32, _total_energy: mielin_rt::energy::Energy) {
    // Calculate remaining runtime
    // Adjust for battery chemistry
    // Account for temperature effects
}

fn display_energy_report(
    _stats: &mielin_rt::measurement::PowerStatistics,
    _estimated_runtime_h: u32,
) {
    // Format and display energy report
    // Show average, min, max power
    // Show total energy consumed
    // Show estimated runtime
}

// Sensor abstraction example
struct SimulatedSensor {
    config: SensorConfig,
}

impl SimulatedSensor {
    fn new() -> Self {
        Self {
            config: SensorConfig {
                sensor_type: SensorType::INA219,
                voltage_range_mv: 26000, // INA219: up to 26V
                current_range_ma: 3200,  // 3.2A max with 0.1 ohm shunt
                resolution_bits: 12,
                shunt_resistor_uohm: 100_000, // 0.1 ohm
            },
        }
    }
}

// In a real implementation, you would implement the PowerSensor trait:
// impl PowerSensor for SimulatedSensor {
//     type Error = ();
//
//     fn read_voltage_mv(&mut self) -> Result<u32, Self::Error> {
//         // Read from I2C/SPI sensor
//         Ok(3300)
//     }
//
//     fn read_current_ma(&mut self) -> Result<i32, Self::Error> {
//         // Read from I2C/SPI sensor
//         Ok(150)
//     }
//
//     fn reset(&mut self) -> Result<(), Self::Error> {
//         // Reset sensor
//         Ok(())
//     }
//
//     fn config(&self) -> SensorConfig {
//         self.config
//     }
// }
