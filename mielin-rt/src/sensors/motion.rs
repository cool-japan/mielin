//! Motion Sensor Module
//!
//! Accelerometer, gyroscope, and magnetometer abstractions for IMU sensors.
//!
//! ## Supported Sensors
//!
//! - 3-axis accelerometers (ADXL345, LIS3DH, MPU6050)
//! - 3-axis gyroscopes (L3GD20, MPU6050, LSM6DS3)
//! - 3-axis magnetometers (HMC5883L, LSM303, MPU9250)
//! - 6-DOF IMU (accelerometer + gyroscope)
//! - 9-DOF IMU (accelerometer + gyroscope + magnetometer)
//!
//! ## Example
//!
//! ```rust
//! use mielin_rt::sensors::motion::{Accelerometer, Vector3};
//!
//! // Read acceleration
//! // let accel = sensor.read_acceleration()?;
//! ```

#![allow(dead_code)]

use super::{Sensor, SensorError, SensorReading};

/// 3D vector for sensor readings
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector3 {
    /// X-axis value
    pub x: f32,
    /// Y-axis value
    pub y: f32,
    /// Z-axis value
    pub z: f32,
}

impl Vector3 {
    /// Create a new 3D vector
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Zero vector
    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    /// Calculate magnitude
    pub fn magnitude(&self) -> f32 {
        libm::sqrtf(self.x * self.x + self.y * self.y + self.z * self.z)
    }

    /// Normalize vector
    pub fn normalize(&self) -> Self {
        let mag = self.magnitude();
        if mag > 0.0 {
            Self {
                x: self.x / mag,
                y: self.y / mag,
                z: self.z / mag,
            }
        } else {
            *self
        }
    }

    /// Dot product
    pub fn dot(&self, other: &Vector3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product
    pub fn cross(&self, other: &Vector3) -> Vector3 {
        Vector3 {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
}

/// Acceleration measurement in g (gravity units)
pub type Acceleration = Vector3;

/// Angular velocity in degrees per second
pub type AngularVelocity = Vector3;

/// Magnetic field in microTesla (μT)
pub type MagneticField = Vector3;

/// Accelerometer measurement range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelRange {
    /// ±2g range
    G2 = 2,
    /// ±4g range
    G4 = 4,
    /// ±8g range
    G8 = 8,
    /// ±16g range
    G16 = 16,
}

impl AccelRange {
    /// Get range in g
    pub fn as_g(&self) -> u8 {
        *self as u8
    }

    /// Get sensitivity (LSB/g) for 16-bit ADC
    pub fn sensitivity(&self) -> f32 {
        match self {
            AccelRange::G2 => 16384.0,
            AccelRange::G4 => 8192.0,
            AccelRange::G8 => 4096.0,
            AccelRange::G16 => 2048.0,
        }
    }
}

/// Gyroscope measurement range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GyroRange {
    /// ±250 deg/s range
    Dps250 = 250,
    /// ±500 deg/s range
    Dps500 = 500,
    /// ±1000 deg/s range
    Dps1000 = 1000,
    /// ±2000 deg/s range
    Dps2000 = 2000,
}

impl GyroRange {
    /// Get range in deg/s
    pub fn as_dps(&self) -> u16 {
        *self as u16
    }

    /// Get sensitivity (LSB/deg/s) for 16-bit ADC
    pub fn sensitivity(&self) -> f32 {
        match self {
            GyroRange::Dps250 => 131.0,
            GyroRange::Dps500 => 65.5,
            GyroRange::Dps1000 => 32.8,
            GyroRange::Dps2000 => 16.4,
        }
    }
}

/// Accelerometer trait
pub trait Accelerometer: Sensor<Reading = SensorReading<Acceleration>> {
    /// Read acceleration in g
    fn read_acceleration(&mut self) -> Result<Acceleration, SensorError> {
        Ok(self.read()?.value)
    }

    /// Get accelerometer range
    fn accel_range(&self) -> AccelRange;

    /// Set accelerometer range
    fn set_accel_range(&mut self, range: AccelRange) -> Result<(), SensorError>;

