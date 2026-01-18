# Troubleshooting Guide

This guide helps you diagnose and fix common issues when working with MielinOS kernel.

## Table of Contents

1. [Build Issues](#build-issues)
2. [Runtime Errors](#runtime-errors)
3. [Memory Problems](#memory-problems)
4. [Scheduler Issues](#scheduler-issues)
5. [Interrupt Problems](#interrupt-problems)
6. [Multi-Core Issues](#multi-core-issues)
7. [Platform-Specific](#platform-specific)
8. [Performance Problems](#performance-problems)
9. [Testing Issues](#testing-issues)
10. [Getting Help](#getting-help)

## Build Issues

### Error: "bootloader requires nightly Rust"

**Problem**: The bootloader feature requires nightly features.

**Solution**:
```bash
# Use stable Rust without bootloader
cargo build --lib --no-default-features

# Or switch to nightly for bootloader support
rustup default nightly
cargo build --features bootable
```

### Error: "can't find crate for `std`"

**Problem**: Building for no_std target but std is being pulled in.

**Solution**:
```bash
# Ensure you're using the lib target
cargo build --lib

# Check for std dependencies in Cargo.toml
# Remove any dependencies that require std
```

### Clippy Warnings About Division

**Problem**: `clippy::integer_division` or `clippy::integer_arithmetic` warnings.

**Solution**:
```rust
// Use checked arithmetic
let result = numerator.checked_div(denominator)
    .ok_or(KernelError::DivisionByZero)?;

// Or use saturating division
let result = numerator.saturating_div(denominator);
```

### Linker Errors on Cross-Compilation

**Problem**: Cannot find linker for target architecture.

**Solution**:
```bash
# Install target
rustup target add <your-target>

# Install appropriate linker
# For ARM:
sudo apt install gcc-aarch64-linux-gnu

# For RISC-V:
sudo apt install gcc-riscv64-unknown-elf
```

## Runtime Errors

### Panic: "null pointer dereference"

**Problem**: Attempting to dereference a null or uninitialized pointer.

**Common Causes**:
1. Allocator not initialized
2. Page table not set up
3. Invalid physical address

**Solution**:
```rust
// Always initialize before use
memory::init()?;
scheduler::init()?;

// Check pointers before dereferencing
if ptr.is_null() {
    return Err(KernelError::NullPointer);
}

// SAFETY: Document why this is safe
unsafe {
    *ptr = value;
}
```

### Panic: "out of memory"

**Problem**: Memory allocator has no free pages.

**Diagnosis**:
```rust
// Check memory statistics
let stats = memory::get_stats();
println!("Free pages: {}", stats.free_pages);
println!("Allocated pages: {}", stats.allocated_pages);
```

**Solutions**:
1. Increase MAX_PAGES in configuration
2. Free unused pages
3. Check for memory leaks
4. Implement page reclamation

### Illegal Instruction (SIGILL)

**Problem**: Executing privileged instruction in user mode.

**Common in Tests**: `hlt` or `wfi` instructions.

**Solution**:
```rust
// Guard privileged instructions
#[cfg(not(any(test, feature = "std")))]
unsafe {
    core::arch::asm!("hlt");
}

#[cfg(any(test, feature = "std"))]
{
    core::hint::spin_loop(); // Use safe alternative in tests
}
```

## Memory Problems

### Double Free Detected

**Problem**: Attempting to free the same memory twice.

**Diagnosis**:
```rust
// Enable debug assertions
#[cfg(debug_assertions)]
{
    if !is_allocated(addr) {
        panic!("Double free detected at {:#x}", addr);
    }
}
```

**Solutions**:
1. Track ownership with Rust's type system
2. Use `Option<T>` to represent "may be freed"
3. Enable allocator debugging

### Memory Leak

**Problem**: Allocated memory never freed.

**Diagnosis**:
```bash
# Run with leak detector (requires nightly)
RUSTFLAGS="-Z sanitizer=address" cargo +nightly test

# Check memory statistics over time
let before = memory::get_stats();
// ... run workload ...
let after = memory::get_stats();
println!("Leaked: {} pages", after.allocated_pages - before.allocated_pages);
```

**Solutions**:
1. Use RAII (Resource Acquisition Is Initialization)
2. Implement `Drop` for cleanup
3. Use smart pointers (`Box`, `Arc`, etc.)
4. Review `unsafe` code for leaks

### Page Alignment Error

**Problem**: Address not aligned to page boundary.

**Solution**:
```rust
// Always align addresses
const PAGE_SIZE: usize = 4096;

fn align_down(addr: usize) -> usize {
    addr & !(PAGE_SIZE - 1)
}

fn align_up(addr: usize) -> usize {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

// Validate alignment before use
if virt_addr % PAGE_SIZE != 0 {
    return Err(VmmError::InvalidVirtualAddress);
}
```

## Scheduler Issues

### Task Not Being Scheduled

**Problem**: Task remains in Ready state but never runs.

**Diagnosis**:
```rust
// Check task state
if let Some(task) = scheduler.get_task(task_id) {
    println!("State: {:?}", task.state());
    println!("Priority: {}", task.priority());
}

// Check scheduler metrics
let metrics = scheduler::metrics().unwrap();
println!("Schedule calls: {}", metrics.schedule_calls);
println!("Idle schedules: {}", metrics.schedule_idle);
```

**Common Causes**:
1. Higher priority tasks are always ready
2. Task is blocked
3. CPU affinity mismatch
4. Task was terminated

**Solutions**:
1. Lower priority of higher-priority tasks
2. Wake blocked tasks
3. Check CPU affinity settings
4. Verify task is not terminated

### Priority Inversion

**Problem**: High-priority task blocked by low-priority task.

**Solution**: Use RT mutexes with priority inheritance:
```rust
use mielin_kernel::rt::RtMutex;

let mutex = RtMutex::new(0);

// Low-priority task locks mutex
mutex.lock(low_priority_task)?;

// High-priority task blocks on mutex
// -> Low-priority task's priority is automatically boosted
mutex.lock(high_priority_task)?;
```

### Deadlock

**Problem**: Tasks waiting for each other indefinitely.

**Diagnosis**:
```rust
// Check for circular dependencies
// Task A waits for Task B
// Task B waits for Task A

// Use timeout-based locking
match mutex.try_lock_timeout(task_id, Duration::from_millis(100)) {
    Ok(_) => { /* acquired */ },
    Err(_) => { /* timeout - potential deadlock */ }
}
```

**Solutions**:
1. Always acquire locks in the same order
2. Use priority ceiling protocol
3. Use lock-free data structures
4. Implement deadlock detection

## Interrupt Problems

### Interrupts Not Firing

**Problem**: Timer or device interrupts not occurring.

**Diagnosis**:
```rust
// Check if interrupts are enabled
let enabled = interrupt::are_enabled();
println!("Interrupts enabled: {}", enabled);

// Check IRQ status
let stats = interrupt::get_stats();
println!("Total interrupts: {}", stats.total_interrupts);
```

**Solutions**:
```rust
// Enable interrupts globally
interrupt::enable();

// Enable specific IRQ
interrupt::enable_irq(IRQ_TIMER);

// Check hardware configuration (platform-specific)
```

### Interrupt Storm

**Problem**: Too many interrupts, system unresponsive.

**Diagnosis**:
```rust
let stats = interrupt::get_stats();
if stats.total_interrupts > 1_000_000 {
    println!("Interrupt storm detected!");
}
```

**Solutions**:
1. Disable misbehaving IRQ
2. Implement interrupt coalescing
3. Fix device driver
4. Use interrupt throttling

### Nested Interrupt Overflow

**Problem**: Stack overflow from too many nested interrupts.

**Solution**:
```rust
// Limit interrupt nesting depth
const MAX_NEST_LEVEL: usize = 8;

if current_nest_level >= MAX_NEST_LEVEL {
    // Defer to bottom-half handler
    schedule_work(handler);
    return;
}
```

## Multi-Core Issues

### Load Imbalance

**Problem**: One CPU overloaded while others are idle.

**Diagnosis**:
```rust
use mielin_kernel::percpu;

let score = percpu::load_imbalance_score();
println!("Load imbalance: {}", score);

// Check per-CPU stats
for cpu in 0..percpu::num_cpus() {
    let stats = percpu::cpu_stats(cpu);
    println!("CPU {}: {} tasks", cpu, stats.task_count);
}
```

**Solution**:
```rust
// Enable load balancing
percpu::balance_load(LoadBalanceStrategy::WorkStealing);
```

### Cache Coherency Issues

**Problem**: Stale data seen by other CPUs.

**Solution**:
```rust
use core::sync::atomic::Ordering;

// Use appropriate memory ordering
shared_data.store(value, Ordering::Release); // Writer
let value = shared_data.load(Ordering::Acquire); // Reader

// Use memory barriers
atomic::fence(Ordering::SeqCst);
```

### IPI Not Received

**Problem**: Inter-processor interrupt not delivered.

**Diagnosis**:
```rust
use mielin_kernel::ipc;

let stats = ipc::get_ipi_stats(cpu);
println!("IPIs sent: {}", stats.sent);
println!("IPIs received: {}", stats.received);
println!("IPIs pending: {}", stats.pending);
```

**Solution**:
```rust
// Ensure IPI handler is registered
ipc::register_ipi_handler(IpiType::Reschedule, handler);

// Check if IPI is masked
ipc::unmask_ipi(IpiType::Reschedule);
```

## Platform-Specific

### QEMU: Kernel doesn't boot

**Problem**: Blank screen or immediate exit.

**Solutions**:
```bash
# Enable debug output
qemu-system-x86_64 \
    -kernel kernel.elf \
    -nographic \
    -serial mon:stdio \
    -d int,cpu_reset

# Check if kernel format is correct
file kernel.elf

# Try different machine types
qemu-system-x86_64 -M q35 -kernel kernel.elf
```

### Raspberry Pi: No serial output

**Problem**: UART not working on real hardware.

**Solutions**:
1. Enable UART in config.txt
2. Check UART GPIO pins (14/15 for Pi 4)
3. Verify baud rate (115200)
4. Use USB-to-serial adapter correctly

### x86_64: Triple fault

**Problem**: CPU triple faults and reboots.

**Common Causes**:
1. Invalid IDT (Interrupt Descriptor Table)
2. Stack overflow
3. Invalid page tables
4. Unhandled exception

**Diagnosis**:
```bash
# Run with QEMU debug
qemu-system-x86_64 -d int,cpu_reset -no-reboot -kernel kernel.elf
```

## Performance Problems

### Slow Page Allocation

**Problem**: Page allocation takes too long.

**Diagnosis**:
```rust
use std::time::Instant;

let start = Instant::now();
let addr = memory::allocate_pages(1)?;
let duration = start.elapsed();
println!("Allocation took: {:?}", duration);
```

**Solutions**:
1. Use pool allocator for common sizes
2. Pre-allocate pages
3. Reduce fragmentation
4. Use better allocation strategy

### High Context Switch Overhead

**Problem**: Task switching is slow.

**Diagnosis**:
```rust
let metrics = scheduler::metrics().unwrap();
let ctx_switch_rate = metrics.schedule_calls as f64 / uptime_seconds;
println!("Context switches/sec: {}", ctx_switch_rate);
```

**Solutions**:
1. Reduce number of tasks
2. Increase time quantum
3. Use cooperative scheduling
4. Minimize state to save/restore

### Cache Misses

**Problem**: Poor cache utilization.

**Solutions**:
```rust
// Align hot structures to cache line
#[repr(align(64))]
struct HotData {
    // Frequently accessed fields
}

// Pack related data together
#[repr(C)]
struct CacheFriendly {
    field1: u32,
    field2: u32,  // Accessed together with field1
}
```

## Testing Issues

### Test Fails Only in Release Mode

**Problem**: Test passes in debug but fails in release.

**Common Causes**:
1. Undefined behavior optimized incorrectly
2. Race condition (timing-dependent)
3. Debug assertions not present in release

**Solutions**:
```bash
# Run with optimizations but debug assertions
cargo test --profile release-debug

# Use MIRI to detect UB
cargo +nightly miri test

# Add explicit checks
assert!(condition, "This should always be true");
```

### Flaky Test

**Problem**: Test passes/fails inconsistently.

**Common Causes**:
1. Race conditions
2. Uninitialized memory
3. Test ordering dependencies

**Solutions**:
```rust
// Run tests in order
cargo test -- --test-threads=1

// Add synchronization
use std::sync::Barrier;
let barrier = Barrier::new(2);

// Reset state between tests
#[cfg(test)]
fn reset_state() {
    memory::reset();
    scheduler::reset();
}
```

## Getting Help

### Before Asking for Help

1. **Search existing issues**: https://github.com/cool-japan/mielin-kernel/issues
2. **Check documentation**: Read the relevant docs in `docs/`
3. **Enable debug logging**: Set `RUST_LOG=debug`
4. **Try the latest version**: Update to main branch
5. **Create minimal reproducer**: Isolate the problem

### When Asking for Help

Include the following information:

1. **MielinOS version**: `git describe --tags`
2. **Rust version**: `rustc --version`
3. **Platform**: QEMU, real hardware, architecture
4. **Error message**: Full error output
5. **Minimal reproducer**: Code that demonstrates the issue
6. **What you've tried**: Steps you've already taken

### Where to Get Help

- **GitHub Issues**: For bugs and feature requests
- **Discussions**: For questions and general discussion
- **Matrix/Discord**: Real-time chat (if available)
- **Email**: For private inquiries

### Emergency Debug Macros

```rust
// Quick debug printing (std builds only)
#[cfg(feature = "std")]
macro_rules! dbg_print {
    ($($arg:tt)*) => {{
        eprintln!("[{}:{}] {}", file!(), line!(), format_args!($($arg)*));
    }};
}

// Trace function entry/exit
#[cfg(feature = "std")]
macro_rules! trace {
    ($fn_name:expr) => {{
        eprintln!("-> {}", $fn_name);
        scopeguard::defer! {
            eprintln!("<- {}", $fn_name);
        }
    }};
}
```

## Useful Commands

```bash
# Run tests with verbose output
cargo test -- --nocapture

# Run specific test
cargo test test_name -- --exact

# Run tests on specific architecture
cargo test --target x86_64-unknown-linux-gnu

# Check for memory leaks (requires nightly)
RUSTFLAGS="-Z sanitizer=address" cargo +nightly test

# Profile performance
cargo bench
perf record -g cargo test
perf report

# Check code coverage
cargo tarpaulin --out Html

# Find slow tests
cargo test -- --show-output --test-threads=1 | grep "test result"
```

## Common Error Messages

| Error | Cause | Solution |
|-------|-------|----------|
| `out of memory` | Allocator exhausted | Increase MAX_PAGES or free memory |
| `task not found` | Invalid task ID | Check task was spawned successfully |
| `page not mapped` | Accessing unmapped virtual memory | Map the page first with vmm::map() |
| `already mapped` | Trying to remap virtual address | Unmap first or use different address |
| `invalid priority` | Priority out of range | Use priority 0-255 |
| `cpu not found` | Invalid CPU ID | Check CPU count with percpu::num_cpus() |

---

**Still stuck? Create an issue with full details and we'll help!** 🔧
