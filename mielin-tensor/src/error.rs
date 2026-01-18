//! Error Handling Module
//!
//! Provides comprehensive error types for tensor operations with detailed diagnostics.
//! Improves debugging by providing context about what went wrong and why.

#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Result type for tensor operations
pub type TensorResult<T> = core::result::Result<T, TensorError>;

/// Comprehensive error type for tensor operations
#[derive(Debug, Clone, PartialEq)]
pub enum TensorError {
    /// Shape mismatch between tensors
    ShapeMismatch {
        operation: &'static str,
        expected: Vec<usize>,
        got: Vec<usize>,
        message: Option<String>,
    },

    /// Dimension mismatch (e.g., expected 2D but got 3D)
    DimensionMismatch {
        operation: &'static str,
        expected_dims: usize,
        got_dims: usize,
    },

    /// Invalid dimension index
    InvalidDimension {
        dimension: usize,
        max_dimension: usize,
    },

    /// Invalid index access
    IndexOutOfBounds {
        indices: Vec<usize>,
        shape: Vec<usize>,
    },

    /// Invalid reshape (size mismatch)
    InvalidReshape {
        original_shape: Vec<usize>,
        new_shape: Vec<usize>,
        original_size: usize,
        new_size: usize,
    },

    /// Singular matrix (not invertible)
    SingularMatrix {
        operation: &'static str,
        determinant: f32,
    },

    /// Matrix is not square
    NotSquareMatrix {
        operation: &'static str,
        shape: Vec<usize>,
    },

    /// Invalid convolution parameters
    InvalidConvolution { reason: String },

    /// Invalid pooling parameters
    InvalidPooling { reason: String },

    /// Numerical error (NaN, Inf, overflow)
    NumericalError {
        operation: &'static str,
        message: String,
    },

    /// Backend not available
    BackendNotAvailable {
        backend: &'static str,
        reason: String,
    },

    /// Quantization error
    QuantizationError {
        operation: &'static str,
        reason: String,
    },

    /// Gradient computation error
    GradientError {
        operation: &'static str,
        reason: String,
    },

    /// Memory allocation failure
    AllocationError {
        requested_size: usize,
        available_size: Option<usize>,
    },

    /// Generic error with message
    Other { message: String },
}

impl TensorError {
    /// Create a generic error from a message
    pub fn other<S: Into<String>>(message: S) -> Self {
        Self::Other {
            message: message.into(),
        }
    }

    /// Create a shape mismatch error
    pub fn shape_mismatch(operation: &'static str, expected: Vec<usize>, got: Vec<usize>) -> Self {
        Self::ShapeMismatch {
            operation,
            expected,
            got,
            message: None,
        }
    }

    /// Create a shape mismatch error with a custom message
    pub fn shape_mismatch_with_msg(
        operation: &'static str,
        expected: Vec<usize>,
        got: Vec<usize>,
        message: String,
    ) -> Self {
        Self::ShapeMismatch {
            operation,
            expected,
            got,
            message: Some(message),
        }
    }

    /// Create a dimension mismatch error
    pub fn dimension_mismatch(
        operation: &'static str,
        expected_dims: usize,
        got_dims: usize,
    ) -> Self {
        Self::DimensionMismatch {
            operation,
            expected_dims,
            got_dims,
        }
    }

    /// Create an invalid reshape error
    pub fn invalid_reshape(original_shape: Vec<usize>, new_shape: Vec<usize>) -> Self {
        let original_size = original_shape.iter().product();
        let new_size = new_shape.iter().product();
        Self::InvalidReshape {
            original_shape,
            new_shape,
            original_size,
            new_size,
        }
    }

    /// Create a singular matrix error
    pub fn singular_matrix(operation: &'static str, determinant: f32) -> Self {
        Self::SingularMatrix {
            operation,
            determinant,
        }
    }

    /// Create a not square matrix error
    pub fn not_square_matrix(operation: &'static str, shape: Vec<usize>) -> Self {
        Self::NotSquareMatrix { operation, shape }
    }

