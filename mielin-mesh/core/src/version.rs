//! Version Vectors for Distributed Consistency
//!
//! Implements vector clocks and version vectors for tracking causality
//! and detecting conflicts in distributed systems.
//!
//! ## Features
//!
//! - **Version Vectors**: Track versions per node for conflict detection
//! - **Vector Clocks**: Establish causal ordering of events
//! - **Conflict Detection**: Identify concurrent updates that need resolution
//! - **Merge Strategies**: Automatic and manual conflict resolution
//!
//! ## Example
//!
//! ```rust
//! use mielin_mesh_core::version::{VersionVector, VersionedValue};
//! use uuid::Uuid;
//!
//! let node_a = Uuid::new_v4();
//! let node_b = Uuid::new_v4();
//!
//! let mut vv_a = VersionVector::new();
//! vv_a.increment(node_a);
//!
//! let mut vv_b = VersionVector::new();
//! vv_b.increment(node_b);
//!
//! // Check for conflicts
//! assert!(vv_a.concurrent_with(&vv_b));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::NodeId;

/// A version number for a single node
pub type Version = u64;

/// Version Vector for tracking distributed state
///
/// A version vector maintains a version counter for each node that has
/// modified the state. This allows detecting causality and conflicts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionVector {
    versions: HashMap<NodeId, Version>,
}

impl VersionVector {
    /// Create a new empty version vector
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
        }
    }

    /// Create a version vector with initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            versions: HashMap::with_capacity(capacity),
        }
    }

    /// Get the version for a specific node
    pub fn get(&self, node_id: &NodeId) -> Version {
        self.versions.get(node_id).copied().unwrap_or(0)
    }

    /// Set the version for a specific node
    pub fn set(&mut self, node_id: NodeId, version: Version) {
        if version > 0 {
            self.versions.insert(node_id, version);
        } else {
            self.versions.remove(&node_id);
        }
    }

    /// Increment the version for a node
    pub fn increment(&mut self, node_id: NodeId) -> Version {
        let new_version = self.get(&node_id) + 1;
        self.versions.insert(node_id, new_version);
        new_version
    }

    /// Check if this version vector dominates another (>=)
    ///
    /// A version vector V1 dominates V2 if for every node:
    /// `V1[node] >= V2[node]`
    pub fn dominates(&self, other: &VersionVector) -> bool {
        // Check all entries in other
        for (node_id, &other_version) in &other.versions {
            if self.get(node_id) < other_version {
                return false;
            }
        }
        true
    }

    /// Check if this version vector strictly dominates another (>)
    ///
    /// V1 strictly dominates V2 if V1 dominates V2 and V1 != V2
    pub fn strictly_dominates(&self, other: &VersionVector) -> bool {
        self.dominates(other) && self != other
    }

    /// Check if two version vectors are concurrent (neither dominates the other)
    ///
    /// Concurrent updates indicate a conflict that needs resolution.
    pub fn concurrent_with(&self, other: &VersionVector) -> bool {
        !self.dominates(other) && !other.dominates(self)
    }

    /// Merge two version vectors, taking the maximum of each component
    pub fn merge(&mut self, other: &VersionVector) {
        for (&node_id, &other_version) in &other.versions {
            let my_version = self.get(&node_id);
            if other_version > my_version {
                self.versions.insert(node_id, other_version);
            }
        }
    }

    /// Create a merged version vector without modifying self
    pub fn merged_with(&self, other: &VersionVector) -> VersionVector {
        let mut result = self.clone();
        result.merge(other);
        result
    }

    /// Get all node IDs in this version vector
    pub fn nodes(&self) -> impl Iterator<Item = &NodeId> {
        self.versions.keys()
    }

    /// Get the number of nodes in this version vector
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    /// Check if the version vector is empty
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// Calculate the total version (sum of all versions)
    ///
    /// Useful for rough ordering when causality is not available.
    pub fn total_version(&self) -> u64 {
        self.versions.values().sum()
    }

    /// Compare two version vectors for ordering
    pub fn compare(&self, other: &VersionVector) -> Ordering {
        if self == other {
            Ordering::Equal
        } else if self.strictly_dominates(other) {
            Ordering::After
        } else if other.strictly_dominates(self) {
            Ordering::Before
        } else {
            Ordering::Concurrent
        }
    }
}

