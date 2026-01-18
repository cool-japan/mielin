//! Sparse Tensor Implementation
//!
//! Provides memory-efficient storage and operations for tensors with many zero elements.
//! Implements Coordinate (COO) and Compressed Sparse Row (CSR) formats.
//!
//! # Examples
//!
//! ```
//! use mielin_tensor::sparse::{SparseTensor, SparseFormat};
//! use mielin_tensor::Tensor;
//!
//! // Create a sparse tensor from coordinates
//! let rows = vec![0, 0, 1, 2];
//! let cols = vec![0, 2, 1, 2];
//! let values = vec![1.0, 2.0, 3.0, 4.0];
//! let sparse = SparseTensor::from_coo(rows, cols, values, 3, 3).unwrap();
//!
//! // Convert to dense
//! let dense = sparse.to_dense();
//! ```

#![allow(dead_code)]

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

/// Sparse matrix storage format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseFormat {
    /// Coordinate (COO) format: stores (row, col, value) triples
    COO,
    /// Compressed Sparse Row (CSR) format: row pointers + column indices + values
    CSR,
}

/// Sparse tensor using Coordinate (COO) format
///
/// Stores only non-zero elements as (row, col, value) triples.
/// Memory efficient for very sparse matrices.
#[derive(Debug, Clone)]
pub struct SparseTensor {
    /// Row indices
    rows: Vec<usize>,
    /// Column indices
    cols: Vec<usize>,
    /// Non-zero values
    values: Vec<f32>,
    /// Number of rows
    nrows: usize,
    /// Number of columns
    ncols: usize,
    /// Storage format
    format: SparseFormat,
}

impl SparseTensor {
    /// Create a sparse tensor from COO format
    ///
    /// # Arguments
    ///
    /// * `rows` - Row indices for each non-zero element
    /// * `cols` - Column indices for each non-zero element
    /// * `values` - Values of non-zero elements
    /// * `nrows` - Total number of rows
    /// * `ncols` - Total number of columns
    ///
    /// # Errors
    ///
    /// Returns an error if indices are out of bounds or input lengths don't match.
    pub fn from_coo(
        rows: Vec<usize>,
        cols: Vec<usize>,
        values: Vec<f32>,
        nrows: usize,
        ncols: usize,
    ) -> TensorResult<Self> {
        if rows.len() != cols.len() || rows.len() != values.len() {
            return Err(TensorError::Other {
                message: alloc::format!(
                    "COO format requires equal length arrays: rows={}, cols={}, values={}",
                    rows.len(),
                    cols.len(),
                    values.len()
                ),
            });
        }

        // Validate indices
        for &r in &rows {
            if r >= nrows {
                return Err(TensorError::IndexOutOfBounds {
                    indices: vec![r],
                    shape: vec![nrows, ncols],
                });
            }
        }
        for &c in &cols {
            if c >= ncols {
                return Err(TensorError::IndexOutOfBounds {
                    indices: vec![c],
                    shape: vec![nrows, ncols],
                });
            }
        }

        Ok(Self {
            rows,
            cols,
            values,
            nrows,
            ncols,
            format: SparseFormat::COO,
        })
    }

    /// Create an empty sparse tensor
    pub fn zeros(nrows: usize, ncols: usize) -> Self {
        Self {
            rows: Vec::new(),
            cols: Vec::new(),
            values: Vec::new(),
            nrows,
            ncols,
            format: SparseFormat::COO,
        }
    }

    /// Create a sparse identity matrix
    pub fn eye(n: usize) -> Self {
        let rows: Vec<usize> = (0..n).collect();
        let cols: Vec<usize> = (0..n).collect();
        let values = vec![1.0; n];
        Self {
            rows,
            cols,
            values,
            nrows: n,
            ncols: n,
            format: SparseFormat::COO,
        }
    }

    /// Get the number of rows
    #[inline]
    pub fn nrows(&self) -> usize {
        self.nrows
    }

    /// Get the number of columns
    #[inline]
    pub fn ncols(&self) -> usize {
        self.ncols
    }

    /// Get the number of non-zero elements
    #[inline]
    pub fn nnz(&self) -> usize {
        self.values.len()
    }

    /// Get the sparsity (fraction of zeros)
    #[inline]
    pub fn sparsity(&self) -> f32 {
        let total = self.nrows * self.ncols;
        if total == 0 {
            return 0.0;
        }
        1.0 - (self.nnz() as f32 / total as f32)
    }

    /// Get the storage format
    #[inline]
    pub fn format(&self) -> SparseFormat {
        self.format
    }

