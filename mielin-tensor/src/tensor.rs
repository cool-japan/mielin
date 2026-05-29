//! Tensor data structure and basic operations

extern crate alloc;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub struct Tensor<T> {
    data: Vec<T>,
    shape: Vec<usize>,
    strides: Vec<usize>,
}

impl<T: Clone + Default> Tensor<T> {
    /// Create a new tensor with the given shape
    pub fn new(shape: Vec<usize>) -> Self {
        let size = shape.iter().product();
        let strides = Self::compute_strides(&shape);

        Self {
            data: alloc::vec![T::default(); size],
            shape,
            strides,
        }
    }

    /// Create a tensor from raw data and shape
    pub fn from_vec(data: Vec<T>, shape: Vec<usize>) -> Option<Self> {
        let expected_size: usize = shape.iter().product();
        if data.len() != expected_size {
            return None;
        }

        let strides = Self::compute_strides(&shape);

        Some(Self {
            data,
            shape,
            strides,
        })
    }

    fn compute_strides(shape: &[usize]) -> Vec<usize> {
        let mut strides = Vec::with_capacity(shape.len());
        let mut stride = 1;

        for &dim in shape.iter().rev() {
            strides.push(stride);
            stride *= dim;
        }

        strides.reverse();
        strides
    }

    /// Get the shape of the tensor
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get the total number of elements
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Get the number of dimensions
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Get a reference to the underlying data
    pub fn data(&self) -> &[T] {
        &self.data
    }

    /// Get a mutable reference to the underlying data
    pub fn data_mut(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Get an element at the given multi-dimensional index
    pub fn get(&self, indices: &[usize]) -> Option<&T> {
        if indices.len() != self.ndim() {
            return None;
        }

        let flat_index = self.compute_flat_index(indices)?;
        self.data.get(flat_index)
    }

    /// Set an element at the given multi-dimensional index
    pub fn set(&mut self, indices: &[usize], value: T) -> Option<()> {
        if indices.len() != self.ndim() {
            return None;
        }

        let flat_index = self.compute_flat_index(indices)?;
        self.data.get_mut(flat_index).map(|v| *v = value)
    }

    fn compute_flat_index(&self, indices: &[usize]) -> Option<usize> {
        if indices.len() != self.shape.len() {
            return None;
        }

        let mut flat_index = 0;
        for ((&idx, &dim), &stride) in indices
            .iter()
            .zip(self.shape.iter())
            .zip(self.strides.iter())
        {
            if idx >= dim {
                return None;
            }
            flat_index += idx * stride;
        }

        Some(flat_index)
    }

    /// Reshape the tensor (must preserve total size)
    pub fn reshape(&mut self, new_shape: Vec<usize>) -> Option<()> {
        let new_size: usize = new_shape.iter().product();
        if new_size != self.size() {
            return None;
        }

        self.shape = new_shape;
        self.strides = Self::compute_strides(&self.shape);
        Some(())
    }
}

impl Tensor<f32> {
    /// Create a 1D tensor (vector)
    pub fn vector(data: Vec<f32>) -> Self {
        let len = data.len();
        Self::from_vec(data, alloc::vec![len]).expect("data length matches 1D shape")
    }

    /// Create a 2D tensor (matrix)
    pub fn matrix(data: Vec<f32>, rows: usize, cols: usize) -> Option<Self> {
        Self::from_vec(data, alloc::vec![rows, cols])
    }

    /// Create a 3D tensor (e.g., for images with channels, batches of matrices)
    /// Shape: [depth, rows, cols]
    pub fn tensor3d(data: Vec<f32>, depth: usize, rows: usize, cols: usize) -> Option<Self> {
        Self::from_vec(data, alloc::vec![depth, rows, cols])
    }

    /// Create a 4D tensor (e.g., for batches of images, video frames)
    /// Shape: [batch, depth, rows, cols]
    pub fn tensor4d(
        data: Vec<f32>,
        batch: usize,
        depth: usize,
        rows: usize,
        cols: usize,
    ) -> Option<Self> {
        Self::from_vec(data, alloc::vec![batch, depth, rows, cols])
    }

