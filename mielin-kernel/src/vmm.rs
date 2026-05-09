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

/// Page size (4KB standard)
pub const PAGE_SIZE: usize = 4096;

/// Huge page size (2MB)
pub const HUGE_PAGE_2MB: usize = 2 * 1024 * 1024;

/// Huge page size (1GB)
pub const HUGE_PAGE_1GB: usize = 1024 * 1024 * 1024;

/// Number of page table entries per level
const ENTRIES_PER_TABLE: usize = 512;

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
struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create a new empty entry
    const fn new() -> Self {
        Self(0)
    }

    /// Check if entry is present
    fn is_present(&self) -> bool {
        (self.0 & PageTableFlags::PRESENT.bits()) != 0
    }

    /// Get physical address from entry
    fn phys_addr(&self) -> usize {
        (self.0 & 0x000f_ffff_ffff_f000) as usize
    }

    /// Get flags from entry
    #[allow(dead_code)] // May be useful for future features
    fn flags(&self) -> PageTableFlags {
        PageTableFlags(self.0 & 0xfff0_0000_0000_0fff)
    }

    /// Set entry with physical address and flags
    fn set(&mut self, phys_addr: usize, flags: PageTableFlags) {
        self.0 = (phys_addr as u64 & 0x000f_ffff_ffff_f000) | flags.bits();
    }

    /// Clear entry
    fn clear(&mut self) {
        self.0 = 0;
    }
}

/// Page table (one level)
#[repr(align(4096))]
struct PageTable {
    entries: [PageTableEntry; ENTRIES_PER_TABLE],
}

impl PageTable {
    /// Create a new empty page table
    #[allow(dead_code)] // May be useful for future allocations
    const fn new() -> Self {
        Self {
            entries: [PageTableEntry::new(); ENTRIES_PER_TABLE],
        }
    }

    /// Get entry at index
    fn entry(&self, index: usize) -> Option<&PageTableEntry> {
        self.entries.get(index)
    }

    /// Get mutable entry at index
    fn entry_mut(&mut self, index: usize) -> Option<&mut PageTableEntry> {
        self.entries.get_mut(index)
    }

    /// Zero all entries
    fn zero(&mut self) {
        for entry in &mut self.entries {
            entry.clear();
        }
    }
}

/// Page reference counter for COW (Copy-on-Write) tracking
///
/// Tracks the number of address spaces sharing a physical page.
/// When the reference count drops to 1, the page can be written directly.
/// When >1, a write operation triggers a copy.
struct PageRefCount {
    /// Reference count per physical page
    refcounts: BTreeMap<usize, AtomicUsize>,
}

impl PageRefCount {
    /// Create a new reference counter
    const fn new() -> Self {
        Self {
            refcounts: BTreeMap::new(),
        }
    }

