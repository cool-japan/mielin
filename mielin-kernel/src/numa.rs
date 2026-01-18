//! NUMA (Non-Uniform Memory Access) Support
//!
//! Provides NUMA-aware memory allocation and task placement for multi-socket
//! and heterogeneous memory systems. NUMA awareness is critical for performance
//! on modern servers and high-performance computing platforms.
//!
//! ## Overview
//!
//! NUMA systems have multiple memory nodes where memory access latency varies
//! depending on which CPU accesses which memory node. Local memory access is
//! fast, while remote access incurs additional latency.
//!
//! ```text
//! ┌─────────────┐          ┌─────────────┐
//! │   Node 0    │──────────│   Node 1    │
//! │ ┌─────────┐ │   QPI/   │ ┌─────────┐ │
//! │ │ CPU 0-3 │ │   UPI    │ │ CPU 4-7 │ │
//! │ └────┬────┘ │  Link    │ └────┬────┘ │
//! │      │      │          │      │      │
//! │ ┌────▼────┐ │          │ ┌────▼────┐ │
//! │ │ Memory  │ │          │ │ Memory  │ │
//! │ │  16GB   │ │          │ │  16GB   │ │
//! │ └─────────┘ │          │ └─────────┘ │
//! └─────────────┘          └─────────────┘
//!        │                        │
//!        └────────────────────────┘
//!            Distance Matrix:
//!            Node 0 → Node 0: 10
//!            Node 0 → Node 1: 20
//!            Node 1 → Node 0: 20
//!            Node 1 → Node 1: 10
//! ```
//!
//! ## Features
//!
//! - **NUMA Node Abstraction**: Represent and discover NUMA topology
//! - **Node-Local Allocation**: Allocate memory on specific NUMA nodes
//! - **Distance Matrix**: Track inter-node access latencies
//! - **Task Affinity**: Pin tasks to specific NUMA nodes for data locality
//! - **Statistics**: Track local vs remote memory access patterns
//!
//! ## Example
//!
//! ```rust,ignore
//! use mielin_kernel::numa::{self, NodeId, NumaPolicy};
//!
//! // Initialize NUMA subsystem
//! numa::init()?;
//!
//! // Allocate memory on a specific node
//! let node = NodeId(0);
//! let pages = numa::allocate_on_node(node, 4)?;
//!
//! // Set NUMA policy for current task
//! numa::set_policy(NumaPolicy::Local)?;
//!
//! // Get NUMA statistics
//! let stats = numa::get_stats()?;
//! println!("Local allocations: {}", stats.local_allocations);
//! ```

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of NUMA nodes supported
pub const MAX_NUMA_NODES: usize = 8;

/// Maximum number of CPUs per NUMA node
pub const MAX_CPUS_PER_NODE: usize = 32;

/// Maximum number of CPUs supported across all nodes
pub const MAX_CPUS: usize = MAX_NUMA_NODES * MAX_CPUS_PER_NODE;

/// Distance representing local access (same node)
pub const LOCAL_DISTANCE: u8 = 10;

/// Distance representing remote access (different node, typical)
pub const REMOTE_DISTANCE: u8 = 20;

/// Maximum distance value (unreachable)
pub const UNREACHABLE_DISTANCE: u8 = 255;

/// Default pages per node for simulation
const DEFAULT_PAGES_PER_NODE: usize = 256;

// =============================================================================
// Error Types
// =============================================================================

/// NUMA-specific errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumaError {
    /// NUMA subsystem not initialized
    NotInitialized,
    /// Already initialized
    AlreadyInitialized,
    /// Invalid NUMA node ID
    InvalidNodeId {
        /// The invalid node ID
        node_id: usize,
        /// Maximum valid node ID
        max_node_id: usize,
    },
    /// Node is offline
    NodeOffline {
        /// The offline node ID
        node_id: usize,
    },
    /// No memory available on the specified node
    OutOfMemory {
        /// The node that ran out of memory
        node_id: usize,
        /// Requested pages
        requested: usize,
        /// Available pages
        available: usize,
    },
    /// Invalid CPU ID
    InvalidCpuId {
        /// The invalid CPU ID
        cpu_id: usize,
        /// Maximum valid CPU ID
        max_cpu_id: usize,
    },
    /// CPU not mapped to any node
    CpuNotMapped {
        /// The unmapped CPU ID
        cpu_id: usize,
    },
    /// Invalid allocation request
    InvalidAllocation,
    /// Hardware topology not supported
    TopologyNotSupported,
    /// Operation would cross NUMA boundaries
    CrossNodeAccess {
        /// Source node
        source_node: usize,
        /// Target node
        target_node: usize,
    },
}

