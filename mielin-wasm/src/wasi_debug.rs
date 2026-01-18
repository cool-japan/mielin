//! WASI Debugging Tools Integration
//!
//! This module provides debugging capabilities specifically for WASI modules,
//! including system call tracing, file descriptor monitoring, and WASI-specific
//! debugging utilities.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::wasi::Errno;

/// WASI system call types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasiSyscall {
    /// File descriptor read
    FdRead,
    /// File descriptor write
    FdWrite,
    /// File descriptor close
    FdClose,
    /// File descriptor seek
    FdSeek,
    /// Clock time get
    ClockTimeGet,
    /// Environment sizes get
    EnvironSizesGet,
    /// Environment get
    EnvironGet,
    /// Arguments sizes get
    ArgsSizesGet,
    /// Arguments get
    ArgsGet,
    /// Process exit
    ProcExit,
    /// Path open
    PathOpen,
    /// File descriptor prestat get
    FdPrestatGet,
    /// File descriptor prestat dir name
    FdPrestatDirName,
}

impl WasiSyscall {
    /// Get syscall name as string
    pub fn name(&self) -> &'static str {
        match self {
            Self::FdRead => "fd_read",
            Self::FdWrite => "fd_write",
            Self::FdClose => "fd_close",
            Self::FdSeek => "fd_seek",
            Self::ClockTimeGet => "clock_time_get",
            Self::EnvironSizesGet => "environ_sizes_get",
            Self::EnvironGet => "environ_get",
            Self::ArgsSizesGet => "args_sizes_get",
            Self::ArgsGet => "args_get",
            Self::ProcExit => "proc_exit",
            Self::PathOpen => "path_open",
            Self::FdPrestatGet => "fd_prestat_get",
            Self::FdPrestatDirName => "fd_prestat_dir_name",
        }
    }

    /// Get all syscalls
    pub fn all() -> Vec<Self> {
        vec![
            Self::FdRead,
            Self::FdWrite,
            Self::FdClose,
            Self::FdSeek,
            Self::ClockTimeGet,
            Self::EnvironSizesGet,
            Self::EnvironGet,
            Self::ArgsSizesGet,
            Self::ArgsGet,
            Self::ProcExit,
            Self::PathOpen,
            Self::FdPrestatGet,
            Self::FdPrestatDirName,
        ]
    }
}

/// WASI system call trace entry
#[derive(Debug, Clone)]
pub struct WasiTraceEntry {
    /// Syscall type
    pub syscall: WasiSyscall,
    /// Timestamp when the call started
    pub timestamp: Instant,
    /// Duration of the call
    pub duration: Duration,
    /// Arguments (formatted as strings)
    pub args: Vec<String>,
    /// Return value (errno)
    pub result: Errno,
    /// File descriptor (if applicable)
    pub fd: Option<i32>,
    /// Number of bytes transferred (if applicable)
    pub bytes: Option<usize>,
}

impl WasiTraceEntry {
    /// Create a new trace entry
    pub fn new(syscall: WasiSyscall) -> Self {
        Self {
            syscall,
            timestamp: Instant::now(),
            duration: Duration::ZERO,
            args: Vec::new(),
            result: Errno::Success,
            fd: None,
            bytes: None,
        }
    }

    /// Set duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Add argument
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set result
    pub fn with_result(mut self, result: Errno) -> Self {
        self.result = result;
        self
    }

    /// Set file descriptor
    pub fn with_fd(mut self, fd: i32) -> Self {
        self.fd = Some(fd);
        self
    }

    /// Set bytes transferred
    pub fn with_bytes(mut self, bytes: usize) -> Self {
        self.bytes = Some(bytes);
        self
    }

    /// Format trace entry as string
    pub fn format(&self) -> String {
        let mut parts = vec![
            format!("[{:?}]", self.timestamp.elapsed()),
            format!("{}", self.syscall.name()),
        ];

        if let Some(fd) = self.fd {
            parts.push(format!("fd={}", fd));
        }

        if !self.args.is_empty() {
            parts.push(format!("args=[{}]", self.args.join(", ")));
        }

        if let Some(bytes) = self.bytes {
            parts.push(format!("bytes={}", bytes));
        }

        parts.push(format!("result={:?}", self.result));
        parts.push(format!("duration={:?}", self.duration));

        parts.join(" ")
    }
}

