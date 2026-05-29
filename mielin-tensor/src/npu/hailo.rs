#![allow(dead_code)]
//! Hailo-8 NPU backend stub.
//!
//! Models the Hailo PCIe/M.2 edge AI accelerator. Real implementation would use
//! the Hailo RT library (HailoRT). This stub is pure Rust and device-probe-based.
//!
//! Detection is performed by checking for `/dev/hailo0` on Linux. On all other
//! platforms (or when the `std` feature is absent) the backend is unavailable and
//! `detect()` returns an error, causing `NpuContext::new()` to continue to the
//! next backend in the priority chain.

use crate::error::{TensorError, TensorResult};
use crate::npu::{NpuBackend, NpuDevice, NpuModel};
use crate::tensor::Tensor;
use alloc::vec::Vec;

/// Hailo-8 NPU backend.
///
/// Provides stub access to the Hailo RT (HailoRT) library interface. In a real
/// deployment the crate would link against `hailort` and use the C FFI bindings
/// exposed by the `hailort-sys` crate.
pub struct Hailo8Backend;

impl Hailo8Backend {
    /// Detect Hailo-8 hardware by probing `/dev/hailo0` on Linux.
    ///
    /// Returns `Ok(NpuDevice)` when the device node is present; returns
    /// `Err(TensorError)` on all other platforms or when the node is absent.
    pub fn detect() -> TensorResult<NpuDevice> {
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            if std::path::Path::new("/dev/hailo0").exists() {
                return Ok(NpuDevice {
                    backend: NpuBackend::Hailo8,
                    name: "Hailo-8",
                    supported_ops: &[
                        "conv",
                        "matmul",
                        "depthwise_conv",
                        "relu",
                        "sigmoid",
                        "pool",
                        "concat",
                    ],
                    max_model_size: 256 * 1024 * 1024, // 256 MB
                    performance_class: 8,              // high-performance edge AI
                });
            }
        }

        Err(TensorError::other("Hailo-8 device not found"))
    }

    /// Returns `true` if `/dev/hailo0` exists on Linux, `false` everywhere else.
    pub fn is_available() -> bool {
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            std::path::Path::new("/dev/hailo0").exists()
        }
        #[cfg(not(all(feature = "std", target_os = "linux")))]
        {
            false
        }
    }

    /// Compile a model for Hailo-8 execution.
    ///
    /// Stub: stores the raw bytes as-is. A real implementation would invoke the
    /// Hailo Model Compiler (HMC) to convert an ONNX or TFLite graph into a Hailo
    /// Executable Format (HEF) binary.
    pub fn compile(
        model_data: Vec<u8>,
        input_shapes: Vec<Vec<usize>>,
        output_shapes: Vec<Vec<usize>>,
    ) -> TensorResult<NpuModel> {
        // Stub: Accept any byte payload.  The real compiler would validate that the
        // model fits within the Hailo-8's on-chip SRAM budget and raise an error if
        // the HEF does not target a supported device type.
        Ok(NpuModel::new(
            NpuBackend::Hailo8,
            model_data,
            input_shapes,
            output_shapes,
        ))
    }

    /// Run inference on Hailo-8.
    ///
    /// Stub: returns zero-valued tensors whose shapes match `model.output_shapes()`.
    /// A real implementation would use `hailo_infer()` via the HailoRT C API,
    /// mapping input/output buffers into the device's DMA memory regions.
    pub fn infer(model: &NpuModel, inputs: &[Tensor<f32>]) -> TensorResult<Vec<Tensor<f32>>> {
        // Validate input count against model's declared input shapes.
        let expected = model.input_shapes().len();
        if inputs.len() != expected {
            return Err(TensorError::other(alloc::format!(
                "Hailo8: expected {} input tensor(s), got {}",
                expected,
                inputs.len()
            )));
        }

        let outputs: Vec<Tensor<f32>> = model
            .output_shapes()
            .iter()
            .map(|shape| Tensor::zeros(shape.clone()))
            .collect();

        Ok(outputs)
    }

    /// Return current performance metrics for the Hailo-8 device.
    ///
    /// Stub: returns zeroed metrics. A real implementation would query the
    /// HailoRT power and performance counters via `hailo_get_chip_temperature`
    /// and the profiling API.
    pub fn get_performance_metrics() -> Hailo8Metrics {
        Hailo8Metrics::default()
    }

    /// List all Hailo devices present in the system.
    ///
    /// Stub: probes `/dev/hailo0` through `/dev/hailo7`.  A real implementation
    /// would call `hailo_scan_devices()`.
    pub fn list_devices() -> Vec<HailoDeviceInfo> {
        let mut found = alloc::vec![];

        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            for idx in 0u8..8 {
                let path = alloc::format!("/dev/hailo{}", idx);
                if std::path::Path::new(&path).exists() {
                    found.push(HailoDeviceInfo {
                        device_index: idx,
                        device_type: HailoDeviceType::Hailo8,
                        firmware_version: "4.17.0",
                        neural_core_count: 8,
                    });
                }
            }
        }

        found
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Metrics and device-type types
// ─────────────────────────────────────────────────────────────────────────────

