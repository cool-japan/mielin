//! Inter-agent messaging API
//!
//! Provides communication primitives for agents to exchange messages.
//! Supports request/response, pub/sub, and fire-and-forget patterns.

use crate::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

/// Message ID for tracking and deduplication
pub type MessageId = Uuid;

/// Topic for pub/sub messaging
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Topic(pub String);

impl Topic {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl From<&str> for Topic {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Message envelope containing metadata and payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: MessageId,
    /// Sender agent ID
    pub from: AgentId,
    /// Receiver agent ID (None for broadcast/topic messages)
    pub to: Option<AgentId>,
    /// Optional topic for pub/sub
    pub topic: Option<Topic>,
    /// Message payload
    pub payload: Vec<u8>,
    /// Message priority
    pub priority: Priority,
    /// Message creation timestamp (millis since epoch)
    pub timestamp: u64,
    /// Time-to-live in milliseconds (0 = no expiry)
    pub ttl: u64,
    /// Correlation ID for request/response patterns
    pub correlation_id: Option<MessageId>,
    /// Whether this is a response message
    pub is_response: bool,
}

impl Message {
    /// Create a new message
    pub fn new(from: AgentId, to: AgentId, payload: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to: Some(to),
            topic: None,
            payload,
            priority: Priority::Normal,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            ttl: 0,
            correlation_id: None,
            is_response: false,
        }
    }

    /// Create a broadcast message to a topic
    pub fn broadcast(from: AgentId, topic: Topic, payload: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to: None,
            topic: Some(topic),
            payload,
            priority: Priority::Normal,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            ttl: 0,
            correlation_id: None,
            is_response: false,
        }
    }

    /// Set message priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set time-to-live
    pub fn with_ttl(mut self, ttl_ms: u64) -> Self {
        self.ttl = ttl_ms;
        self
    }

    /// Set correlation ID for request/response
    pub fn with_correlation(mut self, correlation_id: MessageId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Create a response to this message
    pub fn create_response(&self, from: AgentId, payload: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to: Some(self.from),
            topic: None,
            payload,
            priority: self.priority,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            ttl: 0,
            correlation_id: Some(self.id),
            is_response: true,
        }
    }

    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        if self.ttl == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        now > self.timestamp + self.ttl
    }
}

/// Agent mailbox for receiving messages
pub struct Mailbox {
    agent_id: AgentId,
    receiver: mpsc::Receiver<Message>,
    #[allow(dead_code)] // Reserved for future direct response handling
    pending_responses: Arc<RwLock<HashMap<MessageId, oneshot::Sender<Message>>>>,
}

impl Mailbox {
    /// Receive next message (blocking)
    pub async fn recv(&mut self) -> Option<Message> {
        self.receiver.recv().await
    }

    /// Try to receive a message without blocking
    pub fn try_recv(&mut self) -> Option<Message> {
        self.receiver.try_recv().ok()
    }

    /// Receive with timeout
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<Message> {
        tokio::time::timeout(timeout, self.receiver.recv())
            .await
            .ok()
            .flatten()
    }

    /// Get the agent ID this mailbox belongs to
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }
}

