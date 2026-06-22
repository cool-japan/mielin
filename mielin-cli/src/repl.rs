//! Interactive REPL (Read-Eval-Print Loop) for MielinCTL

use crate::history::History;
use anyhow::Result;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Editor, Helper};
use std::borrow::Cow;
use std::time::Instant;

/// REPL helper for command completion and highlighting
struct MielinHelper {
    commands: Vec<String>,
}

impl Helper for MielinHelper {}

impl MielinHelper {
    fn new() -> Self {
        Self {
            commands: vec![
                "node".to_string(),
                "node list".to_string(),
                "node info".to_string(),
                "node start".to_string(),
                "node stop".to_string(),
                "node create".to_string(),
                "node join".to_string(),
                "node leave".to_string(),
                "node config".to_string(),
                "agent".to_string(),
                "agent list".to_string(),
                "agent deploy".to_string(),
                "agent migrate".to_string(),
                "agent stop".to_string(),
                "agent create".to_string(),
                "agent inspect".to_string(),
                "agent logs".to_string(),
                "agent exec".to_string(),
                "mesh".to_string(),
                "mesh status".to_string(),
                "mesh peers".to_string(),
                "cluster".to_string(),
                "cluster init".to_string(),
                "cluster status".to_string(),
                "cluster health".to_string(),
                "cluster upgrade".to_string(),
                "migrate".to_string(),
                "migrate status".to_string(),
                "migrate cancel".to_string(),
                "migrate history".to_string(),
                "registry".to_string(),
                "registry list".to_string(),
                "registry query".to_string(),
                "registry stats".to_string(),
                "gossip".to_string(),
                "gossip status".to_string(),
                "gossip members".to_string(),
                "gossip sync".to_string(),
                "wasm".to_string(),
                "wasm build".to_string(),
                "wasm validate".to_string(),
                "wasm test".to_string(),
                "wasm optimize".to_string(),
                "debug".to_string(),
                "debug attach".to_string(),
                "debug trace".to_string(),
                "debug dump".to_string(),
                "debug profile".to_string(),
                "history".to_string(),
                "history show".to_string(),
                "history search".to_string(),
                "history stats".to_string(),
                "history clear".to_string(),
                "history export".to_string(),
                "version".to_string(),
                "help".to_string(),
                "exit".to_string(),
                "quit".to_string(),
            ],
        }
    }
}

impl Completer for MielinHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let mut matches = Vec::new();

        for cmd in &self.commands {
            if cmd.starts_with(&line[..pos]) {
                matches.push(Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                });
            }
        }

        Ok((0, matches))
    }
}

impl Hinter for MielinHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        for cmd in &self.commands {
            if cmd.starts_with(line) && cmd != line {
                return Some(cmd[line.len()..].to_string());
            }
        }

        None
    }
}

impl Highlighter for MielinHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // Simple highlighting: no color changes for now
        // Could be extended with ANSI colors in the future
        Cow::Borrowed(line)
    }
}

impl Validator for MielinHelper {}

/// Interactive REPL for MielinCTL
pub struct Repl {
    editor: Editor<MielinHelper, DefaultHistory>,
    history: History,
}

impl Repl {
    /// Create a new REPL instance
    pub fn new() -> Result<Self> {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .build();

        let helper = MielinHelper::new();
        let mut editor = Editor::with_config(config)?;
        editor.set_helper(Some(helper));

        // Load command history
        let history = History::load()?;

        Ok(Self { editor, history })
    }

    /// Run the REPL
    pub async fn run(&mut self) -> Result<()> {
        println!("MielinOS CLI - Interactive Mode");
        println!("Type 'help' for commands, 'exit' or 'quit' to exit");
        println!();

        loop {
            let readline = self.editor.readline("mielin> ");

            match readline {
                Ok(line) => {
                    let line = line.trim();

                    if line.is_empty() {
                        continue;
                    }

                    // Add to readline history
                    let _ = self.editor.add_history_entry(line);

                    // Handle exit commands
                    if line == "exit" || line == "quit" {
                        println!("Goodbye!");
                        break;
                    }

                    // Handle help command
                    if line == "help" {
                        self.show_help();
                        continue;
                    }

                    // Execute command
                    let start = Instant::now();
                    let result = self.execute_command(line).await;
                    let duration = start.elapsed();

                    let exit_code = if result.is_ok() { 0 } else { 1 };

                    // Add to command history
                    self.history
                        .add(line.to_string(), exit_code, duration.as_millis() as u64);

                    if let Err(e) = result {
                        eprintln!("Error: {}", crate::format_error(&e));
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("Interrupted (Ctrl+C). Type 'exit' or 'quit' to exit.");
                }
                Err(ReadlineError::Eof) => {
                    println!("EOF (Ctrl+D). Exiting...");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }

        // Save command history
        self.history.save()?;

        Ok(())
    }

    /// Execute a command
    async fn execute_command(&self, line: &str) -> Result<()> {
        use clap::Parser;
        let args = std::iter::once("mielinctl").chain(line.split_whitespace());
        let cli = match crate::cli::Cli::try_parse_from(args) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                return Ok(());
            }
        };
        match &cli.command {
            crate::cli::Commands::Interactive => {
                println!("'interactive' is not available within an interactive session");
                return Ok(());
            }
            crate::cli::Commands::Completion { .. } => {
                println!("'completion' is not available in REPL mode");
                return Ok(());
            }
            crate::cli::Commands::Daemon { .. } => {
                println!("'daemon' is not available in REPL mode");
                return Ok(());
            }
            _ => {}
        }
        let format = cli.output;
        crate::cli::dispatch(cli.command, format).await
    }

    /// Show help message
    fn show_help(&self) {
        println!("Available commands:");
        println!();
        println!("  Node Management:");
        println!("    node list                 - List all nodes");
        println!("    node info <id>            - Show node details");
        println!("    node start <id>           - Start a node");
        println!("    node stop <id>            - Stop a node");
        println!();
        println!("  Agent Management:");
        println!("    agent list                - List agents");
        println!("    agent deploy <wasm>       - Deploy WASM agent");
        println!("    agent migrate <id> <node> - Migrate agent");
        println!("    agent stop <id>           - Stop agent");
        println!();
        println!("  Mesh Network:");
        println!("    mesh status               - Mesh status");
        println!("    mesh peers                - List peers");
        println!();
        println!("  Cluster:");
        println!("    cluster init              - Initialize cluster");
        println!("    cluster status            - Cluster status");
        println!("    cluster health            - Health check");
        println!();
        println!("  Other:");
        println!("    history show              - Show command history");
        println!("    history stats             - History statistics");
        println!("    version                   - Show version");
        println!("    help                      - Show this help");
        println!("    exit, quit                - Exit interactive mode");
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repl() -> Repl {
        Repl::new().expect("failed to create test repl")
    }

    #[tokio::test]
    async fn test_repl_unknown_command_returns_ok() {
        let repl = make_repl();
        let result = repl.execute_command("not-a-command").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_repl_interactive_guard() {
        let repl = make_repl();
        let result = repl.execute_command("interactive").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_repl_completion_guard() {
        let repl = make_repl();
        let result = repl.execute_command("completion bash").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_repl_version_returns_ok() {
        let repl = make_repl();
        let result = repl.execute_command("version").await;
        assert!(result.is_ok());
    }
}
