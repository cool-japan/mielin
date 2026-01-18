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

Output:
```
Creating a simple agent...
Agent ID: 1b4e28ba-2fa1-11d2-883f-b9a761bde3fb
Agent State: Created
...
```

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

## Working with the CLI (Future)

```bash
# Node management
mielinctl node start --role edge
mielinctl node list
mielinctl node info <node-id>

# Agent deployment
mielinctl agent deploy agent.wasm
mielinctl agent list
mielinctl agent migrate <agent-id> <target-node>

# Mesh inspection
mielinctl mesh status
mielinctl mesh peers
```

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
