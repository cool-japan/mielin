//! Tensor Performance Benchmarks
//!
//! Comprehensive benchmarks for tensor operations:
//! - Matrix multiplication (CPU optimizations)
//! - Tensor operations throughput
//! - Quantization performance
//! - Model inference latency
//! - Memory bandwidth utilization

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mielin_tensor::{
    blocked_matmul, Activation, CacheConfig, Dense, Matrix, QuantGranularity, QuantParams,
    QuantScheme, QuantizedTensor, Tensor,
};
use std::hint::black_box;

// ============================================================================
// Matrix Multiplication Performance
// ============================================================================

fn bench_matrix_multiply_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("matrix_multiply");

    // Common matrix sizes for ML workloads
    for size in [64, 128, 256].iter() {
        let flops = 2 * size * size * size; // 2*N^3 for matrix multiply
        group.throughput(Throughput::Elements(flops as u64));

        group.bench_with_input(BenchmarkId::new("square_matrix", size), size, |b, &n| {
            let data_a = vec![1.0f32; n * n];
            let data_b = vec![2.0f32; n * n];
            let a = Tensor::matrix(data_a, n, n).expect("Failed to create matrix A");
            let b_mat = Tensor::matrix(data_b, n, n).expect("Failed to create matrix B");

            b.iter(|| {
                // Use blocked_matmul for matrix multiplication
                let config = CacheConfig::default();
                let result = blocked_matmul(black_box(&a), black_box(&b_mat), &config)
                    .expect("Matrix multiplication failed");
                black_box(result);
            });
        });
    }

    group.finish();
}

fn bench_matrix_transpose(c: &mut Criterion) {
    let mut group = c.benchmark_group("matrix_transpose");

    for size in [64, 128, 256, 512].iter() {
        group.throughput(Throughput::Elements((size * size) as u64));

        group.bench_with_input(BenchmarkId::new("transpose", size), size, |b, &n| {
            let data = vec![1.0f32; n * n];
            let matrix = Tensor::matrix(data, n, n).expect("Failed to create matrix");

            b.iter(|| {
                let result =
                    Matrix::transpose(black_box(&matrix)).expect("Matrix transpose failed");
                black_box(result);
            });
        });
    }

    group.finish();
}

fn bench_blocked_matmul(c: &mut Criterion) {
    let mut group = c.benchmark_group("blocked_matmul");

    for size in [128, 256, 512].iter() {
        let flops = 2 * size * size * size;
        group.throughput(Throughput::Elements(flops as u64));

        group.bench_with_input(BenchmarkId::new("cache_blocked", size), size, |b, &n| {
            let data_a = vec![1.0f32; n * n];
            let data_b = vec![2.0f32; n * n];
            let a = Tensor::matrix(data_a, n, n).expect("Failed to create matrix A");
            let b_mat = Tensor::matrix(data_b, n, n).expect("Failed to create matrix B");
            let config = CacheConfig::default();

            b.iter(|| {
                let result = blocked_matmul(black_box(&a), black_box(&b_mat), black_box(&config))
                    .expect("Failed to multiply");
                black_box(&result);
            });
        });
    }

    group.finish();
}

// ============================================================================
// Tensor Operations Throughput
// ============================================================================

fn bench_tensor_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_creation");

    for size in [1000, 10000, 100000, 1000000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        // Tensor creation
        group.bench_with_input(BenchmarkId::new("zeros", size), size, |b, &s| {
            b.iter(|| {
                let tensor = Tensor::<f32>::zeros(vec![black_box(s)]);
                black_box(tensor);
            });
        });

        // Tensor from vec
        group.bench_with_input(BenchmarkId::new("from_vec", size), size, |b, &s| {
            let data = vec![1.0f32; s];
            b.iter(|| {
                let tensor = Tensor::from_vec(black_box(data.clone()), vec![s]);
                black_box(tensor);
            });
        });
    }

    group.finish();
}

