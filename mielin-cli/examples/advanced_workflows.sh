#!/bin/bash
# MielinOS CLI - Advanced Workflows Examples
# This file demonstrates complex real-world scenarios

# 1. Complete cluster bootstrap
echo "=== Cluster Bootstrap Workflow ==="

# Initialize first node (core)
mielinctl cluster init \
    --name production \
    --listen-addr 0.0.0.0:7000 \
    --role core &
CORE_PID=$!

sleep 5

# Add relay nodes for redundancy
for i in {1..3}; do
    mielinctl daemon \
        --listen-addr 0.0.0.0:700$i \
        --role relay \
        --bootstrap tcp://localhost:7000 &
done

sleep 10

# Add edge nodes for workloads
for i in {1..5}; do
    mielinctl daemon \
        --listen-addr 0.0.0.0:710$i \
        --role edge \
        --bootstrap tcp://localhost:7001 &
done

sleep 15

# Verify cluster health
mielinctl cluster health

# 2. Blue-Green Deployment
echo -e "\n=== Blue-Green Deployment ==="

# Deploy new version to "green" nodes
GREEN_NODES=("edge-01" "edge-02" "edge-03")

for node in "${GREEN_NODES[@]}"; do
    echo "Deploying to $node"

    # Create new agent with v2
    mielinctl agent create \
        --name "app-v2-$node" \
        --wasm-file ./app-v2.wasm \
        --node "$node"

    # Wait for health check
    sleep 5

    # Verify deployment
    if mielinctl agent inspect "app-v2-$node" --quiet; then
        echo "$node deployment successful"

        # Stop old version
        mielinctl agent stop "app-v1-$node"
    else
        echo "$node deployment failed, rolling back"
        mielinctl agent stop "app-v2-$node"
    fi
done

# 3. Load Balancing Scenario
echo -e "\n=== Load Balancing ==="

# Find least loaded node
NODES=$(mielinctl node list --output json)
LEAST_LOADED=$(echo "$NODES" | jq -r 'sort_by(.agent_count) | .[0].id')

echo "Deploying to least loaded node: $LEAST_LOADED"
mielinctl agent create \
    --name "lb-agent" \
    --wasm-file ./agent.wasm \
    --node "$LEAST_LOADED"

# 4. Disaster Recovery
echo -e "\n=== Disaster Recovery ==="

# Backup all agent states
mkdir -p ./backups/$(date +%Y%m%d)
AGENTS=$(mielinctl agent list --output json | jq -r '.[].id')

for agent in $AGENTS; do
    echo "Backing up $agent"
    mielinctl debug dump "$agent" > "./backups/$(date +%Y%m%d)/$agent.json"
done

# Backup cluster configuration
mielinctl cluster status --output json > "./backups/$(date +%Y%m%d)/cluster.json"
mielinctl node list --output json > "./backups/$(date +%Y%m%d)/nodes.json"

# 5. Auto-scaling Workflow
echo -e "\n=== Auto-scaling ==="

while true; do
    # Check cluster load
    TOTAL_AGENTS=$(mielinctl registry stats --output json | jq -r '.total_agents')
    TOTAL_NODES=$(mielinctl node list --output json | jq -r 'length')
    AVG_LOAD=$((TOTAL_AGENTS / TOTAL_NODES))

    echo "Average load: $AVG_LOAD agents per node"

    # Scale up if needed
    if [ $AVG_LOAD -gt 10 ]; then
        echo "High load detected, scaling up"
        NEW_NODE_ID="edge-autoscale-$(date +%s)"
        NEW_PORT=$((7100 + TOTAL_NODES))

        mielinctl daemon \
            --node-id "$NEW_NODE_ID" \
            --listen-addr "0.0.0.0:$NEW_PORT" \
            --role edge \
            --bootstrap tcp://localhost:7000 &

        sleep 10
    fi

    # Scale down if needed
    if [ $AVG_LOAD -lt 3 ]; then
        echo "Low load detected, consider scaling down"
        # Implement scale-down logic here
    fi

    sleep 60
done

# 6. Health Monitoring and Alerting
echo -e "\n=== Health Monitoring ==="

