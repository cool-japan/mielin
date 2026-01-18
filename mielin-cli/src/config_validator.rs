//! Configuration validation and migration utilities

use crate::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Is the configuration valid
    pub is_valid: bool,
    /// Validation errors
    pub errors: Vec<ValidationError>,
    /// Validation warnings
    pub warnings: Vec<ValidationWarning>,
    /// Suggested fixes
    pub suggestions: Vec<ValidationSuggestion>,
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Field path (e.g., "node.listen_address")
    pub field: String,
    /// Error message
    pub message: String,
    /// Severity level
    pub severity: ErrorSeverity,
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorSeverity {
    /// Critical error - config cannot be used
    Critical,
    /// Error - config may not work correctly
    Error,
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Field path
    pub field: String,
    /// Warning message
    pub message: String,
}

/// Validation suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationSuggestion {
    /// Field path
    pub field: String,
    /// Suggested value
    pub suggested_value: String,
    /// Reason for suggestion
    pub reason: String,
}

/// Configuration validator
pub struct ConfigValidator {
    config: Config,
}

impl ConfigValidator {
    /// Create a new validator for the given configuration
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Load configuration from file and create a validator
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = Config::load_from_path(path.as_ref())?;
        Ok(Self::new(config))
    }

    /// Validate the configuration
    pub fn validate(&self) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();

        // Validate node configuration
        self.validate_node(&mut errors, &mut warnings, &mut suggestions);

        // Validate daemon configuration
        self.validate_daemon(&mut errors, &mut warnings, &mut suggestions);

        // Validate CLI configuration
        self.validate_cli(&mut errors, &mut warnings, &mut suggestions);

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
            suggestions,
        }
    }

    fn validate_node(
        &self,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationWarning>,
        suggestions: &mut Vec<ValidationSuggestion>,
    ) {
        // Validate node ID
        if let Some(ref id) = self.config.node.id {
            if id.is_empty() {
                errors.push(ValidationError {
                    field: "node.id".to_string(),
                    message: "Node ID cannot be empty".to_string(),
                    severity: ErrorSeverity::Critical,
                });
            } else if id.len() > 64 {
                warnings.push(ValidationWarning {
                    field: "node.id".to_string(),
                    message: "Node ID is very long (>64 characters), this may cause issues"
                        .to_string(),
                });
            }
        } else {
            warnings.push(ValidationWarning {
                field: "node.id".to_string(),
                message: "Node ID is not set, will be auto-generated at runtime".to_string(),
            });
        }

        // Validate role
        let valid_roles = ["core", "relay", "edge"];
        if !valid_roles.contains(&self.config.node.role.as_str()) {
            errors.push(ValidationError {
                field: "node.role".to_string(),
                message: format!(
                    "Invalid node role '{}'. Must be one of: core, relay, edge",
                    self.config.node.role
                ),
                severity: ErrorSeverity::Error,
            });
        }

        // Validate listen address
        if let Err(e) = self
            .config
            .node
            .listen_address
            .parse::<std::net::SocketAddr>()
        {
            errors.push(ValidationError {
                field: "node.listen_address".to_string(),
                message: format!("Invalid listen address: {}", e),
                severity: ErrorSeverity::Critical,
            });
        }

        // Validate bootstrap nodes
        for (idx, node) in self.config.node.bootstrap_nodes.iter().enumerate() {
            if let Err(e) = node.parse::<std::net::SocketAddr>() {
                errors.push(ValidationError {
                    field: format!("node.bootstrap_nodes[{}]", idx),
                    message: format!("Invalid bootstrap node address '{}': {}", node, e),
                    severity: ErrorSeverity::Error,
                });
            }
        }

        // Suggestions
        if self.config.node.role == "edge" && !self.config.node.bootstrap_nodes.is_empty() {
            suggestions.push(ValidationSuggestion {
                field: "node.role".to_string(),
                suggested_value: "relay".to_string(),
                reason: "Edge nodes with bootstrap nodes may want to be relay nodes instead"
                    .to_string(),
            });
        }
    }

    fn validate_daemon(
        &self,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationWarning>,
        _suggestions: &mut Vec<ValidationSuggestion>,
    ) {
        // Validate that at least one service is enabled
        if !self.config.daemon.enable_mdns
            && !self.config.daemon.enable_gossip
            && !self.config.daemon.enable_registry
            && !self.config.daemon.enable_migration
        {
            warnings.push(ValidationWarning {
                field: "daemon".to_string(),
                message: "All daemon services are disabled, the daemon may not function properly"
                    .to_string(),
            });
        }

        // Warn if mDNS is enabled on non-local networks
        if self.config.daemon.enable_mdns && !is_local_address(&self.config.node.listen_address) {
            warnings.push(ValidationWarning {
                field: "daemon.enable_mdns".to_string(),
                message:
                    "mDNS is enabled but listen address is not local, this may not work as expected"
                        .to_string(),
            });
        }

        // Validate gossip is enabled for distributed operations
        if self.config.daemon.enable_registry && !self.config.daemon.enable_gossip {
            errors.push(ValidationError {
                field: "daemon.enable_gossip".to_string(),
                message: "Gossip must be enabled when registry is enabled".to_string(),
                severity: ErrorSeverity::Error,
            });
        }
    }

    fn validate_cli(
        &self,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationWarning>,
        suggestions: &mut Vec<ValidationSuggestion>,
    ) {
        // Validate command timeout
        if self.config.cli.command_timeout_secs == 0 {
            errors.push(ValidationError {
                field: "cli.command_timeout_secs".to_string(),
                message: "Command timeout cannot be 0".to_string(),
                severity: ErrorSeverity::Error,
            });
        } else if self.config.cli.command_timeout_secs < 5 {
            warnings.push(ValidationWarning {
                field: "cli.command_timeout_secs".to_string(),
                message: "Command timeout is very short (<5s), commands may timeout prematurely"
                    .to_string(),
            });
        } else if self.config.cli.command_timeout_secs > 300 {
            warnings.push(ValidationWarning {
                field: "cli.command_timeout_secs".to_string(),
                message: "Command timeout is very long (>5min), failed commands may hang"
                    .to_string(),
            });
        }

        // Validate output format
        let valid_formats = ["table", "json", "yaml", "quiet"];
        if !valid_formats.contains(&self.config.cli.default_output_format.as_str()) {
            errors.push(ValidationError {
                field: "cli.default_output_format".to_string(),
                message: format!(
                    "Invalid output format '{}'. Must be one of: table, json, yaml, quiet",
                    self.config.cli.default_output_format
                ),
                severity: ErrorSeverity::Error,
            });
        }

        // Suggestions for performance
        if self.config.cli.enable_colors && self.config.cli.default_output_format == "json" {
            suggestions.push(ValidationSuggestion {
                field: "cli.enable_colors".to_string(),
                suggested_value: "false".to_string(),
                reason: "Colors are not needed for JSON output and may interfere with parsing"
                    .to_string(),
            });
        }
    }

    /// Auto-fix configuration issues
    pub fn auto_fix(&mut self) -> Vec<String> {
        let mut fixes = Vec::new();

        // Fix empty or missing node ID
        if self
            .config
            .node
            .id
            .as_ref()
            .map(|id| id.is_empty())
            .unwrap_or(false)
        {
            let new_id = format!("node-{}", uuid::Uuid::new_v4());
            self.config.node.id = Some(new_id.clone());
            fixes.push(format!("Generated node ID: {}", new_id));
        }

        // Fix invalid role
        let valid_roles = ["core", "relay", "edge"];
        if !valid_roles.contains(&self.config.node.role.as_str()) {
            self.config.node.role = "edge".to_string();
            fixes.push("Set node role to 'edge' (default)".to_string());
        }

        // Fix invalid timeout
        if self.config.cli.command_timeout_secs == 0 {
            self.config.cli.command_timeout_secs = 30;
            fixes.push("Set command timeout to 30 seconds (default)".to_string());
        }

        // Fix invalid output format
        let valid_formats = ["table", "json", "yaml", "quiet"];
        if !valid_formats.contains(&self.config.cli.default_output_format.as_str()) {
            self.config.cli.default_output_format = "table".to_string();
            fixes.push("Set output format to 'table' (default)".to_string());
        }

        // Enable gossip if registry is enabled
        if self.config.daemon.enable_registry && !self.config.daemon.enable_gossip {
            self.config.daemon.enable_gossip = true;
            fixes.push("Enabled gossip (required for registry)".to_string());
        }

        fixes
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Save the configuration to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.config.save_to_path(path.as_ref())
    }
}