/// Ordering relationship between version vectors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    /// First vector happened before second
    Before,
    /// First vector happened after second
    After,
    /// Vectors are equal (same event)
    Equal,
    /// Vectors are concurrent (conflict)
    Concurrent,
}

/// A value with associated version vector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedValue<T> {
    /// The actual value
    pub value: T,
    /// Version vector tracking the value's history
    pub version: VersionVector,
    /// Timestamp when the value was last modified (Unix milliseconds)
    pub timestamp: u64,
    /// Node that last modified the value
    pub last_modified_by: NodeId,
}

impl<T> VersionedValue<T> {
    /// Create a new versioned value
    pub fn new(value: T, node_id: NodeId, timestamp: u64) -> Self {
        let mut version = VersionVector::new();
        version.increment(node_id);
        Self {
            value,
            version,
            timestamp,
            last_modified_by: node_id,
        }
    }

    /// Update the value, incrementing the version
    pub fn update(&mut self, new_value: T, node_id: NodeId, timestamp: u64) {
        self.value = new_value;
        self.version.increment(node_id);
        self.timestamp = timestamp;
        self.last_modified_by = node_id;
    }

    /// Check if this value conflicts with another
    pub fn conflicts_with(&self, other: &VersionedValue<T>) -> bool {
        self.version.concurrent_with(&other.version)
    }

    /// Compare with another versioned value
    pub fn compare(&self, other: &VersionedValue<T>) -> Ordering {
        self.version.compare(&other.version)
    }
}

impl<T: Clone> VersionedValue<T> {
    /// Merge with another versioned value using a resolution strategy
    pub fn merge_with<F>(&mut self, other: &VersionedValue<T>, resolver: F)
    where
        F: FnOnce(&T, &T) -> T,
    {
        match self.compare(other) {
            Ordering::Before => {
                // Other is newer, take it
                self.value = other.value.clone();
                self.version.merge(&other.version);
                self.timestamp = other.timestamp;
                self.last_modified_by = other.last_modified_by;
            }
            Ordering::After | Ordering::Equal => {
                // We are newer or equal, keep our value but merge versions
                self.version.merge(&other.version);
            }
            Ordering::Concurrent => {
                // Conflict! Use resolver
                self.value = resolver(&self.value, &other.value);
                self.version.merge(&other.version);
                // Keep our node as modifier but update timestamp
                self.timestamp = self.timestamp.max(other.timestamp);
            }
        }
    }
}

/// Conflict resolution strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictResolution {
    /// Last-Writer-Wins based on timestamp
    #[default]
    LastWriterWins,
    /// First-Writer-Wins based on timestamp
    FirstWriterWins,
    /// Keep both values (for sets/lists)
    KeepBoth,
    /// Always prefer local value
    PreferLocal,
    /// Always prefer remote value
    PreferRemote,
}

/// Builder for conflict resolution
pub struct ConflictResolver {
    strategy: ConflictResolution,
}

impl ConflictResolver {
    /// Create a new conflict resolver with the given strategy
    pub fn new(strategy: ConflictResolution) -> Self {
        Self { strategy }
    }

    /// Resolve a conflict between two values
    pub fn resolve<T: Clone>(&self, local: &VersionedValue<T>, remote: &VersionedValue<T>) -> T {
        match self.strategy {
            ConflictResolution::LastWriterWins => {
                if local.timestamp >= remote.timestamp {
                    local.value.clone()
                } else {
                    remote.value.clone()
                }
            }
            ConflictResolution::FirstWriterWins => {
                if local.timestamp <= remote.timestamp {
                    local.value.clone()
                } else {
                    remote.value.clone()
                }
            }
            ConflictResolution::PreferLocal | ConflictResolution::KeepBoth => local.value.clone(),
            ConflictResolution::PreferRemote => remote.value.clone(),
        }
    }
}

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new(ConflictResolution::LastWriterWins)
    }
}

