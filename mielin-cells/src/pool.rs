//! Agent pooling for efficient agent reuse
//!
//! Provides an agent pool that maintains a collection of pre-initialized agents
//! that can be reused to avoid the cost of repeated creation and destruction.
//!
//! ## Features
//!
//! - **Size limits**: Maximum and minimum pool sizes
//! - **Lazy initialization**: Agents created on-demand
//! - **Automatic cleanup**: Idle agents are recycled
//! - **Statistics**: Pool usage metrics

use crate::{Agent, AgentState, Policy};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Configuration for agent pool
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of agents to keep in pool
    pub min_size: usize,
    /// Maximum number of agents in pool
    pub max_size: usize,
    /// Maximum age for pooled agents (before recreation)
    pub max_age: Duration,
    /// Maximum idle time before agent is removed
    pub max_idle_time: Duration,
    /// Whether to pre-warm the pool on creation
    pub pre_warm: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 2,
            max_size: 10,
            max_age: Duration::from_secs(3600),      // 1 hour
            max_idle_time: Duration::from_secs(300), // 5 minutes
            pre_warm: false,
        }
    }
}

impl PoolConfig {
    /// Create a minimal pool configuration
    pub fn minimal() -> Self {
        Self {
            min_size: 1,
            max_size: 5,
            max_age: Duration::from_secs(1800),
            max_idle_time: Duration::from_secs(180),
            pre_warm: false,
        }
    }

    /// Create a high-performance pool configuration
    pub fn high_performance() -> Self {
        Self {
            min_size: 5,
            max_size: 50,
            max_age: Duration::from_secs(7200),
            max_idle_time: Duration::from_secs(600),
            pre_warm: true,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.min_size > self.max_size {
            return Err("min_size cannot be greater than max_size".to_string());
        }
        if self.max_size == 0 {
            return Err("max_size must be greater than 0".to_string());
        }
        Ok(())
    }
}

/// Pooled agent with metadata
struct PooledAgent {
    agent: Agent,
    created_at: Instant,
    last_used: Instant,
}

impl PooledAgent {
    fn new(agent: Agent) -> Self {
        let now = Instant::now();
        Self {
            agent,
            created_at: now,
            last_used: now,
        }
    }

    fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    fn idle_time(&self) -> Duration {
        self.last_used.elapsed()
    }

    fn reset(&mut self) {
        // Reset agent to created state
        self.agent.set_state(AgentState::Created);
        self.last_used = Instant::now();
    }

    fn is_expired(&self, max_age: Duration) -> bool {
        self.age() > max_age
    }

    fn is_idle_too_long(&self, max_idle: Duration) -> bool {
        self.idle_time() > max_idle
    }
}

/// Statistics about pool usage
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Total agents acquired from pool
    pub total_acquired: u64,
    /// Total agents returned to pool
    pub total_returned: u64,
    /// Total agents created
    pub total_created: u64,
    /// Total agents destroyed
    pub total_destroyed: u64,
    /// Number of times pool was empty when acquire requested
    pub pool_misses: u64,
    /// Current pool size
    pub current_size: usize,
    /// Peak pool size
    pub peak_size: usize,
}

/// Agent pool for efficient reuse
pub struct AgentPool {
    config: PoolConfig,
    pool: Arc<Mutex<VecDeque<PooledAgent>>>,
    stats: Arc<Mutex<PoolStats>>,
    wasm_binary: Vec<u8>,
    default_policy: Policy,
}

impl AgentPool {
    /// Create a new agent pool with default configuration
    pub fn new(wasm_binary: Vec<u8>) -> Self {
        Self::with_config(wasm_binary, PoolConfig::default())
    }

    /// Create a new agent pool with specified configuration
    pub fn with_config(wasm_binary: Vec<u8>, config: PoolConfig) -> Self {
        config.validate().expect("Invalid pool configuration");

        let pool = Arc::new(Mutex::new(VecDeque::with_capacity(config.max_size)));
        let stats = Arc::new(Mutex::new(PoolStats::default()));

        let mut agent_pool = Self {
            config,
            pool,
            stats,
            wasm_binary,
            default_policy: Policy::default(),
        };

        if agent_pool.config.pre_warm {
            agent_pool.warm_up();
        }

        agent_pool
    }