impl core::fmt::Display for NumaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "NUMA subsystem not initialized"),
            Self::AlreadyInitialized => write!(f, "NUMA subsystem already initialized"),
            Self::InvalidNodeId {
                node_id,
                max_node_id,
            } => {
                write!(
                    f,
                    "Invalid NUMA node ID: {} (max: {})",
                    node_id, max_node_id
                )
            }
            Self::NodeOffline { node_id } => {
                write!(f, "NUMA node {} is offline", node_id)
            }
            Self::OutOfMemory {
                node_id,
                requested,
                available,
            } => {
                write!(
                    f,
                    "Out of memory on node {}: requested {} pages, {} available",
                    node_id, requested, available
                )
            }
            Self::InvalidCpuId { cpu_id, max_cpu_id } => {
                write!(f, "Invalid CPU ID: {} (max: {})", cpu_id, max_cpu_id)
            }
            Self::CpuNotMapped { cpu_id } => {
                write!(f, "CPU {} is not mapped to any NUMA node", cpu_id)
            }
            Self::InvalidAllocation => write!(f, "Invalid allocation request"),
            Self::TopologyNotSupported => write!(f, "NUMA topology not supported"),
            Self::CrossNodeAccess {
                source_node,
                target_node,
            } => {
                write!(
                    f,
                    "Cross-node access from node {} to node {}",
                    source_node, target_node
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NumaError {}

// =============================================================================
// Node ID Type
// =============================================================================

/// NUMA Node Identifier
///
/// A type-safe wrapper around the numeric node ID to prevent
/// accidental mixing with other integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeId(pub usize);

impl NodeId {
    /// Create a new NodeId
    pub const fn new(id: usize) -> Self {
        Self(id)
    }

    /// Get the raw node ID
    pub const fn as_usize(&self) -> usize {
        self.0
    }

    /// Check if this is a valid node ID
    pub fn is_valid(&self) -> bool {
        self.0 < MAX_NUMA_NODES
    }
}

impl From<usize> for NodeId {
    fn from(id: usize) -> Self {
        Self(id)
    }
}

impl From<NodeId> for usize {
    fn from(id: NodeId) -> Self {
        id.0
    }
}

// =============================================================================
// NUMA Policy
// =============================================================================

/// Memory allocation policy for NUMA systems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumaPolicy {
    /// Allocate on the local node (where the task runs)
    #[default]
    Local,
    /// Allocate on a specific node (bind)
    Bind(NodeId),
    /// Interleave allocations across multiple nodes
    Interleave,
    /// Prefer a node but fall back to others if needed
    Preferred(NodeId),
    /// First-touch policy (allocate on first access)
    FirstTouch,
}

impl NumaPolicy {
    /// Check if this policy prefers local allocation
    pub fn prefers_local(&self) -> bool {
        matches!(self, Self::Local | Self::FirstTouch)
    }

    /// Check if this policy requires a specific node
    pub fn required_node(&self) -> Option<NodeId> {
        match self {
            Self::Bind(node) => Some(*node),
            _ => None,
        }
    }

    /// Check if this policy allows fallback to other nodes
    pub fn allows_fallback(&self) -> bool {
        matches!(self, Self::Local | Self::Preferred(_) | Self::FirstTouch)
    }
}

// =============================================================================
// NUMA Node
// =============================================================================

/// Per-node memory statistics
#[derive(Debug)]
struct NodeMemoryStats {
    /// Total pages on this node
    total_pages: AtomicUsize,
    /// Free pages on this node
    free_pages: AtomicUsize,
    /// Total allocations on this node
    allocations: AtomicU64,
    /// Total deallocations on this node
    deallocations: AtomicU64,
    /// Local allocations (task on same node)
    local_allocations: AtomicU64,
    /// Remote allocations (task on different node)
    remote_allocations: AtomicU64,
    /// Pages migrated to this node
    pages_migrated_in: AtomicU64,
    /// Pages migrated from this node
    pages_migrated_out: AtomicU64,
}

impl NodeMemoryStats {
    const fn new() -> Self {
        Self {
            total_pages: AtomicUsize::new(0),
            free_pages: AtomicUsize::new(0),
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            local_allocations: AtomicU64::new(0),
            remote_allocations: AtomicU64::new(0),
            pages_migrated_in: AtomicU64::new(0),
            pages_migrated_out: AtomicU64::new(0),
        }
    }

    fn reset(&self, total_pages: usize) {
        self.total_pages.store(total_pages, Ordering::SeqCst);
        self.free_pages.store(total_pages, Ordering::SeqCst);
        self.allocations.store(0, Ordering::SeqCst);
        self.deallocations.store(0, Ordering::SeqCst);
        self.local_allocations.store(0, Ordering::SeqCst);
        self.remote_allocations.store(0, Ordering::SeqCst);
        self.pages_migrated_in.store(0, Ordering::SeqCst);
        self.pages_migrated_out.store(0, Ordering::SeqCst);
    }

    fn record_allocation(&self, pages: usize, is_local: bool) {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.free_pages.fetch_sub(pages, Ordering::SeqCst);
        if is_local {
            self.local_allocations.fetch_add(1, Ordering::Relaxed);
        } else {
            self.remote_allocations.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_deallocation(&self, pages: usize) {
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.free_pages.fetch_add(pages, Ordering::SeqCst);
    }

    fn record_migration_in(&self, pages: usize) {
        self.pages_migrated_in
            .fetch_add(pages as u64, Ordering::Relaxed);
    }

    fn record_migration_out(&self, pages: usize) {
        self.pages_migrated_out
            .fetch_add(pages as u64, Ordering::Relaxed);
    }
}

/// NUMA Node representation
///
/// Represents a single NUMA node with its CPUs, memory, and statistics.
#[repr(align(64))] // Cache line alignment to prevent false sharing
pub struct NumaNode {
    /// Node ID
    node_id: NodeId,
    /// Is this node online?
    online: AtomicBool,
    /// CPUs belonging to this node (bitmap)
    cpu_mask: AtomicU64,
    /// Number of CPUs on this node
    cpu_count: AtomicU32,
    /// Memory statistics
    stats: NodeMemoryStats,
    /// Memory capacity in pages
    capacity_pages: AtomicUsize,
    /// Current allocation offset (for simple bump allocation)
    alloc_offset: AtomicUsize,
}

impl NumaNode {
    /// Create a new NUMA node
    const fn new(node_id: usize) -> Self {
        Self {
            node_id: NodeId(node_id),
            online: AtomicBool::new(false),
            cpu_mask: AtomicU64::new(0),
            cpu_count: AtomicU32::new(0),
            stats: NodeMemoryStats::new(),
            capacity_pages: AtomicUsize::new(0),
            alloc_offset: AtomicUsize::new(0),
        }
    }

    /// Get node ID
    pub fn id(&self) -> NodeId {
        self.node_id
    }

    /// Check if node is online
    pub fn is_online(&self) -> bool {
        self.online.load(Ordering::Acquire)
    }

    /// Get number of CPUs on this node
    pub fn cpu_count(&self) -> u32 {
        self.cpu_count.load(Ordering::Relaxed)
    }

    /// Check if a CPU belongs to this node
    pub fn has_cpu(&self, cpu_id: usize) -> bool {
        if cpu_id >= 64 {
            return false;
        }
        let mask = self.cpu_mask.load(Ordering::Relaxed);
        (mask & (1 << cpu_id)) != 0
    }

    /// Get total memory capacity in pages
    pub fn total_pages(&self) -> usize {
        self.capacity_pages.load(Ordering::Relaxed)
    }

    /// Get free pages on this node
    pub fn free_pages(&self) -> usize {
        self.stats.free_pages.load(Ordering::SeqCst)
    }

    /// Allocate pages on this node
    fn allocate(&self, pages: usize, is_local: bool) -> Result<usize, NumaError> {
        if !self.is_online() {
            return Err(NumaError::NodeOffline {
                node_id: self.node_id.0,
            });
        }

        let available = self.free_pages();
        if pages > available {
            return Err(NumaError::OutOfMemory {
                node_id: self.node_id.0,
                requested: pages,
                available,
            });
        }

        // Simple bump allocation (in a real implementation, this would use a proper allocator)
        let offset = self.alloc_offset.fetch_add(pages, Ordering::SeqCst);

        self.stats.record_allocation(pages, is_local);

        Ok(offset)
    }

    /// Free pages on this node
    fn deallocate(&self, _offset: usize, pages: usize) {
        self.stats.record_deallocation(pages);
    }

    /// Get statistics snapshot
    pub fn stats(&self) -> NodeStatsSnapshot {
        NodeStatsSnapshot {
            node_id: self.node_id,
            online: self.is_online(),
            cpu_count: self.cpu_count(),
            total_pages: self.total_pages(),
            free_pages: self.free_pages(),
            allocations: self.stats.allocations.load(Ordering::Relaxed),
            deallocations: self.stats.deallocations.load(Ordering::Relaxed),
            local_allocations: self.stats.local_allocations.load(Ordering::Relaxed),
            remote_allocations: self.stats.remote_allocations.load(Ordering::Relaxed),
            pages_migrated_in: self.stats.pages_migrated_in.load(Ordering::Relaxed),
            pages_migrated_out: self.stats.pages_migrated_out.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: NumaNode uses atomic operations for all mutable state
unsafe impl Send for NumaNode {}
unsafe impl Sync for NumaNode {}

// =============================================================================
// Statistics Snapshot
// =============================================================================

/// Snapshot of per-node statistics
#[derive(Debug, Clone, Copy)]
pub struct NodeStatsSnapshot {
    /// Node ID
    pub node_id: NodeId,
    /// Is node online
    pub online: bool,
    /// Number of CPUs
    pub cpu_count: u32,
    /// Total pages on this node
    pub total_pages: usize,
    /// Free pages on this node
    pub free_pages: usize,
    /// Total allocations
    pub allocations: u64,
    /// Total deallocations
    pub deallocations: u64,
    /// Local allocations
    pub local_allocations: u64,
    /// Remote allocations
    pub remote_allocations: u64,
    /// Pages migrated to this node
    pub pages_migrated_in: u64,
    /// Pages migrated from this node
    pub pages_migrated_out: u64,
}

impl NodeStatsSnapshot {
    /// Calculate memory utilization (0.0 - 1.0)
    pub fn utilization(&self) -> f64 {
        if self.total_pages == 0 {
            return 0.0;
        }
        let allocated = self.total_pages.saturating_sub(self.free_pages);
        allocated as f64 / self.total_pages as f64
    }

    /// Calculate local access ratio (0.0 - 1.0)
    pub fn local_access_ratio(&self) -> f64 {
        let total = self.local_allocations + self.remote_allocations;
        if total == 0 {
            return 1.0;
        }
        self.local_allocations as f64 / total as f64
    }

    /// Get net migration (positive = more in than out)
    pub fn net_migration(&self) -> i64 {
        self.pages_migrated_in as i64 - self.pages_migrated_out as i64
    }
}

/// Global NUMA statistics
#[derive(Debug, Clone, Copy)]
pub struct GlobalNumaStats {
    /// Number of online nodes
    pub online_nodes: usize,
    /// Total allocations across all nodes
    pub total_allocations: u64,
    /// Total local allocations
    pub total_local_allocations: u64,
    /// Total remote allocations
    pub total_remote_allocations: u64,
    /// Total pages available
    pub total_pages: usize,
    /// Total free pages
    pub total_free_pages: usize,
    /// Total migrations
    pub total_migrations: u64,
}

impl GlobalNumaStats {
    /// Calculate overall local access ratio
    pub fn local_access_ratio(&self) -> f64 {
        let total = self.total_local_allocations + self.total_remote_allocations;
        if total == 0 {
            return 1.0;
        }
        self.total_local_allocations as f64 / total as f64
    }

    /// Calculate overall utilization
    pub fn utilization(&self) -> f64 {
        if self.total_pages == 0 {
            return 0.0;
        }
        let allocated = self.total_pages.saturating_sub(self.total_free_pages);
        allocated as f64 / self.total_pages as f64
    }
}

// =============================================================================
// Distance Matrix
// =============================================================================

/// NUMA distance matrix
///
/// Stores the relative memory access latency between NUMA nodes.
/// Lower values indicate faster access.
struct DistanceMatrix {
    /// Distance values [from][to]
    distances: [[AtomicU32; MAX_NUMA_NODES]; MAX_NUMA_NODES],
}

impl DistanceMatrix {
    const fn new() -> Self {
        #[allow(clippy::declare_interior_mutable_const)]
        const ROW: [AtomicU32; MAX_NUMA_NODES] = [
            AtomicU32::new(LOCAL_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
            AtomicU32::new(UNREACHABLE_DISTANCE as u32),
        ];
        Self {
            distances: [ROW; MAX_NUMA_NODES],
        }
    }

    fn init(&self, num_nodes: usize) {
        for i in 0..num_nodes {
            for j in 0..num_nodes {
                let distance = if i == j {
                    LOCAL_DISTANCE
                } else {
                    REMOTE_DISTANCE
                };
                self.distances[i][j].store(distance as u32, Ordering::SeqCst);
            }
        }
    }

    fn get(&self, from: NodeId, to: NodeId) -> u8 {
        if from.0 >= MAX_NUMA_NODES || to.0 >= MAX_NUMA_NODES {
            return UNREACHABLE_DISTANCE;
        }
        self.distances[from.0][to.0].load(Ordering::Relaxed) as u8
    }

    fn set(&self, from: NodeId, to: NodeId, distance: u8) {
        if from.0 < MAX_NUMA_NODES && to.0 < MAX_NUMA_NODES {
            self.distances[from.0][to.0].store(distance as u32, Ordering::SeqCst);
        }
    }
}

// =============================================================================
// CPU to Node Mapping
// =============================================================================

/// CPU to NUMA node mapping
struct CpuNodeMap {
    /// Maps CPU ID to Node ID (None = not mapped)
    map: [AtomicU32; MAX_CPUS],
}

impl CpuNodeMap {
    const fn new() -> Self {
        #[allow(clippy::declare_interior_mutable_const)]
        const UNMAPPED: AtomicU32 = AtomicU32::new(u32::MAX);
        Self {
            map: [UNMAPPED; MAX_CPUS],
        }
    }

    fn set(&self, cpu_id: usize, node_id: NodeId) {
        if cpu_id < MAX_CPUS {
            self.map[cpu_id].store(node_id.0 as u32, Ordering::SeqCst);
        }
    }

    fn get(&self, cpu_id: usize) -> Option<NodeId> {
        if cpu_id >= MAX_CPUS {
            return None;
        }
        let value = self.map[cpu_id].load(Ordering::Relaxed);
        if value == u32::MAX {
            None
        } else {
            Some(NodeId(value as usize))
        }
    }
}

// =============================================================================
// Task NUMA Affinity
// =============================================================================

/// Task NUMA affinity configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskNumaAffinity {
    /// Preferred NUMA node
    pub preferred_node: Option<NodeId>,
    /// Memory allocation policy
    pub policy: NumaPolicy,
    /// Allow migration between nodes
    pub allow_migration: bool,
    /// Strict binding (fail if preferred node unavailable)
    pub strict: bool,
}

impl Default for TaskNumaAffinity {
    fn default() -> Self {
        Self {
            preferred_node: None,
            policy: NumaPolicy::Local,
            allow_migration: true,
            strict: false,
        }
    }
}

impl TaskNumaAffinity {
    /// Create affinity bound to a specific node
    pub fn bind(node: NodeId) -> Self {
        Self {
            preferred_node: Some(node),
            policy: NumaPolicy::Bind(node),
            allow_migration: false,
            strict: true,
        }
    }

    /// Create affinity preferring a specific node
    pub fn preferred(node: NodeId) -> Self {
        Self {
            preferred_node: Some(node),
            policy: NumaPolicy::Preferred(node),
            allow_migration: true,
            strict: false,
        }
    }

    /// Create affinity for local allocation
    pub fn local() -> Self {
        Self::default()
    }

    /// Create affinity for interleaved allocation
    pub fn interleave() -> Self {
        Self {
            preferred_node: None,
            policy: NumaPolicy::Interleave,
            allow_migration: true,
            strict: false,
        }
    }
}

// =============================================================================
// Global NUMA State
// =============================================================================

/// Global NUMA subsystem state
struct NumaState {
    /// Is NUMA initialized
    initialized: AtomicBool,
    /// NUMA nodes
    nodes: [NumaNode; MAX_NUMA_NODES],
    /// Number of online nodes
    num_online_nodes: AtomicUsize,
    /// Distance matrix
    distances: DistanceMatrix,
    /// CPU to node mapping
    cpu_map: CpuNodeMap,
    /// Default allocation policy
    default_policy: Mutex<NumaPolicy>,
    /// Current allocation node for interleaving
    interleave_index: AtomicUsize,
}

impl NumaState {
    const fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            nodes: [
                NumaNode::new(0),
                NumaNode::new(1),
                NumaNode::new(2),
                NumaNode::new(3),
                NumaNode::new(4),
                NumaNode::new(5),
                NumaNode::new(6),
                NumaNode::new(7),
            ],
            num_online_nodes: AtomicUsize::new(0),
            distances: DistanceMatrix::new(),
            cpu_map: CpuNodeMap::new(),
            default_policy: Mutex::new(NumaPolicy::Local),
            interleave_index: AtomicUsize::new(0),
        }
    }
}

static NUMA_STATE: NumaState = NumaState::new();

// =============================================================================
// Public API
// =============================================================================

/// Initialize the NUMA subsystem
///
/// Detects NUMA topology and initializes nodes.
pub fn init() -> Result<(), NumaError> {
    if NUMA_STATE.initialized.load(Ordering::SeqCst) {
        // In test mode, allow re-initialization
        #[cfg(any(test, feature = "std"))]
        {
            return reinit_for_test();
        }
        #[cfg(not(any(test, feature = "std")))]
        {
            return Err(NumaError::AlreadyInitialized);
        }
    }

    // Detect topology (simulated for now)
    let num_nodes = detect_topology()?;

    NUMA_STATE
        .num_online_nodes
        .store(num_nodes, Ordering::SeqCst);
    NUMA_STATE.initialized.store(true, Ordering::SeqCst);

    Ok(())
}

/// Initialize with custom configuration (for testing)
pub fn init_with_config(num_nodes: usize, pages_per_node: usize) -> Result<(), NumaError> {
    if num_nodes == 0 || num_nodes > MAX_NUMA_NODES {
        return Err(NumaError::InvalidNodeId {
            node_id: num_nodes,
            max_node_id: MAX_NUMA_NODES - 1,
        });
    }

    // Reset interleave index
    NUMA_STATE.interleave_index.store(0, Ordering::SeqCst);

    // Initialize each node with full reset
    for i in 0..num_nodes {
        let cpus: alloc::vec::Vec<usize> = ((i * 4)..(i * 4 + 4)).collect();

        // Reset node state
        NUMA_STATE.nodes[i]
            .capacity_pages
            .store(pages_per_node, Ordering::SeqCst);
        NUMA_STATE.nodes[i].alloc_offset.store(0, Ordering::SeqCst);
        NUMA_STATE.nodes[i].online.store(true, Ordering::SeqCst);
        NUMA_STATE.nodes[i].stats.reset(pages_per_node);

        // Set CPU mask
        let mut mask = 0u64;
        for &cpu in &cpus {
            if cpu < 64 {
                mask |= 1 << cpu;
            }
        }
        NUMA_STATE.nodes[i].cpu_mask.store(mask, Ordering::SeqCst);
        NUMA_STATE.nodes[i]
            .cpu_count
            .store(cpus.len() as u32, Ordering::SeqCst);

        // Map CPUs to node
        for cpu in &cpus {
            NUMA_STATE.cpu_map.set(*cpu, NodeId(i));
        }
    }

    // Initialize distance matrix
    NUMA_STATE.distances.init(num_nodes);

    // Reset default policy
    *NUMA_STATE.default_policy.lock() = NumaPolicy::Local;

    NUMA_STATE
        .num_online_nodes
        .store(num_nodes, Ordering::SeqCst);
    NUMA_STATE.initialized.store(true, Ordering::SeqCst);

    Ok(())
}

/// Reinitialize for testing
#[cfg(any(test, feature = "std"))]
fn reinit_for_test() -> Result<(), NumaError> {
    // Reset state for testing
    let num_nodes = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
    if num_nodes == 0 {
        return detect_topology().map(|_| ());
    }

    // Reset interleave index
    NUMA_STATE.interleave_index.store(0, Ordering::SeqCst);

    // Reset all nodes
    for i in 0..num_nodes {
        let total_pages = NUMA_STATE.nodes[i].total_pages();
        NUMA_STATE.nodes[i].alloc_offset.store(0, Ordering::SeqCst);
        NUMA_STATE.nodes[i].stats.reset(total_pages);
    }

    // Reset default policy
    *NUMA_STATE.default_policy.lock() = NumaPolicy::Local;

    Ok(())
}

/// Detect NUMA topology
fn detect_topology() -> Result<usize, NumaError> {
    // Default to 2 nodes for simulation
    let num_nodes = 2;

    // Reset interleave index
    NUMA_STATE.interleave_index.store(0, Ordering::SeqCst);

    for i in 0..num_nodes {
        // Each node gets 4 CPUs
        let cpus: alloc::vec::Vec<usize> = ((i * 4)..(i * 4 + 4)).collect();

        // Full reset for each node
        NUMA_STATE.nodes[i]
            .capacity_pages
            .store(DEFAULT_PAGES_PER_NODE, Ordering::SeqCst);
        NUMA_STATE.nodes[i].alloc_offset.store(0, Ordering::SeqCst);
        NUMA_STATE.nodes[i].online.store(true, Ordering::SeqCst);
        NUMA_STATE.nodes[i].stats.reset(DEFAULT_PAGES_PER_NODE);

        // Set CPU mask
        let mut mask = 0u64;
        for &cpu in &cpus {
            if cpu < 64 {
                mask |= 1 << cpu;
            }
        }
        NUMA_STATE.nodes[i].cpu_mask.store(mask, Ordering::SeqCst);
        NUMA_STATE.nodes[i]
            .cpu_count
            .store(cpus.len() as u32, Ordering::SeqCst);

        // Map CPUs to node
        for cpu in &cpus {
            NUMA_STATE.cpu_map.set(*cpu, NodeId(i));
        }
    }

    // Initialize distance matrix
    NUMA_STATE.distances.init(num_nodes);

    // Reset default policy
    *NUMA_STATE.default_policy.lock() = NumaPolicy::Local;

    Ok(num_nodes)
}

/// Check if NUMA is initialized
pub fn is_initialized() -> bool {
    NUMA_STATE.initialized.load(Ordering::SeqCst)
}

/// Get number of online NUMA nodes
pub fn num_nodes() -> Result<usize, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }
    Ok(NUMA_STATE.num_online_nodes.load(Ordering::SeqCst))
}

/// Get a reference to a NUMA node
pub fn get_node(node_id: NodeId) -> Result<&'static NumaNode, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let max_node = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
    if node_id.0 >= max_node {
        return Err(NumaError::InvalidNodeId {
            node_id: node_id.0,
            max_node_id: max_node.saturating_sub(1),
        });
    }

    let node = &NUMA_STATE.nodes[node_id.0];
    if !node.is_online() {
        return Err(NumaError::NodeOffline { node_id: node_id.0 });
    }

    Ok(node)
}

/// Get the NUMA node for a CPU
pub fn cpu_to_node(cpu_id: usize) -> Result<NodeId, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    if cpu_id >= MAX_CPUS {
        return Err(NumaError::InvalidCpuId {
            cpu_id,
            max_cpu_id: MAX_CPUS - 1,
        });
    }

    NUMA_STATE
        .cpu_map
        .get(cpu_id)
        .ok_or(NumaError::CpuNotMapped { cpu_id })
}

