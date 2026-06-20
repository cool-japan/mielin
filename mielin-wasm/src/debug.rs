//! Debugging Support for WASM Modules
//!
//! This module provides debugging capabilities including:
//! - Source map parsing and storage
//! - Enhanced stack traces with source locations
//! - Memory inspection and dumping
//! - Breakpoint management
//! - Step-by-step execution support
//!
//! # Architecture
//!
//! The debugging system is designed to work with wasmtime's debugging
//! capabilities and extends them with MielinOS-specific features.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Instance, Store, Trap};

use crate::host::HostState;

/// Source map entry mapping WASM bytecode offset to source location
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMapEntry {
    /// WASM bytecode offset
    pub wasm_offset: usize,
    /// Source file path
    pub file: String,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Function name (if available)
    pub function_name: Option<String>,
}

/// Source map for a WASM module
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Module name
    module_name: String,
    /// Mapping from WASM offset to source location
    entries: Vec<SourceMapEntry>,
    /// Source file contents (for inline display)
    sources: HashMap<String, Vec<String>>,
}

impl SourceMap {
    /// Create a new empty source map
    pub fn new(module_name: String) -> Self {
        Self {
            module_name,
            entries: Vec::new(),
            sources: HashMap::new(),
        }
    }

    /// Add a source map entry
    pub fn add_entry(&mut self, entry: SourceMapEntry) {
        self.entries.push(entry);
        // Keep entries sorted by WASM offset
        self.entries.sort_by_key(|e| e.wasm_offset);
    }

    /// Add source file content
    pub fn add_source(&mut self, file: String, content: String) {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        self.sources.insert(file, lines);
    }

    /// Find source location for a given WASM offset
    pub fn lookup(&self, wasm_offset: usize) -> Option<&SourceMapEntry> {
        // Binary search for the entry with largest offset <= wasm_offset
        let idx = self
            .entries
            .binary_search_by_key(&wasm_offset, |e| e.wasm_offset);

        match idx {
            Ok(i) => Some(&self.entries[i]),
            Err(i) => {
                if i > 0 {
                    Some(&self.entries[i - 1])
                } else {
                    None
                }
            }
        }
    }

    /// Get source line content
    pub fn get_source_line(&self, file: &str, line: u32) -> Option<&str> {
        self.sources
            .get(file)
            .and_then(|lines| lines.get((line - 1) as usize))
            .map(|s| s.as_str())
    }

    /// Get module name
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Get all entries
    pub fn entries(&self) -> &[SourceMapEntry] {
        &self.entries
    }
}

/// Enhanced stack frame with source information
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function index
    pub func_index: u32,
    /// Function name
    pub func_name: Option<String>,
    /// WASM bytecode offset
    pub wasm_offset: usize,
    /// Source location (if available)
    pub source_location: Option<SourceMapEntry>,
    /// Module name
    pub module_name: String,
}

impl StackFrame {
    /// Format frame for display
    pub fn format(&self) -> String {
        let func_name = self.func_name.as_deref().unwrap_or("<anonymous>");

        if let Some(ref loc) = self.source_location {
            format!(
                "  at {} ({}:{}:{})",
                func_name, loc.file, loc.line, loc.column
            )
        } else {
            format!(
                "  at {} (wasm-function[{}]:0x{:x})",
                func_name, self.func_index, self.wasm_offset
            )
        }
    }
}

/// Enhanced stack trace with source mapping
#[derive(Debug, Clone)]
pub struct StackTrace {
    /// Error message
    pub message: String,
    /// Stack frames
    pub frames: Vec<StackFrame>,
}

impl StackTrace {
    /// Create from a wasmtime Trap
    pub fn from_trap(trap: &Trap, _source_map: Option<&SourceMap>) -> Self {
        let message = trap.to_string();
        let frames = Vec::new(); // Will be populated by wasmtime backtrace

        Self { message, frames }
    }

    /// Format the entire stack trace
    pub fn format(&self) -> String {
        let mut output = format!("Error: {}\n", self.message);
        for frame in &self.frames {
            output.push_str(&frame.format());
            output.push('\n');
        }
        output
    }

    /// Add a frame to the stack trace
    pub fn add_frame(&mut self, frame: StackFrame) {
        self.frames.push(frame);
    }
}

/// Memory inspector for examining WASM linear memory
pub struct MemoryInspector {
    /// Start address to inspect
    start_addr: usize,
    /// Length to inspect
    length: usize,
    /// Bytes per row for hex dump
    bytes_per_row: usize,
}

impl MemoryInspector {
    /// Create a new memory inspector
    pub fn new(start_addr: usize, length: usize) -> Self {
        Self {
            start_addr,
            length,
            bytes_per_row: 16,
        }
    }

    /// Set bytes per row for display
    pub fn with_bytes_per_row(mut self, bytes_per_row: usize) -> Self {
        self.bytes_per_row = bytes_per_row;
        self
    }

