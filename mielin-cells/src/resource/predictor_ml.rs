//! ML-enhanced resource predictor using Holt's double exponential smoothing
//! and ridge-regularized least squares with R²-based confidence scoring.

use super::types::{ResourceHistory, ResourcePrediction, ResourceSnapshot};

// ─────────────────────────────────────────────────────────────────────────────
// HoltModel
// ─────────────────────────────────────────────────────────────────────────────

/// Holt double-exponential smoothing model for a single time series.
///
/// Uses the Holt linear trend method:
/// ```text
/// level[t] = α * y[t] + (1 − α) * (level[t−1] + trend[t−1])
/// trend[t] = β * (level[t] − level[t−1]) + (1 − β) * trend[t−1]
/// forecast[h] = level[t] + h * trend[t]
/// ```
#[derive(Debug, Clone)]
pub struct HoltModel {
    /// Level-smoothing coefficient  (0 < alpha < 1)
    pub alpha: f64,
    /// Trend-smoothing coefficient  (0 < beta < 1)
    pub beta: f64,
    /// Current level estimate
    pub level: f64,
    /// Current trend estimate
    pub trend: f64,
    /// Most recent observation
    pub last_value: f64,
    /// Number of observations ingested
    pub n: usize,
    /// Whether at least one update has been processed
    pub initialized: bool,
}

impl Default for HoltModel {
    fn default() -> Self {
        Self::new(0.3, 0.1)
    }
}

impl HoltModel {
    /// Create a new Holt model with given smoothing parameters.
    ///
    /// Values are clamped to (0, 1) so that the recursion is always stable.
    pub fn new(alpha: f64, beta: f64) -> Self {
        Self {
            alpha: alpha.clamp(1e-6, 1.0 - 1e-6),
            beta: beta.clamp(1e-6, 1.0 - 1e-6),
            level: 0.0,
            trend: 0.0,
            last_value: 0.0,
            n: 0,
            initialized: false,
        }
    }

    /// Ingest one new observation and update the level/trend estimates.
    pub fn update(&mut self, value: f64) {
        if !self.initialized {
            self.level = value;
            self.trend = 0.0;
            self.last_value = value;
            self.initialized = true;
        } else {
            let prev_level = self.level;
            self.level =
                self.alpha * value + (1.0 - self.alpha) * (prev_level + self.trend);
            self.trend =
                self.beta * (self.level - prev_level) + (1.0 - self.beta) * self.trend;
            self.last_value = value;
        }
        self.n += 1;
    }

    /// Forecast `h` steps ahead.
    ///
    /// For a continuous time series where `h` is expressed in the same units
    /// as the training step-size, this gives the predicted value.
    /// Returns `0.0` if the model has not yet been initialised.
    pub fn forecast(&self, h: f64) -> f64 {
        if !self.initialized {
            return 0.0;
        }
        (self.level + h * self.trend).max(0.0)
    }

