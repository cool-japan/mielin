//! Scripting Support for MielinCTL
//!
//! Provides scripting capabilities using Rhai scripting language.
//! Scripts can automate CLI operations, perform batch tasks, and
//! create reusable workflows.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Script metadata from script headers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptMetadata {
    /// Script name
    pub name: String,
    /// Script version
    pub version: String,
    /// Description
    pub description: String,
    /// Author
    pub author: Option<String>,
    /// Required CLI version (semver)
    pub required_version: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Script execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptContext {
    /// Script arguments
    pub args: HashMap<String, String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Working directory
    pub working_dir: String,
    /// CLI version
    pub cli_version: String,
}

/// Script execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptResult {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Return value from script
    pub return_value: Option<serde_json::Value>,
}

/// Script engine for executing Rhai scripts
pub struct ScriptEngine {
    /// Scripts directory
    scripts_dir: PathBuf,
    /// Loaded scripts cache
    scripts: HashMap<String, Script>,
}

/// Represents a loaded script
#[derive(Debug, Clone)]
pub struct Script {
    /// Script metadata
    pub metadata: ScriptMetadata,
    /// Script content
    pub content: String,
    /// Script file path
    pub path: PathBuf,
}

impl ScriptEngine {
    /// Create a new script engine
    pub fn new() -> Result<Self> {
        let scripts_dir = Self::get_scripts_dir()?;

        // Create scripts directory if it doesn't exist
        if !scripts_dir.exists() {
            fs::create_dir_all(&scripts_dir).context("Failed to create scripts directory")?;
            info!("Created scripts directory: {:?}", scripts_dir);
        }

        Ok(ScriptEngine {
            scripts_dir,
            scripts: HashMap::new(),
        })
    }

