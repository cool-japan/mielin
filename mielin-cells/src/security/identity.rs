//! Agent identity management with cryptographic keys
//!
//! This module provides Ed25519-based identity for agents.

use crate::CellError;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};

/// Agent identity with public/private key pair
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    /// Agent ID (derived from public key)
    agent_id: [u8; 16],
    /// Private key (Ed25519)
    private_key: Vec<u8>,
    /// Public key (Ed25519)
    public_key: Vec<u8>,
    /// Identity metadata
    metadata: IdentityMetadata,
}

/// Public identity (shareable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicIdentity {
    /// Agent ID
    pub agent_id: [u8; 16],
    /// Public key
    pub public_key: Vec<u8>,
    /// Metadata
    pub metadata: IdentityMetadata,
}

/// Identity metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityMetadata {
    /// Creation timestamp
    pub created_at: u64,
    /// Identity version
    pub version: String,
    /// Optional friendly name
    pub name: Option<String>,
}

impl AgentIdentity {
    /// Generate a new random identity
    pub fn generate() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_bytes =
            Ed25519KeyPair::generate_pkcs8(&rng).expect("Failed to generate key pair");

        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
            .expect("Failed to parse generated key pair");

        let public_key = key_pair.public_key().as_ref().to_vec();
        let private_key = pkcs8_bytes.as_ref().to_vec();

        // Derive agent ID from public key (first 16 bytes of SHA-256 hash)
        let agent_id = Self::derive_agent_id(&public_key);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time error")
            .as_secs();

