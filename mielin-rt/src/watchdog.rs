//! Watchdog Timer Module
//!
//! This module provides watchdog timer integration for embedded systems,
//! ensuring system reliability and automatic recovery from software faults.
//!
//! # Features
//!
//! - Independent watchdog (IWDG) support
//! - Window watchdog (WWDG) support
//! - Configurable timeout periods
//! - Automatic reset on timeout
//! - Freeze in debug mode option
//! - Multiple watchdog instances
//! - Task-level watchdog monitoring
//! - Hierarchical watchdog architecture
//!
//! # Watchdog Types
//!
//! ## Independent Watchdog (IWDG)
//! - Runs from independent clock (LSI)
//! - Cannot be stopped once started
//! - Continues running in sleep/stop modes
//! - Best for safety-critical applications
//!
//! ## Window Watchdog (WWDG)
//! - Runs from system clock
//! - Can be stopped in sleep mode
//! - Supports windowed refresh
//! - Early warning interrupt

#![allow(dead_code)]

use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Maximum number of watchdog instances
pub const MAX_WATCHDOG_INSTANCES: usize = 8;

/// Maximum number of monitored tasks
pub const MAX_MONITORED_TASKS: usize = 32;

/// Watchdog error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogError {
    /// Watchdog already running
    AlreadyRunning,
    /// Watchdog not running
    NotRunning,
    /// Invalid timeout
    InvalidTimeout,
    /// Invalid window
    InvalidWindow,
    /// Task not found
    TaskNotFound,
    /// Too many tasks
    TooManyTasks,
    /// Refresh too early (window violation)
    RefreshTooEarly,
    /// Refresh too late (timeout)
    RefreshTooLate,
    /// Not initialized
    NotInitialized,
}

impl fmt::Display for WatchdogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "Watchdog already running"),
            Self::NotRunning => write!(f, "Watchdog not running"),
            Self::InvalidTimeout => write!(f, "Invalid timeout"),
            Self::InvalidWindow => write!(f, "Invalid window"),
            Self::TaskNotFound => write!(f, "Task not found"),
            Self::TooManyTasks => write!(f, "Too many tasks"),
            Self::RefreshTooEarly => write!(f, "Refresh too early (window violation)"),
            Self::RefreshTooLate => write!(f, "Refresh too late (timeout)"),
            Self::NotInitialized => write!(f, "Not initialized"),
        }
    }
}

/// Watchdog type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogType {
    /// Independent watchdog (IWDG)
    Independent,
    /// Window watchdog (WWDG)
    Window,
    /// Software watchdog (task monitoring)
    Software,
}

/// Watchdog configuration
#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// Watchdog type
    pub watchdog_type: WatchdogType,
    /// Timeout in milliseconds
    pub timeout_ms: u32,
    /// Window start time in milliseconds (for WWDG)
    pub window_start_ms: Option<u32>,
    /// Freeze in debug mode
    pub freeze_in_debug: bool,
    /// Enable early warning interrupt (for WWDG)
    pub early_warning: bool,
}

impl WatchdogConfig {
    /// Create a new configuration for independent watchdog
    pub const fn independent(timeout_ms: u32) -> Self {
        Self {
            watchdog_type: WatchdogType::Independent,
            timeout_ms,
            window_start_ms: None,
            freeze_in_debug: true,
            early_warning: false,
        }
    }

    /// Create a new configuration for window watchdog
    pub const fn window(timeout_ms: u32, window_start_ms: u32) -> Self {
        Self {
            watchdog_type: WatchdogType::Window,
            timeout_ms,
            window_start_ms: Some(window_start_ms),
            freeze_in_debug: true,
            early_warning: true,
        }
    }

    /// Create a new configuration for software watchdog
    pub const fn software(timeout_ms: u32) -> Self {
        Self {
            watchdog_type: WatchdogType::Software,
            timeout_ms,
            window_start_ms: None,
            freeze_in_debug: false,
            early_warning: false,
        }
    }

