//! Fault Recovery System
//!
//! This module provides comprehensive fault detection, logging, and recovery
//! mechanisms for embedded systems to ensure high reliability and availability.
//!
//! # Features
//!
//! - Fault detection and classification
//! - Automatic recovery strategies
//! - Fault logging with circular buffer
//! - Boot-loop detection and prevention
//! - Crash dump generation
//! - Safe mode operation
//! - Recovery policy configuration
//! - Hierarchical fault handling
//!
//! # Fault Types
//!
//! - Hardware faults (peripherals, memory)
//! - Software faults (panics, assertions)
//! - Communication faults (timeouts, disconnections)
//! - Power faults (brownout, undervoltage)
//! - Watchdog timeouts
//! - Stack overflows
//!
//! # Recovery Strategies
//!
//! - Task restart
//! - System reboot
//! - Safe mode
//! - Factory reset
//! - Firmware rollback

#![allow(dead_code)]

use core::fmt;
use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum fault log entries
pub const MAX_FAULT_LOG_ENTRIES: usize = 64;

/// Maximum crash dump size
pub const MAX_CRASH_DUMP_SIZE: usize = 512;

/// Maximum number of recovery handlers
pub const MAX_RECOVERY_HANDLERS: usize = 16;

/// Fault error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultError {
    /// Log full
    LogFull,
    /// Invalid fault type
    InvalidFaultType,
    /// Recovery failed
    RecoveryFailed,
    /// No recovery handler
    NoRecoveryHandler,
    /// Too many handlers
    TooManyHandlers,
    /// Not initialized
    NotInitialized,
}

impl fmt::Display for FaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LogFull => write!(f, "Fault log full"),
            Self::InvalidFaultType => write!(f, "Invalid fault type"),
            Self::RecoveryFailed => write!(f, "Recovery failed"),
            Self::NoRecoveryHandler => write!(f, "No recovery handler"),
            Self::TooManyHandlers => write!(f, "Too many handlers"),
            Self::NotInitialized => write!(f, "Not initialized"),
        }
    }
}

/// Fault type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultType {
    /// Hardware fault
    Hardware,
    /// Software fault (panic, assertion)
    Software,
    /// Communication fault
    Communication,
    /// Power fault
    Power,
    /// Watchdog timeout
    Watchdog,
    /// Stack overflow
    StackOverflow,
    /// Memory fault
    Memory,
    /// Peripheral fault
    Peripheral,
    /// Unknown fault
    Unknown,
}

/// Fault severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FaultSeverity {
    /// Informational
    Info,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Critical
    Critical,
    /// Fatal
    Fatal,
}

/// Recovery action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No action required
    None,
    /// Retry operation
    Retry,
    /// Restart task
    RestartTask,
    /// Reset peripheral
    ResetPeripheral,
    /// Enter safe mode
    SafeMode,
    /// Reboot system
    Reboot,
    /// Firmware rollback
    Rollback,
    /// Factory reset
    FactoryReset,
}

/// Fault source information
#[derive(Debug, Clone, Copy)]
pub struct FaultSource {
    /// Source file (encoded as u32 for no_std)
    pub file_id: u32,
    /// Line number
    pub line: u32,
    /// Function/module ID
    pub function_id: u32,
}

impl FaultSource {
    /// Create a new fault source
    pub const fn new(file_id: u32, line: u32, function_id: u32) -> Self {
        Self {
            file_id,
            line,
            function_id,
        }
    }

    /// Unknown source
    pub const fn unknown() -> Self {
        Self {
            file_id: 0,
            line: 0,
            function_id: 0,
        }
    }
}

/// Fault log entry
#[derive(Debug, Clone, Copy)]
pub struct FaultLogEntry {
    /// Fault type
    pub fault_type: FaultType,
    /// Severity
    pub severity: FaultSeverity,
    /// Timestamp (milliseconds)
    pub timestamp_ms: u32,
    /// Source information
    pub source: FaultSource,
    /// Error code
    pub error_code: u32,
    /// Recovery action taken
    pub recovery_action: RecoveryAction,
    /// Recovery successful
    pub recovery_successful: bool,
}

impl FaultLogEntry {
    /// Create a new fault log entry
    pub const fn new(
        fault_type: FaultType,
        severity: FaultSeverity,
        timestamp_ms: u32,
        source: FaultSource,
        error_code: u32,
    ) -> Self {
        Self {
            fault_type,
            severity,
            timestamp_ms,
            source,
            error_code,
            recovery_action: RecoveryAction::None,
            recovery_successful: false,
        }
    }

