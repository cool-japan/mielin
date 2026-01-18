//! x86_64 AVX-512 SIMD intrinsics for tensor operations
//!
//! AVX-512 provides 512-bit SIMD vectors that can hold 16 x f32 values.
//! This module implements hardware-accelerated tensor operations for x86_64
//! with AVX-512 support.
//!
//! Key advantages over AVX2:
//! - 2x wider vectors (512 vs 256 bits)
//! - Built-in masking (predication)
//! - FMA (fused multiply-add) throughput
//! - Scatter/gather operations

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

/// AVX-512-optimized dot product
///
/// Processes 16 elements at a time using 512-bit AVX-512 registers.
/// Falls back to scalar for remaining elements.
///
/// # Safety
///
/// Requires x86_64 architecture with AVX-512F support. Caller must ensure
/// arrays have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn dot_avx512(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    // Accumulator vector (16 x f32)
    let mut acc = _mm512_setzero_ps();

    // Process 16 elements at a time
    for i in 0..chunks {
        let offset = i * 16;

        // Load 16 floats from each array
        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm512_loadu_ps(b.as_ptr().add(offset));

        // Fused multiply-accumulate: acc = acc + va * vb
        acc = _mm512_fmadd_ps(va, vb, acc);
    }

    // Horizontal sum of 16 elements
    let mut sum = _mm512_reduce_add_ps(acc);

    // Handle remaining elements with scalar code
    let base = chunks * 16;
    for i in 0..remainder {
        sum += a[base + i] * b[base + i];
    }

    sum
}

/// AVX-512-optimized element-wise addition
///
/// Adds two arrays element-wise using 512-bit AVX-512 vectors.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. All arrays must have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn add_avx512(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    // Process 16 elements at a time
    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm512_loadu_ps(b.as_ptr().add(offset));

        // Add vectors
        let vr = _mm512_add_ps(va, vb);

        // Store result
        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    // Handle remaining elements
    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = a[base + i] + b[base + i];
    }
}

/// AVX-512-optimized element-wise subtraction
///
/// Subtracts b from a element-wise using 512-bit AVX-512 vectors.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. All arrays must have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn sub_avx512(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm512_loadu_ps(b.as_ptr().add(offset));

        let vr = _mm512_sub_ps(va, vb);

        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = a[base + i] - b[base + i];
    }
}

/// AVX-512-optimized element-wise multiplication
///
/// Multiplies two arrays element-wise using 512-bit AVX-512 vectors.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. All arrays must have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn mul_avx512(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm512_loadu_ps(b.as_ptr().add(offset));

        let vr = _mm512_mul_ps(va, vb);

        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = a[base + i] * b[base + i];
    }
}

/// AVX-512-optimized matrix-vector multiplication
///
/// Optimized row-major matrix-vector product using AVX-512.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. Matrix length must equal rows * cols,
/// vector length must equal cols, and result length must equal rows.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn matvec_avx512(
    matrix: &[f32],
    vector: &[f32],
    result: &mut [f32],
    rows: usize,
    cols: usize,
) {
    debug_assert_eq!(matrix.len(), rows * cols);
    debug_assert_eq!(vector.len(), cols);
    debug_assert_eq!(result.len(), rows);

    let chunks = cols / 16;
    let remainder = cols % 16;

    for (i, result_elem) in result.iter_mut().enumerate().take(rows) {
        let row_offset = i * cols;
        let mut acc = _mm512_setzero_ps();

        // Process 16 elements per iteration
        for j in 0..chunks {
            let offset = row_offset + j * 16;

            let vm = _mm512_loadu_ps(matrix.as_ptr().add(offset));
            let vv = _mm512_loadu_ps(vector.as_ptr().add(j * 16));

            // Fused multiply-accumulate
            acc = _mm512_fmadd_ps(vm, vv, acc);
        }

        // Horizontal sum
        let mut sum = _mm512_reduce_add_ps(acc);

        // Handle remainder
        let base = chunks * 16;
        for j in 0..remainder {
            sum += matrix[row_offset + base + j] * vector[base + j];
        }

        *result_elem = sum;
    }
}

