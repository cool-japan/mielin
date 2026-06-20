//! Network Partition Tolerance and Split-Brain Detection
//!
//! Provides mechanisms for detecting and handling network partitions:
//! - Split-brain detection using quorum-based voting
//! - Partition event notification
//! - Recovery coordination
//! - Consistent hashing for state rebalancing

use crate::{Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Type alias for partition event handler
type EventHandler = Box<dyn Fn(PartitionEvent) + Send + Sync>;

/// Type alias for shared event handlers collection
type EventHandlers = Arc<RwLock<Vec<EventHandler>>>;

/// Partition check interval
const PARTITION_CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Minimum quorum ratio (>50% to prevent split-brain)
const QUORUM_RATIO: f64 = 0.5;

/// Time to wait before declaring partition stable
const _PARTITION_STABILIZATION_TIME: Duration = Duration::from_secs(30);

/// Maximum time to wait for partition recovery
const _PARTITION_RECOVERY_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Error)]
pub enum PartitionError {
    #[error("Split-brain detected: multiple partitions claim leadership")]
    SplitBrainDetected,
    #[error("No quorum: {0} nodes out of {1} required")]
    NoQuorum(usize, usize),
    #[error("Partition recovery failed: {0}")]
    RecoveryFailed(String),
    #[error("Invalid partition state: {0}")]
    InvalidState(String),
}

/// Partition state for the local node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionState {
    /// Normal operation - no partition detected
    Normal,
    /// Partition suspected - reduced connectivity
    Suspected,
    /// Partition confirmed - operating in degraded mode
    Partitioned,
    /// Recovery in progress
    Recovering,
}

/// Information about a network partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// Unique identifier for this partition view
    pub partition_id: u64,
    /// Nodes visible in this partition
    pub visible_nodes: HashSet<NodeId>,
    /// Total known nodes in the cluster
    pub known_nodes: usize,
    /// Whether this partition has quorum
    pub has_quorum: bool,
    /// Leader node for this partition (if elected)
    pub leader: Option<NodeId>,
    /// When partition was first detected
    pub detected_at: SystemTime,
    /// Estimated partition cause
    pub cause: PartitionCause,
}

impl PartitionInfo {
    pub fn new(visible: HashSet<NodeId>, known: usize) -> Self {
        let visible_count = visible.len();
        let has_quorum = (visible_count as f64 / known as f64) > QUORUM_RATIO;

        Self {
            partition_id: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            visible_nodes: visible,
            known_nodes: known,
            has_quorum,
            leader: None,
            detected_at: SystemTime::now(),
            cause: PartitionCause::Unknown,
        }
    }

    /// Calculate quorum size needed
    pub fn quorum_size(&self) -> usize {
        (self.known_nodes as f64 * QUORUM_RATIO).ceil() as usize + 1
    }

    /// Check if we can form a valid quorum
    pub fn can_form_quorum(&self) -> bool {
        self.visible_nodes.len() >= self.quorum_size()
    }

    /// Get the partition ratio
    pub fn partition_ratio(&self) -> f64 {
        if self.known_nodes == 0 {
            return 0.0;
        }
        self.visible_nodes.len() as f64 / self.known_nodes as f64
    }

    /// Duration since partition was detected
    pub fn duration(&self) -> Duration {
        self.detected_at.elapsed().unwrap_or_default()
    }
}

/// Estimated cause of partition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionCause {
    Unknown,
    NetworkFailure,
    NodeCrash,
    ConfigurationError,
    HighLatency,
}

/// Event emitted during partition handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionEvent {
    /// Partition detected
    PartitionDetected(PartitionInfo),
    /// Quorum lost
    QuorumLost { visible: usize, required: usize },
    /// Quorum regained
    QuorumRegained { visible: usize, total: usize },
    /// Split-brain detected
    SplitBrainDetected { partitions: Vec<PartitionInfo> },
    /// Recovery started
    RecoveryStarted { partition_id: u64 },
    /// Recovery completed
    RecoveryCompleted { duration: Duration },
    /// Node rejoined after partition
    NodeRejoined { node_id: NodeId },
    /// Partition resolved
    PartitionResolved { duration: Duration },
}

