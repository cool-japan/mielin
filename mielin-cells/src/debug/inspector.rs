//! State inspection API for agents
//!
//! This module provides tools to inspect and query agent state at runtime.

use crate::CellError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// State inspector for examining agent state
#[derive(Debug)]
pub struct StateInspector {
    /// Agent ID being inspected
    #[allow(dead_code)]
    agent_id: [u8; 16],
    /// Cached state snapshots
    snapshots: HashMap<u64, StateSnapshot>,
    /// Next snapshot ID
    next_snapshot_id: u64,
}

/// State snapshot at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Snapshot ID
    pub id: u64,
    /// Timestamp when snapshot was taken
    pub timestamp: u64,
    /// Agent state data
    pub state_data: Vec<u8>,
    /// Memory layout information
    pub memory_layout: MemoryLayout,
    /// Variable information
    pub variables: Vec<VariableInfo>,
}

/// Memory layout information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLayout {
    /// Total memory size
    pub total_size: usize,
    /// Stack size
    pub stack_size: usize,
    /// Heap size
    pub heap_size: usize,
    /// Memory regions
    pub regions: Vec<MemoryRegion>,
}

/// Memory region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegion {
    /// Region name
    pub name: String,
    /// Start offset
    pub start: usize,
    /// Size in bytes
    pub size: usize,
    /// Permissions (read/write/execute)
    pub permissions: String,
}

/// Variable information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableInfo {
    /// Variable name
    pub name: String,
    /// Type name
    pub type_name: String,
    /// Value (as string)
    pub value: String,
    /// Memory address
    pub address: usize,
    /// Size in bytes
    pub size: usize,
}

/// Inspection query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InspectionQuery {
    /// Get full state snapshot
    FullSnapshot,
    /// Get variable by name
    GetVariable { name: String },
    /// Get memory region
    GetMemoryRegion { name: String },
    /// Get memory at address
    GetMemory { address: usize, size: usize },
    /// List all variables
    ListVariables,
    /// Get call stack
    GetCallStack,
}

/// Inspection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InspectionResult {
    /// Full snapshot
    Snapshot(StateSnapshot),
    /// Variable value
    Variable(VariableInfo),
    /// Memory region
    MemoryRegion(MemoryRegion),
    /// Memory data
    Memory { address: usize, data: Vec<u8> },
    /// List of variables
    Variables(Vec<VariableInfo>),
    /// Call stack frames
    CallStack(Vec<StackFrame>),
    /// Error
    Error(String),
}

/// Stack frame information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Frame number (0 = current)
    pub frame_number: usize,
    /// Function name
    pub function_name: String,
    /// File name (if available)
    pub file_name: Option<String>,
    /// Line number (if available)
    pub line_number: Option<u32>,
    /// Local variables
    pub locals: Vec<VariableInfo>,
}

impl StateInspector {
    /// Create a new state inspector
    pub fn new(agent_id: [u8; 16]) -> Self {
        Self {
            agent_id,
            snapshots: HashMap::new(),
            next_snapshot_id: 1,
        }
    }

    /// Capture a state snapshot
    pub fn capture_snapshot(&mut self, state_data: Vec<u8>) -> Result<u64, CellError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        let snapshot = StateSnapshot {
            id: self.next_snapshot_id,
            timestamp,
            state_data: state_data.clone(),
            memory_layout: Self::analyze_memory_layout(&state_data),
            variables: Self::extract_variables(&state_data),
        };

        let snapshot_id = self.next_snapshot_id;
        self.snapshots.insert(snapshot_id, snapshot);
        self.next_snapshot_id += 1;

