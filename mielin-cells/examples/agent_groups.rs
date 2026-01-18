//! Agent Groups example
//!
//! Demonstrates:
//! - Creating and managing agent groups
//! - Different member roles (Member, Leader, Observer)
//! - Group lifecycle (Active, Paused, Dissolving, Dissolved)
//! - Hierarchical groups
//! - Group registry and tagging
//! - Shared policies
//! - Group constraints and limits

use mielin_cells::{Agent, AgentGroup, GroupConfig, GroupRegistry, GroupResult, GroupRole, Policy};

fn main() {
    println!("=== MielinOS Agent Groups ===\n");

    // Example 1: Basic group creation and membership
    basic_group_management();

    // Example 2: Group roles and constraints
    group_roles_and_constraints();

    // Example 3: Group lifecycle
    group_lifecycle();

    // Example 4: Shared policies
    shared_policies();

    // Example 5: Hierarchical groups
    hierarchical_groups();

    // Example 6: Group registry and discovery
    group_registry_usage();

    // Example 7: Tags and categorization
    tags_and_categorization();
}

/// Example 1: Basic group creation and membership
fn basic_group_management() {
    println!("1. Basic Group Management");
    println!("-------------------------");

    // Create agents
    let agent1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let agent2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let agent3 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    println!("Created agents:");
    println!("  Agent 1: {}", agent1.id());
    println!("  Agent 2: {}", agent2.id());
    println!("  Agent 3: {}", agent3.id());

    // Create a group
    let group = AgentGroup::new("backend-services");
    println!("\nCreated group: {} (ID: {})", group.name(), group.id());
    println!("Group state: {:?}", group.state());

    // Add members
    println!("\nAdding members to group:");
    group.add_member(agent1.id(), GroupRole::Leader).unwrap();
    println!("  Added Agent 1 as Leader");

    group.add_member(agent2.id(), GroupRole::Member).unwrap();
    println!("  Added Agent 2 as Member");

    group.add_member(agent3.id(), GroupRole::Observer).unwrap();
    println!("  Added Agent 3 as Observer");

    // Check membership
    println!("\nGroup has {} members", group.member_count());

    for member in group.members() {
        println!(
            "  Agent {}: {:?} (joined at: {})",
            member.agent_id, member.role, member.joined_at
        );
    }

    // Check specific member
    if group.is_member(&agent1.id()) {
        println!("\nAgent 1 is a member of the group");
    }

    println!();
}

/// Example 2: Group roles and constraints
fn group_roles_and_constraints() {
    println!("2. Group Roles and Constraints");
    println!("-------------------------------");

    // Create group with constraints
    let config = GroupConfig {
        max_members: 5,
        min_leaders: 1,
        max_leaders: 2,
        enforce_shared_policy: true,
        allow_voluntary_leave: true,
    };

    let group = AgentGroup::with_config("database-cluster", config);
    println!("Created group with constraints:");
    println!("  Max members: {}", group.config().max_members);
    println!("  Min leaders: {}", group.config().min_leaders);
    println!("  Max leaders: {}", group.config().max_leaders);

    // Add leaders
    let leader1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let leader2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    group.add_member(leader1.id(), GroupRole::Leader).unwrap();
    group.add_member(leader2.id(), GroupRole::Leader).unwrap();
    println!("\nAdded 2 leaders");
    println!("Leader count: {}", group.leader_count());

    // Try to add third leader (should fail)
    let leader3 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    match group.add_member(leader3.id(), GroupRole::Leader) {
        GroupResult::Success(_) => println!("Successfully added third leader"),
        GroupResult::Error(_) => println!("Cannot add third leader (max leaders constraint)"),
    }

    // Add regular members
    let member1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let member2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    group.add_member(member1.id(), GroupRole::Member).unwrap();
    group.add_member(member2.id(), GroupRole::Member).unwrap();

    println!("\nGroup composition:");
    let leaders = group.members_by_role(GroupRole::Leader);
    let members = group.members_by_role(GroupRole::Member);
    let observers = group.members_by_role(GroupRole::Observer);

    println!("  Leaders: {}", leaders.len());
    println!("  Members: {}", members.len());
    println!("  Observers: {}", observers.len());

    // Change role
    println!("\nPromoting member to leader...");
    match group.change_role(&member1.id(), GroupRole::Leader) {
        GroupResult::Success(_) => println!("Successfully promoted member"),
        GroupResult::Error(_) => println!("Cannot promote (max leaders constraint)"),
    }

    println!();
}

