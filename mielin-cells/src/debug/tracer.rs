//! Execution tracing for agents
//!
//! This module provides execution tracing to record and analyze agent behavior.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Execution tracer for recording trace events
#[derive(Debug)]
pub struct ExecutionTracer {
    /// Agent ID being traced
    #[allow(dead_code)]
    agent_id: [u8; 16],
    /// Recorded events
    events: VecDeque<TraceEvent>,
    /// Maximum events to keep
    max_events: usize,
    /// Filter for events
    filter: Option<TraceFilter>,
    /// Tracing enabled
    enabled: bool,
}

/// Trace event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceEvent {
    /// Function call
    FunctionCall {
        timestamp: u64,
        function_name: String,
        arguments: Vec<String>,
    },
    /// Function return
    FunctionReturn {
        timestamp: u64,
        function_name: String,
        return_value: Option<String>,
    },
    /// Memory allocation
    MemoryAllocation {
        timestamp: u64,
        address: usize,
        size: usize,
    },
    /// Memory deallocation
    MemoryDeallocation { timestamp: u64, address: usize },
    /// State change
    StateChange {
        timestamp: u64,
        variable_name: String,
        old_value: String,
        new_value: String,
    },
    /// Message sent
    MessageSent {
        timestamp: u64,
        destination: [u8; 16],
        size: usize,
    },
    /// Message received
    MessageReceived {
        timestamp: u64,
        source: [u8; 16],
        size: usize,
    },
    /// Custom event
    Custom {
        timestamp: u64,
        event_type: String,
        data: String,
    },
}

impl TraceEvent {
    /// Get the timestamp of this event
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::FunctionCall { timestamp, .. }
            | Self::FunctionReturn { timestamp, .. }
            | Self::MemoryAllocation { timestamp, .. }
            | Self::MemoryDeallocation { timestamp, .. }
            | Self::StateChange { timestamp, .. }
            | Self::MessageSent { timestamp, .. }
            | Self::MessageReceived { timestamp, .. }
            | Self::Custom { timestamp, .. } => *timestamp,
        }
    }

    /// Get the event type name
    pub fn event_type(&self) -> &str {
        match self {
            Self::FunctionCall { .. } => "FunctionCall",
            Self::FunctionReturn { .. } => "FunctionReturn",
            Self::MemoryAllocation { .. } => "MemoryAllocation",
            Self::MemoryDeallocation { .. } => "MemoryDeallocation",
            Self::StateChange { .. } => "StateChange",
            Self::MessageSent { .. } => "MessageSent",
            Self::MessageReceived { .. } => "MessageReceived",
            Self::Custom { .. } => "Custom",
        }
    }
}

/// Trace filter for selecting events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFilter {
    /// Include function calls
    pub include_function_calls: bool,
    /// Include memory events
    pub include_memory_events: bool,
    /// Include state changes
    pub include_state_changes: bool,
    /// Include messages
    pub include_messages: bool,
    /// Include custom events
    pub include_custom_events: bool,
    /// Filter by function name regex
    pub function_name_pattern: Option<String>,
}

impl Default for TraceFilter {
    fn default() -> Self {
        Self {
            include_function_calls: true,
            include_memory_events: true,
            include_state_changes: true,
            include_messages: true,
            include_custom_events: true,
            function_name_pattern: None,
        }
    }
}

impl TraceFilter {
    /// Check if an event passes the filter
    pub fn matches(&self, event: &TraceEvent) -> bool {
        match event {
            TraceEvent::FunctionCall { .. } | TraceEvent::FunctionReturn { .. } => {
                self.include_function_calls
            }
            TraceEvent::MemoryAllocation { .. } | TraceEvent::MemoryDeallocation { .. } => {
                self.include_memory_events
            }
            TraceEvent::StateChange { .. } => self.include_state_changes,
            TraceEvent::MessageSent { .. } | TraceEvent::MessageReceived { .. } => {
                self.include_messages
            }
            TraceEvent::Custom { .. } => self.include_custom_events,
        }
    }
}

impl ExecutionTracer {
    /// Create a new execution tracer
    pub fn new(agent_id: [u8; 16], max_events: usize) -> Self {
        Self {
            agent_id,
            events: VecDeque::with_capacity(max_events.min(10000)),
            max_events,
            filter: None,
            enabled: false,
        }
    }

    /// Enable tracing
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable tracing
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Set the trace filter
    pub fn set_filter(&mut self, filter: TraceFilter) {
        self.filter = Some(filter);
    }

    /// Clear the trace filter
    pub fn clear_filter(&mut self) {
        self.filter = None;
    }

