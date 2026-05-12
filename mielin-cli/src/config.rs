//! Configuration management for MielinCTL

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// MielinCTL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Node configuration
    pub node: NodeConfig,
    /// CLI configuration
    pub cli: CliConfig,
    /// Daemon configuration
    pub daemon: DaemonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node ID (optional, generated if not provided)
    pub id: Option<String>,
    /// Node role (core, relay, edge)
    pub role: String,
    /// Listen address for the mesh service
    pub listen_address: String,
    /// Bootstrap nodes to connect to
    pub bootstrap_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Default output format
    pub default_output_format: String,
    /// Enable colors in output
    pub enable_colors: bool,
    /// Timeout for commands in seconds
    pub command_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Enable mDNS discovery
    pub enable_mdns: bool,
    /// Enable gossip protocol
    pub enable_gossip: bool,
    /// Enable registry
    pub enable_registry: bool,
    /// Enable migration
    pub enable_migration: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node: NodeConfig {
                id: None,
                role: "edge".to_string(),
                listen_address: "0.0.0.0:8080".to_string(),
                bootstrap_nodes: Vec::new(),
            },
            cli: CliConfig {
                default_output_format: "table".to_string(),
                enable_colors: true,
                command_timeout_secs: 30,
            },
            daemon: DaemonConfig {
                enable_mdns: true,
                enable_gossip: true,
                enable_registry: true,
                enable_migration: true,
            },
        }
    }
}

