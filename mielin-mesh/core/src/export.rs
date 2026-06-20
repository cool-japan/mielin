//! Metrics Export Module
//!
//! Provides exporters for various monitoring systems:
//! - Prometheus text format
//! - JSON format for REST APIs
//! - OpenTelemetry format (future)
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_mesh_core::metrics::MetricsRegistry;
//! use mielin_mesh_core::export::{PrometheusExporter, JsonExporter};
//!
//! let registry = MetricsRegistry::new(node_id);
//! let prometheus = PrometheusExporter::new("mielin");
//! let output = prometheus.export(&registry).await;
//! ```

use crate::metrics::{
    DhtMetricsSummary, GossipMetricsSummary, MetricsRegistry, MetricsSummary, NodeMetricsSummary,
};
use crate::NodeId;
use serde::Serialize;
use std::fmt::Write;

/// Prometheus text format exporter
///
/// Exports metrics in the Prometheus exposition format for scraping.
pub struct PrometheusExporter {
    /// Metric name prefix
    prefix: String,
    /// Include help text
    include_help: bool,
    /// Include type annotations
    include_type: bool,
}

impl PrometheusExporter {
    /// Create a new Prometheus exporter with the given prefix
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            include_help: true,
            include_type: true,
        }
    }

    /// Set whether to include help text
    pub fn with_help(mut self, include: bool) -> Self {
        self.include_help = include;
        self
    }

    /// Set whether to include type annotations
    pub fn with_type(mut self, include: bool) -> Self {
        self.include_type = include;
        self
    }

    /// Export metrics to Prometheus text format
    pub async fn export(&self, registry: &MetricsRegistry) -> String {
        let summary = registry.summary().await;
        self.format_summary(&summary)
    }

    /// Format a metrics summary to Prometheus text format
    pub fn format_summary(&self, summary: &MetricsSummary) -> String {
        let mut output = String::with_capacity(4096);

        // Uptime
        self.write_metric(
            &mut output,
            "uptime_seconds",
            "gauge",
            "Total uptime in seconds",
            summary.uptime_secs as f64,
            &[],
        );

        // Message rate
        self.write_metric(
            &mut output,
            "message_rate",
            "gauge",
            "Current message rate per second",
            summary.message_rate as f64,
            &[],
        );

        // Local node metrics
        self.write_node_metrics(&mut output, &summary.local, "local");

        // Gossip metrics
        self.write_gossip_metrics(&mut output, &summary.gossip);

        // DHT metrics
        self.write_dht_metrics(&mut output, &summary.dht);

        // Peer metrics
        for peer in &summary.peers {
            let peer_id = format_node_id(peer.node_id);
            self.write_node_metrics(&mut output, peer, &peer_id);
        }

        output
    }

    /// Write a single metric line
    fn write_metric(
        &self,
        output: &mut String,
        name: &str,
        metric_type: &str,
        help: &str,
        value: f64,
        labels: &[(&str, &str)],
    ) {
        let full_name = format!("{}_{}", self.prefix, name);

        if self.include_help {
            let _ = writeln!(output, "# HELP {} {}", full_name, help);
        }
        if self.include_type {
            let _ = writeln!(output, "# TYPE {} {}", full_name, metric_type);
        }

        if labels.is_empty() {
            let _ = writeln!(output, "{} {}", full_name, format_value(value));
        } else {
            let label_str: Vec<String> = labels
                .iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, escape_label_value(v)))
                .collect();
            let _ = writeln!(
                output,
                "{}{{{}}} {}",
                full_name,
                label_str.join(","),
                format_value(value)
            );
        }
    }

    /// Write node metrics
    fn write_node_metrics(&self, output: &mut String, metrics: &NodeMetricsSummary, label: &str) {
        let labels = [("node", label)];

        self.write_metric(
            output,
            "messages_sent_total",
            "counter",
            "Total messages sent",
            metrics.messages_sent as f64,
            &labels,
        );

        self.write_metric(
            output,
            "messages_received_total",
            "counter",
            "Total messages received",
            metrics.messages_received as f64,
            &labels,
        );

        self.write_metric(
            output,
            "bytes_sent_total",
            "counter",
            "Total bytes sent",
            metrics.bytes_sent as f64,
            &labels,
        );

        self.write_metric(
            output,
            "bytes_received_total",
            "counter",
            "Total bytes received",
            metrics.bytes_received as f64,
            &labels,
        );

        self.write_metric(
            output,
            "active_connections",
            "gauge",
            "Number of active connections",
            metrics.active_connections as f64,
            &labels,
        );

        self.write_metric(
            output,
            "message_latency_avg_us",
            "gauge",
            "Average message latency in microseconds",
            metrics.avg_latency_us as f64,
            &labels,
        );

        self.write_metric(
            output,
            "message_latency_p99_us",
            "gauge",
            "99th percentile message latency in microseconds",
            metrics.p99_latency_us as f64,
            &labels,
        );

        self.write_metric(
            output,
            "failures_total",
            "counter",
            "Total number of failures",
            metrics.failures as f64,
            &labels,
        );
    }

    /// Write gossip metrics
    fn write_gossip_metrics(&self, output: &mut String, metrics: &GossipMetricsSummary) {
        self.write_metric(
            output,
            "gossip_heartbeats_sent_total",
            "counter",
            "Total gossip heartbeats sent",
            metrics.heartbeats_sent as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_heartbeats_received_total",
            "counter",
            "Total gossip heartbeats received",
            metrics.heartbeats_received as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_membership_updates_total",
            "counter",
            "Total membership updates",
            metrics.membership_updates as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_state_syncs_total",
            "counter",
            "Total state synchronizations",
            metrics.state_syncs as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_failed_rounds_total",
            "counter",
            "Total failed gossip rounds",
            metrics.failed_rounds as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_member_count",
            "gauge",
            "Current number of cluster members",
            metrics.member_count as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_suspect_count",
            "gauge",
            "Number of suspected members",
            metrics.suspect_count as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_dead_count",
            "gauge",
            "Number of dead members",
            metrics.dead_count as f64,
            &[],
        );

        self.write_metric(
            output,
            "gossip_round_duration_avg_us",
            "gauge",
            "Average gossip round duration in microseconds",
            metrics.avg_round_duration_us as f64,
            &[],
        );
    }

    /// Write DHT metrics
    fn write_dht_metrics(&self, output: &mut String, metrics: &DhtMetricsSummary) {
        self.write_metric(
            output,
            "dht_gets_total",
            "counter",
            "Total DHT GET operations",
            metrics.gets as f64,
            &[],
        );

        self.write_metric(
            output,
            "dht_puts_total",
            "counter",
            "Total DHT PUT operations",
            metrics.puts as f64,
            &[],
        );

        self.write_metric(
            output,
            "dht_lookups_total",
            "counter",
            "Total DHT lookups",
            metrics.lookups as f64,
            &[],
        );

        self.write_metric(
            output,
            "dht_lookup_success_rate",
            "gauge",
            "DHT lookup success rate (0-1)",
            metrics.lookup_success_rate,
            &[],
        );

        self.write_metric(
            output,
            "dht_cache_hit_rate",
            "gauge",
            "DHT cache hit rate (0-1)",
            metrics.cache_hit_rate,
            &[],
        );

        self.write_metric(
            output,
            "dht_routing_table_size",
            "gauge",
            "Number of entries in routing table",
            metrics.routing_table_size as f64,
            &[],
        );

        self.write_metric(
            output,
            "dht_lookup_latency_avg_us",
            "gauge",
            "Average lookup latency in microseconds",
            metrics.avg_lookup_latency_us as f64,
            &[],
        );

        self.write_metric(
            output,
            "dht_lookup_latency_p95_us",
            "gauge",
            "95th percentile lookup latency in microseconds",
            metrics.p95_lookup_latency_us as f64,
            &[],
        );
    }
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new("mielin")
    }
}

