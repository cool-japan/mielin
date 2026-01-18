# MielinOS CLI - Command Reference

Complete reference for all `mielinctl` commands.

## Table of Contents

- [Global Flags](#global-flags)
- [Node Commands](#node-commands)
- [Agent Commands](#agent-commands)
- [Cluster Commands](#cluster-commands)
- [Mesh Commands](#mesh-commands)
- [Migration Commands](#migration-commands)
- [Registry Commands](#registry-commands)
- [Gossip Commands](#gossip-commands)
- [WASM Commands](#wasm-commands)
- [Debug Commands](#debug-commands)
- [Daemon Command](#daemon-command)
- [Utility Commands](#utility-commands)

## Global Flags

These flags are available for all commands:

```bash
--output, -o <format>    Output format: table, json, yaml, quiet
--quiet, -q             Minimal output (equivalent to --output quiet)
--help, -h              Show help for command
--version, -V           Show version information
```

## Node Commands

Manage MielinOS nodes in the cluster.

### `node list`

List all nodes in the cluster.

**Aliases:** `ls`

**Usage:**
```bash
mielinctl node list [OPTIONS]
```

**Options:**
- `--output, -o <format>` - Output format (table, json, yaml, quiet)
- `--quiet, -q` - Show only node IDs

**Examples:**
```bash
# List all nodes (table format)
mielinctl node list

# List nodes as JSON
mielinctl node list --output json

# Get only node IDs
mielinctl node ls --quiet
```

### `node info`

Show detailed information about a specific node.

**Aliases:** `show`, `details`

**Usage:**
```bash
mielinctl node info <NODE_ID> [OPTIONS]
```

**Arguments:**
- `<NODE_ID>` - Node identifier

**Options:**
- `--output, -o <format>` - Output format

**Examples:**
```bash
# View node details
mielinctl node info node-001

# Get node info as JSON
mielinctl node show node-001 --output json
```

### `node create`

Create a new node with specified configuration.

**Aliases:** `new`, `add`

**Usage:**
```bash
mielinctl node create [OPTIONS]
```

**Options:**
- `--role <role>` - Node role: core, relay, edge (required)
- `--listen-addr <addr>` - Listen address (e.g., 0.0.0.0:7000)
- `--node-id <id>` - Custom node identifier

**Examples:**
```bash
# Create a core node
mielinctl node create --role core --listen-addr 0.0.0.0:7000

# Create an edge node with custom ID
mielinctl node new --role edge --node-id my-edge-01
```

### `node start`

Start a stopped node.

**Usage:**
```bash
mielinctl node start <NODE_ID>
```

**Examples:**
```bash
mielinctl node start node-001
```

### `node stop`

Stop a running node.

**Aliases:** `kill`, `terminate`

**Usage:**
```bash
mielinctl node stop <NODE_ID> [OPTIONS]
```

**Options:**
- `--force` - Force stop without graceful shutdown

**Examples:**
```bash
# Graceful stop
mielinctl node stop node-001

# Force stop
mielinctl node kill node-001 --force
```

### `node join`

Join an existing cluster.

**Aliases:** `connect`, `add`

**Usage:**
```bash
mielinctl node join [OPTIONS]
```

**Options:**
- `--bootstrap <addr>` - Bootstrap node address (required)

**Examples:**
```bash
mielinctl node join --bootstrap tcp://192.168.1.100:7000
```

### `node leave`

Leave the cluster gracefully.

**Aliases:** `disconnect`, `depart`

**Usage:**
```bash
mielinctl node leave [OPTIONS]
```

**Options:**
- `--drain` - Drain agents before leaving

**Examples:**
```bash
# Leave and migrate agents
mielinctl node leave --drain
```

### `node config`

View or modify node configuration.

**Aliases:** `cfg`, `configure`

**Usage:**
```bash
# View all configuration
mielinctl node config --all

# Get specific value
mielinctl node config <KEY>

# Set value
mielinctl node config <KEY> <VALUE>
```

**Examples:**
```bash
# View all config
mielinctl node config --all

# Get node ID
mielinctl node cfg node.id

# Set node role
mielinctl node config node.role core
```

## Agent Commands

Manage WASM agents running on nodes.

### `agent list`

List all agents in the cluster.

**Aliases:** `ls`

**Usage:**
```bash
mielinctl agent list [OPTIONS]
```

**Options:**
- `--filter <state>` - Filter by state (running, paused, error, migrating)
- `--output, -o <format>` - Output format

**Examples:**
```bash
# List all agents
mielinctl agent list

# List only running agents
mielinctl agent ls --filter running
```

### `agent inspect`

View detailed agent information.

**Aliases:** `show`, `details`

**Usage:**
```bash
mielinctl agent inspect <AGENT_ID> [OPTIONS]
```

**Examples:**
```bash
mielinctl agent inspect agent-001
mielinctl agent show agent-001 --output json
```

### `agent create`

Create a new agent from WASM module.

**Aliases:** `new`, `add`

**Usage:**
```bash
mielinctl agent create [OPTIONS]
```

**Options:**
- `--name <name>` - Agent name (required)
- `--wasm-file <path>` - Path to WASM module (required)
- `--memory <mb>` - Memory limit in MB (default: 256)
- `--cpu-shares <shares>` - CPU shares (default: 1024)
- `--env <KEY=VALUE>` - Environment variables (can be repeated)
- `--node <node-id>` - Target node for deployment

**Examples:**
```bash
# Create basic agent
mielinctl agent create --name my-agent --wasm-file agent.wasm

# Create with resource limits
mielinctl agent new \
  --name my-agent \
  --wasm-file agent.wasm \
  --memory 512 \
  --cpu-shares 2048

# Create with environment variables
mielinctl agent create \
  --name my-agent \
  --wasm-file agent.wasm \
  --env DATABASE_URL=postgres://localhost \
  --env API_KEY=secret123
```

### `agent deploy`

Deploy agent to a specific node.

**Aliases:** `dep`

**Usage:**
```bash
mielinctl agent deploy <AGENT_ID> [OPTIONS]
```

**Options:**
- `--target <node-id>` - Target node (required)

**Examples:**
```bash
mielinctl agent deploy agent-001 --target node-002
```

### `agent migrate`

Migrate agent to another node.

**Aliases:** `move`, `mv`

**Usage:**
```bash
mielinctl agent migrate <AGENT_ID> [OPTIONS]
```

**Options:**
- `--to <node-id>` - Destination node (required)
- `--strategy <strategy>` - Migration strategy: live, cold (default: live)

**Examples:**
```bash
# Live migration
mielinctl agent migrate agent-001 --to node-002

# Cold migration
mielinctl agent move agent-001 --to node-002 --strategy cold
```

### `agent stop`

Stop a running agent.

**Aliases:** `kill`, `terminate`

**Usage:**
```bash
mielinctl agent stop <AGENT_ID>
```

**Examples:**
```bash
mielinctl agent stop agent-001
```

### `agent logs`

View agent logs.

**Aliases:** `log`

**Usage:**
```bash
mielinctl agent logs <AGENT_ID> [OPTIONS]
```

**Options:**
- `--tail <lines>` - Number of lines to show (default: 100)
- `--follow, -f` - Follow log output

**Examples:**
```bash
# View last 100 lines
mielinctl agent logs agent-001

# View last 500 lines
mielinctl agent log agent-001 --tail 500

# Follow logs in real-time
mielinctl agent logs agent-001 --follow
```

### `agent exec`

Execute command inside agent.

**Aliases:** `run`, `execute`

**Usage:**
```bash
mielinctl agent exec [OPTIONS] <AGENT_ID> <COMMAND> [ARGS...]
```

**Options:**
- `-i, --interactive` - Keep STDIN open
- `-t, --tty` - Allocate a pseudo-TTY

**Examples:**
```bash
# Execute single command
mielinctl agent exec agent-001 echo "Hello"

# Interactive shell
mielinctl agent exec -it agent-001 /bin/sh

# Run script
mielinctl agent run agent-001 python script.py arg1 arg2
```

## Cluster Commands

Cluster-wide operations and management.

### `cluster init`

Initialize a new cluster.

**Aliases:** `initialize`, `setup`

**Usage:**
```bash
mielinctl cluster init [OPTIONS]
```

**Options:**
- `--name <name>` - Cluster name (required)
- `--listen-addr <addr>` - Listen address (required)
- `--role <role>` - Node role: core, relay, edge (default: core)

**Examples:**
```bash
mielinctl cluster init \
  --name production \
  --listen-addr 0.0.0.0:7000 \
  --role core
```

### `cluster status`

View cluster-wide status.

**Aliases:** `st`, `stat`

**Usage:**
```bash
mielinctl cluster status [OPTIONS]
```

**Examples:**
```bash
mielinctl cluster status
mielinctl cluster st --output json
```

### `cluster health`

Perform health check on all nodes.

**Aliases:** `check`, `healthcheck`

**Usage:**
```bash
mielinctl cluster health [OPTIONS]
```

**Examples:**
```bash
mielinctl cluster health
mielinctl cluster check --output json
```

### `cluster upgrade`

Perform rolling cluster upgrade.

**Aliases:** `up`, `update`

**Usage:**
```bash
mielinctl cluster upgrade [OPTIONS]
```

**Options:**
- `--version <version>` - Target version (required)
- `--batch-size <size>` - Nodes per batch (default: 1)
- `--wait-time <seconds>` - Wait between batches (default: 30)
- `--skip-health-check` - Skip health checks (not recommended)

**Examples:**
```bash
# Safe upgrade (one node at a time)
mielinctl cluster upgrade --version 0.2.0

# Faster upgrade (2 nodes at a time)
mielinctl cluster up --version 0.2.0 --batch-size 2 --wait-time 60
```

## Mesh Commands

Mesh network status and peer management.

### `mesh status`

View mesh network status.

**Aliases:** `st`, `stat`

**Usage:**
```bash
mielinctl mesh status [OPTIONS]
```

**Examples:**
```bash
mielinctl mesh status
mielinctl mesh st --output json
```

### `mesh peers`

List connected peers.

**Aliases:** `ls`, `list`

**Usage:**
```bash
mielinctl mesh peers [OPTIONS]
```

**Examples:**
```bash
mielinctl mesh peers
mielinctl mesh ls --output yaml
```

## Migration Commands

Agent migration operations and history.

### `migrate agent`

Initiate agent migration (alias for `agent migrate`).

**Usage:**
```bash
mielinctl migrate agent <AGENT_ID> [OPTIONS]
```

**Options:**
- `--to <node-id>` - Destination node (required)
- `--strategy <strategy>` - Migration strategy

**Examples:**
```bash
mielinctl migrate agent agent-001 --to node-002
```

### `migrate status`

View migration status.

**Aliases:** `st`, `stat`

**Usage:**
```bash
mielinctl migrate status [OPTIONS]
```

**Options:**
- `--agent <agent-id>` - Filter by agent

**Examples:**
```bash
# All migrations
mielinctl migrate status

# Specific agent
mielinctl migrate st --agent agent-001
```

### `migrate cancel`

Cancel a pending migration.

**Aliases:** `abort`, `stop`

**Usage:**
```bash
mielinctl migrate cancel <MIGRATION_ID>
```

**Examples:**
```bash
mielinctl migrate cancel migration-123
```

### `migrate history`

View migration history.

**Aliases:** `hist`, `log`

**Usage:**
```bash
mielinctl migrate history [OPTIONS]
```

**Options:**
- `--limit <n>` - Number of entries (default: 10)

**Examples:**
```bash
# Last 10 migrations
mielinctl migrate history

# Last 50 migrations
mielinctl migrate hist --limit 50
```

## Registry Commands

Agent registry queries and statistics.

### `registry list`

List all agents in registry.

**Aliases:** `ls`

**Usage:**
```bash
mielinctl registry list [OPTIONS]
```

**Examples:**
```bash
mielinctl registry list
mielinctl registry ls --output json
```

### `registry query`

Query agents by pattern.

**Aliases:** `search`, `find`

**Usage:**
```bash
mielinctl registry query <PATTERN> [OPTIONS]
```

**Examples:**
```bash
# Find by name pattern
mielinctl registry query "web-*"

# Find by status
mielinctl registry search "status:running"
```

### `registry stats`

View registry statistics.

**Aliases:** `statistics`, `info`

**Usage:**
```bash
mielinctl registry stats [OPTIONS]
```

**Examples:**
```bash
mielinctl registry stats
mielinctl registry info --output json
```

## Gossip Commands

Gossip protocol management.

### `gossip status`

View gossip protocol status.

**Aliases:** `st`, `stat`

**Usage:**
```bash
mielinctl gossip status [OPTIONS]
```

**Examples:**
```bash
mielinctl gossip status
mielinctl gossip st --output json
```

### `gossip members`

List cluster members.

**Aliases:** `ls`, `list`

**Usage:**
```bash
mielinctl gossip members [OPTIONS]
```

**Options:**
- `--alive` - Show only alive members

**Examples:**
```bash
# All members
mielinctl gossip members

# Only alive members
mielinctl gossip ls --alive
```

### `gossip sync`

Force gossip synchronization.

**Aliases:** `synchronize`, `refresh`

**Usage:**
```bash
mielinctl gossip sync [PEER_ID]
```

**Examples:**
```bash
# Sync with all peers
mielinctl gossip sync

# Sync with specific peer
mielinctl gossip synchronize peer-abc123
```

## WASM Commands

WASM agent development tools.

### `wasm build`

Build WASM agent from source.

**Aliases:** `compile`, `make`

**Usage:**
```bash
mielinctl wasm build [OPTIONS]
```

**Options:**
- `--source <path>` - Source directory (required)
- `--output <path>` - Output WASM file (required)
- `--optimize` - Optimize during build

**Examples:**
```bash
# Basic build
mielinctl wasm build --source ./src --output agent.wasm

# Build with optimization
mielinctl wasm compile --source ./src --output agent.wasm --optimize
```

### `wasm validate`

Validate WASM module.

**Aliases:** `check`, `verify`

**Usage:**
```bash
mielinctl wasm validate <WASM_FILE>
```

**Examples:**
```bash
mielinctl wasm validate agent.wasm
mielinctl wasm check agent.wasm
```

### `wasm test`

Test WASM agent locally.

**Aliases:** `run`

**Usage:**
```bash
mielinctl wasm test <WASM_FILE> [OPTIONS]
```

**Options:**
- `--args <args>` - Arguments to pass

**Examples:**
```bash
mielinctl wasm test agent.wasm
mielinctl wasm run agent.wasm --args "arg1 arg2"
```

### `wasm optimize`

Optimize WASM module.

**Aliases:** `opt`, `shrink`

**Usage:**
```bash
mielinctl wasm optimize [OPTIONS]
```

**Options:**
- `--input <path>` - Input WASM file (required)
- `--output <path>` - Output WASM file (required)
- `--level <level>` - Optimization level 0-3 (default: 2)

**Examples:**
```bash
mielinctl wasm optimize \
  --input agent.wasm \
  --output agent-opt.wasm \
  --level 3
```

## Debug Commands

Debugging and profiling tools.

### `debug attach`

Attach debugger to agent.

**Aliases:** `connect`

**Usage:**
```bash
mielinctl debug attach <AGENT_ID> [OPTIONS]
```

**Options:**
- `--port <port>` - Debugger port (default: 9229)

**Examples:**
```bash
mielinctl debug attach agent-001
mielinctl debug connect agent-001 --port 9230
```

### `debug trace`

Trace agent execution.

**Aliases:** `track`, `follow`

**Usage:**
```bash
mielinctl debug trace <AGENT_ID> [OPTIONS]
```

**Options:**
- `--duration <seconds>` - Trace duration (default: 30)
- `--file <path>` - Output file

**Examples:**
```bash
# Trace for 30 seconds
mielinctl debug trace agent-001

# Trace for 60 seconds and save
mielinctl debug track agent-001 --duration 60 --file trace.json
```

### `debug dump`

Dump agent state.

**Aliases:** `snapshot`, `export`

**Usage:**
```bash
mielinctl debug dump <AGENT_ID> [OPTIONS]
```

**Options:**
- `--memory` - Include memory dump

**Examples:**
```bash
mielinctl debug dump agent-001
mielinctl debug snapshot agent-001 --memory
```

### `debug profile`

Show profiling data.

**Aliases:** `prof`, `perf`

**Usage:**
```bash
mielinctl debug profile <AGENT_ID> [OPTIONS]
```

**Options:**
- `--cpu` - Show CPU profiling
- `--mem` - Show memory profiling

**Examples:**
```bash
# All profiling data
mielinctl debug profile agent-001

# CPU only
mielinctl debug prof agent-001 --cpu

# Both CPU and memory
mielinctl debug perf agent-001 --cpu --mem
```

## Daemon Command

Run MielinOS node in daemon mode.

### `daemon`

Start MielinOS node daemon.

**Usage:**
```bash
mielinctl daemon [OPTIONS]
```

**Options:**
- `--listen-addr <addr>` - Listen address (default: 0.0.0.0:7000)
- `--role <role>` - Node role: core, relay, edge (default: core)
- `--node-id <id>` - Custom node ID
- `--bootstrap <addr>` - Bootstrap nodes (comma-separated)

**Examples:**
```bash
# Start core node
mielinctl daemon --role core

# Start edge node and join cluster
mielinctl daemon \
  --role edge \
  --bootstrap tcp://192.168.1.100:7000,tcp://192.168.1.101:7001
```

## Utility Commands

Miscellaneous utility commands.

### `version`

Show version information.

**Usage:**
```bash
mielinctl version [OPTIONS]
```

**Examples:**
```bash
mielinctl version
mielinctl version --output json
```

### `completion`

Generate shell completion scripts.

**Usage:**
```bash
mielinctl completion <SHELL>
```

**Arguments:**
- `<SHELL>` - Shell type: bash, zsh, fish, powershell, elvish

**Examples:**
```bash
# Bash
mielinctl completion bash > /etc/bash_completion.d/mielinctl

# Zsh
mielinctl completion zsh > /usr/local/share/zsh/site-functions/_mielinctl

# Fish
mielinctl completion fish > ~/.config/fish/completions/mielinctl.fish
```

## Environment Variables

Configure `mielinctl` via environment variables:

| Variable | Description | Example |
|----------|-------------|---------|
| `MIELIN_NODE_ID` | Node identifier | `my-node-01` |
| `MIELIN_NODE_ROLE` | Node role | `core`, `relay`, `edge` |
| `MIELIN_NODE_LISTEN_ADDRESS` | Listen address | `0.0.0.0:7000` |
| `MIELIN_NODE_BOOTSTRAP_NODES` | Bootstrap nodes | `tcp://host1:7000,tcp://host2:7000` |
| `MIELIN_CLI_DEFAULT_OUTPUT_FORMAT` | Default output format | `json`, `yaml`, `table` |
| `MIELIN_CLI_ENABLE_COLORS` | Enable colored output | `true`, `false` |
| `MIELIN_CLI_COMMAND_TIMEOUT_SECS` | Command timeout | `30`, `60`, `120` |
| `MIELIN_DAEMON_ENABLE_MDNS` | Enable mDNS | `true`, `false` |
| `MIELIN_DAEMON_ENABLE_GOSSIP` | Enable gossip | `true`, `false` |
| `MIELIN_DAEMON_ENABLE_REGISTRY` | Enable registry | `true`, `false` |
| `MIELIN_DAEMON_ENABLE_MIGRATION` | Enable migration | `true`, `false` |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid command or arguments |
| 3 | Connection error |
| 4 | Timeout |
| 5 | Not found |
| 6 | Permission denied |

## See Also

- [Quickstart Guide](./QUICKSTART.md)
- [Common Workflows](./WORKFLOWS.md)
- [Troubleshooting](./TROUBLESHOOTING.md)
- [Examples](../examples/)
