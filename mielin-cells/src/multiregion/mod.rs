//! Multi-region Deployment Module
//!
//! Provides multi-region deployment capabilities including:
//! - Geographic distribution of agents
//! - Cross-region replication
//! - Latency-based routing
//! - Regional failover
//! - Data sovereignty compliance

pub mod deployment;
pub mod routing;
pub mod sync;

pub use deployment::{
    DeploymentError, Region, RegionConfig, RegionDeployment, RegionManager, RegionStatus,
};
pub use routing::{GeoRouter, LatencyMap, RouteDecision, RoutingPolicy, RoutingStrategy};
pub use sync::{CrossRegionSync, SyncConfig, SyncError, SyncManager, SyncStatus};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MultiRegionError {
    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),
    #[error("Routing failed: {0}")]
    RoutingFailed(String),
    #[error("Sync failed: {0}")]
    SyncFailed(String),
    #[error("Region not available: {0}")]
    RegionUnavailable(String),
}
