//! Tensor operations with hardware-specific backends

extern crate alloc;
use crate::backends::{
    add_avx2, add_neon, add_sve2, div_sve2, dot_avx2, dot_neon, matvec_avx2, matvec_neon,
    matvec_sve2, mul_sve2, sub_sve2,
};
use crate::broadcast::{broadcast_shape, broadcast_strides, index_from_strides, unravel_index};
use crate::tensor::Tensor;
use alloc::vec::Vec;
use mielin_hal::capabilities::HardwareCapabilities;

/// Tensor operation backend dispatcher
pub struct TensorOps {
    capabilities: HardwareCapabilities,
}

impl TensorOps {
    pub fn new(capabilities: HardwareCapabilities) -> Self {
        Self { capabilities }
    }

    /// Compute dot product of two vectors
    pub fn dot(&self, a: &Tensor<f32>, b: &Tensor<f32>) -> Option<f32> {
        // Verify both are 1D tensors of same length
        if a.ndim() != 1 || b.ndim() != 1 {
            return None;
        }
        if a.size() != b.size() {
            return None;
        }

        // Dispatch to hardware-specific implementation
        if self.capabilities.contains(HardwareCapabilities::SVE2) {
            Some(self.dot_sve2(a.data(), b.data()))
        } else if self.capabilities.contains(HardwareCapabilities::NEON) {
            Some(self.dot_neon(a.data(), b.data()))
        } else if self.capabilities.contains(HardwareCapabilities::AVX2) {
            Some(self.dot_avx2(a.data(), b.data()))
        } else {
            Some(self.dot_scalar(a.data(), b.data()))
        }
    }

    /// Element-wise addition with broadcasting support
    pub fn add(&self, a: &Tensor<f32>, b: &Tensor<f32>) -> Option<Tensor<f32>> {
        // Fast path: exact shape match
        if a.shape() == b.shape() {
            let mut result = Tensor::zeros(a.shape().to_vec());
            if self.capabilities.contains(HardwareCapabilities::SVE2) {
                #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
                unsafe {
                    add_sve2(a.data(), b.data(), result.data_mut())
                };
                #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
                add_sve2(a.data(), b.data(), result.data_mut());
            } else if self.capabilities.contains(HardwareCapabilities::NEON)
                || self.capabilities.contains(HardwareCapabilities::AVX2)
            {
                self.add_simd(a.data(), b.data(), result.data_mut());
            } else {
                self.add_scalar(a.data(), b.data(), result.data_mut());
            }
            return Some(result);
        }
        // Broadcasting path
        self.broadcast_binary_op(a, b, |x, y| x + y)
    }

    /// Element-wise subtraction with broadcasting support
    pub fn sub(&self, a: &Tensor<f32>, b: &Tensor<f32>) -> Option<Tensor<f32>> {
        if a.shape() == b.shape() {
            let mut result = Tensor::zeros(a.shape().to_vec());
            if self.capabilities.contains(HardwareCapabilities::SVE2) {
                #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
                unsafe {
                    sub_sve2(a.data(), b.data(), result.data_mut())
                };
                #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
                sub_sve2(a.data(), b.data(), result.data_mut());
                return Some(result);
            }
            for i in 0..a.size() {
                result.data_mut()[i] = a.data()[i] - b.data()[i];
            }
            return Some(result);
        }
        self.broadcast_binary_op(a, b, |x, y| x - y)
    }

    /// Element-wise multiplication with broadcasting support
    pub fn mul(&self, a: &Tensor<f32>, b: &Tensor<f32>) -> Option<Tensor<f32>> {
        if a.shape() == b.shape() {
            let mut result = Tensor::zeros(a.shape().to_vec());
            if self.capabilities.contains(HardwareCapabilities::SVE2) {
                #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
                unsafe {
                    mul_sve2(a.data(), b.data(), result.data_mut())
                };
                #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
                mul_sve2(a.data(), b.data(), result.data_mut());
            } else {
                for i in 0..a.size() {
                    result.data_mut()[i] = a.data()[i] * b.data()[i];
                }
            }
            return Some(result);
        }
        self.broadcast_binary_op(a, b, |x, y| x * y)
    }

    /// Element-wise division with broadcasting support
    pub fn div(&self, a: &Tensor<f32>, b: &Tensor<f32>) -> Option<Tensor<f32>> {
        if a.shape() == b.shape() {
            let mut result = Tensor::zeros(a.shape().to_vec());
            if self.capabilities.contains(HardwareCapabilities::SVE2) {
                #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
                unsafe {
                    div_sve2(a.data(), b.data(), result.data_mut())
                };
                #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
                div_sve2(a.data(), b.data(), result.data_mut());
            } else {
                for i in 0..a.size() {
                    result.data_mut()[i] = a.data()[i] / b.data()[i];
                }
            }
            return Some(result);
        }
        self.broadcast_binary_op(a, b, |x, y| x / y)
    }

