# mielin-hal Porting Guide

This guide explains how to add support for new architectures to mielin-hal.

## Table of Contents

1. [Overview](#overview)
2. [Architecture Requirements](#architecture-requirements)
3. [Step-by-Step Porting Process](#step-by-step-porting-process)
4. [Implementation Checklist](#implementation-checklist)
5. [Testing Guidelines](#testing-guidelines)
6. [Example: Adding MIPS64 Support](#example-adding-mips64-support)
7. [Common Pitfalls](#common-pitfalls)
8. [Resources](#resources)

## Overview

mielin-hal is designed to be easily extensible to new architectures. The porting process typically involves:

1. Adding the architecture enum variant
2. Creating architecture-specific detection code
3. Implementing capability detection
4. Adding tests
5. Documenting the new architecture

**Estimated effort**: 2-5 days for a basic port, 1-2 weeks for comprehensive support.

## Architecture Requirements

Before porting, ensure:

- [ ] The target architecture has a stable Rust target (check `rustc --print target-list`)
- [ ] You have access to documentation for:
  - Instruction set architecture (ISA)
  - SIMD/vector extensions
  - System registers for capability detection
  - Cache hierarchy
  - Power management features
- [ ] You have access to hardware or an emulator for testing (QEMU recommended)

## Step-by-Step Porting Process

### Step 1: Update Core Types

**File**: `src/arch.rs`

Add your architecture to the `Architecture` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    X86_64,
    AArch64,
    RiscV64,
    CortexM,
    Mips64,      // New architecture
    PowerPC64,   // New architecture
    Unknown,
}
```

Update the `detect_architecture()` function:

```rust
pub fn detect_architecture() -> Architecture {
    #[cfg(target_arch = "x86_64")]
    return Architecture::X86_64;

    #[cfg(target_arch = "aarch64")]
    return Architecture::AArch64;

    // ... existing architectures ...

    #[cfg(target_arch = "mips64")]
    return Architecture::Mips64;

    #[cfg(target_arch = "powerpc64")]
    return Architecture::PowerPC64;

    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        // ... add all supported architectures
    )))]
    Architecture::Unknown
}
```

### Step 2: Create Architecture Module

**File**: `src/arch/your_arch.rs` (e.g., `src/arch/mips64.rs`)

Create a new module with this structure:

```rust
//! MIPS64 Architecture Support
//!
//! Provides hardware capability detection for MIPS64 processors.
//!
//! ## Supported Processors
//!
//! - MIPS64 Release 2 and later
//! - Loongson 3A/3B series
//! - Cavium OCTEON
//!
//! ## Features Detected
//!
//! - Base MIPS64 ISA
//! - MSA (MIPS SIMD Architecture)
//! - FPU capabilities
//! - Cache information
//!
//! ## Detection Method
//!
//! Uses CPUCFG instruction (Loongson) or /proc/cpuinfo parsing

use crate::capabilities::HardwareCapabilities;
use crate::error::Result;

/// Detect MIPS64-specific capabilities
pub fn detect_mips64_capabilities() -> HardwareCapabilities {
    let mut caps = HardwareCapabilities::empty();

    // Detect MSA (MIPS SIMD Architecture)
    if has_msa() {
        caps |= HardwareCapabilities::SIMD;
    }

    caps
}

/// Check if MSA (MIPS SIMD Architecture) is available
pub fn has_msa() -> bool {
    #[cfg(target_feature = "msa")]
    {
        true
    }
    #[cfg(not(target_feature = "msa"))]
    {
        // Runtime detection via /proc/cpuinfo or CPUCFG
        detect_msa_runtime()
    }
}

#[cfg(target_os = "linux")]
fn detect_msa_runtime() -> bool {
    // Parse /proc/cpuinfo for "msa" feature
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        cpuinfo.contains("msa")
    } else {
        false
    }
}

#[cfg(not(target_os = "linux"))]
fn detect_msa_runtime() -> bool {
    false
}

/// Get maximum vector width in bits
pub fn max_vector_width() -> usize {
    if has_msa() {
        128  // MSA uses 128-bit vectors
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_detection() {
        let caps = detect_mips64_capabilities();
        // Should not panic
        println!("MIPS64 capabilities: {:?}", caps);
    }

    #[test]
    fn test_vector_width() {
        let width = max_vector_width();
        assert!(width == 0 || width == 128);
    }
}
```

### Step 3: Update Capability Detection

**File**: `src/capabilities.rs`

Update the `detect_architecture_capabilities()` function:

```rust
fn detect_architecture_capabilities() -> HardwareCapabilities {
    #[cfg(target_arch = "x86_64")]
    {
        crate::arch::x86_64::detect_x86_64_capabilities()
    }

    // ... existing architectures ...

    #[cfg(target_arch = "mips64")]
    {
        crate::arch::mips64::detect_mips64_capabilities()
    }

    #[cfg(not(any(
        target_arch = "x86_64",
        // ... all supported architectures
    )))]
    {
        HardwareCapabilities::empty()
    }
}
```

Update `max_vector_width()`:

```rust
pub fn max_vector_width() -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        crate::arch::x86_64::max_vector_width()
    }

    // ... existing architectures ...

    #[cfg(target_arch = "mips64")]
    {
        crate::arch::mips64::max_vector_width()
    }

    #[cfg(not(any(/* all archs */)))]
    {
        0
    }
}
```

### Step 4: Add Module to lib.rs

**File**: `src/lib.rs`

```rust
#[cfg(target_arch = "mips64")]
pub mod mips64;
```

Update the architecture list in documentation.

### Step 5: Add Architecture-Specific Capability Flags (Optional)

**File**: `src/capabilities.rs`

If the architecture has unique features, add new flags:

```rust
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct HardwareCapabilities: u64 {
        // ... existing flags ...

        /// MIPS MSA (MIPS SIMD Architecture)
        const MSA       = 1 << 20;

        /// MIPS DSP ASE
        const MIPS_DSP  = 1 << 21;
    }
}
```

### Step 6: Update Cache Detection (Optional)

**File**: `src/cache.rs`

Add cache detection for the new architecture:

```rust
#[cfg(target_arch = "mips64")]
fn detect_cache_sizes() -> (usize, usize, usize) {
    // Use /sys/devices/system/cpu/cpu0/cache on Linux
    // Or implement MIPS-specific detection via CPUCFG
    (0, 0, 0)
}
```

### Step 7: Add Tests

**File**: `src/arch/your_arch.rs`

Add comprehensive tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_detection_no_panic() {
        let caps = detect_mips64_capabilities();
        println!("Detected capabilities: {:?}", caps);
    }

    #[test]
    fn test_vector_width_valid() {
        let width = max_vector_width();
        assert!(width == 0 || width == 128);
    }

    #[test]
    fn test_msa_detection_consistent() {
        let has_msa_flag = has_msa();
        let caps = detect_mips64_capabilities();

        if has_msa_flag {
            assert!(caps.contains(HardwareCapabilities::SIMD));
        }
    }
}
```