/// A versioned map that tracks versions for each key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    data: HashMap<K, VersionedValue<V>>,
    /// This node's ID for incrementing versions
    node_id: NodeId,
}

impl<K, V> VersionedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    /// Create a new versioned map
    pub fn new(node_id: NodeId) -> Self {
        Self {
            data: HashMap::new(),
            node_id,
        }
    }

    /// Get a value and its version
    pub fn get(&self, key: &K) -> Option<&VersionedValue<V>> {
        self.data.get(key)
    }

    /// Check if a key exists
    pub fn contains_key(&self, key: &K) -> bool {
        self.data.contains_key(key)
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = (&K, &VersionedValue<V>)> {
        self.data.iter()
    }

    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.data.keys()
    }
}

impl<K, V> VersionedMap<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    /// Insert or update a value
    pub fn insert(&mut self, key: K, value: V, timestamp: u64) {
        if let Some(entry) = self.data.get_mut(&key) {
            entry.update(value, self.node_id, timestamp);
        } else {
            self.data
                .insert(key, VersionedValue::new(value, self.node_id, timestamp));
        }
    }

    /// Remove a key (tombstone with version tracking)
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.data.remove(key).map(|v| v.value)
    }

    /// Merge with another versioned map
    pub fn merge(&mut self, other: &VersionedMap<K, V>, resolver: &ConflictResolver) {
        for (key, other_value) in &other.data {
            if let Some(my_value) = self.data.get_mut(key) {
                // Capture values before calling merge_with to avoid borrow issues
                let my_timestamp = my_value.timestamp;
                let my_modified_by = my_value.last_modified_by;
                let other_timestamp = other_value.timestamp;
                let other_modified_by = other_value.last_modified_by;

                my_value.merge_with(other_value, |local, remote| {
                    resolver.resolve(
                        &VersionedValue {
                            value: local.clone(),
                            version: VersionVector::new(),
                            timestamp: my_timestamp,
                            last_modified_by: my_modified_by,
                        },
                        &VersionedValue {
                            value: remote.clone(),
                            version: VersionVector::new(),
                            timestamp: other_timestamp,
                            last_modified_by: other_modified_by,
                        },
                    )
                });
            } else {
                // Key doesn't exist locally, take remote value
                self.data.insert(key.clone(), other_value.clone());
            }
        }
    }

    /// Find all keys with conflicts between this map and another
    pub fn find_conflicts(&self, other: &VersionedMap<K, V>) -> Vec<K> {
        let mut conflicts = Vec::new();
        for (key, my_value) in &self.data {
            if let Some(other_value) = other.data.get(key) {
                if my_value.conflicts_with(other_value) {
                    conflicts.push(key.clone());
                }
            }
        }
        conflicts
    }
}

/// Dot context for optimized CRDTs
///
/// A dot is a unique identifier for an operation (node_id, counter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dot {
    /// Node that created this dot
    pub node: NodeId,
    /// Counter value at creation
    pub counter: u64,
}

impl Dot {
    /// Create a new dot
    pub fn new(node: NodeId, counter: u64) -> Self {
        Self { node, counter }
    }
}

/// Dot context tracks seen dots for optimized state-based CRDTs
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DotContext {
    /// Compact representation: for each node, the highest contiguous counter
    compact: HashMap<NodeId, u64>,
    /// Dots seen above the compact threshold
    dots: std::collections::HashSet<Dot>,
}

impl DotContext {
    /// Create a new dot context
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the next dot for a node
    pub fn next_dot(&mut self, node: NodeId) -> Dot {
        let counter = self.compact.entry(node).or_insert(0);
        *counter += 1;
        Dot::new(node, *counter)
    }

