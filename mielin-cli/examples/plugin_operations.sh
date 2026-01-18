#!/bin/bash
#
# Plugin Operations Examples
# Demonstrates MielinCTL plugin management commands

set -e

echo "=== MielinCTL Plugin Operations Examples ==="
echo

# List all installed plugins
echo "1. Listing all installed plugins:"
mielinctl plugin list
echo

# List plugins with different output formats
echo "2. Listing plugins in JSON format:"
mielinctl plugin list --output json
echo

echo "3. Listing plugins in YAML format:"
mielinctl plugin list --output yaml
echo

# Show plugin directory
echo "4. Show plugin directory path:"
mielinctl plugin directory
echo

# Using aliases
echo "5. Using plugin command aliases:"
mielinctl plugins ls
mielinctl plugin dir
echo

# Show plugin information (if a plugin exists)
echo "6. Show plugin information (example - may fail if plugin doesn't exist):"
mielinctl plugin info my-custom-plugin || echo "Plugin not found (expected if no plugins installed)"
echo

# Reload plugins
echo "7. Reload all plugins:"
mielinctl plugin reload
echo

# Plugin installation example (requires plugin directory structure)
echo "8. Install plugin (example - requires actual plugin directory):"
echo "   mielinctl plugin install /path/to/plugin/directory"
echo "   Plugin directory structure:"
echo "     my-plugin/"
echo "       ├── plugin.toml     (plugin metadata)"
echo "       └── plugin.wasm     (WASM module)"
echo

# Plugin TOML example
echo "9. Example plugin.toml file:"
cat <<EOF
name = "my-plugin"
version = "1.0.0"
description = "A sample MielinCTL plugin"
author = "Your Name"
license = "MIT"
min_version = "0.1.0"

[[commands]]
name = "hello"
description = "Say hello"
aliases = ["hi", "greet"]

[[commands.arguments]]
name = "name"
description = "Name to greet"
required = true
EOF
echo

# Plugin execution example
echo "10. Execute plugin command (example):"
echo "    mielinctl plugin execute my-plugin hello name=World"
echo

# Plugin management
echo "11. Plugin lifecycle management:"
echo "    # Enable a plugin"
echo "    mielinctl plugin enable my-plugin"
echo ""
echo "    # Disable a plugin"
echo "    mielinctl plugin disable my-plugin"
echo ""
echo "    # Uninstall a plugin"
echo "    mielinctl plugin uninstall my-plugin"
echo

# Quiet mode
echo "12. Quiet mode (minimal output):"
mielinctl --quiet plugin list
echo

# Error handling
echo "13. Error handling - trying to get info for non-existent plugin:"
mielinctl plugin info non-existent-plugin 2>&1 || echo "Error handled gracefully"
echo

echo "=== Plugin Operations Examples Complete ==="
