//! Fault Injection and Cross-Version Migration Tests
//!
//! Validates the FaultInjector under a wide range of scenarios and exercises
//! cross-version agent migration paths under induced faults.

use mielin_cells::{
    fault::{apply_delay_sync, FaultInjector, FaultKind, FaultSpec, Probability},
    migration::{MigrationManager, MigrationSnapshot},
    versioning::{Version, VersionMetadata, VersionRegistry},
    Agent, Dna, FailoverConfig, FailoverCoordinator, FailoverPolicy, FailoverStrategy,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;

// ── helpers ──────────────────────────────────────────────────────────────────

fn minimal_wasm() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d]
}

// ── 1 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_always_drop() {
    let fi = FaultInjector::always_drop("net.send");
    for _ in 0..50 {
        let kind = fi.inject_if("net.send");
        assert!(
            matches!(kind, Some(FaultKind::Drop)),
            "expected Drop on every call"
        );
    }
}

// ── 2 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_probability() {
    let fi = FaultInjector::occasionally("op", Probability::new(0.5), FaultKind::Drop);

    let hits: usize = (0..1_000)
        .map(|_| fi.inject_if("op").is_some() as usize)
        .sum();

    // With 1000 draws at p=0.5 the 99.9 % CI is roughly [400, 600].
    assert!(hits >= 400, "too few injections: {hits}");
    assert!(hits <= 600, "too many injections: {hits}");
}

// ── 3 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_max_occurrences() {
    let mut fi = FaultInjector::new();
    fi.add_fault(
        "rpc",
        FaultSpec::new(FaultKind::Drop, Probability::always()).with_max_occurrences(5),
    );

    let hits: usize = (0..20)
        .map(|_| fi.inject_if("rpc").is_some() as usize)
        .sum();
    assert_eq!(hits, 5, "injector must stop after max_occurrences=5");
}

// ── 4 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_delay() {
    let micros = 5_000u64; // 5 ms
    let fi = FaultInjector::always_delay("io.read", micros);

    let start = Instant::now();
    if let Some(FaultKind::Delay { micros: m }) = fi.inject_if("io.read") {
        apply_delay_sync(m);
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_micros() >= micros as u128,
        "delay was too short: {} µs",
        elapsed.as_micros()
    );
}

// ── 5 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_reset_clears_counts() {
    let fi = FaultInjector::always_drop("x");
    fi.inject_if("x");
    fi.inject_if("x");

    fi.reset();

    let s = fi.stats();
    assert_eq!(s.total_injected, 0);
    assert!(s.per_label.is_empty());

    // After reset the limit is also reset, so we can inject again.
    let kind = fi.inject_if("x");
    assert!(kind.is_some());
    assert_eq!(fi.stats().total_injected, 1);
}

// ── 6 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_stats() {
    let fi = FaultInjector::always_drop("a");
    fi.inject_if("a");
    fi.inject_if("a");
    fi.inject_if("a");

    let s = fi.stats();
    assert_eq!(s.total_injected, 3);
    assert_eq!(s.per_label.get("a").copied().unwrap_or(0), 3);
    // Label "b" never fired
    assert_eq!(s.per_label.get("b").copied().unwrap_or(0), 0);
}

// ── 7 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_agent_migration_under_fault_injection() {
    let fi =
        FaultInjector::occasionally("migration.transfer", Probability::new(0.5), FaultKind::Drop);

    let mut manager = MigrationManager::new();
    let mut success_count = 0usize;
    let mut error_count = 0usize;

    for _ in 0..40 {
        let agent = Agent::new(minimal_wasm());

        // Attempt migration; if fault fires, treat as transient error.
        match fi.inject_if("migration.transfer") {
            Some(FaultKind::Drop) => {
                error_count += 1;
                // No panic — we gracefully count the error.
            }
            _ => {
                let snapshot = manager.initiate_migration(&agent, None);
                assert!(snapshot.is_ok(), "initiate_migration must not panic");
                manager.complete_migration(agent.id().as_bytes());
                success_count += 1;
            }
        }
    }

    // Both paths must have been exercised (probabilistic, but extremely
    // unlikely to have 0 in either bucket over 40 trials at p=0.5).
    assert!(success_count > 0);
    assert!(error_count > 0);
    assert_eq!(manager.pending_count(), 0);
}

