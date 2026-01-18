//! Matrix operations for 2D tensors
//!
//! Provides matrix-specific operations including:
//! - Transpose
//! - Inverse (for small matrices)
//! - Determinant
//! - LU decomposition
//! - Identity matrix generation
//! - Eigenvalue decomposition (QR algorithm, power iteration)
//! - Singular Value Decomposition (SVD)
//! - QR decomposition

extern crate alloc;

use crate::tensor::Tensor;
use libm::sqrtf;

/// Error type for matrix operations
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixError {
    /// Matrix is not square
    NotSquare,
    /// Matrix is singular (not invertible)
    Singular,
    /// Dimension mismatch
    DimensionMismatch,
    /// Matrix is not 2D
    Not2D,
    /// Matrix is too large for operation
    TooLarge,
}

/// Result of eigenvalue decomposition
#[derive(Debug, Clone)]
pub struct EigenResult {
    /// Eigenvalues (may be complex, stored as real parts)
    pub eigenvalues: Tensor<f32>,
    /// Eigenvectors as columns of a matrix
    pub eigenvectors: Tensor<f32>,
    /// Number of iterations taken
    pub iterations: usize,
    /// Whether the algorithm converged
    pub converged: bool,
}

impl PartialEq for EigenResult {
    fn eq(&self, other: &Self) -> bool {
        self.eigenvalues.data() == other.eigenvalues.data()
            && self.eigenvectors.data() == other.eigenvectors.data()
            && self.iterations == other.iterations
            && self.converged == other.converged
    }
}

/// Result of Singular Value Decomposition (U, S, V)
/// where A = U * S * V^T
pub type SvdResult = (Tensor<f32>, Tensor<f32>, Tensor<f32>);

/// Matrix operations on 2D tensors
pub struct Matrix;

impl Matrix {
    /// Create an identity matrix of size n x n
    pub fn identity(n: usize) -> Tensor<f32> {
        let mut result = Tensor::zeros(alloc::vec![n, n]);
        for i in 0..n {
            result.set(&[i, i], 1.0);
        }
        result
    }

    /// Transpose a 2D matrix
    ///
    /// Converts a matrix of shape (m, n) to shape (n, m)
    pub fn transpose(matrix: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        let rows = shape[0];
        let cols = shape[1];

        let mut result = Tensor::zeros(alloc::vec![cols, rows]);

        for i in 0..rows {
            for j in 0..cols {
                let val = *matrix.get(&[i, j]).unwrap();
                result.set(&[j, i], val);
            }
        }

        Ok(result)
    }

