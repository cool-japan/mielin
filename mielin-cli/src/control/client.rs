//! HTTP client for the control-plane API
//!
//! Wraps `reqwest` to speak with the `ControlServer` started by `mielinctl daemon`.

use super::dto::{HealthResponse, MeshStatusResponse, PeerInfo};
use anyhow::{Context, Result};

/// Typed HTTP client for the MielinOS control-plane REST API.
pub struct ControlClient {
    http: reqwest::Client,
    base_url: String,
}

impl ControlClient {
    /// Create a new client pointing at `base_url` (e.g. `"http://127.0.0.1:8081"`).
    pub fn new(base_url: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// `GET /api/v1/health`
    pub async fn health(&self) -> Result<HealthResponse> {
        self.http
            .get(format!("{}/api/v1/health", self.base_url))
            .send()
            .await
            .context("health request failed")?
            .error_for_status()
            .context("health endpoint returned error status")?
            .json()
            .await
            .context("failed to deserialize health response")
    }

    /// `GET /api/v1/mesh/status`
    pub async fn mesh_status(&self) -> Result<MeshStatusResponse> {
        self.http
            .get(format!("{}/api/v1/mesh/status", self.base_url))
            .send()
            .await
            .context("mesh/status request failed")?
            .error_for_status()
            .context("mesh/status endpoint returned error status")?
            .json()
            .await
            .context("failed to deserialize mesh status response")
    }

    /// `GET /api/v1/mesh/peers`
    pub async fn mesh_peers(&self) -> Result<Vec<PeerInfo>> {
        self.http
            .get(format!("{}/api/v1/mesh/peers", self.base_url))
            .send()
            .await
            .context("mesh/peers request failed")?
            .error_for_status()
            .context("mesh/peers endpoint returned error status")?
            .json()
            .await
            .context("failed to deserialize peers response")
    }
}
