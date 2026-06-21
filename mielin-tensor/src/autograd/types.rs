//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::tensor::Tensor;
use alloc::collections::BTreeMap;
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::RefCell;

use super::type_aliases::{GradFn, GradientStorage, NodeId, TangentFn};

/// Operation types for gradient computation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpType {
    /// Leaf node (input variable)
    Leaf,
    /// Addition: z = x + y
    Add,
    /// Subtraction: z = x - y
    Sub,
    /// Multiplication: z = x * y (element-wise)
    Mul,
    /// Matrix multiplication: z = x @ y
    MatMul,
    /// Division: z = x / y
    Div,
    /// Power: z = x^n
    Pow,
    /// Exponential: z = exp(x)
    Exp,
    /// Logarithm: z = log(x)
    Log,
    /// Sum reduction: z = sum(x)
    Sum,
    /// Mean reduction: z = mean(x)
    Mean,
    /// ReLU activation: z = max(0, x)
    ReLU,
    /// Sigmoid activation: z = 1 / (1 + exp(-x))
    Sigmoid,
    /// Tanh activation: z = tanh(x)
    Tanh,
    /// Reshape operation: z = reshape(x)
    Reshape,
    /// Transpose operation: z = transpose(x)
    Transpose,
    /// Convolution: z = conv(x, w)
    Conv2D,
    /// Max pooling: z = maxpool(x)
    MaxPool2D,
    /// Batch normalization: z = batchnorm(x)
    BatchNorm,
}
/// Node in the computational graph
pub(super) struct GraphNode {
    /// Unique identifier
    pub(super) id: NodeId,
    /// Operation type
    #[allow(dead_code)]
    pub(super) op: OpType,
    /// Input nodes (parents in the graph)
    pub(super) inputs: Vec<Weak<RefCell<GraphNode>>>,
    /// Output tensor value
    pub(super) value: Tensor<f32>,
    /// Function to compute gradients for inputs
    pub(super) grad_fn: Option<GradFn>,
    /// Whether this node requires gradient computation
    pub(super) requires_grad: bool,
    /// Checkpoint flag for gradient checkpointing
    pub(super) checkpoint: bool,
    /// First-order tangent for forward-mode AD
    pub(super) tangent: Option<Tensor<f32>>,
    /// Second-order tangent (hyper-dual) for forward-mode AD
    pub(super) tangent2: Option<Tensor<f32>>,
    /// Tangent propagation function
    pub(super) tangent_fn: Option<TangentFn>,
}
impl GraphNode {
    /// Create a new leaf node (input variable)
    pub(super) fn new_leaf(value: Tensor<f32>, requires_grad: bool) -> Self {
        Self {
            id: 0,
            op: OpType::Leaf,
            inputs: Vec::new(),
            value,
            grad_fn: None,
            requires_grad,
            checkpoint: false,
            tangent: None,
            tangent2: None,
            tangent_fn: None,
        }
    }
    /// Create a new operation node
    pub(super) fn new_op(
        op: OpType,
        inputs: Vec<Weak<RefCell<GraphNode>>>,
        value: Tensor<f32>,
        grad_fn: Option<GradFn>,
        tangent_fn: Option<TangentFn>,
    ) -> Self {
        Self {
            id: 0,
            op,
            inputs,
            value,
            grad_fn,
            requires_grad: true,
            checkpoint: false,
            tangent: None,
            tangent2: None,
            tangent_fn,
        }
    }
}
/// Variable: Tensor wrapper with gradient tracking
#[derive(Clone)]
pub struct Variable {
    /// Shared reference to the computational graph node
    pub(super) node: Rc<RefCell<GraphNode>>,
    /// Shared gradient storage
    pub(super) gradients: GradientStorage,
}
impl Variable {
    /// Create a new variable from a tensor
    pub fn new(tensor: Tensor<f32>, requires_grad: bool) -> Self {
        let node = GraphNode::new_leaf(tensor, requires_grad);
        Self {
            node: Rc::new(RefCell::new(node)),
            gradients: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }
    /// Create a variable from an existing graph node with shared gradient storage
    pub(super) fn from_node(node: Rc<RefCell<GraphNode>>, gradients: GradientStorage) -> Self {
        Self { node, gradients }
    }
    /// Get the tensor value
    pub fn data(&self) -> Tensor<f32> {
        self.node.borrow().value.clone()
    }
    /// Get the gradient (if computed)
    pub fn grad(&self) -> Option<Tensor<f32>> {
        let node_id = self.node.borrow().id;
        self.gradients.borrow().get(&node_id).cloned()
    }
    /// Get the gradient as a Variable for higher-order derivatives
    ///
    /// This allows computing gradients of gradients (second derivatives, etc.)
    /// by making the gradient itself differentiable.
    pub fn grad_var(&self, requires_grad: bool) -> Option<Variable> {
        self.grad().map(|g| Variable::new(g, requires_grad))
    }
    /// Compute the gradient and return it as a Variable for chaining
    ///
    /// This is a convenience method for computing higher-order derivatives.
    /// It performs backward pass and returns the gradient as a new Variable.
    pub fn grad_and_detach(&self) -> Option<Variable> {
        self.grad().map(|g| Variable::new(g, false))
    }
    pub fn tangent(&self) -> Option<Tensor<f32>> {
        self.node.borrow().tangent.clone()
    }
    pub fn tangent2(&self) -> Option<Tensor<f32>> {
        self.node.borrow().tangent2.clone()
    }
    pub fn set_tangent(&self, t: Tensor<f32>) {
        self.node.borrow_mut().tangent = Some(t);
    }
    pub fn set_tangent2(&self, t: Tensor<f32>) {
        self.node.borrow_mut().tangent2 = Some(t);
    }
    /// Set whether this variable requires gradient
    pub fn set_requires_grad(&mut self, requires_grad: bool) {
        self.node.borrow_mut().requires_grad = requires_grad;
    }
    /// Check if this variable requires gradient
    pub fn requires_grad(&self) -> bool {
        self.node.borrow().requires_grad
    }
    /// Zero the gradient
    pub fn zero_grad(&mut self) {
        let node_id = self.node.borrow().id;
        self.gradients.borrow_mut().remove(&node_id);
    }
    /// Mark this node as a checkpoint for gradient checkpointing
    ///
    /// Gradient checkpointing trades compute for memory by not storing
    /// intermediate activations. During backward pass, these values are
    /// recomputed from the last checkpoint.
    pub fn checkpoint(&mut self) {
        self.node.borrow_mut().checkpoint = true;
    }
    /// Check if this node is marked as a checkpoint
    pub fn is_checkpoint(&self) -> bool {
        self.node.borrow().checkpoint
    }
    /// Clear intermediate values for memory efficiency
    /// Use this after marking checkpoints to free memory
    pub fn clear_cache(&mut self) {
        if self.is_checkpoint() {
            let shape = self.node.borrow().value.shape().to_vec();
            self.node.borrow_mut().value = Tensor::zeros(shape);
        }
    }
    /// Perform backward pass from this variable
    pub fn backward(&self) {
        let value = &self.node.borrow().value;
        let grad = Tensor::ones(value.shape().to_vec());
        self.backward_impl(grad);
    }
    /// Backward pass with custom gradient
    pub fn backward_with_grad(&self, grad: Tensor<f32>) {
        self.backward_impl(grad);
    }
    /// Implementation of backward pass using iterative topological sort
    pub(super) fn backward_impl(&self, grad: Tensor<f32>) {
        let mut topo_order: Vec<Rc<RefCell<GraphNode>>> = Vec::new();
        let mut visited = Vec::new();
        let mut stack = alloc::vec![self.node.clone()];
        while let Some(node_rc) = stack.pop() {
            let (node_id, inputs, already_visited) = {
                let node = node_rc.borrow();
                let id = node.id;
                let already_visited = visited.contains(&id);
                (id, node.inputs.clone(), already_visited)
            };
            if already_visited {
                continue;
            }
            let mut all_inputs_visited = true;
            let mut unvisited_inputs = Vec::new();
            for input_weak in inputs.iter() {
                if let Some(input_rc) = input_weak.upgrade() {
                    let input_id = input_rc.borrow().id;
                    if !visited.contains(&input_id) {
                        all_inputs_visited = false;
                        unvisited_inputs.push(input_rc.clone());
                    }
                }
            }
            if all_inputs_visited {
                visited.push(node_id);
                topo_order.push(node_rc.clone());
            } else {
                stack.push(node_rc.clone());
                for input_rc in unvisited_inputs {
                    stack.push(input_rc);
                }
            }
        }
        let output_node_id = self.node.borrow().id;
        self.gradients.borrow_mut().insert(output_node_id, grad);
        for node_rc in topo_order.iter().rev() {
            let (node_id, requires_grad, grad_fn_opt, inputs_weak) = {
                let node = node_rc.borrow();
                (
                    node.id,
                    node.requires_grad,
                    node.grad_fn.clone(),
                    node.inputs.clone(),
                )
            };
            if !requires_grad {
                continue;
            }
            let grad = if let Some(g) = self.gradients.borrow().get(&node_id) {
                g.clone()
            } else {
                continue;
            };
            if let Some(grad_fn) = grad_fn_opt {
                let input_grads = grad_fn(&grad);
                for (i, input_weak) in inputs_weak.iter().enumerate() {
                    if let Some(input_rc) = input_weak.upgrade() {
                        if i < input_grads.len() {
                            let input_id = input_rc.borrow().id;
                            let input_grad = input_grads[i].clone();
                            let mut grads = self.gradients.borrow_mut();
                            if let Some(existing_grad) = grads.get_mut(&input_id) {
                                for (j, &g) in input_grad.data().iter().enumerate() {
                                    existing_grad.data_mut()[j] += g;
                                }
                            } else {
                                grads.insert(input_id, input_grad);
                            }
                        }
                    }
                }
            }
        }
    }
    /// Topological sort using DFS (recursive helper)
    #[allow(clippy::only_used_in_recursion, dead_code)]
    pub(super) fn topological_sort(
        &self,
        node: &Rc<RefCell<GraphNode>>,
        visited: &mut Vec<NodeId>,
        topo_order: &mut Vec<Rc<RefCell<GraphNode>>>,
    ) {
        let (node_id, inputs) = {
            let n = node.borrow();
            (n.id, n.inputs.clone())
        };
        if visited.contains(&node_id) {
            return;
        }
        visited.push(node_id);
        for input_weak in inputs.iter() {
            if let Some(input_rc) = input_weak.upgrade() {
                self.topological_sort(&input_rc, visited, topo_order);
            }
        }
        topo_order.push(node.clone());
    }
}
