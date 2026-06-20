//! QUIC transport implementation using oxiquic-transport
//!
//! Provides low-latency, multiplexed transport for MielinMesh communication.
//! The crypto provider is oxiquic_crypto (pure-Rust, no ring/aws-lc).

use crate::certs::{CertManager, Certificate};
use crate::{Message, WireError};
use oxiquic_transport::{
    ClientEndpoint, DrivenConnection, QuicConnection as OxiQuicConnection, ServerEndpoint,
    TransportConfig,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

/// Maximum message size (16 MiB).
/// Inbound reads are capped with `.take(MAX_MESSAGE_SIZE as u64)` because
/// `AsyncReadExt::read_to_end` is uncapped unlike quinn's `read_to_end(max)`.
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Connection timeout duration
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Build the shared TransportConfig once.
// Keep-alive every 15 seconds: deliberate robustness improvement for NAT'd
// mesh links where long-idle connections would otherwise be silently dropped
// by intermediate NAT boxes.
// ---------------------------------------------------------------------------
fn mesh_transport_config() -> TransportConfig {
    TransportConfig::default().keep_alive_interval(Some(Duration::from_secs(15)))
}

// ---------------------------------------------------------------------------
// Build the shared CryptoProvider (oxiquic pure-Rust provider, no ring).
// The same Arc<CryptoProvider> is reused for every TLS config in this file
// to guarantee provider consistency on hot-rotated ServerConfigs.
// ---------------------------------------------------------------------------
fn quic_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(oxiquic_crypto::quic_crypto_provider())
}

/// Return an IPv4 wildcard `SocketAddr` (`0.0.0.0:0`) without `.unwrap()`.
fn ipv4_wildcard() -> SocketAddr {
    use std::net::{Ipv4Addr, SocketAddrV4};
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
}

/// Return an IPv6 wildcard `SocketAddr` (`[::]:0`) without `.unwrap()`.
fn ipv6_wildcard() -> SocketAddr {
    use std::net::{Ipv6Addr, SocketAddrV6};
    SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0))
}

/// QUIC transport for MielinMesh
pub struct QuicTransport {
    /// Server endpoint — Some on the server side; None for client-only.
    server: Option<ServerEndpoint>,
    /// Client endpoint — always present (used for outgoing connections).
    client: Option<ClientEndpoint>,
    connections: Arc<RwLock<ConnectionPool>>,
    cert_manager: Option<Arc<CertManager>>,
}