    /// Convert to dense tensor
    pub fn to_dense(&self) -> Tensor<f32> {
        let mut data = vec![0.0; self.nrows * self.ncols];

        match self.format {
            SparseFormat::COO => {
                for i in 0..self.nnz() {
                    let row = self.rows[i];
                    let col = self.cols[i];
                    let value = self.values[i];
                    data[row * self.ncols + col] = value;
                }
            }
            SparseFormat::CSR => {
                // CSR format not yet implemented, fall back to COO interpretation
                for i in 0..self.nnz() {
                    let row = self.rows[i];
                    let col = self.cols[i];
                    let value = self.values[i];
                    data[row * self.ncols + col] = value;
                }
            }
        }

        Tensor::from_vec(data, vec![self.nrows, self.ncols]).unwrap()
    }

    /// Create sparse tensor from dense tensor
    ///
    /// Only stores elements with absolute value > threshold
    pub fn from_dense(tensor: &Tensor<f32>, threshold: f32) -> TensorResult<Self> {
        if tensor.shape().len() != 2 {
            return Err(TensorError::dimension_mismatch(
                "from_dense",
                2,
                tensor.shape().len(),
            ));
        }

        let nrows = tensor.shape()[0];
        let ncols = tensor.shape()[1];

        let mut rows = Vec::new();
        let mut cols = Vec::new();
        let mut values = Vec::new();

        for i in 0..nrows {
            for j in 0..ncols {
                let val = *tensor.get(&[i, j]).unwrap();
                if val.abs() > threshold {
                    rows.push(i);
                    cols.push(j);
                    values.push(val);
                }
            }
        }

        Ok(Self {
            rows,
            cols,
            values,
            nrows,
            ncols,
            format: SparseFormat::COO,
        })
    }

    /// Get element at (row, col)
    pub fn get(&self, row: usize, col: usize) -> TensorResult<f32> {
        if row >= self.nrows || col >= self.ncols {
            return Err(TensorError::IndexOutOfBounds {
                indices: vec![row, col],
                shape: vec![self.nrows, self.ncols],
            });
        }

        // Linear search in COO format (slow, but simple)
        for i in 0..self.nnz() {
            if self.rows[i] == row && self.cols[i] == col {
                return Ok(self.values[i]);
            }
        }

        Ok(0.0)
    }

    /// Element-wise addition with another sparse tensor
    pub fn add(&self, other: &Self) -> TensorResult<Self> {
        if self.nrows != other.nrows || self.ncols != other.ncols {
            return Err(TensorError::shape_mismatch(
                "sparse_add",
                vec![self.nrows, self.ncols],
                vec![other.nrows, other.ncols],
            ));
        }

        // Merge non-zero elements
        let mut map = alloc::collections::BTreeMap::new();

        for i in 0..self.nnz() {
            let key = (self.rows[i], self.cols[i]);
            map.insert(key, self.values[i]);
        }

        for i in 0..other.nnz() {
            let key = (other.rows[i], other.cols[i]);
            *map.entry(key).or_insert(0.0) += other.values[i];
        }

        // Convert back to COO format
        let mut rows = Vec::new();
        let mut cols = Vec::new();
        let mut values = Vec::new();

        for ((r, c), v) in map {
            if v.abs() > 1e-10 {
                // Skip near-zero values
                rows.push(r);
                cols.push(c);
                values.push(v);
            }
        }

        Ok(Self {
            rows,
            cols,
            values,
            nrows: self.nrows,
            ncols: self.ncols,
            format: SparseFormat::COO,
        })
    }

    /// Element-wise multiplication with another sparse tensor
    ///
    /// Only non-zero elements in both tensors contribute to the result.
    pub fn mul(&self, other: &Self) -> TensorResult<Self> {
        if self.nrows != other.nrows || self.ncols != other.ncols {
            return Err(TensorError::shape_mismatch(
                "sparse_mul",
                vec![self.nrows, self.ncols],
                vec![other.nrows, other.ncols],
            ));
        }

        // Create a map for quick lookup
        let mut other_map = alloc::collections::BTreeMap::new();
        for i in 0..other.nnz() {
            let key = (other.rows[i], other.cols[i]);
            other_map.insert(key, other.values[i]);
        }

        let mut rows = Vec::new();
        let mut cols = Vec::new();
        let mut values = Vec::new();

        // Only elements present in both tensors contribute
        for i in 0..self.nnz() {
            let key = (self.rows[i], self.cols[i]);
            if let Some(&other_val) = other_map.get(&key) {
                let product = self.values[i] * other_val;
                if product.abs() > 1e-10 {
                    rows.push(self.rows[i]);
                    cols.push(self.cols[i]);
                    values.push(product);
                }
            }
        }

        Ok(Self {
            rows,
            cols,
            values,
            nrows: self.nrows,
            ncols: self.ncols,
            format: SparseFormat::COO,
        })
    }

