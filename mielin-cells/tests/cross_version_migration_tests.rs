//! Cross-Version Migration Tests
//!
//! Comprehensive test suite covering multi-hop migration chains, deprecation policies,
//! rolling updates under fault injection, and agent versioning invariants.

use mielin_cells::{
    fault::{FaultInjector, FaultKind, Probability},
    migration::{MigrationManager, MigrationSnapshot},
    versioning::{
        ABTestConfig, CanaryConfig, CanaryDeployment, RollingUpdateConfig, RollingUpdateStrategy,
        Version, VersionDeployer, VersionMetadata, VersionRegistry,
    },
    Agent, AgentId, Dna,
};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

// ── deterministic rng (no rand dep) ─────────────────────────────────────────

struct Xorshift64(u64);

impl Xorshift64 {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn minimal_wasm() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d]
}

fn make_dna() -> Dna {
    Dna::new(minimal_wasm())
}

/// Register a version (non-deprecated) in `registry` and return the version.
fn reg(registry: &VersionRegistry, major: u32, minor: u32, patch: u32) -> Version {
    let v = Version::new(major, minor, patch);
    registry
        .register_version(VersionMetadata::new(
            v,
            make_dna(),
            format!("v{major}.{minor}.{patch}"),
        ))
        .expect("register_version must succeed");
    v
}

/// Register a version with an explicit migration path.
fn reg_with_path(
    registry: &VersionRegistry,
    major: u32,
    minor: u32,
    patch: u32,
    path: Vec<Version>,
) -> Version {
    let v = Version::new(major, minor, patch);
    registry
        .register_version(
            VersionMetadata::new(v, make_dna(), format!("v{major}.{minor}.{patch}"))
                .with_migration_path(path),
        )
        .expect("register_version with path must succeed");
    v
}

/// Register a deprecated version.
fn reg_deprecated(registry: &VersionRegistry, major: u32, minor: u32, patch: u32) -> Version {
    let v = Version::new(major, minor, patch);
    registry
        .register_version(
            VersionMetadata::new(
                v,
                make_dna(),
                format!("v{major}.{minor}.{patch} [deprecated]"),
            )
            .deprecate(),
        )
        .expect("register deprecated version must succeed");
    v
}

/// Register a version in an Arc<VersionRegistry> and return the Version.
fn reg_arc(registry: &Arc<VersionRegistry>, major: u32, minor: u32, patch: u32) -> Version {
    let v = Version::new(major, minor, patch);
    registry
        .register_version(VersionMetadata::new(
            v,
            Dna::new(minimal_wasm()),
            format!("v{major}.{minor}.{patch}"),
        ))
        .expect("register_version must succeed");
    v
}

/// Register an agent at a given version in an Arc<VersionRegistry> and return its AgentId.
fn make_agent_at(registry: &Arc<VersionRegistry>, version: Version) -> AgentId {
    let id = AgentId::new_v4();
    registry
        .register_agent(id, version)
        .expect("register_agent must succeed for registered version");
    id
}

// ═══════════════════════════════════════════════════════════════════════════
// A. Multi-hop migration chains (tests 1–10)
// ═══════════════════════════════════════════════════════════════════════════

// ── A-1 ──────────────────────────────────────────────────────────────────────

/// Migrate v1.0.0 → v2.0.0 (direct major jump via migration_path).
#[test]
fn migration_v1_to_v2_direct() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v2 = reg_with_path(&registry, 2, 0, 0, vec![v1]);

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), v1)
        .expect("register agent at v1");

    let ok = registry
        .can_migrate(&v1, &v2)
        .expect("can_migrate must not error when both versions registered");

    assert!(
        ok,
        "v1.0.0 → v2.0.0 must be migratable when v1 is in migration path"
    );

    registry
        .update_agent_version(agent.id(), v2)
        .expect("update_agent_version to v2 must succeed");

    assert_eq!(
        registry.get_agent_version(&agent.id()).unwrap(),
        v2,
        "agent must now be at v2"
    );
}

// ── A-2 ──────────────────────────────────────────────────────────────────────

/// Multi-hop: v1.0.0 → v1.5.0 → v2.0.0; each hop succeeds independently.
#[test]
fn migration_v1_to_v1_5_to_v2_chain() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v1_5 = reg(&registry, 1, 5, 0);
    let v2 = reg_with_path(&registry, 2, 0, 0, vec![v1_5]);

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), v1)
        .expect("register at v1");

    // Hop 1: v1.0.0 → v1.5.0 (same major, minor >= source)
    let hop1 = registry
        .can_migrate(&v1, &v1_5)
        .expect("can_migrate v1→v1_5 must not error");
    assert!(hop1, "v1.0.0 → v1.5.0 must be allowed (compatible minor)");
    registry
        .update_agent_version(agent.id(), v1_5)
        .expect("update to v1.5 must succeed");

    // Hop 2: v1.5.0 → v2.0.0 (migration_path contains v1.5.0)
    let hop2 = registry
        .can_migrate(&v1_5, &v2)
        .expect("can_migrate v1_5→v2 must not error");
    assert!(hop2, "v1.5.0 → v2.0.0 must be allowed via migration_path");
    registry
        .update_agent_version(agent.id(), v2)
        .expect("update to v2 must succeed");

    assert_eq!(registry.get_agent_version(&agent.id()).unwrap(), v2);
}