    /// Train the model on a complete slice of observations (oldest first).
    pub fn train(&mut self, values: &[f64]) {
        for &v in values {
            self.update(v);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RidgeLinear
// ─────────────────────────────────────────────────────────────────────────────

/// Ridge-regularised simple linear regression (intercept + slope).
///
/// For paired vectors `x` (normalised time 0..1) and `y` (observed values),
/// fits the model `ŷ = w0 + w1 * x` using normal equations with L2 penalty:
///
/// ```text
/// w1 = (n·Σxy − Σx·Σy) / (n·Σx² − (Σx)² + λ·n)
/// w0 = (Σy − w1·Σx) / n
/// ```
///
/// R² is also computed in-place for use as a confidence signal.
#[derive(Debug, Clone)]
pub struct RidgeLinear {
    /// L2 regularisation strength λ (default 0.01)
    pub lambda: f64,
    /// Fitted intercept w₀
    pub w0: f64,
    /// Fitted slope w₁
    pub w1: f64,
    /// Coefficient of determination R² (0..1; negative means worse than mean)
    pub r_squared: f64,
    /// Number of training points used
    pub n: usize,
}

impl Default for RidgeLinear {
    fn default() -> Self {
        Self::new(0.01)
    }
}

impl RidgeLinear {
    /// Construct a ridge regression model with regularisation strength `lambda`.
    pub fn new(lambda: f64) -> Self {
        Self {
            lambda: lambda.max(0.0),
            w0: 0.0,
            w1: 0.0,
            r_squared: 0.0,
            n: 0,
        }
    }

    /// Fit the model on paired `(x, y)` slices.
    ///
    /// `x` should be normalised to [0, 1] to keep the ridge penalty scale-free.
    /// The shorter slice length is used.
    pub fn fit(&mut self, x: &[f64], y: &[f64]) {
        let n = x.len().min(y.len());
        if n < 2 {
            return;
        }
        self.n = n;
        let nf = n as f64;

        let sum_x: f64 = x[..n].iter().sum();
        let sum_y: f64 = y[..n].iter().sum();
        let sum_xx: f64 = x[..n].iter().map(|xi| xi * xi).sum();
        let sum_xy: f64 = x[..n]
            .iter()
            .zip(y[..n].iter())
            .map(|(xi, yi)| xi * yi)
            .sum();

        // Ridge-penalised denominator: add λ·n so the penalty is size-invariant
        let denom = nf * sum_xx - sum_x * sum_x + self.lambda * nf;
        if denom.abs() < 1e-15 {
            // Degenerate — fall back to predicting the mean
            self.w0 = sum_y / nf;
            self.w1 = 0.0;
            self.r_squared = 0.0;
            return;
        }

        self.w1 = (nf * sum_xy - sum_x * sum_y) / denom;
        self.w0 = (sum_y - self.w1 * sum_x) / nf;

        // Compute R²
        let mean_y = sum_y / nf;
        let ss_res: f64 = x[..n]
            .iter()
            .zip(y[..n].iter())
            .map(|(xi, yi)| {
                let pred = self.w0 + self.w1 * xi;
                (yi - pred).powi(2)
            })
            .sum();
        let ss_tot: f64 = y[..n].iter().map(|yi| (yi - mean_y).powi(2)).sum();
        self.r_squared = if ss_tot < 1e-15 {
            1.0 // flat series — perfect fit by the constant
        } else {
            1.0 - ss_res / ss_tot
        };
    }

    /// Predict at normalised time `t`.
    pub fn predict(&self, t: f64) -> f64 {
        (self.w0 + self.w1 * t).max(0.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PredictorWeights
// ─────────────────────────────────────────────────────────────────────────────

/// Tunable hyperparameters for `EnhancedPredictor`.
#[derive(Debug, Clone)]
pub struct PredictorWeights {
    /// Holt level-smoothing α (default 0.3)
    pub holt_alpha: f64,
    /// Holt trend-smoothing β (default 0.1)
    pub holt_beta: f64,
    /// Ridge regularisation λ (default 0.01)
    pub ridge_lambda: f64,
    /// Blending weight for Holt vs ridge prediction (0..1, default 0.6)
    pub holt_weight: f64,
}

impl Default for PredictorWeights {
    fn default() -> Self {
        Self {
            holt_alpha: 0.3,
            holt_beta: 0.1,
            ridge_lambda: 0.01,
            holt_weight: 0.6,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EnhancedPredictor
// ─────────────────────────────────────────────────────────────────────────────

/// ML-enhanced resource predictor combining Holt double exponential smoothing
/// with ridge-regularised linear regression.
///
/// Maintains per-metric Holt models (online) and ridge-linear models (batch,
/// re-fitted from the last window of history). Predictions are a
/// `holt_weight`-blended average of both, and the confidence score is derived
/// from the ridge model's R².
#[derive(Debug, Clone)]
pub struct EnhancedPredictor {
    /// Holt model for memory usage
    pub memory_model: HoltModel,
    /// Holt model for CPU cumulative time
    pub cpu_model: HoltModel,
    /// Holt model for network total bytes
    pub network_model: HoltModel,
    /// Holt model for storage usage
    pub storage_model: HoltModel,
    /// Ridge regression for memory
    pub memory_ridge: RidgeLinear,
    /// Ridge regression for CPU
    pub cpu_ridge: RidgeLinear,
    /// Ridge regression for network
    pub network_ridge: RidgeLinear,
    /// Ridge regression for storage
    pub storage_ridge: RidgeLinear,
    /// Hyperparameters
    pub weights: PredictorWeights,
}

impl Default for EnhancedPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl EnhancedPredictor {
    /// Create with default hyperparameters.
    pub fn new() -> Self {
        Self::with_weights(PredictorWeights::default())
    }

    /// Create with custom hyperparameters.
    pub fn with_weights(weights: PredictorWeights) -> Self {
        let holt_alpha = weights.holt_alpha;
        let holt_beta = weights.holt_beta;
        let ridge_lambda = weights.ridge_lambda;
        Self {
            memory_model: HoltModel::new(holt_alpha, holt_beta),
            cpu_model: HoltModel::new(holt_alpha, holt_beta),
            network_model: HoltModel::new(holt_alpha, holt_beta),
            storage_model: HoltModel::new(holt_alpha, holt_beta),
            memory_ridge: RidgeLinear::new(ridge_lambda),
            cpu_ridge: RidgeLinear::new(ridge_lambda),
            network_ridge: RidgeLinear::new(ridge_lambda),
            storage_ridge: RidgeLinear::new(ridge_lambda),
            weights,
        }
    }

    /// Incrementally update all Holt models from a single new snapshot.
    pub fn update_from_snapshot(&mut self, snapshot: &ResourceSnapshot) {
        self.memory_model.update(snapshot.memory_bytes as f64);
        self.storage_model.update(snapshot.storage_bytes as f64);
        let net_total = (snapshot.network_sent_bytes + snapshot.network_recv_bytes) as f64;
        self.network_model.update(net_total);
        // Track cumulative cpu_time_us as a monotonic level
        self.cpu_model.update(snapshot.cpu_time_us as f64);
    }

    /// Retrain all models from the full history.
    ///
    /// Holt models are reset and replayed; ridge models are re-fitted from
    /// scratch on the normalised time-axis.
    pub fn train_on_history(&mut self, history: &ResourceHistory) {
        let snapshots = history.snapshots();
        if snapshots.is_empty() {
            return;
        }
        let n = snapshots.len();

        // Reset Holt models
        self.memory_model = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);
        self.cpu_model = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);
        self.network_model = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);
        self.storage_model = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);

        let t_first = snapshots[0].timestamp_us as f64;
        let t_last = snapshots[n - 1].timestamp_us as f64;
        let t_range = (t_last - t_first).max(1.0);

        let mut mem_vals = Vec::with_capacity(n);
        let mut cpu_vals = Vec::with_capacity(n);
        let mut net_vals = Vec::with_capacity(n);
        let mut sto_vals = Vec::with_capacity(n);
        let mut time_norm = Vec::with_capacity(n);

        for s in snapshots {
            let t = (s.timestamp_us as f64 - t_first) / t_range;
            time_norm.push(t);
            mem_vals.push(s.memory_bytes as f64);
            cpu_vals.push(s.cpu_time_us as f64);
            net_vals.push((s.network_sent_bytes + s.network_recv_bytes) as f64);
            sto_vals.push(s.storage_bytes as f64);
        }

        self.memory_model.train(&mem_vals);
        self.cpu_model.train(&cpu_vals);
        self.network_model.train(&net_vals);
        self.storage_model.train(&sto_vals);

        self.memory_ridge = RidgeLinear::new(self.weights.ridge_lambda);
        self.cpu_ridge = RidgeLinear::new(self.weights.ridge_lambda);
        self.network_ridge = RidgeLinear::new(self.weights.ridge_lambda);
        self.storage_ridge = RidgeLinear::new(self.weights.ridge_lambda);

        self.memory_ridge.fit(&time_norm, &mem_vals);
        self.cpu_ridge.fit(&time_norm, &cpu_vals);
        self.network_ridge.fit(&time_norm, &net_vals);
        self.storage_ridge.fit(&time_norm, &sto_vals);
    }

    /// Produce a `ResourcePrediction` from a raw snapshot slice and a time horizon.
    ///
    /// Both Holt and ridge models are fitted here from `snapshots`; this is the
    /// stateless (pure function) path used by `ResourcePredictor::predict`.
    /// Callers that maintain long-running state should use `train_on_history` +
    /// `update_from_snapshot` instead.
    pub fn predict_from_history(
        &self,
        snapshots: &[ResourceSnapshot],
        horizon_us: u64,
    ) -> ResourcePrediction {
        let n = snapshots.len();
        if n == 0 {
            return ResourcePrediction {
                memory_bytes: 0,
                cpu_percent: 0.0,
                network_bytes_per_sec: 0,
                storage_bytes: 0,
                confidence: 0.0,
                horizon_us,
            };
        }

        // ── Normalised timeline ────────────────────────────────────────────
        let t_first = snapshots[0].timestamp_us as f64;
        let t_last = snapshots[n - 1].timestamp_us as f64;
        let t_range = (t_last - t_first).max(1.0);

        // Ridge target: normalised position of the forecast point
        let t_future_norm = (t_last + horizon_us as f64 - t_first) / t_range;
        // Holt step: fraction of the training window we project forward
        let holt_h = horizon_us as f64 / t_range;

        // ── Build training arrays ──────────────────────────────────────────
        let mut holt_mem = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);
        let mut holt_cpu = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);
        let mut holt_net = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);
        let mut holt_sto = HoltModel::new(self.weights.holt_alpha, self.weights.holt_beta);

        let mut ridge_mem = RidgeLinear::new(self.weights.ridge_lambda);
        let mut ridge_cpu = RidgeLinear::new(self.weights.ridge_lambda);
        let mut ridge_net = RidgeLinear::new(self.weights.ridge_lambda);
        let mut ridge_sto = RidgeLinear::new(self.weights.ridge_lambda);

        let mut mem_vals = Vec::with_capacity(n);
        let mut cpu_vals = Vec::with_capacity(n);
        let mut net_vals = Vec::with_capacity(n);
        let mut sto_vals = Vec::with_capacity(n);
        let mut time_norm = Vec::with_capacity(n);

        for s in snapshots {
            let t = (s.timestamp_us as f64 - t_first) / t_range;
            time_norm.push(t);
            mem_vals.push(s.memory_bytes as f64);
            cpu_vals.push(s.cpu_time_us as f64);
            net_vals.push((s.network_sent_bytes + s.network_recv_bytes) as f64);
            sto_vals.push(s.storage_bytes as f64);
        }

        holt_mem.train(&mem_vals);
        holt_cpu.train(&cpu_vals);
        holt_net.train(&net_vals);
        holt_sto.train(&sto_vals);

        ridge_mem.fit(&time_norm, &mem_vals);
        ridge_cpu.fit(&time_norm, &cpu_vals);
        ridge_net.fit(&time_norm, &net_vals);
        ridge_sto.fit(&time_norm, &sto_vals);

        // ── Blend Holt + Ridge forecasts ──────────────────────────────────
        let hw = self.weights.holt_weight;
        let rw = 1.0 - hw;

        let mem_pred =
            hw * holt_mem.forecast(holt_h) + rw * ridge_mem.predict(t_future_norm);
        let cpu_pred =
            hw * holt_cpu.forecast(holt_h) + rw * ridge_cpu.predict(t_future_norm);
        let net_pred =
            hw * holt_net.forecast(holt_h) + rw * ridge_net.predict(t_future_norm);
        let sto_pred =
            hw * holt_sto.forecast(holt_h) + rw * ridge_sto.predict(t_future_norm);

        // ── CPU %: convert Δcpu_us over horizon → utilisation percentage ──
        let last_cpu_us = snapshots[n - 1].cpu_time_us as f64;
        let cpu_delta_us = (cpu_pred - last_cpu_us).max(0.0);
        let cpu_percent = if horizon_us > 0 {
            (cpu_delta_us / horizon_us as f64) * 100.0
        } else {
            0.0
        }
        .clamp(0.0, 100.0);

        // ── Network rate: Δtotal_bytes over horizon → bytes/sec ──────────
        let last_net = (snapshots[n - 1].network_sent_bytes
            + snapshots[n - 1].network_recv_bytes) as f64;
        let net_delta = (net_pred - last_net).max(0.0);
        let network_bytes_per_sec = if horizon_us > 0 {
            net_delta / (horizon_us as f64 / 1_000_000.0)
        } else {
            0.0
        } as u64;

        // ── R²-based confidence ───────────────────────────────────────────
        let r2_vals = [
            ridge_mem.r_squared,
            ridge_cpu.r_squared,
            ridge_net.r_squared,
            ridge_sto.r_squared,
        ];
        let avg_r2: f64 = r2_vals.iter().sum::<f64>() / 4.0;
        // Negative R² (worse than mean) maps to a small non-zero floor so that
        // callers still receive a directional signal from the Holt component.
        let confidence = if avg_r2 < 0.0 { 0.1 } else { avg_r2.min(1.0) };
        // Dampen for very short histories
        let confidence = if n < 10 {
            confidence * (n as f64 / 10.0)
        } else {
            confidence
        };
        let confidence = confidence.clamp(0.05, 1.0);

        ResourcePrediction {
            memory_bytes: mem_pred.max(0.0) as u64,
            cpu_percent,
            network_bytes_per_sec,
            storage_bytes: sto_pred.max(0.0) as u64,
            confidence,
            horizon_us,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::types::{
        AnomalyConfig, HistoryConfig, ResourceHistory, ResourcePredictor, ResourceQuota,
        ResourceUsage,
    };

    // ── helpers ───────────────────────────────────────────────────────────

    fn make_history_with_tuples(
        snapshots: Vec<(u64, u64, u64, u64, u64, u64)>,
    ) -> ResourceHistory {
        // (timestamp_us, memory, cpu_time, net_sent, net_recv, storage)
        let config = HistoryConfig {
            max_snapshots: 1000,
            min_interval_us: 0,
        };
        let mut h = ResourceHistory::new([0u8; 16], config);
        for (ts, mem, cpu, ns, nr, sto) in snapshots {
            let usage = ResourceUsage {
                total_memory_bytes: mem,
                total_cpu_time_us: cpu,
                total_bytes_sent: ns,
                total_bytes_received: nr,
                persistent_storage_bytes: sto,
                ..Default::default()
            };
            h.record(&usage, ts);
        }
        h
    }

    fn make_trending_history(n: usize) -> ResourceHistory {
        let tuples: Vec<_> = (0..n)
            .map(|i| {
                let t = i as u64 * 1_000_000;
                let mem = 1_000_000u64 + i as u64 * 100_000;
                let cpu = i as u64 * 10_000;
                let net = i as u64 * 500;
                let sto = i as u64 * 2_000;
                (t, mem, cpu, net, net, sto)
            })
            .collect();
        make_history_with_tuples(tuples)
    }

    // ── HoltModel (4 tests) ───────────────────────────────────────────────

    #[test]
    fn test_holt_model_single_update() {
        let mut m = HoltModel::new(0.3, 0.1);
        m.update(42.0);
        let f = m.forecast(0.0);
        assert!(
            (f - 42.0).abs() < 1e-9,
            "forecast(0) = {f}, expected ≈42.0"
        );
    }

    #[test]
    fn test_holt_model_trending_up() {
        let mut m = HoltModel::new(0.3, 0.1);
        m.train(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        // The Holt level is dampened (≈3.4) but the trend is positive;
        // forecast(1.0) must exceed forecast(0.0) — the direction is correct.
        let f0 = m.forecast(0.0);
        let f1 = m.forecast(1.0);
        assert!(
            f1 > f0,
            "forecast(1.0) = {f1} should be > forecast(0.0) = {f0} (positive trend)"
        );
        // Also assert the trend is meaningful: forecast(2) > forecast(1)
        let f2 = m.forecast(2.0);
        assert!(f2 > f1, "forecast(2.0)={f2} should be > forecast(1.0)={f1}");
    }

    #[test]
    fn test_holt_model_flat_series() {
        let mut m = HoltModel::new(0.3, 0.1);
        m.train(&[5.0, 5.0, 5.0, 5.0, 5.0]);
        let f = m.forecast(1.0);
        assert!(
            (f - 5.0).abs() < 0.5,
            "forecast(1.0) = {f}, expected ≈5.0"
        );
    }

    #[test]
    fn test_holt_model_n_updates() {
        let mut m = HoltModel::new(0.3, 0.1);
        for i in 0..7 {
            m.update(i as f64);
        }
        assert_eq!(m.n, 7);
    }

    // ── RidgeLinear (3 tests) ─────────────────────────────────────────────

    #[test]
    fn test_ridge_fit_perfect_line() {
        // x ∈ [0, 1], y = 0,1,2,3,4  → extrapolate to t = 1.25 ≈ y = 5
        let x: Vec<f64> = (0..5).map(|i| i as f64 / 4.0).collect();
        let y: Vec<f64> = (0..5).map(|i| i as f64).collect();
        let mut r = RidgeLinear::new(1e-6);
        r.fit(&x, &y);
        let pred = r.predict(5.0 / 4.0);
        assert!(
            (pred - 5.0).abs() < 0.3,
            "predict ≈5.0, got {pred}"
        );
        assert!(
            r.r_squared > 0.99,
            "R² should be ≈1.0, got {}",
            r.r_squared
        );
    }

    #[test]
    fn test_ridge_fit_noisy_line() {
        let x: Vec<f64> = (0..20).map(|i| i as f64 / 19.0).collect();
        let y: Vec<f64> = (0..20)
            .map(|i| i as f64 + if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let mut r = RidgeLinear::new(0.01);
        r.fit(&x, &y);
        assert!(
            r.r_squared > 0.0,
            "R² should be positive, got {}",
            r.r_squared
        );
    }

    #[test]
    fn test_ridge_regularization_stabilizes() {
        // Very short and very noisy series
        let x: Vec<f64> = vec![0.0, 0.5, 1.0];
        let y: Vec<f64> = vec![1e9, 1.0, 1e9]; // wild swings
        let mut r = RidgeLinear::new(1.0); // strong regularisation
        r.fit(&x, &y);
        let pred = r.predict(10.0); // far extrapolation
        assert!(
            pred < 1e12,
            "prediction should stay bounded, got {pred}"
        );
    }

    // ── EnhancedPredictor (4 tests) ───────────────────────────────────────

    #[test]
    fn test_enhanced_predict_trend_beats_flat() {
        // Build a flat (constant) history and a trending one; the trending
        // prediction for a horizon of 5 s should be higher than the flat one.
        let trending = make_trending_history(20);
        let ep = EnhancedPredictor::new();

        let flat_snaps: Vec<_> = (0..20)
            .map(|i| (i as u64 * 1_000_000, 1_000_000u64, 0u64, 0u64, 0u64, 0u64))
            .collect();
        let flat = make_history_with_tuples(flat_snaps);

        let trending_pred = ep.predict_from_history(trending.snapshots(), 5_000_000);
        let flat_pred = ep.predict_from_history(flat.snapshots(), 5_000_000);

        assert!(
            trending_pred.memory_bytes > flat_pred.memory_bytes,
            "trending pred {} should exceed flat pred {}",
            trending_pred.memory_bytes,
            flat_pred.memory_bytes
        );
    }

    #[test]
    fn test_enhanced_confidence_better_fit_higher() {
        let clean = make_trending_history(30);
        let ep = EnhancedPredictor::new();
        let clean_pred = ep.predict_from_history(clean.snapshots(), 1_000_000);

        // Alternating noisy series
        let noisy_snaps: Vec<_> = (0..30)
            .map(|i| {
                let t = i as u64 * 1_000_000;
                let mem = if i % 2 == 0 { 1_000_000u64 } else { 9_000_000u64 };
                (t, mem, 0u64, 0u64, 0u64, 0u64)
            })
            .collect();
        let noisy = make_history_with_tuples(noisy_snaps);
        let noisy_pred = ep.predict_from_history(noisy.snapshots(), 1_000_000);

        assert!(
            clean_pred.confidence >= noisy_pred.confidence,
            "clean confidence {} should be >= noisy confidence {}",
            clean_pred.confidence,
            noisy_pred.confidence
        );
    }

    #[test]
    fn test_enhanced_train_on_history() {
        let h = make_trending_history(15);
        let mut ep = EnhancedPredictor::new();
        ep.train_on_history(&h);
        assert!(ep.memory_model.initialized);
        assert!(ep.memory_model.n > 0);
    }

    #[test]
    fn test_enhanced_update_from_snapshot() {
        let snap = ResourceSnapshot {
            timestamp_us: 1_000_000,
            memory_bytes: 2_000_000,
            cpu_time_us: 50_000,
            network_sent_bytes: 100,
            network_recv_bytes: 200,
            storage_bytes: 500_000,
        };
        let mut ep = EnhancedPredictor::new();
        ep.update_from_snapshot(&snap);
        assert_eq!(ep.memory_model.n, 1);
        assert_eq!(ep.storage_model.n, 1);
    }

    // ── Integration with ResourcePredictor (4 tests) ──────────────────────

    #[test]
    fn test_resource_predictor_trending_memory() {
        let h = make_trending_history(10);
        let last_mem = h.snapshots().last().unwrap().memory_bytes;
        let predictor = ResourcePredictor::new();
        let pred = predictor
            .predict(&h, 5_000_000)
            .expect("should produce a prediction with 10 snapshots");
        assert!(
            pred.memory_bytes >= last_mem / 2,
            "prediction {} should be reasonably close to last {}",
            pred.memory_bytes,
            last_mem
        );
    }

    #[test]
    fn test_resource_predictor_confidence_formula() {
        let h = make_trending_history(20);
        let predictor = ResourcePredictor::new();
        let pred = predictor.predict(&h, 1_000_000).expect("prediction");
        assert!(
            pred.confidence >= 0.0 && pred.confidence <= 1.0,
            "confidence out of range: {}",
            pred.confidence
        );
    }

    #[test]
    fn test_resource_predictor_fallback_short_history() {
        // Fewer than min_samples — must not panic (None is acceptable)
        let h = make_trending_history(3);
        let predictor = ResourcePredictor::new(); // min_samples = 5
        let _ = predictor.predict(&h, 1_000_000);
    }

    #[test]
    fn test_resource_predictor_quota_breach_prediction() {
        let h = make_trending_history(10);
        let quota = ResourceQuota::minimal();
        let predictor = ResourcePredictor::new();
        if let Some(breach_us) = predictor.predict_quota_breach(&h, &quota) {
            assert!(breach_us > 0, "breach time must be positive");
        }
    }

    // ── Extra: verify anomaly config compiles alongside our code ──────────

    #[test]
    fn test_anomaly_config_default_constructed() {
        let _cfg = AnomalyConfig::default();
    }
}
