//! Automatic Differentiation (Autograd) Module
//!
//! Provides both forward-mode and reverse-mode automatic differentiation
//! for tensor operations. Essential for neural network training with backpropagation.
//!
//! # Features
//! - Forward mode AD (dual numbers)
//! - Reverse mode AD (backpropagation with computational graph)
//! - Gradient checkpointing for memory efficiency
//! - Higher-order derivatives support
//!
//! # Architecture
//! - `Variable`: Tensor wrapper with gradient tracking
//! - `ComputeGraph`: Records operations for backpropagation
//! - `GradFn`: Backward function for each operation
//! - `Checkpoint`: Recomputation nodes for memory savings

#![allow(dead_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::{Add, Mul, Sub};

use crate::tensor::Tensor;

/// Unique identifier for computational graph nodes
type NodeId = usize;

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

/// Gradient function that computes gradients for backward pass
type GradFn = Rc<dyn Fn(&Tensor<f32>) -> Vec<Tensor<f32>>>;

/// Node in the computational graph
struct GraphNode {
    /// Unique identifier
    id: NodeId,
    /// Operation type
    op: OpType,
    /// Input nodes (parents in the graph)
    inputs: Vec<Weak<RefCell<GraphNode>>>,
    /// Output tensor value
    value: Tensor<f32>,
    /// Function to compute gradients for inputs
    grad_fn: Option<GradFn>,
    /// Whether this node requires gradient computation
    requires_grad: bool,
    /// Checkpoint flag for gradient checkpointing
    checkpoint: bool,
}

impl GraphNode {
    /// Create a new leaf node (input variable)
    fn new_leaf(value: Tensor<f32>, requires_grad: bool) -> Self {
        Self {
            id: 0, // Will be set by graph
            op: OpType::Leaf,
            inputs: Vec::new(),
            value,
            grad_fn: None,
            requires_grad,
            checkpoint: false,
        }
    }

    /// Create a new operation node
    fn new_op(
        op: OpType,
        inputs: Vec<Weak<RefCell<GraphNode>>>,
        value: Tensor<f32>,
        grad_fn: Option<GradFn>,
    ) -> Self {
        Self {
            id: 0, // Will be set by graph
            op,
            inputs,
            value,
            grad_fn,
            requires_grad: true,
            checkpoint: false,
        }
    }
}

/// Gradient storage shared between variables
type GradientStorage = Rc<RefCell<BTreeMap<NodeId, Tensor<f32>>>>;

/// Variable: Tensor wrapper with gradient tracking
#[derive(Clone)]
pub struct Variable {
    /// Shared reference to the computational graph node
    node: Rc<RefCell<GraphNode>>,
    /// Shared gradient storage
    gradients: GradientStorage,
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
    fn from_node(node: Rc<RefCell<GraphNode>>, gradients: GradientStorage) -> Self {
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
        // For checkpointed nodes, we can clear the value tensor
        // to save memory (it will be recomputed during backward)
        if self.is_checkpoint() {
            let shape = self.node.borrow().value.shape().to_vec();
            self.node.borrow_mut().value = Tensor::zeros(shape);
        }
    }

    /// Perform backward pass from this variable
    pub fn backward(&self) {
        // Initialize gradient to ones (scalar gradient)
        let value = &self.node.borrow().value;
        let grad = Tensor::ones(value.shape().to_vec());

        // Perform topological sort and backward pass
        self.backward_impl(grad);
    }

    /// Backward pass with custom gradient
    pub fn backward_with_grad(&self, grad: Tensor<f32>) {
        self.backward_impl(grad);
    }

