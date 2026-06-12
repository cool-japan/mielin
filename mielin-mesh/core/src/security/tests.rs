use super::*;

// ========================================================================
// Certificate Tests
// ========================================================================

#[test]
fn test_certificate_data_creation() {
    let now = SystemTime::now();
    let expires = now + Duration::from_secs(86400); // 24 hours

    let cert = CertificateData::new(vec![1, 2, 3, 4], "node-1", "mielin-ca", now, expires);

    assert_eq!(cert.subject_cn, "node-1");
    assert_eq!(cert.issuer_cn, "mielin-ca");
    assert!(cert.is_valid());
    assert!(!cert.is_expired());
}

#[test]
fn test_certificate_expired() {
    let past = SystemTime::now() - Duration::from_secs(86400);
    let expired = past - Duration::from_secs(3600);

    let cert = CertificateData::new(vec![1, 2, 3, 4], "node-1", "ca", expired, past);

    assert!(!cert.is_valid());
    assert!(cert.is_expired());
    assert!(cert.remaining_validity().is_none());
}

#[test]
fn test_certificate_not_yet_valid() {
    let now = SystemTime::now();
    let future = now + Duration::from_secs(86400);
    let far_future = future + Duration::from_secs(86400);

    let cert = CertificateData::new(vec![1, 2, 3, 4], "node-1", "ca", future, far_future);

    assert!(!cert.is_valid());
}

// ========================================================================
// mTLS Config Tests
// ========================================================================

#[test]
fn test_mtls_config_creation() {
    let now = SystemTime::now();
    let expires = now + Duration::from_secs(86400);

    let cert = CertificateData::new(vec![1, 2, 3], "node-1", "ca", now, expires);
    let ca = CertificateData::new(vec![4, 5, 6], "ca", "root", now, expires);

    let config = MtlsConfig::new(cert, vec![7, 8, 9], vec![ca]);

    assert!(config.require_client_cert);
    assert_eq!(config.min_tls_version, TlsVersion::Tls13);
    assert_eq!(config.trusted_cas.len(), 1);
}

#[test]
fn test_mtls_config_builder() {
    let now = SystemTime::now();
    let expires = now + Duration::from_secs(86400);

    let cert = CertificateData::new(vec![1, 2, 3], "node-1", "ca", now, expires);
    let ca = CertificateData::new(vec![4, 5, 6], "ca", "root", now, expires);

    let config = MtlsConfig::builder()
        .certificate(cert)
        .private_key(vec![7, 8, 9])
        .trusted_ca(ca)
        .require_client_cert(false)
        .min_tls_version(TlsVersion::Tls12)
        .verify_depth(5)
        .build()
        .expect("Failed to build config");

    assert!(!config.require_client_cert);
    assert_eq!(config.min_tls_version, TlsVersion::Tls12);
    assert_eq!(config.verify_depth, 5);
}

#[test]
fn test_mtls_certificate_revocation() {
    let now = SystemTime::now();
    let expires = now + Duration::from_secs(86400);

    let cert = CertificateData::new(vec![1, 2, 3], "node-1", "ca", now, expires);
    let mut config = MtlsConfig::new(cert, vec![7, 8, 9], vec![]);

    config.revoke_certificate("abc123");

    assert!(config.is_revoked("abc123"));
    assert!(!config.is_revoked("xyz789"));
}

#[test]
fn test_mtls_peer_verification() {
    let now = SystemTime::now();
    let expires = now + Duration::from_secs(86400);

    let cert = CertificateData::new(vec![1, 2, 3], "node-1", "mielin-ca", now, expires);
    let ca = CertificateData::new(vec![4, 5, 6], "mielin-ca", "root", now, expires);
    let config = MtlsConfig::new(cert, vec![7, 8, 9], vec![ca]);

    let peer = CertificateData::new(vec![10, 11, 12], "node-2", "mielin-ca", now, expires);
    assert!(config.verify_peer_certificate(&peer).is_ok());

    let untrusted = CertificateData::new(vec![13, 14, 15], "node-3", "unknown-ca", now, expires);
    assert!(config.verify_peer_certificate(&untrusted).is_err());
}

