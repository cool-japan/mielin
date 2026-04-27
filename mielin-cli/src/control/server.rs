//! Axum HTTP control-plane server
//!
//! Exposes the mesh service state over a REST API consumed by the CLI client
//! and any other tooling that speaks HTTP.

use super::dto::{AgentInfo, HealthResponse, MeshStatusResponse, MigrationStats, PeerInfo};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use mielin_mesh_core::service::MeshService;
use std::{net::SocketAddr, sync::Arc};
use tracing::info;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Shared handle to the running mesh service, accessible from every handler.
#[derive(Clone)]
pub struct AppState {
    mesh: Arc<MeshService>,
}

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------

/// Internal error wrapper that converts to an HTTP 500 response.
struct ServiceError(anyhow::Error);

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let body = format!("Internal service error: {}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ServiceError {
    fn from(e: E) -> Self {
        ServiceError(e.into())
    }
}

// ---------------------------------------------------------------------------
// ControlServer
// ---------------------------------------------------------------------------

/// Axum-based HTTP server exposing the mesh service over a REST API.
pub struct ControlServer {
    state: AppState,
}

impl ControlServer {
    /// Create a new control server wrapping the given mesh service.
    pub fn new(mesh: Arc<MeshService>) -> Self {
        Self {
            state: AppState { mesh },
        }
    }

    /// Build and return the router (useful for testing without binding).
    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/v1/health", get(health_handler))
            .route("/api/v1/mesh/status", get(mesh_status_handler))
            .route("/api/v1/mesh/peers", get(mesh_peers_handler))
            .route("/api/v1/mesh/nodes", get(mesh_nodes_handler))
            .route("/api/v1/agents", get(agents_handler))
            .route("/api/v1/migrate/status", get(migrate_status_handler))
            .with_state(self.state.clone())
    }

    /// Bind to `addr` and serve requests until the process is shut down.
    pub async fn serve(self, addr: SocketAddr) -> anyhow::Result<()> {
        info!("Control plane listening on http://{}", addr);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, self.router()).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/v1/health`
///
/// Returns a simple liveness probe together with the running node ID.
async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let node_id = state.mesh.node_id().to_string();
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        node_id,
    })
}

/// `GET /api/v1/mesh/status`
///
/// Returns gossip membership counts, DHT peer count, and local agent count.
/// Failing sub-calls fall back to 0 rather than surfacing an error, because
/// these are purely informational metrics.
async fn mesh_status_handler(State(state): State<AppState>) -> Json<MeshStatusResponse> {
    let (alive, suspect, dead) = state.mesh.get_member_stats().await.unwrap_or((0, 0, 0));

    let dht_peers = state.mesh.dht_peer_count().await;

    let local_agents = state.mesh.local_agent_count().await.unwrap_or(0);

    Json(MeshStatusResponse {
        alive,
        suspect,
        dead,
        dht_peers,
        local_agents,
    })
}

/// Convert a `MemberInfo` from the gossip layer into the wire DTO.
fn member_to_peer_info(m: mielin_mesh_core::gossip::MemberInfo) -> PeerInfo {
    let status_str = match m.status {
        mielin_mesh_core::gossip::HealthStatus::Alive => "alive",
        mielin_mesh_core::gossip::HealthStatus::Suspect => "suspect",
        mielin_mesh_core::gossip::HealthStatus::Dead => "dead",
    };

    let last_seen_secs = m
        .last_seen
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    PeerInfo {
        node_id: m.node_id.to_string(),
        status: status_str.to_string(),
        last_seen_secs,
    }
}

/// `GET /api/v1/mesh/peers`
///
/// Returns all alive gossip members as `PeerInfo` DTOs.
async fn mesh_peers_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<PeerInfo>>, ServiceError> {
    let members = state.mesh.get_alive_members().await?;
    let peers: Vec<PeerInfo> = members.into_iter().map(member_to_peer_info).collect();
    Ok(Json(peers))
}

/// `GET /api/v1/mesh/nodes`
///
/// Alias for `/api/v1/mesh/peers` — returns alive mesh members.
async fn mesh_nodes_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<PeerInfo>>, ServiceError> {
    let members = state.mesh.get_alive_members().await?;
    let peers: Vec<PeerInfo> = members.into_iter().map(member_to_peer_info).collect();
    Ok(Json(peers))
}

/// `GET /api/v1/agents`
///
/// Returns a lightweight list containing a count placeholder for each local
/// agent.  Full registry integration (per-agent metadata) is deferred to a
/// future phase.
async fn agents_handler(State(state): State<AppState>) -> Json<Vec<AgentInfo>> {
    let count = state.mesh.local_agent_count().await.unwrap_or(0);
    let node_id = state.mesh.node_id().to_string();

    // Emit one synthetic entry per slot so the count is visible in the
    // response without requiring full per-agent metadata.
    let agents: Vec<AgentInfo> = (0..count)
        .map(|i| AgentInfo {
            agent_id: format!("agent-{:04}", i),
            node_id: node_id.clone(),
            address: String::new(),
        })
        .collect();

    Json(agents)
}

/// `GET /api/v1/migrate/status`
///
/// Returns aggregate migration statistics from the coordinator.
async fn migrate_status_handler(
    State(state): State<AppState>,
) -> Result<Json<MigrationStats>, ServiceError> {
    let raw = state.mesh.get_migration_stats().await?;

    let dto = MigrationStats {
        total_migrations: raw.total_migrations as u64,
        active_migrations: raw.active_migrations,
        successful: raw.successful_migrations as u64,
        failed: raw.failed_migrations as u64,
    };

    Ok(Json(dto))
}
