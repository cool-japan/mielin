//! End-to-End Embedded IoT Example
//!
//! This example demonstrates MielinOS embedded runtime (mielin-rt)
//! running on a simulated IoT sensor node with:
//! - Battery management and power mode optimization
//! - Energy profiling and budget management
//! - Automatic agent migration on low battery
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example e2e-embedded-iot
//! ```

#![allow(dead_code)]

use mielin_cells::Agent;
use mielin_rt::{
    config::ConfigPreset,
    energy::EnergyProfiler,
    measurement::{MeasurementConfig, PowerMeasurement},
    power::{BatteryStatus, PowerMode},
    EmbeddedRuntime, RuntimeError,
};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;

/// Simulated IoT sensor node
struct IoTSensorNode {
    /// Embedded runtime
    runtime: EmbeddedRuntime,
    /// Energy profiler
    energy_profiler: EnergyProfiler,
    /// Deployed agent (if any)
    agent: Option<Agent>,
    /// Node ID
    node_id: Uuid,
}

impl IoTSensorNode {
    /// Create a new IoT sensor node with default configuration
    fn new() -> Result<Self, RuntimeError> {
        Self::from_preset(ConfigPreset::LowPower)
    }

    /// Create a new IoT sensor node from a configuration preset
    fn from_preset(preset: ConfigPreset) -> Result<Self, RuntimeError> {
        let runtime = EmbeddedRuntime::from_preset(preset)?;

        // Create energy profiler
        let energy_profiler = EnergyProfiler::new();

        Ok(Self {
            runtime,
            energy_profiler,
            agent: None,
            node_id: Uuid::new_v4(),
        })
    }

    /// Initialize the sensor node
    fn init(&mut self) -> Result<(), RuntimeError> {
        info!("🚀 Initializing IoT Sensor Node");
        info!("   Node ID: {}", self.node_id);
        info!("   Architecture: {:?}", self.runtime.architecture());

        // Initialize runtime
        self.runtime.init()?;

        // Enable power monitoring
        self.runtime
            .enable_power_monitoring(MeasurementConfig::default().with_sample_rate_hz(100));
        info!("   Power monitoring enabled");

        info!("✅ Sensor node initialized");

        Ok(())
    }

    /// Deploy an agent to this node
    fn deploy_agent(&mut self, agent: Agent) {
        let agent_id = agent.id();
        info!("🤖 Deploying agent to sensor node");
        info!("   Agent ID: {}", agent_id);

        self.agent = Some(agent);

        info!("   ✅ Agent deployed");
    }

    /// Simulate battery discharge
    fn simulate_battery_discharge(&self, current_level: u8, elapsed_secs: u64) -> u8 {
        // Discharge rate depends on power mode
        let discharge_rate_per_hour = match self.runtime.power_mode() {
            PowerMode::Normal => 5.0,        // 5% per hour
            PowerMode::LowPower => 2.0,      // 2% per hour
            PowerMode::UltraLowPower => 0.5, // 0.5% per hour
            PowerMode::Sleep | PowerMode::Standby | PowerMode::Shutdown => 0.1, // 0.1% per hour
        };

        let discharge_per_sec = discharge_rate_per_hour / 3600.0;
        let total_discharge = (discharge_per_sec * elapsed_secs as f32) as u8;

        current_level.saturating_sub(total_discharge)
    }

    /// Check if agent should migrate based on battery level
    fn should_migrate(&self) -> bool {
        self.runtime.should_migrate()
    }