/// WASI syscall statistics
#[derive(Debug, Clone, Default)]
pub struct WasiSyscallStats {
    /// Number of calls
    pub call_count: u64,
    /// Total duration
    pub total_duration: Duration,
    /// Average duration
    pub avg_duration: Duration,
    /// Min duration
    pub min_duration: Option<Duration>,
    /// Max duration
    pub max_duration: Option<Duration>,
    /// Error count
    pub error_count: u64,
}

impl WasiSyscallStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Update stats with a new trace entry
    pub fn update(&mut self, entry: &WasiTraceEntry) {
        self.call_count += 1;
        self.total_duration += entry.duration;
        self.avg_duration = self.total_duration / self.call_count as u32;

        if let Some(min) = self.min_duration {
            self.min_duration = Some(min.min(entry.duration));
        } else {
            self.min_duration = Some(entry.duration);
        }

        if let Some(max) = self.max_duration {
            self.max_duration = Some(max.max(entry.duration));
        } else {
            self.max_duration = Some(entry.duration);
        }

        if entry.result != Errno::Success {
            self.error_count += 1;
        }
    }
}

/// File descriptor monitor for tracking FD usage
#[derive(Debug, Clone)]
pub struct FdMonitor {
    /// File descriptor number
    pub fd: i32,
    /// Number of reads
    pub read_count: u64,
    /// Number of writes
    pub write_count: u64,
    /// Total bytes read
    pub bytes_read: u64,
    /// Total bytes written
    pub bytes_written: u64,
    /// Last access time
    pub last_access: Option<Instant>,
    /// FD description (if known)
    pub description: String,
}

impl FdMonitor {
    /// Create a new FD monitor
    pub fn new(fd: i32) -> Self {
        Self {
            fd,
            read_count: 0,
            write_count: 0,
            bytes_read: 0,
            bytes_written: 0,
            last_access: None,
            description: format!("fd:{}", fd),
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Record a read operation
    pub fn record_read(&mut self, bytes: usize) {
        self.read_count += 1;
        self.bytes_read += bytes as u64;
        self.last_access = Some(Instant::now());
    }

    /// Record a write operation
    pub fn record_write(&mut self, bytes: usize) {
        self.write_count += 1;
        self.bytes_written += bytes as u64;
        self.last_access = Some(Instant::now());
    }

    /// Get total operations
    pub fn total_ops(&self) -> u64 {
        self.read_count + self.write_count
    }

    /// Get total bytes transferred
    pub fn total_bytes(&self) -> u64 {
        self.bytes_read + self.bytes_written
    }
}

/// WASI debugger for tracing and monitoring WASI calls
pub struct WasiDebugger {
    /// Trace entries
    traces: Arc<RwLock<Vec<WasiTraceEntry>>>,
    /// Syscall statistics
    stats: Arc<RwLock<HashMap<WasiSyscall, WasiSyscallStats>>>,
    /// File descriptor monitors
    fd_monitors: Arc<RwLock<HashMap<i32, FdMonitor>>>,
    /// Whether tracing is enabled
    enabled: Arc<RwLock<bool>>,
    /// Maximum trace entries to keep
    max_traces: usize,
}

impl WasiDebugger {
    /// Create a new WASI debugger
    pub fn new() -> Self {
        Self {
            traces: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(HashMap::new())),
            fd_monitors: Arc::new(RwLock::new(HashMap::new())),
            enabled: Arc::new(RwLock::new(true)),
            max_traces: 10000,
        }
    }

    /// Create with custom max traces
    pub fn with_max_traces(max_traces: usize) -> Self {
        Self {
            max_traces,
            ..Self::new()
        }
    }

    /// Enable or disable tracing
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        let mut en = self
            .enabled
            .write()
            .map_err(|e| anyhow!("Failed to acquire enabled write lock: {}", e))?;
        *en = enabled;
        Ok(())
    }

    /// Check if tracing is enabled
    pub fn is_enabled(&self) -> Result<bool> {
        let en = self
            .enabled
            .read()
            .map_err(|e| anyhow!("Failed to acquire enabled read lock: {}", e))?;
        Ok(*en)
    }

