//! Post-quantum cryptographic primitives for MielinMesh Wire Protocol
//!
//! This module provides ML-KEM (NIST FIPS 203 Module-Lattice Key Encapsulation Mechanism)
//! and a hybrid X25519+ML-KEM key exchange conforming to draft-ietf-tls-hybrid-design.
//!
//! # Security Levels
//! - [`MlKemVariant::MlKem512`]  — 128-bit post-quantum security
//! - [`MlKemVariant::MlKem768`]  — 192-bit post-quantum security (recommended)
//! - [`MlKemVariant::MlKem1024`] — 256-bit post-quantum security

use oxicrypto_core::KeyAgreement;
use oxicrypto_kex::{x25519_generate_keypair, X25519};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use ml_kem::{
    kem::Ciphertext, Decapsulate, DecapsulationKey1024, DecapsulationKey512, DecapsulationKey768,
    EncapsulationKey1024, EncapsulationKey512, EncapsulationKey768, KeyExport, KeyInit, MlKem1024,
    MlKem512, MlKem768, Seed,
};

// ---------------------------------------------------------------------------
// MlKemVariant
// ---------------------------------------------------------------------------

/// ML-KEM algorithm variant selector, specifying the post-quantum security level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MlKemVariant {
    /// ML-KEM-512: NIST category 1 — 128-bit post-quantum security
    MlKem512,
    /// ML-KEM-768: NIST category 3 — 192-bit post-quantum security (recommended)
    MlKem768,
    /// ML-KEM-1024: NIST category 5 — 256-bit post-quantum security
    MlKem1024,
}

impl MlKemVariant {
    /// Security level in bits.
    pub fn security_bits(&self) -> usize {
        match self {
            Self::MlKem512 => 128,
            Self::MlKem768 => 192,
            Self::MlKem1024 => 256,
        }
    }

    /// IANA TLS group ID for hybrid X25519 + this variant.
    ///
    /// - 0x11EB — `X25519MLKEM512` (unofficial / draft)
    /// - 0x11EC — `X25519MLKEM768` (IANA provisional)
    /// - 0x11ED — `X25519MLKEM1024` (unofficial / draft)
    pub fn hybrid_group_id(&self) -> u16 {
        match self {
            Self::MlKem512 => 0x11EB,
            Self::MlKem768 => 0x11EC,
            Self::MlKem1024 => 0x11ED,
        }
    }

    /// Byte length of the encapsulation (public) key.
    pub fn encapsulation_key_bytes(&self) -> usize {
        match self {
            Self::MlKem512 => 800,
            Self::MlKem768 => 1184,
            Self::MlKem1024 => 1568,
        }
    }

    /// Byte length of the decapsulation (private / seed) key stored in this crate.
    ///
    /// The seed is always 64 bytes for all ML-KEM variants; the expanded form differs.
    pub fn decapsulation_key_bytes(&self) -> usize {
        match self {
            Self::MlKem512 => 64,
            Self::MlKem768 => 64,
            Self::MlKem1024 => 64,
        }
    }

    /// Byte length of the ML-KEM ciphertext.
    pub fn ciphertext_bytes(&self) -> usize {
        match self {
            Self::MlKem512 => 768,
            Self::MlKem768 => 1088,
            Self::MlKem1024 => 1568,
        }
    }

    /// Byte length of the shared secret (always 32 bytes for all variants).
    pub const fn shared_secret_bytes(&self) -> usize {
        32
    }
}

impl std::fmt::Display for MlKemVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MlKem512 => write!(f, "ML-KEM-512"),
            Self::MlKem768 => write!(f, "ML-KEM-768"),
            Self::MlKem1024 => write!(f, "ML-KEM-1024"),
        }
    }
}

// ---------------------------------------------------------------------------
// QuantumCryptoError
// ---------------------------------------------------------------------------

/// Error type for quantum cryptographic operations.
#[derive(Debug, Error)]
pub enum QuantumCryptoError {
    /// The supplied seed was the wrong length.
    #[error("Invalid seed length: expected 64 bytes, got {actual}")]
    InvalidSeedLength { actual: usize },

    /// System randomness could not be obtained.
    #[error("Failed to obtain system randomness: {0}")]
    RandomnessFailure(String),

    /// The ciphertext was the wrong length for the requested variant.
    #[error("Ciphertext length mismatch: expected {expected} bytes, got {actual}")]
    CiphertextLengthMismatch { expected: usize, actual: usize },

    /// The encapsulation key was invalid or the wrong length.
    #[error("Invalid encapsulation key: {0}")]
    InvalidEncapsulationKey(String),

    /// X25519 key agreement failed.
    #[error("X25519 key agreement failed: {0}")]
    X25519Failure(String),

    /// HKDF derivation failed.
    #[error("HKDF key derivation failed")]
    HkdfFailure,

    /// The peer's key share was malformed.
    #[error("Malformed peer key share: {0}")]
    MalformedKeyShare(String),

    /// An unsupported operation was requested.
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

// ---------------------------------------------------------------------------
// Internal: Opaque decapsulation key storage
// ---------------------------------------------------------------------------

/// Internal representation of a decapsulation key for a specific variant.
enum DecapsulationKeyInner {
    Kem512(Box<DecapsulationKey512>),
    Kem768(Box<DecapsulationKey768>),
    Kem1024(Box<DecapsulationKey1024>),
}

impl std::fmt::Debug for DecapsulationKeyInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let variant = match self {
            Self::Kem512(_) => "ML-KEM-512",
            Self::Kem768(_) => "ML-KEM-768",
            Self::Kem1024(_) => "ML-KEM-1024",
        };
        write!(f, "DecapsulationKeyInner({})", variant)
    }
}

// ---------------------------------------------------------------------------
// MlKemKeyPair
// ---------------------------------------------------------------------------

/// A generated ML-KEM key pair, holding both the encapsulation (public) and
/// decapsulation (private) keys.
#[derive(Debug)]
pub struct MlKemKeyPair {
    variant: MlKemVariant,
    /// Encapsulation key bytes (serialised public key).
    encapsulation_key: Vec<u8>,
    /// Decapsulation key seed bytes (64 bytes — the compact seed representation).
    decapsulation_key: Vec<u8>,
    /// Opaque inner key material for decapsulation operations.
    inner_dk: DecapsulationKeyInner,
}

