# mielin-cells TODO

## Pending Tasks

### Medium Priority
- [x] Fault injection testing — FaultInjector with probability, max-occurrences, delay; 15+ tests in fault_injection_tests.rs
- [x] Cross-version migration tests — multi-hop chains, deprecation rejection, rolling update fault injection; 35 tests in cross_version_migration_tests.rs

### Low Priority
- [ ] Video tutorials for agent development

## Completed Features (v0.1.0-rc.1)

The following major features have been implemented and are production-ready:

### Agent Lifecycle
- ✅ Agent creation and initialization
- ✅ State management (Created, Running, Paused, Migrating, Terminated, Failed)
- ✅ State transitions with validation
- ✅ Error handling and recovery

### Migration
- ✅ Agent serialization/deserialization
- ✅ Migration snapshot capture
- ✅ State compression (LZ4, Zstd)
- ✅ Checksum verification
- ✅ Migration telemetry

### Policy System
- ✅ Resource policies (CPU, memory, network)
- ✅ Security policies (sandboxing, permissions)
- ✅ Placement policies (hardware affinity, geographic constraints)
- ✅ Policy evaluation and enforcement

### Communication
- ✅ Inter-agent messaging
- ✅ Message bus implementation
- ✅ Publish-subscribe pattern
- ✅ Request-response pattern
- ✅ Message queuing

### Agent Organization
- ✅ Agent groups and roles
- ✅ Group lifecycle management
- ✅ Hierarchical agent structures

### Discovery & Versioning
- ✅ Service discovery
- ✅ Health monitoring
- ✅ Load balancing
- ✅ Version management
- ✅ Migration compatibility checking

### Testing & Quality
- ✅ Comprehensive test suite (500+ tests)
- ✅ Examples (agent groups, messaging, state machines)
- ✅ Documentation

For detailed feature descriptions and API documentation, see [README.md](README.md) and the generated rustdoc.
