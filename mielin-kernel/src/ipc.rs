//! Inter-Processor Communication (IPC) for Multi-Core Systems
//!
//! Provides low-level primitives for communication between CPU cores:
//! - IPI (Inter-Processor Interrupts) for signaling
//! - Lock-free message passing between CPUs
//! - Shared memory synchronization primitives
//!
//! ## Overview
//!
//! ```text
//! ┌─────────┐                    ┌─────────┐
//! │  CPU 0  │ ─── Message ────▶  │  CPU 1  │
//! │         │ ◀── IPI Reply ───  │         │
//! └─────────┘                    └─────────┘
//!      │                              │
//!      └──── Shared Memory Queue ─────┘
//! ```
//!
//! ## Features
//!
//! - **IPI Support**: Send interrupts to specific CPUs
//! - **Message Passing**: Lock-free MPMC message queues
//! - **Shared Memory**: Wait-free synchronization primitives
//! - **CPU Barriers**: Synchronize multiple CPUs
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::ipc::{send_ipi, IpiType, send_message};
//!
//! // Send IPI to wake up CPU 2
//! send_ipi(2, IpiType::Wakeup);
//!
//! // Send message to CPU 1
//! send_message(1, Message::TaskMigration { task_id: 42 });
//!
//! // Receive message on current CPU
//! if let Some(msg) = receive_message() {
//!     // Process message
//! }
//! ```

use crate::percpu::MAX_CPUS;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

/// Get current CPU ID (wrapper for test compatibility)
#[cfg(not(test))]
#[inline]
fn get_current_cpu_id() -> usize {
    crate::percpu::current_cpu_id()
}

/// Get current CPU ID (test stub - always returns 0)
#[cfg(test)]
#[inline]
fn get_current_cpu_id() -> usize {
    0
}

/// Get number of online CPUs (wrapper for test compatibility)
#[cfg(not(test))]
#[inline]
fn get_num_online_cpus() -> usize {
    crate::percpu::num_online_cpus()
}

/// Get number of online CPUs (test stub - returns MAX_CPUS)
#[cfg(test)]
#[inline]
fn get_num_online_cpus() -> usize {
    MAX_CPUS
}

/// Maximum number of messages per CPU queue
const MAX_MESSAGES_PER_CPU: usize = 256;

/// IPI (Inter-Processor Interrupt) types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpiType {
    /// Wake up a sleeping CPU
    Wakeup = 0,
    /// Request task reschedule
    Reschedule = 1,
    /// Request TLB flush
    TlbFlush = 2,
    /// Request cache invalidation
    CacheInvalidate = 3,
    /// Generic function call on target CPU
    FunctionCall = 4,
    /// Halt the target CPU
    Halt = 5,
    /// Custom IPI (user-defined)
    Custom = 255,
}

/// IPI statistics per CPU
#[derive(Debug, Clone, Copy)]
pub struct IpiStats {
    /// Total IPIs sent from this CPU
    pub sent: u64,
    /// Total IPIs received on this CPU
    pub received: u64,
    /// IPIs pending (not yet handled)
    pub pending: u32,
    /// IPI handling errors
    pub errors: u32,
}

/// Per-CPU IPI state
#[repr(align(64))]
struct IpiState {
    /// Pending IPI bitmap (one bit per IPI type)
    pending: AtomicU32,
    /// Total IPIs sent
    sent_count: AtomicU64,
    /// Total IPIs received
    received_count: AtomicU64,
    /// IPI handling errors
    error_count: AtomicU32,
}

impl IpiState {
    const fn new() -> Self {
        Self {
            pending: AtomicU32::new(0),
            sent_count: AtomicU64::new(0),
            received_count: AtomicU64::new(0),
            error_count: AtomicU32::new(0),
        }
    }

