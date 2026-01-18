//! Power Consumption Measurement Module
//!
//! Provides hardware-level power consumption measurement capabilities for embedded devices.
//! Supports various power sensors and measurement techniques including:
//! - INA219/INA260 current/voltage sensors
//! - ADC-based voltage/current measurements
//! - Historical power consumption tracking
//! - Real-time power monitoring
//! - Anomaly detection
//!
//! ## Features
//!
//! - **Hardware Abstraction**: Generic traits for different power sensors
//! - **Real-time Monitoring**: Continuous power consumption tracking
//! - **Historical Data**: Time-series power consumption statistics
//! - **Anomaly Detection**: Detect unusual power consumption patterns
//! - **Multi-rail Support**: Monitor multiple power rails simultaneously
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::measurement::{PowerMeasurement, PowerMonitor, MeasurementConfig};
//!
//! // Configure measurement
//! let config = MeasurementConfig::default()
//!     .with_sample_rate_hz(100)
//!     .with_averaging(8);
//!
//! // Create monitor
//! let mut monitor = PowerMonitor::new(config);
//!
//! // Record measurement
//! let measurement = PowerMeasurement {
//!     voltage_mv: 3300,
//!     current_ma: 150,
//!     timestamp_us: 1000000,
//! };
//! monitor.record(measurement);
//!
//! // Get statistics
//! let stats = monitor.statistics();
//! println!("Average power: {} mW", stats.average_power_mw());
//! ```

use crate::energy::Energy;
use core::fmt;

// ============================================================================
// Error Types
// ============================================================================

/// Error type for multi-rail monitor operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiRailError {
    /// All rail slots are occupied
    RailsFull,
}

impl fmt::Display for MultiRailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultiRailError::RailsFull => write!(f, "All rail slots are occupied (max 8 rails)"),
        }
    }
}

// ============================================================================
// Power Sensor Traits
// ============================================================================

/// Trait for power sensors that can measure voltage and current
pub trait PowerSensor {
    /// Error type for sensor operations
    type Error;

    /// Read voltage in millivolts
    fn read_voltage_mv(&mut self) -> Result<u32, Self::Error>;

    /// Read current in milliamps
    fn read_current_ma(&mut self) -> Result<i32, Self::Error>;

    /// Read power in milliwatts (optional, can be calculated)
    fn read_power_mw(&mut self) -> Result<u32, Self::Error> {
        let voltage = self.read_voltage_mv()?;
        let current = self.read_current_ma()?;
        Ok(voltage * current.unsigned_abs() / 1000)
    }

    /// Reset the sensor
    fn reset(&mut self) -> Result<(), Self::Error>;

    /// Get sensor configuration
    fn config(&self) -> SensorConfig;
}

/// Configuration for a power sensor
#[derive(Debug, Clone, Copy)]
pub struct SensorConfig {
    /// Sensor name/type
    pub sensor_type: SensorType,
    /// Voltage range in millivolts
    pub voltage_range_mv: u32,
    /// Current range in milliamps
    pub current_range_ma: u32,
    /// Measurement resolution in bits
    pub resolution_bits: u8,
    /// Shunt resistor value in microohms (for current sensing)
    pub shunt_resistor_uohm: u32,
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self {
            sensor_type: SensorType::Generic,
            voltage_range_mv: 5000,
            current_range_ma: 3200,
            resolution_bits: 12,
            shunt_resistor_uohm: 100_000, // 0.1 ohm
        }
    }
}

/// Type of power sensor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorType {
    /// Generic ADC-based sensor
    Generic,
    /// INA219 current/power monitor
    INA219,
    /// INA260 current/power monitor
    INA260,
    /// INA3221 triple-channel monitor
    INA3221,
    /// LTC2990 voltage/current monitor
    LTC2990,
    /// Custom sensor type
    Custom,
}

impl fmt::Display for SensorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorType::Generic => write!(f, "Generic ADC"),
            SensorType::INA219 => write!(f, "INA219"),
            SensorType::INA260 => write!(f, "INA260"),
            SensorType::INA3221 => write!(f, "INA3221"),
            SensorType::LTC2990 => write!(f, "LTC2990"),
            SensorType::Custom => write!(f, "Custom"),
        }
    }
}

