//! Security module for MielinOS agents
//!
//! This module provides security features including:
//! - Agent identity and authentication
//! - Encrypted state snapshots
//! - Capability attestation
//! - Sandboxed execution

pub mod attestation;
pub mod auth;
pub mod encryption;
pub mod identity;
pub mod sandbox;

pub use attestation::{AttestationValidator, Capability, CapabilityAttestation};
pub use auth::{AuthChallenge, AuthResponse, AuthToken, Authenticator};
pub use encryption::{EncryptedSnapshot, EncryptionKey, StateEncryptor};
pub use identity::{AgentIdentity, IdentityProvider, PublicIdentity};
pub use sandbox::{SandboxConfig, SandboxExecutor, SandboxViolation};

use crate::CellError;

/// Security context for an agent
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Agent identity
    pub identity: AgentIdentity,
    /// Current authentication token
    pub auth_token: Option<AuthToken>,
    /// Granted capabilities
    pub capabilities: Vec<Capability>,
    /// Sandbox configuration
    pub sandbox_config: SandboxConfig,
}

impl SecurityContext {
    /// Create a new security context with an identity
    pub fn new(identity: AgentIdentity) -> Self {
        Self {
            identity,
            auth_token: None,
            capabilities: Vec::new(),
            sandbox_config: SandboxConfig::default(),
        }
    }

    /// Set the authentication token
    pub fn set_auth_token(&mut self, token: AuthToken) {
        self.auth_token = Some(token);
    }

    /// Grant a capability
    pub fn grant_capability(&mut self, capability: Capability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
    }

    /// Check if a capability is granted
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }

    /// Verify the authentication token is valid
    pub fn verify_token(&self) -> Result<bool, CellError> {
        match &self.auth_token {
            Some(token) => token.verify(),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_context_creation() {
        let identity = AgentIdentity::generate();
        let context = SecurityContext::new(identity);
        assert!(context.auth_token.is_none());
        assert!(context.capabilities.is_empty());
    }

    #[test]
    fn test_security_context_grant_capability() {
        let identity = AgentIdentity::generate();
        let mut context = SecurityContext::new(identity);

        let cap = Capability::Network;
        context.grant_capability(cap.clone());
        assert!(context.has_capability(&cap));
    }

    #[test]
    fn test_security_context_duplicate_capability() {
        let identity = AgentIdentity::generate();
        let mut context = SecurityContext::new(identity);

        let cap = Capability::Network;
        context.grant_capability(cap.clone());
        context.grant_capability(cap.clone());
        assert_eq!(context.capabilities.len(), 1);
    }
}
