# Hardware Capability Matrix

This document provides a comprehensive matrix of hardware capabilities supported by mielin-hal across different architectures and platforms.

## Quick Reference

| Capability | x86_64 | AArch64 | RISC-V | Cortex-M | Detection Method |
|------------|--------|---------|--------|----------|------------------|
| **SIMD Extensions** |
| SSE4.2 | ✓ | - | - | - | CPUID leaf 1, ECX bit 20 |
| AVX | ✓ | - | - | - | CPUID leaf 1, ECX bit 28 |
| AVX2 | ✓ | - | - | - | CPUID leaf 7, EBX bit 5 |
| AVX-512 | ✓ | - | - | - | CPUID leaf 7, EBX bit 16 |
| NEON | - | ✓ | - | Partial | Compile-time/CTR_EL0 |
| SVE | - | ✓ | - | - | Compile-time feature |
| SVE2 | - | ✓ | - | - | Compile-time feature |
| SME | - | ✓ | - | - | Compile-time feature |
| RVV | - | - | ✓ | - | CSR misa |
| **Math Extensions** |
| FMA | ✓ | Implicit | - | - | CPUID leaf 1, ECX bit 12 |
| FPU | ✓ | ✓ | ✓ | Partial | Architecture default |
| **Cryptography** |
| AES-NI | ✓ | - | - | Partial | CPUID leaf 1, ECX bit 25 |
| ARM Crypto | - | ✓ | - | - | Compile-time feature |
| **Atomics** |
| Atomics | ✓ | ✓ | ✓ | Partial | Architecture default |
| **Accelerators** |
| NPU | Platform | Platform | - | - | Platform detection |
| TPU | Platform | Platform | - | - | Platform detection |

## Architecture-Specific Details

### x86_64

#### Intel Processors

| Processor Family | SSE4.2 | AVX | AVX2 | AVX-512 | FMA | AES-NI |
|-----------------|--------|-----|------|---------|-----|--------|
| Nehalem (2008) | ✓ | - | - | - | - | ✓ |
| Sandy Bridge (2011) | ✓ | ✓ | - | - | - | ✓ |
| Haswell (2013) | ✓ | ✓ | ✓ | - | ✓ | ✓ |
| Skylake (2015) | ✓ | ✓ | ✓ | - | ✓ | ✓ |
| Skylake-X (2017) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Ice Lake (2019) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Alder Lake (2021) | ✓ | ✓ | ✓ | Partial | ✓ | ✓ |
| Sapphire Rapids (2023) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

#### AMD Processors

| Processor Family | SSE4.2 | AVX | AVX2 | AVX-512 | FMA | AES-NI |
|-----------------|--------|-----|------|---------|-----|--------|
| Bulldozer (2011) | ✓ | ✓ | - | - | ✓ | ✓ |
| Excavator (2015) | ✓ | ✓ | ✓ | - | ✓ | ✓ |
| Zen (2017) | ✓ | ✓ | ✓ | - | ✓ | ✓ |
| Zen 2 (2019) | ✓ | ✓ | ✓ | - | ✓ | ✓ |
| Zen 3 (2020) | ✓ | ✓ | ✓ | - | ✓ | ✓ |
| Zen 4 (2022) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

#### Vector Width Progression

- **SSE/SSE2**: 128-bit vectors (4x float32 or 2x float64)
- **AVX/AVX2**: 256-bit vectors (8x float32 or 4x float64)
- **AVX-512**: 512-bit vectors (16x float32 or 8x float64)

### AArch64

#### ARM Processors

| Processor | NEON | SVE | SVE2 | SME | ARM Crypto |
|-----------|------|-----|------|-----|------------|
| Cortex-A53 | ✓ | - | - | - | ✓ |
| Cortex-A57 | ✓ | - | - | - | ✓ |
| Cortex-A72 | ✓ | - | - | - | ✓ |
| Cortex-A76 | ✓ | - | - | - | ✓ |
| Neoverse V1 | ✓ | ✓ | ✓ | - | ✓ |
| Cortex-X2 | ✓ | - | ✓ | - | ✓ |
| Neoverse V2 | ✓ | ✓ | ✓ | ✓ | ✓ |

#### Apple Silicon

| Chip | NEON | SVE | SVE2 | SME | Performance Cores |
|------|------|-----|------|-----|------------------|
| M1 | ✓ | - | - | - | 4 |
| M1 Pro/Max | ✓ | - | - | - | 8 |
| M2 | ✓ | - | - | - | 4 |
| M2 Pro/Max | ✓ | - | - | - | 8 |
| M3/M3 Pro/Max | ✓ | - | - | - | 4-12 |

