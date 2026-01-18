//! RISC-V Vector Extension (RVV) intrinsics for tensor operations
//!
//! RVV provides scalable vector processing with variable-length vectors.
//! Vector length is determined by hardware at runtime via VLEN.
//! This module implements hardware-accelerated tensor operations for RISC-V.
//!
//! Key features:
//! - Vector-length agnostic programming (like ARM SVE)
//! - Predicated operations via mask registers
//! - Rich reduction operations
//! - Gather/scatter memory access

#[cfg(target_arch = "riscv64")]
use core::arch::riscv64::*;

/// Get RISC-V vector length in f32 elements
///
/// Returns the number of 32-bit elements that fit in one RVV vector.
/// This is determined by hardware and varies between implementations.
#[cfg(target_arch = "riscv64")]
#[inline(always)]
unsafe fn rvv_f32_count() -> usize {
    // vsetvl returns the number of elements that can be processed
    // with e32 (32-bit elements) and m1 (LMUL=1)
    let vl: usize;
    core::arch::asm!(
        "vsetvli {vl}, zero, e32, m1",
        vl = out(reg) vl,
        options(nomem, nostack)
    );
    vl
}

/// RVV-optimized dot product
///
/// Processes vector-length-agnostic chunks using RVV.
/// Falls back to scalar for remaining elements.
///
/// # Safety
///
/// Requires RISC-V 64-bit architecture with V extension support.
/// Caller must ensure arrays have equal length.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn dot_rvv(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    // Initialize accumulator
    let mut sum = 0.0f32;

    // Process full vector-width chunks
    for i in 0..chunks {
        let offset = i * vl;

        // Set vector length
        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            vl = in(reg) vl,
            options(nomem, nostack)
        );

        // Load vectors and compute dot product
        let partial: f32;
        core::arch::asm!(
            // Load a[offset..offset+vl]
            "vle32.v v0, ({a})",
            // Load b[offset..offset+vl]
            "vle32.v v1, ({b})",
            // Element-wise multiply: v2 = v0 * v1
            "vfmul.vv v2, v0, v1",
            // Reduction sum: scalar = sum(v2)
            "vfredusum.vs v3, v2, v3",
            // Move to scalar register
            "vfmv.f.s {out}, v3",
            a = in(reg) a.as_ptr().add(offset),
            b = in(reg) b.as_ptr().add(offset),
            out = out(freg) partial,
            options(nostack)
        );
        sum += partial;
    }

    // Handle remaining elements with scalar code
    let base = chunks * vl;
    for i in 0..remainder {
        sum += a[base + i] * b[base + i];
    }

    sum
}

/// RVV-optimized element-wise addition
///
/// Adds two arrays element-wise using scalable vectors.
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension. All arrays must have equal length.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn add_rvv(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    for i in 0..chunks {
        let offset = i * vl;

        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vle32.v v1, ({b})",
            "vfadd.vv v2, v0, v1",
            "vse32.v v2, ({r})",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            b = in(reg) b.as_ptr().add(offset),
            r = in(reg) result.as_mut_ptr().add(offset),
            options(nostack)
        );
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] + b[base + i];
    }
}

/// RVV-optimized element-wise multiplication
///
/// Multiplies two arrays element-wise using scalable vectors.
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension. All arrays must have equal length.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn mul_rvv(a: &[f32], b: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    for i in 0..chunks {
        let offset = i * vl;

        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vle32.v v1, ({b})",
            "vfmul.vv v2, v0, v1",
            "vse32.v v2, ({r})",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            b = in(reg) b.as_ptr().add(offset),
            r = in(reg) result.as_mut_ptr().add(offset),
            options(nostack)
        );
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] * b[base + i];
    }
}

/// RVV-optimized matrix-vector multiplication
///
/// Optimized row-major matrix-vector product using RVV.
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension. Matrix length must equal rows * cols,
/// vector length must equal cols, and result length must equal rows.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn matvec_rvv(
    matrix: &[f32],
    vector: &[f32],
    result: &mut [f32],
    rows: usize,
    cols: usize,
) {
    debug_assert_eq!(matrix.len(), rows * cols);
    debug_assert_eq!(vector.len(), cols);
    debug_assert_eq!(result.len(), rows);

    let vl = rvv_f32_count();
    let chunks = cols / vl;
    let remainder = cols % vl;

    for i in 0..rows {
        let row_offset = i * cols;
        let mut sum = 0.0f32;

        for j in 0..chunks {
            let offset = row_offset + j * vl;
            let vec_offset = j * vl;

            let partial: f32;
            core::arch::asm!(
                "vsetvli zero, {vl}, e32, m1",
                "vle32.v v0, ({m})",
                "vle32.v v1, ({v})",
                "vfmul.vv v2, v0, v1",
                "vfredusum.vs v3, v2, v3",
                "vfmv.f.s {out}, v3",
                vl = in(reg) vl,
                m = in(reg) matrix.as_ptr().add(offset),
                v = in(reg) vector.as_ptr().add(vec_offset),
                out = out(freg) partial,
                options(nostack)
            );
            sum += partial;
        }

        // Handle remainder
        let base = chunks * vl;
        for j in 0..remainder {
            sum += matrix[row_offset + base + j] * vector[base + j];
        }

        result[i] = sum;
    }
}

