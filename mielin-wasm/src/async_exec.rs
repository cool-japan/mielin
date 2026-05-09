//! Async Execution Support for WASM
//!
//! Provides async host function calls, cooperative yielding,
//! timeout enforcement, and non-blocking I/O simulation.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Execution state for cooperative multitasking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    /// Running normally
    Running,
    /// Yielded control voluntarily
    Yielded,
    /// Waiting for async operation
    Waiting,
    /// Completed successfully
    Completed,
    /// Timed out
    TimedOut,
    /// Cancelled
    Cancelled,
    /// Error occurred
    Error,
}

/// Yield reason for cooperative scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YieldReason {
    /// Voluntary yield to allow other tasks
    Voluntary,
    /// Yielding for I/O operation
    IoWait,
    /// Yielding for async host call
    HostCall,
    /// Yielding due to fuel exhaustion
    FuelExhausted,
    /// Yielding at checkpoint
    Checkpoint,
}

/// Async operation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncStatus {
    /// Operation not started
    Pending,
    /// Operation in progress
    InProgress,
    /// Operation completed successfully
    Completed,
    /// Operation failed
    Failed,
    /// Operation was cancelled
    Cancelled,
}

/// Async operation identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncOpId(u64);

impl AsyncOpId {
    /// Create a new async operation ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Async operation result
#[derive(Debug, Clone)]
pub enum AsyncResult {
    /// No result yet
    Pending,
    /// Integer result
    Int(i64),
    /// Float result
    Float(f64),
    /// Bytes result
    Bytes(Vec<u8>),
    /// Error message
    Error(String),
    /// Void/unit result
    Void,
}

impl AsyncResult {
    /// Check if result is ready
    pub fn is_ready(&self) -> bool {
        !matches!(self, AsyncResult::Pending)
    }

    /// Get as integer
    pub fn as_int(&self) -> Option<i64> {
        match self {
            AsyncResult::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            AsyncResult::Bytes(v) => Some(v),
            _ => None,
        }
    }
}

/// Async operation descriptor
#[derive(Debug)]
pub struct AsyncOperation {
    /// Operation ID
    pub id: AsyncOpId,
    /// Operation type name
    pub op_type: String,
    /// Current status
    pub status: AsyncStatus,
    /// Result when completed
    pub result: AsyncResult,
    /// Start time
    pub start_time: Instant,
    /// Timeout duration (if any)
    pub timeout: Option<Duration>,
}

impl AsyncOperation {
    /// Create a new async operation
    pub fn new(id: AsyncOpId, op_type: impl Into<String>) -> Self {
        Self {
            id,
            op_type: op_type.into(),
            status: AsyncStatus::Pending,
            result: AsyncResult::Pending,
            start_time: Instant::now(),
            timeout: None,
        }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Check if operation has timed out
    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout) = self.timeout {
            self.start_time.elapsed() > timeout
        } else {
            false
        }
    }

    /// Mark as in progress
    pub fn start(&mut self) {
        self.status = AsyncStatus::InProgress;
    }

    /// Complete with result
    pub fn complete(&mut self, result: AsyncResult) {
        self.status = AsyncStatus::Completed;
        self.result = result;
    }

