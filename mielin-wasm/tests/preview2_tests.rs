//! Integration tests for the WASI Preview 2 / Component Model executor.
//!
//! These tests are compiled and run only when the `preview2` feature is
//! enabled:
//!   cargo nextest run -p mielin-wasm --features preview2

#![cfg(feature = "preview2")]

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::preview2::ComponentExecutor;

/// The minimal component WAT below:
///   (component
///     (core module (func (export "_start")))
///     (core instance (instantiate 0))
///   )
/// produces a valid Component Model binary that imports nothing and exports
/// nothing from the component level.  `wasmtime` 43 can instantiate it
/// without satisfying any WASI imports.
fn minimal_component_wat() -> &'static str {
    r#"
    (component
        (core module
            (func (export "_start"))
        )
        (core instance (instantiate 0))
    )
    "#
}

#[tokio::test]
async fn test_component_executor_creation() {
    let executor = ComponentExecutor::new(HardwareCapabilities::NONE);
    assert!(
        executor.is_ok(),
        "ComponentExecutor should be created successfully"
    );
}

#[tokio::test]
async fn test_compile_minimal_component() {
    let wasm_bytes = wat::parse_str(minimal_component_wat()).expect("WAT parse failed");

    let executor = ComponentExecutor::new(HardwareCapabilities::NONE)
        .expect("ComponentExecutor creation failed");

    let component = executor
        .compile(&wasm_bytes)
        .expect("Component compilation failed");

    let result = executor
        .execute(&component)
        .await
        .expect("Component execution failed");

    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_compile_invalid_bytes_fails() {
    let executor = ComponentExecutor::new(HardwareCapabilities::NONE)
        .expect("ComponentExecutor creation failed");

    let result = executor.compile(b"not valid wasm");
    assert!(result.is_err(), "Should fail on invalid bytes");
}

#[tokio::test]
async fn test_multiple_sequential_executions() {
    let wasm_bytes = wat::parse_str(minimal_component_wat()).expect("WAT parse failed");

    let executor = ComponentExecutor::new(HardwareCapabilities::NONE)
        .expect("ComponentExecutor creation failed");

    let component = executor
        .compile(&wasm_bytes)
        .expect("Component compilation failed");

    // Each execution must produce an isolated context (fresh WasiCtx + ResourceTable).
    for _ in 0..3 {
        let result = executor
            .execute(&component)
            .await
            .expect("Component execution failed");
        assert_eq!(result.exit_code, 0);
    }
}

#[tokio::test]
async fn test_component_result_is_debug() {
    let wasm_bytes = wat::parse_str(minimal_component_wat()).expect("WAT parse failed");

    let executor = ComponentExecutor::new(HardwareCapabilities::NONE)
        .expect("ComponentExecutor creation failed");

    let component = executor
        .compile(&wasm_bytes)
        .expect("Component compilation failed");

    let result = executor
        .execute(&component)
        .await
        .expect("Component execution failed");

    // Verify that ComponentResult implements Debug.
    let debug_str = format!("{:?}", result);
    assert!(debug_str.contains("exit_code"));
}
