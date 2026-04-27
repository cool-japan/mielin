//! Data Transfer Objects for the HTTP control plane API

use serde::{Deserialize, Serialize};

/// Response for the health endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub node_id: String,
}

/// Response for the mesh status endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStatusResponse {
    pub alive: usize,
    pub suspect: usize,
    pub dead: usize,
    pub dht_peers: usize,
    pub local_agents: usize,
}

/// Peer information returned by the peers/nodes endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub status: String,
    pub last_seen_secs: u64,
}

/// Agent information for the agents endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub node_id: String,
    pub address: String,
}

/// Migration statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    pub total_migrations: u64,
    pub active_migrations: usize,
    pub successful: u64,
    pub failed: u64,
}

/// Request to deploy a new WASM agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    pub wasm_bytes: Vec<u8>,
    pub agent_id: Option<String>,
}

/// Response after deploying an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    pub agent_id: String,
}

/// Request to migrate an agent to another node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRequest {
    pub agent_id: String,
    pub target_node: String,
}

/// Acknowledgement of a migration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationAck {
    pub migration_id: String,
    pub status: String,
}
