//! Sensor Integration Module
//!
//! Comprehensive sensor abstractions and drivers for embedded IoT devices.
//! Provides unified interfaces for temperature, humidity, motion, and other sensor types.
//!
//! ## Features
//!
//! - **Temperature Sensors**: Digital and analog temperature measurement
//! - **Humidity Sensors**: Relative humidity sensing
//! - **Motion Sensors**: Accelerometer, gyroscope, magnetometer
//! - **Environmental Sensors**: Pressure, light, gas sensors
//! - **Sensor Fusion**: Kalman filter, complementary filter for multi-sensor data
//! - **Calibration**: Offset and scale calibration support
//! - **Filtering**: Moving average, exponential smoothing, median filter
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::sensors::temperature::TemperatureSensor;
//! use mielin_rt::sensors::SensorReading;
//!
//! // Read temperature sensor
//! // let mut sensor = TemperatureSensor::new();
//! // let reading = sensor.read_celsius()?;
//! ```

#![allow(dead_code)]

use core::fmt;

pub mod environmental;
pub mod fusion;
pub mod humidity;
pub mod motion;
pub mod temperature;

/// Sensor reading with timestamp and status
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorReading<T> {
    /// Reading value
    pub value: T,
    /// Timestamp (milliseconds)
    pub timestamp_ms: u64,
    /// Reading quality (0-100%)
    pub quality: u8,
    /// Sensor status
    pub status: SensorStatus,
}

impl<T> SensorReading<T> {
    /// Create a new sensor reading
    pub fn new(value: T, timestamp_ms: u64) -> Self {
        Self {
            value,
            timestamp_ms,
            quality: 100,
            status: SensorStatus::Ok,
        }
    }

    /// Create reading with quality
    pub fn with_quality(value: T, timestamp_ms: u64, quality: u8) -> Self {
        Self {
            value,
            timestamp_ms,
            quality,
            status: SensorStatus::Ok,
        }
    }

    /// Create reading with status
    pub fn with_status(value: T, timestamp_ms: u64, status: SensorStatus) -> Self {
        Self {
            value,
            timestamp_ms,
            quality: 100,
            status,
        }
    }

    /// Check if reading is valid
    pub fn is_valid(&self) -> bool {
        matches!(self.status, SensorStatus::Ok) && self.quality >= 50
    }
}

/// Sensor Status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorStatus {
    /// Sensor operating normally
    Ok,
    /// Sensor not initialized
    NotInitialized,
    /// Sensor communication error
    CommunicationError,
    /// Sensor out of range
    OutOfRange,
    /// Sensor calibration needed
    CalibrationNeeded,
    /// Sensor malfunction
    Malfunction,
}

/// Sensor trait
pub trait Sensor {
    /// Reading type
    type Reading;

    /// Initialize sensor
    fn init(&mut self) -> Result<(), SensorError>;

    /// Read sensor value
    fn read(&mut self) -> Result<Self::Reading, SensorError>;

    /// Check if sensor is ready
    fn is_ready(&self) -> bool;

    /// Reset sensor
    fn reset(&mut self) -> Result<(), SensorError>;
}

/// Calibratable sensor trait
pub trait Calibratable {
    /// Calibration data type
    type CalibrationData;

    /// Perform calibration
    fn calibrate(&mut self, data: Self::CalibrationData) -> Result<(), SensorError>;

    /// Get calibration data
    fn get_calibration(&self) -> Option<Self::CalibrationData>;

    /// Clear calibration
    fn clear_calibration(&mut self);
}

/// Sensor Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorError {
    /// Not initialized
    NotInitialized,
    /// Communication error
    CommunicationError,
    /// Invalid configuration
    InvalidConfiguration,
    /// Out of range
    OutOfRange,
    /// Calibration error
    CalibrationError,
    /// Timeout
    Timeout,
    /// Hardware fault
    HardwareFault,
    /// Not supported
    NotSupported,
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorError::NotInitialized => write!(f, "Sensor not initialized"),
            SensorError::CommunicationError => write!(f, "Sensor communication error"),
            SensorError::InvalidConfiguration => write!(f, "Invalid sensor configuration"),
            SensorError::OutOfRange => write!(f, "Sensor reading out of range"),
            SensorError::CalibrationError => write!(f, "Sensor calibration error"),
            SensorError::Timeout => write!(f, "Sensor timeout"),
            SensorError::HardwareFault => write!(f, "Sensor hardware fault"),
            SensorError::NotSupported => write!(f, "Operation not supported"),
        }
    }
}

