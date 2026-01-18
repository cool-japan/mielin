//! AI Accelerator Detection
//!
//! Provides detection and enumeration of Neural Processing Units (NPUs),
//! Tensor Processing Units (TPUs), and other AI accelerators.
//!
//! ## Supported Accelerators
//!
//! - Apple Neural Engine (ANE)
//! - Google Edge TPU
//! - Qualcomm Hexagon NPU
//! - Intel Neural Compute Stick
//! - ARM Ethos NPU
//! - Generic NPU detection
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_hal::accelerator::{detect_accelerators, AcceleratorType};
//!
//! let accelerators = detect_accelerators();
//! for acc in &accelerators {
//!     println!("{}: {} TOPS", acc.name(), acc.compute_tops());
//! }
//! ```

use crate::traits::{DeviceInfo, Named};

// Note: AtomicBool reserved for future caching of detection results
#[allow(unused_imports)]
use core::sync::atomic::AtomicBool;

/// AI accelerator vendor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorVendor {
    /// Apple (Neural Engine)
    Apple,
    /// Google (Edge TPU, Cloud TPU)
    Google,
    /// Qualcomm (Hexagon NPU)
    Qualcomm,
    /// Intel (Movidius, Neural Compute Stick)
    Intel,
    /// ARM (Ethos)
    Arm,
    /// NVIDIA (Tensor Cores)
    Nvidia,
    /// AMD (RDNA AI)
    Amd,
    /// MediaTek (APU)
    MediaTek,
    /// Samsung (Exynos NPU)
    Samsung,
    /// Unknown vendor
    Unknown,
}

impl Named for AcceleratorVendor {
    fn name(&self) -> &'static str {
        match self {
            AcceleratorVendor::Apple => "Apple",
            AcceleratorVendor::Google => "Google",
            AcceleratorVendor::Qualcomm => "Qualcomm",
            AcceleratorVendor::Intel => "Intel",
            AcceleratorVendor::Arm => "ARM",
            AcceleratorVendor::Nvidia => "NVIDIA",
            AcceleratorVendor::Amd => "AMD",
            AcceleratorVendor::MediaTek => "MediaTek",
            AcceleratorVendor::Samsung => "Samsung",
            AcceleratorVendor::Unknown => "Unknown",
        }
    }
}

/// Type of AI accelerator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorType {
    /// Neural Processing Unit (general-purpose AI)
    Npu,
    /// Tensor Processing Unit (matrix operations)
    Tpu,
    /// GPU with tensor cores
    GpuTensorCore,
    /// Digital Signal Processor with AI extensions
    DspAi,
    /// Vision Processing Unit
    Vpu,
    /// Custom ASIC for AI
    CustomAsic,
    /// FPGA-based accelerator
    Fpga,
}

impl Named for AcceleratorType {
    fn name(&self) -> &'static str {
        match self {
            AcceleratorType::Npu => "NPU",
            AcceleratorType::Tpu => "TPU",
            AcceleratorType::GpuTensorCore => "GPU Tensor Core",
            AcceleratorType::DspAi => "DSP AI",
            AcceleratorType::Vpu => "VPU",
            AcceleratorType::CustomAsic => "Custom ASIC",
            AcceleratorType::Fpga => "FPGA",
        }
    }
}

/// Data type support for accelerator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataTypeSupport {
    /// FP32 (32-bit floating point)
    pub fp32: bool,
    /// FP16 (16-bit floating point)
    pub fp16: bool,
    /// BF16 (Brain Float 16)
    pub bf16: bool,
    /// INT8 (8-bit integer)
    pub int8: bool,
    /// INT4 (4-bit integer)
    pub int4: bool,
    /// Binary (1-bit)
    pub binary: bool,
}

impl DataTypeSupport {
    /// Full precision support only
    pub const FP32_ONLY: Self = Self {
        fp32: true,
        fp16: false,
        bf16: false,
        int8: false,
        int4: false,
        binary: false,
    };

