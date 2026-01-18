//! Pool Configuration Example
//!
//! Demonstrates the flexible memory pool configuration options in MielinRT,
//! showing how to customize pool sizes for different system requirements.
//!
//! ## Features Demonstrated
//!
//! - Preset configurations (tiny, minimal, standard, generous)
//! - Custom pool configurations
//! - RAM-budget based configuration
//! - Dynamic pool scaling
//! - Builder pattern usage
//! - Configuration validation

#![no_std]
#![no_main]

extern crate alloc;
extern crate panic_halt;

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

use mielin_rt::pool::{PoolAllocator, PoolConfig};

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Example 1: Using Preset Configurations
    example_preset_configs();

    // Example 2: Custom Configuration with Builder Pattern
    example_custom_config();

    // Example 3: RAM-Budget Based Configuration
    example_ram_budget_config();

    // Example 4: Fine-Grained Control
    example_fine_grained_config();

    // Example 5: Scaling Configurations
    example_scaling_config();

    // Example 6: Memory Breakdown Analysis
    example_memory_analysis();

    loop {}
}

/// Example 1: Using preset configurations for common scenarios
fn example_preset_configs() {
    // Tiny configuration - for very small MCUs (<32KB RAM)
    let tiny_config = PoolConfig::tiny();
    // Only 1152 bytes total (8*16 + 8*32 + 4*64 + 2*128 + 1*256)

    // Minimal configuration - for small embedded systems
    let _minimal_config = PoolConfig::minimal();
    // 3840 bytes total, no statistics tracking

    // Standard configuration - balanced for most use cases
    let _standard_config = PoolConfig::standard();
    // ~16KB total, statistics enabled

    // Generous configuration - for resource-rich systems
    let _generous_config = PoolConfig::generous();
    // ~56KB total, full monitoring

    // Ultra-low power configuration - optimized for battery life
    let _ulp_config = PoolConfig::ultra_low_power();
    // 2368 bytes total, minimal overhead

    // Create allocators with different presets
    let mut tiny_allocator = PoolAllocator::new(tiny_config);
    tiny_allocator.init();

    // Allocate from tiny pool
    if let Ok(alloc) = tiny_allocator.allocate(24) {
        // Will use 32B pool
        tiny_allocator.deallocate(&alloc).ok();
    }
}

/// Example 2: Custom configuration using builder pattern
fn example_custom_config() {
    // Build a custom configuration for a specific use case
    let config = PoolConfig::default()
        .with_16b_blocks(100) // Many small allocations
        .with_32b_blocks(80)
        .with_64b_blocks(50)
        .with_128b_blocks(25)
        .with_256b_blocks(10) // Few large allocations
        .with_512b_blocks(5)
        .with_1kb_blocks(2)
        .with_statistics(true) // Enable monitoring
        .with_fragmentation_tracking(true)
        .with_fragmentation_threshold(40);

    // Validate configuration
    if config.validate().is_ok() {
        let mut allocator = PoolAllocator::new(config);
        allocator.init();

        // Use the custom allocator
        if let Ok(alloc) = allocator.allocate(100) {
            allocator.deallocate(&alloc).ok();
        }
    }
}

/// Example 3: Automatic configuration based on available RAM
fn example_ram_budget_config() {
    // System with 4KB RAM - gets tiny/minimal config
    let _config_4kb = PoolConfig::for_ram_size(4096);

    // System with 16KB RAM - gets minimal config scaled down
    let config_16kb = PoolConfig::for_ram_size(16384);

    // System with 64KB RAM - gets standard config scaled down
    let _config_64kb = PoolConfig::for_ram_size(65536);

    // System with 256KB RAM - gets generous config
    let _config_256kb = PoolConfig::for_ram_size(262144);

    // Each config automatically allocates ~50% of RAM to pools
    // Remaining RAM available for stack, heap, static data

    let mut allocator = PoolAllocator::new(config_16kb);
    allocator.init();

    // Allocator is optimized for the 16KB RAM budget
    if let Ok(alloc) = allocator.allocate(64) {
        allocator.deallocate(&alloc).ok();
    }
}

