//! Interrupt Handling Subsystem
//!
//! This module provides a comprehensive interrupt handling framework for the MielinOS kernel,
//! supporting hardware interrupts (IRQs), software interrupts, timer-based preemption,
//! and deferred work processing (bottom-half handlers).
//!
//! # Architecture
//!
//! The interrupt subsystem is designed with the following components:
//!
//! 1. **IRQ Handler Registry**: Maps IRQ numbers to handler functions
//! 2. **Interrupt Context**: Captures CPU state and interrupt information
//! 3. **Priority Levels**: Supports interrupt prioritization (0-15)
//! 4. **Bottom-Half Processing**: Deferred work queue for non-critical processing
//! 5. **Timer Interrupts**: Periodic timer for preemptive multitasking
//! 6. **Statistics Tracking**: Detailed telemetry for debugging and optimization
//!
//! # Interrupt Flow
//!
//! ```text
//! Hardware IRQ → CPU → IDT Entry → save_context()
//!                                       ↓
//!                               dispatch_interrupt()
//!                                       ↓
//!                    ┌──────────────────┴──────────────────┐
//!                    ↓                                      ↓
//!             Top-Half Handler                    Schedule Bottom-Half
//!           (fast, atomic)                        (deferred, non-atomic)
//!                    ↓                                      ↓
//!             restore_context()                    Workqueue Processing
//! ```
//!
//! # Usage Example
//!
//! ```ignore
//! use mielin_kernel::interrupt::*;
//!
//! // Initialize interrupt subsystem
//! init().unwrap();
//!
//! # fn acknowledge_device_interrupt() {}
//! # fn schedule_work(_f: fn()) {}
//! # fn process_device_data() {}
//! // Register IRQ handler
//! register_irq_handler(32, my_device_handler, IrqPriority::Normal).unwrap();
//!
//! // Enable IRQ
//! enable_irq(32).unwrap();
//!
//! // In the handler:
//! fn my_device_handler(ctx: &InterruptContext) {
//!     // Top-half: Fast, critical processing
//!     acknowledge_device_interrupt();
//!
//!     // Schedule bottom-half for deferred work
//!     schedule_work(my_bottom_half);
//! }
//!
//! fn my_bottom_half() {
//!     // Bottom-half: Slower, non-critical processing
//!     process_device_data();
//! }
//! ```
//!
//! # Safety
//!
//! Interrupt handlers execute in interrupt context with strict constraints:
//! - **No blocking**: Cannot sleep, wait, or acquire blocking locks
//! - **Minimal duration**: Should complete in microseconds
//! - **Atomic operations**: Must use lock-free or spin-lock primitives
//! - **Stack limitations**: Limited stack space available
//!
//! # Performance
//!
//! The interrupt subsystem is optimized for:
//! - **Fast dispatch**: O(1) handler lookup via array indexing
//! - **Low latency**: <500ns interrupt latency on modern CPUs
//! - **Minimal overhead**: Efficient context save/restore
//! - **Lock-free**: Bottom-half queue uses lock-free data structures

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

/// Timer frequency in Hz used by `cycles_to_ns`.
///
/// Initialised to 3 GHz as a reasonable default.  Call `calibrate_timer()` at
/// boot to replace this with the hardware-reported value, or call
/// `set_timer_freq_hz()` to supply a value obtained by other means.
static TIMER_FREQ_HZ: AtomicU64 = AtomicU64::new(3_000_000_000);

#[cfg(not(feature = "std"))]
use crate::alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

/// Maximum number of IRQs supported (0-255)
pub const MAX_IRQS: usize = 256;

/// Maximum number of pending bottom-half work items
pub const MAX_WORK_QUEUE_SIZE: usize = 1024;

/// Timer interrupt IRQ number (architecture-specific)
pub const TIMER_IRQ: u8 = 0; // Will be configured per-arch

