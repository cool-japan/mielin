# Memory Layout Requirements for MielinRT

This document describes the memory layout requirements and considerations for deploying MielinRT on embedded systems.

## Overview

MielinRT is designed for resource-constrained embedded systems with the following memory characteristics:
- **RAM**: 8KB minimum (32KB recommended for full features)
- **Flash**: 64KB minimum (128KB recommended)
- **Stack**: 2KB minimum per task
- **Heap**: Not required (no_std compatible)

## Memory Regions

### 1. Static Memory (.data/.bss)

#### Power Management Subsystems
```
AdvancedPowerManager:
├── DVFS Controller:           ~200 bytes
│   ├── Operating points (5):   120 bytes (24 bytes each)
│   ├── State tracking:          48 bytes
│   └── Statistics:              32 bytes
├── Power Gating:               ~256 bytes
│   ├── State map (BTreeMap):   128 bytes (estimated)
│   ├── Ref count map:          128 bytes (estimated)
│   └── Statistics:              32 bytes
├── Clock Gating:               ~128 bytes
│   ├── State map (12 domains): 48 bytes
│   ├── Frequency map:          48 bytes
│   └── Statistics:              32 bytes
├── Wake Controller:            ~512 bytes
│   ├── Wake sources (Vec):     256 bytes (capacity dependent)
│   ├── Configuration:          256 bytes
│   └── Statistics:              32 bytes
└── Total:                      ~1096 bytes
```

#### Battery Management
```
BatteryManager:
├── Configuration:               56 bytes
├── Fuel gauge reading:          32 bytes
├── Health tracker:             128 bytes
├── Power consumption:           48 bytes
├── History buffer (10 items):   10 bytes
├── Calibration (optional):     Variable
│   └── Points (Vec):           ~320 bytes (10 points × 32 bytes)
└── Total:                      ~594 bytes (with calibration)
```

#### Memory Pool Allocator
```
PoolAllocator (default config):
├── Configuration:               60 bytes
├── Atomic counters (7 pools):
│   ├── Allocated:              56 bytes (7 × 8)
│   ├── Peak:                   56 bytes
│   ├── Alloc counts:           56 bytes
│   ├── Dealloc counts:         56 bytes
│   ├── Failure counts:         56 bytes
│   ├── Free heads:             56 bytes
│   └── Fragmentation:          56 bytes
├── Pool stats:                 336 bytes (7 × 48)
└── Total:                      ~788 bytes (metadata only)
```

#### Stack Management
```
StackManager (8 stacks):
├── Configuration:               48 bytes
├── Stack descriptors:          512 bytes (8 × 64 bytes)
├── Statistics:                  64 bytes
└── Total:                      ~624 bytes
```

**Total Static Memory: ~3.1 KB**

### 2. Heap Allocations (alloc crate)

While MielinRT can operate without a heap, certain features benefit from dynamic allocation:

```
Optional Heap Usage:
├── Battery calibration points:  Variable (typical: 320 bytes)
├── Wake source configurations:  Variable (typical: 256 bytes)
├── Event queue (if used):       Variable (typical: 1-2 KB)
└── Estimated Total:             ~2-3 KB (for full features)
```

### 3. Pool Memory

The memory pool allocator manages its own memory regions:

```
Memory Pool Blocks (default configuration):
├── 16B pool:   64 blocks  =  1,024 bytes
├── 32B pool:   64 blocks  =  2,048 bytes
├── 64B pool:   64 blocks  =  4,096 bytes
├── 128B pool:  32 blocks  =  4,096 bytes
├── 256B pool:  16 blocks  =  4,096 bytes
├── 512B pool:   8 blocks  =  4,096 bytes
├── 1KB pool:    4 blocks  =  4,096 bytes
└── Total:                   23,552 bytes (~23 KB)
```

Alternative configurations:

**Minimal** (for 8KB RAM systems):
```
├── 16B:   16 blocks  =    256 bytes
├── 32B:   16 blocks  =    512 bytes
├── 64B:    8 blocks  =    512 bytes
├── 128B:   4 blocks  =    512 bytes
├── 256B:   2 blocks  =    512 bytes
├── 512B:   1 block   =    512 bytes
├── 1KB:    1 block   =  1,024 bytes
└── Total:              3,840 bytes (~3.8 KB)
```

**Generous** (for 64KB+ RAM systems):
```
├── 16B:  128 blocks  =  2,048 bytes
├── 32B:  128 blocks  =  4,096 bytes
├── 64B:   64 blocks  =  4,096 bytes
├── 128B:  32 blocks  =  4,096 bytes
├── 256B:  16 blocks  =  4,096 bytes
├── 512B:   8 blocks  =  4,096 bytes
├── 1KB:    4 blocks  =  4,096 bytes
└── Total:             28,672 bytes (~28 KB)
```