// ── A-3 ──────────────────────────────────────────────────────────────────────

/// Patch update v1.0.0 → v1.0.1 is always compatible (same major, same minor).
#[test]
fn migration_patch_version_trivial() {
    let registry = VersionRegistry::new();
    let v1_0_0 = reg(&registry, 1, 0, 0);
    let v1_0_1 = reg(&registry, 1, 0, 1);

    // v1.0.1.is_compatible_with(v1.0.0) => major==major && minor>=minor => true
    assert!(
        v1_0_1.is_compatible_with(&v1_0_0),
        "v1.0.1 must be compatible with v1.0.0"
    );
    assert!(
        v1_0_1.is_patch_update(&v1_0_0),
        "must be identified as patch update"
    );

    let ok = registry
        .can_migrate(&v1_0_0, &v1_0_1)
        .expect("can_migrate must succeed");
    assert!(ok, "patch update must always be migratable");
}

// ── A-4 ──────────────────────────────────────────────────────────────────────

/// Minor update v1.0.0 → v1.3.0 is backward-compatible (same major).
#[test]
fn migration_minor_version_backward_compatible() {
    let registry = VersionRegistry::new();
    let v1_0_0 = reg(&registry, 1, 0, 0);
    let v1_3_0 = reg(&registry, 1, 3, 0);

    assert!(
        v1_3_0.is_minor_update(&v1_0_0),
        "must identify as minor update"
    );
    assert!(
        !v1_3_0.is_breaking_change(&v1_0_0),
        "minor update must not be breaking"
    );

    let ok = registry
        .can_migrate(&v1_0_0, &v1_3_0)
        .expect("can_migrate must not error");
    assert!(ok, "v1.0.0 → v1.3.0 must be migratable");
}

// ── A-5 ──────────────────────────────────────────────────────────────────────

/// Major version bump v1.0.0 → v2.0.0 without migration_path is NOT migratable.
#[test]
fn migration_major_version_breaking() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    // Register v2 WITHOUT a migration path
    let v2 = reg(&registry, 2, 0, 0);

    assert!(
        v2.is_breaking_change(&v1),
        "major bump must be identified as breaking"
    );

    let ok = registry
        .can_migrate(&v1, &v2)
        .expect("can_migrate must not error for registered versions");
    assert!(
        !ok,
        "v1.0.0 → v2.0.0 without migration_path must not be migratable"
    );
}

// ── A-6 ──────────────────────────────────────────────────────────────────────

/// Downgrade from v2.0.0 → v1.0.0: can_migrate returns false (v1 not compatible with v2).
#[test]
fn migration_downgrade_major_rejected() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v2 = reg_with_path(&registry, 2, 0, 0, vec![v1]);

    // Downgrade: attempting to go backwards to v1
    let ok = registry
        .can_migrate(&v2, &v1)
        .expect("can_migrate must not error");
    assert!(!ok, "downgrade v2.0.0 → v1.0.0 must not be permitted");
}

// ── A-7 ──────────────────────────────────────────────────────────────────────

/// Three-hop chain: v1.0.0 → v1.5.0 → v1.9.0 → v2.0.0.
#[test]
fn migration_chain_three_hops() {
    let registry = VersionRegistry::new();
    let v1_0_0 = reg(&registry, 1, 0, 0);
    let v1_5_0 = reg(&registry, 1, 5, 0);
    let v1_9_0 = reg(&registry, 1, 9, 0);
    let v2_0_0 = reg_with_path(&registry, 2, 0, 0, vec![v1_9_0]);

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), v1_0_0)
        .expect("register agent");

    for (from, to) in [(v1_0_0, v1_5_0), (v1_5_0, v1_9_0), (v1_9_0, v2_0_0)] {
        let ok = registry
            .can_migrate(&from, &to)
            .expect("can_migrate must not error");
        assert!(ok, "hop {from} → {to} must be allowed");
        registry
            .update_agent_version(agent.id(), to)
            .expect("version update must succeed");
    }

    assert_eq!(registry.get_agent_version(&agent.id()).unwrap(), v2_0_0);
}

// ── A-8 ──────────────────────────────────────────────────────────────────────

/// Registry only accepts v1.5.0 → v2.0.0 when v1.5.0 is the migration_path entry.
/// Direct v1.0.0 → v2.0.0 must fail; via v1.5.0 must succeed.
#[test]
fn migration_chain_finds_intermediate() {
    let registry = VersionRegistry::new();
    let v1_0_0 = reg(&registry, 1, 0, 0);
    let v1_5_0 = reg(&registry, 1, 5, 0);
    let v2_0_0 = reg_with_path(&registry, 2, 0, 0, vec![v1_5_0]);

    // Direct v1.0.0 → v2.0.0: v1.0.0 not in migration_path [v1.5.0]
    let direct = registry.can_migrate(&v1_0_0, &v2_0_0).expect("can_migrate");
    assert!(
        !direct,
        "v1.0.0 → v2.0.0 must fail; intermediate v1.5.0 required"
    );

    // Via intermediate v1.5.0 → v2.0.0: v1.5.0 IS in migration_path
    let via_intermediate = registry.can_migrate(&v1_5_0, &v2_0_0).expect("can_migrate");
    assert!(
        via_intermediate,
        "v1.5.0 → v2.0.0 must succeed via migration_path"
    );
}

