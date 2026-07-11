# MielinOS Quick Start Guide

Get up and running with MielinOS in minutes!

## Running MielinOS

**To run MielinOS in QEMU, see [docs/RUNNING.md](docs/RUNNING.md) for detailed instructions.**

Quick start:
```bash
rustup default nightly
cargo install bootimage
./scripts/run-qemu.sh
```

## Development Setup

### Prerequisites

- Rust 1.83+ (latest stable or nightly for kernel development)
- cargo-nextest (optional but recommended)

## Installation

### Option 1: Automated Setup (Recommended)

```bash
git clone https://github.com/cool-japan/mielin
cd mielin
make setup
```

This will:
- Install required tools (cargo-nextest, cargo-watch)
- Add wasm32-unknown-unknown target
- Build all crates
- Run all tests

### Option 2: Manual Setup

```bash
git clone https://github.com/cool-japan/mielin
cd mielin

# Install tools
cargo install cargo-nextest --locked
rustup target add wasm32-unknown-unknown

# Build
cargo build --workspace

# Test
cargo nextest run --workspace
```

### Option 3: Docker

```bash
git clone https://github.com/cool-japan/mielin
cd mielin

# Build and run development container
docker-compose up -d dev
docker-compose exec dev bash

# Inside container
make test
```

## Your First Agent

### 1. Create a Simple Agent

```rust
use mielin_cells::Agent;

fn main() {
    // WASM binary (minimal valid WASM)
    let wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // Create agent
    let agent = Agent::new(wasm);

    println!("Agent ID: {}", agent.id());
    println!("State: {:?}", agent.state());
}
```

### 2. Run the Hello Agent Example

```bash
cargo run -p hello-agent
```

Output (real, captured from `cargo run -p hello-agent` — your UUID will differ):
```
MielinOS - Hello Agent Example
Created agent with ID: 9250f551-4b65-4174-b324-2ba51b8357e1
Agent state: Created
Agent DNA hash: [8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
```
(the DNA hash is currently a length-derived placeholder — `hash[0]` is the binary length, the rest
zero — not a cryptographic digest; see [`docs/TUTORIALS.md`](docs/TUTORIALS.md) Tutorial 1)

### 3. Run the Migration Demo

```bash
cargo run -p agent-migration
```

This demonstrates a complete 10-phase agent migration from an Edge node to a Core node.

## Common Tasks

### Build Everything

```bash
make build        # Debug build
make release      # Release build (optimized)
```

### Run Tests

```bash
make test         # All tests
cargo nextest run -p mielin-kernel  # Specific crate
```

### Quality Checks

```bash
make check        # Run all checks (fmt, clippy, test)
make fmt          # Format code
make clippy       # Lint code
```

### Build WASM Examples

```bash
make wasm
# Output: target/wasm32-unknown-unknown/release/counter_agent.wasm (229 bytes!)
```

### Run Benchmarks

```bash
make bench
# Results: target/criterion/report/index.html
```

### Generate Documentation

```bash
make doc
# Opens in browser
```

## Project Structure

```
mielin/
├── mielin-kernel/       # Core OS kernel
├── mielin-hal/          # Hardware abstraction
├── mielin-rt/           # Embedded runtime
├── mielin-mesh/         # P2P networking
├── mielin-cells/        # Agent SDK
├── mielin-wasm/         # WASM runtime
├── mielin-tensor/       # AI acceleration
├── mielin-cli/          # CLI tool
├── benches/             # Performance benchmarks
└── examples/            # Example applications
```

## Key Concepts

### Agents
Autonomous programs that can migrate between nodes:
- **DNA**: WASM binary (code)
- **State**: Runtime state (memory)
- **Policy**: Execution constraints (battery, latency, etc.)

### Nodes
Devices running MielinOS:
- **Edge**: IoT devices (Cortex-M, RaspberryPi)
- **Relay**: Intermediate nodes
- **Core**: Cloud servers (AWS, Azure)

### Migration
Moving agents between nodes:
1. **Snapshot**: Capture agent state
2. **Serialize**: Convert to bytes
3. **Transfer**: Send over network
4. **Deserialize**: Restore on target
5. **Resume**: Continue execution

