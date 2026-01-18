//! Message Priority Queue
//!
//! Provides priority-based message queuing for the wire protocol.
//! Critical messages (like agent migrations) are prioritized over
//! routine traffic (like heartbeats).
//!
//! ## Priority Levels
//!
//! - **Critical (0)**: Agent migrations, error notifications
//! - **High (1)**: Discovery, registry updates
//! - **Normal (2)**: Load info, queries
//! - **Low (3)**: Ping/pong, background sync
//!
//! ## Features
//!
//! - O(log n) insertion and removal
//! - Fair scheduling within priority levels
//! - Capacity limits per priority
//! - Queue statistics

use crate::{Message, WireError};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Message priority levels (lower number = higher priority)
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[repr(u8)]
pub enum Priority {
    /// Critical: Agent migrations, error recovery
    Critical = 0,
    /// High: Discovery, registry updates
    High = 1,
    /// Normal: Load info, queries
    #[default]
    Normal = 2,
    /// Low: Ping/pong, background sync
    Low = 3,
}

impl Priority {
    /// Get priority from u8 value
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Priority::Critical,
            1 => Priority::High,
            2 => Priority::Normal,
            _ => Priority::Low,
        }
    }

    /// Convert to u8 value
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Get human-readable name
    pub fn name(self) -> &'static str {
        match self {
            Priority::Critical => "critical",
            Priority::High => "high",
            Priority::Normal => "normal",
            Priority::Low => "low",
        }
    }
}

/// A queued message with metadata
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// The message payload
    pub message: Message,
    /// Priority level
    pub priority: Priority,
    /// Unique sequence number for ordering
    pub sequence: u64,
    /// When the message was enqueued
    pub enqueued_at: Instant,
    /// Destination node ID (optional)
    pub destination: Option<[u8; 16]>,
    /// Number of send attempts
    pub attempts: u32,
    /// Maximum retry attempts
    pub max_attempts: u32,
}

impl QueuedMessage {
    /// Create a new queued message
    pub fn new(message: Message, priority: Priority, sequence: u64) -> Self {
        Self {
            message,
            priority,
            sequence,
            enqueued_at: Instant::now(),
            destination: None,
            attempts: 0,
            max_attempts: 3,
        }
    }

    /// Create a queued message with destination
    pub fn with_destination(
        message: Message,
        priority: Priority,
        sequence: u64,
        destination: [u8; 16],
    ) -> Self {
        Self {
            message,
            priority,
            sequence,
            enqueued_at: Instant::now(),
            destination: Some(destination),
            attempts: 0,
            max_attempts: 3,
        }
    }

    /// Get time spent in queue
    pub fn queue_time(&self) -> Duration {
        self.enqueued_at.elapsed()
    }

    /// Check if message can be retried
    pub fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts
    }

    /// Increment attempt counter
    pub fn increment_attempts(&mut self) {
        self.attempts += 1;
    }
}

// Implement ordering for priority queue (higher priority = smaller value = greater in heap)
impl PartialEq for QueuedMessage {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for QueuedMessage {}

impl PartialOrd for QueuedMessage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedMessage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority (lower number) comes first
        // If same priority, lower sequence (earlier) comes first
        match other.priority.cmp(&self.priority) {
            std::cmp::Ordering::Equal => other.sequence.cmp(&self.sequence),
            ord => ord,
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    /// Total messages enqueued
    pub enqueued: u64,
    /// Total messages dequeued
    pub dequeued: u64,
    /// Total messages dropped (capacity exceeded)
    pub dropped: u64,
    /// Current queue size
    pub current_size: usize,
    /// Messages per priority level
    pub by_priority: [u64; 4],
    /// Average queue time in milliseconds
    pub avg_queue_time_ms: u64,
}

impl QueueStats {
    /// Calculate throughput (messages per second)
    pub fn throughput(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs > 0.0 {
            self.dequeued as f64 / elapsed_secs
        } else {
            0.0
        }
    }
}

/// Configuration for priority queue
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum total messages in queue
    pub max_size: usize,
    /// Maximum messages per priority level
    pub max_per_priority: [usize; 4],
    /// Maximum age before message is dropped (milliseconds)
    pub max_age_ms: u64,
    /// Enable fair scheduling within priorities
    pub fair_scheduling: bool,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_size: 10000,
            max_per_priority: [1000, 2000, 4000, 3000],
            max_age_ms: 30000, // 30 seconds
            fair_scheduling: true,
        }
    }
}

