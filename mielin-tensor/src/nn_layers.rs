//! Advanced Neural Network Layers
//!
//! Additional neural network layers including RNN, LSTM, GRU, Embedding, and Attention.

#![allow(unused)]

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
use core::f32;
use libm::{expf, sqrtf, tanhf};

use crate::conv::{ConvOps, PaddingMode, PoolingMode};
use crate::nn::Initializer;

/// Embedding layer - maps discrete tokens to continuous vectors
///
/// # Example
/// ```ignore
/// let embed = Embedding::new(10000, 256); // 10000 vocab, 256 dimensions
/// let tokens = vec![5, 123, 42];
/// let embeddings = embed.forward(&tokens);
/// ```
#[derive(Debug, Clone)]
pub struct Embedding {
    /// Embedding matrix (num_embeddings x embedding_dim)
    embeddings: Vec<f32>,
    /// Number of embeddings (vocabulary size)
    num_embeddings: usize,
    /// Dimension of each embedding vector
    embedding_dim: usize,
}

impl Embedding {
    /// Create a new embedding layer
    pub fn new(num_embeddings: usize, embedding_dim: usize) -> Self {
        let embeddings = Initializer::Uniform.init(num_embeddings, embedding_dim, 42);
        Self {
            embeddings,
            num_embeddings,
            embedding_dim,
        }
    }

    /// Create with custom embeddings
    pub fn with_embeddings(
        num_embeddings: usize,
        embedding_dim: usize,
        embeddings: Vec<f32>,
    ) -> Option<Self> {
        if embeddings.len() != num_embeddings * embedding_dim {
            return None;
        }
        Some(Self {
            embeddings,
            num_embeddings,
            embedding_dim,
        })
    }

    /// Forward pass: token indices -> embeddings
    pub fn forward(&self, indices: &[usize]) -> Vec<f32> {
        let mut output = Vec::with_capacity(indices.len() * self.embedding_dim);
        for &idx in indices {
            if idx < self.num_embeddings {
                let start = idx * self.embedding_dim;
                let end = start + self.embedding_dim;
                output.extend_from_slice(&self.embeddings[start..end]);
            } else {
                // Out of range, return zeros
                output.extend(vec![0.0; self.embedding_dim]);
            }
        }
        output
    }

    /// Get embedding for a single token
    pub fn get_embedding(&self, index: usize) -> Option<&[f32]> {
        if index < self.num_embeddings {
            let start = index * self.embedding_dim;
            let end = start + self.embedding_dim;
            Some(&self.embeddings[start..end])
        } else {
            None
        }
    }

    pub fn num_embeddings(&self) -> usize {
        self.num_embeddings
    }

    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }
}

/// Simple RNN layer (Elman RNN)
///
/// h_t = tanh(W_ih * x_t + b_ih + W_hh * h_{t-1} + b_hh)
#[derive(Debug, Clone)]
pub struct RNN {
    /// Input-to-hidden weights (hidden_size x input_size)
    w_ih: Vec<f32>,
    /// Hidden-to-hidden weights (hidden_size x hidden_size)
    w_hh: Vec<f32>,
    /// Input-to-hidden bias
    b_ih: Vec<f32>,
    /// Hidden-to-hidden bias
    b_hh: Vec<f32>,
    /// Input size
    input_size: usize,
    /// Hidden state size
    hidden_size: usize,
}

impl RNN {
    /// Create a new RNN layer
    pub fn new(input_size: usize, hidden_size: usize) -> Self {
        let w_ih = Initializer::Xavier.init(hidden_size, input_size, 42);
        let w_hh = Initializer::Xavier.init(hidden_size, hidden_size, 43);
        let b_ih = vec![0.0; hidden_size];
        let b_hh = vec![0.0; hidden_size];

        Self {
            w_ih,
            w_hh,
            b_ih,
            b_hh,
            input_size,
            hidden_size,
        }
    }

    /// Forward pass for a single timestep
    #[allow(clippy::needless_range_loop)]
    pub fn step(&self, input: &[f32], hidden: &[f32]) -> Vec<f32> {
        assert_eq!(input.len(), self.input_size);
        assert_eq!(hidden.len(), self.hidden_size);

        let mut output = vec![0.0; self.hidden_size];

        // output = W_ih * input + b_ih
        for i in 0..self.hidden_size {
            let mut sum = self.b_ih[i];
            for j in 0..self.input_size {
                sum += self.w_ih[i * self.input_size + j] * input[j];
            }
            output[i] = sum;
        }

        // output += W_hh * hidden + b_hh
        for i in 0..self.hidden_size {
            let mut sum = self.b_hh[i];
            for j in 0..self.hidden_size {
                sum += self.w_hh[i * self.hidden_size + j] * hidden[j];
            }
            output[i] += sum;
            output[i] = tanhf(output[i]);
        }

        output
    }

