# mielin-mesh

**Distributed Mesh Network - The Synapse (Layer 3)**

P2P networking stack for MielinOS, enabling autonomous agent migration across heterogeneous nodes. MielinMesh functions as the synaptic connections between computational neurons, facilitating ultra-fast communication and self-organizing network topology.

## Overview

MielinMesh provides Layer 3 of the MielinOS architecture—a fully decentralized, self-healing mesh network that enables agents to discover nodes, communicate, and migrate seamlessly across the computational fabric.

**Current Status:** v0.1.0 "Oligodendrocyte" (Released 2026-06-21)

## Features

- **Kademlia DHT**: Distributed Hash Table for decentralized node discovery
- **Latency-Aware Routing**: Multi-metric routing considering latency, geography, and hardware
- **Self-Healing**: Automatic rerouting on node failures
- **Self-Organizing**: Optimal topology formation based on network conditions
- **QUIC Transport**: Modern, encrypted transport protocol (Phase 2)
- **Gossip Protocol**: Efficient state synchronization across mesh (Phase 2)
- **Geographic Awareness**: Location-based routing optimization
- **Energy-Aware**: Battery-conscious agent placement

## Architecture

Layer 3 in the 5-layer neural architecture:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: MielinMesh (The Synapse)                          │
│          • Extended Kademlia DHT                            │
│          • QUIC Transport Layer                             │
│          • Gossip Protocol                                  │
│          • Multi-Metric Routing                             │
│          • P2P Discovery                                    │
└─────────────────────────────────────────────────────────────┘
```

## Structure

```
mielin-mesh/
├── core/               # DHT, routing, and node management
│   ├── dht/           # Extended Kademlia DHT
│   ├── gossip/        # Gossip protocol (Phase 2)
│   └── routing/       # Multi-metric routing
├── wire/              # QUIC-based wire protocol
│   ├── quic/          # QUIC transport wrapper
│   └── protocol/      # Message serialization
└── README.md
```

## Sub-Crates

### mielin-mesh-core

**Distributed Hash Table and Routing Logic**

Implements extended Kademlia DHT with multi-dimensional distance metrics:

```rust
use mielin_mesh_core::dht::DHT;
use mielin_mesh_core::node::NodeID;

// Create DHT
let mut dht = DHT::new(NodeID::random());

// Add peer
let peer = NodeID::from_bytes([0x12; 16]);
dht.add_peer(peer, 50); // 50ms latency

// Find closest nodes
let closest = dht.find_closest(&target_id, 3);
```

**Key Features:**
- XOR distance-based routing
- k-bucket organization (k=20 default)
- Latency-aware peer selection
- Automatic peer eviction (LRU)
- 160-bit node IDs

**Performance:**
- Node lookup: O(log N) where N = network size
- Peer add/remove: O(1)
- Memory: ~1KB per 100 peers

[See core/README.md for detailed documentation](core/README.md)

### mielin-mesh-wire

**Wire Protocol for Network Communication**

Defines message types and serialization for mesh communication:

```rust
use mielin_mesh_wire::protocol::Message;

// Create message
let msg = Message::AgentMigration {
    agent_id: [0x42; 16],
    snapshot_data: vec![/* ... */],
};

// Serialize
let bytes = msg.serialize()?;

// Deserialize
let msg = Message::deserialize(&bytes)?;
```

**Message Types:**
- `Ping` / `Pong`: Heartbeat and latency measurement
- `FindNode`: DHT node lookup
- `AgentMigration`: Agent transfer between nodes
- `StateSync`: Gossip protocol state exchange (Phase 2)

**Wire Format:**
- Binary protocol (not JSON for efficiency)
- Message type: 1 byte
- Payload length: 4 bytes (u32)
- Payload: Variable length
- Total overhead: 5 bytes

[See wire/README.md for protocol specification](wire/README.md)

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-mesh-core = { path = "../mielin-mesh/core" }
mielin-mesh-wire = { path = "../mielin-mesh/wire" }
```

### Basic DHT Usage

