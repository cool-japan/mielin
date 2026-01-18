# Tensor Layout Conventions in MielinTensor

## Table of Contents

1. [Introduction](#introduction)
2. [Memory Layout](#memory-layout)
3. [Indexing and Strides](#indexing-and-strides)
4. [Common Tensor Shapes](#common-tensor-shapes)
5. [Layout Transformations](#layout-transformations)
6. [Broadcasting Rules](#broadcasting-rules)
7. [Performance Considerations](#performance-considerations)
8. [Interoperability](#interoperability)

## Introduction

This document defines the memory layout and indexing conventions used in mielin-tensor. Understanding these conventions is crucial for efficient tensor operations and interoperability with other frameworks.

### Design Principles

1. **Row-Major Layout**: Follow C/Rust conventions (compatible with NumPy default)
2. **Contiguous Storage**: Sequential memory for cache efficiency
3. **Zero-Copy Views**: Enable efficient slicing without data duplication
4. **SIMD-Friendly**: Align data and operations for vectorization

## Memory Layout

### Row-Major (C-style) Layout

MielinTensor uses **row-major** layout by default, where the last dimension varies fastest:

```
Shape: [2, 3]
Memory: [a₀₀, a₀₁, a₀₂, a₁₀, a₁₁, a₁₂]

Visual:
┌─────────────┐
│ a₀₀ a₀₁ a₀₂ │ ← Row 0 (contiguous)
│ a₁₀ a₁₁ a₁₂ │ ← Row 1 (contiguous)
└─────────────┘
```

**Rationale**:
- Compatible with Rust's natural array layout
- Matches NumPy's default behavior
- Efficient for typical ML workloads (batch-first)

### Contiguous Storage

All tensors store data in a single contiguous `Vec<T>`:

```rust
pub struct Tensor<T> {
    data: Vec<T>,      // Contiguous memory
    shape: Vec<usize>, // Dimensions
    strides: Vec<usize>, // Index multipliers
}
```

**Benefits**:
- Cache-friendly sequential access
- Efficient SIMD operations
- Simple memory management
- Easy serialization

## Indexing and Strides

### Stride Calculation

Strides determine how multi-dimensional indices map to flat memory:

```rust
fn compute_strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = Vec::with_capacity(shape.len());
    let mut stride = 1;

    // Iterate from last to first dimension
    for &dim in shape.iter().rev() {
        strides.push(stride);
        stride *= dim;
    }

    strides.reverse();
    strides
}
```

**Example**:

```
Shape:   [2, 3, 4]
Strides: [12, 4, 1]

Explanation:
- Stride[0] = 3 × 4 = 12  (skip to next batch)
- Stride[1] = 4           (skip to next row)
- Stride[2] = 1           (skip to next element)
```

### Flat Index Computation

Multi-dimensional index to flat memory offset:

```rust
fn flat_index(indices: &[usize], strides: &[usize]) -> usize {
    indices.iter()
        .zip(strides.iter())
        .map(|(&idx, &stride)| idx * stride)
        .sum()
}
```

**Example**:

```
Shape: [2, 3, 4]
Indices: [1, 2, 3]
Strides: [12, 4, 1]

Flat index = 1×12 + 2×4 + 3×1 = 12 + 8 + 3 = 23
```

### Multi-Dimensional Access

```rust
// Create a 3D tensor
let mut tensor = Tensor::zeros(vec![2, 3, 4]);

// Set element at [1, 2, 3]
tensor.set(&[1, 2, 3], 42.0);

// Get element at [1, 2, 3]
let value = tensor.get(&[1, 2, 3]); // Some(42.0)
```

## Common Tensor Shapes

### Scalar (0D)

```rust
let scalar = Tensor::scalar(42.0);

Shape: [1]
Memory: [42.0]
```

**Note**: Scalars are represented as 1-element 1D tensors for simplicity.

### Vector (1D)

```rust
let vector = Tensor::vector(vec![1.0, 2.0, 3.0]);

Shape: [3]
Strides: [1]
Memory: [1.0, 2.0, 3.0]

Access:
vector[i] → data[i]
```

### Matrix (2D)

```rust
let matrix = Tensor::matrix(vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();

Shape: [2, 2]
Strides: [2, 1]
Memory: [1.0, 2.0, 3.0, 4.0]

Visual:
┌─────┐
│ 1 2 │
│ 3 4 │
└─────┘

Access:
matrix[i, j] → data[i × 2 + j]
```

### 3D Tensor

```rust
let tensor3d = Tensor::tensor3d(data, depth, rows, cols).unwrap();

Shape: [depth, rows, cols]
Strides: [rows × cols, cols, 1]

Example: [2, 3, 4]
Strides: [12, 4, 1]

Use cases:
- Grayscale image batch: [batch, height, width]
- RGB image: [channels=3, height, width]
- Time series: [time, features, batch]
```

### 4D Tensor

```rust
let tensor4d = Tensor::tensor4d(data, batch, channels, height, width).unwrap();

Shape: [batch, channels, height, width]
Strides: [channels × height × width, height × width, width, 1]

Example: [2, 3, 32, 32]  // 2 RGB images of 32×32
Strides: [3072, 1024, 32, 1]

Use cases:
- Image batches: [batch, channels, height, width]
- Feature maps: [batch, filters, height, width]
```

### 5D Tensor

```rust
let tensor5d = Tensor::tensor5d(data, batch, time, channels, height, width).unwrap();

Shape: [batch, time, channels, height, width]

Use cases:
- Video batches: [batch, frames, channels, height, width]
- Temporal features: [batch, time, features, height, width]
```

### 6D+ Tensors

```rust
let tensor6d = Tensor::tensor6d(data, d1, d2, d3, d4, d5, d6).unwrap();

// Generic N-dimensional tensors
let tensor = Tensor::zeros(vec![d1, d2, d3, ..., dn]);
```

**Supported**: Arbitrary dimensions through generic `Vec<usize>` shape.

## Layout Transformations

### Reshape

Change shape while preserving data order:

```rust
let mut tensor = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

tensor.reshape(vec![2, 3]).unwrap();

Before: [6]          → [1, 2, 3, 4, 5, 6]
After:  [2, 3]       → │1 2 3│
                       │4 5 6│

// Total elements must match
tensor.reshape(vec![3, 2]).unwrap(); // OK
tensor.reshape(vec![2, 2]); // None (size mismatch)
```

**Rules**:
- Total size must remain constant: `∏ old_shape = ∏ new_shape`
- Data is reinterpreted, not copied
- Strides are recomputed

### Transpose

Swap dimensions:

```rust
use mielin_tensor::matrix::Matrix;

let m = Matrix::new(vec![
    1.0, 2.0,
    3.0, 4.0,
], 2, 2);

let mt = m.transpose();

Original:     Transposed:
┌─────┐      ┌─────┐
│ 1 2 │      │ 1 3 │
│ 3 4 │      │ 2 4 │
└─────┘      └─────┘
```

**Performance Note**: Transpose creates a new tensor with copied data. For efficiency, use transposed operations (e.g., BLAS `gemm` with transpose flags) when possible.

### Views and Slicing

Create views without copying data:

```rust
use mielin_tensor::view::{TensorView, SliceRange};

let tensor = Tensor::zeros(vec![4, 4]);

// Create view of subregion [0:2, 0:2]
let view = TensorView::new(&tensor, &[
    SliceRange::new(0, 2),
    SliceRange::new(0, 2),
]);

// View shares underlying data
assert_eq!(view.ndim(), 2);
```

**Benefits**:
- Zero-copy slicing
- Memory-efficient
- Allows in-place operations on subregions

## Broadcasting Rules

MielinTensor follows NumPy-style broadcasting for element-wise operations:

### Rule 1: Dimension Alignment

Dimensions are aligned from right to left:

```
a: [3, 1, 4]
b:    [2, 4]
→     [3, 2, 4]  (broadcast result)
```

### Rule 2: Size 1 Expansion

Dimensions with size 1 are stretched:

```
a: [3, 1, 4]  →  [3, 2, 4]
b: [3, 2, 4]  →  [3, 2, 4]
```

### Rule 3: Missing Dimensions

Missing dimensions are treated as size 1:

```
a: [4]        →  [1, 1, 4]  →  [3, 2, 4]
b: [3, 2, 4]  →  [3, 2, 4]  →  [3, 2, 4]
```

### Examples

**Valid broadcasts**:

```rust
// Scalar + Vector
let a = Tensor::scalar(5.0);      // [1]
let b = Tensor::vector(vec![1.0, 2.0, 3.0]); // [3]
let c = a.add(&b);                 // [3] = [5, 5, 5] + [1, 2, 3]

// Vector + Matrix
let a = Tensor::vector(vec![1.0, 2.0]); // [2]
let b = Tensor::matrix(vec![...], 3, 2); // [3, 2]
// Broadcast a from [2] to [1, 2] to [3, 2]
let c = a.add(&b);                 // [3, 2]
```

**Invalid broadcasts** (require manual reshaping):

```rust
let a = Tensor::zeros(vec![3, 4]);
let b = Tensor::zeros(vec![4, 3]);
// Cannot broadcast [3, 4] and [4, 3]
// let c = a.add(&b); // Shape mismatch panic
```

## Performance Considerations

### 1. Contiguous Access

Access tensors in row-major order for cache efficiency:

```rust
// GOOD: Row-major traversal (cache-friendly)
for i in 0..rows {
    for j in 0..cols {
        process(tensor.get(&[i, j]));
    }
}

// BAD: Column-major traversal (cache unfriendly)
for j in 0..cols {
    for i in 0..rows {
        process(tensor.get(&[i, j])); // Strided access!
    }
}
```

### 2. Alignment

SIMD operations benefit from aligned data:

```rust
use mielin_tensor::pool::{SIMD_ALIGNMENT, TENSOR_POOL};

// Allocate with 64-byte alignment
let buffer = TENSOR_POOL.allocate_aligned(size, SIMD_ALIGNMENT);
```

**Alignment requirements**:
- NEON: 16 bytes (4 × f32)
- AVX2: 32 bytes (8 × f32)
- AVX-512: 64 bytes (16 × f32)

### 3. Memory Layout for Operations

**Matrix Multiplication** (C = A × B):
```
A: [m, k]  (row-major)
B: [k, n]  (row-major)
C: [m, n]  (row-major)

Efficient: Rows of A × Columns of B
A contiguous, B requires column access (less efficient)

Solution: Use cache-blocked algorithms
```

**Convolution** (NCHW vs NHWC):
```
NCHW: [batch, channels, height, width]  ← MielinTensor default
NHWC: [batch, height, width, channels]

NCHW advantages:
- Channel-contiguous for SIMD
- Easier depthwise separable conv
- Better for image transformations

NHWC advantages:
- Pixel-contiguous
- Some hardware (TPU) prefers this
```

### 4. Avoiding Copies

Use views for zero-copy operations:

```rust
// BAD: Creates copy
let sub = tensor.clone();
process(&sub);

// GOOD: Zero-copy view
let view = TensorView::new(&tensor, &[
    SliceRange::new(0, 10),
    SliceRange::all(),
]);
process_view(&view);
```

## Interoperability

### NumPy Compatibility

MielinTensor uses the same row-major layout as NumPy default:

```python
import numpy as np

# NumPy (row-major by default)
a = np.array([[1, 2, 3],
              [4, 5, 6]])

print(a.strides)  # (12, 4) in bytes → (3, 1) in elements
```

```rust
// MielinTensor (equivalent)
let a = Tensor::matrix(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
// Strides: [3, 1]
```

**Conversion**:
```python
# Export from NumPy
data = a.flatten().tolist()  # Row-major order
shape = list(a.shape)

# Import to MielinTensor
let tensor = Tensor::from_vec(data, shape).unwrap();
```

### PyTorch Compatibility

PyTorch also uses row-major (C-contiguous) by default:

```python
import torch

a = torch.tensor([[1, 2, 3],
                  [4, 5, 6]])

print(a.stride())  # (3, 1)
```

Directly compatible with MielinTensor layout.

### TensorFlow Compatibility

TensorFlow uses row-major for most operations:

```python
import tensorflow as tf

a = tf.constant([[1, 2, 3],
                 [4, 5, 6]])

# Row-major storage (compatible)
```

**Note**: Some TensorFlow operations default to NHWC for images, while MielinTensor uses NCHW. Explicit transposition may be needed.

## Summary

### Key Points

| Aspect | Convention |
|--------|------------|
| **Layout** | Row-major (C-style) |
| **Stride Order** | Descending (last dimension fastest) |
| **Indexing** | `data[i₀×s₀ + i₁×s₁ + ... + iₙ×sₙ]` |
| **Alignment** | 64-byte for SIMD |
| **Broadcasting** | NumPy-compatible |
| **Image Format** | NCHW (batch, channels, height, width) |

### Best Practices

1. ✅ Access tensors in row-major order
2. ✅ Use views for zero-copy slicing
3. ✅ Align large tensors to 64 bytes
4. ✅ Prefer NCHW for image data
5. ✅ Use cache-blocked algorithms for large matrices
6. ✅ Verify shapes before operations

### Quick Reference

```rust
// Scalar
Tensor::scalar(42.0)              // [1]

// Vector
Tensor::vector(vec![1.0, 2.0])    // [2]

// Matrix
Tensor::matrix(data, rows, cols)   // [rows, cols]

// 3D
Tensor::tensor3d(data, d, r, c)    // [d, r, c]

// 4D
Tensor::tensor4d(data, b, d, r, c) // [b, d, r, c]

// 5D
Tensor::tensor5d(data, b, t, d, r, c) // [b, t, d, r, c]

// 6D
Tensor::tensor6d(data, d1, d2, d3, d4, d5, d6)

// N-D
Tensor::zeros(vec![d1, d2, ..., dn])
```

## See Also

- [SIMD Programming Guide](./SIMD_GUIDE.md) - SIMD optimization with layout considerations
- [Performance Tuning Guide](./PERFORMANCE_TUNING.md) - Cache-aware algorithms
- [API Documentation](../src/tensor.rs) - Implementation details

---

**Last Updated**: 2026-01-18
**Maintainers**: COOLJAPAN OU (Team Kitasan)
