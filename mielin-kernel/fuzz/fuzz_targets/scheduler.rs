#![no_main]

use libfuzzer_sys::fuzz_target;
use mielin_kernel::scheduler::Scheduler;

// Fuzz operations for scheduler
#[derive(Debug)]
enum SchedOp {
    SpawnTask { priority: u8 },
    Schedule,
    Yield,
    Terminate { task_id: usize },
}

impl SchedOp {
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        match data[0] % 4 {
            0 => {
                // Spawn task
                if data.len() < 2 {
                    return None;
                }
                Some(SchedOp::SpawnTask {
                    priority: data[1],
                })
            }
            1 => Some(SchedOp::Schedule),
            2 => Some(SchedOp::Yield),
            3 => {
                // Terminate
                if data.len() < 2 {
                    return None;
                }
                Some(SchedOp::Terminate {
                    task_id: data[1] as usize,
                })
            }
            _ => None,
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // Skip too small inputs
    if data.len() < 2 {
        return;
    }

    let mut scheduler = Scheduler::new();

    // Track spawned task IDs
    let mut task_ids = Vec::new();

    // Parse and execute operations
    let mut i = 0;
    while i < data.len() {
        let remaining = &data[i..];
        if let Some(op) = SchedOp::from_bytes(remaining) {
            match op {
                SchedOp::SpawnTask { priority } => {
                    if let Ok(task_id) = scheduler.spawn_task(priority) {
                        task_ids.push(task_id);
                    }
                }
                SchedOp::Schedule => {
                    let _ = scheduler.schedule();
                }
                SchedOp::Yield => {
                    scheduler.yield_current();
                }
                SchedOp::Terminate { task_id: _ } => {
                    // Terminate a random task
                    if !task_ids.is_empty() {
                        let idx = data.get(i + 1).copied().unwrap_or(0) as usize % task_ids.len();
                        let task_id = task_ids.remove(idx);
                        scheduler.terminate_task(task_id);
                    }
                }
            }
            i += 2; // Move to next operation
        } else {
            i += 1;
        }
    }

    // Verify scheduler consistency
    let metrics = scheduler.metrics();

    // Sanity checks
    assert!(metrics.tasks_spawned >= metrics.tasks_terminated);
    assert!(metrics.active_tasks <= 64); // MAX_TASKS
    assert!(metrics.peak_tasks <= 64);
    assert!(metrics.schedules >= 0);
    assert!(metrics.yields >= 0);

    if metrics.tasks_spawned > 0 {
        assert!(metrics.success_rate >= 0.0 && metrics.success_rate <= 100.0);
    }

    // Verify utilization is within bounds
    let utilization = metrics.utilization();
    assert!(utilization >= 0.0 && utilization <= 100.0);

    // Clean up remaining tasks
    for task_id in task_ids {
        scheduler.terminate_task(task_id);
    }
});
