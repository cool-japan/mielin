//! Platform-Specific Hardware Detection
//!
//! This module provides detection and capabilities for specific hardware platforms:
//! - Raspberry Pi (all generations)
//! - STM32 microcontrollers
//! - ESP32 family
//! - BeagleBone boards
//!
//! # Platform Detection
//!
//! Platform detection is done through multiple mechanisms:
//! - Device tree parsing (Linux-based systems)
//! - CPUID/processor identification
//! - Memory-mapped peripheral detection (bare metal)
//! - Board-specific register signatures

use core::fmt;

/// Supported hardware platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Unknown or generic platform
    Unknown,

    /// Raspberry Pi series
    RaspberryPi(RaspberryPiModel),

    /// STM32 microcontroller family
    Stm32(Stm32Family),

    /// ESP32 family
    Esp32(Esp32Variant),

    /// BeagleBone series
    BeagleBone(BeagleBoneModel),

    /// NVIDIA Jetson series
    Jetson(JetsonModel),

    /// Generic x86_64 PC
    GenericX86,

    /// Generic ARM server (Graviton, etc.)
    GenericArm,

    /// Generic RISC-V
    GenericRiscV,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Unknown => write!(f, "Unknown Platform"),
            Platform::RaspberryPi(model) => write!(f, "Raspberry Pi {}", model),
            Platform::Stm32(family) => write!(f, "STM32{}", family),
            Platform::Esp32(variant) => write!(f, "ESP32 {}", variant),
            Platform::BeagleBone(model) => write!(f, "BeagleBone {}", model),
            Platform::Jetson(model) => write!(f, "NVIDIA Jetson {}", model),
            Platform::GenericX86 => write!(f, "Generic x86_64"),
            Platform::GenericArm => write!(f, "Generic ARM"),
            Platform::GenericRiscV => write!(f, "Generic RISC-V"),
        }
    }
}

/// Raspberry Pi models
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaspberryPiModel {
    /// Raspberry Pi 1 Model B/B+
    Pi1,
    /// Raspberry Pi 2
    Pi2,
    /// Raspberry Pi 3
    Pi3,
    /// Raspberry Pi 4
    Pi4,
    /// Raspberry Pi 5
    Pi5,
    /// Raspberry Pi Zero/Zero W
    PiZero,
    /// Raspberry Pi 400
    Pi400,
    /// Raspberry Pi Compute Module
    ComputeModule(u8),
}

impl fmt::Display for RaspberryPiModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RaspberryPiModel::Pi1 => write!(f, "1"),
            RaspberryPiModel::Pi2 => write!(f, "2"),
            RaspberryPiModel::Pi3 => write!(f, "3"),
            RaspberryPiModel::Pi4 => write!(f, "4"),
            RaspberryPiModel::Pi5 => write!(f, "5"),
            RaspberryPiModel::PiZero => write!(f, "Zero"),
            RaspberryPiModel::Pi400 => write!(f, "400"),
            RaspberryPiModel::ComputeModule(gen) => write!(f, "CM{}", gen),
        }
    }
}

/// STM32 microcontroller families
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stm32Family {
    /// STM32F0 (Cortex-M0)
    F0,
    /// STM32F1 (Cortex-M3)
    F1,
    /// STM32F2 (Cortex-M3)
    F2,
    /// STM32F3 (Cortex-M4)
    F3,
    /// STM32F4 (Cortex-M4)
    F4,
    /// STM32F7 (Cortex-M7)
    F7,
    /// STM32H7 (Cortex-M7)
    H7,
    /// STM32L0 (Cortex-M0+, low power)
    L0,
    /// STM32L1 (Cortex-M3, low power)
    L1,
    /// STM32L4 (Cortex-M4, low power)
    L4,
    /// STM32L5 (Cortex-M33, low power)
    L5,
    /// STM32G0 (Cortex-M0+)
    G0,
    /// STM32G4 (Cortex-M4)
    G4,
    /// STM32U5 (Cortex-M33)
    U5,
    /// STM32WB (Wireless, Cortex-M4)
    WB,
    /// STM32WL (LoRa, Cortex-M4)
    WL,
}

impl fmt::Display for Stm32Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// ESP32 variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Esp32Variant {
    /// Original ESP32 (dual-core Xtensa)
    Esp32,
    /// ESP32-S2 (single-core Xtensa)
    Esp32S2,
    /// ESP32-S3 (dual-core Xtensa)
    Esp32S3,
    /// ESP32-C3 (single-core RISC-V)
    Esp32C3,
    /// ESP32-C6 (RISC-V with WiFi 6)
    Esp32C6,
    /// ESP32-H2 (RISC-V, Thread/Zigbee)
    Esp32H2,
}

impl fmt::Display for Esp32Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Esp32Variant::Esp32 => write!(f, "Original"),
            Esp32Variant::Esp32S2 => write!(f, "S2"),
            Esp32Variant::Esp32S3 => write!(f, "S3"),
            Esp32Variant::Esp32C3 => write!(f, "C3"),
            Esp32Variant::Esp32C6 => write!(f, "C6"),
            Esp32Variant::Esp32H2 => write!(f, "H2"),
        }
    }
}

/// BeagleBone models
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeagleBoneModel {
    /// BeagleBone Black
    Black,
    /// BeagleBone Green
    Green,
    /// BeagleBone AI
    Ai,
    /// BeagleBone AI-64
    Ai64,
    /// PocketBeagle
    Pocket,
}