/// AVX-512-optimized fused multiply-add: result = a * b + c
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. All arrays must have equal length.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn fma_avx512(a: &[f32], b: &[f32], c: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm512_loadu_ps(b.as_ptr().add(offset));
        let vc = _mm512_loadu_ps(c.as_ptr().add(offset));

        // FMA: a * b + c
        let vr = _mm512_fmadd_ps(va, vb, vc);

        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = a[base + i] * b[base + i] + c[base + i];
    }
}

/// AVX-512-optimized scalar multiply: result = a * scalar
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn scale_avx512(a: &[f32], scalar: f32, result: &mut [f32]) {
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    let vs = _mm512_set1_ps(scalar);

    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let vr = _mm512_mul_ps(va, vs);

        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = a[base + i] * scalar;
    }
}

/// AVX-512-optimized sum reduction
///
/// Returns the sum of all elements in the array.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn sum_avx512(a: &[f32]) -> f32 {
    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    let mut acc = _mm512_setzero_ps();

    for i in 0..chunks {
        let offset = i * 16;
        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        acc = _mm512_add_ps(acc, va);
    }

    let mut sum = _mm512_reduce_add_ps(acc);

    let base = chunks * 16;
    for i in 0..remainder {
        sum += a[base + i];
    }

    sum
}

/// AVX-512-optimized max reduction
///
/// Returns the maximum value in the array.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. Array must be non-empty.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn max_avx512(a: &[f32]) -> f32 {
    debug_assert!(!a.is_empty());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    let mut acc = _mm512_set1_ps(f32::NEG_INFINITY);

    for i in 0..chunks {
        let offset = i * 16;
        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        acc = _mm512_max_ps(acc, va);
    }

    let mut max_val = _mm512_reduce_max_ps(acc);

    let base = chunks * 16;
    for i in 0..remainder {
        if a[base + i] > max_val {
            max_val = a[base + i];
        }
    }

    max_val
}

/// AVX-512-optimized min reduction
///
/// Returns the minimum value in the array.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support. Array must be non-empty.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn min_avx512(a: &[f32]) -> f32 {
    debug_assert!(!a.is_empty());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    let mut acc = _mm512_set1_ps(f32::INFINITY);

    for i in 0..chunks {
        let offset = i * 16;
        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        acc = _mm512_min_ps(acc, va);
    }

    let mut min_val = _mm512_reduce_min_ps(acc);

    let base = chunks * 16;
    for i in 0..remainder {
        if a[base + i] < min_val {
            min_val = a[base + i];
        }
    }

    min_val
}

/// AVX-512-optimized ReLU activation
///
/// Applies ReLU: max(0, x) to each element.
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn relu_avx512(input: &[f32], result: &mut [f32]) {
    debug_assert_eq!(input.len(), result.len());

    let len = input.len();
    let chunks = len / 16;
    let remainder = len % 16;

    let zero = _mm512_setzero_ps();

    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(input.as_ptr().add(offset));
        let vr = _mm512_max_ps(va, zero);

        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = if input[base + i] > 0.0 {
            input[base + i]
        } else {
            0.0
        };
    }
}

/// AVX-512-optimized absolute value
///
/// # Safety
///
/// Requires x86_64 with AVX-512F support.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn abs_avx512(a: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let chunks = len / 16;
    let remainder = len % 16;

    // Mask to clear sign bit
    let sign_mask = _mm512_set1_epi32(0x7FFF_FFFF_u32 as i32);

    for i in 0..chunks {
        let offset = i * 16;

        let va = _mm512_loadu_ps(a.as_ptr().add(offset));
        let va_int = _mm512_castps_si512(va);
        let abs_int = _mm512_and_epi32(va_int, sign_mask);
        let vr = _mm512_castsi512_ps(abs_int);

        _mm512_storeu_ps(result.as_mut_ptr().add(offset), vr);
    }

    let base = chunks * 16;
    for i in 0..remainder {
        result[base + i] = a[base + i].abs();
    }
}

// ============================================================================
// Scalar fallbacks for non-AVX-512 platforms
// ============================================================================

