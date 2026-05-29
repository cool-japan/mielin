//! Certificate storage and persistence
//!
//! Handles storing certificates to disk for persistence across restarts.
//! Supports both in-memory and file-based storage.

use super::{CertError, CertInfo, Certificate};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Serialization DTO for file-backed storage
// ---------------------------------------------------------------------------

/// Discriminant for the private key variant so we can round-trip faithfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum KeyType {
    Pkcs1,
    Pkcs8,
    Sec1,
}

/// On-disk representation of a `Certificate`.
///
/// All binary fields are hex-encoded so the files are human-inspectable.
#[derive(Debug, Serialize, Deserialize)]
struct CertFile {
    key_type: KeyType,
    key_hex: String,
    certs_hex: Vec<String>,
    common_name: String,
    subject_alt_names: Vec<String>,
    validity_days: u32,
    /// Seconds since UNIX_EPOCH for `created_at`
    created_at_secs: u64,
    /// Seconds since UNIX_EPOCH for `expires_at`
    expires_at_secs: u64,
}

impl CertFile {
    fn from_cert(cert: &Certificate) -> Self {
        let (key_type, key_hex) = match &cert.private_key {
            PrivateKeyDer::Pkcs1(k) => (KeyType::Pkcs1, hex::encode(k.secret_pkcs1_der())),
            PrivateKeyDer::Pkcs8(k) => (KeyType::Pkcs8, hex::encode(k.secret_pkcs8_der())),
            PrivateKeyDer::Sec1(k) => (KeyType::Sec1, hex::encode(k.secret_sec1_der())),
            _ => (KeyType::Pkcs8, hex::encode(cert.private_key.secret_der())),
        };
        let certs_hex = cert
            .cert_chain
            .iter()
            .map(|c| hex::encode(c.as_ref()))
            .collect();
        let created_at_secs = cert
            .info
            .created_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let expires_at_secs = cert
            .info
            .expires_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        CertFile {
            key_type,
            key_hex,
            certs_hex,
            common_name: cert.info.common_name.clone(),
            subject_alt_names: cert.info.subject_alt_names.clone(),
            validity_days: cert.info.validity_days,
            created_at_secs,
            expires_at_secs,
        }
    }

    fn into_certificate(self) -> Result<Certificate, CertError> {
        let key_bytes = hex::decode(&self.key_hex).map_err(|e| {
            CertError::StorageError(format!("Hex decode for private key failed: {}", e))
        })?;

        let private_key = match self.key_type {
            KeyType::Pkcs1 => PrivateKeyDer::Pkcs1(key_bytes.into()),
            KeyType::Pkcs8 => PrivateKeyDer::Pkcs8(key_bytes.into()),
            KeyType::Sec1 => PrivateKeyDer::Sec1(key_bytes.into()),
        };

        let cert_chain: Result<Vec<CertificateDer<'static>>, CertError> = self
            .certs_hex
            .iter()
            .map(|h| {
                hex::decode(h).map(CertificateDer::from).map_err(|e| {
                    CertError::StorageError(format!(
                        "Hex decode for cert chain entry failed: {}",
                        e
                    ))
                })
            })
            .collect();

        let cert_chain = cert_chain?;

        let created_at = UNIX_EPOCH + Duration::from_secs(self.created_at_secs);
        let expires_at = UNIX_EPOCH + Duration::from_secs(self.expires_at_secs);

        let info = CertInfo {
            common_name: self.common_name,
            subject_alt_names: self.subject_alt_names,
            validity_days: self.validity_days,
            created_at,
            expires_at,
        };

        Ok(Certificate {
            cert_chain,
            private_key,
            info,
        })
    }
}

/// Certificate storage backend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    /// In-memory storage (not persistent)
    Memory,

    /// File-based storage
    File,
}

/// Certificate storage
pub struct CertStorage {
    /// Storage backend type
    backend: StorageBackend,

    /// Base directory for file storage
    cert_dir: Option<PathBuf>,

    /// In-memory certificate cache
    memory_cache: Arc<RwLock<HashMap<String, Certificate>>>,
}