    /// Forward pass for a sequence
    pub fn forward(&self, inputs: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut hidden = vec![0.0; self.hidden_size];
        let mut outputs = Vec::with_capacity(inputs.len());

        for input in inputs {
            hidden = self.step(input, &hidden);
            outputs.push(hidden.clone());
        }

        outputs
    }

    pub fn input_size(&self) -> usize {
        self.input_size
    }

    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }
}

/// LSTM layer (Long Short-Term Memory)
///
/// Gates: i_t = sigmoid(W_ii * x_t + b_ii + W_hi * h_{t-1} + b_hi)  `[input]`
///        f_t = sigmoid(W_if * x_t + b_if + W_hf * h_{t-1} + b_hf)  `[forget]`
///        g_t = tanh(W_ig * x_t + b_ig + W_hg * h_{t-1} + b_hg)     `[cell]`
///        o_t = sigmoid(W_io * x_t + b_io + W_ho * h_{t-1} + b_ho)  `[output]`
/// Cell:  c_t = f_t ⊙ c_{t-1} + i_t ⊙ g_t
/// Output: h_t = o_t ⊙ tanh(c_t)
#[derive(Debug, Clone)]
pub struct LSTM {
    /// Input-to-hidden weights (4 * hidden_size x input_size) [i, f, g, o gates]
    w_ih: Vec<f32>,
    /// Hidden-to-hidden weights (4 * hidden_size x hidden_size)
    w_hh: Vec<f32>,
    /// Input biases (4 * hidden_size)
    b_ih: Vec<f32>,
    /// Hidden biases (4 * hidden_size)
    b_hh: Vec<f32>,
    /// Input size
    input_size: usize,
    /// Hidden state size
    hidden_size: usize,
}

impl LSTM {
    /// Create a new LSTM layer
    pub fn new(input_size: usize, hidden_size: usize) -> Self {
        let w_ih = Initializer::Xavier.init(4 * hidden_size, input_size, 42);
        let w_hh = Initializer::Xavier.init(4 * hidden_size, hidden_size, 43);
        let mut b_ih = vec![0.0; 4 * hidden_size];
        let b_hh = vec![0.0; 4 * hidden_size];

        // Initialize forget gate bias to 1 (helps with gradient flow)
        for i in 0..hidden_size {
            b_ih[hidden_size + i] = 1.0;
        }

        Self {
            w_ih,
            w_hh,
            b_ih,
            b_hh,
            input_size,
            hidden_size,
        }
    }

    #[inline]
    fn sigmoid(x: f32) -> f32 {
        1.0 / (1.0 + expf(-x))
    }

    /// Forward pass for a single timestep
    #[allow(clippy::needless_range_loop)]
    pub fn step(&self, input: &[f32], hidden: &[f32], cell: &[f32]) -> (Vec<f32>, Vec<f32>) {
        assert_eq!(input.len(), self.input_size);
        assert_eq!(hidden.len(), self.hidden_size);
        assert_eq!(cell.len(), self.hidden_size);

        let h = self.hidden_size;

        // Compute all gates at once
        let mut gates = vec![0.0; 4 * h];

        // gates = W_ih * input + b_ih + W_hh * hidden + b_hh
        for i in 0..(4 * h) {
            let mut sum = self.b_ih[i] + self.b_hh[i];
            for j in 0..self.input_size {
                sum += self.w_ih[i * self.input_size + j] * input[j];
            }
            for j in 0..h {
                sum += self.w_hh[i * h + j] * hidden[j];
            }
            gates[i] = sum;
        }

        // Apply activations
        let mut i_gate = vec![0.0; h];
        let mut f_gate = vec![0.0; h];
        let mut g_gate = vec![0.0; h];
        let mut o_gate = vec![0.0; h];

        for i in 0..h {
            i_gate[i] = Self::sigmoid(gates[i]);
            f_gate[i] = Self::sigmoid(gates[h + i]);
            g_gate[i] = tanhf(gates[2 * h + i]);
            o_gate[i] = Self::sigmoid(gates[3 * h + i]);
        }

        // Update cell state: c_t = f_t ⊙ c_{t-1} + i_t ⊙ g_t
        let mut new_cell = vec![0.0; h];
        for i in 0..h {
            new_cell[i] = f_gate[i] * cell[i] + i_gate[i] * g_gate[i];
        }

        // Compute new hidden state: h_t = o_t ⊙ tanh(c_t)
        let mut new_hidden = vec![0.0; h];
        for i in 0..h {
            new_hidden[i] = o_gate[i] * tanhf(new_cell[i]);
        }

        (new_hidden, new_cell)
    }

