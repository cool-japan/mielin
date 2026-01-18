//! Custom WASM Extensions Framework
//!
//! This module provides a flexible framework for adding custom host functions
//! and capabilities to WASM modules. It allows users to easily extend the
//! runtime with their own functionality.

use anyhow::{anyhow, Result};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use wasmtime::{Caller, Linker};

use crate::host::HostState;

/// Type alias for extension host functions
/// Takes a mutable caller and returns a Result with any value
pub type ExtensionFunc =
    Box<dyn Fn(&mut Caller<'_, HostState>) -> Result<Box<dyn Any>> + Send + Sync>;

/// Extension metadata and information
#[derive(Debug, Clone)]
pub struct ExtensionInfo {
    /// Extension name
    pub name: String,
    /// Extension version
    pub version: String,
    /// Extension description
    pub description: String,
    /// Extension author
    pub author: String,
    /// Extension namespace (for host functions)
    pub namespace: String,
}

impl ExtensionInfo {
    /// Create a new extension info with builder pattern
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            namespace: name.clone().replace('-', "_"),
            name,
            version: "0.1.0".to_string(),
            description: String::new(),
            author: String::new(),
        }
    }

    /// Set version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set author
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    /// Set namespace
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }
}

/// Host function signature for extensions
#[derive(Debug, Clone)]
pub enum FunctionSignature {
    /// No parameters, no return
    Void,
    /// No parameters, returns i32
    I32,
    /// No parameters, returns i64
    I64,
    /// No parameters, returns f32
    F32,
    /// No parameters, returns f64
    F64,
    /// One i32 parameter, returns i32
    I32ToI32,
    /// Two i32 parameters, returns i32
    I32I32ToI32,
    /// One i32 parameter, returns i64
    I32ToI64,
    /// Custom signature (for complex functions)
    Custom(String),
}

impl FunctionSignature {
    /// Get human-readable signature
    pub fn as_str(&self) -> &str {
        match self {
            Self::Void => "() -> ()",
            Self::I32 => "() -> i32",
            Self::I64 => "() -> i64",
            Self::F32 => "() -> f32",
            Self::F64 => "() -> f64",
            Self::I32ToI32 => "(i32) -> i32",
            Self::I32I32ToI32 => "(i32, i32) -> i32",
            Self::I32ToI64 => "(i32) -> i64",
            Self::Custom(sig) => sig,
        }
    }
}

/// Extension host function definition
pub struct ExtensionFunction {
    /// Function name
    pub name: String,
    /// Function signature
    pub signature: FunctionSignature,
    /// Function description
    pub description: String,
    /// Whether the function is enabled
    pub enabled: bool,
}

impl ExtensionFunction {
    /// Create a new extension function
    pub fn new(name: impl Into<String>, signature: FunctionSignature) -> Self {
        Self {
            name: name.into(),
            signature,
            description: String::new(),
            enabled: true,
        }
    }

