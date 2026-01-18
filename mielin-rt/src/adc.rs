//! ADC (Analog-to-Digital Converter) and DAC (Digital-to-Analog Converter) Drivers
//!
//! Provides hardware abstraction for analog signal processing. Essential for
//! sensor reading and analog output generation.
//!
//! ## Overview
//!
//! The ADC/DAC module provides:
//! - Multi-channel ADC with configurable resolution
//! - Single-shot and continuous conversion modes
//! - Multiple sampling rates and conversion times
//! - Reference voltage configuration
//! - DAC output with various resolutions
//! - Oversampling and averaging
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::adc::{Adc, AdcConfig, Resolution};
//!
//! // Configure 12-bit ADC
//! let config = AdcConfig::default()
//!     .with_resolution(Resolution::Bits12)
//!     .with_reference_voltage_mv(3300);
//!
//! let mut adc = Adc::new(0, config);
//! adc.init().unwrap();
//!
//! // Read channel 0
//! let raw_value = adc.read_channel(0).unwrap();
//! let voltage_mv = adc.read_voltage_mv(0).unwrap();
//! ```

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// =============================================================================
// ADC Configuration
// =============================================================================

/// ADC resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// 6-bit resolution (0-63)
    Bits6,
    /// 8-bit resolution (0-255)
    Bits8,
    /// 10-bit resolution (0-1023)
    Bits10,
    /// 12-bit resolution (0-4095, most common)
    Bits12,
    /// 14-bit resolution (0-16383)
    Bits14,
    /// 16-bit resolution (0-65535)
    Bits16,
}

impl Resolution {
    /// Get maximum value for this resolution
    pub fn max_value(&self) -> u16 {
        match self {
            Resolution::Bits6 => 63,
            Resolution::Bits8 => 255,
            Resolution::Bits10 => 1023,
            Resolution::Bits12 => 4095,
            Resolution::Bits14 => 16383,
            Resolution::Bits16 => 65535,
        }
    }

    /// Get number of bits
    pub fn bits(&self) -> u8 {
        match self {
            Resolution::Bits6 => 6,
            Resolution::Bits8 => 8,
            Resolution::Bits10 => 10,
            Resolution::Bits12 => 12,
            Resolution::Bits14 => 14,
            Resolution::Bits16 => 16,
        }
    }
}

/// ADC conversion mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionMode {
    /// Single conversion (on-demand)
    Single,
    /// Continuous conversion
    Continuous,
    /// Scan mode (multiple channels in sequence)
    Scan,
}

/// ADC sampling time
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingTime {
    /// 1.5 cycles
    Cycles1_5,
    /// 7.5 cycles
    Cycles7_5,
    /// 13.5 cycles
    Cycles13_5,
    /// 28.5 cycles
    Cycles28_5,
    /// 41.5 cycles
    Cycles41_5,
    /// 55.5 cycles
    Cycles55_5,
    /// 71.5 cycles
    Cycles71_5,
    /// 239.5 cycles (slowest, highest precision)
    Cycles239_5,
}

impl SamplingTime {
    /// Get cycles as float
    pub fn cycles(&self) -> f32 {
        match self {
            SamplingTime::Cycles1_5 => 1.5,
            SamplingTime::Cycles7_5 => 7.5,
            SamplingTime::Cycles13_5 => 13.5,
            SamplingTime::Cycles28_5 => 28.5,
            SamplingTime::Cycles41_5 => 41.5,
            SamplingTime::Cycles55_5 => 55.5,
            SamplingTime::Cycles71_5 => 71.5,
            SamplingTime::Cycles239_5 => 239.5,
        }
    }
}

/// ADC configuration
#[derive(Debug, Clone, Copy)]
pub struct AdcConfig {
    /// Resolution
    resolution: Resolution,
    /// Conversion mode
    conversion_mode: ConversionMode,
    /// Sampling time
    sampling_time: SamplingTime,
    /// Reference voltage in millivolts
    reference_voltage_mv: u16,
    /// Enable oversampling
    oversampling: bool,
    /// Oversampling ratio (2, 4, 8, 16, 32, 64, 128, 256)
    oversampling_ratio: u16,
}

