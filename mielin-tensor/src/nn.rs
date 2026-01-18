//! Neural Network Layers
//!
//! Basic neural network building blocks for inference on embedded devices.
//!
//! ## Layers
//!
//! - **Dense (Linear)**: Fully connected layer with optional bias
//! - **Dropout**: Regularization via random zeroing (inference mode)
//! - **BatchNorm**: Batch normalization for stable training
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_tensor::nn::{Dense, Activation};
//!
//! // Create a dense layer: 128 inputs -> 64 outputs
//! let dense = Dense::new(128, 64)
//!     .with_bias(true)
//!     .with_activation(Activation::ReLU);
//!
//! // Forward pass
//! let input = vec![0.5; 128];
//! let output = dense.forward(&input);
//! ```

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use libm::{expf, sqrtf, tanhf};

/// Activation functions for neural network layers
#[derive(Debug, Clone, Copy, Default)]
pub enum Activation {
    /// No activation (identity)
    #[default]
    None,
    /// Rectified Linear Unit
    ReLU,
    /// Leaky ReLU with specified negative slope
    LeakyReLU(f32),
    /// Sigmoid function
    Sigmoid,
    /// Hyperbolic tangent
    Tanh,
    /// Gaussian Error Linear Unit
    GELU,
}

impl Activation {
    /// Apply the activation function to a vector
    pub fn apply(&self, x: &mut [f32]) {
        match self {
            Activation::None => {}
            Activation::ReLU => {
                for val in x.iter_mut() {
                    *val = if *val > 0.0 { *val } else { 0.0 };
                }
            }
            Activation::LeakyReLU(alpha) => {
                for val in x.iter_mut() {
                    *val = if *val > 0.0 { *val } else { *alpha * *val };
                }
            }
            Activation::Sigmoid => {
                for val in x.iter_mut() {
                    *val = 1.0 / (1.0 + expf(-*val));
                }
            }
            Activation::Tanh => {
                for val in x.iter_mut() {
                    *val = tanhf(*val);
                }
            }
            Activation::GELU => {
                // Approximate GELU: 0.5 * x * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))
                let sqrt_2_pi = 0.797_884_6; // sqrt(2/pi)
                for val in x.iter_mut() {
                    let x3 = *val * *val * *val;
                    let inner = sqrt_2_pi * (*val + 0.044715 * x3);
                    *val = 0.5 * *val * (1.0 + tanhf(inner));
                }
            }
        }
    }
}

/// Weight initialization strategies
#[derive(Debug, Clone, Copy, Default)]
pub enum Initializer {
    /// All zeros
    Zeros,
    /// All ones
    Ones,
    /// Xavier/Glorot uniform initialization
    #[default]
    Xavier,
    /// He/Kaiming initialization (for ReLU)
    He,
    /// Uniform random in [-1, 1]
    Uniform,
}

impl Initializer {
    /// Initialize weights for given dimensions
    pub fn init(&self, rows: usize, cols: usize, seed: u64) -> Vec<f32> {
        match self {
            Initializer::Zeros => vec![0.0; rows * cols],
            Initializer::Ones => vec![1.0; rows * cols],
            Initializer::Xavier => {
                let scale = sqrtf(6.0 / (rows + cols) as f32);
                pseudo_random_uniform(rows * cols, -scale, scale, seed)
            }
            Initializer::He => {
                let scale = sqrtf(2.0 / rows as f32);
                pseudo_random_uniform(rows * cols, -scale, scale, seed)
            }
            Initializer::Uniform => pseudo_random_uniform(rows * cols, -1.0, 1.0, seed),
        }
    }
}

/// Dense (Fully Connected) Layer
///
/// Computes: output = activation(input * weights + bias)
///
/// ## Example
///
/// ```rust,ignore
/// let dense = Dense::new(784, 128).with_activation(Activation::ReLU);
/// let output = dense.forward(&input);
/// ```
#[derive(Debug, Clone)]
pub struct Dense {
    /// Weight matrix (out_features x in_features, row-major)
    weights: Vec<f32>,
    /// Bias vector (out_features)
    bias: Option<Vec<f32>>,
    /// Input dimension
    in_features: usize,
    /// Output dimension
    out_features: usize,
    /// Activation function
    activation: Activation,
}

