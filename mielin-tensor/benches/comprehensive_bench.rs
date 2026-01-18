//! Comprehensive Performance Benchmarks
//!
//! Tests all major operations and features:
//! - Matrix operations (transpose, inverse, eigenvalues)
//! - Convolution and pooling
//! - Complex number operations
//! - Sparse tensor operations
//! - Cache-aware algorithms
//! - Neural network layers

use mielin_tensor::*;
use std::time::Instant;

struct BenchResult {
    name: &'static str,
    iterations: usize,
    _total_ns: u128,
    avg_ns: u128,
    throughput: Option<f64>,
}

impl BenchResult {
    fn print(&self) {
        print!(
            "{:40} | {:8} iters | {:10.2} µs avg",
            self.name,
            self.iterations,
            self.avg_ns as f64 / 1000.0
        );
        if let Some(tp) = self.throughput {
            println!(" | {:8.2} GFLOPS", tp);
        } else {
            println!();
        }
    }
}

fn bench<F: FnMut()>(name: &'static str, iterations: usize, mut f: F) -> BenchResult {
    // Warmup
    for _ in 0..10 {
        f();
    }

    // Benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let duration = start.elapsed();

    let total_ns = duration.as_nanos();
    let avg_ns = total_ns / iterations as u128;

    BenchResult {
        name,
        iterations,
        _total_ns: total_ns,
        avg_ns,
        throughput: None,
    }
}

fn bench_with_flops<F: FnMut()>(
    name: &'static str,
    iterations: usize,
    flops: u64,
    mut f: F,
) -> BenchResult {
    // Warmup
    for _ in 0..10 {
        f();
    }

    // Benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let duration = start.elapsed();

    let total_ns = duration.as_nanos();
    let avg_ns = total_ns / iterations as u128;
    let throughput = (flops as f64 * iterations as f64) / (total_ns as f64 / 1e9) / 1e9;

    BenchResult {
        name,
        iterations,
        _total_ns: total_ns,
        avg_ns,
        throughput: Some(throughput),
    }
}

fn print_header(title: &str) {
    println!("\n{}", "=".repeat(80));
    println!("{:^80}", title);
    println!("{}", "=".repeat(80));
    println!(
        "{:40} | {:14} | {:16} | {:12}",
        "Operation", "Iterations", "Avg Time", "Throughput"
    );
    println!("{}", "-".repeat(80));
}

fn bench_matrix_operations() {
    print_header("Matrix Operations");

    // Matrix transpose
    let size = 512;
    let mat = Tensor::from_vec(vec![1.0f32; size * size], vec![size, size]).unwrap();
    bench("Matrix transpose (512x512)", 100, || {
        let _ = Matrix::transpose(&mat);
    })
    .print();

    // Matrix multiplication
    let m = 256;
    let a = Tensor::from_vec(vec![1.0f32; m * m], vec![m, m]).unwrap();
    let b = Tensor::from_vec(vec![1.0f32; m * m], vec![m, m]).unwrap();
    let flops = (2 * m * m * m) as u64;
    let ops = TensorOps::new(mielin_hal::capabilities::HardwareCapabilities::NONE);
    bench_with_flops("Matrix multiplication (256x256)", 50, flops, || {
        let _ = ops.matmul(&a, &b);
    })
    .print();

    // Matrix inverse
    let size = 64;
    let mat = Tensor::from_vec(
        (0..(size * size))
            .map(|i| if i % (size + 1) == 0 { 10.0 } else { 0.01 })
            .collect(),
        vec![size, size],
    )
    .unwrap();
    bench("Matrix inverse (64x64)", 50, || {
        let _ = Matrix::inverse(&mat);
    })
    .print();

    // Determinant
    bench("Matrix determinant (64x64)", 50, || {
        let _ = Matrix::determinant(&mat);
    })
    .print();

    // Eigenvalues (QR algorithm)
    let size = 32;
    let mat = Tensor::from_vec(
        (0..(size * size)).map(|i| i as f32 * 0.01).collect(),
        vec![size, size],
    )
    .unwrap();
    bench("Eigenvalues QR (32x32)", 20, || {
        let _ = Matrix::eigenvalues_qr(&mat, 100, 1e-6);
    })
    .print();

    // SVD decomposition
    bench("SVD decomposition (32x32)", 10, || {
        let _ = Matrix::svd(&mat, 100, 1e-6);
    })
    .print();
}

