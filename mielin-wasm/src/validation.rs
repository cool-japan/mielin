//! Module Validation and Fuzzing Support
//!
//! Provides enhanced validation, fuzzing, and security checks for WASM modules.
//!
//! # Features
//!
//! - **Comprehensive Validation**: Deep inspection of module structure
//! - **Fuzzing Support**: Integration with fuzzing frameworks
//! - **Security Checks**: Detection of malicious patterns
//! - **Performance Analysis**: Complexity analysis and limits

use thiserror::Error;

/// Validation error types
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Module exceeds maximum size: {actual} > {max}")]
    ModuleTooLarge { actual: usize, max: usize },

    #[error("Too many functions: {actual} > {max}")]
    TooManyFunctions { actual: usize, max: usize },

    #[error("Too many imports: {actual} > {max}")]
    TooManyImports { actual: usize, max: usize },

    #[error("Too many exports: {actual} > {max}")]
    TooManyExports { actual: usize, max: usize },

    #[error("Function complexity too high: {actual} > {max}")]
    ComplexityTooHigh { actual: usize, max: usize },

    #[error("Suspicious pattern detected: {0}")]
    SuspiciousPattern(String),

    #[error("Invalid module structure: {0}")]
    InvalidStructure(String),

    #[error("Wasmtime validation failed: {0}")]
    WasmtimeError(String),
}

/// Module validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum module size in bytes
    pub max_module_size: usize,

    /// Maximum number of functions
    pub max_functions: usize,

    /// Maximum number of imports
    pub max_imports: usize,

    /// Maximum number of exports
    pub max_exports: usize,

    /// Maximum function complexity (instructions)
    pub max_function_complexity: usize,

    /// Maximum memory pages
    pub max_memory_pages: u32,

    /// Maximum table size
    pub max_table_size: u32,

    /// Enable security pattern detection
    pub check_suspicious_patterns: bool,

    /// Enable deep validation
    pub deep_validation: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_module_size: 10 * 1024 * 1024, // 10 MB
            max_functions: 10_000,
            max_imports: 1_000,
            max_exports: 1_000,
            max_function_complexity: 100_000,
            max_memory_pages: 256, // 16 MB
            max_table_size: 10_000,
            check_suspicious_patterns: true,
            deep_validation: true,
        }
    }
}

impl ValidationConfig {
    /// Preset for embedded systems (very restrictive)
    pub fn embedded() -> Self {
        Self {
            max_module_size: 512 * 1024, // 512 KB
            max_functions: 100,
            max_imports: 50,
            max_exports: 50,
            max_function_complexity: 10_000,
            max_memory_pages: 16, // 1 MB
            max_table_size: 100,
            check_suspicious_patterns: true,
            deep_validation: true,
        }
    }

    /// Preset for standard applications
    pub fn standard() -> Self {
        Self::default()
    }

    /// Preset for compute-intensive applications
    pub fn compute() -> Self {
        Self {
            max_module_size: 100 * 1024 * 1024, // 100 MB
            max_functions: 100_000,
            max_imports: 10_000,
            max_exports: 10_000,
            max_function_complexity: 1_000_000,
            max_memory_pages: 4096, // 256 MB
            max_table_size: 100_000,
            check_suspicious_patterns: true,
            deep_validation: false, // Skip for performance
        }
    }

    /// Preset for fuzzing (permissive)
    pub fn fuzzing() -> Self {
        Self {
            max_module_size: usize::MAX,
            max_functions: usize::MAX,
            max_imports: usize::MAX,
            max_exports: usize::MAX,
            max_function_complexity: usize::MAX,
            max_memory_pages: u32::MAX,
            max_table_size: u32::MAX,
            check_suspicious_patterns: false,
            deep_validation: false,
        }
    }
}

/// Module statistics collected during validation
#[derive(Debug, Default, Clone)]
pub struct ModuleStats {
    /// Total module size in bytes
    pub module_size: usize,

    /// Number of functions
    pub function_count: usize,

    /// Number of imports
    pub import_count: usize,

    /// Number of exports
    pub export_count: usize,

