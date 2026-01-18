#!/bin/bash
# MielinOS CLI - Cluster Operations Examples
# This file demonstrates cluster-wide management tasks

# 1. Initialize a new cluster
echo "=== Initialize cluster ==="
mielinctl cluster init \
    --name production-cluster \
    --listen-addr 0.0.0.0:7000 \
    --role core
# Alias: mielinctl cluster setup

# 2. View cluster status
echo -e "\n=== Cluster status ==="
mielinctl cluster status
# Alias: mielinctl cluster st

# With JSON output
mielinctl cluster status --output json

# 3. Health check all nodes
echo -e "\n=== Health check ==="
mielinctl cluster health
# Alias: mielinctl cluster check

# 4. Rolling cluster upgrade
echo -e "\n=== Rolling upgrade ==="
mielinctl cluster upgrade \
    --version 0.2.0 \
    --batch-size 2 \
    --wait-time 60

# Upgrade with health check skip (not recommended)
mielinctl cluster upgrade --version 0.2.0 --skip-health-check

# Upgrade one node at a time (safest)
mielinctl cluster upgrade --version 0.2.0 --batch-size 1 --wait-time 30

# 5. Combined cluster operations
echo -e "\n=== Combined operations ==="

# Check health before upgrade
if mielinctl cluster health --quiet; then
    echo "Cluster healthy, proceeding with upgrade"
    mielinctl cluster upgrade --version 0.2.0
else
    echo "Cluster unhealthy, aborting upgrade"
    exit 1
fi

# 6. Monitoring cluster status
echo -e "\n=== Monitor cluster ==="
watch -n 5 'mielinctl cluster status'

# 7. Cluster info in different formats
echo -e "\n=== Different output formats ==="
mielinctl cluster status --output table
mielinctl cluster status --output json
mielinctl cluster status --output yaml
