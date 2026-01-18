//! Quorum Module
//!
//! Implements quorum-based decision making for distributed consensus.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

pub type NodeId = String;

#[derive(Debug, Error)]
pub enum QuorumError {
    #[error("Quorum not reached: {current}/{required}")]
    QuorumNotReached { current: usize, required: usize },
    #[error("Vote timeout")]
    Timeout,
    #[error("Invalid vote: {0}")]
    InvalidVote(String),
    #[error("Node already voted")]
    AlreadyVoted,
    #[error("Decision already made")]
    AlreadyDecided,
}

pub type QuorumResult<T> = Result<T, QuorumError>;

/// Quorum policy defines the voting requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuorumPolicy {
    /// Simple majority (>50%)
    SimpleMajority,
    /// Supermajority (>2/3)
    SuperMajority,
    /// Unanimous (100%)
    Unanimous,
    /// Custom threshold (0.0 to 1.0)
    Custom { threshold: f64 },
    /// Weighted votes with minimum threshold
    Weighted { threshold: f64 },
}

impl QuorumPolicy {
    /// Calculate required votes
    pub fn required_votes(&self, total_nodes: usize) -> usize {
        match self {
            QuorumPolicy::SimpleMajority => (total_nodes / 2) + 1,
            QuorumPolicy::SuperMajority => ((total_nodes * 2) / 3) + 1,
            QuorumPolicy::Unanimous => total_nodes,
            QuorumPolicy::Custom { threshold } => {
                ((*threshold * total_nodes as f64).ceil() as usize).max(1)
            }
            QuorumPolicy::Weighted { threshold } => {
                ((*threshold * total_nodes as f64).ceil() as usize).max(1)
            }
        }
    }
}

/// Quorum configuration
#[derive(Debug, Clone)]
pub struct QuorumConfig {
    /// Voting policy
    pub policy: QuorumPolicy,
    /// Vote timeout
    pub vote_timeout: Duration,
    /// Minimum participants
    pub min_participants: usize,
}

impl Default for QuorumConfig {
    fn default() -> Self {
        Self {
            policy: QuorumPolicy::SimpleMajority,
            vote_timeout: Duration::from_secs(30),
            min_participants: 2,
        }
    }
}

/// Vote on a proposal
#[derive(Debug, Clone)]
pub struct QuorumVote {
    /// Node that cast the vote
    pub node_id: NodeId,
    /// Vote value (true = yes, false = no)
    pub vote: bool,
    /// Vote weight (for weighted voting)
    pub weight: f64,
    /// Timestamp
    pub timestamp: Instant,
}

/// Decision result
#[derive(Debug, Clone)]
pub struct QuorumDecision {
    /// Whether decision passed
    pub passed: bool,
    /// Total votes
    pub total_votes: usize,
    /// Yes votes
    pub yes_votes: usize,
    /// No votes
    pub no_votes: usize,
    /// Required votes
    pub required_votes: usize,
    /// Timestamp of decision
    pub decided_at: Instant,
}

/// Quorum voter manages voting process
pub struct QuorumVoter {
    /// Configuration
    config: QuorumConfig,
    /// Total nodes in cluster
    total_nodes: usize,
    /// Votes received
    votes: Arc<RwLock<HashMap<NodeId, QuorumVote>>>,
    /// Decision result (if made)
    decision: Arc<RwLock<Option<QuorumDecision>>>,
    /// Start time
    start_time: Instant,
}

impl QuorumVoter {
    /// Create a new quorum voter
    pub fn new(config: QuorumConfig, total_nodes: usize) -> Self {
        Self {
            config,
            total_nodes,
            votes: Arc::new(RwLock::new(HashMap::new())),
            decision: Arc::new(RwLock::new(None)),
            start_time: Instant::now(),
        }
    }

    /// Cast a vote
    pub fn cast_vote(&self, vote: QuorumVote) -> QuorumResult<()> {
        // Check if already decided
        if self
            .decision
            .read()
            .map_err(|_| QuorumError::InvalidVote("Failed to acquire lock".to_string()))?
            .is_some()
        {
            return Err(QuorumError::AlreadyDecided);
        }

        // Check if node already voted
        let mut votes = self
            .votes
            .write()
            .map_err(|_| QuorumError::InvalidVote("Failed to acquire lock".to_string()))?;

        if votes.contains_key(&vote.node_id) {
            return Err(QuorumError::AlreadyVoted);
        }

        votes.insert(vote.node_id.clone(), vote);
        drop(votes); // Release write lock before calling check_quorum

        // Check if quorum reached
        self.check_quorum()?;

        Ok(())
    }

    /// Check if quorum has been reached
    fn check_quorum(&self) -> QuorumResult<()> {
        let votes = self
            .votes
            .read()
            .map_err(|_| QuorumError::InvalidVote("Failed to acquire lock".to_string()))?;

        let required_votes = self.config.policy.required_votes(self.total_nodes);
        let total_votes = votes.len();

        if total_votes < self.config.min_participants {
            return Ok(());
        }

        let (yes_votes, no_votes) = match &self.config.policy {
            QuorumPolicy::Weighted { .. } => {
                let yes_weight: f64 = votes.values().filter(|v| v.vote).map(|v| v.weight).sum();
                let no_weight: f64 = votes.values().filter(|v| !v.vote).map(|v| v.weight).sum();
                (yes_weight as usize, no_weight as usize)
            }
            _ => {
                let yes = votes.values().filter(|v| v.vote).count();
                let no = votes.values().filter(|v| !v.vote).count();
                (yes, no)
            }
        };

        // Check if quorum reached
        let passed = yes_votes >= required_votes;
        let failed = no_votes > (self.total_nodes - required_votes);

        if passed || failed || total_votes == self.total_nodes {
            let decision = QuorumDecision {
                passed,
                total_votes,
                yes_votes,
                no_votes,
                required_votes,
                decided_at: Instant::now(),
            };

            *self
                .decision
                .write()
                .map_err(|_| QuorumError::InvalidVote("Failed to acquire lock".to_string()))? =
                Some(decision);
        }

        Ok(())
    }

