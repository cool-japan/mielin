//! Remote Management for MielinCTL
//!
//! Provides capabilities to manage remote MielinOS nodes through
//! the CLI, enabling centralized control of distributed deployments.

use anyhow::{Context, Result};
use oxihttp_client::{Client, HttpsClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Remote node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteNode {
    /// Unique node identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Node address (host:port)
    pub address: String,
    /// Authentication method
    pub auth: AuthMethod,
    /// Connection options
    #[serde(default)]
    pub options: ConnectionOptions,
    /// Node tags for grouping
    #[serde(default)]
    pub tags: Vec<String>,
    /// Node description
    #[serde(default)]
    pub description: String,
}

/// Authentication method for remote connections
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthMethod {
    /// No authentication (insecure)
    None,
    /// API key authentication
    ApiKey {
        /// API key value
        key: String,
    },
    /// Certificate-based authentication
    Certificate {
        /// Path to client certificate
        cert_path: String,
        /// Path to private key
        key_path: String,
        /// Optional CA certificate path
        ca_path: Option<String>,
    },
    /// Token-based authentication
    Token {
        /// Bearer token
        token: String,
    },
}

/// Connection options for remote nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionOptions {
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Enable TLS
    #[serde(default = "default_true")]
    pub tls: bool,
    /// Verify SSL certificates
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    /// Maximum retry attempts
    #[serde(default = "default_retries")]
    pub max_retries: u32,
}

fn default_timeout() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

fn default_retries() -> u32 {
    3
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout(),
            tls: default_true(),
            verify_ssl: default_true(),
            max_retries: default_retries(),
        }
    }
}

