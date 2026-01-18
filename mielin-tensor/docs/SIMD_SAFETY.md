# SIMD Intrinsics Safety Requirements

## Overview

This document outlines the safety requirements and best practices for using SIMD (Single Instruction, Multiple Data) intrinsics in mielin-tensor. Our implementation uses platform-specific intrinsics for ARM NEON, Intel AVX2, Intel AVX-512, ARM SVE2, and RISC-V Vector Extension.

## Platform Support

### ARM NEON (AArch64)
- **Vector Width**: 128-bit (4 x f32)
- **Target Feature**: `neon`
- **Availability**: All ARMv8-A processors
- **Safety Level**: Safe when target features are properly gated

### Intel AVX2 (x86_64)
- **Vector Width**: 256-bit (8 x f32)
- **Target Feature**: `avx2`
- **Availability**: Intel Haswell (2013+), AMD Excavator (2015+)
- **Safety Level**: Safe when target features are properly gated

### Intel AVX-512 (x86_64)
- **Vector Width**: 512-bit (16 x f32)
- **Target Features**: `avx512f`
- **Availability**: Intel Skylake-X (2017+), AMD Zen 4 (2022+)
- **Safety Level**: Safe when target features are properly gated

### ARM SVE2 (AArch64)
- **Vector Width**: Scalable (128-2048 bits)
- **Target Feature**: `sve2`
- **Availability**: ARM Neoverse V1 (2021+), Apple M2+ (partial)
- **Safety Level**: Currently stubbed with NEON fallback

### RISC-V Vector Extension (RVV 1.0)
- **Vector Width**: Scalable (configurable VLEN)
- **Target Feature**: `v`
- **Availability**: RISC-V processors with V extension
- **Safety Level**: Currently scalar fallback only

## Safety Requirements

### 1. Target Feature Gating

All SIMD intrinsics MUST be behind proper `cfg` and `target_feature` attributes:

```rust
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn my_avx2_function() {
    // AVX2 intrinsics here
}
```

**Rationale**: Executing SIMD instructions on CPUs that don't support them causes illegal instruction faults.

### 2. Memory Alignment

SIMD operations often require aligned memory access for optimal performance:

```rust
/// Memory alignment for SIMD operations (64 bytes for AVX-512)
pub const SIMD_ALIGNMENT: usize = 64;

// Use aligned allocations from the pool module
let buffer = TENSOR_POOL.allocate_aligned(size, SIMD_ALIGNMENT);
```

**Alignment Requirements**:
- **NEON**: 16-byte alignment preferred (not required)
- **AVX2**: 32-byte alignment preferred (not required)
- **AVX-512**: 64-byte alignment preferred (not required)
- **SVE2**: No specific alignment required (scalable)

**Note**: Modern x86_64 CPUs don't fault on unaligned SIMD loads/stores, but they may be slower.

### 3. Runtime Detection

Always detect CPU capabilities at runtime before using SIMD:

```rust
use mielin_hal::capabilities::HardwareCapabilities;

let caps = HardwareCapabilities::detect();

if caps.contains(HardwareCapabilities::AVX2) {
    // Use AVX2 implementation
} else if caps.contains(HardwareCapabilities::NEON) {
    // Use NEON implementation
} else {
    // Use scalar fallback
}
```

### 4. Unsafe Block Justification

All uses of `unsafe` for SIMD intrinsics must include a safety comment:

```rust
// SAFETY: This function is only called when AVX2 is available (checked via
// runtime detection). The pointer is guaranteed to be valid and the length
// is sufficient for the SIMD operations.
unsafe {
    let vec = _mm256_loadu_ps(ptr);
    // ...
}
```

### 5. Testing Requirements

- **Unit Tests**: Each SIMD backend must have dedicated unit tests
- **Cross-Platform**: Tests must pass on all supported platforms
- **Fallback Tests**: Scalar fallback must be tested independently
- **Correctness**: SIMD results must match scalar results