/// Get the distance between two nodes
pub fn get_distance(from: NodeId, to: NodeId) -> Result<u8, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let max_node = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
    if from.0 >= max_node {
        return Err(NumaError::InvalidNodeId {
            node_id: from.0,
            max_node_id: max_node.saturating_sub(1),
        });
    }
    if to.0 >= max_node {
        return Err(NumaError::InvalidNodeId {
            node_id: to.0,
            max_node_id: max_node.saturating_sub(1),
        });
    }

    Ok(NUMA_STATE.distances.get(from, to))
}

/// Set the distance between two nodes
pub fn set_distance(from: NodeId, to: NodeId, distance: u8) -> Result<(), NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let max_node = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
    if from.0 >= max_node || to.0 >= max_node {
        return Err(NumaError::InvalidNodeId {
            node_id: from.0.max(to.0),
            max_node_id: max_node.saturating_sub(1),
        });
    }

    NUMA_STATE.distances.set(from, to, distance);
    Ok(())
}

/// Allocate pages on a specific NUMA node
pub fn allocate_on_node(node_id: NodeId, pages: usize) -> Result<usize, NumaError> {
    allocate_on_node_with_locality(node_id, pages, true)
}

/// Allocate pages on a specific NUMA node with locality tracking
pub fn allocate_on_node_with_locality(
    node_id: NodeId,
    pages: usize,
    is_local: bool,
) -> Result<usize, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    if pages == 0 {
        return Err(NumaError::InvalidAllocation);
    }

    let node = get_node(node_id)?;
    node.allocate(pages, is_local)
}

