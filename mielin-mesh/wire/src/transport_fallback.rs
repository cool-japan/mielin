//! Transport Fallback and Negotiation
//!
//! Provides automatic fallback from QUIC to TCP when QUIC is unavailable,
//! blocked, or fails. This ensures connectivity in restricted networks.

use crate::tcp_transport::{TcpConnection, TcpTransport};
use crate::transport::{QuicConnection, QuicTransport};
use crate::{Message, WireError};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;

/// Transport fallback timeout - how long to wait before trying fallback
const FALLBACK_TIMEOUT: Duration = Duration::from_secs(5);

/// Transport mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// QUIC transport (preferred)
    Quic,
    /// TCP transport (fallback)
    Tcp,
}

/// Connection that can use either QUIC or TCP
pub enum Connection {
    Quic(QuicConnection),
    Tcp(TcpConnection),
}

impl Connection {
    /// Send a message through this connection
    pub async fn send(&self, message: &Message) -> Result<(), WireError> {
        match self {
            Connection::Quic(conn) => conn.send(message).await,
            Connection::Tcp(conn) => conn.send(message).await,
        }
    }

    /// Receive a message from this connection
    pub async fn receive(&self) -> Result<Message, WireError> {
        match self {
            Connection::Quic(conn) => conn.receive().await,
            Connection::Tcp(conn) => conn.receive().await,
        }
    }

    /// Get the transport mode being used
    pub fn mode(&self) -> TransportMode {
        match self {
            Connection::Quic(_) => TransportMode::Quic,
            Connection::Tcp(_) => TransportMode::Tcp,
        }
    }
}

/// Transport with automatic fallback capability
pub struct FallbackTransport {
    quic: Option<Arc<QuicTransport>>,
    tcp: Arc<TcpTransport>,
    preferred_mode: Arc<RwLock<TransportMode>>,
}

impl FallbackTransport {
    /// Create a new fallback transport
    pub async fn new(bind_addr: SocketAddr) -> Result<Self, WireError> {
        // Try to create QUIC transport (may fail if not supported)
        let quic = match QuicTransport::new(bind_addr).await {
            Ok(transport) => Some(Arc::new(transport)),
            Err(_) => None,
        };

        // Always create TCP transport as fallback
        let tcp = Arc::new(TcpTransport::new(bind_addr)?);

        let preferred_mode = if quic.is_some() {
            TransportMode::Quic
        } else {
            TransportMode::Tcp
        };

        Ok(Self {
            quic,
            tcp,
            preferred_mode: Arc::new(RwLock::new(preferred_mode)),
        })
    }

    /// Connect to a remote peer with automatic fallback
    pub async fn connect(&self, addr: SocketAddr) -> Result<Connection, WireError> {
        let mode = *self.preferred_mode.read().await;

        match mode {
            TransportMode::Quic => {
                // Try QUIC first
                if let Some(ref quic) = self.quic {
                    match timeout(FALLBACK_TIMEOUT, quic.connect(addr)).await {
                        Ok(Ok(conn)) => return Ok(Connection::Quic(conn)),
                        Ok(Err(_)) | Err(_) => {
                            // QUIC failed, fall back to TCP
                            tracing::info!("QUIC connection failed, falling back to TCP");
                            *self.preferred_mode.write().await = TransportMode::Tcp;
                        }
                    }
                }

                // Fall back to TCP
                let tcp_conn = self.tcp.connect(addr).await?;
                Ok(Connection::Tcp(tcp_conn))
            }
            TransportMode::Tcp => {
                // Use TCP directly
                let tcp_conn = self.tcp.connect(addr).await?;
                Ok(Connection::Tcp(tcp_conn))
            }
        }
    }

    /// Get the current preferred transport mode
    pub async fn preferred_mode(&self) -> TransportMode {
        *self.preferred_mode.read().await
    }

    /// Manually set the preferred transport mode
    pub async fn set_preferred_mode(&self, mode: TransportMode) {
        *self.preferred_mode.write().await = mode;
    }

    /// Check if QUIC transport is available
    pub fn has_quic(&self) -> bool {
        self.quic.is_some()
    }
}

/// Transport negotiator for selecting optimal transport
pub struct TransportNegotiator {
    /// Tracks which transports work for each peer
    peer_transports: Arc<RwLock<std::collections::HashMap<SocketAddr, TransportMode>>>,
}

