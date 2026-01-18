//! WASI Integration Tests
//!
//! Tests for WASI (WebAssembly System Interface) functionality
//! including stdio, clocks, environment, and arguments.

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::{executor::WasmExecutor, host::HostState, wasi::WasiContext};
use std::collections::HashMap;
use wasmtime::{Linker, Store};

/// Helper to create a WASM executor with WASI support
fn create_wasi_executor() -> (WasmExecutor, WasiContext) {
    let executor = WasmExecutor::new().unwrap();
    let wasi_ctx = WasiContext::new();
    (executor, wasi_ctx)
}

#[test]
fn test_wasi_fd_write_stdout() {
    // Create a WASM module that writes to stdout
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "_start")
                ;; Write "Hello, WASI!" to stdout (fd=1)

                ;; Setup iovector at offset 0
                ;; ptr to buffer at offset 0
                i32.const 0
                i32.const 100  ;; buffer is at offset 100
                i32.store

                ;; length of buffer
                i32.const 4
                i32.const 12   ;; "Hello, WASI!" is 12 bytes
                i32.store

                ;; Write "Hello, WASI!" to offset 100
                i32.const 100
                i32.const 72   ;; 'H'
                i32.store8
                i32.const 101
                i32.const 101  ;; 'e'
                i32.store8
                i32.const 102
                i32.const 108  ;; 'l'
                i32.store8
                i32.const 103
                i32.const 108  ;; 'l'
                i32.store8
                i32.const 104
                i32.const 111  ;; 'o'
                i32.store8
                i32.const 105
                i32.const 44   ;; ','
                i32.store8
                i32.const 106
                i32.const 32   ;; ' '
                i32.store8
                i32.const 107
                i32.const 87   ;; 'W'
                i32.store8
                i32.const 108
                i32.const 65   ;; 'A'
                i32.store8
                i32.const 109
                i32.const 83   ;; 'S'
                i32.store8
                i32.const 110
                i32.const 73   ;; 'I'
                i32.store8
                i32.const 111
                i32.const 33   ;; '!'
                i32.store8

                ;; Call fd_write(1, iovs_ptr=0, iovs_len=1, nwritten_ptr=200)
                i32.const 1    ;; stdout
                i32.const 0    ;; iovs pointer
                i32.const 1    ;; iovs length (1 iovec)
                i32.const 200  ;; nwritten pointer
                call $fd_write
                drop
            )
        )
        "#,
    )
    .unwrap();

    let (executor, wasi_ctx) = create_wasi_executor();
    let module = executor.compile_module(&wasm).unwrap();

    // Create host state with WASI context
    let host_state = HostState::with_wasi(HardwareCapabilities::NONE, wasi_ctx);
    let mut store = Store::new(executor.engine(), host_state);

    // Create linker with WASI functions
    let mut linker = Linker::new(executor.engine());
    mielin_wasm::wasi::register_wasi_functions(&mut linker).unwrap();

    // Instantiate and run
    let instance = linker.instantiate(&mut store, &module).unwrap();
    let start = instance.get_func(&mut store, "_start").unwrap();
    start.call(&mut store, &[], &mut []).unwrap();

    // Verify stdout output
    let wasi = store.data().wasi_context().unwrap();
    let output = wasi.get_stdout_output();
    assert_eq!(&output, b"Hello, WASI!");
}

#[test]
fn test_wasi_clock_time_get() {
    // Create a WASM module that gets current time
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "clock_time_get" (func $clock_time_get (param i32 i64 i32) (result i32)))
            (memory (export "memory") 1)
            (global $time (mut i64) (i64.const 0))
            (func (export "_start")
                ;; Get realtime clock (id=0)
                i32.const 0    ;; clock_id = REALTIME
                i64.const 0    ;; precision
                i32.const 100  ;; time output pointer
                call $clock_time_get
                drop

                ;; Load the time value
                i32.const 100
                i64.load
                global.set $time
            )
            (func (export "get_time") (result i64)
                global.get $time
            )
        )
        "#,
    )
    .unwrap();

    let (executor, wasi_ctx) = create_wasi_executor();
    let module = executor.compile_module(&wasm).unwrap();

    let host_state = HostState::with_wasi(HardwareCapabilities::NONE, wasi_ctx);
    let mut store = Store::new(executor.engine(), host_state);

    let mut linker = Linker::new(executor.engine());
    mielin_wasm::wasi::register_wasi_functions(&mut linker).unwrap();

    let instance = linker.instantiate(&mut store, &module).unwrap();
    let start = instance.get_func(&mut store, "_start").unwrap();
    start.call(&mut store, &[], &mut []).unwrap();

    // Verify time is reasonable (should be > 0 for realtime)
    let get_time = instance
        .get_typed_func::<(), i64>(&mut store, "get_time")
        .unwrap();
    let time = get_time.call(&mut store, ()).unwrap();
    assert!(time > 0, "Realtime clock should return positive value");
}

