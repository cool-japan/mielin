//! WASI Debugging Example
//!
//! This example demonstrates how to use WASI debugging tools to trace
//! and monitor WASI system calls.

use mielin_wasm::wasi_debug::{WasiDebugger, WasiSyscall, WasiTraceEntry};
use std::time::Duration;

fn main() {
    println!("=== WASI Debugging Example ===\n");

    // Part 1: Create a WASI debugger
    println!("Part 1: Creating WASI Debugger");
    println!("-------------------------------");

    let debugger = WasiDebugger::new();
    println!("WASI Debugger created");
    println!(
        "Tracing enabled: {}",
        debugger.is_enabled().expect("Failed to check enabled")
    );
    println!(
        "Max traces: {}",
        debugger.trace_count().expect("Failed to get count")
    );
    println!();

    // Part 2: Simulate some WASI calls
    println!("Part 2: Simulating WASI Calls");
    println!("------------------------------");

    // Simulate fd_read
    let read_entry = WasiTraceEntry::new(WasiSyscall::FdRead)
        .with_fd(0)
        .with_bytes(100)
        .with_duration(Duration::from_micros(150))
        .with_arg("stdin");

    debugger.trace(read_entry).expect("Failed to trace");
    println!("Traced: fd_read (stdin, 100 bytes)");

    // Simulate fd_write to stdout
    let write_entry = WasiTraceEntry::new(WasiSyscall::FdWrite)
        .with_fd(1)
        .with_bytes(50)
        .with_duration(Duration::from_micros(80))
        .with_arg("stdout");

    debugger.trace(write_entry).expect("Failed to trace");
    println!("Traced: fd_write (stdout, 50 bytes)");

    // Simulate fd_write to stderr
    let stderr_entry = WasiTraceEntry::new(WasiSyscall::FdWrite)
        .with_fd(2)
        .with_bytes(25)
        .with_duration(Duration::from_micros(60))
        .with_arg("stderr");

    debugger.trace(stderr_entry).expect("Failed to trace");
    println!("Traced: fd_write (stderr, 25 bytes)");

    // Simulate clock_time_get
    let clock_entry = WasiTraceEntry::new(WasiSyscall::ClockTimeGet)
        .with_duration(Duration::from_nanos(500))
        .with_arg("realtime");

    debugger.trace(clock_entry).expect("Failed to trace");
    println!("Traced: clock_time_get (realtime)");

    // Simulate more read operations
    for i in 0..5 {
        let entry = WasiTraceEntry::new(WasiSyscall::FdRead)
            .with_fd(0)
            .with_bytes(20 + i * 10)
            .with_duration(Duration::from_micros(100 + (i as u64) * 20));

        debugger.trace(entry).expect("Failed to trace");
    }
    println!("Traced: 5 more fd_read operations");
    println!();

    // Part 3: Get trace information
    println!("Part 3: Trace Information");
    println!("-------------------------");

    println!(
        "Total traces: {}",
        debugger.trace_count().expect("Failed to get count")
    );

    let read_traces = debugger
        .get_traces_for(WasiSyscall::FdRead)
        .expect("Failed to get traces");
    println!("fd_read traces: {}", read_traces.len());

    let write_traces = debugger
        .get_traces_for(WasiSyscall::FdWrite)
        .expect("Failed to get traces");
    println!("fd_write traces: {}", write_traces.len());
    println!();

    // Part 4: Statistics
    println!("Part 4: Syscall Statistics");
    println!("--------------------------");

    if let Some(read_stats) = debugger
        .get_stats(WasiSyscall::FdRead)
        .expect("Failed to get stats")
    {
        println!("fd_read statistics:");
        println!("  Calls: {}", read_stats.call_count);
        println!("  Errors: {}", read_stats.error_count);
        println!("  Total duration: {:?}", read_stats.total_duration);
        println!("  Avg duration: {:?}", read_stats.avg_duration);
        if let Some(min) = read_stats.min_duration {
            println!("  Min duration: {:?}", min);
        }
        if let Some(max) = read_stats.max_duration {
            println!("  Max duration: {:?}", max);
        }
    }
    println!();

    if let Some(write_stats) = debugger
        .get_stats(WasiSyscall::FdWrite)
        .expect("Failed to get stats")
    {
        println!("fd_write statistics:");
        println!("  Calls: {}", write_stats.call_count);
        println!("  Total duration: {:?}", write_stats.total_duration);
        println!("  Avg duration: {:?}", write_stats.avg_duration);
    }
    println!();

    // Part 5: File descriptor monitoring
    println!("Part 5: File Descriptor Monitoring");
    println!("-----------------------------------");

    let fd_monitors = debugger
        .get_all_fd_monitors()
        .expect("Failed to get monitors");
    println!("Monitored FDs: {}", fd_monitors.len());

    for monitor in &fd_monitors {
        println!("\nFD {} ({}):", monitor.fd, monitor.description);
        println!(
            "  Reads: {} ({} bytes)",
            monitor.read_count, monitor.bytes_read
        );
        println!(
            "  Writes: {} ({} bytes)",
            monitor.write_count, monitor.bytes_written
        );
        println!("  Total ops: {}", monitor.total_ops());
        println!("  Total bytes: {}", monitor.total_bytes());
    }
    println!();

    // Part 6: Formatted output
    println!("Part 6: Formatted Trace Output");
    println!("-------------------------------");

    let trace_output = debugger.format_traces().expect("Failed to format traces");
    println!("First 5 traces:");
    for line in trace_output.lines().take(5) {
        println!("  {}", line);
    }
    println!("  ... ({} more traces)", read_traces.len() - 5);
    println!();

    // Part 7: Statistics summary
    println!("Part 7: Statistics Summary");
    println!("--------------------------");

    let stats_output = debugger.format_stats().expect("Failed to format stats");
    println!("{}", stats_output);

    // Part 8: FD monitor summary
    println!("Part 8: FD Monitor Summary");
    println!("--------------------------");

    let fd_output = debugger
        .format_fd_monitors()
        .expect("Failed to format monitors");
    println!("{}", fd_output);

    // Part 9: Enable/disable tracing
    println!("Part 9: Enable/Disable Tracing");
    println!("-------------------------------");

    let count_before = debugger.trace_count().expect("Failed to get count");
    println!("Traces before disable: {}", count_before);

    debugger.set_enabled(false).expect("Failed to disable");
    println!("Tracing disabled");

    // This trace should not be recorded
    let ignored_entry = WasiTraceEntry::new(WasiSyscall::FdRead).with_fd(0);
    debugger.trace(ignored_entry).expect("Failed to trace");

    let count_after = debugger.trace_count().expect("Failed to get count");
    println!("Traces after disable: {} (no change)", count_after);

    debugger.set_enabled(true).expect("Failed to enable");
    println!("Tracing re-enabled");
    println!();

    // Part 10: Cleanup
    println!("Part 10: Cleanup");
    println!("----------------");

    println!(
        "Clearing {} traces...",
        debugger.trace_count().expect("Failed to get count")
    );
    debugger.reset().expect("Failed to reset");
    println!(
        "Traces after reset: {}",
        debugger.trace_count().expect("Failed to get count")
    );
    println!();

    println!("=== Example Complete ===");
}
