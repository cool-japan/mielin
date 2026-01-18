# MielinOS CLI Documentation

Complete documentation for `mielinctl`, the command-line interface for MielinOS.

## Getting Started

### New to MielinOS CLI?

Start here:
1. **[Quickstart Guide](./QUICKSTART.md)** - Get up and running in 5 minutes
2. **[Command Reference](./COMMAND_REFERENCE.md)** - Complete command documentation
3. **[Examples](../examples/)** - Runnable script examples

### Documentation Index

| Document | Description | Audience |
|----------|-------------|----------|
| **[Quickstart Guide](./QUICKSTART.md)** | Installation, first steps, and basic operations | Beginners |
| **[Command Reference](./COMMAND_REFERENCE.md)** | Complete command documentation with all flags and options | All users |
| **[Common Workflows](./WORKFLOWS.md)** | Real-world workflows and best practices | Intermediate to Advanced |
| **[Troubleshooting Guide](./TROUBLESHOOTING.md)** | Solutions to common problems and debugging techniques | All users |

## Quick Navigation

### By Task

**I want to...**

- **Install mielinctl** → [Quickstart: Installation](./QUICKSTART.md#installation)
- **Start my first node** → [Quickstart: Basic Operations](./QUICKSTART.md#basic-operations)
- **Learn all commands** → [Command Reference](./COMMAND_REFERENCE.md)
- **Deploy an agent** → [Workflows: Agent Deployment](./WORKFLOWS.md#agent-deployment)
- **Set up a cluster** → [Workflows: Cluster Management](./WORKFLOWS.md#cluster-management)
- **Debug a problem** → [Troubleshooting Guide](./TROUBLESHOOTING.md)
- **See code examples** → [Examples Directory](../examples/)

### By Experience Level

**Beginner:**
1. Read [Quickstart Guide](./QUICKSTART.md)
2. Try [Basic Examples](../examples/node_operations.sh)
3. Check [Troubleshooting](./TROUBLESHOOTING.md) when stuck

**Intermediate:**
1. Review [Common Workflows](./WORKFLOWS.md)
2. Explore [Advanced Examples](../examples/advanced_workflows.sh)
3. Use [Command Reference](./COMMAND_REFERENCE.md) as needed

**Advanced:**
1. Study [Production Operations](./WORKFLOWS.md#production-operations)
2. Run [Performance Benchmarks](../benches/)
3. Contribute to [GitHub](https://github.com/mielinos/mielin)

## Document Overview

### Quickstart Guide

**Target Audience:** New users
**Length:** ~15 minutes to read and complete
**Content:**
- Installation (from source, crates.io, binaries)
- First steps (help, completion, configuration)
- Basic operations (nodes, agents, clusters)
- Example workflows
- Tips & tricks
- Quick reference card

**Best for:** Getting started quickly without deep background

### Command Reference

**Target Audience:** All users
**Length:** Comprehensive reference (~50 pages printed)
**Content:**
- All commands with syntax
- All flags and options
- Command aliases
- Environment variables
- Exit codes
- Examples for each command

**Best for:** Looking up specific command syntax and options

### Common Workflows

**Target Audience:** Intermediate to advanced users
**Length:** ~20-30 minutes per workflow
**Content:**
- Cluster management (bootstrap, scale, upgrade)
- Agent deployment (simple, blue-green, canary)
- Development workflows (local dev, testing)
- Production operations (monitoring, backup, logging)
- Debugging techniques
- Migration scenarios

**Best for:** Learning real-world patterns and best practices

### Troubleshooting Guide

**Target Audience:** All users
**Length:** Reference guide, use as needed
**Content:**
- Installation issues
- Connection problems
- Agent issues
- Cluster problems
- Performance issues
- Configuration problems
- Platform-specific issues
- Diagnostic commands

**Best for:** Solving specific problems you're encountering

## Additional Resources

### Code Examples

The [examples directory](../examples/) contains:
- 11 shell scripts covering all command categories
- Runnable code you can copy and adapt
- Real-world scenarios
- Best practices embedded in code

**Start with:**
- [node_operations.sh](../examples/node_operations.sh) - Node management basics
- [agent_operations.sh](../examples/agent_operations.sh) - Agent lifecycle
- [advanced_workflows.sh](../examples/advanced_workflows.sh) - Complex scenarios

### Performance Benchmarks

The [benches directory](../benches/) contains:
- Criterion-based benchmarks for critical operations
- Performance targets and optimization tips
- Instructions for running benchmarks

**Useful for:**
- Understanding performance characteristics
- Identifying bottlenecks
- Regression testing

### Integration Tests

The [tests directory](../tests/) contains:
- 68 automated tests (17 unit + 1 CLI + 50 integration)
- Test all major commands
- Verify output formats
- Validate error handling

**Useful for:**
- Understanding expected behavior
- Verifying your installation
- Contributing to development

## Learning Paths

### Path 1: Developer Quick Start (30 minutes)

1. Install mielinctl → [Quickstart](./QUICKSTART.md#installation)
2. Start local node → [Quickstart: Daemon Mode](./QUICKSTART.md#starting-a-mielinos-node)
3. Deploy test agent → [Workflows: Deploy Simple Agent](./WORKFLOWS.md#deploy-a-simple-agent)
4. View logs and debug → [Command Reference: Agent Logs](./COMMAND_REFERENCE.md#agent-logs)

### Path 2: Production Deployment (2-3 hours)

1. Plan cluster topology
2. Bootstrap cluster → [Workflows: Bootstrap Production Cluster](./WORKFLOWS.md#bootstrap-a-production-cluster)
3. Set up monitoring → [Workflows: Health Monitoring](./WORKFLOWS.md#health-monitoring)
4. Configure backups → [Workflows: Backup and Restore](./WORKFLOWS.md#backup-and-restore)
5. Test disaster recovery

### Path 3: Advanced Operations (ongoing)

1. Master all commands → [Command Reference](./COMMAND_REFERENCE.md)
2. Implement all workflows → [Common Workflows](./WORKFLOWS.md)
3. Optimize performance → [Benchmarks](../benches/)
4. Contribute improvements → [GitHub](https://github.com/mielinos/mielin)

## Command Categories

Quick reference to command categories:

| Category | Commands | Purpose |
|----------|----------|---------|
| **Node** | list, info, create, start, stop, join, leave, config | Manage cluster nodes |
| **Agent** | list, inspect, create, deploy, migrate, stop, logs, exec | Manage WASM agents |
| **Cluster** | init, status, health, upgrade | Cluster-wide operations |
| **Mesh** | status, peers | Network topology |
| **Migration** | agent, status, cancel, history | Agent migration |
| **Registry** | list, query, stats | Agent discovery |
| **Gossip** | status, members, sync | Cluster membership |
| **WASM** | build, validate, test, optimize | Development tools |
| **Debug** | attach, trace, dump, profile | Debugging tools |
| **Utility** | daemon, version, completion | Miscellaneous |

## Tips for Using Documentation

### 1. Start with Quickstart

Even if you're experienced, the [Quickstart Guide](./QUICKSTART.md) will familiarize you with MielinOS-specific concepts and commands.

### 2. Keep Command Reference Handy

Bookmark the [Command Reference](./COMMAND_REFERENCE.md) for quick lookups. Use your browser's search function (Ctrl+F / Cmd+F) to find specific commands.

### 3. Learn from Workflows

The [Workflows Guide](./WORKFLOWS.md) shows you not just what commands to run, but when and why to run them.

### 4. Use Examples as Templates

The [examples scripts](../examples/) are designed to be copied and modified for your use case.

### 5. Troubleshooting First

When something goes wrong, check the [Troubleshooting Guide](./TROUBLESHOOTING.md) before asking for help. Most common issues are covered.

## Shell Completion

Install shell completion for faster command-line use:

```bash
# Bash
mielinctl completion bash > /etc/bash_completion.d/mielinctl

# Zsh
mielinctl completion zsh > /usr/local/share/zsh/site-functions/_mielinctl

# Fish
mielinctl completion fish > ~/.config/fish/completions/mielinctl.fish
```

After installation, press `Tab` to autocomplete commands, flags, and even some argument values.

## Getting Help

### Built-in Help

```bash
# General help
mielinctl --help

# Command-specific help
mielinctl node --help
mielinctl agent create --help
```

### Community Support

- **GitHub Issues**: https://github.com/mielinos/mielin/issues
- **Documentation**: https://docs.mielinos.org
- **Discord**: https://discord.gg/mielinos
- **Stack Overflow**: Tag `mielinos`

### Before Asking for Help

1. Check the [Troubleshooting Guide](./TROUBLESHOOTING.md)
2. Search [existing issues](https://github.com/mielinos/mielin/issues)
3. Review [command reference](./COMMAND_REFERENCE.md)
4. Try the diagnostic script:

```bash
# Collect diagnostic information
mielinctl --version
mielinctl node config --all
mielinctl cluster status
```

## Contributing to Documentation

Documentation improvements are welcome!

**Found an error?** Open an issue.
**Want to add an example?** Submit a pull request.
**Have a workflow to share?** We'd love to include it.

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## Version Information

This documentation is for **mielinctl version 0.1.0-rc.1**.

For latest documentation, see: https://docs.mielinos.org

## Document Status

| Document | Status | Last Updated |
|----------|--------|--------------|
| Quickstart Guide | ✅ Complete | 2026-01-18 |
| Command Reference | ✅ Complete | 2026-01-18 |
| Common Workflows | ✅ Complete | 2026-01-18 |
| Troubleshooting Guide | ✅ Complete | 2026-01-18 |
| Examples | ✅ Complete | 2026-01-18 |
| Benchmarks | ✅ Complete | 2026-01-18 |

## Quick Links

- **[GitHub Repository](https://github.com/mielinos/mielin)**
- **[Issue Tracker](https://github.com/mielinos/mielin/issues)**
- **[Release Notes](https://github.com/mielinos/mielin/releases)**
- **[API Documentation](https://docs.rs/mielin-cli)**

---

**Ready to get started?** Begin with the [Quickstart Guide](./QUICKSTART.md)!
