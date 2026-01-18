//! Runtime abstraction layer for multiple WASM engines
//!
//! Provides a unified interface for different WASM runtime engines,
//! allowing seamless switching between Wasmtime, Wasmer, and others.

use anyhow::{anyhow, Result};
use std::sync::Arc;

/// WASM runtime engine type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEngine {
    /// Wasmtime engine (default)
    Wasmtime,
    /// Wasmer engine (future support)
    Wasmer,
    /// WASM3 interpreter (future support)
    Wasm3,
}

impl RuntimeEngine {
    /// Get engine name
    pub fn name(&self) -> &'static str {
        match self {
            RuntimeEngine::Wasmtime => "wasmtime",
            RuntimeEngine::Wasmer => "wasmer",
            RuntimeEngine::Wasm3 => "wasm3",
        }
    }

    /// Check if engine is available
    pub fn is_available(&self) -> bool {
        match self {
            RuntimeEngine::Wasmtime => true, // Always available
            RuntimeEngine::Wasmer => false,  // Not yet implemented
            RuntimeEngine::Wasm3 => false,   // Not yet implemented
        }
    }

    /// Get all available engines
    pub fn available_engines() -> Vec<RuntimeEngine> {
        vec![
            RuntimeEngine::Wasmtime,
            RuntimeEngine::Wasmer,
            RuntimeEngine::Wasm3,
        ]
        .into_iter()
        .filter(|e| e.is_available())
        .collect()
    }
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Selected engine
    pub engine: RuntimeEngine,
    /// Maximum memory size (bytes)
    pub max_memory_bytes: usize,
    /// Enable JIT compilation
    pub enable_jit: bool,
    /// Enable AOT compilation
    pub enable_aot: bool,
    /// Enable SIMD support
    pub enable_simd: bool,
    /// Enable multi-threading
    pub enable_threads: bool,
    /// Enable bulk memory operations
    pub enable_bulk_memory: bool,
    /// Enable reference types
    pub enable_reference_types: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            engine: RuntimeEngine::Wasmtime,
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            enable_jit: true,
            enable_aot: false,
            enable_simd: true,
            enable_threads: false,
            enable_bulk_memory: true,
            enable_reference_types: true,
        }
    }
}

impl RuntimeConfig {
    /// Create configuration for embedded systems
    pub fn embedded() -> Self {
        Self {
            engine: RuntimeEngine::Wasmtime,
            max_memory_bytes: 4 * 1024 * 1024, // 4 MB
            enable_jit: false,
            enable_aot: false,
            enable_simd: true, // Keep enabled for compatibility
            enable_threads: false,
            enable_bulk_memory: true,
            enable_reference_types: true, // Required by Wasmtime
        }
    }

    /// Create configuration for high performance
    pub fn performance() -> Self {
        Self {
            engine: RuntimeEngine::Wasmtime,
            max_memory_bytes: 512 * 1024 * 1024, // 512 MB
            enable_jit: true,
            enable_aot: true,
            enable_simd: true,
            enable_threads: true,
            enable_bulk_memory: true,
            enable_reference_types: true,
        }
    }

    /// Create configuration for security-focused environments
    pub fn secure() -> Self {
        Self {
            engine: RuntimeEngine::Wasmtime,
            max_memory_bytes: 16 * 1024 * 1024, // 16 MB
            enable_jit: false,
            enable_aot: false,
            enable_simd: true, // Keep enabled for compatibility
            enable_threads: false,
            enable_bulk_memory: true,
            enable_reference_types: true, // Required by Wasmtime
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if !self.engine.is_available() {
            return Err(anyhow!(
                "Runtime engine '{}' is not available",
                self.engine.name()
            ));
        }

        if self.max_memory_bytes == 0 {
            return Err(anyhow!("max_memory_bytes must be greater than 0"));
        }

        if self.max_memory_bytes > 4 * 1024 * 1024 * 1024 {
            // 4 GB
            return Err(anyhow!("max_memory_bytes exceeds 4 GB limit"));
        }

        Ok(())
    }
}

/// Runtime statistics
#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    /// Number of modules compiled
    pub modules_compiled: u64,
    /// Number of modules executed
    pub modules_executed: u64,
    /// Total compilation time (microseconds)
    pub total_compilation_time_us: u64,
    /// Total execution time (microseconds)
    pub total_execution_time_us: u64,
    /// Peak memory usage (bytes)
    pub peak_memory_bytes: usize,
    /// Current active instances
    pub active_instances: usize,
}

