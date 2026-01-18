//! Zero-Copy Tensor Views and Slices
//!
//! Provides efficient view and slice operations without copying data.
//! Views share the underlying data with the original tensor but can have
//! different shapes and strides.
//!
//! ## Features
//!
//! - **TensorView**: Immutable view into a tensor
//! - **TensorViewMut**: Mutable view for in-place operations
//! - **Slice Ranges**: Python-style slicing with start:stop:step
//! - **Transpose Views**: Zero-copy transpose by swapping strides
//! - **Broadcast Views**: Logical expansion without memory duplication
//!
//! ## Example
//!
//! ```rust
//! use mielin_tensor::view::{TensorView, SliceRange};
//!
//! let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
//! let view = TensorView::from_slice(&data, vec![5]);
//! let sliced = view.slice(&[SliceRange::new(1, 4, 1)]);
//! assert_eq!(sliced.to_vec(), vec![2.0, 3.0, 4.0]);
//! ```

extern crate alloc;

use alloc::vec::Vec;
use core::marker::PhantomData;
use core::ops::Range;

/// A slice range specification (start:stop:step)
#[derive(Debug, Clone, Copy)]
pub struct SliceRange {
    /// Start index (inclusive)
    pub start: usize,
    /// End index (exclusive), None means end of dimension
    pub end: Option<usize>,
    /// Step size (must be > 0)
    pub step: usize,
    /// Whether to drop this dimension (single index selection)
    pub drop_dim: bool,
}

impl SliceRange {
    /// Create a new slice range
    pub const fn new(start: usize, end: usize, step: usize) -> Self {
        Self {
            start,
            end: Some(end),
            step: if step == 0 { 1 } else { step },
            drop_dim: false,
        }
    }

    /// Create a slice range from start to end with step 1
    pub const fn range(start: usize, end: usize) -> Self {
        Self::new(start, end, 1)
    }

    /// Create a slice selecting all elements
    pub const fn all() -> Self {
        Self {
            start: 0,
            end: None,
            step: 1,
            drop_dim: false,
        }
    }

    /// Create a slice selecting a single index (drops the dimension like numpy)
    pub const fn index(idx: usize) -> Self {
        Self {
            start: idx,
            end: Some(idx + 1),
            step: 1,
            drop_dim: true,
        }
    }

    /// Create a slice selecting a single index but keeping the dimension
    pub const fn index_keep_dim(idx: usize) -> Self {
        Self::new(idx, idx + 1, 1)
    }

    /// Compute the number of elements in this slice given dimension size
    pub fn len(&self, dim_size: usize) -> usize {
        let end = self.end.unwrap_or(dim_size).min(dim_size);
        if self.start >= end {
            return 0;
        }
        (end - self.start).div_ceil(self.step)
    }

    /// Check if slice is empty
    pub fn is_empty(&self, dim_size: usize) -> bool {
        self.len(dim_size) == 0
    }

    /// Get the effective end index
    pub fn effective_end(&self, dim_size: usize) -> usize {
        self.end.unwrap_or(dim_size).min(dim_size)
    }
}

impl Default for SliceRange {
    fn default() -> Self {
        Self::all()
    }
}

impl From<Range<usize>> for SliceRange {
    fn from(range: Range<usize>) -> Self {
        Self::range(range.start, range.end)
    }
}

impl From<usize> for SliceRange {
    fn from(idx: usize) -> Self {
        Self::index(idx)
    }
}

/// An immutable view into a tensor
///
/// Views share the underlying data with the original tensor.
/// They can have different shapes and strides but never copy data.
#[derive(Debug, Clone)]
pub struct TensorView<'a, T> {
    /// Pointer to the start of this view's data
    data: &'a [T],
    /// Offset from the start of the original data
    offset: usize,
    /// Shape of this view
    shape: Vec<usize>,
    /// Strides for this view (may differ from original)
    strides: Vec<usize>,
    /// Phantom data for lifetime
    _marker: PhantomData<&'a T>,
}