    /// Calculate tilt angles (pitch, roll) in degrees
    fn calculate_tilt(&mut self) -> Result<(f32, f32), SensorError> {
        let accel = self.read_acceleration()?;

        // Calculate pitch and roll from accelerometer
        let pitch =
            libm::atanf(accel.x / libm::sqrtf(accel.y * accel.y + accel.z * accel.z)) * 57.3;
        let roll = libm::atanf(accel.y / libm::sqrtf(accel.x * accel.x + accel.z * accel.z)) * 57.3;

        Ok((pitch, roll))
    }
}

/// Gyroscope trait
pub trait Gyroscope: Sensor<Reading = SensorReading<AngularVelocity>> {
    /// Read angular velocity in deg/s
    fn read_angular_velocity(&mut self) -> Result<AngularVelocity, SensorError> {
        Ok(self.read()?.value)
    }

    /// Get gyroscope range
    fn gyro_range(&self) -> GyroRange;

    /// Set gyroscope range
    fn set_gyro_range(&mut self, range: GyroRange) -> Result<(), SensorError>;
}

/// Magnetometer trait
pub trait Magnetometer: Sensor<Reading = SensorReading<MagneticField>> {
    /// Read magnetic field in μT
    fn read_magnetic_field(&mut self) -> Result<MagneticField, SensorError> {
        Ok(self.read()?.value)
    }

    /// Calculate heading (yaw) in degrees (0-360)
    fn calculate_heading(&mut self) -> Result<f32, SensorError> {
        let mag = self.read_magnetic_field()?;

        // Calculate heading from magnetometer
        let heading = libm::atan2f(mag.y, mag.x) * 57.3;

        // Normalize to 0-360
        let normalized = if heading < 0.0 {
            heading + 360.0
        } else {
            heading
        };

        Ok(normalized)
    }
}

/// 3-axis accelerometer sensor
#[derive(Debug)]
pub struct AccelerometerSensor {
    /// Sensor initialized
    initialized: bool,
    /// Measurement range
    range: AccelRange,
    /// Simulated acceleration
    simulated_accel: Acceleration,
    /// Calibration offset
    offset: Vector3,
}

impl AccelerometerSensor {
    /// Create a new accelerometer
    pub fn new(range: AccelRange) -> Self {
        Self {
            initialized: false,
            range,
            simulated_accel: Acceleration::new(0.0, 0.0, 1.0), // Default: 1g on Z (upright)
            offset: Vector3::zero(),
        }
    }

    /// Set simulated acceleration (for testing)
    pub fn set_simulated_acceleration(&mut self, accel: Acceleration) {
        self.simulated_accel = accel;
    }

    /// Set calibration offset
    pub fn set_offset(&mut self, offset: Vector3) {
        self.offset = offset;
    }
}

impl Sensor for AccelerometerSensor {
    type Reading = SensorReading<Acceleration>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        // Apply offset calibration
        let accel = Acceleration::new(
            self.simulated_accel.x - self.offset.x,
            self.simulated_accel.y - self.offset.y,
            self.simulated_accel.z - self.offset.z,
        );

        Ok(SensorReading::new(accel, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        Ok(())
    }
}

impl Accelerometer for AccelerometerSensor {
    fn accel_range(&self) -> AccelRange {
        self.range
    }

    fn set_accel_range(&mut self, range: AccelRange) -> Result<(), SensorError> {
        self.range = range;
        Ok(())
    }
}

/// 3-axis gyroscope sensor
#[derive(Debug)]
pub struct GyroscopeSensor {
    /// Sensor initialized
    initialized: bool,
    /// Measurement range
    range: GyroRange,
    /// Simulated angular velocity
    simulated_gyro: AngularVelocity,
    /// Calibration offset (bias)
    offset: Vector3,
}

impl GyroscopeSensor {
    /// Create a new gyroscope
    pub fn new(range: GyroRange) -> Self {
        Self {
            initialized: false,
            range,
            simulated_gyro: AngularVelocity::zero(),
            offset: Vector3::zero(),
        }
    }

    /// Set simulated angular velocity (for testing)
    pub fn set_simulated_angular_velocity(&mut self, gyro: AngularVelocity) {
        self.simulated_gyro = gyro;
    }

    /// Set calibration offset
    pub fn set_offset(&mut self, offset: Vector3) {
        self.offset = offset;
    }
}

impl Sensor for GyroscopeSensor {
    type Reading = SensorReading<AngularVelocity>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        // Apply offset calibration
        let gyro = AngularVelocity::new(
            self.simulated_gyro.x - self.offset.x,
            self.simulated_gyro.y - self.offset.y,
            self.simulated_gyro.z - self.offset.z,
        );

        Ok(SensorReading::new(gyro, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        Ok(())
    }
}

impl Gyroscope for GyroscopeSensor {
    fn gyro_range(&self) -> GyroRange {
        self.range
    }