fn bench_convolution_pooling() {
    print_header("Convolution & Pooling");

    // 2D Convolution
    let input = Tensor::from_vec(vec![1.0f32; 32 * 32], vec![32, 32]).unwrap();
    let kernel = Tensor::from_vec(vec![0.1f32; 3 * 3], vec![3, 3]).unwrap();

    bench("Conv2D (32x32 input, 3x3 kernel)", 500, || {
        let _ = ConvOps::conv2d(&input, &kernel, (1, 1), PaddingMode::Same);
    })
    .print();

    // Max pooling
    let input = Tensor::from_vec(vec![1.0f32; 64 * 64], vec![64, 64]).unwrap();
    bench("MaxPool2D (64x64, 2x2 pool)", 500, || {
        let _ = ConvOps::max_pool2d(&input, (2, 2), (2, 2));
    })
    .print();

    // Average pooling
    bench("AvgPool2D (64x64, 2x2 pool)", 500, || {
        let _ = ConvOps::avg_pool2d(&input, (2, 2), (2, 2));
    })
    .print();
}

fn bench_activation_functions() {
    print_header("Activation Functions");

    let size = 10000;
    let tensor = Tensor::from_vec(vec![0.5f32; size], vec![size]).unwrap();
    let activation = mielin_tensor::activation::Activation::new(
        mielin_hal::capabilities::HardwareCapabilities::NONE,
    );

    bench("ReLU (10k elements)", 1000, || {
        let _ = activation.relu(&tensor);
    })
    .print();

    bench("Leaky ReLU (10k elements)", 1000, || {
        let _ = activation.leaky_relu(&tensor, 0.01);
    })
    .print();

    bench("Sigmoid (10k elements)", 1000, || {
        let _ = activation.sigmoid(&tensor);
    })
    .print();

    bench("Tanh (10k elements)", 1000, || {
        let _ = activation.tanh(&tensor);
    })
    .print();

    bench("GELU (10k elements)", 1000, || {
        let _ = activation.gelu(&tensor);
    })
    .print();

    bench("Softmax (10k elements)", 500, || {
        let _ = activation.softmax(&tensor);
    })
    .print();
}

fn bench_complex_operations() {
    print_header("Complex Number Operations");

    let size = 1000;
    let a = ComplexTensor::from_vec(
        (0..size)
            .map(|i| Complex32::new(i as f32, i as f32))
            .collect(),
        vec![size],
    )
    .unwrap();
    let b = ComplexTensor::from_vec(
        (0..size)
            .map(|i| Complex32::new(i as f32 * 2.0, i as f32 * 0.5))
            .collect(),
        vec![size],
    )
    .unwrap();

    bench("Complex addition (1k elements)", 1000, || {
        let _ = a.add(&b);
    })
    .print();

    bench("Complex multiplication (1k elements)", 1000, || {
        let _ = a.mul(&b);
    })
    .print();

    bench("Complex conjugate (1k elements)", 1000, || {
        let _ = a.conj();
    })
    .print();

    bench("Complex abs (1k elements)", 1000, || {
        let _ = a.abs();
    })
    .print();

    bench("Complex arg (1k elements)", 1000, || {
        let _ = a.arg();
    })
    .print();
}

fn bench_sparse_operations() {
    print_header("Sparse Tensor Operations");

    // Create sparse matrix (1% density)
    let size = 1000;
    let nnz = size * size / 100;
    let rows: Vec<usize> = (0..nnz).map(|i| (i * 13) % size).collect();
    let cols: Vec<usize> = (0..nnz).map(|i| (i * 17) % size).collect();
    let values = vec![1.0f32; nnz];

    let sparse = SparseTensor::from_coo(rows, cols, values, size, size).unwrap();

    bench("Sparse to dense (1000x1000, 1% density)", 100, || {
        let _ = sparse.to_dense();
    })
    .print();

    // Sparse matrix-vector multiplication
    let vec = vec![1.0f32; size];
    let flops = (2 * nnz) as u64;
    bench_with_flops("Sparse matvec (1000x1000, 1% density)", 1000, flops, || {
        let _ = sparse.matvec(&vec);
    })
    .print();

    // Sparse transpose
    bench("Sparse transpose (1000x1000)", 1000, || {
        let _ = sparse.transpose();
    })
    .print();
}

