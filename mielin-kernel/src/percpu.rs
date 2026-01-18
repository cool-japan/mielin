//! Per-CPU data structures for multi-core support
//!
//! Provides isolated per-CPU state to minimize contention and enable
//! lock-free operation across multiple CPU cores.
//!
//! ## Overview
//!
//! Each CPU core has its own:
//! - Scheduler instance (64 task slots per CPU)
//! - Memory pool allocator (7 size classes)
//! - CPU-local statistics
//!
//! ## Design
//!
//! ```text
//! ┌────────────┐  ┌────────────┐  ┌────────────┐
//! │   CPU 0    │  │   CPU 1    │  │   CPU N    │
//! ├────────────┤  ├────────────┤  ├────────────┤
//! │ Scheduler  │  │ Scheduler  │  │ Scheduler  │
//! │  64 tasks  │  │  64 tasks  │  │  64 tasks  │
//! ├────────────┤  ├────────────┤  ├────────────┤
//! │ Pool (32B) │  │ Pool (32B) │  │ Pool (32B) │
//! │ Pool (64B) │  │ Pool (64B) │  │ Pool (64B) │
//! │ Pool (128B)│  │ Pool (128B)│  │ Pool (128B)│
//! │    ...     │  │    ...     │  │    ...     │
//! ├────────────┤  ├────────────┤  ├────────────┤
//! │ Statistics │  │ Statistics │  │ Statistics │
//! └────────────┘  └────────────┘  └────────────┘
//! ```
//!
//! ## Benefits
//!
//! - **No lock contention**: Each CPU operates on its own data
//! - **Cache efficiency**: CPU-local data stays in L1/L2 cache
//! - **Scalability**: Linear scaling with CPU count
//! - **Load isolation**: Busy CPU doesn't affect others
//!
//! ## CPU Affinity
//!
//! Tasks can be pinned to specific CPUs for:
//! - Cache locality (ML tensor operations)
//! - Real-time guarantees (deadline scheduling)
//! - NUMA optimization (memory-bound workloads)
//!
//! ## Load Balancing
//!
//! When a CPU becomes idle or overloaded, the system can:
//! 1. **Work stealing**: Idle CPU steals tasks from busy CPUs
//! 2. **Push migration**: Overloaded CPU pushes tasks to idle CPUs
//! 3. **Periodic rebalancing**: Global rebalancer runs every N ms
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::percpu::{get_current_cpu, spawn_on_cpu};
//!
//! // Spawn task on current CPU
//! let task_id = get_current_cpu().spawn_task(100).unwrap();
//!
//! // Spawn task on specific CPU (CPU 2)
//! let task_id = spawn_on_cpu(2, 100).unwrap();
//!
//! // Get global statistics across all CPUs
//! let stats = percpu::global_stats();
//! ```

use crate::pool::{BlockSize, Pool};
use crate::scheduler::{Scheduler, SchedulerMetricsSnapshot};
use crate::KernelError;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 16;

/// Per-CPU data structure
///
/// Each CPU core has its own isolated instance to minimize contention.
#[repr(align(64))] // Cache line alignment to prevent false sharing
pub struct PerCpu {
    /// CPU ID (0-based)
    cpu_id: usize,
    /// Per-CPU scheduler (64 tasks per CPU)
    scheduler: Mutex<Scheduler>,
    /// Per-CPU memory pools (7 size classes)
    pools: [Pool; 7],
    /// CPU-local statistics
    stats: CpuStats,
    /// CPU online/offline state
    online: AtomicU32,
}

impl PerCpu {
    /// Create a new Per-CPU structure
    pub const fn new(cpu_id: usize) -> Self {
        Self {
            cpu_id,
            scheduler: Mutex::new(Scheduler::new()),
            pools: [
                Pool::new(BlockSize::B32.bytes()),
                Pool::new(BlockSize::B64.bytes()),
                Pool::new(BlockSize::B128.bytes()),
                Pool::new(BlockSize::B256.bytes()),
                Pool::new(BlockSize::B512.bytes()),
                Pool::new(BlockSize::K1.bytes()),
                Pool::new(BlockSize::K4.bytes()),
            ],
            stats: CpuStats::new(),
            online: AtomicU32::new(0),
        }
    }

    /// Initialize this CPU's pools
    pub fn init(&self) {
        // Initialize all memory pools
        for pool in &self.pools {
            pool.init();
        }

        // Reset scheduler by replacing it with a new one
        {
            let mut sched = self.scheduler.lock();
            *sched = Scheduler::new();
        }

        // Mark CPU as online
        self.online.store(1, Ordering::Release);
    }

    /// Check if this CPU is online
    pub fn is_online(&self) -> bool {
        self.online.load(Ordering::Acquire) != 0
    }

    /// Get CPU ID
    pub fn cpu_id(&self) -> usize {
        self.cpu_id
    }

