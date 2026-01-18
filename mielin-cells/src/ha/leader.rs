//! Leader Election Module
//!
//! Implements consensus-based leader election using a Raft-inspired algorithm.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Node identifier in the cluster
pub type NodeId = String;

/// Election term number
pub type LeaderTerm = u64;

#[derive(Debug, Error)]
pub enum LeaderError {
    #[error("Election timeout")]
    Timeout,
    #[error("Invalid term: expected {expected}, got {actual}")]
    InvalidTerm {
        expected: LeaderTerm,
        actual: LeaderTerm,
    },
    #[error("Split vote occurred")]
    SplitVote,
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Already has leader")]
    AlreadyHasLeader,
}

/// Leader election state for a node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaderState {
    /// Node is a follower
    Follower,
    /// Node is a candidate
    Candidate,
    /// Node is the leader
    Leader,
}

/// Vote request sent during election
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    /// Term for this election
    pub term: LeaderTerm,
    /// Candidate requesting the vote
    pub candidate_id: NodeId,
    /// Timestamp of the request
    pub timestamp: u64,
}

/// Vote response from a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResponse {
    /// Current term of the responding node
    pub term: LeaderTerm,
    /// Whether the vote was granted
    pub vote_granted: bool,
    /// Node that sent the response
    pub node_id: NodeId,
}

/// Leader election result
#[derive(Debug, Clone)]
pub struct LeaderElection {
    /// The elected leader
    pub leader_id: NodeId,
    /// Term of the election
    pub term: LeaderTerm,
    /// Nodes that participated
    pub participants: Vec<NodeId>,
    /// Timestamp of election
    pub elected_at: Instant,
}

/// Configuration for leader election
#[derive(Debug, Clone)]
pub struct ElectionConfig {
    /// Timeout for election
    pub election_timeout: Duration,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Minimum nodes for quorum
    pub min_quorum_size: usize,
}

impl Default for ElectionConfig {
    fn default() -> Self {
        Self {
            election_timeout: Duration::from_millis(150),
            heartbeat_interval: Duration::from_millis(50),
            min_quorum_size: 2,
        }
    }
}

/// Leader elector manages leader election process
pub struct LeaderElector {
    /// This node's ID
    node_id: NodeId,
    /// Current state
    state: Arc<RwLock<LeaderState>>,
    /// Current term
    term: Arc<RwLock<LeaderTerm>>,
    /// Current leader (if known)
    current_leader: Arc<RwLock<Option<NodeId>>>,
    /// Voted for in current term
    voted_for: Arc<RwLock<Option<NodeId>>>,
    /// Last heartbeat time
    last_heartbeat: Arc<RwLock<Instant>>,
    /// Known nodes in cluster
    cluster_nodes: Arc<RwLock<HashSet<NodeId>>>,
    /// Election configuration
    config: ElectionConfig,
    /// Vote history
    vote_history: Arc<RwLock<HashMap<LeaderTerm, VoteRequest>>>,
}

impl LeaderElector {
    /// Create a new leader elector
    pub fn new(node_id: NodeId, config: ElectionConfig) -> Self {
        Self {
            node_id,
            state: Arc::new(RwLock::new(LeaderState::Follower)),
            term: Arc::new(RwLock::new(0)),
            current_leader: Arc::new(RwLock::new(None)),
            voted_for: Arc::new(RwLock::new(None)),
            last_heartbeat: Arc::new(RwLock::new(Instant::now())),
            cluster_nodes: Arc::new(RwLock::new(HashSet::new())),
            config,
            vote_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a node to the cluster
    pub fn add_node(&self, node_id: NodeId) -> Result<(), LeaderError> {
        self.cluster_nodes
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?
            .insert(node_id);
        Ok(())
    }

    /// Remove a node from the cluster
    pub fn remove_node(&self, node_id: &str) -> Result<(), LeaderError> {
        self.cluster_nodes
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?
            .remove(node_id);
        Ok(())
    }

    /// Get current state
    pub fn state(&self) -> Result<LeaderState, LeaderError> {
        self.state
            .read()
            .map(|s| s.clone())
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))
    }