        Self {
            agent_id,
            private_key,
            public_key,
            metadata: IdentityMetadata {
                created_at: timestamp,
                version: "1.0".to_string(),
                name: None,
            },
        }
    }

    /// Create identity from existing key material (PKCS8 format)
    pub fn from_pkcs8(pkcs8_bytes: &[u8]) -> Result<Self, CellError> {
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes)
            .map_err(|e| CellError::InvalidState(format!("Invalid PKCS8 key: {}", e)))?;

        let public_key = key_pair.public_key().as_ref().to_vec();
        let private_key = pkcs8_bytes.to_vec();
        let agent_id = Self::derive_agent_id(&public_key);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CellError::InvalidState("System time error".to_string()))?
            .as_secs();

        Ok(Self {
            agent_id,
            private_key,
            public_key,
            metadata: IdentityMetadata {
                created_at: timestamp,
                version: "1.0".to_string(),
                name: None,
            },
        })
    }

    /// Derive agent ID from public key
    fn derive_agent_id(public_key: &[u8]) -> [u8; 16] {
        use ring::digest;
        let hash = digest::digest(&digest::SHA256, public_key);
        let mut agent_id = [0u8; 16];
        agent_id.copy_from_slice(&hash.as_ref()[0..16]);
        agent_id
    }

    /// Get the agent ID
    pub fn agent_id(&self) -> [u8; 16] {
        self.agent_id
    }

    /// Get the public key
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CellError> {
        let key_pair = Ed25519KeyPair::from_pkcs8(&self.private_key)
            .map_err(|e| CellError::InvalidState(format!("Key pair error: {}", e)))?;

        let signature = key_pair.sign(message);
        Ok(signature.as_ref().to_vec())
    }

    /// Verify a signature on a message
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<bool, CellError> {
        let public_key = UnparsedPublicKey::new(&ED25519, &self.public_key);
        match public_key.verify(message, signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get the public identity (for sharing)
    pub fn public_identity(&self) -> PublicIdentity {
        PublicIdentity {
            agent_id: self.agent_id,
            public_key: self.public_key.clone(),
            metadata: self.metadata.clone(),
        }
    }

    /// Set a friendly name
    pub fn set_name(&mut self, name: String) {
        self.metadata.name = Some(name);
    }

    /// Get the metadata
    pub fn metadata(&self) -> &IdentityMetadata {
        &self.metadata
    }

    /// Export the private key (PKCS8 format) - use with caution!
    pub fn export_private_key(&self) -> &[u8] {
        &self.private_key
    }
}

impl PublicIdentity {
    /// Verify a signature on a message using this public identity
    pub fn verify_signature(&self, message: &[u8], signature: &[u8]) -> Result<bool, CellError> {
        let public_key = UnparsedPublicKey::new(&ED25519, &self.public_key);
        match public_key.verify(message, signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Identity provider for managing identities
#[derive(Debug)]
pub struct IdentityProvider {
    /// Stored identities by agent ID
    identities: std::collections::HashMap<[u8; 16], AgentIdentity>,
}

impl IdentityProvider {
    /// Create a new identity provider
    pub fn new() -> Self {
        Self {
            identities: std::collections::HashMap::new(),
        }
    }

    /// Generate and store a new identity
    pub fn create_identity(&mut self) -> AgentIdentity {
        let identity = AgentIdentity::generate();
        let agent_id = identity.agent_id();
        self.identities.insert(agent_id, identity.clone());
        identity
    }

    /// Import an existing identity
    pub fn import_identity(&mut self, identity: AgentIdentity) {
        let agent_id = identity.agent_id();
        self.identities.insert(agent_id, identity);
    }

    /// Get an identity by agent ID
    pub fn get_identity(&self, agent_id: &[u8; 16]) -> Option<&AgentIdentity> {
        self.identities.get(agent_id)
    }

    /// Remove an identity
    pub fn remove_identity(&mut self, agent_id: &[u8; 16]) -> Option<AgentIdentity> {
        self.identities.remove(agent_id)
    }

    /// List all agent IDs
    pub fn list_identities(&self) -> Vec<[u8; 16]> {
        self.identities.keys().copied().collect()
    }

    /// Get the number of identities
    pub fn count(&self) -> usize {
        self.identities.len()
    }
}

impl Default for IdentityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_generation() {
        let identity = AgentIdentity::generate();
        assert_eq!(identity.agent_id().len(), 16);
        assert!(!identity.public_key().is_empty());
    }

    #[test]
    fn test_identity_sign_verify() {
        let identity = AgentIdentity::generate();
        let message = b"Hello, MielinOS!";

        let signature = identity.sign(message).expect("Failed to sign");
        let valid = identity
            .verify(message, &signature)
            .expect("Failed to verify");

        assert!(valid);
    }

    #[test]
    fn test_identity_verify_invalid_signature() {
        let identity = AgentIdentity::generate();
        let message = b"Hello, MielinOS!";
        let wrong_message = b"Wrong message";

        let signature = identity.sign(message).expect("Failed to sign");
        let valid = identity
            .verify(wrong_message, &signature)
            .expect("Failed to verify");

        assert!(!valid);
    }

    #[test]
    fn test_identity_public_identity() {
        let identity = AgentIdentity::generate();
        let public = identity.public_identity();

        assert_eq!(public.agent_id, identity.agent_id());
        assert_eq!(public.public_key, identity.public_key());
    }

    #[test]
    fn test_public_identity_verify() {
        let identity = AgentIdentity::generate();
        let public = identity.public_identity();
        let message = b"Test message";

        let signature = identity.sign(message).expect("Failed to sign");
        let valid = public
            .verify_signature(message, &signature)
            .expect("Failed to verify");

        assert!(valid);
    }

    #[test]
    fn test_identity_set_name() {
        let mut identity = AgentIdentity::generate();
        identity.set_name("TestAgent".to_string());

        assert_eq!(identity.metadata().name, Some("TestAgent".to_string()));
    }

    #[test]
    fn test_identity_from_pkcs8() {
        let identity1 = AgentIdentity::generate();
        let pkcs8 = identity1.export_private_key();

        let identity2 = AgentIdentity::from_pkcs8(pkcs8).expect("Failed to import");

        assert_eq!(identity1.agent_id(), identity2.agent_id());
        assert_eq!(identity1.public_key(), identity2.public_key());
    }

    #[test]
    fn test_identity_provider() {
        let mut provider = IdentityProvider::new();

        let identity = provider.create_identity();
        let agent_id = identity.agent_id();

        assert_eq!(provider.count(), 1);
        assert!(provider.get_identity(&agent_id).is_some());
    }

    #[test]
    fn test_identity_provider_import() {
        let mut provider = IdentityProvider::new();
        let identity = AgentIdentity::generate();
        let agent_id = identity.agent_id();

        provider.import_identity(identity);

        assert_eq!(provider.count(), 1);
        assert!(provider.get_identity(&agent_id).is_some());
    }

    #[test]
    fn test_identity_provider_remove() {
        let mut provider = IdentityProvider::new();
        let identity = provider.create_identity();
        let agent_id = identity.agent_id();

        let removed = provider.remove_identity(&agent_id);

        assert!(removed.is_some());
        assert_eq!(provider.count(), 0);
    }

    #[test]
    fn test_identity_provider_list() {
        let mut provider = IdentityProvider::new();
        provider.create_identity();
        provider.create_identity();

        let list = provider.list_identities();
        assert_eq!(list.len(), 2);
    }
}
