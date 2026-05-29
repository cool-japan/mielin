//! Auto-generated module structure

pub mod anomalyconfig_traits;
pub mod cpuquota_traits;
pub mod functions;
pub mod historyconfig_traits;
pub mod memoryquota_traits;
pub mod networkquota_traits;
pub mod predictor_ml;
pub mod resourcemanager_traits;
pub mod resourcepredictor_traits;
pub mod resourcequota_traits;
pub mod storagequota_traits;
pub mod types;

// Re-export all types
pub use predictor_ml::{EnhancedPredictor, HoltModel, PredictorWeights, RidgeLinear};
pub use types::*;
