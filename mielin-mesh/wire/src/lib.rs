//! MielinMesh Wire Protocol
//!
//! QUIC-based protocol for agent migration and mesh communication.

pub mod ack;
pub mod batch;
pub mod certs;
pub mod compression;
pub mod discovery;
pub mod flow;
pub mod gossip;
pub mod health;
pub mod lifecycle_hooks;
pub mod migration;
pub mod multipath;
pub mod cert_rotation;
pub mod load_test;
pub mod netsim;
pub mod quantum;
pub mod tls_bench;
pub mod peer_lifecycle;
pub mod priority;
pub mod protocol;
pub mod retry;
pub mod security;
pub mod tcp_transport;
pub mod transport;
pub mod transport_fallback;
pub mod version;
pub mod websocket;

pub use ack::{
    shared_ack_tracker, AckConfig, AckError, AckManager, AckResult, AckStats, AckStatus,
    AckTracker, Acknowledgment, DeliveryCallback, MessageId, PendingMessage, ReliableMessage,
    RetryAction, SharedAckTracker,
};
pub use batch::{BatchConfig, BatchError, BatchStats, MessageBatch, MessageBatcher};
pub use compression::{
    compress, decompress, CompressedMessage, CompressionAlgorithm, CompressionLevel, Compressor,
};
pub use discovery::{
    Capability, DiscoveredPeer, DiscoveryConfig, DiscoveryMethod, DiscoveryService, DiscoveryStats,
    PeerExchange,
};
pub use flow::{
    shared_flow_controller, BackpressureController, BackpressureLevel, BackpressureReason,
    BackpressureSignal, CongestionAlgorithm, CongestionConfig, CongestionController,
    CongestionState, CongestionStats, FlowControlSummary, FlowController, SharedFlowController,
    TokenBucket, TokenBucketConfig,
};
pub use gossip::{
    GossipConfig, GossipMessage, GossipService, GossipState, GossipStats, VectorClock,
};
pub use health::{HealthConfig, HealthEvent, HealthMetrics, HealthMonitor, HealthStatus};
pub use lifecycle_hooks::{
    HookBuilder, HookContext, HookEventType, HookExecutionMode, HookFn, IntoHookContext,
    LifecycleHookManager,
};
pub use migration::{
    AgentSnapshot, ExecutionContext, MemoryPage, MigrationConfig, MigrationCoordinator,
    MigrationMessage, MigrationPhase, MigrationState, MigrationStats,
};
pub use multipath::{
    shared_path_pool, MtuDiscovery, MtuDiscoveryConfig, MtuDiscoveryState, MultiPathError,
    MultiPathPolicy, NetworkPath, PathFailover, PathHealthConfig, PathId, PathInfo, PathPool,
    PathPoolConfig, PathPoolStats, PathSelector, PathState, SharedPathPool, DEFAULT_MTU, MAX_MTU,
    MIN_MTU,
};
pub use peer_lifecycle::{
    PeerLifecycleConfig, PeerLifecycleEvent, PeerLifecycleManager, PeerLifecycleStats, PeerState,
};
pub use priority::{
    Priority, PriorityQueue, QueueConfig, QueueStats, QueuedMessage, SharedPriorityQueue,
};
pub use retry::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerStats, CircuitState, RetryExecutor,
    RetryPolicy, RetryStrategy,
};
pub use security::{
    AnomalyDetector, IpFilter, KeyExchangeAlgorithm, RateLimiter, SecurityError, SecurityManager,
    SignatureAlgorithm, TlsCipherSuite, TlsConfig, TlsVersion,
};
pub use tcp_transport::{TcpConnection, TcpPoolStats, TcpTransport};
pub use transport::{
    ConnectionPoolStats, ConnectionPoolStatsSnapshot, QuicConnection, QuicTransport,
};
pub use transport_fallback::{FallbackTransport, TransportMode, TransportNegotiator};
pub use version::{
    NegotiationResult, ProtocolFeature, ProtocolVersion, UpgradeRequest, UpgradeResponse,
    VersionError, VersionNegotiation, VersionNegotiator,
};
pub use websocket::{
    UpgradeReason, UpgradeToWebSocket, WebSocketConfig, WebSocketConnection, WebSocketPoolStats,
    WebSocketTransport, WebSocketUrlBuilder,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WireError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Transport error: {0}")]
    TransportError(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Ping {
        timestamp: u64,
    },
    Pong {
        timestamp: u64,
        latency_ms: u32,
    },
    AgentMigration {
        agent_id: [u8; 16],
        snapshot: Vec<u8>,
        priority: u8,
    },
    MigrationAck {
        agent_id: [u8; 16],
        success: bool,
        error_msg: Option<String>,
    },
    Discovery {
        node_id: [u8; 16],
        node_role: NodeRole,
        capabilities: Vec<String>,
    },
    DiscoveryResponse {
        node_id: [u8; 16],
        peers: Vec<PeerDescriptor>,
    },
    LoadInfo {
        cpu_usage: f32,
        memory_usage: f32,
        active_agents: usize,
    },
    AgentQuery {
        agent_id: [u8; 16],
    },
    /// Multi-hop routed message envelope
    RoutedMessage {
        source: [u8; 16],
        destination: [u8; 16],
        ttl: u8,
        hop_count: u8,
        payload: Box<Message>,
    },
    /// Protocol version negotiation
    VersionNegotiationRequest {
        negotiation: version::VersionNegotiation,
    },
    /// Protocol version negotiation response
    VersionNegotiationResponse {
        result: Result<version::NegotiationResult, String>,
    },
    /// Protocol upgrade request
    ProtocolUpgrade {
        request: version::UpgradeRequest,
    },
    /// Protocol upgrade response
    ProtocolUpgradeResponse {
        response: version::UpgradeResponse,
    },
    /// WebSocket upgrade request
    WebSocketUpgrade {
        upgrade: websocket::UpgradeToWebSocket,
    },
    /// WebSocket upgrade response
    WebSocketUpgradeResponse {
        response: websocket::UpgradeResponse,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    Edge,
    Relay,
    Core,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDescriptor {
    pub node_id: [u8; 16],
    pub address: String,
    pub latency_ms: Option<u32>,
}

impl Message {
    pub fn serialize(&self) -> Result<Vec<u8>, WireError> {
        oxicode::encode_to_vec(&oxicode::serde::Compat(self))
            .map_err(|e| WireError::SerializationError(e.to_string()))
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, WireError> {
        let (compat, _): (oxicode::serde::Compat<Self>, _) = oxicode::decode_from_slice(data)
            .map_err(|e| WireError::SerializationError(e.to_string()))?;
        Ok(compat.0)
    }

    pub fn is_critical(&self) -> bool {
        match self {
            Message::AgentMigration { .. } => true,
            Message::RoutedMessage { payload, .. } => payload.is_critical(),
            _ => false,
        }
    }

    pub fn requires_ack(&self) -> bool {
        match self {
            Message::AgentMigration { .. } | Message::Discovery { .. } => true,
            Message::RoutedMessage { payload, .. } => payload.requires_ack(),
            _ => false,
        }
    }

    /// Wrap a message in a routing envelope for multi-hop delivery
    pub fn route(self, source: [u8; 16], destination: [u8; 16]) -> Self {
        Message::RoutedMessage {
            source,
            destination,
            ttl: 16, // Maximum 16 hops
            hop_count: 0,
            payload: Box::new(self),
        }
    }

    /// Increment hop count and decrement TTL for forwarding
    pub fn forward(&mut self) -> Result<(), WireError> {
        if let Message::RoutedMessage { ttl, hop_count, .. } = self {
            if *ttl == 0 {
                return Err(WireError::TransportError("TTL expired".to_string()));
            }
            *ttl -= 1;
            *hop_count += 1;
            Ok(())
        } else {
            Err(WireError::TransportError(
                "Not a routed message".to_string(),
            ))
        }
    }

    /// Check if this message is for the given destination
    pub fn is_for(&self, node_id: &[u8; 16]) -> bool {
        match self {
            Message::RoutedMessage { destination, .. } => destination == node_id,
            _ => true, // Direct messages are always "for" the receiver
        }
    }

    /// Unwrap a routed message to get the inner payload
    pub fn unwrap_payload(self) -> Self {
        match self {
            Message::RoutedMessage { payload, .. } => *payload,
            msg => msg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ==========================================================================
    // Property-Based Tests for Message Serialization
    // ==========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Ping message serialization roundtrip
        #[test]
        fn prop_ping_roundtrip(timestamp: u64) {
            let msg = Message::Ping { timestamp };
            let serialized = msg.serialize().unwrap();
            let deserialized = Message::deserialize(&serialized).unwrap();
            if let Message::Ping { timestamp: ts } = deserialized {
                prop_assert_eq!(ts, timestamp);
            } else {
                prop_assert!(false, "Wrong message type");
            }
        }

        /// Pong message serialization roundtrip
        #[test]
        fn prop_pong_roundtrip(timestamp: u64, latency_ms: u32) {
            let msg = Message::Pong { timestamp, latency_ms };
            let serialized = msg.serialize().unwrap();
            let deserialized = Message::deserialize(&serialized).unwrap();
            if let Message::Pong { timestamp: ts, latency_ms: lat } = deserialized {
                prop_assert_eq!(ts, timestamp);
                prop_assert_eq!(lat, latency_ms);
            } else {
                prop_assert!(false, "Wrong message type");
            }
        }

        /// AgentMigration message serialization roundtrip
        #[test]
        fn prop_migration_roundtrip(
            agent_id in prop::collection::vec(any::<u8>(), 16),
            snapshot in prop::collection::vec(any::<u8>(), 0..1024),
            priority: u8
        ) {
            let agent_id_arr: [u8; 16] = agent_id.try_into().unwrap();
            let msg = Message::AgentMigration {
                agent_id: agent_id_arr,
                snapshot: snapshot.clone(),
                priority,
            };
            let serialized = msg.serialize().unwrap();
            let deserialized = Message::deserialize(&serialized).unwrap();
            if let Message::AgentMigration {
                agent_id: aid,
                snapshot: snap,
                priority: pri,
            } = deserialized
            {
                prop_assert_eq!(aid, agent_id_arr);
                prop_assert_eq!(snap, snapshot);
                prop_assert_eq!(pri, priority);
            } else {
                prop_assert!(false, "Wrong message type");
            }
        }

        /// LoadInfo message serialization roundtrip
        #[test]
        fn prop_load_info_roundtrip(
            cpu_usage in 0.0f32..1.0f32,
            memory_usage in 0.0f32..1.0f32,
            active_agents in 0usize..1000
        ) {
            let msg = Message::LoadInfo {
                cpu_usage,
                memory_usage,
                active_agents,
            };
            let serialized = msg.serialize().unwrap();
            let deserialized = Message::deserialize(&serialized).unwrap();
            if let Message::LoadInfo {
                cpu_usage: cpu,
                memory_usage: mem,
                active_agents: agents,
            } = deserialized
            {
                prop_assert!((cpu - cpu_usage).abs() < 0.0001);
                prop_assert!((mem - memory_usage).abs() < 0.0001);
                prop_assert_eq!(agents, active_agents);
            } else {
                prop_assert!(false, "Wrong message type");
            }
        }

        /// Routed message TTL and hop count are consistent
        #[test]
        fn prop_routed_message_forward_consistency(
            source in prop::collection::vec(any::<u8>(), 16),
            destination in prop::collection::vec(any::<u8>(), 16),
            num_forwards in 0usize..16,
            timestamp: u64
        ) {
            let source_arr: [u8; 16] = source.try_into().unwrap();
            let dest_arr: [u8; 16] = destination.try_into().unwrap();

            let ping = Message::Ping { timestamp };
            let mut routed = ping.route(source_arr, dest_arr);

            for _ in 0..num_forwards {
                routed.forward().unwrap();
            }

            if let Message::RoutedMessage { ttl, hop_count, .. } = routed {
                // TTL starts at 16, decrements with each forward
                prop_assert_eq!(ttl as usize, 16 - num_forwards);
                prop_assert_eq!(hop_count as usize, num_forwards);
                // Sum should always equal 16
                prop_assert_eq!(ttl as usize + hop_count as usize, 16);
            } else {
                prop_assert!(false, "Expected RoutedMessage");
            }
        }

        /// is_for correctly identifies destination
        #[test]
        fn prop_is_for_correctness(
            source in prop::collection::vec(any::<u8>(), 16),
            destination in prop::collection::vec(any::<u8>(), 16),
            other in prop::collection::vec(any::<u8>(), 16)
        ) {
            let source_arr: [u8; 16] = source.try_into().unwrap();
            let dest_arr: [u8; 16] = destination.try_into().unwrap();
            let other_arr: [u8; 16] = other.try_into().unwrap();

            let ping = Message::Ping { timestamp: 0 };
            let routed = ping.route(source_arr, dest_arr);

            prop_assert!(routed.is_for(&dest_arr));
            // Only false if other is different from destination
            if other_arr != dest_arr {
                prop_assert!(!routed.is_for(&other_arr));
            }
        }

        /// unwrap_payload returns the inner message
        #[test]
        fn prop_unwrap_payload_identity(timestamp: u64) {
            let ping = Message::Ping { timestamp };
            let routed = ping.route([1u8; 16], [2u8; 16]);
            let unwrapped = routed.unwrap_payload();

            if let Message::Ping { timestamp: ts } = unwrapped {
                prop_assert_eq!(ts, timestamp);
            } else {
                prop_assert!(false, "Expected Ping message");
            }
        }

        /// Critical messages are correctly identified
        #[test]
        fn prop_critical_migration_is_critical(
            agent_id in prop::collection::vec(any::<u8>(), 16),
            priority: u8
        ) {
            let agent_id_arr: [u8; 16] = agent_id.try_into().unwrap();
            let msg = Message::AgentMigration {
                agent_id: agent_id_arr,
                snapshot: vec![],
                priority,
            };
            prop_assert!(msg.is_critical());

            // Routed critical message is also critical
            let routed = msg.route([1u8; 16], [2u8; 16]);
            prop_assert!(routed.is_critical());
        }

        /// Non-critical messages are correctly identified
        #[test]
        fn prop_ping_not_critical(timestamp: u64) {
            let ping = Message::Ping { timestamp };
            prop_assert!(!ping.is_critical());
        }
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::Ping { timestamp: 12345 };
        let serialized = msg.serialize().unwrap();
        let deserialized = Message::deserialize(&serialized).unwrap();
        assert!(matches!(deserialized, Message::Ping { .. }));
    }

    #[test]
    fn test_migration_message() {
        let msg = Message::AgentMigration {
            agent_id: [1u8; 16],
            snapshot: vec![1, 2, 3, 4],
            priority: 10,
        };

        let serialized = msg.serialize().unwrap();
        let deserialized = Message::deserialize(&serialized).unwrap();

        if let Message::AgentMigration {
            agent_id,
            snapshot,
            priority,
        } = deserialized
        {
            assert_eq!(agent_id, [1u8; 16]);
            assert_eq!(snapshot, vec![1, 2, 3, 4]);
            assert_eq!(priority, 10);
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_critical_messages() {
        let migration = Message::AgentMigration {
            agent_id: [0u8; 16],
            snapshot: vec![],
            priority: 5,
        };
        assert!(migration.is_critical());

        let ping = Message::Ping { timestamp: 0 };
        assert!(!ping.is_critical());
    }

    #[test]
    fn test_routed_message() {
        let source = [1u8; 16];
        let destination = [2u8; 16];

        let ping = Message::Ping { timestamp: 12345 };
        let routed = ping.route(source, destination);

        if let Message::RoutedMessage {
            source: s,
            destination: d,
            ttl,
            hop_count,
            payload,
        } = routed
        {
            assert_eq!(s, source);
            assert_eq!(d, destination);
            assert_eq!(ttl, 16);
            assert_eq!(hop_count, 0);
            assert!(matches!(*payload, Message::Ping { .. }));
        } else {
            panic!("Expected RoutedMessage");
        }
    }

    #[test]
    fn test_message_forward() {
        let source = [1u8; 16];
        let destination = [2u8; 16];

        let ping = Message::Ping { timestamp: 12345 };
        let mut routed = ping.route(source, destination);

        routed.forward().unwrap();

        if let Message::RoutedMessage { ttl, hop_count, .. } = routed {
            assert_eq!(ttl, 15);
            assert_eq!(hop_count, 1);
        } else {
            panic!("Expected RoutedMessage");
        }
    }

    #[test]
    fn test_ttl_expiry() {
        let source = [1u8; 16];
        let destination = [2u8; 16];

        let ping = Message::Ping { timestamp: 12345 };
        let mut routed = ping.route(source, destination);

        // Forward 16 times to expire TTL
        for _ in 0..16 {
            routed.forward().unwrap();
        }

        // Next forward should fail
        assert!(routed.forward().is_err());
    }

    #[test]
    fn test_is_for() {
        let destination = [2u8; 16];
        let other = [3u8; 16];

        let ping = Message::Ping { timestamp: 12345 };
        let routed = ping.route([1u8; 16], destination);

        assert!(routed.is_for(&destination));
        assert!(!routed.is_for(&other));
    }

    #[test]
    fn test_unwrap_payload() {
        let ping = Message::Ping { timestamp: 12345 };
        let routed = ping.clone().route([1u8; 16], [2u8; 16]);

        let unwrapped = routed.unwrap_payload();
        assert!(matches!(unwrapped, Message::Ping { timestamp: 12345 }));
    }

    #[test]
    fn test_routed_critical_message() {
        let migration = Message::AgentMigration {
            agent_id: [0u8; 16],
            snapshot: vec![],
            priority: 5,
        };

        let routed = migration.route([1u8; 16], [2u8; 16]);
        assert!(routed.is_critical());
    }
}
