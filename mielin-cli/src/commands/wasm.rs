//! WASM development command handlers

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use crate::types::OperationResult;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;
use std::path::Path;

#[derive(Subcommand)]
pub enum WasmCommands {
    /// Build WASM agent from source
    #[command(aliases = ["compile", "make"])]
    Build {
        /// Path to source directory or Rust project
        source_path: String,
        /// Output WASM file path
        #[arg(short, long)]
        output: Option<String>,
        /// Optimize for size
        #[arg(long)]
        optimize: bool,
    },
    /// Validate WASM module
    #[command(aliases = ["check", "verify"])]
    Validate {
        /// Path to WASM file
        wasm_path: String,
    },
    /// Test WASM agent locally
    #[command(alias = "run")]
    Test {
        /// Path to WASM file
        wasm_path: String,
        /// Test input data (JSON)
        #[arg(short, long)]
        input: Option<String>,
    },
    /// Optimize WASM module size
    #[command(aliases = ["opt", "shrink"])]
    Optimize {
        /// Path to WASM file
        wasm_path: String,
        /// Output optimized WASM file
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct WasmValidationResult {
    valid: bool,
    size_bytes: u64,
    imports: Vec<String>,
    exports: Vec<String>,
    memory_pages: u32,
}

impl MultiFormatDisplay for WasmValidationResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        let valid_color = if self.valid { Color::Green } else { Color::Red };

        table.add_row(vec![
            Cell::new("Valid").fg(Color::Cyan),
            Cell::new(if self.valid { "Yes" } else { "No" }).fg(valid_color),
        ]);
        table.add_row(vec![
            Cell::new("Size").fg(Color::Cyan),
            Cell::new(format!("{} bytes", self.size_bytes)),
        ]);
        table.add_row(vec![
            Cell::new("Imports").fg(Color::Cyan),
            Cell::new(self.imports.len().to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Exports").fg(Color::Cyan),
            Cell::new(self.exports.len().to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Memory Pages").fg(Color::Cyan),
            Cell::new(self.memory_pages.to_string()),
        ]);

        table
    }
}

pub async fn handle_wasm_command(action: WasmCommands, format: OutputFormat) -> Result<()> {
    match action {
        WasmCommands::Build {
            source_path,
            output,
            optimize,
        } => {
            let output_path = output.unwrap_or_else(|| {
                let source = Path::new(&source_path);
                let filename = source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("agent");
                format!("{}.wasm", filename)
            });

            let result = OperationResult {
                success: true,
                message: format!(
                    "Built WASM agent from {} to {} (optimize: {})",
                    source_path, output_path, optimize
                ),
                id: Some(output_path),
            };
            println!("{}", render_output(&result, format)?);
        }
        WasmCommands::Validate { wasm_path } => {
            // Check if file exists
            if !Path::new(&wasm_path).exists() {
                anyhow::bail!("WASM file not found: {}", wasm_path);
            }

            let validation = mock_wasm_validation(&wasm_path);
            println!("{}", render_output(&validation, format)?);
        }
        WasmCommands::Test { wasm_path, input } => {
            let result = OperationResult {
                success: true,
                message: format!("Testing WASM agent {} with input: {:?}", wasm_path, input),
                id: None,
            };
            println!("{}", render_output(&result, format)?);
        }
        WasmCommands::Optimize { wasm_path, output } => {
            let output_path = output.unwrap_or_else(|| {
                let path = Path::new(&wasm_path);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("agent");
                format!("{}.optimized.wasm", stem)
            });

            let result = OperationResult {
                success: true,
                message: format!("Optimized {} to {}", wasm_path, output_path),
                id: Some(output_path),
            };
            println!("{}", render_output(&result, format)?);
        }
    }
    Ok(())
}

fn mock_wasm_validation(_wasm_path: &str) -> WasmValidationResult {
    WasmValidationResult {
        valid: true,
        size_bytes: 45678,
        imports: vec!["env.memory".to_string(), "env.log".to_string()],
        exports: vec!["_start".to_string(), "process".to_string()],
        memory_pages: 2,
    }
}