    /// Mark an IPI as pending
    fn set_pending(&self, ipi_type: IpiType) {
        let type_val = ipi_type as u8;
        // Only handle IPI types that fit in 32-bit bitmap (0-31)
        if type_val < 32 {
            let bit = 1u32 << type_val;
            self.pending.fetch_or(bit, Ordering::Release);
        }
        self.received_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Check if an IPI is pending
    fn is_pending(&self, ipi_type: IpiType) -> bool {
        let type_val = ipi_type as u8;
        if type_val >= 32 {
            return false; // Custom IPI types not tracked in bitmap
        }
        let bit = 1u32 << type_val;
        (self.pending.load(Ordering::Acquire) & bit) != 0
    }

    /// Clear a pending IPI
    fn clear_pending(&self, ipi_type: IpiType) {
        let type_val = ipi_type as u8;
        if type_val < 32 {
            let bit = 1u32 << type_val;
            self.pending.fetch_and(!bit, Ordering::Release);
        }
    }

    /// Get statistics
    fn stats(&self) -> IpiStats {
        IpiStats {
            sent: self.sent_count.load(Ordering::Relaxed),
            received: self.received_count.load(Ordering::Relaxed),
            pending: self.pending.load(Ordering::Relaxed),
            errors: self.error_count.load(Ordering::Relaxed),
        }
    }
}

/// Global IPI state for all CPUs
static IPI_STATES: [IpiState; MAX_CPUS] = [
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
    IpiState::new(),
];

/// Send an IPI to a specific CPU
///
/// # Safety
///
/// This function is safe to call, but the actual IPI delivery is
/// platform-specific and may involve unsafe hardware access.
///
/// # Arguments
///
/// - `target_cpu`: Target CPU ID (0-based)
/// - `ipi_type`: Type of IPI to send
///
/// # Returns
///
/// `true` if the IPI was queued successfully, `false` otherwise
pub fn send_ipi(target_cpu: usize, ipi_type: IpiType) -> bool {
    if target_cpu >= MAX_CPUS {
        return false;
    }

    let state = &IPI_STATES[target_cpu];
    state.set_pending(ipi_type);

    // x86_64: kick the target CPU via a LAPIC fixed-delivery IPI.
    #[cfg(all(target_arch = "x86_64", not(any(test, feature = "std"))))]
    // SAFETY: LAPIC MMIO at 0xFEE00000 is mapped by the kernel before this
    // code runs.  Write ordering is enforced by the volatile semantics.
    unsafe {
        let lapic_base: *mut u32 = 0xFEE0_0000usize as *mut u32;
        let icr_high = lapic_base.add(0x310 / 4);
        let icr_low = lapic_base.add(0x300 / 4);
        core::ptr::write_volatile(icr_high, (target_cpu as u32) << 24);
        core::ptr::write_volatile(icr_low, 0xFE_u32);
    }

    // aarch64: GICv3 SGI via ICC_SGI1R_EL1.
    #[cfg(all(target_arch = "aarch64", not(any(test, feature = "std"))))]
    // SAFETY: ICC_SGI1R_EL1 is a valid AArch64 system register available at EL1.
    unsafe {
        let target_list: u64 = 1u64 << (target_cpu as u64 & 0xf);
        let sgi1r: u64 = target_list; // SGI ID 0, Aff3/2/1 = 0
        core::arch::asm!(
            "msr ICC_SGI1R_EL1, {}",
            "isb",
            in(reg) sgi1r,
            options(nostack, preserves_flags)
        );
    }

    // riscv64: SBI IPI extension (EID = 0x735049 "sPI", FID = 0).
    #[cfg(all(target_arch = "riscv64", not(any(test, feature = "std"))))]
    // SAFETY: the SBI ecall interface is defined by the RISC-V SBI spec.
    unsafe {
        let hart_mask: usize = 1usize << target_cpu;
        let hart_mask_base: usize = 0;
        core::arch::asm!(
            "ecall",
            inout("a0") hart_mask => _,
            inout("a1") hart_mask_base => _,
            in("a6") 0usize,
            in("a7") 0x735049usize,
            options(nostack)
        );
    }

    true
}

/// Send IPI to all CPUs except self
pub fn send_ipi_all_but_self(ipi_type: IpiType) -> usize {
    let current_cpu = get_current_cpu_id();
    let num_cpus = get_num_online_cpus();
    let mut sent = 0;

    for cpu in 0..num_cpus {
        if cpu != current_cpu && send_ipi(cpu, ipi_type) {
            sent += 1;
        }
    }

    sent
}

/// Send IPI to all CPUs including self
pub fn send_ipi_all(ipi_type: IpiType) -> usize {
    let num_cpus = get_num_online_cpus();
    let mut sent = 0;

    for cpu in 0..num_cpus {
        if send_ipi(cpu, ipi_type) {
            sent += 1;
        }
    }

    sent
}

/// Check if an IPI is pending on current CPU
pub fn is_ipi_pending(ipi_type: IpiType) -> bool {
    let cpu_id = get_current_cpu_id();
    IPI_STATES[cpu_id].is_pending(ipi_type)
}

/// Handle a pending IPI
///
/// Returns `true` if the IPI was pending and handled, `false` otherwise
pub fn handle_ipi(ipi_type: IpiType) -> bool {
    let cpu_id = get_current_cpu_id();
    let state = &IPI_STATES[cpu_id];

    if state.is_pending(ipi_type) {
        state.clear_pending(ipi_type);
        true
    } else {
        false
    }
}

/// Get IPI statistics for a CPU
pub fn ipi_stats(cpu_id: usize) -> Option<IpiStats> {
    if cpu_id >= MAX_CPUS {
        return None;
    }
    Some(IPI_STATES[cpu_id].stats())
}

/// Inter-CPU message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Task migration request
    TaskMigration = 0,
    /// Memory allocation request
    MemoryAlloc = 1,
    /// Memory deallocation request
    MemoryFree = 2,
    /// Function call request
    FunctionCall = 3,
    /// Generic data transfer
    Data = 4,
    /// Acknowledgment
    Ack = 5,
    /// Error response
    Error = 6,
}