    /// Matrix multiplication (2D only for now)
    pub fn matmul(&self, a: &Tensor<f32>, b: &Tensor<f32>) -> Option<Tensor<f32>> {
        if a.ndim() != 2 || b.ndim() != 2 {
            return None;
        }

        let a_shape = a.shape();
        let b_shape = b.shape();

        // Check dimensions are compatible (M x K) * (K x N) = (M x N)
        if a_shape[1] != b_shape[0] {
            return None;
        }

        let m = a_shape[0];
        let k = a_shape[1];
        let n = b_shape[1];

        let mut result = Tensor::zeros(alloc::vec![m, n]);

        // Dispatch to appropriate backend
        if self.capabilities.contains(HardwareCapabilities::SVE2) {
            self.matmul_sve2(a, b, &mut result, m, k, n);
        } else if self.capabilities.contains(HardwareCapabilities::NEON) {
            self.matmul_neon(a, b, &mut result, m, k, n);
        } else if self.capabilities.contains(HardwareCapabilities::AVX2) {
            self.matmul_avx2(a, b, &mut result, m, k, n);
        } else {
            self.matmul_scalar(a, b, &mut result, m, k, n);
        }

        Some(result)
    }

    /// Generic broadcasting binary operation
    fn broadcast_binary_op<F>(&self, a: &Tensor<f32>, b: &Tensor<f32>, op: F) -> Option<Tensor<f32>>
    where
        F: Fn(f32, f32) -> f32,
    {
        // Compute broadcast shape
        let result_shape = broadcast_shape(a.shape(), b.shape())?;

        // Calculate strides for broadcasting
        let (strides_a, strides_b) = broadcast_strides(a.shape(), b.shape(), &result_shape);

        // Create result tensor
        let size: usize = result_shape.iter().product();
        let mut result_data = Vec::with_capacity(size);

        // Perform broadcasting operation
        for i in 0..size {
            let indices = unravel_index(i, &result_shape);
            let idx_a = index_from_strides(&indices, &strides_a);
            let idx_b = index_from_strides(&indices, &strides_b);

            result_data.push(op(a.data()[idx_a], b.data()[idx_b]));
        }

        Tensor::from_vec(result_data, result_shape)
    }

    /// Scalar fallback for dot product
    fn dot_scalar(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// NEON-optimized dot product
    #[inline]
    fn dot_neon(&self, a: &[f32], b: &[f32]) -> f32 {
        #[cfg(target_arch = "aarch64")]
        {
            unsafe { dot_neon(a, b) }
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            dot_neon(a, b) // Falls back to scalar
        }
    }

    /// SVE2-optimized dot product
    #[inline]
    fn dot_sve2(&self, a: &[f32], b: &[f32]) -> f32 {
        #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
        {
            unsafe { crate::backends::sve2::dot_sve2(a, b) }
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
        {
            crate::backends::sve2::dot_sve2(a, b)
        }
    }

    /// AVX2-optimized dot product
    #[inline]
    fn dot_avx2(&self, a: &[f32], b: &[f32]) -> f32 {
        #[cfg(target_arch = "x86_64")]
        {
            unsafe { dot_avx2(a, b) }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            dot_avx2(a, b) // Falls back to scalar
        }
    }

    /// Scalar element-wise addition
    fn add_scalar(&self, a: &[f32], b: &[f32], result: &mut [f32]) {
        for i in 0..a.len() {
            result[i] = a[i] + b[i];
        }
    }

    /// SIMD-optimized element-wise addition
    #[inline]
    fn add_simd(&self, a: &[f32], b: &[f32], result: &mut [f32]) {
        if self.capabilities.contains(HardwareCapabilities::NEON) {
            #[cfg(target_arch = "aarch64")]
            {
                unsafe { add_neon(a, b, result) }
            }
            #[cfg(not(target_arch = "aarch64"))]
            {
                add_neon(a, b, result);
            }
        } else if self.capabilities.contains(HardwareCapabilities::AVX2) {
            #[cfg(target_arch = "x86_64")]
            {
                unsafe { add_avx2(a, b, result) }
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                add_avx2(a, b, result);
            }
        } else {
            self.add_scalar(a, b, result);
        }
    }

    /// Scalar matrix multiplication
    fn matmul_scalar(
        &self,
        a: &Tensor<f32>,
        b: &Tensor<f32>,
        result: &mut Tensor<f32>,
        m: usize,
        k: usize,
        n: usize,
    ) {
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for p in 0..k {
                    sum += a.get(&[i, p]).unwrap() * b.get(&[p, j]).unwrap();
                }
                result.set(&[i, j], sum);
            }
        }
    }

