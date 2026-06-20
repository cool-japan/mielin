//! Multi-Tenancy Support for MielinMesh
//!
//! Provides comprehensive multi-tenant isolation and resource management for
//! the MielinOS mesh network, enabling secure multi-organization deployments.
//!
//! Features:
//! - Namespace isolation between tenants
//! - Resource quotas per tenant (agents, connections, bandwidth)
//! - Tenant-aware routing with isolation enforcement
//! - Comprehensive audit logging for compliance

use crate::error::MeshNetworkError;
use crate::node::NodeId;
use crate::registry::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Multi-tenancy errors
#[derive(Debug, Error)]
pub enum MultiTenancyError {
    #[error("Tenant not found: {tenant_id}")]
    TenantNotFound { tenant_id: String },

    #[error("Namespace not found: {namespace}")]
    NamespaceNotFound { namespace: String },

    #[error("Namespace already exists: {namespace}")]
    NamespaceAlreadyExists { namespace: String },

    #[error("Tenant already exists: {tenant_id}")]
    TenantAlreadyExists { tenant_id: String },

    #[error("Quota exceeded for tenant {tenant_id}: {resource}")]
    QuotaExceeded { tenant_id: String, resource: String },

    #[error("Access denied: tenant {tenant_id} cannot access namespace {namespace}")]
    AccessDenied {
        tenant_id: String,
        namespace: String,
    },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Isolation violation: {reason}")]
    IsolationViolation { reason: String },

    #[error("Network error: {0}")]
    NetworkError(#[from] MeshNetworkError),
}

/// Unique tenant identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    /// Create a new tenant ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the tenant ID as a string reference
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Namespace identifier for tenant isolation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespaceId(pub String);

impl NamespaceId {
    /// Create a new namespace ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the namespace ID as a string reference
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Resource quota limits for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    /// Maximum number of agents allowed
    pub max_agents: usize,
    /// Maximum number of namespaces allowed
    pub max_namespaces: usize,
    /// Maximum connections per agent
    pub max_connections_per_agent: usize,
    /// Maximum total bandwidth (bytes/sec)
    pub max_bandwidth_bytes_per_sec: u64,
    /// Maximum storage (bytes)
    pub max_storage_bytes: u64,
    /// Maximum CPU time per agent (milliseconds/second)
    pub max_cpu_ms_per_sec: u64,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_agents: 1000,
            max_namespaces: 10,
            max_connections_per_agent: 100,
            max_bandwidth_bytes_per_sec: 100 * 1024 * 1024, // 100 MB/s
            max_storage_bytes: 10 * 1024 * 1024 * 1024,     // 10 GB
            max_cpu_ms_per_sec: 500,                        // 50% of one core
        }
    }
}

impl ResourceQuota {
    /// Create quota for a small tenant
    pub fn small() -> Self {
        Self {
            max_agents: 100,
            max_namespaces: 3,
            max_connections_per_agent: 50,
            max_bandwidth_bytes_per_sec: 10 * 1024 * 1024, // 10 MB/s
            max_storage_bytes: 1024 * 1024 * 1024,         // 1 GB
            max_cpu_ms_per_sec: 100,                       // 10% of one core
        }
    }

    /// Create quota for an enterprise tenant
    pub fn enterprise() -> Self {
        Self {
            max_agents: 10000,
            max_namespaces: 100,
            max_connections_per_agent: 500,
            max_bandwidth_bytes_per_sec: 1024 * 1024 * 1024, // 1 GB/s
            max_storage_bytes: 100 * 1024 * 1024 * 1024,     // 100 GB
            max_cpu_ms_per_sec: 4000,                        // 4 cores equivalent
        }
    }

    /// Create unlimited quota (for system tenant)
    pub fn unlimited() -> Self {
        Self {
            max_agents: usize::MAX,
            max_namespaces: usize::MAX,
            max_connections_per_agent: usize::MAX,
            max_bandwidth_bytes_per_sec: u64::MAX,
            max_storage_bytes: u64::MAX,
            max_cpu_ms_per_sec: u64::MAX,
        }
    }
}

/// Current resource usage for a tenant
#[derive(Debug)]
pub struct ResourceUsage {
    /// Current number of agents
    pub agents: AtomicUsize,
    /// Current number of namespaces
    pub namespaces: AtomicUsize,
    /// Current total connections
    pub connections: AtomicUsize,
    /// Current bandwidth usage (bytes/sec, EMA)
    pub bandwidth_bytes_per_sec: AtomicU64,
    /// Current storage usage (bytes)
    pub storage_bytes: AtomicU64,
    /// Current CPU usage (ms/sec, EMA)
    pub cpu_ms_per_sec: AtomicU64,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            agents: AtomicUsize::new(0),
            namespaces: AtomicUsize::new(0),
            connections: AtomicUsize::new(0),
            bandwidth_bytes_per_sec: AtomicU64::new(0),
            storage_bytes: AtomicU64::new(0),
            cpu_ms_per_sec: AtomicU64::new(0),
        }
    }
}

