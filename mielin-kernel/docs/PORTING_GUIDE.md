# MielinOS Kernel Porting Guide

This guide explains how to port the MielinOS kernel to new hardware architectures and platforms.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Supported Architectures](#supported-architectures)
3. [Porting Checklist](#porting-checklist)
4. [Architecture-Specific Modules](#architecture-specific-modules)
5. [Testing Your Port](#testing-your-port)
6. [Performance Optimization](#performance-optimization)
7. [Submitting Your Port](#submitting-your-port)

## Architecture Overview

MielinOS is designed to be portable across multiple architectures through:

1. **Architecture-agnostic core**: Most kernel code is platform-independent
2. **Conditional compilation**: `#[cfg(target_arch = "...")]` for platform-specific code
3. **Hardware abstraction**: Clear separation between generic and platform-specific code
4. **Minimal assumptions**: No hardcoded addresses, page sizes configurable

## Supported Architectures

### Currently Supported

- **x86_64** (64-bit Intel/AMD)
  - 4-level paging (PML4 → PDPT → PD → PT)
  - APIC/IOAPIC interrupts
  - RDTSC for timestamps
  - MSR for power management

- **AArch64** (64-bit ARM)
  - 4-level paging with ASID
  - Generic timer
  - WFI for power saving
  - DVFS support

- **RISC-V 64** (RV64)
  - Sv39/Sv48 paging (planned)
  - PLIC for interrupts
  - Timer interrupts

- **ARM Cortex-M** (embedded)
  - No MMU (planned MPU support)
  - NVIC for interrupts
  - SysTick timer

## Porting Checklist

### Phase 1: Basic Boot

- [ ] Define target triple in `.cargo/config.toml`
- [ ] Create linker script (`link.ld`) with memory layout
- [ ] Implement panic handler (`src/arch/<arch>/panic.rs`)
- [ ] Set up stack pointer initialization
- [ ] Implement early console output (UART/serial)
- [ ] Boot to kernel entry point

### Phase 2: Memory Management

- [ ] Define page size (4KB, 8KB, 16KB, 64KB)
- [ ] Implement physical memory detection
- [ ] Set up page table format
- [ ] Implement virtual memory mapping
- [ ] Add TLB invalidation functions
- [ ] Configure memory-mapped I/O regions

### Phase 3: Interrupt Handling

- [ ] Define interrupt controller type (APIC, GIC, PLIC, NVIC)
- [ ] Implement IRQ enable/disable
- [ ] Set up interrupt vector table
- [ ] Implement timer interrupts
- [ ] Add IPI support (for multi-core)
- [ ] Test interrupt latency

### Phase 4: Scheduling & Synchronization

- [ ] Implement context switching
- [ ] Add atomic operations support
- [ ] Implement spinlocks
- [ ] Test multi-core scheduling (if applicable)
- [ ] Verify CPU affinity works

### Phase 5: Power Management

- [ ] Implement CPU idle states (HLT, WFI, WFE, etc.)
- [ ] Add frequency scaling (if available)
- [ ] Implement voltage scaling (if available)
- [ ] Test power consumption

## Architecture-Specific Modules

### 1. Boot Module (`src/arch/<arch>/boot.rs`)

Responsibilities:
- Early hardware initialization
- Set up stack and heap
- Initialize BSS section
- Jump to Rust entry point

Example for RISC-V:
```rust
// src/arch/riscv64/boot.rs
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    // Clear BSS
    extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }
    let bss_start = &mut __bss_start as *mut u8;
    let bss_end = &mut __bss_end as *mut u8;
    let bss_size = bss_end as usize - bss_start as usize;
    core::ptr::write_bytes(bss_start, 0, bss_size);

    // Set up stack pointer
    asm!("la sp, _stack_top");

    // Jump to kernel main
    kernel_main();
}
```

### 2. MMU Module (`src/arch/<arch>/mmu.rs`)

Responsibilities:
- Page table format definition
- Virtual to physical translation
- TLB management
- Cache control

Example page table entry:
```rust
// Architecture-specific page table entry format
#[derive(Clone, Copy)]
pub struct PageTableEntry {
    entry: u64,
}

impl PageTableEntry {
    pub fn new(phys_addr: usize, flags: PageTableFlags) -> Self {
        let mut entry = phys_addr as u64 & PHYS_ADDR_MASK;
        entry |= flags.bits();
        Self { entry }
    }

    pub fn is_present(&self) -> bool {
        self.entry & (1 << 0) != 0
    }

    pub fn phys_addr(&self) -> usize {
        (self.entry & PHYS_ADDR_MASK) as usize
    }
}
```

### 3. Interrupt Module (`src/arch/<arch>/interrupts.rs`)

Responsibilities:
- Interrupt controller initialization
- IRQ routing
- Exception handling
- IPI generation (for SMP)

Example for ARM GIC:
```rust
// src/arch/aarch64/interrupts.rs
pub fn enable_irq(irq: u32) {
    let gicd_base = 0x0800_0000; // Platform-specific
    unsafe {
        let reg = (gicd_base + 0x100 + (irq / 32) * 4) as *mut u32;
        reg.write_volatile(reg.read_volatile() | (1 << (irq % 32)));
    }
}

pub fn send_ipi(target_cpu: usize, ipi_type: u32) {
    let gicd_base = 0x0800_0000;
    let sgir = (gicd_base + 0xF00) as *mut u32;
    unsafe {
        sgir.write_volatile((target_cpu as u32) << 16 | ipi_type);
    }
}
```

### 4. Timer Module (`src/arch/<arch>/timer.rs`)

Responsibilities:
- Timer initialization
- Tick generation
- Timestamp reading
- Sleep/delay functions

Example for ARM Generic Timer:
```rust
// src/arch/aarch64/timer.rs
pub fn read_timestamp() -> u64 {
    let mut cnt: u64;
    unsafe {
        asm!("mrs {}, cntvct_el0", out(reg) cnt);
    }
    cnt
}

pub fn setup_timer(tick_rate_hz: u32) {
    let freq = read_cntfrq();
    let ticks_per_interrupt = freq / tick_rate_hz;

    unsafe {
        asm!("msr cntv_tval_el0, {}", in(reg) ticks_per_interrupt);
        asm!("msr cntv_ctl_el0, {}", in(reg) 1u64); // Enable
    }
}
```

### 5. Power Management (`src/arch/<arch>/power.rs`)

Responsibilities:
- CPU idle instructions
- Frequency/voltage scaling
- Power state transitions

Example:
```rust
// src/arch/aarch64/power.rs
pub fn cpu_idle() {
    unsafe {
        asm!("wfi"); // Wait For Interrupt
    }
}

pub fn set_cpu_frequency(freq_mhz: u32) -> Result<(), PowerError> {
    // Platform-specific DVFS implementation
    // May involve SCMI, PSCI, or direct register access
    Ok(())
}
```

## Platform Configuration

### Memory Layout

Define in `link.ld`:
```ld
MEMORY {
    /* Adjust for your platform */
    RAM : ORIGIN = 0x80000000, LENGTH = 128M
    FLASH : ORIGIN = 0x00000000, LENGTH = 16M
}

SECTIONS {
    .text : { *(.text .text.*) } > RAM
    .rodata : { *(.rodata .rodata.*) } > RAM
    .data : { *(.data .data.*) } > RAM
    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        __bss_end = .;
    } > RAM
}
```

### Cargo Configuration

`.cargo/config.toml`:
```toml
[build]
target = "riscv64gc-unknown-none-elf"

[target.riscv64gc-unknown-none-elf]
rustflags = ["-C", "link-arg=-Tlink.ld"]
```

## Testing Your Port

### 1. Unit Tests

Run architecture-specific tests:
```bash
cargo test --target <your-target> --lib
```

### 2. Integration Tests

Create platform-specific tests:
```rust
#[cfg(all(test, target_arch = "riscv64"))]
mod riscv64_tests {
    #[test]
    fn test_page_table_format() {
        // Verify page table entry format
    }

    #[test]
    fn test_timer_accuracy() {
        // Verify timer precision
    }
}
```

### 3. Hardware Testing

1. **QEMU Testing**: Start with emulator
   ```bash
   qemu-system-riscv64 -M virt -kernel kernel.elf -nographic
   ```

2. **Real Hardware**: Flash to device and test
   - Serial console output
   - Timer interrupts
   - Memory allocation
   - Task scheduling

### 4. Performance Testing

Run benchmarks:
```bash
cargo bench --target <your-target>
```

Verify targets:
- Page allocation: < 20ns
- Task switch: < 50ns
- Interrupt latency: < 500ns

## Performance Optimization

### 1. Code Size Optimization

For embedded targets, enable LTO and strip symbols:
```toml
[profile.release-embedded]
inherits = "release"
lto = "fat"
codegen-units = 1
opt-level = "z"  # Optimize for size
strip = true
```

### 2. Cache Optimization

Align hot paths to cache lines:
```rust
#[repr(align(64))]
pub struct HotStruct {
    // Frequently accessed data
}
```

### 3. Branch Prediction

Use `likely`/`unlikely` hints:
```rust
if likely(condition) {
    // Hot path
} else {
    // Cold path
}
```

## Common Porting Issues

### 1. Alignment Requirements

Some architectures require strict alignment:
```rust
#[repr(align(16))]
struct AlignedStruct { ... }
```

### 2. Endianness

Handle both big-endian and little-endian:
```rust
#[cfg(target_endian = "big")]
fn read_u32(ptr: *const u8) -> u32 {
    u32::from_be_bytes(...)
}

#[cfg(target_endian = "little")]
fn read_u32(ptr: *const u8) -> u32 {
    u32::from_le_bytes(...)
}
```

### 3. Atomic Operations

Not all platforms support all atomic widths:
```rust
#[cfg(target_has_atomic = "64")]
use core::sync::atomic::AtomicU64;

#[cfg(not(target_has_atomic = "64"))]
use spin::Mutex<u64> as AtomicU64;
```

## Submitting Your Port

1. **Fork the repository**
2. **Create a feature branch**: `git checkout -b port/your-arch`
3. **Follow code style**: Run `cargo fmt` and `cargo clippy`
4. **Add documentation**: Update this guide with platform-specific notes
5. **Add tests**: Include architecture-specific tests
6. **Submit PR**: Describe hardware tested, performance results

### PR Checklist

- [ ] All tests pass
- [ ] Zero clippy warnings
- [ ] Documentation updated
- [ ] Benchmarks included
- [ ] Tested on real hardware (or QEMU)
- [ ] CONTRIBUTING.md guidelines followed

## Resources

### Specifications

- **x86_64**: Intel SDM, AMD APM
- **ARM**: ARM Architecture Reference Manual
- **RISC-V**: RISC-V ISA Specification
- **Cortex-M**: ARM Cortex-M Technical Reference Manual

### Tools

- **QEMU**: Multi-architecture emulator
- **GDB**: Cross-architecture debugging
- **OpenOCD**: On-chip debugging
- **objdump**: Binary inspection

### Community

- GitHub Issues: Report porting challenges
- Discussions: Ask questions about porting
- Matrix/Discord: Real-time help (if available)

## Example: RISC-V Port (Minimal)

Complete minimal example for RISC-V:

```rust
// src/arch/riscv64/mod.rs
pub mod boot;
pub mod interrupts;
pub mod mmu;
pub mod timer;

pub fn init() {
    // Initialize architecture-specific components
    mmu::init();
    interrupts::init();
    timer::init();
}
```

See `src/arch/x86_64/` for a complete reference implementation.

## Getting Help

- **Documentation**: Read the inline code comments
- **Examples**: Check `examples/` directory
- **Issues**: Search existing issues or create new one
- **Community**: Join our discussions forum

Happy porting! 🚀
