//! Task Scheduler Example
//!
//! Demonstrates the kernel's priority-based cooperative scheduler
//! with telemetry and metrics tracking.

use mielin_kernel::scheduler::Scheduler;

fn main() {
    println!("=== MielinOS Task Scheduler Example ===\n");

    let mut scheduler = Scheduler::new();

    // --- Spawn Tasks with Different Priorities ---
    println!("1. Spawning Tasks:");

    let low_priority = scheduler
        .spawn_task(10)
        .expect("Failed to spawn low priority task");
    println!("   Spawned task {} with priority 10 (low)", low_priority);

    let medium_priority = scheduler
        .spawn_task(50)
        .expect("Failed to spawn medium priority task");
    println!(
        "   Spawned task {} with priority 50 (medium)",
        medium_priority
    );

    let high_priority = scheduler
        .spawn_task(100)
        .expect("Failed to spawn high priority task");
    println!("   Spawned task {} with priority 100 (high)", high_priority);

    let critical_priority = scheduler
        .spawn_task(255)
        .expect("Failed to spawn critical priority task");
    println!(
        "   Spawned task {} with priority 255 (critical)",
        critical_priority
    );
    println!();

    // --- Active Tasks ---
    println!("2. Task Status:");
    println!("   Active tasks: {}", scheduler.active_tasks());
    println!();

    // --- Priority-Based Scheduling ---
    println!("3. Priority-Based Scheduling:");
    println!("   (Higher priority tasks are selected first)\n");

    for i in 1..=4 {
        if let Some(task_idx) = scheduler.schedule() {
            println!("   Schedule #{}: Selected task index {}", i, task_idx);
            scheduler.yield_task();
        }
    }
    println!();

    // --- Scheduler Metrics ---
    println!("4. Scheduler Metrics:");
    let metrics = scheduler.metrics();
    println!("   Tasks spawned: {}", metrics.tasks_spawned);
    println!("   Schedule calls: {}", metrics.schedule_calls);
    println!("   Yield calls: {}", metrics.yield_calls);
    println!("   Active tasks: {}", metrics.active_tasks);
    println!("   Peak tasks: {}", metrics.peak_tasks);
    println!("   Utilization: {:.1}%", metrics.utilization() * 100.0);
    println!();

    // --- Terminate Tasks ---
    println!("5. Terminating Tasks:");
    for (name, id) in [
        ("low", low_priority),
        ("medium", medium_priority),
        ("high", high_priority),
        ("critical", critical_priority),
    ] {
        scheduler.terminate_task(id);
        println!("   Terminated {} priority task {}", name, id);
    }
    println!();

    // --- Final Metrics ---
    println!("6. Final Metrics:");
    let metrics = scheduler.metrics();
    println!("   Tasks spawned: {}", metrics.tasks_spawned);
    println!("   Tasks terminated: {}", metrics.tasks_terminated);
    println!("   Active tasks: {}", metrics.active_tasks);
    println!("   Task churn rate: {:.2}", metrics.task_churn_rate());
    println!();

    println!("=== Example Complete ===");
}
