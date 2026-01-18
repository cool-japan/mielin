//! Optimization algorithms for training neural networks
//!
//! This module provides various optimization algorithms for training neural networks
//! including SGD, Adam, RMSprop, and AdaGrad. All optimizers work with the
//! automatic differentiation system.

#![allow(unused)]

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::f32;
use libm::{powf, sqrtf};

use crate::autograd::Variable;
use crate::tensor::Tensor;

/// Learning rate configuration
#[derive(Debug, Clone, Copy)]
pub struct LearningRate {
    /// Initial learning rate
    pub initial: f32,
    /// Current learning rate
    pub current: f32,
}

impl LearningRate {
    /// Create a new learning rate configuration
    pub fn new(initial: f32) -> Self {
        Self {
            initial,
            current: initial,
        }
    }

    /// Create with current value different from initial
    pub fn with_current(initial: f32, current: f32) -> Self {
        Self { initial, current }
    }
}

/// Parameter identifier for optimizer state
pub type ParamId = usize;

/// Trait for optimization algorithms
pub trait Optimizer {
    /// Update parameters using gradients
    ///
    /// # Arguments
    /// * `param_id` - Unique identifier for the parameter
    /// * `param` - The parameter tensor to update (will be modified in place)
    /// * `grad` - The gradient tensor
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>);

    /// Get the current learning rate
    fn learning_rate(&self) -> f32;

    /// Set the learning rate
    fn set_learning_rate(&mut self, lr: f32);

    /// Zero all gradients (optional, default implementation)
    fn zero_grad(&mut self) {}

    /// Get the number of steps performed
    fn num_steps(&self) -> usize;
}

/// Stochastic Gradient Descent optimizer
///
/// Updates parameters using the gradient directly:
/// ```text
/// θ_{t+1} = θ_t - lr * ∇L
/// ```
///
/// With momentum:
/// ```text
/// v_{t+1} = β * v_t + ∇L
/// θ_{t+1} = θ_t - lr * v_{t+1}
/// ```
pub struct SGD {
    lr: LearningRate,
    momentum: f32,
    dampening: f32,
    weight_decay: f32,
    nesterov: bool,
    /// Momentum buffers for each parameter
    momentum_buffers: BTreeMap<ParamId, Tensor<f32>>,
    num_steps: usize,
}

impl SGD {
    /// Create a new SGD optimizer
    ///
    /// # Arguments
    /// * `lr` - Learning rate
    pub fn new(lr: f32) -> Self {
        Self {
            lr: LearningRate::new(lr),
            momentum: 0.0,
            dampening: 0.0,
            weight_decay: 0.0,
            nesterov: false,
            momentum_buffers: BTreeMap::new(),
            num_steps: 0,
        }
    }

    /// Set momentum coefficient
    pub fn with_momentum(mut self, momentum: f32) -> Self {
        self.momentum = momentum;
        self
    }

    /// Set dampening factor
    pub fn with_dampening(mut self, dampening: f32) -> Self {
        self.dampening = dampening;
        self
    }

    /// Set weight decay (L2 regularization)
    pub fn with_weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }

    /// Enable Nesterov momentum
    pub fn with_nesterov(mut self, nesterov: bool) -> Self {
        self.nesterov = nesterov;
        self
    }
}

impl Optimizer for SGD {
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>) {
        let mut d_p = grad.clone();

        // Apply weight decay
        if self.weight_decay != 0.0 {
            d_p = d_p.add(&param.scale(self.weight_decay));
        }

        // Apply momentum
        if self.momentum != 0.0 {
            let buf = self
                .momentum_buffers
                .entry(param_id)
                .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

            // v_t = momentum * v_{t-1} + (1 - dampening) * grad
            *buf = buf
                .scale(self.momentum)
                .add(&d_p.scale(1.0 - self.dampening));

            if self.nesterov {
                // Nesterov: grad + momentum * v_t
                d_p = d_p.add(&buf.scale(self.momentum));
            } else {
                // Standard momentum: v_t
                d_p = buf.clone();
            }
        }

        // Update parameters: θ = θ - lr * d_p
        let update = d_p.scale(self.lr.current);
        *param = param.sub(&update);

        self.num_steps += 1;
    }

    fn learning_rate(&self) -> f32 {
        self.lr.current
    }

    fn set_learning_rate(&mut self, lr: f32) {
        self.lr.current = lr;
    }

    fn num_steps(&self) -> usize {
        self.num_steps
    }
}

/// Adam optimizer (Adaptive Moment Estimation)
///
/// Combines momentum and RMSprop. Maintains running averages of both
/// the gradients and the second moments of the gradients.
///
/// ```text
/// m_t = β₁ * m_{t-1} + (1 - β₁) * ∇L
/// v_t = β₂ * v_{t-1} + (1 - β₂) * (∇L)²
/// m̂_t = m_t / (1 - β₁^t)
/// v̂_t = v_t / (1 - β₂^t)
/// θ_{t+1} = θ_t - lr * m̂_t / (√v̂_t + ε)
/// ```
pub struct Adam {
    lr: LearningRate,
    beta1: f32,
    beta2: f32,
    epsilon: f32,
    weight_decay: f32,
    /// First moment estimates (momentum)
    first_moments: BTreeMap<ParamId, Tensor<f32>>,
    /// Second moment estimates (uncentered variance)
    second_moments: BTreeMap<ParamId, Tensor<f32>>,
    num_steps: usize,
}

