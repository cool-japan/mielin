//! Core migration types

use crate::{Agent, CellError, Policy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSnapshot {
    pub agent_id: [u8; 16],
    pub wasm_binary: Vec<u8>,
    pub wasm_state: Vec<u8>,
    pub policy: Policy,
    pub timestamp: u64,
    pub source_node: Option<[u8; 16]>,
}

impl MigrationSnapshot {
    pub fn capture(agent: &Agent, source_node: Option<[u8; 16]>) -> Result<Self, CellError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("Time error".to_string()))?
            .as_secs();
        Ok(Self {
            agent_id: *agent.id().as_bytes(),
            wasm_binary: agent.dna().binary().to_vec(),
            wasm_state: vec![],
            policy: agent.policy().clone(),
            timestamp,
            source_node,
        })
    }

    pub fn restore(&self) -> Result<Agent, CellError> {
        let mut agent = Agent::new(self.wasm_binary.clone());
        agent.set_policy(self.policy.clone());
        Ok(agent)
    }

    pub fn serialize(&self) -> Result<Vec<u8>, CellError> {
        oxicode::encode_to_vec(&oxicode::serde::Compat(self))
            .map_err(|e| CellError::InvalidState(format!("Serialization failed: {}", e)))
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, CellError> {
        let (compat, _): (oxicode::serde::Compat<Self>, _) = oxicode::decode_from_slice(data)
            .map_err(|e| CellError::InvalidState(format!("Deserialization failed: {}", e)))?;
        Ok(compat.0)
    }

    pub fn size_bytes(&self) -> usize {
        self.wasm_binary.len() + self.wasm_state.len() + 64
    }

    pub fn age_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.timestamp)
    }
}

pub struct MigrationManager {
    pending_migrations: Vec<MigrationSnapshot>,
}

impl MigrationManager {
    pub fn new() -> Self {
        Self {
            pending_migrations: Vec::new(),
        }
    }

    pub fn initiate_migration(
        &mut self,
        agent: &Agent,
        target_node: Option<[u8; 16]>,
    ) -> Result<MigrationSnapshot, CellError> {
        let snapshot = MigrationSnapshot::capture(agent, target_node)?;
        self.pending_migrations.push(snapshot.clone());
        Ok(snapshot)
    }

    pub fn complete_migration(&mut self, agent_id: &[u8; 16]) {
        self.pending_migrations.retain(|s| &s.agent_id != agent_id);
    }

    pub fn pending_count(&self) -> usize {
        self.pending_migrations.len()
    }

    pub fn get_pending(&self, agent_id: &[u8; 16]) -> Option<&MigrationSnapshot> {
        self.pending_migrations
            .iter()
            .find(|s| &s.agent_id == agent_id)
    }
}