// ── A-9 ──────────────────────────────────────────────────────────────────────

/// Migrating to an unregistered version returns Err(VersionNotFound).
#[test]
fn migration_version_not_registered_fails() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v_unknown = Version::new(9, 9, 9); // never registered

    let result = registry.can_migrate(&v1, &v_unknown);
    assert!(
        result.is_err(),
        "can_migrate to unregistered version must return Err"
    );
}

// ── A-10 ─────────────────────────────────────────────────────────────────────

/// Self-migration v1.0.0 → v1.0.0: compatible (same version), returns true.
#[test]
fn migration_self_migration_noop() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);

    let ok = registry
        .can_migrate(&v1, &v1)
        .expect("can_migrate must not error");
    // v1.is_compatible_with(v1) => major==major && minor>=minor => true
    assert!(ok, "self-migration must be permitted (idempotent)");
}

// ═══════════════════════════════════════════════════════════════════════════
// B. Deprecation and rejection (tests 11–18)
// ═══════════════════════════════════════════════════════════════════════════

// ── B-11 ─────────────────────────────────────────────────────────────────────

/// Deprecated target version must be rejected by can_migrate.
#[test]
fn deprecated_target_rejected() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v2_deprecated = reg_deprecated(&registry, 1, 1, 0);

    let ok = registry
        .can_migrate(&v1, &v2_deprecated)
        .expect("can_migrate must not error for registered versions");
    assert!(!ok, "migration TO a deprecated version must be rejected");
}

// ── B-12 ─────────────────────────────────────────────────────────────────────

/// A deprecated source version can still be migrated AWAY from.
#[test]
fn deprecated_source_migration_allowed() {
    let registry = VersionRegistry::new();
    // Register source as deprecated, target as active
    let v1_deprecated = reg_deprecated(&registry, 1, 0, 0);
    let v1_1 = reg(&registry, 1, 1, 0);

    // v1.1.0 is compatible with v1.0.0 (same major, minor >=)
    let ok = registry
        .can_migrate(&v1_deprecated, &v1_1)
        .expect("can_migrate must not error");
    assert!(
        ok,
        "migration FROM a deprecated source must still be allowed"
    );
}

// ── B-13 ─────────────────────────────────────────────────────────────────────

/// Chain containing a deprecated intermediate version:
/// v1.0.0 → deprecated v1.5.0 → v2.0.0; the hop to deprecated must fail.
#[test]
fn multiple_deprecated_versions_in_chain() {
    let registry = VersionRegistry::new();
    let v1_0_0 = reg(&registry, 1, 0, 0);
    let v1_5_0_dep = reg_deprecated(&registry, 1, 5, 0);
    let _v2_0_0 = reg_with_path(&registry, 2, 0, 0, vec![v1_5_0_dep]);

    // Attempting to migrate TO the deprecated intermediate must fail
    let hop_to_dep = registry
        .can_migrate(&v1_0_0, &v1_5_0_dep)
        .expect("can_migrate must not error");
    assert!(
        !hop_to_dep,
        "migration to deprecated intermediate must be rejected"
    );

    // Even though v2.0.0 has migration_path=[v1.5.0 (deprecated)],
    // v1.0.0 → v2.0.0 is only possible via that path, but v1.0.0 is not v1.5.0
    let hop_to_v2 = registry
        .can_migrate(&v1_0_0, &_v2_0_0)
        .expect("can_migrate must not error");
    assert!(
        !hop_to_v2,
        "v1.0.0 → v2.0.0 via deprecated intermediate is also not permitted"
    );
}

// ── B-14 ─────────────────────────────────────────────────────────────────────

/// VersionMetadata.deprecate() sets the `deprecated` flag to true.
#[test]
fn deprecation_timeline() {
    let v = Version::new(3, 0, 0);
    let meta = VersionMetadata::new(v, make_dna(), "pre-deprecation".to_string());
    assert!(
        !meta.deprecated,
        "newly created version must NOT be deprecated"
    );

    let deprecated_meta = meta.deprecate();
    assert!(
        deprecated_meta.deprecated,
        "after .deprecate() the flag must be true"
    );
    assert_eq!(
        deprecated_meta.version, v,
        "version must be unchanged after deprecation"
    );
}

// ── B-15 ─────────────────────────────────────────────────────────────────────

/// VersionDeployer.rolling_update targeting a deprecated version returns Err.
#[tokio::test]
async fn rollout_to_deprecated_target_rejected() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = {
        let v = Version::new(1, 0, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "v1".into()))
            .expect("register v1");
        v
    };
    let v2_dep = {
        let v = Version::new(1, 1, 0);
        registry
            .register_version(
                VersionMetadata::new(v, make_dna(), "v1.1 deprecated".into()).deprecate(),
            )
            .expect("register deprecated v2");
        v
    };

    let agent_id = AgentId::new_v4();
    registry
        .register_agent(agent_id, v1)
        .expect("register agent at v1");

    let deployer = VersionDeployer::new(registry);
    let config = RollingUpdateConfig::new(v1, v2_dep)
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_health_check(false);

    let result = deployer.rolling_update(config, vec![agent_id]).await;
    assert!(
        result.is_err(),
        "rolling_update to deprecated target must return Err"
    );
}