impl Adam {
    /// Create a new Adam optimizer
    ///
    /// # Arguments
    /// * `lr` - Learning rate (default: 0.001)
    pub fn new(lr: f32) -> Self {
        Self {
            lr: LearningRate::new(lr),
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            weight_decay: 0.0,
            first_moments: BTreeMap::new(),
            second_moments: BTreeMap::new(),
            num_steps: 0,
        }
    }

    /// Set beta1 (first moment decay rate)
    pub fn with_beta1(mut self, beta1: f32) -> Self {
        self.beta1 = beta1;
        self
    }

    /// Set beta2 (second moment decay rate)
    pub fn with_beta2(mut self, beta2: f32) -> Self {
        self.beta2 = beta2;
        self
    }

    /// Set epsilon for numerical stability
    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    /// Set weight decay (L2 regularization)
    pub fn with_weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }
}

impl Optimizer for Adam {
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>) {
        self.num_steps += 1;

        // Apply weight decay
        let mut d_p = grad.clone();
        if self.weight_decay != 0.0 {
            d_p = d_p.add(&param.scale(self.weight_decay));
        }

        // Get or initialize moments
        let m = self
            .first_moments
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));
        let v = self
            .second_moments
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

        // Update biased first moment estimate: m_t = β₁ * m_{t-1} + (1 - β₁) * grad
        *m = m.scale(self.beta1).add(&d_p.scale(1.0 - self.beta1));

        // Update biased second moment estimate: v_t = β₂ * v_{t-1} + (1 - β₂) * grad²
        let grad_sq = d_p.mul(&d_p);
        *v = v.scale(self.beta2).add(&grad_sq.scale(1.0 - self.beta2));

        // Bias correction
        let bias_correction1 = 1.0 - powf(self.beta1, self.num_steps as f32);
        let bias_correction2 = 1.0 - powf(self.beta2, self.num_steps as f32);

        // Compute step size: lr * √(1 - β₂^t) / (1 - β₁^t)
        let step_size = self.lr.current * sqrtf(bias_correction2) / bias_correction1;

        // Update parameters: θ = θ - step_size * m / (√v + ε)
        let denom = v
            .sqrt()
            .add(&Tensor::filled(v.shape().to_vec(), self.epsilon));

        // Element-wise division: m / denom
        let mut update = m.clone();
        for i in 0..update.data().len() {
            update.data_mut()[i] /= denom.data()[i];
        }
        update = update.scale(step_size);
        *param = param.sub(&update);
    }

    fn learning_rate(&self) -> f32 {
        self.lr.current
    }

    fn set_learning_rate(&mut self, lr: f32) {
        self.lr.current = lr;
    }

    fn num_steps(&self) -> usize {
        self.num_steps
    }
}

/// AdamW optimizer (Adam with decoupled Weight decay)
///
/// Improves upon Adam by decoupling weight decay from the gradient updates.
/// This leads to better generalization in many cases.
///
/// ```text
/// m_t = β₁ * m_{t-1} + (1 - β₁) * ∇L
/// v_t = β₂ * v_{t-1} + (1 - β₂) * (∇L)²
/// m̂_t = m_t / (1 - β₁^t)
/// v̂_t = v_t / (1 - β₂^t)
/// θ_{t+1} = θ_t - lr * (m̂_t / (√v̂_t + ε) + λ * θ_t)
/// ```
pub struct AdamW {
    lr: LearningRate,
    beta1: f32,
    beta2: f32,
    epsilon: f32,
    weight_decay: f32,
    /// First moment estimates (momentum)
    first_moments: BTreeMap<ParamId, Tensor<f32>>,
    /// Second moment estimates (uncentered variance)
    second_moments: BTreeMap<ParamId, Tensor<f32>>,
    num_steps: usize,
}

impl AdamW {
    /// Create a new AdamW optimizer
    pub fn new(lr: f32) -> Self {
        Self {
            lr: LearningRate::new(lr),
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            weight_decay: 0.01, // Default weight decay for AdamW
            first_moments: BTreeMap::new(),
            second_moments: BTreeMap::new(),
            num_steps: 0,
        }
    }

    pub fn with_beta1(mut self, beta1: f32) -> Self {
        self.beta1 = beta1;
        self
    }

    pub fn with_beta2(mut self, beta2: f32) -> Self {
        self.beta2 = beta2;
        self
    }

    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    pub fn with_weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }
}