    fn set_gyro_range(&mut self, range: GyroRange) -> Result<(), SensorError> {
        self.range = range;
        Ok(())
    }
}

/// 3-axis magnetometer sensor
#[derive(Debug)]
pub struct MagnetometerSensor {
    /// Sensor initialized
    initialized: bool,
    /// Simulated magnetic field
    simulated_mag: MagneticField,
    /// Calibration offset (hard iron)
    hard_iron_offset: Vector3,
    /// Calibration scale (soft iron)
    soft_iron_scale: Vector3,
}

impl MagnetometerSensor {
    /// Create a new magnetometer
    pub fn new() -> Self {
        Self {
            initialized: false,
            simulated_mag: MagneticField::new(20.0, 0.0, 40.0), // Typical Earth field
            hard_iron_offset: Vector3::zero(),
            soft_iron_scale: Vector3::new(1.0, 1.0, 1.0),
        }
    }

    /// Set simulated magnetic field (for testing)
    pub fn set_simulated_magnetic_field(&mut self, mag: MagneticField) {
        self.simulated_mag = mag;
    }

    /// Set hard iron offset
    pub fn set_hard_iron_offset(&mut self, offset: Vector3) {
        self.hard_iron_offset = offset;
    }

    /// Set soft iron scale
    pub fn set_soft_iron_scale(&mut self, scale: Vector3) {
        self.soft_iron_scale = scale;
    }
}

impl Default for MagnetometerSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl Sensor for MagnetometerSensor {
    type Reading = SensorReading<MagneticField>;

    fn init(&mut self) -> Result<(), SensorError> {
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self) -> Result<Self::Reading, SensorError> {
        if !self.initialized {
            return Err(SensorError::NotInitialized);
        }

        // Apply calibration
        let mag = MagneticField::new(
            (self.simulated_mag.x - self.hard_iron_offset.x) * self.soft_iron_scale.x,
            (self.simulated_mag.y - self.hard_iron_offset.y) * self.soft_iron_scale.y,
            (self.simulated_mag.z - self.hard_iron_offset.z) * self.soft_iron_scale.z,
        );

        Ok(SensorReading::new(mag, 0))
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn reset(&mut self) -> Result<(), SensorError> {
        self.initialized = false;
        Ok(())
    }
}

impl Magnetometer for MagnetometerSensor {
    // Uses default implementation from trait
}

/// 6-DOF IMU (accelerometer + gyroscope)
#[derive(Debug)]
pub struct Imu6Dof {
    /// Accelerometer
    accel: AccelerometerSensor,
    /// Gyroscope
    gyro: GyroscopeSensor,
}

impl Imu6Dof {
    /// Create a new 6-DOF IMU
    pub fn new(accel_range: AccelRange, gyro_range: GyroRange) -> Self {
        Self {
            accel: AccelerometerSensor::new(accel_range),
            gyro: GyroscopeSensor::new(gyro_range),
        }
    }

    /// Initialize IMU
    pub fn init(&mut self) -> Result<(), SensorError> {
        self.accel.init()?;
        self.gyro.init()?;
        Ok(())
    }

    /// Read both acceleration and angular velocity
    pub fn read_all(&mut self) -> Result<(Acceleration, AngularVelocity), SensorError> {
        let accel = self.accel.read_acceleration()?;
        let gyro = self.gyro.read_angular_velocity()?;
        Ok((accel, gyro))
    }

    /// Get mutable reference to accelerometer
    pub fn accelerometer_mut(&mut self) -> &mut AccelerometerSensor {
        &mut self.accel
    }

    /// Get mutable reference to gyroscope
    pub fn gyroscope_mut(&mut self) -> &mut GyroscopeSensor {
        &mut self.gyro
    }
}

/// 9-DOF IMU (accelerometer + gyroscope + magnetometer)
#[derive(Debug)]
pub struct Imu9Dof {
    /// Accelerometer
    accel: AccelerometerSensor,
    /// Gyroscope
    gyro: GyroscopeSensor,
    /// Magnetometer
    mag: MagnetometerSensor,
}

impl Imu9Dof {
    /// Create a new 9-DOF IMU
    pub fn new(accel_range: AccelRange, gyro_range: GyroRange) -> Self {
        Self {
            accel: AccelerometerSensor::new(accel_range),
            gyro: GyroscopeSensor::new(gyro_range),
            mag: MagnetometerSensor::new(),
        }
    }