    /// Create a 5D tensor (e.g., for batches of video sequences)
    /// Shape: [batch, time, depth, rows, cols]
    pub fn tensor5d(
        data: Vec<f32>,
        batch: usize,
        time: usize,
        depth: usize,
        rows: usize,
        cols: usize,
    ) -> Option<Self> {
        Self::from_vec(data, alloc::vec![batch, time, depth, rows, cols])
    }

    /// Create a 6D tensor (e.g., for complex multi-dimensional data)
    /// Shape: [dim1, dim2, dim3, dim4, dim5, dim6]
    pub fn tensor6d(
        data: Vec<f32>,
        dim1: usize,
        dim2: usize,
        dim3: usize,
        dim4: usize,
        dim5: usize,
        dim6: usize,
    ) -> Option<Self> {
        Self::from_vec(data, alloc::vec![dim1, dim2, dim3, dim4, dim5, dim6])
    }

    /// Create a scalar tensor (0D tensor with single value)
    pub fn scalar(value: f32) -> Self {
        Self::from_vec(alloc::vec![value], alloc::vec![1])
            .expect("single element matches shape [1]")
    }

    /// Create a tensor filled with zeros
    pub fn zeros(shape: Vec<usize>) -> Self {
        let size = shape.iter().product();
        let strides = Self::compute_strides(&shape);

        Self {
            data: alloc::vec![0.0; size],
            shape,
            strides,
        }
    }

    /// Create a tensor filled with ones
    pub fn ones(shape: Vec<usize>) -> Self {
        let size = shape.iter().product();
        let strides = Self::compute_strides(&shape);

        Self {
            data: alloc::vec![1.0; size],
            shape,
            strides,
        }
    }

    /// Create a tensor filled with a specific value
    pub fn filled(shape: Vec<usize>, value: f32) -> Self {
        let size = shape.iter().product();
        let strides = Self::compute_strides(&shape);

        Self {
            data: alloc::vec![value; size],
            shape,
            strides,
        }
    }

    /// Element-wise addition
    pub fn add(&self, other: &Self) -> Self {
        assert_eq!(self.shape, other.shape, "Shape mismatch for addition");
        let mut result = self.clone();
        for (i, &val) in other.data.iter().enumerate() {
            result.data[i] += val;
        }
        result
    }

    /// Element-wise subtraction
    pub fn sub(&self, other: &Self) -> Self {
        assert_eq!(self.shape, other.shape, "Shape mismatch for subtraction");
        let mut result = self.clone();
        for (i, &val) in other.data.iter().enumerate() {
            result.data[i] -= val;
        }
        result
    }

    /// Element-wise multiplication
    pub fn mul(&self, other: &Self) -> Self {
        assert_eq!(self.shape, other.shape, "Shape mismatch for multiplication");
        let mut result = self.clone();
        for (i, &val) in other.data.iter().enumerate() {
            result.data[i] *= val;
        }
        result
    }

