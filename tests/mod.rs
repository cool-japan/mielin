//! MielinOS Integration Test Modules
//!
//! This module organizes the integration tests for the MielinOS project.
//!
//! ## Test Organization
//!
//! The integration tests are organized into the following modules:
//!
//! - `mesh_cells_integration` - Tests for agent deployment and mesh integration
//! - `tensor_hal_integration` - Tests for hardware detection and tensor operations
//! - `kernel_cells_integration` - Tests for task scheduling and memory management
//! - `migration_flow` - Tests for agent migration between nodes
//! - `full_pipeline` - End-to-end workflow tests
//! - `error_paths` - Error handling and recovery tests
//!
//! ## Running Tests
//!
//! To run all integration tests:
//! ```bash
//! cargo test -p mielin-tests --test integration
//! ```
//!
//! To run a specific test module:
//! ```bash
//! cargo test -p mielin-tests --test integration mesh_cells_integration
//! ```
//!
//! To run a specific test:
//! ```bash
//! cargo test -p mielin-tests --test integration test_full_pipeline_integration
//! ```

// Tests are defined in integration_tests.rs
