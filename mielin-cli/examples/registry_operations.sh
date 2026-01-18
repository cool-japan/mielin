#!/bin/bash
# MielinOS CLI - Registry Operations Examples
# This file demonstrates agent registry tasks

# 1. List all agents in registry
echo "=== List all agents ==="
mielinctl registry list
# Alias: mielinctl registry ls

# 2. Query agents by pattern
echo -e "\n=== Query agents ==="
mielinctl registry query "web-*"
# Alias: mielinctl registry search "web-*"

# Query by specific fields
mielinctl registry query "status:running"
mielinctl registry query "node:node-001"

# 3. View registry statistics
echo -e "\n=== Registry statistics ==="
mielinctl registry stats
# Alias: mielinctl registry info

# 4. Different output formats
echo -e "\n=== JSON output ==="
mielinctl registry list --output json

echo -e "\n=== YAML output ==="
mielinctl registry query "web-*" --output yaml

# 5. Combined registry operations
echo -e "\n=== Find and inspect agents ==="

# Find agents matching pattern
AGENTS=$(mielinctl registry query "backend-*" --output json | jq -r '.[].id')

# Inspect each agent
for agent in $AGENTS; do
    echo "Inspecting $agent"
    mielinctl agent inspect "$agent"
done

# 6. Monitor registry
echo -e "\n=== Monitor registry ==="
watch -n 5 'mielinctl registry stats'

# 7. Registry health check
echo -e "\n=== Registry health check ==="
TOTAL=$(mielinctl registry stats --output json | jq -r '.total_agents')
RUNNING=$(mielinctl registry stats --output json | jq -r '.running_agents')

echo "Total agents: $TOTAL"
echo "Running agents: $RUNNING"

if [ "$RUNNING" -eq "$TOTAL" ]; then
    echo "All agents are running"
else
    echo "Warning: Some agents are not running"
fi
