//! Stack Management for Embedded Runtime
//!
//! Provides stack overflow detection, usage monitoring, and per-task stack allocation.
//!
//! ## Features
//!
//! - Stack canary-based overflow detection
//! - Stack watermark monitoring for high water mark tracking
//! - Per-task stack allocation with configurable sizes
//! - Stack usage statistics and alerts

#![allow(dead_code)]

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Stack canary magic value - used to detect overflow
/// Using a distinctive pattern that's unlikely to occur naturally
const STACK_CANARY: u32 = 0xDEAD_BEEF;

/// Stack fill pattern for watermark detection
const STACK_FILL_PATTERN: u8 = 0xA5;

/// Maximum number of task stacks to track
const MAX_TASK_STACKS: usize = 16;

/// Stack overflow detection result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackStatus {
    /// Stack is healthy
    Healthy,
    /// Stack overflow detected (canary corrupted)
    Overflow,
    /// Stack usage is high (>80%)
    HighUsage,
    /// Stack usage is critical (>95%)
    Critical,
}

/// Stack configuration for a task
#[derive(Debug, Clone, Copy)]
pub struct StackConfig {
    /// Stack size in bytes
    pub size: usize,
    /// Enable canary-based overflow detection
    pub enable_canary: bool,
    /// Enable fill pattern for watermark tracking
    pub enable_watermark: bool,
    /// High water mark threshold percentage (0-100)
    pub high_threshold: u8,
    /// Critical threshold percentage (0-100)
    pub critical_threshold: u8,
}

impl Default for StackConfig {
    fn default() -> Self {
        Self {
            size: 4096, // 4KB default
            enable_canary: true,
            enable_watermark: true,
            high_threshold: 80,
            critical_threshold: 95,
        }
    }
}

impl StackConfig {
    /// Create configuration for a minimal stack (512 bytes)
    pub const fn minimal() -> Self {
        Self {
            size: 512,
            enable_canary: true,
            enable_watermark: false, // Save memory
            high_threshold: 90,
            critical_threshold: 98,
        }
    }

    /// Create configuration for a standard stack (2KB)
    pub const fn standard() -> Self {
        Self {
            size: 2048,
            enable_canary: true,
            enable_watermark: true,
            high_threshold: 80,
            critical_threshold: 95,
        }
    }

    /// Create configuration for a large stack (8KB)
    pub const fn large() -> Self {
        Self {
            size: 8192,
            enable_canary: true,
            enable_watermark: true,
            high_threshold: 75,
            critical_threshold: 90,
        }
    }
}

/// Stack statistics
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StackStats {
    /// Total stack size
    pub total_bytes: usize,
    /// Current estimated usage (high water mark)
    pub used_bytes: usize,
    /// Peak usage ever recorded
    pub peak_bytes: usize,
    /// Number of canary checks performed
    pub canary_checks: u64,
    /// Number of overflow events detected
    pub overflow_events: u64,
}

impl StackStats {
    /// Get usage percentage (0-100)
    pub fn usage_percent(&self) -> u8 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.used_bytes * 100) / self.total_bytes).min(100) as u8
    }

    /// Get peak usage percentage (0-100)
    pub fn peak_percent(&self) -> u8 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.peak_bytes * 100) / self.total_bytes).min(100) as u8
    }
}

/// Stack descriptor for a single task
#[derive(Debug)]
pub struct StackDescriptor {
    /// Task ID
    task_id: u32,
    /// Stack base address (low address)
    base: usize,
    /// Stack size in bytes
    size: usize,
    /// Stack top address (high address, where SP starts)
    top: usize,
    /// Configuration
    config: StackConfig,
    /// Current high water mark (bytes from base)
    high_water_mark: AtomicUsize,
    /// Canary check count
    canary_checks: AtomicUsize,
    /// Overflow detected flag
    overflow_detected: AtomicU32,
}

impl StackDescriptor {
    /// Create a new stack descriptor
    pub fn new(task_id: u32, base: usize, size: usize, config: StackConfig) -> Self {
        Self {
            task_id,
            base,
            size,
            top: base + size,
            config,
            high_water_mark: AtomicUsize::new(0),
            canary_checks: AtomicUsize::new(0),
            overflow_detected: AtomicU32::new(0),
        }
    }