/// Example 4: Fine-grained control with specific pool sizes
fn example_fine_grained_config() {
    // Scenario: IoT sensor node that mostly allocates sensor readings (64B)
    // and occasional messages (256B)
    let config = PoolConfig::minimal()
        .with_16b_blocks(10) // Few small allocations
        .with_32b_blocks(10)
        .with_64b_blocks(50) // Many sensor readings
        .with_128b_blocks(5)
        .with_256b_blocks(20) // Message buffers
        .with_512b_blocks(2) // Rare large buffers
        .with_1kb_blocks(1) // Emergency buffer
        .with_statistics(false) // Disable for power savings
        .with_fragmentation_tracking(false);

    let mut allocator = PoolAllocator::new(config);
    allocator.init();

    // Simulate sensor data allocation pattern
    let mut readings = [None; 10];
    for reading in &mut readings {
        *reading = allocator.allocate(64).ok();
    }

    // Cleanup
    for reading in readings.iter().flatten() {
        allocator.deallocate(reading).ok();
    }
}

/// Example 5: Scaling configurations dynamically
fn example_scaling_config() {
    // Start with standard config
    let base_config = PoolConfig::standard();

    // Scale up by 2x for development/debugging
    let _debug_config = base_config.scale_by(2.0);

    // Scale down by 0.5x for production
    let _production_config = base_config.scale_by(0.5);

    // Limit to specific memory budget
    let limited_config = PoolConfig::generous().limit_to_bytes(8192); // Max 8KB

    let mut allocator = PoolAllocator::new(limited_config);
    allocator.init();

    // Allocator respects the 8KB memory limit
    if let Ok(alloc) = allocator.allocate(512) {
        allocator.deallocate(&alloc).ok();
    }
}

/// Example 6: Analyzing memory usage
fn example_memory_analysis() {
    let config = PoolConfig::standard();

    // Get total memory required
    let total_memory = config.total_memory();

    // Get detailed breakdown by pool size
    let _breakdown = config.memory_breakdown();
    // breakdown[i] = (block_size, block_count, total_bytes_for_this_pool)

    // Example: Check if config fits in available RAM
    const AVAILABLE_RAM: usize = 32768; // 32KB
    if total_memory <= AVAILABLE_RAM / 2 {
        // Good - pools use <= 50% of RAM
        let mut allocator = PoolAllocator::new(config);
        allocator.init();

        // Monitor pool usage
        let stats = allocator.aggregate_stats();
        let _utilization = stats.block_utilization(); // Percentage

        // Check individual pool stats
        if let Some(pool_stats) = allocator.pool_stats(2) {
            // 64B pool stats
            let _free_blocks = pool_stats.free_blocks();
            let _peak_usage = pool_stats.peak_allocated;
        }
    }
}

/// Scenario-based configurations
/// Configuration for low-power IoT sensor
fn _iot_sensor_config() -> PoolConfig {
    PoolConfig::minimal()
        .with_64b_blocks(30) // Sensor readings
        .with_128b_blocks(10) // Aggregated data
        .with_256b_blocks(5) // Network packets
        .with_statistics(false) // Save power
        .limit_to_bytes(2048) // 2KB budget
}

/// Configuration for real-time data logger
fn _data_logger_config() -> PoolConfig {
    PoolConfig::standard()
        .with_256b_blocks(50) // Log entries
        .with_512b_blocks(20) // Buffered logs
        .with_1kb_blocks(10) // Flash page buffers
        .with_fragmentation_tracking(true)
}

/// Configuration for wireless gateway
fn _gateway_config() -> PoolConfig {
    PoolConfig::generous()
        .with_256b_blocks(100) // Message buffers
        .with_512b_blocks(50) // Packet assembly
        .with_1kb_blocks(25) // Large payloads
        .scale_by(1.5) // Extra capacity
}

/// Configuration for resource-constrained bootloader
fn _bootloader_config() -> PoolConfig {
    PoolConfig::tiny()
        .with_128b_blocks(4) // Command buffers
        .with_256b_blocks(2) // Flash page cache
        .with_statistics(false)
        .with_fragmentation_tracking(false)
}

// Best practices demonstrated:
//
// 1. **Start with a preset** - Choose the closest preset to your needs
// 2. **Measure actual usage** - Enable statistics during development
// 3. **Optimize for your pattern** - Increase pools you use most
// 4. **Budget for overhead** - Leave ~50% RAM for other uses
// 5. **Validate configurations** - Always call .validate()
// 6. **Consider power consumption** - Disable tracking in production if needed
// 7. **Plan for fragmentation** - Set appropriate thresholds
// 8. **Test under load** - Verify pool exhaustion handling