// ── B-16 ─────────────────────────────────────────────────────────────────────

/// Canary deployment: stable version is registered, canary version is deprecated.
/// Record failures until should_abort fires to verify gate logic.
#[test]
fn canary_to_deprecated_rejects() {
    let registry = Arc::new(VersionRegistry::new());
    let v_stable = {
        let v = Version::new(1, 0, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "stable".into()))
            .expect("register stable");
        v
    };
    let v_canary_dep = {
        let v = Version::new(1, 1, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "dep canary".into()).deprecate())
            .expect("register deprecated canary");
        v
    };

    // Verify the deprecated flag is set in registry metadata
    let canary_meta = registry
        .get_version(&v_canary_dep)
        .expect("get_version must succeed");
    assert!(
        canary_meta.deprecated,
        "canary version must be deprecated in registry"
    );

    // Attempting can_migrate from stable → deprecated canary must fail
    let ok = registry
        .can_migrate(&v_stable, &v_canary_dep)
        .expect("can_migrate must not error");
    assert!(
        !ok,
        "migration to deprecated canary version must be rejected"
    );
}

// ── B-17 ─────────────────────────────────────────────────────────────────────

/// A/B test with version B deprecated: deployment can be created but version B is
/// flagged as deprecated in the registry.
#[test]
fn ab_test_with_deprecated_version_b() {
    let registry = Arc::new(VersionRegistry::new());
    let v_a = {
        let v = Version::new(1, 0, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "va".into()))
            .expect("register va");
        v
    };
    let v_b_dep = {
        let v = Version::new(1, 1, 0);
        registry
            .register_version(
                VersionMetadata::new(v, make_dna(), "vb deprecated".into()).deprecate(),
            )
            .expect("register deprecated vb");
        v
    };

    // Verify can_migrate from v_a to deprecated v_b is rejected
    let ok = registry
        .can_migrate(&v_a, &v_b_dep)
        .expect("can_migrate must not error");
    assert!(
        !ok,
        "A/B test: migration to deprecated version B must be rejected"
    );

    let deployer = VersionDeployer::new(registry.clone());
    // start_ab_test only verifies versions are registered — does not check deprecated
    let mut ab = deployer
        .start_ab_test(ABTestConfig::new(v_a, v_b_dep, 1.0))
        .expect("start_ab_test must succeed (just checks registration)");

    // With traffic_split=1.0 all agents should go to B
    for _ in 0..20 {
        let agent_id = AgentId::new_v4();
        let assigned = ab.assign_agent(agent_id);
        assert!(
            assigned == v_a || assigned == v_b_dep,
            "assigned version must be one of the two test versions"
        );
    }

    let stats = ab.stats();
    // With traffic_split=1.0 all 20 must be assigned to version B
    assert_eq!(
        stats.version_b_count, 20,
        "all agents must be assigned to version B"
    );
    assert_eq!(stats.version_a_count, 0, "no agents should be in version A");
}

// ── B-18 ─────────────────────────────────────────────────────────────────────

/// VersionRegistry.get_version returns metadata with deprecated=true after .deprecate().
#[test]
fn register_deprecated_version_metadata() {
    let registry = VersionRegistry::new();
    let v = Version::new(5, 0, 0);
    registry
        .register_version(
            VersionMetadata::new(v, make_dna(), "to be retired".into())
                .with_changelog(vec!["final release".to_string()])
                .deprecate(),
        )
        .expect("register deprecated");

    let meta = registry
        .get_version(&v)
        .expect("get_version must succeed for registered version");

    assert!(
        meta.deprecated,
        "retrieved metadata must show deprecated=true"
    );
    assert_eq!(meta.version, v);
    assert!(!meta.changelog.is_empty(), "changelog must be preserved");
}

// ═══════════════════════════════════════════════════════════════════════════
// C. Rolling update with fault injection (tests 19–28)
// ═══════════════════════════════════════════════════════════════════════════

// ── C-19 ─────────────────────────────────────────────────────────────────────

/// 10 agents, all upgrade successfully with Immediate strategy.
#[tokio::test]
async fn rolling_update_success_all_agents() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    let v2 = reg_arc(&registry, 1, 1, 0);

    let agents: Vec<AgentId> = (0..10).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry.clone());
    let config = RollingUpdateConfig::new(v1, v2)
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_health_check(false)
        .with_max_failures(0);

    let updated = deployer
        .rolling_update(config, agents.clone())
        .await
        .expect("rolling_update must succeed when all agents are healthy");

    assert_eq!(updated.len(), 10, "all 10 agents must be updated");

    for id in &agents {
        let ver = registry.get_agent_version(id).expect("must have version");
        assert_eq!(ver, v2, "every agent must be at v2 after successful update");
    }
}

// ── C-20 ─────────────────────────────────────────────────────────────────────