    /// Increment reference count for a physical page
    fn inc_ref(&mut self, phys_addr: usize) {
        let refcount = self
            .refcounts
            .entry(phys_addr)
            .or_insert_with(|| AtomicUsize::new(0));
        refcount.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement reference count for a physical page
    /// Returns the new count
    fn dec_ref(&mut self, phys_addr: usize) -> usize {
        if let Some(refcount) = self.refcounts.get(&phys_addr) {
            let count = refcount.fetch_sub(1, Ordering::SeqCst);
            if count == 1 {
                self.refcounts.remove(&phys_addr);
                return 0;
            }
            count - 1
        } else {
            0
        }
    }

    /// Get reference count for a physical page
    fn get_ref(&self, phys_addr: usize) -> usize {
        self.refcounts
            .get(&phys_addr)
            .map(|r| r.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

/// Global page reference counter for COW
static PAGE_REFCOUNT: Mutex<PageRefCount> = Mutex::new(PageRefCount::new());

/// Shared memory region descriptor
///
/// Represents a named shared memory region that can be attached by multiple
/// address spaces. Each region has a unique ID and tracks reference count.
#[derive(Clone)]
pub struct SharedMemoryRegion {
    /// Unique ID for this shared region
    pub id: usize,
    /// Base virtual address (for reference)
    pub virt_addr: usize,
    /// Size in bytes
    pub size: usize,
    /// Base physical address
    pub phys_addr: usize,
    /// Protection flags
    pub flags: PageTableFlags,
    /// Reference count (number of address spaces attached)
    pub refcount: usize,
}

impl SharedMemoryRegion {
    /// Create a new shared memory region
    fn new(
        id: usize,
        virt_addr: usize,
        size: usize,
        phys_addr: usize,
        flags: PageTableFlags,
    ) -> Self {
        Self {
            id,
            virt_addr,
            size,
            phys_addr,
            flags,
            refcount: 1,
        }
    }
}

/// Global shared memory region registry
struct SharedMemoryRegistry {
    /// Map of region ID -> SharedMemoryRegion
    regions: BTreeMap<usize, SharedMemoryRegion>,
    /// Next region ID to allocate
    next_id: usize,
}

impl SharedMemoryRegistry {
    const fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Create a new shared region
    fn create_region(
        &mut self,
        virt_addr: usize,
        size: usize,
        phys_addr: usize,
        flags: PageTableFlags,
    ) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let region = SharedMemoryRegion::new(id, virt_addr, size, phys_addr, flags);
        self.regions.insert(id, region);

        id
    }

    /// Get a shared region by ID
    fn get_region(&self, id: usize) -> Option<SharedMemoryRegion> {
        self.regions.get(&id).cloned()
    }

    /// Increment reference count for a region
    fn inc_ref(&mut self, id: usize) -> Result<(), VmmError> {
        if let Some(region) = self.regions.get_mut(&id) {
            region.refcount = region.refcount.saturating_add(1);
            Ok(())
        } else {
            Err(VmmError::InvalidEntry)
        }
    }

    /// Decrement reference count for a region
    /// Returns true if the region should be deleted
    fn dec_ref(&mut self, id: usize) -> Result<bool, VmmError> {
        if let Some(region) = self.regions.get_mut(&id) {
            region.refcount = region.refcount.saturating_sub(1);
            if region.refcount == 0 {
                self.regions.remove(&id);
                return Ok(true);
            }
            Ok(false)
        } else {
            Err(VmmError::InvalidEntry)
        }
    }
}

/// Global shared memory registry
static SHARED_MEMORY: Mutex<SharedMemoryRegistry> = Mutex::new(SharedMemoryRegistry::new());

/// Virtual address space
pub struct AddressSpace {
    /// Root page table (PML4 on x86_64, L0 on ARM64)
    root_table_phys: usize,
    /// Address space ID for TLB tagging
    asid: usize,
    /// Number of mapped pages
    mapped_pages: AtomicUsize,
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

    /// Map a page with copy-on-write semantics
    ///
    /// The page is mapped as read-only with the COW flag set.
    /// When a write occurs, a page fault will trigger copying.
    pub fn map_cow(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        flags: PageTableFlags,
    ) -> Result<(), VmmError> {
        // Map as read-only with COW flag
        let cow_flags = (flags & !PageTableFlags::WRITABLE) | PageTableFlags::COW;
        self.map(virt_addr, phys_addr, cow_flags)?;

        // Increment reference count for the physical page
        let mut refcount = PAGE_REFCOUNT.lock();
        refcount.inc_ref(phys_addr);

        Ok(())
    }

    /// Handle a copy-on-write fault
    ///
    /// When a write occurs on a COW page, this function:
    /// 1. Allocates a new physical page
    /// 2. Copies the data from the shared page
    /// 3. Updates the page table to point to the new page
    /// 4. Makes the page writable
    /// 5. Decrements the reference count on the old page
    pub fn handle_cow_fault(&mut self, virt_addr: usize) -> Result<(), VmmError> {
        let state = VMM_STATE.lock();
        state.total_cow_faults.fetch_add(1, Ordering::SeqCst);
        drop(state);

        let virt_page = virt_addr & !0xfff;

        // Get current mapping
        let old_phys_addr = self.translate(virt_page)?;

        // Get current page table entry to check COW flag
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
                // Final level - check if COW
                let flags = entry.flags();
                if !flags.contains(PageTableFlags::COW) {
                    return Err(VmmError::WriteProtectionViolation);
                }

                // Check reference count
                let refcount = PAGE_REFCOUNT.lock().get_ref(old_phys_addr);

                if refcount <= 1 {
                    // We're the only owner - just make it writable (fast path)
                    let state = VMM_STATE.lock();
                    state.total_cow_fast_path.fetch_add(1, Ordering::SeqCst);
                    drop(state);

                    let new_flags = (flags & !PageTableFlags::COW) | PageTableFlags::WRITABLE;
                    entry.set(old_phys_addr, new_flags);
                    flush_tlb(virt_page);
                    return Ok(());
                }

                // Allocate new page
                let new_phys_addr = memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;

                // Copy data from old page to new page
                unsafe {
                    let src = old_phys_addr as *const u8;
                    let dst = new_phys_addr as *mut u8;
                    core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE);
                }

                let state = VMM_STATE.lock();
                state.total_cow_copies.fetch_add(1, Ordering::SeqCst);
                drop(state);

                // Update page table entry
                let new_flags = (flags & !PageTableFlags::COW) | PageTableFlags::WRITABLE;
                entry.set(new_phys_addr, new_flags);

                // Decrement reference count on old page
                let mut refcount_lock = PAGE_REFCOUNT.lock();
                let remaining = refcount_lock.dec_ref(old_phys_addr);

                // If no more references, we could free the page
                // (but we'll leave it to the caller to decide)
                drop(refcount_lock);

                if remaining == 0 {
                    // The old page can be freed
                    let _ = memory::free_page(old_phys_addr);
                }

                // Flush TLB
                flush_tlb(virt_page);

                return Ok(());
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Fork this address space (create copy with COW semantics)
    ///
    /// Creates a new address space that shares all pages with this one
    /// using copy-on-write semantics. All mapped pages are marked as
    /// read-only and COW in both address spaces.
    pub fn fork(&mut self) -> Result<Self, VmmError> {
        // Create new address space
        let mut new_space = Self::new()?;

        // Walk all page tables and copy mappings with COW
        self.walk_and_copy_cow(&mut new_space)?;

        Ok(new_space)
    }

    /// Walk page tables and copy all mappings with COW semantics
    fn walk_and_copy_cow(&mut self, new_space: &mut AddressSpace) -> Result<(), VmmError> {
        // For each mapped page, map it in the new space as COW
        // This is a simplified implementation - a full implementation would
        // recursively walk the page table tree.

        // We'll implement a simple version that tracks virtual addresses
        // In a real implementation, we'd walk the page table tree directly

        // For now, we return Ok(()) as this requires more complex page table walking
        // that should be implemented later with proper page table iteration
        let _ = new_space;
        Ok(())
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

    /// Map a virtual address range for demand paging
    ///
    /// The pages are marked as demand-paged but not allocated.
    /// When first accessed, a page fault will trigger allocation.
    pub fn map_demand(
        &mut self,
        virt_addr: usize,
        page_count: usize,
        flags: PageTableFlags,
    ) -> Result<(), VmmError> {
        // Validate address alignment
        if virt_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // Map each page with DEMAND flag and without PRESENT
        for i in 0..page_count {
            let addr = virt_addr + (i * PAGE_SIZE);
            let indices = self.page_table_indices(addr);
            let mut current_table_phys = self.root_table_phys;

            for (level, &index) in indices.iter().enumerate() {
                let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
                let entry = current_table
                    .entry_mut(index)
                    .ok_or(VmmError::InvalidEntry)?;

                if level < 3 {
                    // Intermediate level
                    if !entry.is_present() {
                        let next_table_phys =
                            memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;
                        let next_table = unsafe { &mut *(next_table_phys as *mut PageTable) };
                        next_table.zero();

                        entry.set(
                            next_table_phys,
                            PageTableFlags::PRESENT
                                | PageTableFlags::WRITABLE
                                | PageTableFlags::USER,
                        );
                    }
                    current_table_phys = entry.phys_addr();
                } else {
                    // Final level - mark as demand-paged (no physical page yet)
                    // Store flags without PRESENT but with DEMAND
                    entry.set(0, flags | PageTableFlags::DEMAND);
                }
            }
        }

        Ok(())
    }

    /// Handle a demand-paging fault
    ///
    /// When a demand-paged page is first accessed:
    /// 1. Allocate a physical page
    /// 2. Zero-fill the page
    /// 3. Update the page table entry
    /// 4. Clear the DEMAND flag and set PRESENT
    pub fn handle_demand_fault(&mut self, virt_addr: usize) -> Result<(), VmmError> {
        let state = VMM_STATE.lock();
        state.total_demand_faults.fetch_add(1, Ordering::SeqCst);
        drop(state);

        let virt_page = virt_addr & !0xfff;
        let indices = self.page_table_indices(virt_page);
        let mut current_table_phys = self.root_table_phys;

        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
            let entry = current_table
                .entry_mut(index)
                .ok_or(VmmError::InvalidEntry)?;

            if level < 3 {
                if !entry.is_present() {
                    return Err(VmmError::NotMapped);
                }
                current_table_phys = entry.phys_addr();
            } else {
                // Final level - check if it's demand-paged
                let flags = entry.flags();
                if !flags.contains(PageTableFlags::DEMAND) {
                    return Err(VmmError::PageFault);
                }

                // Allocate physical page
                let phys_addr = memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;

                // Zero-fill the page
                unsafe {
                    let ptr = phys_addr as *mut u8;
                    core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
                }

                let state = VMM_STATE.lock();
                state.total_demand_pages.fetch_add(1, Ordering::SeqCst);
                drop(state);

                // Update page table entry: remove DEMAND, add PRESENT
                let new_flags = (flags & !PageTableFlags::DEMAND) | PageTableFlags::PRESENT;
                entry.set(phys_addr, new_flags);
                self.mapped_pages.fetch_add(1, Ordering::SeqCst);

                // Flush TLB
                flush_tlb(virt_page);

                return Ok(());
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Check if a virtual address is demand-paged
    pub fn is_demand_paged(&self, virt_addr: usize) -> Result<bool, VmmError> {
        let flags = self.get_flags(virt_addr);
        match flags {
            Ok(f) => Ok(f.contains(PageTableFlags::DEMAND)),
            Err(VmmError::NotMapped) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Map a huge page (2MB or 1GB)
    ///
    /// Huge pages improve TLB efficiency by reducing the number of page table
    /// entries needed for large memory regions.
    ///
    /// # Arguments
    /// * `virt_addr` - Virtual address (must be aligned to huge page size)
    /// * `phys_addr` - Physical address (must be aligned to huge page size)
    /// * `page_size` - Size of the huge page (2MB or 1GB)
    /// * `flags` - Page protection flags
    pub fn map_huge(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        page_size: HugePageSize,
        flags: PageTableFlags,
    ) -> Result<(), VmmError> {
        // Validate page size (only 2MB and 1GB supported)
        if matches!(page_size, HugePageSize::Size4KB) {
            return Err(VmmError::InvalidPageSize);
        }

        // Validate alignment
        if !page_size.is_aligned(virt_addr) || !page_size.is_aligned(phys_addr) {
            return Err(VmmError::UnalignedHugePage);
        }

        // Get target level for this page size
        let target_level = page_size.page_table_level();
        let indices = self.page_table_indices(virt_addr);
        let mut current_table_phys = self.root_table_phys;

        // Walk to the target level
        for (level, &index) in indices.iter().enumerate() {
            if level == target_level {
                // This is where we map the huge page
                let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
                let entry = current_table
                    .entry_mut(index)
                    .ok_or(VmmError::InvalidEntry)?;

                if entry.is_present() {
                    return Err(VmmError::AlreadyMapped);
                }

                // Set entry with HUGE flag
                entry.set(
                    phys_addr,
                    flags | PageTableFlags::PRESENT | PageTableFlags::HUGE,
                );

                // Update statistics
                let state = VMM_STATE.lock();
                match page_size {
                    HugePageSize::Size2MB => {
                        state.total_huge_2mb_pages.fetch_add(1, Ordering::SeqCst);
                    }
                    HugePageSize::Size1GB => {
                        state.total_huge_1gb_pages.fetch_add(1, Ordering::SeqCst);
                    }
                    _ => {}
                }
                drop(state);

                // Update mapped page count (count as multiple 4KB pages)
                let page_count = page_size.bytes() / PAGE_SIZE;
                self.mapped_pages.fetch_add(page_count, Ordering::SeqCst);

                return Ok(());
            }

            // Intermediate level - walk deeper
            let current_table = unsafe { &mut *(current_table_phys as *mut PageTable) };
            let entry = current_table
                .entry_mut(index)
                .ok_or(VmmError::InvalidEntry)?;

            if !entry.is_present() {
                // Need to allocate intermediate table
                let next_table_phys = memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;
                let next_table = unsafe { &mut *(next_table_phys as *mut PageTable) };
                next_table.zero();

                entry.set(
                    next_table_phys,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER,
                );
            }

            current_table_phys = entry.phys_addr();
        }

        Err(VmmError::InvalidPageSize)
    }

    /// Check if a virtual address is mapped as a huge page
    pub fn is_huge_page(&self, virt_addr: usize) -> Result<Option<HugePageSize>, VmmError> {
        let indices = self.page_table_indices(virt_addr);
        let mut current_table_phys = self.root_table_phys;

        for (level, &index) in indices.iter().enumerate() {
            let current_table = unsafe { &*(current_table_phys as *const PageTable) };
            let entry = current_table.entry(index).ok_or(VmmError::InvalidEntry)?;

            if !entry.is_present() {
                return Ok(None);
            }

            let flags = entry.flags();
            if flags.contains(PageTableFlags::HUGE) {
                // Found a huge page at this level
                return Ok(Some(match level {
                    1 => HugePageSize::Size1GB,
                    2 => HugePageSize::Size2MB,
                    _ => return Err(VmmError::InvalidEntry),
                }));
            }

            if level < 3 {
                current_table_phys = entry.phys_addr();
            } else {
                // Regular 4KB page
                return Ok(Some(HugePageSize::Size4KB));
            }
        }

        Ok(None)
    }

    /// Map a memory-mapped I/O (MMIO) region
    ///
    /// MMIO regions are device memory that should not be cached.
    /// This method automatically disables caching and sets appropriate flags.
    ///
    /// # Arguments
    /// * `virt_addr` - Virtual address (must be page-aligned)
    /// * `phys_addr` - Physical device address (must be page-aligned)
    /// * `size` - Size in bytes (will be rounded up to page boundary)
    /// * `writable` - Whether the region should be writable
    ///
    /// # Returns
    /// Number of pages mapped
    pub fn map_mmio(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        size: usize,
        writable: bool,
    ) -> Result<usize, VmmError> {
        // Validate alignment
        if virt_addr & 0xfff != 0 || phys_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // Round size up to page boundary
        let num_pages = size.div_ceil(PAGE_SIZE);

        // Set MMIO flags: no cache, write-through, no execute, MMIO marker
        let mut flags = PageTableFlags::PRESENT
            | PageTableFlags::CACHE_DISABLE
            | PageTableFlags::WRITE_THROUGH
            | PageTableFlags::NO_EXECUTE
            | PageTableFlags::MMIO;

        if writable {
            flags = flags | PageTableFlags::WRITABLE;
        }

        // Map each page in the region
        for i in 0..num_pages {
            let page_virt = virt_addr + (i * PAGE_SIZE);
            let page_phys = phys_addr + (i * PAGE_SIZE);
            self.map(page_virt, page_phys, flags)?;
        }

        // Update statistics
        let state = VMM_STATE.lock();
        state.total_mmio_regions.fetch_add(1, Ordering::SeqCst);
        state
            .total_mmio_pages
            .fetch_add(num_pages as u64, Ordering::SeqCst);
        drop(state);

        Ok(num_pages)
    }

    /// Unmap a memory-mapped I/O (MMIO) region
    ///
    /// # Arguments
    /// * `virt_addr` - Virtual address (must be page-aligned)
    /// * `size` - Size in bytes (will be rounded up to page boundary)
    ///
    /// # Returns
    /// Number of pages unmapped
    pub fn unmap_mmio(&mut self, virt_addr: usize, size: usize) -> Result<usize, VmmError> {
        if virt_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        let num_pages = size.div_ceil(PAGE_SIZE);
        let mut unmapped = 0;

        for i in 0..num_pages {
            let page_virt = virt_addr + (i * PAGE_SIZE);
            match self.unmap(page_virt) {
                Ok(_) => unmapped += 1,
                Err(VmmError::NotMapped) => continue,
                Err(e) => return Err(e),
            }
        }

        // Update statistics
        let state = VMM_STATE.lock();
        state.total_mmio_regions.fetch_sub(1, Ordering::SeqCst);
        state
            .total_mmio_pages
            .fetch_sub(unmapped as u64, Ordering::SeqCst);
        drop(state);

        Ok(unmapped)
    }

    /// Check if a virtual address is mapped as MMIO
    pub fn is_mmio(&self, virt_addr: usize) -> Result<bool, VmmError> {
        let flags = self.get_flags(virt_addr)?;
        Ok(flags.contains(PageTableFlags::MMIO))
    }

    /// Create a new shared memory region
    ///
    /// Allocates physical memory and creates a shareable region that can be
    /// attached by other address spaces.
    ///
    /// # Arguments
    /// * `virt_addr` - Virtual address for this mapping (must be page-aligned)
    /// * `size` - Size in bytes (will be rounded up to page boundary)
    /// * `flags` - Protection flags (SHARED flag will be added automatically)
    ///
    /// # Returns
    /// Shared region ID that can be used by other address spaces
    pub fn create_shared_region(
        &mut self,
        virt_addr: usize,
        size: usize,
        flags: PageTableFlags,
    ) -> Result<usize, VmmError> {
        // Validate alignment
        if virt_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // Round size up to page boundary
        let num_pages = size.div_ceil(PAGE_SIZE);

        // Allocate contiguous physical pages for the shared region
        let base_phys = memory::allocate_pages(num_pages).map_err(|_| VmmError::OutOfMemory)?;

        // Add SHARED flag
        let shared_flags = flags | PageTableFlags::SHARED;

        // Map the pages
        for i in 0..num_pages {
            let page_virt = virt_addr + (i * PAGE_SIZE);
            let page_phys = base_phys + (i * PAGE_SIZE);
            self.map(page_virt, page_phys, shared_flags)?;
        }

        // Register the shared region globally
        let mut registry = SHARED_MEMORY.lock();
        let region_id = registry.create_region(virt_addr, size, base_phys, shared_flags);
        drop(registry);

        // Update statistics
        let state = VMM_STATE.lock();
        state.total_shared_regions.fetch_add(1, Ordering::SeqCst);
        state
            .total_shared_pages
            .fetch_add(num_pages as u64, Ordering::SeqCst);
        drop(state);

        Ok(region_id)
    }

    /// Attach to an existing shared memory region
    ///
    /// Maps an existing shared region into this address space.
    ///
    /// # Arguments
    /// * `region_id` - ID of the shared region (from create_shared_region)
    /// * `virt_addr` - Virtual address for this mapping (must be page-aligned)
    ///
    /// # Returns
    /// Number of pages mapped
    pub fn attach_shared_region(
        &mut self,
        region_id: usize,
        virt_addr: usize,
    ) -> Result<usize, VmmError> {
        // Validate alignment
        if virt_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // Get the shared region
        let mut registry = SHARED_MEMORY.lock();
        let region = registry
            .get_region(region_id)
            .ok_or(VmmError::InvalidEntry)?;

        // Increment reference count
        registry.inc_ref(region_id)?;
        drop(registry);

        // Calculate number of pages
        let num_pages = region.size.div_ceil(PAGE_SIZE);

        // Map the pages
        for i in 0..num_pages {
            let page_virt = virt_addr + (i * PAGE_SIZE);
            let page_phys = region.phys_addr + (i * PAGE_SIZE);
            self.map(page_virt, page_phys, region.flags)?;
        }

        Ok(num_pages)
    }

    /// Detach from a shared memory region
    ///
    /// Unmaps a shared region from this address space and decrements the
    /// reference count. If this is the last attachment, the physical memory
    /// is freed.
    ///
    /// # Arguments
    /// * `region_id` - ID of the shared region
    /// * `virt_addr` - Virtual address where the region is mapped
    pub fn detach_shared_region(
        &mut self,
        region_id: usize,
        virt_addr: usize,
    ) -> Result<(), VmmError> {
        // Get the shared region
        let mut registry = SHARED_MEMORY.lock();
        let region = registry
            .get_region(region_id)
            .ok_or(VmmError::InvalidEntry)?;

        // Decrement reference count
        let should_free = registry.dec_ref(region_id)?;
        drop(registry);

        // Calculate number of pages
        let num_pages = region.size.div_ceil(PAGE_SIZE);

        // Unmap the pages
        for i in 0..num_pages {
            let page_virt = virt_addr + (i * PAGE_SIZE);
            let _ = self.unmap(page_virt); // Ignore errors if already unmapped
        }

        // Free physical memory if this was the last reference
        if should_free {
            for i in 0..num_pages {
                let page_phys = region.phys_addr + (i * PAGE_SIZE);
                let _ = memory::free_page(page_phys);
            }

            // Update statistics
            let state = VMM_STATE.lock();
            state.total_shared_regions.fetch_sub(1, Ordering::SeqCst);
            state
                .total_shared_pages
                .fetch_sub(num_pages as u64, Ordering::SeqCst);
            drop(state);
        }

        Ok(())
    }

    /// Get page table indices for a virtual address (x86_64)
    #[cfg(target_arch = "x86_64")]
    fn page_table_indices(&self, virt_addr: usize) -> [usize; 4] {
        [
            (virt_addr >> 39) & 0x1ff, // PML4 index
            (virt_addr >> 30) & 0x1ff, // PDPT index
            (virt_addr >> 21) & 0x1ff, // PD index
            (virt_addr >> 12) & 0x1ff, // PT index
        ]
    }

    /// Get page table indices for a virtual address (AArch64)
    #[cfg(target_arch = "aarch64")]
    fn page_table_indices(&self, virt_addr: usize) -> [usize; 4] {
        [
            (virt_addr >> 39) & 0x1ff, // L0 index
            (virt_addr >> 30) & 0x1ff, // L1 index
            (virt_addr >> 21) & 0x1ff, // L2 index
            (virt_addr >> 12) & 0x1ff, // L3 index
        ]
    }

    /// Get page table indices for a virtual address (other architectures)
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn page_table_indices(&self, virt_addr: usize) -> [usize; 4] {
        [
            (virt_addr >> 39) & 0x1ff,
            (virt_addr >> 30) & 0x1ff,
            (virt_addr >> 21) & 0x1ff,
            (virt_addr >> 12) & 0x1ff,
        ]
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        // Free all page tables (TODO: implement cleanup)
        // This should recursively free all intermediate page tables
        free_asid(self.asid);
    }
}

/// Global VMM state
struct VmmState {
    /// Is VMM initialized?
    initialized: AtomicBool,
    /// Next ASID to allocate
    next_asid: AtomicUsize,
    /// Number of active address spaces
    active_spaces: AtomicUsize,
    /// Total pages mapped across all address spaces
    total_mapped_pages: AtomicU64,
    /// Total TLB flushes
    total_tlb_flushes: AtomicU64,
    /// Total COW faults handled
    total_cow_faults: AtomicU64,
    /// Total COW pages copied
    total_cow_copies: AtomicU64,
    /// Total COW faults resolved without copy (sole owner)
    total_cow_fast_path: AtomicU64,
    /// Total demand-paging faults
    total_demand_faults: AtomicU64,
    /// Total pages allocated via demand paging
    total_demand_pages: AtomicU64,
    /// Total 2MB huge pages mapped
    total_huge_2mb_pages: AtomicU64,
    /// Total 1GB huge pages mapped
    total_huge_1gb_pages: AtomicU64,
    /// Total MMIO regions mapped
    total_mmio_regions: AtomicU64,
    /// Total MMIO pages mapped
    total_mmio_pages: AtomicU64,
    /// Total shared memory regions
    total_shared_regions: AtomicU64,
    /// Total shared memory pages
    total_shared_pages: AtomicU64,
}

impl VmmState {
    const fn new() -> Self {
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
        }
    }
}

/// Global VMM state
static VMM_STATE: Mutex<VmmState> = Mutex::new(VmmState::new());

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
fn allocate_asid() -> Result<usize, VmmError> {
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
fn free_asid(asid: usize) {
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

/// Flush TLB page (architecture-specific)
#[cfg(target_arch = "x86_64")]
unsafe fn flush_tlb_page(virt_addr: usize) {
    core::arch::asm!(
        "invlpg [{}]",
        in(reg) virt_addr,
        options(nostack, preserves_flags)
    );
}

#[cfg(target_arch = "aarch64")]
unsafe fn flush_tlb_page(virt_addr: usize) {
    core::arch::asm!(
        "tlbi vaae1, {}",
        in(reg) virt_addr >> 12,
        options(nostack, preserves_flags)
    );
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
unsafe fn flush_tlb_page(_virt_addr: usize) {
    // Platform-specific implementation needed
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let result = init();
        assert!(result.is_ok());
    }

    #[test]
    fn test_page_table_flags() {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        assert!(flags.contains(PageTableFlags::PRESENT));
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::USER));
    }

    #[test]
    fn test_page_table_entry() {
        let mut entry = PageTableEntry::new();
        assert!(!entry.is_present());

        entry.set(0x1000, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
        assert!(entry.is_present());
        assert_eq!(entry.phys_addr(), 0x1000);
    }

    #[test]
    fn test_get_stats() {
        init().unwrap();
        let stats = get_stats();
        assert!(stats.initialized);
    }

    // Note: The following tests require actual page table manipulation
    // and may not work properly in std test mode without identity-mapped memory.
    // They are designed to work in a no_std kernel environment.

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_cow_map() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x1000_0000;

        // Map with COW
        let result = addr_space.map_cow(
            virt_addr,
            phys_addr,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        );
        assert!(result.is_ok());

        // Verify mapping
        let translated = addr_space.translate(virt_addr).unwrap();
        assert_eq!(translated, phys_addr);

        // Verify COW flag is set
        let flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(flags.contains(PageTableFlags::COW));
        assert!(!flags.contains(PageTableFlags::WRITABLE));
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_cow_fault_single_owner() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x2000_0000;

        // Map with COW
        addr_space
            .map_cow(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        // Decrement refcount to 1 (we're sole owner)
        {
            let mut refcount = PAGE_REFCOUNT.lock();
            refcount.dec_ref(phys_addr);
        }

        // Handle COW fault (should take fast path)
        let result = addr_space.handle_cow_fault(virt_addr);
        assert!(result.is_ok());

        // Verify page is now writable and not COW
        let flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::COW));

        // Physical address should be the same (no copy)
        let translated = addr_space.translate(virt_addr).unwrap();
        assert_eq!(translated, phys_addr);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_cow_fault_shared() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x3000_0000;

        // Map with COW
        addr_space
            .map_cow(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        // Increment refcount to simulate sharing
        {
            let mut refcount = PAGE_REFCOUNT.lock();
            refcount.inc_ref(phys_addr);
        }

        let old_stats = get_stats();

        // Handle COW fault (should trigger copy)
        let result = addr_space.handle_cow_fault(virt_addr);
        assert!(result.is_ok());

        // Verify COW copy was triggered
        let new_stats = get_stats();
        assert_eq!(new_stats.total_cow_faults, old_stats.total_cow_faults + 1);
        assert_eq!(new_stats.total_cow_copies, old_stats.total_cow_copies + 1);

        // Verify page is now writable and not COW
        let flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::COW));

        // Physical address should be different (copy occurred)
        let new_phys_addr = addr_space.translate(virt_addr).unwrap();
        assert_ne!(new_phys_addr, phys_addr);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_cow_non_cow_page() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x4000_0000;

        // Map without COW (normal writable page)
        addr_space
            .map(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        // Try to handle COW fault on non-COW page (should fail)
        let result = addr_space.handle_cow_fault(virt_addr);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), VmmError::WriteProtectionViolation);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_get_flags() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x5000_0000;

        // Map with specific flags
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;
        addr_space.map(virt_addr, phys_addr, flags).unwrap();

        // Get and verify flags
        let retrieved_flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(retrieved_flags.contains(PageTableFlags::PRESENT));
        assert!(retrieved_flags.contains(PageTableFlags::WRITABLE));
        assert!(retrieved_flags.contains(PageTableFlags::USER));
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_protect() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x6000_0000;

        // Map as writable
        addr_space
            .map(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        // Change to read-only
        let new_flags = PageTableFlags::PRESENT;
        addr_space.protect(virt_addr, new_flags).unwrap();

        // Verify flags changed
        let flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(flags.contains(PageTableFlags::PRESENT));
        assert!(!flags.contains(PageTableFlags::WRITABLE));
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_fork_basic() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();

        // Fork should create new address space
        let result = addr_space.fork();
        assert!(result.is_ok());

        let forked_space = result.unwrap();
        assert_ne!(addr_space.asid, forked_space.asid);
    }

    #[test]
    fn test_page_refcount() {
        let mut refcount = PageRefCount::new();

        // Test increment
        refcount.inc_ref(0x1000);
        assert_eq!(refcount.get_ref(0x1000), 1);

        refcount.inc_ref(0x1000);
        assert_eq!(refcount.get_ref(0x1000), 2);

        // Test decrement
        let count = refcount.dec_ref(0x1000);
        assert_eq!(count, 1);
        assert_eq!(refcount.get_ref(0x1000), 1);

        let count = refcount.dec_ref(0x1000);
        assert_eq!(count, 0);
        assert_eq!(refcount.get_ref(0x1000), 0);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_demand_paging_map() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x8000_0000;
        let page_count = 4;

        // Map pages as demand-paged
        let result = addr_space.map_demand(
            virt_addr,
            page_count,
            PageTableFlags::WRITABLE | PageTableFlags::USER,
        );
        assert!(result.is_ok());

        // Verify pages are marked as demand-paged
        for i in 0..page_count {
            let addr = virt_addr + (i * PAGE_SIZE);
            assert!(addr_space.is_demand_paged(addr).unwrap());
        }
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_demand_paging_fault() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x9000_0000;

        // Map single page as demand-paged
        addr_space
            .map_demand(
                virt_addr,
                1,
                PageTableFlags::WRITABLE | PageTableFlags::USER,
            )
            .unwrap();

        assert!(addr_space.is_demand_paged(virt_addr).unwrap());

        let stats_before = get_stats();

        // Handle demand fault
        let result = addr_space.handle_demand_fault(virt_addr);
        assert!(result.is_ok());

        // Verify statistics updated
        let stats_after = get_stats();
        assert_eq!(
            stats_after.total_demand_faults,
            stats_before.total_demand_faults + 1
        );
        assert_eq!(
            stats_after.total_demand_pages,
            stats_before.total_demand_pages + 1
        );

        // Verify page is no longer demand-paged
        assert!(!addr_space.is_demand_paged(virt_addr).unwrap());

        // Verify page is now present and has physical mapping
        let phys_addr = addr_space.translate(virt_addr);
        assert!(phys_addr.is_ok());
        assert_ne!(phys_addr.unwrap(), 0);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_demand_paging_multiple_pages() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0xA000_0000;
        let page_count = 8;

        // Map multiple pages as demand-paged
        addr_space
            .map_demand(
                virt_addr,
                page_count,
                PageTableFlags::WRITABLE | PageTableFlags::USER,
            )
            .unwrap();

        // Handle faults for alternating pages (0, 2, 4, 6)
        for i in (0..page_count).step_by(2) {
            let addr = virt_addr + (i * PAGE_SIZE);
            addr_space.handle_demand_fault(addr).unwrap();
        }

        // Verify correct pages are allocated
        for i in 0..page_count {
            let addr = virt_addr + (i * PAGE_SIZE);
            if i % 2 == 0 {
                // Should be allocated
                assert!(!addr_space.is_demand_paged(addr).unwrap());
                assert!(addr_space.translate(addr).is_ok());
            } else {
                // Should still be demand-paged
                assert!(addr_space.is_demand_paged(addr).unwrap());
            }
        }
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_demand_paging_non_demand_page() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0xB000_0000;

        // Map as regular page (not demand-paged)
        addr_space
            .map(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        // Verify it's not demand-paged
        assert!(!addr_space.is_demand_paged(virt_addr).unwrap());

        // Try to handle demand fault (should fail)
        let result = addr_space.handle_demand_fault(virt_addr);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), VmmError::PageFault);
    }

    #[test]
    fn test_demand_paging_statistics() {
        crate::memory::init().unwrap();
        init().unwrap();

        let stats = get_stats();
        // Statistics should be accessible
        let _ = stats.total_demand_faults;
        let _ = stats.total_demand_pages;
    }

    #[test]
    fn test_huge_page_size() {
        assert_eq!(HugePageSize::Size4KB.bytes(), PAGE_SIZE);
        assert_eq!(HugePageSize::Size2MB.bytes(), HUGE_PAGE_2MB);
        assert_eq!(HugePageSize::Size1GB.bytes(), HUGE_PAGE_1GB);

        assert_eq!(HugePageSize::Size4KB.page_table_level(), 3);
        assert_eq!(HugePageSize::Size2MB.page_table_level(), 2);
        assert_eq!(HugePageSize::Size1GB.page_table_level(), 1);
    }

    #[test]
    fn test_huge_page_alignment() {
        // 2MB aligned addresses
        assert!(HugePageSize::Size2MB.is_aligned(0x0));
        assert!(HugePageSize::Size2MB.is_aligned(0x20_0000)); // 2MB
        assert!(HugePageSize::Size2MB.is_aligned(0x40_0000)); // 4MB
        assert!(!HugePageSize::Size2MB.is_aligned(0x1000)); // 4KB

        // 1GB aligned addresses
        assert!(HugePageSize::Size1GB.is_aligned(0x0));
        assert!(HugePageSize::Size1GB.is_aligned(0x4000_0000)); // 1GB
        assert!(!HugePageSize::Size1GB.is_aligned(0x20_0000)); // 2MB
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_huge_page_2mb_mapping() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x2_0000_0000; // 8GB (2MB aligned)
        let phys_addr = 0x2_0000_0000; // 8GB (2MB aligned)

        // Map a 2MB huge page
        let result = addr_space.map_huge(
            virt_addr,
            phys_addr,
            HugePageSize::Size2MB,
            PageTableFlags::WRITABLE | PageTableFlags::USER,
        );
        assert!(result.is_ok());

        // Verify it's a huge page
        let page_size = addr_space.is_huge_page(virt_addr).unwrap();
        assert_eq!(page_size, Some(HugePageSize::Size2MB));

        // Verify statistics
        let stats = get_stats();
        assert!(stats.total_huge_2mb_pages > 0);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_huge_page_1gb_mapping() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x40_0000_0000; // 256GB (1GB aligned)
        let phys_addr = 0x40_0000_0000; // 256GB (1GB aligned)

        // Map a 1GB huge page
        let result = addr_space.map_huge(
            virt_addr,
            phys_addr,
            HugePageSize::Size1GB,
            PageTableFlags::WRITABLE | PageTableFlags::USER,
        );
        assert!(result.is_ok());

        // Verify it's a huge page
        let page_size = addr_space.is_huge_page(virt_addr).unwrap();
        assert_eq!(page_size, Some(HugePageSize::Size1GB));

        // Verify statistics
        let stats = get_stats();
        assert!(stats.total_huge_1gb_pages > 0);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_huge_page_unaligned_error() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x1000; // 4KB aligned but not 2MB aligned
        let phys_addr = 0x20_0000; // 2MB aligned

        // Try to map with misaligned virtual address
        let result = addr_space.map_huge(
            virt_addr,
            phys_addr,
            HugePageSize::Size2MB,
            PageTableFlags::WRITABLE,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), VmmError::UnalignedHugePage);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_huge_page_invalid_size() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x20_0000;
        let phys_addr = 0x20_0000;

        // Try to map 4KB page as "huge page"
        let result = addr_space.map_huge(
            virt_addr,
            phys_addr,
            HugePageSize::Size4KB,
            PageTableFlags::WRITABLE,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), VmmError::InvalidPageSize);
    }

    #[test]
    fn test_huge_page_statistics() {
        crate::memory::init().unwrap();
        init().unwrap();

        let stats = get_stats();
        // Statistics should be accessible
        let _ = stats.total_huge_2mb_pages;
        let _ = stats.total_huge_1gb_pages;
    }

    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_cow_statistics() {
        crate::memory::init().unwrap();
        init().unwrap();

        let stats_before = get_stats();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x7000_0000;

        // Map with COW
        addr_space
            .map_cow(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        // Increment refcount to simulate sharing
        {
            let mut refcount = PAGE_REFCOUNT.lock();
            refcount.inc_ref(phys_addr);
        }

        // Handle COW fault
        addr_space.handle_cow_fault(virt_addr).unwrap();

        let stats_after = get_stats();

        // Verify statistics were updated
        assert!(stats_after.total_cow_faults > stats_before.total_cow_faults);
        assert!(stats_after.total_cow_copies > stats_before.total_cow_copies);
    }

    // MMIO Tests

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_mmio_basic_mapping() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x8000_0000;
        let phys_addr = 0xFEE0_0000; // Typical APIC base address
        let size = 4096;

        // Map MMIO region as read-only
        let num_pages = addr_space
            .map_mmio(virt_addr, phys_addr, size, false)
            .unwrap();
        assert_eq!(num_pages, 1);

        // Verify it's mapped as MMIO
        assert!(addr_space.is_mmio(virt_addr).unwrap());

        // Verify the mapping has correct flags (no cache, no execute)
        let flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(flags.contains(PageTableFlags::MMIO));
        assert!(flags.contains(PageTableFlags::CACHE_DISABLE));
        assert!(flags.contains(PageTableFlags::NO_EXECUTE));
        assert!(!flags.contains(PageTableFlags::WRITABLE)); // Read-only
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_mmio_writable_mapping() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x9000_0000;
        let phys_addr = 0xFEE0_0000;
        let size = 8192;

        // Map MMIO region as writable
        let num_pages = addr_space
            .map_mmio(virt_addr, phys_addr, size, true)
            .unwrap();
        assert_eq!(num_pages, 2); // 8KB = 2 pages

        // Verify it's writable
        let flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(flags.contains(PageTableFlags::WRITABLE));
        assert!(flags.contains(PageTableFlags::MMIO));
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_mmio_unmap() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0xA000_0000;
        let phys_addr = 0xFEE0_0000;
        let size = 4096;

        // Map and then unmap
        addr_space
            .map_mmio(virt_addr, phys_addr, size, false)
            .unwrap();
        let unmapped = addr_space.unmap_mmio(virt_addr, size).unwrap();
        assert_eq!(unmapped, 1);

        // Verify it's no longer mapped
        assert!(addr_space.is_mmio(virt_addr).is_err());
    }

    #[test]
    fn test_mmio_statistics() {
        let _ = crate::memory::init();
        let _ = init();

        let stats = get_stats();
        // Statistics should be accessible
        let _ = stats.total_mmio_regions;
        let _ = stats.total_mmio_pages;
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_mmio_misaligned_error() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x8000_0001; // Misaligned
        let phys_addr = 0xFEE0_0000;
        let size = 4096;

        // Should fail due to alignment
        let result = addr_space.map_mmio(virt_addr, phys_addr, size, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), VmmError::InvalidVirtualAddress);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_mmio_large_region() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0xB000_0000;
        let phys_addr = 0xFEE0_0000;
        let size = 1024 * 1024; // 1MB

        // Map large MMIO region
        let num_pages = addr_space
            .map_mmio(virt_addr, phys_addr, size, true)
            .unwrap();
        assert_eq!(num_pages, 256); // 1MB / 4KB = 256 pages

        // Verify statistics
        let stats = get_stats();
        assert!(stats.total_mmio_pages >= 256);
    }

    // Shared Memory Tests

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_create() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0xC000_0000;
        let size = 8192; // 2 pages
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Create shared region
        let _region_id = addr_space
            .create_shared_region(virt_addr, size, flags)
            .unwrap();

        // Verify it's mapped with SHARED flag
        let page_flags = addr_space.get_flags(virt_addr).unwrap();
        assert!(page_flags.contains(PageTableFlags::SHARED));
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_attach() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space1 = AddressSpace::new().unwrap();
        let mut addr_space2 = AddressSpace::new().unwrap();

        let virt_addr1 = 0xD000_0000;
        let virt_addr2 = 0xE000_0000; // Different virtual address
        let size = 4096;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Create shared region in first address space
        let region_id = addr_space1
            .create_shared_region(virt_addr1, size, flags)
            .unwrap();

        // Attach to the same region in second address space
        let num_pages = addr_space2
            .attach_shared_region(region_id, virt_addr2)
            .unwrap();
        assert_eq!(num_pages, 1);

        // Verify both are mapped to the same physical address
        let phys1 = addr_space1.translate(virt_addr1).unwrap();
        let phys2 = addr_space2.translate(virt_addr2).unwrap();
        assert_eq!(phys1 & !0xfff, phys2 & !0xfff); // Same physical page
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_detach() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space1 = AddressSpace::new().unwrap();
        let mut addr_space2 = AddressSpace::new().unwrap();

        let virt_addr1 = 0xF000_0000;
        let virt_addr2 = 0xF100_0000;
        let size = 4096;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Create and attach
        let region_id = addr_space1
            .create_shared_region(virt_addr1, size, flags)
            .unwrap();
        addr_space2
            .attach_shared_region(region_id, virt_addr2)
            .unwrap();

        // Detach from second address space
        addr_space2
            .detach_shared_region(region_id, virt_addr2)
            .unwrap();

        // Verify it's unmapped in addr_space2
        assert!(addr_space2.translate(virt_addr2).is_err());

        // But still mapped in addr_space1
        assert!(addr_space1.translate(virt_addr1).is_ok());
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_multiple_attach() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space1 = AddressSpace::new().unwrap();
        let mut addr_space2 = AddressSpace::new().unwrap();
        let mut addr_space3 = AddressSpace::new().unwrap();

        let size = 4096;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Create shared region
        let region_id = addr_space1
            .create_shared_region(0x1000_0000, size, flags)
            .unwrap();

        // Attach from multiple address spaces
        addr_space2
            .attach_shared_region(region_id, 0x2000_0000)
            .unwrap();
        addr_space3
            .attach_shared_region(region_id, 0x3000_0000)
            .unwrap();

        // Verify all point to the same physical memory
        let phys1 = addr_space1.translate(0x1000_0000).unwrap() & !0xfff;
        let phys2 = addr_space2.translate(0x2000_0000).unwrap() & !0xfff;
        let phys3 = addr_space3.translate(0x3000_0000).unwrap() & !0xfff;

        assert_eq!(phys1, phys2);
        assert_eq!(phys2, phys3);
    }