fn bench_tensor_broadcast(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_broadcast");

    for size in [64, 128, 256].iter() {
        group.throughput(Throughput::Elements((size * size) as u64));

        let data_a = vec![1.0f32; size * size];
        let data_b = vec![2.0f32; *size];
        let tensor_a = Tensor::matrix(data_a, *size, *size).expect("Failed to create tensor A");
        let tensor_b = Tensor::vector(data_b);

        group.bench_with_input(
            BenchmarkId::new("broadcast_creation", size),
            &(&tensor_a, &tensor_b),
            |b, (ta, tb)| {
                b.iter(|| {
                    // Benchmark tensor access patterns (simulate broadcast)
                    let mut sum = 0.0f32;
                    for i in 0..*size {
                        for j in 0..*size {
                            if let Some(val_a) = black_box(*ta).get(&[i, j]) {
                                if let Some(val_b) = black_box(*tb).get(&[j]) {
                                    sum += val_a + val_b;
                                }
                            }
                        }
                    }
                    black_box(sum);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Quantization Performance
// ============================================================================

fn bench_quantization(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantization");

    for size in [1000, 10000, 100000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        let data = vec![1.5f32; *size];
        let tensor = Tensor::from_vec(data, vec![*size]).expect("Failed to create tensor");

        // INT8 quantization
        group.bench_with_input(BenchmarkId::new("int8_quantize", size), &tensor, |b, t| {
            b.iter(|| {
                let quantized = QuantizedTensor::from_tensor(
                    black_box(t),
                    QuantScheme::Symmetric,
                    QuantGranularity::PerTensor,
                );
                black_box(quantized);
            });
        });

        // Quantization params computation
        group.bench_with_input(
            BenchmarkId::new("compute_quant_params", size),
            &tensor,
            |b, t| {
                b.iter(|| {
                    let data = black_box(t).data();
                    let min_val = data.iter().copied().fold(f32::INFINITY, f32::min);
                    let max_val = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let params = QuantParams::symmetric(min_val, max_val);
                    black_box(params);
                });
            },
        );
    }

    group.finish();
}

fn bench_dequantization(c: &mut Criterion) {
    let mut group = c.benchmark_group("dequantization");

    for size in [1000, 10000, 100000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        let data = vec![1.5f32; *size];
        let tensor = Tensor::from_vec(data, vec![*size]).expect("Failed to create tensor");

        // Pre-quantize for dequantization benchmarks
        let quantized = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Symmetric,
            QuantGranularity::PerTensor,
        );

        // INT8 dequantization
        group.bench_with_input(
            BenchmarkId::new("int8_dequantize", size),
            &quantized,
            |b, q| {
                b.iter(|| {
                    let result = black_box(q).dequantize();
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_quantized_matmul(c: &mut Criterion) {
    let mut group = c.benchmark_group("quantized_matmul");

    for size in [64, 128, 256].iter() {
        let flops = 2 * size * size * size;
        group.throughput(Throughput::Elements(flops as u64));

        group.bench_with_input(BenchmarkId::new("int8_matmul", size), size, |b, &n| {
            let data_a = vec![1.5f32; n * n];
            let data_b = vec![2.5f32; n * n];
            let tensor_a = Tensor::matrix(data_a, n, n).expect("Failed to create A");
            let tensor_b = Tensor::matrix(data_b, n, n).expect("Failed to create B");

            // Quantize matrices
            let quant_a = QuantizedTensor::from_tensor(
                &tensor_a,
                QuantScheme::Symmetric,
                QuantGranularity::PerTensor,
            );
            let quant_b = QuantizedTensor::from_tensor(
                &tensor_b,
                QuantScheme::Symmetric,
                QuantGranularity::PerTensor,
            );

            b.iter(|| {
                // Dequantize (simulate quantized compute)
                let deq_a = quant_a.dequantize();
                let deq_b = quant_b.dequantize();
                black_box((deq_a, deq_b));
            });
        });
    }

    group.finish();
}

// ============================================================================
// Model Inference Latency
// ============================================================================

fn bench_dense_layer_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("dense_layer_inference");

    for layer_size in [(128, 64), (256, 128), (512, 256)].iter() {
        let (input_size, output_size) = layer_size;
        let flops = 2 * input_size * output_size; // approximate
        group.throughput(Throughput::Elements(flops as u64));

        group.bench_with_input(
            BenchmarkId::new("forward", format!("{}x{}", input_size, output_size)),
            layer_size,
            |b, &(in_size, out_size)| {
                let layer = Dense::new(in_size, out_size);
                let input_data = vec![1.0f32; in_size];

                b.iter(|| {
                    let output = layer.forward(black_box(&input_data));
                    black_box(output);
                });
            },
        );
    }

    group.finish();
}

fn bench_activation_functions(c: &mut Criterion) {
    use mielin_hal::capabilities::HardwareCapabilities;

    let mut group = c.benchmark_group("activation_functions");

    let sizes = [1000, 10000, 100000];
    let activation = Activation::new(HardwareCapabilities::empty());

    for size in sizes.iter() {
        group.throughput(Throughput::Elements(*size as u64));

        let data = vec![0.5f32; *size];
        let tensor = Tensor::from_vec(data, vec![*size]).expect("Failed to create tensor");

        // ReLU
        group.bench_with_input(BenchmarkId::new("relu", size), &tensor, |b, t| {
            b.iter(|| {
                let result = activation.relu(black_box(t));
                black_box(result);
            });
        });

        // Sigmoid
        group.bench_with_input(BenchmarkId::new("sigmoid", size), &tensor, |b, t| {
            b.iter(|| {
                let result = activation.sigmoid(black_box(t));
                black_box(result);
            });
        });

        // Tanh
        group.bench_with_input(BenchmarkId::new("tanh", size), &tensor, |b, t| {
            b.iter(|| {
                let result = activation.tanh(black_box(t));
                black_box(result);
            });
        });

        // GELU
        group.bench_with_input(BenchmarkId::new("gelu", size), &tensor, |b, t| {
            b.iter(|| {
                let result = activation.gelu(black_box(t));
                black_box(result);
            });
        });
    }

    group.finish();
}

// ============================================================================
// Memory Bandwidth Utilization
// ============================================================================

fn bench_memory_bandwidth(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_bandwidth");

    for size_mb in [1, 10, 100].iter() {
        let size_bytes = size_mb * 1024 * 1024;
        let size_elements = size_bytes / 4; // f32 size
        group.throughput(Throughput::Bytes(size_bytes as u64));

        let data = vec![1.0f32; size_elements];

        // Sequential read
        group.bench_with_input(
            BenchmarkId::new("sequential_read", format!("{}MB", size_mb)),
            &data,
            |b, d| {
                b.iter(|| {
                    let sum: f32 = d.iter().sum();
                    black_box(sum);
                });
            },
        );

        // Sequential write
        group.bench_with_input(
            BenchmarkId::new("sequential_write", format!("{}MB", size_mb)),
            &size_elements,
            |b, &size| {
                b.iter(|| {
                    let mut output = vec![0.0f32; size];
                    for (i, item) in output.iter_mut().enumerate().take(size) {
                        *item = i as f32;
                    }
                    black_box(output);
                });
            },
        );

        // Copy (read + write)
        group.bench_with_input(
            BenchmarkId::new("copy", format!("{}MB", size_mb)),
            &data,
            |b, d| {
                b.iter(|| {
                    let copied = d.clone();
                    black_box(copied);
                });
            },
        );
    }

    group.finish();
}

fn bench_strided_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("strided_memory_access");

    let size = 100000;
    let data = vec![1.0f32; size];

    for stride in [1, 2, 4, 8, 16].iter() {
        group.throughput(Throughput::Elements((size / stride) as u64));

        group.bench_with_input(
            BenchmarkId::new("strided_sum", format!("stride_{}", stride)),
            &(data.clone(), *stride),
            |b, (d, s)| {
                b.iter(|| {
                    let sum: f32 = d.iter().step_by(*s).sum();
                    black_box(sum);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Baseline Comparisons
// ============================================================================

fn bench_baseline_float_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("baseline_float_ops");

    let size = 100000;
    let a = vec![1.0f32; size];
    let b_data = vec![2.0f32; size];

    group.throughput(Throughput::Elements(size as u64));

    // Raw addition
    group.bench_function("raw_add", |bench| {
        bench.iter(|| {
            let result: Vec<f32> = a.iter().zip(b_data.iter()).map(|(x, y)| x + y).collect();
            black_box(result);
        });
    });

    // Raw multiplication
    group.bench_function("raw_mul", |bench| {
        bench.iter(|| {
            let result: Vec<f32> = a.iter().zip(b_data.iter()).map(|(x, y)| x * y).collect();
            black_box(result);
        });
    });

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    matrix_benches,
    bench_matrix_multiply_sizes,
    bench_matrix_transpose,
    bench_blocked_matmul,
);

criterion_group!(
    tensor_ops_benches,
    bench_tensor_creation,
    bench_tensor_broadcast,
);

criterion_group!(
    quantization_benches,
    bench_quantization,
    bench_dequantization,
    bench_quantized_matmul,
);

criterion_group!(
    inference_benches,
    bench_dense_layer_inference,
    bench_activation_functions,
);

criterion_group!(memory_benches, bench_memory_bandwidth, bench_strided_access,);

criterion_group!(baseline_benches, bench_baseline_float_ops,);

criterion_main!(
    matrix_benches,
    tensor_ops_benches,
    quantization_benches,
    inference_benches,
    memory_benches,
    baseline_benches,
);
