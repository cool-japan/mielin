//! WebSocket transport implementation
//!
//! Provides WebSocket and WebSocket Secure (WSS) transport for browser compatibility
//! and proxy traversal. Falls back from QUIC when needed.

use crate::{Message, WireError};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::{
    accept_async, client_async, tungstenite::http::Uri,
    tungstenite::protocol::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};

/// Maximum message size (16MB, same as QUIC)
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// WebSocket transport configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Use TLS for WebSocket connections (WSS)
    pub use_tls: bool,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Enable compression
    pub enable_compression: bool,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            use_tls: false,
            max_message_size: MAX_MESSAGE_SIZE,
            timeout_secs: 10,
            enable_compression: true,
        }
    }
}

impl WebSocketConfig {
    /// Create production config with TLS enabled
    pub fn production() -> Self {
        Self {
            use_tls: true,
            max_message_size: MAX_MESSAGE_SIZE,
            timeout_secs: 30,
            enable_compression: true,
        }
    }

    /// Create development config without TLS
    pub fn development() -> Self {
        Self::default()
    }
}

/// WebSocket transport for MielinMesh
pub struct WebSocketTransport {
    listener: Option<TcpListener>,
    config: WebSocketConfig,
    connections: Arc<RwLock<WebSocketPool>>,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport bound to the given address
    pub async fn new(bind_addr: SocketAddr, config: WebSocketConfig) -> Result<Self, WireError> {
        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to bind: {}", e)))?;

        Ok(Self {
            listener: Some(listener),
            config,
            connections: Arc::new(RwLock::new(WebSocketPool::new())),
        })
    }

    /// Create a client-only WebSocket transport
    pub fn new_client(config: WebSocketConfig) -> Self {
        Self {
            listener: None,
            config,
            connections: Arc::new(RwLock::new(WebSocketPool::new())),
        }
    }

    /// Connect to a WebSocket server
    pub async fn connect(&self, url: &str) -> Result<WebSocketConnection, WireError> {
        // Check if we already have a connection
        {
            let mut pool = self.connections.write().await;
            if let Some(conn) = pool.get(url) {
                return Ok(conn);
            }
        }

        // Parse the URL to get the actual host and port to dial. The WebSocket
        // handshake below (`client_async`) only speaks the WS protocol over an
        // already-open TCP stream, so the stream *must* be opened against the
        // host/port encoded in `url`, not a fixed address.
        let uri: Uri = url.parse().map_err(|e| {
            WireError::ConnectionFailed(format!("Invalid WebSocket URL '{}': {}", url, e))
        })?;
        let host = uri.host().ok_or_else(|| {
            WireError::ConnectionFailed(format!("WebSocket URL '{}' is missing a host", url))
        })?;
        let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
            Some("wss") => 443,
            _ => 80,
        });

        let tcp_stream = TcpStream::connect((host, port)).await.map_err(|e| {
            WireError::ConnectionFailed(format!("Failed to connect to {}:{}: {}", host, port, e))
        })?;

        let maybe_tls_stream = MaybeTlsStream::Plain(tcp_stream);

        // Create new connection
        let (ws_stream, _) = client_async(url, maybe_tls_stream).await.map_err(|e| {
            WireError::ConnectionFailed(format!("WebSocket handshake failed: {}", e))
        })?;

        let conn = WebSocketConnection {
            stream: Arc::new(RwLock::new(ws_stream)),
            url: url.to_string(),
            config: self.config.clone(),
        };

        // Store in pool
        let mut pool = self.connections.write().await;
        pool.insert(url.to_string(), conn.clone());

        Ok(conn)
    }

    /// Accept an incoming WebSocket connection
    pub async fn accept(&self) -> Result<WebSocketConnection, WireError> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| WireError::TransportError("No listener configured".to_string()))?;

        let (stream, addr) = listener
            .accept()
            .await
            .map_err(|e| WireError::TransportError(format!("Accept failed: {}", e)))?;

        let maybe_tls_stream = MaybeTlsStream::Plain(stream);

        let ws_stream = accept_async(maybe_tls_stream)
            .await
            .map_err(|e| WireError::TransportError(format!("WebSocket accept failed: {}", e)))?;

        let url = format!("ws://{}", addr);
        let conn = WebSocketConnection {
            stream: Arc::new(RwLock::new(ws_stream)),
            url: url.clone(),
            config: self.config.clone(),
        };

        // Store in pool
        let mut pool = self.connections.write().await;
        pool.insert(url, conn.clone());

        Ok(conn)
    }

    /// Get local address (if server)
    pub fn local_addr(&self) -> Result<SocketAddr, WireError> {
        self.listener
            .as_ref()
            .ok_or_else(|| WireError::TransportError("No listener configured".to_string()))?
            .local_addr()
            .map_err(|e| WireError::TransportError(format!("Failed to get local addr: {}", e)))
    }

    /// Get connection pool statistics
    pub async fn pool_stats(&self) -> WebSocketPoolStats {
        let pool = self.connections.read().await;
        pool.stats.clone()
    }
}

