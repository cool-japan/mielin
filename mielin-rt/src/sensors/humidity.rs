//! Humidity Sensor Module
//!
//! Relative humidity measurement abstractions and sensor drivers.
//!
//! ## Supported Sensors
//!
//! - Capacitive humidity sensors (DHT series, SHT3x, BME280)
//! - Resistive humidity sensors
//! - Combined temperature/humidity sensors
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::sensors::humidity::HumiditySensor;
//!
//! // Read relative humidity
//! // let rh = sensor.read_humidity()?;
//! ```

#![allow(dead_code)]

use super::{temperature::Temperature, Sensor, SensorError, SensorReading};

/// Relative humidity reading (0-100%)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Humidity {
    /// Relative humidity percentage
    rh_percent: f32,
}

impl Humidity {
    /// Create humidity from percentage
    pub fn from_percent(rh_percent: f32) -> Result<Self, SensorError> {
        if !(0.0..=100.0).contains(&rh_percent) {
            return Err(SensorError::OutOfRange);
        }

        Ok(Self { rh_percent })
    }

    /// Get relative humidity percentage
    pub fn percent(&self) -> f32 {
        self.rh_percent
    }

    /// Calculate dew point (°C) given temperature
    pub fn dew_point(&self, temperature_c: f32) -> f32 {
        // Magnus formula
        let a = 17.27;
        let b = 237.7;

        let alpha =
            ((a * temperature_c) / (b + temperature_c)) + libm::logf(self.rh_percent / 100.0);
        (b * alpha) / (a - alpha)
    }

    /// Calculate absolute humidity (g/m³) given temperature
    pub fn absolute_humidity(&self, temperature_c: f32) -> f32 {
        // Simplified calculation
        let rh = self.rh_percent / 100.0;
        let temp_k = temperature_c + 273.15;

        // Saturation vapor pressure (Magnus formula)
        let svp = 6.112 * libm::expf((17.67 * temperature_c) / (temperature_c + 243.5));

        // Actual vapor pressure
        let avp = rh * svp;

        // Absolute humidity
        (avp * 216.7) / temp_k
    }
}

/// Humidity sensor trait
pub trait HumiditySensor: Sensor<Reading = SensorReading<Humidity>> {
    /// Read relative humidity percentage
    fn read_humidity(&mut self) -> Result<f32, SensorError> {
        Ok(self.read()?.value.percent())
    }

    /// Get humidity resolution (in %)
    fn resolution(&self) -> f32;

    /// Get humidity range
    fn range(&self) -> (f32, f32);

    /// Get measurement accuracy (in %)
    fn accuracy(&self) -> f32;
}

/// Combined temperature and humidity sensor
pub trait TempHumSensor: HumiditySensor {
    /// Read both temperature and humidity
    fn read_temp_humidity(&mut self) -> Result<(Temperature, Humidity), SensorError>;
}

/// Digital humidity sensor (DHT22, SHT3x, etc.)
#[derive(Debug)]
pub struct DigitalHumiditySensor {
    /// Sensor initialized
    initialized: bool,
    /// Simulated humidity
    simulated_rh: f32,
    /// Resolution in %
    resolution: f32,
    /// Measurement accuracy in %
    accuracy: f32,
    /// Range (min, max) in %
    range: (f32, f32),
}

impl DigitalHumiditySensor {
    /// Create a new digital humidity sensor
    pub fn new() -> Self {
        Self {
            initialized: false,
            simulated_rh: 50.0,
            resolution: 0.1, // Typical for SHT3x
            accuracy: 2.0,   // ±2% typical
            range: (0.0, 100.0),
        }
    }

    /// Set simulated humidity (for testing)
    pub fn set_simulated_humidity(&mut self, rh_percent: f32) {
        self.simulated_rh = rh_percent.clamp(0.0, 100.0);
    }

    /// Configure resolution
    pub fn set_resolution(&mut self, resolution: f32) {
        self.resolution = resolution;
    }

    /// Configure accuracy
    pub fn set_accuracy(&mut self, accuracy: f32) {
        self.accuracy = accuracy;
    }
}

impl Default for DigitalHumiditySensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for DigitalHumiditySensor {
    type Reading = SensorReading<Humidity>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        let humidity = Humidity::from_percent(self.simulated_rh)?;
        Ok(SensorReading::new(humidity, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        Ok(())
    }
}

impl HumiditySensor for DigitalHumiditySensor {
    fn resolution(&self) -> f32 {
        self.resolution
    }

    fn range(&self) -> (f32, f32) {
        self.range
    }

    fn accuracy(&self) -> f32 {
        self.accuracy
    }
}

/// Combined temperature and humidity sensor (DHT22, BME280, SHT3x)
#[derive(Debug)]
pub struct CombinedTempHumSensor {
    /// Sensor initialized
    initialized: bool,
    /// Simulated temperature
    simulated_temp: f32,
    /// Simulated humidity
    simulated_rh: f32,
    /// Temperature resolution
    temp_resolution: f32,
    /// Humidity resolution
    humidity_resolution: f32,
    /// Humidity accuracy
    humidity_accuracy: f32,
}

impl CombinedTempHumSensor {
    /// Create a new combined sensor
    pub fn new() -> Self {
        Self {
            initialized: false,
            simulated_temp: 25.0,
            simulated_rh: 50.0,
            temp_resolution: 0.1,
            humidity_resolution: 0.1,
            humidity_accuracy: 2.0,
        }
    }

