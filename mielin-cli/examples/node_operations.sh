#!/bin/bash
# MielinOS CLI - Node Operations Examples
# This file demonstrates common node management tasks

# 1. List all nodes in the cluster
echo "=== Listing all nodes ==="
mielinctl node list
# Alias: mielinctl node ls

# 2. List nodes with JSON output
echo -e "\n=== Listing nodes (JSON format) ==="
mielinctl node list --output json

# 3. Show detailed information about a specific node
echo -e "\n=== Show node details ==="
mielinctl node info node-001
# Alias: mielinctl node show node-001

# 4. Create a new node with specific role
echo -e "\n=== Create new node ==="
mielinctl node create --role core --listen-addr 0.0.0.0:7001
# Alias: mielinctl node new --role core --listen-addr 0.0.0.0:7001

# 5. Join an existing cluster
echo -e "\n=== Join cluster ==="
mielinctl node join --bootstrap tcp://192.168.1.100:7000
# Alias: mielinctl node connect --bootstrap tcp://192.168.1.100:7000

# 6. View node configuration
echo -e "\n=== View all configuration ==="
mielinctl node config --all

# 7. Get specific configuration value
echo -e "\n=== Get node ID ==="
mielinctl node config node.id

# 8. Set configuration value
echo -e "\n=== Set node role ==="
mielinctl node config node.role edge

# 9. Start a node
echo -e "\n=== Start node ==="
mielinctl node start node-001

# 10. Stop a node
echo -e "\n=== Stop node ==="
mielinctl node stop node-001
# Alias: mielinctl node kill node-001

# 11. Leave cluster gracefully
echo -e "\n=== Leave cluster ==="
mielinctl node leave --drain
# Alias: mielinctl node disconnect --drain

# 12. Quiet mode (minimal output)
echo -e "\n=== Quiet mode ==="
mielinctl node list --quiet