/// Message bus for inter-agent communication
pub struct MessageBus {
    /// Agent mailboxes (agent_id -> sender)
    mailboxes: RwLock<HashMap<AgentId, mpsc::Sender<Message>>>,
    /// Topic subscriptions (topic -> set of agent_ids)
    subscriptions: RwLock<HashMap<Topic, Vec<AgentId>>>,
    /// Pending request/response handlers
    pending_responses: Arc<RwLock<HashMap<MessageId, oneshot::Sender<Message>>>>,
    /// Message queue for offline agents
    offline_queues: RwLock<HashMap<AgentId, VecDeque<Message>>>,
    /// Maximum queue size per offline agent
    max_queue_size: usize,
    /// Seen message IDs for deduplication
    seen_messages: RwLock<HashMap<MessageId, Instant>>,
    /// Deduplication window
    dedup_window: Duration,
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        Self {
            mailboxes: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
            offline_queues: RwLock::new(HashMap::new()),
            max_queue_size: 1000,
            dedup_window: Duration::from_secs(60),
            seen_messages: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new message bus with custom settings
    pub fn with_settings(max_queue_size: usize, dedup_window: Duration) -> Self {
        Self {
            mailboxes: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
            offline_queues: RwLock::new(HashMap::new()),
            max_queue_size,
            dedup_window,
            seen_messages: RwLock::new(HashMap::new()),
        }
    }

    /// Register an agent and get its mailbox
    pub async fn register(&self, agent_id: AgentId) -> Mailbox {
        let (tx, rx) = mpsc::channel(256);

        // Store the sender
        self.mailboxes.write().await.insert(agent_id, tx);

        // Deliver any queued messages
        if let Some(queue) = self.offline_queues.write().await.remove(&agent_id) {
            let mailboxes = self.mailboxes.read().await;
            if let Some(sender) = mailboxes.get(&agent_id) {
                for msg in queue {
                    if !msg.is_expired() {
                        let _ = sender.try_send(msg);
                    }
                }
            }
        }

        Mailbox {
            agent_id,
            receiver: rx,
            pending_responses: Arc::clone(&self.pending_responses),
        }
    }

    /// Unregister an agent
    pub async fn unregister(&self, agent_id: &AgentId) {
        self.mailboxes.write().await.remove(agent_id);

        // Remove from all topic subscriptions
        let mut subs = self.subscriptions.write().await;
        for subscribers in subs.values_mut() {
            subscribers.retain(|id| id != agent_id);
        }
    }

    /// Send a message to a specific agent
    pub async fn send(&self, message: Message) -> Result<(), MessagingError> {
        // Check for duplicate
        if self.is_duplicate(&message).await {
            return Err(MessagingError::DuplicateMessage(message.id));
        }

        // Mark as seen
        self.mark_seen(message.id).await;

        // Check if expired
        if message.is_expired() {
            return Err(MessagingError::MessageExpired(message.id));
        }

        let to = message.to.ok_or(MessagingError::NoRecipient)?;

        // Check if this is a response
        if message.is_response {
            if let Some(correlation_id) = message.correlation_id {
                let mut pending = self.pending_responses.write().await;
                if let Some(sender) = pending.remove(&correlation_id) {
                    let _ = sender.send(message);
                    return Ok(());
                }
            }
        }

        // Try to deliver directly
        let mailboxes = self.mailboxes.read().await;
        if let Some(sender) = mailboxes.get(&to) {
            sender
                .try_send(message)
                .map_err(|e| MessagingError::SendFailed(e.to_string()))?;
        } else {
            // Queue for offline agent
            drop(mailboxes);
            self.queue_for_offline(to, message).await?;
        }

        Ok(())
    }

    /// Send a request and wait for response
    pub async fn request(
        &self,
        message: Message,
        timeout: Duration,
    ) -> Result<Message, MessagingError> {
        let message_id = message.id;

        // Create response channel
        let (tx, rx) = oneshot::channel();
        self.pending_responses.write().await.insert(message_id, tx);

        // Send the request
        self.send(message).await?;

        // Wait for response with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                self.pending_responses.write().await.remove(&message_id);
                Err(MessagingError::ResponseChannelClosed)
            }
            Err(_) => {
                self.pending_responses.write().await.remove(&message_id);
                Err(MessagingError::Timeout)
            }
        }
    }

    /// Subscribe an agent to a topic
    pub async fn subscribe(&self, agent_id: AgentId, topic: Topic) {
        let mut subs = self.subscriptions.write().await;
        subs.entry(topic).or_default().push(agent_id);
    }

    /// Unsubscribe an agent from a topic
    pub async fn unsubscribe(&self, agent_id: &AgentId, topic: &Topic) {
        let mut subs = self.subscriptions.write().await;
        if let Some(subscribers) = subs.get_mut(topic) {
            subscribers.retain(|id| id != agent_id);
        }
    }

    /// Publish a message to a topic
    pub async fn publish(&self, message: Message) -> Result<usize, MessagingError> {
        // Check for duplicate
        if self.is_duplicate(&message).await {
            return Err(MessagingError::DuplicateMessage(message.id));
        }

        // Mark as seen
        self.mark_seen(message.id).await;

        let topic = message.topic.as_ref().ok_or(MessagingError::NoTopic)?;

        let subs = self.subscriptions.read().await;
        let subscribers = subs.get(topic).cloned().unwrap_or_default();
        drop(subs);

        let mut delivered = 0;
        let mailboxes = self.mailboxes.read().await;

        for agent_id in subscribers {
            if let Some(sender) = mailboxes.get(&agent_id) {
                if sender.try_send(message.clone()).is_ok() {
                    delivered += 1;
                }
            }
        }

        Ok(delivered)
    }

    /// Queue a message for an offline agent
    async fn queue_for_offline(
        &self,
        agent_id: AgentId,
        message: Message,
    ) -> Result<(), MessagingError> {
        let mut queues = self.offline_queues.write().await;
        let queue = queues.entry(agent_id).or_default();

        if queue.len() >= self.max_queue_size {
            // Remove oldest message
            queue.pop_front();
        }

        queue.push_back(message);
        Ok(())
    }

    /// Check if message is a duplicate
    async fn is_duplicate(&self, message: &Message) -> bool {
        let seen = self.seen_messages.read().await;
        seen.contains_key(&message.id)
    }

    /// Mark message as seen
    async fn mark_seen(&self, message_id: MessageId) {
        let mut seen = self.seen_messages.write().await;
        seen.insert(message_id, Instant::now());

        // Clean up old entries
        let cutoff = Instant::now() - self.dedup_window;
        seen.retain(|_, time| *time > cutoff);
    }

    /// Get number of registered agents
    pub async fn agent_count(&self) -> usize {
        self.mailboxes.read().await.len()
    }

    /// Get number of topics
    pub async fn topic_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }

    /// Get subscribers for a topic
    pub async fn get_subscribers(&self, topic: &Topic) -> Vec<AgentId> {
        self.subscriptions
            .read()
            .await
            .get(topic)
            .cloned()
            .unwrap_or_default()
    }
}

