//! MielinRT - Embedded Runtime Profile
//!
//! Lightweight runtime for Cortex-M and other embedded targets.
//! Enables MielinOS to run on resource-constrained IoT devices.
//!
//! ## Features
//!
//! - **Low Power Management**: Sleep, deep sleep, and power mode transitions
//! - **Memory Pool**: Bump allocator for no-heap embedded systems
//! - **Cortex-M Support**: WFI, WFE, SysTick, interrupt priorities
//! - **Agent Execution**: Minimal overhead for WASM agent hosting
//! - **Battery-Aware**: Automatic migration triggers on low battery
//! - **Energy Profiling**: Per-task energy tracking and budget management
//! - **Power Measurement**: Hardware-level power consumption monitoring
//! - **Stack Management**: Overflow detection and usage monitoring
//! - **Real-Time Scheduling**: Priority inheritance, EDF, deadline scheduling
//! - **Peripheral Drivers**: GPIO, I2C, SPI, UART, ADC/DAC abstractions
//! - **Communication Protocols**: CoAP, MQTT-SN, BLE, LoRaWAN for IoT connectivity
//! - **Sensor Integration**: Temperature, humidity, motion, environmental sensors with fusion algorithms
//! - **Security**: Secure boot, encrypted firmware updates, secure element integration, hardware crypto acceleration
//! - **OTA Updates**: Over-the-air firmware updates with A/B partitioning and rollback
//! - **Watchdog**: Independent and window watchdog support with task monitoring
//! - **Fault Recovery**: Automatic fault detection, logging, and recovery strategies
//! - **Remote Monitoring**: Real-time system health, performance metrics, and alerting
//!
//! ## Quick Start Example
//!
//! ```rust,no_run
//! use mielin_rt::{EmbeddedRuntime, power::PowerMode};
//!
//! let mut runtime = EmbeddedRuntime::new();
//! if let Err(e) = runtime.init() {
//!     // Handle initialization error
//!     panic!("Failed to initialize runtime: {:?}", e);
//! }
//!
//! // Enter low power mode
//! runtime.set_power_mode(PowerMode::LowPower);
//! ```
//!
//! ## Architecture
//!
//! MielinRT is organized into the following major modules:
//!
//! ### Power & Energy Management
//! - [`power`]: Power mode management and battery monitoring
//! - [`energy`]: Energy profiling and budget management
//! - [`measurement`]: Hardware power measurement and monitoring
//!
//! ### Memory Management
//! - [`pool`]: Memory pool allocator for no-heap systems
//! - [`stack`]: Stack overflow detection and usage monitoring
//!
//! ### Real-Time & Scheduling
//! - [`realtime`]: Real-time task scheduling (EDF, Rate Monotonic, priority inheritance)
//! - [`interrupt`]: Interrupt management and event queuing
//!
//! ### Peripheral Drivers
//! - [`gpio`]: GPIO pin control and port operations
//! - [`i2c`]: I2C master/slave communication
//! - [`spi`]: SPI master/slave communication
//! - [`uart`]: UART serial communication
//! - [`adc`]: ADC/DAC analog conversion
//!
//! ### Communication Protocols
//! - [`coap`]: Constrained Application Protocol (RFC 7252)
//! - [`mqttsn`]: MQTT for Sensor Networks
//! - [`ble`]: Bluetooth Low Energy (GAP/GATT)
//! - [`lorawan`]: LoRaWAN long-range IoT protocol
//!
//! ### Sensors & Fusion
//! - [`sensors`]: Temperature, humidity, motion, environmental sensors
//!
//! ### Security & Updates
//! - [`security`]: Secure boot, encrypted updates, secure element integration
//! - [`ota`]: Over-the-air firmware updates with A/B partitioning
//!
//! ### System Reliability
//! - [`watchdog`]: Watchdog timer management
//! - [`fault`]: Fault detection and recovery
//! - [`monitoring`]: Remote system monitoring and alerting
//!
//! ### Configuration
//! - [`config`]: Runtime configuration presets

#![no_std]

extern crate alloc;

pub mod adc;
pub mod agent;
pub mod battery;
pub mod ble;
/// Cortex-M A/B partition bootloader with anti-rollback and trial/confirm semantics.
pub mod bootloader;
pub mod coap;
pub mod config;
pub mod energy;
pub mod energy_scheduler;
pub mod fault;
pub mod gpio;
pub mod i2c;
pub mod interrupt;
pub mod lorawan;
pub mod measurement;
pub mod monitoring;
pub mod mqttsn;
pub mod ota;
pub mod pool;
pub mod power;
pub mod realtime;
pub mod security;
pub mod sensors;
pub mod spi;
pub mod stack;
pub mod uart;
pub mod watchdog;

#[cfg(target_arch = "arm")]
pub mod cortex_m;

/// ARMv8-M (Cortex-M23/M33/M55) runtime with TrustZone and SAU support.
pub mod armv8m;
/// RISC-V IMAC and GC runtime support (CLINT, PLIC, rv32imac, rv64imac).
pub mod riscv;

