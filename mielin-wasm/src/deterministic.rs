//! Deterministic Execution Framework for MielinOS WASM Runtime
//!
//! This module provides deterministic execution capabilities for reproducible
//! testing, debugging, and verification of WASM modules.
//!
//! # Features
//!
//! - **Event Recording**: Record all non-deterministic events
//! - **Replay**: Reproduce exact execution from recorded events
//! - **Checkpoints**: Save and restore execution state
//! - **Deterministic Time**: Controlled time progression
//! - **Deterministic Random**: Reproducible random number generation
//! - **Call Trace**: Complete execution trace for analysis
//!
//! # Example
//!
//! ```rust,ignore
//! use mielin_wasm::deterministic::{DeterministicContext, RecordMode};
//!
//! // Create recording context
//! let mut ctx = DeterministicContext::new(RecordMode::Record);
//!
//! // Execute with recording
//! ctx.execute(instance, func_name)?;
//!
//! // Save trace
//! ctx.save_trace("execution.trace")?;
//!
//! // Later: replay execution
//! let mut replay_ctx = DeterministicContext::from_trace("execution.trace")?;
//! replay_ctx.execute(instance, func_name)?; // Exact same behavior
//! ```

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Record mode for deterministic execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordMode {
    /// Record all non-deterministic events
    Record,
    /// Replay from recorded events
    Replay,
    /// Normal execution (no recording/replay)
    Normal,
}

/// Non-deterministic event types
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Time query event (timestamp)
    TimeQuery(u64),
    /// Random number generation (value)
    RandomValue(u64),
    /// External call (function name, args, result)
    ExternalCall(String, Vec<u8>, Vec<u8>),
    /// Memory operation (address, old value, new value)
    MemoryWrite(u32, Vec<u8>, Vec<u8>),
    /// Function call (name, depth)
    FunctionCall(String, usize),
    /// Function return (name, depth)
    FunctionReturn(String, usize),
}

/// Execution checkpoint
#[derive(Debug, Clone)]
pub struct Checkpoint {
    /// Checkpoint ID
    pub id: String,
    /// Timestamp when checkpoint was created
    pub timestamp: u64,
    /// Event index at checkpoint
    pub event_index: usize,
    /// Call stack depth
    pub call_depth: usize,
    /// Memory snapshot (address -> value)
    pub memory_state: HashMap<u32, u8>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Execution trace
#[derive(Debug, Clone)]
pub struct ExecutionTrace {
    /// All recorded events
    pub events: Vec<Event>,
    /// Checkpoints in this trace
    pub checkpoints: Vec<Checkpoint>,
    /// Initial timestamp
    pub start_time: u64,
    /// Final timestamp
    pub end_time: Option<u64>,
    /// Total events recorded
    pub total_events: usize,
}

impl ExecutionTrace {
    /// Create new empty trace
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_nanos() as u64;

        Self {
            events: Vec::new(),
            checkpoints: Vec::new(),
            start_time,
            end_time: None,
            total_events: 0,
        }
    }

    /// Add event to trace
    pub fn record_event(&mut self, event: Event) {
        self.events.push(event);
        self.total_events += 1;
    }

    /// Add checkpoint
    pub fn add_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.checkpoints.push(checkpoint);
    }

    /// Finalize trace
    pub fn finalize(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        self.end_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("SystemTime before UNIX_EPOCH")
                .as_nanos() as u64,
        );
    }

    /// Get event at index
    pub fn get_event(&self, index: usize) -> Option<&Event> {
        self.events.get(index)
    }

    /// Find checkpoint by ID
    pub fn find_checkpoint(&self, id: &str) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|cp| cp.id == id)
    }

    /// Get execution duration
    pub fn duration_nanos(&self) -> Option<u64> {
        self.end_time.map(|end| end.saturating_sub(self.start_time))
    }
}

impl Default for ExecutionTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// Deterministic execution context
pub struct DeterministicContext {
    /// Current recording mode
    mode: RecordMode,
    /// Execution trace
    trace: ExecutionTrace,
    /// Current event index (for replay)
    replay_index: usize,
    /// Current call depth
    call_depth: usize,
    /// Deterministic time counter
    time_counter: u64,
    /// Deterministic random state (XorShift128+)
    random_state: (u64, u64),
}

impl DeterministicContext {
    /// Create new deterministic context
    pub fn new(mode: RecordMode) -> Self {
        Self {
            mode,
            trace: ExecutionTrace::new(),
            replay_index: 0,
            call_depth: 0,
            time_counter: 0,
            random_state: (0x123456789ABCDEF0, 0xFEDCBA9876543210),
        }
    }