    /// Create a numerical error
    pub fn numerical_error(operation: &'static str, message: String) -> Self {
        Self::NumericalError { operation, message }
    }

    /// Create a backend not available error
    pub fn backend_not_available(backend: &'static str, reason: String) -> Self {
        Self::BackendNotAvailable { backend, reason }
    }

    /// Create a quantization error
    pub fn quantization_error(operation: &'static str, reason: String) -> Self {
        Self::QuantizationError { operation, reason }
    }

    /// Create a gradient error
    pub fn gradient_error(operation: &'static str, reason: String) -> Self {
        Self::GradientError { operation, reason }
    }

    /// Get a human-readable error message
    pub fn message(&self) -> String {
        match self {
            Self::ShapeMismatch {
                operation,
                expected,
                got,
                message,
            } => {
                let mut msg = alloc::format!(
                    "Shape mismatch in {}: expected {:?}, got {:?}",
                    operation,
                    expected,
                    got
                );
                if let Some(extra) = message {
                    msg.push_str(&alloc::format!(". {}", extra));
                }
                msg
            }
            Self::DimensionMismatch {
                operation,
                expected_dims,
                got_dims,
            } => alloc::format!(
                "Dimension mismatch in {}: expected {} dimensions, got {}",
                operation,
                expected_dims,
                got_dims
            ),
            Self::InvalidDimension {
                dimension,
                max_dimension,
            } => alloc::format!(
                "Invalid dimension index {}: maximum is {}",
                dimension,
                max_dimension
            ),
            Self::IndexOutOfBounds { indices, shape } => {
                alloc::format!("Index {:?} out of bounds for shape {:?}", indices, shape)
            }
            Self::InvalidReshape {
                original_shape,
                new_shape,
                original_size,
                new_size,
            } => alloc::format!(
                "Invalid reshape: {:?} (size {}) cannot be reshaped to {:?} (size {})",
                original_shape,
                original_size,
                new_shape,
                new_size
            ),
            Self::SingularMatrix {
                operation,
                determinant,
            } => alloc::format!(
                "Singular matrix in {}: determinant = {}, matrix is not invertible",
                operation,
                determinant
            ),
            Self::NotSquareMatrix { operation, shape } => {
                alloc::format!("Not a square matrix in {}: shape {:?}", operation, shape)
            }
            Self::InvalidConvolution { reason } => {
                alloc::format!("Invalid convolution: {}", reason)
            }
            Self::InvalidPooling { reason } => {
                alloc::format!("Invalid pooling: {}", reason)
            }
            Self::NumericalError { operation, message } => {
                alloc::format!("Numerical error in {}: {}", operation, message)
            }
            Self::BackendNotAvailable { backend, reason } => {
                alloc::format!("Backend {} not available: {}", backend, reason)
            }
            Self::QuantizationError { operation, reason } => {
                alloc::format!("Quantization error in {}: {}", operation, reason)
            }
            Self::GradientError { operation, reason } => {
                alloc::format!("Gradient error in {}: {}", operation, reason)
            }
            Self::AllocationError {
                requested_size,
                available_size,
            } => {
                if let Some(available) = available_size {
                    alloc::format!(
                        "Memory allocation failed: requested {} bytes, only {} available",
                        requested_size,
                        available
                    )
                } else {
                    alloc::format!(
                        "Memory allocation failed: requested {} bytes",
                        requested_size
                    )
                }
            }
            Self::Other { message } => message.clone(),
        }
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::BackendNotAvailable { .. } | Self::NumericalError { .. }
        )
    }

    /// Get error category
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::ShapeMismatch { .. }
            | Self::DimensionMismatch { .. }
            | Self::InvalidDimension { .. }
            | Self::InvalidReshape { .. }
            | Self::NotSquareMatrix { .. } => ErrorCategory::ShapeError,

            Self::IndexOutOfBounds { .. } => ErrorCategory::IndexError,

            Self::SingularMatrix { .. } | Self::NumericalError { .. } => {
                ErrorCategory::NumericalError
            }

            Self::InvalidConvolution { .. } | Self::InvalidPooling { .. } => {
                ErrorCategory::OperationError
            }

            Self::BackendNotAvailable { .. } => ErrorCategory::BackendError,

            Self::QuantizationError { .. } => ErrorCategory::QuantizationError,

            Self::GradientError { .. } => ErrorCategory::GradientError,

            Self::AllocationError { .. } => ErrorCategory::MemoryError,

            Self::Other { .. } => ErrorCategory::Unknown,
        }
    }
}

