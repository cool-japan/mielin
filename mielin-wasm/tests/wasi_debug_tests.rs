//! Integration tests for the wasi_debug module
//!
//! These tests verify WASI debugging capabilities in realistic scenarios.

use mielin_wasm::wasi::Errno;
use mielin_wasm::wasi_debug::{
    FdMonitor, WasiDebugger, WasiSyscall, WasiSyscallStats, WasiTraceEntry,
};
use std::time::Duration;

#[test]
fn test_wasi_debugger_full_workflow() {
    let debugger = WasiDebugger::new();

    // Simulate a series of WASI calls
    let calls = vec![
        (WasiSyscall::FdRead, 0, 100),
        (WasiSyscall::FdWrite, 1, 50),
        (WasiSyscall::FdWrite, 2, 25),
        (WasiSyscall::FdRead, 0, 200),
        (WasiSyscall::ClockTimeGet, -1, 0),
    ];

    for (syscall, fd, bytes) in calls {
        let mut entry = WasiTraceEntry::new(syscall).with_duration(Duration::from_micros(100));

        if fd >= 0 {
            entry = entry.with_fd(fd);
        }

        if bytes > 0 {
            entry = entry.with_bytes(bytes);
        }

        debugger.trace(entry).expect("Failed to trace");
    }

    // Verify traces were recorded
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 5);

    // Verify statistics
    let read_stats = debugger
        .get_stats(WasiSyscall::FdRead)
        .expect("Failed to get stats");
    assert!(read_stats.is_some());

    let read_stats = read_stats.expect("Stats should exist");
    assert_eq!(read_stats.call_count, 2);

    let write_stats = debugger
        .get_stats(WasiSyscall::FdWrite)
        .expect("Failed to get stats");
    assert!(write_stats.is_some());

    let write_stats = write_stats.expect("Stats should exist");
    assert_eq!(write_stats.call_count, 2);
}

#[test]
fn test_wasi_debugger_fd_monitoring_comprehensive() {
    let debugger = WasiDebugger::new();

    // Simulate multiple operations on different FDs
    for _ in 0..10 {
        debugger
            .trace(
                WasiTraceEntry::new(WasiSyscall::FdRead)
                    .with_fd(0)
                    .with_bytes(100),
            )
            .expect("Failed to trace");
    }

    for _ in 0..5 {
        debugger
            .trace(
                WasiTraceEntry::new(WasiSyscall::FdWrite)
                    .with_fd(1)
                    .with_bytes(50),
            )
            .expect("Failed to trace");
    }

    for _ in 0..3 {
        debugger
            .trace(
                WasiTraceEntry::new(WasiSyscall::FdWrite)
                    .with_fd(2)
                    .with_bytes(25),
            )
            .expect("Failed to trace");
    }

    // Check FD monitors
    let monitor0 = debugger.get_fd_monitor(0).expect("Failed to get monitor");
    assert!(monitor0.is_some());
    let monitor0 = monitor0.expect("Monitor should exist");
    assert_eq!(monitor0.read_count, 10);
    assert_eq!(monitor0.bytes_read, 1000);

    let monitor1 = debugger.get_fd_monitor(1).expect("Failed to get monitor");
    assert!(monitor1.is_some());
    let monitor1 = monitor1.expect("Monitor should exist");
    assert_eq!(monitor1.write_count, 5);
    assert_eq!(monitor1.bytes_written, 250);

    let monitor2 = debugger.get_fd_monitor(2).expect("Failed to get monitor");
    assert!(monitor2.is_some());
    let monitor2 = monitor2.expect("Monitor should exist");
    assert_eq!(monitor2.write_count, 3);
    assert_eq!(monitor2.bytes_written, 75);
}

#[test]
fn test_wasi_debugger_statistics_accuracy() {
    let debugger = WasiDebugger::new();

    // Add traces with varying durations
    let durations = vec![100, 150, 200, 250, 300];

    for &duration_us in &durations {
        debugger
            .trace(
                WasiTraceEntry::new(WasiSyscall::FdRead)
                    .with_duration(Duration::from_micros(duration_us)),
            )
            .expect("Failed to trace");
    }

    let stats = debugger
        .get_stats(WasiSyscall::FdRead)
        .expect("Failed to get stats")
        .expect("Stats should exist");

    assert_eq!(stats.call_count, 5);
    assert_eq!(stats.min_duration, Some(Duration::from_micros(100)));
    assert_eq!(stats.max_duration, Some(Duration::from_micros(300)));

    // Average should be 200μs
    let avg_micros = stats.avg_duration.as_micros();
    assert_eq!(avg_micros, 200);
}

