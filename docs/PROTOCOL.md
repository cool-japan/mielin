# MielinMesh Wire Protocol Specification

**Crate**: `mielin-mesh-wire` (`mielin-mesh/wire`) — Layer 3 ("Synapse") of the MielinOS stack, see [ARCHITECTURE.md](ARCHITECTURE.md).

**Status of this document**: Describes the wire protocol as implemented in the `mielin-mesh-wire` crate at the current workspace revision. This is a living implementation-tracking specification, not a ratified external standard. Where the source code leaves a detail unspecified or simulated, this document says so explicitly rather than inventing a value.

## Table of Contents

1. [Scope and Terminology](#1-scope-and-terminology)
2. [Transport Layer](#2-transport-layer)
3. [Wire Framing and Serialization](#3-wire-framing-and-serialization)
4. [Message Taxonomy](#4-message-taxonomy)
5. [Capability Negotiation and Protocol Versioning](#5-capability-negotiation-and-protocol-versioning)
6. [Extension Mechanism](#6-extension-mechanism)
7. [Reliability, Batching, Priority, and Flow Control](#7-reliability-batching-priority-and-flow-control)
8. [Security Considerations](#8-security-considerations)
9. [Compatibility and Wire-Stability Notes](#9-compatibility-and-wire-stability-notes)
10. [Known Discrepancies Between Code and Prior Documentation](#10-known-discrepancies-between-code-and-prior-documentation)
11. [Further Reading](#11-further-reading)

---

## 1. Scope and Terminology

This document specifies the network-visible behavior of the `mielin-mesh-wire` crate: transport selection, framing, serialization, the `Message` enum, capability/version negotiation, the extension registry, and the reliability/QoS subsystems (acknowledgement, batching, priority, flow control).

It does **not** cover the DHT/routing logic of `mielin-mesh/core`, agent lifecycle semantics of `mielin-cells`, or certificate issuance/rotation mechanics — those are covered by [ARCHITECTURE.md](ARCHITECTURE.md), [MIGRATION.md](MIGRATION.md), and [CERTIFICATES.md](CERTIFICATES.md) respectively.

### 1.1 Requirement Keywords

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** in this document are to be interpreted as in RFC 2119: they describe constraints actually enforced (or clearly intended) by the code, not aspirational design goals. Where the code does not enforce a constraint, this document uses **MAY** or explicitly notes "implementation-defined."

### 1.2 Node Roles

The crate defines three logical node roles via `NodeRole` (`wire/src/lib.rs`):

```rust
pub enum NodeRole {
    Edge,
    Relay,
    Core,
}
```

These are carried in `Message::Discovery` and have no independent wire encoding of their own; they are ordinary enum variants serialized as part of the enclosing message.

---

## 2. Transport Layer

MielinMesh nodes communicate over one of three interchangeable transports, all of which move an opaque, already-serialized `Message` byte buffer:

| Transport | Module | Preference order | Underlying I/O |
|---|---|---|---|
| QUIC | `wire/src/transport.rs` | 1st (preferred) | `oxiquic-transport` + `oxiquic-crypto` over UDP |
| TCP | `wire/src/tcp_transport.rs` | 2nd (fallback) | `std::net::TcpStream`, `TCP_NODELAY` enabled |
| WebSocket | `wire/src/websocket.rs` | 3rd (browser/proxy path) | `tokio-tungstenite` over TCP, optional TLS |

All three transports share the same **maximum message size**: `16 * 1024 * 1024` bytes (16 MiB), defined independently as `MAX_MESSAGE_SIZE` in `transport.rs`, `MAX_TCP_MESSAGE_SIZE` in `tcp_transport.rs`, and `MAX_MESSAGE_SIZE` in `websocket.rs`. A sender **MUST** reject (return `WireError::TransportError` / `WireError::SerializationError`) any `Message` whose serialized form exceeds this bound before attempting to write it to the wire.

### 2.1 Transport Selection and Fallback

`FallbackTransport` (`wire/src/transport_fallback.rs`) implements automatic QUIC→TCP fallback:

- On construction (`FallbackTransport::new`), it attempts to bind a `QuicTransport`; if that fails, `quic` is `None` and the initial `preferred_mode` is `TransportMode::Tcp`. A `TcpTransport` is always constructed as the guaranteed fallback.
- `connect()` first tries QUIC with a `FALLBACK_TIMEOUT` of `Duration::from_secs(5)`. On timeout or connection error it logs (`tracing::info!("QUIC connection failed, falling back to TCP")`), flips `preferred_mode` to `TransportMode::Tcp` for **all subsequent connections from this transport instance**, and retries over TCP.
- `TransportNegotiator` (also in `transport_fallback.rs`) additionally remembers, **per peer address**, which transport mode last worked (`peer_transports: HashMap<SocketAddr, TransportMode>`), and reuses that choice on the next `negotiate()` call, skipping the QUIC attempt entirely for peers already known to require TCP. `reset_peer()` clears this memory (e.g., after a connection failure) so QUIC is retried.

WebSocket is not wired into `FallbackTransport`'s automatic selection; it is a distinct, explicitly-invoked transport intended for browser clients or proxy traversal. The `Message::WebSocketUpgrade` / `Message::WebSocketUpgradeResponse` variants (§4) exist to negotiate an in-band upgrade from an established QUIC/TCP connection to WebSocket, driven by `UpgradeReason`:

```rust
pub enum UpgradeReason {
    QuicNotSupported,
    ProxyTraversal,
    BrowserClient,
    ManualFallback,
}
```

### 2.2 QUIC Transport

`QuicTransport` (`wire/src/transport.rs`) wraps `oxiquic_transport::{ClientEndpoint, ServerEndpoint, DrivenConnection}` with a pure-Rust TLS crypto provider from `oxiquic_crypto::quic_crypto_provider()` (**no `ring`, no `aws-lc`**).

Construction modes:

| Constructor | Server endpoint | Client endpoint | Certificate source |
|---|---|---|---|
| `QuicTransport::new(bind_addr)` | yes | no | fresh self-signed P-256 cert (`oxitls_rcgen::generate_self_signed_p256(&["localhost"])`) |
| `QuicTransport::new_with_certs(bind_addr, node_id, cert_manager)` | yes | no | `CertManager::get_or_generate_cert(node_id)` — see [CERTIFICATES.md](CERTIFICATES.md) |
| `QuicTransport::new_client()` | no | yes, bound to IPv4 wildcard `0.0.0.0:0` | n/a (client-only) |
| `QuicTransport::new_client_for(server_addr)` | no | yes, bound to `[::]:0` or `0.0.0.0:0` depending on `server_addr`'s family | n/a |

Fixed transport parameters (`mesh_transport_config()`):

- **Keep-alive interval**: 15 seconds (`TransportConfig::default().keep_alive_interval(Some(Duration::from_secs(15)))`), deliberately enabled so NAT'd mesh links are not silently dropped by intermediate NAT boxes during long idle periods.
- **Connection timeout**: `CONNECTION_TIMEOUT = Duration::from_secs(10)`, enforced client-side via `tokio::time::timeout` around `ClientEndpoint::connect`.
- **ALPN protocol**: both server and client rustls configs set `alpn_protocols = vec![b"h3".to_vec()]`. This reuses the HTTP/3 ALPN identifier even though the application protocol carried is MielinMesh's own `Message` framing, not HTTP/3 — this is an implementation choice in the current code, not a spec requirement.

**Message-per-stream model**: each `Message` is sent on its own unidirectional QUIC stream (`QuicConnection::send` calls `open_uni_stream()`, writes the serialized bytes, then `shutdown()` to send the stream FIN). The receiver calls `accept_uni_stream()` and reads to EOF with a `.take(MAX_MESSAGE_SIZE as u64)` cap (necessary because `AsyncReadExt::read_to_end` is uncapped, unlike quinn's `read_to_end(max)`). There is **no explicit length prefix on the QUIC path** — the stream FIN delimits the message boundary.

**Connection pooling**: `ConnectionPool` (private to `transport.rs`) caches live connections keyed by `SocketAddr`, with `DEFAULT_MAX_CONNECTIONS = 100` and `MAX_IDLE_TIME = Duration::from_secs(300)`. `evict_idle()` removes closed or stale entries; `ConnectionPoolStats` tracks created/reused/closed counts and byte totals (the byte counters exist in the struct but are not currently incremented by `QuicConnection::send`/`receive` — implementation-defined for future instrumentation).

`connect_with_retry(addr, max_retries)` layers exponential backoff (start 100ms, doubling, capped at 10s) over `connect()`.

### 2.3 TCP Fallback Transport

`TcpTransport` (`wire/src/tcp_transport.rs`) is a synchronous-`std::net`-backed fallback used when QUIC is blocked (e.g., UDP-filtering firewalls). Key parameters:

- `TCP_CONNECT_TIMEOUT = Duration::from_secs(10)`.
- `TCP_NODELAY` is set on every outbound connection.
- Framing is **length-prefixed**, independent of the QUIC and WireSerializer framings described in §3:

```text
+----------------------------+--------------------------------+
| length (u32, big-endian)   | Message::serialize() payload   |
|            4 bytes         |   `length` bytes (oxicode)     |
+----------------------------+--------------------------------+
```

`TcpConnection::send` writes the 4-byte big-endian length via `to_be_bytes()`, then the oxicode-encoded `Message`, then flushes. `TcpConnection::receive` reads exactly 4 bytes for the length, validates `len <= MAX_TCP_MESSAGE_SIZE` (returning `WireError::SerializationError` otherwise), then reads exactly `len` bytes and calls `Message::deserialize`.

`TcpTransport::start_listening()` binds and sets the listener non-blocking, but its accept loop is not implemented in this module (the doc comment notes "In a real implementation, this would spawn a tokio task" — implementation-defined / left to the integrating application).

### 2.4 WebSocket Transport

`WebSocketTransport` (`wire/src/websocket.rs`) wraps `tokio_tungstenite` for browser-compatible / proxy-traversing connectivity, sharing the same 16 MiB `MAX_MESSAGE_SIZE` bound. `WebSocketConfig::production()` sets `use_tls = true` with a 30s timeout; `WebSocketConfig::development()` (the `Default`) runs without TLS on a 10s timeout. `WebSocketUrlBuilder` constructs `ws://`/`wss://` URLs from host/port/path. As currently implemented, `WebSocketTransport::connect(url)` parses only the `url` argument for pooling purposes but establishes the underlying TCP socket against a hardcoded `"localhost:8080"` (`TcpStream::connect("localhost:8080")`) rather than the host/port encoded in `url` — implementation-defined / likely incomplete in the current revision; do not rely on `connect()` reaching arbitrary hosts today.

### 2.5 Connection Lifecycle Summary

```
             +-----------+                       +-----------+
             |  Node A   |                       |  Node B   |
             +-----------+                       +-----------+
                   |                                    |
   1. FallbackTransport::connect(addr)                  |
      (try QUIC, else TCP; remember choice per-peer)    |
                   |---------------------------------->  |
   2. QUIC/TLS 1.3 handshake or TCP 3-way handshake      |
                   |<----------------------------------  |
   3. (optional) ProtocolHandler capability handshake    |
      Hello -> HelloAck  (see §5.1)                      |
                   |---------------------------------->  |
   4. Message exchange (Ping/Pong, Discovery,            |
      AgentMigration, ...) per §4                        |
                   |<--------------------------------->  |
   5. Idle keep-alive (QUIC: 15s interval) or             |
      application-driven close                           |
```

The capability handshake in step 3 is a distinct, optional layer (`protocol::ProtocolHandler`, §5.1/§6) built on top of an already-open QUIC/TCP/WebSocket connection; it is not required before ordinary `Message` traffic can flow, since `Message` itself has no version/capability header.

---

## 3. Wire Framing and Serialization

The crate contains **two independent serialization facilities** that MUST NOT be confused:

1. **`Message`'s own fixed encoding** (`wire/src/lib.rs`), used by every transport in §2.
2. **`WireSerializer` / `WireFormat`** (`wire/src/wire_formats.rs`), a general-purpose, pluggable, self-describing encoder for arbitrary `T: Serialize` payloads (e.g., extension payloads). It is defined and unit-tested but, as of this revision, **no transport module in this crate invokes it to encode a `Message` value** — the transports call `Message::serialize()` directly instead.

### 3.1 `Message` Encoding (used on every transport)

```rust
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
}
```

`oxicode` is the workspace's bincode-compatible binary codec (dependency `oxicode.workspace = true`); `oxicode::serde::Compat` adapts an ordinary `serde::Serialize`/`Deserialize` type into oxicode's native (de)serialization path. **There is no leading format-tag byte and no explicit length field in the bytes `Message::serialize()` produces** — length framing, where needed, is provided externally by the transport (TCP's 4-byte prefix, §2.3) or implicitly by the transport's own message boundary (QUIC stream FIN, §2.2; a WebSocket frame boundary).

The exact byte-level tag oxicode assigns to each `Message` enum variant (i.e., how the discriminant is encoded) is internal to the `oxicode` crate and is **implementation-defined** from this document's perspective — it is not re-derived here to avoid inventing values not verified against the dependency's source.

### 3.2 `WireSerializer` — Pluggable Format with Self-Describing Prefix

`wire/src/wire_formats.rs` defines three interchangeable formats:

```rust
pub enum WireFormat {
    #[default]
    Bincode,   // oxicode — default, most compact for complex/nested types
    Json,      // serde_json — human-readable, cross-language
    Postcard,  // postcard::to_allocvec — embedded-optimized, varint-encoded
}
```

Each format has a fixed 1-byte tag, defined as private constants:

| Format | Tag byte (hex) | Codec |
|---|---|---|
| `WireFormat::Bincode` | `0x01` (`TAG_BINCODE`) | `oxicode::encode_to_vec(&oxicode::serde::Compat(value))` |
| `WireFormat::Json` | `0x02` (`TAG_JSON`) | `serde_json::to_vec(value)` |
| `WireFormat::Postcard` | `0x03` (`TAG_POSTCARD`) | `postcard::to_allocvec(value)` |

`WireSerializer::encode()` prepends the tag byte; `WireSerializer::decode()` is a **static** method that reads the first byte to select the codec, independent of the instance's configured format — so a prefixed payload is self-describing on the wire:

```text
+-----------+------------------------------------------+
| tag (u8)  |  format-specific encoded payload          |
+-----------+------------------------------------------+
```

An unrecognized tag byte yields `FormatError::UnknownFormat(byte)`. `encode_raw()` / `decode_raw()` omit the tag entirely, for cases where both peers already agree on the format out-of-band (e.g., after `FormatNegotiation`, §3.3).

`WireSerializer::benchmark_formats()` encodes a value in all three formats and reports `smallest_format` / `largest_format` (tie-break preference order: Postcard < Bincode < Json for smallest; Json < Bincode < Postcard reversed for largest) — a diagnostic helper, not itself part of the wire protocol.

### 3.3 Format Negotiation (`FormatNegotiation`)

```rust
pub struct FormatNegotiation {
    pub supported_formats: Vec<WireFormat>,   // in preference order
    pub preferred_format: WireFormat,
}
```

`FormatNegotiation::default_client()` advertises `[Postcard, Bincode, Json]` preferring `Postcard` (smallest wire size, favoring bandwidth-constrained/embedded clients); `FormatNegotiation::default_server()` advertises `[Bincode, Postcard, Json]` preferring `Bincode` (most compact for complex nested server-side messages). `negotiate(&self, remote)`:

1. Fast path: if both sides' `preferred_format` match **and** each side's `supported_formats` contains the other's preference, use it directly.
2. Otherwise, walk `self.supported_formats` in order and return the first format also present in `remote.supported_formats`.
3. `None` if the intersection is empty.

`FormatNegotiation` itself has no dedicated `Message` variant or transport-level handshake step in this crate — it is a standalone, serializable type (`Serialize`/`Deserialize`) that an integrating application or a `ProtocolExtension` (§6) can exchange to agree on a `WireFormat` before using `WireSerializer::encode_raw`/`decode_raw` for its own payloads.

### 3.4 Summary of Framing Layers by Path

| Path | Uses `WireSerializer` prefix? | Length framing | Codec |
|---|---|---|---|
| `Message` over QUIC | No | Implicit (QUIC stream FIN) | oxicode via `Compat` |
| `Message` over TCP | No | Explicit 4-byte BE `u32` prefix | oxicode via `Compat` |
| `Message` over WebSocket | No | Implicit (WS frame boundary) | oxicode via `Compat` |
| `ExtensionMessage.payload` (§6) | Extension-defined (e.g. `MetadataExtension` uses raw `serde_json::to_vec`/`from_slice`, not `WireSerializer`) | n/a — carried inside an already-framed `ExtensionMessage` | Extension-defined |
| Generic `T: Serialize` via `WireSerializer::encode`/`decode` | Yes, 1-byte tag | Caller-defined (not used by any transport module today) | Bincode/JSON/Postcard |

---

## 4. Message Taxonomy

The top-level wire message is the `Message` enum (`wire/src/lib.rs`), transmitted as described in §3.1. All fifteen variants share one enum, one serializer, and one size ceiling (16 MiB, §2).

```rust
pub enum Message {
    Ping { timestamp: u64 },
    Pong { timestamp: u64, latency_ms: u32 },
    AgentMigration { agent_id: [u8; 16], snapshot: Vec<u8>, priority: u8 },
    MigrationAck { agent_id: [u8; 16], success: bool, error_msg: Option<String> },
    Discovery { node_id: [u8; 16], node_role: NodeRole, capabilities: Vec<String> },
    DiscoveryResponse { node_id: [u8; 16], peers: Vec<PeerDescriptor> },
    LoadInfo { cpu_usage: f32, memory_usage: f32, active_agents: usize },
    AgentQuery { agent_id: [u8; 16] },
    RoutedMessage { source: [u8; 16], destination: [u8; 16], ttl: u8, hop_count: u8, payload: Box<Message> },
    VersionNegotiationRequest { negotiation: version::VersionNegotiation },
    VersionNegotiationResponse { result: Result<version::NegotiationResult, String> },
    ProtocolUpgrade { request: version::UpgradeRequest },
    ProtocolUpgradeResponse { response: version::UpgradeResponse },
    WebSocketUpgrade { upgrade: websocket::UpgradeToWebSocket },
    WebSocketUpgradeResponse { response: websocket::UpgradeResponse },
}
```

`PeerDescriptor`:

```rust
pub struct PeerDescriptor {
    pub node_id: [u8; 16],
    pub address: String,
    pub latency_ms: Option<u32>,
}
```

### 4.1 Per-Variant Semantics

Classification columns come from three independent methods, each of which reads the message differently — this is intentional but worth stating precisely: `Message::is_critical()` and `Message::requires_ack()` (`lib.rs`) are coarse, batching/ack-relevant booleans; `PriorityQueue::detect_priority()` (`priority.rs`) is a finer 4-tier classification used only for queue ordering.

| Variant | Direction | Queue priority (`detect_priority`) | `is_critical()` | `requires_ack()` |
|---|---|---|---|---|
| `Ping` | either peer, unsolicited | Low | false | false |
| `Pong` | reply to `Ping` | Low | false | false |
| `AgentMigration` | source → destination | Critical if `priority >= 8`, High if `>= 5`, else Normal | **true** (always, regardless of the `priority` field) | true |
| `MigrationAck` | destination → source | Normal if `success`, **Critical if failed** | false | false |
| `Discovery` | broadcaster → bootstrap/peers | High | false | true |
| `DiscoveryResponse` | reply to `Discovery` | High | false | false |
| `LoadInfo` | periodic broadcast | Low | false | false |
| `AgentQuery` | either peer | Normal | false | false |
| `RoutedMessage` | multi-hop envelope | recurses into `payload` | recurses into `payload` | recurses into `payload` |
| `VersionNegotiationRequest` | initiator → peer | High | false | false |
| `VersionNegotiationResponse` | reply | High | false | false |
| `ProtocolUpgrade` | either peer | High | false | false |
| `ProtocolUpgradeResponse` | reply | High | false | false |
| `WebSocketUpgrade` | either peer | Normal | false | false |
| `WebSocketUpgradeResponse` | reply | Normal | false | false |

Note the asymmetry on `MigrationAck`: a failed acknowledgment is queued at `Critical` priority (so failure notifications are delivered promptly) but is **not** `is_critical()` for batching purposes and does not itself `requires_ack()` — acknowledgments are not themselves acknowledged, avoiding infinite regress.

### 4.2 Routing Envelope (`RoutedMessage`)

`RoutedMessage` implements multi-hop delivery over an otherwise flat mesh connection graph:

- `Message::route(self, source, destination)` wraps any message with `ttl: 16` (the hard-coded maximum hop count) and `hop_count: 0`.
- `Message::forward(&mut self)` **MUST** be called by each relaying node before re-transmitting: it decrements `ttl` and increments `hop_count`, returning `Err(WireError::TransportError("TTL expired"))` if `ttl` is already `0`. A relay **MUST NOT** forward a message whose TTL has expired.
- `Message::is_for(&self, node_id)` returns `true` for non-routed messages unconditionally (they have no separate destination concept) and compares `destination` for `RoutedMessage`.
- `Message::unwrap_payload(self)` strips the envelope, returning the inner `Message` (a no-op for non-routed messages).

`ttl + hop_count` is invariant at `16` across the lifetime of a routed message (verified by the crate's own property tests, `wire/src/lib.rs::tests::prop_routed_message_forward_consistency`).

---

## 5. Capability Negotiation and Protocol Versioning

> **This section documents two distinct, independently-defined types both named `ProtocolVersion`.** They live in different modules, have different field widths, different `CURRENT` constants, and different compatibility algorithms. This is a real naming collision in the current codebase, not a simplification introduced by this document — see §10 for the full discrepancy note.

### 5.1 Handshake-Level Versioning: `protocol::ProtocolVersion` and `ProtocolHandler`

`wire/src/protocol.rs` defines a lightweight version type used exclusively by `ProtocolHandler`'s Hello/HelloAck handshake:

```rust
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}
impl ProtocolVersion {
    pub const CURRENT: Self = Self { major: 0, minor: 1, patch: 0 };
    pub fn is_compatible_with(self, other: Self) -> bool {
        self.major == other.major   // ONLY major is checked, at any major value
    }
}
```

**Handshake messages:**

```rust
pub struct HelloMessage {
    pub protocol_version: ProtocolVersion,
    pub node_id: String,
    pub capabilities: Vec<Capability>,   // protocol::Capability — see §6.3 for naming caveat
    pub nonce: u64,
    pub timestamp_ms: u64,
}

pub struct HelloAckMessage {
    pub accepted: bool,
    pub remote_version: ProtocolVersion,
    pub accepted_capabilities: Vec<String>,
    pub reason: Option<String>,
    pub reply_nonce: u64,
}
```

**Handshake sequence** (`ProtocolHandler::build_hello` / `process_hello` / `process_hello_ack`):

1. Initiator calls `build_hello(node_id)`. This assigns a `nonce` from an internal counter (`nonce_counter`, starting at `1`, incremented by `wrapping_add(1)` on every call — **the field doc comment on `HelloMessage::nonce` says "Random value to prevent replay," but the actual generator is a predictable monotonic counter, not a cryptographically random value; see §10**), sets `handshake_state = HandshakeState::HelloSent { nonce }`, and populates `capabilities` from every locally-registered `ProtocolExtension` (§6) as `Capability { name, version: ext.required_version(), flags: 0 }`.
2. Responder calls `process_hello(&hello)`. It rejects with `ProtocolError::IncompatibleVersion { expected_major, got_major }` if `self.version.is_compatible_with(hello.protocol_version)` is false (major mismatch only — the responder's own major, not the sender's, is used as `expected_major`). On success it computes `accepted_capabilities` as the intersection of the hello's advertised capability names with its own registered extension names, calls `on_capability_negotiated()` on each matched extension, transitions to `HandshakeState::Established { remote_version }`, and returns a `HelloAckMessage` with `accepted: true` and `reply_nonce` echoing the hello's nonce.
3. Initiator calls `process_hello_ack(&ack)`. It **MUST** currently be in `HandshakeState::HelloSent { nonce }` (else `ProtocolError::HandshakeNotEstablished`); it rejects with `ProtocolError::NegotiationFailed` if `ack.accepted` is false, and with `ProtocolError::NonceMismatch { expected, got }` if `ack.reply_nonce != nonce`. On success, it notifies its own matching extensions via `on_capability_negotiated()` for each name in `ack.accepted_capabilities` and transitions to `HandshakeState::Established { remote_version: ack.remote_version }`.

```rust
pub enum HandshakeState {
    Uninitiated,
    HelloSent { nonce: u64 },
    Established { remote_version: ProtocolVersion },
    Failed,
}
```

`ProtocolHandler` also tracks `ProtocolStats { messages_processed, extension_messages_routed, hello_messages, hello_acks, errors, bytes_processed }`, and enforces `DEFAULT_MAX_EXTENSIONS = 64` registered extensions.

`ProtocolHandler::handle_message()` is explicitly a convenience/placeholder, per its own doc comment: it only special-cases `Message::Ping` (replying with `Message::Pong { timestamp, latency_ms: 0 }`); every other `Message` variant, including extension traffic, passes through as `Ok(None)`. Callers that need real extension routing **MUST** call `route_extension_message()` directly (§6) rather than relying on `handle_message()`.

### 5.2 Feature-Level Versioning: `version::ProtocolVersion` and `VersionNegotiation`

`wire/src/version.rs` defines a **separate, wider** version type, re-exported at the crate root (`pub use version::{ProtocolVersion, ...}` in `lib.rs` — this is the `ProtocolVersion` visible as `mielin_mesh_wire::ProtocolVersion`):

```rust
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}
impl ProtocolVersion {
    pub const CURRENT: Self = Self::new(0, 2, 0);
    pub const MIN_SUPPORTED: Self = Self::new(0, 1, 0);

    pub fn is_compatible_with(&self, other: &Self) -> bool {
        if self.major != other.major { return false; }
        if self.major == 0 { return self.minor == other.minor; }  // pre-1.0: exact minor match required
        true                                                       // major >= 1: any minor in same major
    }
}
```

This is a stricter algorithm than `protocol::ProtocolVersion::is_compatible_with` (§5.1), which checks **only** the major component regardless of its value. See §10 for why this matters.

`ProtocolFeature` gates ten named capabilities by minimum version:

```rust
pub enum ProtocolFeature {
    BasicMessaging, Compression, PriorityQueues, FlowControl, Acknowledgments,
    HealthMonitoring,          // all >= v0.1.0
    ProtocolVersioning, WebSocketSupport, SecurityHardening, CustomExtensions,  // all >= v0.2.0
}
```

`VersionNegotiation` carries a peer's supported versions (in preference order), supported features, and free-form `capabilities: HashMap<String, String>`:

```rust
pub struct VersionNegotiation {
    pub supported_versions: Vec<ProtocolVersion>,
    pub supported_features: Vec<ProtocolFeature>,
    pub capabilities: HashMap<String, String>,
}
```

`VersionNegotiation::current()` advertises `[ProtocolVersion::CURRENT, ProtocolVersion::new(0, 1, 0)]` and **all ten** `ProtocolFeature` variants regardless of version — the version-gating is applied later, during negotiation, not at advertisement time.

**`negotiate(&self, other)` algorithm:**

1. Iterate `self.supported_versions` (outer) × `other.supported_versions` (inner); for the first pair where `is_compatible_with` holds, set `agreed_version = min(our_version, their_version)`.
2. `enabled_features` = features present in **both** sides' `supported_features` **and** actually supported by `agreed_version` per `ProtocolFeature`'s version gates.
3. `upgrade_available = true` if either side's matched version exceeds `agreed_version`.
4. If no compatible pair is found across the full cross-product, return `VersionError::IncompatibleVersions(ours, theirs)`.

```rust
pub struct NegotiationResult {
    pub agreed_version: ProtocolVersion,
    pub enabled_features: Vec<ProtocolFeature>,
    pub upgrade_available: bool,
}
```

`VersionNegotiator` (async, `tokio::sync::RwLock`-backed) caches one `NegotiationResult` per `peer_id: &str` via `negotiate_with_peer()`, and exposes `is_feature_enabled(peer_id, feature)`, `remove_peer(peer_id)` (on disconnect), and an explicit upgrade flow:

```rust
pub struct UpgradeRequest  { pub current_version: ProtocolVersion, pub target_version: ProtocolVersion, pub reason: String }
pub struct UpgradeResponse { pub accepted: bool, pub new_version: Option<ProtocolVersion>, pub error: Option<String> }
```

`handle_upgrade_request()` accepts a request only if (a) `target_version` is present in the responder's own `supported_versions`, and (b) an active negotiation already exists for `peer_id`; on acceptance it mutates the cached `NegotiationResult.agreed_version` in place and clears `upgrade_available`. Note it does **not** re-validate `target_version` against the peer's originally-advertised `supported_versions` — only against the local list — an implementation detail worth being aware of when reasoning about downgrade/upgrade attacks (see §8).

These four message shapes are carried over the wire as the top-level `Message::VersionNegotiationRequest`, `Message::VersionNegotiationResponse`, `Message::ProtocolUpgrade`, `Message::ProtocolUpgradeResponse` variants (§4).

---

## 6. Extension Mechanism

### 6.1 `ProtocolExtension` Trait

`wire/src/protocol.rs` defines the extensibility point used by `ProtocolHandler`:

```rust
pub trait ProtocolExtension: Send + Sync {
    fn name(&self) -> &str;
    fn required_version(&self) -> ProtocolVersion;   // protocol::ProtocolVersion, §5.1
    fn handle(&self, msg: &ExtensionMessage) -> Result<Option<ExtensionMessage>, ProtocolError>;
    fn on_capability_negotiated(&self, _remote: &Capability) {}   // default no-op
}
```

Extensions are registered by `name()` (a `HashMap<String, Box<dyn ProtocolExtension>>`, capacity-bounded at `DEFAULT_MAX_EXTENSIONS = 64`, §5.1) via `ProtocolHandler::register_extension()` / `unregister_extension()`. Names are advertised in the handshake `Capability` list and intersected during `process_hello`/`process_hello_ack` (§5.1). After the handshake, inbound traffic for a given extension is routed with:

```rust
pub fn route_extension_message(&mut self, msg: &ExtensionMessage)
    -> Result<Option<ExtensionMessage>, ProtocolError>;
```

which looks up `msg.extension_name` in the registry, increments `stats.extension_messages_routed` and `stats.bytes_processed += msg.payload.len()`, and calls the extension's `handle()`, wrapping any error as `ProtocolError::HandlerError`. An unregistered name yields `ProtocolError::ExtensionNotFound`.

**Envelope for all extension traffic:**

```rust
pub struct ExtensionMessage {
    pub extension_name: String,
    pub sequence: u64,
    pub payload: Vec<u8>,   // extension-defined encoding — see §3.4
    pub flags: u32,          // extension-defined bit field
}
```

### 6.2 Built-In Extensions

Three reference extensions ship with the crate:

**`EchoExtension`** (`name() == "echo"`) — returns the payload verbatim, OR-ing bit `0x1` into the reply's `flags` ("echo flag"). Used for round-trip / latency baseline testing.

**`PingExtension`** (`name() == "ping"`) — measures round-trip latency using an internal `Instant` epoch:

```text
Ping payload   (>= 8 bytes required): bytes[0..8]  = send timestamp, u64 LE, microseconds since extension creation
Pong payload   (exactly 16 bytes):    bytes[0..8]   = echoed send timestamp
                                       bytes[8..16]  = arrival timestamp, u64 LE, microseconds
Pong flags: bit 0x1 set (pong indicator)
```

`handle()` treats any message with `flags & 0x1 != 0` as an already-received pong (increments `pong_count`, returns `Ok(None)`); otherwise it treats it as an inbound ping (increments `ping_count`), validates `payload.len() >= 8` (else `ProtocolError::DecodeError`), and constructs the 16-byte pong reply. This is distinct from the top-level `Message::Ping`/`Message::Pong` (§4) — the extension protocol is a separate, application-selectable ping mechanism reachable only via the handshake/extension path, not the always-available `Message::Ping`.

**`MetadataExtension`** (`name() == "metadata"`) — key/value metadata exchange. `payload` is a JSON-encoded `HashMap<String, String>` (`serde_json::to_vec`/`from_slice` directly — **not** routed through `WireSerializer`, §3.4). `handle()` merges the incoming map into `remote_metadata` and always returns `Ok(None)` — metadata exchange is fire-and-forget, with no reply message.

### 6.3 Naming Caveat: Two Types Named `Capability`

`protocol::Capability` (used by the handshake, §5.1) is a **struct**:

```rust
pub struct Capability { pub name: String, pub version: ProtocolVersion, pub flags: u32 }
```

`discovery::Capability` (`wire/src/discovery.rs`, re-exported at the crate root as `mielin_mesh_wire::Capability` via `lib.rs`'s `pub use discovery::{Capability, ...}`) is an unrelated **enum** of peer feature tags (e.g. `WasmRuntime`, `AgentMigration`, `GpuAcceleration`, `Architecture(String)`, ...) used by `DiscoveryService`/`PeerExchange`. The `protocol` module is *not* re-exported at the crate root at all (`lib.rs` has no `pub use protocol::*`), so `protocol::Capability` is only reachable as `mielin_mesh_wire::protocol::Capability`. Code and documentation referring simply to "`Capability`" **MUST** disambiguate which one is meant — see §10.

---

## 7. Reliability, Batching, Priority, and Flow Control

These four subsystems sit above the raw `Message`/transport layer and are independently composable; none of them is mandatory for basic `Message` exchange.

### 7.1 Acknowledgement (`wire/src/ack.rs`)

`MessageId(u64)` is a process-wide monotonic counter (`AtomicU64`, starting at `1`, via `MessageId::generate()`). `AckStatus` is `{ Pending, Acknowledged, Failed, Rejected, Expired }`.

```rust
pub struct Acknowledgment {
    pub message_id: MessageId,
    pub sender: [u8; 16],
    pub success: bool,
    pub error: Option<String>,
    pub timestamp_us: u64,
}
```

**`Acknowledgment` does not derive `Serialize`/`Deserialize` and has no corresponding `Message` enum variant.** The ack subsystem provides the *mechanics* of reliable delivery — timers, retry scheduling, duplicate suppression — as an in-process API; an integrator that wants to actually transmit an acknowledgment between nodes must define their own wire representation for it (for example, via a `ProtocolExtension`, §6, or by extending the `Message` enum). This is a deliberate scope note, not a bug: see §10.

`AckConfig` defaults: `initial_timeout_us = 100_000` (100ms), `max_timeout_us = 30_000_000` (30s), `backoff_multiplier = 2.0`, `max_retries = 5`, `duplicate_window_us = 60_000_000` (60s), `max_pending = 10_000`. Presets `low_latency()`, `high_reliability()`, `embedded()` retune these. `timeout_for_retry(attempt) = min(initial * multiplier^attempt, max)`.

`AckTracker::register(destination, payload) -> MessageId` enqueues a `PendingMessage` (rejecting with `AckError::TooManyPending` past `max_pending`). `process_ack(&Acknowledgment) -> AckResult { Acknowledged { rtt_us } | Rejected { error } | Unknown }` removes the pending entry and updates an exponential-moving-average RTT (`avg_rtt_us = (avg_rtt_us * 7 + rtt) / 8`). `is_duplicate(message_id, sender)` de-duplicates by `(MessageId, sender)` within `duplicate_window_us`. `get_retries()` scans for timed-out pending messages, returning `RetryAction::Retry { .. }` (with the next backoff timeout applied) or `RetryAction::GiveUp { .. }` once `retry_count >= max_retries`. `AckManager` is a callback-driven variant (`DeliveryCallback = Box<dyn FnOnce(AckResult) + Send>`) for push-style delivery notification instead of polling `get_retries()`.

`ReliableMessage<T>` and `ReliableMessage::fire_and_forget(inner, sender)` wrap an arbitrary payload with a generated `MessageId` and a `requires_ack: bool` flag — again, a local scheduling/bookkeeping construct, not itself a `Message` variant.

### 7.2 Batching (`wire/src/batch.rs`)

```rust
pub struct MessageBatch {
    pub messages: Vec<Message>,
    pub created_at_ms: u64,
    pub total_size_bytes: usize,
    pub sequence: u64,
}
```

`MessageBatch` **does** derive `Serialize`/`Deserialize` (unlike `Acknowledgment`), but like `Acknowledgment` it has no `Message` enum variant of its own — it is a standalone unit that `MessageBatcher` hands to the caller (via an `mpsc::UnboundedReceiver<MessageBatch>` returned from `MessageBatcher::new`) to transmit however the integrating application sees fit.

`BatchConfig` defaults: `max_messages = 100`, `max_size_bytes = 1_000_000` (1 MB), `max_wait_time = 10ms`, `batch_critical = false`, `flush_threshold = 0.8`. Presets: `high_throughput()` (500 msgs / 5 MB / 50ms / `batch_critical = true` / 0.9), `low_latency()` (20 / 100 KB / 1ms / 0.5), `embedded()` (10 / 10 KB / 5ms / 0.7).

`MessageBatcher::add_message()`:

1. Estimates size via `estimate_message_size()` — a **heuristic per-variant byte count** (e.g. `Ping` ≈ 16, `Discovery` ≈ 32 + sum of capability string lengths, `AgentMigration` ≈ 32 + `snapshot.len()`), not the true oxicode-serialized size.
2. Rejects with `BatchError::MessageTooLarge` if the estimate exceeds `max_size_bytes`.
3. If `message.is_critical()` (§4) is true and `batch_critical` is false (the default), the message bypasses batching entirely and is flushed as a singleton batch immediately.
4. Otherwise it accumulates into `current_batch`, flushing when message count or byte estimate would exceed the configured maximum, or when the running `fill_ratio >= flush_threshold`.

`start_auto_flush()` additionally spawns a `tokio::time::interval(max_wait_time)` ticker that flushes any non-empty partial batch on a timer, bounding worst-case latency for low-traffic periods.

### 7.3 Priority Queuing (`wire/src/priority.rs`)

```rust
#[repr(u8)]
pub enum Priority { Critical = 0, High = 1, #[default] Normal = 2, Low = 3 }
```

Lower numeric value = higher urgency. `PriorityQueue` supports two scheduling modes selected by `QueueConfig::fair_scheduling`:

- **Strict heap mode** (`fair_scheduling: false`): a single `BinaryHeap<QueuedMessage>`; `QueuedMessage`'s custom `Ord` reverses both priority and sequence comparisons so the heap always pops the lowest-priority-number (most urgent) message, and — within equal priority — the lowest sequence number (earliest-enqueued) first.
- **Fair mode** (`fair_scheduling: true`, the default): four parallel `VecDeque`s, one per priority level, dequeued by scanning `Critical → High → Normal → Low` and popping the front of the first non-empty queue — i.e., strict priority order across levels, FIFO within a level.

`QueueConfig` defaults: `max_size = 10_000`, `max_per_priority = [1000, 2000, 4000, 3000]` (index 0 = Critical .. 3 = Low), `max_age_ms = 30_000`, `fair_scheduling = true`. Presets: `high_throughput()`, `low_latency()`, `embedded()` retune these (see source for exact values).

`enqueue()` (no explicit priority) calls `detect_priority()`, the same auto-classification table given in §4.1. `enqueue_with_priority()` allows an explicit override. Both variants enforce the global `max_size` and the per-priority-level cap independently, incrementing `stats.dropped` and returning `WireError::TransportError` on either limit. `cleanup_expired()` drops messages older than `max_age_ms` from whichever internal structure is active. `SharedPriorityQueue` is an `Arc<Mutex<PriorityQueue>>` convenience wrapper with a matching API surface for multi-task access.

### 7.4 Flow Control (`wire/src/flow.rs`)

Three composable mechanisms, bundled per-peer by `FlowController`:

**Token bucket rate limiting** (`TokenBucket`): `TokenBucketConfig { capacity, refill_rate, initial_tokens }` (all in bytes / bytes-per-second). Default: 1 MB capacity, 10 MB/s refill. Presets: `high_throughput()` (10 MB burst, 100 MB/s), `low_latency()` (100 KB burst, 1 MB/s), `embedded()` (10 KB burst, 100 KB/s). `try_consume(amount)` performs a lazy refill (based on elapsed wall-clock time since `last_refill`) then attempts a lock-free compare-and-swap decrement; `consume(amount)` blocks (via `tokio::time::sleep`) until enough tokens accrue.

**Backpressure signaling** (`BackpressureController`, `BackpressureSignal`): five levels with fixed rate multipliers —

| Level | Rate multiplier | `from_fill_ratio` threshold |
|---|---|---|
| `None` | 1.0 | ratio < 0.5 |
| `Light` | 0.8 | ratio < 0.7 |
| `Moderate` | 0.5 | ratio < 0.85 |
| `Heavy` | 0.2 | ratio < 0.95 |
| `Critical` | 0.0 (pauses sending) | ratio >= 0.95 |

`BackpressureSignal { peer_id, level, suggested_rate, timestamp, reason }` **does** derive `Serialize`/`Deserialize` and is explicitly documented ("for cross-connection coordination") as intended to travel between peers — but, as with `Acknowledgment`/`MessageBatch` (§7.1/§7.2), it has **no corresponding `Message` variant**; an integrator must carry it via an extension or a custom channel. `suggested_rate` per level: `None` → `u64::MAX`, `Light` → 8 MB/s, `Moderate` → 4 MB/s, `Heavy` → 1 MB/s, `Critical` → 0. `BackpressureReason` is one of `{ QueueFull, MemoryPressure, CpuOverload, NetworkCongestion, DownstreamPressure, Unknown }`. `BackpressureController::set_outbound_level(Critical)` also sets an internal `paused` flag that `can_send()`/`wait_to_send()` respect.

**Congestion control** (`CongestionController`):

```rust
pub enum CongestionAlgorithm { None, #[default] Aimd, Cubic, Bbr }
```

Only AIMD (Additive-Increase/Multiplicative-Decrease) has a distinct implementation; per an explicit code comment in `flow.rs`, selecting `Cubic` or `Bbr` currently **falls back to the same AIMD logic** ("Simplified: use AIMD for now") — these two algorithm variants are not yet independently realized. State machine: `SlowStart` (exponential window growth until `cwnd >= ssthresh`) → `CongestionAvoidance` (linear growth, `+aimd_increase * bytes / cwnd` per ACK) → `Recovery` (entered on loss; multiplicative decrease `cwnd *= aimd_decrease`, rate-limited to at most one reduction per 100ms). Defaults (`CongestionConfig`): `initial_window = 64_000`, `min_window = 4_000`, `max_window = 16_000_000` bytes, `aimd_increase = 16_000` bytes/RTT, `aimd_decrease = 0.5`, `rtt_alpha = 0.125` (RTT EMA smoothing factor, RFC 6298-style).

`FlowController::can_send(bytes)` requires all three gates to pass: not backpressure-paused, within the congestion window, and tokens available in the rate limiter. `wait_to_send(bytes)` blocks on each in turn. `effective_rate()` combines the token bucket's configured `refill_rate` with the backpressure multiplier (factoring in the most recent signal from the specific peer if received within 5000ms).

### 7.5 Migration Sub-Protocol Status (`wire/src/migration.rs`)

`migration.rs` defines a fine-grained, pre-copy live-migration message set distinct from the top-level `Message::AgentMigration`/`Message::MigrationAck` pair (§4):

```rust
pub enum MigrationMessage {
    PrepareRequest  { migration_id: [u8; 16], agent_id: [u8; 16], agent_size: usize, memory_size: usize },
    PrepareResponse { migration_id: [u8; 16], accepted: bool, reason: Option<String> },
    PreCopyData     { migration_id: [u8; 16], iteration: u32, pages: Vec<MemoryPage>, dirty_page_count: usize },
    PreCopyAck      { migration_id: [u8; 16], iteration: u32, received_pages: usize },
    StopAndCopy     { migration_id: [u8; 16], final_state: AgentSnapshot, dirty_pages: Vec<MemoryPage> },
    CommitRequest   { migration_id: [u8; 16] },
    CommitAck       { migration_id: [u8; 16], success: bool },
    RollbackRequest { migration_id: [u8; 16], reason: String },
    RollbackAck     { migration_id: [u8; 16] },
}
```

with supporting types `MemoryPage { page_num: u32, data: Vec<u8>, dirty: bool }`, `AgentSnapshot { agent_id, code, state, memory, context: ExecutionContext }`, and a four/five-phase state machine `MigrationPhase { Prepare, PreCopy, StopAndCopy, Commit, Rollback }` / `MigrationState { Idle, Preparing, PreCopying, Finalizing, Committing, Completed, Failed, RollingBack }`.

**Implementation status — read before relying on this as a network protocol**: `MigrationCoordinator::execute_migration()` drives all four phases (`execute_prepare_phase`, `execute_precopy_phase`, `execute_stop_and_copy_phase`, `execute_commit_phase`) internally, but each phase currently **simulates** its network step with `tokio::time::sleep(...)` and locally-synthesized dirty-page counts (`dirty_count = (dirty_count as f64 * 0.7) as usize`) rather than performing real transport I/O — the source contains an explicit stub comment: `// Send prepare request (in real implementation, would use transport)` / `// let response = transport.send_prepare_request(...).await?;` (commented out). The coordinator's internal `mpsc::Sender<MigrationMessage>`/`Receiver` channel is reserved for future transport integration and is not currently connected to `QuicTransport`/`TcpTransport`. The `MigrationMessage` enum is fully `Serialize`/`Deserialize` and ready to be framed exactly as `Message` is (§3.1), but as of this revision, agent migration is actually plumbed end-to-end only through the coarse-grained `Message::AgentMigration` / `Message::MigrationAck` pair described in §4 (consistent with the migration flow documented in [ARCHITECTURE.md](ARCHITECTURE.md)'s "Agent Migration Flow (10 Phases)"). See [MIGRATION.md](MIGRATION.md) for the authoritative migration-specific design.

---

## 8. Security Considerations

Full certificate lifecycle (issuance, ACME, rotation, pinning, mTLS) is covered in the forthcoming [CERTIFICATES.md](CERTIFICATES.md); this section summarizes only what is directly wire-protocol-relevant.

- **Transport encryption**: QUIC connections are secured with TLS 1.3 via `rustls`, using a pure-Rust crypto provider (`oxiquic_crypto::quic_crypto_provider()`) — no `ring`/`aws-lc` dependency. `wire/src/security.rs` documents the supported TLS 1.3 cipher suites (`TlsCipherSuite::{Aes128GcmSha256, Aes256GcmSha384, ChaCha20Poly1305Sha256}`, mapping directly to `rustls::CipherSuite::TLS13_*`), key-exchange groups (`X25519`, `Secp256r1`, `Secp384r1`), and signature algorithms (`Ed25519`, `EcdsaP256Sha256`, `EcdsaP384Sha384`, `RsaPssSha256`, `RsaPssSha384`), with `min_version`/`max_version` both defaulting to `TlsVersion::V1_3`.
- **Default (non-cert-managed) QUIC servers are self-signed**: `QuicTransport::new()` (no `CertManager`) generates a fresh, ephemeral, self-signed P-256 certificate on every startup via `oxitls_rcgen::generate_self_signed_p256(&["localhost"])`. Production deployments **SHOULD** use `QuicTransport::new_with_certs()` with a `CertManager` (see [CERTIFICATES.md](CERTIFICATES.md)) instead.
- **`QuicTransport::new_client()`'s TLS client config disables server certificate verification** (`build_client_config()` installs `SkipServerVerification`, a `rustls::client::danger::ServerCertVerifier` whose `verify_server_cert`/`verify_tls12_signature`/`verify_tls13_signature` all unconditionally return "verified"). This is explicitly marked `for development only` in the source. Any deployment that connects to untrusted networks **MUST NOT** rely on the plain `new_client()` path for certificate validation; it provides transport encryption but no authentication of the remote peer's identity. Certificate pinning (`certs/pinning.rs`, `advanced_tls.rs::CertPin`/`CertPinStore`) and mTLS (`certs/mtls.rs::MtlsConfig`/`MtlsContext`) are available and documented in [CERTIFICATES.md](CERTIFICATES.md).
- **Handshake nonces are not cryptographically random**: as noted in §5.1, `ProtocolHandler`'s `HelloMessage.nonce` is generated by a predictable, monotonically-incrementing counter (`nonce_counter`, starting at 1). The field's own doc comment describes it as a value that should prevent replay, but a sequential counter provides materially weaker replay resistance than a random nonce would. This does not affect QUIC/TLS session security (which has its own, independent nonce/IV handling within the TLS 1.3 record layer) — it is specific to the application-level `ProtocolHandler` handshake.
- **Version/upgrade acceptance is locally-scoped**: `VersionNegotiator::handle_upgrade_request()` (§5.2) validates the requested `target_version` only against the *responder's own* `supported_versions`, not against the peer's originally-negotiated set — implementers extending the upgrade flow should be aware that this alone does not prevent a peer from requesting an upgrade to a version it never actually advertised support for in the initial `VersionNegotiation`.
- **Message-level integrity/authentication**: the `Message` enum itself carries no signature, MAC, or sequence-number field of its own; integrity and authentication of `Message` payloads rely entirely on the enclosing transport's TLS 1.3 session (QUIC) or the operator's chosen WebSocket TLS configuration. Plain TCP fallback (§2.3) provides **no** encryption or authentication at all — it is a raw length-prefixed stream. Deployments that may fall back to TCP across untrusted networks should treat that path as sensitive and constrain it accordingly (e.g., VPN, private network, or an additional application-layer security wrapper), since `TcpTransport` in this crate performs no TLS itself.

---

## 9. Compatibility and Wire-Stability Notes

- **Two live version numbers, three compatibility algorithms**: `protocol::ProtocolVersion::CURRENT = 0.1.0` (major-only compatibility) governs the `ProtocolHandler` handshake; `version::ProtocolVersion::CURRENT = 0.2.0` with `MIN_SUPPORTED = 0.1.0` (major-and-conditionally-minor compatibility) governs `VersionNegotiation`/feature gating. These two numbers are **not** the same protocol version and **MUST NOT** be conflated when reasoning about what a given peer supports.
- **Pre-1.0 minor versions are breaking** under `version::ProtocolVersion::is_compatible_with` (exact minor match required while `major == 0`) — i.e. a `0.1.x` peer and a `0.2.x` peer are, by this crate's own compatibility check, **not** wire-compatible for feature negotiation purposes, even though both are present in `VersionNegotiation::current()`'s advertised `supported_versions` list (which is precisely why that list includes both `0.2.0` and `0.1.0` — to let a `0.1`-only peer still find a compatible entry).
- **`Message` enum changes are not self-describing across versions**: because `Message::serialize()`/`deserialize()` use oxicode's native enum encoding with no accompanying schema or version tag on the wire (§3.1), adding, removing, or reordering variants in the `Message` enum is a breaking wire change for any peer running a different build. New variants **SHOULD** be appended at the end of the enum, following ordinary bincode-style enum evolution discipline, and existing variants' field order **MUST NOT** change without a coordinated rollout.
- **`ProtocolVersion::supports_feature()` gating is advisory, not enforced on the wire**: nothing in `protocol.rs`/`version.rs` prevents a peer from sending a `Message` variant associated with a `ProtocolFeature` that negotiation determined is not `enabled_features` for the connection — the crate provides the negotiation *result*, but applying it (e.g., refusing to send `Message::WebSocketUpgrade` unless `ProtocolFeature::WebSocketSupport` was negotiated) is left to the integrating application.
- **`WireFormat`/`WireSerializer` (§3.2) is additive-safe for its own use cases**: since `WireSerializer::decode()` selects its codec from the leading tag byte, introducing a new `WireFormat` variant with a new tag value is backward-compatible for readers that don't need to understand the new format (they will simply fail to decode payloads tagged with it, via `FormatError::UnknownFormat`) — but this facility is not currently used to frame `Message` itself (§3.4), so it does not change `Message`'s own compatibility story.
- **Transport fallback masks — but does not fix — protocol incompatibility**: `FallbackTransport`/`TransportNegotiator` (§2.1) select *which byte pipe* to use (QUIC vs. TCP), independent of whether the two ends agree on `Message` framing or protocol version. A QUIC-vs-TCP fallback will not resolve a `ProtocolVersion` mismatch.

---

## 10. Known Discrepancies Between Code and Prior Documentation

This section exists to keep future edits honest about where the code and the pre-existing crate `README.md` / `docs/ARCHITECTURE.md` diverge, discovered while grounding this specification in the source:

1. **`mielin-mesh/wire/README.md`'s `Message` enum listing is stale.** It documents only 8 variants (`Ping`, `Pong`, `AgentMigration`, `MigrationAck`, `Discovery`, `DiscoveryResponse`, `LoadInfo`, `AgentQuery`); the current `lib.rs` defines 15, adding `RoutedMessage`, `VersionNegotiationRequest`/`Response`, `ProtocolUpgrade`/`Response`, and `WebSocketUpgrade`/`Response`.
2. **`ARCHITECTURE.md` states the QUIC implementation is "the `quinn` crate."** The actual dependency, per `wire/Cargo.toml` and `transport.rs`'s imports, is `oxiquic-transport` / `oxiquic-crypto` (a workspace-internal, pure-Rust QUIC stack), not `quinn`.
3. **Two distinct `ProtocolVersion` types share a name** (§5): `protocol::ProtocolVersion` (u8 fields, `CURRENT = 0.1.0`, major-only compatibility) and `version::ProtocolVersion` (u16 fields, `CURRENT = 0.2.0`/`MIN_SUPPORTED = 0.1.0`, major-and-conditional-minor compatibility, and the one re-exported as `mielin_mesh_wire::ProtocolVersion`). Code or docs referencing "`ProtocolVersion`" without a module path are ambiguous.
4. **Two distinct `Capability` types share a name** (§6.3): `protocol::Capability` (struct: name/version/flags, used by the handshake) and `discovery::Capability` (enum of feature tags, re-exported at the crate root). Only the latter is reachable as `mielin_mesh_wire::Capability`.
5. **`HelloMessage.nonce`'s doc comment claims randomness it does not have** (§5.1, §8): the field comment says "Random value to prevent replay," but `ProtocolHandler::next_nonce()` is a plain incrementing counter starting at 1.
6. **`ProtocolHandler::handle_message()` is explicitly a placeholder** per its own doc comment, handling only `Message::Ping`; this is stated plainly in-source and preserved here rather than described as a complete dispatch mechanism.
7. **`CongestionAlgorithm::Cubic` and `::Bbr` do not have independent implementations**; both currently execute the same AIMD code path, per an explicit `// Simplified: use AIMD for now` comment in `flow.rs`.
8. **`migration.rs`'s `MigrationCoordinator` phases are simulated**, not wired to real transport I/O, despite `MigrationMessage` being a fully-defined, serializable wire protocol (§7.5). `ARCHITECTURE.md`'s 10-phase migration flow diagram matches the coarse-grained `Message::AgentMigration`/`MigrationAck` path that *is* wired end-to-end, not the fine-grained `MigrationMessage` pre-copy protocol.
9. **`Acknowledgment` (ack.rs), `BackpressureSignal` (flow.rs) — one serializable, one not — and `MessageBatch` (batch.rs) have no corresponding `Message` enum variants** (§7.1, §7.2, §7.4), despite `BackpressureSignal` and `MessageBatch` being fully `Serialize`/`Deserialize` and, per their own doc comments, intended for inter-node exchange. Integrators must supply their own framing (e.g., a `ProtocolExtension`) to actually transmit these types.
10. **`WebSocketTransport::connect(url)` does not dial the host/port encoded in `url`**; it currently opens a TCP connection to a hardcoded `localhost:8080` regardless of the `url` argument's authority component (§2.4).

---

## 11. Further Reading

- [ARCHITECTURE.md](ARCHITECTURE.md) — overall MielinOS system architecture, of which the wire protocol is Layer 3 ("Synapse")
- [NETWORKING.md](NETWORKING.md) — mesh networking, DHT/routing, and discovery layer built atop this wire protocol
- [CERTIFICATES.md](CERTIFICATES.md) — certificate issuance, ACME, rotation, pinning, and mTLS referenced in §8
- [MIGRATION.md](MIGRATION.md) — authoritative live agent-migration design, superseding the status summary in §7.5
- [API.md](API.md) — broader MielinOS API reference
- `mielin-mesh/wire/README.md` — crate-level quick-start (see §10 for known staleness)

---

**MielinMesh Wire Protocol Specification** — tracks `mielin-mesh-wire` source as of this workspace revision.
