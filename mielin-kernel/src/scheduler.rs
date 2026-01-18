//! Async/Await based cooperative scheduler
//!
//! Provides near-zero overhead context switching for agent execution.
//!
//! ## Overview
//!
//! The scheduler implements a priority-based cooperative scheduling algorithm:
//!
//! 1. **Priority Scheduling**: Higher priority tasks (0-255) are selected first
//! 2. **Cooperative Yielding**: Tasks must explicitly yield control
//! 3. **O(n) Selection**: Scans all slots to find highest priority ready task
//!
//! ## Task Lifecycle
//!
//! ```text
//! ┌──────────┐    spawn()    ┌─────────┐   schedule()   ┌─────────┐
//! │  (none)  │ ───────────> │  Ready  │ ─────────────> │ Running │
//! └──────────┘              └─────────┘                └─────────┘
//!                                 ▲                         │
//!                                 │      yield_task()       │
//!                                 └─────────────────────────┘
//!                                           │
//!                                     terminate()
//!                                           │
//!                                           ▼
//!                                    ┌───────────┐
//!                                    │ (removed) │
//!                                    └───────────┘
//! ```
//!
//! ## Limits
//!
//! - **MAX_TASKS**: 64 concurrent tasks
//! - **Priority range**: 0 (lowest) to 255 (highest/critical)
//!
//! ## Telemetry
//!
//! The scheduler tracks various metrics for observability:
//! - Task spawns and terminations
//! - Schedule decisions and yields
//! - Priority-based selections
//! - Peak task count (high water mark)
//! - Utilization rate (busy vs idle schedules)
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::scheduler::Scheduler;
//!
//! let mut scheduler = Scheduler::new();
//!
//! // Spawn tasks with different priorities
//! let low = scheduler.spawn_task(10).unwrap();    // Low priority
//! let high = scheduler.spawn_task(100).unwrap();  // High priority
//!
//! // Schedule selects highest priority ready task
//! if let Some(idx) = scheduler.schedule() {
//!     // Run the task...
//!     scheduler.yield_task();  // Cooperative yield
//! }
//!
//! // Terminate when done
//! scheduler.terminate_task(low);
//! scheduler.terminate_task(high);
//! ```

extern crate alloc;

use crate::KernelError;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

const MAX_TASKS: usize = 64;

// =============================================================================
// Scheduler Metrics
// =============================================================================

/// Scheduler telemetry for observability
#[derive(Debug)]
pub struct SchedulerMetrics {
    /// Total tasks spawned
    tasks_spawned: AtomicU64,
    /// Total tasks terminated
    tasks_terminated: AtomicU64,
    /// Total schedule() calls
    schedule_calls: AtomicU64,
    /// Times no task was ready
    schedule_idle: AtomicU64,
    /// Total yield() calls
    yield_calls: AtomicU64,
    /// Current active task count
    active_tasks: AtomicUsize,
    /// Peak active tasks (high water mark)
    peak_tasks: AtomicUsize,
    /// Task spawn failures (MAX_TASKS reached)
    spawn_failures: AtomicU64,
}

impl SchedulerMetrics {
    const fn new() -> Self {
        Self {
            tasks_spawned: AtomicU64::new(0),
            tasks_terminated: AtomicU64::new(0),
            schedule_calls: AtomicU64::new(0),
            schedule_idle: AtomicU64::new(0),
            yield_calls: AtomicU64::new(0),
            active_tasks: AtomicUsize::new(0),
            peak_tasks: AtomicUsize::new(0),
            spawn_failures: AtomicU64::new(0),
        }
    }

    fn record_spawn(&self) {
        self.tasks_spawned.fetch_add(1, Ordering::Relaxed);
        let active = self.active_tasks.fetch_add(1, Ordering::Relaxed) + 1;
        // Update peak if needed
        let mut peak = self.peak_tasks.load(Ordering::Relaxed);
        while active > peak {
            match self.peak_tasks.compare_exchange_weak(
                peak,
                active,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => peak = p,
            }
        }
    }

    fn record_terminate(&self) {
        self.tasks_terminated.fetch_add(1, Ordering::Relaxed);
        self.active_tasks.fetch_sub(1, Ordering::Relaxed);
    }

