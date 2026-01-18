# MielinOS CLI - Quickstart Guide

Get started with `mielinctl`, the command-line interface for MielinOS.

## Table of Contents

- [Installation](#installation)
- [First Steps](#first-steps)
- [Basic Operations](#basic-operations)
- [Next Steps](#next-steps)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/mielinos/mielin
cd mielin/mielin-cli

# Build and install
cargo install --path .

# Verify installation
mielinctl --version
```

### From crates.io

```bash
cargo install mielin-cli
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/mielinos/mielin/releases):

**Linux:**
```bash
wget https://github.com/mielinos/mielin/releases/latest/download/mielinctl-linux-amd64.tar.gz
tar xzf mielinctl-linux-amd64.tar.gz
sudo mv mielinctl /usr/local/bin/
```

**macOS:**
```bash
wget https://github.com/mielinos/mielin/releases/latest/download/mielinctl-macos-amd64.tar.gz
tar xzf mielinctl-macos-amd64.tar.gz
sudo mv mielinctl /usr/local/bin/
```

**Windows (PowerShell):**
```powershell
Invoke-WebRequest -Uri "https://github.com/mielinos/mielin/releases/latest/download/mielinctl-windows-amd64.zip" -OutFile "mielinctl.zip"
Expand-Archive -Path mielinctl.zip -DestinationPath .
Move-Item mielinctl.exe C:\Windows\System32\
```

## First Steps

### 1. View Help

```bash
# General help
mielinctl --help

# Command-specific help
mielinctl node --help
mielinctl agent --help
```

### 2. Set Up Shell Completion

**Bash:**
```bash
mielinctl completion bash > /etc/bash_completion.d/mielinctl
source ~/.bashrc
```

**Zsh:**
```bash
mielinctl completion zsh > /usr/local/share/zsh/site-functions/_mielinctl
```

**Fish:**
```bash
mielinctl completion fish > ~/.config/fish/completions/mielinctl.fish
```

### 3. Configure Your Environment

View default configuration:
```bash
mielinctl node config --all
```

Set your node ID and role:
```bash
mielinctl node config node.id my-node-01
mielinctl node config node.role core
```

Or use environment variables:
```bash
export MIELIN_NODE_ID="my-node-01"
export MIELIN_NODE_ROLE="core"
export MIELIN_NODE_LISTEN_ADDRESS="0.0.0.0:7000"
```

## Basic Operations

### Starting a MielinOS Node

#### Option 1: Daemon Mode (Recommended)

```bash
# Start a core node
mielinctl daemon --listen-addr 0.0.0.0:7000 --role core

# Start an edge node and join existing cluster
mielinctl daemon \
  --listen-addr 0.0.0.0:7001 \
  --role edge \
  --bootstrap tcp://192.168.1.100:7000
```

Press `Ctrl+C` to gracefully shut down.

#### Option 2: Cluster Init

```bash
# Initialize a new cluster
mielinctl cluster init \
  --name my-cluster \
  --listen-addr 0.0.0.0:7000 \
  --role core
```

### Managing Nodes

```bash
# List all nodes
mielinctl node list

# View node details
mielinctl node info <node-id>

# Create a new node
mielinctl node create --role edge --listen-addr 0.0.0.0:7002

# Join a cluster
mielinctl node join --bootstrap tcp://192.168.1.100:7000
```

### Working with Agents

#### Create and Deploy

```bash
# Create an agent from WASM module
mielinctl agent create \
  --name my-agent \
  --wasm-file ./agent.wasm \
  --memory 256 \
  --cpu-shares 1024

# List all agents
mielinctl agent list

# View agent details
mielinctl agent inspect <agent-id>
```

#### Execute Commands

```bash
# Execute a command in an agent
mielinctl agent exec <agent-id> echo "Hello from agent"

# Interactive shell
mielinctl agent exec -it <agent-id> /bin/sh
```

#### View Logs

```bash
# View recent logs
mielinctl agent logs <agent-id> --tail 100

# Follow logs in real-time
mielinctl agent logs <agent-id> --follow
```

### Cluster Monitoring

```bash
# View cluster status
mielinctl cluster status

# Check cluster health
mielinctl cluster health

# View mesh network status
mielinctl mesh status

# List connected peers
mielinctl mesh peers
```

### WASM Development

```bash
# Build WASM agent
mielinctl wasm build --source ./src --output ./agent.wasm

# Validate WASM module
mielinctl wasm validate ./agent.wasm

# Test locally
mielinctl wasm test ./agent.wasm

# Optimize for production
mielinctl wasm optimize \
  --input ./agent.wasm \
  --output ./agent-optimized.wasm
```

## Common Patterns

### Output Formats

```bash
# Human-readable table (default)
mielinctl node list

# JSON for scripting
mielinctl node list --output json

# YAML for configuration
mielinctl node list --output yaml

# Quiet mode (IDs only)
mielinctl node list --quiet
```

### Using Aliases

Most commands have shorter aliases:

```bash
# These are equivalent
mielinctl node list
mielinctl node ls

# These are equivalent
mielinctl cluster status
mielinctl cluster st

# These are equivalent
mielinctl agent inspect <id>
mielinctl agent show <id>
```

### Scripting

Extract specific fields with `jq`:

```bash
# Get all node IDs
mielinctl node list --output json | jq -r '.[].id'

# Find nodes with specific role
mielinctl node list --output json | jq -r '.[] | select(.role=="core") | .id'

# Count running agents
mielinctl agent list --output json | jq '[.[] | select(.state=="Running")] | length'
```

## Example Workflows

### 1. Bootstrap a 3-Node Cluster

```bash
# Terminal 1: Start core node
mielinctl daemon --listen-addr 0.0.0.0:7000 --role core

# Terminal 2: Start relay node
mielinctl daemon \
  --listen-addr 0.0.0.0:7001 \
  --role relay \
  --bootstrap tcp://localhost:7000

# Terminal 3: Start edge node
mielinctl daemon \
  --listen-addr 0.0.0.0:7002 \
  --role edge \
  --bootstrap tcp://localhost:7001

# Terminal 4: Verify cluster
mielinctl cluster status
mielinctl node list
```

### 2. Deploy an Agent

```bash
# Build WASM module
cd my-agent
mielinctl wasm build --source . --output ../agent.wasm

# Validate and optimize
cd ..
mielinctl wasm validate agent.wasm
mielinctl wasm optimize --input agent.wasm --output agent-opt.wasm

# Deploy to cluster
mielinctl agent create \
  --name my-agent \
  --wasm-file agent-opt.wasm \
  --memory 512 \
  --env KEY1=value1 \
  --env KEY2=value2

# Verify deployment
mielinctl agent list
mielinctl agent inspect <agent-id>
```

### 3. Monitor Cluster Health

```bash
# Check overall health
mielinctl cluster health

# View mesh connectivity
mielinctl mesh status
mielinctl gossip members --alive

# View registry statistics
mielinctl registry stats

# Monitor specific agent
watch -n 5 'mielinctl agent inspect <agent-id>'
```

## Next Steps

### Learn More

- **[Command Reference](./COMMAND_REFERENCE.md)** - Complete command documentation
- **[Common Workflows](./WORKFLOWS.md)** - Detailed workflow examples
- **[Troubleshooting Guide](./TROUBLESHOOTING.md)** - Solve common issues
- **[Examples](../examples/)** - Runnable script examples

### Advanced Topics

- **Configuration Management** - Custom config files and environment variables
- **Agent Migration** - Live migration between nodes
- **Debugging** - Attach debuggers and trace execution
- **Performance** - Profiling and optimization
- **Production Deployment** - Systemd, Docker, Kubernetes

### Get Help

- **GitHub Issues**: https://github.com/mielinos/mielin/issues
- **Documentation**: https://docs.mielinos.org
- **Community Chat**: https://discord.gg/mielinos

## Tips & Tricks

### 1. Use Tab Completion

After installing shell completion, press `Tab` to autocomplete:

```bash
mielinctl node <TAB>         # Shows: list, info, create, join, etc.
mielinctl agent ls --<TAB>   # Shows: --filter, --output, --quiet
```

### 2. Set Default Output Format

```bash
# Always use JSON
export MIELIN_CLI_DEFAULT_OUTPUT_FORMAT="json"

# Now all commands default to JSON
mielinctl node list  # Returns JSON
```

### 3. Increase Command Timeout

```bash
# For slow networks or large operations
export MIELIN_CLI_COMMAND_TIMEOUT_SECS=120

# Or in config
mielinctl node config cli.command_timeout_secs 120
```

### 4. Monitor in Real-Time

```bash
# Watch cluster status
watch -n 2 'mielinctl cluster status'

# Follow agent logs
mielinctl agent logs <agent-id> --follow

# Stream events
mielinctl events --follow
```

### 5. Batch Operations

```bash
# Stop all agents on a node
for agent in $(mielinctl agent list --output json | jq -r '.[] | select(.node=="node-001") | .id'); do
  mielinctl agent stop $agent
done

# Health check all nodes
mielinctl node list --output json | jq -r '.[].id' | while read node; do
  echo "Checking $node..."
  mielinctl node info $node
done
```

## Quick Reference Card

### Most Common Commands

```bash
# Node Operations
mielinctl node ls                    # List nodes
mielinctl node info <id>             # Node details
mielinctl daemon --role core         # Start daemon

# Agent Operations
mielinctl agent ls                   # List agents
mielinctl agent create ...           # Create agent
mielinctl agent logs <id> --follow   # View logs
mielinctl agent exec -it <id> sh     # Interactive shell

# Cluster Operations
mielinctl cluster status             # Cluster status
mielinctl cluster health             # Health check
mielinctl mesh status                # Mesh network

# WASM Operations
mielinctl wasm build ...             # Build WASM
mielinctl wasm validate <file>       # Validate
mielinctl wasm test <file>           # Test locally

# Debugging
mielinctl debug attach <id>          # Attach debugger
mielinctl debug profile <id>         # Show profile
```

### Key Flags

```bash
--output, -o      # json, yaml, table, quiet
--quiet, -q       # Minimal output
--help, -h        # Show help
--version, -V     # Show version
```

---

**Ready to get started?** Try deploying your first agent!

```bash
# 1. Start a node
mielinctl daemon --role core &

# 2. Create a simple agent
echo 'fn main() { println!("Hello, MielinOS!"); }' > hello.rs
mielinctl wasm build --source . --output hello.wasm

# 3. Deploy it
mielinctl agent create --name hello --wasm-file hello.wasm

# 4. Check it's running
mielinctl agent list
```

For more help, see the [full documentation](https://docs.mielinos.org) or run `mielinctl --help`.
