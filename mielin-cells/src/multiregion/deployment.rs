//! Regional Deployment Module

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeploymentError {
    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),
    #[error("Region not found: {0}")]
    RegionNotFound(String),
}

pub type DeploymentResult<T> = Result<T, DeploymentError>;

/// Geographic region
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub location: Location,
    pub data_sovereignty_rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    pub latitude: i32,  // Stored as int (multiply by 1e6)
    pub longitude: i32, // Stored as int (multiply by 1e6)
}

/// Region status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionStatus {
    Active,
    Degraded,
    Unavailable,
}

/// Region configuration
#[derive(Debug, Clone)]
pub struct RegionConfig {
    pub region: Region,
    pub capacity: usize,
    pub priority: u8,
}

/// Region deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionDeployment {
    pub region_id: String,
    pub agent_ids: Vec<AgentId>,
    pub status: RegionStatus,
}

/// Region manager
pub struct RegionManager {
    regions: Arc<RwLock<HashMap<String, RegionConfig>>>,
    deployments: Arc<RwLock<HashMap<String, RegionDeployment>>>,
}

impl RegionManager {
    pub fn new() -> Self {
        Self {
            regions: Arc::new(RwLock::new(HashMap::new())),
            deployments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_region(&self, config: RegionConfig) -> DeploymentResult<()> {
        let region_id = config.region.id.clone();

        self.regions
            .write()
            .map_err(|_| DeploymentError::DeploymentFailed("Failed to acquire lock".to_string()))?
            .insert(region_id.clone(), config);

        self.deployments
            .write()
            .map_err(|_| DeploymentError::DeploymentFailed("Failed to acquire lock".to_string()))?
            .insert(
                region_id.clone(),
                RegionDeployment {
                    region_id,
                    agent_ids: Vec::new(),
                    status: RegionStatus::Active,
                },
            );

        Ok(())
    }

    pub fn deploy_to_region(&self, region_id: &str, agent_id: AgentId) -> DeploymentResult<()> {
        let mut deployments = self
            .deployments
            .write()
            .map_err(|_| DeploymentError::DeploymentFailed("Failed to acquire lock".to_string()))?;

        let deployment = deployments
            .get_mut(region_id)
            .ok_or_else(|| DeploymentError::RegionNotFound(region_id.to_string()))?;

        deployment.agent_ids.push(agent_id);
        Ok(())
    }

    pub fn get_region_deployment(&self, region_id: &str) -> DeploymentResult<RegionDeployment> {
        self.deployments
            .read()
            .map_err(|_| DeploymentError::DeploymentFailed("Failed to acquire lock".to_string()))?
            .get(region_id)
            .cloned()
            .ok_or_else(|| DeploymentError::RegionNotFound(region_id.to_string()))
    }

    pub fn list_regions(&self) -> DeploymentResult<Vec<Region>> {
        Ok(self
            .regions
            .read()
            .map_err(|_| DeploymentError::DeploymentFailed("Failed to acquire lock".to_string()))?
            .values()
            .map(|c| c.region.clone())
            .collect())
    }
}

impl Default for RegionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn test_region_manager_creation() {
        let _manager = RegionManager::new();
    }

    #[test]
    fn test_add_region() {
        let manager = RegionManager::new();
        let config = RegionConfig {
            region: Region {
                id: "us-west-1".to_string(),
                name: "US West 1".to_string(),
                location: Location {
                    latitude: 37_774_900,
                    longitude: -122_419_400,
                },
                data_sovereignty_rules: vec!["US".to_string()],
            },
            capacity: 1000,
            priority: 1,
        };

        manager.add_region(config).expect("add region");
        let regions = manager.list_regions().expect("list regions");
        assert_eq!(regions.len(), 1);
    }

    #[test]
    fn test_deploy_to_region() {
        let manager = RegionManager::new();
        let config = RegionConfig {
            region: Region {
                id: "us-west-1".to_string(),
                name: "US West 1".to_string(),
                location: Location {
                    latitude: 37_774_900,
                    longitude: -122_419_400,
                },
                data_sovereignty_rules: vec!["US".to_string()],
            },
            capacity: 1000,
            priority: 1,
        };

        manager.add_region(config).expect("add region");

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        manager
            .deploy_to_region("us-west-1", agent.id())
            .expect("deploy to region");

        let deployment = manager
            .get_region_deployment("us-west-1")
            .expect("get deployment");
        assert_eq!(deployment.agent_ids.len(), 1);
    }
}
