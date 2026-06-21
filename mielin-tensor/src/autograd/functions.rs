//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::tensor::Tensor;
use alloc::rc::Weak;
use core::cell::RefCell;
use core::ops::{Add, Mul, Sub};

use super::computegraph_type::ComputeGraph;
use super::types::{GraphNode, Variable};

extern crate alloc;
/// Get the first-order tangent from a weak reference
pub(super) fn get_tangent(w: &Weak<RefCell<GraphNode>>) -> Option<Tensor<f32>> {
    w.upgrade().and_then(|rc| rc.borrow().tangent.clone())
}
/// Get the second-order tangent from a weak reference
pub(super) fn get_tangent2(w: &Weak<RefCell<GraphNode>>) -> Option<Tensor<f32>> {
    w.upgrade().and_then(|rc| rc.borrow().tangent2.clone())
}
/// Get the shape from a weak reference
#[allow(dead_code)]
fn get_shape_from_weak(w: &Weak<RefCell<GraphNode>>) -> alloc::vec::Vec<usize> {
    w.upgrade()
        .map(|rc| rc.borrow().value.shape().to_vec())
        .unwrap_or_default()
}
/// Return tangent or zeros of the given shape
pub(super) fn tangent_or_zero(t: Option<Tensor<f32>>, shape: &[usize]) -> Tensor<f32> {
    t.unwrap_or_else(|| Tensor::zeros(shape.to_vec()))
}
/// Perform matrix multiplication of two tensors (2D@1D or 2D@2D)
pub(super) fn matmul_tensors(a: &Tensor<f32>, b: &Tensor<f32>) -> Tensor<f32> {
    let a_shape = a.shape();
    let b_shape = b.shape();
    if a_shape.len() == 2 && b_shape.len() == 1 {
        let m = a_shape[0];
        let n = a_shape[1];
        let mut c = alloc::vec![0.0f32; m];
        #[allow(clippy::needless_range_loop)]
        for i in 0..m {
            for j in 0..n {
                c[i] += a.get(&[i, j]).copied().unwrap_or(0.0) * b.data()[j];
            }
        }
        Tensor::vector(c)
    } else if a_shape.len() == 2 && b_shape.len() == 2 {
        let m = a_shape[0];
        let n = a_shape[1];
        let k = b_shape[1];
        let mut c = alloc::vec![0.0f32; m * k];
        for i in 0..m {
            for j in 0..k {
                for idx in 0..n {
                    c[i * k + j] += a.get(&[i, idx]).copied().unwrap_or(0.0)
                        * b.get(&[idx, j]).copied().unwrap_or(0.0);
                }
            }
        }
        Tensor::from_vec(c, alloc::vec![m, k]).expect("valid shape")
    } else {
        panic!(
            "matmul_tensors: unsupported shapes {:?} @ {:?}",
            a_shape, b_shape
        );
    }
}
impl Add for &Variable {
    type Output = Variable;
    fn add(self, other: &Variable) -> Variable {
        let graph = ComputeGraph::new();
        graph.add(self, other)
    }
}
impl Sub for &Variable {
    type Output = Variable;
    fn sub(self, other: &Variable) -> Variable {
        let graph = ComputeGraph::new();
        graph.sub(self, other)
    }
}
impl Mul for &Variable {
    type Output = Variable;
    fn mul(self, other: &Variable) -> Variable {
        let graph = ComputeGraph::new();
        graph.mul(self, other)
    }
}
