//! Cross-Platform Networking Tests
//!
//! Validates address handling, public-type address round-trips, and live loopback
//! lifecycle tests for mielin-mesh-core across IPv4 and IPv6 environments.
//!
//! # Groups
//! - Group 1: Synchronous `SocketAddr` parsing and display round-trips
//! - Group 2: Synchronous crate-type address round-trips (StaticPeer, AgentLocation, etc.)
//! - Group 3: Async loopback / mDNS lifecycle tests

use mielin_mesh_core::{
    AgentLocation, DnsSrvConfig, Node, NodeRole, ServiceEndpoint, StaticPeer,
};
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;

// ============================================================================
// Group 1: Address-handling tests (synchronous)
// ============================================================================

/// Parse a canonical IPv4 loopback address and verify type and port.
#[test]
fn parse_ipv4_loopback_socketaddr() {
    let addr: SocketAddr = "127.0.0.1:8080".parse().expect("valid IPv4 socket address");
    assert!(addr.is_ipv4(), "expected IPv4 address");
    assert_eq!(addr.port(), 8080, "expected port 8080");
}

/// Parse a canonical IPv6 loopback address and verify type and IP.
#[test]
fn parse_ipv6_loopback_socketaddr() {
    let addr: SocketAddr = "[::1]:8080".parse().expect("valid IPv6 socket address");
    assert!(addr.is_ipv6(), "expected IPv6 address");
    if let SocketAddr::V6(v6) = addr {
        assert_eq!(*v6.ip(), Ipv6Addr::LOCALHOST, "expected IPv6 loopback");
    }
}

/// Verify that `Display` to `parse` round-trips are identity for both IPv4 and IPv6.
#[test]
fn socketaddr_display_roundtrip_v4_v6() {
    let v4: SocketAddr = "127.0.0.1:9000"
        .parse()
        .expect("valid IPv4 socket address");
    let v4_rt: SocketAddr = v4
        .to_string()
        .parse()
        .expect("IPv4 display string must re-parse");
    assert_eq!(v4, v4_rt, "IPv4 round-trip must be identity");

    let v6: SocketAddr = "[::1]:9000".parse().expect("valid IPv6 socket address");
    let v6_rt: SocketAddr = v6
        .to_string()
        .parse()
        .expect("IPv6 display string must re-parse");
    assert_eq!(v6, v6_rt, "IPv6 round-trip must be identity");
}

/// Ephemeral port 0 is preserved as-is before binding.
#[test]
fn ephemeral_port_zero_is_zero_before_bind() {
    let v4: SocketAddr = "0.0.0.0:0"
        .parse()
        .expect("valid wildcard IPv4 socket address");
    assert_eq!(v4.port(), 0, "unbound IPv4 wildcard must have port 0");

    let v6: SocketAddr = "[::]:0"
        .parse()
        .expect("valid wildcard IPv6 socket address");
    assert_eq!(v6.port(), 0, "unbound IPv6 wildcard must have port 0");
}

// ============================================================================
// Group 2: Public-type address round-trips (synchronous)
// ============================================================================

/// `StaticPeer` stores and retrieves an IPv4 address without corruption.
#[test]
fn static_peer_accepts_ipv4_address() {
    let node = Node::new(NodeRole::Edge);
    let node_id = *node.id();
    let ipv4_addr: SocketAddr = "127.0.0.1:7001"
        .parse()
        .expect("valid IPv4 socket address");

    let peer = StaticPeer::new(node_id, ipv4_addr);
    assert_eq!(peer.address, ipv4_addr, "StaticPeer must preserve IPv4 address");
}

/// `StaticPeer` stores and retrieves an IPv6 address without corruption.
#[test]
fn static_peer_accepts_ipv6_address() {
    let node = Node::new(NodeRole::Edge);
    let node_id = *node.id();
    let ipv6_addr: SocketAddr = "[::1]:7002"
        .parse()
        .expect("valid IPv6 socket address");

    let peer = StaticPeer::new(node_id, ipv6_addr);
    assert_eq!(peer.address, ipv6_addr, "StaticPeer must preserve IPv6 address");
}

