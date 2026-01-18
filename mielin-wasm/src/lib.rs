//! MielinWasm - Wasmtime Runtime Integration
//!
//! Provides WebAssembly sandboxing and execution for agent cells.

pub mod aot;
pub mod async_exec;
pub mod cache;
pub mod component;
pub mod debug;
pub mod deterministic;
pub mod executor;
pub mod extensions;
pub mod filesystem;
pub mod host;
pub mod jit;
pub mod memory;
pub mod module;
pub mod network;
pub mod resource;
pub mod runtime;
pub mod sandbox;
pub mod security;
pub mod system;
pub mod validation;
pub mod verification;
pub mod wasi;
pub mod wasi_debug;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid module: {0}")]
    InvalidModule(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::WasmExecutor;
    use mielin_hal::capabilities::HardwareCapabilities;

    #[test]
    fn test_wasm_error_types() {
        let err = WasmError::InvalidModule("test".to_string());
        assert!(err.to_string().contains("Invalid module"));
    }

    #[test]
    fn test_tensor_host_functions() {
        // Create executor with tensor support
        let executor = WasmExecutor::with_capabilities(HardwareCapabilities::NEON).unwrap();

        // Create a simple WASM module that uses tensor operations
        let wasm = wat::parse_str(
            r#"
            (module
                (import "mielin" "tensor_supports_neon" (func $tensor_supports_neon (result i32)))
                (import "mielin" "tensor_zeros" (func $tensor_zeros (param i32 i32) (result i32)))
                (import "mielin" "tensor_free" (func $tensor_free (param i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "_start")
                    ;; Check NEON support
                    call $tensor_supports_neon
                    drop

                    ;; Create a 2x2 tensor - shape is at offset 0
                    i32.const 0
                    i32.const 2
                    i32.store
                    i32.const 4
                    i32.const 2
                    i32.store

                    ;; Call tensor_zeros with shape pointer and length
                    i32.const 0
                    i32.const 2
                    call $tensor_zeros

                    ;; Free the tensor
                    call $tensor_free
                    drop
                )
            )
            "#,
        )
        .unwrap();

        let module = executor.compile_module(&wasm).unwrap();
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NEON)
            .unwrap();

        // Execute the start function
        let start = instance.get_func(&mut store, "_start").unwrap();
        start.call(&mut store, &[], &mut []).unwrap();

        // Verify host state is accessible
        assert!(store.data().tensor_runtime().supports_neon());
    }

    #[test]
    fn test_tensor_operations_integration() {
        // Create a WASM module that performs tensor operations
        let wasm = wat::parse_str(
            r#"
            (module
                (import "mielin" "tensor_zeros" (func $tensor_zeros (param i32 i32) (result i32)))
                (import "mielin" "tensor_ones" (func $tensor_ones (param i32 i32) (result i32)))
                (import "mielin" "tensor_add" (func $tensor_add (param i32 i32) (result i32)))
                (import "mielin" "tensor_free" (func $tensor_free (param i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "_start") (local i32) (local i32) (local i32)
                    ;; Setup shape [3] at offset 0
                    i32.const 0
                    i32.const 3
                    i32.store

                    ;; Create zeros tensor
                    i32.const 0
                    i32.const 1
                    call $tensor_zeros
                    local.set 0

                    ;; Create ones tensor
                    i32.const 0
                    i32.const 1
                    call $tensor_ones
                    local.set 1

                    ;; Add them
                    local.get 0
                    local.get 1
                    call $tensor_add
                    local.set 2

                    ;; Free all tensors
                    local.get 0
                    call $tensor_free
                    drop
                    local.get 1
                    call $tensor_free
                    drop
                    local.get 2
                    call $tensor_free
                    drop
                )
            )
            "#,
        )
        .unwrap();

        let executor = WasmExecutor::new().unwrap();
        let module = executor.compile_module(&wasm).unwrap();
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let start = instance.get_func(&mut store, "_start").unwrap();
        start.call(&mut store, &[], &mut []).unwrap();

        // All tensors should be freed
        let tensors = store.data().tensors().lock().unwrap();
        assert!(tensors.is_empty());
    }

    #[test]
    fn test_time_host_functions() {
        // Create a WASM module that uses time functions
        let wasm = wat::parse_str(
            r#"
            (module
                (import "mielin" "time_now_millis" (func $time_now_millis (result i64)))
                (import "mielin" "time_monotonic_nanos" (func $time_monotonic_nanos (result i64)))
                (memory (export "memory") 1)
                (global $time1 (mut i64) (i64.const 0))
                (global $time2 (mut i64) (i64.const 0))
                (func (export "_start")
                    ;; Get current time in millis (should be non-zero)
                    call $time_now_millis
                    global.set $time1

                    ;; Get monotonic time (should be small positive)
                    call $time_monotonic_nanos
                    global.set $time2
                )
                (func (export "get_time1") (result i64)
                    global.get $time1
                )
                (func (export "get_time2") (result i64)
                    global.get $time2
                )
            )
            "#,
        )
        .unwrap();

        let executor = WasmExecutor::new().unwrap();
        let module = executor.compile_module(&wasm).unwrap();
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let start = instance.get_func(&mut store, "_start").unwrap();
        start.call(&mut store, &[], &mut []).unwrap();

        // Verify time values
        let get_time1 = instance
            .get_typed_func::<(), i64>(&mut store, "get_time1")
            .unwrap();
        let time1 = get_time1.call(&mut store, ()).unwrap();
        assert!(time1 > 0, "time_now_millis should return positive value");

        let get_time2 = instance
            .get_typed_func::<(), i64>(&mut store, "get_time2")
            .unwrap();
        let time2 = get_time2.call(&mut store, ()).unwrap();
        assert!(
            time2 >= 0,
            "time_monotonic_nanos should return non-negative"
        );
    }

    #[test]
    fn test_random_host_functions() {
        // Create a WASM module that uses random functions
        let wasm = wat::parse_str(
            r#"
            (module
                (import "mielin" "random_u32" (func $random_u32 (result i32)))
                (import "mielin" "random_f32" (func $random_f32 (result f32)))
                (import "mielin" "random_bytes" (func $random_bytes (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (global $rand1 (mut i32) (i32.const 0))
                (global $rand2 (mut i32) (i32.const 0))
                (global $randf (mut f32) (f32.const 0))
                (func (export "_start")
                    ;; Get two random u32 values
                    call $random_u32
                    global.set $rand1
                    call $random_u32
                    global.set $rand2

                    ;; Get random f32
                    call $random_f32
                    global.set $randf

                    ;; Fill 16 bytes with random data at offset 100
                    i32.const 100
                    i32.const 16
                    call $random_bytes
                    drop
                )
                (func (export "get_rand1") (result i32)
                    global.get $rand1
                )
                (func (export "get_rand2") (result i32)
                    global.get $rand2
                )
                (func (export "get_randf") (result f32)
                    global.get $randf
                )
            )
            "#,
        )
        .unwrap();

        let executor = WasmExecutor::new().unwrap();
        let module = executor.compile_module(&wasm).unwrap();
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let start = instance.get_func(&mut store, "_start").unwrap();
        start.call(&mut store, &[], &mut []).unwrap();

        // Random values should be generated (may be zero by chance, but unlikely both)
        let get_rand1 = instance
            .get_typed_func::<(), i32>(&mut store, "get_rand1")
            .unwrap();
        let get_rand2 = instance
            .get_typed_func::<(), i32>(&mut store, "get_rand2")
            .unwrap();
        let r1 = get_rand1.call(&mut store, ()).unwrap();
        let r2 = get_rand2.call(&mut store, ()).unwrap();

        // Two consecutive random calls should produce different values (very high probability)
        // Note: This could theoretically fail, but probability is 1/2^32
        assert!(
            r1 != r2 || r1 == 0,
            "Two random values should likely differ"
        );

        // Check f32 is in [0, 1)
        let get_randf = instance
            .get_typed_func::<(), f32>(&mut store, "get_randf")
            .unwrap();
        let f = get_randf.call(&mut store, ()).unwrap();
        assert!((0.0..1.0).contains(&f), "random_f32 should be in [0, 1)");
    }

    #[test]
    fn test_process_host_functions() {
        // Create a WASM module that queries process info
        let wasm = wat::parse_str(
            r#"
            (module
                (import "mielin" "process_platform" (func $process_platform (result i32)))
                (import "mielin" "process_arch" (func $process_arch (result i32)))
                (import "mielin" "process_cpu_count" (func $process_cpu_count (result i32)))
                (import "mielin" "process_pointer_bits" (func $process_pointer_bits (result i32)))
                (memory (export "memory") 1)
                (global $platform (mut i32) (i32.const 0))
                (global $arch (mut i32) (i32.const 0))
                (global $cpus (mut i32) (i32.const 0))
                (global $bits (mut i32) (i32.const 0))
                (func (export "_start")
                    call $process_platform
                    global.set $platform
                    call $process_arch
                    global.set $arch
                    call $process_cpu_count
                    global.set $cpus
                    call $process_pointer_bits
                    global.set $bits
                )
                (func (export "get_platform") (result i32)
                    global.get $platform
                )
                (func (export "get_arch") (result i32)
                    global.get $arch
                )
                (func (export "get_cpus") (result i32)
                    global.get $cpus
                )
                (func (export "get_bits") (result i32)
                    global.get $bits
                )
            )
            "#,
        )
        .unwrap();

        let executor = WasmExecutor::new().unwrap();
        let module = executor.compile_module(&wasm).unwrap();
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let start = instance.get_func(&mut store, "_start").unwrap();
        start.call(&mut store, &[], &mut []).unwrap();

        // Verify process info
        let get_cpus = instance
            .get_typed_func::<(), i32>(&mut store, "get_cpus")
            .unwrap();
        let cpus = get_cpus.call(&mut store, ()).unwrap();
        assert!(cpus >= 1, "CPU count should be at least 1");

        let get_bits = instance
            .get_typed_func::<(), i32>(&mut store, "get_bits")
            .unwrap();
        let bits = get_bits.call(&mut store, ()).unwrap();
        assert!(bits == 32 || bits == 64, "Pointer bits should be 32 or 64");
    }
}
