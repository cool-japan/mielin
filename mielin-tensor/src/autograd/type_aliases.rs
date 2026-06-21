//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::tensor::Tensor;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

/// Unique identifier for computational graph nodes
pub(super) type NodeId = usize;
/// Gradient function that computes gradients for backward pass
pub(super) type GradFn = Rc<dyn Fn(&Tensor<f32>) -> Vec<Tensor<f32>>>;
/// Tangent propagation function for forward-mode AD
/// Returns (first_order_tangent, second_order_tangent)
pub(super) type TangentFn = Rc<dyn Fn() -> (Option<Tensor<f32>>, Option<Tensor<f32>>)>;
/// Gradient storage shared between variables
pub(super) type GradientStorage = Rc<RefCell<BTreeMap<NodeId, Tensor<f32>>>>;
