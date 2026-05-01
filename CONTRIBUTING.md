# Contributing to MielinOS

Thank you for your interest in contributing to MielinOS! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Contributions](#making-contributions)
- [Coding Standards](#coding-standards)
- [Testing Guidelines](#testing-guidelines)
- [Pull Request Process](#pull-request-process)
- [Project Structure](#project-structure)
- [Communication](#communication)

## Code of Conduct

### Our Pledge

We pledge to make participation in MielinOS a harassment-free experience for everyone, regardless of age, body size, disability, ethnicity, gender identity and expression, level of experience, nationality, personal appearance, race, religion, or sexual identity and orientation.

### Our Standards

- **Be respectful**: Treat everyone with respect and consideration
- **Be collaborative**: Work together towards common goals
- **Be professional**: Keep discussions technical and constructive
- **Be inclusive**: Welcome newcomers and help them learn
- **Give constructive feedback**: Focus on code, not people

### Unacceptable Behavior

- Harassment, discrimination, or offensive comments
- Trolling, insulting/derogatory comments, or personal attacks
- Publishing others' private information without permission
- Any conduct that could reasonably be considered inappropriate

## Getting Started

### Prerequisites

Before contributing, ensure you have:

- **Rust**: 1.83 or later (latest stable recommended)
- **cargo-nextest**: Install with `cargo install cargo-nextest`
- **Git**: For version control
- **Code editor**: VS Code with rust-analyzer recommended

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/mielin.git
   cd mielin
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/cool-japan/mielin.git
   ```

## Development Setup

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/cool-japan/mielin.git
cd mielin

# Build all crates
cargo build

# Run tests
cargo nextest run

# Check formatting
cargo fmt --check

# Run linter
cargo clippy -- -D warnings
```

### Development Workflow

```bash
# Create a feature branch
git checkout -b feature/your-feature-name

# Make your changes
# ... edit files ...

# Format code
cargo fmt

# Run tests
cargo nextest run

# Check for warnings
cargo clippy

# Commit your changes
git add .
git commit -m "Description of your changes"

# Push to your fork
git push origin feature/your-feature-name
```

## Making Contributions

### Types of Contributions

We welcome various types of contributions:

- **Bug fixes**: Fix issues in existing code
- **Features**: Implement new functionality
- **Documentation**: Improve or add documentation
- **Tests**: Add or improve test coverage
- **Performance**: Optimize existing code
- **Examples**: Add new examples or improve existing ones

### Finding Work

- Check the [TODO.md](TODO.md) for planned features
- Look for issues labeled `good first issue` or `help wanted`
- Propose new features in GitHub Discussions first
- Fix bugs reported in GitHub Issues

### Before Starting

1. **Check for duplicates**: Search existing issues and PRs
2. **Discuss major changes**: Open an issue or discussion first
3. **Start small**: Begin with small contributions to learn the codebase
4. **Read documentation**: Familiarize yourself with the architecture

## Coding Standards

### Rust Style Guide

Follow the [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/):

```rust
// Good: Clear, idiomatic Rust
pub fn process_agent(agent: &Agent) -> Result<(), AgentError> {
    let state = agent.state();
    if state == AgentState::Running {
        agent.suspend()?;
    }
    Ok(())
}

// Bad: Unclear, verbose
pub fn process_agent(agent: &Agent) -> Result<(), AgentError> {
    let state = agent.state();
    if state == AgentState::Running {
        match agent.suspend() {
            Ok(_) => {},
            Err(e) => return Err(e),
        }
    }
    return Ok(());
}
```

### File Organization

- **Maximum 2000 lines per file**: Refactor if exceeded
- **Logical modules**: Group related functionality
- **Clear naming**: Use descriptive names for types and functions
- **Documentation**: Add doc comments for public APIs

### Error Handling

Use appropriate error handling patterns:

```rust
// Use Result for recoverable errors
pub fn migrate_agent(agent: &Agent) -> Result<(), MigrationError> {
    // ...
}

// Use thiserror for error types
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
}
```

### Documentation

All public APIs must be documented:

```rust
/// Migrates an agent to a target node.
///
/// # Arguments
///
/// * `agent` - The agent to migrate
/// * `target` - Target node ID
///
/// # Returns
///
/// Returns `Ok(())` on successful migration, or an error if migration fails.
///
/// # Examples
///
/// ```
/// let agent = Agent::new(wasm_binary);
/// migrate_agent(&agent, target_id)?;
/// ```
pub fn migrate_agent(agent: &Agent, target: NodeId) -> Result<(), MigrationError> {
    // Implementation
}
```

### Naming Conventions

- **Types**: `PascalCase` (e.g., `AgentState`, `MigrationSnapshot`)
- **Functions**: `snake_case` (e.g., `migrate_agent`, `create_snapshot`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_AGENTS`, `DEFAULT_TIMEOUT`)
- **Modules**: `snake_case` (e.g., `agent`, `migration`, `wire_protocol`)

## Testing Guidelines

### Test Requirements

All code must include tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        assert!(agent.id().as_bytes().len() == 16);
    }

    #[test]
    fn test_migration_snapshot() {
        let agent = Agent::new(vec![]);
        let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
        assert_eq!(snapshot.agent_id, *agent.id().as_bytes());
    }
}
```

### Test Coverage

- **Unit tests**: Test individual functions and methods
- **Integration tests**: Test component interactions
- **Documentation tests**: Include examples in doc comments
- **Edge cases**: Test boundary conditions and error paths

### Running Tests

```bash
# Run all tests
cargo nextest run

# Run tests for specific crate
cargo nextest run -p mielin-kernel

# Run with output
cargo nextest run -- --nocapture

# Run specific test
cargo test test_agent_creation
```

### Test Guidelines

- **Clear test names**: Describe what is being tested
- **One assertion per test**: Keep tests focused
- **Use fixtures**: Create reusable test data
- **Test failures**: Verify error conditions
- **No flaky tests**: Tests must be deterministic

## Pull Request Process

### Before Submitting

1. **Update documentation**: Ensure docs reflect your changes
2. **Add tests**: Cover new functionality and bug fixes
3. **Run full test suite**: `cargo nextest run`
4. **Format code**: `cargo fmt`
5. **Check lints**: `cargo clippy -- -D warnings`
6. **Update CHANGELOG.md**: Add your changes to Unreleased section

### PR Guidelines

- **Clear title**: Describe the change concisely
- **Detailed description**: Explain what, why, and how
- **Link issues**: Reference related issues (e.g., "Fixes #123")
- **Small PRs**: Keep changes focused and reviewable
- **Clean commits**: Squash work-in-progress commits

### PR Template

```markdown
## Description
Brief description of changes

## Motivation
Why is this change needed?

## Changes
- Bullet list of specific changes
- Feature additions
- Bug fixes

## Testing
How was this tested?

## Checklist
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] Changelog updated
- [ ] No warnings from clippy
- [ ] All tests passing
```

### Review Process

1. **Automated checks**: CI must pass
2. **Code review**: At least one maintainer approval
3. **Address feedback**: Make requested changes
4. **Final approval**: Maintainer merges the PR

## Project Structure

### Crate Organization

```
mielin/
├── mielin-kernel/       # Core kernel (no_std)
├── mielin-hal/          # Hardware abstraction
├── mielin-rt/           # Embedded runtime
├── mielin-mesh/         # Networking
│   ├── core/            # DHT and routing
│   └── wire/            # Protocol
├── mielin-cells/        # Agent SDK
├── mielin-wasm/         # WASM runtime
├── mielin-tensor/       # TensorLogic
├── mielin-cli/          # CLI tool
└── examples/            # Examples
```

### Adding a New Crate

1. Create crate directory: `cargo new --lib mielin-newcrate`
2. Add to workspace in `Cargo.toml`
3. Add README.md with documentation
4. Implement functionality with tests
5. Update main README.md with component link

### Dependencies

- **Use workspace dependencies**: Add to workspace `Cargo.toml`
- **Latest stable versions**: Always use latest from crates.io
- **Minimal dependencies**: Only add what's necessary
- **Document why**: Explain dependency choices in commit messages

## Communication

### Channels

- **GitHub Issues**: Bug reports, feature requests
- **GitHub Discussions**: Questions, ideas, general discussion
- **Pull Requests**: Code contributions
- **Email**: For private matters (security vulnerabilities)

### Issue Template

```markdown
**Bug Report / Feature Request**

**Description**
Clear description of the issue or feature

**Expected Behavior**
What should happen

**Actual Behavior**
What actually happens

**Steps to Reproduce**
1. Step one
2. Step two
3. ...

**Environment**
- OS:
- Rust version:
- MielinOS version:

**Additional Context**
Any other relevant information
```

### Getting Help

- **Read documentation**: Check README and component docs
- **Search issues**: Someone may have asked already
- **Ask in discussions**: For general questions
- **Be patient**: Maintainers are volunteers

## Development Tips

### Useful Commands

```bash
# Watch for changes and run tests
cargo watch -x "nextest run"

# Generate documentation
cargo doc --open

# Check for outdated dependencies
cargo outdated

# Update dependencies
cargo update

# Benchmark performance (future)
cargo bench

# Check code coverage (future)
cargo tarpaulin
```

### IDE Setup (VS Code)

Recommended extensions:

- `rust-analyzer`: Rust language server
- `CodeLLDB`: Debugging
- `Better TOML`: TOML syntax highlighting
- `crates`: Dependency management

### Debugging

```rust
// Use dbg! macro for quick debugging
let agent = Agent::new(wasm);
dbg!(&agent);

// Use RUST_LOG for structured logging
RUST_LOG=debug cargo run

// Use rust-gdb or rust-lldb for debugging
rust-lldb target/debug/mielin-cli
```

## Recognition

Contributors will be:

- Listed in project documentation
- Mentioned in release notes
- Credited in commits and PRs
- Invited to contribute to future releases

## License

By contributing to MielinOS, you agree that your contributions will be licensed under the Apache License 2.0.

---

Thank you for contributing to MielinOS! Together we're building the nervous system for the ASI era. 🧠⚡
