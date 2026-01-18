# MielinOS Mesh Cluster Example

**3-Node QUIC-based Mesh Network Demonstration**

This example demonstrates real QUIC transport, peer discovery, and live agent migration across a cluster of MielinOS nodes.

## Features

- ✅ **QUIC Transport**: Real quinn-based QUIC communication with TLS 1.3
- ✅ **3 Node Types**: Edge, Relay, and Core nodes with different roles
- ✅ **Peer Discovery**: Automatic node discovery and connection
- ✅ **DHT Routing**: Kademlia-based distributed hash table for peer routing
- ✅ **Multi-Hop Routing**: Messages forwarded through intermediate nodes via DHT
- ✅ **Live Migration**: Agent migration over the network with acknowledgment
- ✅ **Connection Pooling**: Efficient connection reuse and management

## Quick Start

### 1. Start a Core Node

```bash
cargo run -p mesh-cluster -- --role core --port 5000 --agent
```

This starts a core node on port 5000 with one agent.

### 2. Start an Edge Node

In a new terminal:

```bash
cargo run -p mesh-cluster -- --role edge --port 5001 --connect 127.0.0.1:5000
```

This starts an edge node that connects to the core node.

### 3. Migrate an Agent

In a third terminal:

```bash
cargo run -p mesh-cluster -- --role edge --port 5002 --connect 127.0.0.1:5000 --agent --migrate-to 127.0.0.1:5000
```

This:
1. Starts an edge node
2. Creates an agent
3. Migrates the agent to the core node
4. Waits for acknowledgment

## Command-Line Options

```bash
mesh-cluster [OPTIONS] --role <ROLE>

Options:
  -r, --role <ROLE>
          Node role: edge, relay, or core

  -p, --port <PORT>
          Port to bind to [default: 0 (random)]

  -c, --connect <ADDR>...
          Peer addresses to connect to (format: ip:port)
          Can be specified multiple times

  -a, --agent
          Create an agent on this node

  -m, --migrate-to <ADDR>
          Migrate agent to this peer (format: ip:port)
```

## Node Roles

### Edge Node
- Runs on IoT devices, sensors, edge servers
- Lightweight, battery-aware
- Can create and host agents
- Migrates agents when resources are low

### Relay Node
- Intermediate routing nodes
- Helps with network connectivity
- Can temporarily host migrating agents
- Optimizes mesh topology

### Core Node
- Runs in cloud or powerful servers
- High resource availability
- Preferred destination for agent migration
- Can run many agents simultaneously

## Example Scenarios

### Scenario 1: Simple 2-Node Migration

Terminal 1 (Core):
```bash
cargo run -p mesh-cluster -- --role core --port 5000
```

Terminal 2 (Edge with migration):
```bash
cargo run -p mesh-cluster -- --role edge --port 5001 \
    --connect 127.0.0.1:5000 --agent --migrate-to 127.0.0.1:5000
```

### Scenario 2: 3-Node Mesh

Terminal 1 (Core):
```bash
cargo run -p mesh-cluster -- --role core --port 5000 --agent
```

Terminal 2 (Relay):
```bash
cargo run -p mesh-cluster -- --role relay --port 5001 \
    --connect 127.0.0.1:5000
```

Terminal 3 (Edge):
```bash
cargo run -p mesh-cluster -- --role edge --port 5002 \
    --connect 127.0.0.1:5000 --connect 127.0.0.1:5001
```

### Scenario 3: Multiple Agents with Migration

Terminal 1 (Core - Agent Receiver):
```bash
cargo run -p mesh-cluster -- --role core --port 5000
```

Terminal 2 (Edge - Agent Source):
```bash
# Create agent and migrate
cargo run -p mesh-cluster -- --role edge --port 5001 \
    --connect 127.0.0.1:5000 --agent --migrate-to 127.0.0.1:5000
```

### Scenario 4: Multi-Hop Routing (4 Nodes)

Test message routing through intermediate nodes:

Terminal 1 (Core Node A):
```bash
cargo run -p mesh-cluster -- --role core --port 5000
```

Terminal 2 (Relay Node B):
```bash
cargo run -p mesh-cluster -- --role relay --port 5001 --connect 127.0.0.1:5000
```

Terminal 3 (Relay Node C):
```bash
cargo run -p mesh-cluster -- --role relay --port 5002 --connect 127.0.0.1:5001
```

Terminal 4 (Edge Node D):
```bash
cargo run -p mesh-cluster -- --role edge --port 5003 \
    --connect 127.0.0.1:5002
```

In this topology: D → C → B → A

Messages from Node D to Node A will automatically route through intermediate nodes B and C using the DHT.

## Output Example

### Core Node Starting

```
═══════════════════════════════════════════════
  MielinOS Mesh Cluster - 3-Node Demo
  Version: v0.1.0-rc.1 (Release Candidate)
═══════════════════════════════════════════════
🚀 Core node started on 127.0.0.1:5000
👂 Listening for incoming connections...
```