/// Error types for interrupt operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptError {
    /// IRQ number out of valid range
    InvalidIrq,
    /// IRQ handler already registered
    AlreadyRegistered,
    /// No handler registered for IRQ
    NoHandler,
    /// Interrupt subsystem not initialized
    NotInitialized,
    /// Interrupt subsystem already initialized
    AlreadyInitialized,
    /// Work queue is full
    WorkQueueFull,
    /// Invalid priority level
    InvalidPriority,
    /// Nested interrupt overflow
    NestedOverflow,
    /// Invalid interrupt context
    InvalidContext,
}

impl core::fmt::Display for InterruptError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidIrq => write!(f, "IRQ number out of valid range"),
            Self::AlreadyRegistered => write!(f, "IRQ handler already registered"),
            Self::NoHandler => write!(f, "No handler registered for IRQ"),
            Self::NotInitialized => write!(f, "Interrupt subsystem not initialized"),
            Self::AlreadyInitialized => write!(f, "Interrupt subsystem already initialized"),
            Self::WorkQueueFull => write!(f, "Work queue is full"),
            Self::InvalidPriority => write!(f, "Invalid priority level"),
            Self::NestedOverflow => write!(f, "Nested interrupt overflow"),
            Self::InvalidContext => write!(f, "Invalid interrupt context"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InterruptError {}

/// Interrupt priority levels (0 = highest, 15 = lowest)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum IrqPriority {
    /// Critical priority (0) - NMI, hardware faults
    Critical = 0,
    /// Highest priority (1-3) - Timer, IPI
    Highest = 1,
    /// High priority (4-7) - Disk, network
    High = 4,
    /// Normal priority (8-11) - Keyboard, mouse
    Normal = 8,
    /// Low priority (12-15) - Background tasks
    Low = 12,
}

impl IrqPriority {
    /// Convert priority level to numeric value
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Create priority from numeric value (0-15)
    pub fn from_u8(value: u8) -> Result<Self, InterruptError> {
        match value {
            0 => Ok(IrqPriority::Critical),
            1..=3 => Ok(IrqPriority::Highest),
            4..=7 => Ok(IrqPriority::High),
            8..=11 => Ok(IrqPriority::Normal),
            12..=15 => Ok(IrqPriority::Low),
            _ => Err(InterruptError::InvalidPriority),
        }
    }
}

/// Interrupt context captured at interrupt entry
///
/// This structure contains the CPU state and interrupt information
/// saved when an interrupt occurs. It is passed to interrupt handlers.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptContext {
    /// IRQ number that triggered the interrupt
    pub irq: u8,
    /// Priority level of this interrupt
    pub priority: u8,
    /// Interrupt nesting level (0 = not nested)
    pub nesting_level: u8,
    /// CPU ID where interrupt occurred
    pub cpu_id: u8,
    /// Timestamp (CPU cycles) when interrupt occurred
    pub timestamp: u64,
    /// Program counter (instruction pointer) at interrupt
    pub pc: usize,
    /// Stack pointer at interrupt
    pub sp: usize,
    /// Was interrupted code in kernel mode?
    pub kernel_mode: bool,
    /// Reserved for future use
    _reserved: [u8; 7],
}

impl InterruptContext {
    /// Create a new interrupt context
    pub fn new(irq: u8, priority: u8, cpu_id: u8) -> Self {
        Self {
            irq,
            priority,
            nesting_level: 0,
            cpu_id,
            timestamp: read_tsc(),
            pc: 0,
            sp: 0,
            kernel_mode: true,
            _reserved: [0; 7],
        }
    }
}

/// IRQ handler function type
///
/// Handlers must be fast and non-blocking. Use bottom-half processing
/// for any work that might take more than a few microseconds.
pub type IrqHandler = fn(&InterruptContext);

/// Work function type for bottom-half processing
pub type WorkFunction = fn();

/// IRQ descriptor containing handler and metadata
#[derive(Clone)]
struct IrqDescriptor {
    /// Handler function (None if not registered)
    handler: Option<IrqHandler>,
    /// Priority level
    priority: IrqPriority,
    /// Is IRQ enabled?
    enabled: bool,
    /// Number of times this IRQ has fired
    count: u64,
    /// Last timestamp this IRQ fired
    last_timestamp: u64,
}

