//! Agent execution policies
//!
//! This module provides policy-based decision making for agent migration,
//! including load-based, thermal, network latency, and cost-based triggers.

use serde::{Deserialize, Serialize};

/// Basic agent execution policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub min_battery_percent: u8,
    pub max_latency_ms: u32,
    pub preferred_architectures: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            min_battery_percent: 20,
            max_latency_ms: 100,
            preferred_architectures: vec![],
        }
    }
}

impl Policy {
    pub fn allows_low_battery(&self) -> bool {
        self.min_battery_percent == 0
    }

    /// Validate policy constraints
    pub fn validate(&self) -> Result<(), String> {
        if self.min_battery_percent > 100 {
            return Err(format!(
                "Invalid min_battery_percent: {} (must be 0-100)",
                self.min_battery_percent
            ));
        }
        if self.max_latency_ms == 0 {
            return Err("Invalid max_latency_ms: 0 (must be > 0)".to_string());
        }
        Ok(())
    }
}

// ============================================================================
// Migration Policy System
// ============================================================================

/// Metrics about the current node's state for policy decisions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeMetrics {
    /// CPU load percentage (0-100)
    pub cpu_load_percent: u8,
    /// Memory usage percentage (0-100)
    pub memory_usage_percent: u8,
    /// Available memory in bytes
    pub available_memory_bytes: u64,
    /// Temperature in Celsius
    pub temperature_celsius: f32,
    /// Network latency to mesh in milliseconds
    pub network_latency_ms: u32,
    /// Available network bandwidth in bytes/sec
    pub available_bandwidth_bps: u64,
    /// Estimated cost per compute unit
    pub cost_per_unit: f64,
    /// Number of agents currently running on this node
    pub agent_count: u32,
    /// Queue depth (pending tasks)
    pub queue_depth: u32,
    /// Battery percentage (for mobile devices)
    pub battery_percent: Option<u8>,
    /// Whether the node is on AC power
    pub on_ac_power: bool,
}

impl NodeMetrics {
    /// Create a new NodeMetrics with default (healthy) values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the node is under high load
    pub fn is_high_load(&self) -> bool {
        self.cpu_load_percent > 80 || self.memory_usage_percent > 85
    }

    /// Check if the node is overheating
    pub fn is_overheating(&self, threshold: f32) -> bool {
        self.temperature_celsius > threshold
    }

    /// Check if network conditions are poor
    pub fn has_poor_network(&self, max_latency: u32) -> bool {
        self.network_latency_ms > max_latency
    }

    /// Validate node metrics constraints
    pub fn validate(&self) -> Result<(), String> {
        if self.cpu_load_percent > 100 {
            return Err(format!(
                "Invalid cpu_load_percent: {} (must be 0-100)",
                self.cpu_load_percent
            ));
        }
        if self.memory_usage_percent > 100 {
            return Err(format!(
                "Invalid memory_usage_percent: {} (must be 0-100)",
                self.memory_usage_percent
            ));
        }
        if self.temperature_celsius < -273.15 {
            return Err(format!(
                "Invalid temperature_celsius: {:.2} (must be >= -273.15°C, absolute zero)",
                self.temperature_celsius
            ));
        }
        if self.temperature_celsius > 200.0 {
            return Err(format!(
                "Invalid temperature_celsius: {:.2} (must be <= 200°C for realistic operation)",
                self.temperature_celsius
            ));
        }
        if let Some(battery) = self.battery_percent {
            if battery > 100 {
                return Err(format!(
                    "Invalid battery_percent: {} (must be 0-100)",
                    battery
                ));
            }
        }
        if self.cost_per_unit < 0.0 {
            return Err(format!(
                "Invalid cost_per_unit: {} (must be >= 0)",
                self.cost_per_unit
            ));
        }
        if !self.cost_per_unit.is_finite() {
            return Err("Invalid cost_per_unit: must be finite (not NaN or infinity)".to_string());
        }
        Ok(())
    }
}

/// Trigger types for migration decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationTrigger {
    /// Triggered by high CPU or memory load
    LoadBased,
    /// Triggered by thermal conditions
    ThermalBased,
    /// Triggered by network latency issues
    LatencyBased,
    /// Triggered by cost optimization
    CostBased,
    /// Triggered by battery concerns
    BatteryBased,
    /// Triggered by explicit user request
    Manual,
    /// No migration needed
    None,
}

/// Result of a migration policy evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationDecision {
    /// Whether migration should happen
    pub should_migrate: bool,
    /// The trigger that caused the decision
    pub trigger: MigrationTrigger,
    /// Priority of the migration (0-100, higher = more urgent)
    pub priority: u8,
    /// Reason for the decision
    pub reason: String,
    /// Suggested target node characteristics
    pub target_requirements: TargetRequirements,
}

impl MigrationDecision {
    /// Create a decision to not migrate
    pub fn no_migration() -> Self {
        Self {
            should_migrate: false,
            trigger: MigrationTrigger::None,
            priority: 0,
            reason: "No migration needed".to_string(),
            target_requirements: TargetRequirements::default(),
        }
    }

    /// Create a decision to migrate with specified trigger
    pub fn migrate(trigger: MigrationTrigger, priority: u8, reason: String) -> Self {
        Self {
            should_migrate: true,
            trigger,
            priority,
            reason,
            target_requirements: TargetRequirements::default(),
        }
    }

    /// Set target requirements
    pub fn with_requirements(mut self, requirements: TargetRequirements) -> Self {
        self.target_requirements = requirements;
        self
    }

