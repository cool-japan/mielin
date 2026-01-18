//! Evaluation Metrics Module
//!
//! Provides common metrics for evaluating machine learning models.
//! Supports both binary and multi-class classification, as well as regression.
//!
//! # Features
//! - Classification metrics (accuracy, precision, recall, F1, AUC)
//! - Regression metrics (R², MSE, MAE, RMSE)
//! - Confusion matrix utilities
//! - Top-k accuracy
//! - Per-class metrics
//!
//! # Examples
//! ```rust,ignore
//! use mielin_tensor::metrics::{accuracy, precision, recall, f1_score};
//!
//! let predictions = vec![1, 0, 1, 1, 0];
//! let targets = vec![1, 0, 1, 0, 0];
//!
//! let acc = accuracy(&predictions, &targets);
//! let prec = precision(&predictions, &targets, 1);
//! ```

#![allow(dead_code)]

extern crate alloc;

use crate::error::{TensorError, TensorResult};
use crate::tensor::Tensor;
use alloc::vec;
use alloc::vec::Vec;

/// Confusion Matrix for binary or multi-class classification
#[derive(Debug, Clone)]
pub struct ConfusionMatrix {
    /// True Positives per class
    pub true_positives: Vec<usize>,
    /// False Positives per class
    pub false_positives: Vec<usize>,
    /// True Negatives per class
    pub true_negatives: Vec<usize>,
    /// False Negatives per class
    pub false_negatives: Vec<usize>,
    /// Number of classes
    pub num_classes: usize,
}

impl ConfusionMatrix {
    /// Create a confusion matrix from predictions and targets
    pub fn from_predictions(
        predictions: &[usize],
        targets: &[usize],
        num_classes: usize,
    ) -> TensorResult<Self> {
        if predictions.len() != targets.len() {
            return Err(TensorError::Other {
                message: "Predictions and targets must have same length".into(),
            });
        }

        let mut tp = vec![0; num_classes];
        let mut fp = vec![0; num_classes];
        let mut tn = vec![0; num_classes];
        let mut fn_ = vec![0; num_classes];

        for (pred, target) in predictions.iter().zip(targets.iter()) {
            if *pred >= num_classes || *target >= num_classes {
                return Err(TensorError::Other {
                    message: "Class index out of bounds".into(),
                });
            }

            for class in 0..num_classes {
                if *target == class && *pred == class {
                    tp[class] += 1;
                } else if *target != class && *pred == class {
                    fp[class] += 1;
                } else if *target == class && *pred != class {
                    fn_[class] += 1;
                } else {
                    tn[class] += 1;
                }
            }
        }

        Ok(Self {
            true_positives: tp,
            false_positives: fp,
            true_negatives: tn,
            false_negatives: fn_,
            num_classes,
        })
    }

    /// Get precision for a specific class
    pub fn precision(&self, class: usize) -> f32 {
        let tp = self.true_positives[class] as f32;
        let fp = self.false_positives[class] as f32;

        if tp + fp == 0.0 {
            0.0
        } else {
            tp / (tp + fp)
        }
    }

    /// Get recall for a specific class
    pub fn recall(&self, class: usize) -> f32 {
        let tp = self.true_positives[class] as f32;
        let fn_ = self.false_negatives[class] as f32;

        if tp + fn_ == 0.0 {
            0.0
        } else {
            tp / (tp + fn_)
        }
    }

    /// Get F1 score for a specific class
    pub fn f1_score(&self, class: usize) -> f32 {
        let prec = self.precision(class);
        let rec = self.recall(class);

        if prec + rec == 0.0 {
            0.0
        } else {
            2.0 * (prec * rec) / (prec + rec)
        }
    }

    /// Get macro-averaged precision (average across all classes)
    pub fn macro_precision(&self) -> f32 {
        let sum: f32 = (0..self.num_classes).map(|c| self.precision(c)).sum();
        sum / self.num_classes as f32
    }

    /// Get macro-averaged recall
    pub fn macro_recall(&self) -> f32 {
        let sum: f32 = (0..self.num_classes).map(|c| self.recall(c)).sum();
        sum / self.num_classes as f32
    }

