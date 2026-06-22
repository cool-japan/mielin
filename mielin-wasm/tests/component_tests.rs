//! Component Model Integration Tests
//!
//! Tests for WebAssembly Component Model functionality including:
//! - Component creation and validation
//! - Interface type checking
//! - Resource management
//! - Component linking and dependency resolution
//! - Import/export compatibility

use mielin_wasm::component::{
    ComponentError, ComponentExport, ComponentImport, ComponentInterface, ComponentLinker,
    ComponentMetadata, FunctionSignature, InterfaceType, ResourceDefinition, WasmComponent,
};
use std::sync::Arc;
use wasmtime::{component::Linker, Engine};

#[cfg_attr(miri, ignore)]
#[test]
fn test_component_creation_and_validation() {
    // Create a simple component
    let engine = Engine::default();

    // Create a minimal component bytecode with kebab-case export name
    let minimal_component = wat::parse_str(
        r#"
        (component
            (core module $m
                (func (export "test") (result i32)
                    i32.const 42
                )
            )
            (core instance $i (instantiate $m))
            (func (export "get-value") (result s32)
                (canon lift (core func $i "test"))
            )
        )
        "#,
    )
    .expect("Failed to parse component WAT");

    let metadata = ComponentMetadata::new("test-component", "1.0.0")
        .with_description("A test component")
        .add_author("Test Author");

    let mut component = WasmComponent::new(&engine, &minimal_component, metadata)
        .expect("Failed to create component");

    // Add an export
    let mut export_interface = ComponentInterface::new("test-interface");
    export_interface.add_function(FunctionSignature::new(
        "get-value",
        vec![],
        vec![InterfaceType::S32],
    ));

    component.add_export(ComponentExport {
        name: "test-interface".to_string(),
        interface: export_interface,
    });

    // Validate the component
    assert!(component.validate().is_ok());
}

#[cfg_attr(miri, ignore)]
#[test]
fn test_component_validation_empty_interface() {
    let engine = Engine::default();

    let minimal_component = wat::parse_str(
        r#"
        (component
            (core module $m
                (func (export "test") (result i32)
                    i32.const 42
                )
            )
        )
        "#,
    )
    .expect("Failed to parse component WAT");

    let metadata = ComponentMetadata::new("test-component", "1.0.0");

    let mut component = WasmComponent::new(&engine, &minimal_component, metadata)
        .expect("Failed to create component");

    // Add an import with empty interface (should fail validation)
    component.add_import(ComponentImport {
        name: "empty-import".to_string(),
        interface: ComponentInterface::new("empty"),
    });

    let result = component.validate();
    assert!(result.is_err());
    match result {
        Err(ComponentError::ValidationError(msg)) => {
            assert!(msg.contains("empty-import"));
        }
        _ => panic!("Expected ValidationError"),
    }
}

#[test]
fn test_component_interface_type_system() {
    // Test primitive types
    let primitives = vec![
        InterfaceType::Bool,
        InterfaceType::S8,
        InterfaceType::U8,
        InterfaceType::S16,
        InterfaceType::U16,
        InterfaceType::S32,
        InterfaceType::U32,
        InterfaceType::S64,
        InterfaceType::U64,
        InterfaceType::F32,
        InterfaceType::F64,
        InterfaceType::Char,
        InterfaceType::String,
    ];

    for prim in &primitives {
        assert!(prim.is_compatible_with(prim));
    }

    // Test composite types
    let list_u32 = InterfaceType::List(Box::new(InterfaceType::U32));
    let list_u32_2 = InterfaceType::List(Box::new(InterfaceType::U32));
    assert!(list_u32.is_compatible_with(&list_u32_2));

    let option_string = InterfaceType::Option(Box::new(InterfaceType::String));
    let option_string_2 = InterfaceType::Option(Box::new(InterfaceType::String));
    assert!(option_string.is_compatible_with(&option_string_2));

    // Test result type
    let result_type = InterfaceType::Result {
        ok: Some(Box::new(InterfaceType::S32)),
        err: Some(Box::new(InterfaceType::String)),
    };
    let result_type_2 = InterfaceType::Result {
        ok: Some(Box::new(InterfaceType::S32)),
        err: Some(Box::new(InterfaceType::String)),
    };
    assert!(result_type.is_compatible_with(&result_type_2));
}

