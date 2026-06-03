//! Mielin Cells - Agent SDK
//!
//! Provides agent definition, lifecycle management, policy execution,
//! and inter-agent communication.

pub mod agent;
pub mod compliance;
pub mod composition;
pub mod debug;
pub mod discovery;
pub mod dna;
pub mod dr;
pub mod evolution;
pub mod fault;
pub mod group;
pub mod ha;
pub mod history;
pub mod messaging;
pub mod migration;
pub mod multiregion;
pub mod orchestration;
pub mod policy;
pub mod pool;
pub mod resource;
pub mod security;
pub mod template;
pub mod versioning;

pub use agent::{
    Agent, AgentError, AgentId, AgentState, StateTransition, TransitionHook, TransitionResult,
};
pub use compliance::{
    AuditEntry, AuditLogger, AuditQuery, CompliancePolicy, ComplianceReport, DataClassification,
    EventType, PolicyChecker, PolicyViolation, PrivacyConfig, PrivacyControl, PrivacyManager,
    ReportGenerator, ReportType, RetentionPolicy,
};
pub use composition::{
    Behavior, BehaviorComposite, BehaviorContext, BehaviorId, BehaviorRegistry, BehaviorResult,
    ClosureBehavior, HealthCheckBehavior, LoggingBehavior, MetricsBehavior,
};
pub use debug::{
    Breakpoint, DebugCommand, DebugContext, DebugSession, DebuggerConfig, ExecutionTracer,
    InspectionQuery, InspectionResult, MemoryProfiler, RemoteDebugger, StateInspector, TraceEvent,
    TraceFilter, WatchExpression,
};
pub use discovery::{
    Capability, DiscoveryError, DiscoveryQuery, DiscoveryResult, HealthCheck,
    HealthStatus as DiscoveryHealthStatus, LoadBalancer, LoadBalancingStrategy,
    Location as DiscoveryLocation, ServiceRegistration, ServiceRegistry,
};
pub use dna::Dna;
pub use dr::{
    Backup, BackupConfig, BackupManager, BackupSchedule, BackupScheduler, BackupStrategy,
    BackupType, RecoveryConfig, RecoveryManager, RecoveryPlan, RecoveryPoint, RecoveryStrategy,
    RecoveryTarget, VerificationConfig, Verifier,
};
pub use evolution::{
    AgentGenome, CapabilityDiscovery, CapabilityGene, CoordinationStrategy, CrossoverOperator,
    EvolutionConfig, EvolutionEngine, EvolutionError, EvolutionResult, EvolutionStats,
    FitnessEvaluator, FitnessMetrics, MutationOperator, MutationRates, SelectionStrategy,
};
pub use group::{
    AgentGroup, GroupConfig, GroupCoordinator, GroupError, GroupId, GroupMember, GroupRegistry,
    GroupResult, GroupRole, GroupState,
};
pub use ha::{
    ClusterHealth, ClusterHealthMonitor, FailoverConfig, FailoverCoordinator, FailoverDecision,
    FailoverEvent, FailoverPolicy, FailoverState, FailoverStrategy, HealthConfig as HAHealthConfig,
    HealthMetric, HealthStatus as HAHealthStatus, HealthThreshold, LeaderElection, LeaderElector,
    LeaderState, NodeHealth, QuorumConfig, QuorumDecision, QuorumPolicy, QuorumVote, QuorumVoter,
    ReplicationConfig, ReplicationManager, ReplicationState, ReplicationStrategy, StateReplica,
    VoteRequest, VoteResponse,
};
pub use history::{
    AgentHistory, HistoryConfig as HistoryTrackerConfig, HistoryTracker, StateHistoryEntry,
    StateStatistics,
};
pub use messaging::{Mailbox, Message, MessageBus, MessagingError, Priority, Topic};
pub use multiregion::{
    CrossRegionSync, GeoRouter, LatencyMap, Region, RegionConfig, RegionDeployment, RegionManager,
    RegionStatus, RouteDecision, RoutingPolicy, RoutingStrategy, SyncConfig, SyncManager,
    SyncStatus,
};
pub use orchestration::{
    Deployment, DeploymentSpec, DeploymentStatus, DeploymentStrategy, Node, OrchestrationError,
    Orchestrator, PlacementConstraint, ResourceRequirements, ScalingPolicy, Scheduler,
};
pub use policy::Policy;
pub use pool::{AgentPool, PoolConfig, PoolStats};
pub use resource::{
    Anomaly, AnomalyConfig, AnomalyDetector, AnomalyType, HistoryConfig, MonitorSummary,
    QuotaEnforcer, QuotaViolation, ResourceHistory, ResourceManager, ResourceMonitor,
    ResourcePrediction, ResourcePredictor, ResourceQuota, ResourceRate, ResourceSnapshot,
    ResourceUsage,
};
pub use security::{
    AgentIdentity, AttestationValidator, AuthChallenge, AuthResponse, AuthToken, Authenticator,
    Capability as SecurityCapability, CapabilityAttestation, EncryptedSnapshot, EncryptionKey,
    IdentityProvider, PublicIdentity, SandboxConfig, SandboxExecutor, SandboxViolation,
    SecurityContext, StateEncryptor,
};
pub use template::{AgentTemplate, TemplateBuilder, TemplateError, TemplateId, TemplateRegistry};
pub use versioning::{
    ABTestConfig, ABTestDeployment, ABTestStats, CanaryConfig, CanaryDeployment, CanaryStats,
    RollingUpdateConfig, RollingUpdateStrategy, Version, VersionDeployer, VersionError,
    VersionMetadata, VersionRegistry,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CellError {
    #[error("Agent execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Migration failed: {0}")]
    MigrationFailed(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new(vec![0x00, 0x61, 0x73, 0x6d]);
        assert!(agent.id().as_bytes().len() == 16);
    }
}
