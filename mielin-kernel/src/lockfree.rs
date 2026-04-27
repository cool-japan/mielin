//! Lock-Free Data Structures for MielinOS Kernel
//!
//! Provides high-performance concurrent data structures without traditional locks,
//! using atomic operations for synchronization.
//!
//! ## Features
//!
//! - **Lock-Free Queue**: MPSC (Multi-Producer Single-Consumer) queue
//! - **Lock-Free Stack**: MPMC (Multi-Producer Multi-Consumer) stack
//! - **Atomic Counter**: High-performance atomic counter with statistics
//! - **Sequence Lock**: Reader-writer lock optimized for frequent reads
//!
//! ## Safety
//!
//! These structures use atomic operations and careful memory ordering to ensure
//! correctness without locks. They are safe for use in interrupt handlers and
//! across multiple CPUs.

extern crate alloc;

use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};

/// A node in a lock-free linked structure
struct Node<T> {
    data: Option<T>,
    next: AtomicPtr<Node<T>>,
}

impl<T> Node<T> {
    fn new(data: T) -> Self {
        Self {
            data: Some(data),
            next: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    fn sentinel() -> Self {
        Self {
            data: None,
            next: AtomicPtr::new(core::ptr::null_mut()),
        }
    }
}

/// Lock-Free MPSC Queue (Multi-Producer Single-Consumer)
///
/// Based on Michael-Scott queue algorithm. Allows multiple producers
/// to enqueue concurrently while a single consumer dequeues.
///
/// # Example
///
/// ```rust,ignore
/// use mielin_kernel::lockfree::LockFreeQueue;
///
/// let queue = LockFreeQueue::new();
/// queue.push(1);
/// queue.push(2);
/// assert_eq!(queue.pop(), Some(1));
/// assert_eq!(queue.pop(), Some(2));
/// ```
pub struct LockFreeQueue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>,
    len: AtomicUsize,
}

impl<T> LockFreeQueue<T> {
    /// Create a new empty queue
    pub fn new() -> Self {
        let sentinel = Box::into_raw(Box::new(Node::<T>::sentinel()));
        Self {
            head: AtomicPtr::new(sentinel),
            tail: AtomicPtr::new(sentinel),
            len: AtomicUsize::new(0),
        }
    }

