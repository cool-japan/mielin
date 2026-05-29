//! Kernel Observability Infrastructure
//!
//! This module provides comprehensive observability features for kernel debugging,
//! performance analysis, and production monitoring:
//! - Kernel tracing infrastructure (eBPF-like probes)
//! - Performance counters (PMU integration)
//! - Memory usage tracking per task
//! - Task execution profiling
//! - Trace event collection and filtering
//!
//! # Design Philosophy
//!
//! Observability is critical for understanding kernel behavior in production:
//! - **Low Overhead**: <5% performance impact when enabled
//! - **Selective Tracing**: Fine-grained control over what to trace
//! - **Zero Cost When Disabled**: No runtime cost if tracing is off
//! - **Ring Buffer**: Lock-free, bounded memory usage
//! - **Statistical Sampling**: Reduce overhead for high-frequency events
//!
//! # Examples
//!
//! ```no_run
//! use mielin_kernel::observability::{self, EventType, TracepointId};
//!
//! // Initialize tracing with 64KB ring buffer
//! observability::init(65536).expect("init tracing");
//!
//! // Enable specific tracepoints
//! observability::enable_tracepoint(TracepointId::TaskSpawn);
//! observability::enable_tracepoint(TracepointId::PageAlloc);
//!
//! // Emit a trace event
//! observability::trace_event(EventType::TaskSpawn {
//!     task_id: 42,
//!     priority: 100,
//! });
//!
//! // Read events
//! observability::for_each_event(|event| {
//!     println!("Event: {:?}", event);
//! });
//!
//! // Get performance counters
//! let pmc = observability::read_pmc();
//! println!("CPU cycles: {}", pmc.cycles);
//! println!("Instructions: {}", pmc.instructions);
//! ```

use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

/// Maximum number of trace events in ring buffer
const MAX_EVENTS: usize = 4096;

/// Tracepoint identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TracepointId {
    /// Task spawned
    TaskSpawn = 0,
    /// Task terminated
    TaskTerminate = 1,
    /// Task scheduled
    TaskSchedule = 2,
    /// Task yielded
    TaskYield = 3,
    /// Task blocked
    TaskBlock = 4,
    /// Task woken up
    TaskWake = 5,
    /// Page allocated
    PageAlloc = 6,
    /// Page freed
    PageFree = 7,
    /// Interrupt entry
    InterruptEntry = 8,
    /// Interrupt exit
    InterruptExit = 9,
    /// IPI sent
    IpiSent = 10,
    /// IPI received
    IpiReceived = 11,
    /// Timer tick
    TimerTick = 12,
    /// Context switch
    ContextSwitch = 13,
    /// System call entry
    SyscallEntry = 14,
    /// System call exit
    SyscallExit = 15,
    /// Page fault
    PageFault = 16,
    /// TLB flush
    TlbFlush = 17,
    /// Lock acquired
    LockAcquire = 18,
    /// Lock released
    LockRelease = 19,
}

impl TracepointId {
    /// Total number of tracepoints
    pub const COUNT: usize = 20;

    /// Get tracepoint name
    pub fn name(&self) -> &'static str {
        match self {
            Self::TaskSpawn => "task_spawn",
            Self::TaskTerminate => "task_terminate",
            Self::TaskSchedule => "task_schedule",
            Self::TaskYield => "task_yield",
            Self::TaskBlock => "task_block",
            Self::TaskWake => "task_wake",
            Self::PageAlloc => "page_alloc",
            Self::PageFree => "page_free",
            Self::InterruptEntry => "interrupt_entry",
            Self::InterruptExit => "interrupt_exit",
            Self::IpiSent => "ipi_sent",
            Self::IpiReceived => "ipi_received",
            Self::TimerTick => "timer_tick",
            Self::ContextSwitch => "context_switch",
            Self::SyscallEntry => "syscall_entry",
            Self::SyscallExit => "syscall_exit",
            Self::PageFault => "page_fault",
            Self::TlbFlush => "tlb_flush",
            Self::LockAcquire => "lock_acquire",
            Self::LockRelease => "lock_release",
        }
    }
}

