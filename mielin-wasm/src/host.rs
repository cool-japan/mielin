//! Host functions and capabilities

use crate::system::{HasSystemState, SystemHostFunctions, SystemState};
use crate::wasi::WasiContext;
use mielin_hal::capabilities::HardwareCapabilities;
use mielin_tensor::{Tensor, TensorRuntime};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Linker};

/// Host state accessible to WASM agents
pub struct HostState {
    tensor_runtime: Arc<TensorRuntime>,
    tensors: Arc<Mutex<TensorStore>>,
    system_state: SystemState,
    wasi_context: Option<Arc<WasiContext>>,
}

impl HostState {
    pub fn new(capabilities: HardwareCapabilities) -> Self {
        Self {
            tensor_runtime: Arc::new(TensorRuntime::new(capabilities)),
            tensors: Arc::new(Mutex::new(TensorStore::new())),
            system_state: SystemState::new(),
            wasi_context: None,
        }
    }

    /// Create with a specific random seed for deterministic execution
    pub fn with_seed(capabilities: HardwareCapabilities, seed: u64) -> Self {
        Self {
            tensor_runtime: Arc::new(TensorRuntime::new(capabilities)),
            tensors: Arc::new(Mutex::new(TensorStore::new())),
            system_state: SystemState::with_seed(seed),
            wasi_context: None,
        }
    }

    /// Create with WASI support
    pub fn with_wasi(capabilities: HardwareCapabilities, wasi: WasiContext) -> Self {
        Self {
            tensor_runtime: Arc::new(TensorRuntime::new(capabilities)),
            tensors: Arc::new(Mutex::new(TensorStore::new())),
            system_state: SystemState::new(),
            wasi_context: Some(Arc::new(wasi)),
        }
    }

    /// Create with WASI support and custom seed
    pub fn with_wasi_and_seed(
        capabilities: HardwareCapabilities,
        wasi: WasiContext,
        seed: u64,
    ) -> Self {
        Self {
            tensor_runtime: Arc::new(TensorRuntime::new(capabilities)),
            tensors: Arc::new(Mutex::new(TensorStore::new())),
            system_state: SystemState::with_seed(seed),
            wasi_context: Some(Arc::new(wasi)),
        }
    }

    pub fn tensor_runtime(&self) -> &TensorRuntime {
        &self.tensor_runtime
    }

    pub fn tensors(&self) -> &Arc<Mutex<TensorStore>> {
        &self.tensors
    }

    pub fn system_state(&self) -> &SystemState {
        &self.system_state
    }

    pub fn wasi_context(&self) -> Option<&Arc<WasiContext>> {
        self.wasi_context.as_ref()
    }
}

impl HasSystemState for HostState {
    fn system_state(&self) -> &SystemState {
        &self.system_state
    }
}

/// Storage for tensors created by WASM agents
pub struct TensorStore {
    tensors: HashMap<u32, Tensor<f32>>,
    next_id: u32,
}