    /// Get current decision (if made)
    pub fn get_decision(&self) -> QuorumResult<Option<QuorumDecision>> {
        Ok(self
            .decision
            .read()
            .map_err(|_| QuorumError::InvalidVote("Failed to acquire lock".to_string()))?
            .clone())
    }

    /// Wait for decision
    pub fn wait_for_decision(&self) -> QuorumResult<QuorumDecision> {
        let deadline = self.start_time + self.config.vote_timeout;

        loop {
            if Instant::now() > deadline {
                return Err(QuorumError::Timeout);
            }

            if let Some(decision) = self.get_decision()? {
                return Ok(decision);
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Check if voting has timed out
    pub fn is_timeout(&self) -> bool {
        self.start_time.elapsed() > self.config.vote_timeout
    }

    /// Get current vote count
    pub fn vote_count(&self) -> QuorumResult<usize> {
        Ok(self
            .votes
            .read()
            .map_err(|_| QuorumError::InvalidVote("Failed to acquire lock".to_string()))?
            .len())
    }

    /// Get required votes for quorum
    pub fn required_votes(&self) -> usize {
        self.config.policy.required_votes(self.total_nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_majority() {
        let policy = QuorumPolicy::SimpleMajority;
        assert_eq!(policy.required_votes(5), 3);
        assert_eq!(policy.required_votes(4), 3);
        assert_eq!(policy.required_votes(3), 2);
    }

    #[test]
    fn test_super_majority() {
        let policy = QuorumPolicy::SuperMajority;
        assert_eq!(policy.required_votes(9), 7);
        assert_eq!(policy.required_votes(6), 5);
        assert_eq!(policy.required_votes(3), 3);
    }

    #[test]
    fn test_unanimous() {
        let policy = QuorumPolicy::Unanimous;
        assert_eq!(policy.required_votes(5), 5);
        assert_eq!(policy.required_votes(3), 3);
    }

    #[test]
    fn test_custom_threshold() {
        let policy = QuorumPolicy::Custom { threshold: 0.75 };
        assert_eq!(policy.required_votes(4), 3);
        assert_eq!(policy.required_votes(5), 4);
    }

    #[test]
    fn test_quorum_voter_creation() {
        let config = QuorumConfig::default();
        let _voter = QuorumVoter::new(config, 5);
    }

    #[test]
    fn test_cast_vote() {
        let config = QuorumConfig::default();
        let voter = QuorumVoter::new(config, 5);

        let vote = QuorumVote {
            node_id: "node1".to_string(),
            vote: true,
            weight: 1.0,
            timestamp: Instant::now(),
        };

        voter.cast_vote(vote).expect("cast vote");
        assert_eq!(voter.vote_count().expect("vote count"), 1);
    }

    #[test]
    fn test_simple_majority_decision() {
        let config = QuorumConfig::default();
        let voter = QuorumVoter::new(config, 5);

        // Cast 3 yes votes (majority)
        for i in 0..3 {
            let vote = QuorumVote {
                node_id: format!("node{}", i),
                vote: true,
                weight: 1.0,
                timestamp: Instant::now(),
            };
            voter.cast_vote(vote).expect("cast vote");
        }

        let decision = voter.get_decision().expect("get decision");
        assert!(decision.is_some());
        assert!(decision.expect("decision").passed);
    }

    #[test]
    fn test_rejection() {
        let config = QuorumConfig::default();
        let voter = QuorumVoter::new(config, 5);

        // Cast 3 no votes
        for i in 0..3 {
            let vote = QuorumVote {
                node_id: format!("node{}", i),
                vote: false,
                weight: 1.0,
                timestamp: Instant::now(),
            };
            voter.cast_vote(vote).expect("cast vote");
        }

        let decision = voter.get_decision().expect("get decision");
        assert!(decision.is_some());
        assert!(!decision.expect("decision").passed);
    }

    #[test]
    fn test_duplicate_vote() {
        let config = QuorumConfig::default();
        let voter = QuorumVoter::new(config, 5);

        let vote = QuorumVote {
            node_id: "node1".to_string(),
            vote: true,
            weight: 1.0,
            timestamp: Instant::now(),
        };

        voter.cast_vote(vote.clone()).expect("cast vote");
        assert!(voter.cast_vote(vote).is_err());
    }

    #[test]
    fn test_weighted_voting() {
        let config = QuorumConfig {
            policy: QuorumPolicy::Weighted { threshold: 0.6 },
            ..Default::default()
        };
        let voter = QuorumVoter::new(config, 3);

        // Node 1 with high weight votes yes
        let vote1 = QuorumVote {
            node_id: "node1".to_string(),
            vote: true,
            weight: 2.0,
            timestamp: Instant::now(),
        };
        voter.cast_vote(vote1).expect("cast vote");

        // Node 2 with low weight votes no
        let vote2 = QuorumVote {
            node_id: "node2".to_string(),
            vote: false,
            weight: 1.0,
            timestamp: Instant::now(),
        };
        voter.cast_vote(vote2).expect("cast vote");

        let decision = voter.get_decision().expect("get decision");
        assert!(decision.is_some());
    }
}
