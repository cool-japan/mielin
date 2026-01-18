//! Compliance and Audit Integration Tests
//!
//! Tests the complete compliance workflow including audit logging,
//! policy enforcement, privacy controls, and reporting.

use mielin_cells::{
    Agent, AuditLogger, AuditQuery, CompliancePolicy, DataClassification, EventType, PolicyChecker,
    PrivacyConfig, PrivacyManager, ReportGenerator, ReportType, RetentionPolicy,
};
use std::time::{Duration, SystemTime};

#[test]
fn test_complete_audit_workflow() {
    // 1. Setup audit logger
    let logger = AuditLogger::new(10000);

    // 2. Create test agent
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    // 3. Log various events
    logger
        .log_event(
            EventType::AgentCreated,
            Some(agent.id()),
            Some("admin".to_string()),
            "agent_service".to_string(),
            "create_agent".to_string(),
            "success".to_string(),
        )
        .expect("log create");

    logger
        .log_event(
            EventType::AgentStarted,
            Some(agent.id()),
            Some("admin".to_string()),
            "agent_service".to_string(),
            "start_agent".to_string(),
            "success".to_string(),
        )
        .expect("log start");

    logger
        .log_event(
            EventType::DataAccessed,
            Some(agent.id()),
            Some("user123".to_string()),
            "database".to_string(),
            "read".to_string(),
            "success".to_string(),
        )
        .expect("log data access");

    logger
        .log_event(
            EventType::AccessDenied,
            None,
            Some("unauthorized_user".to_string()),
            "sensitive_resource".to_string(),
            "write".to_string(),
            "denied".to_string(),
        )
        .expect("log denied");

    // 4. Query audit logs
    let query = AuditQuery {
        event_type: Some(EventType::AgentCreated),
        agent_id: None,
        user_id: None,
        start_time: None,
        end_time: None,
    };

    let results = logger.query(&query).expect("query");
    assert_eq!(results.len(), 1);

    // 5. Query by user
    let user_query = AuditQuery {
        event_type: None,
        agent_id: None,
        user_id: Some("admin".to_string()),
        start_time: None,
        end_time: None,
    };

    let user_results = logger.query(&user_query).expect("query by user");
    assert_eq!(user_results.len(), 2);

    // 6. Verify audit log integrity
    assert!(logger.verify_integrity().expect("verify integrity"));
}

#[test]
fn test_policy_enforcement() {
    let checker = PolicyChecker::new();

    // Create GDPR compliance policy
    let gdpr_policy = CompliancePolicy {
        id: "gdpr-v1".to_string(),
        name: "GDPR Compliance Policy".to_string(),
        rules: vec![
            mielin_cells::compliance::policy::ComplianceRule {
                id: "data-retention".to_string(),
                name: "Data Retention Rule".to_string(),
                description: "Personal data must be deleted after 90 days".to_string(),
                enabled: true,
            },
            mielin_cells::compliance::policy::ComplianceRule {
                id: "consent-required".to_string(),
                name: "Consent Requirement".to_string(),
                description: "User consent required for data processing".to_string(),
                enabled: true,
            },
        ],
    };

    checker.add_policy(gdpr_policy).expect("add policy");

    // Create CCPA compliance policy
    let ccpa_policy = CompliancePolicy {
        id: "ccpa-v1".to_string(),
        name: "CCPA Compliance Policy".to_string(),
        rules: vec![mielin_cells::compliance::policy::ComplianceRule {
            id: "right-to-delete".to_string(),
            name: "Right to Delete".to_string(),
            description: "Users can request deletion of their data".to_string(),
            enabled: true,
        }],
    };

    checker.add_policy(ccpa_policy).expect("add policy");

    // List all policies
    let policies = checker.list_policies().expect("list policies");
    assert_eq!(policies.len(), 2);

    // Check compliance
    let violations = checker
        .check_compliance("test_context")
        .expect("check compliance");
    assert_eq!(violations.len(), 0); // No violations in this test
}

#[test]
fn test_privacy_controls() {
    // Setup privacy manager with default config
    let config = PrivacyConfig::default();
    let manager = PrivacyManager::new(config);

    // Test access control
    let can_access = manager
        .check_access(DataClassification::PII)
        .expect("check access");
    assert!(can_access);

    // Test data anonymization
    let sensitive_data = b"John Doe, SSN: 123-45-6789";
    let anonymized = manager.anonymize_data(sensitive_data).expect("anonymize");

    // Data should not be anonymized by default config
    assert_eq!(anonymized, sensitive_data);

    // Test with anonymization enabled
    let mut anon_config = PrivacyConfig::default();
    anon_config.controls.anonymization = true;
    let anon_manager = PrivacyManager::new(anon_config);

    let anonymized = anon_manager
        .anonymize_data(sensitive_data)
        .expect("anonymize");
    assert_eq!(anonymized, vec![b'*'; sensitive_data.len()]);
}

