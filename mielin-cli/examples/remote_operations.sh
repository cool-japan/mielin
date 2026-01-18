#!/bin/bash
#
# Remote Operations Examples
# Demonstrates MielinCTL remote node management commands

set -e

echo "=== MielinCTL Remote Operations Examples ==="
echo

# Show remote nodes configuration path
echo "1. Show remote nodes configuration path:"
mielinctl remote config
echo

# List all remote nodes
echo "2. Listing all remote nodes:"
mielinctl remote list
echo

# List remote nodes with different output formats
echo "3. Listing remote nodes in JSON format:"
mielinctl remote list --output json
echo

echo "4. Listing remote nodes in YAML format:"
mielinctl remote list --output yaml
echo

# Add a remote node
echo "5. Add a remote node with API key authentication:"
TIMESTAMP=$(date +%s)
NODE_ID="demo-node-${TIMESTAMP}"
mielinctl remote add \
    --id "$NODE_ID" \
    --name "Demo Production Node" \
    --address "prod.example.com:8080" \
    --auth "apikey:my-secret-key-12345" \
    --tags "production,us-east" \
    --description "Production node in US East"
echo

# List nodes again to see the new node
echo "6. List nodes after adding new node:"
mielinctl remote list
echo

# Show node information
echo "7. Show node information:"
mielinctl remote info "$NODE_ID"
echo

# Filter nodes by tag
echo "8. Filter nodes by tag (production):"
mielinctl remote list --tag production
echo

# Update a remote node
echo "9. Update node description:"
mielinctl remote update "$NODE_ID" \
    --description "Updated production node in US East"
echo

# Test connection to remote node
echo "10. Test connection to remote node:"
mielinctl remote test "$NODE_ID"
echo

# Execute command on remote node
echo "11. Execute command on remote node:"
mielinctl remote execute "$NODE_ID" "agent list"
echo

# Execute command on multiple nodes with tag
echo "12. Execute command on all nodes with 'production' tag:"
echo "    mielinctl remote execute @tag:production \"node info\""
echo

# Using aliases
echo "13. Using remote command aliases:"
mielinctl nodes list
mielinctl remote path
echo

# Different authentication methods
echo "14. Different authentication methods:"
echo "    # No authentication (insecure)"
echo "    mielinctl remote add --id node1 --name \"Test Node\" \\"
echo "        --address \"localhost:8080\" --auth none"
echo ""
echo "    # API Key authentication"
echo "    mielinctl remote add --id node2 --name \"Prod Node\" \\"
echo "        --address \"prod.example.com:8080\" --auth \"apikey:secret-key\""
echo ""
echo "    # Token authentication"
echo "    mielinctl remote add --id node3 --name \"Secure Node\" \\"
echo "        --address \"secure.example.com:8080\" --auth \"token:bearer-token\""
echo

# Export remote nodes configuration
echo "15. Export remote nodes configuration:"
EXPORT_FILE=$(mktemp /tmp/remote_nodes_XXXXXX.toml)
mielinctl remote export "$EXPORT_FILE"
echo "Exported to: $EXPORT_FILE"
echo

# Show exported configuration
echo "16. Exported configuration content:"
cat "$EXPORT_FILE"
echo

# Import remote nodes
echo "17. Import remote nodes from file:"
echo "    mielinctl remote import /path/to/remote_nodes.toml"
echo

# Clean up exported file
rm -f "$EXPORT_FILE"

# Remove the demo node
echo "18. Remove the demo node:"
mielinctl remote remove "$NODE_ID" -y
echo

# Multi-cluster management example
echo "19. Multi-cluster management example:"
cat <<'EOF'
# Add nodes for different clusters
mielinctl remote add \
    --id prod-us-east-1 \
    --name "Production US East 1" \
    --address "prod-us-e1.example.com:8080" \
    --tags "production,us-east,primary" \
    --auth "apikey:secret-key-1"

mielinctl remote add \
    --id prod-us-east-2 \
    --name "Production US East 2" \
    --address "prod-us-e2.example.com:8080" \
    --tags "production,us-east,secondary" \
    --auth "apikey:secret-key-2"

mielinctl remote add \
    --id prod-eu-west-1 \
    --name "Production EU West 1" \
    --address "prod-eu-w1.example.com:8080" \
    --tags "production,eu-west,primary" \
    --auth "apikey:secret-key-3"

# Execute commands on all production nodes
mielinctl remote execute @tag:production "cluster status"

# Execute commands on specific region
mielinctl remote execute @tag:us-east "node list"

# List all nodes by cluster
mielinctl remote list --tag production
mielinctl remote list --tag us-east
mielinctl remote list --tag eu-west
EOF
echo

# Batch operations
echo "20. Batch operations on multiple nodes:"
echo "    # Test all production nodes"
echo "    for node in \$(mielinctl --quiet remote list --tag production); do"
echo "        mielinctl remote test \"\$node\""
echo "    done"
echo

# Configuration backup
echo "21. Configuration backup workflow:"
echo "    # Export current configuration"
echo "    mielinctl remote export remote_nodes_backup_\$(date +%Y%m%d).toml"
echo ""
echo "    # Restore from backup"
echo "    mielinctl remote import remote_nodes_backup_20241228.toml"
echo

# Quiet mode
echo "22. Quiet mode (minimal output):"
mielinctl --quiet remote list
echo

echo "=== Remote Operations Examples Complete ==="