#[test]
fn test_component_resource_management() {
    let mut resource = ResourceDefinition::new("FileHandle", true);

    // Add resource methods
    resource.add_method(FunctionSignature::new(
        "read",
        vec![(
            "buffer".to_string(),
            InterfaceType::List(Box::new(InterfaceType::U8)),
        )],
        vec![InterfaceType::S32],
    ));

    resource.add_method(FunctionSignature::new(
        "write",
        vec![(
            "data".to_string(),
            InterfaceType::List(Box::new(InterfaceType::U8)),
        )],
        vec![InterfaceType::S32],
    ));

    resource.add_method(FunctionSignature::new(
        "close",
        vec![],
        vec![InterfaceType::Result {
            ok: None,
            err: Some(Box::new(InterfaceType::String)),
        }],
    ));

    assert_eq!(resource.methods.len(), 3);
    assert!(resource.owned);

    // Verify method names
    assert_eq!(resource.methods[0].name, "read");
    assert_eq!(resource.methods[1].name, "write");
    assert_eq!(resource.methods[2].name, "close");
}

#[cfg_attr(miri, ignore)]
#[test]
fn test_component_linker_dependency_resolution() {
    let mut linker = ComponentLinker::new();
    let engine = Engine::default();

    // Create three components: A, B, C where A depends on B, B depends on C
    let component_bytecode = wat::parse_str(
        r#"
        (component
            (core module $m
                (func (export "test") (result i32)
                    i32.const 1
                )
            )
        )
        "#,
    )
    .expect("Failed to parse component");

    let component_a = Arc::new(
        WasmComponent::new(
            &engine,
            &component_bytecode,
            ComponentMetadata::new("component-a", "1.0.0"),
        )
        .expect("Failed to create component A"),
    );

    let component_b = Arc::new(
        WasmComponent::new(
            &engine,
            &component_bytecode,
            ComponentMetadata::new("component-b", "1.0.0"),
        )
        .expect("Failed to create component B"),
    );

    let component_c = Arc::new(
        WasmComponent::new(
            &engine,
            &component_bytecode,
            ComponentMetadata::new("component-c", "1.0.0"),
        )
        .expect("Failed to create component C"),
    );

    // Register components
    linker
        .register_component(component_a)
        .expect("Failed to register A");
    linker
        .register_component(component_b)
        .expect("Failed to register B");
    linker
        .register_component(component_c)
        .expect("Failed to register C");

    // Add dependencies: A -> B -> C
    linker
        .add_dependency("component-a", "component-b")
        .expect("Failed to add A->B dependency");
    linker
        .add_dependency("component-b", "component-c")
        .expect("Failed to add B->C dependency");

    // Resolve dependencies for A (should return [C, B, A] in dependency order)
    let resolved = linker
        .resolve_dependencies("component-a")
        .expect("Failed to resolve dependencies");

    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0].metadata.name, "component-c");
    assert_eq!(resolved[1].metadata.name, "component-b");
    assert_eq!(resolved[2].metadata.name, "component-a");
}

#[test]
fn test_component_linker_circular_dependency_prevention() {
    let mut linker = ComponentLinker::new();

    // Setup: A -> B -> C
    linker
        .add_dependency("component-a", "component-b")
        .expect("Failed to add A->B");
    linker
        .add_dependency("component-b", "component-c")
        .expect("Failed to add B->C");

    // Try to add C -> A (would create a cycle)
    let result = linker.add_dependency("component-c", "component-a");

    assert!(result.is_err());
    match result {
        Err(ComponentError::CircularDependency(msg)) => {
            assert!(msg.contains("component-c") || msg.contains("component-a"));
        }
        _ => panic!("Expected CircularDependency error"),
    }
}