    /// Initialize IMU
    pub fn init(&mut self) -> Result<(), SensorError> {
        self.accel.init()?;
        self.gyro.init()?;
        self.mag.init()?;
        Ok(())
    }

    /// Read all sensors
    pub fn read_all(
        &mut self,
    ) -> Result<(Acceleration, AngularVelocity, MagneticField), SensorError> {
        let accel = self.accel.read_acceleration()?;
        let gyro = self.gyro.read_angular_velocity()?;
        let mag = self.mag.read_magnetic_field()?;
        Ok((accel, gyro, mag))
    }

    /// Get mutable reference to accelerometer
    pub fn accelerometer_mut(&mut self) -> &mut AccelerometerSensor {
        &mut self.accel
    }

    /// Get mutable reference to gyroscope
    pub fn gyroscope_mut(&mut self) -> &mut GyroscopeSensor {
        &mut self.gyro
    }

    /// Get mutable reference to magnetometer
    pub fn magnetometer_mut(&mut self) -> &mut MagnetometerSensor {
        &mut self.mag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector3_creation() {
        let v = Vector3::new(1.0, 2.0, 3.0);
        assert_eq!(v.x, 1.0);
        assert_eq!(v.y, 2.0);
        assert_eq!(v.z, 3.0);
    }

    #[test]
    fn test_vector3_magnitude() {
        let v = Vector3::new(3.0, 4.0, 0.0);
        assert_eq!(v.magnitude(), 5.0);
    }

    #[test]
    fn test_vector3_normalize() {
        let v = Vector3::new(3.0, 4.0, 0.0);
        let normalized = v.normalize();
        assert!((normalized.magnitude() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vector3_dot() {
        let v1 = Vector3::new(1.0, 2.0, 3.0);
        let v2 = Vector3::new(4.0, 5.0, 6.0);
        assert_eq!(v1.dot(&v2), 32.0);
    }

    #[test]
    fn test_vector3_cross() {
        let v1 = Vector3::new(1.0, 0.0, 0.0);
        let v2 = Vector3::new(0.0, 1.0, 0.0);
        let cross = v1.cross(&v2);
        assert_eq!(cross.x, 0.0);
        assert_eq!(cross.y, 0.0);
        assert_eq!(cross.z, 1.0);
    }

    #[test]
    fn test_accel_range_sensitivity() {
        assert_eq!(AccelRange::G2.sensitivity(), 16384.0);
        assert_eq!(AccelRange::G4.sensitivity(), 8192.0);
    }

    #[test]
    fn test_gyro_range_sensitivity() {
        assert_eq!(GyroRange::Dps250.sensitivity(), 131.0);
        assert_eq!(GyroRange::Dps500.sensitivity(), 65.5);
    }

    #[test]
    fn test_accelerometer_creation() {
        let accel = AccelerometerSensor::new(AccelRange::G2);
        assert!(!accel.is_ready());
        assert_eq!(accel.accel_range(), AccelRange::G2);
    }

    #[test]
    fn test_accelerometer_init() {
        let mut accel = AccelerometerSensor::new(AccelRange::G2);
        accel.init().unwrap();
        assert!(accel.is_ready());
    }

    #[test]
    fn test_accelerometer_read() {
        let mut accel = AccelerometerSensor::new(AccelRange::G2);
        accel.init().unwrap();
        accel.set_simulated_acceleration(Acceleration::new(0.0, 0.0, 1.0));

        let reading = accel.read().unwrap();
        assert_eq!(reading.value.z, 1.0);
    }

    #[test]
    fn test_accelerometer_offset_calibration() {
        let mut accel = AccelerometerSensor::new(AccelRange::G2);
        accel.init().unwrap();
        accel.set_simulated_acceleration(Acceleration::new(0.1, 0.2, 1.0));
        accel.set_offset(Vector3::new(0.1, 0.2, 0.0));

        let reading = accel.read().unwrap();
        assert!((reading.value.x - 0.0).abs() < 0.001);
        assert!((reading.value.y - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_accelerometer_calculate_tilt() {
        let mut accel = AccelerometerSensor::new(AccelRange::G2);
        accel.init().unwrap();
        accel.set_simulated_acceleration(Acceleration::new(0.0, 0.0, 1.0));

        let (pitch, roll) = accel.calculate_tilt().unwrap();
        // Upright position should have ~0 pitch and roll
        assert!(pitch.abs() < 1.0);
        assert!(roll.abs() < 1.0);
    }

    #[test]
    fn test_gyroscope_creation() {
        let gyro = GyroscopeSensor::new(GyroRange::Dps250);
        assert!(!gyro.is_ready());
        assert_eq!(gyro.gyro_range(), GyroRange::Dps250);
    }

    #[test]
    fn test_gyroscope_init() {
        let mut gyro = GyroscopeSensor::new(GyroRange::Dps250);
        gyro.init().unwrap();
        assert!(gyro.is_ready());
    }

    #[test]
    fn test_gyroscope_read() {
        let mut gyro = GyroscopeSensor::new(GyroRange::Dps250);
        gyro.init().unwrap();
        gyro.set_simulated_angular_velocity(AngularVelocity::new(10.0, 20.0, 30.0));

        let reading = gyro.read().unwrap();
        assert_eq!(reading.value.x, 10.0);
        assert_eq!(reading.value.y, 20.0);
        assert_eq!(reading.value.z, 30.0);
    }

    #[test]
    fn test_magnetometer_creation() {
        let mag = MagnetometerSensor::new();
        assert!(!mag.is_ready());
    }

    #[test]
    fn test_magnetometer_init() {
        let mut mag = MagnetometerSensor::new();
        mag.init().unwrap();
        assert!(mag.is_ready());
    }

    #[test]
    fn test_magnetometer_read() {
        let mut mag = MagnetometerSensor::new();
        mag.init().unwrap();
        mag.set_simulated_magnetic_field(MagneticField::new(20.0, 10.0, 40.0));

        let reading = mag.read().unwrap();
        assert_eq!(reading.value.x, 20.0);
        assert_eq!(reading.value.y, 10.0);
        assert_eq!(reading.value.z, 40.0);
    }

    #[test]
    fn test_magnetometer_calculate_heading() {
        let mut mag = MagnetometerSensor::new();
        mag.init().unwrap();

        // North (X positive)
        mag.set_simulated_magnetic_field(MagneticField::new(20.0, 0.0, 0.0));
        let heading = mag.calculate_heading().unwrap();
        assert!((heading - 0.0).abs() < 1.0 || (heading - 360.0).abs() < 1.0);

        // East (Y positive)
        mag.set_simulated_magnetic_field(MagneticField::new(0.0, 20.0, 0.0));
        let heading = mag.calculate_heading().unwrap();
        assert!((heading - 90.0).abs() < 1.0);
    }

    #[test]
    fn test_imu6dof_creation() {
        let imu = Imu6Dof::new(AccelRange::G2, GyroRange::Dps250);
        // Check creation works
        assert!(!imu.accel.is_ready());
    }

    #[test]
    fn test_imu6dof_init() {
        let mut imu = Imu6Dof::new(AccelRange::G2, GyroRange::Dps250);
        imu.init().unwrap();
        assert!(imu.accel.is_ready());
        assert!(imu.gyro.is_ready());
    }

    #[test]
    fn test_imu6dof_read_all() {
        let mut imu = Imu6Dof::new(AccelRange::G2, GyroRange::Dps250);
        imu.init().unwrap();

        imu.accelerometer_mut()
            .set_simulated_acceleration(Acceleration::new(0.0, 0.0, 1.0));
        imu.gyroscope_mut()
            .set_simulated_angular_velocity(AngularVelocity::new(5.0, 10.0, 15.0));

        let (accel, gyro) = imu.read_all().unwrap();
        assert_eq!(accel.z, 1.0);
        assert_eq!(gyro.x, 5.0);
    }

    #[test]
    fn test_imu9dof_creation() {
        let imu = Imu9Dof::new(AccelRange::G2, GyroRange::Dps250);
        assert!(!imu.accel.is_ready());
    }

    #[test]
    fn test_imu9dof_init() {
        let mut imu = Imu9Dof::new(AccelRange::G2, GyroRange::Dps250);
        imu.init().unwrap();
        assert!(imu.accel.is_ready());
        assert!(imu.gyro.is_ready());
        assert!(imu.mag.is_ready());
    }

    #[test]
    fn test_imu9dof_read_all() {
        let mut imu = Imu9Dof::new(AccelRange::G2, GyroRange::Dps250);
        imu.init().unwrap();

        let (accel, gyro, mag) = imu.read_all().unwrap();
        // Default simulated values
        assert!(accel.z > 0.0);
        assert!(gyro.magnitude() >= 0.0);
        assert!(mag.magnitude() > 0.0);
    }
}