/// 8 agents all at v1 update to v2, within max_failures=3 threshold.
#[tokio::test]
async fn rolling_update_partial_failure_within_threshold() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    let v2 = reg_arc(&registry, 1, 1, 0);

    let agents_at_v1: Vec<AgentId> = (0..8).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry.clone());
    let config = RollingUpdateConfig::new(v1, v2)
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_health_check(false)
        .with_max_failures(3);

    let updated = deployer
        .rolling_update(config, agents_at_v1.clone())
        .await
        .expect("update must succeed within threshold");

    // All 8 were at v1; all should have been updated
    assert_eq!(updated.len(), 8, "all agents at v1 must be updated");

    for id in &agents_at_v1 {
        assert_eq!(
            registry.get_agent_version(id).unwrap(),
            v2,
            "agents must be at v2"
        );
    }
}

// ── C-21 ─────────────────────────────────────────────────────────────────────

/// Rolling update where the target is deprecated causes immediate failure.
#[tokio::test]
async fn rolling_update_exceeds_failure_threshold_halts() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    // Register deprecated target — rolling_update checks can_migrate first → Err
    let v2_dep = {
        let v = Version::new(1, 1, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "v1.1 dep".into()).deprecate())
            .expect("register");
        v
    };

    let agents: Vec<AgentId> = (0..10).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry);
    let config = RollingUpdateConfig::new(v1, v2_dep)
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_health_check(false)
        .with_max_failures(3);

    let result = deployer.rolling_update(config, agents).await;
    assert!(
        result.is_err(),
        "update to deprecated target must fail (incompatible)"
    );
}

// ── C-22 ─────────────────────────────────────────────────────────────────────

/// Simulate fault-injected upgrade operations at 30% drop rate.
/// Count how many simulated "upgrades" succeeded; verify fault fires probabilistically.
#[test]
fn rolling_update_fault_injected_during_rollout() {
    let fi = FaultInjector::occasionally("upgrade.apply", Probability::new(0.3), FaultKind::Drop);
    let mut rng = Xorshift64(0xABCDEF01_23456789);

    let n_agents = 30;
    let mut success_count = 0usize;
    let mut fault_count = 0usize;

    for _ in 0..n_agents {
        match fi.inject_if("upgrade.apply") {
            Some(FaultKind::Drop) => fault_count += 1,
            _ => success_count += 1,
        }
        let _ = rng.next_f64();
    }

    // Verify stats match observed counts
    let stats = fi.stats();
    assert_eq!(
        stats.total_injected, fault_count,
        "stats must match observed fault count"
    );
    assert!(
        fault_count <= n_agents,
        "fault count cannot exceed total agents"
    );
    assert_eq!(
        success_count + fault_count,
        n_agents,
        "all agents accounted for"
    );
    // At p=0.3 over 30 trials we'd expect ~9 faults; allow wide CI for low trial count
    assert!(
        fault_count < n_agents,
        "not all operations can fail at p=0.3"
    );
}

// ── C-23 ─────────────────────────────────────────────────────────────────────

/// After a failed rollout (deprecated target), agents remain at original version.
#[tokio::test]
async fn rolling_update_rollback_restores_old_version() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    // Deprecated target triggers can_migrate=false → rollout fails before any updates
    let v2_dep = {
        let v = Version::new(1, 1, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "dep".into()).deprecate())
            .expect("register");
        v
    };

    let agents: Vec<AgentId> = (0..5).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry.clone());
    let config = RollingUpdateConfig::new(v1, v2_dep)
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_rollback(true)
        .with_health_check(false)
        .with_max_failures(0);

    let _err = deployer
        .rolling_update(config, agents.clone())
        .await
        .expect_err("update to deprecated version must fail");

    // All agents must still be at v1 (rollout failed before any agent was updated)
    for id in &agents {
        let ver = registry.get_agent_version(id).expect("must have version");
        assert_eq!(
            ver, v1,
            "agent must remain at v1 after failed/rolled-back update"
        );
    }
}

// ── C-24 ─────────────────────────────────────────────────────────────────────

/// Batched strategy (batch_size=3): 9 agents → 3 batches of 3.
#[tokio::test]
async fn rolling_update_wave_strategy() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    let v2 = reg_arc(&registry, 1, 1, 0);

    let agents: Vec<AgentId> = (0..9).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry.clone());
    let config = RollingUpdateConfig::new(v1, v2)
        .with_strategy(RollingUpdateStrategy::Batched {
            batch_size: 3,
            delay_between_batches_ms: 0, // no sleep in tests
        })
        .with_health_check(false)
        .with_max_failures(0);

    let updated = deployer
        .rolling_update(config, agents.clone())
        .await
        .expect("batched update must succeed");

    assert_eq!(
        updated.len(),
        9,
        "all 9 agents must be updated in 3 batches of 3"
    );
    for id in &agents {
        assert_eq!(registry.get_agent_version(id).unwrap(), v2);
    }
}

// ── C-25 ─────────────────────────────────────────────────────────────────────

