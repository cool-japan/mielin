//! WASI Preview 2 / Component Model executor for MielinWasm.
//!
//! This module implements an async [`ComponentExecutor`] backed by
//! `wasmtime-wasi`'s WASIp2 host implementation.  The entire module is
//! compiled only when the `preview2` crate feature is enabled (gated in
//! `lib.rs` via `#[cfg(feature = "preview2")]`).
//!
//! # API surface used from `wasmtime-wasi` 43.x
//!
//! | Symbol | Source |
//! |--------|--------|
//! | `WasiView` | `wasmtime_wasi::WasiView` (top-level re-export) |
//! | `WasiCtxView<'_>` | `wasmtime_wasi::WasiCtxView` |
//! | `WasiCtx` | `wasmtime_wasi::WasiCtx` |
//! | `WasiCtxBuilder` | `wasmtime_wasi::WasiCtxBuilder` |
//! | `ResourceTable` | `wasmtime::component::ResourceTable` (re-exported) |
//! | `add_to_linker_async` | `wasmtime_wasi::p2::add_to_linker_async` |

use std::sync::Arc;

use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use mielin_hal::capabilities::HardwareCapabilities;

use crate::WasmError;

/// Host-side state threaded through every component execution.
///
/// The two fields mirror the example from the `wasmtime-wasi` docs:
/// `WasiCtx` holds per-guest configuration, and `ResourceTable` tracks
/// live resource handles (streams, files, sockets, …).
pub struct P2HostState {
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for P2HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// Result returned after executing a WebAssembly component.
#[derive(Debug, Clone)]
pub struct ComponentResult {
    /// Exit code (0 == success) – derived from the component instantiation
    /// outcome.  We always report 0 on successful instantiation because
    /// minimal components may not export `_start` / `run`.
    pub exit_code: i32,
}

/// Async executor for WASI Preview 2 / Component Model workloads.
///
/// This executor:
/// 1. Creates a wasmtime [`Engine`] with `component-model` and `async`
///    support enabled.
/// 2. Pre-populates a [`Linker`] with the full set of WASIp2 host functions
///    via [`wasmtime_wasi::p2::add_to_linker_async`].
/// 3. Exposes [`ComponentExecutor::compile`] and [`ComponentExecutor::execute`]
///    as the primary public interface.
pub struct ComponentExecutor {
    engine: Arc<Engine>,
    linker: Linker<P2HostState>,
}

impl ComponentExecutor {
    /// Create a new `ComponentExecutor`.
    ///
    /// `_caps` is the hardware capability bitfield of the host; it is accepted
    /// here for API symmetry with the rest of MielinWasm but is not yet used to
    /// restrict WASI functionality.
    pub fn new(_caps: HardwareCapabilities) -> Result<Self, WasmError> {
        let mut config = Config::new();
        config.wasm_component_model(true);

        let engine =
            Engine::new(&config).map_err(|e| WasmError::CompilationFailed(e.to_string()))?;
        let engine = Arc::new(engine);

        let mut linker: Linker<P2HostState> = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))?;

        Ok(Self { engine, linker })
    }

    /// Compile a WebAssembly component binary into a [`Component`].
    ///
    /// The bytes must be a valid Component Model binary.  Passing a
    /// plain core-module binary will return
    /// [`WasmError::CompilationFailed`].
    pub fn compile(&self, bytes: &[u8]) -> Result<Component, WasmError> {
        Component::from_binary(&self.engine, bytes)
            .map_err(|e| WasmError::CompilationFailed(e.to_string()))
    }

    /// Instantiate and execute a compiled component asynchronously.
    ///
    /// A fresh [`WasiCtx`] (with inherited stdio) and a new [`ResourceTable`]
    /// are created for every execution so that runs are fully isolated.
    ///
    /// For now this returns success after a successful instantiation.
    /// Components that export `wasi:cli/run#run` can be driven further via
    /// the typed-instance API once richer test scenarios are needed.
    pub async fn execute(&self, component: &Component) -> Result<ComponentResult, WasmError> {
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        let state = P2HostState {
            wasi,
            table: ResourceTable::new(),
        };

        let mut store = Store::new(&self.engine, state);

        // Instantiate – this wires up the WASIp2 host functions to the
        // component's imports.  We do not call any exports here; the minimal
        // test component has none beyond internal core-module functions.
        let _instance = self
            .linker
            .instantiate_async(&mut store, component)
            .await
            .map_err(|e| WasmError::ExecutionFailed(e.to_string()))?;

        Ok(ComponentResult { exit_code: 0 })
    }
}
