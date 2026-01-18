//! Basic Host Function Example
//!
//! Demonstrates how to create a simple host function that can be called from WASM.
//!
//! Run with: cargo run --example basic_host_function

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::executor::WasmExecutor;
use mielin_wasm::host::HostState;
use wasmtime::{Caller, Linker};

/// Example: Add a custom greeting host function
///
/// This function demonstrates the pattern for creating custom host functions.
/// In a real implementation, you would call this before instantiation.
#[allow(dead_code)]
fn add_greeting_function(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
    // Define a simple host function that returns a greeting
    linker.func_wrap("env", "greet", |caller: Caller<'_, HostState>| -> i32 {
        println!("Hello from host function!");

        // Access host state
        let _state = caller.data();
        // Can access state.tensor_runtime() and state.system_state() here

        // Return success
        0
    })?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    println!("=== Basic Host Function Example ===\n");

    // Create executor
    let executor = WasmExecutor::new()?;

    // Create a WASM module that calls our host function
    let wasm = wat::parse_str(
        r#"
        (module
            ;; Import the host function
            (import "env" "greet" (func $greet (result i32)))

            ;; Export a function that calls the host function
            (func (export "call_greet") (result i32)
                call $greet
            )
        )
        "#,
    )?;

    println!("Compiling WASM module...");
    let module = executor.compile_module(&wasm)?;

    println!("Instantiating module...");
    let (instance, mut store) = executor.instantiate(&module, HardwareCapabilities::NONE)?;

    // Note: In a real implementation, you'd need to add the custom function to the linker
    // before instantiation. This example shows the concept.

    println!("\nCalling WASM function that invokes host function...");
    let call_greet = instance.get_typed_func::<(), i32>(&mut store, "call_greet")?;

    let result = call_greet.call(&mut store, ())?;
    println!("Result: {}", result);

    println!("\n=== Example Complete ===");
    Ok(())
}