/// A WebSocket connection to a peer
#[derive(Clone)]
pub struct WebSocketConnection {
    stream: Arc<RwLock<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    url: String,
    config: WebSocketConfig,
}

impl WebSocketConnection {
    /// Send a message to the remote peer
    pub async fn send(&self, message: &Message) -> Result<(), WireError> {
        let data = message.serialize()?;

        if data.len() > self.config.max_message_size {
            return Err(WireError::TransportError(format!(
                "Message too large: {} bytes (max {})",
                data.len(),
                self.config.max_message_size
            )));
        }

        let mut stream = self.stream.write().await;
        use futures::SinkExt;
        stream
            .send(WsMessage::Binary(data.into()))
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to send: {}", e)))?;

        Ok(())
    }

    /// Receive a message from the remote peer
    pub async fn receive(&self) -> Result<Message, WireError> {
        let mut stream = self.stream.write().await;
        use futures::StreamExt;

        let msg = stream
            .next()
            .await
            .ok_or_else(|| WireError::TransportError("Connection closed".to_string()))?
            .map_err(|e| WireError::TransportError(format!("Failed to receive: {}", e)))?;

        match msg {
            WsMessage::Binary(data) => {
                if data.len() > self.config.max_message_size {
                    return Err(WireError::TransportError(format!(
                        "Message too large: {} bytes",
                        data.len()
                    )));
                }
                Message::deserialize(&data)
            }
            WsMessage::Text(_) => Err(WireError::TransportError(
                "Text messages not supported".to_string(),
            )),
            WsMessage::Close(_) => Err(WireError::TransportError("Connection closed".to_string())),
            _ => Err(WireError::TransportError(
                "Unexpected message type".to_string(),
            )),
        }
    }

    /// Get remote URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Close the connection
    pub async fn close(&self) -> Result<(), WireError> {
        #[allow(unused_imports)]
        use futures::SinkExt;
        let mut stream = self.stream.write().await;
        stream
            .close(None)
            .await
            .map_err(|e| WireError::TransportError(format!("Failed to close: {}", e)))
    }
}

/// WebSocket connection pool
struct WebSocketPool {
    connections: std::collections::HashMap<String, PooledWsConnection>,
    stats: WebSocketPoolStats,
}

/// A pooled WebSocket connection with metadata
struct PooledWsConnection {
    conn: WebSocketConnection,
    #[allow(dead_code)]
    created_at: std::time::Instant,
    last_used: std::time::Instant,
}

/// WebSocket pool statistics
#[derive(Debug, Default, Clone)]
pub struct WebSocketPoolStats {
    /// Total connections created
    pub connections_created: u64,
    /// Total connections reused
    pub connections_reused: u64,
    /// Total connections closed
    pub connections_closed: u64,
    /// Current active connections
    pub active_connections: usize,
}

impl WebSocketPool {
    fn new() -> Self {
        Self {
            connections: std::collections::HashMap::new(),
            stats: WebSocketPoolStats::default(),
        }
    }

    fn get(&mut self, url: &str) -> Option<WebSocketConnection> {
        if let Some(pooled) = self.connections.get_mut(url) {
            pooled.last_used = std::time::Instant::now();
            self.stats.connections_reused += 1;
            return Some(pooled.conn.clone());
        }
        None
    }

    fn insert(&mut self, url: String, conn: WebSocketConnection) {
        let now = std::time::Instant::now();
        self.connections.insert(
            url,
            PooledWsConnection {
                conn,
                created_at: now,
                last_used: now,
            },
        );
        self.stats.connections_created += 1;
        self.stats.active_connections += 1;
    }
}

/// WebSocket URL builder
pub struct WebSocketUrlBuilder {
    host: String,
    port: u16,
    path: String,
    use_tls: bool,
}

impl WebSocketUrlBuilder {
    /// Create a new URL builder
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            path: "/".to_string(),
            use_tls: false,
        }
    }

    /// Set the path
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Enable TLS (wss://)
    pub fn with_tls(mut self, use_tls: bool) -> Self {
        self.use_tls = use_tls;
        self
    }

    /// Build the WebSocket URL
    pub fn build(&self) -> String {
        let scheme = if self.use_tls { "wss" } else { "ws" };
        format!("{}://{}:{}{}", scheme, self.host, self.port, self.path)
    }
}

/// WebSocket handshake upgrade handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeToWebSocket {
    /// QUIC connection ID to upgrade
    pub connection_id: String,
    /// Target WebSocket URL
    pub websocket_url: String,
    /// Reason for upgrade
    pub reason: UpgradeReason,
}

