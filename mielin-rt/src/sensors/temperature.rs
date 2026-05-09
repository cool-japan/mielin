//! Temperature Sensor Module
//!
//! Temperature measurement abstractions and common sensor drivers.
//!
//! ## Supported Sensors
//!
//! - Digital temperature sensors (I2C/SPI)
//! - Analog temperature sensors (ADC)
//! - Thermocouples (with cold junction compensation)
//! - RTD (Resistance Temperature Detectors)
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::sensors::temperature::{TemperatureSensor, TemperatureUnit};
//!
//! // Read temperature
//! // let temp = sensor.read_temperature(TemperatureUnit::Celsius)?;
//! ```

#![allow(dead_code)]

use super::{Sensor, SensorError, SensorReading};

/// Temperature units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperatureUnit {
    /// Celsius (°C)
    Celsius,
    /// Fahrenheit (°F)
    Fahrenheit,
    /// Kelvin (K)
    Kelvin,
}

/// Temperature reading
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Temperature {
    /// Temperature in Celsius
    celsius: f32,
}

impl Temperature {
    /// Create temperature from Celsius
    pub fn from_celsius(celsius: f32) -> Self {
        Self { celsius }
    }

    /// Create temperature from Fahrenheit
    pub fn from_fahrenheit(fahrenheit: f32) -> Self {
        Self {
            celsius: (fahrenheit - 32.0) * 5.0 / 9.0,
        }
    }

    /// Create temperature from Kelvin
    pub fn from_kelvin(kelvin: f32) -> Self {
        Self {
            celsius: kelvin - 273.15,
        }
    }

    /// Get temperature in Celsius
    pub fn celsius(&self) -> f32 {
        self.celsius
    }

    /// Get temperature in Fahrenheit
    pub fn fahrenheit(&self) -> f32 {
        self.celsius * 9.0 / 5.0 + 32.0
    }

    /// Get temperature in Kelvin
    pub fn kelvin(&self) -> f32 {
        self.celsius + 273.15
    }

    /// Get temperature in specified unit
    pub fn as_unit(&self, unit: TemperatureUnit) -> f32 {
        match unit {
            TemperatureUnit::Celsius => self.celsius(),
            TemperatureUnit::Fahrenheit => self.fahrenheit(),
            TemperatureUnit::Kelvin => self.kelvin(),
        }
    }
}

/// Temperature sensor trait
pub trait TemperatureSensor: Sensor<Reading = SensorReading<Temperature>> {
    /// Read temperature in Celsius
    fn read_celsius(&mut self) -> Result<f32, SensorError> {
        Ok(self.read()?.value.celsius())
    }

    /// Read temperature in Fahrenheit
    fn read_fahrenheit(&mut self) -> Result<f32, SensorError> {
        Ok(self.read()?.value.fahrenheit())
    }

    /// Read temperature in Kelvin
    fn read_kelvin(&mut self) -> Result<f32, SensorError> {
        Ok(self.read()?.value.kelvin())
    }

    /// Get temperature resolution (in Celsius)
    fn resolution(&self) -> f32;

    /// Get temperature range
    fn range(&self) -> (f32, f32);
}

/// Digital temperature sensor (I2C/SPI)
#[derive(Debug)]
pub struct DigitalTemperatureSensor {
    /// Sensor initialized
    initialized: bool,
    /// Last reading
    last_reading: Option<Temperature>,
    /// Simulated temperature (for testing)
    simulated_temp: f32,
    /// Resolution in Celsius
    resolution: f32,
    /// Measurement range (min, max) in Celsius
    range: (f32, f32),
}

impl DigitalTemperatureSensor {
    /// Create a new digital temperature sensor
    pub fn new() -> Self {
        Self {
            initialized: false,
            last_reading: None,
            simulated_temp: 25.0,
            resolution: 0.0625, // Typical for DS18B20
            range: (-55.0, 125.0),
        }
    }

    /// Set simulated temperature (for testing)
    pub fn set_simulated_temperature(&mut self, temp: f32) {
        self.simulated_temp = temp;
    }

    /// Configure resolution
    pub fn set_resolution(&mut self, resolution: f32) {
        self.resolution = resolution;
    }

    /// Configure range
    pub fn set_range(&mut self, min: f32, max: f32) {
        self.range = (min, max);
    }
}

impl Default for DigitalTemperatureSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for DigitalTemperatureSensor {
    type Reading = SensorReading<Temperature>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        // Simulate reading from hardware
        let temp = Temperature::from_celsius(self.simulated_temp);