    /// Get the default scripts directory
    pub fn get_scripts_dir() -> Result<PathBuf> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Failed to get config directory"))?;
        Ok(config_dir.join("mielin").join("scripts"))
    }

    /// Discover and load all scripts from the scripts directory
    pub fn discover_scripts(&mut self) -> Result<usize> {
        debug!("Discovering scripts in {:?}", self.scripts_dir);

        let entries =
            fs::read_dir(&self.scripts_dir).context("Failed to read scripts directory")?;

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
            if !path.is_file() {
                continue;
            }

            // Only load .rhai files
            if path.extension().and_then(|s| s.to_str()) != Some("rhai") {
                continue;
            }

            match Script::load_from_file(&path) {
                Ok(script) => {
                    let name = script.metadata.name.clone();
                    info!("Loaded script: {} v{}", name, script.metadata.version);
                    self.scripts.insert(name, script);
                    loaded_count += 1;
                }
                Err(e) => {
                    warn!("Failed to load script from {:?}: {}", path, e);
                }
            }
        }

        info!("Discovered {} scripts", loaded_count);
        Ok(loaded_count)
    }

    /// Get a script by name
    pub fn get_script(&self, name: &str) -> Option<&Script> {
        self.scripts.get(name)
    }

    /// List all loaded scripts
    pub fn list_scripts(&self) -> Vec<&Script> {
        self.scripts.values().collect()
    }

    /// Execute a script with given context
    pub fn execute_script(&self, name: &str, context: ScriptContext) -> Result<ScriptResult> {
        let script = self
            .get_script(name)
            .ok_or_else(|| anyhow::anyhow!("Script not found: {}", name))?;

        debug!("Executing script: {}", name);

        let start_time = std::time::Instant::now();

        // Execute script using Rhai engine
        let result = self.execute_rhai_script(script, &context)?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(ScriptResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms,
            return_value: result.return_value,
        })
    }

    /// Execute Rhai script using the Rhai engine
    fn execute_rhai_script(
        &self,
        script: &Script,
        context: &ScriptContext,
    ) -> Result<ScriptResult> {
        debug!("Executing Rhai script: {}", script.metadata.name);

        // Create a new Rhai engine
        let mut engine = rhai::Engine::new();

        // Create output buffers
        let mut stdout_buffer = String::new();
        let mut stderr_buffer = String::new();

        // Register print function to capture stdout
        let stdout_clone = std::sync::Arc::new(std::sync::Mutex::new(stdout_buffer.clone()));
        let stdout_ref = stdout_clone.clone();
        engine.on_print(move |s| {
            if let Ok(mut buf) = stdout_ref.lock() {
                buf.push_str(s);
                buf.push('\n');
            }
        });

        // Register debug function to capture stderr
        let stderr_clone = std::sync::Arc::new(std::sync::Mutex::new(stderr_buffer.clone()));
        let stderr_ref = stderr_clone.clone();
        engine.on_debug(move |s, _src, _pos| {
            if let Ok(mut buf) = stderr_ref.lock() {
                buf.push_str(s);
                buf.push('\n');
            }
        });

        // Create a scope and inject context variables
        let mut scope = rhai::Scope::new();

        // Inject args as a map
        let mut args_map = rhai::Map::new();
        for (key, value) in &context.args {
            args_map.insert(key.clone().into(), rhai::Dynamic::from(value.clone()));
        }
        scope.push("args", args_map);

        // Inject environment variables as a map
        let mut env_map = rhai::Map::new();
        for (key, value) in &context.env {
            env_map.insert(key.clone().into(), rhai::Dynamic::from(value.clone()));
        }
        scope.push("env", env_map);

        // Inject other context variables
        scope.push("working_dir", context.working_dir.clone());
        scope.push("cli_version", context.cli_version.clone());

        // Execute the script
        let result = engine.eval_with_scope::<rhai::Dynamic>(&mut scope, &script.content);

        // Extract stdout and stderr from buffers
        stdout_buffer = stdout_clone.lock().map(|b| b.clone()).unwrap_or_default();
        stderr_buffer = stderr_clone.lock().map(|b| b.clone()).unwrap_or_default();

        match result {
            Ok(value) => {
                // Convert Rhai dynamic value to JSON
                let return_value = Self::rhai_to_json(value);

                Ok(ScriptResult {
                    exit_code: 0,
                    stdout: stdout_buffer,
                    stderr: stderr_buffer,
                    duration_ms: 0, // Duration is tracked by the caller
                    return_value: Some(return_value),
                })
            }
            Err(e) => {
                // Script execution failed
                stderr_buffer.push_str(&format!("Script error: {}\n", e));

                Ok(ScriptResult {
                    exit_code: 1,
                    stdout: stdout_buffer,
                    stderr: stderr_buffer,
                    duration_ms: 0,
                    return_value: None,
                })
            }
        }
    }

    /// Convert Rhai Dynamic value to JSON
    fn rhai_to_json(value: rhai::Dynamic) -> serde_json::Value {
        if value.is::<i64>() {
            serde_json::json!(value.as_int().unwrap_or(0))
        } else if value.is::<f64>() {
            serde_json::json!(value.as_float().unwrap_or(0.0))
        } else if value.is::<bool>() {
            serde_json::json!(value.as_bool().unwrap_or(false))
        } else if value.is::<rhai::ImmutableString>() {
            serde_json::json!(value.to_string())
        } else if value.is::<rhai::Map>() {
            let map = value.cast::<rhai::Map>();
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                json_map.insert(k.to_string(), Self::rhai_to_json(v));
            }
            serde_json::Value::Object(json_map)
        } else if value.is::<rhai::Array>() {
            let array = value.cast::<rhai::Array>();
            let json_array: Vec<serde_json::Value> =
                array.into_iter().map(Self::rhai_to_json).collect();
            serde_json::Value::Array(json_array)
        } else if value.is::<()>() {
            serde_json::Value::Null
        } else {
            // Fallback: convert to string
            serde_json::json!(value.to_string())
        }
    }

    /// Install a script from a file
    pub fn install_script(&mut self, source_path: &Path) -> Result<()> {
        let script =
            Script::load_from_file(source_path).context("Failed to load script from source")?;

        let dest_filename = format!("{}.rhai", script.metadata.name);
        let dest_path = self.scripts_dir.join(&dest_filename);

        if dest_path.exists() {
            anyhow::bail!("Script already installed: {}", script.metadata.name);
        }

        // Copy script file
        fs::copy(source_path, &dest_path).context("Failed to copy script file")?;

        info!(
            "Installed script: {} v{}",
            script.metadata.name, script.metadata.version
        );

        // Reload scripts
        self.discover_scripts()?;

        Ok(())
    }

    /// Uninstall a script
    pub fn uninstall_script(&mut self, name: &str) -> Result<()> {
        if !self.scripts.contains_key(name) {
            anyhow::bail!("Script not found: {}", name);
        }

        let script_filename = format!("{}.rhai", name);
        let script_path = self.scripts_dir.join(&script_filename);

        if script_path.exists() {
            fs::remove_file(&script_path).context("Failed to remove script file")?;
        }

        self.scripts.remove(name);
        info!("Uninstalled script: {}", name);

        Ok(())
    }

    /// Create a new script template
    pub fn create_template(&self, name: &str, output_path: &Path) -> Result<()> {
        let template = format!(
            r#"// Script: {}
// Version: 1.0.0
// Description: A new MielinCTL script
// Author: Your Name

// This is a Rhai script for MielinCTL
// You can use Rhai syntax to automate CLI operations

fn main(args) {{
    print("Hello from {}!");
    print("Arguments: " + args);

    // TODO: Add your script logic here

    return {{
        status: "success",
        message: "Script executed successfully"
    }};
}}

// Call main function
main(args)
"#,
            name, name
        );

        fs::write(output_path, template).context("Failed to write script template")?;

        info!("Created script template: {:?}", output_path);
        Ok(())
    }
}