    /// Get task ID
    pub fn task_id(&self) -> u32 {
        self.task_id
    }

    /// Get stack base address
    pub fn base(&self) -> usize {
        self.base
    }

    /// Get stack size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get stack top address
    pub fn top(&self) -> usize {
        self.top
    }

    /// Initialize stack with canary and fill pattern
    ///
    /// # Safety
    /// The caller must ensure the stack memory region is valid and writable
    pub unsafe fn initialize(&self) {
        let base_ptr = self.base as *mut u8;

        // Place canary at bottom of stack
        if self.config.enable_canary {
            let canary_ptr = base_ptr as *mut u32;
            *canary_ptr = STACK_CANARY;
        }

        // Fill stack with pattern for watermark tracking
        if self.config.enable_watermark {
            let start_offset = if self.config.enable_canary { 4 } else { 0 };
            let fill_ptr = base_ptr.add(start_offset);
            let fill_len = self.size - start_offset;
            core::ptr::write_bytes(fill_ptr, STACK_FILL_PATTERN, fill_len);
        }
    }

    /// Check for stack overflow via canary
    ///
    /// # Safety
    /// The caller must ensure the stack memory is still valid
    pub unsafe fn check_canary(&self) -> bool {
        if !self.config.enable_canary {
            return true; // Assume OK if canary disabled
        }

        self.canary_checks.fetch_add(1, Ordering::Relaxed);

        let canary_ptr = self.base as *const u32;
        let canary_value = *canary_ptr;

        if canary_value != STACK_CANARY {
            self.overflow_detected.store(1, Ordering::Release);
            false
        } else {
            true
        }
    }

    /// Calculate high water mark from fill pattern
    ///
    /// Stack grows downward: SP starts at top and decreases.
    /// We scan from bottom (base) upward to find where pattern is still intact.
    /// The first non-pattern byte from bottom indicates maximum stack usage.
    ///
    /// # Safety
    /// The caller must ensure the stack memory is valid
    pub unsafe fn calculate_watermark(&self) -> usize {
        if !self.config.enable_watermark {
            return 0;
        }

        let start_offset = if self.config.enable_canary { 4 } else { 0 };
        let base_ptr = (self.base + start_offset) as *const u8;
        let check_len = self.size - start_offset;

        // Scan from bottom (base) upward to find where pattern is still intact
        // The position of first non-pattern byte tells us nothing was written there yet
        // If pattern is intact up to position i, stack usage is (check_len - i)
        let mut intact_bytes = 0;
        for i in 0..check_len {
            if *base_ptr.add(i) == STACK_FILL_PATTERN {
                intact_bytes += 1;
            } else {
                // Found used area - everything above this is used
                break;
            }
        }

        // Used bytes = total - intact
        let used = check_len - intact_bytes;

        // Update atomic high water mark
        let _ = self.high_water_mark.fetch_max(used, Ordering::Relaxed);

        used
    }

    /// Get current stack status
    ///
    /// # Safety
    /// The caller must ensure the stack memory is valid
    pub unsafe fn status(&self) -> StackStatus {
        // Check overflow first
        if self.overflow_detected.load(Ordering::Acquire) != 0 || !self.check_canary() {
            return StackStatus::Overflow;
        }

        // Check usage levels
        let usage = self.calculate_watermark();
        let usage_percent = (usage * 100 / self.size) as u8;

        if usage_percent >= self.config.critical_threshold {
            StackStatus::Critical
        } else if usage_percent >= self.config.high_threshold {
            StackStatus::HighUsage
        } else {
            StackStatus::Healthy
        }
    }

    /// Get statistics
    pub fn stats(&self) -> StackStats {
        StackStats {
            total_bytes: self.size,
            used_bytes: self.high_water_mark.load(Ordering::Relaxed),
            peak_bytes: self.high_water_mark.load(Ordering::Relaxed),
            canary_checks: self.canary_checks.load(Ordering::Relaxed) as u64,
            overflow_events: self.overflow_detected.load(Ordering::Relaxed) as u64,
        }
    }
}

/// Stack manager for multiple tasks
pub struct StackManager {
    /// Task descriptors
    descriptors: [Option<StackDescriptor>; MAX_TASK_STACKS],
    /// Number of active stacks
    active_count: usize,
    /// Total overflow events
    total_overflows: AtomicUsize,
}

