//! Agent (Cell) representation
//!
//! Provides agent lifecycle management with state machine validation,
//! transition hooks, and error recovery.

use crate::{Dna, Policy};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub type AgentId = Uuid;

/// Agent lifecycle state
///
/// # State Machine
///
/// The agent lifecycle follows a state machine with well-defined transitions.
/// Each state represents a distinct phase in the agent's lifecycle, and transitions
/// between states are validated to ensure consistency.
///
/// ## States
///
/// - **Created**: Initial state after agent instantiation. The agent is configured
///   but not yet executing. From here, the agent can start running or be terminated.
///
/// - **Running**: The agent is actively executing its workload. This is the primary
///   operational state. From here, the agent can be paused, suspended, migrated,
///   encounter an error, or be terminated.
///
/// - **Paused**: The agent is voluntarily paused (e.g., by user request). Execution
///   is halted but can be resumed. From here, the agent can resume running, be
///   suspended, migrated, or terminated.
///
/// - **Suspended**: The agent is involuntarily suspended by the system (e.g., due to
///   resource constraints). From here, the agent can resume running, be paused,
///   migrated, encounter an error, or be terminated.
///
/// - **Migrating**: The agent is in the process of being migrated to another node.
///   State and execution context are being transferred. From here, the agent can
///   complete migration and resume running, encounter an error, or be terminated.
///
/// - **Error**: The agent encountered an error and requires recovery. From here,
///   the agent can recover and resume running, be suspended for investigation,
///   or be terminated.
///
/// - **Terminated**: Terminal state. The agent has been shut down and cannot transition
///   to any other state.
///
/// ## State Transition Diagram
///
/// ```text
///                    ┌─────────┐
///                    │ Created │
///                    └────┬────┘
///                         │
///              ┌──────────┼──────────┐
///              │                     │
///              v                     v
///        ┌──────────┐          ┌────────────┐
///        │ Running  │◄────────►│ Terminated │
///        └─────┬────┘          └────────────┘
///              │                     ▲
///    ┌─────────┼─────────┐          │
///    │         │         │          │
///    v         v         v          │
/// ┌───────┐ ┌────────┐ ┌──────────┐│
/// │Paused │ │Suspend-│ │Migrating ││
/// │       │ │  ed    │ │          ││
/// └───┬───┘ └───┬────┘ └─────┬────┘│
///     │         │            │     │
///     │    ┌────┴────┐       │     │
///     └───►│  Error  ├───────┘     │
///          └────┬────┘             │
///               └──────────────────┘
/// ```
///
/// ## Valid Transitions
///
/// | From       | To          | Description                                    |
/// |------------|-------------|------------------------------------------------|
/// | Created    | Running     | Start agent execution                          |
/// | Created    | Terminated  | Terminate before starting                      |
/// | Running    | Paused      | Voluntarily pause execution                    |
/// | Running    | Suspended   | System suspends due to resource constraints    |
/// | Running    | Migrating   | Begin migration to another node                |
/// | Running    | Error       | Execution error occurs                         |
/// | Running    | Terminated  | Normal shutdown                                |
/// | Paused     | Running     | Resume from voluntary pause                    |
/// | Paused     | Suspended   | System suspends a paused agent                 |
/// | Paused     | Migrating   | Migrate a paused agent                         |
/// | Paused     | Terminated  | Terminate a paused agent                       |
/// | Suspended  | Running     | Resume after system releases suspension        |
/// | Suspended  | Paused      | Convert system suspension to voluntary pause   |
/// | Suspended  | Migrating   | Migrate a suspended agent                      |
/// | Suspended  | Error       | Error occurs during suspension                 |
/// | Suspended  | Terminated  | Terminate a suspended agent                    |
/// | Migrating  | Running     | Migration completed successfully               |
/// | Migrating  | Error       | Migration failed                               |
/// | Migrating  | Terminated  | Terminate during migration                     |
/// | Error      | Running     | Error recovered, resume execution              |
/// | Error      | Suspended   | Suspend for further investigation              |
/// | Error      | Terminated  | Give up on recovery, shutdown                  |
///
/// ## Usage
///
/// ```rust
/// use mielin_cells::{Agent, AgentState};
///
/// // Create a new agent
/// let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
/// assert_eq!(agent.state(), &AgentState::Created);
///
/// // Check if transition is valid
/// assert!(agent.state().can_transition_to(&AgentState::Running));
/// assert!(!agent.state().can_transition_to(&AgentState::Paused)); // Invalid!
/// ```
///
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    /// Agent has been created but not yet started
    Created,
    /// Agent is actively executing
    Running,
    /// Agent execution is temporarily paused (voluntary)
    Paused,
    /// Agent is suspended by the system (involuntary)
    Suspended,
    /// Agent is being migrated to another node
    Migrating,
    /// Agent encountered an error and needs recovery
    Error,
    /// Agent has been terminated
    Terminated,
}