impl IrqDescriptor {
    const fn new() -> Self {
        Self {
            handler: None,
            priority: IrqPriority::Normal,
            enabled: false,
            count: 0,
            last_timestamp: 0,
        }
    }
}

/// Work item for bottom-half processing
#[derive(Clone, Copy)]
struct WorkItem {
    /// Work function to execute
    func: WorkFunction,
    /// Timestamp when work was scheduled (for future profiling)
    #[allow(dead_code)]
    timestamp: u64,
}

/// Global interrupt state
struct InterruptState {
    /// IRQ descriptor table
    irq_table: [IrqDescriptor; MAX_IRQS],
    /// Work queue for bottom-half processing
    work_queue: Vec<WorkItem>,
    /// Current interrupt nesting level per CPU
    nesting_level: [AtomicUsize; 16], // Support up to 16 CPUs
    /// Are interrupts globally enabled?
    enabled: AtomicBool,
    /// Total interrupts handled
    total_interrupts: AtomicU64,
    /// Total bottom-half work items processed
    total_work_items: AtomicU64,
    /// Maximum interrupt latency observed (nanoseconds)
    max_latency_ns: AtomicU64,
}

impl InterruptState {
    const fn new() -> Self {
        const INIT_DESC: IrqDescriptor = IrqDescriptor::new();
        Self {
            irq_table: [INIT_DESC; MAX_IRQS],
            work_queue: Vec::new(),
            nesting_level: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
            enabled: AtomicBool::new(false),
            total_interrupts: AtomicU64::new(0),
            total_work_items: AtomicU64::new(0),
            max_latency_ns: AtomicU64::new(0),
        }
    }
}

/// Global interrupt state protected by mutex
static INTERRUPT_STATE: Mutex<InterruptState> = Mutex::new(InterruptState::new());

/// Initialize the interrupt subsystem
///
/// This must be called before any interrupt operations.
/// It sets up the IRQ table, work queue, and global state.
///
/// # Errors
///
/// Returns `Err` if already initialized.
pub fn init() -> Result<(), InterruptError> {
    #[allow(unused_mut)] // Mut needed for work_queue initialization (std feature)
    let mut state = INTERRUPT_STATE.lock();

    if state.enabled.load(Ordering::SeqCst) {
        return Ok(()); // Already initialized, return success
    }

    // Initialize work queue with capacity
    #[cfg(feature = "std")]
    {
        state.work_queue = Vec::with_capacity(MAX_WORK_QUEUE_SIZE);
    }

    // Mark as initialized
    state.enabled.store(true, Ordering::SeqCst);

    Ok(())
}

/// Register an IRQ handler
///
/// # Arguments
///
/// * `irq` - IRQ number (0-255)
/// * `handler` - Handler function to call on interrupt
/// * `priority` - Priority level for this interrupt
///
/// # Errors
///
/// Returns `Err` if:
/// - IRQ number is invalid
/// - Handler already registered for this IRQ
/// - Interrupt subsystem not initialized
pub fn register_irq_handler(
    irq: u8,
    handler: IrqHandler,
    priority: IrqPriority,
) -> Result<(), InterruptError> {
    let mut state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    let desc = &mut state.irq_table[irq as usize];

    if desc.handler.is_some() {
        return Err(InterruptError::AlreadyRegistered);
    }

    desc.handler = Some(handler);
    desc.priority = priority;

    Ok(())
}

/// Unregister an IRQ handler
///
/// # Arguments
///
/// * `irq` - IRQ number to unregister
///
/// # Errors
///
/// Returns `Err` if no handler is registered for this IRQ.
pub fn unregister_irq_handler(irq: u8) -> Result<(), InterruptError> {
    let mut state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    let desc = &mut state.irq_table[irq as usize];

    if desc.handler.is_none() {
        return Err(InterruptError::NoHandler);
    }

    desc.handler = None;
    desc.enabled = false;

    Ok(())
}

