//! Message Batching for Efficient Transmission
//!
//! Batches multiple messages together to reduce network overhead and improve throughput.
//! Supports:
//! - Time-based batching (max wait time)
//! - Size-based batching (max batch size)
//! - Priority-aware batching
//! - Automatic flushing

use crate::Message;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::interval;
use tracing::debug;

/// Batching errors
#[derive(Debug, Error)]
pub enum BatchError {
    #[error("Batch is full: {current_size} bytes (max: {max_size} bytes)")]
    BatchFull {
        current_size: usize,
        max_size: usize,
    },

    #[error("Message too large for batching: {message_size} bytes (max: {max_size} bytes)")]
    MessageTooLarge {
        message_size: usize,
        max_size: usize,
    },

    #[error("Batch channel closed")]
    ChannelClosed,

    #[error("Batch send failed: {0}")]
    SendFailed(String),
}

/// Batch configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of messages per batch
    pub max_messages: usize,

    /// Maximum total size of batch in bytes
    pub max_size_bytes: usize,

    /// Maximum wait time before flushing partial batch
    pub max_wait_time: Duration,

    /// Whether to batch critical messages
    pub batch_critical: bool,

    /// Flush batch when this percentage full
    pub flush_threshold: f32,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_messages: 100,
            max_size_bytes: 1_000_000, // 1MB
            max_wait_time: Duration::from_millis(10),
            batch_critical: false,
            flush_threshold: 0.8,
        }
    }
}

impl BatchConfig {
    /// High throughput preset
    pub fn high_throughput() -> Self {
        Self {
            max_messages: 500,
            max_size_bytes: 5_000_000, // 5MB
            max_wait_time: Duration::from_millis(50),
            batch_critical: true,
            flush_threshold: 0.9,
        }
    }

    /// Low latency preset
    pub fn low_latency() -> Self {
        Self {
            max_messages: 20,
            max_size_bytes: 100_000, // 100KB
            max_wait_time: Duration::from_millis(1),
            batch_critical: false,
            flush_threshold: 0.5,
        }
    }

    /// Embedded/constrained preset
    pub fn embedded() -> Self {
        Self {
            max_messages: 10,
            max_size_bytes: 10_000, // 10KB
            max_wait_time: Duration::from_millis(5),
            batch_critical: false,
            flush_threshold: 0.7,
        }
    }
}

/// A batch of messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatch {
    /// Messages in this batch
    pub messages: Vec<Message>,

    /// Batch creation timestamp (milliseconds since epoch)
    pub created_at_ms: u64,

    /// Total size in bytes
    pub total_size_bytes: usize,

    /// Batch sequence number
    pub sequence: u64,
}

impl MessageBatch {
    /// Create a new empty batch
    pub fn new(sequence: u64) -> Self {
        Self {
            messages: Vec::new(),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            total_size_bytes: 0,
            sequence,
        }
    }

    /// Add a message to the batch
    pub fn add_message(&mut self, message: Message, estimated_size: usize) {
        self.messages.push(message);
        self.total_size_bytes += estimated_size;
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get age of the batch
    pub fn age(&self) -> Duration {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Duration::from_millis(now_ms.saturating_sub(self.created_at_ms))
    }
}

/// Message batcher
pub struct MessageBatcher {
    config: BatchConfig,
    current_batch: Arc<Mutex<MessageBatch>>,
    sequence_counter: Arc<RwLock<u64>>,
    flush_tx: mpsc::UnboundedSender<MessageBatch>,
    stats: Arc<RwLock<BatchStats>>,
}

impl MessageBatcher {
    /// Create a new message batcher
    pub fn new(config: BatchConfig) -> (Self, mpsc::UnboundedReceiver<MessageBatch>) {
        let (flush_tx, flush_rx) = mpsc::unbounded_channel();

        let batcher = Self {
            config,
            current_batch: Arc::new(Mutex::new(MessageBatch::new(0))),
            sequence_counter: Arc::new(RwLock::new(0)),
            flush_tx,
            stats: Arc::new(RwLock::new(BatchStats::default())),
        };

        (batcher, flush_rx)
    }

    /// Start automatic flushing based on timer
    pub fn start_auto_flush(&self) {
        let current_batch = self.current_batch.clone();
        let flush_tx = self.flush_tx.clone();
        let max_wait = self.config.max_wait_time;
        let stats = self.stats.clone();

        tokio::spawn(async move {
            let mut ticker = interval(max_wait);
            loop {
                ticker.tick().await;

                let mut batch = current_batch.lock().await;
                if !batch.is_empty() {
                    let next_seq = batch.sequence + 1;
                    let flushed_batch = std::mem::replace(&mut *batch, MessageBatch::new(next_seq));

                    if flush_tx.send(flushed_batch.clone()).is_ok() {
                        let mut stats_lock = stats.write().await;
                        stats_lock.batches_flushed += 1;
                        stats_lock.messages_sent += flushed_batch.message_count();
                        stats_lock.bytes_sent += flushed_batch.total_size_bytes;
                        debug!(
                            "Auto-flushed batch {} with {} messages ({} bytes)",
                            flushed_batch.sequence,
                            flushed_batch.message_count(),
                            flushed_batch.total_size_bytes
                        );
                    }
                }
            }
        });
    }

