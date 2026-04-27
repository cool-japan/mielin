//! Capability attestation for agents
//!
//! This module provides capability-based security where agents can attest
//! to their capabilities and have them verified.

use super::identity::{AgentIdentity, PublicIdentity};
use crate::CellError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent capability
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Network access capability
    Network,
    /// File system access capability
    FileSystem,
    /// Inter-agent communication capability
    Messaging,
    /// Migration capability
    Migration,
    /// Resource monitoring capability
    Monitoring,
    /// Administrative capability
    Admin,
    /// Custom capability with name
    Custom(String),
}

impl Capability {
    /// Check if this capability implies another capability
    pub fn implies(&self, other: &Capability) -> bool {
        match (self, other) {
            (Capability::Admin, _) => true, // Admin implies all capabilities
            (a, b) => a == b,
        }
    }

    /// Get the capability name
    pub fn name(&self) -> String {
        match self {
            Capability::Network => "network".to_string(),
            Capability::FileSystem => "filesystem".to_string(),
            Capability::Messaging => "messaging".to_string(),
            Capability::Migration => "migration".to_string(),
            Capability::Monitoring => "monitoring".to_string(),
            Capability::Admin => "admin".to_string(),
            Capability::Custom(name) => name.clone(),
        }
    }
}

/// Capability attestation signed by an authority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAttestation {
    /// Agent ID this attestation is for
    pub agent_id: [u8; 16],
    /// Granted capabilities
    pub capabilities: Vec<Capability>,
    /// Attestation ID
    pub attestation_id: [u8; 16],
    /// Issuer agent ID (authority)
    pub issuer_id: [u8; 16],
    /// Signature by the issuer
    pub signature: Vec<u8>,
    /// Issuance timestamp
    pub issued_at: u64,
    /// Expiration timestamp
    pub expires_at: u64,
    /// Metadata
    pub metadata: AttestationMetadata,
}

/// Attestation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationMetadata {
    /// Purpose of this attestation
    pub purpose: String,
    /// Constraints or conditions
    pub constraints: Vec<String>,
    /// Revocation status
    pub revoked: bool,
}

impl CapabilityAttestation {
    /// Create a new capability attestation
    pub fn create(
        agent_id: [u8; 16],
        capabilities: Vec<Capability>,
        issuer: &AgentIdentity,
        duration_secs: u64,
        purpose: String,
    ) -> Result<Self, CellError> {
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut attestation_id = [0u8; 16];
        rng.fill(&mut attestation_id);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        // Create message to sign: attestation_id || agent_id || capabilities
        let mut message = Vec::new();
        message.extend_from_slice(&attestation_id);
        message.extend_from_slice(&agent_id);

        for cap in &capabilities {
            message.extend_from_slice(cap.name().as_bytes());
        }
        message.extend_from_slice(&now.to_le_bytes());

        let signature = issuer.sign(&message)?;

        Ok(Self {
            agent_id,
            capabilities,
            attestation_id,
            issuer_id: issuer.agent_id(),
            signature,
            issued_at: now,
            expires_at: now + duration_secs,
            metadata: AttestationMetadata {
                purpose,
                constraints: Vec::new(),
                revoked: false,
            },
        })
    }

    /// Check if the attestation is valid (not expired, not revoked)
    pub fn is_valid(&self) -> bool {
        if self.metadata.revoked {
            return false;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        now < self.expires_at
    }

    /// Check if this attestation grants a specific capability
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.capabilities.iter().any(|c| c.implies(capability))
    }

    /// Verify the attestation signature
    pub fn verify(&self, issuer_identity: &PublicIdentity) -> Result<bool, CellError> {
        if !self.is_valid() {
            return Ok(false);
        }

        if self.issuer_id != issuer_identity.agent_id {
            return Ok(false);
        }

        // Reconstruct the message that was signed
        let mut message = Vec::new();
        message.extend_from_slice(&self.attestation_id);
        message.extend_from_slice(&self.agent_id);

        for cap in &self.capabilities {
            message.extend_from_slice(cap.name().as_bytes());
        }
        message.extend_from_slice(&self.issued_at.to_le_bytes());

        issuer_identity.verify_signature(&message, &self.signature)
    }

    /// Revoke this attestation
    pub fn revoke(&mut self) {
        self.metadata.revoked = true;
    }

    /// Add a constraint to this attestation
    pub fn add_constraint(&mut self, constraint: String) {
        self.metadata.constraints.push(constraint);
    }
}