    /// Validate configuration
    pub const fn validate(&self) -> Result<(), WatchdogError> {
        if self.timeout_ms == 0 {
            return Err(WatchdogError::InvalidTimeout);
        }

        if let Some(window_start) = self.window_start_ms {
            if window_start >= self.timeout_ms {
                return Err(WatchdogError::InvalidWindow);
            }
        }

        Ok(())
    }
}

/// Watchdog statistics
#[derive(Debug, Clone, Copy)]
pub struct WatchdogStats {
    /// Number of refreshes
    pub refresh_count: u32,
    /// Number of timeouts (resets)
    pub timeout_count: u32,
    /// Number of early warnings
    pub warning_count: u32,
    /// Last refresh timestamp
    pub last_refresh_ms: u32,
    /// Minimum time between refreshes
    pub min_refresh_interval_ms: u32,
    /// Maximum time between refreshes
    pub max_refresh_interval_ms: u32,
}

impl Default for WatchdogStats {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchdogStats {
    /// Create new statistics
    pub const fn new() -> Self {
        Self {
            refresh_count: 0,
            timeout_count: 0,
            warning_count: 0,
            last_refresh_ms: 0,
            min_refresh_interval_ms: u32::MAX,
            max_refresh_interval_ms: 0,
        }
    }

    /// Update on refresh
    pub fn on_refresh(&mut self, current_time_ms: u32) {
        if self.last_refresh_ms > 0 {
            let interval = current_time_ms.saturating_sub(self.last_refresh_ms);
            self.min_refresh_interval_ms = self.min_refresh_interval_ms.min(interval);
            self.max_refresh_interval_ms = self.max_refresh_interval_ms.max(interval);
        }

        self.refresh_count += 1;
        self.last_refresh_ms = current_time_ms;
    }

    /// Update on timeout
    pub fn on_timeout(&mut self) {
        self.timeout_count += 1;
    }

    /// Update on warning
    pub fn on_warning(&mut self) {
        self.warning_count += 1;
    }
}

/// Watchdog timer
pub struct Watchdog {
    /// Configuration
    config: WatchdogConfig,
    /// Running state
    running: AtomicBool,
    /// Last refresh time (milliseconds)
    last_refresh: AtomicU32,
    /// Statistics
    stats: WatchdogStats,
    /// Initialized
    initialized: bool,
}

impl Watchdog {
    /// Create a new watchdog
    pub fn new(config: WatchdogConfig) -> Result<Self, WatchdogError> {
        config.validate()?;

        Ok(Self {
            config,
            running: AtomicBool::new(false),
            last_refresh: AtomicU32::new(0),
            stats: WatchdogStats::new(),
            initialized: true,
        })
    }

    /// Get configuration
    pub const fn config(&self) -> &WatchdogConfig {
        &self.config
    }

    /// Start the watchdog
    pub fn start(&self) -> Result<(), WatchdogError> {
        if !self.initialized {
            return Err(WatchdogError::NotInitialized);
        }

        if self.running.load(Ordering::Acquire) {
            return Err(WatchdogError::AlreadyRunning);
        }

        // In a real implementation, this would configure and start the hardware watchdog
        self.running.store(true, Ordering::Release);

        Ok(())
    }

    /// Check if watchdog is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Refresh (feed) the watchdog
    pub fn refresh(&mut self, current_time_ms: u32) -> Result<(), WatchdogError> {
        if !self.is_running() {
            return Err(WatchdogError::NotRunning);
        }

        // Check window constraint for WWDG (skip check on first refresh)
        if let Some(window_start) = self.config.window_start_ms {
            let last_refresh = self.last_refresh.load(Ordering::Acquire);

            // Skip window check if this is the first refresh
            if self.stats.refresh_count > 0 {
                let elapsed = current_time_ms.saturating_sub(last_refresh);

                if elapsed < window_start {
                    return Err(WatchdogError::RefreshTooEarly);
                }

                if elapsed >= self.config.timeout_ms {
                    self.stats.on_timeout();
                    return Err(WatchdogError::RefreshTooLate);
                }
            }
        }

        // In a real implementation, this would refresh the hardware watchdog
        self.last_refresh.store(current_time_ms, Ordering::Release);
        self.stats.on_refresh(current_time_ms);

        Ok(())
    }