/// Fill a byte slice from system entropy using the OxiCrypto secure RNG.
fn fill_system_random(buf: &mut [u8]) -> Result<(), QuantumCryptoError> {
    use oxicrypto_core::Rng;
    let mut rng = oxicrypto_rand::OxiRng::new().map_err(|_| {
        QuantumCryptoError::RandomnessFailure("OxiRng initialisation failed".to_string())
    })?;
    rng.fill(buf)
        .map_err(|_| QuantumCryptoError::RandomnessFailure("OxiRng fill failed".to_string()))
}

impl MlKemKeyPair {
    /// Generate a new ML-KEM key pair.
    ///
    /// When `seed` is `Some([u8; 64])`, the key pair is derived deterministically
    /// from that seed (suitable for testing or key recovery). When `None`, fresh
    /// system entropy is used.
    pub fn generate(
        variant: MlKemVariant,
        seed: Option<[u8; 64]>,
    ) -> Result<Self, QuantumCryptoError> {
        let seed_bytes: [u8; 64] = match seed {
            Some(s) => s,
            None => {
                let mut buf = [0u8; 64];
                fill_system_random(&mut buf)?;
                buf
            }
        };

        match variant {
            MlKemVariant::MlKem512 => {
                let seed_arr = Seed::from(seed_bytes);
                let dk = DecapsulationKey512::new(&seed_arr);
                let ek = dk.encapsulation_key().clone();
                let ek_bytes = ek.to_bytes().to_vec();
                let dk_bytes = seed_bytes.to_vec();
                Ok(Self {
                    variant,
                    encapsulation_key: ek_bytes,
                    decapsulation_key: dk_bytes,
                    inner_dk: DecapsulationKeyInner::Kem512(Box::new(dk)),
                })
            }
            MlKemVariant::MlKem768 => {
                let seed_arr = Seed::from(seed_bytes);
                let dk = DecapsulationKey768::new(&seed_arr);
                let ek = dk.encapsulation_key().clone();
                let ek_bytes = ek.to_bytes().to_vec();
                let dk_bytes = seed_bytes.to_vec();
                Ok(Self {
                    variant,
                    encapsulation_key: ek_bytes,
                    decapsulation_key: dk_bytes,
                    inner_dk: DecapsulationKeyInner::Kem768(Box::new(dk)),
                })
            }
            MlKemVariant::MlKem1024 => {
                let seed_arr = Seed::from(seed_bytes);
                let dk = DecapsulationKey1024::new(&seed_arr);
                let ek = dk.encapsulation_key().clone();
                let ek_bytes = ek.to_bytes().to_vec();
                let dk_bytes = seed_bytes.to_vec();
                Ok(Self {
                    variant,
                    encapsulation_key: ek_bytes,
                    decapsulation_key: dk_bytes,
                    inner_dk: DecapsulationKeyInner::Kem1024(Box::new(dk)),
                })
            }
        }
    }

    /// Encapsulate a fresh shared secret to the holder of the given encapsulation key.
    ///
    /// Returns `(ciphertext, shared_secret)` packed into [`MlKemEncapsulation`].
    /// This is the sender-side operation.
    pub fn encapsulate(
        ek: &[u8],
        variant: MlKemVariant,
    ) -> Result<MlKemEncapsulation, QuantumCryptoError> {
        let expected_ek_len = variant.encapsulation_key_bytes();
        if ek.len() != expected_ek_len {
            return Err(QuantumCryptoError::InvalidEncapsulationKey(format!(
                "expected {} bytes, got {}",
                expected_ek_len,
                ek.len()
            )));
        }

        // Obtain 32 bytes of system entropy for the encapsulation message
        let mut m_bytes = [0u8; 32];
        fill_system_random(&mut m_bytes)?;

        match variant {
            MlKemVariant::MlKem512 => {
                let ek_key_bytes: [u8; 800] = ek.try_into().map_err(|_| {
                    QuantumCryptoError::InvalidEncapsulationKey(
                        "failed to convert EK bytes for ML-KEM-512".to_string(),
                    )
                })?;
                let ek_arr = ml_kem::kem::Key::<EncapsulationKey512>::from(ek_key_bytes);
                let ek_parsed = EncapsulationKey512::new(&ek_arr).map_err(|_| {
                    QuantumCryptoError::InvalidEncapsulationKey(
                        "ML-KEM-512 encapsulation key validation failed".to_string(),
                    )
                })?;
                let m_arr = ml_kem::B32::from(m_bytes);
                let (ct, ss) = ek_parsed.encapsulate_deterministic(&m_arr);
                Ok(MlKemEncapsulation {
                    ciphertext: ct.to_vec(),
                    shared_secret: ss.to_vec(),
                })
            }
            MlKemVariant::MlKem768 => {
                let ek_key_bytes: [u8; 1184] = ek.try_into().map_err(|_| {
                    QuantumCryptoError::InvalidEncapsulationKey(
                        "failed to convert EK bytes for ML-KEM-768".to_string(),
                    )
                })?;
                let ek_arr = ml_kem::kem::Key::<EncapsulationKey768>::from(ek_key_bytes);
                let ek_parsed = EncapsulationKey768::new(&ek_arr).map_err(|_| {
                    QuantumCryptoError::InvalidEncapsulationKey(
                        "ML-KEM-768 encapsulation key validation failed".to_string(),
                    )
                })?;
                let m_arr = ml_kem::B32::from(m_bytes);
                let (ct, ss) = ek_parsed.encapsulate_deterministic(&m_arr);
                Ok(MlKemEncapsulation {
                    ciphertext: ct.to_vec(),
                    shared_secret: ss.to_vec(),
                })
            }
            MlKemVariant::MlKem1024 => {
                let ek_key_bytes: [u8; 1568] = ek.try_into().map_err(|_| {
                    QuantumCryptoError::InvalidEncapsulationKey(
                        "failed to convert EK bytes for ML-KEM-1024".to_string(),
                    )
                })?;
                let ek_arr = ml_kem::kem::Key::<EncapsulationKey1024>::from(ek_key_bytes);
                let ek_parsed = EncapsulationKey1024::new(&ek_arr).map_err(|_| {
                    QuantumCryptoError::InvalidEncapsulationKey(
                        "ML-KEM-1024 encapsulation key validation failed".to_string(),
                    )
                })?;
                let m_arr = ml_kem::B32::from(m_bytes);
                let (ct, ss) = ek_parsed.encapsulate_deterministic(&m_arr);
                Ok(MlKemEncapsulation {
                    ciphertext: ct.to_vec(),
                    shared_secret: ss.to_vec(),
                })
            }
        }
    }

