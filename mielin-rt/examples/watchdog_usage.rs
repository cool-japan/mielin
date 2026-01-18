//! Watchdog Usage Example
//!
//! This example demonstrates:
//! - Independent watchdog configuration
//! - Window watchdog configuration
//! - Task-level watchdog monitoring
//! - Watchdog manager for multiple instances
//! - Statistics tracking

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

use mielin_rt::watchdog::{TaskWatchdog, Watchdog, WatchdogConfig, WatchdogManager};

// Simulated system time counter
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
    // Example 1: Independent Watchdog
    independent_watchdog_example();

    // Example 2: Window Watchdog
    window_watchdog_example();

    // Example 3: Task Watchdog
    task_watchdog_example();

    // Example 4: Watchdog Manager
    watchdog_manager_example();

    loop {}
}

fn independent_watchdog_example() {
    // Create independent watchdog with 5 second timeout
    let config = WatchdogConfig::independent(5000);
    let mut watchdog = Watchdog::new(config).expect("Failed to create watchdog");

    // Start the watchdog
    watchdog.start().expect("Failed to start watchdog");

    // Main loop - must refresh within timeout
    for _ in 0..10 {
        // Do work
        advance_time(1000); // Simulate 1 second of work

        // Refresh watchdog
        let current_time = system_time_ms();
        if watchdog.refresh(current_time).is_ok() {
            // Watchdog refreshed successfully
        } else {
            // Refresh failed - system will reset
            break;
        }
    }

    // Get statistics
    let _stats = watchdog.stats();
    // Total refreshes: _stats.refresh_count
    // Timeouts: _stats.timeout_count
}

fn window_watchdog_example() {
    // Create window watchdog
    // Timeout: 1000ms, Window: 500-1000ms
    let config = WatchdogConfig::window(1000, 500);
    let mut watchdog = Watchdog::new(config).expect("Failed to create watchdog");

    // Start the watchdog
    watchdog.start().expect("Failed to start watchdog");

    // First refresh can happen immediately
    let current_time = system_time_ms();
    watchdog
        .refresh(current_time)
        .expect("First refresh failed");

    // Main loop with windowed refresh
    for _ in 0..5 {
        // Do work for 600ms (within 500-1000ms window)
        advance_time(600);

        // Refresh within window
        let current_time = system_time_ms();
        match watchdog.refresh(current_time) {
            Ok(_) => {
                // Refresh successful
            }
            Err(e) => {
                // Refresh failed - either too early or too late
                match e {
                    mielin_rt::watchdog::WatchdogError::RefreshTooEarly => {
                        // Refreshed before window start (< 500ms)
                    }
                    mielin_rt::watchdog::WatchdogError::RefreshTooLate => {
                        // Refreshed after timeout (>= 1000ms)
                    }
                    _ => {}
                }
                break;
            }
        }
    }
}

fn task_watchdog_example() {
    let mut task_watchdog = TaskWatchdog::new(5000); // 5 second global timeout

    // Register tasks with individual timeouts
    task_watchdog
        .register_task(1, 2000)
        .expect("Failed to register task 1");
    task_watchdog
        .register_task(2, 3000)
        .expect("Failed to register task 2");
    task_watchdog
        .register_task(3, 1000)
        .expect("Failed to register task 3");

    // Simulate task execution
    for cycle in 0..10 {
        advance_time(500);
        let current_time = system_time_ms();

        // Task 1 checks in every cycle
        task_watchdog
            .checkin(1, current_time)
            .expect("Task 1 checkin failed");

        // Task 2 checks in every other cycle
        if cycle % 2 == 0 {
            task_watchdog
                .checkin(2, current_time)
                .expect("Task 2 checkin failed");
        }

        // Task 3 checks in every cycle (fast task)
        task_watchdog
            .checkin(3, current_time)
            .expect("Task 3 checkin failed");

        // Check for timeouts
        let timed_out = task_watchdog.check_timeouts(current_time);
        if !timed_out.is_empty() {
            // One or more tasks timed out
            for task_id in timed_out.iter() {
                // Handle timeout - restart task or trigger recovery
                match task_id {
                    1 => {
                        // Task 1 timed out
                    }
                    2 => {
                        // Task 2 timed out
                    }
                    3 => {
                        // Task 3 timed out
                    }
                    _ => {}
                }
            }
        }
    }

    // Get statistics
    let _timeout_count = task_watchdog.timeout_count();
    let _task_count = task_watchdog.task_count();
}

fn watchdog_manager_example() {
    let mut manager = WatchdogManager::new(system_time_ms);

    // Add independent watchdog for critical system tasks
    let system_config = WatchdogConfig::independent(10000);
    manager
        .add_watchdog(system_config)
        .expect("Failed to add system watchdog");

    // Add window watchdog for periodic tasks
    let periodic_config = WatchdogConfig::window(2000, 1000);
    manager
        .add_watchdog(periodic_config)
        .expect("Failed to add periodic watchdog");

    // Enable task watchdog for application tasks
    manager.enable_task_watchdog(5000);

    // Register application tasks
    if let Some(task_wd) = manager.task_watchdog() {
        task_wd
            .register_task(100, 3000)
            .expect("Failed to register task 100");
        task_wd
            .register_task(101, 4000)
            .expect("Failed to register task 101");
    }

    // Start all watchdogs
    manager.start_all().expect("Failed to start watchdogs");

    // Main system loop
    for _ in 0..20 {
        // Do system work
        advance_time(500);

        // Refresh all hardware watchdogs
        if manager.refresh_all().is_err() {
            // One or more watchdogs failed to refresh
            // System will reset
            break;
        }

        // Task checkins
        if let Some(task_wd) = manager.task_watchdog() {
            let current_time = system_time_ms();
            let _ = task_wd.checkin(100, current_time);
            let _ = task_wd.checkin(101, current_time);

            // Check for task timeouts
            let timed_out = task_wd.check_timeouts(current_time);
            if !timed_out.is_empty() {
                // Handle task timeouts
            }
        }

        // Check all watchdog timeouts
        if manager.check_all_timeouts() {
            // At least one watchdog timed out
            // System will reset
            break;
        }
    }
}

// Example: Watchdog in interrupt-driven system
fn _interrupt_driven_example() {
    let config = WatchdogConfig::independent(1000);
    let watchdog = Watchdog::new(config).expect("Failed to create watchdog");
    watchdog.start().expect("Failed to start watchdog");

    // In timer interrupt (called every 100ms)
    fn _timer_interrupt_handler(watchdog: &mut Watchdog) {
        let current_time = system_time_ms();

        // Refresh watchdog in interrupt
        if watchdog.refresh(current_time).is_err() {
            // Failed to refresh - system will reset
        }

        // Check statistics
        let stats = watchdog.stats();
        if stats.refresh_count.is_multiple_of(10) {
            // Every 10 refreshes, check intervals
            let _min_interval = stats.min_refresh_interval_ms;
            let max_interval = stats.max_refresh_interval_ms;

            // Ensure intervals are within expected range
            if max_interval > 150 {
                // Refresh interval too long - system may be overloaded
            }
        }
    }
}