    /// Add a trace entry
    pub fn trace(&self, entry: WasiTraceEntry) -> Result<()> {
        if !self.is_enabled()? {
            return Ok(());
        }

        // Update stats
        {
            let mut stats = self
                .stats
                .write()
                .map_err(|e| anyhow!("Failed to acquire stats write lock: {}", e))?;
            stats
                .entry(entry.syscall)
                .or_insert_with(WasiSyscallStats::new)
                .update(&entry);
        }

        // Update FD monitor if applicable
        if let Some(fd) = entry.fd {
            let mut monitors = self
                .fd_monitors
                .write()
                .map_err(|e| anyhow!("Failed to acquire fd_monitors write lock: {}", e))?;
            let monitor = monitors.entry(fd).or_insert_with(|| FdMonitor::new(fd));

            if let Some(bytes) = entry.bytes {
                match entry.syscall {
                    WasiSyscall::FdRead => monitor.record_read(bytes),
                    WasiSyscall::FdWrite => monitor.record_write(bytes),
                    _ => {}
                }
            }
        }

        // Add trace entry
        {
            let mut traces = self
                .traces
                .write()
                .map_err(|e| anyhow!("Failed to acquire traces write lock: {}", e))?;

            // Limit trace size
            if traces.len() >= self.max_traces {
                traces.remove(0);
            }

            traces.push(entry);
        }

        Ok(())
    }

    /// Get all traces
    pub fn get_traces(&self) -> Result<Vec<WasiTraceEntry>> {
        let traces = self
            .traces
            .read()
            .map_err(|e| anyhow!("Failed to acquire traces read lock: {}", e))?;
        Ok(traces.clone())
    }

    /// Get traces for a specific syscall
    pub fn get_traces_for(&self, syscall: WasiSyscall) -> Result<Vec<WasiTraceEntry>> {
        let traces = self.get_traces()?;
        Ok(traces
            .into_iter()
            .filter(|t| t.syscall == syscall)
            .collect())
    }

    /// Get statistics for a syscall
    pub fn get_stats(&self, syscall: WasiSyscall) -> Result<Option<WasiSyscallStats>> {
        let stats = self
            .stats
            .read()
            .map_err(|e| anyhow!("Failed to acquire stats read lock: {}", e))?;
        Ok(stats.get(&syscall).cloned())
    }

    /// Get all statistics
    pub fn get_all_stats(&self) -> Result<HashMap<WasiSyscall, WasiSyscallStats>> {
        let stats = self
            .stats
            .read()
            .map_err(|e| anyhow!("Failed to acquire stats read lock: {}", e))?;
        Ok(stats.clone())
    }

    /// Get FD monitor
    pub fn get_fd_monitor(&self, fd: i32) -> Result<Option<FdMonitor>> {
        let monitors = self
            .fd_monitors
            .read()
            .map_err(|e| anyhow!("Failed to acquire fd_monitors read lock: {}", e))?;
        Ok(monitors.get(&fd).cloned())
    }

    /// Get all FD monitors
    pub fn get_all_fd_monitors(&self) -> Result<Vec<FdMonitor>> {
        let monitors = self
            .fd_monitors
            .read()
            .map_err(|e| anyhow!("Failed to acquire fd_monitors read lock: {}", e))?;
        Ok(monitors.values().cloned().collect())
    }

    /// Clear all traces
    pub fn clear_traces(&self) -> Result<()> {
        let mut traces = self
            .traces
            .write()
            .map_err(|e| anyhow!("Failed to acquire traces write lock: {}", e))?;
        traces.clear();
        Ok(())
    }

    /// Clear all statistics
    pub fn clear_stats(&self) -> Result<()> {
        let mut stats = self
            .stats
            .write()
            .map_err(|e| anyhow!("Failed to acquire stats write lock: {}", e))?;
        stats.clear();
        Ok(())
    }

    /// Clear all FD monitors
    pub fn clear_fd_monitors(&self) -> Result<()> {
        let mut monitors = self
            .fd_monitors
            .write()
            .map_err(|e| anyhow!("Failed to acquire fd_monitors write lock: {}", e))?;
        monitors.clear();
        Ok(())
    }

    /// Reset all debugging data
    pub fn reset(&self) -> Result<()> {
        self.clear_traces()?;
        self.clear_stats()?;
        self.clear_fd_monitors()?;
        Ok(())
    }

    /// Get total trace count
    pub fn trace_count(&self) -> Result<usize> {
        let traces = self
            .traces
            .read()
            .map_err(|e| anyhow!("Failed to acquire traces read lock: {}", e))?;
        Ok(traces.len())
    }

