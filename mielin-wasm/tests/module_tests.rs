//! Integration tests for multi-module support

use mielin_wasm::host::HostState;
use mielin_wasm::module::{ModuleRegistry, ModuleState, SharedMemoryConfig};

fn create_math_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
            (func (export "multiply") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.mul
            )
            (memory (export "memory") 1)
        )
        "#,
    )
    .unwrap()
}

fn create_utils_module() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "square") (param i32) (result i32)
                local.get 0
                local.get 0
                i32.mul
            )
            (func (export "double") (param i32) (result i32)
                local.get 0
                i32.const 2
                i32.mul
            )
            (memory (export "memory") 1)
        )
        "#,
    )
    .unwrap()
}

fn create_app_module_with_imports() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (import "math" "add" (func $add (param i32 i32) (result i32)))
            (import "math" "multiply" (func $mul (param i32 i32) (result i32)))

            (func (export "compute") (param i32 i32) (result i32)
                ;; compute (a + b) * 2
                local.get 0
                local.get 1
                call $add
                i32.const 2
                call $mul
            )
            (memory (export "memory") 1)
        )
        "#,
    )
    .unwrap()
}

#[test]
fn test_module_registry_basic() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();

    // Register module
    let result = registry.register("math", "Math Module", "1.0.0", &math_module);
    assert!(result.is_ok());

    // Verify module info
    let info = registry.get_module_info("math").unwrap();
    assert_eq!(info.id, "math");
    assert_eq!(info.name, "Math Module");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.state, ModuleState::Registered);
}

#[test]
fn test_module_registration_multiple() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();
    let utils_module = create_utils_module();

    // Register multiple modules
    registry
        .register("math", "Math Module", "1.0.0", &math_module)
        .unwrap();
    registry
        .register("utils", "Utils Module", "1.0.0", &utils_module)
        .unwrap();

    // Verify both are registered
    let modules = registry.list_modules();
    assert_eq!(modules.len(), 2);

    let stats = registry.get_stats();
    assert_eq!(stats.total_modules, 2);
}

#[test]
fn test_module_import_export_extraction() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();

    registry
        .register("math", "Math Module", "1.0.0", &math_module)
        .unwrap();

    let info = registry.get_module_info("math").unwrap();

    // Check exports
    assert_eq!(info.exports.len(), 3); // add, multiply, memory
    let export_names: Vec<_> = info.exports.iter().map(|e| e.name.as_str()).collect();
    assert!(export_names.contains(&"add"));
    assert!(export_names.contains(&"multiply"));
    assert!(export_names.contains(&"memory"));
}

#[test]
fn test_module_dependency_detection() {
    let mut registry = ModuleRegistry::new().unwrap();
    let app_module = create_app_module_with_imports();

    registry
        .register("app", "App Module", "1.0.0", &app_module)
        .unwrap();

    let info = registry.get_module_info("app").unwrap();

    // Should detect dependency on "math" module
    assert_eq!(info.dependencies.len(), 1);
    assert_eq!(info.dependencies[0], "math");

    // Check imports
    assert_eq!(info.imports.len(), 2); // add, multiply
}

#[test]
fn test_shared_memory_creation() {
    let mut registry = ModuleRegistry::new().unwrap();

    // Create shared memory with default config
    let result = registry.create_shared_memory("shared1", SharedMemoryConfig::default());
    assert!(result.is_ok());

    // Create with custom config
    let config = SharedMemoryConfig {
        initial_pages: 2,
        maximum_pages: Some(512),
        is_64: false,
        shared: true,
    };
    let result = registry.create_shared_memory("shared2", config);
    assert!(result.is_ok());

    let stats = registry.get_stats();
    assert_eq!(stats.shared_memories, 2);
}

#[test]
fn test_shared_memory_duplicate_prevention() {
    let mut registry = ModuleRegistry::new().unwrap();

    registry
        .create_shared_memory("shared", SharedMemoryConfig::default())
        .unwrap();

    // Second creation should fail
    let result = registry.create_shared_memory("shared", SharedMemoryConfig::default());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}

#[test]
fn test_module_linking_basic() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();
    let utils_module = create_utils_module();

    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();
    registry
        .register("utils", "Utils", "1.0.0", &utils_module)
        .unwrap();
    registry
        .register("app", "App", "1.0.0", &create_app_module_with_imports())
        .unwrap();

    // Link app to math
    let result = registry.link_modules("app", &["math".to_string()]);
    assert!(result.is_ok());

    // Verify dependency graph
    let deps = registry.get_dependency_graph();
    assert!(deps.contains_key("app"));
    assert!(deps["app"].contains("math"));
}

