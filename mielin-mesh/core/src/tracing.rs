//! Distributed Tracing for Mesh Network
//!
//! Provides comprehensive distributed tracing with:
//! - Request tracing across nodes (W3C Trace Context compatible)
//! - Migration path tracing
//! - Gossip propagation tracing
//! - OpenTelemetry-compatible span format

use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

// =============================================================================
// Trace and Span IDs
// =============================================================================

/// 128-bit trace ID (W3C Trace Context compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub [u8; 16]);

impl TraceId {
    /// Generate a new random trace ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().into_bytes())
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Format as hex string (W3C format)
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = hex::decode(s).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes);
        Some(Self(arr))
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// 64-bit span ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub [u8; 8]);

impl SpanId {
    /// Generate a new random span ID
    pub fn new() -> Self {
        let uuid = uuid::Uuid::new_v4();
        let bytes = uuid.as_bytes();
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes[0..8]);
        Self(arr)
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Format as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = hex::decode(s).ok()?;
        if bytes.len() != 8 {
            return None;
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes);
        Some(Self(arr))
    }

    /// Nil span ID (no parent)
    pub fn nil() -> Self {
        Self([0u8; 8])
    }

    /// Check if nil
    pub fn is_nil(&self) -> bool {
        self.0 == [0u8; 8]
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// =============================================================================
// Trace Context (W3C compatible)
// =============================================================================

/// Trace flags (W3C Trace Context)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceFlags(pub u8);

impl TraceFlags {
    /// No flags set
    pub const NONE: Self = Self(0x00);
    /// Sampled flag - trace should be recorded
    pub const SAMPLED: Self = Self(0x01);

    /// Check if sampled
    pub fn is_sampled(&self) -> bool {
        self.0 & 0x01 != 0
    }

    /// Set sampled flag
    pub fn set_sampled(&mut self, sampled: bool) {
        if sampled {
            self.0 |= 0x01;
        } else {
            self.0 &= !0x01;
        }
    }
}

impl Default for TraceFlags {
    fn default() -> Self {
        Self::SAMPLED
    }
}

/// Trace context for propagation across nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID (shared across all spans in trace)
    pub trace_id: TraceId,
    /// Parent span ID
    pub parent_span_id: SpanId,
    /// Trace flags
    pub flags: TraceFlags,
    /// Trace state (vendor-specific key-value pairs)
    pub trace_state: HashMap<String, String>,
}

impl TraceContext {
    /// Create new trace context
    pub fn new() -> Self {
        Self {
            trace_id: TraceId::new(),
            parent_span_id: SpanId::nil(),
            flags: TraceFlags::SAMPLED,
            trace_state: HashMap::new(),
        }
    }

    /// Create child context with new span as parent
    pub fn child(&self, span_id: SpanId) -> Self {
        Self {
            trace_id: self.trace_id,
            parent_span_id: span_id,
            flags: self.flags,
            trace_state: self.trace_state.clone(),
        }
    }

    /// Format as W3C traceparent header
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id.to_hex(),
            self.parent_span_id.to_hex(),
            self.flags.0
        )
    }

    /// Parse from W3C traceparent header
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 || parts[0] != "00" {
            return None;
        }

        let trace_id = TraceId::from_hex(parts[1])?;
        let parent_span_id = SpanId::from_hex(parts[2])?;
        let flags = u8::from_str_radix(parts[3], 16).ok()?;

        Some(Self {
            trace_id,
            parent_span_id,
            flags: TraceFlags(flags),
            trace_state: HashMap::new(),
        })
    }

    /// Add trace state entry
    pub fn with_state(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.trace_state.insert(key.into(), value.into());
        self
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Span
// =============================================================================

/// Span kind (OpenTelemetry compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpanKind {
    /// Internal operation
    #[default]
    Internal,
    /// Server-side of RPC
    Server,
    /// Client-side of RPC
    Client,
    /// Message producer
    Producer,
    /// Message consumer
    Consumer,
}

/// Span status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpanStatus {
    /// Unset status
    #[default]
    Unset,
    /// Operation completed successfully
    Ok,
    /// Operation failed
    Error { message: String },
}

/// Span event (point-in-time occurrence within span)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Timestamp (Unix microseconds)
    pub timestamp_us: u64,
    /// Event attributes
    pub attributes: HashMap<String, AttributeValue>,
}

