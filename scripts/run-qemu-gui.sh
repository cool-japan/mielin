#!/bin/bash
# Run MielinOS in QEMU with graphical output

set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}Building MielinOS kernel...${NC}"

cargo bootimage --target x86_64-unknown-none

BOOTIMAGE=$(find target/x86_64-unknown-none -name "*.bin" | grep bootimage | head -n 1)

if [ -z "$BOOTIMAGE" ]; then
    echo "Error: Bootimage not found!"
    exit 1
fi

echo -e "${GREEN}Launching QEMU with GUI...${NC}"

qemu-system-x86_64 \
    -drive format=raw,file="$BOOTIMAGE" \
    -serial stdio \
    -m 256M \
    "$@"
