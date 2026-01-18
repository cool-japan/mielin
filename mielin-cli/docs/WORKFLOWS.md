# MielinOS CLI - Common Workflows

Real-world workflows and best practices for using `mielinctl`.

## Table of Contents

- [Cluster Management](#cluster-management)
- [Agent Deployment](#agent-deployment)
- [Development Workflows](#development-workflows)
- [Production Operations](#production-operations)
- [Monitoring and Debugging](#monitoring-and-debugging)
- [Migration Scenarios](#migration-scenarios)

## Cluster Management

### Bootstrap a Production Cluster

Set up a multi-node production cluster with core, relay, and edge nodes.

```bash
# Step 1: Initialize core node
mielinctl cluster init \
  --name production \
  --listen-addr 0.0.0.0:7000 \
  --role core

# Wait for core node to stabilize
sleep 10

# Step 2: Add relay nodes for redundancy
for i in {1..3}; do
  mielinctl daemon \
    --node-id relay-0$i \
    --listen-addr 0.0.0.0:700$i \
    --role relay \
    --bootstrap tcp://core-node:7000 &
done

# Wait for relay nodes to join
sleep 15

# Step 3: Add edge nodes for workloads
for i in {1..5}; do
  mielinctl daemon \
    --node-id edge-0$i \
    --listen-addr 0.0.0.0:710$i \
    --role edge \
    --bootstrap tcp://relay-01:7001 &
done

# Step 4: Verify cluster health
mielinctl cluster health
mielinctl node list
mielinctl mesh status
```

**Best Practices:**
- Always start with core nodes first
- Use relay nodes to distribute cluster load
- Deploy edge nodes closest to your workloads
- Wait for nodes to stabilize before adding more

### Scale the Cluster

Add or remove nodes based on demand.

**Adding Nodes:**
```bash
# Determine current load
NODES=$(mielinctl node list --output json)
AVG_AGENTS=$(echo "$NODES" | jq '[.[].agents] | add / length')

if [ "$AVG_AGENTS" -gt 10 ]; then
  echo "High load detected, adding node..."
  mielinctl daemon \
    --role edge \
    --bootstrap tcp://relay-01:7001 &
fi
```

**Removing Nodes:**
```bash
# Drain node before removal
NODE_ID="edge-05"

# Migrate all agents off the node
AGENTS=$(mielinctl agent list --output json | jq -r ".[] | select(.node==\"$NODE_ID\") | .id")
for agent in $AGENTS; do
  mielinctl agent migrate $agent --to edge-01
done

# Leave cluster
mielinctl node leave --drain

# Stop node
mielinctl node stop $NODE_ID
```

### Rolling Cluster Upgrade

Upgrade the cluster to a new version with zero downtime.

```bash
# Step 1: Pre-upgrade health check
if ! mielinctl cluster health --quiet; then
  echo "Cluster unhealthy, aborting upgrade"
  exit 1
fi

# Step 2: Backup configuration
mielinctl cluster status --output json > cluster-backup-$(date +%Y%m%d).json
mielinctl node list --output json > nodes-backup-$(date +%Y%m%d).json

# Step 3: Perform rolling upgrade
mielinctl cluster upgrade \
  --version 0.2.0 \
  --batch-size 2 \
  --wait-time 60

# Step 4: Verify upgrade
mielinctl cluster health
mielinctl node list --output json | jq -r '.[].version' | sort | uniq
```

**Best Practices:**
- Always health check before upgrading
- Backup configuration before major upgrades
- Start with small batch sizes (1-2 nodes)
- Monitor cluster health during upgrade
- Keep previous version binaries for rollback

## Agent Deployment

### Deploy a Simple Agent

Basic agent deployment workflow.

```bash
# Step 1: Build WASM module
cd my-agent
cargo build --target wasm32-wasi --release
cp target/wasm32-wasi/release/my_agent.wasm ../

# Step 2: Validate
cd ..
mielinctl wasm validate my_agent.wasm

# Step 3: Optimize
mielinctl wasm optimize \
  --input my_agent.wasm \
  --output my_agent_opt.wasm

# Step 4: Deploy
AGENT_ID=$(mielinctl agent create \
  --name my-agent \
  --wasm-file my_agent_opt.wasm \
  --memory 256 \
  --output json | jq -r '.id')

# Step 5: Verify
mielinctl agent inspect $AGENT_ID
mielinctl agent logs $AGENT_ID --tail 50
```

### Blue-Green Deployment

Deploy a new version with zero downtime.

```bash
# Step 1: Deploy green version
GREEN_IDS=()
for i in {1..3}; do
  ID=$(mielinctl agent create \
    --name app-v2-$i \
    --wasm-file app-v2.wasm \
    --memory 512 \
    --env VERSION=2.0 \
    --output json | jq -r '.id')
  GREEN_IDS+=($ID)
done

# Step 2: Health check green version
sleep 30
ALL_HEALTHY=true
for id in "${GREEN_IDS[@]}"; do
  if ! mielinctl agent inspect $id --quiet | grep -q "Running"; then
    ALL_HEALTHY=false
    break
  fi
done

# Step 3: Switch or rollback
if [ "$ALL_HEALTHY" = true ]; then
  echo "Green deployment healthy, switching traffic..."

  # Stop blue version
  for i in {1..3}; do
    mielinctl agent stop app-v1-$i
  done

  echo "Deployment successful"
else
  echo "Green deployment unhealthy, rolling back..."

  # Stop green version
  for id in "${GREEN_IDS[@]}"; do
    mielinctl agent stop $id
  done

  echo "Rollback complete"
fi
```

### Canary Deployment

Gradually roll out new version.

```bash
# Step 1: Deploy canary (10% traffic)
CANARY_ID=$(mielinctl agent create \
  --name app-canary \
  --wasm-file app-v2.wasm \
  --memory 512 \
  --output json | jq -r '.id')

# Step 2: Monitor canary (5 minutes)
echo "Monitoring canary for 5 minutes..."
sleep 300

# Step 3: Check canary health
if mielinctl agent inspect $CANARY_ID --quiet | grep -q "Running"; then
  echo "Canary healthy, proceeding with rollout..."

  # Deploy to remaining nodes (gradually)
  for i in {2..10}; do
    mielinctl agent create \
      --name app-v2-$i \
      --wasm-file app-v2.wasm \
      --memory 512

    # Wait and health check
    sleep 60
  done

  # Stop old version
  for i in {1..10}; do
    mielinctl agent stop app-v1-$i
  done
else
  echo "Canary failed, rolling back..."
  mielinctl agent stop $CANARY_ID
fi
```

## Development Workflows

### Local Development Setup

Set up a local development environment.

```bash
# Step 1: Start local cluster
mielinctl daemon --role core --node-id dev-local &
DEV_PID=$!

# Wait for startup
sleep 5

# Step 2: Create development config
cat > dev-config.toml <<EOF
[node]
id = "dev-local"
role = "core"

[cli]
default_output_format = "json"
enable_colors = true
command_timeout_secs = 60

[daemon]
enable_mdns = true
enable_gossip = true
EOF

# Step 3: Build and deploy test agent
cd my-agent
cargo watch -x 'build --target wasm32-wasi --release' -s '
  mielinctl wasm validate target/wasm32-wasi/release/my_agent.wasm &&
  mielinctl agent create --name test-agent --wasm-file target/wasm32-wasi/release/my_agent.wasm
'
```

**Development Tips:**
- Use `cargo watch` for automatic rebuilds
- Enable debug logging for detailed output
- Use shorter timeouts for faster iteration
- Keep test data in a separate directory

### Test Before Deploy

Comprehensive testing workflow.

```bash
# Step 1: Build
mielinctl wasm build --source . --output agent.wasm

# Step 2: Validate
if ! mielinctl wasm validate agent.wasm; then
  echo "Validation failed"
  exit 1
fi

# Step 3: Unit test
if ! mielinctl wasm test agent.wasm; then
  echo "Unit tests failed"
  exit 1
fi

# Step 4: Integration test in isolated environment
TEST_ID=$(mielinctl agent create \
  --name test-agent \
  --wasm-file agent.wasm \
  --env TEST_MODE=true \
  --output json | jq -r '.id')

# Wait for startup
sleep 10

# Run integration tests
mielinctl agent exec $TEST_ID /test/run_tests.sh

# Cleanup
mielinctl agent stop $TEST_ID

# Step 5: Deploy to staging
mielinctl agent create \
  --name staging-agent \
  --wasm-file agent.wasm \
  --node staging-node
```

## Production Operations

### Health Monitoring

Continuous health monitoring script.

```bash
#!/bin/bash
# health-monitor.sh

ALERT_THRESHOLD=80
CHECK_INTERVAL=60

while true; do
  # Check cluster health
  if ! mielinctl cluster health --quiet; then
    echo "ALERT: Cluster unhealthy at $(date)"
    # Send alert (integrate with your alerting system)
    # curl -X POST https://alerts.example.com/webhook -d '{"message": "Cluster unhealthy"}'
  fi

  # Check node resources
  NODES=$(mielinctl node list --output json)

  # Find overloaded nodes
  OVERLOADED=$(echo "$NODES" | jq -r ".[] | select(.agents > 20) | .id")
  if [ -n "$OVERLOADED" ]; then
    echo "ALERT: Overloaded nodes: $OVERLOADED"
  fi

  # Check failed agents
  FAILED=$(mielinctl agent list --output json | jq -r '.[] | select(.state=="Error") | .id')
  if [ -n "$FAILED" ]; then
    echo "ALERT: Failed agents: $FAILED"

    # Auto-restart failed agents
    for agent in $FAILED; do
      echo "Restarting $agent..."
      # Implement restart logic
    done
  fi

  # Check gossip protocol
  GOSSIP=$(mielinctl gossip status --output json)
  DEAD_MEMBERS=$(echo "$GOSSIP" | jq -r '[.members[] | select(.state=="Dead")] | length')
  if [ "$DEAD_MEMBERS" -gt 0 ]; then
    echo "ALERT: $DEAD_MEMBERS dead members detected"
    mielinctl gossip sync
  fi

  sleep $CHECK_INTERVAL
done
```

### Backup and Restore

Backup cluster state and restore if needed.

**Backup:**
```bash
#!/bin/bash
# backup.sh

BACKUP_DIR="./backups/$(date +%Y%m%d_%H%M%S)"
mkdir -p $BACKUP_DIR

# Backup cluster configuration
mielinctl cluster status --output json > $BACKUP_DIR/cluster.json

# Backup all nodes
mielinctl node list --output json > $BACKUP_DIR/nodes.json

# Backup all agents
mielinctl agent list --output json > $BACKUP_DIR/agents.json

# Backup agent states
AGENTS=$(mielinctl agent list --output json | jq -r '.[].id')
mkdir -p $BACKUP_DIR/agent_states
for agent in $AGENTS; do
  mielinctl debug dump $agent --memory > $BACKUP_DIR/agent_states/$agent.json
done

# Backup configuration
cp ~/.config/mielin/config.toml $BACKUP_DIR/

echo "Backup completed: $BACKUP_DIR"
```

**Restore:**
```bash
#!/bin/bash
# restore.sh

BACKUP_DIR=$1

if [ -z "$BACKUP_DIR" ]; then
  echo "Usage: $0 <backup_directory>"
  exit 1
fi

# Restore configuration
cp $BACKUP_DIR/config.toml ~/.config/mielin/

# Restore agents
AGENTS=$(cat $BACKUP_DIR/agents.json | jq -r '.[]')

for agent in $AGENTS; do
  NAME=$(echo $agent | jq -r '.name')
  # Recreate agent from backup
  # (requires WASM modules to be available)
done

echo "Restore completed from: $BACKUP_DIR"
```

### Log Aggregation

Collect logs from all agents.

```bash
#!/bin/bash
# collect-logs.sh

OUTPUT_DIR="./logs/$(date +%Y%m%d)"
mkdir -p $OUTPUT_DIR

# Get all agents
AGENTS=$(mielinctl agent list --output json | jq -r '.[].id')

# Collect logs from each agent
for agent in $AGENTS; do
  echo "Collecting logs for $agent..."
  mielinctl agent logs $agent --tail 1000 > $OUTPUT_DIR/$agent.log
done

# Create aggregate log
cat $OUTPUT_DIR/*.log | sort -t'|' -k1 > $OUTPUT_DIR/aggregate.log

# Compress
tar czf logs-$(date +%Y%m%d).tar.gz $OUTPUT_DIR/

echo "Logs collected: logs-$(date +%Y%m%d).tar.gz"
```

## Monitoring and Debugging

### Debug Performance Issues

Identify and resolve performance problems.

```bash
# Step 1: Identify slow agents
AGENTS=$(mielinctl agent list --output json)
SLOW_AGENTS=$(echo "$AGENTS" | jq -r '.[] | select(.memory_mb > 1000) | .id')

# Step 2: Profile each slow agent
for agent in $SLOW_AGENTS; do
  echo "Profiling $agent..."

  # CPU profile
  mielinctl debug profile $agent --cpu > profile_$agent_cpu.json

  # Memory profile
  mielinctl debug profile $agent --mem > profile_$agent_mem.json

  # Execution trace
  mielinctl debug trace $agent --duration 60 --file trace_$agent.json
done

# Step 3: Analyze profiles
# (Use external tools or custom scripts)

# Step 4: Dump state for offline analysis
for agent in $SLOW_AGENTS; do
  mielinctl debug dump $agent --memory > dump_$agent.json
done
```

### Debug Network Issues

Troubleshoot mesh network problems.

```bash
# Step 1: Check mesh status
mielinctl mesh status

# Step 2: Check gossip protocol
mielinctl gossip status

# Step 3: List all members and their states
mielinctl gossip members --output json | jq -r '.[] | "\(.id): \(.state)"'

# Step 4: Identify dead or suspect members
DEAD=$(mielinctl gossip members --output json | jq -r '.[] | select(.state=="Dead") | .id')

if [ -n "$DEAD" ]; then
  echo "Dead members detected: $DEAD"

  # Force synchronization
  mielinctl gossip sync

  # Wait and recheck
  sleep 10
  mielinctl gossip members
fi

# Step 5: Check connectivity to specific peer
PEER_ID="peer-abc123"
if mielinctl mesh peers --output json | jq -r '.[].id' | grep -q $PEER_ID; then
  echo "Connected to $PEER_ID"
else
  echo "Not connected to $PEER_ID"
  # Investigate firewall or network issues
fi
```

## Migration Scenarios

### Load Balancing Migration

Rebalance agents across nodes.

```bash
# Step 1: Find most loaded node
NODES=$(mielinctl node list --output json)
MOST_LOADED=$(echo "$NODES" | jq -r 'sort_by(.agents) | reverse | .[0]')
MOST_LOADED_ID=$(echo "$MOST_LOADED" | jq -r '.id')
LEAST_LOADED_ID=$(echo "$NODES" | jq -r 'sort_by(.agents) | .[0].id')

echo "Rebalancing from $MOST_LOADED_ID to $LEAST_LOADED_ID"

# Step 2: Get agents to migrate
AGENTS_TO_MIGRATE=$(mielinctl agent list --output json | \
  jq -r ".[] | select(.node==\"$MOST_LOADED_ID\") | .id" | \
  head -5)

# Step 3: Migrate agents
for agent in $AGENTS_TO_MIGRATE; do
  echo "Migrating $agent..."
  mielinctl agent migrate $agent --to $LEAST_LOADED_ID --strategy live
  sleep 10
done

# Step 4: Verify migration
mielinctl node list
```

### Disaster Recovery Migration

Migrate all agents from failed node.

```bash
# Step 1: Detect failed node
FAILED_NODE="edge-03"
if ! mielinctl node info $FAILED_NODE --quiet; then
  echo "Node $FAILED_NODE has failed"

  # Step 2: Get all agents on failed node
  AGENTS=$(mielinctl agent list --output json | \
    jq -r ".[] | select(.node==\"$FAILED_NODE\") | .id")

  # Step 3: Find replacement nodes
  HEALTHY_NODES=$(mielinctl node list --output json | \
    jq -r '.[] | select(.state=="Running" and .role=="edge") | .id')

  # Step 4: Distribute agents to healthy nodes
  i=0
  for agent in $AGENTS; do
    TARGET=$(echo "$HEALTHY_NODES" | sed -n "$((i % 3 + 1))p")
    echo "Migrating $agent to $TARGET..."
    mielinctl agent migrate $agent --to $TARGET --strategy cold
    i=$((i + 1))
  done
fi
```

## See Also

- [Quickstart Guide](./QUICKSTART.md)
- [Command Reference](./COMMAND_REFERENCE.md)
- [Troubleshooting](./TROUBLESHOOTING.md)
- [Examples](../examples/)