// ============================================================================
// Power Measurement Data
// ============================================================================

/// Single power measurement sample
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerMeasurement {
    /// Voltage in millivolts
    pub voltage_mv: u32,
    /// Current in milliamps (signed to support charging/discharging)
    pub current_ma: i32,
    /// Timestamp in microseconds
    pub timestamp_us: u64,
}

impl PowerMeasurement {
    /// Create a new power measurement
    pub const fn new(voltage_mv: u32, current_ma: i32, timestamp_us: u64) -> Self {
        Self {
            voltage_mv,
            current_ma,
            timestamp_us,
        }
    }

    /// Calculate instantaneous power in milliwatts
    pub const fn power_mw(&self) -> u32 {
        self.voltage_mv * self.current_ma.unsigned_abs() / 1000
    }

    /// Get absolute current (always positive)
    pub const fn abs_current_ma(&self) -> u32 {
        self.current_ma.unsigned_abs()
    }

    /// Check if current is flowing in (charging)
    pub const fn is_charging(&self) -> bool {
        self.current_ma < 0
    }

    /// Check if current is flowing out (discharging)
    pub const fn is_discharging(&self) -> bool {
        self.current_ma > 0
    }
}

// ============================================================================
// Power Measurement Configuration
// ============================================================================

/// Configuration for power measurement monitoring
#[derive(Debug, Clone, Copy)]
pub struct MeasurementConfig {
    /// Sample rate in Hz
    pub sample_rate_hz: u32,
    /// Number of samples to average
    pub averaging_samples: u8,
    /// Enable anomaly detection
    pub enable_anomaly_detection: bool,
    /// Anomaly threshold as percentage deviation from baseline (0-100)
    pub anomaly_threshold_percent: u8,
    /// History buffer size (number of measurements to keep)
    pub history_size: usize,
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 100,
            averaging_samples: 4,
            enable_anomaly_detection: true,
            anomaly_threshold_percent: 50,
            history_size: 1000,
        }
    }
}

impl MeasurementConfig {
    /// Set sample rate in Hz
    pub const fn with_sample_rate_hz(mut self, hz: u32) -> Self {
        self.sample_rate_hz = hz;
        self
    }

    /// Set number of samples to average
    pub const fn with_averaging(mut self, samples: u8) -> Self {
        self.averaging_samples = samples;
        self
    }

    /// Enable or disable anomaly detection
    pub const fn with_anomaly_detection(mut self, enabled: bool) -> Self {
        self.enable_anomaly_detection = enabled;
        self
    }

    /// Set anomaly threshold percentage
    pub const fn with_anomaly_threshold(mut self, percent: u8) -> Self {
        self.anomaly_threshold_percent = percent;
        self
    }

    /// Set history buffer size
    pub const fn with_history_size(mut self, size: usize) -> Self {
        self.history_size = size;
        self
    }

    /// Get sample period in microseconds
    pub const fn sample_period_us(&self) -> u64 {
        1_000_000 / self.sample_rate_hz as u64
    }
}

// ============================================================================
// Power Statistics
// ============================================================================

/// Statistical analysis of power consumption
#[derive(Debug, Clone, Copy)]
pub struct PowerStatistics {
    /// Number of samples
    pub sample_count: u32,
    /// Average voltage in millivolts
    pub avg_voltage_mv: u32,
    /// Minimum voltage in millivolts
    pub min_voltage_mv: u32,
    /// Maximum voltage in millivolts
    pub max_voltage_mv: u32,
    /// Average current in milliamps
    pub avg_current_ma: u32,
    /// Minimum current in milliamps
    pub min_current_ma: u32,
    /// Maximum current in milliamps
    pub max_current_ma: u32,
    /// Average power in milliwatts
    pub avg_power_mw: u32,
    /// Minimum power in milliwatts
    pub min_power_mw: u32,
    /// Maximum power in milliwatts
    pub max_power_mw: u32,
    /// Total energy consumed in microjoules
    pub total_energy_uj: u64,
    /// Total measurement time in microseconds
    pub total_time_us: u64,
}

