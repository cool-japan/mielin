//! Shared memory region management
//!
//! Shared memory allows multiple address spaces to access the same physical
//! pages.  A creator calls `create_shared_region` to allocate the backing
//! physical memory and register it under a numeric `region_id`.  Other
//! address spaces can then call `attach_shared_region` with that ID to map
//! the same pages at an arbitrary virtual address.  `detach_shared_region`
//! unmaps the region and, once the last reference is dropped, frees the
//! physical pages.

use super::*;

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
    pub(super) fn new(
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
pub(super) struct SharedMemoryRegistry {
    /// Map of region ID -> SharedMemoryRegion
    pub(super) regions: BTreeMap<usize, SharedMemoryRegion>,
    /// Next region ID to allocate
    pub(super) next_id: usize,
}

impl SharedMemoryRegistry {
    pub(super) const fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Create a new shared region
    pub(super) fn create_region(
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
    pub(super) fn get_region(&self, id: usize) -> Option<SharedMemoryRegion> {
        self.regions.get(&id).cloned()
    }

    /// Increment reference count for a region
    pub(super) fn inc_ref(&mut self, id: usize) -> Result<(), VmmError> {
        if let Some(region) = self.regions.get_mut(&id) {
            region.refcount = region.refcount.saturating_add(1);
            Ok(())
        } else {
            Err(VmmError::InvalidEntry)
        }
    }

    /// Decrement reference count for a region
    /// Returns true if the region should be deleted
    pub(super) fn dec_ref(&mut self, id: usize) -> Result<bool, VmmError> {
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
pub(super) static SHARED_MEMORY: Mutex<SharedMemoryRegistry> =
    Mutex::new(SharedMemoryRegistry::new());

impl AddressSpace {
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
}