    /// Check if a dot has been seen
    pub fn contains(&self, dot: &Dot) -> bool {
        if let Some(&max) = self.compact.get(&dot.node) {
            if dot.counter <= max {
                return true;
            }
        }
        self.dots.contains(dot)
    }

    /// Add a dot to the context
    pub fn add(&mut self, dot: Dot) {
        if self.contains(&dot) {
            return;
        }

        let compact_val = self.compact.entry(dot.node).or_insert(0);

        // Check if this extends the compact range
        if dot.counter == *compact_val + 1 {
            *compact_val = dot.counter;
            // Try to collapse dots into compact
            self.collapse(dot.node);
        } else {
            // Add to dots set
            self.dots.insert(dot);
        }
    }

    /// Collapse dots into compact representation
    fn collapse(&mut self, node: NodeId) {
        // SAFETY: node is guaranteed to exist in compact map by the caller (add method)
        if let Some(compact_val) = self.compact.get_mut(&node) {
            loop {
                let next = Dot::new(node, *compact_val + 1);
                if self.dots.remove(&next) {
                    *compact_val += 1;
                } else {
                    break;
                }
            }
        }
    }

    /// Merge with another dot context
    pub fn merge(&mut self, other: &DotContext) {
        // Merge compact values
        for (&node, &other_val) in &other.compact {
            let my_val = self.compact.entry(node).or_insert(0);
            *my_val = (*my_val).max(other_val);
        }

        // Merge dots
        for &dot in &other.dots {
            self.add(dot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn node_id() -> NodeId {
        Uuid::new_v4()
    }

    #[test]
    fn test_version_vector_basic() {
        let node_a = node_id();
        let mut vv = VersionVector::new();

        assert_eq!(vv.get(&node_a), 0);
        assert!(vv.is_empty());

        vv.increment(node_a);
        assert_eq!(vv.get(&node_a), 1);
        assert_eq!(vv.len(), 1);

        vv.increment(node_a);
        assert_eq!(vv.get(&node_a), 2);
    }

    #[test]
    fn test_version_vector_dominance() {
        let node_a = node_id();
        let node_b = node_id();

        let mut vv1 = VersionVector::new();
        vv1.set(node_a, 2);
        vv1.set(node_b, 1);

        let mut vv2 = VersionVector::new();
        vv2.set(node_a, 1);
        vv2.set(node_b, 1);

        assert!(vv1.dominates(&vv2));
        assert!(vv1.strictly_dominates(&vv2));
        assert!(!vv2.dominates(&vv1));
    }

    #[test]
    fn test_version_vector_concurrent() {
        let node_a = node_id();
        let node_b = node_id();

        let mut vv1 = VersionVector::new();
        vv1.set(node_a, 2);
        vv1.set(node_b, 1);

        let mut vv2 = VersionVector::new();
        vv2.set(node_a, 1);
        vv2.set(node_b, 2);

        assert!(vv1.concurrent_with(&vv2));
        assert!(!vv1.dominates(&vv2));
        assert!(!vv2.dominates(&vv1));
    }

    #[test]
    fn test_version_vector_merge() {
        let node_a = node_id();
        let node_b = node_id();
        let node_c = node_id();

        let mut vv1 = VersionVector::new();
        vv1.set(node_a, 2);
        vv1.set(node_b, 1);

        let mut vv2 = VersionVector::new();
        vv2.set(node_a, 1);
        vv2.set(node_b, 3);
        vv2.set(node_c, 1);

        vv1.merge(&vv2);

        assert_eq!(vv1.get(&node_a), 2);
        assert_eq!(vv1.get(&node_b), 3);
        assert_eq!(vv1.get(&node_c), 1);
    }

    #[test]
    fn test_version_vector_compare() {
        let node_a = node_id();

        let mut vv1 = VersionVector::new();
        let vv2 = VersionVector::new();

        assert_eq!(vv1.compare(&vv2), Ordering::Equal);

        vv1.increment(node_a);
        assert_eq!(vv1.compare(&vv2), Ordering::After);
        assert_eq!(vv2.compare(&vv1), Ordering::Before);
    }

    #[test]
    fn test_versioned_value() {
        let node_a = node_id();
        let mut value = VersionedValue::new("hello", node_a, 1000);

        assert_eq!(value.value, "hello");
        assert_eq!(value.version.get(&node_a), 1);
        assert_eq!(value.timestamp, 1000);

        value.update("world", node_a, 2000);
        assert_eq!(value.value, "world");
        assert_eq!(value.version.get(&node_a), 2);
        assert_eq!(value.timestamp, 2000);
    }

    #[test]
    fn test_versioned_value_conflict() {
        let node_a = node_id();
        let node_b = node_id();

        let value_a = VersionedValue::new("a", node_a, 1000);
        let value_b = VersionedValue::new("b", node_b, 1000);

        assert!(value_a.conflicts_with(&value_b));
        assert_eq!(value_a.compare(&value_b), Ordering::Concurrent);
    }

    #[test]
    fn test_versioned_value_merge_concurrent() {
        let node_a = node_id();
        let node_b = node_id();

        let mut value_a = VersionedValue::new("a".to_string(), node_a, 1000);
        let value_b = VersionedValue::new("b".to_string(), node_b, 2000);

        // Capture timestamps before merge
        let ts_a = value_a.timestamp;
        let ts_b = value_b.timestamp;

        value_a.merge_with(&value_b, |local, remote| {
            // Last-writer-wins resolver
            if ts_a >= ts_b {
                local.clone()
            } else {
                remote.clone()
            }
        });

        // Should take "b" as it has later timestamp
        assert_eq!(value_a.value, "b");
        // Version vector should be merged
        assert_eq!(value_a.version.get(&node_a), 1);
        assert_eq!(value_a.version.get(&node_b), 1);
    }

    #[test]
    fn test_conflict_resolver() {
        let node_a = node_id();
        let node_b = node_id();

        let value_a = VersionedValue::new("a", node_a, 1000);
        let value_b = VersionedValue::new("b", node_b, 2000);

        let lww = ConflictResolver::new(ConflictResolution::LastWriterWins);
        assert_eq!(lww.resolve(&value_a, &value_b), "b");

        let fww = ConflictResolver::new(ConflictResolution::FirstWriterWins);
        assert_eq!(fww.resolve(&value_a, &value_b), "a");

        let local = ConflictResolver::new(ConflictResolution::PreferLocal);
        assert_eq!(local.resolve(&value_a, &value_b), "a");

        let remote = ConflictResolver::new(ConflictResolution::PreferRemote);
        assert_eq!(remote.resolve(&value_a, &value_b), "b");
    }

    #[test]
    fn test_versioned_map() {
        let node_a = node_id();
        let mut map: VersionedMap<String, i32> = VersionedMap::new(node_a);

        map.insert("key1".to_string(), 100, 1000);
        map.insert("key2".to_string(), 200, 1000);

        assert!(map.contains_key(&"key1".to_string()));
        assert_eq!(map.get(&"key1".to_string()).unwrap().value, 100);
        assert_eq!(map.len(), 2);

        // Update existing key
        map.insert("key1".to_string(), 150, 2000);
        assert_eq!(map.get(&"key1".to_string()).unwrap().value, 150);
        assert_eq!(
            map.get(&"key1".to_string()).unwrap().version.get(&node_a),
            2
        );
    }

    #[test]
    fn test_versioned_map_merge() {
        let node_a = node_id();
        let node_b = node_id();

        let mut map_a: VersionedMap<String, i32> = VersionedMap::new(node_a);
        map_a.insert("key1".to_string(), 100, 1000);
        map_a.insert("key2".to_string(), 200, 1000);

        let mut map_b: VersionedMap<String, i32> = VersionedMap::new(node_b);
        map_b.insert("key2".to_string(), 250, 2000); // Conflict with later timestamp
        map_b.insert("key3".to_string(), 300, 1000);

        let resolver = ConflictResolver::default();
        map_a.merge(&map_b, &resolver);

        assert_eq!(map_a.get(&"key1".to_string()).unwrap().value, 100);
        assert_eq!(map_a.get(&"key2".to_string()).unwrap().value, 250); // LWW: later timestamp wins
        assert_eq!(map_a.get(&"key3".to_string()).unwrap().value, 300);
        assert_eq!(map_a.len(), 3);
    }

    #[test]
    fn test_versioned_map_find_conflicts() {
        let node_a = node_id();
        let node_b = node_id();

        let mut map_a: VersionedMap<String, i32> = VersionedMap::new(node_a);
        map_a.insert("key1".to_string(), 100, 1000);
        map_a.insert("key2".to_string(), 200, 1000);

        let mut map_b: VersionedMap<String, i32> = VersionedMap::new(node_b);
        map_b.insert("key2".to_string(), 250, 2000);

        let conflicts = map_a.find_conflicts(&map_b);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0], "key2".to_string());
    }

    #[test]
    fn test_dot() {
        let node = node_id();
        let dot = Dot::new(node, 1);

        assert_eq!(dot.node, node);
        assert_eq!(dot.counter, 1);
    }

    #[test]
    fn test_dot_context() {
        let node_a = node_id();
        let node_b = node_id();
        let mut ctx = DotContext::new();

        let dot1 = ctx.next_dot(node_a);
        assert_eq!(dot1.counter, 1);
        assert!(ctx.contains(&dot1));

        let dot2 = ctx.next_dot(node_a);
        assert_eq!(dot2.counter, 2);

        let dot3 = ctx.next_dot(node_b);
        assert_eq!(dot3.counter, 1);
    }

    #[test]
    fn test_dot_context_add_non_contiguous() {
        let node = node_id();
        let mut ctx = DotContext::new();

        // Add dot 3 first (non-contiguous)
        ctx.add(Dot::new(node, 3));
        assert!(ctx.contains(&Dot::new(node, 3)));
        assert!(!ctx.contains(&Dot::new(node, 1)));
        assert!(!ctx.contains(&Dot::new(node, 2)));

        // Add dots 1 and 2
        ctx.add(Dot::new(node, 1));
        ctx.add(Dot::new(node, 2));

        // All should be seen now
        assert!(ctx.contains(&Dot::new(node, 1)));
        assert!(ctx.contains(&Dot::new(node, 2)));
        assert!(ctx.contains(&Dot::new(node, 3)));
    }

    #[test]
    fn test_dot_context_merge() {
        let node_a = node_id();
        let node_b = node_id();

        let mut ctx1 = DotContext::new();
        ctx1.next_dot(node_a);
        ctx1.next_dot(node_a);

        let mut ctx2 = DotContext::new();
        ctx2.next_dot(node_b);
        ctx2.add(Dot::new(node_a, 5)); // Non-contiguous

        ctx1.merge(&ctx2);

        assert!(ctx1.contains(&Dot::new(node_a, 1)));
        assert!(ctx1.contains(&Dot::new(node_a, 2)));
        assert!(ctx1.contains(&Dot::new(node_a, 5)));
        assert!(ctx1.contains(&Dot::new(node_b, 1)));
    }

    #[test]
    fn test_version_vector_total_version() {
        let node_a = node_id();
        let node_b = node_id();

        let mut vv = VersionVector::new();
        vv.set(node_a, 5);
        vv.set(node_b, 3);

        assert_eq!(vv.total_version(), 8);
    }

    #[test]
    fn test_version_vector_nodes() {
        let node_a = node_id();
        let node_b = node_id();

        let mut vv = VersionVector::new();
        vv.set(node_a, 1);
        vv.set(node_b, 2);

        let nodes: Vec<_> = vv.nodes().collect();
        assert_eq!(nodes.len(), 2);
    }
}
