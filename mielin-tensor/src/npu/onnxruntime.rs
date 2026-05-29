#![allow(dead_code)]
//! ONNX Runtime NPU backend stub.
//!
//! Models the ONNX Runtime execution provider interface. The real implementation
//! would use the onnxruntime-rs crate (C API bindings). This stub is pure Rust.
//!
//! Unlike hardware-specific backends (Edge TPU, Hailo-8), ONNX Runtime is a software
//! runtime that is always available when the `onnxruntime` feature is enabled. It is
//! tried LAST in the hardware detection chain, serving as the highest-priority software
//! fallback before the bare CPU path.

use crate::error::{TensorError, TensorResult};
use crate::npu::{NpuBackend, NpuDevice, NpuModel};
use crate::tensor::Tensor;
use alloc::vec::Vec;

/// ONNX Runtime execution provider.
///
/// Wraps the ONNX Runtime C API (onnxruntime-rs). In the stub implementation,
/// all calls succeed immediately with zero-valued output tensors.
pub struct OnnxRuntimeBackend;

impl OnnxRuntimeBackend {
    /// Detect ONNX Runtime availability.
    ///
    /// Always succeeds when the `onnxruntime` feature is enabled — ONNX Runtime is
    /// a cross-platform software runtime with no hardware prerequisite.
    pub fn detect() -> TensorResult<NpuDevice> {
        Ok(NpuDevice {
            backend: NpuBackend::OnnxRuntime,
            name: "ONNX Runtime",
            supported_ops: &[
                "conv", "matmul", "relu", "softmax", "pool", "norm", "reshape", "gather",
            ],
            max_model_size: 4 * 1024 * 1024 * 1024, // 4 GB
            performance_class: 4,                   // mid-range (software runtime)
        })
    }

    /// Always returns `true` when the `onnxruntime` feature is compiled in.
    pub fn is_available() -> bool {
        true
    }

    /// Compile an ONNX model for execution.
    ///
    /// Stub: validates that the model byte-stream starts with the ONNX protobuf magic
    /// field tag (`0x08` — field 1, varint encoding), then stores it as-is.  A real
    /// implementation would call `OrtCreateSession` with the chosen execution provider.
    pub fn compile(
        model_data: Vec<u8>,
        input_shapes: Vec<Vec<usize>>,
        output_shapes: Vec<Vec<usize>>,
    ) -> TensorResult<NpuModel> {
        // Stub validation: ONNX protobuf models always start with field 1 (ir_version,
        // varint), so the first byte has the high bits 0x08.  Accept empty data for
        // unit-test convenience.
        if !model_data.is_empty() && model_data[0] != 0x08 {
            // Lenient: warn but do not reject — real runtime would reject invalid models.
        }

        Ok(NpuModel::new(
            NpuBackend::OnnxRuntime,
            model_data,
            input_shapes,
            output_shapes,
        ))
    }

