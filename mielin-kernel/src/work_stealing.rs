//! Work-Stealing Scheduler for MielinOS Kernel
//!
//! Implements a production-quality work-stealing scheduler based on the Chase-Lev
//! deque algorithm.  Each worker owns a local deque; when idle it steals from a
//! randomly chosen peer using a lightweight LCG PRNG (no external dependency).
//!
//! # Design
//!
//! - Up to `MAX_WORKERS` (8) hardware threads are supported.
//! - Per-worker priority is modelled with `NUM_PRIORITY_BUCKETS` (256) sub-deques,
//!   one per priority level.  `schedule()` drains the highest non-empty bucket
//!   before falling back to work-stealing.
//! - Work-stealing uses randomised victim selection via an LCG seeded from the
//!   worker id so different workers diverge immediately.
//! - Metrics are tracked with relaxed atomics and summarised via snapshot.

extern crate alloc;

use crate::lockfree::{Steal, WorkStealingDeque};
use crate::KernelError;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// =============================================================================
// Constants
// =============================================================================

pub const MAX_WORKERS: usize = 8;
const NUM_PRIORITY_BUCKETS: usize = 256;

/// Sentinel value stored in `WorkerState::current_task` when the worker is idle.
const IDLE_SENTINEL: usize = usize::MAX;

// =============================================================================
// Task handle
// =============================================================================

/// Opaque handle to a task managed by the `WorkStealingScheduler`.
///
/// The handle encodes both the worker-local sequence number and the priority
/// so that the scheduler can locate and remove tasks efficiently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskHandle {
    pub id: usize,
    pub priority: u8,
    pub worker_hint: usize,
}

// =============================================================================
// Task entry stored inside the per-priority deque
// =============================================================================

/// A concrete entry held inside a worker's priority bucket deque.
#[derive(Debug, Clone, Copy)]
pub struct TaskEntry {
    pub handle: TaskHandle,
}

// =============================================================================
// Metrics
// =============================================================================

/// Live atomic metrics for the `WorkStealingScheduler`.
pub struct WorkStealingMetrics {
    pub total_scheduled: AtomicU64,
    pub total_stolen: AtomicU64,
    pub total_idle: AtomicU64,
    pub peak_queue_depth: AtomicUsize,
}

impl WorkStealingMetrics {
    const fn new() -> Self {
        Self {
            total_scheduled: AtomicU64::new(0),
            total_stolen: AtomicU64::new(0),
            total_idle: AtomicU64::new(0),
            peak_queue_depth: AtomicUsize::new(0),
        }
    }

    fn update_peak(&self, depth: usize) {
        let mut cur = self.peak_queue_depth.load(Ordering::Relaxed);
        while depth > cur {
            match self.peak_queue_depth.compare_exchange_weak(
                cur,
                depth,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => cur = v,
            }
        }
    }
}

/// Point-in-time snapshot of scheduler metrics (no atomics; cheap to copy).
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkStealingMetricsSnapshot {
    pub total_scheduled: u64,
    pub total_stolen: u64,
    pub total_idle: u64,
    pub peak_queue_depth: usize,
}

// =============================================================================
// Per-worker state
// =============================================================================

/// Per-priority-level deque wrapper.
///
/// Using a heap-allocated box so `WorkerState` stays `Sized` and can be placed
/// inside a fixed-length array without triggering stack-size blowup.
struct PriorityDeques {
    /// Index 0 = priority 0 (lowest), index 255 = priority 255 (highest).
    buckets: Box<[WorkStealingDeque<TaskEntry>; NUM_PRIORITY_BUCKETS]>,
    /// Approximate item count across all buckets (Relaxed – best-effort).
    total_len: AtomicUsize,
}

impl PriorityDeques {
    fn new() -> Self {
        // Build each deque individually then collect into a boxed array.
        // `Box::new([...; N])` with N=256 would stack-allocate during construction
        // on some targets; using Vec avoids the temporary stack frame.
        let mut v: alloc::vec::Vec<WorkStealingDeque<TaskEntry>> =
            alloc::vec::Vec::with_capacity(NUM_PRIORITY_BUCKETS);
        for _ in 0..NUM_PRIORITY_BUCKETS {
            v.push(WorkStealingDeque::new(256));
        }
        let arr: Box<[WorkStealingDeque<TaskEntry>; NUM_PRIORITY_BUCKETS]> =
            v.into_boxed_slice().try_into().unwrap_or_else(|_| {
                // This branch is statically impossible.
                panic!("priority bucket count mismatch")
            });
        Self {
            buckets: arr,
            total_len: AtomicUsize::new(0),
        }
    }

