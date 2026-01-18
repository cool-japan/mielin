//! Error types and user-friendly error messages for MielinCTL

use std::fmt;

/// CLI-specific error types with user-friendly messages
#[derive(Debug)]
pub enum CliError {
    /// Configuration error
    Config(String),
    /// Connection error
    Connection(String),
    /// File operation error
    FileOperation(String),
    /// Invalid input
    InvalidInput(String),
    /// Not found error
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// Timeout error
    Timeout(String),
    /// Internal error
    Internal(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Config(msg) => write!(f, "Configuration error: {}", msg),
            CliError::Connection(msg) => write!(f, "Connection error: {}", msg),
            CliError::FileOperation(msg) => write!(f, "File operation error: {}", msg),
            CliError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            CliError::NotFound(msg) => write!(f, "Not found: {}", msg),
            CliError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            CliError::Timeout(msg) => write!(f, "Operation timed out: {}", msg),
            CliError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for CliError {}

/// Convert anyhow::Error to user-friendly message
pub fn format_error(error: &anyhow::Error) -> String {
    let error_str = error.to_string();

    if error_str.contains("timed out") || error_str.contains("timeout") {
        "Operation timed out. Try increasing the timeout with: \
         mielinctl node config cli.command_timeout_secs <seconds>"
            .to_string()
    } else if error_str.contains("Connection refused") || error_str.contains("connect") {
        "Could not connect to the MielinOS daemon. \
         Make sure it's running with: mielinctl daemon"
            .to_string()
    } else if error_str.contains("No such file") || error_str.contains("not found") {
        format!("File not found: {}", error_str)
    } else if error_str.contains("Permission denied") {
        "Permission denied. You may need elevated privileges to perform this operation.".to_string()
    } else if error_str.contains("config") || error_str.contains("Config") {
        format!(
            "Configuration error: {}. \
             Check your config with: mielinctl node config --all",
            error_str
        )
    } else {
        error_str
    }
}

/// Error context extension trait for adding user-friendly context
pub trait ErrorContext<T> {
    fn context_user_friendly(self, msg: &str) -> anyhow::Result<T>;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context_user_friendly(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", msg, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_error_display() {
        let error = CliError::Config("invalid format".to_string());
        assert_eq!(error.to_string(), "Configuration error: invalid format");

        let error = CliError::Connection("refused".to_string());
        assert_eq!(error.to_string(), "Connection error: refused");

        let error = CliError::Timeout("30s".to_string());
        assert_eq!(error.to_string(), "Operation timed out: 30s");
    }

    #[test]
    fn test_format_error_timeout() {
        let error = anyhow::anyhow!("Operation timed out");
        let formatted = format_error(&error);
        assert!(formatted.contains("timed out"));
        assert!(formatted.contains("command_timeout_secs"));
    }

    #[test]
    fn test_format_error_connection() {
        let error = anyhow::anyhow!("Connection refused");
        let formatted = format_error(&error);
        assert!(formatted.contains("connect"));
        assert!(formatted.contains("daemon"));
    }

    #[test]
    fn test_format_error_not_found() {
        let error = anyhow::anyhow!("No such file or directory");
        let formatted = format_error(&error);
        assert!(formatted.contains("File not found"));
    }
}
