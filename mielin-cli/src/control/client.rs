//! HTTP client for the control-plane API
//!
//! Uses `oxihttp-client` to speak with the `ControlServer` started by `mielinctl daemon`.

use super::dto::{HealthResponse, MeshStatusResponse, PeerInfo};
use anyhow::{Context, Result};
use oxihttp_client::{Client, HttpsClient};

/// Typed HTTP client for the MielinOS control-plane REST API.
pub struct ControlClient {
    http: HttpsClient,
    base_url: String,
}

impl ControlClient {
    /// Create a new client pointing at `base_url` (e.g. `"http://127.0.0.1:8081"`).
    pub fn new(base_url: &str) -> Self {
        let http = Client::builder()
            .with_webpki_roots()
            // Accept invalid certs for local daemon (self-signed)
            .danger_accept_invalid_certs(true)
            .build_https()
            .expect("oxihttp-client build should not fail");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// `GET /api/v1/health`
    pub async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .http
            .get(&format!("{}/api/v1/health", self.base_url))
            .context("health request build failed")?
            .send()
            .await
            .context("health request failed")?;
        resp.error_for_status()
            .context("health endpoint returned error status")?
            .body_json()
            .await
            .context("failed to deserialize health response")
    }

    /// `GET /api/v1/mesh/status`
    pub async fn mesh_status(&self) -> Result<MeshStatusResponse> {
        let resp = self
            .http
            .get(&format!("{}/api/v1/mesh/status", self.base_url))
            .context("mesh/status request build failed")?
            .send()
            .await
            .context("mesh/status request failed")?;
        resp.error_for_status()
            .context("mesh/status endpoint returned error status")?
            .body_json()
            .await
            .context("failed to deserialize mesh status response")
    }

    /// `GET /api/v1/mesh/peers`
    pub async fn mesh_peers(&self) -> Result<Vec<PeerInfo>> {
        let resp = self
            .http
            .get(&format!("{}/api/v1/mesh/peers", self.base_url))
            .context("mesh/peers request build failed")?
            .send()
            .await
            .context("mesh/peers request failed")?;
        resp.error_for_status()
            .context("mesh/peers endpoint returned error status")?
            .body_json()
            .await
            .context("failed to deserialize peers response")
    }
}