    /// Push an item to the back of the queue
    ///
    /// This operation is lock-free and can be called from multiple threads.
    pub fn push(&self, data: T) {
        let node = Box::into_raw(Box::new(Node::new(data)));

        loop {
            let tail = self.tail.load(Ordering::Acquire);
            // SAFETY: tail is always a valid pointer to a Node that was allocated via Box.
            // The sentinel node is kept alive for the lifetime of the queue. Subsequent nodes
            // are only freed in pop() after they've been unlinked via successful CAS.
            let tail_ref = unsafe { &*tail };
            let next = tail_ref.next.load(Ordering::Acquire);

            if next.is_null() {
                // Try to link new node at the end
                if tail_ref
                    .next
                    .compare_exchange(
                        core::ptr::null_mut(),
                        node,
                        Ordering::Release,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    // Successfully linked, try to swing tail
                    let _ = self.tail.compare_exchange(
                        tail,
                        node,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    self.len.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            } else {
                // Tail is falling behind, try to advance it
                let _ =
                    self.tail
                        .compare_exchange(tail, next, Ordering::Release, Ordering::Relaxed);
            }
            core::hint::spin_loop();
        }
    }

    /// Pop an item from the front of the queue
    ///
    /// Returns `None` if the queue is empty.
    /// This operation is lock-free but should be called from a single consumer.
    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            // SAFETY: head is always a valid pointer. It starts as the sentinel node and is
            // only updated via successful CAS to point to the next node, which was previously
            // validated as non-null.
            let head_ref = unsafe { &*head };
            let next = head_ref.next.load(Ordering::Acquire);

            if head == tail {
                if next.is_null() {
                    // Queue is empty
                    return None;
                }
                // Tail is falling behind, advance it
                let _ =
                    self.tail
                        .compare_exchange(tail, next, Ordering::Release, Ordering::Relaxed);
            } else if !next.is_null() {
                // Read value before CAS
                // SAFETY: next was checked to be non-null above. The node was allocated via Box
                // and is still linked in the queue. Mutable access is safe because only this
                // consumer thread modifies data, and the node won't be freed until after CAS.
                let next_ref = unsafe { &mut *next };

                // Try to swing head to next node
                if self
                    .head
                    .compare_exchange(head, next, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
                {
                    // Successfully moved head, free old node
                    // SAFETY: head was previously the valid sentinel or a node that has now been
                    // unlinked. The CAS succeeded, so no other thread can access this node.
                    // It was originally allocated with Box::into_raw, so from_raw is safe.
                    unsafe {
                        let _ = Box::from_raw(head);
                    }
                    self.len.fetch_sub(1, Ordering::Relaxed);
                    return next_ref.data.take();
                }
            }
            core::hint::spin_loop();
        }
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.len.load(Ordering::Relaxed) == 0
    }

    /// Get the approximate length of the queue
    ///
    /// Note: This may not be accurate in the presence of concurrent modifications.
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }
}

impl<T> Default for LockFreeQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for LockFreeQueue<T> {
    fn drop(&mut self) {
        // Drain all remaining nodes
        while self.pop().is_some() {}

        // Free the sentinel node
        let head = self.head.load(Ordering::Relaxed);
        if !head.is_null() {
            // SAFETY: The queue is being dropped, so we have exclusive access.
            // The sentinel was allocated via Box::into_raw in new().
            unsafe {
                let _ = Box::from_raw(head);
            }
        }
    }
}

// SAFETY: Queue uses atomic operations for all shared state. The T: Send bound
// ensures that T can be safely transferred between threads. All node access is
// protected by atomic compare-and-swap operations with proper memory ordering.
unsafe impl<T: Send> Send for LockFreeQueue<T> {}
unsafe impl<T: Send> Sync for LockFreeQueue<T> {}

/// Lock-Free Stack (Treiber Stack)
///
/// MPMC (Multi-Producer Multi-Consumer) stack using compare-and-swap.
///
/// # Example
///
/// ```rust,ignore
/// use mielin_kernel::lockfree::LockFreeStack;
///
/// let stack = LockFreeStack::new();
/// stack.push(1);
/// stack.push(2);
/// assert_eq!(stack.pop(), Some(2)); // LIFO order
/// assert_eq!(stack.pop(), Some(1));
/// ```
pub struct LockFreeStack<T> {
    top: AtomicPtr<Node<T>>,
    len: AtomicUsize,
}

impl<T> LockFreeStack<T> {
    /// Create a new empty stack
    pub fn new() -> Self {
        Self {
            top: AtomicPtr::new(core::ptr::null_mut()),
            len: AtomicUsize::new(0),
        }
    }

    /// Push an item onto the stack
    pub fn push(&self, data: T) {
        let node = Box::into_raw(Box::new(Node::new(data)));

        loop {
            let top = self.top.load(Ordering::Acquire);
            // SAFETY: node was just allocated via Box::into_raw above, so it's a valid pointer.
            // We own the node exclusively until the CAS succeeds.
            unsafe {
                (*node).next.store(top, Ordering::Relaxed);
            }

            if self
                .top
                .compare_exchange(top, node, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                self.len.fetch_add(1, Ordering::Relaxed);
                return;
            }
            core::hint::spin_loop();
        }
    }

    /// Pop an item from the stack
    pub fn pop(&self) -> Option<T> {
        loop {
            let top = self.top.load(Ordering::Acquire);
            if top.is_null() {
                return None;
            }

            // SAFETY: top was checked to be non-null above. The node was allocated via Box
            // and is currently the top of the stack. It won't be freed until after CAS succeeds.
            let top_ref = unsafe { &*top };
            let next = top_ref.next.load(Ordering::Acquire);

            if self
                .top
                .compare_exchange(top, next, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                self.len.fetch_sub(1, Ordering::Relaxed);
                // SAFETY: The CAS succeeded, so we now own the node exclusively.
                // It was allocated via Box::into_raw in push().
                let node = unsafe { Box::from_raw(top) };
                return node.data;
            }
            core::hint::spin_loop();
        }
    }

    /// Check if the stack is empty
    pub fn is_empty(&self) -> bool {
        self.top.load(Ordering::Relaxed).is_null()
    }

    /// Get the approximate length
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }
}

impl<T> Default for LockFreeStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for LockFreeStack<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

// SAFETY: Stack uses atomic operations for all shared state. The T: Send bound
// ensures that T can be safely transferred between threads. All node access is
// protected by atomic compare-and-swap operations with proper memory ordering.
unsafe impl<T: Send> Send for LockFreeStack<T> {}
unsafe impl<T: Send> Sync for LockFreeStack<T> {}

