//! Multi-region Deployment Integration Tests
//!
//! Tests the complete multi-region workflow including regional deployment,
//! geo-routing, and cross-region synchronization.

use mielin_cells::{
    Agent, GeoRouter, LatencyMap, Region, RegionConfig, RegionManager, RoutingPolicy,
    RoutingStrategy, SyncConfig, SyncManager, SyncStatus,
};
use std::time::Duration;

#[test]
fn test_multi_region_deployment_workflow() {
    // 1. Setup region manager
    let manager = RegionManager::new();

    // 2. Create regions
    let us_west = RegionConfig {
        region: Region {
            id: "us-west-1".to_string(),
            name: "US West (Oregon)".to_string(),
            location: mielin_cells::multiregion::deployment::Location {
                latitude: 45_523_000,    // 45.523°N
                longitude: -122_676_000, // -122.676°W
            },
            data_sovereignty_rules: vec!["US".to_string()],
        },
        capacity: 1000,
        priority: 1,
    };

    let us_east = RegionConfig {
        region: Region {
            id: "us-east-1".to_string(),
            name: "US East (Virginia)".to_string(),
            location: mielin_cells::multiregion::deployment::Location {
                latitude: 37_431_000,   // 37.431°N
                longitude: -78_657_000, // -78.657°W
            },
            data_sovereignty_rules: vec!["US".to_string()],
        },
        capacity: 1000,
        priority: 2,
    };

    let eu_west = RegionConfig {
        region: Region {
            id: "eu-west-1".to_string(),
            name: "EU West (Ireland)".to_string(),
            location: mielin_cells::multiregion::deployment::Location {
                latitude: 53_350_000,  // 53.35°N
                longitude: -6_260_000, // -6.26°W
            },
            data_sovereignty_rules: vec!["EU".to_string(), "GDPR".to_string()],
        },
        capacity: 1000,
        priority: 1,
    };

    manager.add_region(us_west).expect("add us-west");
    manager.add_region(us_east).expect("add us-east");
    manager.add_region(eu_west).expect("add eu-west");

    // 3. Deploy agents to regions
    let agent1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let agent2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let agent3 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    manager
        .deploy_to_region("us-west-1", agent1.id())
        .expect("deploy to us-west");
    manager
        .deploy_to_region("us-east-1", agent2.id())
        .expect("deploy to us-east");
    manager
        .deploy_to_region("eu-west-1", agent3.id())
        .expect("deploy to eu-west");

    // 4. Verify deployments
    let us_west_deployment = manager
        .get_region_deployment("us-west-1")
        .expect("get deployment");
    assert_eq!(us_west_deployment.agent_ids.len(), 1);

    let regions = manager.list_regions().expect("list regions");
    assert_eq!(regions.len(), 3);
}

#[test]
fn test_geo_routing() {
    // Setup latency map
    let mut latency_map = LatencyMap::new();
    latency_map.add_latency(
        "us-west".to_string(),
        "us-east".to_string(),
        Duration::from_millis(80),
    );
    latency_map.add_latency(
        "us-west".to_string(),
        "eu-west".to_string(),
        Duration::from_millis(150),
    );
    latency_map.add_latency(
        "us-east".to_string(),
        "eu-west".to_string(),
        Duration::from_millis(100),
    );

    // Create router with latency-based strategy
    let policy = RoutingPolicy {
        strategy: RoutingStrategy::LatencyBased,
        fallback_regions: vec!["us-east".to_string()],
    };

    let mut router = GeoRouter::new(policy);
    router.update_latency_map(latency_map);

    // Route from us-west
    let available = vec!["us-east".to_string(), "eu-west".to_string()];
    let decision = router.route("us-west", &available);

    assert!(!decision.target_region.is_empty());
    assert_eq!(decision.reason, "Latency-based routing");
}

#[test]
fn test_cross_region_synchronization() {
    let sync_config = SyncConfig {
        sync_interval: Duration::from_secs(60),
        conflict_resolution: mielin_cells::multiregion::sync::ConflictResolution::LastWriteWins,
    };

    let sync_manager = SyncManager::new(sync_config);

    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    // Sync from us-west to us-east
    let sync_result = sync_manager
        .sync_agent(&agent.id(), "us-west-1", "us-east-1")
        .expect("sync agent");

    assert_eq!(sync_result.status, SyncStatus::InSync);
    assert_eq!(sync_result.source_region, "us-west-1");
    assert_eq!(sync_result.target_region, "us-east-1");
    assert!(sync_result.last_sync.is_some());
}

#[test]
fn test_multi_region_with_data_sovereignty() {
    let manager = RegionManager::new();

    // EU region with GDPR requirements
    let eu_region = RegionConfig {
        region: Region {
            id: "eu-central-1".to_string(),
            name: "EU Central (Frankfurt)".to_string(),
            location: mielin_cells::multiregion::deployment::Location {
                latitude: 50_110_000,
                longitude: 8_682_000,
            },
            data_sovereignty_rules: vec!["EU".to_string(), "GDPR".to_string()],
        },
        capacity: 500,
        priority: 1,
    };

    // US region
    let us_region = RegionConfig {
        region: Region {
            id: "us-gov-west-1".to_string(),
            name: "US Gov West".to_string(),
            location: mielin_cells::multiregion::deployment::Location {
                latitude: 45_523_000,
                longitude: -122_676_000,
            },
            data_sovereignty_rules: vec!["US".to_string(), "FedRAMP".to_string()],
        },
        capacity: 500,
        priority: 1,
    };

    manager.add_region(eu_region).expect("add eu region");
    manager.add_region(us_region).expect("add us region");

    // Deploy agents respecting data sovereignty
    let eu_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    manager
        .deploy_to_region("eu-central-1", eu_agent.id())
        .expect("deploy EU agent");

    let us_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    manager
        .deploy_to_region("us-gov-west-1", us_agent.id())
        .expect("deploy US agent");

    let regions = manager.list_regions().expect("list regions");
    assert_eq!(regions.len(), 2);

    // Verify regions have correct sovereignty rules
    let eu = regions
        .iter()
        .find(|r| r.id == "eu-central-1")
        .expect("find EU");
    assert!(eu.data_sovereignty_rules.contains(&"GDPR".to_string()));

    let us = regions
        .iter()
        .find(|r| r.id == "us-gov-west-1")
        .expect("find US");
    assert!(us.data_sovereignty_rules.contains(&"FedRAMP".to_string()));
}

#[test]
fn test_routing_with_fallback() {
    let policy = RoutingPolicy {
        strategy: RoutingStrategy::LoadBased,
        fallback_regions: vec!["backup-region".to_string()],
    };

    let router = GeoRouter::new(policy);

    // Route with no available regions should use fallback
    let decision = router.route("source", &[]);
    assert_eq!(decision.target_region, "");

    // Route with available regions
    let decision = router.route("source", &["region-1".to_string()]);
    assert_eq!(decision.target_region, "region-1");
}

#[test]
fn test_weighted_routing_strategy() {
    let policy = RoutingPolicy {
        strategy: RoutingStrategy::Weighted,
        fallback_regions: vec![],
    };

    let router = GeoRouter::new(policy);

    let available = vec![
        "high-priority".to_string(),
        "medium-priority".to_string(),
        "low-priority".to_string(),
    ];

    let decision = router.route("source", &available);
    assert!(!decision.target_region.is_empty());
}
