//! WebAssembly Component Model Support
//!
//! This module implements the WebAssembly Component Model, which provides:
//! - Component-level composition and modularity
//! - Interface types for cross-component communication
//! - Resource types for managed objects
//! - Component linking and dependency resolution
//!
//! The Component Model enables better code reuse and composition of WebAssembly modules.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use wasmtime::component::{Component, InstancePre, Linker};
use wasmtime::Engine;

/// Errors that can occur during component operations
#[derive(Debug, Error)]
pub enum ComponentError {
    #[error("Component validation failed: {0}")]
    ValidationError(String),

    #[error("Component linking failed: {0}")]
    LinkingError(String),

    #[error("Interface type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Import resolution failed: {0}")]
    ImportResolutionError(String),

    #[error("Export not found: {0}")]
    ExportNotFound(String),
}

/// Interface type definition
///
/// Represents the type system used in component interfaces.
#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceType {
    /// Boolean type
    Bool,
    /// Signed 8-bit integer
    S8,
    /// Unsigned 8-bit integer
    U8,
    /// Signed 16-bit integer
    S16,
    /// Unsigned 16-bit integer
    U16,
    /// Signed 32-bit integer
    S32,
    /// Unsigned 32-bit integer
    U32,
    /// Signed 64-bit integer
    S64,
    /// Unsigned 64-bit integer
    U64,
    /// 32-bit floating point
    F32,
    /// 64-bit floating point
    F64,
    /// Character (Unicode scalar value)
    Char,
    /// String
    String,
    /// List of elements
    List(Box<InterfaceType>),
    /// Record (struct-like)
    Record(Vec<(String, InterfaceType)>),
    /// Variant (enum-like)
    Variant(Vec<(String, Option<InterfaceType>)>),
    /// Tuple
    Tuple(Vec<InterfaceType>),
    /// Option type
    Option(Box<InterfaceType>),
    /// Result type
    Result {
        ok: Option<Box<InterfaceType>>,
        err: Option<Box<InterfaceType>>,
    },
    /// Flags (bitflags)
    Flags(Vec<String>),
    /// Enum
    Enum(Vec<String>),
    /// Resource type
    Resource(String),
}