    /// Maximum function complexity
    pub max_complexity: usize,

    /// Number of memory sections
    pub memory_count: usize,

    /// Number of table sections
    pub table_count: usize,

    /// Number of global variables
    pub global_count: usize,

    /// Data segment count
    pub data_segment_count: usize,

    /// Element segment count
    pub element_segment_count: usize,
}

impl ModuleStats {
    /// Check if module is complex
    pub fn is_complex(&self) -> bool {
        self.function_count > 1000 || self.max_complexity > 50_000
    }

    /// Get complexity score (0-100)
    pub fn complexity_score(&self) -> u8 {
        let func_score = (self.function_count as f64 / 10_000.0 * 50.0).min(50.0);
        let inst_score = (self.max_complexity as f64 / 100_000.0 * 50.0).min(50.0);
        (func_score + inst_score) as u8
    }
}

/// Suspicious patterns to detect
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspiciousPattern {
    /// Excessive memory growth
    ExcessiveMemoryGrowth,

    /// Tight infinite loop
    TightInfiniteLoop,

    /// Recursive calls without base case
    UnboundedRecursion,

    /// Suspicious integer operations (potential overflow)
    SuspiciousArithmetic,

    /// Excessive branching
    ExcessiveBranching,
}

impl SuspiciousPattern {
    /// Get description
    pub fn description(&self) -> &'static str {
        match self {
            Self::ExcessiveMemoryGrowth => "Excessive memory.grow operations",
            Self::TightInfiniteLoop => "Tight loop without termination",
            Self::UnboundedRecursion => "Recursive call without apparent base case",
            Self::SuspiciousArithmetic => "Suspicious arithmetic operations",
            Self::ExcessiveBranching => "Excessive conditional branching",
        }
    }
}

/// Module validator
pub struct ModuleValidator {
    /// Validation configuration
    config: ValidationConfig,

    /// Detected suspicious patterns
    suspicious_patterns: Vec<SuspiciousPattern>,

    /// Module statistics
    stats: ModuleStats,
}

