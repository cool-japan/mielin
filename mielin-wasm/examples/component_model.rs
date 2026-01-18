//! Component Model Example
//!
//! Demonstrates how to use the WebAssembly Component Model for composable modules.
//!
//! Run with: cargo run --example component_model

use mielin_wasm::component::{
    ComponentExport, ComponentImport, ComponentInterface, ComponentLinker, ComponentMetadata,
    FunctionSignature, InterfaceType, ResourceDefinition, WasmComponent,
};
use std::sync::Arc;
use wasmtime::Engine;

fn main() -> anyhow::Result<()> {
    println!("=== WebAssembly Component Model Example ===\n");

    // Initialize the engine
    let engine = Engine::default();

    println!("--- Part 1: Interface Type System ---\n");
    demonstrate_interface_types();

    println!("\n--- Part 2: Component Interfaces ---\n");
    demonstrate_component_interfaces();

    println!("\n--- Part 3: Resource Management ---\n");
    demonstrate_resources();

    println!("\n--- Part 4: Component Creation ---\n");
    demonstrate_component_creation(&engine)?;

    println!("\n--- Part 5: Component Linking ---\n");
    demonstrate_component_linking(&engine)?;

    println!("\n=== Example Complete ===");
    Ok(())
}

fn demonstrate_interface_types() {
    println!("Primitive Types:");
    let primitives = vec![
        InterfaceType::Bool,
        InterfaceType::S32,
        InterfaceType::U64,
        InterfaceType::F32,
        InterfaceType::String,
    ];

    for prim in &primitives {
        println!("  - {}", prim.name());
    }

    println!("\nComposite Types:");

    // List type
    let list_u8 = InterfaceType::List(Box::new(InterfaceType::U8));
    println!("  - List: {}", list_u8.name());

    // Option type
    let option_string = InterfaceType::Option(Box::new(InterfaceType::String));
    println!("  - Option: {}", option_string.name());

    // Tuple type
    let tuple = InterfaceType::Tuple(vec![InterfaceType::S32, InterfaceType::String]);
    println!("  - Tuple: {}", tuple.name());

    // Record type
    let record = InterfaceType::Record(vec![
        ("id".to_string(), InterfaceType::U64),
        ("name".to_string(), InterfaceType::String),
        ("active".to_string(), InterfaceType::Bool),
    ]);
    println!("  - Record: {}", record.name());

    // Result type
    let result_type = InterfaceType::Result {
        ok: Some(Box::new(InterfaceType::S32)),
        err: Some(Box::new(InterfaceType::String)),
    };
    println!("  - Result: {}", result_type.name());

    // Verify compatibility
    let list_u8_2 = InterfaceType::List(Box::new(InterfaceType::U8));
    println!("\nType Compatibility:");
    println!(
        "  list<u8> compatible with list<u8>: {}",
        list_u8.is_compatible_with(&list_u8_2)
    );
}

fn demonstrate_component_interfaces() {
    let mut logger_interface = ComponentInterface::new("logger");

    // Add functions
    logger_interface.add_function(FunctionSignature::new(
        "log",
        vec![
            ("level".to_string(), InterfaceType::S32),
            ("message".to_string(), InterfaceType::String),
        ],
        vec![],
    ));

    logger_interface.add_function(FunctionSignature::new(
        "flush",
        vec![],
        vec![InterfaceType::Result {
            ok: None,
            err: Some(Box::new(InterfaceType::String)),
        }],
    ));

    println!("Logger Interface:");
    println!("  Name: {}", logger_interface.name);
    println!("  Functions:");
    for func in &logger_interface.functions {
        println!("    - {} with {} params", func.name, func.params.len());
    }

    // Add custom types
    let log_level_enum = InterfaceType::Enum(vec![
        "Debug".to_string(),
        "Info".to_string(),
        "Warning".to_string(),
        "Error".to_string(),
    ]);
    logger_interface.add_type("LogLevel", log_level_enum);

    println!("  Custom Types:");
    for name in logger_interface.types.keys() {
        println!("    - {}", name);
    }
}

fn demonstrate_resources() {
    let mut file_handle = ResourceDefinition::new("FileHandle", true);

    // Add resource methods
    file_handle.add_method(FunctionSignature::new(
        "read",
        vec![("count".to_string(), InterfaceType::U64)],
        vec![InterfaceType::List(Box::new(InterfaceType::U8))],
    ));

    file_handle.add_method(FunctionSignature::new(
        "write",
        vec![(
            "data".to_string(),
            InterfaceType::List(Box::new(InterfaceType::U8)),
        )],
        vec![InterfaceType::S32],
    ));

    file_handle.add_method(FunctionSignature::new(
        "close",
        vec![],
        vec![InterfaceType::Result {
            ok: None,
            err: Some(Box::new(InterfaceType::String)),
        }],
    ));

    println!("FileHandle Resource:");
    println!("  Owned: {}", file_handle.owned);
    println!("  Methods:");
    for method in &file_handle.methods {
        println!(
            "    - {} ({} params, {} results)",
            method.name,
            method.params.len(),
            method.results.len()
        );
    }
}