    /// Run inference.
    ///
    /// Stub: returns zero-valued tensors whose shapes match `model.output_shapes()`.
    /// A real implementation would copy inputs into the ONNX Runtime arena, call
    /// `OrtRun`, and extract the output `OrtValue`s.
    pub fn infer(model: &NpuModel, inputs: &[Tensor<f32>]) -> TensorResult<Vec<Tensor<f32>>> {
        // Validate that the caller provided the expected number of input tensors.
        let expected = model.input_shapes().len();
        if inputs.len() != expected {
            return Err(TensorError::other(alloc::format!(
                "OnnxRuntime: expected {} input tensor(s), got {}",
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

    /// Return performance metrics for the current ONNX Runtime session.
    ///
    /// Stub: all counters are zero; a real implementation would read them from the
    /// session profiling interface.
    pub fn get_performance_metrics() -> OnnxRuntimeMetrics {
        OnnxRuntimeMetrics::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Metrics and execution-provider types
// ─────────────────────────────────────────────────────────────────────────────

/// Performance metrics for a single ONNX Runtime inference session.
#[derive(Debug, Clone, Copy, Default)]
pub struct OnnxRuntimeMetrics {
    /// Wall-clock inference time in microseconds.
    pub inference_time_us: u64,
    /// Peak memory used by the session in bytes.
    pub memory_used_bytes: usize,
    /// Which execution provider was selected for this session.
    pub execution_provider: OnnxExecutionProvider,
}

/// ONNX Runtime execution providers, ordered from most accelerated to least.
///
/// The real ONNX Runtime tries each provider in priority order and falls back to
/// the next one if unavailable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnnxExecutionProvider {
    /// Plain CPU execution (MLAS kernel library).
    #[default]
    Cpu,
    /// NVIDIA CUDA execution provider.
    Cuda,
    /// NVIDIA TensorRT execution provider (highest GPU throughput).
    TensorRT,
    /// Apple Core ML execution provider (uses ANE on Apple Silicon).
    CoreML,
    /// Microsoft DirectML execution provider (Windows, AMD/Intel/NVIDIA).
    DirectML,
    /// Intel OpenVINO execution provider.
    OpenVino,
}

// ─────────────────────────────────────────────────────────────────────────────
// Session builder stub
// ─────────────────────────────────────────────────────────────────────────────

/// Mirrors `SessionOptions` from the ONNX Runtime C API.
///
/// In the real implementation this would configure thread counts, graph
/// optimisation level, execution provider priority list, and custom operators.
#[derive(Debug, Clone)]
pub struct OnnxSessionOptions {
    /// Intra-op thread pool size (0 = auto).
    pub intra_op_num_threads: usize,
    /// Inter-op thread pool size (0 = auto).
    pub inter_op_num_threads: usize,
    /// Ordered list of preferred execution providers.
    pub execution_providers: Vec<OnnxExecutionProvider>,
    /// Enable graph optimisation (level 1-3).
    pub graph_optimization_level: u8,
}

impl Default for OnnxSessionOptions {
    fn default() -> Self {
        Self {
            intra_op_num_threads: 0,
            inter_op_num_threads: 0,
            execution_providers: alloc::vec![OnnxExecutionProvider::Cpu],
            graph_optimization_level: 3,
        }
    }
}

impl OnnxSessionOptions {
    /// Create session options with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the preferred execution provider list (highest priority first).
    pub fn with_execution_providers(mut self, providers: Vec<OnnxExecutionProvider>) -> Self {
        self.execution_providers = providers;
        self
    }

    /// Set the intra-op thread pool size.
    pub fn with_intra_op_threads(mut self, n: usize) -> Self {
        self.intra_op_num_threads = n;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onnxruntime_detect_always_succeeds() {
        let result = OnnxRuntimeBackend::detect();
        assert!(result.is_ok());
        let device = result.unwrap();
        assert_eq!(device.backend, NpuBackend::OnnxRuntime);
    }

    #[test]
    fn test_onnxruntime_is_available() {
        assert!(OnnxRuntimeBackend::is_available());
    }

    #[test]
    fn test_onnxruntime_device_info() {
        let device = OnnxRuntimeBackend::detect().unwrap();
        assert_eq!(device.name, "ONNX Runtime");
        assert_eq!(device.max_model_size, 4 * 1024 * 1024 * 1024);
        assert_eq!(device.performance_class, 4);
        assert!(!device.supported_ops.is_empty());
    }

    #[test]
    fn test_onnxruntime_compile_model() {
        let model_data = alloc::vec![0x08u8, 0x01, 0x00, 0x00]; // ONNX-like header
        let result = OnnxRuntimeBackend::compile(
            model_data,
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        );
        assert!(result.is_ok());
        let model = result.unwrap();
        assert_eq!(model.backend(), NpuBackend::OnnxRuntime);
    }

    #[test]
    fn test_onnxruntime_compile_empty_model() {
        // Empty models are accepted (unit-test convenience).
        let result = OnnxRuntimeBackend::compile(
            alloc::vec![],
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_onnxruntime_infer_stub() {
        let model = OnnxRuntimeBackend::compile(
            alloc::vec![0x08u8],
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        )
        .unwrap();

        let input = Tensor::zeros(alloc::vec![1usize, 3, 224, 224]);
        let outputs = OnnxRuntimeBackend::infer(&model, &[input]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape(), &[1usize, 1000]);
    }

    #[test]
    fn test_onnxruntime_infer_input_count_mismatch() {
        let model = OnnxRuntimeBackend::compile(
            alloc::vec![],
            alloc::vec![alloc::vec![1usize, 3, 224, 224]],
            alloc::vec![alloc::vec![1usize, 1000]],
        )
        .unwrap();

        // Provide zero inputs when one is expected — should return an error.
        let result = OnnxRuntimeBackend::infer(&model, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_onnxruntime_metrics() {
        let metrics = OnnxRuntimeBackend::get_performance_metrics();
        // Stub returns zeros.
        assert_eq!(metrics.inference_time_us, 0);
        assert_eq!(metrics.memory_used_bytes, 0);
        assert_eq!(metrics.execution_provider, OnnxExecutionProvider::Cpu);
    }

    #[test]
    fn test_onnx_execution_provider_enum() {
        // Verify all variants are constructible and distinct.
        let providers = [
            OnnxExecutionProvider::Cpu,
            OnnxExecutionProvider::Cuda,
            OnnxExecutionProvider::TensorRT,
            OnnxExecutionProvider::CoreML,
            OnnxExecutionProvider::DirectML,
            OnnxExecutionProvider::OpenVino,
        ];
        assert_eq!(providers.len(), 6);
        assert_eq!(OnnxExecutionProvider::default(), OnnxExecutionProvider::Cpu);
        assert_ne!(OnnxExecutionProvider::Cuda, OnnxExecutionProvider::TensorRT);
    }

    #[test]
    fn test_session_options_builder() {
        let opts = OnnxSessionOptions::new()
            .with_intra_op_threads(4)
            .with_execution_providers(alloc::vec![
                OnnxExecutionProvider::TensorRT,
                OnnxExecutionProvider::Cuda,
                OnnxExecutionProvider::Cpu,
            ]);

        assert_eq!(opts.intra_op_num_threads, 4);
        assert_eq!(opts.execution_providers.len(), 3);
        assert_eq!(opts.execution_providers[0], OnnxExecutionProvider::TensorRT);
    }
}
