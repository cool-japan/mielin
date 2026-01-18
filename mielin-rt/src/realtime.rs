//! Real-Time Scheduling Features
//!
//! Provides advanced real-time scheduling capabilities including:
//! - Priority inheritance for preventing priority inversion
//! - Deadline-based scheduling (EDF - Earliest Deadline First)
//! - Bounded execution time tracking
//! - Real-time task abstractions
//!
//! ## Overview
//!
//! The real-time module extends the basic scheduler with features required
//! for deterministic, time-critical embedded applications.
//!
//! ## Task Types
//!
//! - **Periodic**: Fixed period and deadline (e.g., sensor sampling)
//! - **Sporadic**: Minimum inter-arrival time with deadline
//! - **Aperiodic**: No timing constraints (best-effort)
//!
//! ## Scheduling Policies
//!
//! - **Rate Monotonic (RM)**: Fixed priority based on period (shorter period = higher priority)
//! - **Earliest Deadline First (EDF)**: Dynamic priority based on absolute deadline
//! - **Priority Inheritance**: Prevents priority inversion by temporarily elevating priority
//!
//! ## Example
//!
//! ```rust,no_run
//! use mielin_rt::realtime::{RealTimeTask, TaskType, SchedulingPolicy};
//!
//! // Create a periodic task with 10ms period
//! let task = RealTimeTask::new(0, TaskType::Periodic {
//!     period_us: 10_000,
//!     deadline_us: 10_000,
//! });
//!
//! // Configure for EDF scheduling
//! let policy = SchedulingPolicy::EarliestDeadlineFirst;
//! ```

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

// =============================================================================
// Real-Time Task Types
// =============================================================================

/// Type of real-time task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    /// Periodic task with fixed period and deadline
    /// period_us: Period in microseconds
    /// deadline_us: Relative deadline in microseconds
    Periodic { period_us: u64, deadline_us: u64 },

    /// Sporadic task with minimum inter-arrival time
    /// min_interarrival_us: Minimum time between task activations
    /// deadline_us: Relative deadline in microseconds
    Sporadic {
        min_interarrival_us: u64,
        deadline_us: u64,
    },

    /// Aperiodic task with no timing constraints
    Aperiodic,
}

/// Real-time scheduling policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingPolicy {
    /// Fixed priority scheduling (priority assigned at creation)
    FixedPriority,

    /// Rate Monotonic scheduling (priority based on period)
    /// Shorter period = higher priority
    RateMonotonic,

    /// Earliest Deadline First (dynamic priority)
    /// Task with earliest absolute deadline has highest priority
    EarliestDeadlineFirst,

    /// Least Laxity First (dynamic priority)
    /// Laxity = deadline - current_time - remaining_execution_time
    LeastLaxityFirst,
}

/// Real-time task state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtTaskState {
    /// Task is ready to run
    Ready,
    /// Task is currently executing
    Running,
    /// Task is waiting for a resource
    Blocked,
    /// Task is waiting for next period
    Waiting,
    /// Task has completed its current instance
    Completed,
}

/// Real-time task descriptor
#[derive(Debug, Clone, Copy)]
pub struct RealTimeTask {
    /// Unique task identifier
    id: usize,

    /// Task type (periodic, sporadic, aperiodic)
    task_type: TaskType,

    /// Base priority (before inheritance)
    base_priority: u8,

    /// Current effective priority (may be elevated due to inheritance)
    effective_priority: u8,

    /// Current task state
    state: RtTaskState,

    /// Absolute deadline for current instance (microseconds since boot)
    absolute_deadline_us: u64,

    /// Worst-case execution time in microseconds
    wcet_us: u64,

    /// Actual execution time for current instance (microseconds)
    execution_time_us: u64,

    /// Last activation time (microseconds since boot)
    last_activation_us: u64,

    /// Number of deadline misses
    deadline_misses: u64,

