#!/bin/bash
set -e

echo "Running MielinOS Test Suite..."
echo "=============================="
echo ""

# Check if cargo-nextest is installed
if ! command -v cargo-nextest &> /dev/null; then
    echo "cargo-nextest not found. Installing..."
    cargo install cargo-nextest --locked
fi

echo "Running all tests with nextest..."
cargo nextest run --all-features --workspace

echo ""
echo "Running doc tests..."
cargo test --doc --workspace

echo ""
echo "All tests passed!"