fn bench_cache_aware_operations() {
    print_header("Cache-Aware Operations");

    let config = CacheConfig::default();
    let config_no_block = CacheConfig::no_blocking();

    // Cache-blocked matrix multiplication
    let size = 256;
    let a = Tensor::from_vec(vec![1.0f32; size * size], vec![size, size]).unwrap();
    let b = Tensor::from_vec(vec![1.0f32; size * size], vec![size, size]).unwrap();
    let flops = (2 * size * size * size) as u64;

    bench_with_flops("Blocked matmul (256x256, L1 blocking)", 50, flops, || {
        let _ = blocked_matmul(&a, &b, &config);
    })
    .print();

    bench_with_flops("Naive matmul (256x256, no blocking)", 50, flops, || {
        let _ = blocked_matmul(&a, &b, &config_no_block);
    })
    .print();

    // Cache-blocked transpose
    bench("Blocked transpose (512x512)", 100, || {
        let mat = Tensor::from_vec(vec![1.0f32; 512 * 512], vec![512, 512]).unwrap();
        let _ = blocked_transpose(&mat, &config);
    })
    .print();

    bench("Naive transpose (512x512)", 100, || {
        let mat = Tensor::from_vec(vec![1.0f32; 512 * 512], vec![512, 512]).unwrap();
        let _ = blocked_transpose(&mat, &config_no_block);
    })
    .print();
}

fn bench_neural_network_layers() {
    print_header("Neural Network Layers");

    // Dense layer forward pass
    let input_size = 784;
    let output_size = 128;

    let dense = Dense::new(input_size, output_size).with_bias(true);
    let input = Tensor::from_vec(vec![1.0f32; input_size], vec![input_size]).unwrap();

    bench("Dense forward (784->128)", 1000, || {
        let _ = dense.forward(input.data());
    })
    .print();

    // Batch normalization
    let size = 128;
    let bn = BatchNorm::new(size);
    let input = Tensor::from_vec(vec![0.5f32; size], vec![size]).unwrap();

    bench("BatchNorm forward (128 features)", 1000, || {
        let _ = bn.forward(input.data());
    })
    .print();

    // Layer normalization
    let size = 512;
    let ln = LayerNorm::new(size);
    let input = Tensor::from_vec(vec![0.5f32; size], vec![size]).unwrap();

    bench("LayerNorm forward (512 features)", 1000, || {
        let _ = ln.forward(input.data());
    })
    .print();
}

fn bench_quantization() {
    print_header("Quantization Operations");

    let size = 10000;
    let data = (0..size).map(|i| i as f32 * 0.01 - 50.0).collect();
    let tensor = Tensor::from_vec(data, vec![size]).unwrap();

    // INT8 quantization
    bench("INT8 quantization (10k elements)", 1000, || {
        let _ = QuantizedTensor::from_tensor(
            &tensor,
            QuantScheme::Symmetric,
            QuantGranularity::PerTensor,
        );
    })
    .print();

    // INT4 quantization
    bench("INT4 quantization (10k elements)", 1000, || {
        let _ = Quant4Tensor::from_tensor(&tensor, QuantScheme::Symmetric);
    })
    .print();

    // Quantized matmul
    let size = 64;
    let a = Tensor::from_vec(vec![1.0f32; size * size], vec![size, size]).unwrap();
    let b = Tensor::from_vec(vec![1.0f32; size * size], vec![size, size]).unwrap();
    let qa = QuantizedTensor::from_tensor(&a, QuantScheme::Symmetric, QuantGranularity::PerTensor);
    let qb = QuantizedTensor::from_tensor(&b, QuantScheme::Symmetric, QuantGranularity::PerTensor);

    bench("Quantized matmul INT8 (64x64)", 100, || {
        let _ = qa.matmul_quant(&qb);
    })
    .print();
}

fn bench_mixed_precision() {
    print_header("Mixed Precision Operations");

    let size = 10000;
    let tensor =
        Tensor::from_vec((0..size).map(|i| i as f32 * 0.01).collect(), vec![size]).unwrap();

    bench("F32 -> FP16 conversion (10k elements)", 1000, || {
        let _ = MixedPrecisionTensor::to_f16(&tensor);
    })
    .print();

    bench("F32 -> BF16 conversion (10k elements)", 1000, || {
        let _ = MixedPrecisionTensor::to_bf16(&tensor);
    })
    .print();

    let mixed = MixedPrecisionTensor::to_f16(&tensor);

    bench("FP16 -> F32 conversion (10k elements)", 1000, || {
        let _ = mixed.to_f32();
    })
    .print();
}

fn main() {
    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════════════╗");
    println!("║           MielinTensor Comprehensive Performance Benchmarks              ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════╝");

    bench_matrix_operations();
    bench_convolution_pooling();
    bench_activation_functions();
    bench_complex_operations();
    bench_sparse_operations();
    bench_cache_aware_operations();
    bench_neural_network_layers();
    bench_quantization();
    bench_mixed_precision();

    println!("\n");
    println!("╔═══════════════════════════════════════════════════════════════════════════╗");
    println!("║                         Benchmark Complete                                ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════╝");
    println!();
}