impl<'a, T> TensorView<'a, T> {
    /// Create a view of the entire tensor
    pub fn from_slice(data: &'a [T], shape: Vec<usize>) -> Self {
        let strides = compute_strides(&shape);
        Self {
            data,
            offset: 0,
            shape,
            strides,
            _marker: PhantomData,
        }
    }

    /// Create from raw parts
    pub fn from_parts(
        data: &'a [T],
        offset: usize,
        shape: Vec<usize>,
        strides: Vec<usize>,
    ) -> Self {
        Self {
            data,
            offset,
            shape,
            strides,
            _marker: PhantomData,
        }
    }

    /// Get the shape of this view
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get the strides of this view
    pub fn strides(&self) -> &[usize] {
        &self.strides
    }

    /// Get the number of dimensions
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Get the total number of elements
    pub fn size(&self) -> usize {
        self.shape.iter().product()
    }

    /// Check if this view is contiguous in memory
    pub fn is_contiguous(&self) -> bool {
        if self.shape.is_empty() {
            return true;
        }

        let expected_strides = compute_strides(&self.shape);
        self.strides == expected_strides
    }

    /// Get an element at the given multi-dimensional index
    pub fn get(&self, indices: &[usize]) -> Option<&T> {
        let flat_index = self.compute_flat_index(indices)?;
        self.data.get(flat_index)
    }

    /// Compute the flat index for multi-dimensional indices
    fn compute_flat_index(&self, indices: &[usize]) -> Option<usize> {
        if indices.len() != self.shape.len() {
            return None;
        }

        let mut flat_index = self.offset;
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

        if flat_index >= self.data.len() {
            return None;
        }

        Some(flat_index)
    }

    /// Create a sub-view using slice ranges
    ///
    /// Each slice range corresponds to one dimension.
    /// Missing slices are treated as `SliceRange::all()`.
    /// If a slice has `drop_dim = true` (created with `SliceRange::index()`),
    /// that dimension is removed from the result (numpy-like indexing).
    pub fn slice(&self, slices: &[SliceRange]) -> TensorView<'a, T> {
        let mut new_offset = self.offset;
        let mut new_shape = Vec::with_capacity(self.ndim());
        let mut new_strides = Vec::with_capacity(self.ndim());

        for (i, (&dim_size, &stride)) in self.shape.iter().zip(self.strides.iter()).enumerate() {
            let slice = slices.get(i).copied().unwrap_or(SliceRange::all());

            // Adjust offset for start
            new_offset += slice.start * stride;

            // Skip dimension if drop_dim is set (single index selection)
            if slice.drop_dim {
                continue;
            }

            // New dimension size
            let new_dim = slice.len(dim_size);
            if new_dim > 0 {
                new_shape.push(new_dim);
                new_strides.push(stride * slice.step);
            }
        }