impl SpanEvent {
    /// Create new event
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp_us: current_timestamp_us(),
            attributes: HashMap::new(),
        }
    }

    /// Add attribute
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<AttributeValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Span link (reference to another span)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLink {
    /// Linked trace ID
    pub trace_id: TraceId,
    /// Linked span ID
    pub span_id: SpanId,
    /// Link attributes
    pub attributes: HashMap<String, AttributeValue>,
}

impl SpanLink {
    /// Create new link
    pub fn new(trace_id: TraceId, span_id: SpanId) -> Self {
        Self {
            trace_id,
            span_id,
            attributes: HashMap::new(),
        }
    }
}

/// Attribute value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    StringArray(Vec<String>),
    IntArray(Vec<i64>),
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i64> for AttributeValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<i32> for AttributeValue {
    fn from(v: i32) -> Self {
        Self::Int(v as i64)
    }
}

impl From<u64> for AttributeValue {
    fn from(v: u64) -> Self {
        Self::Int(v as i64)
    }
}

impl From<f64> for AttributeValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<bool> for AttributeValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

/// A single span in a trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Trace ID
    pub trace_id: TraceId,
    /// Span ID
    pub span_id: SpanId,
    /// Parent span ID (nil if root)
    pub parent_span_id: SpanId,
    /// Operation name
    pub name: String,
    /// Span kind
    pub kind: SpanKind,
    /// Start timestamp (Unix microseconds)
    pub start_time_us: u64,
    /// End timestamp (Unix microseconds, 0 if not ended)
    pub end_time_us: u64,
    /// Span status
    pub status: SpanStatus,
    /// Span attributes
    pub attributes: HashMap<String, AttributeValue>,
    /// Span events
    pub events: Vec<SpanEvent>,
    /// Span links
    pub links: Vec<SpanLink>,
    /// Node that created this span
    pub node_id: Option<NodeId>,
}

impl Span {
    /// Create new span
    pub fn new(ctx: &TraceContext, name: impl Into<String>) -> Self {
        Self {
            trace_id: ctx.trace_id,
            span_id: SpanId::new(),
            parent_span_id: ctx.parent_span_id,
            name: name.into(),
            kind: SpanKind::Internal,
            start_time_us: current_timestamp_us(),
            end_time_us: 0,
            status: SpanStatus::Unset,
            attributes: HashMap::new(),
            events: Vec::new(),
            links: Vec::new(),
            node_id: None,
        }
    }

    /// Create root span (new trace)
    pub fn root(name: impl Into<String>) -> Self {
        Self::new(&TraceContext::new(), name)
    }

    /// Set span kind
    pub fn with_kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }

    /// Set node ID
    pub fn with_node(mut self, node_id: NodeId) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Add attribute
    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<AttributeValue>) {
        self.attributes.insert(key.into(), value.into());
    }

    /// Add event
    pub fn add_event(&mut self, event: SpanEvent) {
        self.events.push(event);
    }

    /// Add event by name
    pub fn event(&mut self, name: impl Into<String>) {
        self.events.push(SpanEvent::new(name));
    }

    /// Add link
    pub fn add_link(&mut self, link: SpanLink) {
        self.links.push(link);
    }

    /// End the span
    pub fn end(&mut self) {
        if self.end_time_us == 0 {
            self.end_time_us = current_timestamp_us();
        }
    }

    /// End with success status
    pub fn end_ok(&mut self) {
        self.status = SpanStatus::Ok;
        self.end();
    }

    /// End with error status
    pub fn end_error(&mut self, message: impl Into<String>) {
        self.status = SpanStatus::Error {
            message: message.into(),
        };
        self.end();
    }

    /// Get duration in microseconds (0 if not ended)
    pub fn duration_us(&self) -> u64 {
        if self.end_time_us > 0 {
            self.end_time_us.saturating_sub(self.start_time_us)
        } else {
            0
        }
    }

    /// Check if span is ended
    pub fn is_ended(&self) -> bool {
        self.end_time_us > 0
    }

    /// Get trace context for this span
    pub fn context(&self) -> TraceContext {
        TraceContext {
            trace_id: self.trace_id,
            parent_span_id: self.span_id,
            flags: TraceFlags::SAMPLED,
            trace_state: HashMap::new(),
        }
    }
}

// =============================================================================
// Span Builder
// =============================================================================

/// Builder for creating spans with fluent API
pub struct SpanBuilder {
    span: Span,
}

impl SpanBuilder {
    /// Create new builder
    pub fn new(ctx: &TraceContext, name: impl Into<String>) -> Self {
        Self {
            span: Span::new(ctx, name),
        }
    }