Example test structure:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_dot_product_scalar() {
        // Test scalar implementation
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_dot_product_avx2() {
        if !HardwareCapabilities::detect().contains(HardwareCapabilities::AVX2) {
            return; // Skip if AVX2 not available
        }
        // Test AVX2 implementation
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn test_dot_product_neon() {
        // Test NEON implementation
    }
}
```

## Common Pitfalls

### 1. Uninitialized Vectors

**WRONG**:
```rust
let mut result: __m256;  // Uninitialized!
result = _mm256_add_ps(a, b);
```

**RIGHT**:
```rust
let result = _mm256_setzero_ps();  // Initialize to zero
let result = _mm256_add_ps(a, b);
```

### 2. Incorrect Pointer Casts

**WRONG**:
```rust
let ptr = data.as_ptr() as *const __m256;  // Incorrect cast!
```

**RIGHT**:
```rust
let ptr = data.as_ptr();
let vec = _mm256_loadu_ps(ptr);  // Load from f32 pointer
```

### 3. Ignoring Remainder Elements

**WRONG**:
```rust
// Only process aligned chunks, ignore remainder
for i in (0..len).step_by(8) {
    // Process 8 elements at a time
}
// Remainder elements are not processed!
```

**RIGHT**:
```rust
let simd_len = len / 8 * 8;
for i in (0..simd_len).step_by(8) {
    // SIMD processing
}
// Process remainder with scalar code
for i in simd_len..len {
    // Scalar processing
}
```

### 4. Platform-Specific Code Without Guards

**WRONG**:
```rust
use std::arch::x86_64::*;  // Compile error on ARM!
```

**RIGHT**:
```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
```

## Performance Considerations

### 1. Loop Unrolling

Unrolling SIMD loops can improve performance by reducing loop overhead:

```rust
// Process 4 vectors per iteration instead of 1
let chunks = len / 32;  // 4 AVX2 vectors = 32 elements
for i in 0..chunks {
    let idx = i * 32;
    let v0 = _mm256_loadu_ps(ptr.add(idx));
    let v1 = _mm256_loadu_ps(ptr.add(idx + 8));
    let v2 = _mm256_loadu_ps(ptr.add(idx + 16));
    let v3 = _mm256_loadu_ps(ptr.add(idx + 24));
    // Process all 4 vectors
}
```

### 2. Minimizing Horizontal Operations

Horizontal operations (e.g., `_mm256_hadd_ps`) are slow. Use reductions instead:

**SLOW**:
```rust
let sum = _mm256_hadd_ps(vec, vec);  // Horizontal add
```

**FAST**:
```rust
// Accumulate in vector, reduce at end
let mut acc = _mm256_setzero_ps();
for vec in vectors {
    acc = _mm256_add_ps(acc, vec);
}
// Final horizontal sum only once
let sum = horizontal_sum_avx2(acc);
```

### 3. Cache Awareness

Access memory in cache-line-aligned chunks:

```rust
const CACHE_LINE: usize = 64;  // bytes
const FLOATS_PER_LINE: usize = CACHE_LINE / 4;  // 16 f32 values

// Process in cache-line-sized chunks
for chunk in data.chunks(FLOATS_PER_LINE) {
    // Process chunk
}
```

## Debugging SIMD Code

### 1. Enable Intrinsics Printing

```rust
#[cfg(debug_assertions)]
fn print_m256(vec: __m256, name: &str) {
    let mut arr = [0.0f32; 8];
    unsafe {
        _mm256_storeu_ps(arr.as_mut_ptr(), vec);
    }
    println!("{}: {:?}", name, arr);
}
```

### 2. Compare with Scalar

Always maintain a scalar reference implementation:

```rust
fn dot_product_reference(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
fn test_simd_correctness() {
    let scalar_result = dot_product_reference(&a, &b);
    let simd_result = dot_product_simd(&a, &b);
    assert!((scalar_result - simd_result).abs() < 1e-5);
}
```

### 3. Use Sanitizers

Enable undefined behavior sanitizer for development:

```bash
RUSTFLAGS="-Zsanitizer=undefined" cargo +nightly build
```

## Future Considerations

### ARM SVE2 Full Implementation

Currently, SVE2 support is stubbed. Full implementation requires:

1. Wait for stable Rust SVE2 intrinsics
2. Implement predicate-based operations
3. Handle variable vector lengths
4. Test on real SVE2 hardware

### RISC-V Vector Extension

RVV intrinsics are in development. Future implementation:

1. Monitor Rust RVV intrinsics stabilization
2. Implement VLEN-agnostic code
3. Test on RISC-V hardware with V extension

## References

- [Intel Intrinsics Guide](https://www.intel.com/content/www/us/en/docs/intrinsics-guide/)
- [ARM NEON Intrinsics](https://developer.arm.com/architectures/instruction-sets/intrinsics/)
- [Rust std::arch Documentation](https://doc.rust-lang.org/std/arch/)
- [SIMD for C++ Developers](https://www.cs.cmu.edu/afs/cs/academic/class/15418-s12/www/)

## Version History

- **2026-01-18**: Documentation updated for v0.1.0-rc.1 release

---

**Note**: This is a living document. Update as new SIMD backends are added or best practices evolve.
