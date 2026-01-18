//! Debugging Integration Tests
//!
//! Tests for debugging features including memory inspection,
//! source maps, and stack traces.

use mielin_hal::capabilities::HardwareCapabilities;
use mielin_wasm::{
    debug::{inspect_memory, DebugContext, MemoryInspector, SourceMap, SourceMapEntry},
    executor::WasmExecutor,
};

#[test]
fn test_memory_inspection_integration() {
    // Create a WASM module that writes data to memory
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "write_data")
                ;; Write "Hello World" to offset 100
                i32.const 100
                i64.const 0x6f57206f6c6c6548  ;; "Hello Wo" in little-endian
                i64.store
                i32.const 108
                i32.const 0x646c72  ;; "rld" in little-endian (3 bytes + padding)
                i32.store
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

    // Execute the function
    let write_data = instance.get_func(&mut store, "write_data").unwrap();
    write_data.call(&mut store, &[], &mut []).unwrap();

    // Inspect memory
    let dump = inspect_memory(&mut store, &instance, 100, 16).unwrap();
    assert!(dump.contains("48 65 6c 6c")); // "Hell" in hex
    assert!(dump.contains("Hello"));
}

#[test]
fn test_memory_inspector_typed_reads() {
    // Create a WASM module that writes typed data
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "write_numbers")
                ;; Write u32: 0x12345678 at offset 0
                i32.const 0
                i32.const 0x12345678
                i32.store

                ;; Write f32: 3.14 at offset 4
                i32.const 4
                f32.const 3.14
                f32.store

                ;; Write u64: 0x123456789ABCDEF0 at offset 8
                i32.const 8
                i64.const 0x123456789ABCDEF0
                i64.store
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

    // Execute
    let write_nums = instance.get_func(&mut store, "write_numbers").unwrap();
    write_nums.call(&mut store, &[], &mut []).unwrap();

    // Get memory data
    let memory_data = mielin_wasm::debug::get_memory_data(&mut store, &instance).unwrap();

    // Use MemoryInspector to read typed values
    let inspector = MemoryInspector::new(0, 16);

    let u32_val = inspector.read_u32(&memory_data, 0).unwrap();
    assert_eq!(u32_val, 0x12345678);

    let f32_val = inspector.read_f32(&memory_data, 4).unwrap();
    assert!((f32_val - std::f32::consts::PI).abs() < 0.01);

    let u64_val = inspector.read_u64(&memory_data, 8).unwrap();
    assert_eq!(u64_val, 0x123456789ABCDEF0);
}

#[test]
fn test_memory_inspector_cstring_read() {
    // Create a WASM module with C-string
    let wasm = wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (data (i32.const 0) "Hello, MielinOS!\00")
            (func (export "_start"))
        )
        "#,
    )
    .unwrap();

    let executor = WasmExecutor::new().unwrap();
    let module = executor.compile_module(&wasm).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Get memory data
    let memory_data = mielin_wasm::debug::get_memory_data(&mut store, &instance).unwrap();

    // Read C-string
    let inspector = MemoryInspector::new(0, memory_data.len());
    let string = inspector.read_cstring(&memory_data, 0).unwrap();
    assert_eq!(string, "Hello, MielinOS!");
}

#[test]
fn test_source_map_integration() {
    let mut source_map = SourceMap::new("test_module".to_string());

    // Add source content
    source_map.add_source(
        "main.rs".to_string(),
        r#"fn main() {
    let x = 42;
    println!("x = {}", x);
}
"#
        .to_string(),
    );

    // Add source map entries
    source_map.add_entry(SourceMapEntry {
        wasm_offset: 0,
        file: "main.rs".to_string(),
        line: 1,
        column: 1,
        function_name: Some("main".to_string()),
    });

    source_map.add_entry(SourceMapEntry {
        wasm_offset: 50,
        file: "main.rs".to_string(),
        line: 2,
        column: 5,
        function_name: Some("main".to_string()),
    });

    source_map.add_entry(SourceMapEntry {
        wasm_offset: 100,
        file: "main.rs".to_string(),
        line: 3,
        column: 5,
        function_name: Some("main".to_string()),
    });

    // Test lookups
    let entry = source_map.lookup(0).unwrap();
    assert_eq!(entry.line, 1);
    assert_eq!(entry.file, "main.rs");

    let entry = source_map.lookup(75).unwrap();
    assert_eq!(entry.line, 2);

    let entry = source_map.lookup(150).unwrap();
    assert_eq!(entry.line, 3);

    // Test source line retrieval
    let line = source_map.get_source_line("main.rs", 2).unwrap();
    assert!(line.contains("let x = 42"));
}

