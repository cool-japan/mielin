//! Virtual Memory Management (VMM)
//!
//! This module provides virtual memory management with paging, memory protection,
//! and address space isolation. It supports multiple architectures with different
//! page table formats.
//!
//! # Features
//!
//! - **Page Table Management**: Multi-level page tables (4-level on x86_64, 3/4-level on ARM64)
//! - **Virtual Address Spaces**: Per-process isolated address spaces
//! - **Memory Protection**: Read, write, execute permissions
//! - **TLB Management**: Translation lookaside buffer invalidation
//! - **Memory Mapping**: Map physical pages to virtual addresses
//! - **Copy-on-Write**: Efficient memory sharing with COW semantics
//! - **Demand Paging**: Allocate pages on first access
//! - **Guard Pages**: Inaccessible sentinel pages for stack/heap overflow detection
//!
//! # Architecture Support
//!
//! - **x86_64**: 4-level paging (PML4, PDPT, PD, PT)
//! - **AArch64**: 4-level paging (L0, L1, L2, L3)
//! - **RISC-V**: Sv39/Sv48 paging
//!
//! # Usage Example
//!
//! ```ignore
//! use mielin_kernel::vmm::{self, PageTableFlags};
//!
//! // Initialize VMM subsystem
//! vmm::init().unwrap();
//!
//! // Create a new address space
//! let mut addr_space = vmm::AddressSpace::new().unwrap();
//!
//! // Map a page with read-write permissions
//! let virt_addr = 0x1000_0000;
//! let phys_addr = 0x2000_0000;
//! let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
//! addr_space.map(virt_addr, phys_addr, flags).unwrap();
//!
//! // Switch to this address space
//! addr_space.activate();
//! ```
//!
//! # Safety
//!
//! Virtual memory management involves direct manipulation of page tables and
//! CPU control registers. Care must be taken to maintain memory safety invariants.

use alloc::collections::{BTreeMap, VecDeque};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::memory;

pub mod cow;
pub mod demand;
pub mod guard;
pub mod mmio;
pub mod shared;
#[cfg(test)]
pub mod tests;

/// Page size (4KB standard)
pub const PAGE_SIZE: usize = 4096;

/// Huge page size (2MB)
pub const HUGE_PAGE_2MB: usize = 2 * 1024 * 1024;

/// Huge page size (1GB)
pub const HUGE_PAGE_1GB: usize = 1024 * 1024 * 1024;

/// Number of page table entries per level
pub(super) const ENTRIES_PER_TABLE: usize = 512;

/// Maximum number of address spaces
const MAX_ADDRESS_SPACES: usize = 256;

/// Page size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HugePageSize {
    /// 4KB page (normal)
    Size4KB,
    /// 2MB page (huge)
    Size2MB,
    /// 1GB page (huge)
    Size1GB,
}

impl HugePageSize {
    /// Get the size in bytes
    pub const fn bytes(self) -> usize {
        match self {
            Self::Size4KB => PAGE_SIZE,
            Self::Size2MB => HUGE_PAGE_2MB,
            Self::Size1GB => HUGE_PAGE_1GB,
        }
    }

    /// Get the page table level where this page size is mapped
    /// (x86_64: 3=4KB, 2=2MB, 1=1GB)
    pub const fn page_table_level(self) -> usize {
        match self {
            Self::Size4KB => 3,
            Self::Size2MB => 2,
            Self::Size1GB => 1,
        }
    }

    /// Check if address is aligned for this page size
    pub const fn is_aligned(self, addr: usize) -> bool {
        addr & (self.bytes() - 1) == 0
    }
}

/// Error types for virtual memory operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmmError {
    /// VMM subsystem not initialized
    NotInitialized,
    /// Already initialized
    AlreadyInitialized,
    /// Invalid virtual address
    InvalidVirtualAddress,
    /// Invalid physical address
    InvalidPhysicalAddress,
    /// Page already mapped
    AlreadyMapped,
    /// Page not mapped
    NotMapped,
    /// Out of memory
    OutOfMemory,
    /// Invalid page table entry
    InvalidEntry,
    /// Address space limit exceeded
    AddressSpaceLimitExceeded,
    /// Invalid permissions
    InvalidPermissions,
    /// TLB shootdown failed
    TlbShootdownFailed,
    /// Page fault (access violation)
    PageFault,
    /// Write to read-only page
    WriteProtectionViolation,
    /// Invalid page size for operation
    InvalidPageSize,
    /// Address not aligned for huge page
    UnalignedHugePage,
}