    /// Create context from saved trace (for replay)
    pub fn from_trace_file(path: impl AsRef<Path>) -> Result<Self> {
        let trace = Self::load_trace(path)?;
        Ok(Self {
            mode: RecordMode::Replay,
            trace,
            replay_index: 0,
            call_depth: 0,
            time_counter: 0,
            random_state: (0x123456789ABCDEF0, 0xFEDCBA9876543210),
        })
    }

    /// Record time query
    pub fn record_time_query(&mut self) -> u64 {
        match self.mode {
            RecordMode::Record => {
                let value = self.time_counter;
                self.trace.record_event(Event::TimeQuery(value));
                self.time_counter += 1000; // Increment by 1μs
                value
            }
            RecordMode::Replay => {
                if let Some(Event::TimeQuery(value)) = self.trace.get_event(self.replay_index) {
                    self.replay_index += 1;
                    *value
                } else {
                    panic!("Replay mismatch: expected TimeQuery");
                }
            }
            RecordMode::Normal => {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("SystemTime before UNIX_EPOCH")
                    .as_nanos() as u64
            }
        }
    }

    /// Record random value generation
    pub fn record_random(&mut self) -> u64 {
        match self.mode {
            RecordMode::Record => {
                let value = self.next_random();
                self.trace.record_event(Event::RandomValue(value));
                value
            }
            RecordMode::Replay => {
                if let Some(Event::RandomValue(value)) = self.trace.get_event(self.replay_index) {
                    self.replay_index += 1;
                    *value
                } else {
                    panic!("Replay mismatch: expected RandomValue");
                }
            }
            RecordMode::Normal => self.next_random(),
        }
    }

    /// Generate next random number (XorShift128+)
    fn next_random(&mut self) -> u64 {
        let mut s1 = self.random_state.0;
        let s0 = self.random_state.1;
        self.random_state.0 = s0;
        s1 ^= s1 << 23;
        s1 ^= s1 >> 17;
        s1 ^= s0;
        s1 ^= s0 >> 26;
        self.random_state.1 = s1;
        s0.wrapping_add(s1)
    }

    /// Record function call
    pub fn record_function_call(&mut self, name: impl Into<String>) {
        let name = name.into();
        if self.mode == RecordMode::Record {
            self.trace
                .record_event(Event::FunctionCall(name, self.call_depth));
        }
        self.call_depth += 1;
    }

    /// Record function return
    pub fn record_function_return(&mut self, name: impl Into<String>) {
        if self.call_depth > 0 {
            self.call_depth -= 1;
        }
        let name = name.into();
        if self.mode == RecordMode::Record {
            self.trace
                .record_event(Event::FunctionReturn(name, self.call_depth));
        }
    }

    /// Create checkpoint
    pub fn create_checkpoint(&mut self, id: impl Into<String>) -> Checkpoint {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_secs();

        let checkpoint = Checkpoint {
            id: id.into(),
            timestamp,
            event_index: self.trace.total_events,
            call_depth: self.call_depth,
            memory_state: HashMap::new(),
            metadata: HashMap::new(),
        };

        self.trace.add_checkpoint(checkpoint.clone());
        checkpoint
    }

    /// Restore from checkpoint
    pub fn restore_checkpoint(&mut self, id: &str) -> Result<()> {
        let checkpoint = self
            .trace
            .find_checkpoint(id)
            .ok_or_else(|| anyhow::anyhow!("Checkpoint not found: {}", id))?;

        self.replay_index = checkpoint.event_index;
        self.call_depth = checkpoint.call_depth;
        Ok(())
    }

