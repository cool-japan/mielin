//! Feedforward Neural Network Inference Example
//!
//! This example demonstrates how to use mielin-tensor for ML inference with a
//! simple 3-layer feedforward neural network for image classification (MNIST-like).
//!
//! Network architecture:
//! - Input: 784 (28x28 flattened image)
//! - Hidden 1: 128 neurons with ReLU
//! - Hidden 2: 64 neurons with ReLU
//! - Output: 10 neurons with Softmax (digit classification)
//!
//! Run with: cargo run --example feedforward_inference

extern crate alloc;
use alloc::vec;

use mielin_tensor::{Dense, Initializer, LayerActivation, Sequential};

fn main() {
    println!("=== Feedforward Neural Network Inference Example ===\n");

    // Network configuration
    const INPUT_DIM: usize = 784; // 28x28 image flattened
    const HIDDEN1_DIM: usize = 128;
    const HIDDEN2_DIM: usize = 64;
    const OUTPUT_DIM: usize = 10; // 10 digit classes (0-9)

    // Build the network using Sequential model
    println!("Building network architecture...");
    let model = Sequential::new()
        // Layer 1: Input (784) -> Hidden1 (128) with ReLU
        .add(
            Dense::new(INPUT_DIM, HIDDEN1_DIM)
                .with_initializer(Initializer::Xavier, 42)
                .with_bias(true)
                .with_activation(LayerActivation::ReLU),
        )
        // Layer 2: Hidden1 (128) -> Hidden2 (64) with ReLU
        .add(
            Dense::new(HIDDEN1_DIM, HIDDEN2_DIM)
                .with_initializer(Initializer::Xavier, 43)
                .with_bias(true)
                .with_activation(LayerActivation::ReLU),
        )
        // Layer 3: Hidden2 (64) -> Output (10) with no activation
        // (Softmax will be applied separately)
        .add(
            Dense::new(HIDDEN2_DIM, OUTPUT_DIM)
                .with_initializer(Initializer::Xavier, 44)
                .with_bias(true),
        );

    println!("Network built successfully!");
    println!("Total parameters: {}\n", model.num_parameters());

    // Create a sample input (simulated 28x28 image)
    println!("Creating sample input (28x28 image flattened to 784 values)...");
    let mut input_data = vec![0.0f32; INPUT_DIM];

    // Simulate a simple pattern (e.g., vertical line in the middle)
    for i in 0..28 {
        for j in 12..16 {
            input_data[i * 28 + j] = 1.0;
        }
    }

    println!("Input size: {}\n", input_data.len());

    // Forward pass (inference)
    println!("Running forward pass...");
    let output = model.forward(&input_data);
    println!("Output size: {}\n", output.len());

    // Apply softmax to get probabilities
    let probabilities = softmax(&output);
    println!("\nClass probabilities:");
    for (i, &prob) in probabilities.iter().enumerate() {
        println!("  Digit {}: {:.4} ({:.1}%)", i, prob, prob * 100.0);
    }

    // Find the predicted class
    let predicted_class = argmax(&probabilities);
    let confidence = probabilities[predicted_class];
    println!(
        "\nPredicted digit: {} (confidence: {:.2}%)",
        predicted_class,
        confidence * 100.0
    );

    // Batch inference example
    println!("\n=== Batch Inference Example ===\n");
    const BATCH_SIZE: usize = 4;

    // Create a batch of inputs
    let mut batch_data = vec![0.0f32; BATCH_SIZE * INPUT_DIM];
    for b in 0..BATCH_SIZE {
        // Create different patterns for each sample
        for i in 0..28 {
            for j in (10 + b * 4)..(14 + b * 4) {
                if j < 28 {
                    batch_data[b * INPUT_DIM + i * 28 + j] = 1.0;
                }
            }
        }
    }

    println!("Processing {} samples...\n", BATCH_SIZE);

    // Process each sample in the batch
    println!("Batch predictions:");
    for b in 0..BATCH_SIZE {
        // Extract input for this sample
        let sample_input: Vec<f32> = batch_data[b * INPUT_DIM..(b + 1) * INPUT_DIM].to_vec();

        // Forward pass for this sample
        let sample_output = model.forward(&sample_input);

        let probs = softmax(&sample_output);
        let pred = argmax(&probs);
        let conf = probs[pred];

        println!(
            "  Sample {}: Predicted digit {} (confidence: {:.2}%)",
            b,
            pred,
            conf * 100.0
        );
    }

    println!("\n=== Model Statistics ===");
    println!("Total layers: {}", model.num_layers());
    println!("Total parameters: {}", model.num_parameters());
    println!("\nInference complete!");
}

/// Softmax activation function
///
/// Converts raw scores to probabilities that sum to 1.
fn softmax(logits: &[f32]) -> Vec<f32> {
    // Find max for numerical stability
    let max_val = logits.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));

    // Compute exp(x - max) for each element
    let mut exp_values = vec![0.0f32; logits.len()];
    let mut sum = 0.0f32;

    for (i, &val) in logits.iter().enumerate() {
        let exp_val = libm::expf(val - max_val);
        exp_values[i] = exp_val;
        sum += exp_val;
    }

    // Normalize
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
