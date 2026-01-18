//! Agent Groups
//!
//! Provides grouping and coordination for multiple agents.
//! Groups can have shared policies, coordinated state transitions,
//! and hierarchical organization.

use crate::{Agent, AgentId, AgentState, Policy, TransitionResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Unique identifier for agent groups
pub type GroupId = Uuid;

/// Group membership role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupRole {
    /// Full member with all privileges
    Member,
    /// Leader with coordination privileges
    Leader,
    /// Observer with read-only access
    Observer,
}

/// Group membership entry
#[derive(Debug, Clone)]
pub struct GroupMember {
    /// Agent ID
    pub agent_id: AgentId,
    /// Role in the group
    pub role: GroupRole,
    /// When the agent joined
    pub joined_at: u64,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

impl GroupMember {
    pub fn new(agent_id: AgentId, role: GroupRole) -> Self {
        Self {
            agent_id,
            role,
            joined_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Agent group configuration
#[derive(Debug, Clone)]
pub struct GroupConfig {
    /// Maximum number of members
    pub max_members: usize,
    /// Minimum number of leaders
    pub min_leaders: usize,
    /// Maximum number of leaders
    pub max_leaders: usize,
    /// Whether to enforce shared policy
    pub enforce_shared_policy: bool,
    /// Whether members can leave voluntarily
    pub allow_voluntary_leave: bool,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            max_members: 1000,
            min_leaders: 1,
            max_leaders: 3,
            enforce_shared_policy: false,
            allow_voluntary_leave: true,
        }
    }
}

/// Group state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupState {
    /// Group is active and accepting members
    Active,
    /// Group is paused (no state changes)
    Paused,
    /// Group is being dissolved
    Dissolving,
    /// Group has been dissolved
    Dissolved,
}

/// Result of group operations
#[derive(Debug)]
pub enum GroupResult<T> {
    /// Operation succeeded
    Success(T),
    /// Operation failed
    Error(GroupError),
}

impl<T> GroupResult<T> {
    pub fn is_success(&self) -> bool {
        matches!(self, GroupResult::Success(_))
    }

