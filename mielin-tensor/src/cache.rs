//! Cache-Efficient Memory Layout and Operations
//!
//! Provides cache-aware algorithms and memory layouts for improved performance.
//! Implements cache blocking (tiling), alignment, and prefetching strategies.
//!
//! # Examples
//!
//! ```
//! use mielin_tensor::cache::{CacheConfig, blocked_matmul};
//! use mielin_tensor::Tensor;
//!
//! let config = CacheConfig::default();
//! let a = Tensor::from_vec(vec![1.0; 64], vec![8, 8]).unwrap();
//! let b = Tensor::from_vec(vec![2.0; 64], vec![8, 8]).unwrap();
//! let c = blocked_matmul(&a, &b, &config).unwrap();
//! ```

#![allow(dead_code)]

extern crate alloc;

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::vec;
use core::mem;

/// Cache line size in bytes (typical for most modern CPUs)
pub const CACHE_LINE_SIZE: usize = 64;

/// L1 cache size (typical: 32-64 KB per core)
pub const L1_CACHE_SIZE: usize = 32 * 1024;

/// L2 cache size (typical: 256-512 KB per core)
pub const L2_CACHE_SIZE: usize = 256 * 1024;

/// L3 cache size (typical: 2-32 MB shared)
pub const L3_CACHE_SIZE: usize = 8 * 1024 * 1024;

/// Cache configuration for blocked algorithms
#[derive(Debug, Clone, Copy)]
pub struct CacheConfig {
    /// Block size for L1 cache (elements)
    pub l1_block_size: usize,
    /// Block size for L2 cache (elements)
    pub l2_block_size: usize,
    /// Block size for L3 cache (elements)
    pub l3_block_size: usize,
    /// Whether to use cache-aware algorithms
    pub use_blocking: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            // For f32 (4 bytes), L1 block = 32KB / 4 = 8K elements
            // Square block: sqrt(8K) ≈ 90, round to 64 for alignment
            l1_block_size: 64,
            // L2 block: 256KB / 4 = 64K elements, sqrt(64K) = 256
            l2_block_size: 256,
            // L3 block: 8MB / 4 = 2M elements, sqrt(2M) ≈ 1448, round to 1024
            l3_block_size: 1024,
            use_blocking: true,
        }
    }
}

impl CacheConfig {
    /// Create a custom cache configuration
    pub fn new(l1_block: usize, l2_block: usize, l3_block: usize) -> Self {
        Self {
            l1_block_size: l1_block,
            l2_block_size: l2_block,
            l3_block_size: l3_block,
            use_blocking: true,
        }
    }

    /// Disable cache blocking (use naive algorithms)
    pub fn no_blocking() -> Self {
        Self {
            l1_block_size: 0,
            l2_block_size: 0,
            l3_block_size: 0,
            use_blocking: false,
        }
    }
}

/// Check if a pointer is aligned to cache line boundary
#[inline]
pub fn is_cache_aligned<T>(ptr: *const T) -> bool {
    (ptr as usize).is_multiple_of(CACHE_LINE_SIZE)
}

/// Get the alignment of a pointer
#[inline]
pub fn get_alignment<T>(ptr: *const T) -> usize {
    let addr = ptr as usize;
    if addr == 0 {
        return mem::size_of::<T>();
    }
    addr & (!addr + 1)
}