    /// Validate migration decision constraints
    pub fn validate(&self) -> Result<(), String> {
        if self.priority > 100 {
            return Err(format!(
                "Invalid priority: {} (must be 0-100)",
                self.priority
            ));
        }
        if self.should_migrate && self.trigger == MigrationTrigger::None {
            return Err("Invalid state: should_migrate is true but trigger is None".to_string());
        }
        if !self.should_migrate && self.trigger != MigrationTrigger::None {
            return Err(format!(
                "Invalid state: should_migrate is false but trigger is {:?}",
                self.trigger
            ));
        }
        if self.reason.is_empty() {
            return Err("Invalid reason: cannot be empty".to_string());
        }
        self.target_requirements.validate()?;
        Ok(())
    }
}

/// Requirements for the target node
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetRequirements {
    /// Maximum CPU load allowed on target
    pub max_cpu_load: Option<u8>,
    /// Maximum memory usage allowed on target
    pub max_memory_usage: Option<u8>,
    /// Minimum available memory required
    pub min_available_memory: Option<u64>,
    /// Maximum temperature allowed
    pub max_temperature: Option<f32>,
    /// Maximum network latency allowed
    pub max_latency: Option<u32>,
    /// Maximum cost per unit allowed
    pub max_cost: Option<f64>,
    /// Required architectures
    pub required_architectures: Vec<String>,
    /// Prefer nodes with AC power
    pub prefer_ac_power: bool,
}

impl TargetRequirements {
    /// Check if a node meets these requirements
    pub fn is_satisfied_by(&self, metrics: &NodeMetrics) -> bool {
        if let Some(max_cpu) = self.max_cpu_load {
            if metrics.cpu_load_percent > max_cpu {
                return false;
            }
        }
        if let Some(max_mem) = self.max_memory_usage {
            if metrics.memory_usage_percent > max_mem {
                return false;
            }
        }
        if let Some(min_mem) = self.min_available_memory {
            if metrics.available_memory_bytes < min_mem {
                return false;
            }
        }
        if let Some(max_temp) = self.max_temperature {
            if metrics.temperature_celsius > max_temp {
                return false;
            }
        }
        if let Some(max_lat) = self.max_latency {
            if metrics.network_latency_ms > max_lat {
                return false;
            }
        }
        if let Some(max_cost) = self.max_cost {
            if metrics.cost_per_unit > max_cost {
                return false;
            }
        }
        if self.prefer_ac_power && !metrics.on_ac_power {
            // This is a preference, not a hard requirement
            // Return true but lower priority elsewhere
        }
        true
    }

    /// Validate target requirements constraints
    pub fn validate(&self) -> Result<(), String> {
        if let Some(max_cpu) = self.max_cpu_load {
            if max_cpu > 100 {
                return Err(format!("Invalid max_cpu_load: {} (must be 0-100)", max_cpu));
            }
        }
        if let Some(max_mem) = self.max_memory_usage {
            if max_mem > 100 {
                return Err(format!(
                    "Invalid max_memory_usage: {} (must be 0-100)",
                    max_mem
                ));
            }
        }
        if let Some(max_temp) = self.max_temperature {
            if max_temp < -273.15 {
                return Err(format!(
                    "Invalid max_temperature: {:.2} (must be >= -273.15°C, absolute zero)",
                    max_temp
                ));
            }
            if max_temp > 200.0 {
                return Err(format!(
                    "Invalid max_temperature: {:.2} (must be <= 200°C for realistic operation)",
                    max_temp
                ));
            }
            if !max_temp.is_finite() {
                return Err(
                    "Invalid max_temperature: must be finite (not NaN or infinity)".to_string(),
                );
            }
        }
        if let Some(max_lat) = self.max_latency {
            if max_lat == 0 {
                return Err("Invalid max_latency: 0 (must be > 0)".to_string());
            }
        }
        if let Some(max_cost) = self.max_cost {
            if max_cost < 0.0 {
                return Err(format!("Invalid max_cost: {} (must be >= 0)", max_cost));
            }
            if !max_cost.is_finite() {
                return Err("Invalid max_cost: must be finite (not NaN or infinity)".to_string());
            }
        }
        for arch in &self.required_architectures {
            if arch.trim().is_empty() {
                return Err(
                    "Invalid required_architectures: architecture name cannot be empty".to_string(),
                );
            }
        }
        Ok(())
    }
}

/// Configuration for load-based migration triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTriggerConfig {
    /// CPU threshold to trigger migration (percent)
    pub cpu_threshold: u8,
    /// Memory threshold to trigger migration (percent)
    pub memory_threshold: u8,
    /// Queue depth threshold
    pub queue_depth_threshold: u32,
    /// How long conditions must persist before triggering (seconds)
    pub sustained_duration_secs: u32,
    /// Whether to enable this trigger
    pub enabled: bool,
}

impl Default for LoadTriggerConfig {
    fn default() -> Self {
        Self {
            cpu_threshold: 85,
            memory_threshold: 90,
            queue_depth_threshold: 100,
            sustained_duration_secs: 30,
            enabled: true,
        }
    }
}

impl LoadTriggerConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.cpu_threshold > 100 {
            return Err(format!(
                "Invalid cpu_threshold: {} (must be 0-100)",
                self.cpu_threshold
            ));
        }
        if self.memory_threshold > 100 {
            return Err(format!(
                "Invalid memory_threshold: {} (must be 0-100)",
                self.memory_threshold
            ));
        }
        if self.cpu_threshold < 50 {
            return Err(format!(
                "cpu_threshold too low: {} (should be >= 50 for reasonable behavior)",
                self.cpu_threshold
            ));
        }
        if self.memory_threshold < 50 {
            return Err(format!(
                "memory_threshold too low: {} (should be >= 50 for reasonable behavior)",
                self.memory_threshold
            ));
        }
        Ok(())
    }
}

/// Configuration for thermal-based migration triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalTriggerConfig {
    /// Warning temperature threshold (Celsius)
    pub warning_threshold: f32,
    /// Critical temperature threshold (Celsius)
    pub critical_threshold: f32,
    /// Whether to enable this trigger
    pub enabled: bool,
}

impl Default for ThermalTriggerConfig {
    fn default() -> Self {
        Self {
            warning_threshold: 70.0,
            critical_threshold: 85.0,
            enabled: true,
        }
    }
}