    /// Dump memory as hexadecimal
    pub fn dump_hex(&self, memory: &[u8]) -> String {
        let mut output = String::new();
        let end_addr = (self.start_addr + self.length).min(memory.len());

        for addr in (self.start_addr..end_addr).step_by(self.bytes_per_row) {
            // Address
            output.push_str(&format!("{:08x}  ", addr));

            // Hex bytes
            for i in 0..self.bytes_per_row {
                if addr + i < end_addr {
                    output.push_str(&format!("{:02x} ", memory[addr + i]));
                } else {
                    output.push_str("   ");
                }

                // Extra space in the middle
                if i == self.bytes_per_row / 2 - 1 {
                    output.push(' ');
                }
            }

            // ASCII representation
            output.push_str(" |");
            for i in 0..self.bytes_per_row {
                if addr + i < end_addr {
                    let byte = memory[addr + i];
                    if (32..127).contains(&byte) {
                        output.push(byte as char);
                    } else {
                        output.push('.');
                    }
                }
            }
            output.push('|');
            output.push('\n');
        }

        output
    }

    /// Read memory as typed values
    pub fn read_u32(&self, memory: &[u8], offset: usize) -> Result<u32> {
        let addr = self.start_addr + offset;
        if addr + 4 > memory.len() {
            return Err(anyhow!("Address out of bounds"));
        }

        let bytes = [
            memory[addr],
            memory[addr + 1],
            memory[addr + 2],
            memory[addr + 3],
        ];
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read memory as i32
    pub fn read_i32(&self, memory: &[u8], offset: usize) -> Result<i32> {
        self.read_u32(memory, offset).map(|v| v as i32)
    }

    /// Read memory as f32
    pub fn read_f32(&self, memory: &[u8], offset: usize) -> Result<f32> {
        self.read_u32(memory, offset).map(f32::from_bits)
    }

    /// Read memory as u64
    pub fn read_u64(&self, memory: &[u8], offset: usize) -> Result<u64> {
        let addr = self.start_addr + offset;
        if addr + 8 > memory.len() {
            return Err(anyhow!("Address out of bounds"));
        }

        let bytes = [
            memory[addr],
            memory[addr + 1],
            memory[addr + 2],
            memory[addr + 3],
            memory[addr + 4],
            memory[addr + 5],
            memory[addr + 6],
            memory[addr + 7],
        ];
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read memory as i64
    pub fn read_i64(&self, memory: &[u8], offset: usize) -> Result<i64> {
        self.read_u64(memory, offset).map(|v| v as i64)
    }

    /// Read memory as f64
    pub fn read_f64(&self, memory: &[u8], offset: usize) -> Result<f64> {
        self.read_u64(memory, offset).map(f64::from_bits)
    }

    /// Read null-terminated string
    pub fn read_cstring(&self, memory: &[u8], offset: usize) -> Result<String> {
        let addr = self.start_addr + offset;
        if addr >= memory.len() {
            return Err(anyhow!("Address out of bounds"));
        }

        let mut bytes = Vec::new();
        let mut current = addr;

        while current < memory.len() {
            let byte = memory[current];
            if byte == 0 {
                break;
            }
            bytes.push(byte);
            current += 1;

            // Prevent infinite loops
            if bytes.len() > 4096 {
                return Err(anyhow!("String too long (> 4KB)"));
            }
        }

        String::from_utf8(bytes).map_err(|e| anyhow!("Invalid UTF-8: {}", e))
    }
}

/// Breakpoint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointType {
    /// Break at specific WASM offset
    Offset(usize),
    /// Break at function entry
    FunctionEntry(u32),
    /// Break at source location
    SourceLocation { line: u32, column: u32 },
}

/// Breakpoint state
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Breakpoint ID
    pub id: usize,
    /// Breakpoint type
    pub bp_type: BreakpointType,
    /// Whether breakpoint is enabled
    pub enabled: bool,
    /// Hit count
    pub hit_count: usize,
    /// Condition (future: could be WASM expression)
    pub condition: Option<String>,
}

/// Debugger state for step-by-step execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebuggerState {
    /// Running normally
    Running,
    /// Stopped at breakpoint
    Stopped,
    /// Single-stepping
    Stepping,
}

/// Debugger context
pub struct DebugContext {
    /// Source maps for modules
    source_maps: HashMap<String, SourceMap>,
    /// Breakpoints
    breakpoints: Arc<Mutex<HashMap<usize, Breakpoint>>>,
    /// Next breakpoint ID
    next_bp_id: usize,
    /// Current debugger state
    state: DebuggerState,
}

impl DebugContext {
    /// Create a new debug context
    pub fn new() -> Self {
        Self {
            source_maps: HashMap::new(),
            breakpoints: Arc::new(Mutex::new(HashMap::new())),
            next_bp_id: 1,
            state: DebuggerState::Running,
        }
    }

    /// Add a source map for a module
    pub fn add_source_map(&mut self, module_name: String, source_map: SourceMap) {
        self.source_maps.insert(module_name, source_map);
    }

    /// Get source map for a module
    pub fn get_source_map(&self, module_name: &str) -> Option<&SourceMap> {
        self.source_maps.get(module_name)
    }

    /// Add a breakpoint
    pub fn add_breakpoint(&mut self, bp_type: BreakpointType) -> usize {
        let id = self.next_bp_id;
        self.next_bp_id += 1;

        let bp = Breakpoint {
            id,
            bp_type,
            enabled: true,
            hit_count: 0,
            condition: None,
        };

        self.breakpoints
            .lock()
            .expect("Breakpoints lock poisoned")
            .insert(id, bp);
        id
    }

    /// Remove a breakpoint
    pub fn remove_breakpoint(&mut self, id: usize) -> bool {
        self.breakpoints
            .lock()
            .expect("Breakpoints lock poisoned")
            .remove(&id)
            .is_some()
    }

