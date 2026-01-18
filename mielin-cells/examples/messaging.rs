//! Inter-agent messaging patterns example
//!
//! Demonstrates:
//! - Point-to-point messaging
//! - Request/response pattern
//! - Pub/sub pattern with topics
//! - Message priorities and TTL
//! - Offline message queuing

use mielin_cells::{Agent, Message, MessageBus, Priority, Topic};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("=== MielinOS Messaging Patterns ===\n");

    // Create a shared message bus
    let bus = Arc::new(MessageBus::new());

    // Example 1: Point-to-point messaging
    point_to_point_messaging(Arc::clone(&bus)).await;

    // Example 2: Request/response pattern
    request_response_pattern(Arc::clone(&bus)).await;

    // Example 3: Pub/sub pattern
    pubsub_pattern(Arc::clone(&bus)).await;

    // Example 4: Message priorities
    message_priorities(Arc::clone(&bus)).await;

    // Example 5: Message TTL and expiry
    message_ttl(Arc::clone(&bus)).await;

    // Example 6: Offline message queuing
    offline_queuing(Arc::clone(&bus)).await;
}

/// Example 1: Basic point-to-point messaging
async fn point_to_point_messaging(bus: Arc<MessageBus>) {
    println!("1. Point-to-Point Messaging");
    println!("----------------------------");

    // Create two agents
    let alice = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let bob = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    println!("Alice ID: {}", alice.id());
    println!("Bob ID: {}", bob.id());

    // Register agents with the message bus
    let _alice_mailbox = bus.register(alice.id()).await;
    let mut bob_mailbox = bus.register(bob.id()).await;

    // Alice sends a message to Bob
    let message = Message::new(alice.id(), bob.id(), b"Hello Bob, this is Alice!".to_vec());

    println!("\nAlice -> Bob: \"Hello Bob, this is Alice!\"");
    bus.send(message).await.unwrap();

    // Bob receives the message
    if let Some(msg) = bob_mailbox.recv_timeout(Duration::from_millis(100)).await {
        let text = String::from_utf8_lossy(&msg.payload);
        println!("Bob received: \"{}\"", text);
        println!("From: {}", msg.from);
        println!("Priority: {:?}", msg.priority);
    }

    println!();
}

/// Example 2: Request/response pattern with timeout
async fn request_response_pattern(bus: Arc<MessageBus>) {
    println!("2. Request/Response Pattern");
    println!("----------------------------");

    // Create client and server agents
    let client = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let server = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    println!("Client ID: {}", client.id());
    println!("Server ID: {}", server.id());

    // Register agents
    let _client_mailbox = bus.register(client.id()).await;
    let mut server_mailbox = bus.register(server.id()).await;

    let bus_clone = Arc::clone(&bus);
    let server_id = server.id();

    // Spawn server task to handle requests
    let server_handle = tokio::spawn(async move {
        if let Some(request) = server_mailbox.recv_timeout(Duration::from_secs(1)).await {
            println!(
                "\nServer received request: \"{}\"",
                String::from_utf8_lossy(&request.payload)
            );

            // Process request and send response
            let response = request.create_response(
                server_id,
                b"Response: Request processed successfully!".to_vec(),
            );

            println!("Server sending response...");
            bus_clone.send(response).await.unwrap();
        }
    });

    // Client sends request and waits for response
    println!("\nClient sending request...");
    let request = Message::new(
        client.id(),
        server.id(),
        b"Please process this data".to_vec(),
    );

    match bus.request(request, Duration::from_secs(2)).await {
        Ok(response) => {
            println!(
                "Client received response: \"{}\"",
                String::from_utf8_lossy(&response.payload)
            );
            println!("Correlation ID: {:?}", response.correlation_id);
        }
        Err(e) => {
            println!("Request failed: {}", e);
        }
    }

    server_handle.await.unwrap();
    println!();
}

/// Example 3: Pub/sub pattern with topics
async fn pubsub_pattern(bus: Arc<MessageBus>) {
    println!("3. Pub/Sub Pattern");
    println!("------------------");

    // Create publisher and subscribers
    let publisher = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let subscriber1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let subscriber2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let subscriber3 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    println!("Publisher ID: {}", publisher.id());
    println!("Subscriber 1 ID: {}", subscriber1.id());
    println!("Subscriber 2 ID: {}", subscriber2.id());
    println!("Subscriber 3 ID: {}", subscriber3.id());

    // Register agents
    let _pub_mailbox = bus.register(publisher.id()).await;
    let mut sub1_mailbox = bus.register(subscriber1.id()).await;
    let mut sub2_mailbox = bus.register(subscriber2.id()).await;
    let mut sub3_mailbox = bus.register(subscriber3.id()).await;

    // Subscribe to topics
    let events_topic = Topic::new("system.events");
    let alerts_topic = Topic::new("system.alerts");

    println!("\nSubscribing agents to topics...");
    bus.subscribe(subscriber1.id(), events_topic.clone()).await;
    bus.subscribe(subscriber2.id(), events_topic.clone()).await;
    bus.subscribe(subscriber3.id(), alerts_topic.clone()).await;

    println!("Subscriber 1 -> {}", events_topic.0);
    println!("Subscriber 2 -> {}", events_topic.0);
    println!("Subscriber 3 -> {}", alerts_topic.0);

    // Publish to events topic
    println!("\nPublishing to {}...", events_topic.0);
    let event_msg = Message::broadcast(
        publisher.id(),
        events_topic.clone(),
        b"System update available".to_vec(),
    );

    let delivered = bus.publish(event_msg).await.unwrap();
    println!("Delivered to {} subscribers", delivered);

    // Publish to alerts topic
    println!("\nPublishing to {}...", alerts_topic.0);
    let alert_msg = Message::broadcast(
        publisher.id(),
        alerts_topic.clone(),
        b"High CPU usage detected!".to_vec(),
    );

    let delivered = bus.publish(alert_msg).await.unwrap();
    println!("Delivered to {} subscribers", delivered);

    // Subscribers receive messages
    if let Some(msg) = sub1_mailbox.recv_timeout(Duration::from_millis(100)).await {
        println!(
            "\nSubscriber 1 received: \"{}\"",
            String::from_utf8_lossy(&msg.payload)
        );
    }

    if let Some(msg) = sub2_mailbox.recv_timeout(Duration::from_millis(100)).await {
        println!(
            "Subscriber 2 received: \"{}\"",
            String::from_utf8_lossy(&msg.payload)
        );
    }

    if let Some(msg) = sub3_mailbox.recv_timeout(Duration::from_millis(100)).await {
        println!(
            "Subscriber 3 received: \"{}\"",
            String::from_utf8_lossy(&msg.payload)
        );
    }

    println!();
}

