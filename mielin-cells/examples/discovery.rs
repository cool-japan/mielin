//! Agent Discovery and Load Balancing Example
//!
//! Demonstrates:
//! - Service registration and discovery
//! - Capability-based discovery
//! - Location-aware routing
//! - Health checking and monitoring
//! - Load balancing strategies

use mielin_cells::{
    AgentId, Capability, DiscoveryHealthStatus as HealthStatus, DiscoveryLocation as Location,
    DiscoveryQuery, HealthCheck, LoadBalancer, LoadBalancingStrategy, ServiceRegistration,
    ServiceRegistry, Version,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Agent Discovery and Load Balancing Example ===\n");

    // Create service registry
    let registry = Arc::new(ServiceRegistry::new());

    // Register services
    println!("--- Service Registration ---");
    register_services(&registry)?;

    // Demonstrate service discovery
    println!("\n--- Service Discovery ---");
    discovery_examples(&registry)?;

    // Demonstrate capability-based discovery
    println!("\n--- Capability-Based Discovery ---");
    capability_discovery(&registry)?;

    // Demonstrate location-aware routing
    println!("\n--- Location-Aware Routing ---");
    location_aware_routing(&registry)?;

    // Demonstrate health checking
    println!("\n--- Health Checking ---");
    health_checking_example(&registry)?;

    // Demonstrate load balancing
    println!("\n--- Load Balancing Strategies ---");
    load_balancing_examples(&registry)?;

    println!("\n=== Example Complete ===");
    Ok(())
}

fn register_services(registry: &ServiceRegistry) -> Result<(), Box<dyn std::error::Error>> {
    println!("Registering services across multiple locations...\n");

    // Register API services in different regions
    let tokyo = Location::new(35.6762, 139.6503);
    let osaka = Location::new(34.6937, 135.5023);
    let new_york = Location::new(40.7128, -74.0060);
    let london = Location::new(51.5074, -0.1278);

    // Tokyo API servers
    for i in 1..=3 {
        let agent_id = AgentId::new_v4();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 1, i)), 8080);

        let registration =
            ServiceRegistration::new(agent_id, "api-service", Version::new(1, 0, 0), addr)
                .with_location(tokyo)
                .with_capability(Capability::new("http", Version::new(1, 1, 0)))
                .with_capability(Capability::new("json", Version::new(1, 0, 0)))
                .with_metadata("region", "asia-northeast-1")
                .with_metadata("datacenter", "tokyo");

        registry.register(registration)?;
        println!("  ✓ Registered API server {} in Tokyo", i);
    }

    // Osaka API servers
    for i in 1..=2 {
        let agent_id = AgentId::new_v4();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 2, i)), 8080);

        let registration =
            ServiceRegistration::new(agent_id, "api-service", Version::new(1, 0, 0), addr)
                .with_location(osaka)
                .with_capability(Capability::new("http", Version::new(1, 1, 0)))
                .with_capability(Capability::new("json", Version::new(1, 0, 0)))
                .with_metadata("region", "asia-northeast-2")
                .with_metadata("datacenter", "osaka");

        registry.register(registration)?;
        println!("  ✓ Registered API server {} in Osaka", i);
    }

    // Register image processing service in New York
    let agent_id = AgentId::new_v4();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 1, 1, 1)), 8081);
    let registration =
        ServiceRegistration::new(agent_id, "image-processing", Version::new(2, 0, 0), addr)
            .with_location(new_york)
            .with_capability(Capability::new("image-resize", Version::new(1, 0, 0)))
            .with_capability(Capability::new("image-format", Version::new(1, 0, 0)))
            .with_metadata("region", "us-east-1")
            .with_metadata("datacenter", "new-york");

    registry.register(registration)?;
    println!("  ✓ Registered image processing service in New York");

    // Register analytics service in London
    let agent_id = AgentId::new_v4();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 2, 1, 1)), 8082);
    let registration =
        ServiceRegistration::new(agent_id, "analytics-service", Version::new(1, 0, 0), addr)
            .with_location(london)
            .with_capability(Capability::new("data-aggregation", Version::new(1, 0, 0)))
            .with_capability(Capability::new("reporting", Version::new(1, 0, 0)))
            .with_metadata("region", "eu-west-1")
            .with_metadata("datacenter", "london");

    registry.register(registration)?;
    println!("  ✓ Registered analytics service in London");

    Ok(())
}

