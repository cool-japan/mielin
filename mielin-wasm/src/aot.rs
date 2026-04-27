//! Ahead-of-Time (AOT) Compilation for MielinOS WASM Runtime
//!
//! This module provides AOT compilation capabilities for precompiling WASM modules
//! to native code, enabling faster startup times and improved performance.
//!
//! # Features
//!
//! - **Precompilation**: Compile WASM to native code ahead of time
//! - **Serialization**: Save compiled native code to disk
//! - **Fast Startup**: Load precompiled modules instantly
//! - **Version Control**: Handle multiple versions of compiled code
//! - **Platform Detection**: Automatic platform-specific optimization
//! - **Cache Integration**: Seamless integration with module cache
//!
//! # Example
//!
//! ```rust,ignore
//! use mielin_wasm::aot::{AotCompiler, AotConfig};
//!
//! // Create AOT compiler
//! let compiler = AotCompiler::new(AotConfig::default())?;
//!
//! // Precompile WASM module
//! let compiled = compiler.precompile(&wasm_bytes)?;
//!
//! // Serialize to file
//! compiler.serialize(&compiled, "module.aot")?;
//!
//! // Later: load precompiled module
//! let module = compiler.load_precompiled("module.aot")?;
//! ```

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Engine, Module};

/// AOT compilation configuration
#[derive(Debug, Clone)]
pub struct AotConfig {
    /// Enable cranelift optimizations
    pub optimize: bool,
    /// Target CPU features (auto-detect if None)
    pub target_cpu: Option<String>,
    /// Compilation tier (baseline, optimized, cranelift)
    pub tier: CompilationTier,
    /// Enable debug info in compiled code
    pub debug_info: bool,
    /// Enable profiling support
    pub enable_profiling: bool,
    /// Cache directory for AOT artifacts
    pub cache_dir: PathBuf,
}

impl Default for AotConfig {
    fn default() -> Self {
        Self {
            optimize: true,
            target_cpu: None,
            tier: CompilationTier::Optimized,
            debug_info: false,
            enable_profiling: false,
            cache_dir: PathBuf::from(".mielin_aot_cache"),
        }
    }
}

impl AotConfig {
    /// Fast compilation (minimal optimizations)
    pub fn fast() -> Self {
        Self {
            optimize: false,
            target_cpu: None,
            tier: CompilationTier::Baseline,
            debug_info: false,
            enable_profiling: false,
            cache_dir: PathBuf::from(".mielin_aot_cache"),
        }
    }

    /// Optimized compilation (maximum performance)
    pub fn optimized() -> Self {
        Self {
            optimize: true,
            target_cpu: Some("native".to_string()),
            tier: CompilationTier::Optimized,
            debug_info: false,
            enable_profiling: false,
            cache_dir: PathBuf::from(".mielin_aot_cache"),
        }
    }

    /// Debug compilation (with debug info)
    pub fn debug() -> Self {
        Self {
            optimize: false,
            target_cpu: None,
            tier: CompilationTier::Baseline,
            debug_info: true,
            enable_profiling: true,
            cache_dir: PathBuf::from(".mielin_aot_cache"),
        }
    }
}

/// Compilation tier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationTier {
    /// Baseline compilation (fast compile, slower execution)
    Baseline,
    /// Optimized compilation (slower compile, fast execution)
    Optimized,
    /// Cranelift with full optimizations
    Cranelift,
}

/// Precompiled module metadata
#[derive(Debug, Clone)]
pub struct PrecompiledModule {
    /// Module hash (for verification)
    pub hash: [u8; 32],
    /// Compilation timestamp
    pub compiled_at: u64,
    /// Wasmtime version used for compilation
    pub wasmtime_version: String,
    /// Target platform
    pub platform: String,
    /// Target architecture
    pub arch: String,
    /// Serialized native code
    pub native_code: Vec<u8>,
    /// Metadata (custom key-value pairs)
    pub metadata: Vec<(String, String)>,
}

impl PrecompiledModule {
    /// Create new precompiled module
    pub fn new(native_code: Vec<u8>) -> Self {
        let hash = Self::compute_hash(&native_code);
        let compiled_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH")
            .as_secs();

        Self {
            hash,
            compiled_at,
            wasmtime_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            native_code,
            metadata: Vec::new(),
        }
    }