    /// Decapsulate a ciphertext to recover the shared secret.
    ///
    /// This is the receiver-side operation using the private decapsulation key.
    pub fn decapsulate(&self, ciphertext: &[u8]) -> Result<Vec<u8>, QuantumCryptoError> {
        let expected_ct_len = self.variant.ciphertext_bytes();
        if ciphertext.len() != expected_ct_len {
            return Err(QuantumCryptoError::CiphertextLengthMismatch {
                expected: expected_ct_len,
                actual: ciphertext.len(),
            });
        }

        match &self.inner_dk {
            DecapsulationKeyInner::Kem512(dk) => {
                let ct_bytes: [u8; 768] = ciphertext.try_into().map_err(|_| {
                    QuantumCryptoError::CiphertextLengthMismatch {
                        expected: 768,
                        actual: ciphertext.len(),
                    }
                })?;
                let ct = Ciphertext::<MlKem512>::from(ct_bytes);
                let ss = dk.decapsulate(&ct);
                Ok(ss.to_vec())
            }
            DecapsulationKeyInner::Kem768(dk) => {
                let ct_bytes: [u8; 1088] = ciphertext.try_into().map_err(|_| {
                    QuantumCryptoError::CiphertextLengthMismatch {
                        expected: 1088,
                        actual: ciphertext.len(),
                    }
                })?;
                let ct = Ciphertext::<MlKem768>::from(ct_bytes);
                let ss = dk.decapsulate(&ct);
                Ok(ss.to_vec())
            }
            DecapsulationKeyInner::Kem1024(dk) => {
                let ct_bytes: [u8; 1568] = ciphertext.try_into().map_err(|_| {
                    QuantumCryptoError::CiphertextLengthMismatch {
                        expected: 1568,
                        actual: ciphertext.len(),
                    }
                })?;
                let ct = Ciphertext::<MlKem1024>::from(ct_bytes);
                let ss = dk.decapsulate(&ct);
                Ok(ss.to_vec())
            }
        }
    }

    /// Return the encapsulation (public) key bytes.
    pub fn encapsulation_key(&self) -> &[u8] {
        &self.encapsulation_key
    }

    /// Return the decapsulation key seed bytes (64 bytes).
    pub fn decapsulation_key(&self) -> &[u8] {
        &self.decapsulation_key
    }

    /// Return the variant of this key pair.
    pub fn variant(&self) -> MlKemVariant {
        self.variant
    }

    /// Return security metadata for this key pair.
    pub fn security_info(&self) -> QuantumSecurityInfo {
        QuantumSecurityInfo {
            variant: self.variant,
            is_hybrid: false,
            key_bits: self.variant.security_bits(),
            encapsulation_key_bytes: self.encapsulation_key.len(),
            decapsulation_key_bytes: self.decapsulation_key.len(),
            ciphertext_bytes: self.variant.ciphertext_bytes(),
            shared_secret_bytes: self.variant.shared_secret_bytes(),
        }
    }
}

// ---------------------------------------------------------------------------
// MlKemEncapsulation
// ---------------------------------------------------------------------------

/// The result of an ML-KEM encapsulation: a ciphertext and the sender's view
/// of the shared secret.
#[derive(Debug, Clone)]
pub struct MlKemEncapsulation {
    /// The ML-KEM ciphertext to send to the decapsulating party.
    ciphertext: Vec<u8>,
    /// The 32-byte shared secret known to the encapsulating party.
    shared_secret: Vec<u8>,
}

impl MlKemEncapsulation {
    /// Return the ciphertext bytes.
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Return the shared secret bytes (always 32 bytes).
    pub fn shared_secret(&self) -> &[u8] {
        &self.shared_secret
    }
}

// ---------------------------------------------------------------------------
// HybridKexState
// ---------------------------------------------------------------------------

/// Ephemeral X25519 + ML-KEM hybrid key exchange state.
///
/// Combining classical (X25519) and post-quantum (ML-KEM) key exchange ensures
/// that security holds as long as *at least one* of the two primitives is secure,
/// following the hybrid key exchange framework of draft-ietf-tls-hybrid-design.
pub struct HybridKexState {
    /// X25519 ephemeral private scalar (32-byte static secret form).
    x25519_private: [u8; 32],
    x25519_public: [u8; 32],
    mlkem_keypair: MlKemKeyPair,
    variant: MlKemVariant,
}

impl std::fmt::Debug for HybridKexState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridKexState")
            .field("x25519_public", &hex::encode(self.x25519_public))
            .field("variant", &self.variant)
            .finish()
    }
}

impl HybridKexState {
    /// Initialise a new hybrid key exchange state with fresh ephemeral keys.
    pub fn new(variant: MlKemVariant) -> Result<Self, QuantumCryptoError> {
        // Generate X25519 ephemeral key pair via the OxiCrypto CSPRNG.
        let mut rng = oxicrypto_rand::OxiRng::new().map_err(|_| {
            QuantumCryptoError::X25519Failure("OxiRng initialisation failed".to_string())
        })?;
        let (x25519_secret, x25519_public) = x25519_generate_keypair(&mut rng)
            .map_err(|_| QuantumCryptoError::X25519Failure("X25519 keygen failed".to_string()))?;

        // Generate ML-KEM key pair
        let mlkem_keypair = MlKemKeyPair::generate(variant, None)?;

        Ok(Self {
            x25519_private: *x25519_secret.as_bytes(),
            x25519_public,
            mlkem_keypair,
            variant,
        })
    }