fn discovery_examples(registry: &ServiceRegistry) -> Result<(), Box<dyn std::error::Error>> {
    // List all services
    let services = registry.list_services();
    println!("Registered services:");
    for service in &services {
        let agents = registry.get_service(service);
        println!("  {} - {} instances", service, agents.len());
    }

    // Discover all API services
    println!("\nDiscovering all API services:");
    let query = DiscoveryQuery::new().service("api-service");
    let results = registry.discover(query);

    println!("  Found {} API service instances:", results.len());
    for (i, result) in results.iter().enumerate() {
        println!(
            "    {}. {} at {} (score: {:.2})",
            i + 1,
            result.registration.agent_id,
            result.registration.address,
            result.score
        );
    }

    Ok(())
}

fn capability_discovery(registry: &ServiceRegistry) -> Result<(), Box<dyn std::error::Error>> {
    println!("Finding services with image processing capabilities:");

    let query =
        DiscoveryQuery::new().capability(Capability::new("image-resize", Version::new(1, 0, 0)));

    let results = registry.discover(query);

    println!(
        "  Found {} services with image-resize capability:",
        results.len()
    );
    for result in results {
        println!(
            "    Service: {} at {}",
            result.registration.service_name, result.registration.address
        );
        println!("    Capabilities:");
        for cap in &result.registration.capabilities {
            println!("      - {} v{}", cap.name, cap.version);
        }
    }

    Ok(())
}

fn location_aware_routing(registry: &ServiceRegistry) -> Result<(), Box<dyn std::error::Error>> {
    // Client location in Tokyo
    let client_location = Location::new(35.7, 139.7);

    println!("Client location: Tokyo (35.7°N, 139.7°E)");
    println!("\nFinding API services within 100km:");

    let query = DiscoveryQuery::new()
        .service("api-service")
        .near(client_location, 100.0);

    let results = registry.discover(query);

    println!("  Found {} nearby services:", results.len());
    for result in results {
        if let (Some(distance), Some(loc)) = (result.distance_km, result.registration.location) {
            println!(
                "    {} at ({:.2}°N, {:.2}°E) - {:.1}km away",
                result.registration.address, loc.latitude, loc.longitude, distance
            );
        }
    }

    // Find services in all locations sorted by distance
    println!("\nAll API services sorted by distance:");
    let query = DiscoveryQuery::new()
        .service("api-service")
        .near(client_location, f64::MAX);

    let results = registry.discover(query);

    for (i, result) in results.iter().enumerate() {
        if let (Some(distance), Some(_loc)) = (result.distance_km, result.registration.location) {
            let unknown = "unknown".to_string();
            let datacenter = result
                .registration
                .metadata
                .get("datacenter")
                .unwrap_or(&unknown);
            println!(
                "    {}. {} - {:.0}km away (score: {:.2})",
                i + 1,
                datacenter,
                distance,
                result.score
            );
        }
    }

    Ok(())
}

