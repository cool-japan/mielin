//! Progress indicators for long-running operations

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Simple spinner for progress indication
pub struct Spinner {
    message: String,
    running: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Spinner {
    /// Create a new spinner with a message
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Start the spinner
    pub fn start(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            return; // Already running
        }

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let message = self.message.clone();

        let handle = tokio::spawn(async move {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut frame_idx = 0;

            while running.load(Ordering::SeqCst) {
                eprint!("\r{} {}", frames[frame_idx], message);
                frame_idx = (frame_idx + 1) % frames.len();
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            eprint!("\r{}\r", " ".repeat(message.len() + 10)); // Clear the line
        });

        self.handle = Some(handle);
    }

    /// Stop the spinner
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            // Wait a bit for the spinner to clean up
            std::thread::sleep(Duration::from_millis(100));
            handle.abort();
        }
    }

    /// Stop the spinner with a completion message
    pub fn finish_with_message(&mut self, message: &str) {
        self.stop();
        eprintln!("✓ {}", message);
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Progress bar for operations with known total
pub struct ProgressBar {
    message: String,
    total: u64,
    current: u64,
    width: usize,
}

impl ProgressBar {
    /// Create a new progress bar
    pub fn new(message: impl Into<String>, total: u64) -> Self {
        Self {
            message: message.into(),
            total,
            current: 0,
            width: 40,
        }
    }

    /// Update progress
    pub fn set_progress(&mut self, current: u64) {
        self.current = current.min(self.total);
        self.render();
    }

    /// Increment progress by 1
    pub fn inc(&mut self) {
        self.set_progress(self.current + 1);
    }

    /// Increment progress by n
    pub fn inc_by(&mut self, n: u64) {
        self.set_progress(self.current + n);
    }

    /// Render the progress bar
    fn render(&self) {
        let percentage = if self.total > 0 {
            (self.current as f64 / self.total as f64 * 100.0) as u64
        } else {
            0
        };

        let filled = if self.total > 0 {
            (self.current as f64 / self.total as f64 * self.width as f64) as usize
        } else {
            0
        };

        let empty = self.width.saturating_sub(filled);

        eprint!(
            "\r{} [{}{}] {}/{}  ({}%)",
            self.message,
            "█".repeat(filled),
            "░".repeat(empty),
            self.current,
            self.total,
            percentage
        );

        if self.current >= self.total {
            eprintln!(); // New line when complete
        }
    }

    /// Finish the progress bar
    pub fn finish(&mut self) {
        self.set_progress(self.total);
    }

    /// Finish with a custom message
    pub fn finish_with_message(&mut self, message: &str) {
        self.finish();
        eprintln!("✓ {}", message);
    }
}

/// Execute a future with a spinner
pub async fn with_spinner<F, T>(message: impl Into<String>, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let mut spinner = Spinner::new(message);
    spinner.start();
    let result = future.await;
    spinner.stop();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spinner_creation() {
        let spinner = Spinner::new("Testing");
        assert!(!spinner.running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_progress_bar() {
        let mut bar = ProgressBar::new("Testing", 100);
        assert_eq!(bar.current, 0);
        assert_eq!(bar.total, 100);

        bar.set_progress(50);
        assert_eq!(bar.current, 50);

        bar.inc();
        assert_eq!(bar.current, 51);

        bar.inc_by(10);
        assert_eq!(bar.current, 61);

        bar.finish();
        assert_eq!(bar.current, 100);
    }

    #[tokio::test]
    async fn test_with_spinner() {
        let result = with_spinner("Processing", async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            42
        })
        .await;

        assert_eq!(result, 42);
    }
}