    /// Forward pass for a sequence
    pub fn forward(&self, inputs: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut hidden = vec![0.0; self.hidden_size];
        let mut cell = vec![0.0; self.hidden_size];
        let mut outputs = Vec::with_capacity(inputs.len());

        for input in inputs {
            let (h, c) = self.step(input, &hidden, &cell);
            hidden = h;
            cell = c;
            outputs.push(hidden.clone());
        }

        outputs
    }

    pub fn input_size(&self) -> usize {
        self.input_size
    }

    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }
}

/// GRU layer (Gated Recurrent Unit)
///
/// Gates: r_t = sigmoid(W_ir * x_t + b_ir + W_hr * h_{t-1} + b_hr)  `[reset]`
///        z_t = sigmoid(W_iz * x_t + b_iz + W_hz * h_{t-1} + b_hz)  `[update]`
///        n_t = tanh(W_in * x_t + b_in + r_t ⊙ (W_hn * h_{t-1} + b_hn))
/// Output: h_t = (1 - z_t) ⊙ n_t + z_t ⊙ h_{t-1}
#[derive(Debug, Clone)]
pub struct GRU {
    /// Input-to-hidden weights (3 * hidden_size x input_size) [r, z, n gates]
    w_ih: Vec<f32>,
    /// Hidden-to-hidden weights (3 * hidden_size x hidden_size)
    w_hh: Vec<f32>,
    /// Input biases (3 * hidden_size)
    b_ih: Vec<f32>,
    /// Hidden biases (3 * hidden_size)
    b_hh: Vec<f32>,
    /// Input size
    input_size: usize,
    /// Hidden state size
    hidden_size: usize,
}

impl GRU {
    /// Create a new GRU layer
    pub fn new(input_size: usize, hidden_size: usize) -> Self {
        let w_ih = Initializer::Xavier.init(3 * hidden_size, input_size, 42);
        let w_hh = Initializer::Xavier.init(3 * hidden_size, hidden_size, 43);
        let b_ih = vec![0.0; 3 * hidden_size];
        let b_hh = vec![0.0; 3 * hidden_size];

        Self {
            w_ih,
            w_hh,
            b_ih,
            b_hh,
            input_size,
            hidden_size,
        }
    }

    #[inline]
    fn sigmoid(x: f32) -> f32 {
        1.0 / (1.0 + expf(-x))
    }

    /// Forward pass for a single timestep
    #[allow(clippy::needless_range_loop)]
    pub fn step(&self, input: &[f32], hidden: &[f32]) -> Vec<f32> {
        assert_eq!(input.len(), self.input_size);
        assert_eq!(hidden.len(), self.hidden_size);

        let h = self.hidden_size;

        // Compute reset and update gates
        let mut r_gate = vec![0.0; h];
        let mut z_gate = vec![0.0; h];

        for i in 0..h {
            let mut r_sum = self.b_ih[i] + self.b_hh[i];
            let mut z_sum = self.b_ih[h + i] + self.b_hh[h + i];

            for j in 0..self.input_size {
                r_sum += self.w_ih[i * self.input_size + j] * input[j];
                z_sum += self.w_ih[(h + i) * self.input_size + j] * input[j];
            }

            for j in 0..h {
                r_sum += self.w_hh[i * h + j] * hidden[j];
                z_sum += self.w_hh[(h + i) * h + j] * hidden[j];
            }

            r_gate[i] = Self::sigmoid(r_sum);
            z_gate[i] = Self::sigmoid(z_sum);
        }

        // Compute new gate (n_t) with reset applied to hidden state
        let mut n_gate = vec![0.0; h];
        for i in 0..h {
            let mut sum = self.b_ih[2 * h + i];
            for j in 0..self.input_size {
                sum += self.w_ih[(2 * h + i) * self.input_size + j] * input[j];
            }
            for j in 0..h {
                sum += r_gate[j] * self.w_hh[(2 * h + i) * h + j] * hidden[j];
            }
            sum += self.b_hh[2 * h + i];
            n_gate[i] = tanhf(sum);
        }

        // Compute new hidden state: h_t = (1 - z_t) ⊙ n_t + z_t ⊙ h_{t-1}
        let mut new_hidden = vec![0.0; h];
        for i in 0..h {
            new_hidden[i] = (1.0 - z_gate[i]) * n_gate[i] + z_gate[i] * hidden[i];
        }

        new_hidden
    }

