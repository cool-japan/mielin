//! Production System Integration Example
//!
//! This example demonstrates a complete production-ready embedded system
//! integrating all major mielin-rt features:
//! - Security (secure boot, encrypted updates)
//! - OTA firmware updates
//! - Watchdog monitoring
//! - Fault recovery
//! - Remote monitoring and diagnostics
//! - Power management
//! - Sensor integration
//!
//! This represents a real-world IoT device with full production features.

#![no_std]
#![no_main]

extern crate alloc;
extern crate panic_halt;

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

use mielin_rt::fault::{
    FaultRecoveryManager, FaultSeverity, FaultSource, FaultType, RecoveryPolicy,
};
use mielin_rt::monitoring::{MetricMetadata, MetricType, MetricValue, RemoteMonitor};
use mielin_rt::ota::{OtaManager, PartitionId, PartitionInfo, UpdateTransport, Version};
use mielin_rt::security::{BootStage, HashAlgorithm, SecureBootVerifier, SignatureAlgorithm};
use mielin_rt::watchdog::{WatchdogConfig, WatchdogManager};
use mielin_rt::{power::PowerMode, EmbeddedRuntime};

// System configuration
const FIRMWARE_VERSION: Version = Version::new(1, 0, 0, 1);
const WATCHDOG_TIMEOUT_MS: u32 = 5000;
const HEALTH_CHECK_INTERVAL_MS: u32 = 10000;
const OTA_CHECK_INTERVAL_MS: u32 = 3600000; // 1 hour

// Metric IDs
const METRIC_CPU: u32 = 1;
const METRIC_MEMORY: u32 = 2;
const METRIC_TEMP: u32 = 3;
const METRIC_BATTERY: u32 = 4;
const METRIC_UPTIME: u32 = 5;

static mut SYSTEM_TIME_MS: u32 = 0;

fn system_time_ms() -> u32 {
    unsafe { SYSTEM_TIME_MS }
}

fn advance_time(ms: u32) {
    unsafe {
        SYSTEM_TIME_MS += ms;
    }
}

/// Production system state
struct ProductionSystem {
    runtime: EmbeddedRuntime,
    secure_boot: SecureBootVerifier,
    ota_manager: OtaManager,
    watchdog_manager: WatchdogManager,
    fault_manager: FaultRecoveryManager,
    monitor: RemoteMonitor,
    last_health_check: u32,
    last_ota_check: u32,
}

impl ProductionSystem {
    fn new() -> Self {
        // Initialize runtime
        let runtime = EmbeddedRuntime::new();

        // Initialize secure boot
        let secure_boot = SecureBootVerifier::new(
            BootStage::Application,
            HashAlgorithm::Sha256,
            SignatureAlgorithm::EcdsaP256,
        );

        // Initialize OTA manager
        let mut ota_manager =
            OtaManager::new(FIRMWARE_VERSION, PartitionId::A, UpdateTransport::Coap);

        // Register partitions
        let partition_a = PartitionInfo::new(PartitionId::A, 1024 * 1024, 0x08000000);
        let partition_b = PartitionInfo::new(PartitionId::B, 1024 * 1024, 0x08100000);
        let _ = ota_manager.register_partition(partition_a);
        let _ = ota_manager.register_partition(partition_b);

        // Initialize watchdog manager
        let mut watchdog_manager = WatchdogManager::new(system_time_ms);

        // Add system watchdog
        let system_wd_config = WatchdogConfig::independent(WATCHDOG_TIMEOUT_MS);
        let _ = watchdog_manager.add_watchdog(system_wd_config);

        // Enable task watchdog
        watchdog_manager.enable_task_watchdog(3000);

        // Initialize fault manager
        let policy = RecoveryPolicy::conservative();
        let fault_manager = FaultRecoveryManager::new(policy, system_time_ms);

        // Initialize monitoring
        let mut monitor = RemoteMonitor::new(system_time_ms);
        Self::setup_monitoring(&mut monitor);

        let current_time = system_time_ms();

        Self {
            runtime,
            secure_boot,
            ota_manager,
            watchdog_manager,
            fault_manager,
            monitor,
            last_health_check: current_time,
            last_ota_check: current_time,
        }
    }