/// RVV-optimized fused multiply-add: result = a * b + c
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension. All arrays must have equal length.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn fma_rvv(a: &[f32], b: &[f32], c: &[f32], result: &mut [f32]) {
    debug_assert_eq!(a.len(), b.len());
    debug_assert_eq!(a.len(), c.len());
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    for i in 0..chunks {
        let offset = i * vl;

        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vle32.v v1, ({b})",
            "vle32.v v2, ({c})",
            // FMA: v2 = v0 * v1 + v2
            "vfmacc.vv v2, v0, v1",
            "vse32.v v2, ({r})",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            b = in(reg) b.as_ptr().add(offset),
            c = in(reg) c.as_ptr().add(offset),
            r = in(reg) result.as_mut_ptr().add(offset),
            options(nostack)
        );
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] * b[base + i] + c[base + i];
    }
}

/// RVV-optimized scalar multiply: result = a * scalar
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn scale_rvv(a: &[f32], scalar: f32, result: &mut [f32]) {
    debug_assert_eq!(a.len(), result.len());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    for i in 0..chunks {
        let offset = i * vl;

        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vfmul.vf v1, v0, {s}",
            "vse32.v v1, ({r})",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            s = in(freg) scalar,
            r = in(reg) result.as_mut_ptr().add(offset),
            options(nostack)
        );
    }

    let base = chunks * vl;
    for i in 0..remainder {
        result[base + i] = a[base + i] * scalar;
    }
}

/// RVV-optimized sum reduction
///
/// Returns the sum of all elements in the array.
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn sum_rvv(a: &[f32]) -> f32 {
    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    let mut sum = 0.0f32;

    for i in 0..chunks {
        let offset = i * vl;

        let partial: f32;
        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vfredusum.vs v1, v0, v1",
            "vfmv.f.s {out}, v1",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            out = out(freg) partial,
            options(nostack)
        );
        sum += partial;
    }

    let base = chunks * vl;
    for i in 0..remainder {
        sum += a[base + i];
    }

    sum
}

/// RVV-optimized max reduction
///
/// Returns the maximum value in the array.
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension. Array must be non-empty.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn max_rvv(a: &[f32]) -> f32 {
    debug_assert!(!a.is_empty());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    let mut max_val = f32::NEG_INFINITY;

    for i in 0..chunks {
        let offset = i * vl;

        let partial: f32;
        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vfredmax.vs v1, v0, v1",
            "vfmv.f.s {out}, v1",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            out = out(freg) partial,
            options(nostack)
        );
        if partial > max_val {
            max_val = partial;
        }
    }

    let base = chunks * vl;
    for i in 0..remainder {
        if a[base + i] > max_val {
            max_val = a[base + i];
        }
    }

    max_val
}

/// RVV-optimized min reduction
///
/// Returns the minimum value in the array.
///
/// # Safety
///
/// Requires RISC-V 64-bit with V extension. Array must be non-empty.
#[cfg(target_arch = "riscv64")]
#[target_feature(enable = "v")]
pub unsafe fn min_rvv(a: &[f32]) -> f32 {
    debug_assert!(!a.is_empty());

    let len = a.len();
    let vl = rvv_f32_count();
    let chunks = len / vl;
    let remainder = len % vl;

    let mut min_val = f32::INFINITY;

    for i in 0..chunks {
        let offset = i * vl;

        let partial: f32;
        core::arch::asm!(
            "vsetvli zero, {vl}, e32, m1",
            "vle32.v v0, ({a})",
            "vfredmin.vs v1, v0, v1",
            "vfmv.f.s {out}, v1",
            vl = in(reg) vl,
            a = in(reg) a.as_ptr().add(offset),
            out = out(freg) partial,
            options(nostack)
        );
        if partial < min_val {
            min_val = partial;
        }
    }

    let base = chunks * vl;
    for i in 0..remainder {
        if a[base + i] < min_val {
            min_val = a[base + i];
        }
    }

    min_val
}

// ============================================================================
// Scalar fallbacks for non-RISC-V platforms
// ============================================================================

