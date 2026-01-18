//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use alloc::vec;
use alloc::vec::Vec;

/// Calibration point mapping voltage to SoC
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrationPoint {
    /// Voltage in millivolts
    pub voltage_mv: u32,
    /// State of charge percentage (0-100)
    pub soc_percent: u8,
}
impl CalibrationPoint {
    /// Create a new calibration point
    pub const fn new(voltage_mv: u32, soc_percent: u8) -> Self {
        Self {
            voltage_mv,
            soc_percent,
        }
    }
}
/// Fuel gauge reading
#[derive(Debug, Clone, Copy)]
pub struct FuelGaugeReading {
    /// State of charge (0-100%)
    pub soc_percent: u8,
    /// Remaining capacity in milliamp-hours
    pub remaining_mah: u32,
    /// Full charge capacity in milliamp-hours
    pub full_capacity_mah: u32,
    /// Design capacity in milliamp-hours
    pub design_capacity_mah: u32,
    /// Current voltage in millivolts
    pub voltage_mv: u32,
    /// Current in milliamps (positive = charging, negative = discharging)
    pub current_ma: i32,
    /// Temperature in tenths of Celsius (e.g., 250 = 25.0°C)
    pub temperature_deci_c: i16,
    /// Time to empty in minutes (0 if charging)
    pub time_to_empty_min: u16,
    /// Time to full in minutes (0 if discharging)
    pub time_to_full_min: u16,
}
impl FuelGaugeReading {
    /// Create a new fuel gauge reading
    pub fn new(soc_percent: u8, voltage_mv: u32, current_ma: i32) -> Self {
        Self {
            soc_percent,
            remaining_mah: 0,
            full_capacity_mah: 0,
            design_capacity_mah: 0,
            voltage_mv,
            current_ma,
            temperature_deci_c: 250,
            time_to_empty_min: 0,
            time_to_full_min: 0,
        }
    }
    /// Get capacity fade percentage
    pub fn capacity_fade(&self) -> u8 {
        if self.design_capacity_mah == 0 {
            return 0;
        }
        let fade = 100u32.saturating_sub((self.full_capacity_mah * 100) / self.design_capacity_mah);
        fade.min(100) as u8
    }
    /// Get temperature in Celsius
    pub fn temperature_c(&self) -> f32 {
        self.temperature_deci_c as f32 / 10.0
    }
    /// Check if battery is hot (>45°C)
    pub fn is_hot(&self) -> bool {
        self.temperature_deci_c > 450
    }
    /// Check if battery is cold (<0°C)
    pub fn is_cold(&self) -> bool {
        self.temperature_deci_c < 0
    }
    /// Get instantaneous power in milliwatts
    pub fn power_mw(&self) -> i32 {
        (self.voltage_mv as i64 * self.current_ma as i64 / 1000) as i32
    }
}
/// Battery health tracking
#[derive(Debug, Clone)]
pub struct BatteryHealth {
    /// Current health status
    pub status: HealthStatus,
    /// Charge cycle count
    pub cycle_count: u32,
    /// State of health percentage (capacity relative to design)
    pub soh_percent: u8,
    /// Internal resistance in milliohms
    pub internal_resistance_mohm: u32,
    /// Maximum temperature ever recorded (deci-Celsius)
    pub max_temperature_deci_c: i16,
    /// Minimum temperature ever recorded (deci-Celsius)
    pub min_temperature_deci_c: i16,
    /// Number of overtemperature events
    pub overtemp_events: u32,
    /// Number of deep discharge events
    pub deep_discharge_events: u32,
    /// Chemistry type
    pub chemistry: BatteryChemistry,
}
impl BatteryHealth {
    /// Create new battery health tracker
    pub fn new(chemistry: BatteryChemistry) -> Self {
        Self {
            status: HealthStatus::Unknown,
            cycle_count: 0,
            soh_percent: 100,
            internal_resistance_mohm: 0,
            max_temperature_deci_c: i16::MIN,
            min_temperature_deci_c: i16::MAX,
            overtemp_events: 0,
            deep_discharge_events: 0,
            chemistry,
        }
    }
    /// Update health based on fuel gauge reading
    pub fn update(&mut self, reading: &FuelGaugeReading) {
        if reading.temperature_deci_c > self.max_temperature_deci_c {
            self.max_temperature_deci_c = reading.temperature_deci_c;
        }
        if reading.temperature_deci_c < self.min_temperature_deci_c {
            self.min_temperature_deci_c = reading.temperature_deci_c;
        }
        if reading.is_hot() {
            self.overtemp_events += 1;
        }
        if reading.soc_percent < 5 {
            self.deep_discharge_events += 1;
        }
        if reading.design_capacity_mah > 0 && reading.full_capacity_mah > 0 {
            self.soh_percent =
                ((reading.full_capacity_mah * 100) / reading.design_capacity_mah).min(100) as u8;
        }
        self.status = self.calculate_status();
    }
    /// Increment cycle count
    pub fn record_charge_cycle(&mut self) {
        self.cycle_count += 1;
        self.status = self.calculate_status();
    }
    fn calculate_status(&self) -> HealthStatus {
        if self.max_temperature_deci_c > 600 || self.min_temperature_deci_c < -200 {
            return HealthStatus::Fault;
        }
        let expected_cycles = self.chemistry.typical_cycle_life();
        let cycle_health = if expected_cycles > 0 {
            100u32.saturating_sub((self.cycle_count * 100) / expected_cycles)
        } else {
            100
        };
        let combined = (self.soh_percent as u32 + cycle_health) / 2;
        match combined {
            80..=100 => HealthStatus::Good,
            60..=79 => HealthStatus::Fair,
            30..=59 => HealthStatus::Poor,
            _ => HealthStatus::Critical,
        }
    }
    /// Get remaining useful life estimate (0-100%)
    pub fn remaining_life_percent(&self) -> u8 {
        let expected_cycles = self.chemistry.typical_cycle_life();
        if expected_cycles == 0 {
            return 50;
        }
        let cycle_life =
            100u32.saturating_sub((self.cycle_count * 100).saturating_div(expected_cycles));
        let combined = (cycle_life + self.soh_percent as u32) / 2;
        combined.min(100) as u8
    }
}
/// Battery configuration
#[derive(Debug, Clone)]
pub struct BatteryConfig {
    /// Battery chemistry
    pub chemistry: BatteryChemistry,
    /// Number of cells in series
    pub cells_in_series: u8,
    /// Design capacity in mAh
    pub design_capacity_mah: u32,
    /// Charge current limit in mA
    pub charge_current_limit_ma: u32,
    /// Discharge current limit in mA
    pub discharge_current_limit_ma: u32,
    /// Low battery threshold (%)
    pub low_battery_percent: u8,
    /// Critical battery threshold (%)
    pub critical_battery_percent: u8,
    /// High temperature shutdown (deci-Celsius)
    pub max_temp_deci_c: i16,
    /// Low temperature shutdown (deci-Celsius)
    pub min_temp_deci_c: i16,
}
impl BatteryConfig {
    /// Create configuration for a single-cell LiPo
    pub fn single_cell_lipo(capacity_mah: u32) -> Self {
        Self {
            chemistry: BatteryChemistry::LithiumPolymer,
            cells_in_series: 1,
            design_capacity_mah: capacity_mah,
            charge_current_limit_ma: capacity_mah,
            discharge_current_limit_ma: capacity_mah * 2,
            low_battery_percent: 20,
            critical_battery_percent: 5,
            max_temp_deci_c: 450,
            min_temp_deci_c: 0,
        }
    }
    /// Create configuration for a 2S LiPo
    pub fn two_cell_lipo(capacity_mah: u32) -> Self {
        let mut config = Self::single_cell_lipo(capacity_mah);
        config.cells_in_series = 2;
        config
    }
    /// Create configuration for a 3S LiPo
    pub fn three_cell_lipo(capacity_mah: u32) -> Self {
        let mut config = Self::single_cell_lipo(capacity_mah);
        config.cells_in_series = 3;
        config
    }
    /// Get full pack voltage in millivolts
    pub fn full_voltage_mv(&self) -> u32 {
        self.chemistry.full_voltage_mv() * self.cells_in_series as u32
    }
    /// Get empty pack voltage in millivolts
    pub fn empty_voltage_mv(&self) -> u32 {
        self.chemistry.empty_voltage_mv() * self.cells_in_series as u32
    }
    /// Get nominal pack voltage in millivolts
    pub fn nominal_voltage_mv(&self) -> u32 {
        self.chemistry.nominal_voltage_mv() * self.cells_in_series as u32
    }
    /// Calculate SoC from voltage (simple linear approximation)
    pub fn voltage_to_soc(&self, voltage_mv: u32) -> u8 {
        let full = self.full_voltage_mv();
        let empty = self.empty_voltage_mv();
        if voltage_mv >= full {
            return 100;
        }
        if voltage_mv <= empty {
            return 0;
        }
        let range = full - empty;
        let above_empty = voltage_mv - empty;
        ((above_empty * 100) / range) as u8
    }
}
/// Battery manager
#[derive(Debug)]
pub struct BatteryManager {
    /// Configuration
    config: BatteryConfig,
    /// Last fuel gauge reading
    last_reading: FuelGaugeReading,
    /// Charging state
    charging_state: ChargingState,
    /// Health tracker
    health: BatteryHealth,
    /// Power consumption tracker
    consumption: PowerConsumption,
    /// Reading history for averaging
    soc_history: Vec<u8>,
    /// History size
    history_size: usize,
    /// Charge cycle tracking
    was_full: bool,
    was_empty: bool,
    /// Timestamp of last update (microseconds)
    last_update_us: u64,
}
impl BatteryManager {
    /// Create a new battery manager
    pub fn new(config: BatteryConfig) -> Self {
        let chemistry = config.chemistry;
        Self {
            config,
            last_reading: FuelGaugeReading::default(),
            charging_state: ChargingState::Unknown,
            health: BatteryHealth::new(chemistry),
            consumption: PowerConsumption::new(),
            soc_history: Vec::with_capacity(10),
            history_size: 10,
            was_full: false,
            was_empty: false,
            last_update_us: 0,
        }
    }
    /// Update with new fuel gauge reading
    pub fn update(&mut self, reading: FuelGaugeReading, timestamp_us: u64) {
        if self.last_update_us > 0 {
            let duration_us = timestamp_us.saturating_sub(self.last_update_us);
            let power_uw = reading.power_mw().unsigned_abs() as u64 * 1000;
            self.consumption.record_sample(power_uw, duration_us);
        }
        self.charging_state = if reading.current_ma > 50 {
            if reading.soc_percent >= 99 {
                ChargingState::Full
            } else {
                ChargingState::Charging
            }
        } else if reading.current_ma < -50 {
            ChargingState::Discharging
        } else if reading.soc_percent >= 99 {
            ChargingState::Full
        } else {
            ChargingState::NotCharging
        };
        if reading.soc_percent >= 95 && !self.was_full {
            self.was_full = true;
        }
        if reading.soc_percent <= 10 && !self.was_empty {
            self.was_empty = true;
        }
        if self.was_full && self.was_empty {
            self.health.record_charge_cycle();
            self.was_full = false;
            self.was_empty = false;
        }
        self.health.update(&reading);
        if self.soc_history.len() >= self.history_size {
            self.soc_history.remove(0);
        }
        self.soc_history.push(reading.soc_percent);
        self.last_reading = reading;
        self.last_update_us = timestamp_us;
    }
    /// Get current state of charge
    pub fn soc_percent(&self) -> u8 {
        self.last_reading.soc_percent
    }
    /// Get averaged state of charge
    pub fn soc_averaged(&self) -> u8 {
        if self.soc_history.is_empty() {
            return self.last_reading.soc_percent;
        }
        let sum: u32 = self.soc_history.iter().map(|&x| x as u32).sum();
        (sum / self.soc_history.len() as u32) as u8
    }
    /// Get current voltage
    pub fn voltage_mv(&self) -> u32 {
        self.last_reading.voltage_mv
    }
    /// Get current (positive = charging)
    pub fn current_ma(&self) -> i32 {
        self.last_reading.current_ma
    }
    /// Get charging state
    pub fn charging_state(&self) -> ChargingState {
        self.charging_state
    }
    /// Get health status
    pub fn health(&self) -> &BatteryHealth {
        &self.health
    }
    /// Get power consumption stats
    pub fn consumption(&self) -> &PowerConsumption {
        &self.consumption
    }
    /// Check if battery is low
    pub fn is_low(&self) -> bool {
        self.last_reading.soc_percent <= self.config.low_battery_percent
    }
    /// Check if battery is critical
    pub fn is_critical(&self) -> bool {
        self.last_reading.soc_percent <= self.config.critical_battery_percent
    }
    /// Check if temperature is within limits
    pub fn temperature_ok(&self) -> bool {
        self.last_reading.temperature_deci_c >= self.config.min_temp_deci_c
            && self.last_reading.temperature_deci_c <= self.config.max_temp_deci_c
    }
    /// Estimate remaining runtime in minutes
    pub fn estimated_runtime_min(&self) -> Option<u32> {
        if self.charging_state.on_external_power() {
            return None;
        }
        if self.last_reading.time_to_empty_min > 0 {
            return Some(self.last_reading.time_to_empty_min as u32);
        }
        if self.last_reading.current_ma >= 0 {
            return None;
        }
        let current_ma = (-self.last_reading.current_ma) as u32;
        if current_ma == 0 {
            return None;
        }
        let remaining_mah = if self.last_reading.remaining_mah > 0 {
            self.last_reading.remaining_mah
        } else {
            (self.config.design_capacity_mah * self.last_reading.soc_percent as u32) / 100
        };
        Some((remaining_mah * 60) / current_ma)
    }
    /// Estimate time to full charge in minutes
    pub fn estimated_charge_time_min(&self) -> Option<u32> {
        if !matches!(self.charging_state, ChargingState::Charging) {
            return None;
        }
        if self.last_reading.time_to_full_min > 0 {
            return Some(self.last_reading.time_to_full_min as u32);
        }
        if self.last_reading.current_ma <= 0 {
            return None;
        }
        let needed_mah = if self.last_reading.full_capacity_mah > 0 {
            self.last_reading.full_capacity_mah - self.last_reading.remaining_mah
        } else {
            let remaining_percent = 100 - self.last_reading.soc_percent as u32;
            (self.config.design_capacity_mah * remaining_percent) / 100
        };
        let current_ma = self.last_reading.current_ma as u32;
        if current_ma == 0 {
            return None;
        }
        Some((needed_mah * 60) / current_ma)
    }
    /// Should migrate workload due to low battery?
    pub fn should_migrate(&self) -> bool {
        self.is_low() && self.charging_state.using_battery()
    }
    /// Get battery summary
    pub fn summary(&self) -> BatterySummary {
        BatterySummary {
            soc_percent: self.soc_percent(),
            voltage_mv: self.voltage_mv(),
            charging_state: self.charging_state,
            health_status: self.health.status,
            soh_percent: self.health.soh_percent,
            cycle_count: self.health.cycle_count,
            average_power_mw: self.consumption.average_mw(),
            runtime_min: self.estimated_runtime_min(),
            is_low: self.is_low(),
            is_critical: self.is_critical(),
        }
    }
    /// Reset consumption statistics
    pub fn reset_consumption(&mut self) {
        self.consumption.reset();
    }
    /// Get configuration
    pub fn config(&self) -> &BatteryConfig {
        &self.config
    }
}
/// Power consumption tracking
#[derive(Debug, Clone, Copy, Default)]
pub struct PowerConsumption {
    /// Average power consumption in microwatts
    pub average_uw: u64,
    /// Peak power consumption in microwatts
    pub peak_uw: u64,
    /// Minimum power consumption in microwatts
    pub min_uw: u64,
    /// Total energy consumed in microjoules
    pub total_energy_uj: u64,
    /// Number of samples
    pub sample_count: u32,
}
impl PowerConsumption {
    /// Create new power consumption tracker
    pub fn new() -> Self {
        Self::default()
    }
    /// Record a power sample in microwatts
    pub fn record_sample(&mut self, power_uw: u64, duration_us: u64) {
        if self.sample_count == 0 {
            self.min_uw = power_uw;
            self.peak_uw = power_uw;
            self.average_uw = power_uw;
        } else {
            self.min_uw = self.min_uw.min(power_uw);
            self.peak_uw = self.peak_uw.max(power_uw);
            let total = self.average_uw * self.sample_count as u64 + power_uw;
            self.average_uw = total / (self.sample_count as u64 + 1);
        }
        self.sample_count += 1;
        self.total_energy_uj += (power_uw * duration_us) / 1_000_000;
    }
    /// Get average power in milliwatts
    pub fn average_mw(&self) -> u32 {
        (self.average_uw / 1000) as u32
    }
    /// Get peak power in milliwatts
    pub fn peak_mw(&self) -> u32 {
        (self.peak_uw / 1000) as u32
    }
    /// Get total energy in millijoules
    pub fn total_energy_mj(&self) -> u64 {
        self.total_energy_uj / 1000
    }
    /// Reset all statistics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
/// Battery summary for quick status
#[derive(Debug, Clone)]
pub struct BatterySummary {
    /// State of charge (%)
    pub soc_percent: u8,
    /// Current voltage (mV)
    pub voltage_mv: u32,
    /// Charging state
    pub charging_state: ChargingState,
    /// Health status
    pub health_status: HealthStatus,
    /// State of health (%)
    pub soh_percent: u8,
    /// Charge cycle count
    pub cycle_count: u32,
    /// Average power consumption (mW)
    pub average_power_mw: u32,
    /// Estimated runtime (minutes)
    pub runtime_min: Option<u32>,
    /// Low battery flag
    pub is_low: bool,
    /// Critical battery flag
    pub is_critical: bool,
}
/// Battery calibration curve
#[derive(Debug, Clone)]
pub struct BatteryCalibration {
    /// Calibration points (voltage -> SoC mapping)
    /// Must be sorted by voltage (ascending)
    points: Vec<CalibrationPoint>,
    /// Temperature compensation coefficient (percent per degree C)
    /// Positive = voltage decreases with increasing temperature
    temp_coefficient: f32,
    /// Reference temperature for calibration (deci-Celsius)
    reference_temp_deci_c: i16,
    /// Chemistry type
    chemistry: BatteryChemistry,
    /// Number of cells in series
    cells_in_series: u8,
}
impl BatteryCalibration {
    /// Create a new calibration with default points for chemistry
    pub fn new(chemistry: BatteryChemistry, cells_in_series: u8) -> Self {
        let points = Self::default_points_for_chemistry(chemistry, cells_in_series);
        Self {
            points,
            temp_coefficient: 0.0,
            reference_temp_deci_c: 250,
            chemistry,
            cells_in_series,
        }
    }
    /// Create default calibration points for a chemistry
    fn default_points_for_chemistry(
        chemistry: BatteryChemistry,
        cells: u8,
    ) -> Vec<CalibrationPoint> {
        let multiplier = cells as u32;
        match chemistry {
            BatteryChemistry::LithiumIon | BatteryChemistry::LithiumPolymer => {
                vec![
                    CalibrationPoint::new(3000 * multiplier, 0),
                    CalibrationPoint::new(3300 * multiplier, 5),
                    CalibrationPoint::new(3500 * multiplier, 10),
                    CalibrationPoint::new(3600 * multiplier, 20),
                    CalibrationPoint::new(3700 * multiplier, 40),
                    CalibrationPoint::new(3800 * multiplier, 60),
                    CalibrationPoint::new(3900 * multiplier, 75),
                    CalibrationPoint::new(4000 * multiplier, 85),
                    CalibrationPoint::new(4100 * multiplier, 95),
                    CalibrationPoint::new(4200 * multiplier, 100),
                ]
            }
            BatteryChemistry::LiFePO4 => {
                vec![
                    CalibrationPoint::new(2500 * multiplier, 0),
                    CalibrationPoint::new(3000 * multiplier, 10),
                    CalibrationPoint::new(3200 * multiplier, 50),
                    CalibrationPoint::new(3300 * multiplier, 90),
                    CalibrationPoint::new(3650 * multiplier, 100),
                ]
            }
            BatteryChemistry::NiMH => {
                vec![
                    CalibrationPoint::new(1000 * multiplier, 0),
                    CalibrationPoint::new(1100 * multiplier, 20),
                    CalibrationPoint::new(1200 * multiplier, 50),
                    CalibrationPoint::new(1300 * multiplier, 80),
                    CalibrationPoint::new(1450 * multiplier, 100),
                ]
            }
            BatteryChemistry::LeadAcid => {
                vec![
                    CalibrationPoint::new(1750 * multiplier, 0),
                    CalibrationPoint::new(1900 * multiplier, 25),
                    CalibrationPoint::new(2000 * multiplier, 50),
                    CalibrationPoint::new(2100 * multiplier, 75),
                    CalibrationPoint::new(2400 * multiplier, 100),
                ]
            }
            BatteryChemistry::Unknown => {
                vec![
                    CalibrationPoint::new(3000 * multiplier, 0),
                    CalibrationPoint::new(3700 * multiplier, 50),
                    CalibrationPoint::new(4200 * multiplier, 100),
                ]
            }
        }
    }
    /// Set custom calibration points
    pub fn set_points(&mut self, mut points: Vec<CalibrationPoint>) -> Result<(), &'static str> {
        if points.len() < 2 {
            return Err("Need at least 2 calibration points");
        }
        points.sort_by_key(|p| p.voltage_mv);
        for i in 0..points.len() - 1 {
            if points[i].soc_percent >= points[i + 1].soc_percent {
                return Err("SoC must increase with voltage");
            }
        }
        if points[0].soc_percent != 0 {
            return Err("First point must be 0%");
        }
        if points[points.len() - 1].soc_percent != 100 {
            return Err("Last point must be 100%");
        }
        self.points = points;
        Ok(())
    }
    /// Add a calibration point
    pub fn add_point(&mut self, voltage_mv: u32, soc_percent: u8) {
        let point = CalibrationPoint::new(voltage_mv, soc_percent);
        let pos = self
            .points
            .binary_search_by_key(&voltage_mv, |p| p.voltage_mv)
            .unwrap_or_else(|e| e);
        self.points.insert(pos, point);
    }
    /// Set temperature compensation coefficient
    pub fn set_temperature_compensation(&mut self, coefficient: f32, reference_temp_deci_c: i16) {
        self.temp_coefficient = coefficient;
        self.reference_temp_deci_c = reference_temp_deci_c;
    }
    /// Calibrate voltage to SoC with temperature compensation
    pub fn voltage_to_soc(&self, voltage_mv: u32, temperature_deci_c: i16) -> u8 {
        let compensated_voltage = if self.temp_coefficient != 0.0 {
            let temp_diff = (temperature_deci_c - self.reference_temp_deci_c) as f32 / 10.0;
            let compensation = temp_diff * self.temp_coefficient * self.cells_in_series as f32;
            (voltage_mv as f32 + compensation) as u32
        } else {
            voltage_mv
        };
        if compensated_voltage <= self.points[0].voltage_mv {
            return 0;
        }
        if compensated_voltage >= self.points[self.points.len() - 1].voltage_mv {
            return 100;
        }
        for i in 0..self.points.len() - 1 {
            let p1 = &self.points[i];
            let p2 = &self.points[i + 1];
            if compensated_voltage >= p1.voltage_mv && compensated_voltage <= p2.voltage_mv {
                let voltage_range = p2.voltage_mv - p1.voltage_mv;
                let soc_range = (p2.soc_percent - p1.soc_percent) as u32;
                let voltage_offset = compensated_voltage - p1.voltage_mv;
                let soc = p1.soc_percent as u32
                    + (voltage_offset * soc_range + voltage_range / 2) / voltage_range;
                return soc.min(100) as u8;
            }
        }
        50
    }
    /// Get SoC without temperature compensation
    pub fn voltage_to_soc_simple(&self, voltage_mv: u32) -> u8 {
        self.voltage_to_soc(voltage_mv, self.reference_temp_deci_c)
    }
    /// Get voltage for a target SoC
    pub fn soc_to_voltage(&self, soc_percent: u8) -> u32 {
        if soc_percent == 0 {
            return self.points[0].voltage_mv;
        }
        if soc_percent >= 100 {
            return self.points[self.points.len() - 1].voltage_mv;
        }
        for i in 0..self.points.len() - 1 {
            let p1 = &self.points[i];
            let p2 = &self.points[i + 1];
            if soc_percent >= p1.soc_percent && soc_percent <= p2.soc_percent {
                let soc_range = (p2.soc_percent - p1.soc_percent) as u32;
                let voltage_range = p2.voltage_mv - p1.voltage_mv;
                let soc_offset = (soc_percent - p1.soc_percent) as u32;
                let voltage =
                    p1.voltage_mv + (soc_offset * voltage_range + soc_range / 2) / soc_range;
                return voltage;
            }
        }
        self.points[self.points.len() / 2].voltage_mv
    }
    /// Get calibration points
    pub fn points(&self) -> &[CalibrationPoint] {
        &self.points
    }
    /// Validate calibration curve
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.points.len() < 2 {
            return Err("Need at least 2 points");
        }
        for i in 0..self.points.len() - 1 {
            if self.points[i].voltage_mv >= self.points[i + 1].voltage_mv {
                return Err("Points must be sorted by voltage");
            }
            if self.points[i].soc_percent >= self.points[i + 1].soc_percent {
                return Err("SoC must increase with voltage");
            }
        }
        if self.points[0].soc_percent != 0 {
            return Err("First point must be 0%");
        }
        if self.points[self.points.len() - 1].soc_percent != 100 {
            return Err("Last point must be 100%");
        }
        Ok(())
    }
    /// Reset to default calibration
    pub fn reset_to_defaults(&mut self) {
        self.points = Self::default_points_for_chemistry(self.chemistry, self.cells_in_series);
        self.temp_coefficient = 0.0;
        self.reference_temp_deci_c = 250;
    }
}
/// Battery charging state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargingState {
    /// Not charging (on battery power)
    Discharging,
    /// Actively charging
    Charging,
    /// Fully charged (on external power)
    Full,
    /// Not charging but on external power (e.g., battery maintenance)
    NotCharging,
    /// Unknown/error state
    Unknown,
}
impl ChargingState {
    /// Check if on external power
    pub fn on_external_power(&self) -> bool {
        matches!(
            self,
            ChargingState::Charging | ChargingState::Full | ChargingState::NotCharging
        )
    }
    /// Check if battery is being used
    pub fn using_battery(&self) -> bool {
        matches!(self, ChargingState::Discharging)
    }
}
/// Battery health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Battery is in good condition
    Good,
    /// Battery is showing some degradation
    Fair,
    /// Battery has significant degradation
    Poor,
    /// Battery needs replacement
    Critical,
    /// Battery has a fault (overtemp, overvoltage, etc.)
    Fault,
    /// Health status unknown
    Unknown,
}
impl HealthStatus {
    /// Get health as percentage (100 = Good, 0 = Critical)
    pub fn as_percentage(&self) -> u8 {
        match self {
            HealthStatus::Good => 100,
            HealthStatus::Fair => 70,
            HealthStatus::Poor => 40,
            HealthStatus::Critical => 10,
            HealthStatus::Fault => 0,
            HealthStatus::Unknown => 50,
        }
    }
}
/// Simple battery monitor using only voltage measurements
///
/// This is a lightweight alternative to BatteryManager for systems without
/// hardware fuel gauge chips. It estimates state of charge based purely on
/// battery voltage using calibration curves.
///
/// ## Features
///
/// - Voltage-only monitoring (no current sensing required)
/// - Automatic SoC estimation from voltage
/// - Low/critical battery detection
/// - Simple integration (just measure voltage)
///
/// ## Limitations
///
/// - Less accurate than fuel gauge-based monitoring
/// - Cannot estimate runtime (no current measurement)
/// - Voltage readings affected by load
/// - No cycle counting or health tracking
///
/// ## Example
///
/// ```rust,no_run
/// use mielin_rt::battery::{SimpleVoltageMonitor, BatteryChemistry};
///
/// # fn read_adc() -> u32 { 3700 }
/// // Create monitor for single-cell LiPo
/// let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumPolymer, 1);
///
/// // Read voltage from ADC
/// let voltage_mv = read_adc(); // Your ADC reading function
///
/// // Update monitor
/// monitor.update(voltage_mv);
///
/// // Check state
/// let soc = monitor.state_of_charge();
/// let is_low = monitor.is_low_battery();
/// ```
#[derive(Debug, Clone)]
pub struct SimpleVoltageMonitor {
    /// Battery calibration for voltage-to-SoC conversion
    calibration: BatteryCalibration,
    /// Current voltage reading in millivolts
    voltage_mv: u32,
    /// Estimated state of charge (0-100%)
    soc_percent: u8,
    /// Low battery threshold (percentage)
    low_threshold: u8,
    /// Critical battery threshold (percentage)
    critical_threshold: u8,
    /// Number of samples for averaging
    avg_samples: u8,
    /// Averaging buffer
    voltage_buffer: Vec<u32>,
}
impl SimpleVoltageMonitor {
    /// Create a new simple voltage monitor
    ///
    /// # Arguments
    ///
    /// * `chemistry` - Battery chemistry type
    /// * `cells_in_series` - Number of cells in series
    ///
    /// # Example
    ///
    /// ```rust
    /// use mielin_rt::battery::{SimpleVoltageMonitor, BatteryChemistry};
    ///
    /// // Single-cell Li-Ion
    /// let monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
    ///
    /// // 3-cell (3S) Li-Ion pack
    /// let monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 3);
    /// ```
    pub fn new(chemistry: BatteryChemistry, cells_in_series: u8) -> Self {
        let calibration = BatteryCalibration::new(chemistry, cells_in_series);
        let nominal_v = chemistry.nominal_voltage_mv() * cells_in_series as u32;
        Self {
            calibration,
            voltage_mv: nominal_v,
            soc_percent: 50,
            low_threshold: 20,
            critical_threshold: 5,
            avg_samples: 4,
            voltage_buffer: Vec::new(),
        }
    }
    /// Set low battery threshold (percentage)
    pub fn set_low_threshold(&mut self, threshold: u8) {
        self.low_threshold = threshold.min(100);
    }
    /// Set critical battery threshold (percentage)
    pub fn set_critical_threshold(&mut self, threshold: u8) {
        self.critical_threshold = threshold.min(100);
    }
    /// Set number of samples for voltage averaging
    ///
    /// Higher values provide more stable readings but slower response.
    /// Typical values: 4-16
    pub fn set_averaging_samples(&mut self, samples: u8) {
        self.avg_samples = samples.clamp(1, 32);
    }
    /// Update with a new voltage reading
    ///
    /// Voltage should be measured when the battery is not under heavy load
    /// for best accuracy.
    ///
    /// # Arguments
    ///
    /// * `voltage_mv` - Battery voltage in millivolts
    pub fn update(&mut self, voltage_mv: u32) {
        self.voltage_buffer.push(voltage_mv);
        if self.voltage_buffer.len() > self.avg_samples as usize {
            self.voltage_buffer.remove(0);
        }
        let avg_voltage = if !self.voltage_buffer.is_empty() {
            let sum: u32 = self.voltage_buffer.iter().sum();
            sum / self.voltage_buffer.len() as u32
        } else {
            voltage_mv
        };
        self.voltage_mv = avg_voltage;
        self.soc_percent = self.calibration.voltage_to_soc_simple(avg_voltage);
    }
    /// Get current battery voltage in millivolts
    pub fn voltage_mv(&self) -> u32 {
        self.voltage_mv
    }
    /// Get estimated state of charge (0-100%)
    pub fn state_of_charge(&self) -> u8 {
        self.soc_percent
    }
    /// Check if battery is low
    pub fn is_low_battery(&self) -> bool {
        self.soc_percent <= self.low_threshold
    }
    /// Check if battery is critical
    pub fn is_critical_battery(&self) -> bool {
        self.soc_percent <= self.critical_threshold
    }
    /// Get battery status summary
    pub fn summary(&self) -> SimpleVoltageSummary {
        SimpleVoltageSummary {
            voltage_mv: self.voltage_mv,
            soc_percent: self.soc_percent,
            is_low: self.is_low_battery(),
            is_critical: self.is_critical_battery(),
        }
    }
    /// Get mutable reference to calibration for customization
    pub fn calibration_mut(&mut self) -> &mut BatteryCalibration {
        &mut self.calibration
    }
    /// Get reference to calibration
    pub fn calibration(&self) -> &BatteryCalibration {
        &self.calibration
    }
    /// Reset averaging buffer
    pub fn reset_averaging(&mut self) {
        self.voltage_buffer.clear();
    }
}
/// Summary of simple voltage monitor status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleVoltageSummary {
    /// Current voltage in millivolts
    pub voltage_mv: u32,
    /// Estimated state of charge (0-100%)
    pub soc_percent: u8,
    /// Is battery low?
    pub is_low: bool,
    /// Is battery critical?
    pub is_critical: bool,
}
/// Battery chemistry type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryChemistry {
    /// Lithium Ion
    LithiumIon,
    /// Lithium Polymer
    LithiumPolymer,
    /// Lithium Iron Phosphate
    LiFePO4,
    /// Nickel Metal Hydride
    NiMH,
    /// Lead Acid
    LeadAcid,
    /// Unknown chemistry
    Unknown,
}
impl BatteryChemistry {
    /// Get nominal cell voltage in millivolts
    pub fn nominal_voltage_mv(&self) -> u32 {
        match self {
            BatteryChemistry::LithiumIon => 3700,
            BatteryChemistry::LithiumPolymer => 3700,
            BatteryChemistry::LiFePO4 => 3200,
            BatteryChemistry::NiMH => 1200,
            BatteryChemistry::LeadAcid => 2000,
            BatteryChemistry::Unknown => 3700,
        }
    }
    /// Get full charge voltage per cell in millivolts
    pub fn full_voltage_mv(&self) -> u32 {
        match self {
            BatteryChemistry::LithiumIon => 4200,
            BatteryChemistry::LithiumPolymer => 4200,
            BatteryChemistry::LiFePO4 => 3650,
            BatteryChemistry::NiMH => 1450,
            BatteryChemistry::LeadAcid => 2400,
            BatteryChemistry::Unknown => 4200,
        }
    }
    /// Get empty voltage per cell in millivolts
    pub fn empty_voltage_mv(&self) -> u32 {
        match self {
            BatteryChemistry::LithiumIon => 3000,
            BatteryChemistry::LithiumPolymer => 3000,
            BatteryChemistry::LiFePO4 => 2500,
            BatteryChemistry::NiMH => 1000,
            BatteryChemistry::LeadAcid => 1750,
            BatteryChemistry::Unknown => 3000,
        }
    }
    /// Get recommended maximum charge cycles
    pub fn typical_cycle_life(&self) -> u32 {
        match self {
            BatteryChemistry::LithiumIon => 500,
            BatteryChemistry::LithiumPolymer => 300,
            BatteryChemistry::LiFePO4 => 2000,
            BatteryChemistry::NiMH => 500,
            BatteryChemistry::LeadAcid => 200,
            BatteryChemistry::Unknown => 500,
        }
    }
}
