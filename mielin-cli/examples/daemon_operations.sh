#!/bin/bash
# MielinOS CLI - Daemon Operations Examples
# This file demonstrates daemon mode for running MielinOS nodes

# 1. Start daemon as core node
echo "=== Start daemon (core node) ==="
mielinctl daemon \
    --listen-addr 0.0.0.0:7000 \
    --role core

# 2. Start daemon as relay node
echo -e "\n=== Start daemon (relay node) ==="
mielinctl daemon \
    --listen-addr 0.0.0.0:7001 \
    --role relay \
    --bootstrap tcp://192.168.1.100:7000

# 3. Start daemon as edge node
echo -e "\n=== Start daemon (edge node) ==="
mielinctl daemon \
    --listen-addr 0.0.0.0:7002 \
    --role edge \
    --bootstrap tcp://192.168.1.100:7000,tcp://192.168.1.101:7001

# 4. Start daemon with custom node ID
echo -e "\n=== Start daemon with custom ID ==="
mielinctl daemon \
    --node-id my-custom-node \
    --listen-addr 0.0.0.0:7000 \
    --role core

# 5. Production deployment
echo -e "\n=== Production deployment ==="

# Core node (primary)
mielinctl daemon \
    --node-id core-01 \
    --listen-addr 0.0.0.0:7000 \
    --role core \
    > /var/log/mielin/core-01.log 2>&1 &

# Wait for core to start
sleep 5

# Relay nodes (for scalability)
for i in {1..3}; do
    mielinctl daemon \
        --node-id relay-0$i \
        --listen-addr 0.0.0.0:700$i \
        --role relay \
        --bootstrap tcp://localhost:7000 \
        > /var/log/mielin/relay-0$i.log 2>&1 &
done

# Wait for relays to join
sleep 10

# Edge nodes (for workloads)
for i in {1..5}; do
    mielinctl daemon \
        --node-id edge-0$i \
        --listen-addr 0.0.0.0:710$i \
        --role edge \
        --bootstrap tcp://localhost:7001,tcp://localhost:7002 \
        > /var/log/mielin/edge-0$i.log 2>&1 &
done

echo "Cluster deployed successfully"

# 6. Systemd service example
echo -e "\n=== Systemd service file example ==="
cat > /tmp/mielinctl-daemon.service << 'EOF'
[Unit]
Description=MielinOS Node Daemon
After=network.target

[Service]
Type=simple
User=mielin
Group=mielin
ExecStart=/usr/local/bin/mielinctl daemon \
    --listen-addr 0.0.0.0:7000 \
    --role core
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

echo "Service file created: /tmp/mielinctl-daemon.service"

# 7. Docker deployment
echo -e "\n=== Docker deployment ==="
cat > /tmp/Dockerfile << 'EOF'
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/mielinctl /usr/local/bin/
EXPOSE 7000
CMD ["mielinctl", "daemon", "--listen-addr", "0.0.0.0:7000", "--role", "core"]
EOF

echo "Dockerfile created: /tmp/Dockerfile"

# Build and run
# docker build -t mielinctl:latest -f /tmp/Dockerfile .
# docker run -p 7000:7000 mielinctl:latest

# 8. Graceful shutdown handling
echo -e "\n=== Graceful shutdown ==="
# Daemon handles SIGINT (Ctrl+C) and SIGTERM automatically
# Just send the signal:
# kill -TERM <PID>

# 9. Monitor daemon
echo -e "\n=== Monitor daemon ==="
# In another terminal:
# tail -f /var/log/mielin/core-01.log
# mielinctl node list
# mielinctl mesh status

# 10. Environment variables
echo -e "\n=== Using environment variables ==="
export MIELIN_NODE_ID="env-node-01"
export MIELIN_NODE_ROLE="core"
export MIELIN_NODE_LISTEN_ADDRESS="0.0.0.0:7000"

mielinctl daemon
# Will use environment variables if CLI flags not provided