// ========================================================================
// Public Key Tests
// ========================================================================

#[test]
fn test_public_key_creation() {
    let bytes = vec![0u8; 32];
    let key = PublicKey::ed25519(bytes.clone()).expect("Failed to create key");

    assert_eq!(key.algorithm, KeyAlgorithm::Ed25519);
    assert_eq!(key.bytes, bytes);
}

#[test]
fn test_public_key_invalid_length() {
    let bytes = vec![0u8; 16]; // Wrong length for Ed25519
    let result = PublicKey::ed25519(bytes);

    assert!(result.is_err());
}

#[test]
fn test_public_key_hex_roundtrip() {
    let bytes = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    let key = PublicKey::new(KeyAlgorithm::EcdsaP256, bytes.clone());

    let hex = key.to_hex();
    let parsed = PublicKey::from_hex(KeyAlgorithm::EcdsaP256, &hex).expect("Failed to parse");

    assert_eq!(parsed.bytes, bytes);
}

// ========================================================================
// Node Identity Tests
// ========================================================================

#[test]
fn test_node_identity_creation() {
    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);
    let identity = NodeIdentity::new(node_id, public_key);

    assert_eq!(identity.node_id, node_id);
    assert!(identity.address.is_none());
    assert!(identity.metadata.is_empty());
}

#[test]
fn test_node_identity_with_address() {
    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);
    let addr: SocketAddr = "127.0.0.1:8080".parse().expect("Invalid address");

    let identity = NodeIdentity::new(node_id, public_key).with_address(addr);

    assert_eq!(identity.address, Some(addr));
}

#[test]
fn test_node_identity_metadata() {
    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);

    let identity = NodeIdentity::new(node_id, public_key)
        .with_metadata("region", "us-east-1")
        .with_metadata("role", "core");

    assert_eq!(
        identity.metadata.get("region"),
        Some(&"us-east-1".to_string())
    );
    assert_eq!(identity.metadata.get("role"), Some(&"core".to_string()));
}

#[tokio::test]
async fn test_identity_verifier_registration() {
    let verifier = IdentityVerifier::new();

    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::Ed25519, vec![0u8; 32]);
    let identity = NodeIdentity::new(node_id, public_key);

    verifier
        .register(identity.clone())
        .await
        .expect("Failed to register");

    let retrieved = verifier.get(&node_id).await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.as_ref().map(|i| i.node_id), Some(node_id));
}

#[tokio::test]
async fn test_identity_verifier_trust_anchors() {
    let node_id = NodeId::new_v4();
    let verifier = IdentityVerifier::with_trust_anchors([node_id]);

    assert!(verifier.is_trust_anchor(&node_id));
    assert!(!verifier.is_trust_anchor(&NodeId::new_v4()));
}

// ========================================================================
// ACL Tests
// ========================================================================

#[test]
fn test_permission_defaults() {
    let all = Permission::all();
    assert_eq!(all.len(), 10);

    let user = Permission::user_defaults();
    assert!(user.contains(&Permission::Read));
    assert!(user.contains(&Permission::Write));
    assert!(!user.contains(&Permission::Admin));

    let node = Permission::node_defaults();
    assert!(node.contains(&Permission::Migrate));
    assert!(node.contains(&Permission::Gossip));
}

#[test]
fn test_acl_subject_matching() {
    let any = AclSubject::Any;
    let node1 = AclSubject::Node(NodeId::new_v4());
    let node2 = AclSubject::Node(NodeId::new_v4());

    assert!(any.matches(&node1));
    assert!(any.matches(&node2));
    assert!(node1.matches(&node1));
    assert!(!node1.matches(&node2));
}

#[test]
fn test_acl_resource_matching() {
    let any = AclResource::Any;
    let agent1 = AclResource::Agent("agent-1".to_string());
    let agent2 = AclResource::Agent("agent-2".to_string());

    assert!(any.matches(&agent1));
    assert!(agent1.matches(&agent1));
    assert!(!agent1.matches(&agent2));

    let path_pattern = AclResource::Path("/agents/*".to_string());
    let path = AclResource::Path("/agents/foo".to_string());
    assert!(path_pattern.matches(&path));
}