impl ThermalTriggerConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.warning_threshold < 0.0 || self.warning_threshold > 200.0 {
            return Err(format!(
                "Invalid warning_threshold: {:.1} (must be 0-200°C)",
                self.warning_threshold
            ));
        }
        if self.critical_threshold < 0.0 || self.critical_threshold > 200.0 {
            return Err(format!(
                "Invalid critical_threshold: {:.1} (must be 0-200°C)",
                self.critical_threshold
            ));
        }
        if self.critical_threshold <= self.warning_threshold {
            return Err(format!(
                "critical_threshold ({:.1}°C) must be greater than warning_threshold ({:.1}°C)",
                self.critical_threshold, self.warning_threshold
            ));
        }
        Ok(())
    }
}

/// Configuration for latency-based migration triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyTriggerConfig {
    /// Maximum acceptable latency (ms)
    pub max_latency_ms: u32,
    /// Minimum bandwidth required (bytes/sec)
    pub min_bandwidth_bps: u64,
    /// Whether to enable this trigger
    pub enabled: bool,
}

impl Default for LatencyTriggerConfig {
    fn default() -> Self {
        Self {
            max_latency_ms: 100,
            min_bandwidth_bps: 1_000_000, // 1 MB/s
            enabled: true,
        }
    }
}

impl LatencyTriggerConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.max_latency_ms == 0 {
            return Err("Invalid max_latency_ms: 0 (must be > 0)".to_string());
        }
        if self.max_latency_ms > 60_000 {
            return Err(format!(
                "max_latency_ms too high: {} (should be <= 60000ms)",
                self.max_latency_ms
            ));
        }
        Ok(())
    }
}

/// Configuration for cost-based migration triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTriggerConfig {
    /// Maximum acceptable cost per compute unit
    pub max_cost_per_unit: f64,
    /// Cost reduction threshold to trigger migration (percent)
    pub cost_reduction_threshold: f32,
    /// Whether to enable this trigger
    pub enabled: bool,
}

impl Default for CostTriggerConfig {
    fn default() -> Self {
        Self {
            max_cost_per_unit: 1.0,
            cost_reduction_threshold: 20.0, // Migrate if 20%+ cheaper elsewhere
            enabled: false,                 // Disabled by default
        }
    }
}

impl CostTriggerConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.max_cost_per_unit < 0.0 {
            return Err(format!(
                "Invalid max_cost_per_unit: {} (must be >= 0)",
                self.max_cost_per_unit
            ));
        }
        if self.cost_reduction_threshold < 0.0 || self.cost_reduction_threshold > 100.0 {
            return Err(format!(
                "Invalid cost_reduction_threshold: {} (must be 0-100%)",
                self.cost_reduction_threshold
            ));
        }
        Ok(())
    }
}

/// Configuration for battery-based migration triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryTriggerConfig {
    /// Minimum battery percentage before migration
    pub min_battery_percent: u8,
    /// Whether to prefer nodes on AC power
    pub prefer_ac_power: bool,
    /// Whether to enable this trigger
    pub enabled: bool,
}

impl Default for BatteryTriggerConfig {
    fn default() -> Self {
        Self {
            min_battery_percent: 15,
            prefer_ac_power: true,
            enabled: true,
        }
    }
}

impl BatteryTriggerConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.min_battery_percent > 100 {
            return Err(format!(
                "Invalid min_battery_percent: {} (must be 0-100)",
                self.min_battery_percent
            ));
        }
        Ok(())
    }
}

/// Complete migration policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPolicy {
    /// Load-based trigger configuration
    pub load_trigger: LoadTriggerConfig,
    /// Thermal trigger configuration
    pub thermal_trigger: ThermalTriggerConfig,
    /// Latency trigger configuration
    pub latency_trigger: LatencyTriggerConfig,
    /// Cost trigger configuration
    pub cost_trigger: CostTriggerConfig,
    /// Battery trigger configuration
    pub battery_trigger: BatteryTriggerConfig,
    /// Minimum time between migrations (seconds)
    pub migration_cooldown_secs: u32,
    /// Maximum migrations per hour
    pub max_migrations_per_hour: u32,
}

impl Default for MigrationPolicy {
    fn default() -> Self {
        Self {
            load_trigger: LoadTriggerConfig::default(),
            thermal_trigger: ThermalTriggerConfig::default(),
            latency_trigger: LatencyTriggerConfig::default(),
            cost_trigger: CostTriggerConfig::default(),
            battery_trigger: BatteryTriggerConfig::default(),
            migration_cooldown_secs: 60,
            max_migrations_per_hour: 10,
        }
    }
}

impl MigrationPolicy {
    /// Validate all policy constraints
    pub fn validate(&self) -> Result<(), String> {
        self.load_trigger.validate()?;
        self.thermal_trigger.validate()?;
        self.latency_trigger.validate()?;
        self.cost_trigger.validate()?;
        self.battery_trigger.validate()?;

        if self.migration_cooldown_secs == 0 {
            return Err("Invalid migration_cooldown_secs: 0 (must be > 0)".to_string());
        }
        if self.migration_cooldown_secs > 86400 {
            return Err(format!(
                "migration_cooldown_secs too high: {} (should be <= 86400 seconds / 24 hours)",
                self.migration_cooldown_secs
            ));
        }
        if self.max_migrations_per_hour == 0 {
            return Err("Invalid max_migrations_per_hour: 0 (must be > 0)".to_string());
        }
        if self.max_migrations_per_hour > 1000 {
            return Err(format!(
                "max_migrations_per_hour too high: {} (should be <= 1000)",
                self.max_migrations_per_hour
            ));
        }
        Ok(())
    }

