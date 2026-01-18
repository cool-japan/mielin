//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::super::types::{
        BatteryCalibration, BatteryChemistry, BatteryConfig, BatteryHealth, BatteryManager,
        CalibrationPoint, ChargingState, FuelGaugeReading, HealthStatus, PowerConsumption,
        SimpleVoltageMonitor,
    };
    use alloc::vec;
    #[test]
    fn test_charging_state() {
        assert!(ChargingState::Charging.on_external_power());
        assert!(ChargingState::Full.on_external_power());
        assert!(!ChargingState::Discharging.on_external_power());
        assert!(ChargingState::Discharging.using_battery());
    }
    #[test]
    fn test_battery_chemistry_voltages() {
        let lipo = BatteryChemistry::LithiumPolymer;
        assert_eq!(lipo.nominal_voltage_mv(), 3700);
        assert_eq!(lipo.full_voltage_mv(), 4200);
        assert_eq!(lipo.empty_voltage_mv(), 3000);
    }
    #[test]
    fn test_battery_chemistry_cycle_life() {
        assert!(
            BatteryChemistry::LiFePO4.typical_cycle_life()
                > BatteryChemistry::LithiumIon.typical_cycle_life()
        );
    }
    #[test]
    fn test_health_status_percentage() {
        assert_eq!(HealthStatus::Good.as_percentage(), 100);
        assert_eq!(HealthStatus::Critical.as_percentage(), 10);
    }
    #[test]
    fn test_fuel_gauge_reading_new() {
        let reading = FuelGaugeReading::new(75, 3800, -500);
        assert_eq!(reading.soc_percent, 75);
        assert_eq!(reading.voltage_mv, 3800);
        assert_eq!(reading.current_ma, -500);
    }
    #[test]
    fn test_fuel_gauge_temperature() {
        let reading = FuelGaugeReading {
            temperature_deci_c: 250,
            ..FuelGaugeReading::default()
        };
        assert!((reading.temperature_c() - 25.0).abs() < 0.1);
        assert!(!reading.is_hot());
        assert!(!reading.is_cold());
        let reading_hot = FuelGaugeReading {
            temperature_deci_c: 500,
            ..FuelGaugeReading::default()
        };
        assert!(reading_hot.is_hot());
        let reading_cold = FuelGaugeReading {
            temperature_deci_c: -50,
            ..FuelGaugeReading::default()
        };
        assert!(reading_cold.is_cold());
    }
    #[test]
    fn test_fuel_gauge_power() {
        let reading = FuelGaugeReading {
            voltage_mv: 3700,
            current_ma: -1000,
            ..Default::default()
        };
        assert_eq!(reading.power_mw(), -3700);
    }
    #[test]
    fn test_fuel_gauge_capacity_fade() {
        let reading = FuelGaugeReading {
            design_capacity_mah: 3000,
            full_capacity_mah: 2700,
            ..Default::default()
        };
        assert_eq!(reading.capacity_fade(), 10);
    }
    #[test]
    fn test_power_consumption_recording() {
        let mut consumption = PowerConsumption::new();
        consumption.record_sample(100_000, 1_000_000);
        consumption.record_sample(200_000, 1_000_000);
        assert_eq!(consumption.sample_count, 2);
        assert_eq!(consumption.min_uw, 100_000);
        assert_eq!(consumption.peak_uw, 200_000);
    }
    #[test]
    fn test_power_consumption_energy() {
        let mut consumption = PowerConsumption::new();
        consumption.record_sample(1_000_000, 1_000_000);
        assert_eq!(consumption.total_energy_uj, 1_000_000);
    }
    #[test]
    fn test_battery_health_new() {
        let health = BatteryHealth::new(BatteryChemistry::LithiumIon);
        assert_eq!(health.status, HealthStatus::Unknown);
        assert_eq!(health.soh_percent, 100);
        assert_eq!(health.cycle_count, 0);
    }
    #[test]
    fn test_battery_health_update() {
        let mut health = BatteryHealth::new(BatteryChemistry::LithiumIon);
        let reading = FuelGaugeReading {
            soc_percent: 50,
            design_capacity_mah: 3000,
            full_capacity_mah: 2700,
            temperature_deci_c: 300,
            ..Default::default()
        };
        health.update(&reading);
        assert_eq!(health.soh_percent, 90);
        assert_eq!(health.max_temperature_deci_c, 300);
    }
    #[test]
    fn test_battery_health_cycles() {
        let mut health = BatteryHealth::new(BatteryChemistry::LithiumIon);
        for _ in 0..100 {
            health.record_charge_cycle();
        }
        assert_eq!(health.cycle_count, 100);
        assert_eq!(health.status, HealthStatus::Good);
    }
    #[test]
    fn test_battery_health_remaining_life() {
        let mut health = BatteryHealth::new(BatteryChemistry::LithiumIon);
        health.cycle_count = 250;
        health.soh_percent = 80;
        let remaining = health.remaining_life_percent();
        assert!(remaining > 60 && remaining < 70);
    }
    #[test]
    fn test_battery_config_single_cell() {
        let config = BatteryConfig::single_cell_lipo(2000);
        assert_eq!(config.cells_in_series, 1);
        assert_eq!(config.design_capacity_mah, 2000);
        assert_eq!(config.full_voltage_mv(), 4200);
    }
    #[test]
    fn test_battery_config_two_cell() {
        let config = BatteryConfig::two_cell_lipo(2000);
        assert_eq!(config.cells_in_series, 2);
        assert_eq!(config.full_voltage_mv(), 8400);
        assert_eq!(config.empty_voltage_mv(), 6000);
    }
    #[test]
    fn test_battery_config_voltage_to_soc() {
        let config = BatteryConfig::single_cell_lipo(1000);
        assert_eq!(config.voltage_to_soc(4200), 100);
        assert_eq!(config.voltage_to_soc(3000), 0);
        assert_eq!(config.voltage_to_soc(3600), 50);
    }
    #[test]
    fn test_battery_manager_new() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let manager = BatteryManager::new(config);
        assert_eq!(manager.charging_state(), ChargingState::Unknown);
    }
    #[test]
    fn test_battery_manager_update_discharging() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 75,
            voltage_mv: 3800,
            current_ma: -500,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        assert_eq!(manager.soc_percent(), 75);
        assert_eq!(manager.charging_state(), ChargingState::Discharging);
    }
    #[test]
    fn test_battery_manager_update_charging() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 75,
            voltage_mv: 4100,
            current_ma: 500,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        assert_eq!(manager.charging_state(), ChargingState::Charging);
    }
    #[test]
    fn test_battery_manager_low_critical() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 15,
            current_ma: -100,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        assert!(manager.is_low());
        assert!(!manager.is_critical());
        let reading = FuelGaugeReading {
            soc_percent: 3,
            current_ma: -100,
            ..Default::default()
        };
        manager.update(reading, 2_000_000);
        assert!(manager.is_critical());
    }
    #[test]
    fn test_battery_manager_should_migrate() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 15,
            current_ma: -500,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        assert!(manager.should_migrate());
        let reading = FuelGaugeReading {
            soc_percent: 15,
            current_ma: 500,
            ..Default::default()
        };
        manager.update(reading, 2_000_000);
        assert!(!manager.should_migrate());
    }
    #[test]
    fn test_battery_manager_runtime_estimate() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 50,
            remaining_mah: 1000,
            current_ma: -500,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        let runtime = manager.estimated_runtime_min();
        assert_eq!(runtime, Some(120));
    }
    #[test]
    fn test_battery_manager_charge_time_estimate() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 50,
            remaining_mah: 1000,
            full_capacity_mah: 2000,
            current_ma: 500,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        let charge_time = manager.estimated_charge_time_min();
        assert_eq!(charge_time, Some(120));
    }
    #[test]
    fn test_battery_manager_cycle_detection() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 100,
            current_ma: -100,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        assert_eq!(manager.health().cycle_count, 0);
        let reading = FuelGaugeReading {
            soc_percent: 5,
            current_ma: -100,
            ..Default::default()
        };
        manager.update(reading, 2_000_000);
        assert_eq!(manager.health().cycle_count, 1);
    }
    #[test]
    fn test_battery_manager_soc_averaging() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        for soc in [50, 51, 49, 50, 52] {
            let reading = FuelGaugeReading {
                soc_percent: soc,
                current_ma: -100,
                ..Default::default()
            };
            manager.update(reading, 1_000_000);
        }
        let avg = manager.soc_averaged();
        assert_eq!(avg, 50);
    }
    #[test]
    fn test_battery_summary() {
        let config = BatteryConfig::single_cell_lipo(2000);
        let mut manager = BatteryManager::new(config);
        let reading = FuelGaugeReading {
            soc_percent: 75,
            voltage_mv: 3800,
            current_ma: -500,
            ..Default::default()
        };
        manager.update(reading, 1_000_000);
        let summary = manager.summary();
        assert_eq!(summary.soc_percent, 75);
        assert_eq!(summary.voltage_mv, 3800);
        assert_eq!(summary.charging_state, ChargingState::Discharging);
        assert!(!summary.is_low);
    }
    #[test]
    fn test_calibration_point_new() {
        let point = CalibrationPoint::new(3700, 50);
        assert_eq!(point.voltage_mv, 3700);
        assert_eq!(point.soc_percent, 50);
    }
    #[test]
    fn test_calibration_default() {
        let cal = BatteryCalibration::default();
        assert!(cal.validate().is_ok());
        assert!(cal.points().len() >= 2);
    }
    #[test]
    fn test_calibration_for_lipo() {
        let cal = BatteryCalibration::new(BatteryChemistry::LithiumPolymer, 1);
        assert_eq!(cal.points()[0].soc_percent, 0);
        assert_eq!(cal.points()[cal.points().len() - 1].soc_percent, 100);
    }
    #[test]
    fn test_calibration_for_two_cell() {
        let cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 2);
        assert_eq!(cal.points()[0].voltage_mv, 6000);
        assert_eq!(cal.points()[cal.points().len() - 1].voltage_mv, 8400);
    }
    #[test]
    fn test_calibration_voltage_to_soc_simple() {
        let cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 1);
        assert_eq!(cal.voltage_to_soc_simple(3000), 0);
        assert_eq!(cal.voltage_to_soc_simple(4200), 100);
        let soc = cal.voltage_to_soc_simple(3700);
        assert!(soc > 30 && soc < 50);
    }
    #[test]
    fn test_calibration_soc_to_voltage() {
        let cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 1);
        assert_eq!(cal.soc_to_voltage(0), 3000);
        assert_eq!(cal.soc_to_voltage(100), 4200);
        let voltage = cal.soc_to_voltage(50);
        assert!(voltage > 3600 && voltage < 3900);
    }
    #[test]
    fn test_calibration_temperature_compensation() {
        let mut cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 1);
        cal.set_temperature_compensation(3.0, 250);
        let voltage = 3700;
        let soc_ref = cal.voltage_to_soc(voltage, 250);
        let soc_hot = cal.voltage_to_soc(voltage, 350);
        let soc_cold = cal.voltage_to_soc(voltage, 150);
        assert!(soc_hot >= soc_ref);
        assert!(soc_cold <= soc_ref);
    }
    #[test]
    fn test_calibration_add_point() {
        let mut cal = BatteryCalibration::new(BatteryChemistry::Unknown, 1);
        let initial_count = cal.points().len();
        cal.add_point(3500, 25);
        assert_eq!(cal.points().len(), initial_count + 1);
        for i in 0..cal.points().len() - 1 {
            assert!(cal.points()[i].voltage_mv <= cal.points()[i + 1].voltage_mv);
        }
    }
    #[test]
    fn test_calibration_set_points_valid() {
        let mut cal = BatteryCalibration::default();
        let points = vec![
            CalibrationPoint::new(3000, 0),
            CalibrationPoint::new(3500, 25),
            CalibrationPoint::new(3700, 50),
            CalibrationPoint::new(3900, 75),
            CalibrationPoint::new(4200, 100),
        ];
        assert!(cal.set_points(points).is_ok());
        assert_eq!(cal.points().len(), 5);
    }
    #[test]
    fn test_calibration_set_points_invalid_too_few() {
        let mut cal = BatteryCalibration::default();
        let points = vec![CalibrationPoint::new(3700, 50)];
        assert!(cal.set_points(points).is_err());
    }
    #[test]
    fn test_calibration_set_points_invalid_no_zero() {
        let mut cal = BatteryCalibration::default();
        let points = vec![
            CalibrationPoint::new(3000, 10),
            CalibrationPoint::new(4200, 100),
        ];
        assert!(cal.set_points(points).is_err());
    }
    #[test]
    fn test_calibration_set_points_invalid_no_hundred() {
        let mut cal = BatteryCalibration::default();
        let points = vec![
            CalibrationPoint::new(3000, 0),
            CalibrationPoint::new(4200, 90),
        ];
        assert!(cal.set_points(points).is_err());
    }
    #[test]
    fn test_calibration_set_points_auto_sort() {
        let mut cal = BatteryCalibration::default();
        let points = vec![
            CalibrationPoint::new(4200, 100),
            CalibrationPoint::new(3000, 0),
            CalibrationPoint::new(3700, 50),
        ];
        assert!(cal.set_points(points).is_ok());
        for i in 0..cal.points().len() - 1 {
            assert!(cal.points()[i].voltage_mv < cal.points()[i + 1].voltage_mv);
        }
    }
    #[test]
    fn test_calibration_validate() {
        let cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 1);
        assert!(cal.validate().is_ok());
    }
    #[test]
    fn test_calibration_reset_to_defaults() {
        let mut cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 1);
        let original_count = cal.points().len();
        cal.set_temperature_compensation(5.0, 300);
        cal.add_point(3750, 45);
        cal.reset_to_defaults();
        assert_eq!(cal.points().len(), original_count);
        assert!(cal.validate().is_ok());
    }
    #[test]
    fn test_calibration_lifepo4() {
        let cal = BatteryCalibration::new(BatteryChemistry::LiFePO4, 1);
        assert!(cal.points().len() >= 2);
        assert_eq!(cal.points()[0].voltage_mv, 2500);
        assert_eq!(cal.points()[cal.points().len() - 1].voltage_mv, 3650);
    }
    #[test]
    fn test_calibration_nimh() {
        let cal = BatteryCalibration::new(BatteryChemistry::NiMH, 1);
        assert!(cal.points().len() >= 2);
        assert_eq!(cal.points()[0].voltage_mv, 1000);
        assert_eq!(cal.points()[cal.points().len() - 1].voltage_mv, 1450);
    }
    #[test]
    fn test_calibration_lead_acid() {
        let cal = BatteryCalibration::new(BatteryChemistry::LeadAcid, 1);
        assert!(cal.points().len() >= 2);
        assert_eq!(cal.points()[0].voltage_mv, 1750);
        assert_eq!(cal.points()[cal.points().len() - 1].voltage_mv, 2400);
    }
    #[test]
    fn test_calibration_interpolation_accuracy() {
        let mut cal = BatteryCalibration::default();
        let points = vec![
            CalibrationPoint::new(3000, 0),
            CalibrationPoint::new(3600, 50),
            CalibrationPoint::new(4200, 100),
        ];
        cal.set_points(points).unwrap();
        let soc = cal.voltage_to_soc_simple(3300);
        assert!((24..=26).contains(&soc));
        let soc = cal.voltage_to_soc_simple(3900);
        assert!((74..=76).contains(&soc));
    }
    #[test]
    fn test_calibration_boundary_handling() {
        let cal = BatteryCalibration::new(BatteryChemistry::LithiumIon, 1);
        assert_eq!(cal.voltage_to_soc_simple(2500), 0);
        assert_eq!(cal.voltage_to_soc_simple(4500), 100);
    }
    #[test]
    fn test_simple_voltage_monitor_creation() {
        let monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumPolymer, 1);
        assert_eq!(monitor.voltage_mv(), 3700);
        assert_eq!(monitor.state_of_charge(), 50);
    }
    #[test]
    fn test_simple_voltage_monitor_voltage_update() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.update(4200);
        assert_eq!(monitor.voltage_mv(), 4200);
        assert!(monitor.state_of_charge() >= 95);
    }
    #[test]
    fn test_simple_voltage_monitor_averaging() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.set_averaging_samples(4);
        monitor.update(3700);
        monitor.update(3750);
        monitor.update(3650);
        monitor.update(3700);
        let voltage = monitor.voltage_mv();
        assert!((3680..=3720).contains(&voltage));
    }
    #[test]
    fn test_simple_voltage_monitor_low_battery_detection() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.set_low_threshold(20);
        monitor.update(3700);
        assert!(!monitor.is_low_battery());
        monitor.update(3200);
        assert!(monitor.is_low_battery());
    }
    #[test]
    fn test_simple_voltage_monitor_critical_battery_detection() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.set_critical_threshold(20);
        monitor.update(3800);
        assert!(!monitor.is_critical_battery());
        monitor.update(3100);
        let soc_low = monitor.state_of_charge();
        assert!(soc_low <= 20 || monitor.is_critical_battery());
    }
    #[test]
    fn test_simple_voltage_monitor_summary() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.set_low_threshold(50);
        monitor.set_critical_threshold(20);
        monitor.update(3400);
        let summary = monitor.summary();
        assert_eq!(summary.voltage_mv, 3400);
        assert!(summary.soc_percent <= 100);
    }
    #[test]
    fn test_simple_voltage_monitor_multi_cell() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 3);
        monitor.update(12600);
        let soc_full = monitor.state_of_charge();
        assert!(soc_full >= 90);
        monitor.update(11100);
        let soc_mid = monitor.state_of_charge();
        assert!(soc_mid > 10 && soc_mid < 90);
        monitor.update(9300);
        let soc_low = monitor.state_of_charge();
        assert!(soc_low < soc_mid);
    }
    #[test]
    fn test_simple_voltage_monitor_reset_averaging() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.update(3700);
        monitor.update(3800);
        monitor.update(3900);
        monitor.reset_averaging();
        monitor.update(3300);
        assert_eq!(monitor.voltage_mv(), 3300);
    }
    #[test]
    fn test_simple_voltage_monitor_threshold_clamping() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        monitor.set_low_threshold(150);
        monitor.set_critical_threshold(120);
        monitor.update(3700);
    }
    #[test]
    fn test_simple_voltage_monitor_lifepo4() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LiFePO4, 1);
        monitor.update(3650);
        let soc_full = monitor.state_of_charge();
        assert!(soc_full >= 80);
        monitor.update(3200);
        let soc_mid = monitor.state_of_charge();
        assert!(soc_mid < soc_full);
        monitor.update(2600);
        let soc_low = monitor.state_of_charge();
        assert!(soc_low < soc_mid);
    }
    #[test]
    fn test_simple_voltage_monitor_calibration_access() {
        let mut monitor = SimpleVoltageMonitor::new(BatteryChemistry::LithiumIon, 1);
        {
            let cal = monitor.calibration_mut();
            cal.add_point(3750, 45);
        }
        monitor.update(3750);
        let soc = monitor.state_of_charge();
        assert!((40..=50).contains(&soc));
    }
}
