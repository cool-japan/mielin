//! GPU Information Detection
//!
//! Provides detection and enumeration of GPUs and their capabilities.
//! Supports NVIDIA, AMD, Intel, and Apple GPUs with compute capability detection.

extern crate alloc;

use crate::traits::{DeviceInfo, MemoryDevice, Named};
use alloc::string::String;
use alloc::vec::Vec;

/// GPU vendor identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    /// NVIDIA Corporation
    Nvidia,
    /// AMD/ATI
    Amd,
    /// Intel Corporation
    Intel,
    /// Apple (Metal)
    Apple,
    /// Qualcomm Adreno
    Qualcomm,
    /// ARM Mali
    Arm,
    /// Unknown vendor
    Unknown,
}

impl Named for GpuVendor {
    fn name(&self) -> &'static str {
        match self {
            GpuVendor::Nvidia => "NVIDIA",
            GpuVendor::Amd => "AMD",
            GpuVendor::Intel => "Intel",
            GpuVendor::Apple => "Apple",
            GpuVendor::Qualcomm => "Qualcomm",
            GpuVendor::Arm => "ARM",
            GpuVendor::Unknown => "Unknown",
        }
    }
}

/// GPU type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuType {
    /// Discrete GPU (dedicated graphics card)
    Discrete,
    /// Integrated GPU (on-die with CPU)
    Integrated,
    /// Virtual GPU (cloud/VM)
    Virtual,
    /// Unknown type
    Unknown,
}

/// Compute API support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComputeSupport {
    /// CUDA compute capability (e.g., 8.6 stored as 86)
    pub cuda_capability: Option<u32>,
    /// OpenCL version (e.g., 3.0 stored as 30)
    pub opencl_version: Option<u32>,
    /// Vulkan compute support
    pub vulkan_compute: bool,
    /// Metal compute support (Apple)
    pub metal: bool,
    /// DirectX compute shader version
    pub directx_compute: Option<u32>,
    /// ROCm support (AMD)
    pub rocm: bool,
    /// oneAPI support (Intel)
    pub oneapi: bool,
}

impl ComputeSupport {
    /// Check if any compute API is supported
    pub fn has_compute(&self) -> bool {
        self.cuda_capability.is_some()
            || self.opencl_version.is_some()
            || self.vulkan_compute
            || self.metal
            || self.directx_compute.is_some()
            || self.rocm
            || self.oneapi
    }

    /// Get CUDA capability as float (e.g., 8.6)
    pub fn cuda_capability_float(&self) -> Option<f32> {
        self.cuda_capability.map(|c| c as f32 / 10.0)
    }

    /// Get OpenCL version as float (e.g., 3.0)
    pub fn opencl_version_float(&self) -> Option<f32> {
        self.opencl_version.map(|v| v as f32 / 10.0)
    }
}

/// GPU memory information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GpuMemory {
    /// Total memory in bytes
    pub total_bytes: u64,
    /// Memory bus width in bits
    pub bus_width_bits: u32,
    /// Memory type (GDDR6, HBM2, etc.)
    pub memory_type: MemoryType,
    /// Memory bandwidth in GB/s (stored as MB/s)
    pub bandwidth_mbps: u32,
}

impl GpuMemory {
    /// Get total memory in megabytes
    pub fn total_mb(&self) -> u64 {
        self.total_bytes / (1024 * 1024)
    }

    /// Get total memory in gigabytes
    pub fn total_gb(&self) -> u64 {
        self.total_bytes / (1024 * 1024 * 1024)
    }

    /// Get bandwidth in GB/s
    pub fn bandwidth_gbps(&self) -> f32 {
        self.bandwidth_mbps as f32 / 1000.0
    }
}

/// GPU memory type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemoryType {
    /// Unknown memory type
    #[default]
    Unknown,
    /// DDR memory (older/integrated)
    Ddr,
    /// GDDR5 memory
    Gddr5,
    /// GDDR5X memory
    Gddr5x,
    /// GDDR6 memory
    Gddr6,
    /// GDDR6X memory
    Gddr6x,
    /// HBM (High Bandwidth Memory)
    Hbm,
    /// HBM2 memory
    Hbm2,
    /// HBM2e memory
    Hbm2e,
    /// HBM3 memory
    Hbm3,
    /// Unified memory (Apple)
    Unified,
}

