//! Runtime Configuration Module
//!
//! Provides a comprehensive configuration system for the MielinRT embedded runtime.
//! Allows customization of power management, memory allocation, monitoring, and
//! system behavior.
//!
//! ## Features
//!
//! - **Power Management**: Configure power modes, transitions, and battery thresholds
//! - **Memory Pool**: Customize pool sizes, allocation strategy, and safety checks
//! - **Monitoring**: Enable/disable power and energy monitoring
//! - **Stack Management**: Configure stack sizes and overflow detection
//! - **Presets**: Common configurations for different use cases
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::config::{RuntimeConfig, ConfigPreset};
//!
//! // Use a preset configuration
//! let config = RuntimeConfig::preset(ConfigPreset::LowPower);
//!
//! // Or customize
//! let custom = RuntimeConfig::default()
//!     .with_power_monitoring(true)
//!     .with_battery_threshold_low(20)
//!     .with_battery_threshold_critical(5);
//! ```

use crate::battery::BatteryConfig;
use crate::measurement::MeasurementConfig;
use crate::pool::PoolConfig;
use crate::stack::StackConfig;

/// Complete runtime configuration
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    /// Power management configuration
    pub power: PowerConfig,
    /// Memory pool configuration
    pub memory: MemoryConfig,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    /// Stack management configuration
    pub stack: StackConfig,
    /// Battery configuration
    pub battery: Option<BatteryConfig>,
}

impl RuntimeConfig {
    /// Create a new runtime configuration with default values
    pub fn new() -> Self {
        Self {
            power: PowerConfig::new(),
            memory: MemoryConfig::new(),
            monitoring: MonitoringConfig::new(),
            stack: StackConfig::minimal(),
            battery: None,
        }
    }

    /// Create a configuration from a preset
    pub fn preset(preset: ConfigPreset) -> Self {
        match preset {
            ConfigPreset::Default => Self::default(),
            ConfigPreset::LowPower => Self::low_power_preset(),
            ConfigPreset::HighPerformance => Self::high_performance_preset(),
            ConfigPreset::Minimal => Self::minimal_preset(),
            ConfigPreset::MaximumMonitoring => Self::maximum_monitoring_preset(),
        }
    }

    /// Low power preset - optimized for battery life
    fn low_power_preset() -> Self {
        Self {
            power: PowerConfig {
                default_mode: crate::power::PowerMode::LowPower,
                enable_dvfs: true,
                enable_power_gating: true,
                enable_clock_gating: true,
                battery_threshold_low: 30,
                battery_threshold_critical: 10,
            },
            memory: MemoryConfig {
                enable_pool_allocator: true,
                pool_config: PoolConfig::minimal(),
                enable_safety_checks: true,
                enable_statistics: false, // Reduce overhead
            },
            monitoring: MonitoringConfig {
                enable_power_monitoring: false, // Disable to save power
                enable_energy_profiling: false,
                power_sample_rate_hz: 10,
                enable_anomaly_detection: false,
            },
            stack: StackConfig::minimal(),
            battery: Some(BatteryConfig::single_cell_lipo(1000)), // Small battery
        }
    }

    /// High performance preset - optimized for speed
    fn high_performance_preset() -> Self {
        Self {
            power: PowerConfig {
                default_mode: crate::power::PowerMode::Normal,
                enable_dvfs: false, // Keep maximum frequency
                enable_power_gating: false,
                enable_clock_gating: false,
                battery_threshold_low: 20,
                battery_threshold_critical: 5,
            },
            memory: MemoryConfig {
                enable_pool_allocator: true,
                pool_config: PoolConfig::generous(),
                enable_safety_checks: false, // Reduce overhead
                enable_statistics: true,
            },
            monitoring: MonitoringConfig {
                enable_power_monitoring: true,
                enable_energy_profiling: true,
                power_sample_rate_hz: 1000, // High sampling rate
                enable_anomaly_detection: true,
            },
            stack: StackConfig::large(),
            battery: None, // Assume external power
        }
    }

