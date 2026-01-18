//! Backup Module
//!
//! Implements backup management for disaster recovery.

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Backup failed: {0}")]
    BackupFailed(String),
    #[error("Backup not found: {0}")]
    BackupNotFound(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Compression error: {0}")]
    CompressionError(String),
    #[error("Encryption error: {0}")]
    EncryptionError(String),
}

pub type BackupResult<T> = Result<T, BackupError>;

/// Backup type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupType {
    /// Full backup
    Full,
    /// Incremental backup (changes since last backup)
    Incremental,
    /// Differential backup (changes since last full backup)
    Differential,
    /// Snapshot (point-in-time copy)
    Snapshot,
}

/// Backup strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupStrategy {
    /// Full backups only
    FullOnly,
    /// Full + Incremental
    FullAndIncremental { incremental_frequency: Duration },
    /// Full + Differential
    FullAndDifferential { differential_frequency: Duration },
    /// Custom strategy
    Custom,
}

/// Backup storage location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupStorage {
    /// Local filesystem
    Local { path: String },
    /// Remote storage
    Remote { endpoint: String },
    /// Object storage (S3-compatible)
    ObjectStorage { bucket: String, endpoint: String },
    /// Custom storage backend
    Custom { backend: String },
}

/// Backup configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    /// Backup strategy
    pub strategy: BackupStrategy,
    /// Storage location
    pub storage: BackupStorage,
    /// Enable compression
    pub enable_compression: bool,
    /// Enable encryption
    pub enable_encryption: bool,
    /// Encryption key (if encryption enabled)
    pub encryption_key: Option<Vec<u8>>,
    /// Backup timeout
    pub timeout: Duration,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            strategy: BackupStrategy::FullAndIncremental {
                incremental_frequency: Duration::from_secs(6 * 3600),
            },
            storage: BackupStorage::Local {
                path: "/var/backups/mielin".to_string(),
            },
            enable_compression: true,
            enable_encryption: false,
            encryption_key: None,
            timeout: Duration::from_secs(300),
        }
    }
}

/// Backup metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// Backup ID
    pub id: String,
    /// Agent ID
    pub agent_id: AgentId,
    /// Backup type
    pub backup_type: BackupType,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Size in bytes
    pub size: usize,
    /// Compressed size (if compressed)
    pub compressed_size: Option<usize>,
    /// Checksum
    pub checksum: u64,
    /// Parent backup ID (for incremental/differential)
    pub parent_id: Option<String>,
    /// Storage location
    pub storage_location: String,
    /// Whether encrypted
    pub encrypted: bool,
}

/// Backup data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    /// Metadata
    pub metadata: BackupMetadata,
    /// Backup data
    pub data: Vec<u8>,
}

impl Backup {
    /// Create a new backup
    pub fn new(
        agent_id: AgentId,
        backup_type: BackupType,
        data: Vec<u8>,
        parent_id: Option<String>,
    ) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        let checksum = hasher.finish();

        Self {
            metadata: BackupMetadata {
                id: uuid::Uuid::new_v4().to_string(),
                agent_id,
                backup_type,
                timestamp: SystemTime::now(),
                size: data.len(),
                compressed_size: None,
                checksum,
                parent_id,
                storage_location: String::new(),
                encrypted: false,
            },
            data,
        }
    }

    /// Verify backup integrity
    pub fn verify(&self) -> BackupResult<bool> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.data.hash(&mut hasher);
        let checksum = hasher.finish();

        Ok(checksum == self.metadata.checksum)
    }
}