fn health_checking_example(registry: &ServiceRegistry) -> Result<(), Box<dyn std::error::Error>> {
    // Get all API service agents
    let api_agents = registry.get_service("api-service");

    println!("Simulating health checks on {} agents...", api_agents.len());

    // Simulate health checks
    for (i, agent_id) in api_agents.iter().enumerate() {
        let health = if i % 3 == 0 {
            // Every third agent is degraded
            HealthCheck::degraded("High memory usage")
                .with_metric("cpu_usage", 45.0)
                .with_metric("memory_usage", 85.0)
        } else if i % 7 == 0 {
            // Every seventh agent is unhealthy
            HealthCheck::unhealthy("Connection timeout")
                .with_metric("cpu_usage", 95.0)
                .with_metric("memory_usage", 95.0)
        } else {
            HealthCheck::healthy()
                .with_metric("cpu_usage", 25.0)
                .with_metric("memory_usage", 50.0)
        };

        registry.update_health(agent_id, health)?;
    }

    // Query for healthy services only
    println!("\nQuerying for healthy API services:");
    let query = DiscoveryQuery::new().service("api-service").healthy();

    let results = registry.discover(query);
    println!("  Found {} healthy instances", results.len());

    // Show health status of all instances
    println!("\nHealth status summary:");
    for agent_id in &api_agents {
        let health = registry.get_health(agent_id)?;
        let status_symbol = match health.status {
            HealthStatus::Healthy => "✓",
            HealthStatus::Degraded => "⚠",
            HealthStatus::Unhealthy => "✗",
            HealthStatus::Unknown => "?",
        };

        println!(
            "  {} Agent: {:?} - {:?}",
            status_symbol, agent_id, health.status
        );
        if let Some(msg) = health.message {
            println!("     Message: {}", msg);
        }
    }

    Ok(())
}

fn load_balancing_examples(registry: &ServiceRegistry) -> Result<(), Box<dyn std::error::Error>> {
    // Create a copy of registry data for load balancing
    let registry_arc = Arc::new(ServiceRegistry::new());

    // Re-register services from the original registry
    for service_name in registry.list_services() {
        for agent_id in registry.get_service(&service_name) {
            if let Ok(reg) = registry.get_registration(&agent_id) {
                registry_arc.register(reg)?;
            }
        }
    }

    // Round Robin
    println!("1. Round Robin Load Balancing:");
    let lb = LoadBalancer::new(registry_arc.clone(), LoadBalancingStrategy::RoundRobin);

    print!("  Requests: ");
    for i in 0..10 {
        let agent = lb.select("api-service", None)?;
        if i > 0 {
            print!(", ");
        }
        print!(
            "{}",
            format!("{:?}", agent).chars().take(8).collect::<String>()
        );
    }
    println!();

    // Random
    println!("\n2. Random Load Balancing:");
    let lb = LoadBalancer::new(registry_arc.clone(), LoadBalancingStrategy::Random);

    print!("  Requests: ");
    for i in 0..10 {
        let agent = lb.select("api-service", None)?;
        if i > 0 {
            print!(", ");
        }
        print!(
            "{}",
            format!("{:?}", agent).chars().take(8).collect::<String>()
        );
    }
    println!();

    // Least Connections
    println!("\n3. Least Connections Load Balancing:");
    let lb = LoadBalancer::new(
        registry_arc.clone(),
        LoadBalancingStrategy::LeastConnections,
    );

    println!("  Simulating concurrent connections:");
    for i in 1..=5 {
        let agent = lb.select("api-service", None)?;
        println!(
            "    Request {}: {} (acquiring connection)",
            i,
            format!("{:?}", agent).chars().take(8).collect::<String>()
        );

        // Simulate connection held for different durations
        if i % 2 == 0 {
            thread::sleep(Duration::from_millis(10));
            lb.release(&agent);
            println!("      (connection released)");
        }
    }

    // Location-Based
    println!("\n4. Location-Based Load Balancing:");
    let lb = LoadBalancer::new(registry_arc.clone(), LoadBalancingStrategy::LocationBased);

    let tokyo = Location::new(35.6762, 139.6503);
    let london = Location::new(51.5074, -0.1278);

    println!("  Client in Tokyo:");
    let agent = lb.select("api-service", Some(tokyo))?;
    if let Ok(reg) = registry.get_registration(&agent) {
        let unknown = "unknown".to_string();
        let datacenter = reg.metadata.get("datacenter").unwrap_or(&unknown);
        println!("    → Routed to: {}", datacenter);
    }

    println!("  Client in London:");
    let agent = lb.select("api-service", Some(london))?;
    if let Ok(reg) = registry.get_registration(&agent) {
        let unknown = "unknown".to_string();
        let datacenter = reg.metadata.get("datacenter").unwrap_or(&unknown);
        println!("    → Routed to: {}", datacenter);
    }

    Ok(())
}
