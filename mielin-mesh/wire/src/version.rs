//! Protocol versioning and negotiation
//!
//! Provides version negotiation, backward compatibility, and protocol upgrades.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Protocol version identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolVersion {
    /// Create a new protocol version
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Current protocol version (v0.2.0)
    pub const CURRENT: Self = Self::new(0, 2, 0);

    /// Minimum supported version for backward compatibility (v0.1.0)
    pub const MIN_SUPPORTED: Self = Self::new(0, 1, 0);

    /// Check if this version is compatible with another version
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        // Same major version is required for compatibility
        if self.major != other.major {
            return false;
        }

        // For major version 0, minor versions must match (pre-1.0 unstable API)
        if self.major == 0 {
            return self.minor == other.minor;
        }

        // For major version >= 1, any version in the same major is compatible
        true
    }

    /// Check if this version supports the given feature
    pub fn supports_feature(&self, feature: ProtocolFeature) -> bool {
        match feature {
            ProtocolFeature::BasicMessaging => self >= &Self::new(0, 1, 0),
            ProtocolFeature::Compression => self >= &Self::new(0, 1, 0),
            ProtocolFeature::PriorityQueues => self >= &Self::new(0, 1, 0),
            ProtocolFeature::FlowControl => self >= &Self::new(0, 1, 0),
            ProtocolFeature::Acknowledgments => self >= &Self::new(0, 1, 0),
            ProtocolFeature::HealthMonitoring => self >= &Self::new(0, 1, 0),
            ProtocolFeature::ProtocolVersioning => self >= &Self::new(0, 2, 0),
            ProtocolFeature::WebSocketSupport => self >= &Self::new(0, 2, 0),
            ProtocolFeature::SecurityHardening => self >= &Self::new(0, 2, 0),
            ProtocolFeature::CustomExtensions => self >= &Self::new(0, 2, 0),
        }
    }

    /// Parse version from string (e.g., "0.2.0")
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::InvalidFormat(s.to_string()));
        }

        let major = parts[0]
            .parse()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let minor = parts[1]
            .parse()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let patch = parts[2]
            .parse()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;

        Ok(Self::new(major, minor, patch))
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Protocol features that can be negotiated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolFeature {
    /// Basic message send/receive
    BasicMessaging,
    /// Message compression (LZ4, Zstd)
    Compression,
    /// Priority-based message queuing
    PriorityQueues,
    /// Flow control and backpressure
    FlowControl,
    /// Reliable delivery with acknowledgments
    Acknowledgments,
    /// Connection health monitoring
    HealthMonitoring,
    /// Protocol versioning support
    ProtocolVersioning,
    /// WebSocket transport support
    WebSocketSupport,
    /// Enhanced security features
    SecurityHardening,
    /// Custom protocol extensions
    CustomExtensions,
}

/// Version negotiation message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionNegotiation {
    /// Versions supported by this peer (in order of preference)
    pub supported_versions: Vec<ProtocolVersion>,
    /// Features supported by this peer
    pub supported_features: Vec<ProtocolFeature>,
    /// Custom capabilities (for extensions)
    pub capabilities: HashMap<String, String>,
}

impl VersionNegotiation {
    /// Create a new version negotiation with current capabilities
    pub fn current() -> Self {
        Self {
            supported_versions: vec![ProtocolVersion::CURRENT, ProtocolVersion::new(0, 1, 0)],
            supported_features: vec![
                ProtocolFeature::BasicMessaging,
                ProtocolFeature::Compression,
                ProtocolFeature::PriorityQueues,
                ProtocolFeature::FlowControl,
                ProtocolFeature::Acknowledgments,
                ProtocolFeature::HealthMonitoring,
                ProtocolFeature::ProtocolVersioning,
                ProtocolFeature::WebSocketSupport,
                ProtocolFeature::SecurityHardening,
                ProtocolFeature::CustomExtensions,
            ],
            capabilities: HashMap::new(),
        }
    }

    /// Add a custom capability
    pub fn with_capability(mut self, key: String, value: String) -> Self {
        self.capabilities.insert(key, value);
        self
    }

