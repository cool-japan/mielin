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

pub mod computegraph_predicates;
pub mod computegraph_traits;
pub mod computegraph_type;
pub mod functions;
pub mod type_aliases;
pub mod types;

// Re-export all types
pub use computegraph_type::*;
pub use types::*;

#[cfg(test)]
mod tests;