impl InterfaceType {
    /// Check if this type is compatible with another type
    pub fn is_compatible_with(&self, other: &InterfaceType) -> bool {
        match (self, other) {
            (InterfaceType::Bool, InterfaceType::Bool) => true,
            (InterfaceType::S8, InterfaceType::S8) => true,
            (InterfaceType::U8, InterfaceType::U8) => true,
            (InterfaceType::S16, InterfaceType::S16) => true,
            (InterfaceType::U16, InterfaceType::U16) => true,
            (InterfaceType::S32, InterfaceType::S32) => true,
            (InterfaceType::U32, InterfaceType::U32) => true,
            (InterfaceType::S64, InterfaceType::S64) => true,
            (InterfaceType::U64, InterfaceType::U64) => true,
            (InterfaceType::F32, InterfaceType::F32) => true,
            (InterfaceType::F64, InterfaceType::F64) => true,
            (InterfaceType::Char, InterfaceType::Char) => true,
            (InterfaceType::String, InterfaceType::String) => true,
            (InterfaceType::List(a), InterfaceType::List(b)) => a.is_compatible_with(b),
            (InterfaceType::Option(a), InterfaceType::Option(b)) => a.is_compatible_with(b),
            (InterfaceType::Tuple(a), InterfaceType::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.is_compatible_with(y))
            }
            (InterfaceType::Record(a), InterfaceType::Record(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((name_a, type_a), (name_b, type_b))| {
                            name_a == name_b && type_a.is_compatible_with(type_b)
                        })
            }
            (InterfaceType::Variant(a), InterfaceType::Variant(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((name_a, type_a), (name_b, type_b))| {
                            name_a == name_b
                                && match (type_a, type_b) {
                                    (Some(t_a), Some(t_b)) => t_a.is_compatible_with(t_b),
                                    (None, None) => true,
                                    _ => false,
                                }
                        })
            }
            (
                InterfaceType::Result {
                    ok: ok_a,
                    err: err_a,
                },
                InterfaceType::Result {
                    ok: ok_b,
                    err: err_b,
                },
            ) => {
                let ok_compatible = match (ok_a, ok_b) {
                    (Some(a), Some(b)) => a.is_compatible_with(b),
                    (None, None) => true,
                    _ => false,
                };
                let err_compatible = match (err_a, err_b) {
                    (Some(a), Some(b)) => a.is_compatible_with(b),
                    (None, None) => true,
                    _ => false,
                };
                ok_compatible && err_compatible
            }
            (InterfaceType::Flags(a), InterfaceType::Flags(b)) => a == b,
            (InterfaceType::Enum(a), InterfaceType::Enum(b)) => a == b,
            (InterfaceType::Resource(a), InterfaceType::Resource(b)) => a == b,
            _ => false,
        }
    }

    /// Get a human-readable name for this type
    pub fn name(&self) -> String {
        match self {
            InterfaceType::Bool => "bool".to_string(),
            InterfaceType::S8 => "s8".to_string(),
            InterfaceType::U8 => "u8".to_string(),
            InterfaceType::S16 => "s16".to_string(),
            InterfaceType::U16 => "u16".to_string(),
            InterfaceType::S32 => "s32".to_string(),
            InterfaceType::U32 => "u32".to_string(),
            InterfaceType::S64 => "s64".to_string(),
            InterfaceType::U64 => "u64".to_string(),
            InterfaceType::F32 => "f32".to_string(),
            InterfaceType::F64 => "f64".to_string(),
            InterfaceType::Char => "char".to_string(),
            InterfaceType::String => "string".to_string(),
            InterfaceType::List(t) => format!("list<{}>", t.name()),
            InterfaceType::Record(_) => "record".to_string(),
            InterfaceType::Variant(_) => "variant".to_string(),
            InterfaceType::Tuple(types) => {
                format!(
                    "tuple<{}>",
                    types
                        .iter()
                        .map(|t| t.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            InterfaceType::Option(t) => format!("option<{}>", t.name()),
            InterfaceType::Result { .. } => "result".to_string(),
            InterfaceType::Flags(_) => "flags".to_string(),
            InterfaceType::Enum(_) => "enum".to_string(),
            InterfaceType::Resource(name) => format!("resource<{}>", name),
        }
    }
}

/// Function signature in component interface
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Function name
    pub name: String,
    /// Parameter types
    pub params: Vec<(String, InterfaceType)>,
    /// Return type
    pub results: Vec<InterfaceType>,
}

impl FunctionSignature {
    /// Create a new function signature
    pub fn new(
        name: impl Into<String>,
        params: Vec<(String, InterfaceType)>,
        results: Vec<InterfaceType>,
    ) -> Self {
        Self {
            name: name.into(),
            params,
            results,
        }
    }

    /// Check if this signature is compatible with another
    pub fn is_compatible_with(&self, other: &FunctionSignature) -> bool {
        // Function names must match
        if self.name != other.name {
            return false;
        }

        if self.params.len() != other.params.len() || self.results.len() != other.results.len() {
            return false;
        }

        self.params
            .iter()
            .zip(other.params.iter())
            .all(|((_, type_a), (_, type_b))| type_a.is_compatible_with(type_b))
            && self
                .results
                .iter()
                .zip(other.results.iter())
                .all(|(a, b)| a.is_compatible_with(b))
    }
}

/// Component interface definition
#[derive(Debug, Clone)]
pub struct ComponentInterface {
    /// Interface name
    pub name: String,
    /// Functions exported by this interface
    pub functions: Vec<FunctionSignature>,
    /// Types defined in this interface
    pub types: HashMap<String, InterfaceType>,
    /// Resources managed by this interface
    pub resources: HashMap<String, ResourceDefinition>,
}

impl ComponentInterface {
    /// Create a new component interface
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            functions: Vec::new(),
            types: HashMap::new(),
            resources: HashMap::new(),
        }
    }

    /// Add a function to this interface
    pub fn add_function(&mut self, signature: FunctionSignature) {
        self.functions.push(signature);
    }

    /// Add a type definition
    pub fn add_type(&mut self, name: impl Into<String>, typ: InterfaceType) {
        self.types.insert(name.into(), typ);
    }

    /// Add a resource definition
    pub fn add_resource(&mut self, name: impl Into<String>, resource: ResourceDefinition) {
        self.resources.insert(name.into(), resource);
    }

    /// Get a function by name
    pub fn get_function(&self, name: &str) -> Option<&FunctionSignature> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Get a type by name
    pub fn get_type(&self, name: &str) -> Option<&InterfaceType> {
        self.types.get(name)
    }

    /// Check if this interface is compatible with another
    pub fn is_compatible_with(&self, other: &ComponentInterface) -> bool {
        // Check that all functions in other are present and compatible in self
        other.functions.iter().all(|other_func| {
            self.functions
                .iter()
                .any(|self_func| self_func.is_compatible_with(other_func))
        })
    }
}