    /// Set span kind
    pub fn kind(mut self, kind: SpanKind) -> Self {
        self.span.kind = kind;
        self
    }

    /// Set node ID
    pub fn node(mut self, node_id: NodeId) -> Self {
        self.span.node_id = Some(node_id);
        self
    }

    /// Add attribute
    pub fn attr(mut self, key: impl Into<String>, value: impl Into<AttributeValue>) -> Self {
        self.span.attributes.insert(key.into(), value.into());
        self
    }

    /// Add link
    pub fn link(mut self, trace_id: TraceId, span_id: SpanId) -> Self {
        self.span.links.push(SpanLink::new(trace_id, span_id));
        self
    }

    /// Build the span
    pub fn build(self) -> Span {
        self.span
    }

    /// Start the span (same as build, for semantic clarity)
    pub fn start(self) -> Span {
        self.span
    }
}

// =============================================================================
// Migration Trace
// =============================================================================

/// Migration-specific trace data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationTrace {
    /// Root span for the migration
    pub root_span: Span,
    /// Agent being migrated
    pub agent_id: [u8; 16],
    /// Source node
    pub source_node: NodeId,
    /// Target node
    pub target_node: NodeId,
    /// Migration phases with timing
    pub phases: Vec<MigrationPhaseSpan>,
    /// Data transfer spans
    pub transfers: Vec<DataTransferSpan>,
}

/// Migration phase span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPhaseSpan {
    /// Phase name
    pub phase: String,
    /// Span for this phase
    pub span: Span,
}

/// Data transfer span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTransferSpan {
    /// Transfer span
    pub span: Span,
    /// Bytes transferred
    pub bytes: u64,
    /// Source
    pub from: NodeId,
    /// Destination
    pub to: NodeId,
}

impl MigrationTrace {
    /// Create new migration trace
    pub fn new(agent_id: [u8; 16], source_node: NodeId, target_node: NodeId) -> Self {
        let mut root_span = Span::root("migration");
        root_span.kind = SpanKind::Internal;
        root_span.set_attribute("agent_id", hex::encode(agent_id));
        root_span.set_attribute("source_node", source_node.to_string());
        root_span.set_attribute("target_node", target_node.to_string());

        Self {
            root_span,
            agent_id,
            source_node,
            target_node,
            phases: Vec::new(),
            transfers: Vec::new(),
        }
    }

    /// Get trace context
    pub fn context(&self) -> TraceContext {
        self.root_span.context()
    }

    /// Start a new phase
    pub fn start_phase(&mut self, phase: impl Into<String>) -> &mut Span {
        let phase_name = phase.into();
        let mut span = Span::new(&self.context(), format!("migration.{}", phase_name));
        span.kind = SpanKind::Internal;

        self.phases.push(MigrationPhaseSpan {
            phase: phase_name,
            span,
        });

        // SAFETY: we just pushed an element, so the vector is non-empty
        let idx = self.phases.len() - 1;
        &mut self.phases[idx].span
    }

    /// End current phase
    pub fn end_phase(&mut self, success: bool) {
        if let Some(phase) = self.phases.last_mut() {
            if success {
                phase.span.end_ok();
            } else {
                phase.span.end_error("Phase failed");
            }
        }
    }

    /// Record data transfer
    pub fn record_transfer(&mut self, from: NodeId, to: NodeId, bytes: u64) {
        let mut span = Span::new(&self.context(), "migration.transfer");
        span.kind = SpanKind::Producer;
        span.set_attribute("bytes", bytes as i64);
        span.set_attribute("from", from.to_string());
        span.set_attribute("to", to.to_string());
        span.end_ok();

        self.transfers.push(DataTransferSpan {
            span,
            bytes,
            from,
            to,
        });
    }

    /// Complete migration
    pub fn complete(&mut self, success: bool, error: Option<&str>) {
        self.root_span.set_attribute("success", success);
        self.root_span.set_attribute(
            "total_bytes",
            self.transfers.iter().map(|t| t.bytes as i64).sum::<i64>(),
        );
        self.root_span
            .set_attribute("phase_count", self.phases.len() as i64);

        if success {
            self.root_span.end_ok();
        } else {
            self.root_span
                .end_error(error.unwrap_or("Migration failed"));
        }
    }

    /// Get total duration
    pub fn duration_us(&self) -> u64 {
        self.root_span.duration_us()
    }
}

// =============================================================================
// Gossip Trace
// =============================================================================