    /// Scale all elements by a constant
    pub fn scale(&self, factor: f32) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val *= factor;
        }
        result
    }

    /// Calculate the sum of all elements
    pub fn sum(&self) -> f32 {
        self.data.iter().sum()
    }

    /// Calculate the mean (average) of all elements
    pub fn mean(&self) -> f32 {
        if self.size() == 0 {
            return 0.0;
        }
        self.sum() / (self.size() as f32)
    }

    /// Calculate the variance of all elements
    pub fn variance(&self) -> f32 {
        if self.size() == 0 {
            return 0.0;
        }

        let mean = self.mean();
        let sum_squared_diff: f32 = self
            .data
            .iter()
            .map(|&x| {
                let diff = x - mean;
                diff * diff
            })
            .sum();

        sum_squared_diff / (self.size() as f32)
    }

    /// Calculate the standard deviation of all elements
    pub fn std(&self) -> f32 {
        libm::sqrtf(self.variance())
    }

    /// Find the minimum value
    pub fn min(&self) -> f32 {
        self.data.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// Find the maximum value
    pub fn max(&self) -> f32 {
        self.data.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }

    /// Calculate element-wise absolute value
    pub fn abs(&self) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val = val.abs();
        }
        result
    }

    /// Calculate element-wise square root
    pub fn sqrt(&self) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val = libm::sqrtf(*val);
        }
        result
    }

    /// Calculate element-wise exponential
    pub fn exp(&self) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val = libm::expf(*val);
        }
        result
    }

    /// Calculate element-wise natural logarithm
    pub fn log(&self) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val = libm::logf(*val);
        }
        result
    }

    /// Calculate element-wise power
    pub fn pow(&self, exponent: f32) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val = libm::powf(*val, exponent);
        }
        result
    }

    /// Clip values to a specified range
    pub fn clip(&self, min_val: f32, max_val: f32) -> Self {
        let mut result = self.clone();
        for val in result.data.iter_mut() {
            *val = val.max(min_val).min(max_val);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_tensor_creation() {
        let tensor: Tensor<f32> = Tensor::new(std::vec![2, 3]);
        assert_eq!(tensor.shape(), &[2, 3]);
        assert_eq!(tensor.size(), 6);
        assert_eq!(tensor.ndim(), 2);
    }

    #[test]
    fn test_tensor_from_vec() {
        let data = std::vec![1.0, 2.0, 3.0, 4.0];
        let tensor = Tensor::from_vec(data.clone(), std::vec![2, 2]).unwrap();
        assert_eq!(tensor.data(), &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(tensor.shape(), &[2, 2]);
    }

    #[test]
    fn test_tensor_get_set() {
        let mut tensor: Tensor<f32> = Tensor::zeros(std::vec![2, 2]);
        tensor.set(&[0, 0], 1.0);
        tensor.set(&[1, 1], 2.0);

        assert_eq!(tensor.get(&[0, 0]), Some(&1.0));
        assert_eq!(tensor.get(&[1, 1]), Some(&2.0));
        assert_eq!(tensor.get(&[0, 1]), Some(&0.0));
    }

    #[test]
    fn test_tensor_reshape() {
        let mut tensor: Tensor<f32> = Tensor::zeros(std::vec![2, 3]);
        assert!(tensor.reshape(std::vec![3, 2]).is_some());
        assert_eq!(tensor.shape(), &[3, 2]);

        // Invalid reshape
        assert!(tensor.reshape(std::vec![2, 2]).is_none());
    }

    #[test]
    fn test_vector_creation() {
        let v = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        assert_eq!(v.shape(), &[3]);
        assert_eq!(v.size(), 3);
    }

    #[test]
    fn test_matrix_creation() {
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        assert_eq!(m.shape(), &[2, 2]);
        assert_eq!(m.get(&[0, 0]), Some(&1.0));
        assert_eq!(m.get(&[1, 1]), Some(&4.0));
    }

    #[test]
    fn test_zeros_ones() {
        let zeros = Tensor::zeros(std::vec![2, 2]);
        assert_eq!(zeros.data(), &[0.0, 0.0, 0.0, 0.0]);

        let ones = Tensor::ones(std::vec![2, 2]);
        assert_eq!(ones.data(), &[1.0, 1.0, 1.0, 1.0]);
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn test_single_element_tensor() {
        let tensor = Tensor::vector(std::vec![42.0]);
        assert_eq!(tensor.shape(), &[1]);
        assert_eq!(tensor.size(), 1);
        assert_eq!(tensor.get(&[0]), Some(&42.0));
    }

    #[test]
    fn test_scalar_operations() {
        let a = Tensor::scalar(5.0);
        let b = Tensor::scalar(3.0);

        let sum = a.add(&b);
        assert_eq!(sum.data()[0], 8.0);

        let diff = a.sub(&b);
        assert_eq!(diff.data()[0], 2.0);

        let prod = a.mul(&b);
        assert_eq!(prod.data()[0], 15.0);
    }

    #[test]
    fn test_out_of_bounds_access() {
        let tensor = Tensor::zeros(std::vec![3, 3]);

        // Valid access
        assert!(tensor.get(&[0, 0]).is_some());
        assert!(tensor.get(&[2, 2]).is_some());

        // Out of bounds
        assert!(tensor.get(&[3, 0]).is_none());
        assert!(tensor.get(&[0, 3]).is_none());
        assert!(tensor.get(&[3, 3]).is_none());
    }

    #[test]
    fn test_wrong_dimension_access() {
        let tensor = Tensor::zeros(std::vec![2, 3]);

        // Wrong number of dimensions
        assert!(tensor.get(&[0]).is_none());
        assert!(tensor.get(&[0, 0, 0]).is_none());
    }

    #[test]
    fn test_negative_values() {
        let tensor = Tensor::vector(std::vec![-1.0, -2.0, -3.0]);
        assert_eq!(tensor.data(), &[-1.0, -2.0, -3.0]);

        let scaled = tensor.scale(-1.0);
        assert_eq!(scaled.data(), &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_zero_multiplication() {
        let a = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let b = Tensor::vector(std::vec![0.0, 0.0, 0.0]);

        let result = a.mul(&b);
        assert_eq!(result.data(), &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_large_tensor() {
        // Create a relatively large tensor
        let size = 10000;
        let tensor = Tensor::zeros(std::vec![size]);

        assert_eq!(tensor.size(), size);
        assert_eq!(tensor.shape(), &[size]);
    }

    #[test]
    fn test_multidimensional_shapes() {
        // 3D tensor
        let tensor_3d = Tensor::<f32>::zeros(std::vec![2, 3, 4]);
        assert_eq!(tensor_3d.ndim(), 3);
        assert_eq!(tensor_3d.size(), 24);

        // 4D tensor
        let tensor_4d = Tensor::<f32>::zeros(std::vec![2, 3, 4, 5]);
        assert_eq!(tensor_4d.ndim(), 4);
        assert_eq!(tensor_4d.size(), 120);
    }

    #[test]
    fn test_reshape_preserves_data() {
        let mut tensor = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

        tensor.reshape(std::vec![2, 3]).unwrap();
        assert_eq!(tensor.data(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(tensor.get(&[0, 0]), Some(&1.0));
        assert_eq!(tensor.get(&[1, 2]), Some(&6.0));
    }

    #[test]
    fn test_reshape_invalid_sizes() {
        let mut tensor = Tensor::zeros(std::vec![2, 3]);

        // Invalid: different total size
        assert!(tensor.reshape(std::vec![2, 2]).is_none());
        assert!(tensor.reshape(std::vec![7]).is_none());

        // Valid: same total size
        assert!(tensor.reshape(std::vec![3, 2]).is_some());
        assert!(tensor.reshape(std::vec![6]).is_some());
    }

    #[test]
    fn test_filled_tensor() {
        let value = core::f32::consts::PI;
        let tensor = Tensor::filled(std::vec![3, 3], value);

        assert_eq!(tensor.size(), 9);
        for &v in tensor.data() {
            assert_eq!(v, value);
        }
    }

    #[test]
    fn test_element_wise_with_different_values() {
        let a = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let b = Tensor::vector(std::vec![10.0, 20.0, 30.0]);

        let sum = a.add(&b);
        assert_eq!(sum.data(), &[11.0, 22.0, 33.0]);

        let diff = b.sub(&a);
        assert_eq!(diff.data(), &[9.0, 18.0, 27.0]);

        let prod = a.mul(&b);
        assert_eq!(prod.data(), &[10.0, 40.0, 90.0]);
    }

    #[test]
    fn test_scale_by_zero() {
        let tensor = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let scaled = tensor.scale(0.0);

        assert_eq!(scaled.data(), &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_scale_by_negative() {
        let tensor = Tensor::vector(std::vec![1.0, -2.0, 3.0]);
        let scaled = tensor.scale(-2.0);

        assert_eq!(scaled.data(), &[-2.0, 4.0, -6.0]);
    }

    #[test]
    #[should_panic(expected = "Shape mismatch")]
    fn test_add_shape_mismatch() {
        let a = Tensor::zeros(std::vec![2, 3]);
        let b = Tensor::zeros(std::vec![3, 2]);
        let _ = a.add(&b); // Should panic
    }

    #[test]
    #[should_panic(expected = "Shape mismatch")]
    fn test_mul_shape_mismatch() {
        let a = Tensor::vector(std::vec![1.0, 2.0]);
        let b = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let _ = a.mul(&b); // Should panic
    }

    #[test]
    fn test_matrix_index_computation() {
        let tensor = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();

        // Row-major order verification
        assert_eq!(tensor.get(&[0, 0]), Some(&1.0));
        assert_eq!(tensor.get(&[0, 1]), Some(&2.0));
        assert_eq!(tensor.get(&[0, 2]), Some(&3.0));
        assert_eq!(tensor.get(&[1, 0]), Some(&4.0));
        assert_eq!(tensor.get(&[1, 1]), Some(&5.0));
        assert_eq!(tensor.get(&[1, 2]), Some(&6.0));
    }

    #[test]
    fn test_from_vec_size_mismatch() {
        // Data length doesn't match shape
        let data = std::vec![1.0, 2.0, 3.0];
        let result = Tensor::from_vec(data, std::vec![2, 2]);

        assert!(result.is_none());
    }

    #[test]
    fn test_matrix_creation_invalid() {
        // Data length doesn't match rows * cols
        let data = std::vec![1.0, 2.0, 3.0];
        let result = Tensor::matrix(data, 2, 2);

        assert!(result.is_none());
    }

    // =========================================================================
    // Higher-Dimensional Tensor Tests (3D+)
    // =========================================================================

    #[test]
    fn test_tensor3d_creation() {
        // Create a 3D tensor: 2x3x4 = 24 elements
        let data: Vec<f32> = (0..24).map(|i| i as f32).collect();
        let tensor = Tensor::tensor3d(data, 2, 3, 4).unwrap();

        assert_eq!(tensor.ndim(), 3);
        assert_eq!(tensor.shape(), &[2, 3, 4]);
        assert_eq!(tensor.size(), 24);

        // Test element access
        assert_eq!(tensor.get(&[0, 0, 0]), Some(&0.0));
        assert_eq!(tensor.get(&[0, 0, 3]), Some(&3.0));
        assert_eq!(tensor.get(&[1, 2, 3]), Some(&23.0));
    }

    #[test]
    fn test_tensor4d_creation() {
        // Create a 4D tensor: 2x2x3x2 = 24 elements
        let data: Vec<f32> = (1..=24).map(|i| i as f32).collect();
        let tensor = Tensor::tensor4d(data, 2, 2, 3, 2).unwrap();

        assert_eq!(tensor.ndim(), 4);
        assert_eq!(tensor.shape(), &[2, 2, 3, 2]);
        assert_eq!(tensor.size(), 24);

        // Test element access
        assert_eq!(tensor.get(&[0, 0, 0, 0]), Some(&1.0));
        assert_eq!(tensor.get(&[1, 1, 2, 1]), Some(&24.0));
    }

    #[test]
    fn test_tensor5d_creation() {
        // Create a 5D tensor: 2x2x2x2x2 = 32 elements
        let data: Vec<f32> = (0..32).map(|i| i as f32).collect();
        let tensor = Tensor::tensor5d(data, 2, 2, 2, 2, 2).unwrap();

        assert_eq!(tensor.ndim(), 5);
        assert_eq!(tensor.shape(), &[2, 2, 2, 2, 2]);
        assert_eq!(tensor.size(), 32);

        // Test element access
        assert_eq!(tensor.get(&[0, 0, 0, 0, 0]), Some(&0.0));
        assert_eq!(tensor.get(&[1, 1, 1, 1, 1]), Some(&31.0));
    }

    #[test]
    fn test_tensor6d_creation() {
        // Create a 6D tensor: 2x2x2x2x2x2 = 64 elements
        let data: Vec<f32> = (0..64).map(|i| i as f32).collect();
        let tensor = Tensor::tensor6d(data, 2, 2, 2, 2, 2, 2).unwrap();

        assert_eq!(tensor.ndim(), 6);
        assert_eq!(tensor.shape(), &[2, 2, 2, 2, 2, 2]);
        assert_eq!(tensor.size(), 64);

        // Test element access
        assert_eq!(tensor.get(&[0, 0, 0, 0, 0, 0]), Some(&0.0));
        assert_eq!(tensor.get(&[1, 1, 1, 1, 1, 1]), Some(&63.0));
    }

    #[test]
    fn test_tensor3d_invalid_size() {
        // Data length doesn't match dimensions
        let data = std::vec![1.0; 20];
        let result = Tensor::tensor3d(data, 2, 3, 4); // Expects 24 elements

        assert!(result.is_none());
    }

    #[test]
    fn test_tensor4d_invalid_size() {
        let data = std::vec![1.0; 20];
        let result = Tensor::tensor4d(data, 2, 2, 3, 2); // Expects 24 elements

        assert!(result.is_none());
    }

    #[test]
    fn test_tensor5d_operations() {
        // Test element-wise operations on 5D tensors
        let a = Tensor::tensor5d(std::vec![1.0; 32], 2, 2, 2, 2, 2).unwrap();
        let b = Tensor::tensor5d(std::vec![2.0; 32], 2, 2, 2, 2, 2).unwrap();

        let sum = a.add(&b);
        assert_eq!(sum.shape(), &[2, 2, 2, 2, 2]);
        assert_eq!(sum.data()[0], 3.0);

        let product = a.mul(&b);
        assert_eq!(product.data()[0], 2.0);
    }

    #[test]
    fn test_tensor6d_reshape() {
        // Create a 6D tensor and reshape it
        let mut tensor = Tensor::tensor6d(std::vec![1.0; 64], 2, 2, 2, 2, 2, 2).unwrap();

        // Reshape to different 6D configuration
        assert!(tensor.reshape(std::vec![4, 2, 2, 2, 2, 1]).is_some());
        assert_eq!(tensor.shape(), &[4, 2, 2, 2, 2, 1]);

        // Reshape to lower dimensions
        assert!(tensor.reshape(std::vec![8, 8]).is_some());
        assert_eq!(tensor.shape(), &[8, 8]);

        // Reshape to 1D
        assert!(tensor.reshape(std::vec![64]).is_some());
        assert_eq!(tensor.shape(), &[64]);
    }

    #[test]
    fn test_tensor3d_zeros_ones() {
        let zeros = Tensor::zeros(std::vec![2, 3, 4]);
        assert_eq!(zeros.ndim(), 3);
        assert_eq!(zeros.size(), 24);
        assert!(zeros.data().iter().all(|&x| x == 0.0));

        let ones = Tensor::ones(std::vec![2, 3, 4]);
        assert_eq!(ones.ndim(), 3);
        assert!(ones.data().iter().all(|&x| x == 1.0));
    }

    #[test]
    fn test_tensor7d_and_beyond() {
        // Test that generic implementation supports even higher dimensions
        let shape = std::vec![2, 2, 2, 2, 2, 2, 2]; // 7D tensor
        let tensor = Tensor::<f32>::zeros(shape.clone());

        assert_eq!(tensor.ndim(), 7);
        assert_eq!(tensor.size(), 128);
        assert_eq!(tensor.shape(), &shape[..]);

        // Test 8D
        let shape_8d = std::vec![2, 2, 2, 2, 2, 2, 2, 2];
        let tensor_8d = Tensor::<f32>::ones(shape_8d.clone());
        assert_eq!(tensor_8d.ndim(), 8);
        assert_eq!(tensor_8d.size(), 256);
    }

    #[test]
    fn test_tensor3d_get_set() {
        let mut tensor = Tensor::zeros(std::vec![3, 4, 5]);

        // Set values at different positions
        tensor.set(&[0, 0, 0], 1.0);
        tensor.set(&[1, 2, 3], 42.0);
        tensor.set(&[2, 3, 4], 99.0);

        // Verify values
        assert_eq!(tensor.get(&[0, 0, 0]), Some(&1.0));
        assert_eq!(tensor.get(&[1, 2, 3]), Some(&42.0));
        assert_eq!(tensor.get(&[2, 3, 4]), Some(&99.0));
        assert_eq!(tensor.get(&[0, 0, 1]), Some(&0.0));
    }

    #[test]
    fn test_tensor4d_batch_processing() {
        // Simulate batch of 3x3 grayscale images: [batch=2, channels=1, height=3, width=3]
        let batch_size = 2;
        let channels = 1;
        let height = 3;
        let width = 3;
        let total = batch_size * channels * height * width;

        let data: Vec<f32> = (0..total).map(|i| i as f32).collect();
        let tensor = Tensor::tensor4d(data, batch_size, channels, height, width).unwrap();

        assert_eq!(tensor.shape(), &[2, 1, 3, 3]);

        // First image, first pixel
        assert_eq!(tensor.get(&[0, 0, 0, 0]), Some(&0.0));

        // Second image, last pixel
        assert_eq!(tensor.get(&[1, 0, 2, 2]), Some(&17.0));
    }

    #[test]
    fn test_tensor5d_video_batch() {
        // Simulate batch of video sequences: [batch=2, time=3, channels=1, height=2, width=2]
        let data: Vec<f32> = (0..24).map(|i| i as f32).collect();
        let tensor = Tensor::tensor5d(data, 2, 3, 1, 2, 2).unwrap();

        assert_eq!(tensor.shape(), &[2, 3, 1, 2, 2]);

        // First batch, first frame, first pixel
        assert_eq!(tensor.get(&[0, 0, 0, 0, 0]), Some(&0.0));

        // Second batch, third frame, last pixel
        assert_eq!(tensor.get(&[1, 2, 0, 1, 1]), Some(&23.0));
    }

    #[test]
    fn test_high_dimensional_stride_computation() {
        // Test that stride computation is correct for high dimensions
        let tensor = Tensor::zeros(std::vec![2, 3, 4, 5]);

        // Manually verify stride computation
        // For shape [2, 3, 4, 5]:
        // strides should be [60, 20, 5, 1]
        // (3*4*5=60, 4*5=20, 5=5, 1=1)

        assert_eq!(tensor.get(&[0, 0, 0, 0]), Some(&0.0));
        assert_eq!(tensor.get(&[1, 0, 0, 0]), tensor.data().get(60));
        assert_eq!(tensor.get(&[0, 1, 0, 0]), tensor.data().get(20));
        assert_eq!(tensor.get(&[0, 0, 1, 0]), tensor.data().get(5));
        assert_eq!(tensor.get(&[0, 0, 0, 1]), tensor.data().get(1));
    }

    // =========================================================================
    // Reduction Operations Tests
    // =========================================================================

    #[test]
    fn test_sum() {
        let tensor = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(tensor.sum(), 15.0);
    }

    #[test]
    fn test_mean() {
        let tensor = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(tensor.mean(), 3.0);

        // Empty tensor
        let empty = Tensor::zeros(std::vec![0]);
        assert_eq!(empty.mean(), 0.0);
    }

    #[test]
    fn test_variance() {
        let tensor = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let variance = tensor.variance();
        // Variance of [1,2,3,4,5] = ((1-3)² + (2-3)² + (3-3)² + (4-3)² + (5-3)²) / 5
        //                          = (4 + 1 + 0 + 1 + 4) / 5 = 2.0
        assert!((variance - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_std() {
        let tensor = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let std = tensor.std();
        assert!((std - 2.0f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn test_min_max() {
        let tensor = Tensor::vector(std::vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]);
        assert_eq!(tensor.min(), 1.0);
        assert_eq!(tensor.max(), 9.0);
    }

    #[test]
    fn test_abs() {
        let tensor = Tensor::vector(std::vec![-2.0, -1.0, 0.0, 1.0, 2.0]);
        let result = tensor.abs();
        assert_eq!(result.data(), &[2.0, 1.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_sqrt() {
        let tensor = Tensor::vector(std::vec![1.0, 4.0, 9.0, 16.0]);
        let result = tensor.sqrt();
        assert_eq!(result.data(), &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_exp() {
        let tensor = Tensor::vector(std::vec![0.0, 1.0]);
        let result = tensor.exp();
        assert!((result.data()[0] - 1.0).abs() < 1e-6);
        assert!((result.data()[1] - std::f32::consts::E).abs() < 1e-6);
    }

    #[test]
    fn test_log() {
        let tensor = Tensor::vector(std::vec![1.0, std::f32::consts::E]);
        let result = tensor.log();
        assert!((result.data()[0] - 0.0).abs() < 1e-6);
        assert!((result.data()[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_pow() {
        let tensor = Tensor::vector(std::vec![2.0, 3.0, 4.0]);
        let result = tensor.pow(2.0);
        assert_eq!(result.data(), &[4.0, 9.0, 16.0]);
    }

    #[test]
    fn test_clip() {
        let tensor = Tensor::vector(std::vec![-5.0, 0.0, 3.0, 7.0, 10.0]);
        let result = tensor.clip(0.0, 5.0);
        assert_eq!(result.data(), &[0.0, 0.0, 3.0, 5.0, 5.0]);
    }

    #[test]
    fn test_reduction_on_multidimensional() {
        let tensor = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();

        assert_eq!(tensor.sum(), 21.0);
        assert_eq!(tensor.mean(), 3.5);
        assert_eq!(tensor.min(), 1.0);
        assert_eq!(tensor.max(), 6.0);
    }
}
