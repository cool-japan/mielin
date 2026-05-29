//! Federated Learning Module
//!
//! Implements the FedAvg algorithm (McMahan et al., 2017) and related federated
//! learning abstractions. Designed for privacy-preserving distributed training
//! where data remains on client devices and only model updates are exchanged.
//!
//! # Architecture
//!
//! The module has three principal layers:
//! - **[`Aggregator`]** trait: extensible strategy pattern for weight aggregation
//! - **[`FederatedCoordinator`]**: server-side orchestrator that manages rounds
//! - **[`LocalTrainer`]**: client-side trainer that produces [`ClientUpdate`]s
//!
//! # Example
//!
//! ```rust,ignore
//! use mielin_tensor::federated::{FederatedCoordinator, LocalTrainer, xavier_init};
//!
//! // Server initialises the global model
//! let global = vec![xavier_init(4, 4)];
//! let mut coordinator = FederatedCoordinator::with_fedavg(global.clone());
//!
//! // Two clients train locally and submit updates
//! let trainer = LocalTrainer::new(0.01, 5);
//! let inputs  = Tensor::zeros(vec![8, 4]);
//! let targets = Tensor::zeros(vec![8, 4]);
//!
//! let update_a = trainer.train(&global, &inputs, &targets, 0).unwrap();
//! let update_b = trainer.train(&global, &inputs, &targets, 0).unwrap();
//!
//! coordinator.submit(update_a).unwrap();
//! coordinator.submit(update_b).unwrap();
//!
//! let new_global = coordinator.aggregate_round().unwrap();
//! ```

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use crate::error::TensorError;
use crate::matrix::Matrix;
use crate::tensor::Tensor;

// ─── Error ────────────────────────────────────────────────────────────────────

/// Errors that can occur in federated learning operations.
#[derive(Debug, Clone, PartialEq)]
pub enum FederatedError {
    /// No client updates were submitted before aggregation was requested.
    NoUpdates,
    /// A client submitted weights whose shape does not match the global model.
    ShapeMismatch {
        expected: Vec<usize>,
        got: Vec<usize>,
    },
    /// The weight vector supplied to an operation is empty.
    EmptyWeights,
    /// An operation was attempted before the current round was properly started.
    RoundNotStarted,
    /// A wrapped tensor-level error propagated up from a lower-level operation.
    TensorError(TensorError),
    /// A client submitted an update with `num_samples == 0`.
    ZeroSamples,
}

impl fmt::Display for FederatedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoUpdates => write!(f, "federated aggregation: no client updates submitted"),
            Self::ShapeMismatch { expected, got } => write!(
                f,
                "federated shape mismatch: expected {:?}, got {:?}",
                expected, got
            ),
            Self::EmptyWeights => write!(f, "federated: weight list is empty"),
            Self::RoundNotStarted => write!(
                f,
                "federated: aggregation attempted before round was started"
            ),
            Self::TensorError(e) => write!(f, "federated tensor error: {}", e),
            Self::ZeroSamples => {
                write!(f, "federated: client update has zero training samples")
            }
        }
    }
}

impl From<TensorError> for FederatedError {
    fn from(e: TensorError) -> Self {
        Self::TensorError(e)
    }
}

// ─── ClientUpdate ─────────────────────────────────────────────────────────────

/// A model update produced by one federated client after local training.
///
/// The update carries the full set of flattened weight tensors (one per layer)
/// together with provenance information used by the server-side aggregator.
#[derive(Debug, Clone)]
pub struct ClientUpdate {
    /// Flattened weight tensors for each layer, matching the global model shape.
    pub weights: Vec<Tensor<f32>>,
    /// Number of training samples this update was computed on.
    pub num_samples: usize,
    /// Optional loss value on the client's local validation set (informational).
    pub local_loss: Option<f32>,
    /// Round number this update was computed for.
    pub round: u64,
}

impl ClientUpdate {
    /// Create a new client update with the given weights and sample count.
    pub fn new(weights: Vec<Tensor<f32>>, num_samples: usize) -> Self {
        Self {
            weights,
            num_samples,
            local_loss: None,
            round: 0,
        }
    }

    /// Attach a local validation loss to the update (builder-style).
    pub fn with_loss(mut self, loss: f32) -> Self {
        self.local_loss = Some(loss);
        self
    }

    /// Attach a round number to the update (builder-style).
    pub fn with_round(mut self, round: u64) -> Self {
        self.round = round;
        self
    }

    /// Return the number of weight tensors (layers) in this update.
    pub fn weight_count(&self) -> usize {
        self.weights.len()
    }
}

// ─── Aggregator trait ─────────────────────────────────────────────────────────