    /// Create a policy optimized for performance
    pub fn performance_optimized() -> Self {
        Self {
            load_trigger: LoadTriggerConfig {
                cpu_threshold: 70,
                memory_threshold: 80,
                queue_depth_threshold: 50,
                sustained_duration_secs: 15,
                enabled: true,
            },
            latency_trigger: LatencyTriggerConfig {
                max_latency_ms: 50,
                min_bandwidth_bps: 10_000_000,
                enabled: true,
            },
            ..Default::default()
        }
    }

    /// Create a policy optimized for cost
    pub fn cost_optimized() -> Self {
        Self {
            cost_trigger: CostTriggerConfig {
                max_cost_per_unit: 0.5,
                cost_reduction_threshold: 10.0,
                enabled: true,
            },
            load_trigger: LoadTriggerConfig {
                cpu_threshold: 95,
                memory_threshold: 95,
                queue_depth_threshold: 200,
                sustained_duration_secs: 60,
                enabled: true,
            },
            ..Default::default()
        }
    }

    /// Create a policy optimized for battery life
    pub fn battery_optimized() -> Self {
        Self {
            battery_trigger: BatteryTriggerConfig {
                min_battery_percent: 30,
                prefer_ac_power: true,
                enabled: true,
            },
            load_trigger: LoadTriggerConfig {
                cpu_threshold: 60, // Lower threshold to save power
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Evaluates migration policies against current metrics
#[derive(Debug, Clone)]
pub struct PolicyEvaluator {
    policy: MigrationPolicy,
    last_migration_time: Option<u64>,
    migrations_this_hour: u32,
    hour_start_time: u64,
}

impl PolicyEvaluator {
    /// Create a new policy evaluator with the given policy
    pub fn new(policy: MigrationPolicy) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            policy,
            last_migration_time: None,
            migrations_this_hour: 0,
            hour_start_time: now,
        }
    }

    /// Get the current policy
    pub fn policy(&self) -> &MigrationPolicy {
        &self.policy
    }

    /// Update the policy
    pub fn set_policy(&mut self, policy: MigrationPolicy) {
        self.policy = policy;
    }

    /// Record that a migration occurred
    pub fn record_migration(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.last_migration_time = Some(now);

        // Reset hourly counter if needed
        if now - self.hour_start_time >= 3600 {
            self.hour_start_time = now;
            self.migrations_this_hour = 0;
        }
        self.migrations_this_hour += 1;
    }

    /// Check if migration is allowed (cooldown and rate limits)
    pub fn migration_allowed(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Check cooldown
        if let Some(last) = self.last_migration_time {
            if now - last < u64::from(self.policy.migration_cooldown_secs) {
                return false;
            }
        }

        // Check hourly rate limit
        if now - self.hour_start_time < 3600
            && self.migrations_this_hour >= self.policy.max_migrations_per_hour
        {
            return false;
        }

        true
    }

    /// Evaluate the current metrics and return a migration decision
    pub fn evaluate(&self, metrics: &NodeMetrics) -> MigrationDecision {
        // Check if migration is allowed at all
        if !self.migration_allowed() {
            return MigrationDecision::no_migration();
        }

        // Evaluate each trigger in priority order
        // 1. Thermal (highest priority for safety)
        if let Some(decision) = self.evaluate_thermal(metrics) {
            return decision;
        }

        // 2. Battery (important for mobile devices)
        if let Some(decision) = self.evaluate_battery(metrics) {
            return decision;
        }

        // 3. Load-based
        if let Some(decision) = self.evaluate_load(metrics) {
            return decision;
        }

        // 4. Latency-based
        if let Some(decision) = self.evaluate_latency(metrics) {
            return decision;
        }

        // 5. Cost-based (lowest priority)
        if let Some(decision) = self.evaluate_cost(metrics) {
            return decision;
        }

        MigrationDecision::no_migration()
    }

    /// Evaluate thermal trigger
    fn evaluate_thermal(&self, metrics: &NodeMetrics) -> Option<MigrationDecision> {
        if !self.policy.thermal_trigger.enabled {
            return None;
        }

        let temp = metrics.temperature_celsius;

        if temp >= self.policy.thermal_trigger.critical_threshold {
            Some(
                MigrationDecision::migrate(
                    MigrationTrigger::ThermalBased,
                    100, // Maximum priority
                    format!("Critical temperature: {:.1}°C", temp),
                )
                .with_requirements(TargetRequirements {
                    max_temperature: Some(self.policy.thermal_trigger.warning_threshold),
                    ..Default::default()
                }),
            )
        } else if temp >= self.policy.thermal_trigger.warning_threshold {
            Some(
                MigrationDecision::migrate(
                    MigrationTrigger::ThermalBased,
                    80,
                    format!("High temperature warning: {:.1}°C", temp),
                )
                .with_requirements(TargetRequirements {
                    max_temperature: Some(self.policy.thermal_trigger.warning_threshold - 10.0),
                    ..Default::default()
                }),
            )
        } else {
            None
        }
    }

    /// Evaluate battery trigger
    fn evaluate_battery(&self, metrics: &NodeMetrics) -> Option<MigrationDecision> {
        if !self.policy.battery_trigger.enabled {
            return None;
        }

        // Skip if on AC power and we prefer AC
        if metrics.on_ac_power && self.policy.battery_trigger.prefer_ac_power {
            return None;
        }

        if let Some(battery) = metrics.battery_percent {
            if battery < self.policy.battery_trigger.min_battery_percent {
                return Some(
                    MigrationDecision::migrate(
                        MigrationTrigger::BatteryBased,
                        90, // High priority
                        format!("Low battery: {}%", battery),
                    )
                    .with_requirements(TargetRequirements {
                        prefer_ac_power: true,
                        ..Default::default()
                    }),
                );
            }
        }

        None
    }

    /// Evaluate load trigger
    fn evaluate_load(&self, metrics: &NodeMetrics) -> Option<MigrationDecision> {
        if !self.policy.load_trigger.enabled {
            return None;
        }

        let load = &self.policy.load_trigger;

        if metrics.cpu_load_percent > load.cpu_threshold {
            return Some(
                MigrationDecision::migrate(
                    MigrationTrigger::LoadBased,
                    70,
                    format!("High CPU load: {}%", metrics.cpu_load_percent),
                )
                .with_requirements(TargetRequirements {
                    max_cpu_load: Some(load.cpu_threshold.saturating_sub(20)),
                    ..Default::default()
                }),
            );
        }

        if metrics.memory_usage_percent > load.memory_threshold {
            return Some(
                MigrationDecision::migrate(
                    MigrationTrigger::LoadBased,
                    75,
                    format!("High memory usage: {}%", metrics.memory_usage_percent),
                )
                .with_requirements(TargetRequirements {
                    max_memory_usage: Some(load.memory_threshold.saturating_sub(20)),
                    ..Default::default()
                }),
            );
        }

        if metrics.queue_depth > load.queue_depth_threshold {
            return Some(
                MigrationDecision::migrate(
                    MigrationTrigger::LoadBased,
                    60,
                    format!("High queue depth: {}", metrics.queue_depth),
                )
                .with_requirements(TargetRequirements {
                    max_cpu_load: Some(50),
                    ..Default::default()
                }),
            );
        }

        None
    }

    /// Evaluate latency trigger
    fn evaluate_latency(&self, metrics: &NodeMetrics) -> Option<MigrationDecision> {
        if !self.policy.latency_trigger.enabled {
            return None;
        }

        let lat = &self.policy.latency_trigger;

        if metrics.network_latency_ms > lat.max_latency_ms {
            return Some(
                MigrationDecision::migrate(
                    MigrationTrigger::LatencyBased,
                    65,
                    format!("High network latency: {}ms", metrics.network_latency_ms),
                )
                .with_requirements(TargetRequirements {
                    max_latency: Some(lat.max_latency_ms / 2),
                    ..Default::default()
                }),
            );
        }

        if metrics.available_bandwidth_bps < lat.min_bandwidth_bps {
            return Some(
                MigrationDecision::migrate(
                    MigrationTrigger::LatencyBased,
                    55,
                    format!("Low bandwidth: {} bytes/s", metrics.available_bandwidth_bps),
                )
                .with_requirements(TargetRequirements {
                    max_latency: Some(lat.max_latency_ms),
                    ..Default::default()
                }),
            );
        }

        None
    }

    /// Evaluate cost trigger
    fn evaluate_cost(&self, metrics: &NodeMetrics) -> Option<MigrationDecision> {
        if !self.policy.cost_trigger.enabled {
            return None;
        }

        let cost = &self.policy.cost_trigger;

        if metrics.cost_per_unit > cost.max_cost_per_unit {
            return Some(
                MigrationDecision::migrate(
                    MigrationTrigger::CostBased,
                    40,
                    format!("High cost: {:.2} per unit", metrics.cost_per_unit),
                )
                .with_requirements(TargetRequirements {
                    max_cost: Some(cost.max_cost_per_unit * 0.8), // 20% buffer
                    ..Default::default()
                }),
            );
        }

        None
    }

    /// Get the number of migrations this hour
    pub fn migrations_this_hour(&self) -> u32 {
        self.migrations_this_hour
    }
}

/// Score a potential target node for migration
pub fn score_target_node(metrics: &NodeMetrics, requirements: &TargetRequirements) -> Option<f64> {
    // Check hard requirements first
    if !requirements.is_satisfied_by(metrics) {
        return None;
    }

    // Calculate a score based on various factors (higher is better)
    let mut score = 100.0;

    // CPU load (prefer lower)
    score -= f64::from(metrics.cpu_load_percent) * 0.5;

    // Memory usage (prefer lower)
    score -= f64::from(metrics.memory_usage_percent) * 0.3;

    // Temperature (prefer lower)
    score -= f64::from(metrics.temperature_celsius) * 0.2;

    // Latency (prefer lower)
    score -= f64::from(metrics.network_latency_ms) * 0.1;

    // Cost (prefer lower)
    score -= metrics.cost_per_unit * 10.0;

    // Bonus for AC power
    if metrics.on_ac_power {
        score += 5.0;
    }

    // Bonus for low agent count
    if metrics.agent_count < 10 {
        score += f64::from(10 - metrics.agent_count);
    }

    Some(score.max(0.0))
}

/// Select the best target from a list of candidates
pub fn select_best_target(
    candidates: &[(usize, NodeMetrics)],
    requirements: &TargetRequirements,
) -> Option<usize> {
    candidates
        .iter()
        .filter_map(|(idx, metrics)| {
            score_target_node(metrics, requirements).map(|score| (*idx, score))
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create healthy node metrics for testing (no triggers should fire)
    fn healthy_metrics() -> NodeMetrics {
        NodeMetrics {
            cpu_load_percent: 30,
            memory_usage_percent: 40,
            available_memory_bytes: 8_000_000_000,
            temperature_celsius: 50.0,
            network_latency_ms: 20,
            available_bandwidth_bps: 100_000_000, // 100 MB/s
            cost_per_unit: 0.5,
            agent_count: 5,
            queue_depth: 10,
            battery_percent: Some(80),
            on_ac_power: true,
        }
    }

    #[test]
    fn test_default_policy() {
        let policy = Policy::default();
        assert_eq!(policy.min_battery_percent, 20);
        assert!(!policy.allows_low_battery());
    }

    #[test]
    fn test_node_metrics_high_load() {
        let metrics = NodeMetrics {
            cpu_load_percent: 85,
            memory_usage_percent: 60,
            ..healthy_metrics()
        };
        assert!(metrics.is_high_load());

        let metrics2 = NodeMetrics {
            cpu_load_percent: 50,
            memory_usage_percent: 90,
            ..healthy_metrics()
        };
        assert!(metrics2.is_high_load());

        let metrics3 = NodeMetrics {
            cpu_load_percent: 50,
            memory_usage_percent: 50,
            ..healthy_metrics()
        };
        assert!(!metrics3.is_high_load());
    }

    #[test]
    fn test_node_metrics_overheating() {
        let metrics = NodeMetrics {
            temperature_celsius: 85.0,
            ..healthy_metrics()
        };
        assert!(metrics.is_overheating(80.0));
        assert!(!metrics.is_overheating(90.0));
    }

    #[test]
    fn test_node_metrics_poor_network() {
        let metrics = NodeMetrics {
            network_latency_ms: 150,
            ..healthy_metrics()
        };
        assert!(metrics.has_poor_network(100));
        assert!(!metrics.has_poor_network(200));
    }

    #[test]
    fn test_migration_decision_no_migration() {
        let decision = MigrationDecision::no_migration();
        assert!(!decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::None);
        assert_eq!(decision.priority, 0);
    }

    #[test]
    fn test_migration_decision_migrate() {
        let decision =
            MigrationDecision::migrate(MigrationTrigger::LoadBased, 70, "High CPU".to_string());
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::LoadBased);
        assert_eq!(decision.priority, 70);
    }

    #[test]
    fn test_target_requirements_satisfied() {
        let requirements = TargetRequirements {
            max_cpu_load: Some(80),
            max_memory_usage: Some(80),
            max_temperature: Some(70.0),
            ..Default::default()
        };

        let good_metrics = NodeMetrics {
            cpu_load_percent: 50,
            memory_usage_percent: 60,
            temperature_celsius: 55.0,
            ..healthy_metrics()
        };
        assert!(requirements.is_satisfied_by(&good_metrics));

        let bad_metrics = NodeMetrics {
            cpu_load_percent: 90,
            memory_usage_percent: 60,
            temperature_celsius: 55.0,
            ..healthy_metrics()
        };
        assert!(!requirements.is_satisfied_by(&bad_metrics));
    }

    #[test]
    fn test_load_trigger_config_default() {
        let config = LoadTriggerConfig::default();
        assert_eq!(config.cpu_threshold, 85);
        assert_eq!(config.memory_threshold, 90);
        assert!(config.enabled);
    }

    #[test]
    fn test_thermal_trigger_config_default() {
        let config = ThermalTriggerConfig::default();
        assert!((config.warning_threshold - 70.0).abs() < 0.01);
        assert!((config.critical_threshold - 85.0).abs() < 0.01);
        assert!(config.enabled);
    }

    #[test]
    fn test_migration_policy_default() {
        let policy = MigrationPolicy::default();
        assert!(policy.load_trigger.enabled);
        assert!(policy.thermal_trigger.enabled);
        assert!(policy.latency_trigger.enabled);
        assert!(!policy.cost_trigger.enabled); // Disabled by default
        assert!(policy.battery_trigger.enabled);
    }

    #[test]
    fn test_migration_policy_performance_optimized() {
        let policy = MigrationPolicy::performance_optimized();
        assert_eq!(policy.load_trigger.cpu_threshold, 70);
        assert_eq!(policy.latency_trigger.max_latency_ms, 50);
    }

    #[test]
    fn test_migration_policy_cost_optimized() {
        let policy = MigrationPolicy::cost_optimized();
        assert!(policy.cost_trigger.enabled);
        assert!((policy.cost_trigger.max_cost_per_unit - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_migration_policy_battery_optimized() {
        let policy = MigrationPolicy::battery_optimized();
        assert_eq!(policy.battery_trigger.min_battery_percent, 30);
        assert!(policy.battery_trigger.prefer_ac_power);
    }

    #[test]
    fn test_policy_evaluator_creation() {
        let policy = MigrationPolicy::default();
        let evaluator = PolicyEvaluator::new(policy);
        assert!(evaluator.migration_allowed());
        assert_eq!(evaluator.migrations_this_hour(), 0);
    }

    #[test]
    fn test_policy_evaluator_record_migration() {
        let policy = MigrationPolicy::default();
        let mut evaluator = PolicyEvaluator::new(policy);

        evaluator.record_migration();
        assert_eq!(evaluator.migrations_this_hour(), 1);

        // After recording, cooldown should be in effect
        assert!(!evaluator.migration_allowed());
    }

    #[test]
    fn test_policy_evaluator_evaluate_thermal() {
        let policy = MigrationPolicy::default();
        let evaluator = PolicyEvaluator::new(policy);

        let hot_metrics = NodeMetrics {
            temperature_celsius: 90.0,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&hot_metrics);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::ThermalBased);
        assert_eq!(decision.priority, 100);

        let warm_metrics = NodeMetrics {
            temperature_celsius: 75.0,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&warm_metrics);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::ThermalBased);
        assert_eq!(decision.priority, 80);

        let cool_metrics = NodeMetrics {
            temperature_celsius: 50.0,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&cool_metrics);
        assert!(!decision.should_migrate);
    }

    #[test]
    fn test_policy_evaluator_evaluate_battery() {
        let policy = MigrationPolicy::default();
        let evaluator = PolicyEvaluator::new(policy);

        let low_battery = NodeMetrics {
            battery_percent: Some(10),
            on_ac_power: false,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&low_battery);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::BatteryBased);

        let good_battery = NodeMetrics {
            battery_percent: Some(50),
            on_ac_power: false,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&good_battery);
        assert!(!decision.should_migrate);

        let ac_power = NodeMetrics {
            battery_percent: Some(10),
            on_ac_power: true,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&ac_power);
        // Should not trigger because on AC power
        assert!(!decision.should_migrate);
    }

    #[test]
    fn test_policy_evaluator_evaluate_load() {
        let policy = MigrationPolicy::default();
        let evaluator = PolicyEvaluator::new(policy);

        let high_cpu = NodeMetrics {
            cpu_load_percent: 90,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&high_cpu);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::LoadBased);

        let high_memory = NodeMetrics {
            memory_usage_percent: 95,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&high_memory);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::LoadBased);

        let normal = NodeMetrics {
            cpu_load_percent: 50,
            memory_usage_percent: 50,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&normal);
        assert!(!decision.should_migrate);
    }

    #[test]
    fn test_policy_evaluator_evaluate_latency() {
        let policy = MigrationPolicy::default();
        let evaluator = PolicyEvaluator::new(policy);

        let high_latency = NodeMetrics {
            network_latency_ms: 200,
            available_bandwidth_bps: 10_000_000,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&high_latency);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::LatencyBased);

        let low_bandwidth = NodeMetrics {
            network_latency_ms: 50,
            available_bandwidth_bps: 100_000,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&low_bandwidth);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::LatencyBased);

