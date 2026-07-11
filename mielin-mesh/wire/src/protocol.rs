//! Protocol State Machine with Extension Registry
//!
//! Provides a full protocol handler for the MielinMesh wire protocol:
//! - Version/capability negotiation handshake
//! - Extension registration and message routing
//! - Three built-in extensions: Echo, Ping, and Metadata
//! - Statistics tracking

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Message, WireError};

// ---------------------------------------------------------------------------
// Protocol version
// ---------------------------------------------------------------------------

/// Semantic version for protocol capability negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl ProtocolVersion {
    pub const CURRENT: Self = Self {
        major: 0,
        minor: 1,
        patch: 0,
    };

    pub const fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Two versions are wire-compatible when they share the same major version.
    pub fn is_compatible_with(self, other: Self) -> bool {
        self.major == other.major
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("incompatible protocol version: expected major {expected_major}, got {got_major}")]
    IncompatibleVersion { expected_major: u8, got_major: u8 },

    #[error("extension not found: {name}")]
    ExtensionNotFound { name: String },

    #[error("extension already registered: {name}")]
    ExtensionAlreadyRegistered { name: String },

    #[error("message decode error: {details}")]
    DecodeError { details: String },

    #[error("extension handler returned error: {details}")]
    HandlerError { details: String },

    #[error("negotiation failed: {reason}")]
    NegotiationFailed { reason: String },

    #[error("max extensions exceeded: {max}")]
    MaxExtensionsExceeded { max: usize },

    #[error("handshake not yet established")]
    HandshakeNotEstablished,

    #[error("nonce mismatch: expected {expected}, got {got}")]
    NonceMismatch { expected: u64, got: u64 },

    #[error("failed to generate cryptographically secure nonce: {details}")]
    RngFailure { details: String },
}

impl From<ProtocolError> for WireError {
    fn from(e: ProtocolError) -> Self {
        WireError::InvalidInput(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Handshake messages
// ---------------------------------------------------------------------------

/// Capability advertised during capability negotiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: ProtocolVersion,
    /// Extension-specific feature flags (bit field).
    pub flags: u32,
}

impl Capability {
    pub fn new(name: impl Into<String>, version: ProtocolVersion) -> Self {
        Self {
            name: name.into(),
            version,
            flags: 0,
        }
    }

    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }
}

/// ClientHello equivalent — sent by the initiating side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloMessage {
    pub protocol_version: ProtocolVersion,
    pub node_id: String,
    pub capabilities: Vec<Capability>,
    /// Random value to prevent replay; must be echoed in the HelloAck.
    pub nonce: u64,
    /// Milliseconds since Unix epoch (informational).
    pub timestamp_ms: u64,
}

/// ServerHello + Finished equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloAckMessage {
    /// Whether this peer accepts the connection.
    pub accepted: bool,
    pub remote_version: ProtocolVersion,
    /// Names of capabilities accepted on both sides.
    pub accepted_capabilities: Vec<String>,
    /// Human-readable rejection reason when `accepted == false`.
    pub reason: Option<String>,
    /// Echo of the hello nonce.
    pub reply_nonce: u64,
}

/// Envelope for all extension-specific messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMessage {
    pub extension_name: String,
    pub sequence: u64,
    pub payload: Vec<u8>,
    /// Extension-defined bit flags.
    pub flags: u32,
}

impl ExtensionMessage {
    pub fn new(extension_name: impl Into<String>, sequence: u64, payload: Vec<u8>) -> Self {
        Self {
            extension_name: extension_name.into(),
            sequence,
            payload,
            flags: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Extension trait
// ---------------------------------------------------------------------------

/// Implementors extend the wire protocol with custom message handling.
pub trait ProtocolExtension: Send + Sync {
    /// Unique name; used as a routing key.
    fn name(&self) -> &str;

    /// Minimum protocol version required for this extension.
    fn required_version(&self) -> ProtocolVersion;

    /// Handle an incoming extension message, optionally producing a reply.
    fn handle(&self, msg: &ExtensionMessage) -> Result<Option<ExtensionMessage>, ProtocolError>;

    /// Called when a remote peer confirms support for this extension.
    fn on_capability_negotiated(&self, _remote: &Capability) {}
}

// ---------------------------------------------------------------------------
// Handshake state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    Uninitiated,
    HelloSent { nonce: u64 },
    Established { remote_version: ProtocolVersion },
    Failed,
}

// ---------------------------------------------------------------------------
// Protocol statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtocolStats {
    pub messages_processed: u64,
    pub extension_messages_routed: u64,
    pub hello_messages: u64,
    pub hello_acks: u64,
    pub errors: u64,
    pub bytes_processed: u64,
}

// ---------------------------------------------------------------------------
// ProtocolHandler
// ---------------------------------------------------------------------------

/// Maximum number of concurrently registered extensions.
const DEFAULT_MAX_EXTENSIONS: usize = 64;

/// Full protocol handler: manages the version handshake and routes extension
/// messages to registered handlers.
pub struct ProtocolHandler {
    version: ProtocolVersion,
    extensions: HashMap<String, Box<dyn ProtocolExtension>>,
    stats: ProtocolStats,
    handshake_state: HandshakeState,
    max_extensions: usize,
}

impl ProtocolHandler {
    pub fn new() -> Self {
        Self::with_version(ProtocolVersion::CURRENT)
    }