impl QueueConfig {
    /// Create config for high-throughput scenarios
    pub fn high_throughput() -> Self {
        Self {
            max_size: 50000,
            max_per_priority: [5000, 10000, 20000, 15000],
            max_age_ms: 60000,
            fair_scheduling: true,
        }
    }

    /// Create config for low-latency scenarios
    pub fn low_latency() -> Self {
        Self {
            max_size: 1000,
            max_per_priority: [200, 300, 300, 200],
            max_age_ms: 5000,
            fair_scheduling: false,
        }
    }

    /// Create config for embedded/resource-constrained scenarios
    pub fn embedded() -> Self {
        Self {
            max_size: 100,
            max_per_priority: [25, 25, 30, 20],
            max_age_ms: 10000,
            fair_scheduling: false,
        }
    }
}

/// Priority queue for messages
pub struct PriorityQueue {
    /// The priority heap
    heap: BinaryHeap<QueuedMessage>,
    /// Per-priority FIFO queues for fair scheduling
    fair_queues: [VecDeque<QueuedMessage>; 4],
    /// Sequence counter for ordering
    sequence: AtomicU64,
    /// Configuration
    config: QueueConfig,
    /// Per-priority counts
    priority_counts: [usize; 4],
    /// Statistics
    stats: QueueStats,
    /// Total queue time for average calculation
    total_queue_time_ns: u64,
}

