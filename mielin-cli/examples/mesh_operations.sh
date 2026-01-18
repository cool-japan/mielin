#!/bin/bash
# MielinOS CLI - Mesh Network Operations Examples
# This file demonstrates mesh networking tasks

# 1. View mesh status
echo "=== Mesh status ==="
mielinctl mesh status
# Alias: mielinctl mesh st

# 2. List all peers
echo -e "\n=== List peers ==="
mielinctl mesh peers
# Alias: mielinctl mesh ls

# 3. View with different output formats
echo -e "\n=== JSON output ==="
mielinctl mesh status --output json

echo -e "\n=== YAML output ==="
mielinctl mesh peers --output yaml

# 4. Monitor mesh in real-time
echo -e "\n=== Monitor mesh ==="
watch -n 2 'mielinctl mesh status'

# 5. Check mesh connectivity
echo -e "\n=== Check connectivity ==="
if mielinctl mesh peers --quiet | grep -q "connected"; then
    echo "Mesh network is operational"
else
    echo "Warning: Mesh network may have issues"
fi

# 6. Mesh debugging workflow
echo -e "\n=== Debugging workflow ==="
echo "Step 1: Check mesh status"
mielinctl mesh status

echo "Step 2: List all peers"
mielinctl mesh peers

echo "Step 3: If issues, check gossip"
mielinctl gossip status
