//! Timer and Preemptive Scheduling Example
//!
//! This example demonstrates the timer subsystem:
//! - Timer initialization and configuration
//! - Jiffies counter
//! - Timer statistics
//! - Integration with interrupt subsystem

use mielin_kernel::timer::{self, TimerConfig};
use mielin_kernel::{interrupt, scheduler};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Timer and Preemptive Scheduling Example ===\n");

    // Initialize required subsystems
    println!("1. Initializing subsystems...");
    scheduler::init()?;
    println!("   ✓ Scheduler initialized");

    interrupt::init()?;
    println!("   ✓ Interrupt subsystem initialized");

    // Initialize timer with custom configuration
    let config = TimerConfig {
        tick_rate_hz: 1000,  // 1ms ticks
        time_quantum_ms: 10, // 10ms time slices
    };
    timer::init(config)?;
    println!(
        "   ✓ Timer initialized ({}Hz, {}ms quantum)\n",
        config.tick_rate_hz, config.time_quantum_ms
    );

    // Get initial jiffies
    println!("2. Reading initial jiffies...");
    let initial_jiffies = timer::jiffies();
    println!("   Jiffies: {}\n", initial_jiffies);

    // Get timer statistics
    println!("3. Timer statistics:");
    let stats = timer::get_stats();
    println!("   - Initialized: {}", stats.initialized);
    println!("   - Running: {}", stats.running);
    println!("   - Jiffies: {}", stats.jiffies);
    println!("   - Tick rate: {} Hz", stats.tick_rate_hz);
    println!("   - Time quantum: {} ms", stats.time_quantum_ms);
    println!("   - Total interrupts: {}", stats.total_interrupts);
    println!("   - Total reschedules: {}", stats.total_reschedules);
    println!("   - Sleeping tasks: {}\n", stats.sleeping_tasks);

    // Spawn some tasks
    println!("4. Spawning tasks...");
    let task1 = scheduler::spawn_task(100)?;
    println!("   ✓ Spawned task {} (priority 100)", task1);

    let task2 = scheduler::spawn_task(50)?;
    println!("   ✓ Spawned task {} (priority 50)", task2);

    let task3 = scheduler::spawn_task(25)?;
    println!("   ✓ Spawned task {} (priority 25)\n", task3);

    // Start the timer (in a real system this would enable timer interrupts)
    println!("5. Starting timer...");
    let result = timer::start();
    match result {
        Ok(_) => println!("   ✓ Timer started"),
        Err(e) => println!("   ⚠ Timer start failed: {:?} (expected in test env)", e),
    }
    println!();

    // Demonstrate preemptive scheduling
    println!("6. Preemptive scheduling demonstration:");
    println!("   In a running system, the timer would trigger:");
    println!(
        "   - Periodic interrupts every {} ms",
        1000 / config.tick_rate_hz
    );
    println!(
        "   - Task switches every {} ms (time quantum)",
        config.time_quantum_ms
    );
    println!("   - Fair CPU time distribution among tasks");
    println!("   - Prevents task starvation\n");

    // Show task scheduling order
    println!("7. Task scheduling order (priority-based):");
    if let Some(task_idx) = scheduler::schedule() {
        println!("   → Task at index {} will run next", task_idx);
    }
    println!();

    // Stop the timer
    println!("8. Stopping timer...");
    timer::stop()?;
    println!("   ✓ Timer stopped\n");

    // Final statistics
    println!("9. Final statistics:");
    let final_stats = timer::get_stats();
    println!("   - Total interrupts: {}", final_stats.total_interrupts);
    println!("   - Total reschedules: {}", final_stats.total_reschedules);
    println!("   - Final jiffies: {}\n", final_stats.jiffies);

    // Clean up tasks
    println!("10. Cleaning up...");
    scheduler::terminate_task(task1);
    scheduler::terminate_task(task2);
    scheduler::terminate_task(task3);
    println!("    ✓ Tasks terminated\n");

    println!("=== Timer Configuration Options ===");
    println!("Tick rate:");
    println!("  - 100 Hz  = 10ms ticks (low overhead, coarse granularity)");
    println!("  - 250 Hz  = 4ms ticks  (balanced)");
    println!("  - 1000 Hz = 1ms ticks  (high precision, default)");
    println!("  - 10000 Hz = 100μs ticks (very high precision, high overhead)");
    println!();
    println!("Time quantum:");
    println!("  - 1ms  = Very responsive, high context switch overhead");
    println!("  - 10ms = Balanced (default)");
    println!("  - 100ms = Lower overhead, less responsive");
    println!();

    println!("=== Example completed successfully ===");

    Ok(())
}