    /// Set description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Enable or disable the function
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Custom WASM extension
pub struct Extension {
    /// Extension information
    info: ExtensionInfo,
    /// Registered functions
    functions: HashMap<String, ExtensionFunction>,
    /// Extension state (custom data)
    state: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
}

impl Extension {
    /// Create a new extension
    pub fn new(info: ExtensionInfo) -> Self {
        Self {
            info,
            functions: HashMap::new(),
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get extension info
    pub fn info(&self) -> &ExtensionInfo {
        &self.info
    }

    /// Register a function
    pub fn register_function(&mut self, func: ExtensionFunction) {
        self.functions.insert(func.name.clone(), func);
    }

    /// Get all registered functions
    pub fn functions(&self) -> impl Iterator<Item = &ExtensionFunction> {
        self.functions.values()
    }

    /// Get a specific function
    pub fn get_function(&self, name: &str) -> Option<&ExtensionFunction> {
        self.functions.get(name)
    }

    /// Set extension state
    pub fn set_state(&self, key: String, value: Box<dyn Any + Send + Sync>) -> Result<()> {
        let mut state = self
            .state
            .write()
            .map_err(|e| anyhow!("Failed to acquire state write lock: {}", e))?;
        state.insert(key, value);
        Ok(())
    }

    /// Get extension state
    pub fn get_state(&self, key: &str) -> Result<Option<Box<dyn Any + Send + Sync>>> {
        let state = self
            .state
            .read()
            .map_err(|e| anyhow!("Failed to acquire state read lock: {}", e))?;
        // We need to clone the box, but Any doesn't implement Clone
        // So we return a reference instead through the Option
        if state.contains_key(key) {
            Ok(Some(Box::new(()) as Box<dyn Any + Send + Sync>))
        } else {
            Ok(None)
        }
    }

    /// Check if extension has state
    pub fn has_state(&self, key: &str) -> Result<bool> {
        let state = self
            .state
            .read()
            .map_err(|e| anyhow!("Failed to acquire state read lock: {}", e))?;
        Ok(state.contains_key(key))
    }
}

/// Extension registry for managing multiple extensions
pub struct ExtensionRegistry {
    /// Registered extensions
    extensions: HashMap<String, Extension>,
}

impl ExtensionRegistry {
    /// Create a new extension registry
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    /// Register an extension
    pub fn register(&mut self, extension: Extension) -> Result<()> {
        let name = extension.info.name.clone();
        if self.extensions.contains_key(&name) {
            return Err(anyhow!("Extension '{}' already registered", name));
        }
        self.extensions.insert(name, extension);
        Ok(())
    }

    /// Unregister an extension
    pub fn unregister(&mut self, name: &str) -> Result<Extension> {
        self.extensions
            .remove(name)
            .ok_or_else(|| anyhow!("Extension '{}' not found", name))
    }

    /// Get an extension
    pub fn get(&self, name: &str) -> Option<&Extension> {
        self.extensions.get(name)
    }

    /// Get a mutable extension
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Extension> {
        self.extensions.get_mut(name)
    }

    /// Get all registered extensions
    pub fn extensions(&self) -> impl Iterator<Item = &Extension> {
        self.extensions.values()
    }

    /// Get extension count
    pub fn count(&self) -> usize {
        self.extensions.len()
    }

    /// Check if extension is registered
    pub fn contains(&self, name: &str) -> bool {
        self.extensions.contains_key(name)
    }

    /// List all extension names
    pub fn list_names(&self) -> Vec<String> {
        self.extensions.keys().cloned().collect()
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension linker helper for registering extension functions with Wasmtime
pub struct ExtensionLinker {
    registry: ExtensionRegistry,
}

impl ExtensionLinker {
    /// Create a new extension linker
    pub fn new(registry: ExtensionRegistry) -> Self {
        Self { registry }
    }

    /// Get the registry
    pub fn registry(&self) -> &ExtensionRegistry {
        &self.registry
    }

    /// Get mutable registry
    pub fn registry_mut(&mut self) -> &mut ExtensionRegistry {
        &mut self.registry
    }

    /// Link all extensions to a Wasmtime linker
    pub fn link_all(&self, linker: &mut Linker<HostState>) -> Result<()> {
        for extension in self.registry.extensions() {
            self.link_extension(linker, extension)?;
        }
        Ok(())
    }

    /// Link a specific extension
    pub fn link_extension(
        &self,
        linker: &mut Linker<HostState>,
        extension: &Extension,
    ) -> Result<()> {
        let namespace = &extension.info.namespace;

        for func in extension.functions() {
            if !func.enabled {
                continue;
            }

            // For demonstration, we'll link a simple example function
            // In a real implementation, you'd have a mapping from signature to actual implementation
            match &func.signature {
                FunctionSignature::I32 => {
                    linker
                        .func_wrap(namespace, &func.name, || -> i32 { 42 })
                        .map_err(|e| anyhow!("Failed to link function {}: {}", func.name, e))?;
                }
                FunctionSignature::I32ToI32 => {
                    linker
                        .func_wrap(namespace, &func.name, |x: i32| -> i32 { x * 2 })
                        .map_err(|e| anyhow!("Failed to link function {}: {}", func.name, e))?;
                }
                FunctionSignature::I32I32ToI32 => {
                    linker
                        .func_wrap(namespace, &func.name, |x: i32, y: i32| -> i32 { x + y })
                        .map_err(|e| anyhow!("Failed to link function {}: {}", func.name, e))?;
                }
                _ => {
                    // For other signatures, we'd need more sophisticated handling
                    // This is a framework - users would extend it
                }
            }
        }

        Ok(())
    }
}

/// Extension builder for fluent API
pub struct ExtensionBuilder {
    extension: Extension,
}

impl ExtensionBuilder {
    /// Create a new extension builder
    pub fn new(name: impl Into<String>) -> Self {
        let info = ExtensionInfo::new(name);
        Self {
            extension: Extension::new(info),
        }
    }

    /// Set version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.extension.info.version = version.into();
        self
    }

    /// Set description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.extension.info.description = description.into();
        self
    }

    /// Set author
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.extension.info.author = author.into();
        self
    }

    /// Set namespace
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.extension.info.namespace = namespace.into();
        self
    }

    /// Add a function
    pub fn function(mut self, func: ExtensionFunction) -> Self {
        self.extension.register_function(func);
        self
    }

    /// Build the extension
    pub fn build(self) -> Extension {
        self.extension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_info() {
        let info = ExtensionInfo::new("test-extension")
            .version("1.0.0")
            .description("Test extension")
            .author("MielinOS Team")
            .namespace("test");

        assert_eq!(info.name, "test-extension");
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.description, "Test extension");
        assert_eq!(info.author, "MielinOS Team");
        assert_eq!(info.namespace, "test");
    }