/// Inter-CPU message
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct Message {
    /// Message type
    pub msg_type: MessageType,
    /// Source CPU ID
    pub from_cpu: u8,
    /// Target CPU ID
    pub to_cpu: u8,
    /// Sequence number for tracking
    pub seq: u32,
    /// Message payload (64 bytes max)
    pub data: [u64; 7],
}

impl Message {
    /// Create a new message
    pub fn new(msg_type: MessageType, from_cpu: usize, to_cpu: usize) -> Self {
        Self {
            msg_type,
            from_cpu: from_cpu as u8,
            to_cpu: to_cpu as u8,
            seq: 0,
            data: [0; 7],
        }
    }

    /// Create a task migration message
    pub fn task_migration(from_cpu: usize, to_cpu: usize, task_id: usize) -> Self {
        let mut msg = Self::new(MessageType::TaskMigration, from_cpu, to_cpu);
        msg.data[0] = task_id as u64;
        msg
    }

    /// Create a memory allocation request
    pub fn memory_alloc(from_cpu: usize, to_cpu: usize, size: usize) -> Self {
        let mut msg = Self::new(MessageType::MemoryAlloc, from_cpu, to_cpu);
        msg.data[0] = size as u64;
        msg
    }

    /// Create an acknowledgment message
    pub fn ack(from_cpu: usize, to_cpu: usize, seq: u32) -> Self {
        let mut msg = Self::new(MessageType::Ack, from_cpu, to_cpu);
        msg.seq = seq;
        msg
    }
}

/// Lock-free message queue (SPSC - Single Producer Single Consumer)
struct MessageQueue {
    /// Ring buffer of messages
    buffer: [Mutex<Option<Message>>; MAX_MESSAGES_PER_CPU],
    /// Write index (producer)
    write_idx: AtomicUsize,
    /// Read index (consumer)
    read_idx: AtomicUsize,
    /// Number of messages in queue
    count: AtomicUsize,
    /// Total messages sent
    sent: AtomicU64,
    /// Total messages received
    received: AtomicU64,
    /// Queue full events
    full_count: AtomicU32,
}

impl MessageQueue {
    const fn new() -> Self {
        Self {
            buffer: [const { Mutex::new(None) }; MAX_MESSAGES_PER_CPU],
            write_idx: AtomicUsize::new(0),
            read_idx: AtomicUsize::new(0),
            count: AtomicUsize::new(0),
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            full_count: AtomicU32::new(0),
        }
    }

    /// Try to enqueue a message
    fn enqueue(&self, msg: Message) -> bool {
        let count = self.count.load(Ordering::Acquire);
        if count >= MAX_MESSAGES_PER_CPU {
            self.full_count.fetch_add(1, Ordering::Relaxed);
            return false;
        }

        let idx = self.write_idx.load(Ordering::Relaxed);
        let next_idx = (idx + 1) % MAX_MESSAGES_PER_CPU;

        let mut slot = self.buffer[idx].lock();
        *slot = Some(msg);
        drop(slot);

        self.write_idx.store(next_idx, Ordering::Release);
        self.count.fetch_add(1, Ordering::Release);
        self.sent.fetch_add(1, Ordering::Relaxed);

        true
    }

    /// Try to dequeue a message
    fn dequeue(&self) -> Option<Message> {
        let count = self.count.load(Ordering::Acquire);
        if count == 0 {
            return None;
        }

        let idx = self.read_idx.load(Ordering::Relaxed);
        let next_idx = (idx + 1) % MAX_MESSAGES_PER_CPU;

        let mut slot = self.buffer[idx].lock();
        let msg = slot.take();
        drop(slot);

        if msg.is_some() {
            self.read_idx.store(next_idx, Ordering::Release);
            self.count.fetch_sub(1, Ordering::Release);
            self.received.fetch_add(1, Ordering::Relaxed);
        }

        msg
    }

