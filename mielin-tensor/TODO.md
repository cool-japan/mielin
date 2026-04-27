# mielin-tensor TODO

## Pending Tasks

### High Priority
- [x] Implement SVE2 backend — dispatcher fully wired; dot, matmul, add, sub, mul, div all route through sve2 intrinsics on aarch64+sve2

### Low Priority
- [ ] Video tutorials for TensorLogic

## Completed Features (v0.1.0-rc.1)

The following major features have been implemented and are production-ready:

### Core Tensor Operations
- ✅ Basic tensor creation (zeros, ones, from_vec)
- ✅ Arithmetic operations (add, sub, mul, div)
- ✅ Matrix operations (dot product, matmul, transpose)
- ✅ Reduction operations (sum, mean, max, min)
- ✅ Broadcasting support
- ✅ Tensor reshaping and slicing

### Hardware Acceleration
- ✅ CPU backend with SIMD optimization (AVX2, NEON)
- ✅ CUDA backend for NVIDIA GPUs
- ✅ Metal backend for Apple Silicon
- ✅ NPU support (Apple Neural Engine, Edge TPU, Qualcomm NPU)
- ✅ Arm SVE2/SME preliminary support
- ✅ Automatic backend selection based on hardware

### Model Formats
- ✅ ONNX model loading and conversion
- ✅ TensorFlow Lite model support
- ✅ Model format conversion utilities

### Performance Features
- ✅ Parallel execution support (rayon integration)
- ✅ Memory-efficient operations
- ✅ Quantization support (INT8, INT16)
- ✅ Fused operations for performance

### Testing & Benchmarks
- ✅ Comprehensive test suite
- ✅ Performance benchmarks (quantization, SIMD)
- ✅ Cross-platform compatibility tests

For detailed feature descriptions and API documentation, see [README.md](README.md) and the generated rustdoc.
