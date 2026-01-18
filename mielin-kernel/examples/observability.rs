//! Kernel Observability Example
//!
//! Demonstrates kernel tracing, performance counters, and profiling.

use mielin_kernel::observability::{self, EventType, TracepointId};

fn main() {
    println!("=== MielinOS Observability Example ===\n");

    // Initialize tracing with 64KB buffer
    println!("1. Initializing tracing subsystem...");
    match observability::init(65536) {
        Ok(_) => println!("   Tracing initialized successfully"),
        Err(e) => {
            println!("   Error: {}", e);
            return;
        }
    }
    println!();

    // Enable specific tracepoints
    println!("2. Enabling tracepoints...");
    observability::enable_tracepoint(TracepointId::TaskSpawn);
    observability::enable_tracepoint(TracepointId::TaskTerminate);
    observability::enable_tracepoint(TracepointId::TaskSchedule);
    observability::enable_tracepoint(TracepointId::PageAlloc);
    observability::enable_tracepoint(TracepointId::PageFree);
    println!("   Enabled: TaskSpawn, TaskTerminate, TaskSchedule, PageAlloc, PageFree");
    println!();

    // Emit some trace events
    println!("3. Emitting trace events...");
    observability::trace_event(EventType::TaskSpawn {
        task_id: 1,
        priority: 100,
    });
    observability::trace_event(EventType::TaskSpawn {
        task_id: 2,
        priority: 50,
    });
    observability::trace_event(EventType::PageAlloc {
        page_addr: 0x1000,
        count: 4,
    });
    observability::trace_event(EventType::TaskSchedule {
        task_id: 1,
        cpu_id: 0,
    });
    observability::trace_event(EventType::TaskTerminate { task_id: 2 });
    observability::trace_event(EventType::PageFree {
        page_addr: 0x1000,
        count: 4,
    });
    println!("   Emitted 6 trace events");
    println!();

    // Read and display events
    println!("4. Reading trace events:");
    let mut count = 0;
    observability::for_each_event(|event| {
        count += 1;
        println!(
            "   Event #{}: CPU={}, TS={}, Type={:?}",
            count, event.cpu_id, event.timestamp, event.event
        );
    });
    println!();

    // Get tracing statistics
    println!("5. Tracing statistics:");
    let stats = observability::get_stats();
    println!("   Total events: {}", stats.total_events);
    println!("   Buffered events: {}", stats.buffered_events);
    println!("   Dropped events: {}", stats.dropped_events);
    println!("   Tracing enabled: {}", stats.tracing_enabled);
    println!();

    // Performance counters
    println!("6. Performance counters:");
    let pmc = observability::read_pmc();
    println!("   CPU cycles: {}", pmc.cycles);
    println!("   Instructions: {}", pmc.instructions);
    println!("   L1 cache misses: {}", pmc.l1_cache_misses);
    println!("   L2 cache misses: {}", pmc.l2_cache_misses);
    println!("   Branch misses: {}", pmc.branch_misses);
    println!("   TLB misses: {}", pmc.tlb_misses);
    println!("   IPC: {:.2}", pmc.ipc());
    println!("   L1 miss rate: {:.4}", pmc.l1_miss_rate());
    println!();

    // Enable all tracepoints
    println!("7. Enabling all tracepoints...");
    observability::enable_all_tracepoints();
    for i in 0..20 {
        let tp_id = unsafe { core::mem::transmute::<u8, TracepointId>(i) };
        if observability::is_tracepoint_enabled(tp_id) {
            println!("   ✓ {}", tp_id.name());
        }
    }
    println!();

    // Emit more events with all tracepoints enabled
    println!("8. Emitting more events (all tracepoints):");
    observability::trace_event(EventType::InterruptEntry { irq: 32, cpu_id: 0 });
    observability::trace_event(EventType::InterruptExit {
        irq: 32,
        duration_ns: 500,
    });
    observability::trace_event(EventType::TimerTick { jiffies: 1000 });
    observability::trace_event(EventType::ContextSwitch {
        from_task: 1,
        to_task: 3,
    });
    println!("   Emitted 4 additional events");
    println!();

    // Final statistics
    println!("9. Final statistics:");
    let final_stats = observability::get_stats();
    println!("   Total events: {}", final_stats.total_events);
    println!("   Events in buffer: {}", final_stats.buffered_events);
    println!();

    println!("=== Summary ===");
    println!("Observability features demonstrated:");
    println!("✓ Tracing initialization with configurable buffer");
    println!("✓ Selective tracepoint enabling/disabling");
    println!("✓ Trace event emission and collection");
    println!("✓ Performance counter reading (PMC)");
    println!("✓ Statistics tracking and reporting");
    println!("\nUse these features for:");
    println!("- Debugging kernel behavior");
    println!("- Performance analysis and optimization");
    println!("- Production monitoring and alerting");
    println!("- Security audit logging");
}