    /// Compute FNV-1a hash of data
    fn compute_hash(data: &[u8]) -> [u8; 32] {
        let mut hash = [0u8; 32];
        let mut h: u64 = 14695981039346656037; // FNV offset basis

        for &byte in data {
            h ^= byte as u64;
            h = h.wrapping_mul(1099511628211); // FNV prime
        }

        // Fill hash array
        for i in 0..4 {
            let bytes = h.to_le_bytes();
            hash[i * 8..(i + 1) * 8].copy_from_slice(&bytes);
            h = h.wrapping_mul(1099511628211);
        }

        hash
    }

    /// Verify hash integrity
    pub fn verify_hash(&self) -> bool {
        let expected = Self::compute_hash(&self.native_code);
        self.hash == expected
    }

    /// Add metadata
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.push((key, value));
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// AOT compiler for precompiling WASM modules
pub struct AotCompiler {
    /// Configuration
    config: AotConfig,
    /// Wasmtime engine configured for AOT
    engine: Engine,
}

impl AotCompiler {
    /// Create new AOT compiler
    pub fn new(config: AotConfig) -> Result<Self> {
        let mut wasmtime_config = wasmtime::Config::new();

        // Configure based on compilation tier
        match config.tier {
            CompilationTier::Baseline => {
                wasmtime_config.strategy(wasmtime::Strategy::Cranelift);
                wasmtime_config.cranelift_opt_level(wasmtime::OptLevel::None);
            }
            CompilationTier::Optimized => {
                wasmtime_config.strategy(wasmtime::Strategy::Cranelift);
                wasmtime_config.cranelift_opt_level(wasmtime::OptLevel::Speed);
            }
            CompilationTier::Cranelift => {
                wasmtime_config.strategy(wasmtime::Strategy::Cranelift);
                wasmtime_config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
            }
        }

        // Enable/disable debug info
        if config.debug_info {
            wasmtime_config.debug_info(true);
        }

        // Enable profiling if requested
        if config.enable_profiling {
            wasmtime_config.profiler(wasmtime::ProfilingStrategy::PerfMap);
        }

        // Additional optimizations
        if config.optimize {
            wasmtime_config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        }

        let engine = Engine::new(&wasmtime_config)?;

        // Create cache directory if it doesn't exist
        if !config.cache_dir.exists() {
            fs::create_dir_all(&config.cache_dir)
                .context("Failed to create AOT cache directory")?;
        }

        Ok(Self { config, engine })
    }

    /// Precompile WASM module to native code
    pub fn precompile(&self, wasm_bytes: &[u8]) -> Result<PrecompiledModule> {
        // Compile module
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to compile WASM module: {}", e))?;

        // Serialize to native code
        let native_code = module
            .serialize()
            .map_err(|e| anyhow::anyhow!("Failed to serialize compiled module: {}", e))?;

        Ok(PrecompiledModule::new(native_code))
    }

    /// Serialize precompiled module to file
    pub fn serialize(&self, module: &PrecompiledModule, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = File::create(path).context("Failed to create AOT file")?;

        // Write magic number
        file.write_all(b"MIELINAOT")?;

        // Write version
        file.write_all(&[0, 1, 0, 0])?; // Version 0.1.0.0

        // Write hash
        file.write_all(&module.hash)?;

        // Write timestamp
        file.write_all(&module.compiled_at.to_le_bytes())?;

        // Write wasmtime version
        let version_bytes = module.wasmtime_version.as_bytes();
        file.write_all(&(version_bytes.len() as u32).to_le_bytes())?;
        file.write_all(version_bytes)?;

        // Write platform
        let platform_bytes = module.platform.as_bytes();
        file.write_all(&(platform_bytes.len() as u32).to_le_bytes())?;
        file.write_all(platform_bytes)?;

        // Write arch
        let arch_bytes = module.arch.as_bytes();
        file.write_all(&(arch_bytes.len() as u32).to_le_bytes())?;
        file.write_all(arch_bytes)?;

        // Write metadata count
        file.write_all(&(module.metadata.len() as u32).to_le_bytes())?;

        // Write metadata
        for (key, value) in &module.metadata {
            let key_bytes = key.as_bytes();
            file.write_all(&(key_bytes.len() as u32).to_le_bytes())?;
            file.write_all(key_bytes)?;

            let value_bytes = value.as_bytes();
            file.write_all(&(value_bytes.len() as u32).to_le_bytes())?;
            file.write_all(value_bytes)?;
        }

        // Write native code size
        file.write_all(&(module.native_code.len() as u64).to_le_bytes())?;

        // Write native code
        file.write_all(&module.native_code)?;

        file.sync_all()?;
        Ok(())
    }