    fn push(&self, entry: TaskEntry) {
        let priority = entry.handle.priority as usize;
        self.buckets[priority].push(entry);
        self.total_len.fetch_add(1, Ordering::Relaxed);
    }

    /// Pop the highest-priority task available in this worker's deques.
    fn pop_highest(&self) -> Option<TaskEntry> {
        // Scan from highest to lowest priority.
        for p in (0..NUM_PRIORITY_BUCKETS).rev() {
            if let Some(entry) = self.buckets[p].pop() {
                self.total_len.fetch_sub(1, Ordering::Relaxed);
                return Some(entry);
            }
        }
        None
    }

    /// Steal one task (highest-priority available) from this worker's deques
    /// as a thief.  Uses `steal()` (top/FIFO side).
    fn steal_one(&self) -> Steal<TaskEntry> {
        for p in (0..NUM_PRIORITY_BUCKETS).rev() {
            match self.buckets[p].steal() {
                Steal::Success(e) => {
                    self.total_len.fetch_sub(1, Ordering::Relaxed);
                    return Steal::Success(e);
                }
                Steal::Retry => return Steal::Retry,
                Steal::Empty => continue,
            }
        }
        Steal::Empty
    }

    fn total_len(&self) -> usize {
        self.total_len.load(Ordering::Relaxed)
    }
}

// =============================================================================
// Worker state
// =============================================================================

struct WorkerState {
    deques: PriorityDeques,
    /// Task id currently being executed, or `IDLE_SENTINEL`.
    current_task: AtomicUsize,
    steal_attempts: AtomicU64,
    steal_successes: AtomicU64,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            deques: PriorityDeques::new(),
            current_task: AtomicUsize::new(IDLE_SENTINEL),
            steal_attempts: AtomicU64::new(0),
            steal_successes: AtomicU64::new(0),
        }
    }
}

// =============================================================================
// LCG PRNG (no external dependency)
// =============================================================================

/// Linear-congruential generator using Knuth's constants.
///
/// Good enough for randomised victim selection; not cryptographic.
struct Lcg {
    state: u64,
}

impl Lcg {
    const fn new(seed: u64) -> Self {
        // Ensure the seed is odd (for full-period LCG over 2^64).
        Self { state: seed | 1 }
    }

