# Certificate Management Guide

Certificate lifecycle management for the `mielin-mesh-wire` TLS layer — generation,
storage, CA/chain validation, pinning, ACME automation, zero-downtime rotation, and
connection health monitoring for QUIC mesh links.

This guide documents the code in `mielin-mesh/wire/src/certs/` (`mod.rs`, `ca.rs`,
`mtls.rs`, `pinning.rs`, `acme.rs`, `renewal.rs`, `storage.rs`),
`mielin-mesh/wire/src/cert_rotation.rs`, and `mielin-mesh/wire/src/advanced_tls.rs`.
Every type, function, and config field named below exists in that source tree — see
the file/line references inline for where to look when you need more detail.

---

## Table of Contents

1. [Overview: TLS 1.3 Trust Model](#1-overview-tls-13-trust-model)
2. [Certificate Generation & Storage](#2-certificate-generation--storage)
3. [CA & Chain Validation](#3-ca--chain-validation)
4. [Certificate Pinning](#4-certificate-pinning)
5. [ACME / Let's Encrypt Automated Issuance](#5-acme--lets-encrypt-automated-issuance)
6. [Hot Rotation Without Downtime](#6-hot-rotation-without-downtime)
7. [Connection Health Monitoring](#7-connection-health-monitoring)
8. [Operational Runbook](#8-operational-runbook)

---

## 1. Overview: TLS 1.3 Trust Model

The mesh wire protocol runs QUIC over TLS 1.3 exclusively. `mielin-mesh-wire/src/security.rs`
defines `TlsVersion`, whose only variant is `V1_3`, and `TlsConfig::default()` sets both
`min_version` and `max_version` to it — there is no negotiated fallback to TLS 1.2.

**Crypto stack (pure Rust, no `ring`/`aws-lc-rs`):**

| Crate | Version (workspace) | Role |
|---|---|---|
| `rustls` | `0.23.41`, `default-features = false`, `features = ["std"]` | TLS state machine, `ClientConfig`/`ServerConfig` builders |
| `oxiquic-crypto` | `0.2.0` | Supplies the `rustls::crypto::CryptoProvider` via `oxiquic_crypto::quic_crypto_provider()` — signature verification algorithms and cipher suites, no default provider feature is compiled into `rustls` |
| `oxicrypto-hash` | `0.2.0` | `Sha256` / `Sha384` / `Sha512` for fingerprints and pins |
| `oxicrypto-sig` | `0.2.0` | `EcdsaP256Signer` / `EcdsaP256Verifier` for ACME JWS signing |
| `rcgen` | `0.14.8`, `features = ["pem"]` | Certificate/CSR construction primitives |
| `oxitls-rcgen` | `0.2.0` | Pure-Rust ECDSA P-256 key generation and self-signed/CA-signed cert issuance on top of `rcgen` |
| `x509-parser` | `0.18.1` | DER parsing (SPKI, SANs, extensions, CRLs) |
| `p256` | `0.14.0-rc.15`, `features = ["ecdsa", "alloc"]` | ACME account key (P-256 scalar/point math) |
| `oxihttp-client` | `0.2.0`, `features = ["tls"]` | HTTPS client used for ACME directory calls and CRL fetching |

Every `rustls::ClientConfig`/`ServerConfig` builder in this codebase is constructed with
`builder_with_provider(Arc::new(oxiquic_crypto::quic_crypto_provider()))` rather than
`rustls::ClientConfig::builder()` — because `rustls` is compiled with `default-features =
false`, there is no built-in provider to fall back to, so every call site must supply one
explicitly. See `mtls.rs::MtlsContext::create_server_config`/`create_client_config` and
`cert_rotation.rs::CertRotator::build_server_config` for the two canonical call sites.

**Where certificates come from into a live connection.** `mielin_mesh_wire::certs::CertManager`
(`certs/mod.rs`) is the entry point most transports use: `CertManager::get_or_generate_cert`
returns a cached certificate or mints a fresh self-signed one, and
`transport.rs::QuicTransport::new_with_certs(bind_addr, node_id, cert_manager)` wires it
directly into the QUIC server endpoint. For mesh links that need mutual authentication,
CA-anchored trust, or pinning, use `certs::mtls::MtlsContext` instead (§3); for public
Let's-Encrypt-issued certs, use `certs::acme::AcmeClient` (§5).

---

## 2. Certificate Generation & Storage

### 2.1 Self-signed certificate generation (`certs/mod.rs`)

`Certificate::generate_self_signed` and `Certificate::generate_self_signed_with_sans`
delegate to `oxitls_rcgen::generate_self_signed_p256`, which produces an ECDSA P-256
key pair and a self-signed leaf in one call:

```rust
use mielin_mesh_wire::certs::Certificate;

// Common name + auto-added "localhost"/"127.0.0.1" SANs, 365-day validity
let cert = Certificate::generate_self_signed("mesh-node-1".to_string(), 365)?;

// With extra SANs
let cert = Certificate::generate_self_signed_with_sans(
    "mesh-node-1".to_string(),
    365,
    vec!["node1.internal".to_string()],
)?;
```

`generate_self_signed_with_sans` always appends `localhost` and `127.0.0.1` to the SAN
list if not already present. Validity is bounded to `1..=825` days (CA/Browser Forum max
lifetime, enforced in `CertInfo::validate` and at generation time). The resulting
`Certificate` struct holds:

```rust
pub struct Certificate {
    pub cert_chain: Vec<CertificateDer<'static>>,
    pub private_key: PrivateKeyDer<'static>,
    pub info: CertInfo,
}
```

`CertInfo` tracks `common_name`, `subject_alt_names`, `validity_days`, `created_at`,
`expires_at`, and exposes `is_expired()`, `time_until_expiry()`,
`should_rotate()` (default 30-day threshold) and `should_rotate_with_threshold(days)`.

### 2.2 `CertManager` (in-process cache + rotation)

`certs::CertManager` wraps a `tokio::sync::RwLock<Option<Certificate>>` and generates
certificates on demand:

```rust
use mielin_mesh_wire::certs::CertManager;

let manager = CertManager::new().with_rotation_threshold(30); // days before expiry
let cert = manager.get_or_generate_cert("mesh-node-1").await?; // Arc<Certificate>
let forced = manager.rotate_cert("mesh-node-1").await?;        // force regeneration, 3 retries w/ exp. backoff
let needed = manager.needs_rotation().await;                   // bool
```

`rotate_cert_with_retry(node_id, max_attempts)` retries `Certificate::generate_self_signed`
with exponential backoff (`100ms * 2^attempt`) and returns `CertError::RotationFailed`
after exhausting attempts.

### 2.3 Storage (`certs/storage.rs`)

`CertStorage` supports two backends via `StorageBackend`:

```rust
use mielin_mesh_wire::certs::storage::CertStorage;

let mem = CertStorage::memory();
let file = CertStorage::file(std::path::PathBuf::from("/var/lib/mielin/certs"));

file.store("mesh-node-1", cert).await?;
let cert = file.retrieve("mesh-node-1").await?;
let all = file.list().await;            // Vec<CertInfo>
file.delete("mesh-node-1").await?;
let removed = file.cleanup_expired().await; // usize
```

File-backed storage writes one `<node_id>.cert.json` per certificate. On-disk records
use an internal `CertFile` DTO that hex-encodes the private key and certificate chain
(so files stay human-inspectable) and preserves the private-key variant
(`Pkcs1`/`Pkcs8`/`Sec1`) via a `KeyType` discriminant so round-tripping doesn't corrupt
the key encoding. `store()` writes to disk *before* updating the in-memory cache, so a
crash mid-write can't leave the cache and disk state inconsistent with each other.
`retrieve()` checks the memory cache first and falls back to disk, populating the cache
on a disk hit. `delete()` is idempotent — deleting a non-existent file is not an error.

---

## 3. CA & Chain Validation

### 3.1 `CertificateAuthority` (`certs/ca.rs`)

`CertificateAuthority` manages trust anchors and revocation checking:

```rust
use mielin_mesh_wire::certs::ca::{CaConfig, CertificateAuthority, RevocationCheckMethod};
use std::time::Duration;

let config = CaConfig::new()
    .with_revocation_check(RevocationCheckMethod::OcspThenCrl)
    .with_ocsp_timeout(Duration::from_secs(10))
    .with_crl_cache_duration(Duration::from_secs(3600));

let ca = CertificateAuthority::new(config);
let fingerprint = ca.add_ca_cert(&ca_cert_der).await?; // SHA-256 hex fingerprint
let removed = ca.remove_ca_cert(&fingerprint).await?;
let anchors = ca.trust_anchors().await;                // Vec<rustls::pki_types::TrustAnchor<'static>>
```

`CaConfig` presets: `CaConfig::production()` (`OcspThenCrl`, 10s OCSP timeout, 1h CRL
cache, expired CRLs rejected) and `CaConfig::development()` (`RevocationCheckMethod::None`,
`allow_expired_crls: true`). There is no builder method for `allow_expired_crls` itself —
set it by constructing `CaConfig` directly or using a preset.

> **Note:** `remove_ca_cert` removes the fingerprint from the `ca_info` map but does
> **not** rebuild `trust_anchors` (the comment in `ca.rs` flags this explicitly — "hard
> to identify... consider rebuilding the trust_anchors list in production"). A removed
> CA's public key can still validate chains via `trust_anchors()` until the
> `CertificateAuthority` is reconstructed.

### 3.2 Revocation checking: CRL fetch + cache

`check_revocation(cert, issuer)` dispatches on `CaConfig::revocation_check`:

- `RevocationCheckMethod::None` — always `RevocationStatus::Valid`.
- `RevocationCheckMethod::Crl` — checks the in-memory `crl_cache` (keyed by SHA-256 of
  the raw issuer DER) and, on a cache miss or expired entry, calls
  `fetch_and_check_crl`, which:
  1. Parses `CRLDistributionPoints` from the certificate's extensions
     (`x509_parser::prelude::DistributionPointName::FullName` → `GeneralName::URI`).
  2. Fetches CRL bytes from the first working distribution point via
     `oxihttp_client::Client::builder().with_webpki_roots().build_https()`, trying each
     URI in order.
  3. Parses the CRL with `x509_parser::prelude::CertificateRevocationList::from_der`.
  4. Caches the parsed entries keyed by issuer fingerprint, with expiry taken from the
     CRL's `nextUpdate` field (falling back to `crl_cache_duration` if absent).
- `RevocationCheckMethod::Ocsp` — extracts the OCSP responder URL from the Authority
  Information Access extension (OID `1.3.6.1.5.5.7.1.1`, access method OID
  `1.3.6.1.5.5.7.48.1`) and logs it, but **returns `RevocationStatus::Unknown`** rather
  than performing a full OCSP request/response — the doc comment in `ca.rs` states this
  is scaffolding pending a suitable pure-Rust ASN.1 dependency for OCSP encoding, not a
  hidden fallback.
- `RevocationCheckMethod::OcspThenCrl` — tries OCSP first, falls back to CRL on error
  (which in practice means it falls straight through to CRL, since OCSP never errors —
  it just returns `Unknown`).

```rust
let status = ca.check_revocation(&leaf_cert_der, None).await?;
match status {
    RevocationStatus::Valid => { /* proceed */ }
    RevocationStatus::Revoked { revoked_at } => { /* reject connection */ }
    RevocationStatus::Unknown => { /* policy decision: allow or reject */ }
}
```

`clear_crl_cache()` and `crl_cache_size()` are available for cache management/diagnostics.

### 3.3 Name constraints

`CertificateAuthority::cert_to_trust_anchor` (used by `add_ca_cert`) extracts the
`NameConstraints` extension (OID `2.5.29.30`) verbatim from the DER and attaches it to
the `rustls::pki_types::TrustAnchor::name_constraints` field, so any name-constrained CA
you add is enforced by `rustls`'s own webpki-based path validation during
`verify_server_cert`/`verify_client_cert` — the mesh code does not re-implement name
constraint checking itself, it just wires the raw extension bytes through.

### 3.4 Mutual TLS chain validation (`certs/mtls.rs`)

`MtlsContext` builds `rustls::ServerConfig`/`ClientConfig` with custom verifiers
(`MtlsClientVerifier` implementing `rustls::server::danger::ClientCertVerifier`,
`MtlsServerVerifier` implementing `rustls::client::danger::ServerCertVerifier`):

```rust
use mielin_mesh_wire::certs::{
    mtls::{MtlsConfig, MtlsContext},
    Certificate,
};

let cert = Certificate::generate_self_signed("mesh-node-1".to_string(), 365)?;
let config = MtlsConfig::production(); // require+verify client cert, verify server cert, pinning on

let context = MtlsContext::new(config, cert)
    .with_trust_anchor(ca_cert_der)?;   // adds to an internal rustls::RootCertStore

let server_cfg: rustls::ServerConfig = context.create_server_config()?;
let client_cfg: rustls::ClientConfig = context.create_client_config()?;
```

`MtlsConfig` fields: `require_client_cert`, `verify_client_cert`, `verify_server_cert`,
`use_pinning`, `allow_self_signed`. Presets:

| Preset | require_client_cert | verify_client_cert | verify_server_cert | use_pinning | allow_self_signed |
|---|---|---|---|---|---|
| `MtlsConfig::production()` | true | true | true | true | false |
| `MtlsConfig::development()` | false | false | false | false | true |
| `MtlsConfig::testing()` | false | false | false | false | true |

When `allow_self_signed` is set, both verifiers short-circuit with
`ClientCertVerified::assertion()` / `ServerCertVerified::assertion()` — no chain
validation happens at all (development/test only). Otherwise, real chain validation is
delegated to `rustls::server::WebPkiClientVerifier` / `rustls::client::WebPkiServerVerifier`
built against `context.trust_anchors` (a `RootCertStore`), using the same
`oxiquic_crypto::quic_crypto_provider()` as everything else. If `trust_anchors` is empty
and `allow_self_signed` is false, the verifier rejects the certificate outright rather
than silently accepting it. ALPN is fixed to `["h3", "h2", "http/1.1"]` in both server
and client configs.

> **Note:** when `use_pinning` is true, the pin-store check on the verified chain is
> currently a documented no-op pass-through (`mtls.rs` comment: "Pin-store check is a
> best-effort augment on top of chain validation... here we just pass through since the
> chain is already trusted"). For enforced pin-or-reject behavior, call
> `PinStore::verify_certificate` (§4) explicitly in your connection-accept path, or use
> `advanced_tls::CertChainVerifier::with_pin_store` (§4.2), which does perform the check.

---

## 4. Certificate Pinning

There are **two independent pinning implementations** in the wire crate — pick the one
that matches the layer you're integrating with.

### 4.1 `certs::pinning::PinStore` — HPKP-style, async, identifier-keyed

```rust
use mielin_mesh_wire::certs::pinning::{PinHashAlgorithm, PinStore, PinType};
use std::time::Duration;

let store = PinStore::new();

let pin = store.pin_certificate(
    "mesh-server".to_string(),
    &cert.cert_chain[0],
    PinType::SubjectPublicKeyInfo,      // Certificate | PublicKey | SubjectPublicKeyInfo
    PinHashAlgorithm::Sha256,           // Sha256 | Sha384 | Sha512
    Some(Duration::from_secs(86400 * 365)),
).await?;

let result = store.verify_certificate("mesh-server", &cert.cert_chain).await?;
assert!(result.is_success());
```

`PinType`:

- `Certificate` — pins the whole DER-encoded cert (strictest; breaks on every renewal).
- `PublicKey` / `SubjectPublicKeyInfo` — pins the extracted SPKI bytes (survives
  re-issuance with the same key pair; SPKI is the HPKP-recommended choice).

`PinHashAlgorithm::hash_length()` returns 32/48/64 bytes for Sha256/384/512
respectively; `Pin::validate()` enforces the hash string is exactly `hash_length() * 2`
lowercase-or-uppercase hex characters.

**Rotation workflow** — `rotate_pin` adds a new pin marked `is_backup: true` with a
60-day expiration, so both the old and new certificate validate during the overlap
window; `promote_backup_pin(identifier, remove_old)` then flips backup pins to primary
(and optionally prunes old non-backup pins older than 1 hour):

```rust
let backup = store.rotate_pin(
    "mesh-server", &new_cert.cert_chain[0],
    PinType::SubjectPublicKeyInfo, PinHashAlgorithm::Sha256,
).await?;
assert!(backup.is_backup);

// both old_cert and new_cert verify successfully here

let promoted = store.promote_backup_pin("mesh-server", false).await?;
```

`PinVerificationResult` has four variants: `Matched { pin_hash, is_backup }`,
`NoPins { identifier }`, `NoMatch { identifier, expected_pins, actual_hash }`, and
`Expired { identifier, pin_hash }`. `cleanup_expired()` sweeps expired pins across all
identifiers and returns the count removed.

`HpkpHeader` (`parse`/`format`) round-trips the classic
`Public-Key-Pins: pin-sha256="..."; max-age=...; includeSubDomains` header format if you
need to interoperate with HTTP clients that understand it.

### 4.2 `advanced_tls::CertPin` / `CertPinStore` — sync, constant-time, hostname-keyed

```rust
use mielin_mesh_wire::advanced_tls::{CertPin, CertPinStore, CertChainVerifier};
use std::time::Duration;

let store = CertPinStore::new(/* allow_unpinned = */ false);
store.add_pin("mesh-server.internal", CertPin::new_sha256(fingerprint, "primary")
    .with_expiry(Duration::from_secs(86400 * 90)));

let verifier = CertChainVerifier::new()
    .with_max_depth(5)
    .with_pin_store(std::sync::Arc::new(store));

let result = verifier.verify_chain(&der_chain, "mesh-server.internal")?;
// result.pinned, result.depth, result.leaf_fingerprint (SHA-256 of leaf DER)
```

`CertPin` supports `new_sha256([u8; 32], label)`, `new_sha384([u8; 48], label)`, and
`new_sha512([u8; 64], label)`, plus a builder `with_expiry(Duration)`.
`verify_der(cert_der: &[u8]) -> bool` hashes the DER with the pin's algorithm and
compares against the stored fingerprint using **constant-time XOR accumulation**
(`computed.iter().zip(fingerprint.iter()).fold(0u8, |acc, (a,b)| acc | (a ^ b))`) —
this is deliberately not a short-circuiting `==` to reduce timing side-channels.

`CertPinStore::verify(hostname, cert_der)` returns `Ok(PinVerification::Pinned)`,
`Ok(PinVerification::Unpinned)` (no pins registered + `allow_unpinned == true`),
`Ok(PinVerification::NoPinsLeft)` (all pins expired), or
`Err(PinError::NoPinsForHost)` / `Err(PinError::FingerprintMismatch)`.
`remove_expired()` sweeps expired pins and drops hostname entries left with zero pins.

`CertChainVerifier` additionally enforces `max_depth` (default 5) and rejects an empty
chain (`ChainError::EmptyChain`) or a chain deeper than the configured max
(`ChainError::TooDeep { depth, max }`) before delegating to the pin store. Its
`require_san`, `allowed_key_algs` (`AllowedKeyAlgorithm::{Ed25519, EcdsaP256, EcdsaP384,
Rsa2048Plus}`), and `min_key_bits` fields are present on the struct and settable via
`.require_san(bool)` / `.with_key_algorithm(alg)` / `.with_min_key_bits(bits)`, but are
documented in-source as "policy field, future integration" — `verify_chain` does not
currently enforce them.

---

## 5. ACME / Let's Encrypt Automated Issuance

`certs/acme.rs` is an **in-house RFC 8555 ACME client** (the crate's dependency comment
notes `instant-acme` was removed in favor of this implementation). Directory endpoints:

```rust
const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";
const LETS_ENCRYPT_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
```

### 5.1 Crypto details

- Account key: P-256 (`p256::SecretKey`, generated via `p256::elliptic_curve::Generate`
  + platform RNG).
- JWS signing algorithm: `ES256`, via `oxicrypto_sig::EcdsaP256Signer::sign_fmt(...,
  SignatureFormat::Raw)` — a 64-byte fixed-width `r‖s` signature per RFC 7515 §A.3 (not
  ASN.1 DER).
- JWK thumbprint: SHA-256 over the canonical JWK JSON (`oxicrypto_hash::Sha256`), with
  keys in the RFC 7638-mandated lexicographic order `crv, kty, x, y`.
- HTTP: `oxihttp_client::Client` built with `.with_webpki_roots().build_https()`.

### 5.2 Configuration and client setup

```rust
use mielin_mesh_wire::certs::acme::{AcmeChallengeType, AcmeClient, AcmeConfig, ChallengeValidator};

let config = AcmeConfig::new()
    .with_email("admin@example.com".to_string())
    .with_challenge_type(AcmeChallengeType::Http01) // or Dns01
    .with_staging()                                 // use LE staging directory
    .with_renewal_threshold(30);                    // days before expiry

let client = AcmeClient::new(config)?
    .with_validator(std::sync::Arc::new(my_validator)); // impl ChallengeValidator
```

`AcmeConfig::acme_directory_url()` resolves in priority order: an explicit
`with_directory_url(url)` override (used for tests / private ACME servers), then
staging vs. production based on `use_staging`.

Implement `ChallengeValidator` to serve the HTTP-01 token at
`http://<domain>/.well-known/acme-challenge/<token>` or create the DNS-01 TXT record at
`_acme-challenge.<domain>`; `cleanup_http01`/`cleanup_dns01` are called for teardown.
See `examples/acme_certificate.rs` for a full (demonstration-only) `ChallengeValidator`.

### 5.3 Issuance flow

`AcmeClient::request_certificate(domains: Vec<String>) -> Result<Certificate, CertError>`
drives the full RFC 8555 flow: `initialize_account` (idempotent — `newAccount`, capturing
the `Location` header as `kid`), `newOrder`, polling each authorization
(`poll_authorization_valid`, 2s interval, 30 attempts by default via the internal
`PollPolicy`), delivering the configured challenge type, generating a fresh ECDSA P-256
key + CSR (`oxitls_rcgen::OxiEcdsaP256Key` + `rcgen::CertificateParams`), finalizing the
order, polling until `status == "valid"` (`poll_order_valid`), and fetching the issued
PEM chain via POST-as-GET. `badNonce` (HTTP 400 with that ACME problem type) triggers a
single automatic retry with a freshly fetched nonce. Issued certificates are hard-coded
to `validity_days = 90` (Let's Encrypt's standard lifetime) in the returned `CertInfo`.

```rust
let cert = client.request_certificate(vec!["node1.example.com".to_string()]).await?;
```

`renew_if_needed(current_cert, domains)` is a convenience wrapper: it checks
`current_cert.info.should_rotate_with_threshold(config.renewal_threshold_days)` and only
calls `request_certificate` if the threshold has been crossed, returning `Ok(None)`
otherwise.

### 5.4 Automatic renewal scheduling (`certs/renewal.rs`)

`RenewalScheduler` runs a periodic check/renew loop independent of the rotation layer
(§6) and is driven by a `RenewalStrategy`:

```rust
use mielin_mesh_wire::certs::renewal::{RenewalConfig, RenewalScheduler, RenewalStrategy};

let strategy = RenewalStrategy::Acme { domains: vec!["node1.example.com".to_string()] };
// or: RenewalStrategy::SelfSigned { node_id: "mesh-node-1".to_string(), validity_days: 365 }

let scheduler = std::sync::Arc::new(
    RenewalScheduler::new(RenewalConfig::new(), strategy)
        .with_acme_client(std::sync::Arc::new(acme_client)),
);
scheduler.set_certificate(cert).await;

let mut events = scheduler.subscribe(); // broadcast::Receiver<RenewalEvent>
tokio::spawn(scheduler.clone().start());
```

`RenewalConfig` presets:

| Preset | check_interval | renewal_threshold_days | max_retry_attempts | initial_retry_delay | max_retry_delay |
|---|---|---|---|---|---|
| `RenewalConfig::aggressive()` | 900s (15m) | 60 | 10 | 30s | 1800s |
| `RenewalConfig::conservative()` | 21600s (6h) | 7 | 3 | 300s | 7200s |
| `RenewalConfig::production()` (= `default()`) | 3600s (1h) | 30 | 5 | 60s | 3600s |

Retry backoff is exponential when `exponential_backoff` is true (the default):
`delay = min(initial_retry_delay * 2^attempt, max_retry_delay)`.

`RenewalEvent` (broadcast to all `subscribe()`rs, buffer size 100) has six variants:
`CheckStarted { identifier, days_until_expiry }`,
`RenewalTriggered { identifier, days_until_expiry, method }` (`method` is
`RenewalMethod::Acme | SelfSigned`), `RenewalSucceeded { identifier, validity_days }`,
`RenewalFailed { identifier, error, attempt }`,
`RenewalRetrying { identifier, attempt, delay_secs }`, and
`RenewalGaveUp { identifier, total_attempts, last_error }`. `RenewalSucceeded` is the
event `cert_rotation.rs::CertRotator::subscribe_to_renewal` listens for (§6.3).

---

## 6. Hot Rotation Without Downtime

`cert_rotation.rs` provides zero-downtime `ServerConfig` swaps for long-lived QUIC
listeners via a `tokio::sync::watch` channel, independent of the higher-level renewal
scheduler above.

### 6.1 `CertRotator` and `CertRotationHandle`

```rust
use mielin_mesh_wire::cert_rotation::{CertRotationConfig, CertRotator};

let (rotator, handle) = CertRotator::with_self_signed_initial(CertRotationConfig::default())?;
// or: CertRotator::new(Arc::new(initial_server_config), CertRotationConfig::default())

rotator.rotate(&new_cert_der, &new_key_der).await?; // builds a new ServerConfig, pushes it

let current: Arc<rustls::ServerConfig> = handle.current();       // non-blocking read
let updated: Option<Arc<rustls::ServerConfig>> = handle.changed().await; // waits for next rotation
```

`CertRotationHandle` is `Clone` and cheap — it's just a `watch::Receiver`, so every QUIC
accept loop (or connection-serving task) can hold its own handle and observe rotations
independently without contending on a lock.

`CertRotator::rotate` builds the new `rustls::ServerConfig` **outside** the internal
`Mutex<InnerState>` lock (so cert-building work never blocks concurrent rotation
requests from being rejected quickly), then pushes it via `watch::Sender::send` only on
success. `build_server_config(cert, key)` (also usable standalone) rejects an empty
certificate DER immediately (`CertRotationError::InvalidCertificate`) and otherwise
builds via `rustls::ServerConfig::builder_with_provider(Arc::new(oxiquic_crypto::quic_crypto_provider()))
.with_safe_default_protocol_versions().with_no_client_auth().with_single_cert(...)`.

### 6.2 Rate limiting (`CertRotationConfig`)

```rust
pub struct CertRotationConfig {
    pub max_rotations_per_hour: u32,      // default 12
    pub min_rotation_interval: Duration,  // default 60s
    pub require_valid_before_swap: bool,  // default true
}
```

`CertRotationConfig::permissive()` (`max_rotations_per_hour: 3600`,
`min_rotation_interval: Duration::ZERO`, `require_valid_before_swap: false`) is intended
for tests, not production. The rate limiter purges rotation timestamps older than 1
hour on every check, then enforces both caps: exceeding `max_rotations_per_hour` or
rotating faster than `min_rotation_interval` since the last successful rotation both
return `CertRotationError::MaxRotationsExceeded { max }`. A rotation already in flight
(`rotation_in_progress`) causes concurrent callers to get
`CertRotationError::RotationInProgress` rather than queueing.

`CertRotator::stats()` returns a `CertRotationStats { total_rotations, last_rotation_at,
failed_rotations }` snapshot; `rotation_count()` gives a lock-free monotonic counter.

### 6.3 Renewal subscription

`CertRotator::subscribe_to_renewal(renewal_rx: broadcast::Receiver<RenewalEvent>)` spawns
a background task that listens for `RenewalEvent::RenewalSucceeded` from a
`RenewalScheduler` (§5.4) and records a rotation-triggered stat bump on receipt:

```rust
let mut renewal_events = renewal_scheduler.subscribe();
let _join = rotator.subscribe_to_renewal(renewal_events);
```

The task exits cleanly when the broadcast channel closes (`RecvError::Closed`) and logs
(without crashing) on `RecvError::Lagged(n)`.

> **Important gap, documented in-source:** `subscribe_to_renewal`'s handler does **not**
> actually call `rotator.rotate()` with new DER material. `RenewalEvent::RenewalSucceeded`
> only carries `{ identifier, validity_days }` — no cert/key bytes — so the handler can't
> perform a real swap from the event alone; it currently just increments
> `stats.total_rotations` and logs that a rotation "hook" fired. The `cert_rotation.rs`
> doc comment is explicit about this: "In production you would wire the `CertManager` or
> a cert store here." If you need renewal to actually hot-swap the live `ServerConfig`,
> fetch the new certificate from your `CertManager`/`CertStorage` inside your own
> `RenewalEvent::RenewalSucceeded` handler and call `rotator.rotate(&cert_der, &key_der)`
> explicitly, rather than relying on `subscribe_to_renewal` alone.

---

## 7. Connection Health Monitoring

`advanced_tls.rs` also provides heartbeat-based RTT/liveness tracking, independent of
the TLS/cert machinery above but designed to sit alongside it on the same connections.

### 7.1 `HeartbeatConfig` and `ConnectionHealthMonitor`

```rust
use mielin_mesh_wire::advanced_tls::{ConnectionHealthMonitor, HeartbeatConfig, heartbeat_loop};
use std::time::Duration;

let config = HeartbeatConfig::default(); // interval 30s, timeout 10s, max_missed 3, rtt_window 10
let monitor = std::sync::Arc::new(ConnectionHealthMonitor::new(config));

monitor.register_connection("peer-node-1");
monitor.record_pong("peer-node-1", Duration::from_millis(12))?;
monitor.record_missed_ping("peer-node-1")?;

let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
tokio::spawn(heartbeat_loop(monitor.clone(), shutdown_rx));
// ... later: shutdown_tx.send(true)?;
```

`HeartbeatConfig` fields: `interval` (default 30s), `timeout` (default 10s — how long to
wait for a pong before it counts as missed), `max_missed` (default 3 consecutive misses
before `Unhealthy`), `rtt_window` (default 10 — sliding-window sample count for RTT
stats).

### 7.2 Status derivation

`ConnectionHealth::update_status` evaluates top-down:

1. `missed_pings >= config.max_missed` → `ConnectionHealthStatus::Unhealthy`
2. `missed_pings >= 1` → `ConnectionHealthStatus::Degraded`
3. `total_pongs_received > 0` → `ConnectionHealthStatus::Healthy`
4. otherwise → `ConnectionHealthStatus::Unknown` (no data yet)

`ConnectionHealth::mean_rtt()` is the arithmetic mean over the current RTT window;
`p99_rtt()` uses the nearest-rank method (`index = ceil(0.99 * n) - 1`, needs ≥ 2
samples). Both trim to `rtt_window` on `update_status`/`record_missed_ping`, and
`record_pong` additionally hard-caps the raw sample deque at 1024 entries to bound
memory before the next `update_status` call trims it further.

### 7.3 Fleet-wide sweeps

```rust
let sweep = monitor.health_sweep(); // HealthSweepResult { healthy, degraded, to_evict }
for node_id in &sweep.to_evict {
    // Unhealthy — caller is responsible for actually closing these connections;
    // health_sweep only classifies, it does not evict.
}

let stats = monitor.stats(); // ConnectionMonitorStats: totals + healthy/degraded/unhealthy counts
```

`heartbeat_loop(monitor, shutdown_rx)` is the async driver: on every `config.interval`
tick it records a missed ping for any connection whose `last_pong_received` predates
`now - config.timeout` (or which has sent pings but never received a pong at all), then
stamps `last_ping_sent` for all registered connections. It exits when `shutdown_rx`
receives `true`. Actually sending the wire-level ping/pong `Message` and calling
`monitor.record_pong`/`record_missed_ping` from the response path is left to the
transport integration — `heartbeat_loop` only manages the bookkeeping/timer side.

---

## 8. Operational Runbook

### 8.1 Rotate a certificate with zero downtime (hot-swap path)

Use this when a listener is already live and you have new cert/key DER material (e.g.
from ACME renewal or your own CA).

1. Obtain the new certificate. Either:
   - Self-signed: `mielin_mesh_wire::cert_rotation::generate_self_signed_cert_der()` →
     `(CertificateDer<'static>, PrivateKeyDer<'static>)`, or
   - ACME: `AcmeClient::request_certificate(domains).await?` → `Certificate { cert_chain,
     private_key, .. }`, or
   - `CertManager::rotate_cert(node_id).await?` → `Arc<Certificate>`.
2. Check the rotation policy won't reject you:
   `rotator.stats().await` to see `total_rotations`/`last_rotation_at`, and confirm you
   are not within `min_rotation_interval` of the last successful rotation or at the
   `max_rotations_per_hour` cap for your `CertRotationConfig`.
3. Call `rotator.rotate(&cert_der, &key_der).await`. On success this atomically builds a
   `rustls::ServerConfig` and pushes it through the `watch` channel; on failure it
   returns one of `CertRotationError::{TlsConfigBuild, InvalidCertificate,
   RotationInProgress, MaxRotationsExceeded}` without touching the live config.
4. Every task holding a `CertRotationHandle` (from `CertRotator::new`/
   `with_self_signed_initial`, or a fresh `handle.rx.clone()`-style subscription) will
   observe the change on its next `handle.changed().await`, or immediately via
   `handle.current()` for a non-blocking read.
5. If you want renewal to trigger this automatically, do **not** rely solely on
   `rotator.subscribe_to_renewal(scheduler.subscribe())` — per §6.3 it only bumps
   stats. Instead, spawn your own task on `scheduler.subscribe()` that, on
   `RenewalEvent::RenewalSucceeded`, fetches the fresh DER (e.g. via
   `scheduler.get_certificate().await`) and calls `rotator.rotate(...)` itself.

### 8.2 Add a certificate pin

Pick `certs::pinning::PinStore` for the async, identifier-keyed store (used by
`MtlsContext`) or `advanced_tls::CertPinStore` for the sync, hostname-keyed store (used
by `CertChainVerifier`) — see §4 for the tradeoffs.

Using `PinStore`:

1. `let store = PinStore::new();` (or reuse an existing `Arc<PinStore>` already wired
   into your `MtlsContext::with_pin_store`).
2. `store.pin_certificate(identifier, &cert.cert_chain[0], PinType::SubjectPublicKeyInfo,
   PinHashAlgorithm::Sha256, Some(expiry_duration)).await?` — prefer
   `SubjectPublicKeyInfo` over `Certificate` so the pin survives a same-key renewal.
3. Confirm it verifies: `store.verify_certificate(identifier,
   &cert.cert_chain).await?.is_success()`.
4. Periodically call `store.cleanup_expired().await` (e.g. from a maintenance task) to
   drop expired pins.

Using `CertPinStore` (`advanced_tls`):

1. `let store = CertPinStore::new(allow_unpinned);` — set `allow_unpinned = false` for
   any hostname that must never connect without an explicit pin.
2. `store.add_pin(hostname, CertPin::new_sha256(fingerprint, "label").with_expiry(duration));`
3. Attach it to a verifier: `CertChainVerifier::new().with_pin_store(Arc::new(store))`.
4. Call `store.remove_expired()` periodically.

### 8.3 Remove / rotate a pin ahead of certificate renewal

1. **Before** rotating the underlying certificate, add the *new* certificate's pin as a
   backup so both old and new certs validate during the overlap window:
   `store.rotate_pin(identifier, &new_cert.cert_chain[0], pin_type,
   hash_algorithm).await?` (`PinStore`) — this pin is created with `is_backup: true` and
   a 60-day expiry.
2. Deploy the new certificate (see §8.1).
3. Once you've confirmed the new certificate is live everywhere that matters, promote
   the backup and optionally prune the old pin:
   `store.promote_backup_pin(identifier, /* remove_old */ true).await?`.
4. For `advanced_tls::CertPinStore`, there is no backup/promote workflow — simply
   `add_pin` the new fingerprint (with an expiry that overlaps your rotation window) and
   later call `remove_expired()` once the old pin's expiry has passed, or track pins
   externally and remove by rebuilding the store.

### 8.4 Configure mTLS between two mesh nodes

1. Generate (or load) a certificate for each side:
   `Certificate::generate_self_signed(node_id, validity_days)?`.
2. Establish a shared trust anchor. For real chain validation (not
   `allow_self_signed`), both sides need a CA certificate in their trust store. You can
   mint a test CA + CA-signed leaf directly with `oxitls_rcgen`:
   ```rust
   use oxitls_rcgen::{generate_ca, generate_ca_signed_client_cert, SigningAlgorithm};
   let ca = generate_ca("Mesh Root CA", SigningAlgorithm::EcdsaP256)?;
   let leaf = generate_ca_signed_client_cert(&["node1.internal"], SigningAlgorithm::EcdsaP256, &ca)?;
   ```
3. Build the `MtlsConfig` for the environment:
   - Production: `MtlsConfig::production()` (requires + verifies client and server
     certs, pinning on).
   - Local/dev: `MtlsConfig::development()` or `.testing()` (`allow_self_signed: true`,
     all verification off — chain validation is bypassed entirely, use only off
     production paths).
4. `let context = MtlsContext::new(config, cert).with_trust_anchor(ca_cert_der)?;` — call
   `.with_trust_anchor` once per CA you need to trust (chainable — it errors if the DER
   doesn't parse as a certificate).
5. Optionally attach a `PinStore` for defense-in-depth beyond the CA chain:
   `.with_pin_store(pin_store.clone())`.
6. Build configs and hand them to your QUIC/TLS transport:
   `context.create_server_config()?` / `context.create_client_config()?`. Both set ALPN
   to `["h3", "h2", "http/1.1"]`.
7. If a peer connects with a certificate signed by a CA you haven't added via
   `with_trust_anchor`, and `allow_self_signed` is false, the verifier returns
   `RustlsError::General("No trust anchors configured...")` (empty store) or the
   standard webpki chain-validation error (untrusted issuer) — confirm your CA
   provisioning ran before flipping `allow_self_signed` off in an environment.

### 8.5 Set up CA + revocation checking

1. `let ca = CertificateAuthority::new(CaConfig::production());` for strict
   `OcspThenCrl` checking, or a custom `CaConfig` if you only want CRL.
2. `ca.add_ca_cert(&root_ca_der).await?` for each trusted root/intermediate.
3. Before accepting a peer certificate, call
   `ca.check_revocation(&peer_leaf_der, None).await?` and reject on
   `RevocationStatus::Revoked`. Decide your policy for `RevocationStatus::Unknown`
   (OCSP is scaffold-only per §3.2 — most deployments will get `Unknown` for OCSP-only
   CAs and should treat CRL as the source of truth, or explicitly accept the risk of
   `Unknown` in low-assurance environments).
4. Periodically call `ca.clear_crl_cache()` if you need to force-refresh CRLs ahead of
   their cached expiry (e.g. after an out-of-band revocation notice).

---

## See Also

- [`PROTOCOL.md`](./PROTOCOL.md) — wire protocol message types and framing that ride on
  top of the TLS 1.3 transport documented here.
- [`NETWORKING.md`](./NETWORKING.md) — mesh discovery, connection pooling, and transport
  fallback that consume `CertManager`/`MtlsContext` when establishing links.
- [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) — diagnosing handshake failures,
  revocation/pin rejections, and rotation errors surfaced by the types in this guide.
- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — where the mesh wire layer (`mielin-mesh/wire`)
  sits in the overall MielinOS layer stack.
- `mielin-mesh/wire/README.md` and `mielin-mesh/wire/TODO.md` — crate-level feature
  status; TODO.md tracks this guide under "Certificate management guide".
- `mielin-mesh/wire/examples/certificate_management.rs`,
  `certificate_pinning.rs`, `mtls_connection.rs`, `acme_certificate.rs` — runnable
  demonstrations of the APIs described above (`cargo run --example <name>`).
