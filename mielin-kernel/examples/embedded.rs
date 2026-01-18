//! Embedded System Configuration Example
//!
//! Demonstrates how to configure MielinOS for embedded systems with
//! constrained resources (flash, RAM).

use mielin_kernel::embedded::{EmbeddedConfig, MemoryLayout};

fn main() {
    println!("=== MielinOS Embedded Configuration Examples ===\n");

    // Example 1: Cortex-M0 (ultra-minimal)
    println!("1. Cortex-M0 Configuration (STM32F0)");
    let m0_config = EmbeddedConfig::cortex_m0();
    println!("   Flash: {} KB", m0_config.flash_size() / 1024);
    println!("   RAM: {} KB", m0_config.ram_size() / 1024);
    println!("   XIP: {}", m0_config.xip_enabled());
    println!(
        "   Est. Code Size: {} KB",
        m0_config.estimated_code_size() / 1024
    );
    println!(
        "   Est. RAM Usage: {} KB",
        m0_config.estimated_ram_usage() / 1024
    );
    println!(
        "   Validation: {}",
        if m0_config.validate().is_ok() {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!();

    // Example 2: Cortex-M4 (moderate)
    println!("2. Cortex-M4 Configuration (STM32F4)");
    let m4_config = EmbeddedConfig::cortex_m4();
    println!("   Flash: {} KB", m4_config.flash_size() / 1024);
    println!("   RAM: {} KB", m4_config.ram_size() / 1024);
    println!("   XIP: {}", m4_config.xip_enabled());
    println!(
        "   Est. Code Size: {} KB",
        m4_config.estimated_code_size() / 1024
    );
    println!(
        "   Est. RAM Usage: {} KB",
        m4_config.estimated_ram_usage() / 1024
    );
    println!(
        "   Validation: {}",
        if m4_config.validate().is_ok() {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!();

    // Example 3: nRF52 (BLE SoC)
    println!("3. nRF52 Configuration (Nordic BLE)");
    let nrf52_config = EmbeddedConfig::nrf52();
    println!("   Flash: {} KB", nrf52_config.flash_size() / 1024);
    println!("   RAM: {} KB", nrf52_config.ram_size() / 1024);
    println!("   XIP: {}", nrf52_config.xip_enabled());
    println!(
        "   Est. Code Size: {} KB",
        nrf52_config.estimated_code_size() / 1024
    );
    println!(
        "   Est. RAM Usage: {} KB",
        nrf52_config.estimated_ram_usage() / 1024
    );
    println!(
        "   Validation: {}",
        if nrf52_config.validate().is_ok() {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!();

    // Example 4: ESP32-C3 (RISC-V)
    println!("4. ESP32-C3 Configuration (RISC-V WiFi)");
    let esp32_config = EmbeddedConfig::esp32c3();
    println!("   Flash: {} KB", esp32_config.flash_size() / 1024);
    println!("   RAM: {} KB", esp32_config.ram_size() / 1024);
    println!("   XIP: {}", esp32_config.xip_enabled());
    println!(
        "   Est. Code Size: {} KB",
        esp32_config.estimated_code_size() / 1024
    );
    println!(
        "   Est. RAM Usage: {} KB",
        esp32_config.estimated_ram_usage() / 1024
    );
    println!(
        "   Validation: {}",
        if esp32_config.validate().is_ok() {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!();

    // Example 5: Custom configuration
    println!("5. Custom Configuration (64KB Flash, 16KB RAM)");
    match EmbeddedConfig::builder()
        .flash_size(64 * 1024)
        .ram_size(16 * 1024)
        .max_tasks(8)
        .max_pages(32)
        .xip_enabled(true)
        .build()
    {
        Ok(custom_config) => {
            println!("   Flash: {} KB", custom_config.flash_size() / 1024);
            println!("   RAM: {} KB", custom_config.ram_size() / 1024);
            println!(
                "   Max Tasks: {}",
                custom_config.kernel_config().scheduler.max_tasks
            );
            println!(
                "   Max Pages: {}",
                custom_config.kernel_config().memory.max_pages
            );
            println!(
                "   Est. Code Size: {} KB",
                custom_config.estimated_code_size() / 1024
            );
            println!(
                "   Est. RAM Usage: {} KB",
                custom_config.estimated_ram_usage() / 1024
            );
            println!("   Status: Configuration OK");
        }
        Err(e) => {
            println!("   Error: {}", e);
        }
    }
    println!();

    // Example 6: Memory layouts
    println!("6. Memory Layouts");
    println!("\n   STM32F0 Layout:");
    let layout_stm32f0 = MemoryLayout::stm32f0();
    println!(
        "     Flash: {:#010x} - {:#010x} ({} KB)",
        layout_stm32f0.flash_base,
        layout_stm32f0.flash_base + layout_stm32f0.flash_size,
        layout_stm32f0.flash_size / 1024
    );
    println!(
        "     RAM:   {:#010x} - {:#010x} ({} KB)",
        layout_stm32f0.ram_base,
        layout_stm32f0.ram_base + layout_stm32f0.ram_size,
        layout_stm32f0.ram_size / 1024
    );
    println!(
        "     XIP:   {:#010x} - {:#010x} ({} KB)",
        layout_stm32f0.xip_base,
        layout_stm32f0.xip_base + layout_stm32f0.xip_size,
        layout_stm32f0.xip_size / 1024
    );
    println!("     Stack: {} KB", layout_stm32f0.stack_size / 1024);
    println!("     Heap:  {} KB", layout_stm32f0.heap_size / 1024);

    println!("\n   nRF52 Layout:");
    let layout_nrf52 = MemoryLayout::nrf52();
    println!(
        "     Flash: {:#010x} - {:#010x} ({} KB)",
        layout_nrf52.flash_base,
        layout_nrf52.flash_base + layout_nrf52.flash_size,
        layout_nrf52.flash_size / 1024
    );
    println!(
        "     RAM:   {:#010x} - {:#010x} ({} KB)",
        layout_nrf52.ram_base,
        layout_nrf52.ram_base + layout_nrf52.ram_size,
        layout_nrf52.ram_size / 1024
    );
    println!(
        "     XIP:   {:#010x} - {:#010x} ({} KB)",
        layout_nrf52.xip_base,
        layout_nrf52.xip_base + layout_nrf52.xip_size,
        layout_nrf52.xip_size / 1024
    );

    println!("\n=== Summary ===");
    println!("MielinOS can be configured for various embedded platforms:");
    println!("- Cortex-M0: 32KB flash, 8KB RAM (ultra-minimal)");
    println!("- Cortex-M4: 512KB flash, 128KB RAM (moderate)");
    println!("- nRF52: 512KB flash, 64KB RAM (BLE capable)");
    println!("- ESP32-C3: 384KB flash, 400KB RAM (WiFi capable)");
    println!("- Custom: Builder pattern for any configuration");
}