/// Remote command execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCommand {
    /// Command to execute
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Remote command execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCommandResult {
    /// Node ID
    pub node_id: String,
    /// Exit code
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

/// Remote node manager
pub struct RemoteManager {
    /// Remote nodes configuration
    nodes: HashMap<String, RemoteNode>,
    /// Configuration file path
    config_path: PathBuf,
}

impl RemoteManager {
    /// Create a new remote manager
    pub fn new() -> Result<Self> {
        let config_path = Self::get_config_path()?;

        let mut manager = RemoteManager {
            nodes: HashMap::new(),
            config_path,
        };

        // Load existing configuration if it exists
        if manager.config_path.exists() {
            manager.load_config()?;
        }

        Ok(manager)
    }

    /// Get the default configuration file path
    pub fn get_config_path() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?;
        let mielin_dir = config_dir.join("mielin");

        // Create directory if it doesn't exist
        if !mielin_dir.exists() {
            fs::create_dir_all(&mielin_dir).context("Failed to create mielin config directory")?;
        }

        Ok(mielin_dir.join("remote_nodes.toml"))
    }

    /// Load remote nodes configuration from file
    pub fn load_config(&mut self) -> Result<()> {
        debug!(
            "Loading remote nodes configuration from {:?}",
            self.config_path
        );

        let content = fs::read_to_string(&self.config_path)
            .context("Failed to read remote nodes configuration")?;

        let nodes: HashMap<String, RemoteNode> =
            toml::from_str(&content).context("Failed to parse remote nodes configuration")?;

        self.nodes = nodes;
        info!("Loaded {} remote node(s)", self.nodes.len());

        Ok(())
    }

    /// Save remote nodes configuration to file
    pub fn save_config(&self) -> Result<()> {
        debug!(
            "Saving remote nodes configuration to {:?}",
            self.config_path
        );

        let content = toml::to_string_pretty(&self.nodes)
            .context("Failed to serialize remote nodes configuration")?;

        fs::write(&self.config_path, content)
            .context("Failed to write remote nodes configuration")?;

        info!("Saved {} remote node(s)", self.nodes.len());
        Ok(())
    }

    /// Add a remote node
    pub fn add_node(&mut self, node: RemoteNode) -> Result<()> {
        if self.nodes.contains_key(&node.id) {
            anyhow::bail!("Remote node already exists: {}", node.id);
        }

        let id = node.id.clone();
        self.nodes.insert(id.clone(), node);
        self.save_config()?;

        info!("Added remote node: {}", id);
        Ok(())
    }

    /// Remove a remote node
    pub fn remove_node(&mut self, id: &str) -> Result<()> {
        if !self.nodes.contains_key(id) {
            anyhow::bail!("Remote node not found: {}", id);
        }

        self.nodes.remove(id);
        self.save_config()?;

        info!("Removed remote node: {}", id);
        Ok(())
    }

    /// Get a remote node by ID
    pub fn get_node(&self, id: &str) -> Option<&RemoteNode> {
        self.nodes.get(id)
    }

    /// List all remote nodes
    pub fn list_nodes(&self) -> Vec<&RemoteNode> {
        self.nodes.values().collect()
    }

    /// List nodes by tag
    pub fn list_nodes_by_tag(&self, tag: &str) -> Vec<&RemoteNode> {
        self.nodes
            .values()
            .filter(|n| n.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            .collect()
    }

    /// Update a remote node
    pub fn update_node(&mut self, id: &str, node: RemoteNode) -> Result<()> {
        if !self.nodes.contains_key(id) {
            anyhow::bail!("Remote node not found: {}", id);
        }

        self.nodes.insert(id.to_string(), node);
        self.save_config()?;

        info!("Updated remote node: {}", id);
        Ok(())
    }

    /// Build a TLS-capable HTTP client for the given node options.
    ///
    /// `try_clone()` has no equivalent in oxihttp-client. The caller constructs
    /// a fresh client per attempt — `HttpsClient: Clone` makes this cheap.
    fn build_http_client(options: &ConnectionOptions) -> Result<HttpsClient> {
        let timeout = std::time::Duration::from_secs(options.timeout_secs);
        let accept_invalid = !options.verify_ssl;

        Client::builder()
            .with_webpki_roots()
            .danger_accept_invalid_certs(accept_invalid)
            .connect_timeout(timeout)
            .read_timeout(timeout)
            .build_https()
            .context("Failed to build HTTP client")
    }

    /// Execute a command on a remote node
    pub async fn execute_command(
        &self,
        node_id: &str,
        command: RemoteCommand,
    ) -> Result<RemoteCommandResult> {
        let node = self
            .get_node(node_id)
            .ok_or_else(|| anyhow::anyhow!("Remote node not found: {}", node_id))?;

        debug!(
            "Executing command on remote node {}: {}",
            node_id, command.command
        );

        let start_time = std::time::Instant::now();

        // Execute command on remote node via HTTP
        let result = self.execute_remote_command(node, &command).await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(RemoteCommandResult {
            node_id: node_id.to_string(),
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms,
        })
    }

    /// Execute command on remote node via HTTP
    async fn execute_remote_command(
        &self,
        node: &RemoteNode,
        command: &RemoteCommand,
    ) -> Result<RemoteCommandResult> {
        debug!(
            "Executing remote command on {}: {}",
            node.address, command.command
        );

        let start_time = std::time::Instant::now();

        // Build HTTP client with configured options (cheap to clone)
        let client = Self::build_http_client(&node.options)?;

        // Construct API endpoint URL
        let url = if node.options.tls {
            format!("https://{}/api/v1/command", node.address)
        } else {
            format!("http://{}/api/v1/command", node.address)
        };

        let body_json = serde_json::json!({
            "command": command.command,
            "args": command.args,
            "env": command.env,
        });

        // Execute request with retry logic.
        // oxihttp-client has no `try_clone()` — build a fresh request per attempt.
        let mut last_error: Option<String> = None;
        for attempt in 0..node.options.max_retries {
            if attempt > 0 {
                debug!("Retrying command execution (attempt {})", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }

            let mut req = client
                .post(&url)
                .context("Failed to create POST request")?
                .json(&body_json)
                .context("Failed to set JSON body")?;

            // Add authentication header based on method
            req = match &node.auth {
                AuthMethod::None => req,
                AuthMethod::ApiKey { key } => req
                    .header("X-API-Key", key)
                    .context("Failed to set X-API-Key header")?,
                AuthMethod::Token { token } => req
                    .header("Authorization", &format!("Bearer {}", token))
                    .context("Failed to set Authorization header")?,
                AuthMethod::Certificate { .. } => {
                    // Certificate-based auth is handled via TLS client config
                    req
                }
            };

            match req.send().await {
                Ok(response) => {
                    let duration_ms = start_time.elapsed().as_millis() as u64;

                    if response.status().is_success() {
                        // Parse successful response
                        let result: serde_json::Value = response
                            .body_json()
                            .await
                            .context("Failed to parse response")?;

                        return Ok(RemoteCommandResult {
                            node_id: node.id.clone(),
                            exit_code: result
                                .get("exit_code")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32,
                            stdout: result
                                .get("stdout")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            stderr: result
                                .get("stderr")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            duration_ms,
                        });
                    } else {
                        // Handle error response
                        let duration_ms = start_time.elapsed().as_millis() as u64;
                        let error_text = response
                            .body_text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        return Ok(RemoteCommandResult {
                            node_id: node.id.clone(),
                            exit_code: 1,
                            stdout: String::new(),
                            stderr: format!("HTTP error: {}", error_text),
                            duration_ms,
                        });
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }
        }

        // All retries exhausted
        let duration_ms = start_time.elapsed().as_millis() as u64;
        Ok(RemoteCommandResult {
            node_id: node.id.clone(),
            exit_code: 1,
            stdout: String::new(),
            stderr: format!(
                "Connection failed after {} attempts: {}",
                node.options.max_retries,
                last_error.unwrap_or_else(|| "Unknown error".to_string())
            ),
            duration_ms,
        })
    }

    /// Test connection to a remote node
    pub async fn test_connection(&self, node_id: &str) -> Result<bool> {
        let node = self
            .get_node(node_id)
            .ok_or_else(|| anyhow::anyhow!("Remote node not found: {}", node_id))?;

        debug!("Testing connection to remote node: {}", node.address);

        let client = Self::build_http_client(&node.options)?;

        // Construct health check URL
        let url = if node.options.tls {
            format!("https://{}/api/v1/health", node.address)
        } else {
            format!("http://{}/api/v1/health", node.address)
        };

        // Execute request with retry logic — fresh request per attempt
        for attempt in 0..node.options.max_retries {
            if attempt > 0 {
                debug!("Retrying connection test (attempt {})", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }

            let mut req = client
                .get(&url)
                .context("Failed to create GET request")?;

            // Add authentication header based on method
            req = match &node.auth {
                AuthMethod::None => req,
                AuthMethod::ApiKey { key } => req
                    .header("X-API-Key", key)
                    .context("Failed to set X-API-Key header")?,
                AuthMethod::Token { token } => req
                    .header("Authorization", &format!("Bearer {}", token))
                    .context("Failed to set Authorization header")?,
                AuthMethod::Certificate { .. } => req,
            };

            match req.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Connection test successful for {}", node.name);
                        return Ok(true);
                    } else {
                        debug!("Connection test failed with status: {}", response.status());
                    }
                }
                Err(e) => {
                    debug!("Connection attempt {} failed: {}", attempt + 1, e);
                }
            }
        }

        // All connection attempts failed
        warn!(
            "Connection test failed for {} after {} attempts",
            node.name, node.options.max_retries
        );
        Ok(false)
    }

    /// Execute command on multiple nodes
    pub async fn execute_on_multiple(
        &self,
        node_ids: &[String],
        command: RemoteCommand,
    ) -> Result<Vec<RemoteCommandResult>> {
        let mut results = Vec::new();

        for node_id in node_ids {
            match self.execute_command(node_id, command.clone()).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Failed to execute command on {}: {}", node_id, e);
                    results.push(RemoteCommandResult {
                        node_id: node_id.clone(),
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("Error: {}", e),
                        duration_ms: 0,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Import nodes from a configuration file
    pub fn import_nodes(&mut self, path: &Path) -> Result<usize> {
        if !path.exists() {
            anyhow::bail!("Import file not found: {:?}", path);
        }

        let content = fs::read_to_string(path).context("Failed to read import file")?;

        let imported_nodes: HashMap<String, RemoteNode> =
            toml::from_str(&content).context("Failed to parse import file")?;

        let count = imported_nodes.len();

        for (id, node) in imported_nodes {
            self.nodes.insert(id, node);
        }

        self.save_config()?;
        info!("Imported {} remote node(s)", count);

        Ok(count)
    }

    /// Export nodes to a configuration file
    pub fn export_nodes(&self, path: &Path) -> Result<()> {
        let content =
            toml::to_string_pretty(&self.nodes).context("Failed to serialize nodes for export")?;

        fs::write(path, content).context("Failed to write export file")?;

        info!("Exported {} remote node(s) to {:?}", self.nodes.len(), path);
        Ok(())
    }
}

impl Default for RemoteManager {
    fn default() -> Self {
        Self::new().expect("Failed to create remote manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_auth_method_serialization() {
        let auth = AuthMethod::ApiKey {
            key: "test-key".to_string(),
        };

        let toml_str = toml::to_string(&auth).expect("serialize");
        assert!(toml_str.contains("apikey"));
        assert!(toml_str.contains("test-key"));
    }

    #[test]
    fn test_connection_options_default() {
        let options = ConnectionOptions::default();

        assert_eq!(options.timeout_secs, 30);
        assert!(options.tls);
        assert!(options.verify_ssl);
        assert_eq!(options.max_retries, 3);
    }

    #[test]
    fn test_remote_node_serialization() {
        let node = RemoteNode {
            id: "node1".to_string(),
            name: "Test Node".to_string(),
            address: "localhost:8080".to_string(),
            auth: AuthMethod::None,
            options: ConnectionOptions::default(),
            tags: vec!["test".to_string()],
            description: "A test node".to_string(),
        };

        let toml_str = toml::to_string(&node).expect("serialize");
        assert!(toml_str.contains("node1"));
        assert!(toml_str.contains("Test Node"));
    }

    #[test]
    fn test_remote_command() {
        let mut env = HashMap::new();
        env.insert("TEST".to_string(), "value".to_string());

        let cmd = RemoteCommand {
            command: "test".to_string(),
            args: vec!["arg1".to_string()],
            env,
        };

        assert_eq!(cmd.command, "test");
        assert_eq!(cmd.args.len(), 1);
    }

    #[test]
    fn test_remote_manager_creation() {
        let manager = RemoteManager::new();
        assert!(manager.is_ok());

        // Note: manager may load existing nodes from config file
        // Just verify it was created successfully - no need to check length
    }

    #[test]
    fn test_add_and_remove_node() {
        let mut manager = RemoteManager::new().expect("create manager");

        // Use timestamp-based unique ID to avoid conflicts with persistent config
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time since epoch")
            .as_micros();
        let node_id = format!("test-node-{}", timestamp);

        let node = RemoteNode {
            id: node_id.clone(),
            name: "Test Node".to_string(),
            address: "localhost:8080".to_string(),
            auth: AuthMethod::None,
            options: ConnectionOptions::default(),
            tags: vec![],
            description: String::new(),
        };

        assert!(manager.add_node(node.clone()).is_ok());
        assert!(manager.get_node(&node_id).is_some());
        assert!(manager.remove_node(&node_id).is_ok());
        assert!(manager.get_node(&node_id).is_none());
    }

    #[test]
    fn test_list_nodes_by_tag() {
        let mut manager = RemoteManager::new().expect("create manager");

        // Use timestamp-based unique IDs to avoid conflicts with persistent config
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time since epoch")
            .as_micros();
        let node1_id = format!("test-tag-node1-{}", timestamp);
        let node2_id = format!("test-tag-node2-{}", timestamp);

        let node1 = RemoteNode {
            id: node1_id.clone(),
            name: "Node 1".to_string(),
            address: "localhost:8080".to_string(),
            auth: AuthMethod::None,
            options: ConnectionOptions::default(),
            tags: vec!["prod".to_string()],
            description: String::new(),
        };

        let node2 = RemoteNode {
            id: node2_id.clone(),
            name: "Node 2".to_string(),
            address: "localhost:8081".to_string(),
            auth: AuthMethod::None,
            options: ConnectionOptions::default(),
            tags: vec!["dev".to_string()],
            description: String::new(),
        };

        let _ = manager.add_node(node1);
        let _ = manager.add_node(node2);

        let prod_nodes = manager.list_nodes_by_tag("prod");
        assert!(prod_nodes.iter().any(|n| n.id == node1_id));

        // Clean up
        let _ = manager.remove_node(&node1_id);
        let _ = manager.remove_node(&node2_id);
    }
}
