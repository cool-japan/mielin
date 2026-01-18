//! Data Utilities Module
//!
//! Provides utilities for managing datasets in machine learning workflows.
//! Includes batching, shuffling, train/test splitting, and data iteration.
//!
//! # Features
//! - Dataset batching with configurable batch size
//! - Shuffling with deterministic or random seeds
//! - Train/validation/test splitting
//! - Mini-batch iteration
//! - Data augmentation pipelines (future)
//!
//! # Examples
//! ```rust,ignore
//! use mielin_tensor::data::{Dataset, DataLoader};
//!
//! let data = vec![tensor1, tensor2, tensor3];
//! let dataset = Dataset::new(data);
//! let loader = DataLoader::new(dataset, 32, true); // batch_size=32, shuffle=true
//!
//! for batch in loader.iter() {
//!     // Train on batch
//! }
//! ```

#![allow(dead_code)]

extern crate alloc;

use alloc::vec::Vec;

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;

/// Simple pseudo-random number generator for shuffling (no_std compatible)
/// Uses Linear Congruential Generator (LCG) algorithm
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    /// Create a new RNG with a seed
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(1),
        }
    }

    /// Generate next random number
    fn next(&mut self) -> u64 {
        // LCG parameters from Numerical Recipes
        const A: u64 = 6364136223846793005;
        const C: u64 = 1442695040888963407;
        self.state = self.state.wrapping_mul(A).wrapping_add(C);
        self.state
    }

    /// Generate random usize in range [0, max)
    pub fn gen_range(&mut self, max: usize) -> usize {
        (self.next() % max as u64) as usize
    }

    /// Shuffle a vector in place using Fisher-Yates algorithm
    pub fn shuffle<T>(&mut self, vec: &mut [T]) {
        let len = vec.len();
        for i in 0..len {
            let j = i + self.gen_range(len - i);
            vec.swap(i, j);
        }
    }
}

/// Dataset wrapper for tensors
#[derive(Clone)]
pub struct Dataset<T> {
    /// Data samples
    pub data: Vec<T>,
}

impl<T> Dataset<T> {
    /// Create a new dataset from a vector of samples
    pub fn new(data: Vec<T>) -> Self {
        Self { data }
    }

    /// Get the number of samples
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if dataset is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get a reference to a sample
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Get a mutable reference to a sample
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }

    /// Shuffle the dataset in place
    pub fn shuffle(&mut self, seed: u64)
    where
        T: Clone,
    {
        let mut rng = SimpleRng::new(seed);
        rng.shuffle(&mut self.data);
    }

    /// Split dataset into train and test sets
    ///
    /// # Arguments
    /// * `train_ratio` - Ratio of data to use for training (0.0 to 1.0)
    ///
    /// # Returns
    /// (train_dataset, test_dataset)
    pub fn train_test_split(&self, train_ratio: f32) -> (Dataset<T>, Dataset<T>)
    where
        T: Clone,
    {
        assert!(
            (0.0..=1.0).contains(&train_ratio),
            "train_ratio must be between 0 and 1"
        );

        let train_size = (self.len() as f32 * train_ratio) as usize;
        let train_data = self.data[..train_size].to_vec();
        let test_data = self.data[train_size..].to_vec();

        (Dataset::new(train_data), Dataset::new(test_data))
    }

    /// Split dataset into train, validation, and test sets
    ///
    /// # Arguments
    /// * `train_ratio` - Ratio of data for training
    /// * `val_ratio` - Ratio of data for validation
    ///
    /// # Returns
    /// (train_dataset, val_dataset, test_dataset)
    pub fn train_val_test_split(
        &self,
        train_ratio: f32,
        val_ratio: f32,
    ) -> (Dataset<T>, Dataset<T>, Dataset<T>)
    where
        T: Clone,
    {
        assert!(
            train_ratio >= 0.0 && val_ratio >= 0.0,
            "Ratios must be non-negative"
        );
        assert!(
            train_ratio + val_ratio <= 1.0,
            "Sum of ratios must be <= 1.0"
        );

        let train_size = (self.len() as f32 * train_ratio) as usize;
        let val_size = (self.len() as f32 * val_ratio) as usize;

        let train_data = self.data[..train_size].to_vec();
        let val_data = self.data[train_size..train_size + val_size].to_vec();
        let test_data = self.data[train_size + val_size..].to_vec();

        (
            Dataset::new(train_data),
            Dataset::new(val_data),
            Dataset::new(test_data),
        )
    }

    /// Create k-fold cross-validation splits
    ///
    /// Returns a vector of (train, validation) dataset pairs
    pub fn k_fold(&self, k: usize) -> Vec<(Dataset<T>, Dataset<T>)>
    where
        T: Clone,
    {
        assert!(k > 1, "k must be at least 2");
        assert!(self.len() >= k, "Dataset too small for k folds");

        let fold_size = self.len() / k;
        let mut folds = Vec::new();

        for i in 0..k {
            let val_start = i * fold_size;
            let val_end = if i == k - 1 {
                self.len()
            } else {
                (i + 1) * fold_size
            };

            let mut train_data = Vec::new();
            train_data.extend_from_slice(&self.data[..val_start]);
            train_data.extend_from_slice(&self.data[val_end..]);

            let val_data = self.data[val_start..val_end].to_vec();

            folds.push((Dataset::new(train_data), Dataset::new(val_data)));
        }

        folds
    }
}