### Step 8: Add Integration Test

**File**: `tests/integration_tests.rs`

```rust
#[cfg(target_arch = "mips64")]
#[test]
fn test_mips64_specific_detection() {
    use mielin_hal::arch::mips64::*;

    let caps = detect_mips64_capabilities();
    println!("MIPS64 capabilities: {:?}", caps);

    println!("Has MSA: {}", has_msa());
}
```

### Step 9: Update CI/CD

**File**: `.github/workflows/ci.yml`

Add cross-compilation test:

```yaml
test-mips64-cross:
  name: Cross-compile MIPS64
  runs-on: ubuntu-latest
  strategy:
    matrix:
      target:
        - mips64-unknown-linux-gnuabi64
        - mips64el-unknown-linux-gnuabi64
  steps:
    - uses: actions/checkout@v4
    - name: Install Rust toolchain
      uses: dtolnay/rust-toolchain@stable
      with:
        targets: ${{ matrix.target }}
    - name: Install cross
      run: cargo install cross --git https://github.com/cross-rs/cross
    - name: Build for ${{ matrix.target }}
      run: cross build --target ${{ matrix.target }} --verbose
    - name: Test for ${{ matrix.target }}
      run: cross test --target ${{ matrix.target }} --verbose
      continue-on-error: true
```

### Step 10: Document the Architecture

Update `CAPABILITIES.md` with architecture-specific capability matrix:

