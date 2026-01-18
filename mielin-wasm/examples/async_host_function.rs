//! Async Host Function Example
//!
//! Demonstrates how to create async host functions with cooperative yielding.
//!
//! Run with: cargo run --example async_host_function

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::async_exec::{CooperativeScheduler, TimeoutConfig};
use mielin_wasm::executor::WasmExecutor;
use mielin_wasm::host::HostState;
use wasmtime::{Caller, Linker};

/// Example: Long-running computation with cooperative yielding
///
/// This function demonstrates the pattern for creating async host functions.
/// In a real implementation, you would call this before instantiation.
#[allow(dead_code)]
fn add_async_functions(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
    // Function that yields periodically during long computation
    linker.func_wrap(
        "env",
        "long_computation",
        |_caller: Caller<'_, HostState>, iterations: u32| -> i32 {
            println!("Starting long computation with {} iterations", iterations);

            let mut result = 0;
            for i in 0..iterations {
                // Simulate work
                result += i;

                // Yield every 1000 iterations
                if i % 1000 == 0 {
                    println!("  Progress: {}/{}", i, iterations);
                    // In real implementation, would call async yield point
                }
            }

            println!("Computation complete!");
            result as i32
        },
    )?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    println!("=== Async Host Function Example ===\n");

    let executor = WasmExecutor::new()?;

    // Create a WASM module with long-running computation
    let wasm = wat::parse_str(
        r#"
        (module
            (import "env" "long_computation" (func $long_computation (param i32) (result i32)))

            (func (export "compute") (result i32)
                ;; Perform a long computation
                i32.const 10000
                call $long_computation
            )

            (func (export "loop_test") (result i32)
                (local $i i32)
                (local $sum i32)

                ;; Initialize
                i32.const 0
                local.set $i
                i32.const 0
                local.set $sum

                ;; Loop with cooperative yielding
                (block $break
                    (loop $continue
                        ;; Check if done
                        local.get $i
                        i32.const 1000
                        i32.ge_u
                        br_if $break

                        ;; Add to sum
                        local.get $sum
                        local.get $i
                        i32.add
                        local.set $sum

                        ;; Increment counter
                        local.get $i
                        i32.const 1
                        i32.add
                        local.set $i

                        ;; Continue loop
                        br $continue
                    )
                )

                local.get $sum
            )
        )
        "#,
    )?;

    println!("Compiling WASM module...");
    let module = executor.compile_module(&wasm)?;

    println!("Instantiating module...");
    let (instance, mut store) = executor.instantiate(&module, HardwareCapabilities::NONE)?;

    println!("\n--- Test 1: Long Computation ---");
    let compute = instance.get_typed_func::<(), i32>(&mut store, "compute")?;
    let result = compute.call(&mut store, ())?;
    println!("Result: {}", result);

    println!("\n--- Test 2: Loop with Yielding ---");
    let loop_test = instance.get_typed_func::<(), i32>(&mut store, "loop_test")?;
    let result = loop_test.call(&mut store, ())?;
    println!("Result: {}", result);

    println!("\n--- Async Context Configuration ---");
    let timeout_config = TimeoutConfig::interactive();
    println!("Interactive timeout: {:?}", timeout_config);

    let scheduler_config = CooperativeScheduler::new();
    println!("Scheduler config: {:?}", scheduler_config);

    println!("\n=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("1. Long-running operations should yield periodically");
    println!("2. Use TimeoutConfig to prevent runaway executions");
    println!("3. CooperativeScheduler manages execution quanta");
    println!("4. Fuel metering provides additional safety");

    Ok(())
}