    /// Forward pass for a sequence
    pub fn forward(&self, inputs: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let mut hidden = vec![0.0; self.hidden_size];
        let mut outputs = Vec::with_capacity(inputs.len());

        for input in inputs {
            hidden = self.step(input, &hidden);
            outputs.push(hidden.clone());
        }

        outputs
    }

    pub fn input_size(&self) -> usize {
        self.input_size
    }

    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }
}

/// Multi-head self-attention layer
///
/// Attention(Q, K, V) = softmax(QK^T / sqrt(d_k))V
#[derive(Debug, Clone)]
pub struct MultiHeadAttention {
    /// Query projection weights (d_model x d_model)
    w_q: Vec<f32>,
    /// Key projection weights (d_model x d_model)
    w_k: Vec<f32>,
    /// Value projection weights (d_model x d_model)
    w_v: Vec<f32>,
    /// Output projection weights (d_model x d_model)
    w_o: Vec<f32>,
    /// Model dimension
    d_model: usize,
    /// Number of attention heads
    num_heads: usize,
    /// Dimension per head
    d_k: usize,
}

impl MultiHeadAttention {
    /// Create a new multi-head attention layer
    #[allow(clippy::manual_is_multiple_of)]
    pub fn new(d_model: usize, num_heads: usize) -> Option<Self> {
        if d_model % num_heads != 0 {
            return None;
        }

        let d_k = d_model / num_heads;
        let w_q = Initializer::Xavier.init(d_model, d_model, 42);
        let w_k = Initializer::Xavier.init(d_model, d_model, 43);
        let w_v = Initializer::Xavier.init(d_model, d_model, 44);
        let w_o = Initializer::Xavier.init(d_model, d_model, 45);

        Some(Self {
            w_q,
            w_k,
            w_v,
            w_o,
            d_model,
            num_heads,
            d_k,
        })
    }

    /// Scaled dot-product attention
    fn attention(
        query: &[f32],
        key: &[f32],
        value: &[f32],
        seq_len: usize,
        d_k: usize,
    ) -> Vec<f32> {
        let scale = 1.0 / sqrtf(d_k as f32);

        // Compute attention scores: QK^T / sqrt(d_k)
        let mut scores = vec![0.0; seq_len * seq_len];
        for i in 0..seq_len {
            for j in 0..seq_len {
                let mut sum = 0.0;
                for k in 0..d_k {
                    sum += query[i * d_k + k] * key[j * d_k + k];
                }
                scores[i * seq_len + j] = sum * scale;
            }
        }

        // Apply softmax row-wise
        for i in 0..seq_len {
            let row_start = i * seq_len;
            let row = &mut scores[row_start..row_start + seq_len];

            // Find max for numerical stability
            let max_val = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);

            // Exp and sum
            let mut sum = 0.0;
            for val in row.iter_mut() {
                *val = expf(*val - max_val);
                sum += *val;
            }

            // Normalize
            for val in row.iter_mut() {
                *val /= sum;
            }
        }

        // Multiply by values: Attention(Q,K,V) = softmax(scores) * V
        let mut output = vec![0.0; seq_len * d_k];
        for i in 0..seq_len {
            for j in 0..d_k {
                let mut sum = 0.0;
                for k in 0..seq_len {
                    sum += scores[i * seq_len + k] * value[k * d_k + j];
                }
                output[i * d_k + j] = sum;
            }
        }