impl Dense {
    /// Create a new dense layer with Xavier initialization
    pub fn new(in_features: usize, out_features: usize) -> Self {
        let weights = Initializer::Xavier.init(out_features, in_features, 42);
        Self {
            weights,
            bias: None,
            in_features,
            out_features,
            activation: Activation::None,
        }
    }

    /// Create with custom weights
    pub fn with_weights(
        in_features: usize,
        out_features: usize,
        weights: Vec<f32>,
    ) -> Option<Self> {
        if weights.len() != in_features * out_features {
            return None;
        }
        Some(Self {
            weights,
            bias: None,
            in_features,
            out_features,
            activation: Activation::None,
        })
    }

    /// Add bias to the layer
    pub fn with_bias(mut self, enable: bool) -> Self {
        if enable {
            self.bias = Some(vec![0.0; self.out_features]);
        } else {
            self.bias = None;
        }
        self
    }

    /// Set custom bias values
    pub fn set_bias(&mut self, bias: Vec<f32>) -> bool {
        if bias.len() != self.out_features {
            return false;
        }
        self.bias = Some(bias);
        true
    }

    /// Set the activation function
    pub fn with_activation(mut self, activation: Activation) -> Self {
        self.activation = activation;
        self
    }

    /// Set the initializer and reinitialize weights
    pub fn with_initializer(mut self, initializer: Initializer, seed: u64) -> Self {
        self.weights = initializer.init(self.out_features, self.in_features, seed);
        self
    }

    /// Get input dimension
    pub fn in_features(&self) -> usize {
        self.in_features
    }

    /// Get output dimension
    pub fn out_features(&self) -> usize {
        self.out_features
    }

    /// Get weight matrix
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    /// Get mutable weight matrix
    pub fn weights_mut(&mut self) -> &mut [f32] {
        &mut self.weights
    }

    /// Get bias vector
    pub fn bias(&self) -> Option<&[f32]> {
        self.bias.as_deref()
    }

    /// Get mutable bias vector
    pub fn bias_mut(&mut self) -> Option<&mut [f32]> {
        self.bias.as_deref_mut()
    }

    /// Number of trainable parameters
    pub fn num_parameters(&self) -> usize {
        let weight_params = self.in_features * self.out_features;
        let bias_params = if self.bias.is_some() {
            self.out_features
        } else {
            0
        };
        weight_params + bias_params
    }

    /// Forward pass
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        assert_eq!(
            input.len(),
            self.in_features,
            "Input size mismatch: expected {}, got {}",
            self.in_features,
            input.len()
        );

        let mut output = vec![0.0; self.out_features];

        // Compute W * x for each output neuron
        for (i, output_elem) in output.iter_mut().enumerate().take(self.out_features) {
            let row_start = i * self.in_features;
            let row_end = row_start + self.in_features;
            *output_elem = dot_product(&self.weights[row_start..row_end], input);
        }

        // Add bias if present
        if let Some(ref bias) = self.bias {
            add_vectors(&mut output, bias);
        }

        // Apply activation
        self.activation.apply(&mut output);

        output
    }

    /// Forward pass for batched input
    pub fn forward_batch(&self, inputs: &[f32], batch_size: usize) -> Vec<f32> {
        assert_eq!(
            inputs.len(),
            batch_size * self.in_features,
            "Batch input size mismatch"
        );

        let mut outputs = vec![0.0; batch_size * self.out_features];

        for b in 0..batch_size {
            let input_start = b * self.in_features;
            let input_end = input_start + self.in_features;
            let input_slice = &inputs[input_start..input_end];

            let output_start = b * self.out_features;
            let output_end = output_start + self.out_features;

            // Compute W * x
            for (i, output_elem) in outputs[output_start..output_end].iter_mut().enumerate() {
                let row_start = i * self.in_features;
                let row_end = row_start + self.in_features;
                *output_elem = dot_product(&self.weights[row_start..row_end], input_slice);
            }

            // Add bias
            if let Some(ref bias) = self.bias {
                add_vectors(&mut outputs[output_start..output_end], bias);
            }

            // Apply activation
            self.activation
                .apply(&mut outputs[output_start..output_end]);
        }

        outputs
    }
}

