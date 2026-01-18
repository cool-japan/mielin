# mielin-wasm

**WebAssembly Runtime Integration**

Wasmtime-based runtime for executing agent code with capability-based security.

## Features

- **Wasmtime Integration**: Production-grade WebAssembly runtime
- **Module Compilation**: Ahead-of-time and just-in-time compilation
- **Capability Security**: Fine-grained permission system
- **Memory Snapshots**: Capture agent state for migration
- **Module Validation**: Verify WASM modules before execution

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
mielin-wasm = { path = "../mielin-wasm" }
```

### Basic Execution

```rust
use mielin_wasm::executor::WasmExecutor;
use mielin_cells::Agent;

// Create executor
let executor = WasmExecutor::new()?;

// Create agent with WASM binary
let agent = Agent::new(wasm_binary);

// Execute agent
let result = executor.execute(&agent)?;
println!("Exit code: {}", result.exit_code);
```

### Module Validation

```rust
use mielin_wasm::executor::WasmExecutor;

let executor = WasmExecutor::new()?;

// Validate WASM module
match executor.validate(&wasm_bytes) {
    Ok(_) => println!("Module is valid"),
    Err(e) => eprintln!("Invalid module: {}", e),
}
```

### Capability-Based Security

```rust
use mielin_wasm::sandbox::{Sandbox, Capability};

let mut sandbox = Sandbox::new();

// Grant specific capabilities
sandbox.grant(Capability::Network);
sandbox.grant(Capability::FileSystem);

// Check permissions
if sandbox.has_capability(&Capability::Camera) {
    // Allow camera access
} else {
    // Deny camera access
}
```

## Components

### WasmExecutor

Main execution engine:

```rust
pub struct WasmExecutor {
    engine: Engine,
}

impl WasmExecutor {
    pub fn new() -> Result<Self, WasmError>;
    pub fn with_config(config: Config) -> Result<Self, WasmError>;
    pub fn compile_module(&self, wasm_bytes: &[u8]) -> Result<Module, WasmError>;
    pub fn execute(&self, agent: &Agent) -> Result<ExecutionResult, WasmError>;
    pub fn suspend(&self, agent: &Agent) -> Result<Vec<u8>, WasmError>;
    pub fn validate(&self, wasm_bytes: &[u8]) -> Result<(), WasmError>;
}
```

### Sandbox

Capability-based security:

```rust
pub struct Sandbox {
    allowed_capabilities: HashSet<Capability>,
}

pub enum Capability {
    FileSystem,
    Network,
    Camera,
    Gpio,
}

impl Sandbox {
    pub fn new() -> Self;
    pub fn grant(&mut self, cap: Capability);
    pub fn has_capability(&self, cap: &Capability) -> bool;
}
```

### ExecutionResult

Execution outcome:

```rust
pub struct ExecutionResult {
    pub exit_code: i32,
    pub memory_snapshot: Vec<u8>,
}
```

## Examples

### Execute Simple Module

```rust
use mielin_wasm::executor::WasmExecutor;
use mielin_cells::Agent;

// WAT format (WebAssembly Text)
let wat = r#"
    (module
        (func (export "_start")
            ;; Agent code here
        )
    )
"#;

let wasm = wat::parse_str(wat)?;
let agent = Agent::new(wasm);

let executor = WasmExecutor::new()?;
let result = executor.execute(&agent)?;

assert_eq!(result.exit_code, 0);
```

### Capture Memory Snapshot

```rust
use mielin_wasm::executor::WasmExecutor;

let executor = WasmExecutor::new()?;

// Execute agent
executor.execute(&agent)?;

// Suspend and capture memory
let memory_snapshot = executor.suspend(&agent)?;

// Use snapshot for migration
println!("Captured {} bytes of memory", memory_snapshot.len());
```

### Custom Configuration

```rust
use mielin_wasm::executor::WasmExecutor;
use wasmtime::Config;

let mut config = Config::new();
config.wasm_multi_memory(true);
config.wasm_multi_value(true);
config.consume_fuel(true); // Enable fuel metering

let executor = WasmExecutor::with_config(config)?;
```

### Sandbox Example

```rust
use mielin_wasm::sandbox::{Sandbox, Capability};

// Create restricted sandbox
let mut sandbox = Sandbox::new();

// Grant only network access
sandbox.grant(Capability::Network);

// Before allowing operation, check capability
fn allow_camera_access(sandbox: &Sandbox) -> bool {
    sandbox.has_capability(&Capability::Camera)
}

assert!(!allow_camera_access(&sandbox)); // Denied
```

## Capabilities

### Available Capabilities

| Capability | Description | Use Case |
|------------|-------------|----------|
| `FileSystem` | Read/write files | Data persistence |
| `Network` | Network I/O | Communication |
| `Camera` | Camera access | Image capture |
| `Gpio` | GPIO pins | Hardware control |

### Capability Model

The capability model follows the principle of least privilege:

1. **No capabilities by default**: Agents start with zero permissions
2. **Explicit grants**: Each capability must be explicitly granted
3. **Revocable**: Capabilities can be revoked at runtime (future)
4. **Auditable**: Capability checks are logged (future)

## Error Handling

```rust
pub enum WasmError {
    CompilationFailed(String),
    ExecutionFailed(String),
    InvalidModule(String),
}
```

## Performance

Execution benchmarks:

| Operation | Time | Notes |
|-----------|------|-------|
| Module compilation | ~10-50 ms | Depends on module size |
| Module validation | ~1-5 ms | Fast safety checks |
| Instantiation | ~100 μs | Create instance |
| Simple function call | ~10 ns | Native-like speed |
| Memory snapshot | ~1 μs/KB | Linear in memory size |

## Testing

```bash
cargo test -p mielin-wasm
```

Tests include:
- Executor creation
- Module compilation
- Module validation
- Capability granting and checking
- Execution with various WASM modules

## Limitations

Current limitations:
- No async support (future enhancement)
- Limited host function imports
- No WASI support yet
- Sandbox is basic (more capabilities needed)

## Advanced Usage

### Fuel Metering

Limit agent execution time:

```rust
let mut config = Config::new();
config.consume_fuel(true);

let executor = WasmExecutor::with_config(config)?;
// Set fuel limit before execution
```

### Multi-Memory Support

Enable multiple linear memories:

```rust
let mut config = Config::new();
config.wasm_multi_memory(true);

let executor = WasmExecutor::with_config(config)?;
```

## Future Enhancements

- [ ] Async/await support for agents
- [ ] WASI (WebAssembly System Interface)
- [ ] More comprehensive host functions
- [ ] Resource limits (CPU, memory)
- [ ] Profiling and debugging support
- [ ] Hot-reload for agent code
- [ ] Shared memory between agents

## Security Considerations

1. **Always validate modules**: Use `validate()` before execution
2. **Sandbox all agents**: Never grant unnecessary capabilities
3. **Limit resources**: Implement fuel metering for production
4. **Audit capability usage**: Log all capability checks
5. **Isolate agents**: Each agent in separate instance

## Best Practices

1. **Compile once, execute many**: Cache compiled modules
2. **Minimize memory**: Keep agent state small
3. **Use snapshots**: Capture state only when needed
4. **Validate early**: Check modules before deployment
5. **Grant minimal capabilities**: Follow least privilege

## License

MIT OR Apache-2.0
