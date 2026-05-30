//! Access control lists for MielinMesh security.

use super::*;

// =============================================================================
// Access Control Lists (ACLs)
// =============================================================================

/// Permission types for mesh operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read data from agents
    Read,
    /// Write data to agents
    Write,
    /// Execute agent code
    Execute,
    /// Migrate agents between nodes
    Migrate,
    /// Create new agents
    Create,
    /// Delete agents
    Delete,
    /// Administer node settings
    Admin,
    /// Access gossip protocol
    Gossip,
    /// Access DHT operations
    DhtAccess,
    /// Join the mesh network
    Join,
}

impl Permission {
    /// Get all permissions
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Execute,
            Permission::Migrate,
            Permission::Create,
            Permission::Delete,
            Permission::Admin,
            Permission::Gossip,
            Permission::DhtAccess,
            Permission::Join,
        ]
    }

    /// Get standard user permissions
    pub fn user_defaults() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Execute,
            Permission::Create,
        ]
    }

    /// Get node permissions
    pub fn node_defaults() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Execute,
            Permission::Migrate,
            Permission::Gossip,
            Permission::DhtAccess,
            Permission::Join,
        ]
    }
}

/// ACL rule effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AclEffect {
    /// Allow the action
    Allow,
    /// Deny the action
    #[default]
    Deny,
}

/// Subject of an ACL rule
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AclSubject {
    /// Specific node
    Node(NodeId),
    /// Specific agent
    Agent(String),
    /// Group of nodes/agents
    Group(String),
    /// Any subject (wildcard)
    Any,
}

impl AclSubject {
    /// Check if subject matches this pattern
    pub fn matches(&self, other: &AclSubject) -> bool {
        match (self, other) {
            (AclSubject::Any, _) => true,
            (AclSubject::Node(a), AclSubject::Node(b)) => a == b,
            (AclSubject::Agent(a), AclSubject::Agent(b)) => a == b,
            (AclSubject::Group(a), AclSubject::Group(b)) => a == b,
            _ => false,
        }
    }
}

/// Resource targeted by an ACL rule
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AclResource {
    /// Specific agent
    Agent(String),
    /// Specific node
    Node(NodeId),
    /// All agents on a node
    NodeAgents(NodeId),
    /// Resource path pattern
    Path(String),
    /// Any resource (wildcard)
    Any,
}

impl AclResource {
    /// Check if resource matches this pattern
    pub fn matches(&self, other: &AclResource) -> bool {
        match (self, other) {
            (AclResource::Any, _) => true,
            (AclResource::Agent(a), AclResource::Agent(b)) => a == b,
            (AclResource::Node(a), AclResource::Node(b)) => a == b,
            (AclResource::NodeAgents(a), AclResource::Agent(_)) => {
                // Would need to check if agent is on node
                // For now, just compare nodes
                matches!(other, AclResource::NodeAgents(b) if a == b)
            }
            (AclResource::Path(pattern), AclResource::Path(path)) => {
                // Simple glob matching
                if pattern.ends_with('*') {
                    path.starts_with(&pattern[..pattern.len() - 1])
                } else {
                    pattern == path
                }
            }
            _ => false,
        }
    }
}

/// Single ACL rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRule {
    /// Rule identifier
    pub id: String,
    /// Subject (who)
    pub subject: AclSubject,
    /// Resource (what)
    pub resource: AclResource,
    /// Permissions
    pub permissions: HashSet<Permission>,
    /// Effect (allow/deny)
    pub effect: AclEffect,
    /// Rule priority (higher = evaluated first)
    pub priority: i32,
    /// Rule description
    pub description: Option<String>,
    /// Expiration time
    pub expires_at: Option<SystemTime>,
}

impl AclRule {
    /// Create new ACL rule
    pub fn new(
        id: impl Into<String>,
        subject: AclSubject,
        resource: AclResource,
        permissions: impl IntoIterator<Item = Permission>,
        effect: AclEffect,
    ) -> Self {
        Self {
            id: id.into(),
            subject,
            resource,
            permissions: permissions.into_iter().collect(),
            effect,
            priority: 0,
            description: None,
            expires_at: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set expiration
    pub fn with_expiration(mut self, expires_at: SystemTime) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if rule is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| SystemTime::now() > exp)
            .unwrap_or(false)
    }

    /// Check if rule matches the request
    pub fn matches(
        &self,
        subject: &AclSubject,
        resource: &AclResource,
        permission: Permission,
    ) -> bool {
        !self.is_expired()
            && self.subject.matches(subject)
            && self.resource.matches(resource)
            && self.permissions.contains(&permission)
    }
}

