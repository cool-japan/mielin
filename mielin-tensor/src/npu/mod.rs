//! NPU (Neural Processing Unit) Integration
//!
//! Supports Apple Neural Engine, Google Edge TPU, and Qualcomm NPU.
//! Provides automatic hardware detection and fallback to CPU/GPU.

#![allow(unused)]

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::vec::Vec;

#[cfg(feature = "apple-neural-engine")]
pub mod apple;

#[cfg(feature = "edge-tpu")]
pub mod edgetpu;

#[cfg(feature = "qualcomm-npu")]
pub mod qualcomm;

#[cfg(feature = "onnxruntime")]
pub mod onnxruntime;

#[cfg(feature = "hailo")]
pub mod hailo;

/// NPU backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpuBackend {
    /// No NPU available
    None,
    /// Apple Neural Engine (ANE)
    #[cfg(feature = "apple-neural-engine")]
    AppleNeuralEngine,
    /// Google Edge TPU
    #[cfg(feature = "edge-tpu")]
    EdgeTpu,
    /// Qualcomm NPU (Hexagon)
    #[cfg(feature = "qualcomm-npu")]
    QualcommNpu,
    /// ONNX Runtime execution provider (software, always available when feature is on)
    #[cfg(feature = "onnxruntime")]
    OnnxRuntime,
    /// Hailo-8 edge AI accelerator
    #[cfg(feature = "hailo")]
    Hailo8,
}

/// NPU device information
#[derive(Debug, Clone)]
pub struct NpuDevice {
    /// Backend type
    pub backend: NpuBackend,
    /// Device name
    pub name: &'static str,
    /// Supported operations
    pub supported_ops: &'static [&'static str],
    /// Maximum model size
    pub max_model_size: usize,
    /// Performance class (0-10, higher is better)
    pub performance_class: u8,
}

impl NpuDevice {
    /// Create a CPU fallback device
    pub fn cpu() -> Self {
        Self {
            backend: NpuBackend::None,
            name: "CPU (no NPU)",
            supported_ops: &[],
            max_model_size: 1_073_741_824, // 1GB for CPU emulation
            performance_class: 0,
        }
    }
}

/// NPU model representation
pub struct NpuModel {
    backend: NpuBackend,
    model_data: Vec<u8>,
    input_shapes: Vec<Vec<usize>>,
    output_shapes: Vec<Vec<usize>>,
}

impl NpuModel {
    /// Create a new NPU model
    pub fn new(
        backend: NpuBackend,
        model_data: Vec<u8>,
        input_shapes: Vec<Vec<usize>>,
        output_shapes: Vec<Vec<usize>>,
    ) -> Self {
        Self {
            backend,
            model_data,
            input_shapes,
            output_shapes,
        }
    }

    /// Get backend type
    pub fn backend(&self) -> NpuBackend {
        self.backend
    }

    /// Get input shapes
    pub fn input_shapes(&self) -> &[Vec<usize>] {
        &self.input_shapes
    }

    /// Get output shapes
    pub fn output_shapes(&self) -> &[Vec<usize>] {
        &self.output_shapes
    }

    /// Get model size in bytes
    pub fn model_size(&self) -> usize {
        self.model_data.len()
    }
}

/// NPU context for managing devices and models
pub struct NpuContext {
    device: NpuDevice,
    models: Vec<NpuModel>,
}

impl NpuContext {
    /// Detect and initialize the best available NPU
    pub fn new() -> TensorResult<Self> {
        #[cfg(feature = "apple-neural-engine")]
        {
            if let Ok(device) = apple::AppleNeuralEngine::detect() {
                return Ok(Self {
                    device,
                    models: Vec::new(),
                });
            }
        }

        #[cfg(feature = "edge-tpu")]
        {
            if let Ok(device) = edgetpu::EdgeTpu::detect() {
                return Ok(Self {
                    device,
                    models: Vec::new(),
                });
            }
        }

        #[cfg(feature = "qualcomm-npu")]
        {
            if let Ok(device) = qualcomm::QualcommNpu::detect() {
                return Ok(Self {
                    device,
                    models: Vec::new(),
                });
            }
        }

        // Hailo-8 hardware probe (Linux only; hardware-specific, tried before software fallback)
        #[cfg(feature = "hailo")]
        {
            if let Ok(device) = hailo::Hailo8Backend::detect() {
                return Ok(Self {
                    device,
                    models: alloc::vec::Vec::new(),
                });
            }
        }

        // ONNX Runtime software fallback (always succeeds when feature is enabled)
        #[cfg(feature = "onnxruntime")]
        {
            if let Ok(device) = onnxruntime::OnnxRuntimeBackend::detect() {
                return Ok(Self {
                    device,
                    models: alloc::vec::Vec::new(),
                });
            }
        }

        // CPU fallback
        Ok(Self {
            device: NpuDevice::cpu(),
            models: Vec::new(),
        })
    }

