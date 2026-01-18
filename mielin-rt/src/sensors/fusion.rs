//! Sensor Fusion Algorithms
//!
//! Kalman filter and complementary filter for combining multi-sensor data.

#![allow(dead_code)]

use super::motion::{Acceleration, AngularVelocity};

/// Euler angles (roll, pitch, yaw) in degrees
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EulerAngles {
    /// Roll angle (rotation around X-axis)
    pub roll: f32,
    /// Pitch angle (rotation around Y-axis)
    pub pitch: f32,
    /// Yaw angle (rotation around Z-axis)
    pub yaw: f32,
}

impl EulerAngles {
    pub fn new(roll: f32, pitch: f32, yaw: f32) -> Self {
        Self { roll, pitch, yaw }
    }

    pub fn zero() -> Self {
        Self {
            roll: 0.0,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

/// Complementary filter for sensor fusion
#[derive(Debug, Clone)]
pub struct ComplementaryFilter {
    /// Filter coefficient (0.0-1.0)
    alpha: f32,
    /// Current orientation
    orientation: EulerAngles,
    /// Last update time (ms)
    last_update_ms: u64,
}

impl ComplementaryFilter {
    pub fn new(alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            orientation: EulerAngles::zero(),
            last_update_ms: 0,
        }
    }

    pub fn update(
        &mut self,
        accel: &Acceleration,
        gyro: &AngularVelocity,
        dt_ms: u64,
    ) -> EulerAngles {
        let dt = dt_ms as f32 / 1000.0;

        // Accelerometer angles
        let accel_roll =
            libm::atanf(accel.y / libm::sqrtf(accel.x * accel.x + accel.z * accel.z)) * 57.3;
        let accel_pitch =
            libm::atanf(accel.x / libm::sqrtf(accel.y * accel.y + accel.z * accel.z)) * 57.3;

        // Gyroscope integration
        let gyro_roll = self.orientation.roll + gyro.x * dt;
        let gyro_pitch = self.orientation.pitch + gyro.y * dt;

        // Complementary filter
        self.orientation.roll = self.alpha * gyro_roll + (1.0 - self.alpha) * accel_roll;
        self.orientation.pitch = self.alpha * gyro_pitch + (1.0 - self.alpha) * accel_pitch;

        self.last_update_ms += dt_ms;
        self.orientation
    }

    pub fn orientation(&self) -> EulerAngles {
        self.orientation
    }

    pub fn reset(&mut self) {
        self.orientation = EulerAngles::zero();
        self.last_update_ms = 0;
    }
}

/// Kalman filter for sensor fusion
#[derive(Debug, Clone)]
pub struct KalmanFilter {
    /// State estimate
    state: f32,
    /// Error covariance
    error_covariance: f32,
    /// Process noise
    process_noise: f32,
    /// Measurement noise
    measurement_noise: f32,
}

impl KalmanFilter {
    pub fn new(process_noise: f32, measurement_noise: f32) -> Self {
        Self {
            state: 0.0,
            error_covariance: 1.0,
            process_noise,
            measurement_noise,
        }
    }

    pub fn update(&mut self, measurement: f32, control: f32) -> f32 {
        // Prediction
        let predicted_state = self.state + control;
        let predicted_error = self.error_covariance + self.process_noise;

        // Update
        let kalman_gain = predicted_error / (predicted_error + self.measurement_noise);
        self.state = predicted_state + kalman_gain * (measurement - predicted_state);
        self.error_covariance = (1.0 - kalman_gain) * predicted_error;

        self.state
    }

    pub fn state(&self) -> f32 {
        self.state
    }

    pub fn reset(&mut self) {
        self.state = 0.0;
        self.error_covariance = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complementary_filter() {
        let mut filter = ComplementaryFilter::new(0.98);
        let accel = Acceleration::new(0.0, 0.0, 1.0);
        let gyro = AngularVelocity::new(0.0, 0.0, 0.0);

        let angles = filter.update(&accel, &gyro, 10);
        assert!(angles.roll.abs() < 5.0);
        assert!(angles.pitch.abs() < 5.0);
    }

    #[test]
    fn test_kalman_filter() {
        let mut filter = KalmanFilter::new(0.01, 0.1);
        let filtered = filter.update(10.0, 0.0);
        assert!(filtered > 0.0);
    }
}