while true; do
    # Check cluster health
    if ! mielinctl cluster health --quiet; then
        echo "ALERT: Cluster unhealthy at $(date)"

        # Check individual nodes
        UNHEALTHY_NODES=$(mielinctl node list --output json | \
            jq -r '.[] | select(.status != "healthy") | .id')

        echo "Unhealthy nodes: $UNHEALTHY_NODES"

        # Send alert (integrate with your alerting system)
        # curl -X POST https://alerts.example.com/webhook \
        #   -d "{'message': 'Cluster unhealthy', 'nodes': '$UNHEALTHY_NODES'}"
    fi

    # Check agent health
    FAILED_AGENTS=$(mielinctl agent list --output json | \
        jq -r '.[] | select(.status == "failed") | .id')

    if [ -n "$FAILED_AGENTS" ]; then
        echo "ALERT: Failed agents at $(date): $FAILED_AGENTS"

        # Auto-restart failed agents
        for agent in $FAILED_AGENTS; do
            echo "Restarting $agent"
            # Implement restart logic
        done
    fi

    sleep 30
done

# 7. Multi-region Deployment
echo -e "\n=== Multi-region Deployment ==="

REGIONS=("us-east" "us-west" "eu-central" "ap-southeast")

for region in "${REGIONS[@]}"; do
    echo "Deploying to $region"

    # Start regional core node
    mielinctl daemon \
        --node-id "$region-core" \
        --listen-addr "0.0.0.0:$((7000 + RANDOM % 1000))" \
        --role core &

    sleep 5

    # Deploy regional agents
    mielinctl agent create \
        --name "regional-app-$region" \
        --wasm-file ./regional-app.wasm \
        --node "$region-core"
done

# 8. Canary Deployment
echo -e "\n=== Canary Deployment ==="

# Deploy canary version (10% traffic)
CANARY_NODE="edge-01"
mielinctl agent create \
    --name "app-canary" \
    --wasm-file ./app-v2.wasm \
    --node "$CANARY_NODE"

# Monitor canary
sleep 300

# Check canary health
if mielinctl agent inspect "app-canary" --quiet; then
    echo "Canary healthy, proceeding with full rollout"

    # Deploy to remaining nodes
    for node in edge-{02..10}; do
        mielinctl agent create \
            --name "app-v2-$node" \
            --wasm-file ./app-v2.wasm \
            --node "$node"
    done
else
    echo "Canary unhealthy, rolling back"
    mielinctl agent stop "app-canary"
fi

# 9. Performance Testing
echo -e "\n=== Performance Testing ==="

# Deploy test agents
for i in {1..100}; do
    mielinctl agent create \
        --name "perf-test-$i" \
        --wasm-file ./perf-test.wasm \
        --memory 128 \
        --cpu-shares 512
done

# Monitor performance
START_TIME=$(date +%s)
while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))

    if [ $ELAPSED -gt 300 ]; then
        break
    fi

    echo "=== Performance at ${ELAPSED}s ==="
    mielinctl cluster status
    mielinctl gossip status

    sleep 10
done

# Cleanup
for i in {1..100}; do
    mielinctl agent stop "perf-test-$i"
done

# 10. CI/CD Integration
echo -e "\n=== CI/CD Pipeline ==="

# Build
echo "Building WASM agent..."
mielinctl wasm build --source ./src --output ./build/agent.wasm || exit 1

# Test
echo "Testing WASM agent..."
mielinctl wasm test ./build/agent.wasm || exit 1

# Optimize
echo "Optimizing WASM agent..."
mielinctl wasm optimize \
    --input ./build/agent.wasm \
    --output ./build/agent-opt.wasm || exit 1

# Deploy to staging
echo "Deploying to staging..."
mielinctl agent create \
    --name "app-staging" \
    --wasm-file ./build/agent-opt.wasm \
    --node staging-node || exit 1

# Run integration tests
sleep 30
# (run your integration tests here)

# Deploy to production
echo "Deploying to production..."
mielinctl cluster upgrade --version $(cat VERSION)

echo "Deployment complete!"
