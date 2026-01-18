# MielinOS Cluster Testing Guide

This guide explains how to test MielinOS's distributed mesh networking capabilities using a local 3-node Docker cluster.

## Overview

The cluster consists of three nodes simulating a realistic MielinOS deployment:

- **Core Node** (Port 8080): Cloud server acting as bootstrap node
- **Relay Node** (Port 8081): Edge gateway connecting core and edge devices
- **Edge Node** (Port 8082): IoT device with an agent, connects through relay

All nodes run the integrated `MeshService` orchestrator, which provides:
- **Discovery Protocol**: Bootstrap-based peer discovery
- **Gossip Protocol**: SWIM-inspired membership and failure detection
- **Agent Registry**: DHT-based agent location tracking
- **Migration Coordinator**: Live agent migration with telemetry

## Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+
- 2GB RAM minimum
- Linux, macOS, or Windows with WSL2

## Quick Start

### 1. Build and Start the Cluster

```bash
./scripts/cluster.sh build
./scripts/cluster.sh start
```

This will:
1. Build optimized Docker images for each node
2. Start the core node (bootstrap)
3. Start the relay node (connects to core)
4. Start the edge node (connects to relay, creates an agent)

### 2. Verify Cluster Health

```bash
./scripts/cluster.sh status
```

Expected output:
```
MielinOS Cluster Status:
NAME           IMAGE              STATUS         PORTS
mielin-core    mielin-core        Up (healthy)   0.0.0.0:8080->8080/tcp
mielin-relay   mielin-relay       Up (healthy)   0.0.0.0:8081->8080/tcp
mielin-edge    mielin-edge        Up (healthy)   0.0.0.0:8082->8080/tcp
```

### 3. View Node Logs

Watch all nodes:
```bash
./scripts/cluster.sh logs
```

Watch specific node:
```bash
./scripts/cluster.sh logs edge
```

Look for key events:
- `✅ Mesh service started (gossip + registry + migration)`
- `🔗 Connecting to peer at`
- `🤖 Created agent with ID`
- `📋 Registered in mesh registry`

## Testing Features

### Gossip Protocol

The gossip protocol maintains cluster membership. Check gossip stats:

```bash
./scripts/cluster.sh exec core /app/node --help
```

Expected behavior:
- Nodes detect each other within 10 seconds
- Heartbeats sent every 5 seconds
- Failed nodes marked as "suspect" after 15s, "dead" after 30s

### Agent Registry

The edge node creates an agent on startup. Verify registration:

```bash
./scripts/cluster.sh logs edge | grep "Registered in mesh registry"
```

You should see:
```
🤖 Created agent with ID: <uuid>
   Registered in mesh registry
```

### Live Migration

Test agent migration from edge to core:

```bash
./scripts/cluster.sh test-migration
```

This triggers migration and shows the full migration flow:

**On Edge Node:**
```
📦 Initiating migration for agent <id>
✅ Migration initiated via mesh service
   Snapshot size: 78 bytes
🎉 Migration successful!
```

**On Core Node:**
```
📥 Received agent migration:
   Agent ID: <id>
   Snapshot size: 78 bytes
✅ Agent restored successfully
   Registered in mesh registry
```

The migration coordinator tracks:
- Migration phase (Planning → PreCopy → Transferring → Complete)
- Downtime (pause to resume time)
- Success/failure rates
- Total duration

## Cluster Architecture

```
┌─────────────────┐
│   Core Node     │  Bootstrap node, receives migrations
│   (8080)        │  DHT routing table
│   MeshService   │  Agent registry
└────────┬────────┘
         │
         │ QUIC/TLS
         │
┌────────┴────────┐
│   Relay Node    │  Gateway between core and edge
│   (8081)        │  Forwards discovery messages
│   MeshService   │  Gossip membership
└────────┬────────┘
         │
         │ QUIC/TLS
         │
┌────────┴────────┐
│   Edge Node     │  IoT device simulation
│   (8082)        │  Creates agent on startup
│   MeshService   │  Initiates migrations
└─────────────────┘
```

## Network Topology

The cluster uses a Docker bridge network (`mielin-mesh`) with:
- Subnet: 172.20.0.0/16
- DNS resolution between containers (e.g., `core:8080`)
- Health checks using `nc -z localhost 8080`

## Advanced Usage

### Execute Commands in Nodes

```bash
# Get mesh status from core node
./scripts/cluster.sh exec core /app/node --help

# Check relay node gossip membership
./scripts/cluster.sh exec relay cat /proc/self/status
```

### Inspect Network Traffic

```bash
# Watch QUIC traffic (requires tcpdump in container)
docker compose exec core tcpdump -i any -n port 8080
```

### Simulate Node Failure

