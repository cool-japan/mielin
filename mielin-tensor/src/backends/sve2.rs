//! ARM SVE2 SIMD intrinsics for tensor operations
//!
//! SVE2 (Scalable Vector Extension 2) provides vector-length agnostic SIMD.
//! Vector length can be 128 to 2048 bits depending on hardware.
//! This module implements hardware-accelerated tensor operations for AArch64 SVE2.
//!
//! Key advantages over NEON:
//! - Scalable vector length (hardware determines at runtime)
//! - Predicated operations for efficient masking
//! - Gather/scatter for sparse operations
//! - Enhanced gather/scatter and complex number support in SVE2

#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
use core::arch::aarch64::*;

/// Get SVE vector length in f32 elements
///
/// Returns the number of 32-bit elements that fit in one SVE vector.
/// This is determined by hardware and varies between implementations.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[inline(always)]
unsafe fn sve_f32_count() -> usize {
    // svcntw returns the number of 32-bit elements in an SVE vector
    svcntw() as usize
}

/// SVE2-optimized dot product
///
/// Processes vector-length-agnostic chunks using SVE predicates.
/// Falls back to scalar for remaining elements.
///
/// # Safety
///
/// Requires AArch64 architecture with SVE2 support. Caller must ensure
/// arrays have equal length.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn dot_sve2(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    // Accumulator vector
    let mut acc = svdup_n_f32(0.0);

    // Process full vector-width chunks
    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32(); // All-true predicate for full vector

        // Load vl floats from each array
        let va = svld1_f32(pg, a.as_ptr().add(offset));
        let vb = svld1_f32(pg, b.as_ptr().add(offset));

        // Fused multiply-accumulate
        acc = svmla_f32_m(pg, acc, va, vb);
    }

    // Horizontal sum
    let pg = svptrue_b32();
    let mut sum = svaddv_f32(pg, acc);

    // Handle remaining elements with scalar code
    let base = chunks * vl;
    for i in 0..remainder {
        sum += a[base + i] * b[base + i];
    }

    sum
}

/// SVE2-optimized element-wise addition
///
/// Adds two arrays element-wise using scalable vectors.
///
/// # Safety
///
/// Requires AArch64 with SVE2 support. All arrays must have equal length.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn add_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    // Process full vector-width chunks
    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();

        let va = svld1_f32(pg, a.as_ptr().add(offset));
        let vb = svld1_f32(pg, b.as_ptr().add(offset));

        // Add vectors
        let vr = svadd_f32_m(pg, va, vb);

        // Store result
        svst1_f32(pg, result.as_mut_ptr().add(offset), vr);
    }

    // Handle remaining elements
    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] + b[base + i];
    }
}

/// SVE2-optimized element-wise multiplication
///
/// Multiplies two arrays element-wise using scalable vectors.
///
/// # Safety
///
/// Requires AArch64 with SVE2 support. All arrays must have equal length.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn mul_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();

        let va = svld1_f32(pg, a.as_ptr().add(offset));
        let vb = svld1_f32(pg, b.as_ptr().add(offset));

        let vr = svmul_f32_m(pg, va, vb);

        svst1_f32(pg, result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] * b[base + i];
    }
}