impl Default for PowerStatistics {
    fn default() -> Self {
        Self {
            sample_count: 0,
            avg_voltage_mv: 0,
            min_voltage_mv: u32::MAX,
            max_voltage_mv: 0,
            avg_current_ma: 0,
            min_current_ma: u32::MAX,
            max_current_ma: 0,
            avg_power_mw: 0,
            min_power_mw: u32::MAX,
            max_power_mw: 0,
            total_energy_uj: 0,
            total_time_us: 0,
        }
    }
}

impl PowerStatistics {
    /// Get total energy as Energy type
    pub const fn total_energy(&self) -> Energy {
        Energy::microjoules(self.total_energy_uj)
    }

    /// Get average power in milliwatts
    pub const fn average_power_mw(&self) -> u32 {
        self.avg_power_mw
    }

    /// Get power range (max - min) in milliwatts
    pub const fn power_range_mw(&self) -> u32 {
        self.max_power_mw.saturating_sub(self.min_power_mw)
    }

    /// Get voltage range (max - min) in millivolts
    pub const fn voltage_range_mv(&self) -> u32 {
        self.max_voltage_mv.saturating_sub(self.min_voltage_mv)
    }

    /// Get current range (max - min) in milliamps
    pub const fn current_range_ma(&self) -> u32 {
        self.max_current_ma.saturating_sub(self.min_current_ma)
    }

    /// Get average efficiency (energy per time)
    pub const fn average_efficiency(&self) -> u32 {
        if self.total_time_us == 0 {
            0
        } else {
            (self.total_energy_uj / self.total_time_us) as u32
        }
    }
}

// ============================================================================
// Power Anomaly Detection
// ============================================================================

/// Power consumption anomaly
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAnomaly {
    /// Power consumption spike
    Spike {
        /// Power in milliwatts
        power_mw: u32,
        /// Baseline power in milliwatts
        baseline_mw: u32,
        /// Percentage increase
        increase_percent: u32,
    },
    /// Power consumption drop
    Drop {
        /// Power in milliwatts
        power_mw: u32,
        /// Baseline power in milliwatts
        baseline_mw: u32,
        /// Percentage decrease
        decrease_percent: u32,
    },
    /// Voltage out of range
    VoltageOutOfRange {
        /// Voltage in millivolts
        voltage_mv: u32,
        /// Expected range min
        min_mv: u32,
        /// Expected range max
        max_mv: u32,
    },
    /// Current out of range
    CurrentOutOfRange {
        /// Current in milliamps
        current_ma: u32,
        /// Expected max current
        max_ma: u32,
    },
}

impl fmt::Display for PowerAnomaly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerAnomaly::Spike {
                power_mw,
                baseline_mw,
                increase_percent,
            } => write!(
                f,
                "Power spike: {} mW (baseline: {} mW, +{}%)",
                power_mw, baseline_mw, increase_percent
            ),
            PowerAnomaly::Drop {
                power_mw,
                baseline_mw,
                decrease_percent,
            } => write!(
                f,
                "Power drop: {} mW (baseline: {} mW, -{}%)",
                power_mw, baseline_mw, decrease_percent
            ),
            PowerAnomaly::VoltageOutOfRange {
                voltage_mv,
                min_mv,
                max_mv,
            } => write!(
                f,
                "Voltage out of range: {} mV (expected: {}-{} mV)",
                voltage_mv, min_mv, max_mv
            ),
            PowerAnomaly::CurrentOutOfRange { current_ma, max_ma } => write!(
                f,
                "Current out of range: {} mA (max: {} mA)",
                current_ma, max_ma
            ),
        }
    }
}

/// Anomaly detector for power measurements
#[derive(Debug)]
pub struct AnomalyDetector {
    /// Baseline power in milliwatts
    baseline_power_mw: u32,
    /// Threshold percentage for anomaly detection
    threshold_percent: u8,
    /// Expected voltage range
    voltage_range: (u32, u32),
    /// Expected current range
    current_max: u32,
    /// Number of samples used for baseline
    baseline_sample_count: u32,
}