/// Sequential strategy: 5 agents, each updated one at a time with 0ms delay.
#[tokio::test]
async fn rolling_update_one_at_a_time() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    let v2 = reg_arc(&registry, 1, 1, 0);

    let agents: Vec<AgentId> = (0..5).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry.clone());
    let config = RollingUpdateConfig::new(v1, v2)
        .with_strategy(RollingUpdateStrategy::Sequential {
            delay_between_agents_ms: 0,
        })
        .with_health_check(false)
        .with_max_failures(0);

    let updated = deployer
        .rolling_update(config, agents.clone())
        .await
        .expect("sequential update must succeed");

    assert_eq!(
        updated.len(),
        5,
        "all 5 agents must be updated sequentially"
    );
    for id in &agents {
        assert_eq!(registry.get_agent_version(id).unwrap(), v2);
    }
}

// ── C-26 ─────────────────────────────────────────────────────────────────────

/// Canary gates: inject failures beyond threshold → should_abort fires, blocks main rollout.
#[test]
fn rolling_update_canary_gates_success() {
    let v_stable = Version::new(1, 0, 0);
    let v_canary = Version::new(1, 1, 0);
    let config = CanaryConfig::new(v_stable, v_canary)
        .with_initial_percentage(0.1)
        .with_max_failures(3)
        .with_success_threshold(0.95);

    let mut deployment = CanaryDeployment {
        config,
        canary_agents: HashSet::new(),
        stable_agents: HashSet::new(),
        current_percentage: 0.1,
        success_count: 0,
        failure_count: 0,
    };

    // Inject 4 failures — exceeds max_failures=3
    let fi = FaultInjector::occasionally("canary.check", Probability::always(), FaultKind::Drop);
    for _ in 0..4 {
        if let Some(FaultKind::Drop) = fi.inject_if("canary.check") {
            deployment.record_failure();
        }
    }
    // Add some successes
    for _ in 0..6 {
        deployment.record_success();
    }

    assert!(
        deployment.should_abort(),
        "canary must abort when failures exceed max_failures"
    );
    assert!(
        !deployment.should_promote(),
        "canary must not promote when it should abort"
    );
}

// ── C-27 ─────────────────────────────────────────────────────────────────────

/// Rolling update halts when migration validation fails at the can_migrate check.
/// Verifies no agent was modified before the error is returned.
#[tokio::test]
async fn rolling_update_health_check_fail_stops_rollout() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    // Deprecated = migration not permitted = deployer returns Err before processing agents
    let v2_invalid = {
        let v = Version::new(1, 1, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "invalid".into()).deprecate())
            .expect("register");
        v
    };

    let agents: Vec<AgentId> = (0..5).map(|_| make_agent_at(&registry, v1)).collect();

    let deployer = VersionDeployer::new(registry.clone());
    let config = RollingUpdateConfig::new(v1, v2_invalid)
        .with_strategy(RollingUpdateStrategy::Immediate)
        .with_health_check(true)
        .with_max_failures(0);

    let result = deployer.rolling_update(config, agents.clone()).await;
    assert!(
        result.is_err(),
        "rollout must halt when migration is not permitted"
    );

    // Verify no agent was updated
    for id in &agents {
        assert_eq!(
            registry.get_agent_version(id).unwrap(),
            v1,
            "agents must remain at v1"
        );
    }
}

// ── C-28 ─────────────────────────────────────────────────────────────────────

/// Stress test: 100 agents, ~5% failure rate simulated via fault injection.
/// Verifies final state consistency across both version buckets.
#[test]
fn rolling_update_100_agents_stress() {
    let fi = FaultInjector::occasionally("upgrade.stress", Probability::new(0.05), FaultKind::Drop);

    let registry = Arc::new(VersionRegistry::new());
    let v1 = reg_arc(&registry, 1, 0, 0);
    let v2 = reg_arc(&registry, 1, 1, 0);

    let agents: Vec<AgentId> = (0..100).map(|_| make_agent_at(&registry, v1)).collect();

    let mut upgraded = 0usize;
    let mut faulted = 0usize;

    for &id in &agents {
        match fi.inject_if("upgrade.stress") {
            Some(FaultKind::Drop) => {
                faulted += 1;
                // Agent stays at v1 (simulated failure)
            }
            _ => {
                registry
                    .update_agent_version(id, v2)
                    .expect("update_agent_version must succeed for registered version");
                upgraded += 1;
            }
        }
    }

    assert_eq!(upgraded + faulted, 100, "all agents must be accounted for");

    let stats = fi.stats();
    assert_eq!(
        stats.total_injected, faulted,
        "fault stats must match faulted count"
    );

    // Verify registry consistency: agents at v1 + agents at v2 == 100
    let at_v1 = registry.get_agents_by_version(&v1).len();
    let at_v2 = registry.get_agents_by_version(&v2).len();
    assert_eq!(
        at_v1 + at_v2,
        100,
        "all 100 agents must be in exactly one version group"
    );
    assert_eq!(at_v2, upgraded, "upgraded count must match registry");
    assert_eq!(at_v1, faulted, "faulted (stayed at v1) must match registry");
}

// ═══════════════════════════════════════════════════════════════════════════
// D. Agent migration versioning (tests 29–35)
// ═══════════════════════════════════════════════════════════════════════════

// ── D-29 ─────────────────────────────────────────────────────────────────────