impl MemoryType {
    /// Get memory type name
    pub fn name(&self) -> &'static str {
        match self {
            MemoryType::Unknown => "Unknown",
            MemoryType::Ddr => "DDR",
            MemoryType::Gddr5 => "GDDR5",
            MemoryType::Gddr5x => "GDDR5X",
            MemoryType::Gddr6 => "GDDR6",
            MemoryType::Gddr6x => "GDDR6X",
            MemoryType::Hbm => "HBM",
            MemoryType::Hbm2 => "HBM2",
            MemoryType::Hbm2e => "HBM2e",
            MemoryType::Hbm3 => "HBM3",
            MemoryType::Unified => "Unified",
        }
    }
}

/// GPU performance metrics
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GpuPerformance {
    /// Base clock frequency in MHz
    pub base_clock_mhz: u32,
    /// Boost clock frequency in MHz
    pub boost_clock_mhz: u32,
    /// Number of compute units/SMs
    pub compute_units: u32,
    /// Number of shader processors
    pub shader_processors: u32,
    /// Tensor cores (NVIDIA) / Matrix cores (AMD)
    pub tensor_cores: u32,
    /// Ray tracing cores
    pub rt_cores: u32,
    /// TDP (Thermal Design Power) in watts
    pub tdp_watts: u32,
    /// FP32 compute performance in TFLOPS
    pub fp32_tflops: f32,
    /// FP16 compute performance in TFLOPS
    pub fp16_tflops: f32,
    /// INT8 compute performance in TOPS
    pub int8_tops: f32,
}

impl GpuPerformance {
    /// Check if GPU has tensor/matrix acceleration
    pub fn has_tensor_acceleration(&self) -> bool {
        self.tensor_cores > 0
    }

    /// Check if GPU has ray tracing hardware
    pub fn has_ray_tracing(&self) -> bool {
        self.rt_cores > 0
    }
}

/// Complete GPU information
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU vendor
    pub vendor: GpuVendor,
    /// GPU type (discrete/integrated)
    pub gpu_type: GpuType,
    /// GPU model name
    pub model: String,
    /// GPU architecture (e.g., "Ada Lovelace", "RDNA 3")
    pub architecture: String,
    /// PCI device ID
    pub device_id: u32,
    /// Device index (for multi-GPU systems)
    pub device_index: u32,
    /// Memory information
    pub memory: GpuMemory,
    /// Compute API support
    pub compute: ComputeSupport,
    /// Performance metrics
    pub performance: GpuPerformance,
    /// Whether GPU is currently available for use
    pub available: bool,
}

impl GpuInfo {
    /// Create a new GPU info with basic details
    pub fn new(vendor: GpuVendor, model: String, device_index: u32) -> Self {
        Self {
            vendor,
            gpu_type: GpuType::Unknown,
            model,
            architecture: String::new(),
            device_id: 0,
            device_index,
            memory: GpuMemory::default(),
            compute: ComputeSupport::default(),
            performance: GpuPerformance::default(),
            available: true,
        }
    }

    /// Check if GPU supports compute workloads
    pub fn supports_compute(&self) -> bool {
        self.compute.has_compute()
    }

    /// Check if GPU is suitable for ML/AI workloads
    pub fn suitable_for_ml(&self) -> bool {
        self.compute.has_compute()
            && (self.performance.has_tensor_acceleration()
                || self.performance.fp16_tflops > 0.0
                || self.performance.int8_tops > 0.0)
    }

    /// Get a short description of the GPU
    pub fn description(&self) -> String {
        alloc::format!(
            "{} {} ({} MB)",
            self.vendor.name(),
            self.model,
            self.memory.total_mb()
        )
    }
}

// Implement common traits for GpuInfo
impl DeviceInfo for GpuInfo {
    fn vendor_name(&self) -> &'static str {
        self.vendor.name()
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn device_index(&self) -> u32 {
        self.device_index
    }
}

impl MemoryDevice for GpuInfo {
    fn memory_bytes(&self) -> u64 {
        self.memory.total_bytes
    }

    fn memory_bandwidth_mbps(&self) -> u32 {
        self.memory.bandwidth_mbps
    }
}

/// GPU list container
pub struct GpuList {
    gpus: Vec<GpuInfo>,
}

