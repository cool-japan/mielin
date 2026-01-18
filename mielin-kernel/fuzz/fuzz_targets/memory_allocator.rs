#![no_main]

use libfuzzer_sys::fuzz_target;
use mielin_kernel::memory::{AllocationStrategy, MemoryManager};

// Fuzz operations for memory allocator
#[derive(Debug)]
enum MemOp {
    Allocate { pages: usize },
    Free { addr: usize },
    ChangeStrategy { strategy: AllocationStrategy },
    Defragment,
}

impl MemOp {
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        match data[0] % 4 {
            0 => {
                // Allocate
                if data.len() < 2 {
                    return None;
                }
                let pages = (data[1] as usize % 64) + 1; // 1-64 pages
                Some(MemOp::Allocate { pages })
            }
            1 => {
                // Free
                if data.len() < 5 {
                    return None;
                }
                let addr = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
                Some(MemOp::Free { addr })
            }
            2 => {
                // Change strategy
                if data.len() < 2 {
                    return None;
                }
                let strategy = match data[1] % 3 {
                    0 => AllocationStrategy::FirstFit,
                    1 => AllocationStrategy::BestFit,
                    _ => AllocationStrategy::WorstFit,
                };
                Some(MemOp::ChangeStrategy { strategy })
            }
            3 => Some(MemOp::Defragment),
            _ => None,
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // Skip too small inputs
    if data.len() < 2 {
        return;
    }

    // Initialize memory manager with 1MB (256 pages)
    let base_addr = 0x1000_0000;
    let mut mm = MemoryManager::new(base_addr, 256);

    // Track allocations for valid frees
    let mut allocations = Vec::new();

    // Parse and execute operations
    let mut i = 0;
    while i < data.len() {
        let remaining = &data[i..];
        if let Some(op) = MemOp::from_bytes(remaining) {
            match op {
                MemOp::Allocate { pages } => {
                    if let Ok(addr) = mm.allocate_pages(pages) {
                        allocations.push((addr, pages));
                    }
                }
                MemOp::Free { addr: _ } => {
                    // Free a random allocation
                    if !allocations.is_empty() {
                        let idx = data.get(i + 1).copied().unwrap_or(0) as usize % allocations.len();
                        let (addr, pages) = allocations.remove(idx);
                        let _ = mm.free_pages(addr, pages);
                    }
                }
                MemOp::ChangeStrategy { strategy } => {
                    mm.set_allocation_strategy(strategy);
                }
                MemOp::Defragment => {
                    mm.defragment();
                }
            }
            i += 5; // Move to next operation
        } else {
            i += 1;
        }
    }

    // Verify memory manager consistency
    let stats = mm.stats();
    let summary = mm.memory_summary();

    // Sanity checks
    assert!(stats.pages_allocated <= 256);
    assert!(stats.pages_free <= 256);
    assert_eq!(stats.pages_allocated + stats.pages_free, 256);
    assert!(summary.utilization >= 0.0 && summary.utilization <= 100.0);

    // Clean up remaining allocations
    for (addr, pages) in allocations {
        let _ = mm.free_pages(addr, pages);
    }

    // After freeing all, should have all pages free
    let final_stats = mm.stats();
    assert_eq!(final_stats.pages_allocated, 0);
    assert_eq!(final_stats.pages_free, 256);
});
