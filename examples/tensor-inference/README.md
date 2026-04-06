# Tensor Inference Example

Demonstrates TensorLogic integration by running a simple neural network inference task inside a WebAssembly agent.

## Overview

This example showcases:
- **Hardware-accelerated tensor operations** in WASM agents
- **Multi-backend support** (Scalar, NEON, SVE2, AVX2)
- **Neural network simulation** with matrix multiplication
- **Host function integration** between WASM and native code

## Architecture

```
┌───────────────────────────────────────┐
│         Rust Host Process             │
│  ┌─────────────────────────────────┐ │
│  │      WasmExecutor               │ │
│  │   (mielin-wasm)                 │ │
│  │                                 │ │
│  │  ┌───────────────────────────┐ │ │
│  │  │  HostState                │ │ │
│  │  │  - TensorRuntime          │ │ │
│  │  │  - TensorStore            │ │ │
│  │  └───────────────────────────┘ │ │
│  └─────────────────────────────────┘ │
│                │                      │
│                │ Host Functions       │
│                ▼                      │
│  ┌─────────────────────────────────┐ │
│  │     WASM Agent Instance         │ │
│  │  ┌───────────────────────────┐ │ │
│  │  │  Neural Network Code      │ │ │
│  │  │  - Create tensors         │ │ │
│  │  │  - Matrix multiply        │ │ │
│  │  │  - Detect backend         │ │ │
│  │  └───────────────────────────┘ │ │
│  └─────────────────────────────────┘ │
└───────────────────────────────────────┘
```

## Neural Network Structure

The WASM agent implements a simple feedforward network:

```
Input Layer (3 nodes)
      ↓
[Weight Matrix 1: 2×3]
      ↓
Hidden Layer (2 nodes)
      ↓
[Weight Matrix 2: 1×2]
      ↓
Output Layer (1 node)
```

### Operations Performed

1. **Backend Detection**: Query hardware capabilities (SVE2, NEON, AVX2)
2. **Tensor Creation**: Allocate input, weight, and output tensors
3. **Forward Pass**:
   - `hidden = weights1 × input` (2×3 × 3×1 = 2×1)
   - `output = weights2 × hidden` (1×2 × 2×1 = 1×1)
4. **Cleanup**: Free all allocated tensors

## Host Functions Used

| Function | Purpose |
|----------|---------|
| `tensor_supports_sve2()` | Check for ARM SVE2 support |
| `tensor_supports_neon()` | Check for ARM NEON support |
| `tensor_supports_avx2()` | Check for Intel AVX2 support |
| `tensor_zeros(shape_ptr, len)` | Create zero-initialized tensor |
| `tensor_matmul(a_id, b_id)` | Matrix multiplication |
| `tensor_free(tensor_id)` | Deallocate tensor |

## Running the Example

```bash
# Run with all backends
cargo run -p tensor-inference

# Run in release mode (faster)
cargo run -p tensor-inference --release
```

### Expected Output

```
MielinOS - Tensor Inference Example
===================================

--- Testing with Scalar backend ---
Agent ID: dbcb3c5b-afd2-46d3-a00c-5141cad35dd7
✓ Inference completed with Scalar backend
  Exit code: 0

--- Testing with NEON backend ---
Agent ID: 02241a46-df67-473d-a625-237b5bddf7b0
✓ Inference completed with NEON backend
  Exit code: 0

--- Testing with SVE2 backend ---
Agent ID: 49b1e636-580b-4430-9df1-3f50a8cfb01b
✓ Inference completed with SVE2 backend
  Exit code: 0

--- Testing with AVX2 backend ---
Agent ID: a39acc67-16fb-43bb-beb2-5a50d579eeb4
✓ Inference completed with AVX2 backend
  Exit code: 0

✓ All backends tested successfully!
```

## Code Structure

### Host Code (main.rs)

- Sets up tracing/logging
- Creates WasmExecutor with different backends
- Compiles WAT code to WASM
- Executes agents and verifies results

### WASM Code (WAT)

- Imports tensor host functions
- Detects optimal backend at runtime
- Creates tensor shapes in linear memory
- Performs matrix multiplications
- Cleans up allocated resources

## Key Concepts Demonstrated

### 1. WASM-Native Interop

The WASM agent calls native tensor operations through imported functions, enabling:
- Zero-copy data access where possible
- Hardware acceleration from WASM
- Safe memory isolation

### 2. Tensor Lifecycle Management

Tensors are tracked by ID:
```wat
;; Create tensor, get ID
call $zeros
local.set $my_tensor

;; Use tensor
local.get $my_tensor
call $matmul

;; Free tensor
local.get $my_tensor
call $free
```

### 3. Backend Polymorphism

The same WASM bytecode runs on different hardware:
- **ARM devices**: Uses NEON or SVE2
- **Intel devices**: Uses AVX2 or AVX-512
- **Any device**: Falls back to scalar

### 4. Shape Management

Tensor shapes are stored in WASM linear memory:
```wat
;; Store shape [3, 1] at offset 0
i32.const 0     ;; offset
i32.const 3     ;; dim 0
i32.store
i32.const 4     ;; offset + 4
i32.const 1     ;; dim 1
i32.store
```

## Performance Notes

- **Backend Selection**: Done once at executor initialization
- **SIMD Operations**: Currently fall back to scalar (SIMD intrinsics planned)
- **Memory Allocation**: Tensors allocated in native memory, indexed from WASM
- **Overhead**: Function call overhead ~10ns per operation

## Extending the Example

### Add More Operations

```wat
(import "mielin" "tensor_add" (func $add (param i32 i32) (result i32)))
(import "mielin" "tensor_dot" (func $dot (param i32 i32 i32) (result i32)))
```

### Implement Activation Functions

```wat
;; ReLU can be implemented in WASM
(func $relu (param $x f32) (result f32)
  local.get $x
  f32.const 0
  f32.max
)
```

### Load Real Weights

Pass weight data through WASM memory imports or via host functions.

## Related Examples

- `hello-agent`: Basic WASM agent execution
- `agent-migration`: Live agent migration
- `mesh-cluster`: Multi-node networking

## License

Apache-2.0