impl ResourceUsage {
    /// Create a snapshot of current usage
    pub fn snapshot(&self) -> ResourceUsageSnapshot {
        ResourceUsageSnapshot {
            agents: self.agents.load(Ordering::Relaxed),
            namespaces: self.namespaces.load(Ordering::Relaxed),
            connections: self.connections.load(Ordering::Relaxed),
            bandwidth_bytes_per_sec: self.bandwidth_bytes_per_sec.load(Ordering::Relaxed),
            storage_bytes: self.storage_bytes.load(Ordering::Relaxed),
            cpu_ms_per_sec: self.cpu_ms_per_sec.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of resource usage (for serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageSnapshot {
    pub agents: usize,
    pub namespaces: usize,
    pub connections: usize,
    pub bandwidth_bytes_per_sec: u64,
    pub storage_bytes: u64,
    pub cpu_ms_per_sec: u64,
}

/// Tenant status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TenantStatus {
    /// Tenant is active and operational
    Active,
    /// Tenant is suspended (quota exceeded, policy violation, etc.)
    Suspended,
    /// Tenant is disabled by admin
    Disabled,
    /// Tenant is pending approval
    #[default]
    Pending,
}

/// Tenant information
#[derive(Debug)]
pub struct Tenant {
    /// Unique tenant ID
    pub id: TenantId,
    /// Human-readable name
    pub name: String,
    /// Tenant status
    pub status: RwLock<TenantStatus>,
    /// Resource quotas
    pub quota: ResourceQuota,
    /// Current resource usage
    pub usage: ResourceUsage,
    /// Namespaces owned by this tenant
    pub namespaces: RwLock<Vec<NamespaceId>>,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last activity timestamp
    pub last_activity: RwLock<Instant>,
    /// Tenant metadata
    pub metadata: RwLock<HashMap<String, String>>,
}

impl Tenant {
    /// Create a new tenant
    pub fn new(id: TenantId, name: String, quota: ResourceQuota) -> Self {
        Self {
            id,
            name,
            status: RwLock::new(TenantStatus::Pending),
            quota,
            usage: ResourceUsage::default(),
            namespaces: RwLock::new(Vec::new()),
            created_at: SystemTime::now(),
            last_activity: RwLock::new(Instant::now()),
            metadata: RwLock::new(HashMap::new()),
        }
    }

    /// Activate the tenant
    pub async fn activate(&self) {
        let mut status = self.status.write().await;
        *status = TenantStatus::Active;
        info!(tenant_id = %self.id, "Tenant activated");
    }

    /// Suspend the tenant
    pub async fn suspend(&self) {
        let mut status = self.status.write().await;
        *status = TenantStatus::Suspended;
        warn!(tenant_id = %self.id, "Tenant suspended");
    }

    /// Check if tenant is active
    pub async fn is_active(&self) -> bool {
        let status = self.status.read().await;
        *status == TenantStatus::Active
    }

    /// Update last activity timestamp
    pub async fn touch(&self) {
        let mut last_activity = self.last_activity.write().await;
        *last_activity = Instant::now();
    }
}

/// Namespace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    /// Namespace ID
    pub id: NamespaceId,
    /// Owner tenant ID
    pub tenant_id: TenantId,
    /// Namespace-specific resource limits (overrides tenant defaults)
    pub resource_limits: Option<ResourceQuota>,
    /// Labels for organization
    pub labels: HashMap<String, String>,
}

/// Namespace with runtime state
#[derive(Debug)]
pub struct Namespace {
    /// Configuration
    pub config: NamespaceConfig,
    /// Agents in this namespace
    pub agents: RwLock<Vec<AgentId>>,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Resource usage specific to this namespace
    pub usage: ResourceUsage,
}

impl Namespace {
    /// Create a new namespace
    pub fn new(config: NamespaceConfig) -> Self {
        Self {
            config,
            agents: RwLock::new(Vec::new()),
            created_at: SystemTime::now(),
            usage: ResourceUsage::default(),
        }
    }

    /// Add agent to namespace
    pub async fn add_agent(&self, agent_id: AgentId) {
        let mut agents = self.agents.write().await;
        if !agents.contains(&agent_id) {
            agents.push(agent_id);
            self.usage.agents.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Remove agent from namespace
    pub async fn remove_agent(&self, agent_id: &AgentId) -> bool {
        let mut agents = self.agents.write().await;
        if let Some(pos) = agents.iter().position(|a| a == agent_id) {
            agents.remove(pos);
            self.usage.agents.fetch_sub(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Check if agent is in this namespace
    pub async fn contains_agent(&self, agent_id: &AgentId) -> bool {
        let agents = self.agents.read().await;
        agents.contains(agent_id)
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Timestamp of the event
    pub timestamp: SystemTime,
    /// Tenant that performed the action
    pub tenant_id: TenantId,
    /// Namespace (if applicable)
    pub namespace_id: Option<NamespaceId>,
    /// Type of action
    pub action: AuditAction,
    /// Target of the action (e.g., agent ID, resource name)
    pub target: String,
    /// Outcome of the action
    pub outcome: AuditOutcome,
    /// Additional details
    pub details: HashMap<String, String>,
    /// Source node ID
    pub source_node: Option<NodeId>,
}

/// Types of auditable actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    /// Agent was created
    AgentCreated,
    /// Agent was deleted
    AgentDeleted,
    /// Agent was migrated
    AgentMigrated,
    /// Connection established
    ConnectionEstablished,
    /// Connection terminated
    ConnectionTerminated,
    /// Namespace created
    NamespaceCreated,
    /// Namespace deleted
    NamespaceDeleted,
    /// Resource quota modified
    QuotaModified,
    /// Tenant status changed
    TenantStatusChanged,
    /// Access attempt
    AccessAttempt,
    /// Configuration changed
    ConfigChanged,
    /// Policy violation detected
    PolicyViolation,
    /// Custom action
    Custom(String),
}

/// Outcome of an audited action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOutcome {
    /// Action succeeded
    Success,
    /// Action failed with reason
    Failure(String),
    /// Action was denied
    Denied(String),
}

/// Audit log configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Maximum entries to keep in memory
    pub max_entries: usize,
    /// Retention period for audit logs
    pub retention: Duration,
    /// Actions to audit (empty = all)
    pub audited_actions: Vec<AuditAction>,
    /// Enable detailed logging
    pub verbose: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            retention: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            audited_actions: Vec::new(),                       // All actions
            verbose: false,
        }
    }
}

/// Audit logger
#[derive(Debug)]
pub struct AuditLog {
    /// Configuration
    config: AuditConfig,
    /// Log entries
    entries: RwLock<Vec<AuditEntry>>,
    /// Entry counter
    total_entries: AtomicU64,
    /// Dropped entries (due to capacity)
    dropped_entries: AtomicU64,
}

impl AuditLog {
    /// Create a new audit log
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            entries: RwLock::new(Vec::new()),
            total_entries: AtomicU64::new(0),
            dropped_entries: AtomicU64::new(0),
        }
    }

    /// Log an audit entry
    pub async fn log(&self, entry: AuditEntry) {
        self.total_entries.fetch_add(1, Ordering::Relaxed);

        let mut entries = self.entries.write().await;

        // Check if we need to make room
        if entries.len() >= self.config.max_entries {
            // Remove oldest entry
            if !entries.is_empty() {
                entries.remove(0);
                self.dropped_entries.fetch_add(1, Ordering::Relaxed);
            }
        }

        if self.config.verbose {
            debug!(
                tenant_id = %entry.tenant_id,
                action = ?entry.action,
                outcome = ?entry.outcome,
                "Audit log entry"
            );
        }

        entries.push(entry);
    }

    /// Get entries for a specific tenant
    pub async fn get_entries_for_tenant(&self, tenant_id: &TenantId) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|e| &e.tenant_id == tenant_id)
            .cloned()
            .collect()
    }

    /// Get entries by action type
    pub async fn get_entries_by_action(&self, action: &AuditAction) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|e| std::mem::discriminant(&e.action) == std::mem::discriminant(action))
            .cloned()
            .collect()
    }

    /// Get all entries
    pub async fn get_all_entries(&self) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries.clone()
    }

    /// Get statistics
    pub fn stats(&self) -> AuditLogStats {
        AuditLogStats {
            total_entries: self.total_entries.load(Ordering::Relaxed),
            dropped_entries: self.dropped_entries.load(Ordering::Relaxed),
        }
    }

    /// Clear old entries based on retention period
    pub async fn cleanup(&self) {
        let mut entries = self.entries.write().await;
        let cutoff = SystemTime::now()
            .checked_sub(self.config.retention)
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let before_len = entries.len();
        entries.retain(|e| e.timestamp > cutoff);
        let removed = before_len - entries.len();

        if removed > 0 {
            self.dropped_entries
                .fetch_add(removed as u64, Ordering::Relaxed);
            debug!(removed = removed, "Cleaned up old audit entries");
        }
    }
}