    /// Number of completed instances
    completed_instances: u64,
}

impl RealTimeTask {
    /// Create a new real-time task
    pub fn new(id: usize, task_type: TaskType) -> Self {
        let base_priority = match task_type {
            TaskType::Periodic { period_us, .. } => {
                // Rate Monotonic: shorter period = higher priority
                // Map period to priority (0-255)
                if period_us < 1_000 {
                    255 // < 1ms: highest priority
                } else if period_us < 10_000 {
                    200 // < 10ms: very high priority
                } else if period_us < 100_000 {
                    150 // < 100ms: high priority
                } else if period_us < 1_000_000 {
                    100 // < 1s: medium priority
                } else {
                    50 // >= 1s: low priority
                }
            }
            TaskType::Sporadic {
                min_interarrival_us,
                ..
            } => {
                // Similar to periodic
                if min_interarrival_us < 1_000 {
                    200
                } else if min_interarrival_us < 10_000 {
                    150
                } else {
                    100
                }
            }
            TaskType::Aperiodic => 0, // Lowest priority
        };

        Self {
            id,
            task_type,
            base_priority,
            effective_priority: base_priority,
            state: RtTaskState::Ready,
            absolute_deadline_us: 0,
            wcet_us: 0,
            execution_time_us: 0,
            last_activation_us: 0,
            deadline_misses: 0,
            completed_instances: 0,
        }
    }

    /// Set worst-case execution time
    pub fn with_wcet(mut self, wcet_us: u64) -> Self {
        self.wcet_us = wcet_us;
        self
    }

    /// Set base priority explicitly
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.base_priority = priority;
        self.effective_priority = priority;
        self
    }

    /// Get task ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Get task type
    pub fn task_type(&self) -> TaskType {
        self.task_type
    }

    /// Get base priority
    pub fn base_priority(&self) -> u8 {
        self.base_priority
    }

    /// Get effective priority (may be elevated due to inheritance)
    pub fn effective_priority(&self) -> u8 {
        self.effective_priority
    }

    /// Get current state
    pub fn state(&self) -> RtTaskState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: RtTaskState) {
        self.state = state;
    }

    /// Get absolute deadline
    pub fn absolute_deadline_us(&self) -> u64 {
        self.absolute_deadline_us
    }

    /// Get worst-case execution time
    pub fn wcet_us(&self) -> u64 {
        self.wcet_us
    }

    /// Get actual execution time for current instance
    pub fn execution_time_us(&self) -> u64 {
        self.execution_time_us
    }

    /// Get deadline misses count
    pub fn deadline_misses(&self) -> u64 {
        self.deadline_misses
    }

    /// Get completed instances count
    pub fn completed_instances(&self) -> u64 {
        self.completed_instances
    }

    /// Activate task (start new instance)
    pub fn activate(&mut self, current_time_us: u64) {
        self.last_activation_us = current_time_us;
        self.execution_time_us = 0;
        self.state = RtTaskState::Ready;

        // Set absolute deadline based on task type
        self.absolute_deadline_us = match self.task_type {
            TaskType::Periodic { deadline_us, .. } => current_time_us + deadline_us,
            TaskType::Sporadic { deadline_us, .. } => current_time_us + deadline_us,
            TaskType::Aperiodic => u64::MAX, // No deadline
        };
    }

    /// Record execution time
    pub fn add_execution_time(&mut self, elapsed_us: u64) {
        self.execution_time_us += elapsed_us;
    }

    /// Complete current instance
    pub fn complete(&mut self, current_time_us: u64) {
        self.state = RtTaskState::Completed;
        self.completed_instances += 1;

        // Check if deadline was missed
        if current_time_us > self.absolute_deadline_us {
            self.deadline_misses += 1;
        }
    }

    /// Calculate laxity (slack time)
    /// Laxity = deadline - current_time - remaining_execution_time
    pub fn laxity(&self, current_time_us: u64) -> i64 {
        let deadline = self.absolute_deadline_us as i64;
        let remaining = (self.wcet_us - self.execution_time_us) as i64;
        deadline - current_time_us as i64 - remaining
    }

    /// Elevate priority (for priority inheritance)
    pub fn elevate_priority(&mut self, new_priority: u8) {
        if new_priority > self.effective_priority {
            self.effective_priority = new_priority;
        }
    }

    /// Restore base priority (after resource release)
    pub fn restore_priority(&mut self) {
        self.effective_priority = self.base_priority;
    }

    /// Check if task is schedulable based on utilization bound
    /// For Rate Monotonic: U = C/T where C = WCET, T = Period
    pub fn utilization(&self) -> f64 {
        match self.task_type {
            TaskType::Periodic { period_us, .. } => self.wcet_us as f64 / period_us as f64,
            TaskType::Sporadic {
                min_interarrival_us,
                ..
            } => self.wcet_us as f64 / min_interarrival_us as f64,
            TaskType::Aperiodic => 0.0,
        }
    }
}