    /// Common ML accelerator support (FP16, INT8)
    pub const COMMON_ML: Self = Self {
        fp32: true,
        fp16: true,
        bf16: false,
        int8: true,
        int4: false,
        binary: false,
    };

    /// Full quantization support
    pub const FULL_QUANTIZED: Self = Self {
        fp32: true,
        fp16: true,
        bf16: true,
        int8: true,
        int4: true,
        binary: true,
    };

    /// Check if any low-precision format is supported
    pub fn supports_quantized(&self) -> bool {
        self.int8 || self.int4 || self.binary
    }

    /// Check if any reduced precision float is supported
    pub fn supports_reduced_float(&self) -> bool {
        self.fp16 || self.bf16
    }
}

impl Default for DataTypeSupport {
    fn default() -> Self {
        Self::COMMON_ML
    }
}

/// Accelerator capabilities
#[derive(Debug, Clone, Copy)]
pub struct AcceleratorCapabilities {
    /// Peak compute in TOPS (Tera Operations Per Second)
    pub compute_tops: f32,
    /// Memory bandwidth in GB/s
    pub memory_bandwidth_gbps: f32,
    /// On-chip SRAM in KB
    pub sram_kb: u32,
    /// Maximum batch size
    pub max_batch_size: u32,
    /// Supported data types
    pub data_types: DataTypeSupport,
    /// Supports dynamic shapes
    pub dynamic_shapes: bool,
    /// Supports custom operators
    pub custom_ops: bool,
    /// Power efficiency in TOPS/W
    pub tops_per_watt: f32,
}

impl Default for AcceleratorCapabilities {
    fn default() -> Self {
        Self {
            compute_tops: 0.0,
            memory_bandwidth_gbps: 0.0,
            sram_kb: 0,
            max_batch_size: 1,
            data_types: DataTypeSupport::default(),
            dynamic_shapes: false,
            custom_ops: false,
            tops_per_watt: 0.0,
        }
    }
}

/// AI accelerator information
#[derive(Debug, Clone)]
pub struct Accelerator {
    /// Vendor
    pub vendor: AcceleratorVendor,
    /// Type
    pub accel_type: AcceleratorType,
    /// Model name
    pub model: &'static str,
    /// Generation/version
    pub generation: u8,
    /// Capabilities
    pub capabilities: AcceleratorCapabilities,
    /// Device index (for multiple accelerators)
    pub device_index: u8,
    /// Is currently available
    pub available: bool,
}

impl Accelerator {
    /// Create a new accelerator entry
    pub const fn new(
        vendor: AcceleratorVendor,
        accel_type: AcceleratorType,
        model: &'static str,
        generation: u8,
    ) -> Self {
        Self {
            vendor,
            accel_type,
            model,
            generation,
            capabilities: AcceleratorCapabilities {
                compute_tops: 0.0,
                memory_bandwidth_gbps: 0.0,
                sram_kb: 0,
                max_batch_size: 1,
                data_types: DataTypeSupport::COMMON_ML,
                dynamic_shapes: false,
                custom_ops: false,
                tops_per_watt: 0.0,
            },
            device_index: 0,
            available: false,
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        self.model
    }

    /// Get compute in TOPS
    pub fn compute_tops(&self) -> f32 {
        self.capabilities.compute_tops
    }

    /// Check if accelerator supports a given data type
    pub fn supports_fp16(&self) -> bool {
        self.capabilities.data_types.fp16
    }

    /// Check if accelerator supports INT8 quantization
    pub fn supports_int8(&self) -> bool {
        self.capabilities.data_types.int8
    }

    /// Check if accelerator is suitable for inference
    pub fn is_inference_capable(&self) -> bool {
        self.available && self.capabilities.compute_tops > 0.0
    }

    /// Check if accelerator is suitable for training
    pub fn is_training_capable(&self) -> bool {
        self.available && self.capabilities.compute_tops > 1.0 && self.capabilities.data_types.fp32
    }
}

// Implement common traits for Accelerator
impl DeviceInfo for Accelerator {
    fn vendor_name(&self) -> &'static str {
        self.vendor.name()
    }

