//! Loss Functions Module
//!
//! Provides common loss functions for neural network training.
//! All loss functions support both element-wise and reduction modes.
//!
//! # Features
//! - Regression losses (MSE, MAE, Huber)
//! - Classification losses (CrossEntropy, BCE, Hinge)
//! - Ranking losses (Triplet, Contrastive)
//! - Reduction modes (mean, sum, none)
//! - Numerical stability for log-based losses
//!
//! # Examples
//! ```rust,ignore
//! use mielin_tensor::loss::{mse_loss, cross_entropy_loss, Reduction};
//!
//! let predictions = Tensor::vector(vec![0.8, 0.2, 0.5]);
//! let targets = Tensor::vector(vec![1.0, 0.0, 1.0]);
//!
//! let loss = mse_loss(&predictions, &targets, Reduction::Mean);
//! ```

#![allow(dead_code)]

extern crate alloc;

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::vec::Vec;

/// Reduction mode for loss functions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reduction {
    /// No reduction, return per-element losses
    None,
    /// Sum all losses
    Sum,
    /// Average all losses (default)
    Mean,
}

/// Mean Squared Error (MSE) Loss
///
/// L = (1/n) * Σ(y_pred - y_true)²
///
/// Common for regression tasks.
pub fn mse_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "mse_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    // Compute (pred - target)^2
    let mut losses = Vec::with_capacity(predictions.data().len());
    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        let diff = pred - target;
        losses.push(diff * diff);
    }

    let loss_tensor = Tensor::from_vec(losses, predictions.shape().to_vec()).ok_or_else(|| {
        TensorError::Other {
            message: "Failed to create loss tensor".into(),
        }
    })?;

    apply_reduction(&loss_tensor, reduction)
}

/// Mean Absolute Error (MAE) Loss / L1 Loss
///
/// L = (1/n) * Σ|y_pred - y_true|
///
/// More robust to outliers than MSE.
pub fn mae_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "mae_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    let mut losses = Vec::with_capacity(predictions.data().len());
    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        losses.push(libm::fabsf(pred - target));
    }

    let loss_tensor = Tensor::from_vec(losses, predictions.shape().to_vec()).ok_or_else(|| {
        TensorError::Other {
            message: "Failed to create loss tensor".into(),
        }
    })?;

    apply_reduction(&loss_tensor, reduction)
}

/// Huber Loss (Smooth L1 Loss)
///
/// Combines MSE for small errors and MAE for large errors.
/// Less sensitive to outliers than MSE.
///
/// L = { 0.5 * (y_pred - y_true)² if |y_pred - y_true| < delta
///     { delta * (|y_pred - y_true| - 0.5 * delta) otherwise
pub fn huber_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    delta: f32,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "huber_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    let mut losses = Vec::with_capacity(predictions.data().len());
    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        let diff = pred - target;
        let abs_diff = libm::fabsf(diff);

        let loss = if abs_diff < delta {
            0.5 * diff * diff
        } else {
            delta * (abs_diff - 0.5 * delta)
        };
        losses.push(loss);
    }

    let loss_tensor = Tensor::from_vec(losses, predictions.shape().to_vec()).ok_or_else(|| {
        TensorError::Other {
            message: "Failed to create loss tensor".into(),
        }
    })?;

    apply_reduction(&loss_tensor, reduction)
}

/// Binary Cross Entropy (BCE) Loss
///
/// L = -[y * log(p) + (1-y) * log(1-p)]
///
/// For binary classification with sigmoid outputs.
pub fn binary_cross_entropy_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "binary_cross_entropy_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    const EPS: f32 = 1e-7;
    let mut losses = Vec::with_capacity(predictions.data().len());

    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        // Clamp predictions to avoid log(0)
        let pred_clamped = pred.clamp(EPS, 1.0 - EPS);
        let loss =
            -(target * libm::logf(pred_clamped) + (1.0 - target) * libm::logf(1.0 - pred_clamped));
        losses.push(loss);
    }

    let loss_tensor = Tensor::from_vec(losses, predictions.shape().to_vec()).ok_or_else(|| {
        TensorError::Other {
            message: "Failed to create loss tensor".into(),
        }
    })?;

    apply_reduction(&loss_tensor, reduction)
}