fn demonstrate_component_creation(engine: &Engine) -> anyhow::Result<()> {
    // Create a minimal component
    let component_wat = r#"
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
    "#;

    let bytecode = wat::parse_str(component_wat)?;

    let metadata = ComponentMetadata::new("math-component", "1.0.0")
        .with_description("A simple math component")
        .add_author("Example Developer")
        .with_license("MIT");

    println!("Creating Component:");
    println!("  Name: {}", metadata.name);
    println!("  Version: {}", metadata.version);
    println!("  Description: {:?}", metadata.description);
    println!("  Authors: {:?}", metadata.authors);
    println!("  License: {:?}", metadata.license);

    let mut component = WasmComponent::new(engine, &bytecode, metadata)?;

    // Add exports
    let mut math_interface = ComponentInterface::new("math");
    math_interface.add_function(FunctionSignature::new(
        "add",
        vec![
            ("a".to_string(), InterfaceType::S32),
            ("b".to_string(), InterfaceType::S32),
        ],
        vec![InterfaceType::S32],
    ));

    component.add_export(ComponentExport {
        name: "math".to_string(),
        interface: math_interface,
    });

    // Validate
    component.validate()?;
    println!("  Validation: OK");
    println!("  Exports: {}", component.exports.len());

    Ok(())
}

fn demonstrate_component_linking(engine: &Engine) -> anyhow::Result<()> {
    let mut linker = ComponentLinker::new();

    // Create component A (application)
    let app_component = create_app_component(engine)?;
    linker.register_component(Arc::new(app_component))?;

    // Create component B (logger)
    let logger_component = create_logger_component(engine)?;
    linker.register_component(Arc::new(logger_component))?;

    // Create component C (database)
    let db_component = create_database_component(engine)?;
    linker.register_component(Arc::new(db_component))?;

    println!("Registered Components:");
    for name in linker.component_names() {
        println!("  - {}", name);
    }

    // Add dependencies: app -> logger, app -> database
    linker.add_dependency("app", "logger")?;
    linker.add_dependency("app", "database")?;
    println!("\nDependencies:");
    println!("  app -> logger");
    println!("  app -> database");

    // Resolve dependencies
    let resolved = linker.resolve_dependencies("app")?;
    println!("\nDependency Resolution Order:");
    for (i, comp) in resolved.iter().enumerate() {
        println!("  {}. {}", i + 1, comp.metadata.name);
    }

    // Demonstrate circular dependency prevention
    println!("\nCircular Dependency Prevention:");
    match linker.add_dependency("logger", "app") {
        Ok(_) => println!("  ERROR: Should have detected circular dependency!"),
        Err(e) => println!("  Correctly rejected: {}", e),
    }

    Ok(())
}

fn create_app_component(engine: &Engine) -> anyhow::Result<WasmComponent> {
    let component_wat = r#"
        (component
            (core module $m
                (func (export "run") (result i32)
                    i32.const 0
                )
            )
        )
    "#;

    let bytecode = wat::parse_str(component_wat)?;
    let metadata =
        ComponentMetadata::new("app", "1.0.0").with_description("Main application component");

    let mut component = WasmComponent::new(engine, &bytecode, metadata)?;

    // Add imports for logger and database
    let mut logger_interface = ComponentInterface::new("logger");
    logger_interface.add_function(FunctionSignature::new(
        "log",
        vec![("message".to_string(), InterfaceType::String)],
        vec![],
    ));

    component.add_import(ComponentImport {
        name: "logger".to_string(),
        interface: logger_interface,
    });

    Ok(component)
}

fn create_logger_component(engine: &Engine) -> anyhow::Result<WasmComponent> {
    let component_wat = r#"
        (component
            (core module $m
                (func (export "log") (result i32)
                    i32.const 0
                )
            )
        )
    "#;

    let bytecode = wat::parse_str(component_wat)?;
    let metadata = ComponentMetadata::new("logger", "1.0.0").with_description("Logging component");

    let mut component = WasmComponent::new(engine, &bytecode, metadata)?;

    // Add exports
    let mut logger_interface = ComponentInterface::new("logger");
    logger_interface.add_function(FunctionSignature::new(
        "log",
        vec![("message".to_string(), InterfaceType::String)],
        vec![],
    ));

    component.add_export(ComponentExport {
        name: "logger".to_string(),
        interface: logger_interface,
    });

    Ok(component)
}

fn create_database_component(engine: &Engine) -> anyhow::Result<WasmComponent> {
    let component_wat = r#"
        (component
            (core module $m
                (func (export "query") (result i32)
                    i32.const 0
                )
            )
        )
    "#;

    let bytecode = wat::parse_str(component_wat)?;
    let metadata =
        ComponentMetadata::new("database", "1.0.0").with_description("Database component");

    let mut component = WasmComponent::new(engine, &bytecode, metadata)?;

    // Add exports
    let mut db_interface = ComponentInterface::new("database");
    db_interface.add_function(FunctionSignature::new(
        "query",
        vec![("sql".to_string(), InterfaceType::String)],
        vec![InterfaceType::List(Box::new(InterfaceType::U8))],
    ));

    component.add_export(ComponentExport {
        name: "database".to_string(),
        interface: db_interface,
    });

    Ok(component)
}