/// Backup manager
pub struct BackupManager {
    /// Configuration
    config: BackupConfig,
    /// Backup catalog
    catalog: Arc<RwLock<HashMap<String, BackupMetadata>>>,
    /// Agent backup history
    agent_backups: Arc<RwLock<HashMap<AgentId, Vec<String>>>>,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(config: BackupConfig) -> Self {
        Self {
            config,
            catalog: Arc::new(RwLock::new(HashMap::new())),
            agent_backups: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a backup
    pub fn create_backup(&self, agent_id: &AgentId, state_data: Vec<u8>) -> BackupResult<Backup> {
        let _start_time = Instant::now();

        // Determine backup type based on strategy
        let backup_type = self.determine_backup_type(agent_id)?;

        // Get parent backup if incremental/differential
        let parent_id = if backup_type != BackupType::Full {
            self.get_latest_backup_id(agent_id)?
        } else {
            None
        };

        // Compress if enabled
        let data = if self.config.enable_compression {
            self.compress_data(&state_data)?
        } else {
            state_data
        };

        // Encrypt if enabled
        let data = if self.config.enable_encryption {
            self.encrypt_data(&data)?
        } else {
            data
        };

        // Create backup
        let mut backup = Backup::new(*agent_id, backup_type, data, parent_id);
        backup.metadata.encrypted = self.config.enable_encryption;

        // Store backup
        let storage_location = self.store_backup(&backup)?;
        backup.metadata.storage_location = storage_location;

        // Update catalog
        self.catalog
            .write()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?
            .insert(backup.metadata.id.clone(), backup.metadata.clone());

        // Update agent backup history
        self.agent_backups
            .write()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?
            .entry(*agent_id)
            .or_default()
            .push(backup.metadata.id.clone());

        Ok(backup)
    }

    /// Determine backup type based on strategy
    fn determine_backup_type(&self, agent_id: &AgentId) -> BackupResult<BackupType> {
        let last_backup = self.get_latest_backup(agent_id)?;

        match &self.config.strategy {
            BackupStrategy::FullOnly => Ok(BackupType::Full),
            BackupStrategy::FullAndIncremental {
                incremental_frequency,
            } => {
                if let Some(last) = last_backup {
                    if last.backup_type == BackupType::Full {
                        let elapsed = SystemTime::now()
                            .duration_since(last.timestamp)
                            .unwrap_or_default();
                        if elapsed >= *incremental_frequency {
                            Ok(BackupType::Incremental)
                        } else {
                            Ok(BackupType::Full)
                        }
                    } else {
                        Ok(BackupType::Incremental)
                    }
                } else {
                    Ok(BackupType::Full)
                }
            }
            BackupStrategy::FullAndDifferential {
                differential_frequency,
            } => {
                if let Some(last) = last_backup {
                    if last.backup_type == BackupType::Full {
                        let elapsed = SystemTime::now()
                            .duration_since(last.timestamp)
                            .unwrap_or_default();
                        if elapsed >= *differential_frequency {
                            Ok(BackupType::Differential)
                        } else {
                            Ok(BackupType::Full)
                        }
                    } else {
                        Ok(BackupType::Differential)
                    }
                } else {
                    Ok(BackupType::Full)
                }
            }
            BackupStrategy::Custom => Ok(BackupType::Full),
        }
    }

    /// Get latest backup for an agent
    fn get_latest_backup(&self, agent_id: &AgentId) -> BackupResult<Option<BackupMetadata>> {
        let agent_backups = self
            .agent_backups
            .read()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?;

        let backup_ids = agent_backups.get(agent_id);
        if backup_ids.is_none() || backup_ids.expect("ids").is_empty() {
            return Ok(None);
        }

        let catalog = self
            .catalog
            .read()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?;

        Ok(backup_ids
            .expect("ids")
            .iter()
            .filter_map(|id| catalog.get(id))
            .max_by_key(|m| m.timestamp)
            .cloned())
    }

    /// Get latest backup ID
    fn get_latest_backup_id(&self, agent_id: &AgentId) -> BackupResult<Option<String>> {
        Ok(self.get_latest_backup(agent_id)?.map(|m| m.id))
    }

    /// Compress data
    fn compress_data(&self, data: &[u8]) -> BackupResult<Vec<u8>> {
        // Simple RLE compression
        let mut compressed = Vec::new();
        let mut count = 1u8;
        let mut current = data.first().copied();

        for &byte in data.iter().skip(1) {
            if Some(byte) == current && count < 255 {
                count += 1;
            } else {
                if let Some(c) = current {
                    compressed.push(count);
                    compressed.push(c);
                }
                current = Some(byte);
                count = 1;
            }
        }

        if let Some(c) = current {
            compressed.push(count);
            compressed.push(c);
        }

        Ok(compressed)
    }

    /// Encrypt data
    fn encrypt_data(&self, data: &[u8]) -> BackupResult<Vec<u8>> {
        // Simple XOR encryption for demonstration
        let key = self
            .config
            .encryption_key
            .as_ref()
            .ok_or_else(|| BackupError::EncryptionError("No encryption key".to_string()))?;

        let encrypted: Vec<u8> = data
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key[i % key.len()])
            .collect();

        Ok(encrypted)
    }

    /// Store backup
    fn store_backup(&self, backup: &Backup) -> BackupResult<String> {
        // In a real implementation, this would write to actual storage
        // For now, we just return a simulated location
        match &self.config.storage {
            BackupStorage::Local { path } => Ok(format!("{}/{}.bak", path, backup.metadata.id)),
            BackupStorage::Remote { endpoint } => {
                Ok(format!("{}/backups/{}", endpoint, backup.metadata.id))
            }
            BackupStorage::ObjectStorage { bucket, endpoint } => {
                Ok(format!("{}/{}/{}", endpoint, bucket, backup.metadata.id))
            }
            BackupStorage::Custom { backend } => Ok(format!("{}/{}", backend, backup.metadata.id)),
        }
    }

    /// Get backup by ID
    pub fn get_backup(&self, backup_id: &str) -> BackupResult<BackupMetadata> {
        self.catalog
            .read()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?
            .get(backup_id)
            .cloned()
            .ok_or_else(|| BackupError::BackupNotFound(backup_id.to_string()))
    }

    /// List backups for an agent
    pub fn list_backups(&self, agent_id: &AgentId) -> BackupResult<Vec<BackupMetadata>> {
        let agent_backups = self
            .agent_backups
            .read()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?;

        let backup_ids = agent_backups.get(agent_id).cloned().unwrap_or_default();

        let catalog = self
            .catalog
            .read()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?;

        Ok(backup_ids
            .iter()
            .filter_map(|id| catalog.get(id).cloned())
            .collect())
    }

    /// Delete backup
    pub fn delete_backup(&self, backup_id: &str) -> BackupResult<()> {
        let metadata = self.get_backup(backup_id)?;

        // Remove from catalog
        self.catalog
            .write()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?
            .remove(backup_id);

        // Remove from agent backup history
        let mut agent_backups = self
            .agent_backups
            .write()
            .map_err(|_| BackupError::BackupFailed("Failed to acquire lock".to_string()))?;

        if let Some(backups) = agent_backups.get_mut(&metadata.agent_id) {
            backups.retain(|id| id != backup_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;

    #[test]
    fn test_backup_creation() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let data = vec![1, 2, 3, 4, 5];
        let backup = Backup::new(agent.id(), BackupType::Full, data, None);

        assert_eq!(backup.metadata.backup_type, BackupType::Full);
        assert_eq!(backup.data.len(), 5);
    }

    #[test]
    fn test_backup_verification() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let data = vec![1, 2, 3, 4, 5];
        let backup = Backup::new(agent.id(), BackupType::Full, data, None);

        assert!(backup.verify().expect("verify"));
    }

    #[test]
    fn test_backup_manager_creation() {
        let config = BackupConfig::default();
        let _manager = BackupManager::new(config);
    }

    #[test]
    fn test_create_full_backup() {
        let config = BackupConfig {
            strategy: BackupStrategy::FullOnly,
            ..Default::default()
        };
        let manager = BackupManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let data = vec![1, 2, 3, 4, 5];

        let backup = manager
            .create_backup(&agent.id(), data)
            .expect("create backup");

        assert_eq!(backup.metadata.backup_type, BackupType::Full);
    }

    #[test]
    fn test_list_backups() {
        let config = BackupConfig::default();
        let manager = BackupManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        manager
            .create_backup(&agent.id(), vec![1, 2, 3])
            .expect("create backup");
        manager
            .create_backup(&agent.id(), vec![4, 5, 6])
            .expect("create backup");

        let backups = manager.list_backups(&agent.id()).expect("list backups");
        assert_eq!(backups.len(), 2);
    }

    #[test]
    fn test_delete_backup() {
        let config = BackupConfig::default();
        let manager = BackupManager::new(config);

        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

        let backup = manager
            .create_backup(&agent.id(), vec![1, 2, 3])
            .expect("create backup");

        manager
            .delete_backup(&backup.metadata.id)
            .expect("delete backup");

        assert!(manager.get_backup(&backup.metadata.id).is_err());
    }
}