    /// Save trace to file
    pub fn save_trace(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let mut file = File::create(path).context("Failed to create trace file")?;

        // Write magic number
        file.write_all(b"MIELINTRC")?;

        // Write version
        file.write_all(&[0, 1, 0, 0])?;

        // Write mode
        let mode_byte = match self.mode {
            RecordMode::Record => 0u8,
            RecordMode::Replay => 1u8,
            RecordMode::Normal => 2u8,
        };
        file.write_all(&[mode_byte])?;

        // Write trace metadata
        file.write_all(&self.trace.start_time.to_le_bytes())?;
        file.write_all(&self.trace.end_time.unwrap_or(0).to_le_bytes())?;
        file.write_all(&(self.trace.total_events as u64).to_le_bytes())?;

        // Write events count
        file.write_all(&(self.trace.events.len() as u64).to_le_bytes())?;

        // Write events (simplified serialization)
        for event in &self.trace.events {
            match event {
                Event::TimeQuery(val) => {
                    file.write_all(&[0u8])?; // Type ID
                    file.write_all(&val.to_le_bytes())?;
                }
                Event::RandomValue(val) => {
                    file.write_all(&[1u8])?;
                    file.write_all(&val.to_le_bytes())?;
                }
                Event::FunctionCall(name, depth) => {
                    file.write_all(&[2u8])?;
                    let name_bytes = name.as_bytes();
                    file.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
                    file.write_all(name_bytes)?;
                    file.write_all(&(*depth as u32).to_le_bytes())?;
                }
                Event::FunctionReturn(name, depth) => {
                    file.write_all(&[3u8])?;
                    let name_bytes = name.as_bytes();
                    file.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
                    file.write_all(name_bytes)?;
                    file.write_all(&(*depth as u32).to_le_bytes())?;
                }
                _ => {
                    // Other events not yet serialized
                    file.write_all(&[255u8])?; // Unknown event
                }
            }
        }

        file.sync_all()?;
        Ok(())
    }

    /// Load trace from file
    pub fn load_trace(path: impl AsRef<Path>) -> Result<ExecutionTrace> {
        let path = path.as_ref();
        let mut file = File::open(path).context("Failed to open trace file")?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let mut offset = 0;

        // Read magic number
        if &buffer[offset..offset + 9] != b"MIELINTRC" {
            bail!("Invalid trace file: bad magic number");
        }
        offset += 9;

        // Read version
        let _version = &buffer[offset..offset + 4];
        offset += 4;

        // Read mode
        let _mode = buffer[offset];
        offset += 1;

        // Read metadata
        let mut ts_bytes = [0u8; 8];
        ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
        let start_time = u64::from_le_bytes(ts_bytes);
        offset += 8;

        ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
        let end_time_val = u64::from_le_bytes(ts_bytes);
        let end_time = if end_time_val > 0 {
            Some(end_time_val)
        } else {
            None
        };
        offset += 8;

        ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
        let total_events = u64::from_le_bytes(ts_bytes) as usize;
        offset += 8;

        // Read events count
        ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
        let events_count = u64::from_le_bytes(ts_bytes) as usize;
        offset += 8;

        // Read events
        let mut events = Vec::with_capacity(events_count);
        for _ in 0..events_count {
            let event_type = buffer[offset];
            offset += 1;

            let event = match event_type {
                0 => {
                    // TimeQuery
                    ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
                    let val = u64::from_le_bytes(ts_bytes);
                    offset += 8;
                    Event::TimeQuery(val)
                }
                1 => {
                    // RandomValue
                    ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
                    let val = u64::from_le_bytes(ts_bytes);
                    offset += 8;
                    Event::RandomValue(val)
                }
                2 | 3 => {
                    // FunctionCall or FunctionReturn
                    let mut len_bytes = [0u8; 4];
                    len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
                    let name_len = u32::from_le_bytes(len_bytes) as usize;
                    offset += 4;

                    let name = String::from_utf8(buffer[offset..offset + name_len].to_vec())?;
                    offset += name_len;

                    len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
                    let depth = u32::from_le_bytes(len_bytes) as usize;
                    offset += 4;

                    if event_type == 2 {
                        Event::FunctionCall(name, depth)
                    } else {
                        Event::FunctionReturn(name, depth)
                    }
                }
                _ => {
                    // Unknown event, skip
                    continue;
                }
            };

            events.push(event);
        }

        Ok(ExecutionTrace {
            events,
            checkpoints: Vec::new(),
            start_time,
            end_time,
            total_events,
        })
    }

    /// Get current mode
    pub fn mode(&self) -> RecordMode {
        self.mode
    }

    /// Get trace
    pub fn trace(&self) -> &ExecutionTrace {
        &self.trace
    }

    /// Finalize recording
    pub fn finalize(&mut self) {
        self.trace.finalize();
    }

    /// Get execution statistics
    pub fn stats(&self) -> DeterministicStats {
        let time_queries = self
            .trace
            .events
            .iter()
            .filter(|e| matches!(e, Event::TimeQuery(_)))
            .count();

        let random_calls = self
            .trace
            .events
            .iter()
            .filter(|e| matches!(e, Event::RandomValue(_)))
            .count();

        let function_calls = self
            .trace
            .events
            .iter()
            .filter(|e| matches!(e, Event::FunctionCall(_, _)))
            .count();

        DeterministicStats {
            total_events: self.trace.total_events,
            time_queries,
            random_calls,
            function_calls,
            checkpoints: self.trace.checkpoints.len(),
            duration_nanos: self.trace.duration_nanos(),
        }
    }
}