/// Messaging errors
#[derive(Debug, thiserror::Error)]
pub enum MessagingError {
    #[error("No recipient specified")]
    NoRecipient,
    #[error("No topic specified for publish")]
    NoTopic,
    #[error("Failed to send message: {0}")]
    SendFailed(String),
    #[error("Message expired: {0}")]
    MessageExpired(MessageId),
    #[error("Duplicate message: {0}")]
    DuplicateMessage(MessageId),
    #[error("Request timeout")]
    Timeout,
    #[error("Response channel closed")]
    ResponseChannelClosed,
    #[error("Agent not found: {0}")]
    AgentNotFound(AgentId),
}

// NOTE: All tests in this module use #[tokio::test] which creates a tokio runtime.
// Tokio's runtime initialization calls mio's Poll::new → kqueue() syscall on macOS,
// which Miri cannot emulate (kqueue is a macOS-specific kernel event notification API).
// This is not undefined behavior — it is an OS-level I/O mechanism unavailable in Miri.
// All tests are annotated with #[cfg_attr(miri, ignore)] accordingly.
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_message_creation() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let msg = Message::new(from, to, b"hello".to_vec());

        assert_eq!(msg.from, from);
        assert_eq!(msg.to, Some(to));
        assert_eq!(msg.payload, b"hello".to_vec());
        assert_eq!(msg.priority, Priority::Normal);
        assert!(!msg.is_expired());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_broadcast_message() {
        let from = Uuid::new_v4();
        let topic = Topic::new("events");
        let msg = Message::broadcast(from, topic.clone(), b"event".to_vec());

        assert_eq!(msg.from, from);
        assert!(msg.to.is_none());
        assert_eq!(msg.topic, Some(topic));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_message_priority() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let msg = Message::new(from, to, vec![]).with_priority(Priority::Critical);

        assert_eq!(msg.priority, Priority::Critical);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_message_expiry() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let msg = Message::new(from, to, vec![]).with_ttl(1); // 1ms TTL

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(msg.is_expired());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_message_response() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let request = Message::new(from, to, b"request".to_vec());
        let response = request.create_response(to, b"response".to_vec());

        assert_eq!(response.from, to);
        assert_eq!(response.to, Some(from));
        assert_eq!(response.correlation_id, Some(request.id));
        assert!(response.is_response);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_message_bus_register() {
        let bus = MessageBus::new();
        let agent_id = Uuid::new_v4();

        let _mailbox = bus.register(agent_id).await;
        assert_eq!(bus.agent_count().await, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_message_bus_unregister() {
        let bus = MessageBus::new();
        let agent_id = Uuid::new_v4();

        let _mailbox = bus.register(agent_id).await;
        bus.unregister(&agent_id).await;
        assert_eq!(bus.agent_count().await, 0);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_send_message() {
        let bus = MessageBus::new();
        let sender_id = Uuid::new_v4();
        let receiver_id = Uuid::new_v4();

        let _sender_mailbox = bus.register(sender_id).await;
        let mut receiver_mailbox = bus.register(receiver_id).await;

        let msg = Message::new(sender_id, receiver_id, b"hello".to_vec());
        bus.send(msg).await.unwrap();

        let received = receiver_mailbox
            .recv_timeout(Duration::from_millis(100))
            .await;
        assert!(received.is_some());
        assert_eq!(received.unwrap().payload, b"hello".to_vec());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_pubsub() {
        let bus = MessageBus::new();
        let publisher_id = Uuid::new_v4();
        let subscriber1_id = Uuid::new_v4();
        let subscriber2_id = Uuid::new_v4();
        let topic = Topic::new("news");

        let _pub_mailbox = bus.register(publisher_id).await;
        let mut sub1_mailbox = bus.register(subscriber1_id).await;
        let mut sub2_mailbox = bus.register(subscriber2_id).await;

        bus.subscribe(subscriber1_id, topic.clone()).await;
        bus.subscribe(subscriber2_id, topic.clone()).await;

        let msg = Message::broadcast(publisher_id, topic, b"breaking news".to_vec());
        let delivered = bus.publish(msg).await.unwrap();

        assert_eq!(delivered, 2);

        let recv1 = sub1_mailbox.recv_timeout(Duration::from_millis(100)).await;
        let recv2 = sub2_mailbox.recv_timeout(Duration::from_millis(100)).await;

        assert!(recv1.is_some());
        assert!(recv2.is_some());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_request_response() {
        let bus = Arc::new(MessageBus::new());
        let requester_id = Uuid::new_v4();
        let responder_id = Uuid::new_v4();

        let _req_mailbox = bus.register(requester_id).await;
        let mut resp_mailbox = bus.register(responder_id).await;

        let bus_clone = Arc::clone(&bus);

        // Spawn responder
        let handle = tokio::spawn(async move {
            if let Some(msg) = resp_mailbox.recv_timeout(Duration::from_millis(100)).await {
                let response = msg.create_response(responder_id, b"pong".to_vec());
                bus_clone.send(response).await.unwrap();
            }
        });

        // Send request
        let request = Message::new(requester_id, responder_id, b"ping".to_vec());
        let response = bus.request(request, Duration::from_millis(500)).await;

        handle.await.unwrap();
        assert!(response.is_ok());
        assert_eq!(response.unwrap().payload, b"pong".to_vec());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_offline_queue() {
        let bus = MessageBus::new();
        let sender_id = Uuid::new_v4();
        let offline_id = Uuid::new_v4();

        let _sender_mailbox = bus.register(sender_id).await;

        // Send to offline agent
        let msg = Message::new(sender_id, offline_id, b"queued".to_vec());
        bus.send(msg).await.unwrap();

        // Now register the agent - should receive queued message
        let mut offline_mailbox = bus.register(offline_id).await;
        let received = offline_mailbox
            .recv_timeout(Duration::from_millis(100))
            .await;

        assert!(received.is_some());
        assert_eq!(received.unwrap().payload, b"queued".to_vec());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_topic_subscription() {
        let bus = MessageBus::new();
        let agent_id = Uuid::new_v4();
        let topic = Topic::new("events");

        bus.subscribe(agent_id, topic.clone()).await;
        let subs = bus.get_subscribers(&topic).await;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], agent_id);

        bus.unsubscribe(&agent_id, &topic).await;
        let subs = bus.get_subscribers(&topic).await;
        assert!(subs.is_empty());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_duplicate_detection() {
        let bus = MessageBus::new();
        let sender_id = Uuid::new_v4();
        let receiver_id = Uuid::new_v4();

        let _sender_mailbox = bus.register(sender_id).await;
        let _receiver_mailbox = bus.register(receiver_id).await;

        let msg = Message::new(sender_id, receiver_id, b"once".to_vec());
        let msg_clone = msg.clone();

        // First send should succeed
        assert!(bus.send(msg).await.is_ok());

        // Second send of same message should fail
        let result = bus.send(msg_clone).await;
        assert!(matches!(result, Err(MessagingError::DuplicateMessage(_))));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_expired_message_rejected() {
        let bus = MessageBus::new();
        let sender_id = Uuid::new_v4();
        let receiver_id = Uuid::new_v4();

        let _sender_mailbox = bus.register(sender_id).await;
        let _receiver_mailbox = bus.register(receiver_id).await;

        let msg = Message::new(sender_id, receiver_id, vec![]).with_ttl(1);

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = bus.send(msg).await;
        assert!(matches!(result, Err(MessagingError::MessageExpired(_))));
    }
}
