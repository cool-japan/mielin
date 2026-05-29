//! VMM unit tests (moved from vmm.rs to keep the module under 2000 lines).
//!
//! All 39 original tests are reproduced verbatim.  Tests that require an
//! identity-mapped kernel environment are annotated with
//! `#[ignore = "requires identity-mapped memory in kernel environment"]` and
//! only run when executed inside the actual kernel test harness.

use super::*;
use cow::PAGE_REFCOUNT;

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
    let mut refcount = cow::PageRefCount::new();

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
