//! Disaster Recovery Example
//!
//! Demonstrates comprehensive backup and recovery workflows including:
//! - Full and incremental backups
//! - Backup verification
//! - Point-in-time recovery
//! - Backup scheduling

use mielin_cells::{
    Agent, BackupConfig, BackupManager, BackupSchedule, BackupScheduler, BackupStrategy,
    RecoveryConfig, RecoveryManager, RecoveryPoint, RecoveryStrategy, RecoveryTarget,
    VerificationConfig, Verifier,
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Disaster Recovery Example ===\n");

    // 1. Configure backup strategy
    println!("1. Configuring backup strategy...");
    let backup_config = BackupConfig {
        strategy: BackupStrategy::FullAndIncremental {
            incremental_frequency: Duration::from_secs(3600), // 1 hour
        },
        storage: mielin_cells::dr::backup::BackupStorage::Local {
            path: "/var/backups/mielin".to_string(),
        },
        enable_compression: true,
        enable_encryption: true,
        encryption_key: Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
        timeout: Duration::from_secs(300),
    };

    let backup_manager = BackupManager::new(backup_config);
    println!("   ✓ Backup manager configured");
    println!("   Strategy: Full + Incremental (every hour)");
    println!("   Storage: /var/backups/mielin");
    println!("   Compression: Enabled");
    println!("   Encryption: AES-256");
    println!();

    // 2. Create agent and initial state
    println!("2. Creating agent with initial state...");
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let initial_state = b"initial_agent_state_v1".to_vec();

    println!("   ✓ Agent created: {}", agent.id());
    println!("   Initial state size: {} bytes", initial_state.len());
    println!();

    // 3. Create full backup
    println!("3. Creating full backup...");
    let backup1 = backup_manager.create_backup(&agent.id(), initial_state)?;

    println!("   ✓ Full backup created");
    println!("   Backup ID: {}", backup1.metadata.id);
    println!("   Type: {:?}", backup1.metadata.backup_type);
    println!("   Size: {} bytes", backup1.metadata.size);
    if let Some(compressed_size) = backup1.metadata.compressed_size {
        println!("   Compressed: {} bytes", compressed_size);
    }
    println!("   Encrypted: {}", backup1.metadata.encrypted);
    println!();

    // 4. Simulate state changes and create incremental backup
    println!("4. Simulating state changes...");
    std::thread::sleep(Duration::from_millis(100));

    let modified_state = b"modified_agent_state_v2_with_changes".to_vec();
    let backup2 = backup_manager.create_backup(&agent.id(), modified_state)?;

    println!("   ✓ State modified and backed up");
    println!("   Backup ID: {}", backup2.metadata.id);
    println!("   Type: {:?}", backup2.metadata.backup_type);
    if let Some(parent_id) = &backup2.metadata.parent_id {
        println!("   Parent backup: {}", parent_id);
    }
    println!();

    // 5. Verify backup integrity
    println!("5. Verifying backup integrity...");
    let verifier = Verifier::new(VerificationConfig {
        verify_checksum: true,
        verify_completeness: true,
        verify_restoration: false,
    });

    let verification_report = verifier.verify_backup(&backup1.metadata.id)?;

    println!("   ✓ Verification complete");
    println!("   Status: {:?}", verification_report.overall_status);
    println!("   Tests passed: {}", verification_report.tests.len());
    for test in &verification_report.tests {
        println!("     - {}: {:?}", test.name, test.status);
    }
    println!();

    // 6. List all backups
    println!("6. Listing all backups...");
    let backups = backup_manager.list_backups(&agent.id())?;

    println!("   ✓ Found {} backups", backups.len());
    for (i, backup) in backups.iter().enumerate() {
        println!(
            "   Backup {}: {} ({:?})",
            i + 1,
            backup.id,
            backup.backup_type
        );
    }
    println!();

    // 7. Create recovery plan
    println!("7. Creating recovery plan...");
    let recovery_manager = RecoveryManager::new(RecoveryConfig {
        strategy: RecoveryStrategy::PointInTime,
        verify_before_restore: true,
    });

    let recovery_point = RecoveryPoint {
        timestamp: backup1.metadata.timestamp,
        backup_id: backup1.metadata.id.clone(),
        agent_id: agent.id(),
    };

    let target = RecoveryTarget {
        agent_id: agent.id(),
        recovery_point,
        strategy: RecoveryStrategy::PointInTime,
    };

    let recovery_plan = recovery_manager.create_recovery_plan(vec![target])?;

    println!("   ✓ Recovery plan created");
    println!("   Targets: {}", recovery_plan.targets.len());
    println!("   Strategy: {:?}", RecoveryStrategy::PointInTime);
    println!();

    // 8. Execute recovery
    println!("8. Executing recovery...");
    recovery_manager.execute_recovery(&recovery_plan)?;

    println!("   ✓ Recovery completed successfully");
    println!("   Agent restored to backup: {}", backup1.metadata.id);
    println!();

    // 9. Configure backup scheduling
    println!("9. Configuring backup schedule...");
    let schedule_config = mielin_cells::dr::schedule::ScheduleConfig {
        policy: mielin_cells::dr::schedule::BackupPolicy {
            full_backup_schedule: BackupSchedule {
                frequency: Duration::from_secs(86400), // Daily
                retention_days: 30,
            },
            incremental_schedule: Some(BackupSchedule {
                frequency: Duration::from_secs(3600), // Hourly
                retention_days: 7,
            }),
        },
        retention: mielin_cells::dr::schedule::BackupRetention {
            daily_retention: 7,
            weekly_retention: 4,
            monthly_retention: 12,
        },
    };

    let _scheduler = BackupScheduler::new(schedule_config);

    println!("   ✓ Backup scheduler configured");
    println!("   Full backups: Daily (30 day retention)");
    println!("   Incremental backups: Hourly (7 day retention)");
    println!("   Retention policy:");
    println!("     - Daily: 7 backups");
    println!("     - Weekly: 4 backups");
    println!("     - Monthly: 12 backups");
    println!();

    // 10. Demonstrate backup cleanup
    println!("10. Demonstrating backup cleanup...");
    let oldest_backup_id = backups[0].id.clone();
    backup_manager.delete_backup(&oldest_backup_id)?;

    let remaining = backup_manager.list_backups(&agent.id())?;
    println!("   ✓ Old backup removed");
    println!("   Remaining backups: {}", remaining.len());

    println!("\n=== Disaster Recovery Setup Complete ===");
    println!("\nKey Features Demonstrated:");
    println!("  ✓ Full and incremental backups");
    println!("  ✓ Compression and encryption");
    println!("  ✓ Backup verification");
    println!("  ✓ Point-in-time recovery");
    println!("  ✓ Automated scheduling");
    println!("  ✓ Retention policies");

    Ok(())
}
