# MielinWasm Host Function Development Examples

This directory contains examples demonstrating how to develop custom host functions for MielinWasm.

## Examples

### 1. Basic Host Function (`basic_host_function.rs`)

Demonstrates the fundamentals of creating a simple host function:
- Defining a host function
- Linking it to the WASM module
- Calling it from WASM code
- Accessing host state

**Run**: `cargo run --example basic_host_function`

### 2. Memory Access (`memory_host_function.rs`)

Shows how to safely access WASM linear memory from host functions:
- Validating memory pointers and bounds
- Reading data from WASM memory
- Writing data to WASM memory
- String manipulation across the boundary

**Run**: `cargo run --example memory_host_function`

### 3. Capability-Based Security (`capability_host_function.rs`)

Demonstrates capability checking in host functions:
- Checking filesystem permissions
- Path whitelisting
- Permission denial handling
- Secure resource access

**Run**: `cargo run --example capability_host_function`

### 4. Async Operations (`async_host_function.rs`)

Shows how to handle long-running operations with cooperative yielding:
- Timeout configuration
- Cooperative scheduling
- Fuel metering
- Yield points

**Run**: `cargo run --example async_host_function`

## Best Practices

### Security

1. **Always validate inputs**:
   ```rust
   if ptr as usize + len as usize > memory.data_size(store) {
       return Err("Out of bounds");
   }
   ```

2. **Check capabilities first**:
   ```rust
   let perms = state.fs_permissions()
       .ok_or("No filesystem access")?;
   if !perms.can_read {
       return Err("Permission denied");
   }
   ```

3. **Use safe error handling**:
   ```rust
   match operation() {
       Ok(v) => Ok(v),
       Err(e) => {
           log::error!("Error: {}", e);
           Ok(-1) // Return error code
       }
   }
   ```

### Performance

1. **Minimize memory copies**:
   ```rust
   // Good: Use slice directly
   let data = &memory.data(store)[ptr..ptr+len];

   // Bad: Unnecessary copy
   let data = memory.data(store)[ptr..ptr+len].to_vec();
   ```

2. **Cache expensive lookups**:
   ```rust
   // Cache memory export lookup
   let memory = caller.get_export("memory")
       .and_then(|e| e.into_memory())
       .ok_or("No memory export")?;
   ```

3. **Use appropriate data structures**:
   ```rust
   // Use FxHashMap for integer keys
   use rustc_hash::FxHashMap;
   let map: FxHashMap<u32, Value> = FxHashMap::default();
   ```

### Error Handling

1. **Return error codes to WASM**:
   ```rust
   // Convention: -1 for error, >= 0 for success
   fn my_host_func() -> i32 {
       match do_work() {
           Ok(value) => value as i32,
           Err(_) => -1,
       }
   }
   ```

2. **Log detailed errors**:
   ```rust
   match operation() {
       Err(e) => {
           log::error!("Operation failed: {:?}", e);
           log::debug!("Stack trace: {:?}", e.backtrace());
           return Ok(-1);
       }
       Ok(v) => Ok(v),
   }
   ```

3. **Use Result types consistently**:
   ```rust
   fn my_host_func() -> anyhow::Result<i32> {
       let value = risky_operation()?;
       Ok(value)
   }
   ```

## Common Patterns

### Pattern 1: Resource Handle Management

```rust
struct ResourceManager {
    handles: HashMap<u32, Resource>,
    next_handle: u32,
}

impl ResourceManager {
    fn create(&mut self, resource: Resource) -> u32 {
        let handle = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(handle, resource);
        handle
    }

    fn get(&self, handle: u32) -> Option<&Resource> {
        self.handles.get(&handle)
    }

    fn remove(&mut self, handle: u32) -> Option<Resource> {
        self.handles.remove(&handle)
    }
}
```

### Pattern 2: Capability Checking

```rust
fn check_capability<T>(
    state: &HostState,
    capability: impl Fn(&T) -> bool,
) -> Result<&T, Error> {
    let cap = state.get_capability()
        .ok_or(Error::NoCapability)?;

    if !capability(cap) {
        return Err(Error::PermissionDenied);
    }

    Ok(cap)
}
```

### Pattern 3: Memory Buffer Transfer

```rust
fn read_buffer(
    memory: &Memory,
    store: &Store,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, Error> {
    validate_bounds(memory, store, ptr, len)?;

    let data = memory.data(store);
    let slice = &data[ptr as usize..(ptr + len) as usize];

    Ok(slice.to_vec())
}

fn write_buffer(
    memory: &Memory,
    mut store: &mut Store,
    ptr: u32,
    data: &[u8],
) -> Result<(), Error> {
    validate_bounds(memory, &store, ptr, data.len() as u32)?;

    let mem_data = memory.data_mut(&mut store);
    mem_data[ptr as usize..ptr as usize + data.len()].copy_from_slice(data);

    Ok(())
}
```

## Testing Host Functions

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_function() {
        let executor = WasmExecutor::new().unwrap();
        let wasm = wat::parse_str(r#"
            (module
                (import "env" "my_func" (func $my_func (result i32)))
                (func (export "test") (result i32)
                    call $my_func
                )
            )
        "#).unwrap();

        let module = executor.compile_module(&wasm).unwrap();
        let (instance, mut store) = executor
            .instantiate(&module, HardwareCapabilities::NONE)
            .unwrap();

        let test_func = instance
            .get_typed_func::<(), i32>(&mut store, "test")
            .unwrap();

        let result = test_func.call(&mut store, ()).unwrap();
        assert_eq!(result, 42);
    }
}
```

### Integration Testing

```rust
#[test]
fn test_with_real_wasm_file() {
    let executor = WasmExecutor::new().unwrap();
    let wasm_bytes = std::fs::read("test.wasm").unwrap();

    let module = executor.compile_module(&wasm_bytes).unwrap();
    let (instance, mut store) = executor
        .instantiate(&module, HardwareCapabilities::NONE)
        .unwrap();

    // Test functionality
}
```

## Additional Resources

- [Wasmtime Book](https://docs.wasmtime.dev/)
- [WebAssembly Specification](https://webassembly.github.io/spec/)
- [MielinOS Documentation](../README.md)
- [Security Model](../docs/SECURITY.md)

## Contributing

When adding new examples:

1. Follow the naming convention: `<topic>_host_function.rs`
2. Include comprehensive documentation
3. Add error handling examples
4. Demonstrate best practices
5. Update this README