/// Moving average filter
#[derive(Debug, Clone)]
pub struct MovingAverageFilter<const N: usize> {
    /// Sample buffer
    buffer: heapless::Vec<f32, N>,
    /// Current index
    index: usize,
    /// Sum of values
    sum: f32,
    /// Number of samples
    count: usize,
}

impl<const N: usize> MovingAverageFilter<N> {
    /// Create a new moving average filter
    pub fn new() -> Self {
        Self {
            buffer: heapless::Vec::new(),
            index: 0,
            sum: 0.0,
            count: 0,
        }
    }

    /// Add a sample
    pub fn add(&mut self, value: f32) {
        if self.buffer.len() < N {
            // Buffer not full yet
            let _ = self.buffer.push(value);
            self.sum += value;
            self.count += 1;
        } else {
            // Buffer full, replace oldest
            self.sum -= self.buffer[self.index];
            self.buffer[self.index] = value;
            self.sum += value;
            self.index = (self.index + 1) % N;
        }
    }

    /// Get average
    pub fn average(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / (self.count as f32)
        }
    }

    /// Reset filter
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.index = 0;
        self.sum = 0.0;
        self.count = 0;
    }

    /// Get sample count
    pub fn count(&self) -> usize {
        self.count
    }
}

impl<const N: usize> Default for MovingAverageFilter<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponential smoothing filter
#[derive(Debug, Clone, Copy)]
pub struct ExponentialFilter {
    /// Smoothing factor (0.0 - 1.0)
    alpha: f32,
    /// Current value
    value: f32,
    /// Initialized flag
    initialized: bool,
}

impl ExponentialFilter {
    /// Create a new exponential filter
    pub fn new(alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            value: 0.0,
            initialized: false,
        }
    }

    /// Add a sample
    pub fn add(&mut self, sample: f32) {
        if !self.initialized {
            self.value = sample;
            self.initialized = true;
        } else {
            self.value = self.alpha * sample + (1.0 - self.alpha) * self.value;
        }
    }

    /// Get filtered value
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Reset filter
    pub fn reset(&mut self) {
        self.value = 0.0;
        self.initialized = false;
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// Median filter
#[derive(Debug, Clone)]
pub struct MedianFilter<const N: usize> {
    /// Sample buffer
    buffer: heapless::Vec<f32, N>,
    /// Current index
    index: usize,
}

impl<const N: usize> MedianFilter<N> {
    /// Create a new median filter
    pub fn new() -> Self {
        Self {
            buffer: heapless::Vec::new(),
            index: 0,
        }
    }

    /// Add a sample
    pub fn add(&mut self, value: f32) {
        if self.buffer.len() < N {
            let _ = self.buffer.push(value);
        } else {
            self.buffer[self.index] = value;
            self.index = (self.index + 1) % N;
        }
    }

    /// Get median value
    pub fn median(&self) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }

        let mut sorted = heapless::Vec::<f32, N>::new();
        let _ = sorted.extend_from_slice(&self.buffer);

        // Simple bubble sort (sufficient for small N)
        let len = sorted.len();
        for i in 0..len {
            for j in 0..len - 1 - i {
                if sorted[j] > sorted[j + 1] {
                    sorted.swap(j, j + 1);
                }
            }
        }

        if len.is_multiple_of(2) {
            (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
        } else {
            sorted[len / 2]
        }
    }

    /// Reset filter
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.index = 0;
    }

    /// Get sample count
    pub fn count(&self) -> usize {
        self.buffer.len()
    }
}

