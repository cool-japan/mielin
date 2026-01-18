//! Quantization Module
//!
//! Provides quantization for neural network inference optimization:
//! - INT8 quantization (8-bit integers)
//! - INT4 quantization (4-bit integers, packed)
//! - Mixed precision (f16/bf16/f32 operations)
//! - Quantization-aware training utilities
//!
//! # Quantization Benefits
//! - Reduced model size (2-8x compression)
//! - Faster inference (2-4x speedup on compatible hardware)
//! - Lower power consumption
//! - Enables deployment on edge devices
//!
//! # Quantization Schemes
//! - **Symmetric**: zero-point = 0, simple scaling
//! - **Asymmetric**: arbitrary zero-point, better range utilization
//! - **Per-tensor**: single scale/zero-point for entire tensor
//! - **Per-channel**: separate scale/zero-point per output channel

#![allow(dead_code)]

extern crate alloc;

use crate::tensor::Tensor;
use alloc::vec::Vec;
use libm::roundf;

/// Quantization scheme
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantScheme {
    /// Symmetric quantization (zero-point = 0)
    Symmetric,
    /// Asymmetric quantization (arbitrary zero-point)
    Asymmetric,
}

/// Quantization granularity
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantGranularity {
    /// Per-tensor quantization (single scale/zero-point)
    PerTensor,
    /// Per-channel quantization (scale/zero-point per channel)
    PerChannel,
}

/// Quantization parameters for INT8
#[derive(Debug, Clone)]
pub struct QuantParams {
    /// Scale factor: real_value = (quantized_value - zero_point) * scale
    pub scale: f32,
    /// Zero point (offset)
    pub zero_point: i32,
    /// Quantization scheme
    pub scheme: QuantScheme,
}

impl QuantParams {
    /// Create symmetric quantization parameters
    pub fn symmetric(min_val: f32, max_val: f32) -> Self {
        let abs_max = min_val.abs().max(max_val.abs());
        // Avoid division by zero
        let scale = if abs_max == 0.0 {
            1.0
        } else {
            abs_max / 127.0 // INT8 range: -128 to 127
        };

        Self {
            scale,
            zero_point: 0,
            scheme: QuantScheme::Symmetric,
        }
    }

    /// Create asymmetric quantization parameters
    pub fn asymmetric(min_val: f32, max_val: f32) -> Self {
        let scale = (max_val - min_val) / 255.0; // INT8 range: -128 to 127
        let zero_point = roundf(-min_val / scale) as i32 - 128;

        Self {
            scale,
            zero_point,
            scheme: QuantScheme::Asymmetric,
        }
    }

    /// Quantize a single value
    pub fn quantize(&self, value: f32) -> i8 {
        let quant = roundf(value / self.scale) + self.zero_point as f32;
        quant.clamp(-128.0, 127.0) as i8
    }

    /// Dequantize a single value
    pub fn dequantize(&self, value: i8) -> f32 {
        (value as f32 - self.zero_point as f32) * self.scale
    }
}

/// INT8 quantized tensor
#[derive(Debug, Clone)]
pub struct QuantizedTensor {
    /// Quantized data (INT8)
    data: Vec<i8>,
    /// Original tensor shape
    shape: Vec<usize>,
    /// Quantization parameters
    params: QuantParams,
    /// Granularity (per-tensor or per-channel)
    granularity: QuantGranularity,
}

impl QuantizedTensor {
    /// Quantize a floating-point tensor to INT8
    pub fn from_tensor(
        tensor: &Tensor<f32>,
        scheme: QuantScheme,
        granularity: QuantGranularity,
    ) -> Self {
        match granularity {
            QuantGranularity::PerTensor => Self::quantize_per_tensor(tensor, scheme),
            QuantGranularity::PerChannel => Self::quantize_per_channel(tensor, scheme),
        }
    }

    /// Per-tensor quantization
    fn quantize_per_tensor(tensor: &Tensor<f32>, scheme: QuantScheme) -> Self {
        let data_slice = tensor.data();
        let min_val = data_slice.iter().copied().fold(f32::INFINITY, f32::min);
        let max_val = data_slice.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        let params = match scheme {
            QuantScheme::Symmetric => QuantParams::symmetric(min_val, max_val),
            QuantScheme::Asymmetric => QuantParams::asymmetric(min_val, max_val),
        };

        let quantized_data: Vec<i8> = data_slice.iter().map(|&val| params.quantize(val)).collect();

        Self {
            data: quantized_data,
            shape: tensor.shape().to_vec(),
            params,
            granularity: QuantGranularity::PerTensor,
        }
    }

