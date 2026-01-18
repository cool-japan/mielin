#!/bin/bash
set -e

echo "Building MielinOS..."
echo "===================="
echo ""

echo "Building in debug mode..."
cargo build --workspace

echo ""
echo "Building in release mode..."
cargo build --release --workspace

echo ""
echo "Build complete!"
