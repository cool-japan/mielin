//! Memory Pool Usage Example
//!
//! Demonstrates how to use MielinRT's memory pool allocator
//! for deterministic memory allocation in embedded systems.

#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::Layout;

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

struct DummyAllocator;

unsafe impl core::alloc::GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

use mielin_rt::pool::{PoolAllocator, PoolConfig, PoolError};
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    // Example 1: Basic allocation and deallocation
    example_basic_allocation();

    // Example 2: Custom pool configuration
    example_custom_configuration();

    // Example 3: Pool statistics and monitoring
    example_pool_monitoring();

    // Example 4: Error handling
    example_error_handling();

    // Example 5: Safety checks
    example_safety_checks();

    loop {
        cortex_m::asm::wfi();
    }
}

/// Example 1: Basic allocation and deallocation
fn example_basic_allocation() {
    let allocator = PoolAllocator::default();

    // Allocate different sizes
    let small = allocator.allocate(16).unwrap();
    let medium = allocator.allocate(128).unwrap();
    let large = allocator.allocate(512).unwrap();

    // Use allocations...
    process_buffer(&small);
    process_buffer(&medium);
    process_buffer(&large);

    // Deallocate when done
    allocator.deallocate(&small).unwrap();
    allocator.deallocate(&medium).unwrap();
    allocator.deallocate(&large).unwrap();
}

/// Example 2: Custom pool configuration
fn example_custom_configuration() {
    // Create minimal configuration for resource-constrained systems
    let minimal_config = PoolConfig::minimal();
    let mut minimal_allocator = PoolAllocator::new(minimal_config);
    minimal_allocator.init();

    // Create generous configuration for systems with more RAM
    let generous_config = PoolConfig::generous();
    let mut generous_allocator = PoolAllocator::new(generous_config);
    generous_allocator.init();

    // Create custom configuration
    let custom_config = PoolConfig {
        blocks_per_pool: [32, 32, 16, 8, 4, 2, 1], // Custom block counts
        track_statistics: true,
        track_fragmentation: true,
        fragmentation_threshold: 60,
    };
    let mut custom_allocator = PoolAllocator::new(custom_config);
    custom_allocator.init();

    // Use the allocator
    let alloc = custom_allocator.allocate(64).unwrap();
    custom_allocator.deallocate(&alloc).unwrap();
}

/// Example 3: Pool statistics and monitoring
fn example_pool_monitoring() {
    let allocator = PoolAllocator::default();

    // Perform some allocations
    let mut allocations = alloc::vec::Vec::new();
    for size in [16, 32, 64, 128, 256] {
        for _ in 0..10 {
            if let Ok(alloc) = allocator.allocate(size) {
                allocations.push(alloc);
            }
        }
    }

    // Get aggregate statistics
    let stats = allocator.aggregate_stats();
    check_pool_health(&stats);

    // Get per-pool statistics
    for i in 0..7 {
        if let Some(pool_stats) = allocator.pool_stats(i) {
            if pool_stats.allocation_failures > 0 {
                // This pool is running out of blocks
                handle_pool_exhaustion(i, &pool_stats);
            }

            // Check utilization as a proxy for fragmentation concerns
            if pool_stats.utilization_percent() > 90 {
                // High utilization detected
                handle_fragmentation(i);
            }
        }
    }

    // Check available memory
    let available = allocator.available_memory();
    if available < 1024 {
        // Less than 1KB available, trigger cleanup
        trigger_memory_cleanup();
    }

    // Deallocate all
    for alloc in allocations {
        allocator.deallocate(&alloc).unwrap();
    }

    // Reset statistics
    allocator.reset_stats();
}

/// Example 4: Error handling
fn example_error_handling() {
    let allocator = PoolAllocator::default();

    // Handle allocation failures gracefully
    match allocator.allocate(2048) {
        Ok(_) => {
            // Should not happen - size too large
        }
        Err(PoolError::SizeTooLarge) => {
            // Size exceeds maximum pool size
            // Fall back to alternative strategy
            fallback_allocation_strategy();
        }
        Err(e) => {
            // Handle other errors
            panic!("Unexpected error: {}", e);
        }
    }

    // Exhaust a pool
    let config = PoolConfig {
        blocks_per_pool: [2, 2, 2, 2, 2, 2, 2],
        ..Default::default()
    };
    let mut small_allocator = PoolAllocator::new(config);
    small_allocator.init();

    let alloc1 = small_allocator.allocate(16).unwrap();
    let alloc2 = small_allocator.allocate(16).unwrap();

    if let Err(PoolError::PoolExhausted) = small_allocator.allocate(16) {
        // Pool exhausted, handle appropriately
        handle_pool_exhaustion_error();
    }

    small_allocator.deallocate(&alloc1).unwrap();
    small_allocator.deallocate(&alloc2).unwrap();
}

/// Example 5: Safety checks
fn example_safety_checks() {
    let allocator = PoolAllocator::default();

    // Check integrity before operations
    allocator.check_integrity().unwrap();

    let alloc = allocator.allocate(64).unwrap();

    // Use safe deallocation (with integrity checks)
    allocator.safe_deallocate(&alloc).unwrap();

    // Verify double-free protection
    match allocator.deallocate(&alloc) {
        Err(PoolError::DoubleFree) => {
            // Double free correctly detected
        }
        _ => panic!("Double free should be detected!"),
    }

    // Zero-size allocation prevention
    match allocator.allocate(0) {
        Err(PoolError::ZeroSizeAllocation) => {
            // Correctly rejected
        }
        _ => panic!("Zero-size allocation should be rejected!"),
    }

    // Final integrity check
    allocator.check_integrity().unwrap();
}

// Helper functions
fn process_buffer(_alloc: &mielin_rt::pool::Allocation) {
    // Process allocated buffer
}

fn check_pool_health(_stats: &mielin_rt::pool::AggregatePoolStats) {
    // Check overall pool health
}

fn handle_pool_exhaustion(_pool_idx: usize, _stats: &mielin_rt::pool::PoolStats) {
    // Increase pool size or trigger garbage collection
}

fn handle_fragmentation(_pool_idx: usize) {
    // Consider defragmentation or pool compaction
}

fn trigger_memory_cleanup() {
    // Trigger application-level memory cleanup
}

fn fallback_allocation_strategy() {
    // Use alternative allocation method
}

fn handle_pool_exhaustion_error() {
    // Handle pool exhaustion gracefully
}