    /// Set default policy for pooled agents
    pub fn set_default_policy(&mut self, policy: Policy) {
        self.default_policy = policy;
    }

    /// Pre-warm the pool by creating minimum number of agents
    fn warm_up(&mut self) {
        let mut pool = self.pool.lock().expect("Lock poisoned: pool");
        let mut stats = self.stats.lock().expect("Lock poisoned: stats");

        while pool.len() < self.config.min_size {
            let agent = self.create_agent();
            pool.push_back(PooledAgent::new(agent));
            stats.total_created += 1;
            stats.current_size = pool.len();
            if pool.len() > stats.peak_size {
                stats.peak_size = pool.len();
            }
        }
    }

    /// Create a new agent
    fn create_agent(&self) -> Agent {
        let mut agent = Agent::new(self.wasm_binary.clone());
        agent.set_policy(self.default_policy.clone());
        agent
    }

    /// Acquire an agent from the pool
    pub fn acquire(&self) -> Agent {
        let mut pool = self.pool.lock().expect("Lock poisoned: pool");
        let mut stats = self.stats.lock().expect("Lock poisoned: stats");

        stats.total_acquired += 1;

        // Try to get an agent from pool
        while let Some(mut pooled) = pool.pop_front() {
            // Check if agent is still usable
            if !pooled.is_expired(self.config.max_age)
                && !pooled.is_idle_too_long(self.config.max_idle_time)
            {
                pooled.reset();
                stats.current_size = pool.len();
                return pooled.agent;
            }

            // Agent expired, destroy it
            stats.total_destroyed += 1;
            stats.current_size = pool.len();
        }

        // Pool is empty, create new agent
        stats.pool_misses += 1;
        stats.total_created += 1;
        self.create_agent()
    }

    /// Return an agent to the pool
    pub fn release(&self, agent: Agent) {
        let mut pool = self.pool.lock().expect("Lock poisoned: pool");
        let mut stats = self.stats.lock().expect("Lock poisoned: stats");

        stats.total_returned += 1;

        // Don't return if pool is at max size
        if pool.len() >= self.config.max_size {
            stats.total_destroyed += 1;
            return;
        }

        // Don't return terminated or error agents
        if matches!(agent.state(), AgentState::Terminated | AgentState::Error) {
            // Agent in bad state, destroy it
            stats.total_destroyed += 1;
            return;
        }

        // Return agent to pool
        pool.push_back(PooledAgent::new(agent));
        stats.current_size = pool.len();
        if pool.len() > stats.peak_size {
            stats.peak_size = pool.len();
        }
    }

    /// Get current pool statistics
    pub fn stats(&self) -> PoolStats {
        self.stats.lock().expect("Lock poisoned: stats").clone()
    }

    /// Get current pool size
    pub fn size(&self) -> usize {
        self.pool.lock().expect("Lock poisoned: pool").len()
    }

    /// Clear the pool
    pub fn clear(&self) {
        let mut pool = self.pool.lock().expect("Lock poisoned: pool");
        let mut stats = self.stats.lock().expect("Lock poisoned: stats");

        let removed = pool.len();
        pool.clear();
        stats.total_destroyed += removed as u64;
        stats.current_size = 0;
    }

    /// Maintenance: Remove expired and idle agents
    pub fn maintain(&self) {
        let mut pool = self.pool.lock().expect("Lock poisoned: pool");
        let mut stats = self.stats.lock().expect("Lock poisoned: stats");

        let mut to_keep = VecDeque::new();
        let mut removed = 0;

        while let Some(pooled) = pool.pop_front() {
            if pooled.is_expired(self.config.max_age)
                || pooled.is_idle_too_long(self.config.max_idle_time)
            {
                removed += 1;
            } else if to_keep.len() < self.config.min_size || to_keep.len() < self.config.max_size {
                to_keep.push_back(pooled);
            } else {
                removed += 1;
            }
        }

        *pool = to_keep;
        stats.total_destroyed += removed;
        stats.current_size = pool.len();
    }

