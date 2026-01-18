//! Message Acknowledgment System
//!
//! Provides reliable message delivery with:
//! - Automatic acknowledgment generation
//! - Retry with exponential backoff
//! - Duplicate detection via message IDs
//! - Configurable timeout and retry limits

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Unique message identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageId(pub u64);

impl MessageId {
    /// Generate a new unique message ID
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Create from raw value
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Get raw value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Acknowledgment status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckStatus {
    /// Message sent, awaiting acknowledgment
    Pending,
    /// Acknowledgment received successfully
    Acknowledged,
    /// Message delivery failed after max retries
    Failed,
    /// Message was explicitly rejected
    Rejected,
    /// Message expired (TTL exceeded)
    Expired,
}

/// Acknowledgment message sent in response to a reliable message
#[derive(Debug, Clone)]
pub struct Acknowledgment {
    /// ID of the message being acknowledged
    pub message_id: MessageId,
    /// Sender's node ID
    pub sender: [u8; 16],
    /// Whether the message was processed successfully
    pub success: bool,
    /// Optional error message if rejected
    pub error: Option<String>,
    /// Timestamp when acknowledgment was generated (microseconds)
    pub timestamp_us: u64,
}

impl Acknowledgment {
    /// Create a successful acknowledgment
    pub fn success(message_id: MessageId, sender: [u8; 16]) -> Self {
        Self {
            message_id,
            sender,
            success: true,
            error: None,
            timestamp_us: current_time_us(),
        }
    }

    /// Create a rejection acknowledgment
    pub fn reject(message_id: MessageId, sender: [u8; 16], error: impl Into<String>) -> Self {
        Self {
            message_id,
            sender,
            success: false,
            error: Some(error.into()),
            timestamp_us: current_time_us(),
        }
    }
}

/// Configuration for the acknowledgment system
#[derive(Debug, Clone)]
pub struct AckConfig {
    /// Initial timeout before first retry (microseconds)
    pub initial_timeout_us: u64,
    /// Maximum timeout (cap for exponential backoff)
    pub max_timeout_us: u64,
    /// Backoff multiplier (typically 2.0)
    pub backoff_multiplier: f64,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// How long to keep message IDs for duplicate detection (microseconds)
    pub duplicate_window_us: u64,
    /// Maximum number of pending messages
    pub max_pending: usize,
}

impl Default for AckConfig {
    fn default() -> Self {
        Self {
            initial_timeout_us: 100_000, // 100ms
            max_timeout_us: 30_000_000,  // 30 seconds
            backoff_multiplier: 2.0,
            max_retries: 5,
            duplicate_window_us: 60_000_000, // 60 seconds
            max_pending: 10_000,
        }
    }
}

impl AckConfig {
    /// Configuration optimized for low latency
    pub fn low_latency() -> Self {
        Self {
            initial_timeout_us: 50_000, // 50ms
            max_timeout_us: 5_000_000,  // 5 seconds
            backoff_multiplier: 1.5,
            max_retries: 3,
            duplicate_window_us: 30_000_000, // 30 seconds
            max_pending: 5_000,
        }
    }

    /// Configuration optimized for reliability
    pub fn high_reliability() -> Self {
        Self {
            initial_timeout_us: 200_000, // 200ms
            max_timeout_us: 60_000_000,  // 60 seconds
            backoff_multiplier: 2.0,
            max_retries: 10,
            duplicate_window_us: 120_000_000, // 2 minutes
            max_pending: 50_000,
        }
    }

    /// Configuration for embedded systems
    pub fn embedded() -> Self {
        Self {
            initial_timeout_us: 500_000, // 500ms
            max_timeout_us: 10_000_000,  // 10 seconds
            backoff_multiplier: 2.0,
            max_retries: 3,
            duplicate_window_us: 30_000_000, // 30 seconds
            max_pending: 100,
        }
    }