/// Error categories for grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Shape-related errors
    ShapeError,
    /// Index-related errors
    IndexError,
    /// Numerical computation errors
    NumericalError,
    /// Invalid operation errors
    OperationError,
    /// Backend availability errors
    BackendError,
    /// Quantization errors
    QuantizationError,
    /// Gradient computation errors
    GradientError,
    /// Memory allocation errors
    MemoryError,
    /// Unknown/other errors
    Unknown,
}

impl fmt::Display for TensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

// Note: std::error::Error implementation omitted for no_std compatibility
// In std environments, users can use the Display impl for error formatting

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shape_mismatch_error() {
        let err = TensorError::shape_mismatch("matmul", alloc::vec![2, 3], alloc::vec![4, 5]);
        assert_eq!(err.category(), ErrorCategory::ShapeError);
        assert!(err.message().contains("matmul"));
        assert!(err.message().contains("[2, 3]"));
        assert!(err.message().contains("[4, 5]"));
    }

    #[test]
    fn test_dimension_mismatch_error() {
        let err = TensorError::dimension_mismatch("transpose", 2, 3);
        assert_eq!(err.category(), ErrorCategory::ShapeError);
        assert!(err.message().contains("expected 2 dimensions"));
    }

    #[test]
    fn test_invalid_reshape_error() {
        let err = TensorError::invalid_reshape(alloc::vec![2, 3], alloc::vec![4, 2]);
        assert_eq!(err.category(), ErrorCategory::ShapeError);
        assert!(err.message().contains("size 6"));
        assert!(err.message().contains("size 8"));
    }

    #[test]
    fn test_singular_matrix_error() {
        let err = TensorError::singular_matrix("inverse", 0.0);
        assert_eq!(err.category(), ErrorCategory::NumericalError);
        assert!(err.message().contains("not invertible"));
    }

    #[test]
    fn test_numerical_error() {
        let err =
            TensorError::numerical_error("sqrt", "Cannot compute sqrt of negative number".into());
        assert_eq!(err.category(), ErrorCategory::NumericalError);
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_backend_not_available() {
        let err = TensorError::backend_not_available("CUDA", "No CUDA device found".into());
        assert_eq!(err.category(), ErrorCategory::BackendError);
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_quantization_error() {
        let err = TensorError::quantization_error("quantize", "Invalid scale factor".into());
        assert_eq!(err.category(), ErrorCategory::QuantizationError);
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_gradient_error() {
        let err = TensorError::gradient_error("backward", "Graph cycle detected".into());
        assert_eq!(err.category(), ErrorCategory::GradientError);
        assert!(err.message().contains("Graph cycle"));
    }

    #[test]
    fn test_allocation_error() {
        let err = TensorError::AllocationError {
            requested_size: 1024,
            available_size: Some(512),
        };
        assert_eq!(err.category(), ErrorCategory::MemoryError);
        assert!(err.message().contains("1024"));
        assert!(err.message().contains("512"));
    }

    #[test]
    fn test_error_with_custom_message() {
        let err = TensorError::shape_mismatch_with_msg(
            "add",
            alloc::vec![2, 3],
            alloc::vec![2, 4],
            "Column count must match".into(),
        );
        assert!(err.message().contains("Column count must match"));
    }

    #[test]
    fn test_index_out_of_bounds() {
        let err = TensorError::IndexOutOfBounds {
            indices: alloc::vec![2, 5],
            shape: alloc::vec![3, 4],
        };
        assert_eq!(err.category(), ErrorCategory::IndexError);
        assert!(err.message().contains("[2, 5]"));
    }
}
