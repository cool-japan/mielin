# mielin-hal

**Hardware Abstraction Layer (Layer 0 Interface)**

Cross-platform hardware abstraction for Arm (AArch64, Cortex-M), RISC-V, and x86 architectures. MielinHAL provides the unified interface between MielinOS kernel (Layer 1) and diverse hardware platforms (Layer 0), enabling true write-once-run-anywhere agent deployment.

## Overview

The Hardware Abstraction Layer (HAL) provides a unified interface to detect and utilize hardware capabilities across diverse platforms—from embedded microcontrollers to high-performance servers. MielinHAL enables the kernel to make intelligent decisions about resource allocation, agent placement, and hardware acceleration.

**Current Status:** v0.1.0-rc.1 "Oligodendrocyte" (Released 2026-01-18)

## Supported Architectures

- **AArch64**: Arm 64-bit (AWS Graviton, Cortex-A series, Neoverse)
- **RISC-V 64**: RISC-V 64-bit processors (emerging platforms)
- **x86_64**: Intel and AMD 64-bit processors
- **Arm Cortex-M**: Embedded ARM microcontrollers (STM32, nRF52, ESP32)

## Features

### Key Capabilities

- **Architecture Detection**: Runtime and compile-time architecture identification
- **Feature Discovery**: Detect CPU extensions (SVE2, SME, NEON, AVX512, etc.)
- **Vector Width Detection**: Identify SIMD capabilities (128-2048 bits)
- **Core Count Detection**: Physical and logical CPU enumeration
- **Cache Information**: Cache line size, hierarchy detection
- **Accelerator Discovery**: NPU, GPU, FPGA detection (Phase 2+)
- **Zero Overhead**: Compile-time optimization where possible

## Architecture

MielinHAL serves as the interface between Layer 0 (Hardware) and Layer 1 (Kernel):

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: MielinOS Kernel                                    │
│          ↕ (uses mielin-hal)                                │
│ Layer 0.5: MielinHAL (Hardware Abstraction)                │
│          ↕                                                  │
│ Layer 0: Physical Hardware                                  │
│          • Arm (Cortex-A/M, Neoverse)                      │
│          • RISC-V (SiFive, StarFive)                       │
│          • x86_64 (Intel, AMD)                             │
└─────────────────────────────────────────────────────────────┘
```

### File Structure

```
mielin-hal/
├── src/
│   ├── aarch64/       # Arm 64-bit support
│   │   ├── sve.rs    # SVE/SVE2 detection
│   │   └── sme.rs    # SME detection
│   ├── riscv/         # RISC-V support
│   ├── x86_64/        # x86 support
│   │   └── avx.rs    # AVX/AVX2/AVX512
│   ├── cortex_m/      # Arm Cortex-M support
│   ├── traits.rs      # Common HAL traits
│   ├── capabilities.rs # Hardware capability flags
│   └── lib.rs         # Public API
└── Cargo.toml
```

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-hal = { path = "../mielin-hal" }
```

### Architecture Detection

Automatic runtime architecture detection:

```rust
use mielin_hal::{detect_architecture, Architecture};

let arch = detect_architecture();
match arch {
    Architecture::AArch64 => println!("Running on Arm 64-bit"),
    Architecture::X86_64 => println!("Running on x86_64"),
    Architecture::RiscV64 => println!("Running on RISC-V"),
    Architecture::ArmCortexM => println!("Running on Cortex-M"),
}
```

### Hardware Capability Detection

Detect CPU features and vector extensions:

```rust
use mielin_hal::capabilities::{HardwareProfile, HardwareCapabilities};

let profile = HardwareProfile::detect();

// Check for specific capabilities
if profile.has_sve2() {
    println!("Arm SVE2 available for vector operations");
    println!("Vector width: {} bits", profile.max_vector_width());
}

if profile.has_simd() {
    println!("SIMD instructions available");
}

if profile.has_npu() {
    println!("Neural Processing Unit detected!");
}

println!("CPU cores: {}", profile.core_count);
println!("Cache line: {} bytes", profile.cache_line_size);
println!("Page size: {} bytes", profile.page_size);
```

## Supported Capabilities

### CPU Vector Extensions

