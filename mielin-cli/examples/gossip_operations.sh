#!/bin/bash
# MielinOS CLI - Gossip Protocol Operations Examples
# This file demonstrates gossip protocol management

# 1. View gossip status
echo "=== Gossip status ==="
mielinctl gossip status
# Alias: mielinctl gossip st

# 2. List all members
echo -e "\n=== List all members ==="
mielinctl gossip members
# Alias: mielinctl gossip ls

# 3. List only alive members
echo -e "\n=== List alive members ==="
mielinctl gossip members --alive

# 4. Force synchronization
echo -e "\n=== Force sync ==="
mielinctl gossip sync
# Alias: mielinctl gossip synchronize

# Sync with specific peer
mielinctl gossip sync peer-abc123

# 5. Different output formats
echo -e "\n=== JSON output ==="
mielinctl gossip status --output json
mielinctl gossip members --output json

# 6. Monitor gossip protocol
echo -e "\n=== Monitor gossip ==="
watch -n 3 'mielinctl gossip status'

# 7. Gossip health check
echo -e "\n=== Health check workflow ==="

# Check if gossip is operational
STATUS=$(mielinctl gossip status --output json)
MESSAGES=$(echo "$STATUS" | jq -r '.messages_sent')

if [ "$MESSAGES" -gt 0 ]; then
    echo "Gossip protocol is operational"
    echo "Messages sent: $MESSAGES"
else
    echo "Warning: Gossip protocol may not be working"
fi

# 8. Debug gossip issues
echo -e "\n=== Debug workflow ==="

echo "Step 1: Check gossip status"
mielinctl gossip status

echo "Step 2: Check member states"
mielinctl gossip members

echo "Step 3: Force sync if needed"
DEAD_MEMBERS=$(mielinctl gossip members --output json | jq -r '[.[] | select(.state=="Dead")] | length')
if [ "$DEAD_MEMBERS" -gt 0 ]; then
    echo "Found $DEAD_MEMBERS dead members, forcing sync"
    mielinctl gossip sync
fi

# 9. Continuous monitoring
echo -e "\n=== Continuous monitoring ==="
while true; do
    clear
    echo "=== Gossip Status at $(date) ==="
    mielinctl gossip status
    echo ""
    echo "=== Active Members ==="
    mielinctl gossip members --alive
    sleep 5
done