/// Dropout Layer
///
/// Randomly zeros elements with probability `p` during training.
/// During inference, scales outputs by (1 - p).
#[derive(Debug, Clone)]
pub struct Dropout {
    /// Dropout probability
    p: f32,
    /// Training mode
    training: bool,
}

impl Dropout {
    /// Create a new dropout layer
    pub fn new(p: f32) -> Self {
        Self {
            p: p.clamp(0.0, 1.0),
            training: false,
        }
    }

    /// Set training mode
    pub fn train(&mut self, training: bool) {
        self.training = training;
    }

    /// Forward pass
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        if self.training {
            // In training mode, would randomly drop neurons
            // For no_std compatibility, we just apply the mask deterministically
            // Real implementation would need a PRNG
            let mask_scale = 1.0 / (1.0 - self.p);
            let mut output = input.to_vec();
            scale_vector(&mut output, mask_scale);
            output
        } else {
            // In inference mode, just pass through
            input.to_vec()
        }
    }
}

impl Default for Dropout {
    fn default() -> Self {
        Self::new(0.5)
    }
}

/// Batch Normalization Layer
///
/// Normalizes inputs across the batch dimension.
/// Uses running statistics during inference.
#[derive(Debug, Clone)]
pub struct BatchNorm {
    /// Number of features
    num_features: usize,
    /// Epsilon for numerical stability
    eps: f32,
    /// Momentum for running statistics
    momentum: f32,
    /// Scale parameter (gamma)
    gamma: Vec<f32>,
    /// Shift parameter (beta)
    beta: Vec<f32>,
    /// Running mean
    running_mean: Vec<f32>,
    /// Running variance
    running_var: Vec<f32>,
    /// Training mode
    training: bool,
}

impl BatchNorm {
    /// Create a new batch normalization layer
    pub fn new(num_features: usize) -> Self {
        Self {
            num_features,
            eps: 1e-5,
            momentum: 0.1,
            gamma: vec![1.0; num_features],
            beta: vec![0.0; num_features],
            running_mean: vec![0.0; num_features],
            running_var: vec![1.0; num_features],
            training: false,
        }
    }

    /// Set epsilon for numerical stability
    pub fn with_eps(mut self, eps: f32) -> Self {
        self.eps = eps;
        self
    }

    /// Set momentum for running statistics
    pub fn with_momentum(mut self, momentum: f32) -> Self {
        self.momentum = momentum;
        self
    }

    /// Set training mode
    pub fn train(&mut self, training: bool) {
        self.training = training;
    }

    /// Get gamma (scale) parameters
    pub fn gamma(&self) -> &[f32] {
        &self.gamma
    }

    /// Get mutable gamma parameters
    pub fn gamma_mut(&mut self) -> &mut [f32] {
        &mut self.gamma
    }

    /// Get beta (shift) parameters
    pub fn beta(&self) -> &[f32] {
        &self.beta
    }

    /// Get mutable beta parameters
    pub fn beta_mut(&mut self) -> &mut [f32] {
        &mut self.beta
    }

    /// Set running statistics (for loading pretrained models)
    pub fn set_running_stats(&mut self, mean: Vec<f32>, var: Vec<f32>) -> bool {
        if mean.len() != self.num_features || var.len() != self.num_features {
            return false;
        }
        self.running_mean = mean;
        self.running_var = var;
        true
    }

    /// Forward pass for a single sample
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        assert_eq!(input.len(), self.num_features);

        let mut output = vec![0.0; self.num_features];