    /// Print traces to string
    pub fn format_traces(&self) -> Result<String> {
        let traces = self.get_traces()?;
        let mut output = String::new();

        for trace in traces {
            output.push_str(&trace.format());
            output.push('\n');
        }

        Ok(output)
    }

    /// Print statistics summary
    pub fn format_stats(&self) -> Result<String> {
        let stats = self.get_all_stats()?;
        let mut output = String::from("WASI Syscall Statistics:\n");
        output.push_str("========================\n\n");

        for syscall in WasiSyscall::all() {
            if let Some(stat) = stats.get(&syscall) {
                output.push_str(&format!("{}:\n", syscall.name()));
                output.push_str(&format!("  Calls: {}\n", stat.call_count));
                output.push_str(&format!("  Errors: {}\n", stat.error_count));
                output.push_str(&format!("  Total Duration: {:?}\n", stat.total_duration));
                output.push_str(&format!("  Avg Duration: {:?}\n", stat.avg_duration));
                if let Some(min) = stat.min_duration {
                    output.push_str(&format!("  Min Duration: {:?}\n", min));
                }
                if let Some(max) = stat.max_duration {
                    output.push_str(&format!("  Max Duration: {:?}\n", max));
                }
                output.push('\n');
            }
        }

        Ok(output)
    }

    /// Print FD monitor summary
    pub fn format_fd_monitors(&self) -> Result<String> {
        let monitors = self.get_all_fd_monitors()?;
        let mut output = String::from("File Descriptor Monitors:\n");
        output.push_str("========================\n\n");

        for monitor in monitors {
            output.push_str(&format!("{} ({}):\n", monitor.description, monitor.fd));
            output.push_str(&format!(
                "  Reads: {} ({} bytes)\n",
                monitor.read_count, monitor.bytes_read
            ));
            output.push_str(&format!(
                "  Writes: {} ({} bytes)\n",
                monitor.write_count, monitor.bytes_written
            ));
            output.push_str(&format!("  Total Ops: {}\n", monitor.total_ops()));
            output.push_str(&format!("  Total Bytes: {}\n", monitor.total_bytes()));
            if let Some(last) = monitor.last_access {
                output.push_str(&format!("  Last Access: {:?} ago\n", last.elapsed()));
            }
            output.push('\n');
        }

        Ok(output)
    }
}