/// Audit log statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogStats {
    pub total_entries: u64,
    pub dropped_entries: u64,
}

/// Routing policy for tenant isolation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RoutingPolicy {
    /// Strict isolation: no cross-tenant communication
    #[default]
    Strict,
    /// Allow explicit cross-tenant routing with permission
    PermissionBased,
    /// Allow routing within same namespace only
    NamespaceOnly,
}

/// Cross-tenant permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTenantPermission {
    /// Source tenant
    pub source_tenant: TenantId,
    /// Target tenant
    pub target_tenant: TenantId,
    /// Allowed namespaces (empty = all)
    pub allowed_namespaces: Vec<NamespaceId>,
    /// Expiration time
    pub expires_at: Option<SystemTime>,
}

/// Multi-tenancy manager
pub struct TenantManager {
    /// All tenants
    tenants: RwLock<HashMap<TenantId, Arc<Tenant>>>,
    /// All namespaces
    namespaces: RwLock<HashMap<NamespaceId, Arc<Namespace>>>,
    /// Routing policy
    routing_policy: RwLock<RoutingPolicy>,
    /// Cross-tenant permissions
    cross_tenant_permissions: RwLock<Vec<CrossTenantPermission>>,
    /// Audit log
    audit_log: Arc<AuditLog>,
    /// Total agents across all tenants
    total_agents: AtomicUsize,
    /// Total namespaces
    total_namespaces: AtomicUsize,
}