/// Deterministic execution statistics
#[derive(Debug, Clone)]
pub struct DeterministicStats {
    /// Total events recorded
    pub total_events: usize,
    /// Number of time queries
    pub time_queries: usize,
    /// Number of random calls
    pub random_calls: usize,
    /// Number of function calls
    pub function_calls: usize,
    /// Number of checkpoints
    pub checkpoints: usize,
    /// Execution duration in nanoseconds
    pub duration_nanos: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_context_creation() {
        let ctx = DeterministicContext::new(RecordMode::Record);
        assert_eq!(ctx.mode(), RecordMode::Record);
        assert_eq!(ctx.trace().total_events, 0);
    }

    #[test]
    fn test_time_recording() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        let t1 = ctx.record_time_query();
        let t2 = ctx.record_time_query();
        let t3 = ctx.record_time_query();

        assert_eq!(t1, 0);
        assert_eq!(t2, 1000);
        assert_eq!(t3, 2000);
        assert_eq!(ctx.trace().total_events, 3);
    }

    #[test]
    fn test_random_recording() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        let r1 = ctx.record_random();
        let r2 = ctx.record_random();

        assert_ne!(r1, r2); // Should be different
        assert_eq!(ctx.trace().total_events, 2);
    }

    #[test]
    fn test_function_call_recording() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        ctx.record_function_call("test_func");
        assert_eq!(ctx.call_depth, 1);

        ctx.record_function_return("test_func");
        assert_eq!(ctx.call_depth, 0);
        assert_eq!(ctx.trace().total_events, 2);
    }

    #[test]
    fn test_checkpoint_creation() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        ctx.record_time_query();
        ctx.record_random();

        let checkpoint = ctx.create_checkpoint("cp1");
        assert_eq!(checkpoint.id, "cp1");
        assert_eq!(checkpoint.event_index, 2);
    }

    #[test]
    fn test_checkpoint_restore() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        ctx.record_time_query();
        ctx.create_checkpoint("cp1");
        ctx.record_random();

        let result = ctx.restore_checkpoint("cp1");
        assert!(result.is_ok());
        assert_eq!(ctx.replay_index, 1);
    }

    #[test]
    fn test_trace_serialization() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        ctx.record_time_query();
        ctx.record_random();
        ctx.record_function_call("add");
        ctx.record_function_return("add");
        ctx.finalize();

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_trace.trc");

        // Save
        let result = ctx.save_trace(&path);
        assert!(result.is_ok());
        assert!(path.exists());

        // Load
        let loaded = DeterministicContext::load_trace(&path);
        assert!(loaded.is_ok());

        let trace = loaded.unwrap();
        assert_eq!(trace.total_events, 4);

        // Cleanup
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_replay_determinism() {
        let mut record_ctx = DeterministicContext::new(RecordMode::Record);

        // Record some operations
        let t1 = record_ctx.record_time_query();
        let r1 = record_ctx.record_random();
        let t2 = record_ctx.record_time_query();

        // Save trace
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("replay_test.trc");
        record_ctx.save_trace(&path).unwrap();

        // Replay
        let mut replay_ctx = DeterministicContext::from_trace_file(&path).unwrap();

        let t1_replay = replay_ctx.record_time_query();
        let r1_replay = replay_ctx.record_random();
        let t2_replay = replay_ctx.record_time_query();

        assert_eq!(t1, t1_replay);
        assert_eq!(r1, r1_replay);
        assert_eq!(t2, t2_replay);

        // Cleanup
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_statistics() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        ctx.record_time_query();
        ctx.record_time_query();
        ctx.record_random();
        ctx.record_function_call("test");
        ctx.record_function_return("test");
        ctx.create_checkpoint("cp1");

        let stats = ctx.stats();
        assert_eq!(stats.total_events, 5);
        assert_eq!(stats.time_queries, 2);
        assert_eq!(stats.random_calls, 1);
        assert_eq!(stats.function_calls, 1);
        assert_eq!(stats.checkpoints, 1);
    }

    #[test]
    fn test_nested_function_calls() {
        let mut ctx = DeterministicContext::new(RecordMode::Record);

        ctx.record_function_call("outer");
        assert_eq!(ctx.call_depth, 1);

        ctx.record_function_call("inner");
        assert_eq!(ctx.call_depth, 2);

        ctx.record_function_return("inner");
        assert_eq!(ctx.call_depth, 1);

        ctx.record_function_return("outer");
        assert_eq!(ctx.call_depth, 0);
    }
}
