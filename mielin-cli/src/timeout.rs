//! Timeout utilities for command execution

use anyhow::{Context, Result};
use std::future::Future;
use std::time::Duration;

/// Execute a future with a timeout
pub async fn with_timeout<F, T>(future: F, timeout_secs: u64) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    let timeout_duration = Duration::from_secs(timeout_secs);

    tokio::time::timeout(timeout_duration, future)
        .await
        .context("Operation timed out")?
}

/// Execute a future with a configurable timeout from config
pub async fn with_config_timeout<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    let config = crate::config::Config::load()?;
    with_timeout(future, config.cli.command_timeout_secs).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_timeout_succeeds() {
        let result = with_timeout(
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(42)
            },
            1,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_timeout_fails() {
        let result = with_timeout(
            async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok::<_, anyhow::Error>(42)
            },
            1,
        )
        .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("timed out"));
    }
}