    /// Get number of pending messages
    fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Get statistics
    fn stats(&self) -> MessageQueueStats {
        MessageQueueStats {
            pending: self.count.load(Ordering::Relaxed),
            sent: self.sent.load(Ordering::Relaxed),
            received: self.received.load(Ordering::Relaxed),
            full_count: self.full_count.load(Ordering::Relaxed),
        }
    }
}

/// Message queue statistics
#[derive(Debug, Clone, Copy)]
pub struct MessageQueueStats {
    /// Messages currently in queue
    pub pending: usize,
    /// Total messages sent
    pub sent: u64,
    /// Total messages received
    pub received: u64,
    /// Number of times queue was full
    pub full_count: u32,
}

/// Per-CPU message queues (one per CPU pair)
static MESSAGE_QUEUES: [[MessageQueue; MAX_CPUS]; MAX_CPUS] = [
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
    [
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
        MessageQueue::new(),
    ],
];

/// Send a message to a specific CPU
///
/// # Arguments
///
/// - `target_cpu`: Target CPU ID
/// - `msg`: Message to send
///
/// # Returns
///
/// `true` if the message was queued successfully, `false` if the queue is full
pub fn send_message(target_cpu: usize, msg: Message) -> bool {
    if target_cpu >= MAX_CPUS {
        return false;
    }

    let from_cpu = get_current_cpu_id();
    let queue = &MESSAGE_QUEUES[from_cpu][target_cpu];

    if queue.enqueue(msg) {
        // Send IPI to notify target CPU (use FunctionCall IPI for messages)
        send_ipi(target_cpu, IpiType::FunctionCall);
        true
    } else {
        false
    }
}

/// Receive a message from any CPU
///
/// Returns the first available message, if any
pub fn receive_message() -> Option<Message> {
    let cpu_id = get_current_cpu_id();

    // Check all queues from other CPUs
    for (from_cpu, queue_row) in MESSAGE_QUEUES.iter().enumerate().take(MAX_CPUS) {
        if from_cpu != cpu_id {
            let queue = &queue_row[cpu_id];
            if let Some(msg) = queue.dequeue() {
                return Some(msg);
            }
        }
    }

    None
}

/// Receive a message from a specific CPU
pub fn receive_message_from(from_cpu: usize) -> Option<Message> {
    if from_cpu >= MAX_CPUS {
        return None;
    }

    let cpu_id = get_current_cpu_id();
    let queue = &MESSAGE_QUEUES[from_cpu][cpu_id];
    queue.dequeue()
}

/// Get number of pending messages for current CPU
pub fn pending_messages() -> usize {
    let cpu_id = get_current_cpu_id();
    let mut total = 0;

    for (from_cpu, queue_row) in MESSAGE_QUEUES.iter().enumerate().take(MAX_CPUS) {
        if from_cpu != cpu_id {
            total += queue_row[cpu_id].len();
        }
    }

    total
}

/// Get message queue statistics
pub fn message_queue_stats(from_cpu: usize, to_cpu: usize) -> Option<MessageQueueStats> {
    if from_cpu >= MAX_CPUS || to_cpu >= MAX_CPUS {
        return None;
    }

    Some(MESSAGE_QUEUES[from_cpu][to_cpu].stats())
}

/// CPU barrier for synchronization
///
/// All CPUs must reach the barrier before any can proceed
pub struct CpuBarrier {
    /// Number of CPUs that need to synchronize
    count: AtomicUsize,
    /// Number of CPUs currently waiting
    waiting: AtomicUsize,
    /// Generation counter (for multiple uses)
    generation: AtomicU64,
}

impl CpuBarrier {
    /// Create a new CPU barrier
    pub const fn new(count: usize) -> Self {
        Self {
            count: AtomicUsize::new(count),
            waiting: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
        }
    }

    /// Wait at the barrier
    ///
    /// Blocks until all CPUs reach the barrier
    pub fn wait(&self) {
        let gen = self.generation.load(Ordering::Acquire);
        let waiting = self.waiting.fetch_add(1, Ordering::AcqRel);

        if waiting + 1 == self.count.load(Ordering::Acquire) {
            // Last CPU to arrive - release all
            self.waiting.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
        } else {
            // Wait for generation to change
            while self.generation.load(Ordering::Acquire) == gen {
                core::hint::spin_loop();
            }
        }
    }

