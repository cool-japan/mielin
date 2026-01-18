//! Runtime Abstraction Example
//!
//! Demonstrates how to use the runtime abstraction layer to work with
//! different WASM engines and configurations.

use mielin_wasm::runtime::{RuntimeCapabilities, RuntimeConfig, RuntimeEngine, RuntimeFactory};

fn main() -> anyhow::Result<()> {
    println!("=== Runtime Abstraction Example ===\n");

    // 1. Check available engines
    println!("Available WASM engines:");
    for engine in RuntimeEngine::available_engines() {
        println!("  - {}", engine.name());
    }
    println!();

    // 2. Create runtime with default configuration
    println!("Creating runtime with default configuration...");
    let default_runtime = RuntimeFactory::default_runtime()?;
    println!("Engine: {}", default_runtime.engine().name());
    println!();

    // 3. Create runtime with embedded configuration
    println!("Creating runtime with embedded configuration...");
    let embedded_config = RuntimeConfig::embedded();
    println!("Config:");
    println!(
        "  Max memory: {} MB",
        embedded_config.max_memory_bytes / (1024 * 1024)
    );
    println!("  JIT enabled: {}", embedded_config.enable_jit);
    println!("  SIMD enabled: {}", embedded_config.enable_simd);

    let _embedded_runtime = RuntimeFactory::create(embedded_config)?;
    println!("Runtime created successfully");
    println!();

    // 4. Create runtime with performance configuration
    println!("Creating runtime with performance configuration...");
    let perf_config = RuntimeConfig::performance();
    println!("Config:");
    println!(
        "  Max memory: {} MB",
        perf_config.max_memory_bytes / (1024 * 1024)
    );
    println!("  JIT enabled: {}", perf_config.enable_jit);
    println!("  AOT enabled: {}", perf_config.enable_aot);
    println!("  SIMD enabled: {}", perf_config.enable_simd);
    println!("  Threads enabled: {}", perf_config.enable_threads);

    let _perf_runtime = RuntimeFactory::create(perf_config)?;
    println!("Runtime created successfully");
    println!();

    // 5. Create runtime with security-focused configuration
    println!("Creating runtime with security configuration...");
    let secure_config = RuntimeConfig::secure();
    println!("Config:");
    println!(
        "  Max memory: {} MB",
        secure_config.max_memory_bytes / (1024 * 1024)
    );
    println!("  JIT enabled: {}", secure_config.enable_jit);
    println!("  Threads enabled: {}", secure_config.enable_threads);

    let _secure_runtime = RuntimeFactory::create(secure_config)?;
    println!("Runtime created successfully");
    println!();

    // 6. Check runtime capabilities
    println!("Runtime capabilities for Wasmtime:");
    let caps = RuntimeCapabilities::wasmtime();
    println!("  Supports JIT: {}", caps.supports_jit);
    println!("  Supports AOT: {}", caps.supports_aot);
    println!("  Supports SIMD: {}", caps.supports_simd);
    println!("  Supports threads: {}", caps.supports_threads);
    println!(
        "  Supports component model: {}",
        caps.supports_component_model
    );
    println!(
        "  Max memory: {} GB",
        caps.max_memory_bytes / (1024 * 1024 * 1024)
    );
    println!();

    // 7. Compile a simple WASM module
    println!("Compiling a simple WASM module...");
    let wasm = wat::parse_str(
        r#"
        (module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
        )
        "#,
    )?;

    let module = default_runtime.compile(&wasm)?;
    println!("Module compiled successfully");

    // Serialize module for caching
    let serialized = module.serialize()?;
    println!("Module serialized: {} bytes", serialized.len());
    println!();

    // 8. Validate WASM modules
    println!("Validating WASM modules...");

    // Valid module
    assert!(default_runtime.validate(&wasm).is_ok());
    println!("  Valid module validated successfully");

    // Invalid module
    let invalid = b"not wasm";
    assert!(default_runtime.validate(invalid).is_err());
    println!("  Invalid module correctly rejected");
    println!();

    // 9. Check configuration compatibility
    println!("Checking configuration compatibility...");
    let config = RuntimeConfig::default();
    let caps = RuntimeCapabilities::wasmtime();

    if caps.is_compatible(&config) {
        println!("  Configuration is compatible with Wasmtime");
    } else {
        println!("  Configuration is NOT compatible with Wasmtime");
    }
    println!();

    // 10. Get runtime statistics
    println!("Runtime statistics:");
    let stats = default_runtime.stats();
    println!("  Modules compiled: {}", stats.modules_compiled);
    println!("  Modules executed: {}", stats.modules_executed);
    println!(
        "  Avg compilation time: {:.2} μs",
        stats.avg_compilation_time_us()
    );
    println!("  Peak memory: {:.2} MB", stats.peak_memory_mb());
    println!();

    println!("=== Example completed successfully ===");

    Ok(())
}
