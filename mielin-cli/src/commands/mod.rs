//! Command handlers for MielinCTL

pub mod agent;
pub mod audit;
pub mod cluster;
pub mod config;
pub mod daemon;
pub mod debug;
pub mod gossip;
pub mod history;
pub mod mesh;
pub mod migrate;
pub mod monitor;
pub mod node;
pub mod plugin;
pub mod registry;
pub mod remote;
pub mod script;
pub mod wasm;

pub use agent::handle_agent_command;
pub use audit::handle_audit_command;
pub use cluster::handle_cluster_command;
pub use config::handle_config_command;
pub use daemon::handle_daemon_command;
pub use debug::handle_debug_command;
pub use gossip::handle_gossip_command;
pub use mesh::handle_mesh_command;
pub use migrate::handle_migrate_command;
pub use monitor::handle_monitor_command;
pub use node::handle_node_command;
pub use plugin::handle_plugin_command;
pub use registry::handle_registry_command;
pub use remote::handle_remote_command;
pub use script::handle_script_command;
pub use wasm::handle_wasm_command;
