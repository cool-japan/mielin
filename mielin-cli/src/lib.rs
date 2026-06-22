//! MielinCTL - Command Line Interface Library
//!
//! Control and management library for MielinOS.

pub mod audit;
pub mod cli;
pub mod commands;
pub mod config;
pub mod config_validator;
pub mod control;
pub mod error;
pub mod history;
pub mod output;
pub mod plugin;
pub mod progress;
pub mod remote;
pub mod repl;
pub mod script;
pub mod timeout;
pub mod types;

pub use audit::{AuditConfig, AuditEntry, AuditEventType, AuditLogger, AuditSeverity, AuditStats};
pub use config_validator::{
    ConfigMigrator, ConfigValidator, ValidationError, ValidationResult, ValidationSuggestion,
    ValidationWarning,
};
pub use error::{format_error, CliError, ErrorContext};
pub use history::{History, HistoryEntry, HistoryStats};
pub use output::{MultiFormatDisplay, OutputFormat};
pub use plugin::{
    Plugin, PluginArgument, PluginCommand, PluginContext, PluginManager, PluginMetadata,
    PluginResult,
};
pub use progress::{with_spinner, ProgressBar, Spinner};
pub use remote::{
    AuthMethod, ConnectionOptions, RemoteCommand, RemoteCommandResult, RemoteManager, RemoteNode,
};
pub use repl::Repl;
pub use script::{Script, ScriptContext, ScriptEngine, ScriptMetadata, ScriptResult};
pub use timeout::{with_config_timeout, with_timeout};
pub use types::*;