        TensorView {
            data: self.data,
            offset: new_offset,
            shape: new_shape,
            strides: new_strides,
            _marker: PhantomData,
        }
    }

    /// Create a transposed view (swap last two dimensions)
    ///
    /// Returns None if tensor has less than 2 dimensions.
    pub fn transpose(&self) -> Option<TensorView<'a, T>> {
        if self.ndim() < 2 {
            return None;
        }

        let mut new_shape = self.shape.clone();
        let mut new_strides = self.strides.clone();

        let n = self.ndim();
        new_shape.swap(n - 2, n - 1);
        new_strides.swap(n - 2, n - 1);

        Some(TensorView {
            data: self.data,
            offset: self.offset,
            shape: new_shape,
            strides: new_strides,
            _marker: PhantomData,
        })
    }

    /// Swap two axes
    pub fn swap_axes(&self, axis1: usize, axis2: usize) -> Option<TensorView<'a, T>> {
        if axis1 >= self.ndim() || axis2 >= self.ndim() {
            return None;
        }

        let mut new_shape = self.shape.clone();
        let mut new_strides = self.strides.clone();

        new_shape.swap(axis1, axis2);
        new_strides.swap(axis1, axis2);

        Some(TensorView {
            data: self.data,
            offset: self.offset,
            shape: new_shape,
            strides: new_strides,
            _marker: PhantomData,
        })
    }

    /// Reshape the view (must preserve total size)
    ///
    /// Note: This only works for contiguous views.
    pub fn reshape(&self, new_shape: Vec<usize>) -> Option<TensorView<'a, T>> {
        let new_size: usize = new_shape.iter().product();
        if new_size != self.size() || !self.is_contiguous() {
            return None;
        }

        let new_strides = compute_strides(&new_shape);

        Some(TensorView {
            data: self.data,
            offset: self.offset,
            shape: new_shape,
            strides: new_strides,
            _marker: PhantomData,
        })
    }

    /// Squeeze: remove dimensions of size 1
    pub fn squeeze(&self) -> TensorView<'a, T> {
        let mut new_shape = Vec::new();
        let mut new_strides = Vec::new();

        for (&dim, &stride) in self.shape.iter().zip(self.strides.iter()) {
            if dim > 1 {
                new_shape.push(dim);
                new_strides.push(stride);
            }
        }

        // Keep at least one dimension
        if new_shape.is_empty() {
            new_shape.push(1);
            new_strides.push(1);
        }

        TensorView {
            data: self.data,
            offset: self.offset,
            shape: new_shape,
            strides: new_strides,
            _marker: PhantomData,
        }
    }

    /// Unsqueeze: add a dimension of size 1 at the given axis
    pub fn unsqueeze(&self, axis: usize) -> Option<TensorView<'a, T>> {
        if axis > self.ndim() {
            return None;
        }

        let mut new_shape = self.shape.clone();
        let mut new_strides = self.strides.clone();

        // Insert dimension of size 1
        new_shape.insert(axis, 1);
        // Stride can be anything for size-1 dimension, use 0 for clarity
        let stride = if axis < self.strides.len() {
            self.strides[axis]
        } else {
            1
        };
        new_strides.insert(axis, stride);

        Some(TensorView {
            data: self.data,
            offset: self.offset,
            shape: new_shape,
            strides: new_strides,
            _marker: PhantomData,
        })
    }

    /// Iterate over elements in logical (row-major) order
    pub fn iter(&'a self) -> ViewIterator<'a, T> {
        ViewIterator::new(self)
    }
}

impl<'a, T: Clone> TensorView<'a, T> {
    /// Copy data from this view into a contiguous Vec
    pub fn to_vec(&self) -> Vec<T> {
        self.iter().cloned().collect()
    }
}

/// A mutable view into a tensor
#[derive(Debug)]
pub struct TensorViewMut<'a, T> {
    /// Pointer to the start of this view's data
    data: &'a mut [T],
    /// Offset from the start of the original data
    offset: usize,
    /// Shape of this view
    shape: Vec<usize>,
    /// Strides for this view (may differ from original)
    strides: Vec<usize>,
}

impl<'a, T> TensorViewMut<'a, T> {
    /// Create a mutable view of the entire tensor
    pub fn from_slice(data: &'a mut [T], shape: Vec<usize>) -> Self {
        let strides = compute_strides(&shape);
        Self {
            data,
            offset: 0,
            shape,
            strides,
        }
    }

    /// Get the shape of this view
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get the strides of this view
    pub fn strides(&self) -> &[usize] {
        &self.strides
    }

    /// Get the number of dimensions
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Get the total number of elements
    pub fn size(&self) -> usize {
        self.shape.iter().product()
    }

    /// Check if this view is contiguous in memory
    pub fn is_contiguous(&self) -> bool {
        let expected_strides = compute_strides(&self.shape);
        self.strides == expected_strides
    }

    /// Get an element at the given multi-dimensional index
    pub fn get(&self, indices: &[usize]) -> Option<&T> {
        let flat_index = self.compute_flat_index(indices)?;
        self.data.get(flat_index)
    }

