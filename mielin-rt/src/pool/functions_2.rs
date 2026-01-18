//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::super::functions::{DEFAULT_BLOCKS_PER_POOL, POOL_SIZES};
    use super::super::types::{
        AggregatePoolStats, Allocation, PoolAllocator, PoolConfig, PoolConfigError, PoolError,
        PoolSizeSelector, PoolStats,
    };
    use alloc::string::ToString;
    use alloc::vec::Vec;
    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.blocks_per_pool[0], DEFAULT_BLOCKS_PER_POOL);
        assert!(config.track_statistics);
        assert!(config.track_fragmentation);
    }
    #[test]
    fn test_pool_config_presets() {
        let minimal = PoolConfig::minimal();
        assert_eq!(minimal.blocks_per_pool[0], 16);
        assert!(!minimal.track_statistics);
        let standard = PoolConfig::standard();
        assert_eq!(standard.blocks_per_pool[0], 32);
        let generous = PoolConfig::generous();
        assert_eq!(generous.blocks_per_pool[0], 128);
    }
    #[test]
    fn test_pool_config_total_memory() {
        let config = PoolConfig::minimal();
        let total = config.total_memory();
        assert_eq!(total, 3840);
    }
    #[test]
    fn test_pool_stats() {
        let stats = PoolStats {
            total_blocks: 100,
            allocated_blocks: 50,
            peak_allocated: 75,
            allocation_count: 100,
            deallocation_count: 50,
            allocation_failures: 5,
        };
        assert_eq!(stats.free_blocks(), 50);
        assert_eq!(stats.utilization_percent(), 50);
        assert_eq!(stats.peak_utilization_percent(), 75);
    }
    #[test]
    fn test_pool_stats_empty() {
        let stats = PoolStats::default();
        assert_eq!(stats.utilization_percent(), 0);
        assert_eq!(stats.peak_utilization_percent(), 0);
    }
    #[test]
    fn test_allocator_creation() {
        let allocator = PoolAllocator::default();
        let stats = allocator.aggregate_stats();
        assert!(stats.total_blocks > 0);
        assert_eq!(stats.allocated_blocks, 0);
    }
    #[test]
    fn test_allocate_small() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(10).unwrap();
        assert_eq!(alloc.pool_index, 0);
        assert_eq!(alloc.size, 16);
    }
    #[test]
    fn test_allocate_exact() {
        let allocator = PoolAllocator::default();
        let alloc16 = allocator.allocate(16).unwrap();
        assert_eq!(alloc16.size, 16);
        let alloc32 = allocator.allocate(32).unwrap();
        assert_eq!(alloc32.size, 32);
        let alloc64 = allocator.allocate(64).unwrap();
        assert_eq!(alloc64.size, 64);
    }
    #[test]
    fn test_allocate_sizes() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(1).unwrap();
        assert_eq!(alloc.size, 16);
        let alloc = allocator.allocate(17).unwrap();
        assert_eq!(alloc.size, 32);
        let alloc = allocator.allocate(33).unwrap();
        assert_eq!(alloc.size, 64);
        let alloc = allocator.allocate(65).unwrap();
        assert_eq!(alloc.size, 128);
        let alloc = allocator.allocate(129).unwrap();
        assert_eq!(alloc.size, 256);
        let alloc = allocator.allocate(257).unwrap();
        assert_eq!(alloc.size, 512);
        let alloc = allocator.allocate(513).unwrap();
        assert_eq!(alloc.size, 1024);
    }
    #[test]
    fn test_allocate_too_large() {
        let allocator = PoolAllocator::default();
        let result = allocator.allocate(2000);
        assert_eq!(result.unwrap_err(), PoolError::SizeTooLarge);
    }
    #[test]
    fn test_deallocate() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(50).unwrap();
        assert!(allocator.deallocate(&alloc).is_ok());
        let stats = allocator.pool_stats(alloc.pool_index).unwrap();
        assert_eq!(stats.deallocation_count, 1);
    }
    #[test]
    fn test_pool_exhaustion() {
        let config = PoolConfig {
            blocks_per_pool: [2, 2, 2, 2, 2, 2, 2],
            track_statistics: true,
            track_fragmentation: false,
            fragmentation_threshold: 50,
        };
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        allocator.allocate(10).unwrap();
        allocator.allocate(10).unwrap();
        let result = allocator.allocate(10);
        assert_eq!(result.unwrap_err(), PoolError::PoolExhausted);
        let stats = allocator.pool_stats(0).unwrap();
        assert_eq!(stats.allocation_failures, 1);
    }
    #[test]
    fn test_aggregate_stats() {
        let allocator = PoolAllocator::default();
        allocator.allocate(10).unwrap();
        allocator.allocate(50).unwrap();
        allocator.allocate(100).unwrap();
        let agg = allocator.aggregate_stats();
        assert_eq!(agg.allocated_blocks, 3);
        assert_eq!(agg.allocation_count, 3);
    }
    #[test]
    fn test_fragmentation_info() {
        let allocator = PoolAllocator::default();
        allocator.allocate(10).unwrap();
        allocator.allocate(10).unwrap();
        let frag = allocator.fragmentation_info(0).unwrap();
        assert!(frag.free_blocks > 0);
    }
    #[test]
    fn test_available_memory() {
        let allocator = PoolAllocator::default();
        let initial = allocator.available_memory();
        allocator.allocate(64).unwrap();
        let after = allocator.available_memory();
        assert_eq!(initial - after, 64);
    }
    #[test]
    fn test_reset_stats() {
        let allocator = PoolAllocator::default();
        allocator.allocate(10).unwrap();
        allocator.allocate(10).unwrap();
        allocator.reset_stats();
        let stats = allocator.pool_stats(0).unwrap();
        assert_eq!(stats.allocation_count, 0);
    }
    #[test]
    fn test_internal_fragmentation() {
        let alloc = Allocation {
            pool_index: 0,
            block_index: 0,
            size: 16,
        };
        assert_eq!(alloc.internal_fragmentation(10), 6);
        assert_eq!(alloc.internal_fragmentation(16), 0);
    }
    #[test]
    fn test_pool_size_selector() {
        assert_eq!(PoolSizeSelector::select(10), Some(16));
        assert_eq!(PoolSizeSelector::select(16), Some(16));
        assert_eq!(PoolSizeSelector::select(17), Some(32));
        assert_eq!(PoolSizeSelector::select(1025), None);
        assert_eq!(PoolSizeSelector::index_for(10), Some(0));
        assert_eq!(PoolSizeSelector::index_for(100), Some(3));
        assert_eq!(PoolSizeSelector::internal_fragmentation(10), Some(6));
    }
    #[test]
    fn test_pool_error_display() {
        assert_eq!(
            PoolError::SizeTooLarge.to_string(),
            "Size too large for any pool"
        );
        assert_eq!(PoolError::PoolExhausted.to_string(), "Pool exhausted");
    }
    #[test]
    fn test_aggregate_stats_methods() {
        let agg = AggregatePoolStats {
            total_blocks: 100,
            allocated_blocks: 25,
            total_bytes: 1000,
            allocated_bytes: 250,
            allocation_count: 30,
            deallocation_count: 5,
            failure_count: 2,
        };
        assert_eq!(agg.block_utilization(), 25);
        assert_eq!(agg.byte_utilization(), 25);
        assert!(agg.success_rate() > 93.0);
        assert!(agg.success_rate() < 94.0);
    }
    #[test]
    fn test_high_fragmentation_detection() {
        let config = PoolConfig {
            blocks_per_pool: [4, 4, 4, 4, 4, 4, 4],
            track_statistics: true,
            track_fragmentation: true,
            fragmentation_threshold: 10,
        };
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        allocator.allocate(10).unwrap();
        allocator.allocate(10).unwrap();
        let _ = allocator.has_high_fragmentation();
    }
    #[test]
    fn test_best_fit_selection() {
        let result = PoolSizeSelector::best_fit(50);
        assert_eq!(result, Some((2, 64)));
        let result = PoolSizeSelector::best_fit(2000);
        assert_eq!(result, None);
    }
    #[test]
    fn test_pool_sizes_constant() {
        assert_eq!(POOL_SIZES.len(), 7);
        assert_eq!(POOL_SIZES[0], 16);
        assert_eq!(POOL_SIZES[6], 1024);
        for i in 1..POOL_SIZES.len() {
            assert!(POOL_SIZES[i] > POOL_SIZES[i - 1]);
        }
    }
    #[test]
    fn test_zero_size_allocation() {
        let allocator = PoolAllocator::default();
        let result = allocator.allocate(0);
        assert_eq!(result.unwrap_err(), PoolError::ZeroSizeAllocation);
    }
    #[test]
    fn test_allocation_size_too_large() {
        let allocator = PoolAllocator::default();
        let result = allocator.allocate(2048);
        assert_eq!(result.unwrap_err(), PoolError::SizeTooLarge);
    }
    #[test]
    fn test_invalid_allocation_structure() {
        let allocator = PoolAllocator::default();
        let invalid_alloc = Allocation {
            pool_index: 100,
            block_index: 0,
            size: 16,
        };
        assert_eq!(
            allocator.validate_allocation(&invalid_alloc).unwrap_err(),
            PoolError::InvalidPool
        );
    }
    #[test]
    fn test_invalid_block_index() {
        let allocator = PoolAllocator::default();
        let invalid_alloc = Allocation {
            pool_index: 0,
            block_index: 10000,
            size: 16,
        };
        assert_eq!(
            allocator.validate_allocation(&invalid_alloc).unwrap_err(),
            PoolError::InvalidBlockIndex
        );
    }
    #[test]
    fn test_invalid_allocation_size_mismatch() {
        let allocator = PoolAllocator::default();
        let invalid_alloc = Allocation {
            pool_index: 0,
            block_index: 0,
            size: 999,
        };
        assert_eq!(
            allocator.validate_allocation(&invalid_alloc).unwrap_err(),
            PoolError::InvalidAllocation
        );
    }
    #[test]
    fn test_double_free_detection() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(16).unwrap();
        assert!(allocator.deallocate(&alloc).is_ok());
        let result = allocator.deallocate(&alloc);
        assert_eq!(result.unwrap_err(), PoolError::DoubleFree);
    }
    #[test]
    fn test_deallocation_without_allocation() {
        let mut config = PoolConfig::default();
        config.blocks_per_pool[0] = 10;
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        let fake_alloc = Allocation {
            pool_index: 0,
            block_index: 0,
            size: 16,
        };
        let result = allocator.deallocate(&fake_alloc);
        assert_eq!(result.unwrap_err(), PoolError::DoubleFree);
    }
    #[test]
    fn test_pool_integrity_check_valid() {
        let allocator = PoolAllocator::default();
        assert!(allocator.check_integrity().is_ok());
        let _alloc1 = allocator.allocate(16).unwrap();
        let _alloc2 = allocator.allocate(32).unwrap();
        assert!(allocator.check_integrity().is_ok());
    }
    #[test]
    fn test_safe_deallocate() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(64).unwrap();
        assert!(allocator.safe_deallocate(&alloc).is_ok());
    }
    #[test]
    fn test_race_condition_protection() {
        let config = PoolConfig {
            blocks_per_pool: [1, 1, 1, 1, 1, 1, 1],
            track_statistics: true,
            track_fragmentation: false,
            fragmentation_threshold: 50,
        };
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        let alloc1 = allocator.allocate(16).unwrap();
        assert_eq!(alloc1.block_index, 0);
        let result = allocator.allocate(16);
        assert_eq!(result.unwrap_err(), PoolError::PoolExhausted);
    }
    #[test]
    fn test_allocation_validation_comprehensive() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(100).unwrap();
        assert!(allocator.validate_allocation(&alloc).is_ok());
        let mut bad_alloc = alloc;
        bad_alloc.pool_index = 99;
        assert!(allocator.validate_allocation(&bad_alloc).is_err());
        let mut bad_alloc = alloc;
        bad_alloc.size = 999;
        assert!(allocator.validate_allocation(&bad_alloc).is_err());
    }
    #[test]
    fn test_error_display_messages() {
        assert!(PoolError::ZeroSizeAllocation
            .to_string()
            .contains("Zero-size"));
        assert!(PoolError::InvalidBlockIndex
            .to_string()
            .contains("Invalid block"));
        assert!(PoolError::MemoryCorruption
            .to_string()
            .contains("corruption"));
        assert!(PoolError::InvalidAlignment
            .to_string()
            .contains("Alignment"));
    }
    #[test]
    fn test_multiple_allocations_integrity() {
        let allocator = PoolAllocator::default();
        let mut allocations = Vec::new();
        for size in [16, 32, 64, 128, 256] {
            for _ in 0..5 {
                allocations.push(allocator.allocate(size).unwrap());
            }
        }
        assert!(allocator.check_integrity().is_ok());
        for alloc in &allocations {
            assert!(allocator.deallocate(alloc).is_ok());
        }
        assert!(allocator.check_integrity().is_ok());
    }
    #[test]
    fn test_deallocation_order_independence() {
        let allocator = PoolAllocator::default();
        let alloc1 = allocator.allocate(16).unwrap();
        let alloc2 = allocator.allocate(16).unwrap();
        let alloc3 = allocator.allocate(16).unwrap();
        assert!(allocator.deallocate(&alloc2).is_ok());
        assert!(allocator.deallocate(&alloc1).is_ok());
        assert!(allocator.deallocate(&alloc3).is_ok());
        assert!(allocator.check_integrity().is_ok());
    }
    #[test]
    fn test_cross_pool_deallocation_safety() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(32).unwrap();
        let mut wrong_alloc = alloc;
        wrong_alloc.pool_index = 5;
        wrong_alloc.size = POOL_SIZES[5];
        let result = allocator.deallocate(&wrong_alloc);
        assert!(result.is_err());
        assert!(allocator.deallocate(&alloc).is_ok());
    }
    #[test]
    fn test_allocation_boundary_sizes() {
        let allocator = PoolAllocator::default();
        for &size in POOL_SIZES.iter() {
            let alloc = allocator.allocate(size).unwrap();
            assert_eq!(alloc.size, size);
            allocator.deallocate(&alloc).unwrap();
        }
        for &size in POOL_SIZES.iter().skip(1) {
            let alloc = allocator.allocate(size - 1).unwrap();
            assert!(alloc.size >= size - 1);
            allocator.deallocate(&alloc).unwrap();
        }
    }
    #[test]
    fn test_maximum_size_allocation() {
        let allocator = PoolAllocator::default();
        let max_size = POOL_SIZES[POOL_SIZES.len() - 1];
        let alloc = allocator.allocate(max_size).unwrap();
        assert_eq!(alloc.size, max_size);
        allocator.deallocate(&alloc).unwrap();
        assert!(matches!(
            allocator.allocate(max_size + 1),
            Err(PoolError::SizeTooLarge)
        ));
    }
    #[test]
    fn test_alternating_allocation_pattern() {
        let allocator = PoolAllocator::default();
        let mut allocations = Vec::new();
        for i in 0..20 {
            let size = if i % 2 == 0 { 32 } else { 128 };
            allocations.push(allocator.allocate(size).unwrap());
        }
        while let Some(alloc) = allocations.pop() {
            allocator.deallocate(&alloc).unwrap();
        }
        assert!(allocator.check_integrity().is_ok());
    }
    #[test]
    fn test_stress_single_pool() {
        let allocator = PoolAllocator::default();
        let mut allocations = Vec::new();
        for _ in 0..100 {
            if let Ok(alloc) = allocator.allocate(64) {
                allocations.push(alloc);
            } else {
                break;
            }
        }
        assert!(!allocations.is_empty());
        for alloc in allocations {
            allocator.deallocate(&alloc).unwrap();
        }
    }
    #[test]
    fn test_interleaved_alloc_dealloc() {
        let allocator = PoolAllocator::default();
        for _ in 0..100 {
            let alloc1 = allocator.allocate(32).unwrap();
            let alloc2 = allocator.allocate(64).unwrap();
            allocator.deallocate(&alloc1).unwrap();
            let alloc3 = allocator.allocate(128).unwrap();
            allocator.deallocate(&alloc2).unwrap();
            allocator.deallocate(&alloc3).unwrap();
        }
        assert!(allocator.check_integrity().is_ok());
    }
    #[test]
    fn test_pool_stats_accuracy() {
        let allocator = PoolAllocator::default();
        let alloc1 = allocator.allocate(16).unwrap();
        let alloc2 = allocator.allocate(16).unwrap();
        let stats = allocator.pool_stats(0).unwrap();
        assert_eq!(stats.allocated_blocks, 2);
        assert!(stats.peak_allocated >= 2);
        allocator.deallocate(&alloc1).unwrap();
        let stats = allocator.pool_stats(0).unwrap();
        assert_eq!(stats.allocated_blocks, 1);
        allocator.deallocate(&alloc2).unwrap();
        let stats = allocator.pool_stats(0).unwrap();
        assert_eq!(stats.allocated_blocks, 0);
    }
    #[test]
    fn test_fragmentation_tracking() {
        let config = PoolConfig {
            track_fragmentation: true,
            fragmentation_threshold: 50,
            ..Default::default()
        };
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        let alloc1 = allocator.allocate(64).unwrap();
        let alloc2 = allocator.allocate(64).unwrap();
        let alloc3 = allocator.allocate(64).unwrap();
        allocator.deallocate(&alloc2).unwrap();
        assert!(alloc1.size == 64);
        assert!(alloc3.size == 64);
        allocator.deallocate(&alloc1).unwrap();
        allocator.deallocate(&alloc3).unwrap();
    }
    #[test]
    fn test_allocation_after_reset() {
        let allocator = PoolAllocator::default();
        let alloc = allocator.allocate(32).unwrap();
        allocator.deallocate(&alloc).unwrap();
        allocator.reset_stats();
        let stats = allocator.aggregate_stats();
        assert_eq!(stats.allocation_count, 0);
        assert_eq!(stats.deallocation_count, 0);
        let alloc2 = allocator.allocate(32).unwrap();
        allocator.deallocate(&alloc2).unwrap();
    }
    #[test]
    fn test_concurrent_size_allocations() {
        let allocator = PoolAllocator::default();
        let allocs: Vec<_> = POOL_SIZES
            .iter()
            .map(|&size| allocator.allocate(size).unwrap())
            .collect();
        for (i, alloc) in allocs.iter().enumerate() {
            assert_eq!(alloc.size, POOL_SIZES[i]);
        }
        for alloc in allocs {
            allocator.deallocate(&alloc).unwrap();
        }
        assert!(allocator.check_integrity().is_ok());
    }
    #[test]
    fn test_allocation_with_disabled_stats() {
        let config = PoolConfig {
            blocks_per_pool: [DEFAULT_BLOCKS_PER_POOL; 7],
            track_statistics: false,
            track_fragmentation: false,
            fragmentation_threshold: 50,
        };
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        let alloc = allocator.allocate(64).unwrap();
        allocator.deallocate(&alloc).unwrap();
    }
    #[test]
    fn test_pool_utilization_calculation() {
        let allocator = PoolAllocator::default();
        let mut allocs = Vec::new();
        for _ in 0..32 {
            allocs.push(allocator.allocate(64).unwrap());
        }
        let stats = allocator.pool_stats(2).unwrap();
        let utilization = stats.utilization_percent();
        assert!((40..=60).contains(&utilization));
        for alloc in allocs {
            allocator.deallocate(&alloc).unwrap();
        }
    }
    #[test]
    fn test_pool_config_tiny_preset() {
        let config = PoolConfig::tiny();
        assert_eq!(config.blocks_per_pool[0], 8);
        assert_eq!(config.blocks_per_pool[1], 8);
        assert_eq!(config.blocks_per_pool[5], 0);
        assert_eq!(config.blocks_per_pool[6], 0);
        assert!(!config.track_statistics);
        assert_eq!(config.fragmentation_threshold, 80);
    }
    #[test]
    fn test_pool_config_ultra_low_power_preset() {
        let config = PoolConfig::ultra_low_power();
        assert_eq!(config.blocks_per_pool[0], 12);
        assert_eq!(config.blocks_per_pool[6], 0);
        assert!(!config.track_statistics);
    }
    #[test]
    fn test_pool_config_custom() {
        let counts = [10, 20, 30, 40, 50, 60, 70];
        let config = PoolConfig::custom(counts);
        assert_eq!(config.blocks_per_pool, counts);
        assert!(config.track_statistics);
        assert!(config.track_fragmentation);
    }
    #[test]
    fn test_pool_config_with_pool_blocks() {
        let config = PoolConfig::default().with_pool_blocks(0, 100);
        assert_eq!(config.blocks_per_pool[0], 100);
        let config = PoolConfig::default().with_pool_blocks(99, 100);
        assert_eq!(config.blocks_per_pool[0], DEFAULT_BLOCKS_PER_POOL);
    }
    #[test]
    fn test_pool_config_builder_methods() {
        let config = PoolConfig::default()
            .with_16b_blocks(50)
            .with_32b_blocks(40)
            .with_64b_blocks(30)
            .with_128b_blocks(20)
            .with_256b_blocks(10)
            .with_512b_blocks(5)
            .with_1kb_blocks(2);
        assert_eq!(config.blocks_per_pool[0], 50);
        assert_eq!(config.blocks_per_pool[1], 40);
        assert_eq!(config.blocks_per_pool[2], 30);
        assert_eq!(config.blocks_per_pool[3], 20);
        assert_eq!(config.blocks_per_pool[4], 10);
        assert_eq!(config.blocks_per_pool[5], 5);
        assert_eq!(config.blocks_per_pool[6], 2);
    }
    #[test]
    fn test_pool_config_with_statistics() {
        let config = PoolConfig::default().with_statistics(false);
        assert!(!config.track_statistics);
        let config = PoolConfig::minimal().with_statistics(true);
        assert!(config.track_statistics);
    }
    #[test]
    fn test_pool_config_with_fragmentation_tracking() {
        let config = PoolConfig::default().with_fragmentation_tracking(false);
        assert!(!config.track_fragmentation);
        let config = PoolConfig::minimal().with_fragmentation_tracking(true);
        assert!(config.track_fragmentation);
    }
    #[test]
    fn test_pool_config_with_fragmentation_threshold() {
        let config = PoolConfig::default().with_fragmentation_threshold(75);
        assert_eq!(config.fragmentation_threshold, 75);
    }
    #[test]
    fn test_pool_config_scale_by() {
        let config = PoolConfig::standard().scale_by(2.0);
        assert_eq!(config.blocks_per_pool[0], 64);
        assert_eq!(config.blocks_per_pool[3], 32);
    }
    #[test]
    fn test_pool_config_scale_by_fractional() {
        let config = PoolConfig::standard().scale_by(0.5);
        assert_eq!(config.blocks_per_pool[0], 16);
        assert_eq!(config.blocks_per_pool[3], 8);
    }
    #[test]
    fn test_pool_config_limit_to_bytes() {
        let config = PoolConfig::generous();
        let original_memory = config.total_memory();
        let limited = config.limit_to_bytes(original_memory / 2);
        let new_memory = limited.total_memory();
        assert!(new_memory < original_memory);
        assert!(new_memory > original_memory / 3);
    }
    #[test]
    fn test_pool_config_limit_to_bytes_no_change() {
        let config = PoolConfig::minimal();
        let original_memory = config.total_memory();
        let limited = config.limit_to_bytes(original_memory * 2);
        assert_eq!(limited.total_memory(), original_memory);
    }
    #[test]
    fn test_pool_config_for_ram_size_tiny() {
        let config = PoolConfig::for_ram_size(2048);
        assert!(config.total_memory() < 2048);
    }
    #[test]
    fn test_pool_config_for_ram_size_small() {
        let config = PoolConfig::for_ram_size(8192);
        assert!(config.total_memory() <= 4096);
    }
    #[test]
    fn test_pool_config_for_ram_size_medium() {
        let config = PoolConfig::for_ram_size(32768);
        assert!(config.total_memory() <= 16384);
    }
    #[test]
    fn test_pool_config_for_ram_size_large() {
        let config = PoolConfig::for_ram_size(131072);
        assert!(config.total_memory() <= 65536);
    }
    #[test]
    fn test_pool_config_memory_breakdown() {
        let config = PoolConfig::minimal();
        let breakdown = config.memory_breakdown();
        assert_eq!(breakdown[0], (16, 16, 16 * 16));
        assert_eq!(breakdown[1], (32, 16, 32 * 16));
        assert_eq!(breakdown[2], (64, 8, 64 * 8));
        let sum: usize = breakdown.iter().map(|(_, _, total)| total).sum();
        assert_eq!(sum, config.total_memory());
    }
    #[test]
    fn test_pool_config_validate_valid() {
        let config = PoolConfig::default();
        assert!(config.validate().is_ok());
        let config = PoolConfig::minimal();
        assert!(config.validate().is_ok());
        let config = PoolConfig::custom([1, 0, 0, 0, 0, 0, 0]);
        assert!(config.validate().is_ok());
    }
    #[test]
    fn test_pool_config_validate_no_pools() {
        let config = PoolConfig::custom([0, 0, 0, 0, 0, 0, 0]);
        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PoolConfigError::NoPoolsConfigured);
    }
    #[test]
    fn test_pool_config_validate_invalid_threshold() {
        let config = PoolConfig {
            fragmentation_threshold: 101,
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PoolConfigError::InvalidThreshold);
    }
    #[test]
    fn test_pool_config_error_display() {
        use alloc::string::ToString;
        let err = PoolConfigError::NoPoolsConfigured;
        assert!(err.to_string().contains("At least one pool"));
        let err = PoolConfigError::InvalidThreshold;
        assert!(err.to_string().contains("0 and 100"));
    }
    #[test]
    fn test_allocator_with_custom_config() {
        let config = PoolConfig::default()
            .with_16b_blocks(10)
            .with_64b_blocks(5)
            .with_statistics(true);
        assert!(config.validate().is_ok());
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        let alloc = allocator.allocate(16).unwrap();
        assert_eq!(alloc.size, 16);
        allocator.deallocate(&alloc).unwrap();
    }
    #[test]
    fn test_allocator_with_ram_budget_config() {
        let config = PoolConfig::for_ram_size(16384);
        assert!(config.validate().is_ok());
        let mut allocator = PoolAllocator::new(config);
        allocator.init();
        let alloc = allocator.allocate(64).unwrap();
        allocator.deallocate(&alloc).unwrap();
    }
    #[test]
    fn test_pool_config_chaining() {
        let config = PoolConfig::minimal()
            .with_16b_blocks(50)
            .with_statistics(true)
            .with_fragmentation_tracking(true)
            .with_fragmentation_threshold(60)
            .scale_by(2.0);
        assert_eq!(config.blocks_per_pool[0], 100);
        assert!(config.track_statistics);
        assert!(config.track_fragmentation);
        assert_eq!(config.fragmentation_threshold, 60);
    }
    #[test]
    fn test_tiny_config_memory_usage() {
        let config = PoolConfig::tiny();
        let total = config.total_memory();
        assert_eq!(total, 1152);
    }
    #[test]
    fn test_ultra_low_power_config_memory_usage() {
        let config = PoolConfig::ultra_low_power();
        let total = config.total_memory();
        assert_eq!(total, 2368);
    }
    #[test]
    fn test_config_for_different_ram_sizes() {
        let sizes = [1024, 4096, 16384, 65536, 262144];
        for &ram_size in &sizes {
            let config = PoolConfig::for_ram_size(ram_size);
            assert!(config.validate().is_ok());
            assert!(config.total_memory() <= ram_size / 2 + 1024);
        }
    }
}