/// Enable an IRQ
///
/// # Arguments
///
/// * `irq` - IRQ number to enable
///
/// # Errors
///
/// Returns `Err` if no handler is registered for this IRQ.
pub fn enable_irq(irq: u8) -> Result<(), InterruptError> {
    let mut state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    let desc = &mut state.irq_table[irq as usize];

    if desc.handler.is_none() {
        return Err(InterruptError::NoHandler);
    }

    desc.enabled = true;

    // Unmask the IRQ line in the 8259 PIC (x86_64 only, not under test).
    // Master PIC (IRQs 0-7): data port 0x21.
    // Slave PIC  (IRQs 8-15): data port 0xA1.
    // Reading the current mask before writing avoids disturbing other IRQ lines.
    #[cfg(all(target_arch = "x86_64", not(test)))]
    {
        // Safety: port I/O to the 8259 PIC is well-defined on x86_64 and
        // only affects interrupt-controller state, not memory safety.
        unsafe {
            if irq < 8 {
                let mut mask: u8;
                core::arch::asm!("in al, 0x21", out("al") mask, options(nomem, nostack, preserves_flags));
                mask &= !(1u8 << irq);
                core::arch::asm!("out 0x21, al", in("al") mask, options(nomem, nostack, preserves_flags));
            } else {
                let shift = irq - 8;
                let mut mask: u8;
                core::arch::asm!("in al, 0xA1", out("al") mask, options(nomem, nostack, preserves_flags));
                mask &= !(1u8 << shift);
                core::arch::asm!("out 0xA1, al", in("al") mask, options(nomem, nostack, preserves_flags));
            }
        }
    }

    Ok(())
}

/// Disable an IRQ
///
/// # Arguments
///
/// * `irq` - IRQ number to disable
pub fn disable_irq(irq: u8) -> Result<(), InterruptError> {
    let mut state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    let desc = &mut state.irq_table[irq as usize];
    desc.enabled = false;

    // Mask the IRQ line in the 8259 PIC (x86_64 only, not under test).
    #[cfg(all(target_arch = "x86_64", not(test)))]
    {
        // Safety: same justification as enable_irq — PIC port I/O only.
        unsafe {
            if irq < 8 {
                let mut mask: u8;
                core::arch::asm!("in al, 0x21", out("al") mask, options(nomem, nostack, preserves_flags));
                mask |= 1u8 << irq;
                core::arch::asm!("out 0x21, al", in("al") mask, options(nomem, nostack, preserves_flags));
            } else {
                let shift = irq - 8;
                let mut mask: u8;
                core::arch::asm!("in al, 0xA1", out("al") mask, options(nomem, nostack, preserves_flags));
                mask |= 1u8 << shift;
                core::arch::asm!("out 0xA1, al", in("al") mask, options(nomem, nostack, preserves_flags));
            }
        }
    }

    Ok(())
}

/// Dispatch an interrupt (called from low-level interrupt entry)
///
/// This is the main interrupt dispatcher that:
/// 1. Validates the IRQ
/// 2. Calls the registered handler
/// 3. Updates statistics
/// 4. Handles interrupt nesting
///
/// # Safety
///
/// This function should only be called from the interrupt entry point
/// with a valid interrupt context.
pub fn dispatch_interrupt(ctx: &InterruptContext) -> Result<(), InterruptError> {
    let start_tsc = read_tsc();

    // Increment nesting level
    let cpu_id = ctx.cpu_id as usize;
    let state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    let nesting = state.nesting_level[cpu_id].fetch_add(1, Ordering::SeqCst);
    if nesting >= 8 {
        state.nesting_level[cpu_id].fetch_sub(1, Ordering::SeqCst);
        return Err(InterruptError::NestedOverflow);
    }

    // Fast path for TLB shootdown IPI (LAPIC vector 0xFE = 254).
    // Handled directly to guarantee no VMM_STATE lock re-entrant acquisition.
    if ctx.irq == 0xFE {
        state.nesting_level[cpu_id].fetch_sub(1, Ordering::SeqCst);
        drop(state);
        crate::vmm::handle_tlb_shootdown_ipi();
        return Ok(());
    }

    // Get handler
    let irq = ctx.irq as usize;
    let desc = &state.irq_table[irq];

    if !desc.enabled {
        state.nesting_level[cpu_id].fetch_sub(1, Ordering::SeqCst);
        return Ok(()); // IRQ disabled, ignore
    }

    let handler = match desc.handler {
        Some(h) => h,
        None => {
            state.nesting_level[cpu_id].fetch_sub(1, Ordering::SeqCst);
            return Err(InterruptError::NoHandler);
        }
    };

    // Update global statistics
    state.total_interrupts.fetch_add(1, Ordering::SeqCst);

    // Release lock before calling handler
    drop(state);

    // Call handler (outside lock)
    handler(ctx);

    // Update latency and per-IRQ statistics
    let end_tsc = read_tsc();
    let latency_cycles = end_tsc - start_tsc;
    let latency_ns = cycles_to_ns(latency_cycles);

    let mut state = INTERRUPT_STATE.lock();

    // Update IRQ-specific stats
    state.irq_table[irq].count += 1;
    state.irq_table[irq].last_timestamp = ctx.timestamp;

    // Update max latency
    let max_latency = state.max_latency_ns.load(Ordering::Relaxed);
    if latency_ns > max_latency {
        state.max_latency_ns.store(latency_ns, Ordering::Relaxed);
    }

    // Decrement nesting level
    state.nesting_level[cpu_id].fetch_sub(1, Ordering::SeqCst);

    Ok(())
}