/// Partition detector and handler
pub struct PartitionDetector {
    local_node: Arc<Node>,
    state: Arc<RwLock<PartitionState>>,
    current_partition: Arc<RwLock<Option<PartitionInfo>>>,
    visible_nodes: Arc<RwLock<HashSet<NodeId>>>,
    known_nodes: Arc<RwLock<HashSet<NodeId>>>,
    event_handlers: EventHandlers,
    partition_history: Arc<RwLock<Vec<PartitionInfo>>>,
}

impl PartitionDetector {
    pub fn new(node: Arc<Node>) -> Self {
        let node_id = *node.id();
        let mut visible = HashSet::new();
        visible.insert(node_id);
        let mut known = HashSet::new();
        known.insert(node_id);

        Self {
            local_node: node,
            state: Arc::new(RwLock::new(PartitionState::Normal)),
            current_partition: Arc::new(RwLock::new(None)),
            visible_nodes: Arc::new(RwLock::new(visible)),
            known_nodes: Arc::new(RwLock::new(known)),
            event_handlers: Arc::new(RwLock::new(Vec::new())),
            partition_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start partition detection
    pub async fn start(&self) {
        info!(
            "Starting partition detector for node {}",
            self.local_node.id()
        );
        self.spawn_detection_task();
    }

    /// Spawn the main detection task
    fn spawn_detection_task(&self) {
        let state = self.state.clone();
        let current_partition = self.current_partition.clone();
        let visible_nodes = self.visible_nodes.clone();
        let known_nodes = self.known_nodes.clone();
        let event_handlers = self.event_handlers.clone();
        let partition_history = self.partition_history.clone();
        let local_id = *self.local_node.id();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(PARTITION_CHECK_INTERVAL);

            loop {
                interval.tick().await;

                let visible = visible_nodes.read().await;
                let known = known_nodes.read().await;
                let mut current_state = state.write().await;

                let visible_count = visible.len();
                let known_count = known.len();

                if known_count == 0 {
                    continue;
                }

                let quorum_size = (known_count as f64 * QUORUM_RATIO).ceil() as usize + 1;
                let has_quorum = visible_count >= quorum_size;

                match *current_state {
                    PartitionState::Normal => {
                        if !has_quorum && visible_count < known_count {
                            // Potential partition detected
                            *current_state = PartitionState::Suspected;
                            debug!(
                                "Partition suspected: {} visible out of {} known nodes",
                                visible_count, known_count
                            );
                        }
                    }
                    PartitionState::Suspected => {
                        if has_quorum && visible_count == known_count {
                            // False alarm - back to normal
                            *current_state = PartitionState::Normal;
                            debug!("Partition suspicion cleared");
                        } else if !has_quorum {
                            // Confirm partition
                            *current_state = PartitionState::Partitioned;

                            let partition_info = PartitionInfo::new(visible.clone(), known_count);

                            let mut current = current_partition.write().await;
                            *current = Some(partition_info.clone());

                            // Record in history
                            let mut history = partition_history.write().await;
                            history.push(partition_info.clone());

                            // Emit event
                            let handlers = event_handlers.read().await;
                            let event = PartitionEvent::PartitionDetected(partition_info.clone());
                            for handler in handlers.iter() {
                                handler(event.clone());
                            }

                            warn!(
                                "Partition confirmed: {} visible, {} required for quorum",
                                visible_count, quorum_size
                            );

                            // Emit quorum lost event
                            let event = PartitionEvent::QuorumLost {
                                visible: visible_count,
                                required: quorum_size,
                            };
                            for handler in handlers.iter() {
                                handler(event.clone());
                            }
                        }
                    }
                    PartitionState::Partitioned => {
                        if has_quorum {
                            // Start recovery
                            *current_state = PartitionState::Recovering;

                            let handlers = event_handlers.read().await;
                            if let Some(partition) = current_partition.read().await.as_ref() {
                                let event = PartitionEvent::RecoveryStarted {
                                    partition_id: partition.partition_id,
                                };
                                for handler in handlers.iter() {
                                    handler(event.clone());
                                }
                            }

                            info!("Quorum regained, starting partition recovery");
                        }
                    }
                    PartitionState::Recovering => {
                        if visible_count >= known_count {
                            // Recovery complete
                            *current_state = PartitionState::Normal;

                            let handlers = event_handlers.read().await;
                            if let Some(partition) = current_partition.read().await.as_ref() {
                                let duration = partition.duration();

                                let event = PartitionEvent::RecoveryCompleted { duration };
                                for handler in handlers.iter() {
                                    handler(event.clone());
                                }

                                let event = PartitionEvent::PartitionResolved { duration };
                                for handler in handlers.iter() {
                                    handler(event.clone());
                                }
                            }

                            let mut current = current_partition.write().await;
                            *current = None;

                            info!(
                                "Partition recovery complete: all {} nodes visible",
                                known_count
                            );
                        }
                    }
                }

                drop(visible);
                drop(known);
                drop(current_state);

                // Check for new nodes joining
                Self::check_rejoined_nodes(local_id, &visible_nodes, &known_nodes, &event_handlers)
                    .await;
            }
        });
    }