        for (i, out_elem) in output.iter_mut().enumerate().take(self.num_features) {
            let normalized =
                (input[i] - self.running_mean[i]) / sqrtf(self.running_var[i] + self.eps);
            *out_elem = self.gamma[i] * normalized + self.beta[i];
        }

        output
    }

    /// Forward pass for batched input
    pub fn forward_batch(&mut self, inputs: &[f32], batch_size: usize) -> Vec<f32> {
        assert_eq!(inputs.len(), batch_size * self.num_features);

        let mut outputs = vec![0.0; inputs.len()];

        if self.training && batch_size > 1 {
            // Compute batch statistics
            let mut batch_mean = vec![0.0; self.num_features];
            let mut batch_var = vec![0.0; self.num_features];

            // Mean
            for b in 0..batch_size {
                for f in 0..self.num_features {
                    batch_mean[f] += inputs[b * self.num_features + f];
                }
            }
            for mean in batch_mean.iter_mut() {
                *mean /= batch_size as f32;
            }

            // Variance
            for b in 0..batch_size {
                for f in 0..self.num_features {
                    let diff = inputs[b * self.num_features + f] - batch_mean[f];
                    batch_var[f] += diff * diff;
                }
            }
            for var in batch_var.iter_mut() {
                *var /= batch_size as f32;
            }

            // Update running statistics
            for f in 0..self.num_features {
                self.running_mean[f] =
                    (1.0 - self.momentum) * self.running_mean[f] + self.momentum * batch_mean[f];
                self.running_var[f] =
                    (1.0 - self.momentum) * self.running_var[f] + self.momentum * batch_var[f];
            }

            // Normalize using batch statistics
            for b in 0..batch_size {
                for f in 0..self.num_features {
                    let idx = b * self.num_features + f;
                    let normalized = (inputs[idx] - batch_mean[f]) / sqrtf(batch_var[f] + self.eps);
                    outputs[idx] = self.gamma[f] * normalized + self.beta[f];
                }
            }
        } else {
            // Use running statistics
            for b in 0..batch_size {
                for f in 0..self.num_features {
                    let idx = b * self.num_features + f;
                    let normalized = (inputs[idx] - self.running_mean[f])
                        / sqrtf(self.running_var[f] + self.eps);
                    outputs[idx] = self.gamma[f] * normalized + self.beta[f];
                }
            }
        }

        outputs
    }
}

/// Layer Normalization
///
/// Normalizes inputs across the feature dimension (not batch).
/// More suitable for RNNs and Transformers.
#[derive(Debug, Clone)]
pub struct LayerNorm {
    /// Normalized shape (feature dimension)
    normalized_shape: usize,
    /// Epsilon for numerical stability
    eps: f32,
    /// Scale parameter (gamma)
    gamma: Vec<f32>,
    /// Shift parameter (beta)
    beta: Vec<f32>,
}

impl LayerNorm {
    /// Create a new layer normalization
    pub fn new(normalized_shape: usize) -> Self {
        Self {
            normalized_shape,
            eps: 1e-5,
            gamma: vec![1.0; normalized_shape],
            beta: vec![0.0; normalized_shape],
        }
    }

    /// Set epsilon
    pub fn with_eps(mut self, eps: f32) -> Self {
        self.eps = eps;
        self
    }

    /// Forward pass
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        assert_eq!(input.len(), self.normalized_shape);

        // Compute mean
        let mean: f32 = input.iter().sum::<f32>() / input.len() as f32;

        // Compute variance
        let var: f32 = input
            .iter()
            .map(|x| {
                let diff = x - mean;
                diff * diff
            })
            .sum::<f32>()
            / input.len() as f32;

        // Normalize and scale
        let mut output = vec![0.0; self.normalized_shape];
        for (i, out) in output.iter_mut().enumerate() {
            let normalized = (input[i] - mean) / sqrtf(var + self.eps);
            *out = self.gamma[i] * normalized + self.beta[i];
        }

        output
    }
}

// Helper functions for vector operations

