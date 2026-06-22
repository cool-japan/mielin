//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::NodeId;

/// Histogram statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramStats {
    pub count: u64,
    pub sum: u64,
    pub mean: u64,
    pub buckets: Vec<u64>,
    pub bucket_counts: Vec<u64>,
}
impl HistogramStats {
    /// Calculate percentile (approximate)
    pub fn percentile(&self, p: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let target = (self.count as f64 * p / 100.0).ceil() as u64;
        let mut cumulative = 0u64;
        for (i, &count) in self.bucket_counts.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return self.buckets[i];
            }
        }
        *self.buckets.last().unwrap_or(&0)
    }
}
/// Gauge for values that can go up and down
#[derive(Debug, Default)]
pub struct Gauge {
    value: AtomicU64,
}
impl Gauge {
    /// Create a new gauge starting at 0
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }
    /// Set the gauge value
    pub fn set(&self, value: u64) {
        self.value.store(value, Ordering::Relaxed);
    }
    /// Increment the gauge by 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }
    /// Decrement the gauge by 1
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }
    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}
/// Per-node metrics
#[derive(Debug)]
pub struct NodeMetrics {
    /// Node identifier
    pub node_id: NodeId,
    /// Messages sent
    pub messages_sent: Counter,
    /// Messages received
    pub messages_received: Counter,
    /// Bytes sent
    pub bytes_sent: Counter,
    /// Bytes received
    pub bytes_received: Counter,
    /// Active connections
    pub active_connections: Gauge,
    /// Message latency histogram (microseconds)
    pub message_latency: Histogram,
    /// Message size histogram (bytes)
    pub message_size: Histogram,
    /// Failed operations
    pub failures: Counter,
    /// Last activity time
    pub last_activity: RwLock<SystemTime>,
}
impl NodeMetrics {
    /// Create new node metrics
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            messages_sent: Counter::new(),
            messages_received: Counter::new(),
            bytes_sent: Counter::new(),
            bytes_received: Counter::new(),
            active_connections: Gauge::new(),
            message_latency: Histogram::new_latency(),
            message_size: Histogram::new_size(),
            failures: Counter::new(),
            last_activity: RwLock::new(SystemTime::now()),
        }
    }
    /// Record a sent message
    pub async fn record_send(&self, bytes: u64, latency_us: u64) {
        self.messages_sent.inc();
        self.bytes_sent.add(bytes);
        self.message_latency.observe(latency_us);
        self.message_size.observe(bytes);
        *self.last_activity.write().await = SystemTime::now();
    }
    /// Record a received message
    pub async fn record_receive(&self, bytes: u64) {
        self.messages_received.inc();
        self.bytes_received.add(bytes);
        self.message_size.observe(bytes);
        *self.last_activity.write().await = SystemTime::now();
    }
    /// Get summary statistics
    pub async fn summary(&self) -> NodeMetricsSummary {
        let latency_stats = self.message_latency.stats();
        NodeMetricsSummary {
            node_id: self.node_id,
            messages_sent: self.messages_sent.get(),
            messages_received: self.messages_received.get(),
            bytes_sent: self.bytes_sent.get(),
            bytes_received: self.bytes_received.get(),
            active_connections: self.active_connections.get(),
            avg_latency_us: latency_stats.mean,
            p99_latency_us: latency_stats.percentile(99.0),
            failures: self.failures.get(),
            last_activity: *self.last_activity.read().await,
        }
    }
}
/// DHT metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtMetricsSummary {
    pub gets: u64,
    pub puts: u64,
    pub lookups: u64,
    pub lookup_success_rate: f64,
    pub cache_hit_rate: f64,
    pub routing_table_size: u64,
    pub avg_lookup_latency_us: u64,
    pub p95_lookup_latency_us: u64,
}
/// Central metrics registry
pub struct MetricsRegistry {
    /// Local node metrics
    local_metrics: Arc<NodeMetrics>,
    /// Peer node metrics
    peer_metrics: Arc<RwLock<HashMap<NodeId, Arc<NodeMetrics>>>>,
    /// Gossip protocol metrics
    gossip_metrics: Arc<GossipMetrics>,
    /// DHT metrics
    dht_metrics: Arc<DhtMetrics>,
    /// Message rate tracker
    message_rate: Arc<RateTracker>,
    /// Registry creation time
    created_at: Instant,
}
impl MetricsRegistry {
    /// Create a new metrics registry
    pub fn new(local_node_id: NodeId) -> Self {
        Self {
            local_metrics: Arc::new(NodeMetrics::new(local_node_id)),
            peer_metrics: Arc::new(RwLock::new(HashMap::new())),
            gossip_metrics: Arc::new(GossipMetrics::new()),
            dht_metrics: Arc::new(DhtMetrics::new()),
            message_rate: Arc::new(RateTracker::new(0.3)),
            created_at: Instant::now(),
        }
    }
    /// Get local node metrics
    pub fn local(&self) -> &Arc<NodeMetrics> {
        &self.local_metrics
    }
    /// Get gossip metrics
    pub fn gossip(&self) -> &Arc<GossipMetrics> {
        &self.gossip_metrics
    }
    /// Get DHT metrics
    pub fn dht(&self) -> &Arc<DhtMetrics> {
        &self.dht_metrics
    }
    /// Get or create peer metrics
    pub async fn peer(&self, node_id: NodeId) -> Arc<NodeMetrics> {
        let peers = self.peer_metrics.read().await;
        if let Some(metrics) = peers.get(&node_id) {
            return Arc::clone(metrics);
        }
        drop(peers);
        let mut peers = self.peer_metrics.write().await;
        peers
            .entry(node_id)
            .or_insert_with(|| Arc::new(NodeMetrics::new(node_id)))
            .clone()
    }
    /// Record a message (updates rate tracker)
    pub fn record_message(&self) {
        self.message_rate.record();
    }
    /// Get current message rate
    pub fn message_rate(&self) -> u64 {
        self.message_rate.rate()
    }
    /// Update rate tracker (call periodically)
    pub async fn update_rates(&self) {
        self.message_rate.update().await;
    }
    /// Get uptime duration
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }
    /// Get full metrics summary
    pub async fn summary(&self) -> MetricsSummary {
        let peers = self.peer_metrics.read().await;
        let mut peer_summaries = Vec::with_capacity(peers.len());
        for metrics in peers.values() {
            peer_summaries.push(metrics.summary().await);
        }
        MetricsSummary {
            uptime_secs: self.uptime().as_secs(),
            local: self.local_metrics.summary().await,
            peers: peer_summaries,
            gossip: self.gossip_metrics.summary(),
            dht: self.dht_metrics.summary(),
            message_rate: self.message_rate(),
        }
    }
    /// Remove stale peer metrics (no activity for given duration)
    pub async fn cleanup_stale_peers(&self, max_age: Duration) {
        let now = SystemTime::now();
        let mut peers = self.peer_metrics.write().await;
        let stale: Vec<_> = {
            let mut stale = Vec::new();
            for (id, metrics) in peers.iter() {
                let last = *metrics.last_activity.read().await;
                if let Ok(elapsed) = now.duration_since(last) {
                    if elapsed > max_age {
                        stale.push(*id);
                    }
                }
            }
            stale
        };
        for id in stale {
            peers.remove(&id);
        }
    }
    /// Reset all metrics
    pub async fn reset(&self) {
        self.local_metrics.messages_sent.reset();
        self.local_metrics.messages_received.reset();
        self.local_metrics.bytes_sent.reset();
        self.local_metrics.bytes_received.reset();
        self.local_metrics.failures.reset();
        self.local_metrics.message_latency.reset();
        self.local_metrics.message_size.reset();
        self.peer_metrics.write().await.clear();
        self.gossip_metrics.heartbeats_sent.reset();
        self.gossip_metrics.heartbeats_received.reset();
        self.gossip_metrics.membership_updates.reset();
        self.gossip_metrics.state_syncs.reset();
        self.gossip_metrics.failed_rounds.reset();
        self.gossip_metrics.round_duration.reset();
        self.dht_metrics.gets.reset();
        self.dht_metrics.puts.reset();
        self.dht_metrics.lookups.reset();
        self.dht_metrics.lookup_successes.reset();
        self.dht_metrics.cache_hits.reset();
        self.dht_metrics.cache_misses.reset();
        self.dht_metrics.lookup_latency.reset();
    }
}
/// Operation type for latency tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationType {
    /// DHT get operation
    DhtGet,
    /// DHT put operation
    DhtPut,
    /// DHT lookup (routing)
    DhtLookup,
    /// Gossip round
    GossipRound,
    /// State sync
    StateSync,
    /// Agent lookup in registry
    AgentLookup,
    /// Agent registration
    AgentRegister,
    /// Migration prepare
    MigrationPrepare,
    /// Migration transfer
    MigrationTransfer,
    /// Migration complete
    MigrationComplete,
    /// Peer connection
    PeerConnect,
    /// Message send
    MessageSend,
    /// Message receive processing
    MessageProcess,
}
/// Per-operation latency stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLatencyStats {
    pub op_type: OperationType,
    pub count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub avg_latency_us: u64,
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub p99_latency_us: u64,
}
/// Message throughput metrics
#[derive(Debug)]
pub struct ThroughputMetrics {
    /// Per-type metrics
    type_metrics: RwLock<HashMap<MessageType, MessageTypeMetrics>>,
    /// Total bytes sent
    pub(crate) total_bytes_sent: Counter,
    /// Total bytes received
    pub(crate) total_bytes_received: Counter,
    /// Total messages sent
    pub(crate) total_messages_sent: Counter,
    /// Total messages received
    pub(crate) total_messages_received: Counter,
    /// Bandwidth limit (bytes/sec, 0 = unlimited)
    bandwidth_limit: AtomicU64,
    /// Current bandwidth usage (bytes/sec)
    current_bandwidth: AtomicU64,
    /// Bandwidth utilization histogram (percentage)
    bandwidth_utilization: Histogram,
    /// Window start for bandwidth calculation
    window_start: RwLock<Instant>,
    /// Bytes in current window
    window_bytes: AtomicU64,
}
impl ThroughputMetrics {
    /// Create new throughput metrics
    pub fn new() -> Self {
        Self {
            type_metrics: RwLock::new(HashMap::new()),
            total_bytes_sent: Counter::new(),
            total_bytes_received: Counter::new(),
            total_messages_sent: Counter::new(),
            total_messages_received: Counter::new(),
            bandwidth_limit: AtomicU64::new(0),
            current_bandwidth: AtomicU64::new(0),
            bandwidth_utilization: Histogram::new(vec![10, 25, 50, 75, 90, 95, 99, 100]),
            window_start: RwLock::new(Instant::now()),
            window_bytes: AtomicU64::new(0),
        }
    }
    /// Set bandwidth limit (bytes/sec)
    pub fn set_bandwidth_limit(&self, limit: u64) {
        self.bandwidth_limit.store(limit, Ordering::Relaxed);
    }
    /// Get bandwidth limit
    pub fn bandwidth_limit(&self) -> u64 {
        self.bandwidth_limit.load(Ordering::Relaxed)
    }
    /// Record sent message
    pub async fn record_send(&self, msg_type: MessageType, bytes: u64) {
        self.total_bytes_sent.add(bytes);
        self.total_messages_sent.inc();
        self.window_bytes.fetch_add(bytes, Ordering::Relaxed);
        let mut metrics = self.type_metrics.write().await;
        let type_metrics = metrics
            .entry(msg_type)
            .or_insert_with(MessageTypeMetrics::new);
        type_metrics.sent_count.inc();
        type_metrics.bytes_sent.add(bytes);
        type_metrics.send_rate.record();
    }
    /// Record received message
    pub async fn record_receive(&self, msg_type: MessageType, bytes: u64) {
        self.total_bytes_received.add(bytes);
        self.total_messages_received.inc();
        self.window_bytes.fetch_add(bytes, Ordering::Relaxed);
        let mut metrics = self.type_metrics.write().await;
        let type_metrics = metrics
            .entry(msg_type)
            .or_insert_with(MessageTypeMetrics::new);
        type_metrics.received_count.inc();
        type_metrics.bytes_received.add(bytes);
        type_metrics.receive_rate.record();
    }
    /// Update bandwidth calculation (call periodically)
    pub async fn update_bandwidth(&self) {
        let mut window_start = self.window_start.write().await;
        let elapsed = window_start.elapsed();
        *window_start = Instant::now();
        drop(window_start);
        let bytes = self.window_bytes.swap(0, Ordering::Relaxed);
        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs > 0.0 {
            let bandwidth = (bytes as f64 / elapsed_secs) as u64;
            self.current_bandwidth.store(bandwidth, Ordering::Relaxed);
            let limit = self.bandwidth_limit.load(Ordering::Relaxed);
            if limit > 0 {
                let utilization = ((bandwidth as f64 / limit as f64) * 100.0) as u64;
                self.bandwidth_utilization.observe(utilization.min(100));
            }
        }
        let mut metrics = self.type_metrics.write().await;
        for type_metrics in metrics.values_mut() {
            type_metrics.send_rate.update().await;
            type_metrics.receive_rate.update().await;
        }
    }
    /// Get current bandwidth (bytes/sec)
    pub fn current_bandwidth(&self) -> u64 {
        self.current_bandwidth.load(Ordering::Relaxed)
    }
    /// Get throughput for specific message type
    pub async fn type_throughput(&self, msg_type: MessageType) -> Option<MessageTypeThroughput> {
        let metrics = self.type_metrics.read().await;
        metrics.get(&msg_type).map(|m| MessageTypeThroughput {
            msg_type,
            sent_count: m.sent_count.get(),
            received_count: m.received_count.get(),
            bytes_sent: m.bytes_sent.get(),
            bytes_received: m.bytes_received.get(),
            send_rate: m.send_rate.rate(),
            receive_rate: m.receive_rate.rate(),
        })
    }
    /// Get summary
    pub async fn summary(&self) -> ThroughputSummary {
        let metrics = self.type_metrics.read().await;
        let mut per_type = Vec::with_capacity(metrics.len());
        for (msg_type, m) in metrics.iter() {
            per_type.push(MessageTypeThroughput {
                msg_type: *msg_type,
                sent_count: m.sent_count.get(),
                received_count: m.received_count.get(),
                bytes_sent: m.bytes_sent.get(),
                bytes_received: m.bytes_received.get(),
                send_rate: m.send_rate.rate(),
                receive_rate: m.receive_rate.rate(),
            });
        }
        let util_stats = self.bandwidth_utilization.stats();
        ThroughputSummary {
            total_bytes_sent: self.total_bytes_sent.get(),
            total_bytes_received: self.total_bytes_received.get(),
            total_messages_sent: self.total_messages_sent.get(),
            total_messages_received: self.total_messages_received.get(),
            current_bandwidth: self.current_bandwidth.load(Ordering::Relaxed),
            bandwidth_limit: self.bandwidth_limit.load(Ordering::Relaxed),
            avg_bandwidth_utilization: util_stats.mean,
            per_type,
        }
    }
}
/// Operation latency metrics
#[derive(Debug)]
pub struct OperationLatencyMetrics {
    /// Per-operation metrics
    operations: RwLock<HashMap<OperationType, OperationMetrics>>,
    /// Overall latency (all operations)
    overall_latency: Histogram,
    /// Total operations
    pub(crate) total_operations: Counter,
    /// SLA threshold (microseconds)
    sla_threshold_us: AtomicU64,
    /// SLA violations
    pub(crate) sla_violations: Counter,
}
impl OperationLatencyMetrics {
    /// Create new operation latency metrics
    pub fn new() -> Self {
        Self {
            operations: RwLock::new(HashMap::new()),
            overall_latency: Histogram::new_latency(),
            total_operations: Counter::new(),
            sla_threshold_us: AtomicU64::new(100_000),
            sla_violations: Counter::new(),
        }
    }
    /// Set SLA threshold (microseconds)
    pub fn set_sla_threshold(&self, threshold_us: u64) {
        self.sla_threshold_us.store(threshold_us, Ordering::Relaxed);
    }
    /// Record operation completion
    pub async fn record(&self, op_type: OperationType, latency_us: u64, success: bool) {
        self.total_operations.inc();
        self.overall_latency.observe(latency_us);
        let threshold = self.sla_threshold_us.load(Ordering::Relaxed);
        if latency_us > threshold {
            self.sla_violations.inc();
        }
        let mut operations = self.operations.write().await;
        let op_metrics = operations
            .entry(op_type)
            .or_insert_with(OperationMetrics::new);
        op_metrics.count.inc();
        op_metrics.latency.observe(latency_us);
        if success {
            op_metrics.success_count.inc();
        } else {
            op_metrics.failure_count.inc();
        }
    }
    /// Record operation with timer
    pub fn start_timer(&self) -> OperationTimer {
        OperationTimer {
            start: Instant::now(),
        }
    }
    /// Get latency stats for specific operation
    pub async fn operation_stats(&self, op_type: OperationType) -> Option<OperationLatencyStats> {
        let operations = self.operations.read().await;
        operations.get(&op_type).map(|m| {
            let stats = m.latency.stats();
            OperationLatencyStats {
                op_type,
                count: m.count.get(),
                success_count: m.success_count.get(),
                failure_count: m.failure_count.get(),
                avg_latency_us: stats.mean,
                p50_latency_us: stats.percentile(50.0),
                p95_latency_us: stats.percentile(95.0),
                p99_latency_us: stats.percentile(99.0),
            }
        })
    }
    /// Get SLA compliance rate (0.0 - 1.0)
    pub fn sla_compliance_rate(&self) -> f64 {
        let total = self.total_operations.get();
        let violations = self.sla_violations.get();
        if total == 0 {
            1.0
        } else {
            1.0 - (violations as f64 / total as f64)
        }
    }
    /// Get summary
    pub async fn summary(&self) -> OperationLatencySummary {
        let operations = self.operations.read().await;
        let mut per_operation = Vec::with_capacity(operations.len());
        for (op_type, m) in operations.iter() {
            let stats = m.latency.stats();
            per_operation.push(OperationLatencyStats {
                op_type: *op_type,
                count: m.count.get(),
                success_count: m.success_count.get(),
                failure_count: m.failure_count.get(),
                avg_latency_us: stats.mean,
                p50_latency_us: stats.percentile(50.0),
                p95_latency_us: stats.percentile(95.0),
                p99_latency_us: stats.percentile(99.0),
            });
        }
        let overall_stats = self.overall_latency.stats();
        OperationLatencySummary {
            total_operations: self.total_operations.get(),
            sla_threshold_us: self.sla_threshold_us.load(Ordering::Relaxed),
            sla_violations: self.sla_violations.get(),
            sla_compliance_rate: self.sla_compliance_rate(),
            overall_avg_latency_us: overall_stats.mean,
            overall_p99_latency_us: overall_stats.percentile(99.0),
            per_operation,
        }
    }
}
/// Metrics for peer connections
#[derive(Debug)]
pub struct PeerConnectionMetrics {
    /// Total connection attempts
    pub connection_attempts: Counter,
    /// Successful connections
    pub connections_established: Counter,
    /// Connection failures
    pub connection_failures: Counter,
    /// Disconnections (clean)
    pub disconnections: Counter,
    /// Reconnection attempts
    pub reconnect_attempts: Counter,
    /// Successful reconnections
    pub reconnections_succeeded: Counter,
    /// Current connected peers
    pub connected_peers: Gauge,
    /// Current connecting peers
    pub connecting_peers: Gauge,
    /// Connection duration histogram (seconds)
    pub connection_duration: Histogram,
    /// Time to establish connection (milliseconds)
    pub connection_time: Histogram,
    /// Per-peer connection state
    peer_states: RwLock<HashMap<NodeId, PeerConnectionState>>,
}
impl PeerConnectionMetrics {
    /// Create new peer connection metrics
    pub fn new() -> Self {
        Self {
            connection_attempts: Counter::new(),
            connections_established: Counter::new(),
            connection_failures: Counter::new(),
            disconnections: Counter::new(),
            reconnect_attempts: Counter::new(),
            reconnections_succeeded: Counter::new(),
            connected_peers: Gauge::new(),
            connecting_peers: Gauge::new(),
            connection_duration: Histogram::new(vec![1, 5, 10, 30, 60, 300, 600, 1800, 3600]),
            connection_time: Histogram::new(vec![10, 50, 100, 250, 500, 1000, 2000, 5000]),
            peer_states: RwLock::new(HashMap::new()),
        }
    }
    /// Record connection attempt
    pub async fn record_connect_attempt(&self, peer_id: NodeId) {
        self.connection_attempts.inc();
        self.connecting_peers.inc();
        let mut states = self.peer_states.write().await;
        states
            .entry(peer_id)
            .or_insert_with(|| PeerConnectionState::new(peer_id))
            .connecting();
    }
    /// Record successful connection
    pub async fn record_connected(&self, peer_id: NodeId, connection_time_ms: u64) {
        self.connections_established.inc();
        self.connecting_peers.dec();
        self.connected_peers.inc();
        self.connection_time.observe(connection_time_ms);
        let mut states = self.peer_states.write().await;
        states
            .entry(peer_id)
            .or_insert_with(|| PeerConnectionState::new(peer_id))
            .connected();
    }
    /// Record connection failure
    pub async fn record_connection_failed(&self, peer_id: NodeId) {
        self.connection_failures.inc();
        self.connecting_peers.dec();
        let mut states = self.peer_states.write().await;
        states
            .entry(peer_id)
            .or_insert_with(|| PeerConnectionState::new(peer_id))
            .failed();
    }
    /// Record disconnection
    pub async fn record_disconnected(&self, peer_id: NodeId) {
        self.disconnections.inc();
        self.connected_peers.dec();
        let mut states = self.peer_states.write().await;
        if let Some(state) = states.get_mut(&peer_id) {
            if let Some(connected_at) = state.connected_at {
                self.connection_duration
                    .observe(connected_at.elapsed().as_secs());
            }
            state.disconnected();
        }
    }
    /// Record reconnection attempt
    pub async fn record_reconnect_attempt(&self, peer_id: NodeId) {
        self.reconnect_attempts.inc();
        self.connecting_peers.inc();
        let mut states = self.peer_states.write().await;
        if let Some(state) = states.get_mut(&peer_id) {
            state.state = ConnectionState::Reconnecting;
        }
    }
    /// Record successful reconnection
    pub async fn record_reconnected(&self, peer_id: NodeId, connection_time_ms: u64) {
        self.reconnections_succeeded.inc();
        self.connecting_peers.dec();
        self.connected_peers.inc();
        self.connection_time.observe(connection_time_ms);
        let mut states = self.peer_states.write().await;
        states
            .entry(peer_id)
            .or_insert_with(|| PeerConnectionState::new(peer_id))
            .connected();
    }
    /// Get peer connection state
    pub async fn peer_state(&self, peer_id: &NodeId) -> Option<PeerConnectionState> {
        self.peer_states.read().await.get(peer_id).cloned()
    }
    /// Get all connected peers
    pub async fn connected_peer_ids(&self) -> Vec<NodeId> {
        self.peer_states
            .read()
            .await
            .iter()
            .filter(|(_, s)| s.state == ConnectionState::Connected)
            .map(|(id, _)| *id)
            .collect()
    }
    /// Get summary
    pub fn summary(&self) -> PeerConnectionSummary {
        let duration_stats = self.connection_duration.stats();
        let time_stats = self.connection_time.stats();
        PeerConnectionSummary {
            connection_attempts: self.connection_attempts.get(),
            connections_established: self.connections_established.get(),
            connection_failures: self.connection_failures.get(),
            disconnections: self.disconnections.get(),
            reconnect_attempts: self.reconnect_attempts.get(),
            reconnections_succeeded: self.reconnections_succeeded.get(),
            connected_peers: self.connected_peers.get(),
            avg_connection_duration_secs: duration_stats.mean,
            avg_connection_time_ms: time_stats.mean,
            p99_connection_time_ms: time_stats.percentile(99.0),
        }
    }
}
/// Individual migration result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    pub agent_id: [u8; 16],
    pub source_node: NodeId,
    pub target_node: NodeId,
    pub strategy: String,
    pub success: bool,
    pub duration_ms: u64,
    pub downtime_ms: u64,
    pub bytes_transferred: u64,
    pub timestamp: SystemTime,
    pub error: Option<String>,
}
/// Peer connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and healthy
    Connected,
    /// Connection degraded (high latency, packet loss)
    Degraded,
    /// Reconnecting after failure
    Reconnecting,
}
/// Full metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub uptime_secs: u64,
    pub local: NodeMetricsSummary,
    pub peers: Vec<NodeMetricsSummary>,
    pub gossip: GossipMetricsSummary,
    pub dht: DhtMetricsSummary,
    pub message_rate: u64,
}
/// Exponential moving average for rate tracking
#[derive(Debug)]
pub struct RateTracker {
    /// Current rate (events per second)
    rate: AtomicU64,
    /// Event count in current window
    window_count: AtomicU64,
    /// Last update time
    last_update: RwLock<Instant>,
    /// Smoothing factor (0-1, higher = more responsive)
    alpha: f64,
}
impl RateTracker {
    /// Create a new rate tracker with given smoothing factor
    pub fn new(alpha: f64) -> Self {
        Self {
            rate: AtomicU64::new(0),
            window_count: AtomicU64::new(0),
            last_update: RwLock::new(Instant::now()),
            alpha: alpha.clamp(0.0, 1.0),
        }
    }
    /// Record an event
    pub fn record(&self) {
        self.window_count.fetch_add(1, Ordering::Relaxed);
    }
    /// Update the rate (call periodically)
    pub async fn update(&self) {
        let mut last = self.last_update.write().await;
        let elapsed = last.elapsed();
        *last = Instant::now();
        drop(last);
        let count = self.window_count.swap(0, Ordering::Relaxed);
        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs > 0.0 {
            let new_rate = (count as f64 / elapsed_secs) as u64;
            let current = self.rate.load(Ordering::Relaxed);
            let smoothed =
                ((self.alpha * new_rate as f64) + ((1.0 - self.alpha) * current as f64)) as u64;
            self.rate.store(smoothed, Ordering::Relaxed);
        }
    }
    /// Get the current rate (events per second)
    pub fn rate(&self) -> u64 {
        self.rate.load(Ordering::Relaxed)
    }
}
/// Gossip protocol metrics
#[derive(Debug, Default)]
pub struct GossipMetrics {
    /// Heartbeats sent
    pub heartbeats_sent: Counter,
    /// Heartbeats received
    pub heartbeats_received: Counter,
    /// Membership updates
    pub membership_updates: Counter,
    /// State syncs completed
    pub state_syncs: Counter,
    /// Failed gossip rounds
    pub failed_rounds: Counter,
    /// Current membership count
    pub member_count: Gauge,
    /// Suspect nodes
    pub suspect_count: Gauge,
    /// Dead nodes
    pub dead_count: Gauge,
    /// Gossip round duration (microseconds)
    pub round_duration: Histogram,
}
impl GossipMetrics {
    /// Create new gossip metrics
    pub fn new() -> Self {
        Self {
            heartbeats_sent: Counter::new(),
            heartbeats_received: Counter::new(),
            membership_updates: Counter::new(),
            state_syncs: Counter::new(),
            failed_rounds: Counter::new(),
            member_count: Gauge::new(),
            suspect_count: Gauge::new(),
            dead_count: Gauge::new(),
            round_duration: Histogram::new_latency(),
        }
    }
    /// Get summary
    pub fn summary(&self) -> GossipMetricsSummary {
        let round_stats = self.round_duration.stats();
        GossipMetricsSummary {
            heartbeats_sent: self.heartbeats_sent.get(),
            heartbeats_received: self.heartbeats_received.get(),
            membership_updates: self.membership_updates.get(),
            state_syncs: self.state_syncs.get(),
            failed_rounds: self.failed_rounds.get(),
            member_count: self.member_count.get(),
            suspect_count: self.suspect_count.get(),
            dead_count: self.dead_count.get(),
            avg_round_duration_us: round_stats.mean,
        }
    }
}
/// Aggregate migration metrics for success rate tracking
#[derive(Debug)]
pub struct MigrationSuccessMetrics {
    /// Total migrations attempted
    pub total_migrations: Counter,
    /// Successful migrations
    pub successful_migrations: Counter,
    /// Failed migrations
    pub failed_migrations: Counter,
    /// Cancelled migrations
    pub cancelled_migrations: Counter,
    /// Migrations by strategy
    pub(crate) precopy_count: Counter,
    pub(crate) postcopy_count: Counter,
    pub(crate) hybrid_count: Counter,
    /// Migration duration histogram (milliseconds)
    pub migration_duration: Histogram,
    /// Migration downtime histogram (milliseconds)
    pub migration_downtime: Histogram,
    /// Data transferred histogram (bytes)
    pub data_transferred: Histogram,
    /// Active migrations
    pub active_migrations: Gauge,
    /// Recent migration results
    recent_results: RwLock<Vec<MigrationResult>>,
    /// Max recent results to keep
    max_recent: usize,
}
impl MigrationSuccessMetrics {
    /// Create new migration metrics
    pub fn new() -> Self {
        Self {
            total_migrations: Counter::new(),
            successful_migrations: Counter::new(),
            failed_migrations: Counter::new(),
            cancelled_migrations: Counter::new(),
            precopy_count: Counter::new(),
            postcopy_count: Counter::new(),
            hybrid_count: Counter::new(),
            migration_duration: Histogram::new(vec![
                100, 500, 1000, 2000, 5000, 10000, 30000, 60000,
            ]),
            migration_downtime: Histogram::new(vec![10, 50, 100, 250, 500, 1000, 2000, 5000]),
            data_transferred: Histogram::new_size(),
            active_migrations: Gauge::new(),
            recent_results: RwLock::new(Vec::new()),
            max_recent: 100,
        }
    }
    /// Record migration start
    pub fn record_start(&self, strategy: &str) {
        self.total_migrations.inc();
        self.active_migrations.inc();
        match strategy {
            "precopy" | "PreCopy" => self.precopy_count.inc(),
            "postcopy" | "PostCopy" => self.postcopy_count.inc(),
            "hybrid" | "Hybrid" => self.hybrid_count.inc(),
            _ => {}
        }
    }
    /// Record migration completion
    pub async fn record_complete(&self, result: MigrationResult) {
        self.active_migrations.dec();
        if result.success {
            self.successful_migrations.inc();
        } else {
            self.failed_migrations.inc();
        }
        self.migration_duration.observe(result.duration_ms);
        self.migration_downtime.observe(result.downtime_ms);
        self.data_transferred.observe(result.bytes_transferred);
        let mut recent = self.recent_results.write().await;
        recent.push(result);
        if recent.len() > self.max_recent {
            recent.remove(0);
        }
    }
    /// Record migration cancellation
    pub fn record_cancelled(&self) {
        self.active_migrations.dec();
        self.cancelled_migrations.inc();
    }
    /// Get success rate (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.total_migrations.get();
        let success = self.successful_migrations.get();
        if total == 0 {
            0.0
        } else {
            success as f64 / total as f64
        }
    }
    /// Get recent migration results
    pub async fn recent_results(&self) -> Vec<MigrationResult> {
        self.recent_results.read().await.clone()
    }
    /// Get summary
    pub fn summary(&self) -> MigrationSuccessSummary {
        let duration_stats = self.migration_duration.stats();
        let downtime_stats = self.migration_downtime.stats();
        MigrationSuccessSummary {
            total_migrations: self.total_migrations.get(),
            successful_migrations: self.successful_migrations.get(),
            failed_migrations: self.failed_migrations.get(),
            cancelled_migrations: self.cancelled_migrations.get(),
            success_rate: self.success_rate(),
            active_migrations: self.active_migrations.get(),
            precopy_count: self.precopy_count.get(),
            postcopy_count: self.postcopy_count.get(),
            hybrid_count: self.hybrid_count.get(),
            avg_duration_ms: duration_stats.mean,
            p99_duration_ms: duration_stats.percentile(99.0),
            avg_downtime_ms: downtime_stats.mean,
            p99_downtime_ms: downtime_stats.percentile(99.0),
        }
    }
}
/// Histogram for tracking distributions
#[derive(Debug)]
pub struct Histogram {
    /// Bucket boundaries (upper bounds)
    pub(crate) buckets: Vec<u64>,
    /// Count per bucket
    counts: Vec<AtomicU64>,
    /// Sum of all observations
    sum: AtomicU64,
    /// Count of all observations
    count: AtomicU64,
}
impl Histogram {
    /// Create a histogram with default latency buckets (in microseconds)
    pub fn new_latency() -> Self {
        Self::new(vec![
            100, 500, 1_000, 5_000, 10_000, 50_000, 100_000, 500_000, 1_000_000,
        ])
    }
    /// Create a histogram with default size buckets (in bytes)
    pub fn new_size() -> Self {
        Self::new(vec![
            64, 256, 1024, 4096, 16384, 65536, 262144, 1048576, 16777216,
        ])
    }
    /// Create a histogram with custom bucket boundaries
    pub fn new(buckets: Vec<u64>) -> Self {
        let counts = buckets.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            buckets,
            counts,
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }
    /// Observe a value
    pub fn observe(&self, value: u64) {
        self.sum.fetch_add(value, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        for (i, &bound) in self.buckets.iter().enumerate() {
            if value <= bound {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        if let Some(last) = self.counts.last() {
            last.fetch_add(1, Ordering::Relaxed);
        }
    }
    /// Get histogram statistics
    pub fn stats(&self) -> HistogramStats {
        let count = self.count.load(Ordering::Relaxed);
        let sum = self.sum.load(Ordering::Relaxed);
        let mean = sum.checked_div(count).unwrap_or(0);
        let bucket_counts: Vec<_> = self
            .counts
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        HistogramStats {
            count,
            sum,
            mean,
            buckets: self.buckets.clone(),
            bucket_counts,
        }
    }
    /// Reset the histogram
    pub fn reset(&self) {
        self.sum.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
        for c in &self.counts {
            c.store(0, Ordering::Relaxed);
        }
    }
}
/// Node metrics summary snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetricsSummary {
    pub node_id: NodeId,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub active_connections: u64,
    pub avg_latency_us: u64,
    pub p99_latency_us: u64,
    pub failures: u64,
    pub last_activity: SystemTime,
}
/// Peer connection metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConnectionSummary {
    pub connection_attempts: u64,
    pub connections_established: u64,
    pub connection_failures: u64,
    pub disconnections: u64,
    pub reconnect_attempts: u64,
    pub reconnections_succeeded: u64,
    pub connected_peers: u64,
    pub avg_connection_duration_secs: u64,
    pub avg_connection_time_ms: u64,
    pub p99_connection_time_ms: u64,
}
/// Gossip metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMetricsSummary {
    pub heartbeats_sent: u64,
    pub heartbeats_received: u64,
    pub membership_updates: u64,
    pub state_syncs: u64,
    pub failed_rounds: u64,
    pub member_count: u64,
    pub suspect_count: u64,
    pub dead_count: u64,
    pub avg_round_duration_us: u64,
}
/// Per-peer connection tracking
#[derive(Debug, Clone)]
pub struct PeerConnectionState {
    pub peer_id: NodeId,
    pub state: ConnectionState,
    pub connected_at: Option<Instant>,
    pub last_activity: Instant,
    pub connection_count: u64,
    pub failure_count: u64,
    pub total_connected_duration: Duration,
}
impl PeerConnectionState {
    /// Create new peer connection state
    pub fn new(peer_id: NodeId) -> Self {
        Self {
            peer_id,
            state: ConnectionState::Disconnected,
            connected_at: None,
            last_activity: Instant::now(),
            connection_count: 0,
            failure_count: 0,
            total_connected_duration: Duration::ZERO,
        }
    }
    /// Mark peer as connecting
    pub fn connecting(&mut self) {
        self.state = ConnectionState::Connecting;
        self.last_activity = Instant::now();
    }
    /// Mark peer as connected
    pub fn connected(&mut self) {
        if self.state != ConnectionState::Connected {
            self.state = ConnectionState::Connected;
            self.connected_at = Some(Instant::now());
            self.connection_count += 1;
        }
        self.last_activity = Instant::now();
    }
    /// Mark peer as disconnected
    pub fn disconnected(&mut self) {
        if let Some(connected_at) = self.connected_at.take() {
            self.total_connected_duration += connected_at.elapsed();
        }
        self.state = ConnectionState::Disconnected;
        self.last_activity = Instant::now();
    }
    /// Mark connection failure
    pub fn failed(&mut self) {
        self.connected_at = None;
        self.state = ConnectionState::Disconnected;
        self.failure_count += 1;
        self.last_activity = Instant::now();
    }
    /// Get connection uptime if connected
    pub fn connection_uptime(&self) -> Option<Duration> {
        self.connected_at.map(|t| t.elapsed())
    }
    /// Get average connection duration
    pub fn avg_connection_duration(&self) -> Duration {
        if self.connection_count == 0 {
            Duration::ZERO
        } else {
            self.total_connected_duration / self.connection_count as u32
        }
    }
}
/// Per-operation latency tracking
#[derive(Debug)]
struct OperationMetrics {
    /// Operation count
    count: Counter,
    /// Success count
    success_count: Counter,
    /// Failure count
    failure_count: Counter,
    /// Latency histogram (microseconds)
    latency: Histogram,
}
impl OperationMetrics {
    fn new() -> Self {
        Self {
            count: Counter::new(),
            success_count: Counter::new(),
            failure_count: Counter::new(),
            latency: Histogram::new_latency(),
        }
    }
}
/// Operation latency summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLatencySummary {
    pub total_operations: u64,
    pub sla_threshold_us: u64,
    pub sla_violations: u64,
    pub sla_compliance_rate: f64,
    pub overall_avg_latency_us: u64,
    pub overall_p99_latency_us: u64,
    pub per_operation: Vec<OperationLatencyStats>,
}
/// Throughput summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputSummary {
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub current_bandwidth: u64,
    pub bandwidth_limit: u64,
    pub avg_bandwidth_utilization: u64,
    pub per_type: Vec<MessageTypeThroughput>,
}
/// Per-message-type throughput tracking
#[derive(Debug)]
struct MessageTypeMetrics {
    /// Messages sent
    sent_count: Counter,
    /// Messages received
    received_count: Counter,
    /// Bytes sent
    bytes_sent: Counter,
    /// Bytes received
    bytes_received: Counter,
    /// Send rate tracker
    send_rate: RateTracker,
    /// Receive rate tracker
    receive_rate: RateTracker,
}
impl MessageTypeMetrics {
    fn new() -> Self {
        Self {
            sent_count: Counter::new(),
            received_count: Counter::new(),
            bytes_sent: Counter::new(),
            bytes_received: Counter::new(),
            send_rate: RateTracker::new(0.3),
            receive_rate: RateTracker::new(0.3),
        }
    }
}
/// Migration success summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSuccessSummary {
    pub total_migrations: u64,
    pub successful_migrations: u64,
    pub failed_migrations: u64,
    pub cancelled_migrations: u64,
    pub success_rate: f64,
    pub active_migrations: u64,
    pub precopy_count: u64,
    pub postcopy_count: u64,
    pub hybrid_count: u64,
    pub avg_duration_ms: u64,
    pub p99_duration_ms: u64,
    pub avg_downtime_ms: u64,
    pub p99_downtime_ms: u64,
}
/// Operation timer for measuring latency
#[derive(Debug)]
pub struct OperationTimer {
    start: Instant,
}
impl OperationTimer {
    /// Get elapsed time in microseconds
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}
/// Per-message-type throughput
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTypeThroughput {
    pub msg_type: MessageType,
    pub sent_count: u64,
    pub received_count: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub send_rate: u64,
    pub receive_rate: u64,
}
/// DHT operation metrics
#[derive(Debug, Default)]
pub struct DhtMetrics {
    /// GET operations
    pub gets: Counter,
    /// PUT operations
    pub puts: Counter,
    /// Routing table lookups
    pub lookups: Counter,
    /// Successful lookups
    pub lookup_successes: Counter,
    /// Cache hits
    pub cache_hits: Counter,
    /// Cache misses
    pub cache_misses: Counter,
    /// Routing table size
    pub routing_table_size: Gauge,
    /// Lookup latency histogram
    pub lookup_latency: Histogram,
}
impl DhtMetrics {
    /// Create new DHT metrics
    pub fn new() -> Self {
        Self {
            gets: Counter::new(),
            puts: Counter::new(),
            lookups: Counter::new(),
            lookup_successes: Counter::new(),
            cache_hits: Counter::new(),
            cache_misses: Counter::new(),
            routing_table_size: Gauge::new(),
            lookup_latency: Histogram::new_latency(),
        }
    }
    /// Get cache hit rate (0.0 - 1.0)
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.get();
        let misses = self.cache_misses.get();
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
    /// Get lookup success rate (0.0 - 1.0)
    pub fn lookup_success_rate(&self) -> f64 {
        let successes = self.lookup_successes.get();
        let total = self.lookups.get();
        if total == 0 {
            0.0
        } else {
            successes as f64 / total as f64
        }
    }
    /// Get summary
    pub fn summary(&self) -> DhtMetricsSummary {
        let latency_stats = self.lookup_latency.stats();
        DhtMetricsSummary {
            gets: self.gets.get(),
            puts: self.puts.get(),
            lookups: self.lookups.get(),
            lookup_success_rate: self.lookup_success_rate(),
            cache_hit_rate: self.cache_hit_rate(),
            routing_table_size: self.routing_table_size.get(),
            avg_lookup_latency_us: latency_stats.mean,
            p95_lookup_latency_us: latency_stats.percentile(95.0),
        }
    }
}
/// Message type for throughput tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    /// Gossip heartbeat
    Heartbeat,
    /// Gossip membership update
    MembershipUpdate,
    /// Gossip state sync
    StateSync,
    /// DHT lookup request
    DhtLookup,
    /// DHT lookup response
    DhtLookupResponse,
    /// DHT store request
    DhtStore,
    /// Migration data transfer
    MigrationData,
    /// Agent communication
    AgentMessage,
    /// Control message
    Control,
    /// Unknown/other
    Other,
}
/// Counter for monotonically increasing values
#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}
impl Counter {
    /// Create a new counter starting at 0
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }
    /// Increment the counter by 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }
    /// Increment the counter by n
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }
    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
    /// Reset the counter to 0
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}