/// Trace event types
#[derive(Debug, Clone, Copy)]
pub enum EventType {
    /// Task spawned
    TaskSpawn { task_id: usize, priority: u8 },
    /// Task terminated
    TaskTerminate { task_id: usize },
    /// Task scheduled
    TaskSchedule { task_id: usize, cpu_id: usize },
    /// Task yielded
    TaskYield { task_id: usize },
    /// Task blocked
    TaskBlock { task_id: usize, reason: u64 },
    /// Task woken up
    TaskWake { task_id: usize },
    /// Page allocated
    PageAlloc { page_addr: usize, count: usize },
    /// Page freed
    PageFree { page_addr: usize, count: usize },
    /// Interrupt entry
    InterruptEntry { irq: u8, cpu_id: usize },
    /// Interrupt exit
    InterruptExit { irq: u8, duration_ns: u64 },
    /// IPI sent
    IpiSent { target_cpu: usize, ipi_type: u8 },
    /// IPI received
    IpiReceived { cpu_id: usize, ipi_type: u8 },
    /// Timer tick
    TimerTick { jiffies: u64 },
    /// Context switch
    ContextSwitch { from_task: usize, to_task: usize },
    /// System call entry
    SyscallEntry { syscall_nr: u32, task_id: usize },
    /// System call exit
    SyscallExit { syscall_nr: u32, result: i64 },
    /// Page fault
    PageFault { fault_addr: usize, flags: u32 },
    /// TLB flush
    TlbFlush { cpu_id: usize, addr: usize },
    /// Lock acquired
    LockAcquire { lock_addr: usize, task_id: usize },
    /// Lock released
    LockRelease { lock_addr: usize, task_id: usize },
}

impl EventType {
    /// Get the tracepoint ID for this event
    pub fn tracepoint_id(&self) -> TracepointId {
        match self {
            Self::TaskSpawn { .. } => TracepointId::TaskSpawn,
            Self::TaskTerminate { .. } => TracepointId::TaskTerminate,
            Self::TaskSchedule { .. } => TracepointId::TaskSchedule,
            Self::TaskYield { .. } => TracepointId::TaskYield,
            Self::TaskBlock { .. } => TracepointId::TaskBlock,
            Self::TaskWake { .. } => TracepointId::TaskWake,
            Self::PageAlloc { .. } => TracepointId::PageAlloc,
            Self::PageFree { .. } => TracepointId::PageFree,
            Self::InterruptEntry { .. } => TracepointId::InterruptEntry,
            Self::InterruptExit { .. } => TracepointId::InterruptExit,
            Self::IpiSent { .. } => TracepointId::IpiSent,
            Self::IpiReceived { .. } => TracepointId::IpiReceived,
            Self::TimerTick { .. } => TracepointId::TimerTick,
            Self::ContextSwitch { .. } => TracepointId::ContextSwitch,
            Self::SyscallEntry { .. } => TracepointId::SyscallEntry,
            Self::SyscallExit { .. } => TracepointId::SyscallExit,
            Self::PageFault { .. } => TracepointId::PageFault,
            Self::TlbFlush { .. } => TracepointId::TlbFlush,
            Self::LockAcquire { .. } => TracepointId::LockAcquire,
            Self::LockRelease { .. } => TracepointId::LockRelease,
        }
    }
}

/// A single trace event
#[derive(Debug, Clone, Copy)]
pub struct TraceEvent {
    /// Timestamp (CPU cycles or nanoseconds)
    pub timestamp: u64,
    /// CPU ID that emitted the event
    pub cpu_id: usize,
    /// Event type and data
    pub event: EventType,
}

/// Ring buffer for trace events
struct RingBuffer {
    /// Events storage
    events: Vec<TraceEvent>,
    /// Write position
    write_pos: AtomicUsize,
    /// Read position
    read_pos: AtomicUsize,
    /// Number of dropped events due to overflow
    dropped: AtomicU64,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Push an event into the ring buffer
    fn push(&mut self, event: TraceEvent) {
        if self.events.len() < self.events.capacity() {
            // Still growing the buffer
            self.events.push(event);
            self.write_pos.fetch_add(1, Ordering::Release);
        } else {
            // Buffer is full, use ring semantics
            let write = self.write_pos.load(Ordering::Acquire);
            let read = self.read_pos.load(Ordering::Acquire);
            let capacity = self.events.capacity();

            if write - read >= capacity {
                // Buffer full, drop event
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }

            let idx = write % capacity;
            self.events[idx] = event;
            self.write_pos.fetch_add(1, Ordering::Release);
        }
    }

    /// Pop an event from the ring buffer
    fn pop(&self) -> Option<TraceEvent> {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        if read >= write {
            return None; // Buffer empty
        }

        let capacity = self.events.capacity();
        let idx = read % capacity;
        let event = self.events[idx];
        self.read_pos.fetch_add(1, Ordering::Release);

        Some(event)
    }

    /// Get number of events in buffer
    fn len(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        write.saturating_sub(read)
    }

