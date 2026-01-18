# Troubleshooting Guide

This guide helps diagnose and resolve common issues with the MielinOS Hardware Abstraction Layer.

## Table of Contents

- [Quick Diagnostics](#quick-diagnostics)
- [Capability Detection Issues](#capability-detection-issues)
- [Runtime Switching Problems](#runtime-switching-problems)
- [Platform-Specific Issues](#platform-specific-issues)
- [Performance Problems](#performance-problems)
- [Cross-Compilation Issues](#cross-compilation-issues)
- [Virtualization Issues](#virtualization-issues)

## Quick Diagnostics

### Step 1: Run the Detection Example

```bash
cargo run --example detect_hardware
```

This will show all detected hardware capabilities. If this fails or shows unexpected results, continue to the relevant section below.

### Step 2: Check Your Architecture

```bash
rustc --version --verbose
```

Look for the `host:` line to confirm your target architecture.

### Step 3: Verify Feature Flags

```bash
# Check what features Rust is using
cargo rustc -- --print cfg | grep target_feature
```

## Capability Detection Issues

### Problem: No SIMD Capabilities Detected on x86_64

**Symptoms:**
- `HardwareProfile::detect()` shows no AVX/AVX2/SSE capabilities
- `max_vector_width()` returns 0

**Diagnosis:**

```rust
use mielin_hal::arch::x86_64;

// Test individual capabilities
println!("SSE4.2: {}", x86_64::has_sse4_2());
println!("AVX: {}", x86_64::has_avx());
println!("AVX2: {}", x86_64::has_avx2());
println!("AVX-512: {}", x86_64::has_avx512f());
```

**Solutions:**

1. **Running in a VM**: Check if your hypervisor is hiding SIMD features
   ```bash
   # On Linux, check /proc/cpuinfo
   cat /proc/cpuinfo | grep flags
   ```

2. **Old CPU**: Your CPU may not support these features
   ```bash
   # Check CPU model
   cat /proc/cpuinfo | grep "model name"
   ```

3. **Disabled in BIOS**: Some systems allow disabling AVX in BIOS/UEFI

4. **Cross-compilation target mismatch**: Ensure you're building for the right architecture

### Problem: Runtime Detection Shows Different Results Than Compile-Time

**Symptoms:**
- `#[cfg(target_feature = "avx2")]` is true, but `has_avx2()` returns false
- Or vice versa

**Explanation:**
- `#[cfg(target_feature)]` checks **compiler flags**, not **hardware**
- Runtime detection checks **actual CPU capabilities**

**Solutions:**

1. **For maximum compatibility**, use runtime detection:
   ```rust
   let caps = x86_64::detect_x86_64_capabilities();
   if caps.contains(HardwareCapabilities::AVX2) {
       // Use AVX2
   }
   ```

2. **For performance**, set compiler flags to match your CPU:
   ```bash
   RUSTFLAGS="-C target-cpu=native" cargo build --release
   ```

### Problem: Cache Information is Zero

**Symptoms:**
- `CacheTopology::detect()` shows 0 for all cache sizes
- `blocking_factor()` returns unexpected values

**Diagnosis:**

```rust
let cache = mielin_hal::cache::CacheTopology::detect();
println!("L1 Data: {} bytes", cache.l1_data.size);
println!("L2: {} bytes", cache.l2.size);
println!("L3: {} bytes", cache.l3.size);
println!("Cache line: {} bytes", cache.default_line_size);
```

**Solutions:**

1. **Platform-specific**: Cache detection varies by platform
   - x86_64: Uses CPUID (should always work)
   - AArch64: Uses CTR_EL0 register
   - Others: May use sysfs or default values

2. **Check sysfs** (Linux):
   ```bash
   ls /sys/devices/system/cpu/cpu0/cache/
   ```

3. **Fallback to defaults**: The code should provide reasonable defaults even if detection fails

### Problem: Memory Size is Zero

**Symptoms:**
- `MemoryInfo::detect()` shows 0 total memory
- Memory calculations fail

**Diagnosis:**

```rust
let mem = mielin_hal::system::MemoryInfo::detect();
println!("Total: {} bytes ({} MB)", mem.total, mem.total_mb());
println!("Page size: {} bytes", mem.page_size);
```

**Solutions:**

1. **Check /proc/meminfo** (Linux):
   ```bash
   cat /proc/meminfo | grep MemTotal
   ```

2. **Platform limitations**:
   - Embedded systems may not have OS support for memory queries
   - Some platforms require specific syscalls

3. **Use syscall detection**: Ensure the `sysinfo` syscall is available

## Runtime Switching Problems

### Problem: Wrong Implementation Selected

**Symptoms:**
- Slower implementation selected despite better hardware being available
- `RuntimeSelector::select()` chooses scalar over SIMD

**Diagnosis:**

```rust
use mielin_hal::runtime::{RuntimeSelector, FallbackChain, FeatureRequirement};

let selector = RuntimeSelector::new();
let profile = selector.profile();

// Check what's actually available
println!("Detected capabilities: {:?}", profile.capabilities);
println!("Vector width: {}", profile.max_vector_width());

// Check requirement satisfaction
let req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]);
println!("AVX2 satisfied: {}", req.is_satisfied_by(profile));
println!("AVX2 score: {}", req.satisfaction_score(profile));
```

**Solutions:**

1. **Check requirement priorities**: Higher priority requirements are selected first
   ```rust
   // Ensure priorities are correct
   let avx512_req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX512]);
   let avx2_req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]);

   assert!(avx512_req.priority > avx2_req.priority);
   ```

2. **Verify capability detection**: Make sure capabilities are actually detected
   ```rust
   let caps = x86_64::detect_x86_64_capabilities();
   println!("Capabilities: {:?}", caps);
   ```

3. **Check excluded capabilities**: Ensure you haven't accidentally excluded what you want
   ```rust
   let req = FeatureRequirement::all_of(&[HardwareCapabilities::AVX2])
       .with_excluded(HardwareCapabilities::AVX512); // This would exclude AVX-512!
   ```

### Problem: Dispatch Table Returns Error

**Symptoms:**
- `DispatchTable::select()` returns error
- `table.call()` fails with "not selected"

**Diagnosis:**

```rust
let mut table: DispatchTable<i32> = DispatchTable::new("test");

// Add implementations
table.add_impl(FeatureRequirement::none(), scalar_func);
table.add_impl(FeatureRequirement::all_of(&[HardwareCapabilities::AVX2]), avx2_func);

let selector = RuntimeSelector::new();

// Try to select
match table.select(&selector) {
    Ok(()) => println!("Selection succeeded"),
    Err(e) => {
        println!("Selection failed: {}", e);
        // Check what's available
        println!("Available: {:?}", selector.profile().capabilities);
    }
}
```

**Solutions:**

1. **Always add a scalar fallback**:
   ```rust
   table.add_impl(FeatureRequirement::none(), scalar_impl);
   ```

2. **Check if select() was called** before `call()`:
   ```rust
   table.select(&selector)?; // Must call this first
   let result = table.call()?; // Then can call
   ```

3. **Verify function signatures match**:
   ```rust
   // Functions must have matching signatures
   fn func1() -> i32 { 42 }
   fn func2() -> i32 { 84 } // Same return type
   ```

## Platform-Specific Issues

### Raspberry Pi

#### Problem: GPIO Pins Not Detected

**Diagnosis:**
```rust
use mielin_hal::platform::{detect_platform, Platform, RaspberryPiCapabilities};

match detect_platform() {
    Platform::RaspberryPi(model) => {
        let caps = RaspberryPiCapabilities::detect();
        println!("GPIO pins: {}", caps.gpio_pins);
    }
    _ => println!("Not a Raspberry Pi"),
}
```

**Solutions:**
1. Check device tree: `cat /sys/firmware/devicetree/base/model`
2. Ensure running on actual Raspberry Pi hardware
3. Check for correct model detection

### STM32

#### Problem: Processor Not Recognized

**Diagnosis:**
```rust
use mielin_hal::arch::cortex_m::CortexMCapabilities;

let caps = CortexMCapabilities::detect();
println!("Processor: {:?}", caps.processor);
println!("MPU: {:?}", caps.mpu);
println!("FPU: {:?}", caps.fpu);
```

**Solutions:**
1. Verify DBGMCU_IDCODE register access
2. Check memory map (0xE0042000 or 0xE0044000)
3. Ensure running in privileged mode

### ESP32

#### Problem: WiFi Capabilities Not Detected

**Diagnosis:**
```rust
use mielin_hal::platform::{detect_platform, Platform, Esp32Capabilities};

match detect_platform() {
    Platform::Esp32(variant) => {
        let caps = Esp32Capabilities::detect(&variant);
        println!("WiFi: {:?}", caps.wifi_standard);
        println!("BLE: {:?}", caps.ble_version);
    }
    _ => println!("Not an ESP32"),
}
```

**Solutions:**
1. Check compile-time feature flags
2. Verify ESP32 variant detection
3. Ensure IDF environment is correct

## Performance Problems

### Problem: SIMD Code Slower Than Scalar

**Possible Causes:**
1. **AVX-512 frequency scaling**: CPU reduces frequency when using AVX-512
2. **Memory bandwidth**: SIMD saturates memory bandwidth
3. **Data alignment**: Unaligned loads/stores are slower
4. **Working set too large**: Exceeds cache capacity

**Diagnosis:**

```rust
// Benchmark different implementations
use mielin_hal::runtime::RuntimeSelector;
use std::time::Instant;

fn benchmark_impl(name: &str, f: impl Fn() -> f64) {
    let start = Instant::now();
    let mut sum = 0.0;
    for _ in 0..1000 {
        sum += f();
    }
    let elapsed = start.elapsed();
    println!("{}: {:?} (sum: {})", name, elapsed, sum);
}

benchmark_impl("scalar", scalar_impl);
benchmark_impl("avx2", avx2_impl);
benchmark_impl("avx512", avx512_impl);
```

**Solutions:**

1. **Profile with `perf`** (Linux):
   ```bash
   perf stat -e cycles,instructions,cache-misses cargo run --release
   ```

2. **Check CPU frequency scaling**:
   ```bash
   cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq
   ```

3. **Try different SIMD levels**: AVX2 may be faster than AVX-512 for sustained workloads

4. **Optimize data layout**: Ensure data is cache-aligned and accessed sequentially

### Problem: Cache-Aware Optimizations Don't Help

**Diagnosis:**

```rust
let cache = mielin_hal::cache::CacheTopology::detect();
let block_size = cache.blocking_factor();

println!("L1: {} KB", cache.l1_total_size() / 1024);
println!("L2: {} KB", cache.l2.size / 1024);
println!("L3: {} KB", cache.l3.size / 1024);
println!("Optimal block: {}", block_size);
```

**Solutions:**

1. **Verify working set size**: Ensure it fits in target cache level
2. **Use perf to check cache misses**:
   ```bash
   perf stat -e cache-misses,cache-references cargo run --release
   ```
3. **Consider prefetching**: Manually prefetch data before use
4. **Check stride patterns**: Sequential access is faster than random

## Cross-Compilation Issues

### Problem: Wrong Architecture Detected

**Symptoms:**
- Building for ARM but detects x86_64 capabilities
- `detect_architecture()` returns unexpected value

**Diagnosis:**

```bash
# Check what you're building for
rustc --version --verbose | grep host
cargo build --target aarch64-unknown-linux-gnu --verbose
```

**Solutions:**

1. **Specify target explicitly**:
   ```bash
   cargo build --target aarch64-unknown-linux-gnu
   ```

2. **Check .cargo/config.toml**:
   ```toml
   [build]
   target = "aarch64-unknown-linux-gnu"
   ```

3. **Use correct toolchain**:
   ```bash
   rustup target add aarch64-unknown-linux-gnu
   cargo build --target aarch64-unknown-linux-gnu
   ```

### Problem: Feature Detection Fails in Cross-Compilation

**Explanation**: Compile-time feature detection (`#[cfg(target_feature)]`) works during cross-compilation, but runtime detection may not work if you run the binary on the build machine.

**Solutions:**

1. **Use QEMU for testing**:
   ```bash
   cargo build --target aarch64-unknown-linux-gnu
   qemu-aarch64 target/aarch64-unknown-linux-gnu/debug/your_binary
   ```

2. **Test on actual hardware**: Copy binary to target device and run there

3. **Use conditional compilation** for build-time decisions

## Virtualization Issues

### Problem: Features Hidden by Hypervisor

**Symptoms:**
- Physical CPU has AVX-512, but VM doesn't detect it
- Inconsistent capability detection across VMs

**Diagnosis:**

```rust
use mielin_hal::virtualization;

let virt = virtualization::detect_virtualization();
println!("Hypervisor: {:?}", virt.hypervisor);
println!("Virtualized: {}", virt.is_virtualized);

// Check if features are hidden
let caps = x86_64::detect_x86_64_capabilities();
println!("Detected capabilities: {:?}", caps);
```

**Solutions:**

1. **Check hypervisor settings**:
   - **VMware**: Enable "Expose hardware assisted virtualization"
   - **VirtualBox**: Enable "Nested VT-x/AMD-V"
   - **KVM**: Use `-cpu host` flag

2. **Use hypervisor-specific flags**:
   ```bash
   # QEMU
   qemu-system-x86_64 -cpu host,+avx2,+fma

   # KVM libvirt
   <cpu mode='host-passthrough'/>
   ```

3. **Test paravirt features**:
   ```rust
   if virt.paravirt_features.contains(ParavirtFeatures::STEAL_TIME) {
       println!("KVM steal time available");
   }
   ```

### Problem: Container Detection Fails

**Diagnosis:**

```rust
let virt = virtualization::detect_virtualization();
println!("Container: {:?}", virt.container);

// Manual check
use std::fs;
if fs::metadata("/.dockerenv").is_ok() {
    println!("Likely running in Docker");
}
```

**Solutions:**

1. **Check cgroup**: `cat /proc/1/cgroup`
2. **Verify namespace**: `ls -l /proc/self/ns/`
3. **Container may hide files**: Some containers hide detection markers

## Getting More Help

### Enable Debug Logging

```rust
// Add debug output
let profile = HardwareProfile::detect();
println!("Profile: {:?}", profile);

let caps = x86_64::detect_x86_64_capabilities();
println!("x86_64 caps: {:?}", caps);
```

### Run Comprehensive Detection

```bash
# Run all examples
cargo run --example detect_hardware
cargo run --example platform_specific
cargo run --example runtime_switching
```

### Check System Information

```bash
# Linux
lscpu
cat /proc/cpuinfo
cat /proc/meminfo
ls /sys/devices/system/cpu/cpu0/cache/

# macOS
sysctl -a | grep cpu
sysctl -a | grep hw

# Windows
wmic cpu get caption,name,numberofcores,numberoflogicalprocessors
```

### Report Issues

If you've tried the above and still have issues, please report with:

1. **Hardware details**: CPU model, architecture, platform
2. **Software details**: Rust version, OS, mielin-hal version
3. **Detection output**: Results from `detect_hardware` example
4. **Expected vs. actual**: What you expected to see

GitHub Issues: https://github.com/cool-japan/mielin-hal/issues

---

**Last Updated**: 2026-01-18
**Version**: v0.1.0-rc.1