    /// NEON-optimized matrix multiplication
    #[inline]
    fn matmul_neon(
        &self,
        a: &Tensor<f32>,
        b: &Tensor<f32>,
        result: &mut Tensor<f32>,
        m: usize,
        k: usize,
        n: usize,
    ) {
        // Use NEON-optimized matrix-vector multiplication
        // Process each column of B as a vector
        for j in 0..n {
            let col_b: alloc::vec::Vec<f32> = (0..k).map(|i| *b.get(&[i, j]).unwrap()).collect();

            let mut col_result = alloc::vec![0.0; m];

            #[cfg(target_arch = "aarch64")]
            {
                unsafe { matvec_neon(a.data(), &col_b, &mut col_result, m, k) }
            }
            #[cfg(not(target_arch = "aarch64"))]
            {
                matvec_neon(a.data(), &col_b, &mut col_result, m, k);
            }

            for (i, &value) in col_result.iter().enumerate() {
                result.set(&[i, j], value);
            }
        }
    }

    /// SVE2-optimized matrix multiplication (column-by-column via matvec_sve2)
    #[inline]
    fn matmul_sve2(
        &self,
        a: &Tensor<f32>,
        b: &Tensor<f32>,
        result: &mut Tensor<f32>,
        m: usize,
        k: usize,
        n: usize,
    ) {
        for j in 0..n {
            let col_b: alloc::vec::Vec<f32> = (0..k).map(|i| *b.get(&[i, j]).unwrap()).collect();
            let mut col_result = alloc::vec![0.0f32; m];
            #[cfg(all(target_arch = "aarch64", target_feature = "sve2"))]
            unsafe {
                matvec_sve2(a.data(), &col_b, &mut col_result, m, k)
            };
            #[cfg(not(all(target_arch = "aarch64", target_feature = "sve2")))]
            matvec_sve2(a.data(), &col_b, &mut col_result, m, k);
            for (i, &val) in col_result.iter().enumerate() {
                result.set(&[i, j], val);
            }
        }
    }

    /// AVX2-optimized matrix multiplication
    #[inline]
    fn matmul_avx2(
        &self,
        a: &Tensor<f32>,
        b: &Tensor<f32>,
        result: &mut Tensor<f32>,
        m: usize,
        k: usize,
        n: usize,
    ) {
        // Use AVX2-optimized matrix-vector multiplication
        // Process each column of B as a vector
        for j in 0..n {
            let col_b: alloc::vec::Vec<f32> = (0..k).map(|i| *b.get(&[i, j]).unwrap()).collect();

            let mut col_result = alloc::vec![0.0; m];

            #[cfg(target_arch = "x86_64")]
            {
                unsafe { matvec_avx2(a.data(), &col_b, &mut col_result, m, k) }
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                matvec_avx2(a.data(), &col_b, &mut col_result, m, k);
            }

            for (i, &value) in col_result.iter().enumerate() {
                result.set(&[i, j], value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_dot_product() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        let a = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let b = Tensor::vector(std::vec![4.0, 5.0, 6.0]);

        let result = ops.dot(&a, &b).unwrap();
        assert_eq!(result, 32.0); // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
    }

    #[test]
    fn test_add() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        let a = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let b = Tensor::vector(std::vec![4.0, 5.0, 6.0]);

        let result = ops.add(&a, &b).unwrap();
        assert_eq!(result.data(), &[5.0, 7.0, 9.0]);
    }

    #[test]
    fn test_matmul() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // 2x2 * 2x2
        let a = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let b = Tensor::matrix(std::vec![5.0, 6.0, 7.0, 8.0], 2, 2).unwrap();

        let result = ops.matmul(&a, &b).unwrap();

        // Expected: [[19, 22], [43, 50]]
        // Row 0: [1*5 + 2*7, 1*6 + 2*8] = [19, 22]
        // Row 1: [3*5 + 4*7, 3*6 + 4*8] = [43, 50]
        assert_eq!(result.get(&[0, 0]), Some(&19.0));
        assert_eq!(result.get(&[0, 1]), Some(&22.0));
        assert_eq!(result.get(&[1, 0]), Some(&43.0));
        assert_eq!(result.get(&[1, 1]), Some(&50.0));
    }

    #[test]
    fn test_matmul_non_square() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // 2x3 * 3x2
        let a = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
        let b = Tensor::matrix(std::vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0], 3, 2).unwrap();