impl StackManager {
    /// Create a new stack manager
    pub const fn new() -> Self {
        Self {
            descriptors: [
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None,
            ],
            active_count: 0,
            total_overflows: AtomicUsize::new(0),
        }
    }

    /// Register a stack for a task
    pub fn register_stack(
        &mut self,
        task_id: u32,
        base: usize,
        size: usize,
        config: StackConfig,
    ) -> Result<usize, StackError> {
        // Find free slot
        let slot = self
            .descriptors
            .iter()
            .position(|d| d.is_none())
            .ok_or(StackError::TooManyStacks)?;

        self.descriptors[slot] = Some(StackDescriptor::new(task_id, base, size, config));
        self.active_count += 1;

        Ok(slot)
    }

    /// Unregister a stack
    pub fn unregister_stack(&mut self, task_id: u32) -> Result<(), StackError> {
        for desc in &mut self.descriptors {
            if let Some(d) = desc {
                if d.task_id == task_id {
                    *desc = None;
                    self.active_count -= 1;
                    return Ok(());
                }
            }
        }
        Err(StackError::NotFound)
    }

    /// Initialize a registered stack
    ///
    /// # Safety
    /// The caller must ensure the stack memory is valid
    pub unsafe fn initialize_stack(&self, task_id: u32) -> Result<(), StackError> {
        for d in self.descriptors.iter().flatten() {
            if d.task_id == task_id {
                d.initialize();
                return Ok(());
            }
        }
        Err(StackError::NotFound)
    }

    /// Check all stacks for overflow
    ///
    /// # Safety
    /// The caller must ensure all stack memory regions are valid
    pub unsafe fn check_all_stacks(&self) -> Vec<(u32, StackStatus)> {
        let mut results = Vec::new();

        for d in self.descriptors.iter().flatten() {
            let status = d.status();
            if status == StackStatus::Overflow {
                self.total_overflows.fetch_add(1, Ordering::Relaxed);
            }
            results.push((d.task_id, status));
        }

        results
    }

    /// Get status for a specific task
    ///
    /// # Safety
    /// The caller must ensure the stack memory is valid
    pub unsafe fn get_task_status(&self, task_id: u32) -> Result<StackStatus, StackError> {
        for d in self.descriptors.iter().flatten() {
            if d.task_id == task_id {
                return Ok(d.status());
            }
        }
        Err(StackError::NotFound)
    }

    /// Get statistics for a specific task
    pub fn get_task_stats(&self, task_id: u32) -> Result<StackStats, StackError> {
        for d in self.descriptors.iter().flatten() {
            if d.task_id == task_id {
                return Ok(d.stats());
            }
        }
        Err(StackError::NotFound)
    }

    /// Get total number of active stacks
    pub fn active_count(&self) -> usize {
        self.active_count
    }

    /// Get total overflow events
    pub fn total_overflows(&self) -> usize {
        self.total_overflows.load(Ordering::Relaxed)
    }

    /// Get aggregate statistics
    pub fn aggregate_stats(&self) -> AggregateStackStats {
        let mut total_bytes = 0usize;
        let mut used_bytes = 0usize;
        let mut peak_bytes = 0usize;
        let mut healthy = 0usize;
        let mut high_usage = 0usize;
        let mut critical = 0usize;

        for d in self.descriptors.iter().flatten() {
            let stats = d.stats();
            total_bytes += stats.total_bytes;
            used_bytes += stats.used_bytes;
            peak_bytes += stats.peak_bytes;

            // Estimate status from stats
            let usage = stats.usage_percent();
            if usage >= 95 {
                critical += 1;
            } else if usage >= 80 {
                high_usage += 1;
            } else {
                healthy += 1;
            }
        }

        AggregateStackStats {
            active_stacks: self.active_count,
            total_bytes,
            used_bytes,
            peak_bytes,
            healthy_stacks: healthy,
            high_usage_stacks: high_usage,
            critical_stacks: critical,
            overflow_events: self.total_overflows.load(Ordering::Relaxed),
        }
    }
}

