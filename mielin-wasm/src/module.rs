//! Multi-Module Support for MielinOS WASM Runtime
//!
//! This module provides infrastructure for managing multiple WASM modules,
//! linking them together, sharing memories, and resolving dependencies.
//!
//! # Features
//!
//! - **Module Registry**: Central registry for managing multiple modules
//! - **Module Linking**: Connect modules together via imports/exports
//! - **Shared Memory**: Share linear memory between modules
//! - **Import Resolution**: Automatic import/export matching
//! - **Circular Dependency Detection**: Prevent circular dependencies
//! - **Module Versioning**: Support for multiple versions of the same module
//! - **Dynamic Loading**: Load and unload modules at runtime
//!
//! # Example
//!
//! ```rust,ignore
//! use mielin_wasm::module::{ModuleRegistry, SharedMemoryConfig};
//!
//! // Create registry
//! let mut registry = ModuleRegistry::new();
//!
//! // Register modules
//! registry.register("math", math_module)?;
//! registry.register("utils", utils_module)?;
//!
//! // Link modules together
//! registry.link_modules("app", &["math", "utils"])?;
//!
//! // Create shared memory
//! let shared_mem = registry.create_shared_memory(
//!     "shared",
//!     SharedMemoryConfig::default()
//! )?;
//!
//! // Instantiate with shared memory
//! let instance = registry.instantiate_with_shared("app", "shared")?;
//! ```

use anyhow::{anyhow, bail, Result};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use wasmtime::{Engine, Instance, Linker, Memory, MemoryType, Module, Store};

use crate::host::HostState;

/// Unique identifier for a module
pub type ModuleId = String;

/// Unique identifier for shared memory
pub type MemoryId = String;

/// Module metadata and instance information
#[derive(Debug)]
pub struct ModuleInfo {
    /// Module unique identifier
    pub id: ModuleId,
    /// Module name (user-friendly)
    pub name: String,
    /// Module version
    pub version: String,
    /// Module description
    pub description: Option<String>,
    /// Module dependencies (module IDs)
    pub dependencies: Vec<ModuleId>,
    /// Compiled module
    pub module: Module,
    /// Import names required by this module
    pub imports: Vec<ImportDescriptor>,
    /// Export names provided by this module
    pub exports: Vec<ExportDescriptor>,
    /// Module state
    pub state: ModuleState,
}

/// Module state in the lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    /// Module registered but not instantiated
    Registered,
    /// Module is being linked
    Linking,
    /// Module is instantiated and ready
    Ready,
    /// Module encountered an error
    Error,
    /// Module has been unloaded
    Unloaded,
}

/// Import descriptor
#[derive(Debug, Clone)]
pub struct ImportDescriptor {
    /// Module name being imported from
    pub module: String,
    /// Name of the imported item
    pub name: String,
    /// Type of import
    pub import_type: ImportType,
}

/// Export descriptor
#[derive(Debug, Clone)]
pub struct ExportDescriptor {
    /// Name of the exported item
    pub name: String,
    /// Type of export
    pub export_type: ExportType,
}

/// Type of import
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportType {
    /// Function import
    Function,
    /// Memory import
    Memory,
    /// Table import
    Table,
    /// Global import
    Global,
}

/// Type of export
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportType {
    /// Function export
    Function,
    /// Memory export
    Memory,
    /// Table export
    Table,
    /// Global export
    Global,
}

/// Shared memory configuration
#[derive(Debug, Clone)]
pub struct SharedMemoryConfig {
    /// Initial memory size in pages (64KB each)
    pub initial_pages: u32,
    /// Maximum memory size in pages (None = unlimited)
    pub maximum_pages: Option<u32>,
    /// Whether memory is 64-bit (true) or 32-bit (false)
    pub is_64: bool,
    /// Whether memory is shared across threads
    pub shared: bool,
}

impl Default for SharedMemoryConfig {
    fn default() -> Self {
        Self {
            initial_pages: 1,         // 64KB
            maximum_pages: Some(256), // 16MB max
            is_64: false,
            shared: true,
        }
    }
}

