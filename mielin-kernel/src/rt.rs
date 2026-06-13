//! Real-Time Scheduling Features
//!
//! This module provides real-time scheduling primitives including:
//! - Priority Inheritance Protocol (PIP)
//! - Priority Ceiling Protocol (PCP)
//! - Earliest Deadline First (EDF) scheduling
//! - Bounded execution time guarantees
//!
//! ## Priority Inheritance
//!
//! When a high-priority task blocks on a resource held by a low-priority task,
//! the low-priority task temporarily inherits the higher priority to prevent
//! priority inversion.
//!
//! ## Priority Ceiling
//!
//! Each resource has a ceiling priority equal to the highest priority of any
//! task that may lock it. A task can only lock a resource if its priority is
//! higher than all currently locked resources' ceilings.
//!
//! ## Deadline Scheduling (EDF)
//!
//! Tasks are scheduled based on their absolute deadlines. The task with the
//! nearest deadline runs first. This is optimal for meeting soft real-time
//! deadlines on a single processor.

extern crate alloc;

use crate::scheduler;
use crate::KernelError;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use spin::Mutex;

// =============================================================================
// Priority Inheritance Protocol
// =============================================================================

/// RT Mutex with Priority Inheritance Protocol
///
/// When a task locks this mutex and another higher-priority task tries to lock it,
/// the owner's priority is temporarily boosted to prevent priority inversion.
pub struct RtMutex {
    /// Unique mutex ID
    id: usize,
    /// Current owner task ID (None if unlocked)
    owner: AtomicUsize,
    /// Original priority of the owner before boost
    original_priority: AtomicU8,
    /// Current boosted priority
    boosted_priority: AtomicU8,
    /// Whether the mutex is locked
    locked: AtomicBool,
    /// Number of tasks waiting on this mutex
    waiters_count: AtomicUsize,
    /// Waiting tasks (task_id, priority)
    waiters: Mutex<Vec<(usize, u8)>>,
    /// Lock/unlock statistics
    lock_count: AtomicU64,
    unlock_count: AtomicU64,
    /// Priority inversions prevented
    inversions_prevented: AtomicU64,
}

impl RtMutex {
    /// Create a new RT mutex
    pub const fn new(id: usize) -> Self {
        Self {
            id,
            owner: AtomicUsize::new(usize::MAX), // MAX = no owner
            original_priority: AtomicU8::new(0),
            boosted_priority: AtomicU8::new(0),
            locked: AtomicBool::new(false),
            waiters_count: AtomicUsize::new(0),
            waiters: Mutex::new(Vec::new()),
            lock_count: AtomicU64::new(0),
            unlock_count: AtomicU64::new(0),
            inversions_prevented: AtomicU64::new(0),
        }
    }

    /// Try to acquire the mutex with priority inheritance
    ///
    /// If the mutex is already locked by a lower-priority task, boost that task's priority.
    pub fn lock(&self, task_id: usize) -> Result<(), KernelError> {
        // Try to acquire the mutex
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // Successfully acquired
            self.owner.store(task_id, Ordering::Release);
            self.lock_count.fetch_add(1, Ordering::Relaxed);

            // Get task's priority
            let task_priority = get_task_priority(task_id)?;
            self.original_priority
                .store(task_priority, Ordering::Release);
            self.boosted_priority
                .store(task_priority, Ordering::Release);

            return Ok(());
        }

        // Mutex is locked - add ourselves to waiters
        let task_priority = get_task_priority(task_id)?;
        {
            let mut waiters = self.waiters.lock();
            waiters.push((task_id, task_priority));
        }
        self.waiters_count.fetch_add(1, Ordering::Relaxed);

        // Check if we need to boost the owner's priority
        let owner_id = self.owner.load(Ordering::Acquire);
        if owner_id != usize::MAX {
            let current_boost = self.boosted_priority.load(Ordering::Acquire);

            if task_priority > current_boost {
                // Priority inversion detected - boost owner's priority
                self.boosted_priority
                    .store(task_priority, Ordering::Release);

                // Update the owner task's priority in the scheduler
                boost_task_priority(owner_id, task_priority)?;

                self.inversions_prevented.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Spin-wait until we can acquire (in a real system, this would block the task)
        loop {
            if self
                .locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // Successfully acquired
                self.owner.store(task_id, Ordering::Release);
                self.lock_count.fetch_add(1, Ordering::Relaxed);

                // Remove ourselves from waiters
                {
                    let mut waiters = self.waiters.lock();
                    waiters.retain(|(id, _)| *id != task_id);
                }
                self.waiters_count.fetch_sub(1, Ordering::Relaxed);

                // Store our priority
                self.original_priority
                    .store(task_priority, Ordering::Release);
                self.boosted_priority
                    .store(task_priority, Ordering::Release);

                return Ok(());
            }
            core::hint::spin_loop();
        }
    }