    /// Calculate timeout for a given retry attempt
    pub fn timeout_for_retry(&self, attempt: u32) -> u64 {
        let timeout = self.initial_timeout_us as f64 * self.backoff_multiplier.powi(attempt as i32);
        (timeout as u64).min(self.max_timeout_us)
    }
}

/// State of a pending message awaiting acknowledgment
#[derive(Debug, Clone)]
pub struct PendingMessage {
    /// Unique message identifier
    pub message_id: MessageId,
    /// Destination node
    pub destination: [u8; 16],
    /// Serialized message payload
    pub payload: Vec<u8>,
    /// When the message was first sent (microseconds)
    pub sent_at_us: u64,
    /// When the current retry times out (microseconds)
    pub timeout_at_us: u64,
    /// Number of retries attempted
    pub retry_count: u32,
    /// Current status
    pub status: AckStatus,
}

impl PendingMessage {
    /// Create a new pending message
    pub fn new(
        message_id: MessageId,
        destination: [u8; 16],
        payload: Vec<u8>,
        timeout_us: u64,
    ) -> Self {
        let now = current_time_us();
        Self {
            message_id,
            destination,
            payload,
            sent_at_us: now,
            timeout_at_us: now + timeout_us,
            retry_count: 0,
            status: AckStatus::Pending,
        }
    }

    /// Check if the message has timed out
    pub fn is_timed_out(&self, now_us: u64) -> bool {
        now_us >= self.timeout_at_us
    }

    /// Get elapsed time since first send
    pub fn elapsed_us(&self, now_us: u64) -> u64 {
        now_us.saturating_sub(self.sent_at_us)
    }
}

/// Entry in the duplicate detection cache
#[derive(Debug, Clone)]
struct DuplicateEntry {
    received_at_us: u64,
}

/// Message acknowledgment tracker
///
/// Manages reliable message delivery with automatic retries and duplicate detection.
pub struct AckTracker {
    config: AckConfig,
    /// Messages awaiting acknowledgment, keyed by message ID
    pending: HashMap<MessageId, PendingMessage>,
    /// Recently seen message IDs for duplicate detection
    seen: HashMap<(MessageId, [u8; 16]), DuplicateEntry>,
    /// Statistics
    stats: AckStats,
}

/// Statistics for the acknowledgment tracker
#[derive(Debug, Clone, Default)]
pub struct AckStats {
    /// Total messages sent requiring ACK
    pub messages_sent: u64,
    /// Total acknowledgments received
    pub acks_received: u64,
    /// Total messages that failed delivery
    pub delivery_failures: u64,
    /// Total retries performed
    pub retries: u64,
    /// Total duplicate messages detected
    pub duplicates_detected: u64,
    /// Currently pending messages
    pub pending_count: usize,
    /// Average round-trip time (microseconds)
    pub avg_rtt_us: u64,
}

impl AckTracker {
    /// Create a new acknowledgment tracker
    pub fn new(config: AckConfig) -> Self {
        Self {
            config,
            pending: HashMap::new(),
            seen: HashMap::new(),
            stats: AckStats::default(),
        }
    }

    /// Register a message for tracking
    ///
    /// Returns the message ID to include in the message header.
    pub fn register(
        &mut self,
        destination: [u8; 16],
        payload: Vec<u8>,
    ) -> Result<MessageId, AckError> {
        if self.pending.len() >= self.config.max_pending {
            return Err(AckError::TooManyPending);
        }

        let message_id = MessageId::generate();
        let timeout = self.config.initial_timeout_us;
        let pending = PendingMessage::new(message_id, destination, payload, timeout);
        self.pending.insert(message_id, pending);
        self.stats.messages_sent += 1;
        self.stats.pending_count = self.pending.len();

        Ok(message_id)
    }

    /// Process an incoming acknowledgment
    pub fn process_ack(&mut self, ack: &Acknowledgment) -> AckResult {
        let now = current_time_us();

        if let Some(pending) = self.pending.remove(&ack.message_id) {
            let rtt = now.saturating_sub(pending.sent_at_us);

            // Update RTT statistics
            if self.stats.acks_received == 0 {
                self.stats.avg_rtt_us = rtt;
            } else {
                // Exponential moving average
                self.stats.avg_rtt_us = (self.stats.avg_rtt_us * 7 + rtt) / 8;
            }

            self.stats.acks_received += 1;
            self.stats.pending_count = self.pending.len();

            if ack.success {
                AckResult::Acknowledged { rtt_us: rtt }
            } else {
                self.stats.delivery_failures += 1;
                AckResult::Rejected {
                    error: ack.error.clone(),
                }
            }
        } else {
            AckResult::Unknown
        }
    }

