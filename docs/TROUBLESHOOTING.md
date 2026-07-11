# MielinOS Troubleshooting Runbook

A symptom-first operational runbook for the MielinOS workspace: build/toolchain
failures, test isolation, node/cluster startup, mesh gossip and partition
behavior, agent migration failures, TLS/certificate problems, performance
triage, and observability. Every error type, variant, default value, and log
line quoted below exists in the source tree cited next to it — this is not a
theoretical guide, it documents what the code actually does (including its
known rough edges) so you can match a real symptom to a real code path.

This guide is organized **symptom → likely cause → diagnosis → fix**. If
you're not sure where to start, jump to [§9 First Response
Checklist](#9-first-response-checklist).

Primary sources: `mielin-mesh/core/src/{error,recovery,partition,timeout,
shutdown,discovery,discovery_dns,discovery_static,gossip,service}.rs`,
`mielin-mesh/wire/src/{lib,retry,adaptive_backoff,health,transport_fallback,
transport,tcp_transport,websocket,protocol,cert_rotation}.rs`,
`mielin-mesh/wire/src/certs/{mod,ca,mtls,pinning,acme,renewal,storage}.rs`,
`mielin-mesh/wire/src/advanced_tls.rs`, `mielin-cells/src/{lib,migration/types/
recovery,migration/types/validation}.rs`, `mielin-kernel/src/lib.rs`,
`mielin-tensor/src/error.rs`, `mielin-cli/src/{error,cli,main,control/server,
commands/*}.rs`, plus `docs/NETWORKING.md`, `docs/CERTIFICATES.md`, and
`docs/MIGRATION.md`, which this guide cross-references rather than duplicates.

---

## Table of Contents

1. [Build & Toolchain Issues](#1-build--toolchain-issues)
2. [Test Failures & Isolation](#2-test-failures--isolation)
3. [Node/Cluster Startup & Connectivity](#3-nodecluster-startup--connectivity)
4. [Mesh/Gossip Issues](#4-meshgossip-issues)
5. [Migration Failures](#5-migration-failures)
6. [TLS/Certificate Problems](#6-tlscertificate-problems)
7. [Performance/Resource Issues](#7-performanceresource-issues)
8. [Observability](#8-observability)
9. [First Response Checklist](#9-first-response-checklist)

---

## 1. Build & Toolchain Issues

### 1.1 Toolchain baseline

`rust-toolchain.toml` pins the workspace to:

```toml
[toolchain]
channel = "nightly"
components = ["rustfmt", "clippy", "llvm-tools"]
profile = "minimal"
```

`rustup` auto-installs this toolchain (and the listed components) the first
time you run any `cargo`/`rustc` command inside the repo — you do not need to
`rustup default nightly` globally, but you do need `rustup` itself present.
`llvm-tools` ships `rust-lld`, which several targets below rely on.

### 1.2 The workspace does not build "everything" by default

Root `Cargo.toml` excludes `mielin-kernel` and the WASM-only example crates
from `default-members` (comment: *"Default members excludes kernel (requires
bootloader) and counter-agent (WASM-only)"*). Concretely:

| Symptom | Cause | Fix |
|---|---|---|
| `cargo build` / `cargo test` silently never touches `mielin-kernel` or `counter-agent` | They're excluded from `default-members` | Target them explicitly: `cargo build -p mielin-kernel ...`, or use `--workspace` to force every member (kernel still needs its own target, see §1.5) |
| `cargo build --workspace` fails on `mielin-kernel` even though you didn't ask for it | `--workspace` overrides `default-members` and tries to build **every** member with the host target | Build the kernel crate with its dedicated bare-metal target instead: `cargo build -p mielin-kernel --target x86_64-unknown-none`, and use `--workspace --exclude mielin-kernel` for everything else |

### 1.3 `error: could not compile mielin-kernel`

Quickstart's short answer (`cargo clean && cargo build`) works because it
clears a stale/mismatched build graph, but the more common root causes are:

- **Wrong build mode.** `mielin-kernel` has a `bootable` feature gating the
  bootloader integration. Per `mielin-kernel/docs/TROUBLESHOOTING.md`:
  ```bash
  # std-target, no bootloader — for unit tests / non-bare-metal analysis
  cargo build -p mielin-kernel --lib --no-default-features
  # bare-metal, bootable image
  rustup default nightly
  cargo build -p mielin-kernel --features bootable --target x86_64-unknown-none
  ```
- **Missing `rust-src`.** Bare-metal builds using `core`/`alloc` without the
  host `std` need the `rust-src` component (RUNNING.md): `rustup component add
  rust-src`.

For anything below the workspace crate boundary (memory/scheduler/interrupt
internals, panics inside QEMU, etc.), see `mielin-kernel/docs/
TROUBLESHOOTING.md` — this guide stays at the workspace/mesh/agent level.

### 1.4 `x86_64-unknown-none` / bootimage issues

| Symptom | Cause | Fix |
|---|---|---|
| `error: target 'x86_64-unknown-none' not found` | The custom target spec (`x86_64-unknown-none.json`) lives at the **repo root**; `rustc`/`cargo` only discovers it relative to the invocation directory | Run cargo from `/notebooks/mielin` (the repo root), not a subdirectory |
| `bootimage: command not found` | The `bootimage` cargo subcommand isn't installed | `cargo install bootimage` |
| `cargo run`/`cargo bootimage` can't find a runner | `.cargo/config.toml`'s `[target.x86_64-unknown-none]` sets `runner = "bootimage runner"`, which requires `bootimage` on `PATH` | Same fix as above |
| Miri fails after uncommenting `build-std` | `.cargo/config.toml` intentionally keeps the `[unstable] build-std = ["core", "compiler_builtins", "alloc"]` block **commented out** "for Miri compatibility" | Don't commit it uncommented; enable `build-std` only in a local, throwaway config when you specifically need a true `no_std` core/alloc rebuild |

Full QEMU boot/run/debug workflow (GDB, serial output, real-hardware USB
imaging) is in `docs/RUNNING.md`.

### 1.5 WASM build issues (`wasm32-unknown-unknown`)

Quickstart's fix still applies and is what `scripts/verify.sh` itself does:

```bash
rustup target add wasm32-unknown-unknown
cargo clean
cargo build -p counter-agent --target wasm32-unknown-unknown --release
```

| Symptom | Cause | Fix |
|---|---|---|
| `error: linking with rust-lld failed` | Target not installed, or a stale `target/` from a different toolchain | `rustup target add wasm32-unknown-unknown`; if `rust-lld` itself is missing, `rustup component add llvm-tools` (it should already be present per §1.1) |
| Build succeeds but the crate you meant to build for WASM isn't the one in `target/wasm32-unknown-unknown/...` | `cargo build --target wasm32-unknown-unknown` without `-p` tries every workspace member for that target, most of which aren't WASM-compatible | Always pass `-p <wasm-crate>` (e.g. `-p counter-agent`) |
| `WasmError::CompilationFailed`/`InvalidModule` at **runtime** (not build time), from `mielin-wasm` | The `.wasm` binary itself is malformed or exceeds validation limits (see `mielin-wasm/src/validation.rs::ValidationError`) | This is a `mielin-wasm` execution-time error, not a `rustc`/`cargo` failure — inspect the module with `wasm-objdump -x <file>.wasm` before assuming the toolchain is at fault |

### 1.6 Native build-tool failures (`ring`, cc/linker)

The mesh wire crate's TLS stack is intentionally pure-Rust: `rustls` is built
with `default-features = false`, and `oxiquic-crypto` supplies the
`rustls::crypto::CryptoProvider` (no `ring`/`aws-lc-rs` default provider is
compiled in — see [§6.1](#61-crypto-stack-and-trust-model)). However, the
workspace `Cargo.toml` notes `ring` can still show up as a **transitive**
dependency through other paths, and `ring`'s build script needs a C toolchain.

| Symptom | Cause | Fix |
|---|---|---|
| `error: failed to run custom build command for 'ring'` | No C compiler/assembler available for `ring`'s `build.rs` | Linux: `apt install build-essential`; macOS: `xcode-select --install` |
| `error: linker 'cc' not found` while cross-compiling | Missing target-specific cross linker | See §1.7 |

### 1.7 Cross-compilation

From `docs/DEVELOPER.md`:

```bash
# AArch64
rustup target add aarch64-unknown-linux-gnu
sudo apt-get install gcc-aarch64-linux-gnu
cargo build --target aarch64-unknown-linux-gnu

# RISC-V 64-bit
rustup target add riscv64gc-unknown-linux-gnu
cargo build --target riscv64gc-unknown-linux-gnu
```

`error: linking with 'cc' failed` / `linker 'aarch64-linux-gnu-gcc' not found`
on these targets means the cross toolchain package above isn't installed —
this is a system package problem, not a Cargo.toml problem.

### 1.8 Workspace/dependency drift

Per the workspace policy every crate uses `*.workspace = true` — versions are
pinned once, in the root `Cargo.toml` (`tokio = "1.52"`, `rustls = "0.23.41"`,
`axum = "0.8.9"`, `anyhow = "1.0.103"`, `thiserror = "2.0.18"`, etc.). If a
crate-local `Cargo.lock`/build cache disagrees with the root lock file:

```bash
cargo update              # re-resolve within the pinned version constraints
cargo clean && cargo build --workspace
```

`make clean` goes further (destructive — only when the above doesn't resolve
it): it removes `Cargo.lock` entirely, deletes example-crate lockfiles, and
`rm -rf`s every `target/` directory under `examples/`.

### 1.9 `make`/`scripts/verify.sh` reference

| Command | What it does |
|---|---|
| `make build` / `make release` | `cargo build --workspace` / `cargo build --release --workspace` |
| `make test` | `cargo nextest run --workspace` |
| `make check` | `cargo fmt --all -- --check` + `cargo clippy --all-features --workspace -- -D warnings` + `cargo check --all-features --workspace` + `cargo nextest run --all-features --workspace` |
| `make wasm` | `rustup target add wasm32-unknown-unknown` + build `counter-agent` for it |
| `make setup` | Runs `scripts/setup.sh` (installs `cargo-nextest`, `cargo-watch`, adds the wasm target) |
| `make verify` | Runs `scripts/verify.sh` |
| `make clean` | See §1.8 |

`scripts/verify.sh` runs, in order: Rust/Cargo version check → tool presence
(`cargo-nextest` optional, `rustfmt`/`clippy-driver` required) →
`wasm32-unknown-unknown` target installed check → `cargo check --workspace` →
`cargo fmt --all -- --check` → `cargo clippy --workspace -- -D warnings` →
tests (`cargo nextest run --workspace --no-fail-fast`, falling back to `cargo
test --workspace` if nextest isn't installed) → WASM build of `counter-agent`
→ required-doc-file existence check → required-infra-file existence check. On
failure it prints the exact remediation command (`rustup component add
rustfmt clippy`, `cargo install cargo-nextest`, `rustup target add
wasm32-unknown-unknown`) — run it directly rather than guessing which step
failed from a wall of output.

---

## 2. Test Failures & Isolation

### 2.1 Baseline

```bash
cargo nextest run --workspace       # make test — fast, process-per-test
cargo test --workspace              # fallback if cargo-nextest isn't installed
```

### 2.2 Isolate

```bash
cargo nextest run -p mielin-mesh-core                 # one crate
cargo nextest run -p mielin-mesh-core gossip           # substring filter on test name
cargo nextest run -p mielin-mesh-core -- --exact test_circuit_breaker_open_to_half_open
```

### 2.3 See output from a failing/hanging test

```bash
cargo nextest run -p <crate> -- --nocapture   # or: cargo nextest run -p <crate> --no-capture
```

`--nocapture` after `--` is explicitly emulated by nextest as equivalent to
its own `--no-capture` flag (confirmed via `cargo nextest run --help`): it
forces serial execution and disables output capture. Both spellings work;
`--no-capture` before `--` is the more idiomatic nextest form.

### 2.4 Feature-gated tests

```bash
cargo nextest run -p mielin-tensor --features cuda
cargo nextest run -p mielin-tensor --all-features
```

`mielin-tensor` is the only one of the crates this guide focuses on that
defines its own `[features]` (`std`, `parallel`, `cuda`, `metal`,
`apple-neural-engine`, `edge-tpu`, `qualcomm-npu`, `onnx`, `tflite`,
`onnxruntime`, `hailo`, `all`). **`mielin-mesh-wire` and `mielin-cells` define
no `[features]` section at all** — passing `--features <name>` against those
crates will error (unknown feature), not silently no-op. Check the target
crate's `Cargo.toml` before assuming a feature flag applies to it.

### 2.5 Tests that are *supposed* to be skipped

Several async/`tokio`-based tests (e.g. the circuit-breaker tests in
`mielin-mesh/core/src/error.rs`) are annotated `#[cfg_attr(miri, ignore)]`.
Running `cargo +nightly miri test` and seeing these skipped is expected
behavior, not a failure to chase.

### 2.6 Release-only failures

The workspace defines a `release-debug` profile (inherits `release`, keeps
`debug = true` and `strip = false`) specifically so you can reproduce a
release-only failure (UB masked by opt-level 0, or an assertion only present
under debug assertions) without losing debug info:

```bash
cargo test --profile release-debug
```

### 2.7 No nextest config exists

There is no `nextest.toml` anywhere in the repo and no `[profile.*]` nextest
section in the root `Cargo.toml` — no crate has custom retries, partitioning,
or slow-test thresholds configured. If a test is flaky, there's no
project-level safety net catching it; use nextest's own ad hoc flag
(`--retries N`) rather than assuming a retry policy is already in place.

### 2.8 Doc tests and general guidance

`CONTRIBUTING.md`'s testing guidelines: keep one assertion per test, add
tests for boundary/error-path conditions, and prefer deterministic fixtures.
`cargo test --doc` still runs doc-tests (`cargo nextest` does not execute
doc-tests — it only runs unit/integration tests built as separate binaries);
run plain `cargo test --workspace --doc` if a doc-comment example is what
broke.

---

## 3. Node/Cluster Startup & Connectivity

### 3.1 Starting a real node

The only `mielinctl` path that starts an actual `MeshService` (as opposed to
the CLI's mocked presentation commands, see §3.6) is the `daemon` subcommand
(`mielin-cli/src/commands/daemon.rs`, `mielin-cli/src/cli.rs`):

```bash
mielinctl daemon \
  --listen 0.0.0.0:8080 \          # mesh bind address (default shown)
  --role edge \                    # core | relay | edge (default: edge)
  --bootstrap 127.0.0.1:8080 \     # optional bootstrap peer address
  --control-listen 127.0.0.1:8081  # HTTP control-plane bind address (default shown)
```

Internally this parses `--listen`/`--bootstrap` as `SocketAddr`, builds a
`Node::new(role)`, a `MeshConfig { bind_address, bootstrap_nodes,
enable_mdns: true, enable_gossip: true, enable_registry: true,
enable_migration: true }`, constructs `MeshService::new(node, config)`, calls
`service.start().await`, then serves the HTTP control plane and blocks on
`tokio::select!` between the control server and `Ctrl+C` for graceful
shutdown (`svc.stop().await`).

| Symptom | Cause | Fix |
|---|---|---|
| `Invalid listen address '<value>': <parse error>` | `--listen` isn't a valid `ip:port` | Supply a full socket address, e.g. `0.0.0.0:8080`, not just a port or hostname |
| `Invalid bootstrap address '<value>': <parse error>` | Same, for `--bootstrap` | Same fix; bootstrap must be an IP:port, not a DNS name |

### 3.2 `MeshError` — the top-level startup error surface

`mielin-mesh/core/src/service.rs` defines the orchestration error that wraps
every subsystem's failure as a stringified message:

```rust
pub enum MeshError {
    DiscoveryError(String),   // "Discovery error: {0}"
    GossipError(String),      // "Gossip error: {0}"
    RegistryError(String),    // "Registry error: {0}"
    MigrationError(String),   // "Migration error: {0}"
    ServiceNotStarted,        // "Service not started"
}
```

`MeshService::start()` wraps a failing `DiscoveryService::new()` as
`MeshError::DiscoveryError(e.to_string())` — so a discovery-layer failure
(§3.4) surfaces as e.g. `MeshError::DiscoveryError("mDNS error: Failed to
create mDNS daemon: <os error>")`. **Every mesh accessor** (`get_peers`,
`get_member_stats`, `get_alive_members`, `migrate_agent`, etc.) returns
`Err(MeshError::ServiceNotStarted)` if called before `start()` has populated
the internal discovery/gossip/registry/migration handles — if you see this,
you're calling into a `MeshService` that either hasn't been started yet or
whose `start()` call itself returned an error you didn't check.

### 3.3 "Address already in use" / bind failures

**Important:** `mielin-mesh-core`'s `DiscoveryService` never calls a raw
socket `.bind()` — `MeshConfig::bind_address` (default `0.0.0.0:8080`) is used
only for mDNS service advertisement (`ServiceInfo::new(..., bind_addr.ip(),
bind_addr.port(), ...)`) and logging. Actual socket binding happens one layer
down, in `mielin-mesh-wire`'s transports, and the wrapped messages are exact:

| Transport | Site | Wrapped error |
|---|---|---|
| QUIC | `transport.rs::QuicTransport::new` | `WireError::TransportError(format!("Failed to create server endpoint: {e}"))` |
| TCP | `tcp_transport.rs::TcpTransport::new` | `WireError::TransportError(format!("Failed to bind TCP listener: {}", e))` |
| WebSocket | `websocket.rs::WebSocketTransport::new` | `WireError::TransportError(format!("Failed to bind: {}", e))` |

In each case `{e}` is the underlying `std::io::Error` — on Linux a port
collision renders as `Address already in use (os error 98)` inside that
message. Fix: pick a different `--listen` port, or find/kill the process
holding it: `ss -ltnp | grep 8080` or `lsof -i :8080`.

The **control-plane** listener is separate and unwrapped:
`ControlServer::serve()` does `tokio::net::TcpListener::bind(addr).await?`
with a bare `?` propagated through `anyhow::Result` — a collision on
`--control-listen` (default `127.0.0.1:8081`) surfaces as the raw OS error
text, not a `MeshError`/`WireError` variant. Running two daemons on one host
requires distinct `--control-listen` addresses in addition to distinct
`--listen` addresses.

### 3.4 Discovery not finding peers

Four independent discovery backends exist (`docs/NETWORKING.md` §4) with very
different levels of "actually does network I/O today":

| Backend | Error type / condition | Notes |
|---|---|---|
| mDNS (`discovery.rs`) | `DiscoveryError::MdnsError` — wraps `"Failed to create mDNS daemon: {}"`, `"Failed to create service info: {}"`, `"Failed to register service: {}"`, `"Failed to browse: {}"` | `connect_bootstrap()` and `exchange_peers()` are **documented stubs** that log and return `Ok(Vec::new())` without opening a connection — don't expect mDNS-discovered bootstrap peers to be dialed automatically by this code path yet |
| Static list (`discovery_static.rs`) | `StaticDiscoveryError::{PeerNotFound, PeerAlreadyExists, NoHealthyPeers}` — `"No healthy peers available"` when every configured peer has been pruned | Peers are pruned once `failure_count` exceeds `max_failures_before_remove` (default **5**) |
| DNS SRV (`discovery_dns.rs`) | `DnsDiscoveryError::{LookupFailed, NoRecords{service}, CacheExpired, InvalidRecord{reason}}` | `do_refresh()` is a **mock**: with no real resolver wired in, it either re-stamps an existing (possibly stale) cache entry or returns `NoRecords` if the cache is empty. No real DNS query happens in this file today; only `inject_records()` (test-only) populates the cache |
| Aggregator (`discovery_aggregator.rs`) | merges Static + DNS results, deduped by `NodeId` | Despite a `DiscoverySource::Mdns` variant existing, **mDNS is not wired into the aggregator** — its module doc marks that "in future" |

Diagnosis: `mielinctl mesh peers --daemon http://127.0.0.1:8081` (or set
`MIELIN_DAEMON`) hits the live control plane's `/api/v1/mesh/peers`; without
`--daemon`/`MIELIN_DAEMON` it prints **mock data**, which will look identical
to a healthy cluster and tell you nothing (see §3.6).

### 3.5 Bootstrap / multi-node cluster testing

For local multi-node testing, `docs/CLUSTER_TESTING.md` documents a 3-node
Docker Compose cluster (`./scripts/cluster.sh`) — Core (`:8080`, bootstrap),
Relay (`:8081`, connects to Core), Edge (`:8082`, connects to Relay, creates
an agent):

```bash
./scripts/cluster.sh build && ./scripts/cluster.sh start
./scripts/cluster.sh status                 # health-check summary
./scripts/cluster.sh logs [node]            # follow logs
```

| Symptom | Diagnosis | Fix |
|---|---|---|
| Cluster won't start / health checks fail | Insufficient Docker resources, or stale images | `docker info \| grep -E "CPUs\|Total Memory"`; `./scripts/cluster.sh clean && build && start` |
| Nodes can't connect | DNS resolution between containers, or the port never actually bound | `./scripts/cluster.sh exec relay ping -c 3 core`; `./scripts/cluster.sh exec core netstat -tuln \| grep 8080` |
| Migration "fails" in the cluster | Usually a reachability problem, not a migration-logic bug | `./scripts/cluster.sh exec edge nc -zv core 8080`; grep `core`'s logs for `Migration` |

Look for these literal log lines to confirm a node actually got past startup:
`"Mesh service started (gossip + registry + migration)"`, `"Connecting to
peer at"`, `"Created agent with ID"`, `"Registered in mesh registry"`. If none
of these appear in `./scripts/cluster.sh logs <node>`, the process never got
past `MeshService::start()` — go back to §3.2/§3.3.

### 3.6 CLI commands that don't talk to a live daemon (mock-data caveat)

This is worth internalizing before you trust any `mielinctl` output as ground
truth: several subcommands currently return **hardcoded/mock** data
regardless of what's actually running, because they aren't wired to a live
process yet:

- `mielinctl node list/info/start/stop` — `mock_node_list()`, and literal
  strings like `"Node started successfully"` / `"Node stopped successfully"`
  are printed unconditionally; no real node is started or stopped by these
  subcommands.
- `mielinctl migrate status/cancel/history` — `"No migrations in progress"`
  is hardcoded, independent of what the running daemon is actually doing.
- `mielinctl cluster status` — a hardcoded `"Cluster status: Healthy (2
  nodes, 7 agents)"`.

**What is actually wired to a live daemon:** `mielinctl mesh status` /
`mielinctl mesh peers`, but only when given `--daemon <url>` or the
`MIELIN_DAEMON` environment variable — in that case they call `ControlClient`
against the real Axum control plane (§8). The `mielinctl daemon` process
itself is the only subcommand that starts a genuine `MeshService`. If a
`node`/`cluster`/`migrate` command reports success but nothing changed on the
wire, this is why — verify with `mielinctl mesh status --daemon
http://127.0.0.1:8081` or `curl http://127.0.0.1:8081/api/v1/health` instead.

---

## 4. Mesh/Gossip Issues

Source: `mielin-mesh/core/src/gossip.rs`, `partition.rs`, `recovery.rs`,
`timeout.rs`, `shutdown.rs`. See also `docs/NETWORKING.md` §3–§5 for the
full protocol description; this section is diagnosis-focused.

### 4.1 Membership flapping (`Alive` ↔ `Suspect` ↔ `Dead`)

`GossipConfig` defaults:

| Field | Default | Meaning |
|---|---|---|
| `gossip_interval` | 5 s | Heartbeat task tick period |
| `heartbeat_timeout` | 15 s | Age after which an `Alive` member is marked `Suspect` |
| `failure_timeout` | 30 s | Age after which a `Suspect` member is marked `Dead` |
| `fanout` | 3 | Target peers per gossip round |
| `max_history` | 512 | `MembershipEvent` ring-buffer capacity |

**Caveat that explains "my custom timeout didn't take effect":** the
background `spawn_failure_detection_task` correctly reads
`config.heartbeat_timeout`/`config.failure_timeout` — but
`MemberInfo::should_suspect()`/`should_declare_dead()` read the **hard-coded
module constants** `HEARTBEAT_TIMEOUT = 15s`/`FAILURE_TIMEOUT = 30s` directly,
not the runtime `GossipConfig`. If your integration calls
`member.should_suspect()` directly instead of relying purely on the
background task's own state transitions, it evaluates against 15 s/30 s
**regardless of what `GossipConfig` you constructed the `GossipState` with**.
Also note: the failure-detection task's own polling interval is **hard-coded
to 5 s**, not derived from `config.gossip_interval`.

If nodes are flapping near the 15 s boundary under normal load, that's
consistent with real network jitter close to the threshold, not necessarily a
bug — raise `heartbeat_timeout`/`failure_timeout` via a custom `GossipConfig`
passed to `GossipState::with_config`, keeping the caveat above in mind.

`increment_incarnation()` implements SWIM-style self-refutation: a node that
observes itself marked `Suspect` should bump its own incarnation so a
subsequent `Heartbeat`/`MemberUpdate` from it is accepted as newer and clears
the suspicion. If a recovered node stays `Suspect`/`Dead` in peers' views,
confirm it's actually calling this and re-broadcasting.

### 4.2 Failure detection never fires / membership never updates

The gossip-propagation task (distinct from the heartbeat task) is
**documented as not yet wired to real network transmission** in
`mielin-mesh-core` — its body is a placeholder comment (`"In a real
implementation, this would: 1. Select random peers to gossip with... 2. Send
member updates... 3. Exchange state information"`), and the `fanout` value it
captures is literally named `_fanout` (never read). Real state exchange
happens only in reaction to inbound `GossipMessage`s
(`Heartbeat`/`MemberUpdate`/`SyncRequest`/`SyncResponse`/`StateUpdate`) via
`GossipState::handle_message()`. **If nothing is feeding received wire
traffic into `handle_message()`, membership will never move past the single
seeded local `Alive` entry** — verify your wire-layer integration is actually
forwarding gossip frames into this call before assuming the SWIM algorithm
itself is broken.

### 4.3 Partitions & quorum

`PartitionDetector` (`partition.rs`) ticks every `PARTITION_CHECK_INTERVAL` =
**10 s**. Quorum is majority-**plus-one**, not the textbook `⌊n/2⌋+1`:

```rust
let quorum_size = (known_count as f64 * 0.5).ceil() as usize + 1;
let has_quorum = visible_count >= quorum_size;
```

A single-node or empty known-set is always treated as quorate.

State machine:

| Transition | Trigger | Log line |
|---|---|---|
| `Normal → Suspected` | no quorum and `visible_count < known_count` | `debug!("Partition suspected: {} visible out of {} known nodes", ...)` |
| `Suspected → Normal` | quorum regained + full visibility | `debug!("Partition suspicion cleared")` |
| `Suspected → Partitioned` | still no quorum | `warn!("Partition confirmed: {} visible, {} required for quorum", ...)`; fires `PartitionDetected` then `QuorumLost` |
| `Partitioned → Recovering` | quorum regained | `info!("Quorum regained, starting partition recovery")`; fires `RecoveryStarted` |
| `Recovering → Normal` | `visible_count >= known_count` | `info!("Partition recovery complete: all {} nodes visible", ...)`; fires `RecoveryCompleted` then `PartitionResolved` |

`PartitionError` variants: `SplitBrainDetected` (`"Split-brain detected:
multiple partitions claim leadership"`), `NoQuorum(usize, usize)` (`"No
quorum: {0} nodes out of {1} required"`), `RecoveryFailed(String)`
(`"Partition recovery failed: {0}"`), `InvalidState(String)` (`"Invalid
partition state: {0}"` — this is what `force_recovery()` returns if called
while not currently `Partitioned`).

**Two things to know before you build alerting on `PartitionEvent`:**
1. `PartitionEvent::SplitBrainDetected { partitions: Vec<PartitionInfo> }` and
   `QuorumRegained { visible, total }` are declared but **never constructed
   anywhere in `partition.rs`** — the detector tracks exactly one current
   partition view (`Option<PartitionInfo>`), not a multi-partition topology
   map. Don't wait for these events; they won't come from this code path.
2. `check_rejoined_nodes()` re-fires `NodeRejoined` for **every currently
   visible-and-known node on every tick**, not just on the transition from
   absent to present — it isn't tracking a "previously invisible" set. Don't
   treat repeated `NodeRejoined` events as a symptom of instability; it's the
   detector's own behavior.

`cause: PartitionCause` on every `PartitionInfo` constructed by the detection
loop is always `PartitionCause::default()` (`Unknown`) — the detector never
auto-infers `NetworkFailure`/`NodeCrash`/`ConfigurationError`/`HighLatency`;
you must classify it yourself if you need that field populated.

### 4.4 Reconnection loops / degraded mode

`recovery.rs` — `RecoveryError` variants: `MaxRetriesExceeded(u32)`
(`"Maximum retry attempts exceeded: {0}"`), `ReconnectionFailed(String)`,
`ReconciliationFailed(String)`, `Timeout` (`"Recovery timeout"`),
`NodeUnreachable(NodeId)`. `ConnectionRecovery::record_failure()` returns
`RecoveryError::MaxRetriesExceeded(attempt)` once `attempt >=
config.max_retries`.

`RetryConfig` presets (reconnection backoff):

| Preset | max_retries | initial_delay | max_delay | multiplier | jitter |
|---|---|---|---|---|---|
| `default()` | 5 | 100 ms | 30 s | 2.0 | yes |
| `aggressive()` | 10 | 50 ms | 5 s | 1.5 | yes |
| `conservative()` | 3 | 1 s | 60 s | 3.0 | yes |

`DegradationThresholds` default: `min_visible_ratio: 0.3`, `max_failures: 10`,
`max_latency_ms: 5000`. `DegradationManager::evaluate()` triggers degraded
mode when **any** of the three thresholds is crossed (checked in that
priority order), emitting one of: `"Low peer visibility: {:.1}%"`, `"High
failure rate: {} failures"`, `"High latency: {}ms"`. If you're seeing
unexpected `DegradedModeEntered` events under real load spikes rather than
genuine failure, `max_latency_ms: 5000` is a common false-positive trigger —
see also [§7](#7-performanceresource-issues).

**There are two independent circuit breakers in this codebase** — check which
layer is actually reporting "open" before tuning the wrong one:

| | `mielin-mesh-core::error::CircuitBreaker` | `mielin-mesh-wire::retry` circuit breaker |
|---|---|---|
| Default `failure_threshold` | 5 | 5 |
| Default `success_threshold` | 2 | 2 |
| Default `timeout` (open → half-open) | 30 s | 60 s |
| Extra field | `max_timeout: 300s` | `metrics_window: 60s` |
| "Open" error surfaced | `MeshNetworkError::CircuitBreakerOpen` → `"Circuit breaker open"` / `"Circuit breaker open for {addr}"` | `WireError::TransportError("Circuit breaker is open")` |

### 4.5 Timeouts

`timeout.rs` — default `TimeoutConfig::new(duration)` is `Duration::from_secs(5)`.
`ServiceTimeoutPolicy::new()` seeds explicit per-operation defaults: `Connect`
5 s, `Request` 30 s, `Idle` 300 s, `DnsLookup` 3 s, `TlsHandshake` 10 s, plus a
`global_timeout` of 60 s and a `CircuitBreakerConfig::default()` attached
automatically to every new service policy.

`TimeoutError` variants: `OperationTimeout { timeout }` (`"Operation timeout
after {timeout:?}"`), `ConfigNotFound { service_name }`, `InvalidConfig
{ reason }`, `CircuitBreakerOpen { service_name }` (`"Circuit breaker open for
service: {service_name}"`). `execute_with_timeout_mesh()` maps these onto
`MeshNetworkError`: `OperationTimeout → Timeout`, `CircuitBreakerOpen →
CircuitBreakerOpen`, everything else → `ProtocolError`.

If `adaptive: true` is set on a `TimeoutConfig`, `calculate_timeout()` takes
the configured percentile (clamped to `[0.5, 0.99]`) of historical latencies
and **adds a 20% buffer on top**, clamped to `[min_duration, max_duration]` —
a timeout that seems to drift upward over time under sustained load is this
mechanism working as designed, not a leak.

### 4.6 Healing / rebalance

`ConsistentHashRing` uses `DEFAULT_VIRTUAL_NODES = 150`.
**`get_affected_keys_on_join(new_node, keys)` has a known implementation gap:**
it's implemented as `keys.filter(|k| self.get_node(k).is_some())`, and since
`get_node()` returns `Some` for any key whenever the ring is non-empty, this
returns essentially the **entire** input key list, not the specific subset
that migrates ownership to `new_node`. Don't rely on it to compute a minimal
rebalance set without verifying the behavior first.

### 4.7 Graceful shutdown

`ShutdownError` variants: `Timeout(Duration)` (`"Shutdown timeout after
{0:?}"`), `ComponentFailed { component, reason }`, `AlreadyInProgress`
(`"Shutdown already in progress"`), `MultipleFailures { count }` (`"Multiple
component failures during shutdown: {count} components failed"`).

`ShutdownPriority` (`High`/`Normal`/`Low`) controls ordering — components are
shut down High-first, Low-last. `graceful_shutdown()` splits the total
timeout evenly across components (`per_component_timeout = shutdown_timeout /
component_count`), so adding components without raising the overall timeout
shrinks each component's individual budget.

**Two things worth knowing when a shutdown "succeeds" but something clearly
didn't stop cleanly:**
- `ComponentFailed` is declared on `ShutdownError` but **never actually
  constructed** by the shutdown flow — real per-component failures accumulate
  into `failed_components` and surface only as the aggregate
  `MultipleFailures { count }`. Look for the `warn!("Component {} shutdown
  failed: {}", ...)` / `warn!("Component {} shutdown timed out after {:?}",
  ...)` log lines to identify *which* component, not the error variant.
- `immediate_shutdown()` and `forced_shutdown()` **always return `Ok(())`**
  even if individual components errored (they only log a `warn!`) —
  `forced_shutdown()` in particular doesn't actually kill any tasks, it just
  clears the component registry. A clean `Ok(())` from either of these two
  signals does not by itself mean every component stopped correctly; check
  logs.

---

## 5. Migration Failures

Source: `mielin-cells/src/migration/types/{recovery,validation,audit}.rs`.
See `docs/MIGRATION.md` §7 for the full narrative; this section is the fast
symptom→strategy lookup.

### 5.1 Error classification

```rust
pub enum MigrationErrorType {
    NetworkTransient, NetworkPermanent, InsufficientResources,
    IncompatibleTarget, StateCorruption, Timeout, TargetUnreachable, Unknown,
}
```

`is_retryable()` is `true` for `NetworkTransient | InsufficientResources |
Timeout | TargetUnreachable`. `should_change_target()` is `true` for
`InsufficientResources | IncompatibleTarget | TargetUnreachable`.

### 5.2 Recovery strategy decision

```rust
pub enum RecoveryStrategy { RetryOnSameTarget, RetryOnDifferentTarget, Rollback, GiveUp }
```

`MigrationRecoveryManager::determine_strategy(&error)`:

```text
if error.retry_count >= config.max_retries:
    Rollback (if config.auto_rollback) else GiveUp
else match error.error_type:
    NetworkTransient | Timeout                                      -> RetryOnSameTarget
    InsufficientResources | IncompatibleTarget | TargetUnreachable   -> RetryOnDifferentTarget
    StateCorruption | NetworkPermanent | Unknown                     -> Rollback (if auto_rollback) else GiveUp
```

### 5.3 `RecoveryConfig` — use the real defaults

```rust
RecoveryConfig {
    max_retries: 3,
    initial_retry_delay_secs: 10,
    max_retry_delay_secs: 300,
    use_exponential_backoff: true,
    recovery_timeout_secs: 600,
    auto_rollback: true,
}
```

**Caveat:** a second, *dead-code* `Default for RecoveryConfig` (`initial_retry_delay_secs:
5, max_retry_delay_secs: 60, recovery_timeout_secs: 300`) exists in
`mielin-cells/src/migration/recoveryconfig_traits.rs`, but that file is not
declared as a module in `migration/mod.rs` — it never compiles into the
running crate. If you're debugging a discrepancy between "the numbers in this
guide" and "the numbers I found by grepping the source," you found the dead
file; the values above are the ones that actually run (also confirmed by the
`test_recovery_config_default` unit test).

`calculate_retry_delay(attempt)` = `initial_retry_delay_secs *
2^(attempt-1)`, capped at `max_retry_delay_secs`, when
`use_exponential_backoff` is set (flat `initial_retry_delay_secs` otherwise).
`RecoveryConfig::validate()` rejects `initial_retry_delay_secs == 0`,
`recovery_timeout_secs == 0`, or `max_retry_delay_secs < initial_retry_delay_secs`
— `MigrationRecoveryManager::new(config)` returns `Err(String)` immediately if
validation fails, so a manager that fails to construct at all means your
`RecoveryConfig` violates one of these three invariants.

### 5.4 Recovery loop

```text
record_failure(error)
  → execute_recovery(&agent_id)     // returns the chosen RecoveryStrategy,
                                     // appends a RecoveryAttempt, bumps retry_count
  → caller acts on the strategy
  → mark_recovery_success(&agent_id) | mark_recovery_failure(&agent_id, msg)
```

Diagnose stuck/pending migrations with `MigrationRecoveryManager::
pending_count()`, `get_pending() -> Vec<&MigrationError>`, `get_history
(&agent_id) -> Option<&Vec<RecoveryAttempt>>`. Backoff-aware retry scheduling:
`next_retry_time(&agent_id)`, `is_ready_for_retry(&agent_id)`,
`get_ready_for_retry() -> Vec<[u8; 16]>`.

### 5.5 Rollback

```text
validator.capture_rollback_info(migration_id, &agent, &state)?;  // before migration
// migration attempted, fails...
let rollback: RollbackInfo = validator.execute_rollback(migration_id)?;
```

`execute_rollback` removes the stored `RollbackInfo` (a rollback can only be
executed **once**) and fails with:

```rust
Err(CellError::InvalidState("Rollback info has expired".to_string()))
```

when `RollbackInfo::is_valid(rollback_max_age_secs)` is false. Default max age
is **3600 seconds**, configurable via
`MigrationValidator::set_rollback_max_age(secs)`. `RollbackInfo` stores
`agent_id`, `original_state`, `original_checksum` (`simple_checksum`, **not**
a cryptographic hash), `original_policy_data` (oxicode-serialized `Policy`),
`created_at`, `source_node`.

The general `mielin-cells` error type wrapping these:

```rust
pub enum CellError {
    ExecutionFailed(String),   // "Agent execution failed: {0}"
    MigrationFailed(String),   // "Migration failed: {0}"
    InvalidState(String),      // "Invalid state: {0}"
}
```

### 5.6 Audit trail

Every recovery/rollback step is recorded to an append-only
`MigrationAuditLog` (capacity-bounded, default 10,000 entries, oldest
evicted first; queryable by migration ID, agent ID, event type, time range,
or failures-only):

```rust
pub enum AuditEventType {
    MigrationStarted, ValidationCompleted, SnapshotCaptured,
    TransferStarted, TransferCompleted, VerificationCompleted,
    MigrationCompleted, MigrationFailed, RollbackStarted, RollbackCompleted,
}
```

`TransferStarted`/`TransferCompleted` are declared but **no call site
constructs them today** — don't search the audit log for these two event
types expecting to find transfer-phase entries; use `SnapshotCaptured` /
`VerificationCompleted` as the surrounding markers instead.

### 5.7 Known gaps worth ruling out before you assume a bug

(Cross-referenced in full in `docs/MIGRATION.md` §10 — summarized here for
triage speed.)

- **`wasm_state` is never populated.** Only the WASM binary and `Policy`
  survive a `MigrationSnapshot` round trip; runtime linear memory does not. If
  an agent "loses state" across a migration, this is expected today, not a
  regression.
- **The mesh-level pre-copy coordinators** (`mielin_mesh_wire::migration::
  MigrationCoordinator` and `mielin_mesh_core::migration::MigrationCoordinator`)
  implement realistic phase/timeout/cancellation state machines but their
  phase-execution bodies contain `tokio::time::sleep` placeholders — no real
  byte transfer happens through these coordinators yet. The path that
  actually moves bytes today is `mielin_cells::migration::MigrationSnapshot`
  serialized into a `mielin_mesh_wire::Message::AgentMigration` frame.
- **`agent_responsive` in `VerificationResult` is hard-coded `true`** — there
  is no independent liveness probe distinct from the checksum/size checks.
- **`MigrationManager` has no `cancel_migration` method** — only
  `initiate_migration`, `complete_migration`, `pending_count`, `get_pending`
  exist, despite example code elsewhere calling `cancel_migration`.

### 5.8 CLI/API surface

`mielinctl migrate status|cancel|history` currently returns mock data (see
§3.6). The live control-plane endpoint is `GET /api/v1/migrate/status`
(`MigrationStats { total_migrations, active_migrations, successful, failed }`,
`server.rs::migrate_status_handler`, backed by
`MeshService::get_migration_stats()`).

---

## 6. TLS/Certificate Problems

Source: `mielin-mesh/wire/src/{advanced_tls,cert_rotation}.rs`,
`mielin-mesh/wire/src/certs/{mod,ca,mtls,pinning,acme,renewal,storage}.rs`.
See `docs/CERTIFICATES.md` §8 for the day-2 **operational runbook** (rotate a
cert with zero downtime, add/rotate a pin, set up mTLS, configure CA +
revocation) — this section is diagnosis-focused: which error variant means
what, and where to look.

### 6.1 Crypto stack and trust model

QUIC runs TLS 1.3 exclusively (`TlsVersion`'s only variant is `V1_3`; no
negotiated fallback to TLS 1.2). Pure-Rust stack: `rustls` (`default-features
= false`) + `oxiquic-crypto` (supplies the `CryptoProvider` — every
`ClientConfig`/`ServerConfig` builder must pass it explicitly via
`builder_with_provider(...)` since there's no default provider compiled in),
`oxicrypto-hash`/`oxicrypto-sig` for fingerprints and ACME JWS signing,
`rcgen`/`oxitls-rcgen` for cert/CSR issuance, `x509-parser` for DER parsing.

The top-level certificate error type (`certs/mod.rs`), used across nearly
every module below unless noted otherwise:

```rust
pub enum CertError {
    GenerationFailed(String),                                  // "Failed to generate certificate: {0}"
    Expired { expired_at: SystemTime, now: SystemTime },        // "Certificate expired on {expired_at:?} (now: {now:?})"
    NotFound { identifier: String },                            // "Certificate not found for {identifier}"
    Invalid { reason: String },                                 // "Invalid certificate: {reason}"
    StorageError(String),                                       // "Storage error: {0}"
    ValidationFailed { reason: String },                        // "Certificate validation failed: {reason}"
    KeyError { details: String },                                // "Key serialization/deserialization failed: {details}"
    ChainVerificationFailed { reason: String },                  // "Certificate chain verification failed: {reason}"
    SanMismatch { expected: String, actual: String },            // "Subject alternative name (SAN) mismatch: expected {expected}, got {actual}"
    RotationFailed { attempts: usize, last_error: String },      // "Certificate rotation failed after {attempts} attempts: {last_error}"
    TlsConfigError { details: String },                          // "TLS configuration error: {details}"
    EncodingError { details: String },                           // "Certificate encoding error: {details}"
}
```

### 6.2 Chain validation failures

`RevocationStatus::{Valid, Revoked { revoked_at }, Unknown}`.
`RevocationCheckMethod::{None, Crl, Ocsp, OcspThenCrl}` (`CaConfig::
production()` = `OcspThenCrl`, 10 s OCSP timeout, 1 h CRL cache; `CaConfig::
development()` = `None`, allows expired CRLs).

**OCSP is scaffolding, not a working check today.** `check_revocation()`'s
`Ocsp` path extracts the responder URL and logs it, but always returns
`RevocationStatus::Unknown` — it never performs a real OCSP request (the
in-source comment cites the lack of a suitable pure-Rust ASN.1 encoder).
`OcspThenCrl` therefore always falls through to the CRL check in practice.
**Treat `Unknown` as "no CRL data available, policy decision required"**, not
as "verified clean" — most deployments should either treat CRL as the source
of truth or explicitly accept the risk of `Unknown` for OCSP-only CAs.

CRL fetch failures (`CertError::ValidationFailed { reason }`, exact format
strings from `ca.rs`): `"CRL: cert parse: {e}"`, `"CRL parse failed: {e}"`,
`"CRL GET build error: {e}"`, `"CRL GET send error for {uri}: {e}"`, `"CRL GET
returned {status} from {uri}"`, `"CRL body read error: {e}"`, `"No CRL
distribution point URIs available"`. `ca.clear_crl_cache()` force-refreshes
ahead of the cached expiry (e.g. after an out-of-band revocation notice).

**mTLS chain rejection:** if a peer's certificate is signed by a CA never
added via `MtlsContext::with_trust_anchor`, and `allow_self_signed` is
`false`, you'll see `RustlsError::General("No trust anchors configured for
client certificate verification")` (or the server-side equivalent) if
`trust_anchors` is empty, or the standard webpki chain-validation error
(untrusted issuer) otherwise. Fix: call `.with_trust_anchor(ca_der)` for
every CA you need to trust *before* setting `allow_self_signed: false` in
production. Also note: `remove_ca_cert()` removes the fingerprint from
`ca_info` but does **not** rebuild `trust_anchors` — a "removed" CA can still
validate chains until the `CertificateAuthority` is reconstructed (flagged as
a known gap directly in `ca.rs`'s own comments).

`MtlsConfig` presets: `production()` (require+verify client cert, verify
server cert, pinning on, `allow_self_signed: false`), `development()` /
`testing()` (`allow_self_signed: true` — chain validation is **bypassed
entirely**; production paths should never hit this branch).

### 6.3 Pinning mismatches

There are **two independent pinning implementations** — identify which one
you're integrating with before chasing the wrong error type:

| | `certs::pinning::PinStore` | `advanced_tls::CertPinStore` |
|---|---|---|
| Model | async, identifier-keyed | sync, hostname-keyed, constant-time compare |
| Result type | `PinVerificationResult::{Matched{pin_hash,is_backup}, NoPins{identifier}, NoMatch{identifier,expected_pins,actual_hash}, Expired{identifier,pin_hash}}` | `Ok(PinVerification::{Pinned,Unpinned,NoPinsLeft})` / `Err(PinError::{NoPinsForHost{hostname}, FingerprintMismatch{hostname}, AllPinsExpired{hostname}})` |
| Used by | `MtlsContext` (see caveat below) | `CertChainVerifier` |
| Rotation | `rotate_pin()` adds a 60-day backup pin, then `promote_backup_pin(id, remove_old)` | no backup/promote workflow — `add_pin` the new fingerprint and call `remove_expired()` later |

**Caveat:** `MtlsContext`'s own `use_pinning` flag is a **documented no-op
pass-through** — when set, it just logs and lets the already-chain-validated
certificate through (`mtls.rs` comment: *"Pin-store check is a best-effort
augment on top of chain validation... here we just pass through since the
chain is already trusted"*). If you need pin-or-reject enforcement, call
`PinStore::verify_certificate` explicitly in your accept path, or use
`advanced_tls::CertChainVerifier::with_pin_store(...)`, which does perform
the check (`ChainError::{TooDeep{depth,max}, PinFailed(PinError), EmptyChain}`,
default `max_depth: 5`).

The literal "pin mismatch" runtime log line (`pinning.rs`): `warn!("Certificate
pin verification failed for '{}': expected {:?}, got {}", identifier,
expected_pins, actual_hash)`.

### 6.4 ACME / Let's Encrypt issuance failures

`certs/acme.rs` is an in-house RFC 8555 client (`instant-acme` was
deliberately removed in favor of this implementation). Directory URLs:

```text
staging:    https://acme-staging-v02.api.letsencrypt.org/directory
production: https://acme-v02.api.letsencrypt.org/directory
```

Authorization/order polling defaults to a 2 s interval, 30 attempts
(`PollPolicy`). A `badNonce` ACME error triggers exactly one automatic retry
with a fresh nonce; issued certs are hard-coded to `validity_days = 90`.

Common `CertError::GenerationFailed` messages you'll actually see (verbatim):
`"Missing Replay-Nonce header"`, `format!("newAccount failed: {}", status)`,
`"newAccount missing Location header (kid)"`, `"newOrder missing Location
header"`, `"No ACME account"` (you called an order/authorization method before
`initialize_account`), `format!("Unexpected authz status: {}", authz.status)`,
`format!("{challenge_type_str} challenge not available for {domain}")`, `"No
challenge validator configured"` (you never called `.with_validator(...)`),
`format!("Authorization did not become valid for {domain} after {} attempts",
max_attempts)`, `"Order valid but no certificate URL"`, `"Order became
invalid"`, `format!("Order did not become valid after {} attempts",
max_attempts)`.

`renew_if_needed(cert, domains)` only calls `request_certificate` if
`current_cert.info.should_rotate_with_threshold(renewal_threshold_days)` is
true — if you expected a renewal and nothing happened, check the threshold
against the certificate's actual remaining validity first.

### 6.5 Renewal scheduling stalls or never fires

`RenewalScheduler` (`certs/renewal.rs`) presets:

| Preset | check_interval | renewal_threshold_days | max_retry_attempts | initial_retry_delay | max_retry_delay |
|---|---|---|---|---|---|
| `aggressive()` | 900 s (15 m) | 60 | 10 | 30 s | 1800 s |
| `conservative()` | 21600 s (6 h) | 7 | 3 | 300 s | 7200 s |
| `production()` (= `default()`) | 3600 s (1 h) | 30 | 5 | 60 s | 3600 s |

`RenewalEvent` (broadcast, buffer 100): `CheckStarted`, `RenewalTriggered`,
`RenewalSucceeded`, `RenewalFailed { identifier, error, attempt }`,
`RenewalRetrying { identifier, attempt, delay_secs }`, `RenewalGaveUp
{ identifier, total_attempts, last_error }`. Exponential backoff (default
on): `delay = min(initial_retry_delay * 2^attempt, max_retry_delay)`.

**Caveat:** `CertRotator::subscribe_to_renewal(scheduler.subscribe())` only
**bumps rotation statistics** on `RenewalSucceeded` — it does not itself
fetch the new certificate material and hot-swap it (the code comment is
explicit: *"Full DER material is not available from the event alone"*). If
you expected renewal to automatically flow into a live rotation, it won't
without your own task listening on `scheduler.subscribe()` that fetches the
fresh cert (`scheduler.get_certificate().await`) and calls
`rotator.rotate(...)` itself.

### 6.6 Rotation failures

`cert_rotation.rs`'s `CertRotationError` (hand-written `Display`, not
`thiserror`):

```rust
pub enum CertRotationError {
    TlsConfigBuild(String),               // "TLS config build failed: {}"
    InvalidCertificate(String),            // "Invalid certificate: {}"
    RotationInProgress,                    // "A certificate rotation is already in progress"
    MaxRotationsExceeded { max: u32 },     // "Maximum rotations per hour exceeded (limit: {})"
}
```

Defaults: `max_rotations_per_hour: 12`, `min_rotation_interval: 60s`,
`require_valid_before_swap: true` (`permissive()` preset relaxes all three).
**Both** "too many rotations this hour" **and** "rotated too recently" return
the **same** `MaxRotationsExceeded` variant/message — the message alone
doesn't tell you which limit was hit. Disambiguate with `rotator.stats()`
(`total_rotations`, `last_rotation_at`) before assuming you've hit the
per-hour cap when it might just be the minimum-interval guard.

### 6.7 Connection health monitoring

`advanced_tls::HeartbeatConfig` defaults: `interval: 30s`, `timeout: 10s`,
`max_missed: 3`, `rtt_window: 10`. `ConnectionHealthStatus::{Healthy,
Degraded, Unhealthy, Unknown}`. `HealthMonitorError::{ConnectionNotFound
{ node_id }, LockPoisoned}`.

For the step-by-step how-to (rotate zero-downtime, add a pin, set up mTLS
between two nodes, configure CA + revocation checking), see
[`CERTIFICATES.md` §8](./CERTIFICATES.md#8-operational-runbook).

---

## 7. Performance/Resource Issues

This guide is diagnosis-of-failure focused; for benchmarking methodology,
profiling workflows (`flamegraph`, `perf`, `valgrind --tool=massif`), SIMD/
cache tuning, and build-profile tradeoffs (`release` vs. `release-speed` vs.
`release-embedded` vs. `release-debug`), see
[`PERFORMANCE_TUNING.md`](./PERFORMANCE_TUNING.md) and `docs/PERFORMANCE.md`.

Two things covered elsewhere in this guide are commonly misdiagnosed as
"performance problems" when they're actually threshold/config behavior:

- **`DegradationThresholds::max_latency_ms` (default 5000)** tripping
  `DegradedModeEntered` under a legitimate load spike, not an actual
  regression — see [§4.4](#44-reconnection-loops--degraded-mode).
- **Adaptive `TimeoutConfig`'s 20%-over-percentile buffer** making effective
  timeouts creep upward under sustained load — see [§4.5](#45-timeouts).

If you're chasing a slow build rather than a slow runtime, check §1.9's `make
bench` / `cargo bench -p benches` and `benches/README.md` for the current
benchmark baselines before assuming a change regressed something — compare
against `cargo bench -- --baseline <name>`.

---

## 8. Observability

### 8.1 Logging — `RUST_LOG`

`RUST_LOG` is honored **inconsistently** across the workspace — check which
init pattern the specific binary uses before assuming it will change
verbosity:

- **Honors `RUST_LOG`:** `mielin-cli/src/main.rs` calls the bare
  `tracing_subscriber::fmt::init();` — no `EnvFilter` type is used anywhere in
  this workspace (`tracing-subscriber`'s `env-filter` Cargo feature isn't
  enabled), but `fmt::init()`'s own internal fallback parser still reads
  `RUST_LOG` (e.g. `RUST_LOG=debug`, or target-scoped like
  `RUST_LOG=mielin_cli=debug,mielin_mesh=info`).
- **Ignores `RUST_LOG` entirely:** most example binaries (`examples/e2e-full-
  stack`, `examples/mesh-cluster`, `examples/e2e-mesh-cluster`,
  `examples/e2e-agent-migration`, `examples/e2e-embedded-iot`, most
  `mielin-mesh/wire/examples/*`) call the builder chain with a **hardcoded**
  `tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init()` —
  setting `RUST_LOG` before running one of these has no effect; you have to
  edit the example's `main.rs` or pick a different init if you need more
  verbosity there.

`docker-compose.yml` sets `RUST_LOG=debug` for the `dev` service and
`RUST_LOG=info` for `core`/`relay`/`edge` — this only takes effect for
`mielin-cli`'s own `daemon` process running inside those containers, per the
rule above. Output goes to stdout/stderr in plain `tracing-subscriber` `fmt`
format — there is no JSON sink and no file sink configured anywhere in the
workspace today.

### 8.2 Distributed tracing

`mielin-mesh/core/src/tracing.rs` is a **hand-rolled, in-process** tracing
library — not a `tracing`/`tracing-subscriber` initializer, and not the real
`opentelemetry` crate (which is not a dependency anywhere in this workspace).
It provides W3C-traceparent-compatible `TraceId`/`SpanId`, `Span`/
`SpanBuilder`, and domain-specific shapes (`GossipTrace`/`GossipHop`,
`MigrationTrace`). `TraceCollector` buffers spans in memory
(`tokio::sync::RwLock`, configurable `max_traces`/`sampling_rate`).
`export_trace_json()`/`export_trace_otel()` serialize a collected trace to
plain JSON or to an OpenTelemetry-**shaped** structure (`OTelSpan`) — this is
a self-written serializer producing OTel-compatible JSON, not a real OTLP
exporter. **Nothing ships spans off-box automatically**; you must drain
`TraceCollector` and export it yourself if you need external trace ingestion.

### 8.3 Metrics

`mielin-mesh/core/src/metrics/` defines `Counter`/`Gauge`/`Histogram`
primitives and domain aggregates (`NodeMetrics`, `GossipMetrics`,
`DhtMetrics`, `PeerConnectionMetrics`, `MigrationSuccessMetrics`), rolled up
into `MetricsSummary`. `export.rs` provides `PrometheusExporter` (hand-written
Prometheus text-exposition format, with optional `# HELP`/`# TYPE` lines) and
`JsonExporter` for that same summary.

**No crate in this workspace depends on the `prometheus`, `metrics`, or
`opentelemetry` crates, and no HTTP route anywhere serves `/metrics`.**
`PrometheusExporter`/`JsonExporter` are library APIs you call and expose
yourself (e.g. by adding a route to the Axum control server) — there is no
working scrape endpoint out of the box. `docker-compose.yml` likewise defines
no Prometheus/Grafana/OTel-collector service.

Sibling orphan file: `mielin-mesh/core/src/metrics.rs.old` (2396 lines) is
**dead code** — `lib.rs` declares `pub mod metrics;` pointing at the
`metrics/` directory, and the `.old` file isn't referenced by any `mod`
declaration. If you're grepping for a metric and find it only in
`metrics.rs.old`, it isn't compiled in; look in `metrics/types.rs` instead.

### 8.4 Live HTTP surface (real, not mocked)

The Axum control plane started by `mielinctl daemon` (`--control-listen`,
default `127.0.0.1:8081`, `mielin-cli/src/control/server.rs`):

```text
GET /api/v1/health           liveness probe + node ID + CARGO_PKG_VERSION
GET /api/v1/mesh/status      gossip alive/suspect/dead counts, DHT peer count, local agent count
GET /api/v1/mesh/peers       alive gossip members
GET /api/v1/mesh/nodes       alias for /api/v1/mesh/peers
GET /api/v1/agents           local agent count (lightweight — no per-agent metadata yet)
GET /api/v1/migrate/status   aggregate migration stats
```

`curl` these directly for live diagnosis, e.g. `curl
http://127.0.0.1:8081/api/v1/mesh/status`. **Caveat:**
`mesh_status_handler` swallows sub-call errors and substitutes `0` rather
than surfacing an HTTP error — a response showing `"alive": 0, "dht_peers":
0` can mean either "genuinely empty mesh" or "an internal query failed
silently." It will never come back as a 5xx for that reason; if the numbers
look wrong, check the daemon's own logs rather than trusting a 200 response
as proof nothing is broken.

### 8.5 CLI surface

`mielinctl mesh status|peers` (`--daemon <url>` or `MIELIN_DAEMON` env var —
see §3.6 for what happens without it), `mielinctl cluster health`,
`mielinctl monitor top|watch|events|dashboard` (aliases: `mon`, `observe`),
`mielinctl debug attach|trace|dump|profile`. Apply the mock-data caveat from
§3.6 to any of these that doesn't explicitly go through `--daemon`/
`ControlClient`.

### 8.6 Kernel-level tracing (separate subsystem)

`mielin-kernel/src/observability.rs` is an unrelated, `no_std` eBPF-like
tracepoint/PMU-counter/ring-buffer facility for bare-metal kernel debugging
(`TracepointId::{TaskSpawn, TaskTerminate, TaskSchedule, PageAlloc, ...}`,
`observability::init(buffer_size)`). It shares no code path with `RUST_LOG`,
`tracing.rs`, or the exporters above. For kernel-level diagnosis, see
`mielin-kernel/docs/TROUBLESHOOTING.md`.

---

## 9. First Response Checklist

Work through these in order before escalating — most symptoms in §3–§6 are
caught by the first four steps:

1. **Confirm the daemon is actually up and started successfully.**
   `curl http://<control-listen>/api/v1/health` or `mielinctl mesh status
   --daemon <url>`. This rules out `MeshError::ServiceNotStarted` (§3.2) and
   the mock-data trap (§3.6) in one step.
2. **Rule out stale build artifacts** before chasing a logic bug: `cargo
   clean && cargo build -p <affected-crate>`.
3. **Enable debug logging** — but check which init pattern the binary uses
   first (§8.1): `RUST_LOG=debug` only affects binaries using bare
   `tracing_subscriber::fmt::init()` (`mielin-cli`); most examples ignore it.
4. **Isolate the failure to one crate and get real output:** `cargo nextest
   run -p <crate> -- --nocapture`.
5. **For mesh symptoms**, check in this order: gossip member counts
   (`/api/v1/mesh/peers` or `get_member_stats()`) → partition/quorum state
   (§4.3) → timeouts/circuit breakers (§4.4–§4.5) — later stages can mask
   earlier ones.
6. **For migration symptoms**, check `MigrationRecoveryManager::get_pending()`
   / `get_history()` and the `MigrationAuditLog` before assuming data loss —
   most `MigrationErrorType` variants are retryable (§5.1), and the recovery
   manager will already be working through them.
7. **For TLS symptoms**, identify the *exact* error enum/variant first
   (`CertError` vs. `PinError`/`ChainError` vs. `CertRotationError` vs. a raw
   `RustlsError`) — §6 is keyed off the specific variant, and the two
   independent pinning stores (§6.3) have non-overlapping error types.
8. **If the failure looks environmental** (missing target, tool, formatting/
   clippy drift) rather than a logic bug, run `./scripts/verify.sh` and act on
   its exact remediation output rather than guessing.
9. **Still stuck — file an issue** with: `mielinctl version` / `git describe
   --tags`, `rustc --version`, target triple/platform, the **full** error
   text (not just the top line — `thiserror` `#[source]` chains carry the
   real root cause), and the exact command/API request that reproduces it.

---

## Related Documentation

- [Deployment Guide](./DEPLOYMENT.md)
- [Networking Guide](./NETWORKING.md) — mesh architecture, DHT, gossip protocol, discovery, partition detection
- [Certificate Management](./CERTIFICATES.md) — TLS trust model and the day-2 operational runbook
- [Migration Protocol](./MIGRATION.md) — full agent migration pipeline and known gaps
- [Performance Tuning](./PERFORMANCE_TUNING.md) — benchmarking, profiling, and tuning methodology
- `mielin-kernel/docs/TROUBLESHOOTING.md` — bare-metal kernel build/runtime issues (memory, scheduler, interrupts, QEMU)
- `mielin-cli/docs/TROUBLESHOOTING.md` — CLI installation, configuration, and day-to-day usage issues