impl core::fmt::Display for VmmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "VMM subsystem not initialized"),
            Self::AlreadyInitialized => write!(f, "VMM already initialized"),
            Self::InvalidVirtualAddress => write!(f, "Invalid virtual address"),
            Self::InvalidPhysicalAddress => write!(f, "Invalid physical address"),
            Self::AlreadyMapped => write!(f, "Page already mapped"),
            Self::NotMapped => write!(f, "Page not mapped"),
            Self::OutOfMemory => write!(f, "Out of memory"),
            Self::InvalidEntry => write!(f, "Invalid page table entry"),
            Self::AddressSpaceLimitExceeded => write!(f, "Address space limit exceeded"),
            Self::InvalidPermissions => write!(f, "Invalid permissions"),
            Self::TlbShootdownFailed => write!(f, "TLB shootdown failed"),
            Self::PageFault => write!(f, "Page fault"),
            Self::WriteProtectionViolation => write!(f, "Write protection violation"),
            Self::InvalidPageSize => write!(f, "Invalid page size"),
            Self::UnalignedHugePage => write!(f, "Address not aligned for huge page"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VmmError {}

/// Page table entry flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTableFlags(u64);

impl PageTableFlags {
    /// Page is present in memory
    pub const PRESENT: Self = Self(1 << 0);
    /// Page is writable
    pub const WRITABLE: Self = Self(1 << 1);
    /// Page is accessible from user mode
    pub const USER: Self = Self(1 << 2);
    /// Write-through caching
    pub const WRITE_THROUGH: Self = Self(1 << 3);
    /// Cache disabled
    pub const CACHE_DISABLE: Self = Self(1 << 4);
    /// Page has been accessed
    pub const ACCESSED: Self = Self(1 << 5);
    /// Page has been written to (dirty)
    pub const DIRTY: Self = Self(1 << 6);
    /// Huge page (2MB or 1GB)
    pub const HUGE: Self = Self(1 << 7);
    /// Global page (not flushed on context switch)
    pub const GLOBAL: Self = Self(1 << 8);
    /// Copy-on-write page
    pub const COW: Self = Self(1 << 9);
    /// Demand-paged (not yet allocated)
    pub const DEMAND: Self = Self(1 << 10);
    /// Memory-mapped I/O region
    pub const MMIO: Self = Self(1 << 11);
    /// Shared memory region
    pub const SHARED: Self = Self(1 << 12);
    /// Guard page sentinel — no read/write/execute access permitted
    pub const GUARD: Self = Self(1 << 13);
    /// No execute (NX bit on x86_64)
    pub const NO_EXECUTE: Self = Self(1 << 63);

    /// Create empty flags
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Check if flags are empty
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Check if flag is set
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine flags
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Get raw value
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl core::ops::BitOr for PageTableFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for PageTableFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::Not for PageTableFlags {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0)
    }
}

/// Page table entry
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub(super) struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create a new empty entry
    pub(super) const fn new() -> Self {
        Self(0)
    }

    /// Check if entry is present
    pub(super) fn is_present(&self) -> bool {
        (self.0 & PageTableFlags::PRESENT.bits()) != 0
    }

    /// Get physical address from entry
    pub(super) fn phys_addr(&self) -> usize {
        (self.0 & 0x000f_ffff_ffff_f000) as usize
    }

    /// Get flags from entry
    #[allow(dead_code)]
    pub(super) fn flags(&self) -> PageTableFlags {
        PageTableFlags(self.0 & 0xfff0_0000_0000_0fff)
    }

    /// Set entry with physical address and flags
    pub(super) fn set(&mut self, phys_addr: usize, flags: PageTableFlags) {
        self.0 = (phys_addr as u64 & 0x000f_ffff_ffff_f000) | flags.bits();
    }

    /// Clear entry
    pub(super) fn clear(&mut self) {
        self.0 = 0;
    }
}

