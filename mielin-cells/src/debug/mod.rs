//! Debugging tools for MielinOS agents
//!
//! This module provides comprehensive debugging capabilities including:
//! - State inspection API
//! - Execution tracing
//! - Memory profiling
//! - Remote debugging

pub mod inspector;
pub mod profiler;
pub mod remote;
pub mod tracer;

pub use inspector::{InspectionQuery, InspectionResult, StateInspector};
pub use profiler::{AllocationRecord, MemoryProfiler, MemorySnapshot, ProfilingReport};
pub use remote::{DebugCommand, DebugSession, DebuggerConfig, RemoteDebugger};
pub use tracer::{ExecutionTracer, TraceEvent, TraceFilter, TraceRecorder};

use serde::{Deserialize, Serialize};

/// Debug context for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugContext {
    /// Agent ID being debugged
    pub agent_id: [u8; 16],
    /// Debug session ID
    pub session_id: [u8; 16],
    /// Debug mode enabled
    pub enabled: bool,
    /// Breakpoints
    pub breakpoints: Vec<Breakpoint>,
    /// Watch expressions
    pub watches: Vec<WatchExpression>,
}

/// Breakpoint in agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    /// Breakpoint ID
    pub id: u64,
    /// Location (e.g., function name, line number)
    pub location: String,
    /// Condition (optional)
    pub condition: Option<String>,
    /// Hit count
    pub hit_count: u64,
    /// Enabled
    pub enabled: bool,
}

/// Watch expression for monitoring values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchExpression {
    /// Watch ID
    pub id: u64,
    /// Expression to evaluate
    pub expression: String,
    /// Last evaluated value
    pub last_value: Option<String>,
}

impl DebugContext {
    /// Create a new debug context
    pub fn new(agent_id: [u8; 16]) -> Self {
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut session_id = [0u8; 16];
        rng.fill(&mut session_id);

        Self {
            agent_id,
            session_id,
            enabled: false,
            breakpoints: Vec::new(),
            watches: Vec::new(),
        }
    }

    /// Enable debugging
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable debugging
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Add a breakpoint
    pub fn add_breakpoint(&mut self, location: String, condition: Option<String>) -> u64 {
        let id = self.breakpoints.len() as u64;
        self.breakpoints.push(Breakpoint {
            id,
            location,
            condition,
            hit_count: 0,
            enabled: true,
        });
        id
    }

    /// Remove a breakpoint
    pub fn remove_breakpoint(&mut self, id: u64) -> bool {
        if let Some(pos) = self.breakpoints.iter().position(|b| b.id == id) {
            self.breakpoints.remove(pos);
            true
        } else {
            false
        }
    }

    /// Add a watch expression
    pub fn add_watch(&mut self, expression: String) -> u64 {
        let id = self.watches.len() as u64;
        self.watches.push(WatchExpression {
            id,
            expression,
            last_value: None,
        });
        id
    }

    /// Remove a watch expression
    pub fn remove_watch(&mut self, id: u64) -> bool {
        if let Some(pos) = self.watches.iter().position(|w| w.id == id) {
            self.watches.remove(pos);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // All four tests call DebugContext::new which uses rand::rng() -> ChaCha20 NEON backend
    // on aarch64. Miri cannot emulate llvm.aarch64.neon.tbl1.v16i8 on macOS.
    // Not UB — hardware SIMD unavailable under Miri interpreter.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_debug_context_creation() {
        let agent_id = [1u8; 16];
        let context = DebugContext::new(agent_id);

        assert_eq!(context.agent_id, agent_id);
        assert!(!context.enabled);
        assert!(context.breakpoints.is_empty());
        assert!(context.watches.is_empty());
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_debug_context_enable_disable() {
        let agent_id = [1u8; 16];
        let mut context = DebugContext::new(agent_id);

        assert!(!context.enabled);
        context.enable();
        assert!(context.enabled);
        context.disable();
        assert!(!context.enabled);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_debug_context_breakpoints() {
        let agent_id = [1u8; 16];
        let mut context = DebugContext::new(agent_id);

        let bp_id = context.add_breakpoint("main".to_string(), None);
        assert_eq!(context.breakpoints.len(), 1);

        let removed = context.remove_breakpoint(bp_id);
        assert!(removed);
        assert_eq!(context.breakpoints.len(), 0);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_debug_context_watches() {
        let agent_id = [1u8; 16];
        let mut context = DebugContext::new(agent_id);

        let watch_id = context.add_watch("variable_x".to_string());
        assert_eq!(context.watches.len(), 1);

        let removed = context.remove_watch(watch_id);
        assert!(removed);
        assert_eq!(context.watches.len(), 0);
    }
}