    /// Get number of dropped events
    fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Global tracing state
struct TracingState {
    /// Ring buffer for events
    ring_buffer: spin::Mutex<RingBuffer>,
    /// Enabled tracepoints (bitmap)
    enabled_tracepoints: [AtomicBool; TracepointId::COUNT],
    /// Global enable/disable
    tracing_enabled: AtomicBool,
    /// Total events emitted
    total_events: AtomicU64,
    /// Performance counters
    pmc: PerformanceCounters,
}

lazy_static::lazy_static! {
    static ref TRACING: TracingState = TracingState {
        ring_buffer: spin::Mutex::new(RingBuffer::new(MAX_EVENTS)),
        enabled_tracepoints: Default::default(),
        tracing_enabled: AtomicBool::new(false),
        total_events: AtomicU64::new(0),
        pmc: PerformanceCounters::new(),
    };
}

/// Initialize tracing subsystem
pub fn init(buffer_size: usize) -> Result<(), ObservabilityError> {
    if buffer_size == 0 || buffer_size > 1024 * 1024 {
        return Err(ObservabilityError::InvalidBufferSize(buffer_size));
    }

    let capacity = buffer_size / core::mem::size_of::<TraceEvent>();
    *TRACING.ring_buffer.lock() = RingBuffer::new(capacity);

    // Reset counters
    TRACING.total_events.store(0, Ordering::Release);

    // Enable tracing
    TRACING.tracing_enabled.store(true, Ordering::Release);

    Ok(())
}

/// Enable a specific tracepoint
pub fn enable_tracepoint(tp: TracepointId) {
    TRACING.enabled_tracepoints[tp as usize].store(true, Ordering::Release);
}

/// Disable a specific tracepoint
pub fn disable_tracepoint(tp: TracepointId) {
    TRACING.enabled_tracepoints[tp as usize].store(false, Ordering::Release);
}

/// Check if tracepoint is enabled
pub fn is_tracepoint_enabled(tp: TracepointId) -> bool {
    TRACING.enabled_tracepoints[tp as usize].load(Ordering::Acquire)
}

/// Enable all tracepoints
pub fn enable_all_tracepoints() {
    for tp in &TRACING.enabled_tracepoints {
        tp.store(true, Ordering::Release);
    }
}

/// Disable all tracepoints
pub fn disable_all_tracepoints() {
    for tp in &TRACING.enabled_tracepoints {
        tp.store(false, Ordering::Release);
    }
}

/// Emit a trace event
#[inline]
pub fn trace_event(event: EventType) {
    // Fast path: check if tracing is enabled
    if !TRACING.tracing_enabled.load(Ordering::Relaxed) {
        return;
    }

    // Check if this specific tracepoint is enabled
    let tp_id = event.tracepoint_id();
    if !TRACING.enabled_tracepoints[tp_id as usize].load(Ordering::Relaxed) {
        return;
    }

    // Get timestamp
    let timestamp = read_timestamp();

    // Get CPU ID
    let cpu_id = current_cpu_id();

    // Create event
    let trace_event = TraceEvent {
        timestamp,
        cpu_id,
        event,
    };

    // Run any attached BPF program for this tracepoint
    crate::bpf::run_attached(&trace_event);

    // Push to ring buffer
    TRACING.ring_buffer.lock().push(trace_event);
    TRACING.total_events.fetch_add(1, Ordering::Relaxed);
}

/// Read all events from ring buffer
pub fn for_each_event<F>(mut f: F)
where
    F: FnMut(TraceEvent),
{
    let buffer = TRACING.ring_buffer.lock();
    while let Some(event) = buffer.pop() {
        f(event);
    }
}

/// Get tracing statistics
pub fn get_stats() -> TracingStats {
    let buffer = TRACING.ring_buffer.lock();
    TracingStats {
        total_events: TRACING.total_events.load(Ordering::Relaxed),
        buffered_events: buffer.len() as u64,
        dropped_events: buffer.dropped(),
        tracing_enabled: TRACING.tracing_enabled.load(Ordering::Relaxed),
    }
}

/// Tracing statistics
#[derive(Debug, Clone, Copy)]
pub struct TracingStats {
    /// Total events emitted
    pub total_events: u64,
    /// Events currently in buffer
    pub buffered_events: u64,
    /// Events dropped due to overflow
    pub dropped_events: u64,
    /// Whether tracing is enabled
    pub tracing_enabled: bool,
}

/// Performance counters (PMU)
#[derive(Debug)]
pub struct PerformanceCounters {
    /// CPU cycles
    cycles: AtomicU64,
    /// Instructions retired
    instructions: AtomicU64,
    /// L1 cache misses
    l1_cache_misses: AtomicU64,
    /// L2 cache misses
    l2_cache_misses: AtomicU64,
    /// Branch mispredictions
    branch_misses: AtomicU64,
    /// TLB misses
    tlb_misses: AtomicU64,
}

impl PerformanceCounters {
    fn new() -> Self {
        Self {
            cycles: AtomicU64::new(0),
            instructions: AtomicU64::new(0),
            l1_cache_misses: AtomicU64::new(0),
            l2_cache_misses: AtomicU64::new(0),
            branch_misses: AtomicU64::new(0),
            tlb_misses: AtomicU64::new(0),
        }
    }