impl QuicTransport {
    /// Create a new QUIC transport bound to the given address (server + implicit client).
    pub async fn new(bind_addr: SocketAddr) -> Result<Self, WireError> {
        let server_cfg = Self::build_server_config()?;
        let server = ServerEndpoint::bind(bind_addr, server_cfg, mesh_transport_config())
            .await
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create server endpoint: {e}"))
            })?;

        Ok(Self {
            server: Some(server),
            client: None,
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: None,
        })
    }

    /// Create a new QUIC transport with managed certificates.
    pub async fn new_with_certs(
        bind_addr: SocketAddr,
        node_id: &str,
        cert_manager: Arc<CertManager>,
    ) -> Result<Self, WireError> {
        let cert = cert_manager
            .get_or_generate_cert(node_id)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to get certificate: {e}")))?;

        let server_cfg = Self::build_server_config_from_cert(&cert)?;
        let server = ServerEndpoint::bind(bind_addr, server_cfg, mesh_transport_config())
            .await
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create server endpoint: {e}"))
            })?;

        Ok(Self {
            server: Some(server),
            client: None,
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: Some(cert_manager),
        })
    }

    /// Create a client-only QUIC transport (no server endpoint).
    pub async fn new_client() -> Result<Self, WireError> {
        let client_cfg = Self::build_client_config()?;
        // Bind to IPv4 wildcard so the client socket can connect to IPv4
        // server addresses (e.g. 127.0.0.1).  macOS does not implement
        // dual-stack on loopback — an IPv6 socket cannot reach 127.0.0.1.
        let wildcard: SocketAddr = "0.0.0.0:0"
            .parse()
            .expect("static IPv4 wildcard addr must parse");
        let client = ClientEndpoint::bind(wildcard, client_cfg, mesh_transport_config())
            .await
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create client endpoint: {e}"))
            })?;

        Ok(Self {
            server: None,
            client: Some(client),
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: None,
        })
    }

    /// Create a client-only QUIC transport whose socket family matches `server_addr`.
    ///
    /// Unlike [`Self::new_client`], which always binds `0.0.0.0:0` (IPv4 wildcard),
    /// this constructor binds `[::]:0` when `server_addr` is IPv6 and `0.0.0.0:0`
    /// otherwise.  On macOS, an IPv6 wildcard socket cannot reach `127.0.0.1`, so
    /// this function must be used when connecting to IPv6 peers.
    pub async fn new_client_for(server_addr: SocketAddr) -> Result<Self, WireError> {
        let client_cfg = Self::build_client_config()?;
        let bind_addr: SocketAddr = if server_addr.is_ipv6() {
            ipv6_wildcard()
        } else {
            ipv4_wildcard()
        };
        let client = ClientEndpoint::bind(bind_addr, client_cfg, mesh_transport_config())
            .await
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create client endpoint: {e}"))
            })?;

        Ok(Self {
            server: None,
            client: Some(client),
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: None,
        })
    }

    /// Get certificate manager (if available).
    pub fn cert_manager(&self) -> Option<&Arc<CertManager>> {
        self.cert_manager.as_ref()
    }

    /// Connect to a remote peer.
    pub async fn connect(&self, addr: SocketAddr) -> Result<QuicConnection, WireError> {
        // Check if we already have a live pooled connection.
        {
            let mut pool = self.connections.write().await;
            if let Some(conn) = pool.get(&addr) {
                return Ok(conn);
            }
        }

        let endpoint = self.client_endpoint_or_err()?;

        let oxi_conn =
            tokio::time::timeout(CONNECTION_TIMEOUT, endpoint.connect(addr, "localhost"))
                .await
                .map_err(|_| WireError::ConnectionFailed("Connection timeout".to_string()))?
                .map_err(|e| WireError::ConnectionFailed(format!("Connection failed: {e}")))?;

        let remote_addr = oxi_conn.peer_addr().unwrap_or(addr);
        let driven = Arc::new(oxi_conn.into_driven());
        let quic_conn = QuicConnection {
            connection: driven,
            remote_addr,
        };

        {
            let mut pool = self.connections.write().await;
            pool.insert(addr, quic_conn.clone());
        }

        Ok(quic_conn)
    }

    /// Accept an incoming connection (server side).
    pub async fn accept(&self) -> Result<QuicConnection, WireError> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| WireError::TransportError("No server endpoint".to_string()))?;

        let oxi_conn: OxiQuicConnection = server
            .accept()
            .await
            .map_err(|e| WireError::ConnectionFailed(format!("Accept failed: {e}")))?;

        // Capture peer_addr before consuming the QuicConnection into DrivenConnection.
        let remote_addr = oxi_conn
            .peer_addr()
            .ok_or_else(|| WireError::TransportError("Accept: no peer address".to_string()))?;

        let driven = Arc::new(oxi_conn.into_driven());
        let quic_conn = QuicConnection {
            connection: driven,
            remote_addr,
        };

        {
            let mut pool = self.connections.write().await;
            pool.insert(remote_addr, quic_conn.clone());
        }

        Ok(quic_conn)
    }

    /// Connect with exponential backoff retry.
    pub async fn connect_with_retry(
        &self,
        addr: SocketAddr,
        max_retries: u32,
    ) -> Result<QuicConnection, WireError> {
        let mut delay = Duration::from_millis(100);
        let max_delay = Duration::from_secs(10);

        for attempt in 0..=max_retries {
            match self.connect(addr).await {
                Ok(conn) => return Ok(conn),
                Err(_) if attempt < max_retries => {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
                Err(e) => return Err(e),
            }
        }
        Err(WireError::ConnectionFailed(format!(
            "Failed to connect after {} retries",
            max_retries
        )))
    }

    /// Get connection pool statistics.
    pub async fn pool_stats(&self) -> ConnectionPoolStats {
        let pool = self.connections.read().await;
        pool.stats.clone()
    }

    /// Cleanup idle connections from the pool.
    pub async fn cleanup_pool(&self) {
        let mut pool = self.connections.write().await;
        pool.evict_idle();
    }

    /// Close the transport.
    ///
    /// Note: oxiquic-transport has no endpoint-level `close()` / `wait_idle()`.
    /// We close all pooled connections then drop them; the server and client
    /// endpoint structs are dropped when this value is dropped, which aborts
    /// any background demux tasks.
    pub async fn close(&self) {
        let mut pool = self.connections.write().await;
        for (_, pooled) in pool.connections.drain() {
            // Best-effort close — ignore errors (connection may already be gone).
            let _ = pooled.conn.connection.close(0, b"shutdown").await;
        }
    }

    /// Get local address.
    pub fn local_addr(&self) -> Result<SocketAddr, WireError> {
        if let Some(ref server) = self.server {
            return server
                .local_addr()
                .map_err(|e| WireError::TransportError(format!("Failed to get local addr: {e}")));
        }
        if let Some(ref client) = self.client {
            return client
                .local_addr()
                .map_err(|e| WireError::TransportError(format!("Failed to get local addr: {e}")));
        }
        Err(WireError::TransportError(
            "No endpoint available".to_string(),
        ))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn client_endpoint_or_err(&self) -> Result<&ClientEndpoint, WireError> {
        // Prefer an explicit client endpoint; for server transports create one
        // on demand is not possible without mutability — callers that need to
        // initiate outgoing connections should use `new_client()`.
        self.client.as_ref().ok_or_else(|| {
            WireError::TransportError("No client endpoint (use new_client())".to_string())
        })
    }

    /// Build a server rustls config with a fresh self-signed certificate.
    fn build_server_config() -> Result<Arc<rustls::ServerConfig>, WireError> {
        let ck = oxitls_rcgen::generate_self_signed_p256(&["localhost"])
            .map_err(|e| WireError::TransportError(format!("Failed to generate cert: {e}")))?;

        let priv_key = PrivateKeyDer::try_from(ck.pkcs8_der)
            .map_err(|_| WireError::TransportError("Failed to parse private key".to_string()))?;
        let cert_chain = vec![CertificateDer::from(ck.cert_der)];
        let provider = quic_provider();

        let mut server_cfg = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| WireError::TransportError(format!("Protocol version error: {e}")))?
            .with_no_client_auth()
            .with_single_cert(cert_chain, priv_key)
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create server config: {e}"))
            })?;

        server_cfg.alpn_protocols = vec![b"h3".to_vec()];
        Ok(Arc::new(server_cfg))
    }

    /// Build a server rustls config from a managed certificate.
    fn build_server_config_from_cert(
        cert: &Certificate,
    ) -> Result<Arc<rustls::ServerConfig>, WireError> {
        let cert_chain = cert.cert_chain.clone();
        let priv_key = cert.private_key.clone_key();
        let provider = quic_provider();

        let mut server_cfg = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| WireError::TransportError(format!("Protocol version error: {e}")))?
            .with_no_client_auth()
            .with_single_cert(cert_chain, priv_key)
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create server config: {e}"))
            })?;

        server_cfg.alpn_protocols = vec![b"h3".to_vec()];
        Ok(Arc::new(server_cfg))
    }

    /// Build a client rustls config (skips server cert verification for dev).
    fn build_client_config() -> Result<Arc<rustls::ClientConfig>, WireError> {
        let provider = quic_provider();

        // supported_verify_schemes derived from the provider's sig-verification algorithms.
        let schemes = provider
            .signature_verification_algorithms
            .supported_schemes();

        let mut client_cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| WireError::TransportError(format!("Protocol version error: {e}")))?
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new(schemes))
            .with_no_client_auth();

        client_cfg.alpn_protocols = vec![b"h3".to_vec()];
        Ok(Arc::new(client_cfg))
    }
}