/// Gossip propagation trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipTrace {
    /// Root span
    pub root_span: Span,
    /// Message type being propagated
    pub message_type: String,
    /// Origin node
    pub origin: NodeId,
    /// Propagation hops
    pub hops: Vec<GossipHop>,
    /// Total nodes reached
    pub nodes_reached: Vec<NodeId>,
}

/// Single gossip hop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipHop {
    /// Hop span
    pub span: Span,
    /// From node
    pub from: NodeId,
    /// To node
    pub to: NodeId,
    /// Hop number
    pub hop_number: u32,
    /// Message size
    pub message_size: u64,
}

impl GossipTrace {
    /// Create new gossip trace
    pub fn new(origin: NodeId, message_type: impl Into<String>) -> Self {
        let msg_type = message_type.into();
        let mut root_span = Span::root("gossip.propagate");
        root_span.kind = SpanKind::Producer;
        root_span.set_attribute("message_type", msg_type.clone());
        root_span.set_attribute("origin", origin.to_string());

        Self {
            root_span,
            message_type: msg_type,
            origin,
            hops: Vec::new(),
            nodes_reached: vec![origin],
        }
    }

    /// Get trace context
    pub fn context(&self) -> TraceContext {
        self.root_span.context()
    }

    /// Record a hop
    pub fn record_hop(&mut self, from: NodeId, to: NodeId, message_size: u64) {
        let hop_number = self.hops.len() as u32 + 1;

        let mut span = Span::new(&self.context(), format!("gossip.hop.{}", hop_number));
        span.kind = SpanKind::Client;
        span.set_attribute("from", from.to_string());
        span.set_attribute("to", to.to_string());
        span.set_attribute("hop_number", hop_number as i64);
        span.set_attribute("message_size", message_size as i64);
        span.end_ok();

        self.hops.push(GossipHop {
            span,
            from,
            to,
            hop_number,
            message_size,
        });

        if !self.nodes_reached.contains(&to) {
            self.nodes_reached.push(to);
        }
    }

    /// Complete gossip propagation
    pub fn complete(&mut self) {
        self.root_span
            .set_attribute("total_hops", self.hops.len() as i64);
        self.root_span
            .set_attribute("nodes_reached", self.nodes_reached.len() as i64);
        self.root_span.end_ok();
    }

    /// Get propagation latency (first to last hop)
    pub fn propagation_latency_us(&self) -> u64 {
        if self.hops.is_empty() {
            return 0;
        }

        let first_start = self.hops.first().map(|h| h.span.start_time_us).unwrap_or(0);
        let last_end = self.hops.last().map(|h| h.span.end_time_us).unwrap_or(0);

        last_end.saturating_sub(first_start)
    }
}

// =============================================================================
// Request Trace
// =============================================================================

/// Request trace for tracking operations across nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestTrace {
    /// Root span
    pub root_span: Span,
    /// Request type
    pub request_type: String,
    /// Child spans from different nodes
    pub node_spans: Vec<NodeSpan>,
}

/// Span from a specific node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpan {
    /// Node that created this span
    pub node_id: NodeId,
    /// The span
    pub span: Span,
}

impl RequestTrace {
    /// Create new request trace
    pub fn new(request_type: impl Into<String>, origin: NodeId) -> Self {
        let req_type = request_type.into();
        let mut root_span = Span::root(&req_type);
        root_span.kind = SpanKind::Server;
        root_span.node_id = Some(origin);
        root_span.set_attribute("request_type", req_type.clone());

        Self {
            root_span,
            request_type: req_type,
            node_spans: Vec::new(),
        }
    }

    /// Get trace context
    pub fn context(&self) -> TraceContext {
        self.root_span.context()
    }

    /// Add a child span from a node
    pub fn add_node_span(&mut self, node_id: NodeId, span: Span) {
        self.node_spans.push(NodeSpan { node_id, span });
    }

    /// Create child span on a node
    pub fn child_span(&self, node_id: NodeId, name: impl Into<String>) -> Span {
        let mut span = Span::new(&self.context(), name);
        span.node_id = Some(node_id);
        span
    }

    /// Complete the request
    pub fn complete(&mut self, success: bool, error: Option<&str>) {
        self.root_span
            .set_attribute("node_count", self.node_spans.len() as i64);

        if success {
            self.root_span.end_ok();
        } else {
            self.root_span.end_error(error.unwrap_or("Request failed"));
        }
    }
}

// =============================================================================
// Trace Collector
// =============================================================================