    /// Get current term
    pub fn term(&self) -> Result<LeaderTerm, LeaderError> {
        self.term
            .read()
            .map(|t| *t)
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))
    }

    /// Get current leader
    pub fn current_leader(&self) -> Result<Option<NodeId>, LeaderError> {
        self.current_leader
            .read()
            .map(|l| l.clone())
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))
    }

    /// Check if this node is the leader
    pub fn is_leader(&self) -> Result<bool, LeaderError> {
        Ok(self.state()? == LeaderState::Leader)
    }

    /// Start an election
    pub fn start_election(&self) -> Result<LeaderElection, LeaderError> {
        // Increment term
        let new_term = {
            let mut term = self
                .term
                .write()
                .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?;
            *term += 1;
            *term
        };

        // Become candidate
        *self
            .state
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            LeaderState::Candidate;

        // Vote for self
        *self
            .voted_for
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            Some(self.node_id.clone());

        let request = VoteRequest {
            term: new_term,
            candidate_id: self.node_id.clone(),
            timestamp: Instant::now().elapsed().as_secs(),
        };

        // Record vote request
        self.vote_history
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?
            .insert(new_term, request.clone());

        // In a real implementation, we would send vote requests to all nodes
        // For now, we'll simulate by assuming we get votes from all cluster nodes
        let cluster_nodes = self
            .cluster_nodes
            .read()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?
            .clone();

        let cluster_size = cluster_nodes.len() + 1; // +1 for self

        let votes_needed = (cluster_size / 2) + 1;
        // Simulate receiving votes from all cluster nodes + self
        let votes_received = cluster_size;

        if votes_received >= votes_needed {
            self.become_leader(new_term)?;

            let mut participants = vec![self.node_id.clone()];
            participants.extend(cluster_nodes);

            Ok(LeaderElection {
                leader_id: self.node_id.clone(),
                term: new_term,
                participants,
                elected_at: Instant::now(),
            })
        } else {
            Err(LeaderError::SplitVote)
        }
    }

    /// Handle vote request from another node
    pub fn handle_vote_request(&self, request: VoteRequest) -> Result<VoteResponse, LeaderError> {
        let current_term = self.term()?;
        let voted_for = self
            .voted_for
            .read()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?
            .clone();

        let vote_granted =
            if request.term < current_term {
                // Reject votes from old terms
                false
            } else if request.term > current_term {
                // Update term and grant vote
                *self.term.write().map_err(|_| {
                    LeaderError::NodeNotFound("Failed to acquire lock".to_string())
                })? = request.term;
                *self.voted_for.write().map_err(|_| {
                    LeaderError::NodeNotFound("Failed to acquire lock".to_string())
                })? = Some(request.candidate_id.clone());
                *self.state.write().map_err(|_| {
                    LeaderError::NodeNotFound("Failed to acquire lock".to_string())
                })? = LeaderState::Follower;
                true
            } else {
                // Same term - grant if haven't voted or voted for same candidate
                voted_for.is_none() || voted_for.as_ref() == Some(&request.candidate_id)
            };

        if vote_granted && voted_for.is_none() {
            *self
                .voted_for
                .write()
                .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
                Some(request.candidate_id.clone());
        }

        Ok(VoteResponse {
            term: self.term()?,
            vote_granted,
            node_id: self.node_id.clone(),
        })
    }

    /// Receive heartbeat from leader
    pub fn receive_heartbeat(
        &self,
        leader_id: NodeId,
        term: LeaderTerm,
    ) -> Result<(), LeaderError> {
        let current_term = self.term()?;

        if term < current_term {
            return Err(LeaderError::InvalidTerm {
                expected: current_term,
                actual: term,
            });
        }

        if term > current_term {
            *self
                .term
                .write()
                .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
                term;
            *self
                .voted_for
                .write()
                .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
                None;
        }

        *self
            .state
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            LeaderState::Follower;
        *self
            .current_leader
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            Some(leader_id);
        *self
            .last_heartbeat
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            Instant::now();

        Ok(())
    }

    /// Check if election timeout has occurred
    pub fn check_timeout(&self) -> Result<bool, LeaderError> {
        let last_heartbeat = self
            .last_heartbeat
            .read()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?;
        Ok(last_heartbeat.elapsed() > self.config.election_timeout)
    }

    /// Step down from leader
    pub fn step_down(&self) -> Result<(), LeaderError> {
        *self
            .state
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            LeaderState::Follower;
        *self
            .current_leader
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? = None;
        Ok(())
    }

    /// Become leader after winning election
    fn become_leader(&self, term: LeaderTerm) -> Result<(), LeaderError> {
        *self
            .state
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            LeaderState::Leader;
        *self
            .current_leader
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            Some(self.node_id.clone());
        *self
            .term
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? = term;
        *self
            .last_heartbeat
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            Instant::now();
        Ok(())
    }

    /// Send heartbeat (as leader)
    pub fn send_heartbeat(&self) -> Result<(), LeaderError> {
        if !self.is_leader()? {
            return Err(LeaderError::NodeNotFound(
                "Only leader can send heartbeats".to_string(),
            ));
        }

        *self
            .last_heartbeat
            .write()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))? =
            Instant::now();
        Ok(())
    }

    /// Get cluster size
    pub fn cluster_size(&self) -> Result<usize, LeaderError> {
        Ok(self
            .cluster_nodes
            .read()
            .map_err(|_| LeaderError::NodeNotFound("Failed to acquire lock".to_string()))?
            .len()
            + 1) // +1 for self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leader_elector_creation() {
        let config = ElectionConfig::default();
        let elector = LeaderElector::new("node1".to_string(), config);
        assert_eq!(elector.state().expect("state"), LeaderState::Follower);
        assert_eq!(elector.term().expect("term"), 0);
        assert!(elector.current_leader().expect("leader").is_none());
    }

    #[test]
    fn test_add_remove_nodes() {
        let config = ElectionConfig::default();
        let elector = LeaderElector::new("node1".to_string(), config);

        elector.add_node("node2".to_string()).expect("add node");
        elector.add_node("node3".to_string()).expect("add node");
        assert_eq!(elector.cluster_size().expect("size"), 3);

        elector.remove_node("node2").expect("remove node");
        assert_eq!(elector.cluster_size().expect("size"), 2);
    }

    #[test]
    fn test_start_election() {
        let config = ElectionConfig::default();
        let elector = LeaderElector::new("node1".to_string(), config);

        let result = elector.start_election();
        assert!(result.is_ok());

        let election = result.expect("election");
        assert_eq!(election.leader_id, "node1");
        assert_eq!(election.term, 1);
        assert!(elector.is_leader().expect("is leader"));
    }

    #[test]
    fn test_vote_request_handling() {
        let config = ElectionConfig::default();
        let elector = LeaderElector::new("node1".to_string(), config);

        let request = VoteRequest {
            term: 1,
            candidate_id: "node2".to_string(),
            timestamp: 0,
        };

        let response = elector.handle_vote_request(request).expect("handle vote");
        assert!(response.vote_granted);
        assert_eq!(response.term, 1);
    }

    #[test]
    fn test_heartbeat_reception() {
        let config = ElectionConfig::default();
        let elector = LeaderElector::new("node1".to_string(), config);

        elector
            .receive_heartbeat("node2".to_string(), 1)
            .expect("receive heartbeat");
        assert_eq!(elector.state().expect("state"), LeaderState::Follower);
        assert_eq!(
            elector.current_leader().expect("leader"),
            Some("node2".to_string())
        );
    }

    #[test]
    fn test_step_down() {
        let config = ElectionConfig::default();
        let elector = LeaderElector::new("node1".to_string(), config);

        elector.start_election().expect("start election");
        assert!(elector.is_leader().expect("is leader"));

        elector.step_down().expect("step down");
        assert!(!elector.is_leader().expect("is leader"));
    }

    #[test]
    fn test_heartbeat_timeout() {
        let config = ElectionConfig {
            election_timeout: Duration::from_millis(10),
            ..Default::default()
        };
        let elector = LeaderElector::new("node1".to_string(), config);

        std::thread::sleep(Duration::from_millis(20));
        assert!(elector.check_timeout().expect("timeout"));
    }
}