    /// Minimal preset - smallest footprint
    fn minimal_preset() -> Self {
        Self {
            power: PowerConfig {
                default_mode: crate::power::PowerMode::Normal,
                enable_dvfs: false,
                enable_power_gating: false,
                enable_clock_gating: false,
                battery_threshold_low: 15,
                battery_threshold_critical: 5,
            },
            memory: MemoryConfig {
                enable_pool_allocator: true,
                pool_config: PoolConfig::minimal(),
                enable_safety_checks: false,
                enable_statistics: false,
            },
            monitoring: MonitoringConfig {
                enable_power_monitoring: false,
                enable_energy_profiling: false,
                power_sample_rate_hz: 1,
                enable_anomaly_detection: false,
            },
            stack: StackConfig::minimal(),
            battery: None,
        }
    }

    /// Maximum monitoring preset - comprehensive diagnostics
    fn maximum_monitoring_preset() -> Self {
        Self {
            power: PowerConfig {
                default_mode: crate::power::PowerMode::Normal,
                enable_dvfs: true,
                enable_power_gating: true,
                enable_clock_gating: true,
                battery_threshold_low: 20,
                battery_threshold_critical: 5,
            },
            memory: MemoryConfig {
                enable_pool_allocator: true,
                pool_config: PoolConfig::standard(),
                enable_safety_checks: true,
                enable_statistics: true,
            },
            monitoring: MonitoringConfig {
                enable_power_monitoring: true,
                enable_energy_profiling: true,
                power_sample_rate_hz: 1000,
                enable_anomaly_detection: true,
            },
            stack: StackConfig::standard(),
            battery: Some(BatteryConfig::single_cell_lipo(2000)),
        }
    }

    /// Configure with power monitoring enabled
    pub const fn with_power_monitoring(mut self, enabled: bool) -> Self {
        self.monitoring.enable_power_monitoring = enabled;
        self
    }

    /// Configure with energy profiling enabled
    pub const fn with_energy_profiling(mut self, enabled: bool) -> Self {
        self.monitoring.enable_energy_profiling = enabled;
        self
    }

    /// Set battery low threshold (percentage)
    pub const fn with_battery_threshold_low(mut self, threshold: u8) -> Self {
        self.power.battery_threshold_low = threshold;
        self
    }

    /// Set battery critical threshold (percentage)
    pub const fn with_battery_threshold_critical(mut self, threshold: u8) -> Self {
        self.power.battery_threshold_critical = threshold;
        self
    }

    /// Set default power mode
    pub const fn with_default_power_mode(mut self, mode: crate::power::PowerMode) -> Self {
        self.power.default_mode = mode;
        self
    }

    /// Configure battery
    pub fn with_battery_config(mut self, config: BatteryConfig) -> Self {
        self.battery = Some(config);
        self
    }

    /// Enable or disable DVFS (Dynamic Voltage and Frequency Scaling)
    pub const fn with_dvfs(mut self, enabled: bool) -> Self {
        self.power.enable_dvfs = enabled;
        self
    }

    /// Enable or disable power gating
    pub const fn with_power_gating(mut self, enabled: bool) -> Self {
        self.power.enable_power_gating = enabled;
        self
    }

    /// Enable or disable clock gating
    pub const fn with_clock_gating(mut self, enabled: bool) -> Self {
        self.power.enable_clock_gating = enabled;
        self
    }

    /// Set memory pool configuration
    pub fn with_pool_config(mut self, config: PoolConfig) -> Self {
        self.memory.pool_config = config;
        self
    }

    /// Set stack configuration
    pub const fn with_stack_config(mut self, config: StackConfig) -> Self {
        self.stack = config;
        self
    }

    /// Set power sampling rate
    pub const fn with_power_sample_rate(mut self, hz: u32) -> Self {
        self.monitoring.power_sample_rate_hz = hz;
        self
    }