```bash
# Stop relay node to test failure detection
docker compose stop relay

# Watch gossip detect failure in core logs
./scripts/cluster.sh logs core
```

You should see relay transition: Alive → Suspect (15s) → Dead (30s)

### Restart relay:
```bash
docker compose start relay
./scripts/cluster.sh logs relay
```

## Cluster Management Commands

| Command | Description |
|---------|-------------|
| `./scripts/cluster.sh start` | Start the cluster |
| `./scripts/cluster.sh stop` | Stop the cluster |
| `./scripts/cluster.sh restart` | Restart all nodes |
| `./scripts/cluster.sh status` | Show node health |
| `./scripts/cluster.sh logs [node]` | View logs |
| `./scripts/cluster.sh build` | Rebuild images |
| `./scripts/cluster.sh clean` | Remove all containers/volumes |
| `./scripts/cluster.sh test-migration` | Test live migration |

## Troubleshooting

### Cluster Won't Start

**Problem:** Nodes fail health checks

**Solution:**
```bash
# Check Docker resources
docker info | grep -E "CPUs|Total Memory"

# Rebuild images
./scripts/cluster.sh clean
./scripts/cluster.sh build
./scripts/cluster.sh start
```

### Nodes Can't Connect

**Problem:** Discovery fails, no peers found

**Solution:**
```bash
# Check DNS resolution
./scripts/cluster.sh exec relay ping -c 3 core

# Check if ports are bound
./scripts/cluster.sh exec core netstat -tuln | grep 8080
```

### Migration Fails

**Problem:** Agent migration returns error

**Solution:**
```bash
# Check if agent exists on edge
./scripts/cluster.sh logs edge | grep "Created agent"

# Verify core is reachable from edge
./scripts/cluster.sh exec edge nc -zv core 8080

# Check core logs for error details
./scripts/cluster.sh logs core | grep "Migration"
```

### High Memory Usage

**Problem:** Containers consuming too much RAM

**Solution:**
```bash
# Check memory usage
docker stats

# Restart cluster with fresh state
./scripts/cluster.sh restart
```

## Performance Metrics

Expected latency measurements in the cluster:

- **Peer Discovery**: < 1 second (bootstrap-based)
- **Gossip Convergence**: 5-10 seconds (all nodes see each other)
- **Agent Creation**: < 100ms
- **Migration Snapshot**: < 10ms (minimal agent)
- **Network Transfer**: < 50ms (localhost)
- **Agent Restoration**: < 100ms
- **Total Migration**: < 500ms (edge → core)

## Testing Scenarios

### 1. Clean Start Test
```bash
./scripts/cluster.sh clean
./scripts/cluster.sh build
./scripts/cluster.sh start
sleep 10
./scripts/cluster.sh status
```

### 2. Migration Round Trip
```bash
# Create agent on edge
./scripts/cluster.sh logs edge | grep "Created agent"

# Migrate to core
./scripts/cluster.sh test-migration

# Verify on core
./scripts/cluster.sh logs core | grep "Agent restored"
```

### 3. Failure Recovery Test
```bash
# Stop relay
docker compose stop relay

# Wait for gossip to detect failure (30s)
sleep 30
./scripts/cluster.sh logs core | grep "Dead"

# Restart relay
docker compose start relay

# Verify rejoin
./scripts/cluster.sh logs relay | grep "Gossip protocol started"
```

### 4. Multi-Agent Test
```bash
# Manually create multiple agents
./scripts/cluster.sh exec edge /app/node --agent
./scripts/cluster.sh exec edge /app/node --agent

# Check registry count
./scripts/cluster.sh logs edge | grep "Total agents"
```

## Next Steps

After validating the cluster:

1. **Scale Testing**: Add more nodes by modifying `docker-compose.yml`
2. **Embedded Testing**: Deploy to Raspberry Pi or similar hardware
3. **Performance Profiling**: Use `cargo flamegraph` for optimization
4. **Certificate Management**: Add Let's Encrypt integration for production
5. **Monitoring**: Integrate Prometheus/Grafana for observability

## References

- [QUIC Transport Implementation](../mielin-mesh/wire/src/transport.rs)
- [MeshService Orchestrator](../mielin-mesh/core/src/service.rs)
- [Gossip Protocol](../mielin-mesh/core/src/gossip.rs)
- [Migration Coordinator](../mielin-mesh/core/src/migration.rs)
- [Agent Registry](../mielin-mesh/core/src/registry.rs)

## Contributing

Found issues with cluster testing? Please report at:
https://github.com/cool-japan/mielin/issues

Include:
- Output of `./scripts/cluster.sh status`
- Relevant logs from `./scripts/cluster.sh logs`
- Docker version (`docker --version`)
- System info (`uname -a`)