/// `AgentLocation` stores and retrieves an IPv6 address correctly.
#[test]
fn agent_location_stores_and_retrieves_ipv6() {
    let node = Node::new(NodeRole::Core);
    let node_id = *node.id();
    let agent_id: [u8; 16] = [0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    let ipv6_addr: SocketAddr = "[::1]:9001"
        .parse()
        .expect("valid IPv6 socket address");

    let location = AgentLocation::new(agent_id, node_id, ipv6_addr);
    assert_eq!(
        location.address, ipv6_addr,
        "AgentLocation must preserve IPv6 address"
    );
}

/// `ServiceEndpoint` preserves both the IPv6 address and the TLS flag after `.with_tls()`.
#[test]
fn service_endpoint_preserves_ipv6_and_tls_flag() {
    let ipv6_addr: SocketAddr = "[::1]:8443"
        .parse()
        .expect("valid IPv6 socket address");

    let endpoint = ServiceEndpoint::new(ipv6_addr, "tcp".to_string()).with_tls();
    assert_eq!(
        endpoint.address, ipv6_addr,
        "ServiceEndpoint must preserve IPv6 address"
    );
    assert!(endpoint.tls, "ServiceEndpoint::with_tls() must set tls = true");
}

/// `DnsSrvConfig::with_resolver` stores an IPv6 resolver address in `resolver_addr`.
///
/// `DnsSrvConfig` exposes `resolver_addr: Option<SocketAddr>` as a public field.
/// `with_resolver(addr: SocketAddr) -> Self` is a builder that sets that field.
#[test]
fn dns_srv_config_stores_ipv6_resolver() {
    let resolver_addr: SocketAddr = "[::1]:53"
        .parse()
        .expect("valid IPv6 DNS resolver address");

    let config = DnsSrvConfig::new("_mesh._tcp", "local.").with_resolver(resolver_addr);

    assert_eq!(
        config.resolver_addr,
        Some(resolver_addr),
        "DnsSrvConfig must store IPv6 resolver in resolver_addr field"
    );
}

// ============================================================================
// Group 3: Live loopback / mDNS lifecycle (async)
// ============================================================================

/// `MeshService` construction with an ephemeral loopback IPv4 bind address succeeds
/// and `bind_address()` reports an IPv4 address.
///
/// Note: `.start()` / `.stop()` are intentionally omitted here because mDNS
/// registration on loopback can behave differently across platforms (macOS
/// vs Linux firewall rules). Construction alone exercises the path that matters
/// for cross-platform compatibility: address family selection and config wiring.
#[tokio::test]
async fn mesh_service_binds_ephemeral_loopback_v4() -> Result<(), Box<dyn std::error::Error>> {
    use mielin_mesh_core::{MeshConfig, MeshService};

    let node = Arc::new(Node::new(NodeRole::Edge));
    let bind_address: SocketAddr = "127.0.0.1:0"
        .parse()
        .expect("valid ephemeral IPv4 address");

    let config = MeshConfig {
        bind_address,
        bootstrap_nodes: vec![],
        enable_mdns: false,
        enable_gossip: false,
        enable_registry: false,
        enable_migration: false,
    };

    let service = MeshService::new(Arc::clone(&node), config)?;
    assert!(
        service.bind_address().is_ipv4(),
        "MeshService constructed with IPv4 address must report is_ipv4()"
    );
    Ok(())
}

/// `DiscoveryService` can be started and stopped on an ephemeral loopback port.
///
/// `DiscoveryError` is not re-exported from the crate's public API; therefore
/// the return type uses `Box<dyn std::error::Error>` to remain compatible.
#[tokio::test]
async fn discovery_service_mdns_start_stop_loopback() -> Result<(), Box<dyn std::error::Error>> {
    use mielin_mesh_core::DiscoveryService;

    let node = Arc::new(Node::new(NodeRole::Edge));
    let bind_addr: SocketAddr = "127.0.0.1:0"
        .parse()
        .expect("valid ephemeral IPv4 address");

    let mut svc = DiscoveryService::new(Arc::clone(&node), bind_addr)
        .map_err(|e| format!("DiscoveryService::new failed: {e:?}"))?;

    svc.start()
        .await
        .map_err(|e| format!("DiscoveryService::start failed: {e:?}"))?;

    svc.stop().await;

    Ok(())
}

/// Two `DiscoveryService` instances with ephemeral ports on loopback do not
/// collide and can each be started and stopped independently.
#[tokio::test]
async fn two_discovery_services_different_ports_no_collision(
) -> Result<(), Box<dyn std::error::Error>> {
    use mielin_mesh_core::DiscoveryService;

    let node_a = Arc::new(Node::new(NodeRole::Edge));
    let node_b = Arc::new(Node::new(NodeRole::Relay));

    let bind_a: SocketAddr = "127.0.0.1:0"
        .parse()
        .expect("valid ephemeral IPv4 address for node A");
    let bind_b: SocketAddr = "127.0.0.1:0"
        .parse()
        .expect("valid ephemeral IPv4 address for node B");

    let mut svc_a = DiscoveryService::new(Arc::clone(&node_a), bind_a)
        .map_err(|e| format!("DiscoveryService A::new failed: {e:?}"))?;

    let mut svc_b = DiscoveryService::new(Arc::clone(&node_b), bind_b)
        .map_err(|e| format!("DiscoveryService B::new failed: {e:?}"))?;

    svc_a
        .start()
        .await
        .map_err(|e| format!("DiscoveryService A::start failed: {e:?}"))?;

    svc_b
        .start()
        .await
        .map_err(|e| format!("DiscoveryService B::start failed: {e:?}"))?;

    svc_a.stop().await;
    svc_b.stop().await;

    Ok(())
}