    /// Get device information
    pub fn device(&self) -> &NpuDevice {
        &self.device
    }

    /// Check if NPU is available
    pub fn has_npu(&self) -> bool {
        self.device.backend != NpuBackend::None
    }

    /// Get backend type
    pub fn backend(&self) -> NpuBackend {
        self.device.backend
    }

    /// Load a model onto the NPU
    pub fn load_model(&mut self, model: NpuModel) -> TensorResult<usize> {
        if model.model_size() > self.device.max_model_size {
            return Err(TensorError::other("Model size exceeds NPU capacity"));
        }

        self.models.push(model);
        Ok(self.models.len() - 1)
    }

    /// Get a loaded model
    pub fn get_model(&self, id: usize) -> TensorResult<&NpuModel> {
        self.models
            .get(id)
            .ok_or_else(|| TensorError::other("Model ID not found"))
    }

    /// Unload a model
    pub fn unload_model(&mut self, id: usize) -> TensorResult<()> {
        if id >= self.models.len() {
            return Err(TensorError::other("Model ID not found"));
        }
        self.models.remove(id);
        Ok(())
    }

    /// Clear all models
    pub fn clear_models(&mut self) {
        self.models.clear();
    }
}

impl Default for NpuContext {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            device: NpuDevice::cpu(),
            models: Vec::new(),
        })
    }
}

/// NPU tensor operations trait
pub trait NpuOps {
    /// Run inference on NPU
    fn npu_infer(&self, ctx: &mut NpuContext, model_id: usize) -> TensorResult<Vec<Tensor<f32>>>;

    /// Check if operation is supported on NPU
    fn is_npu_supported(op_name: &str, ctx: &NpuContext) -> bool;
}

impl NpuOps for Tensor<f32> {
    fn npu_infer(&self, ctx: &mut NpuContext, model_id: usize) -> TensorResult<Vec<Tensor<f32>>> {
        let model = ctx.get_model(model_id)?;

        match model.backend() {
            NpuBackend::None => {
                // CPU fallback
                Err(TensorError::other("No NPU available for inference"))
            }
            #[cfg(feature = "apple-neural-engine")]
            NpuBackend::AppleNeuralEngine => apple::AppleNeuralEngine::infer(self, model),
            #[cfg(feature = "edge-tpu")]
            NpuBackend::EdgeTpu => edgetpu::EdgeTpu::infer(self, model),
            #[cfg(feature = "qualcomm-npu")]
            NpuBackend::QualcommNpu => qualcomm::QualcommNpu::infer(self, model),
            #[cfg(feature = "onnxruntime")]
            NpuBackend::OnnxRuntime => {
                onnxruntime::OnnxRuntimeBackend::infer(model, core::slice::from_ref(self))
            }
            #[cfg(feature = "hailo")]
            NpuBackend::Hailo8 => {
                hailo::Hailo8Backend::infer(model, core::slice::from_ref(self))
            }
            #[allow(unreachable_patterns)]
            _ => Err(TensorError::other("Backend not compiled")),
        }
    }

    fn is_npu_supported(op_name: &str, ctx: &NpuContext) -> bool {
        ctx.device().supported_ops.contains(&op_name)
    }
}