    /// Scalar multiplication
    pub fn scale(&self, scalar: f32) -> Self {
        let values: Vec<f32> = self.values.iter().map(|&v| v * scalar).collect();
        Self {
            rows: self.rows.clone(),
            cols: self.cols.clone(),
            values,
            nrows: self.nrows,
            ncols: self.ncols,
            format: self.format,
        }
    }

    /// Transpose the sparse matrix
    pub fn transpose(&self) -> Self {
        Self {
            rows: self.cols.clone(),
            cols: self.rows.clone(),
            values: self.values.clone(),
            nrows: self.ncols,
            ncols: self.nrows,
            format: self.format,
        }
    }

    /// Sparse matrix-vector multiplication
    ///
    /// Computes y = A * x where A is this sparse matrix and x is a dense vector.
    pub fn matvec(&self, x: &[f32]) -> TensorResult<Vec<f32>> {
        if x.len() != self.ncols {
            return Err(TensorError::dimension_mismatch(
                "sparse_matvec",
                self.ncols,
                x.len(),
            ));
        }

        let mut y = vec![0.0; self.nrows];

        for i in 0..self.nnz() {
            let row = self.rows[i];
            let col = self.cols[i];
            let value = self.values[i];
            y[row] += value * x[col];
        }

        Ok(y)
    }

    /// Sparse matrix-matrix multiplication (SpMM)
    ///
    /// Computes C = A * B where A is this sparse matrix and B is dense.
    pub fn matmul_dense(&self, b: &Tensor<f32>) -> TensorResult<Tensor<f32>> {
        if b.shape().len() != 2 {
            return Err(TensorError::dimension_mismatch(
                "sparse_matmul",
                2,
                b.shape().len(),
            ));
        }

        if self.ncols != b.shape()[0] {
            return Err(TensorError::shape_mismatch(
                "sparse_matmul",
                vec![self.nrows, self.ncols],
                b.shape().to_vec(),
            ));
        }

        let k = b.shape()[1];
        let mut result = vec![0.0; self.nrows * k];

        for i in 0..self.nnz() {
            let row = self.rows[i];
            let col = self.cols[i];
            let a_val = self.values[i];

            for j in 0..k {
                let b_val = *b.get(&[col, j]).unwrap();
                result[row * k + j] += a_val * b_val;
            }
        }

        Tensor::from_vec(result, vec![self.nrows, k]).ok_or_else(|| TensorError::Other {
            message: alloc::string::String::from("Failed to create result tensor"),
        })
    }
}