#[test]
fn test_wasi_debugger_error_tracking() {
    let debugger = WasiDebugger::new();

    // Add successful calls
    for _ in 0..10 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead).with_result(Errno::Success))
            .expect("Failed to trace");
    }

    // Add failed calls
    for _ in 0..3 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead).with_result(Errno::BadF))
            .expect("Failed to trace");
    }

    let stats = debugger
        .get_stats(WasiSyscall::FdRead)
        .expect("Failed to get stats")
        .expect("Stats should exist");

    assert_eq!(stats.call_count, 13);
    assert_eq!(stats.error_count, 3);
}

#[test]
fn test_wasi_debugger_max_traces_limit() {
    let debugger = WasiDebugger::with_max_traces(100);

    // Add 200 traces
    for _ in 0..200 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead))
            .expect("Failed to trace");
    }

    // Should only keep last 100
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 100);
}

#[test]
fn test_wasi_debugger_enable_disable_workflow() {
    let debugger = WasiDebugger::new();

    // Add some traces while enabled
    for _ in 0..5 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead))
            .expect("Failed to trace");
    }

    assert_eq!(debugger.trace_count().expect("Failed to get count"), 5);

    // Disable and try to add more
    debugger.set_enabled(false).expect("Failed to disable");

    for _ in 0..5 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead))
            .expect("Failed to trace");
    }

    // Count should remain 5
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 5);

    // Re-enable and add more
    debugger.set_enabled(true).expect("Failed to enable");

    for _ in 0..5 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead))
            .expect("Failed to trace");
    }

    // Count should now be 10
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 10);
}

#[test]
fn test_wasi_debugger_all_syscalls() {
    let debugger = WasiDebugger::new();

    // Trace each syscall type once
    for syscall in WasiSyscall::all() {
        debugger
            .trace(WasiTraceEntry::new(syscall))
            .expect("Failed to trace");
    }

    let all_stats = debugger.get_all_stats().expect("Failed to get all stats");

    // Should have stats for each syscall
    for syscall in WasiSyscall::all() {
        assert!(all_stats.contains_key(&syscall));
        let stats = &all_stats[&syscall];
        assert_eq!(stats.call_count, 1);
    }
}

#[test]
fn test_wasi_debugger_format_traces_output() {
    let debugger = WasiDebugger::new();

    debugger
        .trace(
            WasiTraceEntry::new(WasiSyscall::FdRead)
                .with_fd(0)
                .with_bytes(100),
        )
        .expect("Failed to trace");

    let output = debugger.format_traces().expect("Failed to format traces");

    assert!(output.contains("fd_read"));
    assert!(output.contains("fd=0"));
    assert!(output.contains("bytes=100"));
}

#[test]
fn test_wasi_debugger_format_stats_output() {
    let debugger = WasiDebugger::new();

    for _ in 0..5 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdWrite))
            .expect("Failed to trace");
    }

    let output = debugger.format_stats().expect("Failed to format stats");

    assert!(output.contains("fd_write"));
    assert!(output.contains("Calls: 5"));
}

#[test]
fn test_wasi_debugger_format_fd_monitors_output() {
    let debugger = WasiDebugger::new();

    debugger
        .trace(
            WasiTraceEntry::new(WasiSyscall::FdRead)
                .with_fd(0)
                .with_bytes(100),
        )
        .expect("Failed to trace");

    let output = debugger
        .format_fd_monitors()
        .expect("Failed to format monitors");

    assert!(output.contains("File Descriptor Monitors"));
    assert!(output.contains("Reads:"));
}

#[test]
fn test_wasi_debugger_clear_operations() {
    let debugger = WasiDebugger::new();

    // Add some data
    debugger
        .trace(
            WasiTraceEntry::new(WasiSyscall::FdRead)
                .with_fd(0)
                .with_bytes(100),
        )
        .expect("Failed to trace");

    assert_eq!(debugger.trace_count().expect("Failed to get count"), 1);

    // Clear traces only
    debugger.clear_traces().expect("Failed to clear traces");
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 0);

    // Stats should still exist
    let stats = debugger.get_all_stats().expect("Failed to get stats");
    assert!(!stats.is_empty());

    // Clear stats
    debugger.clear_stats().expect("Failed to clear stats");
    let stats = debugger.get_all_stats().expect("Failed to get stats");
    assert!(stats.is_empty());
}

#[test]
fn test_wasi_debugger_reset_full() {
    let debugger = WasiDebugger::new();

    // Add various data
    debugger
        .trace(
            WasiTraceEntry::new(WasiSyscall::FdRead)
                .with_fd(0)
                .with_bytes(100),
        )
        .expect("Failed to trace");

    // Reset everything
    debugger.reset().expect("Failed to reset");

    // Everything should be empty
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 0);

    let stats = debugger.get_all_stats().expect("Failed to get stats");
    assert!(stats.is_empty());

    let monitors = debugger
        .get_all_fd_monitors()
        .expect("Failed to get monitors");
    assert!(monitors.is_empty());
}

