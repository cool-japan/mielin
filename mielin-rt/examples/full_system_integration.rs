//! Full System Integration Example
//!
//! Demonstrates how to integrate all MielinRT components
//! for a complete embedded runtime system.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::Layout;

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

struct DummyAllocator;

unsafe impl core::alloc::GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

use panic_halt as _;

use mielin_rt::{
    battery::{BatteryConfig, BatteryManager, FuelGaugeReading},
    pool::{PoolAllocator, PoolConfig},
    power::{AdvancedPowerManager, PowerMode, WakeConfig, WakeSource},
    stack::StackManager,
};

#[cortex_m_rt::entry]
fn main() -> ! {
    // Initialize the complete runtime system
    let mut system = initialize_system();

    // Main application loop
    loop {
        // Update system state
        system.update();

        // Run task scheduler
        system.run_tasks();

        // Check power state and optimize
        system.optimize_power();

        // Monitor battery and adjust behavior
        system.monitor_battery();

        // Enter low-power mode when idle
        system.idle();
    }
}

struct System {
    power_manager: AdvancedPowerManager,
    battery_manager: BatteryManager,
    pool_allocator: PoolAllocator,
    _stack_manager: StackManager,
    tick_count: u64,
}

impl System {
    fn update(&mut self) {
        self.tick_count += 1;

        // Update battery status every 100 ticks (~100ms)
        if self.tick_count.is_multiple_of(100) {
            let reading = self.read_fuel_gauge();
            self.battery_manager.update(reading, self.tick_count * 1000);
        }

        // Check pool health every 1000 ticks (~1s)
        if self.tick_count.is_multiple_of(1000) && self.pool_allocator.check_integrity().is_err() {
            // Pool corruption detected
            self.handle_memory_corruption();
        }
    }

    fn run_tasks(&mut self) {
        // Task 1: Sensor reading (high priority)
        self.run_sensor_task();

        // Task 2: Data processing (medium priority)
        self.run_processing_task();

        // Task 3: Communication (low priority)
        self.run_communication_task();
    }

    fn optimize_power(&mut self) {
        // Adjust power mode based on workload
        if self.has_urgent_work() {
            // High priority work - maximum performance
            self.power_manager.set_mode(PowerMode::Normal).ok();
        } else if self.has_background_work() {
            // Background work - reduce power
            self.power_manager.set_mode(PowerMode::LowPower).ok();
        } else {
            // No work - deep sleep
            self.power_manager.set_mode(PowerMode::Sleep).ok();
        }

        // Battery-aware power management
        // Check battery level and adjust accordingly
        let battery_summary = self.battery_manager.summary();
        if battery_summary.is_low {
            // Battery low - ultra low power mode
            self.power_manager
                .safe_transition_to(PowerMode::UltraLowPower)
                .ok();

            // Reduce non-essential features
            self.disable_non_essential_features();
        }
    }

    fn monitor_battery(&mut self) {
        let summary = self.battery_manager.summary();

        if summary.is_critical {
            // Battery critical - save state and prepare shutdown
            self.save_critical_state();
            self.power_manager
                .safe_transition_to(PowerMode::Standby)
                .ok();
        } else if summary.is_low {
            // Battery low - reduce power consumption
            self.reduce_power_consumption();
        }

        // Check battery health
        let health = self.battery_manager.health();
        if health.cycle_count > health.chemistry.typical_cycle_life() {
            // Battery may need replacement
            self.notify_battery_aging();
        }
    }

    fn idle(&self) {
        // Enter wait-for-interrupt
        cortex_m::asm::wfi();
    }

    // Task implementations
    fn run_sensor_task(&mut self) {
        // Allocate buffer for sensor data
        if let Ok(buffer) = self.pool_allocator.allocate(64) {
            // Read sensor data into buffer
            self.read_sensors(&buffer);

            // Process data
            self.process_sensor_data(&buffer);

            // Free buffer
            self.pool_allocator.deallocate(&buffer).ok();
        }
    }

    fn run_processing_task(&mut self) {
        // Allocate processing buffer
        if let Ok(buffer) = self.pool_allocator.allocate(256) {
            // Perform data processing
            self.process_data(&buffer);

            // Free buffer
            self.pool_allocator.deallocate(&buffer).ok();
        }
    }

    fn run_communication_task(&mut self) {
        // Allocate communication buffer
        if let Ok(buffer) = self.pool_allocator.allocate(128) {
            // Send/receive data
            self.communicate(&buffer);

            // Free buffer
            self.pool_allocator.deallocate(&buffer).ok();
        }
    }

    // Helper methods
    fn read_fuel_gauge(&self) -> FuelGaugeReading {
        // Read from actual hardware fuel gauge
        // This is a simulation
        FuelGaugeReading {
            soc_percent: 75,
            voltage_mv: 3800,
            current_ma: -200,
            temperature_deci_c: 250,
            ..Default::default()
        }
    }

    fn has_urgent_work(&self) -> bool {
        // Check if there's urgent work pending
        false
    }

    fn has_background_work(&self) -> bool {
        // Check if there's background work pending
        false
    }

    fn disable_non_essential_features(&mut self) {
        // Disable LEDs, reduce logging, etc.
    }

    fn save_critical_state(&self) {
        // Save application state to flash
    }

    fn reduce_power_consumption(&mut self) {
        // Reduce sampling rates, disable features, etc.
    }

    fn notify_battery_aging(&self) {
        // Notify user about battery aging
    }

    fn handle_memory_corruption(&mut self) {
        // Handle memory corruption - may need system reset
        panic!("Memory corruption detected!");
    }

    fn read_sensors(&self, _buffer: &mielin_rt::pool::Allocation) {
        // Read sensor data
    }

    fn process_sensor_data(&self, _buffer: &mielin_rt::pool::Allocation) {
        // Process sensor data
    }

    fn process_data(&self, _buffer: &mielin_rt::pool::Allocation) {
        // Process data
    }

    fn communicate(&self, _buffer: &mielin_rt::pool::Allocation) {
        // Communication task
    }
}

fn initialize_system() -> System {
    // Initialize power manager with wake sources
    let mut power_manager = AdvancedPowerManager::new();
    power_manager
        .wake_controller_mut()
        .add_source(WakeConfig::new(WakeSource::RtcAlarm));
    power_manager
        .wake_controller_mut()
        .add_source(WakeConfig::new(WakeSource::Gpio { port: 0, pin: 2 }));

    // Initialize battery manager
    let battery_config = BatteryConfig::single_cell_lipo(2000);
    let battery_manager = BatteryManager::new(battery_config);

    // Initialize memory pool
    let pool_config = PoolConfig::default();
    let mut pool_allocator = PoolAllocator::new(pool_config);
    pool_allocator.init();

    // Initialize stack manager
    let mut stack_manager = StackManager::new();

    // Register tasks
    register_tasks(&mut stack_manager);

    System {
        power_manager,
        battery_manager,
        pool_allocator,
        _stack_manager: stack_manager,
        tick_count: 0,
    }
}

fn register_tasks(stack_manager: &mut StackManager) {
    let config = mielin_rt::stack::StackConfig::default();

    // Register sensor task
    stack_manager
        .register_stack(0, 0x2000_0000, 4096, config)
        .ok();

    // Register processing task
    stack_manager
        .register_stack(1, 0x2000_1000, 4096, config)
        .ok();

    // Register communication task
    stack_manager
        .register_stack(2, 0x2000_2000, 4096, config)
        .ok();
}