    /// Get a mutable element at the given multi-dimensional index
    pub fn get_mut(&mut self, indices: &[usize]) -> Option<&mut T> {
        let flat_index = self.compute_flat_index(indices)?;
        self.data.get_mut(flat_index)
    }

    /// Set an element at the given multi-dimensional index
    pub fn set(&mut self, indices: &[usize], value: T) -> Option<()> {
        let flat_index = self.compute_flat_index(indices)?;
        if let Some(elem) = self.data.get_mut(flat_index) {
            *elem = value;
            Some(())
        } else {
            None
        }
    }

    /// Compute the flat index for multi-dimensional indices
    fn compute_flat_index(&self, indices: &[usize]) -> Option<usize> {
        if indices.len() != self.shape.len() {
            return None;
        }

        let mut flat_index = self.offset;
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

        if flat_index >= self.data.len() {
            return None;
        }

        Some(flat_index)
    }

    /// Get an immutable view of this mutable view
    pub fn as_view(&self) -> TensorView<'_, T> {
        TensorView {
            data: self.data,
            offset: self.offset,
            shape: self.shape.clone(),
            strides: self.strides.clone(),
            _marker: PhantomData,
        }
    }
}

impl<'a, T: Clone> TensorViewMut<'a, T> {
    /// Fill the view with a value
    pub fn fill(&mut self, value: T) {
        for indices in IndexIterator::new(&self.shape) {
            let _ = self.set(&indices, value.clone());
        }
    }

    /// Copy from another view (must have same shape)
    pub fn copy_from(&mut self, other: &TensorView<'_, T>) -> bool {
        if self.shape != other.shape {
            return false;
        }

        for indices in IndexIterator::new(&self.shape) {
            if let Some(value) = other.get(&indices) {
                let _ = self.set(&indices, value.clone());
            }
        }
        true
    }
}

impl<'a, T: Copy + core::ops::AddAssign> TensorViewMut<'a, T> {
    /// Add another view to this one in-place
    pub fn add_assign(&mut self, other: &TensorView<'_, T>) -> bool {
        if self.shape != other.shape {
            return false;
        }

        for indices in IndexIterator::new(&self.shape) {
            if let (Some(elem), Some(other_val)) = (self.get_mut(&indices), other.get(&indices)) {
                *elem += *other_val;
            }
        }
        true
    }
}

impl<'a, T: Copy + core::ops::MulAssign> TensorViewMut<'a, T> {
    /// Multiply this view by a scalar in-place
    pub fn mul_scalar(&mut self, scalar: T) {
        for indices in IndexIterator::new(&self.shape) {
            if let Some(elem) = self.get_mut(&indices) {
                *elem *= scalar;
            }
        }
    }
}

/// Iterator over multi-dimensional indices
pub struct IndexIterator {
    shape: Vec<usize>,
    indices: Vec<usize>,
    done: bool,
}

impl IndexIterator {
    pub fn new(shape: &[usize]) -> Self {
        let size: usize = shape.iter().product();
        Self {
            shape: shape.to_vec(),
            indices: alloc::vec![0; shape.len()],
            done: size == 0,
        }
    }
}

impl Iterator for IndexIterator {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let result = self.indices.clone();

        // Increment indices (row-major order)
        for i in (0..self.shape.len()).rev() {
            self.indices[i] += 1;
            if self.indices[i] < self.shape[i] {
                break;
            }
            if i == 0 {
                self.done = true;
            } else {
                self.indices[i] = 0;
            }
        }

        Some(result)
    }
}

/// Iterator over elements in a view
pub struct ViewIterator<'a, T> {
    view: &'a TensorView<'a, T>,
    index_iter: IndexIterator,
}

impl<'a, T> ViewIterator<'a, T> {
    pub fn new(view: &'a TensorView<'a, T>) -> Self {
        Self {
            view,
            index_iter: IndexIterator::new(&view.shape),
        }
    }
}

impl<'a, T> Iterator for ViewIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let indices = self.index_iter.next()?;
        self.view.get(&indices)
    }
}