    /// Check if timeout occurred
    pub fn check_timeout(&mut self, current_time_ms: u32) -> bool {
        if !self.is_running() {
            return false;
        }

        let last_refresh = self.last_refresh.load(Ordering::Acquire);
        let elapsed = current_time_ms.saturating_sub(last_refresh);

        if elapsed >= self.config.timeout_ms {
            self.stats.on_timeout();
            true
        } else {
            false
        }
    }

    /// Get statistics
    pub const fn stats(&self) -> &WatchdogStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = WatchdogStats::new();
    }
}

/// Task watchdog entry
#[derive(Debug, Clone, Copy)]
struct TaskWatchdogEntry {
    /// Task ID
    task_id: u32,
    /// Timeout in milliseconds
    timeout_ms: u32,
    /// Last checkin time
    last_checkin_ms: u32,
    /// Active
    active: bool,
}

/// Task watchdog manager
pub struct TaskWatchdog {
    /// Monitored tasks
    tasks: [Option<TaskWatchdogEntry>; MAX_MONITORED_TASKS],
    /// Number of active tasks
    task_count: usize,
    /// Global timeout for all tasks
    global_timeout_ms: u32,
    /// Timeout counter
    timeout_count: u32,
}

impl TaskWatchdog {
    /// Create a new task watchdog manager
    pub const fn new(global_timeout_ms: u32) -> Self {
        Self {
            tasks: [None; MAX_MONITORED_TASKS],
            task_count: 0,
            global_timeout_ms,
            timeout_count: 0,
        }
    }

    /// Register a task
    pub fn register_task(&mut self, task_id: u32, timeout_ms: u32) -> Result<(), WatchdogError> {
        // Check if task already registered
        if self
            .tasks
            .iter()
            .any(|t| t.as_ref().is_some_and(|e| e.task_id == task_id))
        {
            return Ok(());
        }

        // Find empty slot
        let slot = self
            .tasks
            .iter_mut()
            .find(|t| t.is_none())
            .ok_or(WatchdogError::TooManyTasks)?;

        *slot = Some(TaskWatchdogEntry {
            task_id,
            timeout_ms,
            last_checkin_ms: 0,
            active: true,
        });

        self.task_count += 1;
        Ok(())
    }

    /// Unregister a task
    pub fn unregister_task(&mut self, task_id: u32) -> Result<(), WatchdogError> {
        let slot = self
            .tasks
            .iter_mut()
            .find(|t| t.as_ref().is_some_and(|e| e.task_id == task_id))
            .ok_or(WatchdogError::TaskNotFound)?;

        *slot = None;
        self.task_count = self.task_count.saturating_sub(1);
        Ok(())
    }

    /// Task check-in
    pub fn checkin(&mut self, task_id: u32, current_time_ms: u32) -> Result<(), WatchdogError> {
        let entry = self
            .tasks
            .iter_mut()
            .find_map(|t| t.as_mut().filter(|e| e.task_id == task_id))
            .ok_or(WatchdogError::TaskNotFound)?;

        entry.last_checkin_ms = current_time_ms;
        Ok(())
    }

    /// Check all tasks for timeout
    pub fn check_timeouts(
        &mut self,
        current_time_ms: u32,
    ) -> heapless::Vec<u32, MAX_MONITORED_TASKS> {
        let mut timed_out = heapless::Vec::new();

        for entry in self.tasks.iter().filter_map(|t| t.as_ref()) {
            if !entry.active {
                continue;
            }

            let elapsed = current_time_ms.saturating_sub(entry.last_checkin_ms);
            let timeout = if entry.timeout_ms > 0 {
                entry.timeout_ms
            } else {
                self.global_timeout_ms
            };

            if elapsed >= timeout {
                let _ = timed_out.push(entry.task_id);
                self.timeout_count += 1;
            }
        }

        timed_out
    }

    /// Get number of registered tasks
    pub const fn task_count(&self) -> usize {
        self.task_count
    }