| Capability | Description | Platforms | Vector Width |
|------------|-------------|-----------|--------------|
| `SVE` | Scalable Vector Extension | AArch64 (Neoverse V1+) | 128-2048 bits |
| `SVE2` | SVE Version 2 | AArch64 (Neoverse V2+) | 128-2048 bits |
| `SME` | Scalable Matrix Extension | AArch64 (Neoverse V3+) | Streaming mode |
| `NEON` | Advanced SIMD | AArch64, Cortex-A/M | 128 bits |
| `AVX` | Advanced Vector Extensions | x86_64 (Intel, AMD) | 256 bits |
| `AVX2` | AVX Version 2 | x86_64 (Haswell+) | 256 bits |
| `AVX512` | 512-bit AVX | x86_64 (Xeon, EPYC) | 512 bits |
| `AMX` | Advanced Matrix Extensions | x86_64 (Sapphire Rapids+) | Tile registers |

### Other Features

| Capability | Description | Use Case |
|------------|-------------|----------|
| `FPU` | Floating Point Unit | Scientific computing |
| `CRYPTO` | Cryptographic extensions | AES, SHA acceleration |
| `ATOMICS` | Atomic operations | Lock-free concurrency |
| `NPU` | Neural Processing Unit | AI inference |
| `CRC32` | CRC32 instructions | Checksums |

## Usage Examples

### Basic Hardware Profile

```rust
use mielin_hal::capabilities::HardwareProfile;

fn main() {
    let hw = HardwareProfile::detect();

    println!("=== Hardware Profile ===");
    println!("Architecture: {:?}", hw.architecture);
    println!("Cores: {}", hw.core_count);
    println!("Cache line: {} bytes", hw.cache_line_size);
    println!("Page size: {} bytes", hw.page_size);
    println!("Memory: {} bytes", hw.memory_size); // 0 if not detected

    // Check capabilities
    println!("\n=== Capabilities ===");
    if hw.capabilities.contains(HardwareCapabilities::SVE2) {
        println!("✓ SVE2 (Scalable Vector Extension 2)");
    }
    if hw.capabilities.contains(HardwareCapabilities::SME) {
        println!("✓ SME (Scalable Matrix Extension)");
    }
    if hw.capabilities.contains(HardwareCapabilities::NEON) {
        println!("✓ NEON (Advanced SIMD)");
    }
    if hw.capabilities.contains(HardwareCapabilities::AVX512) {
        println!("✓ AVX512 (512-bit vectors)");
    }

    // Tensor operation support
    if hw.supports_tensor_ops() {
        println!("✓ Hardware-accelerated tensor ops available!");
    }

    // Get optimal vector width for algorithms
    let vector_width = hw.max_vector_width();
    println!("\nOptimal vector width: {} bits", vector_width);
}
```

### Architecture-Specific Code

```rust
use mielin_hal::Architecture;

fn optimize_for_platform(arch: Architecture) {
    match arch {
        Architecture::AArch64 => {
            // Use Arm-optimized paths
            #[cfg(target_arch = "aarch64")]
            {
                #[cfg(target_feature = "sve2")]
                {
                    // SVE2-specific implementation
                    sve2_matrix_multiply();
                }
                #[cfg(not(target_feature = "sve2"))]
                {
                    // NEON fallback
                    neon_matrix_multiply();
                }
            }
        }
        Architecture::X86_64 => {
            // Use x86-optimized paths
            #[cfg(target_arch = "x86_64")]
            {
                #[cfg(target_feature = "avx512f")]
                {
                    // AVX512-specific implementation
                    avx512_matrix_multiply();
                }
                #[cfg(target_feature = "avx2")]
                {
                    // AVX2 fallback
                    avx2_matrix_multiply();
                }
            }
        }
        Architecture::RiscV64 => {
            // RISC-V implementation
            riscv_matrix_multiply();
        }
        Architecture::ArmCortexM => {
            // Embedded scalar fallback
            scalar_matrix_multiply();
        }
    }
}
```

### Adaptive Algorithm Selection

```rust
use mielin_hal::capabilities::HardwareProfile;

fn matrix_multiply(a: &[f32], b: &[f32]) -> Vec<f32> {
    let hw = HardwareProfile::detect();

    // Select best implementation based on hardware
    match hw.max_vector_width() {
        2048 => {
            println!("Using SVE2 with 2048-bit vectors");
            matrix_multiply_sve2_2048(a, b)
        }
        1024 => {
            println!("Using SVE2 with 1024-bit vectors");
            matrix_multiply_sve2_1024(a, b)
        }
        512 => {
            println!("Using AVX512 or SVE 512-bit");
            matrix_multiply_avx512(a, b)
        }
        256 => {
            println!("Using AVX2 or SVE 256-bit");
            matrix_multiply_avx2(a, b)
        }
        128 => {
            println!("Using NEON or SSE");
            matrix_multiply_neon(a, b)
        }
        _ => {
            println!("Using scalar fallback");
            matrix_multiply_scalar(a, b)
        }
    }
}
```