/// Compile a model for NPU execution
#[allow(unused_variables)]
pub fn compile_model(
    model_data: &[u8],
    backend: NpuBackend,
    input_shapes: Vec<Vec<usize>>,
    output_shapes: Vec<Vec<usize>>,
) -> TensorResult<NpuModel> {
    match backend {
        NpuBackend::None => Err(TensorError::other("Cannot compile for CPU")),
        #[cfg(feature = "apple-neural-engine")]
        NpuBackend::AppleNeuralEngine => {
            apple::AppleNeuralEngine::compile(model_data, input_shapes, output_shapes)
        }
        #[cfg(feature = "edge-tpu")]
        NpuBackend::EdgeTpu => edgetpu::EdgeTpu::compile(model_data, input_shapes, output_shapes),
        #[cfg(feature = "qualcomm-npu")]
        NpuBackend::QualcommNpu => {
            qualcomm::QualcommNpu::compile(model_data, input_shapes, output_shapes)
        }
        #[cfg(feature = "onnxruntime")]
        NpuBackend::OnnxRuntime => {
            onnxruntime::OnnxRuntimeBackend::compile(
                model_data.to_vec(),
                input_shapes,
                output_shapes,
            )
        }
        #[cfg(feature = "hailo")]
        NpuBackend::Hailo8 => {
            hailo::Hailo8Backend::compile(model_data.to_vec(), input_shapes, output_shapes)
        }
        #[allow(unreachable_patterns)]
        _ => Err(TensorError::other("Backend not compiled")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npu_backend() {
        assert_eq!(NpuBackend::None, NpuBackend::None);
    }

    #[test]
    fn test_npu_device_cpu() {
        let device = NpuDevice::cpu();
        assert_eq!(device.backend, NpuBackend::None);
        assert_eq!(device.name, "CPU (no NPU)");
        assert_eq!(device.performance_class, 0);
    }

    #[test]
    fn test_npu_context() {
        let ctx = NpuContext::new().unwrap();
        // On Apple Silicon, ANE will be detected
        // On other platforms, this may be None
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            assert_eq!(ctx.device().backend, NpuBackend::AppleNeuralEngine);
            assert!(ctx.has_npu());
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            // May be None or another backend depending on system
            let _ = ctx.device().backend;
        }
    }

    #[test]
    fn test_npu_model() {
        let model = NpuModel::new(
            NpuBackend::None,
            alloc::vec![0u8; 100],
            alloc::vec![alloc::vec![1, 3, 224, 224]],
            alloc::vec![alloc::vec![1, 1000]],
        );

        assert_eq!(model.backend(), NpuBackend::None);
        assert_eq!(model.model_size(), 100);
        assert_eq!(model.input_shapes().len(), 1);
        assert_eq!(model.output_shapes().len(), 1);
    }

    #[test]
    fn test_npu_context_model_management() {
        let mut ctx = NpuContext::new().unwrap();
        assert_eq!(ctx.models.len(), 0);

        let model = NpuModel::new(
            NpuBackend::None,
            alloc::vec![0u8; 100],
            alloc::vec![alloc::vec![1, 3, 224, 224]],
            alloc::vec![alloc::vec![1, 1000]],
        );

        let id = ctx.load_model(model).unwrap();
        assert_eq!(id, 0);
        assert_eq!(ctx.models.len(), 1);

        let loaded = ctx.get_model(id).unwrap();
        assert_eq!(loaded.backend(), NpuBackend::None);

        ctx.unload_model(id).unwrap();
        assert_eq!(ctx.models.len(), 0);
    }

    #[test]
    fn test_npu_context_clear_models() {
        let mut ctx = NpuContext::new().unwrap();

        for _ in 0..3 {
            let model = NpuModel::new(
                NpuBackend::None,
                alloc::vec![0u8; 100],
                alloc::vec![alloc::vec![1, 3, 224, 224]],
                alloc::vec![alloc::vec![1, 1000]],
            );
            ctx.load_model(model).unwrap();
        }

        assert_eq!(ctx.models.len(), 3);
        ctx.clear_models();
        assert_eq!(ctx.models.len(), 0);
    }

    #[test]
    fn test_is_npu_supported() {
        let ctx = NpuContext::new().unwrap();
        // On Apple Silicon with ANE, NPU operations may be supported
        // On other platforms, NPU is typically not available
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            // ANE may or may not support specific operations - just verify it doesn't crash
            let _supported = Tensor::is_npu_supported("conv2d", &ctx);
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        {
            // When onnxruntime feature is on, the context uses ONNX Runtime, which only
            // advertises its own op names (e.g. "conv"), not "conv2d".
            // When no feature is on, CPU fallback has no supported_ops.
            assert!(!Tensor::is_npu_supported("conv2d", &ctx));
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ONNX Runtime backend tests
    // ─────────────────────────────────────────────────────────────────────────

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnxruntime_detect_always_succeeds() {
        let result = onnxruntime::OnnxRuntimeBackend::detect();
        assert!(result.is_ok());
        let device = result.unwrap();
        assert_eq!(device.backend, NpuBackend::OnnxRuntime);
    }

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnxruntime_is_available() {
        assert!(onnxruntime::OnnxRuntimeBackend::is_available());
    }

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnxruntime_device_info() {
        let device = onnxruntime::OnnxRuntimeBackend::detect().unwrap();
        assert_eq!(device.name, "ONNX Runtime");
        assert_eq!(device.max_model_size, 4 * 1024 * 1024 * 1024);
        assert_eq!(device.performance_class, 4);
        assert!(!device.supported_ops.is_empty());
        // Must advertise "matmul" and "softmax" at minimum.
        assert!(device.supported_ops.contains(&"matmul"));
        assert!(device.supported_ops.contains(&"softmax"));
    }

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnxruntime_compile_model() {
        let model_data = alloc::vec![0x08u8, 0x01, 0x00, 0x00];
        let result = onnxruntime::OnnxRuntimeBackend::compile(
            model_data,
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        );
        assert!(result.is_ok());
        let model = result.unwrap();
        assert_eq!(model.backend(), NpuBackend::OnnxRuntime);
        assert_eq!(model.input_shapes().len(), 1);
        assert_eq!(model.output_shapes().len(), 1);
    }

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnxruntime_infer_stub() {
        let model = onnxruntime::OnnxRuntimeBackend::compile(
            alloc::vec![0x08u8],
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        )
        .unwrap();

        let input = Tensor::zeros(alloc::vec![1usize, 3, 224, 224]);
        let outputs = onnxruntime::OnnxRuntimeBackend::infer(&model, &[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1usize, 1000]);
        // Stub always returns zeros.
        assert!(outputs[0].data().iter().all(|&v| v == 0.0f32));
    }

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnxruntime_metrics() {
        let metrics = onnxruntime::OnnxRuntimeBackend::get_performance_metrics();
        assert_eq!(metrics.inference_time_us, 0);
        assert_eq!(metrics.memory_used_bytes, 0);
        assert_eq!(
            metrics.execution_provider,
            onnxruntime::OnnxExecutionProvider::Cpu
        );
    }

    #[cfg(feature = "onnxruntime")]
    #[test]
    fn test_onnx_execution_provider_enum() {
        use onnxruntime::OnnxExecutionProvider;
        let all_variants = [
            OnnxExecutionProvider::Cpu,
            OnnxExecutionProvider::Cuda,
            OnnxExecutionProvider::TensorRT,
            OnnxExecutionProvider::CoreML,
            OnnxExecutionProvider::DirectML,
            OnnxExecutionProvider::OpenVino,
        ];
        assert_eq!(all_variants.len(), 6);
        assert_eq!(OnnxExecutionProvider::default(), OnnxExecutionProvider::Cpu);
        // Spot-check distinctness.
        assert_ne!(OnnxExecutionProvider::Cuda, OnnxExecutionProvider::TensorRT);
        assert_ne!(OnnxExecutionProvider::CoreML, OnnxExecutionProvider::DirectML);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Hailo backend tests
    // ─────────────────────────────────────────────────────────────────────────

    #[cfg(feature = "hailo")]
    #[test]
    fn test_hailo_detect_no_device() {
        // On non-Linux (or Linux without /dev/hailo0), detect must return Err.
        #[cfg(not(all(feature = "std", target_os = "linux")))]
        {
            let result = hailo::Hailo8Backend::detect();
            assert!(result.is_err());
        }
        // On Linux: must not panic regardless of device presence.
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            let _result = hailo::Hailo8Backend::detect();
        }
    }

    #[cfg(feature = "hailo")]
    #[test]
    fn test_hailo_is_available_no_device() {
        #[cfg(not(all(feature = "std", target_os = "linux")))]
        {
            assert!(!hailo::Hailo8Backend::is_available());
        }
        #[cfg(all(feature = "std", target_os = "linux"))]
        {
            // Just verify no panic on Linux.
            let _ = hailo::Hailo8Backend::is_available();
        }
    }

    #[cfg(feature = "hailo")]
    #[test]
    fn test_hailo_compile_stub() {
        let result = hailo::Hailo8Backend::compile(
            alloc::vec![0xABu8, 0xCD, 0xEF],
            alloc::vec![alloc::vec![1usize, 3, 640, 640]],
            alloc::vec![alloc::vec![1usize, 25200, 85]],
        );
        assert!(result.is_ok());
        let model = result.unwrap();
        assert_eq!(model.backend(), NpuBackend::Hailo8);
    }

    #[cfg(feature = "hailo")]
    #[test]
    fn test_hailo_infer_stub() {
        let model = hailo::Hailo8Backend::compile(
            alloc::vec![0u8; 32],
            alloc::vec![alloc::vec![1usize, 3, 640, 640]],
            alloc::vec![alloc::vec![1usize, 25200, 85]],
        )
        .unwrap();

        let input = Tensor::zeros(alloc::vec![1usize, 3, 640, 640]);
        let outputs = hailo::Hailo8Backend::infer(&model, &[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1usize, 25200, 85]);
        assert!(outputs[0].data().iter().all(|&v| v == 0.0f32));
    }

    #[cfg(feature = "hailo")]
    #[test]
    fn test_hailo_metrics() {
        let metrics = hailo::Hailo8Backend::get_performance_metrics();
        assert_eq!(metrics.inference_time_us, 0);
        assert_eq!(metrics.power_mw, 0);
        assert_eq!(metrics.tops_achieved, 0.0f32);
        assert_eq!(metrics.device_type, hailo::HailoDeviceType::Hailo8);
    }

    #[cfg(feature = "hailo")]
    #[test]
    fn test_hailo_device_type_enum() {
        use hailo::HailoDeviceType;
        let all_variants = [
            HailoDeviceType::Hailo8,
            HailoDeviceType::Hailo8L,
            HailoDeviceType::Hailo8R,
            HailoDeviceType::Hailo10,
            HailoDeviceType::HailoM2,
        ];
        assert_eq!(all_variants.len(), 5);
        assert_eq!(HailoDeviceType::default(), HailoDeviceType::Hailo8);
        assert_ne!(HailoDeviceType::Hailo8, HailoDeviceType::Hailo8L);
        assert_ne!(HailoDeviceType::Hailo10, HailoDeviceType::HailoM2);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Detection order / priority tests
    // ─────────────────────────────────────────────────────────────────────────

    /// Verify that when only `onnxruntime` is compiled in (and no hardware
    /// accelerator features), NpuContext picks ONNX Runtime over CPU fallback.
    #[cfg(all(
        feature = "onnxruntime",
        not(feature = "apple-neural-engine"),
        not(feature = "edge-tpu"),
        not(feature = "qualcomm-npu"),
        not(feature = "hailo")
    ))]
    #[test]
    fn test_npu_backend_priority_order() {
        let ctx = NpuContext::new().unwrap();
        // With no hardware NPU feature enabled, ONNX Runtime should be selected.
        assert_eq!(ctx.device().backend, NpuBackend::OnnxRuntime);
        // ONNX Runtime reports itself as an NPU (performance_class > 0).
        assert!(ctx.device().performance_class > 0);
    }

    /// Verify that without any NPU feature, NpuContext falls back to the CPU device.
    #[cfg(not(any(
        feature = "apple-neural-engine",
        feature = "edge-tpu",
        feature = "qualcomm-npu",
        feature = "hailo",
        feature = "onnxruntime"
    )))]
    #[test]
    fn test_npu_context_cpu_fallback_still_works() {
        let ctx = NpuContext::new().unwrap();
        assert_eq!(ctx.device().backend, NpuBackend::None);
        assert_eq!(ctx.device().performance_class, 0);
        assert!(!ctx.has_npu());
    }
}