impl CertStorage {
    /// Create a new in-memory certificate storage
    pub fn memory() -> Self {
        Self {
            backend: StorageBackend::Memory,
            cert_dir: None,
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new file-based certificate storage
    pub fn file(cert_dir: PathBuf) -> Self {
        Self {
            backend: StorageBackend::File,
            cert_dir: Some(cert_dir),
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a certificate
    pub async fn store(&self, node_id: &str, cert: Certificate) -> Result<(), CertError> {
        // Write to disk before updating the in-memory cache so a crash before
        // the cache update leaves a consistent on-disk state.
        if self.backend == StorageBackend::File {
            if let Some(dir) = &self.cert_dir {
                let path = dir.join(format!("{}.cert.json", node_id));
                let record = CertFile::from_cert(&cert);
                let json = serde_json::to_string_pretty(&record).map_err(|e| {
                    CertError::StorageError(format!("JSON serialisation failed: {}", e))
                })?;
                tokio::fs::write(&path, json.as_bytes())
                    .await
                    .map_err(|e| {
                        CertError::StorageError(format!("Write to {:?} failed: {}", path, e))
                    })?;
            }
        }

        let mut cache = self.memory_cache.write().await;
        cache.insert(node_id.to_string(), cert);

        Ok(())
    }

    /// Retrieve a certificate
    pub async fn retrieve(&self, node_id: &str) -> Result<Certificate, CertError> {
        // Check memory cache first
        {
            let cache = self.memory_cache.read().await;
            if let Some(cert) = cache.get(node_id) {
                return Ok(Certificate {
                    cert_chain: cert.cert_chain.clone(),
                    private_key: cert.private_key.clone_key(),
                    info: cert.info.clone(),
                });
            }
        }

        // Fall through to file backend if applicable
        if self.backend == StorageBackend::File {
            if let Some(dir) = &self.cert_dir {
                let path = dir.join(format!("{}.cert.json", node_id));
                match tokio::fs::read(&path).await {
                    Ok(bytes) => {
                        let record: CertFile = serde_json::from_slice(&bytes).map_err(|e| {
                            CertError::StorageError(format!(
                                "JSON deserialisation of {:?} failed: {}",
                                path, e
                            ))
                        })?;
                        let cert = record.into_certificate()?;
                        // Populate cache for future reads
                        let mut cache = self.memory_cache.write().await;
                        cache.insert(
                            node_id.to_string(),
                            Certificate {
                                cert_chain: cert.cert_chain.clone(),
                                private_key: cert.private_key.clone_key(),
                                info: cert.info.clone(),
                            },
                        );
                        return Ok(cert);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // File not found is not an error; fall through to NotFound below.
                    }
                    Err(e) => {
                        return Err(CertError::StorageError(format!(
                            "Read from {:?} failed: {}",
                            path, e
                        )));
                    }
                }
            }
        }

        Err(CertError::NotFound {
            identifier: node_id.to_string(),
        })
    }

    /// List all stored certificates
    pub async fn list(&self) -> Vec<CertInfo> {
        let cache = self.memory_cache.read().await;
        cache.values().map(|c| c.info.clone()).collect()
    }

    /// Delete a certificate
    pub async fn delete(&self, node_id: &str) -> Result<(), CertError> {
        if self.backend == StorageBackend::File {
            if let Some(dir) = &self.cert_dir {
                let path = dir.join(format!("{}.cert.json", node_id));
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // Treat not-found as a no-op: idempotent delete.
                    }
                    Err(e) => {
                        return Err(CertError::StorageError(format!(
                            "Delete {:?} failed: {}",
                            path, e
                        )));
                    }
                }
            }
        }

        let mut cache = self.memory_cache.write().await;
        cache.remove(node_id);

        Ok(())
    }

    /// Clean up expired certificates
    pub async fn cleanup_expired(&self) -> usize {
        let mut cache = self.memory_cache.write().await;
        let before = cache.len();

        cache.retain(|_, cert| !cert.is_expired());

        let after = cache.len();
        before - after
    }

    /// Get storage backend type
    pub fn backend(&self) -> StorageBackend {
        self.backend
    }

    /// Get certificate directory (if file-based)
    pub fn cert_dir(&self) -> Option<&PathBuf> {
        self.cert_dir.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_crypto() {
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    #[tokio::test]
    async fn test_memory_storage() {
        init_crypto();
        let storage = CertStorage::memory();
        assert_eq!(storage.backend(), StorageBackend::Memory);

        // Store certificate
        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();
        storage.store("test-node", cert).await.unwrap();

        // Retrieve certificate
        let retrieved = storage.retrieve("test-node").await;
        assert!(retrieved.is_ok());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.info.common_name, "test-node");
    }

    #[tokio::test]
    async fn test_list_certificates() {
        init_crypto();
        let storage = CertStorage::memory();

        // Store multiple certificates
        for i in 0..3 {
            let node_id = format!("node-{}", i);
            let cert = Certificate::generate_self_signed(node_id.clone(), 365).unwrap();
            storage.store(&node_id, cert).await.unwrap();
        }

        // List all
        let certs = storage.list().await;
        assert_eq!(certs.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_certificate() {
        init_crypto();
        let storage = CertStorage::memory();

        let cert = Certificate::generate_self_signed("test-node".to_string(), 365).unwrap();
        storage.store("test-node", cert).await.unwrap();

        // Verify it exists
        assert!(storage.retrieve("test-node").await.is_ok());

        // Delete it
        storage.delete("test-node").await.unwrap();

        // Verify it's gone
        assert!(storage.retrieve("test-node").await.is_err());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        init_crypto();
        let storage = CertStorage::memory();

        // Store valid certificate
        let cert1 = Certificate::generate_self_signed("node-1".to_string(), 365).unwrap();
        storage.store("node-1", cert1).await.unwrap();

        // Create a certificate with minimal validity that will appear expired
        // (1 day, but we'll manually create one with past expiry)
        let mut cert2 = Certificate::generate_self_signed("node-2".to_string(), 1).unwrap();
        // Manually set expiry to the past
        cert2.info.expires_at =
            std::time::SystemTime::now() - std::time::Duration::from_secs(86400);
        storage.store("node-2", cert2).await.unwrap();

        // Cleanup
        let removed = storage.cleanup_expired().await;
        assert_eq!(removed, 1);

        // Verify only valid cert remains
        let remaining = storage.list().await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].common_name, "node-1");
    }
}