#### Vector Width Progression

- **NEON**: 128-bit vectors (4x float32 or 2x float64)
- **SVE**: 128-2048 bits (hardware-dependent, typical: 256 or 512)
- **SVE2**: Same as SVE with enhanced instructions
- **SME**: 128-2048 bits (streaming mode)

### RISC-V

#### Vector Extension (RVV)

| VLEN | Typical Use Case | Vector Registers |
|------|------------------|------------------|
| 128 | Embedded | 32 |
| 256 | Desktop/Mobile | 32 |
| 512 | Server | 32 |
| 1024+ | HPC | 32 |

#### Detection

- **Base ISA**: Check `misa` CSR
- **Vector**: Check `V` bit in misa
- **VLEN**: Runtime query via `vlenb` CSR

### ARM Cortex-M

#### Processor Variants

| Variant | FPU | DSP | MPU | SIMD | Cache |
|---------|-----|-----|-----|------|-------|
| M0 | - | - | Optional | - | - |
| M0+ | - | - | Optional | - | - |
| M3 | - | - | Optional | - | - |
| M4 | Optional | ✓ | Optional | - | - |
| M7 | ✓ | ✓ | ✓ | - | ✓ |
| M23 | Optional | - | Optional | - | - |
| M33 | ✓ | ✓ | ✓ | - | Optional |
| M55 | ✓ | ✓ | ✓ | MVE | ✓ |
| M85 | ✓ | ✓ | ✓ | MVE | ✓ |

#### FPU Variants

- **Single Precision**: 32-bit floating point (M4, M7, M33, M55, M85)
- **Double Precision**: 64-bit floating point (M7, M55, M85)

## Platform-Specific Capabilities

### Raspberry Pi

| Model | CPU | NEON | GPU | VideoCore | AI Accelerator |
|-------|-----|------|-----|-----------|----------------|
| Pi 3 | Cortex-A53 | ✓ | VideoCore IV | ✓ | - |
| Pi 4 | Cortex-A72 | ✓ | VideoCore VI | ✓ | - |
| Pi 5 | Cortex-A76 | ✓ | VideoCore VII | ✓ | - |
| Pi Zero 2 | Cortex-A53 | ✓ | VideoCore IV | ✓ | - |

### STM32 Microcontrollers

| Family | Core | FPU | DSP | MPU | Crypto | AES |
|--------|------|-----|-----|-----|--------|-----|
| F0 | M0 | - | - | - | - | - |
| F1 | M3 | - | - | - | - | - |
| F4 | M4 | ✓ | ✓ | ✓ | Partial | ✓ |
| F7 | M7 | ✓ | ✓ | ✓ | ✓ | ✓ |
| H7 | M7 | ✓ | ✓ | ✓ | ✓ | ✓ |
| L4 | M4 | ✓ | ✓ | ✓ | ✓ | ✓ |

### ESP32 Variants

| Variant | Cores | WiFi | Bluetooth | Thread/Zigbee | AI Accelerator |
|---------|-------|------|-----------|---------------|----------------|
| ESP32 | 2 | WiFi 4 | BT 4.2 + BLE | - | - |
| ESP32-S2 | 1 | WiFi 4 | - | - | - |
| ESP32-S3 | 2 | WiFi 4 | BLE 5.0 | - | Vector Ext |
| ESP32-C3 | 1 | WiFi 4 | BLE 5.0 | - | - |
| ESP32-C6 | 1 | WiFi 6 | BLE 5.3 | 802.15.4 | - |
| ESP32-H2 | 1 | - | BLE 5.3 | 802.15.4 | - |

## Detection Methods

### Runtime Detection

#### x86_64 (CPUID)

```rust
use mielin_hal::arch::x86_64;

// Runtime detection
let has_avx2 = x86_64::has_avx2();
let has_fma = x86_64::has_fma();
let has_aes_ni = x86_64::has_aes_ni();

// All capabilities at once
let caps = x86_64::detect_x86_64_capabilities();
```

#### AArch64 (Compile-time + System Registers)

```rust
use mielin_hal::capabilities::HardwareProfile;

// Comprehensive detection
let profile = HardwareProfile::detect();

// Check specific capabilities
if profile.capabilities.contains(HardwareCapabilities::NEON) {
    // Use NEON
}
```

#### ARM Cortex-M (Memory-Mapped Registers)

```rust
use mielin_hal::arch::cortex_m::CortexMCapabilities;

// Detect all Cortex-M capabilities
let caps = CortexMCapabilities::detect();

if let Some(fpu) = &caps.fpu {
    println!("FPU: {:?}", fpu.variant);
}
```

