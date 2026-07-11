//! Core migration types

use crate::{Agent, CellError, Policy};
use serde::{Deserialize, Serialize};

/// A migration snapshot of an [`Agent`].
///
/// # Scope: what this snapshot does *not* capture
///
/// `mielin-cells` does not embed a WebAssembly execution engine, so an
/// [`Agent`] has no reachable linear-memory handle or execution context to
/// capture from — it only tracks the DNA (WASM binary), its lifecycle
/// [`AgentState`](crate::AgentState), and its [`Policy`]. As a direct
/// consequence, [`MigrationSnapshot::capture`] transfers the agent's
/// **binary and policy only**. Any in-flight WASM linear memory, globals, or
/// execution stack that a *running* instance of that binary may hold in a
/// host wasm runtime is **not captured and cannot be reconstructed** from
/// this snapshot. `wasm_state` therefore always encodes as empty today; it
/// exists as a wire-compatible slot for a future runtime integration that
/// can actually reach a memory handle, and [`MigrationSnapshot::restore`]
/// deliberately refuses to silently discard it if it is ever non-empty (see
/// below) rather than pretending a full state transfer happened.
///
/// Callers relying on this type for "full" agent migration must be aware
/// that only DNA + policy round-trip; do not label a `MigrationSnapshot` as
/// a complete runtime-state transfer in telemetry, logs, or documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSnapshot {
    pub agent_id: [u8; 16],
    pub wasm_binary: Vec<u8>,
    /// WASM linear-memory / runtime execution state.
    ///
    /// **Always empty in the current implementation.** `mielin-cells` has no
    /// embedded WASM runtime, so [`capture`](MigrationSnapshot::capture) has
    /// no memory handle to read from. This is not a size optimization or a
    /// "not yet wired up" default — it is a hard capability limit of this
    /// crate. Do not interpret a captured snapshot as including runtime
    /// memory.
    pub wasm_state: Vec<u8>,
    pub policy: Policy,
    pub timestamp: u64,
    pub source_node: Option<[u8; 16]>,
}

impl MigrationSnapshot {
    /// Capture a migration snapshot of `agent`.
    ///
    /// Only the agent's DNA (WASM binary) and [`Policy`] are captured.
    /// `wasm_state` is left empty: `mielin-cells` has no embedded WASM
    /// runtime, so there is no linear-memory handle reachable here to
    /// capture. See the [`MigrationSnapshot`] docs for the full scope
    /// caveat.
    pub fn capture(agent: &Agent, source_node: Option<[u8; 16]>) -> Result<Self, CellError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("Time error".to_string()))?
            .as_secs();
        Ok(Self {
            agent_id: *agent.id().as_bytes(),
            wasm_binary: agent.dna().binary().to_vec(),
            // INTENTIONALLY EMPTY: no WASM runtime is embedded in this
            // crate, so there is no linear-memory handle to capture. Do not
            // populate this with a placeholder or fabricated value.
            wasm_state: vec![],
            policy: agent.policy().clone(),
            timestamp,
            source_node,
        })
    }

    /// Restore an [`Agent`] from this snapshot.
    ///
    /// Only DNA and policy are restored (see the [`MigrationSnapshot`] docs).
    /// If `wasm_state` is non-empty this returns an error rather than
    /// silently discarding it: this crate has no WASM runtime to load
    /// captured memory into, so applying a non-empty `wasm_state` would
    /// require dropping data the caller may be relying on, which would be
    /// a silent, honesty-breaking data loss.
    pub fn restore(&self) -> Result<Agent, CellError> {
        if !self.wasm_state.is_empty() {
            return Err(CellError::MigrationFailed(format!(
                "snapshot carries {} byte(s) of wasm_state, but mielin-cells \
                 has no embedded WASM runtime to restore linear memory into; \
                 refusing to silently drop captured runtime state",
                self.wasm_state.len()
            )));
        }
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