impl GpuList {
    /// Create an empty GPU list
    pub fn new() -> Self {
        Self { gpus: Vec::new() }
    }

    /// Add a GPU to the list
    pub fn add(&mut self, gpu: GpuInfo) {
        self.gpus.push(gpu);
    }

    /// Get number of GPUs
    pub fn count(&self) -> usize {
        self.gpus.len()
    }

    /// Check if any GPUs are present
    pub fn is_empty(&self) -> bool {
        self.gpus.is_empty()
    }

    /// Get GPU by index
    pub fn get(&self, index: usize) -> Option<&GpuInfo> {
        self.gpus.get(index)
    }

    /// Iterate over all GPUs
    pub fn iter(&self) -> impl Iterator<Item = &GpuInfo> {
        self.gpus.iter()
    }

    /// Filter GPUs by vendor
    pub fn by_vendor(&self, vendor: GpuVendor) -> impl Iterator<Item = &GpuInfo> {
        self.gpus.iter().filter(move |g| g.vendor == vendor)
    }

    /// Filter GPUs by type
    pub fn by_type(&self, gpu_type: GpuType) -> impl Iterator<Item = &GpuInfo> {
        self.gpus.iter().filter(move |g| g.gpu_type == gpu_type)
    }

    /// Get only discrete GPUs
    pub fn discrete(&self) -> impl Iterator<Item = &GpuInfo> {
        self.by_type(GpuType::Discrete)
    }

    /// Get only integrated GPUs
    pub fn integrated(&self) -> impl Iterator<Item = &GpuInfo> {
        self.by_type(GpuType::Integrated)
    }

    /// Get GPUs with compute support
    pub fn with_compute(&self) -> impl Iterator<Item = &GpuInfo> {
        self.gpus.iter().filter(|g| g.supports_compute())
    }

    /// Get GPUs suitable for ML workloads
    pub fn ml_capable(&self) -> impl Iterator<Item = &GpuInfo> {
        self.gpus.iter().filter(|g| g.suitable_for_ml())
    }

    /// Get the most powerful GPU (by FP32 TFLOPS)
    pub fn most_powerful(&self) -> Option<&GpuInfo> {
        self.gpus.iter().max_by(|a, b| {
            a.performance
                .fp32_tflops
                .partial_cmp(&b.performance.fp32_tflops)
                .unwrap_or(core::cmp::Ordering::Equal)
        })
    }

    /// Get total GPU memory across all GPUs
    pub fn total_memory_bytes(&self) -> u64 {
        self.gpus.iter().map(|g| g.memory.total_bytes).sum()
    }

    /// Get total FP32 TFLOPS across all GPUs
    pub fn total_fp32_tflops(&self) -> f32 {
        self.gpus.iter().map(|g| g.performance.fp32_tflops).sum()
    }
}

impl Default for GpuList {
    fn default() -> Self {
        Self::new()
    }
}

/// GPU system summary
#[derive(Debug, Clone, Default)]
pub struct GpuSummary {
    /// Total number of GPUs
    pub gpu_count: usize,
    /// Number of discrete GPUs
    pub discrete_count: usize,
    /// Number of integrated GPUs
    pub integrated_count: usize,
    /// Total GPU memory in bytes
    pub total_memory_bytes: u64,
    /// Total FP32 compute in TFLOPS
    pub total_fp32_tflops: f32,
    /// Has CUDA support
    pub has_cuda: bool,
    /// Has ROCm support
    pub has_rocm: bool,
    /// Has Metal support
    pub has_metal: bool,
    /// Has OpenCL support
    pub has_opencl: bool,
    /// Has Vulkan compute support
    pub has_vulkan_compute: bool,
}

impl GpuSummary {
    /// Create summary from GPU list
    pub fn from_list(list: &GpuList) -> Self {
        // Compute boolean flags
        let mut has_cuda = false;
        let mut has_rocm = false;
        let mut has_metal = false;
        let mut has_opencl = false;
        let mut has_vulkan_compute = false;

        for gpu in list.iter() {
            if gpu.compute.cuda_capability.is_some() {
                has_cuda = true;
            }
            if gpu.compute.rocm {
                has_rocm = true;
            }
            if gpu.compute.metal {
                has_metal = true;
            }
            if gpu.compute.opencl_version.is_some() {
                has_opencl = true;
            }
            if gpu.compute.vulkan_compute {
                has_vulkan_compute = true;
            }
        }

        Self {
            gpu_count: list.count(),
            discrete_count: list.discrete().count(),
            integrated_count: list.integrated().count(),
            total_memory_bytes: list.total_memory_bytes(),
            total_fp32_tflops: list.total_fp32_tflops(),
            has_cuda,
            has_rocm,
            has_metal,
            has_opencl,
            has_vulkan_compute,
        }
    }