/// Strategy interface for aggregating client weight updates into a new global model.
///
/// Implement this trait to introduce custom aggregation policies (e.g. median
/// aggregation, secure aggregation, Byzantine-robust aggregation).
pub trait Aggregator {
    /// Aggregate a slice of client updates into a single set of global weights.
    ///
    /// Returns an error if the update set is empty, contains shape-mismatched
    /// tensors, or contains zero-sample clients.
    fn aggregate(&self, updates: &[ClientUpdate]) -> Result<Vec<Tensor<f32>>, FederatedError>;

    /// Human-readable name of this aggregation strategy.
    fn name(&self) -> &'static str;
}

// ─── FedAvg ───────────────────────────────────────────────────────────────────

/// Federated Averaging (FedAvg) — McMahan et al., 2017.
///
/// Computes a weighted mean of client weights where the weight for each client
/// is proportional to its number of training samples:
///
/// ```text
/// w_global = Σ_k  (n_k / N) * w_k,   N = Σ_k n_k
/// ```
///
/// This is the canonical federated learning aggregation algorithm and is
/// unbiased when all clients draw data i.i.d. from the same distribution.
pub struct FedAvg;

impl Aggregator for FedAvg {
    fn aggregate(&self, updates: &[ClientUpdate]) -> Result<Vec<Tensor<f32>>, FederatedError> {
        if updates.is_empty() {
            return Err(FederatedError::NoUpdates);
        }

        // Validate: every client must have at least one sample.
        for update in updates {
            if update.num_samples == 0 {
                return Err(FederatedError::ZeroSamples);
            }
        }

        let num_layers = updates[0].weights.len();
        if num_layers == 0 {
            return Err(FederatedError::EmptyWeights);
        }

        // Validate shapes: every client must match the first client's shapes.
        let reference_shapes: Vec<&[usize]> =
            updates[0].weights.iter().map(|w| w.shape()).collect();

        for update in updates.iter().skip(1) {
            if update.weights.len() != num_layers {
                return Err(FederatedError::ShapeMismatch {
                    expected: alloc::vec![num_layers],
                    got: alloc::vec![update.weights.len()],
                });
            }
            for (client_weight, &ref_shape) in
                update.weights.iter().zip(reference_shapes.iter())
            {
                if client_weight.shape() != ref_shape {
                    return Err(FederatedError::ShapeMismatch {
                        expected: ref_shape.to_vec(),
                        got: client_weight.shape().to_vec(),
                    });
                }
            }
        }

        // Compute total samples for normalisation.
        let total_samples: usize = updates.iter().map(|u| u.num_samples).sum();

        // Accumulate weighted sums layer by layer.
        let mut aggregated: Vec<Tensor<f32>> = updates[0]
            .weights
            .iter()
            .map(|w| Tensor::zeros(w.shape().to_vec()))
            .collect();

        for update in updates {
            let weight_fraction = update.num_samples as f32 / total_samples as f32;
            for (layer_idx, client_layer) in update.weights.iter().enumerate() {
                // scaled = client_layer * (n_k / N)
                let scaled = client_layer.scale(weight_fraction);
                aggregated[layer_idx] = aggregated[layer_idx].add(&scaled);
            }
        }

        Ok(aggregated)
    }

    fn name(&self) -> &'static str {
        "FedAvg"
    }
}

// ─── UniformAvg ───────────────────────────────────────────────────────────────

/// Uniform Averaging — simple mean of client weights, ignoring sample counts.
///
/// Each client contributes equally regardless of the size of its local dataset.
/// This is equivalent to FedAvg when all clients have identical dataset sizes.
pub struct UniformAvg;

impl Aggregator for UniformAvg {
    fn aggregate(&self, updates: &[ClientUpdate]) -> Result<Vec<Tensor<f32>>, FederatedError> {
        if updates.is_empty() {
            return Err(FederatedError::NoUpdates);
        }

        let num_layers = updates[0].weights.len();
        if num_layers == 0 {
            return Err(FederatedError::EmptyWeights);
        }

        // Shape validation.
        let reference_shapes: Vec<&[usize]> =
            updates[0].weights.iter().map(|w| w.shape()).collect();

        for update in updates.iter().skip(1) {
            if update.weights.len() != num_layers {
                return Err(FederatedError::ShapeMismatch {
                    expected: alloc::vec![num_layers],
                    got: alloc::vec![update.weights.len()],
                });
            }
            for (client_weight, &ref_shape) in
                update.weights.iter().zip(reference_shapes.iter())
            {
                if client_weight.shape() != ref_shape {
                    return Err(FederatedError::ShapeMismatch {
                        expected: ref_shape.to_vec(),
                        got: client_weight.shape().to_vec(),
                    });
                }
            }
        }

        let n = updates.len() as f32;
        let inv_n = 1.0 / n;

        // Sum layers, then scale by 1/N.
        let mut aggregated: Vec<Tensor<f32>> = updates[0]
            .weights
            .iter()
            .map(|w| Tensor::zeros(w.shape().to_vec()))
            .collect();

        for update in updates {
            for (layer_idx, client_layer) in update.weights.iter().enumerate() {
                let scaled = client_layer.scale(inv_n);
                aggregated[layer_idx] = aggregated[layer_idx].add(&scaled);
            }
        }

        Ok(aggregated)
    }