impl TenantManager {
    /// Create a new tenant manager
    pub fn new(audit_config: AuditConfig) -> Self {
        Self {
            tenants: RwLock::new(HashMap::new()),
            namespaces: RwLock::new(HashMap::new()),
            routing_policy: RwLock::new(RoutingPolicy::default()),
            cross_tenant_permissions: RwLock::new(Vec::new()),
            audit_log: Arc::new(AuditLog::new(audit_config)),
            total_agents: AtomicUsize::new(0),
            total_namespaces: AtomicUsize::new(0),
        }
    }

    /// Create a new tenant
    pub async fn create_tenant(
        &self,
        id: TenantId,
        name: String,
        quota: ResourceQuota,
    ) -> Result<Arc<Tenant>, MultiTenancyError> {
        let mut tenants = self.tenants.write().await;

        if tenants.contains_key(&id) {
            return Err(MultiTenancyError::TenantAlreadyExists {
                tenant_id: id.to_string(),
            });
        }

        let tenant = Arc::new(Tenant::new(id.clone(), name, quota));
        tenants.insert(id.clone(), tenant.clone());

        self.audit_log
            .log(AuditEntry {
                timestamp: SystemTime::now(),
                tenant_id: id,
                namespace_id: None,
                action: AuditAction::TenantStatusChanged,
                target: "tenant".to_string(),
                outcome: AuditOutcome::Success,
                details: HashMap::new(),
                source_node: None,
            })
            .await;

        Ok(tenant)
    }

    /// Get a tenant by ID
    pub async fn get_tenant(&self, id: &TenantId) -> Option<Arc<Tenant>> {
        let tenants = self.tenants.read().await;
        tenants.get(id).cloned()
    }

    /// Activate a tenant
    pub async fn activate_tenant(&self, id: &TenantId) -> Result<(), MultiTenancyError> {
        let tenant =
            self.get_tenant(id)
                .await
                .ok_or_else(|| MultiTenancyError::TenantNotFound {
                    tenant_id: id.to_string(),
                })?;

        tenant.activate().await;

        self.audit_log
            .log(AuditEntry {
                timestamp: SystemTime::now(),
                tenant_id: id.clone(),
                namespace_id: None,
                action: AuditAction::TenantStatusChanged,
                target: "activated".to_string(),
                outcome: AuditOutcome::Success,
                details: HashMap::new(),
                source_node: None,
            })
            .await;

        Ok(())
    }

    /// Create a namespace for a tenant
    pub async fn create_namespace(
        &self,
        tenant_id: &TenantId,
        namespace_id: NamespaceId,
        labels: HashMap<String, String>,
    ) -> Result<Arc<Namespace>, MultiTenancyError> {
        let tenant =
            self.get_tenant(tenant_id)
                .await
                .ok_or_else(|| MultiTenancyError::TenantNotFound {
                    tenant_id: tenant_id.to_string(),
                })?;

        // Check if tenant is active
        if !tenant.is_active().await {
            return Err(MultiTenancyError::AccessDenied {
                tenant_id: tenant_id.to_string(),
                namespace: namespace_id.to_string(),
            });
        }

        // Check quota
        let current_namespaces = tenant.usage.namespaces.load(Ordering::Relaxed);
        if current_namespaces >= tenant.quota.max_namespaces {
            return Err(MultiTenancyError::QuotaExceeded {
                tenant_id: tenant_id.to_string(),
                resource: "namespaces".to_string(),
            });
        }

        let mut namespaces = self.namespaces.write().await;

        if namespaces.contains_key(&namespace_id) {
            return Err(MultiTenancyError::NamespaceAlreadyExists {
                namespace: namespace_id.to_string(),
            });
        }

        let config = NamespaceConfig {
            id: namespace_id.clone(),
            tenant_id: tenant_id.clone(),
            resource_limits: None,
            labels,
        };

        let namespace = Arc::new(Namespace::new(config));
        namespaces.insert(namespace_id.clone(), namespace.clone());

        // Update tenant's namespace list
        {
            let mut tenant_namespaces = tenant.namespaces.write().await;
            tenant_namespaces.push(namespace_id.clone());
        }

        tenant.usage.namespaces.fetch_add(1, Ordering::Relaxed);
        self.total_namespaces.fetch_add(1, Ordering::Relaxed);

        self.audit_log
            .log(AuditEntry {
                timestamp: SystemTime::now(),
                tenant_id: tenant_id.clone(),
                namespace_id: Some(namespace_id),
                action: AuditAction::NamespaceCreated,
                target: "namespace".to_string(),
                outcome: AuditOutcome::Success,
                details: HashMap::new(),
                source_node: None,
            })
            .await;

        Ok(namespace)
    }