    fn setup_monitoring(monitor: &mut RemoteMonitor) {
        // Register metrics
        for &(id, metric_type) in &[
            (METRIC_CPU, MetricType::Gauge),
            (METRIC_MEMORY, MetricType::Gauge),
            (METRIC_TEMP, MetricType::Gauge),
            (METRIC_BATTERY, MetricType::Gauge),
            (METRIC_UPTIME, MetricType::Counter),
        ] {
            let metadata = MetricMetadata {
                id,
                metric_type,
                unit: 0,
                description: 0,
            };
            let _ = monitor.register_metric(metadata);
        }
    }

    fn verify_boot(&mut self) -> Result<(), ()> {
        // Verify firmware integrity
        let firmware_data = [0u8; 1024];
        let expected_hash = [0u8; 32];

        if self
            .secure_boot
            .verify_hash(&firmware_data, &expected_hash)
            .is_err()
        {
            let source = FaultSource::new(1, 1, 1);
            self.fault_manager.handle_fault(
                FaultType::Software,
                FaultSeverity::Fatal,
                source,
                0x1000,
            );
            return Err(());
        }

        Ok(())
    }

    fn initialize(&mut self) -> Result<(), ()> {
        // Verify boot integrity
        self.verify_boot()?;

        // Initialize runtime
        self.runtime.init().map_err(|_| ())?;

        // Start watchdogs
        self.watchdog_manager.start_all().map_err(|_| ())?;

        // Register tasks
        if let Some(task_wd) = self.watchdog_manager.task_watchdog() {
            let _ = task_wd.register_task(1, 2000); // Sensor task
            let _ = task_wd.register_task(2, 3000); // Communication task
            let _ = task_wd.register_task(3, 1000); // Control task
        }

        Ok(())
    }

    fn run_cycle(&mut self) {
        let current_time = system_time_ms();

        // Refresh watchdogs
        if self.watchdog_manager.refresh_all().is_err() {
            let source = FaultSource::new(2, 100, 2);
            self.fault_manager.handle_fault(
                FaultType::Watchdog,
                FaultSeverity::Critical,
                source,
                0x2000,
            );
        }

        // Task checkins
        if let Some(task_wd) = self.watchdog_manager.task_watchdog() {
            let _ = task_wd.checkin(1, current_time);
            let _ = task_wd.checkin(2, current_time);
            let _ = task_wd.checkin(3, current_time);

            let timed_out = task_wd.check_timeouts(current_time);
            for &task_id in timed_out.iter() {
                let source = FaultSource::new(3, 200, 3);
                self.fault_manager.handle_fault(
                    FaultType::Software,
                    FaultSeverity::Error,
                    source,
                    0x3000 + task_id,
                );
            }
        }

        // Periodic health check
        if current_time - self.last_health_check >= HEALTH_CHECK_INTERVAL_MS {
            self.perform_health_check();
            self.last_health_check = current_time;
        }

        // Periodic OTA check
        if current_time - self.last_ota_check >= OTA_CHECK_INTERVAL_MS {
            self.check_for_updates();
            self.last_ota_check = current_time;
        }

        // Collect metrics
        self.collect_metrics();

        // Process tasks
        self.run_sensor_task();
        self.run_communication_task();
        self.run_control_task();
    }

