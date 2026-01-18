//! Memory Allocator Example
//!
//! Demonstrates the kernel's page-based memory allocator with different
//! allocation strategies and statistics tracking.

use mielin_kernel::memory::{AllocationStrategy, MemoryManager, PAGE_SIZE};

fn main() {
    println!("=== MielinOS Memory Allocator Example ===\n");

    // Create a new memory manager with default (FirstFit) strategy
    let mut mm = MemoryManager::default();

    println!("Initial state:");
    println!("  Free pages: {}", mm.free_page_count());
    println!("  Allocated pages: {}", mm.allocated_pages());
    println!();

    // --- Single Page Allocation ---
    println!("1. Single Page Allocation:");
    let page1 = mm.allocate_page().expect("Failed to allocate page");
    println!("   Allocated page at address: 0x{:08X}", page1);
    println!("   Page size: {} bytes", PAGE_SIZE);
    println!("   Alignment: {} bytes", PAGE_SIZE);
    println!();

    // --- Multi-Page Allocation ---
    println!("2. Multi-Page Allocation:");
    let multi_pages = mm.allocate_pages(10).expect("Failed to allocate 10 pages");
    println!("   Allocated 10 pages at address: 0x{:08X}", multi_pages);
    println!(
        "   Total size: {} bytes ({} KB)",
        10 * PAGE_SIZE,
        10 * PAGE_SIZE / 1024
    );
    println!();

    // --- Allocation Statistics ---
    println!("3. Allocation Statistics:");
    let stats = mm.stats().snapshot();
    println!("   Total allocations: {}", stats.allocations);
    println!("   Pages allocated: {}", stats.pages_allocated);
    println!("   Free pages: {}", mm.free_page_count());
    println!("   Peak usage: {} pages", stats.peak_pages);
    println!();

    // --- Free Memory ---
    println!("4. Freeing Memory:");
    mm.free_page(page1).expect("Failed to free page");
    println!("   Freed single page at 0x{:08X}", page1);
    mm.free_pages(multi_pages, 10)
        .expect("Failed to free multi-pages");
    println!("   Freed 10 pages at 0x{:08X}", multi_pages);
    println!();

    // --- Verify Memory Returned ---
    println!("5. Memory Summary:");
    let summary = mm.summary();
    println!("   Free pages: {}", summary.free_pages);
    println!("   Allocated pages: {}", summary.allocated_pages);
    println!("   Fragmentation score: {:.2}", summary.fragmentation_score);
    println!();

    // --- Allocation Strategies ---
    println!("6. Allocation Strategies:");
    demonstrate_strategies();
    println!();

    println!("=== Example Complete ===");
}

/// Demonstrate different allocation strategies
fn demonstrate_strategies() {
    for (name, strategy) in [
        ("FirstFit", AllocationStrategy::FirstFit),
        ("BestFit", AllocationStrategy::BestFit),
        ("WorstFit", AllocationStrategy::WorstFit),
    ] {
        let mut mm = MemoryManager::with_strategy(strategy);

        // Create fragmentation pattern
        let a1 = mm.allocate_pages(10).unwrap();
        let a2 = mm.allocate_pages(5).unwrap();
        let a3 = mm.allocate_pages(20).unwrap();

        // Free middle allocation to create hole
        mm.free_pages(a2, 5).unwrap();

        // Allocate into the hole - strategy affects which block is chosen
        let a4 = mm.allocate_pages(5).unwrap();

        println!(
            "   {} strategy: new allocation at 0x{:08X} (hole was at 0x{:08X})",
            name, a4, a2
        );

        // Cleanup
        mm.free_pages(a1, 10).unwrap();
        mm.free_pages(a3, 20).unwrap();
        mm.free_pages(a4, 5).unwrap();
    }
}