// ── 8 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_cross_version_migration_v1_to_v2() {
    let registry = VersionRegistry::new();
    let v1 = Version::new(1, 0, 0);
    let v2 = Version::new(2, 0, 0);

    let dna_v1 = Dna::new(minimal_wasm());
    let dna_v2 = Dna::new(minimal_wasm());

    registry
        .register_version(VersionMetadata::new(v1, dna_v1, "v1".into()).with_migration_path(vec![]))
        .expect("register v1");
    registry
        .register_version(
            VersionMetadata::new(v2, dna_v2, "v2".into()).with_migration_path(vec![v1]),
        )
        .expect("register v2");

    let agent = Agent::new(minimal_wasm());
    registry
        .register_agent(agent.id(), v1)
        .expect("register agent at v1");

    assert!(registry.can_migrate(&v1, &v2).unwrap());

    // Capture state at v1.
    let snapshot = MigrationSnapshot::capture(&agent, None).expect("capture");
    assert_eq!(snapshot.agent_id, *agent.id().as_bytes());

    // Restore at v2 level; policy and binary must survive.
    registry
        .update_agent_version(agent.id(), v2)
        .expect("update to v2");
    let restored = snapshot.restore().expect("restore");
    assert_eq!(restored.dna().binary(), agent.dna().binary());

    let current_version = registry.get_agent_version(&agent.id()).unwrap();
    assert_eq!(current_version, v2);
}

// ── 9 ────────────────────────────────────────────────────────────────────────

#[test]
fn test_cross_version_migration_backward_compat() {
    let registry = VersionRegistry::new();
    let v1 = Version::new(1, 0, 0);
    let v2 = Version::new(2, 0, 0);

    let dna = Dna::new(minimal_wasm());
    registry
        .register_version(VersionMetadata::new(v1, dna.clone(), "v1".into()))
        .unwrap();
    registry
        .register_version(VersionMetadata::new(v2, dna, "v2".into()).with_migration_path(vec![v1]))
        .unwrap();

    let agent = Agent::new(minimal_wasm());
    registry.register_agent(agent.id(), v2).unwrap();

    // Downgrade: re-register v1 metadata so the registry knows it, then
    // verify the agent's snapshot can be restored at v1 semantics.
    let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
    let serialized = snapshot.serialize().expect("serialize");
    let deserialized = MigrationSnapshot::deserialize(&serialized).expect("deserialize");

    // State round-trips correctly — a downgrade path only needs the binary.
    assert_eq!(deserialized.wasm_binary, agent.dna().binary());

    // Pretend we downgraded by re-assigning to v1.
    registry.update_agent_version(agent.id(), v1).unwrap();
    assert_eq!(registry.get_agent_version(&agent.id()).unwrap(), v1);
}

// ── 10 ───────────────────────────────────────────────────────────────────────

#[test]
fn test_ha_failover_under_node_failure() {
    let config = FailoverConfig {
        policy: FailoverPolicy {
            strategy: FailoverStrategy::Immediate,
            ..Default::default()
        },
        ..Default::default()
    };
    let coordinator = FailoverCoordinator::new(config);

    let primary = Agent::new(minimal_wasm());
    let backup = Agent::new(minimal_wasm());
    coordinator
        .register_agent(primary.id(), vec![backup.id()])
        .expect("register primary");

    // Inject a node failure event.
    let decision = coordinator
        .report_failure(&primary.id(), "simulated node failure")
        .expect("report failure");

    assert!(
        decision.should_failover,
        "coordinator must decide to failover"
    );
    assert_eq!(decision.backup_id, Some(backup.id()));

    // Execute the failover.
    let event = coordinator
        .execute_failover(&primary.id(), &backup.id())
        .expect("execute failover");

    assert!(event.success);
    assert_eq!(event.agent_id, primary.id());
    assert_eq!(event.backup_id, Some(backup.id()));
}

// ── 11 ───────────────────────────────────────────────────────────────────────