#[test]
fn test_fd_monitor_operations_detailed() {
    let mut monitor = FdMonitor::new(0).with_description("stdin");

    // Record multiple operations
    monitor.record_read(100);
    monitor.record_read(200);
    monitor.record_read(150);

    monitor.record_write(50);
    monitor.record_write(75);

    assert_eq!(monitor.read_count, 3);
    assert_eq!(monitor.write_count, 2);
    assert_eq!(monitor.bytes_read, 450);
    assert_eq!(monitor.bytes_written, 125);
    assert_eq!(monitor.total_ops(), 5);
    assert_eq!(monitor.total_bytes(), 575);
    assert!(monitor.last_access.is_some());
}

#[test]
fn test_wasi_trace_entry_with_args() {
    let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
        .with_fd(0)
        .with_bytes(100)
        .with_arg("buffer_ptr=0x1000")
        .with_arg("buffer_len=100")
        .with_duration(Duration::from_micros(150));

    assert_eq!(entry.syscall, WasiSyscall::FdRead);
    assert_eq!(entry.fd, Some(0));
    assert_eq!(entry.bytes, Some(100));
    assert_eq!(entry.args.len(), 2);
    assert_eq!(entry.duration, Duration::from_micros(150));
}

#[test]
fn test_wasi_syscall_stats_multiple_updates() {
    let mut stats = WasiSyscallStats::new();

    let entries = vec![
        WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_duration(Duration::from_micros(100))
            .with_result(Errno::Success),
        WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_duration(Duration::from_micros(200))
            .with_result(Errno::Success),
        WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_duration(Duration::from_micros(150))
            .with_result(Errno::BadF),
    ];

    for entry in entries {
        stats.update(&entry);
    }

    assert_eq!(stats.call_count, 3);
    assert_eq!(stats.error_count, 1);
    assert_eq!(stats.min_duration, Some(Duration::from_micros(100)));
    assert_eq!(stats.max_duration, Some(Duration::from_micros(200)));
    assert_eq!(stats.avg_duration.as_micros(), 150);
}

#[test]
fn test_wasi_debugger_get_traces_for_specific_syscall() {
    let debugger = WasiDebugger::new();

    // Add mixed syscalls
    for _ in 0..5 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdRead))
            .expect("Failed to trace");
    }

    for _ in 0..3 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::FdWrite))
            .expect("Failed to trace");
    }

    for _ in 0..2 {
        debugger
            .trace(WasiTraceEntry::new(WasiSyscall::ClockTimeGet))
            .expect("Failed to trace");
    }

    // Get specific traces
    let read_traces = debugger
        .get_traces_for(WasiSyscall::FdRead)
        .expect("Failed to get traces");
    assert_eq!(read_traces.len(), 5);

    let write_traces = debugger
        .get_traces_for(WasiSyscall::FdWrite)
        .expect("Failed to get traces");
    assert_eq!(write_traces.len(), 3);

    let clock_traces = debugger
        .get_traces_for(WasiSyscall::ClockTimeGet)
        .expect("Failed to get traces");
    assert_eq!(clock_traces.len(), 2);
}

#[test]
fn test_wasi_debugger_concurrent_tracing() {
    use std::sync::Arc;
    use std::thread;

    let debugger = Arc::new(WasiDebugger::new());
    let mut handles = vec![];

    // Spawn multiple threads that trace concurrently
    for _ in 0..10 {
        let debugger_clone = Arc::clone(&debugger);
        let handle = thread::spawn(move || {
            for _ in 0..10 {
                debugger_clone
                    .trace(WasiTraceEntry::new(WasiSyscall::FdRead))
                    .expect("Failed to trace");
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Should have 100 traces
    assert_eq!(debugger.trace_count().expect("Failed to get count"), 100);

    let stats = debugger
        .get_stats(WasiSyscall::FdRead)
        .expect("Failed to get stats")
        .expect("Stats should exist");
    assert_eq!(stats.call_count, 100);
}

#[test]
fn test_wasi_debugger_all_fd_monitors() {
    let debugger = WasiDebugger::new();

    // Create operations on multiple FDs
    for fd in 0..5 {
        debugger
            .trace(
                WasiTraceEntry::new(WasiSyscall::FdRead)
                    .with_fd(fd)
                    .with_bytes(100),
            )
            .expect("Failed to trace");
    }

    let monitors = debugger
        .get_all_fd_monitors()
        .expect("Failed to get monitors");

    assert_eq!(monitors.len(), 5);

    // Each monitor should have 1 read operation
    for monitor in monitors {
        assert_eq!(monitor.read_count, 1);
        assert_eq!(monitor.bytes_read, 100);
    }
}
