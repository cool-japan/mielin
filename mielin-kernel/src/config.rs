//! Kernel configuration management
//!
//! Provides runtime configuration for kernel parameters including memory,
//! scheduler, and multi-core settings.
//!
//! ## Features
//!
//! - Runtime configuration of MAX_TASKS and MAX_PAGES
//! - Support for alternative page sizes (4KB, 8KB, 16KB, 64KB)
//! - Per-CPU configuration (MAX_CPUS, affinity)
//! - Validation of configuration parameters
//! - Default configurations for common use cases
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::config::{KernelConfig, PageSize};
//!
//! // Create custom configuration
//! let config = KernelConfig::builder()
//!     .max_pages(2048)
//!     .max_tasks(128)
//!     .page_size(PageSize::Size8KB)
//!     .max_cpus(8)
//!     .build()
//!     .unwrap();
//!
//! // Or use default configuration
//! let default_config = KernelConfig::default();
//! ```

use crate::KernelError;

/// Supported page sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    /// 4KB pages (standard x86_64, ARMv8)
    #[default]
    Size4KB = 4096,
    /// 8KB pages (some embedded systems)
    Size8KB = 8192,
    /// 16KB pages (ARMv8, macOS on M1/M2)
    Size16KB = 16384,
    /// 64KB pages (ARMv8, some RISC-V)
    Size64KB = 65536,
}

impl PageSize {
    /// Get the page size in bytes
    pub const fn bytes(self) -> usize {
        self as usize
    }

    /// Get the page size shift (log2)
    pub const fn shift(self) -> usize {
        match self {
            PageSize::Size4KB => 12,  // 2^12 = 4096
            PageSize::Size8KB => 13,  // 2^13 = 8192
            PageSize::Size16KB => 14, // 2^14 = 16384
            PageSize::Size64KB => 16, // 2^16 = 65536
        }
    }

    /// Check if a size is aligned to this page size
    pub const fn is_aligned(self, size: usize) -> bool {
        size & (self.bytes() - 1) == 0
    }

    /// Align a size up to this page size
    pub const fn align_up(self, size: usize) -> usize {
        (size + self.bytes() - 1) & !(self.bytes() - 1)
    }

    /// Align a size down to this page size
    pub const fn align_down(self, size: usize) -> usize {
        size & !(self.bytes() - 1)
    }
}

/// Memory configuration parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryConfig {
    /// Page size in bytes
    pub page_size: PageSize,
    /// Maximum number of pages
    pub max_pages: usize,
    /// Enable free list optimization (O(1) allocation)
    pub use_free_list: bool,
    /// Enable coalescing of free blocks
    pub enable_coalescing: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            page_size: PageSize::Size4KB,
            max_pages: 1024,
            use_free_list: true,
            enable_coalescing: true,
        }
    }
}

impl MemoryConfig {
    /// Validate memory configuration
    pub fn validate(&self) -> Result<(), KernelError> {
        // max_pages must be at least 1
        if self.max_pages == 0 {
            return Err(KernelError::InvalidPageCount);
        }

        // max_pages should be reasonable (< 1M pages = 4GB for 4KB pages)
        if self.max_pages > 1_000_000 {
            return Err(KernelError::AllocationTooLarge {
                requested: self.max_pages,
                max_pages: 1_000_000,
            });
        }

        Ok(())
    }

    /// Get total memory size in bytes
    pub const fn total_memory_bytes(&self) -> usize {
        self.max_pages * self.page_size.bytes()
    }
}

/// Scheduler configuration parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Maximum number of concurrent tasks
    pub max_tasks: usize,
    /// Enable priority scheduling
    pub enable_priority: bool,
    /// Enable task metrics tracking
    pub enable_metrics: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_tasks: 64,
            enable_priority: true,
            enable_metrics: true,
        }
    }
}

