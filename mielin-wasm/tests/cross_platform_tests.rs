//! Cross-Platform Execution Tests for MielinWasm
//!
//! Tests to ensure consistent behavior across different platforms and architectures.
//!
//! Run with: cargo test --test cross_platform_tests

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::executor::WasmExecutor;
// System types available if needed for platform-specific tests

/// Test basic arithmetic operations across platforms
#[test]
fn test_arithmetic_operations() {
    let wasm = wat::parse_str(
        r#"
        (module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
            (func (export "mul") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.mul
            )
            (func (export "div") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.div_s
            )
            (func (export "rem") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.rem_s
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Test addition
    let add = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "add")
        .unwrap();
    assert_eq!(add.call(&mut store, (5, 7)).unwrap(), 12);
    assert_eq!(add.call(&mut store, (-5, 7)).unwrap(), 2);
    assert_eq!(add.call(&mut store, (i32::MAX, 1)).unwrap(), i32::MIN); // Overflow wraps

    // Test multiplication
    let mul = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "mul")
        .unwrap();
    assert_eq!(mul.call(&mut store, (6, 7)).unwrap(), 42);
    assert_eq!(mul.call(&mut store, (-6, 7)).unwrap(), -42);

    // Test division
    let div = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "div")
        .unwrap();
    assert_eq!(div.call(&mut store, (42, 7)).unwrap(), 6);
    assert_eq!(div.call(&mut store, (-42, 7)).unwrap(), -6);

    // Test remainder
    let rem = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "rem")
        .unwrap();
    assert_eq!(rem.call(&mut store, (43, 7)).unwrap(), 1);
    assert_eq!(rem.call(&mut store, (-43, 7)).unwrap(), -1);
}

