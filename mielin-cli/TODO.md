# mielin-cli TODO

## Pending Tasks

### High Priority
- [x] Implement actual node connection — HTTP control plane (axum /api/v1/…) wired; ControlClient talks to live daemon with graceful mock fallback
- [x] Implement actual mesh status fetching — ControlClient.mesh_status() reads live MeshService; `mielinctl mesh status --daemon <addr>` returns real counts
- [ ] **HONESTY NOTE (2026-07-11, /ucont doc-grounding):** the two items above are only PARTIALLY true. Only `mesh status/peers --daemon <addr>` and `node config` actually talk to the daemon. `agent deploy/create/migrate/stop/list/inspect` make **no network call** (they mint a UUID + print a mock `OperationResult`; `commands/agent.rs` never imports `ControlClient`, and the axum server registers only GET routes — no POST deploy/migrate endpoint exists). Most `node/cluster/registry/gossip/migrate` subcommands render hard-coded or `rand`-generated data. Wiring these to the live control plane (add POST routes + real `ControlClient` calls; persist node identity) is real remaining work — see root `TODO.md` → "Advertised-vs-actual gaps discovered".

### Low Priority
- [ ] Video tutorials for CLI usage

## Completed Features (v0.1.0-rc.1)

The following major features have been implemented and are production-ready:

### Core CLI Commands
- ✅ Agent management (list, inspect, deploy, remove, migrate)
- ✅ Mesh management (nodes, status, topology)
- ✅ Monitoring commands (metrics, logs, health)
- ✅ Configuration management
- ✅ Script execution support (Rhai integration)
- ✅ Plugin system

### Infrastructure
- ✅ Command-line parsing (clap)
- ✅ Shell completion generation
- ✅ Interactive REPL mode (rustyline)
- ✅ Configuration file support (TOML, YAML, JSON)
- ✅ Output formatting (JSON, YAML, table, pretty)
- ✅ Remote operations support
- ✅ History tracking

### Testing & Quality
- ✅ Comprehensive test suite
- ✅ Integration tests
- ✅ Benchmark suite
- ✅ Documentation

For detailed feature descriptions and API documentation, see [README.md](README.md) and the generated rustdoc.
