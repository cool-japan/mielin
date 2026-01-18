//! Hardware Database
//!
//! This module provides a database of known hardware configurations for validation,
//! testing, and reference. It includes specifications for popular CPUs, GPUs, and
//! accelerators to help verify detection accuracy.

extern crate alloc;

use crate::capabilities::HardwareCapabilities;
use crate::Architecture;
use alloc::string::String;
use alloc::vec::Vec;

/// Known processor configuration
#[derive(Debug, Clone)]
pub struct ProcessorSpec {
    /// Processor name
    pub name: String,
    /// Vendor
    pub vendor: String,
    /// Architecture
    pub architecture: Architecture,
    /// Expected capabilities
    pub capabilities: HardwareCapabilities,
    /// Core count (physical)
    pub physical_cores: usize,
    /// Thread count (logical cores)
    pub logical_cores: usize,
    /// Base frequency in MHz
    pub base_freq_mhz: u32,
    /// Max frequency in MHz
    pub max_freq_mhz: u32,
    /// L1 data cache per core
    pub l1d_per_core_kb: usize,
    /// L2 cache per core
    pub l2_per_core_kb: usize,
    /// L3 cache total
    pub l3_total_kb: usize,
    /// Cache line size
    pub cache_line_bytes: usize,
    /// TDP in watts
    pub tdp_watts: u32,
}

impl ProcessorSpec {
    /// Check if current hardware matches this specification
    pub fn matches_current(&self) -> bool {
        let profile = crate::capabilities::HardwareProfile::detect();

        profile.architecture == self.architecture
            && profile.core_count >= self.physical_cores
            && profile.capabilities.contains(self.capabilities)
    }

    /// Calculate cache hierarchy score (higher is better)
    pub fn cache_score(&self) -> f32 {
        let l1_score = self.l1d_per_core_kb as f32;
        let l2_score = self.l2_per_core_kb as f32 * 0.5;
        let l3_score = (self.l3_total_kb / self.physical_cores.max(1)) as f32 * 0.25;

        l1_score + l2_score + l3_score
    }

    /// Calculate performance score (arbitrary units for comparison)
    pub fn performance_score(&self) -> f32 {
        let freq_score = (self.max_freq_mhz as f32 / 1000.0) * self.logical_cores as f32;
        let cache_score = self.cache_score();
        let capability_score = self.capabilities.bits().count_ones() as f32 * 10.0;

        freq_score + cache_score * 0.1 + capability_score
    }
}

/// Hardware database containing known configurations
pub struct HardwareDatabase {
    processors: Vec<ProcessorSpec>,
}

impl HardwareDatabase {
    /// Create a new hardware database with known configurations
    pub fn new() -> Self {
        let mut db = Self {
            processors: Vec::new(),
        };

        // Add Intel processors
        db.add_intel_processors();

        // Add AMD processors
        db.add_amd_processors();

        // Add ARM processors
        db.add_arm_processors();

        // Add Apple Silicon
        db.add_apple_processors();

        // Add Qualcomm processors
        db.add_qualcomm_processors();

        // Add MediaTek processors
        db.add_mediatek_processors();

        // Add RISC-V processors
        db.add_riscv_processors();

        // Add LoongArch processors
        db.add_loongarch_processors();

        // Add NVIDIA Jetson processors
        db.add_jetson_processors();

        // Add more mobile SoCs
        db.add_mobile_socs();

        // Add embedded processors
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
        self.processors
            .iter()
            .filter(|p| p.architecture == arch)
            .collect()
    }