/// Resource definition
#[derive(Debug, Clone)]
pub struct ResourceDefinition {
    /// Resource name
    pub name: String,
    /// Methods on this resource
    pub methods: Vec<FunctionSignature>,
    /// Whether this resource is owned
    pub owned: bool,
}

impl ResourceDefinition {
    /// Create a new resource definition
    pub fn new(name: impl Into<String>, owned: bool) -> Self {
        Self {
            name: name.into(),
            methods: Vec::new(),
            owned,
        }
    }

    /// Add a method to this resource
    pub fn add_method(&mut self, method: FunctionSignature) {
        self.methods.push(method);
    }
}

/// Component import
#[derive(Debug, Clone)]
pub struct ComponentImport {
    /// Import name
    pub name: String,
    /// Expected interface
    pub interface: ComponentInterface,
}

/// Component export
#[derive(Debug, Clone)]
pub struct ComponentExport {
    /// Export name
    pub name: String,
    /// Exported interface
    pub interface: ComponentInterface,
}

/// Component metadata
#[derive(Debug, Clone)]
pub struct ComponentMetadata {
    /// Component name
    pub name: String,
    /// Component version
    pub version: String,
    /// Component description
    pub description: Option<String>,
    /// Component authors
    pub authors: Vec<String>,
    /// Component license
    pub license: Option<String>,
}

impl ComponentMetadata {
    /// Create new component metadata
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: None,
            authors: Vec::new(),
            license: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add author
    pub fn add_author(mut self, author: impl Into<String>) -> Self {
        self.authors.push(author.into());
        self
    }

    /// Set license
    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }
}

/// WebAssembly component
pub struct WasmComponent {
    /// Component metadata
    pub metadata: ComponentMetadata,
    /// Component imports
    pub imports: Vec<ComponentImport>,
    /// Component exports
    pub exports: Vec<ComponentExport>,
    /// Compiled component
    component: Component,
    /// Pre-instantiated component (for performance)
    instance_pre: Option<Arc<InstancePre<()>>>,
}

impl WasmComponent {
    /// Create a new component from bytecode
    pub fn new(engine: &Engine, bytecode: &[u8], metadata: ComponentMetadata) -> Result<Self> {
        let component = Component::from_binary(engine, bytecode)?;

        Ok(Self {
            metadata,
            imports: Vec::new(),
            exports: Vec::new(),
            component,
            instance_pre: None,
        })
    }

    /// Add an import requirement
    pub fn add_import(&mut self, import: ComponentImport) {
        self.imports.push(import);
    }

    /// Add an export
    pub fn add_export(&mut self, export: ComponentExport) {
        self.exports.push(export);
    }

    /// Get an import by name
    pub fn get_import(&self, name: &str) -> Option<&ComponentImport> {
        self.imports.iter().find(|i| i.name == name)
    }

    /// Get an export by name
    pub fn get_export(&self, name: &str) -> Option<&ComponentExport> {
        self.exports.iter().find(|e| e.name == name)
    }