impl Default for WasiDebugger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasi_syscall_names() {
        assert_eq!(WasiSyscall::FdRead.name(), "fd_read");
        assert_eq!(WasiSyscall::FdWrite.name(), "fd_write");
        assert_eq!(WasiSyscall::ClockTimeGet.name(), "clock_time_get");
    }

    #[test]
    fn test_wasi_syscall_all() {
        let syscalls = WasiSyscall::all();
        assert!(syscalls.len() >= 10);
        assert!(syscalls.contains(&WasiSyscall::FdRead));
        assert!(syscalls.contains(&WasiSyscall::FdWrite));
    }

    #[test]
    fn test_trace_entry_creation() {
        let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_fd(0)
            .with_bytes(100)
            .with_result(Errno::Success);

        assert_eq!(entry.syscall, WasiSyscall::FdRead);
        assert_eq!(entry.fd, Some(0));
        assert_eq!(entry.bytes, Some(100));
        assert_eq!(entry.result, Errno::Success);
    }

    #[test]
    fn test_trace_entry_format() {
        let entry = WasiTraceEntry::new(WasiSyscall::FdWrite)
            .with_fd(1)
            .with_bytes(50);

        let formatted = entry.format();
        assert!(formatted.contains("fd_write"));
        assert!(formatted.contains("fd=1"));
        assert!(formatted.contains("bytes=50"));
    }

    #[test]
    fn test_syscall_stats_update() {
        let mut stats = WasiSyscallStats::new();
        let entry =
            WasiTraceEntry::new(WasiSyscall::FdRead).with_duration(Duration::from_millis(10));

        stats.update(&entry);

        assert_eq!(stats.call_count, 1);
        assert_eq!(stats.error_count, 0);
        assert!(stats.min_duration.is_some());
        assert!(stats.max_duration.is_some());
    }

    #[test]
    fn test_fd_monitor_creation() {
        let monitor = FdMonitor::new(0).with_description("stdin");

        assert_eq!(monitor.fd, 0);
        assert_eq!(monitor.description, "stdin");
        assert_eq!(monitor.read_count, 0);
        assert_eq!(monitor.write_count, 0);
    }

    #[test]
    fn test_fd_monitor_operations() {
        let mut monitor = FdMonitor::new(1);

        monitor.record_write(100);
        monitor.record_write(50);
        monitor.record_read(25);

        assert_eq!(monitor.write_count, 2);
        assert_eq!(monitor.read_count, 1);
        assert_eq!(monitor.bytes_written, 150);
        assert_eq!(monitor.bytes_read, 25);
        assert_eq!(monitor.total_ops(), 3);
        assert_eq!(monitor.total_bytes(), 175);
    }

    #[test]
    fn test_wasi_debugger_creation() {
        let debugger = WasiDebugger::new();
        assert!(debugger.is_enabled().expect("Failed to check enabled"));
        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            0
        );
    }

    #[test]
    fn test_wasi_debugger_enable_disable() {
        let debugger = WasiDebugger::new();

        debugger.set_enabled(false).expect("Failed to disable");
        assert!(!debugger.is_enabled().expect("Failed to check enabled"));

        debugger.set_enabled(true).expect("Failed to enable");
        assert!(debugger.is_enabled().expect("Failed to check enabled"));
    }

    #[test]
    fn test_wasi_debugger_trace() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_fd(0)
            .with_bytes(100);

        debugger.trace(entry).expect("Failed to trace");

        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            1
        );
    }

    #[test]
    fn test_wasi_debugger_stats() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdWrite)
            .with_fd(1)
            .with_bytes(50);

        debugger.trace(entry).expect("Failed to trace");

        let stats = debugger
            .get_stats(WasiSyscall::FdWrite)
            .expect("Failed to get stats");
        assert!(stats.is_some());

        let stats = stats.expect("Stats should exist");
        assert_eq!(stats.call_count, 1);
    }

    #[test]
    fn test_wasi_debugger_fd_monitor() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_fd(0)
            .with_bytes(100);

        debugger.trace(entry).expect("Failed to trace");

        let monitor = debugger.get_fd_monitor(0).expect("Failed to get monitor");
        assert!(monitor.is_some());

        let monitor = monitor.expect("Monitor should exist");
        assert_eq!(monitor.bytes_read, 100);
    }

    #[test]
    fn test_wasi_debugger_clear() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdRead);
        debugger.trace(entry).expect("Failed to trace");

        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            1
        );

        debugger.clear_traces().expect("Failed to clear traces");
        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            0
        );
    }

    #[test]
    fn test_wasi_debugger_reset() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdWrite)
            .with_fd(1)
            .with_bytes(50);

        debugger.trace(entry).expect("Failed to trace");

        debugger.reset().expect("Failed to reset");

        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            0
        );
        let stats = debugger.get_all_stats().expect("Failed to get stats");
        assert!(stats.is_empty());
    }

    #[test]
    fn test_wasi_debugger_max_traces() {
        let debugger = WasiDebugger::with_max_traces(5);

        for _ in 0..10 {
            let entry = WasiTraceEntry::new(WasiSyscall::FdRead);
            debugger.trace(entry).expect("Failed to trace");
        }

        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            5
        );
    }

    #[test]
    fn test_wasi_debugger_format_traces() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_fd(0)
            .with_bytes(100);

        debugger.trace(entry).expect("Failed to trace");

        let output = debugger.format_traces().expect("Failed to format traces");
        assert!(output.contains("fd_read"));
        assert!(output.contains("fd=0"));
    }

    #[test]
    fn test_wasi_debugger_format_stats() {
        let debugger = WasiDebugger::new();

        let entry = WasiTraceEntry::new(WasiSyscall::FdWrite)
            .with_fd(1)
            .with_bytes(50);

        debugger.trace(entry).expect("Failed to trace");

        let output = debugger.format_stats().expect("Failed to format stats");
        assert!(output.contains("fd_write"));
        assert!(output.contains("Calls: 1"));
    }

    #[test]
    fn test_wasi_debugger_disabled_tracing() {
        let debugger = WasiDebugger::new();
        debugger.set_enabled(false).expect("Failed to disable");

        let entry = WasiTraceEntry::new(WasiSyscall::FdRead);
        debugger.trace(entry).expect("Failed to trace");

        // Should not add any traces when disabled
        assert_eq!(
            debugger.trace_count().expect("Failed to get trace count"),
            0
        );
    }
}