impl AnomalyDetector {
    /// Create a new anomaly detector
    pub fn new(threshold_percent: u8) -> Self {
        Self {
            baseline_power_mw: 0,
            threshold_percent,
            voltage_range: (0, u32::MAX),
            current_max: u32::MAX,
            baseline_sample_count: 0,
        }
    }

    /// Set expected voltage range in millivolts
    pub fn set_voltage_range(&mut self, min_mv: u32, max_mv: u32) {
        self.voltage_range = (min_mv, max_mv);
    }

    /// Set expected maximum current in milliamps
    pub fn set_current_max(&mut self, max_ma: u32) {
        self.current_max = max_ma;
    }

    /// Update baseline with a new measurement
    pub fn update_baseline(&mut self, power_mw: u32) {
        if self.baseline_sample_count == 0 {
            self.baseline_power_mw = power_mw;
        } else {
            // Running average
            let total = self.baseline_power_mw as u64 * self.baseline_sample_count as u64;
            self.baseline_power_mw =
                ((total + power_mw as u64) / (self.baseline_sample_count + 1) as u64) as u32;
        }
        self.baseline_sample_count += 1;
    }

    /// Reset baseline
    pub fn reset_baseline(&mut self) {
        self.baseline_power_mw = 0;
        self.baseline_sample_count = 0;
    }

    /// Check for anomalies in a measurement
    pub fn check(&self, measurement: &PowerMeasurement) -> Option<PowerAnomaly> {
        // Check voltage range
        if measurement.voltage_mv < self.voltage_range.0
            || measurement.voltage_mv > self.voltage_range.1
        {
            return Some(PowerAnomaly::VoltageOutOfRange {
                voltage_mv: measurement.voltage_mv,
                min_mv: self.voltage_range.0,
                max_mv: self.voltage_range.1,
            });
        }

        // Check current range
        let abs_current = measurement.abs_current_ma();
        if abs_current > self.current_max {
            return Some(PowerAnomaly::CurrentOutOfRange {
                current_ma: abs_current,
                max_ma: self.current_max,
            });
        }

        // Check power anomalies (only if baseline is established)
        if self.baseline_sample_count < 10 {
            return None;
        }

        let power_mw = measurement.power_mw();
        let threshold_mw = (self.baseline_power_mw * self.threshold_percent as u32) / 100;

        if power_mw > self.baseline_power_mw + threshold_mw {
            let increase_percent =
                ((power_mw - self.baseline_power_mw) * 100) / self.baseline_power_mw;
            return Some(PowerAnomaly::Spike {
                power_mw,
                baseline_mw: self.baseline_power_mw,
                increase_percent,
            });
        }

        if power_mw + threshold_mw < self.baseline_power_mw {
            let decrease_percent =
                ((self.baseline_power_mw - power_mw) * 100) / self.baseline_power_mw;
            return Some(PowerAnomaly::Drop {
                power_mw,
                baseline_mw: self.baseline_power_mw,
                decrease_percent,
            });
        }

        None
    }

    /// Get current baseline power
    pub const fn baseline_power_mw(&self) -> u32 {
        self.baseline_power_mw
    }
}

// ============================================================================
// Power Monitor
// ============================================================================

/// Maximum history size
const MAX_HISTORY_SIZE: usize = 2000;

/// Power monitor with historical tracking
#[derive(Debug)]
pub struct PowerMonitor {
    /// Configuration
    config: MeasurementConfig,
    /// Measurement history (circular buffer)
    history: [Option<PowerMeasurement>; MAX_HISTORY_SIZE],
    /// Current position in history buffer
    history_pos: usize,
    /// Number of measurements in history
    history_count: usize,
    /// Running statistics
    stats: PowerStatistics,
    /// Anomaly detector
    anomaly_detector: Option<AnomalyDetector>,
    /// Last timestamp
    last_timestamp_us: u64,
    /// Averaging buffer
    avg_buffer: [Option<PowerMeasurement>; 16],
    /// Average buffer position
    avg_pos: usize,
}