    /// Set simulated values (for testing)
    pub fn set_simulated_values(&mut self, temp_c: f32, rh_percent: f32) {
        self.simulated_temp = temp_c;
        self.simulated_rh = rh_percent.clamp(0.0, 100.0);
    }
}

impl Default for CombinedTempHumSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for CombinedTempHumSensor {
    type Reading = SensorReading<Humidity>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        let humidity = Humidity::from_percent(self.simulated_rh)?;
        Ok(SensorReading::new(humidity, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        Ok(())
    }
}

impl HumiditySensor for CombinedTempHumSensor {
    fn resolution(&self) -> f32 {
        self.humidity_resolution
    }

    fn range(&self) -> (f32, f32) {
        (0.0, 100.0)
    }

    fn accuracy(&self) -> f32 {
        self.humidity_accuracy
    }
}

impl TempHumSensor for CombinedTempHumSensor {
    fn read_temp_humidity(&mut self) -> Result<(Temperature, Humidity), SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        let temperature = Temperature::from_celsius(self.simulated_temp);
        let humidity = Humidity::from_percent(self.simulated_rh)?;

        Ok((temperature, humidity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_humidity_creation() {
        let rh = Humidity::from_percent(50.0).unwrap();
        assert_eq!(rh.percent(), 50.0);
    }

    #[test]
    fn test_humidity_out_of_range() {
        let result = Humidity::from_percent(150.0);
        assert_eq!(result.unwrap_err(), SensorError::OutOfRange);

        let result = Humidity::from_percent(-10.0);
        assert_eq!(result.unwrap_err(), SensorError::OutOfRange);
    }

    #[test]
    fn test_dew_point_calculation() {
        let rh = Humidity::from_percent(50.0).unwrap();
        let dew_point = rh.dew_point(25.0);

        // At 25°C and 50% RH, dew point should be around 13.9°C
        assert!((dew_point - 13.9).abs() < 1.0);
    }

    #[test]
    fn test_absolute_humidity() {
        let rh = Humidity::from_percent(50.0).unwrap();
        let abs_humidity = rh.absolute_humidity(25.0);

        // Should be around 11.5 g/m³
        assert!((abs_humidity - 11.5).abs() < 2.0);
    }

    #[test]
    fn test_digital_sensor_creation() {
        let sensor = DigitalHumiditySensor::new();
        assert!(!sensor.is_ready());
        assert_eq!(sensor.resolution(), 0.1);
        assert_eq!(sensor.accuracy(), 2.0);
    }

    #[test]
    fn test_digital_sensor_init() {
        let mut sensor = DigitalHumiditySensor::new();
        sensor.init().unwrap();
        assert!(sensor.is_ready());
    }

    #[test]
    fn test_digital_sensor_read() {
        let mut sensor = DigitalHumiditySensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_humidity(65.0);

        let reading = sensor.read().unwrap();
        assert_eq!(reading.value.percent(), 65.0);
        assert!(reading.is_valid());
    }

    #[test]
    fn test_digital_sensor_not_initialized() {
        let mut sensor = DigitalHumiditySensor::new();
        let result = sensor.read();
        assert_eq!(result.unwrap_err(), SensorError::NotInitialized);
    }

    #[test]
    fn test_digital_sensor_read_humidity() {
        let mut sensor = DigitalHumiditySensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_humidity(70.0);

        let rh = sensor.read_humidity().unwrap();
        assert_eq!(rh, 70.0);
    }

    #[test]
    fn test_combined_sensor_creation() {
        let sensor = CombinedTempHumSensor::new();
        assert!(!sensor.is_ready());
    }

    #[test]
    fn test_combined_sensor_init() {
        let mut sensor = CombinedTempHumSensor::new();
        sensor.init().unwrap();
        assert!(sensor.is_ready());
    }

    #[test]
    fn test_combined_sensor_read() {
        let mut sensor = CombinedTempHumSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_values(22.0, 55.0);

        let (temp, humidity) = sensor.read_temp_humidity().unwrap();
        assert_eq!(temp.celsius(), 22.0);
        assert_eq!(humidity.percent(), 55.0);
    }

    #[test]
    fn test_combined_sensor_not_initialized() {
        let mut sensor = CombinedTempHumSensor::new();
        let result = sensor.read_temp_humidity();
        assert_eq!(result.unwrap_err(), SensorError::NotInitialized);
    }

    #[test]
    fn test_combined_sensor_read_humidity() {
        let mut sensor = CombinedTempHumSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_values(25.0, 60.0);

        let rh = sensor.read_humidity().unwrap();
        assert_eq!(rh, 60.0);
    }

    #[test]
    fn test_humidity_clamp() {
        let mut sensor = DigitalHumiditySensor::new();
        sensor.init().unwrap();

        // Values beyond 100% should be clamped
        sensor.set_simulated_humidity(150.0);
        let reading = sensor.read().unwrap();
        assert_eq!(reading.value.percent(), 100.0);

        // Negative values should be clamped to 0
        sensor.set_simulated_humidity(-10.0);
        let reading = sensor.read().unwrap();
        assert_eq!(reading.value.percent(), 0.0);
    }

    #[test]
    fn test_combined_sensor_dew_point() {
        let mut sensor = CombinedTempHumSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_values(25.0, 50.0);

        let (temp, humidity) = sensor.read_temp_humidity().unwrap();
        let dew_point = humidity.dew_point(temp.celsius());

        assert!((dew_point - 13.9).abs() < 1.0);
    }
}
