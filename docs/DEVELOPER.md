# MielinOS Developer Guide

## Table of Contents

1. [Development Environment Setup](#development-environment-setup)
2. [Build System Overview](#build-system-overview)
3. [Testing Strategy](#testing-strategy)
4. [CI/CD Pipeline](#cicd-pipeline)
5. [Code Organization](#code-organization)
6. [Contributing Guidelines](#contributing-guidelines)
7. [Release Process](#release-process)
8. [Developer Tools](#developer-tools)

---

## Development Environment Setup

### Prerequisites

#### Required Tools

| Tool | Minimum Version | Purpose |
|------|----------------|---------|
| Rust | 1.83+ | Primary language |
| cargo | 1.83+ | Build system |
| cargo-nextest | Latest | Fast test runner |
| git | 2.0+ | Version control |

#### Optional Tools

| Tool | Purpose |
|------|---------|
| QEMU | Kernel testing |
| Docker | Containerization |
| docker-compose | Multi-node testing |
| flamegraph | Performance profiling |
| valgrind | Memory profiling |
| perf | Linux performance analysis |

### Installation

#### 1. Install Rust

```bash
# Install rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Update to latest stable
rustup update stable

# Verify installation
rustc --version  # Should be 1.83 or later
cargo --version
```

#### 2. Install Development Tools

```bash
# Install cargo-nextest (fast test runner)
cargo install cargo-nextest

# Install flamegraph (profiling)
cargo install flamegraph

# Install cargo-watch (auto-rebuild)
cargo install cargo-watch

# Install cargo-edit (dependency management)
cargo install cargo-edit

# Install wasm tools
cargo install wasm-pack
rustup target add wasm32-unknown-unknown
rustup target add wasm32-wasi
```

#### 3. Clone Repository

```bash
# Clone the repository
git clone https://github.com/cool-japan/mielin
cd mielin

# Install dependencies
make setup
```

#### 4. Verify Setup

```bash
# Build all crates
cargo build --all

# Run tests
cargo nextest run

# Should see output:
# test result: ok. 117 passed; 0 failed
```

### IDE Setup

#### Visual Studio Code

**Recommended Extensions**:

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "vadimcn.vscode-lldb",
    "serayuzgur.crates",
    "tamasfe.even-better-toml"
  ]
}
```

**Settings** (`.vscode/settings.json`):

```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.checkOnSave.allTargets": true,
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

#### IntelliJ IDEA / CLion

1. Install Rust plugin
2. Import project as Cargo project
3. Enable external linter (clippy)
4. Configure formatter (rustfmt)

---

## Build System Overview

### Workspace Structure

MielinOS uses a Cargo workspace with multiple crates:

```
mielin/
├── Cargo.toml              # Workspace root
├── mielin-kernel/          # Core unikernel
├── mielin-hal/             # Hardware abstraction
├── mielin-rt/              # Embedded runtime
├── mielin-mesh/            # Networking
│   ├── core/               # DHT and routing
│   └── wire/               # Protocol
├── mielin-cells/           # Agent SDK
├── mielin-wasm/            # WASM runtime
├── mielin-tensor/          # Tensor operations
├── mielin-cli/             # CLI tool
├── benches/                # Benchmarks
└── examples/               # Examples
```

### Build Commands

#### Basic Build

```bash
# Build all crates (debug mode)
cargo build

# Build specific crate
cargo build -p mielin-kernel

# Build with all features
cargo build --all-features

# Build release (optimized)
cargo build --release
```

#### Build Profiles

MielinOS defines multiple build profiles:

```toml
# Development (default)
[profile.dev]
opt-level = 0
debug = true
panic = "abort"

# Release (size-optimized)
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true

# Release (speed-optimized)
[profile.release-speed]
inherits = "release"
opt-level = 3
lto = "thin"

# Release with debug info
[profile.release-debug]
inherits = "release"
debug = true
strip = false
```

**Usage**:

```bash
# Build for size
cargo build --release

# Build for speed
cargo build --profile release-speed

# Build with debug info
cargo build --profile release-debug
```

### Cross-Compilation

#### AArch64 (ARM 64-bit)

```bash
# Add target
rustup target add aarch64-unknown-linux-gnu

# Install cross-compiler (Ubuntu/Debian)
sudo apt-get install gcc-aarch64-linux-gnu

# Build
cargo build --target aarch64-unknown-linux-gnu
```

#### RISC-V 64-bit

```bash
# Add target
rustup target add riscv64gc-unknown-linux-gnu

# Build
cargo build --target riscv64gc-unknown-linux-gnu
```

#### x86_64 Bare Metal

```bash
# Add target
rustup target add x86_64-unknown-none

# Build kernel
cargo build -p mielin-kernel --target x86_64-unknown-none --features bootable
```

#### WebAssembly

```bash
# Build agent as WASM
cargo build --target wasm32-wasi --release

# Optimize WASM
wasm-opt -Oz target/wasm32-wasi/release/agent.wasm -o agent.wasm
```

### Makefile Targets

The project includes a comprehensive Makefile:

```bash
# Show all available targets
make help

# Setup development environment
make setup

# Build all crates
make build

# Run all tests
make test

# Run benchmarks
make bench

# Build WASM examples
make wasm

# Format code
make fmt

# Run linter
make clippy

# Generate documentation
make doc

# Clean build artifacts
make clean
```

---

## Testing Strategy

### Test Categories

MielinOS uses multiple levels of testing:

1. **Unit Tests**: Test individual functions and modules
2. **Integration Tests**: Test interactions between components
3. **Benchmark Tests**: Measure performance
4. **Example Tests**: Verify examples work correctly

### Running Tests

#### All Tests

```bash
# Using cargo-nextest (recommended)
cargo nextest run

# Using standard cargo test
cargo test
```

#### Specific Tests

```bash
# Test specific crate
cargo nextest run -p mielin-kernel

# Test specific module
cargo nextest run -p mielin-kernel memory

# Test with pattern
cargo nextest run allocation

# Test with logging
RUST_LOG=debug cargo nextest run
```

#### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage

# Open report
open coverage/index.html
```

### Writing Tests

#### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_allocation() {
        let result = allocate_pages(10);
        assert!(result.is_ok());

        let address = result.unwrap();
        assert!(address > 0);

        // Cleanup
        deallocate_pages(address, 10).unwrap();
    }

    #[test]
    #[should_panic(expected = "out of memory")]
    fn test_allocation_failure() {
        allocate_pages(10000).unwrap();
    }
}
```

#### Integration Test Example

```rust
// tests/integration_test.rs
use mielin_cells::{Agent, migration::MigrationSnapshot};

#[tokio::test]
async fn test_agent_migration() {
    // Setup
    let wasm = vec![0x00, 0x61, 0x73, 0x6d];
    let agent = Agent::new(wasm);

    // Test migration
    let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
    let bytes = snapshot.serialize().unwrap();

    let restored = MigrationSnapshot::deserialize(&bytes).unwrap();
    let new_agent = restored.restore().unwrap();

    // Verify
    assert_eq!(agent.id(), new_agent.id());
}
```

#### Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_allocation_properties(page_count in 1..100usize) {
        let result = allocate_pages(page_count);
        prop_assert!(result.is_ok());

        let address = result.unwrap();
        prop_assert!(address > 0);
        prop_assert!(address % 4096 == 0);  // Page-aligned

        deallocate_pages(address, page_count).unwrap();
    }
}
```

### Benchmarking

#### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench agent_benches

# Save baseline
cargo bench -- --save-baseline main

# Compare to baseline
git checkout feature-branch
cargo bench -- --baseline main
```

#### Writing Benchmarks

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_agent_creation(c: &mut Criterion) {
    c.bench_function("agent_creation", |b| {
        let wasm = vec![0x00, 0x61, 0x73, 0x6d];
        b.iter(|| Agent::new(wasm.clone()));
    });
}

criterion_group!(benches, bench_agent_creation);
criterion_main!(benches);
```

---

## CI/CD Pipeline

### GitHub Actions Workflow

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

jobs:
  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, beta]

    steps:
    - uses: actions/checkout@v3

    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: ${{ matrix.rust }}
        profile: minimal
        override: true

    - name: Cache dependencies
      uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

    - name: Install cargo-nextest
      run: cargo install cargo-nextest

    - name: Run tests
      run: cargo nextest run --all-features

  lint:
    name: Lint
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v3

    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        components: rustfmt, clippy

    - name: Format check
      run: cargo fmt --all -- --check

    - name: Clippy
      run: cargo clippy --all-features -- -D warnings

  benchmark:
    name: Benchmark
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v3

    - name: Run benchmarks
      run: cargo bench --no-fail-fast

    - name: Store benchmark result
      uses: benchmark-action/github-action-benchmark@v1
      with:
        tool: 'cargo'
        output-file-path: target/criterion/output.json
```

### Pre-commit Hooks

Install pre-commit hooks to ensure code quality:

```bash
# .git/hooks/pre-commit
#!/bin/bash

set -e

echo "Running pre-commit checks..."

# Format check
cargo fmt --all -- --check

# Clippy
cargo clippy --all-features -- -D warnings

# Tests
cargo nextest run

echo "All checks passed!"
```

Make executable:

```bash
chmod +x .git/hooks/pre-commit
```

---

## Code Organization

### Module Structure

#### Module Declaration

```rust
// lib.rs
pub mod agent;
pub mod migration;
pub mod policy;

// Re-export public API
pub use agent::Agent;
pub use migration::MigrationSnapshot;
pub use policy::Policy;
```

#### Submodule Organization

```
mielin-cells/src/
├── lib.rs                  # Main entry point
├── agent.rs                # Agent struct and impl
├── migration.rs            # Migration logic
├── policy.rs               # Policy definitions
├── compliance/             # Compliance submodule
│   ├── mod.rs
│   ├── audit.rs
│   └── gdpr.rs
└── security/               # Security submodule
    ├── mod.rs
    ├── sandbox.rs
    └── crypto.rs
```

### Code Style Guidelines

#### 1. Naming Conventions

```rust
// Types: PascalCase
struct AgentPool { }
enum NodeRole { }

// Functions: snake_case
fn allocate_pages() { }
fn get_agent_by_id() { }

// Constants: SCREAMING_SNAKE_CASE
const MAX_PAGES: usize = 1024;
const PAGE_SIZE: usize = 4096;

// Lifetimes: single lowercase letter
fn process<'a>(data: &'a [u8]) { }
```

#### 2. Documentation

All public items must have documentation:

```rust
/// Allocates a specified number of memory pages.
///
/// # Arguments
///
/// * `count` - Number of pages to allocate (must be > 0)
///
/// # Returns
///
/// Returns the virtual address of the first allocated page on success,
/// or a `KernelError` on failure.
///
/// # Errors
///
/// * `KernelError::ZeroAllocation` - If count is 0
/// * `KernelError::OutOfMemory` - If insufficient pages available
///
/// # Examples
///
/// ```
/// use mielin_kernel::memory::allocate_pages;
///
/// let address = allocate_pages(10).unwrap();
/// assert!(address > 0);
/// ```
pub fn allocate_pages(count: usize) -> Result<usize, KernelError> {
    // Implementation
}
```

#### 3. Error Handling

Use `Result` for fallible operations:

```rust
// Good: Return Result
fn load_config() -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string("config.toml")?;
    toml::from_str(&contents)
        .map_err(|e| ConfigError::ParseError(e))
}

// Avoid: Unwrapping in library code
fn load_config() -> Config {
    let contents = std::fs::read_to_string("config.toml").unwrap();
    toml::from_str(&contents).unwrap()
}
```

#### 4. Use of `unsafe`

Minimize `unsafe` code and document thoroughly:

```rust
/// # Safety
///
/// This function is unsafe because it directly accesses hardware registers.
/// The caller must ensure:
/// - The memory-mapped register address is valid
/// - No concurrent access to the same register
/// - The processor is in the correct mode
///
/// # Example
///
/// ```no_run
/// unsafe {
///     write_register(0xE000_ED00, 0x1234);
/// }
/// ```
unsafe fn write_register(address: usize, value: u32) {
    *(address as *mut u32) = value;
}
```

### Project-Specific Conventions

#### No `unwrap()` Policy

Never use `unwrap()` in production code:

```rust
// Bad
let config = load_config().unwrap();

// Good
let config = load_config()
    .context("Failed to load configuration")?;

// Or
let config = match load_config() {
    Ok(c) => c,
    Err(e) => {
        log::error!("Failed to load config: {}", e);
        return Err(e);
    }
};
```

#### Workspace Dependencies

Use workspace-level dependency management:

```toml
# Root Cargo.toml
[workspace.dependencies]
tokio = { version = "1.48", features = ["full"] }
serde = { version = "1.0.228", features = ["derive"] }

# Subcrate Cargo.toml
[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
```

---

## Contributing Guidelines

### Getting Started

1. **Fork the repository**
   ```bash
   # Fork on GitHub, then clone
   git clone https://github.com/YOUR_USERNAME/mielin
   cd mielin
   git remote add upstream https://github.com/cool-japan/mielin
   ```

2. **Create a branch**
   ```bash
   git checkout -b feature/my-feature
   ```

3. **Make changes**
   - Write code
   - Add tests
   - Update documentation
   - Run checks

4. **Commit changes**
   ```bash
   git add .
   git commit -m "feat: add new feature"
   ```

5. **Push and create PR**
   ```bash
   git push origin feature/my-feature
   # Create pull request on GitHub
   ```

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `test`: Add or update tests
- `chore`: Maintenance tasks

**Examples**:

```
feat(agent): add migration retry logic

Add automatic retry with exponential backoff for failed migrations.

Closes #123
```

```
fix(kernel): prevent memory leak in page allocator

The allocator was not properly tracking freed pages, causing a gradual
memory leak over time.

Fixes #456
```

### Pull Request Guidelines

**Before submitting**:

- [ ] All tests pass (`cargo nextest run`)
- [ ] No clippy warnings (`cargo clippy --all-features`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Documentation is updated
- [ ] Changelog entry added (if applicable)
- [ ] Benchmarks run (for performance changes)

**PR Template**:

```markdown
## Description

Brief description of the changes

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing

Describe how you tested your changes

## Checklist

- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] No new warnings
- [ ] Benchmarks run (if applicable)
```

### Code Review Process

1. **Automated checks**: CI must pass
2. **Peer review**: At least 1 approval required
3. **Maintainer review**: For significant changes
4. **Merge**: Squash and merge to main

---

## Release Process

### Versioning

MielinOS follows [Semantic Versioning](https://semver.org/):

- **Major** (x.0.0): Breaking changes
- **Minor** (0.x.0): New features, backward compatible
- **Patch** (0.0.x): Bug fixes, backward compatible

### Release Checklist

1. **Update version numbers**
   ```bash
   # Update Cargo.toml versions
   cargo set-version 0.1.0
   ```

2. **Update CHANGELOG.md**
   ```markdown
   ## [0.1.0] - 2026-01-17

   ### Added
   - New feature X
   - New feature Y

   ### Fixed
   - Bug fix Z
   ```

3. **Run full test suite**
   ```bash
   make test
   make bench
   ```

4. **Build release binaries**
   ```bash
   cargo build --release
   ```

5. **Create git tag**
   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

6. **Publish to crates.io** (when ready)
   ```bash
   cargo publish --dry-run  # Test publish
   cargo publish            # Actual publish
   ```

---

## Developer Tools

### Useful Cargo Extensions

```bash
# cargo-edit: Manage dependencies
cargo install cargo-edit
cargo add tokio
cargo rm old-dependency

# cargo-watch: Auto-rebuild on file changes
cargo install cargo-watch
cargo watch -x test

# cargo-tree: Dependency tree
cargo tree

# cargo-audit: Security audit
cargo install cargo-audit
cargo audit

# cargo-outdated: Check for outdated dependencies
cargo install cargo-outdated
cargo outdated
```

### Debugging Tools

#### GDB/LLDB

```bash
# Build with debug symbols
cargo build

# Debug with lldb (macOS)
lldb target/debug/mielin-cli

# Debug with gdb (Linux)
gdb target/debug/mielin-cli
```

#### Rust-specific debugging

```bash
# Use rust-gdb wrapper
rust-gdb target/debug/mielin-cli

# Use rust-lldb wrapper
rust-lldb target/debug/mielin-cli
```

### Performance Tools

#### Flamegraph

```bash
cargo install flamegraph
sudo cargo flamegraph --bin mielin-cli
```

#### Valgrind

```bash
valgrind --tool=callgrind ./target/release/mielin-cli
kcachegrind callgrind.out.*
```

#### Perf (Linux)

```bash
perf record -g ./target/release/mielin-cli
perf report
```

---

## Additional Resources

### Documentation

- [Rust Book](https://doc.rust-lang.org/book/)
- [Async Book](https://rust-lang.github.io/async-book/)
- [Embedded Rust Book](https://docs.rust-embedded.org/book/)
- [WebAssembly Book](https://rustwasm.github.io/docs/book/)

### MielinOS Specific

- [Architecture Guide](ARCHITECTURE.md)
- [API Reference](API.md)
- [Performance Guide](PERFORMANCE.md)
- [Integration Guide](INTEGRATION.md)

### Communication

- GitHub Issues: Bug reports and feature requests
- GitHub Discussions: Questions and discussions
- Email: contact@cooljapan.tech

---

## Conclusion

This guide provides the foundation for developing MielinOS. Key points:

1. **Setup**: Rust 1.83+, cargo-nextest, proper IDE configuration
2. **Building**: Workspace structure, cross-compilation, build profiles
3. **Testing**: Unit, integration, benchmarks, coverage
4. **CI/CD**: Automated testing, linting, benchmarking
5. **Contributing**: Fork, branch, commit, PR process
6. **Release**: Versioning, changelog, publishing

Welcome to the MielinOS development community!

---

**MielinOS Developer Guide** - Version 1.0 - 2026-01-17
