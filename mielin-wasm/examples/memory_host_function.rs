//! Memory Access Host Function Example
//!
//! Demonstrates how to create host functions that safely access WASM memory.
//!
//! Run with: cargo run --example memory_host_function

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::executor::WasmExecutor;
use mielin_wasm::host::HostState;
use wasmtime::{Caller, Linker, Memory};

/// Helper function to validate memory access
///
/// This demonstrates safe memory validation patterns for host functions.
#[allow(dead_code)]
fn validate_memory_access(
    memory: &Memory,
    caller: &Caller<'_, HostState>,
    ptr: u32,
    len: u32,
) -> Result<(), &'static str> {
    let memory_size = memory.data_size(caller);
    let end = ptr
        .checked_add(len)
        .ok_or("Integer overflow in memory calculation")?;

    if end as usize > memory_size {
        return Err("Memory access out of bounds");
    }

    Ok(())
}

/// Example: String manipulation host function
///
/// This function demonstrates the pattern for creating string manipulation host functions.
/// In a real implementation, you would call this before instantiation.
#[allow(dead_code)]
fn add_string_functions(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
    // Function to get string length from WASM memory
    linker.func_wrap(
        "env",
        "string_length",
        |mut caller: Caller<'_, HostState>, ptr: u32, max_len: u32| -> i32 {
            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(mem)) => mem,
                _ => return -1,
            };

            // Validate access
            if validate_memory_access(&memory, &caller, ptr, max_len).is_err() {
                return -1;
            }

            // Read string from memory
            let data = memory.data(&caller);
            let slice = &data[ptr as usize..(ptr + max_len) as usize];

            // Find null terminator
            let len = slice
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(max_len as usize);

            len as i32
        },
    )?;

    // Function to reverse a string in WASM memory
    linker.func_wrap(
        "env",
        "string_reverse",
        |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> i32 {
            // Get memory
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(mem)) => mem,
                _ => return -1,
            };

            // Validate access
            if validate_memory_access(&memory, &caller, ptr, len).is_err() {
                return -1;
            }

            // Get mutable access to memory
            let data = memory.data_mut(&mut caller);
            let slice = &mut data[ptr as usize..(ptr + len) as usize];

            // Reverse the string
            slice.reverse();

            0 // Success
        },
    )?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    println!("=== Memory Access Host Function Example ===\n");

    let executor = WasmExecutor::new()?;

    // Create a WASM module that uses string functions
    let wasm = wat::parse_str(
        r#"
        (module
            ;; Import host functions
            (import "env" "string_length" (func $string_length (param i32 i32) (result i32)))
            (import "env" "string_reverse" (func $string_reverse (param i32 i32) (result i32)))

            ;; Declare memory
            (memory (export "memory") 1)

            ;; Initialize with a string "Hello" at offset 0
            (data (i32.const 0) "Hello\00")

            ;; Function to demonstrate string operations
            (func (export "demo") (result i32)
                (local $len i32)

                ;; Get string length
                i32.const 0     ;; ptr
                i32.const 100   ;; max_len
                call $string_length
                local.set $len

                ;; Reverse the string
                i32.const 0     ;; ptr
                local.get $len  ;; len
                call $string_reverse

                ;; Return length
                local.get $len
            )
        )
        "#,
    )?;

    println!("Compiling WASM module...");
    let module = executor.compile_module(&wasm)?;

    println!("Instantiating module...");
    let (_instance, _store) = executor.instantiate(&module, HardwareCapabilities::NONE)?;

    // Note: In real implementation, add custom functions to linker before instantiation
    // The functions validate_memory_access and add_string_functions are defined above
    // as examples of how to implement safe memory access patterns.

    println!("\n=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("1. Always validate memory bounds before access");
    println!("2. Use checked arithmetic to prevent overflow");
    println!("3. Handle errors gracefully with Result types");
    println!("4. Prefer immutable access when possible");

    Ok(())
}
