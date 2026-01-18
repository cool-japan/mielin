//! Interrupt Handling Example
//!
//! This example demonstrates the interrupt handling subsystem:
//! - Registering IRQ handlers
//! - Dispatching interrupts
//! - Bottom-half processing
//! - Interrupt statistics

use mielin_kernel::interrupt::{self, InterruptContext, IrqPriority};

// Handler for IRQ 32 (example timer)
fn timer_handler(ctx: &InterruptContext) {
    println!(
        "Timer interrupt! IRQ: {}, CPU: {}, Timestamp: {}",
        ctx.irq, ctx.cpu_id, ctx.timestamp
    );

    // Schedule bottom-half work
    let _ = interrupt::schedule_work(timer_bottom_half);
}

// Bottom-half handler for timer
fn timer_bottom_half() {
    println!("Timer bottom-half: Processing deferred work...");
}

// Handler for IRQ 33 (example network)
fn network_handler(ctx: &InterruptContext) {
    println!(
        "Network interrupt! IRQ: {}, CPU: {}, Timestamp: {}",
        ctx.irq, ctx.cpu_id, ctx.timestamp
    );

    // Schedule bottom-half work
    let _ = interrupt::schedule_work(network_bottom_half);
}

// Bottom-half handler for network
fn network_bottom_half() {
    println!("Network bottom-half: Processing packet...");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Interrupt Handling Example ===\n");

    // Initialize interrupt subsystem
    println!("1. Initializing interrupt subsystem...");
    interrupt::init()?;
    println!("   ✓ Interrupt subsystem initialized\n");

    // Register interrupt handlers
    println!("2. Registering IRQ handlers...");
    interrupt::register_irq_handler(32, timer_handler, IrqPriority::Highest)?;
    println!("   ✓ Registered timer handler (IRQ 32, Highest priority)");

    interrupt::register_irq_handler(33, network_handler, IrqPriority::High)?;
    println!("   ✓ Registered network handler (IRQ 33, High priority)\n");

    // Enable IRQs
    println!("3. Enabling IRQs...");
    interrupt::enable_irq(32)?;
    interrupt::enable_irq(33)?;
    println!("   ✓ IRQs 32 and 33 enabled\n");

    // Simulate interrupts by calling dispatch_interrupt
    println!("4. Simulating interrupts...\n");

    let timer_ctx = InterruptContext::new(32, IrqPriority::Highest.as_u8(), 0);
    interrupt::dispatch_interrupt(&timer_ctx)?;
    println!();

    let network_ctx = InterruptContext::new(33, IrqPriority::High.as_u8(), 0);
    interrupt::dispatch_interrupt(&network_ctx)?;
    println!();

    // Process bottom-half work
    println!("5. Processing bottom-half work queue...");
    let work_count = interrupt::process_work_queue();
    println!("   ✓ Processed {} work items\n", work_count);

    // Get statistics
    println!("6. Interrupt statistics:");
    let stats = interrupt::get_stats();
    println!("   - Total interrupts: {}", stats.total_interrupts);
    println!("   - Total work items: {}", stats.total_work_items);
    println!("   - Max latency: {} ns", stats.max_latency_ns);
    println!("   - Enabled IRQs: {}", stats.enabled_irqs);
    println!("   - Registered handlers: {}", stats.registered_handlers);
    println!("   - Work queue depth: {}\n", stats.work_queue_depth);

    // Get per-IRQ statistics
    println!("7. Per-IRQ statistics:");
    let timer_stats = interrupt::get_irq_stats(32)?;
    println!("   IRQ 32 (Timer):");
    println!("      - Priority: {}", timer_stats.priority);
    println!("      - Enabled: {}", timer_stats.enabled);
    println!("      - Count: {}", timer_stats.count);

    let network_stats = interrupt::get_irq_stats(33)?;
    println!("   IRQ 33 (Network):");
    println!("      - Priority: {}", network_stats.priority);
    println!("      - Enabled: {}", network_stats.enabled);
    println!("      - Count: {}\n", network_stats.count);

    // Disable IRQs
    println!("8. Disabling IRQs...");
    interrupt::disable_irq(32)?;
    interrupt::disable_irq(33)?;
    println!("   ✓ IRQs disabled\n");

    println!("=== Example completed successfully ===");

    Ok(())
}