/// High-Performance Atomic Counter with Statistics
///
/// Provides a counter with min/max tracking and overflow protection.
pub struct AtomicCounter {
    value: AtomicU64,
    total_increments: AtomicU64,
    total_decrements: AtomicU64,
    peak_value: AtomicU64,
}

impl AtomicCounter {
    /// Create a new counter initialized to 0
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
            total_increments: AtomicU64::new(0),
            total_decrements: AtomicU64::new(0),
            peak_value: AtomicU64::new(0),
        }
    }

    /// Create a counter with an initial value
    pub const fn with_value(initial: u64) -> Self {
        Self {
            value: AtomicU64::new(initial),
            total_increments: AtomicU64::new(0),
            total_decrements: AtomicU64::new(0),
            peak_value: AtomicU64::new(initial),
        }
    }

    /// Increment the counter by 1
    pub fn increment(&self) -> u64 {
        self.add(1)
    }

    /// Decrement the counter by 1 (saturating)
    pub fn decrement(&self) -> u64 {
        self.sub(1)
    }

    /// Add a value to the counter
    pub fn add(&self, val: u64) -> u64 {
        let new_val = self.value.fetch_add(val, Ordering::Relaxed) + val;
        self.total_increments.fetch_add(1, Ordering::Relaxed);

        // Update peak if needed
        loop {
            let peak = self.peak_value.load(Ordering::Relaxed);
            if new_val <= peak {
                break;
            }
            if self
                .peak_value
                .compare_exchange(peak, new_val, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }

        new_val
    }

    /// Subtract a value from the counter (saturating)
    pub fn sub(&self, val: u64) -> u64 {
        loop {
            let current = self.value.load(Ordering::Relaxed);
            let new_val = current.saturating_sub(val);

            if self
                .value
                .compare_exchange(current, new_val, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                self.total_decrements.fetch_add(1, Ordering::Relaxed);
                return new_val;
            }
        }
    }

    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Set the value directly
    pub fn set(&self, val: u64) {
        self.value.store(val, Ordering::Relaxed);

        // Update peak if needed
        loop {
            let peak = self.peak_value.load(Ordering::Relaxed);
            if val <= peak {
                break;
            }
            if self
                .peak_value
                .compare_exchange(peak, val, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }

    /// Get the peak value ever recorded
    pub fn peak(&self) -> u64 {
        self.peak_value.load(Ordering::Relaxed)
    }

    /// Get the total number of increments
    pub fn total_increments(&self) -> u64 {
        self.total_increments.load(Ordering::Relaxed)
    }

    /// Get the total number of decrements
    pub fn total_decrements(&self) -> u64 {
        self.total_decrements.load(Ordering::Relaxed)
    }

    /// Reset statistics (but not value)
    pub fn reset_stats(&self) {
        self.total_increments.store(0, Ordering::Relaxed);
        self.total_decrements.store(0, Ordering::Relaxed);
        self.peak_value
            .store(self.value.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}

impl Default for AtomicCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Sequence Lock for Reader-Writer Synchronization
///
/// Optimized for frequent reads with occasional writes.
/// Readers never block but may need to retry if a write occurs.
///
/// # Example
///
/// ```rust,ignore
/// use mielin_kernel::lockfree::SeqLock;
///
/// let lock = SeqLock::new([1, 2, 3, 4]);
///
/// // Reading
/// let data = lock.read();
///
/// // Writing
/// lock.write([5, 6, 7, 8]);
/// ```
pub struct SeqLock<T: Copy> {
    sequence: AtomicUsize,
    data: UnsafeCell<T>,
}

impl<T: Copy> SeqLock<T> {
    /// Create a new sequence lock with initial data
    pub const fn new(data: T) -> Self {
        Self {
            sequence: AtomicUsize::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Read the data
    ///
    /// May retry internally if a write is in progress.
    pub fn read(&self) -> T {
        loop {
            let seq1 = self.sequence.load(Ordering::Acquire);

            // If odd, a write is in progress - retry
            if seq1 & 1 != 0 {
                core::hint::spin_loop();
                continue;
            }

            // Read the data
            // SAFETY: We checked that sequence is even (no write in progress).
            // T: Copy ensures the read is safe even if a concurrent write starts,
            // because we'll detect it via the sequence check and retry.
            let data = unsafe { *self.data.get() };

            // Check if sequence changed during read
            let seq2 = self.sequence.load(Ordering::Acquire);
            if seq1 == seq2 {
                return data;
            }
            core::hint::spin_loop();
        }
    }

    /// Write new data
    ///
    /// Note: Only one writer should be active at a time.
    /// For multiple writers, external synchronization is needed.
    pub fn write(&self, data: T) {
        // Increment sequence to odd (write in progress)
        loop {
            let seq = self.sequence.load(Ordering::Relaxed);
            if self
                .sequence
                .compare_exchange(
                    seq,
                    seq.wrapping_add(1),
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }

        // Write the data
        // SAFETY: We hold the write lock (sequence is odd). The CAS succeeded,
        // ensuring exclusive access. External synchronization is required for
        // multiple writers per the documentation.
        unsafe {
            *self.data.get() = data;
        }

        // Increment sequence to even (write complete)
        self.sequence.fetch_add(1, Ordering::Release);
    }

    /// Try to read without retrying
    ///
    /// Returns `None` if a write is in progress or occurred during read.
    pub fn try_read(&self) -> Option<T> {
        let seq1 = self.sequence.load(Ordering::Acquire);

        if seq1 & 1 != 0 {
            return None;
        }

        // SAFETY: Same as read() - sequence was even, T: Copy makes the read safe,
        // and we validate no write occurred by checking sequence again.
        let data = unsafe { *self.data.get() };

        let seq2 = self.sequence.load(Ordering::Acquire);
        if seq1 == seq2 {
            Some(data)
        } else {
            None
        }
    }

    /// Get the current sequence number
    pub fn sequence(&self) -> usize {
        self.sequence.load(Ordering::Relaxed)
    }
}

// SAFETY: SeqLock uses atomic operations for sequence and UnsafeCell for data.
// The T: Copy + Send bounds ensure T can be safely copied and sent between threads.
// The sequence number protocol ensures readers never see torn writes.
unsafe impl<T: Copy + Send> Send for SeqLock<T> {}
unsafe impl<T: Copy + Send> Sync for SeqLock<T> {}

/// Lock-Free Ring Buffer
///
/// Fixed-size circular buffer for high-performance bounded queues.
/// SPSC (Single-Producer Single-Consumer) variant for maximum performance.
pub struct RingBuffer<T, const N: usize> {
    buffer: UnsafeCell<[Option<T>; N]>,
    head: AtomicUsize, // Read position
    tail: AtomicUsize, // Write position
}

impl<T, const N: usize> RingBuffer<T, N> {
    /// Create a new ring buffer
    ///
    /// Note: Capacity must be a power of 2 for optimal performance.
    pub fn new() -> Self {
        Self {
            buffer: UnsafeCell::new([const { None }; N]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Try to push an item
    ///
    /// Returns `Err(item)` if the buffer is full.
    pub fn try_push(&self, item: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        let next_tail = (tail + 1) % N;
        if next_tail == head {
            return Err(item); // Buffer full
        }

        // SAFETY: This is SPSC - only one producer can write to tail position.
        // We verified there's space (next_tail != head). The Release store on
        // tail ensures the write is visible before consumers can read it.
        unsafe {
            (*self.buffer.get())[tail] = Some(item);
        }
        self.tail.store(next_tail, Ordering::Release);

        Ok(())
    }

    /// Try to pop an item
    ///
    /// Returns `None` if the buffer is empty.
    pub fn try_pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None; // Buffer empty
        }

        // SAFETY: This is SPSC - only one consumer can read from head position.
        // We verified there's data (head != tail). The Acquire load on tail
        // ensures we see the producer's write before we read it.
        let item = unsafe { (*self.buffer.get())[head].take() };
        self.head.store((head + 1) % N, Ordering::Release);

        item
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Relaxed) == self.tail.load(Ordering::Relaxed)
    }

    /// Check if the buffer is full
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Relaxed);
        (tail + 1) % N == head
    }

    /// Get the number of items in the buffer
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Relaxed);
        (tail + N - head) % N
    }

    /// Get the capacity of the buffer
    pub const fn capacity(&self) -> usize {
        N - 1 // One slot is always empty to distinguish full from empty
    }
}

impl<T, const N: usize> Default for RingBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: RingBuffer is SPSC (Single-Producer Single-Consumer). The T: Send bound
// ensures T can be safely transferred between threads. Head and tail use atomic
// operations with proper memory ordering. Users must ensure only one producer
// calls try_push and only one consumer calls try_pop concurrently.
unsafe impl<T: Send, const N: usize> Send for RingBuffer<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for RingBuffer<T, N> {}

// =============================================================================
// Chase-Lev Work-Stealing Deque
// =============================================================================

/// Result of a steal attempt from another worker's deque.
pub enum Steal<T> {
    /// Successfully stole an item
    Success(T),
    /// The deque was empty at the time of stealing
    Empty,
    /// A concurrent steal or push raced; caller should retry
    Retry,
}

/// Internal circular buffer for the Chase-Lev deque.
///
/// The buffer is always power-of-2 sized so index masking replaces modulo.
/// Items live in `UnsafeCell` slots; the atomic `top`/`bottom` indices on
/// the deque itself guard exclusive access per the paper's protocol.
struct ChaseLevBuffer<T> {
    slots: alloc::vec::Vec<UnsafeCell<core::mem::MaybeUninit<T>>>,
    mask: usize,
}

impl<T> ChaseLevBuffer<T> {
    fn new(capacity: usize) -> Self {
        debug_assert!(capacity.is_power_of_two());
        let mut slots = alloc::vec::Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(UnsafeCell::new(core::mem::MaybeUninit::uninit()));
        }
        Self {
            mask: capacity - 1,
            slots,
        }
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Write item at logical index `i` (owner only, no concurrent write).
    ///
    /// # Safety
    /// Called only by the owner when the slot is logically owned (below bottom).
    #[inline]
    unsafe fn write(&self, i: usize, val: T) {
        let slot = self.slots[i & self.mask].get();
        (*slot).write(val);
    }

    /// Read item at logical index `i`.
    ///
    /// # Safety
    /// The slot must have been written before and not yet invalidated by a
    /// concurrent steal that advanced `top` past `i`.
    #[inline]
    unsafe fn read(&self, i: usize) -> T {
        let slot = self.slots[i & self.mask].get();
        (*slot).assume_init_read()
    }
}

// SAFETY: T: Send means items can cross thread boundaries.  The buffer
// itself is only accessed through the protocol-ordained atomic indices.
unsafe impl<T: Send> Send for ChaseLevBuffer<T> {}
unsafe impl<T: Send> Sync for ChaseLevBuffer<T> {}

/// Chase-Lev single-owner / multi-thief work-stealing deque.
///
/// Owner thread uses `push` / `pop` (LIFO from the bottom end).
/// Thief threads use `steal` (FIFO from the top end).
///
/// Memory orderings follow the 2005 Chase-Lev paper and the 2013 correction
/// by Lê, Pop, Cohen & Nardelli (SC fence at steal boundaries).
pub struct WorkStealingDeque<T: Send> {
    /// Index of the next slot the owner will push into (exclusive bottom).
    bottom: AtomicUsize,
    /// Index of the oldest item available for stealing (inclusive top).
    top: AtomicUsize,
    /// Pointer to the live buffer; replaced on growth.
    ///
    /// We never shrink.  Old buffers are leaked intentionally because a
    /// thief may still be reading from one while the owner has moved on.
    /// In a production kernel, hazard pointers would reclaim them; for
    /// correctness we accept the static-lifetime leak (bounded by growth factor).
    buffer: AtomicPtr<ChaseLevBuffer<T>>,
}

impl<T: Send> WorkStealingDeque<T> {
    /// Create a new deque with the given initial capacity (rounded up to power of 2).
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        let buf = Box::into_raw(Box::new(ChaseLevBuffer::new(cap)));
        Self {
            bottom: AtomicUsize::new(0),
            top: AtomicUsize::new(0),
            buffer: AtomicPtr::new(buf),
        }
    }

    /// Push an item onto the bottom of the deque.  Owner thread only.
    ///
    /// Grows the buffer by 2× when full, leaking the old buffer.
    pub fn push(&self, item: T) {
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Acquire);

        // SAFETY: buffer is set in new() and only replaced (never freed) in push().
        let buf = unsafe { &*self.buffer.load(Ordering::Relaxed) };

        // Grow if the deque is about to overflow the buffer.
        if b.wrapping_sub(t) >= buf.capacity() {
            self.grow(b, t, buf);
        }

        let buf = unsafe { &*self.buffer.load(Ordering::Relaxed) };
        // SAFETY: slot `b` is logically owned by the owner (bottom end); no thief
        // can reach it because top ≤ b.
        unsafe { buf.write(b, item) };

        // Release so a subsequent steal's Acquire on `bottom` sees the write.
        self.bottom.store(b.wrapping_add(1), Ordering::Release);
    }

    /// Pop an item from the bottom of the deque.  Owner thread only.
    ///
    /// Returns `None` when the deque is empty (including after racing with steals).
    pub fn pop(&self) -> Option<T> {
        let b = self.bottom.load(Ordering::Relaxed);
        // SAFETY: buffer never freed; see push().
        let buf = unsafe { &*self.buffer.load(Ordering::Relaxed) };

        // Pre-check: if the deque is already empty, skip the decrement entirely.
        // This guards against the wrapping_sub(0) == usize::MAX edge case where
        // an unsigned comparison `t > b.wrapping_sub(1)` would fail to detect empty.
        let t_pre = self.top.load(Ordering::Relaxed);
        if b == t_pre {
            return None;
        }

        let b = b.wrapping_sub(1);
        self.bottom.store(b, Ordering::Relaxed);
        // SeqCst fence required so that the load of `top` below is ordered
        // after the store of `bottom` above from the perspective of all threads.
        core::sync::atomic::fence(Ordering::SeqCst);

        let t = self.top.load(Ordering::Relaxed);

        // After decrement the invariant is: if t > b, deque was concurrently drained.
        if t > b {
            self.bottom.store(b.wrapping_add(1), Ordering::Relaxed);
            return None;
        }

        // SAFETY: slot `b` is within [top, bottom) so it contains a valid item.
        let item = unsafe { buf.read(b) };

        if t == b {
            // Single item left; race with potential concurrent steals.
            if self
                .top
                .compare_exchange(t, t.wrapping_add(1), Ordering::SeqCst, Ordering::Relaxed)
                .is_err()
            {
                // A thief already took it.
                self.bottom.store(b.wrapping_add(1), Ordering::Relaxed);
                return None;
            }
            self.bottom.store(b.wrapping_add(1), Ordering::Relaxed);
        }

        Some(item)
    }

    /// Steal an item from the top of the deque.  Any thread may call this.
    ///
    /// Returns `Steal::Retry` on ABA / concurrent contention.
    pub fn steal(&self) -> Steal<T> {
        let t = self.top.load(Ordering::Acquire);
        // SeqCst fence to establish ordering between this load and the
        // owner's store of bottom (corrects the race described in the 2013 paper).
        core::sync::atomic::fence(Ordering::SeqCst);
        let b = self.bottom.load(Ordering::Acquire);

        if t >= b {
            return Steal::Empty;
        }

        // SAFETY: slot `t` is within [top, bottom) so it contains a valid item.
        let buf = unsafe { &*self.buffer.load(Ordering::Acquire) };
        let item = unsafe { buf.read(t) };

        // CAS top from t to t+1.  AcqRel so the read above isn't reordered after.
        match self
            .top
            .compare_exchange(t, t.wrapping_add(1), Ordering::AcqRel, Ordering::Relaxed)
        {
            Ok(_) => Steal::Success(item),
            Err(_) => {
                // Another thief or the owner won; item is still valid but we
                // must not expose it — just signal retry.
                core::mem::forget(item);
                Steal::Retry
            }
        }
    }

    /// Return the approximate number of items in the deque.
    pub fn len(&self) -> usize {
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Relaxed);
        b.wrapping_sub(t)
    }

    /// Return true if the deque appears empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Double the buffer capacity, copying live items from the old buffer.
    ///
    /// Only ever called by the owner, so no concurrent push/pop races here.
    fn grow(&self, b: usize, t: usize, old: &ChaseLevBuffer<T>) {
        let new_cap = old.capacity() * 2;
        let new_buf = Box::into_raw(Box::new(ChaseLevBuffer::new(new_cap)));
        // Copy live items [t, b).
        for i in t..b {
            // SAFETY: items [t,b) are live in old buffer; new buffer owns the slots.
            unsafe {
                let val = old.read(i);
                (*new_buf).write(i, val);
            }
        }
        // Release so thieves that load the buffer pointer after this see the copies.
        self.buffer.store(new_buf, Ordering::Release);
        // Intentionally leak old: a thief may still hold a raw reference to it.
    }
}

