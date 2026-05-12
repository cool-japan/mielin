//! Mixed Precision Training and Inference
//!
//! Provides support for FP16 (half precision) and BF16 (bfloat16) floating point formats.
//! These formats reduce memory usage and increase throughput on compatible hardware.
//!
//! # Precision Formats
//! - **FP32**: 1 sign + 8 exponent + 23 mantissa (baseline, full precision)
//! - **FP16**: 1 sign + 5 exponent + 10 mantissa (half precision, IEEE 754)
//! - **BF16**: 1 sign + 8 exponent + 7 mantissa (bfloat16, truncated FP32)
//!
//! # Use Cases
//! - **FP16**: Better dynamic range for small values, higher precision
//! - **BF16**: Same exponent range as FP32, better for ML training
//!
//! # Hardware Support
//! - **NVIDIA**: Tensor Cores support FP16 and BF16
//! - **ARM**: Neoverse V1+ supports BF16
//! - **Intel**: AVX-512 BF16 extensions (Cooper Lake+)
//! - **Apple**: M1+ Neural Engine supports FP16

#![allow(dead_code)]

extern crate alloc;

use crate::tensor::Tensor;
use alloc::vec::Vec;

/// Half-precision floating point (FP16)
/// IEEE 754 half precision: 1 sign + 5 exponent + 10 mantissa
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct F16(u16);

impl F16 {
    /// Create FP16 from F32
    pub fn from_f32(value: f32) -> Self {
        let bits = value.to_bits();
        let sign = (bits >> 31) as u16;
        let exp = ((bits >> 23) & 0xFF) as i16;
        let mantissa = bits & 0x7FFFFF;

        // Handle special cases
        if exp == 0xFF {
            // Inf or NaN
            if mantissa == 0 {
                // Infinity
                return F16((sign << 15) | 0x7C00);
            } else {
                // NaN
                return F16((sign << 15) | 0x7C00 | 0x0200);
            }
        }

        // Rebias exponent: F32 bias=127, F16 bias=15
        let exp_rebias = exp - 127 + 15;

        if exp_rebias <= 0 {
            // Underflow to zero or subnormal
            return F16(sign << 15);
        } else if exp_rebias >= 0x1F {
            // Overflow to infinity
            return F16((sign << 15) | 0x7C00);
        }

        // Convert mantissa from 23 bits to 10 bits (round to nearest)
        let mantissa_f16 = ((mantissa + 0x1000) >> 13) & 0x3FF;

        F16((sign << 15) | ((exp_rebias as u16) << 10) | mantissa_f16 as u16)
    }

    /// Convert FP16 to F32
    pub fn to_f32(self) -> f32 {
        let bits = self.0;
        let sign = (bits >> 15) & 1;
        let exp = ((bits >> 10) & 0x1F) as i32;
        let mantissa = (bits & 0x3FF) as u32;

        // Handle special cases
        if exp == 0x1F {
            // Inf or NaN
            if mantissa == 0 {
                // Infinity
                return f32::from_bits((sign as u32) << 31 | 0x7F800000);
            } else {
                // NaN
                return f32::NAN;
            }
        }

        if exp == 0 {
            if mantissa == 0 {
                // Zero
                return f32::from_bits((sign as u32) << 31);
            }
            // Subnormal - not fully supported, return zero
            return f32::from_bits((sign as u32) << 31);
        }

        // Rebias exponent: F16 bias=15, F32 bias=127
        let exp_rebias = exp - 15 + 127;

        // Convert mantissa from 10 bits to 23 bits
        let mantissa_f32 = mantissa << 13;

        f32::from_bits((sign as u32) << 31 | (exp_rebias as u32) << 23 | mantissa_f32)
    }

    /// Get raw bits
    pub fn to_bits(self) -> u16 {
        self.0
    }

    /// Create from raw bits
    pub fn from_bits(bits: u16) -> Self {
        F16(bits)
    }
}

/// Brain floating point 16 (BF16)
/// Google bfloat16: 1 sign + 8 exponent + 7 mantissa (truncated FP32)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BF16(u16);

impl BF16 {
    /// Create BF16 from F32 (simple truncation)
    pub fn from_f32(value: f32) -> Self {
        let bits = value.to_bits();
        // BF16 is just the upper 16 bits of F32 (with rounding)
        // Simple rounding: add 0x7FFF to round to nearest even
        let rounded = bits + ((bits >> 16) & 1) + 0x7FFF;
        BF16((rounded >> 16) as u16)
    }

    /// Convert BF16 to F32
    pub fn to_f32(self) -> f32 {
        // BF16 is just F32 with lower 16 bits zeroed
        f32::from_bits((self.0 as u32) << 16)
    }

    /// Get raw bits
    pub fn to_bits(self) -> u16 {
        self.0
    }