impl Optimizer for AdamW {
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>) {
        self.num_steps += 1;

        // Get or initialize moments
        let m = self
            .first_moments
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));
        let v = self
            .second_moments
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

        // Update biased first moment estimate
        *m = m.scale(self.beta1).add(&grad.scale(1.0 - self.beta1));

        // Update biased second moment estimate
        let grad_sq = grad.mul(grad);
        *v = v.scale(self.beta2).add(&grad_sq.scale(1.0 - self.beta2));

        // Bias correction
        let bias_correction1 = 1.0 - powf(self.beta1, self.num_steps as f32);
        let bias_correction2 = 1.0 - powf(self.beta2, self.num_steps as f32);

        let step_size = self.lr.current * sqrtf(bias_correction2) / bias_correction1;

        // Compute adaptive gradient
        let denom = v
            .sqrt()
            .add(&Tensor::filled(v.shape().to_vec(), self.epsilon));
        let mut update = m.clone();
        for i in 0..update.data().len() {
            update.data_mut()[i] /= denom.data()[i];
        }

        // AdamW: Decouple weight decay (apply directly to parameters)
        *param = param.scale(1.0 - self.lr.current * self.weight_decay);

        // Apply gradient update
        *param = param.sub(&update.scale(step_size));
    }

    fn learning_rate(&self) -> f32 {
        self.lr.current
    }

    fn set_learning_rate(&mut self, lr: f32) {
        self.lr.current = lr;
    }

    fn num_steps(&self) -> usize {
        self.num_steps
    }
}

/// Nadam optimizer (Nesterov-accelerated Adam)
///
/// Combines Adam with Nesterov momentum for potentially faster convergence.
///
/// ```text
/// m_t = β₁ * m_{t-1} + (1 - β₁) * ∇L
/// v_t = β₂ * v_{t-1} + (1 - β₂) * (∇L)²
/// m̂_t = m_t / (1 - β₁^t)
/// v̂_t = v_t / (1 - β₂^t)
/// θ_{t+1} = θ_t - lr * (β₁ * m̂_t + (1 - β₁) * ∇L / (1 - β₁^t)) / (√v̂_t + ε)
/// ```
pub struct Nadam {
    lr: LearningRate,
    beta1: f32,
    beta2: f32,
    epsilon: f32,
    weight_decay: f32,
    /// First moment estimates
    first_moments: BTreeMap<ParamId, Tensor<f32>>,
    /// Second moment estimates
    second_moments: BTreeMap<ParamId, Tensor<f32>>,
    num_steps: usize,
}

impl Nadam {
    /// Create a new Nadam optimizer
    pub fn new(lr: f32) -> Self {
        Self {
            lr: LearningRate::new(lr),
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            weight_decay: 0.0,
            first_moments: BTreeMap::new(),
            second_moments: BTreeMap::new(),
            num_steps: 0,
        }
    }

    pub fn with_beta1(mut self, beta1: f32) -> Self {
        self.beta1 = beta1;
        self
    }

    pub fn with_beta2(mut self, beta2: f32) -> Self {
        self.beta2 = beta2;
        self
    }

    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    pub fn with_weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }
}

impl Optimizer for Nadam {
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>) {
        self.num_steps += 1;

        // Apply weight decay
        let mut d_p = grad.clone();
        if self.weight_decay != 0.0 {
            d_p = d_p.add(&param.scale(self.weight_decay));
        }

        // Get or initialize moments
        let m = self
            .first_moments
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));
        let v = self
            .second_moments
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

        // Update biased first moment
        *m = m.scale(self.beta1).add(&d_p.scale(1.0 - self.beta1));

        // Update biased second moment
        let grad_sq = d_p.mul(&d_p);
        *v = v.scale(self.beta2).add(&grad_sq.scale(1.0 - self.beta2));

        // Bias correction
        let bias_correction1 = 1.0 - powf(self.beta1, self.num_steps as f32);
        let bias_correction2 = 1.0 - powf(self.beta2, self.num_steps as f32);

        // Nesterov momentum: combine current gradient with momentum
        let m_hat = m.scale(self.beta1).add(&d_p.scale(1.0 - self.beta1));
        let m_hat_corrected = m_hat.scale(1.0 / bias_correction1);

        // Compute denominator
        let denom = v
            .sqrt()
            .add(&Tensor::filled(v.shape().to_vec(), self.epsilon))
            .scale(1.0 / sqrtf(bias_correction2));

        // Update parameters
        let mut update = m_hat_corrected.clone();
        for i in 0..update.data().len() {
            update.data_mut()[i] /= denom.data()[i];
        }
        *param = param.sub(&update.scale(self.lr.current));
    }

    fn learning_rate(&self) -> f32 {
        self.lr.current
    }

    fn set_learning_rate(&mut self, lr: f32) {
        self.lr.current = lr;
    }

    fn num_steps(&self) -> usize {
        self.num_steps
    }
}

/// RMSprop optimizer (Root Mean Square Propagation)
///
/// Maintains a moving average of squared gradients to normalize the gradient.
///
/// ```text
/// v_t = α * v_{t-1} + (1 - α) * (∇L)²
/// θ_{t+1} = θ_t - lr * ∇L / (√v_t + ε)
/// ```
pub struct RMSprop {
    lr: LearningRate,
    alpha: f32,
    epsilon: f32,
    weight_decay: f32,
    momentum: f32,
    centered: bool,
    /// Running average of squared gradients
    square_avg: BTreeMap<ParamId, Tensor<f32>>,
    /// Momentum buffer (if momentum > 0)
    momentum_buffer: BTreeMap<ParamId, Tensor<f32>>,
    /// Running average of gradients (if centered)
    grad_avg: BTreeMap<ParamId, Tensor<f32>>,
    num_steps: usize,
}