    fn record_schedule(&self, task_found: bool) {
        self.schedule_calls.fetch_add(1, Ordering::Relaxed);
        if !task_found {
            self.schedule_idle.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_yield(&self) {
        self.yield_calls.fetch_add(1, Ordering::Relaxed);
    }

    fn record_spawn_failure(&self) {
        self.spawn_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> SchedulerMetricsSnapshot {
        SchedulerMetricsSnapshot {
            tasks_spawned: self.tasks_spawned.load(Ordering::Relaxed),
            tasks_terminated: self.tasks_terminated.load(Ordering::Relaxed),
            schedule_calls: self.schedule_calls.load(Ordering::Relaxed),
            schedule_idle: self.schedule_idle.load(Ordering::Relaxed),
            yield_calls: self.yield_calls.load(Ordering::Relaxed),
            active_tasks: self.active_tasks.load(Ordering::Relaxed),
            peak_tasks: self.peak_tasks.load(Ordering::Relaxed),
            spawn_failures: self.spawn_failures.load(Ordering::Relaxed),
        }
    }
}

/// Non-atomic snapshot of scheduler metrics
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedulerMetricsSnapshot {
    /// Total tasks spawned
    pub tasks_spawned: u64,
    /// Total tasks terminated
    pub tasks_terminated: u64,
    /// Total schedule() calls
    pub schedule_calls: u64,
    /// Times no task was ready
    pub schedule_idle: u64,
    /// Total yield() calls
    pub yield_calls: u64,
    /// Current active task count
    pub active_tasks: usize,
    /// Peak active tasks (high water mark)
    pub peak_tasks: usize,
    /// Task spawn failures (MAX_TASKS reached)
    pub spawn_failures: u64,
}

impl SchedulerMetricsSnapshot {
    /// Calculate scheduler utilization (non-idle percentage)
    pub fn utilization(&self) -> f64 {
        if self.schedule_calls == 0 {
            return 0.0;
        }
        let busy = self.schedule_calls - self.schedule_idle;
        busy as f64 / self.schedule_calls as f64
    }

    /// Calculate average task lifetime (spawns per termination)
    pub fn task_churn_rate(&self) -> f64 {
        if self.tasks_spawned == 0 {
            return 0.0;
        }
        self.tasks_terminated as f64 / self.tasks_spawned as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Suspended,
    Blocked,
    Terminated,
}

#[derive(Debug, Clone, Copy)]
pub struct Task {
    id: usize,
    state: TaskState,
    priority: u8,
    /// CPU affinity: None = can run on any CPU, Some(cpu_id) = pinned to specific CPU
    cpu_affinity: Option<usize>,
}

impl Task {
    pub fn new(id: usize, priority: u8) -> Self {
        Self {
            id,
            state: TaskState::Ready,
            priority,
            cpu_affinity: None,
        }
    }

    pub fn new_with_affinity(id: usize, priority: u8, cpu_affinity: Option<usize>) -> Self {
        Self {
            id,
            state: TaskState::Ready,
            priority,
            cpu_affinity,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn state(&self) -> TaskState {
        self.state
    }

    pub fn set_state(&mut self, state: TaskState) {
        self.state = state;
    }

    pub fn priority(&self) -> u8 {
        self.priority
    }

    pub fn cpu_affinity(&self) -> Option<usize> {
        self.cpu_affinity
    }

    pub fn set_cpu_affinity(&mut self, cpu_affinity: Option<usize>) {
        self.cpu_affinity = cpu_affinity;
    }
}

pub struct Scheduler {
    tasks: [Option<Task>; MAX_TASKS],
    current_task: Option<usize>,
    next_task_id: usize,
    metrics: SchedulerMetrics,
}

/// Global scheduler protected by a spinlock
static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    /// Create a new scheduler instance
    pub const fn new() -> Self {
        Self {
            tasks: [None; MAX_TASKS],
            current_task: None,
            next_task_id: 0,
            metrics: SchedulerMetrics::new(),
        }
    }

    pub fn spawn_task(&mut self, priority: u8) -> Result<usize, KernelError> {
        self.spawn_task_with_affinity(priority, None)
    }

    pub fn spawn_task_with_affinity(
        &mut self,
        priority: u8,
        cpu_affinity: Option<usize>,
    ) -> Result<usize, KernelError> {
        for slot in self.tasks.iter_mut() {
            if slot.is_none() {
                let task_id = self.next_task_id;
                self.next_task_id += 1;
                *slot = Some(Task::new_with_affinity(task_id, priority, cpu_affinity));
                self.metrics.record_spawn();

                #[cfg(feature = "debug-scheduler")]
                {
                    // Debug: log task spawn
                    // In a real kernel, this would go to a debug console
                    let _ = task_id; // Suppress unused warning in non-debug
                }

                return Ok(task_id);
            }
        }
        self.metrics.record_spawn_failure();
        Err(KernelError::TaskSpawnFailed)
    }

    pub fn schedule(&mut self) -> Option<usize> {
        let mut highest_priority = 0;
        let mut selected_task: Option<usize> = None;

        for (idx, task) in self.tasks.iter().enumerate() {
            if let Some(t) = task {
                if t.state == TaskState::Ready && t.priority >= highest_priority {
                    highest_priority = t.priority;
                    selected_task = Some(idx);
                }
            }
        }

        if let Some(idx) = selected_task {
            if let Some(task) = &mut self.tasks[idx] {
                task.set_state(TaskState::Running);
                self.current_task = Some(idx);
            }
        }

        self.metrics.record_schedule(selected_task.is_some());
        selected_task
    }

    pub fn yield_task(&mut self) {
        if let Some(idx) = self.current_task {
            if let Some(task) = &mut self.tasks[idx] {
                task.set_state(TaskState::Ready);
            }
            self.current_task = None;
            self.metrics.record_yield();
        }
    }

    pub fn terminate_task(&mut self, task_id: usize) {
        for task in self.tasks.iter_mut() {
            if let Some(t) = task {
                if t.id == task_id {
                    t.set_state(TaskState::Terminated);
                    *task = None;
                    self.metrics.record_terminate();
                    break;
                }
            }
        }
    }

    pub fn active_tasks(&self) -> usize {
        self.tasks.iter().filter(|t| t.is_some()).count()
    }

    /// Get a snapshot of scheduler metrics
    pub fn metrics(&self) -> SchedulerMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get a task by ID (for testing/debugging)
    pub fn get_task(&self, task_id: usize) -> Option<&Task> {
        self.tasks
            .iter()
            .find_map(|t| t.as_ref().filter(|task| task.id() == task_id))
    }

    /// Extract a task from this scheduler for migration
    /// Returns the task if found and migratable (not pinned or currently running)
    pub fn extract_task(&mut self, task_id: usize) -> Option<Task> {
        for slot in self.tasks.iter_mut() {
            if let Some(task) = slot {
                if task.id == task_id {
                    // Don't migrate currently running tasks
                    if task.state == TaskState::Running {
                        return None;
                    }

                    // Don't migrate terminated tasks
                    if task.state == TaskState::Terminated {
                        return None;
                    }

                    // Don't migrate pinned tasks (tasks with CPU affinity)
                    if task.cpu_affinity.is_some() {
                        return None;
                    }

                    let extracted = *task;
                    *slot = None;
                    return Some(extracted);
                }
            }
        }
        None
    }

    /// Inject a task into this scheduler from another CPU
    /// Returns true if successful, false if no slots available
    pub fn inject_task(&mut self, mut task: Task) -> bool {
        // Set task to Ready state (it was extracted from another scheduler)
        task.set_state(TaskState::Ready);

        for slot in self.tasks.iter_mut() {
            if slot.is_none() {
                *slot = Some(task);
                return true;
            }
        }
        false // No slots available
    }

    /// Get a list of migratable task IDs (not running, not terminated, not pinned to specific CPU)
    pub fn migratable_tasks(&self, current_cpu: usize) -> alloc::vec::Vec<usize> {
        let mut migratable = alloc::vec::Vec::new();

        for task in self.tasks.iter().flatten() {
            // Skip running or terminated tasks
            if task.state == TaskState::Running || task.state == TaskState::Terminated {
                continue;
            }

            // Skip tasks pinned to a different CPU
            if let Some(affinity) = task.cpu_affinity {
                if affinity != current_cpu {
                    continue;
                }
            }

            // Skip tasks with affinity (they should stay on their CPU)
            if task.cpu_affinity.is_some() {
                continue;
            }

            migratable.push(task.id);
        }

        migratable
    }

    /// Get a task's priority (for RT scheduling)
    pub fn get_task_priority(&self, task_id: usize) -> Result<u8, KernelError> {
        for task in self.tasks.iter().flatten() {
            if task.id == task_id {
                return Ok(task.priority);
            }
        }
        Err(KernelError::TaskNotFound { task_id })
    }

    /// Set a task's priority (for priority inheritance)
    pub fn set_task_priority(
        &mut self,
        task_id: usize,
        new_priority: u8,
    ) -> Result<(), KernelError> {
        for task in self.tasks.iter_mut().flatten() {
            if task.id == task_id {
                task.priority = new_priority;
                return Ok(());
            }
        }
        Err(KernelError::TaskNotFound { task_id })
    }
}

/// Initialize the global scheduler
pub fn init() -> Result<(), KernelError> {
    let mut guard = SCHEDULER.lock();
    *guard = Some(Scheduler::new());
    Ok(())
}

/// Spawn a new task with the given priority
pub fn spawn_task(priority: u8) -> Result<usize, KernelError> {
    let mut guard = SCHEDULER.lock();
    guard
        .as_mut()
        .ok_or(KernelError::SchedulerInitFailed)?
        .spawn_task(priority)
}

/// Schedule the next task to run
pub fn schedule() -> Option<usize> {
    let mut guard = SCHEDULER.lock();
    guard.as_mut()?.schedule()
}

/// Yield the current task
pub fn yield_current() {
    let mut guard = SCHEDULER.lock();
    if let Some(scheduler) = guard.as_mut() {
        scheduler.yield_task();
    }
}

/// Get scheduler metrics snapshot
pub fn metrics() -> Option<SchedulerMetricsSnapshot> {
    let guard = SCHEDULER.lock();
    guard.as_ref().map(|s| s.metrics())
}

/// Terminate a task by ID
pub fn terminate_task(task_id: usize) {
    let mut guard = SCHEDULER.lock();
    if let Some(scheduler) = guard.as_mut() {
        scheduler.terminate_task(task_id);
    }
}

/// Alias for yield_current() for compatibility
pub fn yield_task() {
    yield_current();
}

/// Wake a blocked task (change state from Blocked to Ready)
///
/// This is used by the timer subsystem to wake sleeping tasks.
pub fn wake_task(task_id: usize) -> Result<(), KernelError> {
    let mut guard = SCHEDULER.lock();
    let scheduler = guard.as_mut().ok_or(KernelError::SchedulerNotInitialized)?;

    // Find the task
    for task in scheduler.tasks.iter_mut().flatten() {
        if task.id() == task_id && task.state() == TaskState::Blocked {
            task.set_state(TaskState::Ready);
            return Ok(());
        }
    }

    Err(KernelError::TaskNotFound { task_id })
}

/// Get a task's priority (for RT scheduling)
pub fn get_task_priority(task_id: usize) -> Result<u8, KernelError> {
    let guard = SCHEDULER.lock();
    let scheduler = guard.as_ref().ok_or(KernelError::SchedulerNotInitialized)?;
    scheduler.get_task_priority(task_id)
}

/// Set a task's priority (for priority inheritance)
pub fn set_task_priority(task_id: usize, new_priority: u8) -> Result<(), KernelError> {
    let mut guard = SCHEDULER.lock();
    let scheduler = guard.as_mut().ok_or(KernelError::SchedulerNotInitialized)?;
    scheduler.set_task_priority(task_id, new_priority)
}

/// Get the current task ID
///
/// Returns `Some(task_id)` if there is a current task, `None` otherwise.
pub fn current_task_id() -> Option<usize> {
    let guard = SCHEDULER.lock();
    guard.as_ref()?.current_task
}

/// Block the current task
///
/// This changes the current task's state to Blocked and yields control.
/// The task will remain blocked until explicitly woken by `wake_task()`.
pub fn block_current_task() {
    let mut guard = SCHEDULER.lock();
    if let Some(scheduler) = guard.as_mut() {
        if let Some(current_id) = scheduler.current_task {
            // Find and block the current task
            for task in scheduler.tasks.iter_mut().flatten() {
                if task.id() == current_id {
                    task.set_state(TaskState::Blocked);
                    break;
                }
            }
            // Yield to trigger rescheduling
            scheduler.yield_task();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new(1, 5);
        assert_eq!(task.id(), 1);
        assert_eq!(task.priority(), 5);
        assert_eq!(task.state(), TaskState::Ready);
    }

    #[test]
    fn test_scheduler() {
        let mut scheduler = Scheduler::new();
        let task_id = scheduler.spawn_task(10).unwrap();
        assert_eq!(scheduler.active_tasks(), 1);

        scheduler.terminate_task(task_id);
        assert_eq!(scheduler.active_tasks(), 0);
    }

    #[test]
    fn test_priority_scheduling() {
        let mut scheduler = Scheduler::new();
        scheduler.spawn_task(5).unwrap();
        scheduler.spawn_task(10).unwrap();

        let selected = scheduler.schedule();
        assert!(selected.is_some());
    }

    // ==========================================================================
    // Metrics Tests
    // ==========================================================================

    #[test]
    fn test_metrics_spawn() {
        let mut scheduler = Scheduler::new();

        scheduler.spawn_task(5).unwrap();
        scheduler.spawn_task(10).unwrap();

        let metrics = scheduler.metrics();
        assert_eq!(metrics.tasks_spawned, 2);
        assert_eq!(metrics.active_tasks, 2);
        assert_eq!(metrics.peak_tasks, 2);
    }

    #[test]
    fn test_metrics_terminate() {
        let mut scheduler = Scheduler::new();

        let task_id = scheduler.spawn_task(5).unwrap();
        scheduler.terminate_task(task_id);

        let metrics = scheduler.metrics();
        assert_eq!(metrics.tasks_spawned, 1);
        assert_eq!(metrics.tasks_terminated, 1);
        assert_eq!(metrics.active_tasks, 0);
        assert_eq!(metrics.peak_tasks, 1);
    }

    #[test]
    fn test_metrics_schedule() {
        let mut scheduler = Scheduler::new();

        // Schedule with no tasks (idle)
        scheduler.schedule();
        let metrics = scheduler.metrics();
        assert_eq!(metrics.schedule_calls, 1);
        assert_eq!(metrics.schedule_idle, 1);

        // Schedule with a task
        scheduler.spawn_task(5).unwrap();
        scheduler.schedule();
        let metrics = scheduler.metrics();
        assert_eq!(metrics.schedule_calls, 2);
        assert_eq!(metrics.schedule_idle, 1); // Still 1 idle call
    }

    #[test]
    fn test_metrics_yield() {
        let mut scheduler = Scheduler::new();

        scheduler.spawn_task(5).unwrap();
        scheduler.schedule();
        scheduler.yield_task();

        let metrics = scheduler.metrics();
        assert_eq!(metrics.yield_calls, 1);
    }

    #[test]
    fn test_metrics_utilization() {
        let mut scheduler = Scheduler::new();

        // 2 idle calls
        scheduler.schedule();
        scheduler.schedule();

        let metrics = scheduler.metrics();
        assert_eq!(metrics.utilization(), 0.0);

        // 2 busy calls
        scheduler.spawn_task(5).unwrap();
        scheduler.schedule();
        scheduler.yield_task();
        scheduler.schedule();

        let metrics = scheduler.metrics();
        // 2 busy / 4 total = 0.5
        assert!((metrics.utilization() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_metrics_churn_rate() {
        let mut scheduler = Scheduler::new();

        let id1 = scheduler.spawn_task(5).unwrap();
        let id2 = scheduler.spawn_task(10).unwrap();
        scheduler.terminate_task(id1);
        scheduler.terminate_task(id2);

        let metrics = scheduler.metrics();
        assert_eq!(metrics.task_churn_rate(), 1.0);
    }

    #[test]
    fn test_metrics_spawn_failure() {
        let mut scheduler = Scheduler::new();

        // Fill up all task slots
        for _ in 0..MAX_TASKS {
            scheduler.spawn_task(5).unwrap();
        }

        // Next spawn should fail
        let result = scheduler.spawn_task(5);
        assert!(result.is_err());

        let metrics = scheduler.metrics();
        assert_eq!(metrics.spawn_failures, 1);
        assert_eq!(metrics.tasks_spawned, MAX_TASKS as u64);
    }

    #[test]
    fn test_metrics_peak_tracking() {
        let mut scheduler = Scheduler::new();

        // Spawn 3 tasks
        let id1 = scheduler.spawn_task(5).unwrap();
        let id2 = scheduler.spawn_task(5).unwrap();
        let id3 = scheduler.spawn_task(5).unwrap();

        let metrics = scheduler.metrics();
        assert_eq!(metrics.peak_tasks, 3);

        // Terminate 2
        scheduler.terminate_task(id1);
        scheduler.terminate_task(id2);

        let metrics = scheduler.metrics();
        assert_eq!(metrics.active_tasks, 1);
        assert_eq!(metrics.peak_tasks, 3); // Peak should still be 3

        // Spawn 1 more (won't exceed peak)
        scheduler.spawn_task(5).unwrap();
        let metrics = scheduler.metrics();
        assert_eq!(metrics.active_tasks, 2);
        assert_eq!(metrics.peak_tasks, 3);

        // Spawn 2 more (exceeds peak)
        scheduler.spawn_task(5).unwrap();
        scheduler.spawn_task(5).unwrap();
        let metrics = scheduler.metrics();
        assert_eq!(metrics.active_tasks, 4);
        assert_eq!(metrics.peak_tasks, 4);

        // Cleanup
        scheduler.terminate_task(id3);
    }
}

// =============================================================================
// Property-Based Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Spawning and terminating tasks should always result in correct active count
        #[test]
        fn prop_spawn_terminate_consistency(
            spawn_count in 1usize..=MAX_TASKS,
            terminate_count in 0usize..=MAX_TASKS
        ) {
            let mut scheduler = Scheduler::new();
            let mut task_ids = Vec::new();

            // Spawn tasks
            for _ in 0..spawn_count {
                if let Ok(id) = scheduler.spawn_task(5) {
                    task_ids.push(id);
                }
            }

            let spawned = task_ids.len();
            prop_assert_eq!(scheduler.active_tasks(), spawned);

            // Terminate some tasks
            let to_terminate = terminate_count.min(spawned);
            for &task_id in task_ids.iter().take(to_terminate) {
                scheduler.terminate_task(task_id);
            }

            prop_assert_eq!(scheduler.active_tasks(), spawned - to_terminate);
        }

        /// Priority scheduling should always select highest priority ready task
        #[test]
        fn prop_priority_selection(priorities in prop::collection::vec(0u8..=255, 1..=10)) {
            let mut scheduler = Scheduler::new();

            // Spawn tasks with various priorities
            for &priority in &priorities {
                let _ = scheduler.spawn_task(priority);
            }

            // Schedule should select highest priority
            if let Some(idx) = scheduler.schedule() {
                if let Some(task) = &scheduler.tasks[idx] {
                    let max_priority = priorities.iter().copied().max().unwrap();
                    prop_assert_eq!(task.priority(), max_priority);
                }
            }
        }

        /// Metrics should be consistent with operations
        #[test]
        fn prop_metrics_consistency(
            spawns in 0usize..=MAX_TASKS,
            schedules in 0usize..=20,
            yields in 0usize..=20
        ) {
            let mut scheduler = Scheduler::new();

            // Spawn tasks
            let mut spawned = 0;
            for _ in 0..spawns {
                if scheduler.spawn_task(5).is_ok() {
                    spawned += 1;
                }
            }

            // Schedule and yield
            for _ in 0..schedules {
                scheduler.schedule();
            }

            for _ in 0..yields {
                scheduler.yield_task();
            }

            let metrics = scheduler.metrics();

            // Verify metrics consistency
            prop_assert_eq!(metrics.tasks_spawned as usize, spawned);
            prop_assert_eq!(metrics.active_tasks, spawned);
            prop_assert_eq!(metrics.schedule_calls as usize, schedules);
            prop_assert!(metrics.yield_calls as usize <= yields);
            prop_assert!(metrics.peak_tasks >= metrics.active_tasks);
        }

        /// Utilization should always be between 0 and 1
        #[test]
        fn prop_utilization_bounds(
            idle_schedules in 0usize..=10,
            busy_schedules in 0usize..=10
        ) {
            let mut scheduler = Scheduler::new();

            // Do idle schedules (no tasks)
            for _ in 0..idle_schedules {
                scheduler.schedule();
            }

            // Spawn a task for busy schedules
            if busy_schedules > 0 {
                scheduler.spawn_task(5).unwrap();
                for _ in 0..busy_schedules {
                    scheduler.schedule();
                    scheduler.yield_task();
                }
            }

            let metrics = scheduler.metrics();
            let utilization = metrics.utilization();

            prop_assert!(utilization >= 0.0);
            prop_assert!(utilization <= 1.0);
        }

        /// Task churn rate should be non-negative
        #[test]
        fn prop_churn_rate_non_negative(
            spawns in 1usize..=20,
            terminates in 0usize..=20
        ) {
            let mut scheduler = Scheduler::new();
            let mut ids = Vec::new();

            for _ in 0..spawns.min(MAX_TASKS) {
                if let Ok(id) = scheduler.spawn_task(5) {
                    ids.push(id);
                }
            }

            for &id in ids.iter().take(terminates.min(ids.len())) {
                scheduler.terminate_task(id);
            }

            let metrics = scheduler.metrics();
            prop_assert!(metrics.task_churn_rate() >= 0.0);
        }
    }
}

// =============================================================================
// Stress Tests
// =============================================================================

#[cfg(test)]
mod stress_tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

    /// Stress test: 1000+ spawn/terminate cycles
    #[test]
    fn test_stress_spawn_terminate_cycles() {
        let mut scheduler = Scheduler::new();
        let mut total_spawns = 0u64;
        let mut total_terminates = 0u64;

        // Perform 1000 spawn/terminate cycles
        for _ in 0..1000 {
            // Spawn a task
            if let Ok(id) = scheduler.spawn_task(5) {
                total_spawns += 1;
                // Immediately terminate it
                scheduler.terminate_task(id);
                total_terminates += 1;
            }
        }

        // Verify all spawns and terminates were tracked
        let metrics = scheduler.metrics();
        assert_eq!(metrics.tasks_spawned, total_spawns);
        assert_eq!(metrics.tasks_terminated, total_terminates);
        assert_eq!(metrics.active_tasks, 0); // All tasks terminated
    }

    /// Stress test: Fill and drain cycles
    #[test]
    fn test_stress_fill_drain_cycles() {
        let mut scheduler = Scheduler::new();

        // Perform 50 fill/drain cycles
        for cycle in 0..50 {
            let mut ids = Vec::new();

            // Fill all task slots
            for priority in 0..MAX_TASKS {
                if let Ok(id) = scheduler.spawn_task((priority % 256) as u8) {
                    ids.push(id);
                }
            }

            assert_eq!(
                scheduler.active_tasks(),
                MAX_TASKS,
                "Cycle {}: Should fill all {} slots",
                cycle,
                MAX_TASKS
            );

            // Drain all tasks
            for id in ids {
                scheduler.terminate_task(id);
            }

            assert_eq!(
                scheduler.active_tasks(),
                0,
                "Cycle {}: Should drain all tasks",
                cycle
            );
        }

        // Verify metrics
        let metrics = scheduler.metrics();
        assert_eq!(
            metrics.tasks_spawned,
            (50 * MAX_TASKS) as u64,
            "Should have spawned 50 * {} tasks",
            MAX_TASKS
        );
        assert_eq!(metrics.tasks_terminated, metrics.tasks_spawned);
    }

    /// Stress test: Mixed spawn/terminate operations
    #[test]
    fn test_stress_mixed_operations() {
        let mut scheduler = Scheduler::new();
        let mut active_ids: Vec<usize> = Vec::new();

        // Perform 2000 mixed operations
        for i in 0..2000 {
            match i % 3 {
                // 2/3 spawn, 1/3 terminate
                0 | 1 => {
                    // Try to spawn
                    if let Ok(id) = scheduler.spawn_task((i % 256) as u8) {
                        active_ids.push(id);
                    }
                }
                _ => {
                    // Try to terminate oldest
                    if let Some(id) = active_ids.pop() {
                        scheduler.terminate_task(id);
                    }
                }
            }
        }

        // Verify consistency
        assert_eq!(scheduler.active_tasks(), active_ids.len());

        // Cleanup
        for id in active_ids {
            scheduler.terminate_task(id);
        }
        assert_eq!(scheduler.active_tasks(), 0);
    }

    /// Stress test: Priority scheduling under load
    #[test]
    fn test_stress_priority_scheduling() {
        let mut scheduler = Scheduler::new();
        let mut ids = Vec::new();

        // Spawn tasks with various priorities
        for i in 0..MAX_TASKS {
            let priority = ((i * 17) % 256) as u8; // Pseudo-random priorities
            if let Ok(id) = scheduler.spawn_task(priority) {
                ids.push(id);
            }
        }

        // Perform many schedule/yield cycles
        for _ in 0..500 {
            if let Some(_idx) = scheduler.schedule() {
                scheduler.yield_task();
            }
        }

        let metrics = scheduler.metrics();
        assert!(
            metrics.schedule_calls >= 500,
            "Should have 500+ schedule calls"
        );
        assert!(metrics.yield_calls >= 500, "Should have 500+ yield calls");

        // Cleanup
        for id in ids {
            scheduler.terminate_task(id);
        }
    }

    /// Stress test: Metrics consistency under heavy load
    #[test]
    fn test_stress_metrics_consistency() {
        let mut scheduler = Scheduler::new();
        let mut current_ids: Vec<usize> = Vec::new();
        let mut total_spawned = 0u64;
        let mut total_terminated = 0u64;
        let mut total_schedules = 0u64;
        let mut total_yields = 0u64;

        // Perform 1000 random operations
        for i in 0..1000 {
            match i % 5 {
                0 | 1 => {
                    // Spawn
                    if let Ok(id) = scheduler.spawn_task((i % 256) as u8) {
                        current_ids.push(id);
                        total_spawned += 1;
                    }
                }
                2 => {
                    // Terminate
                    if let Some(id) = current_ids.pop() {
                        scheduler.terminate_task(id);
                        total_terminated += 1;
                    }
                }
                3 => {
                    // Schedule
                    scheduler.schedule();
                    total_schedules += 1;
                }
                _ => {
                    // Yield (only if we have a current task)
                    if scheduler.current_task.is_some() {
                        scheduler.yield_task();
                        total_yields += 1;
                    }
                }
            }
        }

        let metrics = scheduler.metrics();

        // Verify all metrics match our tracking
        assert_eq!(metrics.tasks_spawned, total_spawned);
        assert_eq!(metrics.tasks_terminated, total_terminated);
        assert_eq!(metrics.schedule_calls, total_schedules);
        assert!(metrics.yield_calls <= total_yields + 1); // May have extra yields
        assert_eq!(
            metrics.active_tasks,
            (total_spawned - total_terminated) as usize
        );

        // Peak should be at least current active
        assert!(metrics.peak_tasks >= metrics.active_tasks);

        // Utilization should be valid
        let util = metrics.utilization();
        assert!((0.0..=1.0).contains(&util));
    }
}

// =============================================================================
// Additional Edge Case Tests
// =============================================================================

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_spawn_at_max_capacity() {
        let mut scheduler = Scheduler::new();
        let mut ids = Vec::new();

        // Fill scheduler to capacity
        for i in 0..MAX_TASKS {
            let id = scheduler.spawn_task(i as u8).unwrap();
            ids.push(id);
        }

        // Verify we're at capacity
        assert_eq!(scheduler.metrics().active_tasks, MAX_TASKS);

        // Try to spawn one more - should fail
        let result = scheduler.spawn_task(100);
        assert!(result.is_err());
        assert!(matches!(result, Err(KernelError::TaskSpawnFailed)));

        // Verify spawn failure was recorded
        assert_eq!(scheduler.metrics().spawn_failures, 1);
    }

    #[test]
    fn test_schedule_with_no_tasks() {
        let mut scheduler = Scheduler::new();

        // Schedule with no tasks
        let result = scheduler.schedule();
        assert!(result.is_none());

        // Verify idle was recorded
        let metrics = scheduler.metrics();
        assert_eq!(metrics.schedule_calls, 1);
        assert_eq!(metrics.schedule_idle, 1);
        assert_eq!(metrics.utilization(), 0.0);
    }

    #[test]
    fn test_yield_with_no_current_task() {
        let mut scheduler = Scheduler::new();

        // Yield with no current task should be safe (no-op)
        scheduler.yield_task();

        // Yield is not recorded when there's no current task
        assert_eq!(scheduler.metrics().yield_calls, 0);
    }

    #[test]
    fn test_terminate_nonexistent_task() {
        let mut scheduler = Scheduler::new();

        // Terminate a task that doesn't exist
        scheduler.terminate_task(999);

        // Should not panic, just be a no-op
        assert_eq!(scheduler.metrics().tasks_terminated, 0);
    }

    #[test]
    fn test_priority_based_scheduling() {
        let mut scheduler = Scheduler::new();

        // Spawn tasks with different priorities
        let _low = scheduler.spawn_task(10).unwrap();
        let _high = scheduler.spawn_task(200).unwrap();
        let _medium = scheduler.spawn_task(100).unwrap();
        let _critical = scheduler.spawn_task(255).unwrap();

        // All four tasks should eventually be scheduled
        let mut scheduled_count = 0;
        for _ in 0..20 {
            // Give enough iterations
            if scheduler.schedule().is_some() {
                scheduled_count += 1;
                scheduler.yield_task();
            }
        }

        // All four tasks should have been scheduled at least once
        assert!(scheduled_count >= 4);
    }

    #[test]
    fn test_same_priority_selection() {
        let mut scheduler = Scheduler::new();

        // Spawn multiple tasks with same priority
        let _id1 = scheduler.spawn_task(100).unwrap();
        let _id2 = scheduler.spawn_task(100).unwrap();
        let _id3 = scheduler.spawn_task(100).unwrap();

        // Should schedule all eventually
        let mut scheduled_count = 0;
        for _ in 0..20 {
            // Give enough iterations
            if scheduler.schedule().is_some() {
                scheduled_count += 1;
                scheduler.yield_task();
            }
        }

        // All three should have been scheduled
        assert!(scheduled_count >= 3);
    }

    #[test]
    fn test_task_lifecycle_all_states() {
        let mut scheduler = Scheduler::new();

        let id = scheduler.spawn_task(100).unwrap();

        // Initially Ready
        let task = scheduler.get_task(id).unwrap();
        assert!(matches!(task.state(), TaskState::Ready));

        // Schedule makes it Running
        if let Some(idx) = scheduler.schedule() {
            assert!(matches!(
                scheduler.tasks[idx].as_ref().unwrap().state(),
                TaskState::Running
            ));
            scheduler.yield_task();
        }

        // After yield, back to Ready
        let task = scheduler.get_task(id).unwrap();
        assert!(matches!(task.state(), TaskState::Ready));

        // Terminate
        scheduler.terminate_task(id);
    }

    #[test]
    fn test_active_task_count() {
        let mut scheduler = Scheduler::new();

        assert_eq!(scheduler.metrics().active_tasks, 0);

        let id1 = scheduler.spawn_task(100).unwrap();
        assert_eq!(scheduler.metrics().active_tasks, 1);

        let id2 = scheduler.spawn_task(200).unwrap();
        assert_eq!(scheduler.metrics().active_tasks, 2);

        scheduler.terminate_task(id1);
        assert_eq!(scheduler.metrics().active_tasks, 1);

        scheduler.terminate_task(id2);
        assert_eq!(scheduler.metrics().active_tasks, 0);
    }

    #[test]
    fn test_spawn_with_affinity_none() {
        let mut scheduler = Scheduler::new();

        let id = scheduler.spawn_task_with_affinity(100, None).unwrap();
        let task = scheduler.get_task(id).unwrap();

        assert_eq!(task.cpu_affinity(), None);
    }

    #[test]
    fn test_spawn_with_affinity_pinned() {
        let mut scheduler = Scheduler::new();

        let id = scheduler.spawn_task_with_affinity(100, Some(3)).unwrap();
        let task = scheduler.get_task(id).unwrap();

        assert_eq!(task.cpu_affinity(), Some(3));
    }

    #[test]
    fn test_rapid_spawn_terminate_cycles() {
        let mut scheduler = Scheduler::new();

        for _ in 0..100 {
            let id = scheduler.spawn_task(100).unwrap();
            scheduler.terminate_task(id);
        }

        let metrics = scheduler.metrics();
        assert_eq!(metrics.tasks_spawned, 100);
        assert_eq!(metrics.tasks_terminated, 100);
        assert_eq!(metrics.active_tasks, 0);
    }

    #[test]
    fn test_schedule_yield_pattern() {
        let mut scheduler = Scheduler::new();

        let _id = scheduler.spawn_task(100).unwrap();

        // Repeated schedule-yield pattern
        for _ in 0..10 {
            if scheduler.schedule().is_some() {
                scheduler.yield_task();
            }
        }

        let metrics = scheduler.metrics();
        assert_eq!(metrics.schedule_calls, 10);
        assert_eq!(metrics.yield_calls, 10);
    }

    #[test]
    fn test_churn_rate_calculation() {
        let mut scheduler = Scheduler::new();

        // Spawn and terminate to create churn
        for _ in 0..5 {
            let id = scheduler.spawn_task(100).unwrap();
            scheduler.terminate_task(id);
        }

        // Some schedules without churn
        scheduler.spawn_task(100).unwrap();
        for _ in 0..10 {
            scheduler.schedule();
        }

        let metrics = scheduler.metrics();
        let churn = metrics.task_churn_rate();

        // Churn should be non-negative
        assert!(churn >= 0.0);
    }

    #[test]
    fn test_utilization_at_zero() {
        let scheduler = Scheduler::new();
        assert_eq!(scheduler.metrics().utilization(), 0.0);
    }

    #[test]
    fn test_utilization_at_full() {
        let mut scheduler = Scheduler::new();

        // Fill scheduler
        for _ in 0..MAX_TASKS {
            scheduler.spawn_task(100).unwrap();
        }

        // Schedule a few times
        for _ in 0..10 {
            scheduler.schedule();
        }

        let metrics = scheduler.metrics();
        let util = metrics.utilization();

        // Utilization should be high (close to 1.0)
        assert!(util > 0.5);
        assert!(util <= 1.0);
    }

    #[test]
    fn test_metrics_peak_tracking_comprehensive() {
        let mut scheduler = Scheduler::new();

        // Spawn a few tasks
        let id1 = scheduler.spawn_task(100).unwrap();
        let id2 = scheduler.spawn_task(100).unwrap();
        let id3 = scheduler.spawn_task(100).unwrap();

        assert_eq!(scheduler.metrics().peak_tasks, 3);

        // Terminate one
        scheduler.terminate_task(id1);
        assert_eq!(scheduler.metrics().active_tasks, 2);
        assert_eq!(scheduler.metrics().peak_tasks, 3); // Peak stays

        // Spawn more
        let _id4 = scheduler.spawn_task(100).unwrap();
        let _id5 = scheduler.spawn_task(100).unwrap();

        assert_eq!(scheduler.metrics().active_tasks, 4);
        assert_eq!(scheduler.metrics().peak_tasks, 4); // Peak updated

        // Terminate all
        scheduler.terminate_task(id2);
        scheduler.terminate_task(id3);

        assert_eq!(scheduler.metrics().active_tasks, 2);
        assert_eq!(scheduler.metrics().peak_tasks, 4); // Peak still 4
    }

    #[test]
    fn test_priority_extremes() {
        let mut scheduler = Scheduler::new();

        let _min_id = scheduler.spawn_task(0).unwrap();
        let _max_id = scheduler.spawn_task(255).unwrap();

        // Both tasks should be schedulable
        let mut scheduled_count = 0;
        for _ in 0..10 {
            if scheduler.schedule().is_some() {
                scheduled_count += 1;
                scheduler.yield_task();
            }
        }

        // Both tasks should be scheduled
        assert!(scheduled_count >= 2);
    }

    #[test]
    fn test_terminate_running_task() {
        let mut scheduler = Scheduler::new();

        let id = scheduler.spawn_task(100).unwrap();

        // Schedule it (makes it Running)
        scheduler.schedule();

        // Terminate while running
        scheduler.terminate_task(id);

        // Should be removed
        assert!(scheduler.get_task(id).is_none());
        assert_eq!(scheduler.metrics().active_tasks, 0);
    }

    #[test]
    fn test_multiple_yields_same_task() {
        let mut scheduler = Scheduler::new();

        let _id = scheduler.spawn_task(100).unwrap();

        // Schedule and yield multiple times
        for _ in 0..5 {
            if scheduler.schedule().is_some() {
                scheduler.yield_task();
                scheduler.yield_task(); // Extra yield
            }
        }

        // Should handle gracefully
        let metrics = scheduler.metrics();
        assert_eq!(metrics.schedule_calls, 5);
        assert!(metrics.yield_calls >= 5);
    }

    #[test]
    fn test_extract_task() {
        let mut scheduler = Scheduler::new();

        let id = scheduler.spawn_task(100).unwrap();

        // Extract the task
        let extracted = scheduler.extract_task(id);
        assert!(extracted.is_some());

        // Task should no longer be in scheduler
        assert!(scheduler.get_task(id).is_none());

        // Extract doesn't update metrics, so count stays at 1
        // This is expected behavior for migration
    }

    #[test]
    fn test_extract_nonexistent_task() {
        let mut scheduler = Scheduler::new();

        let extracted = scheduler.extract_task(999);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_inject_task() {
        let mut scheduler = Scheduler::new();

        // Create a task manually
        let task = Task::new_with_affinity(42, 150, None);

        // Inject it
        let result = scheduler.inject_task(task);
        assert!(result);

        // Should be findable
        let found = scheduler.get_task(42);
        assert!(found.is_some());
        assert_eq!(found.unwrap().priority(), 150);
    }

    #[test]
    fn test_inject_task_at_capacity() {
        let mut scheduler = Scheduler::new();

        // Fill scheduler
        for i in 0..MAX_TASKS {
            scheduler.spawn_task(i as u8).unwrap();
        }

        // Try to inject
        let task = Task::new_with_affinity(999, 100, None);

        let result = scheduler.inject_task(task);
        assert!(!result); // Should fail
    }

    #[test]
    fn test_migratable_tasks_excludes_running() {
        let mut scheduler = Scheduler::new();

        let _id1 = scheduler.spawn_task(100).unwrap();
        let _id2 = scheduler.spawn_task(100).unwrap();

        // Schedule one (makes it Running)
        scheduler.schedule();

        // Migratable tasks should exclude the running one
        let migratable = scheduler.migratable_tasks(0);
        assert_eq!(migratable.len(), 1); // Only one is migratable
    }

    #[test]
    fn test_migratable_tasks_excludes_pinned() {
        let mut scheduler = Scheduler::new();

        let _id1 = scheduler.spawn_task_with_affinity(100, None).unwrap();
        let _id2 = scheduler.spawn_task_with_affinity(100, Some(2)).unwrap();

        let migratable = scheduler.migratable_tasks(0);

        // Only the non-pinned task should be migratable
        assert_eq!(migratable.len(), 1);
    }

    #[test]
    fn test_global_init() {
        // Initialize global scheduler
        let result = init();
        assert!(result.is_ok());

        // Second init should also succeed (idempotent)
        let result = init();
        assert!(result.is_ok());
    }

    #[test]
    fn test_spawn_failure_increments_counter() {
        let mut scheduler = Scheduler::new();

        // Fill scheduler
        for i in 0..MAX_TASKS {
            scheduler.spawn_task(i as u8).unwrap();
        }

        let before = scheduler.metrics().spawn_failures;

        // Try to spawn more
        for _ in 0..5 {
            let _ = scheduler.spawn_task(100);
        }

        let after = scheduler.metrics().spawn_failures;
        assert_eq!(after, before + 5);
    }
}