    /// Get timeout count
    pub const fn timeout_count(&self) -> u32 {
        self.timeout_count
    }

    /// Reset timeout counter
    pub fn reset_timeout_counter(&mut self) {
        self.timeout_count = 0;
    }
}

/// Watchdog manager for multiple watchdog instances
pub struct WatchdogManager {
    /// Watchdog instances
    watchdogs: heapless::Vec<Watchdog, MAX_WATCHDOG_INSTANCES>,
    /// Task watchdog
    task_watchdog: Option<TaskWatchdog>,
    /// System time getter
    system_time_ms: fn() -> u32,
}

impl WatchdogManager {
    /// Create a new watchdog manager
    pub const fn new(system_time_ms: fn() -> u32) -> Self {
        Self {
            watchdogs: heapless::Vec::new(),
            task_watchdog: None,
            system_time_ms,
        }
    }

    /// Add a watchdog instance
    pub fn add_watchdog(&mut self, config: WatchdogConfig) -> Result<usize, WatchdogError> {
        let watchdog = Watchdog::new(config)?;
        self.watchdogs
            .push(watchdog)
            .map_err(|_| WatchdogError::TooManyTasks)?;
        Ok(self.watchdogs.len() - 1)
    }

    /// Enable task watchdog
    pub fn enable_task_watchdog(&mut self, global_timeout_ms: u32) {
        self.task_watchdog = Some(TaskWatchdog::new(global_timeout_ms));
    }

    /// Get task watchdog
    pub fn task_watchdog(&mut self) -> Option<&mut TaskWatchdog> {
        self.task_watchdog.as_mut()
    }

    /// Start all watchdogs
    pub fn start_all(&self) -> Result<(), WatchdogError> {
        for watchdog in self.watchdogs.iter() {
            watchdog.start()?;
        }
        Ok(())
    }

    /// Refresh all watchdogs
    pub fn refresh_all(&mut self) -> Result<(), WatchdogError> {
        let current_time = (self.system_time_ms)();

        for watchdog in self.watchdogs.iter_mut() {
            watchdog.refresh(current_time)?;
        }

        Ok(())
    }

    /// Check all watchdogs for timeout
    pub fn check_all_timeouts(&mut self) -> bool {
        let current_time = (self.system_time_ms)();
        let mut any_timeout = false;

        for watchdog in self.watchdogs.iter_mut() {
            if watchdog.check_timeout(current_time) {
                any_timeout = true;
            }
        }

        any_timeout
    }