impl fmt::Display for SparseTensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SparseTensor({} x {}, nnz={}, sparsity={:.2}%, format={:?})",
            self.nrows,
            self.ncols,
            self.nnz(),
            self.sparsity() * 100.0,
            self.format
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_sparse_from_coo() {
        let rows = vec![0, 0, 1, 2];
        let cols = vec![0, 2, 1, 2];
        let values = vec![1.0, 2.0, 3.0, 4.0];
        let sparse = SparseTensor::from_coo(rows, cols, values, 3, 3).unwrap();

        assert_eq!(sparse.nrows(), 3);
        assert_eq!(sparse.ncols(), 3);
        assert_eq!(sparse.nnz(), 4);
    }

    #[test]
    fn test_sparse_zeros() {
        let sparse = SparseTensor::zeros(5, 5);
        assert_eq!(sparse.nrows(), 5);
        assert_eq!(sparse.ncols(), 5);
        assert_eq!(sparse.nnz(), 0);
        assert_eq!(sparse.sparsity(), 1.0);
    }

    #[test]
    fn test_sparse_eye() {
        let sparse = SparseTensor::eye(4);
        assert_eq!(sparse.nrows(), 4);
        assert_eq!(sparse.ncols(), 4);
        assert_eq!(sparse.nnz(), 4);

        for i in 0..4 {
            assert_eq!(sparse.get(i, i).unwrap(), 1.0);
        }
    }

    #[test]
    fn test_sparse_sparsity() {
        let rows = vec![0];
        let cols = vec![0];
        let values = vec![1.0];
        let sparse = SparseTensor::from_coo(rows, cols, values, 10, 10).unwrap();

        // 1 non-zero out of 100 elements = 99% sparse
        assert!((sparse.sparsity() - 0.99).abs() < 1e-6);
    }

    #[test]
    fn test_sparse_to_dense() {
        let rows = vec![0, 0, 1];
        let cols = vec![0, 1, 1];
        let values = vec![1.0, 2.0, 3.0];
        let sparse = SparseTensor::from_coo(rows, cols, values, 2, 2).unwrap();

        let dense = sparse.to_dense();
        assert_eq!(*dense.get(&[0, 0]).unwrap(), 1.0);
        assert_eq!(*dense.get(&[0, 1]).unwrap(), 2.0);
        assert_eq!(*dense.get(&[1, 0]).unwrap(), 0.0);
        assert_eq!(*dense.get(&[1, 1]).unwrap(), 3.0);
    }

    #[test]
    fn test_sparse_from_dense() {
        let dense = Tensor::from_vec(vec![1.0, 0.0, 0.0, 2.0], vec![2, 2]).unwrap();
        let sparse = SparseTensor::from_dense(&dense, 0.1).unwrap();

        assert_eq!(sparse.nnz(), 2);
        assert_eq!(sparse.get(0, 0).unwrap(), 1.0);
        assert_eq!(sparse.get(1, 1).unwrap(), 2.0);
    }

    #[test]
    fn test_sparse_get() {
        let rows = vec![0, 1];
        let cols = vec![0, 1];
        let values = vec![5.0, 6.0];
        let sparse = SparseTensor::from_coo(rows, cols, values, 3, 3).unwrap();

        assert_eq!(sparse.get(0, 0).unwrap(), 5.0);
        assert_eq!(sparse.get(1, 1).unwrap(), 6.0);
        assert_eq!(sparse.get(0, 1).unwrap(), 0.0);
        assert_eq!(sparse.get(2, 2).unwrap(), 0.0);
    }

    #[test]
    fn test_sparse_add() {
        let s1 = SparseTensor::from_coo(vec![0, 1], vec![0, 1], vec![1.0, 2.0], 2, 2).unwrap();
        let s2 = SparseTensor::from_coo(vec![0, 1], vec![1, 1], vec![3.0, 4.0], 2, 2).unwrap();

        let result = s1.add(&s2).unwrap();

        assert_eq!(result.get(0, 0).unwrap(), 1.0);
        assert_eq!(result.get(0, 1).unwrap(), 3.0);
        assert_eq!(result.get(1, 1).unwrap(), 6.0);
    }

    #[test]
    fn test_sparse_mul() {
        let s1 = SparseTensor::from_coo(vec![0, 1], vec![0, 1], vec![2.0, 3.0], 2, 2).unwrap();
        let s2 = SparseTensor::from_coo(vec![0, 1], vec![0, 1], vec![4.0, 5.0], 2, 2).unwrap();

        let result = s1.mul(&s2).unwrap();

        assert_eq!(result.get(0, 0).unwrap(), 8.0);
        assert_eq!(result.get(1, 1).unwrap(), 15.0);
        assert_eq!(result.get(0, 1).unwrap(), 0.0);
    }

    #[test]
    fn test_sparse_scale() {
        let sparse = SparseTensor::from_coo(vec![0, 1], vec![0, 1], vec![2.0, 3.0], 2, 2).unwrap();
        let scaled = sparse.scale(2.0);

        assert_eq!(scaled.get(0, 0).unwrap(), 4.0);
        assert_eq!(scaled.get(1, 1).unwrap(), 6.0);
    }

    #[test]
    fn test_sparse_transpose() {
        let sparse = SparseTensor::from_coo(vec![0, 1], vec![0, 1], vec![1.0, 2.0], 2, 3).unwrap();
        let transposed = sparse.transpose();

        assert_eq!(transposed.nrows(), 3);
        assert_eq!(transposed.ncols(), 2);
        assert_eq!(transposed.get(0, 0).unwrap(), 1.0);
        assert_eq!(transposed.get(1, 1).unwrap(), 2.0);
    }

    #[test]
    fn test_sparse_matvec() {
        let sparse =
            SparseTensor::from_coo(vec![0, 0, 1], vec![0, 1, 1], vec![1.0, 2.0, 3.0], 2, 2)
                .unwrap();
        let x = vec![4.0, 5.0];
        let y = sparse.matvec(&x).unwrap();

        assert_eq!(y[0], 14.0); // 1*4 + 2*5 = 14
        assert_eq!(y[1], 15.0); // 3*5 = 15
    }

    #[test]
    fn test_sparse_matmul_dense() {
        let sparse = SparseTensor::from_coo(vec![0, 1], vec![0, 1], vec![2.0, 3.0], 2, 2).unwrap();
        let dense = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();

        let result = sparse.matmul_dense(&dense).unwrap();

        assert_eq!(*result.get(&[0, 0]).unwrap(), 2.0); // 2*1
        assert_eq!(*result.get(&[0, 1]).unwrap(), 4.0); // 2*2
        assert_eq!(*result.get(&[1, 0]).unwrap(), 9.0); // 3*3
        assert_eq!(*result.get(&[1, 1]).unwrap(), 12.0); // 3*4
    }

    #[test]
    fn test_sparse_display() {
        let sparse = SparseTensor::from_coo(vec![0], vec![0], vec![1.0], 10, 10).unwrap();
        let display = alloc::format!("{}", sparse);
        assert!(display.contains("10 x 10"));
        assert!(display.contains("nnz=1"));
    }
}
