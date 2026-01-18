//! Embedded System Optimizations
//!
//! This module provides optimizations specifically for embedded systems with constrained
//! resources:
//! - Flash-optimized code (target: <32KB compiled size)
//! - ROM-able kernel sections (.rodata, .text)
//! - XIP (execute-in-place) support for flash execution
//! - Minimal RAM footprint (target: <8KB runtime)
//! - Link-time optimization (LTO) configuration
//!
//! # Design Philosophy
//!
//! Embedded systems often have severe memory constraints:
//! - Flash: 64KB - 512KB (non-volatile, slow write, fast read)
//! - RAM: 8KB - 64KB (volatile, fast, precious)
//!
//! This module enables MielinOS to run efficiently on such systems by:
//! 1. Placing code and read-only data in flash (.text, .rodata)
//! 2. Minimizing RAM usage for runtime data (.data, .bss)
//! 3. Supporting execute-in-place (XIP) to avoid RAM copying
//! 4. Providing ultra-compact configurations
//!
//! # Examples
//!
//! ```no_run
//! use mielin_kernel::embedded::{EmbeddedConfig, MemoryLayout};
//!
//! // Create ultra-minimal configuration for Cortex-M0
//! let config = EmbeddedConfig::cortex_m0();
//! assert!(config.flash_size() <= 32768); // ≤32KB flash
//! assert!(config.ram_size() <= 8192);    // ≤8KB RAM
//!
//! // Custom configuration
//! let custom = EmbeddedConfig::builder()
//!     .flash_size(65536)   // 64KB flash
//!     .ram_size(16384)     // 16KB RAM
//!     .max_tasks(4)        // Only 4 concurrent tasks
//!     .max_pages(32)       // Only 32 pages (128KB @ 4KB/page)
//!     .xip_enabled(true)   // Execute from flash
//!     .build()
//!     .expect("valid config");
//! ```

use crate::config::{KernelConfig, PageSize};
use core::fmt;

/// Embedded system configuration optimized for minimal resource usage
#[derive(Debug, Clone)]
pub struct EmbeddedConfig {
    /// Total flash memory size (bytes)
    flash_size: usize,
    /// Total RAM size (bytes)
    ram_size: usize,
    /// Kernel configuration
    kernel_config: KernelConfig,
    /// Execute-in-place enabled
    xip_enabled: bool,
    /// Memory layout
    layout: MemoryLayout,
}

/// Memory layout for embedded systems
#[derive(Debug, Clone, Copy)]
pub struct MemoryLayout {
    /// Flash base address (typically 0x0800_0000 on STM32)
    pub flash_base: usize,
    /// Flash size in bytes
    pub flash_size: usize,
    /// RAM base address (typically 0x2000_0000 on STM32)
    pub ram_base: usize,
    /// RAM size in bytes
    pub ram_size: usize,
    /// Stack size (bytes)
    pub stack_size: usize,
    /// Heap size (bytes)
    pub heap_size: usize,
    /// XIP region base (flash address for code)
    pub xip_base: usize,
    /// XIP region size
    pub xip_size: usize,
}

impl MemoryLayout {
    /// STM32F0 (Cortex-M0) typical layout
    /// 32KB Flash @ 0x0800_0000
    /// 8KB RAM @ 0x2000_0000
    pub const fn stm32f0() -> Self {
        Self {
            flash_base: 0x0800_0000,
            flash_size: 32 * 1024, // 32KB
            ram_base: 0x2000_0000,
            ram_size: 8 * 1024,   // 8KB
            stack_size: 2 * 1024, // 2KB stack
            heap_size: 4 * 1024,  // 4KB heap
            xip_base: 0x0800_0000,
            xip_size: 28 * 1024, // 28KB for code (leave 4KB for data)
        }
    }

    /// STM32F4 (Cortex-M4) typical layout
    /// 512KB Flash @ 0x0800_0000
    /// 128KB RAM @ 0x2000_0000
    pub const fn stm32f4() -> Self {
        Self {
            flash_base: 0x0800_0000,
            flash_size: 512 * 1024, // 512KB
            ram_base: 0x2000_0000,
            ram_size: 128 * 1024,  // 128KB
            stack_size: 16 * 1024, // 16KB stack
            heap_size: 64 * 1024,  // 64KB heap
            xip_base: 0x0800_0000,
            xip_size: 256 * 1024, // 256KB for code
        }
    }