    pub fn unwrap(self) -> T {
        match self {
            GroupResult::Success(v) => v,
            GroupResult::Error(e) => panic!("called unwrap on GroupResult::Error: {:?}", e),
        }
    }
}

/// Group error types
#[derive(Debug, Clone)]
pub enum GroupError {
    /// Group is full
    GroupFull,
    /// Agent is already a member
    AlreadyMember,
    /// Agent is not a member
    NotMember,
    /// Agent not found
    AgentNotFound,
    /// Invalid role change
    InvalidRoleChange,
    /// Group is not active
    GroupNotActive,
    /// Would violate min leaders constraint
    MinLeadersViolation,
    /// Would violate max leaders constraint
    MaxLeadersViolation,
    /// Operation not allowed
    NotAllowed(String),
}

/// An agent group for coordinated management
pub struct AgentGroup {
    id: GroupId,
    name: String,
    members: RwLock<HashMap<AgentId, GroupMember>>,
    config: GroupConfig,
    state: RwLock<GroupState>,
    shared_policy: RwLock<Option<Policy>>,
    /// Parent group (for hierarchical groups)
    parent: Option<GroupId>,
    /// Child groups
    children: RwLock<HashSet<GroupId>>,
    /// Tags for categorization
    tags: RwLock<HashSet<String>>,
}

impl AgentGroup {
    /// Create a new agent group
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            members: RwLock::new(HashMap::new()),
            config: GroupConfig::default(),
            state: RwLock::new(GroupState::Active),
            shared_policy: RwLock::new(None),
            parent: None,
            children: RwLock::new(HashSet::new()),
            tags: RwLock::new(HashSet::new()),
        }
    }

    /// Create a new group with custom config
    pub fn with_config(name: impl Into<String>, config: GroupConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            members: RwLock::new(HashMap::new()),
            config,
            state: RwLock::new(GroupState::Active),
            shared_policy: RwLock::new(None),
            parent: None,
            children: RwLock::new(HashSet::new()),
            tags: RwLock::new(HashSet::new()),
        }
    }

    /// Get group ID
    pub fn id(&self) -> GroupId {
        self.id
    }

    /// Get group name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get group state
    pub fn state(&self) -> GroupState {
        *self.state.read().expect("Lock poisoned: state")
    }

    /// Get group config
    pub fn config(&self) -> &GroupConfig {
        &self.config
    }

    /// Get member count
    pub fn member_count(&self) -> usize {
        self.members.read().expect("Lock poisoned: members").len()
    }

    /// Check if agent is a member
    pub fn is_member(&self, agent_id: &AgentId) -> bool {
        self.members
            .read()
            .expect("Lock poisoned: members")
            .contains_key(agent_id)
    }

    /// Get member info
    pub fn get_member(&self, agent_id: &AgentId) -> Option<GroupMember> {
        self.members
            .read()
            .expect("Lock poisoned: members")
            .get(agent_id)
            .cloned()
    }

    /// Get all members
    pub fn members(&self) -> Vec<GroupMember> {
        self.members
            .read()
            .expect("Lock poisoned: members")
            .values()
            .cloned()
            .collect()
    }

    /// Get members by role
    pub fn members_by_role(&self, role: GroupRole) -> Vec<GroupMember> {
        self.members
            .read()
            .expect("Lock poisoned: members")
            .values()
            .filter(|m| m.role == role)
            .cloned()
            .collect()
    }

    /// Get leader count
    pub fn leader_count(&self) -> usize {
        self.members
            .read()
            .expect("Lock poisoned: members")
            .values()
            .filter(|m| m.role == GroupRole::Leader)
            .count()
    }

    /// Add an agent to the group
    pub fn add_member(&self, agent_id: AgentId, role: GroupRole) -> GroupResult<()> {
        if *self.state.read().expect("Lock poisoned: state") != GroupState::Active {
            return GroupResult::Error(GroupError::GroupNotActive);
        }

        let mut members = self.members.write().expect("Lock poisoned: members");

        if members.contains_key(&agent_id) {
            return GroupResult::Error(GroupError::AlreadyMember);
        }

        if members.len() >= self.config.max_members {
            return GroupResult::Error(GroupError::GroupFull);
        }

        // Check leader constraint
        if role == GroupRole::Leader {
            let leader_count = members
                .values()
                .filter(|m| m.role == GroupRole::Leader)
                .count();
            if leader_count >= self.config.max_leaders {
                return GroupResult::Error(GroupError::MaxLeadersViolation);
            }
        }

        members.insert(agent_id, GroupMember::new(agent_id, role));
        GroupResult::Success(())
    }

    /// Remove an agent from the group
    pub fn remove_member(&self, agent_id: &AgentId) -> GroupResult<GroupMember> {
        let state = *self.state.read().expect("Lock poisoned: state");
        if state == GroupState::Dissolved {
            return GroupResult::Error(GroupError::GroupNotActive);
        }

        let mut members = self.members.write().expect("Lock poisoned: members");

        if let Some(member) = members.get(agent_id) {
            // Check min leaders constraint
            if member.role == GroupRole::Leader {
                let leader_count = members
                    .values()
                    .filter(|m| m.role == GroupRole::Leader)
                    .count();
                if leader_count <= self.config.min_leaders && state == GroupState::Active {
                    return GroupResult::Error(GroupError::MinLeadersViolation);
                }
            }

            let removed = members.remove(agent_id).expect("Member exists");
            GroupResult::Success(removed)
        } else {
            GroupResult::Error(GroupError::NotMember)
        }
    }

    /// Change member role
    pub fn change_role(&self, agent_id: &AgentId, new_role: GroupRole) -> GroupResult<()> {
        if *self.state.read().expect("Lock poisoned: state") != GroupState::Active {
            return GroupResult::Error(GroupError::GroupNotActive);
        }

        let mut members = self.members.write().expect("Lock poisoned: members");

        // First, check if member exists and get old role
        let old_role = match members.get(agent_id) {
            Some(member) => member.role,
            None => return GroupResult::Error(GroupError::NotMember),
        };

        // Count current leaders (before any change)
        let leader_count = members
            .values()
            .filter(|m| m.role == GroupRole::Leader)
            .count();

        // Check constraints
        if old_role == GroupRole::Leader
            && new_role != GroupRole::Leader
            && leader_count <= self.config.min_leaders
        {
            return GroupResult::Error(GroupError::MinLeadersViolation);
        }

        if new_role == GroupRole::Leader
            && old_role != GroupRole::Leader
            && leader_count >= self.config.max_leaders
        {
            return GroupResult::Error(GroupError::MaxLeadersViolation);
        }

        // Now update the role
        if let Some(member) = members.get_mut(agent_id) {
            member.role = new_role;
        }

        GroupResult::Success(())
    }

    /// Set shared policy for the group
    pub fn set_shared_policy(&self, policy: Policy) {
        *self
            .shared_policy
            .write()
            .expect("Lock poisoned: shared_policy") = Some(policy);
    }

    /// Get shared policy
    pub fn shared_policy(&self) -> Option<Policy> {
        self.shared_policy
            .read()
            .expect("Lock poisoned: shared_policy")
            .clone()
    }

    /// Clear shared policy
    pub fn clear_shared_policy(&self) {
        *self
            .shared_policy
            .write()
            .expect("Lock poisoned: shared_policy") = None;
    }

    /// Pause the group
    pub fn pause(&self) -> GroupResult<()> {
        let mut state = self.state.write().expect("Lock poisoned: state");
        if *state != GroupState::Active {
            return GroupResult::Error(GroupError::NotAllowed("Group is not active".to_string()));
        }
        *state = GroupState::Paused;
        GroupResult::Success(())
    }

    /// Resume the group
    pub fn resume(&self) -> GroupResult<()> {
        let mut state = self.state.write().expect("Lock poisoned: state");
        if *state != GroupState::Paused {
            return GroupResult::Error(GroupError::NotAllowed("Group is not paused".to_string()));
        }
        *state = GroupState::Active;
        GroupResult::Success(())
    }

    /// Begin dissolving the group
    pub fn dissolve(&self) -> GroupResult<()> {
        let mut state = self.state.write().expect("Lock poisoned: state");
        if *state == GroupState::Dissolved {
            return GroupResult::Error(GroupError::NotAllowed("Already dissolved".to_string()));
        }
        *state = GroupState::Dissolving;
        GroupResult::Success(())
    }

    /// Complete dissolution
    pub fn complete_dissolution(&self) -> GroupResult<()> {
        let mut state = self.state.write().expect("Lock poisoned: state");
        if *state != GroupState::Dissolving {
            return GroupResult::Error(GroupError::NotAllowed("Not dissolving".to_string()));
        }

        // Clear all members
        self.members
            .write()
            .expect("Lock poisoned: members")
            .clear();
        self.children
            .write()
            .expect("Lock poisoned: children")
            .clear();
        *state = GroupState::Dissolved;

        GroupResult::Success(())
    }

    /// Set parent group
    pub fn set_parent(&mut self, parent_id: GroupId) {
        self.parent = Some(parent_id);
    }

    /// Get parent group ID
    pub fn parent(&self) -> Option<GroupId> {
        self.parent
    }

    /// Add a child group
    pub fn add_child(&self, child_id: GroupId) {
        self.children
            .write()
            .expect("Lock poisoned: children")
            .insert(child_id);
    }

    /// Remove a child group
    pub fn remove_child(&self, child_id: &GroupId) {
        self.children
            .write()
            .expect("Lock poisoned: children")
            .remove(child_id);
    }

    /// Get child group IDs
    pub fn children(&self) -> Vec<GroupId> {
        self.children
            .read()
            .expect("Lock poisoned: children")
            .iter()
            .copied()
            .collect()
    }

    /// Add a tag
    pub fn add_tag(&self, tag: impl Into<String>) {
        self.tags
            .write()
            .expect("Lock poisoned: tags")
            .insert(tag.into());
    }

    /// Remove a tag
    pub fn remove_tag(&self, tag: &str) {
        self.tags.write().expect("Lock poisoned: tags").remove(tag);
    }

    /// Check if group has tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.read().expect("Lock poisoned: tags").contains(tag)
    }

    /// Get all tags
    pub fn tags(&self) -> Vec<String> {
        self.tags
            .read()
            .expect("Lock poisoned: tags")
            .iter()
            .cloned()
            .collect()
    }
}