    /// Fail with error
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = AsyncStatus::Failed;
        self.result = AsyncResult::Error(error.into());
    }

    /// Cancel the operation
    pub fn cancel(&mut self) {
        self.status = AsyncStatus::Cancelled;
        self.result = AsyncResult::Error("Cancelled".into());
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

/// Yield point for cooperative multitasking
#[derive(Debug, Clone)]
pub struct YieldPoint {
    /// Yield point ID
    pub id: u64,
    /// Function name
    pub function: String,
    /// Instruction offset (if available)
    pub offset: Option<u64>,
    /// Yield count at this point
    pub yield_count: u64,
}

impl YieldPoint {
    /// Create a new yield point
    pub fn new(id: u64, function: impl Into<String>) -> Self {
        Self {
            id,
            function: function.into(),
            offset: None,
            yield_count: 0,
        }
    }

    /// With instruction offset
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Timeout configuration
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Total execution timeout
    pub total_timeout: Option<Duration>,
    /// Per-function timeout
    pub function_timeout: Option<Duration>,
    /// Per-host-call timeout
    pub host_call_timeout: Option<Duration>,
    /// Yield interval (max time before forced yield)
    pub yield_interval: Option<Duration>,
}

impl TimeoutConfig {
    /// No timeouts
    pub fn none() -> Self {
        Self {
            total_timeout: None,
            function_timeout: None,
            host_call_timeout: None,
            yield_interval: None,
        }
    }

    /// Default timeouts for interactive use
    pub fn interactive() -> Self {
        Self {
            total_timeout: Some(Duration::from_secs(30)),
            function_timeout: Some(Duration::from_secs(10)),
            host_call_timeout: Some(Duration::from_secs(5)),
            yield_interval: Some(Duration::from_millis(100)),
        }
    }

    /// Strict timeouts for untrusted code
    pub fn strict() -> Self {
        Self {
            total_timeout: Some(Duration::from_secs(5)),
            function_timeout: Some(Duration::from_secs(1)),
            host_call_timeout: Some(Duration::from_millis(500)),
            yield_interval: Some(Duration::from_millis(10)),
        }
    }

    /// Batch processing timeouts
    pub fn batch() -> Self {
        Self {
            total_timeout: Some(Duration::from_secs(300)),
            function_timeout: Some(Duration::from_secs(60)),
            host_call_timeout: Some(Duration::from_secs(30)),
            yield_interval: Some(Duration::from_secs(1)),
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self::interactive()
    }
}

/// Fuel-based execution limiter
#[derive(Debug)]
pub struct FuelMeter {
    /// Remaining fuel
    remaining: AtomicU64,
    /// Initial fuel amount
    initial: u64,
    /// Fuel consumed
    consumed: AtomicU64,
    /// Auto-refuel amount (0 = no auto refuel)
    auto_refuel: u64,
}

impl FuelMeter {
    /// Create a new fuel meter
    pub fn new(initial_fuel: u64) -> Self {
        Self {
            remaining: AtomicU64::new(initial_fuel),
            initial: initial_fuel,
            consumed: AtomicU64::new(0),
            auto_refuel: 0,
        }
    }

    /// Create with auto-refuel
    pub fn with_auto_refuel(initial_fuel: u64, refuel_amount: u64) -> Self {
        Self {
            remaining: AtomicU64::new(initial_fuel),
            initial: initial_fuel,
            consumed: AtomicU64::new(0),
            auto_refuel: refuel_amount,
        }
    }

    /// Consume fuel, returns false if exhausted
    pub fn consume(&self, amount: u64) -> bool {
        let mut current = self.remaining.load(Ordering::Relaxed);
        loop {
            if current < amount {
                // Try auto-refuel
                if self.auto_refuel > 0 {
                    self.remaining
                        .fetch_add(self.auto_refuel, Ordering::Relaxed);
                    current = self.remaining.load(Ordering::Relaxed);
                    continue;
                }
                return false;
            }
            match self.remaining.compare_exchange_weak(
                current,
                current - amount,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.consumed.fetch_add(amount, Ordering::Relaxed);
                    return true;
                }
                Err(c) => current = c,
            }
        }
    }

    /// Get remaining fuel
    pub fn remaining(&self) -> u64 {
        self.remaining.load(Ordering::Relaxed)
    }

    /// Get consumed fuel
    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::Relaxed)
    }

    /// Refuel to a specific amount
    pub fn refuel(&self, amount: u64) {
        self.remaining.store(amount, Ordering::Relaxed);
    }

    /// Reset to initial state
    pub fn reset(&self) {
        self.remaining.store(self.initial, Ordering::Relaxed);
        self.consumed.store(0, Ordering::Relaxed);
    }

    /// Check if exhausted
    pub fn is_exhausted(&self) -> bool {
        self.remaining.load(Ordering::Relaxed) == 0
    }

    /// Get consumption percentage
    pub fn consumption_percent(&self) -> f32 {
        if self.initial == 0 {
            return 0.0;
        }
        (self.consumed.load(Ordering::Relaxed) as f32 / self.initial as f32) * 100.0
    }
}

