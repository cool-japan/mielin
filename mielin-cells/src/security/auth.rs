//! Authentication mechanisms for agents
//!
//! This module provides token-based authentication and challenge-response protocols.

use super::identity::{AgentIdentity, PublicIdentity};
use crate::CellError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    /// Token ID
    pub token_id: [u8; 16],
    /// Agent ID this token belongs to
    pub agent_id: [u8; 16],
    /// Token value (signature of token_id by agent's private key)
    pub token_value: Vec<u8>,
    /// Expiration timestamp (Unix epoch seconds)
    pub expires_at: u64,
    /// Granted permissions
    pub permissions: Vec<String>,
    /// Token metadata
    pub metadata: TokenMetadata,
}

/// Token metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    /// Creation timestamp
    pub created_at: u64,
    /// Issuer (optional)
    pub issuer: Option<[u8; 16]>,
    /// Token purpose
    pub purpose: String,
}

impl AuthToken {
    /// Create a new authentication token
    pub fn create(
        identity: &AgentIdentity,
        duration_secs: u64,
        permissions: Vec<String>,
        purpose: String,
    ) -> Result<Self, CellError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        // Generate random token ID
        let token_id = Self::generate_token_id();

        // Sign the token ID with the agent's private key
        let token_value = identity.sign(&token_id)?;

        Ok(Self {
            token_id,
            agent_id: identity.agent_id(),
            token_value,
            expires_at: now + duration_secs,
            permissions,
            metadata: TokenMetadata {
                created_at: now,
                issuer: None,
                purpose,
            },
        })
    }

    /// Generate a random token ID
    fn generate_token_id() -> [u8; 16] {
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut token_id = [0u8; 16];
        rng.fill(&mut token_id);
        token_id
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        now >= self.expires_at
    }

    /// Verify the token signature
    pub fn verify(&self) -> Result<bool, CellError> {
        if self.is_expired() {
            return Ok(false);
        }

        // For verification, we need the public identity
        // This is a simplified version - in practice, you'd look up the public key
        Ok(!self.token_value.is_empty())
    }

    /// Verify with a public identity
    pub fn verify_with_identity(&self, identity: &PublicIdentity) -> Result<bool, CellError> {
        if self.is_expired() {
            return Ok(false);
        }

        if self.agent_id != identity.agent_id {
            return Ok(false);
        }

        identity.verify_signature(&self.token_id, &self.token_value)
    }

    /// Check if the token has a specific permission
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// Get remaining validity duration in seconds
    pub fn remaining_secs(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.expires_at.saturating_sub(now)
    }
}

/// Authentication challenge for challenge-response protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthChallenge {
    /// Challenge ID
    pub challenge_id: [u8; 16],
    /// Random nonce
    pub nonce: Vec<u8>,
    /// Challenge timestamp
    pub timestamp: u64,
    /// Expiration time (challenge validity period in seconds)
    pub validity_secs: u64,
}

impl AuthChallenge {
    /// Create a new authentication challenge
    pub fn create(validity_secs: u64) -> Result<Self, CellError> {
        use rand::RngExt;
        let mut rng = rand::rng();

        let mut challenge_id = [0u8; 16];
        rng.fill(&mut challenge_id);

        // Generate 32-byte nonce
        let mut nonce = vec![0u8; 32];
        rng.fill(&mut nonce[..]);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        Ok(Self {
            challenge_id,
            nonce,
            timestamp,
            validity_secs,
        })
    }

    /// Check if the challenge is still valid
    pub fn is_valid(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        now < self.timestamp + self.validity_secs
    }
}

/// Authentication response to a challenge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Challenge ID this response is for
    pub challenge_id: [u8; 16],
    /// Agent ID of the responder
    pub agent_id: [u8; 16],
    /// Signed challenge (signature of challenge nonce + agent ID)
    pub signature: Vec<u8>,
    /// Response timestamp
    pub timestamp: u64,
}

impl AuthResponse {
    /// Create a response to a challenge
    pub fn create(challenge: &AuthChallenge, identity: &AgentIdentity) -> Result<Self, CellError> {
        // Construct message to sign: nonce || agent_id
        let mut message = challenge.nonce.clone();
        message.extend_from_slice(&identity.agent_id());

        // Sign the message
        let signature = identity.sign(&message)?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        Ok(Self {
            challenge_id: challenge.challenge_id,
            agent_id: identity.agent_id(),
            signature,
            timestamp,
        })
    }