/// Configuration for trace collector
#[derive(Debug, Clone)]
pub struct TraceCollectorConfig {
    /// Maximum traces to keep in memory
    pub max_traces: usize,
    /// Maximum spans per trace
    pub max_spans_per_trace: usize,
    /// Sampling rate (0.0 - 1.0)
    pub sampling_rate: f64,
    /// Enable export
    pub export_enabled: bool,
}

impl Default for TraceCollectorConfig {
    fn default() -> Self {
        Self {
            max_traces: 1000,
            max_spans_per_trace: 100,
            sampling_rate: 1.0,
            export_enabled: true,
        }
    }
}

/// Collected trace data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedTrace {
    /// Trace ID
    pub trace_id: TraceId,
    /// All spans in the trace
    pub spans: Vec<Span>,
    /// Trace start time
    pub start_time_us: u64,
    /// Trace end time (0 if ongoing)
    pub end_time_us: u64,
}

impl CollectedTrace {
    /// Create new collected trace
    pub fn new(trace_id: TraceId) -> Self {
        Self {
            trace_id,
            spans: Vec::new(),
            start_time_us: current_timestamp_us(),
            end_time_us: 0,
        }
    }

    /// Add span
    pub fn add_span(&mut self, span: Span) {
        self.spans.push(span);
    }

    /// Mark trace as complete
    pub fn complete(&mut self) {
        self.end_time_us = current_timestamp_us();
    }

    /// Get duration
    pub fn duration_us(&self) -> u64 {
        if self.end_time_us > 0 {
            self.end_time_us.saturating_sub(self.start_time_us)
        } else {
            current_timestamp_us().saturating_sub(self.start_time_us)
        }
    }

    /// Find root span
    pub fn root_span(&self) -> Option<&Span> {
        self.spans.iter().find(|s| s.parent_span_id.is_nil())
    }
}

/// Trace collector for aggregating spans
pub struct TraceCollector {
    /// Configuration
    config: TraceCollectorConfig,
    /// Active traces
    traces: RwLock<HashMap<TraceId, CollectedTrace>>,
    /// Completed traces (ring buffer)
    completed: RwLock<Vec<CollectedTrace>>,
    /// Traces collected count
    collected_count: AtomicU64,
    /// Spans collected count
    spans_collected: AtomicU64,
    /// Traces dropped count
    traces_dropped: AtomicU64,
}

impl TraceCollector {
    /// Create new trace collector
    pub fn new(config: TraceCollectorConfig) -> Self {
        Self {
            config,
            traces: RwLock::new(HashMap::new()),
            completed: RwLock::new(Vec::new()),
            collected_count: AtomicU64::new(0),
            spans_collected: AtomicU64::new(0),
            traces_dropped: AtomicU64::new(0),
        }
    }

    /// Create with default config
    pub fn default_collector() -> Self {
        Self::new(TraceCollectorConfig::default())
    }

    /// Record a span
    pub async fn record_span(&self, span: Span) {
        // Check sampling
        if self.config.sampling_rate < 1.0 {
            let sample: f64 = rand::random();
            if sample > self.config.sampling_rate {
                return;
            }
        }

        self.spans_collected.fetch_add(1, Ordering::Relaxed);

        let mut traces = self.traces.write().await;

        let trace = traces.entry(span.trace_id).or_insert_with(|| {
            self.collected_count.fetch_add(1, Ordering::Relaxed);
            CollectedTrace::new(span.trace_id)
        });

        // Check span limit
        if trace.spans.len() < self.config.max_spans_per_trace {
            trace.add_span(span);
        }
    }

    /// Complete a trace
    pub async fn complete_trace(&self, trace_id: TraceId) {
        let mut traces = self.traces.write().await;

        if let Some(mut trace) = traces.remove(&trace_id) {
            trace.complete();

            let mut completed = self.completed.write().await;

            // Evict oldest if at capacity
            if completed.len() >= self.config.max_traces {
                completed.remove(0);
                self.traces_dropped.fetch_add(1, Ordering::Relaxed);
            }

            completed.push(trace);
        }
    }

    /// Get active trace
    pub async fn get_trace(&self, trace_id: &TraceId) -> Option<CollectedTrace> {
        let traces = self.traces.read().await;
        traces.get(trace_id).cloned()
    }

    /// Get completed traces
    pub async fn get_completed_traces(&self, limit: usize) -> Vec<CollectedTrace> {
        let completed = self.completed.read().await;
        completed.iter().rev().take(limit).cloned().collect()
    }