    fn name(&self) -> &'static str {
        "UniformAvg"
    }
}

// ─── RoundMetrics ─────────────────────────────────────────────────────────────

/// Diagnostic metrics collected at the end of each federated round.
#[derive(Debug, Clone, Default)]
pub struct RoundMetrics {
    /// Round index (0-based) that produced these metrics.
    pub round: u64,
    /// Number of clients that participated in this round.
    pub num_clients: usize,
    /// Total number of training samples across all participating clients.
    pub total_samples: usize,
    /// Mean local validation loss across clients that reported one.
    pub avg_loss: Option<f32>,
    /// Mean pairwise L2 distance between client weight vectors (weight divergence).
    ///
    /// A high divergence value indicates that client data distributions are
    /// heterogeneous (non-i.i.d.), which can destabilise federated training.
    pub weight_divergence: f32,
}

// ─── FederatedCoordinator ─────────────────────────────────────────────────────

/// Server-side coordinator for a federated learning experiment.
///
/// The coordinator holds the authoritative global model, collects [`ClientUpdate`]s
/// during each round, and applies an [`Aggregator`] strategy to produce a new
/// global model at the end of each round.
pub struct FederatedCoordinator {
    global_weights: Vec<Tensor<f32>>,
    pending_updates: Vec<ClientUpdate>,
    round: u64,
    aggregator: Box<dyn Aggregator>,
    metrics: RoundMetrics,
}

impl FederatedCoordinator {
    /// Create a coordinator with an arbitrary aggregation strategy.
    pub fn new(
        initial_weights: Vec<Tensor<f32>>,
        aggregator: Box<dyn Aggregator>,
    ) -> Self {
        Self {
            global_weights: initial_weights,
            pending_updates: Vec::new(),
            round: 0,
            aggregator,
            metrics: RoundMetrics::default(),
        }
    }

    /// Create a coordinator using the standard FedAvg aggregator.
    pub fn with_fedavg(initial_weights: Vec<Tensor<f32>>) -> Self {
        Self::new(initial_weights, Box::new(FedAvg))
    }

    /// Submit a client update for the current round.
    ///
    /// Returns [`FederatedError::ShapeMismatch`] if the update's weight shapes
    /// are incompatible with the global model.
    pub fn submit(&mut self, update: ClientUpdate) -> Result<(), FederatedError> {
        // Validate shape compatibility against the current global model.
        if update.weights.len() != self.global_weights.len() {
            return Err(FederatedError::ShapeMismatch {
                expected: alloc::vec![self.global_weights.len()],
                got: alloc::vec![update.weights.len()],
            });
        }
        for (client_w, global_w) in update.weights.iter().zip(self.global_weights.iter()) {
            if client_w.shape() != global_w.shape() {
                return Err(FederatedError::ShapeMismatch {
                    expected: global_w.shape().to_vec(),
                    got: client_w.shape().to_vec(),
                });
            }
        }
        self.pending_updates.push(update);
        Ok(())
    }

    /// Aggregate all pending client updates, advance the round counter, and
    /// return a clone of the new global model.
    ///
    /// After this call:
    /// - `pending_updates` is cleared.
    /// - `round` is incremented by one.
    /// - `metrics` reflects the round that just completed.
    pub fn aggregate_round(&mut self) -> Result<Vec<Tensor<f32>>, FederatedError> {
        if self.pending_updates.is_empty() {
            return Err(FederatedError::NoUpdates);
        }

        // Collect metrics before consuming the pending updates.
        let num_clients = self.pending_updates.len();
        let total_samples: usize = self.pending_updates.iter().map(|u| u.num_samples).sum();

        let loss_reports: Vec<f32> = self
            .pending_updates
            .iter()
            .filter_map(|u| u.local_loss)
            .collect();
        let avg_loss = if loss_reports.is_empty() {
            None
        } else {
            let sum: f32 = loss_reports.iter().sum();
            Some(sum / loss_reports.len() as f32)
        };

        let divergence = Self::compute_divergence(&self.pending_updates);

        // Run aggregation.
        let new_weights = self.aggregator.aggregate(&self.pending_updates)?;

        // Update state.
        self.global_weights = new_weights;
        self.metrics = RoundMetrics {
            round: self.round,
            num_clients,
            total_samples,
            avg_loss,
            weight_divergence: divergence,
        };
        self.pending_updates.clear();
        self.round += 1;

        Ok(self.global_weights.clone())
    }

