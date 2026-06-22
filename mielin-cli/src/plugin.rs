//! Plugin System for MielinCTL
//!
//! Provides a secure, extensible plugin architecture using WASM modules.
//! Plugins can add custom commands, extend functionality, and integrate
//! with external tools.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Plugin metadata describing the plugin's capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (must be unique)
    pub name: String,
    /// Plugin version (semver)
    pub version: String,
    /// Short description
    pub description: String,
    /// Author information
    pub author: String,
    /// License identifier
    pub license: String,
    /// Commands provided by this plugin
    pub commands: Vec<PluginCommand>,
    /// Plugin dependencies (other plugins)
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Minimum MielinCTL version required
    #[serde(default)]
    pub min_version: Option<String>,
}

/// Command provided by a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    /// Command name (will be accessible as `mielinctl plugin <name> <command>`)
    pub name: String,
    /// Command description
    pub description: String,
    /// Command aliases
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Arguments specification
    #[serde(default)]
    pub arguments: Vec<PluginArgument>,
}

/// Argument specification for plugin commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginArgument {
    /// Argument name
    pub name: String,
    /// Argument description
    pub description: String,
    /// Whether argument is required
    #[serde(default)]
    pub required: bool,
    /// Default value if not provided
    #[serde(default)]
    pub default: Option<String>,
}

/// Plugin execution context passed to plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContext {
    /// Command name being executed
    pub command: String,
    /// Arguments provided to the command
    pub arguments: HashMap<String, String>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Current working directory
    pub working_dir: String,
    /// MielinCTL version
    pub cli_version: String,
}

/// Plugin execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResult {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// JSON data for structured output
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// Represents a loaded plugin
#[derive(Debug, Clone)]
pub struct Plugin {
    /// Plugin metadata
    pub metadata: PluginMetadata,
    /// Path to the WASM module
    pub wasm_path: PathBuf,
    /// Plugin enabled state
    pub enabled: bool,
}

impl Plugin {
    /// Load a plugin from a directory
    pub fn load_from_dir(path: &Path) -> Result<Self> {
        let metadata_path = path.join("plugin.toml");
        let wasm_path = path.join("plugin.wasm");

        if !metadata_path.exists() {
            anyhow::bail!("Plugin metadata file not found: {:?}", metadata_path);
        }

        if !wasm_path.exists() {
            anyhow::bail!("Plugin WASM module not found: {:?}", wasm_path);
        }

        let metadata_content =
            fs::read_to_string(&metadata_path).context("Failed to read plugin metadata")?;

        let metadata: PluginMetadata =
            toml::from_str(&metadata_content).context("Failed to parse plugin metadata")?;

        // Validate metadata
        if metadata.name.is_empty() {
            anyhow::bail!("Plugin name cannot be empty");
        }

        if metadata.version.is_empty() {
            anyhow::bail!("Plugin version cannot be empty");
        }

        Ok(Plugin {
            metadata,
            wasm_path,
            enabled: true,
        })
    }

    /// Execute a plugin command
    pub async fn execute(&self, context: PluginContext) -> Result<PluginResult> {
        debug!(
            "Executing plugin command: {} - {}",
            self.metadata.name, context.command
        );

        // Serialize context to JSON for passing to WASM
        let context_json =
            serde_json::to_string(&context).context("Failed to serialize plugin context")?;

        // Load and execute WASM module using mielin-wasm
        let result = self.execute_wasm(&context_json).await?;

        Ok(result)
    }