    /// Deserialize precompiled module from file
    pub fn deserialize(&self, path: impl AsRef<Path>) -> Result<PrecompiledModule> {
        let path = path.as_ref();
        let mut file = File::open(path).context("Failed to open AOT file")?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let mut offset = 0;

        // Read magic number
        if &buffer[offset..offset + 9] != b"MIELINAOT" {
            bail!("Invalid AOT file: bad magic number");
        }
        offset += 9;

        // Read version
        let _version = &buffer[offset..offset + 4];
        offset += 4;

        // Read hash
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&buffer[offset..offset + 32]);
        offset += 32;

        // Read timestamp
        let mut ts_bytes = [0u8; 8];
        ts_bytes.copy_from_slice(&buffer[offset..offset + 8]);
        let compiled_at = u64::from_le_bytes(ts_bytes);
        offset += 8;

        // Read wasmtime version
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
        let version_len = u32::from_le_bytes(len_bytes) as usize;
        offset += 4;
        let wasmtime_version = String::from_utf8(buffer[offset..offset + version_len].to_vec())?;
        offset += version_len;

        // Read platform
        len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
        let platform_len = u32::from_le_bytes(len_bytes) as usize;
        offset += 4;
        let platform = String::from_utf8(buffer[offset..offset + platform_len].to_vec())?;
        offset += platform_len;

        // Read arch
        len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
        let arch_len = u32::from_le_bytes(len_bytes) as usize;
        offset += 4;
        let arch = String::from_utf8(buffer[offset..offset + arch_len].to_vec())?;
        offset += arch_len;

        // Read metadata count
        len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
        let metadata_count = u32::from_le_bytes(len_bytes) as usize;
        offset += 4;

        // Read metadata
        let mut metadata = Vec::new();
        for _ in 0..metadata_count {
            // Read key
            len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
            let key_len = u32::from_le_bytes(len_bytes) as usize;
            offset += 4;
            let key = String::from_utf8(buffer[offset..offset + key_len].to_vec())?;
            offset += key_len;

            // Read value
            len_bytes.copy_from_slice(&buffer[offset..offset + 4]);
            let value_len = u32::from_le_bytes(len_bytes) as usize;
            offset += 4;
            let value = String::from_utf8(buffer[offset..offset + value_len].to_vec())?;
            offset += value_len;

            metadata.push((key, value));
        }

        // Read native code size
        let mut size_bytes = [0u8; 8];
        size_bytes.copy_from_slice(&buffer[offset..offset + 8]);
        let code_size = u64::from_le_bytes(size_bytes) as usize;
        offset += 8;

        // Read native code
        let native_code = buffer[offset..offset + code_size].to_vec();

        let module = PrecompiledModule {
            hash,
            compiled_at,
            wasmtime_version,
            platform,
            arch,
            native_code,
            metadata,
        };

        // Verify hash
        if !module.verify_hash() {
            bail!("AOT file corrupted: hash mismatch");
        }

        Ok(module)
    }

    /// Load precompiled module
    pub fn load_precompiled(&self, path: impl AsRef<Path>) -> Result<Module> {
        let precompiled = self.deserialize(path)?;

        // Safety check: platform and arch should match
        if precompiled.platform != std::env::consts::OS {
            bail!(
                "Platform mismatch: compiled for {}, running on {}",
                precompiled.platform,
                std::env::consts::OS
            );
        }

        if precompiled.arch != std::env::consts::ARCH {
            bail!(
                "Architecture mismatch: compiled for {}, running on {}",
                precompiled.arch,
                std::env::consts::ARCH
            );
        }

        // Deserialize module
        unsafe {
            Module::deserialize(&self.engine, &precompiled.native_code)
                .map_err(|e| anyhow::anyhow!("Failed to deserialize precompiled module: {}", e))
        }
    }

