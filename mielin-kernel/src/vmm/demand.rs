//! Demand paging and huge page support
//!
//! Demand paging defers physical page allocation until the first access.
//! Pages are recorded in the page table with the `DEMAND` flag but without
//! `PRESENT`; the first access triggers a fault which calls
//! `handle_demand_fault` to allocate and zero-fill the backing page.
//!
//! Huge pages (2 MB and 1 GB) improve TLB efficiency for large contiguous
//! regions by reducing the number of page-table entries required.

use super::*;

impl AddressSpace {
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
}