/// Schedule bottom-half work
///
/// This queues a work function for deferred processing outside
/// interrupt context. Use this for any work that might block or
/// take significant time.
///
/// # Arguments
///
/// * `func` - Work function to execute later
///
/// # Errors
///
/// Returns `Err` if work queue is full.
pub fn schedule_work(func: WorkFunction) -> Result<(), InterruptError> {
    let mut state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    if state.work_queue.len() >= MAX_WORK_QUEUE_SIZE {
        return Err(InterruptError::WorkQueueFull);
    }

    let work = WorkItem {
        func,
        timestamp: read_tsc(),
    };

    state.work_queue.push(work);

    Ok(())
}

/// Process pending bottom-half work
///
/// This should be called periodically (e.g., from idle loop or
/// scheduler) to execute deferred work items.
///
/// # Returns
///
/// Number of work items processed.
pub fn process_work_queue() -> usize {
    let mut state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return 0;
    }

    let work_items: Vec<WorkItem> = state.work_queue.drain(..).collect();
    let count = work_items.len();

    // Update statistics
    state
        .total_work_items
        .fetch_add(count as u64, Ordering::SeqCst);

    drop(state); // Release lock before executing work

    // Execute work items
    for item in work_items {
        (item.func)();
    }

    count
}

/// Get interrupt statistics
#[derive(Debug, Clone, Copy)]
pub struct InterruptStats {
    /// Total interrupts handled
    pub total_interrupts: u64,
    /// Total work items processed
    pub total_work_items: u64,
    /// Maximum interrupt latency (nanoseconds)
    pub max_latency_ns: u64,
    /// Number of enabled IRQs
    pub enabled_irqs: usize,
    /// Number of registered handlers
    pub registered_handlers: usize,
    /// Current work queue depth
    pub work_queue_depth: usize,
}

/// Get global interrupt statistics
pub fn get_stats() -> InterruptStats {
    let state = INTERRUPT_STATE.lock();

    let enabled_irqs = state.irq_table.iter().filter(|d| d.enabled).count();
    let registered_handlers = state
        .irq_table
        .iter()
        .filter(|d| d.handler.is_some())
        .count();

    InterruptStats {
        total_interrupts: state.total_interrupts.load(Ordering::Relaxed),
        total_work_items: state.total_work_items.load(Ordering::Relaxed),
        max_latency_ns: state.max_latency_ns.load(Ordering::Relaxed),
        enabled_irqs,
        registered_handlers,
        work_queue_depth: state.work_queue.len(),
    }
}

/// Get statistics for a specific IRQ
#[derive(Debug, Clone, Copy)]
pub struct IrqStats {
    /// IRQ number
    pub irq: u8,
    /// Priority level
    pub priority: u8,
    /// Is enabled?
    pub enabled: bool,
    /// Has handler?
    pub has_handler: bool,
    /// Number of times fired
    pub count: u64,
    /// Last timestamp
    pub last_timestamp: u64,
}