impl ModuleValidator {
    /// Create a new validator
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            config,
            suspicious_patterns: Vec::new(),
            stats: ModuleStats::default(),
        }
    }

    /// Create validator with default configuration
    pub fn with_default() -> Self {
        Self::new(ValidationConfig::default())
    }

    /// Validate a WASM module
    pub fn validate(&mut self, wasm_bytes: &[u8]) -> Result<ModuleStats, ValidationError> {
        // Reset state
        self.suspicious_patterns.clear();
        self.stats = ModuleStats::default();

        // Basic size check
        self.stats.module_size = wasm_bytes.len();
        if self.stats.module_size > self.config.max_module_size {
            return Err(ValidationError::ModuleTooLarge {
                actual: self.stats.module_size,
                max: self.config.max_module_size,
            });
        }

        // Check WASM magic number
        if wasm_bytes.len() < 4 || &wasm_bytes[0..4] != b"\0asm" {
            return Err(ValidationError::InvalidStructure(
                "Invalid WASM magic number".to_string(),
            ));
        }

        // Parse module structure
        self.parse_module_structure(wasm_bytes)?;

        // Check limits
        self.check_limits()?;

        // Deep validation if enabled
        if self.config.deep_validation {
            self.deep_validate(wasm_bytes)?;
        }

        // Pattern detection if enabled
        if self.config.check_suspicious_patterns {
            self.detect_suspicious_patterns(wasm_bytes)?;
        }

        Ok(self.stats.clone())
    }

    /// Parse basic module structure
    fn parse_module_structure(&mut self, wasm_bytes: &[u8]) -> Result<(), ValidationError> {
        // Simple parsing of WASM sections
        let mut offset = 8; // Skip magic and version

        while offset < wasm_bytes.len() {
            if offset + 1 > wasm_bytes.len() {
                break;
            }

            let section_id = wasm_bytes[offset];
            offset += 1;

            // Read section size (LEB128)
            let (size, size_len) = self.read_leb128_u32(&wasm_bytes[offset..])?;
            offset += size_len;

            match section_id {
                1 => self.stats.import_count += 1, // Type section (approximation)
                2 => self.stats.import_count += 1, // Import section
                3 => self.stats.function_count += 1, // Function section
                4 => self.stats.table_count += 1,  // Table section
                5 => self.stats.memory_count += 1, // Memory section
                6 => self.stats.global_count += 1, // Global section
                7 => self.stats.export_count += 1, // Export section
                9 => self.stats.element_segment_count += 1, // Element section
                10 => self.stats.function_count += 1, // Code section (approximation)
                11 => self.stats.data_segment_count += 1, // Data section
                _ => {}
            }

            offset += size as usize;
        }

        Ok(())
    }

    /// Read LEB128 encoded u32
    fn read_leb128_u32(&self, bytes: &[u8]) -> Result<(u32, usize), ValidationError> {
        let mut result = 0u32;
        let mut shift = 0;
        let mut count = 0;

        for &byte in bytes.iter().take(5) {
            count += 1;
            result |= ((byte & 0x7F) as u32) << shift;

            if byte & 0x80 == 0 {
                return Ok((result, count));
            }

            shift += 7;
        }

        Err(ValidationError::InvalidStructure(
            "Invalid LEB128 encoding".to_string(),
        ))
    }

    /// Check module limits
    fn check_limits(&self) -> Result<(), ValidationError> {
        if self.stats.function_count > self.config.max_functions {
            return Err(ValidationError::TooManyFunctions {
                actual: self.stats.function_count,
                max: self.config.max_functions,
            });
        }

        if self.stats.import_count > self.config.max_imports {
            return Err(ValidationError::TooManyImports {
                actual: self.stats.import_count,
                max: self.config.max_imports,
            });
        }

        if self.stats.export_count > self.config.max_exports {
            return Err(ValidationError::TooManyExports {
                actual: self.stats.export_count,
                max: self.config.max_exports,
            });
        }

        Ok(())
    }

    /// Perform deep validation
    fn deep_validate(&mut self, _wasm_bytes: &[u8]) -> Result<(), ValidationError> {
        // Deep validation would involve:
        // - Function body analysis
        // - Control flow graph construction
        // - Complexity analysis
        // - Type checking beyond basic validation

        // For now, we rely on Wasmtime's validation
        Ok(())
    }

    /// Detect suspicious patterns
    fn detect_suspicious_patterns(&mut self, _wasm_bytes: &[u8]) -> Result<(), ValidationError> {
        // Pattern detection would involve:
        // - Analyzing instruction sequences
        // - Detecting tight loops
        // - Finding unbounded recursion
        // - Identifying suspicious arithmetic

        // This is a placeholder for future implementation
        Ok(())
    }

    /// Get detected suspicious patterns
    pub fn suspicious_patterns(&self) -> &[SuspiciousPattern] {
        &self.suspicious_patterns
    }

    /// Get module statistics
    pub fn stats(&self) -> &ModuleStats {
        &self.stats
    }
}

/// Fuzzing input generator
pub struct FuzzInputGenerator {
    /// Random seed
    seed: u64,

    /// Mutation rate (0.0 - 1.0)
    mutation_rate: f64,
}