#[test]
fn test_circular_dependency_detection() {
    let mut registry = ModuleRegistry::new().unwrap();

    // Manually create circular dependency for testing
    use std::collections::HashSet;
    registry.dependency_graph.insert(
        "A".to_string(),
        ["B".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );
    registry.dependency_graph.insert(
        "B".to_string(),
        ["C".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );
    registry.dependency_graph.insert(
        "C".to_string(),
        ["A".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );

    // Should detect cycle
    let result = registry.check_circular_dependencies("A");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Circular dependency"));
}

#[test]
fn test_circular_dependency_complex() {
    let mut registry = ModuleRegistry::new().unwrap();

    // Create more complex dependency graph: A -> B -> C -> D -> B (cycle)
    use std::collections::HashSet;
    registry.dependency_graph.insert(
        "A".to_string(),
        ["B".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );
    registry.dependency_graph.insert(
        "B".to_string(),
        ["C".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );
    registry.dependency_graph.insert(
        "C".to_string(),
        ["D".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );
    registry.dependency_graph.insert(
        "D".to_string(),
        ["B".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );

    // Starting from A should detect the B->C->D->B cycle
    let result = registry.check_circular_dependencies("A");
    assert!(result.is_err());
}

#[test]
fn test_no_circular_dependency() {
    let mut registry = ModuleRegistry::new().unwrap();

    // Create acyclic dependency graph: A -> B -> C
    use std::collections::HashSet;
    registry.dependency_graph.insert(
        "A".to_string(),
        ["B".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );
    registry.dependency_graph.insert(
        "B".to_string(),
        ["C".to_string()].iter().cloned().collect::<HashSet<_>>(),
    );

    // Should not detect cycle
    let result = registry.check_circular_dependencies("A");
    assert!(result.is_ok());
}

#[test]
fn test_module_instantiation_simple() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();

    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();

    // Instantiate module
    let result = registry.instantiate(
        "math",
        HostState::new(mielin_hal::capabilities::HardwareCapabilities::NONE),
    );
    assert!(result.is_ok());

    let instance = result.unwrap();
    assert!(instance.read().is_ok());

    // Module should now be in Ready state
    let info = registry.get_module_info("math").unwrap();
    assert_eq!(info.state, ModuleState::Ready);
}

#[test]
fn test_module_unload() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();

    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();
    registry
        .instantiate(
            "math",
            HostState::new(mielin_hal::capabilities::HardwareCapabilities::NONE),
        )
        .unwrap();

    // Unload module
    let result = registry.unload("math");
    assert!(result.is_ok());

    // Module should be in Unloaded state
    let info = registry.get_module_info("math").unwrap();
    assert_eq!(info.state, ModuleState::Unloaded);

    // Instance should be removed
    assert!(registry.get_instance("math").is_none());
}

#[test]
fn test_module_unload_with_dependents() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();
    let app_module = create_app_module_with_imports();

    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();
    registry
        .register("app", "App", "1.0.0", &app_module)
        .unwrap();
    registry.link_modules("app", &["math".to_string()]).unwrap();

    // Try to unload math while app depends on it
    let result = registry.unload("math");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("depended on by"));
}

#[test]
fn test_registry_stats() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();
    let utils_module = create_utils_module();

    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();
    registry
        .register("utils", "Utils", "1.0.0", &utils_module)
        .unwrap();

    registry
        .instantiate(
            "math",
            HostState::new(mielin_hal::capabilities::HardwareCapabilities::NONE),
        )
        .unwrap();

    registry
        .create_shared_memory("shared", SharedMemoryConfig::default())
        .unwrap();

    let stats = registry.get_stats();
    assert_eq!(stats.total_modules, 2);
    assert_eq!(stats.active_instances, 1);
    assert_eq!(stats.shared_memories, 1);
}

#[test]
fn test_module_state_transitions() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();

    // Initial state: Registered
    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();
    let info = registry.get_module_info("math").unwrap();
    assert_eq!(info.state, ModuleState::Registered);

    // After instantiation: Ready
    registry
        .instantiate(
            "math",
            HostState::new(mielin_hal::capabilities::HardwareCapabilities::NONE),
        )
        .unwrap();
    let info = registry.get_module_info("math").unwrap();
    assert_eq!(info.state, ModuleState::Ready);

    // After unload: Unloaded
    registry.unload("math").unwrap();
    let info = registry.get_module_info("math").unwrap();
    assert_eq!(info.state, ModuleState::Unloaded);
}

#[test]
fn test_shared_memory_config_custom() {
    let config = SharedMemoryConfig {
        initial_pages: 4,
        maximum_pages: Some(1024),
        is_64: false,
        shared: true,
    };

    assert_eq!(config.initial_pages, 4);
    assert_eq!(config.maximum_pages, Some(1024));
    assert!(!config.is_64);
    assert!(config.shared);
}

#[test]
fn test_module_registry_list_modules() {
    let mut registry = ModuleRegistry::new().unwrap();
    let math_module = create_math_module();
    let utils_module = create_utils_module();

    registry
        .register("math", "Math", "1.0.0", &math_module)
        .unwrap();
    registry
        .register("utils", "Utils", "2.0.0", &utils_module)
        .unwrap();

    let modules = registry.list_modules();
    assert_eq!(modules.len(), 2);

    let names: Vec<_> = modules.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"Math"));
    assert!(names.contains(&"Utils"));
}

#[test]
fn test_module_missing_dependency() {
    let mut registry = ModuleRegistry::new().unwrap();
    let app_module = create_app_module_with_imports();

    // Register app but not its dependency (math)
    registry
        .register("app", "App", "1.0.0", &app_module)
        .unwrap();

    // Try to instantiate without dependency
    let result = registry.instantiate(
        "app",
        HostState::new(mielin_hal::capabilities::HardwareCapabilities::NONE),
    );
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("must be instantiated before"));
    }
}
