//! Compliance Policy Module

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("Policy violation: {0}")]
    PolicyViolation(String),
    #[error("Policy not found: {0}")]
    PolicyNotFound(String),
}

pub type PolicyResult<T> = Result<T, PolicyError>;

/// Compliance rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

/// Policy violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    pub rule_id: String,
    pub description: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Compliance policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompliancePolicy {
    pub id: String,
    pub name: String,
    pub rules: Vec<ComplianceRule>,
}

/// Policy checker
pub struct PolicyChecker {
    policies: Arc<RwLock<HashMap<String, CompliancePolicy>>>,
}

impl PolicyChecker {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_policy(&self, policy: CompliancePolicy) -> PolicyResult<()> {
        self.policies
            .write()
            .map_err(|_| PolicyError::PolicyViolation("Failed to acquire lock".to_string()))?
            .insert(policy.id.clone(), policy);
        Ok(())
    }

    pub fn check_compliance(&self, _context: &str) -> PolicyResult<Vec<PolicyViolation>> {
        // In a real implementation, this would check against actual policies
        Ok(Vec::new())
    }

    pub fn list_policies(&self) -> PolicyResult<Vec<CompliancePolicy>> {
        Ok(self
            .policies
            .read()
            .map_err(|_| PolicyError::PolicyViolation("Failed to acquire lock".to_string()))?
            .values()
            .cloned()
            .collect())
    }
}

impl Default for PolicyChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_checker() {
        let checker = PolicyChecker::new();
        let policy = CompliancePolicy {
            id: "gdpr".to_string(),
            name: "GDPR Compliance".to_string(),
            rules: vec![ComplianceRule {
                id: "data_retention".to_string(),
                name: "Data Retention".to_string(),
                description: "Data must be retained for X days".to_string(),
                enabled: true,
            }],
        };

        checker.add_policy(policy).expect("add policy");
        let policies = checker.list_policies().expect("list policies");
        assert_eq!(policies.len(), 1);
    }
}