#[test]
fn test_agent_state_corruption_detection() {
    let agent = Agent::new(minimal_wasm());
    let snapshot = MigrationSnapshot::capture(&agent, None).unwrap();
    let bytes = snapshot.serialize().expect("serialize");

    // Strategy A: truncate the serialized payload severely — the deserializer
    // must encounter an unexpected EOF and return Err.
    let truncated = &bytes[..bytes.len() / 2];
    let result_truncated = MigrationSnapshot::deserialize(truncated);
    assert!(
        result_truncated.is_err(),
        "severely truncated data must not deserialize successfully"
    );

    // Strategy B: completely invalid bytes — random garbage.
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];
    let result_garbage = MigrationSnapshot::deserialize(&garbage);
    assert!(
        result_garbage.is_err(),
        "garbage data must not deserialize successfully"
    );
}

// ── 12 ───────────────────────────────────────────────────────────────────────

#[test]
fn test_migration_retry_on_transient_fault() {
    // Inject exactly 2 drops, then succeed on attempt 3+.
    let mut fi = FaultInjector::new();
    fi.add_fault(
        "transfer",
        FaultSpec::new(FaultKind::Drop, Probability::always()).with_max_occurrences(2),
    );

    let agent = Agent::new(minimal_wasm());
    let mut manager = MigrationManager::new();

    let mut attempts = 0usize;
    let snapshot = loop {
        attempts += 1;
        if let Some(FaultKind::Drop) = fi.inject_if("transfer") {
            continue; // transient fault — retry
        }
        let s = manager
            .initiate_migration(&agent, None)
            .expect("migration must succeed once faults exhausted");
        break s;
    };

    manager.complete_migration(&snapshot.agent_id);
    // Exactly 2 fault injections + 1 success attempt.
    assert_eq!(attempts, 3, "should have retried exactly twice");
    assert_eq!(manager.pending_count(), 0);
}

// ── 13 ───────────────────────────────────────────────────────────────────────

#[test]
fn test_concurrent_fault_injection() {
    let fi = Arc::new(FaultInjector::always_drop("shared"));
    let counter = Arc::new(AtomicUsize::new(0));

    let threads: Vec<_> = (0..8)
        .map(|_| {
            let fi_clone = Arc::clone(&fi);
            let ctr = Arc::clone(&counter);
            std::thread::spawn(move || {
                for _ in 0..100 {
                    if fi_clone.inject_if("shared").is_some() {
                        ctr.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for t in threads {
        t.join().expect("thread panicked");
    }

    let external = counter.load(Ordering::Relaxed);
    let stats = fi.stats();

    assert_eq!(
        external, stats.total_injected,
        "AtomicUsize internal counter must match external count"
    );
    assert_eq!(external, 800); // 8 threads × 100 always-fire
}

// ── 14 ───────────────────────────────────────────────────────────────────────

#[test]
fn test_duplicate_fault_idempotency() {
    // A "pure" operation: incrementing a counter.
    // Duplicate must double the increment but not corrupt state.
    let fi =
        FaultInjector::occasionally("counter.inc", Probability::always(), FaultKind::Duplicate);

    let mut counter: u64 = 0;

    let apply = |c: &mut u64| {
        *c += 1;
    };

    for _ in 0..5 {
        if let Some(FaultKind::Duplicate) = fi.inject_if("counter.inc") {
            apply(&mut counter); // first execution
            apply(&mut counter); // duplicate execution
        } else {
            apply(&mut counter);
        }
    }

    // 5 operations × 2 (always duplicated) = 10
    assert_eq!(counter, 10);
    // State is u64, no corruption possible; assert it hasn't wrapped in odd ways.
    assert!(counter <= 10);
}

// ── 15 ───────────────────────────────────────────────────────────────────────

#[test]
fn test_fault_injector_composition() {
    // Register two specs under the same label. The first that fires wins.
    let mut fi = FaultInjector::new();
    // Spec A: always-drop (fires first)
    fi.add_fault("op", FaultSpec::new(FaultKind::Drop, Probability::always()));
    // Spec B: always-delay (never reached because A fires first)
    fi.add_fault(
        "op",
        FaultSpec::new(FaultKind::Delay { micros: 9999 }, Probability::always()),
    );

    for _ in 0..20 {
        match fi.inject_if("op") {
            Some(FaultKind::Drop) => {} // correct
            Some(FaultKind::Delay { .. }) => panic!("Delay must not fire — Drop fires first"),
            None => panic!("at least one spec fires"),
            Some(_) => panic!("unexpected kind"),
        }
    }

    // Only spec A contributed to per-label counts.
    let s = fi.stats();
    assert_eq!(s.per_label["op"], 20);
}