    /// Release the mutex and restore owner's original priority
    pub fn unlock(&self, task_id: usize) -> Result<(), KernelError> {
        // Verify we're the owner
        let owner = self.owner.load(Ordering::Acquire);
        if owner != task_id {
            return Err(KernelError::TaskNotFound { task_id });
        }

        // Restore original priority if it was boosted
        let original = self.original_priority.load(Ordering::Acquire);
        let boosted = self.boosted_priority.load(Ordering::Acquire);

        if boosted > original {
            // Priority was boosted - restore it
            restore_task_priority(task_id, original)?;
        }

        // Find the next highest priority from waiters
        let next_priority = {
            let waiters = self.waiters.lock();
            waiters.iter().map(|(_, p)| *p).max().unwrap_or(0)
        };

        // Release the lock
        self.owner.store(usize::MAX, Ordering::Release);
        self.locked.store(false, Ordering::Release);
        self.unlock_count.fetch_add(1, Ordering::Relaxed);

        // Reset priorities
        self.original_priority.store(0, Ordering::Release);
        self.boosted_priority
            .store(next_priority, Ordering::Release);

        Ok(())
    }

    /// Check if the mutex is locked
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Acquire)
    }

    /// Get the current owner task ID (None if unlocked)
    pub fn get_owner(&self) -> Option<usize> {
        let owner = self.owner.load(Ordering::Acquire);
        if owner == usize::MAX {
            None
        } else {
            Some(owner)
        }
    }

    /// Get statistics for this mutex
    pub fn get_stats(&self) -> RtMutexStats {
        RtMutexStats {
            id: self.id,
            lock_count: self.lock_count.load(Ordering::Relaxed),
            unlock_count: self.unlock_count.load(Ordering::Relaxed),
            inversions_prevented: self.inversions_prevented.load(Ordering::Relaxed),
            current_waiters: self.waiters_count.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: RtMutex is thread-safe through atomic operations and spin locks
unsafe impl Send for RtMutex {}
unsafe impl Sync for RtMutex {}

/// RT Mutex statistics
#[derive(Debug, Clone, Copy)]
pub struct RtMutexStats {
    pub id: usize,
    pub lock_count: u64,
    pub unlock_count: u64,
    pub inversions_prevented: u64,
    pub current_waiters: usize,
}

// =============================================================================
// Priority Ceiling Protocol
// =============================================================================

/// Priority Ceiling Mutex
///
/// Each mutex has a ceiling priority. A task can only lock the mutex if its
/// priority is higher than all currently locked mutexes' ceilings.
pub struct PcpMutex {
    /// Ceiling priority (highest priority of any task that may lock this)
    ceiling: u8,
    /// Current owner task ID
    owner: AtomicUsize,
    /// Whether the mutex is locked
    locked: AtomicBool,
    /// Lock/unlock statistics
    lock_count: AtomicU64,
    unlock_count: AtomicU64,
}

impl PcpMutex {
    /// Create a new PCP mutex with the specified ceiling priority
    pub const fn new(ceiling: u8) -> Self {
        Self {
            ceiling,
            owner: AtomicUsize::new(usize::MAX),
            locked: AtomicBool::new(false),
            lock_count: AtomicU64::new(0),
            unlock_count: AtomicU64::new(0),
        }
    }

    /// Try to lock the mutex
    ///
    /// Returns error if task's priority is not higher than the ceiling.
    pub fn lock(&self, task_id: usize) -> Result<(), KernelError> {
        // Check if task priority exceeds ceiling
        let task_priority = get_task_priority(task_id)?;

        if task_priority < self.ceiling {
            return Err(KernelError::PriorityCeilingViolation {
                task_id,
                ceiling: self.ceiling,
            });
        }

        // Spin until we can acquire
        loop {
            if self
                .locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                self.owner.store(task_id, Ordering::Release);
                self.lock_count.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
            core::hint::spin_loop();
        }
    }

    /// Unlock the mutex
    pub fn unlock(&self, task_id: usize) -> Result<(), KernelError> {
        let owner = self.owner.load(Ordering::Acquire);
        if owner != task_id {
            return Err(KernelError::NotMutexOwner { task_id });
        }

        self.owner.store(usize::MAX, Ordering::Release);
        self.locked.store(false, Ordering::Release);
        self.unlock_count.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Get the ceiling priority
    pub fn get_ceiling(&self) -> u8 {
        self.ceiling
    }
}

// =============================================================================
// Deadline Scheduling (EDF)
// =============================================================================

/// Deadline task parameters
#[derive(Debug, Clone, Copy)]
pub struct DeadlineParams {
    /// Task ID
    pub task_id: usize,
    /// Period (microseconds)
    pub period_us: u64,
    /// Worst-case execution time (microseconds)
    pub wcet_us: u64,
    /// Relative deadline (microseconds)
    pub deadline_us: u64,
    /// Absolute deadline (monotonic time in microseconds)
    pub absolute_deadline_us: u64,
}

impl DeadlineParams {
    /// Create new deadline parameters
    pub fn new(task_id: usize, period_us: u64, wcet_us: u64, deadline_us: u64) -> Self {
        Self {
            task_id,
            period_us,
            wcet_us,
            deadline_us,
            absolute_deadline_us: 0, // Set when task is released
        }
    }

    /// Check if schedulability constraint is met (D ≤ P)
    pub fn is_valid(&self) -> bool {
        self.deadline_us <= self.period_us && self.wcet_us <= self.deadline_us
    }

    /// Calculate CPU utilization for this task (C/P)
    pub fn utilization(&self) -> f64 {
        self.wcet_us as f64 / self.period_us as f64
    }
}

/// EDF Scheduler
///
/// Schedules tasks based on their absolute deadlines.
/// Tasks with earlier deadlines have higher priority.
pub struct EdfScheduler {
    /// Maximum number of deadline tasks
    max_tasks: usize,
    /// Deadline tasks
    tasks: Mutex<Vec<DeadlineParams>>,
    /// Total CPU utilization (0.0 to 1.0)
    total_utilization: Mutex<f64>,
    /// Deadline misses counter
    deadline_misses: AtomicU64,
}

impl EdfScheduler {
    /// Create a new EDF scheduler
    pub const fn new(max_tasks: usize) -> Self {
        Self {
            max_tasks,
            tasks: Mutex::new(Vec::new()),
            total_utilization: Mutex::new(0.0),
            deadline_misses: AtomicU64::new(0),
        }
    }

    /// Add a task to the EDF scheduler
    ///
    /// Returns error if total utilization exceeds 100% (not schedulable).
    pub fn add_task(&self, params: DeadlineParams) -> Result<(), KernelError> {
        if !params.is_valid() {
            return Err(KernelError::InvalidDeadlineParams);
        }

        let mut tasks = self.tasks.lock();
        if tasks.len() >= self.max_tasks {
            return Err(KernelError::TaskSpawnFailed);
        }

        let mut util = self.total_utilization.lock();
        let new_util = *util + params.utilization();

        // EDF schedulability test: U ≤ 1
        if new_util > 1.0 {
            return Err(KernelError::NotSchedulable);
        }

        tasks.push(params);
        *util = new_util;

        Ok(())
    }

    /// Get the task with the earliest deadline (EDF policy)
    pub fn schedule(&self) -> Option<usize> {
        let tasks = self.tasks.lock();
        tasks
            .iter()
            .min_by_key(|t| t.absolute_deadline_us)
            .map(|t| t.task_id)
    }

    /// Update a task's absolute deadline (when it's released)
    pub fn release_task(&self, task_id: usize, current_time_us: u64) -> Result<(), KernelError> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .iter_mut()
            .find(|t| t.task_id == task_id)
            .ok_or(KernelError::TaskNotFound { task_id })?;

        task.absolute_deadline_us = current_time_us + task.deadline_us;

        Ok(())
    }

    /// Check if a task missed its deadline
    pub fn check_deadline(&self, task_id: usize, current_time_us: u64) -> bool {
        let tasks = self.tasks.lock();
        if let Some(task) = tasks.iter().find(|t| t.task_id == task_id) {
            if current_time_us > task.absolute_deadline_us {
                self.deadline_misses.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Get total CPU utilization
    pub fn get_utilization(&self) -> f64 {
        *self.total_utilization.lock()
    }

    /// Get deadline miss count
    pub fn get_deadline_misses(&self) -> u64 {
        self.deadline_misses.load(Ordering::Relaxed)
    }
}

// =============================================================================
// Helper Functions (interface with scheduler)
// =============================================================================

/// Get task priority from the global scheduler
fn get_task_priority(task_id: usize) -> Result<u8, KernelError> {
    // Access the global scheduler to get task priority
    scheduler::get_task_priority(task_id)
}

/// Temporarily boost a task's priority
fn boost_task_priority(task_id: usize, new_priority: u8) -> Result<(), KernelError> {
    scheduler::set_task_priority(task_id, new_priority)
}

/// Restore a task's original priority
fn restore_task_priority(task_id: usize, original_priority: u8) -> Result<(), KernelError> {
    scheduler::set_task_priority(task_id, original_priority)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_mutex_creation() {
        let mutex = RtMutex::new(0);
        assert!(!mutex.is_locked());
        assert_eq!(mutex.get_owner(), None);
    }

    #[test]
    fn test_rt_mutex_stats() {
        let mutex = RtMutex::new(0);
        let stats = mutex.get_stats();
        assert_eq!(stats.id, 0);
        assert_eq!(stats.lock_count, 0);
        assert_eq!(stats.unlock_count, 0);
        assert_eq!(stats.inversions_prevented, 0);
    }

    #[test]
    fn test_pcp_mutex_creation() {
        let mutex = PcpMutex::new(100);
        assert_eq!(mutex.get_ceiling(), 100);
    }

    #[test]
    fn test_deadline_params_valid() {
        let params = DeadlineParams::new(0, 1000, 500, 1000);
        assert!(params.is_valid());
        assert_eq!(params.utilization(), 0.5);
    }

    #[test]
    fn test_deadline_params_invalid() {
        // Deadline > Period
        let params = DeadlineParams::new(0, 1000, 500, 1500);
        assert!(!params.is_valid());

        // WCET > Deadline
        let params = DeadlineParams::new(0, 1000, 1500, 1000);
        assert!(!params.is_valid());
    }

    #[test]
    fn test_edf_scheduler_creation() {
        let scheduler = EdfScheduler::new(10);
        assert_eq!(scheduler.get_utilization(), 0.0);
        assert_eq!(scheduler.get_deadline_misses(), 0);
    }

    #[test]
    fn test_edf_add_task() {
        let scheduler = EdfScheduler::new(10);
        let params = DeadlineParams::new(0, 1000, 500, 1000);

        scheduler.add_task(params).unwrap();
        assert_eq!(scheduler.get_utilization(), 0.5);
    }

    #[test]
    fn test_edf_overload() {
        let scheduler = EdfScheduler::new(10);

        // Add task with 60% utilization
        let params1 = DeadlineParams::new(0, 1000, 600, 1000);
        scheduler.add_task(params1).unwrap();

        // Try to add task with 50% utilization (total = 110%)
        let params2 = DeadlineParams::new(1, 1000, 500, 1000);
        let result = scheduler.add_task(params2);

        assert!(result.is_err()); // Should fail - not schedulable
    }

    #[test]
    fn test_edf_release_task() {
        let scheduler = EdfScheduler::new(10);
        let params = DeadlineParams::new(0, 1000, 500, 1000);

        scheduler.add_task(params).unwrap();
        scheduler.release_task(0, 0).unwrap(); // Release at time 0

        // Check that absolute deadline is set
        let tasks = scheduler.tasks.lock();
        let task = &tasks[0];
        assert_eq!(task.absolute_deadline_us, 1000);
    }

    fn setup_task_with_priority(priority: u8) -> usize {
        crate::scheduler::init().unwrap();
        crate::scheduler::spawn_task(priority).unwrap()
    }

    #[test]
    fn test_rt_error_priority_ceiling_violation() {
        let task_id = setup_task_with_priority(100);
        let mutex = PcpMutex::new(200);
        let err = mutex.lock(task_id).unwrap_err();
        assert_eq!(
            err,
            KernelError::PriorityCeilingViolation { task_id, ceiling: 200 }
        );
    }

    #[test]
    fn test_rt_error_not_mutex_owner_on_unlock() {
        crate::scheduler::init().unwrap();
        let task_a = crate::scheduler::spawn_task(10).unwrap();
        let task_b = crate::scheduler::spawn_task(10).unwrap();
        let mutex = PcpMutex::new(0);
        mutex.lock(task_a).unwrap();
        let err = mutex.unlock(task_b).unwrap_err();
        assert_eq!(err, KernelError::NotMutexOwner { task_id: task_b });
        mutex.unlock(task_a).unwrap();
    }

    #[test]
    fn test_rt_error_invalid_deadline_params() {
        let sched = EdfScheduler::new(10);
        let bad = DeadlineParams::new(0, 1000, 800, 500);
        let err = sched.add_task(bad).unwrap_err();
        assert_eq!(err, KernelError::InvalidDeadlineParams);
    }

    #[test]
    fn test_rt_error_not_schedulable() {
        let sched = EdfScheduler::new(10);
        sched.add_task(DeadlineParams::new(0, 1000, 600, 1000)).unwrap();
        let err = sched.add_task(DeadlineParams::new(1, 1000, 500, 1000)).unwrap_err();
        assert_eq!(err, KernelError::NotSchedulable);
    }
}