    /// Current global model weights (read-only view).
    pub fn global_weights(&self) -> &[Tensor<f32>] {
        &self.global_weights
    }

    /// Round number for the *next* round (number of completed rounds so far).
    pub fn round(&self) -> u64 {
        self.round
    }

    /// Number of client updates queued for the current (not-yet-aggregated) round.
    pub fn pending_count(&self) -> usize {
        self.pending_updates.len()
    }

    /// Metrics from the most recently completed round.
    pub fn last_metrics(&self) -> &RoundMetrics {
        &self.metrics
    }

    /// Compute mean pairwise L2 distance between client weight vectors.
    ///
    /// All layer weights for each client are concatenated into a single flat
    /// vector before computing the pairwise distances. This gives a scalar
    /// measure of how heterogeneous the client updates are.
    fn compute_divergence(updates: &[ClientUpdate]) -> f32 {
        let n = updates.len();
        if n < 2 {
            return 0.0;
        }

        // Flatten each client's weights into a single vector for distance computation.
        let flat: Vec<Vec<f32>> = updates
            .iter()
            .map(|u| {
                u.weights
                    .iter()
                    .flat_map(|w| w.data().iter().copied())
                    .collect()
            })
            .collect();

        let mut total_distance = 0.0f32;
        let mut pair_count = 0usize;

        for i in 0..n {
            for j in (i + 1)..n {
                let dist_sq: f32 = flat[i]
                    .iter()
                    .zip(flat[j].iter())
                    .map(|(a, b)| {
                        let d = a - b;
                        d * d
                    })
                    .sum();
                total_distance += libm::sqrtf(dist_sq);
                pair_count += 1;
            }
        }

        if pair_count == 0 {
            0.0
        } else {
            total_distance / pair_count as f32
        }
    }
}

// ─── LocalTrainer ─────────────────────────────────────────────────────────────

/// Client-side trainer that performs local SGD and returns a [`ClientUpdate`].
///
/// This simplified trainer handles a single-layer linear model (one weight matrix)
/// using mean-squared-error loss and vanilla gradient descent. A production
/// implementation would use the full autograd module to support arbitrary model
/// architectures with arbitrary loss functions.
///
/// # Training procedure (one epoch)
///
/// Given input matrix `X` (n × d_in) and targets `Y` (n × d_out):
///
/// 1. **Forward**: `Ŷ = X W`   where `W` is the weight matrix (d_in × d_out).
/// 2. **Loss**: `L = (1/n) Σ (Ŷ - Y)²`
/// 3. **Gradient**: `∇W = (2/n) Xᵀ (Ŷ - Y)`
/// 4. **Update**: `W ← W − lr ∇W`
pub struct LocalTrainer {
    learning_rate: f32,
    local_epochs: usize,
    batch_size: usize,
}

impl LocalTrainer {
    /// Create a new local trainer with the given learning rate and epoch count.
    pub fn new(learning_rate: f32, local_epochs: usize) -> Self {
        Self {
            learning_rate,
            local_epochs,
            batch_size: 32,
        }
    }

