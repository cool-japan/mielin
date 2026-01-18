//! Activation functions for neural network operations
//!
//! Provides SIMD-optimized implementations of common activation functions:
//! - ReLU (Rectified Linear Unit)
//! - Leaky ReLU
//! - Sigmoid
//! - Tanh
//! - Softmax
//! - GELU (Gaussian Error Linear Unit)

extern crate alloc;

use crate::tensor::Tensor;
use mielin_hal::capabilities::HardwareCapabilities;

/// Activation function implementations with hardware acceleration
pub struct Activation {
    capabilities: HardwareCapabilities,
}

impl Activation {
    /// Create a new activation function handler
    pub fn new(capabilities: HardwareCapabilities) -> Self {
        Self { capabilities }
    }

    /// ReLU (Rectified Linear Unit): max(0, x)
    ///
    /// Returns 0 for negative inputs, passes positive inputs unchanged.
    pub fn relu(&self, input: &Tensor<f32>) -> Tensor<f32> {
        let mut output = Tensor::zeros(input.shape().to_vec());

        if self.capabilities.contains(HardwareCapabilities::AVX2) {
            #[cfg(target_arch = "x86_64")]
            {
                self.relu_avx2(input.data(), output.data_mut());
                return output;
            }
        }

        if self.capabilities.contains(HardwareCapabilities::NEON) {
            #[cfg(target_arch = "aarch64")]
            {
                self.relu_neon(input.data(), output.data_mut());
                return output;
            }
        }

        self.relu_scalar(input.data(), output.data_mut());
        output
    }

    /// Leaky ReLU: max(alpha * x, x) where alpha is typically 0.01
    pub fn leaky_relu(&self, input: &Tensor<f32>, alpha: f32) -> Tensor<f32> {
        let mut output = Tensor::zeros(input.shape().to_vec());

        if self.capabilities.contains(HardwareCapabilities::AVX2) {
            #[cfg(target_arch = "x86_64")]
            {
                self.leaky_relu_avx2(input.data(), output.data_mut(), alpha);
                return output;
            }
        }

        if self.capabilities.contains(HardwareCapabilities::NEON) {
            #[cfg(target_arch = "aarch64")]
            {
                self.leaky_relu_neon(input.data(), output.data_mut(), alpha);
                return output;
            }
        }

        self.leaky_relu_scalar(input.data(), output.data_mut(), alpha);
        output
    }

    /// Sigmoid: 1 / (1 + exp(-x))
    ///
    /// Maps inputs to range (0, 1).
    pub fn sigmoid(&self, input: &Tensor<f32>) -> Tensor<f32> {
        let mut output = Tensor::zeros(input.shape().to_vec());

        // Use fast approximation for SIMD
        if self.capabilities.contains(HardwareCapabilities::AVX2) {
            #[cfg(target_arch = "x86_64")]
            {
                self.sigmoid_avx2(input.data(), output.data_mut());
                return output;
            }
        }

        if self.capabilities.contains(HardwareCapabilities::NEON) {
            #[cfg(target_arch = "aarch64")]
            {
                self.sigmoid_neon(input.data(), output.data_mut());
                return output;
            }
        }

        self.sigmoid_scalar(input.data(), output.data_mut());
        output
    }

    /// Tanh (Hyperbolic Tangent): (exp(x) - exp(-x)) / (exp(x) + exp(-x))
    ///
    /// Maps inputs to range (-1, 1).
    pub fn tanh(&self, input: &Tensor<f32>) -> Tensor<f32> {
        let mut output = Tensor::zeros(input.shape().to_vec());

        if self.capabilities.contains(HardwareCapabilities::AVX2) {
            #[cfg(target_arch = "x86_64")]
            {
                self.tanh_avx2(input.data(), output.data_mut());
                return output;
            }
        }

        if self.capabilities.contains(HardwareCapabilities::NEON) {
            #[cfg(target_arch = "aarch64")]
            {
                self.tanh_neon(input.data(), output.data_mut());
                return output;
            }
        }

        self.tanh_scalar(input.data(), output.data_mut());
        output
    }

    /// Softmax: exp(x_i) / sum(exp(x_j))
    ///
    /// Converts a vector of values into a probability distribution.
    /// Applies along the last dimension.
    pub fn softmax(&self, input: &Tensor<f32>) -> Tensor<f32> {
        let mut output = Tensor::zeros(input.shape().to_vec());

        // Softmax is applied along the last axis
        // For vectors, it's the whole vector
        // For matrices, it's each row

        let ndim = input.ndim();

        if ndim == 1 {
            self.softmax_1d(input.data(), output.data_mut());
        } else if ndim == 2 {
            let shape = input.shape();
            let rows = shape[0];
            let cols = shape[1];

            for i in 0..rows {
                let start = i * cols;
                let end = start + cols;
                self.softmax_1d(
                    &input.data()[start..end],
                    &mut output.data_mut()[start..end],
                );
            }
        } else {
            // For higher dimensions, flatten and apply row-wise
            self.softmax_1d(input.data(), output.data_mut());
        }

        output
    }