pub use armv8m::{ArmV8mError, ArmV8mMpu, ArmV8mRuntime, ArmV8mVariant, MpuAttributes, TrustZoneState};
pub use riscv::{ClintConfig, InterruptCause, PlicConfig, PrivilegeLevel, RiscvError, RiscvRuntime, RiscvVariant};

use mielin_hal::Architecture;

/// Main embedded runtime structure
///
/// Coordinates power management, agent execution, and system resources
/// for embedded devices.
#[derive(Debug)]
pub struct EmbeddedRuntime {
    arch: Architecture,
    power_mode: power::PowerMode,
    battery: Option<power::BatteryStatus>,
    power_monitor: Option<measurement::PowerMonitor>,
}

impl EmbeddedRuntime {
    /// Create a new embedded runtime instance
    pub fn new() -> Self {
        Self {
            arch: mielin_hal::detect_architecture(),
            power_mode: power::PowerMode::Normal,
            battery: None,
            power_monitor: None,
        }
    }

    /// Create a new embedded runtime with power monitoring enabled
    pub fn with_power_monitoring(config: measurement::MeasurementConfig) -> Self {
        Self {
            arch: mielin_hal::detect_architecture(),
            power_mode: power::PowerMode::Normal,
            battery: None,
            power_monitor: Some(measurement::PowerMonitor::new(config)),
        }
    }

    /// Create a new embedded runtime from a configuration
    pub fn from_config(config: config::RuntimeConfig) -> Result<Self, RuntimeError> {
        config.validate().map_err(|_| RuntimeError::InvalidConfig)?;

        let power_monitor = config
            .measurement_config()
            .map(measurement::PowerMonitor::new);

        Ok(Self {
            arch: mielin_hal::detect_architecture(),
            power_mode: config.power.default_mode,
            battery: None,
            power_monitor,
        })
    }

    /// Create a new embedded runtime from a preset configuration
    pub fn from_preset(preset: config::ConfigPreset) -> Result<Self, RuntimeError> {
        Self::from_config(config::RuntimeConfig::preset(preset))
    }

    /// Get the detected architecture
    pub fn architecture(&self) -> Architecture {
        self.arch
    }

    /// Initialize the runtime
    ///
    /// Performs platform-specific initialization including:
    /// - Clock configuration
    /// - Power management setup
    /// - Interrupt controller initialization
    pub fn init(&mut self) -> Result<(), RuntimeError> {
        // Platform-specific initialization would go here
        Ok(())
    }

    /// Set power mode
    pub fn set_power_mode(&mut self, mode: power::PowerMode) {
        self.power_mode = mode;
    }

    /// Get current power mode
    pub fn power_mode(&self) -> power::PowerMode {
        self.power_mode
    }

    /// Update battery status
    pub fn update_battery(&mut self, status: power::BatteryStatus) {
        self.battery = Some(status);
    }

    /// Check if agent should migrate due to low battery
    pub fn should_migrate(&self) -> bool {
        self.battery
            .as_ref()
            .map(|b| b.should_migrate())
            .unwrap_or(false)
    }

    /// Get battery level percentage (0-100)
    pub fn battery_level(&self) -> Option<u8> {
        self.battery.as_ref().map(|b| b.level_percent)
    }

    /// Enter low power mode appropriate for current architecture
    ///
    /// On Cortex-M, this uses WFI (Wait For Interrupt).
    /// On other platforms, this is a no-op.
    pub fn low_power_wait(&self) {
        #[cfg(target_arch = "arm")]
        {
            cortex_m::asm::wfi();
        }
    }

    /// Enable power monitoring with the specified configuration
    pub fn enable_power_monitoring(&mut self, config: measurement::MeasurementConfig) {
        self.power_monitor = Some(measurement::PowerMonitor::new(config));
    }

    /// Disable power monitoring
    pub fn disable_power_monitoring(&mut self) {
        self.power_monitor = None;
    }

    /// Record a power measurement
    ///
    /// Returns an anomaly if one is detected, or None if measurement is normal
    pub fn record_power_measurement(
        &mut self,
        measurement: measurement::PowerMeasurement,
    ) -> Option<measurement::PowerAnomaly> {
        self.power_monitor
            .as_mut()
            .and_then(|m| m.record(measurement))
    }

    /// Get power statistics
    ///
    /// Returns None if power monitoring is not enabled
    pub fn power_statistics(&self) -> Option<&measurement::PowerStatistics> {
        self.power_monitor.as_ref().map(|m| m.statistics())
    }

    /// Get mutable reference to power monitor
    ///
    /// Returns None if power monitoring is not enabled
    pub fn power_monitor_mut(&mut self) -> Option<&mut measurement::PowerMonitor> {
        self.power_monitor.as_mut()
    }