    /// Find processors with specific capabilities
    pub fn find_with_capabilities(&self, caps: HardwareCapabilities) -> Vec<&ProcessorSpec> {
        self.processors
            .iter()
            .filter(|p| p.capabilities.contains(caps))
            .collect()
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
        // Intel Core i9-14900K (Raptor Lake Refresh)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i9-14900K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i9-13900K (Raptor Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i9-13900K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i7-13700K (Raptor Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-13700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i7-12700K (Alder Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-12700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i5-12600K (Alder Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i5-12600K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i9-11900K (Rocket Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i9-11900K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i7-11700K (Rocket Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-11700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i9-10900K (Comet Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i9-10900K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i7-1185G7 (Tiger Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-1185G7".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Xeon Platinum 8380 (Ice Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Xeon Platinum 8380".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Xeon Gold 6338 (Ice Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Xeon Gold 6338".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Xeon W-3375 (Ice Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Xeon W-3375".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i5-13600K (Raptor Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i5-13600K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Core i9-12900K (Alder Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i9-12900K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i9-9900K (Coffee Lake Refresh)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i9-9900K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i7-9700K (Coffee Lake Refresh)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-9700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i5-9600K (Coffee Lake Refresh)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i5-9600K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i7-8700K (Coffee Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-8700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i7-7700K (Kaby Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-7700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i7-6700K (Skylake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i7-6700K".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Xeon E5-2699 v4 (Broadwell)
        self.processors.push(ProcessorSpec {
            name: "Intel Xeon E5-2699 v4".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Xeon Gold 6254 (Cascade Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Xeon Gold 6254".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // Intel Atom x7-Z8750 (Cherry Trail)
        self.processors.push(ProcessorSpec {
            name: "Intel Atom x7-Z8750".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::SSE4_2 | HardwareCapabilities::AES_NI,
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

        // Intel Core i3-12100 (Alder Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i3-12100".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // Intel Core i3-10100 (Comet Lake)
        self.processors.push(ProcessorSpec {
            name: "Intel Core i3-10100".into(),
            vendor: "Intel".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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
        // AMD Ryzen 9 7950X3D (Zen 4 with 3D V-Cache)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 9 7950X3D".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
            physical_cores: 16,
            logical_cores: 32,
            base_freq_mhz: 4200,
            max_freq_mhz: 5700,
            l1d_per_core_kb: 32,
            l2_per_core_kb: 1024,
            l3_total_kb: 131072, // 128 MB with 3D V-Cache
            cache_line_bytes: 64,
            tdp_watts: 120,
        });

        // AMD Ryzen 9 7950X (Zen 4)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 9 7950X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // AMD Ryzen 9 7900X (Zen 4)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 9 7900X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // AMD Ryzen 9 5950X (Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 9 5950X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 9 5900X (Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 9 5900X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 7 5800X (Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 7 5800X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 7 5800X3D (Zen 3 with 3D V-Cache)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 7 5800X3D".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
            physical_cores: 8,
            logical_cores: 16,
            base_freq_mhz: 3400,
            max_freq_mhz: 4500,
            l1d_per_core_kb: 32,
            l2_per_core_kb: 512,
            l3_total_kb: 98304, // 96 MB with 3D V-Cache
            cache_line_bytes: 64,
            tdp_watts: 105,
        });

        // AMD Ryzen 7 3700X (Zen 2)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 7 3700X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 5 5600X (Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 5 5600X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD EPYC 9654 (Genoa, Zen 4)
        self.processors.push(ProcessorSpec {
            name: "AMD EPYC 9654".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
            physical_cores: 96,
            logical_cores: 192,
            base_freq_mhz: 2400,
            max_freq_mhz: 3700,
            l1d_per_core_kb: 32,
            l2_per_core_kb: 1024,
            l3_total_kb: 393216, // 384 MB
            cache_line_bytes: 64,
            tdp_watts: 360,
        });

        // AMD EPYC 7763 (Milan, Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD EPYC 7763".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Threadripper PRO 5995WX (Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD Threadripper PRO 5995WX".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Threadripper PRO 3995WX (Zen 2)
        self.processors.push(ProcessorSpec {
            name: "AMD Threadripper PRO 3995WX".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 7 7700X (Zen 4)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 7 7700X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // AMD Ryzen 5 7600X (Zen 4)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 5 7600X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX512
                | HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
                | HardwareCapabilities::AES_NI,
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

        // AMD Ryzen 5 5600G (Zen 3 APU)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 5 5600G".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 7 5700G (Zen 3 APU)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 7 5700G".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 9 3950X (Zen 2)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 9 3950X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD Ryzen 7 2700X (Zen+)
        self.processors.push(ProcessorSpec {
            name: "AMD Ryzen 7 2700X".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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

        // AMD EPYC 7443 (Milan, Zen 3)
        self.processors.push(ProcessorSpec {
            name: "AMD EPYC 7443".into(),
            vendor: "AMD".into(),
            architecture: Architecture::X86_64,
            capabilities: HardwareCapabilities::AVX2
                | HardwareCapabilities::AVX
                | HardwareCapabilities::SSE4_2
                | HardwareCapabilities::FMA
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
        // ARM Cortex-X4 (ARMv9)
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-X4".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::SVE2
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
            physical_cores: 1,
            logical_cores: 1,
            base_freq_mhz: 2500,
            max_freq_mhz: 3400,
            l1d_per_core_kb: 64,
            l2_per_core_kb: 512,
            l3_total_kb: 0, // SoC-dependent
            cache_line_bytes: 64,
            tdp_watts: 3,
        });

        // ARM Cortex-X2 (ARMv9)
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-X2".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::SVE2
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
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

        // ARM Cortex-X1
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-X1".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // ARM Cortex-A78
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-A78".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // ARM Cortex-A77
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-A77".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // ARM Cortex-A76
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-A76".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // ARM Neoverse V2 (Server)
        self.processors.push(ProcessorSpec {
            name: "ARM Neoverse V2".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::SVE2
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
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

        // ARM Neoverse N2
        self.processors.push(ProcessorSpec {
            name: "ARM Neoverse N2".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::SVE2
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
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

        // AWS Graviton3
        self.processors.push(ProcessorSpec {
            name: "AWS Graviton3".into(),
            vendor: "AWS".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::SVE2
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
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

        // Ampere Altra Max
        self.processors.push(ProcessorSpec {
            name: "Ampere Altra Max".into(),
            vendor: "Ampere".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // ARM Cortex-A720 (ARMv9)
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-A720".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::SVE2
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
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

        // ARM Cortex-A55 (Efficiency core)
        self.processors.push(ProcessorSpec {
            name: "ARM Cortex-A55".into(),
            vendor: "ARM".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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
        // Qualcomm Snapdragon 8 Gen 3
        self.processors.push(ProcessorSpec {
            name: "Qualcomm Snapdragon 8 Gen 3".into(),
            vendor: "Qualcomm".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Qualcomm Snapdragon 8 Gen 2
        self.processors.push(ProcessorSpec {
            name: "Qualcomm Snapdragon 8 Gen 2".into(),
            vendor: "Qualcomm".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Qualcomm Snapdragon 8 Gen 1
        self.processors.push(ProcessorSpec {
            name: "Qualcomm Snapdragon 8 Gen 1".into(),
            vendor: "Qualcomm".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Qualcomm Snapdragon 7+ Gen 2
        self.processors.push(ProcessorSpec {
            name: "Qualcomm Snapdragon 7+ Gen 2".into(),
            vendor: "Qualcomm".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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
        // MediaTek Dimensity 9300
        self.processors.push(ProcessorSpec {
            name: "MediaTek Dimensity 9300".into(),
            vendor: "MediaTek".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // MediaTek Dimensity 9200
        self.processors.push(ProcessorSpec {
            name: "MediaTek Dimensity 9200".into(),
            vendor: "MediaTek".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // MediaTek Dimensity 8300
        self.processors.push(ProcessorSpec {
            name: "MediaTek Dimensity 8300".into(),
            vendor: "MediaTek".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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
        // StarFive JH7110 (VisionFive 2)
        self.processors.push(ProcessorSpec {
            name: "StarFive JH7110".into(),
            vendor: "StarFive".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::ATOMICS
                | HardwareCapabilities::RVV
                | HardwareCapabilities::SIMD,
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

        // SiFive U74 (HiFive Unmatched)
        self.processors.push(ProcessorSpec {
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

        // SiFive E76 (Embedded)
        self.processors.push(ProcessorSpec {
            name: "SiFive E76".into(),
            vendor: "SiFive".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::ATOMICS
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

        // SiFive P550 (High-performance)
        self.processors.push(ProcessorSpec {
            name: "SiFive P550".into(),
            vendor: "SiFive".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::ATOMICS
                | HardwareCapabilities::RVV
                | HardwareCapabilities::RVB
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

        // SiFive P670 (High-performance)
        self.processors.push(ProcessorSpec {
            name: "SiFive P670".into(),
            vendor: "SiFive".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::ATOMICS
                | HardwareCapabilities::RVV
                | HardwareCapabilities::RVB
                | HardwareCapabilities::RVK
                | HardwareCapabilities::SIMD,
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

        // T-Head C910 (Allwinner D1)
        self.processors.push(ProcessorSpec {
            name: "T-Head C910".into(),
            vendor: "T-Head".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::ATOMICS
                | HardwareCapabilities::RVV
                | HardwareCapabilities::SIMD,
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

        // T-Head C920 (High-performance)
        self.processors.push(ProcessorSpec {
            name: "T-Head C920".into(),
            vendor: "T-Head".into(),
            architecture: Architecture::RiscV64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::ATOMICS
                | HardwareCapabilities::RVV
                | HardwareCapabilities::RVB
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

        // StarFive JH7100 (BeagleV)
        self.processors.push(ProcessorSpec {
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
        // Apple M3 Max
        self.processors.push(ProcessorSpec {
            name: "Apple M3 Max".into(),
            vendor: "Apple".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
            physical_cores: 16,
            logical_cores: 16,
            base_freq_mhz: 2400,
            max_freq_mhz: 4050,
            l1d_per_core_kb: 128,
            l2_per_core_kb: 16384,
            l3_total_kb: 0, // Unified memory architecture
            cache_line_bytes: 128,
            tdp_watts: 40,
        });

        // Apple M2
        self.processors.push(ProcessorSpec {
            name: "Apple M2".into(),
            vendor: "Apple".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Apple M1
        self.processors.push(ProcessorSpec {
            name: "Apple M1".into(),
            vendor: "Apple".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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
        // Loongson 3A6000 (LoongArch)
        self.processors.push(ProcessorSpec {
            name: "Loongson 3A6000".into(),
            vendor: "Loongson".into(),
            architecture: Architecture::LoongArch64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::SIMD
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Loongson 3A5000 (LoongArch)
        self.processors.push(ProcessorSpec {
            name: "Loongson 3A5000".into(),
            vendor: "Loongson".into(),
            architecture: Architecture::LoongArch64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::SIMD
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

        // Loongson 3C5000 (Server, LoongArch)
        self.processors.push(ProcessorSpec {
            name: "Loongson 3C5000".into(),
            vendor: "Loongson".into(),
            architecture: Architecture::LoongArch64,
            capabilities: HardwareCapabilities::FPU
                | HardwareCapabilities::SIMD
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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
        // NVIDIA Jetson AGX Orin
        self.processors.push(ProcessorSpec {
            name: "NVIDIA Jetson AGX Orin".into(),
            vendor: "NVIDIA".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // NVIDIA Jetson Xavier NX
        self.processors.push(ProcessorSpec {
            name: "NVIDIA Jetson Xavier NX".into(),
            vendor: "NVIDIA".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // NVIDIA Jetson Orin Nano
        self.processors.push(ProcessorSpec {
            name: "NVIDIA Jetson Orin Nano".into(),
            vendor: "NVIDIA".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // NVIDIA Jetson Nano
        self.processors.push(ProcessorSpec {
            name: "NVIDIA Jetson Nano".into(),
            vendor: "NVIDIA".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
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
        // Samsung Exynos 2400
        self.processors.push(ProcessorSpec {
            name: "Samsung Exynos 2400".into(),
            vendor: "Samsung".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Samsung Exynos 2200
        self.processors.push(ProcessorSpec {
            name: "Samsung Exynos 2200".into(),
            vendor: "Samsung".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Google Tensor G3
        self.processors.push(ProcessorSpec {
            name: "Google Tensor G3".into(),
            vendor: "Google".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Google Tensor G2
        self.processors.push(ProcessorSpec {
            name: "Google Tensor G2".into(),
            vendor: "Google".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // HiSilicon Kirin 9000S
        self.processors.push(ProcessorSpec {
            name: "HiSilicon Kirin 9000S".into(),
            vendor: "HiSilicon".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

        // Rockchip RK3588
        self.processors.push(ProcessorSpec {
            name: "Rockchip RK3588".into(),
            vendor: "Rockchip".into(),
            architecture: Architecture::AArch64,
            capabilities: HardwareCapabilities::NEON
                | HardwareCapabilities::FPU
                | HardwareCapabilities::CRYPTO
                | HardwareCapabilities::ATOMICS,
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

    fn add_embedded_processors(&mut self) {
        // ESP32-S3 specific model
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

        // ESP32-C6 specific model
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

        // STM32H743 (High-performance)
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

        // STM32F407 (Popular mid-range)
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

        // STM32F103 (Very popular entry-level)
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

        // NXP i.MX RT1062
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

        // Raspberry Pi RP2040
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

        // Nordic nRF52840
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

        // Texas Instruments CC2652
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

        // Microchip SAMD51
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

        // Renesas RA6M4
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

        // GigaDevice GD32VF103 (RISC-V)
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

        // Kendryte K210 (RISC-V)
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

impl Default for HardwareDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Get a summary of all processors in the database
pub fn database_summary() -> String {
    let db = HardwareDatabase::new();
    let mut summary = String::new();

    summary.push_str(&alloc::format!(
        "Hardware Database: {} processors\n\n",
        db.processors.len()
    ));

    // Group by vendor
    let vendors = [
        "Intel",
        "AMD",
        "ARM",
        "AWS",
        "Apple",
        "Qualcomm",
        "MediaTek",
        "Ampere",
        "StarFive",
        "SiFive",
        "T-Head",
        "Loongson",
        "NVIDIA",
        "Samsung",
        "Google",
        "HiSilicon",
        "Rockchip",
        "Espressif",
        "STMicroelectronics",
        "NXP",
        "Raspberry Pi",
    ];

    for vendor in &vendors {
        let procs = db.find_by_vendor(vendor);
        if !procs.is_empty() {
            summary.push_str(&alloc::format!(
                "{} ({} processors):\n",
                vendor,
                procs.len()
            ));
            for proc in procs {
                summary.push_str(&alloc::format!(
                    "  - {} ({} cores, {}-{} MHz, L3: {} KB)\n",
                    proc.name,
                    proc.physical_cores,
                    proc.base_freq_mhz,
                    proc.max_freq_mhz,
                    proc.l3_total_kb
                ));
            }
            summary.push('\n');
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_creation() {
        let db = HardwareDatabase::new();
        assert!(!db.processors.is_empty());
        // Verify we have 50+ processors as per the TODO goal
        assert!(
            db.processors.len() >= 50,
            "Database should have at least 50 processors, found {}",
            db.processors.len()
        );
    }

    #[test]
    fn test_database_count() {
        let db = HardwareDatabase::new();
        let count = db.processors.len();

        // Count by vendor
        let intel_count = db.find_by_vendor("Intel").len();
        let amd_count = db.find_by_vendor("AMD").len();
        let arm_count = db.find_by_vendor("ARM").len();
        let qualcomm_count = db.find_by_vendor("Qualcomm").len();
        let mediatek_count = db.find_by_vendor("MediaTek").len();
        let apple_count = db.find_by_vendor("Apple").len();

        assert!(count >= 50, "Total processors: {}", count);
        assert!(intel_count >= 10, "Intel processors: {}", intel_count);
        assert!(amd_count >= 10, "AMD processors: {}", amd_count);
        assert!(arm_count >= 10, "ARM processors: {}", arm_count);
        assert!(
            qualcomm_count >= 4,
            "Qualcomm processors: {}",
            qualcomm_count
        );
        assert!(
            mediatek_count >= 3,
            "MediaTek processors: {}",
            mediatek_count
        );
        assert_eq!(apple_count, 3, "Apple processors: {}", apple_count);
    }

    #[test]
    fn test_find_by_name() {
        let db = HardwareDatabase::new();
        let proc = db.find_by_name("i9-13900K");
        assert!(proc.is_some());
        assert_eq!(proc.expect("proc exists").vendor, "Intel");
    }

    #[test]
    fn test_find_by_vendor() {
        let db = HardwareDatabase::new();
        let intel_procs = db.find_by_vendor("Intel");
        assert!(!intel_procs.is_empty());
        assert!(intel_procs.iter().all(|p| p.vendor == "Intel"));
    }

    #[test]
    fn test_find_by_architecture() {
        let db = HardwareDatabase::new();
        let x86_procs = db.find_by_architecture(Architecture::X86_64);
        assert!(!x86_procs.is_empty());
        assert!(x86_procs
            .iter()
            .all(|p| p.architecture == Architecture::X86_64));
    }

    #[test]
    fn test_find_with_capabilities() {
        let db = HardwareDatabase::new();
        let avx512_procs = db.find_with_capabilities(HardwareCapabilities::AVX512);
        assert!(!avx512_procs.is_empty());
        for proc in avx512_procs {
            assert!(proc.capabilities.contains(HardwareCapabilities::AVX512));
        }
    }

    #[test]
    fn test_cache_score() {
        let db = HardwareDatabase::new();
        let proc = db.find_by_name("M3 Max").expect("M3 Max exists");
        let score = proc.cache_score();
        assert!(score > 0.0);
    }

    #[test]
    fn test_performance_score() {
        let db = HardwareDatabase::new();
        let proc1 = db.find_by_name("i9-13900K").expect("i9 exists");
        let proc2 = db.find_by_name("Ryzen 7 5800X").expect("Ryzen exists");

        let score1 = proc1.performance_score();
        let score2 = proc2.performance_score();

        assert!(score1 > 0.0);
        assert!(score2 > 0.0);
        // i9-13900K should score higher (more cores, higher freq)
        assert!(score1 > score2);
    }

    #[test]
    fn test_database_summary() {
        let summary = database_summary();
        assert!(!summary.is_empty());
        assert!(summary.contains("Hardware Database"));
    }

    #[test]
    fn test_all_processors_have_valid_specs() {
        let db = HardwareDatabase::new();
        for proc in db.all_processors() {
            assert!(!proc.name.is_empty());
            assert!(!proc.vendor.is_empty());
            assert!(proc.physical_cores > 0);
            assert!(proc.logical_cores >= proc.physical_cores);
            assert!(proc.max_freq_mhz >= proc.base_freq_mhz);
            assert!(proc.cache_line_bytes > 0);
        }
    }

    #[test]
    fn test_apple_processors() {
        let db = HardwareDatabase::new();
        let apple_procs = db.find_by_vendor("Apple");
        assert_eq!(apple_procs.len(), 3); // M1, M2, M3 Max
        for proc in apple_procs {
            assert_eq!(proc.cache_line_bytes, 128); // Apple uses 128-byte cache lines
            assert!(proc.capabilities.contains(HardwareCapabilities::NEON));
        }
    }
}