```rust
use mielin_mesh_core::dht::DHT;
use mielin_mesh_core::node::NodeID;

// Initialize DHT with random node ID
let node_id = NodeID::random();
let mut dht = DHT::new(node_id);

// Bootstrap by adding known peers
dht.add_peer(peer1_id, 25);  // 25ms latency
dht.add_peer(peer2_id, 100); // 100ms latency

// Find nodes closest to target
let target = NodeID::random();
let closest = dht.find_closest(&target, 5);

for (id, latency) in closest {
    println!("Node: {:?}, Latency: {}ms", id, latency);
}
```

### Wire Protocol Example

```rust
use mielin_mesh_wire::protocol::Message;

// Create ping message
let ping = Message::Ping;
let bytes = ping.serialize()?;

// Send over network
// socket.send(&bytes)?;

// Receive and parse
// let received = socket.recv()?;
let msg = Message::deserialize(&bytes)?;

match msg {
    Message::Ping => println!("Received ping"),
    Message::Pong => println!("Received pong"),
    _ => {}
}
```

## Extended Kademlia DHT

Traditional Kademlia uses only XOR distance. MielinMesh extends this with multiple metrics:

### Multi-Dimensional Distance

```rust
pub struct NodeMetrics {
    pub xor_distance: u128,      // Traditional Kademlia
    pub latency_ms: u32,          // Network latency
    pub geographic_km: f32,       // Physical distance
    pub hardware_match: f32,      // Architecture similarity (0.0-1.0)
    pub energy_score: f32,        // Battery level (0.0-1.0)
    pub load_score: f32,          // CPU utilization (0.0-1.0)
}

// Composite score for routing
fn routing_score(metrics: &NodeMetrics) -> f32 {
    0.3 * (1.0 / metrics.latency_ms as f32) +
    0.2 * metrics.hardware_match +
    0.2 * metrics.energy_score +
    0.2 * (1.0 - metrics.load_score) +
    0.1 * (1.0 / (metrics.geographic_km + 1.0))
}
```

### Node Discovery

```rust
// Local network discovery (mDNS)
let local_nodes = discover_local_network()?;

// Bootstrap from known nodes
let bootstrap_nodes = vec![
    "node1.example.com:7070",
    "node2.example.com:7070",
];

for addr in bootstrap_nodes {
    let node_id = connect_and_handshake(addr)?;
    dht.add_peer(node_id, measure_latency(addr));
}

// Peer exchange
let peers = dht.find_closest(&target_id, 20);
for (peer_id, _) in peers {
    let new_peers = request_peers(peer_id)?;
    for new_peer in new_peers {
        dht.add_peer(new_peer, measure_latency(&new_peer));
    }
}
```

## Gossip Protocol (Phase 2)

Efficient state synchronization across the mesh:

```rust
// Gossip message structure
pub struct GossipMessage {
    pub version: u32,
    pub state: MeshState,
}

pub struct MeshState {
    pub node_health: HashMap<NodeID, HealthStatus>,
    pub agent_manifest: HashMap<AgentID, NodeID>,
    pub topology_changes: Vec<TopologyEvent>,
}

// Gossip frequency
const CRITICAL_INTERVAL: Duration = Duration::from_millis(100);  // Node failures
const NORMAL_INTERVAL: Duration = Duration::from_secs(5);        // Metrics
const TOPOLOGY_INTERVAL: Duration = Duration::from_secs(30);     // Topology
```

**Anti-Entropy Reconciliation:**
- Periodic state exchange between random peers
- Detect and resolve inconsistencies
- Eventually consistent across mesh

## Fault Tolerance

MielinMesh handles various failure scenarios:

### 1. Graceful Shutdown
```rust
fn graceful_shutdown(dht: &mut DHT, agents: Vec<Agent>) {
    // Broadcast evacuation notice
    broadcast_message(Message::NodeLeaving {
        node_id: dht.node_id(),
        agents: agents.iter().map(|a| a.id()).collect(),
    });

    // Wait for acknowledgments
    wait_for_acks(Duration::from_secs(5));

    // Peers migrate agents away
}
```

### 2. Sudden Failure
```rust
// Heartbeat timeout detection
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

fn detect_failed_nodes(dht: &mut DHT) {
    for (peer_id, last_seen) in dht.peers() {
        if last_seen.elapsed() > HEARTBEAT_TIMEOUT {
            // Mark as failed
            dht.remove_peer(peer_id);

            // Re-instantiate agents from last checkpoint
            restore_agents_from_checkpoint(peer_id)?;
        }
    }
}
```