impl Config {
    /// Get the default configuration file path (without creating directories)
    pub fn default_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("mielin").join("config.toml")
        } else {
            PathBuf::from(".mielin").join("config.toml")
        }
    }

    /// Get the configuration file path (creates directory if needed)
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to determine config directory")?
            .join("mielin");

        fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

        Ok(config_dir.join("config.toml"))
    }

    /// Load configuration from file, or create default if it doesn't exist
    /// Environment variables will override file settings
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        Self::load_from_path(&config_path)
    }

    /// Load configuration from a specific path
    pub fn load_from_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        let mut config = if path.exists() {
            let contents = fs::read_to_string(path).context("Failed to read config file")?;

            // Apply template substitution before parsing
            let processed_contents = Self::process_template(&contents)?;

            toml::from_str(&processed_contents).context("Failed to parse config file")?
        } else {
            let config = Self::default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            config.save_to_path(path)?;
            config
        };

        // Apply environment variable overrides
        config.apply_env_overrides();

        Ok(config)
    }

    /// Process template variables in config content
    /// Supports syntax: ${VAR} and ${VAR:-default}
    fn process_template(content: &str) -> Result<String> {
        let mut result = content.to_string();
        let mut vars = HashMap::new();

        // Collect all environment variables
        for (key, value) in std::env::vars() {
            vars.insert(key, value);
        }

        // Process ${VAR:-default} patterns first (with defaults)
        let re_with_default = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*):-([^}]*)\}")
            .context("Failed to compile regex")?;

        for cap in re_with_default.captures_iter(content) {
            let full_match = cap.get(0).expect("group 0 always present in a match").as_str();
            let var_name = cap.get(1).expect("group 1 defined by regex pattern").as_str();
            let default_value = cap.get(2).expect("group 2 defined by regex pattern").as_str();

            let replacement = vars
                .get(var_name)
                .map(|v| v.as_str())
                .unwrap_or(default_value);
            result = result.replace(full_match, replacement);
        }

        // Process ${VAR} patterns (without defaults)
        let re_simple = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
            .context("Failed to compile regex")?;

        for cap in re_simple.captures_iter(&result.clone()) {
            let full_match = cap.get(0).expect("group 0 always present in a match").as_str();
            let var_name = cap.get(1).expect("group 1 defined by regex pattern").as_str();

            if let Some(value) = vars.get(var_name) {
                result = result.replace(full_match, value);
            } else {
                // If variable is not found and no default, leave it as empty string
                result = result.replace(full_match, "");
            }
        }

        Ok(result)
    }

    /// Apply environment variable overrides
    /// Environment variables use the format: MIELIN_SECTION_KEY (e.g., MIELIN_NODE_ROLE)
    fn apply_env_overrides(&mut self) {
        // Node configuration
        if let Ok(val) = std::env::var("MIELIN_NODE_ID") {
            self.node.id = Some(val);
        }
        if let Ok(val) = std::env::var("MIELIN_NODE_ROLE") {
            self.node.role = val;
        }
        if let Ok(val) = std::env::var("MIELIN_NODE_LISTEN_ADDRESS") {
            self.node.listen_address = val;
        }
        if let Ok(val) = std::env::var("MIELIN_NODE_BOOTSTRAP_NODES") {
            self.node.bootstrap_nodes = val.split(',').map(|s| s.trim().to_string()).collect();
        }

        // CLI configuration
        if let Ok(val) = std::env::var("MIELIN_CLI_DEFAULT_OUTPUT_FORMAT") {
            self.cli.default_output_format = val;
        }
        if let Ok(val) = std::env::var("MIELIN_CLI_ENABLE_COLORS") {
            if let Ok(parsed) = val.parse::<bool>() {
                self.cli.enable_colors = parsed;
            }
        }
        if let Ok(val) = std::env::var("MIELIN_CLI_COMMAND_TIMEOUT_SECS") {
            if let Ok(parsed) = val.parse::<u64>() {
                self.cli.command_timeout_secs = parsed;
            }
        }

        // Daemon configuration
        if let Ok(val) = std::env::var("MIELIN_DAEMON_ENABLE_MDNS") {
            if let Ok(parsed) = val.parse::<bool>() {
                self.daemon.enable_mdns = parsed;
            }
        }
        if let Ok(val) = std::env::var("MIELIN_DAEMON_ENABLE_GOSSIP") {
            if let Ok(parsed) = val.parse::<bool>() {
                self.daemon.enable_gossip = parsed;
            }
        }
        if let Ok(val) = std::env::var("MIELIN_DAEMON_ENABLE_REGISTRY") {
            if let Ok(parsed) = val.parse::<bool>() {
                self.daemon.enable_registry = parsed;
            }
        }
        if let Ok(val) = std::env::var("MIELIN_DAEMON_ENABLE_MIGRATION") {
            if let Ok(parsed) = val.parse::<bool>() {
                self.daemon.enable_migration = parsed;
            }
        }
    }

    /// Save configuration to default file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        self.save_to_path(&config_path)
    }

    /// Save configuration to a specific path
    pub fn save_to_path<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path.as_ref(), contents).context("Failed to write config file")?;
        Ok(())
    }

    /// Get configuration value by key
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "node.id" => self.node.id.clone(),
            "node.role" => Some(self.node.role.clone()),
            "node.listen_address" => Some(self.node.listen_address.clone()),
            "cli.default_output_format" => Some(self.cli.default_output_format.clone()),
            "cli.enable_colors" => Some(self.cli.enable_colors.to_string()),
            "cli.command_timeout_secs" => Some(self.cli.command_timeout_secs.to_string()),
            "daemon.enable_mdns" => Some(self.daemon.enable_mdns.to_string()),
            "daemon.enable_gossip" => Some(self.daemon.enable_gossip.to_string()),
            "daemon.enable_registry" => Some(self.daemon.enable_registry.to_string()),
            "daemon.enable_migration" => Some(self.daemon.enable_migration.to_string()),
            _ => None,
        }
    }

    /// Set configuration value by key
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "node.id" => self.node.id = Some(value.to_string()),
            "node.role" => self.node.role = value.to_string(),
            "node.listen_address" => self.node.listen_address = value.to_string(),
            "cli.default_output_format" => self.cli.default_output_format = value.to_string(),
            "cli.enable_colors" => {
                self.cli.enable_colors = value.parse().context("Invalid boolean value")?;
            }
            "cli.command_timeout_secs" => {
                self.cli.command_timeout_secs = value.parse().context("Invalid integer value")?;
            }
            "daemon.enable_mdns" => {
                self.daemon.enable_mdns = value.parse().context("Invalid boolean value")?;
            }
            "daemon.enable_gossip" => {
                self.daemon.enable_gossip = value.parse().context("Invalid boolean value")?;
            }
            "daemon.enable_registry" => {
                self.daemon.enable_registry = value.parse().context("Invalid boolean value")?;
            }
            "daemon.enable_migration" => {
                self.daemon.enable_migration = value.parse().context("Invalid boolean value")?;
            }
            "node.bootstrap_nodes" => {
                self.node.bootstrap_nodes =
                    value.split(',').map(|s| s.trim().to_string()).collect();
            }
            _ => anyhow::bail!("Unknown configuration key: {}", key),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.node.role, "edge");
        assert_eq!(config.node.listen_address, "0.0.0.0:8080");
        assert!(config.daemon.enable_mdns);
    }

    #[test]
    fn test_get_config() {
        let config = Config::default();
        assert_eq!(config.get("node.role"), Some("edge".to_string()));
        assert_eq!(config.get("cli.enable_colors"), Some("true".to_string()));
        assert_eq!(config.get("unknown"), None);
    }

    #[test]
    fn test_set_config() {
        let mut config = Config::default();
        config.set("node.role", "core").unwrap();
        assert_eq!(config.node.role, "core");

        config.set("cli.command_timeout_secs", "60").unwrap();
        assert_eq!(config.cli.command_timeout_secs, 60);
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.node.role, deserialized.node.role);
    }

    #[test]
    fn test_env_override_node_role() {
        std::env::set_var("MIELIN_NODE_ROLE", "core");
        let mut config = Config::default();
        config.apply_env_overrides();
        assert_eq!(config.node.role, "core");
        std::env::remove_var("MIELIN_NODE_ROLE");
    }

    #[test]
    fn test_env_override_cli_timeout() {
        std::env::set_var("MIELIN_CLI_COMMAND_TIMEOUT_SECS", "120");
        let mut config = Config::default();
        config.apply_env_overrides();
        assert_eq!(config.cli.command_timeout_secs, 120);
        std::env::remove_var("MIELIN_CLI_COMMAND_TIMEOUT_SECS");
    }

    #[test]
    fn test_env_override_bool_values() {
        std::env::set_var("MIELIN_CLI_ENABLE_COLORS", "false");
        std::env::set_var("MIELIN_DAEMON_ENABLE_MDNS", "false");
        let mut config = Config::default();
        config.apply_env_overrides();
        assert!(!config.cli.enable_colors);
        assert!(!config.daemon.enable_mdns);
        std::env::remove_var("MIELIN_CLI_ENABLE_COLORS");
        std::env::remove_var("MIELIN_DAEMON_ENABLE_MDNS");
    }

    #[test]
    fn test_env_override_bootstrap_nodes() {
        std::env::set_var("MIELIN_NODE_BOOTSTRAP_NODES", "node1:8080, node2:8080");
        let mut config = Config::default();
        config.apply_env_overrides();
        assert_eq!(config.node.bootstrap_nodes.len(), 2);
        assert_eq!(config.node.bootstrap_nodes[0], "node1:8080");
        assert_eq!(config.node.bootstrap_nodes[1], "node2:8080");
        std::env::remove_var("MIELIN_NODE_BOOTSTRAP_NODES");
    }

    #[test]
    fn test_template_simple_substitution() {
        std::env::set_var("TEST_VAR", "test_value");
        let input = "key = \"${TEST_VAR}\"";
        let result = Config::process_template(input).unwrap();
        assert_eq!(result, "key = \"test_value\"");
        std::env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_template_with_default() {
        std::env::remove_var("NONEXISTENT_VAR");
        let input = "key = \"${NONEXISTENT_VAR:-default_value}\"";
        let result = Config::process_template(input).unwrap();
        assert_eq!(result, "key = \"default_value\"");
    }

    #[test]
    fn test_template_with_default_override() {
        std::env::set_var("EXISTING_VAR", "actual_value");
        let input = "key = \"${EXISTING_VAR:-default_value}\"";
        let result = Config::process_template(input).unwrap();
        assert_eq!(result, "key = \"actual_value\"");
        std::env::remove_var("EXISTING_VAR");
    }

    #[test]
    fn test_template_multiple_variables() {
        std::env::set_var("VAR1", "value1");
        std::env::set_var("VAR2", "value2");
        let input = "key1 = \"${VAR1}\"\nkey2 = \"${VAR2:-default}\"";
        let result = Config::process_template(input).unwrap();
        assert!(result.contains("value1"));
        assert!(result.contains("value2"));
        std::env::remove_var("VAR1");
        std::env::remove_var("VAR2");
    }

    #[test]
    fn test_template_empty_when_not_found() {
        std::env::remove_var("MISSING_VAR");
        let input = "key = \"${MISSING_VAR}\"";
        let result = Config::process_template(input).unwrap();
        assert_eq!(result, "key = \"\"");
    }
}