/// Data loader for batch iteration
pub struct DataLoader<T> {
    dataset: Dataset<T>,
    batch_size: usize,
    shuffle: bool,
    seed: u64,
    indices: Vec<usize>,
}

impl<T: Clone> DataLoader<T> {
    /// Create a new data loader
    ///
    /// # Arguments
    /// * `dataset` - The dataset to load from
    /// * `batch_size` - Number of samples per batch
    /// * `shuffle` - Whether to shuffle data each epoch
    pub fn new(dataset: Dataset<T>, batch_size: usize, shuffle: bool) -> Self {
        Self::with_seed(dataset, batch_size, shuffle, 0)
    }

    /// Create a new data loader with a specific seed
    pub fn with_seed(dataset: Dataset<T>, batch_size: usize, shuffle: bool, seed: u64) -> Self {
        let indices: Vec<usize> = (0..dataset.len()).collect();
        Self {
            dataset,
            batch_size,
            shuffle,
            seed,
            indices,
        }
    }

    /// Get the number of batches
    pub fn num_batches(&self) -> usize {
        self.dataset.len().div_ceil(self.batch_size)
    }

    /// Reset the loader (shuffle if needed) for a new epoch
    pub fn reset(&mut self) {
        if self.shuffle {
            let mut rng = SimpleRng::new(self.seed);
            rng.shuffle(&mut self.indices);
            // Update seed for next epoch
            self.seed = self.seed.wrapping_add(1);
        }
    }

    /// Get a batch by index
    pub fn get_batch(&self, batch_idx: usize) -> Option<Vec<T>> {
        let start = batch_idx * self.batch_size;
        if start >= self.dataset.len() {
            return None;
        }

        let end = core::cmp::min(start + self.batch_size, self.dataset.len());
        let mut batch = Vec::with_capacity(end - start);

        for i in start..end {
            let idx = self.indices[i];
            if let Some(sample) = self.dataset.get(idx) {
                batch.push(sample.clone());
            }
        }

        Some(batch)
    }

    /// Create an iterator over batches
    pub fn iter(&mut self) -> DataLoaderIter<'_, T> {
        self.reset();
        DataLoaderIter {
            loader: self,
            current_batch: 0,
        }
    }

    /// Get the batch size
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Get the dataset size
    pub fn dataset_size(&self) -> usize {
        self.dataset.len()
    }
}

