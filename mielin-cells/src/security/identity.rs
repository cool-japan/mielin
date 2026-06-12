//! Agent identity management with cryptographic keys
//!
//! This module provides Ed25519-based identity for agents.

use crate::CellError;
use oxicrypto_core::{Signer, Verifier};
use oxicrypto_sig::{Ed25519, Ed25519Verifier};
use serde::{Deserialize, Serialize};

/// Agent identity with public/private key pair
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    /// Agent ID (derived from public key)
    agent_id: [u8; 16],
    /// Private key — the raw 32-byte Ed25519 seed (secret scalar source).
    private_key: Vec<u8>,
    /// Public key (Ed25519, 32-byte compressed Edwards-y point)
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
        let mut rng = oxicrypto_rand::OxiRng::new().expect("Failed to initialise CSPRNG");
        let (seed, public) = oxicrypto_sig::ed25519_generate_keypair(&mut rng)
            .expect("Failed to generate Ed25519 key pair");

        let public_key = public.to_vec();
        let private_key = seed.as_bytes().to_vec();

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

    /// Create identity from existing key material.
    ///
    /// `seed_bytes` must be the raw 32-byte Ed25519 seed (the same value
    /// returned by [`AgentIdentity::export_private_key`]). The matching public
    /// key is re-derived from the seed.
    pub fn from_pkcs8(seed_bytes: &[u8]) -> Result<Self, CellError> {
        let seed: [u8; 32] = seed_bytes.try_into().map_err(|_| {
            CellError::InvalidState(format!(
                "Ed25519 seed must be 32 bytes, got {}",
                seed_bytes.len()
            ))
        })?;

        let public = derive_ed25519_public_key(&seed)
            .map_err(|e| CellError::InvalidState(format!("Invalid Ed25519 seed: {}", e)))?;

        let public_key = public.to_vec();
        let private_key = seed.to_vec();
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
        let hash = oxicrypto_hash::Sha256.hash_fixed(public_key);
        let mut agent_id = [0u8; 16];
        agent_id.copy_from_slice(&hash[0..16]);
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
        let mut signature = [0u8; 64];
        let len = Ed25519
            .sign(&self.private_key, message, &mut signature)
            .map_err(|e| CellError::InvalidState(format!("Signing error: {}", e)))?;
        Ok(signature[..len].to_vec())
    }

    /// Verify a signature on a message
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<bool, CellError> {
        match Ed25519Verifier.verify(&self.public_key, message, signature) {
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
        match Ed25519Verifier.verify(&self.public_key, message, signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Derive the 32-byte Ed25519 public key (compressed Edwards-y point) from a
/// raw 32-byte seed.
///
/// Re-uses the exact key-derivation path of `oxicrypto_sig::ed25519_generate_keypair`
/// by feeding the fixed seed through a one-shot deterministic RNG, avoiding any
/// direct dependency on the underlying `ed25519-dalek` primitive.
fn derive_ed25519_public_key(seed: &[u8; 32]) -> Result<[u8; 32], oxicrypto_core::CryptoError> {
    /// One-shot RNG that emits a fixed 32-byte seed, then errors on overrun.
    struct SeedRng {
        seed: [u8; 32],
        consumed: bool,
    }

    impl rand_core::TryRng for SeedRng {
        type Error = oxicrypto_core::CryptoError;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            let mut buf = [0u8; 4];
            self.try_fill_bytes(&mut buf)?;
            Ok(u32::from_le_bytes(buf))
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            let mut buf = [0u8; 8];
            self.try_fill_bytes(&mut buf)?;
            Ok(u64::from_le_bytes(buf))
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
            // `ed25519_generate_keypair` fills exactly one 32-byte seed.
            if self.consumed || dest.len() != self.seed.len() {
                return Err(oxicrypto_core::CryptoError::Rng);
            }
            dest.copy_from_slice(&self.seed);
            self.consumed = true;
            Ok(())
        }
    }

    impl rand_core::TryCryptoRng for SeedRng {}

    let mut rng = SeedRng {
        seed: *seed,
        consumed: false,
    };
    let (_sk, pk) = oxicrypto_sig::ed25519_generate_keypair(&mut rng)?;
    Ok(pk)
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

    // ── Known-answer test: Ed25519, RFC 8032 §7.1 (TEST 2) ───────────────────
    //
    // Fixed seed → fixed public key → sign a known message → verify the exact
    // expected signature bytes. This pins the migrated oxicrypto-sig Ed25519
    // primitive to the RFC 8032 reference vector.
    //
    //   sk (seed) = 4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb
    //   pk        = 3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c
    //   msg       = 72
    //   sig       = 92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da
    //               085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00

    fn hex_to_vec(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    #[test]
    fn ed25519_rfc8032_test2_known_answer() {
        let seed = hex_to_vec("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb");
        let expected_pk =
            hex_to_vec("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c");
        let message = hex_to_vec("72");
        let expected_sig = hex_to_vec(
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
             085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
        );

        // Reconstruct identity from the fixed seed; the public key must match.
        let identity = AgentIdentity::from_pkcs8(&seed).expect("import seed");
        assert_eq!(
            identity.public_key(),
            expected_pk.as_slice(),
            "RFC 8032 public key derivation mismatch"
        );

        // Signing the known message must reproduce the exact RFC 8032 signature.
        let sig = identity.sign(&message).expect("sign");
        assert_eq!(
            sig, expected_sig,
            "RFC 8032 §7.1 TEST 2 signature bytes mismatch"
        );

        // The expected signature must verify, and a tampered one must be rejected.
        assert!(identity.verify(&message, &expected_sig).expect("verify"));
        let mut bad_sig = expected_sig.clone();
        bad_sig[0] ^= 0xff;
        assert!(
            !identity
                .verify(&message, &bad_sig)
                .expect("verify tampered"),
            "tampered signature must not verify"
        );
    }

    #[test]
    fn ed25519_seed_roundtrip_export_import() {
        // export_private_key() now yields the raw 32-byte seed; re-importing it
        // must reproduce the same agent ID, public key, and signing behaviour.
        let identity = AgentIdentity::generate();
        let seed = identity.export_private_key().to_vec();
        assert_eq!(seed.len(), 32, "exported seed must be 32 bytes");

        let imported = AgentIdentity::from_pkcs8(&seed).expect("re-import seed");
        assert_eq!(imported.agent_id(), identity.agent_id());
        assert_eq!(imported.public_key(), identity.public_key());

        let msg = b"seed roundtrip message";
        let sig = imported.sign(msg).expect("sign with imported key");
        assert!(identity.verify(msg, &sig).expect("cross-verify"));
    }
}