impl RuntimeStats {
    /// Calculate average compilation time
    pub fn avg_compilation_time_us(&self) -> f64 {
        if self.modules_compiled == 0 {
            0.0
        } else {
            self.total_compilation_time_us as f64 / self.modules_compiled as f64
        }
    }

    /// Calculate average execution time
    pub fn avg_execution_time_us(&self) -> f64 {
        if self.modules_executed == 0 {
            0.0
        } else {
            self.total_execution_time_us as f64 / self.modules_executed as f64
        }
    }

    /// Get peak memory in MB
    pub fn peak_memory_mb(&self) -> f64 {
        self.peak_memory_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// Runtime capabilities
#[derive(Debug, Clone, Default)]
pub struct RuntimeCapabilities {
    /// Supports JIT compilation
    pub supports_jit: bool,
    /// Supports AOT compilation
    pub supports_aot: bool,
    /// Supports SIMD instructions
    pub supports_simd: bool,
    /// Supports multi-threading
    pub supports_threads: bool,
    /// Supports bulk memory operations
    pub supports_bulk_memory: bool,
    /// Supports reference types
    pub supports_reference_types: bool,
    /// Supports component model
    pub supports_component_model: bool,
    /// Maximum memory size (bytes)
    pub max_memory_bytes: usize,
}

impl RuntimeCapabilities {
    /// Get capabilities for Wasmtime
    pub fn wasmtime() -> Self {
        Self {
            supports_jit: true,
            supports_aot: true,
            supports_simd: true,
            supports_threads: true,
            supports_bulk_memory: true,
            supports_reference_types: true,
            supports_component_model: true,
            max_memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
        }
    }

    /// Get capabilities for Wasmer (future)
    pub fn wasmer() -> Self {
        Self {
            supports_jit: true,
            supports_aot: true,
            supports_simd: true,
            supports_threads: true,
            supports_bulk_memory: true,
            supports_reference_types: true,
            supports_component_model: false,
            max_memory_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
        }
    }

    /// Get capabilities for WASM3 (future)
    pub fn wasm3() -> Self {
        Self {
            supports_jit: false,
            supports_aot: false,
            supports_simd: false,
            supports_threads: false,
            supports_bulk_memory: true,
            supports_reference_types: false,
            supports_component_model: false,
            max_memory_bytes: 16 * 1024 * 1024, // 16 MB
        }
    }

    /// Get capabilities for the given engine
    pub fn for_engine(engine: RuntimeEngine) -> Self {
        match engine {
            RuntimeEngine::Wasmtime => Self::wasmtime(),
            RuntimeEngine::Wasmer => Self::wasmer(),
            RuntimeEngine::Wasm3 => Self::wasm3(),
        }
    }

    /// Check if configuration is compatible
    pub fn is_compatible(&self, config: &RuntimeConfig) -> bool {
        if config.enable_jit && !self.supports_jit {
            return false;
        }
        if config.enable_aot && !self.supports_aot {
            return false;
        }
        if config.enable_simd && !self.supports_simd {
            return false;
        }
        if config.enable_threads && !self.supports_threads {
            return false;
        }
        if config.enable_bulk_memory && !self.supports_bulk_memory {
            return false;
        }
        if config.enable_reference_types && !self.supports_reference_types {
            return false;
        }
        if config.max_memory_bytes > self.max_memory_bytes {
            return false;
        }
        true
    }
}

/// Runtime abstraction trait
pub trait Runtime: Send + Sync {
    /// Get runtime engine type
    fn engine(&self) -> RuntimeEngine;

    /// Get runtime capabilities
    fn capabilities(&self) -> RuntimeCapabilities;

    /// Get runtime statistics
    fn stats(&self) -> RuntimeStats;

    /// Compile a WASM module
    fn compile(&self, wasm_bytes: &[u8]) -> Result<Arc<dyn Module>>;

    /// Validate a WASM module
    fn validate(&self, wasm_bytes: &[u8]) -> Result<()>;
}

/// Module abstraction trait
pub trait Module: Send + Sync {
    /// Get module size in bytes
    fn size_bytes(&self) -> usize;

    /// Get module hash
    fn hash(&self) -> u64;

    /// Serialize module for caching
    fn serialize(&self) -> Result<Vec<u8>>;
}

/// Runtime factory
pub struct RuntimeFactory;

impl RuntimeFactory {
    /// Create a runtime with the given configuration
    pub fn create(config: RuntimeConfig) -> Result<Arc<dyn Runtime>> {
        config.validate()?;

        let caps = RuntimeCapabilities::for_engine(config.engine);
        if !caps.is_compatible(&config) {
            return Err(anyhow!(
                "Configuration is incompatible with {} engine capabilities",
                config.engine.name()
            ));
        }

        match config.engine {
            RuntimeEngine::Wasmtime => Ok(Arc::new(WasmtimeRuntime::new(config)?)),
            RuntimeEngine::Wasmer => Err(anyhow!("Wasmer support not yet implemented")),
            RuntimeEngine::Wasm3 => Err(anyhow!("WASM3 support not yet implemented")),
        }
    }

    /// Create a runtime with default configuration
    pub fn default_runtime() -> Result<Arc<dyn Runtime>> {
        Self::create(RuntimeConfig::default())
    }

    /// Get list of available engines
    pub fn available_engines() -> Vec<RuntimeEngine> {
        RuntimeEngine::available_engines()
    }
}

/// Wasmtime-specific runtime implementation
struct WasmtimeRuntime {
    _config: RuntimeConfig,
    engine: wasmtime::Engine,
}

impl WasmtimeRuntime {
    fn new(config: RuntimeConfig) -> Result<Self> {
        let mut wasmtime_config = wasmtime::Config::new();

        // Configure based on RuntimeConfig
        if config.enable_jit {
            wasmtime_config.strategy(wasmtime::Strategy::Cranelift);
        }

        wasmtime_config.wasm_simd(config.enable_simd);
        wasmtime_config.wasm_threads(config.enable_threads);
        wasmtime_config.wasm_bulk_memory(config.enable_bulk_memory);
        wasmtime_config.wasm_reference_types(config.enable_reference_types);

        let engine = wasmtime::Engine::new(&wasmtime_config)?;

        Ok(Self {
            _config: config,
            engine,
        })
    }
}

impl Runtime for WasmtimeRuntime {
    fn engine(&self) -> RuntimeEngine {
        RuntimeEngine::Wasmtime
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::wasmtime()
    }

    fn stats(&self) -> RuntimeStats {
        // For now, return default stats
        // In a full implementation, we would track these
        RuntimeStats::default()
    }

    fn compile(&self, wasm_bytes: &[u8]) -> Result<Arc<dyn Module>> {
        let module = wasmtime::Module::new(&self.engine, wasm_bytes)?;
        Ok(Arc::new(WasmtimeModule { module }))
    }

    fn validate(&self, wasm_bytes: &[u8]) -> Result<()> {
        wasmtime::Module::validate(&self.engine, wasm_bytes)?;
        Ok(())
    }
}

/// Wasmtime-specific module implementation
struct WasmtimeModule {
    module: wasmtime::Module,
}

impl Module for WasmtimeModule {
    fn size_bytes(&self) -> usize {
        // Approximate size - in a full implementation we would track this
        0
    }

    fn hash(&self) -> u64 {
        // Compute a hash - for now return 0
        0
    }

    fn serialize(&self) -> Result<Vec<u8>> {
        self.module.serialize().map_err(|e| anyhow!(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_engine_name() {
        assert_eq!(RuntimeEngine::Wasmtime.name(), "wasmtime");
        assert_eq!(RuntimeEngine::Wasmer.name(), "wasmer");
        assert_eq!(RuntimeEngine::Wasm3.name(), "wasm3");
    }

    #[test]
    fn test_runtime_engine_available() {
        assert!(RuntimeEngine::Wasmtime.is_available());
        assert!(!RuntimeEngine::Wasmer.is_available());
        assert!(!RuntimeEngine::Wasm3.is_available());
    }

    #[test]
    fn test_available_engines() {
        let engines = RuntimeEngine::available_engines();
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0], RuntimeEngine::Wasmtime);
    }

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.engine, RuntimeEngine::Wasmtime);
        assert!(config.enable_jit);
        assert!(!config.enable_aot);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_runtime_config_embedded() {
        let config = RuntimeConfig::embedded();
        assert_eq!(config.max_memory_bytes, 4 * 1024 * 1024);
        assert!(!config.enable_jit);
        assert!(config.enable_simd); // SIMD must be enabled for Wasmtime compatibility
        assert!(config.enable_reference_types); // Required by Wasmtime
    }

    #[test]
    fn test_runtime_config_performance() {
        let config = RuntimeConfig::performance();
        assert!(config.enable_jit);
        assert!(config.enable_aot);
        assert!(config.enable_simd);
        assert!(config.enable_threads);
    }

    #[test]
    fn test_runtime_config_secure() {
        let config = RuntimeConfig::secure();
        assert!(!config.enable_jit);
        assert!(!config.enable_threads);
        assert_eq!(config.max_memory_bytes, 16 * 1024 * 1024);
    }

    #[test]
    fn test_runtime_config_validation() {
        let mut config = RuntimeConfig::default();
        assert!(config.validate().is_ok());

        config.max_memory_bytes = 0;
        assert!(config.validate().is_err());

        config.max_memory_bytes = 5 * 1024 * 1024 * 1024; // > 4 GB
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_runtime_capabilities_wasmtime() {
        let caps = RuntimeCapabilities::wasmtime();
        assert!(caps.supports_jit);
        assert!(caps.supports_aot);
        assert!(caps.supports_simd);
        assert!(caps.supports_component_model);
    }

    #[test]
    fn test_runtime_capabilities_compatibility() {
        let caps = RuntimeCapabilities::wasmtime();
        let config = RuntimeConfig::default();
        assert!(caps.is_compatible(&config));

        let incompatible_config = RuntimeConfig {
            max_memory_bytes: 5 * 1024 * 1024 * 1024, // > 4 GB
            ..Default::default()
        };
        assert!(!caps.is_compatible(&incompatible_config));
    }

    #[test]
    fn test_runtime_stats() {
        let mut stats = RuntimeStats::default();
        assert_eq!(stats.avg_compilation_time_us(), 0.0);

        stats.modules_compiled = 10;
        stats.total_compilation_time_us = 1000;
        assert_eq!(stats.avg_compilation_time_us(), 100.0);

        stats.peak_memory_bytes = 10 * 1024 * 1024;
        assert_eq!(stats.peak_memory_mb(), 10.0);
    }

    #[test]
    fn test_runtime_factory_create() {
        let config = RuntimeConfig::default();
        let runtime = RuntimeFactory::create(config);
        assert!(runtime.is_ok());

        let runtime = runtime.expect("Failed to create runtime");
        assert_eq!(runtime.engine(), RuntimeEngine::Wasmtime);
    }

    #[test]
    fn test_runtime_factory_default() {
        let runtime = RuntimeFactory::default_runtime();
        assert!(runtime.is_ok());
    }

    #[test]
    fn test_runtime_factory_unavailable_engine() {
        let config = RuntimeConfig {
            engine: RuntimeEngine::Wasmer,
            ..Default::default()
        };
        let runtime = RuntimeFactory::create(config);
        assert!(runtime.is_err());
    }

    #[test]
    fn test_wasmtime_runtime_validate() {
        let runtime = RuntimeFactory::default_runtime().expect("Failed to create runtime");

        // Valid WASM module
        let wasm = wat::parse_str("(module)").expect("Failed to parse WAT");
        assert!(runtime.validate(&wasm).is_ok());

        // Invalid WASM
        let invalid = b"not wasm";
        assert!(runtime.validate(invalid).is_err());
    }

    #[test]
    fn test_wasmtime_runtime_compile() {
        let runtime = RuntimeFactory::default_runtime().expect("Failed to create runtime");

        let wasm = wat::parse_str("(module)").expect("Failed to parse WAT");
        let module = runtime.compile(&wasm);
        assert!(module.is_ok());
    }

    #[test]
    fn test_wasmtime_module_serialize() {
        let runtime = RuntimeFactory::default_runtime().expect("Failed to create runtime");
        let wasm = wat::parse_str("(module)").expect("Failed to parse WAT");
        let module = runtime.compile(&wasm).expect("Failed to compile");

        let serialized = module.serialize();
        assert!(serialized.is_ok());
        assert!(!serialized.expect("Serialization failed").is_empty());
    }
}
