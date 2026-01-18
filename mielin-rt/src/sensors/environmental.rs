//! Environmental Sensor Module
//!
//! Pressure, light, and air quality sensor abstractions.

#![allow(dead_code)]

use super::{Sensor, SensorError, SensorReading};

/// Barometric pressure sensor
pub type BarometricPressure = f32; // in hPa (hectopascals)

/// Light level sensor
pub type LightLevel = f32; // in lux

/// Pressure sensor trait
pub trait PressureSensor: Sensor<Reading = SensorReading<BarometricPressure>> {
    fn read_pressure(&mut self) -> Result<f32, SensorError> {
        Ok(self.read()?.value)
    }

    /// Calculate altitude from pressure (meters)
    fn calculate_altitude(&mut self, sea_level_pressure: f32) -> Result<f32, SensorError> {
        let pressure = self.read_pressure()?;
        let altitude = 44330.0 * (1.0 - libm::powf(pressure / sea_level_pressure, 0.1903));
        Ok(altitude)
    }
}

/// Light sensor trait
pub trait LightSensor: Sensor<Reading = SensorReading<LightLevel>> {
    fn read_light_level(&mut self) -> Result<f32, SensorError> {
        Ok(self.read()?.value)
    }
}

#[cfg(test)]
mod tests {
    // Tests would go here
}