    pub fn with_version(version: ProtocolVersion) -> Self {
        Self {
            version,
            extensions: HashMap::new(),
            stats: ProtocolStats::default(),
            handshake_state: HandshakeState::Uninitiated,
            max_extensions: DEFAULT_MAX_EXTENSIONS,
        }
    }

    // ── Extension management ────────────────────────────────────────────────

    /// Register an extension handler.  Returns an error if one with the same
    /// name is already registered or the maximum count would be exceeded.
    pub fn register_extension(
        &mut self,
        ext: Box<dyn ProtocolExtension>,
    ) -> Result<(), ProtocolError> {
        if self.extensions.len() >= self.max_extensions {
            return Err(ProtocolError::MaxExtensionsExceeded {
                max: self.max_extensions,
            });
        }
        let name = ext.name().to_owned();
        if self.extensions.contains_key(&name) {
            return Err(ProtocolError::ExtensionAlreadyRegistered { name });
        }
        self.extensions.insert(name, ext);
        Ok(())
    }

    /// Unregister an extension by name.
    pub fn unregister_extension(&mut self, name: &str) -> Result<(), ProtocolError> {
        if self.extensions.remove(name).is_none() {
            return Err(ProtocolError::ExtensionNotFound {
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    /// Returns the names of all registered extensions.
    pub fn extension_names(&self) -> Vec<&str> {
        self.extensions.keys().map(String::as_str).collect()
    }

    // ── Handshake ───────────────────────────────────────────────────────────

    /// Build a `HelloMessage` advertising this handler's version and all
    /// registered extension capabilities.
    ///
    /// The nonce is drawn from a cryptographically secure random source
    /// (`oxicrypto-rand`) rather than a predictable counter, since it is
    /// relied upon for handshake anti-replay/liveness guarantees.
    pub fn build_hello(&mut self, node_id: &str) -> Result<HelloMessage, ProtocolError> {
        let nonce = Self::next_nonce()?;
        self.handshake_state = HandshakeState::HelloSent { nonce };
        self.stats.hello_messages += 1;

        let capabilities: Vec<Capability> = self
            .extensions
            .values()
            .map(|ext| Capability::new(ext.name(), ext.required_version()))
            .collect();

        Ok(HelloMessage {
            protocol_version: self.version,
            node_id: node_id.to_owned(),
            capabilities,
            nonce,
            timestamp_ms: 0, // caller may fill in wall-clock time
        })
    }

    /// Process an incoming `HelloMessage`; returns the ack to send back.
    /// Transitions to `Established` on success.
    pub fn process_hello(
        &mut self,
        hello: &HelloMessage,
    ) -> Result<HelloAckMessage, ProtocolError> {
        self.stats.hello_messages += 1;

        if !self.version.is_compatible_with(hello.protocol_version) {
            self.handshake_state = HandshakeState::Failed;
            self.stats.errors += 1;
            return Err(ProtocolError::IncompatibleVersion {
                expected_major: self.version.major,
                got_major: hello.protocol_version.major,
            });
        }

        // Compute mutually supported capabilities.
        let accepted_capabilities: Vec<String> = hello
            .capabilities
            .iter()
            .filter(|cap| self.extensions.contains_key(cap.name.as_str()))
            .map(|cap| {
                if let Some(ext) = self.extensions.get(cap.name.as_str()) {
                    ext.on_capability_negotiated(cap);
                }
                cap.name.clone()
            })
            .collect();

        self.handshake_state = HandshakeState::Established {
            remote_version: hello.protocol_version,
        };

        let ack = HelloAckMessage {
            accepted: true,
            remote_version: self.version,
            accepted_capabilities,
            reason: None,
            reply_nonce: hello.nonce,
        };
        self.stats.hello_acks += 1;
        Ok(ack)
    }

    /// Process a `HelloAckMessage` (the initiating side calls this after
    /// receiving the responder's ack).  Verifies nonce and transitions to
    /// `Established`.
    pub fn process_hello_ack(&mut self, ack: &HelloAckMessage) -> Result<(), ProtocolError> {
        self.stats.hello_acks += 1;

        let expected_nonce = match self.handshake_state {
            HandshakeState::HelloSent { nonce } => nonce,
            _ => {
                self.stats.errors += 1;
                return Err(ProtocolError::HandshakeNotEstablished);
            }
        };

        if !ack.accepted {
            self.handshake_state = HandshakeState::Failed;
            self.stats.errors += 1;
            return Err(ProtocolError::NegotiationFailed {
                reason: ack
                    .reason
                    .clone()
                    .unwrap_or_else(|| "remote rejected connection".into()),
            });
        }

        if ack.reply_nonce != expected_nonce {
            self.handshake_state = HandshakeState::Failed;
            self.stats.errors += 1;
            return Err(ProtocolError::NonceMismatch {
                expected: expected_nonce,
                got: ack.reply_nonce,
            });
        }

        // Notify matching local extensions of the negotiated remote caps.
        for cap_name in &ack.accepted_capabilities {
            if let Some(ext) = self.extensions.get(cap_name.as_str()) {
                ext.on_capability_negotiated(&Capability::new(
                    cap_name.clone(),
                    ack.remote_version,
                ));
            }
        }

        self.handshake_state = HandshakeState::Established {
            remote_version: ack.remote_version,
        };
        Ok(())
    }

    // ── Extension routing ───────────────────────────────────────────────────

    /// Route an `ExtensionMessage` to the appropriate registered handler.
    pub fn route_extension_message(
        &mut self,
        msg: &ExtensionMessage,
    ) -> Result<Option<ExtensionMessage>, ProtocolError> {
        let ext = self
            .extensions
            .get(msg.extension_name.as_str())
            .ok_or_else(|| ProtocolError::ExtensionNotFound {
                name: msg.extension_name.clone(),
            })?;

        self.stats.extension_messages_routed += 1;
        self.stats.bytes_processed += msg.payload.len() as u64;

        ext.handle(msg).map_err(|e| ProtocolError::HandlerError {
            details: e.to_string(),
        })
    }

    /// Dispatch a generic `Message` enum value.  Extension-type messages
    /// must be represented as `Message::LoadInfo` for now (placeholder —
    /// callers that need extension routing should use `route_extension_message`
    /// directly).
    pub fn handle_message(&mut self, msg: Message) -> Result<Option<Message>, WireError> {
        self.stats.messages_processed += 1;
        self.stats.bytes_processed += 8; // approximate overhead

        // Route known ping/pong for convenience.
        let reply = match msg {
            Message::Ping { timestamp } => Some(Message::Pong {
                timestamp,
                latency_ms: 0,
            }),
            _ => None,
        };
        Ok(reply)
    }

    // ── Accessors ───────────────────────────────────────────────────────────

    pub fn stats(&self) -> &ProtocolStats {
        &self.stats
    }

    pub fn handshake_state(&self) -> HandshakeState {
        self.handshake_state
    }

    pub fn is_established(&self) -> bool {
        matches!(self.handshake_state, HandshakeState::Established { .. })
    }

    pub fn version(&self) -> ProtocolVersion {
        self.version
    }

    // ── Internals ───────────────────────────────────────────────────────────

    /// Draw a fresh handshake nonce from a cryptographically secure random
    /// source. Predictable (e.g. counter-based) nonces would defeat the
    /// anti-replay/liveness purpose of the value.
    fn next_nonce() -> Result<u64, ProtocolError> {
        use oxicrypto_core::Rng;

        let mut rng = oxicrypto_rand::OxiRng::new().map_err(|e| ProtocolError::RngFailure {
            details: format!("OxiRng initialisation failed: {e}"),
        })?;
        let mut bytes = [0u8; 8];
        rng.fill(&mut bytes)
            .map_err(|e| ProtocolError::RngFailure {
                details: format!("OxiRng fill failed: {e}"),
            })?;
        Ok(u64::from_le_bytes(bytes))
    }
}

impl Default for ProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in extensions
// ---------------------------------------------------------------------------

/// Echo extension — returns every payload verbatim with flag bit 0 set.
/// Useful for round-trip testing and latency baseline measurements.
pub struct EchoExtension;

impl ProtocolExtension for EchoExtension {
    fn name(&self) -> &str {
        "echo"
    }

    fn required_version(&self) -> ProtocolVersion {
        ProtocolVersion::CURRENT
    }

    fn handle(&self, msg: &ExtensionMessage) -> Result<Option<ExtensionMessage>, ProtocolError> {
        Ok(Some(ExtensionMessage {
            extension_name: msg.extension_name.clone(),
            sequence: msg.sequence,
            payload: msg.payload.clone(),
            flags: msg.flags | 0x1, // echo flag
        }))
    }
}

// ---------------------------------------------------------------------------

/// Ping extension — measures round-trip latency.
///
/// Payload layout:
/// - bytes 0..8  : send timestamp (u64 LE)
///
/// Pong reply layout (flags bit 0 = pong indicator):
/// - bytes 0..8  : original send timestamp
/// - bytes 8..16 : arrival timestamp (u64 LE, filled by responder)
pub struct PingExtension {
    ping_count: Arc<AtomicU64>,
    pong_count: Arc<AtomicU64>,
    start: Instant,
}

impl PingExtension {
    pub fn new() -> Self {
        Self {
            ping_count: Arc::new(AtomicU64::new(0)),
            pong_count: Arc::new(AtomicU64::new(0)),
            start: Instant::now(),
        }
    }

    /// Build a ping payload using elapsed microseconds as the timestamp.
    pub fn build_ping_payload(&self) -> Vec<u8> {
        let ts = self.start.elapsed().as_micros() as u64;
        ts.to_le_bytes().to_vec()
    }

    pub fn ping_count(&self) -> u64 {
        self.ping_count.load(Ordering::Relaxed)
    }

    pub fn pong_count(&self) -> u64 {
        self.pong_count.load(Ordering::Relaxed)
    }
}

impl Default for PingExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolExtension for PingExtension {
    fn name(&self) -> &str {
        "ping"
    }

    fn required_version(&self) -> ProtocolVersion {
        ProtocolVersion::CURRENT
    }

    fn handle(&self, msg: &ExtensionMessage) -> Result<Option<ExtensionMessage>, ProtocolError> {
        if msg.flags & 0x1 != 0 {
            // This is already a pong reply.
            self.pong_count.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }

        self.ping_count.fetch_add(1, Ordering::Relaxed);

        if msg.payload.len() < 8 {
            return Err(ProtocolError::DecodeError {
                details: "ping payload must be ≥8 bytes".into(),
            });
        }

        let arrival_ts = self.start.elapsed().as_micros() as u64;

        let mut pong_payload = Vec::with_capacity(16);
        pong_payload.extend_from_slice(&msg.payload[..8]); // original send ts
        pong_payload.extend_from_slice(&arrival_ts.to_le_bytes()); // arrival ts

        Ok(Some(ExtensionMessage {
            extension_name: msg.extension_name.clone(),
            sequence: msg.sequence,
            payload: pong_payload,
            flags: 0x1, // pong flag
        }))
    }
}

// ---------------------------------------------------------------------------

/// Metadata extension — key-value metadata exchange between peers.
///
/// Payload: JSON-serialized `HashMap<String, String>`.
/// On receive the remote peer's metadata is stored and queryable via
/// `remote_get()`.
pub struct MetadataExtension {
    local_metadata: RwLock<HashMap<String, String>>,
    remote_metadata: RwLock<HashMap<String, String>>,
}

impl MetadataExtension {
    pub fn new() -> Self {
        Self {
            local_metadata: RwLock::new(HashMap::new()),
            remote_metadata: RwLock::new(HashMap::new()),
        }
    }

    /// Set a local metadata entry.
    pub fn set(&self, key: impl Into<String>, value: impl Into<String>) {
        if let Ok(mut m) = self.local_metadata.write() {
            m.insert(key.into(), value.into());
        }
    }

    /// Get a local metadata entry.
    pub fn get(&self, key: &str) -> Option<String> {
        self.local_metadata
            .read()
            .ok()
            .and_then(|m| m.get(key).cloned())
    }

    /// Get a remote peer's metadata entry (populated after receiving a metadata
    /// message from them).
    pub fn remote_get(&self, key: &str) -> Option<String> {
        self.remote_metadata
            .read()
            .ok()
            .and_then(|m| m.get(key).cloned())
    }

    /// Serialize current local metadata for sending.
    pub fn build_payload(&self) -> Result<Vec<u8>, ProtocolError> {
        let m = self
            .local_metadata
            .read()
            .map_err(|e| ProtocolError::HandlerError {
                details: e.to_string(),
            })?;
        serde_json::to_vec(&*m).map_err(|e| ProtocolError::DecodeError {
            details: e.to_string(),
        })
    }
}

impl Default for MetadataExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolExtension for MetadataExtension {
    fn name(&self) -> &str {
        "metadata"
    }

    fn required_version(&self) -> ProtocolVersion {
        ProtocolVersion::CURRENT
    }

    fn handle(&self, msg: &ExtensionMessage) -> Result<Option<ExtensionMessage>, ProtocolError> {
        let incoming: HashMap<String, String> =
            serde_json::from_slice(&msg.payload).map_err(|e| ProtocolError::DecodeError {
                details: e.to_string(),
            })?;

        if let Ok(mut rm) = self.remote_metadata.write() {
            rm.extend(incoming);
        }

        // No reply needed — metadata exchange is fire-and-forget.
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProtocolVersion ─────────────────────────────────────────────────────

    #[test]
    fn test_protocol_version_compatibility_same_major() {
        let a = ProtocolVersion::new(1, 0, 0);
        let b = ProtocolVersion::new(1, 3, 5);
        assert!(a.is_compatible_with(b));
        assert!(b.is_compatible_with(a));
    }

    #[test]
    fn test_protocol_version_incompatible_different_major() {
        let a = ProtocolVersion::new(1, 0, 0);
        let b = ProtocolVersion::new(2, 0, 0);
        assert!(!a.is_compatible_with(b));
    }

    #[test]
    fn test_protocol_version_ordering() {
        let v100 = ProtocolVersion::new(1, 0, 0);
        let v110 = ProtocolVersion::new(1, 1, 0);
        let v111 = ProtocolVersion::new(1, 1, 1);
        assert!(v100 < v110);
        assert!(v110 < v111);
    }

    #[test]
    fn test_protocol_version_display() {
        let v = ProtocolVersion::new(0, 1, 0);
        assert_eq!(v.to_string(), "0.1.0");
    }

    #[test]
    fn test_protocol_version_current_major_zero() {
        assert_eq!(ProtocolVersion::CURRENT.major, 0);
    }

    // ── ProtocolHandler creation ────────────────────────────────────────────

    #[test]
    fn test_protocol_handler_new() {
        let h = ProtocolHandler::new();
        assert_eq!(h.version(), ProtocolVersion::CURRENT);
        assert!(h.extension_names().is_empty());
        assert!(!h.is_established());
    }

    #[test]
    fn test_protocol_handler_default() {
        let h = ProtocolHandler::default();
        assert_eq!(h.version(), ProtocolVersion::CURRENT);
    }

    #[test]
    fn test_protocol_stats_default() {
        let h = ProtocolHandler::new();
        let s = h.stats();
        assert_eq!(s.messages_processed, 0);
        assert_eq!(s.errors, 0);
    }

    // ── Extension registration ──────────────────────────────────────────────

    #[test]
    fn test_register_extension_success() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        assert_eq!(h.extension_names().len(), 1);
        assert!(h.extension_names().contains(&"echo"));
    }

    #[test]
    fn test_register_duplicate_extension_fails() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        let err = h.register_extension(Box::new(EchoExtension)).unwrap_err();
        assert!(matches!(
            err,
            ProtocolError::ExtensionAlreadyRegistered { .. }
        ));
    }

    #[test]
    fn test_unregister_extension_success() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        h.unregister_extension("echo").unwrap();
        assert!(h.extension_names().is_empty());
    }

    #[test]
    fn test_unregister_nonexistent_extension_fails() {
        let mut h = ProtocolHandler::new();
        let err = h.unregister_extension("nonexistent").unwrap_err();
        assert!(matches!(err, ProtocolError::ExtensionNotFound { .. }));
    }

    #[test]
    fn test_extension_names_empty() {
        let h = ProtocolHandler::new();
        assert!(h.extension_names().is_empty());
    }

    #[test]
    fn test_extension_names_after_register() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        h.register_extension(Box::new(PingExtension::new()))
            .unwrap();
        let names = h.extension_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"ping"));
    }

