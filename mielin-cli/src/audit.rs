//! Audit logging system for tracking CLI operations
//!
//! This module provides comprehensive audit logging for all CLI operations,
//! including command execution, configuration changes, and security events.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Timestamp of the event
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// User who executed the command
    pub user: String,
    /// Command that was executed
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Exit code (None if still running)
    pub exit_code: Option<i32>,
    /// Duration of command execution
    pub duration_ms: Option<u64>,
    /// Event type
    pub event_type: AuditEventType,
    /// Severity level
    pub severity: AuditSeverity,
    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// Audit event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// Command execution
    CommandExecution,
    /// Configuration change
    ConfigChange,
    /// Authentication event
    Authentication,
    /// Authorization event
    Authorization,
    /// Security event
    Security,
    /// System event
    System,
}

/// Audit severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSeverity {
    /// Informational
    Info,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Critical
    Critical,
}

/// Audit logger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Log file path
    pub log_path: PathBuf,
    /// Maximum log entries before rotation
    pub max_entries: usize,
    /// Number of rotated logs to keep
    pub keep_rotations: usize,
    /// Minimum severity to log
    pub min_severity: AuditSeverity,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_path: get_default_audit_log_path(),
            max_entries: 10000,
            keep_rotations: 5,
            min_severity: AuditSeverity::Info,
        }
    }
}

/// Audit logger
pub struct AuditLogger {
    config: AuditConfig,
}

impl AuditLogger {
    /// Create a new audit logger with the given configuration
    pub fn new(config: AuditConfig) -> Result<Self> {
        // Ensure audit log directory exists
        if let Some(parent) = config.log_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create audit log directory: {}", parent.display())
            })?;
        }

        Ok(Self { config })
    }

    /// Create a new audit logger with default configuration
    pub fn with_default_config() -> Result<Self> {
        Self::new(AuditConfig::default())
    }

    /// Log an audit entry
    pub fn log(&self, entry: AuditEntry) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check severity threshold
        if entry.severity < self.config.min_severity {
            return Ok(());
        }

        // Check if rotation is needed
        self.rotate_if_needed()?;

        // Append entry to log file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.log_path)
            .with_context(|| {
                format!(
                    "Failed to open audit log: {}",
                    self.config.log_path.display()
                )
            })?;

        let json = serde_json::to_string(&entry).context("Failed to serialize audit entry")?;

        writeln!(file, "{}", json).context("Failed to write audit entry")?;

        Ok(())
    }

    /// Log a command execution
    pub fn log_command(
        &self,
        command: &str,
        args: &[String],
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            user: get_current_user(),
            command: command.to_string(),
            args: args.to_vec(),
            exit_code,
            duration_ms,
            event_type: AuditEventType::CommandExecution,
            severity: if exit_code == Some(0) || exit_code.is_none() {
                AuditSeverity::Info
            } else {
                AuditSeverity::Warning
            },
            metadata: serde_json::json!({
                "pid": std::process::id(),
            }),
        };

        self.log(entry)
    }

    /// Log a configuration change
    pub fn log_config_change(
        &self,
        key: &str,
        old_value: Option<&str>,
        new_value: &str,
    ) -> Result<()> {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            user: get_current_user(),
            command: "config".to_string(),
            args: vec![key.to_string(), new_value.to_string()],
            exit_code: Some(0),
            duration_ms: None,
            event_type: AuditEventType::ConfigChange,
            severity: AuditSeverity::Info,
            metadata: serde_json::json!({
                "key": key,
                "old_value": old_value,
                "new_value": new_value,
            }),
        };

        self.log(entry)
    }

    /// Log a security event
    pub fn log_security_event(&self, message: &str, severity: AuditSeverity) -> Result<()> {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            user: get_current_user(),
            command: "security".to_string(),
            args: vec![],
            exit_code: None,
            duration_ms: None,
            event_type: AuditEventType::Security,
            severity,
            metadata: serde_json::json!({
                "message": message,
            }),
        };

        self.log(entry)
    }

    /// Read all audit entries
    pub fn read_entries(&self) -> Result<Vec<AuditEntry>> {
        if !self.config.log_path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&self.config.log_path).with_context(|| {
            format!(
                "Failed to read audit log: {}",
                self.config.log_path.display()
            )
        })?;

        let entries: Vec<AuditEntry> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(entries)
    }

    /// Query audit entries with filters
    pub fn query_entries(
        &self,
        event_type: Option<AuditEventType>,
        severity: Option<AuditSeverity>,
        user: Option<&str>,
        since: Option<chrono::DateTime<chrono::Utc>>,
        until: Option<chrono::DateTime<chrono::Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<AuditEntry>> {
        let mut entries = self.read_entries()?;

        // Apply filters
        entries.retain(|entry| {
            if let Some(et) = event_type {
                if entry.event_type != et {
                    return false;
                }
            }

            if let Some(sev) = severity {
                if entry.severity < sev {
                    return false;
                }
            }

            if let Some(u) = user {
                if entry.user != u {
                    return false;
                }
            }

            if let Some(since_time) = since {
                if entry.timestamp < since_time {
                    return false;
                }
            }

            if let Some(until_time) = until {
                if entry.timestamp > until_time {
                    return false;
                }
            }

            true
        });

        // Sort by timestamp (newest first)
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply limit
        if let Some(limit_count) = limit {
            entries.truncate(limit_count);
        }

        Ok(entries)
    }

    /// Rotate audit log if needed
    fn rotate_if_needed(&self) -> Result<()> {
        if !self.config.log_path.exists() {
            return Ok(());
        }

        let entries = self.read_entries()?;

        if entries.len() < self.config.max_entries {
            return Ok(());
        }

        // Rotate logs
        for i in (1..self.config.keep_rotations).rev() {
            let old_path = self.get_rotated_path(i);
            let new_path = self.get_rotated_path(i + 1);

            if old_path.exists() {
                fs::rename(&old_path, &new_path).with_context(|| {
                    format!(
                        "Failed to rotate log {} to {}",
                        old_path.display(),
                        new_path.display()
                    )
                })?;
            }
        }

        // Move current log to .1
        let rotated_path = self.get_rotated_path(1);
        fs::rename(&self.config.log_path, &rotated_path).with_context(|| {
            format!("Failed to rotate current log to {}", rotated_path.display())
        })?;

        // Remove old rotated logs
        let old_path = self.get_rotated_path(self.config.keep_rotations + 1);
        if old_path.exists() {
            fs::remove_file(&old_path).with_context(|| {
                format!("Failed to remove old rotated log: {}", old_path.display())
            })?;
        }

        Ok(())
    }

    /// Get rotated log path
    fn get_rotated_path(&self, rotation: usize) -> PathBuf {
        let mut path = self.config.log_path.clone();
        let file_name = path.file_name().unwrap().to_string_lossy();
        path.set_file_name(format!("{}.{}", file_name, rotation));
        path
    }

    /// Clear all audit logs
    pub fn clear(&self) -> Result<()> {
        // Remove current log
        if self.config.log_path.exists() {
            fs::remove_file(&self.config.log_path).with_context(|| {
                format!(
                    "Failed to remove audit log: {}",
                    self.config.log_path.display()
                )
            })?;
        }

        // Remove rotated logs
        for i in 1..=self.config.keep_rotations {
            let rotated_path = self.get_rotated_path(i);
            if rotated_path.exists() {
                fs::remove_file(&rotated_path).with_context(|| {
                    format!("Failed to remove rotated log: {}", rotated_path.display())
                })?;
            }
        }

        Ok(())
    }

    /// Get audit statistics
    pub fn get_stats(&self) -> Result<AuditStats> {
        let entries = self.read_entries()?;

        let total_entries = entries.len();
        let by_event_type = count_by_event_type(&entries);
        let by_severity = count_by_severity(&entries);
        let by_user = count_by_user(&entries);

        let oldest_entry = entries.iter().map(|e| e.timestamp).min();
        let newest_entry = entries.iter().map(|e| e.timestamp).max();

        Ok(AuditStats {
            total_entries,
            by_event_type,
            by_severity,
            by_user,
            oldest_entry,
            newest_entry,
        })
    }
}