impl<const N: usize> Default for MedianFilter<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_sensor_reading_creation() {
        let reading = SensorReading::new(25.5f32, 1000);
        assert_eq!(reading.value, 25.5);
        assert_eq!(reading.timestamp_ms, 1000);
        assert_eq!(reading.quality, 100);
        assert_eq!(reading.status, SensorStatus::Ok);
    }

    #[test]
    fn test_sensor_reading_with_quality() {
        let reading = SensorReading::with_quality(30.0f32, 2000, 75);
        assert_eq!(reading.quality, 75);
        assert!(reading.is_valid());
    }

    #[test]
    fn test_sensor_reading_with_status() {
        let reading = SensorReading::with_status(20.0f32, 3000, SensorStatus::CommunicationError);
        assert_eq!(reading.status, SensorStatus::CommunicationError);
        assert!(!reading.is_valid());
    }

    #[test]
    fn test_sensor_reading_validity() {
        let valid = SensorReading::new(25.0f32, 1000);
        assert!(valid.is_valid());

        let low_quality = SensorReading::with_quality(25.0f32, 1000, 30);
        assert!(!low_quality.is_valid());

        let error = SensorReading::with_status(25.0f32, 1000, SensorStatus::Malfunction);
        assert!(!error.is_valid());
    }

    #[test]
    fn test_moving_average_filter() {
        let mut filter = MovingAverageFilter::<5>::new();

        filter.add(10.0);
        assert_eq!(filter.average(), 10.0);
        assert_eq!(filter.count(), 1);

        filter.add(20.0);
        assert_eq!(filter.average(), 15.0);
        assert_eq!(filter.count(), 2);

        filter.add(30.0);
        assert_eq!(filter.average(), 20.0);
        assert_eq!(filter.count(), 3);

        filter.add(40.0);
        filter.add(50.0);
        assert_eq!(filter.average(), 30.0);
        assert_eq!(filter.count(), 5);

        // Add 6th value, should replace oldest (10.0)
        filter.add(60.0);
        assert_eq!(filter.average(), 40.0);
    }

    #[test]
    fn test_moving_average_reset() {
        let mut filter = MovingAverageFilter::<5>::new();
        filter.add(10.0);
        filter.add(20.0);

        filter.reset();
        assert_eq!(filter.count(), 0);
        assert_eq!(filter.average(), 0.0);
    }

    #[test]
    fn test_exponential_filter() {
        let mut filter = ExponentialFilter::new(0.5);
        assert!(!filter.is_initialized());

        filter.add(10.0);
        assert!(filter.is_initialized());
        assert_eq!(filter.value(), 10.0);

        filter.add(20.0);
        assert_eq!(filter.value(), 15.0);

        filter.add(30.0);
        assert_eq!(filter.value(), 22.5);
    }

    #[test]
    fn test_exponential_filter_alpha() {
        // High alpha = more weight on new samples
        let mut high_alpha = ExponentialFilter::new(0.9);
        high_alpha.add(10.0);
        high_alpha.add(20.0);
        assert_eq!(high_alpha.value(), 19.0);

        // Low alpha = more smoothing
        let mut low_alpha = ExponentialFilter::new(0.1);
        low_alpha.add(10.0);
        low_alpha.add(20.0);
        assert_eq!(low_alpha.value(), 11.0);
    }

    #[test]
    fn test_exponential_filter_reset() {
        let mut filter = ExponentialFilter::new(0.5);
        filter.add(10.0);
        filter.add(20.0);

        filter.reset();
        assert!(!filter.is_initialized());
        assert_eq!(filter.value(), 0.0);
    }

    #[test]
    fn test_median_filter() {
        let mut filter = MedianFilter::<5>::new();

        filter.add(10.0);
        assert_eq!(filter.median(), 10.0);
        assert_eq!(filter.count(), 1);

        filter.add(30.0);
        filter.add(20.0);
        assert_eq!(filter.median(), 20.0);

        filter.add(50.0);
        filter.add(40.0);
        // [10, 30, 20, 50, 40] -> sorted [10, 20, 30, 40, 50]
        assert_eq!(filter.median(), 30.0);
    }

    #[test]
    fn test_median_filter_even_count() {
        let mut filter = MedianFilter::<4>::new();
        filter.add(10.0);
        filter.add(20.0);
        filter.add(30.0);
        filter.add(40.0);
        // [10, 20, 30, 40] -> median = (20 + 30) / 2 = 25
        assert_eq!(filter.median(), 25.0);
    }

    #[test]
    fn test_median_filter_reset() {
        let mut filter = MedianFilter::<5>::new();
        filter.add(10.0);
        filter.add(20.0);

        filter.reset();
        assert_eq!(filter.count(), 0);
        assert_eq!(filter.median(), 0.0);
    }

    #[test]
    fn test_sensor_error_display() {
        let err = SensorError::CommunicationError;
        let display = format!("{}", err);
        assert!(display.contains("communication"));
    }
}
