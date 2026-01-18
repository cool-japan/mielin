//! Inter-CPU Communication Example
//!
//! Demonstrates IPI (Inter-Processor Interrupts) and message passing
//! between CPU cores in mielin-kernel.
//!
//! Run with: cargo run --example ipc --features std

use mielin_kernel::ipc::*;
use mielin_kernel::percpu;
use std::println;

fn main() {
    println!("=== MielinOS IPC Example ===\n");

    // Initialize the per-CPU subsystem with 4 CPUs
    println!("1. Initializing 4 CPU cores...");
    percpu::init(4).expect("Failed to initialize per-CPU subsystem");
    println!("   ✓ 4 CPUs initialized\n");

    // Demonstrate IPI functionality
    demonstrate_ipi();

    // Demonstrate message passing
    demonstrate_message_passing();

    // Demonstrate CPU barriers
    demonstrate_cpu_barrier();

    // Show statistics
    show_statistics();

    println!("\n=== Example Complete ===");
}

fn demonstrate_ipi() {
    println!("2. Inter-Processor Interrupts (IPI)");
    println!("   -----------------------------------");

    // Send IPI to wake up CPU 2
    println!("   • Sending Wakeup IPI to CPU 2...");
    let success = send_ipi(2, IpiType::Wakeup);
    println!(
        "     Result: {}",
        if success { "✓ Sent" } else { "✗ Failed" }
    );

    // Check if IPI is pending (simulated - we'd check on CPU 2)
    println!("   • Checking IPI status on CPU 2...");
    // Note: In a real system, this would be checked by CPU 2
    if let Some(stats) = ipi_stats(2) {
        println!("     Received: {} IPIs", stats.received);
        println!("     Pending: {} IPIs", stats.pending);
    }

    // Send IPI to all CPUs for TLB flush
    println!("   • Broadcasting TLB flush IPI to all CPUs...");
    let sent_count = send_ipi_all(IpiType::TlbFlush);
    println!("     Result: ✓ Sent to {} CPUs", sent_count);

    // Send IPI to all CPUs except self (reschedule)
    println!("   • Sending Reschedule IPI to all CPUs except CPU 0...");
    let sent_count = send_ipi_all_but_self(IpiType::Reschedule);
    println!("     Result: ✓ Sent to {} CPUs", sent_count);

    // Check and handle pending IPI on current CPU
    println!("   • Checking pending IPIs on current CPU...");
    if is_ipi_pending(IpiType::TlbFlush) {
        println!("     TLB flush IPI is pending!");
        if handle_ipi(IpiType::TlbFlush) {
            println!("     ✓ Handled TLB flush IPI");
        }
    }

    println!();
}

fn demonstrate_message_passing() {
    println!("3. Lock-Free Message Passing");
    println!("   ---------------------------");

    // Send task migration message from CPU 0 to CPU 1
    println!("   • Sending task migration message (CPU 0 → CPU 1)...");
    let msg = Message::task_migration(0, 1, 42);
    let success = send_message(1, msg);
    println!(
        "     Result: {}",
        if success {
            "✓ Message sent (task_id: 42)"
        } else {
            "✗ Queue full"
        }
    );

    // Send memory allocation request from CPU 0 to CPU 2
    println!("   • Sending memory alloc request (CPU 0 → CPU 2)...");
    let msg = Message::memory_alloc(0, 2, 4096);
    let success = send_message(2, msg);
    println!(
        "     Result: {}",
        if success {
            "✓ Message sent (size: 4096 bytes)"
        } else {
            "✗ Queue full"
        }
    );

    // Send acknowledgment from CPU 0 to CPU 3
    println!("   • Sending acknowledgment (CPU 0 → CPU 3)...");
    let msg = Message::ack(0, 3, 123);
    let success = send_message(3, msg);
    println!(
        "     Result: {}",
        if success {
            "✓ Message sent (seq: 123)"
        } else {
            "✗ Queue full"
        }
    );

    // Check pending messages
    println!("   • Checking pending messages on current CPU...");
    let pending = pending_messages();
    println!("     Pending: {} messages", pending);

    // Demonstrate receiving messages (in a real system, this would be on the target CPU)
    println!("   • Simulating message reception...");
    println!("     Note: In production, target CPU would receive these");

    println!();
}

fn demonstrate_cpu_barrier() {
    println!("4. CPU Barrier Synchronization");
    println!("   ----------------------------");

    // Create a barrier for 4 CPUs
    println!("   • Creating barrier for 4 CPUs...");
    let barrier = CpuBarrier::new(4);
    println!("     ✓ Barrier created");

    // In a real multi-threaded scenario, each CPU would call barrier.wait()
    println!("   • In production, all 4 CPUs would synchronize here");
    println!("     Example: barrier.wait() on each CPU");

    // Single-threaded simulation
    println!("   • Simulating single CPU passing barrier...");
    barrier.reset(1);
    barrier.wait();
    println!("     ✓ Barrier passed (single CPU mode)");

    println!();
}

fn show_statistics() {
    println!("5. IPC Statistics");
    println!("   ---------------");

    // Show IPI statistics for all CPUs
    println!("   IPI Statistics:");
    for cpu_id in 0..4 {
        if let Some(stats) = ipi_stats(cpu_id) {
            println!(
                "     CPU {}: Sent={}, Received={}, Pending={}, Errors={}",
                cpu_id, stats.sent, stats.received, stats.pending, stats.errors
            );
        }
    }

    // Show message queue statistics
    println!("\n   Message Queue Statistics:");
    for from_cpu in 0..4 {
        for to_cpu in 0..4 {
            if from_cpu != to_cpu {
                if let Some(stats) = message_queue_stats(from_cpu, to_cpu) {
                    if stats.sent > 0 || stats.received > 0 {
                        println!(
                            "     CPU {} → CPU {}: Sent={}, Received={}, Pending={}, Full={}",
                            from_cpu,
                            to_cpu,
                            stats.sent,
                            stats.received,
                            stats.pending,
                            stats.full_count
                        );
                    }
                }
            }
        }
    }

    println!();
}
