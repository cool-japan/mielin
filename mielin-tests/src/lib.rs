//! MielinOS Integration Tests Crate
//!
//! This crate provides comprehensive end-to-end integration tests
//! for the MielinOS ecosystem.
//!
//! ## Test Modules
//!
//! - **mesh_cells_integration**: Agent deployment and mesh integration
//! - **tensor_hal_integration**: Hardware detection and tensor operations
//! - **kernel_cells_integration**: Task scheduling and memory management
//! - **migration_flow**: Agent migration between nodes
//! - **full_pipeline**: Complete E2E workflows
//! - **error_paths**: Error handling and recovery
//!
//! Tests are organized into several modules covering different aspects
//! of the MielinOS system.

/// Re-export commonly used types for testing
pub mod prelude {
    pub use mielin_cells::{Agent, AgentError, AgentState, TransitionResult};
    pub use mielin_hal::capabilities::{HardwareCapabilities, HardwareProfile};
    pub use mielin_kernel::memory::{MemoryManager, PAGE_SIZE};
    pub use mielin_kernel::scheduler::Scheduler;
    pub use mielin_mesh_core::node::Node;
    pub use mielin_tensor::TensorRuntime;
}