// =============================================================================
// Priority Inheritance Protocol
// =============================================================================

/// Resource protected by priority inheritance
#[derive(Debug)]
pub struct PriorityInheritanceResource {
    /// Resource identifier
    #[allow(dead_code)]
    id: usize,

    /// Task currently holding the resource (None if free)
    owner: Option<usize>,

    /// Tasks waiting for this resource
    waiters: Vec<usize>,

    /// Ceiling priority (highest priority of any task that uses this resource)
    ceiling_priority: AtomicU8,
}

impl PriorityInheritanceResource {
    /// Create a new priority inheritance resource
    pub fn new(id: usize) -> Self {
        Self {
            id,
            owner: None,
            waiters: Vec::new(),
            ceiling_priority: AtomicU8::new(0),
        }
    }

    /// Set ceiling priority
    pub fn set_ceiling_priority(&self, priority: u8) {
        self.ceiling_priority.store(priority, Ordering::Release);
    }

    /// Get ceiling priority
    pub fn ceiling_priority(&self) -> u8 {
        self.ceiling_priority.load(Ordering::Acquire)
    }

    /// Acquire resource (returns priority to inherit, if any)
    pub fn acquire(&mut self, task_id: usize, task_priority: u8) -> Option<u8> {
        if let Some(_owner_id) = self.owner {
            // Resource is held, add to waiters
            self.waiters.push(task_id);

            // Return waiter's priority for inheritance
            Some(task_priority)
        } else {
            // Resource is free, grant it
            self.owner = Some(task_id);
            None
        }
    }