impl<T: Send> Drop for WorkStealingDeque<T> {
    fn drop(&mut self) {
        let b = *self.bottom.get_mut();
        let t = *self.top.get_mut();
        // SAFETY: we have exclusive access (drop is single-threaded).
        let buf = unsafe { &*self.buffer.load(Ordering::Relaxed) };
        // Drop remaining live items to avoid leaks.
        for i in t..b {
            // SAFETY: items [t,b) are live.
            unsafe {
                let _ = buf.read(i);
            }
        }
        // Free the current buffer.
        let raw = self.buffer.load(Ordering::Relaxed);
        if !raw.is_null() {
            // SAFETY: allocated via Box::into_raw in new() or grow().
            unsafe { drop(Box::from_raw(raw)) };
        }
    }
}

// SAFETY: T: Send satisfies cross-thread transfer.  The deque itself is
// always accessed through the atomic protocol documented above.
unsafe impl<T: Send> Send for WorkStealingDeque<T> {}
unsafe impl<T: Send> Sync for WorkStealingDeque<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn test_queue_basic() {
        let queue = LockFreeQueue::new();
        assert!(queue.is_empty());

        queue.push(1);
        queue.push(2);
        queue.push(3);

        assert_eq!(queue.len(), 3);
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_queue_fifo_order() {
        let queue = LockFreeQueue::new();

        for i in 0..10 {
            queue.push(i);
        }

        for i in 0..10 {
            assert_eq!(queue.pop(), Some(i));
        }
    }