impl PowerMonitor {
    /// Create a new power monitor
    pub fn new(config: MeasurementConfig) -> Self {
        let anomaly_detector = if config.enable_anomaly_detection {
            let mut detector = AnomalyDetector::new(config.anomaly_threshold_percent);
            // Set reasonable defaults
            detector.set_voltage_range(2700, 3600); // 2.7V - 3.6V (typical Li-Ion range)
            detector.set_current_max(5000); // 5A max
            Some(detector)
        } else {
            None
        };

        Self {
            config,
            history: [None; MAX_HISTORY_SIZE],
            history_pos: 0,
            history_count: 0,
            stats: PowerStatistics::default(),
            anomaly_detector,
            last_timestamp_us: 0,
            avg_buffer: [None; 16],
            avg_pos: 0,
        }
    }

    /// Record a new measurement
    pub fn record(&mut self, measurement: PowerMeasurement) -> Option<PowerAnomaly> {
        // Add to averaging buffer
        self.avg_buffer[self.avg_pos] = Some(measurement);
        self.avg_pos = (self.avg_pos + 1) % self.config.averaging_samples.min(16) as usize;

        // Check if we should process (based on averaging)
        if self.avg_pos != 0 {
            return None;
        }

        // Calculate averaged measurement
        let averaged = self.calculate_average();

        // Check for anomalies
        let anomaly = self
            .anomaly_detector
            .as_ref()
            .and_then(|d| d.check(&averaged));

        // Update baseline (if no anomaly)
        if anomaly.is_none() {
            if let Some(ref mut detector) = self.anomaly_detector {
                detector.update_baseline(averaged.power_mw());
            }
        }

        // Store in history
        let history_size = self.config.history_size.min(MAX_HISTORY_SIZE);
        if history_size > 0 {
            self.history[self.history_pos] = Some(averaged);
            self.history_pos = (self.history_pos + 1) % history_size;
            if self.history_count < history_size {
                self.history_count += 1;
            }
        }

        // Update statistics
        self.update_statistics(&averaged);

        anomaly
    }

    /// Calculate average from buffer
    fn calculate_average(&self) -> PowerMeasurement {
        let count = self.config.averaging_samples.min(16) as usize;
        let mut sum_voltage = 0u64;
        let mut sum_current = 0i64;
        let mut last_timestamp = 0u64;
        let mut valid_count = 0;

        for i in 0..count {
            if let Some(m) = self.avg_buffer[i] {
                sum_voltage += m.voltage_mv as u64;
                sum_current += m.current_ma as i64;
                last_timestamp = m.timestamp_us;
                valid_count += 1;
            }
        }

        if valid_count == 0 {
            return PowerMeasurement::new(0, 0, 0);
        }

        PowerMeasurement::new(
            (sum_voltage / valid_count as u64) as u32,
            (sum_current / valid_count as i64) as i32,
            last_timestamp,
        )
    }

    /// Update statistics with a new measurement
    fn update_statistics(&mut self, measurement: &PowerMeasurement) {
        let power_mw = measurement.power_mw();
        let abs_current = measurement.abs_current_ma();

        // Update counts
        self.stats.sample_count += 1;

        // Update voltage stats
        self.stats.min_voltage_mv = self.stats.min_voltage_mv.min(measurement.voltage_mv);
        self.stats.max_voltage_mv = self.stats.max_voltage_mv.max(measurement.voltage_mv);

        // Update current stats
        self.stats.min_current_ma = self.stats.min_current_ma.min(abs_current);
        self.stats.max_current_ma = self.stats.max_current_ma.max(abs_current);

        // Update power stats
        self.stats.min_power_mw = self.stats.min_power_mw.min(power_mw);
        self.stats.max_power_mw = self.stats.max_power_mw.max(power_mw);

        // Update averages (running average)
        let count = self.stats.sample_count as u64;
        self.stats.avg_voltage_mv = (((count - 1) * self.stats.avg_voltage_mv as u64
            + measurement.voltage_mv as u64)
            / count) as u32;
        self.stats.avg_current_ma =
            (((count - 1) * self.stats.avg_current_ma as u64 + abs_current as u64) / count) as u32;
        self.stats.avg_power_mw =
            (((count - 1) * self.stats.avg_power_mw as u64 + power_mw as u64) / count) as u32;

        // Update energy (E = P × t)
        if self.last_timestamp_us > 0 {
            let duration_us = measurement
                .timestamp_us
                .saturating_sub(self.last_timestamp_us);
            let energy_uj = (power_mw as u64 * duration_us) / 1000;
            self.stats.total_energy_uj += energy_uj;
            self.stats.total_time_us += duration_us;
        }

        self.last_timestamp_us = measurement.timestamp_us;
    }

