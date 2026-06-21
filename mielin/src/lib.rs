//! # MielinOS
//!
//! MielinOS is a microkernel-based operating system designed for distributed AI agents
//! with neural mesh networking capabilities.
//!
//! ## Crate Organization
//!
//! This meta-crate re-exports all MielinOS components:
//!
//! - [`hal`] - Hardware Abstraction Layer for platform-independent hardware access
//! - [`rt`] - Runtime for async execution and task scheduling
//! - [`mesh`] - Neural mesh networking (core + wire protocols)
//! - [`cells`] - Cell-based computation units for distributed processing
//! - [`wasm`] - WebAssembly runtime for portable agent execution
//! - [`tensor`] - Tensor operations for AI/ML workloads
//! - `kernel` - Microkernel (requires `kernel` feature, bare-metal only)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use mielin::prelude::*;
//!
//! // Access HAL for hardware abstraction
//! // Access runtime for async execution
//! // Access mesh for distributed communication
//! ```
//!
//! ## Feature Flags
//!
//! - `default` - Core components (hal, rt, mesh, cells, wasm, tensor)
//! - `full` - All components including kernel
//! - `kernel` - Include microkernel (bare-metal targets only)

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

// Re-export core crates
pub use mielin_cells as cells;
pub use mielin_hal as hal;
pub use mielin_rt as rt;
pub use mielin_tensor as tensor;
pub use mielin_wasm as wasm;

/// Mesh networking components - neural mesh networking for distributed agent communication.
///
/// This module provides both the core abstractions and wire protocols
/// for MielinOS's distributed mesh network.
pub mod mesh {
    pub use mielin_mesh_core as core;
    pub use mielin_mesh_wire as wire;

    // Re-export commonly used types at mesh level
    pub use mielin_mesh_core::{MeshConfig, MeshError, MeshService};
    pub use mielin_mesh_core::{Node, NodeId, NodeRole};
    pub use mielin_mesh_wire::{MigrationCoordinator, QuicTransport, WireError};
}

/// Microkernel components (requires `kernel` feature)
#[cfg(feature = "kernel")]
#[cfg_attr(docsrs, doc(cfg(feature = "kernel")))]
pub use mielin_kernel as kernel;

/// Prelude module - convenient re-exports for common MielinOS usage.
///
/// ```rust,no_run
/// use mielin::prelude::*;
/// ```
pub mod prelude {
    // Mesh networking essentials
    pub use crate::mesh::{MeshConfig, MeshService};
    pub use crate::mesh::{Node, NodeId, NodeRole};

    // HAL essentials
    pub use crate::hal::detect_architecture;

    // Runtime essentials
    pub use crate::rt::EmbeddedRuntime;

    // Tensor essentials
    pub use crate::tensor::Tensor;

    // Cells/Agent essentials
    pub use crate::cells::{Agent, AgentId, AgentState};

    // WASM essentials
    pub use crate::wasm::executor::WasmExecutor;
}

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Crate name
pub const NAME: &str = env!("CARGO_PKG_NAME");