    /// Set recovery action
    pub fn with_recovery(mut self, action: RecoveryAction, successful: bool) -> Self {
        self.recovery_action = action;
        self.recovery_successful = successful;
        self
    }
}

/// Circular fault log
pub struct FaultLog {
    /// Log entries
    entries: [Option<FaultLogEntry>; MAX_FAULT_LOG_ENTRIES],
    /// Write index
    write_index: usize,
    /// Entry count
    entry_count: usize,
    /// Overflow count
    overflow_count: u32,
}

impl Default for FaultLog {
    fn default() -> Self {
        Self::new()
    }
}

impl FaultLog {
    /// Create a new fault log
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_FAULT_LOG_ENTRIES],
            write_index: 0,
            entry_count: 0,
            overflow_count: 0,
        }
    }

    /// Add an entry
    pub fn add(&mut self, entry: FaultLogEntry) {
        self.entries[self.write_index] = Some(entry);
        self.write_index = (self.write_index + 1) % MAX_FAULT_LOG_ENTRIES;

        if self.entry_count < MAX_FAULT_LOG_ENTRIES {
            self.entry_count += 1;
        } else {
            self.overflow_count += 1;
        }
    }

    /// Get most recent entries
    pub fn recent(&self, count: usize) -> heapless::Vec<FaultLogEntry, MAX_FAULT_LOG_ENTRIES> {
        let mut result = heapless::Vec::new();
        let take = count.min(self.entry_count);

        let mut idx = if self.write_index == 0 {
            MAX_FAULT_LOG_ENTRIES - 1
        } else {
            self.write_index - 1
        };

        for _ in 0..take {
            if let Some(entry) = self.entries[idx] {
                let _ = result.push(entry);
            }

            idx = if idx == 0 {
                MAX_FAULT_LOG_ENTRIES - 1
            } else {
                idx - 1
            };
        }

        result
    }

    /// Get entries by fault type
    pub fn by_type(
        &self,
        fault_type: FaultType,
    ) -> heapless::Vec<FaultLogEntry, MAX_FAULT_LOG_ENTRIES> {
        let mut result = heapless::Vec::new();

        for entry in self.entries.iter().filter_map(|e| e.as_ref()) {
            if entry.fault_type == fault_type {
                let _ = result.push(*entry);
            }
        }

        result
    }

    /// Get entries by severity
    pub fn by_severity(
        &self,
        min_severity: FaultSeverity,
    ) -> heapless::Vec<FaultLogEntry, MAX_FAULT_LOG_ENTRIES> {
        let mut result = heapless::Vec::new();

        for entry in self.entries.iter().filter_map(|e| e.as_ref()) {
            if entry.severity >= min_severity {
                let _ = result.push(*entry);
            }
        }

        result
    }

    /// Get entry count
    pub const fn count(&self) -> usize {
        self.entry_count
    }

    /// Get overflow count
    pub const fn overflow_count(&self) -> u32 {
        self.overflow_count
    }

    /// Clear the log
    pub fn clear(&mut self) {
        self.entries = [None; MAX_FAULT_LOG_ENTRIES];
        self.write_index = 0;
        self.entry_count = 0;
        // Keep overflow count for statistics
    }
}

/// Recovery policy
#[derive(Debug, Clone, Copy)]
pub struct RecoveryPolicy {
    /// Maximum retry attempts
    pub max_retries: u8,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u32,
    /// Enable safe mode on critical faults
    pub safe_mode_on_critical: bool,
    /// Reboot on fatal faults
    pub reboot_on_fatal: bool,
    /// Maximum reboots before factory reset
    pub max_reboots: u32,
}

impl RecoveryPolicy {
    /// Create a conservative policy
    pub const fn conservative() -> Self {
        Self {
            max_retries: 3,
            retry_delay_ms: 1000,
            safe_mode_on_critical: true,
            reboot_on_fatal: true,
            max_reboots: 5,
        }
    }

    /// Create an aggressive policy
    pub const fn aggressive() -> Self {
        Self {
            max_retries: 10,
            retry_delay_ms: 100,
            safe_mode_on_critical: false,
            reboot_on_fatal: true,
            max_reboots: 10,
        }
    }

    /// Create a minimal policy (development)
    pub const fn minimal() -> Self {
        Self {
            max_retries: 1,
            retry_delay_ms: 0,
            safe_mode_on_critical: false,
            reboot_on_fatal: false,
            max_reboots: 0,
        }
    }
}