/// Test floating point operations across platforms
#[test]
fn test_floating_point_operations() {
    let wasm = wat::parse_str(
        r#"
        (module
            (func (export "add_f32") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add
            )
            (func (export "mul_f32") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.mul
            )
            (func (export "sqrt_f32") (param f32) (result f32)
                local.get 0
                f32.sqrt
            )
            (func (export "add_f64") (param f64 f64) (result f64)
                local.get 0
                local.get 1
                f64.add
            )
            (func (export "mul_f64") (param f64 f64) (result f64)
                local.get 0
                local.get 1
                f64.mul
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Test f32 operations
    let add_f32 = instance
        .get_typed_func::<(f32, f32), f32>(&mut store, "add_f32")
        .unwrap();
    assert!((add_f32.call(&mut store, (1.5, 2.5)).unwrap() - 4.0).abs() < f32::EPSILON);

    let mul_f32 = instance
        .get_typed_func::<(f32, f32), f32>(&mut store, "mul_f32")
        .unwrap();
    assert!((mul_f32.call(&mut store, (2.0, 3.0)).unwrap() - 6.0).abs() < f32::EPSILON);

    let sqrt_f32 = instance
        .get_typed_func::<f32, f32>(&mut store, "sqrt_f32")
        .unwrap();
    assert!((sqrt_f32.call(&mut store, 9.0).unwrap() - 3.0).abs() < f32::EPSILON);

    // Test f64 operations
    let add_f64 = instance
        .get_typed_func::<(f64, f64), f64>(&mut store, "add_f64")
        .unwrap();
    assert!((add_f64.call(&mut store, (1.5, 2.5)).unwrap() - 4.0).abs() < f64::EPSILON);

    let mul_f64 = instance
        .get_typed_func::<(f64, f64), f64>(&mut store, "mul_f64")
        .unwrap();
    assert!((mul_f64.call(&mut store, (2.0, 3.0)).unwrap() - 6.0).abs() < f64::EPSILON);
}

/// Test endianness handling
#[test]
fn test_endianness() {
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "write_i32") (param i32 i32)
                local.get 0
                local.get 1
                i32.store
            )
            (func (export "read_i32") (param i32) (result i32)
                local.get 0
                i32.load
            )
            (func (export "write_bytes") (param i32)
                ;; Write bytes individually
                i32.const 0
                i32.const 0x12
                i32.store8

                i32.const 1
                i32.const 0x34
                i32.store8

                i32.const 2
                i32.const 0x56
                i32.store8

                i32.const 3
                i32.const 0x78
                i32.store8
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Write and read i32
    let write = instance
        .get_typed_func::<(i32, i32), ()>(&mut store, "write_i32")
        .unwrap();
    let read = instance
        .get_typed_func::<i32, i32>(&mut store, "read_i32")
        .unwrap();

    let test_value: i32 = 0x12345678;
    write.call(&mut store, (0, test_value)).unwrap();
    let read_value = read.call(&mut store, 0).unwrap();

    assert_eq!(read_value, test_value, "Endianness handling failed");

    // Write bytes and read as i32 (should be little-endian in WASM)
    let write_bytes = instance
        .get_typed_func::<i32, ()>(&mut store, "write_bytes")
        .unwrap();
    write_bytes.call(&mut store, 0).unwrap();

    let value = read.call(&mut store, 0).unwrap();
    // In little-endian: 0x78563412
    assert_eq!(value, 0x78563412, "WASM uses little-endian byte order");
}

/// Test memory alignment across platforms
#[test]
fn test_memory_alignment() {
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "write_aligned") (param i32 i32)
                ;; Write at 4-byte aligned address
                local.get 0
                local.get 1
                i32.store align=4
            )
            (func (export "write_unaligned") (param i32 i32)
                ;; Write at unaligned address
                local.get 0
                local.get 1
                i32.store align=1
            )
            (func (export "read_aligned") (param i32) (result i32)
                local.get 0
                i32.load align=4
            )
            (func (export "read_unaligned") (param i32) (result i32)
                local.get 0
                i32.load align=1
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    let write_aligned = instance
        .get_typed_func::<(i32, i32), ()>(&mut store, "write_aligned")
        .unwrap();
    let read_aligned = instance
        .get_typed_func::<i32, i32>(&mut store, "read_aligned")
        .unwrap();
    let write_unaligned = instance
        .get_typed_func::<(i32, i32), ()>(&mut store, "write_unaligned")
        .unwrap();
    let read_unaligned = instance
        .get_typed_func::<i32, i32>(&mut store, "read_unaligned")
        .unwrap();

    // Test aligned access
    write_aligned.call(&mut store, (0, 0x12345678)).unwrap();
    assert_eq!(read_aligned.call(&mut store, 0).unwrap(), 0x12345678);

    // Test unaligned access (should work on all platforms thanks to WASM spec)
    write_unaligned
        .call(&mut store, (1, 0xABCDEF00u32 as i32))
        .unwrap();
    assert_eq!(
        read_unaligned.call(&mut store, 1).unwrap(),
        0xABCDEF00u32 as i32
    );
}

/// Test platform detection consistency
#[test]
fn test_platform_detection() {
    let wasm = wat::parse_str(
        r#"
        (module
            (import "mielin" "process_platform" (func $process_platform (result i32)))
            (import "mielin" "process_arch" (func $process_arch (result i32)))
            (import "mielin" "process_pointer_bits" (func $process_pointer_bits (result i32)))
            (import "mielin" "process_cpu_count" (func $process_cpu_count (result i32)))
            (memory (export "memory") 1)
            (func (export "get_platform") (result i32)
                call $process_platform
            )
            (func (export "get_arch") (result i32)
                call $process_arch
            )
            (func (export "get_bits") (result i32)
                call $process_pointer_bits
            )
            (func (export "get_cpus") (result i32)
                call $process_cpu_count
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Get platform info from host functions
    let get_platform = instance
        .get_typed_func::<(), i32>(&mut store, "get_platform")
        .unwrap();
    let get_arch = instance
        .get_typed_func::<(), i32>(&mut store, "get_arch")
        .unwrap();
    let get_bits = instance
        .get_typed_func::<(), i32>(&mut store, "get_bits")
        .unwrap();
    let get_cpus = instance
        .get_typed_func::<(), i32>(&mut store, "get_cpus")
        .unwrap();

    let platform = get_platform.call(&mut store, ()).unwrap();
    let arch = get_arch.call(&mut store, ()).unwrap();
    let bits = get_bits.call(&mut store, ()).unwrap();
    let cpus = get_cpus.call(&mut store, ()).unwrap();

    println!("Platform: {}", platform);
    println!("Architecture: {}", arch);
    println!("Pointer bits: {}", bits);
    println!("CPU count: {}", cpus);

    // Validate platform detection (basic sanity check)
    assert!(
        (0..=10).contains(&platform),
        "Platform value should be valid"
    );
    assert!(
        (0..=10).contains(&arch),
        "Architecture value should be valid"
    );

    // Validate pointer size
    #[cfg(target_pointer_width = "32")]
    assert_eq!(bits, 32);

    #[cfg(target_pointer_width = "64")]
    assert_eq!(bits, 64);

    // CPU count should be reasonable
    assert!(cpus >= 1);
    assert!(cpus <= 1024); // Sanity check
}

/// Test random number generation consistency
#[test]
fn test_random_generation() {
    let wasm = wat::parse_str(
        r#"
        (module
            (import "mielin" "random_u32" (func $random_u32 (result i32)))
            (import "mielin" "random_f32" (func $random_f32 (result f32)))
            (func (export "get_u32") (result i32)
                call $random_u32
            )
            (func (export "get_f32") (result f32)
                call $random_f32
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    let get_u32 = instance
        .get_typed_func::<(), i32>(&mut store, "get_u32")
        .unwrap();
    let get_f32 = instance
        .get_typed_func::<(), f32>(&mut store, "get_f32")
        .unwrap();

    // Test range of f32
    for _ in 0..10 {
        let f32_val = get_f32.call(&mut store, ()).unwrap();
        assert!((0.0..1.0).contains(&f32_val), "f32 should be in [0, 1)");
    }

    // Test that random values vary
    let vals: Vec<i32> = (0..10)
        .map(|_| get_u32.call(&mut store, ()).unwrap())
        .collect();

    // Very unlikely all 10 values are the same
    let all_same = vals.windows(2).all(|w| w[0] == w[1]);
    assert!(!all_same, "Random values should vary");
}

/// Test time functions across platforms
#[test]
fn test_time_functions() {
    use std::thread;
    use std::time::Duration;

    let wasm = wat::parse_str(
        r#"
        (module
            (import "mielin" "time_now_millis" (func $time_now_millis (result i64)))
            (import "mielin" "time_monotonic_nanos" (func $time_monotonic_nanos (result i64)))
            (func (export "now_millis") (result i64)
                call $time_now_millis
            )
            (func (export "monotonic") (result i64)
                call $time_monotonic_nanos
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    let now_millis = instance
        .get_typed_func::<(), i64>(&mut store, "now_millis")
        .unwrap();
    let monotonic = instance
        .get_typed_func::<(), i64>(&mut store, "monotonic")
        .unwrap();

    // Test current time
    let t1 = now_millis.call(&mut store, ()).unwrap();
    thread::sleep(Duration::from_millis(10));
    let t2 = now_millis.call(&mut store, ()).unwrap();

    assert!(t2 > t1, "Time should advance");
    assert!(t2 - t1 >= 10, "Time difference should be at least 10ms");

    // Test monotonic time
    let m1 = monotonic.call(&mut store, ()).unwrap();
    thread::sleep(Duration::from_millis(10));
    let m2 = monotonic.call(&mut store, ()).unwrap();

    assert!(m2 > m1, "Monotonic time should advance");
    assert!(
        m2 - m1 >= 10_000_000,
        "Monotonic difference should be at least 10ms in nanos"
    );
}

/// Test memory operations consistency
#[test]
fn test_memory_operations_consistency() {
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "fill_pattern")
                (local $i i32)
                i32.const 0
                local.set $i

                (block
                    (loop
                        local.get $i
                        i32.const 1024
                        i32.ge_u
                        br_if 1

                        local.get $i
                        local.get $i
                        i32.store8

                        local.get $i
                        i32.const 1
                        i32.add
                        local.set $i

                        br 0
                    )
                )
            )
            (func (export "checksum") (result i32)
                (local $i i32)
                (local $sum i32)
                i32.const 0
                local.set $i
                i32.const 0
                local.set $sum

                (block
                    (loop
                        local.get $i
                        i32.const 1024
                        i32.ge_u
                        br_if 1

                        local.get $sum
                        local.get $i
                        i32.load8_u
                        i32.add
                        local.set $sum

                        local.get $i
                        i32.const 1
                        i32.add
                        local.set $i

                        br 0
                    )
                )

                local.get $sum
            )
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    let fill = instance
        .get_typed_func::<(), ()>(&mut store, "fill_pattern")
        .unwrap();
    let checksum = instance
        .get_typed_func::<(), i32>(&mut store, "checksum")
        .unwrap();

    // Fill memory with pattern
    fill.call(&mut store, ()).unwrap();

    // Calculate checksum (should be 0+1+2+...+255+0+1+...+255+0+... = 32640)
    let sum = checksum.call(&mut store, ()).unwrap();
    let expected = (0..256).sum::<i32>() * 4; // 4 complete 0-255 sequences in 1024 bytes
    assert_eq!(
        sum, expected,
        "Memory pattern should be consistent across platforms"
    );
}