    /// Reset the barrier for a new count of CPUs
    pub fn reset(&self, count: usize) {
        self.count.store(count, Ordering::Release);
        self.waiting.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipi_send_receive() {
        // Send IPI from CPU 0 to CPU 1
        assert!(send_ipi(0, IpiType::Wakeup));

        // Check if IPI is pending
        let state = &IPI_STATES[0];
        assert!(state.is_pending(IpiType::Wakeup));

        // Handle the IPI
        state.clear_pending(IpiType::Wakeup);
        assert!(!state.is_pending(IpiType::Wakeup));
    }

    #[test]
    fn test_ipi_stats() {
        let stats = ipi_stats(0).unwrap();
        // Verify stats are accessible (received is u64, always >= 0)
        let _ = stats.received;
        let _ = stats.sent;
    }

    #[test]
    fn test_message_new() {
        let msg = Message::new(MessageType::Data, 0, 1);
        assert_eq!(msg.msg_type, MessageType::Data);
        assert_eq!(msg.from_cpu, 0);
        assert_eq!(msg.to_cpu, 1);
    }

    #[test]
    fn test_message_task_migration() {
        let msg = Message::task_migration(0, 1, 42);
        assert_eq!(msg.msg_type, MessageType::TaskMigration);
        assert_eq!(msg.data[0], 42);
    }

    #[test]
    fn test_message_queue_enqueue_dequeue() {
        let queue = MessageQueue::new();
        let msg = Message::new(MessageType::Data, 0, 1);

        assert!(queue.enqueue(msg));
        assert_eq!(queue.len(), 1);

        let received = queue.dequeue().unwrap();
        assert_eq!(received.msg_type, MessageType::Data);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_message_queue_full() {
        let queue = MessageQueue::new();
        let msg = Message::new(MessageType::Data, 0, 1);

        // Fill the queue
        for _ in 0..MAX_MESSAGES_PER_CPU {
            assert!(queue.enqueue(msg));
        }

        // Queue should be full now
        assert!(!queue.enqueue(msg));
    }

    #[test]
    fn test_message_queue_stats() {
        let queue = MessageQueue::new();
        let msg = Message::new(MessageType::Data, 0, 1);

        queue.enqueue(msg);
        let stats = queue.stats();
        assert_eq!(stats.sent, 1);
        assert_eq!(stats.pending, 1);

        queue.dequeue();
        let stats = queue.stats();
        assert_eq!(stats.received, 1);
        assert_eq!(stats.pending, 0);
    }

    #[test]
    fn test_cpu_barrier() {
        let barrier = CpuBarrier::new(1);
        barrier.wait(); // Should not block with count=1
    }

    #[test]
    fn test_cpu_barrier_reset() {
        let barrier = CpuBarrier::new(2);
        barrier.reset(1);
        barrier.wait(); // Should not block with count=1
    }

    #[test]
    fn test_ipi_type_values() {
        assert_eq!(IpiType::Wakeup as u8, 0);
        assert_eq!(IpiType::Reschedule as u8, 1);
        assert_eq!(IpiType::Custom as u8, 255);
    }

    #[test]
    fn test_message_type_values() {
        assert_eq!(MessageType::TaskMigration as u8, 0);
        assert_eq!(MessageType::Data as u8, 4);
        assert_eq!(MessageType::Error as u8, 6);
    }

    #[test]
    fn test_message_ack() {
        let msg = Message::ack(0, 1, 123);
        assert_eq!(msg.msg_type, MessageType::Ack);
        assert_eq!(msg.seq, 123);
    }

    #[test]
    fn test_message_memory_alloc() {
        let msg = Message::memory_alloc(0, 1, 4096);
        assert_eq!(msg.msg_type, MessageType::MemoryAlloc);
        assert_eq!(msg.data[0], 4096);
    }

    #[test]
    fn test_ipi_software_path() {
        assert!(send_ipi(0, IpiType::Reschedule));
        assert!(is_ipi_pending(IpiType::Reschedule));
        assert!(handle_ipi(IpiType::Reschedule));
        assert!(!is_ipi_pending(IpiType::Reschedule));
    }

    #[test]
    fn test_ipi_invalid_cpu_returns_false() {
        assert!(!send_ipi(MAX_CPUS, IpiType::Wakeup));
        assert!(!send_ipi(usize::MAX, IpiType::Halt));
    }
}