    /// nRF52 (Cortex-M4F) typical layout
    /// 512KB Flash @ 0x0000_0000
    /// 64KB RAM @ 0x2000_0000
    pub const fn nrf52() -> Self {
        Self {
            flash_base: 0x0000_0000,
            flash_size: 512 * 1024, // 512KB
            ram_base: 0x2000_0000,
            ram_size: 64 * 1024,  // 64KB
            stack_size: 8 * 1024, // 8KB stack
            heap_size: 32 * 1024, // 32KB heap
            xip_base: 0x0000_0000,
            xip_size: 256 * 1024, // 256KB for code
        }
    }

    /// ESP32-C3 (RISC-V) typical layout
    /// 384KB Flash @ 0x4200_0000
    /// 400KB RAM @ 0x3FC8_0000
    pub const fn esp32c3() -> Self {
        Self {
            flash_base: 0x4200_0000,
            flash_size: 384 * 1024, // 384KB
            ram_base: 0x3FC8_0000,
            ram_size: 400 * 1024,  // 400KB
            stack_size: 32 * 1024, // 32KB stack
            heap_size: 256 * 1024, // 256KB heap
            xip_base: 0x4200_0000,
            xip_size: 256 * 1024, // 256KB for code
        }
    }

    /// Generic embedded layout
    pub const fn new(
        flash_base: usize,
        flash_size: usize,
        ram_base: usize,
        ram_size: usize,
    ) -> Self {
        let stack_size = ram_size / 4; // 25% for stack
        let heap_size = ram_size / 2; // 50% for heap
        let xip_size = flash_size / 2; // 50% for code

        Self {
            flash_base,
            flash_size,
            ram_base,
            ram_size,
            stack_size,
            heap_size,
            xip_base: flash_base,
            xip_size,
        }
    }

    /// Check if address is in flash
    pub const fn is_flash(&self, addr: usize) -> bool {
        addr >= self.flash_base && addr < self.flash_base + self.flash_size
    }

    /// Check if address is in RAM
    pub const fn is_ram(&self, addr: usize) -> bool {
        addr >= self.ram_base && addr < self.ram_base + self.ram_size
    }

    /// Check if address is in XIP region
    pub const fn is_xip(&self, addr: usize) -> bool {
        addr >= self.xip_base && addr < self.xip_base + self.xip_size
    }
}

impl EmbeddedConfig {
    /// Cortex-M0 configuration (ultra-minimal)
    /// - 32KB Flash
    /// - 8KB RAM
    /// - 4 tasks max
    /// - 32 pages (128KB @ 4KB/page)
    /// - XIP enabled
    pub fn cortex_m0() -> Self {
        Self {
            flash_size: 32 * 1024,
            ram_size: 8 * 1024,
            kernel_config: KernelConfig::builder()
                .max_tasks(4)
                .max_pages(32)
                .page_size(PageSize::Size4KB)
                .max_cpus(1)
                .build()
                .expect("valid cortex-m0 config"),
            xip_enabled: true,
            layout: MemoryLayout::stm32f0(),
        }
    }

    /// Cortex-M4 configuration (moderate)
    /// - 512KB Flash
    /// - 128KB RAM
    /// - 32 tasks max
    /// - 256 pages (1MB @ 4KB/page)
    /// - XIP enabled
    pub fn cortex_m4() -> Self {
        Self {
            flash_size: 512 * 1024,
            ram_size: 128 * 1024,
            kernel_config: KernelConfig::builder()
                .max_tasks(32)
                .max_pages(256)
                .page_size(PageSize::Size4KB)
                .max_cpus(1)
                .build()
                .expect("valid cortex-m4 config"),
            xip_enabled: true,
            layout: MemoryLayout::stm32f4(),
        }
    }