impl PriorityQueue {
    /// Create a new priority queue
    pub fn new() -> Self {
        Self::with_config(QueueConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: QueueConfig) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(config.max_size),
            fair_queues: [
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
                VecDeque::new(),
            ],
            sequence: AtomicU64::new(0),
            config,
            priority_counts: [0; 4],
            stats: QueueStats::default(),
            total_queue_time_ns: 0,
        }
    }

    /// Enqueue a message with automatic priority detection
    pub fn enqueue(&mut self, message: Message) -> Result<u64, WireError> {
        let priority = Self::detect_priority(&message);
        self.enqueue_with_priority(message, priority)
    }

    /// Enqueue a message with specific priority
    pub fn enqueue_with_priority(
        &mut self,
        message: Message,
        priority: Priority,
    ) -> Result<u64, WireError> {
        // Check capacity limits
        if self.len() >= self.config.max_size {
            self.stats.dropped += 1;
            return Err(WireError::TransportError(
                "Queue capacity exceeded".to_string(),
            ));
        }

        let priority_idx = priority.as_u8() as usize;
        if self.priority_counts[priority_idx] >= self.config.max_per_priority[priority_idx] {
            self.stats.dropped += 1;
            return Err(WireError::TransportError(format!(
                "Priority {} capacity exceeded",
                priority.name()
            )));
        }

        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let queued = QueuedMessage::new(message, priority, sequence);

        if self.config.fair_scheduling {
            self.fair_queues[priority_idx].push_back(queued);
        } else {
            self.heap.push(queued);
        }

        self.priority_counts[priority_idx] += 1;
        self.stats.enqueued += 1;
        self.stats.current_size = self.len();
        self.stats.by_priority[priority_idx] += 1;

        Ok(sequence)
    }

    /// Enqueue with destination
    pub fn enqueue_to(
        &mut self,
        message: Message,
        destination: [u8; 16],
    ) -> Result<u64, WireError> {
        let priority = Self::detect_priority(&message);
        self.enqueue_to_with_priority(message, priority, destination)
    }

    /// Enqueue with priority and destination
    pub fn enqueue_to_with_priority(
        &mut self,
        message: Message,
        priority: Priority,
        destination: [u8; 16],
    ) -> Result<u64, WireError> {
        // Check capacity limits
        if self.len() >= self.config.max_size {
            self.stats.dropped += 1;
            return Err(WireError::TransportError(
                "Queue capacity exceeded".to_string(),
            ));
        }

        let priority_idx = priority.as_u8() as usize;
        if self.priority_counts[priority_idx] >= self.config.max_per_priority[priority_idx] {
            self.stats.dropped += 1;
            return Err(WireError::TransportError(format!(
                "Priority {} capacity exceeded",
                priority.name()
            )));
        }

        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let queued = QueuedMessage::with_destination(message, priority, sequence, destination);

        if self.config.fair_scheduling {
            self.fair_queues[priority_idx].push_back(queued);
        } else {
            self.heap.push(queued);
        }

        self.priority_counts[priority_idx] += 1;
        self.stats.enqueued += 1;
        self.stats.current_size = self.len();
        self.stats.by_priority[priority_idx] += 1;

        Ok(sequence)
    }

    /// Dequeue the highest priority message
    pub fn dequeue(&mut self) -> Option<QueuedMessage> {
        let msg = if self.config.fair_scheduling {
            self.dequeue_fair()
        } else {
            self.heap.pop()
        };

        if let Some(ref m) = msg {
            let priority_idx = m.priority.as_u8() as usize;
            self.priority_counts[priority_idx] =
                self.priority_counts[priority_idx].saturating_sub(1);
            self.stats.dequeued += 1;
            self.stats.current_size = self.len();

            // Update average queue time
            let queue_time_ns = m.queue_time().as_nanos() as u64;
            self.total_queue_time_ns += queue_time_ns;
            if self.stats.dequeued > 0 {
                self.stats.avg_queue_time_ms =
                    (self.total_queue_time_ns / self.stats.dequeued) / 1_000_000;
            }
        }

        msg
    }

    /// Fair dequeue: round-robin within priorities
    fn dequeue_fair(&mut self) -> Option<QueuedMessage> {
        // Check queues in priority order
        for queue in self.fair_queues.iter_mut() {
            if let Some(msg) = queue.pop_front() {
                return Some(msg);
            }
        }
        None
    }

    /// Peek at the highest priority message without removing
    pub fn peek(&self) -> Option<&QueuedMessage> {
        if self.config.fair_scheduling {
            for queue in self.fair_queues.iter() {
                if let Some(msg) = queue.front() {
                    return Some(msg);
                }
            }
            None
        } else {
            self.heap.peek()
        }
    }

    /// Get current queue length
    pub fn len(&self) -> usize {
        if self.config.fair_scheduling {
            self.fair_queues.iter().map(|q| q.len()).sum()
        } else {
            self.heap.len()
        }
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get queue statistics
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }

    /// Get count for a specific priority
    pub fn count_by_priority(&self, priority: Priority) -> usize {
        self.priority_counts[priority.as_u8() as usize]
    }

    /// Remove expired messages
    pub fn cleanup_expired(&mut self) -> usize {
        let max_age = Duration::from_millis(self.config.max_age_ms);
        let mut removed = 0;

        if self.config.fair_scheduling {
            for (priority_idx, queue) in self.fair_queues.iter_mut().enumerate() {
                let before_len = queue.len();
                queue.retain(|msg| msg.enqueued_at.elapsed() < max_age);
                let diff = before_len - queue.len();
                removed += diff;
                self.priority_counts[priority_idx] =
                    self.priority_counts[priority_idx].saturating_sub(diff);
            }
        } else {
            // For heap, we need to rebuild
            let expired: Vec<_> = self
                .heap
                .iter()
                .filter(|msg| msg.enqueued_at.elapsed() >= max_age)
                .map(|msg| msg.sequence)
                .collect();

            if !expired.is_empty() {
                let old_heap = std::mem::take(&mut self.heap);
                for msg in old_heap {
                    if !expired.contains(&msg.sequence) {
                        self.heap.push(msg);
                    } else {
                        let priority_idx = msg.priority.as_u8() as usize;
                        self.priority_counts[priority_idx] =
                            self.priority_counts[priority_idx].saturating_sub(1);
                        removed += 1;
                    }
                }
            }
        }

        self.stats.current_size = self.len();
        removed
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.heap.clear();
        for queue in self.fair_queues.iter_mut() {
            queue.clear();
        }
        self.priority_counts = [0; 4];
        self.stats.current_size = 0;
    }

    /// Detect priority from message type
    fn detect_priority(message: &Message) -> Priority {
        match message {
            Message::AgentMigration { priority, .. } => {
                if *priority >= 8 {
                    Priority::Critical
                } else if *priority >= 5 {
                    Priority::High
                } else {
                    Priority::Normal
                }
            }
            Message::MigrationAck { success, .. } => {
                if *success {
                    Priority::Normal
                } else {
                    Priority::Critical // Error acks are critical
                }
            }
            Message::Discovery { .. } | Message::DiscoveryResponse { .. } => Priority::High,
            Message::AgentQuery { .. } => Priority::Normal,
            Message::LoadInfo { .. } => Priority::Low,
            Message::Ping { .. } | Message::Pong { .. } => Priority::Low,
            Message::RoutedMessage { payload, .. } => Self::detect_priority(payload),
            Message::VersionNegotiationRequest { .. } => Priority::High,
            Message::VersionNegotiationResponse { .. } => Priority::High,
            Message::ProtocolUpgrade { .. } => Priority::High,
            Message::ProtocolUpgradeResponse { .. } => Priority::High,
            Message::WebSocketUpgrade { .. } => Priority::Normal,
            Message::WebSocketUpgradeResponse { .. } => Priority::Normal,
        }
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe priority queue wrapper
pub struct SharedPriorityQueue {
    inner: Arc<Mutex<PriorityQueue>>,
}

impl SharedPriorityQueue {
    /// Create a new shared priority queue
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PriorityQueue::new())),
        }
    }

    /// Create with custom config
    pub fn with_config(config: QueueConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PriorityQueue::with_config(config))),
        }
    }

    /// Enqueue a message
    pub fn enqueue(&self, message: Message) -> Result<u64, WireError> {
        self.inner.lock().unwrap().enqueue(message)
    }

    /// Enqueue with specific priority
    pub fn enqueue_with_priority(
        &self,
        message: Message,
        priority: Priority,
    ) -> Result<u64, WireError> {
        self.inner
            .lock()
            .unwrap()
            .enqueue_with_priority(message, priority)
    }

    /// Dequeue the highest priority message
    pub fn dequeue(&self) -> Option<QueuedMessage> {
        self.inner.lock().unwrap().dequeue()
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Get statistics
    pub fn stats(&self) -> QueueStats {
        self.inner.lock().unwrap().stats().clone()
    }

    /// Cleanup expired messages
    pub fn cleanup_expired(&self) -> usize {
        self.inner.lock().unwrap().cleanup_expired()
    }

    /// Clear the queue
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Clone the Arc for sharing
    pub fn clone_shared(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for SharedPriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SharedPriorityQueue {
    fn clone(&self) -> Self {
        self.clone_shared()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Normal);
        assert!(Priority::Normal < Priority::Low);
    }

    #[test]
    fn test_priority_from_u8() {
        assert_eq!(Priority::from_u8(0), Priority::Critical);
        assert_eq!(Priority::from_u8(1), Priority::High);
        assert_eq!(Priority::from_u8(2), Priority::Normal);
        assert_eq!(Priority::from_u8(3), Priority::Low);
        assert_eq!(Priority::from_u8(255), Priority::Low);
    }

    #[test]
    fn test_queue_creation() {
        let queue = PriorityQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_enqueue_dequeue() {
        let mut queue = PriorityQueue::new();

        let msg = Message::Ping { timestamp: 12345 };
        queue.enqueue(msg).unwrap();

        assert_eq!(queue.len(), 1);

        let dequeued = queue.dequeue().unwrap();
        assert!(matches!(dequeued.message, Message::Ping { .. }));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_priority_order() {
        let mut queue = PriorityQueue::with_config(QueueConfig {
            fair_scheduling: false,
            ..Default::default()
        });

        // Enqueue in reverse priority order
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 1 }, Priority::Low)
            .unwrap();
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 2 }, Priority::Normal)
            .unwrap();
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 3 }, Priority::High)
            .unwrap();
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 4 }, Priority::Critical)
            .unwrap();

        // Dequeue should be in priority order
        assert_eq!(queue.dequeue().unwrap().priority, Priority::Critical);
        assert_eq!(queue.dequeue().unwrap().priority, Priority::High);
        assert_eq!(queue.dequeue().unwrap().priority, Priority::Normal);
        assert_eq!(queue.dequeue().unwrap().priority, Priority::Low);
    }

    #[test]
    fn test_fair_scheduling() {
        let mut queue = PriorityQueue::with_config(QueueConfig {
            fair_scheduling: true,
            ..Default::default()
        });

        // Enqueue multiple at same priority
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 1 }, Priority::Normal)
            .unwrap();
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 2 }, Priority::Normal)
            .unwrap();

        // Should come out in FIFO order within priority
        let first = queue.dequeue().unwrap();
        let second = queue.dequeue().unwrap();

        if let (Message::Ping { timestamp: t1 }, Message::Ping { timestamp: t2 }) =
            (&first.message, &second.message)
        {
            assert_eq!(*t1, 1);
            assert_eq!(*t2, 2);
        }
    }

    #[test]
    fn test_auto_priority_detection() {
        let mut queue = PriorityQueue::new();

        // High priority migration
        let migration = Message::AgentMigration {
            agent_id: [0u8; 16],
            snapshot: vec![],
            priority: 10,
        };
        queue.enqueue(migration).unwrap();
        assert_eq!(queue.count_by_priority(Priority::Critical), 1);

        // Low priority ping
        let ping = Message::Ping { timestamp: 0 };
        queue.enqueue(ping).unwrap();
        assert_eq!(queue.count_by_priority(Priority::Low), 1);
    }

    #[test]
    fn test_capacity_limit() {
        let mut queue = PriorityQueue::with_config(QueueConfig {
            max_size: 2,
            ..Default::default()
        });

        queue.enqueue(Message::Ping { timestamp: 1 }).unwrap();
        queue.enqueue(Message::Ping { timestamp: 2 }).unwrap();

        // Third should fail
        let result = queue.enqueue(Message::Ping { timestamp: 3 });
        assert!(result.is_err());
        assert_eq!(queue.stats().dropped, 1);
    }

    #[test]
    fn test_priority_capacity_limit() {
        let mut queue = PriorityQueue::with_config(QueueConfig {
            max_size: 100,
            max_per_priority: [1, 1, 1, 1],
            ..Default::default()
        });

        queue
            .enqueue_with_priority(Message::Ping { timestamp: 1 }, Priority::Low)
            .unwrap();

        // Second low priority should fail
        let result = queue.enqueue_with_priority(Message::Ping { timestamp: 2 }, Priority::Low);
        assert!(result.is_err());

        // But other priorities should work
        queue
            .enqueue_with_priority(Message::Ping { timestamp: 3 }, Priority::Normal)
            .unwrap();
    }

    #[test]
    fn test_queue_stats() {
        let mut queue = PriorityQueue::new();

        queue.enqueue(Message::Ping { timestamp: 1 }).unwrap();
        queue.enqueue(Message::Ping { timestamp: 2 }).unwrap();

        assert_eq!(queue.stats().enqueued, 2);
        assert_eq!(queue.stats().current_size, 2);

        queue.dequeue();

        assert_eq!(queue.stats().dequeued, 1);
        assert_eq!(queue.stats().current_size, 1);
    }

    #[test]
    fn test_queued_message_retry() {
        let mut msg = QueuedMessage::new(Message::Ping { timestamp: 0 }, Priority::Normal, 0);

        assert!(msg.can_retry());
        assert_eq!(msg.attempts, 0);

        msg.increment_attempts();
        msg.increment_attempts();
        msg.increment_attempts();

        assert!(!msg.can_retry());
    }

    #[test]
    fn test_clear() {
        let mut queue = PriorityQueue::new();

        queue.enqueue(Message::Ping { timestamp: 1 }).unwrap();
        queue.enqueue(Message::Ping { timestamp: 2 }).unwrap();

        queue.clear();

        assert!(queue.is_empty());
        assert_eq!(queue.count_by_priority(Priority::Low), 0);
    }

    #[test]
    fn test_peek() {
        let mut queue = PriorityQueue::new();

        queue
            .enqueue_with_priority(Message::Ping { timestamp: 1 }, Priority::Normal)
            .unwrap();

        let peeked = queue.peek().unwrap();
        assert_eq!(peeked.priority, Priority::Normal);

        // Should still be there
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_shared_queue() {
        let queue = SharedPriorityQueue::new();
        let queue2 = queue.clone_shared();

        queue.enqueue(Message::Ping { timestamp: 1 }).unwrap();

        assert_eq!(queue2.len(), 1);

        queue2.dequeue();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_configs() {
        let high = QueueConfig::high_throughput();
        assert_eq!(high.max_size, 50000);

        let low = QueueConfig::low_latency();
        assert_eq!(low.max_size, 1000);

        let embedded = QueueConfig::embedded();
        assert_eq!(embedded.max_size, 100);
    }

    #[test]
    fn test_enqueue_to_destination() {
        let mut queue = PriorityQueue::new();
        let dest = [1u8; 16];

        queue
            .enqueue_to(Message::Ping { timestamp: 1 }, dest)
            .unwrap();

        let msg = queue.dequeue().unwrap();
        assert_eq!(msg.destination, Some(dest));
    }

    #[test]
    fn test_queued_message_ordering() {
        // Higher priority (Critical=0) should be "greater" in the heap
        let critical = QueuedMessage::new(Message::Ping { timestamp: 0 }, Priority::Critical, 0);
        let low = QueuedMessage::new(Message::Ping { timestamp: 0 }, Priority::Low, 1);

        assert!(critical > low);

        // Same priority, lower sequence should be "greater"
        let earlier = QueuedMessage::new(Message::Ping { timestamp: 0 }, Priority::Normal, 0);
        let later = QueuedMessage::new(Message::Ping { timestamp: 0 }, Priority::Normal, 1);

        assert!(earlier > later);
    }
}