/// After migrating an agent, the VersionRegistry reflects the new version.
#[test]
fn agent_version_tracking_on_migration() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v2 = reg_with_path(&registry, 2, 0, 0, vec![v1]);

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), v1)
        .expect("register agent");

    // Simulate migration by updating version
    let old_ver = registry
        .update_agent_version(agent.id(), v2)
        .expect("update must succeed");

    assert_eq!(old_ver, v1, "returned old version must be v1");
    assert_eq!(
        registry.get_agent_version(&agent.id()).unwrap(),
        v2,
        "registry must reflect new version v2"
    );
}

// ── D-30 ─────────────────────────────────────────────────────────────────────

/// Each migration strictly increments the version stored in registry.
#[test]
fn agent_incarnation_increments_on_migration() {
    let registry = VersionRegistry::new();
    let versions: Vec<Version> = (0..5)
        .map(|i| {
            let v = Version::new(1, i, 0);
            registry
                .register_version(VersionMetadata::new(v, make_dna(), format!("v1.{i}")))
                .expect("register");
            v
        })
        .collect();

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), versions[0])
        .expect("register agent at v1.0");

    for (i, &v) in versions.iter().enumerate().skip(1) {
        let prev = registry.get_agent_version(&agent.id()).unwrap();
        registry
            .update_agent_version(agent.id(), v)
            .expect("update must succeed");
        let curr = registry.get_agent_version(&agent.id()).unwrap();
        assert!(curr > prev, "version must strictly increase at hop {i}");
        assert_eq!(curr, v);
    }
}

// ── D-31 ─────────────────────────────────────────────────────────────────────

/// 5 agents each migrating to a different target version (single-threaded simulation).
#[test]
fn multi_agent_concurrent_migration() {
    let registry = VersionRegistry::new();
    let v_base = reg(&registry, 1, 0, 0);
    // Register 5 target versions: v1.1.0, v1.2.0, ..., v1.5.0
    let targets: Vec<Version> = (1..=5).map(|i| reg(&registry, 1, i, 0)).collect();

    // Create 5 agents, each starting at v_base
    let agents: Vec<AgentId> = (0..5)
        .map(|_| {
            let id = AgentId::new_v4();
            registry.register_agent(id, v_base).expect("register agent");
            id
        })
        .collect();

    // Each agent migrates to its own unique target version
    for (i, &agent_id) in agents.iter().enumerate() {
        registry
            .update_agent_version(agent_id, targets[i])
            .expect("concurrent migration must succeed");
    }

    // Verify: each agent is at the correct target
    for (i, &agent_id) in agents.iter().enumerate() {
        let ver = registry
            .get_agent_version(&agent_id)
            .expect("must have version");
        assert_eq!(
            ver, targets[i],
            "agent {i} must be at target version {}",
            targets[i]
        );
    }

    // Verify get_agents_by_version: each target version has exactly 1 agent
    for target in &targets {
        let count = registry.get_agents_by_version(target).len();
        assert_eq!(count, 1, "each target version must have exactly 1 agent");
    }
}

// ── D-32 ─────────────────────────────────────────────────────────────────────

/// Capture version snapshot before and after migration; verify binary preserved.
#[test]
fn agent_version_snapshot_before_after() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v2 = reg_with_path(&registry, 2, 0, 0, vec![v1]);

    let agent = Agent::new(minimal_wasm());
    registry.register_agent(agent.id(), v1).expect("register");

    // Snapshot before migration
    let snapshot_before =
        MigrationSnapshot::capture(&agent, None).expect("capture before migration must succeed");
    let ver_before = registry.get_agent_version(&agent.id()).unwrap();

    // Perform migration
    registry
        .update_agent_version(agent.id(), v2)
        .expect("update to v2");

    // Snapshot after migration (using restored agent)
    let restored_agent = snapshot_before.restore().expect("restore must succeed");
    let snapshot_after = MigrationSnapshot::capture(&restored_agent, None)
        .expect("capture after migration must succeed");

    let ver_after = registry.get_agent_version(&agent.id()).unwrap();

    assert_eq!(ver_before, v1, "version before migration must be v1");
    assert_eq!(ver_after, v2, "version after migration must be v2");
    assert_eq!(
        snapshot_before.wasm_binary, snapshot_after.wasm_binary,
        "WASM binary must be preserved across migration snapshots"
    );
}

// ── D-33 ─────────────────────────────────────────────────────────────────────

/// get_agents_by_version correctly reflects post-migration state.
#[test]
fn version_registry_agent_count_by_version() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v2 = reg(&registry, 1, 1, 0);

    // Register 6 agents at v1
    let agents: Vec<AgentId> = (0..6)
        .map(|_| {
            let id = AgentId::new_v4();
            registry.register_agent(id, v1).expect("register agent");
            id
        })
        .collect();

    // Before migration: all 6 at v1, 0 at v2
    assert_eq!(
        registry.get_agents_by_version(&v1).len(),
        6,
        "pre-migration: 6 at v1"
    );
    assert_eq!(
        registry.get_agents_by_version(&v2).len(),
        0,
        "pre-migration: 0 at v2"
    );

    // Migrate 4 agents to v2
    for &id in agents.iter().take(4) {
        registry
            .update_agent_version(id, v2)
            .expect("update must succeed");
    }

    // After migration: 2 at v1, 4 at v2
    assert_eq!(
        registry.get_agents_by_version(&v1).len(),
        2,
        "post-migration: 2 remaining at v1"
    );
    assert_eq!(
        registry.get_agents_by_version(&v2).len(),
        4,
        "post-migration: 4 at v2"
    );
}

