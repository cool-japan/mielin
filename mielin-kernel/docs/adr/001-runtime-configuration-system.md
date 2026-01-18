# ADR 001: Runtime Configuration System

## Status

Accepted (2026-01-18)

## Context

The MielinOS kernel initially used compile-time constants for critical parameters like `MAX_TASKS` and `MAX_PAGES`. While this approach provides compile-time guarantees and zero runtime overhead, it creates several limitations:

1. **Inflexibility**: Users must recompile the kernel to change configuration
2. **Limited Portability**: Different platforms (embedded vs server) need different builds
3. **Testing Challenges**: Testing different configurations requires multiple compilations
4. **Page Size Variations**: Modern systems use varying page sizes (4KB on x86_64, 16KB on ARM64/M1)

The kernel needed a way to support:
- Different page sizes (4KB, 8KB, 16KB, 64KB)
- Configurable memory and task limits
- Platform-specific optimizations (embedded vs high-performance)
- Multi-core configuration parameters

## Decision

We implemented a comprehensive runtime configuration system with the following components:

### 1. Configuration Module (`config.rs`)

A new `config` module providing:
- `PageSize` enum for alternative page sizes
- `MemoryConfig` for memory subsystem parameters
- `SchedulerConfig` for scheduler parameters
- `MultiCoreConfig` for multi-core settings
- `KernelConfig` as the top-level configuration container

### 2. Builder Pattern

Implemented a fluent builder API for easy configuration:

```rust
let config = KernelConfig::builder()
    .max_pages(2048)
    .max_tasks(128)
    .page_size(PageSize::Size16KB)
    .build()
    .unwrap();
```

### 3. Pre-configured Profiles

Provided common configuration profiles:
- `KernelConfig::default()`: Standard configuration (4KB pages, 1024 pages, 64 tasks)
- `KernelConfig::embedded()`: Minimal resources (512KB memory, 16 tasks, single-core)
- `KernelConfig::high_performance()`: Maximum resources (32MB memory, 256 tasks, 32 CPUs)
- `KernelConfig::arm64_16kb()`: ARM64-specific (16KB pages)

### 4. Validation

All configurations are validated before use:
- Non-zero limits
- Reasonable upper bounds (prevent excessive memory usage)
- Consistent relationships between parameters

### 5. Type Safety

Used Rust's type system for safety:
- `PageSize` enum prevents invalid page sizes
- Builder pattern ensures valid construction
- `Result` types for fallible operations

## Consequences

### Positive

1. **Flexibility**: Users can configure the kernel for different use cases without recompilation
2. **Portability**: Single codebase supports embedded, desktop, and server platforms
3. **Testing**: Easy to test different configurations
4. **Documentation**: Configuration is self-documenting through types
5. **Validation**: Runtime validation prevents invalid configurations
6. **Ergonomics**: Builder pattern and profiles make configuration easy
7. **Backward Compatibility**: Current hardcoded constants remain as defaults

### Negative

1. **Runtime Overhead**: Configuration must be passed around and checked at runtime (minimal impact)
2. **Complexity**: Additional code to maintain (~600 lines)
3. **Memory Usage**: Configuration structures consume memory
4. **Breaking Change**: Future APIs may need to accept configuration parameters

### Neutral

1. **Design Choice**: Chose runtime over compile-time configuration
2. **Scope**: Configuration applies to kernel initialization, not per-operation
3. **Future-Proofing**: Designed to accommodate future parameters

## Alternatives Considered

### 1. Compile-Time Configuration (Status Quo)

**Pros**:
- Zero runtime overhead
- Compile-time guarantees
- Simpler implementation

**Cons**:
- Requires recompilation for changes
- Multiple builds for different platforms
- Difficult to test variations

**Decision**: Rejected due to inflexibility

### 2. Environment Variables

**Pros**:
- Standard Unix approach
- No code changes needed

**Cons**:
- Not available in `no_std` environments
- Type-unsafe
- Difficult to validate
- Not suitable for embedded systems

**Decision**: Rejected due to `no_std` requirement

### 3. Configuration Files

**Pros**:
- Familiar to users
- Easy to edit

**Cons**:
- Requires filesystem
- Parsing overhead
- Not available in early boot
- Adds dependencies

**Decision**: Rejected due to filesystem dependency

### 4. Feature Flags

**Pros**:
- Compile-time selection
- Cargo-native

**Cons**:
- Limited granularity
- Combinatorial explosion
- Still requires recompilation

**Decision**: Rejected due to inflexibility

## Implementation Details

### Page Size Support

```rust
pub enum PageSize {
    Size4KB = 4096,
    Size8KB = 8192,
    Size16KB = 16384,
    Size64KB = 65536,
}
```

Each page size includes helper methods:
- `bytes()`: Get size in bytes
- `shift()`: Get log2 shift value
- `is_aligned()`: Check alignment
- `align_up()` / `align_down()`: Alignment operations

### Configuration Structure

```rust
pub struct KernelConfig {
    pub memory: MemoryConfig,
    pub scheduler: SchedulerConfig,
    pub multicore: MultiCoreConfig,
}
```

### Validation Strategy

Each configuration struct implements `validate()`:
- Checks for invalid values (zero, excessive)
- Returns descriptive errors
- Called automatically by builder

## Migration Path

### Phase 1: Introduction (v0.1.0) ✅
- Add configuration module
- Maintain backward compatibility with defaults
- Provide migration examples

### Phase 2: Adoption (v0.2.0)
- Update kernel initialization to accept configuration
- Migrate subsystems to use configuration
- Deprecate hardcoded constants

### Phase 3: Removal (v1.0.0)
- Remove deprecated constants
- Make configuration mandatory
- Full runtime configuration

## Related Decisions

- **ADR 002** (future): Memory Allocator Architecture
- **ADR 003** (future): Scheduler Design
- **ADR 004** (future): Multi-Core Strategy

## References

- [Rust Builder Pattern](https://rust-lang.github.io/api-guidelines/type-safety.html#builders-enable-construction-of-complex-values-c-builder)
- [Type-Safe Configuration in Rust](https://deterministic.space/elegant-apis-in-rust.html)
- [Linux Kernel Configuration](https://www.kernel.org/doc/html/latest/kbuild/kconfig.html) (for comparison)

## Notes

- Configuration is validated at build time (via builder)
- Default values chosen based on common use cases
- Can be extended with additional parameters in future
- Designed to work in both `std` and `no_std` environments

---

**Author**: COOLJAPAN OU (Team Kitasan)
**Reviewers**: N/A
**Last Updated**: 2026-01-18