    /// Get number of watchdogs
    pub fn watchdog_count(&self) -> usize {
        self.watchdogs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn test_watchdog_config_validation() {
        let config = WatchdogConfig::independent(1000);
        assert!(config.validate().is_ok());

        let config = WatchdogConfig::independent(0);
        assert_eq!(config.validate(), Err(WatchdogError::InvalidTimeout));

        let config = WatchdogConfig::window(1000, 500);
        assert!(config.validate().is_ok());

        let config = WatchdogConfig::window(1000, 1000);
        assert_eq!(config.validate(), Err(WatchdogError::InvalidWindow));
    }

    #[test]
    fn test_watchdog_creation() {
        let config = WatchdogConfig::independent(1000);
        let watchdog = Watchdog::new(config);
        assert!(watchdog.is_ok());

        let watchdog = watchdog.unwrap();
        assert!(!watchdog.is_running());
        assert_eq!(watchdog.config().timeout_ms, 1000);
    }

    #[test]
    fn test_watchdog_start() {
        let config = WatchdogConfig::independent(1000);
        let watchdog = Watchdog::new(config).unwrap();

        assert!(watchdog.start().is_ok());
        assert!(watchdog.is_running());

        // Cannot start twice
        assert_eq!(watchdog.start(), Err(WatchdogError::AlreadyRunning));
    }

    #[test]
    fn test_watchdog_refresh() {
        let config = WatchdogConfig::independent(1000);
        let mut watchdog = Watchdog::new(config).unwrap();

        // Cannot refresh when not running
        assert_eq!(watchdog.refresh(0), Err(WatchdogError::NotRunning));

        watchdog.start().unwrap();

        // Can refresh when running
        assert!(watchdog.refresh(100).is_ok());
        assert_eq!(watchdog.stats().refresh_count, 1);

        assert!(watchdog.refresh(200).is_ok());
        assert_eq!(watchdog.stats().refresh_count, 2);
    }

    #[test]
    fn test_watchdog_timeout() {
        let config = WatchdogConfig::independent(1000);
        let mut watchdog = Watchdog::new(config).unwrap();
        watchdog.start().unwrap();

        watchdog.refresh(0).unwrap();
        assert!(!watchdog.check_timeout(500));
        assert!(watchdog.check_timeout(1000));
        assert_eq!(watchdog.stats().timeout_count, 1);
    }

    #[test]
    fn test_window_watchdog() {
        let config = WatchdogConfig::window(1000, 500);
        let mut watchdog = Watchdog::new(config).unwrap();
        watchdog.start().unwrap();

        // First refresh at start time
        watchdog.refresh(0).unwrap();

        // Too early - within window start time
        assert_eq!(watchdog.refresh(300), Err(WatchdogError::RefreshTooEarly));

        // Within window (500-1000ms after last refresh)
        assert!(watchdog.refresh(600).is_ok());

        // Too early again after second refresh
        assert_eq!(watchdog.refresh(900), Err(WatchdogError::RefreshTooEarly));

        // Within window again
        assert!(watchdog.refresh(1200).is_ok());
    }

    #[test]
    fn test_task_watchdog() {
        let mut task_wdog = TaskWatchdog::new(1000);

        // Register tasks
        assert!(task_wdog.register_task(1, 500).is_ok());
        assert!(task_wdog.register_task(2, 1000).is_ok());
        assert_eq!(task_wdog.task_count(), 2);

        // Check-in
        assert!(task_wdog.checkin(1, 100).is_ok());
        assert!(task_wdog.checkin(2, 100).is_ok());

        // No timeout yet
        let timed_out = task_wdog.check_timeouts(400);
        assert_eq!(timed_out.len(), 0);

        // Task 1 should timeout
        let timed_out = task_wdog.check_timeouts(700);
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], 1);

        // Unregister task
        assert!(task_wdog.unregister_task(1).is_ok());
        assert_eq!(task_wdog.task_count(), 1);
    }

    #[test]
    fn test_watchdog_manager() {
        fn mock_time() -> u32 {
            0
        }

        let mut manager = WatchdogManager::new(mock_time);

        // Add watchdogs
        let config1 = WatchdogConfig::independent(1000);
        let config2 = WatchdogConfig::independent(2000);

        assert!(manager.add_watchdog(config1).is_ok());
        assert!(manager.add_watchdog(config2).is_ok());
        assert_eq!(manager.watchdog_count(), 2);

        // Start all
        assert!(manager.start_all().is_ok());

        // Enable task watchdog
        manager.enable_task_watchdog(3000);
        assert!(manager.task_watchdog().is_some());
    }

    #[test]
    fn test_watchdog_stats() {
        let mut stats = WatchdogStats::new();
        assert_eq!(stats.refresh_count, 0);

        stats.on_refresh(100);
        assert_eq!(stats.refresh_count, 1);
        assert_eq!(stats.last_refresh_ms, 100);

        stats.on_refresh(200);
        assert_eq!(stats.refresh_count, 2);
        assert_eq!(stats.min_refresh_interval_ms, 100);
        assert_eq!(stats.max_refresh_interval_ms, 100);

        stats.on_timeout();
        assert_eq!(stats.timeout_count, 1);

        stats.on_warning();
        assert_eq!(stats.warning_count, 1);
    }

    #[test]
    fn test_watchdog_error_display() {
        assert_eq!(
            format!("{}", WatchdogError::AlreadyRunning),
            "Watchdog already running"
        );
        assert_eq!(
            format!("{}", WatchdogError::RefreshTooEarly),
            "Refresh too early (window violation)"
        );
    }
}