        Ok(snapshot_id)
    }

    /// Analyze memory layout from state data
    fn analyze_memory_layout(state_data: &[u8]) -> MemoryLayout {
        // Simplified analysis - in a real implementation, this would parse
        // the actual WASM memory layout
        let total_size = state_data.len();

        let mut regions = Vec::new();
        if total_size > 0 {
            // Add a single region for now
            regions.push(MemoryRegion {
                name: "linear_memory".to_string(),
                start: 0,
                size: total_size,
                permissions: "rw".to_string(),
            });
        }

        MemoryLayout {
            total_size,
            stack_size: 0, // Would be determined from WASM metadata
            heap_size: total_size,
            regions,
        }
    }

    /// Extract variables from state data
    fn extract_variables(state_data: &[u8]) -> Vec<VariableInfo> {
        // Simplified extraction - in a real implementation, this would use
        // debug symbols and WASM metadata
        let mut variables = Vec::new();

        if !state_data.is_empty() {
            variables.push(VariableInfo {
                name: "state_size".to_string(),
                type_name: "usize".to_string(),
                value: state_data.len().to_string(),
                address: 0,
                size: std::mem::size_of::<usize>(),
            });
        }

        variables
    }

    /// Execute an inspection query
    pub fn query(&self, query: &InspectionQuery) -> InspectionResult {
        match query {
            InspectionQuery::FullSnapshot => {
                if let Some(snapshot) = self.snapshots.values().last() {
                    InspectionResult::Snapshot(snapshot.clone())
                } else {
                    InspectionResult::Error("No snapshots available".to_string())
                }
            }
            InspectionQuery::GetVariable { name } => {
                if let Some(snapshot) = self.snapshots.values().last() {
                    if let Some(var) = snapshot.variables.iter().find(|v| v.name == *name) {
                        InspectionResult::Variable(var.clone())
                    } else {
                        InspectionResult::Error(format!("Variable '{}' not found", name))
                    }
                } else {
                    InspectionResult::Error("No snapshots available".to_string())
                }
            }
            InspectionQuery::GetMemoryRegion { name } => {
                if let Some(snapshot) = self.snapshots.values().last() {
                    if let Some(region) = snapshot
                        .memory_layout
                        .regions
                        .iter()
                        .find(|r| r.name == *name)
                    {
                        InspectionResult::MemoryRegion(region.clone())
                    } else {
                        InspectionResult::Error(format!("Memory region '{}' not found", name))
                    }
                } else {
                    InspectionResult::Error("No snapshots available".to_string())
                }
            }
            InspectionQuery::GetMemory { address, size } => {
                if let Some(snapshot) = self.snapshots.values().last() {
                    let end = (*address + *size).min(snapshot.state_data.len());
                    if *address < snapshot.state_data.len() {
                        let data = snapshot.state_data[*address..end].to_vec();
                        InspectionResult::Memory {
                            address: *address,
                            data,
                        }
                    } else {
                        InspectionResult::Error("Address out of range".to_string())
                    }
                } else {
                    InspectionResult::Error("No snapshots available".to_string())
                }
            }
            InspectionQuery::ListVariables => {
                if let Some(snapshot) = self.snapshots.values().last() {
                    InspectionResult::Variables(snapshot.variables.clone())
                } else {
                    InspectionResult::Error("No snapshots available".to_string())
                }
            }
            InspectionQuery::GetCallStack => {
                // Simplified call stack - would use WASM debug info in real implementation
                InspectionResult::CallStack(vec![StackFrame {
                    frame_number: 0,
                    function_name: "agent_main".to_string(),
                    file_name: Some("agent.rs".to_string()),
                    line_number: Some(1),
                    locals: Vec::new(),
                }])
            }
        }
    }

    /// Get a specific snapshot by ID
    pub fn get_snapshot(&self, id: u64) -> Option<&StateSnapshot> {
        self.snapshots.get(&id)
    }

    /// Get the latest snapshot
    pub fn latest_snapshot(&self) -> Option<&StateSnapshot> {
        self.snapshots.values().max_by_key(|s| s.id)
    }

    /// Clear all snapshots
    pub fn clear_snapshots(&mut self) {
        self.snapshots.clear();
    }

    /// Get snapshot count
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Compare two snapshots
    pub fn compare_snapshots(&self, id1: u64, id2: u64) -> Option<SnapshotDiff> {
        let snap1 = self.snapshots.get(&id1)?;
        let snap2 = self.snapshots.get(&id2)?;

        Some(SnapshotDiff {
            snapshot1_id: id1,
            snapshot2_id: id2,
            memory_diff: Self::compute_memory_diff(&snap1.state_data, &snap2.state_data),
            variable_changes: Self::compute_variable_changes(&snap1.variables, &snap2.variables),
        })
    }

    /// Compute memory differences between two states
    fn compute_memory_diff(state1: &[u8], state2: &[u8]) -> Vec<MemoryChange> {
        let mut changes = Vec::new();
        let min_len = state1.len().min(state2.len());

        let mut start = None;
        for i in 0..min_len {
            if state1[i] != state2[i] {
                if start.is_none() {
                    start = Some(i);
                }
            } else if let Some(s) = start {
                changes.push(MemoryChange {
                    address: s,
                    size: i - s,
                    old_value: state1[s..i].to_vec(),
                    new_value: state2[s..i].to_vec(),
                });
                start = None;
            }
        }

        // Handle size changes
        if state1.len() != state2.len() {
            if let Some(s) = start {
                let end = state1.len().max(state2.len());
                changes.push(MemoryChange {
                    address: s,
                    size: end - s,
                    old_value: state1.get(s..).unwrap_or(&[]).to_vec(),
                    new_value: state2.get(s..).unwrap_or(&[]).to_vec(),
                });
            }
        }

        changes
    }

    /// Compute variable changes between two snapshots
    fn compute_variable_changes(
        vars1: &[VariableInfo],
        vars2: &[VariableInfo],
    ) -> Vec<VariableChange> {
        let mut changes = Vec::new();

        for var2 in vars2 {
            if let Some(var1) = vars1.iter().find(|v| v.name == var2.name) {
                if var1.value != var2.value {
                    changes.push(VariableChange {
                        name: var2.name.clone(),
                        old_value: var1.value.clone(),
                        new_value: var2.value.clone(),
                    });
                }
            } else {
                changes.push(VariableChange {
                    name: var2.name.clone(),
                    old_value: "".to_string(),
                    new_value: var2.value.clone(),
                });
            }
        }

        changes
    }
}

