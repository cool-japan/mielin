# MielinOS CLI Examples

This directory contains comprehensive usage examples for all `mielinctl` commands.

## Quick Start

Each file demonstrates a specific category of operations:

1. **[node_operations.sh](./node_operations.sh)** - Node management (create, join, configure)
2. **[agent_operations.sh](./agent_operations.sh)** - Agent lifecycle (create, deploy, exec)
3. **[cluster_operations.sh](./cluster_operations.sh)** - Cluster-wide operations (init, status, upgrade)
4. **[mesh_operations.sh](./mesh_operations.sh)** - Mesh network status and peers
5. **[migration_operations.sh](./migration_operations.sh)** - Agent migration workflows
6. **[registry_operations.sh](./registry_operations.sh)** - Registry queries and statistics
7. **[gossip_operations.sh](./gossip_operations.sh)** - Gossip protocol management
8. **[wasm_operations.sh](./wasm_operations.sh)** - WASM development workflow
9. **[debug_operations.sh](./debug_operations.sh)** - Debugging and profiling
10. **[daemon_operations.sh](./daemon_operations.sh)** - Running MielinOS daemon
11. **[advanced_workflows.sh](./advanced_workflows.sh)** - Complex real-world scenarios

## Usage

Make the scripts executable:

```bash
chmod +x examples/*.sh
```

Run individual examples:

```bash
# View examples (they use echo to show commands)
bash examples/node_operations.sh

# Or execute specific commands from the examples
source examples/agent_operations.sh
```

## Example Categories

### Basic Operations

- **Node Management**: Create, join, and configure nodes
- **Agent Management**: Deploy and manage WASM agents
- **Cluster Operations**: Initialize and manage clusters

### Advanced Operations

- **Migration**: Live and cold agent migration
- **Debugging**: Attach debuggers, trace execution, profile performance
- **WASM Development**: Build, test, and optimize WASM modules

### Production Workflows

- **Blue-Green Deployment**: Zero-downtime deployments
- **Canary Deployment**: Gradual rollouts with monitoring
- **Auto-scaling**: Dynamic cluster scaling
- **Disaster Recovery**: Backup and restore procedures
- **CI/CD Integration**: Automated build and deployment

## Output Formats

All commands support multiple output formats:

```bash
# Table format (default, human-readable)
mielinctl node list

# JSON format (machine-readable)
mielinctl node list --output json

# YAML format
mielinctl node list --output yaml

# Quiet mode (minimal output)
mielinctl node list --quiet
```

## Command Aliases

Most commands have shorter aliases:

```bash
# These are equivalent:
mielinctl node list
mielinctl node ls

# These are equivalent:
mielinctl cluster status
mielinctl cluster st

# These are equivalent:
mielinctl agent inspect agent-001
mielinctl agent show agent-001
```

See individual example files for more aliases.

## Environment Variables

Override configuration with environment variables:

```bash
export MIELIN_NODE_ID="my-node"
export MIELIN_NODE_ROLE="core"
export MIELIN_NODE_LISTEN_ADDRESS="0.0.0.0:7000"
export MIELIN_CLI_DEFAULT_OUTPUT_FORMAT="json"

# These will be used by mielinctl commands
mielinctl daemon
```

Available environment variables:
- `MIELIN_NODE_ID` - Node identifier
- `MIELIN_NODE_ROLE` - Node role (core, relay, edge)
- `MIELIN_NODE_LISTEN_ADDRESS` - Listen address
- `MIELIN_NODE_BOOTSTRAP_NODES` - Bootstrap nodes (comma-separated)
- `MIELIN_CLI_DEFAULT_OUTPUT_FORMAT` - Default output format
- `MIELIN_CLI_ENABLE_COLORS` - Enable colored output
- `MIELIN_CLI_COMMAND_TIMEOUT_SECS` - Command timeout in seconds
- `MIELIN_DAEMON_ENABLE_MDNS` - Enable mDNS discovery
- `MIELIN_DAEMON_ENABLE_GOSSIP` - Enable gossip protocol
- `MIELIN_DAEMON_ENABLE_REGISTRY` - Enable agent registry
- `MIELIN_DAEMON_ENABLE_MIGRATION` - Enable agent migration