        output
    }

    /// Forward pass
    pub fn forward(&self, x: &[f32], seq_len: usize) -> Vec<f32> {
        assert_eq!(x.len(), seq_len * self.d_model);

        // Project to Q, K, V
        let mut q = vec![0.0; seq_len * self.d_model];
        let mut k = vec![0.0; seq_len * self.d_model];
        let mut v = vec![0.0; seq_len * self.d_model];

        for i in 0..seq_len {
            for j in 0..self.d_model {
                let mut q_sum = 0.0;
                let mut k_sum = 0.0;
                let mut v_sum = 0.0;

                for k_idx in 0..self.d_model {
                    let x_val = x[i * self.d_model + k_idx];
                    q_sum += self.w_q[j * self.d_model + k_idx] * x_val;
                    k_sum += self.w_k[j * self.d_model + k_idx] * x_val;
                    v_sum += self.w_v[j * self.d_model + k_idx] * x_val;
                }

                q[i * self.d_model + j] = q_sum;
                k[i * self.d_model + j] = k_sum;
                v[i * self.d_model + j] = v_sum;
            }
        }

        // Process each head
        let mut concat = Vec::with_capacity(seq_len * self.d_model);

        for head in 0..self.num_heads {
            let head_offset = head * self.d_k;

            // Extract this head's Q, K, V
            let mut head_q = Vec::with_capacity(seq_len * self.d_k);
            let mut head_k = Vec::with_capacity(seq_len * self.d_k);
            let mut head_v = Vec::with_capacity(seq_len * self.d_k);

            for i in 0..seq_len {
                for j in 0..self.d_k {
                    head_q.push(q[i * self.d_model + head_offset + j]);
                    head_k.push(k[i * self.d_model + head_offset + j]);
                    head_v.push(v[i * self.d_model + head_offset + j]);
                }
            }

            // Compute attention for this head
            let head_output = Self::attention(&head_q, &head_k, &head_v, seq_len, self.d_k);
            concat.extend(head_output);
        }

        // Project output
        let mut output = vec![0.0; seq_len * self.d_model];
        for i in 0..seq_len {
            for j in 0..self.d_model {
                let mut sum = 0.0;
                for k in 0..self.d_model {
                    sum += self.w_o[j * self.d_model + k] * concat[i * self.d_model + k];
                }
                output[i * self.d_model + j] = sum;
            }
        }

        output
    }

    pub fn d_model(&self) -> usize {
        self.d_model
    }

    pub fn num_heads(&self) -> usize {
        self.num_heads
    }
}

/// 2D Convolutional Layer
///
/// Performs 2D convolution with learnable filters and optional bias.
/// Input shape: [batch, in_channels, height, width] (NCHW format)
/// Output shape: [batch, out_channels, out_height, out_width]
#[derive(Debug, Clone)]
pub struct Conv2D {
    /// Filters (out_channels, in_channels, kernel_h, kernel_w)
    weights: Vec<f32>,
    /// Bias (out_channels)
    bias: Option<Vec<f32>>,
    /// Input channels
    in_channels: usize,
    /// Output channels
    out_channels: usize,
    /// Kernel size (height, width)
    kernel_size: (usize, usize),
    /// Stride (height, width)
    stride: (usize, usize),
    /// Padding mode
    padding: PaddingMode,
}

impl Conv2D {
    /// Create a new Conv2D layer
    pub fn new(in_channels: usize, out_channels: usize, kernel_size: (usize, usize)) -> Self {
        let total_params = out_channels * in_channels * kernel_size.0 * kernel_size.1;
        let weights = Initializer::He.init(total_params, 1, 42);

        Self {
            weights,
            bias: None,
            in_channels,
            out_channels,
            kernel_size,
            stride: (1, 1),
            padding: PaddingMode::Valid,
        }
    }

    /// Set stride
    pub fn with_stride(mut self, stride: (usize, usize)) -> Self {
        self.stride = stride;
        self
    }

    /// Set padding mode
    pub fn with_padding(mut self, padding: PaddingMode) -> Self {
        self.padding = padding;
        self
    }

    /// Enable bias
    pub fn with_bias(mut self, enable: bool) -> Self {
        if enable {
            self.bias = Some(vec![0.0; self.out_channels]);
        } else {
            self.bias = None;
        }
        self
    }

    /// Forward pass for a single sample
    /// Input shape: [in_channels, height, width]
    /// Output shape: [out_channels, out_height, out_width]
    pub fn forward(&self, input: &[f32], height: usize, width: usize) -> Vec<f32> {
        assert_eq!(input.len(), self.in_channels * height * width);

        let kh = self.kernel_size.0;
        let kw = self.kernel_size.1;

        let pad = match self.padding {
            PaddingMode::Valid => (0, 0),
            PaddingMode::Same => ((kh - 1) / 2, (kw - 1) / 2),
            PaddingMode::Custom(p) => (p, p),
        };

        let out_h = (height + 2 * pad.0 - kh) / self.stride.0 + 1;
        let out_w = (width + 2 * pad.1 - kw) / self.stride.1 + 1;

        let mut output = vec![0.0; self.out_channels * out_h * out_w];

        // For each output channel
        for oc in 0..self.out_channels {
            let bias = self.bias.as_ref().map(|b| b[oc]).unwrap_or(0.0);

            // For each output position
            for oh in 0..out_h {
                for ow in 0..out_w {
                    let mut sum = bias;

                    // Convolution over all input channels
                    for ic in 0..self.in_channels {
                        for kh_idx in 0..kh {
                            for kw_idx in 0..kw {
                                let ih = oh * self.stride.0 + kh_idx;
                                let iw = ow * self.stride.1 + kw_idx;

                                // Check bounds with padding
                                if ih >= pad.0
                                    && ih < height + pad.0
                                    && iw >= pad.1
                                    && iw < width + pad.1
                                {
                                    let ih_actual = ih - pad.0;
                                    let iw_actual = iw - pad.1;

                                    let input_idx =
                                        ic * height * width + ih_actual * width + iw_actual;
                                    let weight_idx = oc * (self.in_channels * kh * kw)
                                        + ic * (kh * kw)
                                        + kh_idx * kw
                                        + kw_idx;

                                    sum += input[input_idx] * self.weights[weight_idx];
                                }
                            }
                        }
                    }

                    output[oc * out_h * out_w + oh * out_w + ow] = sum;
                }
            }
        }

        output
    }

