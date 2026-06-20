//! GPU Acceleration for Tensor Operations
//!
//! Supports CUDA (NVIDIA) and Metal (Apple Silicon) backends.
//! Provides automatic GPU/CPU fallback based on hardware detection.

#![allow(unused)]

use crate::error::{TensorError, TensorResult};
use crate::ops::TensorOps;
use crate::tensor::Tensor;
use alloc::vec::Vec;
use mielin_hal::capabilities::HardwareCapabilities;

#[cfg(feature = "cuda")]
pub mod cuda;

#[cfg(feature = "metal")]
pub mod metal;

/// GPU backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// No GPU available, use CPU
    None,
    /// NVIDIA CUDA backend
    #[cfg(feature = "cuda")]
    Cuda,
    /// Apple Metal backend
    #[cfg(feature = "metal")]
    Metal,
}

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuDevice {
    /// Backend type
    pub backend: GpuBackend,
    /// Device name
    pub name: &'static str,
    /// Total memory in bytes
    pub total_memory: usize,
    /// Available memory in bytes
    pub available_memory: usize,
    /// Compute capability (for CUDA)
    pub compute_capability: Option<(u32, u32)>,
}

impl GpuDevice {
    /// Create a CPU fallback device
    pub fn cpu() -> Self {
        Self {
            backend: GpuBackend::None,
            name: "CPU (no GPU)",
            total_memory: 0,
            available_memory: 0,
            compute_capability: None,
        }
    }
}

/// GPU memory management
pub struct GpuMemory {
    /// Backend type
    backend: GpuBackend,
    /// Allocated bytes
    allocated: usize,
    /// Peak usage
    peak: usize,
}

impl GpuMemory {
    /// Create a new GPU memory manager
    pub fn new(backend: GpuBackend) -> Self {
        Self {
            backend,
            allocated: 0,
            peak: 0,
        }
    }

    /// Get backend type
    pub fn backend(&self) -> GpuBackend {
        self.backend
    }

    /// Get currently allocated bytes
    pub fn allocated(&self) -> usize {
        self.allocated
    }

    /// Get peak memory usage
    pub fn peak(&self) -> usize {
        self.peak
    }

    /// Record allocation
    pub fn record_allocation(&mut self, bytes: usize) {
        self.allocated += bytes;
        if self.allocated > self.peak {
            self.peak = self.allocated;
        }
    }

    /// Record deallocation
    pub fn record_deallocation(&mut self, bytes: usize) {
        self.allocated = self.allocated.saturating_sub(bytes);
    }

    /// Reset statistics
    pub fn reset(&mut self) {
        self.allocated = 0;
        self.peak = 0;
    }
}

/// GPU context for managing devices and memory
pub struct GpuContext {
    device: GpuDevice,
    memory: GpuMemory,
}

impl GpuContext {
    /// Detect and initialize the best available GPU backend
    pub fn new() -> TensorResult<Self> {
        #[cfg(feature = "cuda")]
        {
            if let Ok(device) = cuda::CudaDevice::detect() {
                return Ok(Self {
                    device,
                    memory: GpuMemory::new(GpuBackend::Cuda),
                });
            }
        }

        #[cfg(feature = "metal")]
        {
            if let Ok(device) = metal::MetalDevice::detect() {
                return Ok(Self {
                    device,
                    memory: GpuMemory::new(GpuBackend::Metal),
                });
            }
        }

        // CPU fallback
        Ok(Self {
            device: GpuDevice::cpu(),
            memory: GpuMemory::new(GpuBackend::None),
        })
    }

    /// Get the device information
    pub fn device(&self) -> &GpuDevice {
        &self.device
    }

    /// Get the memory manager
    pub fn memory(&self) -> &GpuMemory {
        &self.memory
    }

    /// Get mutable reference to memory manager
    pub fn memory_mut(&mut self) -> &mut GpuMemory {
        &mut self.memory
    }

    /// Check if GPU is available
    pub fn has_gpu(&self) -> bool {
        self.device.backend != GpuBackend::None
    }

    /// Get backend type
    pub fn backend(&self) -> GpuBackend {
        self.device.backend
    }
}

impl Default for GpuContext {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            device: GpuDevice::cpu(),
            memory: GpuMemory::new(GpuBackend::None),
        })
    }
}

/// GPU tensor operations trait
pub trait GpuOps {
    /// Transfer tensor to GPU
    fn to_gpu(&self, ctx: &mut GpuContext) -> TensorResult<GpuTensor>;

    /// Matrix multiplication on GPU
    fn gpu_matmul(&self, other: &Self, ctx: &mut GpuContext) -> TensorResult<Self>
    where
        Self: Sized;

    /// Element-wise addition on GPU
    fn gpu_add(&self, other: &Self, ctx: &mut GpuContext) -> TensorResult<Self>
    where
        Self: Sized;