    /// Verify the response against a challenge
    pub fn verify(
        &self,
        challenge: &AuthChallenge,
        identity: &PublicIdentity,
    ) -> Result<bool, CellError> {
        // Check challenge is still valid
        if !challenge.is_valid() {
            return Ok(false);
        }

        // Check challenge ID matches
        if self.challenge_id != challenge.challenge_id {
            return Ok(false);
        }

        // Check agent ID matches
        if self.agent_id != identity.agent_id {
            return Ok(false);
        }

        // Reconstruct the message that should have been signed
        let mut message = challenge.nonce.clone();
        message.extend_from_slice(&self.agent_id);

        // Verify the signature
        identity.verify_signature(&message, &self.signature)
    }
}

/// Authenticator for managing authentication challenges and tokens
#[derive(Debug)]
pub struct Authenticator {
    /// Active challenges
    challenges: HashMap<[u8; 16], AuthChallenge>,
    /// Authenticated sessions
    sessions: HashMap<[u8; 16], AuthToken>,
    /// Public identities cache
    identities: HashMap<[u8; 16], PublicIdentity>,
}

impl Authenticator {
    /// Create a new authenticator
    pub fn new() -> Self {
        Self {
            challenges: HashMap::new(),
            sessions: HashMap::new(),
            identities: HashMap::new(),
        }
    }

    /// Register a public identity
    pub fn register_identity(&mut self, identity: PublicIdentity) {
        self.identities.insert(identity.agent_id, identity);
    }

    /// Create and store a new challenge
    pub fn create_challenge(&mut self, validity_secs: u64) -> Result<AuthChallenge, CellError> {
        let challenge = AuthChallenge::create(validity_secs)?;
        self.challenges
            .insert(challenge.challenge_id, challenge.clone());
        Ok(challenge)
    }

    /// Verify a response to a challenge and create a session
    pub fn verify_response(
        &mut self,
        response: &AuthResponse,
        token_duration_secs: u64,
    ) -> Result<AuthToken, CellError> {
        // Get the challenge
        let challenge = self
            .challenges
            .get(&response.challenge_id)
            .ok_or_else(|| CellError::InvalidState("Challenge not found".to_string()))?;

        // Get the public identity
        let identity = self
            .identities
            .get(&response.agent_id)
            .ok_or_else(|| CellError::InvalidState("Identity not registered".to_string()))?;

        // Verify the response
        if !response.verify(challenge, identity)? {
            return Err(CellError::InvalidState("Invalid response".to_string()));
        }

        // Create a token (simplified - normally would use the actual identity)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        let token = AuthToken {
            token_id: response.challenge_id, // Reuse challenge ID as token ID
            agent_id: response.agent_id,
            token_value: response.signature.clone(),
            expires_at: now + token_duration_secs,
            permissions: vec!["default".to_string()],
            metadata: TokenMetadata {
                created_at: now,
                issuer: None,
                purpose: "authenticated_session".to_string(),
            },
        };

        // Store the session
        self.sessions.insert(response.agent_id, token.clone());

        // Remove the challenge (one-time use)
        self.challenges.remove(&response.challenge_id);

        Ok(token)
    }

    /// Verify a token
    pub fn verify_token(&self, token: &AuthToken) -> Result<bool, CellError> {
        if token.is_expired() {
            return Ok(false);
        }

        // Check if we have a matching session
        if let Some(session) = self.sessions.get(&token.agent_id) {
            Ok(session.token_id == token.token_id)
        } else {
            Ok(false)
        }
    }

    /// Get active session for an agent
    pub fn get_session(&self, agent_id: &[u8; 16]) -> Option<&AuthToken> {
        self.sessions.get(agent_id)
    }

    /// Revoke a session
    pub fn revoke_session(&mut self, agent_id: &[u8; 16]) -> bool {
        self.sessions.remove(agent_id).is_some()
    }

    /// Cleanup expired challenges and sessions
    pub fn cleanup_expired(&mut self) {
        self.challenges.retain(|_, c| c.is_valid());
        self.sessions.retain(|_, t| !t.is_expired());
    }

    /// Get the number of active sessions
    pub fn active_sessions(&self) -> usize {
        self.sessions.len()
    }

    /// Get the number of pending challenges
    pub fn pending_challenges(&self) -> usize {
        self.challenges.len()
    }
}