    #[test]
    fn test_queue_interleaved() {
        let queue = LockFreeQueue::new();

        queue.push(1);
        assert_eq!(queue.pop(), Some(1));

        queue.push(2);
        queue.push(3);
        assert_eq!(queue.pop(), Some(2));

        queue.push(4);
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), Some(4));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_stack_basic() {
        let stack = LockFreeStack::new();
        assert!(stack.is_empty());

        stack.push(1);
        stack.push(2);
        stack.push(3);

        assert_eq!(stack.len(), 3);
        assert_eq!(stack.pop(), Some(3)); // LIFO
        assert_eq!(stack.pop(), Some(2));
        assert_eq!(stack.pop(), Some(1));
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn test_stack_lifo_order() {
        let stack = LockFreeStack::new();

        for i in 0..10 {
            stack.push(i);
        }

        for i in (0..10).rev() {
            assert_eq!(stack.pop(), Some(i));
        }
    }

    #[test]
    fn test_atomic_counter() {
        let counter = AtomicCounter::new();
        assert_eq!(counter.get(), 0);

        counter.increment();
        assert_eq!(counter.get(), 1);

        counter.add(5);
        assert_eq!(counter.get(), 6);

        counter.decrement();
        assert_eq!(counter.get(), 5);

        counter.sub(3);
        assert_eq!(counter.get(), 2);

        assert_eq!(counter.peak(), 6);
        assert_eq!(counter.total_increments(), 2);
        assert_eq!(counter.total_decrements(), 2);
    }

    #[test]
    fn test_counter_saturating() {
        let counter = AtomicCounter::new();

        counter.sub(100);
        assert_eq!(counter.get(), 0); // Should not go negative
    }

    #[test]
    fn test_counter_with_value() {
        let counter = AtomicCounter::with_value(100);
        assert_eq!(counter.get(), 100);
        assert_eq!(counter.peak(), 100);
    }

    #[test]
    fn test_seqlock_basic() {
        let lock = SeqLock::new([1, 2, 3, 4]);

        let data = lock.read();
        assert_eq!(data, [1, 2, 3, 4]);

        lock.write([5, 6, 7, 8]);
        let data = lock.read();
        assert_eq!(data, [5, 6, 7, 8]);
    }

    #[test]
    fn test_seqlock_try_read() {
        let lock = SeqLock::new(42u64);

        assert_eq!(lock.try_read(), Some(42));

        lock.write(100);
        assert_eq!(lock.try_read(), Some(100));
    }

    #[test]
    fn test_seqlock_sequence() {
        let lock = SeqLock::new(0u32);

        let seq1 = lock.sequence();
        lock.write(1);
        let seq2 = lock.sequence();

        assert!(seq2 > seq1);
        assert_eq!(seq2 % 2, 0); // Even means no write in progress
    }

    #[test]
    fn test_ring_buffer_basic() {
        let ring: RingBuffer<i32, 4> = RingBuffer::new();
        assert!(ring.is_empty());

        assert!(ring.try_push(1).is_ok());
        assert!(ring.try_push(2).is_ok());
        assert!(ring.try_push(3).is_ok());
        assert!(ring.is_full());

        assert!(ring.try_push(4).is_err()); // Full

        assert_eq!(ring.try_pop(), Some(1));
        assert_eq!(ring.try_pop(), Some(2));
        assert_eq!(ring.try_pop(), Some(3));
        assert_eq!(ring.try_pop(), None); // Empty
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let ring: RingBuffer<i32, 4> = RingBuffer::new();

        // Fill and drain multiple times to test wrap-around
        for _ in 0..3 {
            ring.try_push(1).unwrap();
            ring.try_push(2).unwrap();
            ring.try_push(3).unwrap();

            assert_eq!(ring.try_pop(), Some(1));
            assert_eq!(ring.try_pop(), Some(2));
            assert_eq!(ring.try_pop(), Some(3));
        }
    }

    #[test]
    fn test_ring_buffer_len() {
        let ring: RingBuffer<i32, 8> = RingBuffer::new();

        assert_eq!(ring.len(), 0);
        assert_eq!(ring.capacity(), 7);

        ring.try_push(1).unwrap();
        assert_eq!(ring.len(), 1);

        ring.try_push(2).unwrap();
        ring.try_push(3).unwrap();
        assert_eq!(ring.len(), 3);

        ring.try_pop();
        assert_eq!(ring.len(), 2);
    }

    #[test]
    fn test_queue_drop() {
        let queue = LockFreeQueue::new();
        queue.push(String::from("hello"));
        queue.push(String::from("world"));
        // Queue drops here, should free all memory
    }

    #[test]
    fn test_stack_drop() {
        let stack = LockFreeStack::new();
        stack.push(String::from("hello"));
        stack.push(String::from("world"));
        // Stack drops here, should free all memory
    }

    #[test]
    fn test_chase_lev_push_pop_single_thread() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        assert!(deque.is_empty());

        deque.push(1);
        deque.push(2);
        deque.push(3);
        assert_eq!(deque.len(), 3);

        assert_eq!(deque.pop(), Some(3));
        assert_eq!(deque.pop(), Some(2));
        assert_eq!(deque.pop(), Some(1));
        assert_eq!(deque.pop(), None);
        assert!(deque.is_empty());
    }

    #[test]
    fn test_chase_lev_empty_steal_returns_empty() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        assert!(matches!(deque.steal(), Steal::Empty));
    }

    #[test]
    fn test_chase_lev_steal_from_other_thread() {
        use alloc::sync::Arc;
        use std::thread;

        let deque = Arc::new(WorkStealingDeque::<i32>::new(16));

        for i in 0..8 {
            deque.push(i);
        }

        let thief_deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            let mut stolen = alloc::vec::Vec::new();
            for _ in 0..8 {
                loop {
                    match thief_deque.steal() {
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
        // Steals happen from the top (FIFO), so first items pushed are stolen first.
        // Owner might also pop from the bottom; combined set must cover all 8 items.
        assert!(!stolen.is_empty());
        let mut remaining = alloc::vec::Vec::new();
        while let Some(v) = deque.pop() {
            remaining.push(v);
        }
        let mut all: alloc::vec::Vec<i32> = stolen.into_iter().chain(remaining).collect();
        all.sort();
        assert_eq!(all, (0..8).collect::<alloc::vec::Vec<i32>>());
    }

    #[test]
    fn test_chase_lev_concurrent_steals() {
        use alloc::sync::Arc;
        use std::sync::Barrier;
        use std::thread;

        const ITEMS: i32 = 64;
        const THIEVES: usize = 4;

        let deque = Arc::new(WorkStealingDeque::<i32>::new(128));
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
                let mut spins = 0usize;
                loop {
                    match d.steal() {
                        Steal::Success(_) => count += 1,
                        Steal::Retry => {
                            spins += 1;
                            if spins > 10_000 {
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
        // Owner also participates by popping.
        let mut owner_count = 0usize;
        let mut spins = 0usize;
        loop {
            match deque.pop() {
                Some(_) => owner_count += 1,
                None => {
                    spins += 1;
                    if spins > 100 {
                        break;
                    }
                }
            }
        }

        let total: usize = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .sum::<usize>()
            + owner_count;
        assert_eq!(
            total as i32, ITEMS,
            "All items must be accounted for exactly once"
        );
    }
}