    /// Create from raw bits
    pub fn from_bits(bits: u16) -> Self {
        BF16(bits)
    }
}

/// Mixed precision tensor
#[derive(Debug, Clone)]
pub enum MixedPrecisionTensor {
    /// Full precision (F32)
    F32(Tensor<f32>),
    /// Half precision (F16)
    F16 { data: Vec<F16>, shape: Vec<usize> },
    /// Brain floating point (BF16)
    BF16 { data: Vec<BF16>, shape: Vec<usize> },
}

impl MixedPrecisionTensor {
    /// Create FP16 tensor from F32 tensor
    pub fn to_f16(tensor: &Tensor<f32>) -> Self {
        let data: Vec<F16> = tensor.data().iter().map(|&v| F16::from_f32(v)).collect();
        Self::F16 {
            data,
            shape: tensor.shape().to_vec(),
        }
    }

    /// Create BF16 tensor from F32 tensor
    pub fn to_bf16(tensor: &Tensor<f32>) -> Self {
        let data: Vec<BF16> = tensor.data().iter().map(|&v| BF16::from_f32(v)).collect();
        Self::BF16 {
            data,
            shape: tensor.shape().to_vec(),
        }
    }

    /// Convert to F32 tensor
    pub fn to_f32(&self) -> Tensor<f32> {
        match self {
            Self::F32(tensor) => tensor.clone(),
            Self::F16 { data, shape } => {
                let f32_data: Vec<f32> = data.iter().map(|v| v.to_f32()).collect();
                Tensor::from_vec(f32_data, shape.clone()).expect("f16 data length matches its shape")
            }
            Self::BF16 { data, shape } => {
                let f32_data: Vec<f32> = data.iter().map(|v| v.to_f32()).collect();
                Tensor::from_vec(f32_data, shape.clone()).expect("bf16 data length matches its shape")
            }
        }
    }

    /// Get shape
    pub fn shape(&self) -> &[usize] {
        match self {
            Self::F32(tensor) => tensor.shape(),
            Self::F16 { shape, .. } | Self::BF16 { shape, .. } => shape,
        }
    }

    /// Get memory size in bytes
    pub fn size_bytes(&self) -> usize {
        let numel = self.shape().iter().product::<usize>();
        match self {
            Self::F32(_) => numel * 4,
            Self::F16 { .. } | Self::BF16 { .. } => numel * 2,
        }
    }

    /// Get compression ratio vs F32
    pub fn compression_ratio(&self) -> f32 {
        match self {
            Self::F32(_) => 1.0,
            Self::F16 { .. } | Self::BF16 { .. } => 2.0,
        }
    }

    /// Get precision type
    pub fn precision(&self) -> PrecisionType {
        match self {
            Self::F32(_) => PrecisionType::F32,
            Self::F16 { .. } => PrecisionType::F16,
            Self::BF16 { .. } => PrecisionType::BF16,
        }
    }
}

/// Precision type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecisionType {
    /// Full precision (32-bit)
    F32,
    /// Half precision (16-bit IEEE)
    F16,
    /// Brain floating point (16-bit)
    BF16,
}

impl PrecisionType {
    /// Get bit width
    pub fn bit_width(&self) -> u8 {
        match self {
            Self::F32 => 32,
            Self::F16 | Self::BF16 => 16,
        }
    }

    /// Get bytes per element
    pub fn bytes_per_element(&self) -> usize {
        (self.bit_width() / 8) as usize
    }

    /// Get exponent bits
    pub fn exponent_bits(&self) -> u8 {
        match self {
            Self::F32 | Self::BF16 => 8,
            Self::F16 => 5,
        }
    }