/// Attestation validator for managing and validating capability attestations
#[derive(Debug)]
pub struct AttestationValidator {
    /// Known attestations by attestation ID
    attestations: HashMap<[u8; 16], CapabilityAttestation>,
    /// Attestations by agent ID
    agent_attestations: HashMap<[u8; 16], Vec<[u8; 16]>>,
    /// Trusted issuers (authorities)
    trusted_issuers: HashMap<[u8; 16], PublicIdentity>,
    /// Revocation list
    revoked: std::collections::HashSet<[u8; 16]>,
}

impl AttestationValidator {
    /// Create a new attestation validator
    pub fn new() -> Self {
        Self {
            attestations: HashMap::new(),
            agent_attestations: HashMap::new(),
            trusted_issuers: HashMap::new(),
            revoked: std::collections::HashSet::new(),
        }
    }

    /// Register a trusted issuer
    pub fn register_issuer(&mut self, issuer: PublicIdentity) {
        self.trusted_issuers.insert(issuer.agent_id, issuer);
    }

    /// Remove a trusted issuer
    pub fn remove_issuer(&mut self, issuer_id: &[u8; 16]) -> bool {
        self.trusted_issuers.remove(issuer_id).is_some()
    }

    /// Check if an issuer is trusted
    pub fn is_trusted_issuer(&self, issuer_id: &[u8; 16]) -> bool {
        self.trusted_issuers.contains_key(issuer_id)
    }

    /// Add an attestation
    pub fn add_attestation(&mut self, attestation: CapabilityAttestation) -> Result<(), CellError> {
        // Verify the issuer is trusted
        let issuer = self
            .trusted_issuers
            .get(&attestation.issuer_id)
            .ok_or_else(|| CellError::InvalidState("Issuer not trusted".to_string()))?;

        // Verify the attestation signature
        if !attestation.verify(issuer)? {
            return Err(CellError::InvalidState(
                "Invalid attestation signature".to_string(),
            ));
        }

        let attestation_id = attestation.attestation_id;
        let agent_id = attestation.agent_id;

        self.attestations.insert(attestation_id, attestation);
        self.agent_attestations
            .entry(agent_id)
            .or_default()
            .push(attestation_id);

        Ok(())
    }

    /// Get an attestation by ID
    pub fn get_attestation(&self, attestation_id: &[u8; 16]) -> Option<&CapabilityAttestation> {
        self.attestations.get(attestation_id)
    }

    /// Get all attestations for an agent
    pub fn get_agent_attestations(&self, agent_id: &[u8; 16]) -> Vec<&CapabilityAttestation> {
        self.agent_attestations
            .get(agent_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.attestations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if an agent has a specific capability
    pub fn has_capability(&self, agent_id: &[u8; 16], capability: &Capability) -> bool {
        self.get_agent_attestations(agent_id)
            .iter()
            .any(|att| att.is_valid() && att.has_capability(capability))
    }

    /// Revoke an attestation
    pub fn revoke_attestation(&mut self, attestation_id: &[u8; 16]) -> bool {
        self.revoked.insert(*attestation_id);

        if let Some(attestation) = self.attestations.get_mut(attestation_id) {
            attestation.revoke();
            true
        } else {
            false
        }
    }

    /// Check if an attestation is revoked
    pub fn is_revoked(&self, attestation_id: &[u8; 16]) -> bool {
        self.revoked.contains(attestation_id)
    }

    /// Cleanup expired attestations
    pub fn cleanup_expired(&mut self) {
        let expired: Vec<[u8; 16]> = self
            .attestations
            .iter()
            .filter(|(_, att)| !att.is_valid())
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            if let Some(att) = self.attestations.remove(&id) {
                if let Some(agent_atts) = self.agent_attestations.get_mut(&att.agent_id) {
                    agent_atts.retain(|&aid| aid != id);
                }
            }
        }
    }

    /// Get count of active attestations
    pub fn active_count(&self) -> usize {
        self.attestations
            .values()
            .filter(|att| att.is_valid())
            .count()
    }

    /// Get count of trusted issuers
    pub fn issuer_count(&self) -> usize {
        self.trusted_issuers.len()
    }
}

impl Default for AttestationValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::identity::AgentIdentity;

    #[test]
    fn test_capability_implies() {
        let admin = Capability::Admin;
        let network = Capability::Network;

        assert!(admin.implies(&network));
        assert!(!network.implies(&admin));
        assert!(network.implies(&network));
    }