/// Compute dot product of two vectors
#[inline]
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Add bias vector to output vector (in-place)
#[inline]
fn add_vectors(output: &mut [f32], bias: &[f32]) {
    debug_assert_eq!(output.len(), bias.len());
    for (out, b) in output.iter_mut().zip(bias.iter()) {
        *out += b;
    }
}

/// Scale a vector by a scalar (in-place)
#[inline]
fn scale_vector(output: &mut [f32], scalar: f32) {
    for val in output.iter_mut() {
        *val *= scalar;
    }
}

/// Simple pseudo-random number generator (Xorshift)
fn pseudo_random_uniform(count: usize, min: f32, max: f32, seed: u64) -> Vec<f32> {
    let mut state = seed;
    let range = max - min;

    (0..count)
        .map(|_| {
            // Xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;

            // Convert to [0, 1) range
            let normalized = (state as f32) / (u64::MAX as f32);
            min + normalized * range
        })
        .collect()
}

/// Sequential model - stack of layers
#[derive(Debug, Clone)]
pub struct Sequential {
    /// Dense layers in sequence
    layers: Vec<Dense>,
}

impl Sequential {
    /// Create an empty sequential model
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a dense layer
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, layer: Dense) -> Self {
        // Verify dimensions match
        if let Some(last) = self.layers.last() {
            assert_eq!(
                last.out_features(),
                layer.in_features(),
                "Layer dimension mismatch: {} vs {}",
                last.out_features(),
                layer.in_features()
            );
        }
        self.layers.push(layer);
        self
    }

    /// Forward pass through all layers
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        let mut current = input.to_vec();
        for layer in &self.layers {
            current = layer.forward(&current);
        }
        current
    }

    /// Get total number of parameters
    pub fn num_parameters(&self) -> usize {
        self.layers.iter().map(|l| l.num_parameters()).sum()
    }

    /// Get number of layers
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// Get layer by index
    pub fn layer(&self, index: usize) -> Option<&Dense> {
        self.layers.get(index)
    }

    /// Get mutable layer by index
    pub fn layer_mut(&mut self, index: usize) -> Option<&mut Dense> {
        self.layers.get_mut(index)
    }
}

impl Default for Sequential {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dense_creation() {
        let dense = Dense::new(10, 5);
        assert_eq!(dense.in_features(), 10);
        assert_eq!(dense.out_features(), 5);
        assert_eq!(dense.weights().len(), 50);
        assert!(dense.bias().is_none());
    }

    #[test]
    fn test_dense_with_bias() {
        let dense = Dense::new(10, 5).with_bias(true);
        assert!(dense.bias().is_some());
        assert_eq!(dense.bias().unwrap().len(), 5);
    }

    #[test]
    fn test_dense_forward() {
        let dense = Dense::with_weights(3, 2, vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]).unwrap();
        let input = vec![1.0, 2.0, 3.0];
        let output = dense.forward(&input);

        // First neuron: 1*1 + 0*2 + 0*3 = 1
        // Second neuron: 0*1 + 1*2 + 0*3 = 2
        assert_eq!(output.len(), 2);
        assert!((output[0] - 1.0).abs() < 1e-5);
        assert!((output[1] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_dense_with_activation() {
        let dense = Dense::with_weights(2, 2, vec![1.0, 0.0, 0.0, 1.0])
            .unwrap()
            .with_activation(Activation::ReLU);

        let input = vec![-1.0, 2.0];
        let output = dense.forward(&input);

        // After ReLU: [-1, 2] -> [0, 2]
        assert_eq!(output[0], 0.0);
        assert_eq!(output[1], 2.0);
    }

    #[test]
    fn test_dense_batch_forward() {
        let dense = Dense::with_weights(2, 2, vec![1.0, 0.0, 0.0, 1.0]).unwrap();
        let inputs = vec![1.0, 2.0, 3.0, 4.0]; // batch of 2

        let outputs = dense.forward_batch(&inputs, 2);
        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0], 1.0);
        assert_eq!(outputs[1], 2.0);
        assert_eq!(outputs[2], 3.0);
        assert_eq!(outputs[3], 4.0);
    }