/// Allocate pages using the default policy
pub fn allocate(pages: usize, current_node: Option<NodeId>) -> Result<(NodeId, usize), NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    if pages == 0 {
        return Err(NumaError::InvalidAllocation);
    }

    let policy = *NUMA_STATE.default_policy.lock();
    allocate_with_policy(pages, policy, current_node)
}

/// Allocate pages with a specific policy
pub fn allocate_with_policy(
    pages: usize,
    policy: NumaPolicy,
    current_node: Option<NodeId>,
) -> Result<(NodeId, usize), NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    match policy {
        NumaPolicy::Local | NumaPolicy::FirstTouch => {
            let node_id = current_node.unwrap_or(NodeId(0));
            let offset = allocate_on_node_with_locality(node_id, pages, true)?;
            Ok((node_id, offset))
        }
        NumaPolicy::Bind(node_id) => {
            let offset = allocate_on_node_with_locality(node_id, pages, false)?;
            Ok((node_id, offset))
        }
        NumaPolicy::Preferred(preferred) => {
            match allocate_on_node_with_locality(preferred, pages, true) {
                Ok(offset) => Ok((preferred, offset)),
                Err(_) => {
                    // Fall back to any available node
                    allocate_fallback(pages)
                }
            }
        }
        NumaPolicy::Interleave => {
            let num_nodes = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
            let index = NUMA_STATE.interleave_index.fetch_add(1, Ordering::Relaxed);
            let node_id = NodeId(index % num_nodes);
            let offset = allocate_on_node_with_locality(node_id, pages, false)?;
            Ok((node_id, offset))
        }
    }
}