    /// Add a message to the batch
    pub async fn add_message(&self, message: Message) -> Result<(), BatchError> {
        // Estimate message size
        let estimated_size = Self::estimate_message_size(&message);

        // Check if message is too large
        if estimated_size > self.config.max_size_bytes {
            return Err(BatchError::MessageTooLarge {
                message_size: estimated_size,
                max_size: self.config.max_size_bytes,
            });
        }

        // Check if we should batch critical messages
        if message.is_critical() && !self.config.batch_critical {
            // Send immediately without batching
            let mut seq = self.sequence_counter.write().await;
            *seq += 1;
            let mut batch = MessageBatch::new(*seq);
            batch.add_message(message, estimated_size);

            self.flush_tx
                .send(batch)
                .map_err(|_| BatchError::ChannelClosed)?;

            let mut stats = self.stats.write().await;
            stats.messages_sent_unbatched += 1;

            return Ok(());
        }

        let mut batch = self.current_batch.lock().await;

        // Check if adding this message would exceed limits
        let would_exceed_size =
            batch.total_size_bytes + estimated_size > self.config.max_size_bytes;
        let would_exceed_count = batch.message_count() >= self.config.max_messages;

        if would_exceed_size || would_exceed_count {
            // Flush current batch
            if !batch.is_empty() {
                let next_seq = batch.sequence + 1;
                let flushed_batch = std::mem::replace(&mut *batch, MessageBatch::new(next_seq));

                self.flush_tx
                    .send(flushed_batch.clone())
                    .map_err(|_| BatchError::ChannelClosed)?;

                let mut stats = self.stats.write().await;
                stats.batches_flushed += 1;
                stats.messages_sent += flushed_batch.message_count();
                stats.bytes_sent += flushed_batch.total_size_bytes;

                debug!(
                    "Flushed batch {} (size limit) with {} messages ({} bytes)",
                    flushed_batch.sequence,
                    flushed_batch.message_count(),
                    flushed_batch.total_size_bytes
                );
            }
        }

        // Add message to (possibly new) batch
        batch.add_message(message, estimated_size);

        // Check flush threshold
        let fill_ratio = batch.total_size_bytes as f32 / self.config.max_size_bytes as f32;
        if fill_ratio >= self.config.flush_threshold {
            let next_seq = batch.sequence + 1;
            let flushed_batch = std::mem::replace(&mut *batch, MessageBatch::new(next_seq));

            self.flush_tx
                .send(flushed_batch.clone())
                .map_err(|_| BatchError::ChannelClosed)?;

            let mut stats = self.stats.write().await;
            stats.batches_flushed += 1;
            stats.messages_sent += flushed_batch.message_count();
            stats.bytes_sent += flushed_batch.total_size_bytes;

            debug!(
                "Flushed batch {} (threshold) with {} messages ({} bytes)",
                flushed_batch.sequence,
                flushed_batch.message_count(),
                flushed_batch.total_size_bytes
            );
        }

        Ok(())
    }

    /// Flush current batch immediately
    pub async fn flush(&self) -> Result<(), BatchError> {
        let mut batch = self.current_batch.lock().await;

        if !batch.is_empty() {
            let next_seq = batch.sequence + 1;
            let flushed_batch = std::mem::replace(&mut *batch, MessageBatch::new(next_seq));

            self.flush_tx
                .send(flushed_batch.clone())
                .map_err(|_| BatchError::ChannelClosed)?;

            let mut stats = self.stats.write().await;
            stats.batches_flushed += 1;
            stats.messages_sent += flushed_batch.message_count();
            stats.bytes_sent += flushed_batch.total_size_bytes;

            debug!(
                "Manually flushed batch {} with {} messages ({} bytes)",
                flushed_batch.sequence,
                flushed_batch.message_count(),
                flushed_batch.total_size_bytes
            );
        }

        Ok(())
    }

    /// Get batching statistics
    pub async fn get_stats(&self) -> BatchStats {
        self.stats.read().await.clone()
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = BatchStats::default();
    }

    /// Estimate message size in bytes
    fn estimate_message_size(message: &Message) -> usize {
        // This is a rough estimate - in production, you'd serialize and measure
        match message {
            Message::Ping { .. } => 16,
            Message::Pong { .. } => 20,
            Message::AgentMigration { snapshot, .. } => 32 + snapshot.len(),
            Message::MigrationAck { .. } => 64,
            Message::Discovery { capabilities, .. } => {
                32 + capabilities.iter().map(|s| s.len()).sum::<usize>()
            }
            Message::DiscoveryResponse { peers, .. } => 32 + peers.len() * 64,
            Message::LoadInfo { .. } => 32,
            Message::AgentQuery { .. } => 16,
            Message::RoutedMessage { payload, .. } => 64 + Self::estimate_message_size(payload),
            Message::VersionNegotiationRequest { .. } => 256,
            Message::VersionNegotiationResponse { .. } => 256,
            Message::ProtocolUpgrade { .. } => 128,
            Message::ProtocolUpgradeResponse { .. } => 128,
            Message::WebSocketUpgrade { .. } => 128,
            Message::WebSocketUpgradeResponse { .. } => 128,
        }
    }
}

/// Batching statistics
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    /// Total batches flushed
    pub batches_flushed: u64,