/// SVE2-optimized matrix-vector multiplication
///
/// Optimized row-major matrix-vector product using SVE2.
///
/// # Safety
///
/// Requires AArch64 with SVE2 support. Matrix length must equal rows * cols,
/// vector length must equal cols, and result length must equal rows.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn matvec_sve2(
    matrix: &[f32],
    vector: &[f32],
    result: &mut [f32],
    rows: usize,
    cols: usize,
) {
    debug_assert_eq!(matrix.len(), rows * cols);
    debug_assert_eq!(vector.len(), cols);
    debug_assert_eq!(result.len(), rows);

    let vl = sve_f32_count();
    let chunks = cols / vl;
    let remainder = cols % vl;

    for i in 0..rows {
        let row_offset = i * cols;
        let mut acc = svdup_n_f32(0.0);

        // Process full vector-width chunks
        for j in 0..chunks {
            let offset = row_offset + j * vl;
            let pg = svptrue_b32();

            let vm = svld1_f32(pg, matrix.as_ptr().add(offset));
            let vv = svld1_f32(pg, vector.as_ptr().add(j * vl));

            acc = svmla_f32_m(pg, acc, vm, vv);
        }

        // Horizontal sum
        let pg = svptrue_b32();
        let mut sum = svaddv_f32(pg, acc);

        // Handle remainder
        let base = chunks * vl;
        for j in 0..remainder {
            sum += matrix[row_offset + base + j] * vector[base + j];
        }

        result[i] = sum;
    }
}

/// SVE2-optimized fused multiply-add: result = a * b + c
///
/// # Safety
///
/// Requires AArch64 with SVE2 support. All arrays must have equal length.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn fma_sve2(a: &[f32], b: &[f32], c: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();

        let va = svld1_f32(pg, a.as_ptr().add(offset));
        let vb = svld1_f32(pg, b.as_ptr().add(offset));
        let vc = svld1_f32(pg, c.as_ptr().add(offset));

        // FMA: vc + va * vb
        let vr = svmla_f32_m(pg, vc, va, vb);

        svst1_f32(pg, result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] * b[base + i] + c[base + i];
    }
}

/// SVE2-optimized scalar multiply: result = a * scalar
///
/// # Safety
///
/// Requires AArch64 with SVE2 support.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn scale_sve2(a: &[f32], scalar: f32, result: &mut [f32]) {
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    let vs = svdup_n_f32(scalar);

    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();

        let va = svld1_f32(pg, a.as_ptr().add(offset));
        let vr = svmul_f32_m(pg, va, vs);

        svst1_f32(pg, result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] * scalar;
    }
}

/// SVE2-optimized sum reduction
///
/// Returns the sum of all elements in the array.
///
/// # Safety
///
/// Requires AArch64 with SVE2 support.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn sum_sve2(a: &[f32]) -> f32 {
    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    let mut acc = svdup_n_f32(0.0);

    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();
        let va = svld1_f32(pg, a.as_ptr().add(offset));
        acc = svadd_f32_m(pg, acc, va);
    }

    let pg = svptrue_b32();
    let mut sum = svaddv_f32(pg, acc);

    let base = chunks * vl;
    for i in 0..remainder {
        sum += a[base + i];
    }

    sum
}

/// SVE2-optimized max reduction
///
/// Returns the maximum value in the array.
///
/// # Safety
///
/// Requires AArch64 with SVE2 support. Array must be non-empty.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn max_sve2(a: &[f32]) -> f32 {
    debug_assert!(!a.is_empty());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    // Initialize with negative infinity
    let mut acc = svdup_n_f32(f32::NEG_INFINITY);

    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();
        let va = svld1_f32(pg, a.as_ptr().add(offset));
        acc = svmax_f32_m(pg, acc, va);
    }

    let pg = svptrue_b32();
    let mut max_val = svmaxv_f32(pg, acc);

    let base = chunks * vl;
    for i in 0..remainder {
        if a[base + i] > max_val {
            max_val = a[base + i];
        }
    }

    max_val
}

/// SVE2-optimized min reduction
///
/// Returns the minimum value in the array.
///
/// # Safety
///
/// Requires AArch64 with SVE2 support. Array must be non-empty.
#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn min_sve2(a: &[f32]) -> f32 {
    debug_assert!(!a.is_empty());

    let len = a.len();
    let vl = sve_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    let mut acc = svdup_n_f32(f32::INFINITY);

    for i in 0..chunks {
        let offset = i * vl;
        let pg = svptrue_b32();
        let va = svld1_f32(pg, a.as_ptr().add(offset));
        acc = svmin_f32_m(pg, acc, va);
    }

    let pg = svptrue_b32();
    let mut min_val = svminv_f32(pg, acc);

    let base = chunks * vl;
    for i in 0..remainder {
        if a[base + i] < min_val {
            min_val = a[base + i];
        }
    }

    min_val
}