impl Default for StackManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate statistics across all stacks
#[derive(Debug, Clone, Copy, Default)]
pub struct AggregateStackStats {
    /// Number of active stacks
    pub active_stacks: usize,
    /// Total bytes allocated
    pub total_bytes: usize,
    /// Total bytes used
    pub used_bytes: usize,
    /// Total peak bytes
    pub peak_bytes: usize,
    /// Number of healthy stacks
    pub healthy_stacks: usize,
    /// Number of high usage stacks
    pub high_usage_stacks: usize,
    /// Number of critical stacks
    pub critical_stacks: usize,
    /// Total overflow events
    pub overflow_events: usize,
}

impl AggregateStackStats {
    /// Get average usage percentage
    pub fn avg_usage_percent(&self) -> u8 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.used_bytes * 100) / self.total_bytes).min(100) as u8
    }
}

/// Stack-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackError {
    /// Too many stacks registered
    TooManyStacks,
    /// Task stack not found
    NotFound,
    /// Stack overflow detected
    Overflow,
    /// Invalid configuration
    InvalidConfig,
}

extern crate alloc;
use alloc::vec::Vec;

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec;

    #[test]
    fn test_stack_config_default() {
        let config = StackConfig::default();
        assert_eq!(config.size, 4096);
        assert!(config.enable_canary);
        assert!(config.enable_watermark);
        assert_eq!(config.high_threshold, 80);
        assert_eq!(config.critical_threshold, 95);
    }

    #[test]
    fn test_stack_config_presets() {
        let minimal = StackConfig::minimal();
        assert_eq!(minimal.size, 512);
        assert!(!minimal.enable_watermark);

        let standard = StackConfig::standard();
        assert_eq!(standard.size, 2048);

        let large = StackConfig::large();
        assert_eq!(large.size, 8192);
    }

    #[test]
    fn test_stack_stats() {
        let stats = StackStats {
            total_bytes: 1000,
            used_bytes: 500,
            peak_bytes: 750,
            canary_checks: 10,
            overflow_events: 0,
        };

        assert_eq!(stats.usage_percent(), 50);
        assert_eq!(stats.peak_percent(), 75);
    }

    #[test]
    fn test_stack_stats_zero_total() {
        let stats = StackStats::default();
        assert_eq!(stats.usage_percent(), 0);
        assert_eq!(stats.peak_percent(), 0);
    }

    #[test]
    fn test_stack_descriptor_creation() {
        let config = StackConfig::default();
        let desc = StackDescriptor::new(1, 0x2000_0000, 4096, config);

        assert_eq!(desc.task_id(), 1);
        assert_eq!(desc.base(), 0x2000_0000);
        assert_eq!(desc.size(), 4096);
        assert_eq!(desc.top(), 0x2000_1000);
    }

    #[test]
    fn test_stack_descriptor_stats() {
        let config = StackConfig::default();
        let desc = StackDescriptor::new(1, 0x2000_0000, 4096, config);

        let stats = desc.stats();
        assert_eq!(stats.total_bytes, 4096);
        assert_eq!(stats.used_bytes, 0);
        assert_eq!(stats.overflow_events, 0);
    }

    #[test]
    fn test_stack_manager_creation() {
        let manager = StackManager::new();
        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.total_overflows(), 0);
    }

    #[test]
    fn test_stack_manager_register() {
        let mut manager = StackManager::new();

        let slot = manager
            .register_stack(1, 0x2000_0000, 4096, StackConfig::default())
            .unwrap();
        assert_eq!(slot, 0);
        assert_eq!(manager.active_count(), 1);

        let slot2 = manager
            .register_stack(2, 0x2000_1000, 4096, StackConfig::default())
            .unwrap();
        assert_eq!(slot2, 1);
        assert_eq!(manager.active_count(), 2);
    }

    #[test]
    fn test_stack_manager_unregister() {
        let mut manager = StackManager::new();

        manager
            .register_stack(1, 0x2000_0000, 4096, StackConfig::default())
            .unwrap();
        manager
            .register_stack(2, 0x2000_1000, 4096, StackConfig::default())
            .unwrap();
        assert_eq!(manager.active_count(), 2);

        manager.unregister_stack(1).unwrap();
        assert_eq!(manager.active_count(), 1);

        assert_eq!(manager.unregister_stack(999), Err(StackError::NotFound));
    }

    #[test]
    fn test_stack_manager_max_stacks() {
        let mut manager = StackManager::new();

        // Fill all slots
        for i in 0..MAX_TASK_STACKS {
            manager
                .register_stack(
                    i as u32,
                    0x2000_0000 + i * 0x1000,
                    4096,
                    StackConfig::default(),
                )
                .unwrap();
        }

        // Should fail on next
        let result = manager.register_stack(100, 0x3000_0000, 4096, StackConfig::default());
        assert_eq!(result, Err(StackError::TooManyStacks));
    }

    #[test]
    fn test_stack_manager_get_stats() {
        let mut manager = StackManager::new();

        manager
            .register_stack(42, 0x2000_0000, 4096, StackConfig::default())
            .unwrap();

        let stats = manager.get_task_stats(42).unwrap();
        assert_eq!(stats.total_bytes, 4096);

        assert_eq!(manager.get_task_stats(999), Err(StackError::NotFound));
    }

    #[test]
    fn test_aggregate_stats() {
        let mut manager = StackManager::new();

        manager
            .register_stack(1, 0x2000_0000, 2048, StackConfig::default())
            .unwrap();
        manager
            .register_stack(2, 0x2000_1000, 4096, StackConfig::default())
            .unwrap();

        let agg = manager.aggregate_stats();
        assert_eq!(agg.active_stacks, 2);
        assert_eq!(agg.total_bytes, 6144);
        assert_eq!(agg.healthy_stacks, 2);
        assert_eq!(agg.overflow_events, 0);
    }

    #[test]
    fn test_stack_canary_check() {
        let mut buffer = vec![0u8; 4096];
        let base = buffer.as_mut_ptr() as usize;
        let config = StackConfig::default();
        let desc = StackDescriptor::new(1, base, 4096, config);

        unsafe {
            // Initialize stack (places canary)
            desc.initialize();

            // Check canary should pass
            assert!(desc.check_canary());
            assert_eq!(desc.stats().canary_checks, 1);

            // Corrupt canary
            let canary_ptr = base as *mut u32;
            *canary_ptr = 0x12345678;

            // Check should fail
            assert!(!desc.check_canary());
            assert_eq!(desc.stats().overflow_events, 1);
        }
    }

    #[test]
    fn test_stack_watermark_calculation() {
        let mut buffer = vec![0u8; 1024];
        let base = buffer.as_mut_ptr() as usize;
        let config = StackConfig {
            size: 1024,
            enable_canary: true,
            enable_watermark: true,
            high_threshold: 80,
            critical_threshold: 95,
        };
        let desc = StackDescriptor::new(1, base, 1024, config);

        unsafe {
            // Initialize (fills with pattern)
            desc.initialize();

            // Initially, watermark should be 0 (all pattern)
            let used = desc.calculate_watermark();
            assert_eq!(used, 0);

            // Simulate stack usage by overwriting top 100 bytes
            let stack_top = (base + 1024 - 100) as *mut u8;
            for i in 0..100 {
                *stack_top.add(i) = 0x00;
            }

            // Now watermark should detect ~100 bytes used
            let used = desc.calculate_watermark();
            assert!(used >= 100);
        }
    }

    #[test]
    fn test_stack_status_detection() {
        let mut buffer = vec![0u8; 1024];
        let base = buffer.as_mut_ptr() as usize;
        let config = StackConfig {
            size: 1024,
            enable_canary: true,
            enable_watermark: true,
            high_threshold: 50,     // 50% threshold for testing
            critical_threshold: 80, // 80% critical
        };
        let desc = StackDescriptor::new(1, base, 1024, config);

        unsafe {
            desc.initialize();

            // Initially healthy
            let status = desc.status();
            assert_eq!(status, StackStatus::Healthy);
        }
    }

    #[test]
    fn test_aggregate_stats_usage() {
        let agg = AggregateStackStats {
            active_stacks: 2,
            total_bytes: 1000,
            used_bytes: 500,
            peak_bytes: 750,
            healthy_stacks: 2,
            high_usage_stacks: 0,
            critical_stacks: 0,
            overflow_events: 0,
        };

        assert_eq!(agg.avg_usage_percent(), 50);
    }
}