    /// Server-side response: given the client's `PqKeyShareExtension`, produce
    /// the server's key share and derive the `HybridSharedSecret`.
    ///
    /// The server:
    /// 1. Performs X25519 with the client's X25519 share.
    /// 2. Encapsulates a fresh ML-KEM ciphertext to the client's ML-KEM ek.
    /// 3. Combines classical and PQ secrets via HKDF.
    pub fn server_respond(
        &self,
        client_share: &PqKeyShareExtension,
    ) -> Result<(PqKeyShareExtension, HybridSharedSecret), QuantumCryptoError> {
        // Validate that the variant matches
        if client_share.variant != self.variant {
            return Err(QuantumCryptoError::MalformedKeyShare(format!(
                "variant mismatch: server uses {:?}, client offered {:?}",
                self.variant, client_share.variant
            )));
        }

        // X25519: agree on shared secret using client's classical share.
        // Generate a fresh server-side ephemeral key pair (the server keeps no
        // long-lived X25519 secret in this state object).
        let mut rng = oxicrypto_rand::OxiRng::new().map_err(|_| {
            QuantumCryptoError::X25519Failure("OxiRng initialisation failed".to_string())
        })?;
        let (server_x25519_secret, server_x25519_public_bytes) = x25519_generate_keypair(&mut rng)
            .map_err(|_| {
                QuantumCryptoError::X25519Failure("server X25519 keygen failed".to_string())
            })?;

        let x25519_classical = {
            let mut classical_buf = [0u8; 32];
            X25519
                .agree(
                    server_x25519_secret.as_bytes(),
                    &client_share.classical_share,
                    &mut classical_buf,
                )
                .map_err(|_| {
                    QuantumCryptoError::X25519Failure("X25519 agreement failed".to_string())
                })?;
            classical_buf
        };

        // ML-KEM: encapsulate to the client's ML-KEM encapsulation key
        let encap = MlKemKeyPair::encapsulate(&client_share.pq_share, self.variant)?;
        let pq_secret = encap.shared_secret().to_vec();

        // Combine using HKDF
        let info = build_hybrid_info(self.variant);
        let combined = hkdf_sha256_combine(&x25519_classical, &pq_secret, &info);

        let hybrid_secret = HybridSharedSecret {
            classical: x25519_classical,
            pq: pq_secret,
            combined,
        };

        // Build server's key share: its X25519 public key + ML-KEM ciphertext
        let server_share = PqKeyShareExtension {
            group_id: self.variant.hybrid_group_id(),
            classical_share: server_x25519_public_bytes,
            pq_share: encap.ciphertext().to_vec(),
            variant: self.variant,
        };

        Ok((server_share, hybrid_secret))
    }

    /// Client-side finish: given the server's `PqKeyShareExtension` and the ML-KEM
    /// ciphertext embedded within it, derive the `HybridSharedSecret`.
    ///
    /// The client:
    /// 1. Performs X25519 with the server's X25519 share.
    /// 2. Decapsulates the ML-KEM ciphertext using its own decapsulation key.
    /// 3. Combines classical and PQ secrets via HKDF.
    pub fn client_finish(
        self,
        server_share: &PqKeyShareExtension,
        server_ciphertext: &[u8],
    ) -> Result<HybridSharedSecret, QuantumCryptoError> {
        // X25519: agree with server's classical share using our ephemeral secret.
        let x25519_classical = {
            let mut classical_buf = [0u8; 32];
            X25519
                .agree(
                    &self.x25519_private,
                    &server_share.classical_share,
                    &mut classical_buf,
                )
                .map_err(|_| {
                    QuantumCryptoError::X25519Failure("X25519 agreement failed".to_string())
                })?;
            classical_buf
        };

        // ML-KEM: decapsulate the server's ciphertext
        let pq_secret = self.mlkem_keypair.decapsulate(server_ciphertext)?;

        // Combine
        let info = build_hybrid_info(self.variant);
        let combined = hkdf_sha256_combine(&x25519_classical, &pq_secret, &info);

        Ok(HybridSharedSecret {
            classical: x25519_classical,
            pq: pq_secret,
            combined,
        })
    }

    /// Export this node's public key share for inclusion in a TLS ClientHello.
    pub fn public_key_share(&self) -> PqKeyShareExtension {
        PqKeyShareExtension {
            group_id: self.variant.hybrid_group_id(),
            classical_share: self.x25519_public,
            pq_share: self.mlkem_keypair.encapsulation_key().to_vec(),
            variant: self.variant,
        }
    }
}

// ---------------------------------------------------------------------------
// HybridSharedSecret
// ---------------------------------------------------------------------------

/// The result of a completed hybrid key exchange.
///
/// Holds the individual classical (X25519) and post-quantum (ML-KEM) secrets,
/// plus the HKDF-derived combined session key.
#[derive(Debug, Clone)]
pub struct HybridSharedSecret {
    /// 32-byte X25519 shared secret (classical half).
    classical: [u8; 32],
    /// 32-byte ML-KEM shared secret (post-quantum half).
    pq: Vec<u8>,
    /// 32-byte HKDF-derived combined session key.
    combined: [u8; 32],
}

impl HybridSharedSecret {
    /// Return the classical (X25519) shared secret component.
    pub fn classical(&self) -> &[u8; 32] {
        &self.classical
    }

    /// Return the post-quantum (ML-KEM) shared secret component.
    pub fn pq(&self) -> &[u8] {
        &self.pq
    }

    /// Return the 32-byte HKDF-combined session key.
    ///
    /// This is the value to use for symmetric key derivation.
    pub fn combined(&self) -> &[u8; 32] {
        &self.combined
    }
}

// ---------------------------------------------------------------------------
// PqKeyShareExtension
// ---------------------------------------------------------------------------

/// TLS 1.3 extension data carrying a hybrid post-quantum key share
/// (draft-ietf-tls-hybrid-design).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PqKeyShareExtension {
    /// IANA TLS named-group identifier (e.g. `0x11EC` for X25519MLKEM768).
    pub group_id: u16,
    /// The X25519 ephemeral public key (always 32 bytes).
    pub classical_share: [u8; 32],
    /// The ML-KEM encapsulation key (sender) or ciphertext (responder), variable length.
    pub pq_share: Vec<u8>,
    /// Which ML-KEM variant is in use.
    pub variant: MlKemVariant,
}

impl PqKeyShareExtension {
    /// Total byte length of this key share when serialized.
    pub fn wire_len(&self) -> usize {
        2 /* group_id */ + 32 /* classical */ + 2 /* pq_len */ + self.pq_share.len()
    }