/// Reasons for upgrading to WebSocket
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpgradeReason {
    /// QUIC not supported by peer
    QuicNotSupported,
    /// Behind restrictive firewall/proxy
    ProxyTraversal,
    /// Browser client connection
    BrowserClient,
    /// User requested fallback
    ManualFallback,
}

/// Response to WebSocket upgrade request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    /// Whether upgrade was accepted
    pub accepted: bool,
    /// WebSocket URL to connect to (if accepted)
    pub websocket_url: Option<String>,
    /// Error message (if rejected)
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_config() {
        let config = WebSocketConfig::default();
        assert!(!config.use_tls);
        assert_eq!(config.max_message_size, MAX_MESSAGE_SIZE);
        assert!(config.enable_compression);

        let prod_config = WebSocketConfig::production();
        assert!(prod_config.use_tls);
        assert_eq!(prod_config.timeout_secs, 30);
    }

    #[test]
    fn test_url_builder() {
        let url = WebSocketUrlBuilder::new("localhost", 8080).build();
        assert_eq!(url, "ws://localhost:8080/");

        let url = WebSocketUrlBuilder::new("example.com", 443)
            .with_tls(true)
            .path("/mesh/wire")
            .build();
        assert_eq!(url, "wss://example.com:443/mesh/wire");
    }

    #[test]
    fn test_url_builder_custom_path() {
        let url = WebSocketUrlBuilder::new("localhost", 9000)
            .path("/api/v1/ws")
            .build();
        assert_eq!(url, "ws://localhost:9000/api/v1/ws");
    }

    #[tokio::test]
    async fn test_websocket_transport_creation() {
        let config = WebSocketConfig::development();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = WebSocketTransport::new(addr, config).await;
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_client_only_transport() {
        let config = WebSocketConfig::development();
        let transport = WebSocketTransport::new_client(config);
        assert!(transport.listener.is_none());
        assert!(transport.local_addr().is_err());
    }

    #[test]
    fn test_upgrade_request() {
        let upgrade = UpgradeToWebSocket {
            connection_id: "conn-123".to_string(),
            websocket_url: "wss://example.com:443/ws".to_string(),
            reason: UpgradeReason::BrowserClient,
        };
        assert_eq!(upgrade.reason, UpgradeReason::BrowserClient);
        assert_eq!(upgrade.connection_id, "conn-123");
    }

    #[test]
    fn test_upgrade_response() {
        let response = UpgradeResponse {
            accepted: true,
            websocket_url: Some("wss://example.com/ws".to_string()),
            error: None,
        };
        assert!(response.accepted);
        assert!(response.websocket_url.is_some());
        assert!(response.error.is_none());

        let error_response = UpgradeResponse {
            accepted: false,
            websocket_url: None,
            error: Some("Not supported".to_string()),
        };
        assert!(!error_response.accepted);
        assert!(error_response.error.is_some());
    }

    #[tokio::test]
    async fn test_connect_honors_target_host_and_port() {
        // Regression test: `connect()` must dial the host/port encoded in the
        // supplied URL rather than a hardcoded address. We bind the server on
        // an OS-assigned ephemeral port (almost certainly not 8080) and drive
        // a full round trip through it; this would fail (or hang against a
        // dead/foreign socket) if `connect()` ignored the URL and always
        // dialed a fixed `localhost:8080`.
        let server_config = WebSocketConfig::development();
        let server = WebSocketTransport::new("127.0.0.1:0".parse().unwrap(), server_config)
            .await
            .expect("server transport should bind");
        let server_addr = server.local_addr().expect("server should have local addr");
        assert_ne!(
            server_addr.port(),
            8080,
            "test requires a non-8080 ephemeral port to prove the target is honored"
        );

        let accept_task = tokio::spawn(async move {
            let conn = server.accept().await.expect("server should accept");
            let msg = conn.receive().await.expect("server should receive");
            match msg {
                Message::Ping { timestamp } => timestamp,
                other => panic!("unexpected message: {:?}", other),
            }
        });

        let client = WebSocketTransport::new_client(WebSocketConfig::development());
        let url = WebSocketUrlBuilder::new("127.0.0.1", server_addr.port()).build();
        let conn = client
            .connect(&url)
            .await
            .expect("client should connect to the exact host/port from the URL");

        conn.send(&Message::Ping { timestamp: 42 })
            .await
            .expect("client should send");

        let received_timestamp = accept_task.await.expect("server task should not panic");
        assert_eq!(received_timestamp, 42);
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let config = WebSocketConfig::development();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = WebSocketTransport::new(addr, config).await.unwrap();

        let stats = transport.pool_stats().await;
        assert_eq!(stats.connections_created, 0);
        assert_eq!(stats.active_connections, 0);
    }
}