/// Check if an address is local
fn is_local_address(addr: &str) -> bool {
    addr.starts_with("127.") || addr.starts_with("localhost") || addr.starts_with("0.0.0.0")
}

/// Configuration migration utilities
pub struct ConfigMigrator;

impl ConfigMigrator {
    /// Migrate configuration from version to version
    pub fn migrate(from_version: &str, _config: &mut Config) -> Result<Vec<String>> {
        let mut changes = Vec::new();

        match from_version {
            "0.0.1" => {
                // Example migration: Add new fields with defaults
                changes.push("Migrated from v0.0.1 to current version".to_string());
            }
            _ => {
                anyhow::bail!("Unknown configuration version: {}", from_version);
            }
        }

        Ok(changes)
    }

    /// Detect configuration version
    pub fn detect_version(_config: &Config) -> String {
        // In a real implementation, this would check for version markers
        // For now, we'll just return the current version
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_empty_node_id() {
        let mut config = Config::default();
        config.node.id = Some(String::new());

        let validator = ConfigValidator::new(config);
        let result = validator.validate();

        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "node.id"));
    }

    #[test]
    fn test_validator_invalid_role() {
        let mut config = Config::default();
        config.node.role = "invalid".to_string();

        let validator = ConfigValidator::new(config);
        let result = validator.validate();

        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "node.role"));
    }

    #[test]
    fn test_validator_invalid_listen_address() {
        let mut config = Config::default();
        config.node.listen_address = "invalid".to_string();

        let validator = ConfigValidator::new(config);
        let result = validator.validate();

        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.field == "node.listen_address"));
    }

    #[test]
    fn test_auto_fix_empty_node_id() {
        let mut config = Config::default();
        config.node.id = Some(String::new());

        let mut validator = ConfigValidator::new(config);
        let fixes = validator.auto_fix();

        assert!(!fixes.is_empty());
        assert!(validator
            .config()
            .node
            .id
            .as_ref()
            .map(|id| !id.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn test_auto_fix_invalid_timeout() {
        let mut config = Config::default();
        config.cli.command_timeout_secs = 0;

        let mut validator = ConfigValidator::new(config);
        let fixes = validator.auto_fix();

        assert!(!fixes.is_empty());
        assert_eq!(validator.config().cli.command_timeout_secs, 30);
    }

    #[test]
    fn test_is_local_address() {
        assert!(is_local_address("127.0.0.1:8080"));
        assert!(is_local_address("localhost:8080"));
        assert!(is_local_address("0.0.0.0:8080"));
        assert!(!is_local_address("192.168.1.1:8080"));
    }
}