#[test]
fn test_component_interface_compatibility_checking() {
    // Create two compatible interfaces
    let mut iface1 = ComponentInterface::new("logger");
    iface1.add_function(FunctionSignature::new(
        "log",
        vec![
            ("level".to_string(), InterfaceType::S32),
            ("message".to_string(), InterfaceType::String),
        ],
        vec![],
    ));
    iface1.add_function(FunctionSignature::new(
        "flush",
        vec![],
        vec![InterfaceType::Result {
            ok: None,
            err: Some(Box::new(InterfaceType::String)),
        }],
    ));

    let mut iface2 = ComponentInterface::new("logger");
    iface2.add_function(FunctionSignature::new(
        "log",
        vec![
            ("severity".to_string(), InterfaceType::S32),
            ("msg".to_string(), InterfaceType::String),
        ],
        vec![],
    ));
    iface2.add_function(FunctionSignature::new(
        "flush",
        vec![],
        vec![InterfaceType::Result {
            ok: None,
            err: Some(Box::new(InterfaceType::String)),
        }],
    ));

    // Should be compatible (parameter names don't matter, only types)
    assert!(iface1.is_compatible_with(&iface2));

    // Create incompatible interface (different parameter types)
    let mut iface3 = ComponentInterface::new("logger");
    iface3.add_function(FunctionSignature::new(
        "log",
        vec![
            ("level".to_string(), InterfaceType::U32), // Changed from S32
            ("message".to_string(), InterfaceType::String),
        ],
        vec![],
    ));

    assert!(!iface1.is_compatible_with(&iface3));
}

#[cfg_attr(miri, ignore)]
#[test]
fn test_component_pre_instantiation() {
    let engine = Engine::default();
    let component_bytecode = wat::parse_str(
        r#"
        (component
            (core module $m
                (func (export "add") (param i32 i32) (result i32)
                    local.get 0
                    local.get 1
                    i32.add
                )
            )
            (core instance $i (instantiate $m))
            (func (export "add") (param "a" s32) (param "b" s32) (result s32)
                (canon lift (core func $i "add"))
            )
        )
        "#,
    )
    .expect("Failed to parse component");

    let mut component = WasmComponent::new(
        &engine,
        &component_bytecode,
        ComponentMetadata::new("math-component", "1.0.0"),
    )
    .expect("Failed to create component");

    // Create a linker
    let linker: Linker<()> = Linker::new(&engine);

    // Pre-instantiate the component
    let result = component.pre_instantiate(&linker);
    assert!(result.is_ok());
}

#[test]
fn test_component_metadata_builder() {
    let metadata = ComponentMetadata::new("my-component", "2.1.0")
        .with_description("A comprehensive test component")
        .add_author("Alice Developer")
        .add_author("Bob Engineer")
        .with_license("Apache-2.0");

    assert_eq!(metadata.name, "my-component");
    assert_eq!(metadata.version, "2.1.0");
    assert_eq!(
        metadata.description,
        Some("A comprehensive test component".to_string())
    );
    assert_eq!(metadata.authors.len(), 2);
    assert_eq!(metadata.authors[0], "Alice Developer");
    assert_eq!(metadata.authors[1], "Bob Engineer");
    assert_eq!(metadata.license, Some("Apache-2.0".to_string()));
}