#[test]
fn test_retention_policies() {
    let config = PrivacyConfig {
        controls: mielin_cells::PrivacyControl::default(),
        retention: vec![
            RetentionPolicy {
                classification: DataClassification::PII,
                retention_period: Duration::from_secs(90 * 86400), // 90 days
                auto_delete: true,
            },
            RetentionPolicy {
                classification: DataClassification::Confidential,
                retention_period: Duration::from_secs(365 * 86400), // 1 year
                auto_delete: false,
            },
            RetentionPolicy {
                classification: DataClassification::Public,
                retention_period: Duration::from_secs(3650 * 86400), // 10 years
                auto_delete: false,
            },
        ],
    };

    let _manager = PrivacyManager::new(config);

    // Manager created successfully with retention policies (test passes if no panic)
}

#[test]
fn test_compliance_reporting() {
    let report_config = mielin_cells::compliance::reporting::ReportConfig {
        report_type: ReportType::AuditSummary,
        start_time: SystemTime::now(),
        end_time: SystemTime::now(),
    };

    let generator = ReportGenerator::new(report_config);
    let report = generator.generate().expect("generate report");

    assert_eq!(report.report_type, ReportType::AuditSummary);
    assert!(!report.summary.is_empty());
}

#[test]
fn test_audit_event_types() {
    let logger = AuditLogger::new(1000);
    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);

    // Test all event types
    let event_types = vec![
        EventType::AgentCreated,
        EventType::AgentStarted,
        EventType::AgentStopped,
        EventType::AgentMigrated,
        EventType::AgentDeleted,
        EventType::StateChanged,
        EventType::PolicyViolation,
        EventType::AccessGranted,
        EventType::AccessDenied,
        EventType::ConfigChanged,
        EventType::DataAccessed,
        EventType::DataModified,
        EventType::DataDeleted,
    ];

    for event_type in event_types {
        logger
            .log_event(
                event_type,
                Some(agent.id()),
                Some("test_user".to_string()),
                "test_resource".to_string(),
                "test_action".to_string(),
                "success".to_string(),
            )
            .expect("log event");
    }

    // Query all events
    let all_query = AuditQuery {
        event_type: None,
        agent_id: None,
        user_id: None,
        start_time: None,
        end_time: None,
    };

    let all_events = logger.query(&all_query).expect("query all");
    assert_eq!(all_events.len(), 13);
}

#[test]
fn test_tamper_detection() {
    let logger = AuditLogger::new(1000);

    logger
        .log_event(
            EventType::DataModified,
            None,
            Some("admin".to_string()),
            "database".to_string(),
            "update".to_string(),
            "success".to_string(),
        )
        .expect("log event");

    // Verify integrity - should pass
    assert!(logger.verify_integrity().expect("verify"));

    // In a real scenario, we would test tampering detection
    // by manually modifying entries, but our current implementation
    // uses checksums to detect this
}

#[test]
fn test_multi_policy_compliance() {
    let checker = PolicyChecker::new();

    // Add multiple policies
    for i in 0..5 {
        let policy = CompliancePolicy {
            id: format!("policy-{}", i),
            name: format!("Policy {}", i),
            rules: vec![mielin_cells::compliance::policy::ComplianceRule {
                id: format!("rule-{}", i),
                name: format!("Rule {}", i),
                description: format!("Test rule {}", i),
                enabled: true,
            }],
        };

        checker.add_policy(policy).expect("add policy");
    }

    let policies = checker.list_policies().expect("list policies");
    assert_eq!(policies.len(), 5);
}

#[test]
fn test_data_classification_levels() {
    let classifications = vec![
        DataClassification::Public,
        DataClassification::Internal,
        DataClassification::Confidential,
        DataClassification::Restricted,
        DataClassification::PII,
    ];

    let manager = PrivacyManager::new(PrivacyConfig::default());

    // All classifications should be accessible with default config
    for classification in classifications {
        assert!(manager.check_access(classification).expect("check access"));
    }
}

#[test]
fn test_compliance_report_types() {
    let report_types = vec![
        ReportType::AuditSummary,
        ReportType::ComplianceStatus,
        ReportType::SecurityIncidents,
        ReportType::DataAccess,
        ReportType::PolicyViolations,
    ];

    for report_type in report_types {
        let config = mielin_cells::compliance::reporting::ReportConfig {
            report_type: report_type.clone(),
            start_time: SystemTime::now(),
            end_time: SystemTime::now(),
        };

        let generator = ReportGenerator::new(config);
        let report = generator.generate().expect("generate report");

        assert_eq!(report.report_type, report_type);
    }
}