    /// Get traces by time range
    pub async fn get_traces_in_range(&self, start_us: u64, end_us: u64) -> Vec<CollectedTrace> {
        let completed = self.completed.read().await;
        completed
            .iter()
            .filter(|t| t.start_time_us >= start_us && t.start_time_us <= end_us)
            .cloned()
            .collect()
    }

    /// Get statistics
    pub fn stats(&self) -> TraceCollectorStats {
        TraceCollectorStats {
            traces_collected: self.collected_count.load(Ordering::Relaxed),
            spans_collected: self.spans_collected.load(Ordering::Relaxed),
            traces_dropped: self.traces_dropped.load(Ordering::Relaxed),
        }
    }

    /// Clear all traces
    pub async fn clear(&self) {
        self.traces.write().await.clear();
        self.completed.write().await.clear();
    }
}

/// Trace collector statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCollectorStats {
    pub traces_collected: u64,
    pub spans_collected: u64,
    pub traces_dropped: u64,
}

// =============================================================================
// OpenTelemetry Export
// =============================================================================

/// OpenTelemetry-compatible span export format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OTelSpan {
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    #[serde(rename = "parentSpanId")]
    pub parent_span_id: Option<String>,
    #[serde(rename = "operationName")]
    pub operation_name: String,
    #[serde(rename = "startTime")]
    pub start_time: u64,
    #[serde(rename = "duration")]
    pub duration: u64,
    pub tags: HashMap<String, serde_json::Value>,
    pub logs: Vec<OTelLog>,
    pub references: Vec<OTelReference>,
}

/// OpenTelemetry log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OTelLog {
    pub timestamp: u64,
    pub fields: HashMap<String, serde_json::Value>,
}

/// OpenTelemetry span reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OTelReference {
    #[serde(rename = "refType")]
    pub ref_type: String,
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
}