    /// Get reference to power monitor
    ///
    /// Returns None if power monitoring is not enabled
    pub fn power_monitor(&self) -> Option<&measurement::PowerMonitor> {
        self.power_monitor.as_ref()
    }

    /// Check if power monitoring is enabled
    pub fn is_power_monitoring_enabled(&self) -> bool {
        self.power_monitor.is_some()
    }
}

impl Default for EmbeddedRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime errors
#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// Runtime initialization failed
    InitializationFailed,
    /// Low battery condition
    LowBattery,
    /// Invalid configuration
    InvalidConfig,
    /// Platform-specific error
    PlatformError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let mut rt = EmbeddedRuntime::new();
        assert!(rt.init().is_ok());
    }

    #[test]
    fn test_power_mode_management() {
        let mut rt = EmbeddedRuntime::new();
        assert_eq!(rt.power_mode(), power::PowerMode::Normal);

        rt.set_power_mode(power::PowerMode::LowPower);
        assert_eq!(rt.power_mode(), power::PowerMode::LowPower);

        rt.set_power_mode(power::PowerMode::Sleep);
        assert_eq!(rt.power_mode(), power::PowerMode::Sleep);
    }

    #[test]
    fn test_battery_management() {
        let mut rt = EmbeddedRuntime::new();
        assert_eq!(rt.battery_level(), None);
        assert!(!rt.should_migrate());

        // Set good battery
        rt.update_battery(power::BatteryStatus {
            level_percent: 80,
            is_charging: false,
        });
        assert_eq!(rt.battery_level(), Some(80));
        assert!(!rt.should_migrate());

        // Set low battery
        rt.update_battery(power::BatteryStatus {
            level_percent: 15,
            is_charging: false,
        });
        assert_eq!(rt.battery_level(), Some(15));
        assert!(rt.should_migrate());

        // Charging cancels migration
        rt.update_battery(power::BatteryStatus {
            level_percent: 15,
            is_charging: true,
        });
        assert!(!rt.should_migrate());
    }

    #[test]
    fn test_power_monitoring_enable_disable() {
        let mut rt = EmbeddedRuntime::new();
        assert!(!rt.is_power_monitoring_enabled());
        assert!(rt.power_statistics().is_none());

        // Enable power monitoring
        rt.enable_power_monitoring(measurement::MeasurementConfig::default());
        assert!(rt.is_power_monitoring_enabled());
        assert!(rt.power_statistics().is_some());

        // Disable power monitoring
        rt.disable_power_monitoring();
        assert!(!rt.is_power_monitoring_enabled());
        assert!(rt.power_statistics().is_none());
    }

    #[test]
    fn test_power_monitoring_with_constructor() {
        let config = measurement::MeasurementConfig::default()
            .with_sample_rate_hz(100)
            .with_averaging(4);
        let rt = EmbeddedRuntime::with_power_monitoring(config);

        assert!(rt.is_power_monitoring_enabled());
        assert!(rt.power_monitor().is_some());
    }

    #[test]
    fn test_record_power_measurement() {
        let mut rt = EmbeddedRuntime::new();
        rt.enable_power_monitoring(measurement::MeasurementConfig::default().with_averaging(1));

        // Record a measurement
        let measurement = measurement::PowerMeasurement::new(3300, 150, 1000);
        let anomaly = rt.record_power_measurement(measurement);

        // Should not detect anomaly on first measurement
        assert!(anomaly.is_none());

        // Check statistics were updated
        let stats = rt.power_statistics().unwrap();
        assert!(stats.sample_count > 0);
    }

    #[test]
    fn test_power_monitor_access() {
        let mut rt = EmbeddedRuntime::new();
        rt.enable_power_monitoring(measurement::MeasurementConfig::default());

        // Test immutable access
        assert!(rt.power_monitor().is_some());

        // Test mutable access
        assert!(rt.power_monitor_mut().is_some());

        if let Some(monitor) = rt.power_monitor_mut() {
            // Can reset the monitor
            monitor.reset();
            assert_eq!(monitor.statistics().sample_count, 0);
        }
    }

    #[test]
    fn test_runtime_from_config() {
        let config = config::RuntimeConfig::default()
            .with_power_monitoring(true)
            .with_default_power_mode(power::PowerMode::LowPower);

        let rt = EmbeddedRuntime::from_config(config).unwrap();
        assert_eq!(rt.power_mode(), power::PowerMode::LowPower);
        assert!(rt.is_power_monitoring_enabled());
    }

    #[test]
    fn test_runtime_from_preset() {
        let rt = EmbeddedRuntime::from_preset(config::ConfigPreset::LowPower).unwrap();
        assert_eq!(rt.power_mode(), power::PowerMode::LowPower);
    }

    #[test]
    fn test_runtime_from_invalid_config() {
        let mut config = config::RuntimeConfig::default();
        config.power.battery_threshold_critical = 50;
        config.power.battery_threshold_low = 20;

        // Should fail validation
        let result = EmbeddedRuntime::from_config(config);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RuntimeError::InvalidConfig);
    }
}