impl TensorStore {
    pub fn new() -> Self {
        Self {
            tensors: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn allocate(&mut self, tensor: Tensor<f32>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.tensors.insert(id, tensor);
        id
    }

    pub fn get(&self, id: u32) -> Option<&Tensor<f32>> {
        self.tensors.get(&id)
    }

    pub fn remove(&mut self, id: u32) -> Option<Tensor<f32>> {
        self.tensors.remove(&id)
    }

    pub fn clear(&mut self) {
        self.tensors.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.tensors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tensors.len()
    }
}

impl Default for TensorStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Host functions exposed to WASM agents
pub struct HostFunctions;

impl HostFunctions {
    /// Register all host functions with the Wasmtime linker
    pub fn register(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
        // System host functions (time, random, env, process)
        SystemHostFunctions::register(linker)?;

        // WASI functions (if needed, will be conditionally available)
        // WASI functions are registered separately via register_wasi_functions
        // when a WasiContext is provided

        // Hardware capability queries
        linker.func_wrap(
            "mielin",
            "tensor_supports_sve2",
            |caller: Caller<'_, HostState>| caller.data().tensor_runtime().supports_sve2() as i32,
        )?;

        linker.func_wrap(
            "mielin",
            "tensor_supports_neon",
            |caller: Caller<'_, HostState>| caller.data().tensor_runtime().supports_neon() as i32,
        )?;

        linker.func_wrap(
            "mielin",
            "tensor_supports_avx2",
            |caller: Caller<'_, HostState>| caller.data().tensor_runtime().supports_avx2() as i32,
        )?;

        // Tensor creation
        linker.func_wrap(
            "mielin",
            "tensor_zeros",
            |mut caller: Caller<'_, HostState>, shape_ptr: i32, shape_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                let shape_offset = shape_ptr as usize;
                let shape_size = (shape_len as usize) * 4; // u32 = 4 bytes

                let data = memory.data(&caller);
                if shape_offset + shape_size > data.len() {
                    return 0;
                }

                let (chunks, _remainder) =
                    data[shape_offset..shape_offset + shape_size].as_chunks::<4>();
                let shape: Vec<usize> = chunks
                    .iter()
                    .map(|chunk| {
                        u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize
                    })
                    .collect();

                let tensor = Tensor::zeros(shape);
                let mut store = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");
                store.allocate(tensor) as i32
            },
        )?;

        linker.func_wrap(
            "mielin",
            "tensor_ones",
            |mut caller: Caller<'_, HostState>, shape_ptr: i32, shape_len: i32| -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                let shape_offset = shape_ptr as usize;
                let shape_size = (shape_len as usize) * 4;

                let data = memory.data(&caller);
                if shape_offset + shape_size > data.len() {
                    return 0;
                }

                let (chunks, _remainder) =
                    data[shape_offset..shape_offset + shape_size].as_chunks::<4>();
                let shape: Vec<usize> = chunks
                    .iter()
                    .map(|chunk| {
                        u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize
                    })
                    .collect();

                let tensor = Tensor::ones(shape);
                let mut store = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");
                store.allocate(tensor) as i32
            },
        )?;

        // Tensor operations
        linker.func_wrap(
            "mielin",
            "tensor_dot",
            |mut caller: Caller<'_, HostState>, a_id: i32, b_id: i32, result_ptr: i32| -> i32 {
                let store_guard = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");

                let a = match store_guard.get(a_id as u32) {
                    Some(t) => t,
                    None => return 0,
                };
                let b = match store_guard.get(b_id as u32) {
                    Some(t) => t,
                    None => return 0,
                };

                let result = match caller.data().tensor_runtime().ops().dot(a, b) {
                    Some(r) => r,
                    None => return 0,
                };

                drop(store_guard);

                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                let offset = result_ptr as usize;
                let data = memory.data_mut(&mut caller);
                if offset + 4 > data.len() {
                    return 0;
                }

                data[offset..offset + 4].copy_from_slice(&result.to_le_bytes());
                1
            },
        )?;

        linker.func_wrap(
            "mielin",
            "tensor_add",
            |caller: Caller<'_, HostState>, a_id: i32, b_id: i32| -> i32 {
                let mut store_guard = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");

                let a = match store_guard.get(a_id as u32) {
                    Some(t) => t.clone(),
                    None => return 0,
                };
                let b = match store_guard.get(b_id as u32) {
                    Some(t) => t.clone(),
                    None => return 0,
                };

                let result = match caller.data().tensor_runtime().ops().add(&a, &b) {
                    Some(r) => r,
                    None => return 0,
                };

                store_guard.allocate(result) as i32
            },
        )?;

        linker.func_wrap(
            "mielin",
            "tensor_matmul",
            |caller: Caller<'_, HostState>, a_id: i32, b_id: i32| -> i32 {
                let mut store_guard = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");

                let a = match store_guard.get(a_id as u32) {
                    Some(t) => t.clone(),
                    None => return 0,
                };
                let b = match store_guard.get(b_id as u32) {
                    Some(t) => t.clone(),
                    None => return 0,
                };

                let result = match caller.data().tensor_runtime().ops().matmul(&a, &b) {
                    Some(r) => r,
                    None => return 0,
                };

                store_guard.allocate(result) as i32
            },
        )?;

        // Tensor introspection
        linker.func_wrap(
            "mielin",
            "tensor_get_shape",
            |mut caller: Caller<'_, HostState>,
             tensor_id: i32,
             shape_ptr: i32,
             max_dims: i32|
             -> i32 {
                let store_guard = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");

                let tensor = match store_guard.get(tensor_id as u32) {
                    Some(t) => t,
                    None => return 0,
                };

                let shape: Vec<usize> = tensor.shape().to_vec();
                let dims_to_copy = shape.len().min(max_dims as usize);

                drop(store_guard);

                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return 0,
                };

                let offset = shape_ptr as usize;
                let data = memory.data_mut(&mut caller);
                if offset + dims_to_copy * 4 > data.len() {
                    return 0;
                }

                for (i, &dim) in shape.iter().take(dims_to_copy).enumerate() {
                    let bytes = (dim as u32).to_le_bytes();
                    data[offset + i * 4..offset + (i + 1) * 4].copy_from_slice(&bytes);
                }

                dims_to_copy as i32
            },
        )?;

        linker.func_wrap(
            "mielin",
            "tensor_free",
            |caller: Caller<'_, HostState>, tensor_id: i32| -> i32 {
                let mut store_guard = caller
                    .data()
                    .tensors()
                    .lock()
                    .expect("Tensor store lock poisoned");
                if store_guard.remove(tensor_id as u32).is_some() {
                    1
                } else {
                    0
                }
            },
        )?;

        Ok(())
    }
}