/// Group registry for managing multiple groups
pub struct GroupRegistry {
    groups: RwLock<HashMap<GroupId, Arc<AgentGroup>>>,
    /// Index: agent -> groups they belong to
    agent_groups: RwLock<HashMap<AgentId, HashSet<GroupId>>>,
    /// Index: tag -> groups with that tag
    tag_index: RwLock<HashMap<String, HashSet<GroupId>>>,
}

impl GroupRegistry {
    /// Create a new group registry
    pub fn new() -> Self {
        Self {
            groups: RwLock::new(HashMap::new()),
            agent_groups: RwLock::new(HashMap::new()),
            tag_index: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new group
    pub fn register(&self, group: AgentGroup) -> Arc<AgentGroup> {
        let id = group.id();
        let tags: Vec<String> = group.tags();
        let group = Arc::new(group);

        self.groups
            .write()
            .expect("Lock poisoned: groups")
            .insert(id, Arc::clone(&group));

        // Index tags
        let mut tag_idx = self.tag_index.write().expect("Lock poisoned: tag_index");
        for tag in tags {
            tag_idx.entry(tag).or_default().insert(id);
        }

        group
    }

    /// Get a group by ID
    pub fn get(&self, id: &GroupId) -> Option<Arc<AgentGroup>> {
        self.groups
            .read()
            .expect("Lock poisoned: groups")
            .get(id)
            .cloned()
    }

    /// Unregister a group
    pub fn unregister(&self, id: &GroupId) -> Option<Arc<AgentGroup>> {
        let group = self
            .groups
            .write()
            .expect("Lock poisoned: groups")
            .remove(id);

        if let Some(ref g) = group {
            // Remove from agent index
            let mut agent_groups = self
                .agent_groups
                .write()
                .expect("Lock poisoned: agent_groups");
            for member in g.members() {
                if let Some(groups) = agent_groups.get_mut(&member.agent_id) {
                    groups.remove(id);
                }
            }

            // Remove from tag index
            let mut tag_idx = self.tag_index.write().expect("Lock poisoned: tag_index");
            for tag in g.tags() {
                if let Some(groups) = tag_idx.get_mut(&tag) {
                    groups.remove(id);
                }
            }
        }

        group
    }

    /// Get all groups an agent belongs to
    pub fn groups_for_agent(&self, agent_id: &AgentId) -> Vec<Arc<AgentGroup>> {
        let agent_groups = self
            .agent_groups
            .read()
            .expect("Lock poisoned: agent_groups");
        let groups = self.groups.read().expect("Lock poisoned: groups");

        agent_groups
            .get(agent_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| groups.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all groups with a tag
    pub fn groups_by_tag(&self, tag: &str) -> Vec<Arc<AgentGroup>> {
        let tag_idx = self.tag_index.read().expect("Lock poisoned: tag_index");
        let groups = self.groups.read().expect("Lock poisoned: groups");

        tag_idx
            .get(tag)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| groups.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Record that an agent joined a group
    pub fn record_join(&self, agent_id: AgentId, group_id: GroupId) {
        self.agent_groups
            .write()
            .expect("Lock poisoned: agent_groups")
            .entry(agent_id)
            .or_default()
            .insert(group_id);
    }

    /// Record that an agent left a group
    pub fn record_leave(&self, agent_id: &AgentId, group_id: &GroupId) {
        if let Some(groups) = self
            .agent_groups
            .write()
            .expect("Lock poisoned: agent_groups")
            .get_mut(agent_id)
        {
            groups.remove(group_id);
        }
    }

    /// Get number of registered groups
    pub fn count(&self) -> usize {
        self.groups.read().expect("Lock poisoned: groups").len()
    }

    /// Get all group IDs
    pub fn all_ids(&self) -> Vec<GroupId> {
        self.groups
            .read()
            .expect("Lock poisoned: groups")
            .keys()
            .copied()
            .collect()
    }
}

impl Default for GroupRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Coordinated state transition for a group
pub struct GroupCoordinator {
    group: Arc<AgentGroup>,
}

impl GroupCoordinator {
    pub fn new(group: Arc<AgentGroup>) -> Self {
        Self { group }
    }

    /// Apply a state transition to all agents in the group
    pub fn transition_all<F>(&self, mut transition_fn: F) -> Vec<(AgentId, TransitionResult)>
    where
        F: FnMut(&mut Agent) -> TransitionResult,
    {
        let members = self.group.members();
        let mut results = Vec::with_capacity(members.len());

        // Note: In a real implementation, you would get the actual Agent instances
        // from an agent registry. Here we just collect the IDs.
        for member in members {
            // This is a placeholder - actual implementation would use agent registry
            let mut placeholder_agent = Agent::new(vec![]);
            let result = transition_fn(&mut placeholder_agent);
            results.push((member.agent_id, result));
        }

        results
    }

    /// Check if all agents in the group are in a specific state
    pub fn all_in_state<F>(&self, check_fn: F) -> bool
    where
        F: Fn(&AgentState) -> bool,
    {
        // Placeholder - actual implementation would query agent registry
        let _ = check_fn;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_creation() {
        let group = AgentGroup::new("test-group");
        assert_eq!(group.name(), "test-group");
        assert_eq!(group.state(), GroupState::Active);
        assert_eq!(group.member_count(), 0);
    }

    #[test]
    fn test_add_member() {
        let group = AgentGroup::new("test");
        let agent_id = Uuid::new_v4();

        let result = group.add_member(agent_id, GroupRole::Member);
        assert!(result.is_success());
        assert!(group.is_member(&agent_id));
        assert_eq!(group.member_count(), 1);
    }

    #[test]
    fn test_add_duplicate_member() {
        let group = AgentGroup::new("test");
        let agent_id = Uuid::new_v4();

        group.add_member(agent_id, GroupRole::Member);
        let result = group.add_member(agent_id, GroupRole::Member);

        assert!(matches!(
            result,
            GroupResult::Error(GroupError::AlreadyMember)
        ));
    }

    #[test]
    fn test_remove_member() {
        let group = AgentGroup::new("test");
        let agent_id = Uuid::new_v4();

        group.add_member(agent_id, GroupRole::Member);
        let result = group.remove_member(&agent_id);

        assert!(result.is_success());
        assert!(!group.is_member(&agent_id));
    }

    #[test]
    fn test_max_members_limit() {
        let config = GroupConfig {
            max_members: 2,
            ..Default::default()
        };
        let group = AgentGroup::with_config("test", config);

        group.add_member(Uuid::new_v4(), GroupRole::Member);
        group.add_member(Uuid::new_v4(), GroupRole::Member);

        let result = group.add_member(Uuid::new_v4(), GroupRole::Member);
        assert!(matches!(result, GroupResult::Error(GroupError::GroupFull)));
    }

    #[test]
    fn test_leader_limits() {
        let config = GroupConfig {
            max_leaders: 2,
            min_leaders: 1,
            ..Default::default()
        };
        let group = AgentGroup::with_config("test", config);

        // Add two leaders
        let leader1 = Uuid::new_v4();
        let leader2 = Uuid::new_v4();
        group.add_member(leader1, GroupRole::Leader);
        group.add_member(leader2, GroupRole::Leader);

        // Third leader should fail
        let result = group.add_member(Uuid::new_v4(), GroupRole::Leader);
        assert!(matches!(
            result,
            GroupResult::Error(GroupError::MaxLeadersViolation)
        ));
    }

    #[test]
    fn test_min_leaders_constraint() {
        let config = GroupConfig {
            min_leaders: 1,
            ..Default::default()
        };
        let group = AgentGroup::with_config("test", config);

        let leader_id = Uuid::new_v4();
        group.add_member(leader_id, GroupRole::Leader);

        // Can't remove the only leader
        let result = group.remove_member(&leader_id);
        assert!(matches!(
            result,
            GroupResult::Error(GroupError::MinLeadersViolation)
        ));
    }

    #[test]
    fn test_change_role() {
        let group = AgentGroup::new("test");
        let agent_id = Uuid::new_v4();

        group.add_member(agent_id, GroupRole::Member);
        group.change_role(&agent_id, GroupRole::Leader);

        let member = group.get_member(&agent_id).unwrap();
        assert_eq!(member.role, GroupRole::Leader);
    }

    #[test]
    fn test_members_by_role() {
        let group = AgentGroup::new("test");

        let member1 = Uuid::new_v4();
        let member2 = Uuid::new_v4();
        let leader = Uuid::new_v4();

        group.add_member(member1, GroupRole::Member);
        group.add_member(member2, GroupRole::Member);
        group.add_member(leader, GroupRole::Leader);

        let members = group.members_by_role(GroupRole::Member);
        assert_eq!(members.len(), 2);

        let leaders = group.members_by_role(GroupRole::Leader);
        assert_eq!(leaders.len(), 1);
    }

    #[test]
    fn test_group_pause_resume() {
        let group = AgentGroup::new("test");

        assert!(group.pause().is_success());
        assert_eq!(group.state(), GroupState::Paused);

        // Can't add members while paused
        let result = group.add_member(Uuid::new_v4(), GroupRole::Member);
        assert!(matches!(
            result,
            GroupResult::Error(GroupError::GroupNotActive)
        ));

        assert!(group.resume().is_success());
        assert_eq!(group.state(), GroupState::Active);
    }

    #[test]
    fn test_group_dissolution() {
        let group = AgentGroup::new("test");
        group.add_member(Uuid::new_v4(), GroupRole::Leader);
        group.add_member(Uuid::new_v4(), GroupRole::Member);

        assert!(group.dissolve().is_success());
        assert_eq!(group.state(), GroupState::Dissolving);

        assert!(group.complete_dissolution().is_success());
        assert_eq!(group.state(), GroupState::Dissolved);
        assert_eq!(group.member_count(), 0);
    }

    #[test]
    fn test_shared_policy() {
        let group = AgentGroup::new("test");

        assert!(group.shared_policy().is_none());

        let policy = Policy::default();
        group.set_shared_policy(policy.clone());

        assert!(group.shared_policy().is_some());

        group.clear_shared_policy();
        assert!(group.shared_policy().is_none());
    }

    #[test]
    fn test_group_hierarchy() {
        let parent = AgentGroup::new("parent");
        let child = AgentGroup::new("child");

        let child_id = child.id();
        parent.add_child(child_id);

        assert!(parent.children().contains(&child_id));
    }

    #[test]
    fn test_group_tags() {
        let group = AgentGroup::new("test");

        group.add_tag("production");
        group.add_tag("high-priority");

        assert!(group.has_tag("production"));
        assert!(group.has_tag("high-priority"));
        assert!(!group.has_tag("dev"));

        group.remove_tag("production");
        assert!(!group.has_tag("production"));
    }

    #[test]
    fn test_group_registry() {
        let registry = GroupRegistry::new();

        let group1 = AgentGroup::new("group1");
        let group2 = AgentGroup::new("group2");

        let id1 = group1.id();
        let id2 = group2.id();

        registry.register(group1);
        registry.register(group2);

        assert_eq!(registry.count(), 2);
        assert!(registry.get(&id1).is_some());
        assert!(registry.get(&id2).is_some());

        registry.unregister(&id1);
        assert_eq!(registry.count(), 1);
        assert!(registry.get(&id1).is_none());
    }

    #[test]
    fn test_registry_agent_index() {
        let registry = GroupRegistry::new();
        let agent_id = Uuid::new_v4();

        let group1 = AgentGroup::new("group1");
        let group2 = AgentGroup::new("group2");

        let id1 = group1.id();
        let id2 = group2.id();

        let g1 = registry.register(group1);
        let g2 = registry.register(group2);

        g1.add_member(agent_id, GroupRole::Member);
        g2.add_member(agent_id, GroupRole::Member);

        registry.record_join(agent_id, id1);
        registry.record_join(agent_id, id2);

        let agent_groups = registry.groups_for_agent(&agent_id);
        assert_eq!(agent_groups.len(), 2);
    }

    #[test]
    fn test_registry_tag_index() {
        let registry = GroupRegistry::new();

        let group1 = AgentGroup::new("group1");
        let group2 = AgentGroup::new("group2");
        let group3 = AgentGroup::new("group3");

        group1.add_tag("prod");
        group2.add_tag("prod");
        group3.add_tag("dev");

        registry.register(group1);
        registry.register(group2);
        registry.register(group3);

        let prod_groups = registry.groups_by_tag("prod");
        assert_eq!(prod_groups.len(), 2);

        let dev_groups = registry.groups_by_tag("dev");
        assert_eq!(dev_groups.len(), 1);
    }

    #[test]
    fn test_group_member_metadata() {
        let agent_id = Uuid::new_v4();
        let member = GroupMember::new(agent_id, GroupRole::Member)
            .with_metadata("region", "us-west-2")
            .with_metadata("tier", "premium");

        assert_eq!(
            member.metadata.get("region"),
            Some(&"us-west-2".to_string())
        );
        assert_eq!(member.metadata.get("tier"), Some(&"premium".to_string()));
    }

    #[test]
    fn test_group_result() {
        let success: GroupResult<i32> = GroupResult::Success(42);
        assert!(success.is_success());
        assert_eq!(success.unwrap(), 42);

        let error: GroupResult<i32> = GroupResult::Error(GroupError::GroupFull);
        assert!(!error.is_success());
    }
}
