#!/bin/bash
# MielinOS CLI - Debugging Operations Examples
# This file demonstrates agent debugging workflows

# 1. Attach debugger to agent
echo "=== Attach debugger ==="
mielinctl debug attach agent-001
# Alias: mielinctl debug connect agent-001

# Attach with custom port
mielinctl debug attach agent-001 --port 9229

# 2. Trace agent execution
echo -e "\n=== Trace execution ==="
mielinctl debug trace agent-001 --duration 30
# Alias: mielinctl debug track agent-001 --duration 30

# Trace with output file
mielinctl debug trace agent-001 --duration 60 --file trace-output.json

# 3. Dump agent state
echo -e "\n=== Dump agent state ==="
mielinctl debug dump agent-001
# Alias: mielinctl debug snapshot agent-001

# Dump with memory details
mielinctl debug dump agent-001 --memory

# 4. Show profiling data
echo -e "\n=== Show profiling data ==="
mielinctl debug profile agent-001
# Alias: mielinctl debug perf agent-001

# Profile CPU usage
mielinctl debug profile agent-001 --cpu

# Profile memory usage
mielinctl debug profile agent-001 --mem

# Profile both
mielinctl debug profile agent-001 --cpu --mem

# 5. Complete debugging workflow
echo -e "\n=== Complete debugging workflow ==="

AGENT_ID="agent-001"

echo "Step 1: Check agent status"
mielinctl agent inspect "$AGENT_ID"

echo "Step 2: Dump current state"
mielinctl debug dump "$AGENT_ID" --memory > "/tmp/${AGENT_ID}-dump.json"

echo "Step 3: Start tracing"
mielinctl debug trace "$AGENT_ID" --duration 30 --file "/tmp/${AGENT_ID}-trace.json" &
TRACE_PID=$!

echo "Step 4: Collect profiling data"
sleep 5
mielinctl debug profile "$AGENT_ID" --cpu --mem > "/tmp/${AGENT_ID}-profile.json"

echo "Step 5: Wait for trace to complete"
wait $TRACE_PID

echo "Step 6: Analyze results"
echo "Dump: /tmp/${AGENT_ID}-dump.json"
echo "Trace: /tmp/${AGENT_ID}-trace.json"
echo "Profile: /tmp/${AGENT_ID}-profile.json"

# 6. Debug performance issue
echo -e "\n=== Debug performance issue ==="

# Check if agent is using too much CPU
CPU_USAGE=$(mielinctl debug profile agent-001 --cpu --output json | jq -r '.cpu_percent')

if [ "$CPU_USAGE" -gt 80 ]; then
    echo "High CPU usage detected: ${CPU_USAGE}%"
    echo "Starting detailed trace..."
    mielinctl debug trace agent-001 --duration 60 --file high-cpu-trace.json

    echo "Dumping state for analysis..."
    mielinctl debug dump agent-001 --memory > high-cpu-dump.json
fi

# 7. Debug memory leak
echo -e "\n=== Debug memory leak ==="

# Collect memory snapshots over time
for i in {1..5}; do
    echo "Snapshot $i/5"
    mielinctl debug dump agent-001 --memory > "/tmp/memleak-snapshot-$i.json"
    sleep 60
done

echo "Analyzing memory growth..."
# Compare snapshots (would need jq processing)

# 8. Remote debugging setup
echo -e "\n=== Remote debugging setup ==="

# Attach debugger and get connection URL
DEBUG_URL=$(mielinctl debug attach agent-001 --port 9229 --output json | jq -r '.debugger_url')
echo "Connect your debugger to: $DEBUG_URL"

# Keep debugger attached
echo "Press Ctrl+C to detach debugger"
tail -f /dev/null

# 9. Debugging with different output formats
echo -e "\n=== Different output formats ==="
mielinctl debug profile agent-001 --output json
mielinctl debug profile agent-001 --output yaml
mielinctl debug profile agent-001 --output table
