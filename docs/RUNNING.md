# Running MielinOS - Setup Guide

This guide explains how to build and run MielinOS on your system.

## Prerequisites

### 1. Install Rust Nightly

MielinOS requires Rust nightly for unstable features:

```bash
# Install rustup if not already installed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Switch to nightly
rustup default nightly

# Install rust-src for building no_std targets
rustup component add rust-src
```

### 2. Install Bootimage Tool

The `bootimage` tool creates bootable disk images:

```bash
cargo install bootimage
```

### 3. Install QEMU

QEMU is a hardware emulator that lets you run MielinOS without real hardware.

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install qemu-system-x86
```

**macOS:**
```bash
brew install qemu
```

**Windows:**
- Download QEMU from: https://www.qemu.org/download/#windows
- Or use WSL2 with Ubuntu and follow Linux instructions

**Verify installation:**
```bash
qemu-system-x86_64 --version
```

## Building MielinOS

### Quick Build

From the project root:

```bash
# Build the kernel
cargo build -p mielin-kernel --target x86_64-unknown-none

# Create bootable image
cargo bootimage --target x86_64-unknown-none
```

### Understanding the Build Process

1. **Kernel Compilation**: Rust code is compiled to a bare-metal binary (no OS)
2. **Bootloader Integration**: The bootloader crate prepends boot code
3. **Image Creation**: Everything is packaged into a bootable disk image (.bin file)

The resulting image is at:
```
target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin
```

## Running in QEMU

### Using the Launch Scripts (Recommended)

**Headless mode (terminal only):**
```bash
./scripts/run-qemu.sh
```

**With GUI window:**
```bash
./scripts/run-qemu-gui.sh
```

### Manual QEMU Invocation

**Basic:**
```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin
```

**With serial output to terminal:**
```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin \
  -serial stdio \
  -display none
```

**With more memory:**
```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin \
  -m 512M
```

### QEMU Keyboard Shortcuts

When running in terminal mode:

- **Exit QEMU**: `Ctrl+A` then `X`
- **Switch to QEMU monitor**: `Ctrl+A` then `C`
- **Pause/Resume**: `Ctrl+A` then `S`

## Debugging

### Using GDB

**Terminal 1 - Start QEMU with GDB server:**
```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin \
  -s -S
```

Flags:
- `-s`: Start GDB server on port 1234
- `-S`: Pause execution at startup

**Terminal 2 - Connect GDB:**
```bash
gdb target/x86_64-unknown-none/debug/mielin-kernel
```

In GDB:
```gdb
(gdb) target remote :1234
(gdb) break kernel_main
(gdb) continue
```

### Viewing Kernel Logs

If your kernel outputs to the serial port:

```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin \
  -serial mon:stdio
```

## Running on Real Hardware (Advanced)

⚠️ **Warning**: Only do this if you understand the risks. You can damage your system if done incorrectly.

### Creating a Bootable USB

**Linux:**
```bash
# Find your USB device (be VERY careful!)
lsblk

# Write image to USB (replace /dev/sdX with your device)
sudo dd if=target/x86_64-unknown-none/debug/bootimage-mielin-kernel.bin of=/dev/sdX bs=4M status=progress
sync
```

**Windows:**
- Use Rufus: https://rufus.ie/
- Select the .bin file as the image
- Write to USB drive

**Boot from USB:**
1. Insert USB drive
2. Reboot computer
3. Enter BIOS/UEFI (usually F2, F12, Del, or Esc during boot)
4. Select USB drive as boot device
5. MielinOS will boot

## Troubleshooting

### "bootimage: command not found"

Install it:
```bash
cargo install bootimage
```

### "error: target 'x86_64-unknown-none' not found"

The target specification is in the project root. Make sure you're running from `/mnt/f/mielin`.

### QEMU hangs or crashes

Try:
- Reducing memory: `-m 128M`
- Disabling KVM: Add `-no-kvm` flag
- Check QEMU version: `qemu-system-x86_64 --version`

### Build errors with nightly

Update Rust nightly:
```bash
rustup update nightly
```

### "error: the `-Z` flag is only accepted on the nightly channel"

Switch to nightly:
```bash
rustup default nightly
```

## Performance Tips

- **Use release builds** for better performance:
  ```bash
  cargo bootimage --release --target x86_64-unknown-none
  ```

- **Enable KVM** on Linux for hardware acceleration:
  ```bash
  qemu-system-x86_64 -enable-kvm ...
  ```

- **Allocate more CPU cores**:
  ```bash
  qemu-system-x86_64 -smp 4 ...
  ```

## Next Steps

- Read [mielin-kernel/README.md](mielin-kernel/README.md) for kernel internals
- Explore [examples/](examples/) for sample agents
- Check [CONTRIBUTING.md](CONTRIBUTING.md) to contribute

## Resources

- [QEMU Documentation](https://www.qemu.org/docs/master/)
- [Rust Embedded Book](https://rust-embedded.github.io/book/)
- [OSDev Wiki](https://wiki.osdev.org/)
- [Writing an OS in Rust](https://os.phil-opp.com/)