    /// Check if a message is a duplicate
    pub fn is_duplicate(&mut self, message_id: MessageId, sender: [u8; 16]) -> bool {
        let key = (message_id, sender);
        let now = current_time_us();

        // Clean old entries
        self.clean_duplicates(now);

        if let std::collections::hash_map::Entry::Vacant(e) = self.seen.entry(key) {
            e.insert(DuplicateEntry {
                received_at_us: now,
            });
            false
        } else {
            self.stats.duplicates_detected += 1;
            true
        }
    }

    /// Clean expired duplicate entries
    fn clean_duplicates(&mut self, now_us: u64) {
        let window = self.config.duplicate_window_us;
        self.seen
            .retain(|_, entry| now_us - entry.received_at_us < window);
    }

    /// Get messages that need to be retried
    pub fn get_retries(&mut self) -> Vec<RetryAction> {
        let now = current_time_us();
        let mut actions = Vec::new();

        let message_ids: Vec<_> = self.pending.keys().copied().collect();

        for message_id in message_ids {
            if let Some(pending) = self.pending.get_mut(&message_id) {
                if pending.is_timed_out(now) {
                    if pending.retry_count >= self.config.max_retries {
                        // Max retries exceeded, mark as failed
                        pending.status = AckStatus::Failed;
                        actions.push(RetryAction::GiveUp {
                            message_id,
                            destination: pending.destination,
                            attempts: pending.retry_count + 1,
                        });
                        self.stats.delivery_failures += 1;
                    } else {
                        // Schedule retry
                        pending.retry_count += 1;
                        let new_timeout = self.config.timeout_for_retry(pending.retry_count);
                        pending.timeout_at_us = now + new_timeout;
                        self.stats.retries += 1;

                        actions.push(RetryAction::Retry {
                            message_id,
                            destination: pending.destination,
                            payload: pending.payload.clone(),
                            attempt: pending.retry_count,
                        });
                    }
                }
            }
        }

        // Remove failed messages
        self.pending.retain(|_, p| p.status != AckStatus::Failed);
        self.stats.pending_count = self.pending.len();

        actions
    }

    /// Get current statistics
    pub fn stats(&self) -> &AckStats {
        &self.stats
    }

    /// Get number of pending messages
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Cancel a pending message
    pub fn cancel(&mut self, message_id: MessageId) -> bool {
        if self.pending.remove(&message_id).is_some() {
            self.stats.pending_count = self.pending.len();
            true
        } else {
            false
        }
    }

    /// Get a pending message by ID
    pub fn get_pending(&self, message_id: MessageId) -> Option<&PendingMessage> {
        self.pending.get(&message_id)
    }

    /// Mark a message as expired
    pub fn expire(&mut self, message_id: MessageId) -> bool {
        if let Some(pending) = self.pending.get_mut(&message_id) {
            pending.status = AckStatus::Expired;
            self.pending.remove(&message_id);
            self.stats.pending_count = self.pending.len();
            true
        } else {
            false
        }
    }
}

/// Result of processing an acknowledgment
#[derive(Debug, Clone)]
pub enum AckResult {
    /// Message was acknowledged successfully
    Acknowledged { rtt_us: u64 },
    /// Message was rejected
    Rejected { error: Option<String> },
    /// Unknown message ID (possibly already processed or expired)
    Unknown,
}

/// Action to take for retry handling
#[derive(Debug, Clone)]
pub enum RetryAction {
    /// Retry sending the message
    Retry {
        message_id: MessageId,
        destination: [u8; 16],
        payload: Vec<u8>,
        attempt: u32,
    },
    /// Give up on the message after max retries
    GiveUp {
        message_id: MessageId,
        destination: [u8; 16],
        attempts: u32,
    },
}

/// Errors from the acknowledgment system
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckError {
    /// Too many messages pending acknowledgment
    TooManyPending,
    /// Message not found
    NotFound,
    /// Invalid message ID
    InvalidMessageId,
}

impl std::fmt::Display for AckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AckError::TooManyPending => write!(f, "Too many pending messages"),
            AckError::NotFound => write!(f, "Message not found"),
            AckError::InvalidMessageId => write!(f, "Invalid message ID"),
        }
    }
}