## Common Workflows

### 1. Bootstrap a New Cluster

```bash
# Initialize core node
mielinctl cluster init --name my-cluster --listen-addr 0.0.0.0:7000 --role core

# Add relay nodes
mielinctl daemon --listen-addr 0.0.0.0:7001 --role relay --bootstrap tcp://core-ip:7000

# Add edge nodes
mielinctl daemon --listen-addr 0.0.0.0:7002 --role edge --bootstrap tcp://relay-ip:7001
```

### 2. Deploy a WASM Agent

```bash
# Build and optimize
mielinctl wasm build --source ./src --output ./agent.wasm
mielinctl wasm optimize --input ./agent.wasm --output ./agent-opt.wasm

# Deploy
mielinctl agent create --name my-agent --wasm-file ./agent-opt.wasm --memory 256
```

### 3. Monitor Cluster Health

```bash
# Check cluster status
mielinctl cluster health

# View mesh network
mielinctl mesh status

# Check gossip protocol
mielinctl gossip status

# Monitor agents
mielinctl registry stats
```

### 4. Debug an Agent

```bash
# Dump state
mielinctl debug dump agent-001 --memory

# Trace execution
mielinctl debug trace agent-001 --duration 30 --file trace.json

# Profile performance
mielinctl debug profile agent-001 --cpu --mem
```

## Integration Examples

### Systemd Service

```ini
[Unit]
Description=MielinOS Node
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/mielinctl daemon --listen-addr 0.0.0.0:7000 --role core
Restart=always

[Install]
WantedBy=multi-user.target
```

### Docker Deployment

```bash
docker run -p 7000:7000 \
  -e MIELIN_NODE_ROLE=core \
  -e MIELIN_NODE_LISTEN_ADDRESS=0.0.0.0:7000 \
  mielinctl:latest daemon
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mielinctl-node
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: mielinctl
        image: mielinctl:latest
        command: ["mielinctl", "daemon"]
        env:
        - name: MIELIN_NODE_ROLE
          value: "edge"
        - name: MIELIN_NODE_LISTEN_ADDRESS
          value: "0.0.0.0:7000"
```

## Shell Completion

Generate shell completion scripts:

```bash
# Bash
mielinctl completion bash > /etc/bash_completion.d/mielinctl

# Zsh
mielinctl completion zsh > /usr/local/share/zsh/site-functions/_mielinctl

# Fish
mielinctl completion fish > ~/.config/fish/completions/mielinctl.fish
```

## Tips

1. **Use JSON output for scripting**: `--output json` makes it easy to parse with `jq`
2. **Use aliases for quick operations**: `mielinctl node ls` instead of `mielinctl node list`
3. **Set environment variables**: Avoid repeating common flags
4. **Monitor with watch**: `watch -n 5 'mielinctl cluster status'`
5. **Use quiet mode in scripts**: `--quiet` for cleaner output in automation

## Troubleshooting

### Connection Issues

```bash
# Check mesh connectivity
mielinctl mesh status
mielinctl gossip members

# Force gossip sync
mielinctl gossip sync
```

### Performance Issues

```bash
# Profile agents
mielinctl debug profile <agent-id> --cpu --mem

# Check cluster load
mielinctl cluster status
mielinctl registry stats
```

### Migration Issues

```bash
# Check migration status
mielinctl migrate status

# Cancel stuck migration
mielinctl migrate cancel <migration-id>

# View migration history
mielinctl migrate history
```

## Further Reading

- [MielinOS Documentation](../README.md)
- [CLI Reference](../docs/CLI_REFERENCE.md)
- [Architecture Overview](../docs/ARCHITECTURE.md)

## Contributing

Found a useful workflow? Add it to the examples! See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.
