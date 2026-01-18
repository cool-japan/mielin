# mielin-tensor

**TensorLogic - Kernel-Level Tensor Operations**

Kernel integration for hardware-accelerated tensor operations leveraging Arm SVE2/SME and other accelerators.

## Overview

MielinTensor provides kernel-level awareness of tensor/matrix operations, enabling efficient scheduling and resource allocation for AI workloads on MielinOS.

## Features

- **Hardware Detection**: Automatic detection of SVE2, SME, NPU
- **Resource Management**: Memory budget tracking for tensor operations
- **Accelerator Abstraction**: Unified interface for various accelerators
- **Zero-Copy**: Direct hardware access where possible

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-tensor = { path = "../mielin-tensor" }
```

### Basic Usage

```rust
use mielin_tensor::TensorRuntime;
use mielin_hal::capabilities::HardwareCapabilities;

// Create runtime with detected capabilities
let caps = HardwareCapabilities::SVE2 | HardwareCapabilities::NEON;
let runtime = TensorRuntime::new(caps);

// Check for specific features
if runtime.supports_sve2() {
    println!("SVE2 available for vector operations");
}

if runtime.supports_sme() {
    println!("SME available for matrix operations");
}
```

## API Reference

### `TensorRuntime`

```rust
pub struct TensorRuntime {
    capabilities: HardwareCapabilities,
}

impl TensorRuntime {
    pub fn new(capabilities: HardwareCapabilities) -> Self;
    pub fn supports_sve2(&self) -> bool;
    pub fn supports_sme(&self) -> bool;
}
```

### `Accelerator`

```rust
pub struct Accelerator {
    pub name: &'static str,
}

impl Accelerator {
    pub const fn new(name: &'static str) -> Self;
}
```

### `TensorScheduler`

```rust
pub struct TensorScheduler {}

impl TensorScheduler {
    pub fn new() -> Self;
    pub fn schedule_matmul(&self, size: usize);
}
```

## Examples

### Detect Capabilities

```rust
use mielin_tensor::TensorRuntime;
use mielin_hal::capabilities::HardwareProfile;

let hw = HardwareProfile::detect();
let runtime = TensorRuntime::new(hw.capabilities);

if runtime.supports_sve2() {
    // Use SVE2 for operations
    perform_sve2_matmul();
} else {
    // Fallback to scalar
    perform_scalar_matmul();
}
```

### Schedule Matrix Multiplication

```rust
use mielin_tensor::scheduler::TensorScheduler;

let scheduler = TensorScheduler::new();

// Schedule a 1024x1024 matrix multiplication
scheduler.schedule_matmul(1024);
```

## Supported Accelerators

### Arm SVE2 (Scalable Vector Extension 2)

- Variable vector length (128-2048 bits)
- Predication for complex data patterns
- Gather/scatter operations

### Arm SME (Scalable Matrix Extension)

- Dedicated matrix operations
- Streaming SVE mode
- Optimized for AI/ML workloads

### NPU (Neural Processing Unit)

- Specialized AI accelerator
- Integer and floating-point operations
- Low power consumption

## Performance Characteristics

Performance estimates for 1024x1024 matrix multiplication:

| Platform | Time | Notes |
|----------|------|-------|
| Scalar (no SIMD) | ~1000 ms | Baseline |
| NEON (128-bit) | ~100 ms | 10x speedup |
| SVE2 (256-bit) | ~50 ms | 20x speedup |
| SVE2 (512-bit) | ~25 ms | 40x speedup |
| SME | ~10 ms | 100x speedup |
| NPU | ~5 ms | 200x speedup |

## Integration with Kernel

TensorLogic integrates with the MielinOS kernel:

```rust
use mielin_kernel::tensor::TensorContext;

// Create context with memory budget
let ctx = TensorContext::new(10 * 1024 * 1024); // 10 MB

// Context provides:
// - Memory allocation tracking
// - Resource limits
// - Priority scheduling
```

## Resource Management

### Memory Budget

```rust
use mielin_kernel::tensor::TensorContext;

let ctx = TensorContext::new(memory_budget);
// Kernel ensures tensor ops don't exceed budget
```

### Operation Scheduling

Future: Kernel-level scheduler for tensor operations:
- Priority queuing
- Resource reservation
- Power-aware scheduling

## Future Enhancements

- [ ] Actual SVE2/SME code generation
- [ ] NPU driver integration
- [ ] Operation fusion optimization
- [ ] Quantization support (INT8, INT4)
- [ ] Distributed tensor operations
- [ ] GPU compute support
- [ ] Operator caching and reuse

## Platform Support

### Arm Platforms

- **Neoverse V1/V2**: SVE support
- **Neoverse V3**: SVE2 + SME
- **Cortex-A710**: NEON + SVE
- **Future Arm CPUs**: Full SME support

### x86 Platforms (Future)

- **AVX-512**: 512-bit vector ops
- **AMX**: Advanced Matrix Extensions
- **AVX-VNNI**: Vector neural network instructions

## Testing

```bash
cargo test -p mielin-tensor
```

## Limitations

Current implementation is foundational:
- No actual hardware codegen yet
- Scheduling is placeholder
- NPU support is stub only

This crate provides the structure for future TensorLogic integration.

## Research Direction

Active research areas:
- Optimal operator scheduling
- Power-performance tradeoffs
- Distributed inference strategies
- Model partitioning across heterogeneous nodes

## License

MIT OR Apache-2.0