#[test]
fn test_wasi_environ_vars() {
    // Create a WASM module that reads environment variables
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "environ_sizes_get" (func $environ_sizes_get (param i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "environ_get" (func $environ_get (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (global $count (mut i32) (i32.const 0))
            (global $size (mut i32) (i32.const 0))
            (func (export "_start")
                ;; Get environment sizes
                i32.const 0    ;; count pointer
                i32.const 4    ;; size pointer
                call $environ_sizes_get
                drop

                ;; Load count and size
                i32.const 0
                i32.load
                global.set $count

                i32.const 4
                i32.load
                global.set $size
            )
            (func (export "get_count") (result i32)
                global.get $count
            )
            (func (export "get_size") (result i32)
                global.get $size
            )
        )
        "#,
    )
    .unwrap();

    let (executor, mut wasi_ctx) = create_wasi_executor();

    // Set environment variables
    let mut env_vars = HashMap::new();
    env_vars.insert("FOO".to_string(), "bar".to_string());
    env_vars.insert("BAZ".to_string(), "qux".to_string());
    wasi_ctx.set_env_vars(env_vars);

    let module = executor.compile_module(&wasm).unwrap();
    let host_state = HostState::with_wasi(HardwareCapabilities::NONE, wasi_ctx);
    let mut store = Store::new(executor.engine(), host_state);

    let mut linker = Linker::new(executor.engine());
    mielin_wasm::wasi::register_wasi_functions(&mut linker).unwrap();

    let instance = linker.instantiate(&mut store, &module).unwrap();
    let start = instance.get_func(&mut store, "_start").unwrap();
    start.call(&mut store, &[], &mut []).unwrap();

    // Verify count
    let get_count = instance
        .get_typed_func::<(), i32>(&mut store, "get_count")
        .unwrap();
    let count = get_count.call(&mut store, ()).unwrap();
    assert_eq!(count, 2, "Should have 2 environment variables");

    // Verify size (FOO=bar\0 + BAZ=qux\0 = 8 + 8 = 16)
    let get_size = instance
        .get_typed_func::<(), i32>(&mut store, "get_size")
        .unwrap();
    let size = get_size.call(&mut store, ()).unwrap();
    assert_eq!(size, 16, "Environment buffer size should be 16");
}

#[test]
fn test_wasi_args() {
    // Create a WASM module that reads command-line arguments
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "args_sizes_get" (func $args_sizes_get (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (global $argc (mut i32) (i32.const 0))
            (global $argv_buf_size (mut i32) (i32.const 0))
            (func (export "_start")
                ;; Get argument sizes
                i32.const 0    ;; argc pointer
                i32.const 4    ;; argv_buf_size pointer
                call $args_sizes_get
                drop

                ;; Load argc and buffer size
                i32.const 0
                i32.load
                global.set $argc

                i32.const 4
                i32.load
                global.set $argv_buf_size
            )
            (func (export "get_argc") (result i32)
                global.get $argc
            )
            (func (export "get_buf_size") (result i32)
                global.get $argv_buf_size
            )
        )
        "#,
    )
    .unwrap();

    let (executor, mut wasi_ctx) = create_wasi_executor();

    // Set command-line arguments
    let args = vec![
        "program".to_string(),
        "arg1".to_string(),
        "arg2".to_string(),
    ];
    wasi_ctx.set_args(args);

    let module = executor.compile_module(&wasm).unwrap();
    let host_state = HostState::with_wasi(HardwareCapabilities::NONE, wasi_ctx);
    let mut store = Store::new(executor.engine(), host_state);

    let mut linker = Linker::new(executor.engine());
    mielin_wasm::wasi::register_wasi_functions(&mut linker).unwrap();

    let instance = linker.instantiate(&mut store, &module).unwrap();
    let start = instance.get_func(&mut store, "_start").unwrap();
    start.call(&mut store, &[], &mut []).unwrap();

    // Verify argc
    let get_argc = instance
        .get_typed_func::<(), i32>(&mut store, "get_argc")
        .unwrap();
    let argc = get_argc.call(&mut store, ()).unwrap();
    assert_eq!(argc, 3, "Should have 3 arguments");

    // Verify buffer size (program\0 + arg1\0 + arg2\0 = 8 + 5 + 5 = 18)
    let get_buf_size = instance
        .get_typed_func::<(), i32>(&mut store, "get_buf_size")
        .unwrap();
    let buf_size = get_buf_size.call(&mut store, ()).unwrap();
    assert_eq!(buf_size, 18, "Argument buffer size should be 18");
}

#[test]
fn test_wasi_monotonic_clock() {
    // Create a WASM module that uses monotonic clock
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "clock_time_get" (func $clock_time_get (param i32 i64 i32) (result i32)))
            (memory (export "memory") 1)
            (global $time1 (mut i64) (i64.const 0))
            (global $time2 (mut i64) (i64.const 0))
            (func (export "_start")
                ;; Get monotonic time (id=1)
                i32.const 1    ;; clock_id = MONOTONIC
                i64.const 0    ;; precision
                i32.const 100  ;; time output pointer
                call $clock_time_get
                drop

                i32.const 100
                i64.load
                global.set $time1

                ;; Get monotonic time again
                i32.const 1    ;; clock_id = MONOTONIC
                i64.const 0    ;; precision
                i32.const 100  ;; time output pointer
                call $clock_time_get
                drop

                i32.const 100
                i64.load
                global.set $time2
            )
            (func (export "get_time1") (result i64)
                global.get $time1
            )
            (func (export "get_time2") (result i64)
                global.get $time2
            )
        )
        "#,
    )
    .unwrap();

    let (executor, wasi_ctx) = create_wasi_executor();
    let module = executor.compile_module(&wasm).unwrap();

    let host_state = HostState::with_wasi(HardwareCapabilities::NONE, wasi_ctx);
    let mut store = Store::new(executor.engine(), host_state);

    let mut linker = Linker::new(executor.engine());
    mielin_wasm::wasi::register_wasi_functions(&mut linker).unwrap();

    let instance = linker.instantiate(&mut store, &module).unwrap();
    let start = instance.get_func(&mut store, "_start").unwrap();
    start.call(&mut store, &[], &mut []).unwrap();

    // Verify monotonic time is non-negative and increasing
    let get_time1 = instance
        .get_typed_func::<(), i64>(&mut store, "get_time1")
        .unwrap();
    let time1 = get_time1.call(&mut store, ()).unwrap();
    assert!(time1 >= 0, "Monotonic time should be non-negative");

    let get_time2 = instance
        .get_typed_func::<(), i64>(&mut store, "get_time2")
        .unwrap();
    let time2 = get_time2.call(&mut store, ()).unwrap();
    assert!(time2 >= time1, "Monotonic time should be non-decreasing");
}

