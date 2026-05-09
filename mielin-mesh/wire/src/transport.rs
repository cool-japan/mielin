//! QUIC transport implementation using Quinn
//!
//! Provides low-latency, multiplexed transport for MielinMesh communication.

use crate::certs::{CertManager, Certificate};
use crate::{Message, WireError};
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Maximum message size (16MB)
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Connection timeout duration
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// QUIC transport for MielinMesh
pub struct QuicTransport {
    endpoint: Endpoint,
    connections: Arc<RwLock<ConnectionPool>>,
    cert_manager: Option<Arc<CertManager>>,
}

impl QuicTransport {
    /// Create a new QUIC transport bound to the given address
    pub async fn new(bind_addr: SocketAddr) -> Result<Self, WireError> {
        let server_config = Self::configure_server()?;
        let endpoint = Endpoint::server(server_config, bind_addr)
            .map_err(|e| WireError::TransportError(format!("Failed to create endpoint: {}", e)))?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: None,
        })
    }

    /// Create a new QUIC transport with managed certificates
    pub async fn new_with_certs(
        bind_addr: SocketAddr,
        node_id: &str,
        cert_manager: Arc<CertManager>,
    ) -> Result<Self, WireError> {
        let cert = cert_manager
            .get_or_generate_cert(node_id)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to get certificate: {}", e)))?;

        let server_config = Self::configure_server_with_cert(&cert)?;
        let endpoint = Endpoint::server(server_config, bind_addr)
            .map_err(|e| WireError::TransportError(format!("Failed to create endpoint: {}", e)))?;

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: Some(cert_manager),
        })
    }

    /// Create a client-only QUIC transport
    pub async fn new_client() -> Result<Self, WireError> {
        let client_config = Self::configure_client()?;

        let mut endpoint = Endpoint::client(
            "[::]:0"
                .parse()
                .expect("static IPv6 wildcard addr must parse"),
        )
        .map_err(|e| {
            WireError::TransportError(format!("Failed to create client endpoint: {}", e))
        })?;

        endpoint.set_default_client_config(client_config);

        Ok(Self {
            endpoint,
            connections: Arc::new(RwLock::new(ConnectionPool::new())),
            cert_manager: None,
        })
    }

    /// Get certificate manager (if available)
    pub fn cert_manager(&self) -> Option<&Arc<CertManager>> {
        self.cert_manager.as_ref()
    }

    /// Connect to a remote peer
    pub async fn connect(&self, addr: SocketAddr) -> Result<QuicConnection, WireError> {
        // Check if we already have a connection
        {
            let mut pool = self.connections.write().await;
            if let Some(conn) = pool.get(&addr) {
                return Ok(conn);
            }
        }

        // Create new connection
        let connecting = self
            .endpoint
            .connect(addr, "localhost")
            .map_err(|e| WireError::ConnectionFailed(format!("Connection failed: {}", e)))?;

        let connection = tokio::time::timeout(CONNECTION_TIMEOUT, connecting)
            .await
            .map_err(|_| WireError::ConnectionFailed("Connection timeout".to_string()))?
            .map_err(|e| WireError::ConnectionFailed(format!("Connection failed: {}", e)))?;

        let quic_conn = QuicConnection {
            connection: connection.clone(),
            remote_addr: addr,
        };

        // Store in pool
        let mut pool = self.connections.write().await;
        pool.insert(addr, quic_conn.clone());

        Ok(quic_conn)
    }

    /// Accept an incoming connection
    pub async fn accept(&self) -> Result<QuicConnection, WireError> {
        let connecting = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| WireError::TransportError("Endpoint closed".to_string()))?;

        let connection = connecting
            .await
            .map_err(|e| WireError::ConnectionFailed(format!("Accept failed: {}", e)))?;

        let remote_addr = connection.remote_address();

        let quic_conn = QuicConnection {
            connection: connection.clone(),
            remote_addr,
        };

        // Store in pool
        let mut pool = self.connections.write().await;
        pool.insert(remote_addr, quic_conn.clone());

        Ok(quic_conn)
    }

    /// Connect with exponential backoff retry
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

    /// Get connection pool statistics
    pub async fn pool_stats(&self) -> ConnectionPoolStats {
        let pool = self.connections.read().await;
        pool.stats.clone()
    }

    /// Cleanup idle connections from the pool
    pub async fn cleanup_pool(&self) {
        let mut pool = self.connections.write().await;
        pool.evict_idle();
    }

    /// Close the transport
    pub fn close(&self) {
        self.endpoint.close(VarInt::from_u32(0), b"shutdown");
    }

    /// Get local address
    pub fn local_addr(&self) -> Result<SocketAddr, WireError> {
        self.endpoint
            .local_addr()
            .map_err(|e| WireError::TransportError(format!("Failed to get local addr: {}", e)))
    }

    /// Configure server with self-signed certificate
    fn configure_server() -> Result<ServerConfig, WireError> {
        let cert_key = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|e| WireError::TransportError(format!("Failed to generate cert: {}", e)))?;

        let cert_der = cert_key.cert.der();
        let priv_key = cert_key.signing_key.serialize_der();

        let priv_key = PrivateKeyDer::try_from(priv_key.to_vec())
            .map_err(|_| WireError::TransportError("Failed to parse private key".to_string()))?;
        let cert_chain = vec![cert_der.clone()];

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, priv_key)
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create server config: {}", e))
            })?;

        server_crypto.alpn_protocols = vec![b"h3".to_vec()];

        let server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).map_err(|e| {
                WireError::TransportError(format!("Failed to create QUIC config: {}", e))
            })?,
        ));

        Ok(server_config)
    }

    /// Configure server with a managed certificate
    fn configure_server_with_cert(cert: &Certificate) -> Result<ServerConfig, WireError> {
        let cert_chain = cert.cert_chain.clone();
        let priv_key = cert.private_key.clone_key();

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, priv_key)
            .map_err(|e| {
                WireError::TransportError(format!("Failed to create server config: {}", e))
            })?;

        server_crypto.alpn_protocols = vec![b"h3".to_vec()];

        let server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).map_err(|e| {
                WireError::TransportError(format!("Failed to create QUIC config: {}", e))
            })?,
        ));

        Ok(server_config)
    }

    /// Configure client to skip certificate verification (for development)
    fn configure_client() -> Result<ClientConfig, WireError> {
        let mut crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth();

        crypto.alpn_protocols = vec![b"h3".to_vec()];

        let client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto).map_err(|e| {
                WireError::TransportError(format!("Failed to create QUIC client config: {}", e))
            })?,
        ));

        Ok(client_config)
    }
}

