//! Certificate storage and persistence
//!
//! Handles storing certificates to disk for persistence across restarts.
//! Supports both in-memory and file-based storage.

use super::{CertError, CertInfo, Certificate};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

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
        // Always cache in memory
        let mut cache = self.memory_cache.write().await;
        cache.insert(node_id.to_string(), cert);

        // TODO: If file backend, write to disk
        // This would serialize the certificate and private key to files

        Ok(())
    }

    /// Retrieve a certificate
    pub async fn retrieve(&self, node_id: &str) -> Result<Certificate, CertError> {
        // Check memory cache first
        let cache = self.memory_cache.read().await;
        if let Some(cert) = cache.get(node_id) {
            return Ok(Certificate {
                cert_chain: cert.cert_chain.clone(),
                private_key: cert.private_key.clone_key(),
                info: cert.info.clone(),
            });
        }

        // TODO: If file backend, try to load from disk

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
        let mut cache = self.memory_cache.write().await;
        cache.remove(node_id);

        // TODO: If file backend, delete from disk

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