    /// nRF52 configuration
    /// - 512KB Flash
    /// - 64KB RAM
    /// - 16 tasks max
    /// - 128 pages (512KB @ 4KB/page)
    /// - XIP enabled
    pub fn nrf52() -> Self {
        Self {
            flash_size: 512 * 1024,
            ram_size: 64 * 1024,
            kernel_config: KernelConfig::builder()
                .max_tasks(16)
                .max_pages(128)
                .page_size(PageSize::Size4KB)
                .max_cpus(1)
                .build()
                .expect("valid nrf52 config"),
            xip_enabled: true,
            layout: MemoryLayout::nrf52(),
        }
    }

    /// ESP32-C3 configuration (RISC-V)
    /// - 384KB Flash
    /// - 400KB RAM
    /// - 64 tasks max
    /// - 512 pages (2MB @ 4KB/page)
    /// - XIP enabled
    pub fn esp32c3() -> Self {
        Self {
            flash_size: 384 * 1024,
            ram_size: 400 * 1024,
            kernel_config: KernelConfig::builder()
                .max_tasks(64)
                .max_pages(512)
                .page_size(PageSize::Size4KB)
                .max_cpus(1)
                .build()
                .expect("valid esp32c3 config"),
            xip_enabled: true,
            layout: MemoryLayout::esp32c3(),
        }
    }

    /// Create a builder for custom embedded configurations
    pub fn builder() -> EmbeddedConfigBuilder {
        EmbeddedConfigBuilder::new()
    }

    /// Get flash size
    pub const fn flash_size(&self) -> usize {
        self.flash_size
    }

    /// Get RAM size
    pub const fn ram_size(&self) -> usize {
        self.ram_size
    }

    /// Get kernel configuration
    pub fn kernel_config(&self) -> &KernelConfig {
        &self.kernel_config
    }

    /// Check if XIP is enabled
    pub const fn xip_enabled(&self) -> bool {
        self.xip_enabled
    }

    /// Get memory layout
    pub const fn layout(&self) -> &MemoryLayout {
        &self.layout
    }

    /// Estimate kernel code size (rough approximation)
    pub fn estimated_code_size(&self) -> usize {
        // Base kernel: ~16KB
        let mut size = 16 * 1024;

        // Add per-task overhead: ~256 bytes per task
        size += self.kernel_config.scheduler.max_tasks * 256;

        // Add per-page overhead: ~8 bytes per page (bitmap)
        size += self.kernel_config.memory.max_pages * 8;

        // Round up to nearest KB
        size.div_ceil(1024) * 1024
    }

    /// Estimate kernel RAM usage (runtime data)
    pub fn estimated_ram_usage(&self) -> usize {
        // Task structures: ~512 bytes per task
        let task_ram = self.kernel_config.scheduler.max_tasks * 512;

        // Page metadata: ~16 bytes per page
        let page_ram = self.kernel_config.memory.max_pages * 16;

        // Scheduler state: ~1KB
        let scheduler_ram = 1024;

        // Heap allocator metadata: ~2KB
        let heap_ram = 2048;

        // Stack: from layout
        let stack_ram = self.layout.stack_size;

        task_ram + page_ram + scheduler_ram + heap_ram + stack_ram
    }

    /// Validate configuration against constraints
    pub fn validate(&self) -> Result<(), EmbeddedError> {
        // Check code size fits in flash
        let code_size = self.estimated_code_size();
        if code_size > self.flash_size {
            return Err(EmbeddedError::CodeTooLarge {
                required: code_size,
                available: self.flash_size,
            });
        }

        // Check RAM usage fits
        let ram_usage = self.estimated_ram_usage();
        if ram_usage > self.ram_size {
            return Err(EmbeddedError::InsufficientRam {
                required: ram_usage,
                available: self.ram_size,
            });
        }

        // Check XIP region is valid
        if self.xip_enabled && !self.layout.is_flash(self.layout.xip_base) {
            return Err(EmbeddedError::InvalidXipRegion);
        }

        Ok(())
    }
}

/// Builder for embedded configurations
pub struct EmbeddedConfigBuilder {
    flash_size: Option<usize>,
    ram_size: Option<usize>,
    max_tasks: usize,
    max_pages: usize,
    page_size: PageSize,
    xip_enabled: bool,
    layout: Option<MemoryLayout>,
}