    /// Override the default mini-batch size (builder-style).
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Train starting from `global_weights` on `(inputs, targets)` for
    /// `local_epochs` steps using SGD and MSE loss.
    ///
    /// Only the first weight tensor (`global_weights[0]`) is trained; additional
    /// tensors are carried through unchanged. This matches the intended use-case
    /// of a single linear projection layer.
    ///
    /// # Arguments
    /// - `global_weights` — the current global model (starting point for local SGD)
    /// - `inputs` — input matrix of shape `[n_samples, d_in]`
    /// - `targets` — target matrix of shape `[n_samples, d_out]`
    /// - `round` — the current federated round (embedded in the returned update)
    ///
    /// # Returns
    /// A [`ClientUpdate`] containing the post-training weights and the final MSE loss.
    pub fn train(
        &self,
        global_weights: &[Tensor<f32>],
        inputs: &Tensor<f32>,
        targets: &Tensor<f32>,
        round: u64,
    ) -> Result<ClientUpdate, FederatedError> {
        if global_weights.is_empty() {
            return Err(FederatedError::EmptyWeights);
        }

        // Validate that inputs and targets are 2-D.
        if inputs.ndim() != 2 {
            return Err(FederatedError::TensorError(TensorError::dimension_mismatch(
                "LocalTrainer::train inputs",
                2,
                inputs.ndim(),
            )));
        }
        if targets.ndim() != 2 {
            return Err(FederatedError::TensorError(TensorError::dimension_mismatch(
                "LocalTrainer::train targets",
                2,
                targets.ndim(),
            )));
        }

        let n_samples = inputs.shape()[0];
        let d_in = inputs.shape()[1];
        let d_out = targets.shape()[1];

        // Start from a clone of the global weights.
        let mut weights: Vec<Tensor<f32>> = global_weights.to_vec();

        // Validate the first weight tensor has shape [d_in, d_out].
        let w_shape = weights[0].shape();
        if w_shape.len() != 2 || w_shape[0] != d_in || w_shape[1] != d_out {
            return Err(FederatedError::ShapeMismatch {
                expected: alloc::vec![d_in, d_out],
                got: w_shape.to_vec(),
            });
        }

        let mut final_loss = 0.0f32;

        for _epoch in 0..self.local_epochs {
            // Forward pass: output = inputs (n×d_in) · W (d_in×d_out) → n×d_out
            let output = Self::matmul_2d(inputs, &weights[0])?;

            // Residual: (output - targets), shape n×d_out
            let residual = output.sub(targets);

            // MSE loss = mean((output - targets)²)
            let sq_sum: f32 = residual.data().iter().map(|&v| v * v).sum();
            final_loss = sq_sum / (n_samples * d_out) as f32;

            // Gradient: dW = (2/n) * Xᵀ · (output - targets)
            //   inputs_T : d_in × n
            //   residual : n × d_out
            //   dW       : d_in × d_out
            let inputs_t = Matrix::transpose(inputs).map_err(|_| {
                FederatedError::TensorError(TensorError::other(
                    "transpose failed during gradient computation",
                ))
            })?;

            let grad = Self::matmul_2d(&inputs_t, &residual)?;
            let scale = 2.0 / (n_samples * d_out) as f32;
            let scaled_grad = grad.scale(scale);

            // SGD update: W = W - lr * dW
            let lr_grad = scaled_grad.scale(self.learning_rate);
            weights[0] = weights[0].sub(&lr_grad);
        }

        let update = ClientUpdate::new(weights, n_samples)
            .with_loss(final_loss)
            .with_round(round);

        Ok(update)
    }

    /// Perform matrix multiplication A (m×k) · B (k×n) → C (m×n) using scalar
    /// fallback so there is no dependency on the hardware-dispatched TensorOps.
    fn matmul_2d(a: &Tensor<f32>, b: &Tensor<f32>) -> Result<Tensor<f32>, FederatedError> {
        if a.ndim() != 2 || b.ndim() != 2 {
            return Err(FederatedError::TensorError(TensorError::other(
                "matmul_2d requires 2-D tensors",
            )));
        }
        let m = a.shape()[0];
        let k_a = a.shape()[1];
        let k_b = b.shape()[0];
        let n = b.shape()[1];

        if k_a != k_b {
            return Err(FederatedError::TensorError(TensorError::shape_mismatch(
                "matmul_2d",
                alloc::vec![m, k_a],
                alloc::vec![k_b, n],
            )));
        }

        let mut result_data = alloc::vec![0.0f32; m * n];

        for i in 0..m {
            for j in 0..n {
                let mut acc = 0.0f32;
                for l in 0..k_a {
                    let a_val = *a.get(&[i, l]).ok_or_else(|| {
                        FederatedError::TensorError(TensorError::other(
                            "matmul index out of range",
                        ))
                    })?;
                    let b_val = *b.get(&[l, j]).ok_or_else(|| {
                        FederatedError::TensorError(TensorError::other(
                            "matmul index out of range",
                        ))
                    })?;
                    acc += a_val * b_val;
                }
                result_data[i * n + j] = acc;
            }
        }

        Tensor::from_vec(result_data, alloc::vec![m, n]).ok_or_else(|| {
            FederatedError::TensorError(TensorError::other(
                "matmul result tensor creation failed",
            ))
        })
    }
}

// ─── xavier_init ──────────────────────────────────────────────────────────────

