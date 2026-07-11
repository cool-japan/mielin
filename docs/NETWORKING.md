# MielinMesh Networking Architecture & Gossip Protocol Guide

This document describes the networking stack implemented in `mielin-mesh/core` — the
distributed hash table, gossip/membership protocol, peer discovery, partition
handling, and multi-region/multi-tenancy/load-balancing layers that together form
Layer 3 ("The Synapse") of MielinOS. Every type, field, default, and algorithm
described below is taken directly from the source in `mielin-mesh/core/src/`
(primarily `dht.rs`, `gossip.rs`, `gossip_tests.rs`, `routing.rs`, `node.rs`,
`registry.rs`, `discovery.rs`, `discovery_static.rs`, `discovery_dns.rs`,
`discovery_aggregator.rs`, `service_discovery.rs`, `partition.rs`, `recovery.rs`,
`multiregion.rs`, `multitenancy.rs`, `loadbalancer.rs`, `metrics/`, `export.rs`,
`tracing.rs`, and `lib.rs`). Where the running code diverges from its own doc
comments, from `mielin-mesh/README.md`, or from what its name implies, this guide
calls it out explicitly under an **Implementation note** so the document stays
trustworthy as a reference rather than aspirational marketing copy.

## Table of Contents

1. [Networking Architecture Overview](#1-networking-architecture-overview)
2. [Distributed Hash Table (Kademlia-style)](#2-distributed-hash-table-kademlia-style)
3. [Gossip Protocol](#3-gossip-protocol)
4. [Peer Discovery](#4-peer-discovery)
5. [Partition Detection & Recovery](#5-partition-detection--recovery)
6. [Multi-Region, Multi-Tenancy & Load Balancing](#6-multi-region-multi-tenancy--load-balancing)
7. [Membership History & Observability](#7-membership-history--observability)
8. [Tuning Pointers](#8-tuning-pointers)

---

## 1. Networking Architecture Overview

`mielin-mesh-core`'s crate doc (`lib.rs`) states its purpose plainly:

> "MielinMesh Core - Distributed Hash Table and Routing. Implements
> Kademlia-based DHT with geographic and performance awareness."

The crate is organized as a set of cooperating, mostly independent modules rather
than a single monolithic "network stack" object. `lib.rs` declares 22 public
modules (`dht`, `discovery`, `discovery_aggregator`, `discovery_dns`,
`discovery_static`, `error`, `export`, `gossip`, `loadbalancer`, `metrics`,
`migration`, `multiregion`, `multitenancy`, `node`, `partition`, `recovery`,
`registry`, `routing`, `security`, `service`, `service_discovery`, `shutdown`,
`timeout`, `tracing`, `version`) and re-exports their key types at the crate
root. Conceptually the modules compose into four layers:

```
┌───────────────────────────────────────────────────────────────────────┐
│  Peer Discovery         discovery.rs (mDNS), discovery_static.rs,     │
│                         discovery_dns.rs, discovery_aggregator.rs,    │
│                         service_discovery.rs (service catalog)        │
├───────────────────────────────────────────────────────────────────────┤
│  DHT & Agent Registry   dht.rs (Dht, PeerInfo, LookupCache,           │
│                         LookupState) + registry.rs (AgentRegistry,    │
│                         ShardedRegistry) — location tracking on top   │
│                         of the DHT's routing table                    │
├───────────────────────────────────────────────────────────────────────┤
│  Membership & Gossip    gossip.rs (GossipState — SWIM-inspired flat   │
│                         gossip; HierarchicalGossip — zones/super-peer │
│                         overlay) drives the alive/suspect/dead view   │
│                         consumed by partition.rs and recovery.rs      │
├───────────────────────────────────────────────────────────────────────┤
│  Region/Tenant/LB       multiregion.rs, multitenancy.rs,              │
│                         loadbalancer.rs — route service-mesh traffic  │
│                         (ServiceRegistration/ServiceEndpoint) across  │
│                         regions and tenant namespaces                 │
└───────────────────────────────────────────────────────────────────────┘
```

Discovery produces candidate peers; the DHT stores them in a routing table keyed
by XOR distance and provides lookup/caching over that table; gossip layers a
membership view (alive/suspect/dead, generic key/value state) and, at scale, a
zone/super-peer hierarchy on top of whatever peer set discovery and the DHT have
assembled; `partition.rs`/`recovery.rs` watch gossip's visible/known node sets to
detect quorum loss and drive reconnection backoff; and `multiregion.rs` /
`multitenancy.rs` / `loadbalancer.rs` operate one layer up, routing *service*
traffic (via `service_discovery.rs`'s `ServiceRegistration` catalog) across
regions and tenant namespaces rather than routing raw mesh peers.

**Implementation note — `routing.rs` is currently a stub.** Despite the crate doc
comment's "geographic and performance awareness" and the module doc "Routing
logic with geographic awareness", `routing.rs` in its entirety is:

```rust
//! Routing logic with geographic awareness

pub struct RoutingTable {}

impl RoutingTable {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}
```

`RoutingTable` carries no fields and is not used by `Dht`, `GossipState`, or any
other module in this crate. The routing behavior the crate doc alludes to is
actually implemented piecemeal elsewhere:
- **XOR-distance greedy routing** and **latency-aware peer selection** live in
  `dht.rs` (`Dht::route_to`, `Dht::find_closest_by_latency`,
  `Dht::get_peers_by_latency`).
- **Geographic ("performance-aware") routing** lives in `multiregion.rs`
  (`RegionTopology::find_closest_region`, `RegionTopology::select_region`), using
  Haversine great-circle distance between `GeoLocation` points — see
  [Section 6](#6-multi-region-multi-tenancy--load-balancing).

The `mielin-mesh/README.md` also sketches an aspirational multi-metric
`NodeMetrics { xor_distance, latency_ms, geographic_km, hardware_match,
energy_score, load_score }` struct with a weighted `routing_score()` function;
this struct does not exist anywhere in `dht.rs` or `routing.rs` today — the only
metric the DHT actually tracks per peer is `latency_ms` (see `PeerInfo` below).

---

## 2. Distributed Hash Table (Kademlia-style)

Source: `mielin-mesh/core/src/dht.rs` (module doc: "Provides a Kademlia-style DHT
with: XOR distance-based routing; Iterative lookup with parallelism; Result
caching with TTL; Churn handling optimization; Latency-aware peer selection").

### 2.1 Key space and node identity

`NodeId` is defined in `node.rs` as a 128-bit UUID:

```rust
pub type NodeId = Uuid;

pub enum NodeRole { Edge, Relay, Core }

pub struct Node { id: NodeId, role: NodeRole }
```

**Implementation note:** `mielin-mesh/README.md` describes "160-bit node IDs" in
its Kademlia feature list. The actual `NodeId` type is a `uuid::Uuid`, i.e. 128
bits (16 bytes), and `xor_distance` in `dht.rs` folds exactly those 16 bytes into
a `u128`:

```rust
fn xor_distance(a: &NodeId, b: &NodeId) -> u128 {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut result = 0u128;
    for i in 0..16 {
        result = (result << 8) | ((a_bytes[i] ^ b_bytes[i]) as u128);
    }
    result
}
```

### 2.2 Routing table

```rust
pub struct Dht {
    local_id: NodeId,
    routing_table: HashMap<NodeId, PeerInfo>,
    lookup_cache: LookupCache,
    churn_tracker: ChurnTracker,
}

pub struct PeerInfo {
    pub node_id: NodeId,
    pub address: String,
    pub last_seen: Instant,
    pub latency_ms: Option<u32>,
    pub reliability_score: u8,   // defaults to 100 in PeerInfo::new(); not
                                  // consumed by any selection/eviction logic
                                  // in this file today
}
```

Key constants:

| Constant | Value | Role |
|---|---|---|
| `K_BUCKET_SIZE` | 20 | Default `k` for `find_closest`, and the default `result_count` in `LookupConfig` |
| `PEER_TIMEOUT` | 300 s | `PeerInfo::is_alive()` liveness cutoff |
| `DEFAULT_CACHE_TTL` | 300 s | Default lookup-cache TTL |
| `DEFAULT_LOOKUP_PARALLELISM` | 3 | Default iterative-lookup "alpha" |
| `MAX_LOOKUP_ITERATIONS` | 20 | Iterative-lookup iteration ceiling |
| `CHURN_WINDOW` | 60 s | Sliding window for churn-rate calculation |
| `DEFAULT_LOOKUP_CACHE_SIZE` | 1000 | Default `LookupCache` capacity |

**Implementation note:** despite the `K_BUCKET_SIZE` name and the "Kademlia-style"
module doc, `Dht.routing_table` is a single flat `HashMap<NodeId, PeerInfo>`, not
a tree of 128 (or 160) individual k-buckets partitioned by shared-prefix length.
`K_BUCKET_SIZE` is used only as the default *result count* (`k`) returned by
`find_closest`/`find_closest_uncached`, and eviction is driven purely by
`PeerInfo::is_alive()` (`last_seen.elapsed() < PEER_TIMEOUT`) via
`evict_old_peers()`, not by per-bucket LRU replacement as in classic Kademlia.

`insert_peer()` rejects self-insertion, evicts dead peers on every insert, and —
if the peer is new — records a churn arrival and invalidates any cached lookup
result that contained that node ID (`LookupCache::invalidate_containing`).
`remove_peer()` mirrors this on the departure side.

### 2.3 Lookup: caching, adaptive TTL, and iterative state machine

`find_closest(target, k)` (mutable, may populate the cache) and
`find_closest_fresh(target, k)` (immutable, bypasses the cache) both sort the
entire routing table by XOR distance to `target` and truncate to `k`. Caching
only applies when `k == K_BUCKET_SIZE`:

```rust
pub fn find_closest(&mut self, target: &NodeId, k: usize) -> Vec<NodeId> {
    if k == K_BUCKET_SIZE {
        if let Some(cached) = self.lookup_cache.get(target) {
            return cached;
        }
    }
    let result = self.find_closest_uncached(target, k);
    if k == K_BUCKET_SIZE && !result.is_empty() {
        let ttl = self.churn_tracker.recommended_cache_ttl();
        self.lookup_cache.insert_with_ttl(*target, result.clone(), ttl);
    }
    result
}
```

`LookupCache` tracks hits/misses/evictions/insertions via `CacheStats` (atomic
counters, exposing `hit_rate()`), evicts the least-recently-cached entry
(`evict_lru`, keyed by `cached_at`, **not** by last-access time) when at
capacity, and lazily purges expired entries on every `get()`.

`ChurnTracker` records peer arrivals/departures in a 60-second window and derives
an **adaptive cache TTL** that shortens as churn increases —
`recommended_cache_ttl()`:

| Churn rate (events/min) | Recommended TTL |
|---|---|
| `< 1.0` | 600 s (10 min) |
| `< 5.0` | 300 s (5 min) |
| `< 10.0` | 60 s (1 min) |
| `>= 10.0` | 15 s |

For true iterative lookups (multi-round, parallel querying of unresponsive
peers), `LookupConfig`/`LookupState` implement the pure algorithm — network I/O
is left to the caller (the wire transport layer), so `dht.rs` never performs any
network calls itself:

```rust
pub struct LookupConfig {
    pub parallelism: usize,      // alpha; default 3
    pub result_count: usize,     // k; default K_BUCKET_SIZE (20)
    pub max_iterations: usize,   // default 20
    pub use_cache: bool,         // default true
    pub query_timeout: Duration, // default 5 s
}
```

`LookupConfig::fast()` raises `parallelism` to 5 and drops `max_iterations` to
10 with a 2 s `query_timeout`; `LookupConfig::thorough()` doubles
`result_count` to `2*K_BUCKET_SIZE`, disables caching, and extends
`query_timeout` to 10 s.

`LookupState::new(target, initial_nodes, config)` seeds a `pending` queue sorted
by distance to `target`. `next_batch()` pulls up to `parallelism` unqueried nodes;
`process_responses(responses)` merges newly-returned peers into `pending` in
sorted position, recomputes `results` (closest `result_count` nodes seen so far
across `queried ∪ pending`), and marks the lookup `complete` when either
`pending` is empty, `iteration >= max_iterations`, or the search has converged
(the best pending candidate is no closer than the current best result and
`results.len() >= result_count`).

### 2.4 Latency-aware selection and routing

```rust
pub fn find_closest_by_latency(&self, k: usize) -> Vec<NodeId>   // sorted ascending by latency_ms
pub fn get_peers_by_latency(&self) -> Vec<&PeerInfo>             // all alive peers with known latency, sorted
pub fn update_peer_latency(&mut self, node_id: &NodeId, latency_ms: u32)
pub fn route_to(&self, target: &NodeId) -> Option<&PeerInfo>
```

`route_to` is single-hop greedy routing: if `target` is directly known, return
it; otherwise return the alive peer with the smallest XOR distance to `target`.
This is the DHT's only "routing" primitive — there is no separate multi-hop
store-and-forward routing table (see the `routing.rs` note in
[Section 1](#1-networking-architecture-overview)).

`RoutingReplica` provides a size-bounded (`max_entries`) backup copy of the
routing table (`sync_from(primary)`, trimming the oldest-`last_seen` entries
first) intended for resilience if the primary table is lost.

### 2.5 Distributed agent registry (a DHT consumer)

`registry.rs` ("Distributed Agent Registry — Provides DHT-based agent location
tracking and discovery across the mesh") builds on `dht.rs`:

```rust
pub type AgentId = [u8; 16];

const REGISTRY_TTL: Duration = Duration::from_secs(600);   // 10 min
const REPLICATION_FACTOR: usize = 3;
const DEFAULT_SHARD_COUNT: usize = 16;
const DEFAULT_PAGE_SIZE: usize = 100;
const HOT_KEY_THRESHOLD: u64 = 100;   // queries/minute
```

`AgentRegistry::get_responsible_nodes(agent_id)` converts the 16-byte
`AgentId` directly into a `NodeId` (`Uuid::from_bytes`) and calls
`dht.find_closest_fresh(&target_id, REPLICATION_FACTOR)` — i.e. the DHT's XOR
key space is reused for agent placement, exactly analogous to Kademlia-based
content-addressable storage. `AgentRegistry` itself only maintains local
(`local_agents`) and remote-cache (`remote_cache`) maps in this crate; the
`RegistryMessage` enum (`Register`, `Update`, `Query`/`QueryResponse`,
`Deregister`, `Replicate`) defines the wire-level protocol for propagating
registrations, but `register_agent()`'s doc comment notes that DHT replication
to peer nodes is not yet wired up ("In a real implementation, this would
replicate to peer nodes via DHT for fault tolerance").

`registry.rs` additionally provides a separate, self-contained
**`ShardedRegistry`** for higher-throughput deployments — agents are bucketed
by an FNV-1a hash of the `AgentId` into `ShardConfig::shard_count` shards (each
behind its own `RwLock<RegistryShard>>`), with `ShardConfig::small()` (4
shards / 1000 entries) and `ShardConfig::large()` (64 shards / 50 000 entries)
presets alongside the `Default` (16 shards / 10 000 entries). `AccessTracker` +
`HotSpotMitigator` implement hot-key detection (default threshold 100
queries/60 s window) and replica-set bookkeeping for hot agents, and
`ReplicationManager`/`ReplicationTask` implement a simple retry-bounded
(3 attempts) background replication queue — independent of, and simpler than,
the region-level `ReplicationManager` in `multiregion.rs` (see
[Section 6](#6-multi-region-multi-tenancy--load-balancing); the two types share
a name but live in different modules).

---

## 3. Gossip Protocol

Source: `mielin-mesh/core/src/gossip.rs`. The module doc states its scope
directly:

> "Implements a SWIM-inspired gossip protocol for: Node membership management;
> Failure detection via heartbeats; State dissemination across the mesh;
> Anti-entropy reconciliation; Hierarchical gossip with zones and super-peers"

### 3.1 `GossipConfig` — the tunable knobs

```rust
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// How often gossip messages are sent (default: 5 s)
    pub gossip_interval: Duration,
    /// After this duration without heartbeat, mark suspect (default: 15 s)
    pub heartbeat_timeout: Duration,
    /// After this duration without heartbeat, declare failed (default: 30 s)
    pub failure_timeout: Duration,
    /// Number of nodes to gossip to per round (default: 3)
    pub fanout: usize,
    /// Maximum number of membership event entries to retain (default: 512)
    pub max_history: usize,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            gossip_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(15),
            failure_timeout: Duration::from_secs(30),
            fanout: 3,
            max_history: 512,
        }
    }
}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `gossip_interval` | `Duration` | 5 s | Tick period for the heartbeat task and the (currently log-only, see below) gossip-propagation task |
| `heartbeat_timeout` | `Duration` | 15 s | Age after which an `Alive` member is marked `Suspect` |
| `failure_timeout` | `Duration` | 30 s | Age after which a `Suspect` member is marked `Dead` |
| `fanout` | `usize` | 3 | Target peer count per gossip round (SWIM-style random subset) |
| `max_history` | `usize` | 512 | Capacity of the `MembershipEvent` ring buffer |

`GossipState::new(node)` uses `GossipConfig::default()`; `GossipState::with_config(node,
config)` accepts a caller-supplied config. `gossip.config()` exposes the active
configuration for inspection.

**Implementation note — two independent code paths reference heartbeat/failure
timing, and only one of them honors `GossipConfig`.** The convenience methods on
`MemberInfo` still read the module-level constants directly, not the runtime
config:

```rust
pub fn should_suspect(&self) -> bool {
    self.is_alive() && self.heartbeat_age() > HEARTBEAT_TIMEOUT   // hard-coded 15 s
}
pub fn should_declare_dead(&self) -> bool {
    self.is_suspect() && self.heartbeat_age() > FAILURE_TIMEOUT   // hard-coded 30 s
}
```

whereas the actual background detection loop (`spawn_failure_detection_task`)
correctly reads `self.config.heartbeat_timeout` / `self.config.failure_timeout`
each time it is spawned. A `GossipState` built with a custom `GossipConfig` will
therefore behave correctly for the live failure-detection task, but any code
calling `member.should_suspect()` / `member.should_declare_dead()` directly
(as several unit tests in `gossip_tests.rs` do) will still be evaluated against
the original 15 s / 30 s constants regardless of the configured values.

**Implementation note — the failure-detection task's own tick interval is fixed,
not configurable.** `spawn_failure_detection_task` hard-codes its polling
interval to `Duration::from_secs(5)` rather than reading `config.gossip_interval`
or any dedicated field; only the *thresholds* it compares against
(`heartbeat_timeout`, `failure_timeout`) come from `GossipConfig`.

### 3.2 Membership state and health status

```rust
pub enum HealthStatus { Alive, Suspect, Dead }

pub struct MemberInfo {
    pub node_id: NodeId,
    pub status: HealthStatus,
    pub incarnation: u64,
    pub last_seen: SystemTime,
    pub metadata: HashMap<String, String>,
}
```

`GossipState` holds:

```rust
pub struct GossipState {
    local_node: Arc<Node>,
    members: Arc<RwLock<HashMap<NodeId, MemberInfo>>>,
    local_incarnation: Arc<RwLock<u64>>,
    state_store: Arc<RwLock<HashMap<String, (Vec<u8>, u64)>>>,  // key -> (value, version)
    config: Arc<GossipConfig>,
    history: Arc<RwLock<VecDeque<MembershipEvent>>>,
}
```

`GossipState::new`/`with_config` seed `members` with a single `Alive` entry for
the local node. `start()` spawns three background `tokio` tasks:

1. **Heartbeat task** — every `gossip_interval`, refreshes the local member's
   `last_seen` and stamps its current `local_incarnation`.
2. **Failure-detection task** — every fixed 5 s (see note above), scans all
   non-local members: `Alive` members whose heartbeat age exceeds
   `heartbeat_timeout` become `Suspect`; `Suspect` members whose age exceeds
   `failure_timeout` become `Dead`. Each transition appends a
   `MembershipEvent` (`Failed` for the dead transition, `StatusChanged{from,to}`
   otherwise).
3. **Gossip-propagation task** — every `gossip_interval`, logs alive/suspect/dead
   counts. Its body contains an explicit placeholder:

   ```rust
   // In a real implementation, this would:
   // 1. Select random peers to gossip with (up to _fanout)
   // 2. Send member updates
   // 3. Exchange state information
   ```

   i.e. the self-scheduled, `fanout`-driven epidemic push loop is **not yet
   wired to any network transmission** in `mielin-mesh-core` (`fanout` is
   captured into a variable literally named `_fanout` and never read again).
   Real state exchange currently happens only in response to explicitly
   received `GossipMessage`s via `handle_message()` (below); the wire crate is
   expected to drive that exchange once transport integration lands (consistent
   with the README marking Gossip Protocol as a "Phase 2" feature).

### 3.3 Gossip messages and anti-entropy

```rust
pub enum GossipMessage {
    Heartbeat { node_id: NodeId, incarnation: u64 },
    MemberUpdate { member: MemberInfo },
    SyncRequest { from_node: NodeId },
    SyncResponse { members: Vec<MemberInfo> },
    StateUpdate { key: String, value: Vec<u8>, version: u64 },
}
```

`handle_message()` dispatches each variant:

- **`Heartbeat`** — if the sender is unknown, it is added as a new `Alive`
  member (recording a `Joined` event); if known and the incoming `incarnation`
  is newer, `last_seen`/`incarnation`/`status(→Alive)` are refreshed (recording
  `Recovered` if the member had been unhealthy, or `IncarnationUpdated`
  otherwise). Stale/duplicate incarnations are ignored.
- **`MemberUpdate`** — last-writer-wins by `incarnation`: an incoming record only
  overwrites the local one if its `incarnation` is strictly greater.
- **`SyncRequest` / `SyncResponse`** — this is the protocol's **anti-entropy
  reconciliation** mechanism: a `SyncRequest` reply carries the entire local
  membership table (`handle_sync_request`), and a `SyncResponse` merges each
  incoming `MemberInfo` into the local table by keeping whichever side has the
  higher `incarnation` (`handle_sync_response`) — a full pull-based
  reconciliation pass rather than a delta/digest exchange.
- **`StateUpdate`** — generic last-writer-wins key/value replication for the
  `state_store` (`publish_state(key, value)` auto-increments the version;
  `handle_state_update` only applies an incoming value if its `version` is
  strictly greater than what is stored).

`increment_incarnation()` implements the SWIM self-refutation pattern: a node
that discovers it has been marked `Suspect` bumps its own incarnation so that a
subsequent `Heartbeat`/`MemberUpdate` from it is accepted as newer and clears
the suspicion.

Membership mutation helpers — `add_member`, `remove_member`,
`update_member_status`, `get_alive_members`, `get_all_members`,
`get_member_stats() -> (alive, suspect, dead)` — all record a corresponding
`MembershipEvent` (see [Section 7](#7-membership-history--observability)).

### 3.4 Hierarchical gossip: zones and super-peers

For larger meshes, `HierarchicalGossip` partitions membership into zones and
elects a bounded set of **super-peers** per zone to relay inter-zone traffic,
avoiding O(n²) full-mesh gossip:

```rust
pub struct ZoneId(pub u64);

impl ZoneId {
    pub fn from_node_id(node_id: &NodeId, num_zones: u64) -> Self {
        let hash = node_id.as_bytes().iter()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));
        Self(hash % num_zones)
    }
}

pub enum GossipRole { Regular, SuperPeer, ZoneLeader }

pub struct HierarchicalGossipConfig {
    pub num_zones: u64,               // default 4
    pub super_peers_per_zone: usize,  // default 3
    pub intra_zone_interval: Duration,// default 2 s
    pub inter_zone_interval: Duration,// default 10 s
    pub max_inter_zone_ttl: u8,       // default 4
    pub fanout: usize,                // default 3
}
```

**Implementation note:** `super_peers_per_zone` is stored but never actually
enforced as a cap — no code path in `promote_super_peer` checks how many
super-peers a zone already has before promoting another winner, so in principle
more than `super_peers_per_zone` nodes could hold the `SuperPeer` role
simultaneously.

`ZoneId::from_node_id` assigns a node to a zone via a simple polynomial rolling
hash of its `NodeId` bytes mod `num_zones` — this is a coarse, non-cryptographic
partition, distinct from the SipHash-based `ConsistentHashRing` used elsewhere
for state rebalancing (see [Section 5](#5-partition-detection--recovery)).

`HierarchicalMessage` carries five message shapes: `IntraZone` (delivered only
if the zone matches), `InterZone { source_zone, target_zone, payload, ttl }`
(forwarded by `SuperPeer`/`ZoneLeader` nodes toward the target zone with `ttl`
decremented each hop, dropped once `ttl` hits 0), `ZoneAnnounce`, `ZoneStats`,
and the election pair below.

#### Super-peer term election (vote → majority → promotion)

Election is a lightweight, Raft-flavored term/vote scheme (not leader
heartbeats or log replication — just single-round term voting):

```rust
pub async fn start_election(&self) -> Vec<(NodeId, HierarchicalMessage)> {
    // increments election_state.0 (term), sets election_state.1 = Some(self),
    // and returns a SuperPeerElection { zone_id, candidate: self, term }
    // addressed to every other node currently known in the local zone
}
```

Each recipient calls `vote_for_super_peer(candidate, term)`:

```rust
async fn vote_for_super_peer(&self, candidate: NodeId, term: u64) -> bool {
    let mut state = self.election_state.write().await;
    if term > state.0 {
        *state = (term, Some(candidate));      // adopt newer term, vote yes
        true
    } else if term == state.0 && state.1.is_none() {
        state.1 = Some(candidate);              // first vote this term
        true
    } else {
        false                                    // already voted this term
    }
}
```

— i.e. a node grants at most one vote per term, and immediately adopts any
higher term it observes (matching `test_hierarchical_vote_only_once_per_term`
in `gossip_tests.rs`). A granted vote is returned as a `SuperPeerVote { zone_id,
voter, candidate, term, granted }` reply.

The candidate tallies incoming votes with **`record_vote(term, voter_id,
candidate_id)`**:

```rust
pub async fn record_vote(&self, term: u64, voter_id: NodeId, candidate_id: NodeId) {
    // 1. Insert the vote into votes_received[term][voter_id] = candidate_id.
    // 2. Tally votes-per-candidate for this term under the votes lock, using
    //    term_votes.len().max(1) as a *lower-bound* denominator (the code
    //    comment explains this avoids taking the zones lock while already
    //    holding the votes lock, to prevent lock-order inversion).
    // 3. If any candidate clears floor(lower_bound/2) + 1, re-check with the
    //    *authoritative* zone membership count (all node IDs across all
    //    zones), and only then call promote_super_peer(term, candidate) if
    //    the true majority (total_members / 2 + 1) is actually met.
}
```

`promote_super_peer(term, candidate)` sets that node's `ZoneMember.role` to
`GossipRole::SuperPeer` in whichever zone contains it, registers it into the
`super_peers: HashMap<ZoneId, HashSet<NodeId>>` index used for inter-zone
routing, and records `election_state = (term, Some(candidate))`. This two-phase
check (optimistic tally under the votes lock, using vote-count as a lower bound
on cluster size, followed by an authoritative re-check against real zone
membership before promoting) is the exact mechanism validated by
`test_election_majority_promotes_winner` and `test_election_no_majority_no_promotion`
in `gossip_tests.rs`.

Once elected, super-peers/zone-leaders are the only roles permitted to forward
`InterZone` messages (`find_route_to_zone`, preferring direct super-peers in the
target zone, falling back to any super-peer in a third zone) or to
`broadcast_inter_zone()` a payload to one super-peer in every other known zone.
`select_gossip_peers(count)` (intra-zone fanout target selection) uses a
wrapping-hash-of-`SystemTime::now()`-nanoseconds formula rather than a proper
PRNG — adequate for spreading load across ticks, but not a cryptographically or
statistically rigorous random sample.

---

## 4. Peer Discovery

Four discovery backends exist, plus a separate service-catalog registry that is
easy to confuse with them by name.

### 4.1 mDNS local discovery — `discovery.rs`

`DiscoveryService` wraps the `mdns_sd` crate. It registers a service instance
named `mielin-{node_id}` under `_mielin._udp.local.`, with TXT properties
`node_id`, `role` (`Debug`-formatted `NodeRole`, i.e. exactly `"Edge"`,
`"Relay"`, or `"Core"`), and `version` (`CARGO_PKG_VERSION`); it simultaneously
browses the same service type. On `ServiceEvent::ServiceResolved`, it parses
those TXT properties back into a `DiscoveredPeer`:

```rust
pub struct DiscoveredPeer {
    pub node_id: NodeId,
    pub role: NodeRole,
    pub address: SocketAddr,
    pub capabilities: Vec<String>,   // always empty via the mDNS path today —
                                      // never populated from TXT records
    pub discovered_at: SystemTime,
}
```

filtering out the local node by ID and rejecting any `role` string outside the
three known variants (`DiscoveryError::InvalidPeerData`). A background task
sweeps `peers` every 60 s, dropping any entry whose `discovered_at` is older
than `PEER_CACHE_TTL` (300 s).

```rust
pub struct BootstrapNode { pub address: SocketAddr, pub public_key: Option<Vec<u8>> }
```

**Implementation note:** `DiscoveryService::connect_bootstrap()` and
`exchange_peers()` are present in the public API but are currently stubs — both
iterate their inputs, log, and return `Ok(Vec::new())` without opening any
connection. Their doc comments state this explicitly ("Full implementation
would... This will be implemented when we create the full mesh service that
coordinates discovery + transport + DHT" / "Implementation deferred to mesh
service integration").

### 4.2 Static peer list — `discovery_static.rs`

For environments where mDNS is unavailable or undesired, `StaticPeerList`
provides deterministic, configuration-driven peer discovery with live health
tracking:

```rust
pub struct StaticPeer {
    pub node_id: NodeId,
    pub address: SocketAddr,
    pub role: NodeRole,
    pub tags: Vec<String>,   // e.g. "edge", "us-east-1"
    pub weight: u32,         // load-balancing weight; higher = more selection probability (default 1)
    pub enabled: bool,
}

pub enum PeerHealth {
    Unknown,
    Reachable { last_seen: Instant, latency_ms: Option<u64> },
    Unreachable { last_attempt: Instant, failure_count: u32 },
    Disabled,
}
```

`is_healthy()` treats both `Reachable` and `Unknown` as healthy (benefit of the
doubt for peers never yet probed). `select_peer(seed)` performs weighted
roulette-wheel selection over the currently healthy peers using a small,
dependency-free Xorshift64 PRNG seeded by the caller (`x ^= x<<13; x ^= x>>7;
x ^= x<<17`), falling back to uniform selection if the total weight is zero.
`prune_unreachable()` removes peers whose `failure_count` exceeds
`max_failures_before_remove` (default **5**). `StaticPeerList::new()` defaults
`refresh_interval` to **30 s** (informational — the caller decides when to
actually re-poll).

### 4.3 DNS SRV discovery (RFC 2782) — `discovery_dns.rs`

`DnsSrvDiscovery`'s module doc is explicit about its scope:

> "Models the DNS SRV record protocol semantics for service discovery. This
> abstraction layer does not depend on any async DNS resolver — instead it
> maintains an injected record cache that production code would populate via a
> real resolver (e.g. `trust-dns-resolver`), while tests drive it through
> `inject_records`."

```rust
pub struct DnsSrvRecord {
    pub priority: u16,   // lower = higher preference (RFC 2782)
    pub weight: u16,     // tie-break within a priority tier
    pub port: u16,
    pub target: String,  // FQDN, no trailing dot
}

pub struct DnsSrvConfig {
    pub service_name: String,               // e.g. "_mielin._tcp"
    pub domain: String,                      // e.g. "example.com"
    pub resolver_addr: Option<SocketAddr>,
    pub ttl: Duration,                       // default 60 s
    pub max_records: usize,                  // default 10 (0 = unlimited)
}
```

Selection follows RFC 2782's two-step semantics exactly: group records by
`priority`, take the lowest-numbered tier, and weighted-randomly pick within
that tier (again via the seeded Xorshift64 PRNG; uniform fallback when the
tier's total weight is 0):

```rust
pub async fn select_peer(&self, seed: u64) -> Result<DnsSrvRecord, DnsDiscoveryError> {
    // sorted_peers() sorts ascending by `priority`;
    // tier = records with priority == records[0].priority;
    // weighted draw in [0, sum(tier weights)) selects within the tier.
}
```

**Implementation note:** `do_refresh()` — the function that would normally issue
a real DNS query — is currently a mock: if the cache already holds any entry
(even an expired one) it simply re-stamps `fetched_at = Instant::now()` and
reports success; if the cache is empty it returns `NoRecords`. No actual DNS
resolution occurs in `discovery_dns.rs` today; `inject_records()` is the only
way records enter the cache outside of this mock refresh.

### 4.4 Discovery aggregation and dedup — `discovery_aggregator.rs`

`DiscoveryAggregator` merges `StaticPeerList` and `DnsSrvDiscovery` results into
one deduplicated stream (its module doc notes mDNS integration is "in future" —
`DiscoveryService`/mDNS is not currently wired into the aggregator, even though
a `DiscoverySource::Mdns` variant and priority already exist for it):

```rust
pub enum DiscoverySource { Static, Dns, Mdns }
// priority_value(): Static = 1, Dns = 2, Mdns = 3 (lower = preferred)

pub struct AggregatedPeer {
    pub node_id: NodeId,
    pub address: SocketAddr,
    pub source: DiscoverySource,
    pub discovered_at: Instant,
    pub priority: u32,
}
```

Deduplication is keyed by `NodeId`. Static entries are inserted first at
priority **1**. DNS SRV records — which carry no native `NodeId` — are given a
deterministic synthetic ID via `Uuid::new_v5(&Uuid::NAMESPACE_DNS,
record.target.as_bytes())`, resolved to a `SocketAddr` via
`tokio::net::lookup_host` (skipped with a warning on resolution failure), and
assigned priority `200 + record.priority` (`DiscoverySource::Dns.priority_value()
* 100 + record.priority`) — "blending SRV priority into our ordering (scale by
100 to leave room above the static range of 1-99)" per the source comment. On a
`NodeId` collision the entry with the **lower** priority value wins, so a
`Static` entry (priority 1) always beats any `Dns` entry (priority ≥ 200).
`collect_peers()` returns the deduplicated set sorted ascending by priority;
`best_peer()` returns the first (lowest-priority) entry.

### 4.5 Service-mesh catalog — `service_discovery.rs`

This is a distinct concern from the four peer-discovery backends above: it is a
service (not node) registry, used by `loadbalancer.rs` (see
[Section 6](#6-multi-region-multi-tenancy--load-balancing)):

```rust
pub struct ServiceEndpoint { pub address: SocketAddr, pub protocol: String, pub tls: bool, pub weight: u32 }
pub struct ServiceRegistration {
    pub id: String,        // "{name}:{uuid_v4}"
    pub name: String,
    pub version: String,
    pub node_id: NodeId,
    pub endpoints: Vec<ServiceEndpoint>,
    pub tags: HashSet<String>,
    pub metadata: HashMap<String, String>,
    pub health: ServiceHealth,   // Healthy | Degraded | Unhealthy | Maintenance
    pub registered_at: SystemTime,
    pub last_health_check: SystemTime,
    pub ttl: Duration,           // default 60 s
}
```

`ServiceDiscovery` maintains name/tag/node secondary indexes over a primary
`HashMap<String, ServiceRegistration>`, broadcasts `ServiceEvent`s
(`Registered`, `Deregistered`, `HealthChanged`, `EndpointsChanged`) over a
`tokio::sync::broadcast` channel (capacity 1000), and exposes `cleanup_expired()`
to drop registrations whose `last_health_check` has exceeded their `ttl`.

---

## 5. Partition Detection & Recovery

Sources: `partition.rs` ("Network Partition Tolerance and Split-Brain
Detection") and `recovery.rs` ("Failure Recovery and Resilience").

### 5.1 Detection state machine and quorum

```rust
pub enum PartitionState { Normal, Suspected, Partitioned, Recovering }

const PARTITION_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const QUORUM_RATIO: f64 = 0.5;
```

`PartitionDetector::spawn_detection_task` ticks every 10 s and walks this state
machine based on `visible_count` (nodes currently marked reachable) versus
`known_count` (total known nodes) and quorum:

| Transition | Condition |
|---|---|
| `Normal → Suspected` | no quorum **and** `visible_count < known_count` |
| `Suspected → Normal` | quorum regained **and** `visible_count == known_count` (false alarm) |
| `Suspected → Partitioned` | still no quorum — records a `PartitionInfo`, appends it to `partition_history`, emits `PartitionDetected` then `QuorumLost` |
| `Partitioned → Recovering` | quorum regained — emits `RecoveryStarted` |
| `Recovering → Normal` | `visible_count >= known_count` — emits `RecoveryCompleted` then `PartitionResolved`, clears the current partition |

Quorum is computed as **majority-plus-one**, not the textbook `⌊n/2⌋ + 1`:

```rust
let quorum_size = (known_count as f64 * QUORUM_RATIO).ceil() as usize + 1;
let has_quorum = visible_count >= quorum_size;
```

`PartitionDetector::has_quorum()` special-cases the degenerate single-node/empty
cluster (`known.len() <= 1`) as always quorate. `QuorumDecision` provides the
same formula for standalone majority-vote scenarios (`vote(node_id, bool)` →
`has_quorum()` → `get_decision()`, ties resolve to `None`).

```rust
pub struct PartitionInfo {
    pub partition_id: u64,             // millis since UNIX_EPOCH
    pub visible_nodes: HashSet<NodeId>,
    pub known_nodes: usize,
    pub has_quorum: bool,
    pub leader: Option<NodeId>,
    pub detected_at: SystemTime,
    pub cause: PartitionCause,          // Unknown | NetworkFailure | NodeCrash | ConfigurationError | HighLatency
}
```

`cause` is never auto-inferred anywhere in `partition.rs` — every `PartitionInfo`
constructed by the detection loop uses `PartitionCause::default()`
(`Unknown`); a caller wanting a specific cause must set it manually.

### 5.2 Partition events and split-brain — as actually implemented

```rust
pub enum PartitionEvent {
    PartitionDetected(PartitionInfo),
    QuorumLost { visible: usize, required: usize },
    QuorumRegained { visible: usize, total: usize },       // defined, never emitted in this file
    SplitBrainDetected { partitions: Vec<PartitionInfo> }, // defined, never constructed/emitted in this file
    RecoveryStarted { partition_id: u64 },
    RecoveryCompleted { duration: Duration },
    NodeRejoined { node_id: NodeId },
    PartitionResolved { duration: Duration },
}
```

**Implementation note — "cascading splits" are not modeled.** The
`Vec<PartitionInfo>` shape of `SplitBrainDetected` implies the design intends to
represent multiple simultaneous, possibly nested partition views, but no code
path in `partition.rs` constructs or emits that variant (nor `QuorumRegained`).
The detector as implemented tracks exactly **one** current partition view
(`current_partition: Arc<RwLock<Option<PartitionInfo>>>`) — a global "am I in
the majority partition or not" signal, not a multi-partition topology map.

Handlers are registered with `on_event(handler)` (a `Vec<Box<dyn
Fn(PartitionEvent)+Send+Sync>>`, invoked synchronously in registration order —
the same pattern used by `recovery.rs`'s event types).

`check_rejoined_nodes` (which drives `NodeRejoined` events) computes "rejoined"
nodes as those that are simultaneously visible *and* already present in
`known_nodes` — i.e. essentially every currently-visible known node — rather
than nodes that specifically transitioned from absent to present since the last
check; as written it fires `NodeRejoined` for the same set of nodes on every
tick they remain visible, not just on the transition edge.

### 5.3 Consistent hashing for rebalancing

```rust
pub struct ConsistentHashRing { ring: Vec<(u64, NodeId)>, virtual_nodes: usize }
const DEFAULT_VIRTUAL_NODES: usize = 150;
```

Each physical node gets 150 virtual points on the ring, hashed as
`"{node_id}:{i}"` via `std::collections::hash_map::DefaultHasher` (SipHash with
std's default fixed keys — deterministic within a running process/build, but
the algorithm itself is unspecified by std and not guaranteed stable across
Rust toolchain versions, so ring assignments should not be persisted or compared
across differently-built binaries). `get_node(key)` binary-searches for the
first ring point at or after `key`'s hash, wrapping to `ring[0]` past the end;
`get_nodes(key, count)` walks forward from that point, deduplicating by node, to
produce a replica set.

**Implementation note:** `get_affected_keys_on_join(new_node, keys)` is
implemented as `keys.filter(|k| self.get_node(k).is_some())` — since
`get_node()` returns `Some` for *any* key whenever the ring is non-empty, this
call returns essentially the entire input `keys` list whenever the ring has at
least one node, rather than the specific subset of keys that migrate ownership
to `new_node`. Callers relying on this to compute a minimal rebalance set should
verify behavior before depending on it.

### 5.4 Reconnection backoff and graceful degradation — `recovery.rs`

```rust
pub enum BackoffStrategy { Fixed, Exponential, Linear }

pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
    pub strategy: BackoffStrategy,
    pub jitter: bool,
}
```

| Preset | max_retries | initial_delay | max_delay | multiplier | strategy | jitter |
|---|---|---|---|---|---|---|
| `Default` | 5 | 100 ms | 30 s | 2.0 | Exponential | true |
| `aggressive()` | 10 | 50 ms | 5 s | 1.5 | Exponential | true |
| `conservative()` | 3 | 1 s | 60 s | 3.0 | Exponential | true |

`delay_for_attempt(attempt)` computes `Fixed → initial_delay`,
`Exponential → initial_delay * multiplier.powi(attempt)`,
`Linear → initial_delay + initial_delay * attempt`, caps at `max_delay`, and
(if `jitter` is set) adds up to 25% extra delay derived from
`attempt.wrapping_mul(0x517cc1b727220a95) % (capped_delay_ms / 4)` — a
deterministic, attempt-keyed pseudo-jitter rather than a call into an RNG.

`ConnectionRecovery` tracks one `ConnectionRecoveryState` per reconnecting node
and emits `RecoveryEvent`s (`RecoveryStarted`, `RetryAttempt`,
`RecoverySucceeded`, `RecoveryFailed`, `DegradedModeEntered/Exited`,
`ReconciliationStarted/Completed`). `record_failure()` returns
`RecoveryError::MaxRetriesExceeded(attempt)` once `attempt >= max_retries`.

```rust
pub struct DegradationThresholds { pub min_visible_ratio: f64, pub max_failures: u32, pub max_latency_ms: u64 }
// Default: min_visible_ratio 0.3, max_failures 10, max_latency_ms 5000
```

`DegradationManager::evaluate(visible_ratio, failure_count, avg_latency_ms)`
triggers degraded mode when *any* of `visible_ratio < min_visible_ratio`,
`failure_count > max_failures`, or `avg_latency_ms > max_latency_ms` holds
(checked in that priority order for the emitted reason string).
`enter_degraded_mode`/`exit_degraded_mode` are idempotent.

**Implementation note — `StateReconciler` is bookkeeping only.** Its
`start_reconciliation(peer_count)` / `complete_reconciliation(duration)` /
`time_since_last()` API emits `ReconciliationStarted`/`ReconciliationCompleted`
events and timestamps, but contains no actual state-merge, CRDT, or
vector-clock conflict-resolution logic — that work is left entirely to the
caller. The real, working anti-entropy data exchange in this crate is the
`SyncRequest`/`SyncResponse` pair implemented in `gossip.rs`
(see [Section 3.3](#33-gossip-messages-and-anti-entropy)).

`RetryExecutor::execute`/`execute_with_timeout` wrap an arbitrary async
operation in the configured backoff loop, mapping a timeout to
`RecoveryError::Timeout`.

---

## 6. Multi-Region, Multi-Tenancy & Load Balancing

These three modules sit above `service_discovery.rs`'s `ServiceRegistration`
catalog and are the actual home of "geographic and performance-aware" routing
promised at the crate level.

### 6.1 Multi-region — `multiregion.rs`

```rust
pub struct GeoLocation { pub latitude: f64, pub longitude: f64 }
```

`GeoLocation::distance_to()` implements the **Haversine formula**
(`EARTH_RADIUS_KM = 6371.0`):

```rust
let a = (delta_lat/2.0).sin().powi(2)
      + lat1.cos() * lat2.cos() * (delta_lon/2.0).sin().powi(2);
let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
EARTH_RADIUS_KM * c
```

```rust
pub struct RegionInfo {
    pub id: RegionId, pub name: String, pub location: GeoLocation,
    pub health: RegionHealth,        // Healthy(default) | Degraded | Unhealthy | Maintenance
    pub nodes: HashSet<NodeId>,
    pub capacity: usize,             // default 10 000
    pub current_load: usize,         // default 0
    pub latencies: HashMap<RegionId, Duration>,
    pub last_health_check: SystemTime,
    pub metadata: HashMap<String, String>,
}
```

`RegionTopology::find_closest_region(from)` picks the available region with the
lowest known latency (`latency_matrix`), falling back to a **distance-derived
latency estimate** when no measurement exists:

```rust
let distance_km = from_region.location.distance_to(&r.location);
// Estimate latency: ~1ms per 100km   <-- source comment
Duration::from_millis((distance_km * 10.0) as u64)
```

**Implementation note:** the code computes `distance_km * 10.0` milliseconds,
which is **~1 second per 100 km** (10 ms per km), not the "~1ms per 100km"
claimed by the adjacent comment — a roughly 1000x discrepancy between the
comment and the actual formula. Anyone tuning region-affinity behavior around
this estimate should verify against the real formula, not the comment.

`RegionTopology::select_region(preferred_regions)` tries each preferred region
in order (first available one wins), else falls back to the available region
with the lowest `load_percentage()` (`current_load/capacity * 100`, `100.0` if
`capacity == 0`).

```rust
pub struct ReplicationPolicy {
    pub min_replicas: usize,             // default 2
    pub preferred_regions: Vec<RegionId>,// default []
    pub consistency: ConsistencyLevel,   // Eventual(default) | Quorum | Strong
    pub max_delay: Duration,             // default 60 s
}
```

`ReplicationManager::replicate_agent()` fills replicas from
`preferred_regions` first (excluding the primary, in order), then falls back to
the remaining healthy regions sorted by latency-to-primary (`unwrap_or(999s)`
if unmeasured) — but only **warns**, rather than failing, if the result has
fewer than `min_replicas` regions. `sync_replicas(agent_id)` only bumps
`last_sync` — it performs no actual data synchronization; the doc comment
frames it as a hook for a real sync mechanism.

`FailoverCoordinator::initiate_failover(agent_id, failed_region)` selects the
least-loaded still-available region among the agent's existing
`replica_regions` (excluding the failed one) as the new primary, recording the
`(from, to)` pair in `active_failovers` until `complete_failover()` clears it.

**Implementation note:** the module doc advertises "Region topology with
hierarchical zones", but no zone-of-zones/hierarchy type exists in this file —
only the flat `RegionId`/`RegionInfo` model described above.

### 6.2 Multi-tenancy — `multitenancy.rs`

```rust
pub struct ResourceQuota {
    pub max_agents: usize,
    pub max_namespaces: usize,
    pub max_connections_per_agent: usize,
    pub max_bandwidth_bytes_per_sec: u64,
    pub max_storage_bytes: u64,
    pub max_cpu_ms_per_sec: u64,
}
```

| Preset | agents | namespaces | conns/agent | bandwidth | storage | CPU ms/s |
|---|---|---|---|---|---|---|
| `Default` | 1 000 | 10 | 100 | 100 MB/s | 10 GB | 500 (50% of a core) |
| `small()` | 100 | 3 | 50 | 10 MB/s | 1 GB | 100 (10% of a core) |
| `enterprise()` | 10 000 | 100 | 500 | 1 GB/s | 100 GB | 4 000 (4 cores) |
| `unlimited()` | `usize::MAX` | `usize::MAX` | `usize::MAX` | `u64::MAX` | `u64::MAX` | `u64::MAX` |

`TenantManager` enforces quotas at `create_namespace` (against
`max_namespaces`) and `register_agent` (against `max_agents`, plus a
namespace-ownership check), requiring the tenant to be `TenantStatus::Active`
first; every mutating call writes an `AuditEntry` to `AuditLog` (default
capacity 10 000 entries, 30-day retention, FIFO eviction of the oldest entry
past capacity).

Tenant-to-tenant routing is governed by `RoutingPolicy` (default `Strict`):

```rust
pub enum RoutingPolicy { Strict, PermissionBased, NamespaceOnly }
```

- **`Strict`** — allowed only if source and target share both tenant *and*
  namespace.
- **`NamespaceOnly`** — requires the same tenant, then the same namespace.
- **`PermissionBased`** — same-tenant traffic always allowed; cross-tenant
  traffic requires a matching, non-expired `CrossTenantPermission { source_tenant,
  target_tenant, allowed_namespaces, expires_at }` whose `allowed_namespaces`
  is either empty (all namespaces) or contains the target namespace.

Every denial is logged to the audit trail with an `AuditOutcome::Denied(reason)`.

### 6.3 Load balancing — `loadbalancer.rs`

```rust
pub enum LoadBalancingAlgorithm {
    RoundRobin,          // default
    LeastConnections,
    WeightedRoundRobin,
    Random,
    LeastResponseTime,
}

pub struct HealthCheckConfig {
    pub interval: Duration,          // default 10 s
    pub timeout: Duration,           // default 5 s
    pub unhealthy_threshold: usize,  // default 3
    pub healthy_threshold: usize,    // default 2
}
```

`ServicePool::select_endpoint()` filters to endpoints reporting healthy; **if
none are currently healthy it falls back to `endpoints[0]` regardless of
health** (logged via `warn!`) rather than failing the request outright, then
dispatches to one of five selectors:

- **`round_robin_select`** — `rr_counter.fetch_add(1) % len`.
- **`least_connections_select`** — `min_by_key(active_connections)`.
- **`weighted_round_robin_select`** — walks endpoints subtracting `weight`
  from a running counter (`rr_counter.fetch_add(1) % total_weight`) until it
  goes negative; falls back to plain round-robin if total weight is 0.
- **`random_select`** — `rand::rng().random_range(0..len)`.
- **`least_response_time_select`** — `min_by_key(avg_response_time_ms)`.

`EndpointStats::update_response_time()` maintains an **exponential moving
average** with a 0.9/0.1 old/new weighting: `new_avg = (current_avg*9 +
sample)/10` (the very first sample is stored as-is).

**Implementation note:** `mark_unhealthy()`/`mark_healthy()` track
`consecutive_failures`/`consecutive_successes` against the configured
`unhealthy_threshold`(3)/`healthy_threshold`(2) and log when the threshold is
crossed, but the source comment admits the `EndpointStats.health` field is
**never actually mutated** by these calls ("would need mutable access in real
impl") — so `is_healthy()`-based endpoint selection does not yet respond to the
consecutive-failure/-success bookkeeping described above.

`LoadBalancer::execute_request()` wraps a single call end-to-end: increments
the endpoint's active-connection count, times the call, decrements the count,
records the response time into the EMA, and on error calls
`record_failure()`/wraps the error as `LoadBalancerError::NetworkError`.

---

## 7. Membership History & Observability

### 7.1 Membership event log

`GossipState` maintains a bounded ring buffer of every membership change:

```rust
pub enum MembershipEventKind {
    Joined,
    Left,
    Failed,
    Recovered,
    StatusChanged { from: HealthStatus, to: HealthStatus },
    IncarnationUpdated { old: u64, new: u64 },
}

pub struct MembershipEvent {
    pub node_id: NodeId,
    pub kind: MembershipEventKind,
    pub incarnation: u64,
    pub timestamp: SystemTime,
    pub metadata: Option<String>,
}
```

Query surface:

```rust
pub async fn membership_history(&self) -> Vec<MembershipEvent>              // full snapshot
pub async fn history_for(&self, node_id: &NodeId) -> Vec<MembershipEvent>   // filtered by node
pub async fn history_since(&self, since: SystemTime) -> Vec<MembershipEvent>// filtered by time
pub fn history_capacity(&self) -> usize                                     // = config.max_history
pub async fn history_count(&self) -> usize                                  // current length
```

Every `add_member`, `remove_member`, `update_member_status`, and inbound
`Heartbeat`/`MemberUpdate` handler that changes state calls
`record_membership_event()`, which **spawns a separate `tokio` task** to append
the event (evicting the oldest entry with `pop_front()` once at
`config.max_history` capacity). Because this append happens on a spawned task
rather than inline, callers that need to observe a just-recorded event
immediately after a mutating call (as `gossip_tests.rs` does throughout its
"Feature 2" test block) must `tokio::task::yield_now().await` first to let the
append complete — the history is eventually consistent with respect to the
call that triggered it, not immediately consistent.

### 7.2 Metrics — `metrics/`

`metrics/types.rs` defines the metric primitives (`Counter`, `Gauge`,
`Histogram`/`HistogramStats`) and the domain-specific aggregates re-exported
from `lib.rs`, all consumed through a single `MetricsRegistry`:

```rust
pub struct NodeMetrics {
    pub node_id: NodeId,
    pub messages_sent: Counter, pub messages_received: Counter,
    pub bytes_sent: Counter, pub bytes_received: Counter,
    pub active_connections: Gauge,
    pub message_latency: Histogram, pub message_size: Histogram,
    pub failures: Counter,
    pub last_activity: RwLock<SystemTime>,
}

pub struct GossipMetrics {
    pub heartbeats_sent: Counter, pub heartbeats_received: Counter,
    pub membership_updates: Counter, pub state_syncs: Counter,
    pub failed_rounds: Counter,
    pub member_count: Gauge, pub suspect_count: Gauge, pub dead_count: Gauge,
    pub round_duration: Histogram,
}

pub struct DhtMetrics {
    pub gets: Counter, pub puts: Counter, pub lookups: Counter,
    pub lookup_successes: Counter,
    pub cache_hits: Counter, pub cache_misses: Counter,
    pub routing_table_size: Gauge,
    pub lookup_latency: Histogram,
}

pub struct PeerConnectionMetrics {
    pub connection_attempts: Counter, pub connections_established: Counter,
    pub connection_failures: Counter, pub disconnections: Counter,
    pub reconnect_attempts: Counter, pub reconnections_succeeded: Counter,
    pub connected_peers: Gauge, pub connecting_peers: Gauge,
    pub connection_duration: Histogram, pub connection_time: Histogram,
}
```

Each `*Metrics` struct has a corresponding `*Summary` snapshot type
(`GossipMetricsSummary`, `DhtMetricsSummary`, `PeerConnectionSummary`,
`NodeMetricsSummary`, `ThroughputSummary`, `MigrationSuccessSummary`,
`OperationLatencySummary`), rolled up together in `MetricsSummary { uptime_secs,
local, peers, gossip, dht, message_rate }`.

### 7.3 Exporters — `export.rs`

```rust
pub struct PrometheusExporter { /* prefix, include_help, include_type */ }
impl PrometheusExporter {
    pub fn new(prefix: impl Into<String>) -> Self;
    // .with_help(bool), .with_type_annotations(bool) builder methods
    pub fn export(&self, summary: &MetricsSummary) -> String;
}

pub struct JsonExporter { /* include_timestamps, pretty_print */ }
```

`PrometheusExporter` formats node, gossip, and DHT metric blocks into the
Prometheus text exposition format (with optional `# HELP`/`# TYPE` lines);
`JsonExporter` formats the same `MetricsSummary` to JSON, with an optional
pretty-printed layout and optional embedded timestamp. `MigrationMetricsExport`
provides a flattened summary (total/completed/failed/in-progress migrations,
average duration, bytes transferred, success rate) intended for scraping
alongside the networking metrics above.

### 7.4 Distributed tracing — `tracing.rs`

The module doc frames this as "comprehensive distributed tracing" that is "W3C
Trace Context compatible" and "OpenTelemetry-compatible":

```rust
pub struct TraceId(pub [u8; 16]);
pub struct SpanId(pub [u8; 8]);
pub struct TraceFlags(pub u8);
pub struct TraceContext { /* traceparent-style fields */ }
```

General-purpose `Span`/`SpanBuilder`/`SpanEvent`/`SpanLink`/`SpanKind`/
`SpanStatus` types back two specialized trace shapes relevant to networking:

```rust
pub struct GossipTrace {
    pub root_span: Span,
    pub message_type: String,
    pub origin: NodeId,
    pub hops: Vec<GossipHop>,
    pub nodes_reached: Vec<NodeId>,
}

pub struct GossipHop {
    pub span: Span,
    pub from: NodeId,
    pub to: NodeId,
    pub hop_number: u32,
    pub message_size: u64,
}
```

(and `MigrationTrace`/`MigrationPhaseSpan`/`DataTransferSpan` for agent
migration, outside this document's scope). `TraceCollector` (configured via
`TraceCollectorConfig`, exposing `TraceCollectorStats`) buffers `CollectedTrace`
records; `export_trace_json`/`export_trace_otel` serialize a trace to plain
JSON or to an `OTelSpan`-shaped OpenTelemetry-compatible structure,
respectively.

---

## 8. Tuning Pointers

The knobs most relevant to mesh network behavior, all covered above, are:

- **`GossipConfig`** — `gossip_interval`, `heartbeat_timeout`,
  `failure_timeout`, `fanout`, `max_history` (Section 3.1).
- **`HierarchicalGossipConfig`** — `num_zones`, `super_peers_per_zone`,
  `intra_zone_interval`, `inter_zone_interval`, `max_inter_zone_ttl`, `fanout`
  (Section 3.4).
- **`LookupConfig`** — `parallelism`, `result_count`, `max_iterations`,
  `use_cache`, `query_timeout`, plus the `fast()`/`thorough()` presets and the
  DHT's churn-adaptive cache TTL (Section 2.3).
- **`Dht::with_cache_size`** — lookup-cache capacity independent of the peer
  count.
- **`StaticPeerList`** — `refresh_interval`, `max_failures_before_remove`
  (Section 4.2).
- **`DnsSrvConfig`** — `ttl`, `max_records` (Section 4.3).
- **`RetryConfig`** — `default()` / `aggressive()` / `conservative()` presets
  for reconnection backoff (Section 5.4).
- **`DegradationThresholds`** — `min_visible_ratio`, `max_failures`,
  `max_latency_ms` (Section 5.4).
- **`HealthCheckConfig`** and `LoadBalancingAlgorithm` selection for service
  pools (Section 6.3).
- **`ResourceQuota`** presets (`small()`/`default`/`enterprise()`/`unlimited()`)
  for tenant capacity planning (Section 6.2).

For workload-specific sizing guidance, benchmarking methodology, and profiling
instructions that go beyond documenting these defaults, see
[Performance Tuning](./PERFORMANCE_TUNING.md).

---

## Related Documentation

- [Architecture Guide](./ARCHITECTURE.md) — full MielinOS 5-layer system architecture
- [Wire Protocol](./PROTOCOL.md) — `mielin-mesh-wire` message framing and transport
- [Certificate Management](./CERTIFICATES.md) — mTLS, certificate rotation, and pinning for mesh transport
- [Troubleshooting](./TROUBLESHOOTING.md) — diagnosing connectivity, gossip, and partition issues
- [Performance Tuning](./PERFORMANCE_TUNING.md) — benchmarking and tuning methodology