    /// Get mantissa bits
    pub fn mantissa_bits(&self) -> u8 {
        match self {
            Self::F32 => 23,
            Self::F16 => 10,
            Self::BF16 => 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f16_conversion() {
        let values = [0.0, 1.0, -1.0, 0.5, -0.5, 100.0, -100.0];

        for &val in &values {
            let f16 = F16::from_f32(val);
            let recovered = f16.to_f32();
            let error = (val - recovered).abs();

            // FP16 has ~3 decimal digits of precision
            if val.abs() < 1e-3 {
                assert!(
                    error < 1e-4,
                    "Value {} -> {} (error {})",
                    val,
                    recovered,
                    error
                );
            } else {
                let relative_error = error / val.abs();
                assert!(
                    relative_error < 0.001,
                    "Value {} -> {} (rel error {})",
                    val,
                    recovered,
                    relative_error
                );
            }
        }
    }

    #[test]
    fn test_bf16_conversion() {
        let values = [0.0, 1.0, -1.0, 0.5, -0.5, 100.0, -100.0, 1000.0];

        for &val in &values {
            let bf16 = BF16::from_f32(val);
            let recovered = bf16.to_f32();
            let error = (val - recovered).abs();

            // BF16 has ~2-3 decimal digits of precision
            if val.abs() < 1e-3 {
                assert!(
                    error < 1e-3,
                    "Value {} -> {} (error {})",
                    val,
                    recovered,
                    error
                );
            } else {
                let relative_error = error / val.abs();
                assert!(
                    relative_error < 0.01,
                    "Value {} -> {} (rel error {})",
                    val,
                    recovered,
                    relative_error
                );
            }
        }
    }

    #[test]
    fn test_f16_special_values() {
        // Zero
        assert_eq!(F16::from_f32(0.0).to_f32(), 0.0);
        assert_eq!(F16::from_f32(-0.0).to_f32(), -0.0);

        // Infinity
        let inf = F16::from_f32(f32::INFINITY);
        assert!(inf.to_f32().is_infinite());

        // NaN
        let nan = F16::from_f32(f32::NAN);
        assert!(nan.to_f32().is_nan());
    }

    #[test]
    fn test_bf16_special_values() {
        // Zero
        assert_eq!(BF16::from_f32(0.0).to_f32(), 0.0);

        // Large values
        let large = BF16::from_f32(1e6);
        assert!((large.to_f32() - 1e6).abs() < 1e3);
    }

    #[test]
    fn test_mixed_precision_tensor_f16() {
        let tensor = Tensor::vector(alloc::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let mp = MixedPrecisionTensor::to_f16(&tensor);

        assert_eq!(mp.shape(), &[5]);
        assert_eq!(mp.precision(), PrecisionType::F16);
        assert_eq!(mp.compression_ratio(), 2.0);

        let recovered = mp.to_f32();
        for (i, &original) in tensor.data().iter().enumerate() {
            let error = (original - recovered.data()[i]).abs();
            assert!(
                error < 0.01,
                "Value {} -> {}",
                original,
                recovered.data()[i]
            );
        }
    }

    #[test]
    fn test_mixed_precision_tensor_bf16() {
        let tensor = Tensor::vector(alloc::vec![1.0, 10.0, 100.0, 1000.0]);
        let mp = MixedPrecisionTensor::to_bf16(&tensor);

        assert_eq!(mp.shape(), &[4]);
        assert_eq!(mp.precision(), PrecisionType::BF16);
        assert_eq!(mp.compression_ratio(), 2.0);

        let recovered = mp.to_f32();
        for (i, &original) in tensor.data().iter().enumerate() {
            let error = (original - recovered.data()[i]).abs();
            let rel_error = error / original.abs();
            assert!(
                rel_error < 0.01,
                "Value {} -> {} (rel error {})",
                original,
                recovered.data()[i],
                rel_error
            );
        }
    }

    #[test]
    fn test_precision_type_info() {
        assert_eq!(PrecisionType::F32.bit_width(), 32);
        assert_eq!(PrecisionType::F16.bit_width(), 16);
        assert_eq!(PrecisionType::BF16.bit_width(), 16);

        assert_eq!(PrecisionType::F32.exponent_bits(), 8);
        assert_eq!(PrecisionType::F16.exponent_bits(), 5);
        assert_eq!(PrecisionType::BF16.exponent_bits(), 8);

        assert_eq!(PrecisionType::F32.mantissa_bits(), 23);
        assert_eq!(PrecisionType::F16.mantissa_bits(), 10);
        assert_eq!(PrecisionType::BF16.mantissa_bits(), 7);
    }

    #[test]
    fn test_memory_savings() {
        let size = 10000;
        let data: Vec<f32> = (0..size).map(|i| i as f32).collect();
        let tensor = Tensor::vector(data);

        let f32_mp = MixedPrecisionTensor::F32(tensor.clone());
        let f16_mp = MixedPrecisionTensor::to_f16(&tensor);
        let bf16_mp = MixedPrecisionTensor::to_bf16(&tensor);

        assert_eq!(f32_mp.size_bytes(), size * 4);
        assert_eq!(f16_mp.size_bytes(), size * 2);
        assert_eq!(bf16_mp.size_bytes(), size * 2);
    }

    #[test]
    fn test_f16_vs_bf16_precision() {
        // Small values: FP16 should be more accurate
        let small = 0.001;
        let f16 = F16::from_f32(small).to_f32();
        let bf16 = BF16::from_f32(small).to_f32();

        let f16_error = (small - f16).abs();
        let bf16_error = (small - bf16).abs();

        // FP16 has more mantissa bits, better for small values
        assert!(f16_error < bf16_error || (f16_error - bf16_error).abs() < 1e-6);

        // Large values: both should work well
        let large = 1000.0;
        let f16_large = F16::from_f32(large).to_f32();
        let bf16_large = BF16::from_f32(large).to_f32();

        assert!((large - f16_large).abs() < 1.0);
        assert!((large - bf16_large).abs() < 1.0);
    }
}
