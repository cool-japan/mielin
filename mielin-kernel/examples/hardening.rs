//! Production Hardening Example
//!
//! Demonstrates stress testing, fuzzing, and memory leak detection.

use mielin_kernel::hardening::{self, LeakStats, ProfilingSession, StressTestConfig};

fn main() {
    println!("=== MielinOS Production Hardening Example ===\n");

    // Example 1: Stress testing
    println!("1. Stress Testing");
    println!("\n   Light stress test (smoke test):");
    let light_config = StressTestConfig::light();
    println!("     Iterations: {}", light_config.iterations);
    println!("     Tasks: {}", light_config.num_tasks);
    println!("     Allocations: {}", light_config.num_allocs);
    println!("     Alloc size: {:?} bytes", light_config.alloc_size_range);

    match hardening::run_stress_test(light_config) {
        Ok(result) => {
            println!("\n   Results:");
            println!("     Iterations completed: {}", result.iterations_completed);
            println!("     Failures: {}", result.failures);
            println!("     Duration: {} ms", result.duration_ns / 1_000_000);
            println!(
                "     Memory allocated: {} KB",
                result.memory_allocated / 1024
            );
            println!("     Memory freed: {} KB", result.memory_freed / 1024);
            println!("     Peak memory: {} KB", result.peak_memory / 1024);
            println!("     Tasks spawned: {}", result.tasks_spawned);
            println!("     Tasks terminated: {}", result.tasks_terminated);
            println!(
                "     Status: {}",
                if result.passed() { "PASS" } else { "FAIL" }
            );
        }
        Err(e) => println!("   Error: {}", e),
    }
    println!();

    println!("   Default stress test:");
    let default_config = StressTestConfig::default();
    println!("     Iterations: {}", default_config.iterations);
    println!("     Tasks: {}", default_config.num_tasks);
    println!("     Scheduler stress: {}", default_config.stress_scheduler);
    println!("     Memory stress: {}", default_config.stress_memory);
    println!();

    println!("   Aggressive stress test:");
    let aggressive_config = StressTestConfig::aggressive();
    println!("     Iterations: {}", aggressive_config.iterations);
    println!("     Tasks: {}", aggressive_config.num_tasks);
    println!(
        "     Interrupt stress: {}",
        aggressive_config.stress_interrupts
    );
    println!(
        "     Multi-core stress: {}",
        aggressive_config.stress_multicore
    );
    println!();

    // Example 2: Memory leak detection
    println!("2. Memory Leak Detection");
    println!("\n   Starting leak detection...");
    hardening::start_leak_detection();

    // Simulate some allocations
    println!("   Simulating allocations:");
    hardening::record_allocation(0x1000, 1024);
    println!("     Allocated 1024 bytes at 0x1000");
    hardening::record_allocation(0x2000, 2048);
    println!("     Allocated 2048 bytes at 0x2000");
    hardening::record_allocation(0x3000, 4096);
    println!("     Allocated 4096 bytes at 0x3000");

    // Free some allocations
    println!("\n   Simulating deallocations:");
    hardening::record_deallocation(0x1000);
    println!("     Freed allocation at 0x1000");

    // Check for leaks
    println!("\n   Checking for leaks:");
    let leaks = hardening::check_leaks();
    if leaks.is_empty() {
        println!("     No leaks detected");
    } else {
        println!("     {} leak(s) detected:", leaks.len());
        for leak in &leaks {
            println!(
                "       Address: {:#x}, Size: {} bytes, Age: {} ticks",
                leak.addr, leak.size, leak.age
            );
        }
    }

    // Get leak statistics
    println!("\n   Leak detection statistics:");
    let leak_stats: LeakStats = hardening::get_leak_stats();
    println!("     Total allocations: {}", leak_stats.total_allocations);
    println!(
        "     Total deallocations: {}",
        leak_stats.total_deallocations
    );
    println!("     Bytes allocated: {}", leak_stats.bytes_allocated);
    println!("     Bytes freed: {}", leak_stats.bytes_freed);
    println!("     Active allocations: {}", leak_stats.active_allocations);
    println!("     Leak size: {} bytes", leak_stats.leak_size());
    println!(
        "     Allocation balance: {}",
        leak_stats.allocation_balance()
    );
    println!();

    // Example 3: Performance profiling
    println!("3. Performance Profiling");
    let session = ProfilingSession::new("example_workload".to_string());

    println!("   Running profiled workload...");
    session.sample("operation_1".to_string(), 100);
    session.sample("operation_2".to_string(), 200);
    session.sample("operation_3".to_string(), 150);
    session.sample("operation_4".to_string(), 180);
    session.sample("operation_5".to_string(), 120);

    println!("\n   Profiling report:");
    let report = session.report();
    println!("     Session: {}", report.session_name);
    println!("     Total samples: {}", report.total_samples);
    println!("     Total time: {} cycles", report.total_time);
    println!("     Min: {} cycles", report.min);
    println!("     Max: {} cycles", report.max);
    println!("     Avg: {} cycles", report.avg);
    println!();

    // Example 4: Fuzzing (demonstration)
    println!("4. Fuzzing (Framework)");
    println!("   Fuzzing operations available:");
    println!("     ✓ SpawnTask");
    println!("     ✓ TerminateTask");
    println!("     ✓ AllocPages");
    println!("     ✓ FreePages");
    println!("     ✓ Schedule");
    println!("     ✓ Yield");
    println!("     ✓ MapMemory");
    println!("     ✓ UnmapMemory");
    println!("     ✓ SendIpi");
    println!("\n   Use cargo-fuzz to run fuzzing campaigns:");
    println!("     $ cargo fuzz run memory_fuzzer");
    println!("     $ cargo fuzz run scheduler_fuzzer");
    println!();

    println!("=== Summary ===");
    println!("Production hardening features demonstrated:");
    println!("✓ Stress testing with configurable scenarios");
    println!("✓ Memory leak detection and tracking");
    println!("✓ Performance profiling infrastructure");
    println!("✓ Fuzzing operation framework");
    println!("\nUse these tools for:");
    println!("- Pre-production validation");
    println!("- Continuous integration testing");
    println!("- Security vulnerability discovery");
    println!("- Performance regression detection");
    println!("- Long-running stability testing");
}