    /// Precompile and save to cache
    pub fn precompile_to_cache(&self, wasm_bytes: &[u8], cache_key: &str) -> Result<PathBuf> {
        let mut precompiled = self.precompile(wasm_bytes)?;

        // Add metadata
        precompiled.add_metadata("cache_key".to_string(), cache_key.to_string());

        // Create cache path
        let cache_path = self.config.cache_dir.join(format!("{}.aot", cache_key));

        // Serialize
        self.serialize(&precompiled, &cache_path)?;

        Ok(cache_path)
    }

    /// Load from cache
    pub fn load_from_cache(&self, cache_key: &str) -> Result<Module> {
        let cache_path = self.config.cache_dir.join(format!("{}.aot", cache_key));

        if !cache_path.exists() {
            bail!("Precompiled module not found in cache: {}", cache_key);
        }

        self.load_precompiled(cache_path)
    }

    /// Check if module is in cache
    pub fn is_cached(&self, cache_key: &str) -> bool {
        let cache_path = self.config.cache_dir.join(format!("{}.aot", cache_key));
        cache_path.exists()
    }

    /// Clear cache
    pub fn clear_cache(&self) -> Result<()> {
        if self.config.cache_dir.exists() {
            fs::remove_dir_all(&self.config.cache_dir)?;
            fs::create_dir_all(&self.config.cache_dir)?;
        }
        Ok(())
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> Result<AotCacheStats> {
        if !self.config.cache_dir.exists() {
            return Ok(AotCacheStats {
                total_files: 0,
                total_size: 0,
                oldest_timestamp: None,
                newest_timestamp: None,
            });
        }

        let mut total_files = 0;
        let mut total_size = 0;
        let mut oldest: Option<u64> = None;
        let mut newest: Option<u64> = None;

        for entry in fs::read_dir(&self.config.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("aot") {
                total_files += 1;
                total_size += entry.metadata()?.len();

                // Try to read timestamp
                if let Ok(precompiled) = self.deserialize(&path) {
                    let ts = precompiled.compiled_at;
                    oldest = Some(oldest.map_or(ts, |o| o.min(ts)));
                    newest = Some(newest.map_or(ts, |n| n.max(ts)));
                }
            }
        }

        Ok(AotCacheStats {
            total_files,
            total_size,
            oldest_timestamp: oldest,
            newest_timestamp: newest,
        })
    }
}

/// AOT cache statistics
#[derive(Debug, Clone)]
pub struct AotCacheStats {
    /// Total number of cached files
    pub total_files: usize,
    /// Total size in bytes
    pub total_size: u64,
    /// Oldest compilation timestamp
    pub oldest_timestamp: Option<u64>,
    /// Newest compilation timestamp
    pub newest_timestamp: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (func (export "add") (param i32 i32) (result i32)
                    local.get 0
                    local.get 1
                    i32.add
                )
            )
            "#,
        )
        .unwrap()
    }

    #[test]
    fn test_aot_config_presets() {
        let fast = AotConfig::fast();
        assert!(!fast.optimize);
        assert_eq!(fast.tier, CompilationTier::Baseline);

        let optimized = AotConfig::optimized();
        assert!(optimized.optimize);
        assert_eq!(optimized.tier, CompilationTier::Optimized);

        let debug = AotConfig::debug();
        assert!(debug.debug_info);
        assert!(debug.enable_profiling);
    }

    #[test]
    fn test_aot_compiler_creation() {
        let config = AotConfig::default();
        let compiler = AotCompiler::new(config);
        assert!(compiler.is_ok());
    }

    #[test]
    fn test_precompilation() {
        let config = AotConfig::fast();
        let compiler = AotCompiler::new(config).unwrap();
        let wasm = create_simple_wasm();

        let result = compiler.precompile(&wasm);
        assert!(result.is_ok());

        let precompiled = result.unwrap();
        assert!(!precompiled.native_code.is_empty());
        assert!(precompiled.verify_hash());
    }

    #[test]
    fn test_precompiled_module_hash() {
        let wasm = create_simple_wasm();
        let config = AotConfig::default();
        let compiler = AotCompiler::new(config).unwrap();

        let module = compiler.precompile(&wasm).unwrap();
        assert!(module.verify_hash());

        // Tamper with code
        let mut tampered = module.clone();
        if let Some(byte) = tampered.native_code.get_mut(0) {
            *byte ^= 0xFF;
        }
        assert!(!tampered.verify_hash());
    }

    #[test]
    fn test_serialization_deserialization() {
        let wasm = create_simple_wasm();
        let config = AotConfig::default();
        let compiler = AotCompiler::new(config).unwrap();

        let precompiled = compiler.precompile(&wasm).unwrap();

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_module.aot");

        // Serialize
        let result = compiler.serialize(&precompiled, &path);
        assert!(result.is_ok());
        assert!(path.exists());

        // Deserialize
        let loaded = compiler.deserialize(&path);
        assert!(loaded.is_ok());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.hash, precompiled.hash);
        assert_eq!(loaded.platform, precompiled.platform);
        assert_eq!(loaded.arch, precompiled.arch);

        // Cleanup
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_metadata() {
        let wasm = create_simple_wasm();
        let config = AotConfig::default();
        let compiler = AotCompiler::new(config).unwrap();

        let mut precompiled = compiler.precompile(&wasm).unwrap();

        precompiled.add_metadata("author".to_string(), "MielinOS".to_string());
        precompiled.add_metadata("version".to_string(), "1.0.0".to_string());

        assert_eq!(
            precompiled.get_metadata("author"),
            Some(&"MielinOS".to_string())
        );
        assert_eq!(
            precompiled.get_metadata("version"),
            Some(&"1.0.0".to_string())
        );
        assert_eq!(precompiled.get_metadata("unknown"), None);
    }

    #[test]
    fn test_cache_operations() {
        let temp_dir = std::env::temp_dir().join("mielin_aot_test");
        let config = AotConfig {
            cache_dir: temp_dir.clone(),
            ..Default::default()
        };

        let compiler = AotCompiler::new(config).unwrap();
        let wasm = create_simple_wasm();

        // Not cached initially
        assert!(!compiler.is_cached("test_module"));

        // Precompile to cache
        let result = compiler.precompile_to_cache(&wasm, "test_module");
        assert!(result.is_ok());

        // Should be cached now
        assert!(compiler.is_cached("test_module"));

        // Load from cache
        let loaded = compiler.load_from_cache("test_module");
        assert!(loaded.is_ok());

        // Clear cache
        compiler.clear_cache().unwrap();
        assert!(!compiler.is_cached("test_module"));

        // Cleanup
        std::fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_cache_stats() {
        let temp_dir = std::env::temp_dir().join("mielin_aot_stats");
        let config = AotConfig {
            cache_dir: temp_dir.clone(),
            ..Default::default()
        };

        let compiler = AotCompiler::new(config).unwrap();
        let wasm = create_simple_wasm();

        // Add some cached modules
        compiler.precompile_to_cache(&wasm, "module1").unwrap();
        compiler.precompile_to_cache(&wasm, "module2").unwrap();

        let stats = compiler.cache_stats().unwrap();
        assert_eq!(stats.total_files, 2);
        assert!(stats.total_size > 0);
        assert!(stats.oldest_timestamp.is_some());
        assert!(stats.newest_timestamp.is_some());

        // Cleanup
        std::fs::remove_dir_all(temp_dir).ok();
    }

    #[test]
    fn test_platform_verification() {
        let wasm = create_simple_wasm();
        let config = AotConfig::default();
        let compiler = AotCompiler::new(config).unwrap();

        let mut precompiled = compiler.precompile(&wasm).unwrap();

        // Tamper with platform
        precompiled.platform = "unknown_os".to_string();

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("tampered_platform.aot");

        compiler.serialize(&precompiled, &path).unwrap();

        // Should fail to load due to platform mismatch
        let result = compiler.load_precompiled(&path);
        assert!(result.is_err());

        // Cleanup
        std::fs::remove_file(path).ok();
    }
}