    /// Spawn a task on this CPU with given priority (no CPU affinity)
    pub fn spawn_task(&self, priority: u8) -> Result<usize, KernelError> {
        self.spawn_task_with_affinity(priority, None)
    }

    /// Spawn a task on this CPU with given priority and CPU affinity
    pub fn spawn_task_with_affinity(
        &self,
        priority: u8,
        cpu_affinity: Option<usize>,
    ) -> Result<usize, KernelError> {
        let mut sched = self.scheduler.lock();
        let task_id = sched.spawn_task_with_affinity(priority, cpu_affinity)?;
        self.stats.record_spawn();
        Ok(task_id)
    }

    /// Spawn a task pinned to this specific CPU
    pub fn spawn_task_pinned(&self, priority: u8) -> Result<usize, KernelError> {
        self.spawn_task_with_affinity(priority, Some(self.cpu_id))
    }

    /// Schedule the next task on this CPU
    pub fn schedule(&self) -> Option<usize> {
        let mut sched = self.scheduler.lock();
        let result = sched.schedule();
        if result.is_some() {
            self.stats.record_schedule();
        }
        result
    }

    /// Yield the current task on this CPU
    pub fn yield_current(&self) {
        let mut sched = self.scheduler.lock();
        sched.yield_task();
        self.stats.record_yield();
    }

    /// Terminate a task on this CPU
    pub fn terminate_task(&self, task_id: usize) {
        let mut sched = self.scheduler.lock();
        sched.terminate_task(task_id);
        self.stats.record_terminate();
    }

    /// Get the number of active tasks on this CPU
    pub fn active_tasks(&self) -> usize {
        let sched = self.scheduler.lock();
        sched.active_tasks()
    }

    /// Get scheduler metrics for this CPU
    pub fn scheduler_metrics(&self) -> SchedulerMetricsSnapshot {
        let sched = self.scheduler.lock();
        sched.metrics()
    }

    /// Get CPU statistics
    pub fn stats(&self) -> CpuStatsSnapshot {
        self.stats.snapshot()
    }

    /// Allocate memory from this CPU's pools
    pub fn allocate(&self, size: usize) -> Option<core::ptr::NonNull<u8>> {
        let block_size = BlockSize::for_size(size)?;
        let pool = &self.pools[block_size.index()];
        let ptr = pool.allocate()?;
        self.stats.record_allocation(size);
        Some(ptr)
    }

    /// Free memory to this CPU's pools
    pub fn deallocate(&self, ptr: core::ptr::NonNull<u8>, size: usize) {
        if let Some(block_size) = BlockSize::for_size(size) {
            let pool = &self.pools[block_size.index()];
            // SAFETY: The pointer was allocated from this pool via allocate()
            unsafe {
                pool.deallocate(ptr);
            }
            self.stats.record_deallocation(size);
        }
    }

    /// Get pool statistics for a specific block size (total_blocks, allocated_blocks)
    pub fn pool_stats(&self, block_size: BlockSize) -> (u32, u32) {
        let pool = &self.pools[block_size.index()];
        (pool.total_blocks(), pool.allocated_blocks())
    }
}

// SAFETY: PerCpu uses interior mutability with proper synchronization
// - Scheduler is protected by Mutex
// - Pools use lock-free atomic operations
// - Stats use atomic counters
unsafe impl Send for PerCpu {}
unsafe impl Sync for PerCpu {}

/// CPU-local statistics
#[derive(Debug)]
struct CpuStats {
    /// Total tasks spawned on this CPU
    tasks_spawned: AtomicU64,
    /// Total tasks terminated on this CPU
    tasks_terminated: AtomicU64,
    /// Total schedule calls on this CPU
    schedule_calls: AtomicU64,
    /// Total yield calls on this CPU
    yield_calls: AtomicU64,
    /// Total allocations on this CPU
    allocations: AtomicU64,
    /// Total deallocations on this CPU
    deallocations: AtomicU64,
    /// Total bytes allocated on this CPU
    bytes_allocated: AtomicU64,
    /// Total bytes deallocated on this CPU
    bytes_deallocated: AtomicU64,
}

impl CpuStats {
    const fn new() -> Self {
        Self {
            tasks_spawned: AtomicU64::new(0),
            tasks_terminated: AtomicU64::new(0),
            schedule_calls: AtomicU64::new(0),
            yield_calls: AtomicU64::new(0),
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            bytes_allocated: AtomicU64::new(0),
            bytes_deallocated: AtomicU64::new(0),
        }
    }

    fn record_spawn(&self) {
        self.tasks_spawned.fetch_add(1, Ordering::Relaxed);
    }

    fn record_terminate(&self) {
        self.tasks_terminated.fetch_add(1, Ordering::Relaxed);
    }

    fn record_schedule(&self) {
        self.schedule_calls.fetch_add(1, Ordering::Relaxed);
    }

