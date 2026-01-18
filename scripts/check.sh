#!/bin/bash
set -e

echo "Running MielinOS Quality Checks..."
echo "==================================="
echo ""

echo "1. Checking formatting..."
cargo fmt --all -- --check
echo "   ✓ Formatting OK"
echo ""

echo "2. Running Clippy..."
cargo clippy --all-features --workspace -- -D warnings
echo "   ✓ Clippy OK"
echo ""

echo "3. Checking compilation..."
cargo check --all-features --workspace
echo "   ✓ Compilation OK"
echo ""

echo "4. Running tests..."
if command -v cargo-nextest &> /dev/null; then
    cargo nextest run --all-features --workspace
else
    cargo test --all-features --workspace
fi
echo "   ✓ Tests OK"
echo ""

echo "All checks passed! ✓"
