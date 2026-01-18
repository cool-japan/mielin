# Contributing to MielinOS Kernel

Thank you for your interest in contributing to the MielinOS Kernel! This document provides guidelines and instructions for contributing.

## Code of Conduct

We are committed to providing a welcoming and inclusive environment. All contributors are expected to:

- Be respectful and constructive in communications
- Welcome newcomers and help them get started
- Focus on what is best for the community and the project
- Show empathy towards other community members

## Getting Started

### Prerequisites

- Rust toolchain (stable and nightly)
- Basic understanding of operating systems concepts
- Familiarity with `no_std` Rust development

### Development Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/cool-japan/mielin-kernel
   cd mielin-kernel
   ```

2. Run tests to ensure everything works:
   ```bash
   cargo test --lib --features std
   ```

3. Run examples:
   ```bash
   cargo run --example memory_allocator --features std
   cargo run --example scheduler --features std
   cargo run --example config --features std
   ```

## How to Contribute

### Reporting Bugs

Before creating a bug report:
- Check existing issues to avoid duplicates
- Collect relevant information (OS, Rust version, error messages)

When filing a bug report, include:
- Clear, descriptive title
- Steps to reproduce the issue
- Expected vs actual behavior
- Code samples if applicable
- Environment details

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. When suggesting an enhancement:
- Use a clear and descriptive title
- Provide step-by-step description of the enhancement
- Explain why this enhancement would be useful
- List examples of other projects with similar features (if applicable)

### Pull Requests

1. **Fork and create a branch**
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make your changes**
   - Follow the code style (see below)
   - Add tests for new functionality
   - Update documentation as needed

3. **Run tests and checks**
   ```bash
   cargo test --lib
   cargo clippy -- -D warnings
   cargo fmt --all --check
   ```

4. **Commit your changes**
   - Use clear, descriptive commit messages
   - Follow conventional commits format:
     ```
     feat: add new memory allocation strategy
     fix: resolve double-free in page allocator
     docs: update configuration guide
     test: add edge cases for scheduler
     ```

5. **Push and create PR**
   ```bash
   git push origin feature/my-feature
   ```
   Then create a pull request on GitHub

## Code Style Guidelines

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` with default settings
- Run `cargo clippy` and address all warnings
- Use meaningful variable and function names
- Prefer `snake_case` for variables and functions
- Use `PascalCase` for types and structs

### Documentation

- Document all public APIs with `///` doc comments
- Include examples in documentation where helpful
- Use `//!` for module-level documentation
- Keep line length under 100 characters where practical

### Testing

- Write unit tests for all new functionality
- Add integration tests for subsystem interactions
- Include edge cases and error conditions
- Aim for >90% code coverage for new code

Example test structure:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Arrange
        let mut mm = MemoryManager::default();

        // Act
        let addr = mm.allocate_page().unwrap();

        // Assert
        assert_eq!(addr % PAGE_SIZE, 0);
    }
}
```

### Safety and Correctness

- Minimize use of `unsafe` code
- Document all `unsafe` blocks with SAFETY comments
- Ensure memory safety in all allocations
- Use proper synchronization primitives
- Avoid panics in core kernel code

Example `unsafe` documentation:
```rust
// SAFETY: This is safe because:
// 1. The pointer is properly aligned
// 2. The memory region is exclusively owned
// 3. The lifetime is bound to the containing struct
unsafe { ptr::write(addr, value) }
```

## No Warnings Policy

The MielinOS Kernel follows a strict **no warnings policy**. All code must compile without warnings:

```bash
cargo build --lib 2>&1 | grep warning
# Should output nothing
```

If you encounter warnings:
1. Fix them before submitting your PR
2. If unsure how to fix, ask for help in the PR discussion
3. Use `#[allow(clippy::...)]` only when absolutely necessary and with justification

## Architecture Guidelines

### Module Organization

- Keep modules focused and cohesive
- Maximum 2000 lines per file (use `splitrs` if larger)
- Group related functionality together
- Use clear module hierarchies

### Performance Considerations

- Target performance goals (see TODO.md)
- Benchmark performance-critical code
- Use appropriate data structures
- Minimize allocations in hot paths
- Profile before optimizing

### Memory Management

- All allocations must be tracked
- Implement proper cleanup/deallocation
- Handle OOM conditions gracefully
- Prefer stack allocation when possible

### Concurrency

- Use atomic operations for lock-free code
- Document synchronization requirements
- Avoid deadlocks through careful lock ordering
- Consider cache-line alignment for shared data

## Workspace Policy

- Use workspace dependencies (`*.workspace = true`)
- Keep individual `Cargo.toml` files minimal
- Update workspace `Cargo.toml` for shared dependencies
- Use latest stable versions from crates.io

## Commit Message Format

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `test`: Adding or updating tests
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `style`: Code style changes (formatting, etc.)
- `chore`: Maintenance tasks

Examples:
```
feat(memory): add support for 64KB pages

Implements PageSize::Size64KB for systems that use larger pages.
Includes tests and documentation updates.

Closes #123
```

```
fix(scheduler): prevent double-free in task termination

The scheduler was freeing task memory twice when terminating
running tasks. Added guard to check task state before freeing.

Fixes #456
```

## Review Process

All submissions require review before merging:

1. **Automated Checks**: CI must pass (tests, clippy, fmt)
2. **Code Review**: At least one maintainer approval required
3. **Testing**: New features must include tests
4. **Documentation**: Public APIs must be documented

Review timeline:
- Initial response: Within 3 business days
- Full review: Within 1 week
- Merges: After approval and CI passes

## Getting Help

- **Questions**: Open a GitHub Discussion
- **Issues**: File a GitHub Issue
- **Real-time**: Join our community (details in README)

## Recognition

Contributors are recognized in:
- CONTRIBUTORS.md file
- Release notes
- Git commit history

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (see LICENSE file).

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Embedded Book](https://rust-embedded.github.io/book/)
- [no_std Rust Development](https://docs.rust-embedded.org/embedonomicon/)
- [MielinOS Architecture Docs](docs/architecture/)

---

Thank you for contributing to MielinOS Kernel! 🚀
