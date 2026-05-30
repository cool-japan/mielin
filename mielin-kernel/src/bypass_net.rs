//! DPDK-Style Kernel Bypass Networking Abstraction
//!
//! Implements a poll-mode driver (PMD) framework for zero-copy packet I/O,
//! following DPDK design principles: no interrupts, busy-poll loops, lockfree
//! SPSC rings, and DMA buffer pools.
//!
//! ## Architecture
//!
//! ```text
//!  NIC Hardware (simulated via rx_inject)
//!       |
//!       v
//!  +-----------+     +------------+     +------------------+
//!  |  DmaPool  |<--->| PacketRing |<--->| PollModeDriver   |
//!  | (2KB bufs)|     | (SPSC ring)|     | (rx_ring/tx_ring)|
//!  +-----------+     +------------+     +------------------+
//!                                              |
//!                                              v
//!                                    +--------------------+
//!                                    | ForwardingPipeline |
//!                                    | (zero-copy fwd)    |
//!                                    +--------------------+
//! ```
//!
//! ## Design Principles
//!
//! - Zero-copy: packet descriptors own only a slot index into the DMA pool
//! - Lock-free ring: SPSC ring using atomic head/tail pointers
//! - Pre-allocated buffers: DMA pool avoids runtime allocation in the hot path
//! - Burst I/O: process multiple packets per call to amortize overhead
//! - no_std + alloc: compatible with unikernel environments

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Size of each DMA buffer in bytes
pub const DMA_BUF_SIZE: usize = 2048;

/// Default RX ring size (must be power-of-2)
pub const DEFAULT_RX_RING_SIZE: usize = 1024;

/// Default TX ring size (must be power-of-2)
pub const DEFAULT_TX_RING_SIZE: usize = 1024;

/// Default burst size (max packets per burst call)
pub const DEFAULT_BURST_SIZE: usize = 32;

/// Default MTU
pub const DEFAULT_MTU: usize = 1500;

/// Default number of DMA pool buffers
pub const DEFAULT_DMA_POOL_SIZE: usize = 4096;

// ─────────────────────────────────────────────────────────────────────────────
// Error Type
// ─────────────────────────────────────────────────────────────────────────────

/// Errors produced by the bypass networking subsystem
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BypassNetError {
    /// Ring buffer is full; packet was dropped
    RingFull,
    /// Ring buffer is empty; no packet available
    RingEmpty,
    /// Ring capacity is invalid
    InvalidCapacity {
        requested: usize,
        reason: &'static str,
    },
    /// DMA pool has no free buffers
    DmaPoolExhausted,
    /// Data is too large to fit in a DMA buffer or exceeds the MTU
    BufferTooLarge { size: usize, max: usize },
    /// Buffer index is out of range for the DMA pool
    InvalidBufIdx(usize),
}