    /// Per-channel quantization (for weights in Conv2D/Linear layers)
    fn quantize_per_channel(tensor: &Tensor<f32>, scheme: QuantScheme) -> Self {
        // For simplicity, use per-tensor for now
        // TODO: Implement true per-channel quantization
        Self::quantize_per_tensor(tensor, scheme)
    }

    /// Dequantize back to floating-point tensor
    pub fn dequantize(&self) -> Tensor<f32> {
        let dequantized_data: Vec<f32> = self
            .data
            .iter()
            .map(|&val| self.params.dequantize(val))
            .collect();

        Tensor::from_vec(dequantized_data, self.shape.clone()).unwrap()
    }

    /// Get quantized data
    pub fn data(&self) -> &[i8] {
        &self.data
    }

    /// Get shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get quantization parameters
    pub fn params(&self) -> &QuantParams {
        &self.params
    }

    /// Compute quantized matrix multiplication (INT8 x INT8 -> INT32 -> F32)
    pub fn matmul_quant(&self, other: &QuantizedTensor) -> Tensor<f32> {
        assert_eq!(self.shape.len(), 2, "First tensor must be 2D");
        assert_eq!(other.shape.len(), 2, "Second tensor must be 2D");
        assert_eq!(self.shape[1], other.shape[0], "Shape mismatch for matmul");

        let m = self.shape[0];
        let k = self.shape[1];
        let n = other.shape[1];

        let mut result_data = alloc::vec![0.0f32; m * n];

        for i in 0..m {
            for j in 0..n {
                let mut sum: i32 = 0;
                for p in 0..k {
                    let a = self.data[i * k + p] as i32;
                    let b = other.data[p * n + j] as i32;
                    sum += a * b;
                }

                // Dequantize: result = scale_a * scale_b * (sum - corrections)
                let scale = self.params.scale * other.params.scale;
                result_data[i * n + j] = sum as f32 * scale;
            }
        }

        Tensor::from_vec(result_data, alloc::vec![m, n]).unwrap()
    }
}

/// INT4 quantized tensor (packed, 2 values per byte)
#[derive(Debug, Clone)]
pub struct Quant4Tensor {
    /// Packed INT4 data (2 values per byte)
    data: Vec<u8>,
    /// Original tensor shape
    shape: Vec<usize>,
    /// Quantization parameters
    params: QuantParams,
    /// Total number of elements
    numel: usize,
}

impl Quant4Tensor {
    /// Quantize a floating-point tensor to INT4 (packed)
    pub fn from_tensor(tensor: &Tensor<f32>, scheme: QuantScheme) -> Self {
        let data_slice = tensor.data();
        let numel = data_slice.len();
        let min_val = data_slice.iter().copied().fold(f32::INFINITY, f32::min);
        let max_val = data_slice.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        // INT4 range: -8 to 7
        let params = match scheme {
            QuantScheme::Symmetric => {
                let abs_max = min_val.abs().max(max_val.abs());
                let scale = abs_max / 7.0;
                QuantParams {
                    scale,
                    zero_point: 0,
                    scheme: QuantScheme::Symmetric,
                }
            }
            QuantScheme::Asymmetric => {
                let scale = (max_val - min_val) / 15.0;
                let zero_point = roundf(-min_val / scale) as i32 - 8;
                QuantParams {
                    scale,
                    zero_point,
                    scheme: QuantScheme::Asymmetric,
                }
            }
        };

        // Quantize and pack 2 values per byte
        let packed_size = numel.div_ceil(2);
        let mut packed_data = alloc::vec![0u8; packed_size];

        for (i, &value) in data_slice.iter().enumerate().take(numel) {
            let quant = roundf(value / params.scale) + params.zero_point as f32;
            let quant_i4 = (quant.clamp(-8.0, 7.0) as i8) & 0x0F;

            let byte_idx = i / 2;
            if i % 2 == 0 {
                // Store in lower nibble
                packed_data[byte_idx] = (quant_i4 as u8) & 0x0F;
            } else {
                // Store in upper nibble
                packed_data[byte_idx] |= ((quant_i4 as u8) & 0x0F) << 4;
            }
        }

        Self {
            data: packed_data,
            shape: tensor.shape().to_vec(),
            params,
            numel,
        }
    }

