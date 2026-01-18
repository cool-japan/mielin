#!/bin/bash
set -e

echo "Cleaning MielinOS build artifacts..."
echo "====================================="
echo ""

echo "Removing target directory..."
cargo clean

echo "Removing Cargo.lock..."
rm -f Cargo.lock

echo "Removing example artifacts..."
find examples -name "Cargo.lock" -delete
find examples -type d -name "target" -exec rm -rf {} + 2>/dev/null || true

echo ""
echo "Clean complete!"
