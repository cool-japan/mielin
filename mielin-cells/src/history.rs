//! Agent State History
//!
//! Provides comprehensive tracking and querying of agent state history,
//! enabling temporal analysis and behavior pattern detection.

use crate::{AgentId, AgentState};
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A single state history entry
#[derive(Debug, Clone)]
pub struct StateHistoryEntry {
    /// Agent ID
    pub agent_id: AgentId,
    /// State at this point
    pub state: AgentState,
    /// When the agent entered this state
    pub entered_at: u64,
    /// When the agent left this state (None if current state)
    pub exited_at: Option<u64>,
    /// Duration in this state (milliseconds)
    pub duration_ms: Option<u64>,
    /// Previous state (if any)
    pub previous_state: Option<AgentState>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl StateHistoryEntry {
    /// Create a new history entry
    pub fn new(agent_id: AgentId, state: AgentState) -> Self {
        Self {
            agent_id,
            state,
            entered_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            exited_at: None,
            duration_ms: None,
            previous_state: None,
            metadata: HashMap::new(),
        }
    }

    /// Create entry with previous state
    pub fn with_previous(mut self, previous: AgentState) -> Self {
        self.previous_state = Some(previous);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Mark entry as completed (agent left this state)
    pub fn complete(&mut self, exited_at: u64) {
        self.exited_at = Some(exited_at);
        if exited_at >= self.entered_at {
            self.duration_ms = Some(exited_at - self.entered_at);
        }
    }

    /// Check if entry is completed (has exit time)
    pub fn is_completed(&self) -> bool {
        self.exited_at.is_some()
    }

    /// Get duration if completed
    pub fn duration(&self) -> Option<Duration> {
        self.duration_ms.map(Duration::from_millis)
    }
}

/// Configuration for state history tracking
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    /// Maximum entries per agent (0 = unlimited)
    pub max_entries: usize,
    /// Whether to track metadata
    pub track_metadata: bool,
    /// Whether to compute statistics
    pub compute_stats: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            track_metadata: true,
            compute_stats: true,
        }
    }
}

/// State transition statistics
#[derive(Debug, Clone, Default)]
pub struct StateStatistics {
    /// Total state transitions
    pub total_transitions: usize,
    /// Time spent in each state (milliseconds)
    pub time_in_states: HashMap<String, u64>,
    /// Transition counts (from_state -> to_state -> count)
    pub transition_counts: HashMap<String, HashMap<String, usize>>,
    /// Average duration in each state (milliseconds)
    pub avg_duration: HashMap<String, u64>,
}

impl StateStatistics {
    /// Add a transition to statistics
    pub fn record_transition(&mut self, from: &AgentState, to: &AgentState, duration_ms: u64) {
        self.total_transitions += 1;

        // Record time in state
        let state_key = format!("{:?}", from);
        *self.time_in_states.entry(state_key.clone()).or_insert(0) += duration_ms;

        // Record transition
        let to_key = format!("{:?}", to);
        self.transition_counts
            .entry(state_key)
            .or_default()
            .entry(to_key)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    /// Compute average durations
    pub fn compute_averages(&mut self, state_counts: &HashMap<String, usize>) {
        self.avg_duration.clear();
        for (state, total_time) in &self.time_in_states {
            if let Some(&count) = state_counts.get(state) {
                if count > 0 {
                    self.avg_duration
                        .insert(state.clone(), total_time / count as u64);
                }
            }
        }
    }

    /// Get most common transition
    pub fn most_common_transition(&self) -> Option<(String, String, usize)> {
        let mut max_count = 0;
        let mut max_transition = None;

        for (from, transitions) in &self.transition_counts {
            for (to, &count) in transitions {
                if count > max_count {
                    max_count = count;
                    max_transition = Some((from.clone(), to.clone(), count));
                }
            }
        }

        max_transition
    }
}

/// State history tracker for a single agent
#[derive(Clone)]
pub struct AgentHistory {
    agent_id: AgentId,
    entries: VecDeque<StateHistoryEntry>,
    config: HistoryConfig,
    statistics: StateStatistics,
    current_entry: Option<StateHistoryEntry>,
}

impl AgentHistory {
    /// Create a new agent history tracker
    pub fn new(agent_id: AgentId) -> Self {
        Self::with_config(agent_id, HistoryConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(agent_id: AgentId, config: HistoryConfig) -> Self {
        Self {
            agent_id,
            entries: VecDeque::new(),
            config,
            statistics: StateStatistics::default(),
            current_entry: None,
        }
    }

    /// Record a state transition
    pub fn record_transition(&mut self, new_state: AgentState) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Complete previous entry if exists
        if let Some(mut prev_entry) = self.current_entry.take() {
            prev_entry.complete(now);

            // Update statistics
            if self.config.compute_stats {
                if let Some(duration) = prev_entry.duration_ms {
                    self.statistics
                        .record_transition(&prev_entry.state, &new_state, duration);
                }
            }

            // Add to history
            self.entries.push_back(prev_entry);

            // Enforce max entries limit
            if self.config.max_entries > 0 && self.entries.len() > self.config.max_entries {
                self.entries.pop_front();
            }
        }

        // Create new current entry
        let mut entry = StateHistoryEntry::new(self.agent_id, new_state.clone());
        if let Some(last) = self.entries.back() {
            entry = entry.with_previous(last.state.clone());
        }
        self.current_entry = Some(entry);
    }

    /// Get all history entries
    pub fn entries(&self) -> Vec<StateHistoryEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Get entries in time range
    pub fn entries_in_range(&self, start_ms: u64, end_ms: u64) -> Vec<StateHistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.entered_at >= start_ms && e.entered_at <= end_ms)
            .cloned()
            .collect()
    }