### 4. Stack Memory

Per-task stack requirements:

```
Stack Allocation (per task):
├── Minimum:        2 KB (cortex-m-rt default)
├── Standard:       4 KB (recommended)
├── Large:          8 KB (for complex operations)
└── Canary bytes:   16 bytes (overhead for overflow detection)
```

For a system with 4 concurrent tasks:
- Minimum: 4 × 2 KB = 8 KB
- Standard: 4 × 4 KB = 16 KB
- Large: 4 × 8 KB = 32 KB

### 5. Interrupt Vectors and ISR Memory

```
Cortex-M Interrupt Table:
├── Vector table:        Variable (chip-dependent)
│   ├── STM32F4:        ~400 bytes (100 vectors)
│   ├── nRF52:          ~160 bytes (40 vectors)
│   └── RP2040:         ~120 bytes (30 vectors)
├── ISR stack:          512 bytes - 2 KB
└── Event queue (opt):  1-2 KB
```

## Total Memory Budget Examples

### Minimal System (8KB RAM)
```
Memory Budget:
├── Static data:          3.1 KB
├── Pool memory:          3.8 KB  (minimal config)
├── Single task stack:    2.0 KB
├── ISR stack:            0.5 KB
├── Heap (optional):      0.5 KB
└── Total:               ~10 KB (exceeds 8KB - reduce features)
```

**For 8KB systems, disable:**
- Battery calibration (saves ~320 bytes)
- Detailed wake configurations (saves ~256 bytes)
- Use minimal pool config
- Single task only

**Optimized 8KB Configuration:**
```
├── Static data:          2.5 KB (reduced features)
├── Pool memory:          3.8 KB
├── Task stack:           1.5 KB
├── ISR stack:            0.2 KB
└── Total:               ~8.0 KB ✓
```

### Standard System (32KB RAM)
```
Memory Budget:
├── Static data:          3.1 KB
├── Pool memory:         23.5 KB  (default config)
├── Task stacks (4):     16.0 KB  (4 × 4 KB)
├── ISR stack:            1.0 KB
├── Heap:                 2.0 KB
├── Safety margin:        1.0 KB
└── Total:               46.6 KB (exceeds 32KB)
```

**Optimized for 32KB:**
```
├── Static data:          3.1 KB
├── Pool memory:         15.0 KB  (reduced pools)
├── Task stacks (3):      9.0 KB  (3 × 3 KB)
├── ISR stack:            1.0 KB
├── Heap:                 2.0 KB
├── Safety margin:        1.9 KB
└── Total:               32.0 KB ✓
```

### Large System (128KB+ RAM)
```
Memory Budget:
├── Static data:          3.1 KB
├── Pool memory:         28.7 KB  (generous config)
├── Task stacks (8):     32.0 KB  (8 × 4 KB)
├── ISR stack:            2.0 KB
├── Heap:                 8.0 KB
├── Application data:    40.0 KB
├── Safety margin:       14.2 KB
└── Total:              128.0 KB ✓
```

## Memory Alignment Requirements

### Cortex-M Specific
- **Stack alignment**: 8-byte alignment required
- **Vector table**: 256-byte alignment (128 for Cortex-M0/M0+)
- **Pool blocks**: Naturally aligned (power of 2 sizes)
- **Atomic operations**: 4-byte alignment for 32-bit atomics

### General Requirements
- **Structure packing**: Use `#[repr(C)]` for hardware interaction
- **DMA buffers**: Cache-line aligned (if cache present)
- **Pool blocks**: All blocks are power-of-2 aligned automatically

## Linker Script Considerations

Example memory regions for STM32F401 (64KB RAM, 256KB Flash):

```ld
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 256K
    RAM (rwx)   : ORIGIN = 0x20000000, LENGTH = 64K
}

SECTIONS
{
    /* Vector table at start of flash */
    .vector_table : {
        KEEP(*(.vector_table.reset_vector));
    } > FLASH

    /* Code in flash */
    .text : {
        *(.text .text.*);
    } > FLASH

    /* Static data in RAM */
    .data : {
        *(.data .data.*);
    } > RAM AT > FLASH

    /* BSS (zero-initialized) */
    .bss : {
        *(.bss .bss.*);
    } > RAM

    /* Memory pools (explicitly placed) */
    .pools (NOLOAD) : ALIGN(8) {
        *(.pools .pools.*);
        . = ALIGN(8);
    } > RAM

    /* Stack (grows downward) */
    .stack (NOLOAD) : ALIGN(8) {
        . = . + 4K;  /* Main stack */
        _stack_top = .;
    } > RAM

    /* Heap (optional, for alloc) */
    .heap (NOLOAD) : ALIGN(8) {
        _heap_start = .;
        . = . + 8K;  /* Heap size */
        _heap_end = .;
    } > RAM
}
```

