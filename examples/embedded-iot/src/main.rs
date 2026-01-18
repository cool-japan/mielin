//! Embedded IoT Sensor Node Example
//!
//! Demonstrates MielinOS embedded runtime (mielin-rt) running on a
//! simulated IoT sensor node. Shows power management, battery monitoring,
//! and agent migration triggers.
//!
//! This example simulates a temperature sensor node that:
//! 1. Monitors battery level
//! 2. Adjusts power mode based on battery
//! 3. Triggers agent migration when battery is low
//! 4. Demonstrates sleep modes for power saving

use mielin_cells::Agent;
use mielin_rt::{power::*, EmbeddedRuntime, RuntimeError};

fn main() -> Result<(), RuntimeError> {
    println!("═══════════════════════════════════════════");
    println!("  MielinOS Embedded IoT Sensor Node");
    println!("  Simulated Temperature Monitoring Device");
    println!("═══════════════════════════════════════════\n");

    // Initialize embedded runtime
    let mut runtime = EmbeddedRuntime::new();
    runtime.init()?;

    println!("✅ Runtime initialized");
    println!("   Architecture: {:?}", runtime.architecture());
    println!();

    // Simulate sensor agent
    let wasm_binary = vec![
        0x00, 0x61, 0x73, 0x6d, // WASM magic number
        0x01, 0x00, 0x00, 0x00, // WASM version
    ];
    let agent = Agent::new(wasm_binary);
    println!("🤖 Temperature sensor agent created");
    println!("   Agent ID: {}", agent.id());
    println!();

    // Simulate battery lifecycle
    simulate_battery_lifecycle(&mut runtime, &agent);

    println!("\n═══════════════════════════════════════════");
    println!("  Simulation Complete");
    println!("═══════════════════════════════════════════");

    Ok(())
}

fn simulate_battery_lifecycle(runtime: &mut EmbeddedRuntime, agent: &Agent) {
    let scenarios = vec![
        (
            100,
            false,
            PowerMode::Normal,
            "Full battery, normal operation",
        ),
        (75, false, PowerMode::Normal, "Good battery level"),
        (
            50,
            false,
            PowerMode::LowPower,
            "Medium battery, entering low power mode",
        ),
        (
            25,
            false,
            PowerMode::UltraLowPower,
            "Low battery, ultra low power mode",
        ),
        (
            15,
            false,
            PowerMode::Sleep,
            "Critical battery, sleep mode + migration trigger",
        ),
        (
            15,
            true,
            PowerMode::Normal,
            "Charging detected, resuming normal operation",
        ),
        (80, true, PowerMode::Normal, "Battery restored"),
    ];

    for (level, is_charging, power_mode, description) in scenarios {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Battery Update:");
        println!("   Level: {}%", level);
        println!("   Charging: {}", if is_charging { "Yes" } else { "No" });
        println!("   Status: {}", description);

        // Update battery status
        let battery = BatteryStatus {
            level_percent: level,
            is_charging,
        };
        runtime.update_battery(battery);

        // Check if migration is needed
        if runtime.should_migrate() {
            println!("   🚨 MIGRATION TRIGGER ACTIVATED!");
            println!("   → Agent will migrate to preserve battery");
            println!("   → Target: Nearest relay node with power");
            println!("   → Agent ID: {}", agent.id());
        }

        // Set appropriate power mode
        runtime.set_power_mode(power_mode);
        println!("   ⚡ Power Mode: {:?}", runtime.power_mode());

        // Simulate power-saving sleep
        match power_mode {
            PowerMode::Sleep | PowerMode::UltraLowPower => {
                println!("   💤 Entering low power wait state...");
                // In real hardware, this would execute WFI (Wait For Interrupt)
                runtime.low_power_wait();
            }
            _ => {}
        }

        println!();
    }

    // Final summary
    println!("📈 Battery Lifecycle Summary:");
    println!(
        "   Final battery level: {}%",
        runtime.battery_level().unwrap()
    );
    println!("   Migration was triggered: {}", runtime.should_migrate());
    println!("   Current power mode: {:?}", runtime.power_mode());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_node_initialization() {
        let mut runtime = EmbeddedRuntime::new();
        assert!(runtime.init().is_ok());

        let wasm_binary = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let agent = Agent::new(wasm_binary);
        assert!(!agent.id().is_nil());
    }

    #[test]
    fn test_battery_monitoring() {
        let mut runtime = EmbeddedRuntime::new();

        // Start with full battery
        runtime.update_battery(BatteryStatus {
            level_percent: 100,
            is_charging: false,
        });
        assert!(!runtime.should_migrate());

        // Drain to critical level
        runtime.update_battery(BatteryStatus {
            level_percent: 15,
            is_charging: false,
        });
        assert!(runtime.should_migrate());
    }

    #[test]
    fn test_power_mode_transitions() {
        let mut runtime = EmbeddedRuntime::new();

        runtime.set_power_mode(PowerMode::Normal);
        assert_eq!(runtime.power_mode(), PowerMode::Normal);

        runtime.set_power_mode(PowerMode::LowPower);
        assert_eq!(runtime.power_mode(), PowerMode::LowPower);

        runtime.set_power_mode(PowerMode::Sleep);
        assert_eq!(runtime.power_mode(), PowerMode::Sleep);
    }
}