/// Fallback allocation when preferred node is full
fn allocate_fallback(pages: usize) -> Result<(NodeId, usize), NumaError> {
    let num_nodes = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);

    for i in 0..num_nodes {
        let node_id = NodeId(i);
        if let Ok(offset) = allocate_on_node_with_locality(node_id, pages, false) {
            return Ok((node_id, offset));
        }
    }

    Err(NumaError::OutOfMemory {
        node_id: 0,
        requested: pages,
        available: 0,
    })
}

/// Free pages on a NUMA node
pub fn free_on_node(node_id: NodeId, offset: usize, pages: usize) -> Result<(), NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let node = get_node(node_id)?;
    node.deallocate(offset, pages);
    Ok(())
}

/// Set the default NUMA policy
pub fn set_policy(policy: NumaPolicy) -> Result<(), NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    *NUMA_STATE.default_policy.lock() = policy;
    Ok(())
}

/// Get the default NUMA policy
pub fn get_policy() -> Result<NumaPolicy, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    Ok(*NUMA_STATE.default_policy.lock())
}

/// Migrate pages from one node to another
pub fn migrate_pages(from_node: NodeId, to_node: NodeId, pages: usize) -> Result<(), NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    if from_node == to_node {
        return Ok(()); // No migration needed
    }

    let from = get_node(from_node)?;
    let to = get_node(to_node)?;

    // Record migration statistics
    from.stats.record_migration_out(pages);
    to.stats.record_migration_in(pages);

    Ok(())
}

