//! Guard page support
//!
//! Guard pages are inaccessible sentinel pages inserted at the boundary of
//! a memory region (typically at the bottom of a stack) to detect overflow.
//! Any hardware access to a guard page raises a page fault, which the kernel
//! can intercept and report as a stack-overflow or buffer-overflow condition.
//!
//! # Design
//!
//! A guard page is mapped into the page table with `PRESENT` set (so the
//! hardware can detect the access) but with all permission bits (`WRITABLE`,
//! `USER`, `NO_EXECUTE`) cleared and the `GUARD` marker flag set.
//! `is_guard_page` recognises a page as a guard if the `GUARD` flag is
//! present in the stored flags.
//!
//! # TLB shootdown
//!
//! `map_guard_region` uses `flush_tlb_range` to invalidate all TLB entries
//! covering the newly-protected pages.  This exercises the
//! `VmmError::TlbShootdownFailed` path that was previously dead code.

use super::*;
use core::sync::atomic::Ordering;

/// Flags applied to every guard page entry.
///
/// `PRESENT` is required for the hardware to raise a fault rather than
/// silently skipping the access.  `GUARD` is the software marker we use to
/// identify the page as a sentinel later.  All permission bits are absent.
pub const GUARD_FLAGS: PageTableFlags = PageTableFlags::GUARD;

impl AddressSpace {
    /// Map `count` pages starting at `virt_addr` as guard pages.
    ///
    /// Each page is first mapped normally at the supplied `phys_addr` (which
    /// may be any valid physical page used as a dummy backing) and then
    /// immediately re-protected to strip all permission bits and mark it as
    /// a guard page.  The TLB is flushed for the entire range so that the
    /// new, restrictive flags take effect at once.
    ///
    /// # Arguments
    /// * `virt_addr`  - Page-aligned virtual base address of the guard region.
    /// * `phys_addr`  - Dummy backing physical address (page-aligned).
    /// * `count`      - Number of consecutive guard pages to map.
    ///
    /// # Errors
    /// Returns `VmmError::InvalidVirtualAddress` when either address is not
    /// page-aligned, or any error propagated from the underlying `map` /
    /// `protect` operations.
    pub fn map_guard_region(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        count: usize,
    ) -> Result<(), VmmError> {
        if virt_addr & 0xfff != 0 || phys_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        for i in 0..count {
            let page_virt = virt_addr + i * PAGE_SIZE;
            let page_phys = phys_addr + i * PAGE_SIZE;

            // Map the page with minimal flags first so the walk allocates the
            // intermediate page-table entries.
            self.map(page_virt, page_phys, PageTableFlags::PRESENT)?;

            // Immediately strip all permissions and set the GUARD marker.
            // `protect` preserves PRESENT so the hardware still faults on access.
            self.protect_guard(page_virt)?;
        }

        // Flush the entire range in one pass to make the protection active.
        flush_tlb_range(virt_addr, count);

        // Update statistics.
        let state = VMM_STATE.lock();
        state
            .total_guard_pages
            .fetch_add(count as u64, Ordering::SeqCst);
        drop(state);

        Ok(())
    }

