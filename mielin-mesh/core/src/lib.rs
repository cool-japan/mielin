//! MielinMesh Core - Distributed Hash Table and Routing
//!
//! Implements Kademlia-based DHT with geographic and performance awareness.

pub mod dht;
pub mod discovery;
pub mod error;
pub mod export;
pub mod gossip;
pub mod loadbalancer;
pub mod metrics;
pub mod migration;
pub mod multiregion;
pub mod multitenancy;
pub mod node;
pub mod partition;
pub mod recovery;
pub mod registry;
pub mod routing;
pub mod security;
pub mod service;
pub mod service_discovery;
pub mod shutdown;
pub mod timeout;
pub mod tracing;
pub mod version;

pub use dht::{
    CacheStats, CachedLookup, ChurnTracker, Dht, LookupCache, LookupConfig, LookupState, PeerInfo,
    RoutingReplica,
};
pub use discovery::{BootstrapNode, DiscoveryService};
pub use error::{
    BackoffStrategy, CircuitBreaker, CircuitBreakerConfig, CircuitState, MeshNetworkError,
    RetryExecutor, RetryPolicy,
};
pub use export::{JsonExporter, MigrationMetricsExport, PrometheusExporter};
pub use gossip::{
    GossipConfig, GossipError, GossipMessage, GossipRole, GossipState, HealthStatus,
    HierarchicalGossip, HierarchicalGossipConfig, HierarchicalMessage, MemberInfo, MembershipEvent,
    MembershipEventKind, ZoneId, ZoneMember, ZoneStats,
};
pub use loadbalancer::{
    EndpointStats, HealthCheckConfig, LoadBalancer, LoadBalancerError, LoadBalancingAlgorithm,
    PoolStats, ServicePool,
};
pub use metrics::{
    ConnectionState, Counter, DhtMetrics, DhtMetricsSummary, Gauge, GossipMetrics,
    GossipMetricsSummary, Histogram, HistogramStats, MessageType, MessageTypeThroughput,
    MetricsRegistry, MetricsSummary, MigrationResult, MigrationSuccessMetrics,
    MigrationSuccessSummary, NodeMetrics, NodeMetricsSummary, OperationLatencyMetrics,
    OperationLatencyStats, OperationLatencySummary, OperationTimer, OperationType,
    PeerConnectionMetrics, PeerConnectionState, PeerConnectionSummary, RateTracker,
    ThroughputMetrics, ThroughputSummary,
};
pub use multiregion::{
    ConsistencyLevel, FailoverCoordinator, GeoLocation, MultiRegionError, RegionHealth, RegionId,
    RegionInfo, RegionTopology, ReplicatedAgent, ReplicationManager, ReplicationPolicy,
};
pub use multitenancy::{
    AuditAction, AuditConfig, AuditEntry, AuditLog, AuditLogStats, AuditOutcome,
    CrossTenantPermission, MultiTenancyError, Namespace, NamespaceConfig, NamespaceId,
    ResourceQuota, ResourceUsage, ResourceUsageSnapshot, RoutingPolicy, Tenant, TenantId,
    TenantManager, TenantManagerStats, TenantStatus,
};
pub use node::{Node, NodeId, NodeRole};
pub use partition::{
    ConsistentHashRing, PartitionCause, PartitionDetector, PartitionError, PartitionEvent,
    PartitionInfo, PartitionState, QuorumDecision,
};
pub use recovery::{ConnectionRecovery, DegradationManager, RecoveryError, StateReconciler};
pub use registry::{AgentId, AgentRegistry, QueryOptions, RegistryShard, ShardedRegistry};
pub use security::{
    AclEffect, AclPolicy, AclResource, AclRule, AclSubject, CertificateData, EncryptedMessage,
    GossipEncryption, GossipKey, IdentityVerifier, KeyAlgorithm, KeyExchange, KeyExchangeState,
    MtlsConfig, MtlsConfigBuilder, NodeIdentity, Nonce, Permission, PublicKey, SecurityError,
    SecurityResult, Signature, TlsVersion,
};
pub use service::{MeshConfig, MeshError, MeshService};
pub use service_discovery::{
    ServiceDiscovery, ServiceDiscoveryError, ServiceEndpoint, ServiceEvent, ServiceHealth,
    ServiceQuery, ServiceRegistration,
};
pub use shutdown::{
    ComponentState, ShutdownCoordinator, ShutdownError, ShutdownHandler, ShutdownPriority,
    ShutdownSignal, ShutdownStats,
};
pub use timeout::{
    OperationStats, ServiceTimeoutPolicy, ServiceTimeoutStats, TimeoutConfig, TimeoutError,
    TimeoutManager, TimeoutOperation,
};
pub use tracing::{
    export_trace_json, export_trace_otel, AttributeValue, CollectedTrace, GossipHop, GossipTrace,
    MigrationPhaseSpan, MigrationTrace, NodeSpan, OTelSpan, RequestTrace, Span, SpanBuilder,
    SpanEvent, SpanId, SpanKind, SpanLink, SpanStatus, TraceCollector, TraceCollectorConfig,
    TraceCollectorStats, TraceContext, TraceFlags, TraceId,
};
pub use version::{
    ConflictResolution, ConflictResolver, Dot, DotContext, Ordering, VersionVector, VersionedMap,
    VersionedValue,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_generation() {
        let node = Node::new(NodeRole::Relay);
        assert!(!node.id().is_nil());
    }
}