    #[test]
    fn test_max_extensions_exceeded() {
        let mut h = ProtocolHandler::new();
        h.max_extensions = 1;
        h.register_extension(Box::new(EchoExtension)).unwrap();
        let err = h
            .register_extension(Box::new(PingExtension::new()))
            .unwrap_err();
        assert!(matches!(
            err,
            ProtocolError::MaxExtensionsExceeded { max: 1 }
        ));
    }

    // ── Handshake ───────────────────────────────────────────────────────────

    #[test]
    fn test_build_hello_includes_capabilities() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        let hello = h.build_hello("node-1").unwrap();
        assert_eq!(hello.node_id, "node-1");
        assert_eq!(hello.capabilities.len(), 1);
        assert_eq!(hello.capabilities[0].name, "echo");
        assert_eq!(hello.protocol_version, ProtocolVersion::CURRENT);
    }

    #[test]
    fn test_process_hello_version_match() {
        let mut responder = ProtocolHandler::new();
        responder
            .register_extension(Box::new(EchoExtension))
            .unwrap();
        let mut initiator = ProtocolHandler::new();
        initiator
            .register_extension(Box::new(EchoExtension))
            .unwrap();

        let hello = initiator.build_hello("initiator").unwrap();
        let ack = responder.process_hello(&hello).unwrap();

        assert!(ack.accepted);
        assert!(ack.accepted_capabilities.contains(&"echo".to_string()));
        assert!(responder.is_established());
    }

    #[test]
    fn test_process_hello_version_mismatch_fails() {
        let mut responder = ProtocolHandler::with_version(ProtocolVersion::new(1, 0, 0));
        let hello = HelloMessage {
            protocol_version: ProtocolVersion::new(2, 0, 0),
            node_id: "alien".into(),
            capabilities: vec![],
            nonce: 42,
            timestamp_ms: 0,
        };
        let err = responder.process_hello(&hello).unwrap_err();
        assert!(matches!(
            err,
            ProtocolError::IncompatibleVersion { got_major: 2, .. }
        ));
        assert_eq!(responder.handshake_state(), HandshakeState::Failed);
    }

    #[test]
    fn test_hello_ack_transitions_to_established() {
        let mut initiator = ProtocolHandler::new();
        let mut responder = ProtocolHandler::new();

        let hello = initiator.build_hello("init").unwrap();
        let ack = responder.process_hello(&hello).unwrap();
        initiator.process_hello_ack(&ack).unwrap();

        assert!(initiator.is_established());
        assert!(responder.is_established());
    }

    #[test]
    fn test_hello_ack_rejected_fails() {
        let mut initiator = ProtocolHandler::new();
        initiator.build_hello("init").unwrap(); // set HelloSent state

        let ack = HelloAckMessage {
            accepted: false,
            remote_version: ProtocolVersion::CURRENT,
            accepted_capabilities: vec![],
            reason: Some("auth failed".into()),
            reply_nonce: 1,
        };
        let err = initiator.process_hello_ack(&ack).unwrap_err();
        assert!(matches!(err, ProtocolError::NegotiationFailed { .. }));
        assert_eq!(initiator.handshake_state(), HandshakeState::Failed);
    }

    #[test]
    fn test_hello_nonce_verified() {
        let mut initiator = ProtocolHandler::new();
        initiator.build_hello("init").unwrap(); // consumes a fresh random nonce

        let wrong_ack = HelloAckMessage {
            accepted: true,
            remote_version: ProtocolVersion::CURRENT,
            accepted_capabilities: vec![],
            reason: None,
            reply_nonce: 999, // wrong
        };
        let err = initiator.process_hello_ack(&wrong_ack).unwrap_err();
        assert!(matches!(err, ProtocolError::NonceMismatch { .. }));
    }

    #[test]
    fn test_handshake_state_transitions() {
        let mut h = ProtocolHandler::new();
        assert_eq!(h.handshake_state(), HandshakeState::Uninitiated);
        h.build_hello("n").unwrap();
        assert!(matches!(
            h.handshake_state(),
            HandshakeState::HelloSent { .. }
        ));
    }

    // ── Extension routing ───────────────────────────────────────────────────

    #[test]
    fn test_echo_extension_returns_payload() {
        let ext = EchoExtension;
        let msg = ExtensionMessage::new("echo", 1, b"hello".to_vec());
        let reply = ext.handle(&msg).unwrap().unwrap();
        assert_eq!(reply.payload, b"hello");
    }

    #[test]
    fn test_echo_extension_sets_flag() {
        let ext = EchoExtension;
        let msg = ExtensionMessage::new("echo", 1, vec![0]);
        let reply = ext.handle(&msg).unwrap().unwrap();
        assert!(reply.flags & 0x1 != 0, "echo flag must be set");
    }

    #[test]
    fn test_route_extension_to_echo() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        let msg = ExtensionMessage::new("echo", 1, b"data".to_vec());
        let reply = h.route_extension_message(&msg).unwrap().unwrap();
        assert_eq!(reply.payload, b"data");
        assert_eq!(h.stats().extension_messages_routed, 1);
    }

    #[test]
    fn test_route_unknown_extension_fails() {
        let mut h = ProtocolHandler::new();
        let msg = ExtensionMessage::new("unknown", 1, vec![]);
        let err = h.route_extension_message(&msg).unwrap_err();
        assert!(matches!(err, ProtocolError::ExtensionNotFound { .. }));
    }

    #[test]
    fn test_stats_increment_on_extension_route() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        let msg = ExtensionMessage::new("echo", 1, b"x".to_vec());
        h.route_extension_message(&msg).unwrap();
        assert_eq!(h.stats().extension_messages_routed, 1);
        assert_eq!(h.stats().bytes_processed, 1);
    }

    #[test]
    fn test_hello_stats_tracked() {
        let mut h = ProtocolHandler::new();
        h.build_hello("n").unwrap();
        assert_eq!(h.stats().hello_messages, 1);
    }

    // ── Ping extension ──────────────────────────────────────────────────────

    #[test]
    fn test_ping_extension_returns_pong() {
        let ext = PingExtension::new();
        let payload = ext.build_ping_payload();
        let msg = ExtensionMessage {
            extension_name: "ping".into(),
            sequence: 1,
            payload,
            flags: 0,
        };
        let pong = ext.handle(&msg).unwrap().unwrap();
        assert!(pong.flags & 0x1 != 0, "pong flag must be set");
        assert_eq!(pong.payload.len(), 16);
    }

    #[test]
    fn test_ping_counts_pings_and_pongs() {
        let ext = PingExtension::new();
        let payload = ext.build_ping_payload();

        // Ping
        let ping_msg = ExtensionMessage {
            extension_name: "ping".into(),
            sequence: 1,
            payload: payload.clone(),
            flags: 0,
        };
        ext.handle(&ping_msg).unwrap();
        assert_eq!(ext.ping_count(), 1);

        // Pong reply
        let pong_msg = ExtensionMessage {
            extension_name: "ping".into(),
            sequence: 1,
            payload,
            flags: 0x1,
        };
        ext.handle(&pong_msg).unwrap();
        assert_eq!(ext.pong_count(), 1);
    }

    #[test]
    fn test_ping_payload_too_short_fails() {
        let ext = PingExtension::new();
        let msg = ExtensionMessage::new("ping", 1, vec![0u8; 3]); // <8 bytes
        let err = ext.handle(&msg).unwrap_err();
        assert!(matches!(err, ProtocolError::DecodeError { .. }));
    }

    // ── Metadata extension ──────────────────────────────────────────────────

    #[test]
    fn test_metadata_extension_set_get() {
        let ext = MetadataExtension::new();
        ext.set("key1", "value1");
        assert_eq!(ext.get("key1").as_deref(), Some("value1"));
        assert_eq!(ext.get("missing"), None);
    }

    #[test]
    fn test_metadata_extension_receive_remote() {
        let sender = MetadataExtension::new();
        sender.set("hello", "world");
        let payload = sender.build_payload().unwrap();

        let receiver = MetadataExtension::new();
        let msg = ExtensionMessage::new("metadata", 1, payload);
        receiver.handle(&msg).unwrap();

        assert_eq!(receiver.remote_get("hello").as_deref(), Some("world"));
    }

    #[test]
    fn test_metadata_empty_payload() {
        let ext = MetadataExtension::new();
        let payload = ext.build_payload().unwrap();
        let msg = ExtensionMessage::new("metadata", 1, payload);
        let reply = ext.handle(&msg).unwrap();
        assert!(reply.is_none(), "metadata exchange is fire-and-forget");
    }

    // ── Handle message dispatch ─────────────────────────────────────────────

    #[test]
    fn test_handle_message_ping_returns_pong() {
        let mut h = ProtocolHandler::new();
        let reply = h
            .handle_message(Message::Ping { timestamp: 12345 })
            .unwrap()
            .unwrap();
        assert!(matches!(
            reply,
            Message::Pong {
                timestamp: 12345,
                ..
            }
        ));
    }

    #[test]
    fn test_handle_message_non_ping_returns_none() {
        let mut h = ProtocolHandler::new();
        let msg = Message::LoadInfo {
            cpu_usage: 0.5,
            memory_usage: 0.3,
            active_agents: 2,
        };
        assert!(h.handle_message(msg).unwrap().is_none());
    }

    // ── Capability serialization ────────────────────────────────────────────

    #[test]
    fn test_capability_serialization() {
        let cap = Capability::new("echo", ProtocolVersion::CURRENT).with_flags(7);
        let json = serde_json::to_string(&cap).unwrap();
        let back: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "echo");
        assert_eq!(back.flags, 7);
    }

    #[test]
    fn test_hello_message_serialization() {
        let hello = HelloMessage {
            protocol_version: ProtocolVersion::CURRENT,
            node_id: "n1".into(),
            capabilities: vec![Capability::new("echo", ProtocolVersion::CURRENT)],
            nonce: 42,
            timestamp_ms: 1000,
        };
        let json = serde_json::to_string(&hello).unwrap();
        let back: HelloMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.nonce, 42);
        assert_eq!(back.node_id, "n1");
    }

    #[test]
    fn test_hello_ack_serialization() {
        let ack = HelloAckMessage {
            accepted: true,
            remote_version: ProtocolVersion::CURRENT,
            accepted_capabilities: vec!["echo".into()],
            reason: None,
            reply_nonce: 7,
        };
        let json = serde_json::to_string(&ack).unwrap();
        let back: HelloAckMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.reply_nonce, 7);
    }

    #[test]
    fn test_extension_message_serialization() {
        let msg = ExtensionMessage::new("echo", 5, b"payload".to_vec());
        let json = serde_json::to_string(&msg).unwrap();
        let back: ExtensionMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload, b"payload");
        assert_eq!(back.sequence, 5);
    }

    // ── Protocol error display ──────────────────────────────────────────────

    #[test]
    fn test_protocol_error_display() {
        let e = ProtocolError::ExtensionNotFound { name: "foo".into() };
        assert!(e.to_string().contains("foo"));

        let e = ProtocolError::IncompatibleVersion {
            expected_major: 1,
            got_major: 2,
        };
        assert!(e.to_string().contains('2'));
    }

    // ── Duration / stats ────────────────────────────────────────────────────

    #[test]
    fn test_protocol_stats_bytes_accounted() {
        let mut h = ProtocolHandler::new();
        h.register_extension(Box::new(EchoExtension)).unwrap();
        let payload = b"hello-world".to_vec();
        let msg = ExtensionMessage::new("echo", 1, payload.clone());
        h.route_extension_message(&msg).unwrap();
        assert_eq!(h.stats().bytes_processed, payload.len() as u64);
    }

    #[test]
    fn test_full_three_extension_handshake() {
        let mut initiator = ProtocolHandler::new();
        initiator
            .register_extension(Box::new(EchoExtension))
            .unwrap();
        initiator
            .register_extension(Box::new(PingExtension::new()))
            .unwrap();
        initiator
            .register_extension(Box::new(MetadataExtension::new()))
            .unwrap();

        let mut responder = ProtocolHandler::new();
        responder
            .register_extension(Box::new(EchoExtension))
            .unwrap();
        // Responder only supports echo — metadata/ping not registered

        let hello = initiator.build_hello("init").unwrap();
        let ack = responder.process_hello(&hello).unwrap();
        initiator.process_hello_ack(&ack).unwrap();

        // Only echo should be in the accepted list
        assert!(ack.accepted_capabilities.contains(&"echo".to_owned()));
        assert!(!ack.accepted_capabilities.contains(&"ping".to_owned()));
        assert!(initiator.is_established());
        assert!(responder.is_established());
    }

    #[test]
    fn test_protocol_handler_with_version() {
        let h = ProtocolHandler::with_version(ProtocolVersion::new(2, 3, 4));
        assert_eq!(h.version(), ProtocolVersion::new(2, 3, 4));
    }

    #[test]
    fn test_echo_extension_name() {
        let ext = EchoExtension;
        assert_eq!(ext.name(), "echo");
    }

    #[test]
    fn test_ping_extension_name() {
        let ext = PingExtension::new();
        assert_eq!(ext.name(), "ping");
    }

    #[test]
    fn test_metadata_extension_name() {
        let ext = MetadataExtension::new();
        assert_eq!(ext.name(), "metadata");
    }

    #[test]
    fn test_extension_message_new() {
        let msg = ExtensionMessage::new("foo", 99, vec![1, 2, 3]);
        assert_eq!(msg.extension_name, "foo");
        assert_eq!(msg.sequence, 99);
        assert_eq!(msg.payload, vec![1, 2, 3]);
        assert_eq!(msg.flags, 0);
    }

    #[test]
    fn test_capability_with_flags() {
        let cap = Capability::new("test", ProtocolVersion::CURRENT).with_flags(0xFF);
        assert_eq!(cap.flags, 0xFF);
    }

    #[test]
    fn test_hello_ack_process_without_prior_hello_fails() {
        let mut h = ProtocolHandler::new();
        // handshake_state is Uninitiated, not HelloSent
        let ack = HelloAckMessage {
            accepted: true,
            remote_version: ProtocolVersion::CURRENT,
            accepted_capabilities: vec![],
            reason: None,
            reply_nonce: 0,
        };
        let err = h.process_hello_ack(&ack).unwrap_err();
        assert!(matches!(err, ProtocolError::HandshakeNotEstablished));
    }

    #[test]
    fn test_nonce_is_random_across_hellos() {
        // The nonce is drawn from a CSPRNG (oxicrypto-rand) rather than a
        // predictable counter; successive hellos must not collide (the
        // probability of an accidental u64 collision is negligible) and
        // must not simply increment by one, confirming it is not a counter.
        let mut h = ProtocolHandler::new();
        let hello1 = h.build_hello("n").unwrap();
        // simulate failed handshake to reset state so we can send another hello
        h.handshake_state = HandshakeState::Uninitiated;
        let hello2 = h.build_hello("n").unwrap();
        assert_ne!(
            hello1.nonce, hello2.nonce,
            "nonce must differ across hellos"
        );
        assert_ne!(
            hello2.nonce,
            hello1.nonce.wrapping_add(1),
            "nonce must not be a predictable counter sequence"
        );
    }

    #[test]
    fn test_capability_negotiation_accepted_list() {
        let mut initiator = ProtocolHandler::new();
        initiator
            .register_extension(Box::new(EchoExtension))
            .unwrap();
        initiator
            .register_extension(Box::new(PingExtension::new()))
            .unwrap();

        let mut responder = ProtocolHandler::new();
        responder
            .register_extension(Box::new(EchoExtension))
            .unwrap();
        // responder does NOT have ping

        let hello = initiator.build_hello("i").unwrap();
        let ack = responder.process_hello(&hello).unwrap();

        assert_eq!(ack.accepted_capabilities.len(), 1);
        assert_eq!(ack.accepted_capabilities[0], "echo");
    }

    #[test]
    fn test_metadata_multiple_entries() {
        let ext = MetadataExtension::new();
        ext.set("a", "1");
        ext.set("b", "2");
        ext.set("c", "3");
        assert_eq!(ext.get("a").as_deref(), Some("1"));
        assert_eq!(ext.get("b").as_deref(), Some("2"));
        assert_eq!(ext.get("c").as_deref(), Some("3"));
    }

    #[test]
    fn test_metadata_overwrite() {
        let ext = MetadataExtension::new();
        ext.set("k", "v1");
        ext.set("k", "v2");
        assert_eq!(ext.get("k").as_deref(), Some("v2"));
    }

    #[test]
    fn test_echo_does_not_modify_payload() {
        let ext = EchoExtension;
        let payload: Vec<u8> = (0u8..=127).collect();
        let msg = ExtensionMessage::new("echo", 1, payload.clone());
        let reply = ext.handle(&msg).unwrap().unwrap();
        assert_eq!(reply.payload, payload);
    }

    #[test]
    fn test_from_protocol_error_to_wire_error() {
        let pe = ProtocolError::HandshakeNotEstablished;
        let we: WireError = pe.into();
        assert!(matches!(we, WireError::InvalidInput(_)));
    }

    #[test]
    fn test_duration_of_ping_payload_is_plausible() {
        let ext = PingExtension::new();
        // Sleep briefly so timestamp > 0
        std::thread::sleep(Duration::from_millis(1));
        let payload = ext.build_ping_payload();
        let ts = u64::from_le_bytes(payload.try_into().unwrap());
        assert!(
            ts > 0,
            "elapsed microseconds must be non-zero after 1ms sleep"
        );
    }
}