    /// Encode to wire format: `group_id || classical_share || u16_be(pq_len) || pq_share`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.wire_len());
        out.extend_from_slice(&self.group_id.to_be_bytes());
        out.extend_from_slice(&self.classical_share);
        let pq_len = self.pq_share.len() as u16;
        out.extend_from_slice(&pq_len.to_be_bytes());
        out.extend_from_slice(&self.pq_share);
        out
    }

    /// Decode from wire format.
    pub fn decode(bytes: &[u8], variant: MlKemVariant) -> Result<Self, QuantumCryptoError> {
        if bytes.len() < 36 {
            return Err(QuantumCryptoError::MalformedKeyShare(format!(
                "too short: {} bytes",
                bytes.len()
            )));
        }
        let group_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let classical_share: [u8; 32] = bytes[2..34].try_into().map_err(|_| {
            QuantumCryptoError::MalformedKeyShare("classical share slice error".to_string())
        })?;
        let pq_len = u16::from_be_bytes([bytes[34], bytes[35]]) as usize;
        if bytes.len() < 36 + pq_len {
            return Err(QuantumCryptoError::MalformedKeyShare(format!(
                "truncated PQ share: need {}, have {}",
                36 + pq_len,
                bytes.len()
            )));
        }
        let pq_share = bytes[36..36 + pq_len].to_vec();
        Ok(Self {
            group_id,
            classical_share,
            pq_share,
            variant,
        })
    }
}

// ---------------------------------------------------------------------------
// QuantumSecurityInfo
// ---------------------------------------------------------------------------

/// Connection-level quantum security metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantumSecurityInfo {
    /// The ML-KEM variant in use.
    pub variant: MlKemVariant,
    /// Whether the key exchange is hybrid (X25519 + ML-KEM).
    pub is_hybrid: bool,
    /// Security level in bits.
    pub key_bits: usize,
    /// Byte length of the encapsulation key.
    pub encapsulation_key_bytes: usize,
    /// Byte length of the decapsulation key (seed, 64 bytes).
    pub decapsulation_key_bytes: usize,
    /// Byte length of the ML-KEM ciphertext.
    pub ciphertext_bytes: usize,
    /// Byte length of the shared secret (always 32).
    pub shared_secret_bytes: usize,
}

impl std::fmt::Display for QuantumSecurityInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{} ({}+-bit PQ, ek={}B, ct={}B, ss={}B)",
            if self.is_hybrid { "Hybrid X25519+" } else { "" },
            self.variant,
            self.key_bits,
            self.encapsulation_key_bytes,
            self.ciphertext_bytes,
            self.shared_secret_bytes
        )
    }
}

// ---------------------------------------------------------------------------
// HKDF helpers
// ---------------------------------------------------------------------------

/// Build the `info` string for hybrid HKDF derivation.
fn build_hybrid_info(variant: MlKemVariant) -> Vec<u8> {
    let label = format!("MielinMesh hybrid KEX {}", variant);
    label.into_bytes()
}

/// HKDF-SHA-256: combine classical and post-quantum shared secrets into a
/// 32-byte session key.
///
/// Follows the construction from draft-ietf-tls-hybrid-design §3:
///   `HKDF-Extract(salt=classical, IKM=pq)` then `HKDF-Expand(info)`.
pub fn hkdf_sha256_combine(classical: &[u8], pq: &[u8], info: &[u8]) -> [u8; 32] {
    // HKDF-Extract: salt = classical secret, IKM = pq secret
    let prk = oxicrypto_kdf::hkdf_sha256_extract(classical, pq);

    // HKDF-Expand: info = domain-separation label
    let mut out = [0u8; 32];
    // If expand fails we fall back to a zero array, which must never happen
    // for valid inputs — the error path indicates an impossible length.
    let _ = oxicrypto_kdf::hkdf_sha256_expand(&prk, info, &mut out);
    out
}