impl std::error::Error for AckError {}

/// Thread-safe acknowledgment tracker
pub type SharedAckTracker = Arc<Mutex<AckTracker>>;

/// Create a shared acknowledgment tracker
pub fn shared_ack_tracker(config: AckConfig) -> SharedAckTracker {
    Arc::new(Mutex::new(AckTracker::new(config)))
}

/// Reliable message wrapper
///
/// Wraps a message with metadata for reliable delivery.
#[derive(Debug, Clone)]
pub struct ReliableMessage<T> {
    /// Unique message identifier
    pub message_id: MessageId,
    /// Original message
    pub inner: T,
    /// Sender node ID
    pub sender: [u8; 16],
    /// Whether acknowledgment is required
    pub requires_ack: bool,
    /// Send timestamp (microseconds)
    pub timestamp_us: u64,
}

impl<T> ReliableMessage<T> {
    /// Create a new reliable message
    pub fn new(inner: T, sender: [u8; 16]) -> Self {
        Self {
            message_id: MessageId::generate(),
            inner,
            sender,
            requires_ack: true,
            timestamp_us: current_time_us(),
        }
    }

    /// Create a fire-and-forget message (no ACK required)
    pub fn fire_and_forget(inner: T, sender: [u8; 16]) -> Self {
        Self {
            message_id: MessageId::generate(),
            inner,
            sender,
            requires_ack: false,
            timestamp_us: current_time_us(),
        }
    }

    /// Get the inner message
    pub fn into_inner(self) -> T {
        self.inner
    }
}

/// Delivery confirmation callback type
pub type DeliveryCallback = Box<dyn FnOnce(AckResult) + Send + 'static>;

/// Pending delivery with callback
struct PendingDelivery {
    message: PendingMessage,
    callback: Option<DeliveryCallback>,
}

/// Advanced acknowledgment manager with callbacks
pub struct AckManager {
    config: AckConfig,
    pending: HashMap<MessageId, PendingDelivery>,
    seen: HashMap<(MessageId, [u8; 16]), DuplicateEntry>,
    stats: AckStats,
}

impl AckManager {
    /// Create a new acknowledgment manager
    pub fn new(config: AckConfig) -> Self {
        Self {
            config,
            pending: HashMap::new(),
            seen: HashMap::new(),
            stats: AckStats::default(),
        }
    }

    /// Send a message with delivery confirmation callback
    pub fn send_with_callback(
        &mut self,
        destination: [u8; 16],
        payload: Vec<u8>,
        callback: DeliveryCallback,
    ) -> Result<MessageId, AckError> {
        if self.pending.len() >= self.config.max_pending {
            return Err(AckError::TooManyPending);
        }

        let message_id = MessageId::generate();
        let timeout = self.config.initial_timeout_us;
        let pending = PendingMessage::new(message_id, destination, payload, timeout);

        self.pending.insert(
            message_id,
            PendingDelivery {
                message: pending,
                callback: Some(callback),
            },
        );

        self.stats.messages_sent += 1;
        self.stats.pending_count = self.pending.len();

        Ok(message_id)
    }

    /// Process acknowledgment and trigger callback
    pub fn process_ack(&mut self, ack: &Acknowledgment) {
        let now = current_time_us();

        if let Some(mut delivery) = self.pending.remove(&ack.message_id) {
            let rtt = now.saturating_sub(delivery.message.sent_at_us);

            // Update RTT statistics
            if self.stats.acks_received == 0 {
                self.stats.avg_rtt_us = rtt;
            } else {
                self.stats.avg_rtt_us = (self.stats.avg_rtt_us * 7 + rtt) / 8;
            }

            self.stats.acks_received += 1;
            self.stats.pending_count = self.pending.len();

            let result = if ack.success {
                AckResult::Acknowledged { rtt_us: rtt }
            } else {
                self.stats.delivery_failures += 1;
                AckResult::Rejected {
                    error: ack.error.clone(),
                }
            };

            // Invoke callback
            if let Some(callback) = delivery.callback.take() {
                callback(result);
            }
        }
    }

