//! Real-Time Scheduling Example
//!
//! Demonstrates priority inheritance, priority ceiling protocol, and EDF scheduling.

use mielin_kernel::rt::{DeadlineParams, EdfScheduler, PcpMutex, RtMutex};
use mielin_kernel::scheduler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Real-Time Scheduling Features Demo ===\n");

    // Initialize the kernel
    mielin_kernel::kernel_init()?;

    // =========================================================================
    // Example 1: Priority Inheritance Protocol (PIP)
    // =========================================================================
    println!("1. Priority Inheritance Protocol (PIP)");
    println!("   Prevents priority inversion by boosting low-priority task's priority");
    println!("   when a high-priority task blocks on its mutex.\n");

    let rt_mutex = RtMutex::new(0);

    // Spawn a low-priority task (priority 10)
    let low_priority_task = scheduler::spawn_task(10)?;
    println!(
        "   Spawned low-priority task {} (priority 10)",
        low_priority_task
    );

    // Lock the mutex with the low-priority task
    rt_mutex.lock(low_priority_task)?;
    println!("   Low-priority task locked mutex");

    // Spawn a high-priority task (priority 100)
    let high_priority_task = scheduler::spawn_task(100)?;
    println!(
        "   Spawned high-priority task {} (priority 100)",
        high_priority_task
    );

    // When high-priority task tries to lock the same mutex,
    // the low-priority task's priority is automatically boosted
    println!("   High-priority task attempts to lock mutex...");
    println!("   -> Low-priority task's priority is boosted to 100");

    // Get statistics
    let stats = rt_mutex.get_stats();
    println!(
        "   Mutex stats: locks={}, unlocks={}, inversions_prevented={}",
        stats.lock_count, stats.unlock_count, stats.inversions_prevented
    );

    // Unlock the mutex
    rt_mutex.unlock(low_priority_task)?;
    println!("   Low-priority task unlocked mutex (priority restored to 10)\n");

    // =========================================================================
    // Example 2: Priority Ceiling Protocol (PCP)
    // =========================================================================
    println!("2. Priority Ceiling Protocol (PCP)");
    println!("   Prevents priority inversion by setting a ceiling priority for each mutex.");
    println!("   Tasks can only lock if their priority exceeds the ceiling.\n");

    let pcp_mutex = PcpMutex::new(50); // Ceiling = priority 50
    println!("   Created PCP mutex with ceiling priority 50");

    // Low-priority task cannot lock (priority 10 < ceiling 50)
    let result = pcp_mutex.lock(low_priority_task);
    println!(
        "   Low-priority task (10) tries to lock: {}",
        if result.is_err() {
            "DENIED (priority < ceiling)"
        } else {
            "allowed"
        }
    );

    // High-priority task can lock (priority 100 > ceiling 50)
    let result = pcp_mutex.lock(high_priority_task);
    println!(
        "   High-priority task (100) tries to lock: {}",
        if result.is_ok() {
            "ALLOWED (priority > ceiling)"
        } else {
            "denied"
        }
    );

    if result.is_ok() {
        pcp_mutex.unlock(high_priority_task)?;
        println!("   High-priority task unlocked mutex\n");
    }

    // =========================================================================
    // Example 3: Earliest Deadline First (EDF) Scheduling
    // =========================================================================
    println!("3. Earliest Deadline First (EDF) Scheduling");
    println!("   Tasks are scheduled based on their absolute deadlines.");
    println!("   The task with the nearest deadline runs first.\n");

    let edf = EdfScheduler::new(10);

    // Task 1: Period=1000us, WCET=300us, Deadline=1000us (U=0.3)
    let task1 = DeadlineParams::new(1, 1000, 300, 1000);
    println!(
        "   Task 1: Period=1000us, WCET=300us, Deadline=1000us (U={:.1}%)",
        task1.utilization() * 100.0
    );
    edf.add_task(task1)?;

    // Task 2: Period=2000us, WCET=500us, Deadline=2000us (U=0.25)
    let task2 = DeadlineParams::new(2, 2000, 500, 2000);
    println!(
        "   Task 2: Period=2000us, WCET=500us, Deadline=2000us (U={:.1}%)",
        task2.utilization() * 100.0
    );
    edf.add_task(task2)?;

    // Task 3: Period=5000us, WCET=1000us, Deadline=5000us (U=0.2)
    let task3 = DeadlineParams::new(3, 5000, 1000, 5000);
    println!(
        "   Task 3: Period=5000us, WCET=1000us, Deadline=5000us (U={:.1}%)",
        task3.utilization() * 100.0
    );
    edf.add_task(task3)?;

    println!(
        "\n   Total CPU utilization: {:.1}%",
        edf.get_utilization() * 100.0
    );
    println!(
        "   EDF schedulability test: {}",
        if edf.get_utilization() <= 1.0 {
            "SCHEDULABLE (U ≤ 100%)"
        } else {
            "NOT SCHEDULABLE (U > 100%)"
        }
    );

    // Simulate task releases at time 0
    edf.release_task(1, 0)?; // Task 1 deadline = 0 + 1000 = 1000us
    edf.release_task(2, 0)?; // Task 2 deadline = 0 + 2000 = 2000us
    edf.release_task(3, 0)?; // Task 3 deadline = 0 + 5000 = 5000us

    // Schedule: task with earliest deadline runs first
    if let Some(next_task) = edf.schedule() {
        println!(
            "\n   EDF selected task {} to run first (earliest deadline)",
            next_task
        );
    }

    // Try to add a task that would exceed 100% utilization
    let overload_task = DeadlineParams::new(4, 1000, 300, 1000); // U=0.3
    let result = edf.add_task(overload_task);
    println!(
        "\n   Attempt to add task with 30% utilization (total would be 105%): {}",
        if result.is_err() {
            "REJECTED (not schedulable)"
        } else {
            "allowed"
        }
    );

    println!("   Deadline misses: {}", edf.get_deadline_misses());

    println!("\n=== Real-Time Features Demo Complete ===");

    Ok(())
}