/// JSON metrics exporter for REST APIs
pub struct JsonExporter {
    /// Whether to include timestamps
    include_timestamps: bool,
    /// Whether to pretty print
    pretty: bool,
}

impl JsonExporter {
    /// Create a new JSON exporter
    pub fn new() -> Self {
        Self {
            include_timestamps: true,
            pretty: false,
        }
    }

    /// Set whether to include timestamps
    pub fn with_timestamps(mut self, include: bool) -> Self {
        self.include_timestamps = include;
        self
    }

    /// Set whether to pretty print
    pub fn with_pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    /// Export metrics to JSON format
    pub async fn export(&self, registry: &MetricsRegistry) -> String {
        let summary = registry.summary().await;
        self.format_summary(&summary)
    }

    /// Format a metrics summary to JSON
    pub fn format_summary(&self, summary: &MetricsSummary) -> String {
        let output = JsonMetrics {
            timestamp_ms: if self.include_timestamps {
                Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                )
            } else {
                None
            },
            uptime_secs: summary.uptime_secs,
            message_rate: summary.message_rate,
            local: summary.local.clone(),
            peers: summary.peers.clone(),
            gossip: summary.gossip.clone(),
            dht: summary.dht.clone(),
        };

        if self.pretty {
            serde_json::to_string_pretty(&output).unwrap_or_default()
        } else {
            serde_json::to_string(&output).unwrap_or_default()
        }
    }
}

impl Default for JsonExporter {
    fn default() -> Self {
        Self::new()
    }
}

/// JSON output structure
#[derive(Serialize)]
struct JsonMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_ms: Option<u64>,
    uptime_secs: u64,
    message_rate: u64,
    local: NodeMetricsSummary,
    peers: Vec<NodeMetricsSummary>,
    gossip: GossipMetricsSummary,
    dht: DhtMetricsSummary,
}

