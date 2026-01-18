//! Platform-Specific Features Example
//!
//! This example demonstrates platform-specific capability detection
//! for Raspberry Pi, STM32, and ESP32 platforms.
//!
//! Run with: cargo run --example platform_specific

extern crate alloc;

use mielin_hal::platform::{
    detect_capabilities, detect_platform, Esp32Capabilities, Platform, PlatformCapabilities,
    RaspberryPiCapabilities, Stm32Capabilities,
};

#[cfg(not(target_os = "none"))]
use std::println;

fn main() {
    println!("=== Platform-Specific Features Detection ===\n");

    let platform = detect_platform();
    println!("Detected Platform: {}\n", platform);

    match platform {
        Platform::RaspberryPi(model) => {
            println!("Raspberry Pi Model: {}", model);
            show_raspberry_pi_capabilities();
        }
        Platform::Stm32(family) => {
            println!("STM32 Family: {}", family);
            show_stm32_capabilities();
        }
        Platform::Esp32(variant) => {
            println!("ESP32 Variant: {}", variant);
            show_esp32_capabilities();
        }
        Platform::BeagleBone(model) => {
            println!("BeagleBone Model: {}", model);
        }
        Platform::GenericX86 => {
            println!("Generic x86_64 platform");
            println!("No platform-specific features available.");
        }
        Platform::GenericArm => {
            println!("Generic ARM platform");
            println!("May be a Raspberry Pi - checking capabilities...");
            show_raspberry_pi_capabilities();
        }
        Platform::GenericRiscV => {
            println!("Generic RISC-V platform");
        }
        Platform::Jetson(model) => {
            println!("NVIDIA Jetson Model: {}", model);
            println!("CUDA/TensorRT acceleration available");
        }
        Platform::Unknown => {
            println!("Unknown platform");
        }
    }

    // Detect all capabilities
    println!("\n--- All Platform Capabilities ---");
    let caps = detect_capabilities();
    match caps {
        PlatformCapabilities::RaspberryPi(pi_caps) => {
            println!("Raspberry Pi capabilities detected");
            print_raspberry_pi_details(&pi_caps);
        }
        PlatformCapabilities::Stm32(stm32_caps) => {
            println!("STM32 capabilities detected");
            print_stm32_details(&stm32_caps);
        }
        PlatformCapabilities::Esp32(esp_caps) => {
            println!("ESP32 capabilities detected");
            print_esp32_details(&esp_caps);
        }
        PlatformCapabilities::Generic => {
            println!("Generic platform - no specific capabilities");
        }
    }
}

fn show_raspberry_pi_capabilities() {
    let caps = RaspberryPiCapabilities::detect();
    print_raspberry_pi_details(&caps);
}

fn print_raspberry_pi_details(caps: &RaspberryPiCapabilities) {
    println!("\n--- Raspberry Pi Capabilities ---");
    println!("GPIO pins: {}", caps.gpio_pins);
    println!("Has VideoCore GPU: {}", caps.has_videocore);
    println!("VideoCore generation: {}", caps.videocore_generation);
    println!("H.264 encoder: {}", caps.has_h264_encoder);
    println!("H.264 decoder: {}", caps.has_h264_decoder);
    println!("HEVC support: {}", caps.has_hevc);
    println!("CSI lanes: {}", caps.csi_lanes);
    println!("DSI lanes: {}", caps.dsi_lanes);
    println!("WiFi: {}", caps.has_wifi);
    println!("Bluetooth: {}", caps.has_bluetooth);
    println!("Ethernet: {}", caps.has_ethernet);
    println!("Ethernet speed: {} Mbps", caps.ethernet_speed_mbps);
    println!("Has wireless: {}", caps.has_wireless());
    println!("Is Pi 5: {}", caps.is_pi5());
}

fn show_stm32_capabilities() {
    let caps = Stm32Capabilities::detect();
    print_stm32_details(&caps);
}

fn print_stm32_details(caps: &Stm32Capabilities) {
    println!("\n--- STM32 Capabilities ---");
    println!("Flash size: {} KB", caps.flash_size_kb);
    println!("RAM size: {} KB", caps.ram_size_kb);
    println!("USART count: {}", caps.usart_count);
    println!("SPI count: {}", caps.spi_count);
    println!("I2C count: {}", caps.i2c_count);
    println!("ADC channels: {}", caps.adc_channels);
    println!("DAC channels: {}", caps.dac_channels);
    println!("Timers: {}", caps.timer_count);
    println!("Has DMA: {}", caps.has_dma);
    println!("DMA channels: {}", caps.dma_channels);
    println!("Has USB: {}", caps.has_usb);
    println!("Has CAN: {}", caps.has_can);
    println!("Has Ethernet: {}", caps.has_ethernet);
    println!("Has Crypto: {}", caps.has_crypto);
}

fn show_esp32_capabilities() {
    let caps = Esp32Capabilities::detect();
    print_esp32_details(&caps);
}

fn print_esp32_details(caps: &Esp32Capabilities) {
    println!("\n--- ESP32 Capabilities ---");
    println!(
        "WiFi standard: 802.11{}",
        if caps.wifi_standard == 6 {
            "ax"
        } else if caps.wifi_standard == 5 {
            "ac"
        } else {
            "n"
        }
    );
    println!("Bluetooth Classic: {}", caps.has_bt_classic);
    println!("BLE: {}", caps.has_ble);
    println!("BLE version: {}.{}", caps.ble_version.0, caps.ble_version.1);
    println!("Thread/Zigbee: {}", caps.has_thread_zigbee);
    println!("CPU cores: {}", caps.cpu_cores);
    println!("Has PSRAM: {}", caps.has_psram);
    println!("PSRAM size: {} MB", caps.psram_size_mb);
    println!("Flash size: {} MB", caps.flash_size_mb);
    println!("Secure boot: {}", caps.has_secure_boot);
    println!("Flash encryption: {}", caps.has_flash_encryption);
    println!("Has full wireless: {}", caps.has_full_wireless());
}
