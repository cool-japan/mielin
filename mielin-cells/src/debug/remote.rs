//! Remote debugging support

use crate::CellError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggerConfig {
    pub listen_address: String,
    pub port: u16,
    pub max_sessions: usize,
    pub session_timeout_secs: u64,
}

impl Default for DebuggerConfig {
    fn default() -> Self {
        Self {
            listen_address: "127.0.0.1".to_string(),
            port: 9229,
            max_sessions: 10,
            session_timeout_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugCommand {
    Pause,
    Resume,
    StepOver,
    StepInto,
    StepOut,
    SetBreakpoint { location: String },
    RemoveBreakpoint { id: u64 },
    Evaluate { expression: String },
    GetStackFrame,
    GetVariables,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    pub session_id: [u8; 16],
    pub agent_id: [u8; 16],
    pub created_at: u64,
    pub last_activity: u64,
    pub paused: bool,
}

#[derive(Debug)]
pub struct RemoteDebugger {
    #[allow(dead_code)]
    config: DebuggerConfig,
    sessions: HashMap<[u8; 16], DebugSession>,
}

impl RemoteDebugger {
    pub fn new(config: DebuggerConfig) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
        }
    }

    pub fn create_session(&mut self, agent_id: [u8; 16]) -> Result<[u8; 16], CellError> {
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut session_id = [0u8; 16];
        rng.fill(&mut session_id);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("Time error".to_string()))?
            .as_secs();

        self.sessions.insert(
            session_id,
            DebugSession {
                session_id,
                agent_id,
                created_at: now,
                last_activity: now,
                paused: false,
            },
        );

        Ok(session_id)
    }

    pub fn close_session(&mut self, session_id: &[u8; 16]) -> bool {
        self.sessions.remove(session_id).is_some()
    }

    pub fn get_session(&self, session_id: &[u8; 16]) -> Option<&DebugSession> {
        self.sessions.get(session_id)
    }

    pub fn handle_command(
        &mut self,
        session_id: &[u8; 16],
        _command: &DebugCommand,
    ) -> Result<String, CellError> {
        if let Some(session) = self.sessions.get_mut(session_id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| CellError::InvalidState("Time error".to_string()))?
                .as_secs();
            session.last_activity = now;
            Ok("Command executed".to_string())
        } else {
            Err(CellError::InvalidState("Session not found".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // This test uses rand::rng() which internally calls ChaCha20's NEON SIMD backend
    // on aarch64. Miri cannot emulate NEON intrinsics (llvm.aarch64.neon.tbl1.v16i8),
    // so we skip it under Miri. This is not a UB issue — the SIMD is in rand's RNG backend.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_remote_debugger() {
        let config = DebuggerConfig::default();
        let mut debugger = RemoteDebugger::new(config);

        let agent_id = [1u8; 16];
        let session_id = debugger.create_session(agent_id).expect("Failed");

        assert!(debugger.get_session(&session_id).is_some());

        let closed = debugger.close_session(&session_id);
        assert!(closed);
    }
}
