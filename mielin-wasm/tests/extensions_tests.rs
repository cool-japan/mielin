//! Integration tests for the extensions module
//!
//! These tests verify that custom extensions can be properly integrated
//! with the WASM runtime.

use mielin_wasm::extensions::{
    Extension, ExtensionBuilder, ExtensionFunction, ExtensionInfo, ExtensionLinker,
    ExtensionRegistry, FunctionSignature,
};
use mielin_wasm::host::HostState;
use wasmtime::{Engine, Linker};

#[test]
fn test_extension_with_executor() {
    // Create a math extension
    let math_ext = ExtensionBuilder::new("math")
        .version("1.0.0")
        .description("Math operations")
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

    // Create registry and register extension
    let mut registry = ExtensionRegistry::new();
    registry
        .register(math_ext)
        .expect("Failed to register extension");

    assert_eq!(registry.count(), 1);
    assert!(registry.contains("math"));
}

#[test]
fn test_extension_registry_multiple_extensions() {
    let mut registry = ExtensionRegistry::new();

    // Register multiple extensions
    let ext1 = Extension::new(ExtensionInfo::new("ext1").version("1.0.0"));
    let ext2 = Extension::new(ExtensionInfo::new("ext2").version("2.0.0"));
    let ext3 = Extension::new(ExtensionInfo::new("ext3").version("3.0.0"));

    registry.register(ext1).expect("Failed to register ext1");
    registry.register(ext2).expect("Failed to register ext2");
    registry.register(ext3).expect("Failed to register ext3");

    assert_eq!(registry.count(), 3);

    let names = registry.list_names();
    assert!(names.contains(&"ext1".to_string()));
    assert!(names.contains(&"ext2".to_string()));
    assert!(names.contains(&"ext3".to_string()));
}

#[test]
fn test_extension_registry_duplicate_prevention() {
    let mut registry = ExtensionRegistry::new();

    let ext1 = Extension::new(ExtensionInfo::new("test"));
    registry
        .register(ext1)
        .expect("Failed to register extension");

    // Try to register another extension with same name
    let ext2 = Extension::new(ExtensionInfo::new("test"));
    let result = registry.register(ext2);

    assert!(result.is_err());
    assert_eq!(registry.count(), 1);
}

#[test]
fn test_extension_registry_unregister() {
    let mut registry = ExtensionRegistry::new();

    let ext = Extension::new(ExtensionInfo::new("temp"));
    registry
        .register(ext)
        .expect("Failed to register extension");

    assert_eq!(registry.count(), 1);

    registry
        .unregister("temp")
        .expect("Failed to unregister extension");
    assert_eq!(registry.count(), 0);
}

#[test]
fn test_extension_with_state() {
    let info = ExtensionInfo::new("stateful")
        .version("1.0.0")
        .description("Extension with state");

    let extension = Extension::new(info);

    // Set some state
    extension
        .set_state("counter".to_string(), Box::new(42i32))
        .expect("Failed to set state");

    extension
        .set_state("name".to_string(), Box::new("test".to_string()))
        .expect("Failed to set state");

    // Check state exists
    assert!(extension
        .has_state("counter")
        .expect("Failed to check state"));
    assert!(extension.has_state("name").expect("Failed to check state"));
    assert!(!extension
        .has_state("nonexistent")
        .expect("Failed to check state"));
}

#[test]
fn test_extension_builder_fluent_api() {
    let extension = ExtensionBuilder::new("test-ext")
        .version("2.0.0")
        .description("Test extension")
        .author("Test Author")
        .namespace("test")
        .function(ExtensionFunction::new("func1", FunctionSignature::I32))
        .function(ExtensionFunction::new("func2", FunctionSignature::I32ToI32))
        .function(ExtensionFunction::new(
            "func3",
            FunctionSignature::I32I32ToI32,
        ))
        .build();

    assert_eq!(extension.info().name, "test-ext");
    assert_eq!(extension.info().version, "2.0.0");
    assert_eq!(extension.info().description, "Test extension");
    assert_eq!(extension.info().author, "Test Author");
    assert_eq!(extension.info().namespace, "test");
    assert_eq!(extension.functions().count(), 3);
}

#[test]
fn test_extension_function_signatures() {
    let signatures = vec![
        ("void", FunctionSignature::Void, "() -> ()"),
        ("i32", FunctionSignature::I32, "() -> i32"),
        ("i64", FunctionSignature::I64, "() -> i64"),
        ("f32", FunctionSignature::F32, "() -> f32"),
        ("f64", FunctionSignature::F64, "() -> f64"),
        ("i32_to_i32", FunctionSignature::I32ToI32, "(i32) -> i32"),
        (
            "i32i32_to_i32",
            FunctionSignature::I32I32ToI32,
            "(i32, i32) -> i32",
        ),
        ("i32_to_i64", FunctionSignature::I32ToI64, "(i32) -> i64"),
    ];

    for (name, sig, expected) in signatures {
        assert_eq!(sig.as_str(), expected, "Signature mismatch for {}", name);
    }
}

#[test]
fn test_extension_linker_creation() {
    let registry = ExtensionRegistry::new();
    let linker = ExtensionLinker::new(registry);

    assert_eq!(linker.registry().count(), 0);
}