impl fmt::Display for BeagleBoneModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BeagleBoneModel::Black => write!(f, "Black"),
            BeagleBoneModel::Green => write!(f, "Green"),
            BeagleBoneModel::Ai => write!(f, "AI"),
            BeagleBoneModel::Ai64 => write!(f, "AI-64"),
            BeagleBoneModel::Pocket => write!(f, "Pocket"),
        }
    }
}

/// NVIDIA Jetson models
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JetsonModel {
    /// Jetson AGX Orin
    AgxOrin,
    /// Jetson Xavier NX
    XavierNx,
    /// Jetson Orin Nano
    OrinNano,
    /// Jetson Nano
    Nano,
    /// Jetson TX2
    Tx2,
}

impl fmt::Display for JetsonModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JetsonModel::AgxOrin => write!(f, "AGX Orin"),
            JetsonModel::XavierNx => write!(f, "Xavier NX"),
            JetsonModel::OrinNano => write!(f, "Orin Nano"),
            JetsonModel::Nano => write!(f, "Nano"),
            JetsonModel::Tx2 => write!(f, "TX2"),
        }
    }
}

/// Raspberry Pi capabilities
#[derive(Debug, Clone, Copy, Default)]
pub struct RaspberryPiCapabilities {
    /// Model information
    pub model: Option<RaspberryPiModel>,
    /// GPIO pin count
    pub gpio_pins: u8,
    /// Has VideoCore GPU
    pub has_videocore: bool,
    /// VideoCore generation (4, 6, 7)
    pub videocore_generation: u8,
    /// Has hardware video encoding
    pub has_h264_encoder: bool,
    /// Has hardware video decoding
    pub has_h264_decoder: bool,
    /// Has HEVC support
    pub has_hevc: bool,
    /// Camera Serial Interface lanes
    pub csi_lanes: u8,
    /// Display Serial Interface lanes
    pub dsi_lanes: u8,
    /// Has WiFi
    pub has_wifi: bool,
    /// Has Bluetooth
    pub has_bluetooth: bool,
    /// Has Ethernet
    pub has_ethernet: bool,
    /// Ethernet speed in Mbps (100, 1000)
    pub ethernet_speed_mbps: u16,
}

impl RaspberryPiCapabilities {
    /// Detect Raspberry Pi capabilities
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
        {
            detect_raspberry_pi_linux()
        }

        #[cfg(not(all(target_arch = "aarch64", target_os = "linux")))]
        {
            Self::default()
        }
    }

    /// Check if this is a Raspberry Pi 5
    pub fn is_pi5(&self) -> bool {
        matches!(self.model, Some(RaspberryPiModel::Pi5))
    }

    /// Check if this platform has wireless capabilities
    pub fn has_wireless(&self) -> bool {
        self.has_wifi || self.has_bluetooth
    }
}

/// STM32 peripheral capabilities
#[derive(Debug, Clone, Copy, Default)]
pub struct Stm32Capabilities {
    /// STM32 family
    pub family: Option<Stm32Family>,
    /// Flash size in KB
    pub flash_size_kb: u16,
    /// RAM size in KB
    pub ram_size_kb: u16,
    /// Number of USART peripherals
    pub usart_count: u8,
    /// Number of SPI peripherals
    pub spi_count: u8,
    /// Number of I2C peripherals
    pub i2c_count: u8,
    /// Number of ADC channels
    pub adc_channels: u8,
    /// Number of DAC channels
    pub dac_channels: u8,
    /// Number of timers
    pub timer_count: u8,
    /// Has DMA
    pub has_dma: bool,
    /// Number of DMA channels
    pub dma_channels: u8,
    /// Has USB
    pub has_usb: bool,
    /// Has CAN
    pub has_can: bool,
    /// Has Ethernet
    pub has_ethernet: bool,
    /// Has Crypto accelerator
    pub has_crypto: bool,
}

impl Stm32Capabilities {
    /// Detect STM32 capabilities
    pub fn detect() -> Self {
        #[cfg(all(target_arch = "arm", target_os = "none"))]
        {
            detect_stm32_baremetal()
        }

        #[cfg(not(all(target_arch = "arm", target_os = "none")))]
        {
            Self::default()
        }
    }
}

/// ESP32 wireless capabilities
#[derive(Debug, Clone, Copy, Default)]
pub struct Esp32Capabilities {
    /// ESP32 variant
    pub variant: Option<Esp32Variant>,
    /// WiFi standard (4, 5, 6)
    pub wifi_standard: u8,
    /// Has Bluetooth Classic
    pub has_bt_classic: bool,
    /// Has Bluetooth Low Energy
    pub has_ble: bool,
    /// BLE version (4.2, 5.0, 5.3)
    pub ble_version: (u8, u8),
    /// Has Thread/Zigbee (802.15.4)
    pub has_thread_zigbee: bool,
    /// Number of CPU cores
    pub cpu_cores: u8,
    /// Has PSRAM
    pub has_psram: bool,
    /// PSRAM size in MB
    pub psram_size_mb: u8,
    /// Flash size in MB
    pub flash_size_mb: u8,
    /// Has secure boot
    pub has_secure_boot: bool,
    /// Has flash encryption
    pub has_flash_encryption: bool,
}