/// Migration metrics for tracking agent migrations
#[derive(Debug, Clone, Serialize)]
pub struct MigrationMetricsExport {
    /// Total migrations initiated
    pub initiated: u64,
    /// Total migrations completed
    pub completed: u64,
    /// Total migrations failed
    pub failed: u64,
    /// Total migrations in progress
    pub in_progress: u64,
    /// Average migration duration in milliseconds
    pub avg_duration_ms: u64,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Success rate (0-1)
    pub success_rate: f64,
}

impl Default for MigrationMetricsExport {
    fn default() -> Self {
        Self {
            initiated: 0,
            completed: 0,
            failed: 0,
            in_progress: 0,
            avg_duration_ms: 0,
            bytes_transferred: 0,
            success_rate: 0.0,
        }
    }
}

/// Format a floating point value for Prometheus
fn format_value(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value > 0.0 {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        }
    } else if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{:.0}", value)
    } else {
        format!("{}", value)
    }
}

/// Escape a label value for Prometheus format
fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Format a NodeId for labels
fn format_node_id(id: NodeId) -> String {
    let bytes = id.as_bytes();
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_prometheus_exporter_creation() {
        let exporter = PrometheusExporter::new("test");
        assert_eq!(exporter.prefix, "test");
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_prometheus_export_basic() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        registry.local().messages_sent.add(100);
        registry.gossip().heartbeats_sent.add(50);
        registry.dht().gets.add(25);

        let exporter = PrometheusExporter::new("mielin");
        let output = exporter.export(&registry).await;

        assert!(output.contains("mielin_uptime_seconds"));
        assert!(output.contains("mielin_messages_sent_total"));
        assert!(output.contains("mielin_gossip_heartbeats_sent_total"));
        assert!(output.contains("mielin_dht_gets_total"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_prometheus_without_help() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        let exporter = PrometheusExporter::new("mielin").with_help(false);
        let output = exporter.export(&registry).await;

        assert!(!output.contains("# HELP"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_prometheus_without_type() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        let exporter = PrometheusExporter::new("mielin").with_type(false);
        let output = exporter.export(&registry).await;

        assert!(!output.contains("# TYPE"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_json_exporter_basic() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        registry.local().messages_sent.add(100);

        let exporter = JsonExporter::new();
        let output = exporter.export(&registry).await;

        assert!(output.contains("\"messages_sent\":100"));
        assert!(output.contains("\"timestamp_ms\":"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_json_exporter_without_timestamps() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        let exporter = JsonExporter::new().with_timestamps(false);
        let output = exporter.export(&registry).await;

        assert!(!output.contains("timestamp_ms"));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_json_exporter_pretty() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        let exporter = JsonExporter::new().with_pretty(true);
        let output = exporter.export(&registry).await;

        // Pretty printed JSON has newlines
        assert!(output.contains('\n'));
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value(42.0), "42");
        assert_eq!(format_value(3.14158), "3.14158");
        assert_eq!(format_value(f64::NAN), "NaN");
        assert_eq!(format_value(f64::INFINITY), "+Inf");
        assert_eq!(format_value(f64::NEG_INFINITY), "-Inf");
    }

    #[test]
    fn test_escape_label_value() {
        assert_eq!(escape_label_value("simple"), "simple");
        assert_eq!(escape_label_value("with\"quote"), "with\\\"quote");
        assert_eq!(escape_label_value("with\\backslash"), "with\\\\backslash");
        assert_eq!(escape_label_value("with\nnewline"), "with\\nnewline");
    }

    #[test]
    fn test_format_node_id() {
        let id = NodeId::nil();
        let formatted = format_node_id(id);
        assert_eq!(formatted, "00000000");
    }

    #[test]
    fn test_migration_metrics_export_default() {
        let metrics = MigrationMetricsExport::default();
        assert_eq!(metrics.initiated, 0);
        assert_eq!(metrics.completed, 0);
        assert_eq!(metrics.success_rate, 0.0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_prometheus_peer_metrics() {
        let local_id = NodeId::new_v4();
        let peer_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(local_id);

        // Add peer metrics
        let peer = registry.peer(peer_id).await;
        peer.messages_sent.add(50);

        let exporter = PrometheusExporter::new("mielin");
        let output = exporter.export(&registry).await;

        // Should have both local and peer metrics
        assert!(output.contains("node=\"local\""));
        // Peer should be included
        assert!(output.matches("node=").count() > 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_prometheus_exporter_labels() {
        let node_id = NodeId::new_v4();
        let registry = MetricsRegistry::new(node_id);

        let exporter = PrometheusExporter::new("test");
        let output = exporter.export(&registry).await;

        // Check that labels are properly formatted
        assert!(output.contains("test_messages_sent_total{node=\"local\"}"));
    }
}
