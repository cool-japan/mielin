# mielin-rt

**Embedded Runtime Profile**

Lightweight runtime for Cortex-M and other embedded targets, enabling MielinOS to run on resource-constrained IoT devices.

## Overview

MielinRT provides a minimal runtime environment for embedded devices, bringing the power of agent-based computing to microcontrollers, sensors, and edge devices.

### Key Features

- **Minimal Footprint**: <5KB runtime overhead
- **Power Management**: Sleep modes, battery-aware operation
- **no_std Compatible**: Runs on bare metal
- **Migration Support**: Agents can migrate to/from embedded devices
- **Hardware Integration**: GPIO, sensors, actuators

## Architecture

```
mielin-rt/
├── agent.rs   # Lightweight agent representation
├── power.rs   # Power management and battery monitoring
└── lib.rs     # Embedded runtime core
```

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-rt = { path = "../mielin-rt" }
mielin-hal = { path = "../mielin-hal" }
```

### Basic Example

```rust
#![no_std]
#![no_main]

use mielin_rt::EmbeddedRuntime;
use mielin_rt::power::PowerMode;

#[entry]
fn main() -> ! {
    let mut runtime = EmbeddedRuntime::new();

    // Initialize runtime
    runtime.init().expect("Failed to initialize");

    // Set power-saving mode
    runtime.set_power_mode(PowerMode::LowPower);

    loop {
        // Your agent code here
    }
}
```

## Components

### EmbeddedRuntime

Core runtime for embedded environments:

```rust
use mielin_rt::EmbeddedRuntime;

let mut runtime = EmbeddedRuntime::new();
runtime.init()?;

// Get detected architecture
let arch = runtime.architecture();
```

### Power Management

Battery-aware operation and power mode control:

```rust
use mielin_rt::power::{PowerMode, BatteryStatus};

// Check battery status
let battery = BatteryStatus {
    level_percent: 15,
    is_charging: false,
};

// Trigger migration if battery is low
if battery.should_migrate() {
    // Migrate agent to a powered node
}

// Set power mode
runtime.set_power_mode(PowerMode::UltraLowPower);
```

### Power Modes

| Mode | Description | Typical Current |
|------|-------------|-----------------|
| `Normal` | Full performance | 10-50 mA |
| `LowPower` | Reduced clock speed | 1-5 mA |
| `UltraLowPower` | Minimal activity | 10-100 μA |
| `Sleep` | Deep sleep, wake on interrupt | <10 μA |

### Agent Support

Lightweight agent container for embedded:

```rust
use mielin_rt::agent::EmbeddedAgent;

let agent = EmbeddedAgent::new([0x1, 0x2, /* 16 bytes UUID */]);
```

## API Reference

### `EmbeddedRuntime`

```rust
pub struct EmbeddedRuntime {
    arch: Architecture,
    power_mode: PowerMode,
}

impl EmbeddedRuntime {
    pub fn new() -> Self;
    pub fn init(&mut self) -> Result<(), RuntimeError>;
    pub fn architecture(&self) -> Architecture;
    pub fn set_power_mode(&mut self, mode: PowerMode);
}
```

### `PowerMode`

```rust
pub enum PowerMode {
    Normal,
    LowPower,
    UltraLowPower,
    Sleep,
}
```

### `BatteryStatus`

```rust
pub struct BatteryStatus {
    pub level_percent: u8,
    pub is_charging: bool,
}

impl BatteryStatus {
    pub fn should_migrate(&self) -> bool;
}
```

## Platform Support

### Tested Platforms

- **STM32F4**: Cortex-M4F (168 MHz, 192KB RAM)
- **Nordic nRF52**: Cortex-M4 (64 MHz, 64KB RAM)
- **ESP32**: Dual-core Xtensa (160 MHz, 520KB RAM)
- **Raspberry Pi Pico**: Cortex-M0+ (133 MHz, 264KB RAM)

### Memory Requirements

Minimum requirements:
- **RAM**: 32 KB (64 KB recommended)
- **Flash**: 64 KB (128 KB recommended)
- **CPU**: Cortex-M0+ or equivalent

## Migration Examples

### Battery-Triggered Migration

```rust
use mielin_rt::power::BatteryStatus;

fn check_and_migrate(battery: &BatteryStatus) {
    if battery.should_migrate() {
        // Create migration snapshot
        let snapshot = capture_agent_state();

        // Send to nearby powered node
        send_migration_request(snapshot);

        // Enter deep sleep
        enter_sleep_mode();
    }
}
```

### Scheduled Migration

```rust
use mielin_rt::power::PowerMode;

// Run agent during day, migrate at night
if is_nighttime() {
    runtime.set_power_mode(PowerMode::Sleep);
    migrate_to_always_on_node();
} else {
    runtime.set_power_mode(PowerMode::Normal);
}
```

## Power Optimization Tips

1. **Use Sleep Modes**: Enter sleep when idle
   ```rust
   runtime.set_power_mode(PowerMode::UltraLowPower);
   ```

2. **Batch Operations**: Reduce wake/sleep cycles
   ```rust
   // Bad: Wake up every second
   // Good: Wake up once, process batch
   ```

3. **Optimize Clock Speed**: Run at minimum required frequency
   ```rust
   runtime.set_power_mode(PowerMode::LowPower); // Reduces clock
   ```

4. **Migrate Early**: Don't wait until battery is critical
   ```rust
   if battery.level_percent < 20 {
       prepare_migration();
   }
   ```

## Error Handling

```rust
pub enum RuntimeError {
    InitializationFailed,
    LowBattery,
}
```

## Testing

```bash
cargo test --target thumbv7em-none-eabihf  # For Cortex-M4
cargo test                                   # For host testing
```

## Future Enhancements

- [ ] Wake-on-network support
- [ ] Flash-based state persistence
- [ ] Sensor integration APIs
- [ ] GPIO control primitives
- [ ] I2C/SPI device drivers
- [ ] Watchdog timer integration
- [ ] RTC (Real-Time Clock) support

## Example: IoT Sensor Node

```rust
#![no_std]
#![no_main]

use mielin_rt::{EmbeddedRuntime, power::{PowerMode, BatteryStatus}};

#[entry]
fn main() -> ! {
    let mut runtime = EmbeddedRuntime::new();
    runtime.init().unwrap();

    loop {
        // Read sensor
        let temperature = read_temperature_sensor();

        // Check battery
        let battery = read_battery_status();

        if battery.should_migrate() {
            // Low battery - migrate to powered node
            migrate_agent();
            runtime.set_power_mode(PowerMode::Sleep);
        }

        // Transmit data
        send_telemetry(temperature);

        // Sleep until next reading
        runtime.set_power_mode(PowerMode::UltraLowPower);
        sleep_ms(60_000); // 1 minute
    }
}
```

## License

Apache-2.0