impl AgentState {
    /// Check if this state can transition to the target state
    pub fn can_transition_to(&self, target: &AgentState) -> bool {
        use AgentState::*;
        matches!(
            (self, target),
            // From Created
            (Created, Running) |
            (Created, Terminated) |
            // From Running
            (Running, Paused) |
            (Running, Suspended) |
            (Running, Migrating) |
            (Running, Error) |
            (Running, Terminated) |
            // From Paused
            (Paused, Running) |
            (Paused, Suspended) |
            (Paused, Migrating) |
            (Paused, Terminated) |
            // From Suspended
            (Suspended, Running) |
            (Suspended, Paused) |
            (Suspended, Migrating) |
            (Suspended, Error) |
            (Suspended, Terminated) |
            // From Migrating
            (Migrating, Running) |
            (Migrating, Error) |
            (Migrating, Terminated) |
            // From Error
            (Error, Running) | // Recovery
            (Error, Suspended) | // Suspend for investigation
            (Error, Terminated) // Give up
        )
    }

    /// Check if agent is in an active state (can execute)
    pub fn is_active(&self) -> bool {
        matches!(self, AgentState::Running)
    }

    /// Check if agent can accept new work
    pub fn can_accept_work(&self) -> bool {
        matches!(self, AgentState::Running | AgentState::Created)
    }

    /// Check if agent is in a recoverable state
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            AgentState::Error | AgentState::Suspended | AgentState::Paused
        )
    }

    /// Check if state is terminal (no transitions out)
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Terminated)
    }
}

/// Error information when agent is in Error state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError {
    /// Error message
    pub message: String,
    /// Error code (application-defined)
    pub code: Option<u32>,
    /// When the error occurred
    pub timestamp: u64,
    /// Number of recovery attempts
    pub recovery_attempts: u32,
    /// Whether error is fatal (unrecoverable)
    pub fatal: bool,
}

impl AgentError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            recovery_attempts: 0,
            fatal: false,
        }
    }

    pub fn with_code(mut self, code: u32) -> Self {
        self.code = Some(code);
        self
    }

    pub fn fatal(mut self) -> Self {
        self.fatal = true;
        self
    }

    /// Increment recovery attempts
    pub fn attempt_recovery(&mut self) {
        self.recovery_attempts += 1;
    }
}

/// State transition event for hooks
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub agent_id: AgentId,
    pub from: AgentState,
    pub to: AgentState,
    pub timestamp: Instant,
    pub reason: Option<String>,
}

/// Type alias for state transition hook
pub type TransitionHook = Arc<dyn Fn(&StateTransition) + Send + Sync>;

/// Agent state transition result
#[derive(Debug)]
pub enum TransitionResult {
    /// Transition succeeded
    Success,
    /// Transition not allowed from current state
    InvalidTransition { from: AgentState, to: AgentState },
    /// Transition blocked by hook
    Blocked { reason: String },
}