/// System state for fault recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    /// Normal operation
    Normal,
    /// Safe mode (limited functionality)
    SafeMode,
    /// Recovery mode
    Recovery,
    /// Factory reset mode
    FactoryReset,
}

/// Fault recovery manager
pub struct FaultRecoveryManager {
    /// Fault log
    log: FaultLog,
    /// Recovery policy
    policy: RecoveryPolicy,
    /// Current system state
    state: SystemState,
    /// Reboot counter
    reboot_count: AtomicU32,
    /// Fault counter by type
    fault_counts: [AtomicU32; 9], // One for each FaultType
    /// System time getter
    system_time_ms: fn() -> u32,
    /// Initialized
    initialized: bool,
}

impl FaultRecoveryManager {
    /// Create a new fault recovery manager
    pub fn new(policy: RecoveryPolicy, system_time_ms: fn() -> u32) -> Self {
        Self {
            log: FaultLog::new(),
            policy,
            state: SystemState::Normal,
            reboot_count: AtomicU32::new(0),
            fault_counts: [
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
            ],
            system_time_ms,
            initialized: true,
        }
    }

    /// Record a fault
    pub fn record_fault(
        &mut self,
        fault_type: FaultType,
        severity: FaultSeverity,
        source: FaultSource,
        error_code: u32,
    ) {
        if !self.initialized {
            return;
        }

        let timestamp = (self.system_time_ms)();
        let entry = FaultLogEntry::new(fault_type, severity, timestamp, source, error_code);

        self.log.add(entry);
        self.increment_fault_count(fault_type);
    }

    /// Handle a fault with automatic recovery
    pub fn handle_fault(
        &mut self,
        fault_type: FaultType,
        severity: FaultSeverity,
        source: FaultSource,
        error_code: u32,
    ) -> RecoveryAction {
        let timestamp = (self.system_time_ms)();
        let action = self.determine_recovery_action(fault_type, severity);

        let successful = self.execute_recovery(action);

        let entry = FaultLogEntry::new(fault_type, severity, timestamp, source, error_code)
            .with_recovery(action, successful);

        self.log.add(entry);
        self.increment_fault_count(fault_type);

        action
    }

    /// Determine appropriate recovery action
    fn determine_recovery_action(
        &self,
        fault_type: FaultType,
        severity: FaultSeverity,
    ) -> RecoveryAction {
        match severity {
            FaultSeverity::Info | FaultSeverity::Warning => RecoveryAction::None,
            FaultSeverity::Error => match fault_type {
                FaultType::Communication => RecoveryAction::Retry,
                FaultType::Peripheral => RecoveryAction::ResetPeripheral,
                _ => RecoveryAction::RestartTask,
            },
            FaultSeverity::Critical => {
                if self.policy.safe_mode_on_critical {
                    RecoveryAction::SafeMode
                } else {
                    RecoveryAction::Reboot
                }
            }
            FaultSeverity::Fatal => {
                if self.policy.reboot_on_fatal {
                    let reboot_count = self.reboot_count.load(Ordering::Acquire);
                    if reboot_count >= self.policy.max_reboots {
                        RecoveryAction::FactoryReset
                    } else {
                        RecoveryAction::Reboot
                    }
                } else {
                    RecoveryAction::SafeMode
                }
            }
        }
    }

    /// Execute recovery action
    fn execute_recovery(&mut self, action: RecoveryAction) -> bool {
        match action {
            RecoveryAction::None => true,
            RecoveryAction::Retry => {
                // In a real implementation, retry would be handled by the caller
                true
            }
            RecoveryAction::RestartTask => {
                // In a real implementation, this would restart the faulted task
                true
            }
            RecoveryAction::ResetPeripheral => {
                // In a real implementation, this would reset the peripheral
                true
            }
            RecoveryAction::SafeMode => {
                self.state = SystemState::SafeMode;
                true
            }
            RecoveryAction::Reboot => {
                self.reboot_count.fetch_add(1, Ordering::AcqRel);
                // In a real implementation, this would trigger a reboot
                false // Cannot verify success before reboot
            }
            RecoveryAction::Rollback => {
                // In a real implementation, this would trigger firmware rollback
                true
            }
            RecoveryAction::FactoryReset => {
                self.state = SystemState::FactoryReset;
                // In a real implementation, this would erase user data
                true
            }
        }
    }