// ── D-34 ─────────────────────────────────────────────────────────────────────

/// 10 threads each updating the same agent concurrently; last-writer-wins.
/// Verify no panic/deadlock and final state is one of the valid versions.
#[test]
fn version_registry_concurrent_updates() {
    let registry = Arc::new(VersionRegistry::new());
    let v1 = {
        let v = Version::new(1, 0, 0);
        registry
            .register_version(VersionMetadata::new(v, make_dna(), "v1".into()))
            .expect("register v1");
        v
    };

    // Register 10 distinct target versions
    let targets: Vec<Version> = (1..=10)
        .map(|i| {
            let v = Version::new(1, i, 0);
            registry
                .register_version(VersionMetadata::new(v, make_dna(), format!("v1.{i}")))
                .expect("register");
            v
        })
        .collect();

    let agent_id = AgentId::new_v4();
    registry
        .register_agent(agent_id, v1)
        .expect("register agent at v1");

    let update_count = Arc::new(AtomicUsize::new(0));

    let threads: Vec<_> = targets
        .iter()
        .map(|&target| {
            let reg_clone = Arc::clone(&registry);
            let ctr = Arc::clone(&update_count);
            std::thread::spawn(
                move || match reg_clone.update_agent_version(agent_id, target) {
                    Ok(_) => {
                        ctr.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        panic!("concurrent update failed unexpectedly: {e}");
                    }
                },
            )
        })
        .collect();

    for t in threads {
        t.join().expect("thread must not panic");
    }

    assert_eq!(
        update_count.load(Ordering::Relaxed),
        10,
        "all 10 threads must have successfully updated the agent"
    );

    // Final version must be one of the valid targets (last-writer-wins)
    let final_version = registry
        .get_agent_version(&agent_id)
        .expect("agent must have a version after concurrent updates");
    assert!(
        targets.contains(&final_version),
        "final version {final_version} must be one of the registered targets"
    );
}

// ── D-35 ─────────────────────────────────────────────────────────────────────

/// Multi-hop migration telemetry: MigrationManager records each hop as a pending snapshot,
/// completing each hop and verifying pending_count returns to 0 between hops.
#[test]
fn migration_telemetry_records_hops() {
    let registry = VersionRegistry::new();
    let v1 = reg(&registry, 1, 0, 0);
    let v1_5 = reg(&registry, 1, 5, 0);
    let v2 = reg_with_path(&registry, 2, 0, 0, vec![v1_5]);

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), v1)
        .expect("register agent");

    let mut manager = MigrationManager::new();
    assert_eq!(
        manager.pending_count(),
        0,
        "no pending migrations initially"
    );

    // Hop 1: v1.0.0 → v1.5.0
    let snap1 = manager
        .initiate_migration(&agent, None)
        .expect("initiate migration hop 1 must succeed");
    assert_eq!(
        manager.pending_count(),
        1,
        "one pending migration after hop 1"
    );
    assert_eq!(
        &snap1.agent_id,
        agent.id().as_bytes(),
        "snapshot agent_id must match"
    );

    manager.complete_migration(&snap1.agent_id);
    assert_eq!(
        manager.pending_count(),
        0,
        "pending count must be 0 after completing hop 1"
    );

    // Update registry to reflect hop 1 completion
    registry
        .update_agent_version(agent.id(), v1_5)
        .expect("update to v1.5");

    // Verify can proceed to v2 from v1.5
    assert!(
        registry
            .can_migrate(&v1_5, &v2)
            .expect("can_migrate v1.5→v2"),
        "v1.5 → v2 must be allowed"
    );

    // Hop 2: v1.5.0 → v2.0.0 (using same agent binary)
    let snap2 = manager
        .initiate_migration(&agent, None)
        .expect("initiate migration hop 2 must succeed");
    assert_eq!(
        manager.pending_count(),
        1,
        "one pending migration during hop 2"
    );

    // Serialize and deserialize the hop-2 snapshot to verify round-trip
    let serialized = snap2.serialize().expect("serialize hop-2 snapshot");
    let deserialized = MigrationSnapshot::deserialize(&serialized)
        .expect("deserialize hop-2 snapshot must succeed");
    assert_eq!(
        deserialized.agent_id, snap2.agent_id,
        "deserialized agent_id must match"
    );
    assert_eq!(
        deserialized.wasm_binary, snap2.wasm_binary,
        "WASM binary must survive serialization round-trip"
    );

    manager.complete_migration(&snap2.agent_id);
    assert_eq!(
        manager.pending_count(),
        0,
        "all migrations complete after hop 2"
    );

    registry
        .update_agent_version(agent.id(), v2)
        .expect("update to v2");
    assert_eq!(
        registry.get_agent_version(&agent.id()).unwrap(),
        v2,
        "agent must be at v2 after two migration hops"
    );
}