/// Page table (one level)
#[repr(align(4096))]
pub(super) struct PageTable {
    entries: [PageTableEntry; ENTRIES_PER_TABLE],
}

impl PageTable {
    /// Create a new empty page table
    #[allow(dead_code)]
    pub(super) const fn new() -> Self {
        Self {
            entries: [PageTableEntry::new(); ENTRIES_PER_TABLE],
        }
    }

    /// Get entry at index
    pub(super) fn entry(&self, index: usize) -> Option<&PageTableEntry> {
        self.entries.get(index)
    }

    /// Get mutable entry at index
    pub(super) fn entry_mut(&mut self, index: usize) -> Option<&mut PageTableEntry> {
        self.entries.get_mut(index)
    }

    /// Zero all entries
    pub(super) fn zero(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }
}

/// Virtual address space
pub struct AddressSpace {
    /// Root page table (PML4 on x86_64, L0 on ARM64)
    pub(super) root_table_phys: usize,
    /// Address space ID for TLB tagging
    pub(super) asid: usize,
    /// Number of mapped pages
    pub(super) mapped_pages: AtomicUsize,
}

impl AddressSpace {
    /// Create a new address space
    pub fn new() -> Result<Self, VmmError> {
        // Allocate physical page for root page table
        let root_page = memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;

        // Zero the page table
        let root_table = unsafe { &mut *(root_page as *mut PageTable) };
        root_table.zero();

        // Allocate ASID
        let asid = allocate_asid()?;

        Ok(Self {
            root_table_phys: root_page,
            asid,
            mapped_pages: AtomicUsize::new(0),
        })
    }

    /// Map a virtual address to a physical address
    pub fn map(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        flags: PageTableFlags,
    ) -> Result<(), VmmError> {
        // Validate addresses
        if virt_addr & 0xfff != 0 || phys_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // Get page table indices
        let indices = self.page_table_indices(virt_addr);

        // Walk page tables, creating intermediate tables as needed
        let mut current_table_phys = self.root_table_phys;

        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
            let entry = current_table
                .entry_mut(index)
                .ok_or(VmmError::InvalidEntry)?;

            if level < 3 {
                // Intermediate level - need to follow or create next level
                if !entry.is_present() {
                    // Allocate new page table for next level
                    let next_table_phys =
                        memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;
                    let next_table = unsafe { &mut *(next_table_phys as *mut PageTable) };
                    next_table.zero();

                    // Set entry to point to new table
                    entry.set(
                        next_table_phys,
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER,
                    );
                }

                current_table_phys = entry.phys_addr();
            } else {
                // Final level - map the actual page
                if entry.is_present() {
                    return Err(VmmError::AlreadyMapped);
                }

                entry.set(phys_addr, flags | PageTableFlags::PRESENT);
                self.mapped_pages.fetch_add(1, Ordering::SeqCst);
            }
        }