```markdown
### MIPS64 Architecture

| Processor       | MSA | DSP | Cache L1/L2/L3 | Notes              |
|-----------------|-----|-----|----------------|--------------------|
| Loongson 3A4000 | ✓   | ✓   | 64KB/256KB/8MB | Quad-core          |
| Loongson 3B5000 | ✓   | ✓   | 64KB/256KB/16MB| Server-class       |
| Cavium OCTEON   | ✗   | ✓   | 32KB/128KB/2MB | Network processor  |
```

## Implementation Checklist

Use this checklist to track your porting progress:

### Core Implementation
- [ ] Add architecture enum variant in `src/arch.rs`
- [ ] Implement `detect_architecture()` for new arch
- [ ] Create `src/arch/your_arch.rs` module
- [ ] Implement `detect_xxx_capabilities()` function
- [ ] Implement capability detection functions (SIMD, etc.)
- [ ] Implement `max_vector_width()` function
- [ ] Add module to `src/lib.rs`

### Capability Flags (if needed)
- [ ] Add architecture-specific capability flags to `HardwareCapabilities`
- [ ] Update `detect_architecture_capabilities()`
- [ ] Update `max_vector_width()` global function

### Cache Support (optional)
- [ ] Implement cache size detection
- [ ] Update `src/cache.rs` for new architecture
- [ ] Test cache detection on real hardware

### Memory & Topology (optional)
- [ ] Implement memory size detection (usually OS-level)
- [ ] Implement CPU topology detection
- [ ] Test NUMA detection (if applicable)

### Testing
- [ ] Add unit tests in architecture module
- [ ] Add integration tests
- [ ] Test on real hardware or emulator
- [ ] Add CI/CD cross-compilation job
- [ ] Verify all tests pass

### Documentation
- [ ] Write comprehensive module documentation
- [ ] Document supported processors
- [ ] Document detected features
- [ ] Document detection methods
- [ ] Add examples to module docs
- [ ] Update `CAPABILITIES.md` with capability matrix
- [ ] Update `README.md` with architecture support
- [ ] Add to `PORTING_GUIDE.md` as example (if significant)

### Performance (optional)
- [ ] Add benchmarks for capability detection
- [ ] Optimize hot paths
- [ ] Add caching if detection is expensive

## Testing Guidelines

### Local Testing

1. **Build for target architecture:**
   ```bash
   cargo build --target mips64-unknown-linux-gnuabi64
   ```

2. **Run tests with cross:**
   ```bash
   cross test --target mips64-unknown-linux-gnuabi64
   ```

3. **Test on QEMU (if available):**
   ```bash
   qemu-mips64 -L /usr/mips64-linux-gnu target/mips64-unknown-linux-gnuabi64/debug/your_binary
   ```

### Real Hardware Testing

If you have access to real hardware:

1. Build for the target
2. Copy binary to device
3. Run comprehensive tests
4. Verify capability detection matches hardware specs
5. Test edge cases (old processors, missing features)

### Emulator Testing

QEMU supports many architectures:

```bash
# Install QEMU
sudo apt-get install qemu-user-static

# Run tests
cross test --target mips64-unknown-linux-gnuabi64
```

## Example: Adding MIPS64 Support

Here's a complete minimal example for MIPS64:

### 1. Update `src/arch.rs`:

```rust
pub enum Architecture {
    Mips64,
    // ...
}

pub fn detect_architecture() -> Architecture {
    #[cfg(target_arch = "mips64")]
    return Architecture::Mips64;
    // ...
}
```

### 2. Create `src/arch/mips64.rs`:

```rust
use crate::capabilities::HardwareCapabilities;

pub fn detect_mips64_capabilities() -> HardwareCapabilities {
    let mut caps = HardwareCapabilities::empty();

    if has_msa() {
        caps |= HardwareCapabilities::SIMD;
    }

    caps
}

pub fn has_msa() -> bool {
    #[cfg(target_feature = "msa")]
    return true;

    #[cfg(not(target_feature = "msa"))]
    false
}

pub fn max_vector_width() -> usize {
    if has_msa() { 128 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection() {
        let caps = detect_mips64_capabilities();
        assert_ne!(caps, HardwareCapabilities::from_bits_truncate(0xFFFFFFFF));
    }
}
```

### 3. Update `src/lib.rs`:

