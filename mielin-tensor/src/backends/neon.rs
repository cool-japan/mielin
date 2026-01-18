//! ARM NEON SIMD intrinsics for tensor operations
//!
//! NEON provides 128-bit SIMD vectors that can hold 4 x f32 values.
//! This module implements hardware-accelerated tensor operations for AArch64.

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

/// NEON-optimized dot product
///
/// Processes 4 elements at a time using 128-bit NEON registers.
/// Falls back to scalar for remaining elements.
///
/// # Safety
///
/// This function is unsafe because it uses NEON intrinsics which require
/// the CPU to support NEON instructions. Caller must ensure the target
/// architecture is AArch64 with NEON support.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn dot_neon(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let len = a.len();
    let chunks = len / 4;
    let remainder = len % 4;

    // Accumulator vector (4 x f32)
    let mut acc = vdupq_n_f32(0.0);

    // Process 4 elements at a time
    for i in 0..chunks {
        let offset = i * 4;

        // Load 4 floats from each array
        let va = vld1q_f32(a.as_ptr().add(offset));
        let vb = vld1q_f32(b.as_ptr().add(offset));

        // Multiply and accumulate: acc += va * vb
        acc = vmlaq_f32(acc, va, vb);
    }

    // Horizontal sum of the accumulator
    let mut sum = vaddvq_f32(acc);

    // Handle remaining elements with scalar code
    let base = chunks * 4;
    for i in 0..remainder {
        sum += a[base + i] * b[base + i];
    }

    sum
}

/// NEON-optimized element-wise addition
///
/// Adds two arrays element-wise using 128-bit NEON vectors.
///
/// # Safety
///
/// Requires AArch64 architecture with NEON support. Caller must ensure
/// all arrays have the same length.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn add_neon(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 4;
    let remainder = len % 4;

    // Process 4 elements at a time
    for i in 0..chunks {
        let offset = i * 4;

        let va = vld1q_f32(a.as_ptr().add(offset));
        let vb = vld1q_f32(b.as_ptr().add(offset));

        // Add vectors
        let vr = vaddq_f32(va, vb);

        // Store result
        vst1q_f32(result.as_mut_ptr().add(offset), vr);
    }

    // Handle remaining elements
    let base = chunks * 4;
    for i in 0..remainder {
        result[base + i] = a[base + i] + b[base + i];
    }
}

/// NEON-optimized matrix-vector multiplication
///
/// Optimized row-major matrix-vector product using NEON.
/// Used as building block for full matrix multiplication.
///
/// # Safety
///
/// Requires AArch64 with NEON support. Caller must ensure matrix length
/// equals rows * cols, vector length equals cols, and result length equals rows.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe fn matvec_neon(
    matrix: &[f32],
    vector: &[f32],
    result: &mut [f32],
    rows: usize,
    cols: usize,
) {
    debug_assert_eq!(matrix.len(), rows * cols);
    debug_assert_eq!(vector.len(), cols);
    debug_assert_eq!(result.len(), rows);

    let chunks = cols / 4;
    let remainder = cols % 4;

    for (i, result_elem) in result.iter_mut().enumerate().take(rows) {
        let row_offset = i * cols;
        let mut acc = vdupq_n_f32(0.0);

        // Process 4 elements per iteration
        for j in 0..chunks {
            let offset = row_offset + j * 4;

            let vm = vld1q_f32(matrix.as_ptr().add(offset));
            let vv = vld1q_f32(vector.as_ptr().add(j * 4));

            acc = vmlaq_f32(acc, vm, vv);
        }

        // Horizontal sum
        let mut sum = vaddvq_f32(acc);

        // Handle remainder
        let base = chunks * 4;
        for j in 0..remainder {
            sum += matrix[row_offset + base + j] * vector[base + j];
        }

        *result_elem = sum;
    }
}

// Scalar fallback for non-AArch64 platforms
#[cfg(not(target_arch = "aarch64"))]
pub fn dot_neon(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(not(target_arch = "aarch64"))]
pub fn add_neon(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] + b[i];
    }
}

#[cfg(not(target_arch = "aarch64"))]
pub fn matvec_neon(matrix: &[f32], vector: &[f32], result: &mut [f32], rows: usize, cols: usize) {
    for i in 0..rows {
        let mut sum = 0.0;
        for j in 0..cols {
            sum += matrix[i * cols + j] * vector[j];
        }
        result[i] = sum;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_neon_dot() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0];

        #[cfg(target_arch = "aarch64")]
        let result = unsafe { dot_neon(&a, &b) };

        #[cfg(not(target_arch = "aarch64"))]
        let result = dot_neon(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 = 2 + 6 + 12 + 20 + 30 = 70
        assert_eq!(result, 70.0);
    }

    #[test]
    fn test_neon_add() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = std::vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let mut result = std::vec![0.0; 5];

        #[cfg(target_arch = "aarch64")]
        unsafe {
            add_neon(&a, &b, &mut result)
        };

        #[cfg(not(target_arch = "aarch64"))]
        add_neon(&a, &b, &mut result);

        assert_eq!(result, std::vec![11.0, 22.0, 33.0, 44.0, 55.0]);
    }

    #[test]
    fn test_neon_matvec() {
        // 2x3 matrix
        let matrix = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0,];
        let vector = std::vec![1.0, 2.0, 3.0];
        let mut result = std::vec![0.0; 2];

        #[cfg(target_arch = "aarch64")]
        unsafe {
            matvec_neon(&matrix, &vector, &mut result, 2, 3)
        };

        #[cfg(not(target_arch = "aarch64"))]
        matvec_neon(&matrix, &vector, &mut result, 2, 3);

        // Row 0: 1*1 + 2*2 + 3*3 = 1 + 4 + 9 = 14
        // Row 1: 4*1 + 5*2 + 6*3 = 4 + 10 + 18 = 32
        assert_eq!(result, std::vec![14.0, 32.0]);
    }
}
