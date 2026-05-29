//! Memory-Mapped I/O (MMIO) region management
//!
//! MMIO pages map device registers directly into the virtual address space.
//! They must not be cached and must not be executable.  The `map_mmio`
//! helper automatically sets `CACHE_DISABLE`, `WRITE_THROUGH`, `NO_EXECUTE`,
//! and the `MMIO` marker flag; callers only need to specify whether the
//! region should be writable.

use super::*;

impl AddressSpace {
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
}