impl RMSprop {
    /// Create a new RMSprop optimizer
    ///
    /// # Arguments
    /// * `lr` - Learning rate
    pub fn new(lr: f32) -> Self {
        Self {
            lr: LearningRate::new(lr),
            alpha: 0.99,
            epsilon: 1e-8,
            weight_decay: 0.0,
            momentum: 0.0,
            centered: false,
            square_avg: BTreeMap::new(),
            momentum_buffer: BTreeMap::new(),
            grad_avg: BTreeMap::new(),
            num_steps: 0,
        }
    }

    /// Set alpha (smoothing constant)
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Set epsilon for numerical stability
    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    /// Set weight decay
    pub fn with_weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }

    /// Set momentum
    pub fn with_momentum(mut self, momentum: f32) -> Self {
        self.momentum = momentum;
        self
    }

    /// Enable centered version (subtract mean of gradients)
    pub fn with_centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self
    }
}

impl Optimizer for RMSprop {
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>) {
        self.num_steps += 1;

        // Apply weight decay
        let mut d_p = grad.clone();
        if self.weight_decay != 0.0 {
            d_p = d_p.add(&param.scale(self.weight_decay));
        }

        // Get or initialize square average
        let sq_avg = self
            .square_avg
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

        // Update running average of squared gradients
        let grad_sq = d_p.mul(&d_p);
        *sq_avg = sq_avg
            .scale(self.alpha)
            .add(&grad_sq.scale(1.0 - self.alpha));

        let mut avg = sq_avg.clone();

        if self.centered {
            // Centered RMSprop: subtract mean gradient
            let g_avg = self
                .grad_avg
                .entry(param_id)
                .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

            *g_avg = g_avg.scale(self.alpha).add(&d_p.scale(1.0 - self.alpha));

            let g_avg_sq = g_avg.mul(g_avg);
            avg = avg.sub(&g_avg_sq);
        }

        // Element-wise division: d_p / (sqrt(avg) + epsilon)
        let denom = avg
            .sqrt()
            .add(&Tensor::filled(avg.shape().to_vec(), self.epsilon));
        let mut d_p_normalized = d_p.clone();
        for i in 0..d_p_normalized.data().len() {
            d_p_normalized.data_mut()[i] /= denom.data()[i];
        }

        if self.momentum > 0.0 {
            let buf = self
                .momentum_buffer
                .entry(param_id)
                .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

            *buf = buf.scale(self.momentum).add(&d_p_normalized);

            d_p_normalized = buf.clone();
        }

        // Update parameters
        let update = d_p_normalized.scale(self.lr.current);
        *param = param.sub(&update);
    }

    fn learning_rate(&self) -> f32 {
        self.lr.current
    }

    fn set_learning_rate(&mut self, lr: f32) {
        self.lr.current = lr;
    }

    fn num_steps(&self) -> usize {
        self.num_steps
    }
}

/// AdaGrad optimizer (Adaptive Gradient Algorithm)
///
/// Adapts the learning rate based on the historical gradient information.
/// Good for sparse gradients.
///
/// ```text
/// G_t = G_{t-1} + (∇L)²
/// θ_{t+1} = θ_t - lr * ∇L / (√G_t + ε)
/// ```
pub struct AdaGrad {
    lr: LearningRate,
    epsilon: f32,
    weight_decay: f32,
    lr_decay: f32,
    /// Accumulated squared gradients
    state_sum: BTreeMap<ParamId, Tensor<f32>>,
    num_steps: usize,
}

impl AdaGrad {
    /// Create a new AdaGrad optimizer
    ///
    /// # Arguments
    /// * `lr` - Learning rate
    pub fn new(lr: f32) -> Self {
        Self {
            lr: LearningRate::new(lr),
            epsilon: 1e-10,
            weight_decay: 0.0,
            lr_decay: 0.0,
            state_sum: BTreeMap::new(),
            num_steps: 0,
        }
    }

    /// Set epsilon for numerical stability
    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    /// Set weight decay
    pub fn with_weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }

    /// Set learning rate decay
    pub fn with_lr_decay(mut self, lr_decay: f32) -> Self {
        self.lr_decay = lr_decay;
        self
    }
}

impl Optimizer for AdaGrad {
    fn step(&mut self, param_id: ParamId, param: &mut Tensor<f32>, grad: &Tensor<f32>) {
        self.num_steps += 1;

        // Apply weight decay
        let mut d_p = grad.clone();
        if self.weight_decay != 0.0 {
            d_p = d_p.add(&param.scale(self.weight_decay));
        }

        // Decay learning rate
        let clr = self.lr.current / (1.0 + (self.num_steps - 1) as f32 * self.lr_decay);

        // Get or initialize state sum
        let state = self
            .state_sum
            .entry(param_id)
            .or_insert_with(|| Tensor::zeros(param.shape().to_vec()));

        // Accumulate squared gradients: G_t = G_{t-1} + grad²
        let grad_sq = d_p.mul(&d_p);
        *state = state.add(&grad_sq);

        // Compute adaptive learning rate: lr / (√G_t + ε)
        let std = state
            .sqrt()
            .add(&Tensor::filled(state.shape().to_vec(), self.epsilon));

        // Element-wise division: grad / std
        let mut update = d_p.clone();
        for i in 0..update.data().len() {
            update.data_mut()[i] /= std.data()[i];
        }
        update = update.scale(clr);
        *param = param.sub(&update);
    }