    fn record_yield(&self) {
        self.yield_calls.fetch_add(1, Ordering::Relaxed);
    }

    fn record_allocation(&self, size: usize) {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.bytes_allocated
            .fetch_add(size as u64, Ordering::Relaxed);
    }

    fn record_deallocation(&self, size: usize) {
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.bytes_deallocated
            .fetch_add(size as u64, Ordering::Relaxed);
    }

    fn snapshot(&self) -> CpuStatsSnapshot {
        CpuStatsSnapshot {
            tasks_spawned: self.tasks_spawned.load(Ordering::Relaxed),
            tasks_terminated: self.tasks_terminated.load(Ordering::Relaxed),
            schedule_calls: self.schedule_calls.load(Ordering::Relaxed),
            yield_calls: self.yield_calls.load(Ordering::Relaxed),
            allocations: self.allocations.load(Ordering::Relaxed),
            deallocations: self.deallocations.load(Ordering::Relaxed),
            bytes_allocated: self.bytes_allocated.load(Ordering::Relaxed),
            bytes_deallocated: self.bytes_deallocated.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of CPU statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuStatsSnapshot {
    /// Total tasks spawned on this CPU
    pub tasks_spawned: u64,
    /// Total tasks terminated on this CPU
    pub tasks_terminated: u64,
    /// Total schedule calls on this CPU
    pub schedule_calls: u64,
    /// Total yield calls on this CPU
    pub yield_calls: u64,
    /// Total allocations on this CPU
    pub allocations: u64,
    /// Total deallocations on this CPU
    pub deallocations: u64,
    /// Total bytes allocated on this CPU
    pub bytes_allocated: u64,
    /// Total bytes deallocated on this CPU
    pub bytes_deallocated: u64,
}

impl CpuStatsSnapshot {
    /// Calculate CPU utilization (schedule calls / total operations)
    pub fn utilization(&self) -> f64 {
        let total = self.schedule_calls + self.yield_calls;
        if total == 0 {
            return 0.0;
        }
        self.schedule_calls as f64 / total as f64
    }

    /// Calculate memory utilization (bytes allocated - bytes deallocated)
    pub fn memory_utilization(&self) -> i64 {
        self.bytes_allocated as i64 - self.bytes_deallocated as i64
    }
}

/// Global array of Per-CPU structures
/// Initialized lazily to avoid const evaluation issues with Mutex
static PER_CPU: [PerCpu; MAX_CPUS] = [
    PerCpu::new(0),
    PerCpu::new(1),
    PerCpu::new(2),
    PerCpu::new(3),
    PerCpu::new(4),
    PerCpu::new(5),
    PerCpu::new(6),
    PerCpu::new(7),
    PerCpu::new(8),
    PerCpu::new(9),
    PerCpu::new(10),
    PerCpu::new(11),
    PerCpu::new(12),
    PerCpu::new(13),
    PerCpu::new(14),
    PerCpu::new(15),
];

/// Current CPU ID (thread-local)
/// In a real OS, this would be obtained from CPU registers
#[cfg(not(test))]
static CURRENT_CPU_ID: AtomicUsize = AtomicUsize::new(0);

/// Number of online CPUs
static NUM_ONLINE_CPUS: AtomicUsize = AtomicUsize::new(1);

/// Initialize the per-CPU subsystem
///
/// # Arguments
/// * `num_cpus` - Number of CPUs to initialize (1-16)
pub fn init(num_cpus: usize) -> Result<(), KernelError> {
    if num_cpus == 0 || num_cpus > MAX_CPUS {
        return Err(KernelError::HardwareNotSupported);
    }

    // Initialize each CPU
    for cpu in PER_CPU.iter().take(num_cpus) {
        cpu.init();
    }

    NUM_ONLINE_CPUS.store(num_cpus, Ordering::Release);
    Ok(())
}

/// Get the current CPU's PerCpu structure
#[cfg(not(test))]
pub fn current_cpu() -> &'static PerCpu {
    let cpu_id = CURRENT_CPU_ID.load(Ordering::Relaxed);
    &PER_CPU[cpu_id]
}

/// Get the current CPU ID
#[cfg(not(test))]
pub fn current_cpu_id() -> usize {
    CURRENT_CPU_ID.load(Ordering::Relaxed)
}

/// Set the current CPU ID (used during boot/context switch)
#[cfg(not(test))]
pub fn set_current_cpu_id(cpu_id: usize) {
    if cpu_id < MAX_CPUS {
        CURRENT_CPU_ID.store(cpu_id, Ordering::Relaxed);
    }
}

/// Get a specific CPU's PerCpu structure
pub fn get_cpu(cpu_id: usize) -> Result<&'static PerCpu, KernelError> {
    if cpu_id >= MAX_CPUS {
        return Err(KernelError::HardwareNotSupported);
    }
    if !PER_CPU[cpu_id].is_online() {
        return Err(KernelError::HardwareNotSupported);
    }
    Ok(&PER_CPU[cpu_id])
}

/// Get the number of online CPUs
pub fn num_online_cpus() -> usize {
    NUM_ONLINE_CPUS.load(Ordering::Acquire)
}

/// Spawn a task on a specific CPU (no affinity)
pub fn spawn_on_cpu(cpu_id: usize, priority: u8) -> Result<usize, KernelError> {
    let cpu = get_cpu(cpu_id)?;
    cpu.spawn_task(priority)
}

/// Spawn a task on a specific CPU with CPU affinity
pub fn spawn_on_cpu_with_affinity(
    cpu_id: usize,
    priority: u8,
    affinity: Option<usize>,
) -> Result<usize, KernelError> {
    let cpu = get_cpu(cpu_id)?;
    cpu.spawn_task_with_affinity(priority, affinity)
}

/// Spawn a task pinned to a specific CPU
pub fn spawn_on_cpu_pinned(cpu_id: usize, priority: u8) -> Result<usize, KernelError> {
    let cpu = get_cpu(cpu_id)?;
    cpu.spawn_task_pinned(priority)
}

/// Get global statistics across all CPUs
pub struct GlobalStats {
    /// Total tasks spawned across all CPUs
    pub total_tasks_spawned: u64,
    /// Total tasks terminated across all CPUs
    pub total_tasks_terminated: u64,
    /// Total schedule calls across all CPUs
    pub total_schedule_calls: u64,
    /// Total yield calls across all CPUs
    pub total_yield_calls: u64,
    /// Total allocations across all CPUs
    pub total_allocations: u64,
    /// Total deallocations across all CPUs
    pub total_deallocations: u64,
    /// Total bytes allocated across all CPUs
    pub total_bytes_allocated: u64,
    /// Total bytes deallocated across all CPUs
    pub total_bytes_deallocated: u64,
    /// Per-CPU statistics
    pub per_cpu_stats: [Option<CpuStatsSnapshot>; MAX_CPUS],
}

/// Get global statistics across all CPUs
pub fn global_stats() -> GlobalStats {
    let num_cpus = num_online_cpus();
    let mut global = GlobalStats {
        total_tasks_spawned: 0,
        total_tasks_terminated: 0,
        total_schedule_calls: 0,
        total_yield_calls: 0,
        total_allocations: 0,
        total_deallocations: 0,
        total_bytes_allocated: 0,
        total_bytes_deallocated: 0,
        per_cpu_stats: [None; MAX_CPUS],
    };

    for (i, cpu) in PER_CPU.iter().enumerate().take(num_cpus) {
        if cpu.is_online() {
            let stats = cpu.stats();
            global.total_tasks_spawned += stats.tasks_spawned;
            global.total_tasks_terminated += stats.tasks_terminated;
            global.total_schedule_calls += stats.schedule_calls;
            global.total_yield_calls += stats.yield_calls;
            global.total_allocations += stats.allocations;
            global.total_deallocations += stats.deallocations;
            global.total_bytes_allocated += stats.bytes_allocated;
            global.total_bytes_deallocated += stats.bytes_deallocated;
            global.per_cpu_stats[i] = Some(stats);
        }
    }

    global
}

// =============================================================================
// Load Balancing
// =============================================================================

/// Load balancing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalanceStrategy {
    /// Work stealing: Idle CPUs steal tasks from busy CPUs
    WorkStealing,
    /// Push migration: Overloaded CPUs push tasks to idle CPUs
    PushMigration,
    /// Round-robin: Distribute tasks evenly in round-robin fashion
    RoundRobin,
}