    /// Check if any GPU compute is available
    pub fn has_gpu_compute(&self) -> bool {
        self.has_cuda
            || self.has_rocm
            || self.has_metal
            || self.has_opencl
            || self.has_vulkan_compute
    }
}

// =============================================================================
// GPU Detection Implementation
// =============================================================================

/// Detect all GPUs on the system
pub fn detect_gpus() -> GpuList {
    let mut list = GpuList::new();

    // Platform-specific detection
    #[cfg(target_os = "linux")]
    detect_linux_gpus(&mut list);

    #[cfg(target_os = "macos")]
    detect_macos_gpus(&mut list);

    #[cfg(target_os = "windows")]
    detect_windows_gpus(&mut list);

    // If no GPUs detected, try to add a generic integrated GPU
    if list.is_empty() {
        if let Some(gpu) = detect_integrated_gpu() {
            list.add(gpu);
        }
    }

    list
}

/// Get GPU system summary
pub fn get_gpu_summary() -> GpuSummary {
    GpuSummary::from_list(&detect_gpus())
}

// Platform-specific detection stubs

#[cfg(target_os = "linux")]
fn detect_linux_gpus(list: &mut GpuList) {
    // Try to detect NVIDIA GPUs via sysfs
    detect_nvidia_sysfs(list);

    // Try to detect AMD GPUs via sysfs
    detect_amd_sysfs(list);

    // Try to detect Intel GPUs
    detect_intel_linux(list);
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn detect_nvidia_sysfs(list: &mut GpuList) {
    // In real implementation, would read from:
    // /sys/bus/pci/devices/*/vendor (0x10de for NVIDIA)
    // /sys/bus/pci/devices/*/device
    // /proc/driver/nvidia/gpus/*/information

    // For now, create stub that could be populated via runtime detection
    let _ = list; // Suppress unused warning
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn detect_amd_sysfs(list: &mut GpuList) {
    // Would read from:
    // /sys/bus/pci/devices/*/vendor (0x1002 for AMD)
    // /sys/class/drm/card*/device/
    let _ = list;
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn detect_intel_linux(list: &mut GpuList) {
    // Would read from:
    // /sys/bus/pci/devices/*/vendor (0x8086 for Intel)
    let _ = list;
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn detect_macos_gpus(list: &mut GpuList) {
    // On macOS, would use IOKit or Metal API
    // For Apple Silicon, report the integrated GPU
    #[cfg(target_arch = "aarch64")]
    {
        let mut gpu = GpuInfo::new(GpuVendor::Apple, String::from("Apple GPU"), 0);
        gpu.gpu_type = GpuType::Integrated;
        gpu.architecture = String::from("Apple Silicon");
        gpu.compute = ComputeSupport {
            metal: true,
            ..Default::default()
        };
        gpu.memory = GpuMemory {
            memory_type: MemoryType::Unified,
            ..Default::default()
        };
        list.add(gpu);
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn detect_windows_gpus(list: &mut GpuList) {
    // Would use DXGI or WMI for GPU enumeration
    let _ = list;
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn detect_linux_gpus(_list: &mut GpuList) {}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn detect_macos_gpus(_list: &mut GpuList) {}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn detect_windows_gpus(_list: &mut GpuList) {}

/// Try to detect an integrated GPU
#[allow(dead_code)]
fn detect_integrated_gpu() -> Option<GpuInfo> {
    // Check for common integrated GPUs based on CPU architecture
    #[cfg(target_arch = "x86_64")]
    {
        // Could check CPUID for Intel HD Graphics or AMD APU
        None
    }

    #[cfg(target_arch = "aarch64")]
    {
        // ARM platforms often have Mali or Adreno
        None
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    None
}

// =============================================================================
// Known GPU Specifications
// =============================================================================

/// Create GPU info for NVIDIA GeForce RTX 4090
#[allow(dead_code)]
pub fn nvidia_rtx_4090() -> GpuInfo {
    GpuInfo {
        vendor: GpuVendor::Nvidia,
        gpu_type: GpuType::Discrete,
        model: String::from("GeForce RTX 4090"),
        architecture: String::from("Ada Lovelace"),
        device_id: 0x2684,
        device_index: 0,
        memory: GpuMemory {
            total_bytes: 24 * 1024 * 1024 * 1024, // 24 GB
            bus_width_bits: 384,
            memory_type: MemoryType::Gddr6x,
            bandwidth_mbps: 1008 * 1000, // 1008 GB/s
        },
        compute: ComputeSupport {
            cuda_capability: Some(89), // 8.9
            opencl_version: Some(30),  // 3.0
            vulkan_compute: true,
            directx_compute: Some(12),
            ..Default::default()
        },
        performance: GpuPerformance {
            base_clock_mhz: 2235,
            boost_clock_mhz: 2520,
            compute_units: 128, // 128 SMs
            shader_processors: 16384,
            tensor_cores: 512,
            rt_cores: 128,
            tdp_watts: 450,
            fp32_tflops: 82.6,
            fp16_tflops: 165.2,
            int8_tops: 660.6,
        },
        available: true,
    }
}

/// Create GPU info for AMD Radeon RX 7900 XTX
#[allow(dead_code)]
pub fn amd_rx_7900_xtx() -> GpuInfo {
    GpuInfo {
        vendor: GpuVendor::Amd,
        gpu_type: GpuType::Discrete,
        model: String::from("Radeon RX 7900 XTX"),
        architecture: String::from("RDNA 3"),
        device_id: 0x744C,
        device_index: 0,
        memory: GpuMemory {
            total_bytes: 24 * 1024 * 1024 * 1024, // 24 GB
            bus_width_bits: 384,
            memory_type: MemoryType::Gddr6,
            bandwidth_mbps: 960 * 1000, // 960 GB/s
        },
        compute: ComputeSupport {
            opencl_version: Some(21), // 2.1
            vulkan_compute: true,
            rocm: true,
            ..Default::default()
        },
        performance: GpuPerformance {
            base_clock_mhz: 1855,
            boost_clock_mhz: 2499,
            compute_units: 96,
            shader_processors: 6144,
            tensor_cores: 0, // AI accelerators in RDNA3 work differently
            rt_cores: 96,    // Ray accelerators
            tdp_watts: 355,
            fp32_tflops: 61.0,
            fp16_tflops: 122.0,
            int8_tops: 244.0,
        },
        available: true,
    }
}

/// Create GPU info for Intel Arc A770
#[allow(dead_code)]
pub fn intel_arc_a770() -> GpuInfo {
    GpuInfo {
        vendor: GpuVendor::Intel,
        gpu_type: GpuType::Discrete,
        model: String::from("Arc A770"),
        architecture: String::from("Alchemist"),
        device_id: 0x56A0,
        device_index: 0,
        memory: GpuMemory {
            total_bytes: 16 * 1024 * 1024 * 1024, // 16 GB
            bus_width_bits: 256,
            memory_type: MemoryType::Gddr6,
            bandwidth_mbps: 560 * 1000, // 560 GB/s
        },
        compute: ComputeSupport {
            opencl_version: Some(30), // 3.0
            vulkan_compute: true,
            oneapi: true,
            ..Default::default()
        },
        performance: GpuPerformance {
            base_clock_mhz: 2100,
            boost_clock_mhz: 2400,
            compute_units: 32, // 32 Xe cores
            shader_processors: 4096,
            tensor_cores: 512, // XMX units
            rt_cores: 32,
            tdp_watts: 225,
            fp32_tflops: 19.66,
            fp16_tflops: 39.32,
            int8_tops: 157.3,
        },
        available: true,
    }
}

/// Create GPU info for Apple M3 Max GPU
#[allow(dead_code)]
pub fn apple_m3_max_gpu() -> GpuInfo {
    GpuInfo {
        vendor: GpuVendor::Apple,
        gpu_type: GpuType::Integrated,
        model: String::from("M3 Max GPU"),
        architecture: String::from("Apple GPU (3rd gen)"),
        device_id: 0,
        device_index: 0,
        memory: GpuMemory {
            total_bytes: 0, // Unified memory, shared with CPU
            bus_width_bits: 0,
            memory_type: MemoryType::Unified,
            bandwidth_mbps: 400 * 1000, // ~400 GB/s unified memory
        },
        compute: ComputeSupport {
            metal: true,
            ..Default::default()
        },
        performance: GpuPerformance {
            base_clock_mhz: 0,
            boost_clock_mhz: 0,
            compute_units: 40, // 40 GPU cores
            shader_processors: 5120,
            tensor_cores: 0, // Neural Engine is separate
            rt_cores: 40,    // Ray tracing accelerators
            tdp_watts: 0,    // Part of SoC
            fp32_tflops: 14.2,
            fp16_tflops: 28.4,
            int8_tops: 0.0, // Neural Engine handles this
        },
        available: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_gpu_vendor() {
        assert_eq!(GpuVendor::Nvidia.name(), "NVIDIA");
        assert_eq!(GpuVendor::Amd.name(), "AMD");
        assert_eq!(GpuVendor::Intel.name(), "Intel");
        assert_eq!(GpuVendor::Apple.name(), "Apple");
    }

    #[test]
    fn test_memory_type() {
        assert_eq!(MemoryType::Gddr6.name(), "GDDR6");
        assert_eq!(MemoryType::Hbm2.name(), "HBM2");
        assert_eq!(MemoryType::Unified.name(), "Unified");
    }

    #[test]
    fn test_compute_support_has_compute() {
        let no_compute = ComputeSupport::default();
        assert!(!no_compute.has_compute());

        let cuda = ComputeSupport {
            cuda_capability: Some(89),
            ..Default::default()
        };
        assert!(cuda.has_compute());

        let metal = ComputeSupport {
            metal: true,
            ..Default::default()
        };
        assert!(metal.has_compute());
    }

    #[test]
    fn test_compute_support_versions() {
        let compute = ComputeSupport {
            cuda_capability: Some(86),
            opencl_version: Some(30),
            ..Default::default()
        };

        assert_eq!(compute.cuda_capability_float(), Some(8.6));
        assert_eq!(compute.opencl_version_float(), Some(3.0));
    }

    #[test]
    fn test_gpu_memory() {
        let memory = GpuMemory {
            total_bytes: 24 * 1024 * 1024 * 1024,
            bus_width_bits: 384,
            memory_type: MemoryType::Gddr6x,
            bandwidth_mbps: 1008 * 1000,
        };

        assert_eq!(memory.total_gb(), 24);
        assert_eq!(memory.total_mb(), 24 * 1024);
        assert!((memory.bandwidth_gbps() - 1008.0).abs() < 0.1);
    }

    #[test]
    fn test_gpu_performance() {
        let perf = GpuPerformance {
            tensor_cores: 512,
            rt_cores: 128,
            ..Default::default()
        };

        assert!(perf.has_tensor_acceleration());
        assert!(perf.has_ray_tracing());

        let basic = GpuPerformance::default();
        assert!(!basic.has_tensor_acceleration());
        assert!(!basic.has_ray_tracing());
    }

    #[test]
    fn test_gpu_info_new() {
        let gpu = GpuInfo::new(GpuVendor::Nvidia, std::string::String::from("Test GPU"), 0);

        assert_eq!(gpu.vendor, GpuVendor::Nvidia);
        assert_eq!(gpu.model, "Test GPU");
        assert_eq!(gpu.device_index, 0);
        assert!(gpu.available);
    }

    #[test]
    fn test_gpu_info_supports_compute() {
        let mut gpu = GpuInfo::new(GpuVendor::Nvidia, std::string::String::from("Test"), 0);
        assert!(!gpu.supports_compute());

        gpu.compute.cuda_capability = Some(89);
        assert!(gpu.supports_compute());
    }

    #[test]
    fn test_gpu_info_suitable_for_ml() {
        let mut gpu = GpuInfo::new(GpuVendor::Nvidia, std::string::String::from("Test"), 0);
        assert!(!gpu.suitable_for_ml());

        gpu.compute.cuda_capability = Some(89);
        gpu.performance.tensor_cores = 512;
        assert!(gpu.suitable_for_ml());
    }

    #[test]
    fn test_gpu_list_operations() {
        let mut list = GpuList::new();
        assert!(list.is_empty());
        assert_eq!(list.count(), 0);

        let gpu1 = nvidia_rtx_4090();
        let gpu2 = amd_rx_7900_xtx();

        list.add(gpu1);
        list.add(gpu2);

        assert_eq!(list.count(), 2);
        assert!(!list.is_empty());
    }

    #[test]
    fn test_gpu_list_by_vendor() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());
        list.add(amd_rx_7900_xtx());
        list.add(intel_arc_a770());

        assert_eq!(list.by_vendor(GpuVendor::Nvidia).count(), 1);
        assert_eq!(list.by_vendor(GpuVendor::Amd).count(), 1);
        assert_eq!(list.by_vendor(GpuVendor::Intel).count(), 1);
        assert_eq!(list.by_vendor(GpuVendor::Apple).count(), 0);
    }

    #[test]
    fn test_gpu_list_discrete() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());
        list.add(apple_m3_max_gpu());

        assert_eq!(list.discrete().count(), 1);
        assert_eq!(list.integrated().count(), 1);
    }

    #[test]
    fn test_gpu_list_with_compute() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());
        list.add(amd_rx_7900_xtx());

        assert_eq!(list.with_compute().count(), 2);
    }

    #[test]
    fn test_gpu_list_ml_capable() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());

        assert_eq!(list.ml_capable().count(), 1);
    }

    #[test]
    fn test_gpu_list_most_powerful() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());
        list.add(amd_rx_7900_xtx());
        list.add(intel_arc_a770());

        let most_powerful = list
            .most_powerful()
            .expect("list should have most powerful GPU");
        assert_eq!(most_powerful.vendor, GpuVendor::Nvidia);
    }

    #[test]
    fn test_gpu_list_totals() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());
        list.add(amd_rx_7900_xtx());

        assert_eq!(list.total_memory_bytes(), 48 * 1024 * 1024 * 1024);
        assert!(list.total_fp32_tflops() > 140.0);
    }

    #[test]
    fn test_gpu_summary() {
        let mut list = GpuList::new();
        list.add(nvidia_rtx_4090());
        list.add(apple_m3_max_gpu());

        let summary = GpuSummary::from_list(&list);

        assert_eq!(summary.gpu_count, 2);
        assert_eq!(summary.discrete_count, 1);
        assert_eq!(summary.integrated_count, 1);
        assert!(summary.has_cuda);
        assert!(summary.has_metal);
        assert!(summary.has_gpu_compute());
    }

    #[test]
    fn test_detect_gpus() {
        let gpus = detect_gpus();
        // Should run without panic, may or may not find GPUs
        let _ = gpus.count();
    }

    #[test]
    fn test_get_gpu_summary() {
        let summary = get_gpu_summary();
        // Should run without panic
        let _ = summary.has_gpu_compute();
    }

    #[test]
    fn test_known_gpus_rtx_4090() {
        let gpu = nvidia_rtx_4090();

        assert_eq!(gpu.vendor, GpuVendor::Nvidia);
        assert_eq!(gpu.gpu_type, GpuType::Discrete);
        assert!(gpu.model.contains("4090"));
        assert_eq!(gpu.memory.total_gb(), 24);
        assert!(gpu.compute.cuda_capability.is_some());
        assert!(gpu.performance.tensor_cores > 0);
    }

    #[test]
    fn test_known_gpus_rx_7900_xtx() {
        let gpu = amd_rx_7900_xtx();

        assert_eq!(gpu.vendor, GpuVendor::Amd);
        assert!(gpu.model.contains("7900"));
        assert!(gpu.compute.rocm);
        assert!(gpu.compute.vulkan_compute);
    }

    #[test]
    fn test_known_gpus_arc_a770() {
        let gpu = intel_arc_a770();

        assert_eq!(gpu.vendor, GpuVendor::Intel);
        assert!(gpu.model.contains("A770"));
        assert!(gpu.compute.oneapi);
    }

    #[test]
    fn test_known_gpus_m3_max() {
        let gpu = apple_m3_max_gpu();

        assert_eq!(gpu.vendor, GpuVendor::Apple);
        assert_eq!(gpu.gpu_type, GpuType::Integrated);
        assert!(gpu.compute.metal);
        assert_eq!(gpu.memory.memory_type, MemoryType::Unified);
    }
}