/// Iterator over batches from a DataLoader
pub struct DataLoaderIter<'a, T> {
    loader: &'a DataLoader<T>,
    current_batch: usize,
}

impl<'a, T: Clone> Iterator for DataLoaderIter<'a, T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let batch = self.loader.get_batch(self.current_batch)?;
        self.current_batch += 1;
        Some(batch)
    }
}

/// Utility functions for tensor datasets
pub mod tensor_utils {
    use super::*;

    /// Stack a batch of tensors along a new dimension
    ///
    /// Converts `Vec<Tensor>` with shape \[H, W\] to Tensor with shape \[B, H, W\]
    pub fn stack_batch(tensors: &[Tensor<f32>]) -> TensorResult<Tensor<f32>> {
        if tensors.is_empty() {
            return Err(TensorError::Other {
                message: "Empty batch".into(),
            });
        }

        let first_shape = tensors[0].shape();
        let batch_size = tensors.len();

        // Verify all tensors have the same shape
        for tensor in tensors.iter().skip(1) {
            if tensor.shape() != first_shape {
                return Err(TensorError::shape_mismatch(
                    "stack_batch",
                    first_shape.to_vec(),
                    tensor.shape().to_vec(),
                ));
            }
        }

        // Create new shape [batch_size, ...original_shape]
        let mut new_shape = Vec::with_capacity(first_shape.len() + 1);
        new_shape.push(batch_size);
        new_shape.extend_from_slice(first_shape);

        // Concatenate all data
        let mut data = Vec::new();
        for tensor in tensors {
            data.extend_from_slice(tensor.data());
        }

        Tensor::from_vec(data, new_shape).ok_or_else(|| TensorError::Other {
            message: "Failed to create stacked tensor".into(),
        })
    }

    /// Normalize tensor data to zero mean and unit variance
    pub fn normalize(tensor: &Tensor<f32>) -> Tensor<f32> {
        let mean = tensor.mean();
        let std = tensor.std();

        let mut normalized = tensor.clone();
        for val in normalized.data_mut() {
            *val = (*val - mean) / (std + 1e-8);
        }

        normalized
    }

    /// Min-max normalization to range [0, 1]
    pub fn min_max_normalize(tensor: &Tensor<f32>) -> Tensor<f32> {
        let min = tensor.min();
        let max = tensor.max();
        let range = max - min;

        let mut normalized = tensor.clone();
        for val in normalized.data_mut() {
            *val = (*val - min) / (range + 1e-8);
        }

        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_simple_rng() {
        let mut rng = SimpleRng::new(42);
        let r1 = rng.next();
        let r2 = rng.next();
        assert_ne!(r1, r2);

        // Test deterministic behavior
        let mut rng2 = SimpleRng::new(42);
        assert_eq!(rng2.next(), r1);
    }

    #[test]
    fn test_rng_shuffle() {
        let mut rng = SimpleRng::new(123);
        let mut data = vec![1, 2, 3, 4, 5];
        let original = data.clone();

        rng.shuffle(&mut data);

        // Should be different order (with high probability)
        assert_ne!(data, original);

        // Should contain same elements
        let mut sorted = data.clone();
        sorted.sort();
        assert_eq!(sorted, original);
    }

    #[test]
    fn test_dataset_creation() {
        let data = vec![1, 2, 3, 4, 5];
        let dataset = Dataset::new(data.clone());

        assert_eq!(dataset.len(), 5);
        assert!(!dataset.is_empty());
        assert_eq!(dataset.get(0), Some(&1));
        assert_eq!(dataset.get(4), Some(&5));
        assert_eq!(dataset.get(5), None);
    }

    #[test]
    fn test_dataset_shuffle() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut dataset = Dataset::new(data.clone());

        dataset.shuffle(42);

        // Should be different order
        assert_ne!(dataset.data, data);

        // Should contain same elements
        let mut sorted = dataset.data.clone();
        sorted.sort();
        assert_eq!(sorted, data);
    }