    /// Execute WASM module using mielin-wasm
    async fn execute_wasm(&self, context_json: &str) -> Result<PluginResult> {
        use mielin_wasm::executor::WasmExecutor;

        debug!("Loading WASM module for plugin: {}", self.metadata.name);

        // Read the WASM module from file
        let wasm_bytes = fs::read(&self.wasm_path).context("Failed to read WASM module file")?;

        // Create WASM executor
        let executor = WasmExecutor::new()
            .map_err(|e| anyhow::anyhow!("Failed to create WASM executor: {}", e))?;

        // Compile the module
        let module = executor
            .compile_module(&wasm_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to compile WASM module: {}", e))?;

        // Instantiate the module
        let (instance, mut store) = executor
            .instantiate(
                &module,
                mielin_hal::capabilities::HardwareCapabilities::NONE,
            )
            .map_err(|e| anyhow::anyhow!("Failed to instantiate WASM module: {}", e))?;

        // Try to find and call the main entry point function
        // Common entry points: "main", "_start", "run", or the command name
        let possible_entry_points = vec!["main", "_start", "run", &self.metadata.name];

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        // Try each possible entry point
        let mut executed = false;
        for entry_point in possible_entry_points {
            if let Some(func) = instance.get_func(&mut store, entry_point) {
                debug!("Found entry point: {}", entry_point);

                // For now, we'll call the function without arguments
                // In a full implementation, we would pass the context_json through memory
                match func.call(&mut store, &[], &mut []) {
                    Ok(_) => {
                        stdout = format!(
                            "Plugin {} executed successfully\nContext: {}",
                            self.metadata.name, context_json
                        );
                        executed = true;
                        break;
                    }
                    Err(e) => {
                        stderr = format!("Execution error: {}", e);
                        exit_code = 1;
                        executed = true;
                        break;
                    }
                }
            }
        }

        if !executed {
            // No entry point found, but module loaded successfully
            stdout = format!(
                "Plugin {} loaded successfully but no entry point found\nContext: {}",
                self.metadata.name, context_json
            );
        }

        Ok(PluginResult {
            exit_code,
            stdout,
            stderr,
            data: None,
        })
    }
}

/// Plugin manager for loading and managing plugins
pub struct PluginManager {
    /// Loaded plugins indexed by name
    plugins: HashMap<String, Plugin>,
    /// Plugin directory path
    plugin_dir: PathBuf,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Result<Self> {
        let plugin_dir = Self::get_plugin_dir()?;

        // Create plugin directory if it doesn't exist
        if !plugin_dir.exists() {
            fs::create_dir_all(&plugin_dir).context("Failed to create plugin directory")?;
            info!("Created plugin directory: {:?}", plugin_dir);
        }

        Ok(PluginManager {
            plugins: HashMap::new(),
            plugin_dir,
        })
    }

    /// Get the default plugin directory
    pub fn get_plugin_dir() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?;
        Ok(config_dir.join("mielin").join("plugins"))
    }

    /// Discover and load all plugins from the plugin directory
    pub fn discover_plugins(&mut self) -> Result<usize> {
        debug!("Discovering plugins in {:?}", self.plugin_dir);

        let entries = fs::read_dir(&self.plugin_dir).context("Failed to read plugin directory")?;

        let mut loaded_count = 0;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            match Plugin::load_from_dir(&path) {
                Ok(plugin) => {
                    let name = plugin.metadata.name.clone();
                    info!("Loaded plugin: {} v{}", name, plugin.metadata.version);
                    self.plugins.insert(name, plugin);
                    loaded_count += 1;
                }
                Err(e) => {
                    warn!("Failed to load plugin from {:?}: {}", path, e);
                }
            }
        }

