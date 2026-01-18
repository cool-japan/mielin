//! # LoggingCallback - Trait Implementations
//!
//! This module contains trait implementations for `LoggingCallback`.
//!
//! ## Implemented Traits
//!
//! - `MigrationProgressCallback`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::functions::MigrationProgressCallback;
use super::types::{LoggingCallback, MigrationProgressEvent};

impl MigrationProgressCallback for LoggingCallback {
    fn on_progress(&mut self, event: MigrationProgressEvent) {
        match event {
            MigrationProgressEvent::Started {
                agent_id,
                total_bytes,
            } => {
                println!(
                    "Migration started for agent {:?}, {} bytes to transfer",
                    agent_id, total_bytes
                );
            }
            MigrationProgressEvent::CapturingState { agent_id, percent } => {
                if self.verbose {
                    println!("Capturing state for agent {:?}: {}%", agent_id, percent);
                }
            }
            MigrationProgressEvent::TransferProgress {
                agent_id,
                transferred_bytes,
                total_bytes,
                percent,
            } => {
                if self.verbose || percent % 10 == 0 {
                    println!(
                        "Transfer progress for agent {:?}: {}/{} bytes ({}%)",
                        agent_id, transferred_bytes, total_bytes, percent
                    );
                }
            }
            MigrationProgressEvent::Validating { agent_id, step } => {
                if self.verbose {
                    println!("Validating agent {:?}: {}", agent_id, step);
                }
            }
            MigrationProgressEvent::Restoring { agent_id, percent } => {
                if self.verbose {
                    println!("Restoring agent {:?}: {}%", agent_id, percent);
                }
            }
            MigrationProgressEvent::Completed {
                agent_id,
                duration_ms,
                bytes_transferred,
            } => {
                println!(
                    "Migration completed for agent {:?} in {}ms ({} bytes transferred)",
                    agent_id, duration_ms, bytes_transferred
                );
            }
            MigrationProgressEvent::Failed { agent_id, error } => {
                eprintln!("Migration failed for agent {:?}: {}", agent_id, error);
            }
        }
    }
}