    /// Element-wise multiplication on GPU
    fn gpu_mul(&self, other: &Self, ctx: &mut GpuContext) -> TensorResult<Self>
    where
        Self: Sized;
}

/// Tensor stored on GPU
pub struct GpuTensor {
    /// Backend type
    backend: GpuBackend,
    /// Shape
    shape: Vec<usize>,
    /// Number of elements
    size: usize,
    /// GPU pointer (opaque)
    #[cfg(feature = "cuda")]
    cuda_ptr: Option<*mut f32>,
    #[cfg(feature = "metal")]
    metal_buffer: Option<usize>, // Buffer ID
    /// CPU fallback data
    cpu_data: Option<Vec<f32>>,
}

impl GpuTensor {
    /// Create a new GPU tensor (CPU fallback)
    pub fn new_cpu(data: Vec<f32>, shape: Vec<usize>) -> Self {
        let size = data.len();
        Self {
            backend: GpuBackend::None,
            shape,
            size,
            #[cfg(feature = "cuda")]
            cuda_ptr: None,
            #[cfg(feature = "metal")]
            metal_buffer: None,
            cpu_data: Some(data),
        }
    }

    /// Get backend type
    pub fn backend(&self) -> GpuBackend {
        self.backend
    }

    /// Get shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Transfer back to CPU
    pub fn to_cpu(&self) -> TensorResult<Tensor<f32>> {
        match self.backend {
            GpuBackend::None => {
                if let Some(data) = &self.cpu_data {
                    Tensor::from_vec(data.clone(), self.shape.clone())
                        .ok_or_else(|| TensorError::other("Invalid shape for tensor"))
                } else {
                    Err(TensorError::other("No CPU data available"))
                }
            }
            #[cfg(feature = "cuda")]
            GpuBackend::Cuda => cuda::CudaTensor::to_cpu(self),
            #[cfg(feature = "metal")]
            GpuBackend::Metal => metal::MetalTensor::to_cpu(self),
            #[allow(unreachable_patterns)]
            _ => Err(TensorError::other("Backend not compiled")),
        }
    }
}

impl GpuOps for Tensor<f32> {
    fn to_gpu(&self, ctx: &mut GpuContext) -> TensorResult<GpuTensor> {
        match ctx.backend() {
            GpuBackend::None => {
                // CPU fallback
                Ok(GpuTensor::new_cpu(
                    self.data().to_vec(),
                    self.shape().to_vec(),
                ))
            }
            #[cfg(feature = "cuda")]
            GpuBackend::Cuda => cuda::CudaTensor::from_cpu(self, ctx),
            #[cfg(feature = "metal")]
            GpuBackend::Metal => metal::MetalTensor::from_cpu(self, ctx),
            #[allow(unreachable_patterns)]
            _ => Err(TensorError::other("Backend not compiled")),
        }
    }

    fn gpu_matmul(&self, other: &Self, ctx: &mut GpuContext) -> TensorResult<Self> {
        match ctx.backend() {
            GpuBackend::None => {
                // CPU fallback - use proper matrix multiplication via TensorOps
                TensorOps::new(HardwareCapabilities::NONE)
                    .matmul(self, other)
                    .ok_or_else(|| {
                        TensorError::other("Matrix multiplication failed: incompatible shapes")
                    })
            }
            #[cfg(feature = "cuda")]
            GpuBackend::Cuda => cuda::CudaTensor::matmul(self, other, ctx),
            #[cfg(feature = "metal")]
            GpuBackend::Metal => metal::MetalTensor::matmul(self, other, ctx),
            #[allow(unreachable_patterns)]
            _ => Err(TensorError::other("Backend not compiled")),
        }
    }

    fn gpu_add(&self, other: &Self, ctx: &mut GpuContext) -> TensorResult<Self> {
        match ctx.backend() {
            GpuBackend::None => Ok(self.add(other)),
            #[cfg(feature = "cuda")]
            GpuBackend::Cuda => cuda::CudaTensor::add(self, other, ctx),
            #[cfg(feature = "metal")]
            GpuBackend::Metal => metal::MetalTensor::add(self, other, ctx),
            #[allow(unreachable_patterns)]
            _ => Err(TensorError::other("Backend not compiled")),
        }
    }