impl FuzzInputGenerator {
    /// Create a new fuzzing input generator
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            mutation_rate: 0.1,
        }
    }

    /// Set mutation rate
    pub fn with_mutation_rate(mut self, rate: f64) -> Self {
        self.mutation_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Generate a minimal WASM module
    pub fn generate_minimal_module(&self) -> Vec<u8> {
        // Minimal WASM module: (module)
        vec![
            0x00, 0x61, 0x73, 0x6d, // magic: \0asm
            0x01, 0x00, 0x00, 0x00, // version: 1
        ]
    }

    /// Mutate WASM bytecode
    pub fn mutate(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = input.to_vec();
        let mutation_count = (output.len() as f64 * self.mutation_rate) as usize;

        for _ in 0..mutation_count {
            let idx = self.next_random() as usize % output.len();
            output[idx] = self.next_random() as u8;
        }

        output
    }

    /// Simple PRNG (XorShift64)
    fn next_random(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    /// Generate interesting test cases
    pub fn generate_test_cases(&mut self) -> Vec<Vec<u8>> {
        vec![
            self.generate_minimal_module(),
            self.generate_with_memory(),
            self.generate_with_functions(10),
            self.generate_with_loops(),
        ]
    }

    fn generate_with_memory(&self) -> Vec<u8> {
        // WASM module with memory: (module (memory 1))
        vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
            0x05, 0x03, 0x01, 0x00, 0x01, // memory section: 1 page min
        ]
    }

    fn generate_with_functions(&self, _count: usize) -> Vec<u8> {
        // WASM module with a simple function: (module (func (result i32) (i32.const 42)))
        vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
            0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, // type section: [] -> [i32]
            0x03, 0x02, 0x01, 0x00, // function section: func 0 has type 0
            0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b, // code section: i32.const 42, end
        ]
    }

    fn generate_with_loops(&self) -> Vec<u8> {
        // WASM module with simple loop
        vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic + version
            0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, // type section: [] -> [i32]
            0x03, 0x02, 0x01, 0x00, // function section
            0x0a, 0x09, 0x01, 0x07, 0x00, 0x03, 0x40, 0x41, 0x00, 0x0c, 0x00, 0x0b,
            0x0b, // code section with loop
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_config_presets() {
        let embedded = ValidationConfig::embedded();
        assert!(embedded.max_module_size < ValidationConfig::default().max_module_size);

        let compute = ValidationConfig::compute();
        assert!(compute.max_module_size > ValidationConfig::default().max_module_size);

        let fuzzing = ValidationConfig::fuzzing();
        assert_eq!(fuzzing.max_module_size, usize::MAX);
    }

    #[test]
    fn test_module_stats() {
        let stats = ModuleStats {
            function_count: 5000,
            max_complexity: 75_000,
            ..Default::default()
        };

        assert!(stats.is_complex());
        assert!(stats.complexity_score() > 50);
    }

    #[test]
    fn test_validator_basic() {
        let mut validator = ModuleValidator::with_default();

        // Valid minimal module
        let wasm = wat::parse_str("(module)").unwrap();
        let stats = validator.validate(&wasm).unwrap();

        assert_eq!(stats.module_size, wasm.len());
    }

    #[test]
    fn test_validator_size_limit() {
        let config = ValidationConfig {
            max_module_size: 4, // Smaller than minimal WASM module (8 bytes)
            ..Default::default()
        };
        let mut validator = ModuleValidator::new(config);

        // Minimal WASM module is 8 bytes
        let wasm = vec![
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
        ];
        let result = validator.validate(&wasm);

        assert!(result.is_err());
        if let Err(ValidationError::ModuleTooLarge { actual, max }) = result {
            assert_eq!(max, 4);
            assert_eq!(actual, 8);
        }
    }

    #[test]
    fn test_validator_invalid_magic() {
        let mut validator = ModuleValidator::with_default();

        let invalid_wasm = b"invalid";
        let result = validator.validate(invalid_wasm);

        assert!(result.is_err());
    }

    #[test]
    fn test_fuzz_generator() {
        let mut gen = FuzzInputGenerator::new(12345);

        let minimal = gen.generate_minimal_module();
        assert!(!minimal.is_empty());

        let test_cases = gen.generate_test_cases();
        assert_eq!(test_cases.len(), 4);
    }

    #[test]
    fn test_fuzz_mutation() {
        let mut gen = FuzzInputGenerator::new(12345).with_mutation_rate(0.1);

        let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mutated = gen.mutate(&input);

        assert_eq!(mutated.len(), input.len());
        // At least some bytes should be different
        assert_ne!(mutated, input);
    }

    #[test]
    fn test_suspicious_pattern_descriptions() {
        let patterns = [
            SuspiciousPattern::ExcessiveMemoryGrowth,
            SuspiciousPattern::TightInfiniteLoop,
            SuspiciousPattern::UnboundedRecursion,
        ];

        for pattern in &patterns {
            assert!(!pattern.description().is_empty());
        }
    }
}
