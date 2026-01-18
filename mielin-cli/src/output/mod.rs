//! Output formatting module for MielinCTL

use clap::ValueEnum;
use comfy_table::Table;
use serde::Serialize;

/// Output format for command results
#[derive(Debug, Clone, Copy, Default, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable table format (default)
    #[default]
    Table,
    /// JSON format for scripting
    Json,
    /// YAML format for configuration
    Yaml,
    /// Quiet mode - minimal output
    Quiet,
}

/// Trait for types that can be displayed in multiple formats
pub trait MultiFormatDisplay: Serialize {
    fn to_table(&self) -> Table;
    fn to_quiet(&self) -> String {
        String::new()
    }
}

/// Render output in the specified format
pub fn render_output<T: MultiFormatDisplay>(
    data: &T,
    format: OutputFormat,
) -> anyhow::Result<String> {
    match format {
        OutputFormat::Table => Ok(data.to_table().to_string()),
        OutputFormat::Json => Ok(serde_json::to_string_pretty(data)?),
        OutputFormat::Yaml => Ok(serde_yaml::to_string(data)?),
        OutputFormat::Quiet => Ok(data.to_quiet()),
    }
}