impl SchedulerConfig {
    /// Validate scheduler configuration
    pub fn validate(&self) -> Result<(), KernelError> {
        // max_tasks must be at least 1
        if self.max_tasks == 0 {
            return Err(KernelError::TaskSpawnFailed);
        }

        // max_tasks should be reasonable (< 100K tasks)
        if self.max_tasks > 100_000 {
            return Err(KernelError::TaskSpawnFailed);
        }

        Ok(())
    }
}

/// Multi-core configuration parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiCoreConfig {
    /// Maximum number of CPUs
    pub max_cpus: usize,
    /// Enable per-CPU memory pools
    pub enable_per_cpu_pools: bool,
    /// Enable load balancing
    pub enable_load_balancing: bool,
    /// Enable work stealing
    pub enable_work_stealing: bool,
}

impl Default for MultiCoreConfig {
    fn default() -> Self {
        Self {
            max_cpus: 16,
            enable_per_cpu_pools: true,
            enable_load_balancing: true,
            enable_work_stealing: true,
        }
    }
}

impl MultiCoreConfig {
    /// Validate multi-core configuration
    pub fn validate(&self) -> Result<(), KernelError> {
        // max_cpus must be at least 1
        if self.max_cpus == 0 {
            return Err(KernelError::HardwareNotSupported);
        }

        // max_cpus should be reasonable (< 1024 CPUs)
        if self.max_cpus > 1024 {
            return Err(KernelError::HardwareNotSupported);
        }

        Ok(())
    }
}

/// Complete kernel configuration
#[derive(Debug, Clone, Copy, Default)]
pub struct KernelConfig {
    /// Memory subsystem configuration
    pub memory: MemoryConfig,
    /// Scheduler subsystem configuration
    pub scheduler: SchedulerConfig,
    /// Multi-core subsystem configuration
    pub multicore: MultiCoreConfig,
}

impl KernelConfig {
    /// Create a new configuration builder
    pub fn builder() -> KernelConfigBuilder {
        KernelConfigBuilder::default()
    }

    /// Validate the entire configuration
    pub fn validate(&self) -> Result<(), KernelError> {
        self.memory.validate()?;
        self.scheduler.validate()?;
        self.multicore.validate()?;
        Ok(())
    }

    /// Create a configuration for embedded systems (minimal resources)
    pub fn embedded() -> Self {
        Self {
            memory: MemoryConfig {
                page_size: PageSize::Size4KB,
                max_pages: 128, // 512KB total
                use_free_list: true,
                enable_coalescing: true,
            },
            scheduler: SchedulerConfig {
                max_tasks: 16,
                enable_priority: true,
                enable_metrics: false, // Save memory
            },
            multicore: MultiCoreConfig {
                max_cpus: 1, // Single core
                enable_per_cpu_pools: false,
                enable_load_balancing: false,
                enable_work_stealing: false,
            },
        }
    }

    /// Create a configuration for high-performance systems
    pub fn high_performance() -> Self {
        Self {
            memory: MemoryConfig {
                page_size: PageSize::Size4KB,
                max_pages: 8192, // 32MB total
                use_free_list: true,
                enable_coalescing: true,
            },
            scheduler: SchedulerConfig {
                max_tasks: 256,
                enable_priority: true,
                enable_metrics: true,
            },
            multicore: MultiCoreConfig {
                max_cpus: 32,
                enable_per_cpu_pools: true,
                enable_load_balancing: true,
                enable_work_stealing: true,
            },
        }
    }

    /// Create a configuration for ARM64 systems with 16KB pages
    pub fn arm64_16kb() -> Self {
        Self {
            memory: MemoryConfig {
                page_size: PageSize::Size16KB,
                max_pages: 1024, // 16MB total
                use_free_list: true,
                enable_coalescing: true,
            },
            scheduler: SchedulerConfig::default(),
            multicore: MultiCoreConfig::default(),
        }
    }
}

