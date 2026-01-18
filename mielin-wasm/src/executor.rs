//! WebAssembly execution engine

use crate::host::{HostFunctions, HostState};
use crate::WasmError;
use mielin_cells::Agent;
use mielin_hal::capabilities::HardwareCapabilities;
use wasmtime::*;

pub struct WasmExecutor {
    engine: Engine,
    linker: Linker<HostState>,
}

impl WasmExecutor {
    pub fn new() -> Result<Self, WasmError> {
        Self::with_capabilities(HardwareCapabilities::NONE)
    }

    pub fn with_capabilities(_capabilities: HardwareCapabilities) -> Result<Self, WasmError> {
        let mut config = Config::new();
        config.wasm_multi_memory(true);
        config.wasm_multi_value(true);
        config.async_support(false);

        let engine =
            Engine::new(&config).map_err(|e| WasmError::CompilationFailed(e.to_string()))?;

        let mut linker = Linker::new(&engine);
        HostFunctions::register(&mut linker)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))?;

        Ok(Self { engine, linker })
    }

    pub fn with_config(
        config: Config,
        _capabilities: HardwareCapabilities,
    ) -> Result<Self, WasmError> {
        let engine =
            Engine::new(&config).map_err(|e| WasmError::CompilationFailed(e.to_string()))?;

        let mut linker = Linker::new(&engine);
        HostFunctions::register(&mut linker)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))?;

        Ok(Self { engine, linker })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn compile_module(&self, wasm_bytes: &[u8]) -> Result<Module, WasmError> {
        Module::new(&self.engine, wasm_bytes)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))
    }

    pub fn instantiate(
        &self,
        module: &Module,
        capabilities: HardwareCapabilities,
    ) -> Result<(Instance, Store<HostState>), WasmError> {
        let host_state = HostState::new(capabilities);
        let mut store = Store::new(&self.engine, host_state);

        let instance = self
            .linker
            .instantiate(&mut store, module)
            .map_err(|e| WasmError::ExecutionFailed(e.to_string()))?;

        Ok((instance, store))
    }

    pub fn execute(&self, agent: &Agent) -> Result<ExecutionResult, WasmError> {
        self.execute_with_capabilities(agent, HardwareCapabilities::NONE)
    }

    pub fn execute_with_capabilities(
        &self,
        agent: &Agent,
        capabilities: HardwareCapabilities,
    ) -> Result<ExecutionResult, WasmError> {
        let wasm_bytes = agent.dna().binary();

        if wasm_bytes.len() < 4 || &wasm_bytes[0..4] != b"\0asm" {
            return Err(WasmError::InvalidModule(
                "Invalid WASM magic number".to_string(),
            ));
        }

        let module = self.compile_module(wasm_bytes)?;
        let (instance, mut store) = self.instantiate(&module, capabilities)?;

        let start_func = instance
            .get_func(&mut store, "_start")
            .or_else(|| instance.get_func(&mut store, "main"));

        if let Some(func) = start_func {
            func.call(&mut store, &[], &mut [])
                .map_err(|e| WasmError::ExecutionFailed(e.to_string()))?;
        }

        Ok(ExecutionResult {
            exit_code: 0,
            memory_snapshot: vec![],
        })
    }

    pub fn suspend(&self, agent: &Agent) -> Result<Vec<u8>, WasmError> {
        let wasm_bytes = agent.dna().binary();
        let module = self.compile_module(wasm_bytes)?;
        let (instance, mut store) = self.instantiate(&module, HardwareCapabilities::NONE)?;

        if let Some(memory) = instance.get_memory(&mut store, "memory") {
            let memory_data = memory.data(&store);
            Ok(memory_data.to_vec())
        } else {
            Ok(vec![])
        }
    }

    pub fn validate(&self, wasm_bytes: &[u8]) -> Result<(), WasmError> {
        Module::validate(&self.engine, wasm_bytes)
            .map_err(|e| WasmError::InvalidModule(e.to_string()))
    }
}

impl Default for WasmExecutor {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub memory_snapshot: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = WasmExecutor::new();
        assert!(executor.is_ok());
    }

    #[test]
    fn test_module_validation() {
        let executor = WasmExecutor::new().unwrap();

        let valid_wasm = wat::parse_str("(module)").unwrap();
        assert!(executor.validate(&valid_wasm).is_ok());

        let invalid_wasm = b"not wasm";
        assert!(executor.validate(invalid_wasm).is_err());
    }

    #[test]
    fn test_module_compilation() {
        let executor = WasmExecutor::new().unwrap();

        let wasm = wat::parse_str("(module)").unwrap();
        let result = executor.compile_module(&wasm);
        assert!(result.is_ok());
    }
}