    fn gpu_mul(&self, other: &Self, ctx: &mut GpuContext) -> TensorResult<Self> {
        match ctx.backend() {
            GpuBackend::None => Ok(self.mul(other)),
            #[cfg(feature = "cuda")]
            GpuBackend::Cuda => cuda::CudaTensor::mul(self, other, ctx),
            #[cfg(feature = "metal")]
            GpuBackend::Metal => metal::MetalTensor::mul(self, other, ctx),
            #[allow(unreachable_patterns)]
            _ => Err(TensorError::other("Backend not compiled")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_gpu_backend() {
        assert_eq!(GpuBackend::None, GpuBackend::None);
    }

    #[test]
    fn test_gpu_device_cpu() {
        let device = GpuDevice::cpu();
        assert_eq!(device.backend, GpuBackend::None);
        assert_eq!(device.name, "CPU (no GPU)");
    }

    #[test]
    fn test_gpu_memory() {
        let mut mem = GpuMemory::new(GpuBackend::None);
        assert_eq!(mem.allocated(), 0);
        assert_eq!(mem.peak(), 0);

        mem.record_allocation(1024);
        assert_eq!(mem.allocated(), 1024);
        assert_eq!(mem.peak(), 1024);

        mem.record_allocation(512);
        assert_eq!(mem.allocated(), 1536);
        assert_eq!(mem.peak(), 1536);

        mem.record_deallocation(512);
        assert_eq!(mem.allocated(), 1024);
        assert_eq!(mem.peak(), 1536);

        mem.reset();
        assert_eq!(mem.allocated(), 0);
        assert_eq!(mem.peak(), 0);
    }

    #[test]
    fn test_gpu_context() {
        let ctx = GpuContext::new().unwrap();
        // On Apple Silicon, Metal backend will be detected
        // On other platforms, this may be None
        #[cfg(all(target_os = "macos", target_arch = "aarch64", feature = "metal"))]
        {
            assert_eq!(ctx.device().backend, GpuBackend::Metal);
            assert!(ctx.has_gpu());
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64", feature = "metal")))]
        {
            // May be None or another backend depending on system
            let _ = ctx.device().backend;
        }
    }

    #[test]
    fn test_gpu_tensor_cpu() {
        let data = alloc::vec![1.0, 2.0, 3.0, 4.0];
        let shape = alloc::vec![2, 2];
        let gpu_tensor = GpuTensor::new_cpu(data.clone(), shape.clone());

        assert_eq!(gpu_tensor.backend(), GpuBackend::None);
        assert_eq!(gpu_tensor.shape(), &[2, 2]);
        assert_eq!(gpu_tensor.size(), 4);

        let cpu_tensor = gpu_tensor.to_cpu().unwrap();
        assert_eq!(cpu_tensor.data(), &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_tensor_to_gpu_cpu_fallback() {
        let tensor = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let mut ctx = GpuContext::new().unwrap();

        let gpu_tensor = tensor.to_gpu(&mut ctx).unwrap();
        assert_eq!(gpu_tensor.backend(), GpuBackend::None);

        let cpu_tensor = gpu_tensor.to_cpu().unwrap();
        assert_eq!(cpu_tensor.data(), tensor.data());
    }

    #[test]
    fn test_gpu_ops_cpu_fallback() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();
        let mut ctx = GpuContext::new().unwrap();

        let result = a.gpu_add(&b, &mut ctx).unwrap();
        assert_eq!(result.data(), &[6.0, 8.0, 10.0, 12.0]);

        let result = a.gpu_mul(&b, &mut ctx).unwrap();
        assert_eq!(result.data(), &[5.0, 12.0, 21.0, 32.0]);
    }

    #[test]
    fn test_gpu_matmul_cpu_fallback() {
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let b = Tensor::from_vec(vec![5.0, 6.0, 7.0, 8.0], vec![2, 2]).unwrap();
        let mut ctx = GpuContext::new().unwrap();

        let result = a.gpu_matmul(&b, &mut ctx).unwrap();
        // [1,2] * [5,6] = [19, 22]
        // [3,4]   [7,8]   [43, 50]
        assert_eq!(result.data(), &[19.0, 22.0, 43.0, 50.0]);
    }

    #[test]
    fn test_gpu_matmul_non_square_cpu_fallback() {
        // (2x3) * (3x2) => (2x2)
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], vec![2, 3]).unwrap();
        let b = Tensor::from_vec(vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0], vec![3, 2]).unwrap();
        let mut ctx = GpuContext::new().unwrap();

        let result = a.gpu_matmul(&b, &mut ctx).unwrap();
        assert_eq!(result.shape(), &[2, 2]);
        // Row 0: [1*7+2*9+3*11, 1*8+2*10+3*12] = [58, 64]
        // Row 1: [4*7+5*9+6*11, 4*8+5*10+6*12] = [139, 154]
        assert_eq!(result.data(), &[58.0, 64.0, 139.0, 154.0]);
    }

    #[test]
    fn test_gpu_matmul_incompatible_shapes_returns_error() {
        // (2x3) * (2x2) is incompatible — inner dims 3 != 2
        let a = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], vec![2, 3]).unwrap();
        let b = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]).unwrap();
        let mut ctx = GpuContext::new().unwrap();

        let result = a.gpu_matmul(&b, &mut ctx);
        assert!(
            result.is_err(),
            "Expected Err for incompatible matmul shapes, got Ok"
        );
    }
}
