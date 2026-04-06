# mielin-mesh-core

**Distributed Hash Table and Routing**

Core networking primitives for the MielinMesh distributed system.

## Features

- **Kademlia DHT**: XOR distance-based routing
- **Latency-Aware**: Peer selection considers network latency
- **Peer Management**: Automatic timeout and eviction
- **Node Roles**: Edge, Relay, Core node types

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-mesh-core = { path = "../mielin-mesh/core" }
```

### Creating a Node

```rust
use mielin_mesh_core::{Node, NodeRole};

// Create a relay node
let node = Node::new(NodeRole::Relay);
println!("Node ID: {}", node.id());
println!("Role: {:?}", node.role());
```

### Using the DHT

```rust
use mielin_mesh_core::dht::{Dht, PeerInfo};
use uuid::Uuid;

// Create DHT
let local_id = Uuid::new_v4();
let mut dht = Dht::new(local_id);

// Add peers
let peer = PeerInfo::new(Uuid::new_v4());
dht.insert_peer(peer);

// Find closest peers to a target
let target = Uuid::new_v4();
let closest = dht.find_closest(&target, 3);

// Find peers by latency
dht.update_peer_latency(&peer_id, 50); // 50ms
let low_latency_peers = dht.find_closest_by_latency(5);
```

## API Reference

### `Node`

```rust
pub struct Node {
    id: NodeId,
    role: NodeRole,
}

pub enum NodeRole {
    Edge,   // Sensor/actuator nodes
    Relay,  // Intermediate routing nodes
    Core,   // High-capacity compute nodes
}

impl Node {
    pub fn new(role: NodeRole) -> Self;
    pub fn id(&self) -> &NodeId;
    pub fn role(&self) -> NodeRole;
}
```

### `Dht`

```rust
pub struct Dht {
    local_id: NodeId,
    routing_table: HashMap<NodeId, PeerInfo>,
}

impl Dht {
    pub fn new(local_id: NodeId) -> Self;
    pub fn insert_peer(&mut self, peer: PeerInfo);
    pub fn find_closest(&self, target: &NodeId, k: usize) -> Vec<NodeId>;
    pub fn find_closest_by_latency(&self, k: usize) -> Vec<NodeId>;
    pub fn update_peer_latency(&mut self, node_id: &NodeId, latency_ms: u32);
    pub fn peer_count(&self) -> usize;
}
```

### `PeerInfo`

```rust
pub struct PeerInfo {
    pub node_id: NodeId,
    pub last_seen: Instant,
    pub latency_ms: Option<u32>,
    pub reliability_score: u8,
}

impl PeerInfo {
    pub fn new(node_id: NodeId) -> Self;
    pub fn is_alive(&self) -> bool;  // Checks if peer is still responsive
    pub fn update_seen(&mut self);
    pub fn set_latency(&mut self, latency_ms: u32);
}
```

## DHT Algorithm

### XOR Distance

Peers are organized by XOR distance:

```rust
distance(A, B) = A XOR B
```

Closer distance = more similar IDs = preferred routing path.

### K-Buckets

- K = 20 peers per bucket
- Timeout = 5 minutes
- Automatic eviction of stale peers

### Routing

1. Find K closest peers to target
2. Sort by XOR distance
3. Optionally sort by latency
4. Return top K results

## Examples

### Basic DHT Usage

```rust
use mielin_mesh_core::dht::{Dht, PeerInfo};
use uuid::Uuid;

let mut dht = Dht::new(Uuid::new_v4());

// Add 10 peers
for _ in 0..10 {
    dht.insert_peer(PeerInfo::new(Uuid::new_v4()));
}

// Find 3 closest to target
let target = Uuid::new_v4();
let closest = dht.find_closest(&target, 3);

println!("Found {} closest peers", closest.len());
```

### Latency-Based Routing

```rust
let mut dht = Dht::new(local_id);

// Add peers with latency information
let peer1 = Uuid::new_v4();
let peer2 = Uuid::new_v4();

dht.insert_peer(PeerInfo::new(peer1));
dht.insert_peer(PeerInfo::new(peer2));

dht.update_peer_latency(&peer1, 100); // 100ms
dht.update_peer_latency(&peer2, 10);  // 10ms

// Get lowest latency peers
let fast_peers = dht.find_closest_by_latency(1);
assert_eq!(fast_peers[0], peer2); // Lowest latency first
```

### Node Discovery

```rust
use mielin_mesh_core::{Node, NodeRole};

// Create different node types
let edge = Node::new(NodeRole::Edge);
let relay = Node::new(NodeRole::Relay);
let core = Node::new(NodeRole::Core);

// Routing decisions based on role
match node.role() {
    NodeRole::Edge => {
        // Connect to nearby relay
    }
    NodeRole::Relay => {
        // Bridge edge and core
    }
    NodeRole::Core => {
        // Handle heavy computation
    }
}
```

## Performance

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Insert peer | O(1) | HashMap insertion |
| Find closest | O(n log n) | Sorting by distance |
| Find by latency | O(n log n) | Filter + sort |
| Peer eviction | O(n) | Full table scan |

Typical performance (1000 peers):
- Insert: <1 μs
- Find closest (K=20): ~100 μs
- Eviction: ~50 μs

## Configuration

```rust
const K_BUCKET_SIZE: usize = 20;              // Peers per bucket
const PEER_TIMEOUT: Duration = Duration::from_secs(300);  // 5 minutes
```

## Testing

```bash
cargo test -p mielin-mesh-core
```

Tests include:
- DHT creation and peer insertion
- XOR distance calculation
- Closest peer finding
- Latency-based routing
- Peer timeout and eviction

## Future Enhancements

- [ ] Iterative lookups (multi-hop routing)
- [ ] Bucket splitting for large networks
- [ ] Peer reputation system
- [ ] Geographic awareness
- [ ] NAT traversal support

## License

Apache-2.0
