//! Compliance and Audit Module
//!
//! Provides compliance and auditing capabilities including:
//! - Audit logging with tamper-proof records
//! - Compliance policy enforcement
//! - Data privacy controls (GDPR, CCPA, etc.)
//! - Access control and authorization
//! - Regulatory reporting

pub mod audit;
pub mod policy;
pub mod privacy;
pub mod reporting;

pub use audit::{
    AuditEntry, AuditError, AuditLog, AuditLogger, AuditQuery, AuditResult, EventType,
};
pub use policy::{
    CompliancePolicy, ComplianceRule, PolicyChecker, PolicyError, PolicyResult, PolicyViolation,
};
pub use privacy::{
    DataClassification, PrivacyConfig, PrivacyControl, PrivacyError, PrivacyManager, PrivacyResult,
    RetentionPolicy,
};
pub use reporting::{
    ComplianceReport, ReportConfig, ReportError, ReportGenerator, ReportResult, ReportType,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("Audit error: {0}")]
    AuditError(String),
    #[error("Policy violation: {0}")]
    PolicyViolation(String),
    #[error("Privacy violation: {0}")]
    PrivacyViolation(String),
    #[error("Reporting error: {0}")]
    ReportingError(String),
}
