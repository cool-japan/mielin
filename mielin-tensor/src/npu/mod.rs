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
            assert!(!Tensor::is_npu_supported("conv2d", &ctx));
        }
    }
}
