//! Kernel Configuration Example
//!
//! Demonstrates how to configure the MielinOS kernel with custom parameters.

use mielin_kernel::config::{KernelConfig, PageSize};

fn main() {
    println!("MielinOS Kernel Configuration Examples\n");

    // Example 1: Default configuration
    println!("=== Default Configuration ===");
    let default_config = KernelConfig::default();
    print_config(&default_config);

    // Example 2: Embedded system configuration
    println!("\n=== Embedded Configuration ===");
    let embedded_config = KernelConfig::embedded();
    print_config(&embedded_config);
    println!(
        "Total memory: {} KB",
        embedded_config.memory.total_memory_bytes() / 1024
    );

    // Example 3: High-performance configuration
    println!("\n=== High-Performance Configuration ===");
    let hp_config = KernelConfig::high_performance();
    print_config(&hp_config);
    println!(
        "Total memory: {} MB",
        hp_config.memory.total_memory_bytes() / (1024 * 1024)
    );

    // Example 4: ARM64 with 16KB pages
    println!("\n=== ARM64 16KB Page Configuration ===");
    let arm_config = KernelConfig::arm64_16kb();
    print_config(&arm_config);
    println!(
        "Total memory: {} MB",
        arm_config.memory.total_memory_bytes() / (1024 * 1024)
    );

    // Example 5: Custom configuration using builder
    println!("\n=== Custom Configuration (Builder Pattern) ===");
    let custom_config = KernelConfig::builder()
        .max_pages(2048)
        .max_tasks(128)
        .max_cpus(8)
        .page_size(PageSize::Size8KB)
        .build()
        .expect("Failed to build config");
    print_config(&custom_config);
    println!(
        "Total memory: {} MB",
        custom_config.memory.total_memory_bytes() / (1024 * 1024)
    );

    // Example 6: Page size operations
    println!("\n=== Page Size Operations ===");
    demonstrate_page_sizes();

    // Example 7: Configuration validation
    println!("\n=== Configuration Validation ===");
    demonstrate_validation();
}

fn print_config(config: &KernelConfig) {
    println!("Memory:");
    println!("  Page size: {} bytes", config.memory.page_size.bytes());
    println!("  Max pages: {}", config.memory.max_pages);
    println!("  Free list: {}", config.memory.use_free_list);
    println!("  Coalescing: {}", config.memory.enable_coalescing);

    println!("Scheduler:");
    println!("  Max tasks: {}", config.scheduler.max_tasks);
    println!("  Priority: {}", config.scheduler.enable_priority);
    println!("  Metrics: {}", config.scheduler.enable_metrics);

    println!("Multi-core:");
    println!("  Max CPUs: {}", config.multicore.max_cpus);
    println!("  Per-CPU pools: {}", config.multicore.enable_per_cpu_pools);
    println!(
        "  Load balancing: {}",
        config.multicore.enable_load_balancing
    );
    println!("  Work stealing: {}", config.multicore.enable_work_stealing);
}

fn demonstrate_page_sizes() {
    let sizes = [
        PageSize::Size4KB,
        PageSize::Size8KB,
        PageSize::Size16KB,
        PageSize::Size64KB,
    ];

    for size in &sizes {
        println!("Page size: {} bytes", size.bytes());
        println!("  Shift: {}", size.shift());
        println!("  4097 aligned up: {}", size.align_up(4097));
        println!("  4097 aligned down: {}", size.align_down(4097));
        println!("  Is 8192 aligned: {}", size.is_aligned(8192));
        println!();
    }
}

fn demonstrate_validation() {
    // Valid configuration
    let valid = KernelConfig::builder()
        .max_pages(1024)
        .max_tasks(64)
        .max_cpus(8)
        .build();
    match valid {
        Ok(_) => println!("✓ Valid configuration accepted"),
        Err(e) => println!("✗ Unexpected error: {}", e),
    }

    // Invalid: zero pages
    let invalid_pages = KernelConfig::builder().max_pages(0).build();
    match invalid_pages {
        Ok(_) => println!("✗ Should have rejected zero pages"),
        Err(e) => println!("✓ Correctly rejected zero pages: {}", e),
    }

    // Invalid: zero tasks
    let invalid_tasks = KernelConfig::builder().max_tasks(0).build();
    match invalid_tasks {
        Ok(_) => println!("✗ Should have rejected zero tasks"),
        Err(e) => println!("✓ Correctly rejected zero tasks: {}", e),
    }

    // Invalid: zero CPUs
    let invalid_cpus = KernelConfig::builder().max_cpus(0).build();
    match invalid_cpus {
        Ok(_) => println!("✗ Should have rejected zero CPUs"),
        Err(e) => println!("✓ Correctly rejected zero CPUs: {}", e),
    }

    // Invalid: too many pages
    let too_many_pages = KernelConfig::builder().max_pages(2_000_000).build();
    match too_many_pages {
        Ok(_) => println!("✗ Should have rejected excessive pages"),
        Err(e) => println!("✓ Correctly rejected excessive pages: {}", e),
    }
}