impl AdcConfig {
    /// Create a new ADC configuration
    pub fn new() -> Self {
        Self {
            resolution: Resolution::Bits12,
            conversion_mode: ConversionMode::Single,
            sampling_time: SamplingTime::Cycles28_5,
            reference_voltage_mv: 3300, // 3.3V typical
            oversampling: false,
            oversampling_ratio: 1,
        }
    }

    /// Set resolution
    pub fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    /// Set conversion mode
    pub fn with_conversion_mode(mut self, mode: ConversionMode) -> Self {
        self.conversion_mode = mode;
        self
    }

    /// Set sampling time
    pub fn with_sampling_time(mut self, sampling_time: SamplingTime) -> Self {
        self.sampling_time = sampling_time;
        self
    }

    /// Set reference voltage in millivolts
    pub fn with_reference_voltage_mv(mut self, voltage_mv: u16) -> Self {
        self.reference_voltage_mv = voltage_mv;
        self
    }

    /// Enable oversampling with ratio
    pub fn with_oversampling(mut self, ratio: u16) -> Self {
        self.oversampling = true;
        self.oversampling_ratio = ratio;
        self
    }

    /// Get resolution
    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// Get reference voltage in millivolts
    pub fn reference_voltage_mv(&self) -> u16 {
        self.reference_voltage_mv
    }
}

impl Default for AdcConfig {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ADC Controller
// =============================================================================

/// ADC controller
pub struct Adc {
    /// Instance number
    instance: u8,

    /// Configuration
    config: AdcConfig,

    /// Initialized flag
    initialized: AtomicBool,

    /// Active channels (bit mask)
    active_channels: AtomicU32,

    /// Conversion count
    conversion_count: AtomicU32,
}

impl Adc {
    /// Create a new ADC instance
    pub fn new(instance: u8, config: AdcConfig) -> Self {
        Self {
            instance,
            config,
            initialized: AtomicBool::new(false),
            active_channels: AtomicU32::new(0),
            conversion_count: AtomicU32::new(0),
        }
    }

    /// Get instance number
    pub fn instance(&self) -> u8 {
        self.instance
    }

    /// Initialize ADC
    pub fn init(&mut self) -> Result<(), AdcError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Enable ADC peripheral clock
        // 2. Configure reference voltage
        // 3. Set resolution
        // 4. Configure sampling time
        // 5. Calibrate ADC
        // 6. Enable ADC

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Enable a channel
    pub fn enable_channel(&mut self, channel: u8) -> Result<(), AdcError> {
        if channel >= 32 {
            return Err(AdcError::InvalidChannel);
        }

        let mask = 1u32 << channel;
        self.active_channels.fetch_or(mask, Ordering::Relaxed);

        Ok(())
    }

    /// Disable a channel
    pub fn disable_channel(&mut self, channel: u8) -> Result<(), AdcError> {
        if channel >= 32 {
            return Err(AdcError::InvalidChannel);
        }

        let mask = !(1u32 << channel);
        self.active_channels.fetch_and(mask, Ordering::Relaxed);

        Ok(())
    }

    /// Check if channel is enabled
    pub fn is_channel_enabled(&self, channel: u8) -> bool {
        if channel >= 32 {
            return false;
        }

        let mask = 1u32 << channel;
        (self.active_channels.load(Ordering::Relaxed) & mask) != 0
    }

    /// Read raw value from channel
    pub fn read_channel(&mut self, channel: u8) -> Result<u16, AdcError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(AdcError::NotInitialized);
        }

        if channel >= 32 {
            return Err(AdcError::InvalidChannel);
        }

        // In a real implementation, this would:
        // 1. Select channel
        // 2. Start conversion
        // 3. Wait for conversion complete
        // 4. Read data register
        // 5. Apply oversampling if enabled

        self.conversion_count.fetch_add(1, Ordering::Relaxed);

