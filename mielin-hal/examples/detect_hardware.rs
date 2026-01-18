//! Hardware Detection Example
//!
//! This example demonstrates how to detect various hardware capabilities
//! using the mielin-hal crate.
//!
//! Run with: cargo run --example detect_hardware

extern crate alloc;

use mielin_hal::{
    capabilities::HardwareProfile, detect_architecture, platform, system::CpuTopology,
    system::MemoryInfo,
};

#[cfg(not(target_os = "none"))]
use std::println;

fn main() {
    println!("=== MielinOS Hardware Abstraction Layer ===\n");

    // Detect architecture
    let arch = detect_architecture();
    println!("Architecture: {}", arch);
    println!();

    // Detect hardware profile
    println!("--- Hardware Profile ---");
    let profile = HardwareProfile::detect();
    println!("Core count: {}", profile.core_count);
    println!(
        "Memory size: {} bytes ({} MB)",
        profile.memory_size,
        profile.memory_size / 1024 / 1024
    );
    println!("Page size: {} bytes", profile.page_size);
    println!();

    // Cache information
    println!("--- Cache Information ---");
    println!("L1 cache: {} bytes", profile.l1_cache_size);
    println!("L2 cache: {} bytes", profile.l2_cache_size);
    println!("L3 cache: {} bytes", profile.l3_cache_size);
    println!("Cache line size: {} bytes", profile.cache_line_size);
    println!();

    // Capabilities
    println!("--- Capabilities ---");
    println!("Has SIMD: {}", profile.has_simd());
    println!("Has SVE2: {}", profile.has_sve2());
    println!("Has NPU: {}", profile.has_npu());
    println!("Supports tensor ops: {}", profile.supports_tensor_ops());
    println!("Max vector width: {} bits", profile.max_vector_width());
    println!();

    // CPU Topology
    println!("--- CPU Topology ---");
    let topology = CpuTopology::detect();
    println!("Physical cores: {}", topology.physical_cores);
    println!("Logical cores: {}", topology.logical_cores);
    println!("Sockets: {}", topology.sockets);
    println!("NUMA nodes: {}", topology.numa_nodes);
    println!("Hyperthreading: {}", topology.hyperthreading);
    println!("Threads per core: {}", topology.threads_per_core());
    println!("Cores per socket: {}", topology.cores_per_socket());
    println!();

    // Memory information
    println!("--- Memory Information ---");
    let mem_info = MemoryInfo::detect();
    println!("Total memory: {} MB", mem_info.total_mb());
    println!("Total memory: {} GB", mem_info.total_gb());
    println!("Available memory: {} bytes", mem_info.available);
    println!("Page size: {} bytes", mem_info.page_size);
    println!();

    // Platform detection
    println!("--- Platform Detection ---");
    let platform = platform::detect_platform();
    println!("Platform: {}", platform);
    println!();

    // Performance note
    println!("--- Performance Note ---");
    println!("Hardware detection is cached after first call.");
    println!("Subsequent calls use cached values for better performance.");
}