/// Performance metrics for Hailo-8 inference.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hailo8Metrics {
    /// Wall-clock inference time in microseconds.
    pub inference_time_us: u64,
    /// Instantaneous device power draw in milliwatts.
    pub power_mw: u32,
    /// Device variant that produced the metrics.
    pub device_type: HailoDeviceType,
    /// Effective throughput in TOPS achieved during the last inference.
    pub tops_achieved: f32,
}

/// Hailo device variants.
///
/// The Hailo product family spans PCIe cards, M.2 modules, and the integrated
/// Hailo-10 found in some embedded SoCs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HailoDeviceType {
    /// Hailo-8 — standard PCIe/M.2 edge AI accelerator (26 TOPS).
    #[default]
    Hailo8,
    /// Hailo-8L — low-power variant (13 TOPS).
    Hailo8L,
    /// Hailo-8R — automotive-grade variant.
    Hailo8R,
    /// Hailo-10 — next-generation device (40+ TOPS, integrated SoC).
    Hailo10,
    /// Hailo M.2 module (same silicon as Hailo-8, M.2 2242 form factor).
    HailoM2,
}

// ─────────────────────────────────────────────────────────────────────────────
// Device enumeration types
// ─────────────────────────────────────────────────────────────────────────────

/// Information about a single discovered Hailo device.
#[derive(Debug, Clone)]
pub struct HailoDeviceInfo {
    /// Zero-based device index (corresponds to `/dev/hailoN`).
    pub device_index: u8,
    /// Hardware variant.
    pub device_type: HailoDeviceType,
    /// Firmware version string reported by HailoRT.
    pub firmware_version: &'static str,
    /// Number of neural processing cores on the device.
    pub neural_core_count: u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// HEF (Hailo Executable Format) stub
// ─────────────────────────────────────────────────────────────────────────────

/// Stub representation of a Hailo Executable Format (HEF) binary.
///
/// In production this wraps the `hailort_hef` opaque pointer returned by
/// `hailo_create_hef_file`.  The network groups and stream information would be
/// read from the HEF metadata section.
#[derive(Debug, Clone)]
pub struct HailoHef {
    raw_bytes: Vec<u8>,
    network_group_name: &'static str,
}

impl HailoHef {
    /// Parse (stub) a HEF binary from raw bytes.
    pub fn from_bytes(data: Vec<u8>) -> TensorResult<Self> {
        // Stub: Accept any payload. Real parser would validate the HEF magic header.
        Ok(Self {
            raw_bytes: data,
            network_group_name: "default_hef_group",
        })
    }

    /// The name of the primary network group embedded in the HEF.
    pub fn network_group_name(&self) -> &'static str {
        self.network_group_name
    }