/// Cross Entropy Loss (multi-class classification)
///
/// L = -Σ y_true * log(y_pred)
///
/// Expects predictions to be probabilities (post-softmax).
pub fn cross_entropy_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "cross_entropy_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    const EPS: f32 = 1e-7;

    // For batched inputs [batch_size, num_classes]
    if predictions.shape().len() == 2 {
        let batch_size = predictions.shape()[0];
        let num_classes = predictions.shape()[1];
        let mut batch_losses = Vec::with_capacity(batch_size);

        for b in 0..batch_size {
            let mut sample_loss = 0.0;
            for c in 0..num_classes {
                let idx = b * num_classes + c;
                let pred = predictions.data()[idx].max(EPS);
                let target = targets.data()[idx];
                sample_loss -= target * libm::logf(pred);
            }
            batch_losses.push(sample_loss);
        }

        let loss_tensor = Tensor::vector(batch_losses);
        apply_reduction(&loss_tensor, reduction)
    } else {
        // Single sample
        let mut total_loss = 0.0;
        for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
            let pred_clamped = pred.max(EPS);
            total_loss -= target * libm::logf(pred_clamped);
        }

        Ok(Tensor::scalar(total_loss))
    }
}

/// Sparse Cross Entropy Loss
///
/// More efficient when targets are class indices rather than one-hot vectors.
///
/// # Arguments
/// * `predictions` - Probabilities `[batch_size, num_classes]`
/// * `target_indices` - Class indices `[batch_size]`
pub fn sparse_cross_entropy_loss(
    predictions: &Tensor<f32>,
    target_indices: &[usize],
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape().len() != 2 {
        return Err(TensorError::dimension_mismatch(
            "sparse_cross_entropy_loss",
            2,
            predictions.shape().len(),
        ));
    }

    let batch_size = predictions.shape()[0];
    let num_classes = predictions.shape()[1];

    if target_indices.len() != batch_size {
        return Err(TensorError::Other {
            message: "Target indices length must match batch size".into(),
        });
    }

    const EPS: f32 = 1e-7;
    let mut losses = Vec::with_capacity(batch_size);

    for (b, &target_idx) in target_indices.iter().enumerate() {
        if target_idx >= num_classes {
            return Err(TensorError::Other {
                message: "Target index out of bounds".into(),
            });
        }

        let idx = b * num_classes + target_idx;
        let pred = predictions.data()[idx].max(EPS);
        losses.push(-libm::logf(pred));
    }

    let loss_tensor = Tensor::vector(losses);
    apply_reduction(&loss_tensor, reduction)
}

/// Hinge Loss (SVM loss)
///
/// L = max(0, 1 - y_true * y_pred)
///
/// For binary classification with targets in {-1, 1}.
pub fn hinge_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "hinge_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    let mut losses = Vec::with_capacity(predictions.data().len());
    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        let loss = (1.0 - target * pred).max(0.0);
        losses.push(loss);
    }

    let loss_tensor = Tensor::from_vec(losses, predictions.shape().to_vec()).ok_or_else(|| {
        TensorError::Other {
            message: "Failed to create loss tensor".into(),
        }
    })?;

    apply_reduction(&loss_tensor, reduction)
}

/// Kullback-Leibler Divergence Loss
///
/// L = Σ y_true * log(y_true / y_pred)
///
/// Measures how one probability distribution diverges from another.
pub fn kl_div_loss(
    predictions: &Tensor<f32>,
    targets: &Tensor<f32>,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "kl_div_loss",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    const EPS: f32 = 1e-7;
    let mut losses = Vec::with_capacity(predictions.data().len());

    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        if *target > EPS {
            let pred_clamped = pred.max(EPS);
            let loss = target * libm::logf(target / pred_clamped);
            losses.push(loss);
        } else {
            losses.push(0.0);
        }
    }

    let loss_tensor = Tensor::from_vec(losses, predictions.shape().to_vec()).ok_or_else(|| {
        TensorError::Other {
            message: "Failed to create loss tensor".into(),
        }
    })?;

    apply_reduction(&loss_tensor, reduction)
}

