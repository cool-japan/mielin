# Performance Tuning Guide

This guide helps you optimize MielinOS Kernel performance for your specific use case.

## Table of Contents

- [Quick Start](#quick-start)
- [Configuration Profiles](#configuration-profiles)
- [Memory Optimization](#memory-optimization)
- [Scheduler Optimization](#scheduler-optimization)
- [Multi-Core Optimization](#multi-core-optimization)
- [Benchmarking](#benchmarking)
- [Platform-Specific Tuning](#platform-specific-tuning)

## Quick Start

### 1. Choose a Configuration Profile

Start with a pre-configured profile matching your use case:

```rust
use mielin_kernel::config::KernelConfig;

// For embedded systems (minimal resources)
let config = KernelConfig::embedded();

// For high-performance systems (maximum throughput)
let config = KernelConfig::high_performance();

// For ARM64 with 16KB pages
let config = KernelConfig::arm64_16kb();
```

### 2. Measure Baseline Performance

Run benchmarks to establish baseline:

```bash
cargo bench
```

### 3. Tune Incrementally

Change one parameter at a time and measure impact.

## Configuration Profiles

### Embedded Profile

**Use Case**: Resource-constrained devices (IoT, microcontrollers)

**Characteristics**:
- Total memory: 512KB (128 pages × 4KB)
- Max tasks: 16
- CPUs: 1 (single-core)
- Metrics: Disabled (saves memory)

**When to Use**:
- RAM < 1MB
- Single-core processors
- Real-time constraints
- Battery-powered devices

```rust
let config = KernelConfig::embedded();
```

### High-Performance Profile

**Use Case**: Server and desktop systems

**Characteristics**:
- Total memory: 32MB (8192 pages × 4KB)
- Max tasks: 256
- CPUs: 32 (multi-core)
- All features enabled

**When to Use**:
- RAM > 512MB
- Multi-core processors (4+ cores)
- Throughput-critical applications
- Development and testing

```rust
let config = KernelConfig::high_performance();
```

### ARM64 16KB Page Profile

**Use Case**: Apple Silicon (M1/M2), some ARM64 servers

**Characteristics**:
- Page size: 16KB
- Total memory: 16MB (1024 pages × 16KB)
- Standard task/CPU limits

**When to Use**:
- macOS on Apple Silicon
- ARM64 systems with 16KB pages
- Better memory efficiency on ARM64

```rust
let config = KernelConfig::arm64_16kb();
```

## Memory Optimization

### Page Size Selection

**Impact**: 10-30% performance difference

Choose based on your workload:

| Page Size | Use Case | Pros | Cons |
|-----------|----------|------|------|
| 4KB | Default, x86_64 | Fine-grained, standard | More page table overhead |
| 8KB | Embedded systems | Balance of size/overhead | Less common |
| 16KB | ARM64, macOS | Better TLB efficiency | Wastes memory on small allocs |
| 64KB | Large-memory systems | Minimal overhead | Significant internal fragmentation |

```rust
use mielin_kernel::config::PageSize;

let config = KernelConfig::builder()
    .page_size(PageSize::Size16KB)
    .build()
    .unwrap();
```

### Memory Pool Configuration

**Impact**: 5-15% allocation performance

```rust
let config = KernelConfig::builder()
    .max_pages(2048)  // Increase for more memory
    .build()
    .unwrap();
```

**Guidelines**:
- Embedded: 128-512 pages (512KB-2MB)
- Desktop: 1024-4096 pages (4MB-16MB)
- Server: 8192+ pages (32MB+)

### Allocation Strategy

**Impact**: 5-20% on fragmented workloads

Choose strategy based on allocation patterns:

```rust
use mielin_kernel::memory::{set_strategy, AllocationStrategy};

// First-fit: Fast, good for sequential allocations
set_strategy(AllocationStrategy::FirstFit);

// Best-fit: Reduces fragmentation, slower
set_strategy(AllocationStrategy::BestFit);

// Worst-fit: Good for varying sizes
set_strategy(AllocationStrategy::WorstFit);
```

**Recommendations**:
- **FirstFit**: Default, good general-purpose choice
- **BestFit**: Use when memory is limited and fragmentation is a concern
- **WorstFit**: Use when allocation sizes vary significantly

### Coalescing

**Impact**: Reduces fragmentation by 30-60%

Enable automatic coalescing (enabled by default):

```rust
let config = KernelConfig::builder()
    .memory(MemoryConfig {
        enable_coalescing: true,
        ..Default::default()
    })
    .build()
    .unwrap();
```

Manual defragmentation when needed:

```rust
use mielin_kernel::memory::defragment;

// Periodically defragment
let blocks_merged = defragment().unwrap_or(0);
```

## Scheduler Optimization

### Task Limit Tuning

**Impact**: 10-20% scheduler overhead

```rust
let config = KernelConfig::builder()
    .max_tasks(128)  // Increase for more concurrent tasks
    .build()
    .unwrap();
```

**Guidelines**:
- Embedded: 8-32 tasks
- Desktop: 64-128 tasks
- Server: 128-512 tasks

**Trade-offs**:
- More tasks = higher memory usage (~64 bytes per task slot)
- More tasks = slower O(n) scheduling scans
- Fewer tasks = risk of task exhaustion

### Priority Scheduling

**Impact**: Minimal overhead, better QoS

Enable priority scheduling (enabled by default):

```rust
let config = KernelConfig::builder()
    .scheduler(SchedulerConfig {
        enable_priority: true,
        ..Default::default()
    })
    .build()
    .unwrap();
```

**Priority Guidelines**:
- 0-63: Background tasks
- 64-127: Normal tasks
- 128-191: High-priority tasks
- 192-255: Critical/real-time tasks

### Metrics Collection

**Impact**: 2-5% overhead when enabled

Disable for production if telemetry not needed:

```rust
let config = KernelConfig::builder()
    .scheduler(SchedulerConfig {
        enable_metrics: false,  // Disable for production
        ..Default::default()
    })
    .build()
    .unwrap();
```

**When to Disable**:
- Embedded systems with tight memory constraints
- Maximum performance requirements
- Production deployments without monitoring

**When to Keep Enabled**:
- Development and debugging
- Performance profiling
- Systems with monitoring infrastructure

## Multi-Core Optimization

### CPU Count Configuration

**Impact**: Linear scaling up to physical core count

```rust
let config = KernelConfig::builder()
    .max_cpus(8)  // Match physical core count
    .build()
    .unwrap();
```

**Guidelines**:
- Set to physical core count (not hyperthreads)
- Embedded: 1-2 CPUs
- Desktop: 4-8 CPUs
- Server: 16-32 CPUs

### Per-CPU Memory Pools

**Impact**: 20-40% reduction in lock contention

Enable for multi-core systems (enabled by default):

```rust
let config = KernelConfig::builder()
    .multicore(MultiCoreConfig {
        enable_per_cpu_pools: true,
        ..Default::default()
    })
    .build()
    .unwrap();
```

**Benefits**:
- Eliminates lock contention for allocations
- Better cache locality
- Linear scaling with CPU count

**Trade-offs**:
- Higher memory usage (7 pools × CPU count)
- Potential load imbalance

### Load Balancing

**Impact**: 10-30% better CPU utilization

```rust
let config = KernelConfig::builder()
    .multicore(MultiCoreConfig {
        enable_load_balancing: true,
        enable_work_stealing: true,
        ..Default::default()
    })
    .build()
    .unwrap();
```

**Strategies**:
- **Work Stealing**: Idle CPUs steal tasks from busy ones (default)
- **Push Migration**: Busy CPUs push to idle ones
- **Round-Robin**: Distribute tasks evenly

**When to Disable**:
- Single-core systems
- Real-time requirements (deterministic scheduling)
- CPU isolation scenarios

### CPU Affinity

**Impact**: Improves cache locality by 15-25%

Pin tasks to specific CPUs:

```rust
use mielin_kernel::scheduler::spawn_task_with_affinity;

// Pin to CPU 2
let task_id = spawn_task_with_affinity(priority, Some(2));

// Free-floating (can migrate)
let task_id = spawn_task_with_affinity(priority, None);
```

**When to Use Pinning**:
- Cache-sensitive workloads
- NUMA systems
- Real-time tasks
- Interrupt handling

## Benchmarking

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench allocation

# Save baseline for comparison
cargo bench -- --save-baseline before_optimization

# Compare with baseline
cargo bench -- --baseline before_optimization
```

### Benchmark Targets (v0.1.0)

| Operation | Target | Typical | Notes |
|-----------|--------|---------|-------|
| Page allocation | <20ns | 10-15ns | O(1) with free list |
| Task spawn | <100ns | 60-80ns | Pre-allocated pool |
| Task switch | <50ns | 30-40ns | Cooperative yield |
| Schedule decision | <100ns | 50-70ns | O(n) scan |

### Profiling

Use `perf` on Linux:

```bash
# Profile allocations
perf record --call-graph=dwarf cargo bench allocation
perf report

# Check cache misses
perf stat -e cache-references,cache-misses cargo bench
```

## Platform-Specific Tuning

### x86_64 (Intel/AMD)

```rust
let config = KernelConfig::builder()
    .page_size(PageSize::Size4KB)  // Standard x86_64
    .max_cpus(16)  // Typical desktop/server
    .build()
    .unwrap();
```

**Optimizations**:
- Use 4KB pages (hardware default)
- Enable all features (sufficient resources)
- Consider huge pages for large allocations (future)

### ARM64 (Cortex-A, Apple Silicon)

```rust
let config = KernelConfig::arm64_16kb();
```

**Optimizations**:
- Use 16KB pages on M1/M2 (matches hardware)
- Consider 64KB pages on servers
- Leverage efficient atomics
- Use cache-line alignment (64 bytes)

### RISC-V

```rust
let config = KernelConfig::builder()
    .page_size(PageSize::Size4KB)  // Common RISC-V
    .max_cpus(4)  // Typical embedded count
    .build()
    .unwrap();
```

**Optimizations**:
- 4KB pages most common
- May support 8KB/16KB on some implementations
- Optimize for instruction cache (smaller code)

### Cortex-M (Embedded ARM)

```rust
let config = KernelConfig::embedded();
```

**Optimizations**:
- Minimize memory footprint
- Disable metrics collection
- Single-core configuration
- Consider custom pool sizes for specific chips

## Compilation Flags

### Release Profiles

Use appropriate release profile:

```toml
# In Cargo.toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

Profiles:
- `release`: Standard optimized build
- `release-speed`: Maximum runtime performance
- `release-embedded`: Minimal code size
- `release-debug`: Optimized with debug symbols

### Build Command

```bash
# Maximum performance
RUSTFLAGS="-C target-cpu=native" cargo build --release --profile release-speed

# Embedded (size-optimized)
cargo build --release --profile release-embedded

# With LTO
cargo build --release
```

## Monitoring Performance

### Memory Statistics

```rust
use mielin_kernel::memory::summary;

let stats = summary().unwrap();
println!("Allocated: {} / {}", stats.allocated_pages, stats.total_pages);
println!("Fragmentation: {}", stats.fragmentation_score);
```

### Scheduler Metrics

```rust
use mielin_kernel::scheduler::metrics;

let metrics = metrics();
println!("Utilization: {:.2}%", metrics.utilization() * 100.0);
println!("Churn rate: {:.2}", metrics.task_churn_rate());
```

## Common Performance Issues

### High Allocation Latency

**Symptoms**: Slow page allocations

**Solutions**:
1. Check fragmentation score
2. Run manual defragmentation
3. Increase free list efficiency
4. Consider larger page size

### Poor Scheduler Throughput

**Symptoms**: Low CPU utilization, high schedule overhead

**Solutions**:
1. Reduce MAX_TASKS if excessive
2. Enable priority scheduling
3. Use load balancing on multi-core
4. Check for priority inversion

### Lock Contention

**Symptoms**: Poor multi-core scaling

**Solutions**:
1. Enable per-CPU pools
2. Use work stealing load balancer
3. Pin cache-sensitive tasks
4. Check cache-line alignment

## Best Practices

1. **Start Simple**: Use default or profile configurations first
2. **Measure First**: Benchmark before optimizing
3. **One Change at a Time**: Isolate performance impacts
4. **Profile Workload**: Match configuration to actual usage
5. **Test on Target**: Benchmark on deployment hardware
6. **Monitor Production**: Keep metrics enabled initially
7. **Document Changes**: Track configuration decisions

## Further Reading

- [MielinOS Architecture](docs/architecture/)
- [Memory Allocator Design](docs/adr/002-memory-allocator.md)
- [Scheduler Design](docs/adr/003-scheduler-design.md)
- [Multi-Core Strategy](docs/adr/004-multicore-strategy.md)

---

**Last Updated**: 2026-01-18
**Maintainer**: COOLJAPAN OU (Team Kitasan)