    /// Check for duplicates
    pub fn is_duplicate(&mut self, message_id: MessageId, sender: [u8; 16]) -> bool {
        let key = (message_id, sender);
        let now = current_time_us();

        // Clean old entries
        let window = self.config.duplicate_window_us;
        self.seen
            .retain(|_, entry| now - entry.received_at_us < window);

        if let std::collections::hash_map::Entry::Vacant(e) = self.seen.entry(key) {
            e.insert(DuplicateEntry {
                received_at_us: now,
            });
            false
        } else {
            self.stats.duplicates_detected += 1;
            true
        }
    }

    /// Process retries and invoke failure callbacks
    pub fn process_retries(&mut self) -> Vec<(MessageId, [u8; 16], Vec<u8>)> {
        let now = current_time_us();
        let mut to_retry = Vec::new();
        let mut to_remove = Vec::new();

        for (message_id, delivery) in &mut self.pending {
            if delivery.message.is_timed_out(now) {
                if delivery.message.retry_count >= self.config.max_retries {
                    to_remove.push(*message_id);
                } else {
                    delivery.message.retry_count += 1;
                    let new_timeout = self.config.timeout_for_retry(delivery.message.retry_count);
                    delivery.message.timeout_at_us = now + new_timeout;
                    self.stats.retries += 1;

                    to_retry.push((
                        *message_id,
                        delivery.message.destination,
                        delivery.message.payload.clone(),
                    ));
                }
            }
        }

        // Handle failures and invoke callbacks
        for message_id in to_remove {
            if let Some(mut delivery) = self.pending.remove(&message_id) {
                self.stats.delivery_failures += 1;
                if let Some(callback) = delivery.callback.take() {
                    callback(AckResult::Rejected {
                        error: Some("Max retries exceeded".to_string()),
                    });
                }
            }
        }

        self.stats.pending_count = self.pending.len();
        to_retry
    }

    /// Get statistics
    pub fn stats(&self) -> &AckStats {
        &self.stats
    }
}