/// Example 4: Message priorities
async fn message_priorities(bus: Arc<MessageBus>) {
    println!("4. Message Priorities");
    println!("---------------------");

    let sender = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let receiver = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    let _sender_mailbox = bus.register(sender.id()).await;
    let mut receiver_mailbox = bus.register(receiver.id()).await;

    // Send messages with different priorities
    let low = Message::new(sender.id(), receiver.id(), b"Low priority task".to_vec())
        .with_priority(Priority::Low);

    let normal = Message::new(sender.id(), receiver.id(), b"Normal task".to_vec())
        .with_priority(Priority::Normal);

    let high = Message::new(sender.id(), receiver.id(), b"High priority task".to_vec())
        .with_priority(Priority::High);

    let critical = Message::new(sender.id(), receiver.id(), b"CRITICAL ALERT!".to_vec())
        .with_priority(Priority::Critical);

    println!("Sending messages with different priorities...");
    bus.send(low).await.unwrap();
    bus.send(normal).await.unwrap();
    bus.send(high).await.unwrap();
    bus.send(critical).await.unwrap();

    // Receive and display messages
    for i in 1..=4 {
        if let Some(msg) = receiver_mailbox
            .recv_timeout(Duration::from_millis(100))
            .await
        {
            println!(
                "Message {}: {:?} - \"{}\"",
                i,
                msg.priority,
                String::from_utf8_lossy(&msg.payload)
            );
        }
    }

    println!();
}

/// Example 5: Message TTL and expiry
async fn message_ttl(bus: Arc<MessageBus>) {
    println!("5. Message TTL and Expiry");
    println!("-------------------------");

    let sender = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let receiver = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    let _sender_mailbox = bus.register(sender.id()).await;
    let mut receiver_mailbox = bus.register(receiver.id()).await;

    // Create message with short TTL
    let msg = Message::new(
        sender.id(),
        receiver.id(),
        b"This message will expire quickly".to_vec(),
    )
    .with_ttl(50); // 50ms TTL

    println!("Sending message with 50ms TTL...");
    bus.send(msg.clone()).await.unwrap();

    // Receive immediately - should succeed
    if let Some(received) = receiver_mailbox
        .recv_timeout(Duration::from_millis(10))
        .await
    {
        println!(
            "Received before expiry: \"{}\"",
            String::from_utf8_lossy(&received.payload)
        );
    }

    // Wait for expiry
    println!("\nWaiting for message to expire...");
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Try to send expired message
    let expired_msg = Message::new(sender.id(), receiver.id(), b"Expired".to_vec()).with_ttl(1);

    tokio::time::sleep(Duration::from_millis(10)).await;

    match bus.send(expired_msg).await {
        Ok(_) => println!("Message sent"),
        Err(e) => println!("Failed to send expired message: {}", e),
    }

    println!();
}

/// Example 6: Offline message queuing
async fn offline_queuing(bus: Arc<MessageBus>) {
    println!("6. Offline Message Queuing");
    println!("--------------------------");

    let sender = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let offline_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    let _sender_mailbox = bus.register(sender.id()).await;

    println!("Sender ID: {}", sender.id());
    println!("Offline Agent ID: {}", offline_agent.id());

    // Send messages to offline agent
    println!("\nSending messages to offline agent...");
    for i in 1..=3 {
        let msg = Message::new(
            sender.id(),
            offline_agent.id(),
            format!("Queued message #{}", i).into_bytes(),
        );
        bus.send(msg).await.unwrap();
        println!("Queued message #{}", i);
    }

    // Now bring the agent online
    println!("\nBringing agent online...");
    let mut offline_mailbox = bus.register(offline_agent.id()).await;

    // Agent should receive all queued messages
    println!("Receiving queued messages:");
    for i in 1..=3 {
        if let Some(msg) = offline_mailbox
            .recv_timeout(Duration::from_millis(100))
            .await
        {
            println!(
                "  Message {}: \"{}\"",
                i,
                String::from_utf8_lossy(&msg.payload)
            );
        }
    }

    println!("\n=== All examples completed ===");
}