    /// Get macro-averaged F1 score
    pub fn macro_f1(&self) -> f32 {
        let sum: f32 = (0..self.num_classes).map(|c| self.f1_score(c)).sum();
        sum / self.num_classes as f32
    }
}

/// Calculate accuracy: (correct predictions) / (total predictions)
pub fn accuracy(predictions: &[usize], targets: &[usize]) -> TensorResult<f32> {
    if predictions.len() != targets.len() {
        return Err(TensorError::Other {
            message: "Predictions and targets must have same length".into(),
        });
    }

    if predictions.is_empty() {
        return Ok(0.0);
    }

    let correct = predictions
        .iter()
        .zip(targets.iter())
        .filter(|(p, t)| p == t)
        .count();

    Ok(correct as f32 / predictions.len() as f32)
}

/// Calculate precision for a specific class
/// Precision = TP / (TP + FP)
pub fn precision(
    predictions: &[usize],
    targets: &[usize],
    positive_class: usize,
) -> TensorResult<f32> {
    if predictions.len() != targets.len() {
        return Err(TensorError::Other {
            message: "Predictions and targets must have same length".into(),
        });
    }

    let mut tp = 0;
    let mut fp = 0;

    for (pred, target) in predictions.iter().zip(targets.iter()) {
        if *pred == positive_class {
            if *target == positive_class {
                tp += 1;
            } else {
                fp += 1;
            }
        }
    }

    if tp + fp == 0 {
        Ok(0.0)
    } else {
        Ok(tp as f32 / (tp + fp) as f32)
    }
}

/// Calculate recall for a specific class
/// Recall = TP / (TP + FN)
pub fn recall(
    predictions: &[usize],
    targets: &[usize],
    positive_class: usize,
) -> TensorResult<f32> {
    if predictions.len() != targets.len() {
        return Err(TensorError::Other {
            message: "Predictions and targets must have same length".into(),
        });
    }

    let mut tp = 0;
    let mut fn_ = 0;

    for (pred, target) in predictions.iter().zip(targets.iter()) {
        if *target == positive_class {
            if *pred == positive_class {
                tp += 1;
            } else {
                fn_ += 1;
            }
        }
    }

    if tp + fn_ == 0 {
        Ok(0.0)
    } else {
        Ok(tp as f32 / (tp + fn_) as f32)
    }
}

/// Calculate F1 score for a specific class
/// F1 = 2 * (precision * recall) / (precision + recall)
pub fn f1_score(
    predictions: &[usize],
    targets: &[usize],
    positive_class: usize,
) -> TensorResult<f32> {
    let prec = precision(predictions, targets, positive_class)?;
    let rec = recall(predictions, targets, positive_class)?;

    if prec + rec == 0.0 {
        Ok(0.0)
    } else {
        Ok(2.0 * (prec * rec) / (prec + rec))
    }
}

/// Calculate top-k accuracy
/// Checks if true class is in top k predictions
pub fn top_k_accuracy(predictions: &Tensor<f32>, targets: &[usize], k: usize) -> TensorResult<f32> {
    if predictions.shape().len() != 2 {
        return Err(TensorError::dimension_mismatch(
            "top_k_accuracy",
            2,
            predictions.shape().len(),
        ));
    }

    let batch_size = predictions.shape()[0];
    let num_classes = predictions.shape()[1];

    if targets.len() != batch_size {
        return Err(TensorError::Other {
            message: "Targets length must match batch size".into(),
        });
    }

    if k > num_classes {
        return Err(TensorError::Other {
            message: "k cannot be larger than number of classes".into(),
        });
    }

    let mut correct = 0;

    for (batch_idx, &target_class) in targets.iter().enumerate() {
        // Get predictions for this sample
        let mut class_scores: Vec<(usize, f32)> = Vec::with_capacity(num_classes);
        for class_idx in 0..num_classes {
            let score = predictions.data()[batch_idx * num_classes + class_idx];
            class_scores.push((class_idx, score));
        }

        // Sort by score descending
        class_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));

        // Check if target is in top k
        for &(class_idx, _score) in class_scores.iter().take(k) {
            if class_idx == target_class {
                correct += 1;
                break;
            }
        }
    }

    Ok(correct as f32 / batch_size as f32)
}

