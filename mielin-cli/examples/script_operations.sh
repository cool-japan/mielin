#!/bin/bash
#
# Script Operations Examples
# Demonstrates MielinCTL script management commands

set -e

echo "=== MielinCTL Script Operations Examples ==="
echo

# List all installed scripts
echo "1. Listing all installed scripts:"
mielinctl script list
echo

# List scripts with different output formats
echo "2. Listing scripts in JSON format:"
mielinctl script list --output json
echo

echo "3. Listing scripts in YAML format:"
mielinctl script list --output yaml
echo

# Filter scripts by tag
echo "4. Filter scripts by tag:"
mielinctl script list --tag automation || echo "No scripts with 'automation' tag"
echo

# Show script directory
echo "5. Show script directory path:"
mielinctl script directory
echo

# Create a new script template
echo "6. Create a new script template:"
TEMP_SCRIPT=$(mktemp /tmp/test_script_XXXXXX.rhai)
mielinctl script create my-automation --file "$TEMP_SCRIPT"
echo "Script template created at: $TEMP_SCRIPT"
echo

# Show template content
echo "7. Script template content:"
cat "$TEMP_SCRIPT"
echo

# Install the script
echo "8. Install the script:"
mielinctl script install "$TEMP_SCRIPT" || echo "Script installation (stub)"
echo

# List scripts again
echo "9. List scripts after installation:"
mielinctl script list
echo

# Show script information
echo "10. Show script information (if available):"
mielinctl script info my-automation || echo "Script info not found (expected for stub)"
echo

# Script execution example
echo "11. Execute a script with arguments:"
echo "    mielinctl script execute my-automation key1=value1 key2=value2"
echo

# Example Rhai script structure
echo "12. Example Rhai script structure:"
cat <<'EOF'
// Script: backup-agent
// Version: 1.0.0
// Description: Backup agent data to remote storage
// Author: Your Name
// Tags: backup, automation
// RequiredVersion: 0.1.0

fn main(args) {
    print("Starting backup operation...");

    // Access arguments
    let agent_id = args.get("agent_id");
    let destination = args.get("destination");

    print("Backing up agent: " + agent_id);
    print("Destination: " + destination);

    // Perform backup operations here

    return #{
        status: "success",
        message: "Backup completed successfully"
    };
}

// Execute main
main(args)
EOF
echo

# Script execution with arguments
echo "13. Execute script with arguments (example):"
echo "    mielinctl script execute backup-agent agent_id=agent-001 destination=/backup"
echo

# Edit a script
echo "14. Edit a script in \$EDITOR:"
echo "    mielinctl script edit my-automation"
echo

# Reload scripts
echo "15. Reload all scripts:"
mielinctl script reload
echo

# Using aliases
echo "16. Using script command aliases:"
mielinctl scripts list
mielinctl script dir
echo

# Quiet mode
echo "17. Quiet mode (minimal output):"
mielinctl --quiet script list
echo

# Uninstall the script
echo "18. Uninstall the script:"
mielinctl script uninstall my-automation -y || echo "Script uninstall (stub)"
echo

# Clean up temporary file
rm -f "$TEMP_SCRIPT"

echo "19. Script management workflow:"
echo "    # Create a new script"
echo "    mielinctl script create my-script --file my-script.rhai"
echo ""
echo "    # Edit the script"
echo "    \$EDITOR my-script.rhai"
echo ""
echo "    # Install the script"
echo "    mielinctl script install my-script.rhai"
echo ""
echo "    # Test the script"
echo "    mielinctl script execute my-script arg1=value1"
echo ""
echo "    # View script logs/info"
echo "    mielinctl script info my-script"
echo

echo "=== Script Operations Examples Complete ==="