    /// Enable/disable a breakpoint
    pub fn set_breakpoint_enabled(&mut self, id: usize, enabled: bool) -> Result<()> {
        let mut bps = self.breakpoints.lock().expect("Breakpoints lock poisoned");
        if let Some(bp) = bps.get_mut(&id) {
            bp.enabled = enabled;
            Ok(())
        } else {
            Err(anyhow!("Breakpoint {} not found", id))
        }
    }

    /// List all breakpoints
    pub fn list_breakpoints(&self) -> Vec<Breakpoint> {
        self.breakpoints
            .lock()
            .expect("Breakpoints lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Get current debugger state
    pub fn state(&self) -> DebuggerState {
        self.state
    }

    /// Set debugger state
    pub fn set_state(&mut self, state: DebuggerState) {
        self.state = state;
    }

    /// Check if execution should stop at current location
    pub fn should_stop(&self, _wasm_offset: usize, _func_index: u32) -> bool {
        // This would be called during WASM execution to check breakpoints
        // For now, simplified implementation
        matches!(self.state, DebuggerState::Stepping)
    }
}

impl Default for DebugContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract memory from store for inspection
pub fn get_memory_data(store: &mut Store<HostState>, instance: &Instance) -> Option<Vec<u8>> {
    let memory = instance.get_export(&mut *store, "memory")?;
    if let wasmtime::Extern::Memory(mem) = memory {
        Some(mem.data(store).to_vec())
    } else {
        None
    }
}

/// Inspect memory at a specific address
pub fn inspect_memory(
    store: &mut Store<HostState>,
    instance: &Instance,
    addr: usize,
    len: usize,
) -> Result<String> {
    let memory_data =
        get_memory_data(store, instance).ok_or_else(|| anyhow!("No memory export found"))?;

    let inspector = MemoryInspector::new(addr, len);
    Ok(inspector.dump_hex(&memory_data))
}

// ============================================================================
// Advanced Debugging Features
// ============================================================================

/// Variable type for inspection
#[derive(Debug, Clone, PartialEq)]
pub enum VariableType {
    /// 32-bit integer
    I32,
    /// 64-bit integer
    I64,
    /// 32-bit float
    F32,
    /// 64-bit float
    F64,
    /// Memory address
    Pointer,
    /// Unknown/unsupported type
    Unknown,
}

/// Variable value representation
#[derive(Debug, Clone, PartialEq)]
pub enum VariableValue {
    /// 32-bit integer
    I32(i32),
    /// 64-bit integer
    I64(i64),
    /// 32-bit float
    F32(f32),
    /// 64-bit float
    F64(f64),
    /// Memory pointer
    Pointer(u32),
    /// Unavailable (out of scope)
    Unavailable,
}

impl VariableValue {
    /// Format value for display
    pub fn format(&self) -> String {
        match self {
            VariableValue::I32(v) => format!("{}", v),
            VariableValue::I64(v) => format!("{}", v),
            VariableValue::F32(v) => format!("{}", v),
            VariableValue::F64(v) => format!("{}", v),
            VariableValue::Pointer(v) => format!("0x{:08x}", v),
            VariableValue::Unavailable => "<unavailable>".to_string(),
        }
    }
}

/// Variable information for debugging
#[derive(Debug, Clone)]
pub struct Variable {
    /// Variable name
    pub name: String,
    /// Variable type
    pub var_type: VariableType,
    /// Current value
    pub value: VariableValue,
    /// Scope depth (0 = global, 1+ = local)
    pub scope_depth: u32,
    /// Memory location (if applicable)
    pub memory_location: Option<u32>,
}

impl Variable {
    /// Create a new variable
    pub fn new(name: impl Into<String>, var_type: VariableType, value: VariableValue) -> Self {
        Self {
            name: name.into(),
            var_type,
            value,
            scope_depth: 0,
            memory_location: None,
        }
    }

    /// Create variable with scope
    pub fn with_scope(mut self, scope_depth: u32) -> Self {
        self.scope_depth = scope_depth;
        self
    }

    /// Create variable with memory location
    pub fn with_memory_location(mut self, location: u32) -> Self {
        self.memory_location = Some(location);
        self
    }

    /// Format variable for display
    pub fn format(&self) -> String {
        let type_str = match self.var_type {
            VariableType::I32 => "i32",
            VariableType::I64 => "i64",
            VariableType::F32 => "f32",
            VariableType::F64 => "f64",
            VariableType::Pointer => "ptr",
            VariableType::Unknown => "unknown",
        };

        format!("{}: {} = {}", self.name, type_str, self.value.format())
    }
}

/// Watch expression for tracking values
#[derive(Debug, Clone)]
pub struct WatchExpression {
    /// Watch ID
    pub id: usize,
    /// Expression to watch (variable name or memory address)
    pub expression: String,
    /// Last known value
    pub last_value: Option<VariableValue>,
    /// Number of times value has changed
    pub change_count: usize,
    /// Whether watch is enabled
    pub enabled: bool,
}

impl WatchExpression {
    /// Create new watch expression
    pub fn new(id: usize, expression: impl Into<String>) -> Self {
        Self {
            id,
            expression: expression.into(),
            last_value: None,
            change_count: 0,
            enabled: true,
        }
    }

    /// Update watch with new value
    pub fn update(&mut self, value: VariableValue) -> bool {
        let changed = match (&self.last_value, &value) {
            (Some(old), new) => old != new,
            (None, _) => true,
        };

        if changed {
            self.change_count += 1;
            self.last_value = Some(value);
        }

        changed
    }

    /// Get value change description
    pub fn describe_change(&self, new_value: &VariableValue) -> String {
        match &self.last_value {
            Some(old) => format!(
                "{}: {} -> {}",
                self.expression,
                old.format(),
                new_value.format()
            ),
            None => format!("{}: {}", self.expression, new_value.format()),
        }
    }
}

/// Step mode for debugging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Step to next instruction
    StepInstruction,
    /// Step over function calls
    StepOver,
    /// Step into function calls
    StepInto,
    /// Step out of current function
    StepOut,
    /// Continue until breakpoint
    Continue,
}

/// Breakpoint injection result
#[derive(Debug, Clone)]
pub struct InjectionResult {
    /// Number of breakpoints injected
    pub injected_count: usize,
    /// Modified WASM bytecode
    pub modified_wasm: Vec<u8>,
    /// Mapping from original to modified offsets
    pub offset_map: HashMap<usize, usize>,
}

/// Breakpoint injector for modifying WASM bytecode
pub struct BreakpointInjector {
    /// Original WASM bytecode
    original_wasm: Vec<u8>,
    /// Breakpoint locations (WASM offsets)
    breakpoint_offsets: Vec<usize>,
}

impl BreakpointInjector {
    /// Create new breakpoint injector
    pub fn new(wasm_bytes: Vec<u8>) -> Self {
        Self {
            original_wasm: wasm_bytes,
            breakpoint_offsets: Vec::new(),
        }
    }