impl Default for Authenticator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::identity::AgentIdentity;

    #[test]
    fn test_auth_token_creation() {
        let identity = AgentIdentity::generate();
        let token = AuthToken::create(
            &identity,
            3600,
            vec!["read".to_string()],
            "test".to_string(),
        )
        .expect("Failed to create token");

        assert_eq!(token.agent_id, identity.agent_id());
        assert!(token.has_permission("read"));
        assert!(!token.is_expired());
    }

    #[test]
    fn test_auth_token_expiration() {
        let identity = AgentIdentity::generate();
        let token = AuthToken::create(&identity, 0, vec![], "test".to_string())
            .expect("Failed to create token");

        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(token.is_expired());
    }

    #[test]
    fn test_auth_token_verify_with_identity() {
        let identity = AgentIdentity::generate();
        let token = AuthToken::create(&identity, 3600, vec![], "test".to_string())
            .expect("Failed to create token");

        let public = identity.public_identity();
        let valid = token
            .verify_with_identity(&public)
            .expect("Failed to verify");
        assert!(valid);
    }

    #[test]
    fn test_auth_challenge_creation() {
        let challenge = AuthChallenge::create(60).expect("Failed to create challenge");
        assert_eq!(challenge.nonce.len(), 32);
        assert!(challenge.is_valid());
    }

    #[test]
    fn test_auth_response_creation() {
        let challenge = AuthChallenge::create(60).expect("Failed to create challenge");
        let identity = AgentIdentity::generate();

        let response =
            AuthResponse::create(&challenge, &identity).expect("Failed to create response");
        assert_eq!(response.challenge_id, challenge.challenge_id);
        assert_eq!(response.agent_id, identity.agent_id());
    }

    #[test]
    fn test_auth_response_verify() {
        let challenge = AuthChallenge::create(60).expect("Failed to create challenge");
        let identity = AgentIdentity::generate();
        let public = identity.public_identity();

        let response =
            AuthResponse::create(&challenge, &identity).expect("Failed to create response");

        let valid = response
            .verify(&challenge, &public)
            .expect("Failed to verify");
        assert!(valid);
    }

    #[test]
    fn test_authenticator_challenge_response_flow() {
        let mut authenticator = Authenticator::new();
        let identity = AgentIdentity::generate();
        let public = identity.public_identity();

        // Register the identity
        authenticator.register_identity(public);

        // Create a challenge
        let challenge = authenticator
            .create_challenge(60)
            .expect("Failed to create challenge");

        // Create a response
        let response =
            AuthResponse::create(&challenge, &identity).expect("Failed to create response");

        // Verify and create session
        let token = authenticator
            .verify_response(&response, 3600)
            .expect("Failed to verify response");

        assert_eq!(token.agent_id, identity.agent_id());
        assert_eq!(authenticator.active_sessions(), 1);
    }

    #[test]
    fn test_authenticator_verify_token() {
        let mut authenticator = Authenticator::new();
        let identity = AgentIdentity::generate();
        let public = identity.public_identity();

        authenticator.register_identity(public);

        let challenge = authenticator.create_challenge(60).expect("Failed");
        let response = AuthResponse::create(&challenge, &identity).expect("Failed");
        let token = authenticator
            .verify_response(&response, 3600)
            .expect("Failed");

        let valid = authenticator
            .verify_token(&token)
            .expect("Failed to verify");
        assert!(valid);
    }

    #[test]
    fn test_authenticator_revoke_session() {
        let mut authenticator = Authenticator::new();
        let identity = AgentIdentity::generate();
        let public = identity.public_identity();

        authenticator.register_identity(public);

        let challenge = authenticator.create_challenge(60).expect("Failed");
        let response = AuthResponse::create(&challenge, &identity).expect("Failed");
        authenticator
            .verify_response(&response, 3600)
            .expect("Failed");

        assert_eq!(authenticator.active_sessions(), 1);

        let revoked = authenticator.revoke_session(&identity.agent_id());
        assert!(revoked);
        assert_eq!(authenticator.active_sessions(), 0);
    }

    #[test]
    fn test_authenticator_cleanup_expired() {
        let mut authenticator = Authenticator::new();

        // Create an expired challenge
        let mut challenge = AuthChallenge::create(1).expect("Failed");
        challenge.validity_secs = 0;
        authenticator
            .challenges
            .insert(challenge.challenge_id, challenge);

        assert_eq!(authenticator.pending_challenges(), 1);

        authenticator.cleanup_expired();

        assert_eq!(authenticator.pending_challenges(), 0);
    }
}