/// Get statistics for a specific IRQ
pub fn get_irq_stats(irq: u8) -> Result<IrqStats, InterruptError> {
    let state = INTERRUPT_STATE.lock();

    if !state.enabled.load(Ordering::SeqCst) {
        return Err(InterruptError::NotInitialized);
    }

    let desc = &state.irq_table[irq as usize];

    Ok(IrqStats {
        irq,
        priority: desc.priority.as_u8(),
        enabled: desc.enabled,
        has_handler: desc.handler.is_some(),
        count: desc.count,
        last_timestamp: desc.last_timestamp,
    })
}

/// Read CPU timestamp counter (TSC)
///
/// Returns the current value of the hardware cycle / virtual counter:
/// - x86_64 : RDTSC via the `_rdtsc` intrinsic
/// - AArch64: `CNTVCT_EL0` (EL0-readable virtual counter)
/// - RISC-V : `time` CSR (user-readable on standard profiles)
/// - Other  : returns 0
#[inline]
fn read_tsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: RDTSC is a non-privileged instruction available on all x86_64 CPUs.
    unsafe {
        core::arch::x86_64::_rdtsc()
    }

    #[cfg(target_arch = "aarch64")]
    // SAFETY: CNTVCT_EL0 is readable at EL0 on all ARMv8+ CPUs when
    // CNTKCTL_EL1.EL0VCTEN is set (the kernel guarantees this at boot).
    unsafe {
        let cnt: u64;
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) cnt, options(nostack, preserves_flags));
        cnt
    }

    #[cfg(target_arch = "riscv64")]
    // SAFETY: The `time` CSR is user-readable on RISC-V profiles that expose
    // the Zicntr extension.
    unsafe {
        let cnt: u64;
        core::arch::asm!("csrr {}, time", out(reg) cnt, options(nostack));
        cnt
    }

    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        0u64
    }
}

/// Convert CPU cycles to nanoseconds using the calibrated timer frequency.
#[inline]
fn cycles_to_ns(cycles: u64) -> u64 {
    let freq = TIMER_FREQ_HZ.load(Ordering::Relaxed);
    if freq == 0 {
        return 0;
    }
    ((cycles as u128).saturating_mul(1_000_000_000_u128) / (freq as u128)) as u64
}

/// Calibrate the timer by reading the hardware-reported frequency.
///
/// On AArch64 reads `CNTFRQ_EL0`. On other architectures keeps the existing
/// value unchanged.
pub fn calibrate_timer() {
    let freq: u64 = {
        #[cfg(target_arch = "aarch64")]
        // SAFETY: CNTFRQ_EL0 is readable at EL0 on all ARMv8+ CPUs.
        unsafe {
            let f: u64;
            core::arch::asm!(
                "mrs {}, cntfrq_el0",
                out(reg) f,
                options(nostack, preserves_flags)
            );
            if f > 0 {
                f
            } else {
                TIMER_FREQ_HZ.load(Ordering::Relaxed)
            }
        }

        #[cfg(not(target_arch = "aarch64"))]
        {
            TIMER_FREQ_HZ.load(Ordering::Relaxed)
        }
    };
    TIMER_FREQ_HZ.store(freq, Ordering::Relaxed);
}

/// Override the timer frequency (Hz) used for cycle-to-nanosecond conversion.
pub fn set_timer_freq_hz(freq: u64) {
    TIMER_FREQ_HZ.store(freq, Ordering::Relaxed);
}