/// Cosine Embedding Loss
///
/// Measures the cosine similarity between two vectors.
/// Useful for learning embeddings.
///
/// L = { 1 - cos(x1, x2)           if y = 1
///     { max(0, cos(x1, x2) - margin) if y = -1
pub fn cosine_embedding_loss(
    input1: &Tensor<f32>,
    input2: &Tensor<f32>,
    target: f32,
    margin: f32,
    reduction: Reduction,
) -> TensorResult<Tensor<f32>> {
    if input1.shape() != input2.shape() {
        return Err(TensorError::shape_mismatch(
            "cosine_embedding_loss",
            input2.shape().to_vec(),
            input1.shape().to_vec(),
        ));
    }

    // Compute cosine similarity
    let mut dot_product = 0.0;
    let mut norm1 = 0.0;
    let mut norm2 = 0.0;

    for (v1, v2) in input1.data().iter().zip(input2.data().iter()) {
        dot_product += v1 * v2;
        norm1 += v1 * v1;
        norm2 += v2 * v2;
    }

    let cosine_sim = dot_product / (libm::sqrtf(norm1) * libm::sqrtf(norm2) + 1e-8);

    let loss = if target > 0.0 {
        1.0 - cosine_sim
    } else {
        (cosine_sim - margin).max(0.0)
    };

    let loss_tensor = Tensor::scalar(loss);
    apply_reduction(&loss_tensor, reduction)
}

