//! Heap Allocator Example
//!
//! Demonstrates the kernel's hybrid heap allocator that combines:
//! - Pool allocator for small allocations (fast, fixed-size blocks)
//! - Bump allocator for large allocations (simple, cache-friendly)
//! - OOM handler for memory exhaustion recovery

use mielin_kernel::heap::{oom_stats, set_oom_handler, stats as heap_stats};
use mielin_kernel::pool;
use std::alloc::Layout;

fn main() {
    println!("=== MielinOS Heap Allocator Example ===\n");

    // Initialize the pool allocator
    pool::init();
    println!("1. Pool Allocator Initialized\n");

    // --- Small Allocations (Pool) ---
    println!("2. Small Allocations (Pool Allocator):");
    demonstrate_pool_allocations();
    println!();

    // --- Large Allocations (Bump) ---
    println!("3. Large Allocations (Bump Allocator):");
    demonstrate_bump_allocations();
    println!();

    // --- Heap Statistics ---
    println!("4. Heap Statistics:");
    let stats = heap_stats();
    println!("   Pool allocations: {}", stats.pool_allocations);
    println!("   Bump allocations: {}", stats.bump_allocations);
    println!("   Pool deallocations: {}", stats.pool_deallocations);
    println!();

    // --- OOM Handler ---
    println!("5. OOM Handler:");
    demonstrate_oom_handler();
    println!();

    println!("=== Example Complete ===");
}

fn demonstrate_pool_allocations() {
    // Pool handles small allocations efficiently
    // Block sizes: 16, 32, 64, 128, 256, 512, 1024, 2048 bytes

    let sizes = [8, 16, 32, 64, 128, 256, 512, 1024];

    for size in sizes {
        // Allocate using Vec (uses global allocator)
        let data: Vec<u8> = vec![0u8; size];
        println!(
            "   Allocated {} bytes at {:p} (pool block)",
            size,
            data.as_ptr()
        );
    }

    println!("   All allocations fit in pool blocks (<=2048 bytes)");
}

fn demonstrate_bump_allocations() {
    // Bump allocator handles large allocations
    let sizes = [4096, 8192, 16384];

    for size in sizes {
        let data: Vec<u8> = vec![0u8; size];
        println!(
            "   Allocated {} bytes at {:p} (bump allocator)",
            size,
            data.as_ptr()
        );
    }

    println!("   Large allocations use bump allocator (>2048 bytes)");
}

fn demonstrate_oom_handler() {
    // Custom OOM handler
    fn my_oom_handler(layout: Layout) -> bool {
        println!(
            "   OOM handler called: requested {} bytes, align {}",
            layout.size(),
            layout.align()
        );
        // Return false = no recovery, don't retry
        // Return true = freed some memory, retry allocation
        false
    }

    // Set custom handler
    set_oom_handler(my_oom_handler);
    println!("   Custom OOM handler installed");

    // Get OOM statistics
    let stats = oom_stats();
    println!("   OOM count: {}", stats.oom_count);
    println!("   Recovery count: {}", stats.recovery_count);
    println!("   Failed count: {}", stats.failed_count);
}