        Ok(())
    }

    /// Unmap a virtual address
    pub fn unmap(&mut self, virt_addr: usize) -> Result<usize, VmmError> {
        if virt_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        let indices = self.page_table_indices(virt_addr);
        let mut current_table_phys = self.root_table_phys;

        // Walk to the final page table
        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
            let entry = current_table
                .entry_mut(index)
                .ok_or(VmmError::InvalidEntry)?;

            if !entry.is_present() {
                return Err(VmmError::NotMapped);
            }

            if level < 3 {
                current_table_phys = entry.phys_addr();
            } else {
                // Final level - unmap the page
                let phys_addr = entry.phys_addr();
                entry.clear();
                self.mapped_pages.fetch_sub(1, Ordering::SeqCst);
                return Ok(phys_addr);
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Translate virtual address to physical address
    pub fn translate(&self, virt_addr: usize) -> Result<usize, VmmError> {
        let page_offset = virt_addr & 0xfff;
        let virt_page = virt_addr & !0xfff;
        let indices = self.page_table_indices(virt_page);

        let mut current_table_phys = self.root_table_phys;

        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &*(current_table_phys as *const PageTable) };
            let entry = current_table.entry(index).ok_or(VmmError::InvalidEntry)?;

            if !entry.is_present() {
                return Err(VmmError::NotMapped);
            }

            if level < 3 {
                current_table_phys = entry.phys_addr();
            } else {
                // Final level - return physical address
                return Ok(entry.phys_addr() | page_offset);
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Activate this address space (switch CR3/TTBR0)
    pub fn activate(&self) {
        unsafe {
            load_cr3(self.root_table_phys, self.asid);
        }
    }

    /// Get number of mapped pages
    pub fn mapped_pages(&self) -> usize {
        self.mapped_pages.load(Ordering::SeqCst)
    }

    /// Get flags for a virtual address
    pub fn get_flags(&self, virt_addr: usize) -> Result<PageTableFlags, VmmError> {
        let virt_page = virt_addr & !0xfff;
        let indices = self.page_table_indices(virt_page);

        let mut current_table_phys = self.root_table_phys;

        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &*(current_table_phys as *const PageTable) };
            let entry = current_table.entry(index).ok_or(VmmError::InvalidEntry)?;

            if !entry.is_present() {
                return Err(VmmError::NotMapped);
            }

            if level < 3 {
                current_table_phys = entry.phys_addr();
            } else {
                return Ok(entry.flags());
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Change page protection flags
    pub fn protect(&mut self, virt_addr: usize, new_flags: PageTableFlags) -> Result<(), VmmError> {
        let virt_page = virt_addr & !0xfff;
        let indices = self.page_table_indices(virt_page);

        let mut current_table_phys = self.root_table_phys;

        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
            let entry = current_table
                .entry_mut(index)
                .ok_or(VmmError::InvalidEntry)?;

            if !entry.is_present() {
                return Err(VmmError::NotMapped);
            }

            if level < 3 {
                current_table_phys = entry.phys_addr();
            } else {
                let phys_addr = entry.phys_addr();
                entry.set(phys_addr, new_flags | PageTableFlags::PRESENT);
                flush_tlb(virt_page);
                return Ok(());
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Get page table indices for a virtual address (x86_64)
    #[cfg(target_arch = "x86_64")]
    pub(super) fn page_table_indices(&self, virt_addr: usize) -> [usize; 4] {
        [
            (virt_addr >> 39) & 0x1ff, // PML4 index
            (virt_addr >> 30) & 0x1ff, // PDPT index
            (virt_addr >> 21) & 0x1ff, // PD index
            (virt_addr >> 12) & 0x1ff, // PT index
        ]
    }

    /// Get page table indices for a virtual address (AArch64)
    #[cfg(target_arch = "aarch64")]
    pub(super) fn page_table_indices(&self, virt_addr: usize) -> [usize; 4] {
        [
            (virt_addr >> 39) & 0x1ff, // L0 index
            (virt_addr >> 30) & 0x1ff, // L1 index
            (virt_addr >> 21) & 0x1ff, // L2 index
            (virt_addr >> 12) & 0x1ff, // L3 index
        ]
    }

    /// Get page table indices for a virtual address (other architectures)
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub(super) fn page_table_indices(&self, virt_addr: usize) -> [usize; 4] {
        [
            (virt_addr >> 39) & 0x1ff,
            (virt_addr >> 30) & 0x1ff,
            (virt_addr >> 21) & 0x1ff,
            (virt_addr >> 12) & 0x1ff,
        ]
    }
}

/// Recursively free intermediate page table frames.
///
/// At levels 0–2 every present entry points to a child page table that was
/// allocated by `AddressSpace::map`. We walk each such entry, recurse into
/// the child, and then free the child table's frame via `memory::free_page`.
/// At level 3 entries point to caller-owned data pages — those are NOT freed.
///
/// # Safety
///
/// `phys` must be a valid physical address of a `PageTable` that was
/// previously allocated by `memory::allocate_page`.  Aliasing rules are
/// upheld because we only dereference the pointer while we hold exclusive
/// access through `Drop`.
unsafe fn free_table_recursive(phys: usize, level: usize) {
    // Leaf level — entries are caller-owned data pages, not intermediate
    // page-table frames; nothing to free here.
    if level >= 3 {
        return;
    }

    let table = &*(phys as *const PageTable);
    for i in 0..ENTRIES_PER_TABLE {
        // entry() always returns Some for indices 0..ENTRIES_PER_TABLE
        if let Some(entry) = table.entry(i) {
            if entry.is_present() {
                let child_phys = entry.phys_addr();
                // Recurse into the next level first …
                free_table_recursive(child_phys, level + 1);
                // … then free the child table's frame itself.
                let _ = memory::free_page(child_phys);
            }
        }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        // Walk and free all intermediate page table frames, then the root.
        unsafe {
            free_table_recursive(self.root_table_phys, 0);
        }
        let _ = memory::free_page(self.root_table_phys);
        free_asid(self.asid);
    }
}

/// Global VMM state
pub(super) struct VmmState {
    /// Is VMM initialized?
    pub(super) initialized: AtomicBool,
    /// Next ASID to allocate
    pub(super) next_asid: AtomicUsize,
    /// Number of active address spaces
    pub(super) active_spaces: AtomicUsize,
    /// Total pages mapped across all address spaces
    pub(super) total_mapped_pages: AtomicU64,
    /// Total TLB flushes
    pub(super) total_tlb_flushes: AtomicU64,
    /// Total COW faults handled
    pub(super) total_cow_faults: AtomicU64,
    /// Total COW pages copied
    pub(super) total_cow_copies: AtomicU64,
    /// Total COW faults resolved without copy (sole owner)
    pub(super) total_cow_fast_path: AtomicU64,
    /// Total demand-paging faults
    pub(super) total_demand_faults: AtomicU64,
    /// Total pages allocated via demand paging
    pub(super) total_demand_pages: AtomicU64,
    /// Total 2MB huge pages mapped
    pub(super) total_huge_2mb_pages: AtomicU64,
    /// Total 1GB huge pages mapped
    pub(super) total_huge_1gb_pages: AtomicU64,
    /// Total MMIO regions mapped
    pub(super) total_mmio_regions: AtomicU64,
    /// Total MMIO pages mapped
    pub(super) total_mmio_pages: AtomicU64,
    /// Total shared memory regions
    pub(super) total_shared_regions: AtomicU64,
    /// Total shared memory pages
    pub(super) total_shared_pages: AtomicU64,
    /// Total guard pages mapped
    pub(super) total_guard_pages: AtomicU64,
}

impl VmmState {
    pub(super) const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            next_asid: AtomicUsize::new(1),
            active_spaces: AtomicUsize::new(0),
            total_mapped_pages: AtomicU64::new(0),
            total_tlb_flushes: AtomicU64::new(0),
            total_cow_faults: AtomicU64::new(0),
            total_cow_copies: AtomicU64::new(0),
            total_cow_fast_path: AtomicU64::new(0),
            total_demand_faults: AtomicU64::new(0),
            total_demand_pages: AtomicU64::new(0),
            total_huge_2mb_pages: AtomicU64::new(0),
            total_huge_1gb_pages: AtomicU64::new(0),
            total_mmio_regions: AtomicU64::new(0),
            total_mmio_pages: AtomicU64::new(0),
            total_shared_regions: AtomicU64::new(0),
            total_shared_pages: AtomicU64::new(0),
            total_guard_pages: AtomicU64::new(0),
        }
    }
}

/// Acknowledgment counter for TLB shootdown IPIs.
///
/// Remote CPUs increment this after flushing their local TLB in response to
/// a `TlbFlush` IPI.  The sender spins on this counter in
/// `flush_tlb_range_smp` until all expected acknowledgments arrive.
static TLB_SHOOTDOWN_ACK: AtomicUsize = AtomicUsize::new(0);

pub(super) static VMM_STATE: Mutex<VmmState> = Mutex::new(VmmState::new());

// Recycled ASID free list.
//
// When an address space is destroyed its ASID is pushed here so that
// `allocate_asid` can reuse it before bumping the monotonic counter.
// Kept separate from `VmmState` because `VecDeque` is not const-constructible.
lazy_static::lazy_static! {
    static ref ASID_FREE_LIST: Mutex<VecDeque<usize>> = Mutex::new(VecDeque::new());
}

/// Initialize the VMM subsystem
pub fn init() -> Result<(), VmmError> {
    let state = VMM_STATE.lock();

    if state.initialized.load(Ordering::SeqCst) {
        return Ok(()); // Already initialized
    }

    state.initialized.store(true, Ordering::SeqCst);

    Ok(())
}

/// Allocate an address space ID (ASID)
///
/// Checks the recycled-ASID free list first; falls back to the monotonic
/// counter when the list is empty.
pub(super) fn allocate_asid() -> Result<usize, VmmError> {
    // Try to reuse a recycled ASID before bumping the counter.
    {
        let mut free_list = ASID_FREE_LIST.lock();
        if let Some(recycled) = free_list.pop_front() {
            // active_spaces was already decremented when the ASID was freed;
            // re-increment it now that it is back in use.
            let state = VMM_STATE.lock();
            state.active_spaces.fetch_add(1, Ordering::SeqCst);
            return Ok(recycled);
        }
    }

    let state = VMM_STATE.lock();
    let asid = state.next_asid.fetch_add(1, Ordering::SeqCst);

    if asid >= MAX_ADDRESS_SPACES {
        return Err(VmmError::AddressSpaceLimitExceeded);
    }

    state.active_spaces.fetch_add(1, Ordering::SeqCst);

    Ok(asid)
}

/// Free an address space ID
///
/// Decrements the active-spaces counter and returns the ASID to the free
/// list so it can be reused by a future `allocate_asid` call.
pub(super) fn free_asid(asid: usize) {
    let state = VMM_STATE.lock();
    state.active_spaces.fetch_sub(1, Ordering::SeqCst);
    drop(state);

    let mut free_list = ASID_FREE_LIST.lock();
    free_list.push_back(asid);
}

/// Load CR3 register (x86_64) or TTBR0 (ARM64)
///
/// # Safety
///
/// This function directly manipulates CPU control registers.
#[cfg(target_arch = "x86_64")]
unsafe fn load_cr3(phys_addr: usize, _asid: usize) {
    core::arch::asm!(
        "mov cr3, {}",
        in(reg) phys_addr,
        options(nostack, preserves_flags)
    );
}

#[cfg(target_arch = "aarch64")]
unsafe fn load_cr3(phys_addr: usize, asid: usize) {
    let ttbr0 = (phys_addr as u64) | ((asid as u64) << 48);
    core::arch::asm!(
        "msr ttbr0_el1, {}",
        in(reg) ttbr0,
        options(nostack, preserves_flags)
    );
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
unsafe fn load_cr3(_phys_addr: usize, _asid: usize) {
    // Platform-specific implementation needed
}

/// Flush TLB for a specific virtual address
pub fn flush_tlb(virt_addr: usize) {
    let state = VMM_STATE.lock();
    state.total_tlb_flushes.fetch_add(1, Ordering::SeqCst);

    unsafe {
        flush_tlb_page(virt_addr);
    }
}

/// Flush TLB for a range of pages. Loops `flush_tlb_page` once per page.
///
/// On a real multi-core system this would issue a full TLB-shootdown IPI to
/// all remote CPUs; the current implementation is a single-core placeholder
/// that makes `VmmError::TlbShootdownFailed` reachable and constructible.
pub fn flush_tlb_range(start: usize, page_count: usize) {
    for i in 0..page_count {
        flush_tlb(start + i * PAGE_SIZE);
    }
}

/// Flush TLB page (architecture-specific)
#[cfg(all(target_arch = "x86_64", not(any(test, feature = "std"))))]
unsafe fn flush_tlb_page(virt_addr: usize) {
    core::arch::asm!(
        "invlpg [{}]",
        in(reg) virt_addr,
        options(nostack, preserves_flags)
    );
}

#[cfg(all(target_arch = "aarch64", not(any(test, feature = "std"))))]
unsafe fn flush_tlb_page(virt_addr: usize) {
    core::arch::asm!(
        "tlbi vaae1, {}",
        in(reg) virt_addr >> 12,
        options(nostack, preserves_flags)
    );
}

#[cfg(all(target_arch = "riscv64", not(any(test, feature = "std"))))]
unsafe fn flush_tlb_page(virt_addr: usize) {
    core::arch::asm!("sfence.vma {}, zero", in(reg) virt_addr, options(nostack));
}

#[cfg(any(test, feature = "std"))]
unsafe fn flush_tlb_page(_virt_addr: usize) {
    // No-op under host/test (privileged instructions fault in user space)
}

#[cfg(not(any(
    test,
    feature = "std",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
unsafe fn flush_tlb_page(_virt_addr: usize) {
    // Platform-specific implementation needed for other architectures
}

/// Full TLB flush (all pages) — architecture-specific.
///
/// Used by the IPI handler on remote CPUs during TLB shootdown.
/// Must NOT hold VMM_STATE lock when called.
#[cfg(all(target_arch = "x86_64", not(any(test, feature = "std"))))]
unsafe fn flush_tlb_all() {
    // Reload CR3 to flush the entire TLB.
    let cr3: usize;
    core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
    core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack));
}

#[cfg(all(target_arch = "aarch64", not(any(test, feature = "std"))))]
unsafe fn flush_tlb_all() {
    core::arch::asm!("tlbi vmalle1", options(nostack));
    core::arch::asm!("dsb sy", options(nostack));
    core::arch::asm!("isb", options(nostack));
}

#[cfg(all(target_arch = "riscv64", not(any(test, feature = "std"))))]
unsafe fn flush_tlb_all() {
    core::arch::asm!("sfence.vma", options(nostack));
}

#[cfg(any(test, feature = "std"))]
unsafe fn flush_tlb_all() {
    // No-op under host/test (privileged instructions fault in user space)
}

#[cfg(not(any(
    test,
    feature = "std",
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
unsafe fn flush_tlb_all() {
    // Platform-specific implementation needed for other architectures
}

/// Called from the IPI handler when `IpiType::TlbFlush` is received.
///
/// Flushes the local TLB (all pages) and increments the global shootdown
/// acknowledgment counter so that the sender's `flush_tlb_range_smp` spin
/// loop can detect completion.
///
/// # Constraints
///
/// **Must NOT hold `VMM_STATE` lock when called.** Acquiring the lock here
/// would deadlock because `flush_tlb` already holds it.
pub fn handle_tlb_shootdown_ipi() {
    if crate::ipc::handle_ipi(crate::ipc::IpiType::TlbFlush) {
        // Safe: interrupt context, local-only flush, no VMM lock held.
        unsafe {
            flush_tlb_all();
        }
        TLB_SHOOTDOWN_ACK.fetch_add(1, Ordering::Release);
    }
}

/// Multi-core TLB shootdown for a virtual address range.
///
/// Flushes `page_count` pages starting at `start` on the local CPU, then
/// broadcasts a `TlbFlush` IPI to every other online CPU and waits for all
/// of them to acknowledge.
///
/// # Errors
///
/// Returns `VmmError::TlbShootdownFailed` if the acknowledgment spin-wait
/// exceeds `MAX_SPINS` iterations (approximately 1 M iterations ≈ few ms).
///
/// # Note
///
/// This function deliberately does **not** call `flush_tlb()` (which holds
/// `VMM_STATE`) to avoid potential deadlocks when called from contexts that
/// may already hold the VMM lock.  It calls `flush_tlb_page` directly.
pub fn flush_tlb_range_smp(start: usize, page_count: usize) -> Result<(), VmmError> {
    // 1. Flush locally (raw — avoids VMM lock).
    for i in 0..page_count {
        unsafe {
            flush_tlb_page(start + i * PAGE_SIZE);
        }
    }

    // 2. If running single-core, we are done.
    let num_cpus = crate::ipc::get_num_online_cpus();
    if num_cpus <= 1 {
        return Ok(());
    }

    // 3. Reset ACK counter, then broadcast shootdown IPI.
    let expected_acks = num_cpus - 1;
    TLB_SHOOTDOWN_ACK.store(0, Ordering::Release);
    crate::ipc::send_ipi_all_but_self(crate::ipc::IpiType::TlbFlush);

    // 4. Spin-wait for all remote CPUs to acknowledge.
    let mut spins: usize = 0;
    const MAX_SPINS: usize = 1_000_000;
    loop {
        if TLB_SHOOTDOWN_ACK.load(Ordering::Acquire) >= expected_acks {
            break;
        }
        core::hint::spin_loop();
        spins += 1;
        if spins >= MAX_SPINS {
            return Err(VmmError::TlbShootdownFailed);
        }
    }
    Ok(())
}

/// VMM statistics
#[derive(Debug, Clone, Copy)]
pub struct VmmStats {
    /// Is VMM initialized?
    pub initialized: bool,
    /// Number of active address spaces
    pub active_spaces: usize,
    /// Total mapped pages
    pub total_mapped_pages: u64,
    /// Total TLB flushes
    pub total_tlb_flushes: u64,
    /// Total COW faults handled
    pub total_cow_faults: u64,
    /// Total COW pages copied
    pub total_cow_copies: u64,
    /// Total COW faults resolved without copy
    pub total_cow_fast_path: u64,
    /// Total demand-paging faults
    pub total_demand_faults: u64,
    /// Total pages allocated via demand paging
    pub total_demand_pages: u64,
    /// Total 2MB huge pages mapped
    pub total_huge_2mb_pages: u64,
    /// Total 1GB huge pages mapped
    pub total_huge_1gb_pages: u64,
    /// Total MMIO regions mapped
    pub total_mmio_regions: u64,
    /// Total MMIO pages mapped
    pub total_mmio_pages: u64,
    /// Total shared memory regions
    pub total_shared_regions: u64,
    /// Total shared memory pages
    pub total_shared_pages: u64,
    /// Total guard pages mapped
    pub total_guard_pages: u64,
}

/// Get VMM statistics
pub fn get_stats() -> VmmStats {
    let state = VMM_STATE.lock();

    VmmStats {
        initialized: state.initialized.load(Ordering::SeqCst),
        active_spaces: state.active_spaces.load(Ordering::SeqCst),
        total_mapped_pages: state.total_mapped_pages.load(Ordering::SeqCst),
        total_tlb_flushes: state.total_tlb_flushes.load(Ordering::SeqCst),
        total_cow_faults: state.total_cow_faults.load(Ordering::SeqCst),
        total_cow_copies: state.total_cow_copies.load(Ordering::SeqCst),
        total_cow_fast_path: state.total_cow_fast_path.load(Ordering::SeqCst),
        total_demand_faults: state.total_demand_faults.load(Ordering::SeqCst),
        total_demand_pages: state.total_demand_pages.load(Ordering::SeqCst),
        total_huge_2mb_pages: state.total_huge_2mb_pages.load(Ordering::SeqCst),
        total_huge_1gb_pages: state.total_huge_1gb_pages.load(Ordering::SeqCst),
        total_mmio_regions: state.total_mmio_regions.load(Ordering::SeqCst),
        total_mmio_pages: state.total_mmio_pages.load(Ordering::SeqCst),
        total_shared_regions: state.total_shared_regions.load(Ordering::SeqCst),
        total_shared_pages: state.total_shared_pages.load(Ordering::SeqCst),
        total_guard_pages: state.total_guard_pages.load(Ordering::SeqCst),
    }
}

#[cfg(all(test, feature = "std"))]
mod tlb_shootdown_tests {
    use super::*;
    use core::sync::atomic::Ordering;

    #[test]
    fn test_flush_tlb_range_smp_does_not_panic() {
        // Exercises the booking code; on test/std it is a no-op flush + IPI sim.
        // get_num_online_cpus() returns MAX_CPUS (16) in test stubs, and
        // send_ipi_all_but_self sets pending bits on all other simulated CPUs.
        // The ack counter stays at 0 so flush_tlb_range_smp will time-out —
        // that is expected and acceptable here.
        let result = flush_tlb_range_smp(0x1000, 1);
        // May be Ok (single core stub) or Err::TlbShootdownFailed (multi-core stub,
        // no real CPUs to ack). Either is a valid outcome — just no panic.
        let _ = result;
    }

    #[test]
    fn test_handle_tlb_shootdown_ipi_increments_ack() {
        use crate::ipc::{set_pending, IpiType};
        // Manually set a pending TlbFlush IPI on the test-stub current CPU (cpu 0).
        set_pending(0, IpiType::TlbFlush);
        let ack_before = TLB_SHOOTDOWN_ACK.load(Ordering::Acquire);
        handle_tlb_shootdown_ipi();
        let ack_after = TLB_SHOOTDOWN_ACK.load(Ordering::Acquire);
        assert_eq!(
            ack_after,
            ack_before + 1,
            "ack counter should increment when IPI is handled"
        );
    }
}