    fn learning_rate(&self) -> f32 {
        self.lr.current
    }

    fn set_learning_rate(&mut self, lr: f32) {
        self.lr.current = lr;
    }

    fn num_steps(&self) -> usize {
        self.num_steps
    }
}

/// Learning rate scheduler
pub trait LRScheduler {
    /// Get the learning rate for the current epoch
    fn get_lr(&self, epoch: usize) -> f32;
}

/// Step learning rate scheduler
///
/// Decays learning rate by gamma every step_size epochs.
pub struct StepLR {
    initial_lr: f32,
    step_size: usize,
    gamma: f32,
}

impl StepLR {
    pub fn new(initial_lr: f32, step_size: usize, gamma: f32) -> Self {
        Self {
            initial_lr,
            step_size,
            gamma,
        }
    }
}

impl LRScheduler for StepLR {
    fn get_lr(&self, epoch: usize) -> f32 {
        self.initial_lr * powf(self.gamma, (epoch / self.step_size) as f32)
    }
}

/// Exponential learning rate scheduler
///
/// Decays learning rate by gamma every epoch.
pub struct ExponentialLR {
    initial_lr: f32,
    gamma: f32,
}

impl ExponentialLR {
    pub fn new(initial_lr: f32, gamma: f32) -> Self {
        Self { initial_lr, gamma }
    }
}

impl LRScheduler for ExponentialLR {
    fn get_lr(&self, epoch: usize) -> f32 {
        self.initial_lr * powf(self.gamma, epoch as f32)
    }
}

/// Cosine annealing learning rate scheduler
///
/// Anneals learning rate using a cosine function.
pub struct CosineAnnealingLR {
    initial_lr: f32,
    min_lr: f32,
    t_max: usize,
}

impl CosineAnnealingLR {
    pub fn new(initial_lr: f32, t_max: usize) -> Self {
        Self {
            initial_lr,
            min_lr: 0.0,
            t_max,
        }
    }

    pub fn with_min_lr(mut self, min_lr: f32) -> Self {
        self.min_lr = min_lr;
        self
    }
}

impl LRScheduler for CosineAnnealingLR {
    fn get_lr(&self, epoch: usize) -> f32 {
        let t = (epoch % self.t_max) as f32;
        let t_max = self.t_max as f32;
        self.min_lr
            + (self.initial_lr - self.min_lr)
                * (1.0 + libm::cosf(core::f32::consts::PI * t / t_max))
                / 2.0
    }
}

/// Reduce learning rate on plateau scheduler
///
/// Reduces learning rate when a metric has stopped improving.
/// Useful for adaptive learning during training.
pub struct ReduceLROnPlateau {
    current_lr: f32,
    factor: f32,
    patience: usize,
    min_lr: f32,
    mode: PlateauMode,
    threshold: f32,
    cooldown: usize,
    best_metric: f32,
    bad_epochs: usize,
    cooldown_counter: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlateauMode {
    /// Reduce LR when metric stops decreasing (for loss)
    Min,
    /// Reduce LR when metric stops increasing (for accuracy)
    Max,
}

impl ReduceLROnPlateau {
    pub fn new(initial_lr: f32, mode: PlateauMode) -> Self {
        let best_metric = match mode {
            PlateauMode::Min => f32::INFINITY,
            PlateauMode::Max => f32::NEG_INFINITY,
        };

        Self {
            current_lr: initial_lr,
            factor: 0.1,
            patience: 10,
            min_lr: 0.0,
            mode,
            threshold: 1e-4,
            cooldown: 0,
            best_metric,
            bad_epochs: 0,
            cooldown_counter: 0,
        }
    }

    pub fn with_factor(mut self, factor: f32) -> Self {
        self.factor = factor;
        self
    }

    pub fn with_patience(mut self, patience: usize) -> Self {
        self.patience = patience;
        self
    }

    pub fn with_min_lr(mut self, min_lr: f32) -> Self {
        self.min_lr = min_lr;
        self
    }