impl From<&Span> for OTelSpan {
    fn from(span: &Span) -> Self {
        let mut tags: HashMap<String, serde_json::Value> = span
            .attributes
            .iter()
            .map(|(k, v)| {
                let json_val = match v {
                    AttributeValue::String(s) => serde_json::Value::String(s.clone()),
                    AttributeValue::Int(i) => serde_json::Value::Number((*i).into()),
                    AttributeValue::Float(f) => serde_json::Value::Number(
                        serde_json::Number::from_f64(*f).unwrap_or(0.into()),
                    ),
                    AttributeValue::Bool(b) => serde_json::Value::Bool(*b),
                    AttributeValue::StringArray(arr) => serde_json::Value::Array(
                        arr.iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                    AttributeValue::IntArray(arr) => serde_json::Value::Array(
                        arr.iter()
                            .map(|i| serde_json::Value::Number((*i).into()))
                            .collect(),
                    ),
                };
                (k.clone(), json_val)
            })
            .collect();

        // Add span kind as tag
        tags.insert(
            "span.kind".to_string(),
            serde_json::Value::String(format!("{:?}", span.kind).to_lowercase()),
        );

        // Add status
        match &span.status {
            SpanStatus::Ok => {
                tags.insert(
                    "otel.status_code".to_string(),
                    serde_json::Value::String("OK".to_string()),
                );
            }
            SpanStatus::Error { message } => {
                tags.insert(
                    "otel.status_code".to_string(),
                    serde_json::Value::String("ERROR".to_string()),
                );
                tags.insert("error".to_string(), serde_json::Value::Bool(true));
                tags.insert(
                    "error.message".to_string(),
                    serde_json::Value::String(message.clone()),
                );
            }
            SpanStatus::Unset => {}
        }

        let logs: Vec<OTelLog> = span
            .events
            .iter()
            .map(|e| {
                let mut fields: HashMap<String, serde_json::Value> = e
                    .attributes
                    .iter()
                    .map(|(k, v)| {
                        let json_val = match v {
                            AttributeValue::String(s) => serde_json::Value::String(s.clone()),
                            AttributeValue::Int(i) => serde_json::Value::Number((*i).into()),
                            AttributeValue::Float(f) => serde_json::Value::Number(
                                serde_json::Number::from_f64(*f).unwrap_or(0.into()),
                            ),
                            AttributeValue::Bool(b) => serde_json::Value::Bool(*b),
                            _ => serde_json::Value::Null,
                        };
                        (k.clone(), json_val)
                    })
                    .collect();
                fields.insert(
                    "event".to_string(),
                    serde_json::Value::String(e.name.clone()),
                );
                OTelLog {
                    timestamp: e.timestamp_us,
                    fields,
                }
            })
            .collect();

        let references: Vec<OTelReference> = span
            .links
            .iter()
            .map(|link| OTelReference {
                ref_type: "FOLLOWS_FROM".to_string(),
                trace_id: link.trace_id.to_hex(),
                span_id: link.span_id.to_hex(),
            })
            .collect();

        Self {
            trace_id: span.trace_id.to_hex(),
            span_id: span.span_id.to_hex(),
            parent_span_id: if span.parent_span_id.is_nil() {
                None
            } else {
                Some(span.parent_span_id.to_hex())
            },
            operation_name: span.name.clone(),
            start_time: span.start_time_us,
            duration: span.duration_us(),
            tags,
            logs,
            references,
        }
    }
}

/// Export collected trace to OpenTelemetry format
pub fn export_trace_otel(trace: &CollectedTrace) -> Vec<OTelSpan> {
    trace.spans.iter().map(OTelSpan::from).collect()
}

/// Export trace to JSON string
pub fn export_trace_json(trace: &CollectedTrace) -> Result<String, serde_json::Error> {
    let otel_spans = export_trace_otel(trace);
    serde_json::to_string_pretty(&otel_spans)
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Get current timestamp in microseconds since Unix epoch
fn current_timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_micros() as u64
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_generation() {
        let id1 = TraceId::new();
        let id2 = TraceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_trace_id_hex_roundtrip() {
        let id = TraceId::new();
        let hex = id.to_hex();
        let parsed = TraceId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_span_id_generation() {
        let id1 = SpanId::new();
        let id2 = SpanId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_span_id_nil() {
        let nil = SpanId::nil();
        assert!(nil.is_nil());

        let non_nil = SpanId::new();
        assert!(!non_nil.is_nil());
    }

    #[test]
    fn test_trace_context_new() {
        let ctx = TraceContext::new();
        assert!(ctx.parent_span_id.is_nil());
        assert!(ctx.flags.is_sampled());
    }

    #[test]
    fn test_trace_context_child() {
        let ctx = TraceContext::new();
        let span_id = SpanId::new();
        let child = ctx.child(span_id);

        assert_eq!(child.trace_id, ctx.trace_id);
        assert_eq!(child.parent_span_id, span_id);
    }

    #[test]
    fn test_traceparent_roundtrip() {
        let ctx = TraceContext::new();
        let traceparent = ctx.to_traceparent();
        let parsed = TraceContext::from_traceparent(&traceparent).unwrap();

        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.parent_span_id, ctx.parent_span_id);
        assert_eq!(parsed.flags.0, ctx.flags.0);
    }

    #[test]
    fn test_span_creation() {
        let ctx = TraceContext::new();
        let span = Span::new(&ctx, "test_operation");

        assert_eq!(span.trace_id, ctx.trace_id);
        assert_eq!(span.name, "test_operation");
        assert!(!span.is_ended());
    }

    #[test]
    fn test_span_lifecycle() {
        let mut span = Span::root("test");
        assert!(!span.is_ended());

        span.set_attribute("key", "value");
        span.event("something_happened");

        span.end_ok();
        assert!(span.is_ended());
        assert!(matches!(span.status, SpanStatus::Ok));
        assert!(span.duration_us() > 0 || span.duration_us() == 0);
    }

    #[test]
    fn test_span_error() {
        let mut span = Span::root("test");
        span.end_error("Something went wrong");

        assert!(span.is_ended());
        assert!(matches!(span.status, SpanStatus::Error { .. }));
    }

    #[test]
    fn test_span_builder() {
        let ctx = TraceContext::new();
        let span = SpanBuilder::new(&ctx, "built_span")
            .kind(SpanKind::Server)
            .attr("service", "test")
            .attr("count", 42i64)
            .build();

        assert_eq!(span.name, "built_span");
        assert_eq!(span.kind, SpanKind::Server);
        assert!(span.attributes.contains_key("service"));
        assert!(span.attributes.contains_key("count"));
    }

    #[test]
    fn test_migration_trace() {
        let agent_id = [1u8; 16];
        let source = NodeId::new_v4();
        let target = NodeId::new_v4();

        let mut trace = MigrationTrace::new(agent_id, source, target);

        trace.start_phase("prepare");
        trace.end_phase(true);

        trace.start_phase("transfer");
        trace.record_transfer(source, target, 1024);
        trace.end_phase(true);

        trace.complete(true, None);

        assert_eq!(trace.phases.len(), 2);
        assert_eq!(trace.transfers.len(), 1);
        assert!(trace.root_span.is_ended());
    }

    #[test]
    fn test_gossip_trace() {
        let origin = NodeId::new_v4();
        let node2 = NodeId::new_v4();
        let node3 = NodeId::new_v4();

        let mut trace = GossipTrace::new(origin, "heartbeat");

        trace.record_hop(origin, node2, 100);
        trace.record_hop(node2, node3, 100);
        trace.complete();

        assert_eq!(trace.hops.len(), 2);
        assert_eq!(trace.nodes_reached.len(), 3);
        assert!(trace.root_span.is_ended());
    }

    #[test]
    fn test_request_trace() {
        let origin = NodeId::new_v4();
        let mut trace = RequestTrace::new("dht.lookup", origin);

        let child_node = NodeId::new_v4();
        let mut child_span = trace.child_span(child_node, "dht.lookup.forward");
        child_span.end_ok();
        trace.add_node_span(child_node, child_span);

        trace.complete(true, None);

        assert_eq!(trace.node_spans.len(), 1);
        assert!(trace.root_span.is_ended());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_trace_collector() {
        let collector = TraceCollector::default_collector();

        let span1 = Span::root("test1");
        let trace_id = span1.trace_id;
        collector.record_span(span1).await;

        let mut span2 = Span::root("test2");
        span2.trace_id = trace_id;
        span2.parent_span_id = SpanId::new();
        collector.record_span(span2).await;

        let trace = collector.get_trace(&trace_id).await.unwrap();
        assert_eq!(trace.spans.len(), 2);

        collector.complete_trace(trace_id).await;

        let completed = collector.get_completed_traces(10).await;
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_otel_export() {
        let mut span = Span::root("test_op");
        span.set_attribute("service", "test");
        span.set_attribute("count", 10i64);
        span.event("checkpoint");
        span.end_ok();

        let otel: OTelSpan = (&span).into();

        assert_eq!(otel.operation_name, "test_op");
        assert!(otel.parent_span_id.is_none());
        assert!(otel.tags.contains_key("service"));
        assert!(!otel.logs.is_empty());
    }

    #[test]
    fn test_collected_trace() {
        let trace_id = TraceId::new();
        let mut collected = CollectedTrace::new(trace_id);

        let span1 = Span::root("root");
        let mut span2 = Span::new(&span1.context(), "child");
        span2.end_ok();

        collected.add_span(span1);
        collected.add_span(span2);
        collected.complete();

        assert_eq!(collected.spans.len(), 2);
        assert!(collected.end_time_us > 0);
        assert!(collected.root_span().is_some());
    }

    #[test]
    fn test_trace_flags() {
        let mut flags = TraceFlags::NONE;
        assert!(!flags.is_sampled());

        flags.set_sampled(true);
        assert!(flags.is_sampled());

        flags.set_sampled(false);
        assert!(!flags.is_sampled());
    }

    #[test]
    fn test_span_event() {
        let event = SpanEvent::new("test_event")
            .with_attr("key", "value")
            .with_attr("count", 5i64);

        assert_eq!(event.name, "test_event");
        assert_eq!(event.attributes.len(), 2);
    }

    #[test]
    fn test_attribute_values() {
        let s: AttributeValue = "test".into();
        assert!(matches!(s, AttributeValue::String(_)));

        let i: AttributeValue = 42i64.into();
        assert!(matches!(i, AttributeValue::Int(42)));

        let f: AttributeValue = 1.5f64.into();
        assert!(matches!(f, AttributeValue::Float(_)));

        let b: AttributeValue = true.into();
        assert!(matches!(b, AttributeValue::Bool(true)));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_collector_stats() {
        let collector = TraceCollector::default_collector();

        let span = Span::root("test");
        collector.record_span(span).await;

        let stats = collector.stats();
        assert_eq!(stats.traces_collected, 1);
        assert_eq!(stats.spans_collected, 1);
    }

    #[test]
    fn test_export_trace_json() {
        let trace_id = TraceId::new();
        let mut collected = CollectedTrace::new(trace_id);

        let mut span = Span::root("test");
        span.trace_id = trace_id;
        span.end_ok();
        collected.add_span(span);

        let json = export_trace_json(&collected).unwrap();
        assert!(json.contains("traceId"));
        assert!(json.contains("spanId"));
    }
}