    /// Size of the raw HEF binary.
    pub fn size(&self) -> usize {
        self.raw_bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hailo_detect_no_device() {
        // On non-Linux or any host without /dev/hailo0 the backend must return Err.
        #[cfg(not(all(feature = "std", target_os = "linux")))]
        {
            let result = Hailo8Backend::detect();
            assert!(result.is_err());
        }
        // On Linux without the device node this also returns Err.
        // We cannot assert on Linux in CI since /dev/hailo0 might not exist;
        // instead we only assert that the function does not panic.
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            let _result = Hailo8Backend::detect();
        }
    }

    #[test]
    fn test_hailo_is_available_no_device() {
        // On non-Linux platforms the result must be false.
        #[cfg(not(all(feature = "std", target_os = "linux")))]
        {
            assert!(!Hailo8Backend::is_available());
        }
        // On Linux: function must not panic.
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            let _ = Hailo8Backend::is_available();
        }
    }

    #[test]
    fn test_hailo_compile_stub() {
        let model_data = alloc::vec![0xABu8, 0xCD, 0xEF, 0x00]; // arbitrary HEF bytes
        let result = Hailo8Backend::compile(
            model_data,
            alloc::vec![alloc::vec![1usize, 3, 640, 640]],
            alloc::vec![alloc::vec![1usize, 25200, 85]],
        );
        assert!(result.is_ok());
        let model = result.unwrap();
        assert_eq!(model.backend(), NpuBackend::Hailo8);
        assert_eq!(model.input_shapes().len(), 1);
        assert_eq!(model.output_shapes().len(), 1);
    }

    #[test]
    fn test_hailo_infer_stub() {
        let model = Hailo8Backend::compile(
            alloc::vec![0u8; 64],
            alloc::vec![alloc::vec![1usize, 3, 640, 640]],
            alloc::vec![alloc::vec![1usize, 25200, 85]],
        )
        .unwrap();

        let input = Tensor::zeros(alloc::vec![1usize, 3, 640, 640]);
        let outputs = Hailo8Backend::infer(&model, &[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1usize, 25200, 85]);
        // Stub always returns zeros.
        assert!(outputs[0].data().iter().all(|&v| v == 0.0f32));
    }

    #[test]
    fn test_hailo_infer_input_count_mismatch() {
        let model = Hailo8Backend::compile(
            alloc::vec![],
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        )
        .unwrap();

        // Wrong number of inputs.
        let result = Hailo8Backend::infer(&model, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_hailo_metrics() {
        let metrics = Hailo8Backend::get_performance_metrics();
        assert_eq!(metrics.inference_time_us, 0);
        assert_eq!(metrics.power_mw, 0);
        assert_eq!(metrics.tops_achieved, 0.0f32);
        assert_eq!(metrics.device_type, HailoDeviceType::Hailo8);
    }

    #[test]
    fn test_hailo_device_type_enum() {
        // Verify all HailoDeviceType variants are constructible and distinct.
        let types = [
            HailoDeviceType::Hailo8,
            HailoDeviceType::Hailo8L,
            HailoDeviceType::Hailo8R,
            HailoDeviceType::Hailo10,
            HailoDeviceType::HailoM2,
        ];
        assert_eq!(types.len(), 5);
        assert_eq!(HailoDeviceType::default(), HailoDeviceType::Hailo8);
        assert_ne!(HailoDeviceType::Hailo8, HailoDeviceType::Hailo8L);
        assert_ne!(HailoDeviceType::Hailo10, HailoDeviceType::HailoM2);
    }

    #[test]
    fn test_hailo_list_devices_no_device() {
        // On non-Linux the list must be empty.
        #[cfg(not(all(feature = "std", target_os = "linux")))]
        {
            let devices = Hailo8Backend::list_devices();
            assert!(devices.is_empty());
        }
        // On Linux: must not panic.
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            let _ = Hailo8Backend::list_devices();
        }
    }

    #[test]
    fn test_hailo_hef_stub() {
        let hef = HailoHef::from_bytes(alloc::vec![0xAAu8; 128]).unwrap();
        assert_eq!(hef.size(), 128);
        assert!(!hef.network_group_name().is_empty());
    }
}