    /// Record a trace event
    pub fn record(&mut self, event: TraceEvent) {
        if !self.enabled {
            return;
        }

        // Check filter
        if let Some(ref filter) = self.filter {
            if !filter.matches(&event) {
                return;
            }
        }

        self.events.push_back(event);

        // Limit size
        if self.events.len() > self.max_events {
            self.events.pop_front();
        }
    }

    /// Get all recorded events
    pub fn events(&self) -> Vec<&TraceEvent> {
        self.events.iter().collect()
    }

    /// Get events in a time range
    pub fn events_in_range(&self, start: u64, end: u64) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| {
                let ts = e.timestamp();
                ts >= start && ts <= end
            })
            .collect()
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Get event count
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

/// Trace recorder for batch recording
#[derive(Debug)]
pub struct TraceRecorder {
    /// Tracers by agent ID
    tracers: std::collections::HashMap<[u8; 16], ExecutionTracer>,
}

impl TraceRecorder {
    /// Create a new trace recorder
    pub fn new() -> Self {
        Self {
            tracers: std::collections::HashMap::new(),
        }
    }

    /// Add a tracer for an agent
    pub fn add_tracer(&mut self, agent_id: [u8; 16], max_events: usize) {
        self.tracers
            .insert(agent_id, ExecutionTracer::new(agent_id, max_events));
    }

    /// Get a tracer for an agent
    pub fn get_tracer(&self, agent_id: &[u8; 16]) -> Option<&ExecutionTracer> {
        self.tracers.get(agent_id)
    }

    /// Get a mutable tracer for an agent
    pub fn get_tracer_mut(&mut self, agent_id: &[u8; 16]) -> Option<&mut ExecutionTracer> {
        self.tracers.get_mut(agent_id)
    }

    /// Remove a tracer
    pub fn remove_tracer(&mut self, agent_id: &[u8; 16]) -> Option<ExecutionTracer> {
        self.tracers.remove(agent_id)
    }

    /// Get all agent IDs being traced
    pub fn traced_agents(&self) -> Vec<[u8; 16]> {
        self.tracers.keys().copied().collect()
    }

    /// Get total event count across all tracers
    pub fn total_events(&self) -> usize {
        self.tracers.values().map(|t| t.event_count()).sum()
    }
}

impl Default for TraceRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    #[test]
    fn test_execution_tracer_creation() {
        let agent_id = [1u8; 16];
        let tracer = ExecutionTracer::new(agent_id, 1000);

        assert_eq!(tracer.agent_id, agent_id);
        assert!(!tracer.enabled);
        assert_eq!(tracer.event_count(), 0);
    }

    #[test]
    fn test_execution_tracer_record() {
        let agent_id = [1u8; 16];
        let mut tracer = ExecutionTracer::new(agent_id, 1000);
        tracer.enable();

        let event = TraceEvent::FunctionCall {
            timestamp: current_timestamp(),
            function_name: "test_fn".to_string(),
            arguments: vec!["arg1".to_string()],
        };

        tracer.record(event);
        assert_eq!(tracer.event_count(), 1);
    }

    #[test]
    fn test_execution_tracer_filter() {
        let agent_id = [1u8; 16];
        let mut tracer = ExecutionTracer::new(agent_id, 1000);
        tracer.enable();

        let filter = TraceFilter {
            include_function_calls: false,
            ..Default::default()
        };
        tracer.set_filter(filter);

        tracer.record(TraceEvent::FunctionCall {
            timestamp: current_timestamp(),
            function_name: "test".to_string(),
            arguments: vec![],
        });

        assert_eq!(tracer.event_count(), 0);

        tracer.record(TraceEvent::StateChange {
            timestamp: current_timestamp(),
            variable_name: "x".to_string(),
            old_value: "1".to_string(),
            new_value: "2".to_string(),
        });

        assert_eq!(tracer.event_count(), 1);
    }

    #[test]
    fn test_trace_recorder() {
        let mut recorder = TraceRecorder::new();
        let agent_id = [1u8; 16];

        recorder.add_tracer(agent_id, 1000);

        assert!(recorder.get_tracer(&agent_id).is_some());

        let removed = recorder.remove_tracer(&agent_id);
        assert!(removed.is_some());
    }

    #[test]
    fn test_trace_event_type() {
        let event = TraceEvent::FunctionCall {
            timestamp: current_timestamp(),
            function_name: "test".to_string(),
            arguments: vec![],
        };

        assert_eq!(event.event_type(), "FunctionCall");
    }
}