/// Example 3: Group lifecycle
fn group_lifecycle() {
    println!("3. Group Lifecycle");
    println!("------------------");

    let group = AgentGroup::new("worker-pool");
    println!("Created group: {}", group.name());
    println!("Initial state: {:?}", group.state());

    // Add some members
    for i in 0..3 {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        let role = if i == 0 {
            GroupRole::Leader
        } else {
            GroupRole::Member
        };
        group.add_member(agent.id(), role).unwrap();
    }
    println!("Added {} members", group.member_count());

    // Pause the group
    println!("\nPausing group...");
    group.pause().unwrap();
    println!("State: {:?}", group.state());

    // Try to add member while paused (should fail)
    let new_agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    match group.add_member(new_agent.id(), GroupRole::Member) {
        GroupResult::Success(_) => println!("Added member while paused"),
        GroupResult::Error(_) => println!("Cannot add member while paused"),
    }

    // Resume the group
    println!("\nResuming group...");
    group.resume().unwrap();
    println!("State: {:?}", group.state());

    // Now can add members again
    group.add_member(new_agent.id(), GroupRole::Member).unwrap();
    println!("Successfully added member after resume");

    // Begin dissolution
    println!("\nDemonstrating group dissolution:");
    println!("Starting dissolution...");
    group.dissolve().unwrap();
    println!("State: {:?}", group.state());
    println!("Members still in group: {}", group.member_count());

    // Complete dissolution
    println!("Completing dissolution...");
    group.complete_dissolution().unwrap();
    println!("State: {:?}", group.state());
    println!("Members after dissolution: {}", group.member_count());

    println!();
}

/// Example 4: Shared policies
fn shared_policies() {
    println!("4. Shared Policies");
    println!("------------------");

    let group = AgentGroup::new("edge-nodes");
    println!("Created group: {}", group.name());

    // Create and set shared policy
    let policy = Policy {
        min_battery_percent: 30,
        max_latency_ms: 50,
        ..Default::default()
    };

    println!("\nSetting shared policy:");
    println!("  Min battery: {}%", policy.min_battery_percent);
    println!("  Max latency: {}ms", policy.max_latency_ms);

    group.set_shared_policy(policy.clone());

    // Retrieve shared policy
    if let Some(retrieved_policy) = group.shared_policy() {
        println!("\nRetrieved shared policy:");
        println!("  Min battery: {}%", retrieved_policy.min_battery_percent);
        println!("  Max latency: {}ms", retrieved_policy.max_latency_ms);
        println!(
            "  Preferred archs: {:?}",
            retrieved_policy.preferred_architectures
        );
    }

    // Clear shared policy
    println!("\nClearing shared policy...");
    group.clear_shared_policy();

    if group.shared_policy().is_none() {
        println!("Shared policy cleared successfully");
    }

    println!();
}

/// Example 5: Hierarchical groups
fn hierarchical_groups() {
    println!("5. Hierarchical Groups");
    println!("----------------------");

    // Create parent group
    let parent = AgentGroup::new("all-services");
    println!("Created parent group: {}", parent.name());

    // Create child groups
    let frontend = AgentGroup::new("frontend-services");
    let backend = AgentGroup::new("backend-services");
    let database = AgentGroup::new("database-services");

    println!("Created child groups:");
    println!("  - {}", frontend.name());
    println!("  - {}", backend.name());
    println!("  - {}", database.name());

    // Build hierarchy
    parent.add_child(frontend.id());
    parent.add_child(backend.id());
    parent.add_child(database.id());

    println!(
        "\nParent group has {} child groups",
        parent.children().len()
    );
    println!("Child group IDs:");
    for child_id in parent.children() {
        println!("  {}", child_id);
    }

    // Add agents to child groups
    let agent1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let agent2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    frontend.add_member(agent1.id(), GroupRole::Leader).unwrap();
    backend.add_member(agent2.id(), GroupRole::Leader).unwrap();

    println!("\nAdded agents to child groups");
    println!("  Frontend members: {}", frontend.member_count());
    println!("  Backend members: {}", backend.member_count());

    println!();
}