    #[test]
    fn test_capability_attestation_creation() {
        let agent_id = [1u8; 16];
        let issuer = AgentIdentity::generate();
        let capabilities = vec![Capability::Network, Capability::Messaging];

        let attestation = CapabilityAttestation::create(
            agent_id,
            capabilities,
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed to create attestation");

        assert_eq!(attestation.agent_id, agent_id);
        assert_eq!(attestation.issuer_id, issuer.agent_id());
        assert!(attestation.is_valid());
    }

    #[test]
    fn test_capability_attestation_verify() {
        let agent_id = [1u8; 16];
        let issuer = AgentIdentity::generate();
        let issuer_public = issuer.public_identity();

        let capabilities = vec![Capability::Network];
        let attestation = CapabilityAttestation::create(
            agent_id,
            capabilities,
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed to create attestation");

        let valid = attestation
            .verify(&issuer_public)
            .expect("Failed to verify");
        assert!(valid);
    }

    #[test]
    fn test_capability_attestation_has_capability() {
        let agent_id = [1u8; 16];
        let issuer = AgentIdentity::generate();

        let capabilities = vec![Capability::Network, Capability::Messaging];
        let attestation = CapabilityAttestation::create(
            agent_id,
            capabilities,
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed to create attestation");

        assert!(attestation.has_capability(&Capability::Network));
        assert!(attestation.has_capability(&Capability::Messaging));
        assert!(!attestation.has_capability(&Capability::FileSystem));
    }

    #[test]
    fn test_capability_attestation_revoke() {
        let agent_id = [1u8; 16];
        let issuer = AgentIdentity::generate();

        let mut attestation = CapabilityAttestation::create(
            agent_id,
            vec![Capability::Network],
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed to create attestation");

        assert!(attestation.is_valid());
        attestation.revoke();
        assert!(!attestation.is_valid());
    }

    #[test]
    fn test_attestation_validator() {
        let mut validator = AttestationValidator::new();
        let issuer = AgentIdentity::generate();
        let issuer_public = issuer.public_identity();

        validator.register_issuer(issuer_public);

        let agent_id = [1u8; 16];
        let attestation = CapabilityAttestation::create(
            agent_id,
            vec![Capability::Network],
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed to create attestation");

        validator
            .add_attestation(attestation)
            .expect("Failed to add attestation");

        assert!(validator.has_capability(&agent_id, &Capability::Network));
        assert!(!validator.has_capability(&agent_id, &Capability::FileSystem));
    }

    #[test]
    fn test_attestation_validator_untrusted_issuer() {
        let mut validator = AttestationValidator::new();
        let issuer = AgentIdentity::generate();

        let agent_id = [1u8; 16];
        let attestation = CapabilityAttestation::create(
            agent_id,
            vec![Capability::Network],
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed to create attestation");

        let result = validator.add_attestation(attestation);
        assert!(result.is_err());
    }

    #[test]
    fn test_attestation_validator_revoke() {
        let mut validator = AttestationValidator::new();
        let issuer = AgentIdentity::generate();
        validator.register_issuer(issuer.public_identity());

        let agent_id = [1u8; 16];
        let attestation = CapabilityAttestation::create(
            agent_id,
            vec![Capability::Network],
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed");

        let attestation_id = attestation.attestation_id;
        validator.add_attestation(attestation).expect("Failed");

        assert!(validator.has_capability(&agent_id, &Capability::Network));

        validator.revoke_attestation(&attestation_id);

        assert!(!validator.has_capability(&agent_id, &Capability::Network));
    }

    #[test]
    fn test_attestation_validator_cleanup() {
        let mut validator = AttestationValidator::new();
        let issuer = AgentIdentity::generate();
        validator.register_issuer(issuer.public_identity());

        let agent_id = [1u8; 16];

        // Create attestation that expires in 1 second
        let attestation = CapabilityAttestation::create(
            agent_id,
            vec![Capability::Network],
            &issuer,
            1, // 1 second expiration
            "test".to_string(),
        )
        .expect("Failed");

        validator.add_attestation(attestation).expect("Failed");

        // Initially should be active
        assert_eq!(validator.active_count(), 1);

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Now should be expired
        assert_eq!(validator.active_count(), 0);

        // Cleanup should remove it
        validator.cleanup_expired();
        assert_eq!(validator.attestations.len(), 0);
    }

    #[test]
    fn test_capability_admin_implies_all() {
        let agent_id = [1u8; 16];
        let issuer = AgentIdentity::generate();

        let attestation = CapabilityAttestation::create(
            agent_id,
            vec![Capability::Admin],
            &issuer,
            3600,
            "test".to_string(),
        )
        .expect("Failed");

        assert!(attestation.has_capability(&Capability::Network));
        assert!(attestation.has_capability(&Capability::FileSystem));
        assert!(attestation.has_capability(&Capability::Messaging));
        assert!(attestation.has_capability(&Capability::Migration));
    }
}
