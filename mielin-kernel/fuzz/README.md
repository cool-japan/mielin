# Fuzz Testing for mielin-kernel

This directory contains fuzz testing targets for the MielinOS kernel using libFuzzer via cargo-fuzz.

## Prerequisites

Fuzzing requires **nightly Rust** due to the use of sanitizers and unstable compiler features:

```bash
# Install nightly toolchain
rustup install nightly

# Install cargo-fuzz
cargo install cargo-fuzz
```

## Available Fuzz Targets

### 1. Memory Allocator (`memory_allocator`)

Fuzzes the page-based memory allocator with random allocation, deallocation, strategy changes, and defragmentation operations.

**Operations:**
- Allocate pages (1-64 pages)
- Free allocated pages
- Change allocation strategy (FirstFit, BestFit, WorstFit)
- Defragment memory

**Invariants checked:**
- Total pages = allocated + free
- Utilization within 0-100%
- All allocations can be freed
- Memory manager remains consistent

**Run:**
```bash
cargo +nightly fuzz run memory_allocator
```

### 2. Scheduler (`scheduler`)

Fuzzes the task scheduler with random task spawning, scheduling, yielding, and termination operations.

**Operations:**
- Spawn tasks with various priorities (0-255)
- Schedule next task
- Yield current task
- Terminate tasks

**Invariants checked:**
- spawned >= terminated
- active_tasks <= MAX_TASKS (64)
- peak_tasks <= MAX_TASKS
- success_rate within 0-100%
- utilization within 0-100%

**Run:**
```bash
cargo +nightly fuzz run scheduler
```

## Running Fuzz Tests

### Quick Test (10 seconds)
```bash
cargo +nightly fuzz run memory_allocator -- -max_total_time=10
cargo +nightly fuzz run scheduler -- -max_total_time=10
```

### Full Fuzz (Continuous)
```bash
# Memory allocator
cargo +nightly fuzz run memory_allocator

# Scheduler
cargo +nightly fuzz run scheduler

# Run both in parallel (separate terminals)
cargo +nightly fuzz run memory_allocator &
cargo +nightly fuzz run scheduler &
```

### With Custom Options
```bash
# Limit execution time
cargo +nightly fuzz run memory_allocator -- -max_total_time=60

# Limit number of runs
cargo +nightly fuzz run memory_allocator -- -runs=1000000

# Use multiple cores
cargo +nightly fuzz run memory_allocator -- -workers=4

# Minimize corpus
cargo +nightly fuzz cmin memory_allocator
```

## Corpus Management

Fuzz inputs that trigger new code paths are saved in `fuzz/corpus/<target>/`.

```bash
# List corpus
ls fuzz/corpus/memory_allocator/

# Clear corpus
rm -rf fuzz/corpus/memory_allocator/*

# Minimize corpus (remove redundant inputs)
cargo +nightly fuzz cmin memory_allocator
```

## Crash Analysis

If a crash is found, it will be saved in `fuzz/artifacts/<target>/`.

```bash
# List crashes
ls fuzz/artifacts/memory_allocator/

# Reproduce a crash
cargo +nightly fuzz run memory_allocator fuzz/artifacts/memory_allocator/crash-<hash>

# Debug a crash
cargo +nightly fuzz run memory_allocator fuzz/artifacts/memory_allocator/crash-<hash> -- -runs=1
```

## Coverage Analysis

```bash
# Generate coverage report
cargo +nightly fuzz coverage memory_allocator

# View coverage (requires llvm-cov)
cargo +nightly fuzz coverage memory_allocator --html
open fuzz/coverage/memory_allocator/html/index.html
```

## CI Integration

The fuzz targets are set up but require nightly Rust. For CI:

```yaml
- name: Run fuzz tests
  run: |
    rustup install nightly
    cargo install cargo-fuzz
    cargo +nightly fuzz run memory_allocator -- -max_total_time=60 -rss_limit_mb=2048
    cargo +nightly fuzz run scheduler -- -max_total_time=60 -rss_limit_mb=2048
```

## Troubleshooting

### "only accepted on nightly"
Switch to nightly Rust: `cargo +nightly fuzz ...`

### Out of memory
Limit memory usage: `cargo +nightly fuzz run memory_allocator -- -rss_limit_mb=2048`

### Slow fuzzing
Enable more workers: `cargo +nightly fuzz run memory_allocator -- -workers=4`

## Performance Tips

1. **Use release mode**: Fuzzing runs in release mode by default for speed
2. **Multiple workers**: Use `-workers=N` to parallelize fuzzing
3. **Corpus minimization**: Regularly minimize corpus with `cargo +nightly fuzz cmin`
4. **Dictionary**: Add a dictionary file in `fuzz/` to guide fuzzing
5. **Limit memory**: Use `-rss_limit_mb` to prevent OOM

## References

- [cargo-fuzz documentation](https://rust-fuzz.github.io/book/cargo-fuzz.html)
- [libFuzzer options](https://llvm.org/docs/LibFuzzer.html#options)
- [Rust Fuzz Book](https://rust-fuzz.github.io/book/)