        // Simulate: return middle value
        Ok(self.config.resolution.max_value() / 2)
    }

    /// Read voltage in millivolts from channel
    pub fn read_voltage_mv(&mut self, channel: u8) -> Result<u16, AdcError> {
        let raw = self.read_channel(channel)?;
        let max = self.config.resolution.max_value() as u32;
        let vref = self.config.reference_voltage_mv as u32;

        // voltage = (raw / max) * vref
        let voltage = (raw as u32 * vref) / max;

        Ok(voltage as u16)
    }

    /// Read multiple channels
    pub fn read_channels(&mut self, channels: &[u8]) -> Result<Vec<u16>, AdcError> {
        let mut results = Vec::with_capacity(channels.len());

        for &channel in channels {
            let value = self.read_channel(channel)?;
            results.push(value);
        }

        Ok(results)
    }

    /// Get conversion count
    pub fn conversion_count(&self) -> u32 {
        self.conversion_count.load(Ordering::Relaxed)
    }

    /// Reset conversion count
    pub fn reset_conversion_count(&mut self) {
        self.conversion_count.store(0, Ordering::Relaxed);
    }
}

// =============================================================================
// DAC Controller
// =============================================================================

/// DAC configuration
#[derive(Debug, Clone, Copy)]
pub struct DacConfig {
    /// Resolution
    resolution: Resolution,
    /// Reference voltage in millivolts
    reference_voltage_mv: u16,
    /// Enable output buffer
    buffer_enabled: bool,
}

impl DacConfig {
    /// Create a new DAC configuration
    pub fn new() -> Self {
        Self {
            resolution: Resolution::Bits12,
            reference_voltage_mv: 3300,
            buffer_enabled: true,
        }
    }

    /// Set resolution
    pub fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    /// Set reference voltage
    pub fn with_reference_voltage_mv(mut self, voltage_mv: u16) -> Self {
        self.reference_voltage_mv = voltage_mv;
        self
    }

    /// Enable/disable output buffer
    pub fn with_buffer(mut self, enabled: bool) -> Self {
        self.buffer_enabled = enabled;
        self
    }

    /// Get resolution
    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// Get reference voltage
    pub fn reference_voltage_mv(&self) -> u16 {
        self.reference_voltage_mv
    }
}

impl Default for DacConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// DAC controller
pub struct Dac {
    /// Instance number
    instance: u8,

    /// Configuration
    config: DacConfig,

    /// Initialized flag
    initialized: AtomicBool,

    /// Current channel 1 value
    channel1_value: AtomicU32,

    /// Current channel 2 value
    channel2_value: AtomicU32,
}

impl Dac {
    /// Create a new DAC instance
    pub fn new(instance: u8, config: DacConfig) -> Self {
        Self {
            instance,
            config,
            initialized: AtomicBool::new(false),
            channel1_value: AtomicU32::new(0),
            channel2_value: AtomicU32::new(0),
        }
    }

    /// Get instance number
    pub fn instance(&self) -> u8 {
        self.instance
    }

    /// Initialize DAC
    pub fn init(&mut self) -> Result<(), DacError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        // In a real implementation, this would:
        // 1. Enable DAC peripheral clock
        // 2. Configure channels
        // 3. Enable output buffer if configured
        // 4. Enable DAC channels

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Write raw value to channel
    pub fn write_channel(&mut self, channel: u8, value: u16) -> Result<(), DacError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(DacError::NotInitialized);
        }

        if channel > 2 || channel == 0 {
            return Err(DacError::InvalidChannel);
        }

        let max_value = self.config.resolution.max_value();
        if value > max_value {
            return Err(DacError::ValueOutOfRange);
        }

        // In a real implementation, this would write to DAC data register

        match channel {
            1 => self.channel1_value.store(value as u32, Ordering::Relaxed),
            2 => self.channel2_value.store(value as u32, Ordering::Relaxed),
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Write voltage in millivolts to channel
    pub fn write_voltage_mv(&mut self, channel: u8, voltage_mv: u16) -> Result<(), DacError> {
        if voltage_mv > self.config.reference_voltage_mv {
            return Err(DacError::ValueOutOfRange);
        }

        let max = self.config.resolution.max_value() as u32;
        let vref = self.config.reference_voltage_mv as u32;

        // value = (voltage / vref) * max
        let value = (voltage_mv as u32 * max) / vref;

        self.write_channel(channel, value as u16)
    }

    /// Get current value of channel
    pub fn get_channel_value(&self, channel: u8) -> Result<u16, DacError> {
        if channel > 2 || channel == 0 {
            return Err(DacError::InvalidChannel);
        }

        let value = match channel {
            1 => self.channel1_value.load(Ordering::Relaxed),
            2 => self.channel2_value.load(Ordering::Relaxed),
            _ => unreachable!(),
        };

        Ok(value as u16)
    }
}