    /// Add breakpoint at WASM offset
    pub fn add_breakpoint_at_offset(&mut self, offset: usize) {
        if !self.breakpoint_offsets.contains(&offset) {
            self.breakpoint_offsets.push(offset);
            self.breakpoint_offsets.sort_unstable();
        }
    }

    /// Add breakpoints at the entry of the specified function indices.
    ///
    /// Parses `self.original_wasm` with `wasmparser` to locate each function
    /// body's byte offset, then calls [`add_breakpoint_at_offset`] for every
    /// index listed in `function_indices`.  Returns an error if the stored
    /// bytes are not valid WebAssembly.
    pub fn add_breakpoints_at_functions(&mut self, function_indices: &[u32]) -> Result<()> {
        let mut code_func_idx: u32 = 0;
        let mut offsets_to_add: Vec<usize> = Vec::new();

        for payload in wasmparser::Parser::new(0).parse_all(&self.original_wasm) {
            let payload = payload.map_err(|e| anyhow!("Failed to parse WASM module: {}", e))?;
            if let wasmparser::Payload::CodeSectionEntry(body) = payload {
                if function_indices.contains(&code_func_idx) {
                    offsets_to_add.push(body.range().start);
                }
                code_func_idx += 1;
            }
        }

        for offset in offsets_to_add {
            self.add_breakpoint_at_offset(offset);
        }

        Ok(())
    }

    /// Inject breakpoints into WASM bytecode
    pub fn inject(&self) -> Result<InjectionResult> {
        // NOTE: This is a simplified implementation
        // Real implementation would use wasmparser to parse and modify WASM

        let modified = self.original_wasm.clone();
        let mut offset_map = HashMap::new();

        // For each breakpoint, we would:
        // 1. Parse WASM to find the instruction at offset
        // 2. Insert a call to a breakpoint handler function
        // 3. Update all branch targets and function indices
        // 4. Recompute section sizes

        // Simplified: Just track original offsets
        for (idx, _) in self.original_wasm.iter().enumerate() {
            offset_map.insert(idx, idx);
        }

        Ok(InjectionResult {
            injected_count: self.breakpoint_offsets.len(),
            modified_wasm: modified,
            offset_map,
        })
    }

    /// Get original WASM
    pub fn original_wasm(&self) -> &[u8] {
        &self.original_wasm
    }

    /// Get breakpoint offsets
    pub fn breakpoint_offsets(&self) -> &[usize] {
        &self.breakpoint_offsets
    }
}

/// Variable scope tracker
pub struct VariableScope {
    /// Variables in current scope
    variables: HashMap<String, Variable>,
    /// Parent scope (for nested scopes)
    parent: Option<Box<VariableScope>>,
    /// Scope depth
    depth: u32,
}

impl VariableScope {
    /// Create new scope
    pub fn new(depth: u32) -> Self {
        Self {
            variables: HashMap::new(),
            parent: None,
            depth,
        }
    }

    /// Create child scope
    pub fn push_scope(&mut self) -> VariableScope {
        VariableScope {
            variables: HashMap::new(),
            parent: None,
            depth: self.depth + 1,
        }
    }

    /// Add variable to scope
    pub fn add_variable(&mut self, var: Variable) {
        self.variables.insert(var.name.clone(), var);
    }

    /// Get variable from scope (searches parent scopes)
    pub fn get_variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|parent| parent.get_variable(name))
        })
    }

    /// List all variables in current scope only
    pub fn list_local_variables(&self) -> Vec<&Variable> {
        self.variables.values().collect()
    }

    /// List all variables (including parent scopes)
    pub fn list_all_variables(&self) -> Vec<&Variable> {
        let mut vars = self.list_local_variables();
        if let Some(parent) = &self.parent {
            vars.extend(parent.list_all_variables());
        }
        vars
    }

    /// Get scope depth
    pub fn depth(&self) -> u32 {
        self.depth
    }
}