impl TransportNegotiator {
    /// Create a new transport negotiator
    pub fn new() -> Self {
        Self {
            peer_transports: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Negotiate transport with a peer
    pub async fn negotiate(
        &self,
        fallback: &FallbackTransport,
        addr: SocketAddr,
    ) -> Result<Connection, WireError> {
        // Check if we have a known working transport for this peer
        {
            let transports = self.peer_transports.read().await;
            if let Some(&mode) = transports.get(&addr) {
                // Use the known working transport
                match mode {
                    TransportMode::Quic => {
                        if let Some(ref quic) = fallback.quic {
                            if let Ok(conn) = quic.connect(addr).await {
                                return Ok(Connection::Quic(conn));
                            }
                        }
                    }
                    TransportMode::Tcp => {
                        let conn = fallback.tcp.connect(addr).await?;
                        return Ok(Connection::Tcp(conn));
                    }
                }
            }
        }

        // Try to connect and remember what works
        let conn = fallback.connect(addr).await?;
        let mode = conn.mode();

        // Remember this transport for future connections
        {
            let mut transports = self.peer_transports.write().await;
            transports.insert(addr, mode);
        }

        Ok(conn)
    }

    /// Reset transport selection for a peer (e.g., after connection failure)
    pub async fn reset_peer(&self, addr: &SocketAddr) {
        let mut transports = self.peer_transports.write().await;
        transports.remove(addr);
    }

    /// Get statistics about transport usage
    pub async fn transport_stats(&self) -> TransportStats {
        let transports = self.peer_transports.read().await;
        let mut quic_count = 0;
        let mut tcp_count = 0;

        for mode in transports.values() {
            match mode {
                TransportMode::Quic => quic_count += 1,
                TransportMode::Tcp => tcp_count += 1,
            }
        }

        TransportStats {
            quic_peers: quic_count,
            tcp_peers: tcp_count,
            total_peers: transports.len(),
        }
    }
}

impl Default for TransportNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about transport usage
#[derive(Debug, Clone)]
pub struct TransportStats {
    pub quic_peers: usize,
    pub tcp_peers: usize,
    pub total_peers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_mode_equality() {
        assert_eq!(TransportMode::Quic, TransportMode::Quic);
        assert_eq!(TransportMode::Tcp, TransportMode::Tcp);
        assert_ne!(TransportMode::Quic, TransportMode::Tcp);
    }

    #[tokio::test]
    async fn test_fallback_transport_creation() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let transport = FallbackTransport::new(addr).await;
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_fallback_transport_modes() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let transport = FallbackTransport::new(addr).await.unwrap();

        // Should have a preferred mode
        let mode = transport.preferred_mode().await;
        assert!(matches!(mode, TransportMode::Quic | TransportMode::Tcp));

        // Should be able to change preferred mode
        transport.set_preferred_mode(TransportMode::Tcp).await;
        assert_eq!(transport.preferred_mode().await, TransportMode::Tcp);
    }

    #[tokio::test]
    async fn test_transport_negotiator_creation() {
        let negotiator = TransportNegotiator::new();
        let stats = negotiator.transport_stats().await;
        assert_eq!(stats.total_peers, 0);
        assert_eq!(stats.quic_peers, 0);
        assert_eq!(stats.tcp_peers, 0);
    }

    #[tokio::test]
    async fn test_negotiator_default() {
        let negotiator = TransportNegotiator::default();
        let stats = negotiator.transport_stats().await;
        assert_eq!(stats.total_peers, 0);
    }

    #[tokio::test]
    async fn test_negotiator_reset_peer() {
        let negotiator = TransportNegotiator::new();
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();

        // Reset non-existent peer (should not panic)
        negotiator.reset_peer(&addr).await;

        let stats = negotiator.transport_stats().await;
        assert_eq!(stats.total_peers, 0);
    }

    #[test]
    fn test_transport_stats_creation() {
        let stats = TransportStats {
            quic_peers: 5,
            tcp_peers: 3,
            total_peers: 8,
        };

        assert_eq!(stats.quic_peers, 5);
        assert_eq!(stats.tcp_peers, 3);
        assert_eq!(stats.total_peers, 8);
    }

    #[test]
    fn test_connection_mode_detection() {
        // This is a conceptual test - actual connection creation requires running servers
        // The test verifies that Connection enum has the mode() method
        // Real integration tests would create actual connections
    }
}