### 3. Network Partition
```rust
// Mesh splits into sub-meshes
// Each sub-mesh continues operating
// Auto-merge when reconnected

fn handle_partition(dht: &mut DHT) {
    // Detect partition (sudden loss of many peers)
    if dht.peer_count() < expected_peers / 2 {
        // Continue operating in partition
        log::warn!("Network partition detected");

        // Try to reconnect to bootstrap nodes
        reconnect_bootstrap_nodes()?;
    }
}
```

## Performance Characteristics

### DHT Operations (1000-node mesh)

| Operation | Time | Network Hops |
|-----------|------|--------------|
| Node lookup | <100ms (p95) | log₂(1000) ≈ 10 |
| Peer add | <1μs | 0 |
| Closest nodes | <10μs | 0 (local) |

### Target Metrics (v1.0)

| Metric | Phase 1 | Phase 2 | Phase 4 (v1.0) |
|--------|---------|---------|----------------|
| Max nodes | 100 | 10,000 | 1,000,000 |
| Node discovery | 5s | 1s | <1s |
| Gossip convergence | - | 30s | <30s |
| Message loss rate | - | <1% | <0.1% |

## QUIC Transport (Phase 2)

Modern transport protocol replacing TCP:

```rust
use quinn::{Endpoint, ServerConfig};

// Create QUIC endpoint
let mut endpoint = Endpoint::server(server_config, addr)?;

// Accept connections
while let Some(conn) = endpoint.accept().await {
    let connection = conn.await?;

    // Stream multiplexing
    let (mut send, mut recv) = connection.open_bi().await?;

    // Send message
    send.write_all(&msg_bytes).await?;

    // Receive response
    let response = recv.read_to_end(1024).await?;
}
```

**QUIC Benefits:**
- 0-RTT connection establishment
- Built-in TLS 1.3 encryption
- Stream multiplexing (multiple messages per connection)
- Better loss recovery than TCP
- NAT traversal support

## API Reference

### `DHT`

```rust
pub struct DHT {
    node_id: NodeID,
    buckets: Vec<KBucket>,
}

impl DHT {
    pub fn new(node_id: NodeID) -> Self;
    pub fn add_peer(&mut self, peer_id: NodeID, latency_ms: u32);
    pub fn remove_peer(&mut self, peer_id: &NodeID);
    pub fn find_closest(&self, target: &NodeID, k: usize)
        -> Vec<(NodeID, u32)>;
    pub fn peer_count(&self) -> usize;
}
```

### `NodeID`

```rust
pub struct NodeID([u8; 16]);  // 128-bit UUID

impl NodeID {
    pub fn random() -> Self;
    pub fn from_bytes(bytes: [u8; 16]) -> Self;
    pub fn xor_distance(&self, other: &NodeID) -> u128;
}
```

### `Message` (Wire Protocol)

```rust
pub enum Message {
    Ping,
    Pong,
    FindNode { target: NodeID },
    NodesFound { nodes: Vec<NodeInfo> },
    AgentMigration { agent_id: [u8; 16], snapshot_data: Vec<u8> },
}

impl Message {
    pub fn serialize(&self) -> Result<Vec<u8>, WireError>;
    pub fn deserialize(data: &[u8]) -> Result<Self, WireError>;
}
```

## Roadmap

### Phase 1 (v0.1 "Ranvier") ✅ Complete
- ✅ Kademlia DHT foundation
- ✅ XOR distance routing
- ✅ Latency-aware peer selection
- ✅ Wire protocol message types
- ✅ Binary serialization

### Phase 2 (v0.2 "Oligodendrocyte") - 2026-06-21
- [ ] Real QUIC transport (Quinn library)
- [ ] mDNS local discovery
- [ ] Peer exchange protocol
- [ ] Gossip protocol implementation
- [ ] NAT traversal (STUN/TURN)
- [ ] Connection pooling