impl EmbeddedConfigBuilder {
    fn new() -> Self {
        Self {
            flash_size: None,
            ram_size: None,
            max_tasks: 8,
            max_pages: 64,
            page_size: PageSize::Size4KB,
            xip_enabled: true,
            layout: None,
        }
    }

    /// Set flash size
    pub fn flash_size(mut self, size: usize) -> Self {
        self.flash_size = Some(size);
        self
    }

    /// Set RAM size
    pub fn ram_size(mut self, size: usize) -> Self {
        self.ram_size = Some(size);
        self
    }

    /// Set maximum tasks
    pub fn max_tasks(mut self, tasks: usize) -> Self {
        self.max_tasks = tasks;
        self
    }

    /// Set maximum pages
    pub fn max_pages(mut self, pages: usize) -> Self {
        self.max_pages = pages;
        self
    }

    /// Set page size
    pub fn page_size(mut self, size: PageSize) -> Self {
        self.page_size = size;
        self
    }

    /// Enable/disable XIP
    pub fn xip_enabled(mut self, enabled: bool) -> Self {
        self.xip_enabled = enabled;
        self
    }

    /// Set memory layout
    pub fn layout(mut self, layout: MemoryLayout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<EmbeddedConfig, EmbeddedError> {
        let flash_size = self.flash_size.ok_or(EmbeddedError::MissingFlashSize)?;
        let ram_size = self.ram_size.ok_or(EmbeddedError::MissingRamSize)?;

        let layout = self
            .layout
            .unwrap_or_else(|| MemoryLayout::new(0x0800_0000, flash_size, 0x2000_0000, ram_size));

        let kernel_config = KernelConfig::builder()
            .max_tasks(self.max_tasks)
            .max_pages(self.max_pages)
            .page_size(self.page_size)
            .max_cpus(1) // Embedded systems typically single-core
            .build()
            .map_err(|_| EmbeddedError::InvalidKernelConfig)?;

        let config = EmbeddedConfig {
            flash_size,
            ram_size,
            kernel_config,
            xip_enabled: self.xip_enabled,
            layout,
        };

        config.validate()?;

        Ok(config)
    }
}

/// Errors that can occur in embedded configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedError {
    /// Code size exceeds flash capacity
    CodeTooLarge { required: usize, available: usize },
    /// RAM usage exceeds available RAM
    InsufficientRam { required: usize, available: usize },
    /// XIP region is not in flash
    InvalidXipRegion,
    /// Flash size not specified
    MissingFlashSize,
    /// RAM size not specified
    MissingRamSize,
    /// Invalid kernel configuration
    InvalidKernelConfig,
}