    /// Check for nodes that have rejoined
    async fn check_rejoined_nodes(
        _local_id: NodeId,
        visible_nodes: &Arc<RwLock<HashSet<NodeId>>>,
        known_nodes: &Arc<RwLock<HashSet<NodeId>>>,
        event_handlers: &EventHandlers,
    ) {
        let visible = visible_nodes.read().await;
        let known = known_nodes.read().await;

        // Find nodes that were known but not visible, now visible again
        let rejoined: Vec<NodeId> = visible
            .iter()
            .filter(|id| known.contains(id))
            .cloned()
            .collect();

        if !rejoined.is_empty() {
            let handlers = event_handlers.read().await;
            for node_id in rejoined {
                debug!("Node {} rejoined the cluster", node_id);
                let event = PartitionEvent::NodeRejoined { node_id };
                for handler in handlers.iter() {
                    handler(event.clone());
                }
            }
        }
    }

    /// Register a node as visible (connected)
    pub async fn mark_node_visible(&self, node_id: NodeId) {
        let mut visible = self.visible_nodes.write().await;
        visible.insert(node_id);

        let mut known = self.known_nodes.write().await;
        known.insert(node_id);
    }

    /// Register a node as invisible (disconnected)
    pub async fn mark_node_invisible(&self, node_id: NodeId) {
        let mut visible = self.visible_nodes.write().await;
        visible.remove(&node_id);
    }

    /// Add a known node to the cluster
    pub async fn add_known_node(&self, node_id: NodeId) {
        let mut known = self.known_nodes.write().await;
        known.insert(node_id);
    }

    /// Remove a node from known nodes (permanent removal)
    pub async fn remove_known_node(&self, node_id: NodeId) {
        let mut known = self.known_nodes.write().await;
        known.remove(&node_id);

        let mut visible = self.visible_nodes.write().await;
        visible.remove(&node_id);
    }

    /// Get current partition state
    pub async fn get_state(&self) -> PartitionState {
        *self.state.read().await
    }

    /// Get current partition info (if in partitioned state)
    pub async fn get_partition_info(&self) -> Option<PartitionInfo> {
        self.current_partition.read().await.clone()
    }

    /// Check if we currently have quorum
    pub async fn has_quorum(&self) -> bool {
        let visible = self.visible_nodes.read().await;
        let known = self.known_nodes.read().await;

        // Single-node or empty cluster always has quorum
        if known.len() <= 1 {
            return true;
        }

        let quorum_size = (known.len() as f64 * QUORUM_RATIO).ceil() as usize + 1;
        visible.len() >= quorum_size
    }

    /// Get the number of visible nodes
    pub async fn visible_count(&self) -> usize {
        self.visible_nodes.read().await.len()
    }

