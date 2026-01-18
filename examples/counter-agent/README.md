# Counter Agent

A simple WebAssembly agent that maintains a counter state.

## Overview

This example demonstrates a minimal MielinOS agent implemented in WebAssembly. The agent maintains an atomic counter and exposes functions to manipulate it.

## Exported Functions

- `increment() -> i32`: Increments the counter and returns the new value
- `decrement() -> i32`: Decrements the counter and returns the new value
- `get_value() -> i32`: Returns the current counter value
- `reset()`: Resets the counter to zero

## Building

```bash
# Install wasm32 target if not already installed
rustup target add wasm32-unknown-unknown

# Build the WASM module
cargo build --target wasm32-unknown-unknown --release

# The compiled WASM binary will be at:
# target/wasm32-unknown-unknown/release/counter_agent.wasm
```

## Usage in MielinOS

```rust
use mielin_cells::Agent;
use std::fs;

// Load the WASM binary
let wasm_bytes = fs::read("target/wasm32-unknown-unknown/release/counter_agent.wasm")?;

// Create an agent
let agent = Agent::new(wasm_bytes);

// Deploy to MielinOS
// (integration with mielin-wasm runtime)
```

## Testing

```bash
# Run tests (non-WASM)
cargo test
```

## Features

- **No dependencies**: Pure Rust with no_std
- **Atomic operations**: Thread-safe counter using AtomicI32
- **Small binary**: Minimal WASM output (~150 bytes)
- **State preservation**: Counter state survives agent migration

## Migration Example

When this agent is migrated between nodes, the counter value is preserved in the migration snapshot, demonstrating MielinOS's ability to maintain agent state across the distributed mesh.