    fn model_name(&self) -> &str {
        self.model
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn device_index(&self) -> u32 {
        self.device_index as u32
    }
}

/// Apple Neural Engine specifications
#[allow(dead_code)]
fn apple_neural_engine() -> Accelerator {
    Accelerator {
        vendor: AcceleratorVendor::Apple,
        accel_type: AcceleratorType::Npu,
        model: "Apple Neural Engine",
        generation: 5, // M3/A17
        capabilities: AcceleratorCapabilities {
            compute_tops: 35.0, // A17 Pro
            memory_bandwidth_gbps: 100.0,
            sram_kb: 32768, // 32MB
            max_batch_size: 16,
            data_types: DataTypeSupport {
                fp32: true,
                fp16: true,
                bf16: false,
                int8: true,
                int4: false,
                binary: false,
            },
            dynamic_shapes: true,
            custom_ops: true,
            tops_per_watt: 5.0,
        },
        device_index: 0,
        available: false,
    }
}

/// Google Edge TPU specifications
#[allow(dead_code)]
fn google_edge_tpu() -> Accelerator {
    Accelerator {
        vendor: AcceleratorVendor::Google,
        accel_type: AcceleratorType::Tpu,
        model: "Google Edge TPU",
        generation: 2,
        capabilities: AcceleratorCapabilities {
            compute_tops: 4.0,
            memory_bandwidth_gbps: 8.0,
            sram_kb: 8192, // 8MB
            max_batch_size: 1,
            data_types: DataTypeSupport {
                fp32: false,
                fp16: false,
                bf16: false,
                int8: true,
                int4: false,
                binary: false,
            },
            dynamic_shapes: false,
            custom_ops: false,
            tops_per_watt: 2.0,
        },
        device_index: 0,
        available: false,
    }
}

/// Qualcomm Hexagon NPU specifications
#[allow(dead_code)]
fn qualcomm_hexagon_npu() -> Accelerator {
    Accelerator {
        vendor: AcceleratorVendor::Qualcomm,
        accel_type: AcceleratorType::Npu,
        model: "Qualcomm Hexagon NPU",
        generation: 8, // Snapdragon 8 Gen 3
        capabilities: AcceleratorCapabilities {
            compute_tops: 45.0,
            memory_bandwidth_gbps: 77.0,
            sram_kb: 16384, // 16MB
            max_batch_size: 8,
            data_types: DataTypeSupport {
                fp32: true,
                fp16: true,
                bf16: false,
                int8: true,
                int4: true,
                binary: false,
            },
            dynamic_shapes: true,
            custom_ops: true,
            tops_per_watt: 4.5,
        },
        device_index: 0,
        available: false,
    }
}

/// ARM Ethos NPU specifications
#[allow(dead_code)]
fn arm_ethos_npu() -> Accelerator {
    Accelerator {
        vendor: AcceleratorVendor::Arm,
        accel_type: AcceleratorType::Npu,
        model: "ARM Ethos-U85",
        generation: 3,
        capabilities: AcceleratorCapabilities {
            compute_tops: 4.0,
            memory_bandwidth_gbps: 4.0,
            sram_kb: 512,
            max_batch_size: 1,
            data_types: DataTypeSupport {
                fp32: false,
                fp16: true,
                bf16: false,
                int8: true,
                int4: true,
                binary: false,
            },
            dynamic_shapes: false,
            custom_ops: false,
            tops_per_watt: 8.0, // Very efficient
        },
        device_index: 0,
        available: false,
    }
}

/// Intel Neural Compute Stick specifications
#[allow(dead_code)]
fn intel_neural_compute() -> Accelerator {
    Accelerator {
        vendor: AcceleratorVendor::Intel,
        accel_type: AcceleratorType::Vpu,
        model: "Intel Movidius Myriad X",
        generation: 2,
        capabilities: AcceleratorCapabilities {
            compute_tops: 4.0,
            memory_bandwidth_gbps: 3.2,
            sram_kb: 2048, // 2MB
            max_batch_size: 4,
            data_types: DataTypeSupport {
                fp32: false,
                fp16: true,
                bf16: false,
                int8: true,
                int4: false,
                binary: false,
            },
            dynamic_shapes: true,
            custom_ops: true,
            tops_per_watt: 1.0,
        },
        device_index: 0,
        available: false,
    }
}

/// Maximum number of accelerators to track
const MAX_ACCELERATORS: usize = 8;

/// Detect accelerators on the current platform
///
/// This function probes for available AI accelerators and returns
/// information about each one found.
pub fn detect_accelerators() -> AcceleratorList {
    let mut list = AcceleratorList::new();

    // Platform-specific detection
    #[cfg(target_os = "macos")]
    {
        // Check for Apple Neural Engine
        let mut ane = apple_neural_engine();
        ane.available = detect_apple_neural_engine();
        if ane.available {
            list.add(ane);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Check for Google Edge TPU
        let mut edge_tpu = google_edge_tpu();
        edge_tpu.available = detect_edge_tpu();
        if edge_tpu.available {
            list.add(edge_tpu);
        }

        // Check for Intel Neural Compute
        let mut intel_ncs = intel_neural_compute();
        intel_ncs.available = detect_intel_ncs();
        if intel_ncs.available {
            list.add(intel_ncs);
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // Check for Qualcomm Hexagon
        let mut hexagon = qualcomm_hexagon_npu();
        hexagon.available = detect_qualcomm_hexagon();
        if hexagon.available {
            list.add(hexagon);
        }

        // Check for ARM Ethos
        let mut ethos = arm_ethos_npu();
        ethos.available = detect_arm_ethos();
        if ethos.available {
            list.add(ethos);
        }
    }

    list
}

/// List of detected accelerators
#[derive(Debug, Clone)]
pub struct AcceleratorList {
    storage: [Option<Accelerator>; MAX_ACCELERATORS],
    count: usize,
}

impl AcceleratorList {
    /// Create an empty list
    pub fn new() -> Self {
        Self {
            storage: [None, None, None, None, None, None, None, None],
            count: 0,
        }
    }

    /// Add an accelerator to the list
    pub fn add(&mut self, accelerator: Accelerator) {
        if self.count < MAX_ACCELERATORS {
            self.storage[self.count] = Some(accelerator);
            self.count += 1;
        }
    }

    /// Get number of accelerators
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if list is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get accelerator by index
    pub fn get(&self, index: usize) -> Option<&Accelerator> {
        if index < self.count {
            self.storage[index].as_ref()
        } else {
            None
        }
    }

    /// Iterate over accelerators
    pub fn iter(&self) -> AcceleratorIter<'_> {
        AcceleratorIter {
            list: self,
            index: 0,
        }
    }

    /// Find accelerators by vendor
    pub fn by_vendor(&self, vendor: AcceleratorVendor) -> AcceleratorList {
        let mut result = AcceleratorList::new();
        for acc in self.iter() {
            if acc.vendor == vendor {
                result.add(acc.clone());
            }
        }
        result
    }

    /// Find accelerators by type
    pub fn by_type(&self, accel_type: AcceleratorType) -> AcceleratorList {
        let mut result = AcceleratorList::new();
        for acc in self.iter() {
            if acc.accel_type == accel_type {
                result.add(acc.clone());
            }
        }
        result
    }

    /// Get total compute capacity in TOPS
    pub fn total_tops(&self) -> f32 {
        self.iter()
            .filter(|a| a.available)
            .map(|a| a.capabilities.compute_tops)
            .sum()
    }

    /// Find the most powerful accelerator
    pub fn most_powerful(&self) -> Option<&Accelerator> {
        self.iter().filter(|a| a.available).max_by(|a, b| {
            a.capabilities
                .compute_tops
                .partial_cmp(&b.capabilities.compute_tops)
                .unwrap_or(core::cmp::Ordering::Equal)
        })
    }

    /// Find the most efficient accelerator (TOPS/W)
    pub fn most_efficient(&self) -> Option<&Accelerator> {
        self.iter().filter(|a| a.available).max_by(|a, b| {
            a.capabilities
                .tops_per_watt
                .partial_cmp(&b.capabilities.tops_per_watt)
                .unwrap_or(core::cmp::Ordering::Equal)
        })
    }
}

impl Default for AcceleratorList {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over accelerators
pub struct AcceleratorIter<'a> {
    list: &'a AcceleratorList,
    index: usize,
}

impl<'a> Iterator for AcceleratorIter<'a> {
    type Item = &'a Accelerator;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.list.count {
            let item = self.list.storage[self.index].as_ref();
            self.index += 1;
            item
        } else {
            None
        }
    }
}