/// Difference between two snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiff {
    /// First snapshot ID
    pub snapshot1_id: u64,
    /// Second snapshot ID
    pub snapshot2_id: u64,
    /// Memory differences
    pub memory_diff: Vec<MemoryChange>,
    /// Variable changes
    pub variable_changes: Vec<VariableChange>,
}

/// Memory change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChange {
    /// Address where change occurred
    pub address: usize,
    /// Size of change
    pub size: usize,
    /// Old value
    pub old_value: Vec<u8>,
    /// New value
    pub new_value: Vec<u8>,
}

/// Variable change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableChange {
    /// Variable name
    pub name: String,
    /// Old value
    pub old_value: String,
    /// New value
    pub new_value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_inspector_creation() {
        let agent_id = [1u8; 16];
        let inspector = StateInspector::new(agent_id);

        assert_eq!(inspector.agent_id, agent_id);
        assert_eq!(inspector.snapshot_count(), 0);
    }

    #[test]
    fn test_state_inspector_capture_snapshot() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        let state_data = vec![1, 2, 3, 4, 5];
        let snapshot_id = inspector
            .capture_snapshot(state_data)
            .expect("Failed to capture snapshot");

        assert_eq!(inspector.snapshot_count(), 1);
        assert!(inspector.get_snapshot(snapshot_id).is_some());
    }

    #[test]
    fn test_state_inspector_query_full_snapshot() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        let state_data = vec![1, 2, 3, 4, 5];
        inspector.capture_snapshot(state_data).expect("Failed");

        let result = inspector.query(&InspectionQuery::FullSnapshot);
        assert!(matches!(result, InspectionResult::Snapshot(_)));
    }

    #[test]
    fn test_state_inspector_query_variables() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        let state_data = vec![1, 2, 3, 4, 5];
        inspector.capture_snapshot(state_data).expect("Failed");

        let result = inspector.query(&InspectionQuery::ListVariables);
        assert!(matches!(result, InspectionResult::Variables(_)));
    }

    #[test]
    fn test_state_inspector_query_memory() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        let state_data = vec![1, 2, 3, 4, 5];
        inspector.capture_snapshot(state_data).expect("Failed");

        let result = inspector.query(&InspectionQuery::GetMemory {
            address: 0,
            size: 3,
        });

        if let InspectionResult::Memory { address, data } = result {
            assert_eq!(address, 0);
            assert_eq!(data, vec![1, 2, 3]);
        } else {
            panic!("Expected Memory result");
        }
    }

    #[test]
    fn test_state_inspector_compare_snapshots() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        let id1 = inspector
            .capture_snapshot(vec![1, 2, 3, 4, 5])
            .expect("Failed");
        let id2 = inspector
            .capture_snapshot(vec![1, 2, 9, 4, 5])
            .expect("Failed");

        let diff = inspector.compare_snapshots(id1, id2);
        assert!(diff.is_some());

        let diff = diff.expect("Diff not found");
        assert!(!diff.memory_diff.is_empty());
    }

    #[test]
    fn test_state_inspector_clear_snapshots() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        inspector.capture_snapshot(vec![1, 2, 3]).expect("Failed");
        assert_eq!(inspector.snapshot_count(), 1);

        inspector.clear_snapshots();
        assert_eq!(inspector.snapshot_count(), 0);
    }

    #[test]
    fn test_state_inspector_latest_snapshot() {
        let agent_id = [1u8; 16];
        let mut inspector = StateInspector::new(agent_id);

        inspector.capture_snapshot(vec![1]).expect("Failed");
        let id2 = inspector.capture_snapshot(vec![2]).expect("Failed");

        let latest = inspector.latest_snapshot();
        assert!(latest.is_some());
        assert_eq!(latest.expect("Latest not found").id, id2);
    }
}