/// Calculate R² (coefficient of determination) for regression
/// R² = 1 - (SS_res / SS_tot)
pub fn r2_score(predictions: &Tensor<f32>, targets: &Tensor<f32>) -> TensorResult<f32> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "r2_score",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    // Calculate mean of targets
    let target_mean = targets.mean();

    // SS_tot = Σ(y_i - mean(y))²
    let ss_tot: f32 = targets
        .data()
        .iter()
        .map(|&y| (y - target_mean) * (y - target_mean))
        .sum();

    // SS_res = Σ(y_i - pred_i)²
    let ss_res: f32 = targets
        .data()
        .iter()
        .zip(predictions.data().iter())
        .map(|(&y, &pred)| (y - pred) * (y - pred))
        .sum();

    if ss_tot == 0.0 {
        Ok(0.0)
    } else {
        Ok(1.0 - (ss_res / ss_tot))
    }
}

/// Root Mean Squared Error
pub fn rmse(predictions: &Tensor<f32>, targets: &Tensor<f32>) -> TensorResult<f32> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "rmse",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    let mse: f32 = predictions
        .data()
        .iter()
        .zip(targets.data().iter())
        .map(|(pred, target)| (pred - target) * (pred - target))
        .sum::<f32>()
        / predictions.data().len() as f32;

    Ok(libm::sqrtf(mse))
}