// Platform-specific detection functions

/// Detect Apple Neural Engine (macOS/iOS)
#[cfg(target_os = "macos")]
fn detect_apple_neural_engine() -> bool {
    // In a real implementation, we'd use IOKit to query for ANE
    // For now, we check if we're on Apple Silicon
    #[cfg(target_arch = "aarch64")]
    {
        true // Apple Silicon always has ANE
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false // Intel Macs don't have ANE
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn detect_apple_neural_engine() -> bool {
    false
}

/// Detect Google Edge TPU
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn detect_edge_tpu() -> bool {
    // In a real implementation, we'd check for /dev/apex_0 or USB device
    // Check for Coral USB device (Google Edge TPU)
    false // Stub - would check /sys/bus/usb/devices for 1a6e:089a or 18d1:9302
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
fn detect_edge_tpu() -> bool {
    false
}

/// Detect Intel Neural Compute Stick
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn detect_intel_ncs() -> bool {
    // In a real implementation, we'd check for Myriad device
    false // Stub - would check /sys/bus/usb/devices for 03e7:2485
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
fn detect_intel_ncs() -> bool {
    false
}

/// Detect Qualcomm Hexagon NPU
#[cfg(target_arch = "aarch64")]
fn detect_qualcomm_hexagon() -> bool {
    // In a real implementation, we'd check for Hexagon DSP
    // Check /sys/devices/platform/ for DSP nodes
    false // Stub
}

#[cfg(not(target_arch = "aarch64"))]
#[allow(dead_code)]
fn detect_qualcomm_hexagon() -> bool {
    false
}

/// Detect ARM Ethos NPU
#[cfg(target_arch = "aarch64")]
fn detect_arm_ethos() -> bool {
    // In a real implementation, we'd check for Ethos driver
    // Check /sys/class/ethosu/
    false // Stub
}

#[cfg(not(target_arch = "aarch64"))]
#[allow(dead_code)]
fn detect_arm_ethos() -> bool {
    false
}

/// Summary of available AI acceleration
#[derive(Debug, Clone)]
pub struct AccelerationSummary {
    /// Total number of accelerators
    pub accelerator_count: usize,
    /// Total compute in TOPS
    pub total_tops: f32,
    /// Has any NPU
    pub has_npu: bool,
    /// Has any TPU
    pub has_tpu: bool,
    /// Has GPU tensor cores
    pub has_gpu_tensor: bool,
    /// Supports INT8 quantization
    pub supports_int8: bool,
    /// Supports FP16
    pub supports_fp16: bool,
}

impl AccelerationSummary {
    /// Create summary from accelerator list
    pub fn from_list(list: &AcceleratorList) -> Self {
        let mut summary = Self {
            accelerator_count: list.len(),
            total_tops: 0.0,
            has_npu: false,
            has_tpu: false,
            has_gpu_tensor: false,
            supports_int8: false,
            supports_fp16: false,
        };

        for acc in list.iter() {
            if acc.available {
                summary.total_tops += acc.capabilities.compute_tops;

                match acc.accel_type {
                    AcceleratorType::Npu => summary.has_npu = true,
                    AcceleratorType::Tpu => summary.has_tpu = true,
                    AcceleratorType::GpuTensorCore => summary.has_gpu_tensor = true,
                    _ => {}
                }

                if acc.capabilities.data_types.int8 {
                    summary.supports_int8 = true;
                }
                if acc.capabilities.data_types.fp16 {
                    summary.supports_fp16 = true;
                }
            }
        }

        summary
    }

    /// Check if any AI acceleration is available
    pub fn has_acceleration(&self) -> bool {
        self.accelerator_count > 0 && self.total_tops > 0.0
    }

    /// Get recommended inference precision
    pub fn recommended_precision(&self) -> &'static str {
        if self.supports_int8 {
            "INT8"
        } else if self.supports_fp16 {
            "FP16"
        } else {
            "FP32"
        }
    }
}

/// Get summary of available AI acceleration
pub fn get_acceleration_summary() -> AccelerationSummary {
    let list = detect_accelerators();
    AccelerationSummary::from_list(&list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accelerator_vendor_name() {
        assert_eq!(AcceleratorVendor::Apple.name(), "Apple");
        assert_eq!(AcceleratorVendor::Google.name(), "Google");
        assert_eq!(AcceleratorVendor::Qualcomm.name(), "Qualcomm");
    }

    #[test]
    fn test_accelerator_type_name() {
        assert_eq!(AcceleratorType::Npu.name(), "NPU");
        assert_eq!(AcceleratorType::Tpu.name(), "TPU");
        assert_eq!(AcceleratorType::GpuTensorCore.name(), "GPU Tensor Core");
    }

    #[test]
    fn test_data_type_support() {
        let fp32_only = DataTypeSupport::FP32_ONLY;
        assert!(fp32_only.fp32);
        assert!(!fp32_only.fp16);
        assert!(!fp32_only.supports_quantized());

        let common_ml = DataTypeSupport::COMMON_ML;
        assert!(common_ml.supports_quantized());
        assert!(common_ml.supports_reduced_float());

        let full = DataTypeSupport::FULL_QUANTIZED;
        assert!(full.int4);
        assert!(full.bf16);
    }

    #[test]
    fn test_accelerator_creation() {
        let acc = Accelerator::new(
            AcceleratorVendor::Apple,
            AcceleratorType::Npu,
            "Test NPU",
            1,
        );

        assert_eq!(acc.vendor, AcceleratorVendor::Apple);
        assert_eq!(acc.accel_type, AcceleratorType::Npu);
        assert_eq!(acc.name(), "Test NPU");
        assert_eq!(acc.generation, 1);
        assert!(!acc.available);
    }

    #[test]
    fn test_apple_neural_engine_specs() {
        let ane = apple_neural_engine();
        assert_eq!(ane.vendor, AcceleratorVendor::Apple);
        assert_eq!(ane.accel_type, AcceleratorType::Npu);
        assert!(ane.capabilities.compute_tops > 0.0);
        assert!(ane.capabilities.data_types.fp16);
    }

    #[test]
    fn test_google_edge_tpu_specs() {
        let tpu = google_edge_tpu();
        assert_eq!(tpu.vendor, AcceleratorVendor::Google);
        assert_eq!(tpu.accel_type, AcceleratorType::Tpu);
        assert!(tpu.capabilities.data_types.int8);
        assert!(!tpu.capabilities.data_types.fp32);
    }

    #[test]
    fn test_accelerator_list_operations() {
        let mut list = AcceleratorList::new();
        assert!(list.is_empty());

        let mut acc = apple_neural_engine();
        acc.available = true;
        list.add(acc);

        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());

        let retrieved = list.get(0).expect("list should have element at index 0");
        assert_eq!(retrieved.vendor, AcceleratorVendor::Apple);
    }

    #[test]
    fn test_accelerator_list_filtering() {
        let mut list = AcceleratorList::new();

        let mut ane = apple_neural_engine();
        ane.available = true;
        list.add(ane);

        let mut tpu = google_edge_tpu();
        tpu.available = true;
        list.add(tpu);

        let npus = list.by_type(AcceleratorType::Npu);
        assert_eq!(npus.len(), 1);

        let tpus = list.by_type(AcceleratorType::Tpu);
        assert_eq!(tpus.len(), 1);

        let apple_list = list.by_vendor(AcceleratorVendor::Apple);
        assert_eq!(apple_list.len(), 1);
    }

    #[test]
    fn test_accelerator_list_total_tops() {
        let mut list = AcceleratorList::new();

        let mut ane = apple_neural_engine();
        ane.available = true;
        list.add(ane);

        let mut tpu = google_edge_tpu();
        tpu.available = true;
        list.add(tpu);

        let total = list.total_tops();
        assert!(total > 35.0); // ANE is 35 TOPS, TPU is 4 TOPS
    }

    #[test]
    fn test_accelerator_list_most_powerful() {
        let mut list = AcceleratorList::new();

        let mut ane = apple_neural_engine();
        ane.available = true;
        list.add(ane);

        let mut tpu = google_edge_tpu();
        tpu.available = true;
        list.add(tpu);

        let most_powerful = list
            .most_powerful()
            .expect("list should have most powerful accelerator");
        assert_eq!(most_powerful.vendor, AcceleratorVendor::Apple);
    }

    #[test]
    fn test_accelerator_capabilities() {
        let ane = apple_neural_engine();
        assert!(ane.supports_fp16());
        assert!(ane.supports_int8());
    }

    #[test]
    fn test_inference_training_capable() {
        let mut acc = apple_neural_engine();
        assert!(!acc.is_inference_capable()); // Not available

        acc.available = true;
        assert!(acc.is_inference_capable());
        assert!(acc.is_training_capable());
    }

    #[test]
    fn test_acceleration_summary() {
        let mut list = AcceleratorList::new();

        let mut ane = apple_neural_engine();
        ane.available = true;
        list.add(ane);

        let summary = AccelerationSummary::from_list(&list);
        assert_eq!(summary.accelerator_count, 1);
        assert!(summary.has_npu);
        assert!(!summary.has_tpu);
        assert!(summary.supports_fp16);
        assert!(summary.has_acceleration());
    }

    #[test]
    fn test_recommended_precision() {
        let summary = AccelerationSummary {
            accelerator_count: 1,
            total_tops: 10.0,
            has_npu: true,
            has_tpu: false,
            has_gpu_tensor: false,
            supports_int8: true,
            supports_fp16: true,
        };

        assert_eq!(summary.recommended_precision(), "INT8");
    }

    #[test]
    fn test_detect_accelerators() {
        // This tests that detection runs without panic
        let list = detect_accelerators();
        // On most systems this will be empty unless we have actual hardware
        let _ = list.len();
    }

    #[test]
    fn test_get_acceleration_summary() {
        let summary = get_acceleration_summary();
        // Just check it returns something valid
        assert!(summary.accelerator_count < 100);
    }

    #[test]
    fn test_accelerator_iter() {
        let mut list = AcceleratorList::new();

        let mut ane = apple_neural_engine();
        ane.available = true;
        list.add(ane);

        let mut tpu = google_edge_tpu();
        tpu.available = true;
        list.add(tpu);

        let count = list.iter().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_arm_ethos_specs() {
        let ethos = arm_ethos_npu();
        assert_eq!(ethos.vendor, AcceleratorVendor::Arm);
        assert!(ethos.capabilities.tops_per_watt > 5.0); // Very efficient
    }

    #[test]
    fn test_qualcomm_hexagon_specs() {
        let hexagon = qualcomm_hexagon_npu();
        assert_eq!(hexagon.vendor, AcceleratorVendor::Qualcomm);
        assert!(hexagon.capabilities.compute_tops > 40.0);
        assert!(hexagon.capabilities.data_types.int4);
    }

    #[test]
    fn test_intel_ncs_specs() {
        let ncs = intel_neural_compute();
        assert_eq!(ncs.vendor, AcceleratorVendor::Intel);
        assert_eq!(ncs.accel_type, AcceleratorType::Vpu);
    }
}