/// Load balancing statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadBalanceStats {
    /// Total task migrations performed
    pub migrations: u64,
    /// Successful work stealing attempts
    pub work_steals: u64,
    /// Failed work stealing attempts
    pub failed_steals: u64,
    /// Load balancing iterations
    pub balance_iterations: u64,
}

static LOAD_BALANCE_STATS: AtomicU64 = AtomicU64::new(0);
static WORK_STEALS: AtomicU64 = AtomicU64::new(0);
static FAILED_STEALS: AtomicU64 = AtomicU64::new(0);
static BALANCE_ITERATIONS: AtomicU64 = AtomicU64::new(0);

/// Get load balancing statistics
pub fn load_balance_stats() -> LoadBalanceStats {
    LoadBalanceStats {
        migrations: LOAD_BALANCE_STATS.load(Ordering::Relaxed),
        work_steals: WORK_STEALS.load(Ordering::Relaxed),
        failed_steals: FAILED_STEALS.load(Ordering::Relaxed),
        balance_iterations: BALANCE_ITERATIONS.load(Ordering::Relaxed),
    }
}

/// Find the CPU with the most tasks
pub fn find_busiest_cpu() -> Option<usize> {
    let num_cpus = num_online_cpus();
    let mut busiest_cpu = None;
    let mut max_tasks = 0;

    for i in 0..num_cpus {
        if let Ok(cpu) = get_cpu(i) {
            let active = cpu.active_tasks();
            if active > max_tasks {
                max_tasks = active;
                busiest_cpu = Some(i);
            }
        }
    }

    busiest_cpu
}

