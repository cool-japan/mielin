//! Broadcasting support for tensor operations
//!
//! Broadcasting allows element-wise operations on tensors with different shapes
//! following NumPy-style broadcasting rules:
//! 1. If tensors have different number of dimensions, prepend 1s to the smaller shape
//! 2. Dimensions are compatible if they're equal or one of them is 1
//! 3. Result shape is the maximum along each dimension

extern crate alloc;
use alloc::vec::Vec;

/// Check if two shapes can be broadcast together
pub fn can_broadcast(shape_a: &[usize], shape_b: &[usize]) -> bool {
    let ndim_a = shape_a.len();
    let ndim_b = shape_b.len();
    let max_ndim = ndim_a.max(ndim_b);

    for i in 0..max_ndim {
        let dim_a = if i < ndim_a {
            shape_a[ndim_a - 1 - i]
        } else {
            1
        };
        let dim_b = if i < ndim_b {
            shape_b[ndim_b - 1 - i]
        } else {
            1
        };

        if dim_a != dim_b && dim_a != 1 && dim_b != 1 {
            return false;
        }
    }

    true
}

/// Compute the broadcast shape for two input shapes
pub fn broadcast_shape(shape_a: &[usize], shape_b: &[usize]) -> Option<Vec<usize>> {
    if !can_broadcast(shape_a, shape_b) {
        return None;
    }

    let ndim_a = shape_a.len();
    let ndim_b = shape_b.len();
    let max_ndim = ndim_a.max(ndim_b);

    let mut result = Vec::with_capacity(max_ndim);

    for i in 0..max_ndim {
        let dim_a = if i < ndim_a {
            shape_a[ndim_a - 1 - i]
        } else {
            1
        };
        let dim_b = if i < ndim_b {
            shape_b[ndim_b - 1 - i]
        } else {
            1
        };

        result.push(dim_a.max(dim_b));
    }

    result.reverse();
    Some(result)
}

/// Calculate strides for broadcasting
/// Returns (stride_a, stride_b) where stride is 0 if dimension should be broadcast
pub fn broadcast_strides(
    shape_a: &[usize],
    shape_b: &[usize],
    broadcast_shape: &[usize],
) -> (Vec<usize>, Vec<usize>) {
    let ndim_a = shape_a.len();
    let ndim_b = shape_b.len();
    let ndim_out = broadcast_shape.len();

    let mut strides_a = Vec::with_capacity(ndim_out);
    let mut strides_b = Vec::with_capacity(ndim_out);

    // Calculate strides for each dimension
    let mut stride_a = 1;
    let mut stride_b = 1;

    for i in (0..ndim_out).rev() {
        let dim_a = if i >= ndim_out - ndim_a {
            shape_a[i - (ndim_out - ndim_a)]
        } else {
            1
        };
        let dim_b = if i >= ndim_out - ndim_b {
            shape_b[i - (ndim_out - ndim_b)]
        } else {
            1
        };

        // Stride is 0 if dimension is 1 (broadcast dimension)
        if dim_a == 1 {
            strides_a.push(0);
        } else {
            strides_a.push(stride_a);
            stride_a *= dim_a;
        }

        if dim_b == 1 {
            strides_b.push(0);
        } else {
            strides_b.push(stride_b);
            stride_b *= dim_b;
        }
    }

    strides_a.reverse();
    strides_b.reverse();

    (strides_a, strides_b)
}

/// Convert multi-dimensional index to flat index using strides
#[inline]
pub fn index_from_strides(indices: &[usize], strides: &[usize]) -> usize {
    indices
        .iter()
        .zip(strides.iter())
        .map(|(&idx, &stride)| idx * stride)
        .sum()
}

/// Compute multi-dimensional indices from flat index
pub fn unravel_index(mut index: usize, shape: &[usize]) -> Vec<usize> {
    let mut indices = Vec::with_capacity(shape.len());

    for &dim in shape.iter().rev() {
        indices.push(index % dim);
        index /= dim;
    }

    indices.reverse();
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_can_broadcast_same_shape() {
        assert!(can_broadcast(&[3, 4], &[3, 4]));
    }

    #[test]
    fn test_can_broadcast_scalar() {
        assert!(can_broadcast(&[3, 4], &[1]));
        assert!(can_broadcast(&[1], &[3, 4]));
    }

    #[test]
    fn test_can_broadcast_compatible() {
        assert!(can_broadcast(&[3, 1], &[1, 4]));
        assert!(can_broadcast(&[5, 1, 3], &[1, 4, 3]));
    }

    #[test]
    fn test_cannot_broadcast_incompatible() {
        assert!(!can_broadcast(&[3, 4], &[3, 5]));
        assert!(!can_broadcast(&[2, 3], &[3, 2]));
    }

    #[test]
    fn test_broadcast_shape_same() {
        assert_eq!(broadcast_shape(&[3, 4], &[3, 4]), Some(std::vec![3, 4]));
    }

    #[test]
    fn test_broadcast_shape_scalar() {
        assert_eq!(broadcast_shape(&[3, 4], &[1]), Some(std::vec![3, 4]));
        assert_eq!(broadcast_shape(&[1], &[3, 4]), Some(std::vec![3, 4]));
    }

    #[test]
    fn test_broadcast_shape_compatible() {
        assert_eq!(broadcast_shape(&[3, 1], &[1, 4]), Some(std::vec![3, 4]));
        assert_eq!(
            broadcast_shape(&[5, 1, 3], &[1, 4, 3]),
            Some(std::vec![5, 4, 3])
        );
    }

    #[test]
    fn test_broadcast_shape_incompatible() {
        assert_eq!(broadcast_shape(&[3, 4], &[3, 5]), None);
    }

    #[test]
    fn test_broadcast_strides() {
        let shape_a = &[3, 1];
        let shape_b = &[1, 4];
        let broadcast = &[3, 4];

        let (strides_a, strides_b) = broadcast_strides(shape_a, shape_b, broadcast);

        // Shape A: [3, 1] -> dim 1 is broadcast, so stride should be 0
        assert_eq!(strides_a, std::vec![1, 0]);

        // Shape B: [1, 4] -> dim 0 is broadcast, so stride should be 0
        assert_eq!(strides_b, std::vec![0, 1]);
    }

    #[test]
    fn test_index_from_strides() {
        let strides = &[4, 1];
        assert_eq!(index_from_strides(&[0, 0], strides), 0);
        assert_eq!(index_from_strides(&[0, 1], strides), 1);
        assert_eq!(index_from_strides(&[1, 0], strides), 4);
        assert_eq!(index_from_strides(&[2, 3], strides), 11);
    }

    #[test]
    fn test_unravel_index() {
        let shape = &[3, 4];
        assert_eq!(unravel_index(0, shape), std::vec![0, 0]);
        assert_eq!(unravel_index(1, shape), std::vec![0, 1]);
        assert_eq!(unravel_index(4, shape), std::vec![1, 0]);
        assert_eq!(unravel_index(11, shape), std::vec![2, 3]);
    }
}