impl fmt::Display for BypassNetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RingFull => write!(f, "ring buffer is full"),
            Self::RingEmpty => write!(f, "ring buffer is empty"),
            Self::InvalidCapacity { requested, reason } => {
                write!(f, "invalid ring capacity {}: {}", requested, reason)
            }
            Self::DmaPoolExhausted => write!(f, "DMA pool exhausted: no free buffers"),
            Self::BufferTooLarge { size, max } => {
                write!(f, "buffer too large: {} bytes, max {} bytes", size, max)
            }
            Self::InvalidBufIdx(idx) => {
                write!(f, "invalid DMA buffer index: {}", idx)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Packet Flags
// ─────────────────────────────────────────────────────────────────────────────

/// Bitfield of status/offload flags attached to a packet descriptor
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PacketFlags(pub u32);

impl PacketFlags {
    /// RX: hardware verified L3/L4 checksum
    pub const RX_CHECKSUM_OK: u32 = 0x01;
    /// TX: ask NIC to compute L3/L4 checksum
    pub const TX_CHECKSUM_OFFLOAD: u32 = 0x02;
    /// RX: VLAN tag was stripped by hardware
    pub const VLAN_STRIPPED: u32 = 0x04;
    /// Descriptor spans multiple DMA buffers (scatter-gather)
    pub const SCATTER_GATHER: u32 = 0x08;
    /// RX: packet is a multicast frame
    pub const MULTICAST: u32 = 0x10;
    /// RX: packet is a broadcast frame
    pub const BROADCAST: u32 = 0x20;
    /// TX: enable TCP segmentation offload
    pub const TX_TSO: u32 = 0x40;
    /// Packet is marked for mirroring/sampling
    pub const MIRROR: u32 = 0x80;

    /// Create a new flag set from a raw bitmask
    #[inline]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Test whether a specific flag bit is set
    #[inline]
    pub fn has(&self, flag: u32) -> bool {
        self.0 & flag != 0
    }

    /// Set a flag bit
    #[inline]
    pub fn set(&mut self, flag: u32) {
        self.0 |= flag;
    }

    /// Clear a flag bit
    #[inline]
    pub fn clear(&mut self, flag: u32) {
        self.0 &= !flag;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Packet Descriptor
// ─────────────────────────────────────────────────────────────────────────────

/// Zero-copy packet descriptor.
///
/// Owns a slot in the [`DmaPool`] identified by `buf_idx`; never holds
/// a raw pointer to the data. The actual bytes live in `DmaPool::buffers`.
#[derive(Debug, Clone)]
pub struct PacketDesc {
    /// Index of the DMA buffer slot that holds the packet data
    pub buf_idx: usize,
    /// Byte offset within the DMA buffer where the packet data starts
    pub data_offset: usize,
    /// Length of the actual packet data in bytes
    pub data_len: usize,
    /// Status/offload flags
    pub flags: PacketFlags,
}

impl PacketDesc {
    /// Create a new packet descriptor
    pub fn new(buf_idx: usize, data_offset: usize, data_len: usize, flags: PacketFlags) -> Self {
        Self {
            buf_idx,
            data_offset,
            data_len,
            flags,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DMA Pool Statistics
// ─────────────────────────────────────────────────────────────────────────────

/// Counters collected by the DMA pool
#[derive(Debug)]
pub struct DmaPoolStats {
    /// Total allocation calls that succeeded
    pub allocs: AtomicUsize,
    /// Total free calls
    pub frees: AtomicUsize,
    /// Failed allocation attempts (pool was empty)
    pub alloc_failures: AtomicUsize,
    /// Total number of slots in the pool
    pub total_slots: usize,
}

impl Default for DmaPoolStats {
    fn default() -> Self {
        Self {
            allocs: AtomicUsize::new(0),
            frees: AtomicUsize::new(0),
            alloc_failures: AtomicUsize::new(0),
            total_slots: 0,
        }
    }
}

impl DmaPoolStats {
    fn new(total_slots: usize) -> Self {
        Self {
            allocs: AtomicUsize::new(0),
            frees: AtomicUsize::new(0),
            alloc_failures: AtomicUsize::new(0),
            total_slots,
        }
    }

    /// Currently allocated slot count (allocs - frees)
    pub fn in_use(&self) -> usize {
        let a = self.allocs.load(Ordering::Relaxed);
        let f = self.frees.load(Ordering::Relaxed);
        a.saturating_sub(f)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DMA Pool
// ─────────────────────────────────────────────────────────────────────────────

/// Pre-allocated pool of fixed-size DMA buffers.
///
/// Each buffer is [`DMA_BUF_SIZE`] (2 KB) bytes. The free list is protected
/// by a `spin::Mutex` so that alloc/free can be called from shared references
/// (`&self`), which is required for `Arc<DmaPool>` usage.
pub struct DmaPool {
    /// The backing store — one 2 KB array per slot
    buffers: Vec<[u8; DMA_BUF_SIZE]>,
    /// Indices of currently free slots
    free_list: Mutex<Vec<usize>>,
    /// Accounting counters
    stats: DmaPoolStats,
}

impl DmaPool {
    /// Allocate a new DMA pool with `size` slots.
    ///
    /// All slots are immediately available in the free list.
    pub fn new(size: usize) -> Self {
        let buffers = (0..size).map(|_| [0u8; DMA_BUF_SIZE]).collect();
        let free_list: Vec<usize> = (0..size).collect();
        Self {
            buffers,
            free_list: Mutex::new(free_list),
            stats: DmaPoolStats::new(size),
        }
    }

    /// Allocate a slot from the pool.
    ///
    /// Returns the slot index on success, or `None` if the pool is empty.
    pub fn alloc(&self) -> Option<usize> {
        let idx = self.free_list.lock().pop();
        match idx {
            Some(i) => {
                self.stats.allocs.fetch_add(1, Ordering::Relaxed);
                Some(i)
            }
            None => {
                self.stats.alloc_failures.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Return a slot back to the pool.
    ///
    /// Silently ignores invalid indices to avoid production crashes.
    pub fn free(&self, idx: usize) {
        if idx < self.buffers.len() {
            self.free_list.lock().push(idx);
            self.stats.frees.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Write `data` into slot `idx` at byte `offset`.
    ///
    /// Returns `Err(BypassNetError::InvalidBufIdx)` for bad indices,
    /// `Err(BypassNetError::BufferTooLarge)` if the write would overflow.
    pub fn write_checked(
        &self,
        idx: usize,
        offset: usize,
        data: &[u8],
    ) -> Result<(), BypassNetError> {
        let buf = self
            .buffers
            .get(idx)
            .ok_or(BypassNetError::InvalidBufIdx(idx))?;
        let end = offset
            .checked_add(data.len())
            .ok_or(BypassNetError::BufferTooLarge {
                size: data.len(),
                max: DMA_BUF_SIZE,
            })?;
        if end > DMA_BUF_SIZE {
            return Err(BypassNetError::BufferTooLarge {
                size: end,
                max: DMA_BUF_SIZE,
            });
        }
        // SAFETY: idx is in range (checked via .get()), offset+len checked above.
        // The caller is responsible for not aliasing (only one thread should own a slot).
        let buf_ptr = buf.as_ptr() as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), buf_ptr.add(offset), data.len());
        }
        Ok(())
    }

    /// Convenience write (mirrors the public API). Silently ignores errors.
    pub fn write(&self, idx: usize, offset: usize, data: &[u8]) {
        let _ = self.write_checked(idx, offset, data);
    }

    /// Read `len` bytes from slot `idx` starting at `offset`.
    ///
    /// Returns an empty slice on any bounds error rather than panicking.
    pub fn read(&self, idx: usize, offset: usize, len: usize) -> &[u8] {
        match self.buffers.get(idx) {
            Some(buf) => {
                let start = offset.min(DMA_BUF_SIZE);
                let end = offset.saturating_add(len).min(DMA_BUF_SIZE);
                &buf[start..end]
            }
            None => &[],
        }
    }

    /// Read the full slice for a descriptor (offset + data_len).
    pub fn read_desc(&self, desc: &PacketDesc) -> &[u8] {
        self.read(desc.buf_idx, desc.data_offset, desc.data_len)
    }

    /// Return a reference to the pool's statistics.
    pub fn stats(&self) -> &DmaPoolStats {
        &self.stats
    }

    /// Return the total number of slots in the pool.
    pub fn capacity(&self) -> usize {
        self.buffers.len()
    }

    /// Return number of currently free slots.
    pub fn free_count(&self) -> usize {
        self.free_list.lock().len()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Packet Ring (SPSC, power-of-2)
// ─────────────────────────────────────────────────────────────────────────────

/// Single-producer single-consumer lock-free ring buffer for packet descriptors.
///
/// Uses a power-of-2 capacity so that head/tail wrap-around is a bitwise AND.
/// The producer owns `head` and the consumer owns `tail`; no CAS is needed.
///
/// Each slot is wrapped in `UnsafeCell` to enable sound interior mutability:
/// writing through a `*mut` derived from an `UnsafeCell` is explicitly permitted
/// by the Rust memory model, unlike writing through a `*mut` cast from a shared
/// reference to a plain `Vec` element.
pub struct PacketRing {
    /// Ring slots, each wrapped for interior mutability
    ring: Vec<UnsafeCell<Option<PacketDesc>>>,
    /// Write cursor — producer advances after writing a slot
    head: AtomicUsize,
    /// Read cursor — consumer advances after reading a slot
    tail: AtomicUsize,
    /// capacity - 1, used as bitmask for O(1) wrap-around
    mask: usize,
}

// SAFETY: The SPSC discipline (single producer + single consumer) ensures that
// at any moment head and tail diverge by at most `capacity` slots, and the
// producer never reads a slot that the consumer is currently reading, and vice
// versa.  Sending across threads is safe because PacketDesc is Send.
unsafe impl Send for PacketRing {}
// SAFETY: Shared access (&PacketRing) is needed so that `enqueue` can be
// called from a `&PollModeDriver` (e.g. from `rx_inject`).  The SPSC
// invariant means only ONE producer and ONE consumer exist at a time; the
// Sync impl documents this contract to the compiler.
unsafe impl Sync for PacketRing {}

impl PacketRing {
    /// Create a new ring of `capacity` slots.
    ///
    /// `capacity` must be a non-zero power of two; otherwise returns
    /// [`BypassNetError::InvalidCapacity`].
    pub fn new(capacity: usize) -> Result<Self, BypassNetError> {
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(BypassNetError::InvalidCapacity {
                requested: capacity,
                reason: "must be a non-zero power of two",
            });
        }
        let ring = (0..capacity)
            .map(|_| UnsafeCell::new(None))
            .collect();
        Ok(Self {
            ring,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            mask: capacity - 1,
        })
    }

    /// Enqueue a single descriptor.
    ///
    /// Returns [`BypassNetError::RingFull`] if the ring is at capacity.
    pub fn enqueue(&self, desc: PacketDesc) -> Result<(), BypassNetError> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.ring.len() {
            return Err(BypassNetError::RingFull);
        }
        let slot = head & self.mask;
        // SAFETY: `slot` is within bounds (mask ensures it). `UnsafeCell::get()`
        // yields a *mut that we are allowed to write to.  We are the sole producer,
        // so no other thread writes this slot simultaneously.
        unsafe {
            *self.ring[slot].get() = Some(desc);
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Dequeue a single descriptor.
    ///
    /// Returns `None` if the ring is empty.
    pub fn dequeue(&self) -> Option<PacketDesc> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let slot = tail & self.mask;
        // SAFETY: `slot` is within bounds. We are the sole consumer,
        // so no other thread reads/writes this slot simultaneously.
        let desc = unsafe { (*self.ring[slot].get()).take() };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        desc
    }

    /// Enqueue up to `descs.len()` descriptors in one call.
    ///
    /// Returns the number actually enqueued (may be less than requested if
    /// the ring fills up mid-burst).
    pub fn enqueue_burst(&self, descs: &[PacketDesc]) -> usize {
        let mut count = 0;
        for desc in descs {
            match self.enqueue(desc.clone()) {
                Ok(_) => count += 1,
                Err(_) => break,
            }
        }
        count
    }

    /// Dequeue up to `out.len()` descriptors into the provided slice.
    ///
    /// Returns the number actually dequeued.
    pub fn dequeue_burst(&mut self, out: &mut [PacketDesc]) -> usize {
        let mut count = 0;
        for slot in out.iter_mut() {
            match self.dequeue() {
                Some(desc) => {
                    *slot = desc;
                    count += 1;
                }
                None => break,
            }
        }
        count
    }

    /// Number of descriptors currently in the ring.
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Returns `true` if the ring contains no descriptors.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total capacity of the ring.
    pub fn capacity(&self) -> usize {
        self.ring.len()
    }
}

impl fmt::Debug for PacketRing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PacketRing")
            .field("capacity", &self.ring.len())
            .field("head", &self.head.load(Ordering::Relaxed))
            .field("tail", &self.tail.load(Ordering::Relaxed))
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PMD Statistics
// ─────────────────────────────────────────────────────────────────────────────

/// Counters for a single [`PollModeDriver`]
#[derive(Debug, Clone, Default)]
pub struct PmdStats {
    /// Total packets received into the RX ring
    pub rx_packets: u64,
    /// Total packets sent from the TX ring
    pub tx_packets: u64,
    /// RX packets dropped due to ring overflow
    pub rx_dropped: u64,
    /// TX packets dropped due to ring overflow or pool exhaustion
    pub tx_dropped: u64,
    /// Total RX bytes
    pub rx_bytes: u64,
    /// Total TX bytes
    pub tx_bytes: u64,
    /// Number of `rx_burst` calls that returned at least 1 packet
    pub rx_bursts: u64,
    /// Number of `tx_flush` calls that sent at least 1 packet
    pub tx_bursts: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Bypass Networking Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for a [`PollModeDriver`] instance
#[derive(Debug, Clone)]
pub struct BypassNetConfig {
    /// RX ring size — must be a power of two
    pub rx_ring_size: usize,
    /// TX ring size — must be a power of two
    pub tx_ring_size: usize,
    /// Maximum packets processed per burst call
    pub burst_size: usize,
    /// Maximum transmission unit (bytes)
    pub mtu: usize,
    /// Number of DMA buffer slots to pre-allocate
    pub dma_pool_size: usize,
    /// NUMA node affinity (informational; unused in simulation)
    pub numa_node: Option<usize>,
}

impl Default for BypassNetConfig {
    fn default() -> Self {
        Self {
            rx_ring_size: DEFAULT_RX_RING_SIZE,
            tx_ring_size: DEFAULT_TX_RING_SIZE,
            burst_size: DEFAULT_BURST_SIZE,
            mtu: DEFAULT_MTU,
            dma_pool_size: DEFAULT_DMA_POOL_SIZE,
            numa_node: None,
        }
    }
}

impl BypassNetConfig {
    /// Validate that ring sizes are non-zero powers of two.
    pub fn validate(&self) -> Result<(), BypassNetError> {
        if self.rx_ring_size == 0 || !self.rx_ring_size.is_power_of_two() {
            return Err(BypassNetError::InvalidCapacity {
                requested: self.rx_ring_size,
                reason: "rx_ring_size must be a non-zero power of two",
            });
        }
        if self.tx_ring_size == 0 || !self.tx_ring_size.is_power_of_two() {
            return Err(BypassNetError::InvalidCapacity {
                requested: self.tx_ring_size,
                reason: "tx_ring_size must be a non-zero power of two",
            });
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Poll-Mode Driver
// ─────────────────────────────────────────────────────────────────────────────

/// DPDK-style poll-mode driver.
///
/// Owns an RX ring, a TX ring, and a shared DMA pool. In a real kernel this
/// would map directly to NIC descriptor rings via DMA; here `rx_inject`
/// simulates the NIC writing to the RX ring.
pub struct PollModeDriver {
    config: BypassNetConfig,
    rx_ring: PacketRing,
    tx_ring: PacketRing,
    dma_pool: Arc<DmaPool>,
    stats: Mutex<PmdStats>,
}

impl PollModeDriver {
    /// Create a new PMD with the given configuration.
    ///
    /// Allocates the DMA pool and both rings.
    pub fn new(config: BypassNetConfig) -> Result<Self, BypassNetError> {
        config.validate()?;
        let rx_ring = PacketRing::new(config.rx_ring_size)?;
        let tx_ring = PacketRing::new(config.tx_ring_size)?;
        let dma_pool = Arc::new(DmaPool::new(config.dma_pool_size));
        Ok(Self {
            config,
            rx_ring,
            tx_ring,
            dma_pool,
            stats: Mutex::new(PmdStats::default()),
        })
    }

    /// Simulate a NIC DMA-writing a packet into the RX ring.
    ///
    /// Allocates a DMA buffer slot, copies `data` into it, and enqueues
    /// the descriptor into the RX ring. Returns an error if the pool is
    /// exhausted or the ring is full.
    pub fn rx_inject(&self, data: &[u8]) -> Result<(), BypassNetError> {
        if data.len() > DMA_BUF_SIZE {
            return Err(BypassNetError::BufferTooLarge {
                size: data.len(),
                max: DMA_BUF_SIZE,
            });
        }
        let buf_idx = self
            .dma_pool
            .alloc()
            .ok_or(BypassNetError::DmaPoolExhausted)?;
        self.dma_pool.write(buf_idx, 0, data);
        let desc = PacketDesc::new(
            buf_idx,
            0,
            data.len(),
            PacketFlags::new(PacketFlags::RX_CHECKSUM_OK),
        );
        match self.rx_ring.enqueue(desc) {
            Ok(()) => {
                let mut stats = self.stats.lock();
                stats.rx_packets += 1;
                stats.rx_bytes += data.len() as u64;
                Ok(())
            }
            Err(e) => {
                self.dma_pool.free(buf_idx);
                {
                    let mut stats = self.stats.lock();
                    stats.rx_dropped += 1;
                }
                Err(e)
            }
        }
    }

    /// Poll the RX ring for received packets.
    ///
    /// Dequeues up to `out.len()` (capped by `config.burst_size`) descriptors.
    /// Returns the number of packets dequeued.
    pub fn rx_burst(&mut self, out: &mut [PacketDesc]) -> usize {
        let limit = out.len().min(self.config.burst_size);
        let out_slice = &mut out[..limit];
        let n = self.rx_ring.dequeue_burst(out_slice);
        if n > 0 {
            let mut stats = self.stats.lock();
            stats.rx_bursts += 1;
        }
        n
    }

    /// Submit descriptors to the TX ring for transmission.
    ///
    /// Returns the number of descriptors successfully enqueued.
    pub fn tx_burst(&self, descs: &[PacketDesc]) -> usize {
        let n = self.tx_ring.enqueue_burst(descs);
        if n < descs.len() {
            let dropped = (descs.len() - n) as u64;
            let mut stats = self.stats.lock();
            stats.tx_dropped += dropped;
        }
        n
    }

    /// Drain the TX ring, simulating NIC transmission.
    ///
    /// Returns the number of packets "transmitted" and frees the DMA buffers.
    pub fn tx_flush(&mut self) -> usize {
        let mut flushed = 0usize;
        let mut total_bytes = 0u64;
        while let Some(desc) = self.tx_ring.dequeue() {
            total_bytes += desc.data_len as u64;
            self.dma_pool.free(desc.buf_idx);
            flushed += 1;
        }
        if flushed > 0 {
            let mut stats = self.stats.lock();
            stats.tx_packets += flushed as u64;
            stats.tx_bytes += total_bytes;
            stats.tx_bursts += 1;
        }
        flushed
    }

    /// Zero-copy read: return the packet data slice for a descriptor.
    ///
    /// The slice is valid as long as the descriptor is not freed.
    pub fn get_buf(&self, desc: &PacketDesc) -> &[u8] {
        self.dma_pool
            .read(desc.buf_idx, desc.data_offset, desc.data_len)
    }

    /// Allocate a TX buffer from the pool and return a blank descriptor.
    pub fn alloc_buf(&self) -> Option<PacketDesc> {
        let buf_idx = self.dma_pool.alloc()?;
        Some(PacketDesc::new(buf_idx, 0, 0, PacketFlags::default()))
    }

    /// Write `data` into an existing descriptor's DMA slot.
    ///
    /// Updates `desc.data_len` to reflect the new payload length.
    /// Returns `Err(BypassNetError::BufferTooLarge)` if `data` exceeds
    /// the MTU or DMA buffer size.
    pub fn fill_buf(&self, desc: &mut PacketDesc, data: &[u8]) -> Result<(), BypassNetError> {
        let max = self.config.mtu.min(DMA_BUF_SIZE);
        if data.len() > max {
            return Err(BypassNetError::BufferTooLarge {
                size: data.len(),
                max,
            });
        }
        self.dma_pool
            .write_checked(desc.buf_idx, desc.data_offset, data)?;
        desc.data_len = data.len();
        Ok(())
    }

    /// Return a buffer to the DMA pool without transmitting it.
    pub fn free_buf(&self, desc: PacketDesc) {
        self.dma_pool.free(desc.buf_idx);
    }

    /// Snapshot the current statistics.
    pub fn stats(&self) -> PmdStats {
        self.stats.lock().clone()
    }

    /// Reset all statistics counters to zero.
    pub fn reset_stats(&self) {
        *self.stats.lock() = PmdStats::default();
    }

    /// Expose a clone of the DMA pool handle (for pipeline composition)
    pub fn dma_pool(&self) -> Arc<DmaPool> {
        Arc::clone(&self.dma_pool)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Forwarding Pipeline Statistics
// ─────────────────────────────────────────────────────────────────────────────

/// Counters for a [`ForwardingPipeline`]
#[derive(Debug, Clone, Default)]
pub struct ForwardingStats {
    /// Packets successfully forwarded from RX to TX
    pub forwarded: u64,
    /// Packets dropped by the filter function
    pub filtered: u64,
    /// Packets dropped because the TX ring was full
    pub tx_overflow: u64,
    /// Total forward_burst calls made
    pub burst_calls: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Forwarding Pipeline
// ─────────────────────────────────────────────────────────────────────────────

/// Type alias for the per-packet filter closure used by [`ForwardingPipeline`].
///
/// Returns `true` to forward the packet, `false` to drop it.
pub type PacketFilter = dyn Fn(&PacketDesc, &DmaPool) -> bool + Send;

/// Zero-copy forwarding pipeline connecting one PMD's RX to another PMD's TX.
///
/// An optional filter closure decides per-packet whether to forward or drop.
/// The DMA pool reference is shared between both PMDs for zero-copy hand-off;
/// when the filter drops a packet the buffer is returned to the pool.
pub struct ForwardingPipeline {
    rx: PollModeDriver,
    tx: PollModeDriver,
    filter: Option<Box<PacketFilter>>,
    stats: ForwardingStats,
}

impl ForwardingPipeline {
    /// Create a forwarding pipeline.
    pub fn new(rx: PollModeDriver, tx: PollModeDriver) -> Self {
        Self {
            rx,
            tx,
            filter: None,
            stats: ForwardingStats::default(),
        }
    }

    /// Attach a per-packet filter closure.
    ///
    /// The closure receives a reference to the descriptor and the RX pool.
    /// Return `true` to forward the packet, `false` to drop it.
    pub fn with_filter(
        mut self,
        f: impl Fn(&PacketDesc, &DmaPool) -> bool + Send + 'static,
    ) -> Self {
        self.filter = Some(Box::new(f));
        self
    }

    /// Run one forwarding burst: dequeue from RX, apply filter, enqueue to TX.
    ///
    /// Returns the number of packets forwarded (not filtered).
    pub fn forward_burst(&mut self) -> usize {
        let burst_size = self.rx.config.burst_size;
        let mut scratch: Vec<PacketDesc> = (0..burst_size)
            .map(|_| PacketDesc::new(0, 0, 0, PacketFlags::default()))
            .collect();

        let n = self.rx.rx_burst(&mut scratch);
        scratch.truncate(n);

        self.stats.burst_calls += 1;

        let dma_pool = Arc::clone(&self.rx.dma_pool);
        let mut forwarded = 0usize;
        let mut filtered = 0usize;
        let mut tx_overflow = 0usize;

        for desc in scratch.drain(..) {
            let pass = match &self.filter {
                Some(f) => f(&desc, &dma_pool),
                None => true,
            };
            if !pass {
                dma_pool.free(desc.buf_idx);
                filtered += 1;
                continue;
            }
            match self.tx.tx_ring.enqueue(desc.clone()) {
                Ok(_) => forwarded += 1,
                Err(_) => {
                    dma_pool.free(desc.buf_idx);
                    tx_overflow += 1;
                }
            }
        }

        self.stats.forwarded += forwarded as u64;
        self.stats.filtered += filtered as u64;
        self.stats.tx_overflow += tx_overflow as u64;
        forwarded
    }

    /// Read-only reference to pipeline statistics.
    pub fn stats(&self) -> &ForwardingStats {
        &self.stats
    }

    /// Flush pending TX packets.
    pub fn tx_flush(&mut self) -> usize {
        self.tx.tx_flush()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ──────────────────────────────────────
    // Helpers
    // ──────────────────────────────────────

    fn default_pmd() -> PollModeDriver {
        PollModeDriver::new(BypassNetConfig::default()).unwrap()
    }

    fn small_config() -> BypassNetConfig {
        BypassNetConfig {
            rx_ring_size: 16,
            tx_ring_size: 16,
            burst_size: 8,
            mtu: 1500,
            dma_pool_size: 64,
            numa_node: None,
        }
    }

    // ──────────────────────────────────────
    // 1. Config defaults are power-of-2
    // ──────────────────────────────────────
    #[test]
    fn test_config_default() {
        let cfg = BypassNetConfig::default();
        assert!(cfg.rx_ring_size.is_power_of_two());
        assert!(cfg.tx_ring_size.is_power_of_two());
        assert_eq!(cfg.rx_ring_size, DEFAULT_RX_RING_SIZE);
        assert_eq!(cfg.tx_ring_size, DEFAULT_TX_RING_SIZE);
        assert_eq!(cfg.mtu, DEFAULT_MTU);
        assert_eq!(cfg.burst_size, DEFAULT_BURST_SIZE);
    }

    // ──────────────────────────────────────
    // 2. Non-power-of-2 ring size → error
    // ──────────────────────────────────────
    #[test]
    fn test_ring_capacity_must_be_pow2() {
        let err = PacketRing::new(100).unwrap_err();
        assert!(matches!(err, BypassNetError::InvalidCapacity { .. }));

        let err2 = PacketRing::new(0).unwrap_err();
        assert!(matches!(err2, BypassNetError::InvalidCapacity { .. }));

        // Power-of-2 should succeed
        assert!(PacketRing::new(64).is_ok());
    }

    // ──────────────────────────────────────
    // 3. Single enqueue/dequeue roundtrip
    // ──────────────────────────────────────
    #[test]
    fn test_ring_enqueue_dequeue() {
        let ring = PacketRing::new(8).unwrap();
        let desc = PacketDesc::new(0, 0, 42, PacketFlags::default());
        ring.enqueue(desc).unwrap();
        let out = ring.dequeue().unwrap();
        assert_eq!(out.buf_idx, 0);
        assert_eq!(out.data_len, 42);
    }

    // ──────────────────────────────────────
    // 4. Burst enqueue
    // ──────────────────────────────────────
    #[test]
    fn test_ring_enqueue_burst() {
        let ring = PacketRing::new(32).unwrap();
        let descs: Vec<PacketDesc> = (0..16)
            .map(|i| PacketDesc::new(i, 0, i * 10, PacketFlags::default()))
            .collect();
        let n = ring.enqueue_burst(&descs);
        assert_eq!(n, 16);
        assert_eq!(ring.len(), 16);
    }

    // ──────────────────────────────────────
    // 5. Burst dequeue
    // ──────────────────────────────────────
    #[test]
    fn test_ring_dequeue_burst() {
        let mut ring = PacketRing::new(32).unwrap();
        let descs: Vec<PacketDesc> = (0..16)
            .map(|i| PacketDesc::new(i, 0, i, PacketFlags::default()))
            .collect();
        ring.enqueue_burst(&descs);

        let mut out = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); 16];
        let n = ring.dequeue_burst(&mut out);
        assert_eq!(n, 16);
        assert_eq!(out[0].data_len, 0);
        assert_eq!(out[15].data_len, 15);
    }

    // ──────────────────────────────────────
    // 6. Ring full → RingFull error
    // ──────────────────────────────────────
    #[test]
    fn test_ring_overflow() {
        let ring = PacketRing::new(4).unwrap();
        for i in 0..4 {
            ring.enqueue(PacketDesc::new(i, 0, 1, PacketFlags::default()))
                .unwrap();
        }
        let err = ring
            .enqueue(PacketDesc::new(99, 0, 1, PacketFlags::default()))
            .unwrap_err();
        assert_eq!(err, BypassNetError::RingFull);
    }

    // ──────────────────────────────────────
    // 7. Dequeue from empty ring → None
    // ──────────────────────────────────────
    #[test]
    fn test_ring_empty_dequeue() {
        let ring = PacketRing::new(8).unwrap();
        assert!(ring.dequeue().is_none());
        assert!(ring.is_empty());
    }

    // ──────────────────────────────────────
    // 8. len() matches enqueue/dequeue
    // ──────────────────────────────────────
    #[test]
    fn test_ring_len_tracking() {
        let ring = PacketRing::new(16).unwrap();
        assert_eq!(ring.len(), 0);
        for i in 0..5 {
            ring.enqueue(PacketDesc::new(i, 0, 1, PacketFlags::default()))
                .unwrap();
        }
        assert_eq!(ring.len(), 5);
        ring.dequeue().unwrap();
        ring.dequeue().unwrap();
        assert_eq!(ring.len(), 3);
    }

    // ──────────────────────────────────────
    // 9. DMA pool alloc / free
    // ──────────────────────────────────────
    #[test]
    fn test_dma_pool_alloc_free() {
        let pool = DmaPool::new(8);
        let idx = pool.alloc().unwrap();
        assert!(idx < 8);
        assert_eq!(pool.free_count(), 7);
        pool.free(idx);
        assert_eq!(pool.free_count(), 8);
    }

    // ──────────────────────────────────────
    // 10. Pool exhaustion
    // ──────────────────────────────────────
    #[test]
    fn test_dma_pool_exhaustion() {
        let pool = DmaPool::new(4);
        let _a = pool.alloc().unwrap();
        let _b = pool.alloc().unwrap();
        let _c = pool.alloc().unwrap();
        let _d = pool.alloc().unwrap();
        assert!(pool.alloc().is_none());
        assert_eq!(
            pool.stats().alloc_failures.load(Ordering::Relaxed),
            1
        );
    }

    // ──────────────────────────────────────
    // 11. Write then read roundtrip
    // ──────────────────────────────────────
    #[test]
    fn test_dma_pool_write_read() {
        let pool = DmaPool::new(4);
        let idx = pool.alloc().unwrap();
        let payload = b"hello, bypass net!";
        pool.write(idx, 0, payload);
        let got = pool.read(idx, 0, payload.len());
        assert_eq!(got, payload);
    }

    // ──────────────────────────────────────
    // 12. rx_inject → rx_burst gets it
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_rx_inject() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        pmd.rx_inject(b"packet!").unwrap();
        let mut out = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); 8];
        let n = pmd.rx_burst(&mut out);
        assert_eq!(n, 1);
        assert_eq!(out[0].data_len, 7);
    }

    // ──────────────────────────────────────
    // 13. inject 10, burst 10
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_rx_burst_multiple() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        for i in 0..10u8 {
            let data = [i; 20];
            pmd.rx_inject(&data).unwrap();
        }
        let mut out = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); 16];
        let n = pmd.rx_burst(&mut out);
        assert_eq!(n, 8); // capped by burst_size=8
        // Drain remaining
        let m = pmd.rx_burst(&mut out);
        assert_eq!(m, 2);
    }

    // ──────────────────────────────────────
    // 14. alloc buf, fill, tx_burst, tx_flush
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_tx_burst() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        let mut desc = pmd.alloc_buf().unwrap();
        pmd.fill_buf(&mut desc, b"hello tx").unwrap();
        let n = pmd.tx_burst(&[desc]);
        assert_eq!(n, 1);
        let flushed = pmd.tx_flush();
        assert_eq!(flushed, 1);
    }

    // ──────────────────────────────────────
    // 15. rx_packets stat increments
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_rx_stats() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        pmd.rx_inject(b"abc").unwrap();
        pmd.rx_inject(b"def").unwrap();
        let stats = pmd.stats();
        assert_eq!(stats.rx_packets, 2);
        assert_eq!(stats.rx_bytes, 6);
        // drain the ring to keep pool clean
        let mut drain = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); 2];
        let _ = pmd.rx_burst(&mut drain);
    }

    // ──────────────────────────────────────
    // 16. tx_packets stat increments after flush
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_tx_stats() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        let mut desc = pmd.alloc_buf().unwrap();
        pmd.fill_buf(&mut desc, b"data").unwrap();
        pmd.tx_burst(&[desc]);
        pmd.tx_flush();
        let stats = pmd.stats();
        assert_eq!(stats.tx_packets, 1);
        assert_eq!(stats.tx_bytes, 4);
    }

    // ──────────────────────────────────────
    // 17. rx_dropped increments when ring full
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_rx_dropped_stat() {
        let cfg = BypassNetConfig {
            rx_ring_size: 4,
            tx_ring_size: 4,
            burst_size: 4,
            mtu: 1500,
            dma_pool_size: 64,
            numa_node: None,
        };
        let pmd = PollModeDriver::new(cfg).unwrap();
        // Fill the ring exactly
        for _ in 0..4 {
            pmd.rx_inject(b"x").unwrap();
        }
        // One more must fail with RingFull and bump rx_dropped
        let err = pmd.rx_inject(b"overflow");
        assert!(err.is_err());
        let stats = pmd.stats();
        assert_eq!(stats.rx_dropped, 1);
    }

    // ──────────────────────────────────────
    // 18. alloc_buf / fill_buf / free_buf
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_alloc_fill_free() {
        let pmd = default_pmd();
        let mut desc = pmd.alloc_buf().unwrap();
        pmd.fill_buf(&mut desc, b"roundtrip").unwrap();
        assert_eq!(desc.data_len, 9);
        pmd.free_buf(desc);
    }

    // ──────────────────────────────────────
    // 19. Zero-copy read via get_buf
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_zero_copy_read() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        let payload = b"zero copy payload";
        pmd.rx_inject(payload).unwrap();
        let mut out = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); 1];
        let n = pmd.rx_burst(&mut out);
        assert_eq!(n, 1);
        let got = pmd.get_buf(&out[0]);
        assert_eq!(got, payload);
    }

    // ──────────────────────────────────────
    // 20. reset_stats clears all counters
    // ──────────────────────────────────────
    #[test]
    fn test_pmd_reset_stats() {
        let mut pmd = PollModeDriver::new(small_config()).unwrap();
        pmd.rx_inject(b"pkt").unwrap();
        let mut out = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); 4];
        pmd.rx_burst(&mut out);
        let before = pmd.stats();
        assert!(before.rx_packets > 0);
        pmd.reset_stats();
        let after = pmd.stats();
        assert_eq!(after.rx_packets, 0);
        assert_eq!(after.tx_packets, 0);
        assert_eq!(after.rx_dropped, 0);
        assert_eq!(after.tx_dropped, 0);
        assert_eq!(after.rx_bytes, 0);
        assert_eq!(after.tx_bytes, 0);
        assert_eq!(after.rx_bursts, 0);
        assert_eq!(after.tx_bursts, 0);
    }

    // ──────────────────────────────────────
    // 21. Forwarding pipeline basic test
    // ──────────────────────────────────────
    #[test]
    fn test_forwarding_pipeline_basic() {
        let rx_pmd = PollModeDriver::new(small_config()).unwrap();
        let tx_pmd = PollModeDriver::new(small_config()).unwrap();
        rx_pmd.rx_inject(b"fwd packet").unwrap();

        let mut pipeline = ForwardingPipeline::new(rx_pmd, tx_pmd);
        let forwarded = pipeline.forward_burst();
        assert_eq!(forwarded, 1);
        assert_eq!(pipeline.stats().forwarded, 1);

        let flushed = pipeline.tx_flush();
        assert_eq!(flushed, 1);
    }

    // ──────────────────────────────────────
    // 22. Filter drops odd-length packets
    // ──────────────────────────────────────
    #[test]
    fn test_forwarding_pipeline_filter() {
        let rx_pmd = PollModeDriver::new(small_config()).unwrap();
        let tx_pmd = PollModeDriver::new(small_config()).unwrap();

        // Inject 4 packets: lengths 2, 3, 4, 5
        rx_pmd.rx_inject(b"ab").unwrap(); // len 2 → even → pass
        rx_pmd.rx_inject(b"abc").unwrap(); // len 3 → odd  → drop
        rx_pmd.rx_inject(b"abcd").unwrap(); // len 4 → even → pass
        rx_pmd.rx_inject(b"abcde").unwrap(); // len 5 → odd  → drop

        let mut pipeline = ForwardingPipeline::new(rx_pmd, tx_pmd)
            .with_filter(|desc, _pool| desc.data_len % 2 == 0);

        let forwarded = pipeline.forward_burst();
        assert_eq!(forwarded, 2);
        assert_eq!(pipeline.stats().filtered, 2);
    }

    // ──────────────────────────────────────
    // 23. Forward a burst of 32 packets
    // ──────────────────────────────────────
    #[test]
    fn test_forwarding_pipeline_burst() {
        let cfg = BypassNetConfig {
            rx_ring_size: 64,
            tx_ring_size: 64,
            burst_size: 32,
            mtu: 1500,
            dma_pool_size: 256,
            numa_node: None,
        };
        let rx_pmd = PollModeDriver::new(cfg.clone()).unwrap();
        let tx_pmd = PollModeDriver::new(cfg).unwrap();

        for i in 0..32u8 {
            let data = [i; 10];
            rx_pmd.rx_inject(&data).unwrap();
        }

        let mut pipeline = ForwardingPipeline::new(rx_pmd, tx_pmd);
        let forwarded = pipeline.forward_burst();
        assert_eq!(forwarded, 32);

        let flushed = pipeline.tx_flush();
        assert_eq!(flushed, 32);
    }

    // ──────────────────────────────────────
    // 24. Forwarding stats — forwarded/filtered
    // ──────────────────────────────────────
    #[test]
    fn test_forwarding_stats() {
        let rx_pmd = PollModeDriver::new(small_config()).unwrap();
        let tx_pmd = PollModeDriver::new(small_config()).unwrap();

        for i in 0..6u8 {
            rx_pmd.rx_inject(&[i; 5]).unwrap();
        }

        // The DMA pool stack pops from the top, so the first alloc gets the last slot
        // (slot = pool_size - 1 = 63). Filter: keep only packets where buf_idx is even.
        let mut pipeline = ForwardingPipeline::new(rx_pmd, tx_pmd)
            .with_filter(|desc, _pool| desc.buf_idx % 2 == 0);

        pipeline.forward_burst();
        let s = pipeline.stats();
        // forwarded + filtered == 6 (all 6 packets processed)
        assert_eq!(s.forwarded + s.filtered, 6);
        assert_eq!(s.burst_calls, 1);
    }

    // ──────────────────────────────────────
    // 25. PacketFlags set / check
    // ──────────────────────────────────────
    #[test]
    fn test_packet_flags() {
        let mut flags = PacketFlags::default();
        assert!(!flags.has(PacketFlags::RX_CHECKSUM_OK));
        flags.set(PacketFlags::RX_CHECKSUM_OK);
        assert!(flags.has(PacketFlags::RX_CHECKSUM_OK));
        flags.set(PacketFlags::VLAN_STRIPPED);
        assert!(flags.has(PacketFlags::VLAN_STRIPPED));
        assert!(flags.has(PacketFlags::RX_CHECKSUM_OK));
        flags.clear(PacketFlags::RX_CHECKSUM_OK);
        assert!(!flags.has(PacketFlags::RX_CHECKSUM_OK));
        assert!(flags.has(PacketFlags::VLAN_STRIPPED));

        // Verify all constant values are distinct powers-of-2 (no overlap)
        let constants = [
            PacketFlags::RX_CHECKSUM_OK,
            PacketFlags::TX_CHECKSUM_OFFLOAD,
            PacketFlags::VLAN_STRIPPED,
            PacketFlags::SCATTER_GATHER,
            PacketFlags::MULTICAST,
            PacketFlags::BROADCAST,
            PacketFlags::TX_TSO,
            PacketFlags::MIRROR,
        ];
        for (i, &a) in constants.iter().enumerate() {
            for &b in &constants[i + 1..] {
                assert_eq!(a & b, 0, "flag overlap: {:#x} & {:#x}", a, b);
            }
        }
    }

    // ──────────────────────────────────────
    // 26. fill_buf with data > mtu → error
    // ──────────────────────────────────────
    #[test]
    fn test_mtu_buffer_limit() {
        let pmd = PollModeDriver::new(BypassNetConfig {
            mtu: 100,
            ..small_config()
        })
        .unwrap();
        let mut desc = pmd.alloc_buf().unwrap();
        let big_data = vec![0u8; 101];
        let err = pmd.fill_buf(&mut desc, &big_data).unwrap_err();
        assert!(matches!(
            err,
            BypassNetError::BufferTooLarge { size: 101, max: 100 }
        ));
        pmd.free_buf(desc);
    }

    // ──────────────────────────────────────
    // 27. SPSC FIFO ordering
    // ──────────────────────────────────────
    #[test]
    fn test_ring_spsc_ordering() {
        let mut ring = PacketRing::new(64).unwrap();
        let n = 50usize;
        for i in 0..n {
            ring.enqueue(PacketDesc::new(i, 0, i, PacketFlags::default()))
                .unwrap();
        }
        let mut out = vec![PacketDesc::new(0, 0, 0, PacketFlags::default()); n];
        let got = ring.dequeue_burst(&mut out);
        assert_eq!(got, n);
        for (i, desc) in out[..n].iter().enumerate() {
            assert_eq!(desc.buf_idx, i, "FIFO violation at position {}", i);
            assert_eq!(desc.data_len, i);
        }
    }
}
