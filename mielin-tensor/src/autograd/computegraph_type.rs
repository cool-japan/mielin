//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use core::cell::RefCell;

use super::type_aliases::{GradientStorage, NodeId};

/// Computational graph for managing automatic differentiation
pub struct ComputeGraph {
    /// Counter for node IDs
    pub(super) next_id: RefCell<NodeId>,
    /// Shared gradient storage for all variables in this graph
    pub(super) gradients: GradientStorage,
    /// Enable gradient checkpointing for memory efficiency
    pub(super) checkpointing_enabled: RefCell<bool>,
}