```rust
#[cfg(target_arch = "mips64")]
pub mod mips64;
```

### 4. Update `src/capabilities.rs`:

```rust
fn detect_architecture_capabilities() -> HardwareCapabilities {
    #[cfg(target_arch = "mips64")]
    {
        crate::arch::mips64::detect_mips64_capabilities()
    }
    // ...
}
```

## Common Pitfalls

### 1. Compile-Time vs Runtime Detection

**Problem**: Using `#[cfg(target_feature)]` gives compile-time detection, not runtime.

**Solution**: Implement runtime detection via:
- System registers (if privileged access available)
- `/proc/cpuinfo` on Linux
- CPUID-like instructions (if available)
- OS APIs (sysctl, syscalls)

### 2. Missing std on Embedded

**Problem**: Embedded targets (no_std) can't use `std::fs` or `std::env`.

**Solution**: Use syscalls directly or conditional compilation:

```rust
#[cfg(all(target_os = "linux", not(target_env = "sgx")))]
fn detect_feature() -> bool {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.contains("feature"))
        .unwrap_or(false)
}

#[cfg(not(all(target_os = "linux", not(target_env = "sgx"))))]
fn detect_feature() -> bool {
    false
}
```

### 3. Unsafe Code

**Problem**: Reading system registers requires unsafe code.

**Solution**: Document safety requirements:

```rust
/// # Safety
///
/// This function reads the MIPS CPUCFG register, which requires:
/// - Running on MIPS64 hardware with CPUCFG support (Loongson 3A4000+)
/// - Proper privilege level (may require kernel mode)
///
/// # Panics
///
/// May panic or cause undefined behavior if:
/// - Not running on supported hardware
/// - Insufficient privilege level
#[cfg(target_arch = "mips64")]
pub unsafe fn read_cpucfg(index: u32) -> u32 {
    let result: u32;
    core::arch::asm!(
        "cpucfg {}, {}",
        out(reg) result,
        in(reg) index,
    );
    result
}
```

### 4. Cross-Compilation Issues

**Problem**: Cross-compilation fails due to missing dependencies.

**Solution**: Use `cross` tool which provides pre-built environments:

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --target mips64-unknown-linux-gnuabi64
```

### 5. Test Failures in CI

**Problem**: Tests fail in emulation but work on real hardware.

**Solution**: Mark emulation-sensitive tests:

```rust
#[test]
#[cfg_attr(target_env = "qemu", ignore)]
fn test_real_hardware_only() {
    // This test requires real hardware
}
```

## Resources

### Architecture Documentation

- **MIPS**: [MIPS Architecture](https://www.mips.com/products/)
- **PowerPC**: [Power ISA](https://openpowerfoundation.org/)
- **LoongArch**: [Loongson Documentation](http://www.loongson.cn/)

### Rust Resources

- [Rust Platform Support](https://doc.rust-lang.org/nightly/rustc/platform-support.html)
- [Rust Target Triple](https://doc.rust-lang.org/nightly/rustc/platform-support.html)
- [std::arch module](https://doc.rust-lang.org/std/arch/)

### Testing Resources

- [cross-rs](https://github.com/cross-rs/cross) - Cross-compilation tool
- [QEMU User Mode](https://qemu-project.gitlab.io/qemu/user/main.html)
- [cargo-nextest](https://nexte.st/) - Better test runner

### Example Ports in Other Projects

- [hwloc](https://www.open-mpi.org/projects/hwloc/) - Hardware locality library
- [cpuinfo](https://github.com/pytorch/cpuinfo) - CPU detection library
- [raw-cpuid](https://github.com/gz/rust-cpuid) - x86 CPUID library

## Getting Help

- Open an issue on GitHub for questions
- Check existing architecture implementations for examples
- Refer to `TROUBLESHOOTING.md` for common problems
- Review `CAPABILITIES.md` for capability matrix format

## Contributing Your Port

Once your port is complete:

1. Run full test suite: `cargo test --all-features`
2. Run clippy: `cargo clippy -- -D warnings`
3. Format code: `cargo fmt`
4. Update TODO.md with completion status
5. Create pull request with:
   - Clear description of architecture
   - Test results (hardware or emulator)
   - Documentation updates
   - Example usage

Thank you for contributing to mielin-hal!