    /// Advance the generator and return the next pseudo-random value.
    #[inline]
    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    /// Return a pseudo-random usize in `[0, n)`.
    #[inline]
    fn next_usize(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

// =============================================================================
// Global task-id counter
// =============================================================================

static NEXT_TASK_ID: AtomicUsize = AtomicUsize::new(0);

fn alloc_task_id() -> usize {
    NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
}

// =============================================================================
// WorkStealingScheduler
// =============================================================================

/// Work-stealing scheduler supporting up to `MAX_WORKERS` (8) CPU cores.
///
/// Each worker has 256 per-priority deques.  `schedule()` pops from the
/// worker's own deques (highest priority first); if empty, it steals from a
/// randomly selected peer.
pub struct WorkStealingScheduler {
    workers: [WorkerState; MAX_WORKERS],
    num_workers: usize,
    metrics: WorkStealingMetrics,
}

// SAFETY: WorkerState contains WorkStealingDeque which is Sync.
// AtomicU64 / AtomicUsize are Sync.
unsafe impl Send for WorkStealingScheduler {}
unsafe impl Sync for WorkStealingScheduler {}

impl WorkStealingScheduler {
    /// Create a new scheduler with the given number of worker threads.
    ///
    /// `num_workers` is clamped to `[1, MAX_WORKERS]`.
    pub fn new(num_workers: usize) -> Self {
        let num_workers = num_workers.clamp(1, MAX_WORKERS);
        Self {
            workers: core::array::from_fn(|_| WorkerState::new()),
            num_workers,
            metrics: WorkStealingMetrics::new(),
        }
    }

    /// Spawn a task onto a specific worker's deque.
    ///
    /// If `worker_hint >= num_workers`, the task is placed on the worker
    /// with the smallest current queue depth (least-loaded placement).
    pub fn spawn_task(&self, worker_hint: usize, priority: u8) -> Result<TaskHandle, KernelError> {
        let target = if worker_hint < self.num_workers {
            worker_hint
        } else {
            self.least_loaded_worker()
        };

        let id = alloc_task_id();
        let handle = TaskHandle {
            id,
            priority,
            worker_hint: target,
        };
        let entry = TaskEntry { handle };

        self.workers[target].deques.push(entry);

        let depth = self.total_queue_depth();
        self.metrics.update_peak(depth);
        self.metrics.total_scheduled.fetch_add(1, Ordering::Relaxed);

        Ok(handle)
    }

    /// Schedule the next task for the given worker.
    ///
    /// Pops from the worker's own deques first (highest priority); if empty,
    /// attempts to steal from up to `num_workers - 1` random victims.
    pub fn schedule(&self, worker_id: usize) -> Option<TaskHandle> {
        if worker_id >= self.num_workers {
            return None;
        }

        let worker = &self.workers[worker_id];

        // Local pop – highest priority first.
        if let Some(entry) = worker.deques.pop_highest() {
            worker
                .current_task
                .store(entry.handle.id, Ordering::Relaxed);
            return Some(entry.handle);
        }

        // Work-stealing phase.
        let stolen = self.try_steal(worker_id);
        if let Some(handle) = stolen {
            worker.current_task.store(handle.id, Ordering::Relaxed);
            self.metrics.total_stolen.fetch_add(1, Ordering::Relaxed);
            return Some(handle);
        }

        self.metrics.total_idle.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Re-queue the current task at the back of this worker's deque.
    pub fn yield_task(&self, worker_id: usize) {
        if worker_id >= self.num_workers {
            return;
        }
        let worker = &self.workers[worker_id];
        let current_id = worker.current_task.swap(IDLE_SENTINEL, Ordering::Relaxed);
        if current_id == IDLE_SENTINEL {
            return;
        }
        // Reconstruct a minimal entry.  Priority 0 is used as a placeholder
        // because we do not store priority alongside the running task id.
        // Callers that need accurate re-queuing should use `yield_task_with_priority`.
        let handle = TaskHandle {
            id: current_id,
            priority: 0,
            worker_hint: worker_id,
        };
        worker.deques.push(TaskEntry { handle });
    }

    /// Re-queue the current task with its known priority.
    pub fn yield_task_with_priority(&self, worker_id: usize, priority: u8) {
        if worker_id >= self.num_workers {
            return;
        }
        let worker = &self.workers[worker_id];
        let current_id = worker.current_task.swap(IDLE_SENTINEL, Ordering::Relaxed);
        if current_id == IDLE_SENTINEL {
            return;
        }
        let handle = TaskHandle {
            id: current_id,
            priority,
            worker_hint: worker_id,
        };
        worker.deques.push(TaskEntry { handle });
    }

    /// Terminate a task: mark the worker as idle.
    ///
    /// Because tasks are values (not pointers to state), "termination" simply
    /// means the worker stops tracking the handle.  If the handle was never
    /// scheduled (i.e. still in a deque), this returns `KernelError::TaskNotFound`.
    pub fn terminate_task(&self, worker_id: usize, handle: TaskHandle) -> Result<(), KernelError> {
        if worker_id >= self.num_workers {
            return Err(KernelError::TaskNotFound { task_id: handle.id });
        }
        let worker = &self.workers[worker_id];
        let cur = worker.current_task.load(Ordering::Relaxed);
        if cur == handle.id {
            worker.current_task.store(IDLE_SENTINEL, Ordering::Relaxed);
            Ok(())
        } else {
            Err(KernelError::TaskNotFound { task_id: handle.id })
        }
    }

    /// Return a snapshot of all scheduler metrics.
    pub fn snapshot_metrics(&self) -> WorkStealingMetricsSnapshot {
        WorkStealingMetricsSnapshot {
            total_scheduled: self.metrics.total_scheduled.load(Ordering::Relaxed),
            total_stolen: self.metrics.total_stolen.load(Ordering::Relaxed),
            total_idle: self.metrics.total_idle.load(Ordering::Relaxed),
            peak_queue_depth: self.metrics.peak_queue_depth.load(Ordering::Relaxed),
        }
    }

    /// Return the total number of tasks currently queued across all workers.
    pub fn total_queue_depth(&self) -> usize {
        self.workers[..self.num_workers]
            .iter()
            .map(|w| w.deques.total_len())
            .sum()
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    fn least_loaded_worker(&self) -> usize {
        let mut best = 0;
        let mut best_len = self.workers[0].deques.total_len();
        for i in 1..self.num_workers {
            let l = self.workers[i].deques.total_len();
            if l < best_len {
                best_len = l;
                best = i;
            }
        }
        best
    }

    fn try_steal(&self, worker_id: usize) -> Option<TaskHandle> {
        if self.num_workers <= 1 {
            return None;
        }
        let mut rng = Lcg::new((worker_id as u64).wrapping_add(1));
        let other_count = self.num_workers - 1;

        for _ in 0..other_count {
            let raw = rng.next_usize(other_count);
            // Map raw index to victim id (skip self).
            let victim = if raw < worker_id { raw } else { raw + 1 };

            self.workers[victim]
                .steal_attempts
                .fetch_add(1, Ordering::Relaxed);
            match self.workers[victim].deques.steal_one() {
                Steal::Success(entry) => {
                    self.workers[victim]
                        .steal_successes
                        .fetch_add(1, Ordering::Relaxed);
                    return Some(entry.handle);
                }
                Steal::Retry | Steal::Empty => continue,
            }
        }
        None
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    // -------------------------------------------------------------------------
    // Chase-Lev deque tests (scheduler-level scenarios)
    // -------------------------------------------------------------------------

    #[test]
    fn test_chase_lev_push_pop_single_thread() {
        let deque: WorkStealingDeque<u64> = WorkStealingDeque::new(4);
        assert!(deque.is_empty());

        deque.push(10);
        deque.push(20);
        deque.push(30);
        assert_eq!(deque.len(), 3);

        assert_eq!(deque.pop(), Some(30));
        assert_eq!(deque.pop(), Some(20));
        assert_eq!(deque.pop(), Some(10));
        assert_eq!(deque.pop(), None);
    }

    #[test]
    fn test_chase_lev_steal_from_other_thread() {
        let deque = Arc::new(WorkStealingDeque::<u32>::new(16));

        for i in 0..8_u32 {
            deque.push(i);
        }

        let thief = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            let mut stolen = alloc::vec::Vec::new();
            for _ in 0..8 {
                loop {
                    match thief.steal() {
                        Steal::Success(v) => {
                            stolen.push(v);
                            break;
                        }
                        Steal::Retry => core::hint::spin_loop(),
                        Steal::Empty => break,
                    }
                }
            }
            stolen
        });

        let stolen = handle.join().unwrap();
        let mut remaining = alloc::vec::Vec::new();
        while let Some(v) = deque.pop() {
            remaining.push(v);
        }

        let mut all: alloc::vec::Vec<u32> = stolen.into_iter().chain(remaining).collect();
        all.sort();
        assert_eq!(all, (0..8_u32).collect::<alloc::vec::Vec<_>>());
    }

    #[test]
    fn test_chase_lev_concurrent_steals() {
        use std::sync::Barrier;

        const ITEMS: u64 = 64;
        const THIEVES: usize = 4;

        let deque = Arc::new(WorkStealingDeque::<u64>::new(128));
        let barrier = Arc::new(Barrier::new(THIEVES + 1));

        for i in 0..ITEMS {
            deque.push(i);
        }

        let mut handles = alloc::vec::Vec::new();
        for _ in 0..THIEVES {
            let d = Arc::clone(&deque);
            let b = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                b.wait();
                let mut count = 0usize;
                let mut spin = 0usize;
                loop {
                    match d.steal() {
                        Steal::Success(_) => {
                            count += 1;
                            spin = 0;
                        }
                        Steal::Retry => {
                            spin += 1;
                            if spin > 100_000 {
                                break;
                            }
                            core::hint::spin_loop();
                        }
                        Steal::Empty => break,
                    }
                }
                count
            }));
        }

        barrier.wait();
        let mut owner_count = 0usize;
        for _ in 0..ITEMS {
            if deque.pop().is_some() {
                owner_count += 1;
            }
        }

        let total: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .sum::<usize>()
            + owner_count;
        assert_eq!(total as u64, ITEMS);
    }