/// Compute row-major strides for a shape
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_slice_range() {
        let sr = SliceRange::new(1, 5, 1);
        assert_eq!(sr.len(10), 4);

        let sr_step = SliceRange::new(0, 6, 2);
        assert_eq!(sr_step.len(10), 3); // 0, 2, 4

        let sr_all = SliceRange::all();
        assert_eq!(sr_all.len(10), 10);

        let sr_index = SliceRange::index(3);
        assert_eq!(sr_index.len(10), 1);
    }

    #[test]
    fn test_tensor_view_basic() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        assert_eq!(view.shape(), &[2, 3]);
        assert_eq!(view.ndim(), 2);
        assert_eq!(view.size(), 6);
        assert!(view.is_contiguous());
    }

    #[test]
    fn test_tensor_view_get() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        assert_eq!(view.get(&[0, 0]), Some(&1.0));
        assert_eq!(view.get(&[0, 2]), Some(&3.0));
        assert_eq!(view.get(&[1, 0]), Some(&4.0));
        assert_eq!(view.get(&[1, 2]), Some(&6.0));
        assert_eq!(view.get(&[2, 0]), None); // Out of bounds
    }

    #[test]
    fn test_tensor_view_slice() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        // Slice first row: [1, 2, 3]
        let row0 = view.slice(&[SliceRange::index(0), SliceRange::all()]);
        assert_eq!(row0.shape(), &[3]);
        assert_eq!(row0.get(&[0]), Some(&1.0));
        assert_eq!(row0.get(&[2]), Some(&3.0));

        // Slice column 1: [2, 5]
        let col1 = view.slice(&[SliceRange::all(), SliceRange::index(1)]);
        assert_eq!(col1.shape(), &[2]);
        assert_eq!(col1.get(&[0]), Some(&2.0));
        assert_eq!(col1.get(&[1]), Some(&5.0));
    }

    #[test]
    fn test_tensor_view_transpose() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        let transposed = view.transpose().unwrap();
        assert_eq!(transposed.shape(), &[3, 2]);

        // Original: [[1, 2, 3], [4, 5, 6]]
        // Transposed: [[1, 4], [2, 5], [3, 6]]
        assert_eq!(transposed.get(&[0, 0]), Some(&1.0));
        assert_eq!(transposed.get(&[0, 1]), Some(&4.0));
        assert_eq!(transposed.get(&[1, 0]), Some(&2.0));
        assert_eq!(transposed.get(&[2, 1]), Some(&6.0));
    }

    #[test]
    fn test_tensor_view_reshape() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        let reshaped = view.reshape(vec![3, 2]).unwrap();
        assert_eq!(reshaped.shape(), &[3, 2]);
        assert_eq!(reshaped.get(&[0, 0]), Some(&1.0));
        assert_eq!(reshaped.get(&[2, 1]), Some(&6.0));

        // Invalid reshape (wrong size)
        assert!(view.reshape(vec![4, 2]).is_none());
    }

    #[test]
    fn test_tensor_view_squeeze() {
        let data = vec![1.0, 2.0, 3.0];
        let view = TensorView::from_slice(&data, vec![1, 3, 1]);

        let squeezed = view.squeeze();
        assert_eq!(squeezed.shape(), &[3]);
        assert_eq!(squeezed.get(&[1]), Some(&2.0));
    }

    #[test]
    fn test_tensor_view_unsqueeze() {
        let data = vec![1.0, 2.0, 3.0];
        let view = TensorView::from_slice(&data, vec![3]);

        let unsqueezed = view.unsqueeze(0).unwrap();
        assert_eq!(unsqueezed.shape(), &[1, 3]);
        assert_eq!(unsqueezed.get(&[0, 1]), Some(&2.0));

        let unsqueezed2 = view.unsqueeze(1).unwrap();
        assert_eq!(unsqueezed2.shape(), &[3, 1]);
    }

    #[test]
    fn test_tensor_view_iter() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let view = TensorView::from_slice(&data, vec![2, 2]);

        let collected: Vec<f64> = view.iter().cloned().collect();
        assert_eq!(collected, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_tensor_view_to_vec() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        // Slice and convert to vec
        let sliced = view.slice(&[SliceRange::all(), SliceRange::range(0, 2)]);
        let vec = sliced.to_vec();
        assert_eq!(vec, vec![1.0, 2.0, 4.0, 5.0]);
    }

    #[test]
    fn test_tensor_view_mut_basic() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0];
        let mut view = TensorViewMut::from_slice(&mut data, vec![2, 2]);

        assert_eq!(view.get(&[0, 0]), Some(&1.0));
        view.set(&[0, 0], 10.0);
        assert_eq!(view.get(&[0, 0]), Some(&10.0));
    }

    #[test]
    fn test_tensor_view_mut_fill() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0];
        let mut view = TensorViewMut::from_slice(&mut data, vec![2, 2]);

        view.fill(0.0);
        assert_eq!(data, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_tensor_view_mut_copy_from() {
        let src_data = vec![1.0, 2.0, 3.0, 4.0];
        let src_view = TensorView::from_slice(&src_data, vec![2, 2]);

        let mut dst_data = vec![0.0, 0.0, 0.0, 0.0];
        let mut dst_view = TensorViewMut::from_slice(&mut dst_data, vec![2, 2]);

        assert!(dst_view.copy_from(&src_view));
        assert_eq!(dst_data, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_tensor_view_mut_add_assign() {
        let a_data = vec![1.0, 2.0, 3.0, 4.0];
        let a_view = TensorView::from_slice(&a_data, vec![2, 2]);

        let mut b_data = vec![10.0, 20.0, 30.0, 40.0];
        let mut b_view = TensorViewMut::from_slice(&mut b_data, vec![2, 2]);

        assert!(b_view.add_assign(&a_view));
        assert_eq!(b_data, vec![11.0, 22.0, 33.0, 44.0]);
    }

    #[test]
    fn test_tensor_view_mut_mul_scalar() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0];
        let mut view = TensorViewMut::from_slice(&mut data, vec![2, 2]);

        view.mul_scalar(2.0);
        assert_eq!(data, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_index_iterator() {
        let iter = IndexIterator::new(&[2, 3]);
        let indices: Vec<_> = iter.collect();

        assert_eq!(
            indices,
            vec![
                vec![0, 0],
                vec![0, 1],
                vec![0, 2],
                vec![1, 0],
                vec![1, 1],
                vec![1, 2],
            ]
        );
    }

    #[test]
    fn test_slice_with_step() {
        let data: Vec<f64> = (0..12).map(|x| x as f64).collect();
        let view = TensorView::from_slice(&data, vec![12]);

        // Select every 2nd element: [0, 2, 4, 6, 8, 10]
        let stepped = view.slice(&[SliceRange::new(0, 12, 2)]);
        assert_eq!(stepped.shape(), &[6]);
        let collected: Vec<f64> = stepped.iter().cloned().collect();
        assert_eq!(collected, vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    }

    #[test]
    fn test_swap_axes() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let view = TensorView::from_slice(&data, vec![2, 2, 2]);

        let swapped = view.swap_axes(0, 2).unwrap();
        assert_eq!(swapped.shape(), &[2, 2, 2]);

        // Verify the swap worked correctly
        assert_eq!(view.get(&[0, 0, 0]), swapped.get(&[0, 0, 0]));
        assert_eq!(view.get(&[1, 0, 0]), swapped.get(&[0, 0, 1]));
    }

    #[test]
    fn test_non_contiguous_view() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let view = TensorView::from_slice(&data, vec![2, 3]);

        // Transpose makes view non-contiguous
        let transposed = view.transpose().unwrap();
        assert!(!transposed.is_contiguous());

        // Non-contiguous view can't be reshaped
        assert!(transposed.reshape(vec![6]).is_none());
    }
}