    pub fn in_channels(&self) -> usize {
        self.in_channels
    }

    pub fn out_channels(&self) -> usize {
        self.out_channels
    }

    pub fn kernel_size(&self) -> (usize, usize) {
        self.kernel_size
    }
}

/// 2D Max Pooling Layer
///
/// Performs max pooling over spatial dimensions
/// Input shape: [channels, height, width]
/// Output shape: [channels, out_height, out_width]
#[derive(Debug, Clone)]
pub struct MaxPool2D {
    /// Pool size (height, width)
    pool_size: (usize, usize),
    /// Stride (height, width)
    stride: (usize, usize),
}

impl MaxPool2D {
    /// Create a new MaxPool2D layer
    pub fn new(pool_size: (usize, usize)) -> Self {
        Self {
            pool_size,
            stride: pool_size, // Default stride = pool_size
        }
    }

    /// Set custom stride
    pub fn with_stride(mut self, stride: (usize, usize)) -> Self {
        self.stride = stride;
        self
    }

    /// Forward pass
    /// Input shape: [channels, height, width]
    /// Output shape: [channels, out_height, out_width]
    pub fn forward(&self, input: &[f32], channels: usize, height: usize, width: usize) -> Vec<f32> {
        assert_eq!(input.len(), channels * height * width);

        let ph = self.pool_size.0;
        let pw = self.pool_size.1;

        let out_h = (height - ph) / self.stride.0 + 1;
        let out_w = (width - pw) / self.stride.1 + 1;

        let mut output = vec![f32::NEG_INFINITY; channels * out_h * out_w];

        for c in 0..channels {
            for oh in 0..out_h {
                for ow in 0..out_w {
                    let mut max_val = f32::NEG_INFINITY;

                    for ph_idx in 0..ph {
                        for pw_idx in 0..pw {
                            let ih = oh * self.stride.0 + ph_idx;
                            let iw = ow * self.stride.1 + pw_idx;

                            let input_idx = c * height * width + ih * width + iw;
                            max_val = max_val.max(input[input_idx]);
                        }
                    }

                    output[c * out_h * out_w + oh * out_w + ow] = max_val;
                }
            }
        }

        output
    }

    pub fn pool_size(&self) -> (usize, usize) {
        self.pool_size
    }
}

/// 2D Average Pooling Layer
///
/// Performs average pooling over spatial dimensions
/// Input shape: [channels, height, width]
/// Output shape: [channels, out_height, out_width]
#[derive(Debug, Clone)]
pub struct AvgPool2D {
    /// Pool size (height, width)
    pool_size: (usize, usize),
    /// Stride (height, width)
    stride: (usize, usize),
}

impl AvgPool2D {
    /// Create a new AvgPool2D layer
    pub fn new(pool_size: (usize, usize)) -> Self {
        Self {
            pool_size,
            stride: pool_size, // Default stride = pool_size
        }
    }

    /// Set custom stride
    pub fn with_stride(mut self, stride: (usize, usize)) -> Self {
        self.stride = stride;
        self
    }

    /// Forward pass
    /// Input shape: [channels, height, width]
    /// Output shape: [channels, out_height, out_width]
    pub fn forward(&self, input: &[f32], channels: usize, height: usize, width: usize) -> Vec<f32> {
        assert_eq!(input.len(), channels * height * width);

        let ph = self.pool_size.0;
        let pw = self.pool_size.1;
        let pool_area = (ph * pw) as f32;

        let out_h = (height - ph) / self.stride.0 + 1;
        let out_w = (width - pw) / self.stride.1 + 1;

        let mut output = vec![0.0; channels * out_h * out_w];

        for c in 0..channels {
            for oh in 0..out_h {
                for ow in 0..out_w {
                    let mut sum = 0.0;

                    for ph_idx in 0..ph {
                        for pw_idx in 0..pw {
                            let ih = oh * self.stride.0 + ph_idx;
                            let iw = ow * self.stride.1 + pw_idx;

                            let input_idx = c * height * width + ih * width + iw;
                            sum += input[input_idx];
                        }
                    }

                    output[c * out_h * out_w + oh * out_w + ow] = sum / pool_area;
                }
            }
        }

        output
    }

