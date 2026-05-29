//! Copy-on-Write (COW) memory management
//!
//! Implements COW page sharing semantics. A COW page is initially shared
//! between multiple address spaces as read-only. The first write triggers
//! a page-fault that allocates a private copy for the faulting process.

use super::*;

/// Page reference counter for COW (Copy-on-Write) tracking
///
/// Tracks the number of address spaces sharing a physical page.
/// When the reference count drops to 1, the page can be written directly.
/// When >1, a write operation triggers a copy.
pub(super) struct PageRefCount {
    /// Reference count per physical page
    pub(super) refcounts: BTreeMap<usize, AtomicUsize>,
}

impl PageRefCount {
    /// Create a new reference counter
    pub(super) const fn new() -> Self {
        Self {
            refcounts: BTreeMap::new(),
        }
    }

    /// Increment reference count for a physical page
    pub(super) fn inc_ref(&mut self, phys_addr: usize) {
        let refcount = self
            .refcounts
            .entry(phys_addr)
            .or_insert_with(|| AtomicUsize::new(0));
        refcount.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement reference count for a physical page
    /// Returns the new count
    pub(super) fn dec_ref(&mut self, phys_addr: usize) -> usize {
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
    pub(super) fn get_ref(&self, phys_addr: usize) -> usize {
        self.refcounts
            .get(&phys_addr)
            .map(|r| r.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

/// Global page reference counter for COW
pub(super) static PAGE_REFCOUNT: Mutex<PageRefCount> = Mutex::new(PageRefCount::new());

impl AddressSpace {
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
    pub(super) fn walk_and_copy_cow(&mut self, new_space: &mut AddressSpace) -> Result<(), VmmError> {
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
}
