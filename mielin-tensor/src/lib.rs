//! MielinTensor - TensorLogic Integration
//!
//! Kernel-level awareness and optimization for tensor/matrix operations.
//! Leverages Arm SVE2/SME and other hardware accelerators.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

pub mod accelerator;
pub mod activation;
pub mod autograd;
pub mod backends;
pub mod broadcast;
pub mod cache;
pub mod complex;
pub mod conv;
pub mod data;
#[cfg(feature = "std")]
pub mod distributed;
pub mod error;
pub mod formats;
pub mod gpu;
pub mod loss;
pub mod matrix;
pub mod metrics;
pub mod mixed_precision;
pub mod nn;
pub mod nn_layers;
pub mod npu;
pub mod ops;
pub mod optim;
#[cfg(feature = "parallel")]
pub mod parallel;
pub mod pool;
pub mod profiling;
pub mod quant;
pub mod scheduler;
pub mod serialize;
pub mod sparse;
pub mod tensor;
pub mod view;

pub use activation::Activation;
pub use autograd::{ComputeGraph, OpType, Variable};
pub use broadcast::{
    broadcast_shape, broadcast_strides, can_broadcast, index_from_strides, unravel_index,
};
pub use cache::{
    blocked_matmul, blocked_transpose, CacheConfig, CacheMetrics, CACHE_LINE_SIZE, L1_CACHE_SIZE,
    L2_CACHE_SIZE, L3_CACHE_SIZE,
};
pub use complex::{Complex, Complex32, Complex64, ComplexTensor};
pub use conv::{ConvError, ConvOps, PaddingMode, PoolingMode};
pub use data::{tensor_utils, DataLoader, Dataset, SimpleRng};
#[cfg(feature = "std")]
pub use distributed::{
    distributed_matmul, ActivationFn, CompiledModel, DistributedError, DistributedInferenceEngine,
    DistributedTransport, InferenceMetrics, LayerSpec, LocalTransport, ModelParallelLayer,
    ModelParallelPipeline, NodeCapabilities, NodeId, PartitionStrategy, ReduceOp, ShardId,
    ShardedTensor, TensorShard,
};
pub use error::{ErrorCategory, TensorError, TensorResult};
pub use formats::{
    AttributeValue, ExportModel, GraphNode, ImportedModel, ModelExporter, ModelFormat, ModelGraph,
    ModelImporter, ModelInfo,
};
pub use gpu::{GpuBackend, GpuContext, GpuDevice, GpuMemory, GpuOps, GpuTensor};
pub use loss::{
    binary_cross_entropy_loss, cosine_embedding_loss, cross_entropy_loss, hinge_loss, huber_loss,
    kl_div_loss, mae_loss, mse_loss, sparse_cross_entropy_loss, Reduction,
};
pub use matrix::{EigenResult, Matrix, MatrixError, SvdResult};
pub use metrics::{
    accuracy, f1_score, mape, precision, r2_score, recall, rmse, top_k_accuracy, ConfusionMatrix,
};
pub use mixed_precision::{MixedPrecisionTensor, PrecisionType, BF16, F16};
pub use nn::{
    Activation as LayerActivation, BatchNorm, Dense, Dropout, Initializer, LayerNorm, Sequential,
};
pub use nn_layers::{AvgPool2D, Conv2D, Embedding, MaxPool2D, MultiHeadAttention, GRU, LSTM, RNN};
pub use npu::{NpuBackend, NpuContext, NpuDevice, NpuModel, NpuOps};
pub use ops::TensorOps;
pub use optim::{
    AdaGrad, Adam, AdamW, CosineAnnealingLR, CyclicLR, CyclicMode, ExponentialLR, LRScheduler,
    LearningRate, Nadam, OneCycleLR, Optimizer, ParamId, PlateauMode, RMSprop, ReduceLROnPlateau,
    StepLR, SGD,
};
#[cfg(feature = "parallel")]
pub use parallel::{
    parallel_mean, parallel_std, parallel_sum, parallel_variance, ParallelConfig, ParallelOps,
    ThreadPool,
};
pub use pool::{PoolStats, PooledBuffer, TensorPool, SIMD_ALIGNMENT};
pub use profiling::{
    MemoryDiff, MemorySnapshot, MemoryTracker, PerfMetrics, PerfMonitor, RegressionDetector,
};
pub use quant::{
    Quant4Tensor, QuantCalibrator, QuantGranularity, QuantParams, QuantScheme, QuantizedTensor,
};
pub use serialize::{DataType, Deserializer, ModelMetadata, Serializer};
pub use sparse::{SparseFormat, SparseTensor};
pub use tensor::Tensor;
pub use view::{IndexIterator, SliceRange, TensorView, TensorViewMut, ViewIterator};

use mielin_hal::capabilities::HardwareCapabilities;

pub struct TensorRuntime {
    capabilities: HardwareCapabilities,
    ops: TensorOps,
}

impl TensorRuntime {
    pub fn new(capabilities: HardwareCapabilities) -> Self {
        let ops = TensorOps::new(capabilities);
        Self { capabilities, ops }
    }

    pub fn supports_sve2(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::SVE2)
    }

    pub fn supports_sme(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::SME)
    }

    pub fn supports_neon(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::NEON)
    }

    pub fn supports_avx2(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::AVX2)
    }

    pub fn supports_avx512(&self) -> bool {
        self.capabilities.contains(HardwareCapabilities::AVX512)
    }

    /// Get the tensor operations handler
    pub fn ops(&self) -> &TensorOps {
        &self.ops
    }

    /// Get hardware capabilities
    pub fn capabilities(&self) -> HardwareCapabilities {
        self.capabilities
    }

    /// Get a human-readable description of available acceleration
    pub fn acceleration_info(&self) -> &'static str {
        if self.supports_sve2() {
            "Arm SVE2 (Scalable Vector Extension)"
        } else if self.supports_sme() {
            "Arm SME (Scalable Matrix Extension)"
        } else if self.supports_neon() {
            "Arm NEON (Advanced SIMD)"
        } else if self.supports_avx512() {
            "Intel AVX-512"
        } else if self.supports_avx2() {
            "Intel AVX2"
        } else {
            "Scalar (no SIMD acceleration)"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_runtime() {
        let runtime = TensorRuntime::new(HardwareCapabilities::SVE2);
        assert!(runtime.supports_sve2());
        assert_eq!(
            runtime.acceleration_info(),
            "Arm SVE2 (Scalable Vector Extension)"
        );
    }

    #[test]
    fn test_tensor_runtime_ops() {
        let runtime = TensorRuntime::new(HardwareCapabilities::NONE);

        // Create vectors
        let a = Tensor::vector(alloc::vec![1.0, 2.0, 3.0]);
        let b = Tensor::vector(alloc::vec![4.0, 5.0, 6.0]);

        // Compute dot product
        let result = runtime.ops().dot(&a, &b).unwrap();
        assert_eq!(result, 32.0);
    }

    #[test]
    fn test_tensor_runtime_different_backends() {
        // Test with different hardware capabilities
        for caps in [
            HardwareCapabilities::NONE,
            HardwareCapabilities::NEON,
            HardwareCapabilities::SVE2,
            HardwareCapabilities::AVX2,
        ] {
            let runtime = TensorRuntime::new(caps);
            let a = Tensor::vector(alloc::vec![1.0, 2.0, 3.0]);
            let b = Tensor::vector(alloc::vec![4.0, 5.0, 6.0]);

            // All backends should produce the same result
            let result = runtime.ops().dot(&a, &b).unwrap();
            assert_eq!(result, 32.0);
        }
    }
}