    fn perform_health_check(&mut self) {
        let cpu = self.measure_cpu_usage();
        let memory = self.measure_memory_usage();
        let temp = self.measure_temperature();
        let faults = self.fault_manager.total_fault_count() as u16;

        let health = self.monitor.health_report(cpu, memory, temp, faults);

        match health.status {
            mielin_rt::monitoring::HealthStatus::Critical => {
                // Enter safe mode
                self.fault_manager.enter_safe_mode();
                self.runtime.set_power_mode(PowerMode::LowPower);
            }
            mielin_rt::monitoring::HealthStatus::Unhealthy => {
                // Reduce load
                self.runtime.set_power_mode(PowerMode::LowPower);
            }
            _ => {
                // Normal operation
            }
        }
    }

    fn check_for_updates(&mut self) {
        match self.ota_manager.check_for_update() {
            Ok(Some(metadata)) => {
                // Update available - initiate download
                if self.ota_manager.start_download(metadata).is_ok() {
                    // Download in background
                    // (In real system, this would be asynchronous)
                }
            }
            Ok(None) => {
                // No update available
            }
            Err(_) => {
                let source = FaultSource::new(4, 300, 4);
                self.fault_manager.record_fault(
                    FaultType::Communication,
                    FaultSeverity::Warning,
                    source,
                    0x4000,
                );
            }
        }
    }

    fn collect_metrics(&mut self) {
        let cpu = self.measure_cpu_usage();
        let memory = self.measure_memory_usage();
        let temp = self.measure_temperature();
        let battery = self.measure_battery_voltage();
        let uptime = self.monitor.uptime_seconds();

        let _ = self
            .monitor
            .record_metric(METRIC_CPU, MetricValue::Percentage(cpu));
        let _ = self
            .monitor
            .record_metric(METRIC_MEMORY, MetricValue::Percentage(memory));
        let _ = self
            .monitor
            .record_metric(METRIC_TEMP, MetricValue::Integer(temp as i64));
        let _ = self
            .monitor
            .record_metric(METRIC_BATTERY, MetricValue::Integer(battery as i64));
        let _ = self
            .monitor
            .record_metric(METRIC_UPTIME, MetricValue::Integer(uptime as i64));
    }

    fn run_sensor_task(&mut self) {
        // Read sensors, process data
        // If sensor error occurs, record fault
    }

    fn run_communication_task(&mut self) {
        // Handle network communication
        // If communication fails, record fault
    }

    fn run_control_task(&mut self) {
        // Execute control logic
        // If control error occurs, record fault
    }

    fn measure_cpu_usage(&self) -> u8 {
        45
    }
    fn measure_memory_usage(&self) -> u8 {
        60
    }
    fn measure_temperature(&self) -> i16 {
        42
    }
    fn measure_battery_voltage(&self) -> u16 {
        3800
    }
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    let mut system = ProductionSystem::new();

    // Initialize system
    if system.initialize().is_err() {
        // Boot verification failed - halt
        loop {}
    }

    // Main system loop
    loop {
        advance_time(100); // 100ms cycle time

        system.run_cycle();

        // Check system state
        match system.fault_manager.state() {
            mielin_rt::fault::SystemState::Normal => {
                // Continue normal operation
            }
            mielin_rt::fault::SystemState::SafeMode => {
                // Limited operation in safe mode
            }
            mielin_rt::fault::SystemState::Recovery => {
                // Attempting recovery
            }
            mielin_rt::fault::SystemState::FactoryReset => {
                // Perform factory reset
                break;
            }
        }

        // Check boot loop
        if system.fault_manager.is_boot_loop() {
            // Too many failed boots
            break;
        }
    }

    loop {}
}

// Note: In production, you would implement a custom panic handler like this:
// #[panic_handler]
// fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
//     let policy = RecoveryPolicy::conservative();
//     let mut fault_manager = FaultRecoveryManager::new(policy, system_time_ms);
//     let source = FaultSource::unknown();
//     fault_manager.handle_fault(FaultType::Software, FaultSeverity::Fatal, source, 0xFFFF);
//     let _crash_dump = fault_manager.generate_crash_dump();
//     loop {}
// }