    #[test]
    fn test_dense_num_parameters() {
        let dense = Dense::new(100, 50).with_bias(true);
        assert_eq!(dense.num_parameters(), 5050); // 100*50 + 50
    }

    #[test]
    fn test_dropout_inference() {
        let dropout = Dropout::new(0.5);
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = dropout.forward(&input);

        // In inference mode, should be unchanged
        assert_eq!(output, input);
    }

    #[test]
    fn test_batch_norm_creation() {
        let bn = BatchNorm::new(10);
        assert_eq!(bn.gamma().len(), 10);
        assert_eq!(bn.beta().len(), 10);
    }

    #[test]
    fn test_batch_norm_forward() {
        let bn = BatchNorm::new(3);
        let input = vec![1.0, 2.0, 3.0];
        let output = bn.forward(&input);

        // Output should be normalized
        assert_eq!(output.len(), 3);
    }

    #[test]
    fn test_layer_norm() {
        let ln = LayerNorm::new(4);
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = ln.forward(&input);

        // Mean of output should be close to 0 (with default beta=0)
        let mean: f32 = output.iter().sum::<f32>() / output.len() as f32;
        assert!(mean.abs() < 1e-5);
    }

    #[test]
    fn test_sequential_model() {
        let model = Sequential::new()
            .add(Dense::new(10, 5).with_activation(Activation::ReLU))
            .add(Dense::new(5, 2));

        assert_eq!(model.num_layers(), 2);

        let input = vec![0.5; 10];
        let output = model.forward(&input);
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn test_sequential_num_parameters() {
        let model = Sequential::new()
            .add(Dense::new(784, 128).with_bias(true))
            .add(Dense::new(128, 10).with_bias(true));

        // 784*128 + 128 + 128*10 + 10 = 100352 + 128 + 1280 + 10 = 101770
        assert_eq!(model.num_parameters(), 101770);
    }

    #[test]
    fn test_initializer_zeros() {
        let weights = Initializer::Zeros.init(10, 10, 0);
        assert!(weights.iter().all(|&w| w == 0.0));
    }

    #[test]
    fn test_initializer_ones() {
        let weights = Initializer::Ones.init(10, 10, 0);
        assert!(weights.iter().all(|&w| w == 1.0));
    }

    #[test]
    fn test_initializer_xavier() {
        let weights = Initializer::Xavier.init(100, 100, 42);
        let mean: f32 = weights.iter().sum::<f32>() / weights.len() as f32;
        // Xavier should have mean close to 0
        assert!(mean.abs() < 0.1);
    }

    #[test]
    fn test_activation_relu() {
        let mut data = vec![-1.0, 0.0, 1.0, 2.0];
        Activation::ReLU.apply(&mut data);
        assert_eq!(data, vec![0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_activation_sigmoid() {
        let mut data = vec![0.0];
        Activation::Sigmoid.apply(&mut data);
        assert!((data[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_dense_custom_weights() {
        let weights = vec![1.0; 20];
        let dense = Dense::with_weights(4, 5, weights);
        assert!(dense.is_some());

        let bad_weights = vec![1.0; 10];
        let dense = Dense::with_weights(4, 5, bad_weights);
        assert!(dense.is_none());
    }

    #[test]
    fn test_sequential_layer_access() {
        let model = Sequential::new()
            .add(Dense::new(10, 5))
            .add(Dense::new(5, 2));

        assert!(model.layer(0).is_some());
        assert!(model.layer(1).is_some());
        assert!(model.layer(2).is_none());
    }

    #[test]
    fn test_batch_norm_set_stats() {
        let mut bn = BatchNorm::new(3);
        let mean = vec![0.1, 0.2, 0.3];
        let var = vec![1.0, 1.1, 1.2];

        assert!(bn.set_running_stats(mean.clone(), var.clone()));

        // Wrong size should fail
        assert!(!bn.set_running_stats(vec![0.0], vec![1.0]));
    }
}