    #[test]
    fn test_chase_lev_empty_steal_returns_empty() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        assert!(matches!(deque.steal(), Steal::Empty));
    }

    // -------------------------------------------------------------------------
    // WorkStealingScheduler tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_work_stealing_scheduler_basic_schedule() {
        let sched = WorkStealingScheduler::new(2);
        let h = sched.spawn_task(0, 100).unwrap();
        let got = sched.schedule(0);
        assert!(got.is_some());
        assert_eq!(got.unwrap().id, h.id);
    }

    #[test]
    fn test_work_stealing_scheduler_multi_worker() {
        let sched = WorkStealingScheduler::new(4);

        let mut handles = alloc::vec::Vec::new();
        for w in 0..4 {
            for _ in 0..4 {
                handles.push(sched.spawn_task(w, 50).unwrap());
            }
        }

        // Each worker should be able to pop its own tasks.
        let mut total = 0usize;
        for w in 0..4 {
            while sched.schedule(w).is_some() {
                total += 1;
            }
        }
        assert_eq!(total, 16);
    }

    #[test]
    fn test_work_stealing_scheduler_steal_when_empty() {
        let sched = WorkStealingScheduler::new(2);

        // Worker 0 has tasks; worker 1 is empty.
        for _ in 0..4 {
            sched.spawn_task(0, 50).unwrap();
        }

        // Worker 1 should steal from worker 0.
        let stolen = sched.schedule(1);
        assert!(stolen.is_some(), "worker 1 should have stolen a task");

        let snap = sched.snapshot_metrics();
        assert!(snap.total_stolen >= 1);
    }

    #[test]
    fn test_work_stealing_scheduler_priority_order() {
        let sched = WorkStealingScheduler::new(1);

        sched.spawn_task(0, 10).unwrap();
        sched.spawn_task(0, 200).unwrap();
        sched.spawn_task(0, 50).unwrap();

        // First pop should yield priority 200.
        let first = sched.schedule(0).unwrap();
        assert_eq!(
            first.priority, 200,
            "highest priority must be scheduled first"
        );

        // Second pop should yield priority 50.
        let second = sched.schedule(0).unwrap();
        assert_eq!(second.priority, 50);

        // Third pop should yield priority 10.
        let third = sched.schedule(0).unwrap();
        assert_eq!(third.priority, 10);
    }

    #[test]
    fn test_work_stealing_scheduler_yield_requeueing() {
        let sched = WorkStealingScheduler::new(1);

        let h = sched.spawn_task(0, 100).unwrap();

        // Run it.
        let got = sched.schedule(0).unwrap();
        assert_eq!(got.id, h.id);

        // Yield it back.
        sched.yield_task_with_priority(0, 100);

        // Should be schedulable again.
        let again = sched.schedule(0);
        assert!(again.is_some());
        assert_eq!(again.unwrap().id, h.id);
    }

    #[test]
    fn test_work_stealing_scheduler_metrics() {
        let sched = WorkStealingScheduler::new(2);

        for _ in 0..5 {
            sched.spawn_task(0, 50).unwrap();
        }

        // Worker 0 pops its own tasks.
        for _ in 0..5 {
            sched.schedule(0);
        }

        // Worker 1 attempts steal on an empty deque.
        sched.schedule(1);

        let snap = sched.snapshot_metrics();
        assert_eq!(snap.total_scheduled, 5);
        assert!(snap.total_idle >= 1, "worker 1 should record idle");
        assert!(snap.peak_queue_depth >= 5);
    }

    #[test]
    fn test_work_stealing_scheduler_max_capacity() {
        // Spawn many tasks to exercise buffer growth past 256.
        let sched = WorkStealingScheduler::new(1);
        const N: usize = 512;
        let mut handles = alloc::vec::Vec::with_capacity(N);
        for _ in 0..N {
            handles.push(sched.spawn_task(0, 128).unwrap());
        }

        let snap = sched.snapshot_metrics();
        assert_eq!(snap.total_scheduled, N as u64);

        let mut count = 0;
        while sched.schedule(0).is_some() {
            count += 1;
        }
        assert_eq!(count, N);
    }

    #[test]
    fn test_work_stealing_scheduler_terminate_nonexistent() {
        let sched = WorkStealingScheduler::new(2);
        let fake_handle = TaskHandle {
            id: 99_999,
            priority: 50,
            worker_hint: 0,
        };

        let result = sched.terminate_task(0, fake_handle);
        assert!(result.is_err(), "terminating a non-current task must fail");
        assert!(matches!(
            result.unwrap_err(),
            KernelError::TaskNotFound { task_id: 99_999 }
        ));
    }

    #[test]
    fn test_work_stealing_concurrent_spawn_and_steal() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Barrier;

        const PRODUCERS: usize = 4;
        const TASKS_PER_PRODUCER: usize = 32;
        const TOTAL: u64 = (PRODUCERS * TASKS_PER_PRODUCER) as u64;

        let sched = Arc::new(WorkStealingScheduler::new(4));
        // Barrier: PRODUCERS producers + 4 consumers all start together.
        let barrier = Arc::new(Barrier::new(PRODUCERS + 4));
        // Signal set when all producers have finished enqueuing.
        let producers_done = Arc::new(AtomicBool::new(false));
        let producers_remaining = Arc::new(AtomicU64::new(PRODUCERS as u64));

        let mut prod_handles = alloc::vec::Vec::new();
        for p in 0..PRODUCERS {
            let s = Arc::clone(&sched);
            let b = Arc::clone(&barrier);
            let done = Arc::clone(&producers_done);
            let remaining = Arc::clone(&producers_remaining);
            prod_handles.push(thread::spawn(move || {
                b.wait();
                for _ in 0..TASKS_PER_PRODUCER {
                    s.spawn_task(p, (p * 10) as u8).unwrap();
                }
                // Signal when this producer has finished.
                if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
                    done.store(true, Ordering::Release);
                }
            }));
        }

        let consumed = Arc::new(AtomicU64::new(0));
        let mut cons_handles = alloc::vec::Vec::new();
        for c in 0..4 {
            let s = Arc::clone(&sched);
            let b = Arc::clone(&barrier);
            let con = Arc::clone(&consumed);
            let done = Arc::clone(&producers_done);
            cons_handles.push(thread::spawn(move || {
                b.wait();
                // Keep consuming until all producers are done AND the scheduler
                // is fully drained.  The two conditions must both be true to exit.
                loop {
                    match s.schedule(c) {
                        Some(_) => {
                            con.fetch_add(1, Ordering::Relaxed);
                        }
                        None => {
                            // Producers finished and scheduler is empty.
                            if done.load(Ordering::Acquire) && con.load(Ordering::Relaxed) >= TOTAL
                            {
                                break;
                            }
                            core::hint::spin_loop();
                        }
                    }
                }
            }));
        }

        for h in prod_handles {
            h.join().unwrap();
        }
        for h in cons_handles {
            h.join().unwrap();
        }

        assert_eq!(
            consumed.load(Ordering::Relaxed),
            TOTAL,
            "all tasks must be consumed exactly once"
        );
    }

    #[test]
    fn test_work_stealing_no_duplicate_schedule() {
        use std::sync::Mutex as StdMutex;

        const TASKS: usize = 64;
        let sched = Arc::new(WorkStealingScheduler::new(2));
        let seen = Arc::new(StdMutex::new(alloc::collections::BTreeSet::new()));

        for _ in 0..TASKS {
            sched.spawn_task(0, 100).unwrap();
        }

        let mut handles = alloc::vec::Vec::new();
        for w in 0..2 {
            let s = Arc::clone(&sched);
            let seen2 = Arc::clone(&seen);
            handles.push(thread::spawn(move || {
                let mut idle = 0;
                loop {
                    match s.schedule(w) {
                        Some(h) => {
                            idle = 0;
                            let mut g = seen2.lock().unwrap_or_else(|e| e.into_inner());
                            assert!(g.insert(h.id), "task {} scheduled twice", h.id);
                        }
                        None => {
                            idle += 1;
                            if idle > 1_000 {
                                break;
                            }
                        }
                    }
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let g = seen.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(g.len(), TASKS, "all tasks must be seen exactly once");
    }

    #[test]
    fn test_work_stealing_load_balance() {
        const TASKS: usize = 100;
        const WORKERS: usize = 4;

        let sched = WorkStealingScheduler::new(WORKERS);

        // Spawn all tasks on worker 0.
        for _ in 0..TASKS {
            sched.spawn_task(0, 100).unwrap();
        }

        // Each worker tries to schedule; workers 1-3 should steal.
        let mut per_worker = [0usize; WORKERS];
        let mut remaining = TASKS;
        while remaining > 0 {
            let mut progress = false;
            for (w, count) in per_worker.iter_mut().enumerate().take(WORKERS) {
                if sched.schedule(w).is_some() {
                    *count += 1;
                    remaining -= 1;
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }

        assert_eq!(remaining, 0, "all tasks must be scheduled");

        // At least 2 workers must have participated (load balance happened).
        let workers_with_work = per_worker.iter().filter(|&&c| c > 0).count();
        assert!(
            workers_with_work >= 2,
            "work should be distributed; per_worker={:?}",
            per_worker
        );
    }
}
