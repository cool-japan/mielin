#!/bin/bash
set -e

echo "Setting up MielinOS development environment..."
echo "=============================================="
echo ""

echo "Checking Rust installation..."
if ! command -v rustc &> /dev/null; then
    echo "Error: Rust is not installed. Please install from https://rustup.rs/"
    exit 1
fi

RUST_VERSION=$(rustc --version | cut -d' ' -f2)
echo "   ✓ Rust $RUST_VERSION installed"
echo ""

echo "Installing required tools..."

# Install nightly for kernel development
echo "   Checking Rust nightly..."
rustup install nightly || true
rustup component add rust-src --toolchain nightly || true

# Install bootimage for kernel builds
if ! command -v bootimage &> /dev/null; then
    echo "   Installing bootimage..."
    cargo +nightly install bootimage
else
    echo "   ✓ bootimage already installed"
fi

if ! command -v cargo-nextest &> /dev/null; then
    echo "   Installing cargo-nextest..."
    cargo install cargo-nextest --locked
else
    echo "   ✓ cargo-nextest already installed"
fi

if ! command -v cargo-watch &> /dev/null; then
    echo "   Installing cargo-watch (optional, for development)..."
    cargo install cargo-watch || true
else
    echo "   ✓ cargo-watch already installed"
fi

# Check QEMU
echo ""
if command -v qemu-system-x86_64 &> /dev/null; then
    echo "   ✓ QEMU installed: $(qemu-system-x86_64 --version | head -1)"
else
    echo "   ⚠️  QEMU not installed (needed for testing)"
    echo "      Install: sudo apt install qemu-system-x86 (Linux)"
    echo "               brew install qemu (macOS)"
fi

# Add WASM target
echo ""
echo "   Adding wasm32-unknown-unknown target..."
rustup target add wasm32-unknown-unknown || true

echo ""
echo "Building project..."
cargo build --workspace

echo ""
echo "Running tests..."
cargo nextest run --workspace

echo ""
echo "Setup complete! You can now start developing MielinOS."
echo ""
echo "Useful commands:"
echo "  ./scripts/run-qemu.sh     - Run MielinOS in QEMU"
echo "  ./scripts/run-qemu-gui.sh - Run MielinOS with GUI"
echo "  ./scripts/build.sh        - Build all crates"
echo "  ./scripts/test.sh         - Run all tests"
echo "  ./scripts/check.sh        - Run all quality checks"
echo "  ./scripts/clean.sh        - Clean build artifacts"
echo ""
echo "For detailed QEMU setup, see: docs/RUNNING.md"