    /// Enable or disable anomaly detection
    pub const fn with_anomaly_detection(mut self, enabled: bool) -> Self {
        self.monitoring.enable_anomaly_detection = enabled;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate battery thresholds
        if self.power.battery_threshold_critical >= self.power.battery_threshold_low {
            return Err(ConfigError::InvalidBatteryThresholds);
        }

        if self.power.battery_threshold_low > 100 {
            return Err(ConfigError::InvalidBatteryThresholds);
        }

        // Validate monitoring settings
        if self.monitoring.power_sample_rate_hz == 0 && self.monitoring.enable_power_monitoring {
            return Err(ConfigError::InvalidSampleRate);
        }

        Ok(())
    }

    /// Get measurement configuration for power monitoring
    pub fn measurement_config(&self) -> Option<MeasurementConfig> {
        if self.monitoring.enable_power_monitoring {
            Some(
                MeasurementConfig::default()
                    .with_sample_rate_hz(self.monitoring.power_sample_rate_hz)
                    .with_anomaly_detection(self.monitoring.enable_anomaly_detection),
            )
        } else {
            None
        }
    }
}

/// Power management configuration
#[derive(Debug, Clone, Copy)]
pub struct PowerConfig {
    /// Default power mode on startup
    pub default_mode: crate::power::PowerMode,
    /// Enable DVFS (Dynamic Voltage and Frequency Scaling)
    pub enable_dvfs: bool,
    /// Enable peripheral power gating
    pub enable_power_gating: bool,
    /// Enable clock gating
    pub enable_clock_gating: bool,
    /// Battery level threshold for "low battery" (percentage)
    pub battery_threshold_low: u8,
    /// Battery level threshold for "critical battery" (percentage)
    pub battery_threshold_critical: u8,
}

impl PowerConfig {
    /// Create a new power configuration with default values
    pub const fn new() -> Self {
        Self {
            default_mode: crate::power::PowerMode::Normal,
            enable_dvfs: true,
            enable_power_gating: true,
            enable_clock_gating: true,
            battery_threshold_low: 20,
            battery_threshold_critical: 5,
        }
    }
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory management configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Enable pool allocator
    pub enable_pool_allocator: bool,
    /// Pool configuration
    pub pool_config: PoolConfig,
    /// Enable safety checks (bounds checking, double-free detection)
    pub enable_safety_checks: bool,
    /// Enable statistics tracking
    pub enable_statistics: bool,
}