#[test]
fn test_debug_context_with_breakpoints() {
    let mut ctx = DebugContext::new();

    // Add a source map
    let mut source_map = SourceMap::new("test".to_string());
    source_map.add_entry(SourceMapEntry {
        wasm_offset: 100,
        file: "test.rs".to_string(),
        line: 10,
        column: 5,
        function_name: Some("test_func".to_string()),
    });
    ctx.add_source_map("test_module".to_string(), source_map);

    // Add breakpoints
    let bp1 = ctx.add_breakpoint(mielin_wasm::debug::BreakpointType::Offset(100));
    let bp2 = ctx.add_breakpoint(mielin_wasm::debug::BreakpointType::FunctionEntry(5));

    let breakpoints = ctx.list_breakpoints();
    assert_eq!(breakpoints.len(), 2);

    // Disable a breakpoint
    ctx.set_breakpoint_enabled(bp1, false).unwrap();

    let breakpoints = ctx.list_breakpoints();
    let bp = breakpoints.iter().find(|b| b.id == bp1).unwrap();
    assert!(!bp.enabled);

    // Remove a breakpoint
    assert!(ctx.remove_breakpoint(bp2));
    assert_eq!(ctx.list_breakpoints().len(), 1);

    // Verify source map is accessible
    let sm = ctx.get_source_map("test_module").unwrap();
    assert_eq!(sm.module_name(), "test");
}

#[test]
fn test_debugger_state_transitions() {
    let mut ctx = DebugContext::new();

    // Initial state should be Running
    assert_eq!(ctx.state(), mielin_wasm::debug::DebuggerState::Running);

    // Transition to Stepping
    ctx.set_state(mielin_wasm::debug::DebuggerState::Stepping);
    assert_eq!(ctx.state(), mielin_wasm::debug::DebuggerState::Stepping);
    assert!(ctx.should_stop(0, 0));

    // Transition to Stopped
    ctx.set_state(mielin_wasm::debug::DebuggerState::Stopped);
    assert_eq!(ctx.state(), mielin_wasm::debug::DebuggerState::Stopped);

    // Back to Running
    ctx.set_state(mielin_wasm::debug::DebuggerState::Running);
    assert_eq!(ctx.state(), mielin_wasm::debug::DebuggerState::Running);
}

#[test]
fn test_memory_hex_dump_formatting() {
    // Create data with recognizable patterns
    let mut memory = vec![0u8; 256];

    // Write some recognizable data
    for (i, byte) in memory.iter_mut().take(16).enumerate() {
        *byte = i as u8;
    }

    // Write ASCII "TEST"
    memory[16] = b'T';
    memory[17] = b'E';
    memory[18] = b'S';
    memory[19] = b'T';

    let inspector = MemoryInspector::new(0, 32).with_bytes_per_row(16);
    let dump = inspector.dump_hex(&memory);

    // Verify hex dump contains expected values
    assert!(dump.contains("00 01 02 03"));
    assert!(dump.contains("TEST"));
    assert!(dump.contains("00000000  "));
    assert!(dump.contains("|"));
}

#[test]
fn test_stack_trace_formatting() {
    let mut trace = mielin_wasm::debug::StackTrace {
        message: "Division by zero".to_string(),
        frames: Vec::new(),
    };

    // Add frames
    trace.add_frame(mielin_wasm::debug::StackFrame {
        func_index: 0,
        func_name: Some("divide".to_string()),
        wasm_offset: 0x100,
        source_location: Some(SourceMapEntry {
            wasm_offset: 0x100,
            file: "math.rs".to_string(),
            line: 42,
            column: 10,
            function_name: Some("divide".to_string()),
        }),
        module_name: "math_module".to_string(),
    });

    trace.add_frame(mielin_wasm::debug::StackFrame {
        func_index: 1,
        func_name: Some("main".to_string()),
        wasm_offset: 0x50,
        source_location: Some(SourceMapEntry {
            wasm_offset: 0x50,
            file: "main.rs".to_string(),
            line: 10,
            column: 5,
            function_name: Some("main".to_string()),
        }),
        module_name: "main_module".to_string(),
    });

    let formatted = trace.format();
    assert!(formatted.contains("Division by zero"));
    assert!(formatted.contains("divide"));
    assert!(formatted.contains("math.rs:42:10"));
    assert!(formatted.contains("main"));
    assert!(formatted.contains("main.rs:10:5"));
}