/// Audit statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_entries: usize,
    pub by_event_type: std::collections::HashMap<String, usize>,
    pub by_severity: std::collections::HashMap<String, usize>,
    pub by_user: std::collections::HashMap<String, usize>,
    pub oldest_entry: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_entry: Option<chrono::DateTime<chrono::Utc>>,
}

fn count_by_event_type(entries: &[AuditEntry]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for entry in entries {
        let key = format!("{:?}", entry.event_type);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn count_by_severity(entries: &[AuditEntry]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for entry in entries {
        let key = format!("{:?}", entry.severity);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn count_by_user(entries: &[AuditEntry]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for entry in entries {
        *counts.entry(entry.user.clone()).or_insert(0) += 1;
    }
    counts
}

/// Get default audit log path
fn get_default_audit_log_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("mielin").join("audit.log")
    } else {
        PathBuf::from(".mielin_audit.log")
    }
}

/// Get current user
fn get_current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            user: "test_user".to_string(),
            command: "node".to_string(),
            args: vec!["list".to_string()],
            exit_code: Some(0),
            duration_ms: Some(100),
            event_type: AuditEventType::CommandExecution,
            severity: AuditSeverity::Info,
            metadata: serde_json::json!({}),
        };

        assert_eq!(entry.command, "node");
        assert_eq!(entry.severity, AuditSeverity::Info);
    }

    #[test]
    fn test_audit_config_default() {
        let config = AuditConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10000);
        assert_eq!(config.keep_rotations, 5);
        assert_eq!(config.min_severity, AuditSeverity::Info);
    }

    #[test]
    fn test_get_current_user() {
        let user = get_current_user();
        assert!(!user.is_empty());
    }

    #[test]
    fn test_severity_ordering() {
        assert!(AuditSeverity::Info < AuditSeverity::Warning);
        assert!(AuditSeverity::Warning < AuditSeverity::Error);
        assert!(AuditSeverity::Error < AuditSeverity::Critical);
    }
}