/// Builder for KernelConfig
#[derive(Debug, Default)]
pub struct KernelConfigBuilder {
    memory: Option<MemoryConfig>,
    scheduler: Option<SchedulerConfig>,
    multicore: Option<MultiCoreConfig>,
    // Individual overrides
    page_size: Option<PageSize>,
    max_pages: Option<usize>,
    max_tasks: Option<usize>,
    max_cpus: Option<usize>,
}

impl KernelConfigBuilder {
    /// Set memory configuration
    pub fn memory(mut self, config: MemoryConfig) -> Self {
        self.memory = Some(config);
        self
    }

    /// Set scheduler configuration
    pub fn scheduler(mut self, config: SchedulerConfig) -> Self {
        self.scheduler = Some(config);
        self
    }

    /// Set multi-core configuration
    pub fn multicore(mut self, config: MultiCoreConfig) -> Self {
        self.multicore = Some(config);
        self
    }

    /// Set page size
    pub fn page_size(mut self, size: PageSize) -> Self {
        self.page_size = Some(size);
        self
    }

    /// Set maximum pages
    pub fn max_pages(mut self, pages: usize) -> Self {
        self.max_pages = Some(pages);
        self
    }

    /// Set maximum tasks
    pub fn max_tasks(mut self, tasks: usize) -> Self {
        self.max_tasks = Some(tasks);
        self
    }

