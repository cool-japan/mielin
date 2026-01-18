//! Audit Logging Module

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("Audit failed: {0}")]
    AuditFailed(String),
    #[error("Entry not found: {0}")]
    EntryNotFound(String),
}

pub type AuditResult<T> = Result<T, AuditError>;

/// Event type for audit logging
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    AgentCreated,
    AgentStarted,
    AgentStopped,
    AgentMigrated,
    AgentDeleted,
    StateChanged,
    PolicyViolation,
    AccessGranted,
    AccessDenied,
    ConfigChanged,
    DataAccessed,
    DataModified,
    DataDeleted,
}

/// Audit entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: SystemTime,
    pub event_type: EventType,
    pub agent_id: Option<AgentId>,
    pub user_id: Option<String>,
    pub resource: String,
    pub action: String,
    pub result: String,
    pub metadata: HashMap<String, String>,
    pub checksum: u64,
}

impl AuditEntry {
    pub fn new(
        event_type: EventType,
        agent_id: Option<AgentId>,
        user_id: Option<String>,
        resource: String,
        action: String,
        result: String,
    ) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = SystemTime::now();
        let metadata = HashMap::new();

        // Calculate checksum for tamper detection
        let checksum = Self::calculate_checksum(&id, &timestamp, &event_type, &resource, &action);

        Self {
            id,
            timestamp,
            event_type,
            agent_id,
            user_id,
            resource,
            action,
            result,
            metadata,
            checksum,
        }
    }

    fn calculate_checksum(
        id: &str,
        timestamp: &SystemTime,
        event_type: &EventType,
        resource: &str,
        action: &str,
    ) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        format!("{:?}", timestamp).hash(&mut hasher);
        format!("{:?}", event_type).hash(&mut hasher);
        resource.hash(&mut hasher);
        action.hash(&mut hasher);
        hasher.finish()
    }

    pub fn verify(&self) -> bool {
        let checksum = Self::calculate_checksum(
            &self.id,
            &self.timestamp,
            &self.event_type,
            &self.resource,
            &self.action,
        );
        checksum == self.checksum
    }
}

/// Audit query
#[derive(Debug, Clone)]
pub struct AuditQuery {
    pub event_type: Option<EventType>,
    pub agent_id: Option<AgentId>,
    pub user_id: Option<String>,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
}

/// Audit log
pub struct AuditLog {
    entries: Arc<RwLock<Vec<AuditEntry>>>,
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            max_entries,
        }
    }

    pub fn add_entry(&self, entry: AuditEntry) -> AuditResult<()> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| AuditError::AuditFailed("Failed to acquire lock".to_string()))?;

        entries.push(entry);

        // Rotate if needed
        if entries.len() > self.max_entries {
            entries.remove(0);
        }

        Ok(())
    }

    pub fn query(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEntry>> {
        let entries = self
            .entries
            .read()
            .map_err(|_| AuditError::AuditFailed("Failed to acquire lock".to_string()))?;

        let filtered: Vec<AuditEntry> = entries
            .iter()
            .filter(|e| {
                if let Some(ref event_type) = query.event_type {
                    if &e.event_type != event_type {
                        return false;
                    }
                }
                if let Some(ref agent_id) = query.agent_id {
                    if e.agent_id.as_ref() != Some(agent_id) {
                        return false;
                    }
                }
                if let Some(ref user_id) = query.user_id {
                    if e.user_id.as_ref() != Some(user_id) {
                        return false;
                    }
                }
                if let Some(start_time) = query.start_time {
                    if e.timestamp < start_time {
                        return false;
                    }
                }
                if let Some(end_time) = query.end_time {
                    if e.timestamp > end_time {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        Ok(filtered)
    }

    pub fn verify_integrity(&self) -> AuditResult<bool> {
        let entries = self
            .entries
            .read()
            .map_err(|_| AuditError::AuditFailed("Failed to acquire lock".to_string()))?;

        Ok(entries.iter().all(|e| e.verify()))
    }

    pub fn count(&self) -> AuditResult<usize> {
        Ok(self
            .entries
            .read()
            .map_err(|_| AuditError::AuditFailed("Failed to acquire lock".to_string()))?
            .len())
    }
}

/// Audit logger
pub struct AuditLogger {
    log: Arc<AuditLog>,
}

impl AuditLogger {
    pub fn new(max_entries: usize) -> Self {
        Self {
            log: Arc::new(AuditLog::new(max_entries)),
        }
    }

    pub fn log_event(
        &self,
        event_type: EventType,
        agent_id: Option<AgentId>,
        user_id: Option<String>,
        resource: String,
        action: String,
        result: String,
    ) -> AuditResult<()> {
        let entry = AuditEntry::new(event_type, agent_id, user_id, resource, action, result);
        self.log.add_entry(entry)
    }

    pub fn query(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEntry>> {
        self.log.query(query)
    }

    pub fn verify_integrity(&self) -> AuditResult<bool> {
        self.log.verify_integrity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn test_audit_entry_creation() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let entry = AuditEntry::new(
            EventType::AgentCreated,
            Some(agent.id()),
            Some("user1".to_string()),
            "agent_service".to_string(),
            "create".to_string(),
            "success".to_string(),
        );

        assert!(entry.verify());
    }

    #[test]
    fn test_audit_log() {
        let log = AuditLog::new(100);
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        let entry = AuditEntry::new(
            EventType::AgentCreated,
            Some(agent.id()),
            None,
            "agent_service".to_string(),
            "create".to_string(),
            "success".to_string(),
        );

        log.add_entry(entry).expect("add entry");
        assert_eq!(log.count().expect("count"), 1);
    }

    #[test]
    fn test_audit_query() {
        let log = AuditLog::new(100);
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        let entry = AuditEntry::new(
            EventType::AgentCreated,
            Some(agent.id()),
            None,
            "agent_service".to_string(),
            "create".to_string(),
            "success".to_string(),
        );

        log.add_entry(entry).expect("add entry");

        let query = AuditQuery {
            event_type: Some(EventType::AgentCreated),
            agent_id: None,
            user_id: None,
            start_time: None,
            end_time: None,
        };

        let results = log.query(&query).expect("query");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_integrity_verification() {
        let log = AuditLog::new(100);
        let entry = AuditEntry::new(
            EventType::AgentCreated,
            None,
            None,
            "agent_service".to_string(),
            "create".to_string(),
            "success".to_string(),
        );

        log.add_entry(entry).expect("add entry");
        assert!(log.verify_integrity().expect("verify"));
    }

    #[test]
    fn test_audit_logger() {
        let logger = AuditLogger::new(100);
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        logger
            .log_event(
                EventType::AgentCreated,
                Some(agent.id()),
                None,
                "agent_service".to_string(),
                "create".to_string(),
                "success".to_string(),
            )
            .expect("log event");

        assert!(logger.verify_integrity().expect("verify"));
    }
}