// ============================================================================
// Scalar fallbacks for non-SVE2 platforms
// ============================================================================

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn dot_sve2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn add_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] + b[i];
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn mul_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * b[i];
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn matvec_sve2(matrix: &[f32], vector: &[f32], result: &mut [f32], rows: usize, cols: usize) {
    for i in 0..rows {
        let mut sum = 0.0;
        for j in 0..cols {
            sum += matrix[i * cols + j] * vector[j];
        }
        result[i] = sum;
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn fma_sve2(a: &[f32], b: &[f32], c: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * b[i] + c[i];
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn scale_sve2(a: &[f32], scalar: f32, result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * scalar;
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn sum_sve2(a: &[f32]) -> f32 {
    a.iter().sum()
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn max_sve2(a: &[f32]) -> f32 {
    a.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn min_sve2(a: &[f32]) -> f32 {
    a.iter().cloned().fold(f32::INFINITY, f32::min)
}

#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn sub_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    use core::arch::aarch64::*;
    let len = a.len().min(b.len()).min(result.len());
    let mut i = 0;
    while i < len {
        let pg = svwhilelt_b32_u64(i as u64, len as u64);
        let va = svld1_f32(pg, a.as_ptr().add(i));
        let vb = svld1_f32(pg, b.as_ptr().add(i));
        let vr = svsub_f32_m(pg, va, vb);
        svst1_f32(pg, result.as_mut_ptr().add(i), vr);
        i += svcntw();
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn sub_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    let len = a.len().min(b.len()).min(result.len());
    for i in 0..len {
        result[i] = a[i] - b[i];
    }
}

#[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
#[target_feature(enable = "sve2")]
pub unsafe fn div_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    use core::arch::aarch64::*;
    let len = a.len().min(b.len()).min(result.len());
    let mut i = 0;
    while i < len {
        let pg = svwhilelt_b32_u64(i as u64, len as u64);
        let va = svld1_f32(pg, a.as_ptr().add(i));
        let vb = svld1_f32(pg, b.as_ptr().add(i));
        let vr = svdiv_f32_m(pg, va, vb);
        svst1_f32(pg, result.as_mut_ptr().add(i), vr);
        i += svcntw();
    }
}

#[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
pub fn div_sve2(a: &[f32], b: &[f32], result: &mut [f32]) {
    let len = a.len().min(b.len()).min(result.len());
    for i in 0..len {
        result[i] = a[i] / b[i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_sve2_dot() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { dot_sve2(&a, &b) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = dot_sve2(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 + 6*7 + 7*8 + 8*9 = 240
        assert_eq!(result, 240.0);
    }

    #[test]
    fn test_sve2_dot_odd_length() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { dot_sve2(&a, &b) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = dot_sve2(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 = 70
        assert_eq!(result, 70.0);
    }

    #[test]
    fn test_sve2_add() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = std::vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0];
        let mut result = std::vec![0.0; 9];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            add_sve2(&a, &b, &mut result)
        };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        add_sve2(&a, &b, &mut result);

        assert_eq!(
            result,
            std::vec![11.0, 22.0, 33.0, 44.0, 55.0, 66.0, 77.0, 88.0, 99.0]
        );
    }

    #[test]
    fn test_sve2_mul() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            mul_sve2(&a, &b, &mut result)
        };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        mul_sve2(&a, &b, &mut result);

        assert_eq!(result, std::vec![2.0, 6.0, 12.0, 20.0]);
    }

    #[test]
    fn test_sve2_matvec() {
        // 2x4 matrix
        let matrix = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let vector = std::vec![1.0, 1.0, 1.0, 1.0];
        let mut result = std::vec![0.0; 2];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            matvec_sve2(&matrix, &vector, &mut result, 2, 4)
        };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        matvec_sve2(&matrix, &vector, &mut result, 2, 4);

        // Row 0: 1+2+3+4 = 10
        // Row 1: 5+6+7+8 = 26
        assert_eq!(result, std::vec![10.0, 26.0]);
    }

    #[test]
    fn test_sve2_fma() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0];
        let b = std::vec![2.0, 2.0, 2.0, 2.0];
        let c = std::vec![10.0, 20.0, 30.0, 40.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            fma_sve2(&a, &b, &c, &mut result)
        };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        fma_sve2(&a, &b, &c, &mut result);

        // a*b+c = [1*2+10, 2*2+20, 3*2+30, 4*2+40] = [12, 24, 36, 48]
        assert_eq!(result, std::vec![12.0, 24.0, 36.0, 48.0]);
    }

    #[test]
    fn test_sve2_scale() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut result = std::vec![0.0; 5];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            scale_sve2(&a, 3.0, &mut result)
        };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        scale_sve2(&a, 3.0, &mut result);

        assert_eq!(result, std::vec![3.0, 6.0, 9.0, 12.0, 15.0]);
    }

    #[test]
    fn test_sve2_sum() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { sum_sve2(&a) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = sum_sve2(&a);

        assert_eq!(result, 55.0);
    }

    #[test]
    fn test_sve2_max() {
        let a = std::vec![1.0, 5.0, 3.0, 9.0, 2.0, 7.0, 4.0, 8.0, 6.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { max_sve2(&a) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = max_sve2(&a);

        assert_eq!(result, 9.0);
    }

    #[test]
    fn test_sve2_min() {
        let a = std::vec![5.0, 3.0, 9.0, 1.0, 7.0, 4.0, 8.0, 6.0, 2.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { min_sve2(&a) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = min_sve2(&a);

        assert_eq!(result, 1.0);
    }

    #[test]
    fn test_sve2_max_with_negative() {
        let a = std::vec![-5.0, -3.0, -9.0, -1.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { max_sve2(&a) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = max_sve2(&a);

        assert_eq!(result, -1.0);
    }

    #[test]
    fn test_sve2_min_with_negative() {
        let a = std::vec![-5.0, -3.0, -9.0, -1.0];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { min_sve2(&a) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = min_sve2(&a);

        assert_eq!(result, -9.0);
    }

    #[test]
    fn test_sve2_empty_array_sum() {
        let a: std::vec::Vec<f32> = std::vec![];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        let result = unsafe { sum_sve2(&a) };

        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        let result = sum_sve2(&a);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_sve2_sub() {
        let a = std::vec![5.0f32, 4.0, 3.0, 2.0];
        let b = std::vec![1.0f32, 2.0, 1.0, 1.0];
        let mut result = std::vec![0.0f32; 4];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            sub_sve2(&a, &b, &mut result)
        };
        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        sub_sve2(&a, &b, &mut result);

        assert!((result[0] - 4.0).abs() < 1e-6);
        assert!((result[1] - 2.0).abs() < 1e-6);
        assert!((result[2] - 2.0).abs() < 1e-6);
        assert!((result[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_sve2_div() {
        let a = std::vec![4.0f32, 9.0, 6.0, 8.0];
        let b = std::vec![2.0f32, 3.0, 2.0, 4.0];
        let mut result = std::vec![0.0f32; 4];

        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        unsafe {
            div_sve2(&a, &b, &mut result)
        };
        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        div_sve2(&a, &b, &mut result);

        assert!((result[0] - 2.0).abs() < 1e-6);
        assert!((result[1] - 3.0).abs() < 1e-6);
        assert!((result[2] - 3.0).abs() < 1e-6);
        assert!((result[3] - 2.0).abs() < 1e-6);
    }
}
