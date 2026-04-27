//! Hardware-specific SIMD backends for tensor operations
//!
//! This module provides optimized implementations using:
//! - ARM NEON (AArch64): 128-bit vectors, 4 x f32
//! - ARM SVE2 (AArch64): Scalable vectors, 128-2048 bits
//! - x86_64 AVX2: 256-bit vectors, 8 x f32
//! - x86_64 AVX-512: 512-bit vectors, 16 x f32
//! - RISC-V Vector (RVV): Scalable vectors
//! - Scalar fallback: Works on all platforms
//!
//! Backends are selected at runtime based on hardware capabilities.

pub mod avx2;
pub mod avx512;
pub mod neon;
pub mod rvv;
pub mod sve2;

pub use avx2::{add_avx2, dot_avx2, matvec_avx2};
pub use avx512::{
    abs_avx512, add_avx512, dot_avx512, fma_avx512, matvec_avx512, max_avx512, min_avx512,
    mul_avx512, relu_avx512, scale_avx512, sub_avx512, sum_avx512,
};
pub use neon::{add_neon, dot_neon, matvec_neon};
pub use rvv::{
    add_rvv, dot_rvv, fma_rvv, matvec_rvv, max_rvv, min_rvv, mul_rvv, scale_rvv, sum_rvv,
};
pub use sve2::{
    add_sve2, div_sve2, dot_sve2, fma_sve2, matvec_sve2, max_sve2, min_sve2, mul_sve2, scale_sve2,
    sub_sve2, sum_sve2,
};