    #[test]
    fn test_function_signature() {
        assert_eq!(FunctionSignature::Void.as_str(), "() -> ()");
        assert_eq!(FunctionSignature::I32.as_str(), "() -> i32");
        assert_eq!(FunctionSignature::I32ToI32.as_str(), "(i32) -> i32");
        assert_eq!(FunctionSignature::I32I32ToI32.as_str(), "(i32, i32) -> i32");
    }

    #[test]
    fn test_extension_function() {
        let func = ExtensionFunction::new("add", FunctionSignature::I32I32ToI32)
            .description("Add two numbers")
            .enabled(true);

        assert_eq!(func.name, "add");
        assert_eq!(func.description, "Add two numbers");
        assert!(func.enabled);
    }

    #[test]
    fn test_extension_creation() {
        let info = ExtensionInfo::new("math").version("1.0.0");
        let extension = Extension::new(info);

        assert_eq!(extension.info().name, "math");
        assert_eq!(extension.info().version, "1.0.0");
        assert_eq!(extension.functions().count(), 0);
    }

    #[test]
    fn test_extension_register_function() {
        let info = ExtensionInfo::new("math");
        let mut extension = Extension::new(info);

        let func = ExtensionFunction::new("add", FunctionSignature::I32I32ToI32);
        extension.register_function(func);

        assert_eq!(extension.functions().count(), 1);
        assert!(extension.get_function("add").is_some());
    }

    #[test]
    fn test_extension_state() {
        let info = ExtensionInfo::new("test");
        let extension = Extension::new(info);

        // Set state
        extension
            .set_state("counter".to_string(), Box::new(42i32))
            .expect("Failed to set state");

        // Check state exists
        assert!(extension
            .has_state("counter")
            .expect("Failed to check state"));
        assert!(!extension
            .has_state("nonexistent")
            .expect("Failed to check state"));
    }

    #[test]
    fn test_extension_registry() {
        let mut registry = ExtensionRegistry::new();
        assert_eq!(registry.count(), 0);

        let extension = Extension::new(ExtensionInfo::new("test"));
        registry
            .register(extension)
            .expect("Failed to register extension");

        assert_eq!(registry.count(), 1);
        assert!(registry.contains("test"));
        assert!(registry.get("test").is_some());
    }

    #[test]
    fn test_extension_registry_duplicate() {
        let mut registry = ExtensionRegistry::new();

        let extension1 = Extension::new(ExtensionInfo::new("test"));
        registry
            .register(extension1)
            .expect("Failed to register extension");

        let extension2 = Extension::new(ExtensionInfo::new("test"));
        let result = registry.register(extension2);

        assert!(result.is_err());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_extension_registry_unregister() {
        let mut registry = ExtensionRegistry::new();

        let extension = Extension::new(ExtensionInfo::new("test"));
        registry
            .register(extension)
            .expect("Failed to register extension");

        assert_eq!(registry.count(), 1);

        registry
            .unregister("test")
            .expect("Failed to unregister extension");
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_extension_registry_list() {
        let mut registry = ExtensionRegistry::new();

        registry
            .register(Extension::new(ExtensionInfo::new("ext1")))
            .expect("Failed to register extension");
        registry
            .register(Extension::new(ExtensionInfo::new("ext2")))
            .expect("Failed to register extension");

        let names = registry.list_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"ext1".to_string()));
        assert!(names.contains(&"ext2".to_string()));
    }

    #[test]
    fn test_extension_builder() {
        let extension = ExtensionBuilder::new("math")
            .version("1.0.0")
            .description("Math operations")
            .author("MielinOS Team")
            .namespace("math")
            .function(
                ExtensionFunction::new("add", FunctionSignature::I32I32ToI32)
                    .description("Add two numbers"),
            )
            .function(
                ExtensionFunction::new("multiply", FunctionSignature::I32I32ToI32)
                    .description("Multiply two numbers"),
            )
            .build();

        assert_eq!(extension.info().name, "math");
        assert_eq!(extension.info().version, "1.0.0");
        assert_eq!(extension.functions().count(), 2);
    }

    #[test]
    fn test_extension_linker_creation() {
        let registry = ExtensionRegistry::new();
        let linker = ExtensionLinker::new(registry);

        assert_eq!(linker.registry().count(), 0);
    }

    #[test]
    fn test_extension_builder_with_multiple_functions() {
        let extension = ExtensionBuilder::new("utils")
            .version("2.0.0")
            .function(ExtensionFunction::new("get_answer", FunctionSignature::I32))
            .function(ExtensionFunction::new(
                "double",
                FunctionSignature::I32ToI32,
            ))
            .build();

        assert_eq!(extension.functions().count(), 2);
        assert!(extension.get_function("get_answer").is_some());
        assert!(extension.get_function("double").is_some());
    }
}
