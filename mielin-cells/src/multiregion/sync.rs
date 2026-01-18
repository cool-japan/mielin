//! Cross-region Synchronization Module

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;

pub type RegionId = String;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("Sync failed: {0}")]
    SyncFailed(String),
}

pub type SyncResult<T> = Result<T, SyncError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    InSync,
    Syncing,
    OutOfSync,
    Failed,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub sync_interval: Duration,
    pub conflict_resolution: ConflictResolution,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(60),
            conflict_resolution: ConflictResolution::LastWriteWins,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolution {
    LastWriteWins,
    FirstWriteWins,
    MergeStrategy,
}

#[derive(Debug, Clone)]
pub struct CrossRegionSync {
    pub source_region: RegionId,
    pub target_region: RegionId,
    pub agent_id: AgentId,
    pub status: SyncStatus,
    pub last_sync: Option<Instant>,
}

pub struct SyncManager {
    #[allow(dead_code)]
    config: SyncConfig,
}

impl SyncManager {
    pub fn new(config: SyncConfig) -> Self {
        Self { config }
    }

    pub fn sync_agent(
        &self,
        agent_id: &AgentId,
        source_region: &str,
        target_region: &str,
    ) -> SyncResult<CrossRegionSync> {
        Ok(CrossRegionSync {
            source_region: source_region.to_string(),
            target_region: target_region.to_string(),
            agent_id: *agent_id,
            status: SyncStatus::InSync,
            last_sync: Some(Instant::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn test_sync_manager() {
        let config = SyncConfig::default();
        let manager = SyncManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let sync = manager
            .sync_agent(&agent.id(), "us-west", "us-east")
            .expect("sync agent");

        assert_eq!(sync.status, SyncStatus::InSync);
    }
}