impl Default for FuelMeter {
    fn default() -> Self {
        Self::new(1_000_000) // 1M default fuel
    }
}

/// Async execution context
#[derive(Debug)]
pub struct AsyncContext {
    /// Current execution state
    state: ExecutionState,
    /// Pending async operations
    pending_ops: VecDeque<AsyncOpId>,
    /// Completed operations
    completed_ops: Vec<AsyncOperation>,
    /// Next operation ID
    next_op_id: AtomicU64,
    /// Yield points
    yield_points: Vec<YieldPoint>,
    /// Total yield count
    yield_count: u64,
    /// Timeout configuration
    timeout_config: TimeoutConfig,
    /// Fuel meter
    fuel: FuelMeter,
    /// Start time
    start_time: Instant,
    /// Cancellation flag
    cancelled: AtomicBool,
    /// Last yield time
    last_yield_time: Instant,
}

impl AsyncContext {
    /// Create a new async context
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            state: ExecutionState::Running,
            pending_ops: VecDeque::new(),
            completed_ops: Vec::new(),
            next_op_id: AtomicU64::new(1),
            yield_points: Vec::new(),
            yield_count: 0,
            timeout_config: TimeoutConfig::default(),
            fuel: FuelMeter::default(),
            start_time: now,
            cancelled: AtomicBool::new(false),
            last_yield_time: now,
        }
    }

    /// Create with configuration
    pub fn with_config(timeout_config: TimeoutConfig, fuel: u64) -> Self {
        let now = Instant::now();
        Self {
            state: ExecutionState::Running,
            pending_ops: VecDeque::new(),
            completed_ops: Vec::new(),
            next_op_id: AtomicU64::new(1),
            yield_points: Vec::new(),
            yield_count: 0,
            timeout_config,
            fuel: FuelMeter::new(fuel),
            start_time: now,
            cancelled: AtomicBool::new(false),
            last_yield_time: now,
        }
    }

    /// Get current state
    pub fn state(&self) -> ExecutionState {
        self.state
    }

    /// Set state
    pub fn set_state(&mut self, state: ExecutionState) {
        self.state = state;
    }

    /// Check if should yield
    pub fn should_yield(&self) -> bool {
        // Check cancellation
        if self.cancelled.load(Ordering::Relaxed) {
            return true;
        }

        // Check fuel
        if self.fuel.is_exhausted() {
            return true;
        }

        // Check yield interval
        if let Some(interval) = self.timeout_config.yield_interval {
            if self.last_yield_time.elapsed() > interval {
                return true;
            }
        }

        // Check total timeout
        if let Some(timeout) = self.timeout_config.total_timeout {
            if self.start_time.elapsed() > timeout {
                return true;
            }
        }

        false
    }

    /// Record a yield
    pub fn record_yield(&mut self, reason: YieldReason, function: &str) {
        self.yield_count += 1;
        self.last_yield_time = Instant::now();

        let yield_point = YieldPoint::new(self.yield_count, function);

        // Update or add yield point
        if let Some(existing) = self
            .yield_points
            .iter_mut()
            .find(|p| p.function == function)
        {
            existing.yield_count += 1;
        } else {
            self.yield_points.push(yield_point);
        }

        self.state = match reason {
            YieldReason::Voluntary | YieldReason::Checkpoint => ExecutionState::Yielded,
            YieldReason::IoWait | YieldReason::HostCall => ExecutionState::Waiting,
            YieldReason::FuelExhausted => {
                if self.fuel.is_exhausted() {
                    ExecutionState::Error
                } else {
                    ExecutionState::Yielded
                }
            }
        };
    }

    /// Resume execution
    pub fn resume(&mut self) {
        if self.state == ExecutionState::Yielded || self.state == ExecutionState::Waiting {
            self.state = ExecutionState::Running;
        }
    }

    /// Create a new async operation
    pub fn create_async_op(&mut self, op_type: impl Into<String>) -> AsyncOpId {
        let id = AsyncOpId::new(self.next_op_id.fetch_add(1, Ordering::Relaxed));
        let op = AsyncOperation::new(id, op_type);

        if let Some(timeout) = self.timeout_config.host_call_timeout {
            self.completed_ops.push(op.with_timeout(timeout));
        } else {
            self.completed_ops.push(op);
        }

        self.pending_ops.push_back(id);
        id
    }

    /// Get async operation by ID
    pub fn get_op(&self, id: AsyncOpId) -> Option<&AsyncOperation> {
        self.completed_ops.iter().find(|op| op.id == id)
    }

    /// Get mutable async operation by ID
    pub fn get_op_mut(&mut self, id: AsyncOpId) -> Option<&mut AsyncOperation> {
        self.completed_ops.iter_mut().find(|op| op.id == id)
    }

    /// Complete an async operation
    pub fn complete_op(&mut self, id: AsyncOpId, result: AsyncResult) {
        if let Some(op) = self.get_op_mut(id) {
            op.complete(result);
        }
        self.pending_ops.retain(|&op_id| op_id != id);
    }

    /// Fail an async operation
    pub fn fail_op(&mut self, id: AsyncOpId, error: impl Into<String>) {
        if let Some(op) = self.get_op_mut(id) {
            op.fail(error);
        }
        self.pending_ops.retain(|&op_id| op_id != id);
    }

    /// Check for timed out operations
    pub fn check_timeouts(&mut self) -> Vec<AsyncOpId> {
        let mut timed_out = Vec::new();

        for op in &mut self.completed_ops {
            if op.status == AsyncStatus::InProgress && op.is_timed_out() {
                op.fail("Operation timed out");
                timed_out.push(op.id);
            }
        }

        for id in &timed_out {
            self.pending_ops.retain(|&op_id| op_id != *id);
        }

        timed_out
    }

    /// Consume fuel
    pub fn consume_fuel(&self, amount: u64) -> bool {
        self.fuel.consume(amount)
    }

    /// Get fuel meter
    pub fn fuel(&self) -> &FuelMeter {
        &self.fuel
    }

    /// Cancel execution
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Check if timed out
    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout) = self.timeout_config.total_timeout {
            self.start_time.elapsed() > timeout
        } else {
            false
        }
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get yield count
    pub fn yield_count(&self) -> u64 {
        self.yield_count
    }

    /// Get pending operation count
    pub fn pending_count(&self) -> usize {
        self.pending_ops.len()
    }

    /// Has pending operations
    pub fn has_pending(&self) -> bool {
        !self.pending_ops.is_empty()
    }

    /// Get summary
    pub fn summary(&self) -> AsyncSummary {
        AsyncSummary {
            state: self.state,
            elapsed: self.elapsed(),
            yield_count: self.yield_count,
            pending_ops: self.pending_ops.len(),
            completed_ops: self
                .completed_ops
                .iter()
                .filter(|op| op.status == AsyncStatus::Completed)
                .count(),
            failed_ops: self
                .completed_ops
                .iter()
                .filter(|op| op.status == AsyncStatus::Failed)
                .count(),
            fuel_remaining: self.fuel.remaining(),
            fuel_consumed: self.fuel.consumed(),
            is_cancelled: self.is_cancelled(),
            is_timed_out: self.is_timed_out(),
        }
    }
}

