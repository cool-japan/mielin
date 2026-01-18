//! # OperationType - Trait Implementations
//!
//! This module contains trait implementations for `OperationType`.
//!
//! ## Implemented Traits
//!
//! - `Display`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::OperationType;

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DhtGet => write!(f, "dht_get"),
            Self::DhtPut => write!(f, "dht_put"),
            Self::DhtLookup => write!(f, "dht_lookup"),
            Self::GossipRound => write!(f, "gossip_round"),
            Self::StateSync => write!(f, "state_sync"),
            Self::AgentLookup => write!(f, "agent_lookup"),
            Self::AgentRegister => write!(f, "agent_register"),
            Self::MigrationPrepare => write!(f, "migration_prepare"),
            Self::MigrationTransfer => write!(f, "migration_transfer"),
            Self::MigrationComplete => write!(f, "migration_complete"),
            Self::PeerConnect => write!(f, "peer_connect"),
            Self::MessageSend => write!(f, "message_send"),
            Self::MessageProcess => write!(f, "message_process"),
        }
    }
}