    /// GELU (Gaussian Error Linear Unit): x * Phi(x) where Phi is the CDF of N(0,1)
    ///
    /// Uses the approximation: 0.5 * x * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
    pub fn gelu(&self, input: &Tensor<f32>) -> Tensor<f32> {
        let mut output = Tensor::zeros(input.shape().to_vec());

        if self.capabilities.contains(HardwareCapabilities::AVX2) {
            #[cfg(target_arch = "x86_64")]
            {
                self.gelu_avx2(input.data(), output.data_mut());
                return output;
            }
        }

        if self.capabilities.contains(HardwareCapabilities::NEON) {
            #[cfg(target_arch = "aarch64")]
            {
                self.gelu_neon(input.data(), output.data_mut());
                return output;
            }
        }

        self.gelu_scalar(input.data(), output.data_mut());
        output
    }

    // ========== Scalar Implementations ==========

    #[inline]
    fn relu_scalar(&self, input: &[f32], output: &mut [f32]) {
        for (i, &x) in input.iter().enumerate() {
            output[i] = if x > 0.0 { x } else { 0.0 };
        }
    }

    #[inline]
    fn leaky_relu_scalar(&self, input: &[f32], output: &mut [f32], alpha: f32) {
        for (i, &x) in input.iter().enumerate() {
            output[i] = if x > 0.0 { x } else { alpha * x };
        }
    }

    #[inline]
    fn sigmoid_scalar(&self, input: &[f32], output: &mut [f32]) {
        for (i, &x) in input.iter().enumerate() {
            // Clamp to prevent overflow
            let x_clamped = x.clamp(-88.0, 88.0);
            output[i] = 1.0 / (1.0 + libm::expf(-x_clamped));
        }
    }

    #[inline]
    fn tanh_scalar(&self, input: &[f32], output: &mut [f32]) {
        for (i, &x) in input.iter().enumerate() {
            // Use libm for no_std compatibility
            output[i] = libm::tanhf(x);
        }
    }

    #[inline]
    fn softmax_1d(&self, input: &[f32], output: &mut [f32]) {
        // Numerical stability: subtract max before exp
        let max_val = input.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        let mut sum = 0.0f32;
        for (i, &x) in input.iter().enumerate() {
            let exp_val = libm::expf(x - max_val);
            output[i] = exp_val;
            sum += exp_val;
        }

        // Normalize
        let inv_sum = 1.0 / sum;
        for val in output.iter_mut() {
            *val *= inv_sum;
        }
    }

    #[inline]
    fn gelu_scalar(&self, input: &[f32], output: &mut [f32]) {
        // GELU approximation: 0.5 * x * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
        const SQRT_2_OVER_PI: f32 = 0.797_884_6;
        const COEFF: f32 = 0.044715;

        for (i, &x) in input.iter().enumerate() {
            let x3 = x * x * x;
            let inner = SQRT_2_OVER_PI * (x + COEFF * x3);
            output[i] = 0.5 * x * (1.0 + libm::tanhf(inner));
        }
    }

