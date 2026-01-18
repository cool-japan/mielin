# mielin-mesh-wire

**Wire Protocol for MielinMesh**

QUIC-based protocol for agent migration and mesh communication.

## Features

- **Type-Safe Messages**: Comprehensive message types for all operations
- **Binary Serialization**: Efficient bincode encoding
- **Priority Support**: Critical messages (migrations) get priority
- **ACK Protocol**: Reliable delivery for important messages

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-mesh-wire = { path = "../mielin-mesh/wire" }
```

### Basic Messaging

```rust
use mielin_mesh_wire::Message;

// Create a ping message
let ping = Message::Ping { timestamp: 123456 };

// Serialize for network
let bytes = ping.serialize()?;

// Send over network...

// Deserialize on receiver
let received = Message::deserialize(&bytes)?;
```

### Agent Migration

```rust
use mielin_mesh_wire::Message;

// Create migration message
let migration = Message::AgentMigration {
    agent_id: [0x1, 0x2, /* ... */],
    snapshot: snapshot_bytes,
    priority: 10,
};

// Serialize and send
let bytes = migration.serialize()?;

// Check if critical
assert!(migration.is_critical());
assert!(migration.requires_ack());
```

## Message Types

### Agent Migration

```rust
AgentMigration {
    agent_id: [u8; 16],
    snapshot: Vec<u8>,
    priority: u8,
}

MigrationAck {
    agent_id: [u8; 16],
    success: bool,
    error_msg: Option<String>,
}
```

### Discovery

```rust
Discovery {
    node_id: [u8; 16],
    node_role: NodeRole,
    capabilities: Vec<String>,
}

DiscoveryResponse {
    node_id: [u8; 16],
    peers: Vec<PeerDescriptor>,
}
```

### Monitoring

```rust
Ping {
    timestamp: u64,
}

Pong {
    timestamp: u64,
    latency_ms: u32,
}

LoadInfo {
    cpu_usage: f32,
    memory_usage: f32,
    active_agents: usize,
}
```

### Query

```rust
AgentQuery {
    agent_id: [u8; 16],
}
```

## API Reference

### `Message`

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
}

impl Message {
    pub fn serialize(&self) -> Result<Vec<u8>, WireError>;
    pub fn deserialize(data: &[u8]) -> Result<Self, WireError>;
    pub fn is_critical(&self) -> bool;
    pub fn requires_ack(&self) -> bool;
}
```

### `NodeRole`

```rust
pub enum NodeRole {
    Edge,
    Relay,
    Core,
}
```

### `PeerDescriptor`

```rust
pub struct PeerDescriptor {
    pub node_id: [u8; 16],
    pub address: String,
    pub latency_ms: Option<u32>,
}
```

## Examples

### Ping/Pong

```rust
use mielin_mesh_wire::Message;
use std::time::{SystemTime, UNIX_EPOCH};

// Send ping
let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as u64;

let ping = Message::Ping { timestamp: now };
send_message(ping.serialize()?);

// Receive and respond with pong
let received = Message::deserialize(&bytes)?;
if let Message::Ping { timestamp } = received {
    let latency = calculate_latency(timestamp);
    let pong = Message::Pong {
        timestamp,
        latency_ms: latency,
    };
    send_message(pong.serialize()?);
}
```

### Agent Migration Protocol

```rust
// Source node
let migration = Message::AgentMigration {
    agent_id: agent.id().as_bytes().clone(),
    snapshot: snapshot.serialize()?,
    priority: 10,
};

send_message(migration.serialize()?)?;

// Wait for ACK
let response = receive_message()?;
if let Message::MigrationAck { success, error_msg, .. } = response {
    if success {
        println!("Migration successful");
    } else {
        eprintln!("Migration failed: {:?}", error_msg);
    }
}
```

### Node Discovery

```rust
// Send discovery request
let discovery = Message::Discovery {
    node_id: local_id.as_bytes().clone(),
    node_role: NodeRole::Relay,
    capabilities: vec!["wasm".to_string(), "sve2".to_string()],
};

send_broadcast(discovery.serialize()?)?;

// Receive discovery responses
loop {
    let msg = receive_message()?;
    if let Message::DiscoveryResponse { node_id, peers } = msg {
        for peer in peers {
            add_peer(peer);
        }
    }
}
```

### Load Monitoring

```rust
// Periodically broadcast load info
let load = Message::LoadInfo {
    cpu_usage: get_cpu_usage(),
    memory_usage: get_memory_usage(),
    active_agents: count_active_agents(),
};

broadcast(load.serialize()?)?;
```

## Message Priorities

Messages are categorized by importance:

| Message | Critical | Requires ACK | Notes |
|---------|----------|--------------|-------|
| Ping/Pong | No | No | Best-effort delivery |
| Discovery | No | Yes | Important for topology |
| AgentMigration | Yes | Yes | Must be reliable |
| MigrationAck | Yes | No | Response only |
| LoadInfo | No | No | Periodic updates |

## Serialization Format

Uses `bincode` for efficient binary serialization:

- Compact representation
- Type-safe encoding/decoding
- Zero-copy where possible

Typical message sizes:
- Ping/Pong: 10-20 bytes
- Discovery: 50-200 bytes
- AgentMigration: 100 bytes - 1 MB (depends on agent)
- LoadInfo: 20-30 bytes

## Error Handling

```rust
pub enum WireError {
    ConnectionFailed(String),
    SerializationError(String),
    TransportError(String),
}
```

## Testing

```bash
cargo test -p mielin-mesh-wire
```

Tests cover:
- Message serialization/deserialization
- Critical message detection
- ACK requirement detection
- Round-trip encoding

## Performance

Serialization benchmarks (x86_64):

| Message Type | Serialize | Deserialize | Size |
|--------------|-----------|-------------|------|
| Ping | ~100 ns | ~100 ns | 12 bytes |
| Discovery | ~500 ns | ~500 ns | ~150 bytes |
| Migration (1KB) | ~2 μs | ~2 μs | ~1.1 KB |
| LoadInfo | ~200 ns | ~200 ns | 24 bytes |

## Future Enhancements

- [ ] Message compression for large payloads
- [ ] Encryption support
- [ ] Message batching
- [ ] Fragmentation for large messages
- [ ] Sequence numbers for ordering
- [ ] Checksums for integrity

## License

MIT OR Apache-2.0