/// A QUIC connection to a remote peer
#[derive(Clone)]
pub struct QuicConnection {
    connection: Connection,
    remote_addr: SocketAddr,
}

impl QuicConnection {
    /// Send a message to the remote peer
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
            .open_uni()
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to open stream: {}", e)))?;

        send.write_all(&data)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to write: {}", e)))?;

        send.finish()
            .map_err(|e| WireError::TransportError(format!("Failed to finish: {}", e)))?;

        Ok(())
    }

    /// Receive a message from the remote peer
    pub async fn receive(&self) -> Result<Message, WireError> {
        let mut recv =
            self.connection.accept_uni().await.map_err(|e| {
                WireError::TransportError(format!("Failed to accept stream: {}", e))
            })?;

        let data = recv
            .read_to_end(MAX_MESSAGE_SIZE)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to read: {}", e)))?;

        Message::deserialize(&data)
    }

    /// Get remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Check if connection is closed
    pub fn is_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }

    /// Close the connection
    pub fn close(&self) {
        self.connection.close(VarInt::from_u32(0), b"closed");
    }
}

/// Connection pool for managing multiple connections with health tracking
pub struct ConnectionPool {
    connections: std::collections::HashMap<SocketAddr, PooledConnection>,
    /// Maximum connections per pool
    max_connections: usize,
    /// Statistics for connection pool
    pub stats: ConnectionPoolStats,
}

/// A pooled connection with metadata
struct PooledConnection {
    conn: QuicConnection,
    #[allow(dead_code)]
    created_at: std::time::Instant,
    last_used: std::time::Instant,
    #[allow(dead_code)]
    use_count: u64,
}

/// Connection pool statistics
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
    /// Get hit rate (reused / total)
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

    /// Get a snapshot of stats as simple values
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

/// Snapshot of connection pool statistics (non-atomic)
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
    /// Default maximum connections
    const DEFAULT_MAX_CONNECTIONS: usize = 100;
    /// Maximum connection idle time before cleanup
    const MAX_IDLE_TIME: Duration = Duration::from_secs(300);

    fn new() -> Self {
        Self {
            connections: std::collections::HashMap::new(),
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            stats: ConnectionPoolStats::default(),
        }
    }

    /// Create pool with custom max connections
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
            // Check if connection is still alive
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
        // Evict idle connections if at capacity
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

    /// Remove idle and closed connections
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

    /// Get number of active connections
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.connections.len()
    }

    /// Check if pool is empty
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Cleanup all closed connections
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

/// Skip server certificate verification (for development only)
#[derive(Debug)]
struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
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
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_crypto() {
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    #[tokio::test]
    async fn test_transport_creation() {
        init_crypto();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = QuicTransport::new(addr).await;
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_client_creation() {
        init_crypto();
        let transport = QuicTransport::new_client().await;
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_transport_with_cert_manager() {
        init_crypto();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let cert_manager = Arc::new(CertManager::new());
        let transport =
            QuicTransport::new_with_certs(addr, "test-node", cert_manager.clone()).await;
        assert!(transport.is_ok());

        let transport = transport.unwrap();
        assert!(transport.cert_manager().is_some());
        assert!(Arc::ptr_eq(
            transport.cert_manager().unwrap(),
            &cert_manager
        ));
    }

    #[tokio::test]
    async fn test_cert_manager_access() {
        init_crypto();
        // Transport without cert manager
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = QuicTransport::new(addr).await.unwrap();
        assert!(transport.cert_manager().is_none());

        // Transport with cert manager
        let cert_manager = Arc::new(CertManager::new());
        let transport = QuicTransport::new_with_certs(addr, "test-node", cert_manager.clone())
            .await
            .unwrap();
        assert!(transport.cert_manager().is_some());
    }

    #[tokio::test]
    async fn test_message_send_receive() {
        init_crypto();
        // Create server
        let server_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = QuicTransport::new(server_addr).await.unwrap();
        let actual_addr = server.local_addr().unwrap();

        // Create client
        let client = QuicTransport::new_client().await.unwrap();

        // Spawn server task
        let server_task = tokio::spawn(async move {
            let conn = server.accept().await.unwrap();
            let msg = conn.receive().await.unwrap();
            msg
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect and send message
        let conn = client.connect(actual_addr).await.unwrap();
        let test_msg = Message::Ping { timestamp: 12345 };
        conn.send(&test_msg).await.unwrap();

        // Wait for server to receive
        let received = server_task.await.unwrap();
        assert!(matches!(received, Message::Ping { timestamp: 12345 }));
    }
}