/// ACL policy managing multiple rules
pub struct AclPolicy {
    /// Rules sorted by priority
    rules: Arc<RwLock<Vec<AclRule>>>,
    /// Default effect when no rule matches
    default_effect: AclEffect,
    /// Node group memberships
    node_groups: Arc<RwLock<HashMap<NodeId, HashSet<String>>>>,
    /// Agent group memberships
    agent_groups: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl AclPolicy {
    /// Create new ACL policy
    pub fn new(default_effect: AclEffect) -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            default_effect,
            node_groups: Arc::new(RwLock::new(HashMap::new())),
            agent_groups: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create permissive policy (allow by default)
    pub fn permissive() -> Self {
        Self::new(AclEffect::Allow)
    }

    /// Create restrictive policy (deny by default)
    pub fn restrictive() -> Self {
        Self::new(AclEffect::Deny)
    }

    /// Add a rule
    pub async fn add_rule(&self, rule: AclRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
        // Sort by priority (highest first)
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Remove a rule by ID
    pub async fn remove_rule(&self, rule_id: &str) -> Option<AclRule> {
        let mut rules = self.rules.write().await;
        let pos = rules.iter().position(|r| r.id == rule_id)?;
        Some(rules.remove(pos))
    }

    /// Get a rule by ID
    pub async fn get_rule(&self, rule_id: &str) -> Option<AclRule> {
        let rules = self.rules.read().await;
        rules.iter().find(|r| r.id == rule_id).cloned()
    }

    /// Add node to a group
    pub async fn add_node_to_group(&self, node_id: NodeId, group: impl Into<String>) {
        let mut groups = self.node_groups.write().await;
        groups.entry(node_id).or_default().insert(group.into());
    }

    /// Add agent to a group
    pub async fn add_agent_to_group(&self, agent_id: impl Into<String>, group: impl Into<String>) {
        let mut groups = self.agent_groups.write().await;
        groups
            .entry(agent_id.into())
            .or_default()
            .insert(group.into());
    }

    /// Check permission for a node
    pub async fn check_node_permission(
        &self,
        node_id: &NodeId,
        resource: &AclResource,
        permission: Permission,
    ) -> SecurityResult<()> {
        let subject = AclSubject::Node(*node_id);
        self.check_permission(&subject, resource, permission).await
    }

    /// Check permission for an agent
    pub async fn check_agent_permission(
        &self,
        agent_id: &str,
        resource: &AclResource,
        permission: Permission,
    ) -> SecurityResult<()> {
        let subject = AclSubject::Agent(agent_id.to_string());
        self.check_permission(&subject, resource, permission).await
    }

    /// Check permission with subject
    pub async fn check_permission(
        &self,
        subject: &AclSubject,
        resource: &AclResource,
        permission: Permission,
    ) -> SecurityResult<()> {
        let rules = self.rules.read().await;

        // Find first matching rule (rules are sorted by priority)
        for rule in rules.iter() {
            if rule.matches(subject, resource, permission) {
                return match rule.effect {
                    AclEffect::Allow => Ok(()),
                    AclEffect::Deny => Err(SecurityError::AccessDenied {
                        subject: format!("{:?}", subject),
                        operation: format!("{:?} on {:?}", permission, resource),
                    }),
                };
            }
        }

        // Also check group memberships
        let group_effect = self
            .check_group_permissions(subject, resource, permission)
            .await;
        if let Some(effect) = group_effect {
            return match effect {
                AclEffect::Allow => Ok(()),
                AclEffect::Deny => Err(SecurityError::AccessDenied {
                    subject: format!("{:?}", subject),
                    operation: format!("{:?} on {:?}", permission, resource),
                }),
            };
        }

        // Apply default effect
        match self.default_effect {
            AclEffect::Allow => Ok(()),
            AclEffect::Deny => Err(SecurityError::AccessDenied {
                subject: format!("{:?}", subject),
                operation: format!("{:?} on {:?}", permission, resource),
            }),
        }
    }

    /// Check group permissions
    async fn check_group_permissions(
        &self,
        subject: &AclSubject,
        resource: &AclResource,
        permission: Permission,
    ) -> Option<AclEffect> {
        let rules = self.rules.read().await;

        // Get groups for the subject
        let groups: Vec<String> = match subject {
            AclSubject::Node(node_id) => {
                let node_groups = self.node_groups.read().await;
                node_groups
                    .get(node_id)
                    .map(|g| g.iter().cloned().collect())
                    .unwrap_or_default()
            }
            AclSubject::Agent(agent_id) => {
                let agent_groups = self.agent_groups.read().await;
                agent_groups
                    .get(agent_id)
                    .map(|g| g.iter().cloned().collect())
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };

        // Check rules for each group
        for group in groups {
            let group_subject = AclSubject::Group(group);
            for rule in rules.iter() {
                if rule.matches(&group_subject, resource, permission) {
                    return Some(rule.effect);
                }
            }
        }

        None
    }

    /// Get all rules
    pub async fn all_rules(&self) -> Vec<AclRule> {
        let rules = self.rules.read().await;
        rules.clone()
    }

    /// Clean up expired rules
    pub async fn cleanup_expired(&self) -> usize {
        let mut rules = self.rules.write().await;
        let before = rules.len();
        rules.retain(|r| !r.is_expired());
        before - rules.len()
    }
}

impl Default for AclPolicy {
    fn default() -> Self {
        Self::restrictive()
    }
}