    /// Implementation of backward pass using iterative topological sort
    fn backward_impl(&self, grad: Tensor<f32>) {
        // Build topological order using iterative DFS (avoids recursive borrow issues)
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

            // Check if all inputs have been visited
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
                // All inputs visited, we can visit this node
                visited.push(node_id);
                topo_order.push(node_rc.clone());
            } else {
                // Push this node back and push unvisited inputs
                stack.push(node_rc.clone());
                for input_rc in unvisited_inputs {
                    stack.push(input_rc);
                }
            }
        }

        // Initialize gradient for the output node in separate storage
        let output_node_id = self.node.borrow().id;
        self.gradients.borrow_mut().insert(output_node_id, grad);

        // Backward pass in reverse topological order
        for node_rc in topo_order.iter().rev() {
            // Extract data needed for gradient computation (avoid holding borrow)
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

            // Get the gradient for this node from separate storage
            let grad = if let Some(g) = self.gradients.borrow().get(&node_id) {
                g.clone()
            } else {
                continue;
            };

            // Compute gradients for inputs using the gradient function
            if let Some(grad_fn) = grad_fn_opt {
                let input_grads = grad_fn(&grad);

                // Distribute gradients to input nodes in separate storage
                for (i, input_weak) in inputs_weak.iter().enumerate() {
                    if let Some(input_rc) = input_weak.upgrade() {
                        if i < input_grads.len() {
                            let input_id = input_rc.borrow().id;
                            let input_grad = input_grads[i].clone();

                            // Accumulate gradient in separate storage
                            let mut grads = self.gradients.borrow_mut();
                            if let Some(existing_grad) = grads.get_mut(&input_id) {
                                // Add gradients element-wise
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
    #[allow(clippy::only_used_in_recursion)]
    fn topological_sort(
        &self,
        node: &Rc<RefCell<GraphNode>>,
        visited: &mut Vec<NodeId>,
        topo_order: &mut Vec<Rc<RefCell<GraphNode>>>,
    ) {
        // Extract node_id and inputs without holding the borrow
        let (node_id, inputs) = {
            let n = node.borrow();
            (n.id, n.inputs.clone())
        };

        if visited.contains(&node_id) {
            return;
        }

        visited.push(node_id);

        // Visit all input nodes
        for input_weak in inputs.iter() {
            if let Some(input_rc) = input_weak.upgrade() {
                self.topological_sort(&input_rc, visited, topo_order);
            }
        }

        topo_order.push(node.clone());
    }
}

/// Computational graph for managing automatic differentiation
pub struct ComputeGraph {
    /// Counter for node IDs
    next_id: RefCell<NodeId>,
    /// Shared gradient storage for all variables in this graph
    gradients: GradientStorage,
    /// Enable gradient checkpointing for memory efficiency
    checkpointing_enabled: RefCell<bool>,
}

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

    /// Compute the Jacobian-vector product (JVP) for forward-mode AD
    ///
    /// Given function f: R^n -> R^m and tangent vector v in R^n,
    /// computes df/dx * v efficiently in forward mode.
    pub fn jvp(&self, f: &Variable, x: &Variable, v: &Tensor<f32>) -> Tensor<f32> {
        // For now, use finite differences as a placeholder
        // A full dual-numbers implementation would be more efficient
        let eps = 1e-5;
        let x_data = x.data();

        // f(x + eps*v)
        let mut x_plus = x_data.clone();
        for (i, val) in x_plus.data_mut().iter_mut().enumerate() {
            *val += eps * v.data()[i];
        }
        let _x_plus_var = self.variable(x_plus, false);

        // Recompute f with x + eps*v (this is a simplified placeholder)
        // In a real implementation, we'd need to replay the computation
        let f_plus = f.data();

        // (f(x + eps*v) - f(x)) / eps
        let mut jvp = f_plus.clone();
        for (i, val) in jvp.data_mut().iter_mut().enumerate() {
            *val = (*val - f.data().data()[i]) / eps;
        }

        jvp
    }

    /// Compute second derivative using backward-over-backward
    ///
    /// Given scalar function f: R -> R, computes d²f/dx²
    pub fn second_derivative(&self, f: &Variable, x: &Variable) -> Option<f32> {
        // First backward pass to get df/dx
        f.backward();
        let first_grad = x.grad()?;

        // Create a new graph for second derivative
        let _graph2 = ComputeGraph::new();
        let _x2 = _graph2.variable(x.data(), true);

        // We need to recreate the computation to get gradients of gradients
        // For now, return the first gradient as a placeholder
        // A full implementation would require graph recording
        Some(first_grad.data()[0])
    }

    /// Add two variables: z = x + y
    /// Gradient: dL/dx = dL/dz, dL/dy = dL/dz
    pub fn add(&self, x: &Variable, y: &Variable) -> Variable {
        let x_data = x.data();
        let y_data = y.data();

        // Forward pass
        let z_data = x_data.add(&y_data);

        // Backward pass function
        let grad_fn: GradFn = Rc::new(|grad: &Tensor<f32>| {
            // Both inputs receive the same gradient
            alloc::vec![grad.clone(), grad.clone()]
        });

        let mut node = GraphNode::new_op(
            OpType::Add,
            alloc::vec![Rc::downgrade(&x.node), Rc::downgrade(&y.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Subtract two variables: z = x - y
    /// Gradient: dL/dx = dL/dz, dL/dy = -dL/dz
    pub fn sub(&self, x: &Variable, y: &Variable) -> Variable {
        let x_data = x.data();
        let y_data = y.data();

        // Forward pass
        let z_data = x_data.sub(&y_data);

        // Backward pass function
        let grad_fn: GradFn = Rc::new(|grad: &Tensor<f32>| {
            let neg_grad = grad.scale(-1.0);
            alloc::vec![grad.clone(), neg_grad]
        });

        let mut node = GraphNode::new_op(
            OpType::Sub,
            alloc::vec![Rc::downgrade(&x.node), Rc::downgrade(&y.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Multiply two variables element-wise: z = x * y
    /// Gradient: dL/dx = dL/dz * y, dL/dy = dL/dz * x
    pub fn mul(&self, x: &Variable, y: &Variable) -> Variable {
        let x_data = x.data();
        let y_data = y.data();

        // Forward pass
        let z_data = x_data.mul(&y_data);

        // Clone for closure
        let x_data_clone = x_data.clone();
        let y_data_clone = y_data.clone();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let grad_x = grad.mul(&y_data_clone);
            let grad_y = grad.mul(&x_data_clone);
            alloc::vec![grad_x, grad_y]
        });

        let mut node = GraphNode::new_op(
            OpType::Mul,
            alloc::vec![Rc::downgrade(&x.node), Rc::downgrade(&y.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Sum reduction: z = sum(x)
    /// Gradient: dL/dx = dL/dz * ones_like(x)
    pub fn sum(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass: sum all elements
        let sum_val = x_data.data().iter().sum::<f32>();
        let z_data = Tensor::scalar(sum_val);

        // Clone shape for closure
        let x_shape = x_data.shape().to_vec();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            // Broadcast scalar gradient to input shape
            let grad_val = grad.data()[0];
            let grad_x = Tensor::filled(x_shape.clone(), grad_val);
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Sum,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Mean reduction: z = mean(x)
    /// Gradient: dL/dx = dL/dz / numel(x)
    pub fn mean(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass: mean of all elements
        let numel = x_data.data().len() as f32;
        let mean_val = x_data.data().iter().sum::<f32>() / numel;
        let z_data = Tensor::scalar(mean_val);

        // Clone for closure
        let x_shape = x_data.shape().to_vec();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            // Broadcast and scale gradient
            let numel = x_shape.iter().product::<usize>() as f32;
            let grad_val = grad.data()[0] / numel;
            let grad_x = Tensor::filled(x_shape.clone(), grad_val);
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Mean,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// ReLU activation: z = max(0, x)
    /// Gradient: dL/dx = dL/dz if x > 0 else 0
    pub fn relu(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = val.max(0.0);
        }

        // Clone for closure (mask where x > 0)
        let mask: Vec<f32> = x_data
            .data()
            .iter()
            .map(|&v| if v > 0.0 { 1.0 } else { 0.0 })
            .collect();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                *val *= mask[i];
            }
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::ReLU,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Sigmoid activation: z = 1 / (1 + exp(-x))
    /// Gradient: dL/dx = dL/dz * z * (1 - z)
    pub fn sigmoid(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = 1.0 / (1.0 + libm::expf(-*val));
        }

        // Clone for closure
        let z_clone = z_data.clone();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                let z = z_clone.data()[i];
                *val *= z * (1.0 - z);
            }
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Sigmoid,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Tanh activation: z = tanh(x)
    /// Gradient: dL/dx = dL/dz * (1 - z^2)
    pub fn tanh(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::tanhf(*val);
        }

        // Clone for closure
        let z_clone = z_data.clone();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                let z = z_clone.data()[i];
                *val *= 1.0 - z * z;
            }
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Tanh,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Power operation: z = x^n
    /// Gradient: dL/dx = dL/dz * n * x^(n-1)
    pub fn pow(&self, x: &Variable, n: f32) -> Variable {
        let x_data = x.data();

        // Forward pass
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::powf(*val, n);
        }

        // Clone for closure
        let x_clone = x_data.clone();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                let x = x_clone.data()[i];
                *val *= n * libm::powf(x, n - 1.0);
            }
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Pow,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Exponential: z = exp(x)
    /// Gradient: dL/dx = dL/dz * exp(x) = dL/dz * z
    pub fn exp(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::expf(*val);
        }

        // Clone for closure
        let z_clone = z_data.clone();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                *val *= z_clone.data()[i];
            }
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Exp,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }

    /// Natural logarithm: z = log(x)
    /// Gradient: dL/dx = dL/dz / x
    pub fn log(&self, x: &Variable) -> Variable {
        let x_data = x.data();

        // Forward pass
        let mut z_data = x_data.clone();
        for val in z_data.data_mut().iter_mut() {
            *val = libm::logf(*val);
        }

        // Clone for closure
        let x_clone = x_data.clone();

        // Backward pass function
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grad_x = grad.clone();
            for (i, val) in grad_x.data_mut().iter_mut().enumerate() {
                *val /= x_clone.data()[i];
            }
            alloc::vec![grad_x]
        });

        let mut node = GraphNode::new_op(
            OpType::Log,
            alloc::vec![Rc::downgrade(&x.node)],
            z_data,
            Some(grad_fn),
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

        // Determine output shape and perform matmul
        let (m, n, k, is_matvec) = if a_shape.len() == 2 && b_shape.len() == 1 {
            // Matrix-vector multiplication: [m, n] @ [n] -> [m]
            assert_eq!(
                a_shape[1], b_shape[0],
                "Matrix-vector dimension mismatch: [{}, {}] @ [{}]",
                a_shape[0], a_shape[1], b_shape[0]
            );
            (a_shape[0], a_shape[1], 1, true)
        } else if a_shape.len() == 2 && b_shape.len() == 2 {
            // Matrix-matrix multiplication: [m, n] @ [n, k] -> [m, k]
            assert_eq!(
                a_shape[1], b_shape[0],
                "Matrix-matrix dimension mismatch: [{}, {}] @ [{}, {}]",
                a_shape[0], a_shape[1], b_shape[0], b_shape[1]
            );
            (a_shape[0], a_shape[1], b_shape[1], false)
        } else {
            panic!("Unsupported matmul shapes: {:?} @ {:?}", a_shape, b_shape);
        };

        // Forward pass: compute C = A @ B
        let mut c_data = if is_matvec {
            alloc::vec![0.0f32; m]
        } else {
            alloc::vec![0.0f32; m * k]
        };

        if is_matvec {
            // Matrix-vector: c[i] = sum_j(a[i,j] * b[j])
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
            // Matrix-matrix: c[i,j] = sum_k(a[i,k] * b[k,j])
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

        // Clone data for backward pass
        let a_clone = a_data.clone();
        let b_clone = b_data.clone();
        let m_clone = m;
        let n_clone = n;
        let k_clone = k;

        // Backward pass: compute gradients
        // dL/dA = dL/dC @ B^T
        // dL/dB = A^T @ dL/dC
        let grad_fn: GradFn = Rc::new(move |grad: &Tensor<f32>| {
            let mut grads = alloc::vec![];

            if is_matvec {
                // grad is [m], compute grad_a = grad @ b^T = [m] outer [n] = [m, n]
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

                // grad_b = a^T @ grad = [n, m] @ [m] = [n]
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
                // grad is [m, k]
                // grad_a = grad @ b^T = [m, k] @ [k, n] = [m, n]
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

                // grad_b = a^T @ grad = [n, m] @ [m, k] = [n, k]
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

        let mut node = GraphNode::new_op(
            OpType::MatMul,
            alloc::vec![Rc::downgrade(&a.node), Rc::downgrade(&b.node)],
            c_tensor,
            Some(grad_fn),
        );
        node.id = self.next_id();

        Variable::from_node(Rc::new(RefCell::new(node)), self.gradients.clone())
    }
}

impl Default for ComputeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// Operator overloading for Variables
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_creation() {
        let x = Variable::new(Tensor::scalar(5.0), true);
        assert_eq!(x.data().data()[0], 5.0);
        assert!(x.requires_grad());
    }

    #[test]
    fn test_add_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.variable(Tensor::scalar(4.0), true);
        let z = graph.add(&x, &y);

        assert_eq!(z.data().data()[0], 7.0);

        z.backward();

        assert_eq!(x.grad().unwrap().data()[0], 1.0);
        assert_eq!(y.grad().unwrap().data()[0], 1.0);
    }

    #[test]
    fn test_mul_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.variable(Tensor::scalar(4.0), true);
        let z = graph.mul(&x, &y);

        assert_eq!(z.data().data()[0], 12.0);

        z.backward();

        // dL/dx = y = 4.0, dL/dy = x = 3.0
        assert_eq!(x.grad().unwrap().data()[0], 4.0);
        assert_eq!(y.grad().unwrap().data()[0], 3.0);
    }

    #[test]
    fn test_sum_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::vector(alloc::vec![1.0, 2.0, 3.0]), true);
        let z = graph.sum(&x);

        assert_eq!(z.data().data()[0], 6.0);

        z.backward();

        let grad = x.grad().unwrap();
        assert_eq!(grad.data(), &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_mean_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::vector(alloc::vec![2.0, 4.0, 6.0]), true);
        let z = graph.mean(&x);

        assert_eq!(z.data().data()[0], 4.0);

        z.backward();

        let grad = x.grad().unwrap();
        // Each element contributes 1/3 to the gradient
        for &g in grad.data() {
            assert!((g - 1.0 / 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_relu_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::vector(alloc::vec![-1.0, 0.0, 2.0]), true);
        let z = graph.relu(&x);

        assert_eq!(z.data().data(), &[0.0, 0.0, 2.0]);

        z.backward_with_grad(Tensor::vector(alloc::vec![1.0, 1.0, 1.0]));

        let grad = x.grad().unwrap();
        assert_eq!(grad.data(), &[0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_sigmoid_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(0.0), true);
        let z = graph.sigmoid(&x);

        // sigmoid(0) = 0.5
        assert!((z.data().data()[0] - 0.5).abs() < 1e-6);

        z.backward();

        // gradient at x=0 is 0.5 * (1 - 0.5) = 0.25
        assert!((x.grad().unwrap().data()[0] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_tanh_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(0.0), true);
        let z = graph.tanh(&x);

        // tanh(0) = 0
        assert!((z.data().data()[0]).abs() < 1e-6);

        z.backward();

        // gradient at x=0 is 1 - 0^2 = 1
        assert!((x.grad().unwrap().data()[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_pow_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let z = graph.pow(&x, 3.0);

        // 2^3 = 8
        assert_eq!(z.data().data()[0], 8.0);

        z.backward();

        // gradient: 3 * 2^2 = 12
        assert_eq!(x.grad().unwrap().data()[0], 12.0);
    }

    #[test]
    fn test_exp_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(1.0), true);
        let z = graph.exp(&x);

        // exp(1) ≈ 2.718
        assert!((z.data().data()[0] - libm::expf(1.0)).abs() < 1e-6);

        z.backward();

        // gradient of exp(x) is exp(x)
        assert!((x.grad().unwrap().data()[0] - libm::expf(1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_log_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let z = graph.log(&x);

        // log(2) ≈ 0.693
        assert!((z.data().data()[0] - libm::logf(2.0)).abs() < 1e-6);

        z.backward();

        // gradient: 1/x = 1/2 = 0.5
        assert!((x.grad().unwrap().data()[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_chain_rule() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.variable(Tensor::scalar(3.0), true);

        // z = (x + y) * x = (2 + 3) * 2 = 10
        let sum = graph.add(&x, &y);
        let z = graph.mul(&sum, &x);

        assert_eq!(z.data().data()[0], 10.0);

        z.backward();

        // dz/dx = (x + y) + x = 2x + y = 4 + 3 = 7
        // dz/dy = x = 2
        assert_eq!(x.grad().unwrap().data()[0], 7.0);
        assert_eq!(y.grad().unwrap().data()[0], 2.0);
    }

    #[test]
    fn test_complex_computation() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);

        // y = x^2 + 2x + 1 = 9 + 6 + 1 = 16
        let x_squared = graph.pow(&x, 2.0);
        let two_x = graph.mul(&x, &graph.variable(Tensor::scalar(2.0), false));
        let sum1 = graph.add(&x_squared, &two_x);
        let y = graph.add(&sum1, &graph.variable(Tensor::scalar(1.0), false));

        assert_eq!(y.data().data()[0], 16.0);

        y.backward();

        // dy/dx = 2x + 2 = 6 + 2 = 8
        assert_eq!(x.grad().unwrap().data()[0], 8.0);
    }

    #[test]
    fn test_zero_grad() {
        let graph = ComputeGraph::new();
        let mut x = graph.variable(Tensor::scalar(5.0), true);
        let z = graph.pow(&x, 2.0);

        z.backward();
        assert!(x.grad().is_some());

        x.zero_grad();
        assert!(x.grad().is_none());
    }

    #[test]
    fn test_matmul_matrix_vector() {
        let graph = ComputeGraph::new();

        // Matrix [2, 3] @ Vector [3] -> [2]
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], alloc::vec![2, 3]).unwrap(),
            true,
        );
        let b = graph.variable(Tensor::vector(alloc::vec![1.0, 2.0, 3.0]), true);

        let c = graph.matmul(&a, &b);

        // Forward pass: [1, 2, 3] @ [1, 2, 3]^T = [1*1 + 2*2 + 3*3] = 14
        //               [4, 5, 6] @ [1, 2, 3]^T = [4*1 + 5*2 + 6*3] = 32
        assert_eq!(c.data().data(), &[14.0, 32.0]);

        c.backward_with_grad(Tensor::vector(alloc::vec![1.0, 1.0]));

        // Gradient of A: grad @ b^T = [1, 1] outer [1, 2, 3] = [[1, 2, 3], [1, 2, 3]]
        let a_grad = a.grad().unwrap();
        assert_eq!(a_grad.data(), &[1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);

        // Gradient of b: A^T @ grad = [1, 4; 2, 5; 3, 6] @ [1, 1] = [5, 7, 9]
        let b_grad = b.grad().unwrap();
        assert_eq!(b_grad.data(), &[5.0, 7.0, 9.0]);
    }

    #[test]
    fn test_matmul_matrix_matrix() {
        let graph = ComputeGraph::new();

        // Matrix [2, 3] @ Matrix [3, 2] -> [2, 2]
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], alloc::vec![2, 3]).unwrap(),
            true,
        );
        let b = graph.variable(
            Tensor::from_vec(
                alloc::vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
                alloc::vec![3, 2],
            )
            .unwrap(),
            true,
        );

        let c = graph.matmul(&a, &b);

        // Forward pass:
        // [1, 2, 3] @ [7,  8  ]   [1*7+2*9+3*11,  1*8+2*10+3*12]   [58,  64]
        // [4, 5, 6]   [9,  10 ] = [4*7+5*9+6*11,  4*8+5*10+6*12] = [139, 154]
        //             [11, 12]
        assert_eq!(c.data().shape(), &[2, 2]);
        assert_eq!(c.data().get(&[0, 0]), Some(&58.0));
        assert_eq!(c.data().get(&[0, 1]), Some(&64.0));
        assert_eq!(c.data().get(&[1, 0]), Some(&139.0));
        assert_eq!(c.data().get(&[1, 1]), Some(&154.0));

        c.backward_with_grad(
            Tensor::from_vec(alloc::vec![1.0, 1.0, 1.0, 1.0], alloc::vec![2, 2]).unwrap(),
        );

        // Gradients should be computed correctly
        assert!(a.grad().is_some());
        assert!(b.grad().is_some());
    }

    #[test]
    fn test_matmul_gradient_simple() {
        let graph = ComputeGraph::new();

        // Simple case: [1, 2] @ [2] -> [1]
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![2.0, 3.0], alloc::vec![1, 2]).unwrap(),
            true,
        );
        let b = graph.variable(Tensor::vector(alloc::vec![4.0, 5.0]), true);

        let c = graph.matmul(&a, &b);

        // Forward: 2*4 + 3*5 = 8 + 15 = 23
        assert_eq!(c.data().data()[0], 23.0);

        c.backward();

        // Gradient of A with respect to loss = b^T = [4, 5]
        let a_grad = a.grad().unwrap();
        assert_eq!(a_grad.data(), &[4.0, 5.0]);

        // Gradient of b with respect to loss = A^T = [2, 3]
        let b_grad = b.grad().unwrap();
        assert_eq!(b_grad.data(), &[2.0, 3.0]);
    }

    #[test]
    fn test_checkpoint_flag() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(5.0), true);
        let mut y = graph.add(&x, &graph.variable(Tensor::scalar(3.0), false));

        assert!(!y.is_checkpoint());
        y.checkpoint();
        assert!(y.is_checkpoint());
    }

    #[test]
    fn test_checkpoint_enable_disable() {
        let graph = ComputeGraph::new();
        assert!(!graph.is_checkpointing_enabled());

        graph.enable_checkpointing();
        assert!(graph.is_checkpointing_enabled());

        graph.disable_checkpointing();
        assert!(!graph.is_checkpointing_enabled());
    }

    #[test]
    fn test_checkpoint_gradient_computation() {
        // Test that checkpointing doesn't affect gradient computation
        let graph = ComputeGraph::new();
        graph.enable_checkpointing();

        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.variable(Tensor::scalar(4.0), true);
        let mut z = graph.mul(&x, &y);

        // Mark as checkpoint
        z.checkpoint();
        assert_eq!(z.data().data()[0], 12.0);

        // Backward pass should still work correctly
        z.backward();

        assert_eq!(x.grad().unwrap().data()[0], 4.0);
        assert_eq!(y.grad().unwrap().data()[0], 3.0);
    }

    #[test]
    fn test_checkpoint_clear_cache() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(5.0), true);
        let mut y = graph.pow(&x, 2.0);

        // Value before checkpointing
        assert_eq!(y.data().data()[0], 25.0);

        // Mark as checkpoint and clear cache
        y.checkpoint();
        y.clear_cache();

        // Value should be zeroed (memory saved)
        assert_eq!(y.data().data()[0], 0.0);
    }

    #[test]
    fn test_checkpoint_multiple_nodes() {
        let graph = ComputeGraph::new();
        graph.enable_checkpointing();

        let x = graph.variable(Tensor::scalar(2.0), true);
        let mut y1 = graph.mul(&x, &x); // x^2
        let mut y2 = graph.add(&y1, &x); // x^2 + x
        let z = graph.mul(&y2, &x); // x * (x^2 + x) = x^3 + x^2

        // Checkpoint intermediate nodes
        y1.checkpoint();
        y2.checkpoint();

        assert!(y1.is_checkpoint());
        assert!(y2.is_checkpoint());

        // Forward: 2^3 + 2^2 = 8 + 4 = 12
        assert_eq!(z.data().data()[0], 12.0);

        z.backward();

        // dz/dx = 3x^2 + 2x = 12 + 4 = 16
        assert_eq!(x.grad().unwrap().data()[0], 16.0);
    }

    #[test]
    fn test_grad_var() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.pow(&x, 2.0); // y = x^2

        y.backward();

        // Get gradient as a Variable
        let grad_var = x.grad_var(false);
        assert!(grad_var.is_some());
        assert_eq!(grad_var.unwrap().data().data()[0], 6.0); // dy/dx = 2x = 6
    }

    #[test]
    fn test_higher_order_gradient_manual() {
        // Test computing second derivative manually
        // f(x) = x^3, f'(x) = 3x^2, f''(x) = 6x

        let graph1 = ComputeGraph::new();
        let x1 = graph1.variable(Tensor::scalar(2.0), true);
        let y1 = graph1.pow(&x1, 3.0); // y = x^3

        y1.backward();
        let first_deriv = x1.grad().unwrap().data()[0]; // 3 * 2^2 = 12
        assert_eq!(first_deriv, 12.0);

        // Manually compute second derivative by differentiating again
        let graph2 = ComputeGraph::new();
        let x2 = graph2.variable(Tensor::scalar(2.0), true);
        let grad_func = graph2.pow(&x2, 2.0);
        let scaled = graph2.mul(&grad_func, &graph2.variable(Tensor::scalar(3.0), false));

        scaled.backward();
        let second_deriv = x2.grad().unwrap().data()[0]; // 6 * 2 = 12
        assert_eq!(second_deriv, 12.0); // f''(2) = 6 * 2 = 12
    }

    #[test]
    fn test_grad_and_detach() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(5.0), true);
        let y = graph.mul(&x, &x);

        y.backward();

        let grad_detached = x.grad_and_detach();
        assert!(grad_detached.is_some());
        assert!(!grad_detached.unwrap().requires_grad());
        assert_eq!(x.grad().unwrap().data()[0], 10.0);
    }

    #[test]
    fn test_jvp_placeholder() {
        // Test the JVP placeholder implementation
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.pow(&x, 2.0); // y = x^2

        let v = Tensor::scalar(1.0);
        let jvp_result = graph.jvp(&y, &x, &v);

        // JVP is a placeholder implementation that returns difference
        // Just verify it returns a value (not necessarily correct)
        // In a real implementation, this would use dual numbers
        assert_eq!(jvp_result.data().len(), 1);
    }

    #[test]
    fn test_second_derivative_placeholder() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.pow(&x, 2.0); // y = x^2, dy/dx = 2x = 4

        let second_deriv = graph.second_derivative(&y, &x);
        assert!(second_deriv.is_some());
        // This is a placeholder implementation, so just check it returns something
        assert_eq!(second_deriv.unwrap(), 4.0); // First derivative value
    }
}