/// Get the best node for a CPU based on locality
pub fn best_node_for_cpu(cpu_id: usize) -> Result<NodeId, NumaError> {
    cpu_to_node(cpu_id)
}

/// Find the closest node with available memory
pub fn find_closest_with_memory(from_node: NodeId, min_pages: usize) -> Result<NodeId, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let num_nodes = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
    let mut best_node: Option<NodeId> = None;
    let mut best_distance = UNREACHABLE_DISTANCE;

    for i in 0..num_nodes {
        let node_id = NodeId(i);
        if let Ok(node) = get_node(node_id) {
            if node.free_pages() >= min_pages {
                let distance = NUMA_STATE.distances.get(from_node, node_id);
                if distance < best_distance {
                    best_distance = distance;
                    best_node = Some(node_id);
                }
            }
        }
    }

    best_node.ok_or(NumaError::OutOfMemory {
        node_id: from_node.0,
        requested: min_pages,
        available: 0,
    })
}

/// Get statistics for a specific node
pub fn get_node_stats(node_id: NodeId) -> Result<NodeStatsSnapshot, NumaError> {
    let node = get_node(node_id)?;
    Ok(node.stats())
}

/// Get global NUMA statistics
pub fn get_global_stats() -> Result<GlobalNumaStats, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let num_nodes = NUMA_STATE.num_online_nodes.load(Ordering::SeqCst);
    let mut stats = GlobalNumaStats {
        online_nodes: num_nodes,
        total_allocations: 0,
        total_local_allocations: 0,
        total_remote_allocations: 0,
        total_pages: 0,
        total_free_pages: 0,
        total_migrations: 0,
    };

    for i in 0..num_nodes {
        if let Ok(node) = get_node(NodeId(i)) {
            let node_stats = node.stats();
            stats.total_allocations += node_stats.allocations;
            stats.total_local_allocations += node_stats.local_allocations;
            stats.total_remote_allocations += node_stats.remote_allocations;
            stats.total_pages += node_stats.total_pages;
            stats.total_free_pages += node_stats.free_pages;
            stats.total_migrations += node_stats.pages_migrated_in + node_stats.pages_migrated_out;
        }
    }

    Ok(stats)
}

/// Get scheduling hint for a task based on memory locality
pub fn get_scheduling_hint(memory_node: NodeId) -> Result<alloc::vec::Vec<usize>, NumaError> {
    if !is_initialized() {
        return Err(NumaError::NotInitialized);
    }

    let node = get_node(memory_node)?;
    let cpu_mask = node.cpu_mask.load(Ordering::Relaxed);

    let mut cpus = alloc::vec::Vec::new();
    for cpu in 0..64 {
        if (cpu_mask & (1 << cpu)) != 0 {
            cpus.push(cpu);
        }
    }

    Ok(cpus)
}

