//! # `HardwareDatabase` - add_embedded_processors_group Methods
//!
//! This module contains method implementations for `HardwareDatabase`.
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::capabilities::HardwareCapabilities;
use crate::Architecture;

use super::types::ProcessorSpec;

use super::hardwaredatabase_type::HardwareDatabase;

impl HardwareDatabase {
    pub(super) fn add_embedded_processors(&mut self) {
        self.processors.push(ProcessorSpec {
            name: "ESP32-S3-WROOM-1".into(),
            vendor: "Espressif".into(),
            architecture: Architecture::Xtensa,
            capabilities: HardwareCapabilities::FPU | HardwareCapabilities::SIMD,
            physical_cores: 2,
            logical_cores: 2,
            base_freq_mhz: 240,
            max_freq_mhz: 240,
            l1d_per_core_kb: 16,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "ESP32-C6-WROOM-1".into(),
            vendor: "Espressif".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 160,
            max_freq_mhz: 160,
            l1d_per_core_kb: 16,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "STM32H743".into(),
            vendor: "STMicroelectronics".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 480,
            max_freq_mhz: 480,
            l1d_per_core_kb: 16,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "STM32F407".into(),
            vendor: "STMicroelectronics".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 168,
            max_freq_mhz: 168,
            l1d_per_core_kb: 8,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "STM32F103".into(),
            vendor: "STMicroelectronics".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::NONE,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 72,
            max_freq_mhz: 72,
            l1d_per_core_kb: 0,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "NXP i.MX RT1062".into(),
            vendor: "NXP".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 600,
            max_freq_mhz: 600,
            l1d_per_core_kb: 32,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 2,
        });
        self.processors.push(ProcessorSpec {
            name: "Raspberry Pi RP2040".into(),
            vendor: "Raspberry Pi".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::NONE,
            physical_cores: 2,
            logical_cores: 2,
            base_freq_mhz: 133,
            max_freq_mhz: 133,
            l1d_per_core_kb: 0,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "Nordic nRF52840".into(),
            vendor: "Nordic".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 64,
            max_freq_mhz: 64,
            l1d_per_core_kb: 0,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "TI CC2652".into(),
            vendor: "Texas Instruments".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 48,
            max_freq_mhz: 48,
            l1d_per_core_kb: 0,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "Microchip SAMD51".into(),
            vendor: "Microchip".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 120,
            max_freq_mhz: 120,
            l1d_per_core_kb: 0,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "Renesas RA6M4".into(),
            vendor: "Renesas".into(),
            architecture: Architecture::CortexM,
            capabilities: HardwareCapabilities::FPU,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 200,
            max_freq_mhz: 200,
            l1d_per_core_kb: 0,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "GigaDevice GD32VF103".into(),
            vendor: "GigaDevice".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 108,
            max_freq_mhz: 108,
            l1d_per_core_kb: 16,
            l2_per_core_kb: 0,
            l3_total_kb: 0,
            cache_line_bytes: 32,
            tdp_watts: 1,
        });
        self.processors.push(ProcessorSpec {
            name: "Kendryte K210".into(),
            vendor: "Kendryte".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS,
            physical_cores: 2,
            logical_cores: 2,
            base_freq_mhz: 400,
            max_freq_mhz: 400,
            l1d_per_core_kb: 32,
            l2_per_core_kb: 6144,
            l3_total_kb: 0,
            cache_line_bytes: 64,
            tdp_watts: 1,
        });
    }
}