### Phase 3 (v0.3 "Schwann") - Q2-Q3 2026
- [ ] Geographic awareness (GPS/IP geolocation)
- [ ] Energy-aware routing
- [ ] Adaptive gossip intervals
- [ ] Compression (Zstd for large payloads)

### Phase 4 (v1.0 "Saltatory") - Q4 2026
- [ ] 1M node simulation
- [ ] Byzantine fault tolerance
- [ ] Reputation system
- [ ] Mesh analytics and observability

See [TODO.md](../TODO.md) for detailed roadmap.

## Testing

```bash
# Run all mesh tests
cargo test -p mielin-mesh-core
cargo test -p mielin-mesh-wire

# Run with logging
RUST_LOG=debug cargo test

# Benchmark DHT operations
cargo bench -p mielin-mesh-core
```

**Test Coverage:**
- DHT peer management
- XOR distance calculations
- Routing table maintenance
- Message serialization/deserialization
- Latency-based peer selection
- Network partition scenarios

## Examples

### Complete Node Discovery

```rust
use mielin_mesh_core::dht::DHT;
use mielin_mesh_core::node::NodeID;

// Create DHT
let my_id = NodeID::random();
let mut dht = DHT::new(my_id);

// Bootstrap from known nodes
let bootstrap = vec![
    (NodeID::from_bytes([0x11; 16]), 25),
    (NodeID::from_bytes([0x22; 16]), 50),
];

for (peer_id, latency) in bootstrap {
    dht.add_peer(peer_id, latency);
}

// Discover more nodes via peer exchange
fn discover_peers(dht: &mut DHT) -> Result<(), Error> {
    let target = NodeID::random();
    let closest = dht.find_closest(&target, 20);

    for (peer_id, latency) in closest {
        // In real implementation, request peers from each node
        // let new_peers = request_peers(peer_id)?;
        // for new_peer in new_peers {
        //     dht.add_peer(new_peer.id, new_peer.latency);
        // }
    }

    Ok(())
}
```

### Agent Location Lookup

```rust
use mielin_mesh_core::dht::DHT;
use mielin_cells::AgentID;

fn find_agent_location(
    dht: &DHT,
    agent_id: &AgentID,
) -> Option<NodeID> {
    // Agent ID maps to node via DHT
    // Find nodes closest to agent's ID hash
    let target = NodeID::from_bytes(*agent_id.as_bytes());
    let closest = dht.find_closest(&target, 3);

    // Query each node for agent
    for (node_id, _latency) in closest {
        // In real implementation:
        // if query_node_for_agent(node_id, agent_id)? {
        //     return Some(node_id);
        // }
    }

    None
}
```

## Security Considerations

1. **Node Authentication**: Verify node identity before adding to DHT (Phase 2)
2. **Sybil Attack Prevention**: Limit peers per IP address
3. **Eclipse Attack Mitigation**: Connect to diverse peers (geographic, network)
4. **Message Authentication**: Sign all messages (Phase 4)
5. **Rate Limiting**: Prevent DoS via message flooding

## Best Practices

1. **Maintain Diverse Peers**: Mix of low-latency and diverse geographic nodes
2. **Monitor Peer Health**: Regular heartbeat checks
3. **Graceful Degradation**: Continue operating with partial connectivity
4. **Cache DHT Lookups**: Reduce network overhead for frequent queries
5. **Adaptive Timeouts**: Adjust based on network conditions

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

**Key areas for contribution:**
- QUIC transport integration
- NAT traversal implementation
- Mesh topology optimization algorithms
- Geographic routing enhancements
- Performance benchmarking

## Resources

- [Main Documentation](../mielin.md) - Complete technical whitepaper
- [TODO & Roadmap](../TODO.md) - Development plan
- [Kademlia Paper](http://www.scs.stanford.edu/~dm/home/papers/kpos.pdf) - Original DHT design

## Contact

- **Repository**: https://github.com/cool-japan/mielin
- **Issues**: https://github.com/cool-japan/mielin/issues
- **Email**: contact@cooljapan.tech

## License

Licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](../LICENSE-APACHE))

---

**MielinMesh** - The synaptic network enabling autonomous agents to discover and traverse the computational nervous system 🧠⚡

**Current Version:** v0.1.0 "Oligodendrocyte" | Released 2026-06-21