    /// Find the best compatible version with another peer
    pub fn negotiate(&self, other: &Self) -> Result<NegotiationResult, VersionError> {
        // Find the highest mutually supported version
        for our_version in &self.supported_versions {
            for their_version in &other.supported_versions {
                if our_version.is_compatible_with(their_version) {
                    // Use the lower version for maximum compatibility
                    let agreed_version = std::cmp::min(our_version, their_version);

                    // Find common features
                    let common_features: Vec<ProtocolFeature> = self
                        .supported_features
                        .iter()
                        .filter(|f| other.supported_features.contains(f))
                        .filter(|f| agreed_version.supports_feature(**f))
                        .copied()
                        .collect();

                    return Ok(NegotiationResult {
                        agreed_version: *agreed_version,
                        enabled_features: common_features,
                        upgrade_available: our_version > agreed_version
                            || their_version > agreed_version,
                    });
                }
            }
        }

        Err(VersionError::IncompatibleVersions(
            self.supported_versions.clone(),
            other.supported_versions.clone(),
        ))
    }
}

/// Result of version negotiation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationResult {
    /// The agreed protocol version
    pub agreed_version: ProtocolVersion,
    /// Features enabled for this connection
    pub enabled_features: Vec<ProtocolFeature>,
    /// Whether an upgrade is available
    pub upgrade_available: bool,
}

/// Protocol upgrade request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// Current protocol version
    pub current_version: ProtocolVersion,
    /// Desired protocol version
    pub target_version: ProtocolVersion,
    /// Reason for upgrade
    pub reason: String,
}

/// Protocol upgrade response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    /// Whether upgrade was accepted
    pub accepted: bool,
    /// New protocol version (if accepted)
    pub new_version: Option<ProtocolVersion>,
    /// Error message (if rejected)
    pub error: Option<String>,
}

/// Version negotiator for managing protocol versions
pub struct VersionNegotiator {
    /// Our version negotiation info
    negotiation: VersionNegotiation,
    /// Active negotiations by peer ID
    active_negotiations: tokio::sync::RwLock<HashMap<String, NegotiationResult>>,
}