## API Reference

### `Architecture`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    AArch64,      // 64-bit Arm (Cortex-A, Neoverse, Graviton)
    RiscV64,      // 64-bit RISC-V
    X86_64,       // 64-bit x86 (Intel, AMD)
    ArmCortexM,   // Arm Cortex-M embedded
}

pub fn detect_architecture() -> Architecture;
```

### `HardwareCapabilities`

Bitflags for hardware features:

```rust
bitflags! {
    pub struct HardwareCapabilities: u32 {
        const SVE       = 1 << 0;   // Scalable Vector Extension
        const SVE2      = 1 << 1;   // SVE Version 2
        const SME       = 1 << 2;   // Scalable Matrix Extension
        const NEON      = 1 << 3;   // Advanced SIMD
        const AVX       = 1 << 4;   // Advanced Vector Extensions
        const AVX2      = 1 << 5;   // AVX Version 2
        const AVX512    = 1 << 6;   // 512-bit AVX
        const FPU       = 1 << 7;   // Floating Point Unit
        const CRYPTO    = 1 << 8;   // Cryptographic extensions
        const ATOMICS   = 1 << 9;   // Atomic operations
        const CRC32     = 1 << 10;  // CRC32 instructions
        const NPU       = 1 << 11;  // Neural Processing Unit
    }
}

// Usage
let caps = HardwareCapabilities::SVE2 | HardwareCapabilities::NEON;

if caps.contains(HardwareCapabilities::SVE2) {
    // Use SVE2 instructions
}
```

### `HardwareProfile`

Complete hardware description:

```rust
pub struct HardwareProfile {
    pub architecture: Architecture,
    pub capabilities: HardwareCapabilities,
    pub core_count: usize,
    pub memory_size: usize,         // 0 if not detected
    pub cache_line_size: usize,     // Typically 64 bytes
    pub page_size: usize,           // Typically 4096 bytes
}

impl HardwareProfile {
    /// Detect hardware at runtime
    pub fn detect() -> Self;

    /// Check for specific capabilities
    pub fn has_sve2(&self) -> bool;
    pub fn has_sme(&self) -> bool;
    pub fn has_npu(&self) -> bool;
    pub fn has_simd(&self) -> bool;

    /// Check if tensor operations are supported
    pub fn supports_tensor_ops(&self) -> bool;

    /// Get maximum vector width in bits
    pub fn max_vector_width(&self) -> usize;
}
```

## Platform Support Matrix

| Feature | AArch64 | x86_64 | RISC-V | Cortex-M |
|---------|---------|--------|--------|----------|
| Architecture detection | ✅ | ✅ | ✅ | ✅ |
| CPU features (SVE, AVX, etc.) | ✅ | ✅ | ⚠️ | ⚠️ |
| Vector width detection | ✅ | ✅ | ❌ | ❌ |
| Core count | ✅ | ✅ | ✅ | ✅ (1) |
| Cache info | ✅ | ✅ | ⚠️ | ⚠️ |
| Memory size | ❌¹ | ❌¹ | ❌¹ | ❌¹ |
| NPU detection | 🚧 | 🚧 | 🚧 | 🚧 |

**Legend:**
- ✅ Fully supported
- ⚠️ Partial support or defaults
- ❌ Not implemented
- 🚧 Planned (Phase 2+)

**Notes:**
1. Memory size detection requires OS support (future enhancement)

## Performance

All detection operations are efficient:

| Operation | Time | Notes |
|-----------|------|-------|
| `detect_architecture()` | <1μs | Often compile-time constant |
| `HardwareProfile::detect()` | <10μs | Cache for best performance |
| `has_sve2()` / `has_avx512()` | <1ns | Bitflag check |
| `max_vector_width()` | <10ns | Simple computation |

**Recommendation:** Detect hardware once at startup and cache the result.

```rust
// Global cached profile (example)
lazy_static! {
    static ref HW_PROFILE: HardwareProfile = HardwareProfile::detect();
}

fn get_hardware() -> &'static HardwareProfile {
    &HW_PROFILE
}
```

## Testing

```bash
# Run all tests
cargo test -p mielin-hal