impl Default for AsyncContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Async execution summary
#[derive(Debug, Clone)]
pub struct AsyncSummary {
    /// Current state
    pub state: ExecutionState,
    /// Total elapsed time
    pub elapsed: Duration,
    /// Total yield count
    pub yield_count: u64,
    /// Pending operations
    pub pending_ops: usize,
    /// Completed operations
    pub completed_ops: usize,
    /// Failed operations
    pub failed_ops: usize,
    /// Remaining fuel
    pub fuel_remaining: u64,
    /// Consumed fuel
    pub fuel_consumed: u64,
    /// Was cancelled
    pub is_cancelled: bool,
    /// Was timed out
    pub is_timed_out: bool,
}

/// Cooperative scheduler for multiple WASM instances
#[derive(Debug)]
pub struct CooperativeScheduler {
    /// Execution contexts
    contexts: Vec<Arc<Mutex<AsyncContext>>>,
    /// Current context index
    current_index: usize,
    /// Round-robin quantum (max yields before switching)
    quantum: u64,
    /// Yields remaining for current context
    quantum_remaining: u64,
    /// Total scheduled count
    scheduled_count: u64,
}

impl CooperativeScheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            contexts: Vec::new(),
            current_index: 0,
            quantum: 10,
            quantum_remaining: 10,
            scheduled_count: 0,
        }
    }

    /// Create with specific quantum
    pub fn with_quantum(quantum: u64) -> Self {
        Self {
            contexts: Vec::new(),
            current_index: 0,
            quantum,
            quantum_remaining: quantum,
            scheduled_count: 0,
        }
    }

    /// Add a context to schedule
    pub fn add(&mut self, context: Arc<Mutex<AsyncContext>>) -> usize {
        let index = self.contexts.len();
        self.contexts.push(context);
        index
    }

    /// Remove a context
    pub fn remove(&mut self, index: usize) -> Option<Arc<Mutex<AsyncContext>>> {
        if index < self.contexts.len() {
            Some(self.contexts.remove(index))
        } else {
            None
        }
    }

    /// Get the quantum value (max yields before switching)
    pub fn quantum(&self) -> u64 {
        self.quantum
    }

    /// Get next runnable context using round-robin scheduling
    ///
    /// Returns the next context that is in Running or Yielded state.
    /// Respects quantum limits - after `quantum` yields, moves to next context.
    pub fn next_runnable(&mut self) -> Option<Arc<Mutex<AsyncContext>>> {
        if self.contexts.is_empty() {
            return None;
        }

        // Check if quantum exhausted for current context
        if self.quantum_remaining == 0 {
            self.quantum_remaining = self.quantum;
            self.current_index = (self.current_index + 1) % self.contexts.len();
        }

        let start = self.current_index;
        loop {
            if let Some(ctx) = self.contexts.get(self.current_index) {
                if let Ok(guard) = ctx.lock() {
                    let state = guard.state();
                    if state == ExecutionState::Running || state == ExecutionState::Yielded {
                        self.scheduled_count += 1;
                        self.quantum_remaining = self.quantum_remaining.saturating_sub(1);
                        return Some(Arc::clone(ctx));
                    }
                }
            }

            // Move to next context
            self.current_index = (self.current_index + 1) % self.contexts.len();
            self.quantum_remaining = self.quantum;

            if self.current_index == start {
                // Checked all contexts, none runnable
                return None;
            }
        }
    }

    /// Get number of contexts
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    /// Get scheduled count
    pub fn scheduled_count(&self) -> u64 {
        self.scheduled_count
    }

    /// Get runnable count
    pub fn runnable_count(&self) -> usize {
        self.contexts
            .iter()
            .filter(|ctx| {
                ctx.lock()
                    .map(|g| matches!(g.state(), ExecutionState::Running | ExecutionState::Yielded))
                    .unwrap_or(false)
            })
            .count()
    }
}

