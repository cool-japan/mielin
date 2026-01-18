//! Custom WASM Extensions Example
//!
//! This example demonstrates how to create and use custom WASM extensions
//! to add your own host functions to the runtime.

use mielin_wasm::extensions::{
    Extension, ExtensionBuilder, ExtensionFunction, ExtensionInfo, ExtensionRegistry,
    FunctionSignature,
};

fn main() {
    println!("=== Custom WASM Extensions Example ===\n");

    // Part 1: Create a simple extension using the builder
    println!("Part 1: Creating a Math Extension");
    println!("-----------------------------------");

    let math_extension = ExtensionBuilder::new("math-ops")
        .version("1.0.0")
        .description("Mathematical operations extension")
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
        .function(
            ExtensionFunction::new("square", FunctionSignature::I32ToI32)
                .description("Square a number"),
        )
        .build();

    println!("Extension: {}", math_extension.info().name);
    println!("Version: {}", math_extension.info().version);
    println!("Author: {}", math_extension.info().author);
    println!("Functions:");
    for func in math_extension.functions() {
        println!(
            "  - {} {}: {}",
            func.name,
            func.signature.as_str(),
            func.description
        );
    }
    println!();

    // Part 2: Create a string utilities extension
    println!("Part 2: Creating a String Utilities Extension");
    println!("----------------------------------------------");

    let string_extension = ExtensionBuilder::new("string-utils")
        .version("2.0.0")
        .description("String manipulation utilities")
        .author("MielinOS Team")
        .namespace("str")
        .function(
            ExtensionFunction::new("strlen", FunctionSignature::I32ToI32)
                .description("Get string length"),
        )
        .function(
            ExtensionFunction::new("to_upper", FunctionSignature::I32ToI32)
                .description("Convert string to uppercase"),
        )
        .build();

    println!("Extension: {}", string_extension.info().name);
    println!("Functions: {}", string_extension.functions().count());
    println!();

    // Part 3: Create an extension registry
    println!("Part 3: Extension Registry");
    println!("--------------------------");

    let mut registry = ExtensionRegistry::new();

    registry
        .register(math_extension)
        .expect("Failed to register math extension");
    registry
        .register(string_extension)
        .expect("Failed to register string extension");

    println!("Registered extensions: {}", registry.count());
    println!("Extension names:");
    for name in registry.list_names() {
        println!("  - {}", name);
    }
    println!();

    // Part 4: Query extensions
    println!("Part 4: Querying Extensions");
    println!("----------------------------");

    if let Some(math_ext) = registry.get("math-ops") {
        println!("Found math-ops extension:");
        println!("  Version: {}", math_ext.info().version);
        println!("  Description: {}", math_ext.info().description);
        println!("  Namespace: {}", math_ext.info().namespace);

        if let Some(add_func) = math_ext.get_function("add") {
            println!("\n  Function 'add':");
            println!("    Signature: {}", add_func.signature.as_str());
            println!("    Description: {}", add_func.description);
            println!("    Enabled: {}", add_func.enabled);
        }
    }
    println!();

    // Part 5: Create a custom extension with state
    println!("Part 5: Extension with State");
    println!("-----------------------------");

    let info = ExtensionInfo::new("counter")
        .version("1.0.0")
        .description("Counter extension with state")
        .namespace("counter");

    let counter_ext = Extension::new(info);

    // Set some state
    counter_ext
        .set_state("count".to_string(), Box::new(0i32))
        .expect("Failed to set state");

    println!("Counter extension created");
    println!(
        "Has 'count' state: {}",
        counter_ext
            .has_state("count")
            .expect("Failed to check state")
    );
    println!(
        "Has 'other' state: {}",
        counter_ext
            .has_state("other")
            .expect("Failed to check state")
    );
    println!();

    // Part 6: Function signatures
    println!("Part 6: Function Signature Types");
    println!("---------------------------------");

    let signatures = vec![
        ("void", FunctionSignature::Void),
        ("get_value", FunctionSignature::I32),
        ("get_time", FunctionSignature::I64),
        ("get_pi", FunctionSignature::F32),
        ("get_e", FunctionSignature::F64),
        ("double", FunctionSignature::I32ToI32),
        ("add", FunctionSignature::I32I32ToI32),
        ("to_i64", FunctionSignature::I32ToI64),
    ];

    println!("Available function signatures:");
    for (name, sig) in signatures {
        println!("  {:<15} -> {}", name, sig.as_str());
    }
    println!();

    // Part 7: Disabled functions
    println!("Part 7: Disabling Functions");
    println!("---------------------------");

    let debug_ext = ExtensionBuilder::new("debug")
        .function(
            ExtensionFunction::new("log", FunctionSignature::I32ToI32)
                .description("Log a message")
                .enabled(true),
        )
        .function(
            ExtensionFunction::new("trace", FunctionSignature::I32ToI32)
                .description("Trace execution")
                .enabled(false), // Disabled
        )
        .build();

    println!("Debug extension functions:");
    for func in debug_ext.functions() {
        println!("  - {}: enabled={}", func.name, func.enabled);
    }
    println!();

    println!("=== Example Complete ===");
}