/// Xavier/Glorot uniform initialisation for a weight matrix of shape `[rows, cols]`.
///
/// Uses a deterministic linear sequence instead of random sampling so that
/// results are reproducible without a random number generator. The distribution
/// is centred at zero with scale `sqrt(6 / (rows + cols))`, matching the
/// variance target of the original Glorot & Bengio (2010) paper.
///
/// The deterministic filling uses:
///
/// ```text
/// W[i] = scale * (2 * t - 1),   t = i / (rows * cols - 1)
/// ```
///
/// which sweeps uniformly from `-scale` to `+scale`.
pub fn xavier_init(rows: usize, cols: usize) -> Tensor<f32> {
    let fan_in = rows;
    let fan_out = cols;
    let scale = libm::sqrtf(6.0 / (fan_in + fan_out) as f32);
    let size = rows * cols;

    let data: Vec<f32> = (0..size)
        .map(|i| {
            let t = if size > 1 {
                i as f32 / (size - 1) as f32
            } else {
                0.5
            };
            scale * (2.0 * t - 1.0)
        })
        .collect();

    Tensor::from_vec(data, alloc::vec![rows, cols])
        .expect("xavier_init: size matches shape product")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Build a single-layer weight list filled with a constant value.
    fn make_weights(rows: usize, cols: usize, val: f32) -> Vec<Tensor<f32>> {
        alloc::vec![
            Tensor::from_vec(alloc::vec![val; rows * cols], alloc::vec![rows, cols]).unwrap()
        ]
    }

    /// Build a `ClientUpdate` using `make_weights`.
    fn make_update(val: f32, samples: usize, round: u64) -> ClientUpdate {
        ClientUpdate::new(make_weights(2, 2, val), samples).with_round(round)
    }

    // ── FedAvg tests ──────────────────────────────────────────────────────────

    /// Three clients with identical weights and equal sample counts → aggregate
    /// must equal that same weight value.
    #[test]
    fn test_fedavg_uniform_weights_equal() {
        let updates = alloc::vec![
            make_update(3.0, 100, 0),
            make_update(3.0, 100, 0),
            make_update(3.0, 100, 0),
        ];
        let result = FedAvg.aggregate(&updates).unwrap();
        for val in result[0].data() {
            assert!(
                (val - 3.0).abs() < 1e-5,
                "expected 3.0 but got {val}"
            );
        }
    }

    /// Equal sample counts, different weight values → simple average.
    /// A: 1.0 × 100 samples, B: 3.0 × 100 samples → expected mean = 2.0.
    #[test]
    fn test_fedavg_weighted_by_samples() {
        let updates = alloc::vec![make_update(1.0, 100, 0), make_update(3.0, 100, 0)];
        let result = FedAvg.aggregate(&updates).unwrap();
        for val in result[0].data() {
            assert!(
                (val - 2.0).abs() < 1e-5,
                "expected 2.0 but got {val}"
            );
        }
    }

    /// Unequal sample counts → sample-weighted mean.
    /// A: 2.0 × 10 samples, B: 8.0 × 90 samples.
    /// Expected: (2.0*10 + 8.0*90) / 100 = 7.4
    #[test]
    fn test_fedavg_sample_weighted() {
        let updates = alloc::vec![make_update(2.0, 10, 0), make_update(8.0, 90, 0)];
        let result = FedAvg.aggregate(&updates).unwrap();
        let expected = (2.0f32 * 10.0 + 8.0f32 * 90.0) / 100.0;
        for val in result[0].data() {
            assert!(
                (val - expected).abs() < 1e-4,
                "expected {expected} but got {val}"
            );
        }
    }

    /// A single-client aggregate must return exactly the client's own weights.
    #[test]
    fn test_fedavg_single_client_identity() {
        let updates = alloc::vec![make_update(5.5, 50, 0)];
        let result = FedAvg.aggregate(&updates).unwrap();
        for val in result[0].data() {
            assert!(
                (val - 5.5).abs() < 1e-5,
                "expected 5.5 but got {val}"
            );
        }
    }

    /// Empty update list → `NoUpdates` error.
    #[test]
    fn test_fedavg_empty_updates() {
        let result = FedAvg.aggregate(&[]);
        assert_eq!(result.unwrap_err(), FederatedError::NoUpdates);
    }

    /// Clients with different tensor shapes → `ShapeMismatch` error.
    #[test]
    fn test_fedavg_shape_mismatch() {
        let update_a = ClientUpdate::new(make_weights(2, 2, 1.0), 10);
        let update_b = ClientUpdate::new(make_weights(3, 3, 1.0), 10); // different shape
        let result = FedAvg.aggregate(&[update_a, update_b]);
        assert!(
            matches!(result.unwrap_err(), FederatedError::ShapeMismatch { .. }),
            "expected ShapeMismatch"
        );
    }

    /// A client with `num_samples == 0` → `ZeroSamples` error.
    #[test]
    fn test_fedavg_zero_samples() {
        let updates = alloc::vec![make_update(1.0, 0, 0)];
        let result = FedAvg.aggregate(&updates);
        assert_eq!(result.unwrap_err(), FederatedError::ZeroSamples);
    }

    /// UniformAvg ignores sample counts: A=1.0 n=10, B=3.0 n=1 → mean=2.0.
    #[test]
    fn test_uniform_avg_equal_weighting() {
        let update_a = make_update(1.0, 10, 0);
        let update_b = make_update(3.0, 1, 0);
        let result = UniformAvg.aggregate(&[update_a, update_b]).unwrap();
        for val in result[0].data() {
            assert!(
                (val - 2.0).abs() < 1e-5,
                "expected 2.0 (uniform) but got {val}"
            );
        }
    }

    // ── FederatedCoordinator tests ────────────────────────────────────────────

    /// Submit two updates, aggregate, and verify the round advances and weights
    /// are the FedAvg-weighted mean.
    #[test]
    fn test_coordinator_aggregate_round() {
        let mut coord = FederatedCoordinator::with_fedavg(make_weights(2, 2, 0.0));
        coord.submit(make_update(2.0, 50, 0)).unwrap();
        coord.submit(make_update(4.0, 50, 0)).unwrap();
        let new_weights = coord.aggregate_round().unwrap();
        for val in new_weights[0].data() {
            assert!((val - 3.0).abs() < 1e-5, "expected 3.0 but got {val}");
        }
        assert_eq!(coord.round(), 1);
    }

    /// Coordinator starts at round 0 and increments after each aggregate.
    #[test]
    fn test_coordinator_round_counter() {
        let mut coord = FederatedCoordinator::with_fedavg(make_weights(2, 2, 0.0));
        assert_eq!(coord.round(), 0);
        coord.submit(make_update(1.0, 10, 0)).unwrap();
        coord.aggregate_round().unwrap();
        assert_eq!(coord.round(), 1);
        coord.submit(make_update(1.0, 10, 1)).unwrap();
        coord.aggregate_round().unwrap();
        assert_eq!(coord.round(), 2);
    }

    /// `pending_count` tracks submissions and clears after aggregation.
    #[test]
    fn test_coordinator_pending_count() {
        let mut coord = FederatedCoordinator::with_fedavg(make_weights(2, 2, 0.0));
        assert_eq!(coord.pending_count(), 0);
        coord.submit(make_update(1.0, 10, 0)).unwrap();
        coord.submit(make_update(2.0, 10, 0)).unwrap();
        coord.submit(make_update(3.0, 10, 0)).unwrap();
        assert_eq!(coord.pending_count(), 3);
        coord.aggregate_round().unwrap();
        assert_eq!(coord.pending_count(), 0);
    }

    /// End-to-end flow with FedAvg: weights should change after aggregation.
    #[test]
    fn test_coordinator_with_fedavg() {
        let initial = make_weights(2, 2, 0.0);
        let mut coord = FederatedCoordinator::with_fedavg(initial);
        coord.submit(make_update(2.0, 30, 0)).unwrap();
        coord.submit(make_update(8.0, 70, 0)).unwrap();
        let result = coord.aggregate_round().unwrap();
        // Expected: (2.0*30 + 8.0*70) / 100 = (60 + 560) / 100 = 6.2
        let expected = (2.0f32 * 30.0 + 8.0f32 * 70.0) / 100.0;
        for val in result[0].data() {
            assert!(
                (val - expected).abs() < 1e-4,
                "expected {expected} but got {val}"
            );
        }
    }

    // ── LocalTrainer tests ────────────────────────────────────────────────────

    /// Running 10 epochs on linearly separable data should reduce the MSE loss.
    #[test]
    fn test_local_trainer_reduces_loss() {
        // Simple linear relationship: Y = X·I (identity mapping, d_in = d_out = 2).
        // With W initialised to zeros the initial loss equals mean(Y²).
        // After 10 gradient steps the loss must be strictly lower.
        let n = 8usize;
        let d_in = 2usize;
        let d_out = 2usize;
        let input_data: Vec<f32> = alloc::vec![
            1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 0.0, 0.0, 2.0, 2.0, 1.0, 1.0, 2.0, 2.0, 2.0,
        ];
        let target_data = input_data.clone();

        let inputs = Tensor::from_vec(input_data, alloc::vec![n, d_in]).unwrap();
        let targets = Tensor::from_vec(target_data, alloc::vec![n, d_out]).unwrap();

        // Initial loss when W = 0: output = 0, residual = -targets.
        let initial_loss = {
            let sq_sum: f32 = targets.data().iter().map(|&v| v * v).sum();
            sq_sum / (n * d_out) as f32
        };

        let initial_weights = make_weights(d_in, d_out, 0.0);
        let trainer = LocalTrainer::new(0.05, 10);
        let update = trainer
            .train(&initial_weights, &inputs, &targets, 0)
            .unwrap();

        let final_loss = update.local_loss.expect("local_loss should be set");
        assert!(
            final_loss < initial_loss,
            "expected loss to decrease: initial={initial_loss}, final={final_loss}"
        );
    }

    /// Trainer produces an update with the correct round and `num_samples`.
    #[test]
    fn test_local_trainer_produces_update() {
        let n = 4usize;
        let d = 2usize;
        let inputs = Tensor::zeros(alloc::vec![n, d]);
        let targets = Tensor::zeros(alloc::vec![n, d]);
        let global_w = make_weights(d, d, 1.0);

        let trainer = LocalTrainer::new(0.01, 3);
        let update = trainer.train(&global_w, &inputs, &targets, 7).unwrap();

        assert_eq!(update.num_samples, n);
        assert_eq!(update.round, 7);
        assert!(update.local_loss.is_some());
    }

    // ── Metrics tests ─────────────────────────────────────────────────────────

    /// After `aggregate_round`, metrics must reflect the completed round.
    #[test]
    fn test_coordinator_metrics_after_round() {
        let mut coord = FederatedCoordinator::with_fedavg(make_weights(2, 2, 0.0));
        let u1 = ClientUpdate::new(make_weights(2, 2, 1.0), 40)
            .with_loss(0.5)
            .with_round(0);
        let u2 = ClientUpdate::new(make_weights(2, 2, 3.0), 60)
            .with_loss(0.3)
            .with_round(0);

        coord.submit(u1).unwrap();
        coord.submit(u2).unwrap();
        coord.aggregate_round().unwrap();

        let m = coord.last_metrics();
        assert_eq!(m.num_clients, 2);
        assert_eq!(m.total_samples, 100);
        assert!(m.avg_loss.is_some());
        // avg_loss = (0.5 + 0.3) / 2 = 0.4
        assert!(
            (m.avg_loss.unwrap() - 0.4).abs() < 1e-5,
            "expected avg_loss ≈ 0.4 but got {:?}",
            m.avg_loss
        );
    }

    /// When all clients submit identical weights, divergence must be ≈ 0.0.
    #[test]
    fn test_round_metrics_divergence_zero_same_clients() {
        let mut coord = FederatedCoordinator::with_fedavg(make_weights(2, 2, 1.0));
        coord.submit(make_update(1.0, 50, 0)).unwrap();
        coord.submit(make_update(1.0, 50, 0)).unwrap();
        coord.submit(make_update(1.0, 50, 0)).unwrap();
        coord.aggregate_round().unwrap();

        let divergence = coord.last_metrics().weight_divergence;
        assert!(
            divergence < 1e-5,
            "expected near-zero divergence for identical clients but got {divergence}"
        );
    }

    /// `xavier_init` must produce a tensor with the correct shape.
    #[test]
    fn test_xavier_init_shape() {
        let w = xavier_init(3, 4);
        assert_eq!(w.shape(), &[3, 4]);
        assert_eq!(w.size(), 12);
    }

    /// `xavier_init` values must be within the expected `[-scale, +scale]` range.
    #[test]
    fn test_xavier_init_range() {
        let rows = 8;
        let cols = 16;
        let w = xavier_init(rows, cols);
        let scale = libm::sqrtf(6.0 / (rows + cols) as f32);
        for &val in w.data() {
            assert!(
                val >= -scale - 1e-6 && val <= scale + 1e-6,
                "xavier value {val} outside expected range [-{scale}, {scale}]"
            );
        }
    }

    /// Aggregator name accessors return the expected string constants.
    #[test]
    fn test_aggregator_names() {
        assert_eq!(FedAvg.name(), "FedAvg");
        assert_eq!(UniformAvg.name(), "UniformAvg");
    }

    /// `FederatedError` Display output is non-empty for every variant.
    #[test]
    fn test_error_display() {
        use alloc::string::ToString;
        assert!(!FederatedError::NoUpdates.to_string().is_empty());
        assert!(!FederatedError::EmptyWeights.to_string().is_empty());
        assert!(!FederatedError::RoundNotStarted.to_string().is_empty());
        assert!(!FederatedError::ZeroSamples.to_string().is_empty());
        let e = FederatedError::ShapeMismatch {
            expected: alloc::vec![2, 2],
            got: alloc::vec![3, 3],
        };
        assert!(e.to_string().contains("[2, 2]"));
    }
}