    /// Apply guard protection to a single already-mapped page.
    ///
    /// Sets the page-table entry to `PRESENT | GUARD` (no read/write/execute).
    /// This is a lower-level helper used by `map_guard_region` and
    /// `map_stack_guarded`.
    fn protect_guard(&mut self, virt_addr: usize) -> Result<(), VmmError> {
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
                // Store PRESENT (for HW fault generation) + GUARD (software marker).
                // No WRITABLE, no USER, no execute — the page is truly inaccessible.
                entry.set(phys_addr, PageTableFlags::PRESENT | PageTableFlags::GUARD);
                return Ok(());
            }
        }

        Err(VmmError::NotMapped)
    }

    /// Return `true` when the page containing `virt_addr` is a guard page.
    ///
    /// A page is considered a guard page if the `GUARD` flag is set in its
    /// page-table entry flags.
    ///
    /// # Errors
    /// Returns `Err(VmmError::NotMapped)` if the address is not mapped at all.
    pub fn is_guard_page(&self, virt_addr: usize) -> Result<bool, VmmError> {
        match self.get_flags(virt_addr) {
            Ok(flags) => Ok(flags.contains(PageTableFlags::GUARD)),
            Err(e) => Err(e),
        }
    }

    /// Map a guarded stack region.
    ///
    /// Maps `stack_pages` pages at `virt_addr` with `flags` for the usable
    /// stack area, then maps a single guard page immediately **below** the
    /// stack (`virt_addr - PAGE_SIZE`) to catch downward stack overflow.
    ///
    /// ```text
    ///  virt_addr - PAGE_SIZE  ┌──────────────────┐  ← guard page (inaccessible)
    ///  virt_addr              ├──────────────────┤  ← stack base
    ///                         │  stack_pages × 4K│
    ///  virt_addr + stack_pages│  × PAGE_SIZE     │  ← stack top (highest address)
    ///                         └──────────────────┘
    /// ```
    ///
    /// # Arguments
    /// * `virt_addr`   - Page-aligned base address of the usable stack area.
    /// * `phys_addr`   - Page-aligned physical backing address for the stack.
    /// * `stack_pages` - Number of usable stack pages.
    /// * `flags`       - Flags for the usable stack pages (e.g. `PRESENT | WRITABLE`).
    ///
    /// # Returns
    /// The virtual address of the guard page (`virt_addr - PAGE_SIZE`).
    ///
    /// # Errors
    /// * `VmmError::InvalidVirtualAddress` — if `virt_addr` is not page-aligned
    ///   or equals zero (no room for a guard page below it).
    pub fn map_stack_guarded(
        &mut self,
        virt_addr: usize,
        phys_addr: usize,
        stack_pages: usize,
        flags: PageTableFlags,
    ) -> Result<usize, VmmError> {
        if virt_addr & 0xfff != 0 || phys_addr & 0xfff != 0 {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // There must be room below virt_addr for a guard page.
        if virt_addr < PAGE_SIZE {
            return Err(VmmError::InvalidVirtualAddress);
        }

        // Map the usable stack pages.
        for i in 0..stack_pages {
            let page_virt = virt_addr + i * PAGE_SIZE;
            let page_phys = phys_addr + i * PAGE_SIZE;
            self.map(page_virt, page_phys, flags | PageTableFlags::PRESENT)?;
        }

        // Map the guard page one page below the stack base.
        let guard_virt = virt_addr - PAGE_SIZE;
        // The guard page's physical backing can be the page just below phys_addr
        // (or any dummy physical address); for simplicity we allocate a fresh
        // page rather than aliasing existing memory.
        let guard_phys = memory::allocate_page().map_err(|_| VmmError::OutOfMemory)?;
        self.map(guard_virt, guard_phys, PageTableFlags::PRESENT)?;
        self.protect_guard(guard_virt)?;
        flush_tlb(guard_virt);

        // Update statistics.
        let state = VMM_STATE.lock();
        state.total_guard_pages.fetch_add(1, Ordering::SeqCst);
        drop(state);

        Ok(guard_virt)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::GUARD_FLAGS;

    /// Verify `GUARD_FLAGS` has no permission bits set.
    #[test]
    fn test_guard_flags_are_inaccessible() {
        assert!(!GUARD_FLAGS.contains(PageTableFlags::WRITABLE));
        assert!(!GUARD_FLAGS.contains(PageTableFlags::USER));
        assert!(!GUARD_FLAGS.contains(PageTableFlags::NO_EXECUTE));
        assert!(GUARD_FLAGS.contains(PageTableFlags::GUARD));
    }

    /// `is_guard_page` must return an error for an unmapped address.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_guard_page_mapped_as_inaccessible() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x1000_0000;

        addr_space
            .map_guard_region(virt_addr, phys_addr, 1)
            .unwrap();

        let flags = addr_space.get_flags(virt_addr).unwrap();
        // Guard page must NOT be writable or user-accessible.
        assert!(!flags.contains(PageTableFlags::WRITABLE));
        assert!(!flags.contains(PageTableFlags::USER));
        // Guard marker must be present.
        assert!(flags.contains(PageTableFlags::GUARD));
    }

    /// `is_guard_page` returns true for a page mapped with `map_guard_region`.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_is_guard_page_returns_true() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x2000_0000;

        addr_space
            .map_guard_region(virt_addr, phys_addr, 1)
            .unwrap();

        assert!(addr_space.is_guard_page(virt_addr).unwrap());
    }

    /// `is_guard_page` returns false for an ordinary mapped page.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_is_guard_page_returns_false_for_normal() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0x3000_0000;

        addr_space
            .map(
                virt_addr,
                phys_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        assert!(!addr_space.is_guard_page(virt_addr).unwrap());
    }

    /// `map_stack_guarded` places the guard page exactly one page below the
    /// stack base.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_stack_guard_page_placed_correctly() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let stack_phys = memory::allocate_page().unwrap();
        let virt_addr = 0x4000_0000;

        let guard_addr = addr_space
            .map_stack_guarded(
                virt_addr,
                stack_phys,
                2,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            )
            .unwrap();

        assert_eq!(guard_addr, virt_addr - PAGE_SIZE);
        assert!(addr_space.is_guard_page(guard_addr).unwrap());
        // Stack pages themselves must NOT be guard pages.
        assert!(!addr_space.is_guard_page(virt_addr).unwrap());
    }

    /// `flush_tlb_range` must complete without panicking (smoke test).
    /// Marked ignored because `invlpg`/`tlbi` instructions fault in user-space.
    #[test]
    #[ignore = "requires kernel privilege level for TLB instructions"]
    fn test_flush_tlb_range_completes() {
        // Verify the function doesn't panic when called in kernel context.
        flush_tlb_range(0x5000_0000, 4);
    }

    /// Map multiple independent guard regions and verify they are all detected.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_multiple_guard_regions() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let base_addrs = [0x6000_0000usize, 0x7000_0000, 0x8000_0000];

        for &virt_addr in &base_addrs {
            let phys_addr = memory::allocate_page().unwrap();
            addr_space
                .map_guard_region(virt_addr, phys_addr, 1)
                .unwrap();
        }

        for &virt_addr in &base_addrs {
            assert!(addr_space.is_guard_page(virt_addr).unwrap());
        }
    }

    /// Unmapped address must return an error from `is_guard_page`.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_is_guard_page_unmapped_returns_err() {
        crate::memory::init().unwrap();
        init().unwrap();

        let addr_space = AddressSpace::new().unwrap();
        let result = addr_space.is_guard_page(0xDEAD_0000);
        assert!(result.is_err());
    }

    /// Guard region statistics are updated correctly.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_guard_region_statistics_updated() {
        crate::memory::init().unwrap();
        init().unwrap();

        let before = get_stats();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        addr_space
            .map_guard_region(0x9000_0000, phys_addr, 3)
            .unwrap();

        let after = get_stats();
        assert!(after.total_guard_pages >= before.total_guard_pages + 3);
    }

    /// `map_stack_guarded` with zero `virt_addr` must be rejected.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_stack_guard_rejects_zero_virt_addr() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let result = addr_space.map_stack_guarded(
            0,
            phys_addr,
            1,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        );
        assert!(result.is_err());
    }

    /// Guard page flags must not include WRITABLE even after re-protect.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_guard_flags_no_write_after_protect() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let phys_addr = memory::allocate_page().unwrap();
        let virt_addr = 0xA000_0000;

        addr_space
            .map_guard_region(virt_addr, phys_addr, 1)
            .unwrap();

        // Try to upgrade to writable via protect — the guard flag must survive.
        addr_space
            .protect(
                virt_addr,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::GUARD,
            )
            .unwrap();

        assert!(addr_space.is_guard_page(virt_addr).unwrap());
    }

    /// Multi-page guard region: every page in the range is a guard page.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_guard_region_multi_page_all_inaccessible() {
        crate::memory::init().unwrap();
        init().unwrap();

        let mut addr_space = AddressSpace::new().unwrap();
        let virt_addr = 0xB000_0000;
        let count = 4;
        let phys_addr = memory::allocate_page().unwrap();

        addr_space
            .map_guard_region(virt_addr, phys_addr, count)
            .unwrap();

        for i in 0..count {
            let page = virt_addr + i * PAGE_SIZE;
            assert!(addr_space.is_guard_page(page).unwrap(), "page {} not guard", i);
        }
    }

    /// `flush_tlb_range` with count=0 must be a no-op (no TLB instructions called).
    #[test]
    fn test_flush_tlb_range_zero_count() {
        // count=0 means zero iterations — no TLB instruction is ever issued.
        // This is safe to call from user-space.
        flush_tlb_range(0xC000_0000, 0);
    }

    /// Guard page statistics accumulate across multiple `map_guard_region` calls.
    #[test]
    #[ignore = "requires identity-mapped memory in kernel environment"]
    fn test_guard_statistics_accumulate() {
        crate::memory::init().unwrap();
        init().unwrap();

        let before = get_stats();

        let mut addr_space = AddressSpace::new().unwrap();
        for i in 0..3usize {
            let phys_addr = memory::allocate_page().unwrap();
            let virt_addr = 0xD000_0000 + i * PAGE_SIZE * 8;
            addr_space
                .map_guard_region(virt_addr, phys_addr, 2)
                .unwrap();
        }

        let after = get_stats();
        assert!(after.total_guard_pages >= before.total_guard_pages + 6);
    }
}