impl Esp32Capabilities {
    /// Detect ESP32 capabilities
    pub fn detect() -> Self {
        #[cfg(target_os = "espidf")]
        {
            detect_esp32_espidf()
        }

        #[cfg(not(target_os = "espidf"))]
        {
            Self::default()
        }
    }

    /// Check if this ESP32 has full wireless stack
    pub fn has_full_wireless(&self) -> bool {
        self.wifi_standard > 0 && (self.has_bt_classic || self.has_ble)
    }
}

/// Platform capabilities union
#[derive(Debug, Clone, Copy)]
pub enum PlatformCapabilities {
    /// Raspberry Pi capabilities
    RaspberryPi(RaspberryPiCapabilities),
    /// STM32 capabilities
    Stm32(Stm32Capabilities),
    /// ESP32 capabilities
    Esp32(Esp32Capabilities),
    /// Generic platform (no specific capabilities)
    Generic,
}

/// Detect the current platform
pub fn detect_platform() -> Platform {
    #[cfg(target_arch = "x86_64")]
    {
        return Platform::GenericX86;
    }

    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    {
        return detect_platform_aarch64_linux();
    }

    #[cfg(all(target_arch = "aarch64", not(target_os = "linux")))]
    {
        return Platform::GenericArm;
    }

    #[cfg(all(target_arch = "arm", target_os = "none"))]
    {
        return detect_platform_cortex_m();
    }

    #[cfg(target_os = "espidf")]
    {
        return detect_platform_esp32();
    }

    #[cfg(all(target_arch = "riscv64", not(target_os = "espidf")))]
    {
        return Platform::GenericRiscV;
    }

    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "riscv64"
    )))]
    {
        return Platform::Unknown;
    }

    #[allow(unreachable_code)]
    {
        Platform::Unknown
    }
}

/// Detect platform capabilities
pub fn detect_capabilities() -> PlatformCapabilities {
    let platform = detect_platform();

    match platform {
        Platform::RaspberryPi(_) => {
            PlatformCapabilities::RaspberryPi(RaspberryPiCapabilities::detect())
        }
        Platform::Stm32(_) => PlatformCapabilities::Stm32(Stm32Capabilities::detect()),
        Platform::Esp32(_) => PlatformCapabilities::Esp32(Esp32Capabilities::detect()),
        _ => PlatformCapabilities::Generic,
    }
}

// Platform-specific detection implementations