    /// Increment fault counter for a type
    fn increment_fault_count(&self, fault_type: FaultType) {
        let index = match fault_type {
            FaultType::Hardware => 0,
            FaultType::Software => 1,
            FaultType::Communication => 2,
            FaultType::Power => 3,
            FaultType::Watchdog => 4,
            FaultType::StackOverflow => 5,
            FaultType::Memory => 6,
            FaultType::Peripheral => 7,
            FaultType::Unknown => 8,
        };

        self.fault_counts[index].fetch_add(1, Ordering::Relaxed);
    }

    /// Get fault count for a type
    pub fn get_fault_count(&self, fault_type: FaultType) -> u32 {
        let index = match fault_type {
            FaultType::Hardware => 0,
            FaultType::Software => 1,
            FaultType::Communication => 2,
            FaultType::Power => 3,
            FaultType::Watchdog => 4,
            FaultType::StackOverflow => 5,
            FaultType::Memory => 6,
            FaultType::Peripheral => 7,
            FaultType::Unknown => 8,
        };

        self.fault_counts[index].load(Ordering::Relaxed)
    }

    /// Get total fault count
    pub fn total_fault_count(&self) -> u32 {
        self.fault_counts
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .sum()
    }

    /// Get current system state
    pub const fn state(&self) -> SystemState {
        self.state
    }

    /// Get fault log
    pub const fn log(&self) -> &FaultLog {
        &self.log
    }

    /// Get reboot count
    pub fn reboot_count(&self) -> u32 {
        self.reboot_count.load(Ordering::Acquire)
    }

    /// Reset reboot counter
    pub fn reset_reboot_counter(&self) {
        self.reboot_count.store(0, Ordering::Release);
    }

    /// Check if boot loop detected
    pub fn is_boot_loop(&self) -> bool {
        self.reboot_count() >= self.policy.max_reboots
    }

    /// Enter safe mode
    pub fn enter_safe_mode(&mut self) {
        self.state = SystemState::SafeMode;
    }

    /// Exit safe mode
    pub fn exit_safe_mode(&mut self) {
        if self.state == SystemState::SafeMode {
            self.state = SystemState::Normal;
        }
    }