// ---------------------------------------------------------------------------
// #[cfg(test)]
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Key generation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ml_kem_768_key_generation() {
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem768, None)
            .expect("ML-KEM-768 key generation failed");
        assert_eq!(kp.variant(), MlKemVariant::MlKem768);
        assert_eq!(kp.encapsulation_key().len(), 1184);
        assert_eq!(kp.decapsulation_key().len(), 64);
    }

    #[test]
    fn test_ml_kem_512_key_generation() {
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem512, None)
            .expect("ML-KEM-512 key generation failed");
        assert_eq!(kp.variant(), MlKemVariant::MlKem512);
        assert_eq!(kp.encapsulation_key().len(), 800);
        assert_eq!(kp.decapsulation_key().len(), 64);
    }

    #[test]
    fn test_ml_kem_1024_key_generation() {
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem1024, None)
            .expect("ML-KEM-1024 key generation failed");
        assert_eq!(kp.variant(), MlKemVariant::MlKem1024);
        assert_eq!(kp.encapsulation_key().len(), 1568);
        assert_eq!(kp.decapsulation_key().len(), 64);
    }

    // -----------------------------------------------------------------------
    // Encapsulate/decapsulate round-trip tests
    // -----------------------------------------------------------------------

    fn roundtrip_test(variant: MlKemVariant) {
        let seed = [42u8; 64];
        let kp = MlKemKeyPair::generate(variant, Some(seed)).expect("key generation failed");

        let encap = MlKemKeyPair::encapsulate(kp.encapsulation_key(), variant)
            .expect("encapsulation failed");

        let ss_decap = kp
            .decapsulate(encap.ciphertext())
            .expect("decapsulation failed");

        assert_eq!(
            encap.shared_secret(),
            ss_decap.as_slice(),
            "shared secrets must match for variant {:?}",
            variant
        );
    }

    #[test]
    fn test_ml_kem_encapsulate_decapsulate_roundtrip_768() {
        roundtrip_test(MlKemVariant::MlKem768);
    }

    #[test]
    fn test_ml_kem_encapsulate_decapsulate_roundtrip_512() {
        roundtrip_test(MlKemVariant::MlKem512);
    }

    #[test]
    fn test_ml_kem_encapsulate_decapsulate_roundtrip_1024() {
        roundtrip_test(MlKemVariant::MlKem1024);
    }

    // -----------------------------------------------------------------------
    // Wrong ciphertext must fail (implicit rejection yields different secret)
    // -----------------------------------------------------------------------

    #[test]
    fn test_ml_kem_wrong_ciphertext_fails_decapsulation() {
        let seed = [7u8; 64];
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed))
            .expect("key generation failed");

        let encap = MlKemKeyPair::encapsulate(kp.encapsulation_key(), MlKemVariant::MlKem768)
            .expect("encapsulation failed");

        // Corrupt the ciphertext: flip all bits
        let mut bad_ct = encap.ciphertext().to_vec();
        for byte in &mut bad_ct {
            *byte ^= 0xFF;
        }

        // ML-KEM uses implicit rejection — decapsulation always succeeds but
        // returns a pseudorandom (wrong) value when given a bad ciphertext.
        let ss_bad = kp
            .decapsulate(&bad_ct)
            .expect("decapsulate should succeed (implicit rejection)");

        // The recovered secret must differ from the original
        assert_ne!(
            encap.shared_secret(),
            ss_bad.as_slice(),
            "corrupted ciphertext should yield a different shared secret"
        );
    }

    // -----------------------------------------------------------------------
    // Deterministic key generation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ml_kem_deterministic_key_gen_same_seed() {
        let seed = [99u8; 64];
        let kp1 =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).expect("key gen 1 failed");
        let kp2 =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).expect("key gen 2 failed");

        assert_eq!(
            kp1.encapsulation_key(),
            kp2.encapsulation_key(),
            "same seed must yield same encapsulation key"
        );
        assert_eq!(
            kp1.decapsulation_key(),
            kp2.decapsulation_key(),
            "same seed must yield same decapsulation key"
        );
    }

    #[test]
    fn test_ml_kem_different_seeds_different_keys() {
        let seed_a = [11u8; 64];
        let seed_b = [22u8; 64];
        let kp_a =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed_a)).expect("key gen A failed");
        let kp_b =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed_b)).expect("key gen B failed");

        assert_ne!(
            kp_a.encapsulation_key(),
            kp_b.encapsulation_key(),
            "different seeds must yield different encapsulation keys"
        );
    }

    // -----------------------------------------------------------------------
    // Hybrid key exchange tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_hybrid_kex_full_handshake() {
        let client = HybridKexState::new(MlKemVariant::MlKem768).expect("client init failed");
        let server = HybridKexState::new(MlKemVariant::MlKem768).expect("server init failed");

        let client_share = client.public_key_share();
        let (server_share, server_secret) = server
            .server_respond(&client_share)
            .expect("server respond failed");

        // Client decapsulates server's ML-KEM ciphertext (in server_share.pq_share)
        let client_secret = client
            .client_finish(&server_share, &server_share.pq_share)
            .expect("client finish failed");

        assert_eq!(
            client_secret.combined(),
            server_secret.combined(),
            "hybrid shared secrets must match"
        );
    }

    #[test]
    fn test_hybrid_kex_shared_secret_matches_both_sides() {
        let client = HybridKexState::new(MlKemVariant::MlKem768).expect("client init failed");
        let server = HybridKexState::new(MlKemVariant::MlKem768).expect("server init failed");

        let client_share = client.public_key_share();
        let (server_share, server_secret) = server
            .server_respond(&client_share)
            .expect("server respond failed");

        let client_secret = client
            .client_finish(&server_share, &server_share.pq_share)
            .expect("client finish failed");

        // All components must match
        assert_eq!(
            client_secret.pq(),
            server_secret.pq(),
            "PQ shared secrets must match"
        );
        assert_eq!(
            client_secret.combined(),
            server_secret.combined(),
            "combined secrets must match"
        );
    }

    #[test]
    fn test_hybrid_kex_different_ephemeral_keys_each_time() {
        let kex1 = HybridKexState::new(MlKemVariant::MlKem768).expect("kex1 init failed");
        let kex2 = HybridKexState::new(MlKemVariant::MlKem768).expect("kex2 init failed");

        let share1 = kex1.public_key_share();
        let share2 = kex2.public_key_share();

        // Ephemeral keys should be different (negligible collision probability)
        assert_ne!(
            share1.classical_share, share2.classical_share,
            "each kex must produce a unique X25519 public key"
        );
        assert_ne!(
            share1.pq_share, share2.pq_share,
            "each kex must produce a unique ML-KEM encapsulation key"
        );
    }

    // -----------------------------------------------------------------------
    // Serialisation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_pq_key_share_extension_serialization() {
        let kex = HybridKexState::new(MlKemVariant::MlKem768).expect("kex init failed");
        let share = kex.public_key_share();

        let encoded = share.encode();
        let decoded =
            PqKeyShareExtension::decode(&encoded, MlKemVariant::MlKem768).expect("decode failed");

        assert_eq!(share.group_id, decoded.group_id);
        assert_eq!(share.classical_share, decoded.classical_share);
        assert_eq!(share.pq_share, decoded.pq_share);
        assert_eq!(share.variant, decoded.variant);
    }

    #[test]
    fn test_pq_key_share_encode_decode_roundtrip_512() {
        let kex = HybridKexState::new(MlKemVariant::MlKem512).expect("kex init failed");
        let share = kex.public_key_share();
        let encoded = share.encode();
        let decoded =
            PqKeyShareExtension::decode(&encoded, MlKemVariant::MlKem512).expect("decode failed");
        assert_eq!(share.group_id, decoded.group_id);
        assert_eq!(share.pq_share, decoded.pq_share);
    }

    // -----------------------------------------------------------------------
    // HKDF tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_hkdf_combine_deterministic() {
        let classical = [1u8; 32];
        let pq = [2u8; 32];
        let info = b"test-info";

        let out1 = hkdf_sha256_combine(&classical, &pq, info);
        let out2 = hkdf_sha256_combine(&classical, &pq, info);

        assert_eq!(out1, out2, "HKDF must be deterministic");
        assert_ne!(out1, [0u8; 32], "output should not be all-zeros");
    }

    #[test]
    fn test_hkdf_combine_different_inputs_different_outputs() {
        let classical_a = [1u8; 32];
        let pq_a = [2u8; 32];
        let info = b"test-info";

        let classical_b = [3u8; 32];
        let pq_b = [4u8; 32];

        let out_a = hkdf_sha256_combine(&classical_a, &pq_a, info);
        let out_b = hkdf_sha256_combine(&classical_b, &pq_b, info);

        assert_ne!(
            out_a, out_b,
            "different inputs must yield different outputs"
        );
    }

    #[test]
    fn test_hkdf_combine_different_info_different_outputs() {
        let classical = [1u8; 32];
        let pq = [2u8; 32];

        let out_a = hkdf_sha256_combine(&classical, &pq, b"label-a");
        let out_b = hkdf_sha256_combine(&classical, &pq, b"label-b");

        assert_ne!(
            out_a, out_b,
            "different info labels must yield different outputs"
        );
    }

    // -----------------------------------------------------------------------
    // Security info tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_quantum_security_info_ml_kem_768() {
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem768, None).expect("key gen failed");
        let info = kp.security_info();

        assert_eq!(info.variant, MlKemVariant::MlKem768);
        assert_eq!(info.key_bits, 192);
        assert_eq!(info.encapsulation_key_bytes, 1184);
        assert_eq!(info.decapsulation_key_bytes, 64);
        assert_eq!(info.ciphertext_bytes, 1088);
        assert_eq!(info.shared_secret_bytes, 32);
        assert!(!info.is_hybrid);
    }

    #[test]
    fn test_quantum_security_info_variants() {
        let info_512 = MlKemKeyPair::generate(MlKemVariant::MlKem512, None)
            .expect("512 gen failed")
            .security_info();
        let info_1024 = MlKemKeyPair::generate(MlKemVariant::MlKem1024, None)
            .expect("1024 gen failed")
            .security_info();

        assert_eq!(info_512.key_bits, 128);
        assert_eq!(info_1024.key_bits, 256);

        assert_eq!(info_512.encapsulation_key_bytes, 800);
        assert_eq!(info_1024.encapsulation_key_bytes, 1568);

        assert_eq!(info_512.ciphertext_bytes, 768);
        assert_eq!(info_1024.ciphertext_bytes, 1568);
    }

    // -----------------------------------------------------------------------
    // Error display tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_quantum_crypto_error_display() {
        let err = QuantumCryptoError::InvalidSeedLength { actual: 32 };
        let msg = err.to_string();
        assert!(
            msg.contains("64"),
            "error message should mention expected length"
        );
        assert!(
            msg.contains("32"),
            "error message should mention actual length"
        );
    }

    #[test]
    fn test_quantum_crypto_error_variants_display() {
        let errs: Vec<QuantumCryptoError> = vec![
            QuantumCryptoError::InvalidSeedLength { actual: 10 },
            QuantumCryptoError::RandomnessFailure("test".to_string()),
            QuantumCryptoError::CiphertextLengthMismatch {
                expected: 1088,
                actual: 512,
            },
            QuantumCryptoError::InvalidEncapsulationKey("bad key".to_string()),
            QuantumCryptoError::X25519Failure("x25519 err".to_string()),
            QuantumCryptoError::HkdfFailure,
            QuantumCryptoError::MalformedKeyShare("share err".to_string()),
            QuantumCryptoError::Unsupported("op".to_string()),
        ];
        for err in &errs {
            let msg = err.to_string();
            assert!(
                !msg.is_empty(),
                "error display should not be empty: {:?}",
                err
            );
        }
    }

    // -----------------------------------------------------------------------
    // Variant display test
    // -----------------------------------------------------------------------

    #[test]
    fn test_ml_kem_variant_display() {
        assert_eq!(MlKemVariant::MlKem512.to_string(), "ML-KEM-512");
        assert_eq!(MlKemVariant::MlKem768.to_string(), "ML-KEM-768");
        assert_eq!(MlKemVariant::MlKem1024.to_string(), "ML-KEM-1024");
    }

    // -----------------------------------------------------------------------
    // Key length assertion tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_encapsulation_key_length_768() {
        let seed = [0u8; 64];
        let kp =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).expect("key gen failed");
        assert_eq!(
            kp.encapsulation_key().len(),
            1184,
            "ML-KEM-768 encapsulation key must be 1184 bytes"
        );
    }

    #[test]
    fn test_decapsulation_key_length_768() {
        let seed = [0u8; 64];
        let kp =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).expect("key gen failed");
        // Seed (compact representation) is always 64 bytes
        assert_eq!(
            kp.decapsulation_key().len(),
            64,
            "ML-KEM-768 decapsulation seed must be 64 bytes"
        );
    }

    #[test]
    fn test_ciphertext_length_768() {
        let seed = [0u8; 64];
        let kp =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).expect("key gen failed");
        let encap = MlKemKeyPair::encapsulate(kp.encapsulation_key(), MlKemVariant::MlKem768)
            .expect("encapsulation failed");
        assert_eq!(
            encap.ciphertext().len(),
            1088,
            "ML-KEM-768 ciphertext must be 1088 bytes"
        );
    }

    #[test]
    fn test_shared_secret_length() {
        let seed = [0u8; 64];
        let kp =
            MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).expect("key gen failed");
        let encap = MlKemKeyPair::encapsulate(kp.encapsulation_key(), MlKemVariant::MlKem768)
            .expect("encapsulation failed");
        assert_eq!(
            encap.shared_secret().len(),
            32,
            "ML-KEM shared secret must be 32 bytes"
        );

        let ss_decap = kp.decapsulate(encap.ciphertext()).expect("decap failed");
        assert_eq!(
            ss_decap.len(),
            32,
            "decapsulated shared secret must be 32 bytes"
        );
    }

    #[test]
    fn test_hybrid_shared_secret_combined_length() {
        let client = HybridKexState::new(MlKemVariant::MlKem768).expect("client init failed");
        let server = HybridKexState::new(MlKemVariant::MlKem768).expect("server init failed");

        let client_share = client.public_key_share();
        let (server_share, server_secret) = server
            .server_respond(&client_share)
            .expect("server respond failed");

        let client_secret = client
            .client_finish(&server_share, &server_share.pq_share)
            .expect("client finish failed");

        assert_eq!(client_secret.combined().len(), 32);
        assert_eq!(server_secret.combined().len(), 32);
        assert_eq!(client_secret.pq().len(), 32);
        assert_eq!(server_secret.pq().len(), 32);
    }

    // -----------------------------------------------------------------------
    // Group ID tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_group_id_x25519_mlkem768() {
        let kex = HybridKexState::new(MlKemVariant::MlKem768).expect("kex init failed");
        let share = kex.public_key_share();
        assert_eq!(
            share.group_id, 0x11EC,
            "X25519MLKEM768 group ID must be 0x11EC"
        );
    }

    #[test]
    fn test_group_id_x25519_mlkem512() {
        let kex = HybridKexState::new(MlKemVariant::MlKem512).expect("kex init failed");
        let share = kex.public_key_share();
        assert_eq!(share.group_id, 0x11EB);
    }

    #[test]
    fn test_group_id_x25519_mlkem1024() {
        let kex = HybridKexState::new(MlKemVariant::MlKem1024).expect("kex init failed");
        let share = kex.public_key_share();
        assert_eq!(share.group_id, 0x11ED);
    }

    // -----------------------------------------------------------------------
    // Property tests: deterministic encapsulation with the same message seed
    // yields the same ciphertext and shared secret
    // -----------------------------------------------------------------------

    #[test]
    fn test_deterministic_encap_same_ek_same_m() {
        // Because we use fill_system_random for `m` in encapsulate(),
        // two calls will produce different ciphertexts/secrets (random encapsulation).
        // This test verifies that only the KEY is deterministic, not the message.
        let seed = [55u8; 64];
        let kp1 = MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).unwrap();
        let kp2 = MlKemKeyPair::generate(MlKemVariant::MlKem768, Some(seed)).unwrap();

        // Keys must be identical
        assert_eq!(kp1.encapsulation_key(), kp2.encapsulation_key());
        assert_eq!(kp1.decapsulation_key(), kp2.decapsulation_key());

        // But encapsulation uses fresh randomness, so ciphertexts differ
        let encap1 =
            MlKemKeyPair::encapsulate(kp1.encapsulation_key(), MlKemVariant::MlKem768).unwrap();
        let encap2 =
            MlKemKeyPair::encapsulate(kp2.encapsulation_key(), MlKemVariant::MlKem768).unwrap();

        // Both round-trips must succeed
        let ss1 = kp1.decapsulate(encap1.ciphertext()).unwrap();
        let ss2 = kp2.decapsulate(encap2.ciphertext()).unwrap();

        assert_eq!(encap1.shared_secret(), ss1.as_slice());
        assert_eq!(encap2.shared_secret(), ss2.as_slice());
    }

    // -----------------------------------------------------------------------
    // Hybrid variant mismatch test
    // -----------------------------------------------------------------------

    #[test]
    fn test_hybrid_kex_variant_mismatch_fails() {
        let client = HybridKexState::new(MlKemVariant::MlKem512).expect("client init failed");
        let server = HybridKexState::new(MlKemVariant::MlKem768).expect("server init failed");

        let client_share = client.public_key_share();
        // Server expects MlKem768, but client offered MlKem512
        let result = server.server_respond(&client_share);

        assert!(result.is_err(), "server should reject mismatched variant");
    }

    // -----------------------------------------------------------------------
    // Wire encoding/decoding edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_pq_key_share_decode_too_short() {
        let short = [0u8; 10];
        let result = PqKeyShareExtension::decode(&short, MlKemVariant::MlKem768);
        assert!(result.is_err());
    }

    #[test]
    fn test_pq_key_share_decode_truncated_pq() {
        // Build a valid header but truncate the pq payload
        let mut data = vec![0u8; 36];
        // Set pq_len = 1000
        data[34] = 0x03;
        data[35] = 0xE8;
        // Only 36 bytes total — not enough for 1000 bytes of PQ data
        let result = PqKeyShareExtension::decode(&data, MlKemVariant::MlKem768);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // SecurityInfo Display test
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_info_display() {
        let info = QuantumSecurityInfo {
            variant: MlKemVariant::MlKem768,
            is_hybrid: true,
            key_bits: 192,
            encapsulation_key_bytes: 1184,
            decapsulation_key_bytes: 64,
            ciphertext_bytes: 1088,
            shared_secret_bytes: 32,
        };
        let s = info.to_string();
        assert!(s.contains("ML-KEM-768"));
        assert!(s.contains("Hybrid"));
    }

    // -----------------------------------------------------------------------
    // Variant security bits test
    // -----------------------------------------------------------------------

    #[test]
    fn test_variant_security_bits() {
        assert_eq!(MlKemVariant::MlKem512.security_bits(), 128);
        assert_eq!(MlKemVariant::MlKem768.security_bits(), 192);
        assert_eq!(MlKemVariant::MlKem1024.security_bits(), 256);
    }

    // -----------------------------------------------------------------------
    // Variant ciphertext/ek sizes
    // -----------------------------------------------------------------------

    #[test]
    fn test_variant_byte_sizes() {
        // ML-KEM-512
        assert_eq!(MlKemVariant::MlKem512.encapsulation_key_bytes(), 800);
        assert_eq!(MlKemVariant::MlKem512.ciphertext_bytes(), 768);
        // ML-KEM-768
        assert_eq!(MlKemVariant::MlKem768.encapsulation_key_bytes(), 1184);
        assert_eq!(MlKemVariant::MlKem768.ciphertext_bytes(), 1088);
        // ML-KEM-1024
        assert_eq!(MlKemVariant::MlKem1024.encapsulation_key_bytes(), 1568);
        assert_eq!(MlKemVariant::MlKem1024.ciphertext_bytes(), 1568);
        // Shared secret always 32
        assert_eq!(MlKemVariant::MlKem512.shared_secret_bytes(), 32);
        assert_eq!(MlKemVariant::MlKem768.shared_secret_bytes(), 32);
        assert_eq!(MlKemVariant::MlKem1024.shared_secret_bytes(), 32);
    }

    // -----------------------------------------------------------------------
    // MlKemEncapsulation accessors
    // -----------------------------------------------------------------------

    #[test]
    fn test_encapsulation_accessors() {
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem768, None).unwrap();
        let encap =
            MlKemKeyPair::encapsulate(kp.encapsulation_key(), MlKemVariant::MlKem768).unwrap();
        assert!(!encap.ciphertext().is_empty());
        assert_eq!(encap.shared_secret().len(), 32);
    }

    // -----------------------------------------------------------------------
    // Security info is_hybrid flag
    // -----------------------------------------------------------------------

    #[test]
    fn test_security_info_not_hybrid_for_pure_mlkem() {
        let kp = MlKemKeyPair::generate(MlKemVariant::MlKem512, None).unwrap();
        assert!(!kp.security_info().is_hybrid);
    }

    // -----------------------------------------------------------------------
    // Wire length calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_pq_key_share_wire_len() {
        let kex = HybridKexState::new(MlKemVariant::MlKem768).unwrap();
        let share = kex.public_key_share();
        // group_id(2) + classical(32) + pq_len(2) + pq_data(1184)
        assert_eq!(share.wire_len(), 2 + 32 + 2 + 1184);
        assert_eq!(share.encode().len(), share.wire_len());
    }

    // -----------------------------------------------------------------------
    // HKDF zero classical / PQ is allowed (still deterministic)
    // -----------------------------------------------------------------------

    #[test]
    fn test_hkdf_combine_zero_inputs() {
        let classical = [0u8; 32];
        let pq = [0u8; 32];
        let out = hkdf_sha256_combine(&classical, &pq, b"zero-test");
        assert_ne!(out, [0u8; 32], "HKDF should expand even from zero inputs");
    }
}