/// Cache-blocked matrix multiplication (single-level blocking)
///
/// Divides matrices into blocks that fit in L1 cache for better locality.
/// Computes C = A * B using blocked algorithm.
///
/// # Arguments
///
/// * `a` - Left matrix (M x K)
/// * `b` - Right matrix (K x N)
/// * `config` - Cache configuration
///
/// # Errors
///
/// Returns error if matrix dimensions are incompatible.
pub fn blocked_matmul(
    a: &Tensor<f32>,
    b: &Tensor<f32>,
    config: &CacheConfig,
) -> TensorResult<Tensor<f32>> {
    if a.shape().len() != 2 || b.shape().len() != 2 {
        return Err(TensorError::dimension_mismatch(
            "blocked_matmul",
            2,
            a.shape().len(),
        ));
    }

    let m = a.shape()[0];
    let k_a = a.shape()[1];
    let k_b = b.shape()[0];
    let n = b.shape()[1];

    if k_a != k_b {
        return Err(TensorError::shape_mismatch(
            "blocked_matmul",
            vec![m, k_a],
            vec![k_b, n],
        ));
    }
    let k = k_a;

    let mut result = vec![0.0f32; m * n];

    if !config.use_blocking {
        // Naive algorithm
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for p in 0..k {
                    sum += a
                        .get(&[i, p])
                        .copied()
                        .expect("i < m and p < k within bounds")
                        * b.get(&[p, j])
                            .copied()
                            .expect("p < k and j < n within bounds");
                }
                result[i * n + j] = sum;
            }
        }
    } else {
        // Blocked algorithm for cache efficiency
        let block_size = config.l1_block_size;

        for i_block in (0..m).step_by(block_size) {
            for j_block in (0..n).step_by(block_size) {
                for k_block in (0..k).step_by(block_size) {
                    let i_end = (i_block + block_size).min(m);
                    let j_end = (j_block + block_size).min(n);
                    let k_end = (k_block + block_size).min(k);

                    // Process block
                    for i in i_block..i_end {
                        for j in j_block..j_end {
                            let mut sum = result[i * n + j];
                            for p in k_block..k_end {
                                sum += a
                                    .get(&[i, p])
                                    .copied()
                                    .expect("i < m and p < k within block bounds")
                                    * b.get(&[p, j])
                                        .copied()
                                        .expect("p < k and j < n within block bounds");
                            }
                            result[i * n + j] = sum;
                        }
                    }
                }
            }
        }
    }

    Tensor::from_vec(result, vec![m, n]).ok_or_else(|| TensorError::Other {
        message: alloc::string::String::from("Failed to create result tensor"),
    })
}

/// Cache-optimized transpose
///
/// Uses cache blocking to minimize cache misses during transpose.
pub fn blocked_transpose(tensor: &Tensor<f32>, config: &CacheConfig) -> TensorResult<Tensor<f32>> {
    if tensor.shape().len() != 2 {
        return Err(TensorError::dimension_mismatch(
            "blocked_transpose",
            2,
            tensor.shape().len(),
        ));
    }

    let rows = tensor.shape()[0];
    let cols = tensor.shape()[1];
    let mut result = vec![0.0f32; rows * cols];

    if !config.use_blocking {
        // Naive transpose
        for i in 0..rows {
            for j in 0..cols {
                result[j * rows + i] = *tensor
                    .get(&[i, j])
                    .expect("i < rows and j < cols within bounds");
            }
        }
    } else {
        // Blocked transpose
        let block_size = config.l1_block_size;

        for i_block in (0..rows).step_by(block_size) {
            for j_block in (0..cols).step_by(block_size) {
                let i_end = (i_block + block_size).min(rows);
                let j_end = (j_block + block_size).min(cols);

                for i in i_block..i_end {
                    for j in j_block..j_end {
                        result[j * rows + i] =
                            *tensor.get(&[i, j]).expect("i,j within block bounds");
                    }
                }
            }
        }
    }

    Tensor::from_vec(result, vec![cols, rows]).ok_or_else(|| TensorError::Other {
        message: alloc::string::String::from("Failed to create result tensor"),
    })
}

/// Compute cache efficiency metrics for a given access pattern
#[derive(Debug, Clone, Copy)]
pub struct CacheMetrics {
    /// Estimated L1 cache hit rate (0.0 - 1.0)
    pub l1_hit_rate: f32,
    /// Estimated L2 cache hit rate (0.0 - 1.0)
    pub l2_hit_rate: f32,
    /// Estimated L3 cache hit rate (0.0 - 1.0)
    pub l3_hit_rate: f32,
    /// Total memory traffic (bytes)
    pub memory_traffic: usize,
}

impl CacheMetrics {
    /// Estimate cache metrics for matrix multiplication
    ///
    /// This is a simplified model based on matrix dimensions.
    pub fn estimate_matmul(m: usize, n: usize, k: usize, block_size: usize) -> Self {
        let elem_size = mem::size_of::<f32>();
        let working_set = (m * k + k * n + m * n) * elem_size;

        // Simple heuristic: if working set fits in cache, high hit rate
        let l1_hit_rate = if working_set <= L1_CACHE_SIZE {
            0.95
        } else if block_size * block_size * elem_size <= L1_CACHE_SIZE {
            0.80 // Blocks fit in L1
        } else {
            0.30
        };

        let l2_hit_rate = if working_set <= L2_CACHE_SIZE {
            0.90
        } else if block_size * block_size * elem_size <= L2_CACHE_SIZE {
            0.75
        } else {
            0.50
        };

        let l3_hit_rate = if working_set <= L3_CACHE_SIZE {
            0.85
        } else {
            0.60
        };

        // Estimate memory traffic based on cache misses
        let total_accesses = m * n * k * 2; // 2 loads per multiply-add
        let l1_misses = (total_accesses as f32 * (1.0 - l1_hit_rate)) as usize;
        let memory_traffic = l1_misses * elem_size;

        Self {
            l1_hit_rate,
            l2_hit_rate,
            l3_hit_rate,
            memory_traffic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.l1_block_size, 64);
        assert_eq!(config.l2_block_size, 256);
        assert_eq!(config.l3_block_size, 1024);
        assert!(config.use_blocking);
    }