        // Check range
        if self.simulated_temp < self.range.0 || self.simulated_temp > self.range.1 {
            return Err(SensorError::OutOfRange);
        }

        self.last_reading = Some(temp);

        Ok(SensorReading::new(temp, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        self.last_reading = None;
        Ok(())
    }
}

impl TemperatureSensor for DigitalTemperatureSensor {
    fn resolution(&self) -> f32 {
        self.resolution
    }

    fn range(&self) -> (f32, f32) {
        self.range
    }
}

/// Analog temperature sensor (NTC thermistor, LM35, etc.)
#[derive(Debug)]
pub struct AnalogTemperatureSensor {
    /// Sensor initialized
    initialized: bool,
    /// ADC reference voltage (mV)
    vref_mv: u16,
    /// Sensor type
    sensor_type: AnalogSensorType,
    /// Simulated ADC value
    simulated_adc: u16,
    /// Resolution
    resolution: f32,
}

/// Analog sensor type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalogSensorType {
    /// LM35 (10mV/°C)
    LM35,
    /// TMP36 (10mV/°C, 750mV at 25°C)
    TMP36,
    /// NTC thermistor (requires Beta parameter)
    NTC { beta: u16, r25: u32 },
}

impl AnalogTemperatureSensor {
    /// Create a new analog temperature sensor
    pub fn new(sensor_type: AnalogSensorType, vref_mv: u16) -> Self {
        Self {
            initialized: false,
            vref_mv,
            sensor_type,
            simulated_adc: 2048, // Mid-range for 12-bit ADC
            resolution: 0.1,
        }
    }

    /// Set simulated ADC value (for testing)
    pub fn set_simulated_adc(&mut self, adc_value: u16) {
        self.simulated_adc = adc_value;
    }

    /// Convert ADC value to temperature
    fn adc_to_temperature(&self, adc_value: u16) -> f32 {
        // Assume 12-bit ADC (0-4095)
        let voltage_mv = (adc_value as u32 * self.vref_mv as u32) / 4096;

        match self.sensor_type {
            AnalogSensorType::LM35 => {
                // LM35: 10mV/°C
                voltage_mv as f32 / 10.0
            }
            AnalogSensorType::TMP36 => {
                // TMP36: 10mV/°C, 750mV offset at 25°C
                (voltage_mv as f32 - 750.0) / 10.0 + 25.0
            }
            AnalogSensorType::NTC { beta, r25 } => {
                // Simplified NTC calculation
                // Assuming voltage divider with R1 = R25
                let v_out = voltage_mv as f32;
                let v_in = self.vref_mv as f32;
                let r_ntc = r25 as f32 * (v_in / v_out - 1.0);

                // Steinhart-Hart equation (simplified)
                let t0 = 298.15; // 25°C in Kelvin
                let r0 = r25 as f32;
                let beta_f = beta as f32;

                let temp_k = 1.0 / ((1.0 / t0) + (1.0 / beta_f) * libm::logf(r_ntc / r0));
                temp_k - 273.15
            }
        }
    }
}

impl Sensor for AnalogTemperatureSensor {
    type Reading = SensorReading<Temperature>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        let temp_c = self.adc_to_temperature(self.simulated_adc);
        let temp = Temperature::from_celsius(temp_c);

        Ok(SensorReading::new(temp, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        Ok(())
    }
}

impl TemperatureSensor for AnalogTemperatureSensor {
    fn resolution(&self) -> f32 {
        self.resolution
    }