    /// Calculate the determinant of a square matrix
    ///
    /// Uses LU decomposition for matrices larger than 3x3
    pub fn determinant(matrix: &Tensor<f32>) -> Result<f32, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];

        match n {
            0 => Ok(1.0),
            1 => Ok(*matrix.get(&[0, 0]).unwrap()),
            2 => {
                // ad - bc
                let a = *matrix.get(&[0, 0]).unwrap();
                let b = *matrix.get(&[0, 1]).unwrap();
                let c = *matrix.get(&[1, 0]).unwrap();
                let d = *matrix.get(&[1, 1]).unwrap();
                Ok(a * d - b * c)
            }
            3 => {
                // Rule of Sarrus
                let a = matrix.data();
                Ok(a[0] * a[4] * a[8] + a[1] * a[5] * a[6] + a[2] * a[3] * a[7]
                    - a[2] * a[4] * a[6]
                    - a[1] * a[3] * a[8]
                    - a[0] * a[5] * a[7])
            }
            _ => {
                // LU decomposition
                let (_, _, det_sign, det_prod) = Self::lu_decompose(matrix)?;
                Ok(det_sign * det_prod)
            }
        }
    }

    /// LU decomposition with partial pivoting
    ///
    /// Returns (L, U, determinant_sign, diagonal_product)
    /// where det(A) = determinant_sign * diagonal_product
    fn lu_decompose(
        matrix: &Tensor<f32>,
    ) -> Result<(Tensor<f32>, Tensor<f32>, f32, f32), MatrixError> {
        let n = matrix.shape()[0];

        // Copy matrix to U, L starts as identity
        let mut l = Self::identity(n);
        let mut u = matrix.clone();

        let mut det_sign = 1.0f32;

        for k in 0..n {
            // Find pivot
            let mut max_idx = k;
            let mut max_val = libm::fabsf(*u.get(&[k, k]).unwrap());

            for i in (k + 1)..n {
                let val = libm::fabsf(*u.get(&[i, k]).unwrap());
                if val > max_val {
                    max_val = val;
                    max_idx = i;
                }
            }

            // Check for singularity
            if max_val < 1e-10 {
                return Err(MatrixError::Singular);
            }

            // Swap rows if needed
            if max_idx != k {
                for j in 0..n {
                    let uk = *u.get(&[k, j]).unwrap();
                    let um = *u.get(&[max_idx, j]).unwrap();
                    u.set(&[k, j], um);
                    u.set(&[max_idx, j], uk);

                    if j < k {
                        let lk = *l.get(&[k, j]).unwrap();
                        let lm = *l.get(&[max_idx, j]).unwrap();
                        l.set(&[k, j], lm);
                        l.set(&[max_idx, j], lk);
                    }
                }
                det_sign = -det_sign;
            }

            // Elimination
            for i in (k + 1)..n {
                let factor = *u.get(&[i, k]).unwrap() / *u.get(&[k, k]).unwrap();
                l.set(&[i, k], factor);

                for j in k..n {
                    let uij = *u.get(&[i, j]).unwrap();
                    let ukj = *u.get(&[k, j]).unwrap();
                    u.set(&[i, j], uij - factor * ukj);
                }
            }
        }

        // Calculate diagonal product of U
        let mut det_prod = 1.0f32;
        for i in 0..n {
            det_prod *= *u.get(&[i, i]).unwrap();
        }

        Ok((l, u, det_sign, det_prod))
    }

    /// Calculate the inverse of a square matrix
    ///
    /// Uses Gauss-Jordan elimination for efficiency
    pub fn inverse(matrix: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];

        // Special cases for small matrices
        match n {
            0 => return Ok(Tensor::zeros(alloc::vec![0, 0])),
            1 => {
                let a = *matrix.get(&[0, 0]).unwrap();
                if libm::fabsf(a) < 1e-10 {
                    return Err(MatrixError::Singular);
                }
                let mut result = Tensor::zeros(alloc::vec![1, 1]);
                result.set(&[0, 0], 1.0 / a);
                return Ok(result);
            }
            2 => {
                return Self::inverse_2x2(matrix);
            }
            3 => {
                return Self::inverse_3x3(matrix);
            }
            _ => {}
        }

        // Gauss-Jordan elimination for larger matrices
        Self::inverse_gauss_jordan(matrix)
    }

    /// Optimized 2x2 matrix inverse
    fn inverse_2x2(matrix: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        let a = *matrix.get(&[0, 0]).unwrap();
        let b = *matrix.get(&[0, 1]).unwrap();
        let c = *matrix.get(&[1, 0]).unwrap();
        let d = *matrix.get(&[1, 1]).unwrap();

        let det = a * d - b * c;

        if libm::fabsf(det) < 1e-10 {
            return Err(MatrixError::Singular);
        }

        let inv_det = 1.0 / det;

        let mut result = Tensor::zeros(alloc::vec![2, 2]);
        result.set(&[0, 0], d * inv_det);
        result.set(&[0, 1], -b * inv_det);
        result.set(&[1, 0], -c * inv_det);
        result.set(&[1, 1], a * inv_det);

        Ok(result)
    }

    /// Optimized 3x3 matrix inverse using cofactor method
    fn inverse_3x3(matrix: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        let m = matrix.data();

        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6]);

        if libm::fabsf(det) < 1e-10 {
            return Err(MatrixError::Singular);
        }

        let inv_det = 1.0 / det;

        // Cofactor matrix (transposed)
        let mut result = Tensor::zeros(alloc::vec![3, 3]);

        result.set(&[0, 0], (m[4] * m[8] - m[5] * m[7]) * inv_det);
        result.set(&[0, 1], (m[2] * m[7] - m[1] * m[8]) * inv_det);
        result.set(&[0, 2], (m[1] * m[5] - m[2] * m[4]) * inv_det);
        result.set(&[1, 0], (m[5] * m[6] - m[3] * m[8]) * inv_det);
        result.set(&[1, 1], (m[0] * m[8] - m[2] * m[6]) * inv_det);
        result.set(&[1, 2], (m[2] * m[3] - m[0] * m[5]) * inv_det);
        result.set(&[2, 0], (m[3] * m[7] - m[4] * m[6]) * inv_det);
        result.set(&[2, 1], (m[1] * m[6] - m[0] * m[7]) * inv_det);
        result.set(&[2, 2], (m[0] * m[4] - m[1] * m[3]) * inv_det);

        Ok(result)
    }

    /// Gauss-Jordan elimination for matrix inversion
    fn inverse_gauss_jordan(matrix: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        let n = matrix.shape()[0];

        // Create augmented matrix [A | I]
        let mut aug = Tensor::zeros(alloc::vec![n, 2 * n]);

        // Copy matrix A and identity I
        for i in 0..n {
            for j in 0..n {
                aug.set(&[i, j], *matrix.get(&[i, j]).unwrap());
            }
            aug.set(&[i, n + i], 1.0);
        }

        // Forward elimination
        for k in 0..n {
            // Find pivot
            let mut max_idx = k;
            let mut max_val = libm::fabsf(*aug.get(&[k, k]).unwrap());

            for i in (k + 1)..n {
                let val = libm::fabsf(*aug.get(&[i, k]).unwrap());
                if val > max_val {
                    max_val = val;
                    max_idx = i;
                }
            }

            if max_val < 1e-10 {
                return Err(MatrixError::Singular);
            }

            // Swap rows
            if max_idx != k {
                for j in 0..(2 * n) {
                    let ak = *aug.get(&[k, j]).unwrap();
                    let am = *aug.get(&[max_idx, j]).unwrap();
                    aug.set(&[k, j], am);
                    aug.set(&[max_idx, j], ak);
                }
            }

            // Scale pivot row
            let pivot = *aug.get(&[k, k]).unwrap();
            for j in 0..(2 * n) {
                let val = *aug.get(&[k, j]).unwrap();
                aug.set(&[k, j], val / pivot);
            }

            // Eliminate column
            for i in 0..n {
                if i != k {
                    let factor = *aug.get(&[i, k]).unwrap();
                    for j in 0..(2 * n) {
                        let aij = *aug.get(&[i, j]).unwrap();
                        let akj = *aug.get(&[k, j]).unwrap();
                        aug.set(&[i, j], aij - factor * akj);
                    }
                }
            }
        }

        // Extract inverse from augmented matrix
        let mut result = Tensor::zeros(alloc::vec![n, n]);
        for i in 0..n {
            for j in 0..n {
                result.set(&[i, j], *aug.get(&[i, n + j]).unwrap());
            }
        }

        Ok(result)
    }

    /// Matrix trace (sum of diagonal elements)
    pub fn trace(matrix: &Tensor<f32>) -> Result<f32, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];
        let mut sum = 0.0f32;
        for i in 0..n {
            sum += *matrix.get(&[i, i]).unwrap();
        }
        Ok(sum)
    }

    /// Check if matrix is symmetric (A == A^T)
    pub fn is_symmetric(matrix: &Tensor<f32>, tolerance: f32) -> Result<bool, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];
        for i in 0..n {
            for j in (i + 1)..n {
                let aij = *matrix.get(&[i, j]).unwrap();
                let aji = *matrix.get(&[j, i]).unwrap();
                if libm::fabsf(aij - aji) > tolerance {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Check if matrix is diagonal
    pub fn is_diagonal(matrix: &Tensor<f32>, tolerance: f32) -> Result<bool, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let val = *matrix.get(&[i, j]).unwrap();
                    if libm::fabsf(val) > tolerance {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    /// Create a diagonal matrix from a vector
    pub fn diag(vector: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        if vector.ndim() != 1 {
            return Err(MatrixError::Not2D);
        }

        let n = vector.shape()[0];
        let mut result = Tensor::zeros(alloc::vec![n, n]);

        for i in 0..n {
            result.set(&[i, i], *vector.get(&[i]).unwrap());
        }

        Ok(result)
    }

    /// Extract diagonal from a matrix as a vector
    pub fn extract_diag(matrix: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        let n = shape[0].min(shape[1]);

        let mut result = Tensor::zeros(alloc::vec![n]);

        for i in 0..n {
            result.set(&[i], *matrix.get(&[i, i]).unwrap());
        }

        Ok(result)
    }

    // =========================================================================
    // Eigenvalue Decomposition
    // =========================================================================

    /// Find the dominant eigenvalue and eigenvector using power iteration
    ///
    /// Power iteration finds the largest eigenvalue (by absolute value) and
    /// its corresponding eigenvector. Best for large sparse matrices.
    ///
    /// # Arguments
    /// * `matrix` - Square matrix
    /// * `max_iter` - Maximum iterations (default: 1000)
    /// * `tolerance` - Convergence tolerance (default: 1e-6)
    ///
    /// # Returns
    /// Tuple of (eigenvalue, eigenvector, iterations, converged)
    pub fn power_iteration(
        matrix: &Tensor<f32>,
        max_iter: usize,
        tolerance: f32,
    ) -> Result<(f32, Tensor<f32>, usize, bool), MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];

        // Start with random-ish initial vector
        let mut v = Tensor::zeros(alloc::vec![n]);
        for i in 0..n {
            v.set(&[i], 1.0 / sqrtf(n as f32));
        }

        let mut eigenvalue = 0.0f32;
        let mut converged = false;

        for iter in 0..max_iter {
            // Multiply: w = A * v
            let mut w = Tensor::zeros(alloc::vec![n]);
            for i in 0..n {
                let mut sum = 0.0;
                for j in 0..n {
                    sum += matrix.get(&[i, j]).unwrap() * v.get(&[j]).unwrap();
                }
                w.set(&[i], sum);
            }

            // Compute norm of w
            let mut norm = 0.0;
            for i in 0..n {
                let wi = *w.get(&[i]).unwrap();
                norm += wi * wi;
            }
            norm = sqrtf(norm);

            if norm < tolerance {
                // Matrix might be zero or nearly zero
                return Ok((0.0, v, iter, false));
            }

            // Normalize: v_new = w / ||w||
            let mut v_new = Tensor::zeros(alloc::vec![n]);
            for i in 0..n {
                v_new.set(&[i], *w.get(&[i]).unwrap() / norm);
            }

            // Compute Rayleigh quotient: lambda = v^T * A * v
            let mut av = Tensor::zeros(alloc::vec![n]);
            for i in 0..n {
                let mut sum = 0.0;
                for j in 0..n {
                    sum += matrix.get(&[i, j]).unwrap() * v_new.get(&[j]).unwrap();
                }
                av.set(&[i], sum);
            }

            let mut new_eigenvalue = 0.0;
            for i in 0..n {
                new_eigenvalue += v_new.get(&[i]).unwrap() * av.get(&[i]).unwrap();
            }

            // Check convergence
            if (new_eigenvalue - eigenvalue).abs() < tolerance {
                converged = true;
                return Ok((new_eigenvalue, v_new, iter + 1, converged));
            }

            eigenvalue = new_eigenvalue;
            v = v_new;
        }

        Ok((eigenvalue, v, max_iter, converged))
    }

    /// Compute all eigenvalues using QR algorithm
    ///
    /// The QR algorithm iteratively computes the Schur decomposition to find
    /// all eigenvalues. Works best for symmetric matrices.
    ///
    /// # Arguments
    /// * `matrix` - Square matrix
    /// * `max_iter` - Maximum iterations (default: 100)
    /// * `tolerance` - Convergence tolerance (default: 1e-6)
    pub fn eigenvalues_qr(
        matrix: &Tensor<f32>,
        max_iter: usize,
        tolerance: f32,
    ) -> Result<EigenResult, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        if shape[0] != shape[1] {
            return Err(MatrixError::NotSquare);
        }

        let n = shape[0];

        // Copy matrix for iteration
        let mut a = matrix.clone();
        let mut q_total = Self::identity(n);
        let mut converged = false;
        let mut iter_count = 0;

        for iter in 0..max_iter {
            iter_count = iter + 1;

            // QR decomposition using Gram-Schmidt
            let (q, r) = Self::qr_decomposition(&a)?;

            // A_new = R * Q
            a = Self::matrix_multiply(&r, &q)?;

            // Accumulate Q for eigenvectors: Q_total = Q_total * Q
            q_total = Self::matrix_multiply(&q_total, &q)?;

            // Check convergence: off-diagonal elements should be small
            let mut off_diag_norm = 0.0;
            for i in 0..n {
                for j in 0..n {
                    if i != j {
                        let val = *a.get(&[i, j]).unwrap();
                        off_diag_norm += val * val;
                    }
                }
            }
            off_diag_norm = sqrtf(off_diag_norm);

            if off_diag_norm < tolerance {
                converged = true;
                break;
            }
        }

        // Extract eigenvalues from diagonal
        let mut eigenvalues = Tensor::zeros(alloc::vec![n]);
        for i in 0..n {
            eigenvalues.set(&[i], *a.get(&[i, i]).unwrap());
        }

        Ok(EigenResult {
            eigenvalues,
            eigenvectors: q_total,
            iterations: iter_count,
            converged,
        })
    }

    /// QR decomposition using modified Gram-Schmidt
    ///
    /// Returns (Q, R) where A = Q * R, Q is orthogonal, R is upper triangular
    pub fn qr_decomposition(
        matrix: &Tensor<f32>,
    ) -> Result<(Tensor<f32>, Tensor<f32>), MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        let m = shape[0];
        let n = shape[1];

        let mut q = Tensor::zeros(alloc::vec![m, n]);
        let mut r = Tensor::zeros(alloc::vec![n, n]);

        // Modified Gram-Schmidt process
        for j in 0..n {
            // Start with column j of A
            let mut v = Tensor::zeros(alloc::vec![m]);
            for i in 0..m {
                v.set(&[i], *matrix.get(&[i, j]).unwrap());
            }

            // Orthogonalize against previous columns
            for k in 0..j {
                // r_kj = q_k^T * v
                let mut r_kj = 0.0;
                for i in 0..m {
                    r_kj += q.get(&[i, k]).unwrap() * v.get(&[i]).unwrap();
                }
                r.set(&[k, j], r_kj);

                // v = v - r_kj * q_k
                for i in 0..m {
                    let vi = *v.get(&[i]).unwrap();
                    let qi = *q.get(&[i, k]).unwrap();
                    v.set(&[i], vi - r_kj * qi);
                }
            }

            // r_jj = ||v||
            let mut norm = 0.0;
            for i in 0..m {
                let vi = *v.get(&[i]).unwrap();
                norm += vi * vi;
            }
            norm = sqrtf(norm);

            if norm < 1e-10 {
                // Handle near-zero column
                r.set(&[j, j], 0.0);
                for i in 0..m {
                    q.set(&[i, j], 0.0);
                }
            } else {
                r.set(&[j, j], norm);

                // q_j = v / ||v||
                for i in 0..m {
                    q.set(&[i, j], *v.get(&[i]).unwrap() / norm);
                }
            }
        }

        Ok((q, r))
    }

    /// Matrix multiplication helper
    fn matrix_multiply(a: &Tensor<f32>, b: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        if a.ndim() != 2 || b.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let a_shape = a.shape();
        let b_shape = b.shape();

        if a_shape[1] != b_shape[0] {
            return Err(MatrixError::DimensionMismatch);
        }

        let m = a_shape[0];
        let k = a_shape[1];
        let n = b_shape[1];

        let mut result = Tensor::zeros(alloc::vec![m, n]);

        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for p in 0..k {
                    sum += a.get(&[i, p]).unwrap() * b.get(&[p, j]).unwrap();
                }
                result.set(&[i, j], sum);
            }
        }

        Ok(result)
    }

    /// Singular Value Decomposition (SVD)
    ///
    /// Computes A = U * S * V^T where:
    /// - U: m x m orthogonal matrix (left singular vectors)
    /// - S: m x n diagonal matrix (singular values)
    /// - V: n x n orthogonal matrix (right singular vectors)
    ///
    /// Uses eigendecomposition of A^T * A and A * A^T
    pub fn svd(
        matrix: &Tensor<f32>,
        max_iter: usize,
        tolerance: f32,
    ) -> Result<SvdResult, MatrixError> {
        if matrix.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }

        let shape = matrix.shape();
        let m = shape[0];
        let n = shape[1];

        // Compute A^T * A (n x n)
        let at = Self::transpose(matrix)?;
        let ata = Self::matrix_multiply(&at, matrix)?;

        // Eigendecomposition of A^T * A gives V and singular values squared
        let eigen_result = Self::eigenvalues_qr(&ata, max_iter, tolerance)?;

        // Singular values are square roots of eigenvalues
        let mut singular_values = Tensor::zeros(alloc::vec![n]);
        for i in 0..n {
            let ev = *eigen_result.eigenvalues.get(&[i]).unwrap();
            singular_values.set(&[i], if ev > 0.0 { sqrtf(ev) } else { 0.0 });
        }

        // V is the eigenvectors of A^T * A
        let v = eigen_result.eigenvectors;

        // Compute U = A * V * S^(-1)
        let av = Self::matrix_multiply(matrix, &v)?;

        let mut u = Tensor::zeros(alloc::vec![m, n.min(m)]);
        for j in 0..n.min(m) {
            let s = *singular_values.get(&[j]).unwrap();
            if s > tolerance {
                for i in 0..m {
                    u.set(&[i, j], *av.get(&[i, j]).unwrap() / s);
                }
            }
        }

        // Create diagonal matrix S
        let mut s_matrix = Tensor::zeros(alloc::vec![m.min(n), n.min(m)]);
        for i in 0..m.min(n) {
            s_matrix.set(&[i, i], *singular_values.get(&[i]).unwrap());
        }

        Ok((u, s_matrix, v))
    }

    /// Compute the condition number of a matrix
    ///
    /// The condition number is the ratio of the largest to smallest singular value.
    /// A high condition number indicates the matrix is nearly singular.
    pub fn condition_number(
        matrix: &Tensor<f32>,
        max_iter: usize,
        tolerance: f32,
    ) -> Result<f32, MatrixError> {
        let (_, s, _) = Self::svd(matrix, max_iter, tolerance)?;

        let n = s.shape()[0].min(s.shape()[1]);

        let mut max_sv = 0.0f32;
        let mut min_sv = f32::MAX;

        for i in 0..n {
            let sv = *s.get(&[i, i]).unwrap();
            if sv > max_sv {
                max_sv = sv;
            }
            if sv > tolerance && sv < min_sv {
                min_sv = sv;
            }
        }

        if min_sv < tolerance {
            Ok(f32::INFINITY)
        } else {
            Ok(max_sv / min_sv)
        }
    }

    /// Solve linear system Ax = b using QR decomposition
    ///
    /// More numerically stable than direct inversion for ill-conditioned matrices.
    pub fn solve_qr(a: &Tensor<f32>, b: &Tensor<f32>) -> Result<Tensor<f32>, MatrixError> {
        if a.ndim() != 2 {
            return Err(MatrixError::Not2D);
        }
        if b.ndim() != 1 {
            return Err(MatrixError::DimensionMismatch);
        }

        let a_shape = a.shape();
        if a_shape[0] != a_shape[1] {
            return Err(MatrixError::NotSquare);
        }
        if a_shape[0] != b.shape()[0] {
            return Err(MatrixError::DimensionMismatch);
        }

        let n = a_shape[0];

        // QR decomposition: A = Q * R
        let (q, r) = Self::qr_decomposition(a)?;

        // Solve Q * R * x = b
        // First: y = Q^T * b
        let mut y = Tensor::zeros(alloc::vec![n]);
        for i in 0..n {
            let mut sum = 0.0;
            for j in 0..n {
                sum += q.get(&[j, i]).unwrap() * b.get(&[j]).unwrap();
            }
            y.set(&[i], sum);
        }

        // Then: R * x = y (back substitution)
        let mut x = Tensor::zeros(alloc::vec![n]);
        for i in (0..n).rev() {
            let r_ii = *r.get(&[i, i]).unwrap();
            if r_ii.abs() < 1e-10 {
                return Err(MatrixError::Singular);
            }

            let mut sum = *y.get(&[i]).unwrap();
            for j in (i + 1)..n {
                sum -= r.get(&[i, j]).unwrap() * x.get(&[j]).unwrap();
            }
            x.set(&[i], sum / r_ii);
        }

        Ok(x)
    }

    /// Compute matrix rank using SVD
    ///
    /// The rank is the number of non-zero singular values.
    pub fn rank(matrix: &Tensor<f32>, tolerance: f32) -> Result<usize, MatrixError> {
        let (_, s, _) = Self::svd(matrix, 100, 1e-8)?;

        let n = s.shape()[0].min(s.shape()[1]);
        let mut rank = 0;

        for i in 0..n {
            if s.get(&[i, i]).unwrap().abs() > tolerance {
                rank += 1;
            }
        }

        Ok(rank)
    }

    /// Compute the pseudo-inverse using SVD
    ///
    /// The Moore-Penrose pseudo-inverse A+ satisfies:
    /// - A * A+ * A = A
    /// - A+ * A * A+ = A+
    pub fn pseudo_inverse(
        matrix: &Tensor<f32>,
        tolerance: f32,
    ) -> Result<Tensor<f32>, MatrixError> {
        let (u, s, v) = Self::svd(matrix, 100, 1e-8)?;

        let shape = matrix.shape();
        let m = shape[0];
        let n = shape[1];

        // Compute S+ (reciprocals of singular values above tolerance)
        let k = s.shape()[0].min(s.shape()[1]);
        let mut s_inv = Tensor::zeros(alloc::vec![n, m]);

        for i in 0..k {
            let sv = *s.get(&[i, i]).unwrap();
            if sv > tolerance {
                s_inv.set(&[i, i], 1.0 / sv);
            }
        }

        // A+ = V * S+ * U^T
        let ut = Self::transpose(&u)?;
        let vs = Self::matrix_multiply(&v, &s_inv)?;
        Self::matrix_multiply(&vs, &ut)
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
    fn test_identity() {
        let id = Matrix::identity(3);
        assert_eq!(id.shape(), &[3, 3]);
        assert_eq!(id.get(&[0, 0]), Some(&1.0));
        assert_eq!(id.get(&[1, 1]), Some(&1.0));
        assert_eq!(id.get(&[2, 2]), Some(&1.0));
        assert_eq!(id.get(&[0, 1]), Some(&0.0));
    }

    #[test]
    fn test_transpose() {
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
        let t = Matrix::transpose(&m).unwrap();

        assert_eq!(t.shape(), &[3, 2]);
        assert_eq!(t.get(&[0, 0]), Some(&1.0));
        assert_eq!(t.get(&[0, 1]), Some(&4.0));
        assert_eq!(t.get(&[1, 0]), Some(&2.0));
        assert_eq!(t.get(&[2, 1]), Some(&6.0));
    }

    #[test]
    fn test_transpose_square() {
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let t = Matrix::transpose(&m).unwrap();

        assert_eq!(t.shape(), &[2, 2]);
        assert_eq!(t.get(&[0, 0]), Some(&1.0));
        assert_eq!(t.get(&[0, 1]), Some(&3.0));
        assert_eq!(t.get(&[1, 0]), Some(&2.0));
        assert_eq!(t.get(&[1, 1]), Some(&4.0));
    }

    #[test]
    fn test_determinant_2x2() {
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let det = Matrix::determinant(&m).unwrap();
        assert_near(det, -2.0, EPSILON); // 1*4 - 2*3 = -2
    }

    #[test]
    fn test_determinant_3x3() {
        let m = Tensor::matrix(
            std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0],
            3,
            3,
        )
        .unwrap();
        let det = Matrix::determinant(&m).unwrap();
        assert_near(det, -3.0, EPSILON);
    }

    #[test]
    fn test_determinant_4x4() {
        // Use a simple diagonal matrix for predictable determinant
        let m = Tensor::matrix(
            std::vec![
                2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0,
            ],
            4,
            4,
        )
        .unwrap();
        let det = Matrix::determinant(&m).unwrap();
        assert_near(det, 120.0, EPSILON); // 2 * 3 * 4 * 5 = 120
    }

    #[test]
    fn test_inverse_2x2() {
        let m = Tensor::matrix(std::vec![4.0, 7.0, 2.0, 6.0], 2, 2).unwrap();
        let inv = Matrix::inverse(&m).unwrap();

        // Verify A * A^-1 = I
        let id = Matrix::identity(2);
        let product = multiply_matrices(&m, &inv);

        for i in 0..2 {
            for j in 0..2 {
                assert_near(
                    *product.get(&[i, j]).unwrap(),
                    *id.get(&[i, j]).unwrap(),
                    EPSILON,
                );
            }
        }
    }

    #[test]
    fn test_inverse_3x3() {
        let m =
            Tensor::matrix(std::vec![1.0, 2.0, 3.0, 0.0, 1.0, 4.0, 5.0, 6.0, 0.0], 3, 3).unwrap();
        let inv = Matrix::inverse(&m).unwrap();

        // Verify A * A^-1 = I
        let id = Matrix::identity(3);
        let product = multiply_matrices(&m, &inv);

        for i in 0..3 {
            for j in 0..3 {
                assert_near(
                    *product.get(&[i, j]).unwrap(),
                    *id.get(&[i, j]).unwrap(),
                    EPSILON,
                );
            }
        }
    }

    #[test]
    fn test_inverse_4x4() {
        // Non-singular 4x4 matrix
        let m = Tensor::matrix(
            std::vec![
                4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 1.0, 2.0, 0.0, 1.0, 0.0, 0.0, 1.0,
            ],
            4,
            4,
        )
        .unwrap();
        let inv = Matrix::inverse(&m).unwrap();

        // Verify A * A^-1 = I
        let id = Matrix::identity(4);
        let product = multiply_matrices(&m, &inv);

        for i in 0..4 {
            for j in 0..4 {
                assert_near(
                    *product.get(&[i, j]).unwrap(),
                    *id.get(&[i, j]).unwrap(),
                    EPSILON,
                );
            }
        }
    }

    #[test]
    fn test_inverse_singular() {
        // Singular matrix (rows are linearly dependent)
        let m = Tensor::matrix(std::vec![1.0, 2.0, 2.0, 4.0], 2, 2).unwrap();
        let result = Matrix::inverse(&m);
        assert_eq!(result, Err(MatrixError::Singular));
    }

    #[test]
    fn test_trace() {
        let m =
            Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3).unwrap();
        let tr = Matrix::trace(&m).unwrap();
        assert_near(tr, 15.0, EPSILON); // 1 + 5 + 9
    }

    #[test]
    fn test_is_symmetric() {
        let symmetric =
            Tensor::matrix(std::vec![1.0, 2.0, 3.0, 2.0, 4.0, 5.0, 3.0, 5.0, 6.0], 3, 3).unwrap();
        assert!(Matrix::is_symmetric(&symmetric, EPSILON).unwrap());

        let non_symmetric =
            Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3).unwrap();
        assert!(!Matrix::is_symmetric(&non_symmetric, EPSILON).unwrap());
    }

    #[test]
    fn test_is_diagonal() {
        let diagonal =
            Tensor::matrix(std::vec![1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0], 3, 3).unwrap();
        assert!(Matrix::is_diagonal(&diagonal, EPSILON).unwrap());

        let non_diagonal =
            Tensor::matrix(std::vec![1.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0], 3, 3).unwrap();
        assert!(!Matrix::is_diagonal(&non_diagonal, EPSILON).unwrap());
    }

    #[test]
    fn test_diag() {
        let v = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let d = Matrix::diag(&v).unwrap();

        assert_eq!(d.shape(), &[3, 3]);
        assert_eq!(d.get(&[0, 0]), Some(&1.0));
        assert_eq!(d.get(&[1, 1]), Some(&2.0));
        assert_eq!(d.get(&[2, 2]), Some(&3.0));
        assert_eq!(d.get(&[0, 1]), Some(&0.0));
    }

    #[test]
    fn test_extract_diag() {
        let m =
            Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3).unwrap();
        let d = Matrix::extract_diag(&m).unwrap();

        assert_eq!(d.shape(), &[3]);
        assert_eq!(d.data(), &[1.0, 5.0, 9.0]);
    }

    #[test]
    fn test_transpose_not_2d() {
        let v = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let result = Matrix::transpose(&v);
        assert_eq!(result, Err(MatrixError::Not2D));
    }

    #[test]
    fn test_determinant_not_square() {
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
        let result = Matrix::determinant(&m);
        assert_eq!(result, Err(MatrixError::NotSquare));
    }

    // Helper function for tests
    fn multiply_matrices(a: &Tensor<f32>, b: &Tensor<f32>) -> Tensor<f32> {
        let a_shape = a.shape();
        let b_shape = b.shape();

        let m = a_shape[0];
        let k = a_shape[1];
        let n = b_shape[1];

        let mut result = Tensor::zeros(std::vec![m, n]);

        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for p in 0..k {
                    sum += a.get(&[i, p]).unwrap() * b.get(&[p, j]).unwrap();
                }
                result.set(&[i, j], sum);
            }
        }

        result
    }

    // =========================================================================
    // Eigenvalue and SVD Tests
    // =========================================================================

    #[test]
    fn test_power_iteration_diagonal() {
        // Diagonal matrix with known eigenvalues
        let m =
            Tensor::matrix(std::vec![4.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 2.0], 3, 3).unwrap();

        let (eigenvalue, eigenvector, _iterations, converged) =
            Matrix::power_iteration(&m, 1000, 1e-6).unwrap();

        assert!(converged);
        assert_near(eigenvalue, 4.0, 0.01); // Largest eigenvalue

        // Eigenvector should be approximately [1, 0, 0] (normalized)
        let v0 = eigenvector.get(&[0]).unwrap().abs();
        assert!(v0 > 0.9); // Dominant component
    }

    #[test]
    fn test_power_iteration_symmetric() {
        // Symmetric matrix: eigenvalues are 5 and 1
        // [[3, 2], [2, 3]] has eigenvalues 5 and 1
        let m = Tensor::matrix(std::vec![3.0, 2.0, 2.0, 3.0], 2, 2).unwrap();

        let (eigenvalue, _eigenvector, _iterations, converged) =
            Matrix::power_iteration(&m, 1000, 1e-6).unwrap();

        assert!(converged);
        assert_near(eigenvalue, 5.0, 0.01); // Largest eigenvalue
    }

    #[test]
    fn test_qr_decomposition() {
        let m = Tensor::matrix(
            std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0],
            3,
            3,
        )
        .unwrap();

        let (q, r) = Matrix::qr_decomposition(&m).unwrap();

        // Q should be orthogonal: Q^T * Q = I
        let qt = Matrix::transpose(&q).unwrap();
        let qtq = multiply_matrices(&qt, &q);

        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert_near(*qtq.get(&[i, j]).unwrap(), expected, 0.001);
            }
        }

        // Q * R should equal original matrix
        let qr = multiply_matrices(&q, &r);
        for i in 0..3 {
            for j in 0..3 {
                assert_near(*qr.get(&[i, j]).unwrap(), *m.get(&[i, j]).unwrap(), 0.001);
            }
        }

        // R should be upper triangular
        for i in 0..3 {
            for j in 0..i {
                assert_near(*r.get(&[i, j]).unwrap(), 0.0, 0.001);
            }
        }
    }

    #[test]
    fn test_eigenvalues_qr_diagonal() {
        // Diagonal matrix: eigenvalues are the diagonal entries
        let m =
            Tensor::matrix(std::vec![5.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 1.0], 3, 3).unwrap();

        let result = Matrix::eigenvalues_qr(&m, 100, 1e-6).unwrap();

        // Eigenvalues should be 5, 3, 1 (in some order)
        let mut evs: std::vec::Vec<f32> = (0..3)
            .map(|i| *result.eigenvalues.get(&[i]).unwrap())
            .collect();
        evs.sort_by(|a, b| b.partial_cmp(a).unwrap()); // Sort descending

        assert_near(evs[0], 5.0, 0.1);
        assert_near(evs[1], 3.0, 0.1);
        assert_near(evs[2], 1.0, 0.1);
    }

    #[test]
    fn test_eigenvalues_qr_symmetric() {
        // Symmetric 2x2 matrix with known eigenvalues
        let m = Tensor::matrix(std::vec![2.0, 1.0, 1.0, 2.0], 2, 2).unwrap();

        let result = Matrix::eigenvalues_qr(&m, 100, 1e-6).unwrap();

        // Eigenvalues should be 3 and 1
        let ev0 = *result.eigenvalues.get(&[0]).unwrap();
        let ev1 = *result.eigenvalues.get(&[1]).unwrap();

        let max_ev = ev0.max(ev1);
        let min_ev = ev0.min(ev1);

        assert_near(max_ev, 3.0, 0.1);
        assert_near(min_ev, 1.0, 0.1);
    }

    #[test]
    fn test_svd_basic() {
        // Simple 2x2 matrix
        let m = Tensor::matrix(std::vec![3.0, 0.0, 0.0, 2.0], 2, 2).unwrap();

        let (u, s, v) = Matrix::svd(&m, 100, 1e-6).unwrap();

        // Singular values should be 3 and 2 (in some order)
        let s0 = *s.get(&[0, 0]).unwrap();
        let s1 = *s.get(&[1, 1]).unwrap();

        let max_s = s0.max(s1);
        let min_s = s0.min(s1);

        assert_near(max_s, 3.0, 0.1);
        assert_near(min_s, 2.0, 0.1);

        // U should be orthogonal
        let ut = Matrix::transpose(&u).unwrap();
        let utu = multiply_matrices(&ut, &u);
        assert_near(*utu.get(&[0, 0]).unwrap(), 1.0, 0.1);

        // V should be orthogonal
        let vt = Matrix::transpose(&v).unwrap();
        let vtv = multiply_matrices(&vt, &v);
        assert_near(*vtv.get(&[0, 0]).unwrap(), 1.0, 0.1);
    }

    #[test]
    fn test_solve_qr() {
        // Solve 2x + y = 5, x + 3y = 7
        // Solution: x = 8/5 = 1.6, y = 9/5 = 1.8
        let a = Tensor::matrix(std::vec![2.0, 1.0, 1.0, 3.0], 2, 2).unwrap();
        let b = Tensor::vector(std::vec![5.0, 7.0]);

        let x = Matrix::solve_qr(&a, &b).unwrap();

        assert_near(*x.get(&[0]).unwrap(), 1.6, 0.01);
        assert_near(*x.get(&[1]).unwrap(), 1.8, 0.01);
    }

    #[test]
    fn test_solve_qr_3x3() {
        // Identity matrix: solution is b itself
        let a = Matrix::identity(3);
        let b = Tensor::vector(std::vec![1.0, 2.0, 3.0]);

        let x = Matrix::solve_qr(&a, &b).unwrap();

        assert_near(*x.get(&[0]).unwrap(), 1.0, 0.001);
        assert_near(*x.get(&[1]).unwrap(), 2.0, 0.001);
        assert_near(*x.get(&[2]).unwrap(), 3.0, 0.001);
    }

    #[test]
    fn test_rank_full() {
        // Full rank matrix
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0], 2, 2).unwrap();
        let rank = Matrix::rank(&m, 1e-6).unwrap();
        assert_eq!(rank, 2);
    }

    #[test]
    fn test_rank_deficient() {
        // Rank deficient matrix (rows are linearly dependent)
        let m = Tensor::matrix(std::vec![1.0, 2.0, 2.0, 4.0], 2, 2).unwrap();
        let rank = Matrix::rank(&m, 0.1).unwrap();
        assert_eq!(rank, 1);
    }

    #[test]
    fn test_condition_number_identity() {
        let id = Matrix::identity(3);
        let cond = Matrix::condition_number(&id, 100, 1e-6).unwrap();
        // Identity matrix has condition number 1
        assert_near(cond, 1.0, 0.1);
    }

    #[test]
    fn test_condition_number_ill_conditioned() {
        // Nearly singular matrix should have high condition number
        // [[1, 0], [0, 0.001]] has condition number 1000
        let m = Tensor::matrix(std::vec![1.0, 0.0, 0.0, 0.001], 2, 2).unwrap();
        let cond = Matrix::condition_number(&m, 100, 1e-8).unwrap();
        // Should be approximately 1000 (1/0.001)
        assert!(cond > 500.0);
    }

    #[test]
    fn test_pseudo_inverse_square() {
        // For invertible matrix, pseudo-inverse equals inverse
        let m = Tensor::matrix(std::vec![4.0, 7.0, 2.0, 6.0], 2, 2).unwrap();
        let pinv = Matrix::pseudo_inverse(&m, 1e-6).unwrap();

        // m * pinv should be approximately identity
        let product = multiply_matrices(&m, &pinv);
        assert_near(*product.get(&[0, 0]).unwrap(), 1.0, 0.1);
        assert_near(*product.get(&[0, 1]).unwrap(), 0.0, 0.1);
        assert_near(*product.get(&[1, 0]).unwrap(), 0.0, 0.1);
        assert_near(*product.get(&[1, 1]).unwrap(), 1.0, 0.1);
    }

    #[test]
    fn test_eigen_error_not_square() {
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3).unwrap();
        let result = Matrix::eigenvalues_qr(&m, 100, 1e-6);
        assert_eq!(result, Err(MatrixError::NotSquare));
    }

    #[test]
    fn test_power_iteration_not_2d() {
        let v = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let result = Matrix::power_iteration(&v, 100, 1e-6);
        assert_eq!(result, Err(MatrixError::Not2D));
    }

    // =========================================================================
    // Numerical Stability Tests
    // =========================================================================

    #[test]
    fn test_nearly_singular_matrix_determinant() {
        // Matrix very close to singular (rows nearly proportional)
        let m = Tensor::matrix(std::vec![1.0, 2.0, 1.0000001, 2.0000002], 2, 2).unwrap();
        let det = Matrix::determinant(&m).unwrap();

        // Determinant should be very small but non-zero
        assert!(det.abs() < 1e-5);
    }

    #[test]
    fn test_ill_conditioned_inverse() {
        // Hilbert matrix (2x2) is ill-conditioned but still invertible
        // H = [[1, 1/2], [1/2, 1/3]]
        let m = Tensor::matrix(std::vec![1.0, 0.5, 0.5, 0.333333], 2, 2).unwrap();
        let inv = Matrix::inverse(&m).unwrap();

        // Verify that the inverse exists and A * A^-1 ≈ I
        let product = multiply_matrices(&m, &inv);
        let id = Matrix::identity(2);

        for i in 0..2 {
            for j in 0..2 {
                assert_near(
                    *product.get(&[i, j]).unwrap(),
                    *id.get(&[i, j]).unwrap(),
                    0.01, // Larger tolerance for ill-conditioned matrix
                );
            }
        }
    }

    #[test]
    fn test_large_values_determinant() {
        // Test with large values to check for overflow
        let m = Tensor::matrix(std::vec![1000.0, 0.0, 0.0, 1000.0,], 2, 2).unwrap();
        let det = Matrix::determinant(&m).unwrap();
        assert_near(det, 1_000_000.0, 1.0);
    }

    #[test]
    fn test_small_values_determinant() {
        // Test with small values to check for underflow
        let m = Tensor::matrix(std::vec![0.001, 0.0, 0.0, 0.001,], 2, 2).unwrap();
        let det = Matrix::determinant(&m).unwrap();
        assert_near(det, 0.000001, 1e-9);
    }

    #[test]
    fn test_mixed_scale_matrix() {
        // Matrix with very different magnitude entries
        let m = Tensor::matrix(std::vec![1000.0, 0.001, 0.001, 0.001,], 2, 2).unwrap();

        let det = Matrix::determinant(&m).unwrap();
        // det = 1000*0.001 - 0.001*0.001 = 1 - 0.000001 ≈ 1
        assert_near(det, 1.0, 0.01);
    }

    #[test]
    fn test_orthogonal_matrix_determinant() {
        // Rotation matrix (orthogonal) should have det = ±1
        let angle = 0.5; // radians
        let cos_a = libm::cosf(angle);
        let sin_a = libm::sinf(angle);

        let m = Tensor::matrix(std::vec![cos_a, -sin_a, sin_a, cos_a,], 2, 2).unwrap();

        let det = Matrix::determinant(&m).unwrap();
        assert_near(det, 1.0, EPSILON);
    }

    #[test]
    fn test_nearly_zero_singular_values() {
        // Matrix with one very small singular value
        // This tests numerical stability of SVD
        let m = Tensor::matrix(std::vec![1.0, 0.0, 0.0, 1e-7,], 2, 2).unwrap();

        let (_u, s, _v) = Matrix::svd(&m, 100, 1e-10).unwrap();

        // Should compute both singular values correctly
        let s0 = *s.get(&[0, 0]).unwrap();
        let s1 = *s.get(&[1, 1]).unwrap();

        let max_s = s0.max(s1);
        let min_s = s0.min(s1);

        assert_near(max_s, 1.0, 0.01);
        assert!(min_s < 1e-6); // Small but should be detected
    }

    #[test]
    fn test_qr_numerical_stability() {
        // QR decomposition should maintain orthogonality even for ill-conditioned matrices
        let m = Tensor::matrix(
            std::vec![1.0, 1.0, 1.0, 1e-6, 0.0, 0.0, 0.0, 1e-6, 0.0,],
            3,
            3,
        )
        .unwrap();

        let (q, _r) = Matrix::qr_decomposition(&m).unwrap();

        // Q^T * Q should be identity
        let qt = Matrix::transpose(&q).unwrap();
        let qtq = multiply_matrices(&qt, &q);

        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert_near(*qtq.get(&[i, j]).unwrap(), expected, 0.01);
            }
        }
    }

    #[test]
    fn test_condition_number_scaling() {
        // Scaling a matrix should not change its condition number significantly
        let m = Tensor::matrix(std::vec![2.0, 1.0, 1.0, 2.0,], 2, 2).unwrap();

        let m_scaled = Tensor::matrix(std::vec![20.0, 10.0, 10.0, 20.0,], 2, 2).unwrap();

        let cond1 = Matrix::condition_number(&m, 100, 1e-8).unwrap();
        let cond2 = Matrix::condition_number(&m_scaled, 100, 1e-8).unwrap();

        // Condition numbers should be approximately equal
        assert_near(cond1, cond2, 0.5);
    }

    #[test]
    fn test_pseudo_inverse_rank_deficient() {
        // Pseudo-inverse should work for rank-deficient matrices
        // This matrix has rank 1 (second row is 2x first row)
        let m = Tensor::matrix(std::vec![1.0, 2.0, 3.0, 2.0, 4.0, 6.0,], 2, 3).unwrap();

        let pinv = Matrix::pseudo_inverse(&m, 1e-6).unwrap();

        // Should have correct dimensions (3x2)
        assert_eq!(pinv.shape(), &[3, 2]);

        // A * A+ * A should equal A (one of the Moore-Penrose conditions)
        let a_pinv = Matrix::matrix_multiply(&m, &pinv).unwrap();
        let a_pinv_a = Matrix::matrix_multiply(&a_pinv, &m).unwrap();

        for i in 0..2 {
            for j in 0..3 {
                assert_near(
                    *a_pinv_a.get(&[i, j]).unwrap(),
                    *m.get(&[i, j]).unwrap(),
                    0.1,
                );
            }
        }
    }

    #[test]
    fn test_eigenvalue_convergence() {
        // Test that QR algorithm converges for a well-conditioned matrix
        let m = Tensor::matrix(
            std::vec![4.0, 1.0, 0.0, 1.0, 4.0, 1.0, 0.0, 1.0, 4.0,],
            3,
            3,
        )
        .unwrap();

        let result = Matrix::eigenvalues_qr(&m, 1000, 1e-6).unwrap();

        // Should converge
        assert!(result.converged);

        // Eigenvalues should be real and positive for this symmetric matrix
        for i in 0..3 {
            let ev = *result.eigenvalues.get(&[i]).unwrap();
            assert!(ev > 0.0);
        }
    }

    #[test]
    fn test_trace_numerical_accuracy() {
        // Trace should sum diagonal elements accurately even with cancellation
        let m = Tensor::matrix(
            std::vec![1e10, 0.0, 0.0, 0.0, -1e10, 0.0, 0.0, 0.0, 1.0,],
            3,
            3,
        )
        .unwrap();

        let tr = Matrix::trace(&m).unwrap();

        // Should be 1e10 - 1e10 + 1 = 1
        // This tests numerical cancellation
        assert_near(tr, 1.0, 1.0);
    }
}