/// Enhanced debugger with advanced features
pub struct AdvancedDebugger {
    /// Base debug context
    context: DebugContext,
    /// Watch expressions
    watches: Arc<Mutex<HashMap<usize, WatchExpression>>>,
    /// Next watch ID
    next_watch_id: usize,
    /// Current step mode
    step_mode: StepMode,
    /// Current call depth (for step out)
    call_depth: usize,
    /// Target call depth (for step out)
    target_depth: Option<usize>,
    /// Current variable scope
    current_scope: Option<VariableScope>,
}

impl AdvancedDebugger {
    /// Create new advanced debugger
    pub fn new() -> Self {
        Self {
            context: DebugContext::new(),
            watches: Arc::new(Mutex::new(HashMap::new())),
            next_watch_id: 1,
            step_mode: StepMode::Continue,
            call_depth: 0,
            target_depth: None,
            current_scope: Some(VariableScope::new(0)),
        }
    }

    /// Add watch expression
    pub fn add_watch(&mut self, expression: impl Into<String>) -> usize {
        let id = self.next_watch_id;
        self.next_watch_id += 1;

        let watch = WatchExpression::new(id, expression);
        self.watches
            .lock()
            .expect("Watches lock poisoned")
            .insert(id, watch);
        id
    }

    /// Remove watch expression
    pub fn remove_watch(&mut self, id: usize) -> bool {
        self.watches
            .lock()
            .expect("Watches lock poisoned")
            .remove(&id)
            .is_some()
    }

    /// Update watch expression value
    pub fn update_watch(&mut self, id: usize, value: VariableValue) -> Result<bool> {
        let mut watches = self.watches.lock().expect("Watches lock poisoned");
        if let Some(watch) = watches.get_mut(&id) {
            Ok(watch.update(value))
        } else {
            Err(anyhow!("Watch {} not found", id))
        }
    }