    /// Validate this component
    pub fn validate(&self) -> Result<(), ComponentError> {
        // Validate that all imports have valid interfaces
        for import in &self.imports {
            if import.interface.functions.is_empty()
                && import.interface.types.is_empty()
                && import.interface.resources.is_empty()
            {
                return Err(ComponentError::ValidationError(format!(
                    "Import '{}' has empty interface",
                    import.name
                )));
            }
        }

        // Validate that exports have valid interfaces
        for export in &self.exports {
            if export.interface.functions.is_empty()
                && export.interface.types.is_empty()
                && export.interface.resources.is_empty()
            {
                return Err(ComponentError::ValidationError(format!(
                    "Export '{}' has empty interface",
                    export.name
                )));
            }
        }

        Ok(())
    }

    /// Pre-instantiate this component for faster instantiation
    pub fn pre_instantiate(&mut self, linker: &Linker<()>) -> Result<(), ComponentError> {
        match linker.instantiate_pre(&self.component) {
            Ok(instance_pre) => {
                self.instance_pre = Some(Arc::new(instance_pre));
                Ok(())
            }
            Err(e) => Err(ComponentError::LinkingError(e.to_string())),
        }
    }

    /// Get the underlying component
    pub fn component(&self) -> &Component {
        &self.component
    }
}

/// Component linker for resolving dependencies
pub struct ComponentLinker {
    /// Registered components by name
    components: HashMap<String, Arc<WasmComponent>>,
    /// Dependency graph (component -> dependencies)
    dependencies: HashMap<String, Vec<String>>,
}

impl ComponentLinker {
    /// Create a new component linker
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    /// Register a component
    pub fn register_component(&mut self, component: Arc<WasmComponent>) -> Result<()> {
        let name = component.metadata.name.clone();
        self.components.insert(name.clone(), component);
        self.dependencies.insert(name, Vec::new());
        Ok(())
    }

    /// Add a dependency between components
    pub fn add_dependency(
        &mut self,
        dependent: impl Into<String>,
        dependency: impl Into<String>,
    ) -> Result<(), ComponentError> {
        let dependent = dependent.into();
        let dependency = dependency.into();

        // Check for circular dependencies
        if self.has_circular_dependency(&dependent, &dependency) {
            return Err(ComponentError::CircularDependency(format!(
                "{} -> {}",
                dependent, dependency
            )));
        }

        self.dependencies
            .entry(dependent)
            .or_default()
            .push(dependency);

        Ok(())
    }

    /// Check if adding a dependency would create a circular dependency
    fn has_circular_dependency(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }

        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![to];

        while let Some(current) = stack.pop() {
            if current == from {
                return true;
            }

            if visited.insert(current) {
                if let Some(deps) = self.dependencies.get(current) {
                    stack.extend(deps.iter().map(|s| s.as_str()));
                }
            }
        }

        false
    }

    /// Resolve dependencies for a component
    pub fn resolve_dependencies(
        &self,
        component_name: &str,
    ) -> Result<Vec<Arc<WasmComponent>>, ComponentError> {
        // Verify component exists
        let _component = self
            .components
            .get(component_name)
            .ok_or_else(|| ComponentError::ComponentNotFound(component_name.to_string()))?;

        let mut resolved = Vec::new();
        let mut visited = std::collections::HashSet::new();

        self.resolve_recursive(component_name, &mut resolved, &mut visited)?;

        Ok(resolved)
    }

    /// Recursively resolve dependencies
    fn resolve_recursive(
        &self,
        component_name: &str,
        resolved: &mut Vec<Arc<WasmComponent>>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<(), ComponentError> {
        if visited.contains(component_name) {
            return Ok(());
        }

        visited.insert(component_name.to_string());

        if let Some(deps) = self.dependencies.get(component_name) {
            for dep in deps {
                self.resolve_recursive(dep, resolved, visited)?;
            }
        }

        if let Some(component) = self.components.get(component_name) {
            resolved.push(Arc::clone(component));
        }

        Ok(())
    }

    /// Link a component with its dependencies
    pub fn link_component(
        &self,
        component_name: &str,
    ) -> Result<Arc<WasmComponent>, ComponentError> {
        let component = self
            .components
            .get(component_name)
            .ok_or_else(|| ComponentError::ComponentNotFound(component_name.to_string()))?;

        // Validate all imports are satisfied
        for import in &component.imports {
            self.validate_import(component_name, import)?;
        }

        Ok(Arc::clone(component))
    }

    /// Validate that an import can be satisfied
    fn validate_import(
        &self,
        component_name: &str,
        import: &ComponentImport,
    ) -> Result<(), ComponentError> {
        if let Some(deps) = self.dependencies.get(component_name) {
            for dep_name in deps {
                if let Some(_dep_component) = self.components.get(dep_name) {
                    // Check if dependency exports match import
                    for export in &_dep_component.exports {
                        if export.name == import.name
                            && export.interface.is_compatible_with(&import.interface)
                        {
                            return Ok(());
                        }
                    }
                }
            }
        }

        Err(ComponentError::ImportResolutionError(format!(
            "Cannot resolve import '{}' for component '{}'",
            import.name, component_name
        )))
    }

    /// Get all registered component names
    pub fn component_names(&self) -> Vec<String> {
        self.components.keys().cloned().collect()
    }

    /// Get a component by name
    pub fn get_component(&self, name: &str) -> Option<Arc<WasmComponent>> {
        self.components.get(name).cloned()
    }
}