    /// Get current statistics
    pub fn statistics(&self) -> &PowerStatistics {
        &self.stats
    }

    /// Get anomaly detector
    pub fn anomaly_detector(&self) -> Option<&AnomalyDetector> {
        self.anomaly_detector.as_ref()
    }

    /// Get mutable anomaly detector
    pub fn anomaly_detector_mut(&mut self) -> Option<&mut AnomalyDetector> {
        self.anomaly_detector.as_mut()
    }

    /// Get measurement history
    pub fn history(&self) -> &[Option<PowerMeasurement>] {
        &self.history[..self.history_count.min(MAX_HISTORY_SIZE)]
    }

    /// Get latest measurement
    pub fn latest(&self) -> Option<PowerMeasurement> {
        if self.history_count == 0 {
            return None;
        }
        let idx = if self.history_pos == 0 {
            self.history_count - 1
        } else {
            self.history_pos - 1
        };
        self.history[idx]
    }

    /// Reset all statistics and history
    pub fn reset(&mut self) {
        self.history = [None; MAX_HISTORY_SIZE];
        self.history_pos = 0;
        self.history_count = 0;
        self.stats = PowerStatistics::default();
        self.last_timestamp_us = 0;
        self.avg_buffer = [None; 16];
        self.avg_pos = 0;

        if let Some(ref mut detector) = self.anomaly_detector {
            detector.reset_baseline();
        }
    }

    /// Get configuration
    pub fn config(&self) -> &MeasurementConfig {
        &self.config
    }
}

impl Default for PowerMonitor {
    fn default() -> Self {
        Self::new(MeasurementConfig::default())
    }
}

// ============================================================================
// Multi-Rail Power Monitor
// ============================================================================

/// Monitor multiple power rails simultaneously
pub struct MultiRailMonitor {
    /// Monitors for each rail (up to 8 rails)
    rails: [Option<PowerMonitor>; 8],
    /// Rail names
    rail_names: [Option<&'static str>; 8],
}

impl MultiRailMonitor {
    /// Create a new multi-rail monitor
    pub fn new() -> Self {
        Self {
            rails: [const { None }; 8],
            rail_names: [None; 8],
        }
    }

    /// Add a power rail to monitor
    pub fn add_rail(
        &mut self,
        name: &'static str,
        config: MeasurementConfig,
    ) -> Result<usize, MultiRailError> {
        for (i, slot) in self.rails.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(PowerMonitor::new(config));
                self.rail_names[i] = Some(name);
                return Ok(i);
            }
        }
        Err(MultiRailError::RailsFull)
    }

    /// Record measurement for a specific rail
    pub fn record(
        &mut self,
        rail_id: usize,
        measurement: PowerMeasurement,
    ) -> Option<PowerAnomaly> {
        self.rails
            .get_mut(rail_id)
            .and_then(|r| r.as_mut())
            .and_then(|m| m.record(measurement))
    }

    /// Get statistics for a specific rail
    pub fn rail_statistics(&self, rail_id: usize) -> Option<&PowerStatistics> {
        self.rails
            .get(rail_id)
            .and_then(|r| r.as_ref())
            .map(|m| m.statistics())
    }

    /// Get total power across all rails
    pub fn total_power_mw(&self) -> u32 {
        self.rails
            .iter()
            .flatten()
            .filter_map(|m| m.latest())
            .map(|m| m.power_mw())
            .sum()
    }

    /// Get total energy across all rails
    pub fn total_energy(&self) -> Energy {
        let total_uj: u64 = self
            .rails
            .iter()
            .flatten()
            .map(|m| m.statistics().total_energy_uj)
            .sum();
        Energy::microjoules(total_uj)
    }

    /// Get rail name
    pub fn rail_name(&self, rail_id: usize) -> Option<&'static str> {
        self.rail_names.get(rail_id).copied().flatten()
    }

    /// Reset all rails
    pub fn reset_all(&mut self) {
        for rail in self.rails.iter_mut().flatten() {
            rail.reset();
        }
    }
}