    /// Dequantize back to floating-point tensor
    pub fn dequantize(&self) -> Tensor<f32> {
        let mut dequantized_data = alloc::vec![0.0f32; self.numel];

        for (i, dequant_val) in dequantized_data.iter_mut().enumerate().take(self.numel) {
            let byte_idx = i / 2;
            let nibble = if i % 2 == 0 {
                (self.data[byte_idx] & 0x0F) as i8
            } else {
                ((self.data[byte_idx] >> 4) & 0x0F) as i8
            };

            // Sign-extend from 4-bit to 8-bit
            let value_i8 = if nibble & 0x08 != 0 {
                nibble | (-16i8) // Negative: extend with 1s (0xF0 = -16)
            } else {
                nibble // Positive: already correct
            };

            *dequant_val = (value_i8 as f32 - self.params.zero_point as f32) * self.params.scale;
        }

        Tensor::from_vec(dequantized_data, self.shape.clone()).unwrap()
    }

    /// Get packed data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get compression ratio vs f32
    pub fn compression_ratio(&self) -> f32 {
        let original_size = self.numel * 4; // f32 = 4 bytes
        let compressed_size = self.data.len(); // packed INT4
        original_size as f32 / compressed_size as f32
    }
}

/// Calibration for quantization-aware training
pub struct QuantCalibrator {
    /// Running minimum value
    min_val: f32,
    /// Running maximum value
    max_val: f32,
    /// Number of batches observed
    num_batches: usize,
}

impl QuantCalibrator {
    /// Create a new calibrator
    pub fn new() -> Self {
        Self {
            min_val: f32::INFINITY,
            max_val: f32::NEG_INFINITY,
            num_batches: 0,
        }
    }

    /// Update calibrator with a new batch
    pub fn update(&mut self, tensor: &Tensor<f32>) {
        let batch_min = tensor.data().iter().copied().fold(f32::INFINITY, f32::min);
        let batch_max = tensor
            .data()
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);

        self.min_val = self.min_val.min(batch_min);
        self.max_val = self.max_val.max(batch_max);
        self.num_batches += 1;
    }

    /// Get quantization parameters from calibration
    pub fn get_params(&self, scheme: QuantScheme) -> QuantParams {
        match scheme {
            QuantScheme::Symmetric => QuantParams::symmetric(self.min_val, self.max_val),
            QuantScheme::Asymmetric => QuantParams::asymmetric(self.min_val, self.max_val),
        }
    }

    /// Reset calibrator
    pub fn reset(&mut self) {
        self.min_val = f32::INFINITY;
        self.max_val = f32::NEG_INFINITY;
        self.num_batches = 0;
    }
}

impl Default for QuantCalibrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symmetric_quantization() {
        let params = QuantParams::symmetric(-127.0, 127.0);
        assert_eq!(params.zero_point, 0);
        assert!((params.scale - 1.0).abs() < 1e-6);