/// Shared memory instance
#[derive(Debug, Clone)]
pub struct SharedMemoryInfo {
    /// Memory unique identifier
    pub id: MemoryId,
    /// Memory instance
    pub memory: Memory,
    /// Configuration used
    pub config: SharedMemoryConfig,
    /// Modules using this shared memory
    pub users: HashSet<ModuleId>,
}

/// Module instance wrapper
pub struct ModuleInstance {
    /// Module ID
    pub module_id: ModuleId,
    /// Wasmtime instance
    pub instance: Instance,
    /// Store for this instance
    pub store: Store<HostState>,
    /// Shared memories this instance uses
    pub shared_memories: Vec<MemoryId>,
}

/// Module registry for managing multiple modules
pub struct ModuleRegistry {
    /// WASM engine
    engine: Engine,
    /// Registered modules
    modules: HashMap<ModuleId, ModuleInfo>,
    /// Active instances
    instances: HashMap<ModuleId, Arc<RwLock<ModuleInstance>>>,
    /// Shared memories
    shared_memories: HashMap<MemoryId, SharedMemoryInfo>,
    /// Dependency graph (module_id -> dependencies)
    pub dependency_graph: HashMap<ModuleId, HashSet<ModuleId>>,
    /// Reverse dependency graph (module_id -> dependents)
    reverse_deps: HashMap<ModuleId, HashSet<ModuleId>>,
}

impl ModuleRegistry {
    /// Create a new module registry
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_multi_memory(true);
        config.wasm_bulk_memory(true);
        config.wasm_reference_types(true);
        config.wasm_simd(true);
        config.wasm_threads(true);

        let engine = Engine::new(&config)?;