### Edge Node with Agent Migration

```
═══════════════════════════════════════════════
  MielinOS Mesh Cluster - 3-Node Demo
  Version: v0.1.0-rc.1 (Release Candidate)
═══════════════════════════════════════════════
🚀 Edge node started on 127.0.0.1:5001
🔗 Connecting to peer at 127.0.0.1:5000
✅ Discovery message sent to 127.0.0.1:5000
🤖 Created agent with ID: a1b2c3d4-...
   Total agents on this node: 1
📦 Capturing migration snapshot for agent a1b2c3d4-...
   Snapshot size: 72 bytes
🚀 Migrating agent to 127.0.0.1:5000
✅ Agent migration message sent
   Waiting for acknowledgment...
🎉 Migration successful!
```

### Core Node Receiving Agent

```
📨 Received connection from 127.0.0.1:5001
🔍 Discovery from peer:
   Node ID: [...]
   Role: Edge
   Capabilities: ["quic", "migration"]
📥 Received agent migration:
   Agent ID: [...]
   Snapshot size: 78 bytes
   Priority: 10
✅ Agent restored successfully
   Total agents on this node: 1
```

## Architecture

The mesh-cluster example demonstrates:

1. **QUIC Transport Layer**
   - TLS 1.3 encryption
   - Stream multiplexing
   - Connection pooling
   - 16MB message size support

2. **DHT-based Routing**
   - Kademlia distributed hash table
   - XOR distance metric for peer selection
   - Automatic peer table maintenance
   - Greedy routing algorithm
   - Latency-aware peer selection

3. **Multi-Hop Routing**
   - Messages automatically forwarded through intermediate nodes
   - RoutedMessage envelope with source, destination, TTL
   - Maximum 16 hops to prevent loops
   - Hop count tracking for debugging
   - Automatic next-hop selection via DHT
   - Transparent routing for all message types

4. **Mesh Networking**
   - Peer discovery protocol
   - Multi-peer connectivity
   - Role-based routing
   - DHT peer address lookup

5. **Agent Migration**
   - Snapshot capture
   - Binary serialization (bincode 2.0.1)
   - Network transfer
   - State restoration
   - Migration acknowledgment

## Network Protocol

### Discovery Message
```rust
Message::Discovery {
    node_id: [u8; 16],
    node_role: NodeRole,
    capabilities: Vec<String>,
}
```

### Migration Message
```rust
Message::AgentMigration {
    agent_id: [u8; 16],
    snapshot: Vec<u8>,    // Serialized agent state
    priority: u8,          // Migration priority (0-255)
}
```

### Acknowledgment
```rust
Message::MigrationAck {
    agent_id: [u8; 16],
    success: bool,
    error_msg: Option<String>,
}
```

## Performance

Current benchmarks (local network):

| Operation | Time | Notes |
|-----------|------|-------|
| Connection setup | ~100ms | QUIC handshake + TLS |
| Discovery message | <1ms | Small message |
| Agent migration (72 bytes) | ~5ms | Including ACK |
| Snapshot serialization | ~190ns | bincode 2.0.1 |
| Agent restoration | ~370ns | State unpacking |

## Limitations (v0.1.0-rc.1)

- ⚠️ **No real DHT**: Peer discovery is manual via `--connect`
- ⚠️ **No automatic routing**: Must specify peers manually
- ⚠️ **Self-signed certificates**: Development-only TLS configuration
- ⚠️ **No persistence**: Agents lost on node shutdown
- ⚠️ **Local network only**: Tested on 127.0.0.1 only

## Next Steps (Phase 2)

- [ ] Automatic peer discovery with mDNS
- [ ] DHT-based routing implementation
- [ ] Certificate management (Let's Encrypt)
- [ ] Agent persistence across restarts
- [ ] RaspberryPi + AWS Graviton testing
- [ ] Performance optimization (<10ms migration)

## Troubleshooting

### Port already in use
```
Error: Address already in use (os error 98)
```
Solution: Use `--port 0` to bind to a random available port.

### Connection refused
```
Error: Connection refused
```
Solution: Ensure the target node is running and the address is correct.

### Migration failed
```
❌ Migration failed: "Deserialization error"
```
Solution: Ensure both nodes are running the same version of the code.

## Related Examples

- [hello-agent](../hello-agent/) - Basic agent creation
- [agent-migration](../agent-migration/) - Detailed migration demo (single-node)
- [counter-agent](../counter-agent/) - Minimal WASM agent

## Documentation

- [MielinMesh Core](../../mielin-mesh/core/README.md) - DHT and routing
- [MielinMesh Wire](../../mielin-mesh/wire/README.md) - QUIC protocol
- [Mielin Cells](../../mielin-cells/README.md) - Agent SDK

---

**MielinOS v0.1.0-rc.1 - Phase 2 "Oligodendrocyte" (Release Candidate)