impl Default for ComponentLinker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_type_compatibility() {
        assert!(InterfaceType::Bool.is_compatible_with(&InterfaceType::Bool));
        assert!(InterfaceType::S32.is_compatible_with(&InterfaceType::S32));
        assert!(!InterfaceType::S32.is_compatible_with(&InterfaceType::U32));

        let list_s32 = InterfaceType::List(Box::new(InterfaceType::S32));
        let list_s32_2 = InterfaceType::List(Box::new(InterfaceType::S32));
        assert!(list_s32.is_compatible_with(&list_s32_2));

        let list_u32 = InterfaceType::List(Box::new(InterfaceType::U32));
        assert!(!list_s32.is_compatible_with(&list_u32));
    }

    #[test]
    fn test_interface_type_name() {
        assert_eq!(InterfaceType::Bool.name(), "bool");
        assert_eq!(InterfaceType::S32.name(), "s32");
        assert_eq!(InterfaceType::String.name(), "string");

        let list_s32 = InterfaceType::List(Box::new(InterfaceType::S32));
        assert_eq!(list_s32.name(), "list<s32>");

        let tuple = InterfaceType::Tuple(vec![InterfaceType::S32, InterfaceType::String]);
        assert_eq!(tuple.name(), "tuple<s32, string>");
    }

    #[test]
    fn test_function_signature_compatibility() {
        let sig1 = FunctionSignature::new(
            "test",
            vec![("x".to_string(), InterfaceType::S32)],
            vec![InterfaceType::Bool],
        );

        let sig2 = FunctionSignature::new(
            "test",
            vec![("y".to_string(), InterfaceType::S32)],
            vec![InterfaceType::Bool],
        );

        assert!(sig1.is_compatible_with(&sig2));

        let sig3 = FunctionSignature::new(
            "test",
            vec![("x".to_string(), InterfaceType::U32)],
            vec![InterfaceType::Bool],
        );

        assert!(!sig1.is_compatible_with(&sig3));
    }

    #[test]
    fn test_component_interface() {
        let mut interface = ComponentInterface::new("test-interface");

        interface.add_function(FunctionSignature::new(
            "add",
            vec![
                ("a".to_string(), InterfaceType::S32),
                ("b".to_string(), InterfaceType::S32),
            ],
            vec![InterfaceType::S32],
        ));

        interface.add_type("MyType", InterfaceType::String);

        assert!(interface.get_function("add").is_some());
        assert!(interface.get_type("MyType").is_some());
        assert!(interface.get_function("missing").is_none());
    }

    #[test]
    fn test_resource_definition() {
        let mut resource = ResourceDefinition::new("FileHandle", true);

        resource.add_method(FunctionSignature::new(
            "read",
            vec![("count".to_string(), InterfaceType::U64)],
            vec![InterfaceType::List(Box::new(InterfaceType::U8))],
        ));

        assert_eq!(resource.methods.len(), 1);
        assert_eq!(resource.methods[0].name, "read");
    }

    #[test]
    fn test_component_metadata() {
        let metadata = ComponentMetadata::new("my-component", "1.0.0")
            .with_description("A test component")
            .add_author("Test Author")
            .with_license("MIT");

        assert_eq!(metadata.name, "my-component");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.description, Some("A test component".to_string()));
        assert_eq!(metadata.authors.len(), 1);
        assert_eq!(metadata.license, Some("MIT".to_string()));
    }

    #[test]
    fn test_component_linker_basic() {
        let linker = ComponentLinker::new();
        assert_eq!(linker.component_names().len(), 0);
    }

    #[test]
    fn test_component_linker_circular_dependency() {
        let mut linker = ComponentLinker::new();

        // Create a simple dependency graph: A -> B -> C -> A (circular)
        linker
            .dependencies
            .insert("A".to_string(), vec!["B".to_string()]);
        linker
            .dependencies
            .insert("B".to_string(), vec!["C".to_string()]);

        assert!(linker.has_circular_dependency("C", "A"));
        assert!(linker.add_dependency("C", "A").is_err());
    }

    #[test]
    fn test_component_linker_non_circular() {
        let mut linker = ComponentLinker::new();

        // Create a non-circular dependency graph: A -> B, C -> B
        linker
            .dependencies
            .insert("A".to_string(), vec!["B".to_string()]);
        linker
            .dependencies
            .insert("C".to_string(), vec!["B".to_string()]);

        assert!(!linker.has_circular_dependency("B", "D"));
        assert!(linker.add_dependency("B", "D").is_ok());
    }

    #[test]
    fn test_interface_compatibility() {
        let mut iface1 = ComponentInterface::new("iface1");
        iface1.add_function(FunctionSignature::new(
            "test",
            vec![("x".to_string(), InterfaceType::S32)],
            vec![InterfaceType::Bool],
        ));

        let mut iface2 = ComponentInterface::new("iface2");
        iface2.add_function(FunctionSignature::new(
            "test",
            vec![("y".to_string(), InterfaceType::S32)],
            vec![InterfaceType::Bool],
        ));

        assert!(iface1.is_compatible_with(&iface2));

        let mut iface3 = ComponentInterface::new("iface3");
        iface3.add_function(FunctionSignature::new(
            "test",
            vec![("x".to_string(), InterfaceType::U32)],
            vec![InterfaceType::Bool],
        ));

        assert!(!iface1.is_compatible_with(&iface3));
    }

    #[test]
    fn test_tuple_type_compatibility() {
        let tuple1 = InterfaceType::Tuple(vec![InterfaceType::S32, InterfaceType::Bool]);
        let tuple2 = InterfaceType::Tuple(vec![InterfaceType::S32, InterfaceType::Bool]);
        let tuple3 = InterfaceType::Tuple(vec![InterfaceType::S32, InterfaceType::String]);

        assert!(tuple1.is_compatible_with(&tuple2));
        assert!(!tuple1.is_compatible_with(&tuple3));
    }

    #[test]
    fn test_record_type_compatibility() {
        let record1 = InterfaceType::Record(vec![
            ("x".to_string(), InterfaceType::S32),
            ("y".to_string(), InterfaceType::Bool),
        ]);

        let record2 = InterfaceType::Record(vec![
            ("x".to_string(), InterfaceType::S32),
            ("y".to_string(), InterfaceType::Bool),
        ]);

        let record3 = InterfaceType::Record(vec![
            ("x".to_string(), InterfaceType::S32),
            ("y".to_string(), InterfaceType::String),
        ]);

        assert!(record1.is_compatible_with(&record2));
        assert!(!record1.is_compatible_with(&record3));
    }

    #[test]
    fn test_resource_type_compatibility() {
        let res1 = InterfaceType::Resource("FileHandle".to_string());
        let res2 = InterfaceType::Resource("FileHandle".to_string());
        let res3 = InterfaceType::Resource("SocketHandle".to_string());

        assert!(res1.is_compatible_with(&res2));
        assert!(!res1.is_compatible_with(&res3));
    }
}