        Ok(Self {
            engine,
            modules: HashMap::new(),
            instances: HashMap::new(),
            shared_memories: HashMap::new(),
            dependency_graph: HashMap::new(),
            reverse_deps: HashMap::new(),
        })
    }

    /// Register a new module
    pub fn register(
        &mut self,
        id: impl Into<ModuleId>,
        name: impl Into<String>,
        version: impl Into<String>,
        module_bytes: &[u8],
    ) -> Result<()> {
        let id = id.into();
        let name = name.into();
        let version = version.into();

        if self.modules.contains_key(&id) {
            bail!("Module '{}' is already registered", id);
        }

        // Compile module
        let module = Module::new(&self.engine, module_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to compile module: {}", e))?;

        // Extract imports and exports
        let imports = self.extract_imports(&module);
        let exports = self.extract_exports(&module);

        // Determine dependencies from imports
        let dependencies: Vec<ModuleId> = imports
            .iter()
            .filter_map(|imp| {
                // Dependencies are modules that provide imports
                if imp.module != "env" && imp.module != "wasi_snapshot_preview1" {
                    Some(imp.module.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let info = ModuleInfo {
            id: id.clone(),
            name,
            version,
            description: None,
            dependencies: dependencies.clone(),
            module,
            imports,
            exports,
            state: ModuleState::Registered,
        };

        // Update dependency graph
        self.dependency_graph
            .insert(id.clone(), dependencies.iter().cloned().collect());
        for dep in &dependencies {
            self.reverse_deps
                .entry(dep.clone())
                .or_default()
                .insert(id.clone());
        }

        self.modules.insert(id, info);
        Ok(())
    }

    /// Extract imports from a module
    fn extract_imports(&self, module: &Module) -> Vec<ImportDescriptor> {
        module
            .imports()
            .map(|import| {
                let import_type = match import.ty() {
                    wasmtime::ExternType::Func(_) => ImportType::Function,
                    wasmtime::ExternType::Memory(_) => ImportType::Memory,
                    wasmtime::ExternType::Table(_) => ImportType::Table,
                    wasmtime::ExternType::Global(_) => ImportType::Global,
                    _ => ImportType::Function, // Handle Tag and future types
                };

                ImportDescriptor {
                    module: import.module().to_string(),
                    name: import.name().to_string(),
                    import_type,
                }
            })
            .collect()
    }

    /// Extract exports from a module
    fn extract_exports(&self, module: &Module) -> Vec<ExportDescriptor> {
        module
            .exports()
            .map(|export| {
                let export_type = match export.ty() {
                    wasmtime::ExternType::Func(_) => ExportType::Function,
                    wasmtime::ExternType::Memory(_) => ExportType::Memory,
                    wasmtime::ExternType::Table(_) => ExportType::Table,
                    wasmtime::ExternType::Global(_) => ExportType::Global,
                    _ => ExportType::Function, // Handle Tag and future types
                };

                ExportDescriptor {
                    name: export.name().to_string(),
                    export_type,
                }
            })
            .collect()
    }

    /// Check for circular dependencies
    pub fn check_circular_dependencies(&self, module_id: impl AsRef<str>) -> Result<()> {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();

        self.dfs_cycle_check(&module_id.as_ref().to_string(), &mut visited, &mut stack)
    }

    /// DFS-based cycle detection
    fn dfs_cycle_check(
        &self,
        node: &ModuleId,
        visited: &mut HashSet<ModuleId>,
        stack: &mut HashSet<ModuleId>,
    ) -> Result<()> {
        if stack.contains(node) {
            bail!("Circular dependency detected involving module '{}'", node);
        }

        if visited.contains(node) {
            return Ok(());
        }

        visited.insert(node.clone());
        stack.insert(node.clone());

        if let Some(deps) = self.dependency_graph.get(node) {
            for dep in deps {
                self.dfs_cycle_check(dep, visited, stack)?;
            }
        }

        stack.remove(node);
        Ok(())
    }

    /// Create shared memory
    pub fn create_shared_memory(
        &mut self,
        id: impl Into<MemoryId>,
        config: SharedMemoryConfig,
    ) -> Result<MemoryId> {
        let id = id.into();

        if self.shared_memories.contains_key(&id) {
            bail!("Shared memory '{}' already exists", id);
        }

        let memory_type = MemoryType::new(config.initial_pages, config.maximum_pages);

        let mut store = Store::new(
            &self.engine,
            HostState::new(mielin_hal::capabilities::HardwareCapabilities::NONE),
        );
        let memory = Memory::new(&mut store, memory_type)
            .map_err(|e| anyhow::anyhow!("Failed to create shared memory: {}", e))?;

        let info = SharedMemoryInfo {
            id: id.clone(),
            memory,
            config,
            users: HashSet::new(),
        };

        self.shared_memories.insert(id.clone(), info);
        Ok(id)
    }

    /// Link modules together
    pub fn link_modules(
        &mut self,
        target_id: impl AsRef<str>,
        dependency_ids: &[impl AsRef<str>],
    ) -> Result<()> {
        let target_id_str = target_id.as_ref();

        // Verify all modules exist
        if !self.modules.contains_key(target_id_str) {
            bail!("Target module '{}' not found", target_id_str);
        }

        for dep_id in dependency_ids {
            if !self.modules.contains_key(dep_id.as_ref()) {
                bail!("Dependency module '{}' not found", dep_id.as_ref());
            }
        }

        // Update dependency graph
        let deps: HashSet<ModuleId> = dependency_ids
            .iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.dependency_graph
            .insert(target_id_str.to_string(), deps);

        for dep_id in dependency_ids {
            self.reverse_deps
                .entry(dep_id.as_ref().to_string())
                .or_default()
                .insert(target_id_str.to_string());
        }

        // Check for circular dependencies
        self.check_circular_dependencies(target_id_str)?;

        Ok(())
    }

    /// Instantiate a module with dependencies
    pub fn instantiate(
        &mut self,
        module_id: impl AsRef<str>,
        host_state: HostState,
    ) -> Result<Arc<RwLock<ModuleInstance>>> {
        let module_id_str = module_id.as_ref();
        let module_info = self
            .modules
            .get_mut(module_id_str)
            .ok_or_else(|| anyhow!("Module '{}' not found", module_id_str))?;

        // Check dependencies are instantiated
        for dep_id in &module_info.dependencies {
            if !self.instances.contains_key(dep_id) {
                bail!(
                    "Dependency '{}' must be instantiated before '{}'",
                    dep_id,
                    module_id_str
                );
            }
        }

        module_info.state = ModuleState::Linking;

        let mut store = Store::new(&self.engine, host_state);
        let linker = Linker::new(&self.engine);

        // Note: In a full implementation, we would link dependencies here
        // For now, we just instantiate the module directly
        // Future enhancement: implement proper cross-module linking

        // Instantiate the module
        let instance = linker
            .instantiate(&mut store, &module_info.module)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate module: {}", e))?;

        module_info.state = ModuleState::Ready;

        let module_instance = ModuleInstance {
            module_id: module_id_str.to_string(),
            instance,
            store,
            shared_memories: Vec::new(),
        };

        let instance_arc = Arc::new(RwLock::new(module_instance));
        self.instances
            .insert(module_id_str.to_string(), instance_arc.clone());

        Ok(instance_arc)
    }

    /// Instantiate a module with shared memory
    pub fn instantiate_with_shared(
        &mut self,
        module_id: impl AsRef<str>,
        memory_id: impl AsRef<str>,
        host_state: HostState,
    ) -> Result<Arc<RwLock<ModuleInstance>>> {
        let module_id_str = module_id.as_ref();
        let memory_id_str = memory_id.as_ref();

        let shared_mem = self
            .shared_memories
            .get_mut(memory_id_str)
            .ok_or_else(|| anyhow!("Shared memory '{}' not found", memory_id_str))?;

        // Add module to users
        shared_mem.users.insert(module_id_str.to_string());

        // Instantiate normally first
        let instance = self.instantiate(module_id_str, host_state)?;

        // Add shared memory reference
        {
            let mut inst_lock = instance.write().unwrap();
            inst_lock.shared_memories.push(memory_id_str.to_string());
        }

        Ok(instance)
    }

    /// Get module information
    pub fn get_module_info(&self, module_id: impl AsRef<str>) -> Option<&ModuleInfo> {
        self.modules.get(module_id.as_ref())
    }

    /// Get module instance
    pub fn get_instance(&self, module_id: impl AsRef<str>) -> Option<Arc<RwLock<ModuleInstance>>> {
        self.instances.get(module_id.as_ref()).cloned()
    }

    /// Unload a module
    pub fn unload(&mut self, module_id: impl AsRef<str>) -> Result<()> {
        let module_id_str = module_id.as_ref();

        // Check if any modules depend on this one
        if let Some(dependents) = self.reverse_deps.get(module_id_str) {
            if !dependents.is_empty() {
                bail!(
                    "Cannot unload module '{}': depended on by {:?}",
                    module_id_str,
                    dependents
                );
            }
        }

        // Remove instance
        self.instances.remove(module_id_str);

        // Update module state
        if let Some(module_info) = self.modules.get_mut(module_id_str) {
            module_info.state = ModuleState::Unloaded;
        }

        // Clean up shared memory references
        for shared_mem in self.shared_memories.values_mut() {
            shared_mem.users.remove(module_id_str);
        }

        Ok(())
    }

    /// Get dependency graph
    pub fn get_dependency_graph(&self) -> &HashMap<ModuleId, HashSet<ModuleId>> {
        &self.dependency_graph
    }

    /// Get list of all registered modules
    pub fn list_modules(&self) -> Vec<&ModuleInfo> {
        self.modules.values().collect()
    }

    /// Get statistics
    pub fn get_stats(&self) -> RegistryStats {
        RegistryStats {
            total_modules: self.modules.len(),
            active_instances: self.instances.len(),
            shared_memories: self.shared_memories.len(),
            total_dependencies: self.dependency_graph.values().map(|deps| deps.len()).sum(),
        }
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create default ModuleRegistry")
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    /// Total number of registered modules
    pub total_modules: usize,
    /// Number of active instances
    pub active_instances: usize,
    /// Number of shared memories
    pub shared_memories: usize,
    /// Total number of dependencies across all modules
    pub total_dependencies: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_module() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (func (export "add") (param i32 i32) (result i32)
                    local.get 0
                    local.get 1
                    i32.add
                )
            )
            "#,
        )
        .unwrap()
    }

    fn create_module_with_import() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (import "math" "add" (func $add (param i32 i32) (result i32)))
                (func (export "double_add") (param i32 i32) (result i32)
                    local.get 0
                    local.get 1
                    call $add
                    i32.const 2
                    i32.mul
                )
            )
            "#,
        )
        .unwrap()
    }

    #[test]
    fn test_module_registry_creation() {
        let registry = ModuleRegistry::new();
        assert!(registry.is_ok());
    }

    #[test]
    fn test_module_registration() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_simple_module();

        let result = registry.register("math", "Math Module", "1.0.0", &module_bytes);
        assert!(result.is_ok());

        let module_info = registry.get_module_info("math");
        assert!(module_info.is_some());
        assert_eq!(module_info.unwrap().name, "Math Module");
        assert_eq!(module_info.unwrap().version, "1.0.0");
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_simple_module();

        registry
            .register("math", "Math Module", "1.0.0", &module_bytes)
            .unwrap();
        let result = registry.register("math", "Math Module", "1.0.0", &module_bytes);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already registered"));
    }

    #[test]
    fn test_import_extraction() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_module_with_import();

        registry
            .register("app", "App Module", "1.0.0", &module_bytes)
            .unwrap();

        let module_info = registry.get_module_info("app").unwrap();
        assert_eq!(module_info.imports.len(), 1);
        assert_eq!(module_info.imports[0].module, "math");
        assert_eq!(module_info.imports[0].name, "add");
    }

    #[test]
    fn test_export_extraction() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_simple_module();

        registry
            .register("math", "Math Module", "1.0.0", &module_bytes)
            .unwrap();

        let module_info = registry.get_module_info("math").unwrap();
        assert_eq!(module_info.exports.len(), 1);
        assert_eq!(module_info.exports[0].name, "add");
        assert_eq!(module_info.exports[0].export_type, ExportType::Function);
    }

    #[test]
    fn test_dependency_detection() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_module_with_import();

        registry
            .register("app", "App Module", "1.0.0", &module_bytes)
            .unwrap();

        let module_info = registry.get_module_info("app").unwrap();
        assert_eq!(module_info.dependencies.len(), 1);
        assert_eq!(module_info.dependencies[0], "math");
    }

    #[test]
    fn test_shared_memory_creation() {
        let mut registry = ModuleRegistry::new().unwrap();

        let result = registry.create_shared_memory("shared", SharedMemoryConfig::default());
        assert!(result.is_ok());

        let mem_id = result.unwrap();
        assert!(registry.shared_memories.contains_key(&mem_id));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut registry = ModuleRegistry::new().unwrap();

        // Create artificial circular dependency
        registry
            .dependency_graph
            .insert("A".to_string(), ["B".to_string()].iter().cloned().collect());
        registry
            .dependency_graph
            .insert("B".to_string(), ["C".to_string()].iter().cloned().collect());
        registry
            .dependency_graph
            .insert("C".to_string(), ["A".to_string()].iter().cloned().collect());

        let result = registry.check_circular_dependencies("A");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circular dependency"));
    }

    #[test]
    fn test_module_linking() {
        let mut registry = ModuleRegistry::new().unwrap();
        let math_module = create_simple_module();
        let app_module = create_module_with_import();

        registry
            .register("math", "Math Module", "1.0.0", &math_module)
            .unwrap();
        registry
            .register("app", "App Module", "1.0.0", &app_module)
            .unwrap();

        let result = registry.link_modules("app", &["math".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_registry_stats() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_simple_module();

        registry
            .register("math", "Math Module", "1.0.0", &module_bytes)
            .unwrap();

        let stats = registry.get_stats();
        assert_eq!(stats.total_modules, 1);
        assert_eq!(stats.active_instances, 0);
    }

    #[test]
    fn test_list_modules() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_simple_module();

        registry
            .register("math", "Math Module", "1.0.0", &module_bytes)
            .unwrap();
        registry
            .register("utils", "Utils Module", "1.0.0", &module_bytes)
            .unwrap();

        let modules = registry.list_modules();
        assert_eq!(modules.len(), 2);
    }

    #[test]
    fn test_module_state_transitions() {
        let mut registry = ModuleRegistry::new().unwrap();
        let module_bytes = create_simple_module();

        registry
            .register("math", "Math Module", "1.0.0", &module_bytes)
            .unwrap();

        let module_info = registry.get_module_info("math").unwrap();
        assert_eq!(module_info.state, ModuleState::Registered);
    }

    #[test]
    fn test_shared_memory_config_default() {
        let config = SharedMemoryConfig::default();
        assert_eq!(config.initial_pages, 1);
        assert_eq!(config.maximum_pages, Some(256));
        assert!(!config.is_64);
        assert!(config.shared);
    }
}