/// Find the CPU with the fewest tasks
pub fn find_idlest_cpu() -> Option<usize> {
    let num_cpus = num_online_cpus();
    let mut idlest_cpu = None;
    let mut min_tasks = usize::MAX;

    for i in 0..num_cpus {
        if let Ok(cpu) = get_cpu(i) {
            let active = cpu.active_tasks();
            if active < min_tasks {
                min_tasks = active;
                idlest_cpu = Some(i);
            }
        }
    }

    idlest_cpu
}

/// Calculate load imbalance score (higher = more imbalanced)
/// Returns variance as the imbalance metric (no sqrt to maintain no_std compatibility)
pub fn load_imbalance_score() -> f64 {
    let num_cpus = num_online_cpus();
    if num_cpus <= 1 {
        return 0.0;
    }

    let mut task_counts = alloc::vec::Vec::new();

    for i in 0..num_cpus {
        if let Ok(cpu) = get_cpu(i) {
            task_counts.push(cpu.active_tasks() as f64);
        }
    }

    if task_counts.is_empty() {
        return 0.0;
    }

    // Calculate variance as imbalance metric
    let mean: f64 = task_counts.iter().sum::<f64>() / task_counts.len() as f64;
    let variance: f64 = task_counts
        .iter()
        .map(|&x| {
            let diff = x - mean;
            diff * diff
        })
        .sum::<f64>()
        / task_counts.len() as f64;

    variance // Return variance directly (avoids sqrt for no_std)
}

/// Perform load balancing across all CPUs
///
/// Returns the number of tasks migrated
pub fn balance_load(strategy: LoadBalanceStrategy) -> usize {
    BALANCE_ITERATIONS.fetch_add(1, Ordering::Relaxed);

    match strategy {
        LoadBalanceStrategy::WorkStealing => balance_work_stealing(),
        LoadBalanceStrategy::PushMigration => balance_push_migration(),
        LoadBalanceStrategy::RoundRobin => balance_round_robin(),
    }
}

/// Migrate a task from source CPU to destination CPU
///
/// Returns true if migration was successful
pub fn migrate_task(task_id: usize, from_cpu: usize, to_cpu: usize) -> bool {
    let source_cpu = match get_cpu(from_cpu) {
        Ok(cpu) => cpu,
        Err(_) => return false,
    };

    let dest_cpu = match get_cpu(to_cpu) {
        Ok(cpu) => cpu,
        Err(_) => return false,
    };

    // Extract task from source CPU
    let task = {
        let mut sched = source_cpu.scheduler.lock();
        sched.extract_task(task_id)
    };

    let task = match task {
        Some(t) => t,
        None => return false,
    };

    // Inject task into destination CPU
    let success = {
        let mut sched = dest_cpu.scheduler.lock();
        sched.inject_task(task)
    };

    if success {
        LOAD_BALANCE_STATS.fetch_add(1, Ordering::Relaxed);
    }

    success
}