impl VersionNegotiator {
    /// Create a new version negotiator with current protocol version
    pub fn new() -> Self {
        Self {
            negotiation: VersionNegotiation::current(),
            active_negotiations: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Create with custom version negotiation
    pub fn with_negotiation(negotiation: VersionNegotiation) -> Self {
        Self {
            negotiation,
            active_negotiations: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Get our version negotiation info
    pub fn negotiation(&self) -> &VersionNegotiation {
        &self.negotiation
    }

    /// Negotiate with a peer
    pub async fn negotiate_with_peer(
        &self,
        peer_id: &str,
        peer_negotiation: &VersionNegotiation,
    ) -> Result<NegotiationResult, VersionError> {
        let result = self.negotiation.negotiate(peer_negotiation)?;

        // Store the result
        let mut negotiations = self.active_negotiations.write().await;
        negotiations.insert(peer_id.to_string(), result.clone());

        Ok(result)
    }

    /// Get negotiation result for a peer
    pub async fn get_peer_negotiation(&self, peer_id: &str) -> Option<NegotiationResult> {
        let negotiations = self.active_negotiations.read().await;
        negotiations.get(peer_id).cloned()
    }

    /// Check if a feature is enabled for a peer
    pub async fn is_feature_enabled(
        &self,
        peer_id: &str,
        feature: ProtocolFeature,
    ) -> Result<bool, VersionError> {
        let negotiations = self.active_negotiations.read().await;
        let result = negotiations
            .get(peer_id)
            .ok_or_else(|| VersionError::NegotiationNotFound(peer_id.to_string()))?;

        Ok(result.enabled_features.contains(&feature))
    }

    /// Request protocol upgrade for a peer
    pub fn create_upgrade_request(
        &self,
        current: ProtocolVersion,
        target: ProtocolVersion,
        reason: String,
    ) -> UpgradeRequest {
        UpgradeRequest {
            current_version: current,
            target_version: target,
            reason,
        }
    }

    /// Handle upgrade request from a peer
    pub async fn handle_upgrade_request(
        &self,
        peer_id: &str,
        request: &UpgradeRequest,
    ) -> UpgradeResponse {
        // Check if target version is supported
        if !self
            .negotiation
            .supported_versions
            .contains(&request.target_version)
        {
            return UpgradeResponse {
                accepted: false,
                new_version: None,
                error: Some(format!(
                    "Target version {} not supported",
                    request.target_version
                )),
            };
        }

        // Check if current negotiation exists
        let negotiations = self.active_negotiations.read().await;
        if !negotiations.contains_key(peer_id) {
            return UpgradeResponse {
                accepted: false,
                new_version: None,
                error: Some("No active negotiation found".to_string()),
            };
        }

        // Accept the upgrade
        drop(negotiations);
        let mut negotiations = self.active_negotiations.write().await;
        if let Some(result) = negotiations.get_mut(peer_id) {
            result.agreed_version = request.target_version;
            result.upgrade_available = false;
        }

        UpgradeResponse {
            accepted: true,
            new_version: Some(request.target_version),
            error: None,
        }
    }

    /// Remove negotiation for a disconnected peer
    pub async fn remove_peer(&self, peer_id: &str) {
        let mut negotiations = self.active_negotiations.write().await;
        negotiations.remove(peer_id);
    }
}

impl Default for VersionNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors related to version negotiation
#[derive(Debug, Error)]
pub enum VersionError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),

    #[error("Incompatible protocol versions: ours={0:?}, theirs={1:?}")]
    IncompatibleVersions(Vec<ProtocolVersion>, Vec<ProtocolVersion>),

    #[error("Feature not supported: {0:?}")]
    FeatureNotSupported(ProtocolFeature),

    #[error("Negotiation not found for peer: {0}")]
    NegotiationNotFound(String),

    #[error("Protocol upgrade failed: {0}")]
    UpgradeFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_creation() {
        let v = ProtocolVersion::new(0, 2, 0);
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_version_comparison() {
        let v1 = ProtocolVersion::new(0, 1, 0);
        let v2 = ProtocolVersion::new(0, 2, 0);
        let v3 = ProtocolVersion::new(1, 0, 0);

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert_eq!(v1, v1);
    }

    #[test]
    fn test_version_compatibility() {
        let v010 = ProtocolVersion::new(0, 1, 0);
        let v020 = ProtocolVersion::new(0, 2, 0);
        let v100 = ProtocolVersion::new(1, 0, 0);
        let v110 = ProtocolVersion::new(1, 1, 0);

        // Major version 0: minor must match
        assert!(!v010.is_compatible_with(&v020));
        assert!(v010.is_compatible_with(&v010));

        // Major version >= 1: any minor in same major is compatible
        assert!(v100.is_compatible_with(&v110));
        assert!(!v100.is_compatible_with(&v020));
    }

    #[test]
    fn test_version_parse() {
        let v = ProtocolVersion::parse("0.2.0").unwrap();
        assert_eq!(v, ProtocolVersion::new(0, 2, 0));

        assert!(ProtocolVersion::parse("invalid").is_err());
        assert!(ProtocolVersion::parse("1.2").is_err());
        assert!(ProtocolVersion::parse("1.2.3.4").is_err());
    }

    #[test]
    fn test_version_to_string() {
        let v = ProtocolVersion::new(0, 2, 0);
        assert_eq!(v.to_string(), "0.2.0");
        assert_eq!(format!("{}", v), "0.2.0");
    }

    #[test]
    fn test_feature_support() {
        let v010 = ProtocolVersion::new(0, 1, 0);
        let v020 = ProtocolVersion::new(0, 2, 0);

        // v0.1.0 features
        assert!(v010.supports_feature(ProtocolFeature::BasicMessaging));
        assert!(v010.supports_feature(ProtocolFeature::Compression));
        assert!(!v010.supports_feature(ProtocolFeature::ProtocolVersioning));

        // v0.2.0 features
        assert!(v020.supports_feature(ProtocolFeature::BasicMessaging));
        assert!(v020.supports_feature(ProtocolFeature::ProtocolVersioning));
        assert!(v020.supports_feature(ProtocolFeature::WebSocketSupport));
    }

    #[test]
    fn test_version_negotiation() {
        let our_neg = VersionNegotiation::current();
        let their_neg = VersionNegotiation {
            supported_versions: vec![ProtocolVersion::new(0, 2, 0)],
            supported_features: vec![
                ProtocolFeature::BasicMessaging,
                ProtocolFeature::Compression,
            ],
            capabilities: HashMap::new(),
        };

        let result = our_neg.negotiate(&their_neg).unwrap();
        assert_eq!(result.agreed_version, ProtocolVersion::new(0, 2, 0));
        assert!(result
            .enabled_features
            .contains(&ProtocolFeature::BasicMessaging));
        assert!(result
            .enabled_features
            .contains(&ProtocolFeature::Compression));
    }

    #[test]
    fn test_incompatible_negotiation() {
        let our_neg = VersionNegotiation {
            supported_versions: vec![ProtocolVersion::new(0, 2, 0)],
            supported_features: vec![],
            capabilities: HashMap::new(),
        };
        let their_neg = VersionNegotiation {
            supported_versions: vec![ProtocolVersion::new(1, 0, 0)],
            supported_features: vec![],
            capabilities: HashMap::new(),
        };

        assert!(our_neg.negotiate(&their_neg).is_err());
    }

    #[tokio::test]
    async fn test_version_negotiator() {
        let negotiator = VersionNegotiator::new();
        let peer_neg = VersionNegotiation::current();

        let result = negotiator
            .negotiate_with_peer("peer1", &peer_neg)
            .await
            .unwrap();
        assert_eq!(result.agreed_version, ProtocolVersion::CURRENT);

        let stored = negotiator.get_peer_negotiation("peer1").await.unwrap();
        assert_eq!(stored.agreed_version, ProtocolVersion::CURRENT);
    }

    #[tokio::test]
    async fn test_feature_enabled_check() {
        let negotiator = VersionNegotiator::new();
        let peer_neg = VersionNegotiation::current();

        negotiator
            .negotiate_with_peer("peer1", &peer_neg)
            .await
            .unwrap();

        assert!(negotiator
            .is_feature_enabled("peer1", ProtocolFeature::Compression)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_protocol_upgrade() {
        let negotiator = VersionNegotiator::new();
        let peer_neg = VersionNegotiation {
            supported_versions: vec![ProtocolVersion::new(0, 1, 0)],
            supported_features: vec![ProtocolFeature::BasicMessaging],
            capabilities: HashMap::new(),
        };

        negotiator
            .negotiate_with_peer("peer1", &peer_neg)
            .await
            .unwrap();

        let upgrade_req = negotiator.create_upgrade_request(
            ProtocolVersion::new(0, 1, 0),
            ProtocolVersion::new(0, 2, 0),
            "New features available".to_string(),
        );

        let response = negotiator
            .handle_upgrade_request("peer1", &upgrade_req)
            .await;
        assert!(response.accepted);
        assert_eq!(response.new_version, Some(ProtocolVersion::new(0, 2, 0)));
    }

    #[tokio::test]
    async fn test_upgrade_unsupported_version() {
        let negotiator = VersionNegotiator::new();
        let peer_neg = VersionNegotiation::current();

        negotiator
            .negotiate_with_peer("peer1", &peer_neg)
            .await
            .unwrap();

        let upgrade_req = UpgradeRequest {
            current_version: ProtocolVersion::new(0, 2, 0),
            target_version: ProtocolVersion::new(99, 0, 0),
            reason: "Test".to_string(),
        };

        let response = negotiator
            .handle_upgrade_request("peer1", &upgrade_req)
            .await;
        assert!(!response.accepted);
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn test_remove_peer() {
        let negotiator = VersionNegotiator::new();
        let peer_neg = VersionNegotiation::current();

        negotiator
            .negotiate_with_peer("peer1", &peer_neg)
            .await
            .unwrap();
        assert!(negotiator.get_peer_negotiation("peer1").await.is_some());

        negotiator.remove_peer("peer1").await;
        assert!(negotiator.get_peer_negotiation("peer1").await.is_none());
    }

    #[test]
    fn test_custom_capabilities() {
        let neg = VersionNegotiation::current()
            .with_capability("compression".to_string(), "zstd".to_string())
            .with_capability("max_message_size".to_string(), "16777216".to_string());

        assert_eq!(
            neg.capabilities.get("compression"),
            Some(&"zstd".to_string())
        );
        assert_eq!(
            neg.capabilities.get("max_message_size"),
            Some(&"16777216".to_string())
        );
    }
}