        // Test quantize/dequantize round-trip
        let original = 50.0;
        let quantized = params.quantize(original);
        let dequantized = params.dequantize(quantized);
        assert!((original - dequantized).abs() < 2.0); // Allow some error
    }

    #[test]
    fn test_asymmetric_quantization() {
        let params = QuantParams::asymmetric(0.0, 255.0);
        assert!((params.scale - 1.0).abs() < 1e-6);

        let original = 100.0;
        let quantized = params.quantize(original);
        let dequantized = params.dequantize(quantized);
        assert!((original - dequantized).abs() < 2.0);
    }

    #[test]
    fn test_quantize_tensor_symmetric() {
        let tensor = Tensor::vector(alloc::vec![-10.0, -5.0, 0.0, 5.0, 10.0]);
        let quant = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Symmetric,
            QuantGranularity::PerTensor,
        );

        assert_eq!(quant.shape(), &[5]);
        assert_eq!(quant.params().scheme, QuantScheme::Symmetric);
        assert_eq!(quant.params().zero_point, 0);

        // Dequantize and check
        let dequant = quant.dequantize();
        for (i, &original) in tensor.data().iter().enumerate() {
            let recovered = dequant.data()[i];
            assert!((original - recovered).abs() < 1.0); // Quantization error
        }
    }

    #[test]
    fn test_quantize_tensor_asymmetric() {
        let tensor = Tensor::vector(alloc::vec![0.0, 25.5, 51.0, 76.5, 102.0]);
        let quant = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Asymmetric,
            QuantGranularity::PerTensor,
        );

        assert_eq!(quant.shape(), &[5]);
        assert_eq!(quant.params().scheme, QuantScheme::Asymmetric);

        // Dequantize and check
        let dequant = quant.dequantize();
        for (i, &original) in tensor.data().iter().enumerate() {
            let recovered = dequant.data()[i];
            assert!((original - recovered).abs() < 2.0);
        }
    }

    #[test]
    fn test_int4_quantization() {
        let tensor = Tensor::vector(alloc::vec![-7.0, -3.0, 0.0, 3.0, 7.0]);
        let quant4 = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);

        assert_eq!(quant4.shape(), &[5]);
        // 5 values packed into 3 bytes (2 values per byte, last byte half-used)
        assert_eq!(quant4.data().len(), 3);

        // Check compression ratio
        // Original: 5 * 4 bytes = 20 bytes
        // Compressed: 3 bytes
        // Ratio: 20 / 3 ≈ 6.67
        let ratio = quant4.compression_ratio();
        assert!(ratio > 6.0 && ratio < 7.0); // Should be ~6.67x

        // Dequantize and check
        let dequant = quant4.dequantize();
        for (i, &original) in tensor.data().iter().enumerate() {
            let recovered = dequant.data()[i];
            assert!((original - recovered).abs() < 2.0);
        }
    }

    #[test]
    fn test_quantized_matmul() {
        let a = Tensor::matrix(alloc::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let b = Tensor::matrix(alloc::vec![5.0, 6.0, 7.0, 8.0], 2, 2).unwrap();

        let a_quant =
            QuantizedTensor::from_tensor(&a, QuantScheme::Symmetric, QuantGranularity::PerTensor);
        let b_quant =
            QuantizedTensor::from_tensor(&b, QuantScheme::Symmetric, QuantGranularity::PerTensor);

        let result = a_quant.matmul_quant(&b_quant);

        // Expected result: [19, 22; 43, 50]
        assert_eq!(result.shape(), &[2, 2]);

        // Check approximate correctness (quantization introduces error)
        let expected = alloc::vec![19.0, 22.0, 43.0, 50.0];
        for (i, &exp) in expected.iter().enumerate() {
            let diff = (result.data()[i] - exp).abs();
            assert!(diff < 3.0, "Expected {}, got {}", exp, result.data()[i]);
        }
    }

    #[test]
    fn test_calibrator() {
        let mut calibrator = QuantCalibrator::new();

        let batch1 = Tensor::vector(alloc::vec![1.0, 2.0, 3.0]);
        let batch2 = Tensor::vector(alloc::vec![-5.0, 0.0, 10.0]);

        calibrator.update(&batch1);
        calibrator.update(&batch2);

        assert_eq!(calibrator.num_batches, 2);
        assert_eq!(calibrator.min_val, -5.0);
        assert_eq!(calibrator.max_val, 10.0);

        let params = calibrator.get_params(QuantScheme::Symmetric);
        assert_eq!(params.zero_point, 0);
    }

    #[test]
    fn test_quant_params_edge_cases() {
        // All zeros
        let params = QuantParams::symmetric(0.0, 0.0);
        assert_eq!(params.quantize(0.0), 0);

        // Very small range
        let params = QuantParams::asymmetric(0.001, 0.002);
        let q = params.quantize(0.0015);
        let dq = params.dequantize(q);
        assert!((dq - 0.0015).abs() < 0.001);
    }

    #[test]
    fn test_int4_packing() {
        let tensor = Tensor::vector(alloc::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, -1.0]);
        let quant4 = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);

        // 8 values should pack into 4 bytes
        assert_eq!(quant4.data().len(), 4);

        // Verify round-trip
        let dequant = quant4.dequantize();
        assert_eq!(dequant.shape(), tensor.shape());
        assert_eq!(dequant.data().len(), 8);
    }
}
