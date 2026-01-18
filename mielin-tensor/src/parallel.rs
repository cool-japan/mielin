//! Parallel Tensor Operations
//!
//! Multi-threaded execution for large tensor operations using rayon.
//! Provides automatic parallelization with configurable thread pool.

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Global configuration for parallel execution
pub struct ParallelConfig {
    /// Minimum elements before using parallel execution
    pub threshold: usize,
    /// Number of threads to use (0 = automatic)
    pub num_threads: usize,
    /// Chunk size for parallel iteration
    pub chunk_size: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            threshold: 10_000,
            num_threads: 0, // Auto-detect
            chunk_size: 1024,
        }
    }
}

static PARALLEL_THRESHOLD: AtomicUsize = AtomicUsize::new(10_000);

/// Set the minimum number of elements before using parallel execution
pub fn set_parallel_threshold(threshold: usize) {
    PARALLEL_THRESHOLD.store(threshold, Ordering::Relaxed);
}

/// Get the current parallel threshold
pub fn get_parallel_threshold() -> usize {
    PARALLEL_THRESHOLD.load(Ordering::Relaxed)
}

/// Check if parallelization should be used based on size
pub fn should_parallelize(size: usize) -> bool {
    size >= get_parallel_threshold()
}

/// Thread pool manager for tensor operations
pub struct ThreadPool {
    config: ParallelConfig,
}

impl ThreadPool {
    /// Create a new thread pool with default configuration
    pub fn new() -> Self {
        Self {
            config: ParallelConfig::default(),
        }
    }

    /// Create a thread pool with custom configuration
    pub fn with_config(config: ParallelConfig) -> Self {
        Self { config }
    }

    /// Get the configuration
    pub fn config(&self) -> &ParallelConfig {
        &self.config
    }

    /// Set the number of threads
    pub fn set_num_threads(&mut self, num_threads: usize) {
        self.config.num_threads = num_threads;
    }

    /// Set the parallel threshold
    pub fn set_threshold(&mut self, threshold: usize) {
        self.config.threshold = threshold;
    }

    /// Set the chunk size
    pub fn set_chunk_size(&mut self, chunk_size: usize) {
        self.config.chunk_size = chunk_size;
    }
}

impl Default for ThreadPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel tensor operations
pub trait ParallelOps {
    /// Element-wise addition with parallel execution
    fn parallel_add(&self, other: &Self) -> TensorResult<Self>
    where
        Self: Sized;

    /// Element-wise multiplication with parallel execution
    fn parallel_mul(&self, other: &Self) -> TensorResult<Self>
    where
        Self: Sized;

    /// Matrix multiplication with parallel execution
    fn parallel_matmul(&self, other: &Self) -> TensorResult<Self>
    where
        Self: Sized;

    /// Map function over elements in parallel
    fn parallel_map<F>(&self, f: F) -> Self
    where
        Self: Sized,
        F: Fn(f32) -> f32 + Sync + Send;

    /// Reduce operation in parallel
    fn parallel_reduce<F>(&self, init: f32, f: F) -> f32
    where
        F: Fn(f32, f32) -> f32 + Sync + Send;
}

impl ParallelOps for Tensor<f32> {
    fn parallel_add(&self, other: &Self) -> TensorResult<Self> {
        if self.shape() != other.shape() {
            return Err(TensorError::other("Shape mismatch for parallel addition"));
        }

        let data = self.data();
        let other_data = other.data();
        let n = data.len();

        if !should_parallelize(n) {
            // Fall back to sequential
            return Ok(self.add(other));
        }

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            let result: Vec<f32> = data
                .par_iter()
                .zip(other_data.par_iter())
                .map(|(a, b)| a + b)
                .collect();
            Tensor::from_vec(result, self.shape().to_vec())
                .ok_or_else(|| TensorError::other("Invalid shape for tensor"))
        }