// =============================================================================
// Errors
// =============================================================================

/// ADC error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdcError {
    /// ADC not initialized
    NotInitialized,
    /// Invalid channel number
    InvalidChannel,
    /// Conversion timeout
    Timeout,
    /// Calibration failed
    CalibrationFailed,
    /// Hardware error
    HardwareError,
}

impl core::fmt::Display for AdcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "ADC not initialized"),
            Self::InvalidChannel => write!(f, "invalid channel number"),
            Self::Timeout => write!(f, "conversion timeout"),
            Self::CalibrationFailed => write!(f, "calibration failed"),
            Self::HardwareError => write!(f, "hardware error"),
        }
    }
}

/// DAC error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DacError {
    /// DAC not initialized
    NotInitialized,
    /// Invalid channel number
    InvalidChannel,
    /// Value out of range
    ValueOutOfRange,
    /// Hardware error
    HardwareError,
}

impl core::fmt::Display for DacError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "DAC not initialized"),
            Self::InvalidChannel => write!(f, "invalid channel number"),
            Self::ValueOutOfRange => write!(f, "value out of range"),
            Self::HardwareError => write!(f, "hardware error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::format;

    #[test]
    fn test_resolution_max_value() {
        assert_eq!(Resolution::Bits6.max_value(), 63);
        assert_eq!(Resolution::Bits8.max_value(), 255);
        assert_eq!(Resolution::Bits10.max_value(), 1023);
        assert_eq!(Resolution::Bits12.max_value(), 4095);
        assert_eq!(Resolution::Bits16.max_value(), 65535);
    }

    #[test]
    fn test_resolution_bits() {
        assert_eq!(Resolution::Bits6.bits(), 6);
        assert_eq!(Resolution::Bits12.bits(), 12);
        assert_eq!(Resolution::Bits16.bits(), 16);
    }

    #[test]
    fn test_sampling_time_cycles() {
        assert!((SamplingTime::Cycles1_5.cycles() - 1.5).abs() < 0.01);
        assert!((SamplingTime::Cycles239_5.cycles() - 239.5).abs() < 0.01);
    }

    #[test]
    fn test_adc_config_default() {
        let config = AdcConfig::default();
        assert_eq!(config.resolution(), Resolution::Bits12);
        assert_eq!(config.reference_voltage_mv(), 3300);
    }

    #[test]
    fn test_adc_config_builder() {
        let config = AdcConfig::default()
            .with_resolution(Resolution::Bits10)
            .with_conversion_mode(ConversionMode::Continuous)
            .with_sampling_time(SamplingTime::Cycles71_5)
            .with_reference_voltage_mv(5000)
            .with_oversampling(16);

        assert_eq!(config.resolution(), Resolution::Bits10);
        assert_eq!(config.reference_voltage_mv(), 5000);
    }

    #[test]
    fn test_adc_creation() {
        let config = AdcConfig::default();
        let adc = Adc::new(0, config);
        assert_eq!(adc.instance(), 0);
        assert_eq!(adc.conversion_count(), 0);
    }

    #[test]
    fn test_adc_init() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);
        assert!(adc.init().is_ok());
    }

    #[test]
    fn test_adc_enable_disable_channel() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);
        adc.init().unwrap();

        assert!(!adc.is_channel_enabled(0));

        adc.enable_channel(0).unwrap();
        assert!(adc.is_channel_enabled(0));

        adc.disable_channel(0).unwrap();
        assert!(!adc.is_channel_enabled(0));
    }

    #[test]
    fn test_adc_invalid_channel() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);
        adc.init().unwrap();

        assert_eq!(adc.enable_channel(32), Err(AdcError::InvalidChannel));
        assert_eq!(adc.read_channel(32), Err(AdcError::InvalidChannel));
    }

    #[test]
    fn test_adc_read_not_initialized() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);

        assert_eq!(adc.read_channel(0), Err(AdcError::NotInitialized));
    }

    #[test]
    fn test_adc_read_channel() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);
        adc.init().unwrap();

        let result = adc.read_channel(0);
        assert!(result.is_ok());
        assert_eq!(adc.conversion_count(), 1);
    }

    #[test]
    fn test_adc_read_voltage() {
        let config = AdcConfig::default()
            .with_resolution(Resolution::Bits12)
            .with_reference_voltage_mv(3300);
        let mut adc = Adc::new(0, config);
        adc.init().unwrap();

        let voltage = adc.read_voltage_mv(0);
        assert!(voltage.is_ok());
        // Simulated value is mid-range, so voltage should be ~1650mV
        assert!((voltage.unwrap() as i32 - 1650).abs() < 10);
    }

    #[test]
    fn test_adc_read_multiple_channels() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);
        adc.init().unwrap();

        let channels = [0, 1, 2, 3];
        let results = adc.read_channels(&channels);
        assert!(results.is_ok());
        assert_eq!(results.unwrap().len(), 4);
        assert_eq!(adc.conversion_count(), 4);
    }

    #[test]
    fn test_adc_counter_reset() {
        let config = AdcConfig::default();
        let mut adc = Adc::new(0, config);
        adc.init().unwrap();

        adc.read_channel(0).unwrap();
        assert_eq!(adc.conversion_count(), 1);

        adc.reset_conversion_count();
        assert_eq!(adc.conversion_count(), 0);
    }

    #[test]
    fn test_dac_config_default() {
        let config = DacConfig::default();
        assert_eq!(config.resolution(), Resolution::Bits12);
        assert_eq!(config.reference_voltage_mv(), 3300);
    }

    #[test]
    fn test_dac_creation() {
        let config = DacConfig::default();
        let dac = Dac::new(0, config);
        assert_eq!(dac.instance(), 0);
    }

    #[test]
    fn test_dac_init() {
        let config = DacConfig::default();
        let mut dac = Dac::new(0, config);
        assert!(dac.init().is_ok());
    }

    #[test]
    fn test_dac_write_channel() {
        let config = DacConfig::default();
        let mut dac = Dac::new(0, config);
        dac.init().unwrap();

        assert!(dac.write_channel(1, 2048).is_ok());
        assert_eq!(dac.get_channel_value(1).unwrap(), 2048);
    }

    #[test]
    fn test_dac_invalid_channel() {
        let config = DacConfig::default();
        let mut dac = Dac::new(0, config);
        dac.init().unwrap();

        assert_eq!(dac.write_channel(0, 100), Err(DacError::InvalidChannel));
        assert_eq!(dac.write_channel(3, 100), Err(DacError::InvalidChannel));
    }

    #[test]
    fn test_dac_value_out_of_range() {
        let config = DacConfig::default().with_resolution(Resolution::Bits12);
        let mut dac = Dac::new(0, config);
        dac.init().unwrap();

        assert_eq!(
            dac.write_channel(1, 5000), // Max is 4095 for 12-bit
            Err(DacError::ValueOutOfRange)
        );
    }

    #[test]
    fn test_dac_write_voltage() {
        let config = DacConfig::default()
            .with_resolution(Resolution::Bits12)
            .with_reference_voltage_mv(3300);
        let mut dac = Dac::new(0, config);
        dac.init().unwrap();

        assert!(dac.write_voltage_mv(1, 1650).is_ok()); // Mid-range
        let value = dac.get_channel_value(1).unwrap();
        assert!((value as i32 - 2048).abs() < 10); // Should be ~2048
    }

    #[test]
    fn test_dac_voltage_out_of_range() {
        let config = DacConfig::default().with_reference_voltage_mv(3300);
        let mut dac = Dac::new(0, config);
        dac.init().unwrap();

        assert_eq!(
            dac.write_voltage_mv(1, 5000),
            Err(DacError::ValueOutOfRange)
        );
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", AdcError::NotInitialized),
            "ADC not initialized"
        );
        assert_eq!(
            format!("{}", AdcError::InvalidChannel),
            "invalid channel number"
        );
        assert_eq!(
            format!("{}", DacError::NotInitialized),
            "DAC not initialized"
        );
        assert_eq!(
            format!("{}", DacError::ValueOutOfRange),
            "value out of range"
        );
    }
}