#[cfg_attr(miri, ignore)]
#[test]
fn test_component_import_export_management() {
    let engine = Engine::default();
    let component_bytecode = wat::parse_str(
        r#"
        (component
            (core module $m
                (func (export "noop"))
            )
        )
        "#,
    )
    .expect("Failed to parse component");

    let mut component = WasmComponent::new(
        &engine,
        &component_bytecode,
        ComponentMetadata::new("test", "1.0.0"),
    )
    .expect("Failed to create component");

    // Add imports
    let mut import_interface = ComponentInterface::new("database");
    import_interface.add_function(FunctionSignature::new(
        "query",
        vec![("sql".to_string(), InterfaceType::String)],
        vec![InterfaceType::List(Box::new(InterfaceType::U8))],
    ));

    component.add_import(ComponentImport {
        name: "database".to_string(),
        interface: import_interface,
    });

    // Add exports
    let mut export_interface = ComponentInterface::new("api");
    export_interface.add_function(FunctionSignature::new(
        "handle_request",
        vec![("request".to_string(), InterfaceType::String)],
        vec![InterfaceType::String],
    ));

    component.add_export(ComponentExport {
        name: "api".to_string(),
        interface: export_interface,
    });

    // Verify imports and exports
    assert_eq!(component.imports.len(), 1);
    assert_eq!(component.exports.len(), 1);
    assert!(component.get_import("database").is_some());
    assert!(component.get_export("api").is_some());
    assert!(component.get_import("nonexistent").is_none());
}

#[test]
fn test_complex_interface_types() {
    // Test complex nested types
    let record_type = InterfaceType::Record(vec![
        ("id".to_string(), InterfaceType::U64),
        ("name".to_string(), InterfaceType::String),
        ("active".to_string(), InterfaceType::Bool),
        (
            "tags".to_string(),
            InterfaceType::List(Box::new(InterfaceType::String)),
        ),
    ]);

    let variant_type = InterfaceType::Variant(vec![
        ("Success".to_string(), Some(InterfaceType::S32)),
        ("Error".to_string(), Some(InterfaceType::String)),
        ("Pending".to_string(), None),
    ]);

    let flags_type = InterfaceType::Flags(vec![
        "READ".to_string(),
        "WRITE".to_string(),
        "EXECUTE".to_string(),
    ]);

    let enum_type = InterfaceType::Enum(vec![
        "Red".to_string(),
        "Green".to_string(),
        "Blue".to_string(),
    ]);

    // Create interface with complex types
    let mut interface = ComponentInterface::new("complex-api");
    interface.add_type("User", record_type.clone());
    interface.add_type("Status", variant_type);
    interface.add_type("Permissions", flags_type);
    interface.add_type("Color", enum_type);

    interface.add_function(FunctionSignature::new(
        "get_user",
        vec![("id".to_string(), InterfaceType::U64)],
        vec![InterfaceType::Option(Box::new(record_type))],
    ));

    // Verify type retrieval
    assert!(interface.get_type("User").is_some());
    assert!(interface.get_type("Status").is_some());
    assert!(interface.get_type("Permissions").is_some());
    assert!(interface.get_type("Color").is_some());
    assert!(interface.get_function("get_user").is_some());
}

#[cfg_attr(miri, ignore)]
#[test]
fn test_component_linker_multiple_components() {
    let mut linker = ComponentLinker::new();
    let engine = Engine::default();

    let component_bytecode = wat::parse_str(
        r#"
        (component
            (core module $m
                (func (export "f") (result i32)
                    i32.const 0
                )
            )
        )
        "#,
    )
    .expect("Failed to parse component");

    // Create and register multiple components
    for i in 0..5 {
        let component = Arc::new(
            WasmComponent::new(
                &engine,
                &component_bytecode,
                ComponentMetadata::new(format!("component-{}", i), "1.0.0"),
            )
            .expect("Failed to create component"),
        );
        linker
            .register_component(component)
            .expect("Failed to register component");
    }

    // Verify all components are registered
    let names = linker.component_names();
    assert_eq!(names.len(), 5);
    assert!(names.contains(&"component-0".to_string()));
    assert!(names.contains(&"component-4".to_string()));

    // Verify component retrieval
    for i in 0..5 {
        let name = format!("component-{}", i);
        assert!(linker.get_component(&name).is_some());
    }
}