    /// Set maximum CPUs
    pub fn max_cpus(mut self, cpus: usize) -> Self {
        self.max_cpus = Some(cpus);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<KernelConfig, KernelError> {
        let mut config = KernelConfig::default();

        // Apply subsystem configs
        if let Some(mem) = self.memory {
            config.memory = mem;
        }
        if let Some(sched) = self.scheduler {
            config.scheduler = sched;
        }
        if let Some(mc) = self.multicore {
            config.multicore = mc;
        }

        // Apply individual overrides
        if let Some(ps) = self.page_size {
            config.memory.page_size = ps;
        }
        if let Some(mp) = self.max_pages {
            config.memory.max_pages = mp;
        }
        if let Some(mt) = self.max_tasks {
            config.scheduler.max_tasks = mt;
        }
        if let Some(mc) = self.max_cpus {
            config.multicore.max_cpus = mc;
        }

        // Validate before returning
        config.validate()?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_size_bytes() {
        assert_eq!(PageSize::Size4KB.bytes(), 4096);
        assert_eq!(PageSize::Size8KB.bytes(), 8192);
        assert_eq!(PageSize::Size16KB.bytes(), 16384);
        assert_eq!(PageSize::Size64KB.bytes(), 65536);
    }

    #[test]
    fn test_page_size_shift() {
        assert_eq!(PageSize::Size4KB.shift(), 12);
        assert_eq!(PageSize::Size8KB.shift(), 13);
        assert_eq!(PageSize::Size16KB.shift(), 14);
        assert_eq!(PageSize::Size64KB.shift(), 16);
    }

    #[test]
    fn test_page_size_is_aligned() {
        assert!(PageSize::Size4KB.is_aligned(4096));
        assert!(PageSize::Size4KB.is_aligned(8192));
        assert!(!PageSize::Size4KB.is_aligned(4097));

        assert!(PageSize::Size16KB.is_aligned(16384));
        assert!(PageSize::Size16KB.is_aligned(32768));
        assert!(!PageSize::Size16KB.is_aligned(16385));
    }

    #[test]
    fn test_page_size_align_up() {
        assert_eq!(PageSize::Size4KB.align_up(0), 0);
        assert_eq!(PageSize::Size4KB.align_up(1), 4096);
        assert_eq!(PageSize::Size4KB.align_up(4096), 4096);
        assert_eq!(PageSize::Size4KB.align_up(4097), 8192);

        assert_eq!(PageSize::Size16KB.align_up(16383), 16384);
        assert_eq!(PageSize::Size16KB.align_up(16384), 16384);
        assert_eq!(PageSize::Size16KB.align_up(16385), 32768);
    }

    #[test]
    fn test_page_size_align_down() {
        assert_eq!(PageSize::Size4KB.align_down(0), 0);
        assert_eq!(PageSize::Size4KB.align_down(4095), 0);
        assert_eq!(PageSize::Size4KB.align_down(4096), 4096);
        assert_eq!(PageSize::Size4KB.align_down(4097), 4096);
        assert_eq!(PageSize::Size4KB.align_down(8191), 4096);

        assert_eq!(PageSize::Size16KB.align_down(16383), 0);
        assert_eq!(PageSize::Size16KB.align_down(16384), 16384);
        assert_eq!(PageSize::Size16KB.align_down(32767), 16384);
    }

    #[test]
    fn test_default_config() {
        let config = KernelConfig::default();
        assert_eq!(config.memory.page_size, PageSize::Size4KB);
        assert_eq!(config.memory.max_pages, 1024);
        assert_eq!(config.scheduler.max_tasks, 64);
        assert_eq!(config.multicore.max_cpus, 16);
    }

    #[test]
    fn test_embedded_config() {
        let config = KernelConfig::embedded();
        assert_eq!(config.memory.max_pages, 128);
        assert_eq!(config.scheduler.max_tasks, 16);
        assert_eq!(config.multicore.max_cpus, 1);
        assert!(!config.scheduler.enable_metrics);
    }

    #[test]
    fn test_high_performance_config() {
        let config = KernelConfig::high_performance();
        assert_eq!(config.memory.max_pages, 8192);
        assert_eq!(config.scheduler.max_tasks, 256);
        assert_eq!(config.multicore.max_cpus, 32);
    }

    #[test]
    fn test_arm64_16kb_config() {
        let config = KernelConfig::arm64_16kb();
        assert_eq!(config.memory.page_size, PageSize::Size16KB);
        assert_eq!(config.memory.max_pages, 1024);
    }

    #[test]
    fn test_builder() {
        let config = KernelConfig::builder()
            .max_pages(2048)
            .max_tasks(128)
            .max_cpus(8)
            .page_size(PageSize::Size8KB)
            .build()
            .unwrap();

        assert_eq!(config.memory.max_pages, 2048);
        assert_eq!(config.scheduler.max_tasks, 128);
        assert_eq!(config.multicore.max_cpus, 8);
        assert_eq!(config.memory.page_size, PageSize::Size8KB);
    }

    #[test]
    fn test_builder_validation() {
        // Zero pages should fail
        let result = KernelConfig::builder().max_pages(0).build();
        assert!(result.is_err());

        // Zero tasks should fail
        let result = KernelConfig::builder().max_tasks(0).build();
        assert!(result.is_err());

        // Zero CPUs should fail
        let result = KernelConfig::builder().max_cpus(0).build();
        assert!(result.is_err());

        // Too many pages should fail
        let result = KernelConfig::builder().max_pages(2_000_000).build();
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_config_total_bytes() {
        let config = MemoryConfig {
            page_size: PageSize::Size4KB,
            max_pages: 1024,
            use_free_list: true,
            enable_coalescing: true,
        };
        assert_eq!(config.total_memory_bytes(), 4_194_304); // 4MB

        let config = MemoryConfig {
            page_size: PageSize::Size16KB,
            max_pages: 1024,
            use_free_list: true,
            enable_coalescing: true,
        };
        assert_eq!(config.total_memory_bytes(), 16_777_216); // 16MB
    }

    #[test]
    fn test_config_validation() {
        let mut config = KernelConfig::default();
        assert!(config.validate().is_ok());

        // Invalid memory config
        config.memory.max_pages = 0;
        assert!(config.validate().is_err());

        config.memory.max_pages = 1024;
        assert!(config.validate().is_ok());

        // Invalid scheduler config
        config.scheduler.max_tasks = 0;
        assert!(config.validate().is_err());

        config.scheduler.max_tasks = 64;
        assert!(config.validate().is_ok());

        // Invalid multicore config
        config.multicore.max_cpus = 0;
        assert!(config.validate().is_err());
    }
}