    /// Run sensor node simulation
    fn run_simulation(&mut self, duration_secs: u64) -> Result<(), RuntimeError> {
        info!("🔄 Starting sensor node simulation ({}s)", duration_secs);
        info!("");

        let start_time = Instant::now();
        let mut battery_level = 100u8;
        let mut last_battery_update = Instant::now();
        let mut iteration: u64 = 0;

        while start_time.elapsed().as_secs() < duration_secs {
            iteration += 1;

            // Update battery every second
            if last_battery_update.elapsed().as_secs() >= 1 {
                let elapsed_secs = last_battery_update.elapsed().as_secs();
                battery_level = self.simulate_battery_discharge(battery_level, elapsed_secs);

                self.runtime.update_battery(BatteryStatus {
                    level_percent: battery_level,
                    is_charging: false,
                });

                // Adjust power mode based on battery
                if battery_level > 70 {
                    self.runtime.set_power_mode(PowerMode::Normal);
                } else if battery_level > 40 {
                    self.runtime.set_power_mode(PowerMode::LowPower);
                } else if battery_level > 20 {
                    self.runtime.set_power_mode(PowerMode::UltraLowPower);
                } else {
                    self.runtime.set_power_mode(PowerMode::Sleep);
                }

                // Check if migration is needed
                if self.should_migrate() && self.agent.is_some() {
                    warn!("🚨 CRITICAL BATTERY - TRIGGERING MIGRATION");
                    warn!("   Current level: {}%", battery_level);
                    warn!(
                        "   Agent ID: {}",
                        self.agent.as_ref().map(|a| a.id()).unwrap_or_default()
                    );
                    warn!("   Target: Nearest node with power");
                }

                last_battery_update = Instant::now();

                // Print status every 10 seconds
                if iteration.is_multiple_of(100) {
                    info!("📊 Status:");
                    info!("   Battery: {}%", battery_level);
                    info!("   Power mode: {:?}", self.runtime.power_mode());
                }
            }

            // Record power measurement
            if iteration.is_multiple_of(10) {
                let voltage_mv: u32 = 3700u32.saturating_sub((100u32 - battery_level as u32) * 5);
                let current_ma: i32 = match self.runtime.power_mode() {
                    PowerMode::Normal => 50,
                    PowerMode::LowPower => 15,
                    PowerMode::UltraLowPower => 3,
                    PowerMode::Sleep | PowerMode::Standby | PowerMode::Shutdown => 1,
                };

                let measurement = PowerMeasurement::new(voltage_mv, current_ma, iteration * 100);
                if let Some(anomaly) = self.runtime.record_power_measurement(measurement) {
                    warn!("⚠️  Power anomaly detected: {:?}", anomaly);
                }
            }

            // Simulate periodic execution
            std::thread::sleep(Duration::from_millis(100));
        }

        info!("");
        info!("✅ Simulation completed");

        // Display final statistics
        self.display_statistics();

        Ok(())
    }

    /// Display node statistics
    fn display_statistics(&self) {
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📊 IoT Sensor Node Statistics");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Battery status
        if let Some(level) = self.runtime.battery_level() {
            info!("🔋 Battery:");
            info!("   Level: {}%", level);
            info!("   Power mode: {:?}", self.runtime.power_mode());
            info!("   Migration trigger: {}", self.should_migrate());
        }

        // Power statistics
        if let Some(stats) = self.runtime.power_statistics() {
            info!("⚡ Power Statistics:");
            info!("   Samples: {}", stats.sample_count);
            info!(
                "   Avg voltage: {:.2}V",
                stats.avg_voltage_mv as f32 / 1000.0
            );
            info!("   Avg current: {:.2}mA", stats.avg_current_ma);
            info!("   Avg power: {:.2}mW", stats.avg_power_mw);
        }

        // Energy profiling
        let summary = self.energy_profiler.task_summary();
        info!("📈 Energy Profile:");
        info!("   Tracked tasks: {}", summary.task_count);

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }
}

fn main() -> Result<(), RuntimeError> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info!("═══════════════════════════════════════════════════════");
    info!("  MielinOS End-to-End Embedded IoT");
    info!("  Version: v0.0.1 (Development Preview)");
    info!("  Simulated Sensor Node with Battery Management");
    info!("═══════════════════════════════════════════════════════");
    info!("");

    // Create sensor node
    let mut node = IoTSensorNode::new()?;

    // Initialize
    node.init()?;
    info!("");

    // Deploy a sample agent
    let agent_wasm = vec![
        0x00, 0x61, 0x73, 0x6d, // WASM magic
        0x01, 0x00, 0x00, 0x00, // Version
    ];
    let agent = Agent::new(agent_wasm);
    node.deploy_agent(agent);
    info!("");

    // Run simulation for 30 seconds (reduced for faster demo)
    node.run_simulation(30)?;

    info!("");
    info!("═══════════════════════════════════════════════════════");
    info!("✅ Embedded IoT simulation completed successfully!");
    info!("═══════════════════════════════════════════════════════");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_node_creation() {
        let node = IoTSensorNode::new();
        assert!(node.is_ok());
    }

    #[test]
    fn test_sensor_node_initialization() {
        let mut node = IoTSensorNode::new().expect("Failed to create node");
        assert!(node.init().is_ok());
    }

    #[test]
    fn test_battery_discharge_simulation() {
        let node = IoTSensorNode::new().expect("Failed to create node");
        let initial_level = 100;
        let discharged = node.simulate_battery_discharge(initial_level, 3600);
        assert!(discharged < initial_level);
    }
}