#[cfg(not(target_arch = "x86_64"))]
pub fn dot_avx512(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(not(target_arch = "x86_64"))]
pub fn add_avx512(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] + b[i];
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn sub_avx512(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] - b[i];
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn mul_avx512(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * b[i];
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn matvec_avx512(matrix: &[f32], vector: &[f32], result: &mut [f32], rows: usize, cols: usize) {
    for i in 0..rows {
        let mut sum = 0.0;
        for j in 0..cols {
            sum += matrix[i * cols + j] * vector[j];
        }
        result[i] = sum;
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn fma_avx512(a: &[f32], b: &[f32], c: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * b[i] + c[i];
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn scale_avx512(a: &[f32], scalar: f32, result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * scalar;
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn sum_avx512(a: &[f32]) -> f32 {
    a.iter().sum()
}

#[cfg(not(target_arch = "x86_64"))]
pub fn max_avx512(a: &[f32]) -> f32 {
    a.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
}

#[cfg(not(target_arch = "x86_64"))]
pub fn min_avx512(a: &[f32]) -> f32 {
    a.iter().cloned().fold(f32::INFINITY, f32::min)
}

#[cfg(not(target_arch = "x86_64"))]
pub fn relu_avx512(input: &[f32], result: &mut [f32]) {
    for i in 0..input.len() {
        result[i] = if input[i] > 0.0 { input[i] } else { 0.0 };
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn abs_avx512(a: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i].abs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_avx512_dot() {
        // 16 elements to test full vector width
        let a = std::vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
            17.0
        ];
        let b = std::vec![
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0
        ];

        #[cfg(target_arch = "x86_64")]
        let result = if std::is_x86_feature_detected!("avx512f") {
            unsafe { dot_avx512(&a, &b) }
        } else {
            a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
        };

        #[cfg(not(target_arch = "x86_64"))]
        let result = dot_avx512(&a, &b);

        // Sum 1..17 = 153
        assert_eq!(result, 153.0);
    }

    #[test]
    fn test_avx512_dot_small() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0];

        #[cfg(target_arch = "x86_64")]
        let result = if std::is_x86_feature_detected!("avx512f") {
            unsafe { dot_avx512(&a, &b) }
        } else {
            a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
        };

        #[cfg(not(target_arch = "x86_64"))]
        let result = dot_avx512(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 = 70
        assert_eq!(result, 70.0);
    }

    #[test]
    fn test_avx512_add() {
        let a = std::vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
            17.0
        ];
        let b = std::vec![
            10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0, 130.0,
            140.0, 150.0, 160.0, 170.0
        ];
        let mut result = std::vec![0.0; 17];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { add_avx512(&a, &b, &mut result) };
        } else {
            for i in 0..a.len() {
                result[i] = a[i] + b[i];
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        add_avx512(&a, &b, &mut result);

        assert_eq!(result[0], 11.0);
        assert_eq!(result[15], 176.0);
        assert_eq!(result[16], 187.0);
    }

    #[test]
    fn test_avx512_sub() {
        let a = std::vec![10.0, 20.0, 30.0, 40.0];
        let b = std::vec![1.0, 2.0, 3.0, 4.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { sub_avx512(&a, &b, &mut result) };
        } else {
            for i in 0..a.len() {
                result[i] = a[i] - b[i];
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        sub_avx512(&a, &b, &mut result);

        assert_eq!(result, std::vec![9.0, 18.0, 27.0, 36.0]);
    }

    #[test]
    fn test_avx512_mul() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { mul_avx512(&a, &b, &mut result) };
        } else {
            for i in 0..a.len() {
                result[i] = a[i] * b[i];
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        mul_avx512(&a, &b, &mut result);

        assert_eq!(result, std::vec![2.0, 6.0, 12.0, 20.0]);
    }

    #[test]
    fn test_avx512_matvec() {
        // 2x16 matrix to test full AVX-512 width
        let matrix = std::vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
            17.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0,
            31.0, 32.0,
        ];
        let vector = std::vec![
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0
        ];
        let mut result = std::vec![0.0; 2];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { matvec_avx512(&matrix, &vector, &mut result, 2, 16) };
        } else {
            for i in 0..2 {
                let mut sum = 0.0;
                for j in 0..16 {
                    sum += matrix[i * 16 + j] * vector[j];
                }
                result[i] = sum;
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        matvec_avx512(&matrix, &vector, &mut result, 2, 16);

        // Row 0: 1+2+...+16 = 136
        // Row 1: 17+18+...+32 = 392
        assert_eq!(result, std::vec![136.0, 392.0]);
    }

    #[test]
    fn test_avx512_fma() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0];
        let b = std::vec![2.0, 2.0, 2.0, 2.0];
        let c = std::vec![10.0, 20.0, 30.0, 40.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { fma_avx512(&a, &b, &c, &mut result) };
        } else {
            for i in 0..a.len() {
                result[i] = a[i] * b[i] + c[i];
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        fma_avx512(&a, &b, &c, &mut result);

        // a*b+c = [1*2+10, 2*2+20, 3*2+30, 4*2+40] = [12, 24, 36, 48]
        assert_eq!(result, std::vec![12.0, 24.0, 36.0, 48.0]);
    }

    #[test]
    fn test_avx512_scale() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut result = std::vec![0.0; 5];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { scale_avx512(&a, 3.0, &mut result) };
        } else {
            for i in 0..a.len() {
                result[i] = a[i] * 3.0;
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        scale_avx512(&a, 3.0, &mut result);

        assert_eq!(result, std::vec![3.0, 6.0, 9.0, 12.0, 15.0]);
    }

    #[test]
    fn test_avx512_sum() {
        let a = std::vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
            17.0, 18.0
        ];

        #[cfg(target_arch = "x86_64")]
        let result = if std::is_x86_feature_detected!("avx512f") {
            unsafe { sum_avx512(&a) }
        } else {
            a.iter().sum()
        };

        #[cfg(not(target_arch = "x86_64"))]
        let result = sum_avx512(&a);

        // Sum 1..18 = 171
        assert_eq!(result, 171.0);
    }

    #[test]
    fn test_avx512_max() {
        let a = std::vec![
            1.0, 5.0, 3.0, 9.0, 2.0, 7.0, 4.0, 8.0, 6.0, 15.0, 12.0, 11.0, 14.0, 10.0, 13.0, 16.0,
            0.0
        ];

        #[cfg(target_arch = "x86_64")]
        let result = if std::is_x86_feature_detected!("avx512f") {
            unsafe { max_avx512(&a) }
        } else {
            a.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        };

        #[cfg(not(target_arch = "x86_64"))]
        let result = max_avx512(&a);

        assert_eq!(result, 16.0);
    }

    #[test]
    fn test_avx512_min() {
        let a = std::vec![
            5.0, 3.0, 9.0, 1.0, 7.0, 4.0, 8.0, 6.0, 2.0, 15.0, 12.0, 11.0, 14.0, 10.0, 13.0, 16.0,
            0.0
        ];

        #[cfg(target_arch = "x86_64")]
        let result = if std::is_x86_feature_detected!("avx512f") {
            unsafe { min_avx512(&a) }
        } else {
            a.iter().cloned().fold(f32::INFINITY, f32::min)
        };

        #[cfg(not(target_arch = "x86_64"))]
        let result = min_avx512(&a);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_avx512_relu() {
        let input = std::vec![-1.0, 0.0, 1.0, -2.0, 3.0];
        let mut result = std::vec![0.0; 5];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { relu_avx512(&input, &mut result) };
        } else {
            for i in 0..input.len() {
                result[i] = if input[i] > 0.0 { input[i] } else { 0.0 };
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        relu_avx512(&input, &mut result);

        assert_eq!(result, std::vec![0.0, 0.0, 1.0, 0.0, 3.0]);
    }

    #[test]
    fn test_avx512_abs() {
        let a = std::vec![-1.0, 2.0, -3.0, 4.0, -5.0];
        let mut result = std::vec![0.0; 5];

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx512f") {
            unsafe { abs_avx512(&a, &mut result) };
        } else {
            for i in 0..a.len() {
                result[i] = a[i].abs();
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        abs_avx512(&a, &mut result);

        assert_eq!(result, std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_avx512_empty_array_sum() {
        let a: std::vec::Vec<f32> = std::vec![];

        #[cfg(target_arch = "x86_64")]
        let result = if std::is_x86_feature_detected!("avx512f") {
            unsafe { sum_avx512(&a) }
        } else {
            0.0
        };

        #[cfg(not(target_arch = "x86_64"))]
        let result = sum_avx512(&a);

        assert_eq!(result, 0.0);
    }
}