        let good_network = NodeMetrics {
            network_latency_ms: 50,
            available_bandwidth_bps: 10_000_000,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&good_network);
        assert!(!decision.should_migrate);
    }

    #[test]
    fn test_policy_evaluator_evaluate_cost() {
        let mut policy = MigrationPolicy::default();
        policy.cost_trigger.enabled = true;
        let evaluator = PolicyEvaluator::new(policy);

        let expensive = NodeMetrics {
            cost_per_unit: 2.0,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&expensive);
        assert!(decision.should_migrate);
        assert_eq!(decision.trigger, MigrationTrigger::CostBased);

        let cheap = NodeMetrics {
            cost_per_unit: 0.5,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&cheap);
        assert!(!decision.should_migrate);
    }

    #[test]
    fn test_score_target_node() {
        let requirements = TargetRequirements {
            max_cpu_load: Some(80),
            ..Default::default()
        };

        let good_node = NodeMetrics {
            cpu_load_percent: 30,
            memory_usage_percent: 40,
            temperature_celsius: 50.0,
            network_latency_ms: 20,
            cost_per_unit: 0.5,
            on_ac_power: true,
            agent_count: 5,
            ..healthy_metrics()
        };

        let score = score_target_node(&good_node, &requirements);
        assert!(score.is_some());
        let score = score.unwrap();
        assert!(score > 50.0);

        let bad_node = NodeMetrics {
            cpu_load_percent: 90, // Exceeds max_cpu_load
            ..healthy_metrics()
        };
        let score = score_target_node(&bad_node, &requirements);
        assert!(score.is_none());
    }

    #[test]
    fn test_select_best_target() {
        let requirements = TargetRequirements {
            max_cpu_load: Some(80),
            ..Default::default()
        };

        let candidates = vec![
            (
                0,
                NodeMetrics {
                    cpu_load_percent: 70,
                    memory_usage_percent: 60,
                    ..healthy_metrics()
                },
            ),
            (
                1,
                NodeMetrics {
                    cpu_load_percent: 30,
                    memory_usage_percent: 40,
                    on_ac_power: true,
                    ..healthy_metrics()
                },
            ),
            (
                2,
                NodeMetrics {
                    cpu_load_percent: 90, // Too high
                    ..healthy_metrics()
                },
            ),
        ];

        let best = select_best_target(&candidates, &requirements);
        assert_eq!(best, Some(1)); // Should select the low-load node with AC power
    }

    #[test]
    fn test_policy_evaluator_update_policy() {
        let policy = MigrationPolicy::default();
        let mut evaluator = PolicyEvaluator::new(policy);

        assert_eq!(evaluator.policy().load_trigger.cpu_threshold, 85);

        let new_policy = MigrationPolicy::performance_optimized();
        evaluator.set_policy(new_policy);

        assert_eq!(evaluator.policy().load_trigger.cpu_threshold, 70);
    }

    #[test]
    fn test_trigger_priority_order() {
        // Test that triggers are evaluated in priority order
        let policy = MigrationPolicy::default();
        let evaluator = PolicyEvaluator::new(policy);

        // Thermal should take priority over load
        let both_triggered = NodeMetrics {
            temperature_celsius: 90.0,
            cpu_load_percent: 95,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&both_triggered);
        assert_eq!(decision.trigger, MigrationTrigger::ThermalBased);

        // Battery should take priority over load
        let battery_and_load = NodeMetrics {
            battery_percent: Some(5),
            on_ac_power: false,
            cpu_load_percent: 95,
            ..healthy_metrics()
        };
        let decision = evaluator.evaluate(&battery_and_load);
        assert_eq!(decision.trigger, MigrationTrigger::BatteryBased);
    }

    // =========================================================================
    // Policy Validation Tests
    // =========================================================================

    #[test]
    fn test_policy_validate_valid() {
        let policy = Policy::default();
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_policy_validate_invalid_battery() {
        let policy = Policy {
            min_battery_percent: 101,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn test_policy_validate_invalid_latency() {
        let policy = Policy {
            max_latency_ms: 0,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn test_load_trigger_validate_valid() {
        let config = LoadTriggerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_load_trigger_validate_cpu_too_high() {
        let config = LoadTriggerConfig {
            cpu_threshold: 101,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_load_trigger_validate_cpu_too_low() {
        let config = LoadTriggerConfig {
            cpu_threshold: 30,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_load_trigger_validate_memory_too_high() {
        let config = LoadTriggerConfig {
            memory_threshold: 101,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_thermal_trigger_validate_valid() {
        let config = ThermalTriggerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_thermal_trigger_validate_warning_out_of_range() {
        let config = ThermalTriggerConfig {
            warning_threshold: 250.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_thermal_trigger_validate_critical_lower_than_warning() {
        let config = ThermalTriggerConfig {
            warning_threshold: 80.0,
            critical_threshold: 75.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_latency_trigger_validate_valid() {
        let config = LatencyTriggerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_latency_trigger_validate_zero_latency() {
        let config = LatencyTriggerConfig {
            max_latency_ms: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_latency_trigger_validate_latency_too_high() {
        let config = LatencyTriggerConfig {
            max_latency_ms: 100_000,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cost_trigger_validate_valid() {
        let config = CostTriggerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cost_trigger_validate_negative_cost() {
        let config = CostTriggerConfig {
            max_cost_per_unit: -1.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cost_trigger_validate_reduction_out_of_range() {
        let config = CostTriggerConfig {
            cost_reduction_threshold: 150.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_battery_trigger_validate_valid() {
        let config = BatteryTriggerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_battery_trigger_validate_invalid_battery() {
        let config = BatteryTriggerConfig {
            min_battery_percent: 101,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_migration_policy_validate_valid() {
        let policy = MigrationPolicy::default();
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_migration_policy_validate_zero_cooldown() {
        let policy = MigrationPolicy {
            migration_cooldown_secs: 0,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn test_migration_policy_validate_cooldown_too_high() {
        let policy = MigrationPolicy {
            migration_cooldown_secs: 100_000,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn test_migration_policy_validate_zero_migrations_per_hour() {
        let policy = MigrationPolicy {
            max_migrations_per_hour: 0,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn test_migration_policy_validate_migrations_too_high() {
        let policy = MigrationPolicy {
            max_migrations_per_hour: 2000,
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn test_migration_policy_validate_nested_invalid() {
        let policy = MigrationPolicy {
            load_trigger: LoadTriggerConfig {
                cpu_threshold: 101,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(policy.validate().is_err());
    }

    // ========================================================================
    // NodeMetrics validation tests
    // ========================================================================

    #[test]
    fn test_node_metrics_validate_valid() {
        let metrics = healthy_metrics();
        assert!(metrics.validate().is_ok());
    }

    #[test]
    fn test_node_metrics_validate_invalid_cpu() {
        let metrics = NodeMetrics {
            cpu_load_percent: 101,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_invalid_memory() {
        let metrics = NodeMetrics {
            memory_usage_percent: 150,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_temperature_too_low() {
        let metrics = NodeMetrics {
            temperature_celsius: -300.0,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_temperature_too_high() {
        let metrics = NodeMetrics {
            temperature_celsius: 250.0,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_invalid_battery() {
        let metrics = NodeMetrics {
            battery_percent: Some(101),
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_negative_cost() {
        let metrics = NodeMetrics {
            cost_per_unit: -1.0,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_nan_cost() {
        let metrics = NodeMetrics {
            cost_per_unit: f64::NAN,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn test_node_metrics_validate_infinity_cost() {
        let metrics = NodeMetrics {
            cost_per_unit: f64::INFINITY,
            ..healthy_metrics()
        };
        assert!(metrics.validate().is_err());
    }

    // ========================================================================
    // MigrationDecision validation tests
    // ========================================================================

    #[test]
    fn test_migration_decision_validate_valid_no_migration() {
        let decision = MigrationDecision::no_migration();
        assert!(decision.validate().is_ok());
    }

    #[test]
    fn test_migration_decision_validate_valid_migration() {
        let decision = MigrationDecision::migrate(
            MigrationTrigger::LoadBased,
            80,
            "High load detected".to_string(),
        );
        assert!(decision.validate().is_ok());
    }

    #[test]
    fn test_migration_decision_validate_invalid_priority() {
        let decision =
            MigrationDecision::migrate(MigrationTrigger::LoadBased, 101, "High load".to_string());
        assert!(decision.validate().is_err());
    }

    #[test]
    fn test_migration_decision_validate_should_migrate_but_none_trigger() {
        let decision = MigrationDecision {
            should_migrate: true,
            trigger: MigrationTrigger::None,
            priority: 50,
            reason: "Invalid state".to_string(),
            target_requirements: TargetRequirements::default(),
        };
        assert!(decision.validate().is_err());
    }

    #[test]
    fn test_migration_decision_validate_should_not_migrate_but_has_trigger() {
        let decision = MigrationDecision {
            should_migrate: false,
            trigger: MigrationTrigger::LoadBased,
            priority: 0,
            reason: "Invalid state".to_string(),
            target_requirements: TargetRequirements::default(),
        };
        assert!(decision.validate().is_err());
    }

    #[test]
    fn test_migration_decision_validate_empty_reason() {
        let decision = MigrationDecision {
            should_migrate: true,
            trigger: MigrationTrigger::LoadBased,
            priority: 50,
            reason: "".to_string(),
            target_requirements: TargetRequirements::default(),
        };
        assert!(decision.validate().is_err());
    }

    // ========================================================================
    // TargetRequirements validation tests
    // ========================================================================

    #[test]
    fn test_target_requirements_validate_valid() {
        let requirements = TargetRequirements::default();
        assert!(requirements.validate().is_ok());
    }

    #[test]
    fn test_target_requirements_validate_invalid_cpu() {
        let requirements = TargetRequirements {
            max_cpu_load: Some(101),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_invalid_memory() {
        let requirements = TargetRequirements {
            max_memory_usage: Some(150),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_temperature_too_low() {
        let requirements = TargetRequirements {
            max_temperature: Some(-300.0),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_temperature_too_high() {
        let requirements = TargetRequirements {
            max_temperature: Some(250.0),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_nan_temperature() {
        let requirements = TargetRequirements {
            max_temperature: Some(f32::NAN),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_zero_latency() {
        let requirements = TargetRequirements {
            max_latency: Some(0),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_negative_cost() {
        let requirements = TargetRequirements {
            max_cost: Some(-1.0),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_nan_cost() {
        let requirements = TargetRequirements {
            max_cost: Some(f64::NAN),
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_empty_architecture() {
        let requirements = TargetRequirements {
            required_architectures: vec!["x86_64".to_string(), "".to_string()],
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }

    #[test]
    fn test_target_requirements_validate_whitespace_architecture() {
        let requirements = TargetRequirements {
            required_architectures: vec!["x86_64".to_string(), "   ".to_string()],
            ..Default::default()
        };
        assert!(requirements.validate().is_err());
    }
}