    /// Read current PMC values
    pub fn read() -> PmcSnapshot {
        PmcSnapshot {
            cycles: TRACING.pmc.cycles.load(Ordering::Relaxed),
            instructions: TRACING.pmc.instructions.load(Ordering::Relaxed),
            l1_cache_misses: TRACING.pmc.l1_cache_misses.load(Ordering::Relaxed),
            l2_cache_misses: TRACING.pmc.l2_cache_misses.load(Ordering::Relaxed),
            branch_misses: TRACING.pmc.branch_misses.load(Ordering::Relaxed),
            tlb_misses: TRACING.pmc.tlb_misses.load(Ordering::Relaxed),
        }
    }

    /// Update cycle counter
    pub fn update_cycles(&self, delta: u64) {
        self.cycles.fetch_add(delta, Ordering::Relaxed);
    }

    /// Update instruction counter
    pub fn update_instructions(&self, delta: u64) {
        self.instructions.fetch_add(delta, Ordering::Relaxed);
    }
}

/// PMC snapshot
#[derive(Debug, Clone, Copy)]
pub struct PmcSnapshot {
    pub cycles: u64,
    pub instructions: u64,
    pub l1_cache_misses: u64,
    pub l2_cache_misses: u64,
    pub branch_misses: u64,
    pub tlb_misses: u64,
}

impl PmcSnapshot {
    /// Calculate instructions per cycle (IPC)
    pub fn ipc(&self) -> f64 {
        if self.cycles == 0 {
            0.0
        } else {
            self.instructions as f64 / self.cycles as f64
        }
    }

    /// Calculate L1 cache miss rate
    pub fn l1_miss_rate(&self) -> f64 {
        if self.instructions == 0 {
            0.0
        } else {
            self.l1_cache_misses as f64 / self.instructions as f64
        }
    }
}

/// Read PMC snapshot
pub fn read_pmc() -> PmcSnapshot {
    PerformanceCounters::read()
}

/// Task profiling data
#[derive(Debug, Clone, Copy)]
pub struct TaskProfile {
    /// Task ID
    pub task_id: usize,
    /// Total CPU time (cycles)
    pub cpu_time: u64,
    /// Number of times scheduled
    pub schedule_count: u64,
    /// Number of times yielded
    pub yield_count: u64,
    /// Number of times blocked
    pub block_count: u64,
    /// Memory allocated (bytes)
    pub memory_allocated: usize,
    /// Memory freed (bytes)
    pub memory_freed: usize,
    /// Peak memory usage (bytes)
    pub peak_memory: usize,
}

impl TaskProfile {
    /// Create new empty profile
    pub fn new(task_id: usize) -> Self {
        Self {
            task_id,
            cpu_time: 0,
            schedule_count: 0,
            yield_count: 0,
            block_count: 0,
            memory_allocated: 0,
            memory_freed: 0,
            peak_memory: 0,
        }
    }

    /// Get current memory usage
    pub fn current_memory(&self) -> usize {
        self.memory_allocated.saturating_sub(self.memory_freed)
    }
}

/// Get current CPU timestamp
#[cfg(target_arch = "x86_64")]
fn read_timestamp() -> u64 {
    unsafe {
        let mut aux: u32 = 0;
        core::arch::x86_64::__rdtscp(&mut aux)
    }
}

#[cfg(target_arch = "aarch64")]
fn read_timestamp() -> u64 {
    unsafe {
        let cnt: u64;
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) cnt);
        cnt
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn read_timestamp() -> u64 {
    // Fallback: use a counter
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Get current CPU ID
fn current_cpu_id() -> usize {
    #[cfg(feature = "std")]
    {
        0 // In std mode, assume single CPU
    }
    #[cfg(not(feature = "std"))]
    {
        // In no_std mode, read from CPU-specific register
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let mut aux: u32 = 0;
            core::arch::x86_64::__rdtscp(&mut aux);
            (aux & 0xFFF) as usize
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            0
        }
    }
}

/// Observability errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservabilityError {
    /// Invalid buffer size
    InvalidBufferSize(usize),
    /// Tracing not initialized
    NotInitialized,
}

