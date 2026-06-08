//! Hardware Database
//!
//! This module provides a database of known hardware configurations for validation,
//! testing, and reference. It includes specifications for popular CPUs, GPUs, and
//! accelerators to help verify detection accuracy.

pub mod functions;
pub mod hardwaredatabase_impl;
pub mod hardwaredatabase_queries;
pub mod hardwaredatabase_traits;
pub mod hardwaredatabase_type;
pub mod types;

// Re-export all types
pub use functions::*;
pub use hardwaredatabase_type::*;
pub use types::*;