    /// Release resource (returns next waiter, if any)
    pub fn release(&mut self, task_id: usize) -> Option<usize> {
        if self.owner == Some(task_id) {
            self.owner = None;

            // Find highest priority waiter
            if !self.waiters.is_empty() {
                // For simplicity, just take the first waiter
                // In production, should select highest priority waiter
                let next = self.waiters.remove(0);
                self.owner = Some(next);
                Some(next)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Get current owner
    pub fn owner(&self) -> Option<usize> {
        self.owner
    }

    /// Get waiting tasks
    pub fn waiters(&self) -> &[usize] {
        &self.waiters
    }
}

// =============================================================================
// Real-Time Scheduler
// =============================================================================

/// Real-time task scheduler
pub struct RealTimeScheduler {
    /// All real-time tasks
    tasks: Vec<RealTimeTask>,

    /// Scheduling policy
    policy: SchedulingPolicy,

    /// Priority inheritance resources
    resources: Vec<PriorityInheritanceResource>,

    /// System time in microseconds
    system_time_us: AtomicU64,
}

impl RealTimeScheduler {
    /// Create a new real-time scheduler
    pub fn new(policy: SchedulingPolicy) -> Self {
        Self {
            tasks: Vec::new(),
            policy,
            resources: Vec::new(),
            system_time_us: AtomicU64::new(0),
        }
    }

    /// Add a task to the scheduler
    pub fn add_task(&mut self, task: RealTimeTask) {
        self.tasks.push(task);
    }

    /// Remove a task from the scheduler
    pub fn remove_task(&mut self, task_id: usize) {
        self.tasks.retain(|t| t.id != task_id);
    }

    /// Get a task by ID
    pub fn get_task(&self, task_id: usize) -> Option<&RealTimeTask> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    /// Get a mutable task by ID
    pub fn get_task_mut(&mut self, task_id: usize) -> Option<&mut RealTimeTask> {
        self.tasks.iter_mut().find(|t| t.id == task_id)
    }

    /// Update system time
    pub fn update_time(&self, time_us: u64) {
        self.system_time_us.store(time_us, Ordering::Release);
    }

    /// Get current system time
    pub fn current_time_us(&self) -> u64 {
        self.system_time_us.load(Ordering::Acquire)
    }

    /// Select next task to run based on scheduling policy
    pub fn schedule(&mut self) -> Option<usize> {
        let current_time = self.current_time_us();

        match self.policy {
            SchedulingPolicy::FixedPriority | SchedulingPolicy::RateMonotonic => {
                self.schedule_fixed_priority()
            }
            SchedulingPolicy::EarliestDeadlineFirst => self.schedule_edf(current_time),
            SchedulingPolicy::LeastLaxityFirst => self.schedule_llf(current_time),
        }
    }

    /// Fixed priority scheduling
    fn schedule_fixed_priority(&self) -> Option<usize> {
        self.tasks
            .iter()
            .filter(|t| t.state == RtTaskState::Ready)
            .max_by_key(|t| t.effective_priority)
            .map(|t| t.id)
    }

    /// Earliest Deadline First scheduling
    fn schedule_edf(&self, _current_time: u64) -> Option<usize> {
        self.tasks
            .iter()
            .filter(|t| t.state == RtTaskState::Ready)
            .min_by_key(|t| t.absolute_deadline_us)
            .map(|t| t.id)
    }

    /// Least Laxity First scheduling
    fn schedule_llf(&self, current_time: u64) -> Option<usize> {
        self.tasks
            .iter()
            .filter(|t| t.state == RtTaskState::Ready)
            .min_by_key(|t| t.laxity(current_time))
            .map(|t| t.id)
    }

    /// Create a new priority inheritance resource
    pub fn create_resource(&mut self) -> usize {
        let id = self.resources.len();
        self.resources.push(PriorityInheritanceResource::new(id));
        id
    }

    /// Acquire a resource with priority inheritance
    pub fn acquire_resource(&mut self, resource_id: usize, task_id: usize) -> Result<(), RtError> {
        // First, get task priority
        let task_priority = self
            .get_task(task_id)
            .ok_or(RtError::TaskNotFound)?
            .effective_priority();

        // Then acquire resource
        let resource = self
            .resources
            .get_mut(resource_id)
            .ok_or(RtError::ResourceNotFound)?;

        if let Some(inherit_priority) = resource.acquire(task_id, task_priority) {
            // Get owner ID before modifying tasks
            let owner_id = resource.owner();

            // Resource is held by another task, inherit priority
            if let Some(_owner_id) = owner_id {
                if let Some(owner) = self.get_task_mut(_owner_id) {
                    owner.elevate_priority(inherit_priority);
                }
            }

            // Block the requesting task
            if let Some(requesting_task) = self.get_task_mut(task_id) {
                requesting_task.set_state(RtTaskState::Blocked);
            }
        }

        Ok(())
    }

    /// Release a resource and restore priority
    pub fn release_resource(&mut self, resource_id: usize, task_id: usize) -> Result<(), RtError> {
        let resource = self
            .resources
            .get_mut(resource_id)
            .ok_or(RtError::ResourceNotFound)?;

        if let Some(next_waiter) = resource.release(task_id) {
            // Wake up next waiter
            if let Some(waiter) = self.get_task_mut(next_waiter) {
                waiter.set_state(RtTaskState::Ready);
            }
        }

        // Restore releasing task's priority
        if let Some(task) = self.get_task_mut(task_id) {
            task.restore_priority();
        }

        Ok(())
    }

    /// Calculate total system utilization
    pub fn total_utilization(&self) -> f64 {
        self.tasks.iter().map(|t| t.utilization()).sum()
    }

    /// Check if task set is schedulable under current policy
    pub fn is_schedulable(&self) -> bool {
        match self.policy {
            SchedulingPolicy::RateMonotonic => {
                // Liu & Layland bound: U ≤ n(2^(1/n) - 1)
                let n = self
                    .tasks
                    .iter()
                    .filter(|t| matches!(t.task_type, TaskType::Periodic { .. }))
                    .count();

                if n == 0 {
                    return true;
                }

                // Approximate Liu & Layland bounds for small n
                // For n=1: 1.0, n=2: 0.828, n=3: 0.780, n=4: 0.757, etc.
                // As n→∞, bound approaches ln(2) ≈ 0.693
                let bound = match n {
                    1 => 1.0,
                    2 => 0.828,
                    3 => 0.780,
                    4 => 0.757,
                    5 => 0.743,
                    _ => 0.693, // Conservative bound for larger n
                };

                self.total_utilization() <= bound
            }
            SchedulingPolicy::EarliestDeadlineFirst => {
                // EDF: U ≤ 1.0 (optimal for single processor)
                self.total_utilization() <= 1.0
            }
            _ => true, // Conservative: assume schedulable
        }
    }
}

// =============================================================================
// Errors
// =============================================================================

/// Real-time scheduler errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtError {
    /// Task not found
    TaskNotFound,
    /// Resource not found
    ResourceNotFound,
    /// Deadline missed
    DeadlineMissed,
    /// Task set not schedulable
    NotSchedulable,
}

impl core::fmt::Display for RtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TaskNotFound => write!(f, "task not found"),
            Self::ResourceNotFound => write!(f, "resource not found"),
            Self::DeadlineMissed => write!(f, "deadline missed"),
            Self::NotSchedulable => write!(f, "task set not schedulable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_periodic_task_creation() {
        let task = RealTimeTask::new(
            0,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_wcet(2_000);

        assert_eq!(task.id(), 0);
        assert_eq!(task.wcet_us(), 2_000);
        assert_eq!(task.state(), RtTaskState::Ready);
    }

    #[test]
    fn test_task_activation() {
        let mut task = RealTimeTask::new(
            0,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_wcet(2_000);

        task.activate(0);
        assert_eq!(task.absolute_deadline_us(), 10_000);
        assert_eq!(task.execution_time_us(), 0);
    }

    #[test]
    fn test_task_utilization() {
        let task = RealTimeTask::new(
            0,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_wcet(2_000);

        assert!((task.utilization() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_priority_inheritance_resource() {
        let mut resource = PriorityInheritanceResource::new(0);

        // Task 1 acquires resource
        let result = resource.acquire(1, 100);
        assert!(result.is_none()); // No inheritance needed
        assert_eq!(resource.owner(), Some(1));

        // Task 2 tries to acquire (should block)
        let result = resource.acquire(2, 150);
        assert_eq!(result, Some(150)); // Should inherit priority 150

        // Task 1 releases
        let next = resource.release(1);
        assert_eq!(next, Some(2)); // Task 2 should get resource
    }

    #[test]
    fn test_edf_scheduling() {
        let mut scheduler = RealTimeScheduler::new(SchedulingPolicy::EarliestDeadlineFirst);

        let mut task1 = RealTimeTask::new(
            1,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        );
        task1.activate(0);

        let mut task2 = RealTimeTask::new(
            2,
            TaskType::Periodic {
                period_us: 20_000,
                deadline_us: 20_000,
            },
        );
        task2.activate(0);

        scheduler.add_task(task1);
        scheduler.add_task(task2);

        // Task 1 has earlier deadline, should be selected
        let selected = scheduler.schedule();
        assert_eq!(selected, Some(1));
    }

    #[test]
    fn test_rate_monotonic_priority() {
        let task1 = RealTimeTask::new(
            1,
            TaskType::Periodic {
                period_us: 5_000, // Shorter period
                deadline_us: 5_000,
            },
        );

        let task2 = RealTimeTask::new(
            2,
            TaskType::Periodic {
                period_us: 10_000, // Longer period
                deadline_us: 10_000,
            },
        );

        // Task 1 should have higher priority
        assert!(task1.base_priority() > task2.base_priority());
    }

    #[test]
    fn test_laxity_calculation() {
        let mut task = RealTimeTask::new(
            0,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_wcet(2_000);

        task.activate(0);

        // At time 0: laxity = 10000 - 0 - 2000 = 8000
        assert_eq!(task.laxity(0), 8000);

        // At time 5000: laxity = 10000 - 5000 - 2000 = 3000
        assert_eq!(task.laxity(5_000), 3_000);
    }

    #[test]
    fn test_deadline_miss_detection() {
        let mut task = RealTimeTask::new(
            0,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_wcet(2_000);

        task.activate(0);
        assert_eq!(task.deadline_misses(), 0);

        // Complete after deadline
        task.complete(15_000);
        assert_eq!(task.deadline_misses(), 1);

        // Complete before deadline
        task.activate(15_000);
        task.complete(20_000);
        assert_eq!(task.deadline_misses(), 1); // Still 1
    }

    #[test]
    fn test_priority_elevation() {
        let mut task = RealTimeTask::new(
            0,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_priority(100);

        assert_eq!(task.effective_priority(), 100);

        // Elevate priority
        task.elevate_priority(150);
        assert_eq!(task.effective_priority(), 150);
        assert_eq!(task.base_priority(), 100);

        // Restore priority
        task.restore_priority();
        assert_eq!(task.effective_priority(), 100);
    }

    #[test]
    fn test_schedulability_analysis() {
        let mut scheduler = RealTimeScheduler::new(SchedulingPolicy::EarliestDeadlineFirst);

        // Add tasks with total utilization < 1.0
        let task1 = RealTimeTask::new(
            1,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        )
        .with_wcet(2_000); // U = 0.2

        let task2 = RealTimeTask::new(
            2,
            TaskType::Periodic {
                period_us: 20_000,
                deadline_us: 20_000,
            },
        )
        .with_wcet(4_000); // U = 0.2

        scheduler.add_task(task1);
        scheduler.add_task(task2);

        assert!(scheduler.is_schedulable());
        assert!((scheduler.total_utilization() - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_resource_creation() {
        let mut scheduler = RealTimeScheduler::new(SchedulingPolicy::FixedPriority);

        let resource_id = scheduler.create_resource();
        assert_eq!(resource_id, 0);

        let resource_id2 = scheduler.create_resource();
        assert_eq!(resource_id2, 1);
    }

    #[test]
    fn test_acquire_release_resource() {
        let mut scheduler = RealTimeScheduler::new(SchedulingPolicy::FixedPriority);

        let mut task = RealTimeTask::new(
            1,
            TaskType::Periodic {
                period_us: 10_000,
                deadline_us: 10_000,
            },
        );
        task.activate(0);
        scheduler.add_task(task);

        let resource_id = scheduler.create_resource();

        // Acquire resource
        let result = scheduler.acquire_resource(resource_id, 1);
        assert!(result.is_ok());

        // Release resource
        let result = scheduler.release_resource(resource_id, 1);
        assert!(result.is_ok());
    }
}