# Test specific architecture (cross-compilation)
cargo test --target aarch64-unknown-linux-gnu
cargo test --target x86_64-unknown-linux-gnu

# Test with features
cargo test --features sve2
```

**Test Coverage:**
- Architecture detection correctness
- Capability flag operations
- Vector width detection
- Profile creation and caching
- Cross-platform compatibility

## Limitations

### Current Limitations

- **Memory Size**: Detection not implemented (returns 0)
- **Core Count**: Returns 1 on embedded (no OS thread support)
- **CPU Features**: Some features may not be detected on all platforms
- **Runtime Detection**: Some features are compile-time only

### Known Issues

- Cortex-M always reports 1 core (single-core MCUs)
- Memory size requires platform-specific OS calls
- NPU detection is placeholder only

## Roadmap

### Phase 1 (v0.1 "Ranvier") ✅ Complete
- ✅ Multi-architecture support (AArch64, RISC-V, x86_64, Cortex-M)
- ✅ Hardware capability detection (SVE2, SME, NEON, AVX512)
- ✅ Vector width detection
- ✅ Core count detection

### Phase 2 (v0.2 "Oligodendrocyte") - Q1-Q2 2026
- [ ] Runtime CPU feature detection (CPUID on x86, AT_HWCAP on Linux)
- [ ] Memory size detection via platform APIs
- [ ] Cache hierarchy enumeration
- [ ] GPU enumeration (CUDA, ROCm, Metal)

### Phase 3 (v0.3 "Schwann") - Q2-Q3 2026
- [ ] NPU enumeration (Arm Ethos, Qualcomm Hexagon)
- [ ] FPGA detection
- [ ] Power/thermal state monitoring
- [ ] Battery level detection for IoT

### Phase 4 (v1.0 "Saltatory") - Q4 2026
- [ ] Performance counter access (PMU)
- [ ] Hardware health monitoring
- [ ] Dynamic frequency scaling detection
- [ ] Accelerator benchmarking

See [TODO.md](../TODO.md) for detailed roadmap.

## Advanced Usage

### Platform-Specific Optimizations

```rust
#[cfg(target_arch = "aarch64")]
fn platform_specific() {
    use mielin_hal::aarch64;

    if aarch64::has_sve2() {
        // Direct platform-specific check
        println!("SVE2 confirmed on AArch64");
    }
}

#[cfg(target_arch = "x86_64")]
fn platform_specific() {
    use mielin_hal::x86_64;

    if x86_64::has_avx512f() {
        println!("AVX512F confirmed on x86_64");
    }
}
```

### Conditional Compilation

```rust
#[cfg(any(
    target_feature = "sve2",
    target_feature = "avx512f"
))]
fn fast_path() {
    // Compiled only if SVE2 or AVX512 available
    println!("Using hardware-accelerated path");
}

#[cfg(not(any(
    target_feature = "sve2",
    target_feature = "avx512f"
)))]
fn fast_path() {
    // Fallback implementation
    println!("Using scalar fallback");
}
```

## Best Practices

1. **Detect Once**: Cache `HardwareProfile` at startup
2. **Check Features**: Always verify capabilities before using instructions
3. **Provide Fallbacks**: Support scalar paths for all platforms
4. **Use Compile-Time**: Prefer `#[cfg(target_feature)]` where possible
5. **Profile**: Measure actual performance, don't assume

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

**Key areas for contribution:**
- Runtime CPU feature detection (CPUID, AT_HWCAP)
- Memory size detection (platform-specific APIs)
- GPU/NPU enumeration
- Performance counter access
- New architecture support (RISC-V extensions, ARM v9)

## Resources

- [Main Documentation](../mielin.md) - Complete technical whitepaper
- [TODO & Roadmap](../TODO.md) - Development plan
- [Arm Architecture Reference Manual](https://developer.arm.com/documentation/)
- [Intel Software Developer Manual](https://www.intel.com/sdm)

## Contact

- **Repository**: https://github.com/cool-japan/mielin
- **Issues**: https://github.com/cool-japan/mielin/issues
- **Email**: contact@cooljapan.tech

## License

Licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE))

---

**MielinHAL** - Unified hardware abstraction enabling agents to traverse seamlessly from embedded to cloud 🧠⚡

**Current Phase:** Phase 1 "Ranvier" Complete | Targeting Phase 2 Q1-Q2 2026