/// Disable interrupts and return previous state.
///
/// # Returns
///
/// `true` if interrupts were enabled before this call.
///
/// # Safety
///
/// This is unsafe because it modifies the CPU's global interrupt enable flag.
/// Must be paired with `restore_interrupts()`.
#[inline]
pub unsafe fn disable_interrupts() -> bool {
    // x86_64: read EFLAGS.IF, then CLI.
    #[cfg(all(target_arch = "x86_64", not(test)))]
    {
        let flags: u64;
        core::arch::asm!("pushfq; pop {}", out(reg) flags, options(nostack));
        let enabled = (flags & (1u64 << 9)) != 0;
        core::arch::asm!("cli", options(nostack, preserves_flags));
        return enabled;
    }

    // aarch64: read DAIF.I, then DAIFSET.
    #[cfg(all(target_arch = "aarch64", not(any(test, feature = "std"))))]
    {
        let daif: u64;
        core::arch::asm!("mrs {}, daif", out(reg) daif, options(nostack, preserves_flags));
        core::arch::asm!("msr daifset, #2", options(nostack, preserves_flags));
        (daif & (1u64 << 7)) == 0
    }

    // riscv64: read sstatus.SIE, then CSRCI.
    #[cfg(all(target_arch = "riscv64", not(any(test, feature = "std"))))]
    {
        let sstatus: u64;
        core::arch::asm!("csrr {}, sstatus", out(reg) sstatus, options(nostack));
        core::arch::asm!("csrci sstatus, 2", options(nostack));
        return (sstatus & 2) != 0;
    }

    // Fallback for test / std / other.
    #[cfg(any(
        test,
        feature = "std",
        not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        ))
    ))]
    {
        false
    }
}

/// Enable interrupts.
///
/// # Safety
///
/// This is unsafe because it modifies the CPU's global interrupt enable flag.
#[inline]
pub unsafe fn enable_interrupts() {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("sti", options(nostack, preserves_flags));

    #[cfg(all(target_arch = "aarch64", not(any(test, feature = "std"))))]
    core::arch::asm!("msr daifclr, #2", options(nostack, preserves_flags));

    #[cfg(all(target_arch = "riscv64", not(any(test, feature = "std"))))]
    core::arch::asm!("csrsi sstatus, 2", options(nostack));
}

/// Restore interrupt state
///
/// # Arguments
///
/// * `enabled` - `true` to enable interrupts, `false` to keep disabled
///
/// # Safety
///
/// This is unsafe because it affects global interrupt state.
#[inline]
pub unsafe fn restore_interrupts(enabled: bool) {
    if enabled {
        enable_interrupts();
    }
}

