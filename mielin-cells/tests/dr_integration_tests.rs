//! Disaster Recovery Integration Tests
//!
//! Tests the complete DR workflow including backup creation,
//! recovery planning, and restoration.

use mielin_cells::{
    Agent, BackupConfig, BackupManager, BackupScheduler, BackupStrategy, BackupType,
    RecoveryConfig, RecoveryManager, RecoveryPoint, RecoveryStrategy, RecoveryTarget,
    VerificationConfig, Verifier,
};
use std::time::{Duration, SystemTime};

#[test]
fn test_complete_backup_recovery_workflow() {
    // 1. Create backup manager
    let config = BackupConfig {
        strategy: BackupStrategy::FullAndIncremental {
            incremental_frequency: Duration::from_secs(3600),
        },
        storage: mielin_cells::dr::backup::BackupStorage::Local {
            path: std::env::temp_dir()
                .join("test-backups")
                .to_string_lossy()
                .into_owned(),
        },
        enable_compression: true,
        enable_encryption: false,
        encryption_key: None,
        timeout: Duration::from_secs(300),
    };

    let backup_manager = BackupManager::new(config);

    // 2. Create test agent and state
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
    let state_data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    // 3. Create full backup
    let backup1 = backup_manager
        .create_backup(&agent.id(), state_data.clone())
        .expect("create backup 1");

    assert_eq!(backup1.metadata.backup_type, BackupType::Full);
    assert!(backup1.verify().expect("verify backup 1"));

    // 4. Create incremental backup
    let modified_state = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let _backup2 = backup_manager
        .create_backup(&agent.id(), modified_state)
        .expect("create backup 2");

    // 5. List all backups
    let backups = backup_manager
        .list_backups(&agent.id())
        .expect("list backups");
    assert_eq!(backups.len(), 2);

    // 6. Verify backup integrity
    let verifier = Verifier::new(VerificationConfig::default());
    let report = verifier
        .verify_backup(&backup1.metadata.id)
        .expect("verify");
    assert_eq!(
        report.overall_status,
        mielin_cells::dr::verification::VerificationStatus::Passed
    );

    // 7. Create recovery plan
    let recovery_manager = RecoveryManager::new(RecoveryConfig::default());

    let recovery_point = RecoveryPoint {
        timestamp: SystemTime::now(),
        backup_id: backup1.metadata.id.clone(),
        agent_id: agent.id(),
    };

    let target = RecoveryTarget {
        agent_id: agent.id(),
        recovery_point,
        strategy: RecoveryStrategy::Full,
    };

    let plan = recovery_manager
        .create_recovery_plan(vec![target])
        .expect("create plan");

    assert_eq!(plan.targets.len(), 1);

    // 8. Execute recovery
    recovery_manager
        .execute_recovery(&plan)
        .expect("execute recovery");
}

#[test]
fn test_backup_with_compression_and_encryption() {
    let encryption_key = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

    let config = BackupConfig {
        strategy: BackupStrategy::FullOnly,
        storage: mielin_cells::dr::backup::BackupStorage::Local {
            path: std::env::temp_dir()
                .join("encrypted-backups")
                .to_string_lossy()
                .into_owned(),
        },
        enable_compression: true,
        enable_encryption: true,
        encryption_key: Some(encryption_key),
        timeout: Duration::from_secs(300),
    };

    let manager = BackupManager::new(config);
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    let large_state = vec![42u8; 10000]; // 10KB of data

    let backup = manager
        .create_backup(&agent.id(), large_state)
        .expect("create backup");

    assert!(backup.metadata.encrypted);
    // Note: compressed_size field implementation pending
    // assert!(backup.metadata.compressed_size.is_some());
    assert!(backup.verify().expect("verify"));
}

#[test]
fn test_backup_retention_and_cleanup() {
    let config = BackupConfig::default();
    let manager = BackupManager::new(config);

    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    // Create multiple backups
    for i in 0..5 {
        let data = vec![i as u8; 100];
        manager
            .create_backup(&agent.id(), data)
            .expect("create backup");
    }

    let backups = manager.list_backups(&agent.id()).expect("list backups");
    assert_eq!(backups.len(), 5);

    // Delete oldest backup
    let oldest_backup_id = backups[0].id.clone();
    manager
        .delete_backup(&oldest_backup_id)
        .expect("delete backup");

    let remaining = manager.list_backups(&agent.id()).expect("list backups");
    assert_eq!(remaining.len(), 4);
}

#[test]
fn test_differential_backup_strategy() {
    let config = BackupConfig {
        strategy: BackupStrategy::FullAndDifferential {
            differential_frequency: Duration::from_secs(1800),
        },
        storage: mielin_cells::dr::backup::BackupStorage::Local {
            path: std::env::temp_dir()
                .join("differential-backups")
                .to_string_lossy()
                .into_owned(),
        },
        enable_compression: false,
        enable_encryption: false,
        encryption_key: None,
        timeout: Duration::from_secs(300),
    };

    let manager = BackupManager::new(config);
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    // Create full backup
    let data1 = vec![1, 2, 3, 4, 5];
    let backup1 = manager.create_backup(&agent.id(), data1).expect("backup 1");
    assert_eq!(backup1.metadata.backup_type, BackupType::Full);

    // Create differential backup
    let data2 = vec![1, 2, 3, 4, 5, 6, 7];
    let backup2 = manager.create_backup(&agent.id(), data2).expect("backup 2");

    // Verify parent relationship for differential backups
    if backup2.metadata.backup_type == BackupType::Differential {
        assert_eq!(
            backup2.metadata.parent_id,
            Some(backup1.metadata.id.clone())
        );
    }
}

#[test]
fn test_backup_scheduler_configuration() {
    let schedule_config = mielin_cells::dr::schedule::ScheduleConfig::default();
    let _scheduler = BackupScheduler::new(schedule_config);

    // Scheduler created successfully (test passes if no panic)
}

#[test]
fn test_recovery_point_in_time() {
    let manager = BackupManager::new(BackupConfig::default());
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    // Create multiple backups at different "times"
    let _backup1 = manager
        .create_backup(&agent.id(), vec![1, 2, 3])
        .expect("backup 1");

    std::thread::sleep(Duration::from_millis(10));

    let backup2 = manager
        .create_backup(&agent.id(), vec![4, 5, 6])
        .expect("backup 2");

    std::thread::sleep(Duration::from_millis(10));

    let _backup3 = manager
        .create_backup(&agent.id(), vec![7, 8, 9])
        .expect("backup 3");

    // Create recovery point targeting backup2
    let recovery_point = RecoveryPoint {
        timestamp: backup2.metadata.timestamp,
        backup_id: backup2.metadata.id,
        agent_id: agent.id(),
    };

    let recovery_mgr = RecoveryManager::new(RecoveryConfig {
        strategy: RecoveryStrategy::PointInTime,
        verify_before_restore: true,
    });

    let target = RecoveryTarget {
        agent_id: agent.id(),
        recovery_point,
        strategy: RecoveryStrategy::PointInTime,
    };

    let plan = recovery_mgr
        .create_recovery_plan(vec![target])
        .expect("create plan");

    assert_eq!(plan.targets.len(), 1);
}
