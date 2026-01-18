#!/bin/bash
# MielinOS CLI - Migration Operations Examples
# This file demonstrates agent migration tasks

# 1. Check migration status
echo "=== Migration status ==="
mielinctl migrate status
# Alias: mielinctl migrate st

# 2. View migration status for specific agent
echo -e "\n=== Agent migration status ==="
mielinctl migrate status --agent agent-001

# 3. Migrate an agent
echo -e "\n=== Migrate agent ==="
mielinctl migrate agent agent-001 \
    --to node-002 \
    --strategy live \
    --timeout 60

# Cold migration (stop, move, start)
mielinctl migrate agent agent-001 \
    --to node-002 \
    --strategy cold

# 4. Cancel a pending migration
echo -e "\n=== Cancel migration ==="
mielinctl migrate cancel migration-123
# Alias: mielinctl migrate abort migration-123

# 5. View migration history
echo -e "\n=== Migration history ==="
mielinctl migrate history
# Alias: mielinctl migrate hist

# With limit
mielinctl migrate history --limit 20

# 6. Complete migration workflow
echo -e "\n=== Complete migration workflow ==="

# Step 1: Check if agent is ready
if mielinctl agent inspect agent-001 --quiet; then
    echo "Agent found, proceeding with migration"

    # Step 2: Initiate migration
    MIGRATION_ID=$(mielinctl migrate agent agent-001 --to node-002 --output json | jq -r '.id')

    # Step 3: Monitor migration
    while true; do
        STATUS=$(mielinctl migrate status --quiet)
        if echo "$STATUS" | grep -q "completed"; then
            echo "Migration completed successfully"
            break
        elif echo "$STATUS" | grep -q "failed"; then
            echo "Migration failed"
            mielinctl migrate cancel "$MIGRATION_ID"
            exit 1
        fi
        sleep 5
    done
fi

# 7. View history in different formats
echo -e "\n=== Different formats ==="
mielinctl migrate history --output json
mielinctl migrate history --output yaml