    /// Get entries for specific state
    pub fn entries_for_state(&self, state: &AgentState) -> Vec<StateHistoryEntry> {
        self.entries
            .iter()
            .filter(|e| &e.state == state)
            .cloned()
            .collect()
    }

    /// Get current state entry
    pub fn current_entry(&self) -> Option<&StateHistoryEntry> {
        self.current_entry.as_ref()
    }

    /// Get statistics
    pub fn statistics(&self) -> &StateStatistics {
        &self.statistics
    }

    /// Get total entries count
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_entry = None;
        self.statistics = StateStatistics::default();
    }

    /// Get latest N entries
    pub fn latest(&self, n: usize) -> Vec<StateHistoryEntry> {
        self.entries.iter().rev().take(n).cloned().collect()
    }
}

/// Global history tracker for all agents
pub struct HistoryTracker {
    histories: RwLock<HashMap<AgentId, AgentHistory>>,
    config: HistoryConfig,
}

impl HistoryTracker {
    /// Create a new history tracker
    pub fn new() -> Self {
        Self::with_config(HistoryConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: HistoryConfig) -> Self {
        Self {
            histories: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Register an agent for history tracking
    pub fn register_agent(&self, agent_id: AgentId, initial_state: AgentState) {
        let mut histories = self.histories.write().expect("Lock poisoned: histories");
        let mut history = AgentHistory::with_config(agent_id, self.config.clone());
        history.record_transition(initial_state);
        histories.insert(agent_id, history);
    }

    /// Unregister an agent
    pub fn unregister_agent(&self, agent_id: &AgentId) -> Option<AgentHistory> {
        self.histories
            .write()
            .expect("Lock poisoned: histories")
            .remove(agent_id)
    }

    /// Record a state transition for an agent
    pub fn record_transition(&self, agent_id: &AgentId, new_state: AgentState) {
        let mut histories = self.histories.write().expect("Lock poisoned: histories");
        if let Some(history) = histories.get_mut(agent_id) {
            history.record_transition(new_state);
        } else {
            // Auto-register if not exists
            let mut history = AgentHistory::with_config(*agent_id, self.config.clone());
            history.record_transition(new_state);
            histories.insert(*agent_id, history);
        }
    }

    /// Get history for an agent
    pub fn get_history(&self, agent_id: &AgentId) -> Option<AgentHistory> {
        self.histories
            .read()
            .expect("Lock poisoned: histories")
            .get(agent_id)
            .cloned()
    }

    /// Query entries across all agents in time range
    pub fn query_range(&self, start_ms: u64, end_ms: u64) -> Vec<StateHistoryEntry> {
        let histories = self.histories.read().expect("Lock poisoned: histories");
        histories
            .values()
            .flat_map(|h| h.entries_in_range(start_ms, end_ms))
            .collect()
    }

    /// Query entries for specific state across all agents
    pub fn query_state(&self, state: &AgentState) -> Vec<StateHistoryEntry> {
        let histories = self.histories.read().expect("Lock poisoned: histories");
        histories
            .values()
            .flat_map(|h| h.entries_for_state(state))
            .collect()
    }

    /// Get aggregate statistics across all agents
    pub fn aggregate_statistics(&self) -> StateStatistics {
        let histories = self.histories.read().expect("Lock poisoned: histories");
        let mut agg = StateStatistics::default();

        for history in histories.values() {
            let stats = history.statistics();
            agg.total_transitions += stats.total_transitions;

            for (state, time) in &stats.time_in_states {
                *agg.time_in_states.entry(state.clone()).or_insert(0) += time;
            }

            for (from, transitions) in &stats.transition_counts {
                for (to, count) in transitions {
                    *agg.transition_counts
                        .entry(from.clone())
                        .or_default()
                        .entry(to.clone())
                        .or_insert(0) += count;
                }
            }
        }

        agg
    }

    /// Get number of tracked agents
    pub fn agent_count(&self) -> usize {
        self.histories
            .read()
            .expect("Lock poisoned: histories")
            .len()
    }

    /// Get all agent IDs being tracked
    pub fn tracked_agents(&self) -> Vec<AgentId> {
        self.histories
            .read()
            .expect("Lock poisoned: histories")
            .keys()
            .copied()
            .collect()
    }

    /// Clear history for an agent
    pub fn clear_agent_history(&self, agent_id: &AgentId) {
        if let Some(history) = self
            .histories
            .write()
            .expect("Lock poisoned: histories")
            .get_mut(agent_id)
        {
            history.clear();
        }
    }

    /// Clear all history
    pub fn clear_all(&self) {
        self.histories
            .write()
            .expect("Lock poisoned: histories")
            .clear();
    }
}

impl Default for HistoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_history_entry() {
        let agent_id = Uuid::new_v4();
        let mut entry = StateHistoryEntry::new(agent_id, AgentState::Created);

        assert_eq!(entry.state, AgentState::Created);
        assert!(!entry.is_completed());

        let exit_time = entry.entered_at + 1000;
        entry.complete(exit_time);

        assert!(entry.is_completed());
        assert_eq!(entry.duration_ms, Some(1000));
    }

    #[test]
    fn test_agent_history() {
        let agent_id = Uuid::new_v4();
        let mut history = AgentHistory::new(agent_id);

        history.record_transition(AgentState::Created);
        std::thread::sleep(Duration::from_millis(10));
        history.record_transition(AgentState::Running);
        std::thread::sleep(Duration::from_millis(10));
        history.record_transition(AgentState::Paused);

        assert_eq!(history.entry_count(), 2); // 2 completed transitions
    }

    #[test]
    fn test_entries_for_state() {
        let agent_id = Uuid::new_v4();
        let mut history = AgentHistory::new(agent_id);

        history.record_transition(AgentState::Created);
        history.record_transition(AgentState::Running);
        history.record_transition(AgentState::Paused);
        history.record_transition(AgentState::Running);
        history.record_transition(AgentState::Terminated);

        let running_entries = history.entries_for_state(&AgentState::Running);
        assert_eq!(running_entries.len(), 2);
    }

    #[test]
    fn test_latest_entries() {
        let agent_id = Uuid::new_v4();
        let mut history = AgentHistory::new(agent_id);

        for _ in 0..5 {
            history.record_transition(AgentState::Running);
            std::thread::sleep(Duration::from_millis(1));
            history.record_transition(AgentState::Paused);
            std::thread::sleep(Duration::from_millis(1));
        }

        let latest_3 = history.latest(3);
        assert_eq!(latest_3.len(), 3);
    }

    #[test]
    fn test_history_tracker() {
        let tracker = HistoryTracker::new();
        let agent_id = Uuid::new_v4();

        tracker.register_agent(agent_id, AgentState::Created);
        assert_eq!(tracker.agent_count(), 1);

        tracker.record_transition(&agent_id, AgentState::Running);
        tracker.record_transition(&agent_id, AgentState::Paused);

        let history = tracker.get_history(&agent_id).unwrap();
        assert_eq!(history.entry_count(), 2);
    }

    #[test]
    fn test_query_state() {
        let tracker = HistoryTracker::new();

        let agent1 = Uuid::new_v4();
        let agent2 = Uuid::new_v4();

        tracker.register_agent(agent1, AgentState::Created);
        tracker.register_agent(agent2, AgentState::Created);

        tracker.record_transition(&agent1, AgentState::Running);
        tracker.record_transition(&agent2, AgentState::Running);
        tracker.record_transition(&agent1, AgentState::Paused);
        tracker.record_transition(&agent2, AgentState::Paused);

        let running_entries = tracker.query_state(&AgentState::Running);
        assert_eq!(running_entries.len(), 2);
    }

    #[test]
    fn test_max_entries_limit() {
        let config = HistoryConfig {
            max_entries: 3,
            ..Default::default()
        };

        let agent_id = Uuid::new_v4();
        let mut history = AgentHistory::with_config(agent_id, config);

        for _ in 0..5 {
            history.record_transition(AgentState::Running);
            std::thread::sleep(Duration::from_millis(1));
        }

        // Should only keep last 3 completed entries
        assert!(history.entry_count() <= 3);
    }

    #[test]
    fn test_statistics() {
        let agent_id = Uuid::new_v4();
        let mut history = AgentHistory::new(agent_id);

        history.record_transition(AgentState::Created);
        std::thread::sleep(Duration::from_millis(10));
        history.record_transition(AgentState::Running);
        std::thread::sleep(Duration::from_millis(10));
        history.record_transition(AgentState::Paused);

        let stats = history.statistics();
        assert!(stats.total_transitions > 0);
    }

    #[test]
    fn test_clear_history() {
        let agent_id = Uuid::new_v4();
        let mut history = AgentHistory::new(agent_id);

        history.record_transition(AgentState::Created);
        history.record_transition(AgentState::Running);

        history.clear();
        assert_eq!(history.entry_count(), 0);
    }
}