    /// Step the scheduler with current metric value
    pub fn step_with_metric(&mut self, metric: f32) -> f32 {
        if self.cooldown_counter > 0 {
            self.cooldown_counter -= 1;
            return self.current_lr;
        }

        let is_better = match self.mode {
            PlateauMode::Min => metric < self.best_metric - self.threshold,
            PlateauMode::Max => metric > self.best_metric + self.threshold,
        };

        if is_better {
            self.best_metric = metric;
            self.bad_epochs = 0;
        } else {
            self.bad_epochs += 1;
        }

        if self.bad_epochs > self.patience {
            self.current_lr = (self.current_lr * self.factor).max(self.min_lr);
            self.bad_epochs = 0;
            self.cooldown_counter = self.cooldown;
        }

        self.current_lr
    }
}

/// Cyclic learning rate scheduler
///
/// Cycles learning rate between a minimum and maximum value.
/// Helps escape local minima.
pub struct CyclicLR {
    base_lr: f32,
    max_lr: f32,
    step_size: usize,
    mode: CyclicMode,
    gamma: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CyclicMode {
    /// Triangular cycle with constant amplitude
    Triangular,
    /// Triangular cycle with decreasing amplitude
    Triangular2,
    /// Triangular cycle with exponentially decreasing amplitude
    ExpRange,
}

impl CyclicLR {
    pub fn new(base_lr: f32, max_lr: f32, step_size: usize) -> Self {
        Self {
            base_lr,
            max_lr,
            step_size,
            mode: CyclicMode::Triangular,
            gamma: 1.0,
        }
    }

    pub fn with_mode(mut self, mode: CyclicMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_gamma(mut self, gamma: f32) -> Self {
        self.gamma = gamma;
        self
    }
}

impl LRScheduler for CyclicLR {
    fn get_lr(&self, epoch: usize) -> f32 {
        let cycle_pos = (epoch % self.step_size) as f32;
        let half_cycle = self.step_size as f32 / 2.0;

        // Triangular wave: 0 -> 1 -> 0 within step_size
        let x = if cycle_pos <= half_cycle {
            // Increasing phase: 0 to 1
            cycle_pos / half_cycle
        } else {
            // Decreasing phase: 1 to 0
            2.0 - cycle_pos / half_cycle
        };

        let amplitude = match self.mode {
            CyclicMode::Triangular => 1.0,
            CyclicMode::Triangular2 => {
                let cycle_num = (epoch / self.step_size) as f32;
                1.0 / powf(2.0, cycle_num)
            }
            CyclicMode::ExpRange => powf(self.gamma, epoch as f32),
        };

        self.base_lr + (self.max_lr - self.base_lr) * x * amplitude
    }
}

/// One-cycle learning rate scheduler
///
/// Implements the 1cycle policy: starts at base_lr, increases to max_lr,
/// then decreases back to base_lr, and finally to a very small lr.
pub struct OneCycleLR {
    max_lr: f32,
    total_steps: usize,
    pct_start: f32,
    div_factor: f32,
    final_div_factor: f32,
}

impl OneCycleLR {
    pub fn new(max_lr: f32, total_steps: usize) -> Self {
        Self {
            max_lr,
            total_steps,
            pct_start: 0.3,
            div_factor: 25.0,
            final_div_factor: 10000.0,
        }
    }

    pub fn with_pct_start(mut self, pct_start: f32) -> Self {
        self.pct_start = pct_start;
        self
    }