        let result = ops.matmul(&a, &b).unwrap();
        assert_eq!(result.shape(), &[2, 2]);

        // Expected: [[58, 64], [139, 154]]
        assert_eq!(result.get(&[0, 0]), Some(&58.0));
        assert_eq!(result.get(&[0, 1]), Some(&64.0));
        assert_eq!(result.get(&[1, 0]), Some(&139.0));
        assert_eq!(result.get(&[1, 1]), Some(&154.0));
    }

    // Broadcasting tests
    #[test]
    fn test_broadcast_add_scalar() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        let a = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let b = Tensor::vector(std::vec![10.0]);

        let result = ops.add(&a, &b).unwrap();
        assert_eq!(result.shape(), &[2, 2]);
        assert_eq!(result.data(), &[11.0, 12.0, 13.0, 14.0]);
    }

    #[test]
    fn test_broadcast_add_vector_to_matrix() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // (2, 3) + (3,) should broadcast to (2, 3)
        let a = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
        let b = Tensor::vector(std::vec![10.0, 20.0, 30.0]);

        let result = ops.add(&a, &b).unwrap();
        assert_eq!(result.shape(), &[2, 3]);
        // Row 0: [1+10, 2+20, 3+30] = [11, 22, 33]
        // Row 1: [4+10, 5+20, 6+30] = [14, 25, 36]
        assert_eq!(result.data(), &[11.0, 22.0, 33.0, 14.0, 25.0, 36.0]);
    }

    #[test]
    fn test_broadcast_add_compatible_shapes() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // (3, 1) + (1, 4) should broadcast to (3, 4)
        let a = Tensor::matrix(std::vec![1.0, 2.0, 3.0], 3, 1).unwrap();
        let b = Tensor::matrix(std::vec![10.0, 20.0, 30.0, 40.0], 1, 4).unwrap();

        let result = ops.add(&a, &b).unwrap();
        assert_eq!(result.shape(), &[3, 4]);

        // Expected result:
        // [1+10, 1+20, 1+30, 1+40] = [11, 21, 31, 41]
        // [2+10, 2+20, 2+30, 2+40] = [12, 22, 32, 42]
        // [3+10, 3+20, 3+30, 3+40] = [13, 23, 33, 43]
        assert_eq!(
            result.data(),
            &[11.0, 21.0, 31.0, 41.0, 12.0, 22.0, 32.0, 42.0, 13.0, 23.0, 33.0, 43.0]
        );
    }

    #[test]
    fn test_broadcast_sub() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        let a = Tensor::vector(std::vec![10.0, 20.0, 30.0]);
        let b = Tensor::vector(std::vec![1.0]);

        let result = ops.sub(&a, &b).unwrap();
        assert_eq!(result.data(), &[9.0, 19.0, 29.0]);
    }

    #[test]
    fn test_broadcast_mul() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        let a = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let b = Tensor::vector(std::vec![10.0]);

        let result = ops.mul(&a, &b).unwrap();
        assert_eq!(result.data(), &[10.0, 20.0, 30.0, 40.0]);
    }

    #[test]
    fn test_broadcast_div() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        let a = Tensor::vector(std::vec![10.0, 20.0, 30.0]);
        let b = Tensor::vector(std::vec![2.0]);

        let result = ops.div(&a, &b).unwrap();
        assert_eq!(result.data(), &[5.0, 10.0, 15.0]);
    }

    #[test]
    fn test_broadcast_incompatible_shapes() {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // (3, 4) and (3, 5) are incompatible
        let a = Tensor::matrix(std::vec![1.0; 12], 3, 4).unwrap();
        let b = Tensor::matrix(std::vec![1.0; 15], 3, 5).unwrap();

        assert!(ops.add(&a, &b).is_none());
    }

    #[test]
    fn test_broadcast_with_different_backends() {
        // Test that broadcasting works with different hardware capabilities
        for caps in [
            HardwareCapabilities::NONE,
            HardwareCapabilities::NEON,
            HardwareCapabilities::AVX2,
            HardwareCapabilities::SVE2,
        ] {
            let ops = TensorOps::new(caps);

            let a = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
            let b = Tensor::vector(std::vec![10.0]);

            let result = ops.add(&a, &b).unwrap();
            assert_eq!(result.data(), &[11.0, 12.0, 13.0]);
        }
    }
}
