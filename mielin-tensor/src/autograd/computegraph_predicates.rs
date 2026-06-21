//! # `ComputeGraph` - predicates Methods
//!
//! This module contains method implementations for `ComputeGraph`.
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::tensor::Tensor;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use super::functions::{get_tangent, get_tangent2, matmul_tensors, tangent_or_zero};
use super::type_aliases::{GradFn, NodeId, TangentFn};
use super::types::{GraphNode, OpType, Variable};

use super::computegraph_type::ComputeGraph;

impl ComputeGraph {
    /// Create a new computational graph
    pub fn new() -> Self {
        Self {
            next_id: RefCell::new(0),
            gradients: Rc::new(RefCell::new(BTreeMap::new())),
            checkpointing_enabled: RefCell::new(false),
        }
    }
    /// Enable gradient checkpointing for memory-efficient training
    ///
    /// When enabled, intermediate activations can be cleared and
    /// recomputed during backward pass, trading compute for memory.
    pub fn enable_checkpointing(&self) {
        *self.checkpointing_enabled.borrow_mut() = true;
    }
    /// Disable gradient checkpointing
    pub fn disable_checkpointing(&self) {
        *self.checkpointing_enabled.borrow_mut() = false;
    }
    /// Check if checkpointing is enabled
    pub fn is_checkpointing_enabled(&self) -> bool {
        *self.checkpointing_enabled.borrow()
    }
    /// Allocate a new node ID
    fn next_id(&self) -> NodeId {
        let id = *self.next_id.borrow();
        *self.next_id.borrow_mut() += 1;
        id
    }
    /// Create a new leaf variable
    pub fn variable(&self, tensor: Tensor<f32>, requires_grad: bool) -> Variable {
        let mut node = GraphNode::new_leaf(tensor, requires_grad);
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    fn propagate_tangents(&self, root: &Variable) {
        let mut visited = BTreeSet::new();
        let mut topo_order: alloc::vec::Vec<Rc<RefCell<GraphNode>>> = alloc::vec::Vec::new();
        let mut stack = alloc::vec![Rc::clone(&root.node)];
        while let Some(node_rc) = stack.pop() {
            let id = node_rc.borrow().id;
            if visited.contains(&id) {
                continue;
            }
            let inputs: alloc::vec::Vec<_> = node_rc
                .borrow()
                .inputs
                .iter()
                .filter_map(|w| w.upgrade())
                .collect();
            let all_visited = inputs.iter().all(|inp| visited.contains(&inp.borrow().id));
            if all_visited {
                visited.insert(id);
                topo_order.push(Rc::clone(&node_rc));
            } else {
                stack.push(Rc::clone(&node_rc));
                for inp in inputs {
                    if !visited.contains(&inp.borrow().id) {
                        stack.push(inp);
                    }
                }
            }
        }
        for node_rc in &topo_order {
            let tangent_fn = node_rc.borrow().tangent_fn.clone();
            if let Some(f) = tangent_fn {
                let (t1, t2) = f();
                node_rc.borrow_mut().tangent = t1;
                node_rc.borrow_mut().tangent2 = t2;
            }
        }
    }
    /// Compute the Jacobian-vector product (JVP) for forward-mode AD
    ///
    /// Given function f: R^n -> R^m and tangent vector v in R^n,
    /// computes df/dx * v efficiently in forward mode.
    pub fn jvp(&self, f: &Variable, x: &Variable, v: &Tensor<f32>) -> Tensor<f32> {
        x.set_tangent(v.clone());
        self.propagate_tangents(f);
        f.tangent()
            .unwrap_or_else(|| Tensor::zeros(f.data().shape().to_vec()))
    }
    /// Compute second derivative using backward-over-backward
    ///
    /// Given scalar function f: R -> R, computes d²f/dx²
    pub fn second_derivative(&self, f: &Variable, x: &Variable) -> Option<f32> {
        x.set_tangent(Tensor::scalar(1.0));
        x.set_tangent2(Tensor::scalar(0.0));
        self.propagate_tangents(f);
        f.tangent2().map(|t| t.data()[0])
    }
    /// Add two variables: z = x + y
    /// Gradient: dL/dx = dL/dz, dL/dy = dL/dz
    pub fn add(&self, x: &Variable, y: &Variable) -> Variable {
        let x_data = x.data();
        let y_data = y.data();
        let z_data = x_data.add(&y_data);
        let grad_fn: GradFn = Rc::new(|grad: &Tensor<f32>| alloc::vec![grad.clone(), grad.clone()]);
        let x_weak = Rc::downgrade(&x.node);
        let y_weak = Rc::downgrade(&y.node);
        let z_shape = z_data.shape().to_vec();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let yd = get_tangent(&y_weak);
            let xd2 = get_tangent2(&x_weak);
            let yd2 = get_tangent2(&y_weak);
            let t1 = {
                let a = tangent_or_zero(xd, &z_shape);
                let b = tangent_or_zero(yd, &z_shape);
                Some(a.add(&b))
            };
            let t2 = {
                let a = tangent_or_zero(xd2, &z_shape);
                let b = tangent_or_zero(yd2, &z_shape);
                Some(a.add(&b))
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Add,
            alloc::vec![Rc::downgrade(&x.node), Rc::downgrade(&y.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Subtract two variables: z = x - y
    /// Gradient: dL/dx = dL/dz, dL/dy = -dL/dz
    pub fn sub(&self, x: &Variable, y: &Variable) -> Variable {
        let x_data = x.data();
        let y_data = y.data();
        let z_data = x_data.sub(&y_data);
        let grad_fn: GradFn = Rc::new(|grad: &Tensor<f32>| {
            let neg_grad = grad.scale(-1.0);
            alloc::vec![grad.clone(), neg_grad]
        });
        let x_weak = Rc::downgrade(&x.node);
        let y_weak = Rc::downgrade(&y.node);
        let z_shape = z_data.shape().to_vec();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let yd = get_tangent(&y_weak);
            let xd2 = get_tangent2(&x_weak);
            let yd2 = get_tangent2(&y_weak);
            let t1 = {
                let a = tangent_or_zero(xd, &z_shape);
                let b = tangent_or_zero(yd, &z_shape);
                Some(a.sub(&b))
            };
            let t2 = {
                let a = tangent_or_zero(xd2, &z_shape);
                let b = tangent_or_zero(yd2, &z_shape);
                Some(a.sub(&b))
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Sub,
            alloc::vec![Rc::downgrade(&x.node), Rc::downgrade(&y.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Multiply two variables element-wise: z = x * y
    /// Gradient: dL/dx = dL/dz * y, dL/dy = dL/dz * x
    pub fn mul(&self, x: &Variable, y: &Variable) -> Variable {
        let x_data = x.data();
        let y_data = y.data();
        let z_data = x_data.mul(&y_data);
        let x_data_clone = x_data.clone();
        let y_data_clone = y_data.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let grad_x = grad.mul(&y_data_clone);
            let grad_y = grad.mul(&x_data_clone);
            alloc::vec![grad_x, grad_y]
        });
        let x_weak_t = Rc::downgrade(&x.node);
        let y_weak_t = Rc::downgrade(&y.node);
        let x_data_t = x_data.clone();
        let y_data_t = y_data.clone();
        let z_shape = z_data.shape().to_vec();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak_t);
            let yd = get_tangent(&y_weak_t);
            let xd2 = get_tangent2(&x_weak_t);
            let yd2 = get_tangent2(&y_weak_t);
            let xd_z = tangent_or_zero(xd.clone(), &z_shape);
            let yd_z = tangent_or_zero(yd.clone(), &z_shape);
            let xd2_z = tangent_or_zero(xd2, &z_shape);
            let yd2_z = tangent_or_zero(yd2, &z_shape);
            let t1 = Some(xd_z.mul(&y_data_t).add(&x_data_t.mul(&yd_z)));
            let cross = xd
                .as_ref()
                .map(|a| a.mul(yd.as_ref().unwrap_or(&Tensor::zeros(z_shape.clone()))))
                .unwrap_or_else(|| Tensor::zeros(z_shape.clone()));
            let t2 = Some(
                xd2_z
                    .mul(&y_data_t)
                    .add(&cross.scale(2.0))
                    .add(&x_data_t.mul(&yd2_z)),
            );
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Mul,
            alloc::vec![Rc::downgrade(&x.node), Rc::downgrade(&y.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Sum reduction: z = sum(x)
    /// Gradient: dL/dx = dL/dz * ones_like(x)
    pub fn sum(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let sum_val = x_data.data().iter().sum::<f32>();
        let z_data = Tensor::scalar(sum_val);
        let x_shape = x_data.shape().to_vec();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let grad_val = grad.data()[0];
            let grad_x = Tensor::filled(x_shape.clone(), grad_val);
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let t1 = xd.map(|t| {
                let s: f32 = t.data().iter().sum();
                Tensor::scalar(s)
            });
            let t2 = xd2.map(|t| {
                let s: f32 = t.data().iter().sum();
                Tensor::scalar(s)
            });
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Sum,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Mean reduction: z = mean(x)
    /// Gradient: dL/dx = dL/dz / numel(x)
    pub fn mean(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let numel = x_data.data().len() as f32;
        let mean_val = x_data.data().iter().sum::<f32>() / numel;
        let z_data = Tensor::scalar(mean_val);
        let x_shape = x_data.shape().to_vec();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let numel = x_shape.iter().product::<usize>() as f32;
            let grad_val = grad.data()[0] / numel;
            let grad_x = Tensor::filled(x_shape.clone(), grad_val);
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let x_numel = x_data.data().len() as f32;
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let t1 = xd.map(|t| {
                let s: f32 = t.data().iter().sum::<f32>() / x_numel;
                Tensor::scalar(s)
            });
            let t2 = xd2.map(|t| {
                let s: f32 = t.data().iter().sum::<f32>() / x_numel;
                Tensor::scalar(s)
            });
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Mean,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// ReLU activation: z = max(0, x)
    /// Gradient: dL/dx = dL/dz if x > 0 else 0
    pub fn relu(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = val.max(0.0);
        }
        let mask: Vec<f32> = x_data
            .data()
            .iter()
            .map(|&v| if v > 0.0 { 1.0 } else { 0.0 })
            .collect();
        let mask_t = mask.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                *val *= mask[i];
            }
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let apply_mask = |t: Tensor<f32>| {
                let mut r = t.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    *v *= mask_t[i];
                }
                r
            };
            let t1 = xd.map(apply_mask);
            let apply_mask2 = |t: Tensor<f32>| {
                let mut r = t.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    *v *= mask_t[i];
                }
                r
            };
            let t2 = xd2.map(apply_mask2);
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::ReLU,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Sigmoid activation: z = 1 / (1 + exp(-x))
    /// Gradient: dL/dx = dL/dz * z * (1 - z)
    pub fn sigmoid(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = 1.0 / (1.0 + libm::expf(-*val));
        }
        let z_clone = z_data.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                let z = z_clone.data()[i];
                *val *= z * (1.0 - z);
            }
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let z_sig = z_data.clone();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let z_ref = &z_sig;
            let t1 = xd.as_ref().map(|xdt| {
                let mut r = xdt.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    let z = z_ref.data()[i];
                    *v *= z * (1.0 - z);
                }
                r
            });
            let t2 = {
                let xd_ref = xd.as_ref();
                let xd2_ref = xd2.as_ref();
                if xd_ref.is_none() && xd2_ref.is_none() {
                    None
                } else {
                    let shape = z_ref.shape().to_vec();
                    let xd_z = tangent_or_zero(xd.clone(), &shape);
                    let xd2_z = tangent_or_zero(xd2.clone(), &shape);
                    let mut r = Tensor::zeros(shape.clone());
                    for (i, v) in r.data_mut().iter_mut().enumerate() {
                        let z = z_ref.data()[i];
                        let d1 = xd_z.data()[i];
                        let d2 = xd2_z.data()[i];
                        *v = d2 * z * (1.0 - z) + d1 * d1 * z * (1.0 - z) * (1.0 - 2.0 * z);
                    }
                    Some(r)
                }
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Sigmoid,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Tanh activation: z = tanh(x)
    /// Gradient: dL/dx = dL/dz * (1 - z^2)
    pub fn tanh(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::tanhf(*val);
        }
        let z_clone = z_data.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                let z = z_clone.data()[i];
                *val *= 1.0 - z * z;
            }
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let z_tanh = z_data.clone();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let z_ref = &z_tanh;
            let t1 = xd.as_ref().map(|xdt| {
                let mut r = xdt.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    let z = z_ref.data()[i];
                    *v *= 1.0 - z * z;
                }
                r
            });
            let t2 = {
                if xd.is_none() && xd2.is_none() {
                    None
                } else {
                    let shape = z_ref.shape().to_vec();
                    let xd_z = tangent_or_zero(xd.clone(), &shape);
                    let xd2_z = tangent_or_zero(xd2.clone(), &shape);
                    let mut r = Tensor::zeros(shape.clone());
                    for (i, v) in r.data_mut().iter_mut().enumerate() {
                        let z = z_ref.data()[i];
                        let d1 = xd_z.data()[i];
                        let d2 = xd2_z.data()[i];
                        *v = d2 * (1.0 - z * z) + d1 * d1 * (-2.0 * z * (1.0 - z * z));
                    }
                    Some(r)
                }
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Tanh,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Power operation: z = x^n
    /// Gradient: dL/dx = dL/dz * n * x^(n-1)
    pub fn pow(&self, x: &Variable, n: f32) -> Variable {
        let x_data = x.data();
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::powf(*val, n);
        }
        let x_clone = x_data.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                let x = x_clone.data()[i];
                *val *= n * libm::powf(x, n - 1.0);
            }
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let x_data_t = x_data.clone();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let t1 = xd.as_ref().map(|xdt| {
                let mut r = xdt.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    let x = x_data_t.data()[i];
                    *v *= n * libm::powf(x, n - 1.0);
                }
                r
            });
            let t2 = {
                if xd.is_none() && xd2.is_none() {
                    None
                } else {
                    let shape = x_data_t.shape().to_vec();
                    let xd_z = tangent_or_zero(xd.clone(), &shape);
                    let xd2_z = tangent_or_zero(xd2.clone(), &shape);
                    let mut r = Tensor::zeros(shape.clone());
                    for (i, v) in r.data_mut().iter_mut().enumerate() {
                        let x = x_data_t.data()[i];
                        let d1 = xd_z.data()[i];
                        let d2 = xd2_z.data()[i];
                        *v = d2 * n * libm::powf(x, n - 1.0)
                            + d1 * d1 * n * (n - 1.0) * libm::powf(x, n - 2.0);
                    }
                    Some(r)
                }
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Pow,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Exponential: z = exp(x)
    /// Gradient: dL/dx = dL/dz * exp(x) = dL/dz * z
    pub fn exp(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::expf(*val);
        }
        let z_clone = z_data.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                *val *= z_clone.data()[i];
            }
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let z_exp = z_data.clone();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let z_ref = &z_exp;
            let t1 = xd.as_ref().map(|xdt| {
                let mut r = xdt.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    *v *= z_ref.data()[i];
                }
                r
            });
            let t2 = {
                if xd.is_none() && xd2.is_none() {
                    None
                } else {
                    let shape = z_ref.shape().to_vec();
                    let xd_z = tangent_or_zero(xd.clone(), &shape);
                    let xd2_z = tangent_or_zero(xd2.clone(), &shape);
                    let mut r = Tensor::zeros(shape);
                    for (i, v) in r.data_mut().iter_mut().enumerate() {
                        let z = z_ref.data()[i];
                        let d1 = xd_z.data()[i];
                        let d2 = xd2_z.data()[i];
                        *v = (d2 + d1 * d1) * z;
                    }
                    Some(r)
                }
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Exp,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Natural logarithm: z = log(x)
    /// Gradient: dL/dx = dL/dz / x
    pub fn log(&self, x: &Variable) -> Variable {
        let x_data = x.data();
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::logf(*val);
        }
        let x_clone = x_data.clone();
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                *val /= x_clone.data()[i];
            }
            alloc::vec![grad_x]
        });
        let x_weak = Rc::downgrade(&x.node);
        let x_data_log = x_data.clone();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let xd = get_tangent(&x_weak);
            let xd2 = get_tangent2(&x_weak);
            let t1 = xd.as_ref().map(|xdt| {
                let mut r = xdt.clone();
                for (i, v) in r.data_mut().iter_mut().enumerate() {
                    let x = x_data_log.data()[i];
                    *v /= x;
                }
                r
            });
            let t2 = {
                if xd.is_none() && xd2.is_none() {
                    None
                } else {
                    let shape = x_data_log.shape().to_vec();
                    let xd_z = tangent_or_zero(xd.clone(), &shape);
                    let xd2_z = tangent_or_zero(xd2.clone(), &shape);
                    let inv_x: alloc::vec::Vec<f32> =
                        x_data_log.data().iter().map(|&v| 1.0 / v).collect();
                    let mut r = Tensor::zeros(shape);
                    for (i, v) in r.data_mut().iter_mut().enumerate() {
                        let d1 = xd_z.data()[i];
                        let d2 = xd2_z.data()[i];
                        *v = d2 * inv_x[i] - d1 * d1 * inv_x[i] * inv_x[i];
                    }
                    Some(r)
                }
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::Log,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
    /// Matrix multiplication: C = A @ B
    ///
    /// Supports:
    /// - Matrix-vector: `[m, n] @ [n] -> [m]`
    /// - Matrix-matrix: `[m, n] @ [n, p] -> [m, p]`
    pub fn matmul(&self, a: &Variable, b: &Variable) -> Variable {
        let a_data = a.data();
        let b_data = b.data();
        let a_shape = a_data.shape();
        let b_shape = b_data.shape();
        let (m, n, k, is_matvec) = if a_shape.len() == 2 && b_shape.len() == 1 {
            assert_eq!(
                a_shape[1], b_shape[0],
                "Matrix-vector dimension mismatch: [{}, {}] @ [{}]",
                a_shape[0], a_shape[1], b_shape[0]
            );
            (a_shape[0], a_shape[1], 1, true)
        } else if a_shape.len() == 2 && b_shape.len() == 2 {
            assert_eq!(
                a_shape[1], b_shape[0],
                "Matrix-matrix dimension mismatch: [{}, {}] @ [{}, {}]",
                a_shape[0], a_shape[1], b_shape[0], b_shape[1]
            );
            (a_shape[0], a_shape[1], b_shape[1], false)
        } else {
            panic!("Unsupported matmul shapes: {:?} @ {:?}", a_shape, b_shape);
        };
        let mut c_data = if is_matvec {
            alloc::vec![0.0f32; m]
        } else {
            alloc::vec![0.0f32; m * k]
        };
        if is_matvec {
            #[allow(clippy::needless_range_loop)]
            for i in 0..m {
                let mut sum = 0.0;
                for j in 0..n {
                    sum += a_data.get(&[i, j]).expect("i < m and j < n within bounds")
                        * b_data.data()[j];
                }
                c_data[i] = sum;
            }
        } else {
            for i in 0..m {
                for j in 0..k {
                    let mut sum = 0.0;
                    for idx in 0..n {
                        sum += a_data
                            .get(&[i, idx])
                            .expect("i < m and idx < n within bounds")
                            * b_data
                                .get(&[idx, j])
                                .expect("idx < n and j < k within bounds");
                    }
                    c_data[i * k + j] = sum;
                }
            }
        }
        let c_tensor = if is_matvec {
            Tensor::vector(c_data)
        } else {
            Tensor::from_vec(c_data, alloc::vec![m, k])
                .expect("c_data length = m * k matches shape")
        };
        let a_clone = a_data.clone();
        let b_clone = b_data.clone();
        let m_clone = m;
        let n_clone = n;
        let k_clone = k;
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grads = alloc::vec![];
            if is_matvec {
                let mut grad_a = alloc::vec![0.0f32; m_clone * n_clone];
                for i in 0..m_clone {
                    for j in 0..n_clone {
                        grad_a[i * n_clone + j] = grad.data()[i] * b_clone.data()[j];
                    }
                }
                grads.push(
                    Tensor::from_vec(grad_a, alloc::vec![m_clone, n_clone])
                        .expect("grad_a length = m*n matches shape"),
                );
                let mut grad_b = alloc::vec![0.0f32; n_clone];
                #[allow(clippy::needless_range_loop)]
                for j in 0..n_clone {
                    let mut sum = 0.0;
                    for i in 0..m_clone {
                        sum += a_clone.get(&[i, j]).expect("i < m and j < n within bounds")
                            * grad.data()[i];
                    }
                    grad_b[j] = sum;
                }
                grads.push(Tensor::vector(grad_b));
            } else {
                let mut grad_a = alloc::vec![0.0f32; m_clone * n_clone];
                for i in 0..m_clone {
                    for j in 0..n_clone {
                        let mut sum = 0.0;
                        for idx in 0..k_clone {
                            sum += grad
                                .get(&[i, idx])
                                .expect("i < m and idx < k within bounds")
                                * b_clone
                                    .get(&[j, idx])
                                    .expect("j < n and idx < k within bounds");
                        }
                        grad_a[i * n_clone + j] = sum;
                    }
                }
                grads.push(
                    Tensor::from_vec(grad_a, alloc::vec![m_clone, n_clone])
                        .expect("grad_a length = m*n matches shape"),
                );
                let mut grad_b = alloc::vec![0.0f32; n_clone * k_clone];
                for i in 0..n_clone {
                    for j in 0..k_clone {
                        let mut sum = 0.0;
                        for idx in 0..m_clone {
                            sum += a_clone
                                .get(&[idx, i])
                                .expect("idx < m and i < n within bounds")
                                * grad
                                    .get(&[idx, j])
                                    .expect("idx < m and j < k within bounds");
                        }
                        grad_b[i * k_clone + j] = sum;
                    }
                }
                grads.push(
                    Tensor::from_vec(grad_b, alloc::vec![n_clone, k_clone])
                        .expect("grad_b length = n*k matches shape"),
                );
            }
            grads
        });
        let a_weak_t = Rc::downgrade(&a.node);
        let b_weak_t = Rc::downgrade(&b.node);
        let a_data_t = a_data.clone();
        let b_data_t = b_data.clone();
        let c_shape = c_tensor.shape().to_vec();
        let tangent_fn: Option<TangentFn> = Some(Rc::new(move || {
            let ad = get_tangent(&a_weak_t);
            let bd = get_tangent(&b_weak_t);
            let ad2 = get_tangent2(&a_weak_t);
            let bd2 = get_tangent2(&b_weak_t);
            let t1 = {
                let part_a = ad.as_ref().map(|t| matmul_tensors(t, &b_data_t));
                let part_b = bd.as_ref().map(|t| matmul_tensors(&a_data_t, t));
                match (part_a, part_b) {
                    (Some(pa), Some(pb)) => Some(pa.add(&pb)),
                    (Some(pa), None) => Some(pa),
                    (None, Some(pb)) => Some(pb),
                    (None, None) => None,
                }
            };
            let t2 = {
                let part_a = ad2.as_ref().map(|t| matmul_tensors(t, &b_data_t));
                let part_cross = ad
                    .as_ref()
                    .zip(bd.as_ref())
                    .map(|(at, bt)| matmul_tensors(at, bt).scale(2.0));
                let part_b = bd2.as_ref().map(|t| matmul_tensors(&a_data_t, t));
                match (part_a, part_cross, part_b) {
                    (None, None, None) => None,
                    (a, b, c) => {
                        let zero = Tensor::zeros(c_shape.clone());
                        let s = a
                            .unwrap_or_else(|| zero.clone())
                            .add(&b.unwrap_or_else(|| zero.clone()))
                            .add(&c.unwrap_or_else(|| zero.clone()));
                        Some(s)
                    }
                }
            };
            (t1, t2)
        }));
        let mut node = GraphNode::new_op(
            OpType::MatMul,
            alloc::vec![Rc::downgrade(&a.node), Rc::downgrade(&b.node)],
            c_tensor,
            Some(grad_fn),
            tangent_fn,
        );
        node.id = self.next_id();
        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
}