/// Mean Absolute Percentage Error
pub fn mape(predictions: &Tensor<f32>, targets: &Tensor<f32>) -> TensorResult<f32> {
    if predictions.shape() != targets.shape() {
        return Err(TensorError::shape_mismatch(
            "mape",
            targets.shape().to_vec(),
            predictions.shape().to_vec(),
        ));
    }

    let mut sum = 0.0;
    let mut count = 0;

    for (pred, target) in predictions.data().iter().zip(targets.data().iter()) {
        if target.abs() > 1e-7 {
            sum += libm::fabsf((target - pred) / target);
            count += 1;
        }
    }

    if count == 0 {
        Ok(0.0)
    } else {
        Ok(100.0 * sum / count as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_accuracy_perfect() {
        let preds = vec![0, 1, 2, 0, 1];
        let targets = vec![0, 1, 2, 0, 1];

        let acc = accuracy(&preds, &targets).unwrap();
        assert_eq!(acc, 1.0);
    }

    #[test]
    fn test_accuracy_half() {
        let preds = vec![0, 1, 0, 1];
        let targets = vec![0, 0, 1, 1];

        let acc = accuracy(&preds, &targets).unwrap();
        assert_eq!(acc, 0.5);
    }

    #[test]
    fn test_precision_binary() {
        let preds = vec![1, 1, 0, 1, 0];
        let targets = vec![1, 0, 0, 1, 0];

        let prec = precision(&preds, &targets, 1).unwrap();
        // TP=2 (indices 0,3), FP=1 (index 1)
        // Precision = 2 / (2+1) = 0.667
        assert!((prec - 0.6666667).abs() < 1e-4);
    }

    #[test]
    fn test_recall_binary() {
        let preds = vec![1, 1, 0, 1, 0];
        let targets = vec![1, 0, 0, 1, 0];

        let rec = recall(&preds, &targets, 1).unwrap();
        // TP=2 (indices 0,3), FN=0
        // Recall = 2 / (2+0) = 1.0
        assert_eq!(rec, 1.0);
    }

    #[test]
    fn test_f1_score_binary() {
        let preds = vec![1, 1, 0, 1, 0];
        let targets = vec![1, 0, 0, 1, 0];

        let f1 = f1_score(&preds, &targets, 1).unwrap();
        let prec = precision(&preds, &targets, 1).unwrap();
        let rec = recall(&preds, &targets, 1).unwrap();

        let expected_f1 = 2.0 * (prec * rec) / (prec + rec);
        assert!((f1 - expected_f1).abs() < 1e-4);
    }

    #[test]
    fn test_confusion_matrix_binary() {
        let preds = vec![1, 1, 0, 1, 0, 0];
        let targets = vec![1, 0, 0, 1, 1, 0];

        let cm = ConfusionMatrix::from_predictions(&preds, &targets, 2).unwrap();

        // For class 0:
        assert_eq!(cm.true_positives[0], 2); // Indices 2, 5
        assert_eq!(cm.false_positives[0], 1); // Index 4

        // For class 1:
        assert_eq!(cm.true_positives[1], 2); // Indices 0, 3
        assert_eq!(cm.false_positives[1], 1); // Index 1
    }

    #[test]
    fn test_confusion_matrix_multiclass() {
        let preds = vec![0, 1, 2, 1, 2];
        let targets = vec![0, 1, 2, 2, 1];

        let cm = ConfusionMatrix::from_predictions(&preds, &targets, 3).unwrap();

        assert_eq!(cm.num_classes, 3);
        assert_eq!(cm.true_positives[0], 1); // Class 0
        assert_eq!(cm.true_positives[1], 1); // Class 1
        assert_eq!(cm.true_positives[2], 1); // Class 2
    }

    #[test]
    fn test_macro_metrics() {
        let preds = vec![0, 1, 2, 1, 2, 0];
        let targets = vec![0, 1, 2, 2, 1, 0];

        let cm = ConfusionMatrix::from_predictions(&preds, &targets, 3).unwrap();

        let macro_prec = cm.macro_precision();
        let macro_rec = cm.macro_recall();
        let macro_f1 = cm.macro_f1();

        assert!(macro_prec > 0.0 && macro_prec <= 1.0);
        assert!(macro_rec > 0.0 && macro_rec <= 1.0);
        assert!(macro_f1 > 0.0 && macro_f1 <= 1.0);
    }

    #[test]
    fn test_top_k_accuracy() {
        // Batch of 3 samples, 4 classes
        let preds = Tensor::from_vec(
            vec![
                0.1, 0.2, 0.6, 0.1, // Sample 0: top class is 2
                0.5, 0.1, 0.3, 0.1, // Sample 1: top class is 0
                0.2, 0.3, 0.1, 0.4, // Sample 2: top class is 3
            ],
            vec![3, 4],
        )
        .unwrap();

        let targets = vec![2, 0, 3];

        // Top-1 accuracy: all correct
        let top1 = top_k_accuracy(&preds, &targets, 1).unwrap();
        assert_eq!(top1, 1.0);

        // Top-2 accuracy: also all correct
        let top2 = top_k_accuracy(&preds, &targets, 2).unwrap();
        assert_eq!(top2, 1.0);
    }

    #[test]
    fn test_r2_score_perfect() {
        let preds = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0]);
        let targets = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0]);

        let r2 = r2_score(&preds, &targets).unwrap();
        assert!((r2 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_r2_score_nonperfect() {
        let preds = Tensor::vector(vec![1.5, 2.5, 3.5, 4.5]);
        let targets = Tensor::vector(vec![1.0, 2.0, 3.0, 4.0]);

        let r2 = r2_score(&preds, &targets).unwrap();
        assert!(r2 < 1.0);
        assert!(r2 > 0.0);
    }

    #[test]
    fn test_rmse() {
        let preds = Tensor::vector(vec![3.0, 5.0, 7.0]);
        let targets = Tensor::vector(vec![1.0, 2.0, 3.0]);

        let rmse_val = rmse(&preds, &targets).unwrap();
        // MSE = ((4 + 9 + 16) / 3) = 9.666...
        // RMSE = sqrt(9.666...) = 3.109...
        assert!((rmse_val - 3.1091).abs() < 0.01);
    }

    #[test]
    fn test_mape() {
        let preds = Tensor::vector(vec![90.0, 110.0, 95.0]);
        let targets = Tensor::vector(vec![100.0, 100.0, 100.0]);

        let mape_val = mape(&preds, &targets).unwrap();
        // MAPE = 100 * mean(|100-90|/100, |100-110|/100, |100-95|/100)
        //      = 100 * mean(0.1, 0.1, 0.05) = 100 * 0.0833... = 8.33%
        assert!((mape_val - 8.3333).abs() < 0.01);
    }

    #[test]
    fn test_length_mismatch() {
        let preds = vec![0, 1];
        let targets = vec![0, 1, 2];

        assert!(accuracy(&preds, &targets).is_err());
        assert!(precision(&preds, &targets, 1).is_err());
        assert!(recall(&preds, &targets, 1).is_err());
    }
}