    fn range(&self) -> (f32, f32) {
        match self.sensor_type {
            AnalogSensorType::LM35 => (-55.0, 150.0),
            AnalogSensorType::TMP36 => (-40.0, 125.0),
            AnalogSensorType::NTC { .. } => (-40.0, 125.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_conversions() {
        let temp = Temperature::from_celsius(25.0);
        assert_eq!(temp.celsius(), 25.0);
        assert!((temp.fahrenheit() - 77.0).abs() < 0.01);
        assert!((temp.kelvin() - 298.15).abs() < 0.01);
    }

    #[test]
    fn test_temperature_from_fahrenheit() {
        let temp = Temperature::from_fahrenheit(32.0);
        assert!((temp.celsius() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_temperature_from_kelvin() {
        let temp = Temperature::from_kelvin(273.15);
        assert!((temp.celsius() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_temperature_as_unit() {
        let temp = Temperature::from_celsius(100.0);
        assert_eq!(temp.as_unit(TemperatureUnit::Celsius), 100.0);
        assert_eq!(temp.as_unit(TemperatureUnit::Fahrenheit), 212.0);
        assert!((temp.as_unit(TemperatureUnit::Kelvin) - 373.15).abs() < 0.01);
    }

    #[test]
    fn test_digital_sensor_creation() {
        let sensor = DigitalTemperatureSensor::new();
        assert!(!sensor.is_ready());
        assert_eq!(sensor.resolution(), 0.0625);
        assert_eq!(sensor.range(), (-55.0, 125.0));
    }

    #[test]
    fn test_digital_sensor_init() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        assert!(sensor.is_ready());
    }

    #[test]
    fn test_digital_sensor_read() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_temperature(25.0);

        let reading = sensor.read().expect("test setup");
        assert_eq!(reading.value.celsius(), 25.0);
        assert!(reading.is_valid());
    }

    #[test]
    fn test_digital_sensor_not_initialized() {
        let mut sensor = DigitalTemperatureSensor::new();
        let result = sensor.read();
        assert_eq!(result.unwrap_err(), SensorError::NotInitialized);
    }

    #[test]
    fn test_digital_sensor_out_of_range() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_temperature(200.0); // Above max range

        let result = sensor.read();
        assert_eq!(result.unwrap_err(), SensorError::OutOfRange);
    }

    #[test]
    fn test_digital_sensor_reset() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        sensor.reset().unwrap();
        assert!(!sensor.is_ready());
    }

    #[test]
    fn test_digital_sensor_read_celsius() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_temperature(30.0);

        let temp = sensor.read_celsius().unwrap();
        assert_eq!(temp, 30.0);
    }

    #[test]
    fn test_digital_sensor_read_fahrenheit() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_temperature(25.0);

        let temp = sensor.read_fahrenheit().unwrap();
        assert!((temp - 77.0).abs() < 0.01);
    }

    #[test]
    fn test_digital_sensor_read_kelvin() {
        let mut sensor = DigitalTemperatureSensor::new();
        sensor.init().unwrap();
        sensor.set_simulated_temperature(0.0);

        let temp = sensor.read_kelvin().unwrap();
        assert!((temp - 273.15).abs() < 0.01);
    }

    #[test]
    fn test_analog_sensor_lm35() {
        let mut sensor = AnalogTemperatureSensor::new(AnalogSensorType::LM35, 3300);
        sensor.init().unwrap();

        // LM35: 10mV/°C, so 250mV = 25°C
        // ADC = 250mV / 3300mV * 4096 = 310
        sensor.set_simulated_adc(310);

        let temp = sensor.read_celsius().unwrap();
        assert!((temp - 25.0).abs() < 1.0);
    }

    #[test]
    fn test_analog_sensor_tmp36() {
        let mut sensor = AnalogTemperatureSensor::new(AnalogSensorType::TMP36, 3300);
        sensor.init().unwrap();

        // TMP36: 750mV at 25°C
        // ADC = 750mV / 3300mV * 4096 = 930
        sensor.set_simulated_adc(930);

        let temp = sensor.read_celsius().unwrap();
        assert!((temp - 25.0).abs() < 1.0);
    }

    #[test]
    fn test_analog_sensor_ntc() {
        let sensor_type = AnalogSensorType::NTC {
            beta: 3950,
            r25: 10000,
        };
        let mut sensor = AnalogTemperatureSensor::new(sensor_type, 3300);
        sensor.init().unwrap();

        // Mid-range ADC value
        sensor.set_simulated_adc(2048);

        let reading = sensor.read();
        assert!(reading.is_ok());
    }

    #[test]
    fn test_analog_sensor_not_initialized() {
        let mut sensor = AnalogTemperatureSensor::new(AnalogSensorType::LM35, 3300);
        let result = sensor.read();
        assert_eq!(result.unwrap_err(), SensorError::NotInitialized);
    }

    #[test]
    fn test_analog_sensor_range() {
        let sensor = AnalogTemperatureSensor::new(AnalogSensorType::LM35, 3300);
        assert_eq!(sensor.range(), (-55.0, 150.0));

        let sensor_tmp = AnalogTemperatureSensor::new(AnalogSensorType::TMP36, 3300);
        assert_eq!(sensor_tmp.range(), (-40.0, 125.0));
    }
}