    /// Get the number of known nodes
    pub async fn known_count(&self) -> usize {
        self.known_nodes.read().await.len()
    }

    /// Register an event handler
    pub async fn on_event(&self, handler: impl Fn(PartitionEvent) + Send + Sync + 'static) {
        let mut handlers = self.event_handlers.write().await;
        handlers.push(Box::new(handler));
    }

    /// Get partition history
    pub async fn get_history(&self) -> Vec<PartitionInfo> {
        self.partition_history.read().await.clone()
    }

    /// Force partition recovery (for manual intervention)
    pub async fn force_recovery(&self) -> Result<(), PartitionError> {
        let mut state = self.state.write().await;

        if *state != PartitionState::Partitioned {
            return Err(PartitionError::InvalidState(
                "Not in partitioned state".to_string(),
            ));
        }

        *state = PartitionState::Recovering;
        info!("Forced partition recovery initiated");

        Ok(())
    }

    /// Clear partition history
    pub async fn clear_history(&self) {
        let mut history = self.partition_history.write().await;
        history.clear();
    }
}

/// Quorum-based decision maker
pub struct QuorumDecision {
    /// Required ratio for quorum
    pub quorum_ratio: f64,
    /// Current votes
    votes: HashMap<NodeId, bool>,
    /// Total eligible voters
    total_voters: usize,
}

impl QuorumDecision {
    pub fn new(total_voters: usize) -> Self {
        Self {
            quorum_ratio: QUORUM_RATIO,
            votes: HashMap::new(),
            total_voters,
        }
    }

    /// Submit a vote
    pub fn vote(&mut self, node_id: NodeId, decision: bool) {
        self.votes.insert(node_id, decision);
    }

    /// Check if quorum has been reached
    pub fn has_quorum(&self) -> bool {
        let quorum_size = (self.total_voters as f64 * self.quorum_ratio).ceil() as usize + 1;
        self.votes.len() >= quorum_size
    }

    /// Get the current decision (if quorum reached)
    pub fn get_decision(&self) -> Option<bool> {
        if !self.has_quorum() {
            return None;
        }

        let yes_votes = self.votes.values().filter(|&&v| v).count();
        let no_votes = self.votes.len() - yes_votes;

        if yes_votes > no_votes {
            Some(true)
        } else if no_votes > yes_votes {
            Some(false)
        } else {
            None // Tie
        }
    }

    /// Get vote counts
    pub fn vote_counts(&self) -> (usize, usize) {
        let yes = self.votes.values().filter(|&&v| v).count();
        let no = self.votes.len() - yes;
        (yes, no)
    }

    /// Reset votes
    pub fn reset(&mut self) {
        self.votes.clear();
    }
}

/// Consistent hash ring for state rebalancing
pub struct ConsistentHashRing {
    ring: Vec<(u64, NodeId)>,
    virtual_nodes: usize,
}

impl ConsistentHashRing {
    const DEFAULT_VIRTUAL_NODES: usize = 150;

    pub fn new() -> Self {
        Self {
            ring: Vec::new(),
            virtual_nodes: Self::DEFAULT_VIRTUAL_NODES,
        }
    }

    /// Add a node to the ring
    pub fn add_node(&mut self, node_id: NodeId) {
        for i in 0..self.virtual_nodes {
            let hash = Self::hash_key(&format!("{}:{}", node_id, i));
            self.ring.push((hash, node_id));
        }
        self.ring.sort_by_key(|(hash, _)| *hash);
    }

    /// Remove a node from the ring
    pub fn remove_node(&mut self, node_id: NodeId) {
        self.ring.retain(|(_, id)| *id != node_id);
    }

    /// Get the node responsible for a key
    pub fn get_node(&self, key: &str) -> Option<NodeId> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = Self::hash_key(key);

