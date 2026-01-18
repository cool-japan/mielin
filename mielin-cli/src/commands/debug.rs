//! Debugging command handlers

use crate::output::{render_output, MultiFormatDisplay, OutputFormat};
use crate::types::OperationResult;
use anyhow::Result;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Subcommand)]
pub enum DebugCommands {
    /// Attach debugger to an agent
    #[command(alias = "connect")]
    Attach {
        /// Agent ID to attach to
        agent_id: String,
        /// Debugger port
        #[arg(short, long, default_value = "9229")]
        port: u16,
    },
    /// Trace agent execution
    #[command(aliases = ["track", "follow"])]
    Trace {
        /// Agent ID to trace
        agent_id: String,
        /// Duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
        /// Output file for trace data
        #[arg(short = 'f', long = "file")]
        output_file: Option<String>,
    },
    /// Dump agent state
    #[command(aliases = ["snapshot", "export"])]
    Dump {
        /// Agent ID to dump
        agent_id: String,
        /// Include memory snapshot
        #[arg(short, long)]
        memory: bool,
        /// Dump format (json, binary, text)
        #[arg(long, default_value = "json")]
        dump_format: String,
    },
    /// Show agent profiling data
    #[command(aliases = ["prof", "perf"])]
    Profile {
        /// Agent ID to profile
        agent_id: String,
        /// Show CPU profile
        #[arg(long)]
        cpu: bool,
        /// Show memory profile
        #[arg(long)]
        mem: bool,
    },
}

#[derive(Debug, Serialize)]
struct DebugSession {
    agent_id: String,
    session_id: String,
    debugger_url: String,
    status: String,
}

impl MultiFormatDisplay for DebugSession {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Agent ID").fg(Color::Cyan),
            Cell::new(&self.agent_id),
        ]);
        table.add_row(vec![
            Cell::new("Session ID").fg(Color::Cyan),
            Cell::new(&self.session_id),
        ]);
        table.add_row(vec![
            Cell::new("Debugger URL").fg(Color::Cyan),
            Cell::new(&self.debugger_url),
        ]);
        table.add_row(vec![
            Cell::new("Status").fg(Color::Cyan),
            Cell::new(&self.status).fg(Color::Green),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        self.debugger_url.clone()
    }
}

#[derive(Debug, Serialize)]
struct TraceResult {
    agent_id: String,
    duration_secs: u64,
    events_captured: u64,
    output_file: String,
    file_size_kb: u64,
}

impl MultiFormatDisplay for TraceResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Agent ID").fg(Color::Cyan),
            Cell::new(&self.agent_id),
        ]);
        table.add_row(vec![
            Cell::new("Duration").fg(Color::Cyan),
            Cell::new(format!("{}s", self.duration_secs)),
        ]);
        table.add_row(vec![
            Cell::new("Events Captured").fg(Color::Cyan),
            Cell::new(self.events_captured.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Output File").fg(Color::Cyan),
            Cell::new(&self.output_file),
        ]);
        table.add_row(vec![
            Cell::new("File Size").fg(Color::Cyan),
            Cell::new(format!("{} KB", self.file_size_kb)),
        ]);

        table
    }

    fn to_quiet(&self) -> String {
        self.output_file.clone()
    }
}

#[derive(Debug, Serialize)]
struct AgentDump {
    agent_id: String,
    timestamp: String,
    state: String,
    memory_mb: f64,
    stack_depth: usize,
    local_variables: usize,
}

impl MultiFormatDisplay for AgentDump {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.add_row(vec![
            Cell::new("Agent ID").fg(Color::Cyan),
            Cell::new(&self.agent_id),
        ]);
        table.add_row(vec![
            Cell::new("Timestamp").fg(Color::Cyan),
            Cell::new(&self.timestamp),
        ]);
        table.add_row(vec![
            Cell::new("State").fg(Color::Cyan),
            Cell::new(&self.state).fg(Color::Green),
        ]);
        table.add_row(vec![
            Cell::new("Memory").fg(Color::Cyan),
            Cell::new(format!("{:.2} MB", self.memory_mb)),
        ]);
        table.add_row(vec![
            Cell::new("Stack Depth").fg(Color::Cyan),
            Cell::new(self.stack_depth.to_string()),
        ]);
        table.add_row(vec![
            Cell::new("Local Variables").fg(Color::Cyan),
            Cell::new(self.local_variables.to_string()),
        ]);

        table
    }
}

pub async fn handle_debug_command(action: DebugCommands, format: OutputFormat) -> Result<()> {
    match action {
        DebugCommands::Attach { agent_id, port } => {
            let session = mock_debug_session(&agent_id, port);
            println!("{}", render_output(&session, format)?);
        }
        DebugCommands::Trace {
            agent_id,
            duration,
            output_file,
        } => {
            let output_path = output_file.unwrap_or_else(|| {
                format!("trace-{}-{}.json", agent_id, chrono::Utc::now().timestamp())
            });

            let trace = mock_trace_result(&agent_id, duration, &output_path);
            println!("{}", render_output(&trace, format)?);
        }
        DebugCommands::Dump {
            agent_id,
            memory,
            dump_format,
        } => {
            let dump = mock_agent_dump(&agent_id);

            if memory {
                let result = OperationResult {
                    success: true,
                    message: format!(
                        "Agent {} state dumped to agent-{}.{} (with memory snapshot)",
                        agent_id, agent_id, dump_format
                    ),
                    id: Some(agent_id),
                };
                println!("{}", render_output(&result, format)?);
            } else {
                println!("{}", render_output(&dump, format)?);
            }
        }
        DebugCommands::Profile { agent_id, cpu, mem } => {
            let profile_types = match (cpu, mem) {
                (true, true) => "CPU and memory".to_string(),
                (true, false) => "CPU".to_string(),
                (false, true) => "memory".to_string(),
                (false, false) => "CPU and memory".to_string(), // default to both
            };

            let result = OperationResult {
                success: true,
                message: format!(
                    "Profiling agent {} ({} profile available at /tmp/profile-{}.pb.gz)",
                    agent_id, profile_types, agent_id
                ),
                id: Some(agent_id),
            };
            println!("{}", render_output(&result, format)?);
        }
    }
    Ok(())
}

fn mock_debug_session(agent_id: &str, port: u16) -> DebugSession {
    DebugSession {
        agent_id: agent_id.to_string(),
        session_id: format!("debug-session-{}", uuid::Uuid::new_v4()),
        debugger_url: format!("ws://localhost:{}/debugger/{}", port, agent_id),
        status: "Attached".to_string(),
    }
}

fn mock_trace_result(agent_id: &str, duration: u64, output_file: &str) -> TraceResult {
    TraceResult {
        agent_id: agent_id.to_string(),
        duration_secs: duration,
        events_captured: duration * 1250, // Simulate ~1250 events/sec
        output_file: output_file.to_string(),
        file_size_kb: duration * 85, // Simulate ~85KB/sec
    }
}

fn mock_agent_dump(agent_id: &str) -> AgentDump {
    AgentDump {
        agent_id: agent_id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        state: "Running".to_string(),
        memory_mb: 12.5,
        stack_depth: 8,
        local_variables: 24,
    }
}