/// Apply reduction to a loss tensor
fn apply_reduction(loss: &Tensor<f32>, reduction: Reduction) -> TensorResult<Tensor<f32>> {
    match reduction {
        Reduction::None => Ok(loss.clone()),
        Reduction::Sum => {
            let sum: f32 = loss.data().iter().sum();
            Ok(Tensor::scalar(sum))
        }
        Reduction::Mean => {
            let sum: f32 = loss.data().iter().sum();
            let mean = sum / loss.data().len() as f32;
            Ok(Tensor::scalar(mean))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_mse_loss_mean() {
        let pred = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let target = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let loss = mse_loss(&pred, &target, Reduction::Mean).unwrap();
        assert_eq!(loss.data()[0], 0.0);
    }

    #[test]
    fn test_mse_loss_nonzero() {
        let pred = Tensor::vector(vec![2.0, 3.0, 4.0]);
        let target = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let loss = mse_loss(&pred, &target, Reduction::Mean).unwrap();
        // MSE = ((1)^2 + (1)^2 + (1)^2) / 3 = 1.0
        assert!((loss.data()[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_mae_loss() {
        let pred = Tensor::vector(vec![2.0, 3.0, 4.0]);
        let target = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let loss = mae_loss(&pred, &target, Reduction::Mean).unwrap();
        // MAE = (1 + 1 + 1) / 3 = 1.0
        assert!((loss.data()[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_huber_loss_small_error() {
        let pred = Tensor::vector(vec![1.1, 2.1, 3.1]);
        let target = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let loss = huber_loss(&pred, &target, 1.0, Reduction::Mean).unwrap();
        // All errors < delta, so uses quadratic: 0.5 * 0.1^2 * 3 / 3 = 0.005
        assert!((loss.data()[0] - 0.005).abs() < 1e-6);
    }

    #[test]
    fn test_huber_loss_large_error() {
        let pred = Tensor::vector(vec![3.0]);
        let target = Tensor::vector(vec![1.0]);

        let loss = huber_loss(&pred, &target, 1.0, Reduction::Mean).unwrap();
        // Error = 2.0 > delta = 1.0, so uses linear: 1.0 * (2.0 - 0.5 * 1.0) = 1.5
        assert!((loss.data()[0] - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_binary_cross_entropy() {
        let pred = Tensor::vector(vec![0.9, 0.1, 0.8]);
        let target = Tensor::vector(vec![1.0, 0.0, 1.0]);

        let loss = binary_cross_entropy_loss(&pred, &target, Reduction::Mean).unwrap();
        // Should be small since predictions match targets well
        assert!(loss.data()[0] > 0.0);
        assert!(loss.data()[0] < 0.5);
    }

    #[test]
    fn test_cross_entropy_single() {
        // Single sample: [0.7, 0.2, 0.1] (predictions after softmax)
        let pred = Tensor::vector(vec![0.7, 0.2, 0.1]);
        let target = Tensor::vector(vec![1.0, 0.0, 0.0]); // One-hot for class 0

        let loss = cross_entropy_loss(&pred, &target, Reduction::Mean).unwrap();
        // Loss = -log(0.7) ≈ 0.357
        assert!((loss.data()[0] - (-libm::logf(0.7))).abs() < 1e-4);
    }

    #[test]
    fn test_cross_entropy_batch() {
        // Batch of 2 samples, 3 classes each
        let pred = Tensor::from_vec(
            vec![
                0.7, 0.2, 0.1, // Sample 1
                0.1, 0.8, 0.1, // Sample 2
            ],
            vec![2, 3],
        )
        .unwrap();

        let target = Tensor::from_vec(
            vec![
                1.0, 0.0, 0.0, // Sample 1: class 0
                0.0, 1.0, 0.0, // Sample 2: class 1
            ],
            vec![2, 3],
        )
        .unwrap();

        let loss = cross_entropy_loss(&pred, &target, Reduction::Mean).unwrap();
        assert!(loss.data()[0] > 0.0);
    }

    #[test]
    fn test_sparse_cross_entropy() {
        let pred = Tensor::from_vec(
            vec![
                0.7, 0.2, 0.1, // Sample 1
                0.1, 0.8, 0.1, // Sample 2
            ],
            vec![2, 3],
        )
        .unwrap();

        let target_indices = vec![0, 1]; // Class indices

        let loss = sparse_cross_entropy_loss(&pred, &target_indices, Reduction::Mean).unwrap();
        assert!(loss.data()[0] > 0.0);
    }

    #[test]
    fn test_hinge_loss() {
        let pred = Tensor::vector(vec![0.8, -0.5, 0.3]);
        let target = Tensor::vector(vec![1.0, -1.0, 1.0]);

        let loss = hinge_loss(&pred, &target, Reduction::Mean).unwrap();
        // Hinge = max(0, 1 - y*pred)
        // Sample 1: max(0, 1 - 1*0.8) = 0.2
        // Sample 2: max(0, 1 - (-1)*(-0.5)) = max(0, 0.5) = 0.5
        // Sample 3: max(0, 1 - 1*0.3) = 0.7
        // Mean = (0.2 + 0.5 + 0.7) / 3 = 0.467
        assert!((loss.data()[0] - 0.4666667).abs() < 1e-4);
    }

    #[test]
    fn test_kl_div_loss() {
        let pred = Tensor::vector(vec![0.5, 0.3, 0.2]);
        let target = Tensor::vector(vec![0.4, 0.4, 0.2]);

        let loss = kl_div_loss(&pred, &target, Reduction::Sum).unwrap();
        assert!(loss.data()[0] > 0.0);
    }

    #[test]
    fn test_cosine_embedding_loss_similar() {
        let input1 = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let input2 = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let loss = cosine_embedding_loss(&input1, &input2, 1.0, 0.0, Reduction::Mean).unwrap();
        // Cosine sim = 1.0, loss = 1 - 1 = 0
        assert!(loss.data()[0].abs() < 1e-4);
    }

    #[test]
    fn test_cosine_embedding_loss_dissimilar() {
        let input1 = Tensor::vector(vec![1.0, 0.0, 0.0]);
        let input2 = Tensor::vector(vec![0.0, 1.0, 0.0]);

        let loss = cosine_embedding_loss(&input1, &input2, 1.0, 0.0, Reduction::Mean).unwrap();
        // Cosine sim = 0.0, loss = 1 - 0 = 1
        assert!((loss.data()[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_reduction_modes() {
        let pred = Tensor::vector(vec![2.0, 3.0, 4.0]);
        let target = Tensor::vector(vec![1.0, 2.0, 3.0]);

        // None: returns all losses
        let loss_none = mse_loss(&pred, &target, Reduction::None).unwrap();
        assert_eq!(loss_none.data().len(), 3);

        // Sum: sum of all losses
        let loss_sum = mse_loss(&pred, &target, Reduction::Sum).unwrap();
        assert_eq!(loss_sum.data()[0], 3.0); // 1 + 1 + 1

        // Mean: average of all losses
        let loss_mean = mse_loss(&pred, &target, Reduction::Mean).unwrap();
        assert_eq!(loss_mean.data()[0], 1.0); // 3 / 3
    }

    #[test]
    fn test_shape_mismatch() {
        let pred = Tensor::vector(vec![1.0, 2.0]);
        let target = Tensor::vector(vec![1.0, 2.0, 3.0]);

        assert!(mse_loss(&pred, &target, Reduction::Mean).is_err());
    }
}