    /// Total messages sent in batches
    pub messages_sent: usize,

    /// Total messages sent without batching
    pub messages_sent_unbatched: usize,

    /// Total bytes sent
    pub bytes_sent: usize,
}

impl BatchStats {
    /// Get average batch size
    pub fn average_batch_size(&self) -> f32 {
        if self.batches_flushed == 0 {
            0.0
        } else {
            self.messages_sent as f32 / self.batches_flushed as f32
        }
    }

    /// Get average batch bytes
    pub fn average_batch_bytes(&self) -> f32 {
        if self.batches_flushed == 0 {
            0.0
        } else {
            self.bytes_sent as f32 / self.batches_flushed as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_config_presets() {
        let high_throughput = BatchConfig::high_throughput();
        assert_eq!(high_throughput.max_messages, 500);

        let low_latency = BatchConfig::low_latency();
        assert_eq!(low_latency.max_messages, 20);

        let embedded = BatchConfig::embedded();
        assert_eq!(embedded.max_messages, 10);
    }

    #[test]
    fn test_message_batch_creation() {
        let batch = MessageBatch::new(1);
        assert!(batch.is_empty());
        assert_eq!(batch.message_count(), 0);
        assert_eq!(batch.sequence, 1);
    }

    #[test]
    fn test_message_batch_add() {
        let mut batch = MessageBatch::new(1);
        let msg = Message::Ping { timestamp: 12345 };

        batch.add_message(msg, 100);
        assert_eq!(batch.message_count(), 1);
        assert_eq!(batch.total_size_bytes, 100);
    }

    #[tokio::test]
    async fn test_batcher_creation() {
        let config = BatchConfig::default();
        let (batcher, _rx) = MessageBatcher::new(config);

        let stats = batcher.get_stats().await;
        assert_eq!(stats.batches_flushed, 0);
    }

    #[tokio::test]
    async fn test_batcher_add_message() {
        let config = BatchConfig {
            max_messages: 10,
            max_size_bytes: 1000,
            max_wait_time: Duration::from_secs(10),
            batch_critical: false,
            flush_threshold: 0.9,
        };

        let (batcher, mut rx) = MessageBatcher::new(config);

        let msg = Message::Ping { timestamp: 12345 };
        batcher.add_message(msg).await.unwrap();

        // Manually flush
        batcher.flush().await.unwrap();

        // Receive batch
        let batch = rx.recv().await.unwrap();
        assert_eq!(batch.message_count(), 1);
    }

    #[tokio::test]
    async fn test_batcher_auto_flush_on_size() {
        let config = BatchConfig {
            max_messages: 2,
            max_size_bytes: 1000,
            max_wait_time: Duration::from_secs(10),
            batch_critical: false,
            flush_threshold: 0.9,
        };

        let (batcher, mut rx) = MessageBatcher::new(config);

        // Add messages to trigger flush
        batcher
            .add_message(Message::Ping { timestamp: 1 })
            .await
            .unwrap();
        batcher
            .add_message(Message::Ping { timestamp: 2 })
            .await
            .unwrap();
        batcher
            .add_message(Message::Ping { timestamp: 3 })
            .await
            .unwrap();

        // Should have flushed first batch
        let batch = rx.recv().await.unwrap();
        assert_eq!(batch.message_count(), 2);
    }

    #[tokio::test]
    async fn test_batcher_critical_messages_unbatched() {
        let config = BatchConfig {
            max_messages: 10,
            max_size_bytes: 1000,
            max_wait_time: Duration::from_secs(10),
            batch_critical: false,
            flush_threshold: 0.9,
        };

        let (batcher, mut rx) = MessageBatcher::new(config);

        let critical_msg = Message::AgentMigration {
            agent_id: [1u8; 16],
            snapshot: vec![],
            priority: 10,
        };

        batcher.add_message(critical_msg).await.unwrap();

        // Should receive immediately
        let batch = rx.recv().await.unwrap();
        assert_eq!(batch.message_count(), 1);

        let stats = batcher.get_stats().await;
        assert_eq!(stats.messages_sent_unbatched, 1);
    }

    #[tokio::test]
    async fn test_batcher_stats() {
        let config = BatchConfig::default();
        let (batcher, _rx) = MessageBatcher::new(config);

        batcher
            .add_message(Message::Ping { timestamp: 1 })
            .await
            .unwrap();
        batcher.flush().await.unwrap();

        let stats = batcher.get_stats().await;
        assert_eq!(stats.batches_flushed, 1);
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.average_batch_size(), 1.0);
    }
}