impl Default for CooperativeScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_state() {
        assert_ne!(ExecutionState::Running, ExecutionState::Yielded);
        assert_ne!(ExecutionState::Completed, ExecutionState::Error);
    }

    #[test]
    fn test_yield_reason() {
        assert_ne!(YieldReason::Voluntary, YieldReason::FuelExhausted);
    }

    #[test]
    fn test_async_op_id() {
        let id = AsyncOpId::new(42);
        assert_eq!(id.raw(), 42);
    }

    #[test]
    fn test_async_result() {
        let pending = AsyncResult::Pending;
        assert!(!pending.is_ready());

        let int_result = AsyncResult::Int(42);
        assert!(int_result.is_ready());
        assert_eq!(int_result.as_int(), Some(42));

        let bytes_result = AsyncResult::Bytes(vec![1, 2, 3]);
        assert_eq!(bytes_result.as_bytes(), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn test_async_operation_new() {
        let op = AsyncOperation::new(AsyncOpId::new(1), "test_op");
        assert_eq!(op.status, AsyncStatus::Pending);
        assert!(!op.is_timed_out());
    }

    #[test]
    fn test_async_operation_lifecycle() {
        let mut op = AsyncOperation::new(AsyncOpId::new(1), "test_op");

        op.start();
        assert_eq!(op.status, AsyncStatus::InProgress);

        op.complete(AsyncResult::Int(42));
        assert_eq!(op.status, AsyncStatus::Completed);
        assert_eq!(op.result.as_int(), Some(42));
    }

    #[test]
    fn test_async_operation_failure() {
        let mut op = AsyncOperation::new(AsyncOpId::new(1), "test_op");
        op.start();
        op.fail("Something went wrong");

        assert_eq!(op.status, AsyncStatus::Failed);
        match &op.result {
            AsyncResult::Error(msg) => assert!(msg.contains("wrong")),
            _ => panic!("Expected error result"),
        }
    }

    #[test]
    fn test_async_operation_cancel() {
        let mut op = AsyncOperation::new(AsyncOpId::new(1), "test_op");
        op.start();
        op.cancel();

        assert_eq!(op.status, AsyncStatus::Cancelled);
    }

    #[test]
    fn test_yield_point_new() {
        let yp = YieldPoint::new(1, "test_func");
        assert_eq!(yp.id, 1);
        assert_eq!(yp.function, "test_func");
        assert_eq!(yp.yield_count, 0);
    }

    #[test]
    fn test_timeout_config_presets() {
        let none = TimeoutConfig::none();
        assert!(none.total_timeout.is_none());

        let interactive = TimeoutConfig::interactive();
        assert!(interactive.total_timeout.is_some());
        assert!(interactive.yield_interval.is_some());

        let strict = TimeoutConfig::strict();
        assert!(strict.total_timeout.unwrap() < interactive.total_timeout.unwrap());
    }

    #[test]
    fn test_fuel_meter_new() {
        let fuel = FuelMeter::new(1000);
        assert_eq!(fuel.remaining(), 1000);
        assert_eq!(fuel.consumed(), 0);
        assert!(!fuel.is_exhausted());
    }

    #[test]
    fn test_fuel_meter_consume() {
        let fuel = FuelMeter::new(100);

        assert!(fuel.consume(30));
        assert_eq!(fuel.remaining(), 70);
        assert_eq!(fuel.consumed(), 30);

        assert!(fuel.consume(70));
        assert_eq!(fuel.remaining(), 0);
        assert!(fuel.is_exhausted());

        assert!(!fuel.consume(1));
    }

    #[test]
    fn test_fuel_meter_auto_refuel() {
        let fuel = FuelMeter::with_auto_refuel(50, 100);

        assert!(fuel.consume(50));
        assert_eq!(fuel.remaining(), 0);

        // Should auto-refuel
        assert!(fuel.consume(50));
        assert_eq!(fuel.remaining(), 50); // 100 added - 50 consumed
    }

    #[test]
    fn test_fuel_meter_reset() {
        let fuel = FuelMeter::new(1000);
        fuel.consume(500);
        fuel.reset();

        assert_eq!(fuel.remaining(), 1000);
        assert_eq!(fuel.consumed(), 0);
    }

    #[test]
    fn test_async_context_new() {
        let ctx = AsyncContext::new();
        assert_eq!(ctx.state(), ExecutionState::Running);
        assert_eq!(ctx.yield_count(), 0);
        assert!(!ctx.has_pending());
    }

    #[test]
    fn test_async_context_yield() {
        let mut ctx = AsyncContext::new();

        ctx.record_yield(YieldReason::Voluntary, "test_func");
        assert_eq!(ctx.state(), ExecutionState::Yielded);
        assert_eq!(ctx.yield_count(), 1);

        ctx.resume();
        assert_eq!(ctx.state(), ExecutionState::Running);
    }

    #[test]
    fn test_async_context_async_ops() {
        let mut ctx = AsyncContext::new();

        let op_id = ctx.create_async_op("test_op");
        assert!(ctx.has_pending());
        assert_eq!(ctx.pending_count(), 1);

        ctx.complete_op(op_id, AsyncResult::Int(42));
        assert!(!ctx.has_pending());

        let op = ctx.get_op(op_id).unwrap();
        assert_eq!(op.status, AsyncStatus::Completed);
    }

    #[test]
    fn test_async_context_cancel() {
        let ctx = AsyncContext::new();

        assert!(!ctx.is_cancelled());
        ctx.cancel();
        assert!(ctx.is_cancelled());
    }

    #[test]
    fn test_async_context_fuel() {
        let ctx = AsyncContext::with_config(TimeoutConfig::none(), 100);

        assert!(ctx.consume_fuel(50));
        assert_eq!(ctx.fuel().remaining(), 50);

        assert!(ctx.consume_fuel(50));
        assert!(ctx.fuel().is_exhausted());
    }

    #[test]
    fn test_async_context_summary() {
        let mut ctx = AsyncContext::new();
        ctx.record_yield(YieldReason::Voluntary, "test");
        let op_id = ctx.create_async_op("test_op");
        ctx.complete_op(op_id, AsyncResult::Void);

        let summary = ctx.summary();
        assert_eq!(summary.yield_count, 1);
        assert_eq!(summary.completed_ops, 1);
    }

    #[test]
    fn test_cooperative_scheduler_new() {
        let sched = CooperativeScheduler::new();
        assert!(sched.is_empty());
        assert_eq!(sched.len(), 0);
    }

    #[test]
    fn test_cooperative_scheduler_add_remove() {
        let mut sched = CooperativeScheduler::new();

        let ctx = Arc::new(Mutex::new(AsyncContext::new()));
        let idx = sched.add(ctx);

        assert_eq!(sched.len(), 1);

        let removed = sched.remove(idx);
        assert!(removed.is_some());
        assert!(sched.is_empty());
    }

    #[test]
    fn test_cooperative_scheduler_next_runnable() {
        let mut sched = CooperativeScheduler::new();

        let ctx1 = Arc::new(Mutex::new(AsyncContext::new()));
        let ctx2 = Arc::new(Mutex::new(AsyncContext::new()));

        sched.add(ctx1);
        sched.add(ctx2);

        let next = sched.next_runnable();
        assert!(next.is_some());
        assert_eq!(sched.scheduled_count(), 1);
    }

    #[test]
    fn test_cooperative_scheduler_quantum() {
        let mut sched = CooperativeScheduler::with_quantum(3);
        assert_eq!(sched.quantum(), 3);

        let ctx = Arc::new(Mutex::new(AsyncContext::new()));
        sched.add(ctx);

        // Should be able to get the context 3 times (quantum)
        for _ in 0..3 {
            let next = sched.next_runnable();
            assert!(next.is_some());
        }
    }

    #[test]
    fn test_cooperative_scheduler_runnable_count() {
        let mut sched = CooperativeScheduler::new();

        let ctx1 = Arc::new(Mutex::new(AsyncContext::new()));
        let ctx2 = Arc::new(Mutex::new(AsyncContext::new()));

        // Make ctx2 completed
        ctx2.lock().unwrap_or_else(|e| e.into_inner()).set_state(ExecutionState::Completed);

        sched.add(ctx1);
        sched.add(ctx2);

        assert_eq!(sched.runnable_count(), 1);
    }

    #[test]
    fn test_async_operation_timeout() {
        // Use a longer timeout to avoid flakiness from scheduling delays
        let op = AsyncOperation::new(AsyncOpId::new(1), "test_op")
            .with_timeout(Duration::from_millis(50));

        // Should not be timed out immediately after creation
        assert!(!op.is_timed_out());

        // Wait well beyond the timeout
        std::thread::sleep(Duration::from_millis(100));

        // Now should be timed out
        assert!(op.is_timed_out());
    }
}
