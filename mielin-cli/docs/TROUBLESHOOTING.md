# MielinOS CLI - Troubleshooting Guide

Solutions to common problems and debugging techniques.

## Table of Contents

- [Installation Issues](#installation-issues)
- [Connection Problems](#connection-problems)
- [Agent Issues](#agent-issues)
- [Cluster Problems](#cluster-problems)
- [Performance Issues](#performance-issues)
- [Configuration Problems](#configuration-problems)
- [Platform-Specific Issues](#platform-specific-issues)
- [Getting Help](#getting-help)

## Installation Issues

### Problem: `cargo install` fails

**Symptoms:**
```
error: failed to compile mielin-cli
```

**Solutions:**

1. **Update Rust toolchain:**
```bash
rustup update stable
```

2. **Check Rust version (requires 1.70+):**
```bash
rustc --version
```

3. **Install with specific features:**
```bash
cargo install mielin-cli --no-default-features
```

4. **Clean and retry:**
```bash
cargo clean
cargo install --path . --locked --force
```

### Problem: Binary not found after installation

**Symptoms:**
```
command not found: mielinctl
```

**Solutions:**

1. **Add cargo bin to PATH:**
```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.cargo/bin:$PATH"

# Reload shell
source ~/.bashrc
```

2. **Verify installation location:**
```bash
ls -la ~/.cargo/bin/mielinctl
```

3. **Install to custom location:**
```bash
cargo install --path . --root /usr/local
```

### Problem: Permission denied on Linux/macOS

**Symptoms:**
```
Permission denied (os error 13)
```

**Solutions:**

1. **Install to user directory (recommended):**
```bash
cargo install --path . --root ~/.local
export PATH="$HOME/.local/bin:$PATH"
```

2. **Or use sudo (not recommended for Cargo):**
```bash
sudo mv ~/.cargo/bin/mielinctl /usr/local/bin/
```

## Connection Problems

### Problem: Cannot connect to daemon

**Symptoms:**
```
Error: Connection refused
Could not connect to the MielinOS daemon
```

**Solutions:**

1. **Check if daemon is running:**
```bash
ps aux | grep mielinctl
```

2. **Start the daemon:**
```bash
mielinctl daemon --role core &
```

3. **Check listen address:**
```bash
# View current configuration
mielinctl node config node.listen_address

# Set correct address
mielinctl node config node.listen_address 0.0.0.0:7000
```

4. **Check firewall:**
```bash
# Linux (UFW)
sudo ufw allow 7000/tcp

# macOS
# System Preferences > Security & Privacy > Firewall

# Windows
netsh advfirewall firewall add rule name="MielinOS" dir=in action=allow protocol=TCP localport=7000
```

### Problem: Timeout errors

**Symptoms:**
```
Error: Operation timed out
```

**Solutions:**

1. **Increase timeout:**
```bash
# Via environment variable
export MIELIN_CLI_COMMAND_TIMEOUT_SECS=120

# Via configuration
mielinctl node config cli.command_timeout_secs 120
```

2. **Check network latency:**
```bash
ping -c 5 <node-address>
```

3. **Check if node is responsive:**
```bash
mielinctl mesh status
mielinctl node info <node-id>
```

### Problem: Cannot join cluster

**Symptoms:**
```
Error: Failed to join cluster
Bootstrap node not reachable
```

**Solutions:**

1. **Verify bootstrap node is running:**
```bash
# On bootstrap node
mielinctl node info <bootstrap-node-id>
```

2. **Check bootstrap address:**
```bash
# Use IP instead of hostname
mielinctl node join --bootstrap tcp://192.168.1.100:7000

# Check DNS resolution
nslookup bootstrap-node
```

3. **Verify network connectivity:**
```bash
telnet bootstrap-node 7000
# or
nc -zv bootstrap-node 7000
```

4. **Check bootstrap node logs:**
```bash
# View daemon logs
journalctl -u mielinctl -f
```

## Agent Issues

### Problem: Agent creation fails

**Symptoms:**
```
Error: Failed to create agent
WASM module validation failed
```

**Solutions:**

1. **Validate WASM module:**
```bash
mielinctl wasm validate agent.wasm
```

2. **Check file exists and is readable:**
```bash
ls -lh agent.wasm
file agent.wasm
```

3. **Verify WASM is correct format:**
```bash
# Should show "WebAssembly"
file agent.wasm

# Rebuild if needed
cargo build --target wasm32-wasi --release
```

4. **Check memory/CPU limits:**
```bash
# Increase limits if too restrictive
mielinctl agent create \
  --name my-agent \
  --wasm-file agent.wasm \
  --memory 512 \
  --cpu-shares 2048
```

### Problem: Agent stuck in "Paused" state

**Symptoms:**
```
State: Paused
Expected: Running
```

**Solutions:**

1. **Check agent logs:**
```bash
mielinctl agent logs <agent-id> --tail 100
```

2. **Inspect agent details:**
```bash
mielinctl agent inspect <agent-id> --output json
```

3. **Check node resources:**
```bash
mielinctl node info <node-id>
```

4. **Restart agent:**
```bash
mielinctl agent stop <agent-id>
sleep 5
mielinctl agent create --name <name> --wasm-file <file>
```

### Problem: Agent logs not showing

**Symptoms:**
```
No logs available
```

**Solutions:**

1. **Check if agent is running:**
```bash
mielinctl agent inspect <agent-id>
```

2. **Ensure agent is writing to stdout/stderr:**
```rust
// In your WASM code
println!("Log message");
eprintln!("Error message");
```

3. **Follow logs in real-time:**
```bash
mielinctl agent logs <agent-id> --follow
```

4. **Check log retention settings:**
```bash
mielinctl node config daemon.log_retention_days
```

### Problem: Agent migration fails

**Symptoms:**
```
Error: Migration failed
Agent state inconsistent
```

**Solutions:**

1. **Check migration status:**
```bash
mielinctl migrate status --agent <agent-id>
```

2. **Cancel stuck migration:**
```bash
mielinctl migrate cancel <migration-id>
```

3. **Try cold migration instead:**
```bash
mielinctl agent migrate <agent-id> --to <node-id> --strategy cold
```

4. **Check destination node has resources:**
```bash
mielinctl node info <destination-node>
```

5. **View migration history:**
```bash
mielinctl migrate history --limit 20
```

## Cluster Problems

### Problem: Cluster health check fails

**Symptoms:**
```
Cluster status: Unhealthy
Some nodes are not responding
```

**Solutions:**

1. **Identify unhealthy nodes:**
```bash
mielinctl node list --output json | jq -r '.[] | select(.state != "Running") | .id'
```

2. **Check individual node status:**
```bash
for node in $(mielinctl node list --quiet); do
  echo "Checking $node..."
  mielinctl node info $node
done
```

3. **Check gossip protocol:**
```bash
mielinctl gossip status
mielinctl gossip members --output json | jq -r '.[] | "\(.id): \(.state)"'
```

4. **Force gossip sync:**
```bash
mielinctl gossip sync
```

5. **Check for network partitions:**
```bash
mielinctl mesh status
mielinctl mesh peers
```

### Problem: Nodes not discovering each other

**Symptoms:**
```
Connected peers: 0
Cluster members: 1
```

**Solutions:**

1. **Check if mDNS is enabled:**
```bash
mielinctl node config daemon.enable_mdns
```

2. **Enable mDNS:**
```bash
export MIELIN_DAEMON_ENABLE_MDNS=true
mielinctl daemon --role core
```

3. **Use explicit bootstrap:**
```bash
mielinctl daemon \
  --role edge \
  --bootstrap tcp://192.168.1.100:7000,tcp://192.168.1.101:7001
```

4. **Check multicast firewall rules:**
```bash
# Linux
sudo iptables -A INPUT -p udp --dport 5353 -j ACCEPT

# macOS - ensure firewall allows mDNS
```

### Problem: Gossip protocol issues

**Symptoms:**
```
Gossip round: 0
Members: Suspect or Dead
```

**Solutions:**

1. **Check gossip status:**
```bash
mielinctl gossip status --output json
```

2. **Force synchronization:**
```bash
mielinctl gossip sync
```

3. **Restart gossip protocol:**
```bash
# Restart daemon with gossip enabled
mielinctl daemon --role core
```

4. **Check for dead members:**
```bash
DEAD=$(mielinctl gossip members --output json | jq -r '.[] | select(.state=="Dead") | .id')
if [ -n "$DEAD" ]; then
  echo "Dead members: $DEAD"
  mielinctl gossip sync
fi
```

## Performance Issues

### Problem: High memory usage

**Symptoms:**
```
Memory usage: 95%
Agents being evicted
```

**Solutions:**

1. **Identify high-memory agents:**
```bash
mielinctl agent list --output json | \
  jq -r 'sort_by(.memory_mb) | reverse | .[] | "\(.id): \(.memory_mb)MB"' | \
  head -10
```

2. **Profile memory usage:**
```bash
mielinctl debug profile <agent-id> --mem
```

3. **Reduce agent memory limits:**
```bash
# When creating new agents
mielinctl agent create \
  --name my-agent \
  --wasm-file agent.wasm \
  --memory 128
```

4. **Distribute load across nodes:**
```bash
# Migrate some agents to other nodes
mielinctl agent migrate <agent-id> --to <less-loaded-node>
```

5. **Add more nodes:**
```bash
mielinctl daemon --role edge --bootstrap tcp://core:7000
```

### Problem: Slow command execution

**Symptoms:**
```
Commands taking > 10 seconds
Frequent timeouts
```

**Solutions:**

1. **Check cluster load:**
```bash
mielinctl cluster status
mielinctl node list --output json | jq -r '.[] | "\(.id): \(.agents) agents"'
```

2. **Profile cluster performance:**
```bash
time mielinctl node list
time mielinctl agent list
```

3. **Check network latency:**
```bash
ping -c 10 <node-address>
```

4. **Reduce agent count per node:**
```bash
# Current distribution
mielinctl node list --output json | jq -r '.[] | "\(.id): \(.agents)"'

# Rebalance if needed
```

5. **Optimize WASM modules:**
```bash
mielinctl wasm optimize \
  --input agent.wasm \
  --output agent-opt.wasm \
  --level 3
```

### Problem: High CPU usage

**Symptoms:**
```
CPU usage: 90%+
System slow/unresponsive
```

**Solutions:**

1. **Identify CPU-intensive agents:**
```bash
for agent in $(mielinctl agent list --quiet); do
  mielinctl debug profile $agent --cpu
done
```

2. **Trace execution:**
```bash
mielinctl debug trace <agent-id> --duration 60 --file trace.json
```

3. **Reduce CPU shares:**
```bash
# Limit CPU for specific agents
# (requires recreating agent)
```

4. **Distribute load:**
```bash
# Add more edge nodes
mielinctl daemon --role edge --bootstrap tcp://core:7000 &
```

## Configuration Problems

### Problem: Configuration not persisting

**Symptoms:**
```
Settings reset after restart
Configuration changes not saved
```

**Solutions:**

1. **Check config file location:**
```bash
ls -la ~/.config/mielin/config.toml
```

2. **Create config directory if missing:**
```bash
mkdir -p ~/.config/mielin
```

3. **Verify config file permissions:**
```bash
chmod 644 ~/.config/mielin/config.toml
```

4. **Save configuration manually:**
```bash
mielinctl node config node.id my-node
# Verify
mielinctl node config node.id
```

5. **Check for environment variable overrides:**
```bash
env | grep MIELIN_
```

### Problem: Environment variables not working

**Symptoms:**
```
MIELIN_* variables ignored
Settings not applied
```

**Solutions:**

1. **Check variable names are correct:**
```bash
# Correct
export MIELIN_NODE_ID="my-node"

# Incorrect
export MIELIN_NODEID="my-node"
```

2. **Verify variables are exported:**
```bash
export MIELIN_NODE_ID="my-node"
env | grep MIELIN_NODE_ID
```

3. **Restart daemon after setting variables:**
```bash
export MIELIN_NODE_ROLE="core"
mielinctl daemon
```

4. **Use config file instead:**
```bash
mielinctl node config node.role core
```

## Platform-Specific Issues

### Linux Issues

**Problem: Systemd service won't start**

```bash
# Check service status
sudo systemctl status mielinctl

# View logs
sudo journalctl -u mielinctl -n 50

# Verify service file
cat /etc/systemd/system/mielinctl.service

# Reload systemd
sudo systemctl daemon-reload
sudo systemctl restart mielinctl
```

**Problem: SELinux blocking connections**

```bash
# Check if SELinux is enforcing
getenforce

# Temporarily disable (for testing)
sudo setenforce 0

# Add permanent rule
sudo setsebool -P nis_enabled 1
```

### macOS Issues

**Problem: "mielinctl" cannot be opened**

```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine /usr/local/bin/mielinctl

# Or allow in System Preferences
# System Preferences > Security & Privacy > Allow
```

**Problem: Port binding fails**

```bash
# Check if port is in use
lsof -i :7000

# Kill process using port
kill -9 $(lsof -t -i :7000)
```

### Windows Issues

**Problem: PowerShell execution policy**

```powershell
# Check current policy
Get-ExecutionPolicy

# Allow scripts
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
```

**Problem: Path not updated**

```powershell
# Add to PATH permanently
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";C:\Program Files\MielinOS", [EnvironmentVariableTarget]::Machine)

# Restart PowerShell
```

**Problem: Firewall blocking**

```powershell
# Add firewall rule
New-NetFirewallRule -DisplayName "MielinOS" -Direction Inbound -Protocol TCP -LocalPort 7000 -Action Allow
```

## Diagnostic Commands

### Collect diagnostic information

```bash
#!/bin/bash
# diagnose.sh - Collect diagnostic information

echo "=== System Information ==="
uname -a
cat /etc/os-release

echo "=== MielinCTL Version ==="
mielinctl --version

echo "=== Configuration ==="
mielinctl node config --all

echo "=== Cluster Status ==="
mielinctl cluster status

echo "=== Node List ==="
mielinctl node list --output json

echo "=== Agent List ==="
mielinctl agent list --output json

echo "=== Mesh Status ==="
mielinctl mesh status --output json

echo "=== Gossip Status ==="
mielinctl gossip status --output json

echo "=== Network Connectivity ==="
netstat -an | grep 7000

echo "=== Process Information ==="
ps aux | grep mielinctl

echo "=== Recent Logs ==="
# Platform specific
```

### Enable debug logging

```bash
# Set log level
export RUST_LOG=debug

# Run with debug output
mielinctl --verbose daemon --role core

# Or for specific modules
export RUST_LOG=mielin_cli=debug,mielin_mesh=info
```

## Getting Help

### Community Resources

- **GitHub Issues**: https://github.com/mielinos/mielin/issues
- **Documentation**: https://docs.mielinos.org
- **Discord**: https://discord.gg/mielinos
- **Stack Overflow**: Tag `mielinos`

### Reporting Bugs

When reporting issues, include:

1. **Environment Information:**
```bash
mielinctl --version
uname -a
```

2. **Configuration:**
```bash
mielinctl node config --all
```

3. **Error Messages:**
```bash
# Full error output
mielinctl <command> 2>&1
```

4. **Diagnostic Output:**
```bash
# Run diagnostic script
./diagnose.sh > diagnostic.txt
```

5. **Steps to Reproduce:**
- List exact commands run
- Include any error output
- Note expected vs actual behavior

### Quick Checklist

Before asking for help, verify:

- [ ] Using latest version: `mielinctl --version`
- [ ] Checked this troubleshooting guide
- [ ] Daemon is running: `ps aux | grep mielinctl`
- [ ] Network connectivity: `mielinctl mesh status`
- [ ] Firewall allows connections
- [ ] Configuration is correct: `mielinctl node config --all`
- [ ] Logs reviewed: `mielinctl agent logs <id>`
- [ ] Tried restarting daemon

### Advanced Debugging

For complex issues:

1. **Enable trace logging:**
```bash
export RUST_LOG=trace
export RUST_BACKTRACE=full
mielinctl daemon
```

2. **Use debugger:**
```bash
rust-lldb -- mielinctl daemon --role core
```

3. **Profile performance:**
```bash
cargo flamegraph -- mielinctl daemon
```

4. **Check for memory leaks:**
```bash
valgrind --leak-check=full mielinctl daemon
```

## See Also

- [Quickstart Guide](./QUICKSTART.md)
- [Command Reference](./COMMAND_REFERENCE.md)
- [Common Workflows](./WORKFLOWS.md)
- [Examples](../examples/)