        #[cfg(not(feature = "rayon"))]
        {
            // Fallback: use chunks with std::thread
            let chunk_size = (n + num_cpus::get() - 1) / num_cpus::get();
            let mut result = alloc::vec![0.0; n];

            use alloc::vec;
            let handles: Vec<_> = result
                .chunks_mut(chunk_size)
                .enumerate()
                .map(|(i, chunk)| {
                    let start = i * chunk_size;
                    let end = (start + chunk.len()).min(n);
                    let data_slice = &data[start..end];
                    let other_slice = &other_data[start..end];

                    std::thread::spawn(move || {
                        let mut local_result = vec![0.0; end - start];
                        for j in 0..(end - start) {
                            local_result[j] = data_slice[j] + other_slice[j];
                        }
                        local_result
                    })
                })
                .collect();

            let mut offset = 0;
            for handle in handles {
                let local_result = handle
                    .join()
                    .map_err(|_| TensorError::other("Thread join failed"))?;
                result[offset..offset + local_result.len()].copy_from_slice(&local_result);
                offset += local_result.len();
            }

            Tensor::from_vec(result, self.shape().to_vec())
                .ok_or_else(|| TensorError::other("Invalid shape for tensor"))
        }
    }

    fn parallel_mul(&self, other: &Self) -> TensorResult<Self> {
        if self.shape() != other.shape() {
            return Err(TensorError::other(
                "Shape mismatch for parallel multiplication",
            ));
        }

        let data = self.data();
        let other_data = other.data();
        let n = data.len();

        if !should_parallelize(n) {
            return Ok(self.mul(other));
        }

        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;
            let result: Vec<f32> = data
                .par_iter()
                .zip(other_data.par_iter())
                .map(|(a, b)| a * b)
                .collect();
            Tensor::from_vec(result, self.shape().to_vec())
                .ok_or_else(|| TensorError::other("Invalid shape for tensor"))
        }

        #[cfg(not(feature = "rayon"))]
        {
            self.mul(other)
        }
    }

    fn parallel_matmul(&self, other: &Self) -> TensorResult<Self> {
        let m = self.shape()[0];
        let n = self.shape()[1];
        let p = other.shape()[1];

        if self.shape().len() != 2 || other.shape().len() != 2 {
            return Err(TensorError::other(
                "Matrix multiplication requires 2D tensors",
            ));
        }

        if n != other.shape()[0] {
            return Err(TensorError::other(
                "Inner dimensions must match for matrix multiplication",
            ));
        }

        // Helper function for sequential matrix multiplication
        let sequential_matmul = |self_data: &[f32], other_data: &[f32]| -> Vec<f32> {
            let mut result = alloc::vec![0.0; m * p];
            for i in 0..m {
                for j in 0..p {
                    let mut sum = 0.0;
                    for k in 0..n {
                        sum += self_data[i * n + k] * other_data[k * p + j];
                    }
                    result[i * p + j] = sum;
                }
            }
            result
        };

        #[cfg(feature = "rayon")]
        {
            if !should_parallelize(m * p) {
                // Sequential fallback for small matrices
                let result = sequential_matmul(self.data(), other.data());
                return Tensor::from_vec(result, alloc::vec![m, p])
                    .ok_or_else(|| TensorError::other("Invalid shape for tensor"));
            }

            use rayon::prelude::*;
            let mut result = alloc::vec![0.0; m * p];

            result.par_chunks_mut(p).enumerate().for_each(|(i, row)| {
                for (j, cell) in row.iter_mut().enumerate() {
                    let mut sum = 0.0;
                    for k in 0..n {
                        sum += self.data()[i * n + k] * other.data()[k * p + j];
                    }
                    *cell = sum;
                }
            });

            Tensor::from_vec(result, alloc::vec![m, p])
                .ok_or_else(|| TensorError::other("Invalid shape for tensor"))
        }

        #[cfg(not(feature = "rayon"))]
        {
            let result = sequential_matmul(self.data(), other.data());
            Tensor::from_vec(result, alloc::vec![m, p])
                .ok_or_else(|| TensorError::other("Invalid shape for tensor"))
        }
    }

    fn parallel_map<F>(&self, f: F) -> Self
    where
        F: Fn(f32) -> f32 + Sync + Send,
    {
        let data = self.data();
        let n = data.len();

        #[cfg(feature = "rayon")]
        {
            if should_parallelize(n) {
                use rayon::prelude::*;
                let result: Vec<f32> = data.par_iter().map(|&x| f(x)).collect();
                return Tensor::from_vec(result, self.shape().to_vec())
                    .expect("Shape mismatch in parallel_map");
            }
        }

        // Sequential fallback
        let result: Vec<f32> = data.iter().map(|&x| f(x)).collect();
        Tensor::from_vec(result, self.shape().to_vec()).expect("Shape mismatch in parallel_map")
    }

    fn parallel_reduce<F>(&self, init: f32, f: F) -> f32
    where
        F: Fn(f32, f32) -> f32 + Sync + Send,
    {
        let data = self.data();
        let n = data.len();

        #[cfg(feature = "rayon")]
        {
            if should_parallelize(n) {
                use rayon::prelude::*;
                return data
                    .par_iter()
                    .fold(|| init, |acc, &x| f(acc, x))
                    .reduce(|| init, &f);
            }
        }

        // Sequential fallback
        data.iter().fold(init, |acc, &x| f(acc, x))
    }
}

