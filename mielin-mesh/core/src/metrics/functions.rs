//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

#[cfg(test)]
mod tests {
    use super::super::types::*;
    use crate::NodeId;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_counter() {
        let counter = Counter::new();
        assert_eq!(counter.get(), 0);
        counter.inc();
        assert_eq!(counter.get(), 1);
        counter.add(5);
        assert_eq!(counter.get(), 6);
        counter.reset();
        assert_eq!(counter.get(), 0);
    }
    #[test]
    fn test_gauge() {
        let gauge = Gauge::new();
        assert_eq!(gauge.get(), 0);
        gauge.set(10);
        assert_eq!(gauge.get(), 10);
        gauge.inc();
        assert_eq!(gauge.get(), 11);
        gauge.dec();
        assert_eq!(gauge.get(), 10);
    }
    #[test]
    fn test_histogram() {
        let hist = Histogram::new(vec![10, 50, 100, 500]);
        hist.observe(5);
        hist.observe(25);
        hist.observe(75);
        hist.observe(200);
        hist.observe(1000);
        let stats = hist.stats();
        assert_eq!(stats.count, 5);
        assert_eq!(stats.sum, 1305);
        assert_eq!(stats.mean, 261);
    }
    #[test]
    fn test_histogram_percentile() {
        let hist = Histogram::new(vec![10, 50, 100, 500, 1000]);
        for i in 0..100 {
            hist.observe(i * 10);
        }
        let stats = hist.stats();
        assert!(stats.percentile(50.0) > 0);
        assert!(stats.percentile(99.0) >= stats.percentile(50.0));
    }
    #[test]
    fn test_histogram_empty_percentile() {
        let hist = Histogram::new(vec![10, 50, 100]);
        let stats = hist.stats();
        assert_eq!(stats.percentile(50.0), 0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_rate_tracker() {
        let tracker = RateTracker::new(0.5);
        for _ in 0..100 {
            tracker.record();
        }
        tracker.update().await;
        let _rate = tracker.rate();
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_node_metrics() {
        let node_id = NodeId::new_v4();
        let metrics = NodeMetrics::new(node_id);
        metrics.record_send(1024, 5000).await;
        metrics.record_receive(512).await;
        metrics.failures.inc();
        let summary = metrics.summary().await;
        assert_eq!(summary.messages_sent, 1);
        assert_eq!(summary.messages_received, 1);
        assert_eq!(summary.bytes_sent, 1024);
        assert_eq!(summary.bytes_received, 512);
        assert_eq!(summary.failures, 1);
    }
    #[test]
    fn test_gossip_metrics() {
        let metrics = GossipMetrics::new();
        metrics.heartbeats_sent.inc();
        metrics.heartbeats_sent.inc();
        metrics.heartbeats_received.inc();
        metrics.member_count.set(5);
        metrics.suspect_count.set(1);
        let summary = metrics.summary();
        assert_eq!(summary.heartbeats_sent, 2);
        assert_eq!(summary.heartbeats_received, 1);
        assert_eq!(summary.member_count, 5);
        assert_eq!(summary.suspect_count, 1);
    }
    #[test]
    fn test_dht_metrics() {
        let metrics = DhtMetrics::new();
        metrics.lookups.add(100);
        metrics.lookup_successes.add(90);
        metrics.cache_hits.add(60);
        metrics.cache_misses.add(40);
        assert!((metrics.lookup_success_rate() - 0.9).abs() < 0.001);
        assert!((metrics.cache_hit_rate() - 0.6).abs() < 0.001);
    }
    #[test]
    fn test_dht_metrics_zero_division() {
        let metrics = DhtMetrics::new();
        assert_eq!(metrics.lookup_success_rate(), 0.0);
        assert_eq!(metrics.cache_hit_rate(), 0.0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_metrics_registry() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);
        registry.local().messages_sent.inc();
        registry.local().bytes_sent.add(1024);
        registry.gossip().heartbeats_sent.inc();
        registry.dht().gets.inc();
        registry.record_message();
        let summary = registry.summary().await;
        assert_eq!(summary.local.messages_sent, 1);
        assert_eq!(summary.local.bytes_sent, 1024);
        assert_eq!(summary.gossip.heartbeats_sent, 1);
        assert_eq!(summary.dht.gets, 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_metrics() {
        let local_id = NodeId::new_v4();
        let peer_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(local_id);
        let peer = registry.peer(peer_id).await;
        peer.messages_sent.inc();
        let peer2 = registry.peer(peer_id).await;
        assert_eq!(peer2.messages_sent.get(), 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_metrics_reset() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);
        registry.local().messages_sent.add(100);
        registry.gossip().heartbeats_sent.add(50);
        registry.dht().gets.add(25);
        registry.reset().await;
        assert_eq!(registry.local().messages_sent.get(), 0);
        assert_eq!(registry.gossip().heartbeats_sent.get(), 0);
        assert_eq!(registry.dht().gets.get(), 0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_uptime() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);
        tokio::time::sleep(Duration::from_millis(10)).await;
        let uptime = registry.uptime();
        assert!(uptime.as_millis() >= 10);
    }
    #[test]
    fn test_histogram_latency_buckets() {
        let hist = Histogram::new_latency();
        assert!(!hist.buckets.is_empty());
        assert!(hist.buckets.iter().all(|&b| b > 0));
    }
    #[test]
    fn test_histogram_size_buckets() {
        let hist = Histogram::new_size();
        assert!(!hist.buckets.is_empty());
        assert!(hist.buckets.iter().all(|&b| b > 0));
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_cleanup_stale_peers() {
        let local_id = NodeId::new_v4();
        let peer_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(local_id);
        let peer = registry.peer(peer_id).await;
        peer.messages_sent.inc();
        *peer.last_activity.write().await = SystemTime::UNIX_EPOCH;
        registry
            .cleanup_stale_peers(Duration::from_secs(3600))
            .await;
        let summary = registry.summary().await;
        assert!(summary.peers.is_empty());
    }
    #[test]
    fn test_histogram_observe_above_max_bucket() {
        let hist = Histogram::new(vec![10, 50, 100]);
        hist.observe(500);
        let stats = hist.stats();
        assert_eq!(stats.count, 1);
        assert_eq!(stats.sum, 500);
    }
    #[test]
    fn test_peer_connection_state_new() {
        let peer_id = NodeId::new_v4();
        let state = PeerConnectionState::new(peer_id);
        assert_eq!(state.peer_id, peer_id);
        assert_eq!(state.state, ConnectionState::Disconnected);
        assert!(state.connected_at.is_none());
        assert_eq!(state.connection_count, 0);
        assert_eq!(state.failure_count, 0);
    }
    #[test]
    fn test_peer_connection_state_lifecycle() {
        let peer_id = NodeId::new_v4();
        let mut state = PeerConnectionState::new(peer_id);
        state.connecting();
        assert_eq!(state.state, ConnectionState::Connecting);
        state.connected();
        assert_eq!(state.state, ConnectionState::Connected);
        assert!(state.connected_at.is_some());
        assert_eq!(state.connection_count, 1);
        state.disconnected();
        assert_eq!(state.state, ConnectionState::Disconnected);
        assert!(state.connected_at.is_none());
        assert!(state.total_connected_duration >= Duration::ZERO);
    }
    #[test]
    fn test_peer_connection_state_failure() {
        let peer_id = NodeId::new_v4();
        let mut state = PeerConnectionState::new(peer_id);
        state.connecting();
        state.failed();
        assert_eq!(state.state, ConnectionState::Disconnected);
        assert_eq!(state.failure_count, 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_connection_metrics_creation() {
        let metrics = PeerConnectionMetrics::new();
        assert_eq!(metrics.connection_attempts.get(), 0);
        assert_eq!(metrics.connections_established.get(), 0);
        assert_eq!(metrics.connected_peers.get(), 0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_connection_metrics_connect_flow() {
        let metrics = PeerConnectionMetrics::new();
        let peer_id = NodeId::new_v4();
        metrics.record_connect_attempt(peer_id).await;
        assert_eq!(metrics.connection_attempts.get(), 1);
        assert_eq!(metrics.connecting_peers.get(), 1);
        metrics.record_connected(peer_id, 50).await;
        assert_eq!(metrics.connections_established.get(), 1);
        assert_eq!(metrics.connecting_peers.get(), 0);
        assert_eq!(metrics.connected_peers.get(), 1);
        let state = metrics.peer_state(&peer_id).await.unwrap();
        assert_eq!(state.state, ConnectionState::Connected);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_connection_metrics_disconnect() {
        let metrics = PeerConnectionMetrics::new();
        let peer_id = NodeId::new_v4();
        metrics.record_connect_attempt(peer_id).await;
        metrics.record_connected(peer_id, 50).await;
        metrics.record_disconnected(peer_id).await;
        assert_eq!(metrics.disconnections.get(), 1);
        assert_eq!(metrics.connected_peers.get(), 0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_connection_metrics_failure() {
        let metrics = PeerConnectionMetrics::new();
        let peer_id = NodeId::new_v4();
        metrics.record_connect_attempt(peer_id).await;
        metrics.record_connection_failed(peer_id).await;
        assert_eq!(metrics.connection_failures.get(), 1);
        assert_eq!(metrics.connecting_peers.get(), 0);
        let state = metrics.peer_state(&peer_id).await.unwrap();
        assert_eq!(state.failure_count, 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_connection_metrics_reconnect() {
        let metrics = PeerConnectionMetrics::new();
        let peer_id = NodeId::new_v4();
        metrics.record_connect_attempt(peer_id).await;
        metrics.record_connected(peer_id, 50).await;
        metrics.record_disconnected(peer_id).await;
        metrics.record_reconnect_attempt(peer_id).await;
        assert_eq!(metrics.reconnect_attempts.get(), 1);
        metrics.record_reconnected(peer_id, 30).await;
        assert_eq!(metrics.reconnections_succeeded.get(), 1);
        assert_eq!(metrics.connected_peers.get(), 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_peer_connection_metrics_connected_peers() {
        let metrics = PeerConnectionMetrics::new();
        let peer1 = NodeId::new_v4();
        let peer2 = NodeId::new_v4();
        metrics.record_connect_attempt(peer1).await;
        metrics.record_connected(peer1, 50).await;
        metrics.record_connect_attempt(peer2).await;
        metrics.record_connected(peer2, 60).await;
        let connected = metrics.connected_peer_ids().await;
        assert_eq!(connected.len(), 2);
        assert!(connected.contains(&peer1));
        assert!(connected.contains(&peer2));
    }
    #[test]
    fn test_peer_connection_summary() {
        let metrics = PeerConnectionMetrics::new();
        metrics.connection_attempts.add(10);
        metrics.connections_established.add(8);
        metrics.connection_failures.add(2);
        let summary = metrics.summary();
        assert_eq!(summary.connection_attempts, 10);
        assert_eq!(summary.connections_established, 8);
        assert_eq!(summary.connection_failures, 2);
    }
    #[test]
    fn test_message_type_display() {
        assert_eq!(format!("{}", MessageType::Heartbeat), "heartbeat");
        assert_eq!(format!("{}", MessageType::DhtLookup), "dht_lookup");
        assert_eq!(format!("{}", MessageType::MigrationData), "migration_data");
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_throughput_metrics_creation() {
        let metrics = ThroughputMetrics::new();
        assert_eq!(metrics.total_bytes_sent.get(), 0);
        assert_eq!(metrics.total_messages_sent.get(), 0);
        assert_eq!(metrics.bandwidth_limit(), 0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_throughput_metrics_record_send() {
        let metrics = ThroughputMetrics::new();
        metrics.record_send(MessageType::Heartbeat, 100).await;
        metrics.record_send(MessageType::Heartbeat, 100).await;
        metrics.record_send(MessageType::DhtLookup, 500).await;
        assert_eq!(metrics.total_bytes_sent.get(), 700);
        assert_eq!(metrics.total_messages_sent.get(), 3);
        let heartbeat = metrics
            .type_throughput(MessageType::Heartbeat)
            .await
            .unwrap();
        assert_eq!(heartbeat.sent_count, 2);
        assert_eq!(heartbeat.bytes_sent, 200);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_throughput_metrics_record_receive() {
        let metrics = ThroughputMetrics::new();
        metrics.record_receive(MessageType::StateSync, 1000).await;
        assert_eq!(metrics.total_bytes_received.get(), 1000);
        assert_eq!(metrics.total_messages_received.get(), 1);
        let state_sync = metrics
            .type_throughput(MessageType::StateSync)
            .await
            .unwrap();
        assert_eq!(state_sync.received_count, 1);
        assert_eq!(state_sync.bytes_received, 1000);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_throughput_metrics_bandwidth_limit() {
        let metrics = ThroughputMetrics::new();
        metrics.set_bandwidth_limit(1_000_000);
        assert_eq!(metrics.bandwidth_limit(), 1_000_000);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_throughput_metrics_summary() {
        let metrics = ThroughputMetrics::new();
        metrics.record_send(MessageType::Heartbeat, 100).await;
        metrics.record_receive(MessageType::DhtLookup, 200).await;
        let summary = metrics.summary().await;
        assert_eq!(summary.total_bytes_sent, 100);
        assert_eq!(summary.total_bytes_received, 200);
        assert_eq!(summary.total_messages_sent, 1);
        assert_eq!(summary.total_messages_received, 1);
        assert_eq!(summary.per_type.len(), 2);
    }
    #[test]
    fn test_migration_success_metrics_creation() {
        let metrics = MigrationSuccessMetrics::new();
        assert_eq!(metrics.total_migrations.get(), 0);
        assert_eq!(metrics.successful_migrations.get(), 0);
        assert_eq!(metrics.success_rate(), 0.0);
    }
    #[test]
    fn test_migration_success_metrics_record_start() {
        let metrics = MigrationSuccessMetrics::new();
        metrics.record_start("PreCopy");
        metrics.record_start("PostCopy");
        metrics.record_start("Hybrid");
        assert_eq!(metrics.total_migrations.get(), 3);
        assert_eq!(metrics.active_migrations.get(), 3);
        assert_eq!(metrics.precopy_count.get(), 1);
        assert_eq!(metrics.postcopy_count.get(), 1);
        assert_eq!(metrics.hybrid_count.get(), 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_migration_success_metrics_record_complete() {
        let metrics = MigrationSuccessMetrics::new();
        metrics.record_start("PreCopy");
        let result = MigrationResult {
            agent_id: [1; 16],
            source_node: NodeId::new_v4(),
            target_node: NodeId::new_v4(),
            strategy: "PreCopy".to_string(),
            success: true,
            duration_ms: 1000,
            downtime_ms: 50,
            bytes_transferred: 1024 * 1024,
            timestamp: SystemTime::now(),
            error: None,
        };
        metrics.record_complete(result).await;
        assert_eq!(metrics.successful_migrations.get(), 1);
        assert_eq!(metrics.active_migrations.get(), 0);
        assert_eq!(metrics.success_rate(), 1.0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_migration_success_metrics_record_failed() {
        let metrics = MigrationSuccessMetrics::new();
        metrics.record_start("PostCopy");
        let result = MigrationResult {
            agent_id: [2; 16],
            source_node: NodeId::new_v4(),
            target_node: NodeId::new_v4(),
            strategy: "PostCopy".to_string(),
            success: false,
            duration_ms: 500,
            downtime_ms: 0,
            bytes_transferred: 0,
            timestamp: SystemTime::now(),
            error: Some("Network error".to_string()),
        };
        metrics.record_complete(result).await;
        assert_eq!(metrics.failed_migrations.get(), 1);
        assert_eq!(metrics.success_rate(), 0.0);
    }
    #[test]
    fn test_migration_success_metrics_cancelled() {
        let metrics = MigrationSuccessMetrics::new();
        metrics.record_start("Hybrid");
        metrics.record_cancelled();
        assert_eq!(metrics.cancelled_migrations.get(), 1);
        assert_eq!(metrics.active_migrations.get(), 0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_migration_success_metrics_recent_results() {
        let metrics = MigrationSuccessMetrics::new();
        metrics.record_start("PreCopy");
        let result = MigrationResult {
            agent_id: [3; 16],
            source_node: NodeId::new_v4(),
            target_node: NodeId::new_v4(),
            strategy: "PreCopy".to_string(),
            success: true,
            duration_ms: 2000,
            downtime_ms: 100,
            bytes_transferred: 2048,
            timestamp: SystemTime::now(),
            error: None,
        };
        metrics.record_complete(result).await;
        let recent = metrics.recent_results().await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].agent_id, [3; 16]);
    }
    #[test]
    fn test_migration_success_summary() {
        let metrics = MigrationSuccessMetrics::new();
        metrics.total_migrations.add(10);
        metrics.successful_migrations.add(8);
        metrics.failed_migrations.add(2);
        let summary = metrics.summary();
        assert_eq!(summary.total_migrations, 10);
        assert_eq!(summary.successful_migrations, 8);
        assert_eq!(summary.failed_migrations, 2);
        assert!((summary.success_rate - 0.8).abs() < 0.001);
    }
    #[test]
    fn test_operation_type_display() {
        assert_eq!(format!("{}", OperationType::DhtGet), "dht_get");
        assert_eq!(format!("{}", OperationType::GossipRound), "gossip_round");
        assert_eq!(format!("{}", OperationType::AgentLookup), "agent_lookup");
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_operation_latency_metrics_creation() {
        let metrics = OperationLatencyMetrics::new();
        assert_eq!(metrics.total_operations.get(), 0);
        assert_eq!(metrics.sla_violations.get(), 0);
        assert_eq!(metrics.sla_compliance_rate(), 1.0);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_operation_latency_metrics_record() {
        let metrics = OperationLatencyMetrics::new();
        metrics.record(OperationType::DhtGet, 5000, true).await;
        metrics.record(OperationType::DhtGet, 10000, true).await;
        metrics.record(OperationType::DhtPut, 15000, false).await;
        assert_eq!(metrics.total_operations.get(), 3);
        let dht_get_stats = metrics
            .operation_stats(OperationType::DhtGet)
            .await
            .unwrap();
        assert_eq!(dht_get_stats.count, 2);
        assert_eq!(dht_get_stats.success_count, 2);
        assert_eq!(dht_get_stats.failure_count, 0);
        let dht_put_stats = metrics
            .operation_stats(OperationType::DhtPut)
            .await
            .unwrap();
        assert_eq!(dht_put_stats.count, 1);
        assert_eq!(dht_put_stats.success_count, 0);
        assert_eq!(dht_put_stats.failure_count, 1);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_operation_latency_metrics_sla() {
        let metrics = OperationLatencyMetrics::new();
        metrics.set_sla_threshold(10_000);
        metrics.record(OperationType::DhtGet, 5000, true).await;
        assert_eq!(metrics.sla_violations.get(), 0);
        metrics.record(OperationType::DhtGet, 15_000, true).await;
        assert_eq!(metrics.sla_violations.get(), 1);
        assert!((metrics.sla_compliance_rate() - 0.5).abs() < 0.001);
    }
    #[test]
    fn test_operation_timer() {
        let metrics = OperationLatencyMetrics::new();
        let timer = metrics.start_timer();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed_us = timer.elapsed_us();
        assert!(elapsed_us >= 10_000);
        let elapsed_ms = timer.elapsed_ms();
        assert!(elapsed_ms >= 10);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_operation_latency_summary() {
        let metrics = OperationLatencyMetrics::new();
        metrics.record(OperationType::DhtGet, 5000, true).await;
        metrics
            .record(OperationType::GossipRound, 20000, true)
            .await;
        let summary = metrics.summary().await;
        assert_eq!(summary.total_operations, 2);
        assert_eq!(summary.per_operation.len(), 2);
    }
    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_operation_latency_no_stats_for_unknown() {
        let metrics = OperationLatencyMetrics::new();
        let stats = metrics.operation_stats(OperationType::DhtGet).await;
        assert!(stats.is_none());
    }
}
