#!/bin/bash
# MielinOS CLI - WASM Development Operations Examples
# This file demonstrates WASM agent development workflow

# 1. Build WASM agent
echo "=== Build WASM agent ==="
mielinctl wasm build \
    --source ./agent-src \
    --output ./agent.wasm \
    --optimize
# Alias: mielinctl wasm compile

# Build without optimization (for development)
mielinctl wasm build --source ./agent-src --output ./agent-debug.wasm

# 2. Validate WASM module
echo -e "\n=== Validate WASM ==="
mielinctl wasm validate ./agent.wasm
# Alias: mielinctl wasm check ./agent.wasm

# 3. Test WASM agent locally
echo -e "\n=== Test WASM agent ==="
mielinctl wasm test ./agent.wasm
# Alias: mielinctl wasm run ./agent.wasm

# Test with arguments
mielinctl wasm test ./agent.wasm --args "arg1 arg2"

# 4. Optimize WASM module
echo -e "\n=== Optimize WASM ==="
mielinctl wasm optimize \
    --input ./agent.wasm \
    --output ./agent-optimized.wasm \
    --level 3
# Alias: mielinctl wasm opt

# 5. Complete development workflow
echo -e "\n=== Complete development workflow ==="

# Step 1: Build
echo "Building WASM agent..."
if mielinctl wasm build --source ./agent-src --output ./agent.wasm; then
    echo "Build successful"
else
    echo "Build failed"
    exit 1
fi

# Step 2: Validate
echo "Validating WASM module..."
if mielinctl wasm validate ./agent.wasm; then
    echo "Validation successful"
else
    echo "Validation failed"
    exit 1
fi

# Step 3: Test locally
echo "Testing WASM agent..."
if mielinctl wasm test ./agent.wasm; then
    echo "Test successful"
else
    echo "Test failed"
    exit 1
fi

# Step 4: Optimize
echo "Optimizing WASM module..."
if mielinctl wasm optimize --input ./agent.wasm --output ./agent-optimized.wasm; then
    echo "Optimization successful"
else
    echo "Optimization failed"
    exit 1
fi

# Step 5: Deploy
echo "Deploying agent..."
mielinctl agent create \
    --name my-agent \
    --wasm-file ./agent-optimized.wasm \
    --memory 256 \
    --cpu-shares 1024

# 6. Size comparison
echo -e "\n=== Size comparison ==="
echo "Original size: $(ls -lh ./agent.wasm | awk '{print $5}')"
echo "Optimized size: $(ls -lh ./agent-optimized.wasm | awk '{print $5}')"

# 7. Batch processing
echo -e "\n=== Batch build and deploy ==="
for src in ./agents/*/src; do
    name=$(basename $(dirname "$src"))
    echo "Processing $name..."

    mielinctl wasm build --source "$src" --output "./build/$name.wasm"
    mielinctl wasm validate "./build/$name.wasm"
    mielinctl wasm optimize --input "./build/$name.wasm" --output "./build/$name-opt.wasm"

    echo "$name ready for deployment"
done