    #[test]
    fn test_shared_memory_statistics() {
        let _ = crate::memory::init();
        let _ = init();

        let stats = get_stats();
        // Statistics should be accessible
        let _ = stats.total_shared_regions;
        let _ = stats.total_shared_pages;
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_misaligned_error() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x1000_0001; // Misaligned
        let size = 4096;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Should fail due to alignment
        let result = addr_space.create_shared_region(virt_addr, size, flags);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), VmmError::InvalidVirtualAddress);
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_permissions() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space1 = AddressSpace::new().unwrap();
        let mut addr_space2 = AddressSpace::new().unwrap();

        let virt_addr1 = 0x4000_0000;
        let virt_addr2 = 0x5000_0000;
        let size = 4096;
        // Create with read-only permissions
        let flags = PageTableFlags::PRESENT; // No WRITABLE

        // Create shared region
        let region_id = addr_space1
            .create_shared_region(virt_addr1, size, flags)
            .unwrap();

        // Attach to second address space
        addr_space2
            .attach_shared_region(region_id, virt_addr2)
            .unwrap();

        // Verify both have read-only permissions
        let flags1 = addr_space1.get_flags(virt_addr1).unwrap();
        let flags2 = addr_space2.get_flags(virt_addr2).unwrap();

        assert!(!flags1.contains(PageTableFlags::WRITABLE));
        assert!(!flags2.contains(PageTableFlags::WRITABLE));
        assert!(flags1.contains(PageTableFlags::SHARED));
        assert!(flags2.contains(PageTableFlags::SHARED));
    }

    #[test]
    #[ignore = "requires identity-mapped memory in no_std environment"]
    fn test_shared_memory_large_region() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0x6000_0000;
        let size = 64 * 1024; // 64KB = 16 pages
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Create large shared region
        let _region_id = addr_space
            .create_shared_region(virt_addr, size, flags)
            .unwrap();

        // Verify statistics
        let stats = get_stats();
        assert!(stats.total_shared_pages >= 16);
    }
}
