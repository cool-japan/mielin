//! # MessageType - Trait Implementations
//!
//! This module contains trait implementations for `MessageType`.
//!
//! ## Implemented Traits
//!
//! - `Display`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::MessageType;

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Heartbeat => write!(f, "heartbeat"),
            Self::MembershipUpdate => write!(f, "membership_update"),
            Self::StateSync => write!(f, "state_sync"),
            Self::DhtLookup => write!(f, "dht_lookup"),
            Self::DhtLookupResponse => write!(f, "dht_lookup_response"),
            Self::DhtStore => write!(f, "dht_store"),
            Self::MigrationData => write!(f, "migration_data"),
            Self::AgentMessage => write!(f, "agent_message"),
            Self::Control => write!(f, "control"),
            Self::Other => write!(f, "other"),
        }
    }
}
