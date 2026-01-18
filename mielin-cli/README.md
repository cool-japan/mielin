# mielin-cli

**Command Line Interface - mielinctl**

Control and management tool for MielinOS clusters and agents.

## Features

- **Node Management**: Start, stop, and monitor nodes
- **Agent Operations**: Deploy, migrate, and terminate agents
- **Mesh Inspection**: View network topology and status
- **Interactive & Scripting**: Both CLI and programmatic usage

## Installation

Build from source:

```bash
cargo build --release -p mielin-cli
```

The binary will be at `target/release/mielinctl`.

## Usage

```bash
mielinctl <COMMAND>
```

### Available Commands

#### Node Management

```bash
# List all nodes in the mesh
mielinctl node list

# Show detailed info about a node
mielinctl node info <NODE_ID>

# Start a node
mielinctl node start

# Stop a node
mielinctl node stop
```

#### Agent Management

```bash
# List running agents
mielinctl agent list

# Deploy a new agent
mielinctl agent deploy <WASM_PATH>

# Migrate an agent to another node
mielinctl agent migrate <AGENT_ID> <TARGET_NODE>

# Stop an agent
mielinctl agent stop <AGENT_ID>
```

#### Mesh Operations

```bash
# Show mesh network status
mielinctl mesh status

# List connected peers
mielinctl mesh peers
```

## Examples

### Deploy an Agent

```bash
# Compile your agent to WASM
rustc --target wasm32-unknown-unknown myagent.rs

# Deploy to the mesh
mielinctl agent deploy myagent.wasm
```

### Migrate Agent

```bash
# Get agent ID
AGENT_ID=$(mielinctl agent list | grep myagent | cut -d' ' -f1)

# Find target node
TARGET=$(mielinctl mesh peers | head -1 | cut -d' ' -f1)

# Perform migration
mielinctl agent migrate $AGENT_ID $TARGET
```

### Monitor Mesh

```bash
# Watch mesh status
watch -n 1 mielinctl mesh status

# List all peers with details
mielinctl mesh peers --verbose
```

## Architecture

The CLI is built with:
- **clap**: Command-line argument parsing
- **tokio**: Async runtime
- **tracing**: Structured logging

```rust
use mielin_cli::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Node { action } => handle_node_command(action).await,
        Commands::Agent { action } => handle_agent_command(action).await,
        Commands::Mesh { action } => handle_mesh_command(action).await,
    }
}
```

## Command Reference

### `node` Commands

| Command | Description | Example |
|---------|-------------|---------|
| `list` | List all nodes | `mielinctl node list` |
| `info <ID>` | Show node details | `mielinctl node info abc123` |
| `start` | Start local node | `mielinctl node start` |
| `stop` | Stop local node | `mielinctl node stop` |

### `agent` Commands

| Command | Description | Example |
|---------|-------------|---------|
| `list` | List agents | `mielinctl agent list` |
| `deploy <PATH>` | Deploy WASM | `mielinctl agent deploy app.wasm` |
| `migrate <ID> <NODE>` | Migrate agent | `mielinctl agent migrate id123 node456` |
| `stop <ID>` | Stop agent | `mielinctl agent stop id123` |

### `mesh` Commands

| Command | Description | Example |
|---------|-------------|---------|
| `status` | Show mesh status | `mielinctl mesh status` |
| `peers` | List peers | `mielinctl mesh peers` |

## Configuration

Future: Configuration file support:

```toml
# ~/.mielin/config.toml
[node]
id = "my-node-id"
role = "relay"

[mesh]
bootstrap_nodes = [
    "node1.example.com:7070",
    "node2.example.com:7070"
]

[agents]
max_concurrent = 10
default_policy = { min_battery = 20, max_latency = 100 }
```

## Output Formats

Future: Multiple output formats:

```bash
# JSON output
mielinctl agent list --output json

# YAML output
mielinctl node info <ID> --output yaml

# Table output (default)
mielinctl mesh peers
```

## Scripting

Use in scripts:

```bash
#!/bin/bash

# Deploy multiple agents
for agent in agents/*.wasm; do
    mielinctl agent deploy "$agent"
done

# Check mesh health
if mielinctl mesh status | grep -q "Healthy"; then
    echo "Mesh is healthy"
else
    echo "Mesh has issues"
    exit 1
fi
```

## Development

The CLI is implemented as subcommands:

```rust
#[derive(Subcommand)]
enum Commands {
    Node {
        #[command(subcommand)]
        action: NodeCommands,
    },
    Agent {
        #[command(subcommand)]
        action: AgentCommands,
    },
    Mesh {
        #[command(subcommand)]
        action: MeshCommands,
    },
}
```

## Future Enhancements

- [ ] Interactive TUI mode
- [ ] Configuration file support
- [ ] Multiple output formats (JSON, YAML)
- [ ] Shell completion
- [ ] Agent logs streaming
- [ ] Real-time metrics display
- [ ] Cluster orchestration
- [ ] Backup/restore operations

## API Integration

Currently, the CLI provides stubs. Future integration:

```rust
async fn handle_agent_deploy(path: String) -> anyhow::Result<()> {
    let wasm = std::fs::read(&path)?;
    let agent = Agent::new(wasm);

    let client = MielinClient::connect("localhost:7070").await?;
    let agent_id = client.deploy_agent(agent).await?;

    println!("Deployed agent: {}", agent_id);
    Ok(())
}
```

## Testing

```bash
cargo test -p mielin-cli
```

## Logging

Enable detailed logging:

```bash
RUST_LOG=debug mielinctl agent deploy myagent.wasm
```

## License

MIT OR Apache-2.0