    /// Get a namespace by ID
    pub async fn get_namespace(&self, id: &NamespaceId) -> Option<Arc<Namespace>> {
        let namespaces = self.namespaces.read().await;
        namespaces.get(id).cloned()
    }

    /// Delete a namespace
    pub async fn delete_namespace(
        &self,
        tenant_id: &TenantId,
        namespace_id: &NamespaceId,
    ) -> Result<(), MultiTenancyError> {
        let tenant =
            self.get_tenant(tenant_id)
                .await
                .ok_or_else(|| MultiTenancyError::TenantNotFound {
                    tenant_id: tenant_id.to_string(),
                })?;

        let mut namespaces = self.namespaces.write().await;

        let namespace =
            namespaces
                .get(namespace_id)
                .ok_or_else(|| MultiTenancyError::NamespaceNotFound {
                    namespace: namespace_id.to_string(),
                })?;

        // Verify ownership
        if &namespace.config.tenant_id != tenant_id {
            return Err(MultiTenancyError::AccessDenied {
                tenant_id: tenant_id.to_string(),
                namespace: namespace_id.to_string(),
            });
        }

        namespaces.remove(namespace_id);

        // Update tenant's namespace list
        {
            let mut tenant_namespaces = tenant.namespaces.write().await;
            tenant_namespaces.retain(|n| n != namespace_id);
        }

        tenant.usage.namespaces.fetch_sub(1, Ordering::Relaxed);
        self.total_namespaces.fetch_sub(1, Ordering::Relaxed);

        self.audit_log
            .log(AuditEntry {
                timestamp: SystemTime::now(),
                tenant_id: tenant_id.clone(),
                namespace_id: Some(namespace_id.clone()),
                action: AuditAction::NamespaceDeleted,
                target: "namespace".to_string(),
                outcome: AuditOutcome::Success,
                details: HashMap::new(),
                source_node: None,
            })
            .await;

        Ok(())
    }

    /// Register an agent in a namespace
    pub async fn register_agent(
        &self,
        tenant_id: &TenantId,
        namespace_id: &NamespaceId,
        agent_id: AgentId,
    ) -> Result<(), MultiTenancyError> {
        let tenant =
            self.get_tenant(tenant_id)
                .await
                .ok_or_else(|| MultiTenancyError::TenantNotFound {
                    tenant_id: tenant_id.to_string(),
                })?;

        // Check if tenant is active
        if !tenant.is_active().await {
            return Err(MultiTenancyError::AccessDenied {
                tenant_id: tenant_id.to_string(),
                namespace: namespace_id.to_string(),
            });
        }

        // Check agent quota
        let current_agents = tenant.usage.agents.load(Ordering::Relaxed);
        if current_agents >= tenant.quota.max_agents {
            return Err(MultiTenancyError::QuotaExceeded {
                tenant_id: tenant_id.to_string(),
                resource: "agents".to_string(),
            });
        }

        let namespace = self.get_namespace(namespace_id).await.ok_or_else(|| {
            MultiTenancyError::NamespaceNotFound {
                namespace: namespace_id.to_string(),
            }
        })?;

        // Verify namespace ownership
        if &namespace.config.tenant_id != tenant_id {
            return Err(MultiTenancyError::AccessDenied {
                tenant_id: tenant_id.to_string(),
                namespace: namespace_id.to_string(),
            });
        }

        namespace.add_agent(agent_id).await;
        tenant.usage.agents.fetch_add(1, Ordering::Relaxed);
        self.total_agents.fetch_add(1, Ordering::Relaxed);
        tenant.touch().await;

        self.audit_log
            .log(AuditEntry {
                timestamp: SystemTime::now(),
                tenant_id: tenant_id.clone(),
                namespace_id: Some(namespace_id.clone()),
                action: AuditAction::AgentCreated,
                target: format!("{:?}", agent_id),
                outcome: AuditOutcome::Success,
                details: HashMap::new(),
                source_node: None,
            })
            .await;

        Ok(())
    }