#[test]
fn test_wasi_multiple_iovecs() {
    // Create a WASM module that writes multiple iovecs
    let wasm = wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "_start")
                ;; Setup two iovectors
                ;; First iovec at offset 0
                i32.const 0
                i32.const 100  ;; buffer 1 at offset 100
                i32.store
                i32.const 4
                i32.const 5    ;; "Hello" is 5 bytes
                i32.store

                ;; Second iovec at offset 8
                i32.const 8
                i32.const 200  ;; buffer 2 at offset 200
                i32.store
                i32.const 12
                i32.const 6    ;; " World" is 6 bytes
                i32.store

                ;; Write "Hello" to offset 100
                i32.const 100
                i64.const 0x6f6c6c6548  ;; "Hello" in little-endian
                i64.store

                ;; Write " World" to offset 200
                i32.const 200
                i32.const 32   ;; ' '
                i32.store8
                i32.const 201
                i64.const 0x646c726f57  ;; "World" in little-endian
                i64.store

                ;; Call fd_write with 2 iovecs
                i32.const 1    ;; stdout
                i32.const 0    ;; iovs pointer
                i32.const 2    ;; iovs length (2 iovecs)
                i32.const 300  ;; nwritten pointer
                call $fd_write
                drop
            )
        )
        "#,
    )
    .unwrap();

    let (executor, wasi_ctx) = create_wasi_executor();
    let module = executor.compile_module(&wasm).unwrap();

    let host_state = HostState::with_wasi(HardwareCapabilities::NONE, wasi_ctx);
    let mut store = Store::new(executor.engine(), host_state);

    let mut linker = Linker::new(executor.engine());
    mielin_wasm::wasi::register_wasi_functions(&mut linker).unwrap();

    let instance = linker.instantiate(&mut store, &module).unwrap();
    let start = instance.get_func(&mut store, "_start").unwrap();
    start.call(&mut store, &[], &mut []).unwrap();

    // Verify stdout output is "Hello World"
    let wasi = store.data().wasi_context().unwrap();
    let output = wasi.get_stdout_output();
    assert_eq!(&output, b"Hello World");
}
