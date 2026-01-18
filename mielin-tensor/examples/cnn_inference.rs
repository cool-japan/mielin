//! Convolutional Neural Network (CNN) Inference Example
//!
//! This example demonstrates how to use mielin-tensor for CNN-based image classification.
//! It shows convolution, pooling, and dense layer operations.
//!
//! Network architecture:
//! - Input: 28x28 (grayscale image, single channel)
//! - Conv1: 3x3 kernel, 8 filters -> 8x 26x26 feature maps
//! - MaxPool1: 2x2 -> 8x 13x13 feature maps
//! - Conv2: 3x3 kernel, 4 filters (applied to first map) -> 4x 11x11 feature maps
//! - MaxPool2: 2x2 -> 4x 5x5 = 100 features
//! - Dense: 100 -> 10 (classification)
//!
//! Run with: cargo run --example cnn_inference

extern crate alloc;
use alloc::vec;

use mielin_tensor::{ConvOps, Dense, Initializer, PaddingMode, Tensor};

fn main() {
    println!("=== CNN Inference Example ===\n");

    // Create a sample 28x28 grayscale image (flattened)
    println!("Creating sample 28x28 image...");
    let mut image_data = vec![0.0f32; 28 * 28];

    // Create a simple pattern (diagonal line)
    for i in 0..28 {
        image_data[i * 28 + i] = 1.0;
        if i + 1 < 28 {
            image_data[i * 28 + (i + 1)] = 0.8;
        }
        if i > 0 {
            image_data[i * 28 + (i - 1)] = 0.8;
        }
    }

    let input = Tensor::from_vec(image_data, vec![28, 28]).unwrap(); // [height, width]
    println!("Input shape: {:?}\n", input.shape());

    // Layer 1: Convolution with 8 filters (3x3 kernels)
    println!("=== Layer 1: Convolution ===");
    let num_filters1 = 8;
    let kernel_size1 = 3;

    // Initialize 8 convolutional kernels
    let mut kernels1 = vec![];
    for f in 0..num_filters1 {
        // Create small random-like pattern for each filter
        let mut kernel_data = vec![0.0f32; kernel_size1 * kernel_size1];
        for (i, elem) in kernel_data
            .iter_mut()
            .enumerate()
            .take(kernel_size1 * kernel_size1)
        {
            *elem = ((f * 7 + i * 3) % 10) as f32 / 50.0 - 0.1;
        }
        kernels1.push(Tensor::from_vec(kernel_data, vec![kernel_size1, kernel_size1]).unwrap());
    }

    // Apply convolutions
    let mut conv1_outputs = vec![];
    for kernel in kernels1.iter() {
        let conv_out = ConvOps::conv2d(&input, kernel, (1, 1), PaddingMode::Valid).unwrap();
        conv1_outputs.push(conv_out);
    }

    println!("Convolution output shape: {:?}", conv1_outputs[0].shape());
    println!("Number of feature maps: {}\n", conv1_outputs.len());

    // Apply ReLU activation
    for output in conv1_outputs.iter_mut() {
        let data = output.data_mut();
        for val in data.iter_mut() {
            *val = val.max(0.0);
        }
    }

    // Layer 2: Max Pooling (2x2)
    println!("=== Layer 2: Max Pooling ===");
    let mut pool1_outputs = vec![];
    for conv_out in conv1_outputs.iter() {
        let pool_out = ConvOps::max_pool2d(conv_out, (2, 2), (2, 2)).unwrap();
        pool1_outputs.push(pool_out);
    }

    println!("Pooling output shape: {:?}", pool1_outputs[0].shape());
    println!("Number of feature maps: {}\n", pool1_outputs.len());

    // Layer 3: Second Convolution (simplified - just use first pooled output)
    println!("=== Layer 3: Second Convolution ===");
    let num_filters2 = 4;
    let kernel_size2 = 3;

    let mut kernels2 = vec![];
    for f in 0..num_filters2 {
        let mut kernel_data = vec![0.0f32; kernel_size2 * kernel_size2];
        for (i, elem) in kernel_data
            .iter_mut()
            .enumerate()
            .take(kernel_size2 * kernel_size2)
        {
            *elem = ((f * 5 + i * 2) % 10) as f32 / 40.0 - 0.125;
        }
        kernels2.push(Tensor::from_vec(kernel_data, vec![kernel_size2, kernel_size2]).unwrap());
    }

    let mut conv2_outputs = vec![];
    // Apply to first pooled output (simplified example)
    for kernel in kernels2.iter() {
        let conv_out =
            ConvOps::conv2d(&pool1_outputs[0], kernel, (1, 1), PaddingMode::Valid).unwrap();
        conv2_outputs.push(conv_out);
    }

    println!(
        "Second convolution output shape: {:?}",
        conv2_outputs[0].shape()
    );
    println!("Number of feature maps: {}\n", conv2_outputs.len());

    // Apply ReLU
    for output in conv2_outputs.iter_mut() {
        let data = output.data_mut();
        for val in data.iter_mut() {
            *val = val.max(0.0);
        }
    }

    // Layer 4: Second Max Pooling
    println!("=== Layer 4: Second Max Pooling ===");
    let mut pool2_outputs = vec![];
    for conv_out in conv2_outputs.iter() {
        let pool_out = ConvOps::max_pool2d(conv_out, (2, 2), (2, 2)).unwrap();
        pool2_outputs.push(pool_out);
    }

    println!(
        "Second pooling output shape: {:?}",
        pool2_outputs[0].shape()
    );
    println!("Number of feature maps: {}\n", pool2_outputs.len());

    // Flatten for dense layer
    println!("=== Flattening Features ===");
    let mut flattened_data = vec![];
    for pool_out in pool2_outputs.iter() {
        flattened_data.extend_from_slice(pool_out.data());
    }

    let flattened_size = flattened_data.len();
    let flattened = Tensor::vector(flattened_data);
    println!("Flattened feature vector size: {}\n", flattened_size);

    // Layer 5: Dense layer for classification
    println!("=== Layer 5: Dense Classification Layer ===");
    const OUTPUT_DIM: usize = 10; // 10 classes

    let classifier = Dense::new(flattened_size, OUTPUT_DIM)
        .with_initializer(Initializer::Xavier, 45)
        .with_bias(true);

    let output = classifier.forward(flattened.data());
    println!("Output size: {}\n", output.len());

    // Apply softmax for probabilities
    let probabilities = softmax(&output);

    println!("Class probabilities:");
    for (i, &prob) in probabilities.iter().enumerate() {
        println!("  Class {}: {:.4} ({:.1}%)", i, prob, prob * 100.0);
    }

    let predicted_class = argmax(&probabilities);
    let confidence = probabilities[predicted_class];

    println!(
        "\nPredicted class: {} (confidence: {:.2}%)",
        predicted_class,
        confidence * 100.0
    );

    // Summary
    println!("\n=== Network Summary ===");
    println!("Layer 1: Conv2D (8 filters, 3x3 kernel) + ReLU");
    println!("Layer 2: MaxPool2D (2x2)");
    println!("Layer 3: Conv2D (4 filters, 3x3 kernel) + ReLU");
    println!("Layer 4: MaxPool2D (2x2)");
    println!("Layer 5: Dense ({} -> {})", flattened_size, OUTPUT_DIM);
    println!("\nInference complete!");
}

/// Softmax activation function
fn softmax(logits: &[f32]) -> Vec<f32> {
    let max_val = logits.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));

    let mut exp_values = vec![0.0f32; logits.len()];
    let mut sum = 0.0f32;

    for (i, &val) in logits.iter().enumerate() {
        let exp_val = libm::expf(val - max_val);
        exp_values[i] = exp_val;
        sum += exp_val;
    }

    for val in exp_values.iter_mut() {
        *val /= sum;
    }

    exp_values
}

/// Find the index of the maximum value
fn argmax(values: &[f32]) -> usize {
    let mut max_idx = 0;
    let mut max_val = values[0];

    for (i, &val) in values.iter().enumerate().skip(1) {
        if val > max_val {
            max_val = val;
            max_idx = i;
        }
    }

    max_idx
}
