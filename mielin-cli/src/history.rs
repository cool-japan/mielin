//! Command history management for MielinCTL

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of history entries to keep
const MAX_HISTORY_SIZE: usize = 1000;

/// Command history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Timestamp when command was executed
    pub timestamp: DateTime<Utc>,
    /// Command that was executed
    pub command: String,
    /// Exit code of the command
    pub exit_code: i32,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Command history manager
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    /// List of history entries
    entries: Vec<HistoryEntry>,
}

impl History {
    /// Get the history file path
    pub fn history_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to determine config directory")?
            .join("mielin");

        fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

        Ok(config_dir.join("history"))
    }

    /// Load history from file
    pub fn load() -> Result<Self> {
        let history_path = Self::history_path()?;

        if history_path.exists() {
            let contents =
                fs::read_to_string(&history_path).context("Failed to read history file")?;

            let history: History =
                serde_json::from_str(&contents).context("Failed to parse history file")?;

            Ok(history)
        } else {
            Ok(Self::default())
        }
    }

    /// Save history to file
    pub fn save(&self) -> Result<()> {
        let history_path = Self::history_path()?;
        let contents = serde_json::to_string_pretty(self).context("Failed to serialize history")?;

        fs::write(&history_path, contents).context("Failed to write history file")?;

        Ok(())
    }

    /// Add a command to history
    pub fn add(&mut self, command: String, exit_code: i32, duration_ms: u64) {
        let entry = HistoryEntry {
            timestamp: Utc::now(),
            command,
            exit_code,
            duration_ms,
        };

        self.entries.push(entry);

        // Trim history if it exceeds maximum size
        if self.entries.len() > MAX_HISTORY_SIZE {
            let excess = self.entries.len() - MAX_HISTORY_SIZE;
            self.entries.drain(0..excess);
        }
    }

    /// Get all history entries
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Get the last N entries
    pub fn last(&self, n: usize) -> &[HistoryEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }

    /// Search history by pattern
    pub fn search(&self, pattern: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.command.contains(pattern))
            .collect()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get history statistics
    pub fn stats(&self) -> HistoryStats {
        let total_commands = self.entries.len();
        let successful_commands = self.entries.iter().filter(|e| e.exit_code == 0).count();
        let failed_commands = total_commands - successful_commands;

        let avg_duration_ms = if total_commands > 0 {
            self.entries.iter().map(|e| e.duration_ms).sum::<u64>() / total_commands as u64
        } else {
            0
        };

        let most_used_commands = self.get_most_used_commands(5);

        HistoryStats {
            total_commands,
            successful_commands,
            failed_commands,
            avg_duration_ms,
            most_used_commands,
        }
    }

    /// Get the most used commands
    fn get_most_used_commands(&self, limit: usize) -> Vec<(String, usize)> {
        use std::collections::HashMap;

        let mut command_counts = HashMap::new();

        for entry in &self.entries {
            // Extract the main command (first word)
            let main_command = entry.command.split_whitespace().next().unwrap_or("");
            *command_counts.entry(main_command.to_string()).or_insert(0) += 1;
        }

        let mut counts: Vec<_> = command_counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts.truncate(limit);

        counts
    }

    /// Export history to a file
    pub fn export(&self, path: &PathBuf) -> Result<()> {
        let contents =
            serde_json::to_string_pretty(&self.entries).context("Failed to serialize history")?;

        fs::write(path, contents).context("Failed to write export file")?;

        Ok(())
    }
}

/// History statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryStats {
    /// Total number of commands
    pub total_commands: usize,
    /// Number of successful commands
    pub successful_commands: usize,
    /// Number of failed commands
    pub failed_commands: usize,
    /// Average command duration in milliseconds
    pub avg_duration_ms: u64,
    /// Most used commands
    pub most_used_commands: Vec<(String, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_default() {
        let history = History::default();
        assert_eq!(history.entries().len(), 0);
    }

    #[test]
    fn test_add_entry() {
        let mut history = History::default();
        history.add("node list".to_string(), 0, 100);
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].command, "node list");
        assert_eq!(history.entries()[0].exit_code, 0);
        assert_eq!(history.entries()[0].duration_ms, 100);
    }

    #[test]
    fn test_max_history_size() {
        let mut history = History::default();

        // Add more than MAX_HISTORY_SIZE entries
        for i in 0..(MAX_HISTORY_SIZE + 100) {
            history.add(format!("command {}", i), 0, 100);
        }

        // Should be trimmed to MAX_HISTORY_SIZE
        assert_eq!(history.entries().len(), MAX_HISTORY_SIZE);

        // First entry should be command 100 (oldest 100 removed)
        assert_eq!(history.entries()[0].command, "command 100");
    }

    #[test]
    fn test_last_n_entries() {
        let mut history = History::default();
        history.add("cmd1".to_string(), 0, 100);
        history.add("cmd2".to_string(), 0, 100);
        history.add("cmd3".to_string(), 0, 100);

        let last_two = history.last(2);
        assert_eq!(last_two.len(), 2);
        assert_eq!(last_two[0].command, "cmd2");
        assert_eq!(last_two[1].command, "cmd3");
    }

    #[test]
    fn test_search() {
        let mut history = History::default();
        history.add("node list".to_string(), 0, 100);
        history.add("agent list".to_string(), 0, 100);
        history.add("node info".to_string(), 0, 100);

        let results = history.search("node");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|e| e.command == "node list"));
        assert!(results.iter().any(|e| e.command == "node info"));
    }

    #[test]
    fn test_clear() {
        let mut history = History::default();
        history.add("cmd1".to_string(), 0, 100);
        history.add("cmd2".to_string(), 0, 100);
        assert_eq!(history.entries().len(), 2);

        history.clear();
        assert_eq!(history.entries().len(), 0);
    }

    #[test]
    fn test_stats() {
        let mut history = History::default();
        history.add("node list".to_string(), 0, 100);
        history.add("node list".to_string(), 0, 200);
        history.add("agent list".to_string(), 1, 150);

        let stats = history.stats();
        assert_eq!(stats.total_commands, 3);
        assert_eq!(stats.successful_commands, 2);
        assert_eq!(stats.failed_commands, 1);
        assert_eq!(stats.avg_duration_ms, 150);
        assert_eq!(stats.most_used_commands[0].0, "node");
        assert_eq!(stats.most_used_commands[0].1, 2);
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut history = History::default();
        history.add("test command".to_string(), 0, 100);

        let json = serde_json::to_string(&history).unwrap();
        let deserialized: History = serde_json::from_str(&json).unwrap();

        assert_eq!(history.entries().len(), deserialized.entries().len());
        assert_eq!(
            history.entries()[0].command,
            deserialized.entries()[0].command
        );
    }
}