/// Example 6: Group registry and discovery
fn group_registry_usage() {
    println!("6. Group Registry and Discovery");
    println!("--------------------------------");

    // Create registry
    let registry = GroupRegistry::new();
    println!("Created group registry");

    // Create and register groups
    let web_group = AgentGroup::new("web-servers");
    let api_group = AgentGroup::new("api-servers");
    let cache_group = AgentGroup::new("cache-servers");

    let web_id = web_group.id();
    let api_id = api_group.id();

    let web_arc = registry.register(web_group);
    let api_arc = registry.register(api_group);
    registry.register(cache_group);

    println!("\nRegistered {} groups", registry.count());

    // Add agents to groups
    let agent1 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let agent2 = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    web_arc.add_member(agent1.id(), GroupRole::Leader).unwrap();
    api_arc.add_member(agent1.id(), GroupRole::Member).unwrap();
    api_arc.add_member(agent2.id(), GroupRole::Leader).unwrap();

    // Record group membership in registry
    registry.record_join(agent1.id(), web_id);
    registry.record_join(agent1.id(), api_id);
    registry.record_join(agent2.id(), api_id);

    // Find groups for agent
    println!("\nGroups for agent1:");
    let agent1_groups = registry.groups_for_agent(&agent1.id());
    for group in agent1_groups {
        println!("  - {} ({})", group.name(), group.id());
    }

    // Lookup group by ID
    if let Some(group) = registry.get(&web_id) {
        println!("\nLookup group by ID {}:", web_id);
        println!("  Name: {}", group.name());
        println!("  Members: {}", group.member_count());
    }

    // Get all group IDs
    println!("\nAll registered group IDs:");
    for id in registry.all_ids() {
        println!("  {}", id);
    }

    println!();
}

/// Example 7: Tags and categorization
fn tags_and_categorization() {
    println!("7. Tags and Categorization");
    println!("--------------------------");

    let registry = GroupRegistry::new();

    // Create groups with tags
    let prod1 = AgentGroup::new("prod-us-east");
    prod1.add_tag("production");
    prod1.add_tag("us-east-1");
    prod1.add_tag("critical");

    let prod2 = AgentGroup::new("prod-us-west");
    prod2.add_tag("production");
    prod2.add_tag("us-west-2");
    prod2.add_tag("critical");

    let dev = AgentGroup::new("dev-sandbox");
    dev.add_tag("development");
    dev.add_tag("us-west-2");

    println!("Created groups with tags:");
    println!("\n{} tags:", prod1.name());
    for tag in prod1.tags() {
        println!("  - {}", tag);
    }

    println!("\n{} tags:", prod2.name());
    for tag in prod2.tags() {
        println!("  - {}", tag);
    }

    println!("\n{} tags:", dev.name());
    for tag in dev.tags() {
        println!("  - {}", tag);
    }

    // Register groups
    registry.register(prod1);
    registry.register(prod2);
    registry.register(dev);

    // Find groups by tag
    println!("\nFinding groups by tag:");

    let production_groups = registry.groups_by_tag("production");
    println!("  'production' tag ({} groups):", production_groups.len());
    for group in production_groups {
        println!("    - {}", group.name());
    }

    let us_west_groups = registry.groups_by_tag("us-west-2");
    println!("  'us-west-2' tag ({} groups):", us_west_groups.len());
    for group in us_west_groups {
        println!("    - {}", group.name());
    }

    let critical_groups = registry.groups_by_tag("critical");
    println!("  'critical' tag ({} groups):", critical_groups.len());
    for group in critical_groups {
        println!("    - {}", group.name());
    }

    // Check if specific group has tag
    let test_group = registry.groups_by_tag("production").first().cloned();
    if let Some(group) = test_group {
        println!("\nTag checks for {}:", group.name());
        println!("  Has 'production': {}", group.has_tag("production"));
        println!("  Has 'development': {}", group.has_tag("development"));
    }

    println!("\n=== All examples completed ===");
}
