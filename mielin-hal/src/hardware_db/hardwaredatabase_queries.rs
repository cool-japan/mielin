//! # `HardwareDatabase` - queries Methods
//!
//! This module contains method implementations for `HardwareDatabase`.
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::capabilities::HardwareCapabilities;
use crate::Architecture;
use alloc::vec::Vec;

use super::types::ProcessorSpec;

use super::hardwaredatabase_type::HardwareDatabase;

impl HardwareDatabase {
    /// Create a new hardware database with known configurations
    pub fn new() -> Self {
        let mut db = Self { processors: Vec::new() };
        db.add_intel_processors();
        db.add_amd_processors();
        db.add_arm_processors();
        db.add_apple_processors();
        db.add_qualcomm_processors();
        db.add_mediatek_processors();
        db.add_riscv_processors();
        db.add_loongarch_processors();
        db.add_jetson_processors();
        db.add_mobile_socs();
        db.add_embedded_processors();
        db
    }
    /// Find processor specs by name
    pub fn find_by_name(&self, name: &str) -> Option<&ProcessorSpec> {
        self.processors.iter().find(|p| p.name.contains(name))
    }
    /// Find processors by vendor
    pub fn find_by_vendor(&self, vendor: &str) -> Vec<&ProcessorSpec> {
        self.processors
            .iter()
            .filter(|p| p.vendor.eq_ignore_ascii_case(vendor))
            .collect()
    }
    /// Find processors by architecture
    pub fn find_by_architecture(&self, arch: Architecture) -> Vec<&ProcessorSpec> {
        self.processors.iter().filter(|p| p.architecture == arch).collect()
    }
    /// Find processors with specific capabilities
    pub fn find_with_capabilities(
        &self,
        caps: HardwareCapabilities,
    ) -> Vec<&ProcessorSpec> {
        self.processors.iter().filter(|p| p.capabilities.contains(caps)).collect()
    }
    /// Get all processor specs
    pub fn all_processors(&self) -> &[ProcessorSpec] {
        &self.processors
    }
    /// Find best match for current hardware
    pub fn find_current_match(&self) -> Option<&ProcessorSpec> {
        self.processors.iter().find(|p| p.matches_current())
    }
    fn add_intel_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i9-14900K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 24,
                logical_cores: 32,
                base_freq_mhz: 3200,
                max_freq_mhz: 6000,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 2048,
                l3_total_kb: 36864,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i9-13900K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 24,
                logical_cores: 32,
                base_freq_mhz: 3000,
                max_freq_mhz: 5800,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 2048,
                l3_total_kb: 36864,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-13700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 16,
                logical_cores: 24,
                base_freq_mhz: 3400,
                max_freq_mhz: 5400,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 2048,
                l3_total_kb: 30720,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-12700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 12,
                logical_cores: 20,
                base_freq_mhz: 3600,
                max_freq_mhz: 5000,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 25600,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i5-12600K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 10,
                logical_cores: 16,
                base_freq_mhz: 3700,
                max_freq_mhz: 4900,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 20480,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i9-11900K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3500,
                max_freq_mhz: 5300,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 512,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-11700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3600,
                max_freq_mhz: 5000,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 512,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i9-10900K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 10,
                logical_cores: 20,
                base_freq_mhz: 3700,
                max_freq_mhz: 5300,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 20480,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-1185G7".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 4,
                logical_cores: 8,
                base_freq_mhz: 3000,
                max_freq_mhz: 4800,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 12288,
                cache_line_bytes: 64,
                tdp_watts: 28,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Xeon Platinum 8380".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 40,
                logical_cores: 80,
                base_freq_mhz: 2300,
                max_freq_mhz: 3400,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 61440,
                cache_line_bytes: 64,
                tdp_watts: 270,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Xeon Gold 6338".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 32,
                logical_cores: 64,
                base_freq_mhz: 2000,
                max_freq_mhz: 3200,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 49152,
                cache_line_bytes: 64,
                tdp_watts: 205,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Xeon W-3375".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 38,
                logical_cores: 76,
                base_freq_mhz: 2500,
                max_freq_mhz: 4000,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 57344,
                cache_line_bytes: 64,
                tdp_watts: 270,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i5-13600K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 14,
                logical_cores: 20,
                base_freq_mhz: 3500,
                max_freq_mhz: 5100,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 2048,
                l3_total_kb: 24576,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i9-12900K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 16,
                logical_cores: 24,
                base_freq_mhz: 3200,
                max_freq_mhz: 5200,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 30720,
                cache_line_bytes: 64,
                tdp_watts: 125,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i9-9900K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3600,
                max_freq_mhz: 5000,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 95,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-9700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 3600,
                max_freq_mhz: 4900,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 12288,
                cache_line_bytes: 64,
                tdp_watts: 95,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i5-9600K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 6,
                logical_cores: 6,
                base_freq_mhz: 3700,
                max_freq_mhz: 4600,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 9216,
                cache_line_bytes: 64,
                tdp_watts: 95,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-8700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 6,
                logical_cores: 12,
                base_freq_mhz: 3700,
                max_freq_mhz: 4700,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 12288,
                cache_line_bytes: 64,
                tdp_watts: 95,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-7700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 4,
                logical_cores: 8,
                base_freq_mhz: 4200,
                max_freq_mhz: 4500,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 91,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i7-6700K".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 4,
                logical_cores: 8,
                base_freq_mhz: 4000,
                max_freq_mhz: 4200,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 91,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Xeon E5-2699 v4".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 22,
                logical_cores: 44,
                base_freq_mhz: 2200,
                max_freq_mhz: 3600,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 56320,
                cache_line_bytes: 64,
                tdp_watts: 145,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Xeon Gold 6254".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 18,
                logical_cores: 36,
                base_freq_mhz: 3100,
                max_freq_mhz: 4000,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 25344,
                cache_line_bytes: 64,
                tdp_watts: 200,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Atom x7-Z8750".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::AES_NI,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 1600,
                max_freq_mhz: 2560,
                l1d_per_core_kb: 24,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 2,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i3-12100".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 4,
                logical_cores: 8,
                base_freq_mhz: 3300,
                max_freq_mhz: 4300,
                l1d_per_core_kb: 48,
                l2_per_core_kb: 1280,
                l3_total_kb: 12288,
                cache_line_bytes: 64,
                tdp_watts: 60,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Intel Core i3-10100".into(),
                vendor: "Intel".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 4,
                logical_cores: 8,
                base_freq_mhz: 3600,
                max_freq_mhz: 4300,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 256,
                l3_total_kb: 6144,
                cache_line_bytes: 64,
                tdp_watts: 65,
            });
    }
    fn add_amd_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 9 7950X3D".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 16,
                logical_cores: 32,
                base_freq_mhz: 4200,
                max_freq_mhz: 5700,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 131072,
                cache_line_bytes: 64,
                tdp_watts: 120,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 9 7950X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 16,
                logical_cores: 32,
                base_freq_mhz: 4500,
                max_freq_mhz: 5700,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 65536,
                cache_line_bytes: 64,
                tdp_watts: 170,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 9 7900X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 12,
                logical_cores: 24,
                base_freq_mhz: 4700,
                max_freq_mhz: 5400,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 65536,
                cache_line_bytes: 64,
                tdp_watts: 170,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 9 5950X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 16,
                logical_cores: 32,
                base_freq_mhz: 3400,
                max_freq_mhz: 4900,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 65536,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 9 5900X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 12,
                logical_cores: 24,
                base_freq_mhz: 3700,
                max_freq_mhz: 4800,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 65536,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 7 5800X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3800,
                max_freq_mhz: 4700,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 7 5800X3D".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3400,
                max_freq_mhz: 4500,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 98304,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 7 3700X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3600,
                max_freq_mhz: 4400,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 65,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 5 5600X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 6,
                logical_cores: 12,
                base_freq_mhz: 3700,
                max_freq_mhz: 4600,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 65,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD EPYC 9654".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 96,
                logical_cores: 192,
                base_freq_mhz: 2400,
                max_freq_mhz: 3700,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 393216,
                cache_line_bytes: 64,
                tdp_watts: 360,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD EPYC 7763".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 64,
                logical_cores: 128,
                base_freq_mhz: 2450,
                max_freq_mhz: 3500,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 262144,
                cache_line_bytes: 64,
                tdp_watts: 280,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Threadripper PRO 5995WX".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 64,
                logical_cores: 128,
                base_freq_mhz: 2700,
                max_freq_mhz: 4500,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 262144,
                cache_line_bytes: 64,
                tdp_watts: 280,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Threadripper PRO 3995WX".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 64,
                logical_cores: 128,
                base_freq_mhz: 2700,
                max_freq_mhz: 4200,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 262144,
                cache_line_bytes: 64,
                tdp_watts: 280,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 7 7700X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 4500,
                max_freq_mhz: 5400,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 5 7600X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX512 | HardwareCapabilities::AVX2
                    | HardwareCapabilities::AVX | HardwareCapabilities::SSE4_2
                    | HardwareCapabilities::FMA | HardwareCapabilities::AES_NI,
                physical_cores: 6,
                logical_cores: 12,
                base_freq_mhz: 4700,
                max_freq_mhz: 5300,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 1024,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 5 5600G".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 6,
                logical_cores: 12,
                base_freq_mhz: 3900,
                max_freq_mhz: 4400,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 65,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 7 5700G".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3800,
                max_freq_mhz: 4600,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 65,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 9 3950X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 16,
                logical_cores: 32,
                base_freq_mhz: 3500,
                max_freq_mhz: 4700,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 65536,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD Ryzen 7 2700X".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 8,
                logical_cores: 16,
                base_freq_mhz: 3700,
                max_freq_mhz: 4300,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 105,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AMD EPYC 7443".into(),
                vendor: "AMD".into(),
                architecture: Architecture::X86_64,
                capabilities: HardwareCapabilities::AVX2 | HardwareCapabilities::AVX
                    | HardwareCapabilities::SSE4_2 | HardwareCapabilities::FMA
                    | HardwareCapabilities::AES_NI,
                physical_cores: 24,
                logical_cores: 48,
                base_freq_mhz: 2850,
                max_freq_mhz: 4000,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 131072,
                cache_line_bytes: 64,
                tdp_watts: 200,
            });
    }
    fn add_arm_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-X4".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::SVE2
                    | HardwareCapabilities::FPU | HardwareCapabilities::CRYPTO
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2500,
                max_freq_mhz: 3400,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 3,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-X2".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::SVE2
                    | HardwareCapabilities::FPU | HardwareCapabilities::CRYPTO
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2400,
                max_freq_mhz: 3200,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 2,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-X1".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2200,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 2,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-A78".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2000,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 1,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-A77".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 1800,
                max_freq_mhz: 2800,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 1,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-A76".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 1800,
                max_freq_mhz: 2600,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 1,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Neoverse V2".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::SVE2
                    | HardwareCapabilities::FPU | HardwareCapabilities::CRYPTO
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 64,
                logical_cores: 64,
                base_freq_mhz: 2600,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 2048,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 120,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Neoverse N2".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::SVE2
                    | HardwareCapabilities::FPU | HardwareCapabilities::CRYPTO
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 64,
                logical_cores: 64,
                base_freq_mhz: 2400,
                max_freq_mhz: 2800,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 1024,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 100,
            });
        self.processors
            .push(ProcessorSpec {
                name: "AWS Graviton3".into(),
                vendor: "AWS".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::SVE2
                    | HardwareCapabilities::FPU | HardwareCapabilities::CRYPTO
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 64,
                logical_cores: 64,
                base_freq_mhz: 2600,
                max_freq_mhz: 2600,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 1024,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 100,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Ampere Altra Max".into(),
                vendor: "Ampere".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 128,
                logical_cores: 128,
                base_freq_mhz: 3000,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 1024,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 250,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-A720".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::SVE2
                    | HardwareCapabilities::FPU | HardwareCapabilities::CRYPTO
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2000,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 2,
            });
        self.processors
            .push(ProcessorSpec {
                name: "ARM Cortex-A55".into(),
                vendor: "ARM".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 1000,
                max_freq_mhz: 2000,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 128,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 1,
            });
    }
    fn add_qualcomm_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "Qualcomm Snapdragon 8 Gen 3".into(),
                vendor: "Qualcomm".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2300,
                max_freq_mhz: 3300,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 12,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Qualcomm Snapdragon 8 Gen 2".into(),
                vendor: "Qualcomm".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2000,
                max_freq_mhz: 3200,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 6144,
                cache_line_bytes: 64,
                tdp_watts: 11,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Qualcomm Snapdragon 8 Gen 1".into(),
                vendor: "Qualcomm".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 1800,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 6144,
                cache_line_bytes: 64,
                tdp_watts: 10,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Qualcomm Snapdragon 7+ Gen 2".into(),
                vendor: "Qualcomm".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 1800,
                max_freq_mhz: 2910,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 4096,
                cache_line_bytes: 64,
                tdp_watts: 8,
            });
    }
    fn add_mediatek_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "MediaTek Dimensity 9300".into(),
                vendor: "MediaTek".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2000,
                max_freq_mhz: 3250,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 10,
            });
        self.processors
            .push(ProcessorSpec {
                name: "MediaTek Dimensity 9200".into(),
                vendor: "MediaTek".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 1800,
                max_freq_mhz: 3050,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 6144,
                cache_line_bytes: 64,
                tdp_watts: 9,
            });
        self.processors
            .push(ProcessorSpec {
                name: "MediaTek Dimensity 8300".into(),
                vendor: "MediaTek".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2000,
                max_freq_mhz: 3350,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 4096,
                cache_line_bytes: 64,
                tdp_watts: 8,
            });
    }
    fn add_riscv_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "StarFive JH7110".into(),
                vendor: "StarFive".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS
                    | HardwareCapabilities::RVV | HardwareCapabilities::SIMD,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 1500,
                max_freq_mhz: 1500,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 2048,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 6,
            });
        self.processors
            .push(ProcessorSpec {
                name: "SiFive U74".into(),
                vendor: "SiFive".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 1200,
                max_freq_mhz: 1400,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 2048,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 5,
            });
        self.processors
            .push(ProcessorSpec {
                name: "SiFive E76".into(),
                vendor: "SiFive".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS
                    | HardwareCapabilities::RVB,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 800,
                max_freq_mhz: 1000,
                l1d_per_core_kb: 16,
                l2_per_core_kb: 256,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 1,
            });
        self.processors
            .push(ProcessorSpec {
                name: "SiFive P550".into(),
                vendor: "SiFive".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS
                    | HardwareCapabilities::RVV | HardwareCapabilities::RVB
                    | HardwareCapabilities::SIMD,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2000,
                max_freq_mhz: 2400,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 3,
            });
        self.processors
            .push(ProcessorSpec {
                name: "SiFive P670".into(),
                vendor: "SiFive".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS
                    | HardwareCapabilities::RVV | HardwareCapabilities::RVB
                    | HardwareCapabilities::RVK | HardwareCapabilities::SIMD,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 2200,
                max_freq_mhz: 2800,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 1024,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 4,
            });
        self.processors
            .push(ProcessorSpec {
                name: "T-Head C910".into(),
                vendor: "T-Head".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS
                    | HardwareCapabilities::RVV | HardwareCapabilities::SIMD,
                physical_cores: 1,
                logical_cores: 1,
                base_freq_mhz: 1000,
                max_freq_mhz: 1000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 1024,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 2,
            });
        self.processors
            .push(ProcessorSpec {
                name: "T-Head C920".into(),
                vendor: "T-Head".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS
                    | HardwareCapabilities::RVV | HardwareCapabilities::RVB
                    | HardwareCapabilities::SIMD,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 1800,
                max_freq_mhz: 2000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 1024,
                l3_total_kb: 4096,
                cache_line_bytes: 64,
                tdp_watts: 8,
            });
        self.processors
            .push(ProcessorSpec {
                name: "StarFive JH7100".into(),
                vendor: "StarFive".into(),
                architecture: Architecture::RiscV64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::ATOMICS,
                physical_cores: 2,
                logical_cores: 2,
                base_freq_mhz: 1000,
                max_freq_mhz: 1500,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 2048,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 5,
            });
    }
    fn add_apple_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "Apple M3 Max".into(),
                vendor: "Apple".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 16,
                logical_cores: 16,
                base_freq_mhz: 2400,
                max_freq_mhz: 4050,
                l1d_per_core_kb: 128,
                l2_per_core_kb: 16384,
                l3_total_kb: 0,
                cache_line_bytes: 128,
                tdp_watts: 40,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Apple M2".into(),
                vendor: "Apple".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2400,
                max_freq_mhz: 3490,
                l1d_per_core_kb: 128,
                l2_per_core_kb: 16384,
                l3_total_kb: 0,
                cache_line_bytes: 128,
                tdp_watts: 20,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Apple M1".into(),
                vendor: "Apple".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2064,
                max_freq_mhz: 3200,
                l1d_per_core_kb: 128,
                l2_per_core_kb: 12288,
                l3_total_kb: 0,
                cache_line_bytes: 128,
                tdp_watts: 15,
            });
    }
    fn add_loongarch_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "Loongson 3A6000".into(),
                vendor: "Loongson".into(),
                architecture: Architecture::LoongArch64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::SIMD
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 2500,
                max_freq_mhz: 2500,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 50,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Loongson 3A5000".into(),
                vendor: "Loongson".into(),
                architecture: Architecture::LoongArch64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::SIMD
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 2300,
                max_freq_mhz: 2500,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 16384,
                cache_line_bytes: 64,
                tdp_watts: 45,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Loongson 3C5000".into(),
                vendor: "Loongson".into(),
                architecture: Architecture::LoongArch64,
                capabilities: HardwareCapabilities::FPU | HardwareCapabilities::SIMD
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 16,
                logical_cores: 16,
                base_freq_mhz: 2200,
                max_freq_mhz: 2200,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 32768,
                cache_line_bytes: 64,
                tdp_watts: 120,
            });
    }
    fn add_jetson_processors(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "NVIDIA Jetson AGX Orin".into(),
                vendor: "NVIDIA".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 12,
                logical_cores: 12,
                base_freq_mhz: 2000,
                max_freq_mhz: 2200,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 4096,
                cache_line_bytes: 64,
                tdp_watts: 60,
            });
        self.processors
            .push(ProcessorSpec {
                name: "NVIDIA Jetson Xavier NX".into(),
                vendor: "NVIDIA".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 6,
                logical_cores: 6,
                base_freq_mhz: 1400,
                max_freq_mhz: 1900,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 4096,
                cache_line_bytes: 64,
                tdp_watts: 20,
            });
        self.processors
            .push(ProcessorSpec {
                name: "NVIDIA Jetson Orin Nano".into(),
                vendor: "NVIDIA".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 6,
                logical_cores: 6,
                base_freq_mhz: 1500,
                max_freq_mhz: 1500,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 256,
                l3_total_kb: 2048,
                cache_line_bytes: 64,
                tdp_watts: 15,
            });
        self.processors
            .push(ProcessorSpec {
                name: "NVIDIA Jetson Nano".into(),
                vendor: "NVIDIA".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::ATOMICS,
                physical_cores: 4,
                logical_cores: 4,
                base_freq_mhz: 1430,
                max_freq_mhz: 1430,
                l1d_per_core_kb: 32,
                l2_per_core_kb: 512,
                l3_total_kb: 0,
                cache_line_bytes: 64,
                tdp_watts: 10,
            });
    }
    fn add_mobile_socs(&mut self) {
        self.processors
            .push(ProcessorSpec {
                name: "Samsung Exynos 2400".into(),
                vendor: "Samsung".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 10,
                logical_cores: 10,
                base_freq_mhz: 2200,
                max_freq_mhz: 3200,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 10,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Samsung Exynos 2200".into(),
                vendor: "Samsung".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 1800,
                max_freq_mhz: 2800,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 6144,
                cache_line_bytes: 64,
                tdp_watts: 9,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Google Tensor G3".into(),
                vendor: "Google".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 9,
                logical_cores: 9,
                base_freq_mhz: 1900,
                max_freq_mhz: 3000,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 9,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Google Tensor G2".into(),
                vendor: "Google".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 1800,
                max_freq_mhz: 2850,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 6144,
                cache_line_bytes: 64,
                tdp_watts: 8,
            });
        self.processors
            .push(ProcessorSpec {
                name: "HiSilicon Kirin 9000S".into(),
                vendor: "HiSilicon".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 2000,
                max_freq_mhz: 3130,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 8192,
                cache_line_bytes: 64,
                tdp_watts: 9,
            });
        self.processors
            .push(ProcessorSpec {
                name: "Rockchip RK3588".into(),
                vendor: "Rockchip".into(),
                architecture: Architecture::AArch64,
                capabilities: HardwareCapabilities::NEON | HardwareCapabilities::FPU
                    | HardwareCapabilities::CRYPTO | HardwareCapabilities::ATOMICS,
                physical_cores: 8,
                logical_cores: 8,
                base_freq_mhz: 1800,
                max_freq_mhz: 2400,
                l1d_per_core_kb: 64,
                l2_per_core_kb: 512,
                l3_total_kb: 3072,
                cache_line_bytes: 64,
                tdp_watts: 12,
            });
    }
}
