#!/bin/bash
# MielinOS CLI - Agent Operations Examples
# This file demonstrates common agent management tasks

# 1. List all agents
echo "=== Listing all agents ==="
mielinctl agent list
# Alias: mielinctl agent ls

# 2. List agents with filter
echo -e "\n=== List running agents ==="
mielinctl agent list --filter running

# 3. Create agent from WASM module
echo -e "\n=== Create agent ==="
mielinctl agent create \
    --name my-agent \
    --wasm-file ./agent.wasm \
    --env KEY1=value1 \
    --env KEY2=value2 \
    --memory 512 \
    --cpu-shares 2048 \
    --node node-001

# Minimal create
mielinctl agent create --name simple-agent --wasm-file ./simple.wasm

# 4. Deploy agent (migrate to target node)
echo -e "\n=== Deploy agent ==="
mielinctl agent deploy agent-001 --target node-002
# Alias: mielinctl agent dep agent-001 --target node-002

# 5. Inspect agent details
echo -e "\n=== Inspect agent ==="
mielinctl agent inspect agent-001
# Alias: mielinctl agent show agent-001

# 6. View agent logs
echo -e "\n=== View agent logs ==="
mielinctl agent logs agent-001 --tail 100 --follow
# Alias: mielinctl agent log agent-001 --tail 100 --follow

# 7. Execute command in agent
echo -e "\n=== Execute command in agent ==="
mielinctl agent exec agent-001 echo "Hello from agent"

# Interactive mode with TTY
mielinctl agent exec -it agent-001 /bin/sh
# Alias: mielinctl agent run -it agent-001 /bin/sh

# 8. Migrate agent to another node
echo -e "\n=== Migrate agent ==="
mielinctl agent migrate agent-001 --to node-003 --strategy live
# Alias: mielinctl agent move agent-001 --to node-003 --strategy live

# 9. Stop agent
echo -e "\n=== Stop agent ==="
mielinctl agent stop agent-001
# Alias: mielinctl agent kill agent-001

# 10. Agent operations with JSON output
echo -e "\n=== JSON output ==="
mielinctl agent list --output json
mielinctl agent inspect agent-001 --output yaml