#[cfg(not(target_arch = "riscv64"))]
pub fn dot_rvv(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(not(target_arch = "riscv64"))]
pub fn add_rvv(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] + b[i];
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn mul_rvv(a: &[f32], b: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * b[i];
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn matvec_rvv(matrix: &[f32], vector: &[f32], result: &mut [f32], rows: usize, cols: usize) {
    for i in 0..rows {
        let mut sum = 0.0;
        for j in 0..cols {
            sum += matrix[i * cols + j] * vector[j];
        }
        result[i] = sum;
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn fma_rvv(a: &[f32], b: &[f32], c: &[f32], result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * b[i] + c[i];
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn scale_rvv(a: &[f32], scalar: f32, result: &mut [f32]) {
    for i in 0..a.len() {
        result[i] = a[i] * scalar;
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn sum_rvv(a: &[f32]) -> f32 {
    a.iter().sum()
}

#[cfg(not(target_arch = "riscv64"))]
pub fn max_rvv(a: &[f32]) -> f32 {
    a.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
}

#[cfg(not(target_arch = "riscv64"))]
pub fn min_rvv(a: &[f32]) -> f32 {
    a.iter().cloned().fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_rvv_dot() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { dot_rvv(&a, &b) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = dot_rvv(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 + 6*7 + 7*8 + 8*9 = 240
        assert_eq!(result, 240.0);
    }

    #[test]
    fn test_rvv_dot_odd_length() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0, 6.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { dot_rvv(&a, &b) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = dot_rvv(&a, &b);

        // 1*2 + 2*3 + 3*4 + 4*5 + 5*6 = 70
        assert_eq!(result, 70.0);
    }

    #[test]
    fn test_rvv_add() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let b = std::vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0];
        let mut result = std::vec![0.0; 9];

        #[cfg(target_arch = "riscv64")]
        unsafe {
            add_rvv(&a, &b, &mut result)
        };

        #[cfg(not(target_arch = "riscv64"))]
        add_rvv(&a, &b, &mut result);

        assert_eq!(
            result,
            std::vec![11.0, 22.0, 33.0, 44.0, 55.0, 66.0, 77.0, 88.0, 99.0]
        );
    }

    #[test]
    fn test_rvv_mul() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0];
        let b = std::vec![2.0, 3.0, 4.0, 5.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(target_arch = "riscv64")]
        unsafe {
            mul_rvv(&a, &b, &mut result)
        };

        #[cfg(not(target_arch = "riscv64"))]
        mul_rvv(&a, &b, &mut result);

        assert_eq!(result, std::vec![2.0, 6.0, 12.0, 20.0]);
    }

    #[test]
    fn test_rvv_matvec() {
        // 2x4 matrix
        let matrix = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let vector = std::vec![1.0, 1.0, 1.0, 1.0];
        let mut result = std::vec![0.0; 2];

        #[cfg(target_arch = "riscv64")]
        unsafe {
            matvec_rvv(&matrix, &vector, &mut result, 2, 4)
        };

        #[cfg(not(target_arch = "riscv64"))]
        matvec_rvv(&matrix, &vector, &mut result, 2, 4);

        // Row 0: 1+2+3+4 = 10
        // Row 1: 5+6+7+8 = 26
        assert_eq!(result, std::vec![10.0, 26.0]);
    }

    #[test]
    fn test_rvv_fma() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0];
        let b = std::vec![2.0, 2.0, 2.0, 2.0];
        let c = std::vec![10.0, 20.0, 30.0, 40.0];
        let mut result = std::vec![0.0; 4];

        #[cfg(target_arch = "riscv64")]
        unsafe {
            fma_rvv(&a, &b, &c, &mut result)
        };

        #[cfg(not(target_arch = "riscv64"))]
        fma_rvv(&a, &b, &c, &mut result);

        // a*b+c = [1*2+10, 2*2+20, 3*2+30, 4*2+40] = [12, 24, 36, 48]
        assert_eq!(result, std::vec![12.0, 24.0, 36.0, 48.0]);
    }

    #[test]
    fn test_rvv_scale() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut result = std::vec![0.0; 5];

        #[cfg(target_arch = "riscv64")]
        unsafe {
            scale_rvv(&a, 3.0, &mut result)
        };

        #[cfg(not(target_arch = "riscv64"))]
        scale_rvv(&a, 3.0, &mut result);

        assert_eq!(result, std::vec![3.0, 6.0, 9.0, 12.0, 15.0]);
    }

    #[test]
    fn test_rvv_sum() {
        let a = std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { sum_rvv(&a) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = sum_rvv(&a);

        assert_eq!(result, 55.0);
    }

    #[test]
    fn test_rvv_max() {
        let a = std::vec![1.0, 5.0, 3.0, 9.0, 2.0, 7.0, 4.0, 8.0, 6.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { max_rvv(&a) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = max_rvv(&a);

        assert_eq!(result, 9.0);
    }

    #[test]
    fn test_rvv_min() {
        let a = std::vec![5.0, 3.0, 9.0, 1.0, 7.0, 4.0, 8.0, 6.0, 2.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { min_rvv(&a) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = min_rvv(&a);

        assert_eq!(result, 1.0);
    }

    #[test]
    fn test_rvv_max_with_negative() {
        let a = std::vec![-5.0, -3.0, -9.0, -1.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { max_rvv(&a) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = max_rvv(&a);

        assert_eq!(result, -1.0);
    }

    #[test]
    fn test_rvv_min_with_negative() {
        let a = std::vec![-5.0, -3.0, -9.0, -1.0];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { min_rvv(&a) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = min_rvv(&a);

        assert_eq!(result, -9.0);
    }

    #[test]
    fn test_rvv_empty_array_sum() {
        let a: std::vec::Vec<f32> = std::vec![];

        #[cfg(target_arch = "riscv64")]
        let result = unsafe { sum_rvv(&a) };

        #[cfg(not(target_arch = "riscv64"))]
        let result = sum_rvv(&a);

        assert_eq!(result, 0.0);
    }
}