impl Default for MultiRailMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_power_measurement() {
        let m = PowerMeasurement::new(3300, 150, 1000);
        assert_eq!(m.voltage_mv, 3300);
        assert_eq!(m.current_ma, 150);
        assert_eq!(m.power_mw(), 495); // 3.3V * 0.15A = 0.495W = 495mW
        assert!(!m.is_charging());
        assert!(m.is_discharging());

        let m2 = PowerMeasurement::new(3300, -150, 2000);
        assert!(m2.is_charging());
        assert!(!m2.is_discharging());
    }

    #[test]
    fn test_measurement_config() {
        let config = MeasurementConfig::default()
            .with_sample_rate_hz(200)
            .with_averaging(8)
            .with_anomaly_detection(false);

        assert_eq!(config.sample_rate_hz, 200);
        assert_eq!(config.averaging_samples, 8);
        assert!(!config.enable_anomaly_detection);
        assert_eq!(config.sample_period_us(), 5000); // 1/200 Hz = 5ms = 5000us
    }

    #[test]
    fn test_power_statistics() {
        let stats = PowerStatistics {
            sample_count: 100,
            avg_power_mw: 500,
            min_power_mw: 400,
            max_power_mw: 600,
            total_energy_uj: 5_000_000,
            total_time_us: 10_000_000,
            ..Default::default()
        };

        assert_eq!(stats.average_power_mw(), 500);
        assert_eq!(stats.power_range_mw(), 200);
        assert_eq!(stats.total_energy().as_millijoules(), 5000);
    }

    #[test]
    fn test_anomaly_detector_voltage() {
        let mut detector = AnomalyDetector::new(50);
        detector.set_voltage_range(3000, 3600);

        // Normal voltage
        let m1 = PowerMeasurement::new(3300, 100, 1000);
        assert!(detector.check(&m1).is_none());

        // Out of range voltage
        let m2 = PowerMeasurement::new(2500, 100, 2000);
        assert!(matches!(
            detector.check(&m2),
            Some(PowerAnomaly::VoltageOutOfRange { .. })
        ));
    }

    #[test]
    fn test_anomaly_detector_current() {
        let mut detector = AnomalyDetector::new(50);
        detector.set_current_max(1000);

        // Normal current
        let m1 = PowerMeasurement::new(3300, 500, 1000);
        assert!(detector.check(&m1).is_none());

        // Out of range current
        let m2 = PowerMeasurement::new(3300, 2000, 2000);
        assert!(matches!(
            detector.check(&m2),
            Some(PowerAnomaly::CurrentOutOfRange { .. })
        ));
    }

    #[test]
    fn test_anomaly_detector_power_spike() {
        let mut detector = AnomalyDetector::new(50);
        detector.set_voltage_range(0, 5000);
        detector.set_current_max(10000);

        // Establish baseline
        for i in 0..20 {
            let m = PowerMeasurement::new(3300, 100, i * 1000);
            detector.update_baseline(m.power_mw());
        }

        let baseline = detector.baseline_power_mw();
        assert!(baseline > 0);

        // Normal measurement
        let m1 = PowerMeasurement::new(3300, 110, 21000);
        assert!(detector.check(&m1).is_none());

        // Power spike
        let m2 = PowerMeasurement::new(3300, 500, 22000);
        let anomaly = detector.check(&m2);
        assert!(matches!(anomaly, Some(PowerAnomaly::Spike { .. })));
    }

    #[test]
    fn test_power_monitor_basic() {
        let config = MeasurementConfig::default().with_averaging(1);
        let mut monitor = PowerMonitor::new(config);

        // Record measurements
        for i in 0..10 {
            let m = PowerMeasurement::new(3300, 100 + i * 10, i as u64 * 10000);
            monitor.record(m);
        }

        let stats = monitor.statistics();
        assert_eq!(stats.sample_count, 10);
        assert!(stats.avg_power_mw > 0);
        assert!(stats.total_energy_uj > 0);
    }

    #[test]
    fn test_power_monitor_averaging() {
        let config = MeasurementConfig::default().with_averaging(4);
        let mut monitor = PowerMonitor::new(config);

        // Record measurements (should average every 4)
        for i in 0..8 {
            let m = PowerMeasurement::new(3300, 100, i * 1000);
            monitor.record(m);
        }

        let stats = monitor.statistics();
        assert_eq!(stats.sample_count, 2); // 8 samples / 4 averaging = 2
    }

    #[test]
    fn test_power_monitor_history() {
        let config = MeasurementConfig::default()
            .with_averaging(1)
            .with_history_size(10);
        let mut monitor = PowerMonitor::new(config);

        // Record measurements
        for i in 0..15 {
            let m = PowerMeasurement::new(3300, 100, i * 1000);
            monitor.record(m);
        }

        // Should keep last 10 measurements
        let history = monitor.history();
        assert_eq!(history.len(), 10);

        // Check latest
        let latest = monitor.latest().unwrap();
        assert_eq!(latest.timestamp_us, 14000);
    }

    #[test]
    fn test_power_monitor_reset() {
        let config = MeasurementConfig::default().with_averaging(1);
        let mut monitor = PowerMonitor::new(config);

        // Record measurement
        let m = PowerMeasurement::new(3300, 100, 1000);
        monitor.record(m);

        assert_eq!(monitor.statistics().sample_count, 1);

        // Reset
        monitor.reset();
        assert_eq!(monitor.statistics().sample_count, 0);
        assert!(monitor.latest().is_none());
    }

    #[test]
    fn test_multi_rail_monitor() {
        let mut monitor = MultiRailMonitor::new();

        // Add rails
        let rail0 = monitor
            .add_rail("VDD_CORE", MeasurementConfig::default().with_averaging(1))
            .unwrap();
        let rail1 = monitor
            .add_rail("VDD_IO", MeasurementConfig::default().with_averaging(1))
            .unwrap();

        assert_eq!(monitor.rail_name(rail0), Some("VDD_CORE"));
        assert_eq!(monitor.rail_name(rail1), Some("VDD_IO"));

        // Record measurements
        monitor.record(rail0, PowerMeasurement::new(1200, 500, 1000));
        monitor.record(rail1, PowerMeasurement::new(3300, 200, 1000));

        // Check total power: (1.2V * 0.5A) + (3.3V * 0.2A) = 0.6W + 0.66W = 1.26W
        let total = monitor.total_power_mw();
        assert!(total > 1200 && total < 1300);
    }

    #[test]
    fn test_sensor_type_display() {
        assert_eq!(format!("{}", SensorType::INA219), "INA219");
        assert_eq!(format!("{}", SensorType::Generic), "Generic ADC");
    }

    #[test]
    fn test_power_anomaly_display() {
        let spike = PowerAnomaly::Spike {
            power_mw: 1000,
            baseline_mw: 500,
            increase_percent: 100,
        };
        let s = format!("{}", spike);
        assert!(s.contains("Power spike"));
        assert!(s.contains("1000 mW"));
    }

    #[test]
    fn test_sensor_config_default() {
        let config = SensorConfig::default();
        assert_eq!(config.voltage_range_mv, 5000);
        assert_eq!(config.resolution_bits, 12);
    }
}