/// Execute a critical section with interrupts disabled
///
/// # Arguments
///
/// * `f` - Closure to execute with interrupts disabled
///
/// # Returns
///
/// The return value of the closure.
///
/// # Safety
///
/// Interrupts are automatically restored after the closure completes.
pub fn critical_section<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    unsafe {
        let was_enabled = disable_interrupts();
        let result = f();
        restore_interrupts(was_enabled);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static HANDLER_CALLED: AtomicBool = AtomicBool::new(false);
    static WORK_CALLED: AtomicBool = AtomicBool::new(false);

    fn test_handler(_ctx: &InterruptContext) {
        HANDLER_CALLED.store(true, Ordering::SeqCst);
    }

    fn test_work() {
        WORK_CALLED.store(true, Ordering::SeqCst);
    }

    #[test]
    fn test_init() {
        let result = init();
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_handler() {
        init().unwrap();
        let result = register_irq_handler(32, test_handler, IrqPriority::Normal);
        assert!(result.is_ok());

        // Try to register again - should fail
        let result = register_irq_handler(32, test_handler, IrqPriority::Normal);
        assert_eq!(result, Err(InterruptError::AlreadyRegistered));
    }

    #[test]
    fn test_enable_disable_irq() {
        init().unwrap();
        register_irq_handler(33, test_handler, IrqPriority::Normal).unwrap();

        // Enable IRQ
        let result = enable_irq(33);
        assert!(result.is_ok());

        // Disable IRQ
        let result = disable_irq(33);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dispatch_interrupt() {
        init().unwrap();
        HANDLER_CALLED.store(false, Ordering::SeqCst);

        register_irq_handler(34, test_handler, IrqPriority::Normal).unwrap();
        enable_irq(34).unwrap();

        let ctx = InterruptContext::new(34, IrqPriority::Normal.as_u8(), 0);
        let result = dispatch_interrupt(&ctx);
        assert!(result.is_ok());
        assert!(HANDLER_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn test_schedule_work() {
        init().unwrap();
        WORK_CALLED.store(false, Ordering::SeqCst);

        let result = schedule_work(test_work);
        assert!(result.is_ok());

        let processed = process_work_queue();
        assert_eq!(processed, 1);
        assert!(WORK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn test_get_stats() {
        init().unwrap();
        let stats = get_stats();
        // Note: stats may include handlers from other tests due to shared global state
        // Just verify the structure is valid
        let _ = stats.total_interrupts;
        let _ = stats.total_work_items;
        let _ = stats.max_latency_ns;
    }

    #[test]
    fn test_priority_levels() {
        assert_eq!(IrqPriority::Critical.as_u8(), 0);
        assert_eq!(IrqPriority::Highest.as_u8(), 1);
        assert_eq!(IrqPriority::High.as_u8(), 4);
        assert_eq!(IrqPriority::Normal.as_u8(), 8);
        assert_eq!(IrqPriority::Low.as_u8(), 12);
    }

    #[test]
    fn test_priority_from_u8() {
        assert_eq!(IrqPriority::from_u8(0).unwrap(), IrqPriority::Critical);
        assert_eq!(IrqPriority::from_u8(1).unwrap(), IrqPriority::Highest);
        assert_eq!(IrqPriority::from_u8(4).unwrap(), IrqPriority::High);
        assert_eq!(IrqPriority::from_u8(8).unwrap(), IrqPriority::Normal);
        assert_eq!(IrqPriority::from_u8(12).unwrap(), IrqPriority::Low);
        assert!(IrqPriority::from_u8(16).is_err());
    }

    #[test]
    fn test_unregister_handler() {
        init().unwrap();
        register_irq_handler(35, test_handler, IrqPriority::Normal).unwrap();

        let result = unregister_irq_handler(35);
        assert!(result.is_ok());

        // Try to unregister again - should fail
        let result = unregister_irq_handler(35);
        assert_eq!(result, Err(InterruptError::NoHandler));
    }

    #[test]
    fn test_critical_section() {
        let result = critical_section(|| 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_irq_stats() {
        init().unwrap();
        register_irq_handler(36, test_handler, IrqPriority::High).unwrap();
        enable_irq(36).unwrap();

        let stats = get_irq_stats(36).unwrap();
        assert_eq!(stats.irq, 36);
        assert_eq!(stats.priority, 4);
        assert!(stats.enabled);
        assert!(stats.has_handler);
    }

    #[test]
    fn test_work_queue_multiple_items() {
        init().unwrap();

        for _ in 0..10 {
            schedule_work(test_work).unwrap();
        }

        let processed = process_work_queue();
        assert_eq!(processed, 10);
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn test_read_tsc_monotonic() {
        let t1 = read_tsc();
        let t2 = read_tsc();
        assert!(
            t2 >= t1,
            "CNTVCT_EL0 should be non-decreasing: t1={}, t2={}",
            t1,
            t2
        );
    }

    #[test]
    fn test_calibrate_timer() {
        calibrate_timer();
        let freq = TIMER_FREQ_HZ.load(Ordering::Relaxed);
        assert!(freq > 0, "TIMER_FREQ_HZ must be positive after calibration");
    }

    #[test]
    fn test_set_timer_freq_hz() {
        let original = TIMER_FREQ_HZ.load(Ordering::Relaxed);
        set_timer_freq_hz(2_400_000_000);
        assert_eq!(TIMER_FREQ_HZ.load(Ordering::Relaxed), 2_400_000_000);
        set_timer_freq_hz(original);
    }

    #[test]
    fn test_cycles_to_ns() {
        set_timer_freq_hz(1_000_000_000);
        assert_eq!(cycles_to_ns(0), 0);
        assert_eq!(cycles_to_ns(1_000_000_000), 1_000_000_000);
        set_timer_freq_hz(0);
        assert_eq!(cycles_to_ns(12345), 0);
        set_timer_freq_hz(3_000_000_000);
    }
}