// Helper functions for Linux file reading (no_std compatible)

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn read_file_linux(path: &str) -> Option<alloc::vec::Vec<u8>> {
    use alloc::vec::Vec;

    // Convert path to null-terminated C string
    let mut path_bytes = Vec::with_capacity(path.len() + 1);
    path_bytes.extend_from_slice(path.as_bytes());
    path_bytes.push(0); // null terminator

    unsafe {
        let fd: isize;

        // Open file - syscall number differs by architecture
        #[cfg(target_arch = "x86_64")]
        {
            core::arch::asm!(
                "syscall",
                in("rax") 2_usize,  // __NR_open
                in("rdi") path_bytes.as_ptr(),
                in("rsi") 0_usize,  // O_RDONLY
                lateout("rax") fd,
                out("rcx") _,
                out("r11") _,
                options(nostack)
            );
        }

        #[cfg(target_arch = "aarch64")]
        {
            core::arch::asm!(
                "svc #0",
                in("x8") 56_usize,  // __NR_openat on aarch64
                in("x0") -100_isize, // AT_FDCWD
                in("x1") path_bytes.as_ptr(),
                in("x2") 0_usize,   // O_RDONLY
                lateout("x0") fd,
                options(nostack)
            );
        }

        // Check if open succeeded
        if fd < 0 {
            return None;
        }

        // Read file contents
        let mut buffer = Vec::with_capacity(512);
        buffer.resize(512, 0);

        let bytes_read: isize;

        #[cfg(target_arch = "x86_64")]
        {
            core::arch::asm!(
                "syscall",
                in("rax") 0_usize,  // __NR_read
                in("rdi") fd,
                in("rsi") buffer.as_mut_ptr(),
                in("rdx") buffer.len(),
                lateout("rax") bytes_read,
                out("rcx") _,
                out("r11") _,
                options(nostack)
            );
        }

        #[cfg(target_arch = "aarch64")]
        {
            core::arch::asm!(
                "svc #0",
                in("x8") 63_usize,  // __NR_read on aarch64
                in("x0") fd,
                in("x1") buffer.as_mut_ptr(),
                in("x2") buffer.len(),
                lateout("x0") bytes_read,
                options(nostack)
            );
        }

        // Close file
        #[cfg(target_arch = "x86_64")]
        {
            core::arch::asm!(
                "syscall",
                in("rax") 3_usize,  // __NR_close
                in("rdi") fd,
                lateout("rax") _,
                out("rcx") _,
                out("r11") _,
                options(nostack)
            );
        }

        #[cfg(target_arch = "aarch64")]
        {
            core::arch::asm!(
                "svc #0",
                in("x8") 57_usize,  // __NR_close on aarch64
                in("x0") fd,
                lateout("x0") _,
                options(nostack)
            );
        }

        if bytes_read > 0 {
            buffer.truncate(bytes_read as usize);
            Some(buffer)
        } else {
            None
        }
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn read_device_tree_model() -> Option<alloc::string::String> {
    // Try multiple device tree paths
    let paths = [
        "/sys/firmware/devicetree/base/model",
        "/proc/device-tree/model",
    ];

    for path in &paths {
        if let Some(data) = read_file_linux(path) {
            // Device tree strings are null-terminated
            let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
            if let Ok(s) = core::str::from_utf8(&data[..end]) {
                return Some(alloc::string::String::from(s.trim()));
            }
        }
    }

    None
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn to_lowercase_limited(s: &str) -> alloc::string::String {
    use alloc::string::String;
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        result.push(c.to_ascii_lowercase());
    }
    result
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn contains_str(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn parse_raspberry_pi_model(model: &str) -> Platform {
    let model_lower = to_lowercase_limited(model);

    // Parse model string like "Raspberry Pi 4 Model B" or "Raspberry Pi 5"
    if contains_str(&model_lower, "pi 5") || contains_str(&model_lower, "pi5") {
        Platform::RaspberryPi(RaspberryPiModel::Pi5)
    } else if contains_str(&model_lower, "pi 4") || contains_str(&model_lower, "pi4") {
        Platform::RaspberryPi(RaspberryPiModel::Pi4)
    } else if contains_str(&model_lower, "pi 400") {
        Platform::RaspberryPi(RaspberryPiModel::Pi400)
    } else if contains_str(&model_lower, "pi 3") || contains_str(&model_lower, "pi3") {
        Platform::RaspberryPi(RaspberryPiModel::Pi3)
    } else if contains_str(&model_lower, "pi 2") || contains_str(&model_lower, "pi2") {
        Platform::RaspberryPi(RaspberryPiModel::Pi2)
    } else if contains_str(&model_lower, "pi zero") || contains_str(&model_lower, "pi 0") {
        Platform::RaspberryPi(RaspberryPiModel::PiZero)
    } else if contains_str(&model_lower, "compute module") {
        // Extract generation number if possible
        let gen = if contains_str(&model_lower, "cm4") || contains_str(&model_lower, "module 4") {
            4
        } else if contains_str(&model_lower, "cm3") || contains_str(&model_lower, "module 3") {
            3
        } else {
            1
        };
        Platform::RaspberryPi(RaspberryPiModel::ComputeModule(gen))
    } else if contains_str(&model_lower, "pi 1") || contains_str(&model_lower, "pi1") {
        Platform::RaspberryPi(RaspberryPiModel::Pi1)
    } else {
        // Unknown Raspberry Pi model
        Platform::RaspberryPi(RaspberryPiModel::Pi1) // Default to Pi1 for unknown
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn parse_beaglebone_model(model: &str) -> Platform {
    let model_lower = to_lowercase_limited(model);

    if contains_str(&model_lower, "ai-64") {
        Platform::BeagleBone(BeagleBoneModel::Ai64)
    } else if contains_str(&model_lower, "ai") {
        Platform::BeagleBone(BeagleBoneModel::Ai)
    } else if contains_str(&model_lower, "green") {
        Platform::BeagleBone(BeagleBoneModel::Green)
    } else if contains_str(&model_lower, "pocket") {
        Platform::BeagleBone(BeagleBoneModel::Pocket)
    } else if contains_str(&model_lower, "black") {
        Platform::BeagleBone(BeagleBoneModel::Black)
    } else {
        // Default to Black for unknown BeagleBone
        Platform::BeagleBone(BeagleBoneModel::Black)
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn detect_platform_aarch64_linux() -> Platform {
    // Try to read device tree model string
    if let Some(model) = read_device_tree_model() {
        let model_lower = to_lowercase_limited(&model);

        // Check for Raspberry Pi
        if contains_str(&model_lower, "raspberry pi") {
            return parse_raspberry_pi_model(&model);
        }

        // Check for BeagleBone
        if contains_str(&model_lower, "beaglebone") || contains_str(&model_lower, "ti am") {
            return parse_beaglebone_model(&model);
        }
    }

    Platform::GenericArm
}

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
fn detect_raspberry_pi_linux() -> RaspberryPiCapabilities {
    let mut caps = RaspberryPiCapabilities::default();

    // Get model from device tree
    if let Some(model_str) = read_device_tree_model() {
        let platform = parse_raspberry_pi_model(&model_str);
        if let Platform::RaspberryPi(model) = platform {
            caps.model = Some(model);

            // Set capabilities based on model
            match model {
                RaspberryPiModel::Pi5 => {
                    caps.gpio_pins = 40;
                    caps.has_videocore = true;
                    caps.videocore_generation = 7;
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = true;
                    caps.csi_lanes = 4;
                    caps.dsi_lanes = 4;
                    caps.has_wifi = true;
                    caps.has_bluetooth = true;
                    caps.has_ethernet = true;
                    caps.ethernet_speed_mbps = 1000;
                }
                RaspberryPiModel::Pi4 | RaspberryPiModel::Pi400 => {
                    caps.gpio_pins = 40;
                    caps.has_videocore = true;
                    caps.videocore_generation = 6;
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = true;
                    caps.csi_lanes = 2;
                    caps.dsi_lanes = 2;
                    caps.has_wifi = true;
                    caps.has_bluetooth = true;
                    caps.has_ethernet = true;
                    caps.ethernet_speed_mbps = 1000;
                }
                RaspberryPiModel::Pi3 => {
                    caps.gpio_pins = 40;
                    caps.has_videocore = true;
                    caps.videocore_generation = 4;
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = false;
                    caps.csi_lanes = 2;
                    caps.dsi_lanes = 2;
                    caps.has_wifi = true;
                    caps.has_bluetooth = true;
                    caps.has_ethernet = true;
                    caps.ethernet_speed_mbps = 100;
                }
                RaspberryPiModel::Pi2 => {
                    caps.gpio_pins = 40;
                    caps.has_videocore = true;
                    caps.videocore_generation = 4;
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = false;
                    caps.csi_lanes = 2;
                    caps.dsi_lanes = 2;
                    caps.has_wifi = false;
                    caps.has_bluetooth = false;
                    caps.has_ethernet = true;
                    caps.ethernet_speed_mbps = 100;
                }
                RaspberryPiModel::Pi1 => {
                    caps.gpio_pins = 26; // Some models have 40
                    caps.has_videocore = true;
                    caps.videocore_generation = 4;
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = false;
                    caps.csi_lanes = 2;
                    caps.dsi_lanes = 2;
                    caps.has_wifi = false;
                    caps.has_bluetooth = false;
                    caps.has_ethernet = true;
                    caps.ethernet_speed_mbps = 100;
                }
                RaspberryPiModel::PiZero => {
                    caps.gpio_pins = 40;
                    caps.has_videocore = true;
                    caps.videocore_generation = 4;
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = false;
                    caps.csi_lanes = 2;
                    caps.dsi_lanes = 0;
                    caps.has_wifi = model_str.to_lowercase().contains("w"); // Pi Zero W
                    caps.has_bluetooth = model_str.to_lowercase().contains("w");
                    caps.has_ethernet = false;
                    caps.ethernet_speed_mbps = 0;
                }
                RaspberryPiModel::ComputeModule(gen) => {
                    caps.gpio_pins = if gen >= 3 { 40 } else { 26 };
                    caps.has_videocore = true;
                    caps.videocore_generation = if gen >= 4 { 6 } else { 4 };
                    caps.has_h264_encoder = true;
                    caps.has_h264_decoder = true;
                    caps.has_hevc = gen >= 4;
                    caps.csi_lanes = 2;
                    caps.dsi_lanes = 2;
                    caps.has_wifi = false; // CMs typically don't have built-in WiFi
                    caps.has_bluetooth = false;
                    caps.has_ethernet = false; // Depends on carrier board
                    caps.ethernet_speed_mbps = 0;
                }
            }
        }
    }

    caps
}

#[cfg(all(target_arch = "arm", target_os = "none"))]
fn detect_platform_cortex_m() -> Platform {
    // Try to detect STM32 by reading device ID register
    // DBGMCU_IDCODE is at 0xE0042000 for most STM32s
    #[cfg(any(
        feature = "stm32f0",
        feature = "stm32f1",
        feature = "stm32f2",
        feature = "stm32f3",
        feature = "stm32f4",
        feature = "stm32f7"
    ))]
    {
        detect_stm32()
    }

    #[cfg(not(any(
        feature = "stm32f0",
        feature = "stm32f1",
        feature = "stm32f2",
        feature = "stm32f3",
        feature = "stm32f4",
        feature = "stm32f7"
    )))]
    {
        Platform::Unknown
    }
}

#[cfg(all(target_arch = "arm", target_os = "none"))]
fn detect_stm32_baremetal() -> Stm32Capabilities {
    let mut caps = Stm32Capabilities::default();

    // Read DBGMCU_IDCODE register
    // Note: Address varies by STM32 family:
    // - STM32F0/F1/F2/F3/F4/F7/L0/L1/L4: 0xE0042000
    // - STM32H7/G0/G4/L5/U5/WB/WL: 0xE0044000

    const DBGMCU_IDCODE_ADDR_OLD: usize = 0xE0042000;
    const DBGMCU_IDCODE_ADDR_NEW: usize = 0xE0044000;

    let device_id = unsafe {
        // Try old address first (most common)
        let id_old = core::ptr::read_volatile(DBGMCU_IDCODE_ADDR_OLD as *const u32);

        // If the ID looks invalid (all 0s or all 1s), try new address
        if id_old == 0 || id_old == 0xFFFFFFFF {
            core::ptr::read_volatile(DBGMCU_IDCODE_ADDR_NEW as *const u32)
        } else {
            id_old
        }
    };

    // Extract DEV_ID (lower 12 bits) and REV_ID (bits 16-31)
    let dev_id = device_id & 0xFFF;

    // Decode device ID to determine family and capabilities
    // Note: These are representative values; full list is extensive
    match dev_id {
        // STM32F0 family
        0x440 | 0x442 | 0x444 | 0x445 | 0x448 => {
            caps.family = Some(Stm32Family::F0);
            caps.flash_size_kb = 64; // Typical, varies by model
            caps.ram_size_kb = 8;
            caps.usart_count = 2;
            caps.spi_count = 2;
            caps.i2c_count = 2;
            caps.adc_channels = 12;
            caps.dac_channels = 0;
            caps.timer_count = 6;
            caps.has_dma = true;
            caps.dma_channels = 5;
            caps.has_usb = true;
            caps.has_can = false;
            caps.has_ethernet = false;
            caps.has_crypto = false;
        }

        // STM32F1 family
        0x410 | 0x412 | 0x414 | 0x418 | 0x420 | 0x428 | 0x430 => {
            caps.family = Some(Stm32Family::F1);
            caps.flash_size_kb = 128; // Typical
            caps.ram_size_kb = 20;
            caps.usart_count = 3;
            caps.spi_count = 2;
            caps.i2c_count = 2;
            caps.adc_channels = 16;
            caps.dac_channels = 2;
            caps.timer_count = 7;
            caps.has_dma = true;
            caps.dma_channels = 7;
            caps.has_usb = true;
            caps.has_can = true;
            caps.has_ethernet = false;
            caps.has_crypto = false;
        }

        // STM32F4 family
        0x413 | 0x419 | 0x421 | 0x423 | 0x431 | 0x433 | 0x434 | 0x441 | 0x446 => {
            caps.family = Some(Stm32Family::F4);
            caps.flash_size_kb = 512; // Typical
            caps.ram_size_kb = 128;
            caps.usart_count = 6;
            caps.spi_count = 4;
            caps.i2c_count = 3;
            caps.adc_channels = 16;
            caps.dac_channels = 2;
            caps.timer_count = 14;
            caps.has_dma = true;
            caps.dma_channels = 16; // 2 DMA controllers with 8 streams each
            caps.has_usb = true;
            caps.has_can = true;
            caps.has_ethernet = (dev_id == 0x413 || dev_id == 0x419); // F407/F417
            caps.has_crypto = (dev_id == 0x419 || dev_id == 0x434); // F417/F469
        }

        // STM32F7 family
        0x449 | 0x450 | 0x451 | 0x452 => {
            caps.family = Some(Stm32Family::F7);
            caps.flash_size_kb = 1024; // Typical
            caps.ram_size_kb = 320;
            caps.usart_count = 8;
            caps.spi_count = 6;
            caps.i2c_count = 4;
            caps.adc_channels = 24;
            caps.dac_channels = 2;
            caps.timer_count = 16;
            caps.has_dma = true;
            caps.dma_channels = 16;
            caps.has_usb = true;
            caps.has_can = true;
            caps.has_ethernet = true;
            caps.has_crypto = true;
        }

        // STM32H7 family
        0x450 | 0x480 | 0x483 => {
            caps.family = Some(Stm32Family::H7);
            caps.flash_size_kb = 2048; // Typical
            caps.ram_size_kb = 1024;
            caps.usart_count = 8;
            caps.spi_count = 6;
            caps.i2c_count = 4;
            caps.adc_channels = 20;
            caps.dac_channels = 2;
            caps.timer_count = 16;
            caps.has_dma = true;
            caps.dma_channels = 16;
            caps.has_usb = true;
            caps.has_can = true;
            caps.has_ethernet = true;
            caps.has_crypto = true;
        }

        // STM32L4 family
        0x415 | 0x435 | 0x461 | 0x462 | 0x464 | 0x470 | 0x471 => {
            caps.family = Some(Stm32Family::L4);
            caps.flash_size_kb = 512; // Typical
            caps.ram_size_kb = 128;
            caps.usart_count = 5;
            caps.spi_count = 3;
            caps.i2c_count = 4;
            caps.adc_channels = 16;
            caps.dac_channels = 2;
            caps.timer_count = 11;
            caps.has_dma = true;
            caps.dma_channels = 14;
            caps.has_usb = true;
            caps.has_can = true;
            caps.has_ethernet = false;
            caps.has_crypto = true;
        }

        // STM32G4 family
        0x468 | 0x469 => {
            caps.family = Some(Stm32Family::G4);
            caps.flash_size_kb = 512; // Typical
            caps.ram_size_kb = 128;
            caps.usart_count = 5;
            caps.spi_count = 4;
            caps.i2c_count = 4;
            caps.adc_channels = 20;
            caps.dac_channels = 4;
            caps.timer_count = 10;
            caps.has_dma = true;
            caps.dma_channels = 16;
            caps.has_usb = true;
            caps.has_can = true;
            caps.has_ethernet = false;
            caps.has_crypto = true;
        }

        _ => {
            // Unknown device ID - leave as default
        }
    }

    caps
}

#[cfg(all(target_arch = "arm", target_os = "none"))]
#[allow(dead_code)]
fn detect_stm32() -> Platform {
    // Read DBGMCU_IDCODE register to identify STM32 family
    const DBGMCU_IDCODE_ADDR_OLD: usize = 0xE0042000;
    const DBGMCU_IDCODE_ADDR_NEW: usize = 0xE0044000;

    let device_id = unsafe {
        let id_old = core::ptr::read_volatile(DBGMCU_IDCODE_ADDR_OLD as *const u32);
        if id_old == 0 || id_old == 0xFFFFFFFF {
            core::ptr::read_volatile(DBGMCU_IDCODE_ADDR_NEW as *const u32)
        } else {
            id_old
        }
    };

    let dev_id = device_id & 0xFFF;

    match dev_id {
        0x440 | 0x442 | 0x444 | 0x445 | 0x448 => Platform::Stm32(Stm32Family::F0),
        0x410 | 0x412 | 0x414 | 0x418 | 0x420 | 0x428 | 0x430 => Platform::Stm32(Stm32Family::F1),
        0x411 => Platform::Stm32(Stm32Family::F2),
        0x422 | 0x432 | 0x438 | 0x439 => Platform::Stm32(Stm32Family::F3),
        0x413 | 0x419 | 0x421 | 0x423 | 0x431 | 0x433 | 0x434 | 0x441 | 0x446 => {
            Platform::Stm32(Stm32Family::F4)
        }
        0x449 | 0x450 | 0x451 | 0x452 => Platform::Stm32(Stm32Family::F7),
        0x450 | 0x480 | 0x483 => Platform::Stm32(Stm32Family::H7),
        0x417 | 0x425 | 0x427 => Platform::Stm32(Stm32Family::L0),
        0x416 | 0x429 | 0x436 | 0x437 => Platform::Stm32(Stm32Family::L1),
        0x415 | 0x435 | 0x461 | 0x462 | 0x464 | 0x470 | 0x471 => Platform::Stm32(Stm32Family::L4),
        0x472 => Platform::Stm32(Stm32Family::L5),
        0x456 | 0x460 => Platform::Stm32(Stm32Family::G0),
        0x468 | 0x469 => Platform::Stm32(Stm32Family::G4),
        0x472 => Platform::Stm32(Stm32Family::U5),
        0x494 | 0x495 => Platform::Stm32(Stm32Family::WB),
        0x497 => Platform::Stm32(Stm32Family::WL),
        _ => Platform::Unknown,
    }
}

#[cfg(target_os = "espidf")]
fn detect_platform_esp32() -> Platform {
    // Detect ESP32 variant using chip model
    // In a real implementation, this would use:
    // extern "C" { fn esp_chip_info(info: *mut esp_chip_info_t); }
    //
    // For now, we use a simplified approach based on compile-time features
    // or read from EFUSE registers

    #[cfg(any(feature = "esp32", esp_idf_soc = "esp32"))]
    {
        return Platform::Esp32(Esp32Variant::Esp32);
    }

    #[cfg(any(feature = "esp32s2", esp_idf_soc = "esp32s2"))]
    {
        return Platform::Esp32(Esp32Variant::Esp32S2);
    }

    #[cfg(any(feature = "esp32s3", esp_idf_soc = "esp32s3"))]
    {
        return Platform::Esp32(Esp32Variant::Esp32S3);
    }

    #[cfg(any(feature = "esp32c3", esp_idf_soc = "esp32c3"))]
    {
        return Platform::Esp32(Esp32Variant::Esp32C3);
    }

    #[cfg(any(feature = "esp32c6", esp_idf_soc = "esp32c6"))]
    {
        return Platform::Esp32(Esp32Variant::Esp32C6);
    }

    #[cfg(any(feature = "esp32h2", esp_idf_soc = "esp32h2"))]
    {
        return Platform::Esp32(Esp32Variant::Esp32H2);
    }

    // Default fallback
    #[allow(unreachable_code)]
    {
        Platform::Esp32(Esp32Variant::Esp32)
    }
}

#[cfg(target_os = "espidf")]
fn detect_esp32_espidf() -> Esp32Capabilities {
    let mut caps = Esp32Capabilities::default();

    // In a real ESP-IDF implementation, you would use:
    // - esp_chip_info() for chip model, cores, revision
    // - esp_get_flash_size() for flash size
    // - esp_spiram_get_size() for PSRAM size
    // - WiFi/BLE capabilities from the chip model

    // Set capabilities based on variant detected
    let variant = if let Platform::Esp32(v) = detect_platform_esp32() {
        v
    } else {
        Esp32Variant::Esp32
    };

    caps.variant = Some(variant);

    match variant {
        Esp32Variant::Esp32 => {
            caps.wifi_standard = 4; // WiFi 4 (802.11n)
            caps.has_bt_classic = true;
            caps.has_ble = true;
            caps.ble_version = (4, 2);
            caps.has_thread_zigbee = false;
            caps.cpu_cores = 2; // Dual-core Xtensa
            caps.has_psram = false; // Depends on board
            caps.psram_size_mb = 0;
            caps.flash_size_mb = 4; // Typical
            caps.has_secure_boot = true;
            caps.has_flash_encryption = true;
        }

        Esp32Variant::Esp32S2 => {
            caps.wifi_standard = 4;
            caps.has_bt_classic = false;
            caps.has_ble = false;
            caps.ble_version = (0, 0);
            caps.has_thread_zigbee = false;
            caps.cpu_cores = 1; // Single-core Xtensa
            caps.has_psram = false;
            caps.psram_size_mb = 0;
            caps.flash_size_mb = 4;
            caps.has_secure_boot = true;
            caps.has_flash_encryption = true;
        }

        Esp32Variant::Esp32S3 => {
            caps.wifi_standard = 4;
            caps.has_bt_classic = false;
            caps.has_ble = true;
            caps.ble_version = (5, 0);
            caps.has_thread_zigbee = false;
            caps.cpu_cores = 2; // Dual-core Xtensa
            caps.has_psram = false;
            caps.psram_size_mb = 0;
            caps.flash_size_mb = 8; // Typical
            caps.has_secure_boot = true;
            caps.has_flash_encryption = true;
        }

        Esp32Variant::Esp32C3 => {
            caps.wifi_standard = 4;
            caps.has_bt_classic = false;
            caps.has_ble = true;
            caps.ble_version = (5, 0);
            caps.has_thread_zigbee = false;
            caps.cpu_cores = 1; // Single-core RISC-V
            caps.has_psram = false;
            caps.psram_size_mb = 0;
            caps.flash_size_mb = 4;
            caps.has_secure_boot = true;
            caps.has_flash_encryption = true;
        }

        Esp32Variant::Esp32C6 => {
            caps.wifi_standard = 6; // WiFi 6 (802.11ax)
            caps.has_bt_classic = false;
            caps.has_ble = true;
            caps.ble_version = (5, 3);
            caps.has_thread_zigbee = true; // 802.15.4 support
            caps.cpu_cores = 1; // Single-core RISC-V
            caps.has_psram = false;
            caps.psram_size_mb = 0;
            caps.flash_size_mb = 4;
            caps.has_secure_boot = true;
            caps.has_flash_encryption = true;
        }

        Esp32Variant::Esp32H2 => {
            caps.wifi_standard = 0; // No WiFi
            caps.has_bt_classic = false;
            caps.has_ble = true;
            caps.ble_version = (5, 3);
            caps.has_thread_zigbee = true; // Dedicated Thread/Zigbee chip
            caps.cpu_cores = 1; // Single-core RISC-V
            caps.has_psram = false;
            caps.psram_size_mb = 0;
            caps.flash_size_mb = 4;
            caps.has_secure_boot = true;
            caps.has_flash_encryption = true;
        }
    }

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = detect_platform();
        // Should not panic
        let _display = alloc::format!("{}", platform);
    }

    #[test]
    fn test_platform_display() {
        let platforms = [
            Platform::Unknown,
            Platform::RaspberryPi(RaspberryPiModel::Pi4),
            Platform::Stm32(Stm32Family::F4),
            Platform::Esp32(Esp32Variant::Esp32S3),
            Platform::BeagleBone(BeagleBoneModel::Black),
            Platform::GenericX86,
            Platform::GenericArm,
            Platform::GenericRiscV,
        ];

        for platform in platforms {
            let display = alloc::format!("{}", platform);
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn test_raspberry_pi_models_display() {
        let models = [
            RaspberryPiModel::Pi1,
            RaspberryPiModel::Pi2,
            RaspberryPiModel::Pi3,
            RaspberryPiModel::Pi4,
            RaspberryPiModel::Pi5,
            RaspberryPiModel::PiZero,
            RaspberryPiModel::Pi400,
            RaspberryPiModel::ComputeModule(4),
        ];

        for model in models {
            let display = alloc::format!("{}", model);
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn test_stm32_families() {
        let families = [
            Stm32Family::F0,
            Stm32Family::F1,
            Stm32Family::F4,
            Stm32Family::F7,
            Stm32Family::H7,
            Stm32Family::L4,
        ];

        for family in families {
            let display = alloc::format!("{}", family);
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn test_esp32_variants() {
        let variants = [
            Esp32Variant::Esp32,
            Esp32Variant::Esp32S2,
            Esp32Variant::Esp32S3,
            Esp32Variant::Esp32C3,
            Esp32Variant::Esp32C6,
            Esp32Variant::Esp32H2,
        ];

        for variant in variants {
            let display = alloc::format!("{}", variant);
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn test_raspberry_pi_capabilities() {
        let caps = RaspberryPiCapabilities::detect();
        // Should not panic - just ensure detection works
        let _ = caps.gpio_pins;
    }

    #[test]
    fn test_raspberry_pi_is_pi5() {
        let mut caps = RaspberryPiCapabilities::default();
        assert!(!caps.is_pi5());

        caps.model = Some(RaspberryPiModel::Pi5);
        assert!(caps.is_pi5());
    }

    #[test]
    fn test_raspberry_pi_has_wireless() {
        let mut caps = RaspberryPiCapabilities::default();
        assert!(!caps.has_wireless());

        caps.has_wifi = true;
        assert!(caps.has_wireless());

        caps.has_wifi = false;
        caps.has_bluetooth = true;
        assert!(caps.has_wireless());
    }

    #[test]
    fn test_stm32_capabilities() {
        let caps = Stm32Capabilities::detect();
        // Should not panic - just ensure detection works
        let _ = caps.flash_size_kb;
    }

    #[test]
    fn test_esp32_capabilities() {
        let caps = Esp32Capabilities::detect();
        // Should not panic - just ensure detection works
        let _ = caps.cpu_cores;
    }

    #[test]
    fn test_esp32_has_full_wireless() {
        let mut caps = Esp32Capabilities::default();
        assert!(!caps.has_full_wireless());

        caps.wifi_standard = 4;
        caps.has_ble = true;
        assert!(caps.has_full_wireless());
    }

    #[test]
    fn test_detect_capabilities() {
        let caps = detect_capabilities();
        // Should not panic
        match caps {
            PlatformCapabilities::RaspberryPi(_) => {}
            PlatformCapabilities::Stm32(_) => {}
            PlatformCapabilities::Esp32(_) => {}
            PlatformCapabilities::Generic => {}
        }
    }

    #[test]
    fn test_platform_equality() {
        let p1 = Platform::RaspberryPi(RaspberryPiModel::Pi4);
        let p2 = Platform::RaspberryPi(RaspberryPiModel::Pi4);
        let p3 = Platform::RaspberryPi(RaspberryPiModel::Pi5);

        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }

    #[test]
    fn test_beaglebone_models() {
        let models = [
            BeagleBoneModel::Black,
            BeagleBoneModel::Green,
            BeagleBoneModel::Ai,
            BeagleBoneModel::Ai64,
            BeagleBoneModel::Pocket,
        ];

        for model in models {
            let display = alloc::format!("{}", model);
            assert!(!display.is_empty());
        }
    }
}