extern crate alloc;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Test lock to serialize tests that access global state
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_node_id_creation() {
        let node = NodeId::new(0);
        assert_eq!(node.as_usize(), 0);
        assert!(node.is_valid());

        let node = NodeId(MAX_NUMA_NODES);
        assert!(!node.is_valid());
    }

    #[test]
    fn test_node_id_conversions() {
        let node = NodeId::from(5);
        assert_eq!(node.0, 5);

        let value: usize = node.into();
        assert_eq!(value, 5);
    }

    #[test]
    fn test_numa_policy_properties() {
        assert!(NumaPolicy::Local.prefers_local());
        assert!(NumaPolicy::FirstTouch.prefers_local());
        assert!(!NumaPolicy::Interleave.prefers_local());

        let bind = NumaPolicy::Bind(NodeId(1));
        assert_eq!(bind.required_node(), Some(NodeId(1)));
        assert!(!bind.allows_fallback());

        let preferred = NumaPolicy::Preferred(NodeId(2));
        assert!(preferred.allows_fallback());
    }

    #[test]
    fn test_task_affinity_constructors() {
        let local = TaskNumaAffinity::local();
        assert!(local.allow_migration);
        assert!(!local.strict);

        let bind = TaskNumaAffinity::bind(NodeId(1));
        assert!(!bind.allow_migration);
        assert!(bind.strict);
        assert_eq!(bind.preferred_node, Some(NodeId(1)));

        let preferred = TaskNumaAffinity::preferred(NodeId(2));
        assert!(preferred.allow_migration);
        assert!(!preferred.strict);

        let interleave = TaskNumaAffinity::interleave();
        assert_eq!(interleave.policy, NumaPolicy::Interleave);
    }

    #[test]
    fn test_init() {
        let _lock = TEST_LOCK.lock();
        let result = init();
        assert!(result.is_ok());
        assert!(is_initialized());
    }

    #[test]
    fn test_init_with_config() {
        let _lock = TEST_LOCK.lock();
        let result = init_with_config(4, 512);
        assert!(result.is_ok());

        let num = num_nodes().expect("should get num nodes");
        assert_eq!(num, 4);
    }

    #[test]
    fn test_num_nodes() {
        let _lock = TEST_LOCK.lock();
        init().expect("init should succeed");

        let num = num_nodes().expect("should get num nodes");
        assert!(num >= 1);
        assert!(num <= MAX_NUMA_NODES);
    }

    #[test]
    fn test_get_node() {
        let _lock = TEST_LOCK.lock();
        init().expect("init should succeed");

        let node = get_node(NodeId(0)).expect("should get node 0");
        assert!(node.is_online());
        assert_eq!(node.id(), NodeId(0));
    }

    #[test]
    fn test_get_invalid_node() {
        let _lock = TEST_LOCK.lock();
        init().expect("init should succeed");

        let result = get_node(NodeId(MAX_NUMA_NODES + 1));
        assert!(matches!(result, Err(NumaError::InvalidNodeId { .. })));
    }

    #[test]
    fn test_cpu_to_node_mapping() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        // CPUs 0-3 should be on node 0
        let node = cpu_to_node(0).expect("should get node for cpu 0");
        assert_eq!(node, NodeId(0));

        // CPUs 4-7 should be on node 1
        let node = cpu_to_node(4).expect("should get node for cpu 4");
        assert_eq!(node, NodeId(1));
    }

    #[test]
    fn test_distance_matrix() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        // Local distance
        let dist = get_distance(NodeId(0), NodeId(0)).expect("should get distance");
        assert_eq!(dist, LOCAL_DISTANCE);

        // Remote distance
        let dist = get_distance(NodeId(0), NodeId(1)).expect("should get distance");
        assert_eq!(dist, REMOTE_DISTANCE);
    }

    #[test]
    fn test_set_distance() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        set_distance(NodeId(0), NodeId(1), 30).expect("should set distance");
        let dist = get_distance(NodeId(0), NodeId(1)).expect("should get distance");
        assert_eq!(dist, 30);
    }

    #[test]
    fn test_allocate_on_node() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let offset = allocate_on_node(NodeId(0), 10).expect("should allocate");
        assert_eq!(offset, 0);

        let offset2 = allocate_on_node(NodeId(0), 5).expect("should allocate again");
        assert_eq!(offset2, 10);
    }

    #[test]
    fn test_allocate_on_node_out_of_memory() {
        let _lock = TEST_LOCK.lock();
        init_with_config(1, 100).expect("init should succeed");

        let result = allocate_on_node(NodeId(0), 200);
        assert!(matches!(result, Err(NumaError::OutOfMemory { .. })));
    }

    #[test]
    fn test_allocate_with_local_policy() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");
        set_policy(NumaPolicy::Local).expect("should set policy");

        let (node, _offset) = allocate(10, Some(NodeId(0))).expect("should allocate");
        assert_eq!(node, NodeId(0));
    }

    #[test]
    fn test_allocate_with_bind_policy() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let (node, _offset) =
            allocate_with_policy(10, NumaPolicy::Bind(NodeId(1)), None).expect("should allocate");
        assert_eq!(node, NodeId(1));
    }

    #[test]
    fn test_allocate_with_interleave_policy() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        // Reset interleave index
        NUMA_STATE.interleave_index.store(0, Ordering::SeqCst);

        let (node1, _) =
            allocate_with_policy(10, NumaPolicy::Interleave, None).expect("should allocate");
        let (node2, _) =
            allocate_with_policy(10, NumaPolicy::Interleave, None).expect("should allocate");

        // Should interleave between nodes
        assert_ne!(node1, node2);
    }

    #[test]
    fn test_free_on_node() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let offset = allocate_on_node(NodeId(0), 10).expect("should allocate");
        let result = free_on_node(NodeId(0), offset, 10);
        assert!(result.is_ok());

        let stats = get_node_stats(NodeId(0)).expect("should get stats");
        assert_eq!(stats.deallocations, 1);
    }

    #[test]
    fn test_migrate_pages() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let result = migrate_pages(NodeId(0), NodeId(1), 50);
        assert!(result.is_ok());

        let stats0 = get_node_stats(NodeId(0)).expect("should get stats");
        let stats1 = get_node_stats(NodeId(1)).expect("should get stats");

        assert_eq!(stats0.pages_migrated_out, 50);
        assert_eq!(stats1.pages_migrated_in, 50);
    }

    #[test]
    fn test_find_closest_with_memory() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let closest = find_closest_with_memory(NodeId(0), 10).expect("should find node");
        assert_eq!(closest, NodeId(0)); // Local node has memory
    }

    #[test]
    fn test_node_stats_snapshot() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        allocate_on_node(NodeId(0), 50).expect("should allocate");

        let stats = get_node_stats(NodeId(0)).expect("should get stats");
        assert_eq!(stats.allocations, 1);
        assert_eq!(stats.total_pages, 256);
        assert!(stats.free_pages < 256);
        assert!(stats.utilization() > 0.0);
    }

    #[test]
    fn test_global_stats() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        allocate_on_node(NodeId(0), 10).expect("should allocate");
        allocate_on_node(NodeId(1), 20).expect("should allocate");

        let stats = get_global_stats().expect("should get global stats");
        assert_eq!(stats.online_nodes, 2);
        assert_eq!(stats.total_allocations, 2);
        assert_eq!(stats.total_pages, 512);
    }

    #[test]
    fn test_scheduling_hint() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let cpus = get_scheduling_hint(NodeId(0)).expect("should get hint");
        assert!(!cpus.is_empty());
        assert!(cpus.contains(&0));
    }

    #[test]
    fn test_error_display() {
        let errors = [
            NumaError::NotInitialized,
            NumaError::AlreadyInitialized,
            NumaError::InvalidNodeId {
                node_id: 10,
                max_node_id: 7,
            },
            NumaError::NodeOffline { node_id: 5 },
            NumaError::OutOfMemory {
                node_id: 0,
                requested: 100,
                available: 50,
            },
            NumaError::InvalidCpuId {
                cpu_id: 256,
                max_cpu_id: 255,
            },
            NumaError::CpuNotMapped { cpu_id: 100 },
            NumaError::InvalidAllocation,
            NumaError::TopologyNotSupported,
            NumaError::CrossNodeAccess {
                source_node: 0,
                target_node: 1,
            },
        ];

        for error in &errors {
            let _ = alloc::format!("{}", error);
        }
    }

    #[test]
    fn test_zero_allocation_fails() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let result = allocate_on_node(NodeId(0), 0);
        assert!(matches!(result, Err(NumaError::InvalidAllocation)));
    }

    #[test]
    fn test_best_node_for_cpu() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let node = best_node_for_cpu(0).expect("should get best node");
        assert_eq!(node, NodeId(0));

        let node = best_node_for_cpu(5).expect("should get best node");
        assert_eq!(node, NodeId(1));
    }

    #[test]
    fn test_node_has_cpu() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        let node = get_node(NodeId(0)).expect("should get node");
        assert!(node.has_cpu(0));
        assert!(node.has_cpu(1));
        assert!(!node.has_cpu(5));
    }

    #[test]
    fn test_stats_local_access_ratio() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        // Make some local and remote allocations
        allocate_on_node_with_locality(NodeId(0), 10, true).expect("local alloc");
        allocate_on_node_with_locality(NodeId(0), 10, false).expect("remote alloc");

        let stats = get_node_stats(NodeId(0)).expect("should get stats");
        assert!(stats.local_access_ratio() > 0.0);
        assert!(stats.local_access_ratio() < 1.0);
    }

    #[test]
    fn test_preferred_policy_with_fallback() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 50).expect("init should succeed");

        // Fill up node 0
        allocate_on_node(NodeId(0), 50).expect("should allocate all on node 0");

        // Now try preferred policy - should fall back to node 1
        let result = allocate_with_policy(10, NumaPolicy::Preferred(NodeId(0)), None);
        assert!(result.is_ok());
        let (node, _) = result.expect("should succeed with fallback");
        assert_eq!(node, NodeId(1));
    }

    #[test]
    fn test_migrate_same_node() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        // Migrating to same node should be a no-op
        let result = migrate_pages(NodeId(0), NodeId(0), 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_net_migration() {
        let _lock = TEST_LOCK.lock();
        init_with_config(2, 256).expect("init should succeed");

        migrate_pages(NodeId(0), NodeId(1), 100).expect("migrate out");
        migrate_pages(NodeId(1), NodeId(0), 30).expect("migrate in");

        let stats = get_node_stats(NodeId(0)).expect("should get stats");
        assert_eq!(stats.net_migration(), 30 - 100);
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    extern crate alloc;
    use alloc::vec::Vec;

    static STRESS_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_stress_many_allocations() {
        let _lock = STRESS_LOCK.lock();
        init_with_config(4, 1000).expect("init should succeed");

        let mut allocations: Vec<(NodeId, usize, usize)> = Vec::new();

        // Perform many allocations across all nodes
        for i in 0..100 {
            let node = NodeId(i % 4);
            let pages = 1 + (i % 10);
            if let Ok(offset) = allocate_on_node(node, pages) {
                allocations.push((node, offset, pages));
            }
        }

        assert!(
            allocations.len() > 50,
            "Should have many successful allocations"
        );

        // Free all
        for (node, offset, pages) in allocations {
            free_on_node(node, offset, pages).expect("should free");
        }
    }

    #[test]
    fn test_stress_interleave_distribution() {
        let _lock = STRESS_LOCK.lock();
        init_with_config(4, 500).expect("init should succeed");
        NUMA_STATE.interleave_index.store(0, Ordering::SeqCst);

        let mut node_counts = [0usize; 4];

        for _ in 0..100 {
            if let Ok((node, _)) = allocate_with_policy(1, NumaPolicy::Interleave, None) {
                node_counts[node.0] += 1;
            }
        }

        // All nodes should have received some allocations
        for (i, &count) in node_counts.iter().enumerate() {
            assert!(count > 0, "Node {} should have allocations", i);
        }
    }

    #[test]
    fn test_stress_migration_tracking() {
        let _lock = STRESS_LOCK.lock();
        init_with_config(2, 1000).expect("init should succeed");

        // Perform many migrations
        for _ in 0..100 {
            migrate_pages(NodeId(0), NodeId(1), 5).expect("should migrate");
            migrate_pages(NodeId(1), NodeId(0), 3).expect("should migrate");
        }

        let stats0 = get_node_stats(NodeId(0)).expect("should get stats");
        let stats1 = get_node_stats(NodeId(1)).expect("should get stats");

        assert_eq!(stats0.pages_migrated_out, 500);
        assert_eq!(stats0.pages_migrated_in, 300);
        assert_eq!(stats1.pages_migrated_out, 300);
        assert_eq!(stats1.pages_migrated_in, 500);
    }
}