/// Parallel sum reduction
pub fn parallel_sum(tensor: &Tensor<f32>) -> f32 {
    tensor.parallel_reduce(0.0, |a, b| a + b)
}

/// Parallel mean calculation
pub fn parallel_mean(tensor: &Tensor<f32>) -> f32 {
    let sum = parallel_sum(tensor);
    let n = tensor.data().len() as f32;
    sum / n
}

/// Parallel variance calculation
pub fn parallel_variance(tensor: &Tensor<f32>) -> f32 {
    let mean = parallel_mean(tensor);
    let squared_diff = tensor.parallel_map(|x| (x - mean) * (x - mean));
    parallel_mean(&squared_diff)
}

/// Parallel standard deviation
pub fn parallel_std(tensor: &Tensor<f32>) -> f32 {
    libm::sqrtf(parallel_variance(tensor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::Tensor;
    use alloc::vec;

    #[test]
    fn test_parallel_config() {
        let config = ParallelConfig::default();
        assert_eq!(config.threshold, 10_000);
        assert_eq!(config.num_threads, 0);
    }

    #[test]
    fn test_parallel_threshold() {
        set_parallel_threshold(5000);
        assert_eq!(get_parallel_threshold(), 5000);
        assert!(should_parallelize(10000));
        assert!(!should_parallelize(1000));
        set_parallel_threshold(10_000); // Reset
    }

    #[test]
    fn test_thread_pool() {
        let mut pool = ThreadPool::new();
        assert_eq!(pool.config().threshold, 10_000);

        pool.set_num_threads(4);
        assert_eq!(pool.config().num_threads, 4);

        pool.set_threshold(5000);
        assert_eq!(pool.config().threshold, 5000);
    }

    #[cfg(feature = "rayon")]
    #[test]
    fn test_parallel_add() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();

        // Force small size to test sequential path
        let result = a.parallel_add(&b).unwrap();
        assert_eq!(result.data(), &[6.0, 8.0, 10.0, 12.0]);
    }

    #[cfg(feature = "rayon")]
    #[test]
    fn test_parallel_mul() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![2.0, 3.0, 4.0, 5.0], vec![2, 2]).unwrap();

        let result = a.parallel_mul(&b).unwrap();
        assert_eq!(result.data(), &[2.0, 6.0, 12.0, 20.0]);
    }

    #[cfg(feature = "rayon")]
    #[test]
    fn test_parallel_matmul() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();

        let result = a.parallel_matmul(&b).unwrap();
        // [1,2] * [5,6] = [1*5+2*7, 1*6+2*8] = [19, 22]
        // [3,4]   [7,8]   [3*5+4*7, 3*6+4*8]   [43, 50]
        assert_eq!(result.data(), &[19.0, 22.0, 43.0, 50.0]);
    }

    #[test]
    fn test_parallel_map() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let result = a.parallel_map(|x| x * 2.0);
        assert_eq!(result.data(), &[2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_parallel_reduce() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let sum = a.parallel_reduce(0.0, |acc, x| acc + x);
        assert_eq!(sum, 10.0);
    }

    #[test]
    fn test_parallel_sum() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        assert_eq!(parallel_sum(&a), 10.0);
    }

    #[test]
    fn test_parallel_mean() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        assert_eq!(parallel_mean(&a), 2.5);
    }

    #[test]
    fn test_parallel_variance() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let var = parallel_variance(&a);
        assert!((var - 1.25).abs() < 1e-5);
    }

    #[test]
    fn test_parallel_std() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let std = parallel_std(&a);
        assert!((std - libm::sqrtf(1.25)).abs() < 1e-5);
    }

    #[test]
    fn test_dimension_mismatch() {
        let a = Tensor::from_vec(vec![1.0, 2.0], vec![2]).unwrap();
        let b = Tensor::from_vec(vec![1.0, 2.0, 3.0], vec![3]).unwrap();

        assert!(a.parallel_add(&b).is_err());
        assert!(a.parallel_mul(&b).is_err());
    }
}
