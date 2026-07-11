# MielinOS Deployment Guide — Small Clusters

This guide explains how to stand up a small MielinOS cluster (a handful of nodes on a
LAN, a set of Docker containers, or a few cloud VMs) using the tooling that actually
ships in this repository today.

Every command, flag, config key, port, and route named below is taken directly from
the source: `mielin-cli/src/cli.rs`, `mielin-cli/src/commands/*.rs`,
`mielin-cli/src/control/{server,client,dto}.rs`, `mielin-cli/src/config.rs`,
`mielin-mesh/core/src/{service,discovery,discovery_static,discovery_dns,discovery_aggregator,node}.rs`,
`docker-compose.yml`, `Dockerfile`, `Makefile`, `scripts/cluster.sh`, and
`examples/mesh-cluster/src/main.rs`. Where the shipped CLI, Docker Compose file, or
`QUICKSTART.md` describe behavior that isn't wired up yet, this guide says so
explicitly under an **Implementation note** — the goal is that everything here is
copy-pasteable against the current tree, not aspirational.

## Table of Contents

1. [Overview](#1-overview)
2. [Prerequisites & Install](#2-prerequisites--install)
3. [Single-Node Bring-Up](#3-single-node-bring-up)
4. [Forming a Cluster](#4-forming-a-cluster)
5. [Deploying Agents to the Cluster](#5-deploying-agents-to-the-cluster)
6. [Docker / Docker Compose Deployment](#6-docker--docker-compose-deployment)
7. [Configuration Reference](#7-configuration-reference)
8. [Health Checks, Monitoring & Basic Operations](#8-health-checks-monitoring--basic-operations)
9. [Scaling & Teardown](#9-scaling--teardown)

---

## 1. Overview

### 1.1 What "a small cluster" means here

A MielinOS cluster is a set of processes, each running the mesh stack
(`mielin-mesh-core::service::MeshService`), that discover each other and exchange
gossip/registry/migration traffic. Each node has a role
(`mielin-mesh/core/src/node.rs`):

```rust
pub enum NodeRole {
    Edge,
    Relay,
    Core,
}
```

For a small cluster the usual topology is one **Core** node (always-on, most
resources — the natural bootstrap/anchor and migration target), zero or more
**Relay** nodes (gateways that sit between edge devices and the core), and one or
more **Edge** nodes (agent hosts — IoT devices, laptops, small VMs). This mirrors
the roles used throughout `docs/CLUSTER_TESTING.md` and `docker-compose.yml`
(`core` / `relay` / `edge` services).

### 1.2 Two different "node" programs — read this first

This repository ships **two separate executables** that both call themselves a
mesh node, and they are *not* the same thing. Which one you use changes what the
rest of this guide means for you:

| | `mielinctl daemon` | `mesh-cluster` (`node` binary) |
|---|---|---|
| Crate | `mielin-cli` (`mielin-cli/src/commands/daemon.rs`) | `examples/mesh-cluster` (`examples/mesh-cluster/src/main.rs`) |
| Binary name | `mielinctl` (subcommand `daemon`) | `node` (`[[bin]] name = "node"` in `examples/mesh-cluster/Cargo.toml`) |
| HTTP control plane | Yes — axum server on `--control-listen` (`mielin-cli/src/control/server.rs`) | No |
| Real QUIC peer connect | No — see §4.2 | Yes — uses `mielin_mesh_wire::transport::QuicTransport` |
| Used by `docker-compose.yml` | No | Yes (`core`/`relay`/`edge` services run `/app/node`) |
| Used by `mielinctl mesh status/peers --daemon` | Yes | No |

Sections 3–5 and 7–9 of this guide are about `mielinctl daemon`, since that is the
supported "daemon + control plane" model described in the task this guide answers.
Section 6 documents the `mesh-cluster`/`node` binary, since that's what
`docker-compose.yml` and `scripts/cluster.sh` actually run. Keep the distinction in
mind — flags that work for one do not apply to the other.

### 1.3 What's real vs. simulated in `mielinctl` today

Before you build a deployment plan around a subcommand, check this table.
"Live" means the handler makes an HTTP call to a running daemon's control plane.
"Local/simulated" means the handler prints a hard-coded or randomly generated
result and never talks to a daemon at all (verified by absence of any
`ControlClient`/network call in the handler source).

| Command | Status | Source |
|---|---|---|
| `mielinctl mesh status [--daemon <addr>]` | **Live** (with `--daemon`/`MIELIN_DAEMON`) | `commands/mesh.rs` |
| `mielinctl mesh peers [--daemon <addr>]` | **Live** (with `--daemon`/`MIELIN_DAEMON`) | `commands/mesh.rs` |
| `mielinctl mesh gossip`, `mielinctl mesh dht` | Local/simulated (fixed strings) | `commands/mesh.rs` |
| `mielinctl daemon` | **Live** — actually starts the mesh service + control plane | `commands/daemon.rs` |
| `mielinctl node list/info/start/stop/create/join/leave` | Local/simulated (hard-coded/mock data) | `commands/node.rs` |
| `mielinctl node config <get\|set\|--all>` | **Live** — reads/writes the local CLI config file | `commands/node.rs`, `config.rs` |
| `mielinctl agent *` (`list/deploy/create/migrate/stop/inspect/logs/exec`) | Local/simulated — no network call at all | `commands/agent.rs` |
| `mielinctl cluster *` | Local/simulated | `commands/cluster.rs` |
| `mielinctl registry *` | Local/simulated | `commands/registry.rs` |
| `mielinctl gossip *` | Local/simulated | `commands/gossip.rs` |
| `mielinctl migrate *` | Local/simulated | `commands/migrate.rs` |
| `mielinctl monitor top/watch/events/dashboard` | Local/simulated — uses `rand::rng()` for fake CPU/memory/events | `commands/monitor.rs` |
| `mielinctl remote *` | **Live** — SSH-free HTTP client against `POST /api/v1/command` on a peer you register, with its own `remote_nodes.toml` | `remote.rs`, `commands/remote.rs` |
| `mielinctl config *` | **Live** — validates/inits/migrates the local config file | `commands/config.rs` |

This guide only shows commands from the "Live" rows as functioning end-to-end
control-plane operations. Commands from the "Local/simulated" rows are documented
in §5 so you know the real flag surface, but flagged clearly as not yet wired to a
running cluster.

### 1.4 Topology options

- **Single core, N edges (star)** — one `mielinctl daemon --role core` reachable by
  every edge node; simplest and matches the docker-compose demo shape.
- **Core → relay → edge (chain)** — an intermediate relay fronts a group of edges,
  reducing the number of direct connections into the core. This is the shape
  `docker-compose.yml` and `docs/CLUSTER_TESTING.md` describe.
- **LAN with mDNS only** — no bootstrap addressing at all; nodes on the same
  multicast domain find each other automatically (see §4.1). Works well for a
  handful of machines on one switch/Wi-Fi network.

---

## 2. Prerequisites & Install

Full build/toolchain instructions already live in the project root docs — this
section only lists what's specific to running a cluster. See
[`../QUICKSTART.md`](../QUICKSTART.md) for the general dev-environment setup and
[`./RUNNING.md`](./RUNNING.md) for the *unrelated* bare-metal kernel/QEMU boot path
(booting `mielin-kernel` on real/virtual hardware is a different artifact from the
user-space `mielinctl`/mesh daemon this guide covers — don't conflate the two).

### 2.1 Toolchain

- Rust **nightly** — pinned by `rust-toolchain.toml` (`channel = "nightly"`,
  components `rustfmt`, `clippy`, `llvm-tools`). `rustup` will pick this up
  automatically inside the repo.
- `cargo-nextest` (used by `make test`/`make check`, optional for just running a
  daemon).

### 2.2 Build from source

```bash
git clone https://github.com/cool-japan/mielin
cd mielin

# Full workspace build (or `make build` for the debug equivalent)
cargo build --release --workspace
```

To build only the CLI/daemon binary:

```bash
cargo build --release -p mielin-cli
# binary: target/release/mielinctl
```

Confirm the build:

```bash
target/release/mielinctl version
target/release/mielinctl --help
```

### 2.3 Docker

`Dockerfile` is a three-stage build (`builder` → `runtime` → `development`); see
§6 for how `docker-compose.yml` uses each stage. To get a shell with `mielinctl` on
`PATH` without installing Rust locally:

```bash
docker compose up -d dev
docker compose exec dev bash
# inside the container:
cargo build --release -p mielin-cli
./target/release/mielinctl version
```

(The prebuilt `development` image stage also installs `mielinctl` at
`/usr/local/bin/mielinctl` when built directly — see §6.1.)

---

## 3. Single-Node Bring-Up

`mielinctl daemon` is the real entry point that starts a `MeshService` and its HTTP
control plane in one process. Its flags, from `mielin-cli/src/cli.rs`:

```
mielinctl daemon
  -l, --listen <ADDR>          Listen address for the mesh service [default: 0.0.0.0:8080]
  -r, --role <ROLE>            Node role (core, relay, edge) [default: edge]
  -b, --bootstrap <ADDR>       Bootstrap node address to connect to (optional, single address)
      --control-listen <ADDR>  Listen address for the HTTP control-plane API [default: 127.0.0.1:8081]
```

Role matching is case-insensitive and falls back silently: anything other than
`"core"` or `"relay"` (including typos) becomes `NodeRole::Edge`
(`commands/daemon.rs`).

### 3.1 Start a node

```bash
target/release/mielinctl daemon \
  --listen 0.0.0.0:9000 \
  --role core \
  --control-listen 127.0.0.1:8081
```

What happens on startup (`commands/daemon.rs`):

1. A fresh `Node` is created with a random UUID v4 identity (`Node::new`) — **node
   identity is not persisted across restarts**, see the note in §7.4.
2. A `MeshConfig` is built with `bind_address` = `--listen`,
   `bootstrap_nodes` = the single `--bootstrap` address if given, and
   `enable_mdns`/`enable_gossip`/`enable_registry`/`enable_migration` **all
   hard-coded to `true`** — there is currently no CLI flag or config file setting
   that can disable any of these for `mielinctl daemon` (contrast with the
   `daemon.enable_*` keys in `config.toml`, which are read by `mielinctl config
   validate`/`node config` but never by `mielinctl daemon` itself — see §7).
3. `MeshService::start()` runs, which starts (in order): mDNS-based
   `DiscoveryService`, `GossipState`, `AgentRegistry`, `MigrationCoordinator`, and a
   background peer-sync task that copies discovered peers into gossip + DHT every
   10 seconds.
4. The axum `ControlServer` (see §8) binds `--control-listen` and serves
   `/api/v1/...`.
5. The process blocks on `tokio::select!` between the control server and
   `Ctrl+C`; `Ctrl+C` triggers `MeshService::stop()` for a graceful shutdown.

### 3.2 Verify it's up

From a second terminal (or another host that can reach `--control-listen`):

```bash
curl http://127.0.0.1:8081/api/v1/health
# {"ok":true,"version":"0.1.0","node_id":"<uuid>"}

target/release/mielinctl mesh status --daemon 127.0.0.1:8081
```

`mielinctl mesh status --daemon <addr>` (or set `MIELIN_DAEMON=127.0.0.1:8081` and
drop `--daemon`) calls `GET /api/v1/mesh/status` through `ControlClient`
(`mielin-cli/src/control/client.rs`) and renders `alive`/`suspect`/`dead` gossip
counts, DHT peer count, and local agent count. On a freshly started, unpeered node
these will mostly be `0` — that's expected until the node has peers (§4).

---

## 4. Forming a Cluster

### 4.1 mDNS (automatic, same broadcast domain)

Because `enable_mdns` is always `true` for `mielinctl daemon`, every node
registers `_mielin._udp.local.` via `mdns-sd` and browses for the same service
type (`mielin-mesh/core/src/discovery.rs`). If you start two or more
`mielinctl daemon` instances on machines that share IP multicast (e.g. plain
hosts/VMs on the same LAN switch), they will discover each other **without any
`--bootstrap` flag** — no extra configuration needed. Discovered peers are folded
into gossip membership and the DHT automatically by the peer-sync task described
in §3.1.

**Caveat:** default Docker bridge networks do not reliably forward IP multicast
between containers, so mDNS-only discovery is a LAN/bare-metal/VM technique, not a
Docker Compose one — see §6.

### 4.2 Bootstrap nodes (`--bootstrap`)

```bash
target/release/mielinctl daemon --role relay --listen 0.0.0.0:9001 \
  --bootstrap 192.168.1.50:9000 --control-listen 127.0.0.1:8082
```

This registers `192.168.1.50:9000` as a `BootstrapNode { address, public_key: None }`
inside `MeshConfig.bootstrap_nodes` (`commands/daemon.rs`, one address only — the
flag is `Option<String>`, not repeatable).

> **Implementation note.** `DiscoveryService::connect_bootstrap()`
> (`mielin-mesh/core/src/discovery.rs`) is currently a documented placeholder: it
> iterates the configured bootstrap nodes and does nothing with them yet (the
> in-source comment reads *"Actual QUIC connection would be done here... This will
> be implemented when we create the full mesh service"*). `MeshService` does expose
> a `connect_bootstrap()` wrapper (`mielin-mesh/core/src/service.rs`), but
> `mielinctl daemon`'s startup path (`commands/daemon.rs`) never calls it after
> `service.start()`. In the current tree, **`--bootstrap` records the address but
> does not by itself open a connection to that peer** — reachability today comes
> from mDNS (§4.1) or from the DHT/gossip peer-sync task picking up peers that
> mDNS or manual registration already found. If you need guaranteed, non-mDNS
> connectivity between nodes right now, use the `mesh-cluster` example binary
> (§4.4/§6), which has a working QUIC connect path.

### 4.3 Static peer list & DNS-SRV discovery (library primitives, not yet CLI flags)

`mielin-mesh-core` ships two additional, fully-implemented discovery backends plus
an aggregator that aren't yet exposed as `mielinctl daemon` flags:

- **`StaticPeerList`** (`mielin-mesh/core/src/discovery_static.rs`) — a
  configuration-driven peer registry with per-peer health tracking
  (`PeerHealth::{Unknown,Reachable,Unreachable,Disabled}`), weighted random
  selection (`select_peer`), and automatic pruning of consistently unreachable
  peers (`prune_unreachable`). Designed for Kubernetes-style known-topology
  deployments where mDNS is unavailable.
- **`DnsSrvDiscovery`** (`mielin-mesh/core/src/discovery_dns.rs`) — models RFC 2782
  SRV record semantics (priority tiers, weighted selection within a tier, TTL
  caching) against an injectable record cache; production code would populate it
  via a real async DNS resolver.
- **`DiscoveryAggregator`** (`mielin-mesh/core/src/discovery_aggregator.rs`) —
  merges `StaticPeerList` + `DnsSrvDiscovery` (+ future mDNS) results into one
  de-duplicated `AggregatedPeer` stream, preferring Static > DNS > mDNS on
  `NodeId` collision.

All three types are exported from the crate's public API
(`mielin-mesh/core/src/lib.rs`) and are fully unit-tested, but `MeshConfig`,
`MeshService`, and `mielinctl daemon` do not construct or consume them today. If
your deployment needs static-peer-list or DNS-SRV based joining right now, embed
these types directly in a custom node binary (following the pattern
`examples/mesh-cluster/src/main.rs` uses for its own transport-level connect
logic) rather than expecting a `mielinctl daemon` flag for it.

### 4.4 The one path with a real, working peer connection today

The `mesh-cluster` example (`examples/mesh-cluster`, binary name `node`) opens
actual QUIC connections and exchanges `Message::Discovery`/`DiscoveryResponse`
over them:

```bash
# Terminal 1 — core
cargo run -p mesh-cluster -- --role core --port 5000

# Terminal 2 — edge, connects to core
cargo run -p mesh-cluster -- --role edge --port 5001 --connect 127.0.0.1:5000
```

This is a separate program from `mielinctl` (no `--control-listen`/HTTP API), and
it is what `docker-compose.yml` and `scripts/cluster.sh` actually run — see §6 for
the important caveats around its `--connect` flag and bind address.

---

## 5. Deploying Agents to the Cluster

`mielinctl agent` has a full, real flag surface (`commands/agent.rs`):

```
mielinctl agent list [-s|--state <STATE>] [-n|--node <NODE>]
mielinctl agent deploy <WASM_PATH> [-n|--node <NODE>]
mielinctl agent create <WASM_PATH> -n|--name <NAME> [-t|--node <NODE>]
                        [-e|--env KEY=VALUE ...] [-m|--memory <MB>=256] [-c|--cpu <SHARES>=1024]
mielinctl agent migrate <AGENT_ID> <TARGET_NODE>
mielinctl agent stop <AGENT_ID>
mielinctl agent inspect <AGENT_ID>
mielinctl agent logs <AGENT_ID> [-f|--follow] [-n|--lines <N>=100]
mielinctl agent exec <AGENT_ID> <COMMAND...> [-i|--interactive] [-t|--tty]
```

> **Implementation note — read before relying on these for a real rollout.**
> `commands/agent.rs` contains no `ControlClient`/HTTP call of any kind. `agent
> deploy`/`agent create` validate that the local `.wasm` file exists and that
> `--env` entries look like `KEY=VALUE`, then print a locally fabricated
> `OperationResult` with a freshly generated UUID — **no bytes are sent to any
> daemon and no agent is actually scheduled onto the mesh.** `agent list`,
> `agent migrate`, `agent stop`, `agent inspect` behave the same way (fixed or
> mock data). This matches the DTOs already defined server-side
> (`DeployRequest`/`DeployResponse`/`MigrationRequest`/`MigrationAck` in
> `mielin-cli/src/control/dto.rs`) — those exist as message shapes, but the axum
> router in `mielin-cli/src/control/server.rs` only registers `GET` routes
> (`/api/v1/health`, `/api/v1/mesh/status`, `/api/v1/mesh/peers`,
> `/api/v1/mesh/nodes`, `/api/v1/agents`, `/api/v1/migrate/status`); there is no
> `POST /api/v1/agents` (or similar deploy/migrate endpoint) registered yet, so
> even scripting around the HTTP API directly won't deploy an agent today.
>
> `QUICKSTART.md`'s "Working with the CLI (Future)" block
> (`mielinctl agent deploy agent.wasm`, `mielinctl node start --role edge`, etc.)
> describes this same not-yet-wired state — note in particular that
> `mielinctl node start` takes **no** `--role` flag in the real CLI (role is set
> at `node create`/`daemon` time, not at `node start` time).

What *is* real and observable end-to-end for agent placement today is the
`mesh-cluster` example binary from §4.4:

```bash
# Core node
cargo run -p mesh-cluster -- --role core --port 5000

# Edge node: connects, creates a trivial WASM-stub agent, registers it in the
# mesh registry via MeshService::register_agent
cargo run -p mesh-cluster -- --role edge --port 5001 \
  --connect 127.0.0.1:5000 --agent

# Same edge node, but also migrate the agent it just created to the core node
# over a real QUIC connection (Message::AgentMigration / MigrationAck)
cargo run -p mesh-cluster -- --role edge --port 5001 \
  --connect 127.0.0.1:5000 --agent --migrate-to 127.0.0.1:5000
```

This is exactly what `scripts/cluster.sh test-migration` exercises against the
Docker cluster (§6, §8). Until `mielinctl agent`/`node` are wired to the control
plane's (currently GET-only) HTTP API, treat `mielinctl agent *` as a documented,
stable **flag surface for future integration**, not a live deployment path.

---

## 6. Docker / Docker Compose Deployment

`docker-compose.yml` defines five services. All of `core`/`relay`/`edge` build
from the `runtime` target of `Dockerfile`, which contains **only** the
`mesh-cluster` example binary (§4.4), copied in as `/app/node` — not `mielinctl`.

| Service | Build target | Command | Host port → container | Depends on |
|---|---|---|---|---|
| `dev` | `development` (last stage, default) | `bash` | — | — |
| `core` | `runtime` | `/app/node --role core --port 8080` | `8080 → 8080` | — |
| `relay` | `runtime` | `/app/node --role relay --port 8080 --connect core:8080` | `8081 → 8080` | `core` (healthy) |
| `edge` | `runtime` | `/app/node --role edge --port 8080 --connect relay:8080 --agent` | `8082 → 8080` | `relay` (healthy) |
| `bench` | `development` | `cargo bench -p benches` | — | — |

All three mesh services join the `mielin-mesh` bridge network
(subnet `172.20.0.0/16`) and use `healthcheck: nc -z localhost 8080` (checks that
*something* is listening on TCP 8080 inside the container — it does not check
mesh membership or gossip health).

### 6.1 Bring it up

```bash
./scripts/cluster.sh build     # docker compose build core relay edge
./scripts/cluster.sh start     # docker compose up -d core relay edge
./scripts/cluster.sh status    # docker compose ps + per-node health
./scripts/cluster.sh logs edge # docker compose logs -f edge
```

`scripts/cluster.sh` is the real, supported wrapper — its full command surface:

```
scripts/cluster.sh {start|stop|restart|status|logs [node]|exec <node> <cmd>|
                     build|clean|test-migration|help}
```

`./scripts/cluster.sh test-migration` runs
`docker compose exec edge /app/node --migrate-to core:8080` to trigger a live
migration demo (see §8 for what to look for in the logs).

To get a shell with `mielinctl` available instead of the `node` binary:

```bash
docker compose up -d dev
docker compose exec dev bash
```

### 6.2 Two real gotchas in the current compose file

> **Implementation note.** These are read directly from
> `examples/mesh-cluster/src/main.rs` and `docker-compose.yml`, not speculation —
> confirm against your own build before depending on cross-container connectivity:
>
> 1. **`--connect core:8080`/`--connect relay:8080` use Docker service hostnames,
>    but the binary parses them with `str::parse::<SocketAddr>()`**
>    (`MeshNode::connect_to_peer`, `examples/mesh-cluster/src/main.rs`).
>    `SocketAddr`'s `FromStr` implementation requires a literal IP address and does
>    **not** perform DNS resolution — there is no `to_socket_addrs()` call anywhere
>    in this binary or in `mielin_mesh_wire::transport::QuicTransport::connect`
>    (which takes a `SocketAddr`, not a hostname). A hostname like `core:8080`
>    fails to parse; the failure is caught and only logged
>    (`warn!("Failed to connect to peer {addr}: {e}")`), so the container keeps
>    running but the connection attempt does not succeed.
>    `MeshNode::new`'s `bootstrap_addrs` filtering has the same behavior — it
>    silently drops any address it can't parse as a `SocketAddr` via
>    `.filter_map(...ok()...)`.
>    2. **The QUIC listener is hard-bound to loopback regardless of `--port`'s
>    intent to be reachable from other containers**: `MeshNode::new` builds
>    `bind_addr` as `format!("127.0.0.1:{}", bind_port)` unconditionally. A
>    listener bound to `127.0.0.1` inside a container only accepts traffic
>    arriving via that container's own loopback interface, not traffic arriving on
>    its bridge-network interface (`eth0`) from a sibling container — so even a
>    literal-IP `--connect` would need to target the peer's bridge-network IP, and
>    the peer would need to be listening on more than `127.0.0.1` to accept it.
>
> Net effect: as currently wired, `docker compose up -d core relay edge` brings up
> three independent, healthy-per-healthcheck containers, but the `--connect`
> arguments do not establish real inter-container QUIC sessions. Use this compose
> file as a topology template and a single-container build/run exercise; treat
> `docs/CLUSTER_TESTING.md`'s gossip/migration walkthroughs as the target behavior
> to verify against your own build, not a guaranteed-passing default. If you need
> a verified working multi-node QUIC mesh today, run `mesh-cluster` directly on a
> single Docker network namespace or on bare-metal/VM hosts with literal IPs
> (§4.4), or fix the two issues above upstream.
>
> Separately: `Dockerfile`'s `runtime` stage only `EXPOSE`s `8080/udp` (matching
> QUIC-over-UDP), while `docker-compose.yml`'s `ports:` entries (e.g. `"8080:8080"`)
> publish the default **TCP** mapping. If you need to reach a containerized node's
> QUIC port from outside the compose network, publish it explicitly as UDP
> (`"8080:8080/udp"`).

### 6.3 What Is reliable in the compose setup

- Each container builds and boots correctly, passes its `nc -z localhost 8080`
  healthcheck, and its logs show the expected mDNS-independent `MeshService`
  startup sequence (gossip/registry/migration all report started, since
  `mesh-cluster` also enables `enable_gossip`/`enable_registry`/`enable_migration`
  — only `enable_mdns` is turned off in `examples/mesh-cluster/src/main.rs`).
- `docker compose logs`, `docker compose ps`, `docker compose exec <svc> <cmd>`,
  and `docker compose stop/start <svc>` (used for the failure-injection scenarios
  in `docs/CLUSTER_TESTING.md`) all work as documented, since those are plain
  Docker Compose primitives independent of the mesh-connectivity issue above.
- `docker compose exec dev bash` plus a from-source `cargo build -p mielin-cli`
  gives you a working `mielinctl daemon` (§3) inside the compose network, which
  does not have the loopback-bind issue described above (its `--listen` flag
  binds exactly the address you pass, e.g. `0.0.0.0:8080`).

---

## 7. Configuration Reference

`mielinctl` reads a single TOML config file, resolved by `dirs::config_dir()`
(`mielin-cli/src/config.rs`):

- Default path: `~/.config/mielin/config.toml` (Linux/XDG; platform-appropriate
  config dir elsewhere via the `dirs` crate).
- Created automatically with defaults the first time it's read if missing.
- Supports `${VAR}` and `${VAR:-default}` template substitution against process
  environment variables before TOML parsing (`Config::process_template`).

> **Implementation note.** This file is the source of truth for
> `mielinctl node config`/`mielinctl config validate|init|show|migrate|auto-fix`,
> but **`mielinctl daemon` does not read it at all** — every daemon startup
> parameter comes from CLI flags only (§3.1), and the `[daemon]` toggles below have
> no effect on a running daemon today. Don't put production topology in
> `config.toml` expecting `mielinctl daemon` to honor it — use the flags.

### 7.1 Schema

```toml
[node]
id = "..."                       # optional String; auto-generated at runtime if unset
role = "edge"                    # "core" | "relay" | "edge"
listen_address = "0.0.0.0:8080"
bootstrap_nodes = []              # array of "ip:port" strings

[cli]
default_output_format = "table"  # "table" | "json" | "yaml" | "quiet"
enable_colors = true
command_timeout_secs = 30

[daemon]
enable_mdns = true
enable_gossip = true
enable_registry = true
enable_migration = true
```

### 7.2 Environment variable overrides

Applied by `Config::apply_env_overrides` on every load, in this precedence order:
file → env var. Only the keys below have an env-var form (others are file/CLI
only):

| Env var | Overrides |
|---|---|
| `MIELIN_NODE_ID` | `node.id` |
| `MIELIN_NODE_ROLE` | `node.role` |
| `MIELIN_NODE_LISTEN_ADDRESS` | `node.listen_address` |
| `MIELIN_NODE_BOOTSTRAP_NODES` | `node.bootstrap_nodes` (comma-separated) |
| `MIELIN_CLI_DEFAULT_OUTPUT_FORMAT` | `cli.default_output_format` |
| `MIELIN_CLI_ENABLE_COLORS` | `cli.enable_colors` |
| `MIELIN_CLI_COMMAND_TIMEOUT_SECS` | `cli.command_timeout_secs` |
| `MIELIN_DAEMON_ENABLE_MDNS` | `daemon.enable_mdns` |
| `MIELIN_DAEMON_ENABLE_GOSSIP` | `daemon.enable_gossip` |
| `MIELIN_DAEMON_ENABLE_REGISTRY` | `daemon.enable_registry` |
| `MIELIN_DAEMON_ENABLE_MIGRATION` | `daemon.enable_migration` |

Separately, `MIELIN_DAEMON` (note: different variable — not one of the
`Config`-bound ones above) sets the default `--daemon <addr>` target for
`mielinctl mesh status`/`mielinctl mesh peers` (`commands/mesh.rs`).

### 7.3 CLI-managed operations on this file

```bash
mielinctl config init [-p <path>] [-f|--force]        # write out Config::default()
mielinctl config show                                  # print path/exists/size/mtime
mielinctl config validate [-f <path>] [-s]              # structural + semantic checks
mielinctl config auto-fix [-f <path>] [-d|--dry-run]     # fix common issues in place
mielinctl config migrate -f <from> [-t <to>] -v <ver>    # version migration (only "0.0.1" recognized)

mielinctl node config --all                              # dump the whole file as TOML
mielinctl node config <key>                               # get one key
mielinctl node config <key> <value>                       # set one key and save
```

Gettable/settable keys for `node config` (`Config::get`/`Config::set`):
`node.id`, `node.role`, `node.listen_address`, `cli.default_output_format`,
`cli.enable_colors`, `cli.command_timeout_secs`, `daemon.enable_mdns`,
`daemon.enable_gossip`, `daemon.enable_registry`, `daemon.enable_migration`.
`node.bootstrap_nodes` is **set-only** — `Config::set` accepts a comma-separated
list for it, but `Config::get` has no match arm for that key, so
`mielinctl node config node.bootstrap_nodes` returns "Unknown configuration key."

`mielinctl config validate` checks, among other things (`config_validator.rs`):
role must be one of `core`/`relay`/`edge`; `listen_address` and every
`bootstrap_nodes[i]` must parse as a `SocketAddr`; `daemon.enable_registry`
without `daemon.enable_gossip` is an error ("Gossip must be enabled when registry
is enabled"); `cli.command_timeout_secs == 0` is an error, `<5` or `>300` is a
warning; `cli.default_output_format` must be one of `table`/`json`/`yaml`/`quiet`.

### 7.4 Node identity is not persisted

`mielinctl daemon` always calls `Node::new(role)`, which generates a fresh random
UUID v4 (`mielin-mesh/core/src/node.rs`) — it never reads `node.id` from
`config.toml`. Restarting a daemon process therefore gives it a brand-new
`NodeId`; peers see it as an entirely new member, not a rejoin. Plan cluster
restarts (and any external inventory keyed by node ID) accordingly.

### 7.5 Remote node registry (separate file, separate feature)

`mielinctl remote` manages a different file — `remote_nodes.toml`, at
`dirs::config_dir()/mielin/remote_nodes.toml` (`mielin-cli/src/remote.rs`). It's
unrelated to mesh peer discovery: it's an address book of nodes the *CLI* can send
HTTP commands to via `POST /api/v1/command` with `none`/`apikey:<KEY>`/
`token:<TOKEN>` auth. See `mielinctl remote add/list/test/execute/import/export`.

---

## 8. Health Checks, Monitoring & Basic Operations

### 8.1 Control-plane HTTP API (real, from `mielin-cli/src/control/server.rs`)

Every `mielinctl daemon` exposes these `GET` routes on `--control-listen`:

| Route | Returns |
|---|---|
| `/api/v1/health` | `{ ok, version, node_id }` — liveness probe |
| `/api/v1/mesh/status` | `{ alive, suspect, dead, dht_peers, local_agents }` — gossip counts (`get_member_stats`), `dht_peer_count`, `local_agent_count`; sub-call failures fall back to `0` rather than erroring |
| `/api/v1/mesh/peers` | `[{ node_id, status, last_seen_secs }, ...]` — alive gossip members |
| `/api/v1/mesh/nodes` | Alias of `/api/v1/mesh/peers` |
| `/api/v1/agents` | `[{ agent_id, node_id, address }, ...]` — one synthetic entry per local agent slot (count only; no real per-agent metadata yet) |
| `/api/v1/migrate/status` | `{ total_migrations, active_migrations, successful, failed }` |

There is currently no `/metrics` (Prometheus) endpoint on the control server.
`mielin_mesh_core::export::{PrometheusExporter, JsonExporter}`
(`mielin-mesh/core/src/export.rs`) exist and are unit-tested as library-level
formatters for `MetricsSummary`, but nothing in `mielin-cli` or `ControlServer`
calls them yet — they're available for embedding into custom monitoring tooling,
not a wired-up scrape endpoint.

### 8.2 CLI wrapper (the two live subcommands)

```bash
mielinctl mesh status --daemon <control_addr>
mielinctl mesh peers  --daemon <control_addr>
# or: export MIELIN_DAEMON=<control_addr>; mielinctl mesh status
```

`ControlClient` (`mielin-cli/src/control/client.rs`) only implements
`health()`, `mesh_status()`, and `mesh_peers()` — the other four routes above
(`mesh/nodes`, `agents`, `migrate/status`) are reachable with `curl` today but have
no corresponding CLI subcommand yet.

### 8.3 Scripted health check across a small cluster

Since `mielinctl` has no built-in "check all my nodes" command, loop over your
known control-plane addresses directly:

```bash
for addr in 127.0.0.1:8081 127.0.0.1:8082 127.0.0.1:8083; do
  echo "== $addr =="
  curl -sf "http://${addr}/api/v1/health" && echo
  mielinctl mesh status --daemon "$addr"
done
```

### 8.4 Docker Compose operations

```bash
./scripts/cluster.sh status          # docker compose ps + per-service health column
docker compose ps                    # raw compose status
docker stats                         # live CPU/mem per container
./scripts/cluster.sh logs [node]     # docker compose logs -f [core|relay|edge]
```

Healthchecks in `docker-compose.yml` are `nc -z localhost 8080` — they confirm a
listener is bound, not that the mesh has converged (see §6.2 for why
inter-container convergence isn't guaranteed today).

### 8.5 Everything else under `mielinctl monitor`/`gossip`/`registry`

`mielinctl monitor top|watch|events|dashboard`, `mielinctl gossip status|members|sync`,
and `mielinctl registry list|query|stats` all have real, well-formed flag surfaces
(interval/iterations/filters/output overrides) but currently render local,
fabricated data — `monitor top`/`monitor events` explicitly use `rand::rng()` to
synthesize CPU/memory/event values (`commands/monitor.rs`). Use §8.1–8.3 for
signal you can trust about an actual running cluster today.

---

## 9. Scaling & Teardown

### 9.1 Adding nodes

- **Bare-metal/VM/LAN, mDNS:** just start another `mielinctl daemon` on the same
  broadcast domain — no bootstrap flag required (§4.1). Use distinct `--listen`
  and `--control-listen` ports if colocating multiple daemons on one host.
- **Docker Compose:** add another service block to `docker-compose.yml` following
  the `edge` service's shape (own `container_name`/`hostname`, unique host port,
  `depends_on` whichever node it should reach), matching
  `docs/CLUSTER_TESTING.md`'s suggested next step ("Scale Testing: Add more nodes
  by modifying `docker-compose.yml`") — subject to the §6.2 caveats about
  cross-container connectivity as currently wired.
- **`mesh-cluster` demo:** add more `--connect <ip:port>` peers/terminals per
  `examples/mesh-cluster/README.md`'s multi-hop scenarios.

Remember §7.4: every new (or restarted) `mielinctl daemon` gets a fresh `NodeId`,
so "adding a node" and "restarting a node" look identical to gossip/DHT.

### 9.2 Removing a node / graceful shutdown

- **`mielinctl daemon`:** `Ctrl+C` — the process traps it via
  `tokio::signal::ctrl_c()` and calls `MeshService::stop()`, which stops the
  discovery service (including deregistering/shutting down the mDNS daemon) before
  the process exits (`commands/daemon.rs`).
- **`mesh-cluster` / Docker:** the container has no signal handler beyond the
  process default; `docker compose stop <service>` sends `SIGTERM` then
  `SIGKILL` after the compose stop timeout. This is exactly the mechanism
  `docs/CLUSTER_TESTING.md`'s failure-injection scenario uses
  (`docker compose stop relay`) to exercise gossip's suspect/dead transitions.

### 9.3 Full teardown

```bash
./scripts/cluster.sh stop     # docker compose down
./scripts/cluster.sh clean    # docker compose down -v --rmi all (prompts for confirmation)
```

For a from-source deployment, teardown is just stopping each `mielinctl daemon`
process; there is no persisted cluster state to clean up beyond your own
`config.toml`/`remote_nodes.toml` (§7).

---

## See Also

- [`./RUNNING.md`](./RUNNING.md) — building and booting the `mielin-kernel`
  bare-metal image under QEMU (a different artifact from the user-space daemon
  covered here)
- [`./CLUSTER_TESTING.md`](./CLUSTER_TESTING.md) — the detailed 3-node Docker
  cluster testing walkthrough this guide's §6 summarizes and grounds
- [`./NETWORKING.md`](./NETWORKING.md) — full detail on the DHT, gossip protocol,
  discovery backends, partition handling, and multi-region/multi-tenancy layers
  underneath `MeshService`
- [`./TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) — diagnosing cluster and CLI
  issues
- [`./CERTIFICATES.md`](./CERTIFICATES.md) — TLS/mTLS certificate lifecycle for
  the QUIC mesh wire layer (generation, rotation, ACME, pinning)