## Configuration Guidelines

### Choosing Pool Configuration

1. **Analyze allocation patterns** in your application
2. **Profile actual usage** during development
3. **Start with standard config**, adjust based on failures

Example analysis:
```rust
// After running application
let stats = allocator.aggregate_stats();
println!("Block utilization: {}%", stats.block_utilization());
println!("Allocation failures: {}", stats.failure_count);

// Check per-pool stats
for i in 0..7 {
    let pool_stats = allocator.pool_stats(i).unwrap();
    println!("Pool {}: peak {}%, failures {}",
             i,
             pool_stats.peak_utilization_percent(),
             pool_stats.allocation_failures);
}
```

### Memory Safety Best Practices

1. **Enable stack canaries** for all tasks
2. **Check pool integrity** periodically in debug builds
3. **Monitor fragmentation** and defragment if needed
4. **Use safe_deallocate** during development
5. **Validate all allocations** before use

### Power Management Memory Impact

Power modes affect memory retention:

| Mode | RAM Retained | Wake Latency | Power Consumption |
|------|--------------|--------------|-------------------|
| Normal | ✓ | 0 μs | 100% |
| LowPower | ✓ | 1 μs | 50% |
| UltraLowPower | ✓ | 10 μs | 20% |
| Sleep | ✓ | 100 μs | 5% |
| Standby | ✓ | 1 ms | 1% |
| Shutdown | ✗ | 100 ms | 0% |

**Critical data** should be:
- Stored in backup SRAM (if available)
- Persisted to flash before shutdown
- Reloaded on wake from standby

## Optimization Techniques

### 1. Reduce Static Allocations
```rust
// Instead of full-featured manager
let manager = BatteryManager::new(config);  // ~600 bytes

// Use minimal tracking
let mut soc = 50u8;  // 1 byte
let mut voltage = 3700u32;  // 4 bytes
```

### 2. Share Resources
```rust
// Share memory pool between subsystems
static POOL: PoolAllocator = PoolAllocator::new(PoolConfig::minimal());

// Both power and battery systems use same pool
let power_alloc = POOL.allocate(128)?;
let battery_alloc = POOL.allocate(64)?;
```

### 3. Lazy Initialization
```rust
// Only initialize features when needed
let mut calibration: Option<BatteryCalibration> = None;

// Initialize only if calibration is required
if needs_precise_soc {
    calibration = Some(BatteryCalibration::new(chemistry, cells));
}
```

### 4. Use Const Generics
```rust
// Fixed-size alternatives to Vec
struct FixedVec<T, const N: usize> {
    data: [Option<T>; N],
    len: usize,
}
```

## Platform-Specific Considerations

### STM32F4 (Cortex-M4)
- **RAM**: 64-192 KB (depending on variant)
- **CCM RAM**: 64 KB (faster, but DMA-inaccessible)
- **Backup RAM**: 4 KB (retains in standby)

Recommendation: Use CCM for stacks, main RAM for pools

### nRF52 (Cortex-M4F)
- **RAM**: 32-256 KB (depending on variant)
- **Retention**: Configurable RAM banks in system-off

Recommendation: Configure retention for critical data only

### RP2040 (Cortex-M0+)
- **RAM**: 264 KB (SRAM banks 0-5)
- **No CCM**: All RAM is general purpose
- **USB**: Requires dedicated buffer space (4 KB)

Recommendation: Reserve 4 KB for USB, use rest flexibly

## Monitoring and Debugging

### Stack Usage
```rust
let stack_stats = stack_manager.get_stats(task_id);
println!("Stack usage: {} / {} bytes",
         stack_stats.used_bytes,
         stack_stats.total_bytes);

if stack_stats.status == StackStatus::High {
    println!("Warning: Stack usage high!");
}
```

### Pool Usage
```rust
let available = allocator.available_memory();
let integrity = allocator.check_integrity();

if available < 1024 {
    println!("Warning: Less than 1KB available");
}

if integrity.is_err() {
    println!("Error: Pool corruption detected!");
}
```

### Memory Map at Runtime
```rust
extern "C" {
    static _stack_start: u32;
    static _stack_end: u32;
    static _heap_start: u32;
    static _heap_end: u32;
}

unsafe {
    println!("Stack: 0x{:08x} - 0x{:08x}",
             &_stack_start as *const _ as usize,
             &_stack_end as *const _ as usize);
}
```

## Summary

Key takeaways:
1. **Minimum viable system**: ~8 KB RAM with reduced features
2. **Recommended system**: 32 KB RAM for full features
3. **Comfortable system**: 64 KB+ RAM with generous margins
4. **Always measure**: Profile your specific application
5. **Safety first**: Enable all safety checks during development

---

**Last Updated**: 2026-01-18
**Version**: 0.1.0
