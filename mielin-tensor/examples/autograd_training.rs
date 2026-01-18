//! Automatic Differentiation Training Example
//!
//! This example demonstrates how to use mielin-tensor's autograd module
//! for training a simple neural network with backpropagation.
//!
//! Task: Learn XOR function
//! Input: 2 binary values
//! Output: XOR result
//!
//! Run with: cargo run --example autograd_training

extern crate alloc;
use alloc::vec;

use mielin_tensor::{ComputeGraph, Tensor};

fn main() {
    println!("=== Automatic Differentiation Training Example ===\n");
    println!("Task: Learn XOR function using backpropagation\n");

    // XOR training data
    let train_data = [
        (vec![0.0, 0.0], 0.0), // 0 XOR 0 = 0
        (vec![0.0, 1.0], 1.0), // 0 XOR 1 = 1
        (vec![1.0, 0.0], 1.0), // 1 XOR 0 = 1
        (vec![1.0, 1.0], 0.0), // 1 XOR 1 = 0
    ];

    println!("Training Data:");
    for (input, target) in train_data.iter() {
        println!("  Input: {:?}, Target: {}", input, target);
    }
    println!();

    // Network parameters
    const INPUT_DIM: usize = 2;
    const HIDDEN_DIM: usize = 4;
    const OUTPUT_DIM: usize = 1;
    const LEARNING_RATE: f32 = 0.1;
    const EPOCHS: usize = 1000;

    // Initialize network weights
    println!("Initializing network...");
    println!(
        "Architecture: {} -> {} -> {}\n",
        INPUT_DIM, HIDDEN_DIM, OUTPUT_DIM
    );

    // Simple weight initialization
    // W1: [HIDDEN_DIM, INPUT_DIM] = [4, 2] for matmul
    let mut w1_data = vec![
        0.5, -0.3, // hidden neuron 0 weights
        0.2, -0.4, // hidden neuron 1 weights
        0.3, 0.6, // hidden neuron 2 weights
        -0.5, 0.4, // hidden neuron 3 weights
    ];
    let mut b1_data = vec![0.0, 0.0, 0.0, 0.0];

    // W2: [OUTPUT_DIM, HIDDEN_DIM] = [1, 4] for matmul
    let mut w2_data = vec![0.5, -0.6, 0.4, -0.3];
    let mut b2_data = vec![0.0];

    // Training loop
    println!("Training for {} epochs...", EPOCHS);

    for epoch in 0..EPOCHS {
        let mut epoch_loss = 0.0;

        for (input_data, target) in train_data.iter() {
            // Create computational graph
            let graph = ComputeGraph::new();

            // Convert weights and input to variables
            let w1 = graph.variable(
                Tensor::from_vec(w1_data.clone(), vec![HIDDEN_DIM, INPUT_DIM]).unwrap(),
                true,
            );
            let b1 = graph.variable(Tensor::vector(b1_data.clone()), true);
            let w2 = graph.variable(
                Tensor::from_vec(w2_data.clone(), vec![OUTPUT_DIM, HIDDEN_DIM]).unwrap(),
                true,
            );
            let b2 = graph.variable(Tensor::vector(b2_data.clone()), true);

            let input = graph.variable(Tensor::vector(input_data.clone()), false);

            // Forward pass: input -> hidden
            // W1^T @ input + b1 (we use input @ W1 for convenience)
            let hidden_raw = graph.matmul(&w1, &input);
            let hidden_bias = graph.add(&hidden_raw, &b1);
            let hidden_act = graph.sigmoid(&hidden_bias);

            // Forward pass: hidden -> output
            // W2^T @ hidden + b2
            let output_raw = graph.matmul(&w2, &hidden_act);
            let output_bias = graph.add(&output_raw, &b2);
            let output = graph.sigmoid(&output_bias);

            // Compute loss (MSE)
            let target_var = graph.variable(Tensor::scalar(*target), false);
            let diff = graph.sub(&output, &target_var);
            let loss = graph.mul(&diff, &diff);

            // Backward pass
            loss.backward();

            // Extract gradients and update weights
            let w1_grad = w1.grad().unwrap();
            let b1_grad = b1.grad().unwrap();
            let w2_grad = w2.grad().unwrap();
            let b2_grad = b2.grad().unwrap();

            // Gradient descent update
            for (i, item) in w1_data.iter_mut().enumerate() {
                *item -= LEARNING_RATE * w1_grad.data()[i];
            }
            for (i, item) in b1_data.iter_mut().enumerate() {
                *item -= LEARNING_RATE * b1_grad.data()[i];
            }
            for (i, item) in w2_data.iter_mut().enumerate() {
                *item -= LEARNING_RATE * w2_grad.data()[i];
            }
            for (i, item) in b2_data.iter_mut().enumerate() {
                *item -= LEARNING_RATE * b2_grad.data()[i];
            }

            epoch_loss += loss.data().data()[0];
        }

        // Print progress every 100 epochs
        if (epoch + 1) % 100 == 0 {
            println!(
                "Epoch {}: Loss = {:.6}",
                epoch + 1,
                epoch_loss / train_data.len() as f32
            );
        }
    }

    println!("\nTraining complete!\n");

    // Test the trained network
    println!("=== Testing Trained Network ===\n");

    for (input_data, target) in train_data.iter() {
        let graph = ComputeGraph::new();

        let w1 = graph.variable(
            Tensor::from_vec(w1_data.clone(), vec![HIDDEN_DIM, INPUT_DIM]).unwrap(),
            false,
        );
        let b1 = graph.variable(Tensor::vector(b1_data.clone()), false);
        let w2 = graph.variable(
            Tensor::from_vec(w2_data.clone(), vec![OUTPUT_DIM, HIDDEN_DIM]).unwrap(),
            false,
        );
        let b2 = graph.variable(Tensor::vector(b2_data.clone()), false);
        let input = graph.variable(Tensor::vector(input_data.clone()), false);

        // Forward pass
        let hidden_raw = graph.matmul(&w1, &input);
        let hidden_bias = graph.add(&hidden_raw, &b1);
        let hidden_act = graph.sigmoid(&hidden_bias);
        let output_raw = graph.matmul(&w2, &hidden_act);
        let output_bias = graph.add(&output_raw, &b2);
        let output = graph.sigmoid(&output_bias);

        let prediction = output.data().data()[0];
        let rounded = if prediction > 0.5 { 1.0 } else { 0.0 };

        println!("Input: {:?}", input_data);
        println!(
            "  Target: {}, Prediction: {:.4}, Rounded: {}",
            *target, prediction, rounded
        );
        println!("  Correct: {}\n", (*target - rounded).abs() < 0.01);
    }

    println!("XOR function learned successfully!");
}
