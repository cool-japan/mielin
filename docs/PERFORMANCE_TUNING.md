# MielinOS Performance Tuning Guide

This guide is a **configuration and tuning reference**: it tells you which
knobs exist in the MielinOS codebase, what their real defaults are, and how
to change them for your workload. It does **not** repeat benchmark results —
for measured numbers (GFLOPS tables, migration latencies, throughput), see
[`PERFORMANCE.md`](./PERFORMANCE.md) and [`../benches/README.md`](../benches/README.md).
Every parameter documented here is a real struct field, constant, feature
flag, or `Cargo.toml` profile setting that exists in the workspace today —
file paths are given so you can jump straight to the source.

## Table of Contents

1. [Tuning Philosophy](#1-tuning-philosophy)
2. [Build Profiles](#2-build-profiles)
3. [Tensor / Compute Tuning](#3-tensor--compute-tuning)
4. [Kernel / Runtime Tuning](#4-kernel--runtime-tuning)
5. [Mesh / Gossip Tuning](#5-mesh--gossip-tuning)
6. [Wire / Transport Tuning](#6-wire--transport-tuning)
7. [Migration Tuning](#7-migration-tuning)
8. [Memory / Footprint Tuning (Embedded)](#8-memory--footprint-tuning-embedded)
9. [Tuning Checklist & Decision Table](#9-tuning-checklist--decision-table)

---

## 1. Tuning Philosophy

**Measure first, tune second.** MielinOS ships a `benches` workspace member
(criterion-based) and a `Makefile bench` target specifically so that every
tuning claim in this document can be verified rather than assumed.

```bash
# All benchmark suites (agent, mesh, migration, tensor, kernel)
make bench                              # == cargo bench -p benches

# One suite / one benchmark
cargo bench -p benches --bench tensor_performance
cargo bench -p benches --bench mesh_benches -- dht_peer_lookup

# Baseline before/after a tuning change
cargo bench -p benches -- --save-baseline before
#  ... change a knob ...
cargo bench -p benches -- --baseline before
```

The suites map to real files under [`../benches/benches/`](../benches/benches/):
`agent_benches.rs`, `mesh_benches.rs`, `mesh_performance.rs`,
`migration_performance.rs`, `tensor_performance.rs`, `kernel_performance.rs`
(see [`../benches/Cargo.toml`](../benches/Cargo.toml) for the exact `[[bench]]`
targets, and [`../benches/README.md`](../benches/README.md) for what each
benchmark measures). `mielin-tensor` additionally ships its own
`quantization_bench`, `simd_bench`, `comprehensive_bench`, and
`performance_analysis` benches (see
[`../mielin-tensor/Cargo.toml`](../mielin-tensor/Cargo.toml)).

Every current measured baseline (page allocation ~50 ns, task spawn ~100 ns,
migration snapshot ~10 µs for 1 KB state, GEMM GFLOPS per backend, etc.) is
recorded in [`PERFORMANCE.md`](./PERFORMANCE.md#performance-characteristics) —
treat those numbers as the reference point before and after any change you
make using the knobs below.

General order of operations when tuning a MielinOS deployment:

1. Pick the right **build profile** (Section 2) — this dominates raw
   throughput and binary size before any runtime knob matters.
2. Pick the right **tensor backend / quantization** for the target hardware
   (Section 3) if the workload is compute-bound.
3. Size the **kernel scheduler / allocators** for the expected task and
   memory pressure (Section 4).
4. Tune **gossip** for your cluster's failure-detection vs. bandwidth
   trade-off (Section 5) — this is the single most impactful mesh-level knob.
5. Tune **wire-level backoff/retry/batching/compression** for the network
   environment (WAN vs. LAN vs. embedded radio) (Section 6).
6. Tune **migration compression/delta strategy** for agent state size and
   change rate (Section 7).
7. For embedded/IoT targets, tune **`mielin-rt` energy policy and pool
   sizing** (Section 8).

---

## 2. Build Profiles

All profiles are defined once, at the workspace root
[`Cargo.toml`](../Cargo.toml), and apply to every crate in the workspace
(the workspace policy forbids per-crate profile overrides).

| Profile | opt-level | lto | codegen-units | panic | strip | debug |
|---|---|---|---|---|---|---|
| `release` (default `--release`) | `"z"` (size) | `true` (fat) | `1` | `"abort"` | `true` | — |
| `release-speed` | `3` (speed) | `"thin"` | `1` | inherited (`"abort"`) | inherited (`true`) | — |
| `release-embedded` | `"z"` (size) | `true` (fat) | `1` | `"abort"` | `true` | — |
| `release-debug` | inherited (`"z"`) | inherited | inherited | inherited | `false` | `true` |
| `bench` | inherited (`"z"`) | `"thin"` | inherited | inherited | `false` | `true` |
| `dev` (default, no flag) | `0` | — | — | `"abort"` | — | `true` |

Source: [`Cargo.toml`](../Cargo.toml) `[profile.*]` sections.

### When to use which

- **`release`** (`cargo build --release`) — the workspace default optimizes
  for **size**, not raw speed (`opt-level = "z"`). This is a deliberate
  choice consistent with MielinOS's footprint goals in
  [`PERFORMANCE.md`](./PERFORMANCE.md#design-goals) (`<100 MB` kernel target,
  currently `~4 MB`). Use this for the kernel, `mielin-cli`, and any
  deployment where binary/image size matters more than peak throughput.
- **`release-speed`** — use for throughput-critical, non-size-constrained
  deployments: mesh super-peers, tensor-inference servers, load-test/bench
  hosts. It flips `opt-level` to `3` and downgrades LTO from fat (`true`) to
  `"thin"` (faster compile, still cross-function inlining), which matters
  for tight loops in `mielin-tensor`'s SIMD backends and the mesh wire hot
  path.
  ```bash
  cargo build --profile release-speed -p mielin-mesh-wire
  cargo build --profile release-speed -p mielin-tensor
  ```
- **`release-embedded`** — currently identical in substance to `release`
  (same `opt-level`, fat LTO, `codegen-units = 1`, `panic = "abort"`,
  `strip = true`); it exists as a semantically distinct target for
  `mielin-rt`/Cortex-M/RISC-V builds so embedded-specific profile tweaks can
  be introduced later without touching the generic `release` profile used by
  server-class crates.
  ```bash
  cargo build --profile release-embedded -p mielin-rt --target thumbv7em-none-eabihf
  ```
- **`release-debug`** — same optimization level as `release` but with
  `strip = false` and `debug = true`, so you can attach a debugger or capture
  a flamegraph against a binary that behaves like production but retains
  symbols. Use this instead of plain `dev` when a bug only reproduces under
  optimization.
- **`bench`** — automatically selected by `cargo bench`; keeps `"thin"` LTO
  and debug symbols (`strip = false`, `debug = true`) so criterion/perf/
  flamegraph output is attributable to source lines, while still running at
  release-level codegen.
- **`dev`** (default `cargo build`/`cargo test`) — `opt-level = 0`,
  `panic = "abort"` (note: **not** the Rust default `"unwind"` — MielinOS
  aborts on panic even in dev builds, so `catch_unwind`-based test patterns
  will not work; this is consistent with the kernel's `no_std` targets which
  cannot unwind).

**`panic = "abort"` is set on every profile** (`release`, `dev`, and
inherited by all others) — this removes unwind-table codegen everywhere,
which both shrinks binaries and matches the `no_std` kernel's requirements.
Keep this in mind if you add a dependency that assumes panic=unwind test
harness behavior.

---

## 3. Tensor / Compute Tuning

All tensor tuning surfaces live in
[`mielin-tensor/src/`](../mielin-tensor/src/); Cargo features are defined in
[`mielin-tensor/Cargo.toml`](../mielin-tensor/Cargo.toml).

### 3.1 Feature flags (compile-time backend selection)

```toml
[features]
default = []
std = []
parallel = ["std", "rayon", "num_cpus"]
cuda = ["std"]
metal = ["std"]
apple-neural-engine = ["std"]
edge-tpu = ["std"]
qualcomm-npu = ["std"]
onnx = ["std"]
tflite = ["std"]
onnxruntime = ["std"]
hailo = ["std"]
all = ["parallel", "cuda", "metal", "apple-neural-engine", "edge-tpu",
       "qualcomm-npu", "onnx", "tflite", "onnxruntime", "hailo"]
```

- **`parallel`** — enables `rayon`/`num_cpus` and unlocks
  [`mielin-tensor/src/parallel.rs`](../mielin-tensor/src/parallel.rs)'s
  `ParallelOps` (`parallel_add`, `parallel_mul`, `parallel_matmul`,
  `parallel_map`, `parallel_reduce`). Without it, parallel ops fall back to a
  manual `std::thread::spawn` chunking path (still functional, no rayon work
  stealing). Enable for server/desktop deployments with ≥4 cores; leave off
  for embedded targets with a single core (rayon's thread-pool overhead is
  wasted there).
- **`cuda` / `metal`** — gate [`mielin-tensor/src/gpu/cuda.rs`](../mielin-tensor/src/gpu/cuda.rs)
  and [`gpu/metal.rs`](../mielin-tensor/src/gpu/metal.rs), adding
  `GpuBackend::Cuda` / `GpuBackend::Metal` variants to the enum in
  [`gpu/mod.rs`](../mielin-tensor/src/gpu/mod.rs). Without either feature,
  `GpuBackend` only has the `None` (CPU-fallback) variant — GPU code paths
  are compiled out entirely, not merely disabled at runtime.
- **`apple-neural-engine` / `edge-tpu` / `qualcomm-npu` / `onnxruntime` /
  `hailo`** — analogously gate the `NpuBackend` variants in
  [`npu/mod.rs`](../mielin-tensor/src/npu/mod.rs)
  (`AppleNeuralEngine`, `EdgeTpu`, `QualcommNpu`, `OnnxRuntime`, `Hailo8`).
- **`onnx` / `tflite`** — gate model import/export in
  [`formats/onnx.rs`](../mielin-tensor/src/formats/onnx.rs) and
  [`formats/tflite.rs`](../mielin-tensor/src/formats/tflite.rs).

Pick the minimum feature set for your deployment target — every additional
feature adds compiled code (works against the size-optimized `release`
profile from Section 2) even when the corresponding hardware is absent at
runtime (the code still runtime-detects and falls back to CPU if the device
isn't present).

### 3.2 SIMD backend selection (runtime)

Backend dispatch is driven by
[`mielin-hal::capabilities::HardwareCapabilities`](../mielin-hal/src/capabilities.rs),
a `bitflags` set:

```rust
const SIMD = 1 << 0;   const SVE  = 1 << 1;   const SVE2 = 1 << 2;
const SME  = 1 << 3;   const NPU  = 1 << 4;   const CRYPTO = 1 << 5;
const ATOMICS = 1 << 6; const FPU = 1 << 7;   const NEON = 1 << 8;
const AVX  = 1 << 9;   const AVX2 = 1 << 10;  const AVX512 = 1 << 11;
const SSE4_2 = 1 << 12; const FMA = 1 << 13;  const AES_NI = 1 << 14;
const RVV = 1 << 15;   const RVB  = 1 << 16;  const RVK  = 1 << 17;
```

`HardwareProfile::detect()` (in the same file) probes the CPU **once** and
caches the result in `static` atomics; call
`HardwareProfile::invalidate_cache()` to force re-detection (useful for
hot-plug/hypervisor migration scenarios, or in tests that need to simulate
different hardware). `TensorRuntime::new(capabilities)`
([`mielin-tensor/src/lib.rs`](../mielin-tensor/src/lib.rs)) wraps a
`TensorOps` dispatcher and exposes `supports_sve2()`, `supports_sme()`,
`supports_neon()`, `supports_avx2()`, `supports_avx512()`, and
`acceleration_info()` for introspection:

```rust
use mielin_tensor::TensorRuntime;
use mielin_hal::capabilities::HardwareProfile;

let profile = HardwareProfile::detect();
let runtime = TensorRuntime::new(profile.capabilities);
println!("{}", runtime.acceleration_info());
let result = runtime.ops().dot(&a, &b)?;
```

The concrete per-arch kernels live in
[`mielin-tensor/src/backends/`](../mielin-tensor/src/backends/):
`avx2.rs`, `avx512.rs`, `neon.rs`, `sve2.rs`, `rvv.rs` — each exports
`add_*`, `dot_*`, `matvec_*`, and (where applicable) `fma_*`, `scale_*`,
`sum_*`, `min_*`/`max_*`, `mul_*`, `sub_*`. There is no manual "force
backend" switch exposed today — selection is automatic based on detected
capabilities, so tuning here means: **build for the target ISA** (e.g. with
`RUSTFLAGS="-C target-cpu=native"` or an explicit `-C target-feature=+avx512f`)
so the corresponding capability bit is set and the fast path is taken.
Measured GFLOPS/latency per backend are in
[`PERFORMANCE.md`](./PERFORMANCE.md#matrix-multiplication-gemm) (e.g. AVX2
16 GFLOPS / AVX512 64 GFLOPS / SVE2 32 GFLOPS on a 1000×1000 GEMM) — use
those figures, not new measurements, when documenting expected throughput.

### 3.3 Fused multiply-add (fused ops)

`fma_avx512` ([`backends/avx512.rs`](../mielin-tensor/src/backends/avx512.rs)),
`fma_sve2` ([`backends/sve2.rs`](../mielin-tensor/src/backends/sve2.rs)), and
`fma_rvv` ([`backends/rvv.rs`](../mielin-tensor/src/backends/rvv.rs)) fuse a
multiply and an add into a single vector instruction (`a*b+c` in one op,
one rounding step) — this is the "fused ops" tuning surface: for any
GEMM/GEMV/conv inner loop, prefer these over separate `mul_*` + `add_*`
calls where the target ISA supports them (AVX-512, SVE2, RVV), since it
halves instruction count and improves numerical accuracy (single rounding).
AVX2 and NEON backends in this codebase do not currently export a dedicated
`fma_*` entry point — on those targets, `dot_avx2`/`dot_neon` already fold
multiply-accumulate into their reduction loop.

### 3.4 Quantization

[`mielin-tensor/src/quant.rs`](../mielin-tensor/src/quant.rs) implements two
real precision levels — **INT8** and **INT4** (not INT16):

- **`QuantizedTensor`** (INT8, `i8` storage) — created via
  `QuantizedTensor::from_tensor(tensor, scheme, granularity)`.
  - `QuantScheme::Symmetric` — zero-point fixed at `0`,
    `scale = max(|min|, |max|) / 127.0`. Cheapest to dequantize, best for
    weight tensors with roughly zero-centered distributions.
  - `QuantScheme::Asymmetric` — arbitrary zero-point,
    `scale = (max - min) / 255.0`. Better range utilization for skewed
    distributions (e.g. post-ReLU activations), at the cost of one extra
    subtraction per dequantize.
  - `QuantGranularity::PerTensor` — one `(scale, zero_point)` pair for the
    whole tensor; fastest, coarsest.
  - `QuantGranularity::PerChannel` — one pair per output channel (axis 0);
    better accuracy for Conv2D/Linear weights with per-channel dynamic
    range, at the cost of `num_channels` extra scale/zero-point pairs
    stored in `PerChannelParams`.
- **`Quant4Tensor`** (INT4, packed 2 values/byte, range `-8..7`) — the
  aggressive-compression option: roughly half the memory of INT8 for a
  further accuracy hit, useful on the most memory-constrained NPU/edge
  targets. See `Quant4Tensor::from_tensor` in `quant.rs`.

Trade-off summary (reusing the benefits already documented in `quant.rs`'s
module doc): **2–8× model-size reduction**, **2–4× inference speedup on
compatible hardware**, lower power draw — at a monotonically increasing
accuracy cost as you go `f32 → INT8(per-channel) → INT8(per-tensor) →
INT4`. Start with per-channel INT8 for weights and re-evaluate accuracy
before dropping to INT4.

### 3.5 Parallelism threshold

[`mielin-tensor/src/parallel.rs`](../mielin-tensor/src/parallel.rs) exposes
a global, runtime-adjustable threshold below which parallel ops fall back to
sequential execution (thread-spawn overhead is not worth it for small
tensors):

```rust
pub struct ParallelConfig {
    pub threshold: usize,     // default: 10_000 elements
    pub num_threads: usize,   // default: 0 (auto-detect via num_cpus)
    pub chunk_size: usize,    // default: 1024
}
```

`set_parallel_threshold(n)` / `get_parallel_threshold()` mutate/read a
process-wide `AtomicUsize` (`PARALLEL_THRESHOLD`, default `10_000`);
`should_parallelize(size)` is the gate every `ParallelOps` method checks
before dispatching to rayon. Lower the threshold for workloads dominated by
many mid-sized tensors on a many-core box; raise it (or disable the
`parallel` feature outright) on single-core embedded targets where thread
spawn is pure overhead.

### 3.6 Cache blocking

[`mielin-tensor/src/cache.rs`](../mielin-tensor/src/cache.rs) implements
blocked GEMM/transpose to fit working sets in L1/L2/L3:

```rust
pub struct CacheConfig {
    pub l1_block_size: usize,  // default: 64
    pub l2_block_size: usize,  // default: 256
    pub l3_block_size: usize,  // default: 1024
}
```

`blocked_matmul(&a, &b, &config)` tiles the `(i, j, k)` loop nest by
`l1_block_size` (the loop uses `l1_block_size` as its step for all three
loop nests in the current implementation — see `blocked_matmul` in
`cache.rs`). `CacheConfig::estimate_matmul(m, n, k, block_size)` picks a
recommended block size against the exported `L1_CACHE_SIZE`, `L2_CACHE_SIZE`,
`L3_CACHE_SIZE`, `CACHE_LINE_SIZE` constants. If you know your target's
actual cache sizes (e.g. from `/proc/cpuinfo` or a datasheet), construct a
custom `CacheConfig` rather than relying on the generic defaults — the
default `l1_block_size = 64` assumes a 32–64 KB L1 data cache for `f32`
tiles; a CPU with a smaller L1 (common on some embedded cores) will thrash
if you don't shrink it.

### 3.7 Distributed inference sharding

[`mielin-tensor/src/distributed.rs`](../mielin-tensor/src/distributed.rs)
implements tensor partitioning across mesh nodes (Cannon's-algorithm-based
distributed matmul, per the module doc) via `PartitionStrategy`:

```rust
pub enum PartitionStrategy {
    RowWise,
    ColumnWise,
    Block { rows: usize, cols: usize },
    Pipeline { stages: usize },
}
```

`ShardedTensor::partition(&tensor, strategy, num_shards)` /
`ShardedTensor::reconstruct(shards, strategy)` are the entry points.
Tuning guidance:
- **`RowWise`/`ColumnWise`** — simplest, lowest coordination overhead;
  good default for matmul where one operand's contraction dimension aligns
  with the split axis.
- **`Block { rows, cols }`** — 2-D decomposition; reduces per-shard memory
  further at the cost of more cross-shard communication for reconstruction
  (choose `rows`/`cols` so each block still fits comfortably per-node).
  This is the shape Cannon's algorithm assumes for distributed GEMM.
  - **`Pipeline { stages }`** — splits by layer/token range instead of by
    tensor axis; use for model-parallel inference across `stages` nodes
    when the model itself is too large for one node's memory, independent
    of any single matmul's size.

### 3.8 Tensor memory pool

[`mielin-tensor/src/pool.rs`](../mielin-tensor/src/pool.rs) exports
`TensorPool` / `PooledBuffer` / `PoolStats` and the alignment constant
`SIMD_ALIGNMENT = 64` (bytes) — all pooled allocations are 64-byte aligned
so AVX-512 loads never need an unaligned-load fallback. Prefer
`TensorPool::acquire()`/`release()` over ad-hoc `Vec<f32>` allocation on any
hot path that repeatedly allocates same-sized buffers (see the "Use Memory
Pools" example already in [`PERFORMANCE.md`](./PERFORMANCE.md#use-memory-pools)).

---

## 4. Kernel / Runtime Tuning

### 4.1 Work-stealing scheduler

[`mielin-kernel/src/work_stealing.rs`](../mielin-kernel/src/work_stealing.rs)
implements `WorkStealingScheduler`, a Chase-Lev-deque-based scheduler:

- `MAX_WORKERS = 8` — hard ceiling on worker threads;
  `WorkStealingScheduler::new(num_workers)` clamps its argument to
  `[1, MAX_WORKERS]`.
- `NUM_PRIORITY_BUCKETS = 256` — each worker owns 256 per-priority
  Chase-Lev deques (`priority: u8`, so the full range is addressable
  directly as a bucket index); `schedule()` drains the highest non-empty
  bucket before stealing.
- Victim selection on steal uses a seeded LCG (`Lcg`, Knuth's MMIX
  constants) rather than a real RNG — deterministic given the worker id,
  cheap, `no_std`-friendly, and good enough for load-balancing (not
  security-sensitive).
- `spawn_task(worker_hint, priority)` — if `worker_hint >= num_workers`,
  falls back to `least_loaded_worker()` (placement by current queue depth)
  instead of failing.
- Live metrics via `snapshot_metrics()`: `total_scheduled`, `total_stolen`,
  `total_idle`, `peak_queue_depth` — use `total_stolen / total_scheduled`
  as a load-imbalance signal (a healthy multi-worker deployment under
  uneven load should show non-zero steals; near-zero steals with uneven
  `worker_hint` distribution suggests you should let `spawn_task` pick the
  worker instead of hint-pinning).

Tuning guidance: set `num_workers` to the number of physical cores
available to the kernel image (bounded by `MAX_WORKERS = 8`); use explicit
`worker_hint` only when you have real NUMA/cache-affinity reasons to pin a
task to a specific worker, otherwise pass an out-of-range hint (or the
least-loaded worker's id) to get automatic load balancing.

### 4.2 Kernel configuration presets

[`mielin-kernel/src/config.rs`](../mielin-kernel/src/config.rs) defines
`KernelConfig { memory: MemoryConfig, scheduler: SchedulerConfig, multicore: MultiCoreConfig }`
with a builder (`KernelConfig::builder()...build()`) and four presets:

| Preset | `page_size` | `max_pages` (total) | `max_tasks` | `max_cpus` | notes |
|---|---|---|---|---|---|
| `KernelConfig::default()` | 4 KB | 1024 (4 MB) | 64 | 16 | matches the "Kernel Configuration" example in `PERFORMANCE.md` |
| `KernelConfig::embedded()` | 4 KB | 128 (512 KB) | 16 | 1 (single-core) | `enable_metrics: false`, `enable_per_cpu_pools/load_balancing/work_stealing: false` |
| `KernelConfig::high_performance()` | 4 KB | 8192 (32 MB) | 256 | 32 | all multicore features on |
| `KernelConfig::arm64_16kb()` | 16 KB | 1024 (16 MB) | 64 (default) | 16 (default) | for ARMv8/Apple-Silicon-class 16 KB page targets |

`PageSize` (same file) has four variants: `Size4KB` (default),
`Size8KB`, `Size16KB`, `Size64KB` — MielinOS does **not** implement literal
x86_64-style 2 MB/1 GB hugepages; the closest lever to "huge pages" is
selecting a larger `PageSize` variant (up to 64 KB) via `KernelConfig`,
which reduces page-table/allocator bookkeeping overhead for large
allocations at the cost of internal fragmentation for small ones. Choose
`Size16KB`/`Size64KB` for tensor-heavy or bulk-buffer workloads; keep
`Size4KB` for fine-grained agent/task memory.

`SchedulerConfig::default()` is `{ max_tasks: 64, enable_priority: true,
enable_metrics: true }` — the `TIME_SLICE_US` constant shown in
`PERFORMANCE.md`'s illustrative code sample does not exist as a real field
in `SchedulerConfig` today (the scheduler is cooperative, not
preemptive/time-sliced — see `PERFORMANCE.md`'s own "Preemption: N/A
(cooperative scheduler)" note); treat that constant as illustrative only,
not a real knob.

### 4.3 Buddy allocator

[`mielin-kernel/src/buddy.rs`](../mielin-kernel/src/buddy.rs):
`MIN_ORDER = 12` (4 KiB granularity, matches `PAGE_SIZE`), `MAX_LEVELS = 16`
(largest block = `2^(12+16-1)` = 128 MiB). `BuddyAllocator::new(base, size)`
rounds `size` down to the nearest power of two and clamps to at least
`2^MIN_ORDER`. Runtime tuning is via `BuddyStats`:
`fragmentation_percent()` (`100 * (1 - largest_free_block / free_bytes)`)
and `utilization_percent()` — poll these under load and, if fragmentation
trends high, prefer allocating in power-of-two-friendly sizes (the
allocator coalesces buddies automatically on `deallocate`, but only if the
buddy is also free — interleaved alloc/free patterns of oddly-sized blocks
defeat coalescing).

### 4.4 Fixed-size pool allocator (kernel-level)

[`mielin-kernel/src/pool.rs`](../mielin-kernel/src/pool.rs) — **do not
confuse with** the agent-level `mielin_cells::PoolConfig` (Section 8) or the
tensor-level `TensorPool` (Section 3.8); this is the kernel's O(1)
fixed-block-size allocator. `PoolAllocator` manages 7 lock-free `Pool`s
(free-list + CAS), one per `BlockSize`: `B32`, `B64`, `B128`, `B256`,
`B512`, `K1` (1024 B), `K4` (4096 B). Each `Pool::ARENA_SIZE = 64 * 1024`
(64 KB arena per size class, `7 * 64 KB = 448 KB` total for the global
pool). `BlockSize::for_size(n)` rounds a request up to the smallest class
that fits (requests over 4096 B are rejected — use the buddy allocator or
kernel heap for larger allocations). Tune by choosing which size classes
your workload actually needs; there is currently one fixed 64 KB arena per
class (not runtime-configurable) — if a workload only ever allocates 64 B
and 4 KB blocks, the 128/256/512/1024 B pools' 64 KB each are wasted and
should be accounted for in memory budgeting.

### 4.5 Async timers

[`mielin-kernel/src/async_timer.rs`](../mielin-kernel/src/async_timer.rs):
`AsyncTimerRegistry` stores pending wakers in a `BTreeMap<jiffy, Vec<(Waker,
Arc<AtomicBool>)>>` so `tick()` only visits **expired** entries
(`O(k · log n)` for `k` expired, not `O(n)` for all pending) — this matters
under high timer fan-out (many concurrently-sleeping agent tasks).
`DEFAULT_TICKS_PER_MS = 1` (the kernel assumes a 1000 Hz tick rate by
default; if you configure the timer subsystem at a different rate, use
`sleep_ticks`/`sleep_until_jiffy` directly instead of `sleep_ms_async` for
correct sub-millisecond behavior). `PeriodicTimer` is drift-free by design:
`next_tick_future()` computes each deadline eagerly from the *previous*
deadline plus `interval_ticks`, not from "now" — use it instead of a naive
`loop { sleep(interval).await }` for any periodic task (heartbeats, gossip
ticks) where cumulative drift would matter over long uptimes.

### 4.6 NUMA

[`mielin-kernel/src/numa.rs`](../mielin-kernel/src/numa.rs):
`MAX_NUMA_NODES = 8`, `MAX_CPUS_PER_NODE = 32` (so `MAX_CPUS = 256`),
`LOCAL_DISTANCE = 10`, `REMOTE_DISTANCE = 20`, `UNREACHABLE_DISTANCE = 255`.
Use `numa::allocate_on_node(NodeId, pages)` for node-local allocation and
`numa::set_policy(NumaPolicy::Local)` to pin a task's future allocations
to its current node — on multi-socket hosts running the `high_performance()`
kernel preset (32 cores), NUMA-local allocation avoids the ~2× latency
penalty implied by `REMOTE_DISTANCE`/`LOCAL_DISTANCE` (20 vs. 10, i.e. the
distance matrix models remote access as ~2× local).

### 4.7 Power states (server/desktop kernel)

[`mielin-kernel/src/power.rs`](../mielin-kernel/src/power.rs):
`PowerPolicy::{Performance, Balanced, PowerSaver, Custom}` drives P-state
(frequency/voltage) and C-state (idle depth) selection; `CState` variants
are `C0` (active), `C1` (halt, instant wake), `C2` (~80 µs wake), `C3`
(~200 µs wake), `C6` (deepest, ~300 µs wake). Choose `Performance` for
latency-sensitive mesh/tensor nodes (never enters C-states beyond C1),
`PowerSaver` for idle-heavy control-plane nodes, `Balanced` (DVFS-adaptive)
as the general default. For embedded/battery-powered targets, use
`mielin-rt`'s energy subsystem instead (Section 8) — it is the
purpose-built low-power policy layer, not the server-oriented
`mielin-kernel::power` module.

---

## 5. Mesh / Gossip Tuning

This is **the single most important tuning surface for cluster behavior**.
[`mielin-mesh/core/src/gossip.rs`](../mielin-mesh/core/src/gossip.rs)
implements a SWIM-inspired flat gossip protocol plus an optional
hierarchical (zone-based) variant for larger clusters.

### 5.1 `GossipConfig` (flat gossip)

```rust
pub struct GossipConfig {
    pub gossip_interval: Duration,   // default: 5 s
    pub heartbeat_timeout: Duration, // default: 15 s
    pub failure_timeout: Duration,   // default: 30 s
    pub fanout: usize,               // default: 3
    pub max_history: usize,          // default: 512
}
```

Construct a custom `GossipState` with `GossipState::with_config(node,
config)` instead of `GossipState::new(node)` (which uses
`GossipConfig::default()`).

| Knob | Increase it → | Decrease it → |
|---|---|---|
| `gossip_interval` | Less bandwidth, slower convergence & slower failure propagation | Faster convergence/failure propagation, more bandwidth (every node sends a round every `gossip_interval`) |
| `heartbeat_timeout` | Fewer false-positive "suspect" markings on lossy/high-latency links (WAN), slower real-failure detection | Faster suspect detection on reliable LANs, more false positives on jittery links |
| `failure_timeout` | More tolerance for a "suspect" node to self-heal/refute before being declared `Dead` (safer for flappy networks) | Faster convergence to a firm membership view, higher risk of prematurely evicting a slow-but-alive node |
| `fanout` | Faster epidemic convergence (`O(log_fanout(N))` rounds to reach all `N` nodes) and better resilience to a single unlucky peer selection, but `O(fanout)` more messages sent per node per round | Lower bandwidth per round, slower convergence, higher variance in convergence time |
| `max_history` | Deeper audit trail of membership events (`Joined`/`Left`/`Failed`/`Recovered`/`StatusChanged`/`IncarnationUpdated`) retrievable via `membership_history()`/`history_for()`/`history_since()` | Lower memory (`VecDeque<MembershipEvent>` ring buffer), shorter audit window |

**Concrete presets to consider** (none of these are pre-built constructors
in the code today — build them explicitly with `GossipConfig { .. }`):

- **LAN / low-latency cluster**: shrink `gossip_interval` to `1–2 s` and
  `heartbeat_timeout`/`failure_timeout` proportionally (e.g. `3 s`/`6 s`) —
  the network can sustain more frequent rounds and false positives are rare.
- **WAN / cross-region mesh**: keep or increase `heartbeat_timeout` (e.g.
  `30 s`+) to absorb inter-region RTT jitter; consider a lower `fanout`
  (e.g. `2`) per node if egress bandwidth between regions is metered, and
  compensate for slower convergence with the hierarchical variant below.
- **Large cluster (100s–1000s of nodes)**: flat gossip's per-node fanout
  cost stays the same regardless of cluster size, so convergence rounds
  grow only logarithmically — but total messages-in-flight cluster-wide
  grows linearly with `N * fanout`. At this scale, prefer the hierarchical
  protocol.

The failure-detection state machine itself
(`MemberInfo::should_suspect()` / `should_declare_dead()`) is driven by
the **module-level constants** `GOSSIP_INTERVAL = 5s`, `HEARTBEAT_TIMEOUT =
15s`, `FAILURE_TIMEOUT = 30s` at the top of `gossip.rs` for code paths that
don't go through `GossipConfig` (e.g. `MemberInfo` methods use the
constants directly) — when tuning heartbeat/failure timeouts, make sure any
custom `GossipConfig` you build is actually threaded through to every
consumer, since a couple of `MemberInfo` helper methods currently reference
the module constants rather than a config instance.

### 5.2 `HierarchicalGossipConfig` (zone-based, for larger meshes)

```rust
pub struct HierarchicalGossipConfig {
    pub num_zones: u64,                  // default: 4
    pub super_peers_per_zone: usize,     // default: 3
    pub intra_zone_interval: Duration,   // default: 2 s
    pub inter_zone_interval: Duration,   // default: 10 s
    pub max_inter_zone_ttl: u8,          // default: 4
    pub fanout: usize,                   // default: 3
}
```

Nodes are auto-assigned to a zone via `ZoneId::from_node_id(node_id,
num_zones)` (hash-based). Regular nodes only gossip within their zone
(`intra_zone_interval`, faster/cheaper); `SuperPeer`/`ZoneLeader` nodes
additionally relay `InterZone` messages between zones
(`inter_zone_interval`, slower/more expensive, bounded by
`max_inter_zone_ttl` hops to prevent infinite relay loops). Election of
super-peers is majority-vote based (`start_election()` /
`record_vote()` / `promote_super_peer()`).

Trade-offs:
- **`num_zones`** — more zones means smaller, cheaper intra-zone gossip
  rounds, but more inter-zone relay hops and coordination overhead; size
  zones to roughly match physical topology (e.g. one zone per datacenter/
  availability zone).
- **`super_peers_per_zone`** — more super-peers improves inter-zone
  fault-tolerance (any one super-peer can relay) at the cost of more
  cross-zone traffic (each super-peer independently gossips inter-zone).
- **`intra_zone_interval` vs. `inter_zone_interval`** — the 2 s/10 s
  default (5:1 ratio) reflects that intra-zone links are assumed cheap/fast
  and inter-zone links expensive/slow; widen the ratio further for
  WAN-separated zones.
- **`max_inter_zone_ttl`** — bounds worst-case propagation hops across
  zones; increase only if `num_zones` grows large enough that the default
  `4` hops can't reach every zone (rare, since inter-zone routing goes via
  super-peers, not zone-to-zone chains).

Use flat `GossipConfig` for single-region or small (≲50-node) clusters;
switch to `HierarchicalGossipConfig` once cross-region/WAN links or cluster
size make per-node full-mesh fanout too costly.

---

## 6. Wire / Transport Tuning

All wire-level knobs live in
[`mielin-mesh/wire/src/`](../mielin-mesh/wire/src/).

### 6.1 Adaptive backoff

[`adaptive_backoff.rs`](../mielin-mesh/wire/src/adaptive_backoff.rs) —
`AdaptiveBackoff` wraps a `BackoffStrategy`:

- `Fixed { delay }` — constant delay every attempt.
- `Linear { initial, step, max }` — `min(initial + step*attempt, max)`.
- `Exponential { initial, multiplier, max }` — `min(initial * multiplier^attempt, max)`.
- `ExponentialJitter { initial, multiplier, max, jitter_factor }` —
  exponential plus additive jitter in `[0, base*jitter_factor]`, sourced
  from a **splitmix64** hash of a caller-supplied seed (good avalanche —
  nearby seeds like adjacent timestamps still produce well-separated
  jitter, which is exactly what prevents thundering-herd reconnect storms
  after a shared outage).
- `Fibonacci { unit, max }` — delay follows the Fibonacci sequence scaled
  by `unit`; grows more gently than exponential in the early attempts while
  still bounding total wait via `max`.

Factory helpers: `AdaptiveBackoff::exponential(initial_ms, max_ms,
max_attempts)` and `AdaptiveBackoff::linear(initial_ms, step_ms, max_ms,
max_attempts)`.

### 6.2 Retry policy & circuit breaker

[`retry.rs`](../mielin-mesh/wire/src/retry.rs) — `RetryPolicy` presets:

| Preset | strategy | base_delay | max_delay | max_attempts | jitter |
|---|---|---|---|---|---|
| `RetryPolicy::default()` / `production()` | Exponential | 100 ms | 30 s | 5 | on, factor 0.3 |
| `RetryPolicy::aggressive()` | Exponential | 50 ms | 5 s | 10 | on, factor 0.3 |
| `RetryPolicy::conservative()` | Linear | 1 s | 60 s | 3 | on, factor 0.2 |

Use `aggressive()` for low-latency intra-cluster RPCs where a transient
failure should be retried fast and often; `conservative()` for external/
rate-limited endpoints (e.g. ACME/Let's Encrypt calls in
[`certs/acme.rs`](../mielin-mesh/wire/src/certs/acme.rs)) where hammering a
remote service is actively harmful.

`CircuitBreakerConfig::default()`: `failure_threshold: 5`,
`success_threshold: 2`, `timeout: 60 s`, `metrics_window: 60 s`. Wrap a
`RetryExecutor` with `.with_circuit_breaker(breaker)` to stop issuing
retries entirely once a peer has failed `failure_threshold` times in a row,
until `timeout` elapses and a half-open probe succeeds `success_threshold`
times. Lower `failure_threshold` for fail-fast behavior against a known-
flaky dependency; raise `timeout` if the peer typically needs longer than
60 s to recover (e.g. a process restart) to avoid needless half-open probe
storms.

### 6.3 Priority queue

[`priority.rs`](../mielin-mesh/wire/src/priority.rs) — 4 levels
(`Critical=0 > High=1 > Normal=2 (default) > Low=3`), auto-detected per
message type by `PriorityQueue::detect_priority` (agent migrations with
`priority >= 8` → `Critical`; failed `MigrationAck` → `Critical`;
`Discovery`/protocol-negotiation → `High`; `LoadInfo`/`Ping`/`Pong` →
`Low`). `QueueConfig` presets:

| Preset | `max_size` | `max_per_priority` [C,H,N,L] | `max_age_ms` | `fair_scheduling` |
|---|---|---|---|---|
| `default()` | 10 000 | [1000, 2000, 4000, 3000] | 30 000 | true |
| `high_throughput()` | 50 000 | [5000, 10000, 20000, 15000] | 60 000 | true |
| `low_latency()` | 1 000 | [200, 300, 300, 200] | 5 000 | false |
| `embedded()` | 100 | [25, 25, 30, 20] | 10 000 | false |

`fair_scheduling: true` uses per-priority FIFO round-robin (prevents a
Critical-message flood from fully starving Normal/Low traffic within its
own tier); `fair_scheduling: false` uses a strict binary-heap (always
dequeue globally-highest priority first, lowest overhead, but a sustained
Critical-priority burst can starve everything below it). Choose
`low_latency()` (heap-based, tight capacity, aggressive 5 s expiry) for
control-plane traffic where staleness is worse than drops; `high_throughput()`
for bulk data-plane traffic where you'd rather queue than drop.

### 6.4 Batching

[`batch.rs`](../mielin-mesh/wire/src/batch.rs) — `BatchConfig` presets:

| Preset | `max_messages` | `max_size_bytes` | `max_wait_time` | `batch_critical` | `flush_threshold` |
|---|---|---|---|---|---|
| `default()` | 100 | 1 000 000 (1 MB) | 10 ms | false | 0.8 |
| `high_throughput()` | 500 | 5 000 000 (5 MB) | 50 ms | **true** | 0.9 |
| `low_latency()` | 20 | 100 000 (100 KB) | 1 ms | false | 0.5 |
| `embedded()` | 10 | 10 000 (10 KB) | 5 ms | false | 0.7 |

`batch_critical: false` (the default in every preset except
`high_throughput()`) means `Critical`-priority messages (agent migrations,
error acks) bypass batching and are flushed immediately —
`high_throughput()` is the one preset that batches even critical traffic,
trading migration latency for maximum throughput; do not use it for
latency-sensitive migration paths. `flush_threshold` (fraction of
`max_size_bytes`) triggers an early flush before `max_wait_time` elapses
once the batch is that full — lower it (`low_latency()`'s `0.5`) to flush
sooner under load.

### 6.5 Flow control

[`flow.rs`](../mielin-mesh/wire/src/flow.rs) — three independent layers
combined in `FlowController`:

- **Token bucket** (`TokenBucketConfig`): `default()` = 1 MB burst
  capacity, 10 MB/s refill; `high_throughput()` = 10 MB burst, 100 MB/s;
  `low_latency()` = 100 KB burst, 1 MB/s; `embedded()` = 10 KB burst,
  100 KB/s. This is the hard rate cap per connection.
- **Backpressure** (`BackpressureLevel::{None, Light, Moderate, Heavy,
  Critical}`), driven by queue fill ratio via
  `BackpressureLevel::from_fill_ratio`: `<0.5→None`, `<0.7→Light`,
  `<0.85→Moderate`, `<0.95→Heavy`, `else→Critical`. Rate multipliers are
  `1.0 / 0.8 / 0.5 / 0.2 / 0.0` respectively (`Critical` fully pauses
  sending — `BackpressureController::is_paused()`).
- **Congestion control** (`CongestionConfig`, AIMD by default —
  `CongestionAlgorithm::{None, Aimd(default), Cubic, Bbr}`, though `Cubic`
  and `Bbr` currently fall back to the AIMD implementation internally):
  `initial_window: 64 000` B, `min_window: 4 000` B, `max_window:
  16 000 000` B, `aimd_increase: 16 000` B/RTT, `aimd_decrease: 0.5`
  (halve on loss), `rtt_alpha: 0.125` (EWMA smoothing factor, matching
  TCP's classic SRTT smoothing constant).

Tune the token bucket to your link's actual sustained bandwidth (not burst)
so it doesn't become the binding constraint before congestion control
reacts; tune `aimd_decrease` closer to `1.0` only if you have evidence
losses are spurious (e.g. flaky embedded radio) rather than true congestion
— the default `0.5` is the standard conservative AIMD choice.

### 6.6 Compression selection (LZ4 vs Zstd, via `oxiarc`)

[`compression.rs`](../mielin-mesh/wire/src/compression.rs) — `Compressor`
default: `algorithm: Lz4`, `level: Default`, `min_size: 1024` bytes,
`auto_select: true`, backed by `oxiarc-lz4` and `oxiarc-zstd` (see
workspace `Cargo.toml`: `oxiarc-lz4 = "0.3.5"`, `oxiarc-zstd = "0.3.5"`).

`select_algorithm(data)` (used when `auto_select: true`) is entropy-driven:

```
entropy = estimate_entropy(data)   // sampled, 0.0 (uniform byte) .. 1.0 (random)
if entropy > 0.95        → CompressionAlgorithm::None   // already compressed/encrypted
else if len > 64 * 1024   → CompressionAlgorithm::Zstd   // large payload, favor ratio
else                      → CompressionAlgorithm::Lz4    // default, favor speed
```

`CompressionLevel` maps to zstd levels: `Fast → 1`, `Default → 3`,
`Best → 9` (LZ4 has no level knob in this wrapper). Messages under
`min_size` (1024 B default) skip compression entirely — the framing/CPU
overhead isn't worth it for small control messages (this mirrors the
`compression_threshold: 1024` example already in
[`PERFORMANCE.md`](./PERFORMANCE.md#compression)). Both compressors also
verify the compressed output is actually smaller than the input and fall
back to `CompressionAlgorithm::None` otherwise (protects against
incompressible/already-compressed payloads bloating on the wire).

Tuning guidance: keep `auto_select: true` for mixed traffic (the entropy
probe is cheap — a 4096-byte sample, not a full-data scan); force
`Compressor::with_algorithm(CompressionAlgorithm::Zstd).level(Best)` only
for known-large, known-compressible, latency-insensitive payloads (e.g.
bulk migration snapshots you're willing to spend extra CPU on for network
savings) — see Section 7 for the migration-specific compressor, which is a
separate, independently-tuned code path.

---

## 7. Migration Tuning

Migration compression is implemented **twice** in this codebase at
different layers — be aware of both when tuning:

1. **Wire-level** (`mielin-mesh-wire::compression`, Section 6.6) — generic
   message compression, entropy-driven LZ4/Zstd selection.
2. **Agent-state-level** (`mielin-cells::migration`, this section) — a
   dedicated 4-way adaptive compressor plus delta/dirty-page snapshotting,
   specifically for `MigrationSnapshot`/`DeltaSnapshot` payloads.

### 7.1 `adaptive_compress` (agent snapshot compression)

[`mielin-cells/src/migration/functions.rs`](../mielin-cells/src/migration/functions.rs)
— `CompressionMethod::{Rle, Lz4, ZstdDefault, ZstdHigh}` (`ZSTD_DEFAULT_LEVEL
= 3`, `ZSTD_HIGH_LEVEL = 10`). `adaptive_compress(data)` picks a method by
**size tier**, trying successively more (and more expensive) candidates
only as size grows:

| Data size | Candidates tried | Rationale |
|---|---|---|
| `< 256` B | RLE only | Below this, trying LZ4/Zstd costs more CPU than it could ever save |
| `256 B .. 4096` B | RLE, LZ4 | Zstd's fixed overhead isn't worth it yet |
| `>= 4096` B | RLE, LZ4, Zstd(level 3) | Full 3-way comparison, smallest wins |

`adaptive_compress_aggressive(data)` always tries all **four** methods
including `ZstdHigh` (level 10) regardless of size — use it when
compression ratio matters more than compression *time* (e.g. archiving a
migration snapshot to cold storage, not a live hot-path migration). Both
functions pick the method whose *compressed* output is smallest, not a
fixed policy — RLE frequently wins for sparse/zero-heavy agent memory
(common right after allocation), even though it's the "simplest" method,
because it has near-zero framing overhead on already-compressible data.

### 7.2 Delta snapshots (dirty-page tracking)

[`mielin-cells/src/migration/types/delta.rs`](../mielin-cells/src/migration/types/delta.rs) —
`PAGE_SIZE = 4096` (matches the kernel's page size, Section 4.2's
`PageSize::Size4KB`). `DeltaSnapshot::create(agent_id, old_state, new_state,
base_sequence, sequence)` walks both states in 4 KiB pages and only
includes pages that actually differ (`DirtyPage { index, data, compressed }`);
each dirty page is independently RLE-compressed via `DirtyPage::compress()`
(only replacing `data` if the compressed form is smaller — same "verify it
helped" pattern as the wire compressor).

Tuning implications:
- **Delta vs. full snapshot** — delta transmission cost is proportional to
  the **change rate**, not total agent state size; for an agent with large
  but slowly-mutating state (e.g. a big embedding table updated rarely),
  deltas are dramatically cheaper than the full-snapshot latencies in
  [`PERFORMANCE.md`](./PERFORMANCE.md#snapshot-operations) (e.g. ~300 µs
  to capture a 100 KB snapshot). For state that mutates almost entirely
  every step, a delta degenerates to (at best) the full-snapshot cost plus
  per-page bookkeeping overhead — prefer a full `adaptive_compress` snapshot
  in that regime instead.
- **Page granularity is fixed at 4096 B** (`PAGE_SIZE` constant, not
  currently a runtime parameter) — a single dirty byte anywhere in a page
  still transmits the whole 4 KiB page. Agent memory layouts that cluster
  frequently-mutated fields into as few pages as possible will see
  proportionally smaller deltas.
- Combine with Section 7.1: `DirtyPage::compress()` always uses RLE, not
  the full adaptive method — if your dirty pages are large, sparse, or
  benefit from LZ4/Zstd, compressing the reconstructed delta payload with
  `adaptive_compress` before it hits the wire layer (Section 6.6) captures
  further savings the per-page RLE pass leaves on the table.

Reuse the existing migration latency/throughput figures from
[`PERFORMANCE.md`](./PERFORMANCE.md#migration-performance) (e.g. 1 KB
snapshot capture ~10 µs, 100 KB ~300 µs; network transfer 1 KB ~100 µs,
100 KB ~3 ms, 1 MB ~25 ms) as your baseline when evaluating whether a given
compression/delta strategy is paying for itself — always benchmark with
`cargo bench -p benches --bench migration_performance` after changing
these knobs rather than assuming a smaller payload is automatically an
overall win (compression/diffing CPU cost can dominate for small or
already-dense state).

---

## 8. Memory / Footprint Tuning (Embedded)

For Cortex-M/RISC-V/IoT targets, the relevant tuning surface is
[`mielin-rt/`](../mielin-rt/src/), not the server-oriented
`mielin-kernel::power` module from Section 4.7.

### 8.1 Energy policy

[`mielin-rt/src/energy.rs`](../mielin-rt/src/energy.rs) —
`EnergyPolicy { mode: EnergyMode, max_power_mw: u32, idle_threshold_ms: u64,
wakeup_latency_us: u64 }` presets:

| Preset | `max_power_mw` | `idle_threshold_ms` | `wakeup_latency_us` |
|---|---|---|---|
| `EnergyPolicy::performance()` | `u32::MAX` (uncapped) | `u64::MAX` (never suggest sleep) | 0 |
| `EnergyPolicy::balanced()` | 500 | 50 | 500 |
| `EnergyPolicy::power_save()` | 100 | 5 | 10 000 |
| `EnergyPolicy::budget(total_mj, window_ms)` | computed: `total_mj * 1000 / window_ms` | 20 | 2 000 |

`SleepRecommendation` has four depths: `StayAwake`, `LightSleep` (WFI/C1,
fast wake), `DeepSleep` (STOP/C6, clocks off, RAM retained), `Hibernate`
(STANDBY, lowest power, RAM may be lost) — `EnergyPolicy::recommend_sleep`
maps idle duration and wakeup-latency budget to one of these.

[`mielin-rt/src/energy_scheduler.rs`](../mielin-rt/src/energy_scheduler.rs)'s
`EnergyAwareScheduler::pre_schedule(task, now_us)` is the actual admission-
control hook: it checks (in order) the hard `max_power_mw` cap against the
live `PowerDomainTracker::total_power_mw()` reading, then the per-task
`EnergyBudget` (via `EnergyProfiler`), returning an `EnergySchedulingHint {
allow_run, suggested_throttle_percent, sleep_recommendation, reason }`.
Set per-task budgets with `EnergyBudget::millijoules(mj)` /
`::microjoules(uj)` and `profiler.set_budget(task, budget)` — the default
`BudgetAction` on exceeding a budget is `Throttle` (see
`EnergyBudget::millijoules` in `energy.rs`).

Tuning guidance: use `performance()` only when mains-powered or during
active charging; `balanced()` as the default for battery-powered devices
doing periodic work; `power_save()` for deep-sleep-dominated sensor nodes
that wake briefly to sample/transmit; `budget(total_mj, window_ms)` when
you have a hard energy allowance per reporting interval (e.g. a solar-
powered node with a known daily energy harvest) and want the scheduler to
auto-derive a power cap from it.

### 8.2 Fixed-block pool sizing (`mielin-rt::pool`)

[`mielin-rt/src/pool/types.rs`](../mielin-rt/src/pool/types.rs) /
[`functions.rs`](../mielin-rt/src/pool/functions.rs) — seven size classes
`POOL_SIZES = [16, 32, 64, 128, 256, 512, 1024]` bytes. `PoolConfig` presets
(`blocks_per_pool: [usize; 7]`, indices matching `POOL_SIZES` order):

| Preset | blocks per pool `[16,32,64,128,256,512,1024]` | `track_statistics` | `track_fragmentation` | `fragmentation_threshold` |
|---|---|---|---|---|
| `minimal()` | `[16,16,8,4,2,1,1]` | false | false | 75% |
| `standard()` | `[32,32,32,16,8,4,2]` | true | true | 50% |
| `generous()` | `[128,128,64,32,16,8,4]` | true | true | 30% |
| `tiny()` (< 32 KB RAM MCUs) | `[8,8,4,2,1,0,0]` | false | false | 80% |
| `ultra_low_power()` | `[12,12,6,3,2,1,0]` | false | false | 75% |

Also: `PoolConfig::custom([usize; 7])`, per-class builder methods
(`with_16b_blocks`, `with_32b_blocks`, ... `with_1kb_blocks`), and two
programmatic sizing helpers — `scale_by(factor)` (proportionally scales
every class) and `limit_to_bytes(max_bytes)` (auto-scales down to fit a
hard memory budget, computed via `total_memory()` against `POOL_SIZES`).
Note `tiny()` and `ultra_low_power()` both zero out the largest 1–2 size
classes entirely (`0` blocks for `512`/`1024` B) — any allocation request
in a zeroed class simply fails on those presets, so pick a preset whose
non-zero classes actually cover your largest expected allocation, or start
from `custom()`/`standard()` and call `with_1kb_blocks(n)` explicitly.

`track_statistics`/`track_fragmentation` cost a small amount of per-alloc
bookkeeping — the `tiny()`/`ultra_low_power()`/`minimal()` presets disable
both to shave cycles and RAM on the most constrained targets, at the cost
of losing visibility into fragmentation until you switch to a
tracking-enabled preset for diagnosis.

### 8.3 Platform feature flags

[`mielin-rt/Cargo.toml`](../mielin-rt/Cargo.toml) `[features]`:
`stm32f4`, `nrf52`, `rp2040`, `esp32` — each gates platform-specific GPIO
implementations. Enable exactly one for a given firmware image (they are
alternative HAL backends, not additive); this, combined with the
`release-embedded` build profile (Section 2) and a `tiny()`/
`ultra_low_power()` pool preset, is the standard "smallest possible
firmware image" configuration for MCU targets.

---

## 9. Tuning Checklist & Decision Table

### Quick decision table

| I want to... | Change this |
|---|---|
| Ship a smaller binary/image | Use the default `release` profile (already `opt-level="z"`, `lto=true`, `strip=true`); for MCU targets also use `release-embedded` + a small `mielin-rt::pool::PoolConfig` preset (`tiny`/`ultra_low_power`) |
| Maximize raw CPU throughput | Build with `--profile release-speed`; ensure the target ISA's SIMD feature (AVX2/AVX-512/NEON/SVE2) is compiled in (`RUSTFLAGS`/`target-cpu`) so `mielin-tensor` picks the fast backend automatically |
| Shrink a model for edge/NPU deployment | `mielin_tensor::quant`: start with `QuantizedTensor` (INT8, `PerChannel`), drop to `Quant4Tensor` (INT4) only if the extra 2× shrink is worth the accuracy loss |
| Reduce mesh bandwidth | Increase `GossipConfig::gossip_interval` and/or decrease `fanout`; switch to `HierarchicalGossipConfig` for large/WAN clusters |
| Detect node failures faster | Decrease `GossipConfig::heartbeat_timeout`/`failure_timeout` (LAN only — risks false positives on lossy links) |
| Reduce false-positive node failures on a jittery network | Increase `heartbeat_timeout`/`failure_timeout` |
| Cut network overhead for many small messages | Tune `BatchConfig` (`high_throughput()` for bulk data, `low_latency()` for control-plane) |
| Prioritize migrations/critical traffic over background noise | Rely on `PriorityQueue`'s auto-detected `Critical` tier; keep `batch_critical: false` (default) so critical messages bypass batching |
| Trade CPU for network bytes (or vice versa) | `mielin-mesh-wire::compression::Compressor` (`auto_select` for mixed traffic, or force `Zstd` + `Best` for large/compressible/latency-insensitive payloads) |
| Speed up agent migration of large, slowly-changing state | Use `DeltaSnapshot`/dirty-page tracking (`mielin-cells::migration`) instead of full-state `adaptive_compress` |
| Minimize CPU spent compressing tiny/frequent snapshots | Rely on `adaptive_compress`'s size-tiered candidate selection (it already skips Zstd below 4 KiB and skips everything but RLE below 256 B) |
| Extend battery life on an embedded node | `mielin_rt::energy::EnergyPolicy::power_save()` or a custom `budget()`; pair with a `tiny()`/`ultra_low_power()` `PoolConfig` |
| Debug a release-only bug | Build with `--profile release-debug` (same optimization, symbols retained) |
| Avoid thread-pool overhead on a single-core embedded target | Don't enable `mielin-tensor`'s `parallel` feature; raise `ParallelConfig::threshold` if it is enabled but the workload is mostly small tensors |
| Reduce fragmentation in the kernel physical allocator | Monitor `BuddyStats::fragmentation_percent()`; prefer power-of-two allocation sizes; consider a larger `PageSize` (Section 4.2) for bulk allocations |

### Checklist (mirrors, and extends, `PERFORMANCE.md`'s tuning checklist)

**Build**
- [ ] Confirm the build profile matches the goal: size (`release`/`release-embedded`) vs. speed (`release-speed`) vs. debuggability (`release-debug`)
- [ ] For CPU-bound targets, compile with the correct `target-cpu`/`target-feature` so `HardwareCapabilities` detects AVX2/AVX-512/NEON/SVE2 at runtime

**Compute**
- [ ] Only enable the `mielin-tensor` Cargo features your deployment target actually has hardware for (`cuda`/`metal`/NPU features add compiled code even if unused)
- [ ] Quantize (`QuantizedTensor` INT8, or `Quant4Tensor` INT4) if deploying to NPU/edge and accuracy budget allows
- [ ] Set `CacheConfig` block sizes to your actual L1/L2/L3 sizes if they differ materially from the 64/256/1024 defaults
- [ ] Tune or disable `ParallelConfig::threshold` based on core count and typical tensor size

**Kernel**
- [ ] Pick a `KernelConfig` preset (`default`/`embedded`/`high_performance`/`arm64_16kb`) matching the target's core count and memory budget
- [ ] Size `WorkStealingScheduler::new(num_workers)` to physical cores (≤ `MAX_WORKERS = 8`)
- [ ] Watch `BuddyStats`/`PoolStats` for fragmentation/exhaustion under real load

**Mesh**
- [ ] Set `GossipConfig` (interval/timeouts/fanout) to match your network's latency/loss profile (LAN vs. WAN)
- [ ] Switch to `HierarchicalGossipConfig` once cluster size or cross-region links make flat fanout too costly
- [ ] Verify custom `GossipConfig` values are actually threaded through every consumer (some `MemberInfo` helpers reference the module-level constants directly)

**Wire**
- [ ] Choose a `RetryPolicy` preset (`aggressive`/`conservative`/`production`) per remote-dependency class
- [ ] Choose `QueueConfig`/`BatchConfig` presets per traffic class (control-plane vs. bulk data)
- [ ] Leave `Compressor::auto_select` on unless you have a specific, measured reason to force an algorithm/level

**Migration**
- [ ] Use delta/dirty-page snapshots for large, low-churn agent state; full `adaptive_compress` for small or high-churn state
- [ ] Re-benchmark (`cargo bench -p benches --bench migration_performance`) after any compression-strategy change — smaller payload does not always mean lower total latency

**Embedded**
- [ ] Select an `EnergyPolicy` preset matching the power source (mains vs. battery vs. energy-harvesting)
- [ ] Select a `mielin_rt::pool::PoolConfig` preset that covers your largest real allocation (watch for zeroed size classes in `tiny()`/`ultra_low_power()`)
- [ ] Enable exactly one platform feature (`stm32f4`/`nrf52`/`rp2040`/`esp32`)

---

## See also

- [`./PERFORMANCE.md`](./PERFORMANCE.md) — measured benchmark results, design goals, and profiling/monitoring tooling (this guide is the "how to configure" companion to that "what we measured" reference).
- [`./NETWORKING.md`](./NETWORKING.md) — mesh/QUIC networking architecture and protocol details.
- [`./TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) — diagnosing issues that tuning alone won't fix.
- [`../benches/README.md`](../benches/README.md) — benchmark suite reference and baseline-comparison workflow.