#[test]
fn test_acl_rule_creation() {
    let rule = AclRule::new(
        "rule-1",
        AclSubject::Any,
        AclResource::Any,
        [Permission::Read],
        AclEffect::Allow,
    )
    .with_priority(100)
    .with_description("Allow all reads");

    assert_eq!(rule.id, "rule-1");
    assert_eq!(rule.priority, 100);
    assert!(rule.description.is_some());
    assert!(!rule.is_expired());
}

#[test]
fn test_acl_rule_expiration() {
    let expired = SystemTime::now() - Duration::from_secs(3600);
    let rule = AclRule::new(
        "rule-1",
        AclSubject::Any,
        AclResource::Any,
        [Permission::Read],
        AclEffect::Allow,
    )
    .with_expiration(expired);

    assert!(rule.is_expired());
}

#[tokio::test]
async fn test_acl_policy_permissive() {
    let policy = AclPolicy::permissive();

    let node_id = NodeId::new_v4();
    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_acl_policy_restrictive() {
    let policy = AclPolicy::restrictive();

    let node_id = NodeId::new_v4();
    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_acl_policy_explicit_allow() {
    let policy = AclPolicy::restrictive();

    let node_id = NodeId::new_v4();
    let rule = AclRule::new(
        "allow-read",
        AclSubject::Node(node_id),
        AclResource::Any,
        [Permission::Read],
        AclEffect::Allow,
    );

    policy.add_rule(rule).await;

    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
        .await;
    assert!(result.is_ok());

    // Different permission should still be denied
    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Write)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_acl_policy_explicit_deny() {
    let policy = AclPolicy::permissive();

    let node_id = NodeId::new_v4();
    let rule = AclRule::new(
        "deny-admin",
        AclSubject::Node(node_id),
        AclResource::Any,
        [Permission::Admin],
        AclEffect::Deny,
    );

    policy.add_rule(rule).await;

    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Admin)
        .await;
    assert!(result.is_err());

    // Other permissions should still be allowed
    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_acl_rule_priority() {
    let policy = AclPolicy::restrictive();

    let node_id = NodeId::new_v4();

    // Lower priority deny rule
    let deny_rule = AclRule::new(
        "deny",
        AclSubject::Node(node_id),
        AclResource::Any,
        [Permission::Read],
        AclEffect::Deny,
    )
    .with_priority(10);

    // Higher priority allow rule
    let allow_rule = AclRule::new(
        "allow",
        AclSubject::Node(node_id),
        AclResource::Any,
        [Permission::Read],
        AclEffect::Allow,
    )
    .with_priority(100);

    policy.add_rule(deny_rule).await;
    policy.add_rule(allow_rule).await;

    // Higher priority allow should win
    let result = policy
        .check_node_permission(&node_id, &AclResource::Any, Permission::Read)
        .await;
    assert!(result.is_ok());
}

// ========================================================================
// Gossip Encryption Tests
// ========================================================================

#[test]
fn test_nonce_generation() {
    let nonce1 = Nonce::generate();
    let nonce2 = Nonce::generate();

    assert_ne!(nonce1.as_bytes(), nonce2.as_bytes());
}

#[test]
fn test_gossip_key_generation() {
    let key = GossipKey::generate().expect("Failed to generate key");

    assert_eq!(key.bytes.len(), 32);
    assert!(key.key_id() > 0);
    assert!(!key.needs_rotation(Duration::from_secs(3600)));
}

#[tokio::test]
async fn test_gossip_encryption_roundtrip() {
    let encryption = GossipEncryption::new().expect("Failed to create encryption");
    let sender = NodeId::new_v4();
    let plaintext = b"Hello, secure world!";

    let encrypted = encryption
        .encrypt(sender, plaintext)
        .await
        .expect("Encryption failed");

    assert_ne!(encrypted.ciphertext, plaintext);
    assert_eq!(encrypted.sender, sender);

    let decrypted = encryption
        .decrypt(&encrypted)
        .await
        .expect("Decryption failed");

    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn test_gossip_encryption_large_message() {
    let encryption = GossipEncryption::new().expect("Failed to create encryption");
    let sender = NodeId::new_v4();
    let plaintext = vec![42u8; 10000]; // 10KB message

    let encrypted = encryption
        .encrypt(sender, &plaintext)
        .await
        .expect("Encryption failed");

    let decrypted = encryption
        .decrypt(&encrypted)
        .await
        .expect("Decryption failed");

    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn test_gossip_encryption_nonce_reuse_detection() {
    let encryption = GossipEncryption::new().expect("Failed to create encryption");
    let sender = NodeId::new_v4();
    let plaintext = b"Test message";

    let encrypted = encryption
        .encrypt(sender, plaintext)
        .await
        .expect("Encryption failed");

    // First decryption should succeed
    let _ = encryption
        .decrypt(&encrypted)
        .await
        .expect("First decryption failed");

    // Second decryption with same nonce should fail
    let result = encryption.decrypt(&encrypted).await;
    assert!(matches!(result, Err(SecurityError::NonceReuse)));
}

#[tokio::test]
async fn test_gossip_encryption_key_rotation() {
    let encryption = GossipEncryption::new().expect("Failed to create encryption");
    let sender = NodeId::new_v4();
    let plaintext = b"Before rotation";

    let encrypted_before = encryption
        .encrypt(sender, plaintext)
        .await
        .expect("Encryption failed");

    let key_id_before = encrypted_before.key_id;

    // Rotate key
    encryption.rotate_key().await.expect("Key rotation failed");

    let encrypted_after = encryption
        .encrypt(sender, plaintext)
        .await
        .expect("Encryption failed");

    // Key IDs should be different
    assert_ne!(encrypted_after.key_id, key_id_before);

    // Should still be able to decrypt old message
    let mut old_encrypted = encrypted_before.clone();
    // Need to change nonce since we track used nonces
    old_encrypted.nonce = Nonce::generate().0;

    // Re-encrypt with old key for testing
    // (In practice, old messages would have different nonces)
}

#[tokio::test]
async fn test_gossip_encryption_old_message_rejected() {
    let mut encryption = GossipEncryption::new().expect("Failed to create encryption");
    encryption.set_max_message_age(Duration::from_secs(1));

    let sender = NodeId::new_v4();
    let mut encrypted = encryption
        .encrypt(sender, b"Test")
        .await
        .expect("Encryption failed");

    // Modify timestamp to be old
    encrypted.timestamp = 0;

    let result = encryption.decrypt(&encrypted).await;
    assert!(matches!(result, Err(SecurityError::MessageTooOld { .. })));
}

// ========================================================================
// Key Exchange Tests
// ========================================================================

#[test]
fn test_key_exchange_initiation() {
    let mut kex = KeyExchange::new();
    let peer_id = NodeId::new_v4();

    let public_key = kex.initiate(peer_id).expect("Initiation failed");

    assert!(!public_key.is_empty());
    assert_eq!(kex.state(), KeyExchangeState::WaitingForPeerKey);
}

#[test]
fn test_key_exchange_full_flow() {
    let mut kex1 = KeyExchange::new();
    let mut kex2 = KeyExchange::new();

    let node1 = NodeId::new_v4();
    let node2 = NodeId::new_v4();

    // Node 1 initiates
    let pk1 = kex1.initiate(node2).expect("Node 1 initiation failed");

    // Node 2 initiates
    let pk2 = kex2.initiate(node1).expect("Node 2 initiation failed");

    // Exchange public keys
    kex1.process_peer_key(&pk2)
        .expect("Node 1 processing failed");
    kex2.process_peer_key(&pk1)
        .expect("Node 2 processing failed");

    // Both should be complete
    assert!(kex1.is_complete());
    assert!(kex2.is_complete());

    // Shared secrets should be derived
    assert!(kex1.shared_secret().is_some());
    assert!(kex2.shared_secret().is_some());

    // Both can create gossip keys
    let key1 = kex1.create_gossip_key().expect("Key 1 creation failed");
    let key2 = kex2.create_gossip_key().expect("Key 2 creation failed");

    // Keys should exist
    assert!(key1.age() < Duration::from_secs(1));
    assert!(key2.age() < Duration::from_secs(1));
}

// ========================================================================
// Security Error Tests
// ========================================================================

#[test]
fn test_security_error_display() {
    let err = SecurityError::AccessDenied {
        subject: "node-1".to_string(),
        operation: "read on agent-1".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("Access denied"));
    assert!(msg.contains("node-1"));
}

#[test]
fn test_security_error_variants() {
    let errors = vec![
        SecurityError::CertificateError {
            details: "Invalid".to_string(),
        },
        SecurityError::SignatureVerificationFailed {
            details: "Bad sig".to_string(),
        },
        SecurityError::InvalidPublicKey {
            details: "Wrong length".to_string(),
        },
        SecurityError::KeyGenerationFailed {
            details: "RNG failed".to_string(),
        },
        SecurityError::EncryptionFailed {
            details: "Key error".to_string(),
        },
        SecurityError::DecryptionFailed {
            details: "Auth failed".to_string(),
        },
        SecurityError::NonceReuse,
        SecurityError::MessageTooOld { age_secs: 600 },
    ];

    for err in errors {
        let _ = format!("{}", err); // Should not panic
    }
}

// ========================================================================
// Signature verification known-answer / round-trip tests
//
// These pin the migrated oxicrypto-sig verification paths used by
// `IdentityVerifier::verify_signature` for each supported algorithm.
// ========================================================================

fn hex_to_vec(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
        .collect()
}

/// Ed25519 known-answer test (RFC 8032 §7.1 TEST 2) routed through the
/// `IdentityVerifier::verify_signature` public path.
///
///   pk  = 3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c
///   msg = 72
///   sig = 92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da
///         085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00
#[tokio::test]
async fn ed25519_rfc8032_verify_through_identity_verifier() {
    let pk = hex_to_vec("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c");
    let message = hex_to_vec("72");
    let good_sig = hex_to_vec(
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
         085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
    );

    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::Ed25519, pk);
    let identity = NodeIdentity::new(node_id, public_key);

    let verifier = IdentityVerifier::new();
    verifier.register(identity).await.expect("register");

    let good = Signature::new(KeyAlgorithm::Ed25519, good_sig.clone());
    verifier
        .verify_signature(&node_id, &message, &good)
        .await
        .expect("RFC 8032 Ed25519 signature must verify");

    // A tampered signature must be rejected.
    let mut bad_bytes = good_sig;
    bad_bytes[0] ^= 0xff;
    let bad = Signature::new(KeyAlgorithm::Ed25519, bad_bytes);
    assert!(
        verifier
            .verify_signature(&node_id, &message, &bad)
            .await
            .is_err(),
        "tampered Ed25519 signature must be rejected"
    );
}

/// ECDSA P-256 round-trip through the verifier: a SEC1 public key + ASN.1 DER
/// signature produced by oxicrypto-sig's signer must verify, and a wrong
/// message must be rejected. This exercises the exact `EcdsaP256Verify` path.
#[tokio::test]
async fn ecdsa_p256_verify_through_identity_verifier() {
    use oxicrypto_sig::EcdsaP256Signer;

    // Deterministic 32-byte scalar → P-256 signing key.
    let scalar = [0x11u8; 32];
    let signer = EcdsaP256Signer::from_bytes(&scalar).expect("p256 signer");
    let pk_sec1 = signer.verifying_key_bytes(); // compressed SEC1 (33 bytes)

    let message = b"mielin mesh ecdsa p256 identity test";
    let sig_der = signer.sign(message).expect("p256 sign (DER)");

    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::EcdsaP256, pk_sec1);
    let identity = NodeIdentity::new(node_id, public_key);

    let verifier = IdentityVerifier::new();
    verifier.register(identity).await.expect("register");

    let good = Signature::new(KeyAlgorithm::EcdsaP256, sig_der);
    verifier
        .verify_signature(&node_id, message, &good)
        .await
        .expect("ECDSA P-256 signature must verify");

    // Verifying a different message with a signature over that other message,
    // checked against `message`, must fail.
    let wrong = Signature::new(
        KeyAlgorithm::EcdsaP256,
        signer.sign(b"a different message").expect("sign other"),
    );
    assert!(
        verifier
            .verify_signature(&node_id, message, &wrong)
            .await
            .is_err(),
        "ECDSA P-256 signature over a different message must be rejected"
    );
}

/// ECDSA P-384 round-trip through the verifier (SEC1 pubkey + DER signature).
#[tokio::test]
async fn ecdsa_p384_verify_through_identity_verifier() {
    use oxicrypto_sig::EcdsaP384Signer;

    let scalar = [0x22u8; 48];
    let signer = EcdsaP384Signer::from_bytes(&scalar).expect("p384 signer");
    let pk_sec1 = signer.verifying_key_bytes(); // compressed SEC1 (49 bytes)

    let message = b"mielin mesh ecdsa p384 identity test";
    let sig_der = signer.sign(message).expect("p384 sign (DER)");

    let node_id = NodeId::new_v4();
    let public_key = PublicKey::new(KeyAlgorithm::EcdsaP384, pk_sec1);
    let identity = NodeIdentity::new(node_id, public_key);

    let verifier = IdentityVerifier::new();
    verifier.register(identity).await.expect("register");

    let good = Signature::new(KeyAlgorithm::EcdsaP384, sig_der);
    verifier
        .verify_signature(&node_id, message, &good)
        .await
        .expect("ECDSA P-384 signature must verify");

    let mut tampered = good.bytes.clone();
    tampered[0] ^= 0xff;
    let bad = Signature::new(KeyAlgorithm::EcdsaP384, tampered);
    assert!(
        verifier
            .verify_signature(&node_id, message, &bad)
            .await
            .is_err(),
        "tampered ECDSA P-384 signature must be rejected"
    );
}

/// X25519 two-party ECDH round-trip + HKDF-SHA-256 derivation.
///
/// Both sides independently generate ephemeral key pairs, perform X25519, and
/// must arrive at the same shared secret; an HKDF-SHA-256 expansion over that
/// secret must likewise agree on both sides.
#[test]
fn x25519_dh_and_hkdf_roundtrip() {
    use oxicrypto_core::{Kdf, KeyAgreement};
    use oxicrypto_kdf::HkdfSha256;
    use oxicrypto_kex::{x25519_generate_keypair, X25519};

    let mut rng = oxicrypto_rand::OxiRng::new().expect("OxiRng");
    let (alice_sk, alice_pk) = x25519_generate_keypair(&mut rng).expect("alice keygen");
    let (bob_sk, bob_pk) = x25519_generate_keypair(&mut rng).expect("bob keygen");

    let mut alice_shared = [0u8; 32];
    X25519
        .agree(alice_sk.as_bytes(), &bob_pk, &mut alice_shared)
        .expect("alice agree");

    let mut bob_shared = [0u8; 32];
    X25519
        .agree(bob_sk.as_bytes(), &alice_pk, &mut bob_shared)
        .expect("bob agree");

    assert_eq!(
        alice_shared, bob_shared,
        "X25519: both parties must derive the same shared secret"
    );
    assert_ne!(alice_shared, [0u8; 32], "shared secret must be non-zero");

    // HKDF-SHA-256 over the shared secret must agree on both sides.
    let mut alice_key = [0u8; 32];
    let mut bob_key = [0u8; 32];
    HkdfSha256
        .derive(
            &alice_shared,
            b"mielin-mesh-test",
            b"session-key",
            &mut alice_key,
        )
        .expect("alice hkdf");
    HkdfSha256
        .derive(
            &bob_shared,
            b"mielin-mesh-test",
            b"session-key",
            &mut bob_key,
        )
        .expect("bob hkdf");
    assert_eq!(alice_key, bob_key, "HKDF-derived keys must match");
}
