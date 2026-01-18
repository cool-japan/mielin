//! Fault Recovery Example
//!
//! This example demonstrates:
//! - Fault detection and classification
//! - Automatic recovery strategies
//! - Fault logging and analysis
//! - Boot-loop detection
//! - Safe mode operation
//! - Crash dump generation

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
    FaultRecoveryManager, FaultSeverity, FaultSource, FaultType, RecoveryAction, RecoveryPolicy,
    SystemState,
};

static mut SYSTEM_TIME_MS: u32 = 0;

fn system_time_ms() -> u32 {
    unsafe { SYSTEM_TIME_MS }
}

fn advance_time(ms: u32) {
    unsafe {
        SYSTEM_TIME_MS += ms;
    }
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Example 1: Basic Fault Recording
    basic_fault_recording();

    // Example 2: Automatic Recovery
    automatic_recovery_example();

    // Example 3: Boot Loop Detection
    boot_loop_example();

    // Example 4: Safe Mode Operation
    safe_mode_example();

    // Example 5: Fault Analysis
    fault_analysis_example();

    loop {}
}

fn basic_fault_recording() {
    let policy = RecoveryPolicy::conservative();
    let mut fault_manager = FaultRecoveryManager::new(policy, system_time_ms);

    // Record various types of faults

    // Software fault (e.g., assertion failure)
    let source = FaultSource::new(1, 142, 5); // file_id=1, line=142, func=5
    fault_manager.record_fault(
        FaultType::Software,
        FaultSeverity::Error,
        source,
        0x1001, // Error code
    );

    // Hardware fault (e.g., peripheral timeout)
    let source = FaultSource::new(2, 87, 12);
    fault_manager.record_fault(FaultType::Hardware, FaultSeverity::Warning, source, 0x2001);

    // Communication fault (e.g., network timeout)
    let source = FaultSource::new(3, 201, 23);
    fault_manager.record_fault(
        FaultType::Communication,
        FaultSeverity::Error,
        source,
        0x3001,
    );

    // Check fault counts
    let _software_faults = fault_manager.get_fault_count(FaultType::Software);
    let _hardware_faults = fault_manager.get_fault_count(FaultType::Hardware);
    let _total_faults = fault_manager.total_fault_count();

    // Access fault log
    let log = fault_manager.log();
    let recent_faults = log.recent(10);

    // Analyze recent faults
    for fault in recent_faults.iter() {
        match fault.severity {
            FaultSeverity::Critical | FaultSeverity::Fatal => {
                // Critical fault detected
            }
            _ => {}
        }
    }
}

fn automatic_recovery_example() {
    let policy = RecoveryPolicy::conservative();
    let mut fault_manager = FaultRecoveryManager::new(policy, system_time_ms);

    // Simulate various fault scenarios

    // Scenario 1: Communication timeout (should trigger Retry)
    let source = FaultSource::new(1, 100, 1);
    let action = fault_manager.handle_fault(
        FaultType::Communication,
        FaultSeverity::Error,
        source,
        0x1001,
    );

    if action == RecoveryAction::Retry {
        // Retry the operation
        for attempt in 0..3 {
            // Attempt operation
            if attempt == 2 {
                // Success on third try
                break;
            }
        }
    }

    // Scenario 2: Peripheral failure (should trigger ResetPeripheral)
    let source = FaultSource::new(2, 150, 2);
    let action =
        fault_manager.handle_fault(FaultType::Peripheral, FaultSeverity::Error, source, 0x2001);

    if action == RecoveryAction::ResetPeripheral {
        // Reset and reinitialize peripheral
    }

    // Scenario 3: Critical hardware fault (should trigger SafeMode)
    let source = FaultSource::new(3, 200, 3);
    let action =
        fault_manager.handle_fault(FaultType::Hardware, FaultSeverity::Critical, source, 0x3001);

    if action == RecoveryAction::SafeMode {
        // System entered safe mode
        assert_eq!(fault_manager.state(), SystemState::SafeMode);
    }

    // Scenario 4: Fatal watchdog timeout (should trigger Reboot)
    let source = FaultSource::new(4, 250, 4);
    let action =
        fault_manager.handle_fault(FaultType::Watchdog, FaultSeverity::Fatal, source, 0x4001);

    if action == RecoveryAction::Reboot {
        // Generate crash dump before reboot
        let _crash_dump = fault_manager.generate_crash_dump();
        // Store crash dump to non-volatile memory
        // Trigger system reboot
    }
}