impl fmt::Display for EmbeddedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodeTooLarge {
                required,
                available,
            } => {
                write!(
                    f,
                    "Code size {} bytes exceeds flash capacity {} bytes",
                    required, available
                )
            }
            Self::InsufficientRam {
                required,
                available,
            } => {
                write!(
                    f,
                    "RAM usage {} bytes exceeds available RAM {} bytes",
                    required, available
                )
            }
            Self::InvalidXipRegion => write!(f, "XIP region is not in flash memory"),
            Self::MissingFlashSize => write!(f, "Flash size not specified"),
            Self::MissingRamSize => write!(f, "RAM size not specified"),
            Self::InvalidKernelConfig => write!(f, "Invalid kernel configuration"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EmbeddedError {}

/// XIP (Execute-In-Place) support
///
/// This module provides utilities for running code directly from flash memory
/// without copying to RAM first. This is crucial for embedded systems with
/// limited RAM.
pub mod xip {
    use super::MemoryLayout;

    /// XIP configuration
    pub struct XipConfig {
        /// Base address of XIP region in flash
        pub base_addr: usize,
        /// Size of XIP region
        pub size: usize,
        /// Whether to cache XIP accesses
        pub cache_enabled: bool,
    }

    impl XipConfig {
        /// Create XIP configuration from memory layout
        pub fn from_layout(layout: &MemoryLayout) -> Self {
            Self {
                base_addr: layout.xip_base,
                size: layout.xip_size,
                cache_enabled: true,
            }
        }

        /// Check if address is in XIP region
        pub fn contains(&self, addr: usize) -> bool {
            addr >= self.base_addr && addr < self.base_addr + self.size
        }

        /// Enable XIP (platform-specific)
        #[cfg(target_arch = "arm")]
        pub fn enable(&self) -> Result<(), &'static str> {
            // On ARM Cortex-M, flash is already mapped to 0x0800_0000
            // and is executable by default. No special setup needed.
            Ok(())
        }

        #[cfg(not(target_arch = "arm"))]
        pub fn enable(&self) -> Result<(), &'static str> {
            // For other architectures, XIP might need platform-specific setup
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cortex_m0_config() {
        let config = EmbeddedConfig::cortex_m0();
        assert_eq!(config.flash_size(), 32 * 1024);
        assert_eq!(config.ram_size(), 8 * 1024);
        assert!(config.xip_enabled());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cortex_m4_config() {
        let config = EmbeddedConfig::cortex_m4();
        assert_eq!(config.flash_size(), 512 * 1024);
        assert_eq!(config.ram_size(), 128 * 1024);
        assert!(config.xip_enabled());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_nrf52_config() {
        let config = EmbeddedConfig::nrf52();
        assert_eq!(config.flash_size(), 512 * 1024);
        assert_eq!(config.ram_size(), 64 * 1024);
        assert!(config.xip_enabled());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_esp32c3_config() {
        let config = EmbeddedConfig::esp32c3();
        assert_eq!(config.flash_size(), 384 * 1024);
        assert_eq!(config.ram_size(), 400 * 1024);
        assert!(config.xip_enabled());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_custom_config() {
        let config = EmbeddedConfig::builder()
            .flash_size(64 * 1024)
            .ram_size(16 * 1024)
            .max_tasks(8)
            .max_pages(32)
            .xip_enabled(true)
            .build()
            .expect("valid config");

        assert_eq!(config.flash_size(), 64 * 1024);
        assert_eq!(config.ram_size(), 16 * 1024);
    }

    #[test]
    fn test_memory_layout_stm32f0() {
        let layout = MemoryLayout::stm32f0();
        assert_eq!(layout.flash_base, 0x0800_0000);
        assert_eq!(layout.flash_size, 32 * 1024);
        assert_eq!(layout.ram_base, 0x2000_0000);
        assert_eq!(layout.ram_size, 8 * 1024);
        assert!(layout.is_flash(0x0800_0000));
        assert!(layout.is_ram(0x2000_0000));
        assert!(layout.is_xip(0x0800_0000));
    }

    #[test]
    fn test_code_size_estimation() {
        let config = EmbeddedConfig::cortex_m0();
        let code_size = config.estimated_code_size();
        // Should be well under 32KB for minimal config
        assert!(code_size < 32 * 1024);
    }

    #[test]
    fn test_ram_usage_estimation() {
        let config = EmbeddedConfig::cortex_m0();
        let ram_usage = config.estimated_ram_usage();
        // Should be under 8KB for minimal config
        assert!(ram_usage <= 8 * 1024);
    }

    #[test]
    fn test_validation_code_too_large() {
        let result = EmbeddedConfig::builder()
            .flash_size(8 * 1024) // Only 8KB flash
            .ram_size(64 * 1024)
            .max_tasks(100) // Too many tasks
            .max_pages(1000) // Too many pages
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_validation_insufficient_ram() {
        let result = EmbeddedConfig::builder()
            .flash_size(512 * 1024)
            .ram_size(4 * 1024) // Only 4KB RAM
            .max_tasks(64)
            .max_pages(256)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_xip_config() {
        let layout = MemoryLayout::stm32f0();
        let xip = xip::XipConfig::from_layout(&layout);
        assert_eq!(xip.base_addr, layout.xip_base);
        assert_eq!(xip.size, layout.xip_size);
        assert!(xip.contains(0x0800_0000));
        assert!(!xip.contains(0x2000_0000)); // RAM, not XIP
    }

    #[test]
    fn test_error_display() {
        let err = EmbeddedError::CodeTooLarge {
            required: 64000,
            available: 32768,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("64000"));
        assert!(msg.contains("32768"));
    }
}