        info!("Discovered {} plugins", loaded_count);
        Ok(loaded_count)
    }

    /// Get a plugin by name
    pub fn get_plugin(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name)
    }

    /// List all loaded plugins
    pub fn list_plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().collect()
    }

    /// Execute a plugin command
    pub async fn execute_command(
        &self,
        plugin_name: &str,
        command_name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<PluginResult> {
        let plugin = self
            .get_plugin(plugin_name)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_name))?;

        if !plugin.enabled {
            anyhow::bail!("Plugin is disabled: {}", plugin_name);
        }

        // Verify command exists
        let command_exists =
            plugin.metadata.commands.iter().any(|cmd| {
                cmd.name == command_name || cmd.aliases.contains(&command_name.to_string())
            });

        if !command_exists {
            anyhow::bail!("Command not found in plugin: {}", command_name);
        }

        // Build execution context
        let context = PluginContext {
            command: command_name.to_string(),
            arguments,
            environment: std::env::vars().collect(),
            working_dir: std::env::current_dir()
                .context("Failed to get current directory")?
                .to_string_lossy()
                .to_string(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        plugin.execute(context).await
    }

    /// Install a plugin from a path
    pub fn install_plugin(&mut self, source_path: &Path) -> Result<()> {
        let plugin =
            Plugin::load_from_dir(source_path).context("Failed to load plugin from source")?;

        let dest_path = self.plugin_dir.join(&plugin.metadata.name);

        if dest_path.exists() {
            anyhow::bail!("Plugin already installed: {}", plugin.metadata.name);
        }

        // Copy plugin directory
        fs::create_dir_all(&dest_path).context("Failed to create plugin directory")?;

        fs::copy(
            source_path.join("plugin.toml"),
            dest_path.join("plugin.toml"),
        )
        .context("Failed to copy plugin metadata")?;

        fs::copy(
            source_path.join("plugin.wasm"),
            dest_path.join("plugin.wasm"),
        )
        .context("Failed to copy plugin WASM")?;

        info!(
            "Installed plugin: {} v{}",
            plugin.metadata.name, plugin.metadata.version
        );

        // Reload plugins
        self.discover_plugins()?;

        Ok(())
    }

    /// Uninstall a plugin
    pub fn uninstall_plugin(&mut self, name: &str) -> Result<()> {
        if !self.plugins.contains_key(name) {
            anyhow::bail!("Plugin not found: {}", name);
        }

        let plugin_path = self.plugin_dir.join(name);
        if plugin_path.exists() {
            fs::remove_dir_all(&plugin_path).context("Failed to remove plugin directory")?;
        }

        self.plugins.remove(name);
        info!("Uninstalled plugin: {}", name);

        Ok(())
    }

    /// Enable a plugin
    pub fn enable_plugin(&mut self, name: &str) -> Result<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", name))?;

        plugin.enabled = true;
        info!("Enabled plugin: {}", name);
        Ok(())
    }

    /// Disable a plugin
    pub fn disable_plugin(&mut self, name: &str) -> Result<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", name))?;

        plugin.enabled = false;
        info!("Disabled plugin: {}", name);
        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new().expect("Failed to create plugin manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_plugin_metadata_serialization() {
        let metadata = PluginMetadata {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            commands: vec![PluginCommand {
                name: "hello".to_string(),
                description: "Say hello".to_string(),
                aliases: vec!["hi".to_string()],
                arguments: vec![],
            }],
            dependencies: vec![],
            min_version: Some("0.1.0".to_string()),
        };

        let toml_str = toml::to_string(&metadata).unwrap();
        let deserialized: PluginMetadata = toml::from_str(&toml_str).unwrap();

        assert_eq!(metadata.name, deserialized.name);
        assert_eq!(metadata.version, deserialized.version);
        assert_eq!(metadata.commands.len(), deserialized.commands.len());
    }

    #[test]
    fn test_plugin_context_serialization() {
        let mut arguments = HashMap::new();
        arguments.insert("name".to_string(), "world".to_string());

        let mut environment = HashMap::new();
        environment.insert("PATH".to_string(), "/usr/bin".to_string());

        let context = PluginContext {
            command: "hello".to_string(),
            arguments,
            environment,
            working_dir: std::env::temp_dir().to_string_lossy().into_owned(),
            cli_version: "0.1.0".to_string(),
        };

        let json_str = serde_json::to_string(&context).unwrap();
        let deserialized: PluginContext = serde_json::from_str(&json_str).unwrap();

        assert_eq!(context.command, deserialized.command);
        assert_eq!(context.working_dir, deserialized.working_dir);
    }

    #[test]
    fn test_plugin_result_serialization() {
        let result = PluginResult {
            exit_code: 0,
            stdout: "Hello, world!".to_string(),
            stderr: String::new(),
            data: Some(serde_json::json!({"status": "success"})),
        };

        let json_str = serde_json::to_string(&result).unwrap();
        let deserialized: PluginResult = serde_json::from_str(&json_str).unwrap();

        assert_eq!(result.exit_code, deserialized.exit_code);
        assert_eq!(result.stdout, deserialized.stdout);
        assert!(deserialized.data.is_some());
    }

    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert!(manager.is_ok());

        let manager = manager.unwrap();
        assert_eq!(manager.plugins.len(), 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_plugin_load_from_invalid_dir() {
        let temp_dir = env::temp_dir().join("test_invalid_plugin");
        let _ = fs::create_dir_all(&temp_dir);

        let result = Plugin::load_from_dir(&temp_dir);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_plugin_argument_validation() {
        let arg = PluginArgument {
            name: "input".to_string(),
            description: "Input file".to_string(),
            required: true,
            default: None,
        };

        assert_eq!(arg.name, "input");
        assert!(arg.required);
        assert!(arg.default.is_none());
    }
}