impl MemoryConfig {
    /// Create a new memory configuration with default values
    pub fn new() -> Self {
        Self {
            enable_pool_allocator: true,
            pool_config: PoolConfig::default(),
            enable_safety_checks: true,
            enable_statistics: true,
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitoring and profiling configuration
#[derive(Debug, Clone, Copy)]
pub struct MonitoringConfig {
    /// Enable power consumption monitoring
    pub enable_power_monitoring: bool,
    /// Enable energy profiling
    pub enable_energy_profiling: bool,
    /// Power measurement sample rate in Hz
    pub power_sample_rate_hz: u32,
    /// Enable anomaly detection
    pub enable_anomaly_detection: bool,
}

impl MonitoringConfig {
    /// Create a new monitoring configuration with default values
    pub const fn new() -> Self {
        Self {
            enable_power_monitoring: true,
            enable_energy_profiling: true,
            power_sample_rate_hz: 100,
            enable_anomaly_detection: true,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration presets for common use cases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPreset {
    /// Default balanced configuration
    Default,
    /// Optimized for battery life
    LowPower,
    /// Optimized for performance
    HighPerformance,
    /// Minimal footprint
    Minimal,
    /// Maximum monitoring and diagnostics
    MaximumMonitoring,
}

/// Configuration errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// Battery thresholds are invalid
    InvalidBatteryThresholds,
    /// Sample rate is invalid
    InvalidSampleRate,
    /// Invalid power mode configuration
    InvalidPowerMode,
    /// Invalid memory configuration
    InvalidMemoryConfig,
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigError::InvalidBatteryThresholds => {
                write!(f, "Battery thresholds are invalid (critical must be < low)")
            }
            ConfigError::InvalidSampleRate => {
                write!(f, "Sample rate must be > 0 when monitoring is enabled")
            }
            ConfigError::InvalidPowerMode => write!(f, "Invalid power mode configuration"),
            ConfigError::InvalidMemoryConfig => write!(f, "Invalid memory configuration"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_runtime_config_presets() {
        let presets = [
            ConfigPreset::Default,
            ConfigPreset::LowPower,
            ConfigPreset::HighPerformance,
            ConfigPreset::Minimal,
            ConfigPreset::MaximumMonitoring,
        ];

        for preset in &presets {
            let config = RuntimeConfig::preset(*preset);
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_low_power_preset() {
        let config = RuntimeConfig::preset(ConfigPreset::LowPower);
        assert_eq!(config.power.default_mode, crate::power::PowerMode::LowPower);
        assert!(config.power.enable_dvfs);
        assert!(!config.monitoring.enable_power_monitoring); // Disabled to save power
    }

    #[test]
    fn test_high_performance_preset() {
        let config = RuntimeConfig::preset(ConfigPreset::HighPerformance);
        assert_eq!(config.power.default_mode, crate::power::PowerMode::Normal);
        assert!(!config.power.enable_dvfs); // Disabled for max performance
        assert!(config.monitoring.enable_power_monitoring);
        assert_eq!(config.monitoring.power_sample_rate_hz, 1000);
    }

    #[test]
    fn test_minimal_preset() {
        let config = RuntimeConfig::preset(ConfigPreset::Minimal);
        assert!(!config.monitoring.enable_power_monitoring);
        assert!(!config.monitoring.enable_energy_profiling);
        assert!(!config.memory.enable_statistics);
    }

    #[test]
    fn test_maximum_monitoring_preset() {
        let config = RuntimeConfig::preset(ConfigPreset::MaximumMonitoring);
        assert!(config.monitoring.enable_power_monitoring);
        assert!(config.monitoring.enable_energy_profiling);
        assert!(config.monitoring.enable_anomaly_detection);
        assert!(config.memory.enable_statistics);
        assert!(config.memory.enable_safety_checks);
    }

    #[test]
    fn test_config_validation_battery_thresholds() {
        let mut config = RuntimeConfig::default();
        config.power.battery_threshold_critical = 30;
        config.power.battery_threshold_low = 20;

        // Critical should be less than low
        assert!(config.validate().is_err());
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::InvalidBatteryThresholds
        );
    }

    #[test]
    fn test_config_validation_sample_rate() {
        let mut config = RuntimeConfig::default();
        config.monitoring.enable_power_monitoring = true;
        config.monitoring.power_sample_rate_hz = 0;

        assert!(config.validate().is_err());
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::InvalidSampleRate
        );
    }

    #[test]
    fn test_config_builder_pattern() {
        let config = RuntimeConfig::default()
            .with_power_monitoring(true)
            .with_energy_profiling(false)
            .with_battery_threshold_low(25)
            .with_battery_threshold_critical(10)
            .with_dvfs(false);

        assert!(config.monitoring.enable_power_monitoring);
        assert!(!config.monitoring.enable_energy_profiling);
        assert_eq!(config.power.battery_threshold_low, 25);
        assert_eq!(config.power.battery_threshold_critical, 10);
        assert!(!config.power.enable_dvfs);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_measurement_config_generation() {
        let config = RuntimeConfig::default()
            .with_power_monitoring(true)
            .with_power_sample_rate(200)
            .with_anomaly_detection(false);

        let meas_config = config.measurement_config();
        assert!(meas_config.is_some());

        let meas = meas_config.unwrap();
        assert_eq!(meas.sample_rate_hz, 200);
        assert!(!meas.enable_anomaly_detection);
    }

    #[test]
    fn test_measurement_config_disabled() {
        let config = RuntimeConfig::default().with_power_monitoring(false);

        assert!(config.measurement_config().is_none());
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::InvalidBatteryThresholds;
        let msg = format!("{}", err);
        assert!(msg.contains("Battery thresholds"));
    }
}