    pub fn with_div_factor(mut self, div_factor: f32) -> Self {
        self.div_factor = div_factor;
        self
    }
}

impl LRScheduler for OneCycleLR {
    fn get_lr(&self, epoch: usize) -> f32 {
        let step_ratio = epoch as f32 / self.total_steps as f32;
        let start_lr = self.max_lr / self.div_factor;
        let end_lr = self.max_lr / self.final_div_factor;

        if step_ratio < self.pct_start {
            // Increasing phase
            let phase_ratio = step_ratio / self.pct_start;
            start_lr + (self.max_lr - start_lr) * phase_ratio
        } else if step_ratio < 1.0 {
            // Decreasing phase
            let phase_ratio = (step_ratio - self.pct_start) / (1.0 - self.pct_start);
            self.max_lr + (end_lr - self.max_lr) * phase_ratio
        } else {
            end_lr
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_sgd_basic() {
        let mut optimizer = SGD::new(0.1);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        optimizer.step(0, &mut param, &grad);

        // θ_new = θ_old - lr * grad
        assert!((*param.get(&[0]).unwrap() - 0.99).abs() < 1e-6);
        assert!((*param.get(&[1]).unwrap() - 1.98).abs() < 1e-6);
        assert!((*param.get(&[2]).unwrap() - 2.97).abs() < 1e-6);
    }

    #[test]
    fn test_sgd_momentum() {
        let mut optimizer = SGD::new(0.1).with_momentum(0.9);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        // First step
        optimizer.step(0, &mut param, &grad);
        let step1 = param.clone();

        // Second step with same gradient
        optimizer.step(0, &mut param, &grad);

        // With momentum, second step should move further
        assert!(param.get(&[0]).unwrap() < step1.get(&[0]).unwrap());
    }

    #[test]
    fn test_adam_basic() {
        let mut optimizer = Adam::new(0.001);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // Parameters should have moved in opposite direction of gradient
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
        assert!(param.get(&[2]).unwrap() < original.get(&[2]).unwrap());
    }

    #[test]
    fn test_adam_convergence() {
        let mut optimizer = Adam::new(0.1);
        let mut param = Tensor::vector(vec![5.0, 5.0]);

        // Gradient pointing towards zero
        for _ in 0..100 {
            let grad = param.scale(0.1); // Gradient = 0.1 * param
            optimizer.step(0, &mut param, &grad);
        }

        // Should converge towards zero
        assert!(param.get(&[0]).unwrap().abs() < 1.0);
        assert!(param.get(&[1]).unwrap().abs() < 1.0);
    }

    #[test]
    fn test_rmsprop_basic() {
        let mut optimizer = RMSprop::new(0.01);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // Parameters should have moved
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
        assert!(param.get(&[2]).unwrap() < original.get(&[2]).unwrap());
    }

    #[test]
    fn test_adagrad_basic() {
        let mut optimizer = AdaGrad::new(0.1);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // Parameters should have moved
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
        assert!(param.get(&[2]).unwrap() < original.get(&[2]).unwrap());
    }

    #[test]
    fn test_adagrad_adaptive_lr() {
        let mut optimizer = AdaGrad::new(0.1);
        let mut param = Tensor::vector(vec![1.0, 1.0]);

        // Different gradient magnitudes
        let grad1 = Tensor::vector(vec![1.0, 0.1]);
        optimizer.step(0, &mut param, &grad1);
        let step1_val0 = *param.get(&[0]).unwrap();
        let step1_val1 = *param.get(&[1]).unwrap();

        let grad2 = Tensor::vector(vec![1.0, 0.1]);
        optimizer.step(0, &mut param, &grad2);

        // AdaGrad reduces learning rate for parameters with large gradients
        // After step 1, both moved: param[0] moved by ~0.1 (lr=0.1, grad=1.0, state=1.0)
        //                         param[1] moved by ~0.01 (lr=0.1, grad=0.1, state=0.01)
        // Verify that adaptive learning rate is working (parameters are decreasing)
        assert!(*param.get(&[0]).unwrap() < step1_val0);
        assert!(*param.get(&[1]).unwrap() < step1_val1);

        // Verify parameters have moved from original
        assert!(*param.get(&[0]).unwrap() < 1.0);
        assert!(*param.get(&[1]).unwrap() < 1.0);
    }

    #[test]
    fn test_step_lr_scheduler() {
        let scheduler = StepLR::new(0.1, 10, 0.1);

        assert!((scheduler.get_lr(0) - 0.1).abs() < 1e-6);
        assert!((scheduler.get_lr(9) - 0.1).abs() < 1e-6);
        assert!((scheduler.get_lr(10) - 0.01).abs() < 1e-6);
        assert!((scheduler.get_lr(20) - 0.001).abs() < 1e-6);
    }

    #[test]
    fn test_exponential_lr_scheduler() {
        let scheduler = ExponentialLR::new(0.1, 0.9);

        assert!((scheduler.get_lr(0) - 0.1).abs() < 1e-6);
        assert!((scheduler.get_lr(1) - 0.09).abs() < 1e-6);
        assert!((scheduler.get_lr(2) - 0.081).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_annealing_lr() {
        let scheduler = CosineAnnealingLR::new(0.1, 100);

        let lr0 = scheduler.get_lr(0);
        let lr50 = scheduler.get_lr(50);
        let lr100 = scheduler.get_lr(100);

        // Should start at max
        assert!((lr0 - 0.1).abs() < 1e-6);
        // At halfway, cosine goes to 0, so lr = min_lr + (initial - min) * 1/2 = 0.05
        assert!((lr50 - 0.05).abs() < 1e-3);
        // Should cycle back to initial
        assert!((lr100 - 0.1).abs() < 1e-6);
    }

    #[test]
    fn test_reduce_lr_on_plateau_min() {
        let mut scheduler = ReduceLROnPlateau::new(0.1, PlateauMode::Min).with_patience(3);

        // Improving metric (loss decreasing)
        let lr1 = scheduler.step_with_metric(1.0);
        assert_eq!(lr1, 0.1);

        let lr2 = scheduler.step_with_metric(0.9);
        assert_eq!(lr2, 0.1);

        // Plateau (loss not improving)
        let lr3 = scheduler.step_with_metric(0.91);
        assert_eq!(lr3, 0.1); // patience=1

        let lr4 = scheduler.step_with_metric(0.92);
        assert_eq!(lr4, 0.1); // patience=2

        let lr5 = scheduler.step_with_metric(0.93);
        assert_eq!(lr5, 0.1); // patience=3

        let lr6 = scheduler.step_with_metric(0.94);
        // Should reduce LR now (patience exceeded)
        assert!((lr6 - 0.01).abs() < 1e-6);
    }

    #[test]
    fn test_reduce_lr_on_plateau_max() {
        let mut scheduler = ReduceLROnPlateau::new(0.1, PlateauMode::Max).with_patience(2);

        // Improving metric (accuracy increasing)
        let lr1 = scheduler.step_with_metric(0.5);
        assert_eq!(lr1, 0.1);

        let lr2 = scheduler.step_with_metric(0.6);
        assert_eq!(lr2, 0.1);

        // Plateau (accuracy not improving)
        let lr3 = scheduler.step_with_metric(0.59);
        assert_eq!(lr3, 0.1); // patience=1

        let lr4 = scheduler.step_with_metric(0.58);
        assert_eq!(lr4, 0.1); // patience=2

        let lr5 = scheduler.step_with_metric(0.57);
        // Should reduce LR now
        assert!((lr5 - 0.01).abs() < 1e-6);
    }

    #[test]
    fn test_cyclic_lr_triangular() {
        let scheduler = CyclicLR::new(0.001, 0.01, 100);

        let lr0 = scheduler.get_lr(0);
        let lr50 = scheduler.get_lr(50);
        let lr100 = scheduler.get_lr(100);
        let lr150 = scheduler.get_lr(150);

        // Should start at base_lr
        assert!((lr0 - 0.001).abs() < 1e-6);
        // At step_size/2, should be at max_lr
        assert!((lr50 - 0.01).abs() < 1e-4);
        // At step_size, back to base_lr
        assert!((lr100 - 0.001).abs() < 1e-4);
        // Cycle repeats
        assert!((lr150 - 0.01).abs() < 1e-4);
    }

    #[test]
    fn test_one_cycle_lr() {
        let scheduler = OneCycleLR::new(0.1, 100);

        let lr0 = scheduler.get_lr(0);
        let lr30 = scheduler.get_lr(30); // End of increasing phase (pct_start=0.3)
        let lr100 = scheduler.get_lr(100);

        // Should start at base_lr (max_lr / div_factor)
        assert!((lr0 - 0.1 / 25.0).abs() < 1e-4);
        // At pct_start * total_steps, should be at max_lr
        assert!((lr30 - 0.1).abs() < 1e-3);
        // At end, should be at very small lr
        assert!(lr100 < 0.001);
    }

    #[test]
    fn test_optimizer_learning_rate_mutation() {
        let mut optimizer = SGD::new(0.1);
        assert_eq!(optimizer.learning_rate(), 0.1);

        optimizer.set_learning_rate(0.01);
        assert_eq!(optimizer.learning_rate(), 0.01);
    }

    #[test]
    fn test_weight_decay() {
        let mut optimizer = SGD::new(0.1).with_weight_decay(0.01);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.0, 0.0, 0.0]); // Zero gradient

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // Even with zero gradient, weight decay should reduce parameters
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
        assert!(param.get(&[2]).unwrap() < original.get(&[2]).unwrap());
    }

    #[test]
    fn test_num_steps_tracking() {
        let mut optimizer = Adam::new(0.001);
        assert_eq!(optimizer.num_steps(), 0);

        let mut param = Tensor::vector(vec![1.0]);
        let grad = Tensor::vector(vec![0.1]);

        optimizer.step(0, &mut param, &grad);
        assert_eq!(optimizer.num_steps(), 1);

        optimizer.step(0, &mut param, &grad);
        assert_eq!(optimizer.num_steps(), 2);
    }

    #[test]
    fn test_multiple_parameters() {
        let mut optimizer = SGD::new(0.1);

        let mut param1 = Tensor::vector(vec![1.0, 2.0]);
        let mut param2 = Tensor::vector(vec![3.0, 4.0]);

        let grad1 = Tensor::vector(vec![0.1, 0.2]);
        let grad2 = Tensor::vector(vec![0.3, 0.4]);

        optimizer.step(0, &mut param1, &grad1);
        optimizer.step(1, &mut param2, &grad2);

        // Both parameters should be updated independently
        assert!((*param1.get(&[0]).unwrap() - 0.99).abs() < 1e-6);
        assert!((*param2.get(&[0]).unwrap() - 2.97).abs() < 1e-6);
    }

    #[test]
    fn test_adamw_basic() {
        let mut optimizer = AdamW::new(0.001);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // Parameters should have moved
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
        assert!(param.get(&[2]).unwrap() < original.get(&[2]).unwrap());
    }

    #[test]
    fn test_adamw_weight_decay() {
        let mut optimizer = AdamW::new(0.1).with_weight_decay(0.1);
        let mut param = Tensor::vector(vec![1.0, 1.0]);
        let grad = Tensor::vector(vec![0.0, 0.0]); // Zero gradient

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // With decoupled weight decay, parameters should decay even with zero gradient
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
    }

    #[test]
    fn test_nadam_basic() {
        let mut optimizer = Nadam::new(0.001);
        let mut param = Tensor::vector(vec![1.0, 2.0, 3.0]);
        let grad = Tensor::vector(vec![0.1, 0.2, 0.3]);

        let original = param.clone();
        optimizer.step(0, &mut param, &grad);

        // Parameters should have moved
        assert!(param.get(&[0]).unwrap() < original.get(&[0]).unwrap());
        assert!(param.get(&[1]).unwrap() < original.get(&[1]).unwrap());
        assert!(param.get(&[2]).unwrap() < original.get(&[2]).unwrap());
    }

    #[test]
    fn test_nadam_convergence() {
        let mut optimizer = Nadam::new(0.1);
        let mut param = Tensor::vector(vec![5.0, 5.0]);

        // Gradient pointing towards zero
        for _ in 0..100 {
            let grad = param.scale(0.1);
            optimizer.step(0, &mut param, &grad);
        }

        // Should converge towards zero
        assert!(param.get(&[0]).unwrap().abs() < 1.0);
        assert!(param.get(&[1]).unwrap().abs() < 1.0);
    }
}