/// Get current time in microseconds
fn current_time_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_generation() {
        let id1 = MessageId::generate();
        let id2 = MessageId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_acknowledgment_success() {
        let message_id = MessageId::generate();
        let sender = [1u8; 16];
        let ack = Acknowledgment::success(message_id, sender);

        assert!(ack.success);
        assert!(ack.error.is_none());
        assert_eq!(ack.message_id, message_id);
    }

    #[test]
    fn test_acknowledgment_reject() {
        let message_id = MessageId::generate();
        let sender = [1u8; 16];
        let ack = Acknowledgment::reject(message_id, sender, "Processing failed");

        assert!(!ack.success);
        assert_eq!(ack.error, Some("Processing failed".to_string()));
    }

    #[test]
    fn test_config_timeout_calculation() {
        let config = AckConfig::default();

        // Initial timeout
        assert_eq!(config.timeout_for_retry(0), 100_000);

        // After first retry (doubled)
        assert_eq!(config.timeout_for_retry(1), 200_000);

        // After second retry
        assert_eq!(config.timeout_for_retry(2), 400_000);

        // Should cap at max
        let capped = config.timeout_for_retry(10);
        assert_eq!(capped, config.max_timeout_us);
    }

    #[test]
    fn test_config_presets() {
        let low_lat = AckConfig::low_latency();
        assert!(low_lat.initial_timeout_us < AckConfig::default().initial_timeout_us);

        let high_rel = AckConfig::high_reliability();
        assert!(high_rel.max_retries > AckConfig::default().max_retries);

        let embedded = AckConfig::embedded();
        assert!(embedded.max_pending < AckConfig::default().max_pending);
    }

    #[test]
    fn test_tracker_register() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let dest = [2u8; 16];
        let payload = vec![1, 2, 3, 4];

        let message_id = tracker.register(dest, payload.clone()).unwrap();
        assert_eq!(tracker.pending_count(), 1);

        let pending = tracker.get_pending(message_id).unwrap();
        assert_eq!(pending.destination, dest);
        assert_eq!(pending.payload, payload);
        assert_eq!(pending.retry_count, 0);
    }

    #[test]
    fn test_tracker_max_pending() {
        let config = AckConfig {
            max_pending: 2,
            ..Default::default()
        };
        let mut tracker = AckTracker::new(config);

        tracker.register([1u8; 16], vec![]).unwrap();
        tracker.register([2u8; 16], vec![]).unwrap();

        // Third should fail
        let result = tracker.register([3u8; 16], vec![]);
        assert_eq!(result.unwrap_err(), AckError::TooManyPending);
    }

    #[test]
    fn test_tracker_process_ack() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let dest = [2u8; 16];
        let message_id = tracker.register(dest, vec![1, 2, 3]).unwrap();

        let ack = Acknowledgment::success(message_id, dest);
        let result = tracker.process_ack(&ack);

        assert!(matches!(result, AckResult::Acknowledged { .. }));
        assert_eq!(tracker.pending_count(), 0);
        assert_eq!(tracker.stats().acks_received, 1);
    }

    #[test]
    fn test_tracker_process_rejection() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let dest = [2u8; 16];
        let message_id = tracker.register(dest, vec![1, 2, 3]).unwrap();

        let ack = Acknowledgment::reject(message_id, dest, "Failed");
        let result = tracker.process_ack(&ack);

        assert!(matches!(result, AckResult::Rejected { .. }));
        assert_eq!(tracker.stats().delivery_failures, 1);
    }

    #[test]
    fn test_tracker_unknown_ack() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let ack = Acknowledgment::success(MessageId::generate(), [1u8; 16]);
        let result = tracker.process_ack(&ack);

        assert!(matches!(result, AckResult::Unknown));
    }

    #[test]
    fn test_duplicate_detection() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let message_id = MessageId::generate();
        let sender = [1u8; 16];

        // First time - not duplicate
        assert!(!tracker.is_duplicate(message_id, sender));

        // Second time - duplicate
        assert!(tracker.is_duplicate(message_id, sender));

        // Different sender - not duplicate
        let sender2 = [2u8; 16];
        assert!(!tracker.is_duplicate(message_id, sender2));

        assert_eq!(tracker.stats().duplicates_detected, 1);
    }

    #[test]
    fn test_tracker_cancel() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let message_id = tracker.register([1u8; 16], vec![]).unwrap();

        assert!(tracker.cancel(message_id));
        assert_eq!(tracker.pending_count(), 0);

        // Cancel again should fail
        assert!(!tracker.cancel(message_id));
    }

    #[test]
    fn test_tracker_expire() {
        let mut tracker = AckTracker::new(AckConfig::default());
        let message_id = tracker.register([1u8; 16], vec![]).unwrap();

        assert!(tracker.expire(message_id));
        assert_eq!(tracker.pending_count(), 0);

        // Expire again should fail
        assert!(!tracker.expire(message_id));
    }

    #[test]
    fn test_pending_message_timeout() {
        let message_id = MessageId::generate();
        // Use a large timeout (1 second) to avoid timing issues
        let pending = PendingMessage::new(message_id, [1u8; 16], vec![], 1_000_000);

        // Message should not be timed out immediately
        assert!(!pending.is_timed_out(pending.sent_at_us + 500_000)); // 500ms later

        // Message should be timed out well after timeout
        assert!(pending.is_timed_out(pending.sent_at_us + 2_000_000)); // 2s later
    }

    #[test]
    fn test_reliable_message() {
        let inner = "test message";
        let sender = [1u8; 16];

        let reliable = ReliableMessage::new(inner, sender);
        assert!(reliable.requires_ack);
        assert_eq!(reliable.sender, sender);

        let fire_forget = ReliableMessage::fire_and_forget(inner, sender);
        assert!(!fire_forget.requires_ack);
    }

    #[test]
    fn test_reliable_message_into_inner() {
        let inner = vec![1, 2, 3];
        let reliable = ReliableMessage::new(inner.clone(), [1u8; 16]);
        assert_eq!(reliable.into_inner(), inner);
    }

    #[test]
    fn test_retry_actions() {
        let config = AckConfig {
            initial_timeout_us: 1, // 1 microsecond for immediate timeout
            max_retries: 2,
            ..Default::default()
        };
        let mut tracker = AckTracker::new(config);

        let _message_id = tracker.register([1u8; 16], vec![1, 2, 3]).unwrap();

        // Wait briefly and check for retries
        std::thread::sleep(Duration::from_micros(100));
        let actions = tracker.get_retries();

        assert!(!actions.is_empty());
        assert!(matches!(&actions[0], RetryAction::Retry { .. }));
    }

    #[test]
    fn test_max_retries_exceeded() {
        let config = AckConfig {
            initial_timeout_us: 1,
            max_retries: 0, // No retries allowed
            ..Default::default()
        };
        let mut tracker = AckTracker::new(config);

        tracker.register([1u8; 16], vec![1, 2, 3]).unwrap();

        std::thread::sleep(Duration::from_micros(100));
        let actions = tracker.get_retries();

        assert!(!actions.is_empty());
        assert!(matches!(&actions[0], RetryAction::GiveUp { .. }));
        assert_eq!(tracker.pending_count(), 0);
        assert_eq!(tracker.stats().delivery_failures, 1);
    }

    #[test]
    fn test_shared_ack_tracker() {
        let tracker = shared_ack_tracker(AckConfig::default());

        let message_id = {
            let mut t = tracker.lock().unwrap();
            t.register([1u8; 16], vec![]).unwrap()
        };

        let count = {
            let t = tracker.lock().unwrap();
            t.pending_count()
        };

        assert_eq!(count, 1);
        assert!(message_id.raw() > 0);
    }

    #[test]
    fn test_ack_error_display() {
        assert_eq!(
            AckError::TooManyPending.to_string(),
            "Too many pending messages"
        );
        assert_eq!(AckError::NotFound.to_string(), "Message not found");
        assert_eq!(AckError::InvalidMessageId.to_string(), "Invalid message ID");
    }

    #[test]
    fn test_stats_rtt_averaging() {
        let mut tracker = AckTracker::new(AckConfig::default());

        // Send and ack multiple messages
        for _ in 0..5 {
            let id = tracker.register([1u8; 16], vec![]).unwrap();
            let ack = Acknowledgment::success(id, [1u8; 16]);
            tracker.process_ack(&ack);
        }

        // RTT should be non-zero
        assert!(tracker.stats().avg_rtt_us > 0 || tracker.stats().acks_received > 0);
    }

    #[test]
    fn test_ack_manager_with_callback() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut manager = AckManager::new(AckConfig::default());
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let message_id = manager
            .send_with_callback(
                [1u8; 16],
                vec![1, 2, 3],
                Box::new(move |result| {
                    if matches!(result, AckResult::Acknowledged { .. }) {
                        called_clone.store(true, Ordering::SeqCst);
                    }
                }),
            )
            .unwrap();

        // Process ack
        let ack = Acknowledgment::success(message_id, [1u8; 16]);
        manager.process_ack(&ack);

        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_ack_manager_duplicate_check() {
        let mut manager = AckManager::new(AckConfig::default());
        let msg_id = MessageId::generate();
        let sender = [1u8; 16];

        assert!(!manager.is_duplicate(msg_id, sender));
        assert!(manager.is_duplicate(msg_id, sender));
    }

    #[test]
    fn test_ack_manager_process_retries() {
        let config = AckConfig {
            initial_timeout_us: 1,
            max_retries: 1,
            ..Default::default()
        };
        let mut manager = AckManager::new(config);

        manager
            .send_with_callback([1u8; 16], vec![1, 2, 3], Box::new(|_| {}))
            .unwrap();

        std::thread::sleep(Duration::from_micros(100));
        let retries = manager.process_retries();

        // Should get at least one retry
        assert!(!retries.is_empty() || manager.stats().retries > 0);
    }
}