    /// Unregister an agent from a namespace
    pub async fn unregister_agent(
        &self,
        tenant_id: &TenantId,
        namespace_id: &NamespaceId,
        agent_id: &AgentId,
    ) -> Result<(), MultiTenancyError> {
        let tenant =
            self.get_tenant(tenant_id)
                .await
                .ok_or_else(|| MultiTenancyError::TenantNotFound {
                    tenant_id: tenant_id.to_string(),
                })?;

        let namespace = self.get_namespace(namespace_id).await.ok_or_else(|| {
            MultiTenancyError::NamespaceNotFound {
                namespace: namespace_id.to_string(),
            }
        })?;

        // Verify namespace ownership
        if &namespace.config.tenant_id != tenant_id {
            return Err(MultiTenancyError::AccessDenied {
                tenant_id: tenant_id.to_string(),
                namespace: namespace_id.to_string(),
            });
        }

        if namespace.remove_agent(agent_id).await {
            tenant.usage.agents.fetch_sub(1, Ordering::Relaxed);
            self.total_agents.fetch_sub(1, Ordering::Relaxed);
            tenant.touch().await;

            self.audit_log
                .log(AuditEntry {
                    timestamp: SystemTime::now(),
                    tenant_id: tenant_id.clone(),
                    namespace_id: Some(namespace_id.clone()),
                    action: AuditAction::AgentDeleted,
                    target: format!("{:?}", agent_id),
                    outcome: AuditOutcome::Success,
                    details: HashMap::new(),
                    source_node: None,
                })
                .await;
        }

        Ok(())
    }

    /// Check if routing is allowed between two agents
    pub async fn can_route(
        &self,
        source_tenant: &TenantId,
        source_namespace: &NamespaceId,
        target_tenant: &TenantId,
        target_namespace: &NamespaceId,
    ) -> Result<bool, MultiTenancyError> {
        let policy = self.routing_policy.read().await;

        match *policy {
            RoutingPolicy::Strict => {
                // Same tenant and namespace only
                if source_tenant != target_tenant || source_namespace != target_namespace {
                    self.audit_log
                        .log(AuditEntry {
                            timestamp: SystemTime::now(),
                            tenant_id: source_tenant.clone(),
                            namespace_id: Some(source_namespace.clone()),
                            action: AuditAction::AccessAttempt,
                            target: format!("{}:{}", target_tenant, target_namespace),
                            outcome: AuditOutcome::Denied("strict isolation".to_string()),
                            details: HashMap::new(),
                            source_node: None,
                        })
                        .await;
                    return Ok(false);
                }
                Ok(true)
            }
            RoutingPolicy::NamespaceOnly => {
                // Same namespace within same tenant
                if source_tenant != target_tenant {
                    return Ok(false);
                }
                Ok(source_namespace == target_namespace)
            }
            RoutingPolicy::PermissionBased => {
                // Same tenant always allowed
                if source_tenant == target_tenant {
                    return Ok(true);
                }

                // Check cross-tenant permissions
                let permissions = self.cross_tenant_permissions.read().await;
                let now = SystemTime::now();

                for perm in permissions.iter() {
                    if &perm.source_tenant == source_tenant && &perm.target_tenant == target_tenant
                    {
                        // Check expiration
                        if let Some(expires_at) = perm.expires_at {
                            if now > expires_at {
                                continue;
                            }
                        }

                        // Check namespace allowlist
                        if perm.allowed_namespaces.is_empty()
                            || perm.allowed_namespaces.contains(target_namespace)
                        {
                            return Ok(true);
                        }
                    }
                }

                self.audit_log
                    .log(AuditEntry {
                        timestamp: SystemTime::now(),
                        tenant_id: source_tenant.clone(),
                        namespace_id: Some(source_namespace.clone()),
                        action: AuditAction::AccessAttempt,
                        target: format!("{}:{}", target_tenant, target_namespace),
                        outcome: AuditOutcome::Denied("no permission".to_string()),
                        details: HashMap::new(),
                        source_node: None,
                    })
                    .await;

                Ok(false)
            }
        }
    }

    /// Set routing policy
    pub async fn set_routing_policy(&self, policy: RoutingPolicy) {
        let mut current = self.routing_policy.write().await;
        *current = policy;
    }

    /// Grant cross-tenant permission
    pub async fn grant_cross_tenant_permission(&self, permission: CrossTenantPermission) {
        let mut permissions = self.cross_tenant_permissions.write().await;
        permissions.push(permission);
    }

    /// Get audit log
    pub fn audit_log(&self) -> Arc<AuditLog> {
        self.audit_log.clone()
    }