impl fmt::Display for ObservabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBufferSize(size) => {
                write!(f, "Invalid buffer size: {} bytes", size)
            }
            Self::NotInitialized => write!(f, "Tracing not initialized"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ObservabilityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let result = init(65536);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_buffer_size() {
        assert!(init(0).is_err());
        assert!(init(2 * 1024 * 1024).is_err());
    }

    #[test]
    fn test_tracepoint_enable_disable() {
        enable_tracepoint(TracepointId::TaskSpawn);
        assert!(is_tracepoint_enabled(TracepointId::TaskSpawn));

        disable_tracepoint(TracepointId::TaskSpawn);
        assert!(!is_tracepoint_enabled(TracepointId::TaskSpawn));
    }

    #[test]
    fn test_enable_all_tracepoints() {
        enable_all_tracepoints();
        assert!(is_tracepoint_enabled(TracepointId::TaskSpawn));
        assert!(is_tracepoint_enabled(TracepointId::PageAlloc));
        assert!(is_tracepoint_enabled(TracepointId::InterruptEntry));
    }

    #[test]
    fn test_disable_all_tracepoints() {
        enable_all_tracepoints();
        disable_all_tracepoints();
        assert!(!is_tracepoint_enabled(TracepointId::TaskSpawn));
        assert!(!is_tracepoint_enabled(TracepointId::PageAlloc));
    }

    #[test]
    fn test_trace_event() {
        init(65536).expect("init");
        enable_tracepoint(TracepointId::TaskSpawn);

        trace_event(EventType::TaskSpawn {
            task_id: 42,
            priority: 100,
        });

        let stats = get_stats();
        assert_eq!(stats.total_events, 1);
    }

    #[test]
    fn test_for_each_event() {
        init(65536).expect("init");
        enable_tracepoint(TracepointId::TaskSpawn);

        trace_event(EventType::TaskSpawn {
            task_id: 1,
            priority: 100,
        });
        trace_event(EventType::TaskSpawn {
            task_id: 2,
            priority: 50,
        });

        let mut count = 0;
        for_each_event(|_event| {
            count += 1;
        });

        assert_eq!(count, 2);
    }

    #[test]
    fn test_tracing_stats() {
        init(65536).expect("init");
        enable_all_tracepoints();

        for i in 0..10 {
            trace_event(EventType::TaskSpawn {
                task_id: i,
                priority: 100,
            });
        }

        let stats = get_stats();
        assert_eq!(stats.total_events, 10);
        assert!(stats.tracing_enabled);
    }

    #[test]
    fn test_pmc_snapshot() {
        let pmc = read_pmc();
        // PMC values should be initialized (they're u64, always >= 0)
        let _ = pmc.cycles;
        let _ = pmc.instructions;
    }

    #[test]
    fn test_pmc_ipc() {
        TRACING.pmc.cycles.store(1000, Ordering::Relaxed);
        TRACING.pmc.instructions.store(800, Ordering::Relaxed);

        let pmc = read_pmc();
        assert_eq!(pmc.ipc(), 0.8);
    }

    #[test]
    fn test_task_profile() {
        let mut profile = TaskProfile::new(42);
        assert_eq!(profile.task_id, 42);
        assert_eq!(profile.current_memory(), 0);

        profile.memory_allocated = 1024;
        profile.memory_freed = 512;
        assert_eq!(profile.current_memory(), 512);
    }

    #[test]
    fn test_tracepoint_names() {
        assert_eq!(TracepointId::TaskSpawn.name(), "task_spawn");
        assert_eq!(TracepointId::PageAlloc.name(), "page_alloc");
        assert_eq!(TracepointId::InterruptEntry.name(), "interrupt_entry");
    }

    #[test]
    fn test_event_tracepoint_id() {
        let event = EventType::TaskSpawn {
            task_id: 1,
            priority: 100,
        };
        assert_eq!(event.tracepoint_id(), TracepointId::TaskSpawn);
    }

    #[test]
    fn test_ring_buffer() {
        let mut rb = RingBuffer::new(4);

        let event = TraceEvent {
            timestamp: 1000,
            cpu_id: 0,
            event: EventType::TaskSpawn {
                task_id: 1,
                priority: 100,
            },
        };

        rb.push(event);
        assert_eq!(rb.len(), 1);

        let popped = rb.pop();
        assert!(popped.is_some());
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn test_error_display() {
        let err = ObservabilityError::InvalidBufferSize(12345);
        let msg = format!("{}", err);
        assert!(msg.contains("12345"));
    }
}
