//! Cross-platform connectivity tests for mielin-mesh-wire
//!
//! These tests exercise actual QUIC connections over loopback.
//! They cover both IPv4 and IPv6 (the IPv6 test is marked `#[ignore]`
//! since some CI environments lack IPv6 loopback support).

use mielin_mesh_wire::{certs::CertManager, transport::QuicTransport, Message};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::time::timeout;

#[tokio::test]
async fn ipv4_loopback_handshake_and_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    timeout(Duration::from_secs(5), async {
        let server_addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let server = QuicTransport::new(server_addr).await.expect("server creation");
        let local_addr = server.local_addr().expect("local_addr");

        let server_task = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept");
            conn.receive().await.expect("receive")
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = QuicTransport::new_client().await.expect("client creation");
        let conn = client.connect(local_addr).await.expect("connect");
        conn.send(&Message::Ping { timestamp: 42 })
            .await
            .expect("send");

        let received = server_task.await.expect("server task");
        assert!(matches!(received, Message::Ping { timestamp: 42 }));

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "timed out")?
}

/// Validates that multiple independent server+client pairs operate correctly
/// when their I/O tasks interleave on a multi-threaded executor.
///
/// Uses `flavor = "multi_thread"` because `ConnectionDriver::run_handshake`
/// drives the UDP socket inline; in a single-threaded executor the server
/// accept task and the client connect task cannot make progress simultaneously.
///
/// The client sleeps 200 ms after connecting before sending its Ping. This
/// ensures the server has called `accept()` → `into_driven()` so that the
/// `run_driven_connection` background task is active before stream data
/// arrives. Without this pause, the stream open+data is processed by the
/// handshake task and sits in the `Connection` state machine; because
/// `run_driven_connection` only surfaces `poll_new_peer_stream` /
/// `poll_readable` events after receiving a new datagram, the server's
/// `receive()` call blocks until the 15-second keep-alive fires.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ipv4_concurrent_connections() -> Result<(), Box<dyn std::error::Error>> {
    timeout(Duration::from_secs(10), async {
        const PAIRS: usize = 4;

        // Create all server transports and spawn their accept+receive tasks
        // before starting any clients, so every server is ready to accept.
        let mut server_tasks = Vec::with_capacity(PAIRS);
        let mut server_addrs = Vec::with_capacity(PAIRS);

        for i in 0..PAIRS {
            let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
            let server = QuicTransport::new(addr).await.expect("server");
            let server_addr = server.local_addr().expect("local_addr");
            server_addrs.push(server_addr);
            server_tasks.push(tokio::spawn(async move {
                let conn = server.accept().await.expect("accept");
                let msg = conn.receive().await.expect("receive");
                (i, msg)
            }));
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client_futs = Vec::with_capacity(PAIRS);
        for (i, &addr) in server_addrs.iter().enumerate() {
            let ts = i as u64;
            client_futs.push(tokio::spawn(async move {
                let client = QuicTransport::new_client().await.expect("client");
                let conn = client.connect(addr).await.expect("connect");
                // Allow 200 ms for the server to complete accept() → into_driven()
                // before the client opens its send stream.
                tokio::time::sleep(Duration::from_millis(200)).await;
                conn.send(&Message::Ping { timestamp: ts })
                    .await
                    .expect("send");
            }));
        }

        for f in client_futs {
            f.await.expect("client task");
        }
        let results: Vec<(usize, Message)> = futures::future::join_all(server_tasks)
            .await
            .into_iter()
            .map(|r| r.expect("server task"))
            .collect();

        assert_eq!(results.len(), PAIRS);
        for (i, msg) in &results {
            assert!(
                matches!(msg, Message::Ping { timestamp: ts } if *ts == *i as u64),
                "server {i} got unexpected message: {msg:?}",
            );
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "timed out")?
}

#[tokio::test]
async fn graceful_close_marks_connection_closed() -> Result<(), Box<dyn std::error::Error>> {
    timeout(Duration::from_secs(5), async {
        let server_addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let server = QuicTransport::new(server_addr).await.expect("server creation");
        let local_addr = server.local_addr().expect("local_addr");

        let _server_task = tokio::spawn(async move {
            let _conn = server.accept().await.expect("accept");
            // Hold conn open — no receive needed
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = QuicTransport::new_client().await.expect("client creation");
        let conn = client.connect(local_addr).await.expect("connect");

        conn.close();

        // close() spawns a background task; give it time to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(conn.is_closed());

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "timed out")?
}

#[tokio::test]
async fn connect_to_nonexistent_port_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    // Bind a TCP listener to get a free port, then immediately drop it so nothing listens there.
    let port = {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind temp listener");
        listener
            .local_addr()
            .expect("temp listener local_addr")
            .port()
    };

    // QuicTransport::connect has an internal 10-second timeout, so we give 12 seconds here.
    let result = timeout(Duration::from_secs(12), async {
        let client = QuicTransport::new_client().await.expect("client creation");
        client
            .connect(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
    })
    .await
    .map_err(|_| -> Box<dyn std::error::Error> { "timed out".into() })?;

    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn cert_manager_roundtrip_loopback() -> Result<(), Box<dyn std::error::Error>> {
    timeout(Duration::from_secs(5), async {
        let server_addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let server = QuicTransport::new_with_certs(
            server_addr,
            "test-node",
            Arc::new(CertManager::new()),
        )
        .await
        .expect("server with certs");
        let local_addr = server.local_addr().expect("local_addr");

        let server_task = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept");
            conn.receive().await.expect("receive")
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = QuicTransport::new_client().await.expect("client creation");
        let conn = client.connect(local_addr).await.expect("connect");
        conn.send(&Message::Ping { timestamp: 99 })
            .await
            .expect("send");

        let received = server_task.await.expect("server task");
        assert!(matches!(received, Message::Ping { timestamp: 99 }));

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "timed out")?
}

// Requires IPv6 loopback (::1) — skipped in CI without IPv6 support
#[ignore]
#[tokio::test]
async fn ipv6_loopback_handshake_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    timeout(Duration::from_secs(5), async {
        let server_addr: SocketAddr = "[::1]:0".parse().expect("IPv6 addr must parse");
        let server = QuicTransport::new(server_addr).await.expect("server creation");
        let local_addr = server.local_addr().expect("local_addr");

        let server_task = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept");
            conn.receive().await.expect("receive")
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = QuicTransport::new_client_for(local_addr)
            .await
            .expect("client creation for IPv6");
        let conn = client.connect(local_addr).await.expect("connect");
        conn.send(&Message::Ping { timestamp: 7 })
            .await
            .expect("send");

        let received = server_task.await.expect("server task");
        assert!(matches!(received, Message::Ping { timestamp: 7 }));

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "timed out")?
}

#[tokio::test]
async fn connect_with_retry_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    timeout(Duration::from_secs(10), async {
        let server_addr: SocketAddr = "127.0.0.1:0".parse().expect("addr must parse");
        let server = QuicTransport::new(server_addr).await.expect("server creation");
        let local_addr = server.local_addr().expect("local_addr");

        let server_task = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept");
            conn.receive().await.expect("receive")
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = QuicTransport::new_client().await.expect("client creation");
        let conn = client
            .connect_with_retry(local_addr, 3)
            .await
            .expect("connect_with_retry");
        conn.send(&Message::Ping { timestamp: 55 })
            .await
            .expect("send");

        let received = server_task.await.expect("server task");
        assert!(matches!(received, Message::Ping { timestamp: 55 }));

        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "timed out")?
}