    #[test]
    fn test_cache_config_custom() {
        let config = CacheConfig::new(32, 128, 512);
        assert_eq!(config.l1_block_size, 32);
        assert_eq!(config.l2_block_size, 128);
        assert_eq!(config.l3_block_size, 512);
    }

    #[test]
    fn test_cache_config_no_blocking() {
        let config = CacheConfig::no_blocking();
        assert!(!config.use_blocking);
    }

    #[test]
    fn test_cache_line_size() {
        assert_eq!(CACHE_LINE_SIZE, 64);
    }

    #[test]
    fn test_blocked_matmul_small() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();

        let config = CacheConfig::default();
        let c = blocked_matmul(&a, &b, &config).unwrap();

        // [1 2] * [5 6] = [1*5+2*7  1*6+2*8] = [19 22]
        // [3 4]   [7 8]   [3*5+4*7  3*6+4*8]   [43 50]
        assert_eq!(*c.get(&[0, 0]).unwrap(), 19.0);
        assert_eq!(*c.get(&[0, 1]).unwrap(), 22.0);
        assert_eq!(*c.get(&[1, 0]).unwrap(), 43.0);
        assert_eq!(*c.get(&[1, 1]).unwrap(), 50.0);
    }

    #[test]
    fn test_blocked_matmul_no_blocking() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();

        let config = CacheConfig::no_blocking();
        let c = blocked_matmul(&a, &b, &config).unwrap();

        assert_eq!(*c.get(&[0, 0]).unwrap(), 19.0);
        assert_eq!(*c.get(&[0, 1]).unwrap(), 22.0);
        assert_eq!(*c.get(&[1, 0]).unwrap(), 43.0);
        assert_eq!(*c.get(&[1, 1]).unwrap(), 50.0);
    }

    #[test]
    fn test_blocked_transpose() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], vec![2, 3]).unwrap();

        let config = CacheConfig::default();
        let b = blocked_transpose(&a, &config).unwrap();

        assert_eq!(b.shape(), &[3, 2]);
        assert_eq!(*b.get(&[0, 0]).unwrap(), 1.0);
        assert_eq!(*b.get(&[0, 1]).unwrap(), 4.0);
        assert_eq!(*b.get(&[1, 0]).unwrap(), 2.0);
        assert_eq!(*b.get(&[1, 1]).unwrap(), 5.0);
        assert_eq!(*b.get(&[2, 0]).unwrap(), 3.0);
        assert_eq!(*b.get(&[2, 1]).unwrap(), 6.0);
    }

    #[test]
    fn test_blocked_transpose_square() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();

        let config = CacheConfig::default();
        let b = blocked_transpose(&a, &config).unwrap();

        assert_eq!(*b.get(&[0, 0]).unwrap(), 1.0);
        assert_eq!(*b.get(&[0, 1]).unwrap(), 3.0);
        assert_eq!(*b.get(&[1, 0]).unwrap(), 2.0);
        assert_eq!(*b.get(&[1, 1]).unwrap(), 4.0);
    }

    #[test]
    fn test_cache_metrics_small_matrix() {
        let metrics = CacheMetrics::estimate_matmul(10, 10, 10, 64);
        // Small matrix should have high hit rates
        assert!(metrics.l1_hit_rate > 0.9);
        assert!(metrics.l2_hit_rate > 0.8);
    }

    #[test]
    fn test_cache_metrics_large_matrix() {
        let metrics = CacheMetrics::estimate_matmul(1000, 1000, 1000, 64);
        // Large matrix should have lower hit rates
        assert!(metrics.l1_hit_rate < 0.9);
        assert!(metrics.memory_traffic > 0);
    }

    #[test]
    fn test_is_cache_aligned() {
        let data = vec![1.0f32; 100];
        let ptr = data.as_ptr();
        // Alignment depends on allocator, so we just check it doesn't panic
        let _ = is_cache_aligned(ptr);
    }

    #[test]
    fn test_get_alignment() {
        let data = vec![1.0f32; 100];
        let ptr = data.as_ptr();
        let alignment = get_alignment(ptr);
        // Alignment should be at least 4 for f32
        assert!(alignment >= 4);
        // Should be power of 2
        assert_eq!(alignment & (alignment - 1), 0);
    }
}
