#!/bin/bash
# Run MielinOS in QEMU (x86_64)

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Building MielinOS kernel...${NC}"

# Build the bootable image
cargo bootimage --target x86_64-unknown-none

# Find the bootimage
BOOTIMAGE=$(find target/x86_64-unknown-none -name "*.bin" | grep bootimage | head -n 1)

if [ -z "$BOOTIMAGE" ]; then
    echo "Error: Bootimage not found!"
    exit 1
fi

echo -e "${GREEN}Launching QEMU...${NC}"
echo "Press Ctrl+A then X to exit QEMU"
echo ""

# Run QEMU with the bootimage
qemu-system-x86_64 \
    -drive format=raw,file="$BOOTIMAGE" \
    -serial stdio \
    -display none \
    -no-reboot \
    -m 256M \
    "$@"
