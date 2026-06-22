//! MielinCTL - Command Line Interface
//!
//! Control and management tool for MielinOS.

use clap::Parser;
use mielin_cli::cli::{dispatch, Cli};
use mielin_cli::output::OutputFormat;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let format = if cli.quiet {
        OutputFormat::Quiet
    } else {
        cli.output
    };

    if let Err(e) = dispatch(cli.command, format).await {
        eprintln!("Error: {}", mielin_cli::format_error(&e));
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use mielin_cli::cli::Cli;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