/// A QUIC connection to a remote peer backed by `Arc<DrivenConnection>`.
///
/// `DrivenConnection` is not `Clone`; we wrap in `Arc` so the pool can hand
/// out multiple references to the same live connection.
#[derive(Clone)]
pub struct QuicConnection {
    connection: Arc<DrivenConnection>,
    remote_addr: SocketAddr,
}

impl QuicConnection {
    /// Send a message to the remote peer.
    pub async fn send(&self, message: &Message) -> Result<(), WireError> {
        let data = message.serialize()?;

        if data.len() > MAX_MESSAGE_SIZE {
            return Err(WireError::TransportError(format!(
                "Message too large: {} bytes (max {})",
                data.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        let mut send = self
            .connection
            .open_uni_stream()
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to open stream: {e}")))?;

        send.write_all(&data)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to write: {e}")))?;

        // shutdown() sends the FIN, replacing quinn's `finish()`.
        send.shutdown()
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to finish: {e}")))?;

        Ok(())
    }

    /// Receive a message from the remote peer.
    pub async fn receive(&self) -> Result<Message, WireError> {
        let recv = self
            .connection
            .accept_uni_stream()
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to accept stream: {e}")))?;

        // Cap the read at MAX_MESSAGE_SIZE bytes — AsyncReadExt::read_to_end is
        // uncapped (unlike quinn's read_to_end(max)), so we apply a take() limit
        // to bound memory usage.
        let mut buf = Vec::new();
        recv.take(MAX_MESSAGE_SIZE as u64)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to read: {e}")))?;

        Message::deserialize(&buf)
    }

    /// Get remote address.
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Check if connection is closed.
    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }

    /// Close the connection.
    pub fn close(&self) {
        // Spawn a best-effort close task; errors are ignored.
        let conn = self.connection.clone();
        tokio::spawn(async move {
            let _ = conn.close(0, b"closed").await;
        });
    }
}

/// Connection pool for managing multiple connections with health tracking.
pub struct ConnectionPool {
    connections: std::collections::HashMap<SocketAddr, PooledConnection>,
    /// Maximum connections per pool
    max_connections: usize,
    /// Statistics for connection pool
    pub stats: ConnectionPoolStats,
}

/// A pooled connection with metadata.
struct PooledConnection {
    conn: QuicConnection,
    #[allow(dead_code)]
    created_at: std::time::Instant,
    last_used: std::time::Instant,
    #[allow(dead_code)]
    use_count: u64,
}

/// Connection pool statistics.
#[derive(Debug, Default)]
pub struct ConnectionPoolStats {
    /// Total connections created
    pub connections_created: std::sync::atomic::AtomicU64,
    /// Total connections reused from pool
    pub connections_reused: std::sync::atomic::AtomicU64,
    /// Total connections closed
    pub connections_closed: std::sync::atomic::AtomicU64,
    /// Current active connections
    pub active_connections: std::sync::atomic::AtomicUsize,
    /// Total bytes sent
    pub bytes_sent: std::sync::atomic::AtomicU64,
    /// Total bytes received
    pub bytes_received: std::sync::atomic::AtomicU64,
}

impl Clone for ConnectionPoolStats {
    fn clone(&self) -> Self {
        use std::sync::atomic::Ordering::Relaxed;
        Self {
            connections_created: std::sync::atomic::AtomicU64::new(
                self.connections_created.load(Relaxed),
            ),
            connections_reused: std::sync::atomic::AtomicU64::new(
                self.connections_reused.load(Relaxed),
            ),
            connections_closed: std::sync::atomic::AtomicU64::new(
                self.connections_closed.load(Relaxed),
            ),
            active_connections: std::sync::atomic::AtomicUsize::new(
                self.active_connections.load(Relaxed),
            ),
            bytes_sent: std::sync::atomic::AtomicU64::new(self.bytes_sent.load(Relaxed)),
            bytes_received: std::sync::atomic::AtomicU64::new(self.bytes_received.load(Relaxed)),
        }
    }
}

impl ConnectionPoolStats {
    /// Get hit rate (reused / total).
    pub fn hit_rate(&self) -> f64 {
        let created = self
            .connections_created
            .load(std::sync::atomic::Ordering::Relaxed);
        let reused = self
            .connections_reused
            .load(std::sync::atomic::Ordering::Relaxed);
        let total = created + reused;
        if total == 0 {
            0.0
        } else {
            reused as f64 / total as f64
        }
    }

    /// Get a snapshot of stats as simple values.
    pub fn snapshot(&self) -> ConnectionPoolStatsSnapshot {
        use std::sync::atomic::Ordering::Relaxed;
        ConnectionPoolStatsSnapshot {
            connections_created: self.connections_created.load(Relaxed),
            connections_reused: self.connections_reused.load(Relaxed),
            connections_closed: self.connections_closed.load(Relaxed),
            active_connections: self.active_connections.load(Relaxed),
            bytes_sent: self.bytes_sent.load(Relaxed),
            bytes_received: self.bytes_received.load(Relaxed),
        }
    }
}

/// Snapshot of connection pool statistics (non-atomic).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectionPoolStatsSnapshot {
    /// Total connections created
    pub connections_created: u64,
    /// Total connections reused from pool
    pub connections_reused: u64,
    /// Total connections closed
    pub connections_closed: u64,
    /// Current active connections
    pub active_connections: usize,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
}

impl ConnectionPool {
    /// Default maximum connections.
    const DEFAULT_MAX_CONNECTIONS: usize = 100;
    /// Maximum connection idle time before cleanup.
    const MAX_IDLE_TIME: Duration = Duration::from_secs(300);

    fn new() -> Self {
        Self {
            connections: std::collections::HashMap::new(),
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            stats: ConnectionPoolStats::default(),
        }
    }

    /// Create pool with custom max connections.
    #[allow(dead_code)]
    fn with_max_connections(max: usize) -> Self {
        Self {
            connections: std::collections::HashMap::new(),
            max_connections: max,
            stats: ConnectionPoolStats::default(),
        }
    }

    fn get(&mut self, addr: &SocketAddr) -> Option<QuicConnection> {
        if let Some(pooled) = self.connections.get_mut(addr) {
            if pooled.conn.is_closed() {
                self.connections.remove(addr);
                self.stats
                    .connections_closed
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.stats
                    .active_connections
                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
            pooled.last_used = std::time::Instant::now();
            pooled.use_count += 1;
            self.stats
                .connections_reused
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Some(pooled.conn.clone());
        }
        None
    }

    fn insert(&mut self, addr: SocketAddr, conn: QuicConnection) {
        if self.connections.len() >= self.max_connections {
            self.evict_idle();
        }

        let now = std::time::Instant::now();
        self.connections.insert(
            addr,
            PooledConnection {
                conn,
                created_at: now,
                last_used: now,
                use_count: 1,
            },
        );
        self.stats
            .connections_created
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stats
            .active_connections
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    #[allow(dead_code)]
    fn remove(&mut self, addr: &SocketAddr) {
        if self.connections.remove(addr).is_some() {
            self.stats
                .connections_closed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.stats
                .active_connections
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Remove idle and closed connections.
    fn evict_idle(&mut self) {
        let now = std::time::Instant::now();
        let before_count = self.connections.len();

        self.connections.retain(|_, pooled| {
            let is_alive = !pooled.conn.is_closed();
            let is_recent = now.duration_since(pooled.last_used) < Self::MAX_IDLE_TIME;
            is_alive && is_recent
        });

        let evicted = before_count - self.connections.len();
        if evicted > 0 {
            self.stats
                .connections_closed
                .fetch_add(evicted as u64, std::sync::atomic::Ordering::Relaxed);
            self.stats
                .active_connections
                .fetch_sub(evicted, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Get number of active connections.
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.connections.len()
    }

    /// Check if pool is empty.
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Cleanup all closed connections.
    #[allow(dead_code)]
    fn cleanup_closed(&mut self) {
        let before_count = self.connections.len();
        self.connections
            .retain(|_, pooled| !pooled.conn.is_closed());
        let closed = before_count - self.connections.len();
        if closed > 0 {
            self.stats
                .connections_closed
                .fetch_add(closed as u64, std::sync::atomic::Ordering::Relaxed);
            self.stats
                .active_connections
                .fetch_sub(closed, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Skip server certificate verification (for development only)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SkipServerVerification {
    schemes: Vec<rustls::SignatureScheme>,
}

impl SkipServerVerification {
    fn new(schemes: Vec<rustls::SignatureScheme>) -> Arc<Self> {
        Arc::new(Self { schemes })
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.schemes.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transport_creation() {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let transport = QuicTransport::new(addr).await;
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_client_creation() {
        let transport = QuicTransport::new_client().await;
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_transport_with_cert_manager() {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let cert_manager = Arc::new(CertManager::new());
        let transport =
            QuicTransport::new_with_certs(addr, "test-node", cert_manager.clone()).await;
        assert!(transport.is_ok());

        let transport = transport.expect("transport");
        assert!(transport.cert_manager().is_some());
        assert!(Arc::ptr_eq(
            transport.cert_manager().expect("cert_manager"),
            &cert_manager
        ));
    }

    #[tokio::test]
    async fn test_cert_manager_access() {
        // Transport without cert manager
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let transport = QuicTransport::new(addr).await.expect("transport");
        assert!(transport.cert_manager().is_none());

        // Transport with cert manager
        let cert_manager = Arc::new(CertManager::new());
        let transport = QuicTransport::new_with_certs(addr, "test-node", cert_manager.clone())
            .await
            .expect("transport with certs");
        assert!(transport.cert_manager().is_some());
    }

    #[tokio::test]
    async fn test_message_send_receive() {
        // Create server
        let server_addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let server = QuicTransport::new(server_addr).await.expect("server");
        let actual_addr = server.local_addr().expect("local addr");

        // Create client
        let client = QuicTransport::new_client().await.expect("client");

        // Spawn server task
        let server_task = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept");
            conn.receive().await.expect("receive")
        });

        // Give server time to start listening
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect and send message
        let conn = client.connect(actual_addr).await.expect("connect");
        let test_msg = Message::Ping { timestamp: 12345 };
        conn.send(&test_msg).await.expect("send");

        // Wait for server to receive
        let received = server_task.await.expect("server task");
        assert!(matches!(received, Message::Ping { timestamp: 12345 }));
    }
}
