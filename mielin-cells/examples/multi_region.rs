//! Multi-Region Deployment Example
//!
//! Demonstrates global agent deployment with:
//! - Regional agent deployment
//! - Geo-routing and latency optimization
//! - Cross-region synchronization
//! - Data sovereignty compliance

use mielin_cells::{
    Agent, GeoRouter, LatencyMap, Region, RegionConfig, RegionManager, RoutingPolicy,
    RoutingStrategy, SyncConfig, SyncManager,
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Region Deployment Example ===\n");

    // 1. Setup region manager
    println!("1. Setting up global region manager...");
    let region_manager = RegionManager::new();
    println!("   ✓ Region manager initialized");
    println!();

    // 2. Define global regions
    println!("2. Defining geographic regions...");

    let regions = vec![
        RegionConfig {
            region: Region {
                id: "us-west-2".to_string(),
                name: "US West (Oregon)".to_string(),
                location: mielin_cells::multiregion::deployment::Location {
                    latitude: 45_523_000,    // 45.523°N
                    longitude: -122_676_000, // -122.676°W
                },
                data_sovereignty_rules: vec!["US".to_string()],
            },
            capacity: 1000,
            priority: 1,
        },
        RegionConfig {
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
        },
        RegionConfig {
            region: Region {
                id: "eu-west-1".to_string(),
                name: "EU West (Ireland)".to_string(),
                location: mielin_cells::multiregion::deployment::Location {
                    latitude: 53_350_000,  // 53.35°N
                    longitude: -6_260_000, // -6.26°W
                },
                data_sovereignty_rules: vec!["EU".to_string(), "GDPR".to_string()],
            },
            capacity: 800,
            priority: 1,
        },
        RegionConfig {
            region: Region {
                id: "ap-northeast-1".to_string(),
                name: "Asia Pacific (Tokyo)".to_string(),
                location: mielin_cells::multiregion::deployment::Location {
                    latitude: 35_689_000,   // 35.689°N
                    longitude: 139_692_000, // 139.692°E
                },
                data_sovereignty_rules: vec!["JP".to_string()],
            },
            capacity: 600,
            priority: 1,
        },
    ];

    for region_config in regions {
        region_manager.add_region(region_config.clone())?;
        println!(
            "   ✓ Region added: {} ({})",
            region_config.region.name, region_config.region.id
        );
        println!(
            "     Capacity: {}, Priority: {}",
            region_config.capacity, region_config.priority
        );
        println!(
            "     Sovereignty: {:?}",
            region_config.region.data_sovereignty_rules
        );
    }
    println!();

    // 3. Deploy agents to regions
    println!("3. Deploying agents globally...");

    let us_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let eu_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let ap_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    region_manager.deploy_to_region("us-west-2", us_agent.id())?;
    region_manager.deploy_to_region("eu-west-1", eu_agent.id())?;
    region_manager.deploy_to_region("ap-northeast-1", ap_agent.id())?;

    println!("   ✓ Agent deployed to US West: {}", us_agent.id());
    println!("   ✓ Agent deployed to EU West: {}", eu_agent.id());
    println!("   ✓ Agent deployed to AP Northeast: {}", ap_agent.id());
    println!();

    // 4. Configure geo-routing with latency map
    println!("4. Configuring intelligent geo-routing...");

    let mut latency_map = LatencyMap::new();

    // US West to other regions
    latency_map.add_latency(
        "us-west-2".to_string(),
        "us-east-1".to_string(),
        Duration::from_millis(80),
    );
    latency_map.add_latency(
        "us-west-2".to_string(),
        "eu-west-1".to_string(),
        Duration::from_millis(150),
    );
    latency_map.add_latency(
        "us-west-2".to_string(),
        "ap-northeast-1".to_string(),
        Duration::from_millis(120),
    );

    // US East to other regions
    latency_map.add_latency(
        "us-east-1".to_string(),
        "eu-west-1".to_string(),
        Duration::from_millis(90),
    );
    latency_map.add_latency(
        "us-east-1".to_string(),
        "ap-northeast-1".to_string(),
        Duration::from_millis(200),
    );

    // EU to AP
    latency_map.add_latency(
        "eu-west-1".to_string(),
        "ap-northeast-1".to_string(),
        Duration::from_millis(230),
    );

    let routing_policy = RoutingPolicy {
        strategy: RoutingStrategy::LatencyBased,
        fallback_regions: vec!["us-east-1".to_string()],
    };

    let mut router = GeoRouter::new(routing_policy);
    router.update_latency_map(latency_map);

    println!("   ✓ Geo-routing configured");
    println!("   Strategy: Latency-based");
    println!("   Fallback region: us-east-1");
    println!();

    // 5. Perform routing decisions
    println!("5. Making routing decisions...");

    let available_regions = vec![
        "us-east-1".to_string(),
        "eu-west-1".to_string(),
        "ap-northeast-1".to_string(),
    ];

    let decision = router.route("us-west-2", &available_regions);

    println!("   ✓ Routing from us-west-2:");
    println!("     Target region: {}", decision.target_region);
    println!("     Reason: {}", decision.reason);
    println!();

    // 6. Configure cross-region synchronization
    println!("6. Configuring cross-region sync...");

    let sync_config = SyncConfig {
        sync_interval: Duration::from_secs(60),
        conflict_resolution: mielin_cells::multiregion::sync::ConflictResolution::LastWriteWins,
    };

    let sync_manager = SyncManager::new(sync_config);

    println!("   ✓ Sync manager configured");
    println!("   Interval: 60 seconds");
    println!("   Conflict resolution: LastWriteWins");
    println!();

    // 7. Synchronize agents across regions
    println!("7. Synchronizing agents across regions...");

    let sync_result = sync_manager.sync_agent(&us_agent.id(), "us-west-2", "us-east-1")?;

    println!(
        "   ✓ Sync completed: {} -> {}",
        sync_result.source_region, sync_result.target_region
    );
    println!("   Status: {:?}", sync_result.status);
    println!("   Agent: {}", sync_result.agent_id);
    println!();

    // 8. Demonstrate data sovereignty compliance
    println!("8. Verifying data sovereignty compliance...");

    let all_regions = region_manager.list_regions()?;

    for region in &all_regions {
        println!("   Region: {}", region.name);
        println!("     ID: {}", region.id);
        println!(
            "     Sovereignty rules: {:?}",
            region.data_sovereignty_rules
        );

        if region.data_sovereignty_rules.contains(&"GDPR".to_string()) {
            println!("     ⚠️  GDPR compliance required");
        }
        if region.data_sovereignty_rules.contains(&"US".to_string()) {
            println!("     ℹ️  US data residency enforced");
        }
    }
    println!();

    // 9. Display deployment summary
    println!("9. Deployment summary...");

    for region in &all_regions {
        if let Ok(deployment) = region_manager.get_region_deployment(&region.id) {
            println!("   {} ({}):", region.name, region.id);
            println!("     Deployed agents: {}", deployment.agent_ids.len());
            println!("     Status: {:?}", deployment.status);
        }
    }

    println!("\n=== Multi-Region Deployment Complete ===");
    println!("\nKey Features Demonstrated:");
    println!("  ✓ Global region management");
    println!("  ✓ Latency-based routing");
    println!("  ✓ Cross-region synchronization");
    println!("  ✓ Data sovereignty compliance");
    println!("  ✓ Regional failover support");

    Ok(())
}