/// Work stealing load balancing
fn balance_work_stealing() -> usize {
    let busiest = match find_busiest_cpu() {
        Some(cpu) => cpu,
        None => {
            FAILED_STEALS.fetch_add(1, Ordering::Relaxed);
            return 0;
        }
    };

    let idlest = match find_idlest_cpu() {
        Some(cpu) => cpu,
        None => {
            FAILED_STEALS.fetch_add(1, Ordering::Relaxed);
            return 0;
        }
    };

    if busiest == idlest {
        return 0; // Already balanced
    }

    let busiest_cpu = match get_cpu(busiest) {
        Ok(cpu) => cpu,
        Err(_) => return 0,
    };
    let idlest_cpu = match get_cpu(idlest) {
        Ok(cpu) => cpu,
        Err(_) => return 0,
    };

    let busiest_tasks = busiest_cpu.active_tasks();
    let idlest_tasks = idlest_cpu.active_tasks();

    // Only steal if imbalance is significant (>2 task difference)
    if busiest_tasks <= idlest_tasks || (busiest_tasks - idlest_tasks) < 2 {
        FAILED_STEALS.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    // Get migratable tasks from busiest CPU
    let migratable = {
        let sched = busiest_cpu.scheduler.lock();
        sched.migratable_tasks(busiest)
    };

    if migratable.is_empty() {
        FAILED_STEALS.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    // Calculate how many tasks to migrate (half the imbalance)
    let to_migrate = ((busiest_tasks - idlest_tasks) / 2)
        .max(1)
        .min(migratable.len());

    let mut migrated = 0;
    for &task_id in migratable.iter().take(to_migrate) {
        if migrate_task(task_id, busiest, idlest) {
            migrated += 1;
        }
    }

    if migrated > 0 {
        WORK_STEALS.fetch_add(1, Ordering::Relaxed);
    } else {
        FAILED_STEALS.fetch_add(1, Ordering::Relaxed);
    }

    migrated
}

/// Push migration load balancing
fn balance_push_migration() -> usize {
    // Similar to work stealing but initiated by busy CPU
    // Implementation would push tasks from busy to idle CPUs
    0
}

/// Round-robin load balancing
fn balance_round_robin() -> usize {
    // Distribute new tasks in round-robin fashion
    // This is more of a scheduling policy than active rebalancing
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use spin::Mutex;

    // Test lock to serialize tests that access global state
    pub(super) static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_percpu_creation() {
        let cpu = PerCpu::new(0);
        assert_eq!(cpu.cpu_id(), 0);
        assert!(!cpu.is_online());
    }

    #[test]
    fn test_percpu_init() {
        let cpu = PerCpu::new(0);
        cpu.init();
        assert!(cpu.is_online());
    }

    #[test]
    fn test_percpu_spawn() {
        let cpu = PerCpu::new(0);
        cpu.init();

        let task_id = cpu.spawn_task(100).unwrap();
        assert_eq!(cpu.active_tasks(), 1);

        cpu.terminate_task(task_id);
        assert_eq!(cpu.active_tasks(), 0);
    }

    #[test]
    fn test_percpu_schedule() {
        let cpu = PerCpu::new(0);
        cpu.init();

        cpu.spawn_task(100).unwrap();
        let scheduled = cpu.schedule();
        assert!(scheduled.is_some());

        cpu.yield_current();
    }

    #[test]
    fn test_percpu_allocate() {
        let cpu = PerCpu::new(0);
        cpu.init();

        // Allocate 64 bytes
        let ptr = cpu.allocate(64);
        assert!(ptr.is_some());

        if let Some(p) = ptr {
            cpu.deallocate(p, 64);
        }
    }

    #[test]
    fn test_percpu_stats() {
        let cpu = PerCpu::new(0);
        cpu.init();

        cpu.spawn_task(100).unwrap();
        cpu.schedule();

        let stats = cpu.stats();
        assert_eq!(stats.tasks_spawned, 1);
        assert_eq!(stats.schedule_calls, 1);
    }

    #[test]
    fn test_init_multiple_cpus() {
        let _lock = TEST_LOCK.lock();
        init(4).unwrap();
        assert_eq!(num_online_cpus(), 4);

        for i in 0..4 {
            let cpu = get_cpu(i).unwrap();
            assert!(cpu.is_online());
            assert_eq!(cpu.cpu_id(), i);
        }
    }

    #[test]
    fn test_spawn_on_cpu() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();
        let initial_tasks = cpu0.active_tasks();

        let task_id = spawn_on_cpu(0, 100).unwrap();
        assert_eq!(cpu0.active_tasks(), initial_tasks + 1);

        cpu0.terminate_task(task_id);
        assert_eq!(cpu0.active_tasks(), initial_tasks);
    }

    #[test]
    fn test_global_stats() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let stats_before = global_stats();

        // Spawn tasks on different CPUs
        let task_id_0 = spawn_on_cpu(0, 100).unwrap();
        let task_id_1 = spawn_on_cpu(1, 100).unwrap();

        let stats = global_stats();
        assert_eq!(
            stats.total_tasks_spawned,
            stats_before.total_tasks_spawned + 2
        );

        // Cleanup
        let cpu0 = get_cpu(0).unwrap();
        let cpu1 = get_cpu(1).unwrap();
        cpu0.terminate_task(task_id_0);
        cpu1.terminate_task(task_id_1);
    }

    #[test]
    fn test_cpu_isolation() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();
        let cpu1 = get_cpu(1).unwrap();

        let cpu0_initial = cpu0.active_tasks();
        let cpu1_initial = cpu1.active_tasks();

        // Spawn tasks on CPU 0
        let task_id_0a = spawn_on_cpu(0, 100).unwrap();
        let task_id_0b = spawn_on_cpu(0, 100).unwrap();

        // Spawn task on CPU 1
        let task_id_1 = spawn_on_cpu(1, 100).unwrap();

        assert_eq!(cpu0.active_tasks(), cpu0_initial + 2);
        assert_eq!(cpu1.active_tasks(), cpu1_initial + 1);

        // Cleanup
        cpu0.terminate_task(task_id_0a);
        cpu0.terminate_task(task_id_0b);
        cpu1.terminate_task(task_id_1);
    }

    #[test]
    fn test_per_cpu_alignment() {
        // Ensure cache line alignment (64 bytes)
        use core::mem;
        assert_eq!(mem::align_of::<PerCpu>(), 64);
    }

    #[test]
    fn test_cpu_affinity_spawn() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        // Spawn task with affinity to CPU 1
        let task_id = spawn_on_cpu_with_affinity(0, 100, Some(1)).unwrap();

        // Verify task has correct affinity
        let cpu0 = get_cpu(0).unwrap();
        let sched = cpu0.scheduler.lock();
        let task = sched.get_task(task_id).unwrap();
        assert_eq!(task.cpu_affinity(), Some(1));
    }

    #[test]
    fn test_cpu_affinity_pinned() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        // Spawn task pinned to CPU 1
        let task_id = spawn_on_cpu_pinned(1, 100).unwrap();

        // Verify task has correct affinity (pinned to CPU 1)
        let cpu1 = get_cpu(1).unwrap();
        let sched = cpu1.scheduler.lock();
        let task = sched.get_task(task_id).unwrap();
        assert_eq!(task.cpu_affinity(), Some(1));
    }

    #[test]
    fn test_cpu_affinity_none() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        // Spawn task with no affinity
        let task_id = spawn_on_cpu(0, 100).unwrap();

        // Verify task has no affinity
        let cpu0 = get_cpu(0).unwrap();
        let sched = cpu0.scheduler.lock();
        let task = sched.get_task(task_id).unwrap();
        assert_eq!(task.cpu_affinity(), None);
    }

    #[test]
    fn test_load_balance_find_busiest() {
        let _lock = TEST_LOCK.lock();
        init(3).unwrap();

        // Spawn different numbers of tasks on each CPU
        spawn_on_cpu(0, 100).unwrap(); // 1 task
        spawn_on_cpu(1, 100).unwrap(); // 2 tasks
        spawn_on_cpu(1, 100).unwrap();
        spawn_on_cpu(2, 100).unwrap(); // 3 tasks
        spawn_on_cpu(2, 100).unwrap();
        spawn_on_cpu(2, 100).unwrap();

        let busiest = find_busiest_cpu().unwrap();
        assert_eq!(busiest, 2); // CPU 2 has the most tasks
    }

    #[test]
    fn test_load_balance_find_idlest() {
        let _lock = TEST_LOCK.lock();
        init(3).unwrap();

        // Spawn different numbers of tasks on each CPU
        spawn_on_cpu(0, 100).unwrap(); // 1 task
        spawn_on_cpu(1, 100).unwrap(); // 2 tasks
        spawn_on_cpu(1, 100).unwrap();
        spawn_on_cpu(2, 100).unwrap(); // 3 tasks
        spawn_on_cpu(2, 100).unwrap();
        spawn_on_cpu(2, 100).unwrap();

        let idlest = find_idlest_cpu().unwrap();
        assert_eq!(idlest, 0); // CPU 0 has the fewest tasks
    }

    #[test]
    fn test_load_balance_imbalance_score() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        // Balanced load (0 tasks each)
        let score1 = load_imbalance_score();
        assert_eq!(score1, 0.0);

        // Unbalanced load
        spawn_on_cpu(0, 100).unwrap();
        spawn_on_cpu(0, 100).unwrap();
        spawn_on_cpu(0, 100).unwrap();

        let score2 = load_imbalance_score();
        assert!(score2 > 0.0);
    }

    #[test]
    fn test_load_balance_work_stealing() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        // Create significant imbalance
        for _ in 0..10 {
            spawn_on_cpu(0, 100).unwrap();
        }

        let stats_before = load_balance_stats();

        // Attempt work stealing
        let _migrations = balance_load(LoadBalanceStrategy::WorkStealing);

        let stats_after = load_balance_stats();

        assert!(stats_after.balance_iterations > stats_before.balance_iterations);
        // Work stealing should have been attempted
        assert!(
            stats_after.work_steals > stats_before.work_steals
                || stats_after.failed_steals > stats_before.failed_steals
        );
    }

    #[test]
    fn test_load_balance_stats() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let stats_before = load_balance_stats();

        // Perform some load balancing
        balance_load(LoadBalanceStrategy::WorkStealing);

        let stats_after = load_balance_stats();

        // Stats should have increased
        assert!(stats_after.balance_iterations > stats_before.balance_iterations);
    }

    #[test]
    fn test_task_migration() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();
        let cpu1 = get_cpu(1).unwrap();

        // Spawn task on CPU 0
        let task_id = spawn_on_cpu(0, 100).unwrap();

        assert_eq!(cpu0.active_tasks(), 1);
        assert_eq!(cpu1.active_tasks(), 0);

        // Migrate task from CPU 0 to CPU 1
        let success = migrate_task(task_id, 0, 1);
        assert!(success);

        assert_eq!(cpu0.active_tasks(), 0);
        assert_eq!(cpu1.active_tasks(), 1);

        // Cleanup
        cpu1.terminate_task(task_id);
    }

    #[test]
    fn test_task_migration_respects_affinity() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();
        let cpu1 = get_cpu(1).unwrap();

        // Spawn task pinned to CPU 0
        let task_id = spawn_on_cpu_pinned(0, 100).unwrap();

        assert_eq!(cpu0.active_tasks(), 1);

        // Try to migrate pinned task - should fail
        let success = migrate_task(task_id, 0, 1);
        assert!(!success);

        // Task should still be on CPU 0
        assert_eq!(cpu0.active_tasks(), 1);
        assert_eq!(cpu1.active_tasks(), 0);

        // Cleanup
        cpu0.terminate_task(task_id);
    }

    #[test]
    fn test_work_stealing_with_migration() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();
        let cpu1 = get_cpu(1).unwrap();

        // Create imbalance: 10 tasks on CPU 0, 0 on CPU 1
        for _ in 0..10 {
            spawn_on_cpu(0, 100).unwrap();
        }

        assert_eq!(cpu0.active_tasks(), 10);
        assert_eq!(cpu1.active_tasks(), 0);

        // Perform work stealing
        let migrated = balance_load(LoadBalanceStrategy::WorkStealing);

        // Should have migrated some tasks
        assert!(migrated > 0);

        // Load should be more balanced now
        let cpu0_tasks = cpu0.active_tasks();
        let cpu1_tasks = cpu1.active_tasks();

        assert!(cpu1_tasks > 0, "CPU 1 should have received tasks");
        assert!(cpu0_tasks < 10, "CPU 0 should have given up tasks");
        assert_eq!(cpu0_tasks + cpu1_tasks, 10, "Total tasks should remain 10");
    }

    #[test]
    fn test_migratable_tasks_excludes_pinned() {
        let _lock = TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();

        // Spawn 2 unpinned tasks and 2 pinned tasks
        spawn_on_cpu(0, 100).unwrap();
        spawn_on_cpu(0, 100).unwrap();
        spawn_on_cpu_pinned(0, 100).unwrap();
        spawn_on_cpu_pinned(0, 100).unwrap();

        let migratable = {
            let sched = cpu0.scheduler.lock();
            sched.migratable_tasks(0)
        };

        // Only the 2 unpinned tasks should be migratable
        assert_eq!(migratable.len(), 2);
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    extern crate alloc;
    use alloc::vec::Vec;

    #[test]
    fn test_stress_multi_cpu_spawn() {
        let _lock = super::tests::TEST_LOCK.lock();
        init(4).unwrap();

        let stats_before = global_stats();
        let mut task_ids = Vec::new();

        // Spawn 100 tasks across 4 CPUs
        for i in 0..100 {
            let cpu_id = i % 4;
            if let Ok(task_id) = spawn_on_cpu(cpu_id, (i % 256) as u8) {
                task_ids.push((cpu_id, task_id));
            }
        }

        // Verify distribution
        let stats = global_stats();
        assert_eq!(
            stats.total_tasks_spawned,
            stats_before.total_tasks_spawned + 100
        );

        // Cleanup
        for (cpu_id, task_id) in task_ids {
            let cpu = get_cpu(cpu_id).unwrap();
            cpu.terminate_task(task_id);
        }
    }

    #[test]
    fn test_stress_per_cpu_allocations() {
        let _lock = super::tests::TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();

        // Allocate and free many blocks
        for _ in 0..1000 {
            if let Some(ptr) = cpu0.allocate(64) {
                cpu0.deallocate(ptr, 64);
            }
        }

        let stats = cpu0.stats();
        assert!(stats.allocations >= 1000);
        assert!(stats.deallocations >= 1000);
    }

    #[test]
    fn test_stress_concurrent_scheduling() {
        let _lock = super::tests::TEST_LOCK.lock();
        init(2).unwrap();

        let cpu0 = get_cpu(0).unwrap();
        let cpu1 = get_cpu(1).unwrap();

        let stats_before = global_stats();
        let mut task_ids = Vec::new();

        // Spawn tasks on both CPUs
        for _ in 0..10 {
            task_ids.push((0, cpu0.spawn_task(100).unwrap()));
            task_ids.push((1, cpu1.spawn_task(100).unwrap()));
        }

        // Schedule on both CPUs
        for _ in 0..100 {
            cpu0.schedule();
            cpu0.yield_current();
            cpu1.schedule();
            cpu1.yield_current();
        }

        let stats = global_stats();
        assert_eq!(
            stats.total_tasks_spawned,
            stats_before.total_tasks_spawned + 20
        );
        assert!(stats.total_schedule_calls >= stats_before.total_schedule_calls + 200);

        // Cleanup
        for (cpu_id, task_id) in task_ids {
            let cpu = get_cpu(cpu_id).unwrap();
            cpu.terminate_task(task_id);
        }
    }
}