    /// Generate crash dump
    pub fn generate_crash_dump(&self) -> heapless::Vec<u8, MAX_CRASH_DUMP_SIZE> {
        let mut dump = heapless::Vec::new();

        // In a real implementation, this would capture:
        // - Register state
        // - Stack trace
        // - Recent fault log entries
        // - System state
        // For simulation, we just add a marker
        let _ = dump.push(0xDE);
        let _ = dump.push(0xAD);
        let _ = dump.push(0xBE);
        let _ = dump.push(0xEF);

        // Add reboot count
        let reboot_count = self.reboot_count();
        let _ = dump.push((reboot_count & 0xFF) as u8);

        // Add fault counts
        for count in &self.fault_counts {
            let val = count.load(Ordering::Relaxed);
            let _ = dump.push((val & 0xFF) as u8);
        }

        dump
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_fault_source() {
        let source = FaultSource::new(1, 42, 100);
        assert_eq!(source.file_id, 1);
        assert_eq!(source.line, 42);
        assert_eq!(source.function_id, 100);

        let unknown = FaultSource::unknown();
        assert_eq!(unknown.file_id, 0);
        assert_eq!(unknown.line, 0);
    }

    #[test]
    fn test_fault_log_entry() {
        let source = FaultSource::new(1, 42, 100);
        let entry = FaultLogEntry::new(
            FaultType::Software,
            FaultSeverity::Error,
            1000,
            source,
            0x1234,
        );

        assert_eq!(entry.fault_type, FaultType::Software);
        assert_eq!(entry.severity, FaultSeverity::Error);
        assert_eq!(entry.error_code, 0x1234);

        let entry = entry.with_recovery(RecoveryAction::Reboot, true);
        assert_eq!(entry.recovery_action, RecoveryAction::Reboot);
        assert!(entry.recovery_successful);
    }

    #[test]
    fn test_fault_log() {
        let mut log = FaultLog::new();
        assert_eq!(log.count(), 0);

        let source = FaultSource::new(1, 42, 100);
        let entry = FaultLogEntry::new(
            FaultType::Hardware,
            FaultSeverity::Critical,
            1000,
            source,
            0x1234,
        );

        log.add(entry);
        assert_eq!(log.count(), 1);

        let recent = log.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].fault_type, FaultType::Hardware);
    }

    #[test]
    fn test_fault_log_filtering() {
        let mut log = FaultLog::new();

        for i in 0..10 {
            let fault_type = if i % 2 == 0 {
                FaultType::Software
            } else {
                FaultType::Hardware
            };

            let severity = if i < 5 {
                FaultSeverity::Warning
            } else {
                FaultSeverity::Critical
            };

            let entry =
                FaultLogEntry::new(fault_type, severity, i * 1000, FaultSource::unknown(), i);

            log.add(entry);
        }

        let software_faults = log.by_type(FaultType::Software);
        assert_eq!(software_faults.len(), 5);

        let critical_faults = log.by_severity(FaultSeverity::Critical);
        assert_eq!(critical_faults.len(), 5);
    }

    #[test]
    fn test_recovery_policy() {
        let conservative = RecoveryPolicy::conservative();
        assert_eq!(conservative.max_retries, 3);
        assert!(conservative.safe_mode_on_critical);

        let aggressive = RecoveryPolicy::aggressive();
        assert_eq!(aggressive.max_retries, 10);
        assert!(!aggressive.safe_mode_on_critical);

        let minimal = RecoveryPolicy::minimal();
        assert_eq!(minimal.max_retries, 1);
        assert!(!minimal.reboot_on_fatal);
    }

    #[test]
    fn test_fault_recovery_manager() {
        fn mock_time() -> u32 {
            0
        }

        let policy = RecoveryPolicy::conservative();
        let mut manager = FaultRecoveryManager::new(policy, mock_time);

        assert_eq!(manager.state(), SystemState::Normal);
        assert_eq!(manager.total_fault_count(), 0);

        manager.record_fault(
            FaultType::Software,
            FaultSeverity::Error,
            FaultSource::unknown(),
            0x1234,
        );

        assert_eq!(manager.total_fault_count(), 1);
        assert_eq!(manager.get_fault_count(FaultType::Software), 1);
    }

    #[test]
    fn test_fault_handling() {
        fn mock_time() -> u32 {
            0
        }

        let policy = RecoveryPolicy::conservative();
        let mut manager = FaultRecoveryManager::new(policy, mock_time);

        // Warning should not trigger recovery
        let action = manager.handle_fault(
            FaultType::Communication,
            FaultSeverity::Warning,
            FaultSource::unknown(),
            0,
        );
        assert_eq!(action, RecoveryAction::None);

        // Error should trigger retry for communication
        let action = manager.handle_fault(
            FaultType::Communication,
            FaultSeverity::Error,
            FaultSource::unknown(),
            0,
        );
        assert_eq!(action, RecoveryAction::Retry);

        // Critical should trigger safe mode (conservative policy)
        let action = manager.handle_fault(
            FaultType::Hardware,
            FaultSeverity::Critical,
            FaultSource::unknown(),
            0,
        );
        assert_eq!(action, RecoveryAction::SafeMode);
        assert_eq!(manager.state(), SystemState::SafeMode);
    }

    #[test]
    fn test_boot_loop_detection() {
        fn mock_time() -> u32 {
            0
        }

        let mut policy = RecoveryPolicy::conservative();
        policy.max_reboots = 3;
        let mut manager = FaultRecoveryManager::new(policy, mock_time);

        assert!(!manager.is_boot_loop());

        // Simulate multiple reboots
        for _ in 0..3 {
            manager.handle_fault(
                FaultType::Watchdog,
                FaultSeverity::Fatal,
                FaultSource::unknown(),
                0,
            );
        }

        assert!(manager.is_boot_loop());
    }

    #[test]
    fn test_crash_dump() {
        fn mock_time() -> u32 {
            0
        }

        let policy = RecoveryPolicy::conservative();
        let manager = FaultRecoveryManager::new(policy, mock_time);

        let dump = manager.generate_crash_dump();
        assert!(!dump.is_empty());
        assert_eq!(dump[0], 0xDE);
        assert_eq!(dump[1], 0xAD);
    }

    #[test]
    fn test_safe_mode() {
        fn mock_time() -> u32 {
            0
        }

        let policy = RecoveryPolicy::conservative();
        let mut manager = FaultRecoveryManager::new(policy, mock_time);

        assert_eq!(manager.state(), SystemState::Normal);

        manager.enter_safe_mode();
        assert_eq!(manager.state(), SystemState::SafeMode);

        manager.exit_safe_mode();
        assert_eq!(manager.state(), SystemState::Normal);
    }

    #[test]
    fn test_fault_error_display() {
        assert_eq!(format!("{}", FaultError::LogFull), "Fault log full");
        assert_eq!(format!("{}", FaultError::RecoveryFailed), "Recovery failed");
    }
}
