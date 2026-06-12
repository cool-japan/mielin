//! HTTP control-plane for the MielinOS daemon
//!
//! Provides:
//! - `ControlServer` — an axum HTTP server that exposes `MeshService` state
//! - `ControlClient` — a typed `oxihttp-client` wrapper that speaks to the server
//! - `dto` — the shared serde-serialisable data transfer objects

pub mod client;
pub mod dto;
pub mod server;

pub use client::ControlClient;
pub use server::ControlServer;