### Compile-Time Detection

Use Rust's `target_feature` flags:

```rust
#[cfg(target_feature = "avx2")]
fn optimized_path() {
    // AVX2 implementation
}

#[cfg(not(target_feature = "avx2"))]
fn optimized_path() {
    // Fallback implementation
}
```

## Performance Implications

### SIMD Throughput

| Extension | 32-bit ops/cycle | 64-bit ops/cycle | Latency (cycles) |
|-----------|------------------|------------------|------------------|
| SSE | 4 | 2 | 3-5 |
| AVX | 8 | 4 | 3-5 |
| AVX2 | 8 | 4 | 3-5 |
| AVX-512 | 16 | 8 | 4-6 |
| NEON | 4 | 2 | 3-4 |
| SVE (256) | 8 | 4 | 3-4 |

### Power Efficiency

#### AVX-512 Frequency Scaling

On some Intel CPUs, AVX-512 usage may reduce frequency:

- **Light AVX-512**: -100 to -200 MHz
- **Heavy AVX-512**: -300 to -500 MHz

Consider AVX2 for sustained workloads on affected CPUs.

#### ARM Big.LITTLE

On heterogeneous ARM systems:

- **Big cores**: Use SIMD extensively (better performance)
- **LITTLE cores**: Use SIMD selectively (better efficiency)

## Feature Combinations

### Recommended Combinations

#### High Performance Computing

```rust
// Prefer: AVX-512 + FMA + large cache
FeatureRequirement::all_of(&[
    HardwareCapabilities::AVX512,
    HardwareCapabilities::FMA,
])
.with_min_cache_size(32 * 1024 * 1024) // 32 MB L3
```

#### Balanced Performance

```rust
// Prefer: AVX2 + FMA
FeatureRequirement::all_of(&[
    HardwareCapabilities::AVX2,
    HardwareCapabilities::FMA,
])
```

#### Maximum Compatibility

```rust
// Prefer: SSE4.2 (present on all modern x86_64)
FeatureRequirement::all_of(&[
    HardwareCapabilities::SSE4_2,
])
```

#### Cryptography

```rust
// Prefer: AES-NI for hardware acceleration
FeatureRequirement::all_of(&[
    HardwareCapabilities::AES_NI,
])
```

## Common Pitfalls

### 1. AVX-512 Availability

**Problem**: AVX-512 exists in many variants; not all features are universally available.

**Solution**: Check specific AVX-512 subfeatures if needed, or use the conservative `AVX512F` flag.

### 2. Frequency Scaling

**Problem**: AVX-512 usage can reduce CPU frequency on some Intel CPUs.

**Solution**: Benchmark with realistic workloads; consider AVX2 for sustained operations.

### 3. Cross-Compilation

**Problem**: Compile-time feature detection fails when cross-compiling.

**Solution**: Use runtime detection via CPUID/system registers.

### 4. Hypervisor Hiding

**Problem**: Hypervisors may hide or emulate features.

**Solution**: Always verify features work before committing to an implementation.

### 5. ARM Big.LITTLE Scheduling

**Problem**: Thread may migrate between big and LITTLE cores with different capabilities.

**Solution**: Use CPU affinity or design algorithms that work on both core types.

## Testing Recommendations

### Unit Tests

```rust
#[test]
fn test_capability_detection() {
    let caps = detect_capabilities();

    // Verify SIMD consistency
    if caps.contains(HardwareCapabilities::AVX2) {
        assert!(caps.contains(HardwareCapabilities::AVX));
    }

    if caps.contains(HardwareCapabilities::AVX512) {
        assert!(caps.contains(HardwareCapabilities::AVX2));
    }
}
```

### Runtime Verification

```rust
// Test that selected features actually work
let mut selector = RuntimeSelector::new();
let result = selector.select_verified(&chain, |req| {
    // Try using the feature and catch exceptions
    test_feature_works(req)
});
```

## Further Reading

- [Intel Intrinsics Guide](https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html)
- [ARM NEON Programmer's Guide](https://developer.arm.com/documentation/den0018/a)
- [ARM SVE Documentation](https://developer.arm.com/documentation/100891/latest)
- [RISC-V Vector Extension](https://github.com/riscv/riscv-v-spec)
- [x86_64 CPUID Reference](https://www.felixcloutier.com/x86/cpuid)

## Version History


---

**Last Updated**: 2026-01-18
**Maintainer**: COOLJAPAN OU (Team Kitasan)