#[derive(Debug, Clone)]
pub struct Agent {
    id: AgentId,
    dna: Dna,
    state: AgentState,
    policy: Policy,
    /// Error info when in Error state
    error: Option<AgentError>,
    /// Time spent in current state
    state_entered_at: Instant,
    /// History of state transitions (last N)
    state_history: Vec<(AgentState, Instant)>,
}

impl Agent {
    /// Maximum state history entries to keep
    const MAX_STATE_HISTORY: usize = 10;

    pub fn new(wasm_binary: Vec<u8>) -> Self {
        let now = Instant::now();
        Self {
            id: Uuid::new_v4(),
            dna: Dna::new(wasm_binary),
            state: AgentState::Created,
            policy: Policy::default(),
            error: None,
            state_entered_at: now,
            state_history: vec![(AgentState::Created, now)],
        }
    }

    pub fn id(&self) -> AgentId {
        self.id
    }

    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Get time spent in current state
    pub fn time_in_state(&self) -> Duration {
        self.state_entered_at.elapsed()
    }

    /// Get state history
    pub fn state_history(&self) -> &[(AgentState, Instant)] {
        &self.state_history
    }

    /// Transition to a new state with validation
    pub fn transition_to(&mut self, new_state: AgentState) -> TransitionResult {
        self.transition_to_with_reason(new_state, None)
    }

    /// Transition to a new state with reason
    pub fn transition_to_with_reason(
        &mut self,
        new_state: AgentState,
        reason: Option<String>,
    ) -> TransitionResult {
        if !self.state.can_transition_to(&new_state) {
            return TransitionResult::InvalidTransition {
                from: self.state.clone(),
                to: new_state,
            };
        }

        let now = Instant::now();

        // Record history
        self.state_history.push((self.state.clone(), now));
        if self.state_history.len() > Self::MAX_STATE_HISTORY {
            self.state_history.remove(0);
        }

        // Clear error if transitioning out of Error state
        if self.state == AgentState::Error && new_state != AgentState::Error {
            self.error = None;
        }

        self.state = new_state;
        self.state_entered_at = now;

        let _ = reason; // Used for logging/hooks in extended implementation

        TransitionResult::Success
    }

    /// Transition to Error state with error info
    pub fn set_error(&mut self, error: AgentError) -> TransitionResult {
        if !self.state.can_transition_to(&AgentState::Error) {
            return TransitionResult::InvalidTransition {
                from: self.state.clone(),
                to: AgentState::Error,
            };
        }

        let now = Instant::now();
        self.state_history.push((self.state.clone(), now));
        if self.state_history.len() > Self::MAX_STATE_HISTORY {
            self.state_history.remove(0);
        }

        self.error = Some(error);
        self.state = AgentState::Error;
        self.state_entered_at = now;

        TransitionResult::Success
    }

    /// Get current error (if in Error state)
    pub fn error(&self) -> Option<&AgentError> {
        self.error.as_ref()
    }

    /// Attempt recovery from Error state
    pub fn attempt_recovery(&mut self) -> TransitionResult {
        if self.state != AgentState::Error {
            return TransitionResult::InvalidTransition {
                from: self.state.clone(),
                to: AgentState::Running,
            };
        }

        if let Some(ref mut err) = self.error {
            if err.fatal {
                return TransitionResult::Blocked {
                    reason: "Fatal error cannot be recovered".to_string(),
                };
            }
            err.attempt_recovery();
        }

        self.transition_to(AgentState::Running)
    }

    /// Legacy setter - use transition_to for new code
    pub fn set_state(&mut self, state: AgentState) {
        // For backwards compatibility, bypass validation
        let now = Instant::now();
        self.state_history.push((self.state.clone(), now));
        if self.state_history.len() > Self::MAX_STATE_HISTORY {
            self.state_history.remove(0);
        }
        self.state = state;
        self.state_entered_at = now;
    }

    pub fn dna(&self) -> &Dna {
        &self.dna
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: Policy) {
        self.policy = policy;
    }