impl Script {
    /// Load a script from a file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("Script file not found: {:?}", path);
        }

        let content = fs::read_to_string(path).context("Failed to read script file")?;

        // Parse metadata from comments
        let metadata = Self::parse_metadata(&content, path)?;

        Ok(Script {
            metadata,
            content,
            path: path.to_path_buf(),
        })
    }

    /// Parse script metadata from comments
    fn parse_metadata(content: &str, path: &Path) -> Result<ScriptMetadata> {
        let mut name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut version = "1.0.0".to_string();
        let mut description = String::new();
        let mut author = None;
        let mut required_version = None;
        let mut tags = Vec::new();

        // Parse comment headers
        for line in content.lines().take(20) {
            let line = line.trim();
            if !line.starts_with("//") {
                continue;
            }

            let line = line.trim_start_matches("//").trim();

            if line.starts_with("Script:") {
                name = line.trim_start_matches("Script:").trim().to_string();
            } else if line.starts_with("Version:") {
                version = line.trim_start_matches("Version:").trim().to_string();
            } else if line.starts_with("Description:") {
                description = line.trim_start_matches("Description:").trim().to_string();
            } else if line.starts_with("Author:") {
                author = Some(line.trim_start_matches("Author:").trim().to_string());
            } else if line.starts_with("RequiredVersion:") {
                required_version = Some(
                    line.trim_start_matches("RequiredVersion:")
                        .trim()
                        .to_string(),
                );
            } else if line.starts_with("Tags:") {
                let tags_str = line.trim_start_matches("Tags:").trim();
                tags = tags_str.split(',').map(|s| s.trim().to_string()).collect();
            }
        }

        Ok(ScriptMetadata {
            name,
            version,
            description,
            author,
            required_version,
            tags,
        })
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new().expect("Failed to create script engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_script_metadata_parsing() {
        let content = r#"
// Script: test-script
// Version: 1.0.0
// Description: A test script
// Author: Test Author
// RequiredVersion: 0.1.0
// Tags: test, demo

fn main() {
    print("Hello");
}
"#;
        let path = PathBuf::from("test.rhai");
        let metadata = Script::parse_metadata(content, &path).unwrap();

        assert_eq!(metadata.name, "test-script");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.description, "A test script");
        assert_eq!(metadata.author, Some("Test Author".to_string()));
        assert_eq!(metadata.tags.len(), 2);
    }

    #[test]
    fn test_script_context_serialization() {
        let mut args = HashMap::new();
        args.insert("key".to_string(), "value".to_string());

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), "/usr/bin".to_string());

        let context = ScriptContext {
            args,
            env,
            working_dir: std::env::temp_dir().to_string_lossy().into_owned(),
            cli_version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&context).unwrap();
        assert!(json.contains("key"));
        assert!(json.contains("value"));
    }

    #[test]
    fn test_script_result() {
        let result = ScriptResult {
            exit_code: 0,
            stdout: "Success".to_string(),
            stderr: String::new(),
            duration_ms: 100,
            return_value: Some(serde_json::json!({"status": "ok"})),
        };

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Success");
        assert!(result.return_value.is_some());
    }

    #[test]
    fn test_script_engine_creation() {
        let engine = ScriptEngine::new();
        assert!(engine.is_ok());

        let engine = engine.unwrap();
        assert_eq!(engine.scripts.len(), 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_create_template() {
        let engine = ScriptEngine::new().unwrap();
        let temp_path = env::temp_dir().join("test_script.rhai");

        let result = engine.create_template("test", &temp_path);
        assert!(result.is_ok());

        // Clean up
        if temp_path.exists() {
            let _ = fs::remove_file(&temp_path);
        }
    }

    #[test]
    fn test_script_metadata_default_values() {
        let content = r#"
fn main() {
    print("Hello");
}
"#;
        let path = PathBuf::from("simple.rhai");
        let metadata = Script::parse_metadata(content, &path).unwrap();

        assert_eq!(metadata.name, "simple");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.description, "");
        assert!(metadata.author.is_none());
    }
}