    /// Get pool configuration
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_wasm() -> Vec<u8> {
        vec![0x00, 0x61, 0x73, 0x6d]
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_size, 2);
        assert_eq!(config.max_size, 10);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pool_config_validation() {
        let valid = PoolConfig {
            min_size: 2,
            max_size: 10,
            ..Default::default()
        };
        assert!(valid.validate().is_ok());

        let invalid = PoolConfig {
            min_size: 10,
            max_size: 5,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        let invalid2 = PoolConfig {
            min_size: 0,
            max_size: 0,
            ..Default::default()
        };
        assert!(invalid2.validate().is_err());
    }

    #[test]
    fn test_pool_acquire_release() {
        let pool = AgentPool::new(create_test_wasm());

        let agent = pool.acquire();
        assert_eq!(agent.state(), &AgentState::Created);

        pool.release(agent);
        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn test_pool_reuse() {
        let pool = AgentPool::new(create_test_wasm());

        let agent1 = pool.acquire();
        let id1 = agent1.id();
        pool.release(agent1);

        let agent2 = pool.acquire();
        let id2 = agent2.id();

        // Should be the same agent
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_pool_max_size() {
        let config = PoolConfig {
            min_size: 0,
            max_size: 3,
            ..Default::default()
        };
        let pool = AgentPool::with_config(create_test_wasm(), config);

        // Acquire multiple agents then release them all
        let mut agents = Vec::new();
        for _ in 0..5 {
            agents.push(pool.acquire());
        }

        // Release all agents
        for agent in agents {
            pool.release(agent);
        }

        // Should not exceed max_size
        assert_eq!(pool.size(), 3);
    }

    #[test]
    fn test_pool_pre_warm() {
        let config = PoolConfig {
            min_size: 3,
            pre_warm: true,
            ..Default::default()
        };
        let pool = AgentPool::with_config(create_test_wasm(), config);

        // Should have min_size agents ready
        assert_eq!(pool.size(), 3);
    }

    #[test]
    fn test_pool_stats() {
        let pool = AgentPool::new(create_test_wasm());

        let agent1 = pool.acquire();
        let agent2 = pool.acquire();

        pool.release(agent1);
        pool.release(agent2);

        let stats = pool.stats();
        assert_eq!(stats.total_acquired, 2);
        assert_eq!(stats.total_returned, 2);
        assert_eq!(stats.total_created, 2);
    }

    #[test]
    fn test_pool_maintain() {
        let config = PoolConfig {
            min_size: 0,
            max_size: 10,
            max_idle_time: Duration::from_millis(10),
            ..Default::default()
        };
        let pool = AgentPool::with_config(create_test_wasm(), config);

        // Add some agents
        let mut agents = Vec::new();
        for _ in 0..5 {
            agents.push(pool.acquire());
        }
        for agent in agents {
            pool.release(agent);
        }

        assert_eq!(pool.size(), 5);

        // Wait for idle timeout
        std::thread::sleep(Duration::from_millis(20));

        // Maintenance should remove idle agents
        pool.maintain();
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_pool_clear() {
        let pool = AgentPool::new(create_test_wasm());

        let mut agents = Vec::new();
        for _ in 0..5 {
            agents.push(pool.acquire());
        }
        for agent in agents {
            pool.release(agent);
        }

        assert_eq!(pool.size(), 5);
        pool.clear();
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_pool_default_policy() {
        let mut pool = AgentPool::new(create_test_wasm());

        let custom_policy = Policy {
            min_battery_percent: 50,
            max_latency_ms: 50,
            preferred_architectures: vec!["x86_64".to_string()],
        };
        pool.set_default_policy(custom_policy.clone());

        let agent = pool.acquire();
        assert_eq!(agent.policy().min_battery_percent, 50);
        assert_eq!(agent.policy().max_latency_ms, 50);
    }

    #[test]
    fn test_pool_high_performance_config() {
        let config = PoolConfig::high_performance();
        assert_eq!(config.min_size, 5);
        assert_eq!(config.max_size, 50);
        assert!(config.pre_warm);
    }

    #[test]
    fn test_pool_minimal_config() {
        let config = PoolConfig::minimal();
        assert_eq!(config.min_size, 1);
        assert_eq!(config.max_size, 5);
        assert!(!config.pre_warm);
    }

    #[test]
    fn test_pool_misses() {
        let pool = AgentPool::new(create_test_wasm());

        // First acquire will be a miss (pool is empty)
        let _agent1 = pool.acquire();

        let stats = pool.stats();
        assert_eq!(stats.pool_misses, 1);
    }
}