        // Binary search for the first node with hash >= key hash
        match self.ring.binary_search_by_key(&hash, |(h, _)| *h) {
            Ok(idx) => Some(self.ring[idx].1),
            Err(idx) => {
                if idx >= self.ring.len() {
                    Some(self.ring[0].1) // Wrap around
                } else {
                    Some(self.ring[idx].1)
                }
            }
        }
    }

    /// Get N nodes responsible for a key (for replication)
    pub fn get_nodes(&self, key: &str, count: usize) -> Vec<NodeId> {
        if self.ring.is_empty() {
            return Vec::new();
        }

        let hash = Self::hash_key(key);
        let mut nodes = Vec::new();
        let mut seen = HashSet::new();

        let start_idx = match self.ring.binary_search_by_key(&hash, |(h, _)| *h) {
            Ok(idx) => idx,
            Err(idx) => idx % self.ring.len(),
        };

        for i in 0..self.ring.len() {
            if nodes.len() >= count {
                break;
            }

            let idx = (start_idx + i) % self.ring.len();
            let node_id = self.ring[idx].1;

            if !seen.contains(&node_id) {
                nodes.push(node_id);
                seen.insert(node_id);
            }
        }

        nodes
    }

    /// Get keys that need to be moved when a node joins
    pub fn get_affected_keys_on_join(&self, _new_node: NodeId, keys: &[String]) -> Vec<String> {
        // Keys that would now be owned by the new node
        keys.iter()
            .filter(|key| self.get_node(key).is_some())
            .cloned()
            .collect()
    }

    /// Simple hash function
    fn hash_key(key: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    /// Get the number of nodes in the ring
    pub fn node_count(&self) -> usize {
        let mut seen = HashSet::new();
        for (_, id) in &self.ring {
            seen.insert(*id);
        }
        seen.len()
    }

    /// Check if the ring is empty
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

impl Default for ConsistentHashRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeRole;

    #[test]
    fn test_partition_info_creation() {
        let mut visible = HashSet::new();
        visible.insert(NodeId::new_v4());
        visible.insert(NodeId::new_v4());

        let info = PartitionInfo::new(visible.clone(), 5);

        assert_eq!(info.visible_nodes.len(), 2);
        assert_eq!(info.known_nodes, 5);
        assert!(!info.has_quorum); // 2/5 = 40% < 50%
    }

    #[test]
    fn test_partition_info_quorum() {
        let mut visible = HashSet::new();
        for _ in 0..6 {
            visible.insert(NodeId::new_v4());
        }

        let info = PartitionInfo::new(visible, 10);

        assert!(info.can_form_quorum()); // 6/10 = 60% > 50%
    }

    #[test]
    fn test_partition_ratio() {
        let mut visible = HashSet::new();
        visible.insert(NodeId::new_v4());
        visible.insert(NodeId::new_v4());
        visible.insert(NodeId::new_v4());

        let info = PartitionInfo::new(visible, 10);

        assert!((info.partition_ratio() - 0.3).abs() < 0.001);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_partition_detector_creation() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let detector = PartitionDetector::new(node.clone());

        assert_eq!(detector.get_state().await, PartitionState::Normal);
        assert!(detector.has_quorum().await);
        assert_eq!(detector.visible_count().await, 1);
        assert_eq!(detector.known_count().await, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mark_node_visible() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let detector = PartitionDetector::new(node.clone());

        let new_node = NodeId::new_v4();
        detector.mark_node_visible(new_node).await;

        assert_eq!(detector.visible_count().await, 2);
        assert_eq!(detector.known_count().await, 2);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_mark_node_invisible() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let detector = PartitionDetector::new(node.clone());

        let new_node = NodeId::new_v4();
        detector.mark_node_visible(new_node).await;
        detector.mark_node_invisible(new_node).await;

        assert_eq!(detector.visible_count().await, 1);
        assert_eq!(detector.known_count().await, 2); // Still known, just not visible
    }

    #[test]
    fn test_quorum_decision() {
        let mut decision = QuorumDecision::new(5);

        decision.vote(NodeId::new_v4(), true);
        assert!(!decision.has_quorum());

        decision.vote(NodeId::new_v4(), true);
        decision.vote(NodeId::new_v4(), false);
        assert!(!decision.has_quorum());

        decision.vote(NodeId::new_v4(), true);
        assert!(decision.has_quorum());

        let result = decision.get_decision();
        assert_eq!(result, Some(true)); // 3 yes, 1 no
    }

    #[test]
    fn test_quorum_decision_tie() {
        let mut decision = QuorumDecision::new(4);

        decision.vote(NodeId::new_v4(), true);
        decision.vote(NodeId::new_v4(), true);
        decision.vote(NodeId::new_v4(), false);
        decision.vote(NodeId::new_v4(), false);

        assert!(decision.has_quorum());
        assert_eq!(decision.get_decision(), None); // Tie
    }

    #[test]
    fn test_consistent_hash_ring() {
        let mut ring = ConsistentHashRing::new();

        let node1 = NodeId::new_v4();
        let node2 = NodeId::new_v4();
        let node3 = NodeId::new_v4();

        ring.add_node(node1);
        ring.add_node(node2);
        ring.add_node(node3);

        assert_eq!(ring.node_count(), 3);

        // Same key should always return same node
        let key = "test_key";
        let responsible = ring.get_node(key);
        assert!(responsible.is_some());

        let responsible2 = ring.get_node(key);
        assert_eq!(responsible, responsible2);
    }

    #[test]
    fn test_consistent_hash_ring_replication() {
        let mut ring = ConsistentHashRing::new();

        let node1 = NodeId::new_v4();
        let node2 = NodeId::new_v4();
        let node3 = NodeId::new_v4();

        ring.add_node(node1);
        ring.add_node(node2);
        ring.add_node(node3);

        let nodes = ring.get_nodes("test_key", 2);
        assert_eq!(nodes.len(), 2);

        // Nodes should be unique
        let unique: HashSet<_> = nodes.iter().collect();
        assert_eq!(unique.len(), 2);
    }

    #[test]
    fn test_consistent_hash_ring_remove() {
        let mut ring = ConsistentHashRing::new();

        let node1 = NodeId::new_v4();
        let node2 = NodeId::new_v4();

        ring.add_node(node1);
        ring.add_node(node2);

        assert_eq!(ring.node_count(), 2);

        ring.remove_node(node1);

        assert_eq!(ring.node_count(), 1);
    }

    #[test]
    fn test_consistent_hash_ring_empty() {
        let ring = ConsistentHashRing::new();

        assert!(ring.is_empty());
        assert_eq!(ring.get_node("key"), None);
        assert!(ring.get_nodes("key", 3).is_empty());
    }

    #[test]
    fn test_vote_counts() {
        let mut decision = QuorumDecision::new(5);

        decision.vote(NodeId::new_v4(), true);
        decision.vote(NodeId::new_v4(), true);
        decision.vote(NodeId::new_v4(), false);

        let (yes, no) = decision.vote_counts();
        assert_eq!(yes, 2);
        assert_eq!(no, 1);
    }

    #[test]
    fn test_decision_reset() {
        let mut decision = QuorumDecision::new(5);

        decision.vote(NodeId::new_v4(), true);
        decision.vote(NodeId::new_v4(), true);

        decision.reset();

        assert!(!decision.has_quorum());
        let (yes, no) = decision.vote_counts();
        assert_eq!(yes, 0);
        assert_eq!(no, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_add_and_remove_known_node() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let detector = PartitionDetector::new(node.clone());

        let new_node = NodeId::new_v4();
        detector.add_known_node(new_node).await;

        assert_eq!(detector.known_count().await, 2);

        detector.remove_known_node(new_node).await;

        assert_eq!(detector.known_count().await, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_get_partition_info() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let detector = PartitionDetector::new(node.clone());

        // Initially no partition
        assert!(detector.get_partition_info().await.is_none());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_clear_history() {
        let node = Arc::new(Node::new(NodeRole::Relay));
        let detector = PartitionDetector::new(node.clone());

        detector.clear_history().await;

        let history = detector.get_history().await;
        assert!(history.is_empty());
    }
}
