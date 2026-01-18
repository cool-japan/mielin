# MielinOS Linker Scripts

This directory contains linker scripts for embedded systems with ROM-able kernel sections.

## Overview

Embedded systems typically have two types of memory:
- **Flash (ROM)**: Non-volatile memory for code and read-only data
- **RAM**: Volatile memory for runtime data and stack

These linker scripts organize kernel sections optimally:
- `.text` (code) → Flash (execute-in-place)
- `.rodata` (constants) → Flash (read-only)
- `.data` (initialized data) → RAM (copied from flash at startup)
- `.bss` (uninitialized data) → RAM (zero-initialized at startup)
- `.heap` → RAM (dynamic allocation)
- `.stack` → RAM (call stack)

## Available Scripts

### cortex-m.ld

Linker script for ARM Cortex-M microcontrollers (M0, M0+, M3, M4, M7).

**Target Example**: STM32F0 (Cortex-M0)
- Flash: 0x08000000 (32KB)
- RAM: 0x20000000 (8KB)

**Usage**:
```bash
# In .cargo/config.toml
[target.'cfg(all(target_arch = "arm", target_os = "none"))']
rustflags = ["-C", "link-arg=-Tlinker/cortex-m.ld"]
```

**Customize** for your board by editing:
```ld
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 32K
    RAM (rwx)   : ORIGIN = 0x20000000, LENGTH = 8K
}
```

### riscv32-embedded.ld

Linker script for RISC-V 32-bit embedded systems.

**Target Example**: ESP32-C3
- Flash: 0x42000000 (384KB)
- RAM: 0x3FC80000 (400KB)

**Usage**:
```bash
# In .cargo/config.toml
[target.'cfg(all(target_arch = "riscv32", target_os = "none"))']
rustflags = ["-C", "link-arg=-Tlinker/riscv32-embedded.ld"]
```

**Customize** for your board by editing:
```ld
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x42000000, LENGTH = 384K
    RAM (rwx)   : ORIGIN = 0x3FC80000, LENGTH = 400K
}
```

## Memory Layout Visualization

### Cortex-M Layout
```
Flash (0x08000000)
├─ Vector Table   (256 bytes)
├─ .text          (code)
├─ .rodata        (constants)
└─ .data (LMA)    (initialized data image)

RAM (0x20000000)
├─ .data          (copied from flash at startup)
├─ .bss           (zero-initialized)
├─ .heap          (grows upward →)
└─ .stack         (grows downward ←)
```

### RISC-V Layout
```
Flash (0x42000000)
├─ .text          (code + entry point)
├─ .rodata        (constants)
└─ .data/.sdata (LMA)

RAM (0x3FC80000)
├─ .data          (copied from flash)
├─ .sdata         (small data, GP-relative)
├─ .bss/.sbss     (zero-initialized)
├─ .heap          (grows upward →)
└─ .stack         (grows downward ←, 16-byte aligned)
```

## XIP (Execute-In-Place)

Both scripts support XIP:
- Code runs directly from flash (no RAM copy needed)
- Read-only data accessed directly from flash
- Only `.data` section is copied to RAM at startup

**Benefits**:
- Minimal RAM usage (~8KB possible)
- Fast boot time (no large memory copy)
- Ideal for embedded systems

## Startup Sequence

1. **Reset vector** → `Reset_Handler` / `_start`
2. **Copy .data**: flash → RAM (using `_sidata`, `_sdata`, `_edata`)
3. **Zero .bss**: (using `_sbss`, `_ebss`)
4. **Jump to main**: `kernel_main()` or `main()`

## Memory Sections Explained

### .text (Code)
- **Location**: Flash
- **Attributes**: Read, Execute
- **Content**: All executable code
- **XIP**: Yes (execute directly from flash)

### .rodata (Read-Only Data)
- **Location**: Flash
- **Attributes**: Read
- **Content**: String literals, const values
- **XIP**: Yes (access directly from flash)

### .data (Initialized Data)
- **Location**: RAM (LMA in flash)
- **Attributes**: Read, Write
- **Content**: Global/static variables with initial values
- **Startup**: Copied from flash to RAM

### .bss (Uninitialized Data)
- **Location**: RAM
- **Attributes**: Read, Write
- **Content**: Global/static variables (zero-initialized)
- **Startup**: Zeroed by startup code

### .heap (Dynamic Memory)
- **Location**: RAM
- **Attributes**: Read, Write
- **Content**: Malloc/box/vec allocations
- **Runtime**: Managed by allocator

### .stack (Call Stack)
- **Location**: RAM (top of memory)
- **Attributes**: Read, Write
- **Content**: Function call frames, local variables
- **Runtime**: Grows downward from `_estack`

## Customization Guide

### Adjusting Memory Sizes

Edit the `MEMORY` section for your board:
```ld
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 64K    /* Your flash size */
    RAM (rwx)   : ORIGIN = 0x20000000, LENGTH = 16K    /* Your RAM size */
}
```

### Adjusting Stack/Heap Sizes

Edit near the top of the script:
```ld
_stack_size = 4K;   /* Increase if deep call chains */
_heap_size = 8K;    /* Increase if heavy dynamic allocation */
```

### Multiple Memory Regions

Some MCUs have multiple RAM regions:
```ld
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 64K
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 16K
    CCM   (rwx) : ORIGIN = 0x10000000, LENGTH = 4K    /* Core-Coupled Memory */
}

/* Use CCM for critical data */
.ccm_data :
{
    *(.ccm_data)
} > CCM
```

## Build Configuration

### .cargo/config.toml

```toml
[target.thumbv6m-none-eabi]
rustflags = [
    "-C", "link-arg=-Tlinker/cortex-m.ld",
    "-C", "link-arg=-nostartfiles",
]

[target.riscv32imc-unknown-none-elf]
rustflags = [
    "-C", "link-arg=-Tlinker/riscv32-embedded.ld",
    "-C", "link-arg=-nostartfiles",
]
```

### Cargo.toml

```toml
[profile.release]
opt-level = "z"         # Optimize for size
lto = true              # Link-time optimization
codegen-units = 1       # Better optimization
debug = false           # No debug symbols
strip = true            # Strip symbols
panic = "abort"         # Smaller panic handler
```

## Size Optimization

### Check Section Sizes
```bash
cargo size --release -- -A
```

### Target Sizes
- **Cortex-M0**: <32KB code + <8KB RAM
- **Cortex-M4**: <256KB code + <64KB RAM
- **RISC-V**: <256KB code + <256KB RAM

### Optimization Tips
1. Enable LTO: `lto = true`
2. Optimize for size: `opt-level = "z"`
3. Strip symbols: `strip = true`
4. Use single codegen unit: `codegen-units = 1`
5. Minimize dependencies
6. Use `#[inline(never)]` for rarely-called code
7. Use `#[inline(always)]` for hot paths

## Troubleshooting

### "section will not fit in region 'FLASH'"
→ Code is too large. Enable LTO and size optimization.

### "section will not fit in region 'RAM'"
→ Reduce stack/heap size or optimize data structures.

### Stack overflow
→ Increase `_stack_size` or reduce recursion depth.

### Heap exhausted
→ Increase `_heap_size` or use static allocation.

## References

- [ARM Cortex-M Linker Scripts](https://interrupt.memfault.com/blog/how-to-write-linker-scripts-for-firmware)
- [RISC-V ABI](https://github.com/riscv-non-isa/riscv-elf-psabi-doc)
- [GNU LD Manual](https://sourceware.org/binutils/docs/ld/)
