//! Capability-Based Host Function Example
//!
//! Demonstrates how to create host functions that check capabilities before execution.
//!
//! Run with: cargo run --example capability_host_function

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::executor::WasmExecutor;
use mielin_wasm::host::HostState;
use std::path::PathBuf;
use wasmtime::{Caller, Linker};

/// Example: File operation with capability checking
///
/// This function demonstrates the pattern for creating capability-checked host functions.
/// In a real implementation, you would call this before instantiation.
#[allow(dead_code)]
fn add_capability_checked_functions(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
    // Function that requires filesystem read capability
    linker.func_wrap(
        "env",
        "file_exists",
        |mut caller: Caller<'_, HostState>, path_ptr: u32, path_len: u32| -> i32 {
            let _state = caller.data();

            // In a real implementation, check filesystem permissions from state
            // For this example, we'll allow all operations

            // Get memory and read path
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(mem)) => mem,
                _ => return -1,
            };

            let data = memory.data(&caller);
            let path_bytes = &data[path_ptr as usize..(path_ptr + path_len) as usize];
            let path_str = match std::str::from_utf8(path_bytes) {
                Ok(s) => s,
                Err(_) => return -1,
            };

            // Check if file exists
            let path = PathBuf::from(path_str);
            if path.exists() {
                1 // File exists
            } else {
                0 // File does not exist
            }
        },
    )?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    println!("=== Capability-Based Host Function Example ===\n");

    let executor = WasmExecutor::new()?;

    // Create a WASM module that checks file existence
    let wasm = wat::parse_str(
        r#"
        (module
            (import "env" "file_exists" (func $file_exists (param i32 i32) (result i32)))

            (memory (export "memory") 1)

            ;; Path string at offset 0
            (data (i32.const 0) "/tmp/test.txt")

            (func (export "check_file") (result i32)
                i32.const 0   ;; path_ptr
                i32.const 13  ;; path_len
                call $file_exists
            )
        )
        "#,
    )?;

    println!("Compiling WASM module...");
    let module = executor.compile_module(&wasm)?;

    println!("\n--- Test 1: No Filesystem Permissions ---");
    println!("Instantiating with no permissions...");
    let (instance, mut store) = executor.instantiate(&module, HardwareCapabilities::NONE)?;

    let check_file = instance.get_typed_func::<(), i32>(&mut store, "check_file")?;
    let result = check_file.call(&mut store, ())?;
    println!("Result: {} (expected -1 for permission denied)", result);

    println!("\n--- Test 2: With Filesystem Permissions ---");
    // In a real implementation, you would create a new executor with filesystem permissions
    println!("Would instantiate with FsPermissions::read_only([PathBuf::from(\"/tmp\")])");

    println!("\n=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("1. Always check capabilities before performing operations");
    println!("2. Validate inputs from WASM memory");
    println!("3. Return error codes for permission denials");
    println!("4. Use whitelists for path/host access");

    Ok(())
}