    /// Get statistics
    pub async fn stats(&self) -> TenantManagerStats {
        let tenants = self.tenants.read().await;
        let mut active_count = 0;
        for tenant in tenants.values() {
            if tenant.is_active().await {
                active_count += 1;
            }
        }

        TenantManagerStats {
            total_tenants: tenants.len(),
            active_tenants: active_count,
            total_namespaces: self.total_namespaces.load(Ordering::Relaxed),
            total_agents: self.total_agents.load(Ordering::Relaxed),
            audit_stats: self.audit_log.stats(),
        }
    }
}

/// Tenant manager statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantManagerStats {
    pub total_tenants: usize,
    pub active_tenants: usize,
    pub total_namespaces: usize,
    pub total_agents: usize,
    pub audit_stats: AuditLogStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_tenant_creation() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        let tenant = manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .expect("Failed to create tenant");

        assert_eq!(tenant.id, tenant_id);
        assert!(!tenant.is_active().await);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_tenant_activation() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .expect("Failed to create tenant");

        manager
            .activate_tenant(&tenant_id)
            .await
            .expect("Failed to activate tenant");

        let tenant = manager.get_tenant(&tenant_id).await.unwrap();
        assert!(tenant.is_active().await);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_duplicate_tenant() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .expect("Failed to create tenant");

        let result = manager
            .create_tenant(
                tenant_id.clone(),
                "Another".to_string(),
                ResourceQuota::default(),
            )
            .await;
        assert!(matches!(
            result,
            Err(MultiTenancyError::TenantAlreadyExists { .. })
        ));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_namespace_creation() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let namespace_id = NamespaceId::new("test-ns");
        let namespace = manager
            .create_namespace(&tenant_id, namespace_id.clone(), HashMap::new())
            .await
            .expect("Failed to create namespace");

        assert_eq!(namespace.config.id, namespace_id);
        assert_eq!(namespace.config.tenant_id, tenant_id);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_namespace_quota_exceeded() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        let quota = ResourceQuota {
            max_namespaces: 1,
            ..Default::default()
        };
        manager
            .create_tenant(tenant_id.clone(), "Test Tenant".to_string(), quota)
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        // First namespace should succeed
        manager
            .create_namespace(&tenant_id, NamespaceId::new("ns1"), HashMap::new())
            .await
            .unwrap();

        // Second should fail
        let result = manager
            .create_namespace(&tenant_id, NamespaceId::new("ns2"), HashMap::new())
            .await;
        assert!(matches!(
            result,
            Err(MultiTenancyError::QuotaExceeded { .. })
        ));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_agent_registration() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let namespace_id = NamespaceId::new("test-ns");
        manager
            .create_namespace(&tenant_id, namespace_id.clone(), HashMap::new())
            .await
            .unwrap();

        let agent_id: AgentId = [1u8; 16];
        manager
            .register_agent(&tenant_id, &namespace_id, agent_id)
            .await
            .expect("Failed to register agent");

        let namespace = manager.get_namespace(&namespace_id).await.unwrap();
        assert!(namespace.contains_agent(&agent_id).await);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_agent_quota_exceeded() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        let quota = ResourceQuota {
            max_agents: 1,
            ..Default::default()
        };
        manager
            .create_tenant(tenant_id.clone(), "Test Tenant".to_string(), quota)
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let namespace_id = NamespaceId::new("test-ns");
        manager
            .create_namespace(&tenant_id, namespace_id.clone(), HashMap::new())
            .await
            .unwrap();

        // First agent should succeed
        manager
            .register_agent(&tenant_id, &namespace_id, [2u8; 16])
            .await
            .unwrap();

        // Second should fail
        let result = manager
            .register_agent(&tenant_id, &namespace_id, [2u8; 16])
            .await;
        assert!(matches!(
            result,
            Err(MultiTenancyError::QuotaExceeded { .. })
        ));
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_strict_routing_policy() {
        let manager = TenantManager::new(AuditConfig::default());

        // Create two tenants
        let tenant1 = TenantId::new("tenant1");
        let tenant2 = TenantId::new("tenant2");
        manager
            .create_tenant(
                tenant1.clone(),
                "Tenant 1".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager
            .create_tenant(
                tenant2.clone(),
                "Tenant 2".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant1).await.unwrap();
        manager.activate_tenant(&tenant2).await.unwrap();

        let ns1 = NamespaceId::new("ns1");
        let ns2 = NamespaceId::new("ns2");
        manager
            .create_namespace(&tenant1, ns1.clone(), HashMap::new())
            .await
            .unwrap();
        manager
            .create_namespace(&tenant2, ns2.clone(), HashMap::new())
            .await
            .unwrap();

        // Same tenant and namespace - allowed
        assert!(manager
            .can_route(&tenant1, &ns1, &tenant1, &ns1)
            .await
            .unwrap());

        // Different tenant - denied
        assert!(!manager
            .can_route(&tenant1, &ns1, &tenant2, &ns2)
            .await
            .unwrap());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_permission_based_routing() {
        let manager = TenantManager::new(AuditConfig::default());
        manager
            .set_routing_policy(RoutingPolicy::PermissionBased)
            .await;

        let tenant1 = TenantId::new("tenant1");
        let tenant2 = TenantId::new("tenant2");
        manager
            .create_tenant(
                tenant1.clone(),
                "Tenant 1".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager
            .create_tenant(
                tenant2.clone(),
                "Tenant 2".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant1).await.unwrap();
        manager.activate_tenant(&tenant2).await.unwrap();

        let ns1 = NamespaceId::new("ns1");
        let ns2 = NamespaceId::new("ns2");
        manager
            .create_namespace(&tenant1, ns1.clone(), HashMap::new())
            .await
            .unwrap();
        manager
            .create_namespace(&tenant2, ns2.clone(), HashMap::new())
            .await
            .unwrap();

        // Initially denied
        assert!(!manager
            .can_route(&tenant1, &ns1, &tenant2, &ns2)
            .await
            .unwrap());

        // Grant permission
        manager
            .grant_cross_tenant_permission(CrossTenantPermission {
                source_tenant: tenant1.clone(),
                target_tenant: tenant2.clone(),
                allowed_namespaces: Vec::new(), // All namespaces
                expires_at: None,
            })
            .await;

        // Now allowed
        assert!(manager
            .can_route(&tenant1, &ns1, &tenant2, &ns2)
            .await
            .unwrap());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_audit_log() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let entries = manager.audit_log().get_entries_for_tenant(&tenant_id).await;
        assert!(!entries.is_empty());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_namespace_deletion() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let namespace_id = NamespaceId::new("test-ns");
        manager
            .create_namespace(&tenant_id, namespace_id.clone(), HashMap::new())
            .await
            .unwrap();

        manager
            .delete_namespace(&tenant_id, &namespace_id)
            .await
            .unwrap();

        assert!(manager.get_namespace(&namespace_id).await.is_none());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_resource_quota_presets() {
        let small = ResourceQuota::small();
        assert_eq!(small.max_agents, 100);
        assert_eq!(small.max_namespaces, 3);

        let enterprise = ResourceQuota::enterprise();
        assert_eq!(enterprise.max_agents, 10000);
        assert_eq!(enterprise.max_namespaces, 100);

        let unlimited = ResourceQuota::unlimited();
        assert_eq!(unlimited.max_agents, usize::MAX);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_tenant_manager_stats() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let namespace_id = NamespaceId::new("test-ns");
        manager
            .create_namespace(&tenant_id, namespace_id.clone(), HashMap::new())
            .await
            .unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.total_tenants, 1);
        assert_eq!(stats.active_tenants, 1);
        assert_eq!(stats.total_namespaces, 1);
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_namespace_only_routing() {
        let manager = TenantManager::new(AuditConfig::default());
        manager
            .set_routing_policy(RoutingPolicy::NamespaceOnly)
            .await;

        let tenant_id = TenantId::new("tenant1");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Tenant 1".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let ns1 = NamespaceId::new("ns1");
        let ns2 = NamespaceId::new("ns2");
        manager
            .create_namespace(&tenant_id, ns1.clone(), HashMap::new())
            .await
            .unwrap();
        manager
            .create_namespace(&tenant_id, ns2.clone(), HashMap::new())
            .await
            .unwrap();

        // Same namespace - allowed
        assert!(manager
            .can_route(&tenant_id, &ns1, &tenant_id, &ns1)
            .await
            .unwrap());

        // Different namespace - denied
        assert!(!manager
            .can_route(&tenant_id, &ns1, &tenant_id, &ns2)
            .await
            .unwrap());
    }

    #[cfg_attr(miri, ignore)]
    #[tokio::test]
    async fn test_agent_unregistration() {
        let manager = TenantManager::new(AuditConfig::default());
        let tenant_id = TenantId::new("test-tenant");
        manager
            .create_tenant(
                tenant_id.clone(),
                "Test Tenant".to_string(),
                ResourceQuota::default(),
            )
            .await
            .unwrap();
        manager.activate_tenant(&tenant_id).await.unwrap();

        let namespace_id = NamespaceId::new("test-ns");
        manager
            .create_namespace(&tenant_id, namespace_id.clone(), HashMap::new())
            .await
            .unwrap();

        let agent_id: AgentId = [1u8; 16];
        manager
            .register_agent(&tenant_id, &namespace_id, agent_id)
            .await
            .unwrap();

        manager
            .unregister_agent(&tenant_id, &namespace_id, &agent_id)
            .await
            .unwrap();

        let namespace = manager.get_namespace(&namespace_id).await.unwrap();
        assert!(!namespace.contains_agent(&agent_id).await);
    }
}