    /// Check if agent can be paused
    pub fn can_pause(&self) -> bool {
        self.state.can_transition_to(&AgentState::Paused)
    }

    /// Pause agent execution
    pub fn pause(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Paused)
    }

    /// Resume agent from Paused state
    pub fn resume(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Running)
    }

    /// Suspend agent (system-initiated)
    pub fn suspend(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Suspended)
    }

    /// Start agent execution
    pub fn start(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Running)
    }

    /// Terminate agent
    pub fn terminate(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Terminated)
    }

    /// Begin migration
    pub fn begin_migration(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Migrating)
    }

    /// Complete migration (back to Running)
    pub fn complete_migration(&mut self) -> TransitionResult {
        self.transition_to(AgentState::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_transitions() {
        let mut agent = Agent::new(vec![]);
        assert!(matches!(agent.state(), AgentState::Created));

        agent.set_state(AgentState::Running);
        assert!(matches!(agent.state(), AgentState::Running));
    }

    #[test]
    fn test_state_can_transition() {
        // Valid transitions from Created
        assert!(AgentState::Created.can_transition_to(&AgentState::Running));
        assert!(AgentState::Created.can_transition_to(&AgentState::Terminated));
        assert!(!AgentState::Created.can_transition_to(&AgentState::Paused));

        // Valid transitions from Running
        assert!(AgentState::Running.can_transition_to(&AgentState::Paused));
        assert!(AgentState::Running.can_transition_to(&AgentState::Suspended));
        assert!(AgentState::Running.can_transition_to(&AgentState::Error));
        assert!(!AgentState::Running.can_transition_to(&AgentState::Created));

        // Valid transitions from Paused
        assert!(AgentState::Paused.can_transition_to(&AgentState::Running));
        assert!(AgentState::Paused.can_transition_to(&AgentState::Suspended));
        assert!(!AgentState::Paused.can_transition_to(&AgentState::Error));

        // Valid transitions from Error
        assert!(AgentState::Error.can_transition_to(&AgentState::Running));
        assert!(AgentState::Error.can_transition_to(&AgentState::Terminated));
        assert!(!AgentState::Error.can_transition_to(&AgentState::Created));

        // Terminated is terminal
        assert!(!AgentState::Terminated.can_transition_to(&AgentState::Running));
    }

    #[test]
    fn test_state_is_active() {
        assert!(AgentState::Running.is_active());
        assert!(!AgentState::Created.is_active());
        assert!(!AgentState::Paused.is_active());
        assert!(!AgentState::Suspended.is_active());
    }

    #[test]
    fn test_state_is_recoverable() {
        assert!(AgentState::Error.is_recoverable());
        assert!(AgentState::Suspended.is_recoverable());
        assert!(AgentState::Paused.is_recoverable());
        assert!(!AgentState::Running.is_recoverable());
        assert!(!AgentState::Terminated.is_recoverable());
    }

    #[test]
    fn test_state_is_terminal() {
        assert!(AgentState::Terminated.is_terminal());
        assert!(!AgentState::Running.is_terminal());
        assert!(!AgentState::Error.is_terminal());
    }

    #[test]
    fn test_transition_to_valid() {
        let mut agent = Agent::new(vec![]);
        assert!(matches!(agent.start(), TransitionResult::Success));
        assert_eq!(agent.state(), &AgentState::Running);
    }

    #[test]
    fn test_transition_to_invalid() {
        let mut agent = Agent::new(vec![]);
        // Can't go directly from Created to Paused
        let result = agent.pause();
        assert!(matches!(result, TransitionResult::InvalidTransition { .. }));
        // State unchanged
        assert_eq!(agent.state(), &AgentState::Created);
    }

    #[test]
    fn test_pause_resume() {
        let mut agent = Agent::new(vec![]);
        agent.start();
        assert_eq!(agent.state(), &AgentState::Running);

        agent.pause();
        assert_eq!(agent.state(), &AgentState::Paused);

        agent.resume();
        assert_eq!(agent.state(), &AgentState::Running);
    }

    #[test]
    fn test_error_state() {
        let mut agent = Agent::new(vec![]);
        agent.start();

        let error = AgentError::new("Test error").with_code(42);
        agent.set_error(error);

        assert_eq!(agent.state(), &AgentState::Error);
        assert!(agent.error().is_some());
        assert_eq!(agent.error().unwrap().message, "Test error");
        assert_eq!(agent.error().unwrap().code, Some(42));
    }

    #[test]
    fn test_recovery_from_error() {
        let mut agent = Agent::new(vec![]);
        agent.start();
        agent.set_error(AgentError::new("Recoverable error"));

        assert_eq!(agent.state(), &AgentState::Error);

        let result = agent.attempt_recovery();
        assert!(matches!(result, TransitionResult::Success));
        assert_eq!(agent.state(), &AgentState::Running);
        assert!(agent.error().is_none()); // Error cleared
    }

    #[test]
    fn test_fatal_error_no_recovery() {
        let mut agent = Agent::new(vec![]);
        agent.start();
        agent.set_error(AgentError::new("Fatal error").fatal());

        let result = agent.attempt_recovery();
        assert!(matches!(result, TransitionResult::Blocked { .. }));
        assert_eq!(agent.state(), &AgentState::Error);
    }

    #[test]
    fn test_state_history() {
        let mut agent = Agent::new(vec![]);
        assert_eq!(agent.state_history().len(), 1); // Created

        agent.start();
        assert_eq!(agent.state_history().len(), 2);

        agent.pause();
        assert_eq!(agent.state_history().len(), 3);

        agent.resume();
        assert_eq!(agent.state_history().len(), 4);

        // Check last history entry is Paused (before resume to Running)
        let last = agent.state_history().last().unwrap();
        assert_eq!(last.0, AgentState::Paused);
    }

    #[test]
    fn test_migration_lifecycle() {
        let mut agent = Agent::new(vec![]);
        agent.start();

        agent.begin_migration();
        assert_eq!(agent.state(), &AgentState::Migrating);

        agent.complete_migration();
        assert_eq!(agent.state(), &AgentState::Running);
    }

    #[test]
    fn test_agent_error_recovery_attempts() {
        let mut error = AgentError::new("Test error");
        assert_eq!(error.recovery_attempts, 0);

        error.attempt_recovery();
        assert_eq!(error.recovery_attempts, 1);

        error.attempt_recovery();
        assert_eq!(error.recovery_attempts, 2);
    }

    #[test]
    fn test_time_in_state() {
        let agent = Agent::new(vec![]);
        let time = agent.time_in_state();
        // Should be very short since we just created it
        assert!(time < Duration::from_secs(1));
    }

    #[test]
    fn test_can_accept_work() {
        assert!(AgentState::Created.can_accept_work());
        assert!(AgentState::Running.can_accept_work());
        assert!(!AgentState::Paused.can_accept_work());
        assert!(!AgentState::Suspended.can_accept_work());
        assert!(!AgentState::Error.can_accept_work());
    }

    #[test]
    fn test_can_pause() {
        let mut agent = Agent::new(vec![]);
        assert!(!agent.can_pause()); // Created can't pause

        agent.start();
        assert!(agent.can_pause()); // Running can pause

        agent.pause();
        assert!(!agent.can_pause()); // Already paused
    }

    #[test]
    fn test_terminate_from_any_state() {
        // From Running
        let mut agent = Agent::new(vec![]);
        agent.start();
        assert!(matches!(agent.terminate(), TransitionResult::Success));

        // From Paused
        let mut agent = Agent::new(vec![]);
        agent.start();
        agent.pause();
        assert!(matches!(agent.terminate(), TransitionResult::Success));

        // From Error
        let mut agent = Agent::new(vec![]);
        agent.start();
        agent.set_error(AgentError::new("error"));
        assert!(matches!(agent.terminate(), TransitionResult::Success));
    }
}