### Capabilities
Fine-grained permissions for agents:
- FileSystem
- Network
- Camera
- GPIO

## Working with the CLI

`mielin-cli` builds a real binary named `mielinctl` (`cargo run -p mielin-cli --`), with dozens of
subcommands already implemented (`node`, `agent`, `mesh`, `cluster`, `migrate`, `registry`,
`gossip`, `wasm`, `debug`, `audit`, `config`, `history`, `monitor`, `plugin`, `script`, `remote`,
`daemon`, `completion`, `version`, `interactive`). Not every subcommand talks to a live system yet
— some print illustrative mock fixtures so you can explore the output shape before a real daemon is
running:

```bash
# Start the real MielinOS daemon (mesh service + HTTP control-plane API)
mielinctl daemon --listen 0.0.0.0:9000 --role edge --control-listen 127.0.0.1:8081

# Node management (all currently illustrative: canned success messages / mock fixtures,
# not yet backed by a live daemon lookup)
mielinctl node create --role edge
mielinctl node list
mielinctl node info <node-id>
mielinctl node join <bootstrap-ip:port>

# Agent deployment (also illustrative today — see Tutorial 11 for current wiring status)
mielinctl agent deploy agent.wasm
mielinctl agent list
mielinctl agent migrate <agent-id> <target-node>

# Mesh inspection (live once a daemon is running and --daemon/MIELIN_DAEMON is set)
mielinctl mesh status --daemon 127.0.0.1:8081
mielinctl mesh peers --daemon 127.0.0.1:8081
mielinctl mesh status    # without --daemon: illustrative mock fixture
```

See [`docs/TUTORIALS.md`](docs/TUTORIALS.md) (Tutorial 11) for exactly which subcommands are wired
to a live daemon today versus which return mock data, and for a full two-node cluster walkthrough.

## Development Workflow

### 1. Make Changes

```bash
# Edit code
vim mielin-cells/src/agent.rs
```

### 2. Format and Lint

```bash
make fmt
make clippy
```

### 3. Test

```bash
make test
```

### 4. Commit

```bash
git add .
git commit -m "Add feature X"
```

### 5. Create PR

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

## Debugging

### Enable Logging

```bash
RUST_LOG=debug cargo run -p agent-migration
```

### Use Debugger

```bash
rust-lldb target/debug/agent-migration
(lldb) b main
(lldb) run
```

### Inspect WASM

```bash
wasm-objdump -x target/wasm32-unknown-unknown/release/counter_agent.wasm
```

## Performance

Current benchmarks (v0.1.0-rc.1):

| Operation | Time |
|-----------|------|
| Page allocation | 50 ns |
| Task spawn | 100 ns |
| Agent creation | 1 μs |
| Migration snapshot | 10 μs |
| DHT peer lookup | 100 μs |

See [benches/README.md](benches/README.md) for details.

## Troubleshooting

### Build Errors

**Problem**: `error: could not compile mielin-kernel`

**Solution**:
```bash
cargo clean
cargo build
```

### Test Failures

**Problem**: Tests failing after changes

**Solution**:
```bash
cargo nextest run -p <crate-name> -- --nocapture
```

### WASM Build Issues

**Problem**: `error: linking with rust-lld failed`

**Solution**:
```bash
rustup target add wasm32-unknown-unknown
cargo clean
```

## Next Steps

- Read [TODO.md](TODO.md) for roadmap
- Check [examples/](examples/) for more examples
- Join [Discussions](https://github.com/cool-japan/mielin/discussions)
- Review [CONTRIBUTING.md](CONTRIBUTING.md) to contribute

## Resources

- **Documentation**: Component READMEs in each crate
- **Whitepaper**: [mielin.md](mielin.md)
- **Architecture**: [README.md](README.md#architecture)
- **Benchmarks**: [benches/README.md](benches/README.md)

## Support

- **Issues**: https://github.com/cool-japan/mielin/issues
- **Discussions**: https://github.com/cool-japan/mielin/discussions
- **Security**: See [SECURITY.md](SECURITY.md)

---

**Welcome to MielinOS!** 🧠⚡

Start building the nervous system for the ASI era.