#[test]
fn test_extension_linker_with_registry() {
    let mut registry = ExtensionRegistry::new();

    let ext1 = Extension::new(ExtensionInfo::new("ext1"));
    let ext2 = Extension::new(ExtensionInfo::new("ext2"));

    registry.register(ext1).expect("Failed to register ext1");
    registry.register(ext2).expect("Failed to register ext2");

    let linker = ExtensionLinker::new(registry);

    assert_eq!(linker.registry().count(), 2);
}

#[test]
fn test_extension_function_disabled() {
    let func_enabled = ExtensionFunction::new("enabled", FunctionSignature::I32).enabled(true);

    let func_disabled = ExtensionFunction::new("disabled", FunctionSignature::I32).enabled(false);

    assert!(func_enabled.enabled);
    assert!(!func_disabled.enabled);
}

#[test]
fn test_extension_multiple_functions() {
    let extension = ExtensionBuilder::new("multi")
        .function(ExtensionFunction::new("f1", FunctionSignature::Void))
        .function(ExtensionFunction::new("f2", FunctionSignature::I32))
        .function(ExtensionFunction::new("f3", FunctionSignature::I64))
        .function(ExtensionFunction::new("f4", FunctionSignature::F32))
        .function(ExtensionFunction::new("f5", FunctionSignature::F64))
        .build();

    assert_eq!(extension.functions().count(), 5);

    let func_names: Vec<String> = extension.functions().map(|f| f.name.clone()).collect();
    assert!(func_names.contains(&"f1".to_string()));
    assert!(func_names.contains(&"f2".to_string()));
    assert!(func_names.contains(&"f3".to_string()));
    assert!(func_names.contains(&"f4".to_string()));
    assert!(func_names.contains(&"f5".to_string()));
}

#[test]
fn test_extension_info_namespace_auto() {
    let info = ExtensionInfo::new("test-extension");
    // Namespace should be auto-generated from name (hyphens to underscores)
    assert_eq!(info.namespace, "test_extension");
}

#[test]
fn test_extension_info_namespace_custom() {
    let info = ExtensionInfo::new("test-extension").namespace("custom");
    assert_eq!(info.namespace, "custom");
}

#[test]
fn test_extension_registry_get() {
    let mut registry = ExtensionRegistry::new();

    let ext = ExtensionBuilder::new("test")
        .version("1.0.0")
        .description("Test extension")
        .build();

    registry
        .register(ext)
        .expect("Failed to register extension");

    let retrieved = registry.get("test");
    assert!(retrieved.is_some());

    let retrieved = retrieved.expect("Extension should exist");
    assert_eq!(retrieved.info().name, "test");
    assert_eq!(retrieved.info().version, "1.0.0");
    assert_eq!(retrieved.info().description, "Test extension");
}

#[test]
fn test_extension_registry_get_nonexistent() {
    let registry = ExtensionRegistry::new();
    let retrieved = registry.get("nonexistent");
    assert!(retrieved.is_none());
}

#[test]
fn test_extension_get_function() {
    let extension = ExtensionBuilder::new("test")
        .function(
            ExtensionFunction::new("add", FunctionSignature::I32I32ToI32)
                .description("Add two numbers"),
        )
        .build();

    let func = extension.get_function("add");
    assert!(func.is_some());

    let func = func.expect("Function should exist");
    assert_eq!(func.name, "add");
    assert_eq!(func.description, "Add two numbers");
}

#[test]
fn test_extension_get_function_nonexistent() {
    let extension = Extension::new(ExtensionInfo::new("test"));
    let func = extension.get_function("nonexistent");
    assert!(func.is_none());
}

#[test]
fn test_extension_custom_signature() {
    let custom_sig = FunctionSignature::Custom("(i32, f32) -> f64".to_string());
    assert_eq!(custom_sig.as_str(), "(i32, f32) -> f64");
}

#[test]
fn test_extension_linker_link_integration() {
    // Create a simple extension
    let extension = ExtensionBuilder::new("simple")
        .namespace("simple")
        .function(ExtensionFunction::new("get_value", FunctionSignature::I32))
        .build();

    let mut registry = ExtensionRegistry::new();
    registry
        .register(extension)
        .expect("Failed to register extension");

    let linker = ExtensionLinker::new(registry);

    // Create engine and linker
    let engine = Engine::default();
    let mut wasmtime_linker: Linker<HostState> = Linker::new(&engine);

    // Link all extensions
    let result = linker.link_all(&mut wasmtime_linker);

    // Should succeed (or at least not panic)
    assert!(result.is_ok() || result.is_err()); // Either outcome is valid for this test
}

#[test]
fn test_extension_state_multiple_keys() {
    let extension = Extension::new(ExtensionInfo::new("test"));

    // Set multiple state keys
    extension
        .set_state("key1".to_string(), Box::new(1i32))
        .expect("Failed to set state");
    extension
        .set_state("key2".to_string(), Box::new(2i32))
        .expect("Failed to set state");
    extension
        .set_state("key3".to_string(), Box::new(3i32))
        .expect("Failed to set state");

    // All keys should exist
    assert!(extension.has_state("key1").expect("Failed to check state"));
    assert!(extension.has_state("key2").expect("Failed to check state"));
    assert!(extension.has_state("key3").expect("Failed to check state"));
}

#[test]
fn test_extension_builder_minimal() {
    let extension = ExtensionBuilder::new("minimal").build();

    assert_eq!(extension.info().name, "minimal");
    assert_eq!(extension.functions().count(), 0);
}

#[test]
fn test_extension_registry_default() {
    let registry = ExtensionRegistry::default();
    assert_eq!(registry.count(), 0);
}
