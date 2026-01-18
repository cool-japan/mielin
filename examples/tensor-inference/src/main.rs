//! Tensor Inference Example
//!
//! Demonstrates TensorLogic integration by running a simple neural network
//! inference task in a WASM agent. The agent performs matrix multiplication
//! for a single-layer feedforward network.

use anyhow::Result;
use mielin_cells::Agent;
use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::executor::WasmExecutor;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("MielinOS - Tensor Inference Example");
    info!("===================================");

    // Test multiple backends
    let backends = vec![
        ("Scalar", HardwareCapabilities::NONE),
        ("NEON", HardwareCapabilities::NEON),
        ("SVE2", HardwareCapabilities::SVE2),
        ("AVX2", HardwareCapabilities::AVX2),
    ];

    for (name, capabilities) in backends {
        info!("\n--- Testing with {} backend ---", name);

        // Create WASM executor with tensor support
        let executor = WasmExecutor::with_capabilities(capabilities)?;

        // Create a simple neural network inference agent
        let agent = create_inference_agent()?;

        info!("Agent ID: {}", agent.id());

        // Execute the agent
        let result = executor.execute_with_capabilities(&agent, capabilities)?;

        info!("✓ Inference completed with {} backend", name);
        info!("  Exit code: {}", result.exit_code);
    }

    info!("\n✓ All backends tested successfully!");

    Ok(())
}

/// Creates a WASM agent that performs simple neural network inference
fn create_inference_agent() -> Result<Agent> {
    // WAT (WebAssembly Text) code for a simple feedforward network
    // Network: 3 inputs -> 2 hidden nodes -> 1 output
    let wasm_code = wat::parse_str(
        r#"
        (module
            ;; Import tensor operations
            (import "mielin" "tensor_zeros" (func $zeros (param i32 i32) (result i32)))
            (import "mielin" "tensor_matmul" (func $matmul (param i32 i32) (result i32)))
            (import "mielin" "tensor_free" (func $free (param i32) (result i32)))
            (import "mielin" "tensor_supports_neon" (func $neon (result i32)))
            (import "mielin" "tensor_supports_sve2" (func $sve2 (result i32)))
            (import "mielin" "tensor_supports_avx2" (func $avx2 (result i32)))

            (memory (export "memory") 1)

            ;; Main inference function
            (func (export "_start")
                (local $input i32)
                (local $weights1 i32)
                (local $weights2 i32)
                (local $hidden i32)
                (local $output i32)
                (local $backend i32)

                ;; Detect backend
                call $sve2
                if (result i32)
                    i32.const 1  ;; SVE2
                else
                    call $neon
                    if (result i32)
                        i32.const 2  ;; NEON
                    else
                        call $avx2
                        if (result i32)
                            i32.const 3  ;; AVX2
                        else
                            i32.const 0  ;; Scalar
                        end
                    end
                end
                local.set $backend

                ;; Create input vector [3x1] at offset 0
                i32.const 0
                i32.const 3
                i32.store
                i32.const 4
                i32.const 1
                i32.store

                i32.const 0
                i32.const 2
                call $zeros
                local.set $input

                ;; Create weight matrix 1 [2x3] at offset 16
                i32.const 16
                i32.const 2
                i32.store
                i32.const 20
                i32.const 3
                i32.store

                i32.const 16
                i32.const 2
                call $zeros
                local.set $weights1

                ;; Create weight matrix 2 [1x2] at offset 32
                i32.const 32
                i32.const 1
                i32.store
                i32.const 36
                i32.const 2
                i32.store

                i32.const 32
                i32.const 2
                call $zeros
                local.set $weights2

                ;; Forward pass: hidden = weights1 * input
                local.get $weights1
                local.get $input
                call $matmul
                local.set $hidden

                ;; Output layer: output = weights2 * hidden
                local.get $weights2
                local.get $hidden
                call $matmul
                local.set $output

                ;; Store backend indicator at offset 64
                i32.const 64
                local.get $backend
                i32.store

                ;; Clean up tensors
                local.get $input
                call $free
                drop

                local.get $weights1
                call $free
                drop

                local.get $weights2
                call $free
                drop

                local.get $hidden
                call $free
                drop

                local.get $output
                call $free
                drop
            )
        )
        "#,
    )?;

    // Create agent with the compiled WASM binary
    Ok(Agent::new(wasm_code))
}
