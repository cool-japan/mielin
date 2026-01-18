//! x86_64 AVX2 SIMD intrinsics for tensor operations
//!
//! AVX2 provides 256-bit SIMD vectors that can hold 8 x f32 values.
//! This module implements hardware-accelerated tensor operations for x86_64.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// AVX2-optimized dot product
///
/// Processes 8 elements at a time using 256-bit AVX2 registers.
/// Falls back to scalar for remaining elements.
///
/// # Safety
///
/// Requires x86_64 architecture with AVX2 support. Caller must ensure
/// arrays have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn dot_avx2(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let len = a.len();
    let chunks = len / 8;
    let remainder = len % 8;

    // Accumulator vector (8 x f32)
    let mut acc = _mm256_setzero_ps();

    // Process 8 elements at a time
    for i in 0..chunks {
        let offset = i * 8;

        // Load 8 floats from each array
        let va = _mm256_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm256_loadu_ps(b.as_ptr().add(offset));

        // Multiply and accumulate
        let prod = _mm256_mul_ps(va, vb);
        acc = _mm256_add_ps(acc, prod);
    }

    // Horizontal sum of 8 elements
    // Extract high and low 128-bit lanes
    let high = _mm256_extractf128_ps::<1>(acc);
    let low = _mm256_castps256_ps128(acc);

    // Add lanes together
    let sum128 = _mm_add_ps(high, low);

    // Horizontal add within 128-bit
    let sum64 = _mm_hadd_ps(sum128, sum128);
    let sum32 = _mm_hadd_ps(sum64, sum64);

    // Extract scalar result
    let mut sum = _mm_cvtss_f32(sum32);

    // Handle remaining elements with scalar code
    let base = chunks * 8;
    for i in 0..remainder {
        sum += a[base + i] * b[base + i];
    }

    sum
}

/// AVX2-optimized element-wise addition
///
/// Adds two arrays element-wise using 256-bit AVX2 vectors.
///
/// # Safety
///
/// Requires x86_64 with AVX2 support. All arrays must have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn add_avx2(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 8;
    let remainder = len % 8;

    // Process 8 elements at a time
    for i in 0..chunks {
        let offset = i * 8;

        let va = _mm256_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm256_loadu_ps(b.as_ptr().add(offset));

        // Add vectors
        let vr = _mm256_add_ps(va, vb);

        // Store result
        _mm256_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    // Handle remaining elements
    let base = chunks * 8;
    for i in 0..remainder {
        result[base + i] = a[base + i] + b[base + i];
    }
}

/// AVX2-optimized matrix-vector multiplication
///
/// Optimized row-major matrix-vector product using AVX2.
///
/// # Safety
///
/// Requires x86_64 with AVX2 support. Matrix length must equal rows * cols,
/// vector length must equal cols, and result length must equal rows.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn matvec_avx2(
    matrix: &[f32],
    vector: &[f32],
    result: &mut [f32],
    rows: usize,
    cols: usize,
) {
    debug_assert_eq!(matrix.len(), rows * cols);
    debug_assert_eq!(vector.len(), cols);
    debug_assert_eq!(result.len(), rows);

    let chunks = cols / 8;
    let remainder = cols % 8;

    for (i, result_elem) in result.iter_mut().enumerate().take(rows) {
        let row_offset = i * cols;
        let mut acc = _mm256_setzero_ps();

        // Process 8 elements per iteration
        for j in 0..chunks {
            let offset = row_offset + j * 8;

            let vm = _mm256_loadu_ps(matrix.as_ptr().add(offset));
            let vv = _mm256_loadu_ps(vector.as_ptr().add(j * 8));

            let prod = _mm256_mul_ps(vm, vv);
            acc = _mm256_add_ps(acc, prod);
        }

        // Horizontal sum
        let high = _mm256_extractf128_ps::<1>(acc);
        let low = _mm256_castps256_ps128(acc);
        let sum128 = _mm_add_ps(high, low);
        let sum64 = _mm_hadd_ps(sum128, sum128);
        let sum32 = _mm_hadd_ps(sum64, sum64);
        let mut sum = _mm_cvtss_f32(sum32);

        // Handle remainder
        let base = chunks * 8;
        for j in 0..remainder {
            sum += matrix[row_offset + base + j] * vector[base + j];
        }

        *result_elem = sum;
    }
}

// Scalar fallback for non-x86_64 platforms
#[cfg(not(target_arch = "x86_64"))]
pub fn dot_avx2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(not(target_arch = "x86_64"))]
pub fn add_avx2(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] + b[i];
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn matvec_avx2(matrix: &[f32], vector: &[f32], result: &mut [f32], rows: usize, cols: usize) {
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
    fn test_avx2_dot() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

        #[cfg(target_arch = "x86_64")]
        let result = unsafe { dot_avx2(&a, &b) };

        #[cfg(not(target_arch = "x86_64"))]
        let result = dot_avx2(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 + 6*7 + 7*8 + 8*9 + 9*10
        // = 2 + 6 + 12 + 20 + 30 + 42 + 56 + 72 + 90 = 330
        assert_eq!(result, 330.0);
    }

    #[test]
    fn test_avx2_add() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = std::vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0];
        let mut result = std::vec![0.0; 9];

        #[cfg(target_arch = "x86_64")]
        unsafe {
            add_avx2(&a, &b, &mut result)
        };

        #[cfg(not(target_arch = "x86_64"))]
        add_avx2(&a, &b, &mut result);

        assert_eq!(
            result,
            std::vec![11.0, 22.0, 33.0, 44.0, 55.0, 66.0, 77.0, 88.0, 99.0]
        );
    }

    #[test]
    fn test_avx2_matvec() {
        // 2x8 matrix to test full AVX2 width
        let matrix = std::vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        ];
        let vector = std::vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut result = std::vec![0.0; 2];

        #[cfg(target_arch = "x86_64")]
        unsafe {
            matvec_avx2(&matrix, &vector, &mut result, 2, 8)
        };

        #[cfg(not(target_arch = "x86_64"))]
        matvec_avx2(&matrix, &vector, &mut result, 2, 8);

        // Row 0: 1+2+3+4+5+6+7+8 = 36
        // Row 1: 9+10+11+12+13+14+15+16 = 100
        assert_eq!(result, std::vec![36.0, 100.0]);
    }
}