    pub fn pool_size(&self) -> (usize, usize) {
        self.pool_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_creation() {
        let embed = Embedding::new(100, 64);
        assert_eq!(embed.num_embeddings(), 100);
        assert_eq!(embed.embedding_dim(), 64);
    }

    #[test]
    fn test_embedding_forward() {
        let embed = Embedding::new(10, 4);
        let indices = vec![0, 5, 9];
        let output = embed.forward(&indices);
        assert_eq!(output.len(), 3 * 4); // 3 tokens * 4 dims
    }

    #[test]
    fn test_embedding_get() {
        let embed = Embedding::new(10, 4);
        let emb = embed.get_embedding(5).unwrap();
        assert_eq!(emb.len(), 4);

        assert!(embed.get_embedding(100).is_none());
    }

    #[test]
    fn test_rnn_creation() {
        let rnn = RNN::new(32, 64);
        assert_eq!(rnn.input_size(), 32);
        assert_eq!(rnn.hidden_size(), 64);
    }

    #[test]
    fn test_rnn_step() {
        let rnn = RNN::new(8, 16);
        let input = vec![1.0; 8];
        let hidden = vec![0.0; 16];

        let output = rnn.step(&input, &hidden);
        assert_eq!(output.len(), 16);
    }

    #[test]
    fn test_rnn_forward() {
        let rnn = RNN::new(8, 16);
        let inputs = vec![vec![1.0; 8], vec![0.5; 8], vec![0.25; 8]];

        let outputs = rnn.forward(&inputs);
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].len(), 16);
    }

    #[test]
    fn test_lstm_creation() {
        let lstm = LSTM::new(32, 64);
        assert_eq!(lstm.input_size(), 32);
        assert_eq!(lstm.hidden_size(), 64);
    }

    #[test]
    fn test_lstm_step() {
        let lstm = LSTM::new(8, 16);
        let input = vec![1.0; 8];
        let hidden = vec![0.0; 16];
        let cell = vec![0.0; 16];

        let (h, c) = lstm.step(&input, &hidden, &cell);
        assert_eq!(h.len(), 16);
        assert_eq!(c.len(), 16);
    }

    #[test]
    fn test_lstm_forward() {
        let lstm = LSTM::new(8, 16);
        let inputs = vec![vec![1.0; 8], vec![0.5; 8], vec![0.25; 8]];

        let outputs = lstm.forward(&inputs);
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].len(), 16);
    }

    #[test]
    fn test_gru_creation() {
        let gru = GRU::new(32, 64);
        assert_eq!(gru.input_size(), 32);
        assert_eq!(gru.hidden_size(), 64);
    }

    #[test]
    fn test_gru_step() {
        let gru = GRU::new(8, 16);
        let input = vec![1.0; 8];
        let hidden = vec![0.0; 16];

        let output = gru.step(&input, &hidden);
        assert_eq!(output.len(), 16);
    }

    #[test]
    fn test_gru_forward() {
        let gru = GRU::new(8, 16);
        let inputs = vec![vec![1.0; 8], vec![0.5; 8], vec![0.25; 8]];

        let outputs = gru.forward(&inputs);
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].len(), 16);
    }

    #[test]
    fn test_attention_creation() {
        let attn = MultiHeadAttention::new(64, 8).unwrap();
        assert_eq!(attn.d_model(), 64);
        assert_eq!(attn.num_heads(), 8);
    }

    #[test]
    fn test_attention_invalid_heads() {
        // d_model must be divisible by num_heads
        assert!(MultiHeadAttention::new(64, 7).is_none());
    }

    #[test]
    fn test_attention_forward() {
        let attn = MultiHeadAttention::new(64, 8).unwrap();
        let seq_len = 5;
        let input = vec![1.0; seq_len * 64];

        let output = attn.forward(&input, seq_len);
        assert_eq!(output.len(), seq_len * 64);
    }

    #[test]
    fn test_lstm_forget_gate_initialization() {
        let lstm = LSTM::new(8, 16);
        // Forget gate bias should be initialized to 1
        assert!((lstm.b_ih[16] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_out_of_range() {
        let embed = Embedding::new(10, 4);
        let indices = vec![0, 50]; // 50 is out of range
        let output = embed.forward(&indices);
        assert_eq!(output.len(), 2 * 4);

        // Second embedding should be all zeros (out of range)
        for item in output.iter().skip(4).take(4) {
            assert_eq!(*item, 0.0);
        }
    }

    #[test]
    fn test_conv2d_creation() {
        let conv = Conv2D::new(3, 16, (3, 3));
        assert_eq!(conv.in_channels(), 3);
        assert_eq!(conv.out_channels(), 16);
        assert_eq!(conv.kernel_size(), (3, 3));
    }

    #[test]
    fn test_conv2d_forward() {
        let conv = Conv2D::new(1, 1, (3, 3)).with_padding(PaddingMode::Valid);
        let input = vec![1.0; 5 * 5]; // 1 channel, 5x5 image

        let output = conv.forward(&input, 5, 5);
        // With 3x3 kernel and valid padding: (5-3)/1 + 1 = 3
        assert_eq!(output.len(), 3 * 3); // 1 output channel
    }

    #[test]
    fn test_conv2d_same_padding() {
        let conv = Conv2D::new(1, 1, (3, 3)).with_padding(PaddingMode::Same);
        let input = vec![1.0; 5 * 5];

        let output = conv.forward(&input, 5, 5);
        // With same padding, output should be same size as input
        assert_eq!(output.len(), 5 * 5); // 1 output channel
    }

    #[test]
    fn test_conv2d_with_stride() {
        let conv = Conv2D::new(1, 1, (3, 3))
            .with_stride((2, 2))
            .with_padding(PaddingMode::Valid);
        let input = vec![1.0; 6 * 6];

        let output = conv.forward(&input, 6, 6);
        // With stride 2: (6-3)/2 + 1 = 2
        assert_eq!(output.len(), 2 * 2); // 1 output channel
    }

    #[test]
    fn test_conv2d_multi_channel() {
        let conv = Conv2D::new(3, 16, (3, 3)).with_padding(PaddingMode::Same);
        let input = vec![1.0; 3 * 8 * 8]; // 3 channels, 8x8 image

        let output = conv.forward(&input, 8, 8);
        // 16 output channels, same spatial size
        assert_eq!(output.len(), 16 * 8 * 8);
    }

    #[test]
    fn test_maxpool2d_creation() {
        let pool = MaxPool2D::new((2, 2));
        assert_eq!(pool.pool_size(), (2, 2));
    }

    #[test]
    fn test_maxpool2d_forward() {
        let pool = MaxPool2D::new((2, 2));
        let input = vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        ]; // 1 channel, 4x4 image

        let output = pool.forward(&input, 1, 4, 4);
        // With 2x2 pool: output is 2x2
        assert_eq!(output.len(), 2 * 2); // 1 output channel
                                         // Check max values from each 2x2 region
        assert_eq!(output[0], 6.0); // max of [1,2,5,6]
        assert_eq!(output[1], 8.0); // max of [3,4,7,8]
        assert_eq!(output[2], 14.0); // max of [9,10,13,14]
        assert_eq!(output[3], 16.0); // max of [11,12,15,16]
    }

    #[test]
    fn test_maxpool2d_multi_channel() {
        let pool = MaxPool2D::new((2, 2));
        let input = vec![1.0; 3 * 8 * 8]; // 3 channels, 8x8

        let output = pool.forward(&input, 3, 8, 8);
        // 3 channels, 4x4 after pooling
        assert_eq!(output.len(), 3 * 4 * 4);
    }

    #[test]
    fn test_avgpool2d_creation() {
        let pool = AvgPool2D::new((2, 2));
        assert_eq!(pool.pool_size(), (2, 2));
    }

    #[test]
    fn test_avgpool2d_forward() {
        let pool = AvgPool2D::new((2, 2));
        let input = vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        ]; // 1 channel, 4x4 image

        let output = pool.forward(&input, 1, 4, 4);
        assert_eq!(output.len(), 2 * 2); // 1 output channel
                                         // Check average values from each 2x2 region
        assert_eq!(output[0], 3.5); // avg of [1,2,5,6]
        assert_eq!(output[1], 5.5); // avg of [3,4,7,8]
        assert_eq!(output[2], 11.5); // avg of [9,10,13,14]
        assert_eq!(output[3], 13.5); // avg of [11,12,15,16]
    }

    #[test]
    fn test_avgpool2d_with_stride() {
        let pool = AvgPool2D::new((2, 2)).with_stride((1, 1));
        let input = vec![1.0; 4 * 4]; // 1 channel, 4x4

        let output = pool.forward(&input, 1, 4, 4);
        // With stride 1: (4-2)/1 + 1 = 3
        assert_eq!(output.len(), 3 * 3); // 1 output channel
    }
}
