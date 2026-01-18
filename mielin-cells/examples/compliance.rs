//! Compliance and Audit Example
//!
//! Demonstrates comprehensive compliance and audit features:
//! - Audit logging with tamper detection
//! - Policy enforcement (GDPR, CCPA)
//! - Privacy controls and data classification
//! - Compliance reporting

use mielin_cells::{
    Agent, AuditLogger, AuditQuery, CompliancePolicy, DataClassification, EventType, PolicyChecker,
    PrivacyConfig, PrivacyManager, ReportGenerator, ReportType, RetentionPolicy,
};
use std::time::{Duration, SystemTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Compliance and Audit Example ===\n");

    // 1. Setup audit logging
    println!("1. Initializing tamper-proof audit system...");
    let audit_logger = AuditLogger::new(10000);

    println!("   ✓ Audit logger initialized");
    println!("   Max entries: 10,000");
    println!("   Tamper detection: Enabled (checksum-based)");
    println!();

    // 2. Log various compliance events
    println!("2. Logging compliance events...");

    let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    // Agent lifecycle events
    audit_logger.log_event(
        EventType::AgentCreated,
        Some(agent.id()),
        Some("admin@company.com".to_string()),
        "agent_service".to_string(),
        "create_agent".to_string(),
        "success".to_string(),
    )?;

    audit_logger.log_event(
        EventType::AgentStarted,
        Some(agent.id()),
        Some("admin@company.com".to_string()),
        "agent_service".to_string(),
        "start_agent".to_string(),
        "success".to_string(),
    )?;

    // Data access events
    audit_logger.log_event(
        EventType::DataAccessed,
        Some(agent.id()),
        Some("user@company.com".to_string()),
        "customer_database".to_string(),
        "read_pii".to_string(),
        "success".to_string(),
    )?;

    audit_logger.log_event(
        EventType::DataModified,
        Some(agent.id()),
        Some("user@company.com".to_string()),
        "customer_database".to_string(),
        "update_profile".to_string(),
        "success".to_string(),
    )?;

    // Security events
    audit_logger.log_event(
        EventType::AccessDenied,
        None,
        Some("unauthorized@external.com".to_string()),
        "sensitive_data".to_string(),
        "read".to_string(),
        "denied_insufficient_permissions".to_string(),
    )?;

    println!("   ✓ 5 events logged");
    println!("     - 2 Agent lifecycle events");
    println!("     - 2 Data access events");
    println!("     - 1 Security event");
    println!();

    // 3. Query audit logs
    println!("3. Querying audit logs...");

    // Query by event type
    let data_access_query = AuditQuery {
        event_type: Some(EventType::DataAccessed),
        agent_id: None,
        user_id: None,
        start_time: None,
        end_time: None,
    };

    let data_access_events = audit_logger.query(&data_access_query)?;
    println!("   ✓ Data access events: {}", data_access_events.len());

    // Query by user
    let user_query = AuditQuery {
        event_type: None,
        agent_id: None,
        user_id: Some("user@company.com".to_string()),
        start_time: None,
        end_time: None,
    };

    let user_events = audit_logger.query(&user_query)?;
    println!("   ✓ Events by user@company.com: {}", user_events.len());

    // Verify audit log integrity
    let integrity_ok = audit_logger.verify_integrity()?;
    println!(
        "   ✓ Audit log integrity: {}",
        if integrity_ok {
            "VERIFIED ✓"
        } else {
            "COMPROMISED ✗"
        }
    );
    println!();

    // 4. Setup compliance policies
    println!("4. Configuring compliance policies...");

    let policy_checker = PolicyChecker::new();

    // GDPR Compliance Policy
    let gdpr_policy = CompliancePolicy {
        id: "gdpr-2023".to_string(),
        name: "GDPR Compliance Policy v2023".to_string(),
        rules: vec![
            mielin_cells::compliance::policy::ComplianceRule {
                id: "gdpr-data-retention".to_string(),
                name: "Data Retention Limits".to_string(),
                description: "Personal data must be deleted after 90 days unless legally required"
                    .to_string(),
                enabled: true,
            },
            mielin_cells::compliance::policy::ComplianceRule {
                id: "gdpr-consent".to_string(),
                name: "Explicit Consent Required".to_string(),
                description: "User consent required for data processing".to_string(),
                enabled: true,
            },
            mielin_cells::compliance::policy::ComplianceRule {
                id: "gdpr-right-to-erasure".to_string(),
                name: "Right to Erasure".to_string(),
                description: "Users can request complete data deletion".to_string(),
                enabled: true,
            },
        ],
    };

    policy_checker.add_policy(gdpr_policy)?;

    // CCPA Compliance Policy
    let ccpa_policy = CompliancePolicy {
        id: "ccpa-2023".to_string(),
        name: "CCPA Compliance Policy v2023".to_string(),
        rules: vec![
            mielin_cells::compliance::policy::ComplianceRule {
                id: "ccpa-disclosure".to_string(),
                name: "Data Collection Disclosure".to_string(),
                description: "Inform users about data collection at point of collection"
                    .to_string(),
                enabled: true,
            },
            mielin_cells::compliance::policy::ComplianceRule {
                id: "ccpa-opt-out".to_string(),
                name: "Right to Opt-Out".to_string(),
                description: "Users can opt-out of data sale".to_string(),
                enabled: true,
            },
        ],
    };

    policy_checker.add_policy(ccpa_policy)?;

    let policies = policy_checker.list_policies()?;
    println!("   ✓ {} compliance policies configured", policies.len());
    for policy in &policies {
        println!("     - {} ({} rules)", policy.name, policy.rules.len());
    }
    println!();

    // 5. Configure privacy controls
    println!("5. Configuring privacy controls...");

    let privacy_config = PrivacyConfig {
        controls: mielin_cells::PrivacyControl {
            anonymization: true,
            encryption: true,
            access_logging: true,
            consent_required: true,
        },
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
                classification: DataClassification::Internal,
                retention_period: Duration::from_secs(730 * 86400), // 2 years
                auto_delete: false,
            },
            RetentionPolicy {
                classification: DataClassification::Public,
                retention_period: Duration::from_secs(3650 * 86400), // 10 years
                auto_delete: false,
            },
        ],
    };

    let privacy_manager = PrivacyManager::new(privacy_config);

    println!("   ✓ Privacy controls configured");
    println!("     Anonymization: Enabled");
    println!("     Encryption: Enabled");
    println!("     Access logging: Enabled");
    println!("     Consent required: Yes");
    println!();

    println!("   Data retention policies:");
    println!("     - PII: 90 days (auto-delete)");
    println!("     - Confidential: 365 days");
    println!("     - Internal: 730 days");
    println!("     - Public: 3650 days");
    println!();

    // 6. Demonstrate data classification
    println!("6. Testing data access controls...");

    let classifications = vec![
        (DataClassification::Public, "Public data"),
        (DataClassification::Internal, "Internal data"),
        (DataClassification::Confidential, "Confidential data"),
        (DataClassification::Restricted, "Restricted data"),
        (DataClassification::PII, "Personal Identifiable Information"),
    ];

    for (classification, description) in classifications {
        let can_access = privacy_manager.check_access(classification.clone())?;
        println!(
            "   {:?}: {} - {}",
            classification,
            description,
            if can_access {
                "✓ ACCESS GRANTED"
            } else {
                "✗ ACCESS DENIED"
            }
        );
    }
    println!();

    // 7. Demonstrate data anonymization
    println!("7. Testing data anonymization...");

    let sensitive_data = b"Customer: John Doe, SSN: 123-45-6789, Email: john@example.com";
    println!(
        "   Original data: {:?}",
        String::from_utf8_lossy(sensitive_data)
    );

    let anonymized = privacy_manager.anonymize_data(sensitive_data)?;
    println!("   Anonymized: {:?}", String::from_utf8_lossy(&anonymized));
    println!();

    // 8. Generate compliance reports
    println!("8. Generating compliance reports...");

    let report_types = vec![
        (ReportType::AuditSummary, "Audit Summary Report"),
        (ReportType::ComplianceStatus, "Compliance Status Report"),
        (ReportType::DataAccess, "Data Access Report"),
        (ReportType::PolicyViolations, "Policy Violations Report"),
    ];

    for (report_type, description) in report_types {
        let report_config = mielin_cells::compliance::reporting::ReportConfig {
            report_type: report_type.clone(),
            start_time: SystemTime::now(),
            end_time: SystemTime::now(),
        };

        let generator = ReportGenerator::new(report_config);
        let report = generator.generate()?;

        println!("   ✓ {}", description);
        println!("     Type: {:?}", report.report_type);
        println!("     Summary: {}", report.summary);
    }
    println!();

    // 9. Check compliance status
    println!("9. Final compliance status check...");

    let violations = policy_checker.check_compliance("production_environment")?;

    if violations.is_empty() {
        println!("   ✓ COMPLIANT - No policy violations detected");
    } else {
        println!(
            "   ✗ NON-COMPLIANT - {} violations found:",
            violations.len()
        );
        for violation in violations {
            println!("     - {} ({})", violation.description, violation.rule_id);
        }
    }

    println!("\n=== Compliance System Operational ===");
    println!("\nCompliance Features Active:");
    println!("  ✓ Tamper-proof audit logging");
    println!("  ✓ GDPR compliance (3 rules)");
    println!("  ✓ CCPA compliance (2 rules)");
    println!("  ✓ Privacy controls and data classification");
    println!("  ✓ Automated data retention");
    println!("  ✓ PII anonymization");
    println!("  ✓ Compliance reporting");

    Ok(())
}