fn boot_loop_example() {
    let mut policy = RecoveryPolicy::conservative();
    policy.max_reboots = 3;
    let mut fault_manager = FaultRecoveryManager::new(policy, system_time_ms);

    // Simulate boot sequence
    for boot_attempt in 1..=5 {
        // Simulate fault during boot
        let source = FaultSource::new(5, 10, 5);

        if boot_attempt <= 3 {
            // First 3 attempts trigger reboot
            let action = fault_manager.handle_fault(
                FaultType::Software,
                FaultSeverity::Fatal,
                source,
                0x5000 + boot_attempt,
            );

            match action {
                RecoveryAction::Reboot => {
                    // Reboot system
                }
                RecoveryAction::FactoryReset => {
                    // Too many reboots - factory reset
                    break;
                }
                _ => {}
            }
        } else {
            // After 3 failed boots, should trigger factory reset
            let action = fault_manager.handle_fault(
                FaultType::Software,
                FaultSeverity::Fatal,
                source,
                0x5000 + boot_attempt,
            );

            assert_eq!(action, RecoveryAction::FactoryReset);
            break;
        }
    }

    // Check if boot loop was detected
    assert!(fault_manager.is_boot_loop());

    // Reset counter after successful boot
    fault_manager.reset_reboot_counter();
}

fn safe_mode_example() {
    let policy = RecoveryPolicy::conservative();
    let mut fault_manager = FaultRecoveryManager::new(policy, system_time_ms);

    // Normal operation
    assert_eq!(fault_manager.state(), SystemState::Normal);

    // Trigger critical fault
    let source = FaultSource::new(6, 300, 6);
    fault_manager.handle_fault(FaultType::Memory, FaultSeverity::Critical, source, 0x6001);

    // System should be in safe mode
    assert_eq!(fault_manager.state(), SystemState::SafeMode);

    // In safe mode:
    // - Disable non-critical features
    // - Reduce power consumption
    // - Enable remote diagnostics
    // - Wait for manual intervention or recovery

    // Safe mode operations
    safe_mode_operations();

    // After diagnostics and fix, exit safe mode
    fault_manager.exit_safe_mode();
    assert_eq!(fault_manager.state(), SystemState::Normal);
}

fn safe_mode_operations() {
    // Limited functionality in safe mode
    // - Basic communication only
    // - No sensor processing
    // - No power-intensive operations
    // - Remote monitoring enabled
}

fn fault_analysis_example() {
    let policy = RecoveryPolicy::aggressive();
    let mut fault_manager = FaultRecoveryManager::new(policy, system_time_ms);

    // Generate various faults over time
    for i in 0..20 {
        advance_time(1000);

        let fault_type = match i % 4 {
            0 => FaultType::Communication,
            1 => FaultType::Hardware,
            2 => FaultType::Software,
            _ => FaultType::Power,
        };

        let severity = if i % 5 == 0 {
            FaultSeverity::Critical
        } else {
            FaultSeverity::Error
        };

        let source = FaultSource::new(7, 400 + i, 7);
        fault_manager.record_fault(fault_type, severity, source, 0x7000 + i);
    }

    // Analyze fault patterns
    let log = fault_manager.log();

    // Get critical faults
    let _critical_faults = log.by_severity(FaultSeverity::Critical);

    // Get communication faults
    let _comm_faults = log.by_type(FaultType::Communication);

    // Analyze fault frequency
    let total = fault_manager.total_fault_count();
    let comm_count = fault_manager.get_fault_count(FaultType::Communication);
    let hw_count = fault_manager.get_fault_count(FaultType::Hardware);

    // Calculate fault rates
    let comm_rate = (comm_count as f32 / total as f32) * 100.0;
    let _hw_rate = (hw_count as f32 / total as f32) * 100.0;

    // Check if specific fault type is dominant
    if comm_rate > 50.0 {
        // Communication issues are the primary problem
        // Take corrective action
    }

    // Generate crash dump for analysis
    let _crash_dump = fault_manager.generate_crash_dump();
    // Crash dump contains:
    // - Reboot counter
    // - Fault counts by type
    // - Can be extended with register dumps, stack traces, etc.
}

// Example: Fault handler integration with panic handler
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