    // ========== AVX2 Implementations ==========

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn relu_avx2(&self, input: &[f32], output: &mut [f32]) {
        use core::arch::x86_64::*;

        let len = input.len();
        let simd_len = len - (len % 8);

        unsafe {
            let zero = _mm256_setzero_ps();

            for i in (0..simd_len).step_by(8) {
                let x = _mm256_loadu_ps(input.as_ptr().add(i));
                let result = _mm256_max_ps(zero, x);
                _mm256_storeu_ps(output.as_mut_ptr().add(i), result);
            }
        }

        // Handle remainder
        for i in simd_len..len {
            output[i] = if input[i] > 0.0 { input[i] } else { 0.0 };
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn leaky_relu_avx2(&self, input: &[f32], output: &mut [f32], alpha: f32) {
        use core::arch::x86_64::*;

        let len = input.len();
        let simd_len = len - (len % 8);

        unsafe {
            let zero = _mm256_setzero_ps();
            let alpha_vec = _mm256_set1_ps(alpha);

            for i in (0..simd_len).step_by(8) {
                let x = _mm256_loadu_ps(input.as_ptr().add(i));
                let leaky = _mm256_mul_ps(x, alpha_vec);

                // Blend: if x > 0, use x; else use leaky
                let mask = _mm256_cmp_ps(x, zero, _CMP_GT_OQ);
                let result = _mm256_blendv_ps(leaky, x, mask);
                _mm256_storeu_ps(output.as_mut_ptr().add(i), result);
            }
        }

        // Handle remainder
        for i in simd_len..len {
            output[i] = if input[i] > 0.0 {
                input[i]
            } else {
                alpha * input[i]
            };
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn sigmoid_avx2(&self, input: &[f32], output: &mut [f32]) {
        // Fast sigmoid approximation using polynomial
        // For very high accuracy, we use scalar fallback
        self.sigmoid_scalar(input, output);
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn tanh_avx2(&self, input: &[f32], output: &mut [f32]) {
        // tanh requires exp, use scalar for accuracy
        self.tanh_scalar(input, output);
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn gelu_avx2(&self, input: &[f32], output: &mut [f32]) {
        // GELU requires tanh, use scalar for accuracy
        self.gelu_scalar(input, output);
    }

    // ========== NEON Implementations ==========

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn relu_neon(&self, input: &[f32], output: &mut [f32]) {
        use core::arch::aarch64::*;

        let len = input.len();
        let simd_len = len - (len % 4);

        unsafe {
            let zero = vdupq_n_f32(0.0);

            for i in (0..simd_len).step_by(4) {
                let x = vld1q_f32(input.as_ptr().add(i));
                let result = vmaxq_f32(zero, x);
                vst1q_f32(output.as_mut_ptr().add(i), result);
            }
        }

        // Handle remainder
        for i in simd_len..len {
            output[i] = if input[i] > 0.0 { input[i] } else { 0.0 };
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn leaky_relu_neon(&self, input: &[f32], output: &mut [f32], alpha: f32) {
        use core::arch::aarch64::*;

        let len = input.len();
        let simd_len = len - (len % 4);

        unsafe {
            let zero = vdupq_n_f32(0.0);
            let alpha_vec = vdupq_n_f32(alpha);

            for i in (0..simd_len).step_by(4) {
                let x = vld1q_f32(input.as_ptr().add(i));
                let leaky = vmulq_f32(x, alpha_vec);

                // Compare x > 0
                let mask = vcgtq_f32(x, zero);
                // Select: if x > 0, use x; else use leaky
                let result = vbslq_f32(mask, x, leaky);
                vst1q_f32(output.as_mut_ptr().add(i), result);
            }
        }

        // Handle remainder
        for i in simd_len..len {
            output[i] = if input[i] > 0.0 {
                input[i]
            } else {
                alpha * input[i]
            };
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn sigmoid_neon(&self, input: &[f32], output: &mut [f32]) {
        self.sigmoid_scalar(input, output);
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn tanh_neon(&self, input: &[f32], output: &mut [f32]) {
        self.tanh_scalar(input, output);
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn gelu_neon(&self, input: &[f32], output: &mut [f32]) {
        self.gelu_scalar(input, output);
    }
}

/// Convenience functions for common activation operations
pub mod functional {
    use super::*;

    /// Apply ReLU activation
    pub fn relu(input: &Tensor<f32>) -> Tensor<f32> {
        let activation = Activation::new(HardwareCapabilities::NONE);
        activation.relu(input)
    }

    /// Apply Leaky ReLU activation with default alpha=0.01
    pub fn leaky_relu(input: &Tensor<f32>) -> Tensor<f32> {
        let activation = Activation::new(HardwareCapabilities::NONE);
        activation.leaky_relu(input, 0.01)
    }

    /// Apply Sigmoid activation
    pub fn sigmoid(input: &Tensor<f32>) -> Tensor<f32> {
        let activation = Activation::new(HardwareCapabilities::NONE);
        activation.sigmoid(input)
    }

    /// Apply Tanh activation
    pub fn tanh(input: &Tensor<f32>) -> Tensor<f32> {
        let activation = Activation::new(HardwareCapabilities::NONE);
        activation.tanh(input)
    }

    /// Apply Softmax activation
    pub fn softmax(input: &Tensor<f32>) -> Tensor<f32> {
        let activation = Activation::new(HardwareCapabilities::NONE);
        activation.softmax(input)
    }

    /// Apply GELU activation
    pub fn gelu(input: &Tensor<f32>) -> Tensor<f32> {
        let activation = Activation::new(HardwareCapabilities::NONE);
        activation.gelu(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    const EPSILON: f32 = 1e-5;

    fn assert_near(a: f32, b: f32, eps: f32) {
        assert!(
            (a - b).abs() < eps,
            "assertion failed: {} not near {} (diff: {})",
            a,
            b,
            (a - b).abs()
        );
    }

    #[test]
    fn test_relu() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        let input = Tensor::vector(std::vec![-2.0, -1.0, 0.0, 1.0, 2.0]);
        let output = activation.relu(&input);

        assert_eq!(output.data(), &[0.0, 0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_leaky_relu() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        let input = Tensor::vector(std::vec![-2.0, -1.0, 0.0, 1.0, 2.0]);
        let output = activation.leaky_relu(&input, 0.1);

        assert_near(output.data()[0], -0.2, EPSILON);
        assert_near(output.data()[1], -0.1, EPSILON);
        assert_near(output.data()[2], 0.0, EPSILON);
        assert_near(output.data()[3], 1.0, EPSILON);
        assert_near(output.data()[4], 2.0, EPSILON);
    }

    #[test]
    fn test_sigmoid() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        let input = Tensor::vector(std::vec![-10.0, 0.0, 10.0]);
        let output = activation.sigmoid(&input);

        // sigmoid(-10) ~ 0, sigmoid(0) = 0.5, sigmoid(10) ~ 1
        assert!(output.data()[0] < 0.001);
        assert_near(output.data()[1], 0.5, EPSILON);
        assert!(output.data()[2] > 0.999);
    }

    #[test]
    fn test_tanh() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        let input = Tensor::vector(std::vec![-10.0, 0.0, 10.0]);
        let output = activation.tanh(&input);

        // tanh(-10) ~ -1, tanh(0) = 0, tanh(10) ~ 1
        assert!(output.data()[0] < -0.999);
        assert_near(output.data()[1], 0.0, EPSILON);
        assert!(output.data()[2] > 0.999);
    }

    #[test]
    fn test_softmax() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        let input = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let output = activation.softmax(&input);

        // Sum should be 1.0
        let sum: f32 = output.data().iter().sum();
        assert_near(sum, 1.0, EPSILON);

        // Values should be in increasing order
        assert!(output.data()[0] < output.data()[1]);
        assert!(output.data()[1] < output.data()[2]);
    }

    #[test]
    fn test_softmax_numerical_stability() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        // Large values that could cause overflow without proper handling
        let input = Tensor::vector(std::vec![1000.0, 1001.0, 1002.0]);
        let output = activation.softmax(&input);

        // Sum should still be 1.0
        let sum: f32 = output.data().iter().sum();
        assert_near(sum, 1.0, EPSILON);

        // Should not have NaN or Inf
        assert!(output.data().iter().all(|&x| x.is_finite()));
    }

    #[test]
    fn test_softmax_2d() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        // 2x3 matrix - softmax applied to each row
        let input = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
        let output = activation.softmax(&input);

        // Each row should sum to 1.0
        let row1_sum: f32 = output.data()[0..3].iter().sum();
        let row2_sum: f32 = output.data()[3..6].iter().sum();

        assert_near(row1_sum, 1.0, EPSILON);
        assert_near(row2_sum, 1.0, EPSILON);
    }

    #[test]
    fn test_gelu() {
        let activation = Activation::new(HardwareCapabilities::NONE);
        let input = Tensor::vector(std::vec![-2.0, 0.0, 2.0]);
        let output = activation.gelu(&input);

        // GELU(-2) ~ -0.0454, GELU(0) = 0, GELU(2) ~ 1.9545
        assert!(output.data()[0] < 0.0);
        assert_near(output.data()[1], 0.0, EPSILON);
        assert!(output.data()[2] > 1.9 && output.data()[2] < 2.0);
    }

    #[test]
    fn test_functional_api() {
        use functional::*;

        let input = Tensor::vector(std::vec![-1.0, 0.0, 1.0]);

        let relu_out = relu(&input);
        assert_eq!(relu_out.data(), &[0.0, 0.0, 1.0]);

        let sigmoid_out = sigmoid(&input);
        assert!(sigmoid_out.data()[1] > 0.49 && sigmoid_out.data()[1] < 0.51);

        let tanh_out = tanh(&input);
        assert_near(tanh_out.data()[1], 0.0, EPSILON);
    }

    #[test]
    fn test_relu_avx2_fallback() {
        // Test AVX2 path on x86_64, scalar fallback on other architectures
        let activation = Activation::new(HardwareCapabilities::AVX2);
        let input = Tensor::vector(std::vec![
            -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0
        ]);
        let output = activation.relu(&input);

        assert_eq!(
            output.data(),
            &[0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]
        );
    }

    #[test]
    fn test_leaky_relu_avx2_fallback() {
        let activation = Activation::new(HardwareCapabilities::AVX2);
        let input = Tensor::vector(std::vec![
            -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0
        ]);
        let output = activation.leaky_relu(&input, 0.1);

        assert_near(output.data()[0], -0.2, EPSILON);
        assert_near(output.data()[1], -0.1, EPSILON);
        assert_eq!(output.data()[3..], [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }
}