    #[test]
    fn test_train_test_split() {
        let data: Vec<i32> = (0..100).collect();
        let dataset = Dataset::new(data);

        let (train, test) = dataset.train_test_split(0.8);

        assert_eq!(train.len(), 80);
        assert_eq!(test.len(), 20);
        assert_eq!(train.len() + test.len(), 100);
    }

    #[test]
    fn test_train_val_test_split() {
        let data: Vec<i32> = (0..100).collect();
        let dataset = Dataset::new(data);

        let (train, val, test) = dataset.train_val_test_split(0.7, 0.15);

        assert_eq!(train.len(), 70);
        assert_eq!(val.len(), 15);
        assert_eq!(test.len(), 15);
        assert_eq!(train.len() + val.len() + test.len(), 100);
    }

    #[test]
    fn test_k_fold() {
        let data: Vec<i32> = (0..10).collect();
        let dataset = Dataset::new(data);

        let folds = dataset.k_fold(5);

        assert_eq!(folds.len(), 5);

        for (train, val) in folds.iter() {
            assert_eq!(train.len(), 8);
            assert_eq!(val.len(), 2);
        }
    }

    #[test]
    fn test_data_loader() {
        let data: Vec<i32> = (0..10).collect();
        let dataset = Dataset::new(data);

        let loader = DataLoader::new(dataset, 3, false);

        assert_eq!(loader.num_batches(), 4); // ceil(10/3) = 4
        assert_eq!(loader.batch_size(), 3);
        assert_eq!(loader.dataset_size(), 10);

        let batch0 = loader.get_batch(0).unwrap();
        assert_eq!(batch0, vec![0, 1, 2]);

        let batch3 = loader.get_batch(3).unwrap();
        assert_eq!(batch3, vec![9]); // Last batch with 1 element
    }

    #[test]
    fn test_data_loader_iterator() {
        let data: Vec<i32> = (0..7).collect();
        let dataset = Dataset::new(data);

        let mut loader = DataLoader::new(dataset, 3, false);

        let batches: Vec<Vec<i32>> = loader.iter().collect();

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0], vec![0, 1, 2]);
        assert_eq!(batches[1], vec![3, 4, 5]);
        assert_eq!(batches[2], vec![6]);
    }

    #[test]
    fn test_data_loader_shuffle() {
        let data: Vec<i32> = (0..20).collect();
        let dataset = Dataset::new(data.clone());

        let mut loader = DataLoader::with_seed(dataset, 5, true, 42);

        let batch0_epoch1 = loader.get_batch(0).unwrap();
        loader.reset();
        let batch0_epoch2 = loader.get_batch(0).unwrap();

        // After shuffle, first batch should be different
        assert_ne!(batch0_epoch1, batch0_epoch2);
    }

    #[test]
    fn test_stack_batch() {
        use tensor_utils::*;

        let t1 = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let t2 = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();
        let t3 = Tensor::from_vec(vec![9.0, 10.0, 11.0, 12.0], vec![2, 2]).unwrap();

        let batch = stack_batch(&[t1, t2, t3]).unwrap();

        assert_eq!(batch.shape(), &[3, 2, 2]);
        assert_eq!(batch.data()[0], 1.0);
        assert_eq!(batch.data()[4], 5.0);
        assert_eq!(batch.data()[8], 9.0);
    }

    #[test]
    fn test_normalize() {
        use tensor_utils::*;

        let tensor = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let normalized = normalize(&tensor);

        // Mean should be close to 0
        let mean = normalized.mean();
        assert!(mean.abs() < 1e-6);

        // Std should be close to 1
        let std = normalized.std();
        assert!((std - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_min_max_normalize() {
        use tensor_utils::*;

        let tensor = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let normalized = min_max_normalize(&tensor);

        // Min should be 0
        assert!(normalized.min().abs() < 1e-6);

        // Max should be 1
        assert!((normalized.max() - 1.0).abs() < 1e-6);
    }
}