    /// List all watches
    pub fn list_watches(&self) -> Vec<WatchExpression> {
        self.watches
            .lock()
            .expect("Watches lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Set step mode
    pub fn set_step_mode(&mut self, mode: StepMode) {
        self.step_mode = mode;

        // Update target depth for step out
        if mode == StepMode::StepOut {
            self.target_depth = Some(self.call_depth.saturating_sub(1));
        } else {
            self.target_depth = None;
        }
    }

    /// Get current step mode
    pub fn step_mode(&self) -> StepMode {
        self.step_mode
    }

    /// Handle function call (updates call depth)
    pub fn on_function_call(&mut self) {
        self.call_depth += 1;
    }

    /// Handle function return (updates call depth)
    pub fn on_function_return(&mut self) {
        if self.call_depth > 0 {
            self.call_depth -= 1;
        }
    }

    /// Check if should stop at current location
    pub fn should_stop_at(&self, _wasm_offset: usize) -> bool {
        match self.step_mode {
            StepMode::Continue => false,
            StepMode::StepInstruction => true,
            StepMode::StepOver => true,
            StepMode::StepInto => true,
            StepMode::StepOut => {
                if let Some(target) = self.target_depth {
                    self.call_depth <= target
                } else {
                    false
                }
            }
        }
    }

    /// Add variable to current scope
    pub fn add_variable(&mut self, var: Variable) {
        if let Some(scope) = &mut self.current_scope {
            scope.add_variable(var);
        }
    }

    /// Get variable from current scope
    pub fn get_variable(&self, name: &str) -> Option<Variable> {
        self.current_scope
            .as_ref()
            .and_then(|scope| scope.get_variable(name))
            .cloned()
    }

    /// List variables in current scope
    pub fn list_variables(&self) -> Vec<Variable> {
        self.current_scope
            .as_ref()
            .map(|scope| scope.list_all_variables().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Push new scope (entering function/block)
    pub fn push_scope(&mut self) {
        if let Some(mut current) = self.current_scope.take() {
            let new_scope = current.push_scope();
            self.current_scope = Some(new_scope);
        }
    }

    /// Get base debug context
    pub fn context(&self) -> &DebugContext {
        &self.context
    }

    /// Get mutable debug context
    pub fn context_mut(&mut self) -> &mut DebugContext {
        &mut self.context
    }
}

impl Default for AdvancedDebugger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_map_entry() {
        let entry = SourceMapEntry {
            wasm_offset: 100,
            file: "test.wat".to_string(),
            line: 42,
            column: 10,
            function_name: Some("test_func".to_string()),
        };

        assert_eq!(entry.wasm_offset, 100);
        assert_eq!(entry.line, 42);
    }

    #[test]
    fn test_source_map_lookup() {
        let mut source_map = SourceMap::new("test_module".to_string());

        source_map.add_entry(SourceMapEntry {
            wasm_offset: 0,
            file: "test.wat".to_string(),
            line: 1,
            column: 1,
            function_name: None,
        });

        source_map.add_entry(SourceMapEntry {
            wasm_offset: 100,
            file: "test.wat".to_string(),
            line: 10,
            column: 5,
            function_name: Some("func1".to_string()),
        });

        source_map.add_entry(SourceMapEntry {
            wasm_offset: 200,
            file: "test.wat".to_string(),
            line: 20,
            column: 1,
            function_name: Some("func2".to_string()),
        });

        // Exact match
        let entry = source_map.lookup(100).unwrap();
        assert_eq!(entry.line, 10);

        // Between entries - should return previous
        let entry = source_map.lookup(150).unwrap();
        assert_eq!(entry.line, 10);

        // Before first entry
        let entry = source_map.lookup(0).unwrap();
        assert_eq!(entry.line, 1);
    }

    #[test]
    fn test_source_map_with_source_content() {
        let mut source_map = SourceMap::new("test".to_string());

        source_map.add_source("test.wat".to_string(), "line 1\nline 2\nline 3".to_string());

        assert_eq!(source_map.get_source_line("test.wat", 1), Some("line 1"));
        assert_eq!(source_map.get_source_line("test.wat", 2), Some("line 2"));
        assert_eq!(source_map.get_source_line("test.wat", 3), Some("line 3"));
        assert_eq!(source_map.get_source_line("test.wat", 4), None);
    }

    #[test]
    fn test_stack_frame_format() {
        let frame = StackFrame {
            func_index: 5,
            func_name: Some("test_function".to_string()),
            wasm_offset: 0x123,
            source_location: Some(SourceMapEntry {
                wasm_offset: 0x123,
                file: "test.rs".to_string(),
                line: 42,
                column: 10,
                function_name: Some("test_function".to_string()),
            }),
            module_name: "test_module".to_string(),
        };

        let formatted = frame.format();
        assert!(formatted.contains("test_function"));
        assert!(formatted.contains("test.rs"));
        assert!(formatted.contains("42"));
    }

    #[test]
    fn test_stack_trace() {
        let mut trace = StackTrace {
            message: "Test error".to_string(),
            frames: Vec::new(),
        };

        trace.add_frame(StackFrame {
            func_index: 0,
            func_name: Some("main".to_string()),
            wasm_offset: 0,
            source_location: None,
            module_name: "test".to_string(),
        });

        let formatted = trace.format();
        assert!(formatted.contains("Test error"));
        assert!(formatted.contains("main"));
    }

    #[test]
    fn test_memory_inspector_hex_dump() {
        let memory = vec![
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, // "Hello Wo"
            0x72, 0x6c, 0x64, 0x21, 0x00, 0x00, 0x00, 0x00, // "rld!"
        ];

        let inspector = MemoryInspector::new(0, 16);
        let dump = inspector.dump_hex(&memory);

        assert!(dump.contains("48 65 6c 6c"));
        assert!(dump.contains("Hello"));
    }

    #[test]
    fn test_memory_inspector_read_u32() {
        let memory = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let inspector = MemoryInspector::new(0, 8);

        let value = inspector.read_u32(&memory, 0).unwrap();
        assert_eq!(value, 0x04030201); // Little-endian

        let value = inspector.read_u32(&memory, 4).unwrap();
        assert_eq!(value, 0x08070605);
    }

    #[test]
    fn test_memory_inspector_read_cstring() {
        let memory = b"Hello\0World\0";
        let inspector = MemoryInspector::new(0, memory.len());

        let s = inspector.read_cstring(memory, 0).unwrap();
        assert_eq!(s, "Hello");

        let s = inspector.read_cstring(memory, 6).unwrap();
        assert_eq!(s, "World");
    }

    #[test]
    fn test_breakpoint_management() {
        let mut ctx = DebugContext::new();

        let bp1 = ctx.add_breakpoint(BreakpointType::Offset(100));
        let bp2 = ctx.add_breakpoint(BreakpointType::FunctionEntry(5));

        let breakpoints = ctx.list_breakpoints();
        assert_eq!(breakpoints.len(), 2);

        ctx.set_breakpoint_enabled(bp1, false).unwrap();
        let breakpoints = ctx.list_breakpoints();
        let bp = breakpoints.iter().find(|b| b.id == bp1).unwrap();
        assert!(!bp.enabled);

        assert!(ctx.remove_breakpoint(bp2));
        assert_eq!(ctx.list_breakpoints().len(), 1);
    }

    #[test]
    fn test_debugger_state() {
        let mut ctx = DebugContext::new();
        assert_eq!(ctx.state(), DebuggerState::Running);

        ctx.set_state(DebuggerState::Stepping);
        assert_eq!(ctx.state(), DebuggerState::Stepping);
        assert!(ctx.should_stop(0, 0));

        ctx.set_state(DebuggerState::Running);
        assert_eq!(ctx.state(), DebuggerState::Running);
    }

    #[test]
    fn test_debug_context_source_maps() {
        let mut ctx = DebugContext::new();
        let mut source_map = SourceMap::new("test".to_string());

        source_map.add_entry(SourceMapEntry {
            wasm_offset: 0,
            file: "test.wat".to_string(),
            line: 1,
            column: 1,
            function_name: None,
        });

        ctx.add_source_map("test_module".to_string(), source_map);

        let retrieved = ctx.get_source_map("test_module");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().module_name(), "test");
    }

    // Advanced debugging tests

    #[test]
    fn test_variable_types_and_values() {
        // Test variable value formatting
        let val_i32 = VariableValue::I32(42);
        assert_eq!(val_i32.format(), "42");

        let val_f32 = VariableValue::F32(std::f32::consts::PI);
        assert!(val_f32.format().contains("3.14"));

        let val_ptr = VariableValue::Pointer(0x1000);
        assert_eq!(val_ptr.format(), "0x00001000");

        let val_unavail = VariableValue::Unavailable;
        assert_eq!(val_unavail.format(), "<unavailable>");
    }

    #[test]
    fn test_variable_creation() {
        let var = Variable::new("counter", VariableType::I32, VariableValue::I32(10));
        assert_eq!(var.name, "counter");
        assert_eq!(var.var_type, VariableType::I32);
        assert_eq!(var.value, VariableValue::I32(10));

        let formatted = var.format();
        assert!(formatted.contains("counter"));
        assert!(formatted.contains("i32"));
        assert!(formatted.contains("10"));
    }

    #[test]
    fn test_variable_with_scope() {
        let var = Variable::new("x", VariableType::I64, VariableValue::I64(100))
            .with_scope(2)
            .with_memory_location(0x2000);

        assert_eq!(var.scope_depth, 2);
        assert_eq!(var.memory_location, Some(0x2000));
    }

    #[test]
    fn test_watch_expression_creation() {
        let watch = WatchExpression::new(1, "myVar");
        assert_eq!(watch.id, 1);
        assert_eq!(watch.expression, "myVar");
        assert!(watch.enabled);
        assert_eq!(watch.change_count, 0);
        assert!(watch.last_value.is_none());
    }

    #[test]
    fn test_watch_expression_update() {
        let mut watch = WatchExpression::new(1, "counter");

        // First update - should always change
        let changed = watch.update(VariableValue::I32(0));
        assert!(changed);
        assert_eq!(watch.change_count, 1);

        // Same value - no change
        let changed = watch.update(VariableValue::I32(0));
        assert!(!changed);
        assert_eq!(watch.change_count, 1);

        // Different value - should change
        let changed = watch.update(VariableValue::I32(5));
        assert!(changed);
        assert_eq!(watch.change_count, 2);
    }

    #[test]
    fn test_watch_expression_describe_change() {
        let mut watch = WatchExpression::new(1, "value");

        watch.update(VariableValue::I32(10));
        let desc = watch.describe_change(&VariableValue::I32(20));
        assert!(desc.contains("value"));
        assert!(desc.contains("10"));
        assert!(desc.contains("20"));
    }

    #[test]
    fn test_step_modes() {
        assert_eq!(StepMode::Continue, StepMode::Continue);
        assert_ne!(StepMode::StepOver, StepMode::StepInto);

        // All modes should be distinct
        let modes = [
            StepMode::Continue,
            StepMode::StepInstruction,
            StepMode::StepOver,
            StepMode::StepInto,
            StepMode::StepOut,
        ];

        for (i, mode1) in modes.iter().enumerate() {
            for (j, mode2) in modes.iter().enumerate() {
                if i == j {
                    assert_eq!(mode1, mode2);
                } else {
                    assert_ne!(mode1, mode2);
                }
            }
        }
    }

    #[test]
    fn test_breakpoint_injector_creation() {
        let wasm = vec![0x00, 0x61, 0x73, 0x6d]; // WASM magic number
        let injector = BreakpointInjector::new(wasm.clone());

        assert_eq!(injector.original_wasm(), &wasm);
        assert_eq!(injector.breakpoint_offsets().len(), 0);
    }

    #[test]
    fn test_breakpoint_injector_add_offsets() {
        let wasm = vec![0x00; 100];
        let mut injector = BreakpointInjector::new(wasm);

        injector.add_breakpoint_at_offset(10);
        injector.add_breakpoint_at_offset(20);
        injector.add_breakpoint_at_offset(10); // Duplicate

        assert_eq!(injector.breakpoint_offsets().len(), 2);
        assert!(injector.breakpoint_offsets().contains(&10));
        assert!(injector.breakpoint_offsets().contains(&20));
    }

    #[test]
    fn test_breakpoint_injector_inject() {
        let wasm = vec![0x00; 50];
        let mut injector = BreakpointInjector::new(wasm.clone());

        injector.add_breakpoint_at_offset(10);
        injector.add_breakpoint_at_offset(25);

        let result = injector.inject().unwrap();
        assert_eq!(result.injected_count, 2);
        assert_eq!(result.modified_wasm.len(), wasm.len());
        assert!(!result.offset_map.is_empty());
    }

    #[test]
    fn test_variable_scope_creation() {
        let scope = VariableScope::new(0);
        assert_eq!(scope.depth(), 0);
        assert!(scope.list_local_variables().is_empty());
    }

    #[test]
    fn test_variable_scope_add_variable() {
        let mut scope = VariableScope::new(0);

        let var1 = Variable::new("x", VariableType::I32, VariableValue::I32(10));
        let var2 = Variable::new(
            "y",
            VariableType::F32,
            VariableValue::F32(std::f32::consts::PI),
        );

        scope.add_variable(var1);
        scope.add_variable(var2);

        assert_eq!(scope.list_local_variables().len(), 2);
        assert!(scope.get_variable("x").is_some());
        assert!(scope.get_variable("y").is_some());
        assert!(scope.get_variable("z").is_none());
    }

    #[test]
    fn test_variable_scope_nesting() {
        let mut parent = VariableScope::new(0);
        parent.add_variable(Variable::new(
            "global",
            VariableType::I32,
            VariableValue::I32(100),
        ));

        let mut child = parent.push_scope();
        assert_eq!(child.depth(), 1);

        child.add_variable(Variable::new(
            "local",
            VariableType::I32,
            VariableValue::I32(10),
        ));

        // Child scope should have only local variables
        assert_eq!(child.list_local_variables().len(), 1);
    }

    #[test]
    fn test_advanced_debugger_creation() {
        let debugger = AdvancedDebugger::new();
        assert_eq!(debugger.step_mode(), StepMode::Continue);
        assert!(debugger.list_watches().is_empty());
        assert!(debugger.list_variables().is_empty());
    }

    #[test]
    fn test_advanced_debugger_watches() {
        let mut debugger = AdvancedDebugger::new();

        let watch1 = debugger.add_watch("counter");
        let watch2 = debugger.add_watch("position.x");

        assert_eq!(debugger.list_watches().len(), 2);

        // Update watch
        let changed = debugger
            .update_watch(watch1, VariableValue::I32(5))
            .unwrap();
        assert!(changed);

        // Remove watch
        assert!(debugger.remove_watch(watch2));
        assert_eq!(debugger.list_watches().len(), 1);
    }

    #[test]
    fn test_advanced_debugger_step_modes() {
        let mut debugger = AdvancedDebugger::new();

        // Continue mode - should not stop
        assert!(!debugger.should_stop_at(0));

        // Step instruction - should always stop
        debugger.set_step_mode(StepMode::StepInstruction);
        assert!(debugger.should_stop_at(0));

        // Step over - should stop
        debugger.set_step_mode(StepMode::StepOver);
        assert!(debugger.should_stop_at(0));
    }

    #[test]
    fn test_advanced_debugger_step_out() {
        let mut debugger = AdvancedDebugger::new();

        // Simulate function calls
        debugger.on_function_call();
        debugger.on_function_call();
        assert_eq!(debugger.call_depth, 2);

        // Set step out mode
        debugger.set_step_mode(StepMode::StepOut);

        // Should stop when we return to depth 1
        assert!(!debugger.should_stop_at(0)); // Still at depth 2

        debugger.on_function_return();
        assert!(debugger.should_stop_at(0)); // Now at depth 1
    }

    #[test]
    fn test_advanced_debugger_variables() {
        let mut debugger = AdvancedDebugger::new();

        let var1 = Variable::new("x", VariableType::I32, VariableValue::I32(42));
        let var2 = Variable::new(
            "y",
            VariableType::F32,
            VariableValue::F32(std::f32::consts::PI),
        );

        debugger.add_variable(var1);
        debugger.add_variable(var2);

        let vars = debugger.list_variables();
        assert_eq!(vars.len(), 2);

        let x = debugger.get_variable("x").unwrap();
        assert_eq!(x.value, VariableValue::I32(42));
    }

    #[test]
    fn test_advanced_debugger_scope_push() {
        let mut debugger = AdvancedDebugger::new();

        debugger.add_variable(Variable::new(
            "global",
            VariableType::I32,
            VariableValue::I32(100),
        ));

        debugger.push_scope();

        debugger.add_variable(Variable::new(
            "local",
            VariableType::I32,
            VariableValue::I32(10),
        ));

        // Should have local variable in new scope
        assert!(debugger.get_variable("local").is_some());
    }

    #[test]
    fn test_advanced_debugger_context_access() {
        let mut debugger = AdvancedDebugger::new();

        // Should be able to access base context
        let ctx = debugger.context();
        assert_eq!(ctx.state(), DebuggerState::Running);

        // Should be able to modify base context
        let ctx_mut = debugger.context_mut();
        ctx_mut.set_state(DebuggerState::Stepping);
        assert_eq!(debugger.context().state(), DebuggerState::Stepping);
    }

    #[test]
    fn test_breakpoint_injector_function_breakpoints() {
        // All-zero bytes are not valid WASM; the parser must return an error.
        let wasm = vec![0x00; 100];
        let mut injector = BreakpointInjector::new(wasm);
        let result = injector.add_breakpoints_at_functions(&[0, 1, 2]);
        assert!(result.is_err(), "Expected error for invalid WASM bytes");
    }

    #[test]
    fn test_add_breakpoints_at_functions_two_funcs() {
        // Build a minimal 2-function WASM module from WAT source.
        let wasm_bytes = wat::parse_str(
            r#"(module
                (func (result i32) i32.const 1)
                (func (result i32) i32.const 2))"#,
        )
        .expect("WAT compilation failed");

        let mut injector = BreakpointInjector::new(wasm_bytes);
        injector
            .add_breakpoints_at_functions(&[0, 1])
            .expect("Failed to add breakpoints at functions 0 and 1");

        let offsets = injector.breakpoint_offsets();
        assert_eq!(offsets.len(), 2, "Expected exactly 2 breakpoint offsets");
        assert!(offsets[0] > 0, "First function offset should be non-zero");
        assert!(offsets[1] > 0, "Second function offset should be non-zero");
        assert_ne!(offsets[0], offsets[1], "Function offsets must be distinct");
    }

    #[test]
    fn test_add_breakpoints_malformed_returns_error() {
        // Malformed / non-WASM bytes must yield an Err result.
        let mut injector = BreakpointInjector::new(b"not wasm".to_vec());
        let result = injector.add_breakpoints_at_functions(&[0]);
        assert!(result.is_err(), "Expected error for malformed WASM input");
    }
}
